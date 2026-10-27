// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: Apache-2.0

//! Append-only, hash-chained journal of what agents planned, were allowed to do,
//! and did. Each entry commits to the previous entry's hash, so editing or
//! removing a past line is detectable by [`AuditLog::verify`].

use anyhow::{bail, Context, Result};
use chrono::{DateTime, SecondsFormat, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};
use tokio::{fs, io::AsyncWriteExt, sync::Mutex};
use uuid::Uuid;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum AuditPhase {
    /// The agent (or runtime) declared an action it intends to take.
    Planned,
    /// A human or policy allowed a planned action.
    Approved,
    /// A human or policy refused an action, or the runtime rejected it.
    Denied,
    /// The action ran and completed.
    Performed,
    /// The action was allowed but failed downstream (for example, upstream error).
    Failed,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditEntry {
    pub seq: u64,
    pub at: DateTime<Utc>,
    #[serde(default)]
    pub session_id: Option<Uuid>,
    pub phase: AuditPhase,
    pub action: String,
    #[serde(default)]
    pub subject: Option<String>,
    #[serde(default)]
    pub detail: Value,
    pub prev_hash: String,
    pub hash: String,
}

impl AuditEntry {
    fn compute_hash(&self) -> String {
        // Fields are joined with the ASCII unit separator, which cannot appear
        // in compact JSON or RFC 3339 timestamps, so field boundaries are unambiguous.
        let mut hasher = Sha256::new();
        for part in [
            self.prev_hash.as_str(),
            &self.seq.to_string(),
            &self.at.to_rfc3339_opts(SecondsFormat::Nanos, true),
            &self.session_id.map(|id| id.to_string()).unwrap_or_default(),
            &serde_json::to_string(&self.phase).unwrap_or_default(),
            &self.action,
            self.subject.as_deref().unwrap_or(""),
            &serde_json::to_string(&self.detail).unwrap_or_default(),
        ] {
            hasher.update(part.as_bytes());
            hasher.update([0x1f]);
        }
        hex::encode(hasher.finalize())
    }
}

struct Tail {
    next_seq: u64,
    last_hash: String,
}

pub struct AuditLog {
    path: PathBuf,
    tail: Mutex<Tail>,
}

/// Result of walking the whole chain.
#[derive(Debug, Clone, Serialize)]
pub struct ChainStatus {
    pub entries: u64,
    pub chain_ok: bool,
    /// Sequence number of the first entry that failed verification.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub broken_at: Option<u64>,
}

impl AuditLog {
    pub async fn open(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref().to_path_buf();
        // The only real caller joins a fixed literal ("audit.jsonl") onto the operator's configured data
        // root, so nothing here is ever attacker-influenced — but this is a public constructor, and checking
        // costs nothing, so a future caller cannot reintroduce a traversal by passing an unchecked value.
        if path.to_string_lossy().contains("..") {
            bail!("refusing a path with '..' segments");
        }
        let entries = read_entries(&path).await?;
        let tail = match entries.last() {
            Some(last) => Tail {
                next_seq: last.seq + 1,
                last_hash: last.hash.clone(),
            },
            None => Tail {
                next_seq: 0,
                last_hash: String::new(),
            },
        };
        Ok(Self {
            path,
            tail: Mutex::new(tail),
        })
    }

    pub async fn append(
        &self,
        session_id: Option<Uuid>,
        phase: AuditPhase,
        action: impl Into<String>,
        subject: Option<String>,
        detail: Value,
    ) -> Result<AuditEntry> {
        let mut tail = self.tail.lock().await;
        let mut entry = AuditEntry {
            seq: tail.next_seq,
            at: Utc::now(),
            session_id,
            phase,
            action: action.into(),
            subject,
            detail,
            prev_hash: tail.last_hash.clone(),
            hash: String::new(),
        };
        entry.hash = entry.compute_hash();

        let mut line = serde_json::to_vec(&entry)?;
        line.push(b'\n');
        let mut file = fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.path)
            .await
            .with_context(|| format!("opening {}", self.path.display()))?;
        file.write_all(&line).await?;
        file.sync_data().await?;

        tail.next_seq = entry.seq + 1;
        tail.last_hash = entry.hash.clone();
        crate::otlp::emit_entry(&entry);
        Ok(entry)
    }

    /// Newest-last slice of entries, optionally for one session, capped at `limit`.
    pub async fn list(&self, session_id: Option<Uuid>, limit: usize) -> Result<Vec<AuditEntry>> {
        // Hold the tail lock so a concurrent append cannot leave a torn last line.
        let _guard = self.tail.lock().await;
        let mut entries = read_entries(&self.path).await?;
        if let Some(id) = session_id {
            entries.retain(|e| e.session_id == Some(id));
        }
        let skip = entries.len().saturating_sub(limit);
        Ok(entries.split_off(skip))
    }

    /// Newest-last entries that belong to any of `sessions`, capped at `limit`. Used to give a
    /// user their own slice of the journal without exposing anyone else's rows.
    pub async fn list_for_sessions(
        &self,
        sessions: &std::collections::HashSet<Uuid>,
        limit: usize,
    ) -> Result<Vec<AuditEntry>> {
        let _guard = self.tail.lock().await;
        let mut entries = read_entries(&self.path).await?;
        entries.retain(|e| e.session_id.is_some_and(|id| sessions.contains(&id)));
        let skip = entries.len().saturating_sub(limit);
        Ok(entries.split_off(skip))
    }

    pub async fn verify(&self) -> Result<ChainStatus> {
        let _guard = self.tail.lock().await;
        let entries = read_entries(&self.path).await?;
        Ok(verify_chain(&entries))
    }
}

