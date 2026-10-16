// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: Apache-2.0

//! Operator unwrap tokens for Keep 0.1 vault ceremony.
//!
//! When `ZYVOR_AGENT_VAULT_UNWRAP_REQUIRED=1`, credential resolve requires
//! `X-Merion-Unwrap-Token`. Secrets still come from **host env** — this is a
//! software gate, not user-held YubiKey / attested unwrap (Keep 0.2).

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
pub struct UnwrapTokenRecord {
    pub id: Uuid,
    pub token_sha256: String,
    pub scope: String,
    pub created_at: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
}

pub struct UnwrapTokenStore {
    path: PathBuf,
    lock: Mutex<()>,
}

impl UnwrapTokenStore {
    pub async fn open(root: impl AsRef<Path>) -> Result<Self> {
        let path = root.as_ref().join("unwrap-tokens.jsonl");
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).await?;
        }
        Ok(Self {
            path,
            lock: Mutex::new(()),
        })
    }

    pub async fn mint(&self, scope: &str, ttl_seconds: u64) -> Result<(String, UnwrapTokenRecord)> {
        let scope = scope.trim();
        if scope.is_empty() {
            bail!("scope is required");
        }
        if scope != "vault" && !scope.starts_with("vault:") {
            bail!("scope must be 'vault' or start with vault:");
        }
        let ttl = ttl_seconds.clamp(60, 86_400);
        let raw = format!("keep_unwrap_{}", Uuid::new_v4().simple());
        let record = UnwrapTokenRecord {
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

    pub async fn authorize(&self, raw_token: Option<&str>) -> Result<()> {
        let Some(raw) = raw_token.map(str::trim).filter(|s| !s.is_empty()) else {
            bail!(
                "vault unwrap required: present X-Merion-Unwrap-Token \
                 (mint via POST /v1/vault/unwrap-tokens). Secrets still come from host env \
                 (software-test); Keep 0.2 needs user-held unwrap on attested hardware."
            );
        };
        let hash = hex::encode(Sha256::digest(raw.as_bytes()));
        let raw_file = fs::read_to_string(&self.path).await.unwrap_or_default();
        let now = Utc::now();
        for line in raw_file.lines().filter(|l| !l.trim().is_empty()) {
            let Ok(rec) = serde_json::from_str::<UnwrapTokenRecord>(line) else {
                continue;
            };
            if rec.token_sha256 != hash {
                continue;
            }
            if rec.expires_at < now {
                bail!("unwrap token expired");
            }
            if rec.scope == "vault" || rec.scope.starts_with("vault:") {
                return Ok(());
            }
            bail!("unwrap token scope {:?} is not a vault scope", rec.scope);
        }
        bail!("unwrap token not recognized");
    }
}

/// Host-env secrets with an optional operator unwrap ceremony.
/// `UserHeldPending` labels the Keep 0.2 contract without claiming attestation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SecretBackendKind {
    HostEnv,
    /// Soft label when `ZYVOR_AGENT_VAULT_USER_HELD=1` — still host-env material.
    UserHeldPending,
}

impl SecretBackendKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::HostEnv => "host-env",
            Self::UserHeldPending => "user-held-pending",
        }
    }

    pub fn from_env() -> Self {
        if std::env::var("ZYVOR_AGENT_VAULT_USER_HELD").ok().as_deref() == Some("1") {
            Self::UserHeldPending
        } else {
            Self::HostEnv
        }
    }
}

/// Challenge for a future phone/YubiKey unwrap. Completing it without SNP/TDX
/// verified flags is refused — software scaffolding only.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserHeldChallenge {
    pub id: Uuid,
    pub nonce: String,
    pub created_at: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
}

pub struct UserHeldChallengeStore {
    path: PathBuf,
    lock: Mutex<()>,
}

impl UserHeldChallengeStore {
    pub async fn open(root: impl AsRef<Path>) -> Result<Self> {
        let path = root.as_ref().join("user-held-challenges.jsonl");
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).await?;
        }
        Ok(Self {
            path,
            lock: Mutex::new(()),
        })
    }

    pub async fn mint(&self, ttl_seconds: u64) -> Result<UserHeldChallenge> {
        let ttl = ttl_seconds.clamp(60, 3_600);
        let challenge = UserHeldChallenge {
            id: Uuid::new_v4(),
            nonce: format!("keep_uh_{}", Uuid::new_v4().simple()),
            created_at: Utc::now(),
            expires_at: Utc::now() + Duration::seconds(ttl as i64),
        };
        let _g = self.lock.lock().await;
        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.path)
            .await?;
        let mut line = serde_json::to_vec(&challenge)?;
        line.push(b'\n');
        file.write_all(&line).await?;
        file.sync_data().await?;
        Ok(challenge)
    }

    pub async fn take(&self, id: Uuid, nonce: &str) -> Result<UserHeldChallenge> {
        let raw_file = fs::read_to_string(&self.path).await.unwrap_or_default();
        let now = Utc::now();
        for line in raw_file.lines().filter(|l| !l.trim().is_empty()) {
            let Ok(rec) = serde_json::from_str::<UserHeldChallenge>(line) else {
                continue;
            };
            if rec.id != id {
                continue;
            }
            if rec.nonce != nonce {
                bail!("user-held challenge nonce mismatch");
            }
            if rec.expires_at < now {
                bail!("user-held challenge expired");
            }
            return Ok(rec);
        }
        bail!("user-held challenge not found");
    }
}

/// Refuse attested unlock until FluxVM reports verified SNP/TDX launch.
pub fn user_held_complete_allowed(snp_verified: bool, tdx_verified: bool) -> Result<()> {
    if snp_verified || tdx_verified {
        Ok(())
    } else {
        bail!(
            "user-held unwrap complete refused: FluxVM snp_launch_verified / \
             tdx_launch_verified are false (Keep 0.2 hardware gate). Challenge \
             minted for phone/YubiKey ceremony design only; secrets remain host-env."
        )
    }
}

pub fn unwrap_required_from_env() -> bool {
    std::env::var("ZYVOR_AGENT_VAULT_UNWRAP_REQUIRED")
        .ok()
        .as_deref()
        == Some("1")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn mint_and_authorize() {
        let dir = tempfile::tempdir().unwrap();
        let store = UnwrapTokenStore::open(dir.path()).await.unwrap();
        let (raw, _) = store.mint("vault", 600).await.unwrap();
        store.authorize(Some(&raw)).await.unwrap();
        assert!(store.authorize(None).await.is_err());
        assert!(store.authorize(Some("bogus")).await.is_err());
    }

    #[tokio::test]
    async fn user_held_challenge_roundtrip_and_gate() {
        let dir = tempfile::tempdir().unwrap();
        let store = UserHeldChallengeStore::open(dir.path()).await.unwrap();
        let c = store.mint(300).await.unwrap();
        store.take(c.id, &c.nonce).await.unwrap();
        assert!(user_held_complete_allowed(false, false).is_err());
        assert!(user_held_complete_allowed(true, false).is_ok());
    }
}
