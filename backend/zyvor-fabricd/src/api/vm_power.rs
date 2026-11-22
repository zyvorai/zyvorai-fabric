// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: LicenseRef-Zyvor-Production-2.0

use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::sync::Arc;

use crate::server::AppState;
use crate::validation::validate_vm_name;
use security::RequireWrite;

fn validate_image_format(format: &str) -> Result<(), (StatusCode, Json<serde_json::Value>)> {
    crate::validation::validate_image_format(format).map_err(|(s, v)| (s, Json(v)))
}

// ============================================================================
// VM Suspend-to-Disk (Hibernate) and Storage Migration
// ============================================================================

/// POST /api/vms/:name/hibernate - Suspend VM to disk
pub async fn hibernate_vm(
    RequireWrite(_claims): RequireWrite,
    State(state): State<Arc<AppState>>,
    Path(vm_name): Path<String>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    validate_vm_name(&vm_name).map_err(|(s, m)| crate::api_error::json_error(s, m))?;

    let _lock = state.vm_lock(&vm_name).lock_owned().await;

    // Check VM is running — fail explicitly if not found
    let vm = state
        .store
        .get_vm(&vm_name)
        .map_err(|e| {
            crate::api_error::json_error(StatusCode::INTERNAL_SERVER_ERROR, e.to_string())
        })?
        .ok_or_else(|| {
            crate::api_error::json_error(
                StatusCode::NOT_FOUND,
                format!("VM '{}' not found", vm_name),
            )
        })?;

    if vm.state != vm_model::VMState::Running {
        return Err(crate::api_error::json_error(
            StatusCode::CONFLICT,
            "VM must be running to hibernate",
        ));
    }

    // Use QMP to save VM state
    let qmp = state
        .driver
        .get_control_socket(&vm_name)
        .await
        .ok()
        .flatten()
        .map(|p| crate::qmp::QmpClient::for_socket(p.to_string_lossy().into_owned()));
    if let Some(qmp) = qmp {
        // Create a snapshot that includes memory state. `savevm` (the old
        // HMP command name) isn't a real top-level QMP command on current
        // QEMU -- see snapshots::live_snapshot_via_qmp's doc comment for
        // the full story; this is the same fix create_snapshot uses.
        let snap_name = format!("hibernate-{}", chrono::Utc::now().format("%Y%m%d%H%M%S"));
        if let Err(e) = crate::api::snapshots::live_snapshot_via_qmp(&qmp, &snap_name) {
            return Err(crate::api_error::json_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Failed to save VM state: {}", e),
            ));
        }

        // Stop the VM after saving state
        if let Err(e) = qmp.execute("quit", json!({})) {
            tracing::warn!("Failed to quit VM after hibernate: {}", e);
        }

        // Tell FluxVM itself the VM stopped, not just zyvor-fabricd's own
        // store below -- found live: a raw QMP `quit` kills the QEMU
        // process but FluxVM's own VmRecord.status has no way to learn
        // that on its own until its next reconcile() tick, so it kept
        // reporting Running with a now-dead PID. start_from_snapshot's
        // `if vm.status == Running { return Ok(vm) }` guard (see
        // fluxvm-scheduler::VmManager::start_impl) then short-circuited
        // on that stale status and never actually relaunched anything --
        // resume_hibernate looked like it succeeded, but only
        // zyvor-fabricd's own store had changed. The process here is
        // already dead (`quit` above), so this is just a bookkeeping sync,
        // not a real second shutdown -- `poweroff`'s underlying
        // `process_alive` check skips straight past its kill logic.
        if let Err(e) = state.driver.poweroff(&vm_name).await {
            tracing::warn!(
                "Failed to sync FluxVM's own stopped-state after hibernate: {}",
                e
            );
        }

        // Update VM state
        if let Ok(Some(mut vm)) = state.store.get_vm(&vm_name) {
            vm.state = vm_model::VMState::Stopped;
            vm.last_error = None;
            if let Err(e) = state.store.save_vm(&vm) {
                tracing::error!("Failed to save VM state: {}", e);
            }
        }

        // Store hibernate info for resume
        let info = HibernateInfo {
            vm_name: vm_name.clone(),
            snapshot_name: snap_name.clone(),
            created: chrono::Utc::now(),
        };
        if let Err(e) = state.store.save_entity("hibernate", &vm_name, &info) {
            tracing::error!("Store error: {}", e);
        }

        tracing::info!("VM '{}' hibernated with snapshot '{}'", vm_name, snap_name);
        Ok(Json(json!({"status": "hibernated", "snapshot": snap_name})))
    } else {
        // Fallback: no QMP socket available (or the fluxvm backend, which
        // has no QMP savevm equivalent) — just power off via the driver.
        state.driver.poweroff(&vm_name).await.map_err(|e| {
            crate::api_error::json_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Failed to power off VM: {e:#}"),
            )
        })?;

        if let Ok(Some(mut vm)) = state.store.get_vm(&vm_name) {
            vm.state = vm_model::VMState::Stopped;
            if let Err(e) = state.store.save_vm(&vm) {
                tracing::error!("Failed to save VM state: {}", e);
            }
        }

        Ok(Json(
            json!({"status": "stopped", "note": "QMP not available — VM stopped instead of hibernated"}),
        ))
    }
}

