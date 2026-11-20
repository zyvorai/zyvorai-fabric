// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: LicenseRef-Zyvor-Production-1.0

use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    Json,
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
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
pub enum ChannelType {
    Email,
    Slack,
    Webhook,
    Teams,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NotificationChannel {
    pub id: String,
    pub name: String,
    #[serde(rename = "type")]
    pub channel_type: ChannelType,
    pub config: HashMap<String, serde_json::Value>,
    pub enabled: bool,
    pub created: DateTime<Utc>,
    pub last_test: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateChannelRequest {
    pub name: String,
    #[serde(rename = "type")]
    pub channel_type: ChannelType,
    pub config: HashMap<String, serde_json::Value>,
    #[serde(default = "crate::validation::default_true")]
    pub enabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateChannelRequest {
    pub name: Option<String>,
    pub config: Option<HashMap<String, serde_json::Value>>,
    pub enabled: Option<bool>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Severity {
    Info,
    Warning,
    Critical,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NotificationRule {
    pub id: String,
    pub name: String,
    pub description: Option<String>,
    pub event_types: Vec<String>,
    pub severity_levels: Vec<Severity>,
    pub channels: Vec<String>,
    pub vm_tags: Option<Vec<String>>,
    pub enabled: bool,
    pub created: DateTime<Utc>,
    pub triggered_count: u64,
    pub last_triggered: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateRuleRequest {
    pub name: String,
    pub description: Option<String>,
    pub event_types: Vec<String>,
    pub severity_levels: Vec<Severity>,
    pub channels: Vec<String>,
    pub vm_tags: Option<Vec<String>>,
    #[serde(default = "crate::validation::default_true")]
    pub enabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateRuleRequest {
    pub name: Option<String>,
    pub description: Option<String>,
    pub event_types: Option<Vec<String>>,
    pub severity_levels: Option<Vec<Severity>>,
    pub channels: Option<Vec<String>>,
    pub vm_tags: Option<Vec<String>>,
    pub enabled: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum NotificationStatus {
    Sent,
    Failed,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NotificationHistory {
    pub id: String,
    pub rule_id: String,
    pub rule_name: String,
    pub event_type: String,
    pub severity: Severity,
    pub channel: String,
    pub vm_name: Option<String>,
    pub message: String,
    pub sent_at: DateTime<Utc>,
    pub status: NotificationStatus,
    pub error: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct HistoryQuery {
    #[serde(default = "default_limit")]
    pub limit: usize,
}

fn default_limit() -> usize {
    50
}

// ============================================================================
// Validation Functions
// ============================================================================

fn validate_channel_config(
    channel_type: &ChannelType,
    config: &HashMap<String, serde_json::Value>,
) -> Result<(), String> {
    match channel_type {
        ChannelType::Email => {
            // Validate email configuration
            if !config.contains_key("smtp_host") {
                return Err("Email channel requires 'smtp_host'".to_string());
            }
            if !config.contains_key("smtp_port") {
                return Err("Email channel requires 'smtp_port'".to_string());
            }
            if !config.contains_key("from") {
                return Err("Email channel requires 'from' address".to_string());
            }
            if !config.contains_key("to") {
                return Err("Email channel requires 'to' addresses".to_string());
            }
        }
        ChannelType::Slack => {
            // Validate Slack configuration
            if !config.contains_key("webhook_url") {
                return Err("Slack channel requires 'webhook_url'".to_string());
            }
            // Validate webhook URL format
            if let Some(url) = config.get("webhook_url").and_then(|v| v.as_str()) {
                let parsed_host = url
                    .trim_start_matches("https://")
                    .trim_start_matches("http://")
                    .split('/')
                    .next()
                    .unwrap_or("")
                    .split(':')
                    .next()
                    .unwrap_or("");
                if parsed_host != "hooks.slack.com" {
                    return Err("Slack webhook URL must have host hooks.slack.com".to_string());
                }
                if !url.starts_with("https://") {
                    return Err("Slack webhook URL must use HTTPS".to_string());
                }
            }
        }
        ChannelType::Webhook => {
            // Validate webhook configuration
            if !config.contains_key("url") {
                return Err("Webhook channel requires 'url'".to_string());
            }
            // Validate URL format and block internal/private addresses (SSRF prevention)
            if let Some(url) = config.get("url").and_then(|v| v.as_str()) {
                if !url.starts_with("http://") && !url.starts_with("https://") {
                    return Err("Webhook URL must start with http:// or https://".to_string());
                }
                validate_external_url(url)?;
            }
        }
        ChannelType::Teams => {
            // Validate Teams configuration
            if !config.contains_key("webhook_url") {
                return Err("Teams channel requires 'webhook_url'".to_string());
            }
            // Validate webhook URL format
            if let Some(url) = config.get("webhook_url").and_then(|v| v.as_str()) {
                let parsed_host = url
                    .trim_start_matches("https://")
                    .trim_start_matches("http://")
                    .split('/')
                    .next()
                    .unwrap_or("")
                    .split(':')
                    .next()
                    .unwrap_or("");
                if !(parsed_host.ends_with(".office.com")
                    || parsed_host.ends_with(".microsoft.com"))
                {
                    return Err(
                        "Teams webhook URL must be from office.com or microsoft.com domain"
                            .to_string(),
                    );
                }
                if !url.starts_with("https://") {
                    return Err("Teams webhook URL must use HTTPS".to_string());
                }
            }
        }
    }

    Ok(())
}

fn validate_notification_rule(rule: &CreateRuleRequest) -> Result<(), String> {
    // Validate event types are not empty
    if rule.event_types.is_empty() {
        return Err("Rule must have at least one event type".to_string());
    }

    // Validate severity levels are not empty
    if rule.severity_levels.is_empty() {
        return Err("Rule must have at least one severity level".to_string());
    }

    // Validate channels are not empty
    if rule.channels.is_empty() {
        return Err("Rule must have at least one channel".to_string());
    }

    Ok(())
}

/// Validate that a URL points to an external host, not internal/private networks.
/// Prevents SSRF attacks against internal services like metadata endpoints.
/// Validate that a URL does not target internal/private addresses (SSRF prevention).
pub fn validate_external_url_public(url: &str) -> Result<(), String> {
    validate_external_url(url)
}

fn validate_external_url(url: &str) -> Result<(), String> {
    // Parse the URL to extract the host
    let host = url
        .trim_start_matches("https://")
        .trim_start_matches("http://")
        .split('/')
        .next()
        .unwrap_or("")
        .split(':')
        .next()
        .unwrap_or("");

    if host.is_empty() {
        return Err("URL has no host".to_string());
    }

    // Block well-known internal hostnames
    let blocked_hosts = [
        "localhost",
        "localhost.localdomain",
        "metadata.google.internal",
        "metadata",
    ];
    let host_lower = host.to_lowercase();
    if blocked_hosts.iter().any(|&b| host_lower == b) {
        return Err(format!(
            "Webhook URL host '{}' is not allowed (internal host)",
            host
        ));
    }

    // Check if the host is an IP address and block private/internal ranges
    if let Ok(ip) = host.parse::<std::net::IpAddr>() {
        let is_private = match ip {
            std::net::IpAddr::V4(v4) => {
                v4.is_loopback()                              // 127.0.0.0/8
                    || v4.is_private()                        // 10.0.0.0/8, 172.16.0.0/12, 192.168.0.0/16
                    || v4.is_link_local()                     // 169.254.0.0/16
                    || v4.is_unspecified()                     // 0.0.0.0
                    || v4.is_broadcast()                      // 255.255.255.255
                    || v4.octets()[0] == 100 && (v4.octets()[1] & 0xC0) == 64 // 100.64.0.0/10 (CGNAT)
            }
            std::net::IpAddr::V6(v6) => {
                v6.is_loopback()                              // ::1
                    || v6.is_unspecified()                     // ::
                    || (v6.segments()[0] & 0xffc0) == 0xfe80   // fe80::/10 link-local
                    || (v6.segments()[0] & 0xfe00) == 0xfc00 // fc00::/7 ULA
            }
        };

        if is_private {
            return Err(format!(
                "Webhook URL must not target private/internal IP address '{}'",
                ip
            ));
        }
    }

    // Also resolve hostnames to check resolved IPs against private ranges
    if host.parse::<std::net::IpAddr>().is_err() {
        // It's a hostname, not an IP — resolve it
        if let Ok(addrs) = std::net::ToSocketAddrs::to_socket_addrs(&(host, 80u16)) {
            for addr in addrs {
                let ip = addr.ip();
                let is_resolved_private = match ip {
                    std::net::IpAddr::V4(v4) => {
                        v4.is_loopback()
                            || v4.is_private()
                            || v4.is_link_local()
                            || v4.is_unspecified()
                            || v4.is_broadcast()
                            || (v4.octets()[0] == 100 && (v4.octets()[1] & 0xC0) == 64)
                            || (v4.octets()[0] == 169 && v4.octets()[1] == 254)
                    }
                    std::net::IpAddr::V6(v6) => {
                        v6.is_loopback()
                            || v6.is_unspecified()
                            || (v6.segments()[0] & 0xffc0) == 0xfe80
                            || (v6.segments()[0] & 0xfe00) == 0xfc00
                    }
                };
                if is_resolved_private {
                    return Err(format!(
                        "URL host '{}' resolves to private/internal IP '{}'",
                        host, ip
                    ));
                }
            }
        }
    }

    Ok(())
}

// ============================================================================
// Channel Handlers
// ============================================================================

/// Sensitive config keys that should be redacted in API responses.
const REDACTED_CONFIG_KEYS: &[&str] = &["password", "client_secret", "api_key", "token", "secret"];

pub async fn list_channels(
    RequireRead(_claims): RequireRead,
    State(state): State<Arc<AppState>>,
) -> Result<Json<Vec<NotificationChannel>>, (StatusCode, Json<serde_json::Value>)> {
    tracing::debug!("notifications::{}", stringify!(list_channels));
    // Load from state store
    let mut channels = state
        .store
        .list_entities::<NotificationChannel>("notifications/channels")
        .map_err(|e| {
            tracing::error!("Failed to load channels: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"error": "Failed to load notification channels"})),
            )
        })?;

    // Redact sensitive config fields
    for channel in &mut channels {
        for key in REDACTED_CONFIG_KEYS {
            if channel.config.contains_key(*key) {
                channel
                    .config
                    .insert(key.to_string(), serde_json::json!("**REDACTED**"));
            }
        }
    }

    Ok(Json(channels))
}

pub async fn create_channel(
    RequireAdmin(_claims): RequireAdmin,
    State(state): State<Arc<AppState>>,
    Json(req): Json<CreateChannelRequest>,
) -> Result<(StatusCode, Json<NotificationChannel>), (StatusCode, Json<serde_json::Value>)> {
    tracing::debug!("notifications::{}", stringify!(create_channel));
    // Validate config based on channel type
    if let Err(err) = validate_channel_config(&req.channel_type, &req.config) {
        tracing::warn!("Invalid channel config: {}", err);
        return Err((
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"error": err})),
        ));
    }

    let channel = NotificationChannel {
        id: Uuid::new_v4().to_string(),
        name: req.name,
        channel_type: req.channel_type,
        config: req.config,
        enabled: req.enabled,
        created: Utc::now(),
        last_test: None,
    };

    // Save to state store
    if let Err(e) = state
        .store
        .save_entity("notifications/channels", &channel.id, &channel)
    {
        tracing::error!("Failed to save notification channel: {}", e);
        return Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": "Failed to save notification channel"})),
        ));
    }

    // Redact secrets before returning
    let mut response = channel;
    for key in REDACTED_CONFIG_KEYS {
        if response.config.contains_key(*key) {
            response
                .config
                .insert(key.to_string(), serde_json::json!("**REDACTED**"));
        }
    }

    Ok((StatusCode::CREATED, Json(response)))
}

pub async fn update_channel(
    RequireAdmin(_claims): RequireAdmin,
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    Json(req): Json<UpdateChannelRequest>,
) -> Result<Json<NotificationChannel>, (StatusCode, Json<serde_json::Value>)> {
    tracing::debug!("notifications::{}", stringify!(update_channel));
    // Load existing channel from state store
    let mut channel = state
        .store
        .get_entity::<NotificationChannel>("notifications/channels", &id)
        .map_err(|_| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"error": "Failed to load channel"})),
            )
        })?
        .ok_or_else(|| {
            (
                StatusCode::NOT_FOUND,
                Json(serde_json::json!({"error": "Channel not found"})),
            )
        })?;

