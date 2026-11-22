// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: LicenseRef-Zyvor-Production-2.0

use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use security::{RequireRead, RequireWrite};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;

use super::hotplug::{not_available_response, resolve_qmp};
use crate::server::AppState;

#[derive(Debug, Serialize)]
pub struct PciDevice {
    pub address: String,
    pub vendor_id: String,
    pub device_id: String,
    pub vendor_name: String,
    pub device_name: String,
    pub class_name: String,
    pub iommu_group: i64,
    pub driver: String,
    pub numa_node: i64,
    pub attached_to: Option<String>,
}

fn read_sysfs_trimmed(path: &std::path::Path) -> Option<String> {
    std::fs::read_to_string(path)
        .ok()
        .map(|s| s.trim().to_string())
}

fn read_hex_id(dir: &std::path::Path, file: &str) -> String {
    read_sysfs_trimmed(&dir.join(file))
        .map(|s| s.trim_start_matches("0x").to_string())
        .unwrap_or_default()
}

fn read_driver(dir: &std::path::Path) -> String {
    std::fs::read_link(dir.join("driver"))
        .ok()
        .and_then(|p| p.file_name().map(|n| n.to_string_lossy().into_owned()))
        .unwrap_or_default()
}

fn read_iommu_group(dir: &std::path::Path) -> i64 {
    std::fs::read_link(dir.join("iommu_group"))
        .ok()
        .and_then(|p| p.file_name().and_then(|n| n.to_string_lossy().parse().ok()))
        .unwrap_or(-1)
}

/// Best-effort human-readable names via `lspci -Dvmm` (machine-readable,
/// one `Key:\tValue` paragraph per device), keyed by PCI slot address.
/// Names are cosmetic only -- IDs/driver/iommu/numa come from sysfs, which
/// is always present regardless of whether `lspci` is installed.
async fn lspci_names() -> HashMap<String, HashMap<String, String>> {
    let mut out = HashMap::new();
    let Ok(output) = tokio::process::Command::new("lspci")
        .args(["-Dvmm"])
        .output()
        .await
    else {
        return out;
    };
    let stdout = String::from_utf8_lossy(&output.stdout);
    let mut current: HashMap<String, String> = HashMap::new();
    for line in stdout.lines() {
        if line.trim().is_empty() {
            if let Some(slot) = current.get("Slot").cloned() {
                out.insert(slot, std::mem::take(&mut current));
            }
            current.clear();
            continue;
        }
        if let Some((key, value)) = line.split_once(':') {
            current.insert(key.trim().to_string(), value.trim().to_string());
        }
    }
    if let Some(slot) = current.get("Slot").cloned() {
        out.insert(slot, current);
    }
    out
}

/// GET /api/system/pci-devices - List PCI devices available for passthrough
pub async fn list_pci_devices(
    RequireRead(_claims): RequireRead,
) -> Result<Json<Vec<PciDevice>>, (StatusCode, Json<serde_json::Value>)> {
    let names = lspci_names().await;
    let sysfs_root = std::path::Path::new("/sys/bus/pci/devices");
    let entries = std::fs::read_dir(sysfs_root).map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": format!("Failed to read {}: {}", sysfs_root.display(), e)})),
        )
    })?;

    let mut devices = Vec::new();
    for entry in entries.flatten() {
        let dir = entry.path();
        let address = entry.file_name().to_string_lossy().into_owned();
        // sysfs uses the full "0000:01:00.0" address; lspci -D matches the same format.
        let meta = names.get(&address);
        devices.push(PciDevice {
            vendor_id: read_hex_id(&dir, "vendor"),
            device_id: read_hex_id(&dir, "device"),
            vendor_name: meta
                .and_then(|m| m.get("Vendor").cloned())
                .unwrap_or_else(|| "Unknown vendor".to_string()),
            device_name: meta
                .and_then(|m| m.get("Device").cloned())
                .unwrap_or_else(|| "Unknown device".to_string()),
            class_name: meta
                .and_then(|m| m.get("Class").cloned())
                .unwrap_or_else(|| "Unknown class".to_string()),
            driver: read_driver(&dir),
            iommu_group: read_iommu_group(&dir),
            numa_node: read_sysfs_trimmed(&dir.join("numa_node"))
                .and_then(|s| s.parse().ok())
                .unwrap_or(-1),
            attached_to: None,
            address,
        });
    }
    devices.sort_by(|a, b| a.address.cmp(&b.address));

    Ok(Json(devices))
}

