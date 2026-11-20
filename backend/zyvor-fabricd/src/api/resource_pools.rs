// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: LicenseRef-Zyvor-Production-1.0

use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use chrono::Utc;
use std::sync::Arc;
use uuid::Uuid;

use crate::server::AppState;
use resource_pools::{
    AdmissionControlResult, CreateResourcePoolRequest, ResourcePool, ResourcePoolSummary,
    UpdateResourcePoolRequest,
};
use security::{RequireAdmin, RequireRead, RequireWrite};

pub async fn list_pools(
    RequireRead(_claims): RequireRead,
    State(state): State<Arc<AppState>>,
) -> impl IntoResponse {
    tracing::debug!("resource_pools::{}", stringify!(list_pools));
    let items: Vec<ResourcePool> =
        state
            .store
            .list_entities("resource_pools")
            .unwrap_or_else(|e| {
                tracing::error!("Storage error: {}", e);
                Vec::new()
            });
    Json(items)
}

pub async fn create_pool(
    RequireWrite(_claims): RequireWrite,
    State(state): State<Arc<AppState>>,
    Json(req): Json<CreateResourcePoolRequest>,
) -> impl IntoResponse {
    tracing::debug!("resource_pools::{}", stringify!(create_pool));
    if let Err((status, msg)) = crate::validation::validate_entity_name(&req.name) {
        return (status, Json(serde_json::json!({"error": msg}))).into_response();
    }
    let now = Utc::now();
    let pool = ResourcePool {
        id: Uuid::new_v4().to_string(),
        name: req.name,
        parent_id: req.parent_id,
        cluster_id: req.cluster_id,
        cpu_shares: req.cpu_shares,
        cpu_reservation_mhz: req.cpu_reservation_mhz,
        cpu_limit_mhz: req.cpu_limit_mhz,
        cpu_expandable_reservation: req.cpu_expandable_reservation,
        memory_shares: req.memory_shares,
        memory_reservation_mb: req.memory_reservation_mb,
        memory_limit_mb: req.memory_limit_mb,
        memory_expandable_reservation: req.memory_expandable_reservation,
        vms: Vec::new(),
        children: Vec::new(),
        created: now,
        updated: None,
    };
    match state.store.save_entity("resource_pools", &pool.id, &pool) {
        Ok(_) => (StatusCode::CREATED, Json(pool)).into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": e.to_string()})),
        )
            .into_response(),
    }
}

pub async fn get_pool(
    RequireRead(_claims): RequireRead,
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    tracing::debug!("resource_pools::{}", stringify!(get_pool));
    match state
        .store
        .get_entity::<ResourcePool>("resource_pools", &id)
    {
        Ok(Some(p)) => Json(p).into_response(),
        Ok(None) => (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({"error": "Resource pool not found"})),
        )
            .into_response(),
        Err(_) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": "Failed to load resource pool"})),
        )
            .into_response(),
    }
}

pub async fn update_pool(
    RequireWrite(_claims): RequireWrite,
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    Json(req): Json<UpdateResourcePoolRequest>,
) -> impl IntoResponse {
    tracing::debug!("resource_pools::{}", stringify!(update_pool));
    let mut pool = match state
        .store
        .get_entity::<ResourcePool>("resource_pools", &id)
    {
        Ok(Some(p)) => p,
        Ok(None) => {
            return (
                StatusCode::NOT_FOUND,
                Json(serde_json::json!({"error": "Resource pool not found"})),
            )
                .into_response()
        }
        Err(_) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"error": "Failed to load resource pool"})),
            )
                .into_response()
        }
    };
    if let Some(name) = req.name {
        pool.name = name;
    }
    if let Some(shares) = req.cpu_shares {
        pool.cpu_shares = shares;
    }
    if let Some(v) = req.cpu_reservation_mhz {
        pool.cpu_reservation_mhz = v;
    }
    if let Some(v) = req.cpu_limit_mhz {
        pool.cpu_limit_mhz = v;
    }
    if let Some(v) = req.cpu_expandable_reservation {
        pool.cpu_expandable_reservation = v;
    }
    if let Some(shares) = req.memory_shares {
        pool.memory_shares = shares;
    }
    if let Some(v) = req.memory_reservation_mb {
        pool.memory_reservation_mb = v;
    }
    if let Some(v) = req.memory_limit_mb {
        pool.memory_limit_mb = v;
    }
    if let Some(v) = req.memory_expandable_reservation {
        pool.memory_expandable_reservation = v;
    }
    pool.updated = Some(Utc::now());
    if let Err(e) = state.store.save_entity("resource_pools", &pool.id, &pool) {
        tracing::error!("Failed to save entity: {}", e);
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": e.to_string()})),
        )
            .into_response();
    }
    Json(pool).into_response()
}

