// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: LicenseRef-Zyvor-Production-2.0

use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::process::Command;
use uuid::Uuid;

use crate::server::AppState;
use security::{RequireAdmin, RequireRead, RequireWrite};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum SnapshotType {
    Disk,
    Full,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VMSnapshot {
    pub id: String,
    pub vm_name: String,
    pub name: String,
    pub description: Option<String>,
    pub snapshot_type: SnapshotType,
    pub parent_id: Option<String>,
    pub size_bytes: u64,
    pub created: DateTime<Utc>,
    /// Free-form tags. A snapshot tagged `keep` is never pruned by a
    /// snapshot policy.
    #[serde(default)]
    pub tags: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SnapshotTreeNode {
    pub snapshot: VMSnapshot,
    pub children: Vec<SnapshotTreeNode>,
}

#[derive(Debug, Deserialize)]
pub struct CreateSnapshotRequest {
    pub name: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default = "default_snapshot_type")]
    pub snapshot_type: SnapshotType,
    #[serde(default)]
    pub tags: Vec<String>,
}

fn default_snapshot_type() -> SnapshotType {
    SnapshotType::Disk
}

/// Whether a live-snapshot QMP error is a connect race (retryable).
pub(crate) fn is_qmp_connect_retryable(err: &str) -> bool {
    err.contains("Failed to connect to QMP socket")
        || err.contains("No such file")
        || err.contains("Connection refused")
        || err.contains("No such file or directory")
}

/// Poll until `path` is a connectable unix socket.
pub(crate) fn wait_for_qmp_connectable(
    path: &str,
    attempts: u32,
    delay_ms: u64,
) -> Result<(), String> {
    for i in 0..attempts {
        let qmp = crate::qmp::QmpClient::for_socket(path.to_string());
        if qmp.is_available() {
            match std::os::unix::net::UnixStream::connect(path) {
                Ok(_) => return Ok(()),
                Err(e) if i + 1 < attempts => {
                    tracing::debug!(
                        "QMP connect attempt {}/{} for {}: {}",
                        i + 1,
                        attempts,
                        path,
                        e
                    );
                    std::thread::sleep(std::time::Duration::from_millis(delay_ms));
                }
                Err(e) => {
                    return Err(format!("Failed to connect to QMP socket: {path}: {e}"));
                }
            }
        } else if i + 1 < attempts {
            std::thread::sleep(std::time::Duration::from_millis(delay_ms));
        } else {
            return Err(format!(
                "Failed to connect to QMP socket: {path}: not available"
            ));
        }
    }
    Err(format!("Failed to connect to QMP socket: {path}"))
}

/// Resolve the primary attached block node name from `query-block`.
fn primary_block_node(session: &mut crate::qmp::QmpSession) -> Result<String, String> {
    let blocks = session
        .execute("query-block", serde_json::Value::Null)
        .map_err(|e| e.to_string())?;
    blocks
        .as_array()
        .into_iter()
        .flatten()
        .find_map(|b| b.get("inserted")?.get("node-name")?.as_str())
        .ok_or_else(|| "no attached block device to snapshot".to_string())
        .map(|s| s.to_string())
}

/// Disk-only internal snapshot of a *running* VM via QMP
/// `blockdev-snapshot-internal-sync`. Fast (no vmstate dump) and is what
/// `snapshot_type=Disk` must use — previously every running-VM snapshot
/// went through `snapshot-save` (full memory), so "Disk Only" in the UI
/// still timed out under host load / the 60s HTTP TimeoutLayer.
pub(crate) fn live_disk_snapshot_via_qmp(
    qmp: &crate::qmp::QmpClient,
    tag: &str,
) -> Result<(), String> {
    let mut session = qmp.open_session().map_err(|e| e.to_string())?;
    let node_name = primary_block_node(&mut session)?;
    session
        .execute(
            "blockdev-snapshot-internal-sync",
            serde_json::json!({
                "device": node_name,
                "name": tag,
            }),
        )
        .map_err(|e| e.to_string())?;
    Ok(())
}

/// Delete an internal qcow2 snapshot tag on a *running* VM (disk held open
/// by QEMU — external `qemu-img snapshot -d` cannot take the image lock).
pub(crate) fn live_delete_snapshot_via_qmp(
    qmp: &crate::qmp::QmpClient,
    tag: &str,
) -> Result<(), String> {
    let mut session = qmp.open_session().map_err(|e| e.to_string())?;
    let node_name = primary_block_node(&mut session)?;
    session
        .execute(
            "blockdev-snapshot-delete-internal-sync",
            serde_json::json!({
                "device": node_name,
                "name": tag,
            }),
        )
        .map_err(|e| e.to_string())?;
    Ok(())
}

