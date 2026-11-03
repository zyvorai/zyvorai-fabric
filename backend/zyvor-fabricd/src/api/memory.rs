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

/// A balloon cannot take more memory than the VM has. FluxVM treats the balloon as Beta, so refuse the
/// impossible request here with a 400 rather than pass it on. 0 (deflate) is always fine.
fn check_balloon(requested_mib: u64, configured_mib: u64) -> Result<(), ApiErr> {
    if requested_mib > configured_mib {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(json!({
                "error": format!(
                    "balloon_mib {requested_mib} exceeds the VM's configured memory ({configured_mib} MiB)"
                )
            })),
        ));
    }
    Ok(())
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
    let configured = client
        .get_memory(id)
        .await
        .map_err(bad_gateway)?
        .configured_mib;
    check_balloon(body.balloon_mib, configured)?;
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

/// Reject a bad fork request before FluxVM sees it, so the caller gets a 400 and not a 502.
fn check_fork_body(body: &ForkBody) -> Result<(), ApiErr> {
    let bad = |msg: String| (StatusCode::BAD_REQUEST, Json(json!({ "error": msg })));
    if body.count == 0 || body.count > MAX_FORK_COUNT {
        return Err(bad(format!("count must be 1-{MAX_FORK_COUNT}")));
    }
    if let Some(prefix) = &body.name_prefix {
        checked(prefix)?;
    }
    if let Some(key) = &body.idempotency_key {
        zyvor_fabric_fluxvm_client::validate_idempotency_key(key)
            .map_err(|e| bad(e.to_string()))?;
    }
    Ok(())
}

/// POST /api/vms/{name}/live-fork  `{"count": N, "name_prefix": "...", "ready": true}`
///
/// Unlike `POST /api/vms/{name}/fork` (a disk copy-on-write clone), this forks the *running* VM's
/// memory. Needs the flux-vm backend on the KVM engine; anything else comes back from FluxVM as an error.
pub async fn live_fork_vm(
    RequireWrite(_claims): RequireWrite,
    State(state): State<Arc<AppState>>,
    Path(name): Path<String>,
    Json(body): Json<ForkBody>,
) -> Result<Json<serde_json::Value>, ApiErr> {
    checked(&name)?;
    check_fork_body(&body)?;
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

#[cfg(test)]
mod tests {
    use super::*;

    fn body(count: u32, prefix: Option<&str>, key: Option<&str>) -> ForkBody {
        ForkBody {
            count,
            name_prefix: prefix.map(Into::into),
            ready: false,
            idempotency_key: key.map(Into::into),
        }
    }

    #[test]
    fn balloon_cannot_exceed_the_vms_memory() {
        assert!(check_balloon(0, 1024).is_ok());
        assert!(check_balloon(1024, 1024).is_ok());
        let (status, _) = check_balloon(1025, 1024).unwrap_err();
        assert_eq!(status, StatusCode::BAD_REQUEST);
    }

    #[test]
    fn fork_request_is_validated_before_fluxvm() {
        assert!(check_fork_body(&body(1, None, None)).is_ok());
        assert!(check_fork_body(&body(MAX_FORK_COUNT, Some("kid"), Some("retry-1"))).is_ok());
        for bad in [
            body(0, None, None),
            body(MAX_FORK_COUNT + 1, None, None),
            body(1, Some("bad name!"), None),
            body(1, None, Some("")),
            body(1, None, Some("has space")),
        ] {
            let (status, _) = check_fork_body(&bad).unwrap_err();
            assert_eq!(status, StatusCode::BAD_REQUEST);
        }
    }
}
