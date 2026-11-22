// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: LicenseRef-Zyvor-Production-2.0

use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use serde::Deserialize;
use std::sync::Arc;

use crate::api::events;
use crate::qmp::{QmpClient, QmpSession};
use crate::server::AppState;
use security::RequireWrite;

#[derive(Debug, Deserialize)]
pub struct HotplugCpuRequest {
    pub count: u32,
}

#[derive(Debug, Deserialize)]
pub struct HotplugMemoryRequest {
    pub size_mb: u64,
}

#[derive(Debug, Deserialize)]
pub struct HotplugDiskRequest {
    pub path: String,
    #[serde(default = "default_bus")]
    pub bus: String,
}

fn default_bus() -> String {
    "virtio".to_string()
}

#[derive(Debug, Deserialize)]
pub struct HotplugNicRequest {
    /// Host bridge. Empty when `direct_uplink` is set.
    #[serde(default)]
    pub bridge: String,
    #[serde(default)]
    pub model: Option<String>,
    /// Host NIC for a bridge-less tap. Mutually exclusive with `bridge`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub direct_uplink: Option<String>,
    /// `l2-uplink` (default) or `peer-veth`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub direct_mode: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub direct_guest_ips: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mac: Option<String>,
}

pub(crate) fn not_available_response() -> impl IntoResponse {
    (
        StatusCode::NOT_IMPLEMENTED,
        Json(serde_json::json!({
            "error": "QMP socket not available for this VM's driver backend."
        })),
    )
}

/// Resolve `vm_name`'s QMP control socket via the active driver backend
/// (`MachinectlDriver`'s systemd-vmspawn convention, or FluxVM's
/// `VmRecord.control_socket`), returning `None` if the driver has no
/// socket for it (not running, unknown, or a backend/hypervisor with no
/// QMP equivalent) — the caller falls back to `not_available_response()`.
pub(crate) async fn resolve_qmp(state: &AppState, vm_name: &str) -> Option<QmpClient> {
    match state.driver.get_control_socket(vm_name).await {
        Ok(Some(path)) => Some(QmpClient::for_socket(path.to_string_lossy().into_owned())),
        Ok(None) => None,
        Err(e) => {
            tracing::warn!("Failed to resolve QMP control socket for '{vm_name}': {e:#}");
            None
        }
    }
}

/// Number of empty `pcie-root-port` slots FluxVM reserves at boot for
/// device hotplug (see `fluxvm-qemu::HOTPLUG_PCIE_PORTS` -- this
/// convention is mirrored here rather than shared as code, since the two
/// talk over FluxVM's REST API, not a Rust dependency).
const HOTPLUG_PCIE_PORTS: u8 = 4;

/// `device_add` onto the first of `bus_candidates` that accepts it, trying
/// each in turn -- used for PCIe hotplug (`hotplug-pcie-0..N`, one device
/// per port).
fn device_add_trying_buses(
    session: &mut QmpSession,
    mut device_args: serde_json::Value,
    bus_candidates: &[String],
) -> Result<String, String> {
    let mut last_err = String::from("no candidate buses given");
    for bus in bus_candidates {
        if let Some(obj) = device_args.as_object_mut() {
            obj.insert("bus".to_string(), serde_json::json!(bus));
        }
        match session.execute("device_add", device_args.clone()) {
            Ok(_) => return Ok(bus.clone()),
            Err(e) => last_err = e.to_string(),
        }
    }
    Err(format!(
        "all {} candidate buses are full or unavailable (last error: {last_err}) -- restart the VM to reclaim them",
        bus_candidates.len()
    ))
}