/// POST /api/vms/:name/resume-hibernate - Resume from hibernation
pub async fn resume_hibernate(
    RequireWrite(_claims): RequireWrite,
    State(state): State<Arc<AppState>>,
    Path(vm_name): Path<String>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    validate_vm_name(&vm_name).map_err(|(s, m)| crate::api_error::json_error(s, m))?;

    let _lock = state.vm_lock(&vm_name).lock_owned().await;

    // Check for hibernate snapshot
    let info = state
        .store
        .get_entity::<HibernateInfo>("hibernate", &vm_name)
        .map_err(|e| {
            crate::api_error::json_error(StatusCode::INTERNAL_SERVER_ERROR, e.to_string())
        })?
        .ok_or_else(|| {
            crate::api_error::json_error(StatusCode::NOT_FOUND, "No hibernation snapshot found")
        })?;

    // Validate the stored snapshot name before passing it to the driver
    if let Err((_, msg)) = crate::validation::validate_snapshot_name(&info.snapshot_name) {
        return Err(crate::api_error::json_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Corrupted hibernate data: {}", msg),
        ));
    }

    // Relaunch with the hibernate snapshot as a one-shot -loadvm override
    // (see VMDriver::start_from_snapshot) -- this restores CPU/memory/
    // device state, not just disk content. Previously this did an
    // external `qemu-img snapshot -a` to revert disk content only, then
    // an ordinary `start()` -- a cold boot that silently discarded
    // everything hibernate had just captured beyond the disk itself.
    if let Err(e) = state
        .driver
        .start_from_snapshot(&vm_name, &info.snapshot_name)
        .await
    {
        return Err(crate::api_error::json_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Failed to resume VM from hibernation snapshot: {}", e),
        ));
    }

    // Update state and clean up
    if let Ok(Some(mut vm)) = state.store.get_vm(&vm_name) {
        vm.state = vm_model::VMState::Running;
        vm.last_error = None;
        if let Err(e) = state.store.save_vm(&vm) {
            tracing::error!("Failed to save VM state: {}", e);
        }
    }
    if let Err(e) = state.store.delete_entity("hibernate", &vm_name) {
        tracing::error!("Store error: {}", e);
    }

    tracing::info!(
        "VM '{}' resumed from hibernation snapshot '{}'",
        vm_name,
        info.snapshot_name
    );
    Ok(Json(
        json!({"status": "resumed", "snapshot": info.snapshot_name}),
    ))
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct HibernateInfo {
    vm_name: String,
    snapshot_name: String,
    created: chrono::DateTime<chrono::Utc>,
}

