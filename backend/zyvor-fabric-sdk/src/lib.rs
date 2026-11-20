// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: LicenseRef-Zyvor-Production-1.0

//! Zyvor Fabric SDK — Rust client for the zyvor-fabricd REST API.

use anyhow::Result;
use serde::Serialize;

pub mod audit;
pub mod auth;
pub mod auth_client;
pub mod config_snapshot;
pub mod events;
pub mod health;
pub mod logs;
pub mod metrics;
pub mod network;
pub mod storage;
pub mod vms;

pub use audit::AuditLogEntry;
pub use auth_client::MeResponse;
pub use config_snapshot::{ConfigSnapshot, EventRetentionConfig};
pub use events::{EventStream, VMEvent};
pub use logs::{LogEntry, LogQuery, LogResponse};
pub use metrics::VMMetrics;
pub use network::NetworkPolicySummary;
pub use storage::StoragePool;

/// Client configuration.
pub struct ClientConfig {
    pub endpoint: String,
    pub token: Option<String>,
}

/// Zyvor Fabric (`zyvor-fabricd`) API client.
pub struct Client {
    http: reqwest::Client,
    base_url: String,
    token: Option<String>,
}

impl Client {
    /// Create a new client.
    pub fn new(config: ClientConfig) -> Result<Self> {
        let http = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(30))
            .build()?;
        Ok(Self {
            http,
            base_url: config.endpoint.trim_end_matches('/').to_string(),
            token: config.token,
        })
    }

    /// Authenticate and set the token.
    pub async fn login(&mut self, username: &str, password: &str) -> Result<String> {
        let resp: serde_json::Value = self
            .http
            .post(format!("{}/api/auth/login", self.base_url))
            .json(&serde_json::json!({"username": username, "password": password}))
            .send()
            .await?
            .error_for_status()?
            .json()
            .await?;
        let token = resp["token"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("No token in response"))?
            .to_string();
        self.token = Some(token.clone());
        Ok(token)
    }

    /// Build an authenticated request.
    fn request(&self, method: reqwest::Method, path: &str) -> reqwest::RequestBuilder {
        let mut req = self
            .http
            .request(method, format!("{}{}", self.base_url, path));
        if let Some(ref token) = self.token {
            req = req.bearer_auth(token);
        }
        req
    }

    /// GET request returning JSON.
    pub async fn get<T: serde::de::DeserializeOwned>(&self, path: &str) -> Result<T> {
        let resp = self
            .request(reqwest::Method::GET, path)
            .send()
            .await?
            .error_for_status()?
            .json()
            .await?;
        Ok(resp)
    }

    /// POST request with JSON body.
    pub async fn post<T: serde::de::DeserializeOwned, B: Serialize>(
        &self,
        path: &str,
        body: &B,
    ) -> Result<T> {
        let resp = self
            .request(reqwest::Method::POST, path)
            .json(body)
            .send()
            .await?
            .error_for_status()?
            .json()
            .await?;
        Ok(resp)
    }

    /// PUT request with JSON body.
    pub async fn put<T: serde::de::DeserializeOwned, B: Serialize>(
        &self,
        path: &str,
        body: &B,
    ) -> Result<T> {
        let resp = self
            .request(reqwest::Method::PUT, path)
            .json(body)
            .send()
            .await?
            .error_for_status()?
            .json()
            .await?;
        Ok(resp)
    }

    /// GET request returning plain text (e.g. /health).
    pub async fn get_text(&self, path: &str) -> Result<String> {
        let resp = self
            .request(reqwest::Method::GET, path)
            .send()
            .await?
            .error_for_status()?
            .text()
            .await?;
        Ok(resp)
    }

    /// DELETE request.
    pub async fn delete(&self, path: &str) -> Result<()> {
        self.request(reqwest::Method::DELETE, path)
            .send()
            .await?
            .error_for_status()?;
        Ok(())
    }
}