/// Hotplug root ports (`hotplug-pcie-N`) that already host a child device,
/// per QEMU's own `query-pci` view. `device_add` to an already-occupied
/// port does NOT fail -- found live: QEMU happily stacks a second device
/// onto the same root port's bus at the next PCI slot -- but the guest's
/// ACPI hotplug slot notification only fires for a port's first child, so
/// anything added after that is realized in QEMU yet invisible to the
/// guest kernel (a hotplugged NIC then a hotplugged disk both silently
/// landed on `hotplug-pcie-0`; only the NIC ever appeared in the guest's
/// `lspci`/`lsblk`). So port selection can't rely on `device_add` erroring
/// out once a port is "full" -- it must check real occupancy first.
fn occupied_hotplug_buses(qmp: &QmpClient) -> std::collections::HashSet<String> {
    let mut occupied = std::collections::HashSet::new();
    let Ok(pci) = qmp.execute("query-pci", serde_json::Value::Null) else {
        // Can't determine occupancy -- treat everything as free and let
        // device_add itself be the final arbiter, same as before this fix.
        return occupied;
    };
    for bus_info in pci.as_array().into_iter().flatten() {
        for dev in bus_info
            .get("devices")
            .and_then(|d| d.as_array())
            .into_iter()
            .flatten()
        {
            let Some(qdev_id) = dev.get("qdev_id").and_then(|v| v.as_str()) else {
                continue;
            };
            if !qdev_id.starts_with("hotplug-pcie-") {
                continue;
            }
            let has_children = dev
                .get("pci_bridge")
                .and_then(|b| b.get("devices"))
                .and_then(|d| d.as_array())
                .is_some_and(|d| !d.is_empty());
            if has_children {
                occupied.insert(qdev_id.to_string());
            }
        }
    }
    occupied
}

/// `device_add` a PCI(e) device (NIC, extra disk) onto one of the
/// pre-reserved hotplug root ports, trying each genuinely empty one in
/// turn. q35's root complex (`pcie.0`) itself refuses `device_add`
/// outright -- found live: "Bus 'pcie.0' does not support hotplugging" --
/// every hotpluggable PCI(e) device needs an explicit target bus that
/// supports it, and only one device per port is actually visible to the
/// guest (see `occupied_hotplug_buses`), so a previous hotplug may have
/// already filled some of them.
fn device_add_on_hotplug_bus(
    qmp: &QmpClient,
    session: &mut QmpSession,
    device_args: serde_json::Value,
) -> Result<String, String> {
    let occupied = occupied_hotplug_buses(qmp);
    let candidates: Vec<String> = (0..HOTPLUG_PCIE_PORTS)
        .map(|i| format!("hotplug-pcie-{i}"))
        .filter(|bus| !occupied.contains(bus))
        .collect();
    if candidates.is_empty() {
        return Err(format!(
            "all {HOTPLUG_PCIE_PORTS} hotplug root ports already hold a device -- restart the VM to reclaim them"
        ));
    }
    device_add_trying_buses(session, device_args, &candidates)
}

/// A short, unique-enough id for a QMP `node-name`/device `id`. QEMU caps
/// block-node names at 31 bytes (`BDRV_NODE_NAME_MAX`) -- found live: the
/// previous `format!("drive-hotplug-{}", Uuid::new_v4().simple())` was 46
/// bytes and blockdev-add rejected it outright with "Node name too long".
fn short_hotplug_id(prefix: &str) -> String {
    format!(
        "{prefix}-{}",
        &uuid::Uuid::new_v4().simple().to_string()[..8]
    )
}

