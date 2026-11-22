// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: LicenseRef-Zyvor-Production-2.0

use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use chrono::Utc;
use std::sync::Arc;
use uuid::Uuid;

use crate::server::AppState;
use lifecycle_manager::{
    Baseline, ComplianceSummary, HostComplianceStatus, RemediationTask, RollingUpdatePlan,
    RollingUpdateStatus,
};
use security::{RequireAdmin, RequireRead, RequireWrite};

// ============================================================================
// Baseline handlers
// ============================================================================

pub async fn list_baselines(
    RequireRead(_claims): RequireRead,
    State(state): State<Arc<AppState>>,
) -> impl IntoResponse {
    tracing::debug!("lifecycle::{}", stringify!(list_baselines));
    let items: Vec<Baseline> = state
        .store
        .list_entities("lm_baselines")
        .unwrap_or_else(|e| {
            tracing::error!("Storage error: {}", e);
            Vec::new()
        });
    Json(items)
}

pub async fn create_baseline(
    RequireWrite(_claims): RequireWrite,
    State(state): State<Arc<AppState>>,
    Json(mut baseline): Json<Baseline>,
) -> impl IntoResponse {
    tracing::debug!("lifecycle::{}", stringify!(create_baseline));
    baseline.id = Uuid::new_v4().to_string();
    baseline.created = Utc::now();
    baseline.updated = None;
    match state
        .store
        .save_entity("lm_baselines", &baseline.id, &baseline)
    {
        Ok(_) => (StatusCode::CREATED, Json(baseline)).into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": e.to_string()})),
        )
            .into_response(),
    }
}

pub async fn get_baseline(
    RequireRead(_claims): RequireRead,
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    tracing::debug!("lifecycle::{}", stringify!(get_baseline));
    match state.store.get_entity::<Baseline>("lm_baselines", &id) {
        Ok(Some(b)) => Json(b).into_response(),
        Ok(None) => (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({"error": "Baseline not found"})),
        )
            .into_response(),
        Err(_) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": "Internal server error"})),
        )
            .into_response(),
    }
}

pub async fn update_baseline(
    RequireWrite(_claims): RequireWrite,
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    Json(mut baseline): Json<Baseline>,
) -> impl IntoResponse {
    tracing::debug!("lifecycle::{}", stringify!(update_baseline));
    if state
        .store
        .get_entity::<Baseline>("lm_baselines", &id)
        .ok()
        .flatten()
        .is_none()
    {
        return (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({"error": "Not found"})),
        )
            .into_response();
    }
    baseline.id = id.clone();
    baseline.updated = Some(Utc::now());
    if let Err(e) = state.store.save_entity("lm_baselines", &id, &baseline) {
        tracing::error!("Failed to save entity: {}", e);
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": e.to_string()})),
        )
            .into_response();
    }
    Json(baseline).into_response()
}

pub async fn delete_baseline(
    RequireAdmin(_claims): RequireAdmin,
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    tracing::debug!("lifecycle::{}", stringify!(delete_baseline));
    if let Err(e) = state.store.delete_entity("lm_baselines", &id) {
        tracing::error!("Failed to delete entity: {}", e);
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": e.to_string()})),
        )
            .into_response();
    }
    (
        StatusCode::NO_CONTENT,
        Json(serde_json::json!({"status": "deleted"})),
    )
        .into_response()
}

// ============================================================================
// Compliance handlers
// ============================================================================

#[derive(serde::Deserialize)]
pub struct ScanComplianceRequest {
    pub host_id: String,
    pub hostname: String,
    pub baseline_id: String,
    pub installed_packages: Vec<lifecycle_manager::InstalledPatch>,
}

pub async fn scan_host_compliance(
    RequireRead(_claims): RequireRead,
    State(state): State<Arc<AppState>>,
    Json(req): Json<ScanComplianceRequest>,
) -> impl IntoResponse {
    tracing::debug!("lifecycle::{}", stringify!(scan_host_compliance));
    let mgr = lifecycle_manager::LifecycleManager::new();
    // Load baseline into manager
    if let Ok(Some(baseline)) = state
        .store
        .get_entity::<Baseline>("lm_baselines", &req.baseline_id)
    {
        if let Err(e) = mgr.create_baseline(baseline) {
            tracing::warn!("Failed to load baseline into manager: {}", e);
        }
    }
    match mgr.scan_host_compliance(
        &req.host_id,
        &req.hostname,
        &req.baseline_id,
        &req.installed_packages,
    ) {
        Ok(status) => {
            let id = Uuid::new_v4().to_string();
            if let Err(e) = state.store.save_entity("compliance_results", &id, &status) {
                tracing::error!("Failed to save entity: {}", e);
            }
            Json(status).into_response()
        }
        Err(e) => (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"error": e.to_string()})),
        )
            .into_response(),
    }
}

