// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: LicenseRef-Zyvor-Production-1.0

use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    Json,
};
use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::collections::HashMap;
use std::sync::Arc;
use uuid::Uuid;

use crate::server::AppState;
use security::{RequireAdmin, RequireRead};

// ============================================================================
// Data Structures
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AuditStatus {
    Success,
    Failed,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditLog {
    pub id: String,
    pub timestamp: DateTime<Utc>,
    pub user: String,
    pub action: String,
    pub resource_type: String,
    pub resource_name: String,
    pub status: AuditStatus,
    pub ip_address: Option<String>,
    pub details: Option<String>,
    pub error: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct AuditLogFilters {
    pub action: Option<String>,
    pub user: Option<String>,
    pub resource_type: Option<String>,
    pub resource_name: Option<String>,
    pub status: Option<String>,
    pub start_time: Option<String>,
    pub end_time: Option<String>,
    pub search: Option<String>,
    #[serde(default = "default_limit")]
    pub limit: usize,
}

const MAX_AUDIT_LIMIT: usize = 1000;

#[derive(Debug, Deserialize)]
pub struct ExportQuery {
    #[serde(flatten)]
    pub filters: AuditLogFilters,
    #[serde(default = "default_format")]
    pub format: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditStats {
    pub total_logs: u64,
    pub by_action: HashMap<String, u64>,
    pub by_user: HashMap<String, u64>,
    pub by_status: HashMap<String, u64>,
    pub recent_failures: u64,
}

fn default_limit() -> usize {
    100
}

fn default_format() -> String {
    "json".to_string()
}

use crate::validation::escape_csv_field;

// ============================================================================
// Audit Log Handlers
// ============================================================================

pub async fn list_audit_logs(
    RequireRead(_claims): RequireRead,
    State(state): State<Arc<AppState>>,
    Query(filters): Query<AuditLogFilters>,
) -> Result<Json<Vec<AuditLog>>, (StatusCode, Json<serde_json::Value>)> {
    tracing::debug!("audit::{}", stringify!(list_audit_logs));

    // Clone filter values for the closure
    let action = filters.action.clone();
    let user = filters.user.clone();
    let resource_type = filters.resource_type.clone();
    let resource_name = filters.resource_name.clone();
    let status = filters.status.clone();
    let search = filters.search.as_ref().map(|s| s.to_lowercase());
    let limit = filters.limit.min(MAX_AUDIT_LIMIT);

    // Filter at storage layer — only loads and deserializes entries that match
    let logs = state
        .store
        .list_entities_filtered::<AuditLog, _>(
            "audit_logs",
            |log| {
                if let Some(ref a) = action {
                    if !log.action.to_lowercase().contains(&a.to_lowercase()) {
                        return false;
                    }
                }
                if let Some(ref u) = user {
                    if !log.user.to_lowercase().contains(&u.to_lowercase()) {
                        return false;
                    }
                }
                if let Some(ref rt) = resource_type {
                    if !log
                        .resource_type
                        .to_lowercase()
                        .contains(&rt.to_lowercase())
                    {
                        return false;
                    }
                }
                if let Some(ref rn) = resource_name {
                    if !log
                        .resource_name
                        .to_lowercase()
                        .contains(&rn.to_lowercase())
                    {
                        return false;
                    }
                }
                if let Some(ref s) = status {
                    let matches = match s.as_str() {
                        "success" => matches!(log.status, AuditStatus::Success),
                        "failed" => matches!(log.status, AuditStatus::Failed),
                        _ => true,
                    };
                    if !matches {
                        return false;
                    }
                }
                if let Some(ref s) = search {
                    if !log.action.to_lowercase().contains(s)
                        && !log.resource_name.to_lowercase().contains(s)
                        && !log.user.to_lowercase().contains(s)
                    {
                        return false;
                    }
                }
                true
            },
            limit,
        )
        .map_err(|_| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"error": "Failed to load audit logs"})),
            )
        })?;

    Ok(Json(logs))
}

pub async fn get_audit_log(
    RequireRead(_claims): RequireRead,
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<AuditLog>, (StatusCode, Json<serde_json::Value>)> {
    tracing::debug!("audit::{}", stringify!(get_audit_log));
    // Load from state store
    let log = state
        .store
        .get_entity::<AuditLog>("audit_logs", &id)
        .map_err(|_| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"error": "Failed to load audit log"})),
            )
        })?
        .ok_or((
            StatusCode::NOT_FOUND,
            Json(json!({"error": "Audit log not found"})),
        ))?;

    Ok(Json(log))
}

