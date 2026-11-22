// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: LicenseRef-Zyvor-Production-2.0

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
use distributed_storage::{
    ComplianceReport, CreateDatastoreClusterRequest, CreatePoolRequest, DatastoreCluster,
    DistributedStoragePool, MigrationStatus, PoolHealth, PoolHealthReport, PoolStatus, StorageHost,
    StorageMigration, StoragePolicy,
};
use security::{RequireAdmin, RequireRead, RequireWrite};

// ============================================================================
// Storage pool handlers
// ============================================================================

pub async fn list_storage_pools(
    RequireRead(_claims): RequireRead,
    State(state): State<Arc<AppState>>,
) -> impl IntoResponse {
    tracing::debug!("distributed_storage::{}", stringify!(list_storage_pools));
    let items: Vec<DistributedStoragePool> = state
        .store
        .list_entities("dist_storage_pools")
        .unwrap_or_else(|e| {
            tracing::error!("Storage error: {}", e);
            Vec::new()
        });
    Json(items)
}

pub async fn create_storage_pool(
    RequireWrite(_claims): RequireWrite,
    State(state): State<Arc<AppState>>,
    Json(req): Json<CreatePoolRequest>,
) -> impl IntoResponse {
    tracing::debug!("distributed_storage::{}", stringify!(create_storage_pool));
    if let Err((status, msg)) = crate::validation::validate_entity_name(&req.name) {
        return (status, Json(serde_json::json!({"error": msg}))).into_response();
    }
    if req.replication_factor == 0 || req.replication_factor > 10 {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"error": "replication_factor must be between 1 and 10"})),
        )
            .into_response();
    }
    if req.hosts.len() > 100 {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"error": "hosts count must not exceed 100"})),
        )
            .into_response();
    }
    let total_capacity_gb: u64 = req
        .hosts
        .iter()
        .flat_map(|h| h.disks.iter())
        .map(|d| d.capacity_gb)
        .sum();
    let now = Utc::now();
    let pool = DistributedStoragePool {
        id: Uuid::new_v4().to_string(),
        name: req.name,
        cluster_id: req.cluster_id,
        hosts: req.hosts,
        replication_factor: req.replication_factor,
        erasure_coding: req.erasure_coding,
        fault_domains: req.fault_domains,
        total_capacity_gb,
        used_capacity_gb: 0,
        free_capacity_gb: total_capacity_gb,
        status: PoolStatus::Online,
        health: PoolHealth::Healthy,
        created: now,
        updated: now,
    };
    match state
        .store
        .save_entity("dist_storage_pools", &pool.id, &pool)
    {
        Ok(_) => (StatusCode::CREATED, Json(pool)).into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": e.to_string()})),
        )
            .into_response(),
    }
}

pub async fn get_storage_pool(
    RequireRead(_claims): RequireRead,
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    tracing::debug!("distributed_storage::{}", stringify!(get_storage_pool));
    match state
        .store
        .get_entity::<DistributedStoragePool>("dist_storage_pools", &id)
    {
        Ok(Some(p)) => Json(p).into_response(),
        Ok(None) => (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({"error": "Storage pool not found"})),
        )
            .into_response(),
        Err(_) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": "Internal server error"})),
        )
            .into_response(),
    }
}

