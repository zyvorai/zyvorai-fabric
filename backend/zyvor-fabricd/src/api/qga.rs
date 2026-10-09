// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: Apache-2.0

//! Thin QEMU Guest Agent proxies for Windows Kryton VMs.

use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use serde::Deserialize;
use serde_json::json;
use std::sync::Arc;
use zyvor_fabric_fluxvm_client::{
    FluxVmClient, QgaExecRequest, QgaExecResult, QgaFirewallCloseRequest, QgaFirewallOpenRequest,
};

use crate::server::AppState;
use crate::validation::validate_vm_name;
use security::{RequireAdmin, RequireRead, RequireWrite};

pub(crate) fn fluxvm_client(
    state: &AppState,
) -> Result<FluxVmClient, (StatusCode, Json<serde_json::Value>)> {
    let client = FluxVmClient::new(&state.config.driver.fluxvm_url).map_err(|e| {
        (
            StatusCode::BAD_GATEWAY,
            Json(json!({ "error": format!("FluxVM client: {e}") })),
        )
    })?;
    Ok(match state.config.driver.fluxvm_token.as_deref() {
        Some(t) if !t.is_empty() => client.with_token(t.to_owned()),
        _ => client,
    })
}

pub(crate) async fn resolve_id(
    client: &FluxVmClient,
    name: &str,
) -> Result<uuid::Uuid, (StatusCode, Json<serde_json::Value>)> {
    client
        .find_by_name(name)
        .await
        .map_err(|e| {
            (
                StatusCode::BAD_GATEWAY,
                Json(json!({ "error": e.to_string() })),
            )
        })?
        .map(|vm| vm.id)
        .ok_or_else(|| {
            (
                StatusCode::NOT_FOUND,
                Json(json!({ "error": format!("VM '{name}' not found on FluxVM") })),
            )
        })
}

/// POST /api/vms/{name}/qga/ping
pub async fn qga_ping(
    RequireRead(_claims): RequireRead,
    State(state): State<Arc<AppState>>,
    Path(name): Path<String>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    validate_vm_name(&name)
        .map_err(|(_s, msg)| (StatusCode::BAD_REQUEST, Json(json!({ "error": msg }))))?;
    let client = fluxvm_client(&state)?;
    let id = resolve_id(&client, &name).await?;
    client.qga_ping(id).await.map_err(|e| {
        (
            StatusCode::BAD_GATEWAY,
            Json(json!({ "error": e.to_string() })),
        )
    })?;
    Ok(Json(json!({ "ok": true })))
}

#[derive(Debug, Deserialize)]
pub struct QgaExecBody {
    #[serde(default)]
    pub path: Option<String>,
    #[serde(default)]
    pub args: Vec<String>,
    #[serde(default)]
    pub powershell: Option<String>,
    #[serde(default)]
    pub timeout_seconds: Option<u64>,
}

/// POST /api/vms/{name}/qga/exec
pub async fn qga_exec(
    RequireWrite(_claims): RequireWrite,
    State(state): State<Arc<AppState>>,
    Path(name): Path<String>,
    Json(body): Json<QgaExecBody>,
) -> Result<Json<QgaExecResult>, (StatusCode, Json<serde_json::Value>)> {
    validate_vm_name(&name)
        .map_err(|(_s, msg)| (StatusCode::BAD_REQUEST, Json(json!({ "error": msg }))))?;
    let client = fluxvm_client(&state)?;
    let id = resolve_id(&client, &name).await?;
    let result = client
        .qga_exec(
            id,
            &QgaExecRequest {
                path: body.path,
                args: body.args,
                powershell: body.powershell,
                timeout_seconds: body.timeout_seconds,
            },
        )
        .await
        .map_err(|e| {
            (
                StatusCode::BAD_GATEWAY,
                Json(json!({ "error": e.to_string() })),
            )
        })?;
    Ok(Json(result))
}

#[derive(Debug, Deserialize)]
pub struct QgaFirewallOpenBody {
    pub name: String,
    pub port: u16,
    #[serde(default = "default_tcp")]
    pub protocol: String,
    #[serde(default)]
    pub timeout_seconds: Option<u64>,
}
fn default_tcp() -> String {
    "tcp".into()
}

/// POST /api/vms/{name}/qga/firewall/open
pub async fn qga_firewall_open(
    RequireAdmin(_claims): RequireAdmin,
    State(state): State<Arc<AppState>>,
    Path(name): Path<String>,
    Json(body): Json<QgaFirewallOpenBody>,
) -> Result<Json<QgaExecResult>, (StatusCode, Json<serde_json::Value>)> {
    validate_vm_name(&name)
        .map_err(|(_s, msg)| (StatusCode::BAD_REQUEST, Json(json!({ "error": msg }))))?;
    let client = fluxvm_client(&state)?;
    let id = resolve_id(&client, &name).await?;
    let result = client
        .qga_firewall_open(
            id,
            &QgaFirewallOpenRequest {
                name: body.name,
                port: body.port,
                protocol: body.protocol,
                timeout_seconds: body.timeout_seconds,
            },
        )
        .await
        .map_err(|e| {
            (
                StatusCode::BAD_GATEWAY,
                Json(json!({ "error": e.to_string() })),
            )
        })?;
    Ok(Json(result))
}

#[derive(Debug, Deserialize)]
pub struct QgaFirewallCloseBody {
    pub name: String,
    #[serde(default)]
    pub timeout_seconds: Option<u64>,
}

/// POST /api/vms/{name}/qga/firewall/close
pub async fn qga_firewall_close(
    RequireAdmin(_claims): RequireAdmin,
    State(state): State<Arc<AppState>>,
    Path(name): Path<String>,
    Json(body): Json<QgaFirewallCloseBody>,
) -> Result<Json<QgaExecResult>, (StatusCode, Json<serde_json::Value>)> {
    validate_vm_name(&name)
        .map_err(|(_s, msg)| (StatusCode::BAD_REQUEST, Json(json!({ "error": msg }))))?;
    let client = fluxvm_client(&state)?;
    let id = resolve_id(&client, &name).await?;
    let result = client
        .qga_firewall_close(
            id,
            &QgaFirewallCloseRequest {
                name: body.name,
                timeout_seconds: body.timeout_seconds,
            },
        )
        .await
        .map_err(|e| {
            (
                StatusCode::BAD_GATEWAY,
                Json(json!({ "error": e.to_string() })),
            )
        })?;
    Ok(Json(result))
}
