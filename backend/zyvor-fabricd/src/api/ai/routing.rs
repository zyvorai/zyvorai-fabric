// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: LicenseRef-Zyvor-Production-2.0
//!
//! AI-aware Maglev weight controller (Phase 2, preview).
//!
//! Scrapes vLLM Prometheus `/metrics` from each ready replica and maps
//! queue / TTFT / KV-cache into Maglev weights (1..=32). The eBPF program
//! never learns about models or tokens — only weights change.

use chrono::Utc;
use std::collections::HashSet;
use std::sync::Arc;

use crate::server::AppState;

use super::eligibility::replica_serving;
use super::maglev::build_maglev_service_spec_weighted;
use super::types::{
    InferenceDeployment, InferenceEndpoint, InferenceReplica, ReplicaMetrics, RoutingStrategy,
};
use super::{fluxvm_client, STORE_DEPLOYMENTS, STORE_ENDPOINTS};

const MAX_WEIGHT: u16 = 32;
const METRICS_PATH: &str = "/metrics";

/// Background loop: scrape replica metrics and push Maglev weight updates.
pub async fn run_ai_routing_controller(state: Arc<AppState>) {
    let mut interval = tokio::time::interval(std::time::Duration::from_secs(10));
    loop {
        interval.tick().await;
        if let Err(e) = routing_tick(&state).await {
            tracing::debug!("AI routing tick: {e}");
        }
    }
}

async fn routing_tick(state: &AppState) -> Result<(), String> {
    let endpoints: Vec<InferenceEndpoint> = state
        .store
        .list_entities(STORE_ENDPOINTS)
        .map_err(|e| e.to_string())?;
    for ep in endpoints {
        if let Err(e) = refresh_endpoint_weights(state, &ep).await {
            tracing::debug!(endpoint = %ep.name, "AI routing refresh: {e}");
        }
    }
    Ok(())
}

/// Scrape metrics for a deployment's replicas, compute weights, upsert Maglev.
pub async fn refresh_endpoint_weights(
    state: &AppState,
    ep: &InferenceEndpoint,
) -> Result<(), String> {
    let mut dep: InferenceDeployment = state
        .store
        .get_entity(STORE_DEPLOYMENTS, &ep.deployment)
        .map_err(|e| e.to_string())?
        .ok_or_else(|| format!("deployment '{}' not found", ep.deployment))?;

    let dry_run = std::env::var("FLUXVM_AI_DRY_RUN")
        .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
        .unwrap_or(false);

    for rep in &mut dep.status.replicas {
        if !rep.ready {
            continue;
        }
        let metrics = if dry_run {
            synthetic_dry_run_metrics(rep)
        } else if let Some(addr) = rep.address.clone() {
            scrape_vllm_metrics(&addr, ep.port)
                .await
                .unwrap_or_else(|_| ReplicaMetrics {
                    source: Some("scrape_failed".into()),
                    scraped_at: Some(Utc::now()),
                    ..Default::default()
                })
        } else {
            continue;
        };
        rep.metrics = Some(metrics);
    }

    let ready = filter_replicas_for_endpoint(ep, &dep.status.replicas);
    let weights = match ep.routing_strategy {
        RoutingStrategy::SiteLocal => {
            compute_site_local_weights(&ready, ep.preferred_site.as_deref())
        }
        other => compute_weights(other, &ready),
    };
    let named_weights: Vec<(String, u16)> = ready
        .iter()
        .zip(weights.iter().copied())
        .map(|(r, w)| (r.vm_name.clone(), w))
        .collect();
    let weighted_names: HashSet<String> = named_weights.iter().map(|(n, _)| n.clone()).collect();

    for (name, w) in named_weights {
        if let Some(slot) = dep.status.replicas.iter_mut().find(|r| r.vm_name == name) {
            slot.maglev_weight = Some(w);
        }
    }
    for slot in &mut dep.status.replicas {
        if !weighted_names.contains(&slot.vm_name) || !replica_serving(slot) {
            slot.maglev_weight = None;
        }
    }
    dep.updated = Utc::now();
    state
        .store
        .save_entity(STORE_DEPLOYMENTS, &dep.name, &dep)
        .map_err(|e| e.to_string())?;

    // Maglev upsert (skip when FluxVM unreachable / dry-run without VIP service).
    if dry_run {
        return Ok(());
    }
    let Some(vip) = ep.vip.as_deref() else {
        return Ok(());
    };
    let client = match fluxvm_client(state) {
        Ok(c) => c,
        Err(_) => return Ok(()),
    };
    let profile_egress = None;
    let spec = build_maglev_service_spec_weighted(
        &ep.name,
        vip,
        ep.port,
        &dep.status.replicas,
        profile_egress,
    );
    let _ = client.upsert_network_service(&spec).await;
    Ok(())
}

