// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: LicenseRef-Zyvor-Production-1.0
//!
//! Decision router and A/B experiments (Beta).
//!
//! An [`AiRouter`] sits behind a front-door `InferenceEndpoint`. Clients keep
//! one base URL and one API key; for each request the router picks which of
//! its candidate endpoints serves it:
//!
//! * `rules`  – first matching rule wins (prompt size, tools, keywords, header, model).
//! * `cost`   – cheapest candidate that clears the quality bar. Hard requests
//!              (long prompt or tools) raise the bar.
//! * `split`  – weighted A/B split, sticky per user or session.
//!
//! Every candidate is an A/B arm: its name is stamped on the trace, and
//! [`report`] compares arms on score, latency, errors and cost with a Welch
//! z-test against the control arm.

use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    Json,
};
use chrono::Utc;
use security::{RequireRead, RequireWrite};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::sync::Arc;

use crate::server::AppState;

use super::traces::{Trace, STORE_TRACES};
use super::{audit, err, STORE_ENDPOINTS};

pub(crate) const STORE_ROUTERS: &str = "ai_routers";

type ApiErr = (StatusCode, Json<Value>);

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RouteStrategy {
    Rules,
    Cost,
    Split,
}

/// One arm: an existing `InferenceEndpoint` plus what it costs and how good it is.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Candidate {
    /// Arm label recorded on traces.
    pub name: String,
    /// `InferenceEndpoint` that serves this arm.
    pub endpoint: String,
    /// Share of traffic for `split`.
    #[serde(default = "default_weight")]
    pub weight: u32,
    #[serde(default)]
    pub cost_per_mtok_in: f64,
    #[serde(default)]
    pub cost_per_mtok_out: f64,
    /// Operator's quality estimate, 0.0..=1.0 (used by `cost`).
    #[serde(default = "default_quality")]
    pub quality: f64,
    /// Longest prompt (tokens) this arm accepts.
    #[serde(default)]
    pub max_context: Option<u64>,
}

fn default_weight() -> u32 {
    1
}
fn default_quality() -> f64 {
    0.5
}

/// Conditions are ANDed; an empty condition matches everything.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct RouteMatch {
    #[serde(default)]
    pub min_prompt_tokens: Option<u64>,
    #[serde(default)]
    pub max_prompt_tokens: Option<u64>,
    /// Any of these (case-insensitive) in the last user message.
    #[serde(default)]
    pub contains_any: Vec<String>,
    #[serde(default)]
    pub has_tools: Option<bool>,
    /// Request `model` field equals this.
    #[serde(default)]
    pub model: Option<String>,
    /// `[name, value]` request header, name case-insensitive.
    #[serde(default)]
    pub header: Option<(String, String)>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RouteRule {
    #[serde(default)]
    pub when: RouteMatch,
    /// Candidate name.
    pub to: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AiRouter {
    /// Front-door `InferenceEndpoint` this router serves (also the key).
    pub endpoint: String,
    pub strategy: RouteStrategy,
    pub candidates: Vec<Candidate>,
    #[serde(default)]
    pub rules: Vec<RouteRule>,
    /// Fallback candidate for `rules`. Defaults to the first candidate.
    #[serde(default)]
    pub default: Option<String>,
    /// A/B baseline for [`report`]. Defaults to the first candidate.
    #[serde(default)]
    pub control: Option<String>,
    /// Request header whose value keeps a caller on one arm.
    #[serde(default = "default_sticky")]
    pub sticky_header: String,
    /// `cost`: minimum quality.
    #[serde(default)]
    pub min_quality: f64,
    /// `cost`: minimum quality for hard requests.
    #[serde(default)]
    pub hard_quality: Option<f64>,
    /// Prompt tokens at which a request counts as hard.
    #[serde(default = "default_hard_tokens")]
    pub hard_prompt_tokens: u64,
    /// Let callers force an arm with `x-fabric-variant`.
    #[serde(default)]
    pub allow_override: bool,
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default)]
    pub updated_unix: i64,
}

fn default_sticky() -> String {
    "x-user-id".into()
}
fn default_hard_tokens() -> u64 {
    2000
}
fn default_true() -> bool {
    true
}

