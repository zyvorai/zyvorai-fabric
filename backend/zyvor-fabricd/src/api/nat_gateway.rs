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

use nat_gateway::compiler::VMSnapshot;
use nat_gateway::models::{
    CreateNatGatewayRequest, CreateNatPoolRequest, CreateNatRuleRequest, NatGatewayConfig, NatPool,
    NatRule, NatStatus,
};
use networking::models::AdoptHostRequest;

use crate::server::AppState;

const RULE_STORE_KEY: &str = "nat_rules";
const POOL_STORE_KEY: &str = "nat_pools";
const GATEWAY_STORE_KEY: &str = "nat_gateways";

// ── NAT Rule CRUD ───────────────────────────────────────────────────

pub async fn create_nat_rule(
    RequireWrite(_claims): RequireWrite,
    State(state): State<Arc<AppState>>,
    Json(req): Json<CreateNatRuleRequest>,
) -> impl IntoResponse {
    tracing::debug!("nat_gateway::{}", stringify!(create_nat_rule));
    if let Err((status, msg)) = crate::validation::validate_entity_name(&req.name) {
        return (status, Json(json!({"error": msg}))).into_response();
    }
    if let Some(ref cidr) = req.source_cidr {
        if let Err(e) = crate::validation::validate_cidr(cidr) {
            return (
                StatusCode::BAD_REQUEST,
                Json(json!({"error": format!("Invalid source_cidr: {}", e)})),
            )
                .into_response();
        }
    }
    if let Some(ref cidr) = req.dest_cidr {
        if let Err(e) = crate::validation::validate_cidr(cidr) {
            return (
                StatusCode::BAD_REQUEST,
                Json(json!({"error": format!("Invalid dest_cidr: {}", e)})),
            )
                .into_response();
        }
    }
    if let Some(ref ip) = req.translate_to {
        if let Err(e) = crate::validation::validate_ip_address(ip) {
            return (
                StatusCode::BAD_REQUEST,
                Json(json!({"error": format!("Invalid translate_to: {}", e)})),
            )
                .into_response();
        }
    }
    if let Some(ref iface) = req.outbound_interface {
        if let Err(msg) = crate::validation::validate_hostname(iface) {
            return (
                StatusCode::BAD_REQUEST,
                Json(json!({"error": format!("Invalid outbound_interface: {}", msg)})),
            )
                .into_response();
        }
    }
    let now = Utc::now();
    let rule = NatRule {
        id: Uuid::new_v4(),
        name: req.name,
        description: req.description,
        rule_type: req.rule_type,
        selector: req.selector,
        protocol: req.protocol,
        source_cidr: req.source_cidr,
        dest_cidr: req.dest_cidr,
        dest_port: req.dest_port,
        dest_port_end: req.dest_port_end,
        translate_to: req.translate_to,
        translate_port: req.translate_port,
        pool_id: req.pool_id,
        outbound_interface: req.outbound_interface,
        enabled: req.enabled,
        managed: true,
        created: now,
        updated: now,
    };

    if let Err(e) = state
        .store
        .save_entity(RULE_STORE_KEY, &rule.id.to_string(), &rule)
    {
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": e.to_string() })),
        )
            .into_response();
    }

    if let Err(e) = reconcile_nat(&state).await {
        tracing::warn!("Post-create NAT reconciliation failed: {}", e);
    }

    (StatusCode::CREATED, Json(rule)).into_response()
}

pub async fn list_nat_rules(
    RequireRead(_claims): RequireRead,
    State(state): State<Arc<AppState>>,
) -> impl IntoResponse {
    tracing::debug!("nat_gateway::{}", stringify!(list_nat_rules));
    match state.store.list_entities::<NatRule>(RULE_STORE_KEY) {
        Ok(rules) => {
            let merged = super::net_security_discover::merge_nat_rules(&state, rules);
            (StatusCode::OK, Json(merged)).into_response()
        }
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": e.to_string() })),
        )
            .into_response(),
    }
}