pub async fn delete_pool(
    RequireAdmin(_claims): RequireAdmin,
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    tracing::debug!("resource_pools::{}", stringify!(delete_pool));
    if let Err(e) = state.store.delete_entity("resource_pools", &id) {
        tracing::error!("Failed to delete entity: {}", e);
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": e.to_string()})),
        )
            .into_response();
    }
    StatusCode::NO_CONTENT.into_response()
}

pub async fn get_pool_summary(
    RequireRead(_claims): RequireRead,
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    tracing::debug!("resource_pools::{}", stringify!(get_pool_summary));
    let pool = match state
        .store
        .get_entity::<ResourcePool>("resource_pools", &id)
    {
        Ok(Some(p)) => p,
        Ok(None) => {
            return (
                StatusCode::NOT_FOUND,
                Json(serde_json::json!({"error": "Resource pool not found"})),
            )
                .into_response()
        }
        Err(_) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"error": "Failed to load resource pool"})),
            )
                .into_response()
        }
    };
    let summary = ResourcePoolSummary {
        id: pool.id.clone(),
        name: pool.name.clone(),
        parent_id: pool.parent_id.clone(),
        cluster_id: pool.cluster_id.clone(),
        cpu_reservation_mhz: pool.cpu_reservation_mhz,
        cpu_limit_mhz: pool.cpu_limit_mhz,
        cpu_used_mhz: 0,
        memory_reservation_mb: pool.memory_reservation_mb,
        memory_limit_mb: pool.memory_limit_mb,
        memory_used_mb: 0,
        vm_count: pool.vms.len(),
        child_pool_count: pool.children.len(),
    };
    Json(summary).into_response()
}

#[derive(serde::Deserialize)]
pub struct VmAssignment {
    pub vm_name: String,
}

pub async fn assign_vm(
    RequireWrite(_claims): RequireWrite,
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    Json(req): Json<VmAssignment>,
) -> impl IntoResponse {
    tracing::debug!("resource_pools::{}", stringify!(assign_vm));
    let mut pool = match state
        .store
        .get_entity::<ResourcePool>("resource_pools", &id)
    {
        Ok(Some(p)) => p,
        Ok(None) => {
            return (
                StatusCode::NOT_FOUND,
                Json(serde_json::json!({"error": "Resource pool not found"})),
            )
                .into_response()
        }
        Err(_) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"error": "Failed to load resource pool"})),
            )
                .into_response()
        }
    };
    if pool.vms.contains(&req.vm_name) {
        return (
            StatusCode::CONFLICT,
            Json(serde_json::json!({"error": "VM already assigned"})),
        )
            .into_response();
    }
    pool.vms.push(req.vm_name);
    pool.updated = Some(Utc::now());
    if let Err(e) = state.store.save_entity("resource_pools", &pool.id, &pool) {
        tracing::error!("Failed to save entity: {}", e);
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": e.to_string()})),
        )
            .into_response();
    }
    StatusCode::OK.into_response()
}