fn verify_chain(entries: &[AuditEntry]) -> ChainStatus {
    let mut prev = String::new();
    for (index, entry) in entries.iter().enumerate() {
        if entry.seq != index as u64
            || entry.prev_hash != prev
            || entry.hash != entry.compute_hash()
        {
            return ChainStatus {
                entries: entries.len() as u64,
                chain_ok: false,
                broken_at: Some(entry.seq),
            };
        }
        prev = entry.hash.clone();
    }
    ChainStatus {
        entries: entries.len() as u64,
        chain_ok: true,
        broken_at: None,
    }
}

async fn read_entries(path: &Path) -> Result<Vec<AuditEntry>> {
    let raw = match fs::read_to_string(path).await {
        Ok(raw) => raw,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => return Err(error.into()),
    };
    let mut entries = Vec::new();
    for (line_no, line) in raw.lines().enumerate() {
        if line.trim().is_empty() {
            continue;
        }
        match serde_json::from_str::<AuditEntry>(line) {
            Ok(entry) => entries.push(entry),
            Err(error) => bail!(
                "{}:{}: corrupt audit entry: {error}",
                path.display(),
                line_no + 1
            ),
        }
    }
    Ok(entries)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn temp_path(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!("zyvor-audit-{name}-{}.jsonl", Uuid::new_v4()))
    }

    #[tokio::test]
    async fn appends_chain_and_verifies() {
        let path = temp_path("chain");
        let log = AuditLog::open(&path).await.unwrap();
        let session = Uuid::new_v4();
        let a = log
            .append(
                Some(session),
                AuditPhase::Planned,
                "egress",
                Some("example.com".into()),
                json!({}),
            )
            .await
            .unwrap();
        let b = log
            .append(
                Some(session),
                AuditPhase::Approved,
                "egress",
                None,
                json!({"by": "human"}),
            )
            .await
            .unwrap();
        assert_eq!(a.seq, 0);
        assert_eq!(a.prev_hash, "");
        assert_eq!(b.prev_hash, a.hash);
        let status = log.verify().await.unwrap();
        assert!(status.chain_ok);
        assert_eq!(status.entries, 2);
        let _ = fs::remove_file(path).await;
    }

    /// `AuditLog::open` takes an arbitrary path (its only real caller joins a fixed literal onto the
    /// operator's data root, but it is a public constructor), so it refuses one with `..` in it rather
    /// than trust every future caller.
    #[tokio::test]
    async fn open_refuses_a_path_with_traversal_segments() {
        let path = std::env::temp_dir().join(format!("zyvor-audit-{}", Uuid::new_v4()));
        let traversal = path.join("..").join("etc").join("audit.jsonl");
        match AuditLog::open(&traversal).await {
            Ok(_) => panic!("expected a traversal refusal"),
            Err(error) => assert!(
                error.to_string().contains(".."),
                "expected a traversal refusal, got: {error}"
            ),
        }
    }

    #[tokio::test]
    async fn chain_survives_reopen() {
        let path = temp_path("reopen");
        let log = AuditLog::open(&path).await.unwrap();
        let first = log
            .append(None, AuditPhase::Performed, "x", None, Value::Null)
            .await
            .unwrap();
        drop(log);
        let log = AuditLog::open(&path).await.unwrap();
        let second = log
            .append(None, AuditPhase::Performed, "y", None, Value::Null)
            .await
            .unwrap();
        assert_eq!(second.seq, 1);
        assert_eq!(second.prev_hash, first.hash);
        assert!(log.verify().await.unwrap().chain_ok);
        let _ = fs::remove_file(path).await;
    }

    #[tokio::test]
    async fn tampering_is_detected() {
        let path = temp_path("tamper");
        let log = AuditLog::open(&path).await.unwrap();
        for action in ["one", "two", "three"] {
            log.append(None, AuditPhase::Performed, action, None, Value::Null)
                .await
                .unwrap();
        }
        let raw = fs::read_to_string(&path).await.unwrap();
        fs::write(&path, raw.replace("\"two\"", "\"TWO\""))
            .await
            .unwrap();
        let status = log.verify().await.unwrap();
        assert!(!status.chain_ok);
        assert_eq!(status.broken_at, Some(1));
        let _ = fs::remove_file(path).await;
    }

    #[tokio::test]
    async fn deleting_an_entry_is_detected() {
        let path = temp_path("delete");
        let log = AuditLog::open(&path).await.unwrap();
        for action in ["one", "two", "three"] {
            log.append(None, AuditPhase::Performed, action, None, Value::Null)
                .await
                .unwrap();
        }
        let raw = fs::read_to_string(&path).await.unwrap();
        let kept: Vec<&str> = raw
            .lines()
            .enumerate()
            .filter(|(i, _)| *i != 1)
            .map(|(_, l)| l)
            .collect();
        fs::write(&path, kept.join("\n") + "\n").await.unwrap();
        assert!(!log.verify().await.unwrap().chain_ok);
        let _ = fs::remove_file(path).await;
    }

    #[tokio::test]
    async fn list_filters_by_session_and_limits() {
        let path = temp_path("list");
        let log = AuditLog::open(&path).await.unwrap();
        let (s1, s2) = (Uuid::new_v4(), Uuid::new_v4());
        for (session, action) in [(s1, "a"), (s2, "b"), (s1, "c"), (s1, "d")] {
            log.append(
                Some(session),
                AuditPhase::Performed,
                action,
                None,
                Value::Null,
            )
            .await
            .unwrap();
        }
        let s1_entries = log.list(Some(s1), 100).await.unwrap();
        assert_eq!(
            s1_entries
                .iter()
                .map(|e| e.action.as_str())
                .collect::<Vec<_>>(),
            ["a", "c", "d"]
        );
        let last_two = log.list(None, 2).await.unwrap();
        assert_eq!(
            last_two
                .iter()
                .map(|e| e.action.as_str())
                .collect::<Vec<_>>(),
            ["c", "d"]
        );
        let _ = fs::remove_file(path).await;
    }
}
