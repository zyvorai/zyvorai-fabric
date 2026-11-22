// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: LicenseRef-Zyvor-Production-2.0

use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::sync::Arc;

use crate::server::AppState;
use security::{RequireAdmin, RequireRead, RequireWrite};

// ============================================================================
// Availability Zones
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AvailabilityZone {
    pub id: String,
    pub name: String,
    pub description: String,
    pub region: String,
    pub status: ZoneStatus,
    pub hosts: Vec<String>,
    pub created: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ZoneStatus {
    Available,
    Degraded,
    Unavailable,
}

#[derive(Debug, Deserialize)]
pub struct CreateZoneRequest {
    pub name: String,
    #[serde(default)]
    pub description: String,
    #[serde(default = "default_region")]
    pub region: String,
    #[serde(default)]
    pub hosts: Vec<String>,
}

fn default_region() -> String {
    "default".to_string()
}

/// POST /api/zones - Create an availability zone
pub async fn create_zone(
    RequireWrite(_claims): RequireWrite,
    State(state): State<Arc<AppState>>,
    Json(req): Json<CreateZoneRequest>,
) -> Result<(StatusCode, Json<AvailabilityZone>), (StatusCode, Json<serde_json::Value>)> {
    tracing::debug!("zones::{}", stringify!(create_zone));
    if let Err((status, msg)) = crate::validation::validate_entity_name(&req.name) {
        return Err((status, Json(json!({"error": msg}))));
    }
    if req.hosts.len() > 1000 {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(json!({"error": "hosts count must not exceed 1000"})),
        ));
    }
    let zone = AvailabilityZone {
        id: uuid::Uuid::new_v4().to_string(),
        name: req.name,
        description: req.description,
        region: req.region,
        status: ZoneStatus::Available,
        hosts: req.hosts,
        created: Utc::now(),
    };

    state
        .store
        .save_entity("zones", &zone.id, &zone)
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": e.to_string() })),
            )
        })?;

    Ok((StatusCode::CREATED, Json(zone)))
}

/// GET /api/zones - List availability zones
pub async fn list_zones(
    RequireRead(_claims): RequireRead,
    State(state): State<Arc<AppState>>,
) -> Json<Vec<AvailabilityZone>> {
    tracing::debug!("zones::{}", stringify!(list_zones));
    let zones: Vec<AvailabilityZone> = state.store.list_entities("zones").unwrap_or_else(|e| {
        tracing::error!("Storage error: {}", e);
        Vec::new()
    });
    Json(zones)
}

/// GET /api/zones/:id - Get zone details
pub async fn get_zone(
    RequireRead(_claims): RequireRead,
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<AvailabilityZone>, (StatusCode, Json<serde_json::Value>)> {
    tracing::debug!("zones::{}", stringify!(get_zone));
    match state.store.get_entity::<AvailabilityZone>("zones", &id) {
        Ok(Some(zone)) => Ok(Json(zone)),
        Ok(None) => Err((
            StatusCode::NOT_FOUND,
            Json(json!({ "error": "Zone not found" })),
        )),
        Err(e) => Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": e.to_string() })),
        )),
    }
}

/// DELETE /api/zones/:id - Delete an availability zone
pub async fn delete_zone(
    RequireAdmin(_claims): RequireAdmin,
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<StatusCode, (StatusCode, Json<serde_json::Value>)> {
    tracing::debug!("zones::{}", stringify!(delete_zone));
    state.store.delete_entity("zones", &id).map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": e.to_string() })),
        )
    })?;

    Ok(StatusCode::NO_CONTENT)
}

