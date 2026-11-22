// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: LicenseRef-Zyvor-Production-2.0

use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use chrono::Utc;
use std::sync::Arc;
use uuid::Uuid;

use crate::server::AppState;
use networking::models::{
    AdoptHostRequest, BondConfig, BridgeConfig, CreateBondRequest, CreateBridgeRequest,
    CreateLinkFileRequest, CreateMacvtapRequest, CreateNetworkFileRequest,
    CreatePortForwardRequest, CreateSriovRequest, CreateTapRequest, CreateVlanRequest,
    CreateVxlanRequest, LinkFileConfig, MacvtapConfig, NetworkFileConfig, PortForwardConfig,
    SriovConfig, TapConfig, VlanConfig, VxlanConfig,
};
use networking::nftables::NftManager;
use networking::NetworkdManager;
use security::{RequireAdmin, RequireRead, RequireWrite};

pub(crate) fn networkd_manager(state: &AppState) -> NetworkdManager {
    NetworkdManager::new(
        &state.config.network.networkd_config_dir,
        &state.config.network.networkd_file_prefix,
    )
}

fn reject_host_delete(id: &str, kind: &str) -> Option<axum::response::Response> {
    if super::networkd_discover::is_host_managed_id(id) {
        Some(
            crate::api_error::json_error(
                StatusCode::BAD_REQUEST,
                format!(
                    "This {} exists on the host and is not managed by zyvor-fabricd",
                    kind
                ),
            )
            .into_response(),
        )
    } else {
        None
    }
}

// ============================================================================
// Bridge handlers
// ============================================================================

pub async fn list_bridges(
    RequireRead(_claims): RequireRead,
    State(state): State<Arc<AppState>>,
) -> impl IntoResponse {
    tracing::debug!("networkd::{}", stringify!(list_bridges));
    let items: Vec<BridgeConfig> = state
        .store
        .list_entities("networkd_bridges")
        .unwrap_or_else(|e| {
            tracing::error!("Storage error: {}", e);
            Vec::new()
        });
    Json(super::networkd_discover::merge_bridges(&state, items))
}

pub async fn create_bridge(
    RequireWrite(_claims): RequireWrite,
    State(state): State<Arc<AppState>>,
    Json(req): Json<CreateBridgeRequest>,
) -> impl IntoResponse {
    tracing::debug!("networkd::{}", stringify!(create_bridge));
    if let Err((status, msg)) = crate::validation::validate_vm_name(&req.name) {
        return crate::api_error::json_error(status, msg).into_response();
    }
    let now = Utc::now().to_rfc3339();
    let cfg = BridgeConfig {
        id: Uuid::new_v4().to_string(),
        name: req.name,
        stp: req.stp,
        forward_delay_sec: req.forward_delay_sec,
        hello_time_sec: req.hello_time_sec,
        max_age_sec: req.max_age_sec,
        vlan_filtering: req.vlan_filtering,
        mtu: req.mtu,
        mac_address: req.mac_address,
        addresses: req.addresses,
        gateway: req.gateway,
        dns: req.dns,
        dhcp: req.dhcp,
        created: now.clone(),
        updated: now,
        managed: true,
        operational_state: None,
    };

    let mgr = networkd_manager(&state);
    if let Err(e) = mgr.apply_bridge(&cfg) {
        // `e.to_string()` (used for the client-facing error below) only
        // shows the outermost `.with_context()` message -- anyhow drops
        // the rest of the chain unless printed with `{:#}` or `{:?}`.
        // Found live: a bridge-create failure logged nothing beyond
        // "failed to create bridge 'X'" both to the client AND (before
        // this line existed) to the server, with the actual root cause
        // invisible anywhere.
        tracing::error!(bridge = %cfg.name, error = %format!("{e:#}"), "Failed to apply bridge config");
        return crate::api_error::json_error(StatusCode::INTERNAL_SERVER_ERROR, e.to_string())
            .into_response();
    }

    match state.store.save_entity("networkd_bridges", &cfg.id, &cfg) {
        Ok(_) => {
            if let Err(e) = mgr.reload() {
                tracing::warn!("Failed to reload networkd: {}", e);
            }
            (StatusCode::CREATED, Json(cfg)).into_response()
        }
        Err(e) => crate::api_error::json_error(StatusCode::INTERNAL_SERVER_ERROR, e.to_string())
            .into_response(),
    }
}

pub async fn get_bridge(
    RequireRead(_claims): RequireRead,
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    tracing::debug!("networkd::{}", stringify!(get_bridge));
    match state
        .store
        .get_entity::<BridgeConfig>("networkd_bridges", &id)
    {
        Ok(Some(b)) => Json(b).into_response(),
        Ok(None) => {
            if super::networkd_discover::is_host_managed_id(&id) {
                if let Some(b) = super::networkd_discover::find_host_bridge(&state, &id) {
                    return Json(b).into_response();
                }
            }
            crate::api_error::json_error(StatusCode::NOT_FOUND, "Bridge not found").into_response()
        }
        Err(_) => {
            crate::api_error::json_error(StatusCode::INTERNAL_SERVER_ERROR, "Failed to load bridge")
                .into_response()
        }
    }
}

pub async fn update_bridge(
    RequireWrite(_claims): RequireWrite,
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    Json(req): Json<CreateBridgeRequest>,
) -> impl IntoResponse {
    tracing::debug!("networkd::{}", stringify!(update_bridge));
    if let Err((status, msg)) = crate::validation::validate_vm_name(&req.name) {
        return crate::api_error::json_error(status, msg).into_response();
    }
    let existing = match state
        .store
        .get_entity::<BridgeConfig>("networkd_bridges", &id)
    {
        Ok(Some(b)) => b,
        Ok(None) => {
            return crate::api_error::json_error(StatusCode::NOT_FOUND, "Bridge not found")
                .into_response()
        }
        Err(_) => {
            return crate::api_error::json_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                "Failed to load bridge",
            )
            .into_response()
        }
    };

    let mgr = networkd_manager(&state);

    // Remove old config files if name changed
    if existing.name != req.name {
        if let Err(e) = mgr.remove_device(&existing.name) {
            tracing::warn!("Failed to remove device: {}", e);
        }
    }

    let cfg = BridgeConfig {
        id: id.clone(),
        name: req.name,
        stp: req.stp,
        forward_delay_sec: req.forward_delay_sec,
        hello_time_sec: req.hello_time_sec,
        max_age_sec: req.max_age_sec,
        vlan_filtering: req.vlan_filtering,
        mtu: req.mtu,
        mac_address: req.mac_address,
        addresses: req.addresses,
        gateway: req.gateway,
        dns: req.dns,
        dhcp: req.dhcp,
        created: existing.created,
        updated: Utc::now().to_rfc3339(),
        managed: existing.managed,
        operational_state: existing.operational_state,
    };

    if let Err(e) = mgr.apply_bridge(&cfg) {
        return crate::api_error::json_error(StatusCode::INTERNAL_SERVER_ERROR, e.to_string())
            .into_response();
    }

    match state.store.save_entity("networkd_bridges", &cfg.id, &cfg) {
        Ok(_) => {
            if let Err(e) = mgr.reload() {
                tracing::warn!("Failed to reload networkd: {}", e);
            }
            Json(cfg).into_response()
        }
        Err(e) => crate::api_error::json_error(StatusCode::INTERNAL_SERVER_ERROR, e.to_string())
            .into_response(),
    }
}

pub async fn adopt_bridge(
    RequireWrite(_claims): RequireWrite,
    State(state): State<Arc<AppState>>,
    Json(req): Json<AdoptHostRequest>,
) -> impl IntoResponse {
    tracing::debug!("networkd::{}", stringify!(adopt_bridge));
    if !super::networkd_discover::is_host_managed_id(&req.host_id) {
        return crate::api_error::json_error(
            StatusCode::BAD_REQUEST,
            "host_id must be a host-discovered bridge (host:…)",
        )
        .into_response();
    }
    let host = match super::networkd_discover::find_host_bridge(&state, &req.host_id) {
        Some(b) => b,
        None => {
            return crate::api_error::json_error(StatusCode::NOT_FOUND, "Host bridge not found")
                .into_response()
        }
    };
    let stored: Vec<BridgeConfig> = state
        .store
        .list_entities("networkd_bridges")
        .unwrap_or_default();
    if stored.iter().any(|b| b.name == host.name) {
        return crate::api_error::json_error(
            StatusCode::CONFLICT,
            format!("Bridge '{}' is already managed by zyvor-fabricd", host.name),
        )
        .into_response();
    }

    let now = Utc::now().to_rfc3339();
    let cfg = BridgeConfig {
        id: Uuid::new_v4().to_string(),
        name: host.name,
        stp: host.stp,
        forward_delay_sec: host.forward_delay_sec,
        hello_time_sec: host.hello_time_sec,
        max_age_sec: host.max_age_sec,
        vlan_filtering: host.vlan_filtering,
        mtu: host.mtu,
        mac_address: host.mac_address,
        addresses: host.addresses,
        gateway: host.gateway,
        dns: host.dns,
        dhcp: host.dhcp,
        created: now.clone(),
        updated: now,
        managed: true,
        operational_state: host.operational_state,
    };

    let mgr = networkd_manager(&state);
    if let Err(e) = mgr.apply_bridge(&cfg) {
        return crate::api_error::json_error(StatusCode::INTERNAL_SERVER_ERROR, e.to_string())
            .into_response();
    }
    match state.store.save_entity("networkd_bridges", &cfg.id, &cfg) {
        Ok(_) => {
            if let Err(e) = mgr.reload() {
                tracing::warn!("Failed to reload networkd: {}", e);
            }
            (StatusCode::CREATED, Json(cfg)).into_response()
        }
        Err(e) => crate::api_error::json_error(StatusCode::INTERNAL_SERVER_ERROR, e.to_string())
            .into_response(),
    }
}

