// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: Apache-2.0

//! A person's own connections to outside accounts (Google, say).
//!
//! A credential whose descriptor sets `oauth.connection` (see [`crate::credentials`]) is **per person**: the refresh token is the one *that
//! person* stored here, so their agent reads *their* mail and calendar and nobody else's. The person gets the refresh token once, on their own
//! machine, with `scripts/merion-google-auth.py`, and stores it with their own token (`PUT /v1/connections/{name}`).
//!
//! * **Write-only.** The refresh token is never returned, listed, logged or journaled. `GET /v1/connections` says only whether each connection
//!   is set, and since when.
//! * **Host files.** One file per person under `connections/`, mode 0600. Like the vault, the operator of the host can read them; they are not
//!   encrypted to the person.
//! * **Not in a cell.** The host mints short-lived access tokens from it and injects them at the egress broker; the agent only ever sees the
//!   response. Disconnecting deletes the token and the cached access tokens at once.
//! * **Only names the host offers.** A connection can be set only if some credential in the vault asks for that name.
//! * Every route is scoped to the caller. The operator must name `user_id` and every operator access is journaled (`keep.connection.operator_access`).

use crate::{
    app::ApiError, audit::AuditPhase, authz::Principal, model::validate_user_id, AppState,
};
use anyhow::{Context, Result};
use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    Extension, Json,
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{collections::HashMap, path::PathBuf, sync::Arc};
use tokio::{
    fs,
    io::AsyncWriteExt,
    sync::{Mutex, RwLock},
};

/// The longest refresh token accepted.
pub const MAX_TOKEN_LEN: usize = 4096;
/// The shortest (a real refresh token is far longer; this refuses obvious placeholders).
pub const MIN_TOKEN_LEN: usize = 10;

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Stored {
    refresh_token: String,
    connected_at: DateTime<Utc>,
}

pub struct ConnectionStore {
    root: PathBuf,
    users: RwLock<HashMap<String, HashMap<String, Stored>>>,
    write: Mutex<()>,
}

/// A refresh token that looks like one: printable, no spaces, within bounds.
pub fn valid_token(t: &str) -> bool {
    (MIN_TOKEN_LEN..=MAX_TOKEN_LEN).contains(&t.len()) && t.bytes().all(|b| b.is_ascii_graphic())
}

impl ConnectionStore {
    pub async fn open(root: impl AsRef<std::path::Path>) -> Result<Self> {
        let root = root.as_ref().to_path_buf();
        fs::create_dir_all(&root).await?;
        let mut users = HashMap::new();
        let mut entries = fs::read_dir(&root).await?;
        while let Some(entry) = entries.next_entry().await? {
            let path = entry.path();
            let Some(name) = path
                .file_name()
                .and_then(|n| n.to_str())
                .and_then(|n| n.strip_suffix(".json"))
            else {
                continue;
            };
            if validate_user_id(name).is_err() {
                continue;
            }
            let raw = fs::read(&path).await?;
            let map: HashMap<String, Stored> = serde_json::from_slice(&raw)
                .with_context(|| format!("decoding {}", path.display()))?;
            users.insert(name.to_string(), map);
        }
        Ok(Self {
            root,
            users: RwLock::new(users),
            write: Mutex::new(()),
        })
    }

    /// Writes the person's file atomically with mode 0600 (it holds refresh tokens).
    async fn save(&self, user: &str, map: &HashMap<String, Stored>) -> Result<()> {
        let path = self.root.join(format!("{user}.json"));
        let tmp = self
            .root
            .join(format!("{user}.json.tmp-{}", uuid::Uuid::new_v4()));
        let mut options = fs::OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        options.mode(0o600);
        let mut file = options.open(&tmp).await?;
        file.write_all(&serde_json::to_vec_pretty(map)?).await?;
        file.flush().await?;
        drop(file);
        fs::rename(&tmp, &path).await?;
        Ok(())
    }

    /// Stores (or replaces) the person's refresh token for a connection.
    pub async fn set(&self, user: &str, name: &str, refresh_token: &str) -> Result<()> {
        validate_user_id(user).map_err(anyhow::Error::msg)?;
        let _guard = self.write.lock().await;
        let mut map = self
            .users
            .read()
            .await
            .get(user)
            .cloned()
            .unwrap_or_default();
        map.insert(
            name.to_string(),
            Stored {
                refresh_token: refresh_token.to_string(),
                connected_at: Utc::now(),
            },
        );
        self.save(user, &map).await?;
        self.users.write().await.insert(user.to_string(), map);
        Ok(())
    }

    /// Replaces the stored refresh token with the one the provider rotated to, but only if the connection still exists and still holds
    /// `old`: a person who disconnected, or replaced it, meanwhile keeps what they chose. `connected_at` is unchanged. Returns whether it changed.
    pub async fn rotate(&self, user: &str, name: &str, old: &str, new: &str) -> Result<bool> {
        let _guard = self.write.lock().await;
        let Some(mut map) = self.users.read().await.get(user).cloned() else {
            return Ok(false);
        };
        match map.get_mut(name) {
            Some(stored) if stored.refresh_token == old => stored.refresh_token = new.to_string(),
            _ => return Ok(false),
        }
        self.save(user, &map).await?;
        self.users.write().await.insert(user.to_string(), map);
        Ok(true)
    }