impl AiRouter {
    pub fn validate(&self) -> Result<(), String> {
        if self.candidates.is_empty() || self.candidates.len() > 16 {
            return Err("1 to 16 candidates are required".into());
        }
        let mut seen = std::collections::HashSet::new();
        for c in &self.candidates {
            if c.name.is_empty() || c.name.len() > 64 || !seen.insert(c.name.as_str()) {
                return Err(format!("candidate name '{}' is empty, too long or repeated", c.name));
            }
            if !(0.0..=1.0).contains(&c.quality) || c.quality.is_nan() {
                return Err(format!("candidate '{}': quality must be between 0 and 1", c.name));
            }
            for p in [c.cost_per_mtok_in, c.cost_per_mtok_out] {
                if !p.is_finite() || p < 0.0 {
                    return Err(format!("candidate '{}': prices must be zero or more", c.name));
                }
            }
        }
        let known = |n: &str| self.candidates.iter().any(|c| c.name == n);
        for n in self.default.iter().chain(self.control.iter()) {
            if !known(n) {
                return Err(format!("'{n}' is not a candidate"));
            }
        }
        for r in &self.rules {
            if !known(&r.to) {
                return Err(format!("rule target '{}' is not a candidate", r.to));
            }
        }
        if self.strategy == RouteStrategy::Split && self.candidates.iter().all(|c| c.weight == 0) {
            return Err("split needs at least one candidate with weight above 0".into());
        }
        if self.strategy == RouteStrategy::Rules && self.rules.is_empty() && self.default.is_none() {
            return Err("rules strategy needs rules or a default".into());
        }
        for q in [self.min_quality].into_iter().chain(self.hard_quality) {
            if !(0.0..=1.0).contains(&q) || q.is_nan() {
                return Err("quality bars must be between 0 and 1".into());
            }
        }
        if self.sticky_header.is_empty() || self.sticky_header.len() > 64 {
            return Err("sticky_header must be 1-64 characters".into());
        }
        Ok(())
    }

    pub fn control_name(&self) -> &str {
        self.control
            .as_deref()
            .unwrap_or_else(|| self.candidates[0].name.as_str())
    }
}

/// What the router looks at.
#[derive(Debug, Default, Clone)]
pub struct RouteInput {
    pub prompt_tokens: u64,
    pub max_out_tokens: u64,
    pub last_user: String,
    pub has_tools: bool,
    pub model: Option<String>,
    /// Lower-cased header name → value.
    pub headers: Vec<(String, String)>,
}

impl RouteInput {
    pub fn from_request(body: &[u8], headers: &[(String, String)]) -> Self {
        let mut input = RouteInput {
            prompt_tokens: (body.len() as u64 / 4).max(1),
            max_out_tokens: 256,
            headers: headers
                .iter()
                .map(|(k, v)| (k.to_ascii_lowercase(), v.clone()))
                .collect(),
            ..Default::default()
        };
        let Ok(v) = serde_json::from_slice::<Value>(body) else {
            return input;
        };
        input.model = v.get("model").and_then(Value::as_str).map(str::to_string);
        input.has_tools = v
            .get("tools")
            .and_then(Value::as_array)
            .is_some_and(|t| !t.is_empty());
        if let Some(n) = v
            .get("max_tokens")
            .or_else(|| v.get("max_completion_tokens"))
            .and_then(Value::as_u64)
        {
            input.max_out_tokens = n;
        }
        let mut chars = 0usize;
        if let Some(msgs) = v.get("messages").and_then(Value::as_array) {
            for m in msgs {
                let text = message_text(m);
                chars += text.len();
                if m.get("role").and_then(Value::as_str) == Some("user") {
                    input.last_user = text;
                }
            }
        } else if let Some(p) = v.get("prompt").and_then(Value::as_str) {
            chars = p.len();
            input.last_user = p.to_string();
        }
        if chars > 0 {
            input.prompt_tokens = (chars as u64 / 4).max(1);
        }
        input
    }

    fn header(&self, name: &str) -> Option<&str> {
        let name = name.to_ascii_lowercase();
        self.headers
            .iter()
            .find(|(k, _)| *k == name)
            .map(|(_, v)| v.as_str())
    }
}

fn message_text(m: &Value) -> String {
    match m.get("content") {
        Some(Value::String(s)) => s.clone(),
        Some(Value::Array(parts)) => parts
            .iter()
            .filter_map(|p| p.get("text").and_then(Value::as_str))
            .collect::<Vec<_>>()
            .join("\n"),
        _ => String::new(),
    }
}

fn rule_matches(m: &RouteMatch, i: &RouteInput) -> bool {
    if m.min_prompt_tokens.is_some_and(|n| i.prompt_tokens < n) {
        return false;
    }
    if m.max_prompt_tokens.is_some_and(|n| i.prompt_tokens > n) {
        return false;
    }
    if m.has_tools.is_some_and(|t| t != i.has_tools) {
        return false;
    }
    if m.model.as_ref().is_some_and(|want| i.model.as_ref() != Some(want)) {
        return false;
    }
    if let Some((name, want)) = &m.header {
        if i.header(name) != Some(want.as_str()) {
            return false;
        }
    }
    if !m.contains_any.is_empty() {
        let hay = i.last_user.to_lowercase();
        if !m.contains_any.iter().any(|k| hay.contains(&k.to_lowercase())) {
            return false;
        }
    }
    true
}

fn expected_cost(c: &Candidate, i: &RouteInput) -> f64 {
    (c.cost_per_mtok_in * i.prompt_tokens as f64 + c.cost_per_mtok_out * i.max_out_tokens as f64)
        / 1_000_000.0
}

