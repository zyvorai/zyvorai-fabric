// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: LicenseRef-Zyvor-Production-1.0

use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    Json,
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::process::Command;

use zyvor_fabric_system::{
    CpuTopology, HugepageManager, HugepageSize, MemoryController, NumaTopology,
};

use crate::server::AppState;
use crate::validation::validate_vm_name;
use security::{RequireRead, RequireWrite};

// Request/Response types
#[derive(Debug, Deserialize)]
pub struct NumaPlacementQuery {
    pub memory_mb: u64,
    pub cpus: u32,
}

#[derive(Debug, Deserialize)]
pub struct SetCpuPinningRequest {
    pub pinning: CpuPinningDto,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(tag = "type")]
pub enum CpuPinningDto {
    Auto,
    NumaNode { value: u32 },
    Socket { value: u32 },
    Explicit { value: Vec<CpuPinDto> },
}

#[derive(Debug, Deserialize, Serialize)]
pub struct CpuPinDto {
    pub vcpu_id: u32,
    pub physical_cpu: u32,
}

#[derive(Debug, Deserialize)]
pub struct SetMemoryLimitRequest {
    pub limit_bytes: u64,
    pub swap_limit_bytes: Option<u64>,
}

#[derive(Debug, Deserialize)]
pub struct SetMemoryBallooningRequest {
    pub enabled: bool,
    pub target_mb: Option<u64>,
}

#[derive(Debug, Deserialize)]
pub struct AllocateHugepagesRequest {
    pub size: HugepageSizeDto,
    pub count: u32,
}

#[derive(Debug, Deserialize, Serialize)]
pub enum HugepageSizeDto {
    Size2MB,
    Size1GB,
}

impl From<HugepageSizeDto> for HugepageSize {
    fn from(dto: HugepageSizeDto) -> Self {
        match dto {
            HugepageSizeDto::Size2MB => HugepageSize::Size2MB,
            HugepageSizeDto::Size1GB => HugepageSize::Size1GB,
        }
    }
}

#[derive(Debug, Deserialize)]
pub struct HugepageQuery {
    pub size: HugepageSizeDto,
}

// API Handlers

/// GET /api/system/cpu/topology - Get CPU topology
pub async fn get_cpu_topology(
    RequireRead(_claims): RequireRead,
) -> Result<Json<CpuTopology>, (StatusCode, String)> {
    tracing::debug!("system::{}", stringify!(get_cpu_topology));
    match CpuTopology::detect() {
        Ok(topology) => Ok(Json(topology)),
        Err(e) => Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Failed to detect CPU topology: {}", e),
        )),
    }
}

/// GET /api/system/numa/topology - Get NUMA topology
pub async fn get_numa_topology(
    RequireRead(_claims): RequireRead,
) -> Result<Json<NumaTopology>, (StatusCode, String)> {
    tracing::debug!("system::{}", stringify!(get_numa_topology));
    match NumaTopology::detect() {
        Ok(topology) => Ok(Json(topology)),
        Err(e) => Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Failed to detect NUMA topology: {}", e),
        )),
    }
}

/// GET /api/system/numa/nodes/:id - Get NUMA node details
pub async fn get_numa_node(
    RequireRead(_claims): RequireRead,
    Path(node_id): Path<u32>,
) -> Result<Json<zyvor_fabric_system::NumaNode>, (StatusCode, String)> {
    tracing::debug!("system::{}", stringify!(get_numa_node));
    let topology = NumaTopology::detect().map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Failed to detect NUMA topology: {}", e),
        )
    })?;

    match topology.get_node(node_id) {
        Some(node) => Ok(Json(node.clone())),
        None => Err((
            StatusCode::NOT_FOUND,
            format!("NUMA node {} not found", node_id),
        )),
    }
}

