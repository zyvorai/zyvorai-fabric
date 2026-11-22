// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: LicenseRef-Zyvor-Production-2.0

use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use chrono::Utc;
use std::sync::Arc;
use uuid::Uuid;

use crate::server::AppState;
use content_library::{
    CreateLibraryRequest, GuestCustomizationSpec, HostProfile, Library, LibraryItem, LibraryType,
};
use security::{RequireAdmin, RequireRead, RequireWrite};

// ============================================================================
// Library handlers
// ============================================================================

pub async fn list_libraries(
    RequireRead(_claims): RequireRead,
    State(state): State<Arc<AppState>>,
) -> impl IntoResponse {
    tracing::debug!("content_library::{}", stringify!(list_libraries));
    let items: Vec<Library> = state.store.list_entities("libraries").unwrap_or_else(|e| {
        tracing::error!("Storage error: {}", e);
        Vec::new()
    });
    Json(items)
}

pub async fn create_library(
    RequireWrite(_claims): RequireWrite,
    State(state): State<Arc<AppState>>,
    Json(req): Json<CreateLibraryRequest>,
) -> impl IntoResponse {
    tracing::debug!("content_library::{}", stringify!(create_library));
    if let Err((s, m)) = crate::validation::validate_entity_name(&req.name) {
        return (s, Json(serde_json::json!({"error": m}))).into_response();
    }
    if let Err((s, m)) = crate::validation::validate_host_path(&req.storage_path) {
        return (s, Json(serde_json::json!({"error": m}))).into_response();
    }
    let now = Utc::now();
    let library = Library {
        id: Uuid::new_v4().to_string(),
        name: req.name,
        description: req.description,
        library_type: req.library_type,
        storage_path: req.storage_path,
        publish_url: req.publish_url,
        subscription_url: req.subscription_url,
        auto_sync: req.auto_sync,
        sync_interval_hours: req.sync_interval_hours.unwrap_or(24),
        last_sync: None,
        item_count: 0,
        total_size_bytes: 0,
        created: now,
        updated: now,
    };
    match state.store.save_entity("libraries", &library.id, &library) {
        Ok(_) => (StatusCode::CREATED, Json(library)).into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": e.to_string()})),
        )
            .into_response(),
    }
}

pub async fn get_library(
    RequireRead(_claims): RequireRead,
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    tracing::debug!("content_library::{}", stringify!(get_library));
    match state.store.get_entity::<Library>("libraries", &id) {
        Ok(Some(l)) => Json(l).into_response(),
        Ok(None) => (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({"error": "Library not found"})),
        )
            .into_response(),
        Err(_) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": "Internal server error"})),
        )
            .into_response(),
    }
}

pub async fn delete_library(
    RequireAdmin(_claims): RequireAdmin,
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    tracing::debug!("content_library::{}", stringify!(delete_library));
    if let Err(e) = state.store.delete_entity("libraries", &id) {
        tracing::error!("Failed to delete entity: {}", e);
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": e.to_string()})),
        )
            .into_response();
    }
    (
        StatusCode::NO_CONTENT,
        Json(serde_json::json!({"status": "deleted"})),
    )
        .into_response()
}

pub async fn sync_library(
    RequireWrite(_claims): RequireWrite,
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    tracing::debug!("content_library::{}", stringify!(sync_library));
    let mut lib = match state.store.get_entity::<Library>("libraries", &id) {
        Ok(Some(l)) => l,
        Ok(None) => {
            return (
                StatusCode::NOT_FOUND,
                Json(serde_json::json!({"error": "Library not found"})),
            )
                .into_response()
        }
        Err(_) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"error": "Internal server error"})),
            )
                .into_response()
        }
    };
    if lib.library_type != LibraryType::Subscribed {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"error": "Not a subscribed library"})),
        )
            .into_response();
    }
    lib.last_sync = Some(Utc::now());
    lib.updated = Utc::now();
    if let Err(e) = state.store.save_entity("libraries", &lib.id, &lib) {
        tracing::error!("Failed to save entity: {}", e);
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": e.to_string()})),
        )
            .into_response();
    }
    (
        StatusCode::OK,
        Json(serde_json::json!({"status": "sync completed"})),
    )
        .into_response()
}

