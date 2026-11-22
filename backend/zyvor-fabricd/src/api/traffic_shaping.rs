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
use std::sync::Arc;
use uuid::Uuid;

use networking::models::AdoptHostRequest;
use traffic_shaping::classifier::VMSnapshot;
use traffic_shaping::models::{CreateQoSPolicyRequest, QoSPolicy, QoSStatus};

use crate::server::AppState;

const STORE_KEY: &str = "qos_policies";

// ── QoS Policy CRUD ─────────────────────────────────────────────────

pub async fn create_qos_policy(
    RequireWrite(_claims): RequireWrite,
    State(state): State<Arc<AppState>>,
    Json(req): Json<CreateQoSPolicyRequest>,
) -> impl IntoResponse {
    tracing::debug!("traffic_shaping::{}", stringify!(create_qos_policy));
    if let Err((status, msg)) = crate::validation::validate_entity_name(&req.name) {
        return (status, Json(json!({"error": msg}))).into_response();
    }
    if let Err(msg) = crate::validation::validate_hostname(&req.interface) {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({"error": format!("Invalid interface: {}", msg)})),
        )
            .into_response();
    }
    let now = Utc::now();
    let policy = QoSPolicy {
        id: Uuid::new_v4(),
        name: req.name,
        description: req.description,
        interface: req.interface,
        selector: req.selector,
        traffic_class: req.traffic_class,
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

    if let Err(e) = reconcile_qos(&state).await {
        tracing::warn!("Post-create QoS reconciliation failed: {}", e);
    }

    (StatusCode::CREATED, Json(policy)).into_response()
}

pub async fn list_qos_policies(
    RequireRead(_claims): RequireRead,
    State(state): State<Arc<AppState>>,
) -> impl IntoResponse {
    tracing::debug!("traffic_shaping::{}", stringify!(list_qos_policies));
    match state.store.list_entities::<QoSPolicy>(STORE_KEY) {
        Ok(policies) => {
            let merged = super::net_security_discover::merge_qos_policies(&state, policies);
            (StatusCode::OK, Json(merged)).into_response()
        }
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": e.to_string() })),
        )
            .into_response(),
    }
}

