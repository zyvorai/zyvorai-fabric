// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: LicenseRef-Zyvor-Production-1.0

use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use security::RequireAdmin;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;

use crate::server::AppState;

#[derive(Debug, Deserialize)]
pub struct CreateSecretRequest {
    pub name: String,
    pub value: String,
    #[serde(default)]
    pub metadata: HashMap<String, String>,
}

#[derive(Debug, Serialize)]
pub struct SecretResponse {
    pub id: String,
    pub name: String,
    pub created: String,
    pub updated: Option<String>,
    pub metadata: HashMap<String, String>,
}

#[derive(Debug, Deserialize)]
pub struct UpdateSecretRequest {
    pub value: String,
}

/// GET /api/secrets - List all secrets (values redacted).
pub async fn list_secrets(
    RequireAdmin(_claims): RequireAdmin,
    State(state): State<Arc<AppState>>,
) -> Result<impl IntoResponse, (StatusCode, Json<serde_json::Value>)> {
    let secrets = state.secrets_manager.list_secrets();

    let response: Vec<SecretResponse> = secrets
        .into_iter()
        .map(|s| SecretResponse {
            id: s.id,
            name: s.name,
            created: s.created.to_rfc3339(),
            updated: s.updated.map(|u| u.to_rfc3339()),
            metadata: s.metadata,
        })
        .collect();

    Ok(Json(response))
}

/// GET /api/secrets/:id - Get a secret by ID (value redacted).
pub async fn get_secret(
    RequireAdmin(_claims): RequireAdmin,
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<impl IntoResponse, (StatusCode, Json<serde_json::Value>)> {
    let secret = state.secrets_manager.get_secret(&id).ok_or_else(|| {
        (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({"error": "Secret not found"})),
        )
    })?;

    // Return redacted view (no value)
    Ok(Json(SecretResponse {
        id: secret.id,
        name: secret.name,
        created: secret.created.to_rfc3339(),
        updated: secret.updated.map(|u| u.to_rfc3339()),
        metadata: secret.metadata,
    }))
}

/// POST /api/secrets - Create a new secret.
pub async fn create_secret(
    RequireAdmin(_claims): RequireAdmin,
    State(state): State<Arc<AppState>>,
    Json(req): Json<CreateSecretRequest>,
) -> Result<impl IntoResponse, (StatusCode, Json<serde_json::Value>)> {
    // Validate secret name
    crate::validation::validate_entity_name(&req.name)
        .map_err(|(s, m)| (s, Json(serde_json::json!({"error": m}))))?;

    let metadata = if req.metadata.is_empty() {
        None
    } else {
        Some(req.metadata)
    };

    let secret = state
        .secrets_manager
        .create_secret(&req.name, &req.value, metadata)
        .map_err(|e| {
            (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({"error": e.to_string()})),
            )
        })?;

    Ok((
        StatusCode::CREATED,
        Json(SecretResponse {
            id: secret.id,
            name: secret.name,
            created: secret.created.to_rfc3339(),
            updated: secret.updated.map(|u| u.to_rfc3339()),
            metadata: secret.metadata,
        }),
    ))
}

/// DELETE /api/secrets/:id - Delete a secret.
pub async fn delete_secret(
    RequireAdmin(_claims): RequireAdmin,
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<impl IntoResponse, (StatusCode, Json<serde_json::Value>)> {
    if state.secrets_manager.delete_secret(&id) {
        Ok(Json(
            serde_json::json!({"message": "Secret deleted successfully"}),
        ))
    } else {
        Err((
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({"error": "Secret not found"})),
        ))
    }
}
