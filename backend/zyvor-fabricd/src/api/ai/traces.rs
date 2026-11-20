// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: LicenseRef-Zyvor-Production-1.0
//!
//! Inference trace capture (Beta).
//!
//! The gateway can record every request/response that crosses an endpoint:
//! the exact JSON, token usage, latency, and — when the backend returns them —
//! the prompt/completion token IDs and per-token logprobs. Recording the IDs
//! the engine actually used (instead of re-tokenizing text later) keeps a
//! later training or evaluation pass faithful to what was served.
//!
//! Traces are opt-in per endpoint through a [`TracePolicy`]. They can be
//! scored (feedback API or automatic scorers), listed, and exported as JSONL
//! for fine-tuning or offline evaluation.

use axum::{
    extract::{Path, Query, State},
    http::{header, StatusCode},
    response::{IntoResponse, Response},
    Json,
};
use chrono::Utc;
use security::{RequireRead, RequireWrite};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use crate::server::AppState;

use super::{audit, err};

pub(crate) const STORE_TRACES: &str = "ai_traces";
pub(crate) const STORE_TRACE_POLICIES: &str = "ai_trace_policies";

const DEFAULT_MAX_BODY: usize = 256 * 1024;
const DEFAULT_MAX_TRACES: usize = 10_000;
const PRUNE_EVERY: u64 = 100;
const MAX_LIST: usize = 1000;

/// Per-endpoint capture settings. `endpoint` is the key.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TracePolicy {
    pub endpoint: String,
    #[serde(default = "yes")]
    pub enabled: bool,
    /// Fraction of requests recorded, 0.0..=1.0.
    #[serde(default = "one")]
    pub sample_rate: f64,
    /// Largest request/response body kept per trace (bytes).
    #[serde(default = "default_max_body")]
    pub max_body_bytes: usize,
    /// Keep only hashes and sizes of prompts and completions.
    #[serde(default)]
    pub redact: bool,
    /// Ask the backend (vLLM) for token IDs and logprobs on each request.
    #[serde(default)]
    pub capture_token_ids: bool,
    /// Scorer run on each finished trace.
    #[serde(default)]
    pub auto_score: Option<AutoScore>,
    /// Newest traces kept for this deployment of the store.
    #[serde(default = "default_max_traces")]
    pub max_traces: usize,
    /// LLM judge that scores finished traces in the background.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub judge: Option<super::judge::JudgeConfig>,
}

fn yes() -> bool {
    true
}
fn one() -> f64 {
    1.0
}
fn default_max_body() -> usize {
    DEFAULT_MAX_BODY
}
fn default_max_traces() -> usize {
    DEFAULT_MAX_TRACES
}

/// Deterministic scorers that need no judge model.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum AutoScore {
    /// 1.0 when the completion is non-empty and the request succeeded.
    NonEmpty,
    /// 1.0 when the engine stopped on its own (`finish_reason == "stop"`).
    FinishStop,
    /// 1.0 when the completion parses as JSON.
    JsonValid,
    /// 1.0 when the completion contains `needle`.
    Contains { needle: String },
}

impl TracePolicy {
    pub fn validate(&self) -> Result<(), String> {
        if !(0.0..=1.0).contains(&self.sample_rate) || self.sample_rate.is_nan() {
            return Err("sample_rate must be between 0 and 1".into());
        }
        if self.max_body_bytes == 0 || self.max_body_bytes > 16 * 1024 * 1024 {
            return Err("max_body_bytes must be between 1 and 16777216".into());
        }
        if self.max_traces == 0 || self.max_traces > 1_000_000 {
            return Err("max_traces must be between 1 and 1000000".into());
        }
        if let Some(j) = &self.judge {
            j.validate()?;
        }
        Ok(())
    }
}