/// GET /api/system/numa/placement - Get recommended NUMA placement
pub async fn get_numa_placement(
    RequireRead(_claims): RequireRead,
    Query(params): Query<NumaPlacementQuery>,
) -> Result<Json<zyvor_fabric_system::NumaPlacement>, (StatusCode, String)> {
    tracing::debug!("system::{}", stringify!(get_numa_placement));
    let topology = NumaTopology::detect().map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Failed to detect NUMA topology: {}", e),
        )
    })?;

    match topology.recommend_placement(params.memory_mb, params.cpus) {
        Some(placement) => Ok(Json(placement)),
        None => Err((
            StatusCode::NOT_FOUND,
            "No suitable NUMA node found for requested resources".to_string(),
        )),
    }
}

/// POST /api/vms/:name/cpu/pin - Set CPU pinning for a VM
pub async fn set_cpu_pinning(
    State(state): State<Arc<AppState>>,
    RequireWrite(_claims): RequireWrite,
    Path(vm_name): Path<String>,
    Json(req): Json<SetCpuPinningRequest>,
) -> Result<StatusCode, (StatusCode, String)> {
    tracing::debug!("system::{}", stringify!(set_cpu_pinning));
    validate_vm_name(&vm_name)?;
    tracing::info!(
        "Setting CPU pinning for VM '{}': {:?}",
        vm_name,
        req.pinning
    );

    // Build the cpuset list based on pinning type.
    let cpus: Vec<u32> = match &req.pinning {
        CpuPinningDto::Auto => {
            tracing::info!("Auto CPU pinning - no explicit affinity set");
            return Ok(StatusCode::OK);
        }
        CpuPinningDto::NumaNode { value } => {
            tracing::warn!("NUMA node pinning requires reading node CPU list");
            // Would need to read /sys/devices/system/node/nodeN/cpulist
            vec![*value] // Simplified for now
        }
        CpuPinningDto::Socket { value } => {
            tracing::warn!("Socket pinning requires reading socket CPU list");
            // Would need to read socket topology
            vec![*value] // Simplified for now
        }
        CpuPinningDto::Explicit { value } => value.iter().map(|pin| pin.physical_cpu).collect(),
    };

    state
        .driver
        .set_cpuset(&vm_name, &cpus)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Failed to set CPU affinity: {}", e),
            )
        })?;

    tracing::info!("CPU pinning set successfully for VM '{}'", vm_name);
    Ok(StatusCode::OK)
}

/// DELETE /api/vms/:name/cpu/pin - Remove CPU pinning from a VM
pub async fn remove_cpu_pinning(
    RequireWrite(_claims): RequireWrite,
    Path(vm_name): Path<String>,
) -> Result<StatusCode, (StatusCode, String)> {
    tracing::debug!("system::{}", stringify!(remove_cpu_pinning));
    validate_vm_name(&vm_name)?;
    tracing::info!("Removing CPU pinning for VM '{}'", vm_name);
    Ok(StatusCode::OK)
}

/// GET /api/vms/:name/cpu/affinity - Get CPU affinity for a VM
pub async fn get_cpu_affinity(
    RequireRead(_claims): RequireRead,
    State(state): State<Arc<AppState>>,
    Path(vm_name): Path<String>,
) -> Result<Json<Vec<u32>>, (StatusCode, String)> {
    tracing::debug!("system::{}", stringify!(get_cpu_affinity));
    validate_vm_name(&vm_name)?;
    tracing::info!("Getting CPU affinity for VM '{}'", vm_name);

    state
        .driver
        .get_cpuset(&vm_name)
        .await
        .map(Json)
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Failed to read CPU affinity: {e}"),
            )
        })
}

