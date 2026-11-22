// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: LicenseRef-Zyvor-Production-2.0

use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use chrono::Utc;
use security::{RequireRead, RequireWrite};
use serde_json::json;
use std::collections::HashMap;
use std::sync::Arc;
use uuid::Uuid;

use networking::models::AdoptHostRequest;
use vm_firewall::compiler::VMSnapshot;
use vm_firewall::models::{
    AssignFirewallRequest, CreateFirewallProfileRequest, CreateFirewallZoneRequest,
    FirewallProfile, FirewallStatus, FirewallZone, VMFirewallAssignment,
};

use crate::server::AppState;

const PROFILES_KEY: &str = "firewall_profiles";
const ZONES_KEY: &str = "firewall_zones";
const ASSIGNMENTS_KEY: &str = "firewall_assignments";

// ── Firewall Profile CRUD ───────────────────────────────────────────

pub async fn create_profile(
    RequireWrite(_claims): RequireWrite,
    State(state): State<Arc<AppState>>,
    Json(req): Json<CreateFirewallProfileRequest>,
) -> impl IntoResponse {
    tracing::debug!("vm_firewall::{}", stringify!(create_profile));
    if let Err((status, msg)) = crate::validation::validate_entity_name(&req.name) {
        return (status, Json(json!({"error": msg}))).into_response();
    }
    let now = Utc::now();
    // Validate firewall rule fields
    for rule in &req.rules {
        if let Some(ref cidr) = rule.source_cidr {
            if let Err(e) = crate::validation::validate_cidr(cidr) {
                return (
                    StatusCode::BAD_REQUEST,
                    Json(json!({"error": format!("Invalid source_cidr: {}", e)})),
                )
                    .into_response();
            }
        }
        if let Some(ref prefix) = rule.log_prefix {
            if let Err(e) = crate::validation::validate_log_prefix(prefix) {
                return (
                    StatusCode::BAD_REQUEST,
                    Json(json!({"error": format!("Invalid log_prefix: {}", e)})),
                )
                    .into_response();
            }
        }
    }

    let profile = FirewallProfile {
        id: Uuid::new_v4(),
        name: req.name,
        description: req.description,
        default_action: req.default_action,
        rules: req.rules,
        managed: true,
        created: now,
        updated: now,
    };

    if let Err(e) = state
        .store
        .save_entity(PROFILES_KEY, &profile.id.to_string(), &profile)
    {
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": e.to_string() })),
        )
            .into_response();
    }

    (StatusCode::CREATED, Json(profile)).into_response()
}

pub async fn list_profiles(
    RequireRead(_claims): RequireRead,
    State(state): State<Arc<AppState>>,
) -> impl IntoResponse {
    tracing::debug!("vm_firewall::{}", stringify!(list_profiles));
    match state.store.list_entities::<FirewallProfile>(PROFILES_KEY) {
        Ok(profiles) => {
            let merged = super::net_security_discover::merge_firewall_profiles(&state, profiles);
            (StatusCode::OK, Json(merged)).into_response()
        }
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": e.to_string() })),
        )
            .into_response(),
    }
}

pub async fn get_profile(
    RequireRead(_claims): RequireRead,
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    tracing::debug!("vm_firewall::{}", stringify!(get_profile));
    match state.store.get_entity::<FirewallProfile>(PROFILES_KEY, &id) {
        Ok(Some(profile)) => (StatusCode::OK, Json(profile)).into_response(),
        Ok(None) => {
            if let Some(host) =
                super::net_security_discover::find_host_firewall_profile(&state, &id)
            {
                return (StatusCode::OK, Json(host)).into_response();
            }
            (
                StatusCode::NOT_FOUND,
                Json(json!({ "error": "Firewall profile not found" })),
            )
                .into_response()
        }
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": e.to_string() })),
        )
            .into_response(),
    }
}

