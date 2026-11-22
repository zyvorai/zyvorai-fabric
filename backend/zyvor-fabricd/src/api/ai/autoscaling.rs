// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: LicenseRef-Zyvor-Production-2.0
//!
//! Queue / TTFT-driven inference autoscaler (Phase 3, preview).

use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

use chrono::Utc;

use crate::server::AppState;

use super::eligibility::metrics_fresh;
use super::reconcile;
use super::types::{AutoscalingPolicy, InferenceDeployment, InferenceProfile};
use super::{STORE_DEPLOYMENTS, STORE_PROFILES};

/// Background loop evaluating autoscaling policies.
pub async fn run_ai_autoscaler(state: Arc<AppState>) {
    let mut interval = tokio::time::interval(Duration::from_secs(15));
    let mut out_since: HashMap<String, Instant> = HashMap::new();
    let mut in_since: HashMap<String, Instant> = HashMap::new();
    loop {
        interval.tick().await;
        if let Err(e) = autoscaler_tick(&state, &mut out_since, &mut in_since).await {
            tracing::debug!("AI autoscaler tick: {e}");
        }
    }
}

async fn autoscaler_tick(
    state: &Arc<AppState>,
    out_since: &mut HashMap<String, Instant>,
    in_since: &mut HashMap<String, Instant>,
) -> Result<(), String> {
    let deps: Vec<InferenceDeployment> = state
        .store
        .list_entities(STORE_DEPLOYMENTS)
        .map_err(|e| e.to_string())?;

    for dep in deps {
        if !dep.autoscaling.enabled {
            out_since.remove(&dep.name);
            in_since.remove(&dep.name);
            continue;
        }
        if let Err(e) = evaluate_one(state, &dep, out_since, in_since).await {
            tracing::debug!(deployment = %dep.name, "AI autoscaler: {e}");
        }
    }
    Ok(())
}

async fn evaluate_one(
    state: &Arc<AppState>,
    dep: &InferenceDeployment,
    out_since: &mut HashMap<String, Instant>,
    in_since: &mut HashMap<String, Instant>,
) -> Result<(), String> {
    let policy = &dep.autoscaling;
    let (avg_queue, avg_ttft, ready) = aggregate_metrics(dep);

    let want_out = ready > 0
        && (avg_queue > f64::from(policy.scale_out_queue)
            || (policy.scale_out_ttft_ms > 0.0 && avg_ttft > policy.scale_out_ttft_ms));
    let want_in = avg_queue < f64::from(policy.scale_in_queue)
        && (policy.scale_out_ttft_ms <= 0.0 || avg_ttft < policy.scale_out_ttft_ms * 0.5);

    let now = Instant::now();
    if want_out {
        in_since.remove(&dep.name);
        let since = out_since.entry(dep.name.clone()).or_insert(now);
        if now.duration_since(*since) >= Duration::from_secs(policy.scale_out_seconds) {
            let next = next_replicas(dep.replicas, policy, 1);
            if next > dep.replicas {
                apply_scale(state, &dep.name, next, "scale_out").await?;
            }
            out_since.remove(&dep.name);
        }
    } else if want_in {
        out_since.remove(&dep.name);
        let since = in_since.entry(dep.name.clone()).or_insert(now);
        if now.duration_since(*since) >= Duration::from_secs(policy.scale_in_seconds) {
            let next = next_replicas(dep.replicas, policy, -1);
            if next < dep.replicas {
                apply_scale(state, &dep.name, next, "scale_in").await?;
            }
            in_since.remove(&dep.name);
        }
    } else {
        out_since.remove(&dep.name);
        in_since.remove(&dep.name);
    }
    Ok(())
}

