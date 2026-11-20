// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: LicenseRef-Zyvor-Production-1.0

use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    Json,
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::sync::Arc;

use crate::server::AppState;
use crate::validation::validate_vm_name;
use security::{RequireAdmin, RequireRead};

// ============================================================================
// Data Structures
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MigrationType {
    Live,
    Offline,
    Storage,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum MigrationState {
    Pending,
    PreCheck,
    Syncing,
    Switching,
    Completed,
    Failed,
    Cancelled,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MigrationRequest {
    pub vm_name: String,
    pub target_host: String,
    pub migration_type: MigrationType,
    pub compress: Option<bool>,
    pub bandwidth_mbps: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MigrationStatus {
    pub id: String,
    pub vm_name: String,
    pub target_host: String,
    pub migration_type: MigrationType,
    pub state: MigrationState,
    pub progress_percent: u32,
    pub bytes_transferred: u64,
    pub started: DateTime<Utc>,
    pub completed: Option<DateTime<Utc>>,
    pub error: Option<String>,
}

// ============================================================================
// Handlers
// ============================================================================

/// POST /api/migrations - Start a new migration (Admin only)
pub async fn start_migration(
    RequireAdmin(_claims): RequireAdmin,
    State(state): State<Arc<AppState>>,
    Json(req): Json<MigrationRequest>,
) -> Result<(StatusCode, Json<MigrationStatus>), (StatusCode, Json<serde_json::Value>)> {
    tracing::debug!("migration::{}", stringify!(start_migration));
    // Validate VM name
    validate_vm_name(&req.vm_name)
        .map_err(|(_status, msg)| (StatusCode::BAD_REQUEST, Json(json!({ "error": msg }))))?;

    // Validate target host (must be a hostname or IP, no shell metacharacters)
    crate::validation::validate_hostname(&req.target_host)
        .map_err(|msg| (StatusCode::BAD_REQUEST, Json(json!({ "error": msg }))))?;

    // Verify VM exists
    match state.store.get_vm(&req.vm_name) {
        Ok(Some(_)) => {}
        Ok(None) => {
            return Err((
                StatusCode::NOT_FOUND,
                Json(json!({ "error": format!("VM '{}' not found", req.vm_name) })),
            ));
        }
        Err(e) => {
            return Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": e.to_string() })),
            ));
        }
    }

    let migration_id = uuid::Uuid::new_v4().to_string();
    let now = Utc::now();

    let status = MigrationStatus {
        id: migration_id.clone(),
        vm_name: req.vm_name.clone(),
        target_host: req.target_host.clone(),
        migration_type: req.migration_type.clone(),
        state: MigrationState::Pending,
        progress_percent: 0,
        bytes_transferred: 0,
        started: now,
        completed: None,
        error: None,
    };

    // Save initial status
    state
        .store
        .save_entity("migrations", &migration_id, &status)
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": e.to_string() })),
            )
        })?;

    // Spawn background migration task
    let state_clone = state.clone();
    let migration_id_clone = migration_id.clone();
    let req_clone = req.clone();

    let handle = tokio::spawn(async move {
        run_migration(state_clone, migration_id_clone, req_clone).await;
    });

    // Monitor the spawned task for panics
    let migration_id_monitor = migration_id.clone();
    let state_monitor = state.clone();
    tokio::spawn(async move {
        if let Err(e) = handle.await {
            tracing::error!("Migration task '{}' panicked: {}", migration_id_monitor, e);
            // Update migration status to Failed on panic
            if let Ok(Some(mut status)) = state_monitor
                .store
                .get_entity::<MigrationStatus>("migrations", &migration_id_monitor)
            {
                status.state = MigrationState::Failed;
                status.error = Some("Internal error: migration task panicked".to_string());
                status.completed = Some(Utc::now());
                if let Err(save_err) =
                    state_monitor
                        .store
                        .save_entity("migrations", &migration_id_monitor, &status)
                {
                    tracing::error!("Failed to save migration panic status: {}", save_err);
                }
            }
        }
    });

    Ok((StatusCode::ACCEPTED, Json(status)))
}

