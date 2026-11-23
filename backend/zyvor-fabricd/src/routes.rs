// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: LicenseRef-Zyvor-Production-2.0

use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use cloud_init::{CloudInitConfig, CloudInitGenerator};
use security::{RequireAdmin, RequireRead, RequireWrite};
use serde::Deserialize;
use serde_json::json;
use std::sync::Arc;
use vm_model::{CreateVMRequest, VMStartOptions, VM};

use crate::server::AppState;
use crate::validation::validate_vm_name;

#[derive(Debug, Deserialize)]
pub struct PaginationQuery {
    #[serde(default)]
    pub offset: Option<usize>,
    #[serde(default)]
    pub limit: Option<usize>,
    /// Optional Fabric/FluxVM tenant filter — matches `labels.tenant` or tag `tenant:…`.
    #[serde(default)]
    pub tenant: Option<String>,
}

/// GET /api/license - Evaluation trial status. Read-only (RequireRead, not
/// RequireWrite/Admin) since checking remaining trial time shouldn't
/// itself require the write access the trial gates.
pub async fn get_license_status(RequireRead(_claims): RequireRead) -> impl IntoResponse {
    Json(json!({
        "trial": true,
        "days_remaining": security::trial::days_remaining(),
        "expired": security::trial::is_expired(),
    }))
}

/// Helper: record an audit log entry to both tracing and the state store.
pub(crate) fn audit(state: &AppState, user: &str, action: &str, resource: &str, status: &str) {
    // Persisted (and hash-chained when `[audit] chain` is on) for queryable history.
    crate::audit_trail::record(state, user, action, resource, status);
}

use crate::api_error::json_error;

/// JSON error with path sanitization for non-admin users.
fn json_error_safe(
    status: StatusCode,
    msg: impl Into<String>,
    claims: &security::Claims,
) -> (StatusCode, Json<serde_json::Value>) {
    let msg = msg.into();
    let safe_msg = if claims.role.can_manage() {
        msg
    } else {
        crate::validation::sanitize_error(&msg)
    };
    json_error(status, safe_msg)
}

pub async fn list_vms(
    RequireRead(claims): RequireRead,
    State(state): State<Arc<AppState>>,
    Query(pagination): Query<PaginationQuery>,
) -> impl IntoResponse {
    let offset = pagination.offset.unwrap_or(0);
    let limit = pagination.limit.unwrap_or(200);
    // Cap limit to prevent abuse
    let limit = limit.min(1000);

    let tenant_filter =
        match crate::tenant_scope::apply_list_tenant_filter(&claims, pagination.tenant) {
            Ok(t) => t,
            Err((status, msg)) => return json_error(status, msg).into_response(),
        };

    match state.store.list_vms_paginated(offset, limit) {
        Ok((mut vms, mut total)) => {
            if let Some(ref tenant) = tenant_filter {
                vms.retain(|vm| {
                    crate::tenant_scope::vm_tenant(vm).as_deref() == Some(tenant.as_str())
                });
                total = vms.len();
            }
            (
                StatusCode::OK,
                Json(json!({ "items": vms, "total": total, "offset": offset, "limit": limit })),
            )
                .into_response()
        }
        Err(e) => json_error_safe(StatusCode::INTERNAL_SERVER_ERROR, e.to_string(), &claims)
            .into_response(),
    }
}

pub async fn get_vm(
    RequireRead(_claims): RequireRead,
    State(state): State<Arc<AppState>>,
    Path(name): Path<String>,
) -> impl IntoResponse {
    if let Err((status, msg)) = validate_vm_name(&name) {
        return json_error(status, msg).into_response();
    }
    match state.store.get_vm(&name) {
        Ok(Some(vm)) => (StatusCode::OK, Json(vm)).into_response(),
        Ok(None) => json_error(StatusCode::NOT_FOUND, "VM not found").into_response(),
        Err(e) => json_error_safe(StatusCode::INTERNAL_SERVER_ERROR, e.to_string(), &_claims)
            .into_response(),
    }
}

#[derive(Debug, Deserialize)]
pub struct AddTagRequest {
    pub tag: String,
}

#[derive(Debug, Deserialize)]
pub struct UpdateTagsRequest {
    pub tags: Vec<String>,
}

/// Tags are display labels, not identifiers used elsewhere (no path/shell
/// use), so this is a sanity cap rather than a security boundary -- mainly
/// to stop an accidental paste of something huge from bloating the VM
/// record indefinitely.
fn validate_tag(tag: &str) -> Result<(), (StatusCode, String)> {
    if tag.is_empty() || tag.chars().count() > 63 {
        return Err((
            StatusCode::BAD_REQUEST,
            "Tag must be between 1 and 63 characters".to_string(),
        ));
    }
    Ok(())
}