    // Update fields if provided
    if let Some(name) = req.name {
        channel.name = name;
    }
    if let Some(config) = req.config {
        // Validate new config
        if let Err(err) = validate_channel_config(&channel.channel_type, &config) {
            tracing::warn!("Invalid channel config: {}", err);
            return Err((
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({"error": err})),
            ));
        }
        channel.config = config;
    }
    if let Some(enabled) = req.enabled {
        channel.enabled = enabled;
    }

    // Save to state store
    if let Err(e) = state
        .store
        .save_entity("notifications/channels", &channel.id, &channel)
    {
        tracing::error!("Failed to update channel: {}", e);
        return Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": "Failed to update channel"})),
        ));
    }

    // Redact secrets before returning
    for key in REDACTED_CONFIG_KEYS {
        if channel.config.contains_key(*key) {
            channel
                .config
                .insert(key.to_string(), serde_json::json!("**REDACTED**"));
        }
    }

    Ok(Json(channel))
}

pub async fn delete_channel(
    RequireAdmin(_claims): RequireAdmin,
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<StatusCode, (StatusCode, Json<serde_json::Value>)> {
    tracing::debug!("notifications::{}", stringify!(delete_channel));
    // Check if channel is used by any rules
    let rules = state
        .store
        .list_entities::<NotificationRule>("notifications/rules")
        .map_err(|e| {
            tracing::error!("Failed to load rules: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"error": "Failed to load notification rules"})),
            )
        })?;

    for rule in rules {
        if rule.channels.contains(&id) {
            tracing::warn!("Cannot delete channel {} - used by rule {}", id, rule.name);
            return Err((
                StatusCode::CONFLICT,
                Json(
                    serde_json::json!({"error": format!("Channel is in use by rule '{}'", rule.name)}),
                ),
            ));
        }
    }

    // Remove from state store
    if let Err(e) = state.store.delete_entity("notifications/channels", &id) {
        tracing::error!("Failed to delete channel: {}", e);
        return Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": "Failed to delete channel"})),
        ));
    }

    Ok(StatusCode::NO_CONTENT)
}