pub async fn delete_bridge(
    RequireAdmin(_claims): RequireAdmin,
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    tracing::debug!("networkd::{}", stringify!(delete_bridge));
    if id.starts_with("host:") {
        return crate::api_error::json_error(
            StatusCode::BAD_REQUEST,
            "This bridge exists on the host (e.g. libvirt virbr0) and is not managed by zyvor-fabricd",
        )
        .into_response();
    }
    let mgr = networkd_manager(&state);
    if let Ok(Some(cfg)) = state
        .store
        .get_entity::<BridgeConfig>("networkd_bridges", &id)
    {
        if let Err(e) = mgr.remove_device(&cfg.name) {
            tracing::warn!("Failed to remove device: {}", e);
        }
        if let Err(e) = mgr.reload() {
            tracing::warn!("Failed to reload networkd: {}", e);
        }
    }
    if let Err(e) = state.store.delete_entity("networkd_bridges", &id) {
        tracing::error!("Failed to delete entity: {}", e);
    }
    StatusCode::NO_CONTENT.into_response()
}

// ============================================================================
// VLAN handlers
// ============================================================================

pub async fn list_vlans(
    RequireRead(_claims): RequireRead,
    State(state): State<Arc<AppState>>,
) -> impl IntoResponse {
    tracing::debug!("networkd::{}", stringify!(list_vlans));
    let items: Vec<VlanConfig> = state
        .store
        .list_entities("networkd_vlans")
        .unwrap_or_else(|e| {
            tracing::error!("Storage error: {}", e);
            Vec::new()
        });
    Json(super::networkd_discover::merge_vlans(&state, items))
}

pub async fn create_vlan(
    RequireWrite(_claims): RequireWrite,
    State(state): State<Arc<AppState>>,
    Json(req): Json<CreateVlanRequest>,
) -> impl IntoResponse {
    tracing::debug!("networkd::{}", stringify!(create_vlan));
    if let Err((status, msg)) = crate::validation::validate_vm_name(&req.name) {
        return crate::api_error::json_error(status, msg).into_response();
    }
    if let Err(msg) = crate::validation::validate_hostname(&req.parent_interface) {
        return crate::api_error::json_error(
            StatusCode::BAD_REQUEST,
            format!("Invalid parent interface: {}", msg),
        )
        .into_response();
    }
    let now = Utc::now().to_rfc3339();
    let cfg = VlanConfig {
        id: Uuid::new_v4().to_string(),
        name: req.name,
        vlan_id: req.vlan_id,
        parent_interface: req.parent_interface,
        mtu: req.mtu,
        addresses: req.addresses,
        gateway: req.gateway,
        dns: req.dns,
        dhcp: req.dhcp,
        created: now.clone(),
        updated: now,
        managed: true,
        operational_state: None,
    };

    let mgr = networkd_manager(&state);
    if let Err(e) = mgr.apply_vlan(&cfg) {
        return crate::api_error::json_error(StatusCode::INTERNAL_SERVER_ERROR, e.to_string())
            .into_response();
    }

    match state.store.save_entity("networkd_vlans", &cfg.id, &cfg) {
        Ok(_) => {
            if let Err(e) = mgr.reload() {
                tracing::warn!("Failed to reload networkd: {}", e);
            }
            (StatusCode::CREATED, Json(cfg)).into_response()
        }
        Err(e) => crate::api_error::json_error(StatusCode::INTERNAL_SERVER_ERROR, e.to_string())
            .into_response(),
    }
}

pub async fn get_vlan(
    RequireRead(_claims): RequireRead,
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    tracing::debug!("networkd::{}", stringify!(get_vlan));
    match state.store.get_entity::<VlanConfig>("networkd_vlans", &id) {
        Ok(Some(v)) => Json(v).into_response(),
        Ok(None) => {
            if super::networkd_discover::is_host_managed_id(&id) {
                if let Some(v) = super::networkd_discover::find_host_vlan(&state, &id) {
                    return Json(v).into_response();
                }
            }
            crate::api_error::json_error(StatusCode::NOT_FOUND, "VLAN not found").into_response()
        }
        Err(_) => {
            crate::api_error::json_error(StatusCode::INTERNAL_SERVER_ERROR, "Failed to load VLAN")
                .into_response()
        }
    }
}

pub async fn update_vlan(
    RequireWrite(_claims): RequireWrite,
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    Json(req): Json<CreateVlanRequest>,
) -> impl IntoResponse {
    tracing::debug!("networkd::{}", stringify!(update_vlan));
    if let Err((status, msg)) = crate::validation::validate_vm_name(&req.name) {
        return crate::api_error::json_error(status, msg).into_response();
    }
    if let Err(msg) = crate::validation::validate_hostname(&req.parent_interface) {
        return crate::api_error::json_error(
            StatusCode::BAD_REQUEST,
            format!("Invalid parent interface: {}", msg),
        )
        .into_response();
    }
    let existing = match state.store.get_entity::<VlanConfig>("networkd_vlans", &id) {
        Ok(Some(v)) => v,
        Ok(None) => {
            return crate::api_error::json_error(StatusCode::NOT_FOUND, "VLAN not found")
                .into_response()
        }
        Err(_) => {
            return crate::api_error::json_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                "Failed to load VLAN",
            )
            .into_response()
        }
    };

    let mgr = networkd_manager(&state);
    if existing.name != req.name {
        if let Err(e) = mgr.remove_device(&existing.name) {
            tracing::warn!("Failed to remove device: {}", e);
        }
    }

    let cfg = VlanConfig {
        id: id.clone(),
        name: req.name,
        vlan_id: req.vlan_id,
        parent_interface: req.parent_interface,
        mtu: req.mtu,
        addresses: req.addresses,
        gateway: req.gateway,
        dns: req.dns,
        dhcp: req.dhcp,
        created: existing.created,
        updated: Utc::now().to_rfc3339(),
        managed: existing.managed,
        operational_state: existing.operational_state,
    };

    if let Err(e) = mgr.apply_vlan(&cfg) {
        return crate::api_error::json_error(StatusCode::INTERNAL_SERVER_ERROR, e.to_string())
            .into_response();
    }

    match state.store.save_entity("networkd_vlans", &cfg.id, &cfg) {
        Ok(_) => {
            if let Err(e) = mgr.reload() {
                tracing::warn!("Failed to reload networkd: {}", e);
            }
            Json(cfg).into_response()
        }
        Err(e) => crate::api_error::json_error(StatusCode::INTERNAL_SERVER_ERROR, e.to_string())
            .into_response(),
    }
}

pub async fn adopt_vlan(
    RequireWrite(_claims): RequireWrite,
    State(state): State<Arc<AppState>>,
    Json(req): Json<AdoptHostRequest>,
) -> impl IntoResponse {
    tracing::debug!("networkd::{}", stringify!(adopt_vlan));
    if !super::networkd_discover::is_host_managed_id(&req.host_id) {
        return crate::api_error::json_error(StatusCode::BAD_REQUEST, "host_id must be host:…")
            .into_response();
    }
    let host = match super::networkd_discover::find_host_vlan(&state, &req.host_id) {
        Some(v) => v,
        None => {
            return crate::api_error::json_error(StatusCode::NOT_FOUND, "Host VLAN not found")
                .into_response()
        }
    };
    let stored: Vec<VlanConfig> = state
        .store
        .list_entities("networkd_vlans")
        .unwrap_or_default();
    if stored.iter().any(|v| v.name == host.name) {
        return crate::api_error::json_error(
            StatusCode::CONFLICT,
            format!("VLAN '{}' already managed", host.name),
        )
        .into_response();
    }
    let now = Utc::now().to_rfc3339();
    let cfg = VlanConfig {
        id: Uuid::new_v4().to_string(),
        name: host.name,
        vlan_id: host.vlan_id,
        parent_interface: host.parent_interface,
        mtu: host.mtu,
        addresses: host.addresses,
        gateway: host.gateway,
        dns: host.dns,
        dhcp: host.dhcp,
        created: now.clone(),
        updated: now,
        managed: true,
        operational_state: host.operational_state,
    };
    let mgr = networkd_manager(&state);
    if let Err(e) = mgr.apply_vlan(&cfg) {
        return crate::api_error::json_error(StatusCode::INTERNAL_SERVER_ERROR, e.to_string())
            .into_response();
    }
    match state.store.save_entity("networkd_vlans", &cfg.id, &cfg) {
        Ok(_) => {
            if let Err(e) = mgr.reload() {
                tracing::warn!("Failed to reload networkd: {}", e);
            }
            (StatusCode::CREATED, Json(cfg)).into_response()
        }
        Err(e) => crate::api_error::json_error(StatusCode::INTERNAL_SERVER_ERROR, e.to_string())
            .into_response(),
    }
}

pub async fn delete_vlan(
    RequireAdmin(_claims): RequireAdmin,
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    tracing::debug!("networkd::{}", stringify!(delete_vlan));
    if let Some(resp) = reject_host_delete(&id, "VLAN") {
        return resp;
    }
    let mgr = networkd_manager(&state);
    if let Ok(Some(cfg)) = state.store.get_entity::<VlanConfig>("networkd_vlans", &id) {
        if let Err(e) = mgr.remove_device(&cfg.name) {
            tracing::warn!("Failed to remove device: {}", e);
        }
        if let Err(e) = mgr.reload() {
            tracing::warn!("Failed to reload networkd: {}", e);
        }
    }
    if let Err(e) = state.store.delete_entity("networkd_vlans", &id) {
        tracing::error!("Failed to delete entity: {}", e);
    }
    StatusCode::NO_CONTENT.into_response()
}