/// One recorded inference.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Trace {
    pub id: String,
    pub created_unix: i64,
    /// Endpoint the client called.
    pub endpoint: String,
    /// Router that chose the target, when one is attached.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub router: Option<String>,
    /// Router candidate (A/B arm) that served the request.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub variant: Option<String>,
    /// Endpoint that actually served the request.
    pub target_endpoint: String,
    pub model: String,
    pub path: String,
    pub status: u16,
    pub latency_ms: u64,
    pub streamed: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tenant: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub user: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub project: Option<String>,
    pub prompt_tokens: u64,
    pub completion_tokens: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub finish_reason: Option<String>,
    /// Request JSON (absent when redacted or not JSON).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub request: Option<Value>,
    /// Assistant text (absent when redacted).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub completion: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub prompt_token_ids: Option<Vec<i64>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub completion_token_ids: Option<Vec<i64>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub logprobs: Option<Vec<f64>>,
    /// SHA-256 of the request body, always kept.
    pub request_sha256: String,
    #[serde(default)]
    pub redacted: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub score: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub score_source: Option<String>,
    /// One-line rationale when the judge model scored this trace.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub judge_reason: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub labels: Vec<String>,
}

/// What the gateway knows before it forwards a request.
#[derive(Debug, Clone)]
pub struct TraceCtx {
    pub id: String,
    pub started: std::time::Instant,
    pub policy: TracePolicy,
    pub endpoint: String,
    pub router: Option<String>,
    pub variant: Option<String>,
    pub target_endpoint: String,
    pub model: String,
    pub path: String,
    pub tenant: Option<String>,
    pub user: Option<String>,
    pub project: Option<String>,
    pub request_body: Vec<u8>,
}

/// Load the policy for an endpoint when capture is on and this request is sampled.
pub fn policy_for(state: &AppState, endpoint: &str) -> Option<TracePolicy> {
    let policy: TracePolicy = state
        .store
        .get_entity(STORE_TRACE_POLICIES, endpoint)
        .ok()
        .flatten()?;
    if !policy.enabled {
        return None;
    }
    sampled(policy.sample_rate, rand::random::<f64>()).then_some(policy)
}

pub fn sampled(rate: f64, roll: f64) -> bool {
    rate >= 1.0 || (rate > 0.0 && roll < rate)
}

/// Add the vLLM fields that make the engine return token IDs and logprobs.
/// Fields the client set itself are left alone. Returns the (possibly new) body.
pub fn inject_capture_params(body: &[u8]) -> Vec<u8> {
    let Ok(mut value) = serde_json::from_slice::<Value>(body) else {
        return body.to_vec();
    };
    let Some(obj) = value.as_object_mut() else {
        return body.to_vec();
    };
    let chat = obj.contains_key("messages");
    if obj.get("stream").and_then(Value::as_bool) == Some(true) {
        // Streams only carry token usage when asked to.
        if let Some(opts) = obj
            .entry("stream_options")
            .or_insert_with(|| json!({}))
            .as_object_mut()
        {
            opts.entry("include_usage").or_insert(json!(true));
        }
    }
    obj.entry("return_token_ids").or_insert(json!(true));
    if chat {
        obj.entry("logprobs").or_insert(json!(true));
    } else {
        obj.entry("logprobs").or_insert(json!(0));
    }
    serde_json::to_vec(&value).unwrap_or_else(|_| body.to_vec())
}

/// Fields pulled out of a finished response.
#[derive(Debug, Default, Clone, PartialEq)]
pub struct Parsed {
    pub completion: String,
    pub finish_reason: Option<String>,
    pub prompt_tokens: u64,
    pub completion_tokens: u64,
    pub prompt_token_ids: Option<Vec<i64>>,
    pub completion_token_ids: Option<Vec<i64>>,
    pub logprobs: Option<Vec<f64>>,
}

/// Parse a non-streaming OpenAI-style response body.
pub fn parse_response(body: &[u8]) -> Parsed {
    let mut out = Parsed::default();
    let Ok(v) = serde_json::from_slice::<Value>(body) else {
        return out;
    };
    apply_usage(&mut out, &v);
    if let Some(ids) = int_array(v.get("prompt_token_ids")) {
        out.prompt_token_ids = Some(ids);
    }
    let Some(choice) = v.get("choices").and_then(|c| c.get(0)) else {
        return out;
    };
    out.completion = choice
        .pointer("/message/content")
        .or_else(|| choice.get("text"))
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();
    out.finish_reason = choice
        .get("finish_reason")
        .and_then(Value::as_str)
        .map(str::to_string);
    if let Some(ids) = int_array(choice.get("token_ids")) {
        out.completion_token_ids = Some(ids);
    }
    out.logprobs = choice_logprobs(choice);
    out
}

