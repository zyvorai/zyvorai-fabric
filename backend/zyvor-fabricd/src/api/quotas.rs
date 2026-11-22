// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: LicenseRef-Zyvor-Production-2.0

use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::sync::Arc;
use uuid::Uuid;

use crate::api::container_declarative::ContainerGroupSpec;
use crate::server::AppState;
use security::{RequireAdmin, RequireRead};

// ============================================================================
// Data Structures
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourceQuota {
    pub id: String,
    pub name: String,
    pub max_cpus: u32,
    pub max_memory: u64, // MB
    pub max_disk: u64,   // GB
    pub max_vms: u32,
    pub used_cpus: u32,
    pub used_memory: u64,
    pub used_disk: u64,
    pub used_vms: u32,
    /// `None` means no ContainerGroup pod-count limit is configured for this
    /// quota (unlike `max_vms`, which is always enforced) — most existing
    /// quotas predate ContainerGroup support and shouldn't suddenly start
    /// blocking container workloads just because this field deserializes to
    /// a default.
    #[serde(default)]
    pub max_containers: Option<u32>,
    #[serde(default)]
    pub used_containers: u32,
    /// Optional GPU count limit for Fabric AI inference deployments
    /// (preview). `None` means no GPU quota is enforced — existing quotas
    /// predate AI and must not suddenly block inference workloads.
    #[serde(default)]
    pub max_gpus: Option<u32>,
    #[serde(default)]
    pub used_gpus: u32,
    pub tags: Option<Vec<String>>,
    /// Scopes this quota to a single tenant (matched against `VM.labels["tenant"]`
    /// or `ContainerGroupSpec.tenant`) rather than by tag overlap. Takes
    /// precedence over `tags` when set — the natural fit now that
    /// ContainerGroup has a first-class `tenant` field, and needed for a
    /// self-serve model where each tenant gets their own quota.
    #[serde(default)]
    pub tenant: Option<String>,
    pub enabled: bool,
    pub created: DateTime<Utc>,
    pub updated: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateQuotaRequest {
    pub name: String,
    pub max_cpus: u32,
    pub max_memory: u64,
    pub max_disk: u64,
    pub max_vms: u32,
    #[serde(default)]
    pub max_containers: Option<u32>,
    #[serde(default)]
    pub max_gpus: Option<u32>,
    pub tags: Option<Vec<String>>,
    #[serde(default)]
    pub tenant: Option<String>,
    #[serde(default = "crate::validation::default_true")]
    pub enabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateQuotaRequest {
    pub name: Option<String>,
    pub max_cpus: Option<u32>,
    pub max_memory: Option<u64>,
    pub max_disk: Option<u64>,
    pub max_vms: Option<u32>,
    pub max_containers: Option<u32>,
    pub max_gpus: Option<u32>,
    pub tags: Option<Vec<String>>,
    pub tenant: Option<String>,
    pub enabled: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QuotaUsage {
    pub quota_id: String,
    pub quota_name: String,
    pub cpu_percent: f64,
    pub memory_percent: f64,
    pub disk_percent: f64,
    pub vms_percent: f64,
    /// `None` when the quota has no `max_containers` configured.
    pub containers_percent: Option<f64>,
    /// `None` when the quota has no `max_gpus` configured.
    pub gpus_percent: Option<f64>,
    pub is_exceeded: bool,
    pub exceeded_resources: Vec<String>,
}

// ============================================================================
// Validation Functions
// ============================================================================

fn validate_quota(req: &CreateQuotaRequest) -> Result<(), String> {
    // Validate limits are not zero
    if req.max_cpus == 0 {
        return Err("max_cpus must be greater than 0".to_string());
    }
    if req.max_memory == 0 {
        return Err("max_memory must be greater than 0".to_string());
    }
    if req.max_disk == 0 {
        return Err("max_disk must be greater than 0".to_string());
    }
    if req.max_vms == 0 {
        return Err("max_vms must be greater than 0".to_string());
    }

    // Validate name is not empty
    if req.name.trim().is_empty() {
        return Err("Quota name cannot be empty".to_string());
    }

    if let Some(tenant) = &req.tenant {
        if !crate::api::container_declarative::is_valid_tenant_name(tenant) {
            return Err(format!(
                "invalid tenant '{tenant}': must be 1-40 characters of lowercase \
                 alphanumerics or '-', not starting or ending with '-'"
            ));
        }
    }

    Ok(())
}

// ============================================================================
// Helper Functions
// ============================================================================

impl QuotaUsage {
    fn from_quota(quota: &ResourceQuota) -> Self {
        let cpu_percent = if quota.max_cpus > 0 {
            (quota.used_cpus as f64 / quota.max_cpus as f64) * 100.0
        } else {
            0.0
        };

        let memory_percent = if quota.max_memory > 0 {
            (quota.used_memory as f64 / quota.max_memory as f64) * 100.0
        } else {
            0.0
        };

        let disk_percent = if quota.max_disk > 0 {
            (quota.used_disk as f64 / quota.max_disk as f64) * 100.0
        } else {
            0.0
        };

        let vms_percent = if quota.max_vms > 0 {
            (quota.used_vms as f64 / quota.max_vms as f64) * 100.0
        } else {
            0.0
        };

        let containers_percent = quota.max_containers.map(|max| {
            if max > 0 {
                (quota.used_containers as f64 / max as f64) * 100.0
            } else {
                0.0
            }
        });

        let gpus_percent = quota.max_gpus.map(|max| {
            if max > 0 {
                (quota.used_gpus as f64 / max as f64) * 100.0
            } else {
                0.0
            }
        });

        let mut exceeded_resources = Vec::new();
        let mut is_exceeded = false;

        if quota.used_cpus > quota.max_cpus {
            exceeded_resources.push("cpu".to_string());
            is_exceeded = true;
        }
        if quota.used_memory > quota.max_memory {
            exceeded_resources.push("memory".to_string());
            is_exceeded = true;
        }
        if quota.used_disk > quota.max_disk {
            exceeded_resources.push("disk".to_string());
            is_exceeded = true;
        }
        if quota.used_vms > quota.max_vms {
            exceeded_resources.push("vms".to_string());
            is_exceeded = true;
        }
        if let Some(max_containers) = quota.max_containers {
            if quota.used_containers > max_containers {
                exceeded_resources.push("containers".to_string());
                is_exceeded = true;
            }
        }
        if let Some(max_gpus) = quota.max_gpus {
            if quota.used_gpus > max_gpus {
                exceeded_resources.push("gpus".to_string());
                is_exceeded = true;
            }
        }

        Self {
            quota_id: quota.id.clone(),
            quota_name: quota.name.clone(),
            cpu_percent,
            memory_percent,
            disk_percent,
            vms_percent,
            containers_percent,
            gpus_percent,
            is_exceeded,
            exceeded_resources,
        }
    }
}

// ============================================================================
// Quota Handlers
// ============================================================================

pub async fn list_quotas(
    RequireRead(_claims): RequireRead,
    State(state): State<Arc<AppState>>,
) -> Result<Json<Vec<ResourceQuota>>, (StatusCode, Json<serde_json::Value>)> {
    let quotas = state
        .store
        .list_entities::<ResourceQuota>("quotas")
        .map_err(|e| {
            tracing::error!("Failed to load quotas: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"error": "Failed to load quotas"})),
            )
        })?;

    Ok(Json(quotas))
}

pub async fn get_quota(
    RequireRead(_claims): RequireRead,
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<ResourceQuota>, (StatusCode, Json<serde_json::Value>)> {
    // Load from state store
    let quota = state
        .store
        .get_entity::<ResourceQuota>("quotas", &id)
        .map_err(|_| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"error": "Failed to load quota"})),
            )
        })?
        .ok_or((
            StatusCode::NOT_FOUND,
            Json(json!({"error": "Quota not found"})),
        ))?;

    Ok(Json(quota))
}

