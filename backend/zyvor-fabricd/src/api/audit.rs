// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: LicenseRef-Zyvor-Production-2.0

use axum::{
    body::Body,
    extract::{Path, Query, State},
    http::{header, HeaderValue, StatusCode},
    response::Response,
    Json,
};
use chrono::{Duration, Utc};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::collections::HashMap;
use std::sync::Arc;

use crate::audit_trail as trail;
use crate::server::AppState;
use security::{Claims, RequireAdmin, RequireRead, Role};

// ============================================================================
// Data Structures
// ============================================================================

pub use crate::audit_trail::{AuditLog, AuditStatus};

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
    let mut log = AuditLog::new(user, action, resource_type, resource_name, status);
    log.ip_address = ip_address.map(|s| s.to_string());
    log.details = details.map(|s| s.to_string());
    log.error = error.map(|s| s.to_string());

    // Save to state store (hash-chained when `[audit] chain` is on)
    if let Err(e) = trail::append(&state.store, state.config.audit.chain, log) {
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

// ============================================================================
// Export, integrity verification and retention
// ============================================================================

type ApiError = (StatusCode, Json<serde_json::Value>);

fn err(code: StatusCode, msg: impl Into<String>) -> ApiError {
    (code, Json(json!({ "error": msg.into() })))
}

/// Admins and read-only (Viewer) auditors may export and verify; a `user`
/// role that can change things may not.
fn require_auditor(claims: &Claims) -> Result<(), ApiError> {
    match claims.role {
        Role::Admin | Role::Viewer => Ok(()),
        Role::User => Err(err(
            StatusCode::FORBIDDEN,
            "audit export and verification need the admin or read-only auditor role",
        )),
    }
}

fn require_global(claims: &Claims) -> Result<(), ApiError> {
    if claims.tenant.is_some() {
        return Err(err(
            StatusCode::FORBIDDEN,
            "tenant-scoped tokens cannot verify or prune the shared audit log",
        ));
    }
    Ok(())
}

#[derive(Debug, Deserialize)]
pub struct ExportParams {
    pub from: Option<String>,
    pub to: Option<String>,
    pub actor: Option<String>,
    pub action: Option<String>,
    pub format: Option<String>,
}

/// `GET /api/audit/export?from&to&actor&action&format=ndjson|csv`
///
/// Streams matching events oldest first. `from` is inclusive and `to`
/// exclusive (a date-only `to` covers that whole day). `actor` and `action`
/// match exactly, ignoring case. More than `[audit] max_export_events`
/// matches is refused with 413: narrow the filters. A tenant-scoped caller
/// sees only events tagged with its tenant or performed by itself.
pub async fn export_audit(
    RequireRead(claims): RequireRead,
    State(state): State<Arc<AppState>>,
    Query(q): Query<ExportParams>,
) -> Result<Response, ApiError> {
    require_auditor(&claims)?;
    let format = q.format.as_deref().unwrap_or("ndjson").to_ascii_lowercase();
    if format != "ndjson" && format != "csv" {
        return Err(err(
            StatusCode::BAD_REQUEST,
            "unsupported format: use 'ndjson' or 'csv'",
        ));
    }
    let bound = |v: &Option<String>, is_to: bool| {
        v.as_deref()
            .filter(|s| !s.is_empty())
            .map(|s| trail::parse_bound(s, is_to))
            .transpose()
            .map_err(|m| err(StatusCode::BAD_REQUEST, m))
    };
    let from = bound(&q.from, false)?;
    let to = bound(&q.to, true)?;
    if let (Some(f), Some(t)) = (from, to) {
        if f >= t {
            return Err(err(StatusCode::BAD_REQUEST, "'from' must be before 'to'"));
        }
    }
    let nonempty = |v: Option<String>| v.filter(|s| !s.is_empty());
    let filter = trail::ExportFilter {
        from,
        to,
        actor: nonempty(q.actor),
        action: nonempty(q.action),
        tenant_scope: claims.tenant.clone().map(|t| (t, claims.sub.clone())),
    };

    let cap = state.config.audit.max_export_events;
    let (rows, overflow) = trail::collect_export(&state.store, &filter, cap).map_err(|_| {
        err(
            StatusCode::INTERNAL_SERVER_ERROR,
            "Failed to load audit logs",
        )
    })?;
    if overflow {
        return Err(err(
            StatusCode::PAYLOAD_TOO_LARGE,
            format!("more than {cap} events match: narrow the time range or filters"),
        ));
    }

    // Record who exported what (the export itself does not contain this event).
    let count = rows.len();
    let mut ev = AuditLog::new(
        &claims.sub,
        "AUDIT_EXPORT",
        "audit",
        "audit_logs",
        AuditStatus::Success,
    );
    ev.tenant = claims.tenant.clone();
    ev.details = Some(
        json!({"format": format, "events": count, "from": from, "to": to,
               "actor": filter.actor, "action": filter.action})
        .to_string(),
    );
    if let Err(e) = trail::append(&state.store, state.config.audit.chain, ev) {
        tracing::warn!("Failed to record audit export event: {}", e);
    }

    let csv = format == "csv";
    let header_row = csv.then(|| trail::CSV_HEADER.to_string());
    let lines = header_row.into_iter().chain(rows.into_iter().map(move |r| {
        if csv {
            trail::csv_row(&r)
        } else {
            trail::ndjson_line(&r)
        }
    }));
    let stream = futures::stream::iter(
        lines.map(|l| Ok::<_, std::convert::Infallible>(axum::body::Bytes::from(l))),
    );

    let (ctype, ext) = if csv {
        ("text/csv; charset=utf-8", "csv")
    } else {
        ("application/x-ndjson", "ndjson")
    };
    let mut resp = Response::new(Body::from_stream(stream));
    let h = resp.headers_mut();
    h.insert(header::CONTENT_TYPE, HeaderValue::from_static(ctype));
    h.insert(
        header::CONTENT_DISPOSITION,
        HeaderValue::from_str(&format!(
            "attachment; filename=\"audit-export-{}.{ext}\"",
            Utc::now().format("%Y%m%dT%H%M%SZ")
        ))
        .unwrap_or_else(|_| HeaderValue::from_static("attachment")),
    );
    h.insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    h.insert(
        header::HeaderName::from_static("x-audit-export-count"),
        HeaderValue::from(count),
    );
    Ok(resp)
}

/// `GET /api/audit/verify` - walk the hash chain and report the first
/// broken link. Always 200 when the check ran; read `ok` and `first_break`.
pub async fn verify_audit(
    RequireRead(claims): RequireRead,
    State(state): State<Arc<AppState>>,
) -> Result<Json<trail::VerifyReport>, ApiError> {
    require_auditor(&claims)?;
    require_global(&claims)?;
    trail::verify(&state.store, state.config.audit.chain)
        .map(Json)
        .map_err(|_| {
            err(
                StatusCode::INTERNAL_SERVER_ERROR,
                "Failed to verify audit logs",
            )
        })
}

#[derive(Debug, Deserialize)]
pub struct RetentionParams {
    /// Default true: a request that does not say `dry_run=false` only previews.
    pub dry_run: Option<bool>,
    pub max_age_days: Option<u64>,
    pub max_events: Option<u64>,
    /// Prune even though the chain does not verify.
    pub force: Option<bool>,
}

/// `POST /api/audit/retention?dry_run=true|false[&max_age_days=N][&max_events=N]`
///
/// Limits default to the `[audit]` config. A real run writes an
/// `AUDIT_RETENTION` event before it deletes anything, and is refused (409)
/// when the chain does not verify unless `force=true`. Admin only.
pub async fn run_audit_retention(
    RequireAdmin(claims): RequireAdmin,
    State(state): State<Arc<AppState>>,
    Query(q): Query<RetentionParams>,
) -> Result<Json<trail::RetentionReport>, ApiError> {
    require_global(&claims)?;
    let cfg = trail::RetentionLimits::from_config(&state.config.audit);
    let limits = trail::RetentionLimits {
        max_age_days: q.max_age_days.or(cfg.max_age_days),
        max_events: q.max_events.or(cfg.max_events),
    };
    match trail::run_retention(
        &state.store,
        state.config.audit.chain,
        &claims.sub,
        &limits,
        q.dry_run.unwrap_or(true),
        q.force.unwrap_or(false),
        Utc::now(),
    ) {
        Ok(r) => Ok(Json(r)),
        Err(trail::RetentionError::Invalid(m)) => Err(err(StatusCode::BAD_REQUEST, m)),
        Err(trail::RetentionError::IntegrityBroken(v)) => Err((
            StatusCode::CONFLICT,
            Json(json!({
                "error": "the audit chain does not verify; refusing to prune (use force=true to override)",
                "verify": v,
            })),
        )),
        Err(trail::RetentionError::Internal(e)) => {
            tracing::error!("audit retention failed: {e}");
            Err(err(
                StatusCode::INTERNAL_SERVER_ERROR,
                "Audit retention failed",
            ))
        }
    }
}
