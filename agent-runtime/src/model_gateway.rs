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
        && name
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_'))
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
pub fn targets(
    file: Option<&Path>,
    name: &str,
    base: &ModelSpec,
) -> Result<Vec<ModelSpec>, String> {
    chain_targets(file, name)?
        .iter()
        .map(|t| {
            t.applied_to(base)
                .map_err(|e| format!("chain '{name}': {e}"))
        })
        .collect()
}

/// The raw targets of chain `name`, for the agent's `model_socket` path, which has no `ModelSpec` to apply them to.
pub fn chain_targets(file: Option<&Path>, name: &str) -> Result<Vec<Target>, String> {
    let path =
        file.ok_or("this pack names a fallback chain, but the operator configured no chains file")?;
    let text = std::fs::read_to_string(path).map_err(|e| format!("chains file: {e}"))?;
    let parsed: ChainsFile =
        serde_json::from_str(&text).map_err(|e| format!("chains file: {e}"))?;
    let chain = parsed
        .chains
        .get(name)
        .ok_or_else(|| format!("no fallback chain named '{name}'"))?;
    if chain.targets.is_empty() || chain.targets.len() > MAX_FALLBACKS {
        return Err(format!("chain '{name}' needs 1 to {MAX_FALLBACKS} targets"));
    }
    for t in &chain.targets {
        let u =
            url::Url::parse(&t.base_url).map_err(|e| format!("chain '{name}': base_url: {e}"))?;
        if !matches!(u.scheme(), "http" | "https")
            || u.host_str().is_none()
            || u.query().is_some()
            || u.fragment().is_some()
        {
            return Err(format!(
                "chain '{name}': base_url must be an http(s) URL with a host and no query"
            ));
        }
        if t.model.is_empty() || t.model.len() > 80 || t.model.chars().any(char::is_control) {
            return Err(format!(
                "chain '{name}': model must be 1-80 plain characters"
            ));
        }
    }
    Ok(chain.targets.clone())
}

/// The request to send to `target` in place of `req`, if `req` is the agent's chat call to its own model socket:
/// a POST of JSON to `<socket base>/chat/completions` with the socket's credential. Anything else is not a model
/// call and is never redirected. Only the URL, the credential and the JSON `model` change; the messages go unaltered.
pub fn retarget(
    req: &crate::model::EgressRequest,
    socket: &crate::model::ModelSocket,
    target: &Target,
) -> Option<crate::model::EgressRequest> {
    use base64::Engine;
    let want = format!("{}/chat/completions", socket.base_url.trim_end_matches('/'));
    if !req.method.eq_ignore_ascii_case("POST")
        || req.url != want
        || req.credential != socket.credential
    {
        return None;
    }
    let raw = base64::engine::general_purpose::STANDARD
        .decode(req.body_base64.as_deref()?)
        .ok()?;
    let mut body: Value = serde_json::from_slice(&raw).ok()?;
    body.as_object_mut()?
        .insert("model".into(), Value::String(target.model.clone()));
    Some(crate::model::EgressRequest {
        url: format!("{}/chat/completions", target.base_url.trim_end_matches('/')),
        method: req.method.clone(),
        headers: req.headers.clone(),
        body_base64: Some(
            base64::engine::general_purpose::STANDARD.encode(serde_json::to_vec(&body).ok()?),
        ),
        credential: Some(target.credential.clone()),
    })
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
    let n = |k: &str| {
        v.get("usage")
            .and_then(|u| u.get(k))
            .and_then(Value::as_u64)
    };
    (n("prompt_tokens"), n("completion_tokens"))
}

/// Identifies an endpoint for the breaker: where it is, which model, and which key.
pub fn breaker_key(spec: &ModelSpec) -> String {
    endpoint_key(&spec.base_url, &spec.model, &spec.credential)
}