// ============================================================================
// Macvtap handlers
// ============================================================================

pub async fn list_macvtaps(
    RequireRead(_claims): RequireRead,
    State(state): State<Arc<AppState>>,
) -> impl IntoResponse {
    tracing::debug!("networkd::{}", stringify!(list_macvtaps));
    let items: Vec<MacvtapConfig> = state
        .store
        .list_entities("networkd_macvtaps")
        .unwrap_or_else(|e| {
            tracing::error!("Storage error: {}", e);
            Vec::new()
        });
    Json(super::networkd_discover::merge_macvtaps(&state, items))
}

pub async fn create_macvtap(
    RequireWrite(_claims): RequireWrite,
    State(state): State<Arc<AppState>>,
    Json(req): Json<CreateMacvtapRequest>,
) -> impl IntoResponse {
    tracing::debug!("networkd::{}", stringify!(create_macvtap));
    if let Err((status, msg)) = crate::validation::validate_vm_name(&req.name) {
        return crate::api_error::json_error(status, msg).into_response();
    }
    if let Err(msg) = crate::validation::validate_hostname(&req.parent_interface) {
        return crate::api_error::json_error(
            StatusCode::BAD_REQUEST,
            format!("Invalid parent interface: {}", msg),
        )
        .into_response();
    }
    let now = Utc::now().to_rfc3339();
    let mac = req
        .mac_address
        .unwrap_or_else(NetworkdManager::generate_mac_address);
    let cfg = MacvtapConfig {
        id: Uuid::new_v4().to_string(),
        name: req.name,
        parent_interface: req.parent_interface,
        mode: req.mode,
        mtu: req.mtu,
        mac_address: Some(mac),
        created: now.clone(),
        updated: now,
        managed: true,
        operational_state: None,
    };

    let mgr = networkd_manager(&state);
    if let Err(e) = mgr.apply_macvtap(&cfg) {
        return crate::api_error::json_error(StatusCode::INTERNAL_SERVER_ERROR, e.to_string())
            .into_response();
    }

    match state.store.save_entity("networkd_macvtaps", &cfg.id, &cfg) {
        Ok(_) => {
            if let Err(e) = mgr.reload() {
                tracing::warn!("Failed to reload networkd: {}", e);
            }
            (StatusCode::CREATED, Json(cfg)).into_response()
        }
        Err(e) => crate::api_error::json_error(StatusCode::INTERNAL_SERVER_ERROR, e.to_string())
            .into_response(),
    }
}

pub async fn get_macvtap(
    RequireRead(_claims): RequireRead,
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    tracing::debug!("networkd::{}", stringify!(get_macvtap));
    match state
        .store
        .get_entity::<MacvtapConfig>("networkd_macvtaps", &id)
    {
        Ok(Some(m)) => Json(m).into_response(),
        Ok(None) => {
            if super::networkd_discover::is_host_managed_id(&id) {
                if let Some(m) = super::networkd_discover::find_host_macvtap(&state, &id) {
                    return Json(m).into_response();
                }
            }
            crate::api_error::json_error(StatusCode::NOT_FOUND, "Macvtap not found").into_response()
        }
        Err(_) => crate::api_error::json_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "Failed to load macvtap",
        )
        .into_response(),
    }
}

pub async fn adopt_macvtap(
    RequireWrite(_claims): RequireWrite,
    State(state): State<Arc<AppState>>,
    Json(req): Json<AdoptHostRequest>,
) -> impl IntoResponse {
    tracing::debug!("networkd::{}", stringify!(adopt_macvtap));
    if !super::networkd_discover::is_host_managed_id(&req.host_id) {
        return crate::api_error::json_error(StatusCode::BAD_REQUEST, "host_id must be host:…")
            .into_response();
    }
    let host = match super::networkd_discover::find_host_macvtap(&state, &req.host_id) {
        Some(m) => m,
        None => {
            return crate::api_error::json_error(StatusCode::NOT_FOUND, "Host macvtap not found")
                .into_response()
        }
    };
    let stored: Vec<MacvtapConfig> = state
        .store
        .list_entities("networkd_macvtaps")
        .unwrap_or_default();
    if stored.iter().any(|m| m.name == host.name) {
        return crate::api_error::json_error(
            StatusCode::CONFLICT,
            format!("Macvtap '{}' already managed", host.name),
        )
        .into_response();
    }
    let now = Utc::now().to_rfc3339();
    let cfg = MacvtapConfig {
        id: Uuid::new_v4().to_string(),
        name: host.name,
        parent_interface: host.parent_interface,
        mode: host.mode,
        mtu: host.mtu,
        mac_address: host.mac_address,
        created: now.clone(),
        updated: now,
        managed: true,
        operational_state: host.operational_state,
    };
    let mgr = networkd_manager(&state);
    if let Err(e) = mgr.apply_macvtap(&cfg) {
        return crate::api_error::json_error(StatusCode::INTERNAL_SERVER_ERROR, e.to_string())
            .into_response();
    }
    match state.store.save_entity("networkd_macvtaps", &cfg.id, &cfg) {
        Ok(_) => {
            if let Err(e) = mgr.reload() {
                tracing::warn!("Failed to reload networkd: {}", e);
            }
            (StatusCode::CREATED, Json(cfg)).into_response()
        }
        Err(e) => crate::api_error::json_error(StatusCode::INTERNAL_SERVER_ERROR, e.to_string())
            .into_response(),
    }
}

pub async fn delete_macvtap(
    RequireAdmin(_claims): RequireAdmin,
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    tracing::debug!("networkd::{}", stringify!(delete_macvtap));
    if let Some(resp) = reject_host_delete(&id, "macvtap") {
        return resp;
    }
    let mgr = networkd_manager(&state);
    if let Ok(Some(cfg)) = state
        .store
        .get_entity::<MacvtapConfig>("networkd_macvtaps", &id)
    {
        if let Err(e) = mgr.remove_device(&cfg.name) {
            tracing::warn!("Failed to remove device: {}", e);
        }
        if let Err(e) = mgr.reload() {
            tracing::warn!("Failed to reload networkd: {}", e);
        }
    }
    if let Err(e) = state.store.delete_entity("networkd_macvtaps", &id) {
        tracing::error!("Failed to delete entity: {}", e);
    }
    StatusCode::NO_CONTENT.into_response()
}

// ============================================================================
// Tap handlers
// ============================================================================

pub async fn list_taps(
    RequireRead(_claims): RequireRead,
    State(state): State<Arc<AppState>>,
) -> impl IntoResponse {
    tracing::debug!("networkd::{}", stringify!(list_taps));
    let items: Vec<TapConfig> = state
        .store
        .list_entities("networkd_taps")
        .unwrap_or_else(|e| {
            tracing::error!("Storage error: {}", e);
            Vec::new()
        });
    Json(super::networkd_discover::merge_taps(&state, items))
}

pub async fn create_tap(
    RequireWrite(_claims): RequireWrite,
    State(state): State<Arc<AppState>>,
    Json(req): Json<CreateTapRequest>,
) -> impl IntoResponse {
    tracing::debug!("networkd::{}", stringify!(create_tap));
    if let Err((status, msg)) = crate::validation::validate_vm_name(&req.name) {
        return crate::api_error::json_error(status, msg).into_response();
    }
    let now = Utc::now().to_rfc3339();
    let cfg = TapConfig {
        id: Uuid::new_v4().to_string(),
        name: req.name,
        user: req.user,
        group: req.group,
        multi_queue: req.multi_queue,
        vnet_hdr: req.vnet_hdr,
        bridge: req.bridge,
        mtu: req.mtu,
        mac_address: req.mac_address,
        created: now.clone(),
        updated: now,
        managed: true,
        operational_state: None,
    };

    let mgr = networkd_manager(&state);
    if let Err(e) = mgr.apply_tap(&cfg) {
        return crate::api_error::json_error(StatusCode::INTERNAL_SERVER_ERROR, e.to_string())
            .into_response();
    }

    match state.store.save_entity("networkd_taps", &cfg.id, &cfg) {
        Ok(_) => {
            if let Err(e) = mgr.reload() {
                tracing::warn!("Failed to reload networkd: {}", e);
            }
            (StatusCode::CREATED, Json(cfg)).into_response()
        }
        Err(e) => crate::api_error::json_error(StatusCode::INTERNAL_SERVER_ERROR, e.to_string())
            .into_response(),
    }
}

pub async fn get_tap(
    RequireRead(_claims): RequireRead,
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    tracing::debug!("networkd::{}", stringify!(get_tap));
    match state.store.get_entity::<TapConfig>("networkd_taps", &id) {
        Ok(Some(t)) => Json(t).into_response(),
        Ok(None) => {
            if super::networkd_discover::is_host_managed_id(&id) {
                if let Some(t) = super::networkd_discover::find_host_tap(&state, &id) {
                    return Json(t).into_response();
                }
            }
            crate::api_error::json_error(StatusCode::NOT_FOUND, "TAP device not found")
                .into_response()
        }
        Err(_) => crate::api_error::json_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "Failed to load TAP device",
        )
        .into_response(),
    }
}

