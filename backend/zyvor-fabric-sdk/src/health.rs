// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: LicenseRef-Zyvor-Production-1.0

use anyhow::Result;

impl super::Client {
    /// Liveness check (`GET /health`).
    pub async fn health(&self) -> Result<String> {
        self.get_text("/health").await
    }
}
