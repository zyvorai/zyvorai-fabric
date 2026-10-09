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
use zyvor_fabric_fluxvm_client::{BalloonStatus, VmMemory};

use super::qga::{fluxvm_client, resolve_id};
use crate::server::AppState;
use crate::validation::validate_vm_name;
use security::{RequireAdmin, RequireRead};

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
