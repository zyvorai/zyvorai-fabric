// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: Apache-2.0

//! Receipts for approved actions, and at-most-once for an agent that retries.
//!
//! When the egress broker performs a request that a person had to approve (a credential with `requires_approval`, a secret-shaped request, a
//! write from a tainted session), it records a **receipt**: who, which agent and session, which credential, the method, the URL without its
//! query, the size and digest of the body (never the body), the approval that allowed it, and the status the upstream answered. Receipts are
//! append-only on the host (`receipts.jsonl`), a person sees their own (`GET /v1/receipts`), and each one is also journaled
//! (`merion.action.performed`, without any body).
//!
//! An agent that sets an `Idempotency-Key` header on such a request gets **at-most-once** behaviour, which matters when a retry (a goal
//! step tried again, a network timeout) would otherwise send the same email twice:
//! * the first request with a key is held for approval as usual, sent, and its receipt kept;
//! * the same key with the **same** request again is **not sent and not asked about again**: the broker answers with the recorded status and a
//!   body saying it was a replay (the original response body is not kept);
//! * the same key with a **different** request is refused (409), like a payment API would;
//! * the same key while the first request is still in flight is refused (409).
//!
//! A key is scoped to the person (or the operator) and the credential, not to the session, because a retry usually is a new session. A request
//! that failed on the network or got a 5xx is not recorded, so it can be retried; the key is also forwarded upstream, so an API that
//! supports idempotency keys de-duplicates on its side too. This applies to the JSON egress broker (`ctx.fetch`), not to the CONNECT proxy.

use crate::{app::ApiError, authz::Principal, AppState};
use anyhow::{Context, Result};
use axum::{
    extract::{Query, State},
    Extension, Json,
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    collections::{HashMap, HashSet},
    path::PathBuf,
    sync::{Arc, Mutex as StdMutex},
};
use tokio::{fs, io::AsyncWriteExt, sync::RwLock};
use uuid::Uuid;

/// How many receipts are kept in memory for lookups and listing (the file keeps them all).
const KEEP_IN_MEMORY: usize = 10_000;
/// The longest idempotency key accepted.
pub const MAX_KEY_LEN: usize = 128;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Receipt {
    pub id: Uuid,
    pub at: DateTime<Utc>,
    pub user_id: Option<String>,
    pub session_id: Uuid,
    pub agent: String,
    pub credential: Option<String>,
    pub method: String,
    /// The URL without its query string, fragment or credentials.
    pub url: String,
    pub body_bytes: usize,
    pub body_sha256: String,
    pub approval_id: Option<Uuid>,
    pub idempotency_key: Option<String>,
    /// What the upstream answered.
    pub status: u16,
    /// Digest of the whole request (method, URL with query, body), used to tell a replay from a different request under the same key.
    #[serde(default)]
    pub fingerprint: String,
}

/// What to do with a request that carries an idempotency key.
#[derive(Debug)]
pub enum Begin {
    /// Send it. Drop the guard when done (it releases the key for a concurrent request).
    Proceed(InFlight),
    /// The same request was already performed: answer from this receipt.
    Replay(Box<Receipt>),
    /// The key belongs to a different request, or the first is still running.
    Conflict(String),
}

pub struct ReceiptStore {
    file: PathBuf,
    recent: RwLock<Vec<Receipt>>,
    by_key: RwLock<HashMap<String, Receipt>>,
    in_flight: Arc<StdMutex<HashSet<String>>>,
    /// Held while the file is appended to or rewritten, so a purge cannot drop a receipt recorded meanwhile.
    file_lock: tokio::sync::Mutex<()>,
}

/// Holds an idempotency key while its request is in flight.
#[derive(Debug)]
pub struct InFlight {
    key: String,
    set: Arc<StdMutex<HashSet<String>>>,
}

impl Drop for InFlight {
    fn drop(&mut self) {
        if let Ok(mut set) = self.set.lock() {
            set.remove(&self.key);
        }
    }
}

/// The key a receipt is looked up by: the person (or operator) and credential, and the agent's key. Hashed, so the file holds no raw scope.
pub fn scope_key(user: Option<&str>, credential: Option<&str>, key: &str) -> String {
    let mut h = Sha256::new();
    for part in [user.unwrap_or("\0operator"), credential.unwrap_or(""), key] {
        h.update(part.as_bytes());
        h.update([0]);
    }
    hex::encode(h.finalize())
}