pub fn endpoint_key(base_url: &str, model: &str, credential: &str) -> String {
    format!("{base_url}|{model}|{credential}")
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
    map.get(key)
        .and_then(|b| b.open_until)
        .is_some_and(|t| t > now)
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

/// Operator-written prices and budgets (`ZYVOR_AGENT_MODEL_PRICES_FILE`). Merion ships no prices: a model with no entry
/// here has no cost figure, and its calls are counted as unpriced rather than free.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Prices {
    /// Free text shown next to costs, such as "USD". Never used to convert anything.
    #[serde(default)]
    pub currency: String,
    /// Per model id, cost per million tokens.
    #[serde(default)]
    pub prices: BTreeMap<String, Price>,
    /// Applied to an agent with no entry of its own.
    #[serde(default)]
    pub default_budget: Option<Budget>,
    #[serde(default)]
    pub budgets: BTreeMap<String, Budget>,
}

#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Price {
    pub input_per_mtok: f64,
    pub output_per_mtok: f64,
}

/// A rolling 24-hour allowance for one agent's model calls, over both the use-case step and the `model_socket`.
#[derive(Debug, Clone, Copy, Default, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Budget {
    #[serde(default)]
    pub max_calls_per_day: Option<u64>,
    /// Counts only calls that reported tokens for a model with a price; the rest are not guessed.
    #[serde(default)]
    pub max_cost_per_day: Option<f64>,
    /// A cap for one run (one session), counted over its whole life rather than a day.
    #[serde(default)]
    pub run: Option<Limit>,
    /// A cap for one goal: all the sessions started for it (its steps and its planning session) together.
    #[serde(default)]
    pub goal: Option<Limit>,
}

/// A cap on model calls and cost with no time window, for a run or a goal.
#[derive(Debug, Clone, Copy, Default, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Limit {
    #[serde(default)]
    pub max_calls: Option<u64>,
    /// Counts only calls that reported tokens for a model with a price; the rest are not guessed.
    #[serde(default)]
    pub max_cost: Option<f64>,
}

impl Prices {
    pub fn load(file: Option<&Path>) -> Result<Option<Prices>, String> {
        let Some(path) = file else { return Ok(None) };
        let text = std::fs::read_to_string(path).map_err(|e| format!("prices file: {e}"))?;
        let p: Prices = serde_json::from_str(&text).map_err(|e| format!("prices file: {e}"))?;
        let bad = |x: f64| !x.is_finite() || x < 0.0;
        if p.prices
            .values()
            .any(|v| bad(v.input_per_mtok) || bad(v.output_per_mtok))
            || p.budgets.values().chain(p.default_budget.iter()).any(|b| {
                b.max_cost_per_day.is_some_and(bad)
                    || [b.run, b.goal]
                        .into_iter()
                        .flatten()
                        .any(|l| l.max_cost.is_some_and(bad))
            })
        {
            return Err("prices file: prices and budgets must be finite and not negative".into());
        }
        Ok(Some(p))
    }

    pub fn budget_for(&self, agent: &str) -> Option<Budget> {
        self.budgets.get(agent).copied().or(self.default_budget)
    }

    fn cost(&self, model: &str, prompt: u64, completion: u64) -> Option<f64> {
        let p = self.prices.get(model)?;
        Some(
            (prompt as f64 * p.input_per_mtok + completion as f64 * p.output_per_mtok)
                / 1_000_000.0,
        )
    }
}

/// The cost of one journaled `model.call` row: known only with a price for its model and reported tokens.
fn call_cost(prices: Option<&Prices>, r: &crate::audit::AuditEntry) -> Option<f64> {
    let n = |k: &str| r.detail.get(k).and_then(Value::as_u64);
    let model = r.detail.get("model").and_then(Value::as_str)?;
    prices?.cost(model, n("prompt_tokens")?, n("completion_tokens")?)
}