pub async fn create_quota(
    RequireAdmin(_claims): RequireAdmin,
    State(state): State<Arc<AppState>>,
    Json(req): Json<CreateQuotaRequest>,
) -> Result<(StatusCode, Json<ResourceQuota>), (StatusCode, Json<serde_json::Value>)> {
    // Validate quota
    if let Err(err) = validate_quota(&req) {
        tracing::warn!("Invalid quota: {}", err);
        return Err((StatusCode::BAD_REQUEST, Json(json!({"error": err}))));
    }

    let now = Utc::now();
    let quota = ResourceQuota {
        id: Uuid::new_v4().to_string(),
        name: req.name,
        max_cpus: req.max_cpus,
        max_memory: req.max_memory,
        max_disk: req.max_disk,
        max_vms: req.max_vms,
        max_containers: req.max_containers,
        max_gpus: req.max_gpus,
        used_cpus: 0,
        used_memory: 0,
        used_disk: 0,
        used_vms: 0,
        used_containers: 0,
        used_gpus: 0,
        tags: req.tags,
        tenant: req.tenant,
        enabled: req.enabled,
        created: now,
        updated: now,
    };

    // Save to state store
    if let Err(e) = state.store.save_entity("quotas", &quota.id, &quota) {
        tracing::error!("Failed to save quota: {}", e);
        return Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error": "Failed to save quota"})),
        ));
    }

    Ok((StatusCode::CREATED, Json(quota)))
}