pub async fn adopt_tap(
    RequireWrite(_claims): RequireWrite,
    State(state): State<Arc<AppState>>,
    Json(req): Json<AdoptHostRequest>,
) -> impl IntoResponse {
    tracing::debug!("networkd::{}", stringify!(adopt_tap));
    if !super::networkd_discover::is_host_managed_id(&req.host_id) {
        return crate::api_error::json_error(StatusCode::BAD_REQUEST, "host_id must be host:…")
            .into_response();
    }
    let host = match super::networkd_discover::find_host_tap(&state, &req.host_id) {
        Some(t) => t,
        None => {
            return crate::api_error::json_error(StatusCode::NOT_FOUND, "Host tap not found")
                .into_response()
        }
    };
    let stored: Vec<TapConfig> = state
        .store
        .list_entities("networkd_taps")
        .unwrap_or_default();
    if stored.iter().any(|t| t.name == host.name) {
        return crate::api_error::json_error(
            StatusCode::CONFLICT,
            format!("Tap '{}' already managed", host.name),
        )
        .into_response();
    }
    let now = Utc::now().to_rfc3339();
    let cfg = TapConfig {
        id: Uuid::new_v4().to_string(),
        name: host.name,
        user: host.user,
        group: host.group,
        multi_queue: host.multi_queue,
        vnet_hdr: host.vnet_hdr,
        bridge: host.bridge,
        mtu: host.mtu,
        mac_address: host.mac_address,
        created: now.clone(),
        updated: now,
        managed: true,
        operational_state: host.operational_state,
    };
    let mgr = networkd_manager(&state);
    if let Err(e) = mgr.apply_tap(&cfg) {
        return crate::api_error::json_error(StatusCode::INTERNAL_SERVER_ERROR, e.to_string())
            .into_response();
    }
    match state.store.save_entity("networkd_taps", &cfg.id, &cfg) {
        Ok(_) => {
            if let Err(e) = mgr.reload() {
                tracing::warn!("Failed to reload networkd: {}", e);
            }
            (StatusCode::CREATED, Json(cfg)).into_response()
        }
        Err(e) => crate::api_error::json_error(StatusCode::INTERNAL_SERVER_ERROR, e.to_string())
            .into_response(),
    }
}

pub async fn delete_tap(
    RequireAdmin(_claims): RequireAdmin,
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    tracing::debug!("networkd::{}", stringify!(delete_tap));
    if let Some(resp) = reject_host_delete(&id, "tap device") {
        return resp;
    }
    let mgr = networkd_manager(&state);
    if let Ok(Some(cfg)) = state.store.get_entity::<TapConfig>("networkd_taps", &id) {
        if let Err(e) = mgr.remove_device(&cfg.name) {
            tracing::warn!("Failed to remove device: {}", e);
        }
        if let Err(e) = mgr.reload() {
            tracing::warn!("Failed to reload networkd: {}", e);
        }
    }
    if let Err(e) = state.store.delete_entity("networkd_taps", &id) {
        tracing::error!("Failed to delete entity: {}", e);
    }
    StatusCode::NO_CONTENT.into_response()
}

// ============================================================================
// Status & control handlers
// ============================================================================

pub async fn list_links(
    RequireRead(_claims): RequireRead,
    State(state): State<Arc<AppState>>,
) -> impl IntoResponse {
    tracing::debug!("networkd::{}", stringify!(list_links));
    let mgr = networkd_manager(&state);
    match mgr.list_links() {
        Ok(links) => Json(links).into_response(),
        Err(e) => {
            tracing::warn!("list_links failed: {}", e);
            Json(Vec::<networking::models::LinkInfo>::new()).into_response()
        }
    }
}

pub async fn get_device_status(
    RequireRead(_claims): RequireRead,
    State(state): State<Arc<AppState>>,
    Path(name): Path<String>,
) -> impl IntoResponse {
    tracing::debug!("networkd::{}", stringify!(get_device_status));
    let mgr = networkd_manager(&state);
    match mgr.device_status(&name) {
        Ok(status) => Json(serde_json::json!({"name": name, "status": status})).into_response(),
        Err(e) => crate::api_error::json_error(StatusCode::INTERNAL_SERVER_ERROR, e.to_string())
            .into_response(),
    }
}

pub async fn reload_networkd(
    RequireWrite(_claims): RequireWrite,
    State(state): State<Arc<AppState>>,
) -> impl IntoResponse {
    tracing::debug!("networkd::{}", stringify!(reload_networkd));
    let mgr = networkd_manager(&state);
    match mgr.reload() {
        Ok(_) => Json(serde_json::json!({"status": "reloaded"})).into_response(),
        Err(e) => crate::api_error::json_error(StatusCode::INTERNAL_SERVER_ERROR, e.to_string())
            .into_response(),
    }
}

pub async fn list_managed_files(
    RequireRead(_claims): RequireRead,
    State(state): State<Arc<AppState>>,
) -> impl IntoResponse {
    tracing::debug!("networkd::{}", stringify!(list_managed_files));
    let mut files = networking::host_discovery::list_systemd_network_files(
        &state.config.network.networkd_config_dir,
    );
    if let Ok(managed) = networkd_manager(&state).list_managed_files() {
        for f in managed {
            if !files.contains(&f) {
                files.push(f);
            }
        }
    }
    files.sort();
    Json(files).into_response()
}

// ============================================================================
// Bond handlers
// ============================================================================

pub async fn list_bonds(
    RequireRead(_claims): RequireRead,
    State(state): State<Arc<AppState>>,
) -> impl IntoResponse {
    tracing::debug!("networkd::{}", stringify!(list_bonds));
    let items: Vec<BondConfig> = state
        .store
        .list_entities("networkd_bonds")
        .unwrap_or_else(|e| {
            tracing::error!("Storage error: {}", e);
            Vec::new()
        });
    Json(super::networkd_discover::merge_bonds(&state, items))
}

pub async fn create_bond(
    RequireWrite(_claims): RequireWrite,
    State(state): State<Arc<AppState>>,
    Json(req): Json<CreateBondRequest>,
) -> impl IntoResponse {
    tracing::debug!("networkd::{}", stringify!(create_bond));
    if let Err((status, msg)) = crate::validation::validate_vm_name(&req.name) {
        return crate::api_error::json_error(status, msg).into_response();
    }
    for iface in &req.slave_interfaces {
        if let Err(msg) = crate::validation::validate_hostname(iface) {
            return crate::api_error::json_error(
                StatusCode::BAD_REQUEST,
                format!("Invalid slave interface '{}': {}", iface, msg),
            )
            .into_response();
        }
    }
    let now = Utc::now().to_rfc3339();
    let cfg = BondConfig {
        id: Uuid::new_v4().to_string(),
        name: req.name,
        mode: req.mode,
        mii_monitor_sec: req.mii_monitor_sec,
        up_delay_sec: req.up_delay_sec,
        down_delay_sec: req.down_delay_sec,
        lacp_rate: req.lacp_rate,
        transmit_hash_policy: req.transmit_hash_policy,
        min_links: req.min_links,
        primary_slave: req.primary_slave,
        slave_interfaces: req.slave_interfaces,
        mtu: req.mtu,
        mac_address: req.mac_address,
        addresses: req.addresses,
        gateway: req.gateway,
        dns: req.dns,
        dhcp: req.dhcp,
        routes: req.routes,
        created: now.clone(),
        updated: now,
        managed: true,
        operational_state: None,
    };

    let mgr = networkd_manager(&state);
    if let Err(e) = mgr.apply_bond(&cfg) {
        return crate::api_error::json_error(StatusCode::INTERNAL_SERVER_ERROR, e.to_string())
            .into_response();
    }

    match state.store.save_entity("networkd_bonds", &cfg.id, &cfg) {
        Ok(_) => {
            if let Err(e) = mgr.reload() {
                tracing::warn!("Failed to reload networkd: {}", e);
            }
            (StatusCode::CREATED, Json(cfg)).into_response()
        }
        Err(e) => crate::api_error::json_error(StatusCode::INTERNAL_SERVER_ERROR, e.to_string())
            .into_response(),
    }
}

pub async fn get_bond(
    RequireRead(_claims): RequireRead,
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    tracing::debug!("networkd::{}", stringify!(get_bond));
    match state.store.get_entity::<BondConfig>("networkd_bonds", &id) {
        Ok(Some(b)) => Json(b).into_response(),
        Ok(None) => {
            if super::networkd_discover::is_host_managed_id(&id) {
                if let Some(b) = super::networkd_discover::find_host_bond(&state, &id) {
                    return Json(b).into_response();
                }
            }
            crate::api_error::json_error(StatusCode::NOT_FOUND, "Bond not found").into_response()
        }
        Err(_) => {
            crate::api_error::json_error(StatusCode::INTERNAL_SERVER_ERROR, "Failed to load bond")
                .into_response()
        }
    }
}

pub async fn update_bond(
    RequireWrite(_claims): RequireWrite,
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    Json(req): Json<CreateBondRequest>,
) -> impl IntoResponse {
    tracing::debug!("networkd::{}", stringify!(update_bond));
    if let Err((status, msg)) = crate::validation::validate_vm_name(&req.name) {
        return crate::api_error::json_error(status, msg).into_response();
    }
    for iface in &req.slave_interfaces {
        if let Err(msg) = crate::validation::validate_hostname(iface) {
            return crate::api_error::json_error(
                StatusCode::BAD_REQUEST,
                format!("Invalid slave interface '{}': {}", iface, msg),
            )
            .into_response();
        }
    }
    let existing = match state.store.get_entity::<BondConfig>("networkd_bonds", &id) {
        Ok(Some(b)) => b,
        Ok(None) => {
            return crate::api_error::json_error(StatusCode::NOT_FOUND, "Bond not found")
                .into_response()
        }
        Err(_) => {
            return crate::api_error::json_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                "Failed to load bond",
            )
            .into_response()
        }
    };

    let mgr = networkd_manager(&state);
    if existing.name != req.name {
        if let Err(e) = mgr.remove_device(&existing.name) {
            tracing::warn!("Failed to remove device: {}", e);
        }
    }

    let cfg = BondConfig {
        id: id.clone(),
        name: req.name,
        mode: req.mode,
        mii_monitor_sec: req.mii_monitor_sec,
        up_delay_sec: req.up_delay_sec,
        down_delay_sec: req.down_delay_sec,
        lacp_rate: req.lacp_rate,
        transmit_hash_policy: req.transmit_hash_policy,
        min_links: req.min_links,
        primary_slave: req.primary_slave,
        slave_interfaces: req.slave_interfaces,
        mtu: req.mtu,
        mac_address: req.mac_address,
        addresses: req.addresses,
        gateway: req.gateway,
        dns: req.dns,
        dhcp: req.dhcp,
        routes: req.routes,
        created: existing.created,
        updated: Utc::now().to_rfc3339(),
        managed: existing.managed,
        operational_state: existing.operational_state,
    };

    if let Err(e) = mgr.apply_bond(&cfg) {
        return crate::api_error::json_error(StatusCode::INTERNAL_SERVER_ERROR, e.to_string())
            .into_response();
    }

    match state.store.save_entity("networkd_bonds", &cfg.id, &cfg) {
        Ok(_) => {
            if let Err(e) = mgr.reload() {
                tracing::warn!("Failed to reload networkd: {}", e);
            }
            Json(cfg).into_response()
        }
        Err(e) => crate::api_error::json_error(StatusCode::INTERNAL_SERVER_ERROR, e.to_string())
            .into_response(),
    }
}

