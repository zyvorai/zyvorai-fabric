// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: LicenseRef-Zyvor-Production-2.0

//! LLM-as-judge scoring for captured traces.
//!
//! A [`JudgeConfig`] on a trace policy names another Fabric endpoint that
//! serves a grading model. After a trace is stored, a background task asks the
//! judge to rate the completion from 0 to 1 and writes the score back
//! (`score_source = "judge"`). Human feedback is never overwritten. Judge calls
//! go straight to the judge endpoint, so they are not themselves traced.

use axum::{extract::State, http::StatusCode, Json};
use security::RequireWrite;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::sync::Arc;

use crate::server::AppState;

use super::traces::{self, Trace, STORE_TRACES};
use super::types::{InferenceDeployment, InferenceEndpoint};
use super::{audit, err, STORE_DEPLOYMENTS, STORE_ENDPOINTS};

type ApiErr = (StatusCode, Json<Value>);

pub const DEFAULT_RUBRIC: &str = "Rate how correct, helpful and complete the assistant's final \
answer is for the conversation. 1.0 is excellent, 0.5 is partly right, 0.0 is wrong or unhelpful.";
const MAX_PROMPT_CHARS: usize = 6000;
const MAX_REASON_CHARS: usize = 300;
const JUDGE_TIMEOUT_SECS: &str = "30";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct JudgeConfig {
    /// Fabric `InferenceEndpoint` serving the judge model.
    pub endpoint: String,
    /// Grading instructions; defaults to [`DEFAULT_RUBRIC`].
    #[serde(default)]
    pub rubric: Option<String>,
    /// Fraction of recorded traces sent to the judge, 0.0..=1.0.
    #[serde(default = "one")]
    pub sample_rate: f64,
}

fn one() -> f64 {
    1.0
}

impl JudgeConfig {
    pub fn validate(&self) -> Result<(), String> {
        if self.endpoint.trim().is_empty() {
            return Err("judge.endpoint is required".into());
        }
        if !(0.0..=1.0).contains(&self.sample_rate) || self.sample_rate.is_nan() {
            return Err("judge.sample_rate must be between 0 and 1".into());
        }
        if self.rubric.as_deref().is_some_and(|r| r.len() > 2000) {
            return Err("judge.rubric is limited to 2000 characters".into());
        }
        Ok(())
    }

    fn rubric(&self) -> &str {
        self.rubric
            .as_deref()
            .map(str::trim)
            .filter(|r| !r.is_empty())
            .unwrap_or(DEFAULT_RUBRIC)
    }
}

fn content_text(v: &Value) -> String {
    match v {
        Value::String(s) => s.clone(),
        Value::Null => String::new(),
        other => other.to_string(),
    }
}

/// The conversation as plain text, newest turns kept when it is too long.
pub fn render_conversation(request: &Value) -> String {
    let Some(messages) = request.get("messages").and_then(Value::as_array) else {
        return String::new();
    };
    let mut lines: Vec<String> = messages
        .iter()
        .map(|m| {
            let role = m.get("role").and_then(Value::as_str).unwrap_or("user");
            format!(
                "{role}: {}",
                content_text(m.get("content").unwrap_or(&Value::Null))
            )
        })
        .collect();
    let mut total: usize = lines.iter().map(|l| l.len() + 1).sum();
    while total > MAX_PROMPT_CHARS && lines.len() > 1 {
        total -= lines.remove(0).len() + 1;
    }
    let text = lines.join("\n");
    if text.len() > MAX_PROMPT_CHARS {
        let mut cut = text.len() - MAX_PROMPT_CHARS;
        while !text.is_char_boundary(cut) {
            cut += 1;
        }
        text[cut..].to_string()
    } else {
        text
    }
}

fn truncate_chars(s: &str, max: usize) -> String {
    s.chars().take(max).collect()
}