pub async fn update_quota(
    RequireAdmin(_claims): RequireAdmin,
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    Json(req): Json<UpdateQuotaRequest>,
) -> Result<Json<ResourceQuota>, (StatusCode, Json<serde_json::Value>)> {
    // Load existing quota from state store
    let mut quota = state
        .store
        .get_entity::<ResourceQuota>("quotas", &id)
        .map_err(|_| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"error": "Failed to load quota"})),
            )
        })?
        .ok_or((
            StatusCode::NOT_FOUND,
            Json(json!({"error": "Quota not found"})),
        ))?;

    // Update fields if provided
    if let Some(name) = req.name {
        if name.trim().is_empty() {
            return Err((
                StatusCode::BAD_REQUEST,
                Json(json!({"error": "Quota name cannot be empty"})),
            ));
        }
        quota.name = name;
    }
    if let Some(max_cpus) = req.max_cpus {
        if max_cpus == 0 {
            return Err((
                StatusCode::BAD_REQUEST,
                Json(json!({"error": "max_cpus must be greater than 0"})),
            ));
        }
        quota.max_cpus = max_cpus;
    }
    if let Some(max_memory) = req.max_memory {
        if max_memory == 0 {
            return Err((
                StatusCode::BAD_REQUEST,
                Json(json!({"error": "max_memory must be greater than 0"})),
            ));
        }
        quota.max_memory = max_memory;
    }
    if let Some(max_disk) = req.max_disk {
        if max_disk == 0 {
            return Err((
                StatusCode::BAD_REQUEST,
                Json(json!({"error": "max_disk must be greater than 0"})),
            ));
        }
        quota.max_disk = max_disk;
    }
    if let Some(max_vms) = req.max_vms {
        if max_vms == 0 {
            return Err((
                StatusCode::BAD_REQUEST,
                Json(json!({"error": "max_vms must be greater than 0"})),
            ));
        }
        quota.max_vms = max_vms;
    }
    if let Some(max_containers) = req.max_containers {
        quota.max_containers = Some(max_containers);
    }
    if let Some(max_gpus) = req.max_gpus {
        quota.max_gpus = Some(max_gpus);
    }
    if let Some(tags) = req.tags {
        quota.tags = Some(tags);
    }
    if let Some(tenant) = req.tenant {
        if !crate::api::container_declarative::is_valid_tenant_name(&tenant) {
            return Err((
                StatusCode::BAD_REQUEST,
                Json(json!({"error": format!("invalid tenant '{tenant}'")})),
            ));
        }
        quota.tenant = Some(tenant);
    }
    if let Some(enabled) = req.enabled {
        quota.enabled = enabled;
    }

    quota.updated = Utc::now();

    // Save to state store
    if let Err(e) = state.store.save_entity("quotas", &quota.id, &quota) {
        tracing::error!("Failed to update quota: {}", e);
        return Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error": "Failed to update quota"})),
        ));
    }

    Ok(Json(quota))
}

