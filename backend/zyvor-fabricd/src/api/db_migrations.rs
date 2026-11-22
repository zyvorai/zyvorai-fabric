// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: LicenseRef-Zyvor-Production-2.0

use axum::{extract::State, http::StatusCode, Json};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::sync::Arc;

use crate::server::AppState;
use security::RequireAdmin;

// ============================================================================
// Database Schema Migrations
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Migration {
    pub version: u32,
    pub name: String,
    pub applied_at: DateTime<Utc>,
}

/// List of schema migrations in order
fn all_migrations() -> Vec<(u32, &'static str, &'static str)> {
    vec![
        (1, "initial_schema", "CREATE TABLE IF NOT EXISTS users (id TEXT PRIMARY KEY, username TEXT UNIQUE NOT NULL, password_hash TEXT NOT NULL, role TEXT NOT NULL, created TEXT NOT NULL)"),
        (2, "add_user_email", "ALTER TABLE users ADD COLUMN email TEXT DEFAULT ''"),
        (3, "add_user_last_login", "ALTER TABLE users ADD COLUMN last_login TEXT"),
        (4, "add_api_keys_table", "CREATE TABLE IF NOT EXISTS api_keys (id TEXT PRIMARY KEY, name TEXT NOT NULL, key_hash TEXT NOT NULL, user_id TEXT NOT NULL, role TEXT NOT NULL, created TEXT NOT NULL, expires TEXT, revoked INTEGER DEFAULT 0)"),
        (5, "add_sessions_table", "CREATE TABLE IF NOT EXISTS sessions (id TEXT PRIMARY KEY, user_id TEXT NOT NULL, created TEXT NOT NULL, expires TEXT NOT NULL, revoked INTEGER DEFAULT 0)"),
    ]
}

/// GET /api/system/migrations - List applied migrations
pub async fn list_migrations(
    RequireAdmin(_claims): RequireAdmin,
    State(state): State<Arc<AppState>>,
) -> Result<Json<Vec<Migration>>, (StatusCode, Json<serde_json::Value>)> {
    let migrations = state
        .store
        .list_entities::<Migration>("db_migrations")
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"error": e.to_string()})),
            )
        })?;
    Ok(Json(migrations))
}

/// POST /api/system/migrations/apply - Apply pending migrations
pub async fn apply_migrations(
    RequireAdmin(_claims): RequireAdmin,
    State(state): State<Arc<AppState>>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let applied = state
        .store
        .list_entities::<Migration>("db_migrations")
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"error": e.to_string()})),
            )
        })?;

    let applied_versions: std::collections::HashSet<u32> =
        applied.iter().map(|m| m.version).collect();
    let mut newly_applied = Vec::new();

    let _user_db = state.user_db.as_ref().ok_or_else(|| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error": "Database not available"})),
        )
    })?;

    for (version, name, sql) in all_migrations() {
        if applied_versions.contains(&version) {
            continue;
        }

        // Execute migration SQL against the database
        tracing::info!("Applying migration v{}: {}", version, name);

        _user_db.run_migration(sql).map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"error": format!("Migration v{} failed: {}", version, e)})),
            )
        })?;

        // Record migration as applied
        let migration = Migration {
            version,
            name: name.to_string(),
            applied_at: Utc::now(),
        };

        state
            .store
            .save_entity("db_migrations", &version.to_string(), &migration)
            .map_err(|e| {
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(json!({"error": e.to_string()})),
                )
            })?;

        newly_applied.push(name.to_string());
    }

    Ok(Json(json!({
        "applied": newly_applied,
        "total_migrations": all_migrations().len(),
        "already_applied": applied_versions.len(),
    })))
}

/// GET /api/system/migrations/status - Check migration status
pub async fn migration_status(
    RequireAdmin(_claims): RequireAdmin,
    State(state): State<Arc<AppState>>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let applied = state
        .store
        .list_entities::<Migration>("db_migrations")
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"error": e.to_string()})),
            )
        })?;

    let total = all_migrations().len();
    let applied_count = applied.len();
    let pending = total - applied_count;

    Ok(Json(json!({
        "total_migrations": total,
        "applied": applied_count,
        "pending": pending,
        "up_to_date": pending == 0,
    })))
}
