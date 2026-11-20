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

use network_policy::compiler::VMSnapshot;
use network_policy::models::{CreateNetworkPolicyRequest, NetworkPolicy, PolicyStatus};
use networking::models::AdoptHostRequest;

use crate::server::AppState;

const STORE_KEY: &str = "network_policies";

// ── Network Policy CRUD ──────────────────────────────────────────────

pub async fn create_policy(
    RequireWrite(_claims): RequireWrite,
    State(state): State<Arc<AppState>>,
    Json(req): Json<CreateNetworkPolicyRequest>,
) -> impl IntoResponse {
    tracing::debug!("network_policy::{}", stringify!(create_policy));
    if let Err((status, msg)) = crate::validation::validate_entity_name(&req.name) {
        return (status, Json(json!({"error": msg}))).into_response();
    }
    // Validate CIDRs in ingress rules
    for rule in &req.ingress {
        for peer in &rule.from {
            if let network_policy::models::PeerSelector::Cidr(ref cidr) = peer {
                if let Err(e) = crate::validation::validate_cidr(cidr) {
                    return (
                        StatusCode::BAD_REQUEST,
                        Json(json!({"error": format!("Invalid ingress CIDR: {}", e)})),
                    )
                        .into_response();
                }
            }
        }
    }
    // Validate CIDRs in egress rules
    for rule in &req.egress {
        for peer in &rule.to {
            if let network_policy::models::PeerSelector::Cidr(ref cidr) = peer {
                if let Err(e) = crate::validation::validate_cidr(cidr) {
                    return (
                        StatusCode::BAD_REQUEST,
                        Json(json!({"error": format!("Invalid egress CIDR: {}", e)})),
                    )
                        .into_response();
                }
            }
        }
    }
    let now = Utc::now();
    let policy = NetworkPolicy {
        id: Uuid::new_v4(),
        name: req.name,
        description: req.description,
        endpoint_selector: req.endpoint_selector,
        ingress: req.ingress,
        egress: req.egress,
        enabled: req.enabled,
        managed: true,
        created: now,
        updated: now,
    };

    if let Err(e) = state
        .store
        .save_entity(STORE_KEY, &policy.id.to_string(), &policy)
    {
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": e.to_string() })),
        )
            .into_response();
    }

    // Trigger reconciliation
    if let Err(e) = reconcile_policies(&state).await {
        tracing::warn!("Post-create reconciliation failed: {}", e);
    }

    (StatusCode::CREATED, Json(policy)).into_response()
}