pub async fn update_profile(
    RequireWrite(_claims): RequireWrite,
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    Json(req): Json<CreateFirewallProfileRequest>,
) -> impl IntoResponse {
    tracing::debug!("vm_firewall::{}", stringify!(update_profile));
    // Validate firewall rule fields
    for rule in &req.rules {
        if let Some(ref cidr) = rule.source_cidr {
            if let Err(e) = crate::validation::validate_cidr(cidr) {
                return (
                    StatusCode::BAD_REQUEST,
                    Json(json!({"error": format!("Invalid source_cidr: {}", e)})),
                )
                    .into_response();
            }
        }
        if let Some(ref prefix) = rule.log_prefix {
            if let Err(e) = crate::validation::validate_log_prefix(prefix) {
                return (
                    StatusCode::BAD_REQUEST,
                    Json(json!({"error": format!("Invalid log_prefix: {}", e)})),
                )
                    .into_response();
            }
        }
    }
    let existing = match state.store.get_entity::<FirewallProfile>(PROFILES_KEY, &id) {
        Ok(Some(p)) => p,
        Ok(None) => {
            return (
                StatusCode::NOT_FOUND,
                Json(json!({ "error": "Firewall profile not found" })),
            )
                .into_response();
        }
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": e.to_string() })),
            )
                .into_response();
        }
    };

    let profile = FirewallProfile {
        id: existing.id,
        name: req.name,
        description: req.description,
        default_action: req.default_action,
        rules: req.rules,
        managed: existing.managed,
        created: existing.created,
        updated: Utc::now(),
    };

    if let Err(e) = state.store.save_entity(PROFILES_KEY, &id, &profile) {
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": e.to_string() })),
        )
            .into_response();
    }

    if let Err(e) = reconcile_firewall(&state).await {
        tracing::warn!("Post-update firewall reconciliation failed: {}", e);
    }

    (StatusCode::OK, Json(profile)).into_response()
}

pub async fn delete_profile(
    RequireWrite(_claims): RequireWrite,
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    tracing::debug!("vm_firewall::{}", stringify!(delete_profile));
    if let Some(host) = super::net_security_discover::find_host_firewall_profile(&state, &id) {
        if super::net_security_discover::is_host_managed_profile(&host) {
            return (
                StatusCode::BAD_REQUEST,
                Json(json!({
                    "error": "This firewall profile reflects host nftables and is not managed by zyvor-fabricd"
                })),
            )
                .into_response();
        }
    }
    if let Err(e) = state.store.delete_entity(PROFILES_KEY, &id) {
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": e.to_string() })),
        )
            .into_response();
    }

    if let Err(e) = reconcile_firewall(&state).await {
        tracing::warn!("Post-delete firewall reconciliation failed: {}", e);
    }

    StatusCode::NO_CONTENT.into_response()
}

pub async fn adopt_profile(
    RequireWrite(_claims): RequireWrite,
    State(state): State<Arc<AppState>>,
    Json(req): Json<AdoptHostRequest>,
) -> impl IntoResponse {
    tracing::debug!("vm_firewall::{}", stringify!(adopt_profile));
    let host = match super::net_security_discover::find_host_firewall_profile(&state, &req.host_id)
    {
        Some(p) if super::net_security_discover::is_host_managed_profile(&p) => p,
        _ => {
            return (
                StatusCode::NOT_FOUND,
                Json(json!({ "error": "Host firewall profile not found" })),
            )
                .into_response();
        }
    };

    let stored: Vec<FirewallProfile> = state.store.list_entities(PROFILES_KEY).unwrap_or_default();
    if stored.iter().any(|p| p.name == host.name) {
        return (
            StatusCode::CONFLICT,
            Json(json!({
                "error": format!("Profile '{}' is already managed by zyvor-fabricd", host.name)
            })),
        )
            .into_response();
    }

    let now = Utc::now();
    let profile = FirewallProfile {
        id: Uuid::new_v4(),
        name: host.name,
        description: host.description,
        default_action: host.default_action,
        rules: host.rules,
        managed: true,
        created: now,
        updated: now,
    };

    if let Err(e) = state
        .store
        .save_entity(PROFILES_KEY, &profile.id.to_string(), &profile)
    {
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": e.to_string() })),
        )
            .into_response();
    }

    if let Err(e) = reconcile_firewall(&state).await {
        tracing::warn!("Post-adopt firewall reconciliation failed: {}", e);
    }

    (StatusCode::CREATED, Json(profile)).into_response()
}

// ── Firewall Zone CRUD ──────────────────────────────────────────────

pub async fn create_zone(
    RequireWrite(_claims): RequireWrite,
    State(state): State<Arc<AppState>>,
    Json(req): Json<CreateFirewallZoneRequest>,
) -> impl IntoResponse {
    tracing::debug!("vm_firewall::{}", stringify!(create_zone));
    let now = Utc::now();
    let zone = FirewallZone {
        id: Uuid::new_v4(),
        name: req.name,
        description: req.description,
        default_profile_id: req.default_profile_id,
        managed: true,
        created: now,
        updated: now,
    };

    if let Err(e) = state
        .store
        .save_entity(ZONES_KEY, &zone.id.to_string(), &zone)
    {
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": e.to_string() })),
        )
            .into_response();
    }

    (StatusCode::CREATED, Json(zone)).into_response()
}