/// Parse a buffered `text/event-stream` body (chunks joined).
pub fn parse_sse(body: &[u8]) -> Parsed {
    let mut out = Parsed::default();
    let text = String::from_utf8_lossy(body);
    let mut ids: Vec<i64> = Vec::new();
    let mut lps: Vec<f64> = Vec::new();
    for line in text.lines() {
        let Some(data) = line.strip_prefix("data:") else {
            continue;
        };
        let data = data.trim();
        if data.is_empty() || data == "[DONE]" {
            continue;
        }
        let Ok(v) = serde_json::from_str::<Value>(data) else {
            continue;
        };
        apply_usage(&mut out, &v);
        if let Some(p) = int_array(v.get("prompt_token_ids")) {
            out.prompt_token_ids = Some(p);
        }
        let Some(choice) = v.get("choices").and_then(|c| c.get(0)) else {
            continue;
        };
        if let Some(s) = choice
            .pointer("/delta/content")
            .or_else(|| choice.get("text"))
            .and_then(Value::as_str)
        {
            out.completion.push_str(s);
        }
        if let Some(f) = choice.get("finish_reason").and_then(Value::as_str) {
            out.finish_reason = Some(f.to_string());
        }
        if let Some(t) = int_array(choice.get("token_ids")) {
            ids.extend(t);
        }
        if let Some(l) = choice_logprobs(choice) {
            lps.extend(l);
        }
    }
    if !ids.is_empty() {
        out.completion_token_ids = Some(ids);
    }
    if !lps.is_empty() {
        out.logprobs = Some(lps);
    }
    out
}

fn apply_usage(out: &mut Parsed, v: &Value) {
    if let Some(u) = v.get("usage").filter(|u| u.is_object()) {
        if let Some(p) = u.get("prompt_tokens").and_then(Value::as_u64) {
            out.prompt_tokens = p;
        }
        if let Some(c) = u.get("completion_tokens").and_then(Value::as_u64) {
            out.completion_tokens = c;
        }
    }
}

fn int_array(v: Option<&Value>) -> Option<Vec<i64>> {
    let arr = v?.as_array()?;
    let ids: Vec<i64> = arr.iter().filter_map(Value::as_i64).collect();
    (ids.len() == arr.len() && !ids.is_empty()).then_some(ids)
}

fn choice_logprobs(choice: &Value) -> Option<Vec<f64>> {
    let lp = choice.get("logprobs")?;
    if let Some(content) = lp.get("content").and_then(Value::as_array) {
        let v: Vec<f64> = content
            .iter()
            .filter_map(|t| t.get("logprob").and_then(Value::as_f64))
            .collect();
        return (!v.is_empty()).then_some(v);
    }
    let arr = lp.get("token_logprobs")?.as_array()?;
    let v: Vec<f64> = arr.iter().filter_map(Value::as_f64).collect();
    (!v.is_empty()).then_some(v)
}

/// Run the policy's scorer. Failed requests always score 0.
pub fn auto_score(scorer: &AutoScore, status: u16, parsed: &Parsed) -> f64 {
    if !(200..300).contains(&status) {
        return 0.0;
    }
    let ok = match scorer {
        AutoScore::NonEmpty => !parsed.completion.trim().is_empty(),
        AutoScore::FinishStop => parsed.finish_reason.as_deref() == Some("stop"),
        AutoScore::JsonValid => serde_json::from_str::<Value>(parsed.completion.trim()).is_ok(),
        AutoScore::Contains { needle } => parsed.completion.contains(needle.as_str()),
    };
    if ok {
        1.0
    } else {
        0.0
    }
}