/// Refuse a model call when `agent` is at its budget. `rows` are the journal rows of the agent's sessions. Calls that
/// failed before reaching a provider are not counted; only `Performed` rows are.
pub fn over_budget(
    prices: &Prices,
    agent: &str,
    rows: &[crate::audit::AuditEntry],
    since: chrono::DateTime<chrono::Utc>,
) -> Option<String> {
    use crate::audit::AuditPhase;
    let b = prices.budget_for(agent)?;
    let mine: Vec<_> = rows
        .iter()
        .filter(|r| r.action == "model.call" && r.phase == AuditPhase::Performed && r.at >= since)
        .collect();
    if let Some(max) = b.max_calls_per_day.filter(|m| mine.len() as u64 >= *m) {
        return Some(format!(
            "model budget reached for agent '{agent}': {} of {max} calls in the last 24 hours",
            mine.len()
        ));
    }
    if let Some(max) = b.max_cost_per_day {
        let spent: f64 = mine.iter().filter_map(|r| call_cost(Some(prices), r)).sum();
        if spent >= max {
            return Some(format!("model budget reached for agent '{agent}': {spent:.4} of {max} {} in the last 24 hours", prices.currency).replace("  ", " "));
        }
    }
    None
}

/// Refuse a model call when `rows` (the journal rows of one run, or of every session of one goal) already reach `limit`.
/// `what` names the scope in the message, such as `run` or `goal`.
pub fn over_limit(
    prices: &Prices,
    what: &str,
    limit: Limit,
    rows: &[&crate::audit::AuditEntry],
) -> Option<String> {
    use crate::audit::AuditPhase;
    let mine: Vec<_> = rows
        .iter()
        .filter(|r| r.action == "model.call" && r.phase == AuditPhase::Performed)
        .collect();
    if let Some(max) = limit.max_calls.filter(|m| mine.len() as u64 >= *m) {
        return Some(format!(
            "model budget reached for this {what}: {} of {max} calls",
            mine.len()
        ));
    }
    if let Some(max) = limit.max_cost {
        let spent: f64 = mine.iter().filter_map(|r| call_cost(Some(prices), r)).sum();
        if spent >= max {
            return Some(
                format!(
                    "model budget reached for this {what}: {spent:.4} of {max} {}",
                    prices.currency
                )
                .replace("  ", " "),
            );
        }
    }
    None
}