/// Instantaneous scale signal. The live autoscaler still waits
/// `scale_out_seconds` or `scale_in_seconds` before it changes `replicas`.
pub fn explain_scale(dep: &InferenceDeployment) -> ScaleSignal {
    let (avg_queue, avg_ttft, ready) = aggregate_metrics(dep);
    let current = dep.replicas;
    let base = |action, reason, next| ScaleSignal {
        action,
        reason,
        current,
        next,
        ready,
        avg_queue,
        avg_ttft,
    };
    if !dep.autoscaling.enabled {
        return base("hold", "autoscaling disabled", current);
    }
    let policy = &dep.autoscaling;
    let want_out = ready > 0
        && (avg_queue > f64::from(policy.scale_out_queue)
            || (policy.scale_out_ttft_ms > 0.0 && avg_ttft > policy.scale_out_ttft_ms));
    let want_in = avg_queue < f64::from(policy.scale_in_queue)
        && (policy.scale_out_ttft_ms <= 0.0 || avg_ttft < policy.scale_out_ttft_ms * 0.5);
    if want_out {
        let next = next_replicas(current, policy, 1);
        if next <= current {
            return base("hold", "already at max replicas", current);
        }
        return base(
            "scale_out",
            "queue or TTFT is above the scale-out threshold",
            next,
        );
    }
    if want_in {
        if dep
            .status
            .replicas
            .iter()
            .any(|r| r.ready && !metrics_fresh(r.metrics.as_ref()))
        {
            return base(
                "hold",
                "scale-in skipped: replica metrics missing or stale",
                current,
            );
        }
        let next = next_replicas(current, policy, -1);
        if next >= current {
            return base("hold", "already at min replicas", current);
        }
        return base(
            "scale_in",
            "queue and TTFT are below the scale-in threshold",
            next,
        );
    }
    if ready == 0 {
        return base("hold", "no ready replica", current);
    }
    base("hold", "queue and TTFT are inside the hold band", current)
}

fn aggregate_metrics(dep: &InferenceDeployment) -> (f64, f64, usize) {
    let ready: Vec<_> = dep.status.replicas.iter().filter(|r| r.ready).collect();
    if ready.is_empty() {
        return (0.0, 0.0, 0);
    }
    let n = ready.len() as f64;
    let avg_queue = ready
        .iter()
        .map(|r| f64::from(r.metrics.as_ref().map(|m| m.queue_depth).unwrap_or(0)))
        .sum::<f64>()
        / n;
    let avg_ttft = ready
        .iter()
        .map(|r| r.metrics.as_ref().map(|m| m.ttft_ms).unwrap_or(0.0))
        .sum::<f64>()
        / n;
    (avg_queue, avg_ttft, ready.len())
}

/// Compute next desired replica count after a ±1 step, honouring min/max / scale-to-zero.
pub fn next_replicas(current: u32, policy: &AutoscalingPolicy, delta: i32) -> u32 {
    let mut min = policy.min_replicas;
    if policy.scale_to_zero {
        min = 0;
    }
    let max = policy.max_replicas.max(min);
    let standby = if policy.warm_standby { 1u32 } else { 0 };
    let effective_max = max.saturating_add(standby);
    let next = (current as i64 + i64::from(delta)).clamp(i64::from(min), i64::from(effective_max));
    next as u32
}