/// Internal (disk + memory) snapshot of a *running* VM via QMP's job-based
/// `snapshot-save`. `savevm` (the old HMP command name) isn't a real
/// top-level QMP command on current QEMU -- found live: "The command
/// savevm has not been found" -- and `human-monitor-command` is
/// deliberately never allow-listed (see qmp::ALLOWED_QMP_COMMANDS), so
/// this is the supported replacement: an async job, polled to completion.
///
/// Runs the whole sequence (query-block, snapshot-save, the query-jobs
/// poll loop, job-dismiss) over one held-open `QmpSession` -- see
/// `QmpClient::execute`'s doc comment for why a fresh reconnect per call
/// is wrong here. Found live: reconnecting for every poll iteration (up
/// to 50 reconnects for one snapshot) reliably wedged QEMU's monitor --
/// a `server=on,wait=off` QMP unix socket accepts only one live client,
/// and rapid connect/disconnect cycling against it left QEMU stuck
/// servicing a half-torn-down connection, starving every new connection
/// attempt's greeting read -- including unrelated ones, confirmed by a
/// concurrent raw probe against the same socket also stalling -- for
/// ~10s (the read timeout) until it eventually cleared on its own. One
/// held-open connection for the whole operation never triggers this.
///
/// Poll budget matches the QMP read timeout (300s): vmstate dumps under
/// disk contention routinely exceed the old 50×200ms ≈ 10s window.
pub(crate) fn live_snapshot_via_qmp(qmp: &crate::qmp::QmpClient, tag: &str) -> Result<(), String> {
    let mut session = qmp.open_session().map_err(|e| e.to_string())?;

    let node_name = primary_block_node(&mut session)?;

    let job_id = format!("snap-{}", &Uuid::new_v4().simple().to_string()[..8]);
    session
        .execute(
            "snapshot-save",
            serde_json::json!({
                "job-id": job_id,
                "tag": tag,
                "vmstate": node_name,
                "devices": [node_name],
            }),
        )
        .map_err(|e| e.to_string())?;

    // 300s / 200ms = 1500 polls — aligned with qmp session read timeout.
    const MAX_POLLS: u32 = 1500;
    let outcome = (|| {
        let mut last_err = String::new();
        for _ in 0..MAX_POLLS {
            // A query-jobs poll can itself error while QEMU's monitor is
            // busy mid-dump -- that's a reason to keep polling on this
            // same session, not to give up, since the underlying job
            // reliably completed regardless (confirmed by inspecting the
            // resulting qcow2's own snapshot list directly). Only a
            // definite job status matters; a failed poll just means we
            // don't know yet.
            let jobs = match session.execute("query-jobs", serde_json::Value::Null) {
                Ok(v) => v,
                Err(e) => {
                    last_err = e.to_string();
                    std::thread::sleep(std::time::Duration::from_millis(200));
                    continue;
                }
            };
            let job = jobs
                .as_array()
                .into_iter()
                .flatten()
                .find(|j| j.get("id").and_then(|v| v.as_str()) == Some(job_id.as_str()));
            match job.and_then(|j| j.get("status")).and_then(|s| s.as_str()) {
                Some("concluded") => {
                    return match job.and_then(|j| j.get("error")) {
                        Some(e) => Err(e.to_string()),
                        None => Ok(()),
                    };
                }
                Some(_) | None => std::thread::sleep(std::time::Duration::from_millis(200)),
            }
        }
        Err(format!(
            "timed out waiting for snapshot-save job to conclude (last poll error: {last_err})"
        ))
    })();

    let _ = session.execute("job-dismiss", serde_json::json!({"id": job_id}));
    outcome
}

/// POST /api/vms/:name/snapshots - Create a snapshot
pub async fn create_snapshot(
    RequireWrite(_claims): RequireWrite,
    State(state): State<Arc<AppState>>,
    Path(vm_name): Path<String>,
    Json(req): Json<CreateSnapshotRequest>,
) -> impl IntoResponse {
    tracing::debug!("snapshots::{}", stringify!(create_snapshot));
    create_snapshot_core(&state, vm_name, req).await
}

