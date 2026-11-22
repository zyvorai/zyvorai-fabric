// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: LicenseRef-Zyvor-Production-2.0

//! Boot, display, and CPU-model configuration for a VM.
//!
//! These are declarative preferences a user sets in the Create VM wizard's
//! Advanced Options (or later from a VM's own settings) -- persisted here so
//! the API round-trips real data instead of erroring, which is what the
//! wizard was hitting before these handlers existed (every VM created with
//! Advanced Options open failed with "advanced options could not be
//! applied" because no backend route existed for any of these three paths).

use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use security::{RequireRead, RequireWrite};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

use crate::server::AppState;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BootConfig {
    #[serde(default)]
    pub boot_order: Vec<String>,
    #[serde(default = "default_firmware")]
    pub firmware: String,
    #[serde(default)]
    pub secure_boot: bool,
    #[serde(default)]
    pub kernel: Option<String>,
    #[serde(default)]
    pub initrd: Option<String>,
    #[serde(default)]
    pub kernel_args: Option<String>,
}

fn default_firmware() -> String {
    "bios".to_string()
}

impl Default for BootConfig {
    fn default() -> Self {
        Self {
            boot_order: vec!["disk".to_string()],
            firmware: default_firmware(),
            secure_boot: false,
            kernel: None,
            initrd: None,
            kernel_args: None,
        }
    }
}

#[derive(Debug, Deserialize)]
pub struct BootConfigPatch {
    pub boot_order: Option<Vec<String>>,
    pub firmware: Option<String>,
    pub secure_boot: Option<bool>,
    pub kernel: Option<String>,
    pub initrd: Option<String>,
    pub kernel_args: Option<String>,
}

/// GET /api/vms/:name/boot
pub async fn get_boot_config(
    RequireRead(_claims): RequireRead,
    State(state): State<Arc<AppState>>,
    Path(vm_name): Path<String>,
) -> impl IntoResponse {
    if let Err((s, m)) = crate::validation::validate_vm_name(&vm_name) {
        return crate::api_error::json_error(s, m).into_response();
    }
    match state
        .store
        .get_entity::<BootConfig>("vm_boot_config", &vm_name)
    {
        Ok(Some(cfg)) => Json(cfg).into_response(),
        Ok(None) => Json(BootConfig::default()).into_response(),
        Err(e) => crate::api_error::json_error(StatusCode::INTERNAL_SERVER_ERROR, e.to_string())
            .into_response(),
    }
}

