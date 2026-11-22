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
use packet_mirror::compiler::VMSnapshot;
use packet_mirror::models::{CreateMirrorSessionRequest, MirrorSession, MirrorStatus};

use crate::server::AppState;

const STORE_KEY: &str = "mirror_sessions";

// ── Mirror Session CRUD ─────────────────────────────────────────────

pub async fn create_mirror_session(
    RequireWrite(_claims): RequireWrite,
    State(state): State<Arc<AppState>>,
    Json(req): Json<CreateMirrorSessionRequest>,
) -> impl IntoResponse {
    tracing::debug!("packet_mirror::{}", stringify!(create_mirror_session));
    if let Err((status, msg)) = crate::validation::validate_entity_name(&req.name) {
        return (status, Json(json!({"error": msg}))).into_response();
    }
    // Validate collector_target based on collector type
    match req.collector_type {
        packet_mirror::models::CollectorType::RemoteIp => {
            if let Err(e) = crate::validation::validate_ip_address(&req.collector_target) {
                return (
                    StatusCode::BAD_REQUEST,
                    Json(json!({"error": format!("Invalid collector_target IP: {}", e)})),
                )
                    .into_response();
            }
        }
        packet_mirror::models::CollectorType::Interface => {
            if let Err(msg) = crate::validation::validate_hostname(&req.collector_target) {
                return (
                    StatusCode::BAD_REQUEST,
                    Json(json!({"error": format!("Invalid collector_target interface: {}", msg)})),
                )
                    .into_response();
            }
        }
    }
    // Validate filter CIDRs if present
    if let Some(ref filter) = req.filter {
        if let Some(ref cidr) = filter.src_cidr {
            if let Err(e) = crate::validation::validate_cidr(cidr) {
                return (
                    StatusCode::BAD_REQUEST,
                    Json(json!({"error": format!("Invalid filter src_cidr: {}", e)})),
                )
                    .into_response();
            }
        }
        if let Some(ref cidr) = filter.dst_cidr {
            if let Err(e) = crate::validation::validate_cidr(cidr) {
                return (
                    StatusCode::BAD_REQUEST,
                    Json(json!({"error": format!("Invalid filter dst_cidr: {}", e)})),
                )
                    .into_response();
            }
        }
    }
    let now = Utc::now();
    let session = MirrorSession {
        id: Uuid::new_v4(),
        name: req.name,
        description: req.description,
        selector: req.selector,
        collector_type: req.collector_type,
        collector_target: req.collector_target,
        direction: req.direction,
        filter: req.filter,
        enabled: req.enabled,
        managed: true,
        created: now,
        updated: now,
    };

    if let Err(e) = state
        .store
        .save_entity(STORE_KEY, &session.id.to_string(), &session)
    {
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": e.to_string() })),
        )
            .into_response();
    }

    if let Err(e) = reconcile_mirrors(&state).await {
        tracing::warn!("Post-create mirror reconciliation failed: {}", e);
    }

    (StatusCode::CREATED, Json(session)).into_response()
}

pub async fn list_mirror_sessions(
    RequireRead(_claims): RequireRead,
    State(state): State<Arc<AppState>>,
) -> impl IntoResponse {
    tracing::debug!("packet_mirror::{}", stringify!(list_mirror_sessions));
    match state.store.list_entities::<MirrorSession>(STORE_KEY) {
        Ok(sessions) => {
            let merged = super::net_security_discover::merge_mirror_sessions(&state, sessions);
            (StatusCode::OK, Json(merged)).into_response()
        }
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": e.to_string() })),
        )
            .into_response(),
    }
}

