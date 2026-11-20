// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: LicenseRef-Zyvor-Production-1.0

use axum::{extract::State, http::StatusCode, Json};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

use crate::server::AppState;
use security::{RequireAdmin, RequireRead};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppSettings {
    #[serde(default = "default_daemon_name")]
    pub daemon_name: String,
    #[serde(default = "default_log_level")]
    pub log_level: String,
    #[serde(default = "crate::validation::default_true")]
    pub auto_refresh: bool,
    #[serde(default = "default_refresh_interval")]
    pub refresh_interval: u32,
    #[serde(default = "default_bridge")]
    pub default_bridge: String,
    #[serde(default)]
    pub enable_ipv6: bool,
    #[serde(default = "default_dns")]
    pub dns_servers: String,
    #[serde(default = "default_pool")]
    pub default_pool: String,
    #[serde(default = "default_format")]
    pub default_format: String,
    #[serde(default = "crate::validation::default_true")]
    pub enable_compression: bool,
    #[serde(default = "crate::validation::default_retention")]
    pub snapshot_retention: u32,
    #[serde(default = "crate::validation::default_true")]
    pub enable_auth: bool,
    #[serde(default)]
    pub enable_tls: bool,
    #[serde(default = "default_session_timeout")]
    pub session_timeout: u32,
    #[serde(default = "crate::validation::default_true")]
    pub audit_logging: bool,
    #[serde(default)]
    pub email_notifications: bool,
    #[serde(default)]
    pub webhook_url: String,
    #[serde(default = "crate::validation::default_true")]
    pub notify_on_start: bool,
    #[serde(default = "crate::validation::default_true")]
    pub notify_on_stop: bool,
    #[serde(default = "crate::validation::default_true")]
    pub notify_on_error: bool,
}

fn default_daemon_name() -> String {
    "zyvor-fabricd-01".to_string()
}
fn default_log_level() -> String {
    "info".to_string()
}
fn default_refresh_interval() -> u32 {
    5
}
fn default_bridge() -> String {
    "br0".to_string()
}
fn default_dns() -> String {
    "8.8.8.8, 8.8.4.4".to_string()
}
fn default_pool() -> String {
    "default".to_string()
}
fn default_format() -> String {
    "qcow2".to_string()
}
fn default_session_timeout() -> u32 {
    3600
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            daemon_name: default_daemon_name(),
            log_level: default_log_level(),
            auto_refresh: true,
            refresh_interval: default_refresh_interval(),
            default_bridge: default_bridge(),
            enable_ipv6: false,
            dns_servers: default_dns(),
            default_pool: default_pool(),
            default_format: default_format(),
            enable_compression: true,
            snapshot_retention: crate::validation::default_retention(),
            enable_auth: true,
            enable_tls: false,
            session_timeout: default_session_timeout(),
            audit_logging: true,
            email_notifications: false,
            webhook_url: String::new(),
            notify_on_start: true,
            notify_on_stop: true,
            notify_on_error: true,
        }
    }
}

/// GET /api/settings
pub async fn get_settings(
    RequireRead(_claims): RequireRead,
    State(state): State<Arc<AppState>>,
) -> Result<Json<AppSettings>, (StatusCode, Json<serde_json::Value>)> {
    tracing::debug!("settings::{}", stringify!(get_settings));
    let settings = state
        .store
        .get_entity::<AppSettings>("config", "settings")
        .map_err(|_| {
            crate::api_error::json_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                "Failed to load settings",
            )
        })?
        .unwrap_or_default();

    Ok(Json(settings))
}

/// PUT /api/settings
pub async fn update_settings(
    RequireAdmin(_claims): RequireAdmin,
    State(state): State<Arc<AppState>>,
    Json(settings): Json<AppSettings>,
) -> Result<Json<AppSettings>, (StatusCode, Json<serde_json::Value>)> {
    tracing::debug!("settings::{}", stringify!(update_settings));
    // Validate
    if settings.refresh_interval == 0 {
        return Err(crate::api_error::json_error(
            StatusCode::BAD_REQUEST,
            "Refresh interval must be greater than 0",
        ));
    }
    if settings.session_timeout == 0 {
        return Err(crate::api_error::json_error(
            StatusCode::BAD_REQUEST,
            "Session timeout must be greater than 0",
        ));
    }

    // Validate webhook URL against SSRF if provided
    if !settings.webhook_url.is_empty() {
        crate::api::notifications::validate_external_url_public(&settings.webhook_url).map_err(
            |e| {
                crate::api_error::json_error(
                    StatusCode::BAD_REQUEST,
                    format!("Invalid webhook URL: {}", e),
                )
            },
        )?;
    }

    if let Err(e) = state.store.save_entity("config", "settings", &settings) {
        tracing::error!("Failed to save settings: {}", e);
        return Err(crate::api_error::json_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "Failed to save settings",
        ));
    }

    tracing::info!("Settings updated successfully");
    Ok(Json(settings))
}
