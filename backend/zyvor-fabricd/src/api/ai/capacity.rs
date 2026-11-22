// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: LicenseRef-Zyvor-Production-2.0
//!
//! GPU / inference capacity summary and recent AI events (preview).

use axum::{extract::State, http::StatusCode, Json};
use security::RequireRead;
use std::sync::Arc;

use crate::server::AppState;

use super::types::{InferenceDeployment, InferenceEndpoint};
use super::{fluxvm_client, STORE_DEPLOYMENTS, STORE_ENDPOINTS};

/// GET /api/ai/capacity
pub async fn capacity(
    RequireRead(_claims): RequireRead,
    State(state): State<Arc<AppState>>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let client = fluxvm_client(&state)?;
    let inventory = client.list_host_gpus().await.unwrap_or_default();

    let deployments: Vec<InferenceDeployment> = state
        .store
        .list_entities(STORE_DEPLOYMENTS)
        .unwrap_or_default();
    let endpoints: Vec<InferenceEndpoint> = state
        .store
        .list_entities(STORE_ENDPOINTS)
        .unwrap_or_default();

    let mut allocated = 0u32;
    let mut ready_replicas = 0u32;
    let mut desired_replicas = 0u32;
    for dep in &deployments {
        desired_replicas = desired_replicas.saturating_add(dep.replicas);
        ready_replicas = ready_replicas
            .saturating_add(dep.status.replicas.iter().filter(|r| r.ready).count() as u32);
        allocated = allocated.saturating_add(
            dep.status
                .replicas
                .iter()
                .filter(|r| !r.bdf.is_empty() && !r.bdf.starts_with("dry-run"))
                .count() as u32,
        );
    }

    let total_gpus = inventory.len() as u32;
    let free_gpus = total_gpus.saturating_sub(allocated);
    let dry_run = std::env::var("FLUXVM_AI_DRY_RUN")
        .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
        .unwrap_or(false);

    Ok(Json(serde_json::json!({
        "gpus": {
            "total": total_gpus,
            "allocated": allocated,
            "free": free_gpus,
        },
        "deployments": deployments.len(),
        "endpoints": endpoints.len(),
        "replicas": {
            "desired": desired_replicas,
            "ready": ready_replicas,
        },
        "dry_run": dry_run,
        "site": std::env::var("FLUXVM_AI_SITE").ok(),
    })))
}

/// GET /api/ai/events — recent AI audit entries (best-effort).
pub async fn events(
    RequireRead(_claims): RequireRead,
    State(state): State<Arc<AppState>>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    #[derive(serde::Deserialize)]
    struct AuditRow {
        id: String,
        #[serde(default)]
        user_id: String,
        #[serde(default)]
        action: String,
        #[serde(default)]
        resource: String,
        #[serde(default)]
        status: String,
        #[serde(default)]
        timestamp: Option<chrono::DateTime<chrono::Utc>>,
    }

    let mut rows: Vec<AuditRow> = state.store.list_entities("audit_logs").unwrap_or_default();
    rows.retain(|r| r.resource.starts_with("ai/") || r.action == "INFER");
    rows.sort_by_key(|r| std::cmp::Reverse(r.timestamp));
    rows.truncate(50);

    let items: Vec<serde_json::Value> = rows
        .into_iter()
        .map(|r| {
            serde_json::json!({
                "id": r.id,
                "user": r.user_id,
                "action": r.action,
                "resource": r.resource,
                "status": r.status,
                "timestamp": r.timestamp,
            })
        })
        .collect();

    Ok(Json(serde_json::json!({ "items": items })))
}

/// Re-export helpers used by capacity unit tests.
pub fn free_gpus(total: u32, allocated: u32) -> u32 {
    total.saturating_sub(allocated)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn free_math() {
        assert_eq!(free_gpus(4, 1), 3);
        assert_eq!(free_gpus(1, 5), 0);
    }
}
