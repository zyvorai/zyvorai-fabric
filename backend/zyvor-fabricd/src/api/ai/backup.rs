// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: LicenseRef-Zyvor-Production-2.0

//! Desired-state backup. Model bytes stay in the content-addressed cache.

use axum::{extract::State, http::StatusCode, Json};
use security::RequireWrite;
use serde_json::{json, Value};
use std::sync::Arc;

use crate::server::AppState;

use super::{
    err, STORE_DEPLOYMENTS, STORE_EDGE, STORE_ENDPOINTS, STORE_MODELS, STORE_MODEL_JOBS,
    STORE_NODES, STORE_POLICIES, STORE_PROFILES, STORE_REPLICA_SETS, STORE_REVISIONS,
    STORE_ROLLOUTS, STORE_SITES,
};

fn stores() -> &'static [&'static str] {
    &[
        STORE_MODELS,
        STORE_PROFILES,
        STORE_DEPLOYMENTS,
        STORE_ENDPOINTS,
        STORE_REVISIONS,
        STORE_REPLICA_SETS,
        STORE_ROLLOUTS,
        STORE_MODEL_JOBS,
        STORE_NODES,
        STORE_SITES,
        STORE_POLICIES,
        STORE_EDGE,
    ]
}

fn entity_id(value: &Value) -> Option<&str> {
    value
        .get("id")
        .or_else(|| value.get("name"))
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty() && !s.contains('/') && !s.contains(".."))
}

/// GET is not used: export is POST so it is an audited write.
pub async fn export_state(
    RequireWrite(_claims): RequireWrite,
    State(state): State<Arc<AppState>>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let mut doc = serde_json::Map::new();
    for store in stores() {
        let rows: Vec<Value> = state.store.list_entities(store).unwrap_or_default();
        doc.insert((*store).to_string(), json!(rows));
    }
    Ok(Json(Value::Object(doc)))
}

pub async fn restore_state(
    RequireWrite(_claims): RequireWrite,
    State(state): State<Arc<AppState>>,
    Json(doc): Json<Value>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let obj = doc
        .as_object()
        .ok_or_else(|| err(StatusCode::BAD_REQUEST, "backup must be a JSON object"))?;
    let mut restored = 0u64;
    for store in stores() {
        let Some(rows) = obj.get(*store).and_then(|v| v.as_array()) else {
            continue;
        };
        for row in rows {
            let Some(id) = entity_id(row) else {
                continue;
            };
            state
                .store
                .save_entity(store, id, row)
                .map_err(|e| err(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
            restored = restored.saturating_add(1);
        }
    }
    Ok(Json(json!({ "restored": restored })))
}
