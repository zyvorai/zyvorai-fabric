// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: LicenseRef-Zyvor-Production-2.0

//! Answers "why this host?" from the same scheduler used to place replicas.

use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use chrono::Utc;
use security::RequireRead;
use serde_json::json;
use std::sync::Arc;

use crate::server::AppState;

use super::types::{InferenceDeployment, InferenceEndpoint, InferenceProfile, InferenceReplica};
use super::{err, scheduler, STORE_DEPLOYMENTS, STORE_ENDPOINTS, STORE_NODES, STORE_PROFILES};

/// GET /api/ai/explain/placement/{deployment}
pub async fn explain_placement(
    RequireRead(_claims): RequireRead,
    State(state): State<Arc<AppState>>,
    Path(name): Path<String>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let dep = state
        .store
        .get_entity::<InferenceDeployment>(STORE_DEPLOYMENTS, &name)
        .map_err(|e| err(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
        .ok_or_else(|| err(StatusCode::NOT_FOUND, "InferenceDeployment not found"))?;
    let profile = state
        .store
        .get_entity::<InferenceProfile>(STORE_PROFILES, &dep.profile)
        .map_err(|e| err(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
        .ok_or_else(|| err(StatusCode::NOT_FOUND, "profile not found"))?;
    let nodes = state.store.list_entities(STORE_NODES).unwrap_or_default();
    let request = super::reconcile::schedule_request(&state, &dep, &profile);
    let (choice, rejected) = scheduler::explain(&nodes, &request, Utc::now().timestamp());
    let sites = super::federation::distinct_node_sites(&nodes);
    Ok(Json(json!({
        "deployment": name,
        "chosen": choice,
        "rejected": rejected.into_iter().map(|(id, reason)| json!({"node": id, "reason": reason})).collect::<Vec<_>>(),
        "minimum_sites": dep.minimum_sites,
        "sites_satisfied": super::federation::meets_minimum_sites(sites, dep.minimum_sites),
        "phase": dep.status.phase,
        "message": dep.status.message,
    })))
}

/// GET /api/ai/explain/scaling/{deployment}
pub async fn explain_scaling(
    RequireRead(_claims): RequireRead,
    State(state): State<Arc<AppState>>,
    Path(name): Path<String>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let dep = load_deployment(&state, &name)?;
    let signal = super::autoscaling::explain_scale(&dep);
    Ok(Json(json!({
        "deployment": name,
        "action": signal.action,
        "reason": signal.reason,
        "current": signal.current,
        "next": signal.next,
        "ready": signal.ready,
        "avg_queue": signal.avg_queue,
        "avg_ttft": signal.avg_ttft,
        "waits_seconds": if signal.action == "scale_out" {
            dep.autoscaling.scale_out_seconds
        } else if signal.action == "scale_in" {
            dep.autoscaling.scale_in_seconds
        } else {
            0
        },
        "message": dep.status.message,
    })))
}

/// GET /api/ai/explain/routing/{endpoint}
pub async fn explain_routing(
    RequireRead(_claims): RequireRead,
    State(state): State<Arc<AppState>>,
    Path(name): Path<String>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let ep = state
        .store
        .get_entity::<InferenceEndpoint>(STORE_ENDPOINTS, &name)
        .map_err(|e| err(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
        .ok_or_else(|| err(StatusCode::NOT_FOUND, "InferenceEndpoint not found"))?;
    let dep = load_deployment(&state, &ep.deployment)?;
    let replicas = dep
        .status
        .replicas
        .iter()
        .map(|rep| {
            let reason = super::routing::routing_exclusion(&ep, rep);
            json!({
                "replica": replica_label(rep),
                "included": reason.is_none(),
                "reason": reason.unwrap_or("included"),
                "site": rep.site,
                "weight": rep.maglev_weight,
            })
        })
        .collect::<Vec<_>>();
    Ok(Json(json!({
        "endpoint": name,
        "deployment": ep.deployment,
        "residency": ep.residency,
        "allowed_sites": ep.allowed_sites,
        "replicas": replicas,
    })))
}

/// GET /api/ai/explain/failure/{replica}
pub async fn explain_failure(
    RequireRead(_claims): RequireRead,
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let deps: Vec<InferenceDeployment> = state
        .store
        .list_entities(STORE_DEPLOYMENTS)
        .map_err(|e| err(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    let (dep, rep) = deps
        .iter()
        .find_map(|dep| {
            dep.status
                .replicas
                .iter()
                .find(|rep| rep.replica_id == id || rep.vm_name == id)
                .map(|rep| (dep, rep))
        })
        .ok_or_else(|| err(StatusCode::NOT_FOUND, "replica not found"))?;
    let reasons = failure_reasons(rep);
    Ok(Json(json!({
        "deployment": dep.name,
        "replica": replica_label(rep),
        "ready": rep.ready,
        "lifecycle": rep.lifecycle,
        "health": rep.health,
        "unhealthy_streak": rep.unhealthy_streak,
        "reasons": reasons,
        "message": dep.status.message,
    })))
}

fn load_deployment(
    state: &AppState,
    name: &str,
) -> Result<InferenceDeployment, (StatusCode, Json<serde_json::Value>)> {
    state
        .store
        .get_entity(STORE_DEPLOYMENTS, name)
        .map_err(|e| err(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
        .ok_or_else(|| err(StatusCode::NOT_FOUND, "InferenceDeployment not found"))
}

fn replica_label(rep: &InferenceReplica) -> &str {
    if rep.replica_id.is_empty() {
        &rep.vm_name
    } else {
        &rep.replica_id
    }
}

/// Recorded reasons a replica is not a healthy serving backend.
pub fn failure_reasons(rep: &InferenceReplica) -> Vec<&'static str> {
    let mut reasons = Vec::new();
    if !rep.ready {
        reasons.push("not ready");
    }
    if rep.draining {
        reasons.push("draining");
    }
    if rep.unhealthy_streak > 0 {
        reasons.push("health probe failures");
    }
    if rep.lifecycle == "failed" {
        reasons.push("lifecycle failed");
    }
    if rep.health == "unhealthy" {
        reasons.push("health unhealthy");
    }
    if rep.host.is_empty() {
        reasons.push("no host");
    }
    if rep.address.is_none() {
        reasons.push("no address");
    }
    if reasons.is_empty() {
        reasons.push("no failure recorded");
    }
    reasons
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::ai::types::{AutoscalingPolicy, InferenceDeploymentStatus, ReplicaMetrics};
    use chrono::Utc;

    fn replica(id: &str, ready: bool, site: Option<&str>) -> InferenceReplica {
        InferenceReplica {
            replica_id: id.into(),
            ordinal: 0,
            vm_name: format!("{id}-vm"),
            bdf: "0000:01:00.0".into(),
            ready,
            address: Some("10.0.0.8".into()),
            metrics: Some(ReplicaMetrics {
                queue_depth: 30,
                ttft_ms: 100.0,
                source: Some("vllm_prometheus".into()),
                scraped_at: Some(Utc::now()),
                ..Default::default()
            }),
            maglev_weight: None,
            site: site.map(str::to_string),
            cost_tier: None,
            draining: false,
            unhealthy_streak: 0,
            deployment: "d".into(),
            revision: 1,
            model_digest: String::new(),
            profile_digest: String::new(),
            host: "node-a".into(),
            generation: 1,
            lifecycle: if ready { "ready" } else { "pending" }.into(),
            health: if ready { "healthy" } else { "unknown" }.into(),
            created_at: None,
        }
    }

    fn deployment(replicas: Vec<InferenceReplica>, enabled: bool) -> InferenceDeployment {
        InferenceDeployment {
            name: "d".into(),
            model: "m".into(),
            profile: "p".into(),
            replicas: 1,
            gpus_per_replica: 1,
            tenant: None,
            autoscaling: AutoscalingPolicy {
                enabled,
                min_replicas: 1,
                max_replicas: 3,
                ..AutoscalingPolicy::default()
            },
            rollout: None,
            revision: 1,
            preferred_site: None,
            residency: None,
            allowed_sites: vec![],
            failover_sites: vec![],
            minimum_sites: 0,
            max_replicas_per_site: 0,
            status: InferenceDeploymentStatus {
                phase: "Ready".into(),
                replicas,
                message: None,
            },
            created: Utc::now(),
            updated: Utc::now(),
        }
    }

    #[test]
    fn scale_explain_holds_when_disabled_and_scales_on_queue() {
        let dep = deployment(vec![replica("d-0", true, None)], false);
        assert_eq!(
            super::super::autoscaling::explain_scale(&dep).reason,
            "autoscaling disabled"
        );
        let dep = deployment(vec![replica("d-0", true, None)], true);
        let signal = super::super::autoscaling::explain_scale(&dep);
        assert_eq!(signal.action, "scale_out");
        assert_eq!(signal.next, 2);
    }

    #[test]
    fn routing_excludes_residency_mismatch() {
        let ep = InferenceEndpoint {
            name: "ep".into(),
            deployment: "d".into(),
            protocol: "openai".into(),
            port: 8000,
            service_id: None,
            vip: None,
            routing_strategy: Default::default(),
            tenant: None,
            preferred_site: None,
            allowed_sites: vec!["pune-1".into()],
            residency: Some("india".into()),
            phase: String::new(),
            created: Utc::now(),
            updated: Utc::now(),
        };
        let outside = replica("d-0", true, Some("mumbai-1"));
        assert_eq!(
            super::super::routing::routing_exclusion(&ep, &outside),
            Some("outside residency")
        );
        let local = replica("d-1", true, Some("india"));
        assert_eq!(
            super::super::routing::routing_exclusion(&ep, &local),
            Some("site not allowed")
        );
    }

    #[test]
    fn failure_lists_pending_replica() {
        let mut rep = replica("d-0", false, None);
        rep.address = None;
        rep.host.clear();
        let reasons = failure_reasons(&rep);
        assert!(reasons.contains(&"not ready"));
        assert!(reasons.contains(&"no host"));
        assert!(reasons.contains(&"no address"));
    }
}