/// Per endpoint: calls, failures, tokens, latency and, with a price table, cost. Tokens the provider did not report are
/// not counted, and `tokens_unreported` says how many calls had none, so a total is never mistaken for a complete one.
/// `cost` covers only calls with reported tokens on a priced model; `calls_unpriced` counts the rest.
pub fn summarize(rows: &[crate::audit::AuditEntry], prices: Option<&Prices>) -> Value {
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
        cost: f64,
        unpriced: u64,
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
        let text = |k: &str| {
            r.detail
                .get(k)
                .and_then(Value::as_str)
                .unwrap_or("?")
                .to_string()
        };
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
        match call_cost(prices, r) {
            Some(c) => e.cost += c,
            None => e.unpriced += 1,
        }
        if let Some(ms) = n("latency_ms") {
            e.latency_sum += ms;
            e.latency_n += 1;
        }
    }
    let priced = prices.is_some();
    let items: Vec<Value> = by
        .into_iter()
        .map(|((host, model), e)| {
            let mut v = serde_json::json!({
                "host": host, "model": model, "calls": e.calls, "failed": e.failed,
                "prompt_tokens": e.prompt, "completion_tokens": e.completion, "tokens_unreported": e.unreported,
                "avg_latency_ms": e.latency_sum.checked_div(e.latency_n),
            });
            if priced {
                v["cost"] = serde_json::json!(e.cost);
                v["calls_unpriced"] = serde_json::json!(e.unpriced);
            }
            v
        })
        .collect();
    let mut out = serde_json::json!({ "items": items, "failovers": failovers });
    if let Some(p) = prices {
        out["currency"] = serde_json::json!(p.currency);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(
        action: &str,
        phase: crate::audit::AuditPhase,
        detail: Value,
    ) -> crate::audit::AuditEntry {
        crate::audit::AuditEntry {
            seq: 0,
            at: chrono::Utc::now(),
            session_id: None,
            phase,
            action: action.into(),
            subject: None,
            detail,
            prev_hash: String::new(),
            hash: String::new(),
        }
    }

    #[test]
    fn usage_is_summed_per_endpoint_and_never_invents_tokens() {
        use crate::audit::AuditPhase::{Failed, Performed};
        let ok = |p: Option<u64>, c: Option<u64>, ms: u64| {
            row(
                "model.call",
                Performed,
                serde_json::json!({"host":"a","model":"m","prompt_tokens":p,"completion_tokens":c,"latency_ms":ms}),
            )
        };
        let rows = vec![
            ok(Some(10), Some(5), 100),
            ok(Some(20), Some(5), 300),
            ok(None, None, 200),
            row(
                "model.call",
                Failed,
                serde_json::json!({"host":"a","model":"m"}),
            ),
            row(
                "model.call",
                Performed,
                serde_json::json!({"host":"b","model":"n"}),
            ),
            row("model.failover", Performed, serde_json::json!({})),
            row("changeset.apply", Performed, serde_json::json!({})),
        ];
        let s = summarize(&rows, None);
        assert_eq!(s["failovers"], 1);
        let a = &s["items"][0];
        assert_eq!(
            (
                a["host"].as_str(),
                a["calls"].as_u64(),
                a["failed"].as_u64()
            ),
            (Some("a"), Some(4), Some(1))
        );
        assert_eq!(
            (a["prompt_tokens"].as_u64(), a["completion_tokens"].as_u64()),
            (Some(30), Some(10))
        );
        assert_eq!(
            a["tokens_unreported"], 1,
            "a call with no usage is counted as unreported, not as zero"
        );
        assert_eq!(a["avg_latency_ms"], 200);
        assert_eq!(
            s["items"][1]["avg_latency_ms"],
            Value::Null,
            "no latency recorded means null, not 0"
        );
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
        let f = file(
            r#"{"chains":{"main":{"targets":[
            {"credential":"b","base_url":"https://b.example/v1","model":"m2"},
            {"credential":"c","base_url":"http://127.0.0.1:9/v1","model":"m3"}]}}}"#,
        );
        let t = targets(Some(&f), "main", &base()).unwrap();
        assert_eq!(t.len(), 2);
        assert_eq!(t[0].credential, "b");
        assert_eq!(t[1].base_url, "http://127.0.0.1:9/v1");
        assert!(t
            .iter()
            .all(|s| s.instruction == "Summarise." && s.fallback_chain.is_none()));
    }

    #[test]
    fn every_misconfiguration_is_an_error_not_a_silent_skip() {
        assert!(targets(None, "main", &base())
            .unwrap_err()
            .contains("no chains file"));
        let missing = std::env::temp_dir().join("no-such-chains.json");
        assert!(targets(Some(&missing), "main", &base()).is_err());
        let f = file(
            r#"{"chains":{"other":{"targets":[{"credential":"b","base_url":"https://b.example/v1","model":"m"}]}}}"#,
        );
        assert!(targets(Some(&f), "main", &base())
            .unwrap_err()
            .contains("no fallback chain named"));
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
            .map(|i| {
                format!(r#"{{"credential":"b","base_url":"https://b.example/v1","model":"m{i}"}}"#)
            })
            .collect();
        let f = file(&format!(
            r#"{{"chains":{{"main":{{"targets":[{}]}}}}}}"#,
            six.join(",")
        ));
        assert!(targets(Some(&f), "main", &base())
            .unwrap_err()
            .contains("1 to"));
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
        assert!(
            !is_open(&key, t0 + OPEN_FOR + Duration::from_secs(1)),
            "it closes by itself"
        );
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
        assert_eq!(
            tokens(br#"{"usage":{"prompt_tokens":12,"completion_tokens":5}}"#),
            (Some(12), Some(5))
        );
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

    fn prices(json: Value) -> Prices {
        let f = std::env::temp_dir().join(format!("zyvor-prices-{}.json", uuid::Uuid::new_v4()));
        std::fs::write(&f, json.to_string()).unwrap();
        Prices::load(Some(&f)).unwrap().unwrap()
    }

    fn call(
        model: &str,
        tokens: Option<(u64, u64)>,
        phase: crate::audit::AuditPhase,
    ) -> crate::audit::AuditEntry {
        row(
            "model.call",
            phase,
            serde_json::json!({"host":"h","model":model,"prompt_tokens":tokens.map(|t| t.0),"completion_tokens":tokens.map(|t| t.1)}),
        )
    }

    #[test]
    fn cost_needs_a_price_and_reported_tokens_and_the_rest_is_counted_not_guessed() {
        use crate::audit::AuditPhase::Performed;
        let p = prices(
            serde_json::json!({"currency":"USD","prices":{"m":{"input_per_mtok":2.0,"output_per_mtok":10.0}}}),
        );
        let rows = vec![
            call("m", Some((1_000_000, 100_000)), Performed),
            call("m", None, Performed),
            call("other", Some((5, 5)), Performed),
        ];
        let by_model = |v: &Value, m: &str| {
            v["items"]
                .as_array()
                .unwrap()
                .iter()
                .find(|i| i["model"] == m)
                .unwrap()
                .clone()
        };
        let s = summarize(&rows, Some(&p));
        assert_eq!(s["currency"], "USD");
        assert!((by_model(&s, "m")["cost"].as_f64().unwrap() - 3.0).abs() < 1e-9);
        assert_eq!(by_model(&s, "m")["calls_unpriced"], 1);
        assert_eq!(by_model(&s, "other")["calls_unpriced"], 1);
        assert!(
            summarize(&rows, None)["items"][0].get("cost").is_none(),
            "no price file, no cost figure"
        );
    }

    #[test]
    fn a_budget_stops_an_agent_by_calls_or_by_cost_and_only_counts_what_was_performed() {
        use crate::audit::AuditPhase::{Failed, Performed};
        let p = prices(serde_json::json!({
            "currency":"USD",
            "prices":{"m":{"input_per_mtok":1.0,"output_per_mtok":1.0}},
            "default_budget":{"max_calls_per_day":3},
            "budgets":{"spender":{"max_cost_per_day":1.5}}
        }));
        let since = chrono::Utc::now() - chrono::Duration::hours(24);
        let two = vec![
            call("m", None, Performed),
            call("m", None, Performed),
            call("m", None, Failed),
        ];
        assert!(
            over_budget(&p, "any", &two, since).is_none(),
            "a failed call is not spent"
        );
        let three = vec![
            call("m", None, Performed),
            call("m", None, Performed),
            call("m", None, Performed),
        ];
        assert!(over_budget(&p, "any", &three, since)
            .unwrap()
            .contains("3 of 3 calls"));
        let spend = vec![call("m", Some((1_000_000, 1_000_000)), Performed)];
        assert!(over_budget(&p, "spender", &spend, since)
            .unwrap()
            .contains("budget reached"));
        assert!(over_budget(
            &p,
            "spender",
            &[call("m", Some((100, 100)), Performed)],
            since
        )
        .is_none());
        let old = chrono::Utc::now() + chrono::Duration::hours(1);
        assert!(
            over_budget(&p, "any", &three, old).is_none(),
            "calls older than the window do not count"
        );
    }

    #[test]
    fn a_prices_file_with_a_negative_or_unknown_field_is_refused() {
        for bad in [
            serde_json::json!({"prices":{"m":{"input_per_mtok":-1.0,"output_per_mtok":1.0}}}),
            serde_json::json!({"budgets":{"a":{"max_cost_per_day":-5.0}}}),
            serde_json::json!({"surprise":1}),
        ] {
            let f =
                std::env::temp_dir().join(format!("zyvor-prices-{}.json", uuid::Uuid::new_v4()));
            std::fs::write(&f, bad.to_string()).unwrap();
            assert!(Prices::load(Some(&f)).is_err(), "{bad}");
        }
        assert!(Prices::load(None).unwrap().is_none());
    }
}
