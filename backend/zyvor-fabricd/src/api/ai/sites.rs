// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: LicenseRef-Zyvor-Production-1.0

use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use chrono::Utc;
use security::{RequireRead, RequireWrite};
use std::sync::Arc;

use crate::server::AppState;

use super::federation::AiSite;
use super::{audit, err, STORE_SITES};

pub async fn list_sites(
    RequireRead(_claims): RequireRead,
    State(state): State<Arc<AppState>>,
) -> Result<Json<Vec<AiSite>>, (StatusCode, Json<serde_json::Value>)> {
    let mut sites: Vec<AiSite> = state
        .store
        .list_entities(STORE_SITES)
        .map_err(|e| err(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    sites.sort_by(|a, b| a.id.cmp(&b.id));
    Ok(Json(sites))
}

pub async fn get_site(
    RequireRead(_claims): RequireRead,
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<AiSite>, (StatusCode, Json<serde_json::Value>)> {
    state
        .store
        .get_entity(STORE_SITES, &id)
        .map_err(|e| err(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
        .ok_or_else(|| err(StatusCode::NOT_FOUND, "site not found"))
        .map(Json)
}

pub async fn put_site(
    RequireWrite(claims): RequireWrite,
    State(state): State<Arc<AppState>>,
    Json(mut site): Json<AiSite>,
) -> Result<Json<AiSite>, (StatusCode, Json<serde_json::Value>)> {
    crate::validation::validate_entity_name(&site.id).map_err(|(s, m)| err(s, m))?;
    if site.last_sync_unix == 0 {
        site.last_sync_unix = Utc::now().timestamp();
    }
    state
        .store
        .save_entity(STORE_SITES, &site.id, &site)
        .map_err(|e| err(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    audit(
        &state,
        &claims.sub,
        "UPSERT",
        &format!("ai/sites/{}", site.id),
        "SUCCESS",
    );
    Ok(Json(site))
}

/// DELETE /api/ai/sites/{id}
pub async fn delete_site(
    RequireWrite(claims): RequireWrite,
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<StatusCode, (StatusCode, Json<serde_json::Value>)> {
    if state
        .store
        .get_entity::<AiSite>(STORE_SITES, &id)
        .map_err(|e| err(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
        .is_none()
    {
        return Err(err(StatusCode::NOT_FOUND, "site not found"));
    }
    state
        .store
        .delete_entity(STORE_SITES, &id)
        .map_err(|e| err(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    audit(
        &state,
        &claims.sub,
        "DELETE",
        &format!("ai/sites/{id}"),
        "SUCCESS",
    );
    Ok(StatusCode::NO_CONTENT)
}