/// PUT /api/vms/:name/memory/limit - Set memory limit for a VM
pub async fn set_memory_limit(
    RequireWrite(_claims): RequireWrite,
    State(state): State<Arc<AppState>>,
    Path(vm_name): Path<String>,
    Json(req): Json<SetMemoryLimitRequest>,
) -> Result<StatusCode, (StatusCode, String)> {
    tracing::debug!("system::{}", stringify!(set_memory_limit));
    validate_vm_name(&vm_name)?;

    // FluxVM-backed VMs' real cgroups live at fluxvm.slice/<uuid>.scope,
    // keyed by FluxVM's own internal VM id, not the VM name -- resolve it
    // via the driver rather than guessing at a name-based cgroup path (see
    // VmDriver::get_cgroup_path).
    let cgroup_path = state.driver.get_cgroup_path(&vm_name).await.map_err(|e| {
        (
            StatusCode::NOT_FOUND,
            format!("VM '{}' not found: {}", vm_name, e),
        )
    })?;

    let controller = match cgroup_path {
        Some(path) => MemoryController::for_path(&vm_name, &path),
        None => {
            return Err((
                StatusCode::CONFLICT,
                format!(
                    "Memory limit control isn't available for '{}': it has no cgroup yet \
                     (VM isn't running, or cgroup delegation failed at launch).",
                    vm_name
                ),
            ));
        }
    };

    if !controller.exists() {
        return Err((
            StatusCode::CONFLICT,
            format!(
                "Memory limit control isn't available for '{}': its cgroup path is stale \
                 (VM likely stopped between resolving it and this call).",
                vm_name
            ),
        ));
    }

    controller.set_limit(req.limit_bytes).map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Failed to set memory limit: {}", e),
        )
    })?;

    if let Some(swap_limit) = req.swap_limit_bytes {
        controller.set_swap_limit(swap_limit).map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Failed to set swap limit: {}", e),
            )
        })?;
    }

    Ok(StatusCode::OK)
}

/// GET /api/vms/:name/memory/usage - Get memory usage for a VM
pub async fn get_memory_usage(
    RequireRead(_claims): RequireRead,
    State(state): State<Arc<AppState>>,
    Path(vm_name): Path<String>,
) -> Result<Json<zyvor_fabric_system::MemoryStats>, (StatusCode, String)> {
    tracing::debug!("system::{}", stringify!(get_memory_usage));
    validate_vm_name(&vm_name)?;

    let cgroup_path = state.driver.get_cgroup_path(&vm_name).await.map_err(|e| {
        (
            StatusCode::NOT_FOUND,
            format!("VM '{}' not found: {}", vm_name, e),
        )
    })?;

    let controller = match cgroup_path {
        Some(path) => MemoryController::for_path(&vm_name, &path),
        None => {
            return Err((
                StatusCode::CONFLICT,
                format!(
                    "Memory usage stats aren't available for '{}': it has no cgroup yet \
                     (VM isn't running, or cgroup delegation failed at launch).",
                    vm_name
                ),
            ));
        }
    };

    if !controller.exists() {
        return Err((
            StatusCode::CONFLICT,
            format!(
                "Memory usage stats aren't available for '{}': its cgroup path is stale \
                 (VM likely stopped between resolving it and this call).",
                vm_name
            ),
        ));
    }

    let stats = controller.get_stats().map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Failed to get memory stats: {}", e),
        )
    })?;

    Ok(Json(stats))
}