/// POST /api/vms/:name/boot
pub async fn update_boot_config(
    RequireWrite(_claims): RequireWrite,
    State(state): State<Arc<AppState>>,
    Path(vm_name): Path<String>,
    Json(patch): Json<BootConfigPatch>,
) -> impl IntoResponse {
    if let Err((s, m)) = crate::validation::validate_vm_name(&vm_name) {
        return crate::api_error::json_error(s, m).into_response();
    }
    if state.store.get_vm(&vm_name).ok().flatten().is_none() {
        return crate::api_error::json_error(
            StatusCode::NOT_FOUND,
            format!("VM '{}' not found", vm_name),
        )
        .into_response();
    }
    let mut cfg = state
        .store
        .get_entity::<BootConfig>("vm_boot_config", &vm_name)
        .ok()
        .flatten()
        .unwrap_or_default();
    if let Some(v) = patch.boot_order {
        cfg.boot_order = v;
    }
    if let Some(v) = patch.firmware {
        cfg.firmware = v;
    }
    if let Some(v) = patch.secure_boot {
        cfg.secure_boot = v;
    }
    if patch.kernel.is_some() {
        cfg.kernel = patch.kernel;
    }
    if patch.initrd.is_some() {
        cfg.initrd = patch.initrd;
    }
    if patch.kernel_args.is_some() {
        cfg.kernel_args = patch.kernel_args;
    }
    if let Err(e) = state.store.save_entity("vm_boot_config", &vm_name, &cfg) {
        return crate::api_error::json_error(StatusCode::INTERNAL_SERVER_ERROR, e.to_string())
            .into_response();
    }
    (StatusCode::OK, Json(cfg)).into_response()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DisplayConfig {
    #[serde(rename = "type", default = "default_display_type")]
    pub display_type: String,
    #[serde(default = "default_listen")]
    pub listen_address: String,
    #[serde(default)]
    pub port: u16,
    #[serde(default)]
    pub tls_port: Option<u16>,
    #[serde(default)]
    pub password: Option<String>,
    #[serde(default)]
    pub keymap: Option<String>,
}

fn default_display_type() -> String {
    "vnc".to_string()
}
fn default_listen() -> String {
    "127.0.0.1".to_string()
}

impl Default for DisplayConfig {
    fn default() -> Self {
        Self {
            display_type: default_display_type(),
            listen_address: default_listen(),
            port: 0,
            tls_port: None,
            password: None,
            keymap: None,
        }
    }
}

#[derive(Debug, Deserialize)]
pub struct DisplayConfigPatch {
    #[serde(rename = "type")]
    pub display_type: Option<String>,
    pub listen_address: Option<String>,
    pub port: Option<u16>,
    pub tls_port: Option<u16>,
    pub password: Option<String>,
    pub keymap: Option<String>,
}

/// GET /api/vms/:name/display
pub async fn get_display(
    RequireRead(_claims): RequireRead,
    State(state): State<Arc<AppState>>,
    Path(vm_name): Path<String>,
) -> impl IntoResponse {
    if let Err((s, m)) = crate::validation::validate_vm_name(&vm_name) {
        return crate::api_error::json_error(s, m).into_response();
    }
    match state
        .store
        .get_entity::<DisplayConfig>("vm_display_config", &vm_name)
    {
        Ok(Some(cfg)) => Json(cfg).into_response(),
        Ok(None) => Json(DisplayConfig::default()).into_response(),
        Err(e) => crate::api_error::json_error(StatusCode::INTERNAL_SERVER_ERROR, e.to_string())
            .into_response(),
    }
}

/// POST /api/vms/:name/display
pub async fn update_display(
    RequireWrite(_claims): RequireWrite,
    State(state): State<Arc<AppState>>,
    Path(vm_name): Path<String>,
    Json(patch): Json<DisplayConfigPatch>,
) -> impl IntoResponse {
    if let Err((s, m)) = crate::validation::validate_vm_name(&vm_name) {
        return crate::api_error::json_error(s, m).into_response();
    }
    if state.store.get_vm(&vm_name).ok().flatten().is_none() {
        return crate::api_error::json_error(
            StatusCode::NOT_FOUND,
            format!("VM '{}' not found", vm_name),
        )
        .into_response();
    }
    let mut cfg = state
        .store
        .get_entity::<DisplayConfig>("vm_display_config", &vm_name)
        .ok()
        .flatten()
        .unwrap_or_default();
    if let Some(v) = patch.display_type {
        cfg.display_type = v;
    }
    if let Some(v) = patch.listen_address {
        cfg.listen_address = v;
    }
    if let Some(v) = patch.port {
        cfg.port = v;
    }
    if patch.tls_port.is_some() {
        cfg.tls_port = patch.tls_port;
    }
    if patch.password.is_some() {
        cfg.password = patch.password;
    }
    if patch.keymap.is_some() {
        cfg.keymap = patch.keymap;
    }
    if let Err(e) = state.store.save_entity("vm_display_config", &vm_name, &cfg) {
        return crate::api_error::json_error(StatusCode::INTERNAL_SERVER_ERROR, e.to_string())
            .into_response();
    }
    (StatusCode::OK, Json(cfg)).into_response()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CpuModelConfig {
    #[serde(default = "default_cpu_model")]
    pub model: String,
    #[serde(default = "default_cpu_mode")]
    pub mode: String,
    #[serde(default)]
    pub features_add: Vec<String>,
    #[serde(default)]
    pub features_remove: Vec<String>,
    #[serde(default)]
    pub vendor: Option<String>,
}

fn default_cpu_model() -> String {
    "qemu64".to_string()
}
fn default_cpu_mode() -> String {
    "host-model".to_string()
}

impl Default for CpuModelConfig {
    fn default() -> Self {
        Self {
            model: default_cpu_model(),
            mode: default_cpu_mode(),
            features_add: Vec::new(),
            features_remove: Vec::new(),
            vendor: None,
        }
    }
}

#[derive(Debug, Deserialize)]
pub struct CpuModelConfigPatch {
    pub model: Option<String>,
    pub mode: Option<String>,
    pub features_add: Option<Vec<String>>,
    pub features_remove: Option<Vec<String>>,
    pub vendor: Option<String>,
}

const ALLOWED_CPU_MODES: &[&str] = &["custom", "host-model", "host-passthrough"];

/// GET /api/vms/:name/cpu-model
pub async fn get_cpu_config(
    RequireRead(_claims): RequireRead,
    State(state): State<Arc<AppState>>,
    Path(vm_name): Path<String>,
) -> impl IntoResponse {
    if let Err((s, m)) = crate::validation::validate_vm_name(&vm_name) {
        return crate::api_error::json_error(s, m).into_response();
    }
    match state
        .store
        .get_entity::<CpuModelConfig>("vm_cpu_config", &vm_name)
    {
        Ok(Some(cfg)) => Json(cfg).into_response(),
        Ok(None) => Json(CpuModelConfig::default()).into_response(),
        Err(e) => crate::api_error::json_error(StatusCode::INTERNAL_SERVER_ERROR, e.to_string())
            .into_response(),
    }
}

/// POST /api/vms/:name/cpu-model
pub async fn update_cpu_config(
    RequireWrite(_claims): RequireWrite,
    State(state): State<Arc<AppState>>,
    Path(vm_name): Path<String>,
    Json(patch): Json<CpuModelConfigPatch>,
) -> impl IntoResponse {
    if let Err((s, m)) = crate::validation::validate_vm_name(&vm_name) {
        return crate::api_error::json_error(s, m).into_response();
    }
    if state.store.get_vm(&vm_name).ok().flatten().is_none() {
        return crate::api_error::json_error(
            StatusCode::NOT_FOUND,
            format!("VM '{}' not found", vm_name),
        )
        .into_response();
    }
    if let Some(mode) = &patch.mode {
        if !ALLOWED_CPU_MODES.contains(&mode.as_str()) {
            return crate::api_error::json_error(
                StatusCode::BAD_REQUEST,
                format!(
                    "Invalid CPU mode '{}'. Allowed: {}",
                    mode,
                    ALLOWED_CPU_MODES.join(", ")
                ),
            )
            .into_response();
        }
    }
    let mut cfg = state
        .store
        .get_entity::<CpuModelConfig>("vm_cpu_config", &vm_name)
        .ok()
        .flatten()
        .unwrap_or_default();
    if let Some(v) = patch.model {
        cfg.model = v;
    }
    if let Some(v) = patch.mode {
        cfg.mode = v;
    }
    if let Some(v) = patch.features_add {
        cfg.features_add = v;
    }
    if let Some(v) = patch.features_remove {
        cfg.features_remove = v;
    }
    if patch.vendor.is_some() {
        cfg.vendor = patch.vendor;
    }
    if let Err(e) = state.store.save_entity("vm_cpu_config", &vm_name, &cfg) {
        return crate::api_error::json_error(StatusCode::INTERNAL_SERVER_ERROR, e.to_string())
            .into_response();
    }
    (StatusCode::OK, Json(cfg)).into_response()
}

#[derive(Debug, Serialize)]
pub struct CpuModelInfo {
    pub name: String,
    pub vendor: String,
    pub features: Vec<String>,
}

fn guess_vendor(model: &str) -> String {
    let m = model.to_ascii_lowercase();
    if m.contains("epyc") || m.contains("amd") || m.contains("athlon") || m.contains("phenom") {
        "AMD".to_string()
    } else if m.contains("intel")
        || m.contains("core")
        || m.contains("xeon")
        || m.contains("broadwell")
        || m.contains("haswell")
        || m.contains("skylake")
        || m.contains("cascadelake")
        || m.contains("icelake")
        || m.contains("snowridge")
        || m.contains("westmere")
        || m.contains("nehalem")
        || m.contains("sandybridge")
        || m.contains("ivybridge")
        || m.contains("penryn")
        || m.contains("conroe")
    {
        "Intel".to_string()
    } else {
        "".to_string()
    }
}

/// GET /api/system/cpu-models - List QEMU CPU models available on this host
pub async fn list_cpu_models(
    RequireRead(_claims): RequireRead,
) -> Result<Json<Vec<CpuModelInfo>>, (StatusCode, Json<serde_json::Value>)> {
    let output = tokio::process::Command::new("qemu-system-x86_64")
        .args(["-cpu", "help"])
        .output()
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"error": format!("Failed to run qemu-system-x86_64: {}", e)})),
            )
        })?;

    let stdout = String::from_utf8_lossy(&output.stdout);
    let mut models = Vec::new();
    let mut in_list = false;
    for line in stdout.lines() {
        if line.starts_with("Available CPUs") {
            in_list = true;
            continue;
        }
        if !in_list {
            continue;
        }
        if line.trim().is_empty() || line.starts_with("Recognized CPUID") {
            break;
        }
        // "x86 <model>[  <description>]"
        let Some(rest) = line.trim_start().strip_prefix("x86 ") else {
            continue;
        };
        let name = rest.split_whitespace().next().unwrap_or("").to_string();
        if name.is_empty() {
            continue;
        }
        models.push(CpuModelInfo {
            vendor: guess_vendor(&name),
            name,
            // qemu -cpu help doesn't enumerate per-model feature flags without
            // querying each model individually (expensive); left empty rather
            // than guessed.
            features: Vec::new(),
        });
    }

    Ok(Json(models))
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WatchdogConfig {
    pub model: String,
    pub action: String,
}