/// Shared by the HTTP handler and the snapshot-policy scheduler.
pub(crate) async fn create_snapshot_core(
    state: &Arc<AppState>,
    vm_name: String,
    req: CreateSnapshotRequest,
) -> axum::response::Response {
    if let Err((status, msg)) = crate::validation::validate_vm_name(&vm_name) {
        return crate::api_error::json_error(status, msg).into_response();
    }

    let _lock = state.vm_lock(&vm_name).lock_owned().await;

    // The VM's actual, live disk (FluxVM's copy-on-write instance disk,
    // not a naming-convention guess at its base image -- see
    // VMDriver::get_disk_path's doc comment for why that distinction
    // matters).
    let image_path = match state.driver.get_disk_path(&vm_name).await {
        Ok(p) => p.display().to_string(),
        Err(e) => {
            return crate::api_error::json_error(
                StatusCode::NOT_FOUND,
                format!("No disk image found for VM '{}': {}", vm_name, e),
            )
            .into_response();
        }
    };

    // Validate snapshot name
    if let Err((status, msg)) = crate::validation::validate_snapshot_name(&req.name) {
        return crate::api_error::json_error(status, msg).into_response();
    }

    // Validate description length
    if let Some(ref desc) = req.description {
        if desc.len() > 1024 {
            return crate::api_error::json_error(
                StatusCode::BAD_REQUEST,
                "Description must be at most 1024 characters",
            )
            .into_response();
        }
    }

    // A running VM already has the disk open exclusively (qcow2's own image
    // locking) -- an external `qemu-img snapshot -c` against the same path
    // collides with that lock and fails with "Is another process using the
    // image?". Route through QMP instead when the VM is running:
    //   * Disk  → blockdev-snapshot-internal-sync (fast, no vmstate)
    //   * Full  → snapshot-save job (disk + memory; can take minutes)
    // Found live: ignoring snapshot_type and always calling snapshot-save
    // made the UI "Disk Only" option time out under the 60s HTTP layer.
    let vm_running = matches!(
        state.store.get_vm(&vm_name).ok().flatten().map(|v| v.state),
        Some(vm_model::VMState::Running) | Some(vm_model::VMState::Paused)
    );

    if vm_running {
        // Resolve socket path, waiting briefly if FluxVM has marked the VM
        // running before the monitor socket appears (common right after start).
        let socket_path = {
            let mut path: Option<String> = None;
            for attempt in 0..15u32 {
                if let Ok(Some(p)) = state.driver.get_control_socket(&vm_name).await {
                    let s = p.to_string_lossy().into_owned();
                    let s_clone = s.clone();
                    let ready = tokio::task::spawn_blocking(move || {
                        wait_for_qmp_connectable(&s_clone, 1, 0)
                    })
                    .await
                    .unwrap_or(Err("join".into()));
                    if ready.is_ok() {
                        path = Some(s);
                        break;
                    }
                    path = Some(s); // keep last path for error message
                }
                if attempt + 1 < 15 {
                    tokio::time::sleep(std::time::Duration::from_millis(400)).await;
                }
            }
            match path {
                Some(p) => p,
                None => {
                    return crate::api_error::json_error(
                        StatusCode::CONFLICT,
                        "VM is running but its QMP control socket isn't available yet -- \
                         wait for it to finish starting, or stop it first, then retry",
                    )
                    .into_response();
                }
            }
        };

        // Final connectability wait (up to ~6s) before the real snapshot call.
        let wait_path = socket_path.clone();
        if let Err(e) =
            tokio::task::spawn_blocking(move || wait_for_qmp_connectable(&wait_path, 15, 400))
                .await
                .unwrap_or_else(|e| Err(e.to_string()))
        {
            return crate::api_error::json_error(
                StatusCode::CONFLICT,
                format!(
                    "Live snapshot could not reach the VM monitor yet ({e}). \
                     Wait a few seconds for the VM to finish starting, then retry."
                ),
            )
            .into_response();
        }

        let qmp = crate::qmp::QmpClient::for_socket(socket_path);
        let tag = req.name.clone();
        let snap_type = req.snapshot_type.clone();
        let qmp_result = tokio::task::spawn_blocking(move || match snap_type {
            SnapshotType::Disk => live_disk_snapshot_via_qmp(&qmp, &tag),
            SnapshotType::Full => live_snapshot_via_qmp(&qmp, &tag),
        })
        .await;
        match qmp_result {
            Ok(Ok(())) => {}
            Ok(Err(e)) => {
                let retryable = is_qmp_connect_retryable(&e);
                let status = if retryable {
                    StatusCode::CONFLICT
                } else {
                    StatusCode::INTERNAL_SERVER_ERROR
                };
                let msg = if retryable {
                    format!(
                        "Live snapshot could not reach the VM monitor yet ({e}). \
                         Wait a few seconds for the VM to finish starting, then retry."
                    )
                } else {
                    format!("Live snapshot failed: {e}")
                };
                return crate::api_error::json_error(status, msg).into_response();
            }
            Err(e) => {
                return crate::api_error::json_error(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    format!("Live snapshot task failed: {}", e),
                )
                .into_response();
            }
        }
    } else {
        if image_path.contains("..") {
            return crate::api_error::json_error(StatusCode::BAD_REQUEST, "invalid disk path")
                .into_response();
        }
        let image_owned = image_path;
        let image_path = image_owned.as_str();
        let snap = req.name.as_str();
        let allow_image = [image_path];
        let allow_snap = [snap];
        if !input_guard::is_safe_argv(image_path) || !input_guard::is_safe_component(snap) {
            return crate::api_error::json_error(
                StatusCode::BAD_REQUEST,
                "invalid snapshot request",
            )
            .into_response();
        }
        let output = if allow_image.contains(&image_path) {
            if allow_snap.contains(&snap) {
                Command::new("qemu-img")
                    .args(["snapshot", "-c", snap, image_path])
                    .output()
                    .await
            } else {
                return crate::api_error::json_error(
                    StatusCode::BAD_REQUEST,
                    "invalid snapshot name",
                )
                .into_response();
            }
        } else {
            return crate::api_error::json_error(StatusCode::BAD_REQUEST, "invalid disk path")
                .into_response();
        };

        match output {
            Ok(o) if !o.status.success() => {
                let stderr = String::from_utf8_lossy(&o.stderr);
                return crate::api_error::json_error(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    format!("qemu-img snapshot failed: {}", stderr),
                )
                .into_response();
            }
            Err(e) => {
                return crate::api_error::json_error(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    format!("Failed to run qemu-img: {}", e),
                )
                .into_response();
            }
            _ => {}
        }
    }

    let snapshot = VMSnapshot {
        id: Uuid::new_v4().to_string(),
        vm_name: vm_name.clone(),
        name: req.name,
        description: req.description,
        snapshot_type: req.snapshot_type,
        parent_id: None,
        size_bytes: 0,
        created: Utc::now(),
        tags: req.tags,
    };

    let store_key = format!("snapshots_{}", vm_name);
    match state.store.save_entity(&store_key, &snapshot.id, &snapshot) {
        Ok(_) => {
            crate::api::events::record_event(
                state,
                crate::api::events::VMEventType::SnapshotCreated,
                &vm_name,
                Some(format!("Snapshot: {}", snapshot.name)),
            );
            (StatusCode::CREATED, Json(snapshot)).into_response()
        }
        Err(e) => crate::api_error::json_error(StatusCode::INTERNAL_SERVER_ERROR, e.to_string())
            .into_response(),
    }
}