pub async fn get_nat_rule(
    RequireRead(_claims): RequireRead,
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    tracing::debug!("nat_gateway::{}", stringify!(get_nat_rule));
    match state.store.get_entity::<NatRule>(RULE_STORE_KEY, &id) {
        Ok(Some(rule)) => (StatusCode::OK, Json(rule)).into_response(),
        Ok(None) => {
            if let Some(host) = super::net_security_discover::find_host_nat_rule(&state, &id) {
                return (StatusCode::OK, Json(host)).into_response();
            }
            (
                StatusCode::NOT_FOUND,
                Json(json!({ "error": "NAT rule not found" })),
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

pub async fn update_nat_rule(
    RequireWrite(_claims): RequireWrite,
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    Json(req): Json<CreateNatRuleRequest>,
) -> impl IntoResponse {
    tracing::debug!("nat_gateway::{}", stringify!(update_nat_rule));
    if let Some(host) = super::net_security_discover::find_host_nat_rule(&state, &id) {
        if super::net_security_discover::is_host_managed_nat(&host) {
            return (
                StatusCode::BAD_REQUEST,
                Json(json!({
                    "error": "Cannot update a host-discovered NAT rule; adopt it first"
                })),
            )
                .into_response();
        }
    }

    let existing = match state.store.get_entity::<NatRule>(RULE_STORE_KEY, &id) {
        Ok(Some(r)) => r,
        Ok(None) => {
            return (
                StatusCode::NOT_FOUND,
                Json(json!({ "error": "NAT rule not found" })),
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

    let rule = NatRule {
        id: existing.id,
        name: req.name,
        description: req.description,
        rule_type: req.rule_type,
        selector: req.selector,
        protocol: req.protocol,
        source_cidr: req.source_cidr,
        dest_cidr: req.dest_cidr,
        dest_port: req.dest_port,
        dest_port_end: req.dest_port_end,
        translate_to: req.translate_to,
        translate_port: req.translate_port,
        pool_id: req.pool_id,
        outbound_interface: req.outbound_interface,
        enabled: req.enabled,
        managed: existing.managed,
        created: existing.created,
        updated: Utc::now(),
    };

    if let Err(e) = state.store.save_entity(RULE_STORE_KEY, &id, &rule) {
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": e.to_string() })),
        )
            .into_response();
    }

    if let Err(e) = reconcile_nat(&state).await {
        tracing::warn!("Post-update NAT reconciliation failed: {}", e);
    }

    (StatusCode::OK, Json(rule)).into_response()
}

pub async fn delete_nat_rule(
    RequireWrite(_claims): RequireWrite,
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    tracing::debug!("nat_gateway::{}", stringify!(delete_nat_rule));
    if let Some(host) = super::net_security_discover::find_host_nat_rule(&state, &id) {
        if super::net_security_discover::is_host_managed_nat(&host) {
            return (
                StatusCode::BAD_REQUEST,
                Json(json!({
                    "error": "This NAT rule exists on the host and is not managed by zyvor-fabricd"
                })),
            )
                .into_response();
        }
    }
    if let Err(e) = state.store.delete_entity(RULE_STORE_KEY, &id) {
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": e.to_string() })),
        )
            .into_response();
    }

    if let Err(e) = reconcile_nat(&state).await {
        tracing::warn!("Post-delete NAT reconciliation failed: {}", e);
    }

    StatusCode::NO_CONTENT.into_response()
}

pub async fn adopt_nat_rule(
    RequireWrite(_claims): RequireWrite,
    State(state): State<Arc<AppState>>,
    Json(req): Json<AdoptHostRequest>,
) -> impl IntoResponse {
    tracing::debug!("nat_gateway::{}", stringify!(adopt_nat_rule));
    let host = match super::net_security_discover::find_host_nat_rule(&state, &req.host_id) {
        Some(r) if super::net_security_discover::is_host_managed_nat(&r) => r,
        _ => {
            return (
                StatusCode::NOT_FOUND,
                Json(json!({ "error": "Host NAT rule not found" })),
            )
                .into_response();
        }
    };

    let stored: Vec<NatRule> = state
        .store
        .list_entities(RULE_STORE_KEY)
        .unwrap_or_default();
    if stored.iter().any(|r| r.name == host.name) {
        return (
            StatusCode::CONFLICT,
            Json(json!({
                "error": format!("NAT rule '{}' is already managed by zyvor-fabricd", host.name)
            })),
        )
            .into_response();
    }

    let now = Utc::now();
    let rule = NatRule {
        id: Uuid::new_v4(),
        name: host.name,
        description: host.description,
        rule_type: host.rule_type,
        selector: host.selector,
        protocol: host.protocol,
        source_cidr: host.source_cidr,
        dest_cidr: host.dest_cidr,
        dest_port: host.dest_port,
        dest_port_end: host.dest_port_end,
        translate_to: host.translate_to,
        translate_port: host.translate_port,
        pool_id: host.pool_id,
        outbound_interface: host.outbound_interface,
        enabled: host.enabled,
        managed: true,
        created: now,
        updated: now,
    };

    if let Err(e) = state
        .store
        .save_entity(RULE_STORE_KEY, &rule.id.to_string(), &rule)
    {
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": e.to_string() })),
        )
            .into_response();
    }

    if let Err(e) = reconcile_nat(&state).await {
        tracing::warn!("Post-adopt NAT reconciliation failed: {}", e);
    }

    (StatusCode::CREATED, Json(rule)).into_response()
}

// ── NAT Pool CRUD ───────────────────────────────────────────────────

pub async fn create_nat_pool(
    RequireWrite(_claims): RequireWrite,
    State(state): State<Arc<AppState>>,
    Json(req): Json<CreateNatPoolRequest>,
) -> impl IntoResponse {
    tracing::debug!("nat_gateway::{}", stringify!(create_nat_pool));
    for ip_range in &req.ip_ranges {
        if let Err(e) = crate::validation::validate_ip_address(ip_range) {
            return (
                StatusCode::BAD_REQUEST,
                Json(json!({"error": format!("Invalid ip_range: {}", e)})),
            )
                .into_response();
        }
    }
    let now = Utc::now();
    let pool = NatPool {
        id: Uuid::new_v4(),
        name: req.name,
        ip_ranges: req.ip_ranges,
        port_range: req.port_range,
        created: now,
        updated: now,
    };

    if let Err(e) = state
        .store
        .save_entity(POOL_STORE_KEY, &pool.id.to_string(), &pool)
    {
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": e.to_string() })),
        )
            .into_response();
    }

    (StatusCode::CREATED, Json(pool)).into_response()
}

pub async fn list_nat_pools(
    RequireRead(_claims): RequireRead,
    State(state): State<Arc<AppState>>,
) -> impl IntoResponse {
    tracing::debug!("nat_gateway::{}", stringify!(list_nat_pools));
    match state.store.list_entities::<NatPool>(POOL_STORE_KEY) {
        Ok(pools) => (StatusCode::OK, Json(pools)).into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": e.to_string() })),
        )
            .into_response(),
    }
}

pub async fn get_nat_pool(
    RequireRead(_claims): RequireRead,
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    tracing::debug!("nat_gateway::{}", stringify!(get_nat_pool));
    match state.store.get_entity::<NatPool>(POOL_STORE_KEY, &id) {
        Ok(Some(pool)) => (StatusCode::OK, Json(pool)).into_response(),
        Ok(None) => (
            StatusCode::NOT_FOUND,
            Json(json!({ "error": "NAT pool not found" })),
        )
            .into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": e.to_string() })),
        )
            .into_response(),
    }
}

pub async fn delete_nat_pool(
    RequireWrite(_claims): RequireWrite,
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    tracing::debug!("nat_gateway::{}", stringify!(delete_nat_pool));
    if let Err(e) = state.store.delete_entity(POOL_STORE_KEY, &id) {
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": e.to_string() })),
        )
            .into_response();
    }

    StatusCode::NO_CONTENT.into_response()
}