fn synthetic_dry_run_metrics(rep: &InferenceReplica) -> ReplicaMetrics {
    // Deterministic fake load from vm name so weights diverge in smoke tests.
    let n = rep.vm_name.bytes().map(|b| b as u32).sum::<u32>();
    ReplicaMetrics {
        queue_depth: n % 20,
        active_requests: n % 8,
        ttft_ms: 50.0 + (n % 200) as f64,
        tokens_per_sec: 80.0 - (n % 40) as f64,
        gpu_cache_usage: (n % 100) as f64 / 100.0,
        http_error_rate: 0.0,
        scraped_at: Some(Utc::now()),
        source: Some("dry_run".into()),
    }
}

/// Map strategy + metrics → Maglev weights (1..=32), one per ready replica
/// in the same order as `ready`. Non-ready callers should filter first.
pub fn compute_weights(strategy: RoutingStrategy, ready: &[&InferenceReplica]) -> Vec<u16> {
    if ready.is_empty() {
        return Vec::new();
    }
    match strategy {
        RoutingStrategy::Equal => ready.iter().map(|_| 1u16).collect(),
        RoutingStrategy::LeastQueue => ready
            .iter()
            .map(|r| {
                let q = r.metrics.as_ref().map(|m| m.queue_depth).unwrap_or(0);
                weight_from_queue(q)
            })
            .collect(),
        RoutingStrategy::LowestTtft => {
            let ttfts: Vec<f64> = ready
                .iter()
                .map(|r| {
                    let t = r.metrics.as_ref().map(|m| m.ttft_ms).unwrap_or(0.0);
                    if t <= 0.0 {
                        1000.0
                    } else {
                        t
                    }
                })
                .collect();
            let max_t = ttfts.iter().cloned().fold(1.0_f64, f64::max);
            ttfts
                .iter()
                .map(|t| {
                    // Lower TTFT → higher weight.
                    let score = (max_t - t) / max_t;
                    clamp_weight(((score * f64::from(MAX_WEIGHT)).round() as i32).max(1) as u16)
                })
                .collect()
        }
        RoutingStrategy::MostFreeVram | RoutingStrategy::EnergyOptimized => ready
            .iter()
            .map(|r| {
                let usage = r
                    .metrics
                    .as_ref()
                    .map(|m| m.gpu_cache_usage.clamp(0.0, 1.0))
                    .unwrap_or(0.5);
                let free = 1.0 - usage;
                clamp_weight(((free * f64::from(MAX_WEIGHT)).round() as i32).max(1) as u16)
            })
            .collect(),
        RoutingStrategy::WeightedCapacity => {
            let q_w = compute_weights(RoutingStrategy::LeastQueue, ready);
            let v_w = compute_weights(RoutingStrategy::MostFreeVram, ready);
            q_w.into_iter()
                .zip(v_w)
                .map(|(a, b)| {
                    clamp_weight(u32::from(a).saturating_add(u32::from(b)).div_ceil(2) as u16)
                })
                .collect()
        }
        RoutingStrategy::SiteLocal => {
            // Prefer replicas whose site matches a preferred site when known.
            let preferred: Option<&str> = ready.iter().find_map(|r| r.site.as_deref());
            ready
                .iter()
                .map(|r| {
                    if r.draining {
                        return 1;
                    }
                    match (preferred, r.site.as_deref()) {
                        (Some(p), Some(s)) if p == s => MAX_WEIGHT,
                        (Some(_), Some(_)) => 4,
                        _ => 16,
                    }
                })
                .collect()
        }
        RoutingStrategy::CostOptimized => ready
            .iter()
            .map(|r| {
                let tier = u16::from(r.cost_tier.unwrap_or(8)).clamp(1, MAX_WEIGHT);
                // Lower cost_tier → higher weight.
                clamp_weight(MAX_WEIGHT + 1 - tier)
            })
            .collect(),
        RoutingStrategy::HighestThroughput => {
            let rates: Vec<f64> = ready
                .iter()
                .map(|r| {
                    r.metrics
                        .as_ref()
                        .map(|m| m.tokens_per_sec.max(0.0))
                        .unwrap_or(0.0)
                })
                .collect();
            let max_rate = rates.iter().copied().fold(0.0_f64, f64::max);
            if max_rate <= 0.0 {
                return ready.iter().map(|_| 1u16).collect();
            }
            rates
                .iter()
                .map(|rate| {
                    let score = rate / max_rate;
                    clamp_weight(((score * f64::from(MAX_WEIGHT)).round() as i32).max(1) as u16)
                })
                .collect()
        }
        RoutingStrategy::LowestFailure => ready
            .iter()
            .map(|r| {
                let error = r
                    .metrics
                    .as_ref()
                    .map(|m| m.http_error_rate.clamp(0.0, 1.0))
                    .unwrap_or(0.0);
                let ok = 1.0 - error;
                clamp_weight(((ok * f64::from(MAX_WEIGHT)).round() as i32).max(1) as u16)
            })
            .collect(),
    }
}

