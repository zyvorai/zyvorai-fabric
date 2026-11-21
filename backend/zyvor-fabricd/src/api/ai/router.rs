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
//!   (long prompt or tools) raise the bar.
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
    /// Thompson sampling over the traces' scores: traffic drifts to the better arm.
    Bandit,
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
    /// On a connect error or 5xx before any byte is streamed, retry the
    /// next-ranked arm that has a ready backend.
    #[serde(default = "default_true")]
    pub failover: bool,
    /// Arms tried per request, including the first.
    #[serde(default = "default_max_attempts")]
    pub max_attempts: u32,
    /// `bandit`: share of requests sent to a uniformly random arm so every
    /// arm keeps getting measured.
    #[serde(default = "default_explore")]
    pub explore: f64,
    /// `bandit`: scored traces an arm needs before it competes on its record;
    /// until then arms below it are sampled first.
    #[serde(default = "default_warmup")]
    pub warmup: u64,
    #[serde(default)]
    pub updated_unix: i64,
}

fn default_max_attempts() -> u32 {
    2
}
fn default_explore() -> f64 {
    0.1
}
fn default_warmup() -> u64 {
    20
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
                return Err(format!(
                    "candidate name '{}' is empty, too long or repeated",
                    c.name
                ));
            }
            if !(0.0..=1.0).contains(&c.quality) || c.quality.is_nan() {
                return Err(format!(
                    "candidate '{}': quality must be between 0 and 1",
                    c.name
                ));
            }
            for p in [c.cost_per_mtok_in, c.cost_per_mtok_out] {
                if !p.is_finite() || p < 0.0 {
                    return Err(format!(
                        "candidate '{}': prices must be zero or more",
                        c.name
                    ));
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
        if !(0.0..=0.5).contains(&self.explore) || self.explore.is_nan() {
            return Err("explore must be between 0 and 0.5".into());
        }
        if self.strategy == RouteStrategy::Split && self.candidates.iter().all(|c| c.weight == 0) {
            return Err("split needs at least one candidate with weight above 0".into());
        }
        if self.strategy == RouteStrategy::Rules && self.rules.is_empty() && self.default.is_none()
        {
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
    if m.model
        .as_ref()
        .is_some_and(|want| i.model.as_ref() != Some(want))
    {
        return false;
    }
    if let Some((name, want)) = &m.header {
        if i.header(name) != Some(want.as_str()) {
            return false;
        }
    }
    if !m.contains_any.is_empty() {
        let hay = i.last_user.to_lowercase();
        if !m
            .contains_any
            .iter()
            .any(|k| hay.contains(&k.to_lowercase()))
        {
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
    decide_with(r, i, roll, None)
}

/// [`decide`] with the per-arm score record the `bandit` strategy learns from
/// (aligned with `r.candidates`; `None` means no data yet).
pub fn decide_with(r: &AiRouter, i: &RouteInput, roll: u64, arms: Option<&[ArmScore]>) -> Decision {
    if r.allow_override {
        if let Some(want) = i.header("x-fabric-variant") {
            if let Some(idx) = r.candidates.iter().position(|c| c.name == want) {
                return finish(r, idx, format!("override: x-fabric-variant={want}"));
            }
        }
    }
    let (primary, reason) = match r.strategy {
        RouteStrategy::Rules => {
            let hit = r
                .rules
                .iter()
                .enumerate()
                .find(|(_, rule)| rule_matches(&rule.when, i));
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
        RouteStrategy::Bandit => bandit_pick(r, roll, arms),
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
            (
                pick,
                format!("split {}/{}", r.candidates[pick].weight, total),
            )
        }
    };
    finish(r, primary, reason)
}

fn index_of(r: &AiRouter, name: &str) -> usize {
    r.candidates
        .iter()
        .position(|c| c.name == name)
        .unwrap_or(0)
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

// ---- Bandit ----------------------------------------------------------------

/// Scored traces seen on one arm: how many, and the sum of their 0..1 scores.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct ArmScore {
    pub n: u64,
    pub sum: f64,
}

/// SplitMix64: tiny deterministic PRNG so a decision is reproducible from `roll`.
struct Rng(u64);

impl Rng {
    fn next_u64(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }
    /// Uniform in (0, 1).
    fn unit(&mut self) -> f64 {
        ((self.next_u64() >> 11) as f64 + 0.5) / (1u64 << 53) as f64
    }
    fn normal(&mut self) -> f64 {
        let (u1, u2) = (self.unit(), self.unit());
        (-2.0 * u1.ln()).sqrt() * (2.0 * std::f64::consts::PI * u2).cos()
    }
    /// Gamma(shape >= 1, 1) by Marsaglia-Tsang.
    fn gamma(&mut self, shape: f64) -> f64 {
        let d = shape - 1.0 / 3.0;
        let c = 1.0 / (9.0 * d).sqrt();
        loop {
            let x = self.normal();
            let v = (1.0 + c * x).powi(3);
            if v <= 0.0 {
                continue;
            }
            let u = self.unit();
            if u.ln() < 0.5 * x * x + d - d * v + d * v.ln() {
                return d * v;
            }
        }
    }
    fn beta(&mut self, a: f64, b: f64) -> f64 {
        let x = self.gamma(a);
        let y = self.gamma(b);
        x / (x + y)
    }
}

/// Thompson sampling with a Beta(1 + score mass, 1 + miss mass) posterior per
/// arm, which treats a 0..1 score as a fractional success.
fn bandit_pick(r: &AiRouter, roll: u64, arms: Option<&[ArmScore]>) -> (usize, String) {
    let n = r.candidates.len();
    let mut rng = Rng(roll);
    let stat = |k: usize| arms.and_then(|a| a.get(k)).copied().unwrap_or_default();
    let cold: Vec<usize> = (0..n).filter(|k| stat(*k).n < r.warmup).collect();
    if !cold.is_empty() {
        let k = cold[(rng.next_u64() % cold.len() as u64) as usize];
        return (
            k,
            format!(
                "bandit warm-up: '{}' has {} of {} scored traces",
                r.candidates[k].name,
                stat(k).n,
                r.warmup
            ),
        );
    }
    if rng.unit() < r.explore {
        let k = (rng.next_u64() % n as u64) as usize;
        return (k, format!("bandit explore ({:.0}%)", r.explore * 100.0));
    }
    let mut best = (0usize, f64::NEG_INFINITY);
    for k in 0..n {
        let s = stat(k);
        let sum = s.sum.clamp(0.0, s.n as f64);
        let draw = rng.beta(1.0 + sum, 1.0 + (s.n as f64 - sum));
        if draw > best.1 {
            best = (k, draw);
        }
    }
    let means: Vec<String> = (0..n)
        .map(|k| {
            let s = stat(k);
            format!(
                "{}={:.2}",
                r.candidates[k].name,
                if s.n > 0 { s.sum / s.n as f64 } else { 0.0 }
            )
        })
        .collect();
    (best.0, format!("bandit thompson ({})", means.join(" ")))
}

/// Per-arm score record for a bandit router from stored traces, using each
/// arm's newest `window` scored traces so old behaviour ages out.
pub fn arm_scores(r: &AiRouter, traces: &[Trace], window: usize) -> Vec<ArmScore> {
    let mut by: Vec<Vec<(i64, f64)>> = vec![Vec::new(); r.candidates.len()];
    for t in traces {
        let (Some(v), Some(score)) = (t.variant.as_deref(), t.score) else {
            continue;
        };
        if let Some(k) = r.candidates.iter().position(|c| c.name == v) {
            by[k].push((t.created_unix, score));
        }
    }
    by.into_iter()
        .map(|mut list| {
            list.sort_by_key(|(ts, _)| std::cmp::Reverse(*ts));
            list.truncate(window);
            ArmScore {
                n: list.len() as u64,
                sum: list.iter().map(|(_, s)| s).sum(),
            }
        })
        .collect()
}

const BANDIT_WINDOW: usize = 500;
const BANDIT_TTL: std::time::Duration = std::time::Duration::from_secs(10);

type ArmCache = std::collections::HashMap<String, (std::time::Instant, Vec<ArmScore>)>;
static ARM_CACHE: std::sync::LazyLock<std::sync::Mutex<ArmCache>> =
    std::sync::LazyLock::new(Default::default);

/// Cached [`arm_scores`] for a router (refreshed every few seconds so the hot
/// path does not scan the trace store per request).
pub fn bandit_arms(state: &AppState, r: &AiRouter) -> Vec<ArmScore> {
    if let Ok(cache) = ARM_CACHE.lock() {
        if let Some((at, arms)) = cache.get(&r.endpoint) {
            if at.elapsed() < BANDIT_TTL && arms.len() == r.candidates.len() {
                return arms.clone();
            }
        }
    }
    let traces: Vec<Trace> = state
        .store
        .list_entities_filtered(
            STORE_TRACES,
            |t: &Trace| t.router.as_deref() == Some(r.endpoint.as_str()) && t.score.is_some(),
            1_000_000,
        )
        .unwrap_or_default();
    let arms = arm_scores(r, &traces, BANDIT_WINDOW);
    if let Ok(mut cache) = ARM_CACHE.lock() {
        cache.insert(r.endpoint.clone(), (std::time::Instant::now(), arms.clone()));
    }
    arms
}

// ---- A/B report ------------------------------------------------------------

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct Comparison {
    pub delta: f64,
    pub z: f64,
    /// Fixed-horizon p-value: valid only for one look at a planned sample size.
    pub p_value: f64,
    pub significant: bool,
    /// Always-valid p-value (mixture SPRT): safe to check as data arrives.
    pub p_sequential: f64,
    pub sequential_significant: bool,
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
    let p_seq = sequential_p(delta, se * se);
    Some(Comparison {
        delta,
        z: if z.is_finite() { z } else { 0.0 },
        p_value: p,
        significant: p < alpha,
        p_sequential: p_seq,
        sequential_significant: p_seq < alpha,
    })
}

/// Prior standard deviation of the true effect for the mixture SPRT: scores are
/// 0..1, so effects of a few points are the interesting ones.
const MSPRT_TAU: f64 = 0.1;

/// Always-valid p-value of a mean difference `delta` with variance `v` (mixture
/// SPRT with a normal prior on the effect). The likelihood ratio is a martingale
/// under the null, so rejecting whenever it passes 1/alpha keeps the false
/// positive rate at alpha no matter how often the report is read.
pub fn sequential_p(delta: f64, v: f64) -> f64 {
    if v <= 0.0 {
        return if delta == 0.0 { 1.0 } else { 0.0 };
    }
    let t2 = MSPRT_TAU * MSPRT_TAU;
    let ln_lambda = 0.5 * (v / (v + t2)).ln() + t2 * delta * delta / (2.0 * v * (v + t2));
    (-ln_lambda).exp().min(1.0)
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
        let errors = ts
            .iter()
            .filter(|t| !(200..300).contains(&t.status))
            .count() as u64;
        let (pt, ct) = ts.iter().fold((0u64, 0u64), |(p, c), t| {
            (p + t.prompt_tokens, c + t.completion_tokens)
        });
        let vs_control = if c.name == control
            || (sc.len() as u64) < min_samples
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
            error_rate: if ts.is_empty() {
                0.0
            } else {
                errors as f64 / ts.len() as f64
            },
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
            c.sequential_significant && c.delta > 0.0
        })
        .max_by(|a, b| {
            let da = a.vs_control.as_ref().map_or(0.0, |c| c.delta);
            let db = b.vs_control.as_ref().map_or(0.0, |c| c.delta);
            da.total_cmp(&db)
        });
    match best {
        Some(v) => format!(
            "promote '{}': significantly better than '{control}' (sequential test)",
            v.variant
        ),
        None if judged.iter().all(|v| {
            v.vs_control
                .as_ref()
                .is_some_and(|c| c.sequential_significant && c.delta < 0.0)
        }) =>
        {
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
    let r: AiRouter = state
        .store
        .get_entity(STORE_ROUTERS, endpoint)
        .ok()
        .flatten()?;
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
    router
        .validate()
        .map_err(|m| err(StatusCode::BAD_REQUEST, m))?;
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
    audit(
        &state,
        &claims.sub,
        "UPSERT",
        &format!("ai/routers/{}", router.endpoint),
        "SUCCESS",
    );
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
    audit(
        &state,
        &claims.sub,
        "DELETE",
        &format!("ai/routers/{endpoint}"),
        "SUCCESS",
    );
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
    let arms = (r.strategy == RouteStrategy::Bandit).then(|| bandit_arms(&state, &r));
    let d = decide_with(&r, &input, rand::random::<u64>(), arms.as_deref());
    let order: Vec<&str> = d
        .order
        .iter()
        .map(|n| r.candidates[*n].name.as_str())
        .collect();
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
        .filter_map(|(k, v)| {
            v.to_str()
                .ok()
                .map(|v| (k.as_str().to_string(), v.to_string()))
        })
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
    Ok(Json(build_report(
        &r,
        &traces,
        q.min_samples.unwrap_or(30).max(2),
    )))
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
            failover: true,
            max_attempts: 2,
            explore: 0.1,
            warmup: 20,
            updated_unix: 0,
        }
    }

    fn chat(text: &str) -> Vec<u8> {
        serde_json::to_vec(&json!({"model":"m","messages":[{"role":"user","content":text}]}))
            .unwrap()
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
        let tools =
            serde_json::to_vec(&json!({"messages":[],"tools":[{"type":"function"}]})).unwrap();
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
                when: RouteMatch {
                    contains_any: vec!["REFUND".into()],
                    ..Default::default()
                },
                to: "big".into(),
            },
            RouteRule {
                when: RouteMatch {
                    max_prompt_tokens: Some(50),
                    ..Default::default()
                },
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
            let i =
                RouteInput::from_request(&chat("x"), &[("x-user-id".into(), format!("user{n}"))]);
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
        r.rules = vec![RouteRule {
            when: RouteMatch::default(),
            to: "nope".into(),
        }];
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
        let a: Vec<f64> = (0..100)
            .map(|n| if n % 10 < 5 { 1.0 } else { 0.0 })
            .collect();
        let b: Vec<f64> = (0..100)
            .map(|n| if n % 10 < 8 { 1.0 } else { 0.0 })
            .collect();
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
            judge_reason: None,
            labels: vec![],
        }
    }

    #[test]
    fn report_recommends_promoting_a_clear_winner() {
        let r = router(RouteStrategy::Split);
        let mut ts = Vec::new();
        for n in 0..60u64 {
            ts.push(trace(
                "big",
                Some(if n % 10 < 5 { 1.0 } else { 0.0 }),
                200,
                100 + n,
            ));
            ts.push(trace(
                "small",
                Some(if n % 10 < 9 { 1.0 } else { 0.0 }),
                if n == 0 { 500 } else { 200 },
                50 + n,
            ));
        }
        let rep = build_report(&r, &ts, 30);
        assert_eq!(rep.control, "big");
        let small = rep.variants.iter().find(|v| v.variant == "small").unwrap();
        assert_eq!(small.requests, 60);
        assert_eq!(small.errors, 1);
        assert!(small.vs_control.as_ref().unwrap().significant);
        assert!(
            rep.recommendation.contains("promote 'small'"),
            "{}",
            rep.recommendation
        );
        // 60 traces x (1M in, 0.5M out) at $5 / $15 per Mtok.
        let big = rep.variants.iter().find(|v| v.variant == "big").unwrap();
        assert!((big.est_cost_usd - 60.0 * (5.0 + 7.5)).abs() < 1e-6);
        assert!(big.vs_control.is_none());
    }

    #[test]
    fn report_waits_for_enough_samples() {
        let r = router(RouteStrategy::Split);
        let ts = vec![
            trace("big", Some(1.0), 200, 10),
            trace("small", Some(0.0), 200, 10),
        ];
        let rep = build_report(&r, &ts, 30);
        assert!(rep.recommendation.starts_with("keep collecting"));
        assert!(rep.variants.iter().all(|v| v.vs_control.is_none()));
    }

    #[test]
    fn report_ignores_other_variants_and_keeps_empty_arms() {
        let r = router(RouteStrategy::Split);
        let rep = build_report(&r, &[trace("ghost", Some(1.0), 200, 1)], 30);
        assert_eq!(rep.variants.len(), 2);
        assert!(rep
            .variants
            .iter()
            .all(|v| v.requests == 0 && v.mean_score.is_none()));
    }

    #[test]
    fn percentiles() {
        let v: Vec<u64> = (1..=100).collect();
        assert_eq!(percentile(&v, 0.5), 50);
        assert_eq!(percentile(&v, 0.95), 95);
        assert_eq!(percentile(&[], 0.5), 0);
        assert_eq!(percentile(&[7], 0.95), 7);
    }

    #[test]
    fn failover_defaults_on_with_two_attempts() {
        let r: AiRouter = serde_json::from_value(json!({
            "endpoint": "front",
            "strategy": "split",
            "candidates": [{"name":"a","endpoint":"a"},{"name":"b","endpoint":"b"}]
        }))
        .unwrap();
        assert!(r.failover);
        assert_eq!(r.max_attempts, 2);
    }

    fn trace_at(variant: &str, score: Option<f64>, ts: i64) -> Trace {
        let mut t = trace(variant, score, 200, ts as u64);
        t.created_unix = ts;
        t
    }

    fn arms(a: (u64, f64), b: (u64, f64)) -> Vec<ArmScore> {
        vec![
            ArmScore { n: a.0, sum: a.1 },
            ArmScore { n: b.0, sum: b.1 },
        ]
    }

    fn picks(r: &AiRouter, a: &[ArmScore], n: u64) -> [u32; 2] {
        let mut c = [0u32; 2];
        for roll in 0..n {
            let d = decide_with(r, &input("hi"), roll.wrapping_mul(0x9E37_79B9), Some(a));
            c[d.order[0]] += 1;
        }
        c
    }

    #[test]
    fn bandit_warms_up_arms_without_data() {
        let r = router(RouteStrategy::Bandit);
        // "small" has no scores yet: it is the only cold arm, so it is always picked.
        let c = picks(&r, &arms((100, 90.0), (0, 0.0)), 200);
        assert_eq!(c, [0, 200]);
        let d = decide_with(&r, &input("hi"), 1, None);
        assert!(d.reason.starts_with("bandit warm-up"));
    }

    #[test]
    fn bandit_shifts_traffic_to_the_better_arm() {
        let mut r = router(RouteStrategy::Bandit);
        r.explore = 0.0;
        // big averages 0.9, small 0.5 over 100 scored traces each.
        let c = picks(&r, &arms((100, 90.0), (100, 50.0)), 1000);
        assert!(c[0] > 990, "better arm should win nearly always: {c:?}");
    }

    #[test]
    fn bandit_still_explores_and_keeps_uncertain_arms_alive() {
        let mut r = router(RouteStrategy::Bandit);
        r.explore = 0.2;
        let c = picks(&r, &arms((100, 90.0), (100, 50.0)), 2000);
        // 20% explore spreads half of those over the weaker arm: roughly 10%.
        assert!(c[1] > 120 && c[1] < 300, "explore share off: {c:?}");
        // Two arms with almost equal records stay close to 50/50.
        r.explore = 0.0;
        let c = picks(&r, &arms((30, 21.0), (30, 20.0)), 2000);
        assert!(c[0] > 700 && c[1] > 500, "near-tie should mix: {c:?}");
    }

    #[test]
    fn bandit_is_deterministic_for_a_roll() {
        let r = router(RouteStrategy::Bandit);
        let a = arms((50, 40.0), (50, 35.0));
        let x = decide_with(&r, &input("hi"), 42, Some(&a));
        let y = decide_with(&r, &input("hi"), 42, Some(&a));
        assert_eq!(x.order, y.order);
    }

    #[test]
    fn beta_sampler_has_the_right_mean() {
        let mut rng = Rng(7);
        let n = 20_000;
        let mean = (0..n).map(|_| rng.beta(9.0, 3.0)).sum::<f64>() / n as f64;
        assert!((mean - 0.75).abs() < 0.01, "mean {mean}");
    }

    #[test]
    fn arm_scores_use_the_newest_window() {
        let r = router(RouteStrategy::Bandit);
        let mut ts = Vec::new();
        for i in 0..10 {
            ts.push(trace_at("big", Some(if i < 5 { 0.0 } else { 1.0 }), i));
        }
        let a = arm_scores(&r, &ts, 5);
        assert_eq!(a[0], ArmScore { n: 5, sum: 5.0 });
        assert_eq!(a[1], ArmScore::default());
    }

    #[test]
    fn sequential_p_is_conservative_where_fixed_horizon_is_not() {
        // A borderline effect: z ~ 2.1 passes the fixed test but not the always-valid one.
        let v: f64 = 0.001; // se ~ 0.0316
        let delta = 2.1 * v.sqrt();
        assert!(sequential_p(delta, v) > 0.05);
        // A large effect clears both.
        assert!(sequential_p(0.3, v) < 1e-6);
        // No difference never rejects.
        assert_eq!(sequential_p(0.0, v), 1.0);
        assert_eq!(sequential_p(0.0, 0.0), 1.0);
        assert_eq!(sequential_p(0.1, 0.0), 0.0);
    }

    #[test]
    fn sequential_test_holds_its_false_positive_rate_under_peeking() {
        // Two identical arms, checked after every 10 samples up to 1000.
        let mut rng = Rng(99);
        let mut rejected = 0;
        let trials = 300;
        for _ in 0..trials {
            let (mut a, mut b) = (Vec::new(), Vec::new());
            let mut hit = false;
            for step in 1..=100 {
                for _ in 0..10 {
                    a.push(f64::from(rng.unit() < 0.6));
                    b.push(f64::from(rng.unit() < 0.6));
                }
                if let Some(c) = compare(&a, &b, 0.05) {
                    if c.sequential_significant {
                        hit = true;
                        break;
                    }
                }
                let _ = step;
            }
            rejected += usize::from(hit);
        }
        // Naive peeking with the fixed test would reject far more than 5%.
        assert!(
            (rejected as f64 / trials as f64) < 0.08,
            "false positives {rejected}/{trials}"
        );
    }

    #[test]
    fn explore_must_be_a_fraction() {
        let mut r = router(RouteStrategy::Bandit);
        assert!(r.validate().is_ok());
        r.explore = 0.9;
        assert!(r.validate().is_err());
    }
}