const ALLOWED_WATCHDOG_MODELS: &[&str] = &["i6300esb", "ib700"];
const ALLOWED_WATCHDOG_ACTIONS: &[&str] = &["reset", "shutdown", "poweroff", "pause", "none"];

/// GET /api/vms/:name/watchdog - null if no watchdog configured
pub async fn get_watchdog(
    RequireRead(_claims): RequireRead,
    State(state): State<Arc<AppState>>,
    Path(vm_name): Path<String>,
) -> impl IntoResponse {
    if let Err((s, m)) = crate::validation::validate_vm_name(&vm_name) {
        return crate::api_error::json_error(s, m).into_response();
    }
    match state
        .store
        .get_entity::<WatchdogConfig>("vm_watchdog_config", &vm_name)
    {
        Ok(cfg) => Json(cfg).into_response(),
        Err(e) => crate::api_error::json_error(StatusCode::INTERNAL_SERVER_ERROR, e.to_string())
            .into_response(),
    }
}

/// POST /api/vms/:name/watchdog
pub async fn set_watchdog(
    RequireWrite(_claims): RequireWrite,
    State(state): State<Arc<AppState>>,
    Path(vm_name): Path<String>,
    Json(cfg): Json<WatchdogConfig>,
) -> impl IntoResponse {
    if let Err((s, m)) = crate::validation::validate_vm_name(&vm_name) {
        return crate::api_error::json_error(s, m).into_response();
    }
    if state.store.get_vm(&vm_name).ok().flatten().is_none() {
        return crate::api_error::json_error(
            StatusCode::NOT_FOUND,
            format!("VM '{}' not found", vm_name),
        )
        .into_response();
    }
    if !ALLOWED_WATCHDOG_MODELS.contains(&cfg.model.as_str()) {
        return crate::api_error::json_error(
            StatusCode::BAD_REQUEST,
            format!(
                "Invalid watchdog model '{}'. Allowed: {}",
                cfg.model,
                ALLOWED_WATCHDOG_MODELS.join(", ")
            ),
        )
        .into_response();
    }
    if !ALLOWED_WATCHDOG_ACTIONS.contains(&cfg.action.as_str()) {
        return crate::api_error::json_error(
            StatusCode::BAD_REQUEST,
            format!(
                "Invalid watchdog action '{}'. Allowed: {}",
                cfg.action,
                ALLOWED_WATCHDOG_ACTIONS.join(", ")
            ),
        )
        .into_response();
    }
    if let Err(e) = state
        .store
        .save_entity("vm_watchdog_config", &vm_name, &cfg)
    {
        return crate::api_error::json_error(StatusCode::INTERNAL_SERVER_ERROR, e.to_string())
            .into_response();
    }
    (StatusCode::OK, Json(cfg)).into_response()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SerialConfig {
    #[serde(rename = "type")]
    pub serial_type: String,
    #[serde(default)]
    pub path: Option<String>,
    #[serde(default)]
    pub source_host: Option<String>,
    #[serde(default)]
    pub source_port: Option<u16>,
}

const ALLOWED_SERIAL_TYPES: &[&str] = &["pty", "unix", "tcp"];

/// GET /api/vms/:name/serials
pub async fn list_serials(
    RequireRead(_claims): RequireRead,
    State(state): State<Arc<AppState>>,
    Path(vm_name): Path<String>,
) -> impl IntoResponse {
    if let Err((s, m)) = crate::validation::validate_vm_name(&vm_name) {
        return crate::api_error::json_error(s, m).into_response();
    }
    match state
        .store
        .get_entity::<Vec<SerialConfig>>("vm_serial_config", &vm_name)
    {
        Ok(cfg) => Json(cfg.unwrap_or_default()).into_response(),
        Err(e) => crate::api_error::json_error(StatusCode::INTERNAL_SERVER_ERROR, e.to_string())
            .into_response(),
    }
}

/// POST /api/vms/:name/serials - append a serial console definition
pub async fn add_serial(
    RequireWrite(_claims): RequireWrite,
    State(state): State<Arc<AppState>>,
    Path(vm_name): Path<String>,
    Json(cfg): Json<SerialConfig>,
) -> impl IntoResponse {
    if let Err((s, m)) = crate::validation::validate_vm_name(&vm_name) {
        return crate::api_error::json_error(s, m).into_response();
    }
    if state.store.get_vm(&vm_name).ok().flatten().is_none() {
        return crate::api_error::json_error(
            StatusCode::NOT_FOUND,
            format!("VM '{}' not found", vm_name),
        )
        .into_response();
    }
    if !ALLOWED_SERIAL_TYPES.contains(&cfg.serial_type.as_str()) {
        return crate::api_error::json_error(
            StatusCode::BAD_REQUEST,
            format!(
                "Invalid serial type '{}'. Allowed: {}",
                cfg.serial_type,
                ALLOWED_SERIAL_TYPES.join(", ")
            ),
        )
        .into_response();
    }
    let mut serials = state
        .store
        .get_entity::<Vec<SerialConfig>>("vm_serial_config", &vm_name)
        .ok()
        .flatten()
        .unwrap_or_default();
    serials.push(cfg);
    if let Err(e) = state
        .store
        .save_entity("vm_serial_config", &vm_name, &serials)
    {
        return crate::api_error::json_error(StatusCode::INTERNAL_SERVER_ERROR, e.to_string())
            .into_response();
    }
    (StatusCode::OK, Json(serials)).into_response()
}