pub async fn list_policies(
    RequireRead(_claims): RequireRead,
    State(state): State<Arc<AppState>>,
) -> impl IntoResponse {
    tracing::debug!("network_policy::{}", stringify!(list_policies));
    match state.store.list_entities::<NetworkPolicy>(STORE_KEY) {
        Ok(policies) => {
            let merged = super::net_security_discover::merge_network_policies(&state, policies);
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
    tracing::debug!("network_policy::{}", stringify!(get_policy));
    match state.store.get_entity::<NetworkPolicy>(STORE_KEY, &id) {
        Ok(Some(policy)) => (StatusCode::OK, Json(policy)).into_response(),
        Ok(None) => {
            if let Some(policy) =
                super::net_security_discover::find_host_network_policy(&state, &id)
            {
                return (StatusCode::OK, Json(policy)).into_response();
            }
            (
                StatusCode::NOT_FOUND,
                Json(json!({ "error": "Network policy not found" })),
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
    Json(req): Json<CreateNetworkPolicyRequest>,
) -> impl IntoResponse {
    tracing::debug!("network_policy::{}", stringify!(update_policy));
    let existing = match state.store.get_entity::<NetworkPolicy>(STORE_KEY, &id) {
        Ok(Some(p)) => p,
        Ok(None) => {
            return (
                StatusCode::NOT_FOUND,
                Json(json!({ "error": "Network policy not found" })),
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

    let policy = NetworkPolicy {
        id: existing.id,
        name: req.name,
        description: req.description,
        endpoint_selector: req.endpoint_selector,
        ingress: req.ingress,
        egress: req.egress,
        enabled: req.enabled,
        managed: existing.managed,
        created: existing.created,
        updated: Utc::now(),
    };

    if let Err(e) = state.store.save_entity(STORE_KEY, &id, &policy) {
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": e.to_string() })),
        )
            .into_response();
    }

    if let Err(e) = reconcile_policies(&state).await {
        tracing::warn!("Post-update reconciliation failed: {}", e);
    }

    (StatusCode::OK, Json(policy)).into_response()
}

pub async fn delete_policy(
    RequireWrite(_claims): RequireWrite,
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    tracing::debug!("network_policy::{}", stringify!(delete_policy));
    if let Some(host) = super::net_security_discover::find_host_network_policy(&state, &id) {
        if super::net_security_discover::is_host_managed_network_policy(&host) {
            return (
                StatusCode::BAD_REQUEST,
                Json(json!({
                    "error": "This network policy reflects host nftables and is not managed by zyvor-fabricd"
                })),
            )
                .into_response();
        }
    }
    if let Err(e) = state.store.delete_entity(STORE_KEY, &id) {
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": e.to_string() })),
        )
            .into_response();
    }

    if let Err(e) = reconcile_policies(&state).await {
        tracing::warn!("Post-delete reconciliation failed: {}", e);
    }

    StatusCode::NO_CONTENT.into_response()
}

pub async fn adopt_policy(
    RequireWrite(_claims): RequireWrite,
    State(state): State<Arc<AppState>>,
    Json(req): Json<AdoptHostRequest>,
) -> impl IntoResponse {
    tracing::debug!("network_policy::{}", stringify!(adopt_policy));
    let host = match super::net_security_discover::find_host_network_policy(&state, &req.host_id) {
        Some(p) if super::net_security_discover::is_host_managed_network_policy(&p) => p,
        _ => {
            return (
                StatusCode::NOT_FOUND,
                Json(json!({ "error": "Host network policy not found" })),
            )
                .into_response();
        }
    };

    let stored: Vec<NetworkPolicy> = state.store.list_entities(STORE_KEY).unwrap_or_default();
    if stored.iter().any(|p| p.name == host.name) {
        return (
            StatusCode::CONFLICT,
            Json(json!({
                "error": format!("Network policy '{}' is already managed by zyvor-fabricd", host.name)
            })),
        )
            .into_response();
    }

    let now = Utc::now();
    let policy = NetworkPolicy {
        id: Uuid::new_v4(),
        name: host.name,
        description: host.description,
        endpoint_selector: host.endpoint_selector,
        ingress: host.ingress,
        egress: host.egress,
        enabled: false,
        managed: true,
        created: now,
        updated: now,
    };

    if let Err(e) = state
        .store
        .save_entity(STORE_KEY, &policy.id.to_string(), &policy)
    {
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": e.to_string() })),
        )
            .into_response();
    }

    (StatusCode::CREATED, Json(policy)).into_response()
}

// ── Identity queries ─────────────────────────────────────────────────

pub async fn list_identities(
    RequireRead(_claims): RequireRead,
    State(state): State<Arc<AppState>>,
) -> impl IntoResponse {
    tracing::debug!("network_policy::{}", stringify!(list_identities));
    let managed = state.policy_engine.allocator.list_identities();
    let identities = super::net_security_discover::merge_identities(managed);
    (StatusCode::OK, Json(identities)).into_response()
}

pub async fn get_identity(
    RequireRead(_claims): RequireRead,
    State(state): State<Arc<AppState>>,
    Path(id): Path<u32>,
) -> impl IntoResponse {
    tracing::debug!("network_policy::{}", stringify!(get_identity));
    if let Some(identity) = state.policy_engine.allocator.get_identity(id) {
        return (StatusCode::OK, Json(identity)).into_response();
    }
    if let Some(identity) = super::net_security_discover::find_host_identity(id) {
        return (StatusCode::OK, Json(identity)).into_response();
    }
    (
        StatusCode::NOT_FOUND,
        Json(json!({ "error": "Identity not found" })),
    )
        .into_response()
}

pub async fn adopt_identity(
    RequireWrite(_claims): RequireWrite,
    State(state): State<Arc<AppState>>,
    Json(req): Json<AdoptHostRequest>,
) -> impl IntoResponse {
    tracing::debug!("network_policy::{}", stringify!(adopt_identity));
    let host_id = match req.host_id.parse::<u32>() {
        Ok(id) => id,
        Err(_) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(json!({ "error": "Invalid host identity id" })),
            )
                .into_response();
        }
    };

    let host = match super::net_security_discover::find_host_identity(host_id) {
        Some(i) if super::net_security_discover::is_host_managed_identity(&i) => i,
        _ => {
            return (
                StatusCode::NOT_FOUND,
                Json(json!({ "error": "Host identity not found" })),
            )
                .into_response();
        }
    };

    let labels: std::collections::HashMap<String, String> = host
        .labels
        .iter()
        .map(|(k, v)| (k.clone(), v.clone()))
        .collect();

    if state
        .policy_engine
        .allocator
        .get_identity_for_labels(&labels)
        .is_some()
    {
        return (
            StatusCode::CONFLICT,
            Json(json!({
                "error": "An identity with these labels is already managed by zyvor-fabricd"
            })),
        )
            .into_response();
    }

    let mut allocated_id = None;
    for endpoint in &host.endpoints {
        match state
            .policy_engine
            .allocator
            .allocate_or_get(&labels, endpoint)
        {
            Ok(id) => {
                allocated_id = Some(id);
                if endpoint.parse::<std::net::IpAddr>().is_ok() {
                    if let Err(e) = state
                        .policy_engine
                        .allocator
                        .update_ip_mapping(endpoint, id)
                    {
                        tracing::warn!("Failed to map IP {} to identity {}: {}", endpoint, id, e);
                    }
                }
            }
            Err(e) => {
                return (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(json!({ "error": e.to_string() })),
                )
                    .into_response();
            }
        }
    }

    let Some(id) = allocated_id else {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": "Host identity has no endpoints to adopt" })),
        )
            .into_response();
    };

    match state.policy_engine.allocator.get_identity(id) {
        Some(identity) => (StatusCode::CREATED, Json(identity)).into_response(),
        None => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": "Failed to load adopted identity" })),
        )
            .into_response(),
    }
}

// ── Policy operations ────────────────────────────────────────────────

pub async fn sync_policies(
    RequireWrite(_claims): RequireWrite,
    State(state): State<Arc<AppState>>,
) -> impl IntoResponse {
    tracing::debug!("network_policy::{}", stringify!(sync_policies));
    match reconcile_policies(&state).await {
        Ok(_) => (StatusCode::OK, Json(json!({ "status": "synced" }))).into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": e.to_string() })),
        )
            .into_response(),
    }
}