// ============================================================================
// Spot / Preemptible Instances
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SpotInstance {
    pub id: String,
    pub vm_name: String,
    pub max_price_per_hour: f64,
    pub priority: SpotPriority,
    pub status: SpotStatus,
    pub zone_id: Option<String>,
    pub eviction_policy: EvictionPolicy,
    pub created: DateTime<Utc>,
    pub evicted_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SpotPriority {
    Low,
    Regular,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SpotStatus {
    Running,
    Evicted,
    Terminated,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EvictionPolicy {
    Stop,
    Delete,
    Deallocate,
}

#[derive(Debug, Deserialize)]
pub struct CreateSpotRequest {
    pub vm_name: String,
    #[serde(default = "default_max_price")]
    pub max_price_per_hour: f64,
    #[serde(default)]
    pub priority: Option<SpotPriority>,
    pub zone_id: Option<String>,
    #[serde(default = "default_eviction_policy")]
    pub eviction_policy: Option<EvictionPolicy>,
}

fn default_max_price() -> f64 {
    0.10
}
fn default_eviction_policy() -> Option<EvictionPolicy> {
    Some(EvictionPolicy::Stop)
}

/// POST /api/spot-instances - Create a spot instance request
pub async fn create_spot_instance(
    RequireWrite(_claims): RequireWrite,
    State(state): State<Arc<AppState>>,
    Json(req): Json<CreateSpotRequest>,
) -> Result<(StatusCode, Json<SpotInstance>), (StatusCode, Json<serde_json::Value>)> {
    tracing::debug!("zones::{}", stringify!(create_spot_instance));
    crate::validation::validate_vm_name(&req.vm_name)
        .map_err(|(s, m)| (s, Json(json!({"error": m}))))?;
    if !req.max_price_per_hour.is_finite() || req.max_price_per_hour <= 0.0 {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(json!({"error": "max_price_per_hour must be a positive finite number"})),
        ));
    }
    // Verify VM exists
    match state.store.get_vm(&req.vm_name) {
        Ok(Some(_)) => {}
        Ok(None) => {
            return Err((
                StatusCode::NOT_FOUND,
                Json(json!({ "error": "VM not found" })),
            ))
        }
        Err(e) => {
            return Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": e.to_string() })),
            ))
        }
    }

    let spot = SpotInstance {
        id: uuid::Uuid::new_v4().to_string(),
        vm_name: req.vm_name,
        max_price_per_hour: req.max_price_per_hour,
        priority: req.priority.unwrap_or(SpotPriority::Low),
        status: SpotStatus::Running,
        zone_id: req.zone_id,
        eviction_policy: req.eviction_policy.unwrap_or(EvictionPolicy::Stop),
        created: Utc::now(),
        evicted_at: None,
    };

    state
        .store
        .save_entity("spot_instances", &spot.id, &spot)
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": e.to_string() })),
            )
        })?;

    Ok((StatusCode::CREATED, Json(spot)))
}

/// GET /api/spot-instances - List spot instances
pub async fn list_spot_instances(
    RequireRead(_claims): RequireRead,
    State(state): State<Arc<AppState>>,
) -> Json<Vec<SpotInstance>> {
    tracing::debug!("zones::{}", stringify!(list_spot_instances));
    let spots: Vec<SpotInstance> =
        state
            .store
            .list_entities("spot_instances")
            .unwrap_or_else(|e| {
                tracing::error!("Storage error: {}", e);
                Vec::new()
            });
    Json(spots)
}

/// POST /api/spot-instances/:id/evict - Evict a spot instance
pub async fn evict_spot_instance(
    RequireAdmin(_claims): RequireAdmin,
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<SpotInstance>, (StatusCode, Json<serde_json::Value>)> {
    tracing::debug!("zones::{}", stringify!(evict_spot_instance));
    let mut spot = match state
        .store
        .get_entity::<SpotInstance>("spot_instances", &id)
    {
        Ok(Some(s)) => s,
        Ok(None) => {
            return Err((
                StatusCode::NOT_FOUND,
                Json(json!({ "error": "Spot instance not found" })),
            ))
        }
        Err(e) => {
            return Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": e.to_string() })),
            ))
        }
    };

    // Apply eviction policy
    match spot.eviction_policy {
        EvictionPolicy::Stop => {
            if let Err(e) = state.driver.poweroff(&spot.vm_name).await {
                tracing::error!("Failed to stop VM: {}", e);
            }
            if let Ok(Some(mut vm)) = state.store.get_vm(&spot.vm_name) {
                vm.state = vm_model::VMState::Stopped;
                if let Err(e) = state.store.save_vm(&vm) {
                    tracing::error!("Failed to save VM: {}", e);
                }
            }
        }
        EvictionPolicy::Delete => {
            if let Err(e) = state.driver.poweroff(&spot.vm_name).await {
                tracing::error!("Failed to stop VM: {}", e);
            }
            if let Err(e) = state.store.delete_vm(&spot.vm_name) {
                tracing::error!("Failed to delete VM: {}", e);
            }
        }
        EvictionPolicy::Deallocate => {
            if let Err(e) = state.driver.poweroff(&spot.vm_name).await {
                tracing::error!("Failed to stop VM: {}", e);
            }
            if let Ok(Some(mut vm)) = state.store.get_vm(&spot.vm_name) {
                vm.state = vm_model::VMState::Stopped;
                if let Err(e) = state.store.save_vm(&vm) {
                    tracing::error!("Failed to save VM: {}", e);
                }
            }
        }
    }

    spot.status = SpotStatus::Evicted;
    spot.evicted_at = Some(Utc::now());

    state
        .store
        .save_entity("spot_instances", &id, &spot)
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": e.to_string() })),
            )
        })?;

    Ok(Json(spot))
}

/// DELETE /api/spot-instances/:id - Delete spot instance record
pub async fn delete_spot_instance(
    RequireAdmin(_claims): RequireAdmin,
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<StatusCode, (StatusCode, Json<serde_json::Value>)> {
    tracing::debug!("zones::{}", stringify!(delete_spot_instance));
    state
        .store
        .delete_entity("spot_instances", &id)
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": e.to_string() })),
            )
        })?;

    Ok(StatusCode::NO_CONTENT)
}