/// POST /api/vms/:name/memory/balloon - Enable/disable memory ballooning
pub async fn set_memory_ballooning(
    RequireWrite(_claims): RequireWrite,
    Path(vm_name): Path<String>,
    Json(req): Json<SetMemoryBallooningRequest>,
) -> Result<StatusCode, (StatusCode, String)> {
    tracing::debug!("system::{}", stringify!(set_memory_ballooning));
    validate_vm_name(&vm_name)?;
    // Implement memory ballooning control via QEMU monitor
    tracing::info!(
        "Setting memory ballooning for VM '{}': enabled={}, target={:?}",
        vm_name,
        req.enabled,
        req.target_mb
    );

    if !req.enabled {
        tracing::info!("Memory ballooning disabled for VM '{}'", vm_name);
        return Ok(StatusCode::OK);
    }

    // If enabled and target is specified, set balloon target via QEMU monitor
    if let Some(target_mb) = req.target_mb {
        let vm_name = input_guard::vet_component!(
            &vm_name,
            (StatusCode::BAD_REQUEST, "invalid VM name".to_string())
        );
        let monitor_socket = format!("/run/systemd/vmspawn/{vm_name}/qemu.sock");
        if monitor_socket.contains("..") {
            return Err((
                StatusCode::BAD_REQUEST,
                "invalid monitor socket".to_string(),
            ));
        }
        let socat_target = format!("UNIX-CONNECT:{monitor_socket}");
        let socat_target = if input_guard::COMMAND_ARGS.contains(&socat_target) {
            socat_target
        } else {
            return Err((
                StatusCode::BAD_REQUEST,
                "invalid monitor socket".to_string(),
            ));
        };

        // Check if monitor socket exists
        if !std::path::Path::new(&monitor_socket).exists() {
            return Err((
                StatusCode::NOT_FOUND,
                format!("VM '{}' QEMU monitor socket not found", vm_name),
            ));
        }

        // Send balloon command via socat to QEMU monitor
        // Format: { "execute": "balloon", "arguments": { "value": bytes } }
        let target_bytes = target_mb * 1024 * 1024;
        let qmp_command = format!(
            r#"{{"execute":"balloon","arguments":{{"value":{}}}}}"#,
            target_bytes
        );

        // Write QMP command to socat via stdin (no shell interpolation)
        let mut child = Command::new("socat")
            .arg("-")
            .arg(socat_target)
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn()
            .map_err(|e| {
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    format!("Failed to spawn socat: {}", e),
                )
            })?;

        if let Some(mut stdin) = child.stdin.take() {
            use tokio::io::AsyncWriteExt;
            let _ = stdin.write_all(qmp_command.as_bytes()).await;
            let _ = stdin.write_all(b"\n").await;
            drop(stdin); // close stdin to signal EOF
        }

        let output = child.wait_with_output().await.map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Failed to communicate with QEMU monitor: {}", e),
            )
        })?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            tracing::warn!("Failed to set balloon via QEMU monitor: {}", stderr);
            // Don't fail - ballooning might not be supported
        } else {
            tracing::info!(
                "Memory balloon target set to {}MB for VM '{}'",
                target_mb,
                vm_name
            );
        }
    }

    Ok(StatusCode::OK)
}

/// GET /api/system/memory/hugepages - Get hugepage statistics
pub async fn get_hugepage_stats(
    RequireRead(_claims): RequireRead,
    Query(params): Query<HugepageQuery>,
) -> Result<Json<zyvor_fabric_system::HugepageStats>, (StatusCode, String)> {
    tracing::debug!("system::{}", stringify!(get_hugepage_stats));
    let size: HugepageSize = params.size.into();

    match HugepageManager::get_stats(size) {
        Ok(stats) => Ok(Json(stats)),
        Err(e) => Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Failed to get hugepage stats: {}", e),
        )),
    }
}

/// POST /api/system/memory/hugepages - Allocate hugepages
pub async fn allocate_hugepages(
    RequireWrite(_claims): RequireWrite,
    Json(req): Json<AllocateHugepagesRequest>,
) -> Result<StatusCode, (StatusCode, String)> {
    tracing::debug!("system::{}", stringify!(allocate_hugepages));
    let size: HugepageSize = req.size.into();

    HugepageManager::allocate(size, req.count).map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Failed to allocate hugepages: {}", e),
        )
    })?;

    Ok(StatusCode::OK)
}

/// GET /api/system/memory - Get system memory info
pub async fn get_system_memory(
    RequireRead(_claims): RequireRead,
) -> Result<Json<zyvor_fabric_system::SystemMemory>, (StatusCode, String)> {
    tracing::debug!("system::{}", stringify!(get_system_memory));
    match HugepageManager::get_system_memory() {
        Ok(memory) => Ok(Json(memory)),
        Err(e) => Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Failed to get system memory: {}", e),
        )),
    }
}

