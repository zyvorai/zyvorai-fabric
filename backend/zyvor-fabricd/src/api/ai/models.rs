// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: LicenseRef-Zyvor-Production-1.0

//! ModelArtifact REST handlers (AI Inference MVP, preview).

use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    Json,
};
use chrono::Utc;
use security::{RequireRead, RequireWrite};
use std::sync::Arc;

use crate::server::AppState;

use super::types::{CreateModelArtifactRequest, ModelArtifact, TenantQuery};
use super::{audit, err, STORE_MODELS};

/// GET /api/ai/models
pub async fn list_models(
    RequireRead(claims): RequireRead,
    State(state): State<Arc<AppState>>,
    Query(q): Query<TenantQuery>,
) -> Result<Json<Vec<ModelArtifact>>, (StatusCode, Json<serde_json::Value>)> {
    let tenant_filter = crate::tenant_scope::apply_list_tenant_filter(&claims, q.tenant)
        .map_err(|(s, m)| err(s, m))?;

    let mut items: Vec<ModelArtifact> = state
        .store
        .list_entities(STORE_MODELS)
        .map_err(|e| err(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    if let Some(ref t) = tenant_filter {
        items.retain(|m| m.tenant.as_deref() == Some(t.as_str()));
    }
    items.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(Json(items))
}

/// GET /api/ai/models/{name}
pub async fn get_model(
    RequireRead(claims): RequireRead,
    State(state): State<Arc<AppState>>,
    Path(name): Path<String>,
) -> Result<Json<ModelArtifact>, (StatusCode, Json<serde_json::Value>)> {
    let model = state
        .store
        .get_entity::<ModelArtifact>(STORE_MODELS, &name)
        .map_err(|e| err(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
        .ok_or_else(|| err(StatusCode::NOT_FOUND, "ModelArtifact not found"))?;

    if let Some(ref claim_tenant) = claims.tenant {
        if model.tenant.as_deref() != Some(claim_tenant.as_str()) {
            return Err(err(StatusCode::NOT_FOUND, "ModelArtifact not found"));
        }
    }
    Ok(Json(model))
}

/// POST /api/ai/models
pub async fn create_model(
    RequireWrite(claims): RequireWrite,
    State(state): State<Arc<AppState>>,
    Json(mut req): Json<CreateModelArtifactRequest>,
) -> Result<(StatusCode, Json<ModelArtifact>), (StatusCode, Json<serde_json::Value>)> {
    crate::validation::validate_entity_name(&req.name).map_err(|(s, m)| err(s, m))?;
    if req.source.trim().is_empty() {
        return Err(err(StatusCode::BAD_REQUEST, "source is required"));
    }
    if req.format.trim().is_empty() {
        return Err(err(StatusCode::BAD_REQUEST, "format is required"));
    }

    req.tenant = crate::tenant_scope::apply_create_tenant(&claims, req.tenant)
        .map_err(|(s, m)| err(s, m))?;

    if state
        .store
        .get_entity::<ModelArtifact>(STORE_MODELS, &req.name)
        .ok()
        .flatten()
        .is_some()
    {
        return Err(err(
            StatusCode::CONFLICT,
            format!("ModelArtifact '{}' already exists", req.name),
        ));
    }

    if req.require_checksum
        && req
            .checksum
            .as_ref()
            .map(|c| c.trim().is_empty())
            .unwrap_or(true)
    {
        return Err(err(
            StatusCode::BAD_REQUEST,
            "require_checksum is set but no checksum was provided",
        ));
    }
    let now = Utc::now();
    let mut model = ModelArtifact {
        name: req.name.clone(),
        source: req.source.clone(),
        revision: req.revision,
        checksum: req.checksum,
        format: req.format,
        size_bytes: req.size_bytes,
        tenant: req.tenant,
        local_path: None,
        license: req.license,
        residency: req.residency,
        require_checksum: req.require_checksum,
        signature: req.signature,
        optimize: req.optimize,
        derived_from: None,
        created: now,
        updated: now,
    };

    state
        .store
        .save_entity(STORE_MODELS, &model.name, &model)
        .map_err(|e| err(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let job = super::model_jobs::register(&state, &model)
        .map_err(|e| err(StatusCode::INTERNAL_SERVER_ERROR, e))?;
    if super::model_jobs::source_is_local(&model.source) {
        if let Some(done) = super::model_jobs::drive_until_terminal(&state, &job.id) {
            if done.state == super::types::ModelJobState::Failed {
                let _ = state.store.delete_entity(STORE_MODELS, &model.name);
                let _ = state.store.delete_entity(super::STORE_MODEL_JOBS, &job.id);
                return Err(err(
                    StatusCode::BAD_REQUEST,
                    done.message.unwrap_or_else(|| "model job failed".into()),
                ));
            }
        }
        if let Ok(Some(saved)) = state
            .store
            .get_entity::<ModelArtifact>(STORE_MODELS, &model.name)
        {
            model = saved;
        }
    }

    audit(
        &state,
        &claims.sub,
        "CREATE",
        &format!("ai/models/{}", model.name),
        "SUCCESS",
    );
    Ok((StatusCode::CREATED, Json(model)))
}

/// DELETE /api/ai/models/{name}
pub async fn delete_model(
    RequireWrite(claims): RequireWrite,
    State(state): State<Arc<AppState>>,
    Path(name): Path<String>,
) -> Result<StatusCode, (StatusCode, Json<serde_json::Value>)> {
    let model = state
        .store
        .get_entity::<ModelArtifact>(STORE_MODELS, &name)
        .map_err(|e| err(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
        .ok_or_else(|| err(StatusCode::NOT_FOUND, "ModelArtifact not found"))?;

    if let Some(ref claim_tenant) = claims.tenant {
        if model.tenant.as_deref() != Some(claim_tenant.as_str()) {
            return Err(err(StatusCode::NOT_FOUND, "ModelArtifact not found"));
        }
    }

    // Refuse delete while a deployment still references the model.
    let deps: Vec<super::types::InferenceDeployment> = state
        .store
        .list_entities(super::STORE_DEPLOYMENTS)
        .unwrap_or_default();
    if deps.iter().any(|d| d.model == name) {
        return Err(err(
            StatusCode::CONFLICT,
            format!("ModelArtifact '{name}' is referenced by an InferenceDeployment"),
        ));
    }

    state
        .store
        .delete_entity(STORE_MODELS, &name)
        .map_err(|e| err(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    audit(
        &state,
        &claims.sub,
        "DELETE",
        &format!("ai/models/{name}"),
        "SUCCESS",
    );
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Debug, serde::Deserialize, Default)]
pub struct EvictCacheRequest {
    pub budget_bytes: Option<u64>,
}

/// POST /api/ai/models/cache/evict
///
/// Removes unreferenced directories under `{state}/ai-models` until the
/// declared budget. Deployment-referenced models and the shared
/// `FLUXVM_AI_MODEL_DIR` stub are kept. The ModelArtifact record stays.
pub async fn evict_cache(
    RequireWrite(claims): RequireWrite,
    State(state): State<Arc<AppState>>,
    body: Option<Json<EvictCacheRequest>>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let requested = body.and_then(|Json(req)| req.budget_bytes);
    let budget = match requested {
        Some(n) => n,
        None => std::env::var("FLUXVM_AI_MODEL_CACHE_BYTES")
            .ok()
            .and_then(|v| v.trim().parse().ok())
            .ok_or_else(|| {
                err(
                    StatusCode::BAD_REQUEST,
                    "budget_bytes is required, or set FLUXVM_AI_MODEL_CACHE_BYTES",
                )
            })?,
    };

    let models: Vec<ModelArtifact> = state
        .store
        .list_entities(STORE_MODELS)
        .map_err(|e| err(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    let deps: Vec<super::types::InferenceDeployment> = state
        .store
        .list_entities(super::STORE_DEPLOYMENTS)
        .unwrap_or_default();
    let state_dir = std::path::PathBuf::from(&state.config.storage.path);

    let mut candidates = Vec::new();
    for model in &models {
        if let Some(ref tenant) = claims.tenant {
            if model.tenant.as_deref() != Some(tenant.as_str()) {
                continue;
            }
        }
        let Some(path) = model.local_path.as_deref() else {
            continue;
        };
        let path = std::path::Path::new(path);
        if !super::model_cache::is_managed_cache(path, &state_dir) {
            continue;
        }
        candidates.push(super::model_cache::CacheCandidate {
            name: model.name.clone(),
            size_bytes: model.size_bytes.unwrap_or(0),
            last_used_unix: model.updated.timestamp(),
            referenced: deps.iter().any(|d| d.model == model.name),
        });
    }
    let plan = super::model_cache::plan_eviction(&candidates, budget);
    let mut evicted = Vec::new();
    for name in &plan.evict {
        if deps.iter().any(|d| d.model == *name) {
            continue;
        }
        let mut model = state
            .store
            .get_entity::<ModelArtifact>(STORE_MODELS, name)
            .map_err(|e| err(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
            .ok_or_else(|| err(StatusCode::NOT_FOUND, "ModelArtifact not found"))?;
        if let Some(path) = model.local_path.as_deref() {
            let path = std::path::PathBuf::from(path);
            if super::model_cache::is_managed_cache(&path, &state_dir) {
                if path.is_dir() {
                    let _ = std::fs::remove_dir_all(&path);
                } else if path.is_file() {
                    let _ = std::fs::remove_file(&path);
                }
            }
        }
        model.local_path = None;
        model.updated = Utc::now();
        state
            .store
            .save_entity(STORE_MODELS, &model.name, &model)
            .map_err(|e| err(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
        evicted.push(name.clone());
    }
    audit(&state, &claims.sub, "EVICT", "ai/models/cache", "SUCCESS");
    Ok(Json(serde_json::json!({
        "evicted": evicted,
        "retained_bytes": plan.retained_bytes,
        "over_budget": plan.over_budget,
    })))
}
