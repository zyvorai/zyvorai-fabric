// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: LicenseRef-Zyvor-Production-1.0

//! Offline guest configuration via GuestKit's `rescue`/`inspect` commands
//! (mounts the VM's disk directly, no in-guest agent or network needed) --
//! this project uses GuestKit for guest/image work, not qemu-guest-agent.
//! The VM must be stopped: guestkit needs exclusive access to the disk,
//! and a running QEMU process already holds a write lock on it (found
//! live while wiring golden images -- "Failed to get shared 'write' lock").
//!
//! Note: GuestKit's static-IP support (--ip/--mask/--gateway) is Windows-
//! only as of this integration -- there's no equivalent Linux rescue
//! operation, so this deliberately does not expose an IP/gateway option
//! that would silently no-op on a Linux guest.
//!
//! Also: `inject-ssh-key` and `reset-password` both require the target
//! user to already exist in the image (confirmed live -- even `--force`
//! only adds a missing /etc/shadow entry for a user already in /etc/passwd,
//! it does not run useradd). There's no "create a new Linux user account"
//! rescue operation, so this deliberately doesn't offer one.

use axum::{extract::State, http::StatusCode, Json};
use serde::Deserialize;
use serde_json::json;
use std::sync::Arc;

use crate::server::AppState;
use crate::validation::validate_vm_name;
use security::RequireAdmin;

async fn stopped_disk_path(
    state: &Arc<AppState>,
    name: &str,
) -> Result<String, (StatusCode, Json<serde_json::Value>)> {
    validate_vm_name(name).map_err(|(s, m)| crate::api_error::json_error(s, m))?;
    let vm = state
        .store
        .get_vm(name)
        .map_err(|e| {
            crate::api_error::json_error(StatusCode::INTERNAL_SERVER_ERROR, e.to_string())
        })?
        .ok_or_else(|| {
            crate::api_error::json_error(StatusCode::NOT_FOUND, format!("VM '{}' not found", name))
        })?;
    if vm.state != vm_model::VMState::Stopped {
        return Err(crate::api_error::json_error(
            StatusCode::CONFLICT,
            format!(
                "VM '{}' must be stopped first -- GuestKit needs exclusive access to the disk, which a running VM already holds",
                name
            ),
        ));
    }
    let path = state.driver.get_disk_path(name).await.map_err(|e| {
        crate::api_error::json_error(
            StatusCode::NOT_FOUND,
            format!("No disk image found for VM '{}': {}", name, e),
        )
    })?;
    Ok(path.display().to_string())
}

async fn run_guestkit(args: &[&str]) -> Result<String, String> {
    let output = tokio::process::Command::new("guestkit")
        .args(args)
        .output()
        .await
        .map_err(|e| format!("Failed to run guestkit: {}", e))?;
    if output.status.success() {
        Ok(String::from_utf8_lossy(&output.stdout).to_string())
    } else {
        Err(String::from_utf8_lossy(&output.stderr).to_string())
    }
}

#[derive(Debug, Deserialize)]
#[serde(tag = "operation", rename_all = "kebab-case")]
pub enum RescueRequest {
    InjectSshKey {
        user: String,
        key: String,
    },
    EnableSsh,
    SetHostname {
        hostname: String,
    },
    ResetPassword {
        user: String,
        password: String,
    },
    InstallPackages {
        packages: Vec<String>,
        #[serde(default)]
        network: bool,
    },
}

/// POST /api/vms/:name/rescue -- one-shot offline guest configuration via
/// GuestKit (VM must be stopped).
pub async fn rescue(
    RequireAdmin(_claims): RequireAdmin,
    State(state): State<Arc<AppState>>,
    axum::extract::Path(name): axum::extract::Path<String>,
    Json(req): Json<RescueRequest>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let disk = stopped_disk_path(&state, &name).await?;

    let result = match &req {
        RescueRequest::InjectSshKey { user, key } => {
            run_guestkit(&[
                "rescue",
                &disk,
                "-o",
                "inject-ssh-key",
                "--user",
                user,
                "--key",
                key,
                "-q",
            ])
            .await
        }
        RescueRequest::EnableSsh => {
            run_guestkit(&["rescue", &disk, "-o", "enable-ssh", "-q"]).await
        }
        RescueRequest::SetHostname { hostname } => {
            run_guestkit(&[
                "rescue",
                &disk,
                "-o",
                "set-hostname",
                "--hostname",
                hostname,
                "-q",
            ])
            .await
        }
        RescueRequest::ResetPassword { user, password } => {
            run_guestkit(&[
                "rescue",
                &disk,
                "-o",
                "reset-password",
                "--user",
                user,
                "--password",
                password,
                "-q",
            ])
            .await
        }
        RescueRequest::InstallPackages { packages, network } => {
            let pkg_list = packages.join(",");
            let mut args = vec![
                "rescue",
                disk.as_str(),
                "-o",
                "install-packages",
                "--packages",
                pkg_list.as_str(),
                "-q",
            ];
            if *network {
                args.push("--network");
            }
            run_guestkit(&args).await
        }
    };

    match result {
        Ok(_) => {
            tracing::info!("guestkit rescue succeeded for VM '{}'", name);
            Ok(Json(json!({ "status": "applied" })))
        }
        Err(e) => {
            tracing::warn!("guestkit rescue failed for VM '{}': {}", name, e);
            Err(crate::api_error::json_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                e,
            ))
        }
    }
}

/// GET /api/vms/:name/inspect -- OS/boot info about a stopped VM's disk,
/// pulled offline via GuestKit (no agent, no network, no boot required).
pub async fn inspect(
    RequireAdmin(_claims): RequireAdmin,
    State(state): State<Arc<AppState>>,
    axum::extract::Path(name): axum::extract::Path<String>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let disk = stopped_disk_path(&state, &name).await?;
    let output = tokio::process::Command::new("guestkit")
        .args(["inspect", &disk, "-o", "json", "-R", "-q"])
        .output()
        .await
        .map_err(|e| {
            crate::api_error::json_error(StatusCode::INTERNAL_SERVER_ERROR, e.to_string())
        })?;
    if !output.status.success() {
        return Err(crate::api_error::json_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            String::from_utf8_lossy(&output.stderr).to_string(),
        ));
    }
    serde_json::from_slice::<serde_json::Value>(&output.stdout)
        .map(Json)
        .map_err(|e| {
            crate::api_error::json_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Failed to parse guestkit output: {}", e),
            )
        })
}