/// POST /api/vms/:name/tags - Add a tag to a VM
pub async fn add_tag(
    RequireWrite(_claims): RequireWrite,
    State(state): State<Arc<AppState>>,
    Path(name): Path<String>,
    Json(req): Json<AddTagRequest>,
) -> impl IntoResponse {
    if let Err((status, msg)) = validate_vm_name(&name) {
        return json_error(status, msg).into_response();
    }
    if let Err((status, msg)) = validate_tag(&req.tag) {
        return json_error(status, msg).into_response();
    }
    let mut vm = match state.store.get_vm(&name) {
        Ok(Some(vm)) => vm,
        Ok(None) => return json_error(StatusCode::NOT_FOUND, "VM not found").into_response(),
        Err(e) => {
            return json_error(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response()
        }
    };
    let tags = vm.tags.get_or_insert_with(Vec::new);
    if !tags.contains(&req.tag) {
        tags.push(req.tag);
    }
    if let Err(e) = state.store.save_vm(&vm) {
        return json_error(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response();
    }
    (StatusCode::OK, Json(vm)).into_response()
}

/// DELETE /api/vms/:name/tags/:tag - Remove a tag from a VM
pub async fn remove_tag(
    RequireWrite(_claims): RequireWrite,
    State(state): State<Arc<AppState>>,
    Path((name, tag)): Path<(String, String)>,
) -> impl IntoResponse {
    if let Err((status, msg)) = validate_vm_name(&name) {
        return json_error(status, msg).into_response();
    }
    let mut vm = match state.store.get_vm(&name) {
        Ok(Some(vm)) => vm,
        Ok(None) => return json_error(StatusCode::NOT_FOUND, "VM not found").into_response(),
        Err(e) => {
            return json_error(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response()
        }
    };
    if let Some(tags) = vm.tags.as_mut() {
        tags.retain(|t| t != &tag);
    }
    if let Err(e) = state.store.save_vm(&vm) {
        return json_error(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response();
    }
    (StatusCode::OK, Json(vm)).into_response()
}

/// PUT /api/vms/:name/tags - Replace a VM's full tag set
pub async fn update_tags(
    RequireWrite(_claims): RequireWrite,
    State(state): State<Arc<AppState>>,
    Path(name): Path<String>,
    Json(req): Json<UpdateTagsRequest>,
) -> impl IntoResponse {
    if let Err((status, msg)) = validate_vm_name(&name) {
        return json_error(status, msg).into_response();
    }
    if let Some((status, msg)) = req.tags.iter().find_map(|t| validate_tag(t).err()) {
        return json_error(status, msg).into_response();
    }
    let mut vm = match state.store.get_vm(&name) {
        Ok(Some(vm)) => vm,
        Ok(None) => return json_error(StatusCode::NOT_FOUND, "VM not found").into_response(),
        Err(e) => {
            return json_error(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response()
        }
    };
    vm.tags = Some(req.tags);
    if let Err(e) = state.store.save_vm(&vm) {
        return json_error(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response();
    }
    (StatusCode::OK, Json(vm)).into_response()
}

pub async fn create_vm(
    RequireWrite(claims): RequireWrite,
    State(state): State<Arc<AppState>>,
    Json(mut req): Json<CreateVMRequest>,
) -> impl IntoResponse {
    if let Err((status, msg)) = validate_vm_name(&req.name) {
        return json_error(status, msg).into_response();
    }

    if let Err(errors) = req.validate() {
        return json_error(
            StatusCode::BAD_REQUEST,
            format!("Invalid VM parameters: {}", errors.join("; ")),
        )
        .into_response();
    }

    match crate::tenant_scope::apply_create_tenant(&claims, req.tenant.take()) {
        Ok(t) => req.tenant = t,
        Err((status, msg)) => return json_error(status, msg).into_response(),
    }

    // Acquire per-VM lock before duplicate check to prevent TOCTOU races
    let _lock = state.vm_lock(&req.name).lock_owned().await;

    // Check for duplicate VM name
    if let Ok(Some(_)) = state.store.get_vm(&req.name) {
        return json_error(StatusCode::CONFLICT, "VM with this name already exists")
            .into_response();
    }

    let vm = VM::from_request(&req);

    if let Err(e) = state.store.save_vm(&vm) {
        return json_error(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response();
    }

    if let Err(e) = provision_vm_disk(&state, &vm).await {
        tracing::warn!("Failed to provision disk for VM '{}': {}", vm.name, e);
    }

    // Allocate security identity if VM has labels
    if let Some(ref labels) = vm.labels {
        if !labels.is_empty() {
            match state
                .policy_engine
                .allocator
                .allocate_or_get(labels, &vm.name)
            {
                Ok(id) => {
                    if let Some(ref ip) = vm.ip {
                        if let Err(e) = state.policy_engine.allocator.update_ip_mapping(ip, id) {
                            tracing::warn!(
                                "Failed to update IP mapping for VM '{}': {}",
                                vm.name,
                                e
                            );
                        }
                    }
                    tracing::debug!("Allocated identity {} for VM '{}'", id, vm.name);
                }
                Err(e) => {
                    tracing::warn!("Failed to allocate identity for VM '{}': {}", vm.name, e);
                }
            }
        }
    }

    audit(
        &state,
        &claims.sub,
        "CREATE",
        &format!("vm/{}", vm.name),
        "SUCCESS",
    );
    crate::api::events::record_event(
        &state,
        crate::api::events::VMEventType::Created,
        &vm.name,
        None,
    );
    (StatusCode::CREATED, Json(vm)).into_response()
}

pub async fn delete_vm(
    RequireAdmin(claims): RequireAdmin,
    State(state): State<Arc<AppState>>,
    Path(name): Path<String>,
) -> impl IntoResponse {
    if let Err((status, msg)) = validate_vm_name(&name) {
        return json_error(status, msg).into_response();
    }

    let _lock = state.vm_lock(&name).lock_owned().await;

    // Deallocate security identity before deleting VM
    if let Ok(Some(vm)) = state.store.get_vm(&name) {
        if let Some(ref labels) = vm.labels {
            if !labels.is_empty() {
                if let Err(e) = state.policy_engine.allocator.deallocate(&name, labels) {
                    tracing::warn!("Failed to deallocate identity for VM '{}': {}", name, e);
                }
            }
        }
        if let Some(ref ip) = vm.ip {
            if let Err(e) = state.policy_engine.allocator.remove_ip_mapping(ip) {
                tracing::warn!("Failed to remove IP mapping for VM '{}': {}", name, e);
            }
        }
    }

    // Tell the driver to actually destroy the machine -- it stops the VM
    // if still running and reclaims its real disk/storage (LVM thin
    // snapshot, qemu-nbd export, Ceph RBD clone, ...). Previously this only
    // removed zyvor-fabricd's own store record and guessed at a disk file
    // to unlink by naming convention, which never reached FluxVM at all:
    // every deleted VM left its full instance (disk, and if it was still
    // running, the live QEMU process) permanently orphaned. A VM that was
    // created here but never started has no FluxVM-side counterpart yet
    // ("no VM named ... known to FluxVM") -- that's fine, there's nothing
    // to destroy. Any other failure (including FluxVM's own safety
    // refusal when a process won't die) must block the delete, or we'd
    // recreate the exact leak this fixes by dropping the record anyway.
    if let Err(e) = state.driver.delete(&name).await {
        let msg = e.to_string();
        // Soft-miss: FluxVM never knew the VM, or FluxVM is unreachable so we
        // cannot confirm a live instance (integration tests / FluxVM-down labs).
        let soft_miss = msg.contains("known to FluxVM")
            || msg.contains("Connection refused")
            || msg.contains("error sending request")
            || msg.contains("tcp connect error")
            || msg.contains("ConnectError");
        if !soft_miss {
            audit(
                &state,
                &claims.sub,
                "DELETE",
                &format!("vm/{}", name),
                "FAILED",
            );
            return json_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Failed to destroy VM '{}': {}", name, msg),
            )
            .into_response();
        }
    }

    match state.store.delete_vm(&name) {
        Ok(_) => {
            // Clean up the per-VM lock entry to prevent unbounded growth
            if let Ok(mut locks) = state.vm_locks.lock() {
                locks.remove(&name);
            }
            audit(
                &state,
                &claims.sub,
                "DELETE",
                &format!("vm/{}", name),
                "SUCCESS",
            );
            crate::api::events::record_event(
                &state,
                crate::api::events::VMEventType::Deleted,
                &name,
                None,
            );
            StatusCode::NO_CONTENT.into_response()
        }
        Err(e) => {
            audit(
                &state,
                &claims.sub,
                "DELETE",
                &format!("vm/{}", name),
                "FAILED",
            );
            json_error(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response()
        }
    }
}

#[derive(Debug, Deserialize)]
pub struct AddPortForwardRequest {
    pub host_port: u16,
    pub guest_port: u16,
    #[serde(default)]
    pub protocol: Option<String>,
}

/// Expose a guest port (e.g. SSH on 22) on this VM's usermode networking.
/// Usermode/slirp networking only accepts forwards at instance-creation
/// time -- there is no way to add one to an already-running VM -- so a
/// running VM gets destroyed and relaunched with its full, updated forward
/// set; a stopped VM has its FluxVM-side record cleared too (if it has
/// one) so its next Start creates fresh with the forward included, instead
/// of `start_with_options` finding the old record still there and
/// replaying its stale original request (found live: adding a forward to
/// a VM that had already been started once before, then stopped, silently
/// never took effect on the next Start -- `FluxVmDriver::start_with_options`
/// deliberately replays a known VM's stored request rather than
/// re-translating fresh options, so the only way to pick up a forward
/// added afterward is to not have a stale record sitting there at all).
pub async fn add_port_forward(
    RequireWrite(claims): RequireWrite,
    State(state): State<Arc<AppState>>,
    Path(name): Path<String>,
    Json(req): Json<AddPortForwardRequest>,
) -> impl IntoResponse {
    if let Err((status, msg)) = validate_vm_name(&name) {
        return json_error(status, msg).into_response();
    }
    if req.host_port == 0 || req.guest_port == 0 {
        return json_error(
            StatusCode::BAD_REQUEST,
            "host_port and guest_port must be nonzero",
        )
        .into_response();
    }

    let _lock = state.vm_lock(&name).lock_owned().await;

    let mut vm = match state.store.get_vm(&name) {
        Ok(Some(vm)) => vm,
        Ok(None) => return json_error(StatusCode::NOT_FOUND, "VM not found").into_response(),
        Err(e) => {
            return json_error_safe(StatusCode::INTERNAL_SERVER_ERROR, e.to_string(), &claims)
                .into_response()
        }
    };

    if vm.network_tap {
        return json_error(
            StatusCode::BAD_REQUEST,
            "this VM uses bridged networking and already has its own reachable IP (see its Network tab) -- port forwards only apply to NAT-networked VMs",
        )
        .into_response();
    }
    if vm
        .direct_uplink
        .as_ref()
        .is_some_and(|s| !s.trim().is_empty())
    {
        return json_error(
            StatusCode::BAD_REQUEST,
            "this VM uses a direct uplink -- port forwards only apply to NAT-networked VMs",
        )
        .into_response();
    }

    if vm
        .port_forwards
        .iter()
        .any(|f| f.host_port == req.host_port)
    {
        return json_error(
            StatusCode::CONFLICT,
            format!(
                "host port {} is already forwarded on this VM",
                req.host_port
            ),
        )
        .into_response();
    }

    let was_running = matches!(
        vm.state,
        vm_model::VMState::Running | vm_model::VMState::Starting | vm_model::VMState::Paused
    );

    vm.port_forwards.push(vm_model::PortForwardSpec {
        host_port: req.host_port,
        guest_port: req.guest_port,
        protocol: req.protocol.unwrap_or_else(|| "tcp".to_string()),
    });

    if let Err(e) = state.store.save_vm(&vm) {
        return json_error_safe(StatusCode::INTERNAL_SERVER_ERROR, e.to_string(), &claims)
            .into_response();
    }

    // Clear any existing FluxVM-side record regardless of current running
    // state -- a stopped VM that was previously started still has one, and
    // leaving it in place means the next Start replays its stale original
    // forward set instead of picking up what was just added (see doc
    // comment above). Best-effort: "doesn't exist yet" is the expected,
    // harmless outcome for a VM that's never been started at all.
    if let Err(e) = state.driver.delete(&name).await {
        let msg = e.to_string();
        if !msg.contains("known to FluxVM") {
            audit(
                &state,
                &claims.sub,
                "ADD_PORT_FORWARD",
                &format!("vm/{}", name),
                "FAILED",
            );
            return json_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                format!(
                    "Failed to clear VM '{}' for recreation with the new port forward: {}",
                    name, msg
                ),
            )
            .into_response();
        }
    }

    if was_running {
        let opts = vm_model::VMStartOptions {
            port_forwards: vm.port_forwards.clone(),
            network_tap: vm.network_tap,
            network_static_ip: vm.network_static_ip,
            ssh_authorized_keys: vm.ssh_authorized_keys.clone(),
            cloud_init_packages: vm.cloud_init_packages.clone(),
            cloud_init_runcmd: vm.cloud_init_runcmd.clone(),
            cloud_init_write_files: vm.cloud_init_write_files.clone(),
            storage: vm.storage.clone(),
            enable_qga: vm.enable_qga,
            hyperv: vm.hyperv,
            direct_uplink: vm.direct_uplink.clone(),
            direct_mode: vm.direct_mode.clone(),
            direct_guest_ips: vm.direct_guest_ips.clone(),
            ..Default::default()
        };
        if let Err(e) = state.driver.start_with_options(&vm, &opts).await {
            audit(
                &state,
                &claims.sub,
                "ADD_PORT_FORWARD",
                &format!("vm/{}", name),
                "FAILED",
            );
            return json_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                format!(
                    "Added the forward but failed to restart VM '{}': {}",
                    name, e
                ),
            )
            .into_response();
        }
        vm.state = vm_model::VMState::Running;
        vm.last_error = None;
        if let Err(e) = state.store.save_vm(&vm) {
            tracing::error!("Failed to save VM state after port-forward recreate: {}", e);
        }
    }

    audit(
        &state,
        &claims.sub,
        "ADD_PORT_FORWARD",
        &format!("vm/{}", name),
        "SUCCESS",
    );
    (StatusCode::OK, Json(vm)).into_response()
}

/// Remove a previously-added port forward by its host port. Same
/// recreate-on-running-VM reasoning as `add_port_forward` above: usermode
/// networking only accepts forwards at instance-creation time, so removing
/// one from a running VM means destroying and relaunching it with the
/// updated (shorter) forward set.
pub async fn remove_port_forward(
    RequireWrite(claims): RequireWrite,
    State(state): State<Arc<AppState>>,
    Path((name, host_port)): Path<(String, u16)>,
) -> impl IntoResponse {
    if let Err((status, msg)) = validate_vm_name(&name) {
        return json_error(status, msg).into_response();
    }

    let _lock = state.vm_lock(&name).lock_owned().await;

    let mut vm = match state.store.get_vm(&name) {
        Ok(Some(vm)) => vm,
        Ok(None) => return json_error(StatusCode::NOT_FOUND, "VM not found").into_response(),
        Err(e) => {
            return json_error_safe(StatusCode::INTERNAL_SERVER_ERROR, e.to_string(), &claims)
                .into_response()
        }
    };

    let before = vm.port_forwards.len();
    vm.port_forwards.retain(|f| f.host_port != host_port);
    if vm.port_forwards.len() == before {
        return json_error(
            StatusCode::NOT_FOUND,
            format!("no port forward for host port {} on this VM", host_port),
        )
        .into_response();
    }

    let was_running = matches!(
        vm.state,
        vm_model::VMState::Running | vm_model::VMState::Starting | vm_model::VMState::Paused
    );

    if let Err(e) = state.store.save_vm(&vm) {
        return json_error_safe(StatusCode::INTERNAL_SERVER_ERROR, e.to_string(), &claims)
            .into_response();
    }

    if let Err(e) = state.driver.delete(&name).await {
        let msg = e.to_string();
        if !msg.contains("known to FluxVM") {
            audit(
                &state,
                &claims.sub,
                "REMOVE_PORT_FORWARD",
                &format!("vm/{}", name),
                "FAILED",
            );
            return json_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                format!(
                    "Failed to clear VM '{}' for recreation without the removed port forward: {}",
                    name, msg
                ),
            )
            .into_response();
        }
    }

    if was_running {
        let opts = vm_model::VMStartOptions {
            port_forwards: vm.port_forwards.clone(),
            network_tap: vm.network_tap,
            network_static_ip: vm.network_static_ip,
            ssh_authorized_keys: vm.ssh_authorized_keys.clone(),
            cloud_init_packages: vm.cloud_init_packages.clone(),
            cloud_init_runcmd: vm.cloud_init_runcmd.clone(),
            cloud_init_write_files: vm.cloud_init_write_files.clone(),
            storage: vm.storage.clone(),
            enable_qga: vm.enable_qga,
            hyperv: vm.hyperv,
            direct_uplink: vm.direct_uplink.clone(),
            direct_mode: vm.direct_mode.clone(),
            direct_guest_ips: vm.direct_guest_ips.clone(),
            ..Default::default()
        };
        if let Err(e) = state.driver.start_with_options(&vm, &opts).await {
            audit(
                &state,
                &claims.sub,
                "REMOVE_PORT_FORWARD",
                &format!("vm/{}", name),
                "FAILED",
            );
            return json_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                format!(
                    "Removed the forward but failed to restart VM '{}': {}",
                    name, e
                ),
            )
            .into_response();
        }
        vm.state = vm_model::VMState::Running;
        vm.last_error = None;
        if let Err(e) = state.store.save_vm(&vm) {
            tracing::error!(
                "Failed to save VM state after port-forward removal recreate: {}",
                e
            );
        }
    }

    audit(
        &state,
        &claims.sub,
        "REMOVE_PORT_FORWARD",
        &format!("vm/{}", name),
        "SUCCESS",
    );
    (StatusCode::OK, Json(vm)).into_response()
}

#[derive(Debug, Deserialize)]
pub struct SetDirectUplinkRequest {
    pub uplink: String,
    #[serde(default)]
    pub mode: Option<String>,
    #[serde(default)]
    pub guest_ips: Vec<String>,
}

pub(crate) fn stored_start_options(vm: &vm_model::VM) -> VMStartOptions {
    VMStartOptions {
        port_forwards: vm.port_forwards.clone(),
        network_tap: vm.network_tap,
        network_static_ip: vm.network_static_ip,
        ssh_authorized_keys: vm.ssh_authorized_keys.clone(),
        cloud_init_packages: vm.cloud_init_packages.clone(),
        cloud_init_runcmd: vm.cloud_init_runcmd.clone(),
        cloud_init_write_files: vm.cloud_init_write_files.clone(),
        storage: vm.storage.clone(),
        enable_qga: vm.enable_qga,
        hyperv: vm.hyperv,
        direct_uplink: vm.direct_uplink.clone(),
        direct_mode: vm.direct_mode.clone(),
        direct_guest_ips: vm.direct_guest_ips.clone(),
        ..Default::default()
    }
}

/// PUT /api/vms/:name/direct-uplink — store a bridge-less uplink and drop
/// any FluxVM record so the next start creates it. A running VM is recreated.
pub async fn set_direct_uplink(
    RequireWrite(claims): RequireWrite,
    State(state): State<Arc<AppState>>,
    Path(name): Path<String>,
    Json(req): Json<SetDirectUplinkRequest>,
) -> impl IntoResponse {
    if let Err((status, msg)) = validate_vm_name(&name) {
        return json_error(status, msg).into_response();
    }
    let errors = vm_model::direct_uplink_errors(
        Some(&req.uplink),
        req.mode.as_deref(),
        &req.guest_ips,
        false,
        false,
    );
    if !errors.is_empty() {
        return json_error(StatusCode::BAD_REQUEST, errors.join("; ")).into_response();
    }

    let _lock = state.vm_lock(&name).lock_owned().await;
    let mut vm = match state.store.get_vm(&name) {
        Ok(Some(vm)) => vm,
        Ok(None) => return json_error(StatusCode::NOT_FOUND, "VM not found").into_response(),
        Err(e) => {
            return json_error_safe(StatusCode::INTERNAL_SERVER_ERROR, e.to_string(), &claims)
                .into_response()
        }
    };
    if vm.network_tap || !vm.port_forwards.is_empty() {
        return json_error(
            StatusCode::BAD_REQUEST,
            "direct uplink replaces bridged networking and NAT port forwards; remove those first",
        )
        .into_response();
    }
    let was_running = matches!(
        vm.state,
        vm_model::VMState::Running | vm_model::VMState::Starting | vm_model::VMState::Paused
    );
    vm.direct_uplink = Some(req.uplink.trim().to_string());
    vm.direct_mode = req.mode;
    vm.direct_guest_ips = req.guest_ips;
    if let Err(e) = state.store.save_vm(&vm) {
        return json_error_safe(StatusCode::INTERNAL_SERVER_ERROR, e.to_string(), &claims)
            .into_response();
    }
    if let Err(resp) = clear_fluxvm_record(&state, &claims, &name, "SET_DIRECT_UPLINK").await {
        return resp;
    }
    if was_running {
        let opts = stored_start_options(&vm);
        if let Err(e) = state.driver.start_with_options(&vm, &opts).await {
            return json_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Failed to recreate VM '{name}' with the direct uplink: {e}"),
            )
            .into_response();
        }
        vm.state = vm_model::VMState::Running;
        vm.last_error = None;
        if let Err(e) = state.store.save_vm(&vm) {
            tracing::error!("Failed to save VM state after direct-uplink recreate: {e}");
        }
    }
    (StatusCode::OK, Json(vm)).into_response()
}

/// DELETE /api/vms/:name/direct-uplink — clear a stored uplink and drop the
/// FluxVM record so the next start goes back to user-mode NAT.
pub async fn clear_direct_uplink(
    RequireWrite(claims): RequireWrite,
    State(state): State<Arc<AppState>>,
    Path(name): Path<String>,
) -> impl IntoResponse {
    if let Err((status, msg)) = validate_vm_name(&name) {
        return json_error(status, msg).into_response();
    }
    let _lock = state.vm_lock(&name).lock_owned().await;
    let mut vm = match state.store.get_vm(&name) {
        Ok(Some(vm)) => vm,
        Ok(None) => return json_error(StatusCode::NOT_FOUND, "VM not found").into_response(),
        Err(e) => {
            return json_error_safe(StatusCode::INTERNAL_SERVER_ERROR, e.to_string(), &claims)
                .into_response()
        }
    };
    let was_running = matches!(
        vm.state,
        vm_model::VMState::Running | vm_model::VMState::Starting | vm_model::VMState::Paused
    );
    vm.direct_uplink = None;
    vm.direct_mode = None;
    vm.direct_guest_ips.clear();
    if let Err(e) = state.store.save_vm(&vm) {
        return json_error_safe(StatusCode::INTERNAL_SERVER_ERROR, e.to_string(), &claims)
            .into_response();
    }
    if let Err(resp) = clear_fluxvm_record(&state, &claims, &name, "CLEAR_DIRECT_UPLINK").await {
        return resp;
    }
    if was_running {
        let opts = stored_start_options(&vm);
        if let Err(e) = state.driver.start_with_options(&vm, &opts).await {
            return json_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Failed to recreate VM '{name}' without the direct uplink: {e}"),
            )
            .into_response();
        }
        vm.state = vm_model::VMState::Running;
        vm.last_error = None;
        if let Err(e) = state.store.save_vm(&vm) {
            tracing::error!("Failed to save VM state after clearing direct uplink: {e}");
        }
    }
    (StatusCode::OK, Json(vm)).into_response()
}

#[allow(clippy::result_large_err)]
async fn clear_fluxvm_record(
    state: &AppState,
    claims: &security::Claims,
    name: &str,
    action: &str,
) -> Result<(), axum::response::Response> {
    if let Err(e) = state.driver.delete(name).await {
        let msg = e.to_string();
        if !msg.contains("known to FluxVM") {
            audit(state, &claims.sub, action, &format!("vm/{name}"), "FAILED");
            return Err(json_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Failed to clear VM '{name}' for recreation: {msg}"),
            )
            .into_response());
        }
    }
    Ok(())
}