// ============================================================================
// Storage Live Migration
// ============================================================================

#[derive(Debug, Deserialize)]
pub struct StorageMigrateRequest {
    /// Target storage pool name
    pub target_pool: String,
    /// Target format (default: same as source)
    pub target_format: Option<String>,
}

/// POST /api/vms/:name/storage/migrate - Migrate VM disk to different storage pool
pub async fn migrate_storage(
    RequireWrite(_claims): RequireWrite,
    State(state): State<Arc<AppState>>,
    Path(vm_name): Path<String>,
    Json(req): Json<StorageMigrateRequest>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    validate_vm_name(&vm_name).map_err(|(s, m)| crate::api_error::json_error(s, m))?;

    let _lock = state.vm_lock(&vm_name).lock_owned().await;

    // The VM's actual, live disk (not a naming-convention guess -- see
    // VMDriver::get_disk_path's doc comment).
    let source_path = state.driver.get_disk_path(&vm_name).await.map_err(|e| {
        crate::api_error::json_error(
            StatusCode::NOT_FOUND,
            format!("No disk image found for VM '{}': {}", vm_name, e),
        )
    })?;
    let source_path = source_path.display().to_string();

    let source_path = input_guard::vet!(
        &source_path,
        crate::api_error::json_error(StatusCode::BAD_REQUEST, "invalid source path")
    );
    let source_format = std::path::Path::new(source_path)
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("qcow2");
    let source_format = input_guard::vet_component!(
        source_format,
        crate::api_error::json_error(StatusCode::BAD_REQUEST, "invalid source format")
    );

    let target_format = req.target_format.as_deref().unwrap_or(source_format);
    let target_format = input_guard::vet_component!(
        target_format,
        crate::api_error::json_error(StatusCode::BAD_REQUEST, "invalid target format")
    );

    // Validate target format against allowlist
    validate_image_format(target_format)?;

    // Validate pool name using standard validator
    crate::validation::validate_vm_name(&req.target_pool)
        .map_err(|(s, m)| crate::api_error::json_error(s, format!("Invalid pool name: {}", m)))?;
    let pool = input_guard::vet_component!(
        &req.target_pool,
        crate::api_error::json_error(StatusCode::BAD_REQUEST, "invalid pool name")
    );
    let vm_name = input_guard::vet_component!(
        &vm_name,
        crate::api_error::json_error(StatusCode::BAD_REQUEST, "invalid VM name")
    );

    // Determine target path based on pool
    let target_dir = format!("/var/lib/zyvor-fabricd/pools/{pool}");
    tokio::fs::create_dir_all(&target_dir).await.map_err(|e| {
        crate::api_error::json_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Failed to create target dir: {}", e),
        )
    })?;

    let target_path = format!("{target_dir}/{vm_name}.{target_format}");

    tracing::info!(
        "Migrating storage for VM '{}': {} -> {}",
        vm_name,
        source_path,
        target_path
    );

    // Convert/copy to target
    let output = tokio::process::Command::new("qemu-img")
        .args([
            "convert",
            "-p",
            "-f",
            source_format,
            "-O",
            target_format,
            source_path,
            target_path.as_str(),
        ])
        .output()
        .await
        .map_err(|e| {
            crate::api_error::json_error(StatusCode::INTERNAL_SERVER_ERROR, e.to_string())
        })?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(crate::api_error::json_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Storage migration failed: {}", stderr),
        ));
    }

    // Update VM image path — fail explicitly if VM not found
    let mut vm = state
        .store
        .get_vm(vm_name)
        .map_err(|e| {
            crate::api_error::json_error(StatusCode::INTERNAL_SERVER_ERROR, e.to_string())
        })?
        .ok_or_else(|| {
            crate::api_error::json_error(
                StatusCode::NOT_FOUND,
                "VM record not found after migration — not deleting old image",
            )
        })?;

    vm.image = target_path.clone();
    vm.updated = Some(chrono::Utc::now());
    state.store.save_vm(&vm).map_err(|e| {
        crate::api_error::json_error(StatusCode::INTERNAL_SERVER_ERROR, e.to_string())
    })?;

    // Remove old image only after successful store update
    if let Err(e) = tokio::fs::remove_file(&source_path).await {
        tracing::warn!("Failed to remove old image {}: {}", source_path, e);
    }

    let size = tokio::fs::metadata(&target_path)
        .await
        .map(|m| m.len())
        .unwrap_or(0);

    tracing::info!(
        "Storage migration complete for VM '{}': {} bytes at {}",
        vm_name,
        size,
        target_path
    );

    Ok(Json(json!({
        "status": "completed",
        "source": source_path,
        "target": target_path,
        "format": target_format,
        "size_bytes": size,
    })))
}

