// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: LicenseRef-Zyvor-Production-2.0

use crate::server::AppState;
use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use chrono::Utc;
use fault_tolerance::{FailoverResult, FtConfig, FtEvent, FtMetrics, FtStatus, ReplicationState};
use security::{RequireRead, RequireWrite};
use std::sync::Arc;

#[derive(serde::Deserialize)]
pub struct EnableFtRequest {
    pub vm_name: String,
    pub primary_host_id: String,
    pub secondary_host_id: String,
}

pub async fn enable_ft(
    RequireWrite(_claims): RequireWrite,
    State(state): State<Arc<AppState>>,
    Json(req): Json<EnableFtRequest>,
) -> impl IntoResponse {
    tracing::debug!("fault_tolerance::{}", stringify!(enable_ft));
    if let Err((s, m)) = crate::validation::validate_vm_name(&req.vm_name) {
        return (s, Json(serde_json::json!({"error": m}))).into_response();
    }
    let now = Utc::now();
    let config = FtConfig {
        vm_name: req.vm_name.clone(),
        primary_host_id: req.primary_host_id,
        secondary_host_id: req.secondary_host_id,
        status: FtStatus::Enabled,
        replication_state: ReplicationState::Syncing,
        bandwidth_limit_mbps: None,
        last_sync: None,
        failover_count: 0,
        created: now,
        updated: now,
        lock_lease_id: None,
        zfs_dataset: None,
        zfs_last_replicated_snap: None,
        fence_token: None,
    };
    match state.store.save_entity("ft_configs", &req.vm_name, &config) {
        Ok(_) => (StatusCode::CREATED, Json(config)).into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": e.to_string()})),
        )
            .into_response(),
    }
}

pub async fn disable_ft(
    RequireWrite(_claims): RequireWrite,
    State(state): State<Arc<AppState>>,
    Path(vm_name): Path<String>,
) -> impl IntoResponse {
    tracing::debug!("fault_tolerance::{}", stringify!(disable_ft));
    if let Err((s, m)) = crate::validation::validate_vm_name(&vm_name) {
        return (s, Json(serde_json::json!({"error": m}))).into_response();
    }
    if let Err(e) = state.store.delete_entity("ft_configs", &vm_name) {
        tracing::error!("Failed to delete entity: {}", e);
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": e.to_string()})),
        )
            .into_response();
    }
    if let Err(e) = state.store.delete_entity("ft_metrics", &vm_name) {
        tracing::error!("Failed to delete entity: {}", e);
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": e.to_string()})),
        )
            .into_response();
    }
    StatusCode::NO_CONTENT.into_response()
}

pub async fn get_ft_config(
    RequireRead(_claims): RequireRead,
    State(state): State<Arc<AppState>>,
    Path(vm_name): Path<String>,
) -> impl IntoResponse {
    tracing::debug!("fault_tolerance::{}", stringify!(get_ft_config));
    if let Err((s, m)) = crate::validation::validate_vm_name(&vm_name) {
        return (s, Json(serde_json::json!({"error": m}))).into_response();
    }
    match state.store.get_entity::<FtConfig>("ft_configs", &vm_name) {
        Ok(Some(c)) => Json(c).into_response(),
        Ok(None) => (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({"error": "Fault tolerance config not found"})),
        )
            .into_response(),
        Err(_) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": "Failed to load fault tolerance config"})),
        )
            .into_response(),
    }
}

pub async fn list_ft_vms(
    RequireRead(_claims): RequireRead,
    State(state): State<Arc<AppState>>,
) -> impl IntoResponse {
    tracing::debug!("fault_tolerance::{}", stringify!(list_ft_vms));
    let items: Vec<FtConfig> = state.store.list_entities("ft_configs").unwrap_or_else(|e| {
        tracing::error!("Storage error: {}", e);
        Vec::new()
    });
    Json(items)
}

#[derive(serde::Deserialize)]
pub struct CompatibilityRequest {
    pub vm_name: String,
    pub cpus: u32,
    pub memory_mb: u64,
}