pub async fn start_vm(
    RequireWrite(claims): RequireWrite,
    State(state): State<Arc<AppState>>,
    Path(name): Path<String>,
    body: Option<Json<VMStartOptions>>,
) -> impl IntoResponse {
    if let Err((status, msg)) = validate_vm_name(&name) {
        return json_error(status, msg).into_response();
    }

    // Acquire per-VM lock to serialize state transitions
    let _lock = state.vm_lock(&name).lock_owned().await;

    // Check current state before transitioning
    if let Ok(Some(vm)) = state.store.get_vm(&name) {
        match vm.state {
            vm_model::VMState::Running
            | vm_model::VMState::Starting
            | vm_model::VMState::Stopping => {
                return json_error(
                    StatusCode::CONFLICT,
                    format!("VM is already {:?}", vm.state),
                )
                .into_response();
            }
            _ => {}
        }
    }

    // If start options provided, validate them eagerly before accepting
    let start_opts = body.map(|j| j.0);
    if let Some(ref opts) = start_opts {
        if let Err(errors) = opts.validate() {
            return json_error(
                StatusCode::BAD_REQUEST,
                format!("Invalid start options: {}", errors.join("; ")),
            )
            .into_response();
        }
    }

    // Reject options/specs the backend is known to refuse, with the same
    // text the pre-flight check reports, instead of a generic async driver
    // failure after the VM was already marked Starting.
    if let Ok(Some(vm)) = state.store.get_vm(&name) {
        let effective = start_opts
            .clone()
            .unwrap_or_else(|| stored_start_options(&vm));
        let caps = crate::preflight::BackendCaps::for_backend(state.driver.backend_name());
        let blockers: Vec<String> = crate::preflight::capability_findings(&vm, &effective, &caps)
            .into_iter()
            .map(|f| format!("{} {}", f.message, f.hint))
            .collect();
        if !blockers.is_empty() {
            return json_error(
                StatusCode::BAD_REQUEST,
                format!("VM cannot start on this backend: {}", blockers.join("; ")),
            )
            .into_response();
        }
    }

    // Mark as starting (not running — that happens after success)
    if let Ok(Some(mut vm)) = state.store.get_vm(&name) {
        vm.state = vm_model::VMState::Starting;
        if let Err(e) = state.store.save_vm(&vm) {
            return json_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Failed to save VM state: {}", e),
            )
            .into_response();
        }
    }

    audit(
        &state,
        &claims.sub,
        "START",
        &format!("vm/{}", name),
        "ACCEPTED",
    );

    // Spawn start in background so API returns immediately.
    // Transfer the lock into the spawned task to avoid a race window
    // where another request could modify VM state.
    let state_clone = state.clone();
    let task_name = name.clone();
    tokio::spawn(async move {
        // _lock is moved into this task and held until it completes
        let _lock = _lock;
        let _ = run_start_task(&state_clone, &task_name, start_opts).await;
    });

    (StatusCode::ACCEPTED, Json(json!({ "status": "starting" }))).into_response()
}

