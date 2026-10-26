//! Fallback chains for the model step (docs/merion/design/model-gateway.md).
//!
//! A pack's `model` step names one endpoint. It may also name a **chain** (`fallback_chain`), a list of
//! further endpoints that only the operator can write, in a file (`ZYVOR_AGENT_MODEL_CHAINS_FILE`). When the
//! first endpoint is down, rate-limited (429) or answers 5xx, the host tries the next. Everything else is
//! unchanged for each endpoint: the vault decides reachability, the first use needs a person, the call is audited.
//!
//! This module is the pure part: the chain file, the circuit breaker and the failover rules. The calls
//! themselves stay in `model_call.rs`.

use crate::demo_rules::ModelSpec;
use serde::Deserialize;
use serde_json::Value;
use std::collections::{BTreeMap, HashMap};
use std::path::Path;
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

/// At most this many fallbacks after the first endpoint.
pub const MAX_FALLBACKS: usize = 4;
/// Consecutive failover-class failures that open an endpoint's breaker.
pub const FAILURE_THRESHOLD: u32 = 3;
/// How long an open breaker skips its endpoint.
pub const OPEN_FOR: Duration = Duration::from_secs(30);

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Target {
    pub credential: String,
    pub base_url: String,
    pub model: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
struct Chain {
    targets: Vec<Target>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ChainsFile {
    chains: BTreeMap<String, Chain>,
}

/// A chain name is a short plain word, so it cannot carry anything else into a log or a path.
pub fn valid_chain_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 40
        && name.bytes().all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_'))
}

impl Target {
    /// The pack's spec with this target's endpoint swapped in. The instruction and caps stay the pack's, and the
    /// result is checked like any other `model` block, so a bad chain entry is refused the same way.
    pub fn applied_to(&self, base: &ModelSpec) -> Result<ModelSpec, String> {
        let spec = ModelSpec {
            credential: self.credential.clone(),
            base_url: self.base_url.clone(),
            model: self.model.clone(),
            fallback_chain: None,
            ..base.clone()
        };
        spec.check()?;
        Ok(spec)
    }
}

/// The targets of chain `name`, read from the operator's file. A missing file, an unknown chain or a bad entry
/// is an error: a pack that names a chain does not quietly run without it.
pub fn targets(file: Option<&Path>, name: &str, base: &ModelSpec) -> Result<Vec<ModelSpec>, String> {
    let path = file.ok_or("this pack names a fallback chain, but the operator configured no chains file")?;
    let text = std::fs::read_to_string(path).map_err(|e| format!("chains file: {e}"))?;
    let parsed: ChainsFile = serde_json::from_str(&text).map_err(|e| format!("chains file: {e}"))?;
    let chain = parsed
        .chains
        .get(name)
        .ok_or_else(|| format!("no fallback chain named '{name}'"))?;
    if chain.targets.is_empty() || chain.targets.len() > MAX_FALLBACKS {
        return Err(format!("chain '{name}' needs 1 to {MAX_FALLBACKS} targets"));
    }
    chain
        .targets
        .iter()
        .map(|t| t.applied_to(base).map_err(|e| format!("chain '{name}': {e}")))
        .collect()
}

/// Whether a response status means "try the next endpoint": rate limited or a server error. A 4xx is the
/// request's fault, and another provider would only be sent the same text for the same answer.
pub fn failover_status(status: u16) -> bool {
    status == 429 || status >= 500
}

/// Prompt and completion tokens from an OpenAI-style `usage` object. Absent means unknown, never zero.
pub fn tokens(body: &[u8]) -> (Option<u64>, Option<u64>) {
    let Ok(v) = serde_json::from_slice::<Value>(body) else {
        return (None, None);
    };
    let n = |k: &str| v.get("usage").and_then(|u| u.get(k)).and_then(Value::as_u64);
    (n("prompt_tokens"), n("completion_tokens"))
}

/// Identifies an endpoint for the breaker: where it is, which model, and which key.
pub fn breaker_key(spec: &ModelSpec) -> String {
    format!("{}|{}|{}", spec.base_url, spec.model, spec.credential)
}

#[derive(Default)]
struct Breaker {
    failures: u32,
    open_until: Option<Instant>,
}

fn breakers() -> &'static Mutex<HashMap<String, Breaker>> {
    static B: OnceLock<Mutex<HashMap<String, Breaker>>> = OnceLock::new();
    B.get_or_init(Default::default)
}

/// True while the endpoint's breaker is open.
pub fn is_open(key: &str, now: Instant) -> bool {
    let map = breakers().lock().unwrap_or_else(|e| e.into_inner());
    map.get(key).and_then(|b| b.open_until).is_some_and(|t| t > now)
}