#[derive(serde::Deserialize)]
pub struct ComplianceQuery {
    pub baseline_id: Option<String>,
    pub host_id: Option<String>,
}

/// `GET /lifecycle/compliance` — list compliance results across all hosts,
/// optionally filtered by baseline/host via query params. The path-scoped
/// `get_compliance_status` (below) is for a single known host.
pub async fn list_compliance_status(
    RequireRead(_claims): RequireRead,
    State(state): State<Arc<AppState>>,
    Query(q): Query<ComplianceQuery>,
) -> impl IntoResponse {
    tracing::debug!("lifecycle::{}", stringify!(list_compliance_status));
    let items: Vec<HostComplianceStatus> = state
        .store
        .list_entities("compliance_results")
        .unwrap_or_else(|e| {
            tracing::error!("Storage error: {}", e);
            Vec::new()
        });
    let filtered: Vec<_> = items
        .into_iter()
        .filter(|s| q.baseline_id.as_ref().is_none_or(|b| &s.baseline_id == b))
        .filter(|s| q.host_id.as_ref().is_none_or(|h| &s.host_id == h))
        .collect();
    Json(filtered)
}

pub async fn get_compliance_status(
    RequireRead(_claims): RequireRead,
    State(state): State<Arc<AppState>>,
    Path(host_id): Path<String>,
) -> impl IntoResponse {
    tracing::debug!("lifecycle::{}", stringify!(get_compliance_status));
    let items: Vec<HostComplianceStatus> = state
        .store
        .list_entities("compliance_results")
        .unwrap_or_else(|e| {
            tracing::error!("Storage error: {}", e);
            Vec::new()
        });
    let filtered: Vec<_> = items.into_iter().filter(|s| s.host_id == host_id).collect();
    Json(filtered)
}

pub async fn get_cluster_compliance(
    RequireRead(_claims): RequireRead,
    State(state): State<Arc<AppState>>,
    Path(_cluster_id): Path<String>,
) -> impl IntoResponse {
    tracing::debug!("lifecycle::{}", stringify!(get_cluster_compliance));
    let items: Vec<HostComplianceStatus> = state
        .store
        .list_entities("compliance_results")
        .unwrap_or_else(|e| {
            tracing::error!("Storage error: {}", e);
            Vec::new()
        });
    let mut total_hosts = 0u32;
    let mut compliant_hosts = 0u32;
    let mut non_compliant_hosts = 0u32;
    let mut critical_missing = 0u32;
    // Use the latest scan per host
    let mut seen = std::collections::HashSet::new();
    for status in items.iter().rev() {
        if seen.insert(status.host_id.clone()) {
            total_hosts += 1;
            if status.compliant {
                compliant_hosts += 1;
            } else {
                non_compliant_hosts += 1;
                critical_missing += status
                    .missing_patches
                    .iter()
                    .filter(|p| p.severity == Some(lifecycle_manager::PatchSeverity::Critical))
                    .count() as u32;
            }
        }
    }
    let summary = ComplianceSummary {
        total_hosts,
        compliant_hosts,
        non_compliant_hosts,
        critical_missing,
    };
    Json(summary)
}

// ============================================================================
// Remediation handlers
// ============================================================================

pub async fn list_remediations(
    RequireRead(_claims): RequireRead,
    State(state): State<Arc<AppState>>,
) -> impl IntoResponse {
    tracing::debug!("lifecycle::{}", stringify!(list_remediations));
    let items: Vec<RemediationTask> =
        state
            .store
            .list_entities("remediations")
            .unwrap_or_else(|e| {
                tracing::error!("Storage error: {}", e);
                Vec::new()
            });
    Json(items)
}

#[derive(serde::Deserialize)]
pub struct CreateRemediationRequest {
    pub host_id: String,
    pub hostname: String,
    pub baseline_id: String,
}

pub async fn create_remediation(
    RequireWrite(_claims): RequireWrite,
    State(state): State<Arc<AppState>>,
    Json(req): Json<CreateRemediationRequest>,
) -> impl IntoResponse {
    tracing::debug!("lifecycle::{}", stringify!(create_remediation));
    let mgr = lifecycle_manager::LifecycleManager::new();
    match mgr.create_remediation(&req.host_id, &req.hostname, &req.baseline_id) {
        Ok(task) => {
            if let Err(e) = state.store.save_entity("remediations", &task.id, &task) {
                tracing::error!("Failed to save entity: {}", e);
            }
            (StatusCode::CREATED, Json(task)).into_response()
        }
        Err(e) => (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"error": e.to_string()})),
        )
            .into_response(),
    }
}

pub async fn get_remediation(
    RequireRead(_claims): RequireRead,
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    tracing::debug!("lifecycle::{}", stringify!(get_remediation));
    match state
        .store
        .get_entity::<RemediationTask>("remediations", &id)
    {
        Ok(Some(t)) => Json(t).into_response(),
        Ok(None) => (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({"error": "Remediation not found"})),
        )
            .into_response(),
        Err(_) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": "Internal server error"})),
        )
            .into_response(),
    }
}