/// Chat-completion body asking the judge model for `{"score","reason"}`.
pub fn build_judge_request(
    cfg: &JudgeConfig,
    model: &str,
    conversation: &str,
    answer: &str,
) -> Value {
    let system = format!(
        "You are a strict, impartial grader. {}\nReply with only a JSON object: \
{{\"score\": <number from 0 to 1>, \"reason\": \"<one short sentence>\"}}.",
        cfg.rubric()
    );
    let user = format!(
        "## Conversation\n{conversation}\n\n## Assistant answer to grade\n{}",
        truncate_chars(answer, MAX_PROMPT_CHARS)
    );
    json!({
        "model": model,
        "temperature": 0,
        "max_tokens": 200,
        "stream": false,
        "messages": [
            {"role": "system", "content": system},
            {"role": "user", "content": user},
        ],
    })
}

/// Pull `{score, reason}` out of a judge reply, tolerating code fences and
/// chatter around the JSON. Scores outside 0..=1 are rejected, not clamped.
pub fn parse_judge_reply(text: &str) -> Option<(f64, Option<String>)> {
    let start = text.find('{')?;
    let end = text.rfind('}')?;
    if end < start {
        return None;
    }
    let v: Value = serde_json::from_str(&text[start..=end]).ok()?;
    let score = match v.get("score")? {
        Value::Number(n) => n.as_f64()?,
        Value::String(s) => s.trim().parse::<f64>().ok()?,
        _ => return None,
    };
    if !score.is_finite() || !(0.0..=1.0).contains(&score) {
        return None;
    }
    let reason = v
        .get("reason")
        .and_then(Value::as_str)
        .map(|r| truncate_chars(r.trim(), MAX_REASON_CHARS))
        .filter(|r| !r.is_empty());
    Some((score, reason))
}

#[derive(Debug, PartialEq)]
pub enum Outcome {
    Judged,
    /// Nothing to judge (redacted, no text) or a human already scored it.
    Skipped,
    Failed(String),
}

fn load_judge(
    state: &state_store::StateStore,
    name: &str,
) -> Result<(InferenceEndpoint, InferenceDeployment), String> {
    let ep: InferenceEndpoint = state
        .get_entity(STORE_ENDPOINTS, name)
        .map_err(|e| e.to_string())?
        .ok_or_else(|| format!("judge endpoint {name} not found"))?;
    let dep: InferenceDeployment = state
        .get_entity(STORE_DEPLOYMENTS, &ep.deployment)
        .map_err(|e| e.to_string())?
        .ok_or_else(|| format!("judge deployment {} not found", ep.deployment))?;
    Ok((ep, dep))
}

/// Score one stored trace with the judge model and persist the result.
pub async fn judge_trace(
    store: &state_store::StateStore,
    trace_id: &str,
    cfg: &JudgeConfig,
    rejudge: bool,
) -> Outcome {
    let Ok(Some(trace)) = store.get_entity::<Trace>(STORE_TRACES, trace_id) else {
        return Outcome::Failed("trace not found".into());
    };
    if trace
        .score_source
        .as_deref()
        .is_some_and(|s| s.starts_with("feedback:"))
        || (!rejudge && trace.score_source.as_deref() == Some("judge"))
    {
        return Outcome::Skipped;
    }
    let (Some(request), Some(answer)) = (trace.request.as_ref(), trace.completion.as_ref()) else {
        return Outcome::Skipped;
    };
    if !(200..300).contains(&trace.status) {
        return Outcome::Skipped;
    }
    let (ep, dep) = match load_judge(store, &cfg.endpoint) {
        Ok(x) => x,
        Err(e) => return Outcome::Failed(e),
    };
    let body = build_judge_request(cfg, &dep.model, &render_conversation(request), answer);
    let bytes = axum::body::Bytes::from(serde_json::to_vec(&body).unwrap_or_default());
    let mut headers = axum::http::HeaderMap::new();
    headers.insert(
        axum::http::header::CONTENT_TYPE,
        axum::http::HeaderValue::from_static("application/json"),
    );
    headers.insert(
        "x-request-timeout",
        axum::http::HeaderValue::from_static(JUDGE_TIMEOUT_SECS),
    );
    let resp = match super::gateway::connect_upstream(
        &axum::http::Method::POST,
        &ep,
        &dep,
        "/v1/chat/completions",
        &headers,
        &bytes,
    )
    .await
    {
        Ok((resp, _)) => resp,
        Err((_, e)) => return Outcome::Failed(e),
    };
    if !resp.status().is_success() {
        return Outcome::Failed(format!("judge returned {}", resp.status()));
    }
    let Ok(raw) = resp.bytes().await else {
        return Outcome::Failed("judge response unreadable".into());
    };
    let reply = traces::parse_response(&raw).completion;
    let Some((score, reason)) = parse_judge_reply(&reply) else {
        return Outcome::Failed("judge reply had no valid score".into());
    };
    let saved = store.update_entity_exclusive(STORE_TRACES, trace_id, |mut t: Trace| {
        // A human may have scored it while the judge was thinking.
        if t.score_source
            .as_deref()
            .is_some_and(|s| s.starts_with("feedback:"))
        {
            return Ok::<_, String>(t);
        }
        t.score = Some(score);
        t.score_source = Some("judge".into());
        t.judge_reason = reason.clone();
        Ok(t)
    });
    match saved {
        Ok(_) => Outcome::Judged,
        Err(e) => Outcome::Failed(e.to_string()),
    }
}

