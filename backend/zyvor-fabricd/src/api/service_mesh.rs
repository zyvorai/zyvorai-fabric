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
use service_mesh::compiler::VMSnapshot;
use service_mesh::models::{BackendHealth, CreateServiceRequest, Service, ServiceStatus};

use crate::server::AppState;

const STORE_KEY: &str = "services";

// ── Service CRUD ────────────────────────────────────────────────────

pub async fn create_service(
    RequireWrite(_claims): RequireWrite,
    State(state): State<Arc<AppState>>,
    Json(req): Json<CreateServiceRequest>,
) -> impl IntoResponse {
    tracing::debug!("service_mesh::{}", stringify!(create_service));
    if let Err((status, msg)) = crate::validation::validate_entity_name(&req.name) {
        return (status, Json(json!({"error": msg}))).into_response();
    }
    if let Err(e) = crate::validation::validate_ip_address(&req.virtual_ip) {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({"error": format!("Invalid virtual_ip: {}", e)})),
        )
            .into_response();
    }
    let now = Utc::now();
    let service = Service {
        id: Uuid::new_v4(),
        name: req.name,
        description: req.description,
        virtual_ip: req.virtual_ip,
        selector: req.selector,
        ports: req.ports,
        algorithm: req.algorithm,
        health_check: req.health_check,
        enabled: req.enabled,
        managed: true,
        created: now,
        updated: now,
    };

    if let Err(e) = state
        .store
        .save_entity(STORE_KEY, &service.id.to_string(), &service)
    {
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": e.to_string() })),
        )
            .into_response();
    }

    if let Err(e) = reconcile_services(&state).await {
        tracing::warn!("Post-create reconciliation failed: {}", e);
    }

    (StatusCode::CREATED, Json(service)).into_response()
}

pub async fn list_services(
    RequireRead(_claims): RequireRead,
    State(state): State<Arc<AppState>>,
) -> impl IntoResponse {
    tracing::debug!("service_mesh::{}", stringify!(list_services));
    match state.store.list_entities::<Service>(STORE_KEY) {
        Ok(services) => {
            let merged = super::net_security_discover::merge_services(&state, services);
            (StatusCode::OK, Json(merged)).into_response()
        }
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": e.to_string() })),
        )
            .into_response(),
    }
}