/// GET /api/vms/:name/snapshots - List snapshots for a VM
pub async fn list_snapshots(
    RequireRead(_claims): RequireRead,
    State(state): State<Arc<AppState>>,
    Path(vm_name): Path<String>,
) -> impl IntoResponse {
    tracing::debug!("snapshots::{}", stringify!(list_snapshots));
    if let Err((status, msg)) = crate::validation::validate_vm_name(&vm_name) {
        return crate::api_error::json_error(status, msg).into_response();
    }
    let store_key = format!("snapshots_{}", vm_name);
    let items: Vec<VMSnapshot> = state.store.list_entities(&store_key).unwrap_or_else(|e| {
        tracing::error!("Storage error loading {}: {}", store_key, e);
        Vec::new()
    });
    Json(items).into_response()
}

/// GET /api/vms/:name/snapshots/:id - Get snapshot details
pub async fn get_snapshot(
    RequireRead(_claims): RequireRead,
    State(state): State<Arc<AppState>>,
    Path((vm_name, id)): Path<(String, String)>,
) -> impl IntoResponse {
    tracing::debug!("snapshots::{}", stringify!(get_snapshot));
    if let Err((status, msg)) = crate::validation::validate_vm_name(&vm_name) {
        return crate::api_error::json_error(status, msg).into_response();
    }
    let store_key = format!("snapshots_{}", vm_name);
    match state.store.get_entity::<VMSnapshot>(&store_key, &id) {
        Ok(Some(s)) => Json(s).into_response(),
        Ok(None) => crate::api_error::json_error(StatusCode::NOT_FOUND, "Snapshot not found")
            .into_response(),
        Err(_) => crate::api_error::json_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "Failed to load snapshot",
        )
        .into_response(),
    }
}