pub async fn test_channel(
    RequireAdmin(_claims): RequireAdmin,
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<StatusCode, (StatusCode, Json<serde_json::Value>)> {
    tracing::debug!("notifications::{}", stringify!(test_channel));
    // Load channel from state store
    let mut channel = state
        .store
        .get_entity::<NotificationChannel>("notifications/channels", &id)
        .map_err(|_| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"error": "Failed to load channel"})),
            )
        })?
        .ok_or_else(|| {
            (
                StatusCode::NOT_FOUND,
                Json(serde_json::json!({"error": "Channel not found"})),
            )
        })?;

    // Send test notification based on channel type
    let test_message = format!(
        "Test notification from zyvor-fabricd - Channel: {}",
        channel.name
    );
    match send_notification(
        &state.http_client,
        &channel,
        "Test Notification",
        &test_message,
    )
    .await
    {
        Ok(_) => {
            tracing::info!(
                "Successfully sent test notification to channel {} (type: {:?})",
                channel.name,
                channel.channel_type
            );
        }
        Err(e) => {
            tracing::error!(
                "Failed to send test notification to channel {}: {}",
                channel.name,
                e
            );
            return Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"error": format!("Test notification failed: {}", e)})),
            ));
        }
    }

    // Update last_test timestamp
    channel.last_test = Some(Utc::now());

    // Save to state store
    if let Err(e) = state
        .store
        .save_entity("notifications/channels", &channel.id, &channel)
    {
        tracing::error!("Failed to update channel last_test: {}", e);
        return Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": "Failed to update channel"})),
        ));
    }

    Ok(StatusCode::OK)
}