/// GET /api/migrations - List all migrations
pub async fn list_migrations(
    RequireRead(_claims): RequireRead,
    State(state): State<Arc<AppState>>,
) -> Result<Json<Vec<MigrationStatus>>, (StatusCode, Json<serde_json::Value>)> {
    tracing::debug!("migration::{}", stringify!(list_migrations));
    let migrations = state
        .store
        .list_entities::<MigrationStatus>("migrations")
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": e.to_string() })),
            )
        })?;

    Ok(Json(migrations))
}

/// GET /api/migrations/:id - Get migration status
pub async fn get_migration(
    RequireRead(_claims): RequireRead,
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<MigrationStatus>, (StatusCode, Json<serde_json::Value>)> {
    tracing::debug!("migration::{}", stringify!(get_migration));
    match state.store.get_entity::<MigrationStatus>("migrations", &id) {
        Ok(Some(status)) => Ok(Json(status)),
        Ok(None) => Err((
            StatusCode::NOT_FOUND,
            Json(json!({ "error": "Migration not found" })),
        )),
        Err(e) => Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": e.to_string() })),
        )),
    }
}

/// POST /api/migrations/:id/cancel - Cancel a migration (Admin only)
pub async fn cancel_migration(
    RequireAdmin(_claims): RequireAdmin,
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<MigrationStatus>, (StatusCode, Json<serde_json::Value>)> {
    tracing::debug!("migration::{}", stringify!(cancel_migration));
    let mut status = match state.store.get_entity::<MigrationStatus>("migrations", &id) {
        Ok(Some(s)) => s,
        Ok(None) => {
            return Err((
                StatusCode::NOT_FOUND,
                Json(json!({ "error": "Migration not found" })),
            ));
        }
        Err(e) => {
            return Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": e.to_string() })),
            ));
        }
    };

    // Can only cancel pending/in-progress migrations
    match status.state {
        MigrationState::Completed | MigrationState::Failed | MigrationState::Cancelled => {
            return Err((
                StatusCode::BAD_REQUEST,
                Json(json!({ "error": "Migration is already finished" })),
            ));
        }
        _ => {}
    }

    // Kill any rsync processes for this VM.
    // Validate the VM name to prevent regex injection in pkill's -f pattern.
    if crate::validation::validate_vm_name(&status.vm_name).is_err() {
        tracing::error!(
            "Migration cancel: invalid VM name '{}', skipping pkill",
            status.vm_name
        );
    } else {
        // VM name is validated to only contain [a-zA-Z0-9._-].
        // Escape dots for regex safety since '.' matches any character.
        let escaped_name = status.vm_name.replace('.', "\\.");
        if let Err(e) = tokio::process::Command::new("pkill")
            .args(["-f", &format!("rsync.*{}", escaped_name)])
            .output()
            .await
        {
            tracing::warn!("Command failed: {}", e);
        }
    }

    status.state = MigrationState::Cancelled;
    status.completed = Some(Utc::now());

    state
        .store
        .save_entity("migrations", &id, &status)
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": e.to_string() })),
            )
        })?;

    Ok(Json(status))
}

// ============================================================================
// Background Migration Task
// ============================================================================