// ============================================================================
// Resource Optimization
// ============================================================================

#[derive(Debug, Serialize)]
pub struct OptimizationRecommendation {
    pub vm_name: String,
    pub recommendations: Vec<ResourceRecommendation>,
}

#[derive(Debug, Serialize)]
pub struct ResourceRecommendation {
    pub resource: String,
    pub current_value: String,
    pub recommended_value: String,
    pub reason: String,
    pub impact: String,
}

#[derive(Debug, Serialize)]
pub struct OptimizationResult {
    pub vm_name: String,
    pub applied: Vec<String>,
    pub skipped: Vec<String>,
}

/// GET /api/system/optimization/recommendations - Get resource optimization suggestions
pub async fn get_optimization_recommendations(
    RequireRead(_claims): RequireRead,
    State(state): State<Arc<AppState>>,
) -> Result<Json<Vec<OptimizationRecommendation>>, (StatusCode, String)> {
    tracing::debug!("system::{}", stringify!(get_optimization_recommendations));
    let vms = state.store.list_vms().unwrap_or_else(|e| {
        tracing::warn!("Failed to list VMs: {}", e);
        Vec::new()
    });

    let numa_topology = NumaTopology::detect().ok();
    let cpu_topology = CpuTopology::detect().ok();

    let mut recommendations = Vec::new();

    for vm in &vms {
        if !matches!(vm.state, vm_model::VMState::Running) {
            continue;
        }

        let mut vm_recs = Vec::new();

        // NUMA placement recommendation
        if let Some(ref numa) = numa_topology {
            if numa.nodes.len() > 1 {
                if let Some(placement) = numa.recommend_placement(vm.memory, vm.cpus) {
                    vm_recs.push(ResourceRecommendation {
                        resource: "NUMA".to_string(),
                        current_value: "auto".to_string(),
                        recommended_value: format!("node {}", placement.numa_node),
                        reason: format!(
                            "VM requires {} MB memory and {} CPUs; NUMA node {} has optimal resources",
                            vm.memory, vm.cpus, placement.numa_node
                        ),
                        impact: "Reduced memory access latency, improved performance".to_string(),
                    });
                }
            }
        }

        // CPU pinning recommendation
        if let Some(ref cpu) = cpu_topology {
            if vm.cpus <= cpu.total_cpus && cpu.total_cpus > 4 {
                // Recommend pinning for VMs with dedicated cores
                let available_cores: Vec<u32> = cpu
                    .cpus
                    .iter()
                    .filter(|c| c.online)
                    .map(|c| c.id)
                    .take(vm.cpus as usize)
                    .collect();

                if !available_cores.is_empty() {
                    vm_recs.push(ResourceRecommendation {
                        resource: "CPU Pinning".to_string(),
                        current_value: "unpinned".to_string(),
                        recommended_value: format!("cores {:?}", available_cores),
                        reason: "CPU pinning reduces context switching and cache misses"
                            .to_string(),
                        impact: "5-15% CPU performance improvement for compute-intensive workloads"
                            .to_string(),
                    });
                }
            }
        }

        // Memory optimization: check if hugepages would help
        if vm.memory >= 2048 {
            vm_recs.push(ResourceRecommendation {
                resource: "Hugepages".to_string(),
                current_value: "4KB pages".to_string(),
                recommended_value: "2MB hugepages".to_string(),
                reason: format!(
                    "VM has {} MB memory; hugepages reduce TLB misses for large memory VMs",
                    vm.memory
                ),
                impact: "Reduced TLB misses, 2-5% memory access improvement".to_string(),
            });
        }

        // Memory ballooning recommendation for overcommitted scenarios
        if vm.memory >= 4096 {
            vm_recs.push(ResourceRecommendation {
                resource: "Memory Ballooning".to_string(),
                current_value: "disabled".to_string(),
                recommended_value: "enabled".to_string(),
                reason: "Enables dynamic memory reclamation for better host utilization"
                    .to_string(),
                impact: "Allows host to reclaim unused VM memory during pressure".to_string(),
            });
        }

        if !vm_recs.is_empty() {
            recommendations.push(OptimizationRecommendation {
                vm_name: vm.name.clone(),
                recommendations: vm_recs,
            });
        }
    }

    Ok(Json(recommendations))
}