pub async fn unassign_vm(
    RequireWrite(_claims): RequireWrite,
    State(state): State<Arc<AppState>>,
    Path((id, vm_name)): Path<(String, String)>,
) -> impl IntoResponse {
    tracing::debug!("resource_pools::{}", stringify!(unassign_vm));
    let mut pool = match state
        .store
        .get_entity::<ResourcePool>("resource_pools", &id)
    {
        Ok(Some(p)) => p,
        Ok(None) => {
            return (
                StatusCode::NOT_FOUND,
                Json(serde_json::json!({"error": "Resource pool not found"})),
            )
                .into_response()
        }
        Err(_) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"error": "Failed to load resource pool"})),
            )
                .into_response()
        }
    };
    pool.vms.retain(|v| v != &vm_name);
    pool.updated = Some(Utc::now());
    if let Err(e) = state.store.save_entity("resource_pools", &pool.id, &pool) {
        tracing::error!("Failed to save entity: {}", e);
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": e.to_string()})),
        )
            .into_response();
    }
    StatusCode::OK.into_response()
}

#[derive(serde::Deserialize)]
pub struct MoveVmRequest {
    pub vm_name: String,
    pub target_pool_id: String,
}

pub async fn move_vm(
    RequireWrite(_claims): RequireWrite,
    State(state): State<Arc<AppState>>,
    Path(from_id): Path<String>,
    Json(req): Json<MoveVmRequest>,
) -> impl IntoResponse {
    tracing::debug!("resource_pools::{}", stringify!(move_vm));
    let mut src = match state
        .store
        .get_entity::<ResourcePool>("resource_pools", &from_id)
    {
        Ok(Some(p)) => p,
        Ok(None) => {
            return (
                StatusCode::NOT_FOUND,
                Json(serde_json::json!({"error": "Source resource pool not found"})),
            )
                .into_response()
        }
        Err(_) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"error": "Failed to load source resource pool"})),
            )
                .into_response()
        }
    };
    let mut dst = match state
        .store
        .get_entity::<ResourcePool>("resource_pools", &req.target_pool_id)
    {
        Ok(Some(p)) => p,
        Ok(None) => {
            return (
                StatusCode::NOT_FOUND,
                Json(serde_json::json!({"error": "Target resource pool not found"})),
            )
                .into_response()
        }
        Err(_) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"error": "Failed to load target resource pool"})),
            )
                .into_response()
        }
    };
    src.vms.retain(|v| v != &req.vm_name);
    dst.vms.push(req.vm_name);
    let now = Some(Utc::now());
    src.updated = now;
    dst.updated = now;
    if let Err(e) = state.store.save_entity("resource_pools", &src.id, &src) {
        tracing::error!("Failed to save entity: {}", e);
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": e.to_string()})),
        )
            .into_response();
    }
    if let Err(e) = state.store.save_entity("resource_pools", &dst.id, &dst) {
        tracing::error!("Failed to save entity: {}", e);
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": e.to_string()})),
        )
            .into_response();
    }
    StatusCode::OK.into_response()
}

#[derive(serde::Deserialize)]
pub struct AdmissionCheckRequest {
    pub cpu_mhz: u64,
    pub memory_mb: u64,
}

pub async fn check_admission(
    RequireRead(_claims): RequireRead,
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    Json(req): Json<AdmissionCheckRequest>,
) -> impl IntoResponse {
    tracing::debug!("resource_pools::{}", stringify!(check_admission));
    let pool = match state
        .store
        .get_entity::<ResourcePool>("resource_pools", &id)
    {
        Ok(Some(p)) => p,
        Ok(None) => {
            return (
                StatusCode::NOT_FOUND,
                Json(serde_json::json!({"error": "Resource pool not found"})),
            )
                .into_response()
        }
        Err(_) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"error": "Failed to load resource pool"})),
            )
                .into_response()
        }
    };
    let avail_cpu = pool.cpu_limit_mhz.unwrap_or(pool.cpu_reservation_mhz);
    let avail_mem = pool.memory_limit_mb.unwrap_or(pool.memory_reservation_mb);
    let allowed = req.cpu_mhz <= avail_cpu && req.memory_mb <= avail_mem;
    let result = AdmissionControlResult {
        allowed,
        reason: if allowed {
            None
        } else {
            Some("Insufficient resources".to_string())
        },
        available_cpu_mhz: avail_cpu,
        available_memory_mb: avail_mem,
    };
    Json(result).into_response()
}