fn fits(c: &Candidate, i: &RouteInput) -> bool {
    c.max_context.is_none_or(|m| i.prompt_tokens <= m)
}

fn sticky_roll(key: &str, total: u64) -> u64 {
    let d = Sha256::digest(key.as_bytes());
    u64::from_be_bytes(d[..8].try_into().expect("8 bytes")) % total.max(1)
}

/// The router's choice and the arms to try, in order, if it is unavailable.
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct Decision {
    /// Candidate indexes, best first.
    pub order: Vec<usize>,
    pub reason: String,
}

/// Rank the candidates for one request. `roll` is only used by unsticky splits.
pub fn decide(r: &AiRouter, i: &RouteInput, roll: u64) -> Decision {
    if r.allow_override {
        if let Some(want) = i.header("x-fabric-variant") {
            if let Some(idx) = r.candidates.iter().position(|c| c.name == want) {
                return finish(r, idx, format!("override: x-fabric-variant={want}"));
            }
        }
    }
    let (primary, reason) = match r.strategy {
        RouteStrategy::Rules => {
            let hit = r.rules.iter().enumerate().find(|(_, rule)| rule_matches(&rule.when, i));
            match hit {
                Some((n, rule)) => (index_of(r, &rule.to), format!("rule {n} -> {}", rule.to)),
                None => {
                    let d = r.default.as_deref().unwrap_or(&r.candidates[0].name);
                    (index_of(r, d), format!("default -> {d}"))
                }
            }
        }
        RouteStrategy::Cost => {
            let hard = i.has_tools || i.prompt_tokens >= r.hard_prompt_tokens;
            let bar = if hard {
                r.hard_quality.unwrap_or(r.min_quality).max(r.min_quality)
            } else {
                r.min_quality
            };
            let best = r
                .candidates
                .iter()
                .enumerate()
                .filter(|(_, c)| c.quality >= bar && fits(c, i))
                .min_by(|(_, a), (_, b)| {
                    expected_cost(a, i)
                        .total_cmp(&expected_cost(b, i))
                        .then(b.quality.total_cmp(&a.quality))
                })
                .map(|(n, _)| n);
            match best {
                Some(n) => (
                    n,
                    format!(
                        "cheapest with quality >= {bar:.2}{}",
                        if hard { " (hard request)" } else { "" }
                    ),
                ),
                None => {
                    let n = best_quality(r, i);
                    (n, format!("no arm clears quality {bar:.2}; best quality"))
                }
            }
        }
        RouteStrategy::Split => {
            let total: u64 = r.candidates.iter().map(|c| u64::from(c.weight)).sum();
            let roll = match i.header(&r.sticky_header) {
                Some(key) => sticky_roll(key, total),
                None => roll % total.max(1),
            };
            let mut acc = 0u64;
            let mut pick = 0usize;
            for (n, c) in r.candidates.iter().enumerate() {
                acc += u64::from(c.weight);
                if roll < acc {
                    pick = n;
                    break;
                }
            }
            (pick, format!("split {}/{}", r.candidates[pick].weight, total))
        }
    };
    finish(r, primary, reason)
}

fn index_of(r: &AiRouter, name: &str) -> usize {
    r.candidates.iter().position(|c| c.name == name).unwrap_or(0)
}

fn best_quality(r: &AiRouter, i: &RouteInput) -> usize {
    r.candidates
        .iter()
        .enumerate()
        .filter(|(_, c)| fits(c, i))
        .max_by(|(_, a), (_, b)| a.quality.total_cmp(&b.quality))
        .map(|(n, _)| n)
        .unwrap_or(0)
}

fn finish(r: &AiRouter, primary: usize, reason: String) -> Decision {
    let mut rest: Vec<usize> = (0..r.candidates.len()).filter(|n| *n != primary).collect();
    rest.sort_by(|a, b| {
        r.candidates[*b]
            .quality
            .total_cmp(&r.candidates[*a].quality)
            .then(a.cmp(b))
    });
    let mut order = vec![primary];
    order.extend(rest);
    Decision { order, reason }
}