async fn run_migration(state: Arc<AppState>, migration_id: String, req: MigrationRequest) {
    let update_status = |state: &Arc<AppState>,
                         id: &str,
                         mig_state: MigrationState,
                         progress: u32,
                         bytes: u64,
                         error: Option<String>| {
        if let Ok(Some(mut status)) = state.store.get_entity::<MigrationStatus>("migrations", id) {
            status.state = mig_state;
            status.progress_percent = progress;
            status.bytes_transferred = bytes;
            status.error = error;
            if status.progress_percent >= 100 {
                status.completed = Some(Utc::now());
            }
            if let Err(e) = state.store.save_entity("migrations", id, &status) {
                tracing::error!("Failed to save: {}", e);
            }
        }
    };

    // Pre-check: verify target host reachable
    update_status(&state, &migration_id, MigrationState::PreCheck, 5, 0, None);

    let ssh_check = tokio::process::Command::new("ssh")
        .args([
            "-o",
            "ConnectTimeout=5",
            "-o",
            "BatchMode=yes",
            &req.target_host,
            "echo ok",
        ])
        .output()
        .await;

    match ssh_check {
        Ok(output) if output.status.success() => {
            tracing::info!(
                "Migration {}: target host {} is reachable",
                migration_id,
                req.target_host
            );
        }
        _ => {
            update_status(
                &state,
                &migration_id,
                MigrationState::Failed,
                5,
                0,
                Some(format!(
                    "Target host '{}' is not reachable via SSH",
                    req.target_host
                )),
            );
            return;
        }
    }

    // Create migration config for the migration crate
    let config = migration::MigrationConfig {
        vm_name: req.vm_name.clone(),
        source_node: "localhost".to_string(),
        target_node: req.target_host.clone(),
        live: matches!(req.migration_type, MigrationType::Live),
        compress: req.compress.unwrap_or(true),
        bandwidth_mbps: req.bandwidth_mbps,
    };

    let workspace = "/var/lib/zyvor-fabricd/migrations";
    let manager = match migration::MigrationManager::new(workspace, state.driver.clone()) {
        Ok(m) => m,
        Err(e) => {
            update_status(
                &state,
                &migration_id,
                MigrationState::Failed,
                5,
                0,
                Some(format!("Failed to initialize migration: {}", e)),
            );
            return;
        }
    };

    update_status(&state, &migration_id, MigrationState::Syncing, 20, 0, None);

    match manager.migrate_vm(&config).await {
        Ok(result) => {
            let final_state = match result.status {
                migration::MigrationState::Completed => MigrationState::Completed,
                migration::MigrationState::Failed => MigrationState::Failed,
                _ => MigrationState::Completed,
            };
            update_status(
                &state,
                &migration_id,
                final_state.clone(),
                100,
                0,
                result.error,
            );
            if matches!(final_state, MigrationState::Completed) {
                crate::api::events::record_event(
                    &state,
                    crate::api::events::VMEventType::Migrated,
                    &req.vm_name,
                    Some(format!("Migrated to {}", req.target_host)),
                );
            } else {
                crate::api::events::record_event(
                    &state,
                    crate::api::events::VMEventType::Error,
                    &req.vm_name,
                    Some("Migration completed with failed status".to_string()),
                );
            }
            tracing::info!(
                "Migration {} completed for VM '{}'",
                migration_id,
                req.vm_name
            );
        }
        Err(e) => {
            update_status(
                &state,
                &migration_id,
                MigrationState::Failed,
                0,
                0,
                Some(e.to_string()),
            );
            crate::api::events::record_event(
                &state,
                crate::api::events::VMEventType::Error,
                &req.vm_name,
                Some(format!("Migration failed: {}", e)),
            );
            tracing::error!(
                "Migration {} failed for VM '{}': {}",
                migration_id,
                req.vm_name,
                e
            );
        }
    }
}

// ============================================================================
// Native FluxVM runtime-boundary migration (source-side VMM transport)
// ============================================================================

fn runtime_mgr(
    state: &AppState,
) -> Result<migration::RuntimeMigrationManager, (StatusCode, Json<serde_json::Value>)> {
    migration::RuntimeMigrationManager::new(
        &state.config.driver.fluxvm_url,
        state.config.driver.fluxvm_token.as_deref(),
    )
    .map_err(|e| {
        (
            StatusCode::BAD_GATEWAY,
            Json(json!({ "error": format!("FluxVM runtime client: {e}") })),
        )
    })
}

/// GET /api/runtime/capabilities — FluxVM node-local migration/snapshot contract.
pub async fn runtime_capabilities(
    RequireRead(_claims): RequireRead,
    State(state): State<Arc<AppState>>,
) -> Result<
    Json<zyvor_fabric_fluxvm_client::RuntimeCapabilities>,
    (StatusCode, Json<serde_json::Value>),