pub async fn delete_quota(
    RequireAdmin(_claims): RequireAdmin,
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<StatusCode, (StatusCode, Json<serde_json::Value>)> {
    // Check if quota is in use (has any usage)
    if let Ok(Some(quota)) = state.store.get_entity::<ResourceQuota>("quotas", &id) {
        if quota.used_vms > 0 {
            tracing::warn!(
                "Cannot delete quota {} - currently in use by {} VMs",
                id,
                quota.used_vms
            );
            return Err((
                StatusCode::CONFLICT,
                Json(json!({"error": format!("Quota is in use by {} VMs", quota.used_vms)})),
            ));
        }
    }

    // Remove from state store
    if let Err(e) = state.store.delete_entity("quotas", &id) {
        tracing::error!("Failed to delete quota: {}", e);
        return Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error": "Failed to delete quota"})),
        ));
    }

    Ok(StatusCode::NO_CONTENT)
}

pub async fn enable_quota(
    RequireAdmin(_claims): RequireAdmin,
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<StatusCode, (StatusCode, Json<serde_json::Value>)> {
    // Load quota from state store
    let mut quota = state
        .store
        .get_entity::<ResourceQuota>("quotas", &id)
        .map_err(|_| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"error": "Failed to load quota"})),
            )
        })?
        .ok_or((
            StatusCode::NOT_FOUND,
            Json(json!({"error": "Quota not found"})),
        ))?;

    // Set enabled = true
    quota.enabled = true;
    quota.updated = Utc::now();

    // Save to state store
    if let Err(e) = state.store.save_entity("quotas", &quota.id, &quota) {
        tracing::error!("Failed to enable quota: {}", e);
        return Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error": "Failed to enable quota"})),
        ));
    }

    Ok(StatusCode::OK)
}

pub async fn disable_quota(
    RequireAdmin(_claims): RequireAdmin,
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<StatusCode, (StatusCode, Json<serde_json::Value>)> {
    // Load quota from state store
    let mut quota = state
        .store
        .get_entity::<ResourceQuota>("quotas", &id)
        .map_err(|_| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"error": "Failed to load quota"})),
            )
        })?
        .ok_or((
            StatusCode::NOT_FOUND,
            Json(json!({"error": "Quota not found"})),
        ))?;

    // Set enabled = false
    quota.enabled = false;
    quota.updated = Utc::now();

    // Save to state store
    if let Err(e) = state.store.save_entity("quotas", &quota.id, &quota) {
        tracing::error!("Failed to disable quota: {}", e);
        return Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error": "Failed to disable quota"})),
        ));
    }

    Ok(StatusCode::OK)
}

// ============================================================================
// Usage Handlers
// ============================================================================

pub async fn get_quota_usage(
    RequireRead(_claims): RequireRead,
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<QuotaUsage>, (StatusCode, Json<serde_json::Value>)> {
    // Load quota from state store
    let mut quota = state
        .store
        .get_entity::<ResourceQuota>("quotas", &id)
        .map_err(|_| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"error": "Failed to load quota"})),
            )
        })?
        .ok_or((
            StatusCode::NOT_FOUND,
            Json(json!({"error": "Quota not found"})),
        ))?;

    // Load VMs and ContainerGroups once and calculate usage
    let vms = state.store.list_vms().map_err(|e| {
        tracing::error!("Failed to load VMs: {}", e);
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error": "Failed to load VMs"})),
        )
    })?;
    let container_groups = state
        .store
        .list_entities::<ContainerGroupSpec>("container_groups")
        .unwrap_or_default();
    let gpu_workloads = list_gpu_workloads(&state);
    calculate_quota_usage(&vms, &container_groups, &gpu_workloads, &mut quota);

    let usage = QuotaUsage::from_quota(&quota);
    Ok(Json(usage))
}

