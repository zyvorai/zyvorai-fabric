// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: LicenseRef-Zyvor-Production-2.0

//! Warm VM pools -- pre-boot N VMs from a template, pause each once ready,
//! then hand one out instantly on claim instead of a slow cold create+boot.
//! Backed by `state.driver`'s `PoolDriver` (FluxVM's `/v1/pools` API).
//! Named `vm-pools` in the URL, not `pools`, to stay unambiguous next to
//! the unrelated `/api/resource-pools` (CPU/memory share allocation).

use axum::{extract::State, http::StatusCode, Json};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

use crate::server::AppState;
use crate::validation::validate_vm_name;
use security::{RequireAdmin, RequireRead};

#[derive(Debug, Serialize)]
pub struct PoolResponse {
    pub name: String,
    pub size: usize,
    pub image: String,
    pub cpus: u32,
    pub memory: u64,
    pub ready_members: usize,
}

impl From<zyvor_fabric_driver_core::PoolInfo> for PoolResponse {
    fn from(p: zyvor_fabric_driver_core::PoolInfo) -> Self {
        Self {
            name: p.name,
            size: p.size,
            image: p.image,
            cpus: p.cpus,
            memory: p.memory,
            ready_members: p.ready_members,
        }
    }
}

#[derive(Debug, Deserialize)]
pub struct CreatePoolRequest {
    pub name: String,
    pub size: usize,
    pub image: String,
    #[serde(default = "default_cpus")]
    pub cpus: u32,
    #[serde(default = "default_memory")]
    pub memory: u64,
}

fn default_cpus() -> u32 {
    2
}
fn default_memory() -> u64 {
    2048
}

/// POST /api/vm-pools
pub async fn create_pool(
    RequireAdmin(_claims): RequireAdmin,
    State(state): State<Arc<AppState>>,
    Json(req): Json<CreatePoolRequest>,
) -> Result<(StatusCode, Json<PoolResponse>), (StatusCode, Json<serde_json::Value>)> {
    validate_vm_name(&req.name).map_err(|(s, m)| crate::api_error::json_error(s, m))?;
    if req.size == 0 || req.size > 64 {
        return Err(crate::api_error::json_error(
            StatusCode::BAD_REQUEST,
            "Pool size must be between 1 and 64",
        ));
    }
    if req.image.trim().is_empty() {
        return Err(crate::api_error::json_error(
            StatusCode::BAD_REQUEST,
            "Image path is required",
        ));
    }
    let pool = state
        .driver
        .create_pool(&req.name, req.size, &req.image, req.cpus, req.memory)
        .await
        .map_err(|e| {
            crate::api_error::json_error(StatusCode::INTERNAL_SERVER_ERROR, e.to_string())
        })?;
    tracing::info!("Created warm pool '{}' (size {})", req.name, req.size);
    Ok((StatusCode::CREATED, Json(pool.into())))
}

/// GET /api/vm-pools
pub async fn list_pools(
    RequireRead(_claims): RequireRead,
    State(state): State<Arc<AppState>>,
) -> Result<Json<Vec<PoolResponse>>, (StatusCode, Json<serde_json::Value>)> {
    let pools = state.driver.list_pools().await.map_err(|e| {
        crate::api_error::json_error(StatusCode::INTERNAL_SERVER_ERROR, e.to_string())
    })?;
    Ok(Json(pools.into_iter().map(Into::into).collect()))
}

/// GET /api/vm-pools/:name
pub async fn get_pool(
    RequireRead(_claims): RequireRead,
    State(state): State<Arc<AppState>>,
    axum::extract::Path(name): axum::extract::Path<String>,
) -> Result<Json<PoolResponse>, (StatusCode, Json<serde_json::Value>)> {
    validate_vm_name(&name).map_err(|(s, m)| crate::api_error::json_error(s, m))?;
    let pool = state.driver.get_pool(&name).await.map_err(|e| {
        crate::api_error::json_error(
            StatusCode::NOT_FOUND,
            format!("Pool '{}' not found: {}", name, e),
        )
    })?;
    Ok(Json(pool.into()))
}

/// DELETE /api/vm-pools/:name -- FluxVM tears down every member VM as
/// part of this one call, which for a pool of any real size easily runs
/// past the server's 60s request timeout even though the deletion itself
/// keeps going and eventually succeeds. Confirm the pool exists (fast),
/// then let the actual teardown run in the background so the client isn't
/// stuck waiting on (or timing out against) it.
pub async fn delete_pool(
    RequireAdmin(_claims): RequireAdmin,
    State(state): State<Arc<AppState>>,
    axum::extract::Path(name): axum::extract::Path<String>,
) -> Result<StatusCode, (StatusCode, Json<serde_json::Value>)> {
    validate_vm_name(&name).map_err(|(s, m)| crate::api_error::json_error(s, m))?;
    state.driver.get_pool(&name).await.map_err(|e| {
        crate::api_error::json_error(
            StatusCode::NOT_FOUND,
            format!("Pool '{}' not found: {}", name, e),
        )
    })?;

    let driver = state.driver.clone();
    let name_clone = name.clone();
    tokio::spawn(async move {
        match driver.delete_pool(&name_clone).await {
            Ok(_) => tracing::info!("Deleted warm pool '{}'", name_clone),
            Err(e) => tracing::error!("Failed to delete warm pool '{}': {}", name_clone, e),
        }
    });
    Ok(StatusCode::ACCEPTED)
}

#[derive(Debug, Deserialize)]
pub struct ClaimPoolRequest {
    pub name: String,
    #[serde(default)]
    pub ttl_seconds: Option<u64>,
}

/// POST /api/vm-pools/:name/claim
pub async fn claim_pool(
    RequireAdmin(_claims): RequireAdmin,
    State(state): State<Arc<AppState>>,
    axum::extract::Path(pool_name): axum::extract::Path<String>,
    Json(req): Json<ClaimPoolRequest>,
) -> Result<(StatusCode, Json<vm_model::VM>), (StatusCode, Json<serde_json::Value>)> {
    validate_vm_name(&pool_name).map_err(|(s, m)| crate::api_error::json_error(s, m))?;
    validate_vm_name(&req.name).map_err(|(s, m)| crate::api_error::json_error(s, m))?;

    let _lock = state.vm_lock(&req.name).lock_owned().await;
    if let Ok(Some(_)) = state.store.get_vm(&req.name) {
        return Err(crate::api_error::json_error(
            StatusCode::CONFLICT,
            "VM with this name already exists",
        ));
    }

    let vm = state
        .driver
        .claim_pool(&pool_name, &req.name, req.ttl_seconds)
        .await
        .map_err(|e| {
            crate::api_error::json_error(
                StatusCode::CONFLICT,
                format!("Failed to claim from pool '{}': {}", pool_name, e),
            )
        })?;

    // The claimed VM is already running in FluxVM -- mirror it into
    // zyvor-fabric's own store so it shows up like any other VM from here.
    state.store.save_vm(&vm).map_err(|e| {
        crate::api_error::json_error(StatusCode::INTERNAL_SERVER_ERROR, e.to_string())
    })?;

    crate::api::events::record_event(
        &state,
        crate::api::events::VMEventType::Created,
        &vm.name,
        None,
    );
    tracing::info!("Claimed '{}' from warm pool '{}'", vm.name, pool_name);
    Ok((StatusCode::CREATED, Json(vm)))
}