/// The body of a VM start after the VM was marked `Starting` and the caller
/// holds the per-VM lock: launches it through the driver, waits for QMP and
/// records the resulting state and event. Shared by `POST /vms/{name}/start`
/// (which runs it in the background) and autostart reconciliation (which
/// awaits it to report the outcome). The `Err` text is the reason the start
/// did not happen.
pub(crate) async fn run_start_task(
    state: &Arc<AppState>,
    vm_name: &str,
    start_opts: Option<vm_model::VMStartOptions>,
) -> Result<(), String> {
    let shutdown = state.shutdown.clone();

    // Check if shutdown was requested before starting the blocking work
    if shutdown.is_cancelled() {
        tracing::info!("Start task for VM '{}' cancelled due to shutdown", vm_name);
        // Revert state from Starting back to Stopped
        if let Ok(Some(mut vm)) = state.store.get_vm(vm_name) {
            vm.state = vm_model::VMState::Stopped;
            if let Err(e) = state.store.save_vm(&vm) {
                tracing::error!("Failed to save VM state: {}", e);
            }
        }
        return Err("cancelled due to shutdown".to_string());
    }

    // Always go through start_with_options: FluxVM only learns about a
    // VM on its first start (see FluxVmDriver::start_with_options's
    // "First launch" branch, which lazily calls FluxVM's create API).
    // The plain start() has no such fallback and fails with "not known
    // to FluxVM" for any VM that was only ever `POST /vms`-created —
    // which is every VM made through the Create VM wizard, since it
    // never sends start options. Using opts.unwrap_or_default() here
    // routes that common case through the same lazy-create path as an
    // explicit-options start.
    let vm = match state.store.get_vm(vm_name) {
        Ok(Some(vm)) => vm,
        Ok(None) => {
            tracing::error!("VM '{}' not found in store", vm_name);
            return Err("VM not found".to_string());
        }
        Err(e) => {
            tracing::error!("Failed to load VM '{}': {}", vm_name, e);
            return Err(format!("failed to load VM: {e}"));
        }
    };
    // No explicit start body (the plain Start button): fall back to the
    // VM's own stored port_forwards/network_tap (set at Create VM time)
    // rather than a bare default, so a VM created with an exposed SSH
    // port or bridged networking actually gets that applied on its
    // first real launch in FluxVM.
    let opts = start_opts.unwrap_or_else(|| vm_model::VMStartOptions {
        port_forwards: vm.port_forwards.clone(),
        network_tap: vm.network_tap,
        network_static_ip: vm.network_static_ip,
        ssh_authorized_keys: vm.ssh_authorized_keys.clone(),
        cloud_init_packages: vm.cloud_init_packages.clone(),
        cloud_init_runcmd: vm.cloud_init_runcmd.clone(),
        cloud_init_write_files: vm.cloud_init_write_files.clone(),
        storage: vm.storage.clone(),
        enable_qga: vm.enable_qga,
        hyperv: vm.hyperv,
        direct_uplink: vm.direct_uplink.clone(),
        direct_mode: vm.direct_mode.clone(),
        direct_guest_ips: vm.direct_guest_ips.clone(),
        ..Default::default()
    });
    let result = state.driver.start_with_options(&vm, &opts).await;

    match result {
        Ok(_) => {
            // Wait for QMP so "Running" means the monitor is usable
            // (snapshots / hotplug) — FluxVM can report running before
            // qmp.sock exists.
            if let Ok(Some(p)) = state.driver.get_control_socket(vm_name).await {
                let path = p.to_string_lossy().into_owned();
                let _ = tokio::task::spawn_blocking(move || {
                    crate::api::snapshots::wait_for_qmp_connectable(&path, 30, 500)
                })
                .await;
            }
            tracing::info!("VM '{}' started successfully", vm_name);
            if let Ok(Some(mut vm)) = state.store.get_vm(vm_name) {
                vm.state = vm_model::VMState::Running;
                vm.last_error = None; // Clear any previous error
                vm.updated = Some(chrono::Utc::now());
                if let Err(e) = state.store.save_vm(&vm) {
                    tracing::error!("Failed to save VM state: {}", e);
                }
            }
            // Clear autoheal counter on a clean start so a prior crash
            // streak doesn't immediately burn remaining attempts.
            let _ = state.store.delete_entity("autoheal", vm_name);
            crate::api::events::record_event(
                state,
                crate::api::events::VMEventType::Started,
                vm_name,
                None,
            );
        }
        Err(e) => {
            tracing::error!("Failed to start VM '{}': {}", vm_name, e);
            crate::api::events::record_event(
                state,
                crate::api::events::VMEventType::Error,
                vm_name,
                Some(format!("Failed to start: {}", e)),
            );
            if let Ok(Some(mut vm)) = state.store.get_vm(vm_name) {
                vm.state = vm_model::VMState::Failed;
                vm.last_error = Some(e.to_string());
                if let Err(e) = state.store.save_vm(&vm) {
                    tracing::error!("Failed to save VM state: {}", e);
                }
            }
            return Err(e.to_string());
        }
    }
    Ok(())
}

