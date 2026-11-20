// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: LicenseRef-Zyvor-Production-1.0

use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::sync::Arc;

use crate::server::AppState;
use security::{RequireAdmin, RequireRead, RequireWrite};

// ============================================================================
// Data Structures
// ============================================================================

/// VM Profile / Instance Type (like AWS t3.large, m5.xlarge, etc.)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VMProfile {
    pub name: String,
    pub description: String,
    pub cpus: u32,
    pub memory: u64, // MB
    pub disk: u64,   // GB
    pub category: ProfileCategory,
    pub network_bandwidth: Option<String>,
    pub builtin: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ProfileCategory {
    General,
    Compute,
    Memory,
    Storage,
    Gpu,
}

#[derive(Debug, Deserialize)]
pub struct CreateProfileRequest {
    pub name: String,
    pub description: Option<String>,
    pub cpus: u32,
    pub memory: u64,
    pub disk: u64,
    pub category: Option<ProfileCategory>,
    pub network_bandwidth: Option<String>,
}

// ============================================================================
// Built-in profiles
// ============================================================================

fn builtin_profiles() -> Vec<VMProfile> {
    vec![
        // General Purpose
        VMProfile {
            name: "small".to_string(),
            description: "Small general purpose".to_string(),
            cpus: 1,
            memory: 1024,
            disk: 20,
            category: ProfileCategory::General,
            network_bandwidth: Some("1 Gbps".to_string()),
            builtin: true,
        },
        VMProfile {
            name: "medium".to_string(),
            description: "Medium general purpose".to_string(),
            cpus: 2,
            memory: 4096,
            disk: 40,
            category: ProfileCategory::General,
            network_bandwidth: Some("5 Gbps".to_string()),
            builtin: true,
        },
        VMProfile {
            name: "large".to_string(),
            description: "Large general purpose".to_string(),
            cpus: 4,
            memory: 8192,
            disk: 80,
            category: ProfileCategory::General,
            network_bandwidth: Some("10 Gbps".to_string()),
            builtin: true,
        },
        VMProfile {
            name: "xlarge".to_string(),
            description: "Extra large general purpose".to_string(),
            cpus: 8,
            memory: 16384,
            disk: 160,
            category: ProfileCategory::General,
            network_bandwidth: Some("10 Gbps".to_string()),
            builtin: true,
        },
        // Compute Optimized
        VMProfile {
            name: "c.large".to_string(),
            description: "Compute optimized large".to_string(),
            cpus: 8,
            memory: 8192,
            disk: 40,
            category: ProfileCategory::Compute,
            network_bandwidth: Some("10 Gbps".to_string()),
            builtin: true,
        },
        VMProfile {
            name: "c.xlarge".to_string(),
            description: "Compute optimized extra large".to_string(),
            cpus: 16,
            memory: 16384,
            disk: 80,
            category: ProfileCategory::Compute,
            network_bandwidth: Some("25 Gbps".to_string()),
            builtin: true,
        },
        // Memory Optimized
        VMProfile {
            name: "m.large".to_string(),
            description: "Memory optimized large".to_string(),
            cpus: 4,
            memory: 32768,
            disk: 80,
            category: ProfileCategory::Memory,
            network_bandwidth: Some("10 Gbps".to_string()),
            builtin: true,
        },
        VMProfile {
            name: "m.xlarge".to_string(),
            description: "Memory optimized extra large".to_string(),
            cpus: 8,
            memory: 65536,
            disk: 160,
            category: ProfileCategory::Memory,
            network_bandwidth: Some("25 Gbps".to_string()),
            builtin: true,
        },
        // Storage Optimized
        VMProfile {
            name: "s.large".to_string(),
            description: "Storage optimized large".to_string(),
            cpus: 4,
            memory: 8192,
            disk: 500,
            category: ProfileCategory::Storage,
            network_bandwidth: Some("10 Gbps".to_string()),
            builtin: true,
        },
    ]
}

// ============================================================================
// Handlers
// ============================================================================

/// GET /api/profiles - List all VM profiles (built-in + custom)
pub async fn list_profiles(
    RequireRead(_claims): RequireRead,
    State(state): State<Arc<AppState>>,
) -> Json<Vec<VMProfile>> {
    tracing::debug!("profiles::{}", stringify!(list_profiles));
    let mut profiles = builtin_profiles();

    // Add custom profiles from store
    if let Ok(custom) = state.store.list_entities::<VMProfile>("profiles") {
        profiles.extend(custom);
    }

    Json(profiles)
}

/// GET /api/profiles/:name - Get a specific profile
pub async fn get_profile(
    RequireRead(_claims): RequireRead,
    State(state): State<Arc<AppState>>,
    Path(name): Path<String>,
) -> Result<Json<VMProfile>, (StatusCode, Json<serde_json::Value>)> {
    tracing::debug!("profiles::{}", stringify!(get_profile));
    // Check built-in profiles first
    if let Some(profile) = builtin_profiles().into_iter().find(|p| p.name == name) {
        return Ok(Json(profile));
    }

    // Check custom profiles
    match state.store.get_entity::<VMProfile>("profiles", &name) {
        Ok(Some(profile)) => Ok(Json(profile)),
        Ok(None) => Err((
            StatusCode::NOT_FOUND,
            Json(json!({ "error": "Profile not found" })),
        )),
        Err(e) => Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": e.to_string() })),
        )),
    }
}

/// POST /api/profiles - Create a custom profile
pub async fn create_profile(
    RequireWrite(_claims): RequireWrite,
    State(state): State<Arc<AppState>>,
    Json(req): Json<CreateProfileRequest>,
) -> Result<(StatusCode, Json<VMProfile>), (StatusCode, Json<serde_json::Value>)> {
    tracing::debug!("profiles::{}", stringify!(create_profile));
    // Don't allow overriding built-in profiles
    if builtin_profiles().iter().any(|p| p.name == req.name) {
        return Err((
            StatusCode::CONFLICT,
            Json(json!({ "error": "Cannot override built-in profile" })),
        ));
    }

    let profile = VMProfile {
        name: req.name.clone(),
        description: req.description.unwrap_or_default(),
        cpus: req.cpus,
        memory: req.memory,
        disk: req.disk,
        category: req.category.unwrap_or(ProfileCategory::General),
        network_bandwidth: req.network_bandwidth,
        builtin: false,
    };

    state
        .store
        .save_entity("profiles", &profile.name, &profile)
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": e.to_string() })),
            )
        })?;

    Ok((StatusCode::CREATED, Json(profile)))
}

/// DELETE /api/profiles/:name - Delete a custom profile
pub async fn delete_profile(
    RequireAdmin(_claims): RequireAdmin,
    State(state): State<Arc<AppState>>,
    Path(name): Path<String>,
) -> Result<StatusCode, (StatusCode, Json<serde_json::Value>)> {
    tracing::debug!("profiles::{}", stringify!(delete_profile));
    if builtin_profiles().iter().any(|p| p.name == name) {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": "Cannot delete built-in profile" })),
        ));
    }

    state.store.delete_entity("profiles", &name).map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": e.to_string() })),
        )
    })?;

    Ok(StatusCode::NO_CONTENT)
}
