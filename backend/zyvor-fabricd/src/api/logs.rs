// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: LicenseRef-Zyvor-Production-2.0

use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use futures::StreamExt;
use security::RequireRead;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

use crate::server::AppState;

#[derive(Debug, Deserialize)]
pub struct LogQuery {
    /// Number of log lines to return (default 100, max 1000).
    pub lines: Option<u32>,
    /// Optional priority filter (0-7, where 0=emerg, 7=debug).
    pub priority: Option<u8>,
    /// Optional grep pattern.
    pub grep: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct LogEntry {
    pub timestamp: String,
    pub hostname: String,
    pub unit: String,
    pub message: String,
    pub priority: String,
}

#[derive(Debug, Serialize)]
pub struct LogResponse {
    pub entries: Vec<LogEntry>,
    pub count: usize,
}

/// GET /api/vms/:name/logs - Returns recent log entries for a specific VM,
/// via `state.driver`'s `LogDriver` (journald for `MachinectlDriver`,
/// FluxVM's captured console output for `FluxVmDriver`).
pub async fn get_vm_logs(
    RequireRead(_claims): RequireRead,
    State(state): State<Arc<AppState>>,
    Path(vm_name): Path<String>,
    Query(query): Query<LogQuery>,
) -> Result<impl IntoResponse, (StatusCode, Json<serde_json::Value>)> {
    // Validate VM name
    if vm_name.is_empty()
        || vm_name.len() > 64
        || !vm_name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'))
    {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"error": "Invalid VM name"})),
        ));
    }

    let lines = query.lines.unwrap_or(100).min(1000);

    // `LogDriver::stream_logs` always follows (it mirrors `journalctl
    // --follow -n lines`), which is right for a websocket tail but would
    // hang this one-shot GET waiting for more output than currently
    // exists. Drain what's immediately available within a short deadline
    // instead of awaiting the stream to completion.
    let mut stream = state
        .driver
        .stream_logs(&vm_name, lines)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"error": format!("Failed to read VM logs: {:#}", e)})),
            )
        })?;

    let mut raw = Vec::new();
    let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(3);
    while raw.len() < lines as usize {
        let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
        if remaining.is_zero() {
            break;
        }
        match tokio::time::timeout(remaining, stream.next()).await {
            Ok(Some(entry)) => raw.push(entry),
            Ok(None) | Err(_) => break,
        }
    }

    let entries: Vec<LogEntry> = raw
        .into_iter()
        .filter(|e| query.priority.map(|p| e.priority <= p).unwrap_or(true))
        .filter(|e| {
            query
                .grep
                .as_ref()
                .map(|g| e.message.contains(g.as_str()))
                .unwrap_or(true)
        })
        .map(|e| LogEntry {
            timestamp: e.timestamp.to_rfc3339(),
            hostname: String::new(),
            unit: e.unit,
            message: e.message,
            priority: e.priority.to_string(),
        })
        .collect();
    let count = entries.len();

    Ok(Json(LogResponse { entries, count }))
}

/// GET /api/logs - Returns recent system-level journal entries.
pub async fn get_system_logs(
    RequireRead(_claims): RequireRead,
    State(_state): State<Arc<AppState>>,
    Query(query): Query<LogQuery>,
) -> Result<impl IntoResponse, (StatusCode, Json<serde_json::Value>)> {
    let lines = query.lines.unwrap_or(100).min(1000);

    let mut args = vec![
        "--output".to_string(),
        "json".to_string(),
        "--no-pager".to_string(),
        "-n".to_string(),
        lines.to_string(),
    ];

    if let Some(priority) = query.priority {
        if priority <= 7 {
            args.push(format!("--priority={}", priority));
        }
    }

    if let Some(ref pattern) = query.grep {
        // Sanitize grep pattern: only allow alphanumeric, spaces, hyphens, underscores, dots
        if pattern.len() <= 256
            && pattern.chars().all(|c| {
                c.is_ascii_alphanumeric()
                    || matches!(c, ' ' | '-' | '_' | '.' | ':' | '/' | '=' | '[' | ']')
            })
        {
            args.push(format!("--grep={}", pattern));
        }
    }

    let output = tokio::process::Command::new("journalctl")
        .args(&args)
        .output()
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"error": format!("Failed to run journalctl: {}", e)})),
            )
        })?;

    let stdout = String::from_utf8_lossy(&output.stdout);
    let entries = parse_journal_json(&stdout);
    let count = entries.len();

    Ok(Json(LogResponse { entries, count }))
}

/// Parse journalctl JSON output into structured log entries.
fn parse_journal_json(raw: &str) -> Vec<LogEntry> {
    raw.lines()
        .filter_map(|line| {
            let obj: serde_json::Value = serde_json::from_str(line).ok()?;

            let timestamp = obj
                .get("__REALTIME_TIMESTAMP")
                .and_then(|v| v.as_str())
                .and_then(|ts| ts.parse::<i64>().ok())
                .map(|us| {
                    let dt = chrono::DateTime::from_timestamp(
                        us / 1_000_000,
                        ((us % 1_000_000) * 1000) as u32,
                    );
                    dt.map(|d| d.to_rfc3339()).unwrap_or_default()
                })
                .unwrap_or_default();

            let hostname = obj
                .get("_HOSTNAME")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();

            let unit = obj
                .get("_SYSTEMD_UNIT")
                .or_else(|| obj.get("SYSLOG_IDENTIFIER"))
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();

            let message = obj
                .get("MESSAGE")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();

            let priority = obj
                .get("PRIORITY")
                .and_then(|v| v.as_str())
                .unwrap_or("6")
                .to_string();

            Some(LogEntry {
                timestamp,
                hostname,
                unit,
                message,
                priority,
            })
        })
        .collect()
}