pub async fn list_zones(
    RequireRead(_claims): RequireRead,
    State(state): State<Arc<AppState>>,
) -> impl IntoResponse {
    tracing::debug!("vm_firewall::{}", stringify!(list_zones));
    match state.store.list_entities::<FirewallZone>(ZONES_KEY) {
        Ok(zones) => {
            let merged = super::net_security_discover::merge_firewall_zones(&state, zones);
            (StatusCode::OK, Json(merged)).into_response()
        }
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": e.to_string() })),
        )
            .into_response(),
    }
}

pub async fn get_zone(
    RequireRead(_claims): RequireRead,
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    tracing::debug!("vm_firewall::{}", stringify!(get_zone));
    match state.store.get_entity::<FirewallZone>(ZONES_KEY, &id) {
        Ok(Some(zone)) => (StatusCode::OK, Json(zone)).into_response(),
        Ok(None) => {
            if let Some(host) = super::net_security_discover::find_host_firewall_zone(&state, &id) {
                return (StatusCode::OK, Json(host)).into_response();
            }
            (
                StatusCode::NOT_FOUND,
                Json(json!({ "error": "Firewall zone not found" })),
            )
                .into_response()
        }
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": e.to_string() })),
        )
            .into_response(),
    }
}

pub async fn delete_zone(
    RequireWrite(_claims): RequireWrite,
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    tracing::debug!("vm_firewall::{}", stringify!(delete_zone));
    if let Some(host) = super::net_security_discover::find_host_firewall_zone(&state, &id) {
        if super::net_security_discover::is_host_managed_zone(&host) {
            return (
                StatusCode::BAD_REQUEST,
                Json(json!({
                    "error": "This firewall zone is managed by firewalld on the host"
                })),
            )
                .into_response();
        }
    }
    if let Err(e) = state.store.delete_entity(ZONES_KEY, &id) {
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": e.to_string() })),
        )
            .into_response();
    }

    StatusCode::NO_CONTENT.into_response()
}

pub async fn adopt_zone(
    RequireWrite(_claims): RequireWrite,
    State(state): State<Arc<AppState>>,
    Json(req): Json<AdoptHostRequest>,
) -> impl IntoResponse {
    tracing::debug!("vm_firewall::{}", stringify!(adopt_zone));
    let host = match super::net_security_discover::find_host_firewall_zone(&state, &req.host_id) {
        Some(z) if super::net_security_discover::is_host_managed_zone(&z) => z,
        _ => {
            return (
                StatusCode::NOT_FOUND,
                Json(json!({ "error": "Host firewalld zone not found" })),
            )
                .into_response();
        }
    };

    let stored: Vec<FirewallZone> = state.store.list_entities(ZONES_KEY).unwrap_or_default();
    if stored.iter().any(|z| z.name == host.name) {
        return (
            StatusCode::CONFLICT,
            Json(json!({
                "error": format!("Zone '{}' is already managed by zyvor-fabricd", host.name)
            })),
        )
            .into_response();
    }

    let now = Utc::now();
    let zone = FirewallZone {
        id: Uuid::new_v4(),
        name: host.name,
        description: host.description,
        default_profile_id: host.default_profile_id,
        managed: true,
        created: now,
        updated: now,
    };

    if let Err(e) = state
        .store
        .save_entity(ZONES_KEY, &zone.id.to_string(), &zone)
    {
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": e.to_string() })),
        )
            .into_response();
    }

    (StatusCode::CREATED, Json(zone)).into_response()
}

// ── VM Firewall Assignment ──────────────────────────────────────────

pub async fn list_assignments(
    RequireRead(_claims): RequireRead,
    State(state): State<Arc<AppState>>,
) -> impl IntoResponse {
    tracing::debug!("vm_firewall::{}", stringify!(list_assignments));
    match state
        .store
        .list_entities::<VMFirewallAssignment>(ASSIGNMENTS_KEY)
    {
        Ok(assignments) => (StatusCode::OK, Json(assignments)).into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": e.to_string() })),
        )
            .into_response(),
    }
}

pub async fn get_vm_firewall(
    RequireRead(_claims): RequireRead,
    State(state): State<Arc<AppState>>,
    Path(name): Path<String>,
) -> impl IntoResponse {
    tracing::debug!("vm_firewall::{}", stringify!(get_vm_firewall));
    match state
        .store
        .get_entity::<VMFirewallAssignment>(ASSIGNMENTS_KEY, &name)
    {
        Ok(Some(assignment)) => (StatusCode::OK, Json(assignment)).into_response(),
        Ok(None) => (
            StatusCode::NOT_FOUND,
            Json(json!({ "error": "No firewall assignment for this VM" })),
        )
            .into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": e.to_string() })),
        )
            .into_response(),
    }
}