#[derive(Debug, Deserialize)]
pub struct PciAttachRequest {
    pub address: String,
    #[serde(default)]
    pub rombar: Option<bool>,
}

fn pci_device_id(address: &str) -> String {
    format!("pci-hotplug-{}", address.replace([':', '.'], "-"))
}

/// POST /api/vms/:name/devices/pci - Attach a host PCI device to a running VM via VFIO
pub async fn attach_pci(
    RequireWrite(_claims): RequireWrite,
    State(state): State<Arc<AppState>>,
    Path(vm_name): Path<String>,
    Json(req): Json<PciAttachRequest>,
) -> impl IntoResponse {
    if let Err((s, m)) = crate::validation::validate_vm_name(&vm_name) {
        return crate::api_error::json_error(s, m).into_response();
    }
    if let Err((s, m)) = crate::validation::validate_pci_address(&req.address) {
        return crate::api_error::json_error(s, m).into_response();
    }
    let dir = std::path::Path::new("/sys/bus/pci/devices").join(&req.address);
    if !dir.exists() {
        return crate::api_error::json_error(
            StatusCode::NOT_FOUND,
            format!("No PCI device at address '{}'", req.address),
        )
        .into_response();
    }
    let driver = read_driver(&dir);
    if driver != "vfio-pci" {
        return crate::api_error::json_error(
            StatusCode::BAD_REQUEST,
            format!(
                "Device '{}' is bound to driver '{}', not 'vfio-pci'. Rebinding a device's \
                 driver can disrupt the host (e.g. its network or storage controller) and is \
                 intentionally not done automatically -- bind it to vfio-pci yourself first \
                 (driverctl set-override {} vfio-pci), then retry.",
                req.address,
                if driver.is_empty() { "none" } else { &driver },
                req.address
            ),
        )
        .into_response();
    }

    let Some(qmp) = resolve_qmp(&state, &vm_name).await else {
        return not_available_response().into_response();
    };

    let device_id = pci_device_id(&req.address);
    let mut device_args = serde_json::json!({
        "driver": "vfio-pci",
        "host": req.address,
        "id": device_id,
    });
    if let Some(rombar) = req.rombar {
        device_args["rombar"] = serde_json::json!(if rombar { 1 } else { 0 });
    }

    match qmp.execute("device_add", device_args) {
        Ok(_) => Json(serde_json::json!({"status": "ok", "device_id": device_id})).into_response(),
        Err(e) => crate::api_error::json_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("PCI device_add failed: {}", e),
        )
        .into_response(),
    }
}

/// DELETE /api/vms/:name/devices/pci/:address - Detach a passthrough PCI device
pub async fn detach_pci(
    RequireWrite(_claims): RequireWrite,
    State(state): State<Arc<AppState>>,
    Path((vm_name, address)): Path<(String, String)>,
) -> impl IntoResponse {
    if let Err((s, m)) = crate::validation::validate_vm_name(&vm_name) {
        return crate::api_error::json_error(s, m).into_response();
    }
    if let Err((s, m)) = crate::validation::validate_pci_address(&address) {
        return crate::api_error::json_error(s, m).into_response();
    }
    let Some(qmp) = resolve_qmp(&state, &vm_name).await else {
        return not_available_response().into_response();
    };
    let device_id = pci_device_id(&address);
    match qmp.execute("device_del", serde_json::json!({"id": device_id})) {
        Ok(_) => Json(serde_json::json!({"status": "ok"})).into_response(),
        Err(e) => crate::api_error::json_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("PCI device_del failed: {}", e),
        )
        .into_response(),
    }
}