pub async fn export_audit_logs(
    RequireAdmin(_claims): RequireAdmin,
    State(state): State<Arc<AppState>>,
    Query(query): Query<ExportQuery>,
) -> Result<(StatusCode, String), (StatusCode, Json<serde_json::Value>)> {
    tracing::debug!("audit::{}", stringify!(export_audit_logs));
    // Apply the same filters as list_audit_logs
    let action = query.filters.action.clone();
    let user = query.filters.user.clone();
    let resource_type = query.filters.resource_type.clone();
    let resource_name = query.filters.resource_name.clone();
    let status = query.filters.status.clone();
    let search = query.filters.search.as_ref().map(|s| s.to_lowercase());

    let logs: Vec<AuditLog> = state
        .store
        .list_entities("audit_logs")
        .map_err(|_| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"error": "Failed to load audit logs"})),
            )
        })?
        .into_iter()
        .filter(|log: &AuditLog| {
            if let Some(ref a) = action {
                if log.action != *a {
                    return false;
                }
            }
            if let Some(ref u) = user {
                if log.user != *u {
                    return false;
                }
            }
            if let Some(ref rt) = resource_type {
                if log.resource_type != *rt {
                    return false;
                }
            }
            if let Some(ref rn) = resource_name {
                if log.resource_name != *rn {
                    return false;
                }
            }
            if let Some(ref s) = status {
                let log_status = match log.status {
                    AuditStatus::Success => "success",
                    AuditStatus::Failed => "failed",
                };
                if log_status != s {
                    return false;
                }
            }
            if let Some(ref q) = search {
                let haystack = format!(
                    "{} {} {} {} {}",
                    log.user,
                    log.action,
                    log.resource_type,
                    log.resource_name,
                    log.details.as_deref().unwrap_or("")
                )
                .to_lowercase();
                if !haystack.contains(q) {
                    return false;
                }
            }
            true
        })
        .collect();

    match query.format.as_str() {
        "json" => {
            let json_str = serde_json::to_string_pretty(&logs).map_err(|_| {
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(json!({"error": "Failed to serialize audit logs"})),
                )
            })?;
            Ok((StatusCode::OK, json_str))
        }
        "csv" => {
            let mut csv = String::from("ID,Timestamp,User,Action,Resource Type,Resource Name,Status,IP Address,Details,Error\n");

            for log in logs {
                csv.push_str(&format!(
                    "{},{},{},{},{},{},{},{},{},{}\n",
                    escape_csv_field(&log.id),
                    escape_csv_field(&log.timestamp.to_rfc3339()),
                    escape_csv_field(&log.user),
                    escape_csv_field(&log.action),
                    escape_csv_field(&log.resource_type),
                    escape_csv_field(&log.resource_name),
                    escape_csv_field(match log.status {
                        AuditStatus::Success => "success",
                        AuditStatus::Failed => "failed",
                    }),
                    escape_csv_field(&log.ip_address.unwrap_or_default()),
                    escape_csv_field(&log.details.unwrap_or_default()),
                    escape_csv_field(&log.error.unwrap_or_default()),
                ));
            }

            Ok((StatusCode::OK, csv))
        }
        _ => Err((
            StatusCode::BAD_REQUEST,
            Json(json!({"error": "Unsupported export format. Use 'json' or 'csv'"})),
        )),
    }
}

pub async fn get_audit_stats(
    RequireRead(_claims): RequireRead,
    State(state): State<Arc<AppState>>,
) -> Result<Json<AuditStats>, (StatusCode, Json<serde_json::Value>)> {
    tracing::debug!("audit::{}", stringify!(get_audit_stats));
    // Calculate from state store
    let logs = state
        .store
        .list_entities::<AuditLog>("audit_logs")
        .map_err(|_| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"error": "Failed to load audit logs"})),
            )
        })?;

    let total_logs = logs.len() as u64;

    let mut by_action: HashMap<String, u64> = HashMap::new();
    let mut by_user: HashMap<String, u64> = HashMap::new();
    let mut by_status: HashMap<String, u64> = HashMap::new();

    let mut recent_failures = 0;

    for log in logs {
        *by_action.entry(log.action.clone()).or_insert(0) += 1;
        *by_user.entry(log.user.clone()).or_insert(0) += 1;

        let status_str = match &log.status {
            AuditStatus::Success => "success",
            AuditStatus::Failed => "failed",
        };
        *by_status.entry(status_str.to_string()).or_insert(0) += 1;

        // Count failures in last 24 hours
        if matches!(log.status, AuditStatus::Failed)
            && (Utc::now() - log.timestamp) < Duration::hours(24)
        {
            recent_failures += 1;
        }
    }

    let stats = AuditStats {
        total_logs,
        by_action,
        by_user,
        by_status,
        recent_failures,
    };

    Ok(Json(stats))
}

// ============================================================================
// Audit Event Logger
// ============================================================================

/// Helper function to log an audit event
#[allow(clippy::too_many_arguments)]
pub async fn log_audit_event(
    state: &AppState,
    user: &str,
    action: &str,
    resource_type: &str,
    resource_name: &str,
    status: AuditStatus,
    ip_address: Option<&str>,
    details: Option<&str>,
    error: Option<&str>,
) -> Result<(), String> {
    tracing::debug!("audit::{}", stringify!(log_audit_event));
    let log = AuditLog {
        id: Uuid::new_v4().to_string(),
        timestamp: Utc::now(),
        user: user.to_string(),
        action: action.to_string(),
        resource_type: resource_type.to_string(),
        resource_name: resource_name.to_string(),
        status: status.clone(),
        ip_address: ip_address.map(|s| s.to_string()),
        details: details.map(|s| s.to_string()),
        error: error.map(|s| s.to_string()),
    };

    // Save to state store
    if let Err(e) = state.store.save_entity("audit_logs", &log.id, &log) {
        tracing::error!("Failed to save audit log: {}", e);
        return Err(format!("Failed to save audit log: {}", e));
    }

    // Also log to system logger for important events
    match status {
        AuditStatus::Failed => {
            tracing::warn!(
                "AUDIT: {} - {} on {} {} - FAILED: {}",
                user,
                action,
                resource_type,
                resource_name,
                error.unwrap_or("Unknown error")
            );
        }
        AuditStatus::Success => {
            tracing::info!(
                "AUDIT: {} - {} on {} {} - SUCCESS",
                user,
                action,
                resource_type,
                resource_name
            );
        }
    }

    Ok(())
}