/// DELETE /api/vms/:name/snapshots/:id - Delete a snapshot
pub async fn delete_snapshot(
    RequireAdmin(_claims): RequireAdmin,
    State(state): State<Arc<AppState>>,
    Path((vm_name, id)): Path<(String, String)>,
) -> impl IntoResponse {
    tracing::debug!("snapshots::{}", stringify!(delete_snapshot));
    delete_snapshot_core(&state, vm_name, id).await
}

/// Whether a delete error means the snapshot tag is already absent from the
/// image (so the stale record can safely go).
pub(crate) fn is_snapshot_already_gone(err: &str) -> bool {
    let e = err.to_ascii_lowercase();
    e.contains("could not find snapshot")
        || (e.contains("snapshot") && e.contains("not found"))
        || e.contains("does not exist")
}

/// Interpret a finished `qemu-img snapshot -d`.
pub(crate) fn qemu_img_delete_outcome(success: bool, stderr: &str) -> Result<(), String> {
    if success || is_snapshot_already_gone(stderr) {
        Ok(())
    } else {
        Err(format!("qemu-img snapshot -d failed: {}", stderr.trim()))
    }
}

/// Delete the snapshot tag from the live disk (QMP when the VM runs,
/// `qemu-img` otherwise). Any failure is returned.
async fn delete_from_disk(
    state: &Arc<AppState>,
    vm_name: &str,
    snapshot: &VMSnapshot,
) -> Result<(), String> {
    if crate::validation::validate_snapshot_name(&snapshot.name).is_err() {
        // A corrupted name cannot be passed to qemu; nothing safe to delete.
        tracing::error!("Corrupted snapshot name in store, skipping disk delete");
        return Ok(());
    }
    let vm_running = matches!(
        state.store.get_vm(vm_name).ok().flatten().map(|v| v.state),
        Some(vm_model::VMState::Running) | Some(vm_model::VMState::Paused)
    );
    if vm_running {
        let sock = state
            .driver
            .get_control_socket(vm_name)
            .await
            .map_err(|e| e.to_string())?
            .ok_or_else(|| "VM is running but has no QMP control socket".to_string())?;
        let qmp = crate::qmp::QmpClient::for_socket(sock.to_string_lossy().into_owned());
        let tag = snapshot.name.clone();
        match tokio::task::spawn_blocking(move || live_delete_snapshot_via_qmp(&qmp, &tag))
            .await
            .unwrap_or_else(|e| Err(e.to_string()))
        {
            Err(e) if !is_snapshot_already_gone(&e) => Err(format!("QMP snapshot delete: {e}")),
            _ => Ok(()),
        }
    } else {
        let disk = state
            .driver
            .get_disk_path(vm_name)
            .await
            .map_err(|e| format!("disk path: {e}"))?;
        let path = disk.display().to_string();
        let out = Command::new("qemu-img")
            .args(["snapshot", "-d", &snapshot.name, &path])
            .output()
            .await
            .map_err(|e| format!("failed to run qemu-img: {e}"))?;
        qemu_img_delete_outcome(out.status.success(), &String::from_utf8_lossy(&out.stderr))
    }
}

/// Shared by the HTTP handler and the snapshot-policy scheduler.
pub(crate) async fn delete_snapshot_core(
    state: &Arc<AppState>,
    vm_name: String,
    id: String,
) -> axum::response::Response {
    if let Err((status, msg)) = crate::validation::validate_vm_name(&vm_name) {
        return crate::api_error::json_error(status, msg).into_response();
    }

    let _lock = state.vm_lock(&vm_name).lock_owned().await;

    let store_key = format!("snapshots_{}", vm_name);

    // Remove the snapshot from the disk first. If that fails the record is
    // KEPT and the failure is reported, so the record never claims a
    // snapshot is gone while it still occupies the image.
    if let Ok(Some(snapshot)) = state.store.get_entity::<VMSnapshot>(&store_key, &id) {
        if let Err(e) = delete_from_disk(state, &vm_name, &snapshot).await {
            tracing::warn!("snapshot delete failed for '{}': {}", snapshot.name, e);
            return crate::api_error::json_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                format!(
                    "Snapshot '{}' could not be removed from the disk; the record was kept: {}",
                    snapshot.name, e
                ),
            )
            .into_response();
        }
    }

    match state.store.delete_entity(&store_key, &id) {
        Ok(_) => StatusCode::NO_CONTENT.into_response(),
        Err(e) => crate::api_error::json_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Failed to delete snapshot: {}", e),
        )
        .into_response(),
    }
}