async fn apply_scale(
    state: &Arc<AppState>,
    name: &str,
    replicas: u32,
    reason: &str,
) -> Result<(), String> {
    let mut dep: InferenceDeployment = state
        .store
        .get_entity(STORE_DEPLOYMENTS, name)
        .map_err(|e| e.to_string())?
        .ok_or_else(|| format!("deployment '{name}' not found"))?;
    if dep.replicas == replicas {
        return Ok(());
    }
    if replicas < dep.replicas
        && dep
            .status
            .replicas
            .iter()
            .any(|r| r.ready && !metrics_fresh(r.metrics.as_ref()))
    {
        dep.status.message =
            Some("autoscaler skipped scale-in: replica metrics missing or stale".into());
        dep.updated = Utc::now();
        state
            .store
            .save_entity(STORE_DEPLOYMENTS, &dep.name, &dep)
            .map_err(|e| e.to_string())?;
        return Ok(());
    }
    if replicas > dep.replicas {
        let gpus_per = dep.gpus_per_replica.max(1);
        let new_gpus = replicas.saturating_mul(gpus_per);
        let profile = state
            .store
            .get_entity::<InferenceProfile>(STORE_PROFILES, &dep.profile)
            .ok()
            .flatten();
        let (cpus, memory_mb) = match &profile {
            Some(p) => (
                p.cpu.saturating_mul(replicas),
                (p.memory_gib as u64)
                    .saturating_mul(1024)
                    .saturating_mul(replicas as u64),
            ),
            None => (0, 0),
        };
        if let Err(e) = crate::api::quotas::check_quota_enforcement(
            state,
            cpus,
            memory_mb,
            0,
            &[],
            dep.tenant.as_deref(),
            replicas,
            0,
            None,
            new_gpus,
            Some(dep.name.as_str()),
        )
        .await
        {
            dep.status.message = Some(format!("autoscaler skipped scale-out: {e}"));
            dep.updated = Utc::now();
            let _ = state.store.save_entity(STORE_DEPLOYMENTS, &dep.name, &dep);
            return Ok(());
        }
    }
    tracing::info!(
        deployment = %name,
        from = dep.replicas,
        to = replicas,
        reason,
        "AI autoscaler adjusting replicas"
    );
    dep.replicas = replicas;
    dep.revision = dep.revision.saturating_add(1);
    dep.status.phase = "Scaling".into();
    dep.status.message = Some(format!("autoscaler {reason} → {replicas}"));
    dep.updated = Utc::now();
    state
        .store
        .save_entity(STORE_DEPLOYMENTS, &dep.name, &dep)
        .map_err(|e| e.to_string())?;
    reconcile::enqueue_deployment_reconcile(state.clone(), dep.name.clone());
    Ok(())
}

/// Instantaneous autoscaler reading. Duration gates stay in the background loop.
#[derive(Debug, Clone, PartialEq)]
pub struct ScaleSignal {
    pub action: &'static str,
    pub reason: &'static str,
    pub current: u32,
    pub next: u32,
    pub ready: usize,
    pub avg_queue: f64,
    pub avg_ttft: f64,
}

/// Pure decision helper for unit tests.
pub fn suggest_delta(
    avg_queue: f64,
    avg_ttft: f64,
    policy: &AutoscalingPolicy,
    sustained_out: bool,
    sustained_in: bool,
) -> i32 {
    let want_out = avg_queue > f64::from(policy.scale_out_queue)
        || (policy.scale_out_ttft_ms > 0.0 && avg_ttft > policy.scale_out_ttft_ms);
    let want_in = avg_queue < f64::from(policy.scale_in_queue)
        && (policy.scale_out_ttft_ms <= 0.0 || avg_ttft < policy.scale_out_ttft_ms * 0.5);
    if want_out && sustained_out {
        1
    } else if want_in && sustained_in {
        -1
    } else {
        0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn next_respects_bounds() {
        let mut p = AutoscalingPolicy::default();
        p.min_replicas = 1;
        p.max_replicas = 3;
        assert_eq!(next_replicas(1, &p, -1), 1);
        assert_eq!(next_replicas(3, &p, 1), 3);
        assert_eq!(next_replicas(2, &p, 1), 3);
    }

    #[test]
    fn scale_to_zero() {
        let mut p = AutoscalingPolicy::default();
        p.min_replicas = 1;
        p.scale_to_zero = true;
        p.max_replicas = 2;
        assert_eq!(next_replicas(1, &p, -1), 0);
    }

    #[test]
    fn suggest_scale_out_on_queue() {
        let p = AutoscalingPolicy {
            scale_out_queue: 20,
            ..AutoscalingPolicy::default()
        };
        assert_eq!(suggest_delta(25.0, 100.0, &p, true, false), 1);
        assert_eq!(suggest_delta(25.0, 100.0, &p, false, false), 0);
        assert_eq!(suggest_delta(1.0, 50.0, &p, false, true), -1);
    }
}