pub async fn stop_vm(
    RequireWrite(claims): RequireWrite,
    State(state): State<Arc<AppState>>,
    Path(name): Path<String>,
) -> impl IntoResponse {
    if let Err((status, msg)) = validate_vm_name(&name) {
        return json_error(status, msg).into_response();
    }
    match stop_core(&state, &claims.sub, &name).await {
        Ok(()) => (StatusCode::OK, Json(json!({ "status": "stopped" }))).into_response(),
        Err(e) => json_error_safe(StatusCode::INTERNAL_SERVER_ERROR, e, &claims).into_response(),
    }
}

/// Power a VM off under its per-VM lock, record state, audit and event.
/// Shared by `POST /vms/{name}/stop` and bulk operations; the `Err` text is
/// the driver's reason.
pub(crate) async fn stop_core(state: &Arc<AppState>, user: &str, name: &str) -> Result<(), String> {
    let _lock = state.vm_lock(name).lock_owned().await;

    match state.driver.poweroff(name).await {
        Ok(_) => {
            if let Ok(Some(mut vm)) = state.store.get_vm(name) {
                vm.state = vm_model::VMState::Stopped;
                vm.updated = Some(chrono::Utc::now());
                if let Err(e) = state.store.save_vm(&vm) {
                    tracing::error!("Failed to save VM state: {}", e);
                }
            }
            audit(state, user, "STOP", &format!("vm/{}", name), "SUCCESS");
            crate::api::events::record_event(
                state,
                crate::api::events::VMEventType::Stopped,
                name,
                None,
            );
            Ok(())
        }
        Err(e) => {
            audit(state, user, "STOP", &format!("vm/{}", name), "FAILED");
            Err(e.to_string())
        }
    }
}