/// POST /api/vms/:name/hotplug/cpu - Hot-add vCPUs
pub async fn hotplug_cpu(
    RequireWrite(_claims): RequireWrite,
    State(state): State<Arc<AppState>>,
    Path(vm_name): Path<String>,
    Json(req): Json<HotplugCpuRequest>,
) -> impl IntoResponse {
    tracing::debug!("hotplug::{}", stringify!(hotplug_cpu));
    if let Err((s, m)) = crate::validation::validate_vm_name(&vm_name) {
        return crate::api_error::json_error(s, m).into_response();
    }
    let Some(qmp) = resolve_qmp(&state, &vm_name).await else {
        return not_available_response().into_response();
    };

    // Query hotpluggable CPUs to find available slots
    match qmp.execute("query-hotpluggable-cpus", serde_json::Value::Null) {
        Ok(cpus) => {
            // Find unrealized CPU slots and add them
            let mut added = 0u32;
            if let Some(cpu_list) = cpus.as_array() {
                for cpu in cpu_list {
                    if added >= req.count {
                        break;
                    }
                    // Skip already-realized CPUs
                    if cpu.get("qom-path").is_some() {
                        continue;
                    }
                    if let Some(props) = cpu.get("props") {
                        // Use the CPU type reported by QEMU instead of hardcoding x86_64
                        let driver = cpu
                            .get("type")
                            .and_then(|v| v.as_str())
                            .unwrap_or("host-x86_64-cpu");
                        let cpu_id = format!("cpu-hotplug-{}", added);
                        let args = serde_json::json!({
                            "driver": driver,
                            "id": cpu_id,
                            "socket-id": props.get("socket-id").and_then(|v| v.as_u64()).unwrap_or(0),
                            "core-id": props.get("core-id").and_then(|v| v.as_u64()).unwrap_or(0),
                            "thread-id": props.get("thread-id").and_then(|v| v.as_u64()).unwrap_or(0),
                        });

                        match qmp.execute("device_add", args) {
                            Ok(_) => added += 1,
                            Err(e) => {
                                let mut body = api_error::api_error_json(
                                    "operation_failed",
                                    format!("CPU hotplug failed: {}", e),
                                );
                                if let Some(obj) = body.as_object_mut() {
                                    obj.insert("added".to_string(), serde_json::json!(added));
                                }
                                return (StatusCode::INTERNAL_SERVER_ERROR, Json(body))
                                    .into_response();
                            }
                        }
                    }
                }
            }

            events::record_event(
                &state,
                events::VMEventType::CpuHotplug,
                &vm_name,
                Some(format!("Added {} vCPUs", added)),
            );
            Json(serde_json::json!({
                "status": "ok",
                "cpus_added": added,
            }))
            .into_response()
        }
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            crate::api_error::json_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Failed to query CPUs: {}", e),
            ),
        )
            .into_response(),
    }
}

/// POST /api/vms/:name/hotplug/memory - Hot-add memory
pub async fn hotplug_memory(
    RequireWrite(_claims): RequireWrite,
    State(state): State<Arc<AppState>>,
    Path(vm_name): Path<String>,
    Json(req): Json<HotplugMemoryRequest>,
) -> impl IntoResponse {
    tracing::debug!("hotplug::{}", stringify!(hotplug_memory));
    if let Err((s, m)) = crate::validation::validate_vm_name(&vm_name) {
        return crate::api_error::json_error(s, m).into_response();
    }
    let Some(qmp) = resolve_qmp(&state, &vm_name).await else {
        return not_available_response().into_response();
    };

    let size_bytes = req.size_mb * 1024 * 1024;
    let backend_id = format!("mem-hotplug-{}", uuid::Uuid::new_v4().simple());
    let dimm_id = format!("dimm-hotplug-{}", uuid::Uuid::new_v4().simple());

    // object-add then device_add must run over the SAME QMP connection --
    // found live: object-add succeeds, but a device_add on a fresh
    // reconnect immediately after can't see the object it just created
    // ("Device '<id>' not found"), even though the exact same two
    // commands replayed over one held-open session always work.
    let mut session = match qmp.open_session() {
        Ok(s) => s,
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                crate::api_error::json_error(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    format!("Failed to open QMP session: {}", e),
                ),
            )
                .into_response();
        }
    };

    // Add memory backend
    let backend_args = serde_json::json!({
        "qom-type": "memory-backend-ram",
        "id": backend_id,
        "size": size_bytes,
    });

    if let Err(e) = session.execute("object-add", backend_args) {
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            crate::api_error::json_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Memory backend add failed: {}", e),
            ),
        )
            .into_response();
    }

    // Add DIMM device
    let dimm_args = serde_json::json!({
        "driver": "pc-dimm",
        "id": dimm_id,
        "memdev": backend_id,
    });

    match session.execute("device_add", dimm_args) {
        Ok(_) => {
            events::record_event(
                &state,
                events::VMEventType::MemoryHotplug,
                &vm_name,
                Some(format!("Added {} MB", req.size_mb)),
            );
            Json(serde_json::json!({
                "status": "ok",
                "memory_added_mb": req.size_mb,
                "dimm_id": dimm_id,
            }))
            .into_response()
        }
        Err(e) => {
            // Rollback: remove the memory backend object since device_add failed
            if let Err(rollback_err) =
                session.execute("object-del", serde_json::json!({"id": backend_id}))
            {
                tracing::warn!(
                    "Failed to rollback memory backend '{}': {}",
                    backend_id,
                    rollback_err
                );
            }
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                crate::api_error::json_error(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    format!("DIMM hotplug failed: {}", e),
                ),
            )
                .into_response()
        }
    }
}

