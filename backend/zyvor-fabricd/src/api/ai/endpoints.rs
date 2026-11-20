// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: LicenseRef-Zyvor-Production-1.0

//! InferenceEndpoint REST handlers (AI Inference MVP, preview).

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
    CreateInferenceEndpointRequest, InferenceDeployment, InferenceEndpoint, TenantQuery,
};
use super::{audit, err, reconcile, STORE_DEPLOYMENTS, STORE_ENDPOINTS};

/// GET /api/ai/endpoints
pub async fn list_endpoints(
    RequireRead(claims): RequireRead,
    State(state): State<Arc<AppState>>,
    Query(q): Query<TenantQuery>,
) -> Result<Json<Vec<InferenceEndpoint>>, (StatusCode, Json<serde_json::Value>)> {
    let tenant_filter = crate::tenant_scope::apply_list_tenant_filter(&claims, q.tenant)
        .map_err(|(s, m)| err(s, m))?;

    let mut items: Vec<InferenceEndpoint> = state
        .store
        .list_entities(STORE_ENDPOINTS)
        .map_err(|e| err(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    if let Some(ref t) = tenant_filter {
        items.retain(|e| e.tenant.as_deref() == Some(t.as_str()));
    }
    items.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(Json(items))
}

/// GET /api/ai/endpoints/{name}
pub async fn get_endpoint(
    RequireRead(claims): RequireRead,
    State(state): State<Arc<AppState>>,
    Path(name): Path<String>,
) -> Result<Json<InferenceEndpoint>, (StatusCode, Json<serde_json::Value>)> {
    let ep = state
        .store
        .get_entity::<InferenceEndpoint>(STORE_ENDPOINTS, &name)
        .map_err(|e| err(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
        .ok_or_else(|| err(StatusCode::NOT_FOUND, "InferenceEndpoint not found"))?;

    if let Some(ref claim_tenant) = claims.tenant {
        if ep.tenant.as_deref() != Some(claim_tenant.as_str()) {
            return Err(err(StatusCode::NOT_FOUND, "InferenceEndpoint not found"));
        }
    }
    Ok(Json(ep))
}

/// POST /api/ai/endpoints
pub async fn create_endpoint(
    RequireWrite(claims): RequireWrite,
    State(state): State<Arc<AppState>>,
    Json(mut req): Json<CreateInferenceEndpointRequest>,
) -> Result<(StatusCode, Json<InferenceEndpoint>), (StatusCode, Json<serde_json::Value>)> {
    crate::validation::validate_entity_name(&req.name).map_err(|(s, m)| err(s, m))?;
    if req.protocol != "openai" {
        return Err(err(
            StatusCode::BAD_REQUEST,
            "protocol must be 'openai' (MVP)",
        ));
    }
    if req.port == 0 {
        return Err(err(StatusCode::BAD_REQUEST, "port must be non-zero"));
    }

    req.tenant = crate::tenant_scope::apply_create_tenant(&claims, req.tenant)
        .map_err(|(s, m)| err(s, m))?;

    let dep = state
        .store
        .get_entity::<InferenceDeployment>(STORE_DEPLOYMENTS, &req.deployment)
        .map_err(|e| err(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
        .ok_or_else(|| {
            err(
                StatusCode::BAD_REQUEST,
                format!("InferenceDeployment '{}' not found", req.deployment),
            )
        })?;

    if let Some(ref t) = req.tenant {
        if dep.tenant.as_ref() != Some(t) {
            return Err(err(
                StatusCode::FORBIDDEN,
                "endpoint tenant must match deployment tenant",
            ));
        }
    }

    if state
        .store
        .get_entity::<InferenceEndpoint>(STORE_ENDPOINTS, &req.name)
        .ok()
        .flatten()
        .is_some()
    {
        return Err(err(
            StatusCode::CONFLICT,
            format!("InferenceEndpoint '{}' already exists", req.name),
        ));
    }

    let now = Utc::now();
    let endpoint = InferenceEndpoint {
        name: req.name.clone(),
        deployment: req.deployment,
        protocol: req.protocol,
        port: req.port,
        service_id: None,
        vip: req.vip,
        routing_strategy: req.routing_strategy,
        tenant: req.tenant,
        preferred_site: req.preferred_site.or(dep.preferred_site.clone()),
        allowed_sites: req.allowed_sites,
        residency: req.residency.or(dep.residency.clone()),
        phase: String::new(),
        created: now,
        updated: now,
    };

    state
        .store
        .save_entity(STORE_ENDPOINTS, &endpoint.name, &endpoint)
        .map_err(|e| err(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    reconcile::enqueue_endpoint_reconcile(state.clone(), endpoint.name.clone());

    audit(
        &state,
        &claims.sub,
        "CREATE",
        &format!("ai/endpoints/{}", endpoint.name),
        "SUCCESS",
    );
    Ok((StatusCode::CREATED, Json(endpoint)))
}

/// DELETE /api/ai/endpoints/{name}
pub async fn delete_endpoint(
    RequireWrite(claims): RequireWrite,
    State(state): State<Arc<AppState>>,
    Path(name): Path<String>,
) -> Result<StatusCode, (StatusCode, Json<serde_json::Value>)> {
    let ep = state
        .store
        .get_entity::<InferenceEndpoint>(STORE_ENDPOINTS, &name)
        .map_err(|e| err(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
        .ok_or_else(|| err(StatusCode::NOT_FOUND, "InferenceEndpoint not found"))?;

    if let Some(ref claim_tenant) = claims.tenant {
        if ep.tenant.as_deref() != Some(claim_tenant.as_str()) {
            return Err(err(StatusCode::NOT_FOUND, "InferenceEndpoint not found"));
        }
    }

    // Maglev must be gone before the VIP can be reused.
    if let Err(e) = reconcile::teardown_endpoint(&state, &ep).await {
        return Err(err(StatusCode::CONFLICT, e));
    }

    state
        .store
        .delete_entity(STORE_ENDPOINTS, &name)
        .map_err(|e| err(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    audit(
        &state,
        &claims.sub,
        "DELETE",
        &format!("ai/endpoints/{name}"),
        "SUCCESS",
    );
    Ok(StatusCode::NO_CONTENT)
}
