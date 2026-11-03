// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: Apache-2.0

//! Per-user conversation threads and their messages.
//!
//! A thread outlives any one session: a chat client keeps one thread while the agent's sessions come and go.
//! Everything here lives on the host under `threads/<id>/` (a `thread.json` and an append-only
//! `messages.jsonl`), never inside a cell. Every route is scoped to the caller: a user token sees, reads and
//! forgets only its own threads, and a thread that is not yours looks like one that does not exist.

use crate::{
    app::ApiError,
    audit::AuditPhase,
    authz::Principal,
    model::validate_user_id,
    store::{atomic_write, validate_name},
    AppState,
};
use anyhow::{bail, Context, Result};
use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    Extension, Json,
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{
    collections::HashMap,
    path::{Path as FsPath, PathBuf},
    sync::Arc,
};
use tokio::{
    fs,
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader},
    sync::{Mutex, RwLock},
};
use uuid::Uuid;

/// The longest message kept, in bytes. A longer one is refused rather than cut.
pub const MAX_MESSAGE_BYTES: usize = 32 * 1024;
/// Threads one user may hold. Delete one to make room.
pub const MAX_THREADS_PER_USER: usize = 500;
/// A title is cut to this many characters.
pub const MAX_TITLE_CHARS: usize = 120;
/// The most messages one read returns.
pub const MAX_PAGE: usize = 500;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ThreadRecord {
    pub id: Uuid,
    pub user_id: String,
    pub agent: String,
    /// The id a chat client uses for this conversation (AG-UI `threadId`), unique per user.
    #[serde(default)]
    pub client_thread_id: Option<String>,
    pub title: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub message_count: u64,
    /// The session currently serving the thread, if any.
    #[serde(default)]
    pub session_id: Option<Uuid>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Role {
    User,
    Assistant,
    System,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MessageRecord {
    /// Stable within the thread (`msg-<seq>`).
    pub id: String,
    pub thread_id: Uuid,
    /// 1, 2, 3 ... in the order the messages were added; the cursor for `?after=`.
    pub seq: u64,
    pub role: Role,
    pub text: String,
    pub created_at: DateTime<Utc>,
    /// The session that produced or received the message, and the event it ended at, to tie it to the run.
    #[serde(default)]
    pub session_id: Option<Uuid>,
    #[serde(default)]
    pub event_seq: Option<u64>,
}

pub struct ThreadStore {
    root: PathBuf,
    threads: RwLock<HashMap<Uuid, ThreadRecord>>,
    /// Serializes writers so a message's `seq` and its place in the file agree.
    write: Mutex<()>,
}

impl ThreadStore {
    pub async fn open(root: impl AsRef<FsPath>) -> Result<Self> {
        let root = root.as_ref().to_path_buf();
        fs::create_dir_all(&root).await?;
        let mut threads = HashMap::new();
        let mut entries = fs::read_dir(&root).await?;
        while let Some(entry) = entries.next_entry().await? {
            let path = entry.path().join("thread.json");
            let Ok(raw) = fs::read(&path).await else {
                continue;
            };
            let record: ThreadRecord = serde_json::from_slice(&raw)
                .with_context(|| format!("decoding {}", path.display()))?;
            threads.insert(record.id, record);
        }
        Ok(Self {
            root,
            threads: RwLock::new(threads),
            write: Mutex::new(()),
        })
    }

    /// A `Uuid`'s `Display` output is always the fixed `xxxxxxxx-xxxx-...` form and can never contain a path
    /// separator or `..`, but this checks anyway (matching [`atomic_write`]'s own guard) rather than trust that
    /// every future caller of this private helper keeps passing a real `Uuid`.
    fn dir(&self, id: Uuid) -> Result<PathBuf> {
        let segment = id.to_string();
        if segment.contains("..") || segment.contains(['/', '\\']) {
            bail!("refusing path traversal");
        }
        Ok(self.root.join(segment))
    }

    async fn persist(&self, record: &ThreadRecord) -> Result<()> {
        atomic_write(
            self.dir(record.id)?.join("thread.json"),
            &serde_json::to_vec_pretty(record)?,
        )
        .await
    }

    /// The user's thread with this client id, or a new one. `agent` and `title` apply only to a new thread.
    pub async fn get_or_create(
        &self,
        user_id: &str,
        agent: &str,
        title: &str,
        client_thread_id: Option<&str>,
    ) -> Result<ThreadRecord> {
        validate_user_id(user_id).map_err(anyhow::Error::msg)?;
        validate_name(agent)?;
        if let Some(c) = client_thread_id {
            if c.is_empty() || c.len() > 200 || c.chars().any(char::is_control) {
                bail!("the client thread id must be 1 to 200 printable characters");
            }
        }
        let _guard = self.write.lock().await;
        {
            let threads = self.threads.read().await;
            if let Some(c) = client_thread_id {
                if let Some(found) = threads
                    .values()
                    .find(|t| t.user_id == user_id && t.client_thread_id.as_deref() == Some(c))
                {
                    return Ok(found.clone());
                }
            }
            if threads.values().filter(|t| t.user_id == user_id).count() >= MAX_THREADS_PER_USER {
                bail!("thread limit reached ({MAX_THREADS_PER_USER}); delete one first");
            }
        }
        let now = Utc::now();
        let record = ThreadRecord {
            id: Uuid::new_v4(),
            user_id: user_id.to_string(),
            agent: agent.to_string(),
            client_thread_id: client_thread_id.map(str::to_string),
            title: title.trim().chars().take(MAX_TITLE_CHARS).collect(),
            created_at: now,
            updated_at: now,
            message_count: 0,
            session_id: None,
        };
        self.persist(&record).await?;
        self.threads.write().await.insert(record.id, record.clone());
        Ok(record)
    }

    pub async fn get(&self, id: Uuid) -> Option<ThreadRecord> {
        self.threads.read().await.get(&id).cloned()
    }

    /// Newest first. `None` lists every user's threads (the operator).
    pub async fn list(&self, user_id: Option<&str>) -> Vec<ThreadRecord> {
        let mut out: Vec<_> = self
            .threads
            .read()
            .await
            .values()
            .filter(|t| user_id.is_none_or(|u| t.user_id == u))
            .cloned()
            .collect();
        out.sort_by_key(|t| std::cmp::Reverse(t.updated_at));
        out
    }

    /// Adds a message to the end of the thread and returns it.
    pub async fn append(
        &self,
        thread_id: Uuid,
        role: Role,
        text: &str,
        session_id: Option<Uuid>,
        event_seq: Option<u64>,
    ) -> Result<MessageRecord> {
        if text.len() > MAX_MESSAGE_BYTES {
            bail!("a message may be at most {MAX_MESSAGE_BYTES} bytes");
        }
        let _guard = self.write.lock().await;
        let mut record = self
            .get(thread_id)
            .await
            .with_context(|| format!("thread {thread_id} not found"))?;
        let now = Utc::now();
        record.message_count += 1;
        record.updated_at = now;
        let message = MessageRecord {
            id: format!("msg-{}", record.message_count),
            thread_id,
            seq: record.message_count,
            role,
            text: text.to_string(),
            created_at: now,
            session_id,
            event_seq,
        };
        let messages_path = self.dir(thread_id)?.join("messages.jsonl");
        // `dir()` above already refuses this, but repeated here, on the actual path about to be
        // opened and in the same function as that open call, matching how `atomic_write` checks
        // its own path immediately before writing — a scanner that does not credit a check inside
        // a called function should still see this one.
        if messages_path.to_string_lossy().contains("..") {
            bail!("refusing path traversal");
        }
        let mut file = fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&messages_path)
            .await?;
        file.write_all(&serde_json::to_vec(&message)?).await?;
        file.write_all(b"\n").await?;
        file.flush().await?;
        self.persist(&record).await?;
        self.threads.write().await.insert(thread_id, record);
        Ok(message)
    }

    /// Messages with `seq > after`, oldest first, at most `limit` (never more than [`MAX_PAGE`]).
    pub async fn messages(
        &self,
        thread_id: Uuid,
        after: u64,
        limit: usize,
    ) -> Result<Vec<MessageRecord>> {
        let messages_path = self.dir(thread_id)?.join("messages.jsonl");
        // See the matching comment in `append`: repeated here, adjacent to the actual open call.
        if messages_path.to_string_lossy().contains("..") {
            bail!("refusing path traversal");
        }
        let file = match fs::File::open(&messages_path).await {
            Ok(f) => f,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(e) => return Err(e.into()),
        };
        let mut lines = BufReader::new(file).lines();
        let mut out = Vec::new();
        let limit = limit.clamp(1, MAX_PAGE);
        while let Some(line) = lines.next_line().await? {
            if line.trim().is_empty() {
                continue;
            }
            let message: MessageRecord = serde_json::from_str(&line)?;
            if message.seq > after {
                out.push(message);
                if out.len() >= limit {
                    break;
                }
            }
        }
        Ok(out)
    }

    /// Points the thread at the session now serving it (or clears it).
    pub async fn set_session(&self, thread_id: Uuid, session_id: Option<Uuid>) -> Result<()> {
        let _guard = self.write.lock().await;
        let mut record = self
            .get(thread_id)
            .await
            .with_context(|| format!("thread {thread_id} not found"))?;
        record.session_id = session_id;
        self.persist(&record).await?;
        self.threads.write().await.insert(thread_id, record);
        Ok(())
    }

    /// Forgets every thread not touched since `cutoff` that `keep` does not protect (a thread whose session is still running, say).
    /// Returns what was forgotten, so the caller can journal counts (never text).
    pub async fn purge_idle(
        &self,
        cutoff: DateTime<Utc>,
        keep: impl Fn(&ThreadRecord) -> bool,
    ) -> Result<Vec<ThreadRecord>> {
        let idle: Vec<ThreadRecord> = self
            .threads
            .read()
            .await
            .values()
            .filter(|t| t.updated_at < cutoff && !keep(t))
            .cloned()
            .collect();
        let mut gone = Vec::new();
        for t in idle {
            if self.delete(t.id).await? {
                gone.push(t);
            }
        }
        Ok(gone)
    }

    /// Forgets a thread and every message in it. Returns whether it existed.
    pub async fn delete(&self, thread_id: Uuid) -> Result<bool> {
        let _guard = self.write.lock().await;
        let existed = self.threads.write().await.remove(&thread_id).is_some();
        if existed {
            match fs::remove_dir_all(self.dir(thread_id)?).await {
                Ok(()) => {}
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
                Err(e) => return Err(e.into()),
            }
        }
        Ok(existed)
    }
}

// ---- HTTP -----------------------------------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
pub struct CreateThreadRequest {
    pub agent: String,
    #[serde(default)]
    pub title: String,
    /// The operator names the owner; a user token is always its own owner and may not name another.
    #[serde(default)]
    pub user_id: Option<String>,
    #[serde(default)]
    pub client_thread_id: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct ListQuery {
    /// Only threads with this agent.
    #[serde(default)]
    pub agent: Option<String>,
    /// Only this user's threads. Honoured for the operator; a user token always sees only its own.
    #[serde(default)]
    pub user_id: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct MessagesQuery {
    #[serde(default)]
    pub after: u64,
    #[serde(default)]
    pub limit: Option<usize>,
}

/// The thread if `principal` may see it: any thread for the operator, only one's own for a user. The route
/// middleware already refuses a thread that is not the caller's; this is the second check, for handlers that
/// resolve the thread themselves.
async fn visible(
    state: &AppState,
    principal: &Principal,
    id: Uuid,
) -> Result<ThreadRecord, ApiError> {
    let thread = state
        .store
        .threads
        .get(id)
        .await
        .ok_or_else(|| ApiError::not_found("thread not found"))?;
    match principal.user() {
        Some(user) if user != thread.user_id => Err(ApiError::not_found("thread not found")),
        _ => Ok(thread),
    }
}

pub(crate) async fn list_threads(
    State(state): State<Arc<AppState>>,
    Extension(principal): Extension<Principal>,
    Query(q): Query<ListQuery>,
) -> Json<Value> {
    let mut items = state.store.threads.list(principal.user()).await;
    if principal.user().is_none() {
        if let Some(user) = q.user_id.as_deref() {
            items.retain(|t| t.user_id == user);
        }
    }
    if let Some(agent) = q.agent.as_deref() {
        items.retain(|t| t.agent == agent);
    }
    Json(json!({ "items": items }))
}

pub(crate) async fn create_thread(
    State(state): State<Arc<AppState>>,
    Extension(principal): Extension<Principal>,
    Json(req): Json<CreateThreadRequest>,
) -> Result<(StatusCode, Json<ThreadRecord>), ApiError> {
    let user = match (principal.user(), req.user_id.as_deref()) {
        (Some(user), Some(named)) if user != named => {
            return Err(ApiError::bad_request(
                "a user token cannot create a thread for another user",
            ));
        }
        (Some(user), _) => user.to_string(),
        (None, Some(named)) => named.to_string(),
        (None, None) => return Err(ApiError::bad_request("user_id is required")),
    };
    if state.store.get_agent(&req.agent).await.is_none() {
        return Err(ApiError::not_found("agent not found"));
    }
    let thread = state
        .store
        .threads
        .get_or_create(
            &user,
            &req.agent,
            &req.title,
            req.client_thread_id.as_deref(),
        )
        .await
        .map_err(ApiError::bad_request)?;
    Ok((StatusCode::CREATED, Json(thread)))
}

pub(crate) async fn get_thread(
    State(state): State<Arc<AppState>>,
    Extension(principal): Extension<Principal>,
    Path(id): Path<Uuid>,
) -> Result<Json<ThreadRecord>, ApiError> {
    Ok(Json(visible(&state, &principal, id).await?))
}

pub(crate) async fn thread_messages(
    State(state): State<Arc<AppState>>,
    Extension(principal): Extension<Principal>,
    Path(id): Path<Uuid>,
    Query(q): Query<MessagesQuery>,
) -> Result<Json<Value>, ApiError> {
    visible(&state, &principal, id).await?;
    let items = state
        .store
        .threads
        .messages(id, q.after, q.limit.unwrap_or(MAX_PAGE))
        .await
        .map_err(ApiError::internal)?;
    Ok(Json(json!({ "items": items })))
}

pub(crate) async fn delete_thread(
    State(state): State<Arc<AppState>>,
    Extension(principal): Extension<Principal>,
    Path(id): Path<Uuid>,
) -> Result<StatusCode, ApiError> {
    let thread = visible(&state, &principal, id).await?;
    state
        .store
        .threads
        .delete(id)
        .await
        .map_err(ApiError::internal)?;
    // Only the fact of the deletion is journaled, never the messages.
    let _ = state
        .store
        .audit
        .append(
            thread.session_id,
            AuditPhase::Performed,
            "merion.thread.delete",
            Some(thread.agent.clone()),
            json!({ "thread_id": thread.id, "user_id": thread.user_id, "messages": thread.message_count }),
        )
        .await;
    Ok(StatusCode::NO_CONTENT)
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn store() -> (ThreadStore, tempfile::TempDir) {
        let dir = tempfile::tempdir().unwrap();
        (ThreadStore::open(dir.path()).await.unwrap(), dir)
    }

    #[tokio::test]
    async fn a_client_thread_id_finds_the_same_thread_for_the_same_user_only() {
        let (s, _d) = store().await;
        let a = s
            .get_or_create("ana", "chat", "First", Some("c1"))
            .await
            .unwrap();
        let again = s
            .get_or_create("ana", "chat", "ignored", Some("c1"))
            .await
            .unwrap();
        assert_eq!(a.id, again.id);
        assert_eq!(again.title, "First");
        let ben = s
            .get_or_create("ben", "chat", "", Some("c1"))
            .await
            .unwrap();
        assert_ne!(
            a.id, ben.id,
            "the same client id is a different thread for another user"
        );
        assert_eq!(s.list(Some("ana")).await.len(), 1);
        assert_eq!(s.list(None).await.len(), 2);
    }

    #[tokio::test]
    async fn messages_keep_their_order_and_survive_a_restart() {
        let dir = tempfile::tempdir().unwrap();
        let s = ThreadStore::open(dir.path()).await.unwrap();
        let t = s.get_or_create("ana", "chat", "t", None).await.unwrap();
        for (i, text) in ["hello", "world", "again"].iter().enumerate() {
            let role = if i % 2 == 0 {
                Role::User
            } else {
                Role::Assistant
            };
            let m = s
                .append(t.id, role, text, None, Some(i as u64))
                .await
                .unwrap();
            assert_eq!(m.seq, i as u64 + 1);
            assert_eq!(m.id, format!("msg-{}", i + 1));
        }
        drop(s);
        let reopened = ThreadStore::open(dir.path()).await.unwrap();
        let back = reopened.get(t.id).await.unwrap();
        assert_eq!(back.message_count, 3);
        let all = reopened.messages(t.id, 0, 100).await.unwrap();
        assert_eq!(
            all.iter().map(|m| m.text.as_str()).collect::<Vec<_>>(),
            ["hello", "world", "again"]
        );
        let tail = reopened.messages(t.id, 1, 100).await.unwrap();
        assert_eq!(tail.iter().map(|m| m.seq).collect::<Vec<_>>(), [2, 3]);
        let one = reopened.messages(t.id, 0, 1).await.unwrap();
        assert_eq!(one.len(), 1);
        // a message added after the restart continues the numbering
        assert_eq!(
            reopened
                .append(t.id, Role::User, "more", None, None)
                .await
                .unwrap()
                .seq,
            4
        );
    }

    #[tokio::test]
    async fn limits_and_bad_input_are_refused() {
        let (s, _d) = store().await;
        assert!(s.get_or_create("bad user", "chat", "", None).await.is_err());
        assert!(s.get_or_create("ana", "bad/agent", "", None).await.is_err());
        assert!(s.get_or_create("ana", "chat", "", Some("")).await.is_err());
        let t = s
            .get_or_create("ana", "chat", &"x".repeat(500), None)
            .await
            .unwrap();
        assert_eq!(t.title.chars().count(), MAX_TITLE_CHARS);
        let long = "y".repeat(MAX_MESSAGE_BYTES + 1);
        assert!(s.append(t.id, Role::User, &long, None, None).await.is_err());
        assert!(s
            .append(Uuid::new_v4(), Role::User, "x", None, None)
            .await
            .is_err());
        assert_eq!(
            s.get(t.id).await.unwrap().message_count,
            0,
            "a refused message is not counted"
        );
    }

    #[tokio::test]
    async fn deleting_forgets_the_thread_and_its_messages_on_disk() {
        let dir = tempfile::tempdir().unwrap();
        let s = ThreadStore::open(dir.path()).await.unwrap();
        let t = s.get_or_create("ana", "chat", "t", None).await.unwrap();
        s.append(t.id, Role::User, "secret", None, None)
            .await
            .unwrap();
        assert!(s.delete(t.id).await.unwrap());
        assert!(
            !s.delete(t.id).await.unwrap(),
            "a second delete finds nothing"
        );
        assert!(s.get(t.id).await.is_none());
        assert!(!dir.path().join(t.id.to_string()).exists());
        assert!(ThreadStore::open(dir.path())
            .await
            .unwrap()
            .list(None)
            .await
            .is_empty());
    }

    #[tokio::test]
    async fn the_thread_cap_is_per_user() {
        let (s, _d) = store().await;
        for i in 0..MAX_THREADS_PER_USER {
            s.get_or_create("ana", "chat", "", Some(&format!("c{i}")))
                .await
                .unwrap();
        }
        assert!(s
            .get_or_create("ana", "chat", "", Some("one-more"))
            .await
            .is_err());
        assert!(s
            .get_or_create("ben", "chat", "", Some("one-more"))
            .await
            .is_ok());
        // an existing client id is still found at the cap
        assert!(s.get_or_create("ana", "chat", "", Some("c0")).await.is_ok());
    }
}