/// POST /api/vms/:name/hotplug/disk - Hot-add a disk
pub async fn hotplug_disk(
    RequireWrite(_claims): RequireWrite,
    State(state): State<Arc<AppState>>,
    Path(vm_name): Path<String>,
    Json(req): Json<HotplugDiskRequest>,
) -> impl IntoResponse {
    tracing::debug!("hotplug::{}", stringify!(hotplug_disk));
    if let Err((s, m)) = crate::validation::validate_vm_name(&vm_name) {
        return crate::api_error::json_error(s, m).into_response();
    }

    // Validate disk path to prevent traversal
    if let Err((status, msg)) = crate::validation::validate_host_path(&req.path) {
        return crate::api_error::json_error(status, msg).into_response();
    }

    // Reject up front, before touching QMP at all: IDE hotplug can never
    // succeed here. Confirmed live -- q35's built-in ich9-ahci controller
    // has 6 empty ports (ide.0..ide.5, verified present via qom-list), but
    // every one of them rejects device_add outright with "Bus 'ide.N' does
    // not support hotplugging" -- QEMU's AHCI ports only accept a drive
    // declared at boot, never a runtime hot-add. A separate legacy PIIX3
    // IDE controller doesn't help either: QEMU refuses to even device_add
    // one ("Parameter 'driver' expects a pluggable device type").
    if req.bus == "ide" {
        return crate::api_error::json_error(
            StatusCode::BAD_REQUEST,
            "IDE disk hotplug isn't supported: QEMU's AHCI controller only accepts IDE drives \
             declared at boot, not hot-added at runtime. Use bus=\"scsi\" (or the default virtio) instead.",
        )
        .into_response();
    }

    let Some(qmp) = resolve_qmp(&state, &vm_name).await else {
        return not_available_response().into_response();
    };

    let node_name = short_hotplug_id("drive-hotplug");
    let device_id = short_hotplug_id("disk-hotplug");

    // blockdev-add then device_add (referencing that node-name via
    // "drive") must run over the SAME QMP connection -- see QmpClient::
    // execute's doc comment for why a fresh reconnect between the two
    // breaks it.
    let mut session = match qmp.open_session() {
        Ok(s) => s,
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                crate::api_error::json_error(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    format!("Failed to open QMP session: {}", e),
                ),
            )
                .into_response();
        }
    };

    // Add block device
    let blockdev_args = serde_json::json!({
        "driver": "qcow2",
        "node-name": node_name,
        "file": {
            "driver": "file",
            "filename": req.path,
        }
    });

    if let Err(e) = session.execute("blockdev-add", blockdev_args) {
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            crate::api_error::json_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("blockdev-add failed: {}", e),
            ),
        )
            .into_response();
    }

    // Add device, onto whichever bus its driver actually needs: the
    // virtio-blk-pci default goes through a pre-reserved hotplug PCIe root
    // port, scsi-hd through the single virtio-scsi controller FluxVM
    // adds at boot -- see device_add_on_hotplug_bus's doc comment. (`ide`
    // was already rejected above, before we got here.)
    let driver = if req.bus == "scsi" {
        "scsi-hd"
    } else {
        "virtio-blk-pci"
    };

    let device_args = serde_json::json!({
        "driver": driver,
        "id": device_id,
        "drive": node_name,
    });

    let result = if driver == "virtio-blk-pci" {
        device_add_on_hotplug_bus(&qmp, &mut session, device_args)
    } else {
        let mut device_args = device_args;
        if let Some(obj) = device_args.as_object_mut() {
            obj.insert("bus".to_string(), serde_json::json!("scsi0.0"));
        }
        session
            .execute("device_add", device_args)
            .map(|_| String::from("scsi0.0"))
            .map_err(|e| e.to_string())
    };

    match result {
        Ok(_) => {
            events::record_event(
                &state,
                events::VMEventType::DiskAttached,
                &vm_name,
                Some(format!("Disk: {}", req.path)),
            );
            Json(serde_json::json!({
                "status": "ok",
                "device_id": device_id,
                "node_name": node_name,
            }))
            .into_response()
        }
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            crate::api_error::json_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("device_add failed: {}", e),
            ),
        )
            .into_response(),
    }
}

