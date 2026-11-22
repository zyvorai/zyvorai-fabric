// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: LicenseRef-Zyvor-Production-2.0

use axum::{http::StatusCode, Json};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::sync::Arc;

use crate::server::AppState;

// ============================================================================
// Webhook Retry with Exponential Backoff
// ============================================================================

/// Maximum payload size stored per delivery (4 KB).
const MAX_STORED_PAYLOAD_SIZE: usize = 4096;

/// Maximum retry attempts allowed.
const MAX_RETRY_ATTEMPTS: u32 = 10;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WebhookDelivery {
    pub id: String,
    pub channel_id: String,
    pub url: String,
    /// Truncated payload (first 4 KB). Full payload is not persisted.
    pub payload: String,
    pub attempt: u32,
    pub max_attempts: u32,
    pub status: DeliveryStatus,
    pub response_code: Option<u16>,
    pub error: Option<String>,
    pub next_retry: Option<DateTime<Utc>>,
    pub created: DateTime<Utc>,
    pub completed: Option<DateTime<Utc>>,
}

/// Summary view returned by list endpoint (payload omitted).
#[derive(Debug, Clone, Serialize)]
pub struct WebhookDeliverySummary {
    pub id: String,
    pub channel_id: String,
    pub attempt: u32,
    pub max_attempts: u32,
    pub status: DeliveryStatus,
    pub response_code: Option<u16>,
    pub error: Option<String>,
    pub next_retry: Option<DateTime<Utc>>,
    pub created: DateTime<Utc>,
    pub completed: Option<DateTime<Utc>>,
}

