// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: Apache-2.0

//! Memory density proxies: VMM-process PSS and the balloon (Beta: FluxVM lists balloon as
//! unit-tested, not live-verified; it needs a running VM on the flux-vm KVM engine).

use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use serde::Deserialize;
use serde_json::json;
use std::sync::Arc;
use zyvor_fabric_fluxvm_client::{BalloonStatus, ForkVmRequest, ReadyOptions, VmMemory};

use super::qga::{fluxvm_client, resolve_id};
use crate::server::AppState;
use crate::validation::validate_vm_name;
use security::{RequireAdmin, RequireRead, RequireWrite};

type ApiErr = (StatusCode, Json<serde_json::Value>);

fn bad_gateway(e: impl std::fmt::Display) -> ApiErr {
    (
        StatusCode::BAD_GATEWAY,
        Json(json!({ "error": e.to_string() })),
    )
}

fn checked(name: &str) -> Result<(), ApiErr> {
    validate_vm_name(name)
        .map_err(|(_s, msg)| (StatusCode::BAD_REQUEST, Json(json!({ "error": msg }))))
}

/// GET /api/vms/{name}/memory
pub async fn get_memory(
    RequireRead(_claims): RequireRead,
    State(state): State<Arc<AppState>>,
    Path(name): Path<String>,
) -> Result<Json<VmMemory>, ApiErr> {
    checked(&name)?;
    let client = fluxvm_client(&state)?;
    let id = resolve_id(&client, &name).await?;
    client.get_memory(id).await.map(Json).map_err(bad_gateway)
}

/// GET /api/vms/{name}/balloon
pub async fn get_balloon(
    RequireRead(_claims): RequireRead,
    State(state): State<Arc<AppState>>,
    Path(name): Path<String>,
) -> Result<Json<BalloonStatus>, ApiErr> {
    checked(&name)?;
    let client = fluxvm_client(&state)?;
    let id = resolve_id(&client, &name).await?;
    client.get_balloon(id).await.map(Json).map_err(bad_gateway)
}

#[derive(Debug, Deserialize)]
pub struct SetBalloonBody {
    pub balloon_mib: u64,
}

/// POST /api/vms/{name}/balloon  `{"balloon_mib": N}` (0 deflates)
pub async fn set_balloon(
    RequireAdmin(_claims): RequireAdmin,
    State(state): State<Arc<AppState>>,
    Path(name): Path<String>,
    Json(body): Json<SetBalloonBody>,
) -> Result<Json<BalloonStatus>, ApiErr> {
    checked(&name)?;
    let client = fluxvm_client(&state)?;
    let id = resolve_id(&client, &name).await?;
    client
        .set_balloon(id, body.balloon_mib)
        .await
        .map(Json)
        .map_err(bad_gateway)
}

#[derive(Debug, Deserialize)]
pub struct ForkBody {
    #[serde(default = "one")]
    pub count: u32,
    #[serde(default)]
    pub name_prefix: Option<String>,
    /// Wait for a first guest command in every child and report how long it took.
    #[serde(default)]
    pub ready: bool,
    /// Sent to FluxVM as `Idempotency-Key`, so a retry returns the same children.
    #[serde(default)]
    pub idempotency_key: Option<String>,
}

fn one() -> u32 {
    1
}

/// Children one fork may start. FluxVM enforces the same limit; checking here gives a clear 400.
pub const MAX_FORK_COUNT: u32 = 32;

/// POST /api/vms/{name}/fork  `{"count": N, "name_prefix": "...", "ready": true}`
///
/// Needs the flux-vm backend on the KVM engine; anything else comes back from FluxVM as an error.
pub async fn fork_vm(
    RequireWrite(_claims): RequireWrite,
    State(state): State<Arc<AppState>>,
    Path(name): Path<String>,
    Json(body): Json<ForkBody>,
) -> Result<Json<serde_json::Value>, ApiErr> {
    checked(&name)?;
    if body.count == 0 || body.count > MAX_FORK_COUNT {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": format!("count must be 1-{MAX_FORK_COUNT}") })),
        ));
    }
    if let Some(prefix) = &body.name_prefix {
        checked(prefix)?;
    }
    let client = fluxvm_client(&state)?;
    let id = resolve_id(&client, &name).await?;
    let out = client
        .fork_vm(
            id,
            &ForkVmRequest {
                count: body.count,
                name_prefix: body.name_prefix.clone(),
            },
            &ReadyOptions {
                ready_exec: body.ready,
                idempotency_key: body.idempotency_key.as_deref(),
            },
        )
        .await
        .map_err(bad_gateway)?;
    Ok(Json(json!({
        "parent": name,
        "children": out.items.iter().map(|v| json!({ "id": v.id, "name": v.name })).collect::<Vec<_>>(),
        "elapsed_ms": out.elapsed_ms,
        "first_command_ms": out.first_command_ms,
    })))
}