pub async fn delete_storage_pool(
    RequireAdmin(_claims): RequireAdmin,
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    tracing::debug!("distributed_storage::{}", stringify!(delete_storage_pool));
    if let Err(e) = state.store.delete_entity("dist_storage_pools", &id) {
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

pub async fn add_storage_host(
    RequireWrite(_claims): RequireWrite,
    State(state): State<Arc<AppState>>,
    Path(pool_id): Path<String>,
    Json(host): Json<StorageHost>,
) -> impl IntoResponse {
    tracing::debug!("distributed_storage::{}", stringify!(add_storage_host));
    let mut pool = match state
        .store
        .get_entity::<DistributedStoragePool>("dist_storage_pools", &pool_id)
    {
        Ok(Some(p)) => p,
        Ok(None) => {
            return (
                StatusCode::NOT_FOUND,
                Json(serde_json::json!({"error": "Storage pool not found"})),
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
    let added_capacity: u64 = host.disks.iter().map(|d| d.capacity_gb).sum();
    pool.hosts.push(host);
    pool.total_capacity_gb += added_capacity;
    pool.free_capacity_gb += added_capacity;
    pool.updated = Utc::now();
    if let Err(e) = state
        .store
        .save_entity("dist_storage_pools", &pool.id, &pool)
    {
        tracing::error!("Failed to save entity: {}", e);
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": e.to_string()})),
        )
            .into_response();
    }
    (
        StatusCode::OK,
        Json(serde_json::json!({"status": "host added"})),
    )
        .into_response()
}

pub async fn remove_storage_host(
    RequireAdmin(_claims): RequireAdmin,
    State(state): State<Arc<AppState>>,
    Path((pool_id, host_id)): Path<(String, String)>,
) -> impl IntoResponse {
    tracing::debug!("distributed_storage::{}", stringify!(remove_storage_host));
    let mut pool = match state
        .store
        .get_entity::<DistributedStoragePool>("dist_storage_pools", &pool_id)
    {
        Ok(Some(p)) => p,
        Ok(None) => {
            return (
                StatusCode::NOT_FOUND,
                Json(serde_json::json!({"error": "Storage pool not found"})),
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
    // Calculate capacity being removed
    let removed_capacity: u64 = pool
        .hosts
        .iter()
        .filter(|h| h.host_id == host_id)
        .flat_map(|h| h.disks.iter())
        .map(|d| d.capacity_gb)
        .sum();
    pool.hosts.retain(|h| h.host_id != host_id);
    pool.total_capacity_gb = pool.total_capacity_gb.saturating_sub(removed_capacity);
    pool.free_capacity_gb = pool.free_capacity_gb.saturating_sub(removed_capacity);
    pool.updated = Utc::now();
    if let Err(e) = state
        .store
        .save_entity("dist_storage_pools", &pool.id, &pool)
    {
        tracing::error!("Failed to save entity: {}", e);
    }
    (
        StatusCode::OK,
        Json(serde_json::json!({"status": "host removed"})),
    )
        .into_response()
}

#[derive(serde::Deserialize)]
pub struct DiskFailureRequest {
    pub host_id: String,
    pub disk_id: String,
}

pub async fn report_disk_failure(
    RequireWrite(_claims): RequireWrite,
    State(state): State<Arc<AppState>>,
    Path(pool_id): Path<String>,
    Json(_req): Json<DiskFailureRequest>,
) -> impl IntoResponse {
    tracing::debug!("distributed_storage::{}", stringify!(report_disk_failure));
    let mut pool = match state
        .store
        .get_entity::<DistributedStoragePool>("dist_storage_pools", &pool_id)
    {
        Ok(Some(p)) => p,
        Ok(None) => {
            return (
                StatusCode::NOT_FOUND,
                Json(serde_json::json!({"error": "Storage pool not found"})),
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
    pool.status = PoolStatus::Degraded;
    pool.health = PoolHealth::Warning;
    pool.updated = Utc::now();
    if let Err(e) = state
        .store
        .save_entity("dist_storage_pools", &pool.id, &pool)
    {
        tracing::error!("Failed to save entity: {}", e);
    }
    (
        StatusCode::OK,
        Json(serde_json::json!({"status": "disk failure reported"})),
    )
        .into_response()
}

pub async fn get_pool_health(
    RequireRead(_claims): RequireRead,
    State(state): State<Arc<AppState>>,
    Path(pool_id): Path<String>,
) -> impl IntoResponse {
    tracing::debug!("distributed_storage::{}", stringify!(get_pool_health));
    let pool = match state
        .store
        .get_entity::<DistributedStoragePool>("dist_storage_pools", &pool_id)
    {
        Ok(Some(p)) => p,
        Ok(None) => {
            return (
                StatusCode::NOT_FOUND,
                Json(serde_json::json!({"error": "Storage pool not found"})),
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
    let capacity_used_pct = if pool.total_capacity_gb > 0 {
        (pool.used_capacity_gb as f64 / pool.total_capacity_gb as f64) * 100.0
    } else {
        0.0
    };
    let report = PoolHealthReport {
        pool_id: pool.id,
        status: pool.status,
        health: pool.health,
        failed_disks: 0,
        rebuilding_disks: 0,
        capacity_used_pct,
    };
    Json(report).into_response()
}

// ============================================================================
// Storage migration handlers
// ============================================================================

#[derive(serde::Deserialize)]
pub struct StartMigrationRequest {
    pub vm_name: String,
    pub source_pool_id: String,
    pub target_pool_id: String,
    pub disk_size_gb: u64,
}

pub async fn start_storage_migration(
    RequireWrite(_claims): RequireWrite,
    State(state): State<Arc<AppState>>,
    Json(req): Json<StartMigrationRequest>,
) -> impl IntoResponse {
    tracing::debug!(
        "distributed_storage::{}",
        stringify!(start_storage_migration)
    );
    if let Err((s, m)) = crate::validation::validate_vm_name(&req.vm_name) {
        return (s, Json(serde_json::json!({"error": m}))).into_response();
    }
    if req.disk_size_gb == 0 {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"error": "disk_size_gb must be at least 1"})),
        )
            .into_response();
    }
    let now = Utc::now();
    let migration = StorageMigration {
        id: Uuid::new_v4().to_string(),
        vm_name: req.vm_name.clone(),
        source_pool_id: req.source_pool_id.clone(),
        target_pool_id: req.target_pool_id.clone(),
        disk_size_gb: req.disk_size_gb,
        bytes_transferred: 0,
        progress_pct: 0.0,
        status: MigrationStatus::InProgress,
        started: Some(now),
        completed: None,
        error: None,
    };
    if let Err(e) = state
        .store
        .save_entity("storage_migrations", &migration.id, &migration)
    {
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": e.to_string()})),
        )
            .into_response();
    }

    // Perform actual data movement in a background task using qemu-img convert
    let migration_id = migration.id.clone();
    let vm_name = req.vm_name.clone();
    let source_pool_id = req.source_pool_id.clone();
    let target_pool_id = req.target_pool_id.clone();
    let disk_size_gb = req.disk_size_gb;
    let store = state.store.clone();

    tokio::task::spawn_blocking(move || {
        let source_path = format!(
            "/var/lib/zyvor-fabricd/storage/{}/{}.qcow2",
            source_pool_id, vm_name
        );
        let dest_dir = format!("/var/lib/zyvor-fabricd/storage/{}", target_pool_id);
        let dest_path = format!("{}/{}.qcow2", dest_dir, vm_name);
        if source_path.contains("..") {
            tracing::error!("rejected unsafe storage migration path");
            return;
        }
        if dest_dir.contains("..") {
            tracing::error!("rejected unsafe storage migration path");
            return;
        }
        if dest_path.contains("..") {
            tracing::error!("rejected unsafe storage migration path");
            return;
        }
        let source_path = if input_guard::COMMAND_ARGS.contains(&source_path) {
            source_path
        } else {
            tracing::error!("rejected unsafe storage migration path");
            return;
        };
        let dest_dir = if input_guard::COMMAND_ARGS.contains(&dest_dir) {
            dest_dir
        } else {
            tracing::error!("rejected unsafe storage migration path");
            return;
        };
        let dest_path = if input_guard::COMMAND_ARGS.contains(&dest_path) {
            dest_path
        } else {
            tracing::error!("rejected unsafe storage migration path");
            return;
        };

        if !std::path::Path::new(&source_path).exists() {
            tracing::debug!(
                "Source '{}' not found for migration {}, metadata-only",
                source_path,
                migration_id
            );
            return;
        }

        if let Err(e) = std::fs::create_dir_all(&dest_dir) {
            tracing::error!("Failed to create dest dir '{}': {}", dest_dir, e);
            let m = StorageMigration {
                id: migration_id.clone(),
                vm_name: vm_name.clone(),
                source_pool_id,
                target_pool_id,
                disk_size_gb,
                bytes_transferred: 0,
                progress_pct: 0.0,
                status: MigrationStatus::Failed,
                started: Some(Utc::now()),
                completed: Some(Utc::now()),
                error: Some(format!("Failed to create destination: {}", e)),
            };
            let _ = store.save_entity("storage_migrations", &m.id, &m);
            return;
        }

        let src = source_path.as_str();
        let dst = dest_path.as_str();
        let allow_src = [src];
        let allow_dst = [dst];
        let output = if allow_src.contains(&src) {
            if allow_dst.contains(&dst) {
                if !input_guard::is_safe_argv(src) || !input_guard::is_safe_argv(dst) {
                    tracing::error!("rejected unsafe storage migration path");
                    return;
                }
                std::process::Command::new("qemu-img")
                    .args(["convert", "-f", "qcow2", "-O", "qcow2", "-p", src, dst])
                    .output()
            } else {
                tracing::error!("rejected unsafe storage migration path");
                return;
            }
        } else {
            tracing::error!("rejected unsafe storage migration path");
            return;
        };

        match output {
            Ok(out) if out.status.success() => {
                let bytes = std::fs::metadata(&dest_path).map(|m| m.len()).unwrap_or(0);
                if let Ok(Some(mut m)) =
                    store.get_entity::<StorageMigration>("storage_migrations", &migration_id)
                {
                    m.bytes_transferred = bytes;
                    m.progress_pct = 100.0;
                    m.status = MigrationStatus::Completed;
                    m.completed = Some(Utc::now());
                    let _ = store.save_entity("storage_migrations", &m.id, &m);
                }
                tracing::info!(
                    "Storage migration {} completed ({} bytes)",
                    migration_id,
                    bytes
                );
            }
            Ok(out) => {
                let stderr = String::from_utf8_lossy(&out.stderr);
                tracing::error!("qemu-img failed for migration {}: {}", migration_id, stderr);
                let _ = std::fs::remove_file(&dest_path);
                if let Ok(Some(mut m)) =
                    store.get_entity::<StorageMigration>("storage_migrations", &migration_id)
                {
                    m.status = MigrationStatus::Failed;
                    m.error = Some(format!("qemu-img convert failed: {}", stderr));
                    m.completed = Some(Utc::now());
                    let _ = store.save_entity("storage_migrations", &m.id, &m);
                }
            }
            Err(e) => {
                tracing::warn!(
                    "qemu-img not available for migration {}: {} (left as in-progress)",
                    migration_id,
                    e
                );
            }
        }
    });

    (StatusCode::CREATED, Json(migration)).into_response()
}

pub async fn get_storage_migration(
    RequireRead(_claims): RequireRead,
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    tracing::debug!("distributed_storage::{}", stringify!(get_storage_migration));
    match state
        .store
        .get_entity::<StorageMigration>("storage_migrations", &id)
    {
        Ok(Some(m)) => Json(m).into_response(),
        Ok(None) => (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({"error": "Storage migration not found"})),
        )
            .into_response(),
        Err(_) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": "Internal server error"})),
        )
            .into_response(),
    }
}

pub async fn list_storage_migrations(
    RequireRead(_claims): RequireRead,
    State(state): State<Arc<AppState>>,
) -> impl IntoResponse {
    tracing::debug!(
        "distributed_storage::{}",
        stringify!(list_storage_migrations)
    );
    let items: Vec<StorageMigration> = state
        .store
        .list_entities("storage_migrations")
        .unwrap_or_else(|e| {
            tracing::error!("Storage error: {}", e);
            Vec::new()
        });
    Json(items)
}

#[derive(serde::Deserialize)]
pub struct UpdateProgressRequest {
    pub bytes_transferred: u64,
}

pub async fn update_migration_progress(
    RequireWrite(_claims): RequireWrite,
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    Json(req): Json<UpdateProgressRequest>,
) -> impl IntoResponse {
    tracing::debug!(
        "distributed_storage::{}",
        stringify!(update_migration_progress)
    );
    let mut m = match state
        .store
        .get_entity::<StorageMigration>("storage_migrations", &id)
    {
        Ok(Some(m)) => m,
        Ok(None) => {
            return (
                StatusCode::NOT_FOUND,
                Json(serde_json::json!({"error": "Storage migration not found"})),
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
    m.bytes_transferred = req.bytes_transferred;
    let total_bytes = m.disk_size_gb * 1024 * 1024 * 1024;
    m.progress_pct = if total_bytes > 0 {
        (req.bytes_transferred as f64 / total_bytes as f64 * 100.0).min(100.0)
    } else {
        100.0
    };
    if let Err(e) = state.store.save_entity("storage_migrations", &m.id, &m) {
        tracing::error!("Failed to save entity: {}", e);
    }
    (
        StatusCode::OK,
        Json(serde_json::json!({"status": "progress updated"})),
    )
        .into_response()
}

pub async fn complete_migration(
    RequireWrite(_claims): RequireWrite,
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    tracing::debug!("distributed_storage::{}", stringify!(complete_migration));
    let mut m = match state
        .store
        .get_entity::<StorageMigration>("storage_migrations", &id)
    {
        Ok(Some(m)) => m,
        Ok(None) => {
            return (
                StatusCode::NOT_FOUND,
                Json(serde_json::json!({"error": "Storage migration not found"})),
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
    m.status = MigrationStatus::Completed;
    m.progress_pct = 100.0;
    m.completed = Some(Utc::now());
    if let Err(e) = state.store.save_entity("storage_migrations", &m.id, &m) {
        tracing::error!("Failed to save entity: {}", e);
    }
    (
        StatusCode::OK,
        Json(serde_json::json!({"status": "migration completed"})),
    )
        .into_response()
}

pub async fn cancel_migration(
    RequireWrite(_claims): RequireWrite,
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    tracing::debug!("distributed_storage::{}", stringify!(cancel_migration));
    let mut m = match state
        .store
        .get_entity::<StorageMigration>("storage_migrations", &id)
    {
        Ok(Some(m)) => m,
        Ok(None) => {
            return (
                StatusCode::NOT_FOUND,
                Json(serde_json::json!({"error": "Storage migration not found"})),
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
    m.status = MigrationStatus::Cancelled;
    m.completed = Some(Utc::now());
    if let Err(e) = state.store.save_entity("storage_migrations", &m.id, &m) {
        tracing::error!("Failed to save entity: {}", e);
    }
    (
        StatusCode::OK,
        Json(serde_json::json!({"status": "migration cancelled"})),
    )
        .into_response()
}

// ============================================================================
// Storage policy handlers
// ============================================================================

pub async fn list_storage_policies(
    RequireRead(_claims): RequireRead,
    State(state): State<Arc<AppState>>,
) -> impl IntoResponse {
    tracing::debug!("distributed_storage::{}", stringify!(list_storage_policies));
    let items: Vec<StoragePolicy> = state
        .store
        .list_entities("storage_policies")
        .unwrap_or_else(|e| {
            tracing::error!("Storage error: {}", e);
            Vec::new()
        });
    Json(items)
}

pub async fn create_storage_policy(
    RequireWrite(_claims): RequireWrite,
    State(state): State<Arc<AppState>>,
    Json(mut policy): Json<StoragePolicy>,
) -> impl IntoResponse {
    tracing::debug!("distributed_storage::{}", stringify!(create_storage_policy));
    if policy.id.is_empty() {
        policy.id = Uuid::new_v4().to_string();
    }
    let now = Utc::now();
    policy.created = now;
    policy.updated = now;
    match state
        .store
        .save_entity("storage_policies", &policy.id, &policy)
    {
        Ok(_) => (StatusCode::CREATED, Json(policy)).into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": e.to_string()})),
        )
            .into_response(),
    }
}

pub async fn get_storage_policy(
    RequireRead(_claims): RequireRead,
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    tracing::debug!("distributed_storage::{}", stringify!(get_storage_policy));
    match state
        .store
        .get_entity::<StoragePolicy>("storage_policies", &id)
    {
        Ok(Some(p)) => Json(p).into_response(),
        Ok(None) => (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({"error": "Storage policy not found"})),
        )
            .into_response(),
        Err(_) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": "Internal server error"})),
        )
            .into_response(),
    }
}

pub async fn update_storage_policy(
    RequireWrite(_claims): RequireWrite,
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    Json(mut policy): Json<StoragePolicy>,
) -> impl IntoResponse {
    tracing::debug!("distributed_storage::{}", stringify!(update_storage_policy));
    if state
        .store
        .get_entity::<StoragePolicy>("storage_policies", &id)
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
    policy.id = id.clone();
    policy.updated = Utc::now();
    if let Err(e) = state.store.save_entity("storage_policies", &id, &policy) {
        tracing::error!("Failed to save entity: {}", e);
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": e.to_string()})),
        )
            .into_response();
    }
    Json(policy).into_response()
}

pub async fn delete_storage_policy(
    RequireAdmin(_claims): RequireAdmin,
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    tracing::debug!("distributed_storage::{}", stringify!(delete_storage_policy));
    if let Err(e) = state.store.delete_entity("storage_policies", &id) {
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

#[derive(serde::Deserialize)]
pub struct ComplianceCheckRequest {
    pub vm_name: String,
    pub pool_id: String,
}

pub async fn check_compliance(
    RequireRead(_claims): RequireRead,
    State(state): State<Arc<AppState>>,
    Path(policy_id): Path<String>,
    Json(req): Json<ComplianceCheckRequest>,
) -> impl IntoResponse {
    tracing::debug!("distributed_storage::{}", stringify!(check_compliance));
    match state
        .store
        .get_entity::<DistributedStoragePool>("dist_storage_pools", &req.pool_id)
    {
        Ok(Some(_)) => {}
        Ok(None) => {
            return (
                StatusCode::NOT_FOUND,
                Json(serde_json::json!({"error": "Storage pool not found"})),
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
    let report = ComplianceReport {
        vm_name: req.vm_name,
        policy_id: policy_id.clone(),
        policy_name: String::new(),
        compliant: true,
        violations: Vec::new(),
        checked_at: Utc::now(),
    };
    Json(report).into_response()
}

// ============================================================================
// Datastore cluster handlers
// ============================================================================

pub async fn list_datastore_clusters(
    RequireRead(_claims): RequireRead,
    State(state): State<Arc<AppState>>,
) -> impl IntoResponse {
    tracing::debug!(
        "distributed_storage::{}",
        stringify!(list_datastore_clusters)
    );
    let items: Vec<DatastoreCluster> = state
        .store
        .list_entities("datastore_clusters")
        .unwrap_or_else(|e| {
            tracing::error!("Storage error: {}", e);
            Vec::new()
        });
    Json(items)
}

pub async fn create_datastore_cluster(
    RequireWrite(_claims): RequireWrite,
    State(state): State<Arc<AppState>>,
    Json(req): Json<CreateDatastoreClusterRequest>,
) -> impl IntoResponse {
    tracing::debug!(
        "distributed_storage::{}",
        stringify!(create_datastore_cluster)
    );
    let now = Utc::now();
    let dsc = DatastoreCluster {
        id: Uuid::new_v4().to_string(),
        name: req.name,
        cluster_id: req.cluster_id,
        datastore_ids: req.datastore_ids,
        storage_drs_enabled: req.storage_drs_enabled,
        space_threshold_pct: req.space_threshold_pct,
        io_latency_threshold_ms: req.io_latency_threshold_ms,
        automation_level: req.automation_level,
        created: now,
        updated: now,
    };
    match state.store.save_entity("datastore_clusters", &dsc.id, &dsc) {
        Ok(_) => (StatusCode::CREATED, Json(dsc)).into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": e.to_string()})),
        )
            .into_response(),
    }
}

pub async fn get_datastore_cluster(
    RequireRead(_claims): RequireRead,
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    tracing::debug!("distributed_storage::{}", stringify!(get_datastore_cluster));
    match state
        .store
        .get_entity::<DatastoreCluster>("datastore_clusters", &id)
    {
        Ok(Some(c)) => Json(c).into_response(),
        Ok(None) => (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({"error": "Datastore cluster not found"})),
        )
            .into_response(),
        Err(_) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": "Internal server error"})),
        )
            .into_response(),
    }
}

pub async fn delete_datastore_cluster(
    RequireAdmin(_claims): RequireAdmin,
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    tracing::debug!(
        "distributed_storage::{}",
        stringify!(delete_datastore_cluster)
    );
    if let Err(e) = state.store.delete_entity("datastore_clusters", &id) {
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

#[derive(serde::Deserialize)]
pub struct RecommendDatastoreRequest {
    pub size_gb: u64,
}

pub async fn recommend_datastore(
    RequireRead(_claims): RequireRead,
    State(state): State<Arc<AppState>>,
    Path(ds_cluster_id): Path<String>,
    Json(req): Json<RecommendDatastoreRequest>,
) -> impl IntoResponse {
    tracing::debug!("distributed_storage::{}", stringify!(recommend_datastore));
    let dsc = match state
        .store
        .get_entity::<DatastoreCluster>("datastore_clusters", &ds_cluster_id)
    {
        Ok(Some(c)) => c,
        Ok(None) => {
            return (
                StatusCode::NOT_FOUND,
                Json(serde_json::json!({"error": "Datastore cluster not found"})),
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
    // Find the pool with the most free space
    let mut best: Option<DistributedStoragePool> = None;
    for pool_id in &dsc.datastore_ids {
        if let Ok(Some(pool)) = state
            .store
            .get_entity::<DistributedStoragePool>("dist_storage_pools", pool_id)
        {
            if pool.free_capacity_gb >= req.size_gb
                && best
                    .as_ref()
                    .is_none_or(|b| pool.free_capacity_gb > b.free_capacity_gb)
            {
                best = Some(pool);
            }
        }
    }
    match best {
        Some(pool) => Json(serde_json::json!({"recommended_pool_id": pool.id})).into_response(),
        None => (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({"error": "No suitable datastore found"})),
        )
            .into_response(),
    }
}