/// Why this replica is excluded from the endpoint, or `None` when it may receive weight.
pub fn routing_exclusion(ep: &InferenceEndpoint, rep: &InferenceReplica) -> Option<&'static str> {
    if !rep.ready {
        return Some("not ready");
    }
    if rep.draining {
        return Some("draining");
    }
    if !super::eligibility::metrics_fresh(rep.metrics.as_ref()) {
        return Some("metrics missing or stale");
    }
    if let Some(res) = ep.residency.as_deref() {
        if !matches!(rep.site.as_deref(), Some(s) if s == res) {
            return Some("outside residency");
        }
    }
    if !ep.allowed_sites.is_empty() {
        match rep.site.as_deref() {
            Some(s) if ep.allowed_sites.iter().any(|a| a == s) => {}
            Some(_) => return Some("site not allowed"),
            None => return Some("replica has no site"),
        }
    }
    None
}

/// Apply residency / site allow-list filtering before weight compute (Phase 5).
pub fn filter_replicas_for_endpoint<'a>(
    ep: &InferenceEndpoint,
    replicas: &'a [InferenceReplica],
) -> Vec<&'a InferenceReplica> {
    replicas
        .iter()
        .filter(|r| routing_exclusion(ep, r).is_none())
        .collect()
}

fn weight_from_queue(queue: u32) -> u16 {
    // queue 0 → 32, queue ≥ 32 → 1
    let q = queue.min(u32::from(MAX_WEIGHT));
    clamp_weight((u32::from(MAX_WEIGHT) + 1 - q) as u16)
}

fn clamp_weight(w: u16) -> u16 {
    w.clamp(1, MAX_WEIGHT)
}

/// Site-local Maglev weights using the endpoint's preferred site (Phase 5).
pub fn compute_site_local_weights(
    ready: &[&InferenceReplica],
    preferred: Option<&str>,
) -> Vec<u16> {
    if ready.is_empty() {
        return Vec::new();
    }
    ready
        .iter()
        .map(|r| {
            if r.draining {
                return 1;
            }
            match (preferred, r.site.as_deref()) {
                (Some(p), Some(s)) if p == s => MAX_WEIGHT,
                (Some(_), Some(_)) => 4,
                (Some(p), None) => {
                    // Untagged: treat as local only when no preferred site set.
                    let _ = p;
                    8
                }
                _ => 16,
            }
        })
        .collect()
}

/// Parse a subset of vLLM Prometheus text exposition.
pub async fn scrape_vllm_metrics(addr: &str, port: u16) -> Result<ReplicaMetrics, String> {
    let url = format!("http://{addr}:{port}{METRICS_PATH}");
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(2))
        .build()
        .map_err(|e| e.to_string())?;
    let text = client
        .get(&url)
        .send()
        .await
        .map_err(|e| e.to_string())?
        .error_for_status()
        .map_err(|e| e.to_string())?
        .text()
        .await
        .map_err(|e| e.to_string())?;
    Ok(parse_vllm_prometheus(&text))
}