/// The digest that makes one request different from another under the same key.
pub fn fingerprint(method: &str, url_with_query: &str, body_sha256: &str) -> String {
    let mut h = Sha256::new();
    for part in [method, url_with_query, body_sha256] {
        h.update(part.as_bytes());
        h.update([0]);
    }
    hex::encode(h.finalize())
}

/// A valid idempotency key: 1 to [`MAX_KEY_LEN`] printable ASCII characters, no spaces.
pub fn valid_key(key: &str) -> bool {
    !key.is_empty() && key.len() <= MAX_KEY_LEN && key.bytes().all(|b| b.is_ascii_graphic())
}

impl ReceiptStore {
    pub async fn open(root: impl AsRef<std::path::Path>) -> Result<Self> {
        let dir = root.as_ref().to_path_buf();
        fs::create_dir_all(&dir).await?;
        let file = dir.join("receipts.jsonl");
        let mut recent = Vec::new();
        let mut by_key = HashMap::new();
        if let Ok(raw) = fs::read_to_string(&file).await {
            for line in raw.lines().filter(|l| !l.trim().is_empty()) {
                let r: Receipt = serde_json::from_str(line)
                    .with_context(|| format!("decoding {}", file.display()))?;
                if let Some(k) = &r.idempotency_key {
                    by_key.insert(
                        scope_key(r.user_id.as_deref(), r.credential.as_deref(), k),
                        r.clone(),
                    );
                }
                recent.push(r);
            }
        }
        if recent.len() > KEEP_IN_MEMORY {
            recent.drain(..recent.len() - KEEP_IN_MEMORY);
        }
        Ok(Self {
            file,
            recent: RwLock::new(recent),
            by_key: RwLock::new(by_key),
            in_flight: Arc::default(),
            file_lock: tokio::sync::Mutex::new(()),
        })
    }

    /// Decide what to do with a keyed request. `fp` is its [`fingerprint`].
    pub async fn begin(&self, scope: &str, fp: &str) -> Begin {
        if let Some(found) = self.by_key.read().await.get(scope) {
            return if found.fingerprint == fp {
                Begin::Replay(Box::new(found.clone()))
            } else {
                Begin::Conflict(
                    "this idempotency key was already used for a different request".into(),
                )
            };
        }
        let took_slot = {
            let mut set = self.in_flight.lock().unwrap_or_else(|e| e.into_inner());
            set.insert(scope.to_string())
        };
        if !took_slot {
            return Begin::Conflict(
                "a request with this idempotency key is already in progress".into(),
            );
        }
        // a receipt may have been recorded between the lookup above and taking the slot
        if let Some(found) = self.by_key.read().await.get(scope) {
            self.in_flight
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .remove(scope);
            return if found.fingerprint == fp {
                Begin::Replay(Box::new(found.clone()))
            } else {
                Begin::Conflict(
                    "this idempotency key was already used for a different request".into(),
                )
            };
        }
        Begin::Proceed(InFlight {
            key: scope.to_string(),
            set: self.in_flight.clone(),
        })
    }