> {
    let mgr = runtime_mgr(&state)?;
    let caps = mgr.capabilities().await.map_err(|e| {
        (
            StatusCode::BAD_GATEWAY,
            Json(json!({ "error": e.to_string() })),
        )
    })?;
    Ok(Json(caps))
}

#[derive(Debug, Deserialize)]
pub struct NativeMigrationStartRequest {
    /// QEMU migration URI, e.g. `tcp:10.0.0.2:4444` or `unix:/run/mig.sock`.
    pub target_uri: String,
    #[serde(default)]
    pub mode: Option<String>,
    #[serde(default)]
    pub bandwidth_mbps: Option<u64>,
    #[serde(default)]
    pub max_downtime_ms: Option<u64>,
    #[serde(default)]
    pub multifd_channels: Option<u8>,
    /// Required when FluxVM advertises `requiresSharedStorage`.
    #[serde(default)]
    pub shared_storage_confirmed: bool,
    #[serde(default)]
    pub transfer_network_state: bool,
}

#[derive(Debug, Serialize)]
pub struct NativeMigrationStatusResponse {
    pub vm_name: String,
    pub phase: String,
    pub status: String,
    pub progress_percent: u8,
    pub ram_transferred: Option<u64>,
    pub ram_remaining: Option<u64>,
    pub ram_total: Option<u64>,
    pub total_time_ms: Option<u64>,
    pub downtime_ms: Option<u64>,
    pub error: Option<String>,
}

fn native_status_response(
    vm_name: &str,
    status: zyvor_fabric_fluxvm_client::MigrationStatus,
) -> NativeMigrationStatusResponse {
    use zyvor_fabric_fluxvm_client::MigrationPhase;
    let progress = migration::progress_percent(&status);
    let phase = match status.phase {
        MigrationPhase::None => "none",
        MigrationPhase::Setup => "setup",
        MigrationPhase::Active => "active",
        MigrationPhase::PostcopyActive => "postcopy-active",
        MigrationPhase::Completed => "completed",
        MigrationPhase::Failed => "failed",
        MigrationPhase::Cancelled => "cancelled",
        MigrationPhase::Unknown => "unknown",
    };
    NativeMigrationStatusResponse {
        vm_name: vm_name.to_string(),
        phase: phase.to_string(),
        status: status.status,
        progress_percent: progress,
        ram_transferred: status.ram_transferred,
        ram_remaining: status.ram_remaining,
        ram_total: status.ram_total,
        total_time_ms: status.total_time_ms,
        downtime_ms: status.downtime_ms,
        error: status.error,
    }
}

fn parse_mode(
    raw: Option<&str>,
) -> Result<zyvor_fabric_fluxvm_client::MigrationMode, (StatusCode, Json<serde_json::Value>)> {
    match raw.unwrap_or("pre-copy").to_ascii_lowercase().as_str() {
        "pre-copy" | "precopy" | "pre_copy" => {
            Ok(zyvor_fabric_fluxvm_client::MigrationMode::PreCopy)
        }
        "post-copy" | "postcopy" | "post_copy" => {
            Ok(zyvor_fabric_fluxvm_client::MigrationMode::PostCopy)
        }
        other => Err((
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": format!("unsupported migration mode '{other}'") })),
        )),
    }
}

fn resolve_target_fluxvm_url(
    state: &AppState,
    target_node: Option<&str>,
) -> Result<String, (StatusCode, Json<serde_json::Value>)> {
    match target_node {
        None | Some("") => Ok(state.config.driver.fluxvm_url.clone()),
        Some(name) => state
            .config
            .driver
            .fluxvm_nodes
            .iter()
            .find(|n| n.name == name)
            .map(|n| n.url.clone())
            .ok_or_else(|| {
                (
                    StatusCode::BAD_REQUEST,
                    Json(json!({
                        "error": format!(
                            "unknown target FluxVM node '{name}' (configure driver.fluxvm_nodes)"
                        )
                    })),
                )
            }),
    }
}