pub async fn get_service(
    RequireRead(_claims): RequireRead,
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    tracing::debug!("service_mesh::{}", stringify!(get_service));
    match state.store.get_entity::<Service>(STORE_KEY, &id) {
        Ok(Some(service)) => (StatusCode::OK, Json(service)).into_response(),
        Ok(None) => {
            if let Some(host) = super::net_security_discover::find_host_service(&state, &id) {
                return (StatusCode::OK, Json(host)).into_response();
            }
            (
                StatusCode::NOT_FOUND,
                Json(json!({ "error": "Service not found" })),
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

pub async fn update_service(
    RequireWrite(_claims): RequireWrite,
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    Json(req): Json<CreateServiceRequest>,
) -> impl IntoResponse {
    tracing::debug!("service_mesh::{}", stringify!(update_service));
    let existing = match state.store.get_entity::<Service>(STORE_KEY, &id) {
        Ok(Some(s)) => s,
        Ok(None) => {
            return (
                StatusCode::NOT_FOUND,
                Json(json!({ "error": "Service not found" })),
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

    let service = Service {
        id: existing.id,
        name: req.name,
        description: req.description,
        virtual_ip: req.virtual_ip,
        selector: req.selector,
        ports: req.ports,
        algorithm: req.algorithm,
        health_check: req.health_check,
        enabled: req.enabled,
        managed: existing.managed,
        created: existing.created,
        updated: Utc::now(),
    };

    if let Err(e) = state.store.save_entity(STORE_KEY, &id, &service) {
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": e.to_string() })),
        )
            .into_response();
    }

    if let Err(e) = reconcile_services(&state).await {
        tracing::warn!("Post-update reconciliation failed: {}", e);
    }

    (StatusCode::OK, Json(service)).into_response()
}

pub async fn delete_service(
    RequireWrite(_claims): RequireWrite,
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    tracing::debug!("service_mesh::{}", stringify!(delete_service));
    if let Some(host) = super::net_security_discover::find_host_service(&state, &id) {
        if super::net_security_discover::is_host_managed_service(&host) {
            return (
                StatusCode::BAD_REQUEST,
                Json(json!({
                    "error": "This service is a host listener and is not managed by zyvor-fabricd"
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

    if let Err(e) = reconcile_services(&state).await {
        tracing::warn!("Post-delete reconciliation failed: {}", e);
    }

    StatusCode::NO_CONTENT.into_response()
}

pub async fn adopt_service(
    RequireWrite(_claims): RequireWrite,
    State(state): State<Arc<AppState>>,
    Json(req): Json<AdoptHostRequest>,
) -> impl IntoResponse {
    tracing::debug!("service_mesh::{}", stringify!(adopt_service));
    let host = match super::net_security_discover::find_host_service(&state, &req.host_id) {
        Some(s) if super::net_security_discover::is_host_managed_service(&s) => s,
        _ => {
            return (
                StatusCode::NOT_FOUND,
                Json(json!({ "error": "Host listener not found" })),
            )
                .into_response();
        }
    };

    let stored: Vec<Service> = state.store.list_entities(STORE_KEY).unwrap_or_default();
    if stored.iter().any(|s| s.name == host.name) {
        return (
            StatusCode::CONFLICT,
            Json(json!({
                "error": format!("Service '{}' is already managed by zyvor-fabricd", host.name)
            })),
        )
            .into_response();
    }

    let now = Utc::now();
    let service = Service {
        id: Uuid::new_v4(),
        name: host.name,
        description: host.description,
        virtual_ip: host.virtual_ip,
        selector: host.selector,
        ports: host.ports,
        algorithm: host.algorithm,
        health_check: host.health_check,
        enabled: host.enabled,
        managed: true,
        created: now,
        updated: now,
    };

    if let Err(e) = state
        .store
        .save_entity(STORE_KEY, &service.id.to_string(), &service)
    {
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": e.to_string() })),
        )
            .into_response();
    }

    if let Err(e) = reconcile_services(&state).await {
        tracing::warn!("Post-adopt service reconciliation failed: {}", e);
    }

    (StatusCode::CREATED, Json(service)).into_response()
}

// ── Backend health ──────────────────────────────────────────────────

pub async fn get_service_backends(
    RequireRead(_claims): RequireRead,
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    tracing::debug!("service_mesh::{}", stringify!(get_service_backends));
    let service = match state.store.get_entity::<Service>(STORE_KEY, &id) {
        Ok(Some(s)) => s,
        Ok(None) => {
            return (
                StatusCode::NOT_FOUND,
                Json(json!({ "error": "Service not found" })),
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

    let backends = state
        .service_mesh
        .compiler
        .health_checker()
        .get_all_backends(&service.name)
        .await;

    (StatusCode::OK, Json(backends)).into_response()
}

// ── Sync and status ─────────────────────────────────────────────────

pub async fn sync_services(
    RequireWrite(_claims): RequireWrite,
    State(state): State<Arc<AppState>>,
) -> impl IntoResponse {
    tracing::debug!("service_mesh::{}", stringify!(sync_services));
    match reconcile_services(&state).await {
        Ok(_) => (StatusCode::OK, Json(json!({ "status": "synced" }))).into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": e.to_string() })),
        )
            .into_response(),
    }
}

pub async fn get_service_status(
    RequireRead(_claims): RequireRead,
    State(state): State<Arc<AppState>>,
) -> impl IntoResponse {
    tracing::debug!("service_mesh::{}", stringify!(get_service_status));
    let services: Vec<Service> = match state.store.list_entities(STORE_KEY) {
        Ok(s) => s,
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": e.to_string() })),
            )
                .into_response();
        }
    };

    let mut statuses = Vec::new();
    for service in &services {
        let all_backends = state
            .service_mesh
            .compiler
            .health_checker()
            .get_all_backends(&service.name)
            .await;
        let healthy_backends = all_backends
            .iter()
            .filter(|b| b.health == BackendHealth::Healthy)
            .count();

        statuses.push(ServiceStatus {
            service_id: service.id,
            name: service.name.clone(),
            healthy_backends,
            total_backends: all_backends.len(),
            active: service.enabled,
        });
    }

    (StatusCode::OK, Json(statuses)).into_response()
}

// ── Reconciliation ──────────────────────────────────────────────────

pub async fn reconcile_services(state: &AppState) -> anyhow::Result<()> {
    tracing::debug!("service_mesh::{}", stringify!(reconcile_services));
    let services: Vec<Service> = state.store.list_entities(STORE_KEY)?;

    let vms = build_vm_snapshots(state);

    let enabled: Vec<Service> = services.into_iter().filter(|s| s.enabled).collect();
    let rules = state
        .service_mesh
        .compiler
        .compile_all(&enabled, &vms)
        .await;

    state.service_mesh.enforcer.sync_rules(&rules)?;

    tracing::info!(
        "Reconciled {} services → {} DNAT rules",
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