pub async fn get_mirror_session(
    RequireRead(_claims): RequireRead,
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    tracing::debug!("packet_mirror::{}", stringify!(get_mirror_session));
    match state.store.get_entity::<MirrorSession>(STORE_KEY, &id) {
        Ok(Some(session)) => (StatusCode::OK, Json(session)).into_response(),
        Ok(None) => {
            if let Some(session) =
                super::net_security_discover::find_host_mirror_session(&state, &id)
            {
                return (StatusCode::OK, Json(session)).into_response();
            }
            (
                StatusCode::NOT_FOUND,
                Json(json!({ "error": "Mirror session not found" })),
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

pub async fn update_mirror_session(
    RequireWrite(_claims): RequireWrite,
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    Json(req): Json<CreateMirrorSessionRequest>,
) -> impl IntoResponse {
    tracing::debug!("packet_mirror::{}", stringify!(update_mirror_session));
    let existing = match state.store.get_entity::<MirrorSession>(STORE_KEY, &id) {
        Ok(Some(s)) => s,
        Ok(None) => {
            return (
                StatusCode::NOT_FOUND,
                Json(json!({ "error": "Mirror session not found" })),
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

    let session = MirrorSession {
        id: existing.id,
        name: req.name,
        description: req.description,
        selector: req.selector,
        collector_type: req.collector_type,
        collector_target: req.collector_target,
        direction: req.direction,
        filter: req.filter,
        enabled: req.enabled,
        managed: existing.managed,
        created: existing.created,
        updated: Utc::now(),
    };

    if let Err(e) = state.store.save_entity(STORE_KEY, &id, &session) {
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": e.to_string() })),
        )
            .into_response();
    }

    if let Err(e) = reconcile_mirrors(&state).await {
        tracing::warn!("Post-update mirror reconciliation failed: {}", e);
    }

    (StatusCode::OK, Json(session)).into_response()
}

pub async fn delete_mirror_session(
    RequireWrite(_claims): RequireWrite,
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    tracing::debug!("packet_mirror::{}", stringify!(delete_mirror_session));
    if let Some(host) = super::net_security_discover::find_host_mirror_session(&state, &id) {
        if super::net_security_discover::is_host_managed_mirror(&host) {
            return (
                StatusCode::BAD_REQUEST,
                Json(json!({
                    "error": "This mirror session exists on the host and is not managed by zyvor-fabricd"
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

    if let Err(e) = reconcile_mirrors(&state).await {
        tracing::warn!("Post-delete mirror reconciliation failed: {}", e);
    }

    StatusCode::NO_CONTENT.into_response()
}

pub async fn adopt_mirror_session(
    RequireWrite(_claims): RequireWrite,
    State(state): State<Arc<AppState>>,
    Json(req): Json<AdoptHostRequest>,
) -> impl IntoResponse {
    tracing::debug!("packet_mirror::{}", stringify!(adopt_mirror_session));
    let host = match super::net_security_discover::find_host_mirror_session(&state, &req.host_id) {
        Some(s) if super::net_security_discover::is_host_managed_mirror(&s) => s,
        _ => {
            return (
                StatusCode::NOT_FOUND,
                Json(json!({ "error": "Host mirror session not found" })),
            )
                .into_response();
        }
    };

    let stored: Vec<MirrorSession> = state.store.list_entities(STORE_KEY).unwrap_or_default();
    if stored.iter().any(|s| s.name == host.name) {
        return (
            StatusCode::CONFLICT,
            Json(json!({
                "error": format!("Mirror session '{}' is already managed by zyvor-fabricd", host.name)
            })),
        )
            .into_response();
    }

    let now = Utc::now();
    let session = MirrorSession {
        id: Uuid::new_v4(),
        name: host.name,
        description: host.description,
        selector: host.selector,
        collector_type: host.collector_type,
        collector_target: host.collector_target,
        direction: host.direction,
        filter: host.filter,
        enabled: false,
        managed: true,
        created: now,
        updated: now,
    };

    if let Err(e) = state
        .store
        .save_entity(STORE_KEY, &session.id.to_string(), &session)
    {
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": e.to_string() })),
        )
            .into_response();
    }

    (StatusCode::CREATED, Json(session)).into_response()
}

// ── Sync and status ─────────────────────────────────────────────────

pub async fn sync_mirror_sessions(
    RequireWrite(_claims): RequireWrite,
    State(state): State<Arc<AppState>>,
) -> impl IntoResponse {
    tracing::debug!("packet_mirror::{}", stringify!(sync_mirror_sessions));
    match reconcile_mirrors(&state).await {
        Ok(_) => (StatusCode::OK, Json(json!({ "status": "synced" }))).into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": e.to_string() })),
        )
            .into_response(),
    }
}

pub async fn get_mirror_status(
    RequireRead(_claims): RequireRead,
    State(state): State<Arc<AppState>>,
) -> impl IntoResponse {
    tracing::debug!("packet_mirror::{}", stringify!(get_mirror_status));
    let sessions: Vec<MirrorSession> = match state.store.list_entities(STORE_KEY) {
        Ok(s) => s,
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": e.to_string() })),
            )
                .into_response();
        }
    };

    let vms = build_vm_snapshots(state.as_ref());
    let statuses: Vec<MirrorStatus> = sessions
        .iter()
        .map(|session| {
            let rules = state.packet_mirror.compiler.compile_session(session, &vms);

            MirrorStatus {
                session_id: session.id,
                name: session.name.clone(),
                matching_vms: rules.len(),
                active_mirrors: rules.len(),
                enforced: session.enabled,
            }
        })
        .collect();

    (StatusCode::OK, Json(statuses)).into_response()
}

// ── Reconciliation ──────────────────────────────────────────────────

pub async fn reconcile_mirrors(state: &AppState) -> anyhow::Result<()> {
    tracing::debug!("packet_mirror::{}", stringify!(reconcile_mirrors));
    let sessions: Vec<MirrorSession> = state.store.list_entities(STORE_KEY)?;

    let vms = build_vm_snapshots(state);

    let enabled: Vec<MirrorSession> = sessions.into_iter().filter(|s| s.enabled).collect();
    let rules = state.packet_mirror.compiler.compile_all(&enabled, &vms);

    state.packet_mirror.enforcer.sync_all(&rules)?;

    tracing::info!(
        "Reconciled {} mirror sessions → {} rules",
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
        .map(|vm| {
            let tap = Some(format!("tap-{}", vm.name));
            VMSnapshot {
                name: vm.name,
                labels: vm.labels.clone().unwrap_or_default(),
                tap_interface: tap,
            }
        })
        .collect()
}