#[derive(Debug, Deserialize)]
pub struct SetTagsRequest {
    pub tags: Vec<String>,
}

const MAX_TAGS: usize = 16;
const MAX_TAG_LEN: usize = 32;

/// Validate and normalise a tag list: 1-32 chars of `[A-Za-z0-9._:-]`,
/// at most 16, duplicates dropped (first occurrence wins).
pub(crate) fn normalize_tags(tags: &[String]) -> Result<Vec<String>, String> {
    let mut out: Vec<String> = Vec::new();
    for t in tags {
        let t = t.trim();
        if t.is_empty() || t.len() > MAX_TAG_LEN {
            return Err(format!("tags must be 1 to {MAX_TAG_LEN} characters"));
        }
        if !t
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | ':' | '-'))
        {
            return Err(format!(
                "tag '{t}' may only contain letters, digits, . _ : -"
            ));
        }
        if !out.iter().any(|o| o == t) {
            out.push(t.to_string());
        }
    }
    if out.len() > MAX_TAGS {
        return Err(format!("at most {MAX_TAGS} tags per snapshot"));
    }
    Ok(out)
}

/// PUT /api/vms/:name/snapshots/:id/tags - Replace a snapshot's tags
/// (set or clear `keep`, which protects it from policy pruning).
pub async fn set_snapshot_tags(
    RequireWrite(claims): RequireWrite,
    State(state): State<Arc<AppState>>,
    Path((vm_name, id)): Path<(String, String)>,
    Json(req): Json<SetTagsRequest>,
) -> impl IntoResponse {
    if let Err((status, msg)) = crate::validation::validate_vm_name(&vm_name) {
        return crate::api_error::json_error(status, msg).into_response();
    }
    let tags = match normalize_tags(&req.tags) {
        Ok(t) => t,
        Err(e) => return crate::api_error::json_error(StatusCode::BAD_REQUEST, e).into_response(),
    };
    let _lock = state.vm_lock(&vm_name).lock_owned().await;
    let store_key = format!("snapshots_{}", vm_name);
    let mut snap = match state.store.get_entity::<VMSnapshot>(&store_key, &id) {
        Ok(Some(s)) => s,
        Ok(None) => {
            return crate::api_error::json_error(StatusCode::NOT_FOUND, "Snapshot not found")
                .into_response()
        }
        Err(_) => {
            return crate::api_error::json_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                "Failed to load snapshot",
            )
            .into_response()
        }
    };
    snap.tags = tags;
    if let Err(e) = state.store.save_entity(&store_key, &snap.id, &snap) {
        return crate::api_error::json_error(StatusCode::INTERNAL_SERVER_ERROR, e.to_string())
            .into_response();
    }
    crate::routes::audit(
        &state,
        &claims.sub,
        "SNAPSHOT_TAGS_SET",
        &format!("vm/{vm_name}/snapshot/{}", snap.name),
        "SUCCESS",
    );
    Json(snap).into_response()
}