    /// The person's refresh token for a connection: for the host's own use, never returned by an API.
    pub async fn refresh_token(&self, user: &str, name: &str) -> Option<String> {
        self.users
            .read()
            .await
            .get(user)?
            .get(name)
            .map(|s| s.refresh_token.clone())
    }

    /// Removes it. Returns whether it was set.
    pub async fn remove(&self, user: &str, name: &str) -> Result<bool> {
        let _guard = self.write.lock().await;
        let mut map = self
            .users
            .read()
            .await
            .get(user)
            .cloned()
            .unwrap_or_default();
        let existed = map.remove(name).is_some();
        if existed {
            self.save(user, &map).await?;
            self.users.write().await.insert(user.to_string(), map);
        }
        Ok(existed)
    }

    /// When each of the person's connections was set (never the tokens).
    pub async fn connected_at(&self, user: &str) -> HashMap<String, DateTime<Utc>> {
        self.users
            .read()
            .await
            .get(user)
            .map(|m| m.iter().map(|(k, v)| (k.clone(), v.connected_at)).collect())
            .unwrap_or_default()
    }
}

// ---- HTTP -----------------------------------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
pub struct UserQuery {
    #[serde(default)]
    pub user_id: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct SetRequest {
    pub refresh_token: String,
}

async fn subject(
    state: &AppState,
    principal: &Principal,
    q: &UserQuery,
    what: &str,
    name: Option<&str>,
) -> Result<String, ApiError> {
    match principal.user() {
        Some(user) => Ok(user.to_string()),
        None => {
            let user = q
                .user_id
                .clone()
                .ok_or_else(|| ApiError::bad_request("user_id is required for the operator"))?;
            validate_user_id(&user).map_err(ApiError::bad_request)?;
            let _ = state
                .store
                .audit
                .append(
                    None,
                    AuditPhase::Performed,
                    "keep.connection.operator_access",
                    None,
                    json!({ "user_id": user, "action": what, "connection": name }),
                )
                .await;
            Ok(user)
        }
    }
}

/// `GET /v1/connections`: the connections this host offers and whether the person has set each.
pub(crate) async fn list_connections(
    State(state): State<Arc<AppState>>,
    Extension(principal): Extension<Principal>,
    Query(q): Query<UserQuery>,
) -> Result<Json<Value>, ApiError> {
    let user = subject(&state, &principal, &q, "list", None).await?;
    let set = state.store.connections.connected_at(&user).await;
    let items: Vec<Value> = state
        .credentials
        .connection_names()
        .into_iter()
        .map(|name| json!({ "name": name, "connected": set.contains_key(&name), "connected_at": set.get(&name) }))
        .collect();
    Ok(Json(json!({ "items": items })))
}

fn offered(state: &AppState, name: &str) -> Result<(), ApiError> {
    if state
        .credentials
        .connection_names()
        .iter()
        .any(|n| n == name)
    {
        Ok(())
    } else {
        Err(ApiError::not_found("this host offers no such connection"))
    }
}

/// `PUT /v1/connections/{name}` with `{"refresh_token": ...}`: connect. Write-only.
pub(crate) async fn put_connection(
    State(state): State<Arc<AppState>>,
    Extension(principal): Extension<Principal>,
    Path(name): Path<String>,
    Query(q): Query<UserQuery>,
    Json(req): Json<SetRequest>,
) -> Result<(StatusCode, Json<Value>), ApiError> {
    offered(&state, &name)?;
    let user = subject(&state, &principal, &q, "connect", Some(&name)).await?;
    if !valid_token(&req.refresh_token) {
        return Err(ApiError::bad_request(format!("refresh_token must be {MIN_TOKEN_LEN} to {MAX_TOKEN_LEN} printable characters without spaces")));
    }
    state
        .store
        .connections
        .set(&user, &name, &req.refresh_token)
        .await
        .map_err(ApiError::internal)?;
    // a new token replaces whatever access tokens were minted from the old one
    state.credentials.forget_user_connection(&user, &name);
    let _ = state
        .store
        .audit
        .append(
            None,
            AuditPhase::Performed,
            "keep.connection.connected",
            None,
            json!({ "user_id": user, "connection": name }),
        )
        .await;
    Ok((
        StatusCode::OK,
        Json(json!({ "name": name, "connected": true })),
    ))
}

/// `DELETE /v1/connections/{name}`: disconnect (the token and the cached access tokens are gone at once).
pub(crate) async fn delete_connection(
    State(state): State<Arc<AppState>>,
    Extension(principal): Extension<Principal>,
    Path(name): Path<String>,
    Query(q): Query<UserQuery>,
) -> Result<StatusCode, ApiError> {
    offered(&state, &name)?;
    let user = subject(&state, &principal, &q, "disconnect", Some(&name)).await?;
    let existed = state
        .store
        .connections
        .remove(&user, &name)
        .await
        .map_err(ApiError::internal)?;
    state.credentials.forget_user_connection(&user, &name);
    if !existed {
        return Err(ApiError::not_found("that connection was not set"));
    }
    let _ = state
        .store
        .audit
        .append(
            None,
            AuditPhase::Performed,
            "keep.connection.disconnected",
            None,
            json!({ "user_id": user, "connection": name }),
        )
        .await;
    Ok(StatusCode::NO_CONTENT)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_refresh_token_must_look_like_one() {
        assert!(valid_token("1//0gAbCdEfGhIjKlMnOpQr"));
        for bad in [
            "",
            "short",
            "has a space in it!!",
            "tab\tbetween-chars-1234",
            &"x".repeat(MAX_TOKEN_LEN + 1),
        ] {
            assert!(!valid_token(bad), "{bad:?}");
        }
    }

    #[tokio::test]
    async fn tokens_are_stored_per_person_survive_a_restart_and_are_removed() {
        let dir = tempfile::tempdir().unwrap();
        let s = ConnectionStore::open(dir.path()).await.unwrap();
        s.set("ana", "google", "1//ana-refresh-token")
            .await
            .unwrap();
        s.set("ben", "google", "1//ben-refresh-token")
            .await
            .unwrap();
        drop(s);
        let s = ConnectionStore::open(dir.path()).await.unwrap();
        assert_eq!(
            s.refresh_token("ana", "google").await.as_deref(),
            Some("1//ana-refresh-token")
        );
        assert_eq!(
            s.refresh_token("ben", "google").await.as_deref(),
            Some("1//ben-refresh-token")
        );
        assert!(s.refresh_token("ana", "other").await.is_none());
        assert!(s.connected_at("ana").await.contains_key("google"));
        assert!(s.remove("ana", "google").await.unwrap());
        assert!(
            !s.remove("ana", "google").await.unwrap(),
            "a second removal finds nothing"
        );
        assert!(s.refresh_token("ana", "google").await.is_none());
        assert!(
            s.refresh_token("ben", "google").await.is_some(),
            "another person's token is untouched"
        );
        // replacing overwrites
        s.set("ben", "google", "1//ben-new-token-value")
            .await
            .unwrap();
        assert_eq!(
            ConnectionStore::open(dir.path())
                .await
                .unwrap()
                .refresh_token("ben", "google")
                .await
                .as_deref(),
            Some("1//ben-new-token-value")
        );
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn the_files_holding_tokens_are_private_to_the_host_user() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        let s = ConnectionStore::open(dir.path()).await.unwrap();
        s.set("ana", "google", "1//ana-refresh-token")
            .await
            .unwrap();
        let mode = std::fs::metadata(dir.path().join("ana.json"))
            .unwrap()
            .permissions()
            .mode()
            & 0o777;
        assert_eq!(
            mode, 0o600,
            "a refresh token file must not be readable by others"
        );
        let leftovers: Vec<_> = std::fs::read_dir(dir.path())
            .unwrap()
            .filter_map(|e| e.ok())
            .filter(|e| e.file_name().to_string_lossy().contains(".tmp-"))
            .collect();
        assert!(leftovers.is_empty(), "no temporary copy is left behind");
    }

    #[tokio::test]
    async fn a_rotated_token_replaces_only_the_one_that_was_used_and_never_revives_a_removed_connection(
    ) {
        let dir = tempfile::tempdir().unwrap();
        let store = ConnectionStore::open(dir.path()).await.unwrap();
        store.set("ana", "microsoft", "rt-1").await.unwrap();
        let connected = store.connected_at("ana").await;
        assert!(store
            .rotate("ana", "microsoft", "rt-1", "rt-2")
            .await
            .unwrap());
        assert_eq!(
            store.refresh_token("ana", "microsoft").await.as_deref(),
            Some("rt-2")
        );
        assert_eq!(
            store.connected_at("ana").await,
            connected,
            "the time she connected does not change"
        );
        // she connected again meanwhile: what she chose stays
        store.set("ana", "microsoft", "rt-new").await.unwrap();
        assert!(!store
            .rotate("ana", "microsoft", "rt-2", "rt-3")
            .await
            .unwrap());
        assert_eq!(
            store.refresh_token("ana", "microsoft").await.as_deref(),
            Some("rt-new")
        );
        // she disconnected while the token was being refreshed
        store.remove("ana", "microsoft").await.unwrap();
        assert!(!store
            .rotate("ana", "microsoft", "rt-new", "rt-4")
            .await
            .unwrap());
        assert!(store.refresh_token("ana", "microsoft").await.is_none());
        assert!(!store.rotate("nobody", "microsoft", "a", "b").await.unwrap());
        // and it survives a restart
        store.set("ben", "microsoft", "b-1").await.unwrap();
        store
            .rotate("ben", "microsoft", "b-1", "b-2")
            .await
            .unwrap();
        let again = ConnectionStore::open(dir.path()).await.unwrap();
        assert_eq!(
            again.refresh_token("ben", "microsoft").await.as_deref(),
            Some("b-2")
        );
    }
}
