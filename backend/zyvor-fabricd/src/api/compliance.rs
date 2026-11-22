// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: LicenseRef-Zyvor-Production-2.0

use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use serde::Deserialize;
use std::sync::Arc;

use crate::server::AppState;
use security::{RequireAdmin, RequireRead};

/// GET /api/compliance/profiles - List compliance profiles (includes default)
pub async fn list_compliance_profiles(
    RequireRead(_claims): RequireRead,
    State(state): State<Arc<AppState>>,
) -> Result<Json<Vec<compliance::ComplianceProfile>>, (StatusCode, Json<serde_json::Value>)> {
    let mut profiles: Vec<compliance::ComplianceProfile> = state
        .store
        .list_entities("compliance_profiles")
        .unwrap_or_default();

    // Always include the default profile
    let default = compliance::default_security_profile();
    if !profiles.iter().any(|p| p.id == default.id) {
        profiles.insert(0, default);
    }

    Ok(Json(profiles))
}

#[derive(Debug, Deserialize)]
pub struct ScanRequest {
    #[serde(default = "default_profile_id")]
    pub profile_id: String,
}

fn default_profile_id() -> String {
    "cis-baseline-v1".to_string()
}

/// POST /api/compliance/scan/:vm_name - Scan a VM against a compliance profile
pub async fn scan_vm_compliance(
    RequireAdmin(_claims): RequireAdmin,
    State(state): State<Arc<AppState>>,
    Path(vm_name): Path<String>,
    Json(req): Json<ScanRequest>,
) -> Result<Json<compliance::ComplianceScanResult>, (StatusCode, Json<serde_json::Value>)> {
    crate::validation::validate_vm_name(&vm_name)
        .map_err(|(s, m)| (s, Json(serde_json::json!({"error": m}))))?;

    // Validate profile_id
    crate::validation::validate_entity_name(&req.profile_id)
        .map_err(|(s, m)| (s, Json(serde_json::json!({"error": m}))))?;

    // Load profile
    let profile = if req.profile_id == "cis-baseline-v1" {
        compliance::default_security_profile()
    } else {
        state
            .store
            .get_entity::<compliance::ComplianceProfile>("compliance_profiles", &req.profile_id)
            .map_err(|e| {
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(serde_json::json!({"error": e.to_string()})),
                )
            })?
            .ok_or_else(|| {
                (
                    StatusCode::NOT_FOUND,
                    Json(serde_json::json!({"error": "Compliance profile not found"})),
                )
            })?
    };

    // Load VM data as JSON value
    let vm: serde_json::Value = state
        .store
        .get_entity("vms", &vm_name)
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"error": e.to_string()})),
            )
        })?
        .unwrap_or_else(|| serde_json::json!({"name": vm_name}));

    let result = compliance::scan_vm(&vm, &profile);

    // Store the result
    let _ = state
        .store
        .save_entity("compliance_results", &result.id, &result);

    Ok(Json(result))
}

/// GET /api/compliance/results - List all compliance scan results
pub async fn list_compliance_results(
    RequireRead(_claims): RequireRead,
    State(state): State<Arc<AppState>>,
) -> Result<Json<Vec<compliance::ComplianceScanResult>>, (StatusCode, Json<serde_json::Value>)> {
    let results: Vec<compliance::ComplianceScanResult> = state
        .store
        .list_entities("compliance_results")
        .unwrap_or_default();

    Ok(Json(results))
}
