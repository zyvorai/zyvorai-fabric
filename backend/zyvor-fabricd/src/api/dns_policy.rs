// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: LicenseRef-Zyvor-Production-1.0

use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use chrono::Utc;
use security::{RequireRead, RequireWrite};
use serde_json::json;
use std::sync::Arc;
use uuid::Uuid;

use dns_policy::models::{CreateDnsPolicyRequest, CreateDnsZoneRequest, DnsPolicy, DnsZone};
use dns_policy::resolver::VMSnapshot;
use networking::models::AdoptHostRequest;

use crate::server::AppState;

const ZONES_KEY: &str = "dns_zones";
const POLICIES_KEY: &str = "dns_policies";

// ── DNS Zone CRUD ───────────────────────────────────────────────────

pub async fn create_zone(
    RequireWrite(_claims): RequireWrite,
    State(state): State<Arc<AppState>>,
    Json(req): Json<CreateDnsZoneRequest>,
) -> impl IntoResponse {
    tracing::debug!("dns_policy::{}", stringify!(create_zone));
    if let Err(msg) = crate::validation::validate_hostname(&req.name) {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({"error": format!("Invalid zone name: {}", msg)})),
        )
            .into_response();
    }
    let now = Utc::now();
    let zone = DnsZone {
        id: Uuid::new_v4(),
        name: req.name,
        description: req.description,
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
    tracing::debug!("dns_policy::{}", stringify!(list_zones));
    match state.store.list_entities::<DnsZone>(ZONES_KEY) {
        Ok(zones) => {
            let merged = super::net_security_discover::merge_dns_zones(&state, zones);
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
    tracing::debug!("dns_policy::{}", stringify!(get_zone));
    match state.store.get_entity::<DnsZone>(ZONES_KEY, &id) {
        Ok(Some(zone)) => (StatusCode::OK, Json(zone)).into_response(),
        Ok(None) => {
            if let Some(zone) = super::net_security_discover::find_host_dns_zone(&state, &id) {
                return (StatusCode::OK, Json(zone)).into_response();
            }
            (
                StatusCode::NOT_FOUND,
                Json(json!({ "error": "DNS zone not found" })),
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

/// PUT /api/dns-zones/:id - Update a managed DNS zone's name/description
pub async fn update_zone(
    RequireWrite(_claims): RequireWrite,
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    Json(req): Json<CreateDnsZoneRequest>,
) -> impl IntoResponse {
    tracing::debug!("dns_policy::{}", stringify!(update_zone));
    if let Err(msg) = crate::validation::validate_hostname(&req.name) {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({"error": format!("Invalid zone name: {}", msg)})),
        )
            .into_response();
    }
    if let Some(host) = super::net_security_discover::find_host_dns_zone(&state, &id) {
        if super::net_security_discover::is_host_managed_dns_zone(&host) {
            return (
                StatusCode::BAD_REQUEST,
                Json(json!({
                    "error": "This DNS zone exists on the host and is not managed by zyvor-fabricd"
                })),
            )
                .into_response();
        }
    }
    let mut zone = match state.store.get_entity::<DnsZone>(ZONES_KEY, &id) {
        Ok(Some(zone)) => zone,
        Ok(None) => {
            return (
                StatusCode::NOT_FOUND,
                Json(json!({ "error": "DNS zone not found" })),
            )
                .into_response()
        }
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": e.to_string() })),
            )
                .into_response()
        }
    };
    zone.name = req.name;
    zone.description = req.description;
    zone.updated = Utc::now();
    if let Err(e) = state.store.save_entity(ZONES_KEY, &id, &zone) {
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": e.to_string() })),
        )
            .into_response();
    }
    (StatusCode::OK, Json(zone)).into_response()
}

pub async fn delete_zone(
    RequireWrite(_claims): RequireWrite,
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    tracing::debug!("dns_policy::{}", stringify!(delete_zone));
    if let Some(host) = super::net_security_discover::find_host_dns_zone(&state, &id) {
        if super::net_security_discover::is_host_managed_dns_zone(&host) {
            return (
                StatusCode::BAD_REQUEST,
                Json(json!({
                    "error": "This DNS zone exists on the host and is not managed by zyvor-fabricd"
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
    tracing::debug!("dns_policy::{}", stringify!(adopt_zone));
    let host = match super::net_security_discover::find_host_dns_zone(&state, &req.host_id) {
        Some(z) if super::net_security_discover::is_host_managed_dns_zone(&z) => z,
        _ => {
            return (
                StatusCode::NOT_FOUND,
                Json(json!({ "error": "Host DNS zone not found" })),
            )
                .into_response();
        }
    };

    let stored: Vec<DnsZone> = state.store.list_entities(ZONES_KEY).unwrap_or_default();
    if stored.iter().any(|z| z.name == host.name) {
        return (
            StatusCode::CONFLICT,
            Json(json!({
                "error": format!("DNS zone '{}' is already managed by zyvor-fabricd", host.name)
            })),
        )
            .into_response();
    }

    let now = Utc::now();
    let zone = DnsZone {
        id: Uuid::new_v4(),
        name: host.name,
        description: host.description,
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

// ── DNS Policy CRUD ─────────────────────────────────────────────────

pub async fn create_policy(
    RequireWrite(_claims): RequireWrite,
    State(state): State<Arc<AppState>>,
    Json(req): Json<CreateDnsPolicyRequest>,
) -> impl IntoResponse {
    tracing::debug!("dns_policy::{}", stringify!(create_policy));
    let now = Utc::now();
    let policy = DnsPolicy {
        id: Uuid::new_v4(),
        name: req.name,
        description: String::new(),
        zone_id: req.zone_id,
        selector: req.selector,
        record_template: req.record_template,
        record_type: req.record_type,
        enabled: req.enabled,
        managed: true,
        created: now,
        updated: now,
    };

    if let Err(e) = state
        .store
        .save_entity(POLICIES_KEY, &policy.id.to_string(), &policy)
    {
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": e.to_string() })),
        )
            .into_response();
    }

    if let Err(e) = reconcile_dns(&state).await {
        tracing::warn!("Post-create DNS reconciliation failed: {}", e);
    }

    (StatusCode::CREATED, Json(policy)).into_response()
}

pub async fn list_policies(
    RequireRead(_claims): RequireRead,
    State(state): State<Arc<AppState>>,
) -> impl IntoResponse {
    tracing::debug!("dns_policy::{}", stringify!(list_policies));
    match state.store.list_entities::<DnsPolicy>(POLICIES_KEY) {
        Ok(policies) => {
            let merged = super::net_security_discover::merge_dns_policies(&state, policies);
            (StatusCode::OK, Json(merged)).into_response()
        }
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": e.to_string() })),
        )
            .into_response(),
    }
}

pub async fn get_policy(
    RequireRead(_claims): RequireRead,
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    tracing::debug!("dns_policy::{}", stringify!(get_policy));
    match state.store.get_entity::<DnsPolicy>(POLICIES_KEY, &id) {
        Ok(Some(policy)) => (StatusCode::OK, Json(policy)).into_response(),
        Ok(None) => {
            if let Some(policy) = super::net_security_discover::find_host_dns_policy(&state, &id) {
                return (StatusCode::OK, Json(policy)).into_response();
            }
            (
                StatusCode::NOT_FOUND,
                Json(json!({ "error": "DNS policy not found" })),
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

pub async fn update_policy(
    RequireWrite(_claims): RequireWrite,
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    Json(req): Json<CreateDnsPolicyRequest>,
) -> impl IntoResponse {
    tracing::debug!("dns_policy::{}", stringify!(update_policy));
    let existing = match state.store.get_entity::<DnsPolicy>(POLICIES_KEY, &id) {
        Ok(Some(p)) => p,
        Ok(None) => {
            return (
                StatusCode::NOT_FOUND,
                Json(json!({ "error": "DNS policy not found" })),
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

    let policy = DnsPolicy {
        id: existing.id,
        name: req.name,
        description: existing.description,
        zone_id: req.zone_id,
        selector: req.selector,
        record_template: req.record_template,
        record_type: req.record_type,
        enabled: req.enabled,
        managed: existing.managed,
        created: existing.created,
        updated: Utc::now(),
    };

    if let Err(e) = state.store.save_entity(POLICIES_KEY, &id, &policy) {
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": e.to_string() })),
        )
            .into_response();
    }

    if let Err(e) = reconcile_dns(&state).await {
        tracing::warn!("Post-update DNS reconciliation failed: {}", e);
    }

    (StatusCode::OK, Json(policy)).into_response()
}

pub async fn delete_policy(
    RequireWrite(_claims): RequireWrite,
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    tracing::debug!("dns_policy::{}", stringify!(delete_policy));
    if let Some(host) = super::net_security_discover::find_host_dns_policy(&state, &id) {
        if super::net_security_discover::is_host_managed_dns_policy(&host) {
            return (
                StatusCode::BAD_REQUEST,
                Json(json!({
                    "error": "This DNS policy reflects host resolver config and is not managed by zyvor-fabricd"
                })),
            )
                .into_response();
        }
    }
    if let Err(e) = state.store.delete_entity(POLICIES_KEY, &id) {
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": e.to_string() })),
        )
            .into_response();
    }

    if let Err(e) = reconcile_dns(&state).await {
        tracing::warn!("Post-delete DNS reconciliation failed: {}", e);
    }

    StatusCode::NO_CONTENT.into_response()
}

pub async fn adopt_policy(
    RequireWrite(_claims): RequireWrite,
    State(state): State<Arc<AppState>>,
    Json(req): Json<AdoptHostRequest>,
) -> impl IntoResponse {
    tracing::debug!("dns_policy::{}", stringify!(adopt_policy));
    let host = match super::net_security_discover::find_host_dns_policy(&state, &req.host_id) {
        Some(p) if super::net_security_discover::is_host_managed_dns_policy(&p) => p,
        _ => {
            return (
                StatusCode::NOT_FOUND,
                Json(json!({ "error": "Host DNS policy not found" })),
            )
                .into_response();
        }
    };

    let zones: Vec<DnsZone> = state.store.list_entities(ZONES_KEY).unwrap_or_default();
    let zone_id = zones.first().map(|z| z.id).unwrap_or(host.zone_id);

    let stored: Vec<DnsPolicy> = state.store.list_entities(POLICIES_KEY).unwrap_or_default();
    if stored.iter().any(|p| p.name == host.name) {
        return (
            StatusCode::CONFLICT,
            Json(json!({
                "error": format!("DNS policy '{}' is already managed by zyvor-fabricd", host.name)
            })),
        )
            .into_response();
    }

    let now = Utc::now();
    let policy = DnsPolicy {
        id: Uuid::new_v4(),
        name: host.name,
        description: host.description,
        zone_id,
        selector: host.selector,
        record_template: host.record_template,
        record_type: host.record_type,
        enabled: false,
        managed: true,
        created: now,
        updated: now,
    };

    if let Err(e) = state
        .store
        .save_entity(POLICIES_KEY, &policy.id.to_string(), &policy)
    {
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": e.to_string() })),
        )
            .into_response();
    }

    (StatusCode::CREATED, Json(policy)).into_response()
}

// ── DNS records and sync ────────────────────────────────────────────

pub async fn list_dns_records(
    RequireRead(_claims): RequireRead,
    State(state): State<Arc<AppState>>,
) -> impl IntoResponse {
    tracing::debug!("dns_policy::{}", stringify!(list_dns_records));
    let policies: Vec<DnsPolicy> = match state.store.list_entities(POLICIES_KEY) {
        Ok(p) => p,
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": e.to_string() })),
            )
                .into_response();
        }
    };

    let zones: Vec<DnsZone> = match state.store.list_entities(ZONES_KEY) {
        Ok(z) => z,
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": e.to_string() })),
            )
                .into_response();
        }
    };

    let vms = build_vm_snapshots(&state);
    let records = state
        .dns_manager
        .resolver
        .resolve_all(&policies, &zones, &vms);

    (StatusCode::OK, Json(records)).into_response()
}

pub async fn sync_dns_policies(
    RequireWrite(_claims): RequireWrite,
    State(state): State<Arc<AppState>>,
) -> impl IntoResponse {
    tracing::debug!("dns_policy::{}", stringify!(sync_dns_policies));
    match reconcile_dns(&state).await {
        Ok(_) => (StatusCode::OK, Json(json!({ "status": "synced" }))).into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": e.to_string() })),
        )
            .into_response(),
    }
}

// ── Reconciliation ──────────────────────────────────────────────────

pub async fn reconcile_dns(state: &AppState) -> anyhow::Result<()> {
    tracing::debug!("dns_policy::{}", stringify!(reconcile_dns));
    let policies: Vec<DnsPolicy> = state.store.list_entities(POLICIES_KEY)?;
    let zones: Vec<DnsZone> = state.store.list_entities(ZONES_KEY)?;

    let vms = build_vm_snapshots(state);

    let enabled: Vec<DnsPolicy> = policies.into_iter().filter(|p| p.enabled).collect();
    let records = state
        .dns_manager
        .resolver
        .resolve_all(&enabled, &zones, &vms);

    state.dns_manager.enforcer.sync_records(&records, &zones)?;

    tracing::info!(
        "Reconciled {} DNS policies → {} records",
        enabled.len(),
        records.len()
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
            labels: vm.labels.clone().unwrap_or_default(),
            ip: vm.ip,
        })
        .collect()
}
