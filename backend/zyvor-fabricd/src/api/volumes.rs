// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: LicenseRef-Zyvor-Production-1.0

use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use uuid::Uuid;

use crate::server::AppState;
use security::{RequireAdmin, RequireRead, RequireWrite};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Volume {
    pub id: String,
    pub pool_name: String,
    pub name: String,
    pub size: String,
    pub vm_attached: Option<String>,
    pub created: String,
    pub updated: String,
}

#[derive(Debug, Deserialize)]
pub struct CreateVolumeRequest {
    pub name: String,
    pub size: String,
}

#[derive(Debug, Deserialize)]
pub struct ResizeVolumeRequest {
    pub size: String,
}

#[derive(Debug, Deserialize)]
pub struct AttachVolumeRequest {
    pub vm_name: String,
}

/// POST /api/storage/pools/:name/volumes - Create a volume in a pool
pub async fn create_volume(
    RequireWrite(_claims): RequireWrite,
    State(state): State<Arc<AppState>>,
    Path(pool_name): Path<String>,
    Json(req): Json<CreateVolumeRequest>,
) -> impl IntoResponse {
    tracing::debug!("volumes::{}", stringify!(create_volume));
    // Validate pool name to prevent path traversal in store subdirectory
    if let Err((s, m)) = crate::validation::validate_vm_name(&pool_name) {
        return crate::api_error::json_error(s, format!("Invalid pool name: {}", m))
            .into_response();
    }
    // Verify pool exists — scope the lock so it's released before save_entity
    {
        let manager = state.storage_manager.read().await;
        if manager.get_pool(&pool_name).await.is_err() {
            return crate::api_error::json_error(
                StatusCode::NOT_FOUND,
                format!("Pool '{}' not found", pool_name),
            )
            .into_response();
        }
    }

    let now = Utc::now().to_rfc3339();
    let volume = Volume {
        id: Uuid::new_v4().to_string(),
        pool_name: pool_name.clone(),
        name: req.name,
        size: req.size,
        vm_attached: None,
        created: now.clone(),
        updated: now,
    };

    let store_key = format!("volumes_{}", pool_name);
    match state.store.save_entity(&store_key, &volume.id, &volume) {
        Ok(_) => (StatusCode::CREATED, Json(volume)).into_response(),
        Err(e) => crate::api_error::json_error(StatusCode::INTERNAL_SERVER_ERROR, e.to_string())
            .into_response(),
    }
}

/// GET /api/storage/pools/:name/volumes - List volumes in a pool
pub async fn list_volumes(
    RequireRead(_claims): RequireRead,
    State(state): State<Arc<AppState>>,
    Path(pool_name): Path<String>,
) -> impl IntoResponse {
    tracing::debug!("volumes::{}", stringify!(list_volumes));
    if let Err((s, m)) = crate::validation::validate_vm_name(&pool_name) {
        return crate::api_error::json_error(s, format!("Invalid pool name: {}", m))
            .into_response();
    }
    let store_key = format!("volumes_{}", pool_name);
    let items: Vec<Volume> = state.store.list_entities(&store_key).unwrap_or_else(|e| {
        tracing::error!("Storage error loading {}: {}", store_key, e);
        Vec::new()
    });
    Json(items).into_response()
}

/// GET /api/storage/pools/:name/volumes/:id - Get a volume
pub async fn get_volume(
    RequireRead(_claims): RequireRead,
    State(state): State<Arc<AppState>>,
    Path((pool_name, id)): Path<(String, String)>,
) -> impl IntoResponse {
    tracing::debug!("volumes::{}", stringify!(get_volume));
    let store_key = format!("volumes_{}", pool_name);
    match state.store.get_entity::<Volume>(&store_key, &id) {
        Ok(Some(v)) => Json(v).into_response(),
        Ok(None) => {
            crate::api_error::json_error(StatusCode::NOT_FOUND, "Volume not found").into_response()
        }
        Err(_) => {
            crate::api_error::json_error(StatusCode::INTERNAL_SERVER_ERROR, "Failed to load volume")
                .into_response()
        }
    }
}