pub async fn get_policy_status(
    RequireRead(_claims): RequireRead,
    State(state): State<Arc<AppState>>,
) -> impl IntoResponse {
    tracing::debug!("network_policy::{}", stringify!(get_policy_status));
    let policies: Vec<NetworkPolicy> = match state.store.list_entities(STORE_KEY) {
        Ok(p) => p,
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": e.to_string() })),
            )
                .into_response();
        }
    };

    let vms = build_vm_snapshots(&state);
    let statuses: Vec<PolicyStatus> = policies
        .iter()
        .map(|policy| {
            let matching = vms
                .iter()
                .filter(|vm| policy.endpoint_selector.matches(&vm.labels))
                .count();
            let compiled = state.policy_engine.compiler.compile_policy(policy, &vms);

            PolicyStatus {
                policy_id: policy.id,
                policy_name: policy.name.clone(),
                matching_endpoints: matching,
                compiled_rules_count: compiled.len(),
                enforced: policy.enabled,
                last_synced: Some(Utc::now()),
            }
        })
        .collect();

    (StatusCode::OK, Json(statuses)).into_response()
}

// ── Reconciliation ───────────────────────────────────────────────────

/// Shared reconciliation logic used by both API handlers and the background task.
pub async fn reconcile_policies(state: &AppState) -> anyhow::Result<()> {
    tracing::debug!("network_policy::{}", stringify!(reconcile_policies));
    let policies: Vec<NetworkPolicy> = state.store.list_entities(STORE_KEY)?;

    let vms = build_vm_snapshots(state);

    // Re-register all identities (idempotent)
    for vm in &vms {
        if !vm.labels.is_empty() {
            if let Ok(id) = state
                .policy_engine
                .allocator
                .allocate_or_get(&vm.labels, &vm.name)
            {
                if let Some(ref ip) = vm.ip {
                    if let Err(e) = state.policy_engine.allocator.update_ip_mapping(ip, id) {
                        tracing::error!("Failed to update IP mapping for VM '{}': {}", vm.name, e);
                    }
                }
            }
        }
    }

    // Compile all enabled policies
    let enabled: Vec<NetworkPolicy> = policies.into_iter().filter(|p| p.enabled).collect();
    let rules = state.policy_engine.compiler.compile_all(&enabled, &vms);

    // Sync to nftables
    state.policy_engine.enforcer.sync_all(&rules)?;

    tracing::info!(
        "Reconciled {} policies → {} compiled rules",
        enabled.len(),
        rules.len()
    );

    Ok(())
}

/// Build VM snapshots from the state store for policy compilation.
fn build_vm_snapshots(state: &AppState) -> Vec<VMSnapshot> {
    let vms = match state.store.list_vms() {
        Ok(v) => v,
        Err(_) => return vec![],
    };

    vms.into_iter()
        .filter_map(|vm| {
            let labels = vm.labels.clone().unwrap_or_default();
            if labels.is_empty() {
                return None;
            }
            Some(VMSnapshot {
                name: vm.name,
                labels,
                ip: vm.ip,
            })
        })
        .collect()
}