pub async fn assign_vm_firewall(
    RequireWrite(_claims): RequireWrite,
    State(state): State<Arc<AppState>>,
    Path(name): Path<String>,
    Json(req): Json<AssignFirewallRequest>,
) -> impl IntoResponse {
    tracing::debug!("vm_firewall::{}", stringify!(assign_vm_firewall));
    let assignment = VMFirewallAssignment {
        vm_name: name.clone(),
        profile_id: req.profile_id,
        zone_id: req.zone_id,
    };

    if let Err(e) = state.store.save_entity(ASSIGNMENTS_KEY, &name, &assignment) {
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": e.to_string() })),
        )
            .into_response();
    }

    if let Err(e) = reconcile_firewall(&state).await {
        tracing::warn!("Post-assign firewall reconciliation failed: {}", e);
    }

    (StatusCode::OK, Json(assignment)).into_response()
}

pub async fn remove_vm_firewall(
    RequireWrite(_claims): RequireWrite,
    State(state): State<Arc<AppState>>,
    Path(name): Path<String>,
) -> impl IntoResponse {
    tracing::debug!("vm_firewall::{}", stringify!(remove_vm_firewall));
    if let Err(e) = state.store.delete_entity(ASSIGNMENTS_KEY, &name) {
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": e.to_string() })),
        )
            .into_response();
    }

    if let Err(e) = reconcile_firewall(&state).await {
        tracing::warn!("Post-remove firewall reconciliation failed: {}", e);
    }

    StatusCode::NO_CONTENT.into_response()
}

// ── Sync and status ─────────────────────────────────────────────────

pub async fn sync_firewall(
    RequireWrite(_claims): RequireWrite,
    State(state): State<Arc<AppState>>,
) -> impl IntoResponse {
    tracing::debug!("vm_firewall::{}", stringify!(sync_firewall));
    match reconcile_firewall(&state).await {
        Ok(_) => (StatusCode::OK, Json(json!({ "status": "synced" }))).into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": e.to_string() })),
        )
            .into_response(),
    }
}

pub async fn get_firewall_status(
    RequireRead(_claims): RequireRead,
    State(state): State<Arc<AppState>>,
) -> impl IntoResponse {
    tracing::debug!("vm_firewall::{}", stringify!(get_firewall_status));
    let profiles: Vec<FirewallProfile> = match state.store.list_entities(PROFILES_KEY) {
        Ok(p) => p,
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": e.to_string() })),
            )
                .into_response();
        }
    };

    let assignments: Vec<VMFirewallAssignment> = state
        .store
        .list_entities(ASSIGNMENTS_KEY)
        .unwrap_or_default();

    let statuses: Vec<FirewallStatus> = profiles
        .iter()
        .map(|profile| {
            let assigned = assignments
                .iter()
                .filter(|a| a.profile_id == profile.id)
                .count();

            FirewallStatus {
                profile_id: profile.id,
                name: profile.name.clone(),
                assigned_vms: assigned,
                rules_count: profile.rules.len(),
                enforced: assigned > 0,
            }
        })
        .collect();

    (StatusCode::OK, Json(statuses)).into_response()
}

// ── Reconciliation ──────────────────────────────────────────────────

pub async fn reconcile_firewall(state: &AppState) -> anyhow::Result<()> {
    tracing::debug!("vm_firewall::{}", stringify!(reconcile_firewall));
    let assignments: Vec<VMFirewallAssignment> = state.store.list_entities(ASSIGNMENTS_KEY)?;
    let profiles_list: Vec<FirewallProfile> = state.store.list_entities(PROFILES_KEY)?;

    let profiles: HashMap<Uuid, FirewallProfile> =
        profiles_list.into_iter().map(|p| (p.id, p)).collect();

    let vms = build_vm_snapshots(state);

    let chains = state
        .vm_firewall
        .compiler
        .compile_all(&assignments, &profiles, &vms);

    state.vm_firewall.enforcer.sync_chains(&chains)?;

    tracing::info!(
        "Reconciled {} firewall assignments → {} chains",
        assignments.len(),
        chains.len()
    );

    Ok(())
}

fn build_vm_snapshots(state: &AppState) -> Vec<VMSnapshot> {
    let vms = match state.store.list_vms() {
        Ok(v) => v,
        Err(_) => return vec![],
    };

    vms.into_iter()
        .map(|vm| VMSnapshot {
            name: vm.name,
            ip: vm.ip,
        })
        .collect()
}