// ============================================================================
// Affinity / Anti-Affinity Rules
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AffinityRule {
    pub id: String,
    pub name: String,
    pub rule_type: AffinityType,
    pub vm_names: Vec<String>,
    /// Labels to match VMs (alternative to explicit names)
    pub label_selector: Option<std::collections::HashMap<String, String>>,
    pub enabled: bool,
    pub created: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AffinityType {
    /// VMs should run on the same host
    Affinity,
    /// VMs must NOT run on the same host
    AntiAffinity,
    /// Soft preference — try to colocate but don't enforce
    SoftAffinity,
    /// Soft preference — try to separate but don't enforce
    SoftAntiAffinity,
}

#[derive(Debug, Deserialize)]
pub struct CreateAffinityRuleRequest {
    pub name: String,
    pub rule_type: AffinityType,
    #[serde(default)]
    pub vm_names: Vec<String>,
    pub label_selector: Option<std::collections::HashMap<String, String>>,
    #[serde(default = "crate::validation::default_true")]
    pub enabled: bool,
}

const MAX_AFFINITY_VM_NAMES: usize = 100;

/// GET /api/affinity-rules - List affinity rules
pub async fn list_affinity_rules(
    security::RequireRead(_claims): security::RequireRead,
    axum::extract::State(state): axum::extract::State<Arc<AppState>>,
) -> Result<Json<Vec<AffinityRule>>, (StatusCode, Json<serde_json::Value>)> {
    let rules = state
        .store
        .list_entities::<AffinityRule>("vm_affinity_rules")
        .map_err(|e| {
            crate::api_error::json_error(StatusCode::INTERNAL_SERVER_ERROR, e.to_string())
        })?;
    Ok(Json(rules))
}

/// POST /api/affinity-rules - Create an affinity rule
pub async fn create_affinity_rule(
    security::RequireAdmin(_claims): security::RequireAdmin,
    axum::extract::State(state): axum::extract::State<Arc<AppState>>,
    Json(req): Json<CreateAffinityRuleRequest>,
) -> Result<(StatusCode, Json<AffinityRule>), (StatusCode, Json<serde_json::Value>)> {
    // Validate name
    if req.name.is_empty() || req.name.len() > 128 {
        return Err(crate::api_error::json_error(
            StatusCode::BAD_REQUEST,
            "Rule name must be between 1 and 128 characters",
        ));
    }

    // Validate vm_names count
    if req.vm_names.len() > MAX_AFFINITY_VM_NAMES {
        return Err(crate::api_error::json_error(
            StatusCode::BAD_REQUEST,
            format!("Too many VM names (max {})", MAX_AFFINITY_VM_NAMES),
        ));
    }

    // Validate each VM name
    for name in &req.vm_names {
        validate_vm_name(name).map_err(|(s, m)| {
            crate::api_error::json_error(s, format!("Invalid VM name '{}': {}", name, m))
        })?;
    }

    let rule = AffinityRule {
        id: uuid::Uuid::new_v4().to_string(),
        name: req.name,
        rule_type: req.rule_type,
        vm_names: req.vm_names,
        label_selector: req.label_selector,
        enabled: req.enabled,
        created: chrono::Utc::now(),
    };

    state
        .store
        .save_entity("vm_affinity_rules", &rule.id, &rule)
        .map_err(|e| {
            crate::api_error::json_error(StatusCode::INTERNAL_SERVER_ERROR, e.to_string())
        })?;

    Ok((StatusCode::CREATED, Json(rule)))
}

/// DELETE /api/affinity-rules/:id - Delete an affinity rule
pub async fn delete_affinity_rule(
    security::RequireAdmin(_claims): security::RequireAdmin,
    axum::extract::State(state): axum::extract::State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<StatusCode, (StatusCode, Json<serde_json::Value>)> {
    // Check existence first
    let _rule = state
        .store
        .get_entity::<AffinityRule>("vm_affinity_rules", &id)
        .map_err(|e| {
            crate::api_error::json_error(StatusCode::INTERNAL_SERVER_ERROR, e.to_string())
        })?
        .ok_or_else(|| {
            crate::api_error::json_error(StatusCode::NOT_FOUND, "Affinity rule not found")
        })?;

    state
        .store
        .delete_entity("vm_affinity_rules", &id)
        .map_err(|e| {
            crate::api_error::json_error(StatusCode::INTERNAL_SERVER_ERROR, e.to_string())
        })?;
    Ok(StatusCode::NO_CONTENT)
}

// ============================================================================
// API Key Rate Limiting
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiKeyRateLimit {
    pub key_id: String,
    pub requests_per_minute: u32,
    pub requests_per_hour: u32,
    pub enabled: bool,
}

impl Default for ApiKeyRateLimit {
    fn default() -> Self {
        Self {
            key_id: String::new(),
            requests_per_minute: 60,
            requests_per_hour: 1000,
            enabled: true,
        }
    }
}

/// GET /api/system/rate-limits - Get rate limit configuration
pub async fn get_rate_limits(
    security::RequireRead(_claims): security::RequireRead,
    axum::extract::State(state): axum::extract::State<Arc<AppState>>,
) -> Result<Json<ApiKeyRateLimit>, (StatusCode, Json<serde_json::Value>)> {
    let config = state
        .store
        .get_entity::<ApiKeyRateLimit>("config", "rate_limits")
        .map_err(|e| {
            crate::api_error::json_error(StatusCode::INTERNAL_SERVER_ERROR, e.to_string())
        })?
        .unwrap_or_default();
    Ok(Json(config))
}

/// PUT /api/system/rate-limits - Update rate limit configuration
pub async fn update_rate_limits(
    security::RequireAdmin(_claims): security::RequireAdmin,
    axum::extract::State(state): axum::extract::State<Arc<AppState>>,
    Json(config): Json<ApiKeyRateLimit>,
) -> Result<Json<ApiKeyRateLimit>, (StatusCode, Json<serde_json::Value>)> {
    // Validate rate limit bounds
    if config.enabled {
        if config.requests_per_minute < 1 || config.requests_per_minute > 10000 {
            return Err(crate::api_error::json_error(
                StatusCode::BAD_REQUEST,
                "requests_per_minute must be between 1 and 10000",
            ));
        }
        if config.requests_per_hour < 1 || config.requests_per_hour > 100000 {
            return Err(crate::api_error::json_error(
                StatusCode::BAD_REQUEST,
                "requests_per_hour must be between 1 and 100000",
            ));
        }
        if config.requests_per_hour < config.requests_per_minute {
            return Err(crate::api_error::json_error(
                StatusCode::BAD_REQUEST,
                "requests_per_hour must be >= requests_per_minute",
            ));
        }
    }

    state
        .store
        .save_entity("config", "rate_limits", &config)
        .map_err(|e| {
            crate::api_error::json_error(StatusCode::INTERNAL_SERVER_ERROR, e.to_string())
        })?;
    Ok(Json(config))
}
