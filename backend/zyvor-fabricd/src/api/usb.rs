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
use std::sync::Arc;

use super::hotplug::{not_available_response, resolve_qmp};
use crate::server::AppState;

#[derive(Debug, Serialize)]
pub struct UsbDevice {
    pub bus: u32,
    pub device: u32,
    pub vendor_id: String,
    pub product_id: String,
    pub vendor_name: String,
    pub product_name: String,
    pub speed: String,
    pub attached_to: Option<String>,
}

/// GET /api/system/usb-devices - List USB devices available for passthrough
pub async fn list_usb_devices(
    RequireRead(_claims): RequireRead,
) -> Result<Json<Vec<UsbDevice>>, (StatusCode, Json<serde_json::Value>)> {
    let output = tokio::process::Command::new("lsusb")
        .output()
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"error": format!("Failed to run lsusb: {}", e)})),
            )
        })?;

    let stdout = String::from_utf8_lossy(&output.stdout);
    let devices: Vec<UsbDevice> = stdout
        .lines()
        .filter_map(|line| {
            // "Bus 001 Device 002: ID 1234:5678 Vendor Product Description"
            let parts: Vec<&str> = line.splitn(7, ' ').collect();
            if parts.len() < 6 {
                return None;
            }
            let bus: u32 = parts[1].parse().ok()?;
            let device: u32 = parts[3].trim_end_matches(':').parse().ok()?;
            let id_part = parts[5].trim_end_matches(':');
            let (vendor_id, product_id) = id_part.split_once(':')?;
            let description = parts.get(6).unwrap_or(&"").trim();
            let (vendor_name, product_name) = match description.split_once(' ') {
                Some((v, p)) if !p.is_empty() => (v.to_string(), p.to_string()),
                _ => (description.to_string(), String::new()),
            };
            // lsusb doesn't report negotiated link speed; leave unknown
            // rather than guessing from bcdUSB, which `-v` would require.
            Some(UsbDevice {
                bus,
                device,
                vendor_id: vendor_id.to_string(),
                product_id: product_id.to_string(),
                vendor_name,
                product_name,
                speed: "unknown".to_string(),
                attached_to: None,
            })
        })
        .collect();

    Ok(Json(devices))
}

#[derive(Debug, Deserialize)]
pub struct UsbAttachRequest {
    pub vendor_id: String,
    pub product_id: String,
}

fn usb_device_id(vendor_id: &str, product_id: &str) -> String {
    format!("usb-hotplug-{}-{}", vendor_id, product_id)
}

/// USB vendor/product ids are 4 hex digits (e.g. "1d6b"). Reject anything
/// else before it's embedded in a QMP command or a device id string.
fn is_usb_id(s: &str) -> bool {
    s.len() == 4 && s.chars().all(|c| c.is_ascii_hexdigit())
}

/// POST /api/vms/:name/devices/usb - Attach a host USB device to a running VM
pub async fn attach_usb(
    RequireWrite(_claims): RequireWrite,
    State(state): State<Arc<AppState>>,
    Path(vm_name): Path<String>,
    Json(req): Json<UsbAttachRequest>,
) -> impl IntoResponse {
    if let Err((s, m)) = crate::validation::validate_vm_name(&vm_name) {
        return crate::api_error::json_error(s, m).into_response();
    }
    if !is_usb_id(&req.vendor_id) || !is_usb_id(&req.product_id) {
        return crate::api_error::json_error(
            StatusCode::BAD_REQUEST,
            "vendor_id and product_id must each be 4 hex digits".to_string(),
        )
        .into_response();
    }
    let Some(qmp) = resolve_qmp(&state, &vm_name).await else {
        return not_available_response().into_response();
    };

    let device_id = usb_device_id(&req.vendor_id, &req.product_id);
    let device_args = serde_json::json!({
        "driver": "usb-host",
        "id": device_id,
        "vendorid": format!("0x{}", req.vendor_id),
        "productid": format!("0x{}", req.product_id),
    });

    match qmp.execute("device_add", device_args) {
        Ok(_) => Json(serde_json::json!({"status": "ok", "device_id": device_id})).into_response(),
        Err(e) => crate::api_error::json_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("USB device_add failed: {}", e),
        )
        .into_response(),
    }
}

/// DELETE /api/vms/:name/devices/usb/:vendor_id::product_id - Detach a USB device
pub async fn detach_usb(
    RequireWrite(_claims): RequireWrite,
    State(state): State<Arc<AppState>>,
    Path((vm_name, ids)): Path<(String, String)>,
) -> impl IntoResponse {
    if let Err((s, m)) = crate::validation::validate_vm_name(&vm_name) {
        return crate::api_error::json_error(s, m).into_response();
    }
    let Some((vendor_id, product_id)) = ids.split_once(':') else {
        return crate::api_error::json_error(
            StatusCode::BAD_REQUEST,
            "Expected id in 'vendor_id:product_id' form".to_string(),
        )
        .into_response();
    };
    let Some(qmp) = resolve_qmp(&state, &vm_name).await else {
        return not_available_response().into_response();
    };
    let device_id = usb_device_id(vendor_id, product_id);
    match qmp.execute("device_del", serde_json::json!({"id": device_id})) {
        Ok(_) => Json(serde_json::json!({"status": "ok"})).into_response(),
        Err(e) => crate::api_error::json_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("USB device_del failed: {}", e),
        )
        .into_response(),
    }
}