pub async fn adopt_bond(
    RequireWrite(_claims): RequireWrite,
    State(state): State<Arc<AppState>>,
    Json(req): Json<AdoptHostRequest>,
) -> impl IntoResponse {
    tracing::debug!("networkd::{}", stringify!(adopt_bond));
    if !super::networkd_discover::is_host_managed_id(&req.host_id) {
        return crate::api_error::json_error(StatusCode::BAD_REQUEST, "host_id must be host:…")
            .into_response();
    }
    let host = match super::networkd_discover::find_host_bond(&state, &req.host_id) {
        Some(b) => b,
        None => {
            return crate::api_error::json_error(StatusCode::NOT_FOUND, "Host bond not found")
                .into_response()
        }
    };
    let stored: Vec<BondConfig> = state
        .store
        .list_entities("networkd_bonds")
        .unwrap_or_default();
    if stored.iter().any(|b| b.name == host.name) {
        return crate::api_error::json_error(
            StatusCode::CONFLICT,
            format!("Bond '{}' already managed", host.name),
        )
        .into_response();
    }
    let now = Utc::now().to_rfc3339();
    let cfg = BondConfig {
        id: Uuid::new_v4().to_string(),
        name: host.name,
        mode: host.mode,
        mii_monitor_sec: host.mii_monitor_sec,
        up_delay_sec: host.up_delay_sec,
        down_delay_sec: host.down_delay_sec,
        lacp_rate: host.lacp_rate,
        transmit_hash_policy: host.transmit_hash_policy,
        min_links: host.min_links,
        primary_slave: host.primary_slave,
        slave_interfaces: host.slave_interfaces,
        mtu: host.mtu,
        mac_address: host.mac_address,
        addresses: host.addresses,
        gateway: host.gateway,
        dns: host.dns,
        dhcp: host.dhcp,
        routes: host.routes,
        created: now.clone(),
        updated: now,
        managed: true,
        operational_state: host.operational_state,
    };
    let mgr = networkd_manager(&state);
    if let Err(e) = mgr.apply_bond(&cfg) {
        return crate::api_error::json_error(StatusCode::INTERNAL_SERVER_ERROR, e.to_string())
            .into_response();
    }
    match state.store.save_entity("networkd_bonds", &cfg.id, &cfg) {
        Ok(_) => {
            if let Err(e) = mgr.reload() {
                tracing::warn!("Failed to reload networkd: {}", e);
            }
            (StatusCode::CREATED, Json(cfg)).into_response()
        }
        Err(e) => crate::api_error::json_error(StatusCode::INTERNAL_SERVER_ERROR, e.to_string())
            .into_response(),
    }
}

pub async fn delete_bond(
    RequireAdmin(_claims): RequireAdmin,
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    tracing::debug!("networkd::{}", stringify!(delete_bond));
    if let Some(resp) = reject_host_delete(&id, "bond") {
        return resp;
    }
    let mgr = networkd_manager(&state);
    if let Ok(Some(cfg)) = state.store.get_entity::<BondConfig>("networkd_bonds", &id) {
        if let Err(e) = mgr.remove_device(&cfg.name) {
            tracing::warn!("Failed to remove device: {}", e);
        }
        if let Err(e) = mgr.reload() {
            tracing::warn!("Failed to reload networkd: {}", e);
        }
    }
    if let Err(e) = state.store.delete_entity("networkd_bonds", &id) {
        tracing::error!("Failed to delete entity: {}", e);
    }
    StatusCode::NO_CONTENT.into_response()
}

// ============================================================================
// Network file handlers (physical interface config)
// ============================================================================

pub async fn list_network_files(
    RequireRead(_claims): RequireRead,
    State(state): State<Arc<AppState>>,
) -> impl IntoResponse {
    tracing::debug!("networkd::{}", stringify!(list_network_files));
    let items: Vec<NetworkFileConfig> = state
        .store
        .list_entities("networkd_netfiles")
        .unwrap_or_else(|e| {
            tracing::error!("Storage error: {}", e);
            Vec::new()
        });
    Json(super::networkd_discover::merge_netfiles(&state, items))
}

pub async fn create_network_file(
    RequireWrite(_claims): RequireWrite,
    State(state): State<Arc<AppState>>,
    Json(req): Json<CreateNetworkFileRequest>,
) -> impl IntoResponse {
    tracing::debug!("networkd::{}", stringify!(create_network_file));
    if let Err((status, msg)) = crate::validation::validate_vm_name(&req.match_name) {
        return crate::api_error::json_error(status, msg).into_response();
    }
    let now = Utc::now().to_rfc3339();
    let cfg = NetworkFileConfig {
        id: Uuid::new_v4().to_string(),
        match_name: req.match_name,
        match_mac: req.match_mac,
        addresses: req.addresses,
        gateway: req.gateway,
        dns: req.dns,
        dhcp: req.dhcp,
        bridge: req.bridge,
        bond: req.bond,
        mtu: req.mtu,
        routes: req.routes,
        description: req.description,
        created: now.clone(),
        updated: now,
        managed: true,
        operational_state: None,
    };

    let mgr = networkd_manager(&state);
    if let Err(e) = mgr.apply_network_file(&cfg) {
        return crate::api_error::json_error(StatusCode::INTERNAL_SERVER_ERROR, e.to_string())
            .into_response();
    }

    match state.store.save_entity("networkd_netfiles", &cfg.id, &cfg) {
        Ok(_) => {
            if let Err(e) = mgr.reload() {
                tracing::warn!("Failed to reload networkd: {}", e);
            }
            (StatusCode::CREATED, Json(cfg)).into_response()
        }
        Err(e) => crate::api_error::json_error(StatusCode::INTERNAL_SERVER_ERROR, e.to_string())
            .into_response(),
    }
}

pub async fn get_network_file(
    RequireRead(_claims): RequireRead,
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    tracing::debug!("networkd::{}", stringify!(get_network_file));
    match state
        .store
        .get_entity::<NetworkFileConfig>("networkd_netfiles", &id)
    {
        Ok(Some(n)) => Json(n).into_response(),
        Ok(None) => {
            if super::networkd_discover::is_host_managed_id(&id) {
                if let Some(n) = super::networkd_discover::find_host_netfile(&state, &id) {
                    return Json(n).into_response();
                }
            }
            crate::api_error::json_error(StatusCode::NOT_FOUND, "Network file not found")
                .into_response()
        }
        Err(_) => crate::api_error::json_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "Failed to load network file",
        )
        .into_response(),
    }
}

pub async fn adopt_network_file(
    RequireWrite(_claims): RequireWrite,
    State(state): State<Arc<AppState>>,
    Json(req): Json<AdoptHostRequest>,
) -> impl IntoResponse {
    tracing::debug!("networkd::{}", stringify!(adopt_network_file));
    if !super::networkd_discover::is_host_managed_id(&req.host_id) {
        return crate::api_error::json_error(
            StatusCode::BAD_REQUEST,
            "host_id must be a host-discovered interface (host:…)",
        )
        .into_response();
    }
    let host = match super::networkd_discover::find_host_netfile(&state, &req.host_id) {
        Some(n) => n,
        None => {
            return crate::api_error::json_error(StatusCode::NOT_FOUND, "Host interface not found")
                .into_response()
        }
    };
    let stored: Vec<NetworkFileConfig> = state
        .store
        .list_entities("networkd_netfiles")
        .unwrap_or_default();
    if stored.iter().any(|n| n.match_name == host.match_name) {
        return crate::api_error::json_error(
            StatusCode::CONFLICT,
            format!(
                "Interface '{}' is already managed by zyvor-fabricd",
                host.match_name
            ),
        )
        .into_response();
    }

    let now = Utc::now().to_rfc3339();
    let cfg = NetworkFileConfig {
        id: Uuid::new_v4().to_string(),
        match_name: host.match_name,
        match_mac: host.match_mac,
        addresses: host.addresses,
        gateway: host.gateway,
        dns: host.dns,
        dhcp: host.dhcp,
        bridge: host.bridge,
        bond: host.bond,
        mtu: host.mtu,
        routes: host.routes,
        description: host.description,
        created: now.clone(),
        updated: now,
        managed: true,
        operational_state: host.operational_state,
    };

    let mgr = networkd_manager(&state);
    if let Err(e) = mgr.apply_network_file(&cfg) {
        return crate::api_error::json_error(StatusCode::INTERNAL_SERVER_ERROR, e.to_string())
            .into_response();
    }
    match state.store.save_entity("networkd_netfiles", &cfg.id, &cfg) {
        Ok(_) => {
            if let Err(e) = mgr.reload() {
                tracing::warn!("Failed to reload networkd: {}", e);
            }
            (StatusCode::CREATED, Json(cfg)).into_response()
        }
        Err(e) => crate::api_error::json_error(StatusCode::INTERNAL_SERVER_ERROR, e.to_string())
            .into_response(),
    }
}