/// Extract waiting/running/cache/TTFT from Prometheus text.
pub fn parse_vllm_prometheus(text: &str) -> ReplicaMetrics {
    let mut m = ReplicaMetrics {
        scraped_at: Some(Utc::now()),
        source: Some("vllm_prometheus".into()),
        ..Default::default()
    };
    let mut succeeded = 0.0_f64;
    let mut aborted = 0.0_f64;
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let Some((name, rest)) = split_prom_line(line) else {
            continue;
        };
        let Ok(val) = rest.trim().parse::<f64>() else {
            continue;
        };
        if name.contains("request_success") {
            if line.contains("finished_reason=\"abort\"")
                || line.contains("finished_reason=\"error\"")
            {
                aborted += val.max(0.0);
            } else {
                succeeded += val.max(0.0);
            }
        }
        // Match both `vllm:` and `vllm_` prefixes across versions.
        let n = name.replace(':', "_");
        if n.contains("num_requests_waiting") || n.ends_with("num_requests_waiting") {
            m.queue_depth = val.max(0.0) as u32;
        } else if n.contains("num_requests_running") {
            m.active_requests = val.max(0.0) as u32;
        } else if n.contains("gpu_cache_usage") {
            m.gpu_cache_usage = val.clamp(0.0, 1.0);
        } else if n.contains("time_to_first_token_seconds_sum") {
            // Prefer histogram sum/count when both present — handled below.
            m.ttft_ms = val * 1000.0;
        } else if n.contains("time_to_first_token_seconds_count") && val > 0.0 {
            if m.ttft_ms > 0.0 {
                // reinterpret sum we stored as sum; convert to mean ms
                let sum_s = m.ttft_ms / 1000.0;
                m.ttft_ms = (sum_s / val) * 1000.0;
            }
        } else if n.contains("avg_generation_throughput")
            || ((n.contains("avg_prompt_throughput") || n.contains("prompt_tokens_total"))
                && m.tokens_per_sec <= 0.0)
        {
            m.tokens_per_sec = val.max(0.0);
        }
    }
    let finished = succeeded + aborted;
    if finished > 0.0 {
        m.http_error_rate = (aborted / finished).clamp(0.0, 1.0);
    }
    m
}