pub async fn check_ft_compatibility(
    RequireRead(_claims): RequireRead,
    State(_state): State<Arc<AppState>>,
    Json(req): Json<CompatibilityRequest>,
) -> impl IntoResponse {
    tracing::debug!("fault_tolerance::{}", stringify!(check_ft_compatibility));
    let mgr = fault_tolerance::FaultToleranceManager::new();
    let compat = mgr.check_compatibility(&req.vm_name, req.cpus, req.memory_mb);
    Json(compat)
}

pub async fn trigger_failover(
    RequireWrite(_claims): RequireWrite,
    State(state): State<Arc<AppState>>,
    Path(vm_name): Path<String>,
) -> impl IntoResponse {
    tracing::debug!("fault_tolerance::{}", stringify!(trigger_failover));
    if let Err((s, m)) = crate::validation::validate_vm_name(&vm_name) {
        return (s, Json(serde_json::json!({"error": m}))).into_response();
    }
    let mut config = match state.store.get_entity::<FtConfig>("ft_configs", &vm_name) {
        Ok(Some(c)) => c,
        Ok(None) => {
            return (
                StatusCode::NOT_FOUND,
                Json(serde_json::json!({"error": "Fault tolerance config not found"})),
            )
                .into_response()
        }
        Err(_) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"error": "Failed to load fault tolerance config"})),
            )
                .into_response()
        }
    };
    if config.status != FtStatus::Enabled {
        return (
            StatusCode::CONFLICT,
            Json(serde_json::json!({"error": "Fault tolerance is not enabled for this VM"})),
        )
            .into_response();
    }

    let old_primary = config.primary_host_id.clone();
    let new_primary = config.secondary_host_id.clone();
    let start_time = std::time::Instant::now();

    // Fence the old primary via the FluxVM VM driver rather than a direct shellout.
    tracing::info!(vm = %vm_name, host = %old_primary, "FT failover: fencing VM on old primary");
    let fence_method = match state.driver.poweroff(&vm_name).await {
        Ok(()) => "poweroff".to_string(),
        Err(e) => {
            tracing::warn!(vm = %vm_name, error = %e, "FT failover: graceful poweroff failed, force terminating");
            let _ = state.driver.terminate(&vm_name).await;
            "terminate".to_string()
        }
    };

    config.primary_host_id = new_primary.clone();
    config.secondary_host_id = String::new();
    config.status = FtStatus::NeedSecondary;
    config.replication_state = ReplicationState::OutOfSync;
    config.failover_count += 1;
    config.updated = Utc::now();
    if let Err(e) = state.store.save_entity("ft_configs", &vm_name, &config) {
        tracing::error!("Failed to save entity: {}", e);
    }

    let (success, error) = match state.driver.start(&vm_name).await {
        Ok(()) => (true, None),
        Err(e) => (
            false,
            Some(format!("failed to start VM on new primary: {e:#}")),
        ),
    };

    let downtime_ms = start_time.elapsed().as_millis() as u64;
    let result = FailoverResult {
        vm_name,
        old_primary,
        new_primary,
        downtime_ms,
        data_loss: false,
        success,
        error,
        fence_method: Some(fence_method),
        storage_promoted: success,
        replication_lag_secs: None,
    };
    Json(result).into_response()
}

pub async fn test_failover(
    RequireWrite(_claims): RequireWrite,
    State(state): State<Arc<AppState>>,
    Path(vm_name): Path<String>,
) -> impl IntoResponse {
    tracing::debug!("fault_tolerance::{}", stringify!(test_failover));
    if let Err((s, m)) = crate::validation::validate_vm_name(&vm_name) {
        return (s, Json(serde_json::json!({"error": m}))).into_response();
    }
    let config = match state.store.get_entity::<FtConfig>("ft_configs", &vm_name) {
        Ok(Some(c)) => c,
        Ok(None) => {
            return (
                StatusCode::NOT_FOUND,
                Json(serde_json::json!({"error": "Fault tolerance config not found"})),
            )
                .into_response()
        }
        Err(_) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"error": "Failed to load fault tolerance config"})),
            )
                .into_response()
        }
    };
    let result = FailoverResult {
        vm_name,
        old_primary: config.primary_host_id,
        new_primary: config.secondary_host_id,
        downtime_ms: 0,
        data_loss: false,
        success: true,
        error: None,
        fence_method: None,
        storage_promoted: false,
        replication_lag_secs: None,
    };
    Json(result).into_response()
}