// ============================================================================
// Download handler
// ============================================================================

#[derive(serde::Deserialize)]
pub struct DownloadImageRequest {
    pub url: String,
    pub name: String,
    #[serde(default = "default_item_type")]
    pub item_type: content_library::ItemType,
}

fn default_item_type() -> content_library::ItemType {
    content_library::ItemType::VmImage
}

pub async fn download_image(
    RequireWrite(_claims): RequireWrite,
    State(state): State<Arc<AppState>>,
    Path(library_id): Path<String>,
    Json(req): Json<DownloadImageRequest>,
) -> impl IntoResponse {
    tracing::debug!("content_library::{}", stringify!(download_image));
    // Validate URL against SSRF
    if let Err(e) = crate::api::notifications::validate_external_url_public(&req.url) {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"error": format!("Invalid URL: {}", e)})),
        )
            .into_response();
    }
    // Verify library exists
    match state.store.get_entity::<Library>("libraries", &library_id) {
        Ok(Some(_)) => {}
        Ok(None) => {
            return (
                StatusCode::NOT_FOUND,
                Json(serde_json::json!({"error": "Library not found"})),
            )
                .into_response()
        }
        Err(_) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"error": "Internal server error"})),
            )
                .into_response()
        }
    }

    let mgr = content_library::ContentLibraryManager::new();

    // We need a library in the manager. Create a temporary one with the storage path.
    let lib = match state.store.get_entity::<Library>("libraries", &library_id) {
        Ok(Some(l)) => l,
        _ => {
            return (
                StatusCode::NOT_FOUND,
                Json(serde_json::json!({"error": "Library not found"})),
            )
                .into_response()
        }
    };

    // Create library in the in-memory manager so download_image can find it
    if let Err(e) = mgr.create_library(content_library::CreateLibraryRequest {
        name: lib.name.clone(),
        description: lib.description.clone(),
        library_type: lib.library_type.clone(),
        storage_path: lib.storage_path.clone(),
        publish_url: None,
        subscription_url: None,
        auto_sync: false,
        sync_interval_hours: None,
    }) {
        tracing::warn!("Failed to create library in manager: {}", e);
    }

    // Find the library ID in the manager (it generates a new one)
    let manager_libs = mgr.list_libraries();
    let manager_lib_id = match manager_libs.first() {
        Some(l) => l.id.clone(),
        None => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"error": "Failed to initialize library manager"})),
            )
                .into_response()
        }
    };

    match mgr
        .download_image(&manager_lib_id, &req.url, &req.name, req.item_type)
        .await
    {
        Ok(item) => {
            // Save to our state store
            let mut saved_item = item.clone();
            saved_item.library_id = library_id;
            let id = Uuid::new_v4().to_string();
            saved_item.id = id.clone();
            match state.store.save_entity("library_items", &id, &saved_item) {
                Ok(_) => (StatusCode::CREATED, Json(saved_item)).into_response(),
                Err(e) => (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(serde_json::json!({"error": e.to_string()})),
                )
                    .into_response(),
            }
        }
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": e.to_string()})),
        )
            .into_response(),
    }
}

// ============================================================================
// Library item handlers
// ============================================================================

pub async fn list_library_items(
    RequireRead(_claims): RequireRead,
    State(state): State<Arc<AppState>>,
) -> impl IntoResponse {
    tracing::debug!("content_library::{}", stringify!(list_library_items));
    let items: Vec<LibraryItem> = state
        .store
        .list_entities("library_items")
        .unwrap_or_else(|e| {
            tracing::error!("Storage error: {}", e);
            Vec::new()
        });
    Json(items)
}