fn runtime_mgr_with_target(
    state: &AppState,
    target_node: Option<&str>,
) -> Result<migration::RuntimeMigrationManager, (StatusCode, Json<serde_json::Value>)> {
    let target_url = resolve_target_fluxvm_url(state, target_node)?;
    migration::RuntimeMigrationManager::with_target(
        &state.config.driver.fluxvm_url,
        &target_url,
        state.config.driver.fluxvm_token.as_deref(),
    )
    .map_err(|e| {
        (
            StatusCode::BAD_GATEWAY,
            Json(json!({ "error": format!("FluxVM runtime client: {e}") })),
        )
    })
}

#[derive(Debug, Deserialize)]
pub struct PrepareReceiverRequest {
    pub disk_path: String,
    /// Hostname/IP embedded in `tcp:<host>:<port>` returned to the caller.
    pub listen_host: String,
    #[serde(default)]
    pub receiver_ttl_seconds: Option<u64>,
    /// Named entry from `driver.fluxvm_nodes`; default = primary `fluxvm_url`.
    #[serde(default)]
    pub target_node: Option<String>,
}

/// POST /api/vms/{name}/migration/native/prepare-receiver
pub async fn prepare_native_receiver(
    RequireAdmin(_claims): RequireAdmin,
    State(state): State<Arc<AppState>>,
    Path(name): Path<String>,
    Json(req): Json<PrepareReceiverRequest>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    validate_vm_name(&name)
        .map_err(|(_status, msg)| (StatusCode::BAD_REQUEST, Json(json!({ "error": msg }))))?;
    if req.disk_path.trim().is_empty() || req.listen_host.trim().is_empty() {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": "disk_path and listen_host are required" })),
        ));
    }
    let mgr = runtime_mgr_with_target(&state, req.target_node.as_deref())?;
    let prepared = mgr
        .prepare_receiver(
            &name,
            req.listen_host.trim(),
            &migration::PrepareReceiverOptions {
                disk_path: std::path::PathBuf::from(req.disk_path.trim()),
                receiver_ttl_seconds: req.receiver_ttl_seconds,
                spec: None,
            },
        )
        .await
        .map_err(|e| {
            (
                StatusCode::BAD_GATEWAY,
                Json(json!({ "error": e.to_string() })),
            )
        })?;
    Ok(Json(json!({
        "receiver_id": prepared.receiver.id,
        "port": prepared.receiver.port,
        "status": prepared.receiver.status,
        "expires_at": prepared.receiver.expires_at,
        "target_uri": prepared.target_uri,
    })))
}

/// POST /api/migration/receivers/{id}/activate
pub async fn activate_native_receiver(
    RequireAdmin(_claims): RequireAdmin,
    State(state): State<Arc<AppState>>,
    Path(id): Path<uuid::Uuid>,
    Json(req): Json<serde_json::Value>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let target_node = req
        .get("target_node")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());
    let mgr = runtime_mgr_with_target(&state, target_node.as_deref())?;
    let record = mgr.activate_receiver(id).await.map_err(|e| {
        (
            StatusCode::BAD_GATEWAY,
            Json(json!({ "error": e.to_string() })),
        )
    })?;
    Ok(Json(json!({
        "id": record.id,
        "name": record.name,
        "status": record.status,
    })))
}

/// DELETE /api/migration/receivers/{id}
pub async fn abort_native_receiver(
    RequireAdmin(_claims): RequireAdmin,
    State(state): State<Arc<AppState>>,
    Path(id): Path<uuid::Uuid>,
    Query(q): Query<std::collections::HashMap<String, String>>,
) -> Result<StatusCode, (StatusCode, Json<serde_json::Value>)> {
    let target_node = q.get("target_node").map(|s| s.as_str());
    let mgr = runtime_mgr_with_target(&state, target_node)?;
    mgr.abort_receiver(id).await.map_err(|e| {
        (
            StatusCode::BAD_GATEWAY,
            Json(json!({ "error": e.to_string() })),
        )
    })?;
    Ok(StatusCode::NO_CONTENT)
}