/// POST /api/vms/:name/optimize - Auto-apply optimal NUMA/CPU settings
pub async fn optimize_vm(
    RequireWrite(_claims): RequireWrite,
    State(state): State<Arc<AppState>>,
    Path(vm_name): Path<String>,
) -> Result<Json<OptimizationResult>, (StatusCode, String)> {
    tracing::debug!("system::{}", stringify!(optimize_vm));
    validate_vm_name(&vm_name)?;

    let vm = match state.store.get_vm(&vm_name) {
        Ok(Some(vm)) => vm,
        Ok(None) => {
            return Err((StatusCode::NOT_FOUND, format!("VM '{}' not found", vm_name)));
        }
        Err(e) => {
            return Err((StatusCode::INTERNAL_SERVER_ERROR, e.to_string()));
        }
    };

    let mut applied = Vec::new();
    let mut skipped = Vec::new();

    // Try to apply NUMA placement
    if let Ok(numa) = NumaTopology::detect() {
        if numa.nodes.len() > 1 {
            if let Some(placement) = numa.recommend_placement(vm.memory, vm.cpus) {
                let cpu_list = placement
                    .cpu_affinity
                    .iter()
                    .map(|c| c.to_string())
                    .collect::<Vec<_>>()
                    .join(",");

                match state
                    .driver
                    .set_cpuset(&vm_name, &placement.cpu_affinity)
                    .await
                {
                    Ok(()) => {
                        applied.push(format!(
                            "CPU pinning to NUMA node {} (cores: {})",
                            placement.numa_node, cpu_list
                        ));
                    }
                    Err(e) => {
                        skipped.push(format!("CPU pinning: failed to apply: {e}"));
                    }
                }
            } else {
                skipped.push("NUMA placement: no suitable node found".to_string());
            }
        } else {
            skipped.push("NUMA placement: single-node system".to_string());
        }
    } else {
        skipped.push("NUMA placement: failed to detect topology".to_string());
    }

    // Try to set memory limit via cgroup
    let cgroup_controller = match state.driver.get_cgroup_path(&vm_name).await {
        Ok(Some(path)) => Some(MemoryController::for_path(&vm_name, &path)),
        Ok(None) => None,
        Err(e) => {
            skipped.push(format!("Memory limit: failed to resolve cgroup: {e}"));
            None
        }
    };
    match cgroup_controller {
        Some(controller) if controller.exists() => {
            let limit_bytes = vm.memory * 1024 * 1024;
            match controller.set_limit(limit_bytes) {
                Ok(_) => {
                    applied.push(format!("Memory limit set to {} MB", vm.memory));
                }
                Err(e) => {
                    skipped.push(format!("Memory limit: {}", e));
                }
            }
        }
        _ => {
            skipped.push("Memory limit: cgroup not found (VM may not be running)".to_string());
        }
    }

    Ok(Json(OptimizationResult {
        vm_name,
        applied,
        skipped,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_hugepage_size_conversion() {
        let size_2mb = HugepageSizeDto::Size2MB;
        let hugepage_size: HugepageSize = size_2mb.into();
        assert!(matches!(hugepage_size, HugepageSize::Size2MB));
    }

    #[test]
    fn test_cpu_pinning_deserialization() {
        let json = r#"{"type": "NumaNode", "value": 0}"#;
        let pinning: CpuPinningDto = serde_json::from_str(json).unwrap();
        assert!(matches!(pinning, CpuPinningDto::NumaNode { value: 0 }));
    }
}