// ============================================================================
// Rule Handlers
// ============================================================================

pub async fn list_rules(
    RequireRead(_claims): RequireRead,
    State(state): State<Arc<AppState>>,
) -> Result<Json<Vec<NotificationRule>>, (StatusCode, Json<serde_json::Value>)> {
    tracing::debug!("notifications::{}", stringify!(list_rules));
    // Load from state store
    let rules = state
        .store
        .list_entities::<NotificationRule>("notifications/rules")
        .map_err(|e| {
            tracing::error!("Failed to load rules: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"error": "Failed to load notification rules"})),
            )
        })?;

    Ok(Json(rules))
}

pub async fn create_rule(
    RequireAdmin(_claims): RequireAdmin,
    State(state): State<Arc<AppState>>,
    Json(req): Json<CreateRuleRequest>,
) -> Result<(StatusCode, Json<NotificationRule>), (StatusCode, Json<serde_json::Value>)> {
    tracing::debug!("notifications::{}", stringify!(create_rule));
    // Validate rule
    if let Err(err) = validate_notification_rule(&req) {
        tracing::warn!("Invalid notification rule: {}", err);
        return Err((
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"error": err})),
        ));
    }

    // Validate channels exist
    for channel_id in &req.channels {
        if state
            .store
            .get_entity::<NotificationChannel>("notifications/channels", channel_id)
            .ok()
            .flatten()
            .is_none()
        {
            tracing::warn!("Channel not found: {}", channel_id);
            return Err((
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({"error": format!("Channel not found: {}", channel_id)})),
            ));
        }
    }

    let rule = NotificationRule {
        id: Uuid::new_v4().to_string(),
        name: req.name,
        description: req.description,
        event_types: req.event_types,
        severity_levels: req.severity_levels,
        channels: req.channels,
        vm_tags: req.vm_tags,
        enabled: req.enabled,
        created: Utc::now(),
        triggered_count: 0,
        last_triggered: None,
    };

    // Save to state store
    if let Err(e) = state
        .store
        .save_entity("notifications/rules", &rule.id, &rule)
    {
        tracing::error!("Failed to save notification rule: {}", e);
        return Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": "Failed to save notification rule"})),
        ));
    }

    Ok((StatusCode::CREATED, Json(rule)))
}