pub async fn restart_vm(
    RequireWrite(claims): RequireWrite,
    State(state): State<Arc<AppState>>,
    Path(name): Path<String>,
) -> impl IntoResponse {
    if let Err((status, msg)) = validate_vm_name(&name) {
        return json_error(status, msg).into_response();
    }

    let _lock = state.vm_lock(&name).lock_owned().await;

    let mut vm = match state.store.get_vm(&name) {
        Ok(Some(vm)) => vm,
        Ok(None) => return json_error(StatusCode::NOT_FOUND, "VM not found").into_response(),
        Err(e) => {
            return json_error_safe(StatusCode::INTERNAL_SERVER_ERROR, e.to_string(), &claims)
                .into_response()
        }
    };

    // `driver.reboot()` (FluxVM stop+start, replaying its own stored
    // launch request) used to be used here directly -- found live: any
    // cloud-init/ssh-key change made via configure_cloud_init *after* a
    // VM's first boot silently never took effect on a later restart,
    // contradicting the Cloud-init tab's own "applied on next (re)start"
    // promise. `FluxVmDriver::start_with_options` only re-translates
    // fresh options for a VM FluxVM doesn't already know about --
    // for an existing record it just replays what was stored at creation
    // time. Same delete-then-recreate fix already used by
    // add_port_forward/remove_port_forward: clear the stale FluxVM
    // record first so the restart actually picks up this VM's current
    // fields, not its original ones.
    if let Err(e) = state.driver.delete(&name).await {
        let msg = e.to_string();
        if !msg.contains("known to FluxVM") {
            audit(
                &state,
                &claims.sub,
                "RESTART",
                &format!("vm/{}", name),
                "FAILED",
            );
            return json_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Failed to clear VM '{}' for restart: {}", name, msg),
            )
            .into_response();
        }
    }

    let opts = vm_model::VMStartOptions {
        port_forwards: vm.port_forwards.clone(),
        network_tap: vm.network_tap,
        network_static_ip: vm.network_static_ip,
        ssh_authorized_keys: vm.ssh_authorized_keys.clone(),
        cloud_init_packages: vm.cloud_init_packages.clone(),
        cloud_init_runcmd: vm.cloud_init_runcmd.clone(),
        cloud_init_write_files: vm.cloud_init_write_files.clone(),
        storage: vm.storage.clone(),
        enable_qga: vm.enable_qga,
        hyperv: vm.hyperv,
        direct_uplink: vm.direct_uplink.clone(),
        direct_mode: vm.direct_mode.clone(),
        direct_guest_ips: vm.direct_guest_ips.clone(),
        ..Default::default()
    };

    match state.driver.start_with_options(&vm, &opts).await {
        Ok(_) => {
            vm.state = vm_model::VMState::Running;
            vm.last_error = None;
            vm.updated = Some(chrono::Utc::now());
            if let Err(e) = state.store.save_vm(&vm) {
                tracing::error!("Failed to save VM state: {}", e);
            }
            audit(
                &state,
                &claims.sub,
                "RESTART",
                &format!("vm/{}", name),
                "SUCCESS",
            );
            (StatusCode::OK, Json(json!({ "status": "restarted" }))).into_response()
        }
        Err(e) => {
            audit(
                &state,
                &claims.sub,
                "RESTART",
                &format!("vm/{}", name),
                "FAILED",
            );
            json_error_safe(StatusCode::INTERNAL_SERVER_ERROR, e.to_string(), &claims)
                .into_response()
        }
    }
}

pub async fn pause_vm(
    RequireWrite(claims): RequireWrite,
    State(state): State<Arc<AppState>>,
    Path(name): Path<String>,
) -> impl IntoResponse {
    if let Err((status, msg)) = validate_vm_name(&name) {
        return json_error(status, msg).into_response();
    }

    let _lock = state.vm_lock(&name).lock_owned().await;

    let result = state.driver.pause(&name).await;

    match result {
        Ok(_) => {
            if let Ok(Some(mut vm)) = state.store.get_vm(&name) {
                vm.state = vm_model::VMState::Paused;
                vm.updated = Some(chrono::Utc::now());
                if let Err(e) = state.store.save_vm(&vm) {
                    tracing::error!("Failed to save VM state: {}", e);
                }
            }
            audit(
                &state,
                &claims.sub,
                "PAUSE",
                &format!("vm/{}", name),
                "SUCCESS",
            );
            crate::api::events::record_event(
                &state,
                crate::api::events::VMEventType::Paused,
                &name,
                None,
            );
            (StatusCode::OK, Json(json!({ "status": "paused" }))).into_response()
        }
        Err(e) => json_error_safe(StatusCode::INTERNAL_SERVER_ERROR, e.to_string(), &claims)
            .into_response(),
    }
}

pub async fn resume_vm(
    RequireWrite(claims): RequireWrite,
    State(state): State<Arc<AppState>>,
    Path(name): Path<String>,
) -> impl IntoResponse {
    if let Err((status, msg)) = validate_vm_name(&name) {
        return json_error(status, msg).into_response();
    }

    let _lock = state.vm_lock(&name).lock_owned().await;

    let result = state.driver.resume(&name).await;

    match result {
        Ok(_) => {
            if let Ok(Some(mut vm)) = state.store.get_vm(&name) {
                vm.state = vm_model::VMState::Running;
                vm.updated = Some(chrono::Utc::now());
                if let Err(e) = state.store.save_vm(&vm) {
                    tracing::error!("Failed to save VM state: {}", e);
                }
            }
            audit(
                &state,
                &claims.sub,
                "RESUME",
                &format!("vm/{}", name),
                "SUCCESS",
            );
            crate::api::events::record_event(
                &state,
                crate::api::events::VMEventType::Resumed,
                &name,
                None,
            );
            (StatusCode::OK, Json(json!({ "status": "running" }))).into_response()
        }
        Err(e) => json_error_safe(StatusCode::INTERNAL_SERVER_ERROR, e.to_string(), &claims)
            .into_response(),
    }
}

#[derive(serde::Deserialize)]
pub struct CloneVMRequest {
    pub target_name: String,
    #[serde(default)]
    pub linked_clone: bool,
}