/// DELETE /api/vms/:name/hotplug/disk/:id - Hot-remove a disk
pub async fn hotremove_disk(
    RequireWrite(_claims): RequireWrite,
    State(state): State<Arc<AppState>>,
    Path((vm_name, device_id)): Path<(String, String)>,
) -> impl IntoResponse {
    tracing::debug!("hotplug::{}", stringify!(hotremove_disk));
    if let Err((s, m)) = crate::validation::validate_vm_name(&vm_name) {
        return crate::api_error::json_error(s, m).into_response();
    }
    // Validate device_id format (alphanumeric, hyphens, underscores, dots)
    if device_id.is_empty()
        || device_id.len() > 128
        || !device_id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'))
    {
        return crate::api_error::json_error(StatusCode::BAD_REQUEST, "Invalid device ID")
            .into_response();
    }
    let Some(qmp) = resolve_qmp(&state, &vm_name).await else {
        return not_available_response().into_response();
    };

    let args = serde_json::json!({"id": device_id});
    match qmp.execute("device_del", args) {
        Ok(_) => Json(serde_json::json!({
            "status": "ok",
            "removed": device_id,
        }))
        .into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            crate::api_error::json_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("device_del failed: {}", e),
            ),
        )
            .into_response(),
    }
}

/// POST /api/vms/:name/hotplug/nic - Hot-add a NIC
pub async fn hotplug_nic(
    RequireWrite(_claims): RequireWrite,
    State(state): State<Arc<AppState>>,
    Path(vm_name): Path<String>,
    Json(req): Json<HotplugNicRequest>,
) -> impl IntoResponse {
    tracing::debug!("hotplug::{}", stringify!(hotplug_nic));
    if let Err((s, m)) = crate::validation::validate_vm_name(&vm_name) {
        return crate::api_error::json_error(s, m).into_response();
    }
    let uplink = req
        .direct_uplink
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string);
    if uplink.is_some() && !req.bridge.trim().is_empty() {
        return crate::api_error::json_error(
            StatusCode::BAD_REQUEST,
            "NIC hotplug takes a bridge or a direct uplink, not both",
        )
        .into_response();
    }
    if let Some(outer) = uplink {
        return hotplug_direct_nic(&state, &vm_name, &outer, &req).await;
    }
    let Some(qmp) = resolve_qmp(&state, &vm_name).await else {
        return not_available_response().into_response();
    };

    let netdev_id = short_hotplug_id("net-hotplug");
    let device_id = short_hotplug_id("nic-hotplug");

    // Validate bridge name
    if let Err(msg) = crate::validation::validate_hostname(&req.bridge) {
        return crate::api_error::json_error(
            StatusCode::BAD_REQUEST,
            format!("Invalid bridge name: {}", msg),
        )
        .into_response();
    }

    // netdev_add then device_add (referencing that netdev id) must run
    // over the SAME QMP connection -- see QmpClient::execute's doc comment
    // for why a fresh reconnect between the two breaks it.
    let mut session = match qmp.open_session() {
        Ok(s) => s,
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                crate::api_error::json_error(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    format!("Failed to open QMP session: {}", e),
                ),
            )
                .into_response();
        }
    };

    // Add netdev (tap backend attached to bridge)
    let netdev_args = serde_json::json!({
        "type": "tap",
        "id": netdev_id,
        "br": req.bridge,
        "helper": "/usr/lib/qemu/qemu-bridge-helper",
    });

    if let Err(e) = session.execute("netdev_add", netdev_args) {
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            crate::api_error::json_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("netdev_add failed: {}", e),
            ),
        )
            .into_response();
    }

    // Validate NIC model against allowlist
    const ALLOWED_NIC_MODELS: &[&str] = &["virtio-net-pci", "e1000", "e1000e", "rtl8139"];
    let model = req.model.as_deref().unwrap_or("virtio-net-pci");
    if !ALLOWED_NIC_MODELS.contains(&model) {
        return crate::api_error::json_error(
            StatusCode::BAD_REQUEST,
            format!(
                "Invalid NIC model '{}'. Allowed: {}",
                model,
                ALLOWED_NIC_MODELS.join(", ")
            ),
        )
        .into_response();
    }

    // Add NIC device, onto one of the pre-reserved hotplug root ports --
    // q35's root complex refuses device_add outright otherwise (see
    // device_add_on_hotplug_bus's doc comment).
    let device_args = serde_json::json!({
        "driver": model,
        "id": device_id,
        "netdev": netdev_id,
    });

    match device_add_on_hotplug_bus(&qmp, &mut session, device_args) {
        Ok(_) => Json(serde_json::json!({
            "status": "ok",
            "device_id": device_id,
            "netdev_id": netdev_id,
        }))
        .into_response(),
        Err(e) => {
            // Rollback: remove the orphaned netdev since device_add failed
            if let Err(rollback_err) =
                session.execute("netdev_del", serde_json::json!({"id": netdev_id}))
            {
                tracing::warn!(
                    "Failed to rollback netdev '{}': {}",
                    netdev_id,
                    rollback_err
                );
            }
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                crate::api_error::json_error(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    format!("NIC device_add failed: {}", e),
                ),
            )
                .into_response()
        }
    }
}