// ---- A/B report ------------------------------------------------------------

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct Comparison {
    pub delta: f64,
    pub z: f64,
    pub p_value: f64,
    pub significant: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct VariantStats {
    pub variant: String,
    pub requests: u64,
    pub errors: u64,
    pub error_rate: f64,
    pub scored: u64,
    pub mean_score: Option<f64>,
    pub latency_p50_ms: u64,
    pub latency_p95_ms: u64,
    pub prompt_tokens: u64,
    pub completion_tokens: u64,
    pub est_cost_usd: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub vs_control: Option<Comparison>,
}

#[derive(Debug, Clone, Serialize)]
pub struct Report {
    pub router: String,
    pub control: String,
    pub min_samples: u64,
    pub variants: Vec<VariantStats>,
    pub recommendation: String,
}

fn percentile(sorted: &[u64], p: f64) -> u64 {
    if sorted.is_empty() {
        return 0;
    }
    let rank = ((sorted.len() as f64) * p).ceil() as usize;
    sorted[rank.clamp(1, sorted.len()) - 1]
}

fn mean_var(xs: &[f64]) -> (f64, f64) {
    let n = xs.len() as f64;
    let mean = xs.iter().sum::<f64>() / n;
    let var = if xs.len() > 1 {
        xs.iter().map(|x| (x - mean).powi(2)).sum::<f64>() / (n - 1.0)
    } else {
        0.0
    };
    (mean, var)
}

/// Standard normal CDF (Abramowitz & Stegun 7.1.26, error < 1.5e-7).
pub fn normal_cdf(x: f64) -> f64 {
    let t = 1.0 / (1.0 + 0.327_591_1 * (x.abs() / std::f64::consts::SQRT_2));
    let poly = t
        * (0.254_829_592
            + t * (-0.284_496_736
                + t * (1.421_413_741 + t * (-1.453_152_027 + t * 1.061_405_429))));
    let erf = 1.0 - poly * (-(x * x) / 2.0).exp();
    if x >= 0.0 {
        0.5 * (1.0 + erf)
    } else {
        0.5 * (1.0 - erf)
    }
}

/// Welch z-test of `b` against baseline `a` (normal approximation; use
/// `min_samples` of 30 or more per arm).
pub fn compare(a: &[f64], b: &[f64], alpha: f64) -> Option<Comparison> {
    if a.len() < 2 || b.len() < 2 {
        return None;
    }
    let (ma, va) = mean_var(a);
    let (mb, vb) = mean_var(b);
    let delta = mb - ma;
    let se = (va / a.len() as f64 + vb / b.len() as f64).sqrt();
    let (z, p) = if se == 0.0 {
        if delta == 0.0 {
            (0.0, 1.0)
        } else {
            (delta.signum() * f64::INFINITY, 0.0)
        }
    } else {
        let z = delta / se;
        (z, 2.0 * (1.0 - normal_cdf(z.abs())))
    };
    Some(Comparison {
        delta,
        z: if z.is_finite() { z } else { 0.0 },
        p_value: p,
        significant: p < alpha,
    })
}

pub fn build_report(r: &AiRouter, traces: &[Trace], min_samples: u64) -> Report {
    let control = r.control_name().to_string();
    let mut by: BTreeMap<&str, Vec<&Trace>> = BTreeMap::new();
    for c in &r.candidates {
        by.entry(c.name.as_str()).or_default();
    }
    for t in traces {
        if let Some(v) = t.variant.as_deref() {
            if let Some(list) = by.get_mut(v) {
                list.push(t);
            }
        }
    }
    let scores = |ts: &[&Trace]| -> Vec<f64> { ts.iter().filter_map(|t| t.score).collect() };
    let control_scores = scores(by.get(control.as_str()).map(Vec::as_slice).unwrap_or(&[]));

    let mut variants = Vec::new();
    for c in &r.candidates {
        let ts = &by[c.name.as_str()];
        let sc = scores(ts);
        let mut lat: Vec<u64> = ts.iter().map(|t| t.latency_ms).collect();
        lat.sort_unstable();
        let errors = ts.iter().filter(|t| !(200..300).contains(&t.status)).count() as u64;
        let (pt, ct) = ts.iter().fold((0u64, 0u64), |(p, c), t| {
            (p + t.prompt_tokens, c + t.completion_tokens)
        });
        let vs_control = if c.name == control || (sc.len() as u64) < min_samples
            || (control_scores.len() as u64) < min_samples
        {
            None
        } else {
            compare(&control_scores, &sc, 0.05)
        };
        variants.push(VariantStats {
            variant: c.name.clone(),
            requests: ts.len() as u64,
            errors,
            error_rate: if ts.is_empty() { 0.0 } else { errors as f64 / ts.len() as f64 },
            scored: sc.len() as u64,
            mean_score: (!sc.is_empty()).then(|| mean_var(&sc).0),
            latency_p50_ms: percentile(&lat, 0.5),
            latency_p95_ms: percentile(&lat, 0.95),
            prompt_tokens: pt,
            completion_tokens: ct,
            est_cost_usd: (c.cost_per_mtok_in * pt as f64 + c.cost_per_mtok_out * ct as f64)
                / 1_000_000.0,
            vs_control,
        });
    }
    let recommendation = recommend(&variants, &control);
    Report {
        router: r.endpoint.clone(),
        control,
        min_samples,
        variants,
        recommendation,
    }
}

fn recommend(variants: &[VariantStats], control: &str) -> String {
    let judged: Vec<&VariantStats> = variants.iter().filter(|v| v.vs_control.is_some()).collect();
    if judged.is_empty() {
        return "keep collecting: no arm has enough scored traces yet".into();
    }
    let best = judged
        .iter()
        .filter(|v| {
            let c = v.vs_control.as_ref().expect("filtered");
            c.significant && c.delta > 0.0
        })
        .max_by(|a, b| {
            let da = a.vs_control.as_ref().map_or(0.0, |c| c.delta);
            let db = b.vs_control.as_ref().map_or(0.0, |c| c.delta);
            da.total_cmp(&db)
        });
    match best {
        Some(v) => format!("promote '{}': significantly better than '{control}'", v.variant),
        None if judged.iter().all(|v| v.vs_control.as_ref().is_some_and(|c| c.significant && c.delta < 0.0)) => {
            format!("keep '{control}': every challenger scores significantly lower")
        }
        None => "keep collecting: no significant difference yet".into(),
    }
}

// ---- REST ------------------------------------------------------------------

fn load_router(state: &AppState, endpoint: &str) -> Result<AiRouter, ApiErr> {
    state
        .store
        .get_entity(STORE_ROUTERS, endpoint)
        .map_err(|e| err(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
        .ok_or_else(|| err(StatusCode::NOT_FOUND, "router not found"))
}

/// Router attached to a front-door endpoint, when enabled.
pub fn router_for(state: &AppState, endpoint: &str) -> Option<AiRouter> {
    let r: AiRouter = state.store.get_entity(STORE_ROUTERS, endpoint).ok().flatten()?;
    r.enabled.then_some(r)
}

/// GET /api/ai/routers
pub async fn list_routers(
    RequireRead(_c): RequireRead,
    State(state): State<Arc<AppState>>,
) -> Result<Json<Vec<AiRouter>>, ApiErr> {
    let mut v: Vec<AiRouter> = state
        .store
        .list_entities(STORE_ROUTERS)
        .map_err(|e| err(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    v.sort_by(|a, b| a.endpoint.cmp(&b.endpoint));
    Ok(Json(v))
}

/// GET /api/ai/routers/{endpoint}
pub async fn get_router(
    RequireRead(_c): RequireRead,
    State(state): State<Arc<AppState>>,
    Path(endpoint): Path<String>,
) -> Result<Json<AiRouter>, ApiErr> {
    load_router(&state, &endpoint).map(Json)
}

/// POST /api/ai/routers — create or replace.
pub async fn put_router(
    RequireWrite(claims): RequireWrite,
    State(state): State<Arc<AppState>>,
    Json(mut router): Json<AiRouter>,
) -> Result<Json<AiRouter>, ApiErr> {
    crate::validation::validate_entity_name(&router.endpoint).map_err(|(s, m)| err(s, m))?;
    router.validate().map_err(|m| err(StatusCode::BAD_REQUEST, m))?;
    let names = std::iter::once(router.endpoint.as_str())
        .chain(router.candidates.iter().map(|c| c.endpoint.as_str()));
    for name in names {
        let found = state
            .store
            .get_entity::<super::types::InferenceEndpoint>(STORE_ENDPOINTS, name)
            .map_err(|e| err(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
        if found.is_none() {
            return Err(err(
                StatusCode::NOT_FOUND,
                format!("InferenceEndpoint '{name}' not found"),
            ));
        }
    }
    router.updated_unix = Utc::now().timestamp();
    state
        .store
        .save_entity(STORE_ROUTERS, &router.endpoint, &router)
        .map_err(|e| err(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    audit(&state, &claims.sub, "UPSERT", &format!("ai/routers/{}", router.endpoint), "SUCCESS");
    Ok(Json(router))
}

/// DELETE /api/ai/routers/{endpoint}
pub async fn delete_router(
    RequireWrite(claims): RequireWrite,
    State(state): State<Arc<AppState>>,
    Path(endpoint): Path<String>,
) -> Result<StatusCode, ApiErr> {
    load_router(&state, &endpoint)?;
    state
        .store
        .delete_entity(STORE_ROUTERS, &endpoint)
        .map_err(|e| err(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    audit(&state, &claims.sub, "DELETE", &format!("ai/routers/{endpoint}"), "SUCCESS");
    Ok(StatusCode::NO_CONTENT)
}

/// POST /api/ai/routers/{endpoint}/decide — dry run: which arm would this request take?
pub async fn explain_decision(
    RequireRead(_c): RequireRead,
    State(state): State<Arc<AppState>>,
    Path(endpoint): Path<String>,
    headers: axum::http::HeaderMap,
    body: axum::body::Bytes,
) -> Result<Json<Value>, ApiErr> {
    let r = load_router(&state, &endpoint)?;
    let hs = header_pairs(&headers);
    let input = RouteInput::from_request(&body, &hs);
    let d = decide(&r, &input, rand::random::<u64>());
    let order: Vec<&str> = d.order.iter().map(|n| r.candidates[*n].name.as_str()).collect();
    Ok(Json(json!({
        "chosen": order[0],
        "order": order,
        "reason": d.reason,
        "prompt_tokens_estimate": input.prompt_tokens,
        "hard": input.has_tools || input.prompt_tokens >= r.hard_prompt_tokens,
    })))
}

pub fn header_pairs(h: &axum::http::HeaderMap) -> Vec<(String, String)> {
    h.iter()
        .filter_map(|(k, v)| v.to_str().ok().map(|v| (k.as_str().to_string(), v.to_string())))
        .collect()
}

#[derive(Debug, Default, Deserialize)]
pub struct ReportQuery {
    pub since: Option<i64>,
    pub min_samples: Option<u64>,
}

/// GET /api/ai/routers/{endpoint}/report
pub async fn report(
    RequireRead(_c): RequireRead,
    State(state): State<Arc<AppState>>,
    Path(endpoint): Path<String>,
    Query(q): Query<ReportQuery>,
) -> Result<Json<Report>, ApiErr> {
    let r = load_router(&state, &endpoint)?;
    let traces: Vec<Trace> = state
        .store
        .list_entities_filtered(
            STORE_TRACES,
            |t: &Trace| {
                t.router.as_deref() == Some(endpoint.as_str())
                    && q.since.is_none_or(|s| t.created_unix >= s)
            },
            1_000_000,
        )
        .map_err(|e| err(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    Ok(Json(build_report(&r, &traces, q.min_samples.unwrap_or(30).max(2))))
}

#[derive(Debug, Deserialize)]
pub struct PromoteRequest {
    pub variant: String,
}

/// POST /api/ai/routers/{endpoint}/promote — send all `split` traffic to one arm.
pub async fn promote(
    RequireWrite(claims): RequireWrite,
    State(state): State<Arc<AppState>>,
    Path(endpoint): Path<String>,
    Json(req): Json<PromoteRequest>,
) -> Result<Json<AiRouter>, ApiErr> {
    let mut r = load_router(&state, &endpoint)?;
    if !r.candidates.iter().any(|c| c.name == req.variant) {
        return Err(err(StatusCode::BAD_REQUEST, "variant is not a candidate"));
    }
    r.strategy = RouteStrategy::Split;
    for c in &mut r.candidates {
        c.weight = u32::from(c.name == req.variant);
    }
    r.control = Some(req.variant.clone());
    r.updated_unix = Utc::now().timestamp();
    state
        .store
        .save_entity(STORE_ROUTERS, &r.endpoint, &r)
        .map_err(|e| err(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    audit(
        &state,
        &claims.sub,
        "PROMOTE",
        &format!("ai/routers/{endpoint}/{}", req.variant),
        "SUCCESS",
    );
    Ok(Json(r))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cand(name: &str, weight: u32, quality: f64, cin: f64, cout: f64) -> Candidate {
        Candidate {
            name: name.into(),
            endpoint: format!("ep-{name}"),
            weight,
            cost_per_mtok_in: cin,
            cost_per_mtok_out: cout,
            quality,
            max_context: None,
        }
    }

    fn router(strategy: RouteStrategy) -> AiRouter {
        AiRouter {
            endpoint: "front".into(),
            strategy,
            candidates: vec![
                cand("big", 1, 0.9, 5.0, 15.0),
                cand("small", 1, 0.6, 0.2, 0.6),
            ],
            rules: vec![],
            default: None,
            control: None,
            sticky_header: "x-user-id".into(),
            min_quality: 0.5,
            hard_quality: Some(0.8),
            hard_prompt_tokens: 2000,
            allow_override: false,
            enabled: true,
            updated_unix: 0,
        }
    }

    fn chat(text: &str) -> Vec<u8> {
        serde_json::to_vec(&json!({"model":"m","messages":[{"role":"user","content":text}]})).unwrap()
    }

    fn input(text: &str) -> RouteInput {
        RouteInput::from_request(&chat(text), &[])
    }

    #[test]
    fn cost_picks_cheapest_that_clears_the_bar() {
        let d = decide(&router(RouteStrategy::Cost), &input("hi"), 0);
        assert_eq!(d.order, vec![1, 0]);
    }

    #[test]
    fn cost_raises_the_bar_for_hard_requests() {
        let r = router(RouteStrategy::Cost);
        let long = "x".repeat(9000);
        assert_eq!(decide(&r, &input(&long), 0).order[0], 0);
        let tools = serde_json::to_vec(&json!({"messages":[],"tools":[{"type":"function"}]})).unwrap();
        let d = decide(&r, &RouteInput::from_request(&tools, &[]), 0);
        assert_eq!(d.order[0], 0);
        assert!(d.reason.contains("hard"));
    }

    #[test]
    fn cost_falls_back_to_best_quality_when_none_clear() {
        let mut r = router(RouteStrategy::Cost);
        r.min_quality = 0.95;
        assert_eq!(decide(&r, &input("hi"), 0).order[0], 0);
    }

    #[test]
    fn cost_respects_max_context() {
        let mut r = router(RouteStrategy::Cost);
        r.candidates[1].max_context = Some(10);
        let long = "x".repeat(400);
        assert_eq!(decide(&r, &input(&long), 0).order[0], 0);
    }

    #[test]
    fn rules_first_match_then_default() {
        let mut r = router(RouteStrategy::Rules);
        r.rules = vec![
            RouteRule {
                when: RouteMatch { contains_any: vec!["REFUND".into()], ..Default::default() },
                to: "big".into(),
            },
            RouteRule {
                when: RouteMatch { max_prompt_tokens: Some(50), ..Default::default() },
                to: "small".into(),
            },
        ];
        r.default = Some("big".into());
        assert_eq!(decide(&r, &input("I want a refund"), 0).order[0], 0);
        assert_eq!(decide(&r, &input("hello"), 0).order[0], 1);
        assert_eq!(decide(&r, &input(&"y".repeat(1000)), 0).order[0], 0);
    }

    #[test]
    fn rules_match_headers_model_and_tools() {
        let mut r = router(RouteStrategy::Rules);
        r.rules = vec![RouteRule {
            when: RouteMatch {
                header: Some(("X-Tier".into(), "gold".into())),
                model: Some("m".into()),
                ..Default::default()
            },
            to: "big".into(),
        }];
        r.default = Some("small".into());
        let gold = RouteInput::from_request(&chat("hi"), &[("x-tier".into(), "gold".into())]);
        assert_eq!(decide(&r, &gold, 0).order[0], 0);
        assert_eq!(decide(&r, &input("hi"), 0).order[0], 1);
    }

    #[test]
    fn split_follows_weights_and_is_sticky() {
        let mut r = router(RouteStrategy::Split);
        r.candidates[0].weight = 1;
        r.candidates[1].weight = 3;
        assert_eq!(decide(&r, &input("x"), 0).order[0], 0);
        assert_eq!(decide(&r, &input("x"), 1).order[0], 1);
        assert_eq!(decide(&r, &input("x"), 5).order[0], 1);
        let a = RouteInput::from_request(&chat("x"), &[("x-user-id".into(), "alice".into())]);
        let first = decide(&r, &a, 0).order[0];
        for roll in 0..50 {
            assert_eq!(decide(&r, &a, roll).order[0], first, "sticky per user");
        }
    }

    #[test]
    fn split_spreads_users_roughly_by_weight() {
        let mut r = router(RouteStrategy::Split);
        r.candidates[0].weight = 1;
        r.candidates[1].weight = 1;
        let mut zero = 0;
        for n in 0..1000 {
            let i = RouteInput::from_request(&chat("x"), &[("x-user-id".into(), format!("user{n}"))]);
            if decide(&r, &i, 0).order[0] == 0 {
                zero += 1;
            }
        }
        assert!((400..=600).contains(&zero), "got {zero}");
    }

    #[test]
    fn zero_weight_arm_never_primary_but_is_a_fallback() {
        let mut r = router(RouteStrategy::Split);
        r.candidates[0].weight = 0;
        for roll in 0..20 {
            let d = decide(&r, &input("x"), roll);
            assert_eq!(d.order, vec![1, 0]);
        }
    }

    #[test]
    fn override_header_only_when_allowed() {
        let mut r = router(RouteStrategy::Cost);
        let i = RouteInput::from_request(&chat("hi"), &[("X-Fabric-Variant".into(), "big".into())]);
        assert_eq!(decide(&r, &i, 0).order[0], 1);
        r.allow_override = true;
        assert_eq!(decide(&r, &i, 0).order[0], 0);
    }

    #[test]
    fn input_parsing_handles_parts_and_prompt() {
        let body = serde_json::to_vec(&json!({"messages":[{"role":"user","content":[{"type":"text","text":"hello"}]}],"max_tokens":50})).unwrap();
        let i = RouteInput::from_request(&body, &[]);
        assert_eq!(i.last_user, "hello");
        assert_eq!(i.max_out_tokens, 50);
        let legacy = RouteInput::from_request(br#"{"prompt":"abcdefgh"}"#, &[]);
        assert_eq!(legacy.prompt_tokens, 2);
        assert!(RouteInput::from_request(b"junk", &[]).prompt_tokens >= 1);
    }

    #[test]
    fn validation() {
        assert!(router(RouteStrategy::Cost).validate().is_ok());
        let mut r = router(RouteStrategy::Cost);
        r.candidates.push(cand("big", 1, 0.5, 0.0, 0.0));
        assert!(r.validate().is_err(), "duplicate name");
        let mut r = router(RouteStrategy::Split);
        r.candidates.iter_mut().for_each(|c| c.weight = 0);
        assert!(r.validate().is_err());
        let mut r = router(RouteStrategy::Rules);
        assert!(r.validate().is_err(), "rules need a rule or default");
        r.rules = vec![RouteRule { when: RouteMatch::default(), to: "nope".into() }];
        assert!(r.validate().is_err());
        let mut r = router(RouteStrategy::Cost);
        r.candidates[0].quality = 1.5;
        assert!(r.validate().is_err());
        r.candidates[0].quality = 0.5;
        r.candidates[0].cost_per_mtok_in = f64::NAN;
        assert!(r.validate().is_err());
    }

    #[test]
    fn normal_cdf_known_values() {
        assert!((normal_cdf(0.0) - 0.5).abs() < 1e-6);
        assert!((normal_cdf(1.96) - 0.975).abs() < 1e-3);
        assert!((normal_cdf(-1.96) - 0.025).abs() < 1e-3);
    }

    #[test]
    fn compare_detects_a_real_difference_only() {
        let a: Vec<f64> = (0..100).map(|n| if n % 10 < 5 { 1.0 } else { 0.0 }).collect();
        let b: Vec<f64> = (0..100).map(|n| if n % 10 < 8 { 1.0 } else { 0.0 }).collect();
        let c = compare(&a, &b, 0.05).unwrap();
        assert!(c.delta > 0.29 && c.significant && c.p_value < 0.01);
        let same = compare(&a, &a, 0.05).unwrap();
        assert!(!same.significant && same.delta == 0.0);
        assert!(compare(&a[..1], &b, 0.05).is_none());
    }

    fn trace(variant: &str, score: Option<f64>, status: u16, ms: u64) -> Trace {
        Trace {
            id: format!("{variant}{ms}{}", score.unwrap_or(9.0)),
            created_unix: 0,
            endpoint: "front".into(),
            router: Some("front".into()),
            variant: Some(variant.into()),
            target_endpoint: format!("ep-{variant}"),
            model: "m".into(),
            path: "/v1/chat/completions".into(),
            status,
            latency_ms: ms,
            streamed: false,
            tenant: None,
            user: None,
            project: None,
            prompt_tokens: 1_000_000,
            completion_tokens: 500_000,
            finish_reason: None,
            request: None,
            completion: None,
            prompt_token_ids: None,
            completion_token_ids: None,
            logprobs: None,
            request_sha256: String::new(),
            redacted: false,
            score,
            score_source: None,
            labels: vec![],
        }
    }

    #[test]
    fn report_recommends_promoting_a_clear_winner() {
        let r = router(RouteStrategy::Split);
        let mut ts = Vec::new();
        for n in 0..60u64 {
            ts.push(trace("big", Some(if n % 10 < 5 { 1.0 } else { 0.0 }), 200, 100 + n));
            ts.push(trace("small", Some(if n % 10 < 9 { 1.0 } else { 0.0 }), if n == 0 { 500 } else { 200 }, 50 + n));
        }
        let rep = build_report(&r, &ts, 30);
        assert_eq!(rep.control, "big");
        let small = rep.variants.iter().find(|v| v.variant == "small").unwrap();
        assert_eq!(small.requests, 60);
        assert_eq!(small.errors, 1);
        assert!(small.vs_control.as_ref().unwrap().significant);
        assert!(rep.recommendation.contains("promote 'small'"), "{}", rep.recommendation);
        // 60 traces x (1M in, 0.5M out) at $5 / $15 per Mtok.
        let big = rep.variants.iter().find(|v| v.variant == "big").unwrap();
        assert!((big.est_cost_usd - 60.0 * (5.0 + 7.5)).abs() < 1e-6);
        assert!(big.vs_control.is_none());
    }

    #[test]
    fn report_waits_for_enough_samples() {
        let r = router(RouteStrategy::Split);
        let ts = vec![trace("big", Some(1.0), 200, 10), trace("small", Some(0.0), 200, 10)];
        let rep = build_report(&r, &ts, 30);
        assert!(rep.recommendation.starts_with("keep collecting"));
        assert!(rep.variants.iter().all(|v| v.vs_control.is_none()));
    }

    #[test]
    fn report_ignores_other_variants_and_keeps_empty_arms() {
        let r = router(RouteStrategy::Split);
        let rep = build_report(&r, &[trace("ghost", Some(1.0), 200, 1)], 30);
        assert_eq!(rep.variants.len(), 2);
        assert!(rep.variants.iter().all(|v| v.requests == 0 && v.mean_score.is_none()));
    }

    #[test]
    fn percentiles() {
        let v: Vec<u64> = (1..=100).collect();
        assert_eq!(percentile(&v, 0.5), 50);
        assert_eq!(percentile(&v, 0.95), 95);
        assert_eq!(percentile(&[], 0.5), 0);
        assert_eq!(percentile(&[7], 0.95), 7);
    }
}