pub async fn suspend_replication(
    RequireWrite(_claims): RequireWrite,
    State(state): State<Arc<AppState>>,
    Path(vm_name): Path<String>,
) -> impl IntoResponse {
    tracing::debug!("fault_tolerance::{}", stringify!(suspend_replication));
    if let Err((s, m)) = crate::validation::validate_vm_name(&vm_name) {
        return (s, Json(serde_json::json!({"error": m}))).into_response();
    }
    let mut config = match state.store.get_entity::<FtConfig>("ft_configs", &vm_name) {
        Ok(Some(c)) => c,
        Ok(None) => {
            return (
                StatusCode::NOT_FOUND,
                Json(serde_json::json!({"error": "Fault tolerance config not found"})),
            )
                .into_response()
        }
        Err(_) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"error": "Failed to load fault tolerance config"})),
            )
                .into_response()
        }
    };
    config.replication_state = ReplicationState::Suspended;
    config.updated = Utc::now();
    if let Err(e) = state.store.save_entity("ft_configs", &vm_name, &config) {
        tracing::error!("Failed to save entity: {}", e);
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": e.to_string()})),
        )
            .into_response();
    }
    StatusCode::OK.into_response()
}

pub async fn resume_replication(
    RequireWrite(_claims): RequireWrite,
    State(state): State<Arc<AppState>>,
    Path(vm_name): Path<String>,
) -> impl IntoResponse {
    tracing::debug!("fault_tolerance::{}", stringify!(resume_replication));
    if let Err((s, m)) = crate::validation::validate_vm_name(&vm_name) {
        return (s, Json(serde_json::json!({"error": m}))).into_response();
    }
    let mut config = match state.store.get_entity::<FtConfig>("ft_configs", &vm_name) {
        Ok(Some(c)) => c,
        Ok(None) => {
            return (
                StatusCode::NOT_FOUND,
                Json(serde_json::json!({"error": "Fault tolerance config not found"})),
            )
                .into_response()
        }
        Err(_) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"error": "Failed to load fault tolerance config"})),
            )
                .into_response()
        }
    };
    config.replication_state = ReplicationState::Syncing;
    config.updated = Utc::now();
    if let Err(e) = state.store.save_entity("ft_configs", &vm_name, &config) {
        tracing::error!("Failed to save entity: {}", e);
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": e.to_string()})),
        )
            .into_response();
    }
    StatusCode::OK.into_response()
}

pub async fn get_ft_metrics(
    RequireRead(_claims): RequireRead,
    State(state): State<Arc<AppState>>,
    Path(vm_name): Path<String>,
) -> impl IntoResponse {
    tracing::debug!("fault_tolerance::{}", stringify!(get_ft_metrics));
    if let Err((s, m)) = crate::validation::validate_vm_name(&vm_name) {
        return (s, Json(serde_json::json!({"error": m}))).into_response();
    }
    match state.store.get_entity::<FtMetrics>("ft_metrics", &vm_name) {
        Ok(Some(m)) => Json(m).into_response(),
        Ok(None) => (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({"error": "Fault tolerance metrics not found"})),
        )
            .into_response(),
        Err(_) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": "Failed to load fault tolerance metrics"})),
        )
            .into_response(),
    }
}

pub async fn get_ft_events(
    RequireRead(_claims): RequireRead,
    State(state): State<Arc<AppState>>,
) -> impl IntoResponse {
    tracing::debug!("fault_tolerance::{}", stringify!(get_ft_events));
    let items: Vec<FtEvent> = state.store.list_entities("ft_events").unwrap_or_else(|e| {
        tracing::error!("Storage error: {}", e);
        Vec::new()
    });
    Json(items)
}