/// Record a call's outcome. Only failover-class failures count; a success closes the breaker.
pub fn record(key: &str, ok: bool, now: Instant) {
    let mut map = breakers().lock().unwrap_or_else(|e| e.into_inner());
    let b = map.entry(key.to_string()).or_default();
    if ok {
        *b = Breaker::default();
        return;
    }
    b.failures += 1;
    if b.failures >= FAILURE_THRESHOLD {
        b.open_until = Some(now + OPEN_FOR);
    }
}

/// Per endpoint: calls, failures, tokens and latency, from the journal's `model.call` rows. Tokens the provider did
/// not report are not counted, and `tokens_unreported` says how many calls had none, so a total is never mistaken
/// for a complete one. There is no cost here: prices are the operator's to supply and Merion does not guess them.
pub fn summarize(rows: &[crate::audit::AuditEntry]) -> Value {
    use crate::audit::AuditPhase;
    #[derive(Default)]
    struct Row {
        calls: u64,
        failed: u64,
        prompt: u64,
        completion: u64,
        unreported: u64,
        latency_sum: u64,
        latency_n: u64,
    }
    let mut by: BTreeMap<(String, String), Row> = BTreeMap::new();
    let mut failovers = 0u64;
    for r in rows {
        if r.action == "model.failover" {
            failovers += 1;
            continue;
        }
        if r.action != "model.call" || r.phase == AuditPhase::Planned {
            continue;
        }
        let text = |k: &str| r.detail.get(k).and_then(Value::as_str).unwrap_or("?").to_string();
        let e = by.entry((text("host"), text("model"))).or_default();
        e.calls += 1;
        if r.phase == AuditPhase::Failed {
            e.failed += 1;
            continue;
        }
        let n = |k: &str| r.detail.get(k).and_then(Value::as_u64);
        match (n("prompt_tokens"), n("completion_tokens")) {
            (Some(p), Some(c)) => {
                e.prompt += p;
                e.completion += c;
            }
            _ => e.unreported += 1,
        }
        if let Some(ms) = n("latency_ms") {
            e.latency_sum += ms;
            e.latency_n += 1;
        }
    }
    let items: Vec<Value> = by
        .into_iter()
        .map(|((host, model), e)| {
            serde_json::json!({
                "host": host, "model": model, "calls": e.calls, "failed": e.failed,
                "prompt_tokens": e.prompt, "completion_tokens": e.completion, "tokens_unreported": e.unreported,
                "avg_latency_ms": e.latency_sum.checked_div(e.latency_n),
            })
        })
        .collect();
    serde_json::json!({ "items": items, "failovers": failovers })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(action: &str, phase: crate::audit::AuditPhase, detail: Value) -> crate::audit::AuditEntry {
        crate::audit::AuditEntry {
            seq: 0, at: chrono::Utc::now(), session_id: None, phase, action: action.into(), subject: None,
            detail, prev_hash: String::new(), hash: String::new(),
        }
    }

    #[test]
    fn usage_is_summed_per_endpoint_and_never_invents_tokens() {
        use crate::audit::AuditPhase::{Failed, Performed};
        let ok = |p: Option<u64>, c: Option<u64>, ms: u64| {
            row("model.call", Performed, serde_json::json!({"host":"a","model":"m","prompt_tokens":p,"completion_tokens":c,"latency_ms":ms}))
        };
        let rows = vec![
            ok(Some(10), Some(5), 100),
            ok(Some(20), Some(5), 300),
            ok(None, None, 200),
            row("model.call", Failed, serde_json::json!({"host":"a","model":"m"})),
            row("model.call", Performed, serde_json::json!({"host":"b","model":"n"})),
            row("model.failover", Performed, serde_json::json!({})),
            row("changeset.apply", Performed, serde_json::json!({})),
        ];
        let s = summarize(&rows);
        assert_eq!(s["failovers"], 1);
        let a = &s["items"][0];
        assert_eq!((a["host"].as_str(), a["calls"].as_u64(), a["failed"].as_u64()), (Some("a"), Some(4), Some(1)));
        assert_eq!((a["prompt_tokens"].as_u64(), a["completion_tokens"].as_u64()), (Some(30), Some(10)));
        assert_eq!(a["tokens_unreported"], 1, "a call with no usage is counted as unreported, not as zero");
        assert_eq!(a["avg_latency_ms"], 200);
        assert_eq!(s["items"][1]["avg_latency_ms"], Value::Null, "no latency recorded means null, not 0");
        assert_eq!(s["items"].as_array().unwrap().len(), 2);
    }

    fn base() -> ModelSpec {
        ModelSpec {
            credential: "a".into(),
            base_url: "https://a.example/v1".into(),
            model: "m".into(),
            instruction: "Summarise.".into(),
            max_input_chars: None,
            max_output_tokens: None,
            fallback_chain: Some("main".into()),
        }
    }

    fn file(json: &str) -> std::path::PathBuf {
        let p = std::env::temp_dir().join(format!("chains-{}.json", uuid::Uuid::new_v4()));
        std::fs::write(&p, json).unwrap();
        p
    }

    #[test]
    fn a_chain_swaps_in_each_endpoint_and_keeps_the_packs_instruction() {
        let f = file(r#"{"chains":{"main":{"targets":[
            {"credential":"b","base_url":"https://b.example/v1","model":"m2"},
            {"credential":"c","base_url":"http://127.0.0.1:9/v1","model":"m3"}]}}}"#);
        let t = targets(Some(&f), "main", &base()).unwrap();
        assert_eq!(t.len(), 2);
        assert_eq!(t[0].credential, "b");
        assert_eq!(t[1].base_url, "http://127.0.0.1:9/v1");
        assert!(t.iter().all(|s| s.instruction == "Summarise." && s.fallback_chain.is_none()));
    }

    #[test]
    fn every_misconfiguration_is_an_error_not_a_silent_skip() {
        assert!(targets(None, "main", &base()).unwrap_err().contains("no chains file"));
        let missing = std::env::temp_dir().join("no-such-chains.json");
        assert!(targets(Some(&missing), "main", &base()).is_err());
        let f = file(r#"{"chains":{"other":{"targets":[{"credential":"b","base_url":"https://b.example/v1","model":"m"}]}}}"#);
        assert!(targets(Some(&f), "main", &base()).unwrap_err().contains("no fallback chain named"));
        for bad in [
            r#"{"chains":{"main":{"targets":[]}}}"#,
            r#"{"chains":{"main":{"targets":[{"credential":"b","base_url":"http://evil.example/v1","model":"m"}]}}}"#,
            r#"{"chains":{"main":{"targets":[{"credential":"b","base_url":"https://u:p@b.example/v1","model":"m"}]}}}"#,
            r#"{"chains":{"main":{"targets":[{"credential":"b","base_url":"https://b.example/v1","model":"m","key":"sk-1"}]}}}"#,
            r#"{"chains":{"main":{"targets":[{"credential":"b c","base_url":"https://b.example/v1","model":"m"}]}}}"#,
            "not json",
        ] {
            assert!(targets(Some(&file(bad)), "main", &base()).is_err(), "{bad}");
        }
        let six: Vec<String> = (0..=MAX_FALLBACKS)
            .map(|i| format!(r#"{{"credential":"b","base_url":"https://b.example/v1","model":"m{i}"}}"#))
            .collect();
        let f = file(&format!(r#"{{"chains":{{"main":{{"targets":[{}]}}}}}}"#, six.join(",")));
        assert!(targets(Some(&f), "main", &base()).unwrap_err().contains("1 to"));
    }

    #[test]
    fn only_rate_limits_and_server_errors_fail_over() {
        for s in [429, 500, 502, 503, 504] {
            assert!(failover_status(s), "{s}");
        }
        for s in [200, 400, 401, 403, 404, 422] {
            assert!(!failover_status(s), "{s}");
        }
    }

    #[test]
    fn the_breaker_opens_after_repeated_failures_and_closes_on_success() {
        let key = format!("k-{}", uuid::Uuid::new_v4());
        let t0 = Instant::now();
        for _ in 0..FAILURE_THRESHOLD - 1 {
            record(&key, false, t0);
        }
        assert!(!is_open(&key, t0), "below the threshold it stays closed");
        record(&key, false, t0);
        assert!(is_open(&key, t0));
        assert!(!is_open(&key, t0 + OPEN_FOR + Duration::from_secs(1)), "it closes by itself");
        record(&key, false, t0);
        record(&key, true, t0);
        assert!(!is_open(&key, t0), "a success resets it");
        for _ in 0..FAILURE_THRESHOLD - 1 {
            record(&key, false, t0);
        }
        assert!(!is_open(&key, t0), "and the count started again from zero");
    }

    #[test]
    fn tokens_are_unknown_not_zero_when_the_provider_gives_none() {
        assert_eq!(tokens(br#"{"usage":{"prompt_tokens":12,"completion_tokens":5}}"#), (Some(12), Some(5)));
        assert_eq!(tokens(br#"{"choices":[]}"#), (None, None));
        assert_eq!(tokens(b"not json"), (None, None));
    }

    #[test]
    fn chain_names_are_short_plain_words() {
        assert!(valid_chain_name("main-2_x"));
        for bad in ["", "a b", "../x", &"a".repeat(41), "a/b"] {
            assert!(!valid_chain_name(bad), "{bad}");
        }
    }
}