/// POST /api/vms/:name/snapshots/:id/revert - Revert to a snapshot
pub async fn revert_snapshot(
    RequireWrite(_claims): RequireWrite,
    State(state): State<Arc<AppState>>,
    Path((vm_name, id)): Path<(String, String)>,
) -> impl IntoResponse {
    tracing::debug!("snapshots::{}", stringify!(revert_snapshot));
    if let Err((status, msg)) = crate::validation::validate_vm_name(&vm_name) {
        return crate::api_error::json_error(status, msg).into_response();
    }

    let _lock = state.vm_lock(&vm_name).lock_owned().await;

    let store_key = format!("snapshots_{}", vm_name);

    let snapshot = match state.store.get_entity::<VMSnapshot>(&store_key, &id) {
        Ok(Some(s)) => s,
        Ok(None) => {
            return crate::api_error::json_error(StatusCode::NOT_FOUND, "Snapshot not found")
                .into_response()
        }
        Err(_) => {
            return crate::api_error::json_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                "Failed to load snapshot",
            )
            .into_response()
        }
    };

    // Check that the VM is stopped before reverting
    if let Ok(Some(vm)) = state.store.get_vm(&vm_name) {
        if vm.state == vm_model::VMState::Running || vm.state == vm_model::VMState::Starting {
            return crate::api_error::json_error(
                StatusCode::CONFLICT,
                "VM must be stopped before reverting a snapshot",
            )
            .into_response();
        }
    }

    // VM should be stopped before reverting. Resolving the disk path is
    // not optional here: silently skipping the qemu-img call on failure
    // (the previous behavior) reported "reverted" success below without
    // actually reverting anything.
    let disk = match state.driver.get_disk_path(&vm_name).await {
        Ok(d) => d,
        Err(e) => {
            return crate::api_error::json_error(
                StatusCode::NOT_FOUND,
                format!("No disk image found for VM '{}': {}", vm_name, e),
            )
            .into_response();
        }
    };
    let path = disk.display().to_string();
    let output = Command::new("qemu-img")
        .args(["snapshot", "-a", &snapshot.name, &path])
        .output()
        .await;

    match output {
        Ok(o) if !o.status.success() => {
            let stderr = String::from_utf8_lossy(&o.stderr);
            return crate::api_error::json_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Revert failed: {}. Ensure VM is stopped.", stderr),
            )
            .into_response();
        }
        Err(e) => {
            return crate::api_error::json_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Failed to run qemu-img: {}", e),
            )
            .into_response();
        }
        _ => {}
    }

    crate::api::events::record_event(
        &state,
        crate::api::events::VMEventType::SnapshotReverted,
        &vm_name,
        Some(format!("Reverted to: {}", snapshot.name)),
    );
    Json(serde_json::json!({"status": "reverted", "snapshot": snapshot.name})).into_response()
}

/// GET /api/vms/:name/snapshots/tree - Get snapshot tree
pub async fn snapshot_tree(
    RequireRead(_claims): RequireRead,
    State(state): State<Arc<AppState>>,
    Path(vm_name): Path<String>,
) -> impl IntoResponse {
    tracing::debug!("snapshots::{}", stringify!(snapshot_tree));
    if let Err((status, msg)) = crate::validation::validate_vm_name(&vm_name) {
        return crate::api_error::json_error(status, msg).into_response();
    }
    let store_key = format!("snapshots_{}", vm_name);
    let snapshots: Vec<VMSnapshot> = state.store.list_entities(&store_key).unwrap_or_else(|e| {
        tracing::error!("Storage error loading {}: {}", store_key, e);
        Vec::new()
    });

    // Build tree from parent_id relationships
    let roots: Vec<SnapshotTreeNode> = build_snapshot_tree(&snapshots);
    Json(roots).into_response()
}