    /// Records a receipt (append-only) and, when it has a key, makes it the answer to any repeat of that request.
    pub async fn record(&self, r: Receipt) -> Result<()> {
        let mut line = serde_json::to_vec(&r)?;
        line.push(b'\n');
        let _file = self.file_lock.lock().await;
        let mut f = fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.file)
            .await?;
        f.write_all(&line).await?;
        f.flush().await?;
        if let Some(k) = &r.idempotency_key {
            self.by_key.write().await.insert(
                scope_key(r.user_id.as_deref(), r.credential.as_deref(), k),
                r.clone(),
            );
        }
        let mut recent = self.recent.write().await;
        recent.push(r);
        if recent.len() > KEEP_IN_MEMORY {
            recent.remove(0);
        }
        Ok(())
    }

    /// Forgets receipts made before `cutoff` (from the file, the recent list and the key index). Returns how many. A forgotten receipt no
    /// longer answers a repeat of its idempotency key: that key can be used again.
    pub async fn purge_before(&self, cutoff: DateTime<Utc>) -> Result<usize> {
        let _file = self.file_lock.lock().await;
        let raw = fs::read_to_string(&self.file).await.unwrap_or_default();
        let mut kept = String::new();
        let mut removed = 0usize;
        for line in raw.lines().filter(|l| !l.trim().is_empty()) {
            let r: Receipt = serde_json::from_str(line)
                .with_context(|| format!("decoding {}", self.file.display()))?;
            if r.at < cutoff {
                removed += 1;
            } else {
                kept.push_str(line);
                kept.push('\n');
            }
        }
        if removed == 0 {
            return Ok(0);
        }
        crate::store::atomic_write(self.file.clone(), kept.as_bytes()).await?;
        self.recent.write().await.retain(|r| r.at >= cutoff);
        self.by_key.write().await.retain(|_, r| r.at >= cutoff);
        Ok(removed)
    }

    /// Newest first. `user` limits to one person's receipts; `None` is everyone's.
    pub async fn list(&self, user: Option<&str>, limit: usize) -> Vec<Receipt> {
        self.recent
            .read()
            .await
            .iter()
            .rev()
            .filter(|r| user.is_none_or(|u| r.user_id.as_deref() == Some(u)))
            .take(limit)
            .cloned()
            .collect()
    }
}

/// The answer to a replayed request: the recorded status, a note that it was not sent again, and no original body.
pub fn replay_response(r: &Receipt) -> Value {
    let note = json!({
        "replayed": true,
        "receipt_id": r.id,
        "original_status": r.status,
        "note": "This request was already performed; it was not sent again. The original response body is not kept.",
    });
    json!({
        "status": r.status,
        "headers": { "x-keep-replayed": "true", "x-keep-receipt": r.id.to_string(), "content-type": "application/json" },
        "header_list": [["x-keep-replayed", "true"], ["x-keep-receipt", r.id.to_string()], ["content-type", "application/json"]],
        "body_base64": base64::Engine::encode(&base64::engine::general_purpose::STANDARD, note.to_string()),
        "replayed": true,
    })
}

#[derive(Debug, Deserialize)]
pub struct ReceiptsQuery {
    #[serde(default)]
    pub limit: Option<usize>,
    /// The operator may narrow to one person; a user token always sees only its own.
    #[serde(default)]
    pub user_id: Option<String>,
}

