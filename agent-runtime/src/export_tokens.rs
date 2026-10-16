// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: Apache-2.0

//! Scoped trajectory/audit export tokens. Training default off: without a
//! matching token, journal/trajectory export is refused.

use anyhow::{bail, Result};
use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};
use tokio::{
    fs::{self, OpenOptions},
    io::AsyncWriteExt,
    sync::Mutex,
};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExportTokenRecord {
    pub id: Uuid,
    pub token_sha256: String,
    pub scope: String,
    pub created_at: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
}

pub struct ExportTokenStore {
    path: PathBuf,
    lock: Mutex<()>,
}

impl ExportTokenStore {
    pub async fn open(root: impl AsRef<Path>) -> Result<Self> {
        let path = root.as_ref().join("export-tokens.jsonl");
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).await?;
        }
        Ok(Self {
            path,
            lock: Mutex::new(()),
        })
    }

    pub async fn mint(&self, scope: &str, ttl_seconds: u64) -> Result<(String, ExportTokenRecord)> {
        let scope = scope.trim();
        if scope.is_empty() {
            bail!("scope is required");
        }
        if !scope.starts_with("trajectory:") && !scope.starts_with("audit:") && scope != "pack" {
            bail!("scope must start with trajectory:, audit:, or be 'pack'");
        }
        let ttl = ttl_seconds.clamp(60, 86_400);
        let raw = format!("keep_export_{}", Uuid::new_v4().simple());
        let record = ExportTokenRecord {
            id: Uuid::new_v4(),
            token_sha256: hex::encode(Sha256::digest(raw.as_bytes())),
            scope: scope.to_string(),
            created_at: Utc::now(),
            expires_at: Utc::now() + Duration::seconds(ttl as i64),
        };
        let _g = self.lock.lock().await;
        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.path)
            .await?;
        let mut line = serde_json::to_vec(&record)?;
        line.push(b'\n');
        file.write_all(&line).await?;
        file.sync_data().await?;
        Ok((raw, record))
    }

    pub async fn authorize(&self, raw_token: Option<&str>, needed_prefix: &str) -> Result<()> {
        let Some(raw) = raw_token.map(str::trim).filter(|s| !s.is_empty()) else {
            bail!(
                "export refused: present X-Merion-Export-Token (mint via POST /v1/export-tokens). Training/trajectory export is off by default."
            );
        };
        let hash = hex::encode(Sha256::digest(raw.as_bytes()));
        let raw_file = fs::read_to_string(&self.path).await.unwrap_or_default();
        let now = Utc::now();
        for line in raw_file.lines().filter(|l| !l.trim().is_empty()) {
            let Ok(rec) = serde_json::from_str::<ExportTokenRecord>(line) else {
                continue;
            };
            if rec.token_sha256 != hash {
                continue;
            }
            if rec.expires_at < now {
                bail!("export token expired");
            }
            if rec.scope == "pack"
                || rec.scope == needed_prefix
                || rec.scope.starts_with(&format!("{needed_prefix}:"))
                || (needed_prefix == "audit" && rec.scope.starts_with("trajectory:"))
            {
                return Ok(());
            }
            bail!(
                "export token scope {:?} does not allow {needed_prefix}",
                rec.scope
            );
        }
        bail!("export token not recognized");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn mint_and_authorize() {
        let dir = tempfile::tempdir().unwrap();
        let store = ExportTokenStore::open(dir.path()).await.unwrap();
        let (raw, _) = store.mint("trajectory:read:7d", 600).await.unwrap();
        store.authorize(Some(&raw), "trajectory").await.unwrap();
        store.authorize(Some(&raw), "audit").await.unwrap();
        assert!(store.authorize(None, "trajectory").await.is_err());
        assert!(store.authorize(Some("nope"), "trajectory").await.is_err());
    }
}