pub async fn update_rule(
    RequireAdmin(_claims): RequireAdmin,
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    Json(req): Json<UpdateRuleRequest>,
) -> Result<Json<NotificationRule>, (StatusCode, Json<serde_json::Value>)> {
    tracing::debug!("notifications::{}", stringify!(update_rule));
    // Load existing rule from state store
    let mut rule = state
        .store
        .get_entity::<NotificationRule>("notifications/rules", &id)
        .map_err(|_| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"error": "Failed to load rule"})),
            )
        })?
        .ok_or_else(|| {
            (
                StatusCode::NOT_FOUND,
                Json(serde_json::json!({"error": "Rule not found"})),
            )
        })?;

    // Update fields if provided
    if let Some(name) = req.name {
        rule.name = name;
    }
    if let Some(description) = req.description {
        rule.description = Some(description);
    }
    if let Some(event_types) = req.event_types {
        rule.event_types = event_types;
    }
    if let Some(severity_levels) = req.severity_levels {
        rule.severity_levels = severity_levels;
    }
    if let Some(channels) = req.channels {
        // Validate channels exist
        for channel_id in &channels {
            if state
                .store
                .get_entity::<NotificationChannel>("notifications/channels", channel_id)
                .ok()
                .flatten()
                .is_none()
            {
                tracing::warn!("Channel not found: {}", channel_id);
                return Err((
                    StatusCode::BAD_REQUEST,
                    Json(
                        serde_json::json!({"error": format!("Channel not found: {}", channel_id)}),
                    ),
                ));
            }
        }
        rule.channels = channels;
    }
    if let Some(vm_tags) = req.vm_tags {
        rule.vm_tags = Some(vm_tags);
    }
    if let Some(enabled) = req.enabled {
        rule.enabled = enabled;
    }

    // Save to state store
    if let Err(e) = state
        .store
        .save_entity("notifications/rules", &rule.id, &rule)
    {
        tracing::error!("Failed to update rule: {}", e);
        return Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": "Failed to update rule"})),
        ));
    }

    Ok(Json(rule))
}

pub async fn delete_rule(
    RequireAdmin(_claims): RequireAdmin,
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<StatusCode, (StatusCode, Json<serde_json::Value>)> {
    tracing::debug!("notifications::{}", stringify!(delete_rule));
    // Remove from state store
    if let Err(e) = state.store.delete_entity("notifications/rules", &id) {
        tracing::error!("Failed to delete rule: {}", e);
        return Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": "Failed to delete rule"})),
        ));
    }

    Ok(StatusCode::NO_CONTENT)
}

pub async fn enable_rule(
    RequireAdmin(_claims): RequireAdmin,
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<StatusCode, (StatusCode, Json<serde_json::Value>)> {
    tracing::debug!("notifications::{}", stringify!(enable_rule));
    // Load rule from state store
    let mut rule = state
        .store
        .get_entity::<NotificationRule>("notifications/rules", &id)
        .map_err(|_| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"error": "Failed to load rule"})),
            )
        })?
        .ok_or_else(|| {
            (
                StatusCode::NOT_FOUND,
                Json(serde_json::json!({"error": "Rule not found"})),
            )
        })?;

    // Set enabled = true
    rule.enabled = true;

    // Save to state store
    if let Err(e) = state
        .store
        .save_entity("notifications/rules", &rule.id, &rule)
    {
        tracing::error!("Failed to enable rule: {}", e);
        return Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": "Failed to enable rule"})),
        ));
    }

    Ok(StatusCode::OK)
}

pub async fn disable_rule(
    RequireAdmin(_claims): RequireAdmin,
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<StatusCode, (StatusCode, Json<serde_json::Value>)> {
    tracing::debug!("notifications::{}", stringify!(disable_rule));
    // Load rule from state store
    let mut rule = state
        .store
        .get_entity::<NotificationRule>("notifications/rules", &id)
        .map_err(|_| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"error": "Failed to load rule"})),
            )
        })?
        .ok_or_else(|| {
            (
                StatusCode::NOT_FOUND,
                Json(serde_json::json!({"error": "Rule not found"})),
            )
        })?;

    // Set enabled = false
    rule.enabled = false;

    // Save to state store
    if let Err(e) = state
        .store
        .save_entity("notifications/rules", &rule.id, &rule)
    {
        tracing::error!("Failed to disable rule: {}", e);
        return Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": "Failed to disable rule"})),
        ));
    }

    Ok(StatusCode::OK)
}

