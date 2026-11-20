// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: LicenseRef-Zyvor-Production-1.0

use anyhow::Result;
use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
pub struct MeResponse {
    pub id: String,
    pub username: String,
    pub role: String,
}

impl super::Client {
    /// Current user (`GET /api/auth/me`).
    pub async fn me(&self) -> Result<MeResponse> {
        self.get("/api/auth/me").await
    }
}