pub async fn clone_vm(
    RequireWrite(claims): RequireWrite,
    State(state): State<Arc<AppState>>,
    Path(source_name): Path<String>,
    Json(req): Json<CloneVMRequest>,
) -> impl IntoResponse {
    if let Err((status, msg)) = validate_vm_name(&source_name) {
        return json_error(status, msg).into_response();
    }
    if let Err((status, msg)) = validate_vm_name(&req.target_name) {
        return json_error(status, msg).into_response();
    }

    // Prevent cloning to same name
    if source_name == req.target_name {
        return json_error(
            StatusCode::BAD_REQUEST,
            "Source and target VM names must be different",
        )
        .into_response();
    }

    // Lock both VMs in canonical (lexicographic) order to prevent deadlock
    let (_first_lock, _second_lock) = if source_name < req.target_name {
        let first = state.vm_lock(&source_name).lock_owned().await;
        let second = state.vm_lock(&req.target_name).lock_owned().await;
        (first, second)
    } else {
        let first = state.vm_lock(&req.target_name).lock_owned().await;
        let second = state.vm_lock(&source_name).lock_owned().await;
        (first, second)
    };

    // Check source VM exists
    let source_vm = match state.store.get_vm(&source_name) {
        Ok(Some(vm)) => vm,
        Ok(None) => {
            return json_error(StatusCode::NOT_FOUND, "Source VM not found").into_response();
        }
        Err(e) => {
            return json_error(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response();
        }
    };

    // Prevent linked clone while source VM is running
    if req.linked_clone && source_vm.state == vm_model::VMState::Running {
        return json_error(
            StatusCode::CONFLICT,
            "Cannot create a linked clone while the source VM is running",
        )
        .into_response();
    }

    // Check target name not taken
    if let Ok(Some(_)) = state.store.get_vm(&req.target_name) {
        return json_error(StatusCode::CONFLICT, "Target VM name already exists").into_response();
    }

    // The source VM's actual, live disk (not a naming-convention guess --
    // see VMDriver::get_disk_path's doc comment). This is exactly the case
    // that guessing gets wrong: a VM cloned from a shared base image isn't
    // named after its own disk file.
    let src_path = match state.driver.get_disk_path(&source_name).await {
        Ok(p) => p.display().to_string(),
        Err(driver_err) => match crate::validation::find_vm_image(&source_name) {
            Some(p) => p,
            None => {
                return json_error(
                    StatusCode::NOT_FOUND,
                    format!(
                        "No disk image found for source VM '{}': {}",
                        source_name, driver_err
                    ),
                )
                .into_response();
            }
        },
    };

    // Build target path using proper Path API
    let src = std::path::Path::new(&src_path);
    let target_path = src
        .with_file_name(format!("{}.qcow2", req.target_name))
        .to_string_lossy()
        .to_string();

    // Ensure target directory exists
    if let Some(parent) = std::path::Path::new(&target_path).parent() {
        if let Err(e) = tokio::fs::create_dir_all(parent).await {
            return json_error_safe(
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Failed to create directory: {}", e),
                &claims,
            )
            .into_response();
        }
    }

    let result = if req.linked_clone {
        tokio::process::Command::new("qemu-img")
            .args([
                "create",
                "-f",
                "qcow2",
                "-b",
                &src_path,
                "-F",
                "qcow2",
                &target_path,
            ])
            .output()
            .await
    } else {
        tokio::process::Command::new("cp")
            .args(["--reflink=auto", &src_path, &target_path])
            .output()
            .await
    };

    match result {
        Ok(output) if !output.status.success() => {
            let stderr = String::from_utf8_lossy(&output.stderr);
            audit(
                &state,
                &claims.sub,
                "CLONE",
                &format!("vm/{}->{}", source_name, req.target_name),
                "FAILED",
            );
            return json_error_safe(
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Failed to clone disk: {}", stderr),
                &claims,
            )
            .into_response();
        }
        Err(e) => {
            audit(
                &state,
                &claims.sub,
                "CLONE",
                &format!("vm/{}->{}", source_name, req.target_name),
                "FAILED",
            );
            return json_error_safe(
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Failed to clone disk: {}", e),
                &claims,
            )
            .into_response();
        }
        _ => {}
    }

    // Create new VM entry
    let mut new_vm = source_vm.clone();
    new_vm.name = req.target_name.clone();
    new_vm.state = vm_model::VMState::Stopped;
    new_vm.pid = None;
    new_vm.ip = None;
    new_vm.created = chrono::Utc::now();
    new_vm.updated = Some(chrono::Utc::now());

    match state.store.save_vm(&new_vm) {
        Ok(_) => {
            audit(
                &state,
                &claims.sub,
                "CLONE",
                &format!("vm/{}->{}", source_name, req.target_name),
                "SUCCESS",
            );
            crate::api::events::record_event(
                &state,
                crate::api::events::VMEventType::Cloned,
                &req.target_name,
                Some(format!("Cloned from {}", source_name)),
            );
            (StatusCode::CREATED, Json(new_vm)).into_response()
        }
        Err(e) => json_error(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    }
}

pub async fn get_metrics(
    RequireRead(_claims): RequireRead,
    State(state): State<Arc<AppState>>,
    Path(name): Path<String>,
) -> impl IntoResponse {
    if let Err((status, msg)) = validate_vm_name(&name) {
        return json_error(status, msg).into_response();
    }
    match state.store.get_vm(&name) {
        Ok(None) => return json_error(StatusCode::NOT_FOUND, "VM not found").into_response(),
        Err(e) => {
            return json_error(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response()
        }
        Ok(Some(_)) => {}
    }
    match state.driver.get_metrics(&name).await {
        Ok(metrics) => (StatusCode::OK, Json(metrics)).into_response(),
        Err(e) => {
            // Metadata-only / stopped VMs not yet registered in FluxVM have no cgroup stats.
            tracing::debug!("metrics unavailable for '{}': {}", name, e);
            (
                StatusCode::OK,
                Json(vm_model::VMMetrics {
                    cpu_usage: 0.0,
                    memory_usage: 0,
                    disk_usage: 0,
                    network_rx: 0,
                    network_tx: 0,
                }),
            )
                .into_response()
        }
    }
}

pub(crate) fn resolve_image_source(state: &AppState, image: &str) -> std::path::PathBuf {
    let path = std::path::Path::new(image);
    if path.is_absolute() {
        return path.to_path_buf();
    }
    let from_config = std::path::PathBuf::from(&state.config.storage.image_path).join(image);
    if from_config.exists() {
        return from_config;
    }
    for prefix in [
        "/var/lib/zyvor-fabricd/images",
        "/var/lib/fluxvm/images",
        "/var/lib/machines",
    ] {
        let candidate = std::path::PathBuf::from(prefix).join(image);
        if candidate.exists() {
            return candidate;
        }
    }
    from_config
}

async fn provision_vm_disk(state: &AppState, vm: &VM) -> Result<(), String> {
    let vm_name = input_guard::vet_component!(&vm.name, "invalid VM name".to_string());
    let image = input_guard::vet!(&vm.image, "invalid image path".to_string());
    let target = crate::validation::find_vm_image_or_default(vm_name);
    let target = input_guard::vet!(&target, "rejected target path".to_string());
    if std::path::Path::new(target).exists() {
        return Ok(());
    }
    if let Some(parent) = std::path::Path::new(&target).parent() {
        tokio::fs::create_dir_all(parent)
            .await
            .map_err(|e| e.to_string())?;
    }

    let source = resolve_image_source(state, image);
    if source.exists() {
        let source_s = source
            .to_str()
            .ok_or_else(|| "invalid source path".to_string())?;
        let source_s = input_guard::vet!(source_s, "rejected source path".to_string());
        let output =
            input_guard::argv_checked!(source_s, "rejected source path".to_string(), |source_s| {
                input_guard::argv_checked!(target, "rejected target path".to_string(), |target| {
                    tokio::process::Command::new("cp")
                        .args(["--reflink=auto", source_s, target])
                        .output()
                        .await
                        .map_err(|e| e.to_string())?
                })
            });
        if !output.status.success() {
            return Err(String::from_utf8_lossy(&output.stderr).to_string());
        }
        return Ok(());
    }

    let size_gib = if vm.disk > 0 { vm.disk } else { 20 };
    let size_arg = format!("{size_gib}G");
    let size_arg = input_guard::vet!(&size_arg, "invalid disk size".to_string());
    let output =
        input_guard::argv_checked!(&size_arg, "invalid disk size".to_string(), |size_arg| {
            input_guard::argv_checked!(target, "rejected target path".to_string(), |target| {
                tokio::process::Command::new("qemu-img")
                    .args(["create", "-f", "qcow2", target, size_arg])
                    .output()
                    .await
                    .map_err(|e| e.to_string())?
            })
        });
    if !output.status.success() {
        return Err(String::from_utf8_lossy(&output.stderr).to_string());
    }
    Ok(())
}

/// Pulls `ssh_authorized_keys` out of arbitrary cloud-config YAML -- either
/// top-level (`ssh_authorized_keys: [...]`) or nested under `users[]`
/// (the more common shape, matching CloudInitTab.tsx's default template).
/// Best-effort: invalid/unparseable YAML just yields no keys rather than
/// erroring the whole request, since user_data is free-text the user edits
/// directly and may be mid-edit or intentionally minimal.
fn extract_ssh_keys_from_user_data(user_data: &str) -> Vec<String> {
    let Ok(doc) = serde_yaml::from_str::<serde_yaml::Value>(user_data) else {
        return Vec::new();
    };
    let mut keys = Vec::new();
    let mut collect = |v: &serde_yaml::Value| {
        if let Some(seq) = v.as_sequence() {
            keys.extend(seq.iter().filter_map(|k| k.as_str().map(String::from)));
        }
    };
    if let Some(v) = doc.get("ssh_authorized_keys") {
        collect(v);
    }
    if let Some(users) = doc.get("users").and_then(|v| v.as_sequence()) {
        for user in users {
            if let Some(v) = user.get("ssh_authorized_keys") {
                collect(v);
            }
        }
    }
    keys
}

/// Pulls `packages` (top-level `packages:` list) out of arbitrary
/// cloud-config YAML. Best-effort, same reasoning as
/// `extract_ssh_keys_from_user_data`.
fn extract_packages_from_user_data(user_data: &str) -> Vec<String> {
    let Ok(doc) = serde_yaml::from_str::<serde_yaml::Value>(user_data) else {
        return Vec::new();
    };
    doc.get("packages")
        .and_then(|v| v.as_sequence())
        .map(|seq| {
            seq.iter()
                .filter_map(|p| p.as_str().map(String::from))
                .collect()
        })
        .unwrap_or_default()
}

/// Pulls `runcmd` (top-level `runcmd:` list) out of arbitrary cloud-config
/// YAML. Each entry may be a plain string or a YAML sequence
/// (`[bash, -lc, "..."]`); sequence entries are rejoined with spaces since
/// FluxVM's own `CloudInitSpec.runcmd` is a flat `Vec<String>` of shell
/// commands, not argv arrays.
fn extract_runcmd_from_user_data(user_data: &str) -> Vec<String> {
    let Ok(doc) = serde_yaml::from_str::<serde_yaml::Value>(user_data) else {
        return Vec::new();
    };
    let Some(seq) = doc.get("runcmd").and_then(|v| v.as_sequence()) else {
        return Vec::new();
    };
    seq.iter()
        .filter_map(|entry| {
            if let Some(s) = entry.as_str() {
                Some(s.to_string())
            } else {
                entry.as_sequence().map(|parts| {
                    parts
                        .iter()
                        .filter_map(|p| p.as_str())
                        .collect::<Vec<_>>()
                        .join(" ")
                })
            }
        })
        .collect()
}

/// Pulls `write_files` (top-level `write_files:` list, each with `path` and
/// `content`) out of arbitrary cloud-config YAML. Entries missing either
/// field are skipped rather than erroring the whole request.
fn extract_write_files_from_user_data(user_data: &str) -> Vec<vm_model::CloudInitFile> {
    let Ok(doc) = serde_yaml::from_str::<serde_yaml::Value>(user_data) else {
        return Vec::new();
    };
    let Some(seq) = doc.get("write_files").and_then(|v| v.as_sequence()) else {
        return Vec::new();
    };
    seq.iter()
        .filter_map(|entry| {
            let path = entry.get("path")?.as_str()?.to_string();
            let content = entry.get("content")?.as_str()?.to_string();
            let permissions = entry
                .get("permissions")
                .and_then(|v| v.as_str())
                .map(String::from);
            Some(vm_model::CloudInitFile {
                path,
                content,
                permissions,
            })
        })
        .collect()
}

/// `hostname`, `ssh_authorized_keys`, `packages`, `runcmd`, and
/// `write_files` found in `user_data` are all persisted onto the VM record
/// and applied on its next (re)launch, via FluxVM's own CloudInitSpec (see
/// FluxVmDriver::translate_start_options). Persisting even just the empty
/// case matters beyond the values themselves: any cloud-init config at all
/// is what makes FluxVM attach a cloud-init seed disk, which is what lets
/// cloud-init find a datasource and run its own default network config --
/// without one, a guest's DHCP client never comes up (found live: this
/// endpoint used to write an ISO no VM ever received, so cloud-init and
/// therefore networking silently never worked for any VM here).
pub async fn configure_cloud_init(
    RequireWrite(_claims): RequireWrite,
    State(state): State<Arc<AppState>>,
    Path(vm_name): Path<String>,
    Json(config): Json<CloudInitConfig>,
) -> impl IntoResponse {
    if let Err((status, msg)) = validate_vm_name(&vm_name) {
        return json_error(status, msg).into_response();
    }

    if let Ok(Some(mut vm)) = state.store.get_vm(&vm_name) {
        vm.hostname = Some(config.hostname.clone());
        if let Some(ref user_data) = config.user_data {
            let keys = extract_ssh_keys_from_user_data(user_data);
            if !keys.is_empty() {
                vm.ssh_authorized_keys = keys;
            }
            vm.cloud_init_packages = extract_packages_from_user_data(user_data);
            vm.cloud_init_runcmd = extract_runcmd_from_user_data(user_data);
            vm.cloud_init_write_files = extract_write_files_from_user_data(user_data);
        }
        if let Err(e) = state.store.save_vm(&vm) {
            tracing::warn!(
                "Failed to persist cloud-init settings for VM '{}': {}",
                vm_name,
                e
            );
        }
    }

    let generator = match CloudInitGenerator::new("/var/lib/zyvor-fabricd/cloud-init") {
        Ok(gen) => gen,
        Err(e) => {
            return json_error(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response();
        }
    };

    match generator.generate(&config) {
        Ok(iso_path) => (
            StatusCode::OK,
            Json(json!({
                "status": "created",
                "iso_path": iso_path.to_string_lossy()
            })),
        )
            .into_response(),
        Err(e) => json_error(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    }
}
