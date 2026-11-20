// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: LicenseRef-Zyvor-Production-1.0

use anyhow::Result;
use serde::Deserialize;
use serde_json::Value;

#[derive(Debug, Clone, Deserialize)]
pub struct VMMetrics {
    #[serde(default)]
    pub cpu_usage: Option<f64>,
    #[serde(default)]
    pub memory_usage: Option<u64>,
    #[serde(default)]
    pub disk_usage: Option<u64>,
    #[serde(default)]
    pub network_rx: Option<u64>,
    #[serde(default)]
    pub network_tx: Option<u64>,
    #[serde(flatten)]
    pub raw: Value,
}

impl super::Client {
    /// VM resource metrics (`GET /api/vms/{name}/metrics`).
    pub async fn vm_metrics(&self, name: &str) -> Result<VMMetrics> {
        self.get(&format!("/api/vms/{name}/metrics")).await
    }
}