// ============================================================================
// History Handlers
// ============================================================================

pub async fn get_history(
    RequireRead(_claims): RequireRead,
    State(state): State<Arc<AppState>>,
    Query(query): Query<HistoryQuery>,
) -> Result<Json<Vec<NotificationHistory>>, (StatusCode, Json<serde_json::Value>)> {
    tracing::debug!("notifications::{}", stringify!(get_history));
    // Load from state store
    let mut history = state
        .store
        .list_entities::<NotificationHistory>("notification_history")
        .map_err(|e| {
            tracing::error!("Failed to load history: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"error": "Failed to load notification history"})),
            )
        })?;

    // Sort by sent_at (most recent first)
    history.sort_by_key(|a| std::cmp::Reverse(a.sent_at));

    // Prune old history entries beyond retention limit (keep 500 most recent)
    const HISTORY_RETENTION_LIMIT: usize = 500;
    if history.len() > HISTORY_RETENTION_LIMIT {
        let to_delete: Vec<String> = history
            .drain(HISTORY_RETENTION_LIMIT..)
            .map(|h| h.id)
            .collect();
        let deleted_count = to_delete.len();
        for id in to_delete {
            let _ = state.store.delete_entity("notification_history", &id);
        }
        tracing::debug!("Pruned {} old notification history entries", deleted_count);
    }

    // Apply limit (cap at 500)
    history.truncate(query.limit.min(HISTORY_RETENTION_LIMIT));

    Ok(Json(history))
}

// ============================================================================
// Notification Sending Infrastructure
// ============================================================================

/// Send a notification through a specific channel with retry logic
async fn send_notification(
    client: &reqwest::Client,
    channel: &NotificationChannel,
    subject: &str,
    message: &str,
) -> Result<(), String> {
    let max_retries = 3u32;
    let mut last_err = String::new();

    for attempt in 0..max_retries {
        let result = match channel.channel_type {
            ChannelType::Email => send_email_notification(channel, subject, message).await,
            ChannelType::Slack => send_slack_notification(client, channel, subject, message).await,
            ChannelType::Webhook => {
                send_webhook_notification(client, channel, subject, message).await
            }
            ChannelType::Teams => send_teams_notification(client, channel, subject, message).await,
        };

        match result {
            Ok(()) => return Ok(()),
            Err(e) => {
                last_err = e;
                if attempt < max_retries - 1 {
                    let delay = tokio::time::Duration::from_secs(1 << attempt); // 1s, 2s, 4s
                    tracing::warn!(
                        "Notification attempt {} failed for channel '{}': {}. Retrying in {:?}",
                        attempt + 1,
                        channel.name,
                        last_err,
                        delay,
                    );
                    tokio::time::sleep(delay).await;
                }
            }
        }
    }

    Err(format!(
        "Failed after {} attempts: {}",
        max_retries, last_err
    ))
}

/// Send email notification (SMTP)
async fn send_email_notification(
    channel: &NotificationChannel,
    subject: &str,
    message: &str,
) -> Result<(), String> {
    use lettre::message::header::ContentType;
    use lettre::transport::smtp::authentication::Credentials;
    use lettre::{Message, SmtpTransport, Transport};

    let smtp_host = channel
        .config
        .get("smtp_host")
        .and_then(|v| v.as_str())
        .ok_or("Missing smtp_host in channel config")?;
    let smtp_port = channel
        .config
        .get("smtp_port")
        .and_then(|v| v.as_u64())
        .unwrap_or(587) as u16;
    let from = channel
        .config
        .get("from")
        .and_then(|v| v.as_str())
        .ok_or("Missing from address in channel config")?;
    let to_addrs = channel
        .config
        .get("to")
        .and_then(|v| v.as_array())
        .ok_or("Missing to addresses in channel config")?;

    // Optional SMTP authentication
    let username = channel.config.get("username").and_then(|v| v.as_str());
    let password = channel.config.get("password").and_then(|v| v.as_str());

    tracing::info!(
        "Sending email notification: {} -> {:?} via {}:{} (Subject: {})",
        from,
        to_addrs,
        smtp_host,
        smtp_port,
        subject
    );

    // Send email to each recipient
    for to_value in to_addrs {
        let to = to_value.as_str().ok_or("Invalid to address format")?;

        // Build email message
        let email = Message::builder()
            .from(
                from.parse()
                    .map_err(|e| format!("Invalid from address: {}", e))?,
            )
            .to(to
                .parse()
                .map_err(|e| format!("Invalid to address: {}", e))?)
            .subject(subject)
            .header(ContentType::TEXT_PLAIN)
            .body(message.to_string())
            .map_err(|e| format!("Failed to build email: {}", e))?;

        // Check if TLS verification is disabled via config
        let tls_verify = channel
            .config
            .get("tls_verify")
            .and_then(|v| v.as_bool())
            .unwrap_or(true);

        // Create SMTP transport with TLS by default
        let mailer = if let (Some(user), Some(pass)) = (username, password) {
            let creds = Credentials::new(user.to_string(), pass.to_string());
            SmtpTransport::relay(smtp_host)
                .map_err(|e| format!("Failed to create SMTP relay: {}", e))?
                .port(smtp_port)
                .credentials(creds)
                .build()
        } else if tls_verify {
            SmtpTransport::relay(smtp_host)
                .map_err(|e| format!("Failed to create SMTP relay: {}", e))?
                .port(smtp_port)
                .build()
        } else {
            tracing::warn!(
                "SMTP TLS verification disabled for channel '{}' - this is insecure",
                channel.name
            );
            SmtpTransport::builder_dangerous(smtp_host)
                .port(smtp_port)
                .build()
        };

        // Send the email via spawn_blocking to avoid blocking the async runtime
        let to_str = to.to_string();
        match tokio::task::spawn_blocking(move || mailer.send(&email)).await {
            Ok(Ok(_)) => {
                tracing::info!("Email sent successfully to {}", to_str);
            }
            Ok(Err(e)) => {
                return Err(format!("Failed to send email to {}: {}", to_str, e));
            }
            Err(e) => {
                return Err(format!("Email send task panicked: {}", e));
            }
        }
    }

    tracing::info!("All email notifications sent successfully");
    Ok(())
}