// ── NAT Gateway CRUD ────────────────────────────────────────────────

pub async fn create_nat_gateway(
    RequireWrite(_claims): RequireWrite,
    State(state): State<Arc<AppState>>,
    Json(req): Json<CreateNatGatewayRequest>,
) -> impl IntoResponse {
    tracing::debug!("nat_gateway::{}", stringify!(create_nat_gateway));
    if let Err(msg) = crate::validation::validate_hostname(&req.outbound_interface) {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({"error": format!("Invalid outbound_interface: {}", msg)})),
        )
            .into_response();
    }
    if let Err(e) = crate::validation::validate_cidr(&req.subnet) {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({"error": format!("Invalid subnet: {}", e)})),
        )
            .into_response();
    }
    let now = Utc::now();
    let gw = NatGatewayConfig {
        id: Uuid::new_v4(),
        name: req.name,
        description: req.description,
        subnet: req.subnet,
        outbound_interface: req.outbound_interface,
        selector: req.selector,
        enabled: req.enabled,
        created: now,
        updated: now,
    };

    if let Err(e) = state
        .store
        .save_entity(GATEWAY_STORE_KEY, &gw.id.to_string(), &gw)
    {
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": e.to_string() })),
        )
            .into_response();
    }

    if let Err(e) = reconcile_nat(&state).await {
        tracing::warn!("Post-create NAT gateway reconciliation failed: {}", e);
    }

    (StatusCode::CREATED, Json(gw)).into_response()
}