fn split_prom_line(line: &str) -> Option<(&str, &str)> {
    // name{labels} value   OR   name value
    let (left, right) = line
        .rfind(' ')
        .map(|idx| (&line[..idx], &line[idx + 1..]))?;
    let name = if let Some(brace) = left.find('{') {
        &left[..brace]
    } else {
        left
    };
    Some((name, right))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::ai::types::InferenceReplica;

    fn rep(name: &str, queue: u32, ttft: f64, cache: f64) -> InferenceReplica {
        InferenceReplica {
            replica_id: name.into(),
            ordinal: 0,
            vm_name: name.into(),
            bdf: "0000:01:00.0".into(),
            ready: true,
            address: Some("10.0.0.1".into()),
            metrics: Some(ReplicaMetrics {
                queue_depth: queue,
                ttft_ms: ttft,
                gpu_cache_usage: cache,
                ..Default::default()
            }),
            maglev_weight: None,
            site: None,
            cost_tier: None,
            draining: false,
            unhealthy_streak: 0,
            deployment: String::new(),
            revision: 0,
            model_digest: String::new(),
            profile_digest: String::new(),
            host: String::new(),
            generation: 0,
            lifecycle: String::new(),
            health: String::new(),
            created_at: None,
        }
    }

    #[test]
    fn least_queue_prefers_idle() {
        let a = rep("a", 0, 0.0, 0.0);
        let b = rep("b", 30, 0.0, 0.0);
        let ready = vec![&a, &b];
        let w = compute_weights(RoutingStrategy::LeastQueue, &ready);
        assert_eq!(w[0], 32);
        assert_eq!(w[1], 3); // 33 - 30
        assert!(w[0] > w[1]);
    }

    #[test]
    fn equal_is_one() {
        let a = rep("a", 5, 0.0, 0.0);
        let w = compute_weights(RoutingStrategy::Equal, &[&a]);
        assert_eq!(w, vec![1]);
    }

    #[test]
    fn most_free_vram() {
        let a = rep("a", 0, 0.0, 0.1);
        let b = rep("b", 0, 0.0, 0.9);
        let w = compute_weights(RoutingStrategy::MostFreeVram, &[&a, &b]);
        assert!(w[0] > w[1]);
    }

    #[test]
    fn highest_throughput_prefers_the_faster_replica() {
        let mut fast = rep("fast", 0, 0.0, 0.0);
        let mut slow = rep("slow", 0, 0.0, 0.0);
        fast.metrics.as_mut().unwrap().tokens_per_sec = 80.0;
        slow.metrics.as_mut().unwrap().tokens_per_sec = 20.0;
        let w = compute_weights(RoutingStrategy::HighestThroughput, &[&fast, &slow]);
        assert!(w[0] > w[1]);
    }

    #[test]
    fn lowest_failure_prefers_the_cleaner_replica() {
        let mut clean = rep("clean", 0, 0.0, 0.0);
        let mut failing = rep("failing", 0, 0.0, 0.0);
        clean.metrics.as_mut().unwrap().http_error_rate = 0.0;
        failing.metrics.as_mut().unwrap().http_error_rate = 0.5;
        let w = compute_weights(RoutingStrategy::LowestFailure, &[&clean, &failing]);
        assert!(w[0] > w[1]);
    }

    #[test]
    fn parse_vllm_throughput_and_failures() {
        let text = r#"
vllm:avg_generation_throughput_toks_per_s 42.5
vllm:request_success_total{finished_reason="stop"} 8
vllm:request_success_total{finished_reason="error"} 2
"#;
        let m = parse_vllm_prometheus(text);
        assert!((m.tokens_per_sec - 42.5).abs() < 0.001);
        assert!((m.http_error_rate - 0.2).abs() < 0.001);
    }

    #[test]
    fn throughput_and_failure_change_weight() {
        let mut fast = rep("fast", 0, 0.0, 0.0);
        let mut slow = rep("slow", 0, 0.0, 0.0);
        fast.metrics.as_mut().unwrap().tokens_per_sec = 80.0;
        slow.metrics.as_mut().unwrap().tokens_per_sec = 20.0;
        let w = compute_weights(RoutingStrategy::HighestThroughput, &[&fast, &slow]);
        assert!(w[0] > w[1]);
        fast.metrics.as_mut().unwrap().http_error_rate = 0.0;
        slow.metrics.as_mut().unwrap().http_error_rate = 0.5;
        let w = compute_weights(RoutingStrategy::LowestFailure, &[&fast, &slow]);
        assert!(w[0] > w[1]);
    }

    #[test]
    fn parse_vllm_waiting_and_cache() {
        let text = r#"
# HELP vllm:num_requests_waiting ...
vllm:num_requests_waiting 7
vllm:num_requests_running 2
vllm:gpu_cache_usage_perc 0.42
vllm:avg_generation_throughput_toks_per_s 40
vllm:request_success_total{finished_reason="stop"} 8
vllm:request_success_total{finished_reason="abort"} 2
"#;
        let m = parse_vllm_prometheus(text);
        assert_eq!(m.queue_depth, 7);
        assert_eq!(m.active_requests, 2);
        assert!((m.gpu_cache_usage - 0.42).abs() < 0.001);
        assert!((m.tokens_per_sec - 40.0).abs() < 0.001);
        assert!((m.http_error_rate - 0.2).abs() < 0.001);
    }

    #[test]
    fn residency_excludes_untagged_replicas() {
        let mut tagged = rep("tagged", 0, 0.0, 0.0);
        tagged.site = Some("lab".into());
        tagged.metrics.as_mut().unwrap().scraped_at = Some(Utc::now());
        tagged.metrics.as_mut().unwrap().source = Some("vllm_prometheus".into());
        let mut untagged = rep("untagged", 0, 0.0, 0.0);
        untagged.metrics.as_mut().unwrap().scraped_at = Some(Utc::now());
        untagged.metrics.as_mut().unwrap().source = Some("vllm_prometheus".into());
        let ep = InferenceEndpoint {
            name: "ep".into(),
            deployment: "dep".into(),
            protocol: "openai".into(),
            port: 8000,
            service_id: None,
            vip: None,
            routing_strategy: RoutingStrategy::LeastQueue,
            tenant: None,
            preferred_site: None,
            allowed_sites: Vec::new(),
            residency: Some("lab".into()),
            phase: String::new(),
            created: Utc::now(),
            updated: Utc::now(),
        };
        let replicas = [tagged, untagged];
        let kept = filter_replicas_for_endpoint(&ep, &replicas);
        assert_eq!(kept.len(), 1);
        assert_eq!(kept[0].vm_name, "tagged");
    }
}