pub async fn get_all_quota_usage(
    RequireRead(_claims): RequireRead,
    State(state): State<Arc<AppState>>,
) -> Result<Json<Vec<QuotaUsage>>, (StatusCode, Json<serde_json::Value>)> {
    // Check cache first
    {
        let cache = state.quota_cache.read().await;
        if !cache.is_stale() && !cache.usage.is_empty() {
            let usage: Vec<QuotaUsage> = cache.usage.values().cloned().collect();
            return Ok(Json(usage));
        }
    }

    // Cache miss or stale - recalculate
    let mut quotas = state
        .store
        .list_entities::<ResourceQuota>("quotas")
        .unwrap_or_default();

    // Load VMs and ContainerGroups once for all quota calculations
    let vms = state.store.list_vms().unwrap_or_default();
    let container_groups = state
        .store
        .list_entities::<ContainerGroupSpec>("container_groups")
        .unwrap_or_default();
    let gpu_workloads = list_gpu_workloads(&state);
    for quota in &mut quotas {
        calculate_quota_usage(&vms, &container_groups, &gpu_workloads, quota);
    }

    let usage: Vec<QuotaUsage> = quotas.iter().map(QuotaUsage::from_quota).collect();

    // Update cache
    {
        let mut cache = state.quota_cache.write().await;
        cache.usage.clear();
        for u in &usage {
            cache.usage.insert(u.quota_id.clone(), u.clone());
        }
        cache.last_updated = std::time::Instant::now();
    }

    Ok(Json(usage))
}

// ============================================================================
// Usage Calculation Helper
// ============================================================================

/// Whether `quota` applies to a workload with the given tenant/tags. A
/// `quota.tenant` match takes precedence over tag overlap — the natural fit
/// now that ContainerGroup carries a first-class `tenant` (VM's tenant lives
/// in `labels["tenant"]`, since `vm_model::VM` has no dedicated field). A
/// quota with neither `tenant` nor `tags` set applies to every workload,
/// matching the pre-existing tag-only behavior.
fn quota_applies(quota: &ResourceQuota, tenant: Option<&str>, tags: &[String]) -> bool {
    if let Some(quota_tenant) = &quota.tenant {
        return tenant == Some(quota_tenant.as_str());
    }
    match &quota.tags {
        Some(quota_tags) => tags.iter().any(|tag| quota_tags.contains(tag)),
        None => true,
    }
}

/// Slim view of an AI inference deployment used only for GPU quota math.
/// Kept here (not imported from `api::ai`) to avoid a module cycle; serde
/// ignores unknown fields when the stored entity grows.
#[derive(Debug, Clone, Deserialize)]
struct AiDeploymentGpuUsage {
    name: String,
    #[serde(default)]
    tenant: Option<String>,
    #[serde(default)]
    replicas: u32,
    #[serde(default = "default_gpus_per_replica")]
    gpus_per_replica: u32,
}

fn default_gpus_per_replica() -> u32 {
    1
}

fn list_gpu_workloads(state: &AppState) -> Vec<AiDeploymentGpuUsage> {
    state
        .store
        .list_entities::<AiDeploymentGpuUsage>("ai_inference_deployments")
        .unwrap_or_default()
}

/// Calculate real quota usage from pre-loaded VMs, ContainerGroups, and AI
/// inference deployments. CPU and memory form one shared compute budget
/// across VM and container workloads; GPU count is tracked separately from
/// AI deployment desired replicas × `gpus_per_replica`.
fn calculate_quota_usage(
    vms: &[vm_model::VM],
    container_groups: &[ContainerGroupSpec],
    gpu_workloads: &[AiDeploymentGpuUsage],
    quota: &mut ResourceQuota,
) {
    // Reset usage counters
    quota.used_cpus = 0;
    quota.used_memory = 0;
    quota.used_disk = 0;
    quota.used_vms = 0;
    quota.used_containers = 0;
    quota.used_gpus = 0;

    for vm in vms {
        let vm_tenant = vm.labels.as_ref().and_then(|l| l.get("tenant"));
        let vm_tags = vm.tags.as_deref().unwrap_or(&[]);
        if quota_applies(quota, vm_tenant.map(String::as_str), vm_tags) {
            quota.used_cpus += vm.cpus;
            quota.used_memory += vm.memory;
            quota.used_disk += vm.disk;
            quota.used_vms += 1;
        }
    }

    for cg in container_groups {
        if quota_applies(quota, cg.tenant.as_deref(), &cg.tags) {
            let (cpus, memory_mb) = cg.total_resources();
            quota.used_cpus += cpus;
            quota.used_memory += memory_mb;
            quota.used_containers += cg.replicas;
        }
    }

    for dep in gpu_workloads {
        if quota_applies(quota, dep.tenant.as_deref(), &[]) {
            quota.used_gpus += dep.replicas.saturating_mul(dep.gpus_per_replica.max(1));
        }
    }

    tracing::debug!(
        "Calculated quota '{}' usage: {} CPUs, {} MB memory, {} GB disk, {} VMs, {} container replicas, {} GPUs",
        quota.name,
        quota.used_cpus,
        quota.used_memory,
        quota.used_disk,
        quota.used_vms,
        quota.used_containers,
        quota.used_gpus
    );
}