/// Fire-and-forget scoring for a freshly recorded trace.
pub fn spawn_judge(store: state_store::StateStore, trace_id: String, cfg: JudgeConfig) {
    if cfg.sample_rate < 1.0 && rand::random::<f64>() >= cfg.sample_rate {
        return;
    }
    let Ok(handle) = tokio::runtime::Handle::try_current() else {
        return;
    };
    handle.spawn(async move {
        if let Outcome::Failed(e) = judge_trace(&store, &trace_id, &cfg, false).await {
            tracing::debug!("judge skipped trace {trace_id}: {e}");
        }
    });
}

#[derive(Debug, Deserialize)]
pub struct BatchRequest {
    /// Trace endpoint to judge. Required.
    pub endpoint: String,
    /// Overrides the policy's judge config.
    #[serde(default)]
    pub judge: Option<JudgeConfig>,
    #[serde(default)]
    pub variant: Option<String>,
    /// Re-score traces the judge already scored.
    #[serde(default)]
    pub rejudge: bool,
    #[serde(default)]
    pub limit: Option<usize>,
}

#[derive(Debug, Serialize, Default)]
pub struct BatchResult {
    pub judged: usize,
    pub skipped: usize,
    pub failed: usize,
    pub errors: Vec<String>,
}

const BATCH_MAX: usize = 200;
const BATCH_PARALLEL: usize = 4;