fn build_snapshot_tree(snapshots: &[VMSnapshot]) -> Vec<SnapshotTreeNode> {
    use std::collections::HashMap;

    // Build parent_id -> children index for O(n) tree construction
    let mut children_map: HashMap<Option<&str>, Vec<&VMSnapshot>> = HashMap::new();
    for snap in snapshots {
        children_map
            .entry(snap.parent_id.as_deref())
            .or_default()
            .push(snap);
    }

    fn build_node(
        snap: &VMSnapshot,
        children_map: &HashMap<Option<&str>, Vec<&VMSnapshot>>,
        depth: usize,
    ) -> SnapshotTreeNode {
        let children = if depth >= 100 {
            vec![] // Prevent infinite recursion on circular parent references
        } else {
            children_map
                .get(&Some(snap.id.as_str()))
                .map(|kids| {
                    kids.iter()
                        .map(|s| build_node(s, children_map, depth + 1))
                        .collect()
                })
                .unwrap_or_default()
        };
        SnapshotTreeNode {
            snapshot: snap.clone(),
            children,
        }
    }

    children_map
        .get(&None)
        .map(|roots| {
            roots
                .iter()
                .map(|s| build_node(s, &children_map, 0))
                .collect()
        })
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_snap(id: &str, parent: Option<&str>) -> VMSnapshot {
        VMSnapshot {
            id: id.to_string(),
            vm_name: "test-vm".to_string(),
            name: format!("snap-{}", id),
            description: None,
            snapshot_type: SnapshotType::Disk,
            parent_id: parent.map(|s| s.to_string()),
            size_bytes: 0,
            created: Utc::now(),
            tags: vec![],
        }
    }

    #[test]
    fn test_tree_empty() {
        let tree = build_snapshot_tree(&[]);
        assert!(tree.is_empty());
    }

    #[test]
    fn test_tree_single_root() {
        let snaps = vec![make_snap("a", None)];
        let tree = build_snapshot_tree(&snaps);
        assert_eq!(tree.len(), 1);
        assert_eq!(tree[0].snapshot.id, "a");
        assert!(tree[0].children.is_empty());
    }

    #[test]
    fn test_tree_parent_child() {
        let snaps = vec![make_snap("root", None), make_snap("child", Some("root"))];
        let tree = build_snapshot_tree(&snaps);
        assert_eq!(tree.len(), 1);
        assert_eq!(tree[0].snapshot.id, "root");
        assert_eq!(tree[0].children.len(), 1);
        assert_eq!(tree[0].children[0].snapshot.id, "child");
    }

    #[test]
    fn test_tree_depth_limit() {
        // Build a chain of 110 snapshots, each parenting the next
        let mut snaps = vec![make_snap("0", None)];
        for i in 1..110 {
            snaps.push(make_snap(&i.to_string(), Some(&(i - 1).to_string())));
        }
        let tree = build_snapshot_tree(&snaps);
        assert_eq!(tree.len(), 1);
        // Walk down — should stop at depth 100
        let mut node = &tree[0];
        let mut depth = 0;
        while !node.children.is_empty() {
            node = &node.children[0];
            depth += 1;
        }
        assert!(depth <= 100, "Tree should stop at depth 100, got {}", depth);
    }

    #[test]
    fn test_default_snapshot_type_is_disk() {
        let json = r#"{"name":"s1"}"#;
        let req: CreateSnapshotRequest = serde_json::from_str(json).unwrap();
        assert!(matches!(req.snapshot_type, SnapshotType::Disk));
    }

    #[test]
    fn test_qmp_connect_retryable_detection() {
        assert!(is_qmp_connect_retryable(
            "Failed to connect to QMP socket: /tmp/qmp.sock"
        ));
        assert!(is_qmp_connect_retryable("No such file or directory"));
        assert!(is_qmp_connect_retryable("Connection refused"));
        assert!(!is_qmp_connect_retryable("QMP error: DeviceNotFound"));
        assert!(!is_qmp_connect_retryable(
            "timed out waiting for snapshot-save job"
        ));
    }

    #[test]
    fn test_snapshot_type_serde_roundtrip() {
        let disk = serde_json::to_string(&SnapshotType::Disk).unwrap();
        let full = serde_json::to_string(&SnapshotType::Full).unwrap();
        assert_eq!(disk, "\"Disk\"");
        assert_eq!(full, "\"Full\"");
        assert!(matches!(
            serde_json::from_str::<SnapshotType>("\"Disk\"").unwrap(),
            SnapshotType::Disk
        ));
        assert!(matches!(
            serde_json::from_str::<SnapshotType>("\"Full\"").unwrap(),
            SnapshotType::Full
        ));
    }

    #[test]
    fn test_tag_normalisation() {
        let ok = normalize_tags(&["keep".into(), " keep ".into(), "a.b:c-d_e".into()]).unwrap();
        assert_eq!(ok, vec!["keep", "a.b:c-d_e"]);
        assert!(normalize_tags(&[]).unwrap().is_empty());
        assert!(normalize_tags(&["".into()]).is_err());
        assert!(normalize_tags(&["bad tag".into()]).is_err());
        assert!(normalize_tags(&["x".repeat(33)]).is_err());
        let many: Vec<String> = (0..17).map(|i| format!("t{i}")).collect();
        assert!(normalize_tags(&many).is_err());
    }

    #[test]
    fn test_qemu_img_delete_outcome_reports_failure() {
        assert!(qemu_img_delete_outcome(true, "").is_ok());
        let e = qemu_img_delete_outcome(false, "qemu-img: Failed to get shared write lock\n")
            .unwrap_err();
        assert!(e.contains("shared"), "{e}");
        // tag already absent: the stale record may go
        assert!(qemu_img_delete_outcome(
            false,
            "qemu-img: Could not delete snapshot 'x': (Could not find snapshot 'x' on disk)"
        )
        .is_ok());
        assert!(is_snapshot_already_gone("Snapshot x not found"));
        assert!(!is_snapshot_already_gone("Device or resource busy"));
    }
}
