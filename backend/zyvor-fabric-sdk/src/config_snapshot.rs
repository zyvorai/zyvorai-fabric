// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: LicenseRef-Zyvor-Production-2.0

use anyhow::Result;
use serde::Deserialize;
use serde_json::Value;

#[derive(Debug, Clone, Deserialize)]
pub struct ConfigSnapshot {
    pub version: String,
    pub exported_at: String,
    pub vms: Value,
    pub network_policies: Value,
    pub storage_pools: Value,
    pub recent_events_count: usize,
}

#[derive(Debug, Clone, Deserialize)]
pub struct EventRetentionConfig {
    pub max_events: usize,
}

impl super::Client {
    /// Export config snapshot (`GET /api/config/snapshot`).
    pub async fn config_snapshot(&self) -> Result<ConfigSnapshot> {
        self.get("/api/config/snapshot").await
    }

    /// Event retention settings (`GET /api/events/retention`).
    pub async fn event_retention(&self) -> Result<EventRetentionConfig> {
        self.get("/api/events/retention").await
    }
}
