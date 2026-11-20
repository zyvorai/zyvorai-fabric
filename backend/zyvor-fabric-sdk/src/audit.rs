// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: LicenseRef-Zyvor-Production-1.0

use anyhow::Result;
use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
pub struct AuditLogEntry {
    pub id: Option<String>,
    pub timestamp: Option<String>,
    pub action: Option<String>,
    pub resource_type: Option<String>,
    pub detail: Option<String>,
    pub user: Option<String>,
}

impl super::Client {
    /// Audit log entries (`GET /api/audit/logs`).
    pub async fn audit_logs(&self) -> Result<Vec<AuditLogEntry>> {
        self.get("/api/audit/logs").await
    }
}