pub async fn delete_network_file(
    RequireAdmin(_claims): RequireAdmin,
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    tracing::debug!("networkd::{}", stringify!(delete_network_file));
    if let Some(resp) = reject_host_delete(&id, "interface") {
        return resp;
    }
    let mgr = networkd_manager(&state);
    if let Ok(Some(cfg)) = state
        .store
        .get_entity::<NetworkFileConfig>("networkd_netfiles", &id)
    {
        if let Err(e) = mgr.remove_device(&format!("net-{}", cfg.match_name)) {
            tracing::warn!("Failed to remove device: {}", e);
        }
        if let Err(e) = mgr.reload() {
            tracing::warn!("Failed to reload networkd: {}", e);
        }
    }
    if let Err(e) = state.store.delete_entity("networkd_netfiles", &id) {
        tracing::error!("Failed to delete entity: {}", e);
    }
    StatusCode::NO_CONTENT.into_response()
}

// ============================================================================
// Link file handlers
// ============================================================================

pub async fn list_link_files(
    RequireRead(_claims): RequireRead,
    State(state): State<Arc<AppState>>,
) -> impl IntoResponse {
    tracing::debug!("networkd::{}", stringify!(list_link_files));
    let items: Vec<LinkFileConfig> = state
        .store
        .list_entities("networkd_linkfiles")
        .unwrap_or_else(|e| {
            tracing::error!("Storage error: {}", e);
            Vec::new()
        });
    Json(super::networkd_discover::merge_link_files(&state, items))
}

pub async fn create_link_file(
    RequireWrite(_claims): RequireWrite,
    State(state): State<Arc<AppState>>,
    Json(req): Json<CreateLinkFileRequest>,
) -> impl IntoResponse {
    tracing::debug!("networkd::{}", stringify!(create_link_file));
    if let Some(ref name) = req.name {
        if let Err((status, msg)) = crate::validation::validate_vm_name(name) {
            return crate::api_error::json_error(status, msg).into_response();
        }
    }
    let now = Utc::now().to_rfc3339();
    let cfg = LinkFileConfig {
        id: Uuid::new_v4().to_string(),
        match_mac: req.match_mac,
        match_path: req.match_path,
        match_driver: req.match_driver,
        match_original_name: req.match_original_name,
        name: req.name,
        mtu: req.mtu,
        mac_address: req.mac_address,
        wake_on_lan: req.wake_on_lan,
        description: req.description,
        created: now.clone(),
        updated: now,
        managed: true,
        source_file: None,
    };

    let mgr = networkd_manager(&state);
    if let Err(e) = mgr.apply_link_file(&cfg) {
        return crate::api_error::json_error(StatusCode::INTERNAL_SERVER_ERROR, e.to_string())
            .into_response();
    }

    match state.store.save_entity("networkd_linkfiles", &cfg.id, &cfg) {
        Ok(_) => {
            if let Err(e) = mgr.reload() {
                tracing::warn!("Failed to reload networkd: {}", e);
            }
            (StatusCode::CREATED, Json(cfg)).into_response()
        }
        Err(e) => crate::api_error::json_error(StatusCode::INTERNAL_SERVER_ERROR, e.to_string())
            .into_response(),
    }
}

pub async fn delete_link_file(
    RequireAdmin(_claims): RequireAdmin,
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    tracing::debug!("networkd::{}", stringify!(delete_link_file));
    if let Some(resp) = reject_host_delete(&id, "link file") {
        return resp;
    }
    let mgr = networkd_manager(&state);
    if let Ok(Some(cfg)) = state
        .store
        .get_entity::<LinkFileConfig>("networkd_linkfiles", &id)
    {
        let file_id = cfg
            .name
            .as_deref()
            .or(cfg.match_original_name.as_deref())
            .unwrap_or(&cfg.id);
        if let Err(e) = mgr.remove_device(&format!("link-{}", file_id)) {
            tracing::warn!("Failed to remove device: {}", e);
        }
        if let Err(e) = mgr.reload() {
            tracing::warn!("Failed to reload networkd: {}", e);
        }
    }
    if let Err(e) = state.store.delete_entity("networkd_linkfiles", &id) {
        tracing::error!("Failed to delete entity: {}", e);
    }
    StatusCode::NO_CONTENT.into_response()
}

// ============================================================================
// Port forwarding handlers (nftables DNAT)
// ============================================================================

pub async fn list_port_forwards(
    RequireRead(_claims): RequireRead,
    State(state): State<Arc<AppState>>,
) -> impl IntoResponse {
    tracing::debug!("networkd::{}", stringify!(list_port_forwards));
    let items: Vec<PortForwardConfig> = state
        .store
        .list_entities("networkd_port_forwards")
        .unwrap_or_default();
    Json(super::networkd_discover::merge_port_forwards(items))
}

pub async fn create_port_forward(
    RequireWrite(_claims): RequireWrite,
    State(state): State<Arc<AppState>>,
    Json(req): Json<CreatePortForwardRequest>,
) -> impl IntoResponse {
    tracing::debug!("networkd::{}", stringify!(create_port_forward));
    if let Err((status, msg)) = crate::validation::validate_vm_name(&req.name) {
        return crate::api_error::json_error(status, msg).into_response();
    }
    let now = Utc::now().to_rfc3339();
    let cfg = PortForwardConfig {
        id: Uuid::new_v4().to_string(),
        name: req.name,
        protocol: req.protocol,
        host_port: req.host_port,
        guest_ip: req.guest_ip,
        guest_port: req.guest_port,
        interface: req.interface,
        enabled: req.enabled,
        description: req.description,
        created: now.clone(),
        updated: now,
        managed: true,
    };

    if cfg.enabled {
        let nft = NftManager::new();
        if let Err(e) = nft.apply(&cfg) {
            return crate::api_error::json_error(StatusCode::INTERNAL_SERVER_ERROR, e.to_string())
                .into_response();
        }
    }

    match state
        .store
        .save_entity("networkd_port_forwards", &cfg.id, &cfg)
    {
        Ok(_) => (StatusCode::CREATED, Json(cfg)).into_response(),
        Err(e) => crate::api_error::json_error(StatusCode::INTERNAL_SERVER_ERROR, e.to_string())
            .into_response(),
    }
}

pub async fn get_port_forward(
    RequireRead(_claims): RequireRead,
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    tracing::debug!("networkd::{}", stringify!(get_port_forward));
    match state
        .store
        .get_entity::<PortForwardConfig>("networkd_port_forwards", &id)
    {
        Ok(Some(pf)) => Json(pf).into_response(),
        Ok(None) => {
            if super::networkd_discover::is_host_managed_id(&id) {
                if let Some(pf) = super::networkd_discover::find_host_port_forward(&id) {
                    return Json(pf).into_response();
                }
            }
            crate::api_error::json_error(StatusCode::NOT_FOUND, "Port forward not found")
                .into_response()
        }
        Err(_) => crate::api_error::json_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "Failed to load port forward",
        )
        .into_response(),
    }
}

pub async fn delete_port_forward(
    RequireAdmin(_claims): RequireAdmin,
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    tracing::debug!("networkd::{}", stringify!(delete_port_forward));
    if let Some(resp) = reject_host_delete(&id, "port forward") {
        return resp;
    }
    if let Ok(Some(cfg)) = state
        .store
        .get_entity::<PortForwardConfig>("networkd_port_forwards", &id)
    {
        let nft = NftManager::new();
        if let Err(e) = nft.remove(&cfg) {
            tracing::warn!("Failed to remove nft rule: {}", e);
        }
    }
    if let Err(e) = state.store.delete_entity("networkd_port_forwards", &id) {
        tracing::error!("Failed to delete entity: {}", e);
    }
    StatusCode::NO_CONTENT.into_response()
}

pub async fn adopt_port_forward(
    RequireWrite(_claims): RequireWrite,
    State(state): State<Arc<AppState>>,
    Json(req): Json<AdoptHostRequest>,
) -> impl IntoResponse {
    tracing::debug!("networkd::{}", stringify!(adopt_port_forward));
    let host = match super::networkd_discover::find_host_port_forward(&req.host_id) {
        Some(p) if super::networkd_discover::is_host_managed_id(&p.id) => p,
        _ => {
            return crate::api_error::json_error(
                StatusCode::NOT_FOUND,
                "Host port forward not found",
            )
            .into_response();
        }
    };

    let stored: Vec<PortForwardConfig> = state
        .store
        .list_entities("networkd_port_forwards")
        .unwrap_or_default();
    if stored.iter().any(|p| p.name == host.name) {
        return crate::api_error::json_error(
            StatusCode::CONFLICT,
            format!(
                "Port forward '{}' is already managed by zyvor-fabricd",
                host.name
            ),
        )
        .into_response();
    }

    let now = Utc::now().to_rfc3339();
    let mut cfg = host;
    cfg.id = Uuid::new_v4().to_string();
    cfg.managed = true;
    cfg.created = now.clone();
    cfg.updated = now;

    if let Err(e) = state
        .store
        .save_entity("networkd_port_forwards", &cfg.id, &cfg)
    {
        return crate::api_error::json_error(StatusCode::INTERNAL_SERVER_ERROR, e.to_string())
            .into_response();
    }

    let nft = NftManager::new();
    if let Err(e) = nft.sync_all(&[cfg.clone()]) {
        tracing::warn!("Post-adopt port forward nft sync failed: {}", e);
    }

    (StatusCode::CREATED, Json(cfg)).into_response()
}