/// `GET /v1/receipts`: a person's own receipts (newest first), or, for the operator, everyone's or one person's.
pub(crate) async fn list_receipts(
    State(state): State<Arc<AppState>>,
    Extension(principal): Extension<Principal>,
    Query(q): Query<ReceiptsQuery>,
) -> Result<Json<Value>, ApiError> {
    let user = principal.user().or(q.user_id.as_deref());
    let items = state
        .store
        .receipts
        .list(user, q.limit.unwrap_or(100).clamp(1, 500))
        .await;
    Ok(Json(json!({ "items": items })))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn receipt(user: Option<&str>, key: Option<&str>, fp: &str, status: u16) -> Receipt {
        Receipt {
            id: Uuid::new_v4(),
            at: Utc::now(),
            user_id: user.map(str::to_string),
            session_id: Uuid::new_v4(),
            agent: "mail".into(),
            credential: Some("mail".into()),
            method: "POST".into(),
            url: "https://mail.example/send".into(),
            body_bytes: 5,
            body_sha256: "ab".into(),
            approval_id: Some(Uuid::new_v4()),
            idempotency_key: key.map(str::to_string),
            status,
            fingerprint: fp.into(),
        }
    }

    async fn store() -> (ReceiptStore, tempfile::TempDir) {
        let d = tempfile::tempdir().unwrap();
        (ReceiptStore::open(d.path()).await.unwrap(), d)
    }

    #[test]
    fn keys_are_validated_and_scoped_to_the_person_and_credential() {
        assert!(valid_key("goal-1:step-2:attempt") && valid_key(&"k".repeat(MAX_KEY_LEN)));
        for bad in ["", "has space", "tab\t", "é", &"k".repeat(MAX_KEY_LEN + 1)] {
            assert!(!valid_key(bad), "{bad:?}");
        }
        let a = scope_key(Some("ana"), Some("mail"), "k1");
        assert_eq!(a, scope_key(Some("ana"), Some("mail"), "k1"));
        assert_ne!(
            a,
            scope_key(Some("ben"), Some("mail"), "k1"),
            "another person's key is a different key"
        );
        assert_ne!(
            a,
            scope_key(Some("ana"), Some("bank"), "k1"),
            "another credential too"
        );
        assert_ne!(a, scope_key(None, Some("mail"), "k1"), "and the operator's");
        assert_ne!(
            fingerprint("POST", "https://x/y", "a"),
            fingerprint("POST", "https://x/y", "b")
        );
        assert_ne!(
            fingerprint("POST", "https://x/y?a=1", "a"),
            fingerprint("POST", "https://x/y?a=2", "a"),
            "the query is part of the request"
        );
    }

    #[tokio::test]
    async fn the_first_request_proceeds_a_repeat_replays_and_a_different_one_conflicts() {
        let (s, _d) = store().await;
        let scope = scope_key(Some("ana"), Some("mail"), "k");
        let Begin::Proceed(guard) = s.begin(&scope, "fp1").await else {
            panic!("the first should proceed")
        };
        // while it runs, the same key is refused, whether it is the same request or not
        assert!(
            matches!(s.begin(&scope, "fp1").await, Begin::Conflict(m) if m.contains("in progress"))
        );
        s.record(receipt(Some("ana"), Some("k"), "fp1", 200))
            .await
            .unwrap();
        drop(guard);
        let Begin::Replay(r) = s.begin(&scope, "fp1").await else {
            panic!("the same request replays")
        };
        assert_eq!(r.status, 200);
        assert!(
            matches!(s.begin(&scope, "fp2").await, Begin::Conflict(m) if m.contains("different request"))
        );
        // another person with the same key string is independent
        assert!(matches!(
            s.begin(&scope_key(Some("ben"), Some("mail"), "k"), "fp1")
                .await,
            Begin::Proceed(_)
        ));
    }

    #[tokio::test]
    async fn a_request_that_was_not_recorded_can_be_tried_again() {
        let (s, _d) = store().await;
        let scope = scope_key(None, Some("mail"), "k");
        {
            let Begin::Proceed(_g) = s.begin(&scope, "fp").await else {
                panic!()
            };
            // it failed: nothing recorded, the guard drops
        }
        assert!(
            matches!(s.begin(&scope, "fp").await, Begin::Proceed(_)),
            "a failed attempt does not burn the key"
        );
    }

    #[tokio::test]
    async fn receipts_survive_a_restart_and_are_listed_per_person_newest_first() {
        let d = tempfile::tempdir().unwrap();
        let s = ReceiptStore::open(d.path()).await.unwrap();
        s.record(receipt(Some("ana"), Some("k"), "fp", 200))
            .await
            .unwrap();
        s.record(receipt(Some("ben"), None, "fp2", 201))
            .await
            .unwrap();
        s.record(receipt(Some("ana"), None, "fp3", 202))
            .await
            .unwrap();
        drop(s);
        let s = ReceiptStore::open(d.path()).await.unwrap();
        let ana = s.list(Some("ana"), 10).await;
        assert_eq!(
            ana.iter().map(|r| r.status).collect::<Vec<_>>(),
            [202, 200],
            "newest first, only hers"
        );
        assert_eq!(s.list(None, 10).await.len(), 3);
        assert_eq!(s.list(Some("ana"), 1).await.len(), 1);
        assert!(
            matches!(
                s.begin(&scope_key(Some("ana"), Some("mail"), "k"), "fp")
                    .await,
                Begin::Replay(_)
            ),
            "the key still replays after a restart"
        );
    }

    #[test]
    fn a_replay_says_so_and_carries_no_original_body() {
        let r = receipt(Some("ana"), Some("k"), "fp", 201);
        let v = replay_response(&r);
        assert_eq!(v["status"], 201);
        assert_eq!(v["headers"]["x-keep-replayed"], "true");
        let body = String::from_utf8(
            base64::Engine::decode(
                &base64::engine::general_purpose::STANDARD,
                v["body_base64"].as_str().unwrap(),
            )
            .unwrap(),
        )
        .unwrap();
        let body: Value = serde_json::from_str(&body).unwrap();
        assert_eq!(
            (body["replayed"].clone(), body["original_status"].clone()),
            (json!(true), json!(201))
        );
    }
}