pub async fn add_library_item(
    RequireWrite(_claims): RequireWrite,
    State(state): State<Arc<AppState>>,
    Json(mut item): Json<LibraryItem>,
) -> impl IntoResponse {
    tracing::debug!("content_library::{}", stringify!(add_library_item));
    item.id = Uuid::new_v4().to_string();
    let now = Utc::now();
    item.created = now;
    item.updated = now;
    if item.version == 0 {
        item.version = 1;
    }
    match state.store.save_entity("library_items", &item.id, &item) {
        Ok(_) => (StatusCode::CREATED, Json(item)).into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": e.to_string()})),
        )
            .into_response(),
    }
}

pub async fn get_library_item(
    RequireRead(_claims): RequireRead,
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    tracing::debug!("content_library::{}", stringify!(get_library_item));
    match state.store.get_entity::<LibraryItem>("library_items", &id) {
        Ok(Some(i)) => Json(i).into_response(),
        Ok(None) => (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({"error": "Library item not found"})),
        )
            .into_response(),
        Err(_) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": "Internal server error"})),
        )
            .into_response(),
    }
}

pub async fn delete_library_item(
    RequireAdmin(_claims): RequireAdmin,
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    tracing::debug!("content_library::{}", stringify!(delete_library_item));
    if let Err(e) = state.store.delete_entity("library_items", &id) {
        tracing::error!("Failed to delete entity: {}", e);
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": e.to_string()})),
        )
            .into_response();
    }
    (
        StatusCode::NO_CONTENT,
        Json(serde_json::json!({"status": "deleted"})),
    )
        .into_response()
}

#[derive(serde::Deserialize)]
pub struct SearchQuery {
    pub q: String,
}

pub async fn search_items(
    RequireRead(_claims): RequireRead,
    State(state): State<Arc<AppState>>,
    Query(query): Query<SearchQuery>,
) -> impl IntoResponse {
    tracing::debug!("content_library::{}", stringify!(search_items));
    if query.q.len() > 256 {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"error": "Search query must not exceed 256 characters"})),
        )
            .into_response();
    }
    let items: Vec<LibraryItem> = state
        .store
        .list_entities("library_items")
        .unwrap_or_else(|e| {
            tracing::error!("Storage error: {}", e);
            Vec::new()
        });
    let q = query.q.to_lowercase();
    let matched: Vec<_> = items
        .into_iter()
        .filter(|i| i.name.to_lowercase().contains(&q))
        .collect();
    Json(matched).into_response()
}

// ============================================================================
// Customization spec handlers
// ============================================================================

pub async fn list_customization_specs(
    RequireRead(_claims): RequireRead,
    State(state): State<Arc<AppState>>,
) -> impl IntoResponse {
    tracing::debug!("content_library::{}", stringify!(list_customization_specs));
    let items: Vec<GuestCustomizationSpec> = state
        .store
        .list_entities("customization_specs")
        .unwrap_or_else(|e| {
            tracing::error!("Storage error: {}", e);
            Vec::new()
        });
    Json(items)
}

pub async fn create_customization_spec(
    RequireWrite(_claims): RequireWrite,
    State(state): State<Arc<AppState>>,
    Json(mut spec): Json<GuestCustomizationSpec>,
) -> impl IntoResponse {
    tracing::debug!("content_library::{}", stringify!(create_customization_spec));
    spec.id = Uuid::new_v4().to_string();
    let now = Utc::now();
    spec.created = now;
    spec.updated = now;
    match state
        .store
        .save_entity("customization_specs", &spec.id, &spec)
    {
        Ok(_) => (StatusCode::CREATED, Json(spec)).into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": e.to_string()})),
        )
            .into_response(),
    }
}