impl From<WebhookDelivery> for WebhookDeliverySummary {
    fn from(d: WebhookDelivery) -> Self {
        Self {
            id: d.id,
            channel_id: d.channel_id,
            attempt: d.attempt,
            max_attempts: d.max_attempts,
            status: d.status,
            response_code: d.response_code,
            error: d.error,
            next_retry: d.next_retry,
            created: d.created,
            completed: d.completed,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DeliveryStatus {
    Pending,
    Sending,
    Delivered,
    Retrying,
    Failed,
}

/// Calculate exponential backoff delay: 2^attempt * base_secs (capped at max_secs)
fn backoff_delay(attempt: u32, base_secs: u64, max_secs: u64) -> u64 {
    let delay = base_secs * 2u64.pow(attempt.min(10));
    delay.min(max_secs)
}

/// Send a webhook with retry logic. Called from notification system.
pub async fn send_webhook_with_retry(
    state: &Arc<AppState>,
    channel_id: &str,
    url: &str,
    payload: &str,
    max_attempts: u32,
) {
    let max_attempts = max_attempts.min(MAX_RETRY_ATTEMPTS);
    let delivery_id = uuid::Uuid::new_v4().to_string();

    // Truncate stored payload to limit storage usage
    let stored_payload = if payload.len() > MAX_STORED_PAYLOAD_SIZE {
        format!(
            "{}... (truncated, {} bytes total)",
            &payload[..MAX_STORED_PAYLOAD_SIZE],
            payload.len()
        )
    } else {
        payload.to_string()
    };

    let mut delivery = WebhookDelivery {
        id: delivery_id.clone(),
        channel_id: channel_id.to_string(),
        url: url.to_string(),
        payload: stored_payload,
        attempt: 0,
        max_attempts,
        status: DeliveryStatus::Pending,
        response_code: None,
        error: None,
        next_retry: None,
        created: Utc::now(),
        completed: None,
    };

    let body = payload.to_string();

    for attempt in 0..max_attempts {
        delivery.attempt = attempt + 1;
        delivery.status = DeliveryStatus::Sending;
        if let Err(e) = state
            .store
            .save_entity("webhook_deliveries", &delivery_id, &delivery)
        {
            tracing::error!("Store error: {}", e);
        }

        match state
            .http_client
            .post(url)
            .header("Content-Type", "application/json")
            .header("User-Agent", "zyvor-fabricd-webhook/1.0")
            .body(body.clone())
            .timeout(std::time::Duration::from_secs(10))
            .send()
            .await
        {
            Ok(response) => {
                let status_code = response.status().as_u16();
                delivery.response_code = Some(status_code);

                if response.status().is_success() {
                    delivery.status = DeliveryStatus::Delivered;
                    delivery.completed = Some(Utc::now());
                    if let Err(e) =
                        state
                            .store
                            .save_entity("webhook_deliveries", &delivery_id, &delivery)
                    {
                        tracing::error!("Store error: {}", e);
                    }
                    tracing::info!("Webhook delivered to {} (attempt {})", url, attempt + 1);
                    return;
                }

                // Non-success response — retry if retryable (5xx)
                if status_code >= 500 {
                    let delay = backoff_delay(attempt, 5, 300);
                    delivery.status = DeliveryStatus::Retrying;
                    delivery.error = Some(format!("HTTP {}", status_code));
                    delivery.next_retry =
                        Some(Utc::now() + chrono::Duration::seconds(delay as i64));
                    if let Err(e) =
                        state
                            .store
                            .save_entity("webhook_deliveries", &delivery_id, &delivery)
                    {
                        tracing::error!("Store error: {}", e);
                    }

                    tracing::warn!(
                        "Webhook {} returned {}, retrying in {}s (attempt {}/{})",
                        url,
                        status_code,
                        delay,
                        attempt + 1,
                        max_attempts
                    );
                    tokio::time::sleep(std::time::Duration::from_secs(delay)).await;
                    continue;
                }

                // 4xx — don't retry client errors
                delivery.status = DeliveryStatus::Failed;
                delivery.error = Some(format!("HTTP {} (not retryable)", status_code));
                delivery.completed = Some(Utc::now());
                if let Err(e) =
                    state
                        .store
                        .save_entity("webhook_deliveries", &delivery_id, &delivery)
                {
                    tracing::error!("Store error: {}", e);
                }
                tracing::warn!("Webhook {} returned {} (not retryable)", url, status_code);
                return;
            }
            Err(e) => {
                let delay = backoff_delay(attempt, 5, 300);
                delivery.status = DeliveryStatus::Retrying;
                delivery.error = Some(e.to_string());
                delivery.next_retry = Some(Utc::now() + chrono::Duration::seconds(delay as i64));
                if let Err(e) =
                    state
                        .store
                        .save_entity("webhook_deliveries", &delivery_id, &delivery)
                {
                    tracing::error!("Store error: {}", e);
                }

                tracing::warn!(
                    "Webhook {} failed: {}, retrying in {}s (attempt {}/{})",
                    url,
                    e,
                    delay,
                    attempt + 1,
                    max_attempts
                );
                tokio::time::sleep(std::time::Duration::from_secs(delay)).await;
            }
        }
    }

    // All attempts exhausted
    delivery.status = DeliveryStatus::Failed;
    delivery.error = Some(format!("All {} attempts failed", max_attempts));
    delivery.completed = Some(Utc::now());
    if let Err(e) = state
        .store
        .save_entity("webhook_deliveries", &delivery_id, &delivery)
    {
        tracing::error!("Store error: {}", e);
    }
    tracing::error!(
        "Webhook delivery to {} failed after {} attempts",
        url,
        max_attempts
    );
}

/// GET /api/webhooks/deliveries - List recent webhook deliveries (payload omitted)
pub async fn list_deliveries(
    security::RequireRead(_claims): security::RequireRead,
    axum::extract::State(state): axum::extract::State<Arc<AppState>>,
) -> Result<Json<Vec<WebhookDeliverySummary>>, (StatusCode, Json<serde_json::Value>)> {
    let deliveries = state
        .store
        .list_entities::<WebhookDelivery>("webhook_deliveries")
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"error": format!("Storage error: {}", e)})),
            )
        })?;
    let summaries: Vec<WebhookDeliverySummary> = deliveries.into_iter().map(Into::into).collect();
    Ok(Json(summaries))
}