pub async fn sync_port_forwards(
    RequireWrite(_claims): RequireWrite,
    State(state): State<Arc<AppState>>,
) -> impl IntoResponse {
    tracing::debug!("networkd::{}", stringify!(sync_port_forwards));
    let configs: Vec<PortForwardConfig> = state
        .store
        .list_entities("networkd_port_forwards")
        .unwrap_or_default();

    let nft = NftManager::new();
    match nft.sync_all(&configs) {
        Ok(_) => Json(serde_json::json!({
            "status": "synced",
            "rules": configs.len(),
        }))
        .into_response(),
        Err(e) => crate::api_error::json_error(StatusCode::INTERNAL_SERVER_ERROR, e.to_string())
            .into_response(),
    }
}

// ============================================================================
// VXLAN handlers
// ============================================================================

pub async fn list_vxlans(
    RequireRead(_claims): RequireRead,
    State(state): State<Arc<AppState>>,
) -> impl IntoResponse {
    tracing::debug!("networkd::{}", stringify!(list_vxlans));
    let items: Vec<VxlanConfig> =
        state
            .store
            .list_entities("networkd_vxlans")
            .unwrap_or_else(|e| {
                tracing::error!("Storage error: {}", e);
                Vec::new()
            });
    Json(super::networkd_discover::merge_vxlans(&state, items))
}

pub async fn create_vxlan(
    RequireWrite(_claims): RequireWrite,
    State(state): State<Arc<AppState>>,
    Json(req): Json<CreateVxlanRequest>,
) -> impl IntoResponse {
    tracing::debug!("networkd::{}", stringify!(create_vxlan));
    if let Err((status, msg)) = crate::validation::validate_vm_name(&req.name) {
        return crate::api_error::json_error(status, msg).into_response();
    }
    if let Some(ref iface) = req.parent_interface {
        if let Err(msg) = crate::validation::validate_hostname(iface) {
            return crate::api_error::json_error(
                StatusCode::BAD_REQUEST,
                format!("Invalid parent interface: {}", msg),
            )
            .into_response();
        }
    }
    let now = Utc::now().to_rfc3339();
    let cfg = VxlanConfig {
        id: Uuid::new_v4().to_string(),
        name: req.name,
        vni: req.vni,
        remote: req.remote,
        local: req.local,
        port: req.port,
        parent_interface: req.parent_interface,
        mtu: req.mtu,
        addresses: req.addresses,
        gateway: req.gateway,
        dns: req.dns,
        dhcp: req.dhcp,
        created: now.clone(),
        updated: now,
        managed: true,
    };

    let mgr = networkd_manager(&state);
    if let Err(e) = mgr.apply_vxlan(&cfg) {
        return crate::api_error::json_error(StatusCode::INTERNAL_SERVER_ERROR, e.to_string())
            .into_response();
    }

    match state.store.save_entity("networkd_vxlans", &cfg.id, &cfg) {
        Ok(_) => {
            if let Err(e) = mgr.reload() {
                tracing::warn!("Failed to reload networkd: {}", e);
            }
            (StatusCode::CREATED, Json(cfg)).into_response()
        }
        Err(e) => crate::api_error::json_error(StatusCode::INTERNAL_SERVER_ERROR, e.to_string())
            .into_response(),
    }
}

pub async fn get_vxlan(
    RequireRead(_claims): RequireRead,
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    tracing::debug!("networkd::{}", stringify!(get_vxlan));
    match state
        .store
        .get_entity::<VxlanConfig>("networkd_vxlans", &id)
    {
        Ok(Some(v)) => Json(v).into_response(),
        Ok(None) => {
            if let Some(v) = super::networkd_discover::find_host_vxlan(&state, &id) {
                return Json(v).into_response();
            }
            crate::api_error::json_error(StatusCode::NOT_FOUND, "VXLAN not found").into_response()
        }
        Err(_) => {
            crate::api_error::json_error(StatusCode::INTERNAL_SERVER_ERROR, "Failed to load VXLAN")
                .into_response()
        }
    }
}

pub async fn delete_vxlan(
    RequireAdmin(_claims): RequireAdmin,
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    tracing::debug!("networkd::{}", stringify!(delete_vxlan));
    if let Some(resp) = reject_host_delete(&id, "VXLAN") {
        return resp;
    }
    let mgr = networkd_manager(&state);
    if let Ok(Some(cfg)) = state
        .store
        .get_entity::<VxlanConfig>("networkd_vxlans", &id)
    {
        if let Err(e) = mgr.remove_device(&cfg.name) {
            tracing::warn!("Failed to remove device: {}", e);
        }
        if let Err(e) = mgr.reload() {
            tracing::warn!("Failed to reload networkd: {}", e);
        }
    }
    if let Err(e) = state.store.delete_entity("networkd_vxlans", &id) {
        tracing::error!("Failed to delete entity: {}", e);
    }
    StatusCode::NO_CONTENT.into_response()
}

pub async fn adopt_vxlan(
    RequireWrite(_claims): RequireWrite,
    State(state): State<Arc<AppState>>,
    Json(req): Json<AdoptHostRequest>,
) -> impl IntoResponse {
    tracing::debug!("networkd::{}", stringify!(adopt_vxlan));
    let host = match super::networkd_discover::find_host_vxlan(&state, &req.host_id) {
        Some(v) if super::networkd_discover::is_host_managed_id(&v.id) => v,
        _ => {
            return crate::api_error::json_error(StatusCode::NOT_FOUND, "Host VXLAN not found")
                .into_response();
        }
    };

    let stored: Vec<VxlanConfig> = state
        .store
        .list_entities("networkd_vxlans")
        .unwrap_or_default();
    if stored.iter().any(|v| v.name == host.name) {
        return crate::api_error::json_error(
            StatusCode::CONFLICT,
            format!("VXLAN '{}' is already managed by zyvor-fabricd", host.name),
        )
        .into_response();
    }

    let now = Utc::now().to_rfc3339();
    let cfg = VxlanConfig {
        id: Uuid::new_v4().to_string(),
        name: host.name,
        vni: host.vni,
        remote: host.remote,
        local: host.local,
        port: host.port,
        parent_interface: host.parent_interface,
        mtu: host.mtu,
        addresses: host.addresses,
        gateway: host.gateway,
        dns: host.dns,
        dhcp: host.dhcp,
        created: now.clone(),
        updated: now,
        managed: true,
    };

    let mgr = networkd_manager(&state);
    if let Err(e) = mgr.apply_vxlan(&cfg) {
        return crate::api_error::json_error(StatusCode::INTERNAL_SERVER_ERROR, e.to_string())
            .into_response();
    }
    if let Err(e) = state.store.save_entity("networkd_vxlans", &cfg.id, &cfg) {
        return crate::api_error::json_error(StatusCode::INTERNAL_SERVER_ERROR, e.to_string())
            .into_response();
    }
    if let Err(e) = mgr.reload() {
        tracing::warn!("Failed to reload networkd after VXLAN adopt: {}", e);
    }

    (StatusCode::CREATED, Json(cfg)).into_response()
}

// ============================================================================
// SR-IOV handlers
// ============================================================================

pub async fn list_sriov(
    RequireRead(_claims): RequireRead,
    State(state): State<Arc<AppState>>,
) -> impl IntoResponse {
    tracing::debug!("networkd::{}", stringify!(list_sriov));
    let items: Vec<SriovConfig> = state
        .store
        .list_entities("networkd_sriov")
        .unwrap_or_else(|e| {
            tracing::error!("Storage error: {}", e);
            Vec::new()
        });
    Json(super::networkd_discover::merge_sriov(&state, items))
}

pub async fn create_sriov(
    RequireWrite(_claims): RequireWrite,
    State(state): State<Arc<AppState>>,
    Json(req): Json<CreateSriovRequest>,
) -> impl IntoResponse {
    tracing::debug!("networkd::{}", stringify!(create_sriov));
    if let Err(msg) = crate::validation::validate_hostname(&req.pf_name) {
        return crate::api_error::json_error(
            StatusCode::BAD_REQUEST,
            format!("Invalid PF name: {}", msg),
        )
        .into_response();
    }
    // validate_hostname allows '.' (legitimate in names like "eth0.100"),
    // so a name consisting only of dots (".", "..") would otherwise pass
    // through -- apply_sriov interpolates pf_name directly into a sysfs
    // path (/sys/class/net/{pf_name}/device/sriov_numvfs), and "/" isn't
    // allowed so this can't escape further than one directory, but reject
    // it outright anyway since it's never a real interface name.
    if req.pf_name.chars().all(|c| c == '.') {
        return crate::api_error::json_error(
            StatusCode::BAD_REQUEST,
            "Invalid PF name: must be a real interface name, not '.' or '..'",
        )
        .into_response();
    }
    let now = Utc::now().to_rfc3339();
    let cfg = SriovConfig {
        id: Uuid::new_v4().to_string(),
        pf_name: req.pf_name,
        num_vfs: req.num_vfs,
        vf_configs: req.vf_configs,
        created: now.clone(),
        updated: now,
        managed: true,
    };

    let mgr = networkd_manager(&state);
    if let Err(e) = mgr.apply_sriov(&cfg) {
        return crate::api_error::json_error(StatusCode::INTERNAL_SERVER_ERROR, e.to_string())
            .into_response();
    }

    match state.store.save_entity("networkd_sriov", &cfg.id, &cfg) {
        Ok(_) => (StatusCode::CREATED, Json(cfg)).into_response(),
        Err(e) => crate::api_error::json_error(StatusCode::INTERNAL_SERVER_ERROR, e.to_string())
            .into_response(),
    }
}