fn sha256_hex(data: &[u8]) -> String {
    Sha256::digest(data)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

/// Build the stored record from a finished exchange.
pub fn build_trace(ctx: &TraceCtx, status: u16, streamed: bool, resp: &[u8]) -> Trace {
    let parsed = if streamed {
        parse_sse(resp)
    } else {
        parse_response(resp)
    };
    let redact = ctx.policy.redact;
    let request = if redact || ctx.request_body.len() > ctx.policy.max_body_bytes {
        None
    } else {
        serde_json::from_slice::<Value>(&ctx.request_body).ok()
    };
    let (score, score_source) = match &ctx.policy.auto_score {
        Some(s) => (
            Some(auto_score(s, status, &parsed)),
            Some("auto".to_string()),
        ),
        None => (None, None),
    };
    let keep_text = !redact && parsed.completion.len() <= ctx.policy.max_body_bytes;
    Trace {
        id: ctx.id.clone(),
        created_unix: Utc::now().timestamp(),
        endpoint: ctx.endpoint.clone(),
        router: ctx.router.clone(),
        variant: ctx.variant.clone(),
        target_endpoint: ctx.target_endpoint.clone(),
        model: ctx.model.clone(),
        path: ctx.path.clone(),
        status,
        latency_ms: ctx.started.elapsed().as_millis() as u64,
        streamed,
        tenant: ctx.tenant.clone(),
        user: ctx.user.clone(),
        project: ctx.project.clone(),
        prompt_tokens: parsed.prompt_tokens,
        completion_tokens: parsed.completion_tokens,
        finish_reason: parsed.finish_reason.clone(),
        request,
        completion: keep_text.then(|| parsed.completion.clone()),
        prompt_token_ids: if redact {
            None
        } else {
            parsed.prompt_token_ids.clone()
        },
        completion_token_ids: if redact {
            None
        } else {
            parsed.completion_token_ids.clone()
        },
        logprobs: if redact {
            None
        } else {
            parsed.logprobs.clone()
        },
        request_sha256: sha256_hex(&ctx.request_body),
        redacted: redact,
        score,
        score_source,
        labels: Vec::new(),
        judge_reason: None,
    }
}

/// Collects a response as it streams to the client and records the trace when
/// the exchange ends, including when the client hangs up early.
pub struct TraceSink {
    store: state_store::StateStore,
    ctx: TraceCtx,
    buf: Vec<u8>,
    cap: usize,
    overflow: bool,
    pub status: u16,
    streamed: bool,
}

impl TraceSink {
    pub fn new(store: state_store::StateStore, ctx: TraceCtx, status: u16, streamed: bool) -> Self {
        let cap = ctx.policy.max_body_bytes.saturating_mul(4);
        Self {
            store,
            ctx,
            buf: Vec::new(),
            cap,
            overflow: false,
            status,
            streamed,
        }
    }

    pub fn push(&mut self, chunk: &[u8]) {
        if self.overflow {
            return;
        }
        if self.buf.len() + chunk.len() > self.cap {
            // A cut-off body cannot be parsed; keep request, status and latency only.
            self.overflow = true;
            self.buf.clear();
            return;
        }
        self.buf.extend_from_slice(chunk);
    }
}

impl Drop for TraceSink {
    fn drop(&mut self) {
        record(
            &self.store,
            &self.ctx,
            self.status,
            self.streamed,
            &self.buf,
        );
    }
}

static WRITES: AtomicU64 = AtomicU64::new(0);

/// Persist a finished trace and prune old ones now and then. Never fails the request.
pub fn record(
    store: &state_store::StateStore,
    ctx: &TraceCtx,
    status: u16,
    streamed: bool,
    resp: &[u8],
) {
    let trace = build_trace(ctx, status, streamed, resp);
    if let Err(e) = store.save_entity(STORE_TRACES, &trace.id, &trace) {
        tracing::warn!("trace write failed: {e}");
        return;
    }
    if let Some(cfg) = ctx.policy.judge.clone() {
        super::judge::spawn_judge(store.clone(), trace.id.clone(), cfg);
    }
    if WRITES.fetch_add(1, Ordering::Relaxed) % PRUNE_EVERY == PRUNE_EVERY - 1 {
        prune(store, ctx.policy.max_traces);
    }
}

fn prune(store: &state_store::StateStore, keep: usize) {
    let Ok(mut all) = store.list_entities::<Trace>(STORE_TRACES) else {
        return;
    };
    if all.len() <= keep {
        return;
    }
    all.sort_by_key(|t| std::cmp::Reverse(t.created_unix));
    for old in all.into_iter().skip(keep) {
        let _ = store.delete_entity(STORE_TRACES, &old.id);
    }
}

// ---- REST ------------------------------------------------------------------

type ApiErr = (StatusCode, Json<Value>);

#[derive(Debug, Default, Deserialize)]
pub struct TraceQuery {
    pub endpoint: Option<String>,
    pub router: Option<String>,
    pub variant: Option<String>,
    pub model: Option<String>,
    pub since: Option<i64>,
    pub min_score: Option<f64>,
    pub max_score: Option<f64>,
    pub scored: Option<bool>,
    pub limit: Option<usize>,
    /// Export only: `raw` (default) or `sft`.
    pub format: Option<String>,
}

pub fn matches(t: &Trace, q: &TraceQuery) -> bool {
    q.endpoint.as_deref().is_none_or(|e| t.endpoint == e)
        && q.router
            .as_deref()
            .is_none_or(|r| t.router.as_deref() == Some(r))
        && q.variant
            .as_deref()
            .is_none_or(|v| t.variant.as_deref() == Some(v))
        && q.model.as_deref().is_none_or(|m| t.model == m)
        && q.since.is_none_or(|s| t.created_unix >= s)
        && q.min_score.is_none_or(|m| t.score.is_some_and(|s| s >= m))
        && q.max_score.is_none_or(|m| t.score.is_some_and(|s| s <= m))
        && q.scored.is_none_or(|want| t.score.is_some() == want)
}

fn load(state: &AppState, q: &TraceQuery) -> Result<Vec<Trace>, ApiErr> {
    let mut all: Vec<Trace> = state
        .store
        .list_entities(STORE_TRACES)
        .map_err(|e| err(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    all.retain(|t| matches(t, q));
    all.sort_by_key(|t| std::cmp::Reverse(t.created_unix));
    Ok(all)
}

/// GET /api/ai/traces
pub async fn list_traces(
    RequireRead(_c): RequireRead,
    State(state): State<Arc<AppState>>,
    Query(q): Query<TraceQuery>,
) -> Result<Json<Vec<Trace>>, ApiErr> {
    let mut all = load(&state, &q)?;
    all.truncate(q.limit.unwrap_or(100).clamp(1, MAX_LIST));
    Ok(Json(all))
}

/// GET /api/ai/traces/{id}
pub async fn get_trace(
    RequireRead(_c): RequireRead,
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<Trace>, ApiErr> {
    state
        .store
        .get_entity(STORE_TRACES, &id)
        .map_err(|e| err(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
        .ok_or_else(|| err(StatusCode::NOT_FOUND, "trace not found"))
        .map(Json)
}

#[derive(Debug, Deserialize)]
pub struct FeedbackRequest {
    pub score: f64,
    #[serde(default)]
    pub labels: Vec<String>,
}

/// POST /api/ai/traces/{id}/feedback — attach a human or application score.
pub async fn feedback(
    RequireWrite(claims): RequireWrite,
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    Json(req): Json<FeedbackRequest>,
) -> Result<Json<Trace>, ApiErr> {
    if !req.score.is_finite() || !(0.0..=1.0).contains(&req.score) {
        return Err(err(
            StatusCode::BAD_REQUEST,
            "score must be between 0 and 1",
        ));
    }
    if req.labels.len() > 16 || req.labels.iter().any(|l| l.is_empty() || l.len() > 64) {
        return Err(err(
            StatusCode::BAD_REQUEST,
            "at most 16 labels of 1-64 characters",
        ));
    }
    let updated = state
        .store
        .update_entity_exclusive(STORE_TRACES, &id, |mut t: Trace| {
            t.score = Some(req.score);
            t.score_source = Some(format!("feedback:{}", claims.sub));
            for l in &req.labels {
                if !t.labels.contains(l) {
                    t.labels.push(l.clone());
                }
            }
            Ok::<_, String>(t)
        })
        .map_err(|e| err(StatusCode::NOT_FOUND, e.to_string()))?;
    audit(
        &state,
        &claims.sub,
        "FEEDBACK",
        &format!("ai/traces/{id}"),
        "SUCCESS",
    );
    Ok(Json(updated))
}

/// One export line. `sft` gives chat fine-tuning rows, `raw` the full trace.
pub fn export_line(t: &Trace, format: &str) -> Option<String> {
    match format {
        "sft" => {
            let mut messages = t.request.as_ref()?.get("messages")?.as_array()?.clone();
            let completion = t.completion.as_ref()?;
            messages.push(json!({"role": "assistant", "content": completion}));
            serde_json::to_string(&json!({"id": t.id, "score": t.score, "messages": messages})).ok()
        }
        _ => serde_json::to_string(t).ok(),
    }
}

/// GET /api/ai/traces/export — newline-delimited JSON.
pub async fn export_traces(
    RequireRead(claims): RequireRead,
    State(state): State<Arc<AppState>>,
    Query(q): Query<TraceQuery>,
) -> Result<Response, ApiErr> {
    let format = q.format.clone().unwrap_or_else(|| "raw".into());
    if format != "raw" && format != "sft" {
        return Err(err(StatusCode::BAD_REQUEST, "format must be raw or sft"));
    }
    let mut all = load(&state, &q)?;
    all.truncate(q.limit.unwrap_or(10_000).clamp(1, 100_000));
    let mut body = String::new();
    for t in &all {
        if let Some(line) = export_line(t, &format) {
            body.push_str(&line);
            body.push('\n');
        }
    }
    audit(&state, &claims.sub, "EXPORT", "ai/traces", "SUCCESS");
    Ok(([(header::CONTENT_TYPE, "application/x-ndjson")], body).into_response())
}

/// DELETE /api/ai/traces?endpoint=… — purge matching traces.
pub async fn purge_traces(
    RequireWrite(claims): RequireWrite,
    State(state): State<Arc<AppState>>,
    Query(q): Query<TraceQuery>,
) -> Result<Json<Value>, ApiErr> {
    if q.endpoint.is_none() && q.router.is_none() && q.since.is_none() {
        return Err(err(
            StatusCode::BAD_REQUEST,
            "give endpoint, router or since to limit the purge",
        ));
    }
    let victims = load(&state, &q)?;
    let n = victims.len();
    for t in victims {
        let _ = state.store.delete_entity(STORE_TRACES, &t.id);
    }
    audit(&state, &claims.sub, "PURGE", "ai/traces", "SUCCESS");
    Ok(Json(json!({ "deleted": n })))
}

/// GET /api/ai/trace-policies
pub async fn list_policies(
    RequireRead(_c): RequireRead,
    State(state): State<Arc<AppState>>,
) -> Result<Json<Vec<TracePolicy>>, ApiErr> {
    let mut v: Vec<TracePolicy> = state
        .store
        .list_entities(STORE_TRACE_POLICIES)
        .map_err(|e| err(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    v.sort_by(|a, b| a.endpoint.cmp(&b.endpoint));
    Ok(Json(v))
}

/// GET /api/ai/trace-policies/{endpoint}
pub async fn get_policy(
    RequireRead(_c): RequireRead,
    State(state): State<Arc<AppState>>,
    Path(endpoint): Path<String>,
) -> Result<Json<TracePolicy>, ApiErr> {
    state
        .store
        .get_entity(STORE_TRACE_POLICIES, &endpoint)
        .map_err(|e| err(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
        .ok_or_else(|| err(StatusCode::NOT_FOUND, "trace policy not found"))
        .map(Json)
}

/// POST /api/ai/trace-policies — create or replace the policy for an endpoint.
pub async fn put_policy(
    RequireWrite(claims): RequireWrite,
    State(state): State<Arc<AppState>>,
    Json(policy): Json<TracePolicy>,
) -> Result<Json<TracePolicy>, ApiErr> {
    crate::validation::validate_entity_name(&policy.endpoint).map_err(|(s, m)| err(s, m))?;
    policy
        .validate()
        .map_err(|m| err(StatusCode::BAD_REQUEST, m))?;
    if state
        .store
        .get_entity::<super::types::InferenceEndpoint>(super::STORE_ENDPOINTS, &policy.endpoint)
        .map_err(|e| err(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
        .is_none()
    {
        return Err(err(StatusCode::NOT_FOUND, "InferenceEndpoint not found"));
    }
    state
        .store
        .save_entity(STORE_TRACE_POLICIES, &policy.endpoint, &policy)
        .map_err(|e| err(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    audit(
        &state,
        &claims.sub,
        "UPSERT",
        &format!("ai/trace-policies/{}", policy.endpoint),
        "SUCCESS",
    );
    Ok(Json(policy))
}

/// DELETE /api/ai/trace-policies/{endpoint}
pub async fn delete_policy(
    RequireWrite(claims): RequireWrite,
    State(state): State<Arc<AppState>>,
    Path(endpoint): Path<String>,
) -> Result<StatusCode, ApiErr> {
    state
        .store
        .delete_entity(STORE_TRACE_POLICIES, &endpoint)
        .map_err(|e| err(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    audit(
        &state,
        &claims.sub,
        "DELETE",
        &format!("ai/trace-policies/{endpoint}"),
        "SUCCESS",
    );
    Ok(StatusCode::NO_CONTENT)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn policy() -> TracePolicy {
        TracePolicy {
            endpoint: "ep".into(),
            enabled: true,
            sample_rate: 1.0,
            max_body_bytes: 4096,
            redact: false,
            capture_token_ids: true,
            auto_score: None,
            max_traces: 10,
            judge: None,
        }
    }

    fn ctx(policy: TracePolicy) -> TraceCtx {
        TraceCtx {
            id: "t1".into(),
            started: std::time::Instant::now(),
            policy,
            endpoint: "ep".into(),
            router: Some("r".into()),
            variant: Some("a".into()),
            target_endpoint: "ep-a".into(),
            model: "m".into(),
            path: "/v1/chat/completions".into(),
            tenant: None,
            user: Some("u1".into()),
            project: None,
            request_body: br#"{"model":"m","messages":[{"role":"user","content":"hi"}]}"#.to_vec(),
        }
    }

    const CHAT: &str = r#"{"id":"x","prompt_token_ids":[1,2,3],"choices":[{"index":0,
        "message":{"role":"assistant","content":"hello"},"finish_reason":"stop",
        "token_ids":[7,8],"logprobs":{"content":[{"token":"he","logprob":-0.5},{"token":"llo","logprob":-0.25}]}}],
        "usage":{"prompt_tokens":3,"completion_tokens":2,"total_tokens":5}}"#;

    #[test]
    fn parses_ids_logprobs_and_usage() {
        let p = parse_response(CHAT.as_bytes());
        assert_eq!(p.completion, "hello");
        assert_eq!(p.finish_reason.as_deref(), Some("stop"));
        assert_eq!(p.prompt_token_ids, Some(vec![1, 2, 3]));
        assert_eq!(p.completion_token_ids, Some(vec![7, 8]));
        assert_eq!(p.logprobs, Some(vec![-0.5, -0.25]));
        assert_eq!((p.prompt_tokens, p.completion_tokens), (3, 2));
    }

    #[test]
    fn legacy_completion_logprobs_shape() {
        let body = r#"{"choices":[{"text":"ab","logprobs":{"token_logprobs":[-1.0,-2.0]}}]}"#;
        let p = parse_response(body.as_bytes());
        assert_eq!(p.completion, "ab");
        assert_eq!(p.logprobs, Some(vec![-1.0, -2.0]));
    }

    #[test]
    fn garbage_body_parses_empty() {
        assert_eq!(parse_response(b"not json"), Parsed::default());
    }

    #[test]
    fn sse_chunks_are_joined() {
        let sse = "data: {\"choices\":[{\"delta\":{\"content\":\"Hel\"},\"token_ids\":[1]}]}\n\n\
                   data: {\"choices\":[{\"delta\":{\"content\":\"lo\"},\"token_ids\":[2],\"finish_reason\":\"stop\"}],\
                   \"usage\":{\"prompt_tokens\":4,\"completion_tokens\":2}}\n\n\
                   data: [DONE]\n\n";
        let p = parse_sse(sse.as_bytes());
        assert_eq!(p.completion, "Hello");
        assert_eq!(p.completion_token_ids, Some(vec![1, 2]));
        assert_eq!(p.finish_reason.as_deref(), Some("stop"));
        assert_eq!((p.prompt_tokens, p.completion_tokens), (4, 2));
    }

    #[test]
    fn inject_adds_fields_but_keeps_client_choices() {
        let out: Value = serde_json::from_slice(&inject_capture_params(
            br#"{"messages":[],"logprobs":false}"#,
        ))
        .unwrap();
        assert_eq!(out["return_token_ids"], json!(true));
        assert_eq!(out["logprobs"], json!(false));
        assert_eq!(inject_capture_params(b"plain"), b"plain".to_vec());
        let s: Value =
            serde_json::from_slice(&inject_capture_params(br#"{"messages":[],"stream":true}"#))
                .unwrap();
        assert_eq!(s["stream_options"]["include_usage"], json!(true));
    }

    #[test]
    fn scorers() {
        let mut p = parse_response(CHAT.as_bytes());
        assert_eq!(auto_score(&AutoScore::NonEmpty, 200, &p), 1.0);
        assert_eq!(auto_score(&AutoScore::FinishStop, 200, &p), 1.0);
        assert_eq!(auto_score(&AutoScore::JsonValid, 200, &p), 0.0);
        assert_eq!(
            auto_score(
                &AutoScore::Contains {
                    needle: "ell".into()
                },
                200,
                &p
            ),
            1.0
        );
        assert_eq!(auto_score(&AutoScore::NonEmpty, 500, &p), 0.0);
        p.completion = r#"{"a":1}"#.into();
        assert_eq!(auto_score(&AutoScore::JsonValid, 200, &p), 1.0);
    }

    #[test]
    fn build_trace_keeps_request_and_ids() {
        let t = build_trace(&ctx(policy()), 200, false, CHAT.as_bytes());
        assert_eq!(t.variant.as_deref(), Some("a"));
        assert_eq!(t.completion.as_deref(), Some("hello"));
        assert!(t.request.is_some());
        assert_eq!(t.completion_token_ids, Some(vec![7, 8]));
        assert_eq!(t.request_sha256.len(), 64);
    }

    #[test]
    fn redaction_drops_content_but_keeps_hash() {
        let mut p = policy();
        p.redact = true;
        let t = build_trace(&ctx(p), 200, false, CHAT.as_bytes());
        assert!(t.request.is_none() && t.completion.is_none());
        assert!(t.prompt_token_ids.is_none() && t.logprobs.is_none());
        assert_eq!(t.request_sha256.len(), 64);
        assert_eq!(t.prompt_tokens, 3);
    }

    #[test]
    fn oversized_bodies_are_not_stored() {
        let mut p = policy();
        p.max_body_bytes = 4;
        let t = build_trace(&ctx(p), 200, false, CHAT.as_bytes());
        assert!(t.request.is_none() && t.completion.is_none());
    }

    #[test]
    fn sampling() {
        assert!(sampled(1.0, 0.99));
        assert!(!sampled(0.0, 0.0));
        assert!(sampled(0.5, 0.1));
        assert!(!sampled(0.5, 0.9));
    }

    #[test]
    fn policy_validation() {
        let mut p = policy();
        assert!(p.validate().is_ok());
        p.sample_rate = 1.5;
        assert!(p.validate().is_err());
        p.sample_rate = f64::NAN;
        assert!(p.validate().is_err());
        p.sample_rate = 0.5;
        p.max_body_bytes = 0;
        assert!(p.validate().is_err());
    }

    #[test]
    fn query_filters() {
        let t = build_trace(&ctx(policy()), 200, false, CHAT.as_bytes());
        let q = |f: fn(&mut TraceQuery)| {
            let mut q = TraceQuery::default();
            f(&mut q);
            matches(&t, &q)
        };
        assert!(q(|_| {}));
        assert!(q(|q| q.variant = Some("a".into())));
        assert!(!q(|q| q.variant = Some("b".into())));
        assert!(
            !q(|q| q.min_score = Some(0.5)),
            "unscored trace has no score to match"
        );
        assert!(q(|q| q.scored = Some(false)));
    }

    #[test]
    fn sft_export_appends_assistant_turn() {
        let t = build_trace(&ctx(policy()), 200, false, CHAT.as_bytes());
        let line = export_line(&t, "sft").unwrap();
        let v: Value = serde_json::from_str(&line).unwrap();
        let msgs = v["messages"].as_array().unwrap();
        assert_eq!(msgs.len(), 2);
        assert_eq!(msgs[1]["content"], "hello");
        let raw: Value = serde_json::from_str(&export_line(&t, "raw").unwrap()).unwrap();
        assert_eq!(raw["completion_token_ids"], json!([7, 8]));
    }
}