/// Direct taps are created by FluxVM (it attaches `fluxvm_direct` and passes
/// the fd). QMP `netdev_add` cannot open that tap.
async fn hotplug_direct_nic(
    state: &AppState,
    vm_name: &str,
    outer: &str,
    req: &HotplugNicRequest,
) -> Response {
    use zyvor_fabric_fluxvm_client::{
        DirectMode, DirectSpec, FluxVmClient, HotplugNicRequest as FluxHotplug,
    };

    let errors = vm_model::direct_uplink_errors(
        Some(outer),
        req.direct_mode.as_deref(),
        &req.direct_guest_ips,
        false,
        false,
    );
    if !errors.is_empty() {
        return crate::api_error::json_error(StatusCode::BAD_REQUEST, errors.join("; "))
            .into_response();
    }
    let mode = match req
        .direct_mode
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        None | Some("l2-uplink") => DirectMode::L2Uplink,
        Some("peer-veth") => DirectMode::PeerVeth,
        Some(other) => {
            return crate::api_error::json_error(
                StatusCode::BAD_REQUEST,
                format!("direct_mode must be 'l2-uplink' or 'peer-veth', got '{other}'"),
            )
            .into_response();
        }
    };
    let client = match FluxVmClient::new(&state.config.driver.fluxvm_url) {
        Ok(client) => match state.config.driver.fluxvm_token.as_deref() {
            Some(token) if !token.is_empty() => client.with_token(token.to_owned()),
            _ => client,
        },
        Err(e) => {
            return crate::api_error::json_error(
                StatusCode::BAD_GATEWAY,
                format!("FluxVM client: {e}"),
            )
            .into_response();
        }
    };
    let record = match client.find_by_name(vm_name).await {
        Ok(Some(record)) => record,
        Ok(None) => {
            return crate::api_error::json_error(
                StatusCode::NOT_FOUND,
                format!("VM '{vm_name}' is not known to FluxVM"),
            )
            .into_response();
        }
        Err(e) => {
            return crate::api_error::json_error(StatusCode::BAD_GATEWAY, e.to_string())
                .into_response();
        }
    };
    let body = FluxHotplug {
        bridge: String::new(),
        mac: req.mac.clone(),
        direct: Some(DirectSpec {
            outer: outer.to_string(),
            netns_path: None,
            mode,
            guest_ips: req.direct_guest_ips.clone(),
        }),
    };
    match client.hotplug_nic(record.id, &body).await {
        Ok(value) => {
            let persisted = persist_direct_hotplug(state, vm_name, outer, req);
            Json(serde_json::json!({
                "status": "ok",
                "direct_uplink": outer,
                "persisted": persisted,
                "fluxvm": value,
            }))
            .into_response()
        }
        Err(e) => crate::api_error::json_error(
            StatusCode::BAD_GATEWAY,
            format!("FluxVM hotplug/nic failed: {e}"),
        )
        .into_response(),
    }
}