pub async fn get_qos_policy(
    RequireRead(_claims): RequireRead,
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    tracing::debug!("traffic_shaping::{}", stringify!(get_qos_policy));
    match state.store.get_entity::<QoSPolicy>(STORE_KEY, &id) {
        Ok(Some(policy)) => (StatusCode::OK, Json(policy)).into_response(),
        Ok(None) => {
            if let Some(host) = super::net_security_discover::find_host_qos_policy(&state, &id) {
                return (StatusCode::OK, Json(host)).into_response();
            }
            (
                StatusCode::NOT_FOUND,
                Json(json!({ "error": "QoS policy not found" })),
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

pub async fn update_qos_policy(
    RequireWrite(_claims): RequireWrite,
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    Json(req): Json<CreateQoSPolicyRequest>,
) -> impl IntoResponse {
    tracing::debug!("traffic_shaping::{}", stringify!(update_qos_policy));
    let existing = match state.store.get_entity::<QoSPolicy>(STORE_KEY, &id) {
        Ok(Some(p)) => p,
        Ok(None) => {
            return (
                StatusCode::NOT_FOUND,
                Json(json!({ "error": "QoS policy not found" })),
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

    let policy = QoSPolicy {
        id: existing.id,
        name: req.name,
        description: req.description,
        interface: req.interface,
        selector: req.selector,
        traffic_class: req.traffic_class,
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

    if let Err(e) = reconcile_qos(&state).await {
        tracing::warn!("Post-update QoS reconciliation failed: {}", e);
    }

    (StatusCode::OK, Json(policy)).into_response()
}

pub async fn delete_qos_policy(
    RequireWrite(_claims): RequireWrite,
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    tracing::debug!("traffic_shaping::{}", stringify!(delete_qos_policy));
    if let Some(host) = super::net_security_discover::find_host_qos_policy(&state, &id) {
        if super::net_security_discover::is_host_managed_qos(&host) {
            return (
                StatusCode::BAD_REQUEST,
                Json(json!({
                    "error": "This QoS policy reflects host traffic control and is not managed by zyvor-fabricd"
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

    if let Err(e) = reconcile_qos(&state).await {
        tracing::warn!("Post-delete QoS reconciliation failed: {}", e);
    }

    StatusCode::NO_CONTENT.into_response()
}

pub async fn adopt_qos_policy(
    RequireWrite(_claims): RequireWrite,
    State(state): State<Arc<AppState>>,
    Json(req): Json<AdoptHostRequest>,
) -> impl IntoResponse {
    tracing::debug!("traffic_shaping::{}", stringify!(adopt_qos_policy));
    let host = match super::net_security_discover::find_host_qos_policy(&state, &req.host_id) {
        Some(p) if super::net_security_discover::is_host_managed_qos(&p) => p,
        _ => {
            return (
                StatusCode::NOT_FOUND,
                Json(json!({ "error": "Host QoS policy not found" })),
            )
                .into_response();
        }
    };

    let stored: Vec<QoSPolicy> = state.store.list_entities(STORE_KEY).unwrap_or_default();
    if stored.iter().any(|p| p.name == host.name) {
        return (
            StatusCode::CONFLICT,
            Json(json!({
                "error": format!("QoS policy '{}' is already managed by zyvor-fabricd", host.name)
            })),
        )
            .into_response();
    }

    let now = Utc::now();
    let policy = QoSPolicy {
        id: Uuid::new_v4(),
        name: host.name,
        description: host.description,
        interface: host.interface,
        selector: host.selector,
        traffic_class: host.traffic_class,
        enabled: host.enabled,
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

    if let Err(e) = reconcile_qos(&state).await {
        tracing::warn!("Post-adopt QoS reconciliation failed: {}", e);
    }

    (StatusCode::CREATED, Json(policy)).into_response()
}

// ── Sync and status ─────────────────────────────────────────────────

pub async fn sync_qos_policies(
    RequireWrite(_claims): RequireWrite,
    State(state): State<Arc<AppState>>,
) -> impl IntoResponse {
    tracing::debug!("traffic_shaping::{}", stringify!(sync_qos_policies));
    match reconcile_qos(&state).await {
        Ok(_) => (StatusCode::OK, Json(json!({ "status": "synced" }))).into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": e.to_string() })),
        )
            .into_response(),
    }
}

pub async fn get_qos_status(
    RequireRead(_claims): RequireRead,
    State(state): State<Arc<AppState>>,
) -> impl IntoResponse {
    tracing::debug!("traffic_shaping::{}", stringify!(get_qos_status));
    let policies: Vec<QoSPolicy> = match state.store.list_entities(STORE_KEY) {
        Ok(p) => p,
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": e.to_string() })),
            )
                .into_response();
        }
    };

    let vms = build_vm_snapshots(state.as_ref());
    let statuses: Vec<QoSStatus> = policies
        .iter()
        .map(|policy| {
            let matching = vms
                .iter()
                .filter(|vm| policy.selector.matches(&vm.labels))
                .count();

            QoSStatus {
                policy_id: policy.id,
                name: policy.name.clone(),
                matching_vms: matching,
                enforced: policy.enabled,
            }
        })
        .collect();

    (StatusCode::OK, Json(statuses)).into_response()
}

// ── Reconciliation ──────────────────────────────────────────────────

pub async fn reconcile_qos(state: &AppState) -> anyhow::Result<()> {
    tracing::debug!("traffic_shaping::{}", stringify!(reconcile_qos));
    let policies: Vec<QoSPolicy> = state.store.list_entities(STORE_KEY)?;

    let vms = build_vm_snapshots(state);

    let enabled: Vec<QoSPolicy> = policies.into_iter().filter(|p| p.enabled).collect();
    let rules = state.traffic_shaper.classifier.classify_all(&enabled, &vms);

    state.traffic_shaper.enforcer.sync_all(&rules)?;

    tracing::info!(
        "Reconciled {} QoS policies → {} rules",
        enabled.len(),
        rules.len()
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