// ============================================================================
// Rolling update handlers
// ============================================================================

pub async fn list_rolling_updates(
    RequireRead(_claims): RequireRead,
    State(state): State<Arc<AppState>>,
) -> impl IntoResponse {
    tracing::debug!("lifecycle::{}", stringify!(list_rolling_updates));
    let items: Vec<RollingUpdatePlan> = state
        .store
        .list_entities("rolling_updates")
        .unwrap_or_else(|e| {
            tracing::error!("Storage error: {}", e);
            Vec::new()
        });
    Json(items)
}

pub async fn create_rolling_update(
    RequireWrite(_claims): RequireWrite,
    State(state): State<Arc<AppState>>,
    Json(mut plan): Json<RollingUpdatePlan>,
) -> impl IntoResponse {
    tracing::debug!("lifecycle::{}", stringify!(create_rolling_update));
    plan.id = Uuid::new_v4().to_string();
    plan.status = RollingUpdateStatus::Planned;
    plan.current_host_index = 0;
    plan.started = None;
    plan.completed = None;
    if plan.max_concurrent == 0 {
        plan.max_concurrent = 1;
    }
    match state.store.save_entity("rolling_updates", &plan.id, &plan) {
        Ok(_) => (StatusCode::CREATED, Json(plan)).into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": e.to_string()})),
        )
            .into_response(),
    }
}

pub async fn start_rolling_update(
    RequireWrite(_claims): RequireWrite,
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    tracing::debug!("lifecycle::{}", stringify!(start_rolling_update));
    let mut plan = match state
        .store
        .get_entity::<RollingUpdatePlan>("rolling_updates", &id)
    {
        Ok(Some(p)) => p,
        Ok(None) => {
            return (
                StatusCode::NOT_FOUND,
                Json(serde_json::json!({"error": "Rolling update not found"})),
            )
                .into_response()
        }
        Err(_) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"error": "Internal server error"})),
            )
                .into_response()
        }
    };
    plan.status = RollingUpdateStatus::InProgress;
    plan.started = Some(Utc::now());
    if let Err(e) = state.store.save_entity("rolling_updates", &plan.id, &plan) {
        tracing::error!("Failed to save entity: {}", e);
    }
    (
        StatusCode::OK,
        Json(serde_json::json!({"status": "rolling update started"})),
    )
        .into_response()
}

pub async fn pause_rolling_update(
    RequireWrite(_claims): RequireWrite,
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    tracing::debug!("lifecycle::{}", stringify!(pause_rolling_update));
    let mut plan = match state
        .store
        .get_entity::<RollingUpdatePlan>("rolling_updates", &id)
    {
        Ok(Some(p)) => p,
        Ok(None) => {
            return (
                StatusCode::NOT_FOUND,
                Json(serde_json::json!({"error": "Rolling update not found"})),
            )
                .into_response()
        }
        Err(_) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"error": "Internal server error"})),
            )
                .into_response()
        }
    };
    plan.status = RollingUpdateStatus::Paused;
    if let Err(e) = state.store.save_entity("rolling_updates", &plan.id, &plan) {
        tracing::error!("Failed to save entity: {}", e);
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": e.to_string()})),
        )
            .into_response();
    }
    (
        StatusCode::OK,
        Json(serde_json::json!({"status": "rolling update paused"})),
    )
        .into_response()
}

pub async fn advance_rolling_update(
    RequireWrite(_claims): RequireWrite,
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    tracing::debug!("lifecycle::{}", stringify!(advance_rolling_update));
    let mut plan = match state
        .store
        .get_entity::<RollingUpdatePlan>("rolling_updates", &id)
    {
        Ok(Some(p)) => p,
        Ok(None) => {
            return (
                StatusCode::NOT_FOUND,
                Json(serde_json::json!({"error": "Rolling update not found"})),
            )
                .into_response()
        }
        Err(_) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"error": "Internal server error"})),
            )
                .into_response()
        }
    };
    let index = plan.current_host_index as usize;
    if index >= plan.host_order.len() {
        plan.status = RollingUpdateStatus::Completed;
        plan.completed = Some(Utc::now());
        if let Err(e) = state.store.save_entity("rolling_updates", &plan.id, &plan) {
            tracing::error!("Failed to save entity: {}", e);
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"error": e.to_string()})),
            )
                .into_response();
        }
        return Json(serde_json::json!({"completed": true})).into_response();
    }
    let host_id = plan.host_order[index].clone();
    plan.current_host_index += 1;
    if let Err(e) = state.store.save_entity("rolling_updates", &plan.id, &plan) {
        tracing::error!("Failed to save entity: {}", e);
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": e.to_string()})),
        )
            .into_response();
    }
    Json(serde_json::json!({"next_host_id": host_id})).into_response()
}
