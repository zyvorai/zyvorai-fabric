// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: LicenseRef-Zyvor-Production-1.0
//!
//! Drain and rollout helpers (Phase 3, preview).

use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use chrono::Utc;
use security::RequireWrite;
use std::sync::Arc;

use crate::server::AppState;

use super::maglev::{build_maglev_service_spec_weighted, drain_backend};
use super::reconcile;
use super::types::{
    DrainRequest, InferenceDeployment, InferenceEndpoint, RolloutRequest, RolloutSpec,
    RolloutStrategy,
};
use super::{audit, err, fluxvm_client, STORE_DEPLOYMENTS, STORE_ENDPOINTS};

/// POST /api/ai/deployments/{name}/drain
pub async fn drain_deployment(
    RequireWrite(claims): RequireWrite,
    State(state): State<Arc<AppState>>,
    Path(name): Path<String>,
    Json(req): Json<DrainRequest>,
) -> Result<Json<InferenceDeployment>, (StatusCode, Json<serde_json::Value>)> {
    let mut dep = load_scoped(&state, &claims, &name)?;

    let targets: Vec<String> = match &req.replica {
        Some(r) => {
            if !dep.status.replicas.iter().any(|x| x.vm_name == *r) {
                return Err(err(
                    StatusCode::NOT_FOUND,
                    format!("replica '{r}' not found"),
                ));
            }
            vec![r.clone()]
        }
        None => dep
            .status
            .replicas
            .iter()
            .map(|r| r.vm_name.clone())
            .collect(),
    };

    for rep in &mut dep.status.replicas {
        if targets.iter().any(|t| t == &rep.vm_name) {
            rep.draining = true;
            rep.maglev_weight = Some(1);
            if let Some(addr) = rep.address.clone() {
                let _ = drain_endpoints_for(&state, &dep.name, &addr, req.grace_seconds).await;
            }
        }
    }

    dep.status.phase = "Draining".into();
    dep.status.message = Some(format!("draining {} replica(s)", targets.len()));
    dep.revision = dep.revision.saturating_add(1);
    dep.updated = Utc::now();
    state
        .store
        .save_entity(STORE_DEPLOYMENTS, &dep.name, &dep)
        .map_err(|e| err(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    audit(
        &state,
        &claims.sub,
        "DRAIN",
        &format!("ai/deployments/{}", dep.name),
        "SUCCESS",
    );
    Ok(Json(dep))
}

/// POST /api/ai/deployments/{name}/rollout
pub async fn rollout_deployment(
    RequireWrite(claims): RequireWrite,
    State(state): State<Arc<AppState>>,
    Path(name): Path<String>,
    Json(req): Json<RolloutRequest>,
) -> Result<Json<InferenceDeployment>, (StatusCode, Json<serde_json::Value>)> {
    let mut dep = load_scoped(&state, &claims, &name)?;

    if let Some(ref target) = req.target_model {
        let model_ok = state
            .store
            .get_entity::<super::types::ModelArtifact>(super::STORE_MODELS, target)
            .ok()
            .flatten()
            .is_some();
        if !model_ok {
            return Err(err(
                StatusCode::BAD_REQUEST,
                format!("target model '{target}' not found"),
            ));
        }
    }

    // Rollback gate from live metrics.
    if should_rollback(&dep, req.rollback_error_rate, req.rollback_ttft_ms) {
        dep.status.phase = "Rollback".into();
        dep.status.message = Some("rollout aborted: error/TTFT threshold exceeded".into());
        dep.updated = Utc::now();
        state
            .store
            .save_entity(STORE_DEPLOYMENTS, &dep.name, &dep)
            .map_err(|e| err(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
        return Err(err(
            StatusCode::CONFLICT,
            "rollout aborted: live metrics exceed rollback thresholds",
        ));
    }

    let to_revision = dep.revision.saturating_add(1);
    let model_name = req
        .target_model
        .clone()
        .unwrap_or_else(|| dep.model.clone());
    let rev = super::types::InferenceDeploymentRevision {
        id: super::revisions::revision_id(&dep.name, to_revision),
        deployment: dep.name.clone(),
        revision: to_revision,
        model: model_name,
        model_digest: String::new(),
        profile: dep.profile.clone(),
        runtime_config_digest: String::new(),
        desired_replicas: dep.replicas,
        created_at: Utc::now(),
        state: super::types::RevisionState::Pending,
    };
    state
        .store
        .save_entity(super::STORE_REVISIONS, &rev.id, &rev)
        .map_err(|e| err(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let canary = if matches!(req.strategy, RolloutStrategy::Canary) {
        req.canary_percent.clamp(1, 50)
    } else {
        0
    };
    dep.rollout = Some(RolloutSpec {
        strategy: req.strategy,
        canary_percent: canary,
        target_model: req.target_model.clone(),
        rollback_error_rate: req.rollback_error_rate,
        rollback_ttft_ms: req.rollback_ttft_ms,
    });
    let steps = if matches!(req.strategy, RolloutStrategy::Canary) {
        super::revisions::default_canary_steps()
    } else {
        vec![]
    };
    let run = super::types::RolloutRun {
        id: format!("{}--rollout-{}", dep.name, to_revision),
        deployment: dep.name.clone(),
        strategy: req.strategy,
        from_revision: dep.revision,
        to_revision,
        max_surge: 1,
        max_unavailable: 1,
        min_ready_secs: 0,
        phase: super::types::RolloutPhase::Progressing,
        step_index: 0,
        steps,
        rollback_error_rate: req.rollback_error_rate,
        rollback_ttft_ms: req.rollback_ttft_ms,
        rollback_window_secs: 600,
        step_started_unix: 0,
        promoted_at: None,
        message: Some("rollout persisted; controller will resume".into()),
        created_at: Utc::now(),
        updated_at: Utc::now(),
    };
    state
        .store
        .save_entity(super::STORE_ROLLOUTS, &run.id, &run)
        .map_err(|e| err(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    dep.status.phase = "Rolling".into();
    dep.status.message = Some(format!(
        "rollout {} -> {} persisted",
        run.from_revision, run.to_revision
    ));
    dep.updated = Utc::now();
    state
        .store
        .save_entity(STORE_DEPLOYMENTS, &dep.name, &dep)
        .map_err(|e| err(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    reconcile::enqueue_deployment_reconcile(state.clone(), dep.name.clone());

    audit(
        &state,
        &claims.sub,
        "ROLLOUT",
        &format!("ai/deployments/{}", dep.name),
        "SUCCESS",
    );
    Ok(Json(dep))
}

fn should_rollback(dep: &InferenceDeployment, err_thresh: f64, ttft_thresh: f64) -> bool {
    let ready: Vec<_> = dep
        .status
        .replicas
        .iter()
        .filter(|r| r.ready)
        .filter_map(|r| r.metrics.as_ref())
        .collect();
    if ready.is_empty() {
        return false;
    }
    let n = ready.len() as f64;
    let avg_err = ready.iter().map(|m| m.http_error_rate).sum::<f64>() / n;
    let avg_ttft = ready.iter().map(|m| m.ttft_ms).sum::<f64>() / n;
    (err_thresh > 0.0 && avg_err > err_thresh) || (ttft_thresh > 0.0 && avg_ttft > ttft_thresh)
}

async fn drain_endpoints_for(
    state: &AppState,
    deployment: &str,
    address: &str,
    grace_seconds: u64,
) -> Result<(), String> {
    let dry_run = std::env::var("FLUXVM_AI_DRY_RUN")
        .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
        .unwrap_or(false);
    if dry_run {
        return Ok(());
    }
    let endpoints: Vec<InferenceEndpoint> = state
        .store
        .list_entities(STORE_ENDPOINTS)
        .unwrap_or_default();
    let client = fluxvm_client(state).map_err(|(_, b)| {
        b.0.get("error")
            .and_then(|v| v.as_str())
            .unwrap_or("fluxvm")
            .to_string()
    })?;
    for ep in endpoints.into_iter().filter(|e| e.deployment == deployment) {
        let Some(vip) = ep.vip.as_deref() else {
            continue;
        };
        let dep: InferenceDeployment = state
            .store
            .get_entity(STORE_DEPLOYMENTS, deployment)
            .ok()
            .flatten()
            .ok_or_else(|| "deployment missing".to_string())?;
        let mut spec =
            build_maglev_service_spec_weighted(&ep.name, vip, ep.port, &dep.status.replicas, None);
        drain_backend(&mut spec, address);
        // Extend drain window if caller asked for longer grace.
        let now_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis() as u64)
            .unwrap_or(0);
        for b in &mut spec.backends {
            if b.address == address {
                b.drain_until_unix_ms =
                    Some(now_ms.saturating_add(grace_seconds.saturating_mul(1000)));
            }
        }
        let _ = client.upsert_network_service(&spec).await;
    }
    Ok(())
}

pub(crate) async fn set_rollout_phase(
    state: &AppState,
    claims: &security::Claims,
    deployment: &str,
    id: &str,
    phase: super::types::RolloutPhase,
) -> Result<Json<super::types::RolloutRun>, (StatusCode, Json<serde_json::Value>)> {
    let _ = load_scoped(state, claims, deployment)?;
    let mut run = state
        .store
        .get_entity::<super::types::RolloutRun>(super::STORE_ROLLOUTS, id)
        .map_err(|e| err(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
        .ok_or_else(|| err(StatusCode::NOT_FOUND, "rollout not found"))?;
    if run.deployment != deployment {
        return Err(err(StatusCode::NOT_FOUND, "rollout not found"));
    }
    run.phase = phase;
    run.updated_at = Utc::now();
    state
        .store
        .save_entity(super::STORE_ROLLOUTS, id, &run)
        .map_err(|e| err(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    Ok(Json(run))
}

/// POST /api/ai/deployments/{name}/rollouts
pub async fn create_rollout(
    claims: RequireWrite,
    state: State<Arc<AppState>>,
    name: Path<String>,
    Json(body): Json<super::types::CreateRolloutBody>,
) -> Result<Json<InferenceDeployment>, (StatusCode, Json<serde_json::Value>)> {
    let req = RolloutRequest {
        strategy: body.strategy,
        canary_percent: body.steps.first().map(|s| s.weight).unwrap_or(5),
        target_model: body.target_model,
        rollback_error_rate: body.rollback_error_rate,
        rollback_ttft_ms: body.rollback_ttft_ms,
    };
    rollout_deployment(claims, state, name, Json(req)).await
}

pub async fn pause_rollout(
    RequireWrite(claims): RequireWrite,
    State(state): State<Arc<AppState>>,
    Path((name, id)): Path<(String, String)>,
) -> Result<Json<super::types::RolloutRun>, (StatusCode, Json<serde_json::Value>)> {
    set_rollout_phase(
        &state,
        &claims,
        &name,
        &id,
        super::types::RolloutPhase::Paused,
    )
    .await
}

pub async fn resume_rollout(
    RequireWrite(claims): RequireWrite,
    State(state): State<Arc<AppState>>,
    Path((name, id)): Path<(String, String)>,
) -> Result<Json<super::types::RolloutRun>, (StatusCode, Json<serde_json::Value>)> {
    set_rollout_phase(
        &state,
        &claims,
        &name,
        &id,
        super::types::RolloutPhase::Progressing,
    )
    .await
}

pub async fn promote_rollout(
    RequireWrite(claims): RequireWrite,
    State(state): State<Arc<AppState>>,
    Path((name, id)): Path<(String, String)>,
) -> Result<Json<super::types::RolloutRun>, (StatusCode, Json<serde_json::Value>)> {
    let mut run = set_rollout_phase(
        &state,
        &claims,
        &name,
        &id,
        super::types::RolloutPhase::Succeeded,
    )
    .await?
    .0;
    if let Ok(Some(mut dep)) = state
        .store
        .get_entity::<InferenceDeployment>(STORE_DEPLOYMENTS, &name)
    {
        if let Some(rev) = super::revisions::list_revisions(&state, &name)
            .into_iter()
            .find(|r| r.revision == run.to_revision)
        {
            dep.model = rev.model;
            dep.revision = rev.revision;
        }
        for rep in &mut dep.status.replicas {
            if rep.revision == run.to_revision {
                rep.draining = false;
                rep.maglev_weight = Some(32);
            } else if rep.revision == run.from_revision {
                rep.draining = true;
                rep.maglev_weight = Some(1);
            }
        }
        dep.updated = Utc::now();
        let _ = state.store.save_entity(STORE_DEPLOYMENTS, &dep.name, &dep);
        super::revisions::note_revision_states(
            &state,
            &name,
            run.from_revision,
            run.to_revision,
            super::types::RevisionState::Superseded,
            super::types::RevisionState::Active,
        );
    }
    run.promoted_at = Some(Utc::now());
    run.message = Some("promoted".into());
    let _ = state.store.save_entity(super::STORE_ROLLOUTS, &id, &run);
    Ok(Json(run))
}

pub async fn rollback_rollout(
    RequireWrite(claims): RequireWrite,
    State(state): State<Arc<AppState>>,
    Path((name, id)): Path<(String, String)>,
) -> Result<Json<super::types::RolloutRun>, (StatusCode, Json<serde_json::Value>)> {
    let mut run = set_rollout_phase(
        &state,
        &claims,
        &name,
        &id,
        super::types::RolloutPhase::RollingBack,
    )
    .await?
    .0;
    if let Ok(Some(mut dep)) = state
        .store
        .get_entity::<InferenceDeployment>(STORE_DEPLOYMENTS, &name)
    {
        super::revisions::restore_old_weights(
            &mut dep.status.replicas,
            run.from_revision,
            run.to_revision,
        );
        dep.revision = run.from_revision;
        dep.updated = Utc::now();
        let _ = state.store.save_entity(STORE_DEPLOYMENTS, &dep.name, &dep);
        super::revisions::note_revision_states(
            &state,
            &name,
            run.from_revision,
            run.to_revision,
            super::types::RevisionState::Active,
            super::types::RevisionState::RollingBack,
        );
    }
    run.message = Some("previous revision restored".into());
    let _ = state.store.save_entity(super::STORE_ROLLOUTS, &id, &run);
    Ok(Json(run))
}

pub(crate) fn load_scoped(
    state: &AppState,
    claims: &security::Claims,
    name: &str,
) -> Result<InferenceDeployment, (StatusCode, Json<serde_json::Value>)> {
    let dep = state
        .store
        .get_entity::<InferenceDeployment>(STORE_DEPLOYMENTS, name)
        .map_err(|e| err(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
        .ok_or_else(|| err(StatusCode::NOT_FOUND, "InferenceDeployment not found"))?;

    if let Some(ref claim_tenant) = claims.tenant {
        if dep.tenant.as_deref() != Some(claim_tenant.as_str()) {
            return Err(err(StatusCode::NOT_FOUND, "InferenceDeployment not found"));
        }
    }
    Ok(dep)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::ai::types::{InferenceReplica, ReplicaMetrics};

    #[test]
    fn rollback_on_high_error() {
        let dep = InferenceDeployment {
            name: "d".into(),
            model: "m".into(),
            profile: "p".into(),
            replicas: 1,
            gpus_per_replica: 1,
            tenant: None,
            autoscaling: Default::default(),
            rollout: None,
            revision: 0,
            preferred_site: None,
            residency: None,
            allowed_sites: vec![],
            failover_sites: vec![],
            minimum_sites: 0,
            max_replicas_per_site: 0,
            status: crate::api::ai::types::InferenceDeploymentStatus {
                phase: "Ready".into(),
                replicas: vec![InferenceReplica {
                    replica_id: "d-r000000".into(),
                    ordinal: 0,
                    vm_name: "d-0".into(),
                    bdf: "x".into(),
                    ready: true,
                    address: None,
                    metrics: Some(ReplicaMetrics {
                        http_error_rate: 0.5,
                        ttft_ms: 100.0,
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
                }],
                message: None,
            },
            created: Utc::now(),
            updated: Utc::now(),
        };
        assert!(should_rollback(&dep, 0.15, 2000.0));
        assert!(!should_rollback(&dep, 0.9, 2000.0));
    }
}
