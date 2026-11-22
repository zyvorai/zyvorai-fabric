// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: LicenseRef-Zyvor-Production-2.0

use anyhow::Result;
use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
pub struct LogEntry {
    pub timestamp: String,
    pub hostname: String,
    pub unit: String,
    pub message: String,
    pub priority: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct LogResponse {
    pub entries: Vec<LogEntry>,
    pub count: usize,
}

#[derive(Debug, Default)]
pub struct LogQuery<'a> {
    pub lines: Option<u32>,
    pub priority: Option<u8>,
    pub grep: Option<&'a str>,
}

impl super::Client {
    fn log_query_suffix(q: &LogQuery<'_>) -> String {
        let mut parts = Vec::new();
        if let Some(lines) = q.lines {
            parts.push(format!("lines={lines}"));
        }
        if let Some(priority) = q.priority {
            parts.push(format!("priority={priority}"));
        }
        if let Some(grep) = q.grep {
            parts.push(format!("grep={grep}"));
        }
        if parts.is_empty() {
            String::new()
        } else {
            format!("?{}", parts.join("&"))
        }
    }

    /// System journal logs (`GET /api/logs`).
    pub async fn system_logs(&self, query: &LogQuery<'_>) -> Result<LogResponse> {
        let path = format!("/api/logs{}", Self::log_query_suffix(query));
        self.get(&path).await
    }

    /// VM journal logs (`GET /api/vms/{name}/logs`).
    pub async fn vm_logs(&self, name: &str, query: &LogQuery<'_>) -> Result<LogResponse> {
        let path = format!("/api/vms/{name}/logs{}", Self::log_query_suffix(query));
        self.get(&path).await
    }
}