/// Send Slack notification (Webhook)
async fn send_slack_notification(
    client: &reqwest::Client,
    channel: &NotificationChannel,
    subject: &str,
    message: &str,
) -> Result<(), String> {
    let webhook_url = channel
        .config
        .get("webhook_url")
        .and_then(|v| v.as_str())
        .ok_or("Missing webhook_url in channel config")?;

    tracing::info!(
        "Sending Slack notification to webhook: {} (Subject: {})",
        webhook_url,
        subject
    );

    // Prepare Slack message payload
    let payload = serde_json::json!({
        "text": format!("*{}*\n{}", subject, message),
        "username": "zyvor-fabricd",
        "icon_emoji": ":robot_face:",
    });

    // Send HTTP POST to Slack webhook
    let response = client
        .post(webhook_url)
        .json(&payload)
        .send()
        .await
        .map_err(|e| format!("Failed to send Slack notification: {}", e))?;

    if !response.status().is_success() {
        return Err(format!(
            "Slack webhook returned error: {}",
            response.status()
        ));
    }

    tracing::info!("Slack notification sent successfully");
    Ok(())
}

/// Send webhook notification (Generic HTTP POST)
async fn send_webhook_notification(
    client: &reqwest::Client,
    channel: &NotificationChannel,
    subject: &str,
    message: &str,
) -> Result<(), String> {
    let webhook_url = channel
        .config
        .get("url")
        .and_then(|v| v.as_str())
        .ok_or("Missing url in channel config")?;

    // Re-validate URL at send time to catch DNS rebinding attacks
    validate_external_url(webhook_url)?;

    tracing::info!(
        "Sending webhook notification to: {} (Subject: {})",
        webhook_url,
        subject
    );

    // Prepare generic webhook payload
    let payload = serde_json::json!({
        "subject": subject,
        "message": message,
        "timestamp": Utc::now().to_rfc3339(),
        "source": "zyvor-fabricd",
    });

    // Send HTTP POST to webhook
    let response = client
        .post(webhook_url)
        .json(&payload)
        .send()
        .await
        .map_err(|e| format!("Failed to send webhook notification: {}", e))?;

    if !response.status().is_success() {
        return Err(format!("Webhook returned error: {}", response.status()));
    }

    tracing::info!("Webhook notification sent successfully");
    Ok(())
}

/// Send Microsoft Teams notification (Webhook)
async fn send_teams_notification(
    client: &reqwest::Client,
    channel: &NotificationChannel,
    subject: &str,
    message: &str,
) -> Result<(), String> {
    let webhook_url = channel
        .config
        .get("webhook_url")
        .and_then(|v| v.as_str())
        .ok_or("Missing webhook_url in channel config")?;

    tracing::info!(
        "Sending Teams notification to webhook: {} (Subject: {})",
        webhook_url,
        subject
    );

    // Prepare Teams message card payload
    let payload = serde_json::json!({
        "@type": "MessageCard",
        "@context": "https://schema.org/extensions",
        "summary": subject,
        "themeColor": "0078D7",
        "title": subject,
        "text": message,
    });

    // Send HTTP POST to Teams webhook
    let response = client
        .post(webhook_url)
        .json(&payload)
        .send()
        .await
        .map_err(|e| format!("Failed to send Teams notification: {}", e))?;

    if !response.status().is_success() {
        return Err(format!(
            "Teams webhook returned error: {}",
            response.status()
        ));
    }

    tracing::info!("Teams notification sent successfully");
    Ok(())
}

