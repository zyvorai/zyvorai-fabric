// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: LicenseRef-Zyvor-Production-1.0

//! Per-user usage and quotas.
//!
//! [`usage_for`] adds up what one user has consumed, straight from the session, artifact and audit
//! records the runtime already keeps: it is the metering hook a vendor bills or rate-limits from.
//! Limits are off unless the operator sets them, so a single-user install behaves as before.
//!
//! | Variable | Limit |
//! |---|---|
//! | `ZYVOR_AGENT_USER_MAX_RUNS_PER_DAY` | sessions (use-case runs included) started in the last 24 h |
//! | `ZYVOR_AGENT_USER_MAX_ARTIFACTS` | artifacts kept |
//! | `ZYVOR_AGENT_USER_MAX_MODEL_CALLS_PER_DAY` | model-step calls in the last 24 h |
//!
//! A user over a limit gets HTTP 429.

use crate::{app::ApiError, audit::AuditPhase, authz, AppState};
use chrono::{DateTime, Duration, Utc};
use serde::Serialize;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Limits {
    pub max_runs_per_day: Option<u64>,
    pub max_artifacts: Option<u64>,
    pub max_model_calls_per_day: Option<u64>,
}

impl Limits {
    pub fn from_env() -> Self {
        let get = |name: &str| {
            std::env::var(name)
                .ok()
                .and_then(|v| v.trim().parse::<u64>().ok())
                .filter(|n| *n > 0)
        };
        Self {
            max_runs_per_day: get("ZYVOR_AGENT_USER_MAX_RUNS_PER_DAY"),
            max_artifacts: get("ZYVOR_AGENT_USER_MAX_ARTIFACTS"),
            max_model_calls_per_day: get("ZYVOR_AGENT_USER_MAX_MODEL_CALLS_PER_DAY"),
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct Usage {
    pub user_id: String,
    pub since: Option<DateTime<Utc>>,
    /// Sessions started since `since` (use-case runs are sessions).
    pub runs: u64,
    /// Artifacts the user owns (all time).
    pub artifacts: u64,
    pub artifact_bytes: u64,
    /// Model-step calls since `since`.
    pub model_calls: u64,
    /// Wall-clock seconds of the sessions started since `since`, from created to last update.
    /// Approximate: a cell that idles or is still running is counted up to its last update.
    pub session_seconds: u64,
}

/// What `user` has used since `since` (all time when `None`).
pub async fn usage_for(state: &AppState, user: &str, since: Option<DateTime<Utc>>) -> Usage {
    let mine = authz::session_ids_of(state, user).await;
    let mut u = Usage {
        user_id: user.to_string(),
        since,
        ..Usage::default()
    };
    for s in state.store.list_sessions().await {
        if s.user_id.as_deref() != Some(user) || since.is_some_and(|t| s.created_at < t) {
            continue;
        }
        u.runs += 1;
        u.session_seconds += (s.updated_at - s.created_at).num_seconds().max(0) as u64;
    }
    for a in state.store.list_artifacts().await {
        if a.session_id.is_some_and(|sid| mine.contains(&sid)) {
            u.artifacts += 1;
            u.artifact_bytes += a.body.len() as u64;
        }
    }
    if let Ok(rows) = state.store.audit.list_for_sessions(&mine, usize::MAX).await {
        u.model_calls = rows
            .iter()
            .filter(|e| {
                e.action == "model.call"
                    && e.phase == AuditPhase::Performed
                    && since.is_none_or(|t| e.at >= t)
            })
            .count() as u64;
    }
    u
}

fn over(what: &str, used: u64, limit: u64) -> ApiError {
    ApiError::too_many(format!("quota reached: {used} of {limit} {what}"))
}

/// Refuse to start another run when the user is at a limit.
pub(crate) async fn check_run_quota(
    state: &AppState,
    user: &str,
    limits: Limits,
) -> Result<(), ApiError> {
    if limits.max_runs_per_day.is_none() && limits.max_artifacts.is_none() {
        return Ok(());
    }
    let u = usage_for(state, user, Some(Utc::now() - Duration::hours(24))).await;
    if let Some(max) = limits.max_runs_per_day.filter(|m| u.runs >= *m) {
        return Err(over("runs in the last 24 hours", u.runs, max));
    }
    if let Some(max) = limits.max_artifacts {
        let all = usage_for(state, user, None).await;
        if all.artifacts >= max {
            return Err(over("stored artifacts", all.artifacts, max));
        }
    }
    Ok(())
}

/// Refuse a model call when the user is at their daily limit.
pub(crate) async fn check_model_quota(
    state: &AppState,
    user: &str,
    limits: Limits,
) -> Result<(), ApiError> {
    let Some(max) = limits.max_model_calls_per_day else {
        return Ok(());
    };
    let u = usage_for(state, user, Some(Utc::now() - Duration::hours(24))).await;
    if u.model_calls >= max {
        return Err(over("model calls in the last 24 hours", u.model_calls, max));
    }
    Ok(())
}