pub async fn get_sriov(
    RequireRead(_claims): RequireRead,
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    tracing::debug!("networkd::{}", stringify!(get_sriov));
    match state.store.get_entity::<SriovConfig>("networkd_sriov", &id) {
        Ok(Some(s)) => Json(s).into_response(),
        Ok(None) => {
            if let Some(s) = super::networkd_discover::find_host_sriov(&state, &id) {
                return Json(s).into_response();
            }
            crate::api_error::json_error(StatusCode::NOT_FOUND, "SR-IOV config not found")
                .into_response()
        }
        Err(_) => crate::api_error::json_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "Failed to load SR-IOV config",
        )
        .into_response(),
    }
}

pub async fn delete_sriov(
    RequireAdmin(_claims): RequireAdmin,
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    tracing::debug!("networkd::{}", stringify!(delete_sriov));
    if let Some(resp) = reject_host_delete(&id, "SR-IOV config") {
        return resp;
    }
    if let Ok(Some(cfg)) = state.store.get_entity::<SriovConfig>("networkd_sriov", &id) {
        let mgr = networkd_manager(&state);
        if let Err(e) = mgr.remove_sriov(&cfg.pf_name) {
            tracing::warn!("Failed to remove device: {}", e);
        }
    }
    if let Err(e) = state.store.delete_entity("networkd_sriov", &id) {
        tracing::error!("Failed to delete entity: {}", e);
    }
    StatusCode::NO_CONTENT.into_response()
}

pub async fn adopt_sriov(
    RequireWrite(_claims): RequireWrite,
    State(state): State<Arc<AppState>>,
    Json(req): Json<AdoptHostRequest>,
) -> impl IntoResponse {
    tracing::debug!("networkd::{}", stringify!(adopt_sriov));
    let host = match super::networkd_discover::find_host_sriov(&state, &req.host_id) {
        Some(s) if super::networkd_discover::is_host_managed_id(&s.id) => s,
        _ => {
            return crate::api_error::json_error(
                StatusCode::NOT_FOUND,
                "Host SR-IOV config not found",
            )
            .into_response();
        }
    };

    let stored: Vec<SriovConfig> = state
        .store
        .list_entities("networkd_sriov")
        .unwrap_or_default();
    if stored.iter().any(|s| s.pf_name == host.pf_name) {
        return crate::api_error::json_error(
            StatusCode::CONFLICT,
            format!(
                "SR-IOV on '{}' is already managed by zyvor-fabricd",
                host.pf_name
            ),
        )
        .into_response();
    }

    let now = Utc::now().to_rfc3339();
    let cfg = SriovConfig {
        id: Uuid::new_v4().to_string(),
        pf_name: host.pf_name,
        num_vfs: host.num_vfs,
        vf_configs: host.vf_configs,
        created: now.clone(),
        updated: now,
        managed: true,
    };

    let mgr = networkd_manager(&state);
    if let Err(e) = mgr.apply_sriov(&cfg) {
        return crate::api_error::json_error(StatusCode::INTERNAL_SERVER_ERROR, e.to_string())
            .into_response();
    }
    if let Err(e) = state.store.save_entity("networkd_sriov", &cfg.id, &cfg) {
        return crate::api_error::json_error(StatusCode::INTERNAL_SERVER_ERROR, e.to_string())
            .into_response();
    }

    (StatusCode::CREATED, Json(cfg)).into_response()
}

// ============================================================================
// Scan existing configs (parser)
// ============================================================================

pub async fn scan_configs(
    RequireRead(_claims): RequireRead,
    State(state): State<Arc<AppState>>,
) -> impl IntoResponse {
    tracing::debug!("networkd::{}", stringify!(scan_configs));
    let dir = std::path::Path::new(&state.config.network.networkd_config_dir);
    match networking::parser::scan_networkd_dir(dir) {
        Ok(configs) => Json(configs).into_response(),
        Err(e) => crate::api_error::json_error(StatusCode::INTERNAL_SERVER_ERROR, e.to_string())
            .into_response(),
    }
}

// ============================================================================
// DHCP Server configuration handler
// ============================================================================

#[derive(Debug, serde::Deserialize)]
pub struct DhcpServerConfig {
    pub bridge_name: String,
    pub pool_start: String,
    pub pool_end: String,
    pub dns_servers: Option<Vec<String>>,
    pub lease_time_sec: Option<u32>,
}

/// POST /api/networkd/dhcp - Configure a per-bridge DHCP server via
/// dnsmasq (replaces the old systemd-networkd `[DHCPServer]` file-write +
/// `networkctl reload` — see api/network_cloud.rs::create_dhcp_server,
/// which this now shares a backend with; that endpoint's request shape
/// takes an explicit gateway + pool_offset/pool_size, this one takes a
/// pool_start/pool_end IP range instead, so the two aren't simply
/// duplicates of each other despite doing the same underlying thing).
pub async fn configure_dhcp_server(
    RequireAdmin(_claims): RequireAdmin,
    State(state): State<Arc<AppState>>,
    Json(req): Json<DhcpServerConfig>,
) -> impl IntoResponse {
    tracing::debug!("networkd::{}", stringify!(configure_dhcp_server));

    if let Err((status, msg)) = crate::validation::validate_device_name(&req.bridge_name) {
        return crate::api_error::json_error(status, msg).into_response();
    }

    if let Err(e) = crate::validation::validate_ip_address(&req.pool_start) {
        return crate::api_error::json_error(StatusCode::BAD_REQUEST, e).into_response();
    }
    if let Err(e) = crate::validation::validate_ip_address(&req.pool_end) {
        return crate::api_error::json_error(StatusCode::BAD_REQUEST, e).into_response();
    }

    let (start, end): (std::net::Ipv4Addr, std::net::Ipv4Addr) =
        match (req.pool_start.parse(), req.pool_end.parse()) {
            (Ok(s), Ok(e)) => (s, e),
            _ => {
                return crate::api_error::json_error(
                    StatusCode::BAD_REQUEST,
                    "pool_start/pool_end must be IPv4 addresses",
                )
                .into_response()
            }
        };
    if start.octets()[..3] != end.octets()[..3] {
        return crate::api_error::json_error(
            StatusCode::BAD_REQUEST,
            "pool_start and pool_end must be in the same /24 (dnsmasq is configured per-subnet)",
        )
        .into_response();
    }
    let pool_offset = start.octets()[3] as u32;
    let pool_size = (end.octets()[3] as u32).saturating_sub(pool_offset) + 1;
    // dnsmasq-manager derives the pool/netmask from the bridge's own
    // gateway address assuming a /24 — this endpoint doesn't collect one
    // explicitly, so use the pool's own network base .1, the same
    // convention the old systemd-networkd config generator assumed.
    let gateway =
        std::net::Ipv4Addr::new(start.octets()[0], start.octets()[1], start.octets()[2], 1);

    let lease_time = req.lease_time_sec.unwrap_or(3600);
    // See network_cloud::create_dhcp_server's matching comment: with no
    // explicit dns_servers, default to this bridge's own gateway, which
    // now also answers DNS for zone records (zone_hosts_dir below).
    let dns_servers = req.dns_servers.clone().unwrap_or_default();
    let dns_servers = if dns_servers.is_empty() {
        vec![gateway.to_string()]
    } else {
        dns_servers
    };
    let dhcp_cfg = zyvor_fabric_dnsmasq_manager::DhcpConfig {
        bridge: req.bridge_name.clone(),
        gateway,
        pool_offset,
        pool_size,
        default_lease_time_sec: lease_time,
        dns_servers,
        domain: None,
        zone_hosts_dir: Some(::dns_policy::enforcement::DNS_DIR.into()),
    };

    match state.dnsmasq_manager.start(&dhcp_cfg).await {
        Ok(()) => Json(serde_json::json!({
            "status": "configured",
            "bridge": req.bridge_name,
            "pool_offset": pool_offset,
            "pool_size": pool_size,
        }))
        .into_response(),
        Err(e) => crate::api_error::json_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Failed to start DHCP server: {e:#}"),
        )
        .into_response(),
    }
}

/// List all network interfaces via netlink (real-time kernel state).
pub async fn list_netlink_interfaces(RequireRead(_claims): RequireRead) -> impl IntoResponse {
    tracing::debug!("networkd::{}", stringify!(list_netlink_interfaces));
    match networking::netlink::list_interfaces().await {
        Ok(ifaces) => Json(ifaces).into_response(),
        Err(e) => crate::api_error::json_error(StatusCode::INTERNAL_SERVER_ERROR, e.to_string())
            .into_response(),
    }
}

/// List only physical network interfaces (for bond slave / bridge member selection).
pub async fn list_physical_interfaces(RequireRead(_claims): RequireRead) -> impl IntoResponse {
    tracing::debug!("networkd::{}", stringify!(list_physical_interfaces));
    match networking::netlink::list_physical_interfaces().await {
        Ok(ifaces) => Json(ifaces).into_response(),
        Err(e) => crate::api_error::json_error(StatusCode::INTERNAL_SERVER_ERROR, e.to_string())
            .into_response(),
    }
}

/// List interfaces available as bond slaves or bridge members (not already enslaved).
pub async fn list_available_interfaces(RequireRead(_claims): RequireRead) -> impl IntoResponse {
    tracing::debug!("networkd::{}", stringify!(list_available_interfaces));
    match networking::netlink::list_available_interfaces().await {
        Ok(ifaces) => Json(ifaces).into_response(),
        Err(e) => crate::api_error::json_error(StatusCode::INTERNAL_SERVER_ERROR, e.to_string())
            .into_response(),
    }
}