// ============================================================================
// Enforcement Logic
// ============================================================================

/// Check if creating/applying a workload would exceed any applicable quota.
/// Usage is recalculated live from the current VM/ContainerGroup/AI
/// deployment lists rather than trusting each quota's stored `used_*`
/// counters, which are only refreshed when someone calls the usage-report
/// endpoints — trusting them here would let usage drift stale between
/// reports and silently admit requests a fresh count would have blocked.
///
/// `exclude_container_group`, when set, drops that group from the usage
/// count before checking — required when re-applying an existing
/// ContainerGroup, since the store still holds its *previous* spec at check
/// time and `cpus`/`memory`/`containers_delta` already represent that
/// group's full new footprint, not an incremental add.
///
/// `exclude_inference_deployment` does the same for AI GPU accounting.
#[allow(clippy::too_many_arguments)]
pub async fn check_quota_enforcement(
    state: &AppState,
    cpus: u32,
    memory: u64,
    disk: u64,
    tags: &[String],
    tenant: Option<&str>,
    vms_delta: u32,
    containers_delta: u32,
    exclude_container_group: Option<&str>,
    gpus_delta: u32,
    exclude_inference_deployment: Option<&str>,
) -> Result<(), String> {
    let mut quotas: Vec<ResourceQuota> = state
        .store
        .list_entities::<ResourceQuota>("quotas")
        .unwrap_or_default();
    quotas.retain(|q| q.enabled && quota_applies(q, tenant, tags));
    if quotas.is_empty() {
        return Ok(());
    }

    let vms = state.store.list_vms().unwrap_or_default();
    let mut container_groups: Vec<ContainerGroupSpec> = state
        .store
        .list_entities::<ContainerGroupSpec>("container_groups")
        .unwrap_or_default();
    if let Some(exclude) = exclude_container_group {
        container_groups.retain(|cg| cg.name != exclude);
    }
    let mut gpu_workloads = list_gpu_workloads(state);
    if let Some(exclude) = exclude_inference_deployment {
        gpu_workloads.retain(|d| d.name != exclude);
    }

    for mut quota in quotas {
        calculate_quota_usage(&vms, &container_groups, &gpu_workloads, &mut quota);
        let mut violations = Vec::new();

        if quota.used_cpus + cpus > quota.max_cpus {
            violations.push(format!(
                "CPU quota exceeded: would use {} CPUs but limit is {} (current: {})",
                quota.used_cpus + cpus,
                quota.max_cpus,
                quota.used_cpus
            ));
        }

        if quota.used_memory + memory > quota.max_memory {
            violations.push(format!(
                "Memory quota exceeded: would use {} MB but limit is {} MB (current: {} MB)",
                quota.used_memory + memory,
                quota.max_memory,
                quota.used_memory
            ));
        }

        if quota.used_disk + disk > quota.max_disk {
            violations.push(format!(
                "Disk quota exceeded: would use {} GB but limit is {} GB (current: {} GB)",
                quota.used_disk + disk,
                quota.max_disk,
                quota.used_disk
            ));
        }

        if vms_delta > 0 && quota.used_vms + vms_delta > quota.max_vms {
            violations.push(format!(
                "VM count quota exceeded: would have {} VMs but limit is {} (current: {})",
                quota.used_vms + vms_delta,
                quota.max_vms,
                quota.used_vms
            ));
        }

        if containers_delta > 0 {
            if let Some(max_containers) = quota.max_containers {
                if quota.used_containers + containers_delta > max_containers {
                    violations.push(format!(
                        "Container quota exceeded: would have {} container replicas but limit is {} (current: {})",
                        quota.used_containers + containers_delta,
                        max_containers,
                        quota.used_containers
                    ));
                }
            }
        }

        if gpus_delta > 0 {
            if let Some(max_gpus) = quota.max_gpus {
                if quota.used_gpus + gpus_delta > max_gpus {
                    violations.push(format!(
                        "GPU quota exceeded: would use {} GPUs but limit is {} (current: {})",
                        quota.used_gpus + gpus_delta,
                        max_gpus,
                        quota.used_gpus
                    ));
                }
            }
        }

        if !violations.is_empty() {
            let error_msg = format!(
                "Quota '{}' would be exceeded:\n  - {}",
                quota.name,
                violations.join("\n  - ")
            );
            tracing::warn!("{}", error_msg);
            return Err(error_msg);
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_quota() -> ResourceQuota {
        let now = Utc::now();
        ResourceQuota {
            id: "q1".to_string(),
            name: "acme quota".to_string(),
            max_cpus: 10,
            max_memory: 10_240,
            max_disk: 500,
            max_vms: 5,
            used_cpus: 0,
            used_memory: 0,
            used_disk: 0,
            used_vms: 0,
            max_containers: None,
            used_containers: 0,
            max_gpus: None,
            used_gpus: 0,
            tags: None,
            tenant: None,
            enabled: true,
            created: now,
            updated: now,
        }
    }

    fn sample_container_group(
        name: &str,
        tenant: Option<&str>,
        replicas: u32,
    ) -> ContainerGroupSpec {
        let tenant_field = match tenant {
            Some(t) => format!(r#", "tenant": "{t}""#),
            None => String::new(),
        };
        serde_json::from_str(&format!(
            r#"{{"name": "{name}", "replicas": {replicas}, "containers": [{{"name": "app", "image": "nginx", "resources": {{"cpus": 2, "memory": "512M"}}}}]{tenant_field}}}"#
        ))
        .unwrap()
    }

    #[test]
    fn quota_applies_with_no_tenant_or_tags_matches_everything() {
        let quota = sample_quota();
        assert!(quota_applies(&quota, None, &[]));
        assert!(quota_applies(&quota, Some("acme"), &["prod".to_string()]));
    }

    #[test]
    fn quota_applies_by_tenant_ignores_tags() {
        let mut quota = sample_quota();
        quota.tenant = Some("acme".to_string());
        quota.tags = Some(vec!["prod".to_string()]);
        assert!(quota_applies(&quota, Some("acme"), &[]));
        assert!(!quota_applies(
            &quota,
            Some("other-tenant"),
            &["prod".to_string()]
        ));
        assert!(!quota_applies(&quota, None, &["prod".to_string()]));
    }

    #[test]
    fn quota_applies_by_tag_overlap_when_no_tenant_set() {
        let mut quota = sample_quota();
        quota.tags = Some(vec!["prod".to_string()]);
        assert!(quota_applies(
            &quota,
            None,
            &["prod".to_string(), "web".to_string()]
        ));
        assert!(!quota_applies(&quota, None, &["staging".to_string()]));
    }

    #[test]
    fn calculate_quota_usage_counts_container_group_resources_and_replicas() {
        let mut quota = sample_quota();
        quota.tenant = Some("acme".to_string());
        let groups = vec![
            sample_container_group("web", Some("acme"), 3),
            sample_container_group("other", Some("other-tenant"), 5),
        ];
        calculate_quota_usage(&[], &groups, &[], &mut quota);
        // "web": 2 cpus * 3 replicas = 6 cpus, 512 MB * 3 = 1536 MB
        assert_eq!(quota.used_cpus, 6);
        assert_eq!(quota.used_memory, 1536);
        assert_eq!(quota.used_containers, 3);
        assert_eq!(quota.used_vms, 0);
    }

    #[test]
    fn calculate_quota_usage_combines_vm_and_container_group_cpu_and_memory() {
        let mut quota = sample_quota();
        let mut vm = vm_model::VM::new("vm-1".to_string(), "img".to_string(), 4, 4096);
        vm.labels = Some(std::collections::HashMap::from([(
            "tenant".to_string(),
            "acme".to_string(),
        )]));
        quota.tenant = Some("acme".to_string());
        let groups = vec![sample_container_group("web", Some("acme"), 1)];
        calculate_quota_usage(&[vm], &groups, &[], &mut quota);
        assert_eq!(quota.used_cpus, 4 + 2);
        assert_eq!(quota.used_memory, 4096 + 512);
        assert_eq!(quota.used_vms, 1);
        assert_eq!(quota.used_containers, 1);
    }

    #[test]
    fn quota_usage_containers_percent_is_none_without_max_containers() {
        let quota = sample_quota();
        let usage = QuotaUsage::from_quota(&quota);
        assert!(usage.containers_percent.is_none());
    }

    #[test]
    fn quota_usage_flags_exceeded_containers() {
        let mut quota = sample_quota();
        quota.max_containers = Some(2);
        quota.used_containers = 3;
        let usage = QuotaUsage::from_quota(&quota);
        assert_eq!(usage.containers_percent, Some(150.0));
        assert!(usage.is_exceeded);
        assert!(usage.exceeded_resources.contains(&"containers".to_string()));
    }

    #[test]
    fn calculate_quota_usage_is_zero_once_the_only_group_is_excluded() {
        // `check_quota_enforcement` re-applying an existing ContainerGroup
        // filters that group's own (stale, pre-apply) spec out of the
        // container_groups list before calling `calculate_quota_usage` --
        // otherwise a no-op re-apply would double-count the group's own
        // prior contribution against itself and get rejected. This checks
        // the usage math that filtering relies on.
        let mut quota = ResourceQuota {
            max_cpus: 2,
            max_containers: Some(1),
            tenant: Some("acme".to_string()),
            ..sample_quota()
        };
        let all_groups = vec![sample_container_group("web", Some("acme"), 1)];
        let excluding_web: Vec<ContainerGroupSpec> = all_groups
            .iter()
            .filter(|cg| cg.name != "web")
            .cloned()
            .collect();

        calculate_quota_usage(&[], &all_groups, &[], &mut quota);
        assert_eq!(quota.used_cpus, 2);
        assert_eq!(quota.used_containers, 1);

        calculate_quota_usage(&[], &excluding_web, &[], &mut quota);
        assert_eq!(quota.used_cpus, 0);
        assert_eq!(quota.used_containers, 0);
    }

    #[test]
    fn calculate_quota_usage_counts_ai_deployment_gpus() {
        let mut quota = sample_quota();
        quota.max_gpus = Some(4);
        quota.tenant = Some("acme".to_string());
        let workloads = vec![
            AiDeploymentGpuUsage {
                name: "llm".into(),
                tenant: Some("acme".into()),
                replicas: 2,
                gpus_per_replica: 1,
            },
            AiDeploymentGpuUsage {
                name: "other".into(),
                tenant: Some("other".into()),
                replicas: 8,
                gpus_per_replica: 1,
            },
        ];
        calculate_quota_usage(&[], &[], &workloads, &mut quota);
        assert_eq!(quota.used_gpus, 2);
    }

    #[test]
    fn quota_usage_flags_exceeded_gpus() {
        let mut quota = sample_quota();
        quota.max_gpus = Some(2);
        quota.used_gpus = 3;
        let usage = QuotaUsage::from_quota(&quota);
        assert_eq!(usage.gpus_percent, Some(150.0));
        assert!(usage.is_exceeded);
        assert!(usage.exceeded_resources.contains(&"gpus".to_string()));
    }

    #[test]
    fn max_gpus_none_means_no_gpu_percent() {
        let quota = sample_quota();
        let usage = QuotaUsage::from_quota(&quota);
        assert!(usage.gpus_percent.is_none());
    }
}