/// POST /api/ai/traces/judge: score existing traces on demand.
pub async fn judge_batch(
    RequireWrite(claims): RequireWrite,
    State(state): State<Arc<AppState>>,
    Json(req): Json<BatchRequest>,
) -> Result<Json<BatchResult>, ApiErr> {
    let cfg = match req.judge.clone() {
        Some(c) => c,
        None => traces::policy_for(&state, &req.endpoint)
            .and_then(|p| p.judge)
            .ok_or_else(|| {
                err(
                    StatusCode::BAD_REQUEST,
                    "no judge config: pass \"judge\" or set one on the trace policy",
                )
            })?,
    };
    cfg.validate()
        .map_err(|m| err(StatusCode::BAD_REQUEST, m))?;
    load_judge(&state.store, &cfg.endpoint).map_err(|m| err(StatusCode::NOT_FOUND, m))?;
    let mut all: Vec<Trace> = state
        .store
        .list_entities(STORE_TRACES)
        .map_err(|e| err(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    all.retain(|t| {
        t.endpoint == req.endpoint
            && req
                .variant
                .as_deref()
                .is_none_or(|v| t.variant.as_deref() == Some(v))
    });
    all.sort_by_key(|t| std::cmp::Reverse(t.created_unix));
    all.truncate(req.limit.unwrap_or(50).clamp(1, BATCH_MAX));
    use futures::StreamExt;
    let outcomes: Vec<Outcome> = futures::stream::iter(all.into_iter().map(|t| {
        let store = state.store.clone();
        let cfg = cfg.clone();
        let rejudge = req.rejudge;
        async move { judge_trace(&store, &t.id, &cfg, rejudge).await }
    }))
    .buffer_unordered(BATCH_PARALLEL)
    .collect()
    .await;
    let mut out = BatchResult::default();
    for o in outcomes {
        match o {
            Outcome::Judged => out.judged += 1,
            Outcome::Skipped => out.skipped += 1,
            Outcome::Failed(e) => {
                out.failed += 1;
                if out.errors.len() < 5 && !out.errors.contains(&e) {
                    out.errors.push(e);
                }
            }
        }
    }
    audit(
        &state,
        &claims.sub,
        "JUDGE",
        &format!("ai/traces/{}", req.endpoint),
        "SUCCESS",
    );
    Ok(Json(out))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cfg() -> JudgeConfig {
        JudgeConfig {
            endpoint: "judge".into(),
            rubric: None,
            sample_rate: 1.0,
        }
    }

    #[test]
    fn parses_plain_json() {
        assert_eq!(
            parse_judge_reply(r#"{"score": 0.8, "reason": "good"}"#),
            Some((0.8, Some("good".into())))
        );
    }

    #[test]
    fn parses_fenced_and_chatty_replies() {
        let r = "Sure!\n```json\n{\"score\": 1, \"reason\": \"\"}\n```\nDone.";
        assert_eq!(parse_judge_reply(r), Some((1.0, None)));
    }

    #[test]
    fn accepts_string_scores() {
        assert_eq!(parse_judge_reply(r#"{"score":"0.25"}"#), Some((0.25, None)));
    }

    #[test]
    fn rejects_out_of_range_and_garbage() {
        assert_eq!(parse_judge_reply(r#"{"score": 4}"#), None);
        assert_eq!(parse_judge_reply(r#"{"score": -0.1}"#), None);
        assert_eq!(parse_judge_reply("no json here"), None);
        assert_eq!(parse_judge_reply(r#"{"reason":"x"}"#), None);
        assert_eq!(parse_judge_reply("} {"), None);
    }

    #[test]
    fn reason_is_truncated() {
        let long = "x".repeat(1000);
        let (_, r) = parse_judge_reply(&format!(r#"{{"score":0.5,"reason":"{long}"}}"#)).unwrap();
        assert_eq!(r.unwrap().chars().count(), MAX_REASON_CHARS);
    }

    #[test]
    fn request_uses_default_and_custom_rubric() {
        let body = build_judge_request(&cfg(), "m", "user: hi", "hello");
        assert_eq!(body["model"], "m");
        assert_eq!(body["stream"], false);
        assert!(body["messages"][0]["content"]
            .as_str()
            .unwrap()
            .contains("strict"));
        assert!(body["messages"][1]["content"]
            .as_str()
            .unwrap()
            .contains("hello"));
        let mut c = cfg();
        c.rubric = Some("Score brevity.".into());
        let body = build_judge_request(&c, "m", "", "a");
        assert!(body["messages"][0]["content"]
            .as_str()
            .unwrap()
            .contains("Score brevity."));
    }

    #[test]
    fn conversation_keeps_newest_turns_when_long() {
        let msgs: Vec<Value> = (0..40)
            .map(|i| json!({"role":"user","content": format!("turn{i} {}", "y".repeat(400))}))
            .collect();
        let text = render_conversation(&json!({ "messages": msgs }));
        assert!(text.len() <= MAX_PROMPT_CHARS);
        assert!(text.contains("turn39"));
        assert!(!text.contains("turn0 "));
    }

    #[test]
    fn config_validation() {
        assert!(cfg().validate().is_ok());
        let mut c = cfg();
        c.endpoint = " ".into();
        assert!(c.validate().is_err());
        let mut c = cfg();
        c.sample_rate = 1.5;
        assert!(c.validate().is_err());
        let mut c = cfg();
        c.rubric = Some("r".repeat(2001));
        assert!(c.validate().is_err());
    }
}
