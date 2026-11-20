// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: LicenseRef-Zyvor-Production-1.0

use anyhow::{Context, Result};
use eventsource_stream::Eventsource;
use futures_util::StreamExt;
use serde::{Deserialize, Serialize};
use std::pin::Pin;

/// Live VM lifecycle event from `GET /api/events/stream` (SSE).
pub type EventStream = Pin<Box<dyn futures_util::Stream<Item = Result<VMEvent>> + Send>>;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VMEvent {
    pub id: String,
    pub event_type: String,
    pub vm_name: String,
    pub detail: Option<String>,
    pub timestamp: String,
}

impl super::Client {
    /// Recent VM lifecycle events (`GET /api/events`).
    pub async fn list_events(&self) -> Result<Vec<VMEvent>> {
        self.get("/api/events").await
    }

    /// Open SSE stream (`GET /api/events/stream`). Requires auth when enabled.
    pub async fn stream_events(&self) -> Result<EventStream> {
        let resp = self
            .request(reqwest::Method::GET, "/api/events/stream")
            .header(reqwest::header::ACCEPT, "text/event-stream")
            .send()
            .await?
            .error_for_status()
            .context("failed to open /api/events/stream")?;

        let byte_stream = resp.bytes_stream().eventsource();
        let mapped = byte_stream.filter_map(|item| async move {
            match item {
                Ok(ev) => {
                    let data = ev.data;
                    if data.is_empty() {
                        return None;
                    }
                    match serde_json::from_str::<VMEvent>(&data) {
                        Ok(vm_event) => Some(Ok(vm_event)),
                        Err(e) => Some(Err(e.into())),
                    }
                }
                Err(e) => Some(Err(e.into())),
            }
        });

        Ok(Box::pin(mapped))
    }
}