/// Remember a hotplugged uplink on the Fabric VM so a later recreate sends
/// it. Skipped when the VM is bridged or NAT, or already has a different uplink.
fn persist_direct_hotplug(
    state: &AppState,
    vm_name: &str,
    outer: &str,
    req: &HotplugNicRequest,
) -> bool {
    let Ok(Some(mut vm)) = state.store.get_vm(vm_name) else {
        return false;
    };
    if vm.network_tap || !vm.port_forwards.is_empty() {
        return false;
    }
    match vm
        .direct_uplink
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        Some(existing) if existing != outer => return false,
        Some(_) => {
            for ip in &req.direct_guest_ips {
                if !vm.direct_guest_ips.iter().any(|have| have == ip) {
                    vm.direct_guest_ips.push(ip.clone());
                }
            }
            vm.direct_guest_ips.truncate(8);
        }
        None => {
            vm.direct_uplink = Some(outer.to_string());
            vm.direct_mode = req.direct_mode.clone();
            vm.direct_guest_ips = req.direct_guest_ips.clone();
        }
    }
    state.store.save_vm(&vm).is_ok()
}

/// DELETE /api/vms/:name/hotplug/nic/:id - Hot-remove a NIC
pub async fn hotremove_nic(
    RequireWrite(_claims): RequireWrite,
    State(state): State<Arc<AppState>>,
    Path((vm_name, device_id)): Path<(String, String)>,
) -> impl IntoResponse {
    tracing::debug!("hotplug::{}", stringify!(hotremove_nic));
    if let Err((s, m)) = crate::validation::validate_vm_name(&vm_name) {
        return crate::api_error::json_error(s, m).into_response();
    }
    // Validate device_id format (alphanumeric, hyphens, underscores, dots)
    if device_id.is_empty()
        || device_id.len() > 128
        || !device_id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'))
    {
        return crate::api_error::json_error(StatusCode::BAD_REQUEST, "Invalid device ID")
            .into_response();
    }
    let Some(qmp) = resolve_qmp(&state, &vm_name).await else {
        return not_available_response().into_response();
    };

    let args = serde_json::json!({"id": device_id});
    match qmp.execute("device_del", args) {
        Ok(_) => Json(serde_json::json!({
            "status": "ok",
            "removed": device_id,
        }))
        .into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            crate::api_error::json_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("NIC device_del failed: {}", e),
            ),
        )
            .into_response(),
    }
}