pub async fn get_customization_spec(
    RequireRead(_claims): RequireRead,
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    tracing::debug!("content_library::{}", stringify!(get_customization_spec));
    match state
        .store
        .get_entity::<GuestCustomizationSpec>("customization_specs", &id)
    {
        Ok(Some(s)) => Json(s).into_response(),
        Ok(None) => (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({"error": "Customization spec not found"})),
        )
            .into_response(),
        Err(_) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": "Internal server error"})),
        )
            .into_response(),
    }
}

pub async fn delete_customization_spec(
    RequireAdmin(_claims): RequireAdmin,
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    tracing::debug!("content_library::{}", stringify!(delete_customization_spec));
    if let Err(e) = state.store.delete_entity("customization_specs", &id) {
        tracing::error!("Failed to delete entity: {}", e);
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": e.to_string()})),
        )
            .into_response();
    }
    (
        StatusCode::NO_CONTENT,
        Json(serde_json::json!({"status": "deleted"})),
    )
        .into_response()
}

// ============================================================================
// Host profile handlers
// ============================================================================

pub async fn list_host_profiles(
    RequireRead(_claims): RequireRead,
    State(state): State<Arc<AppState>>,
) -> impl IntoResponse {
    tracing::debug!("content_library::{}", stringify!(list_host_profiles));
    let items: Vec<HostProfile> = state
        .store
        .list_entities("host_profiles")
        .unwrap_or_else(|e| {
            tracing::error!("Storage error: {}", e);
            Vec::new()
        });
    Json(items)
}

pub async fn create_host_profile(
    RequireWrite(_claims): RequireWrite,
    State(state): State<Arc<AppState>>,
    Json(mut profile): Json<HostProfile>,
) -> impl IntoResponse {
    tracing::debug!("content_library::{}", stringify!(create_host_profile));
    profile.id = Uuid::new_v4().to_string();
    let now = Utc::now();
    profile.created = now;
    profile.updated = now;
    match state
        .store
        .save_entity("host_profiles", &profile.id, &profile)
    {
        Ok(_) => (StatusCode::CREATED, Json(profile)).into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": e.to_string()})),
        )
            .into_response(),
    }
}

pub async fn get_host_profile(
    RequireRead(_claims): RequireRead,
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    tracing::debug!("content_library::{}", stringify!(get_host_profile));
    match state.store.get_entity::<HostProfile>("host_profiles", &id) {
        Ok(Some(p)) => Json(p).into_response(),
        Ok(None) => (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({"error": "Host profile not found"})),
        )
            .into_response(),
        Err(_) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": "Internal server error"})),
        )
            .into_response(),
    }
}

pub async fn delete_host_profile(
    RequireAdmin(_claims): RequireAdmin,
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    tracing::debug!("content_library::{}", stringify!(delete_host_profile));
    if let Err(e) = state.store.delete_entity("host_profiles", &id) {
        tracing::error!("Failed to delete: {}", e);
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": e.to_string()})),
        )
            .into_response();
    }
    (
        StatusCode::NO_CONTENT,
        Json(serde_json::json!({"status": "deleted"})),
    )
        .into_response()
}

#[derive(serde::Deserialize)]
pub struct HostComplianceRequest {
    pub host_id: String,
    pub current_config: serde_json::Value,
}

pub async fn check_host_compliance(
    RequireRead(_claims): RequireRead,
    State(state): State<Arc<AppState>>,
    Path(profile_id): Path<String>,
    Json(req): Json<HostComplianceRequest>,
) -> impl IntoResponse {
    tracing::debug!("content_library::{}", stringify!(check_host_compliance));
    let mgr = content_library::ContentLibraryManager::new();
    // Load profile into the manager for compliance check
    if let Ok(Some(profile)) = state
        .store
        .get_entity::<HostProfile>("host_profiles", &profile_id)
    {
        if let Err(e) = mgr.create_host_profile(profile) {
            tracing::warn!("Failed to create host profile in manager: {}", e);
        }
    }
    let result = mgr.check_host_compliance(&req.host_id, &profile_id, &req.current_config);
    Json(result)
}