/// GET /api/vms/{name}/migration/native/network-state
pub async fn native_network_migration_state(
    RequireRead(_claims): RequireRead,
    State(state): State<Arc<AppState>>,
    Path(name): Path<String>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    validate_vm_name(&name)
        .map_err(|(_status, msg)| (StatusCode::BAD_REQUEST, Json(json!({ "error": msg }))))?;
    let mgr = runtime_mgr(&state)?;
    let status = mgr.network_migration_state(&name).await.map_err(|e| {
        (
            StatusCode::BAD_GATEWAY,
            Json(json!({ "error": e.to_string() })),
        )
    })?;
    Ok(Json(serde_json::to_value(status).unwrap_or(json!({}))))
}

/// POST /api/vms/{name}/migration/native/start
pub async fn start_native_migration(
    RequireAdmin(_claims): RequireAdmin,
    State(state): State<Arc<AppState>>,
    Path(name): Path<String>,
    Json(req): Json<NativeMigrationStartRequest>,
) -> Result<Json<NativeMigrationStatusResponse>, (StatusCode, Json<serde_json::Value>)> {
    validate_vm_name(&name)
        .map_err(|(_status, msg)| (StatusCode::BAD_REQUEST, Json(json!({ "error": msg }))))?;
    if req.target_uri.trim().is_empty() {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": "target_uri is required (e.g. tcp:host:port)" })),
        ));
    }
    let options = migration::NativeMigrationOptions {
        mode: parse_mode(req.mode.as_deref())?,
        bandwidth_mbps: req.bandwidth_mbps,
        max_downtime_ms: req.max_downtime_ms.or(Some(300)),
        multifd_channels: req.multifd_channels.or(Some(4)),
        shared_storage_confirmed: req.shared_storage_confirmed,
        transfer_network_state: req.transfer_network_state,
    };
    let mgr = runtime_mgr(&state)?;
    if options.transfer_network_state {
        let _ = mgr.network_migration_quiesce(&name).await.map_err(|e| {
            (
                StatusCode::BAD_GATEWAY,
                Json(json!({ "error": e.to_string() })),
            )
        })?;
    }
    let status = mgr
        .start_prepared_target(&name, req.target_uri.trim(), &options)
        .await
        .map_err(|e| {
            (
                StatusCode::BAD_REQUEST,
                Json(json!({ "error": e.to_string() })),
            )
        })?;
    tracing::info!(
        vm = %name,
        uri = %req.target_uri,
        "Started native FluxVM migration transport"
    );
    Ok(Json(native_status_response(&name, status)))
}

/// GET /api/vms/{name}/migration/native/status
pub async fn native_migration_status(
    RequireRead(_claims): RequireRead,
    State(state): State<Arc<AppState>>,
    Path(name): Path<String>,
) -> Result<Json<NativeMigrationStatusResponse>, (StatusCode, Json<serde_json::Value>)> {
    validate_vm_name(&name)
        .map_err(|(_status, msg)| (StatusCode::BAD_REQUEST, Json(json!({ "error": msg }))))?;
    let mgr = runtime_mgr(&state)?;
    let status = mgr.status(&name).await.map_err(|e| {
        (
            StatusCode::BAD_GATEWAY,
            Json(json!({ "error": e.to_string() })),
        )
    })?;
    Ok(Json(native_status_response(&name, status)))
}

/// POST /api/vms/{name}/migration/native/cancel
pub async fn cancel_native_migration(
    RequireAdmin(_claims): RequireAdmin,
    State(state): State<Arc<AppState>>,
    Path(name): Path<String>,
) -> Result<Json<NativeMigrationStatusResponse>, (StatusCode, Json<serde_json::Value>)> {
    validate_vm_name(&name)
        .map_err(|(_status, msg)| (StatusCode::BAD_REQUEST, Json(json!({ "error": msg }))))?;
    let mgr = runtime_mgr(&state)?;
    let status = mgr.cancel(&name).await.map_err(|e| {
        (
            StatusCode::BAD_GATEWAY,
            Json(json!({ "error": e.to_string() })),
        )
    })?;
    Ok(Json(native_status_response(&name, status)))
}