pub async fn list_nat_gateways(
    RequireRead(_claims): RequireRead,
    State(state): State<Arc<AppState>>,
) -> impl IntoResponse {
    tracing::debug!("nat_gateway::{}", stringify!(list_nat_gateways));
    match state
        .store
        .list_entities::<NatGatewayConfig>(GATEWAY_STORE_KEY)
    {
        Ok(gateways) => (StatusCode::OK, Json(gateways)).into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": e.to_string() })),
        )
            .into_response(),
    }
}

pub async fn get_nat_gateway(
    RequireRead(_claims): RequireRead,
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    tracing::debug!("nat_gateway::{}", stringify!(get_nat_gateway));
    match state
        .store
        .get_entity::<NatGatewayConfig>(GATEWAY_STORE_KEY, &id)
    {
        Ok(Some(gw)) => (StatusCode::OK, Json(gw)).into_response(),
        Ok(None) => (
            StatusCode::NOT_FOUND,
            Json(json!({ "error": "NAT gateway not found" })),
        )
            .into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": e.to_string() })),
        )
            .into_response(),
    }
}

pub async fn delete_nat_gateway(
    RequireWrite(_claims): RequireWrite,
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    tracing::debug!("nat_gateway::{}", stringify!(delete_nat_gateway));
    if let Err(e) = state.store.delete_entity(GATEWAY_STORE_KEY, &id) {
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": e.to_string() })),
        )
            .into_response();
    }

    if let Err(e) = reconcile_nat(&state).await {
        tracing::warn!("Post-delete NAT gateway reconciliation failed: {}", e);
    }

    StatusCode::NO_CONTENT.into_response()
}

// ── Sync and status ─────────────────────────────────────────────────

pub async fn sync_nat_rules(
    RequireWrite(_claims): RequireWrite,
    State(state): State<Arc<AppState>>,
) -> impl IntoResponse {
    tracing::debug!("nat_gateway::{}", stringify!(sync_nat_rules));
    match reconcile_nat(&state).await {
        Ok(_) => (StatusCode::OK, Json(json!({ "status": "synced" }))).into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": e.to_string() })),
        )
            .into_response(),
    }
}

pub async fn get_nat_status(
    RequireRead(_claims): RequireRead,
    State(state): State<Arc<AppState>>,
) -> impl IntoResponse {
    tracing::debug!("nat_gateway::{}", stringify!(get_nat_status));
    let rules: Vec<NatRule> = match state.store.list_entities(RULE_STORE_KEY) {
        Ok(r) => r,
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": e.to_string() })),
            )
                .into_response();
        }
    };

    let vms = build_vm_snapshots(state.as_ref());
    let statuses: Vec<NatStatus> = rules
        .iter()
        .map(|rule| {
            let matching = vms
                .iter()
                .filter(|vm| rule.selector.matches(&vm.labels))
                .count();

            NatStatus {
                rule_id: rule.id,
                name: rule.name.clone(),
                rule_type: rule.rule_type.clone(),
                matching_vms: matching,
                enforced: rule.enabled,
            }
        })
        .collect();

    (StatusCode::OK, Json(statuses)).into_response()
}

// ── Reconciliation ──────────────────────────────────────────────────

pub async fn reconcile_nat(state: &AppState) -> anyhow::Result<()> {
    tracing::debug!("nat_gateway::{}", stringify!(reconcile_nat));
    let rules: Vec<NatRule> = state.store.list_entities(RULE_STORE_KEY)?;
    let gateways: Vec<NatGatewayConfig> = state.store.list_entities(GATEWAY_STORE_KEY)?;
    let pools: Vec<NatPool> = state.store.list_entities(POOL_STORE_KEY)?;

    let vms = build_vm_snapshots(state);

    let enabled_rules: Vec<NatRule> = rules.into_iter().filter(|r| r.enabled).collect();
    let enabled_gateways: Vec<NatGatewayConfig> =
        gateways.into_iter().filter(|g| g.enabled).collect();

    let compiled =
        state
            .nat_gateway
            .compiler
            .compile_all(&enabled_rules, &enabled_gateways, &pools, &vms);

    state.nat_gateway.enforcer.sync_all(&compiled)?;

    tracing::info!(
        "Reconciled {} NAT rules + {} gateways → {} nftables rules",
        enabled_rules.len(),
        enabled_gateways.len(),
        compiled.len()
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