// ============================================================================
// Rule Evaluation / Dispatch
// ============================================================================

/// The frontend/rule `event_types` values are the same snake_case strings
/// VMEventType already serializes to (e.g. "started", "snapshot_created").
fn event_type_key(event_type: &super::events::VMEventType) -> String {
    serde_json::to_value(event_type)
        .ok()
        .and_then(|v| v.as_str().map(|s| s.to_string()))
        .unwrap_or_default()
}

/// VMEvent carries no severity of its own; derive a reasonable one so rules
/// can filter by severity_levels the same way they filter by event_types.
fn event_severity(event_type: &super::events::VMEventType) -> Severity {
    use super::events::VMEventType;
    match event_type {
        VMEventType::Error => Severity::Critical,
        VMEventType::AutoHealed | VMEventType::Deleted => Severity::Warning,
        _ => Severity::Info,
    }
}

/// Evaluate every enabled rule against a fired VM event and dispatch to its
/// channels on a match. Called fire-and-forget from record_event so it never
/// adds latency to the request path that triggered the event.
pub async fn dispatch_event_to_rules(state: &Arc<AppState>, event: &super::events::VMEvent) {
    let event_key = event_type_key(&event.event_type);
    let severity = event_severity(&event.event_type);

    let rules = match state
        .store
        .list_entities::<NotificationRule>("notifications/rules")
    {
        Ok(rules) => rules,
        Err(e) => {
            tracing::error!("Failed to load notification rules for dispatch: {}", e);
            return;
        }
    };

    for mut rule in rules {
        if !rule.enabled {
            continue;
        }
        if !rule.event_types.iter().any(|t| t == &event_key) {
            continue;
        }
        if !rule.severity_levels.contains(&severity) {
            continue;
        }
        if let Some(required_tags) = &rule.vm_tags {
            if !required_tags.is_empty() {
                let vm_tags = state
                    .store
                    .get_vm(&event.vm_name)
                    .ok()
                    .flatten()
                    .and_then(|vm| vm.tags)
                    .unwrap_or_default();
                if !required_tags.iter().any(|t| vm_tags.contains(t)) {
                    continue;
                }
            }
        }

        let message = event
            .detail
            .clone()
            .unwrap_or_else(|| format!("{} on VM '{}'", event_key, event.vm_name));
        let subject = format!("[{}] {} - {}", rule.name, event_key, event.vm_name);

        let mut any_success = false;
        for channel_id in &rule.channels {
            let channel = match state
                .store
                .get_entity::<NotificationChannel>("notifications/channels", channel_id)
            {
                Ok(Some(c)) if c.enabled => c,
                Ok(_) => continue,
                Err(e) => {
                    tracing::warn!(
                        "Failed to load channel {} for rule dispatch: {}",
                        channel_id,
                        e
                    );
                    continue;
                }
            };

            let result = send_notification(&state.http_client, &channel, &subject, &message).await;
            if result.is_ok() {
                any_success = true;
            } else if let Err(ref e) = result {
                tracing::warn!(
                    "Notification dispatch failed for rule '{}' channel '{}': {}",
                    rule.name,
                    channel.name,
                    e
                );
            }

            let history = NotificationHistory {
                id: Uuid::new_v4().to_string(),
                rule_id: rule.id.clone(),
                rule_name: rule.name.clone(),
                event_type: event_key.clone(),
                severity: severity.clone(),
                channel: channel.name.clone(),
                vm_name: Some(event.vm_name.clone()),
                message: message.clone(),
                sent_at: Utc::now(),
                status: if result.is_ok() {
                    NotificationStatus::Sent
                } else {
                    NotificationStatus::Failed
                },
                error: result.err(),
            };
            if let Err(e) = state
                .store
                .save_entity("notification_history", &history.id, &history)
            {
                tracing::error!("Failed to record notification history: {}", e);
            }
        }

        if any_success {
            rule.triggered_count += 1;
            rule.last_triggered = Some(Utc::now());
            if let Err(e) = state
                .store
                .save_entity("notifications/rules", &rule.id, &rule)
            {
                tracing::error!(
                    "Failed to update rule trigger stats for '{}': {}",
                    rule.name,
                    e
                );
            }
        }
    }
}
