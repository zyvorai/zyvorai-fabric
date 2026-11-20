// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: LicenseRef-Zyvor-Production-1.0

//! InferenceProfile REST handlers (AI Inference MVP, preview).

use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    Json,
};
use chrono::Utc;
use security::{RequireRead, RequireWrite};
use std::sync::Arc;

use crate::server::AppState;

use super::types::{CreateInferenceProfileRequest, InferenceProfile, TenantQuery};
use super::{audit, err, STORE_PROFILES};

/// GET /api/ai/profiles
pub async fn list_profiles(
    RequireRead(claims): RequireRead,
    State(state): State<Arc<AppState>>,
    Query(q): Query<TenantQuery>,
) -> Result<Json<Vec<InferenceProfile>>, (StatusCode, Json<serde_json::Value>)> {
    let tenant_filter = crate::tenant_scope::apply_list_tenant_filter(&claims, q.tenant)
        .map_err(|(s, m)| err(s, m))?;

    let mut items: Vec<InferenceProfile> = state
        .store
        .list_entities(STORE_PROFILES)
        .map_err(|e| err(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    if let Some(ref t) = tenant_filter {
        items.retain(|p| p.tenant.as_deref() == Some(t.as_str()));
    }
    items.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(Json(items))
}

/// GET /api/ai/profiles/{name}
pub async fn get_profile(
    RequireRead(claims): RequireRead,
    State(state): State<Arc<AppState>>,
    Path(name): Path<String>,
) -> Result<Json<InferenceProfile>, (StatusCode, Json<serde_json::Value>)> {
    let profile = state
        .store
        .get_entity::<InferenceProfile>(STORE_PROFILES, &name)
        .map_err(|e| err(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
        .ok_or_else(|| err(StatusCode::NOT_FOUND, "InferenceProfile not found"))?;

    if let Some(ref claim_tenant) = claims.tenant {
        if profile.tenant.as_deref() != Some(claim_tenant.as_str()) {
            return Err(err(StatusCode::NOT_FOUND, "InferenceProfile not found"));
        }
    }
    Ok(Json(profile))
}

/// POST /api/ai/profiles
pub async fn create_profile(
    RequireWrite(claims): RequireWrite,
    State(state): State<Arc<AppState>>,
    Json(mut req): Json<CreateInferenceProfileRequest>,
) -> Result<(StatusCode, Json<InferenceProfile>), (StatusCode, Json<serde_json::Value>)> {
    crate::validation::validate_entity_name(&req.name).map_err(|(s, m)| err(s, m))?;
    if req.gpu.count == 0 || req.gpu.count > 8 {
        return Err(err(
            StatusCode::BAD_REQUEST,
            "gpu.count must be between 1 and 8",
        ));
    }
    if let Err(msg) = super::runtime::require_supported(&req.runtime) {
        return Err(err(StatusCode::BAD_REQUEST, msg));
    }
    if req.cpu == 0 || req.memory_gib == 0 {
        return Err(err(
            StatusCode::BAD_REQUEST,
            "cpu and memory_gib must be greater than 0",
        ));
    }
    if req.gpu.vendor.trim().is_empty() {
        return Err(err(StatusCode::BAD_REQUEST, "gpu.vendor is required"));
    }

    req.tenant = crate::tenant_scope::apply_create_tenant(&claims, req.tenant)
        .map_err(|(s, m)| err(s, m))?;

    if state
        .store
        .get_entity::<InferenceProfile>(STORE_PROFILES, &req.name)
        .ok()
        .flatten()
        .is_some()
    {
        return Err(err(
            StatusCode::CONFLICT,
            format!("InferenceProfile '{}' already exists", req.name),
        ));
    }

    let now = Utc::now();
    let profile = InferenceProfile {
        name: req.name.clone(),
        runtime: req.runtime,
        gpu: req.gpu,
        cpu: req.cpu,
        memory_gib: req.memory_gib,
        allowed_hosts: req.allowed_hosts,
        max_egress_mbps: req.max_egress_mbps,
        tenant: req.tenant,
        created: now,
        updated: now,
    };

    state
        .store
        .save_entity(STORE_PROFILES, &profile.name, &profile)
        .map_err(|e| err(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    audit(
        &state,
        &claims.sub,
        "CREATE",
        &format!("ai/profiles/{}", profile.name),
        "SUCCESS",
    );
    Ok((StatusCode::CREATED, Json(profile)))
}

/// DELETE /api/ai/profiles/{name}
pub async fn delete_profile(
    RequireWrite(claims): RequireWrite,
    State(state): State<Arc<AppState>>,
    Path(name): Path<String>,
) -> Result<StatusCode, (StatusCode, Json<serde_json::Value>)> {
    let profile = state
        .store
        .get_entity::<InferenceProfile>(STORE_PROFILES, &name)
        .map_err(|e| err(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
        .ok_or_else(|| err(StatusCode::NOT_FOUND, "InferenceProfile not found"))?;

    if let Some(ref claim_tenant) = claims.tenant {
        if profile.tenant.as_deref() != Some(claim_tenant.as_str()) {
            return Err(err(StatusCode::NOT_FOUND, "InferenceProfile not found"));
        }
    }

    let deps: Vec<super::types::InferenceDeployment> = state
        .store
        .list_entities(super::STORE_DEPLOYMENTS)
        .unwrap_or_default();
    if deps.iter().any(|d| d.profile == name) {
        return Err(err(
            StatusCode::CONFLICT,
            format!("InferenceProfile '{name}' is referenced by an InferenceDeployment"),
        ));
    }

    state
        .store
        .delete_entity(STORE_PROFILES, &name)
        .map_err(|e| err(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    audit(
        &state,
        &claims.sub,
        "DELETE",
        &format!("ai/profiles/{name}"),
        "SUCCESS",
    );
    Ok(StatusCode::NO_CONTENT)
}
