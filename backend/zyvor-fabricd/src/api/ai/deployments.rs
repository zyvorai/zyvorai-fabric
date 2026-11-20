// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: LicenseRef-Zyvor-Production-1.0

//! InferenceDeployment REST handlers (AI Inference MVP, preview).

use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    Json,
};
use chrono::Utc;
use security::{RequireRead, RequireWrite};
use std::sync::Arc;

use crate::server::AppState;

use super::types::{
    CreateInferenceDeploymentRequest, InferenceDeployment, InferenceDeploymentStatus,
    InferenceProfile, ModelArtifact, ScaleInferenceDeploymentRequest, TenantQuery,
};
use super::{audit, err, reconcile, STORE_DEPLOYMENTS, STORE_MODELS, STORE_PROFILES};

/// GET /api/ai/deployments
pub async fn list_deployments(
    RequireRead(claims): RequireRead,
    State(state): State<Arc<AppState>>,
    Query(q): Query<TenantQuery>,
) -> Result<Json<Vec<InferenceDeployment>>, (StatusCode, Json<serde_json::Value>)> {
    let tenant_filter = crate::tenant_scope::apply_list_tenant_filter(&claims, q.tenant)
        .map_err(|(s, m)| err(s, m))?;

    let mut items: Vec<InferenceDeployment> = state
        .store
        .list_entities(STORE_DEPLOYMENTS)
        .map_err(|e| err(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    if let Some(ref t) = tenant_filter {
        items.retain(|d| d.tenant.as_deref() == Some(t.as_str()));
    }
    items.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(Json(items))
}

/// GET /api/ai/deployments/{name}
pub async fn get_deployment(
    RequireRead(claims): RequireRead,
    State(state): State<Arc<AppState>>,
    Path(name): Path<String>,
) -> Result<Json<InferenceDeployment>, (StatusCode, Json<serde_json::Value>)> {
    let dep = load_scoped(&state, &claims, &name)?;
    Ok(Json(dep))
}

/// GET /api/ai/deployments/{name}/metrics — last scraped replica metrics + Maglev weights.
pub async fn deployment_metrics(
    RequireRead(claims): RequireRead,
    State(state): State<Arc<AppState>>,
    Path(name): Path<String>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let dep = load_scoped(&state, &claims, &name)?;
    let replicas: Vec<serde_json::Value> = dep
        .status
        .replicas
        .iter()
        .map(|r| {
            serde_json::json!({
                "vm_name": r.vm_name,
                "bdf": r.bdf,
                "ready": r.ready,
                "address": r.address,
                "maglev_weight": r.maglev_weight,
                "metrics": r.metrics,
            })
        })
        .collect();
    Ok(Json(serde_json::json!({
        "deployment": dep.name,
        "phase": dep.status.phase,
        "replicas": replicas,
    })))
}

/// POST /api/ai/deployments
pub async fn create_deployment(
    RequireWrite(claims): RequireWrite,
    State(state): State<Arc<AppState>>,
    Json(mut req): Json<CreateInferenceDeploymentRequest>,
) -> Result<(StatusCode, Json<InferenceDeployment>), (StatusCode, Json<serde_json::Value>)> {
    crate::validation::validate_entity_name(&req.name).map_err(|(s, m)| err(s, m))?;
    let autoscaling_preview = req.autoscaling.clone().unwrap_or_default();
    if req.replicas == 0 && !autoscaling_preview.scale_to_zero {
        return Err(err(
            StatusCode::BAD_REQUEST,
            "replicas must be greater than 0 (or enable autoscaling.scale_to_zero)",
        ));
    }

    req.tenant = crate::tenant_scope::apply_create_tenant(&claims, req.tenant)
        .map_err(|(s, m)| err(s, m))?;

    let model = state
        .store
        .get_entity::<ModelArtifact>(STORE_MODELS, &req.model)
        .map_err(|e| err(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
        .ok_or_else(|| {
            err(
                StatusCode::BAD_REQUEST,
                format!("ModelArtifact '{}' not found", req.model),
            )
        })?;

    let profile = state
        .store
        .get_entity::<InferenceProfile>(STORE_PROFILES, &req.profile)
        .map_err(|e| err(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
        .ok_or_else(|| {
            err(
                StatusCode::BAD_REQUEST,
                format!("InferenceProfile '{}' not found", req.profile),
            )
        })?;

    if let Some(ref t) = req.tenant {
        if model.tenant.as_ref().is_some_and(|mt| mt != t) {
            return Err(err(
                StatusCode::FORBIDDEN,
                "model tenant does not match deployment tenant",
            ));
        }
        if profile.tenant.as_ref().is_some_and(|pt| pt != t) {
            return Err(err(
                StatusCode::FORBIDDEN,
                "profile tenant does not match deployment tenant",
            ));
        }
    }

    if state
        .store
        .get_entity::<InferenceDeployment>(STORE_DEPLOYMENTS, &req.name)
        .ok()
        .flatten()
        .is_some()
    {
        return Err(err(
            StatusCode::CONFLICT,
            format!("InferenceDeployment '{}' already exists", req.name),
        ));
    }

    let gpus_per_replica = profile.gpu.count.max(1);
    let gpus_delta = req.replicas.saturating_mul(gpus_per_replica);
    let cpus = profile.cpu.saturating_mul(req.replicas);
    let memory_mb = (profile.memory_gib as u64)
        .saturating_mul(1024)
        .saturating_mul(req.replicas as u64);

    crate::api::quotas::check_quota_enforcement(
        &state,
        cpus,
        memory_mb,
        0,
        &[],
        req.tenant.as_deref(),
        req.replicas,
        0,
        None,
        gpus_delta,
        None,
    )
    .await
    .map_err(|e| err(StatusCode::FORBIDDEN, e))?;

    let now = Utc::now();
    let autoscaling = req.autoscaling.unwrap_or_default();
    let dep = InferenceDeployment {
        name: req.name.clone(),
        model: req.model,
        profile: req.profile,
        replicas: req.replicas,
        gpus_per_replica,
        tenant: req.tenant,
        autoscaling,
        rollout: None,
        revision: 1,
        preferred_site: req.preferred_site,
        residency: req.residency.or(model.residency.clone()),
        allowed_sites: req.allowed_sites,
        failover_sites: req.failover_sites,
        minimum_sites: req.minimum_sites,
        max_replicas_per_site: req.max_replicas_per_site,
        status: InferenceDeploymentStatus {
            phase: "Pending".into(),
            replicas: vec![],
            message: Some("reconcile enqueued".into()),
        },
        created: now,
        updated: now,
    };

    super::policy::enforce(&state, dep.tenant.as_deref(), &model.name, &model.source)
        .map_err(|e| err(StatusCode::FORBIDDEN, e))?;

    state
        .store
        .save_entity(STORE_DEPLOYMENTS, &dep.name, &dep)
        .map_err(|e| err(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let digest = model.checksum.clone().unwrap_or_default();
    super::revisions::seed_revision(&state, &dep, &digest)
        .map_err(|e| err(StatusCode::INTERNAL_SERVER_ERROR, e))?;

    reconcile::enqueue_deployment_reconcile(state.clone(), dep.name.clone());

    audit(
        &state,
        &claims.sub,
        "CREATE",
        &format!("ai/deployments/{}", dep.name),
        "SUCCESS",
    );
    Ok((StatusCode::CREATED, Json(dep)))
}

/// POST /api/ai/deployments/{name}/scale
pub async fn scale_deployment(
    RequireWrite(claims): RequireWrite,
    State(state): State<Arc<AppState>>,
    Path(name): Path<String>,
    Json(req): Json<ScaleInferenceDeploymentRequest>,
) -> Result<Json<InferenceDeployment>, (StatusCode, Json<serde_json::Value>)> {
    let mut dep = load_scoped(&state, &claims, &name)?;
    if req.replicas == 0 && !dep.autoscaling.scale_to_zero {
        return Err(err(
            StatusCode::BAD_REQUEST,
            "replicas must be greater than 0 (enable autoscaling.scale_to_zero or DELETE)",
        ));
    }

    let gpus_per = dep.gpus_per_replica.max(1);
    let new_gpus = req.replicas.saturating_mul(gpus_per);
    let profile = state
        .store
        .get_entity::<InferenceProfile>(STORE_PROFILES, &dep.profile)
        .ok()
        .flatten();
    let (cpus, memory_mb) = match &profile {
        Some(p) => (
            p.cpu.saturating_mul(req.replicas),
            (p.memory_gib as u64)
                .saturating_mul(1024)
                .saturating_mul(req.replicas as u64),
        ),
        None => (0, 0),
    };

    crate::api::quotas::check_quota_enforcement(
        &state,
        cpus,
        memory_mb,
        0,
        &[],
        dep.tenant.as_deref(),
        req.replicas,
        0,
        None,
        new_gpus,
        Some(dep.name.as_str()),
    )
    .await
    .map_err(|e| err(StatusCode::FORBIDDEN, e))?;

    dep.replicas = req.replicas;
    dep.revision = dep.revision.saturating_add(1);
    dep.status.phase = "Scaling".into();
    dep.status.message = Some(format!("scaling to {} replicas", req.replicas));
    dep.updated = Utc::now();

    state
        .store
        .save_entity(STORE_DEPLOYMENTS, &dep.name, &dep)
        .map_err(|e| err(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    reconcile::enqueue_deployment_reconcile(state.clone(), dep.name.clone());

    audit(
        &state,
        &claims.sub,
        "SCALE",
        &format!("ai/deployments/{}", dep.name),
        "SUCCESS",
    );
    Ok(Json(dep))
}

/// PUT /api/ai/deployments/{name}/autoscaling
pub async fn patch_autoscaling(
    RequireWrite(claims): RequireWrite,
    State(state): State<Arc<AppState>>,
    Path(name): Path<String>,
    Json(req): Json<super::types::PatchAutoscalingRequest>,
) -> Result<Json<InferenceDeployment>, (StatusCode, Json<serde_json::Value>)> {
    let mut dep = load_scoped(&state, &claims, &name)?;
    if req.autoscaling.max_replicas < req.autoscaling.min_replicas && !req.autoscaling.scale_to_zero
    {
        return Err(err(
            StatusCode::BAD_REQUEST,
            "max_replicas must be >= min_replicas",
        ));
    }
    dep.autoscaling = req.autoscaling;
    dep.revision = dep.revision.saturating_add(1);
    dep.updated = Utc::now();
    state
        .store
        .save_entity(STORE_DEPLOYMENTS, &dep.name, &dep)
        .map_err(|e| err(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    audit(
        &state,
        &claims.sub,
        "UPDATE",
        &format!("ai/deployments/{}/autoscaling", dep.name),
        "SUCCESS",
    );
    Ok(Json(dep))
}

/// DELETE /api/ai/deployments/{name}
pub async fn delete_deployment(
    RequireWrite(claims): RequireWrite,
    State(state): State<Arc<AppState>>,
    Path(name): Path<String>,
) -> Result<StatusCode, (StatusCode, Json<serde_json::Value>)> {
    let dep = load_scoped(&state, &claims, &name)?;

    let endpoints: Vec<super::types::InferenceEndpoint> = state
        .store
        .list_entities(super::STORE_ENDPOINTS)
        .unwrap_or_default();
    if endpoints.iter().any(|e| e.deployment == name) {
        return Err(err(
            StatusCode::CONFLICT,
            format!("InferenceDeployment '{name}' still has InferenceEndpoints"),
        ));
    }

    // Tear down replicas synchronously for MVP so DELETE leaves no orphan GPUs.
    if let Err(e) = reconcile::teardown_deployment(&state, &dep).await {
        tracing::warn!("deployment teardown for '{name}': {e}");
    }

    state
        .store
        .delete_entity(STORE_DEPLOYMENTS, &name)
        .map_err(|e| err(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    audit(
        &state,
        &claims.sub,
        "DELETE",
        &format!("ai/deployments/{name}"),
        "SUCCESS",
    );
    Ok(StatusCode::NO_CONTENT)
}

fn load_scoped(
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
