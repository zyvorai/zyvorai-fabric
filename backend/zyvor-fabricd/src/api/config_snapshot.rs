// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: LicenseRef-Zyvor-Production-2.0

// Infrastructure Time Machine foundation — versioned config export.

use axum::{extract::State, response::IntoResponse, Json};
use chrono::Utc;
use serde::Serialize;
use serde_json::json;
use std::sync::Arc;

use crate::server::AppState;
use network_policy::models::NetworkPolicy;
use security::RequireRead;

#[derive(Debug, Serialize)]
pub struct ConfigSnapshot {
    pub version: String,
    pub exported_at: String,
    pub vms: serde_json::Value,
    pub network_policies: Vec<NetworkPolicy>,
    pub storage_pools: serde_json::Value,
    pub recent_events_count: usize,
}

/// GET /api/config/snapshot — point-in-time export for Time Machine / incident analysis.
pub async fn export_config_snapshot(
    RequireRead(_claims): RequireRead,
    State(state): State<Arc<AppState>>,
) -> impl IntoResponse {
    Json(build_snapshot(&state).await)
}

/// The current snapshot as JSON (same document  returns).
pub async fn collect_snapshot(state: &AppState) -> serde_json::Value {
    json!(build_snapshot(state).await)
}

async fn build_snapshot(state: &AppState) -> ConfigSnapshot {
    let vms = state.store.list_vms().unwrap_or_default();
    let network_policies: Vec<NetworkPolicy> = state
        .store
        .list_entities("network_policies")
        .unwrap_or_default();
    let storage_pools = {
        let manager = state.storage_manager.read().await;
        manager.list_pools().await
    };
    let events: Vec<crate::api::events::VMEvent> =
        state.store.list_entities("vm_events").unwrap_or_default();

    ConfigSnapshot {
        version: "1".into(),
        exported_at: Utc::now().to_rfc3339(),
        vms: json!(vms),
        network_policies,
        storage_pools: json!(storage_pools),
        recent_events_count: events.len(),
    }
}