/// DELETE /api/storage/pools/:name/volumes/:id - Delete a volume
pub async fn delete_volume(
    RequireAdmin(_claims): RequireAdmin,
    State(state): State<Arc<AppState>>,
    Path((pool_name, id)): Path<(String, String)>,
) -> impl IntoResponse {
    tracing::debug!("volumes::{}", stringify!(delete_volume));
    let store_key = format!("volumes_{}", pool_name);
    if let Err(e) = state.store.delete_entity(&store_key, &id) {
        tracing::error!("Failed to delete volume: {}", e);
    }
    StatusCode::NO_CONTENT
}

/// POST /api/storage/pools/:name/volumes/:id/resize - Resize a volume
pub async fn resize_volume(
    RequireWrite(_claims): RequireWrite,
    State(state): State<Arc<AppState>>,
    Path((pool_name, id)): Path<(String, String)>,
    Json(req): Json<ResizeVolumeRequest>,
) -> impl IntoResponse {
    tracing::debug!("volumes::{}", stringify!(resize_volume));
    let store_key = format!("volumes_{}", pool_name);
    match state.store.get_entity::<Volume>(&store_key, &id) {
        Ok(Some(mut v)) => {
            v.size = req.size;
            v.updated = Utc::now().to_rfc3339();
            match state.store.save_entity(&store_key, &v.id, &v) {
                Ok(_) => Json(v).into_response(),
                Err(e) => {
                    crate::api_error::json_error(StatusCode::INTERNAL_SERVER_ERROR, e.to_string())
                        .into_response()
                }
            }
        }
        Ok(None) => {
            crate::api_error::json_error(StatusCode::NOT_FOUND, "Volume not found").into_response()
        }
        Err(_) => {
            crate::api_error::json_error(StatusCode::INTERNAL_SERVER_ERROR, "Failed to load volume")
                .into_response()
        }
    }
}

/// POST /api/storage/pools/:name/volumes/:id/attach - Attach volume to VM
pub async fn attach_volume(
    RequireWrite(_claims): RequireWrite,
    State(state): State<Arc<AppState>>,
    Path((pool_name, id)): Path<(String, String)>,
    Json(req): Json<AttachVolumeRequest>,
) -> impl IntoResponse {
    tracing::debug!("volumes::{}", stringify!(attach_volume));
    let store_key = format!("volumes_{}", pool_name);
    match state.store.get_entity::<Volume>(&store_key, &id) {
        Ok(Some(mut v)) => {
            if v.vm_attached.is_some() {
                return crate::api_error::json_error(
                    StatusCode::CONFLICT,
                    "Volume already attached to a VM",
                )
                .into_response();
            }
            v.vm_attached = Some(req.vm_name);
            v.updated = Utc::now().to_rfc3339();
            match state.store.save_entity(&store_key, &v.id, &v) {
                Ok(_) => Json(v).into_response(),
                Err(e) => {
                    crate::api_error::json_error(StatusCode::INTERNAL_SERVER_ERROR, e.to_string())
                        .into_response()
                }
            }
        }
        Ok(None) => {
            crate::api_error::json_error(StatusCode::NOT_FOUND, "Volume not found").into_response()
        }
        Err(_) => {
            crate::api_error::json_error(StatusCode::INTERNAL_SERVER_ERROR, "Failed to load volume")
                .into_response()
        }
    }
}

/// POST /api/storage/pools/:name/volumes/:id/detach - Detach volume from VM
pub async fn detach_volume(
    RequireWrite(_claims): RequireWrite,
    State(state): State<Arc<AppState>>,
    Path((pool_name, id)): Path<(String, String)>,
) -> impl IntoResponse {
    tracing::debug!("volumes::{}", stringify!(detach_volume));
    let store_key = format!("volumes_{}", pool_name);
    match state.store.get_entity::<Volume>(&store_key, &id) {
        Ok(Some(mut v)) => {
            v.vm_attached = None;
            v.updated = Utc::now().to_rfc3339();
            match state.store.save_entity(&store_key, &v.id, &v) {
                Ok(_) => Json(v).into_response(),
                Err(e) => {
                    crate::api_error::json_error(StatusCode::INTERNAL_SERVER_ERROR, e.to_string())
                        .into_response()
                }
            }
        }
        Ok(None) => {
            crate::api_error::json_error(StatusCode::NOT_FOUND, "Volume not found").into_response()
        }
        Err(_) => {
            crate::api_error::json_error(StatusCode::INTERNAL_SERVER_ERROR, "Failed to load volume")
                .into_response()
        }
    }
}
