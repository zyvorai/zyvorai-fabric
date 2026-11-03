// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: Apache-2.0

//! Things that start a use case without a person uploading a file.
//!
//! - **webhook:** `POST /v1/triggers/{id}/hook`, the body is the file. The HMAC in
//!   `x-zyvor-signature` is the credential (the same scheme as `/v1/hooks/{id}`),
//!   so this route sits outside the bearer token.
//! - **folder:** a directory under `ZYVOR_AGENT_WATCH_ROOT`, scanned on an interval
//!   or a cron expression. Each new file runs in its own sealed cell.
//!
//! A trigger never bypasses the use case: a fire goes through
//! [`crate::demos::run_use_case`], so the extension and size checks, the strict
//! network policy and the freeze-on-connect rule apply exactly as for an upload.

use crate::{
    app::{ApiError, ApiResult},
    audit::AuditPhase,
    demos,
    schedules::{next_cron, signature_matches},
    AppState,
};
use axum::{
    body::Bytes,
    extract::{Path as AxPath, State},
    http::{HeaderMap, StatusCode},
    Json,
};
use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{
    path::{Path, PathBuf},
    sync::Arc,
};
use uuid::Uuid;

/// Operator-set root that folder triggers live under. Unset means folder triggers are off.
pub const WATCH_ROOT_ENV: &str = "ZYVOR_AGENT_WATCH_ROOT";

const SEEN_CAP: usize = 1000;
const MAX_FILES_PER_SCAN: usize = 5;
/// A file touched more recently than this may still be being written.
const SETTLE_SECONDS: i64 = 2;
const MIN_INTERVAL: u64 = 5;
const MAX_INTERVAL: u64 = 86_400;
const DEFAULT_INTERVAL: u64 = 30;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TriggerKind {
    Webhook,
    Folder,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TriggerRecord {
    pub id: Uuid,
    pub use_case: String,
    pub kind: TriggerKind,
    pub enabled: bool,
    pub created_at: DateTime<Utc>,
    #[serde(default)]
    pub runs: u64,
    #[serde(default)]
    pub last_run_at: Option<DateTime<Utc>>,
    #[serde(default)]
    pub last_error: Option<String>,
    /// Webhook only. Shown once, on creation.
    #[serde(default)]
    pub secret: Option<String>,
    /// Folder only: a directory name under the watch root.
    #[serde(default)]
    pub dir: Option<String>,
    #[serde(default)]
    pub interval_seconds: Option<u64>,
    #[serde(default)]
    pub cron: Option<String>,
    #[serde(default)]
    pub next_scan_at: Option<DateTime<Utc>>,
    /// Files already handled, as `name:size:mtime`, so each runs once.
    #[serde(default)]
    pub seen: Vec<String>,
}

impl TriggerRecord {
    /// What the API shows: never the secret, and the seen list as a count.
    pub fn view(&self) -> Value {
        json!({
            "id": self.id,
            "use_case": self.use_case,
            "kind": self.kind,
            "enabled": self.enabled,
            "created_at": self.created_at,
            "runs": self.runs,
            "last_run_at": self.last_run_at,
            "last_error": self.last_error,
            "dir": self.dir,
            "interval_seconds": self.interval_seconds,
            "cron": self.cron,
            "next_scan_at": self.next_scan_at,
            "seen": self.seen.len(),
            "hook": (self.kind == TriggerKind::Webhook)
                .then(|| format!("/v1/triggers/{}/hook", self.id)),
        })
    }
}

#[derive(Debug, Deserialize)]
pub struct CreateTriggerRequest {
    pub use_case: String,
    pub kind: TriggerKind,
    #[serde(default)]
    pub dir: Option<String>,
    #[serde(default)]
    pub interval_seconds: Option<u64>,
    #[serde(default)]
    pub cron: Option<String>,
}

pub fn watch_root() -> Option<PathBuf> {
    std::env::var_os(WATCH_ROOT_ENV)
        .filter(|v| !v.is_empty())
        .map(PathBuf::from)
}

/// A single path component: no separators, no dots at the start, nothing exotic.
pub fn valid_dir_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 64
        && !name.starts_with('.')
        && name
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.'))
}

/// The directory for `dir` under `root`, created if missing, and proven to be inside `root`
/// after symlinks are resolved.
pub fn resolve_watch_dir(root: &Path, dir: &str) -> Result<PathBuf, String> {
    if !valid_dir_name(dir) {
        return Err("dir must be a plain name of letters, digits, '-', '_' or '.'".into());
    }
    let root = root
        .canonicalize()
        .map_err(|e| format!("watch root {} is not usable: {e}", root.display()))?;
    let path = root.join(dir);
    std::fs::create_dir_all(&path).map_err(|e| format!("cannot create the watch dir: {e}"))?;
    let real = path
        .canonicalize()
        .map_err(|e| format!("cannot resolve the watch dir: {e}"))?;
    if real.starts_with(&root) && real != root {
        Ok(real)
    } else {
        Err("the watch dir resolves outside the watch root".into())
    }
}

/// A file name that is safe to log and to hand to the extension check.
pub fn clean_filename(raw: &str) -> Option<String> {
    let base = raw.rsplit(['/', '\\']).next().unwrap_or("").trim();
    (!base.is_empty() && base.len() <= 128 && !base.chars().any(char::is_control))
        .then(|| base.to_string())
}

pub fn file_key(name: &str, size: u64, mtime: i64) -> String {
    format!("{name}:{size}:{mtime}")
}

/// A file found in a watched folder.
#[derive(Debug, PartialEq, Eq)]
pub struct Pending {
    pub name: String,
    pub key: String,
    /// Set when the file will not be run (too big); it is still marked seen.
    pub skip: Option<String>,
}

/// New, settled, regular files in `dir` that the use case accepts.
/// Symlinks, dotfiles, other extensions and already-seen files are ignored.
pub fn pending_files(
    dir: &Path,
    seen: &[String],
    accepts: &[String],
    max_bytes: usize,
    now: DateTime<Utc>,
) -> std::io::Result<Vec<Pending>> {
    let mut out = Vec::new();
    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        let Ok(name) = entry.file_name().into_string() else {
            continue;
        };
        if name.starts_with('.') {
            continue;
        }
        let ext = name
            .rsplit_once('.')
            .map(|(_, e)| e.to_ascii_lowercase())
            .unwrap_or_default();
        if !accepts.contains(&ext) {
            continue;
        }
        let meta = std::fs::symlink_metadata(entry.path())?;
        if !meta.is_file() {
            continue;
        }
        let mtime = meta
            .modified()
            .ok()
            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
            .map_or(0, |d| d.as_secs() as i64);
        if now.timestamp() - mtime < SETTLE_SECONDS {
            continue;
        }
        let key = file_key(&name, meta.len(), mtime);
        if seen.contains(&key) {
            continue;
        }
        let skip = if meta.len() == 0 {
            Some("empty file".to_string())
        } else if meta.len() as usize > max_bytes {
            Some(format!(
                "{} bytes; the use case accepts up to {max_bytes}",
                meta.len()
            ))
        } else {
            None
        };
        out.push(Pending { name, key, skip });
    }
    out.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(out)
}

fn remember(seen: &mut Vec<String>, key: String) {
    seen.push(key);
    if seen.len() > SEEN_CAP {
        let excess = seen.len() - SEEN_CAP;
        seen.drain(..excess);
    }
}

// ---- background scanner -------------------------------------------------

pub async fn trigger_loop(state: Arc<AppState>) {
    loop {
        for trigger in state.store.list_triggers().await {
            if trigger.kind == TriggerKind::Folder && trigger.enabled {
                if let Err(error) = scan_folder(&state, trigger).await {
                    tracing::warn!(%error, "folder trigger scan failed");
                }
            }
        }
        tokio::time::sleep(std::time::Duration::from_secs(5)).await;
    }
}

fn next_scan(t: &TriggerRecord, now: DateTime<Utc>) -> DateTime<Utc> {
    match &t.cron {
        Some(expr) => next_cron(expr, now).unwrap_or(now + Duration::hours(1)),
        None => now + Duration::seconds(t.interval_seconds.unwrap_or(DEFAULT_INTERVAL) as i64),
    }
}

async fn scan_folder(state: &Arc<AppState>, mut t: TriggerRecord) -> anyhow::Result<()> {
    let now = Utc::now();
    if t.next_scan_at.is_some_and(|n| n > now) {
        return Ok(());
    }
    t.next_scan_at = Some(next_scan(&t, now));

    let Some(root) = watch_root() else {
        t.last_error = Some(format!("{WATCH_ROOT_ENV} is not set"));
        state.store.save_trigger_if_present(t).await?;
        return Ok(());
    };
    let dir = match resolve_watch_dir(&root, t.dir.as_deref().unwrap_or("")) {
        Ok(d) => d,
        Err(e) => {
            t.last_error = Some(e);
            state.store.save_trigger_if_present(t).await?;
            return Ok(());
        }
    };
    let Some((accepts, max_bytes)) = demos::use_case_limits(state, &t.use_case).await else {
        t.last_error = Some(format!("use case {:?} no longer exists", t.use_case));
        state.store.save_trigger_if_present(t).await?;
        return Ok(());
    };

    let scan_dir = dir.clone();
    let seen = t.seen.clone();
    let pending = tokio::task::spawn_blocking(move || {
        pending_files(&scan_dir, &seen, &accepts, max_bytes, Utc::now())
    })
    .await??;

    for p in pending.into_iter().take(MAX_FILES_PER_SCAN) {
        remember(&mut t.seen, p.key.clone());
        if let Some(why) = p.skip {
            t.last_error = Some(format!("{}: {why}", p.name));
            continue;
        }
        let bytes = match read_inside(&dir, &p.name, max_bytes).await {
            Ok(b) => b,
            Err(e) => {
                t.last_error = Some(format!("{}: {e}", p.name));
                continue;
            }
        };
        let result = demos::run_use_case(
            state.clone(),
            t.use_case.clone(),
            Some(p.name.clone()),
            Some(bytes),
            None,
            None,
        )
        .await;
        t.last_run_at = Some(Utc::now());
        match result {
            Ok(_) => {
                t.runs += 1;
                t.last_error = None;
            }
            Err(e) => t.last_error = Some(format!("{}: {}", p.name, e.message())),
        }
    }
    state.store.save_trigger_if_present(t).await?;
    Ok(())
}

/// Read one file, refusing anything that is not still a regular file inside `dir`.
async fn read_inside(dir: &Path, name: &str, max_bytes: usize) -> Result<Vec<u8>, String> {
    let dir = dir.to_path_buf();
    let name = name.to_string();
    tokio::task::spawn_blocking(move || {
        use std::io::Read;
        let path = dir.join(&name);
        let real = path.canonicalize().map_err(|e| e.to_string())?;
        if !real.starts_with(&dir) {
            return Err("the file resolves outside the watch dir".to_string());
        }
        if !std::fs::symlink_metadata(&real)
            .map_err(|e| e.to_string())?
            .is_file()
        {
            return Err("not a regular file".to_string());
        }
        let mut buf = Vec::new();
        std::fs::File::open(&real)
            .and_then(|f| f.take(max_bytes as u64 + 1).read_to_end(&mut buf))
            .map_err(|e| e.to_string())?;
        Ok(buf)
    })
    .await
    .map_err(|e| e.to_string())?
}

// ---- HTTP ---------------------------------------------------------------

pub(crate) async fn list_triggers(State(state): State<Arc<AppState>>) -> Json<Value> {
    let items: Vec<Value> = state
        .store
        .list_triggers()
        .await
        .iter()
        .map(TriggerRecord::view)
        .collect();
    Json(json!({ "items": items, "watch_root_configured": watch_root().is_some() }))
}

pub(crate) async fn create_trigger(
    State(state): State<Arc<AppState>>,
    Json(req): Json<CreateTriggerRequest>,
) -> ApiResult<(StatusCode, Json<Value>)> {
    if demos::use_case_limits(&state, &req.use_case)
        .await
        .is_none()
    {
        return Err(ApiError::not_found(format!(
            "no use case named {:?}",
            req.use_case
        )));
    }
    let now = Utc::now();
    let mut record = TriggerRecord {
        id: Uuid::new_v4(),
        use_case: req.use_case,
        kind: req.kind,
        enabled: true,
        created_at: now,
        runs: 0,
        last_run_at: None,
        last_error: None,
        secret: None,
        dir: None,
        interval_seconds: None,
        cron: None,
        next_scan_at: None,
        seen: Vec::new(),
    };
    match req.kind {
        TriggerKind::Webhook => {
            if req.dir.is_some() || req.interval_seconds.is_some() || req.cron.is_some() {
                return Err(ApiError::bad_request(
                    "a webhook trigger takes no dir, interval_seconds or cron",
                ));
            }
            record.secret = Some(format!(
                "{}{}",
                Uuid::new_v4().simple(),
                Uuid::new_v4().simple()
            ));
        }
        TriggerKind::Folder => {
            let Some(root) = watch_root() else {
                return Err(ApiError::bad_request(format!(
                    "folder triggers are off: set {WATCH_ROOT_ENV} on the runtime"
                )));
            };
            let dir = req
                .dir
                .ok_or_else(|| ApiError::bad_request("a folder trigger needs a dir"))?;
            resolve_watch_dir(&root, &dir).map_err(ApiError::bad_request)?;
            record.dir = Some(dir);
            match (req.cron, req.interval_seconds) {
                (Some(_), Some(_)) => {
                    return Err(ApiError::bad_request(
                        "give either cron or interval_seconds",
                    ))
                }
                (Some(expr), None) => {
                    next_cron(&expr, now).map_err(ApiError::bad_request)?;
                    record.cron = Some(expr);
                }
                (None, secs) => {
                    let secs = secs.unwrap_or(DEFAULT_INTERVAL);
                    if !(MIN_INTERVAL..=MAX_INTERVAL).contains(&secs) {
                        return Err(ApiError::bad_request(format!(
                            "interval_seconds must be between {MIN_INTERVAL} and {MAX_INTERVAL}"
                        )));
                    }
                    record.interval_seconds = Some(secs);
                }
            }
            record.next_scan_at = Some(now);
        }
    }
    state
        .store
        .save_trigger(record.clone())
        .await
        .map_err(ApiError::internal)?;
    let _ = state
        .store
        .audit
        .append(
            None,
            AuditPhase::Performed,
            "merion.trigger.created",
            Some(record.use_case.clone()),
            json!({ "trigger_id": record.id, "kind": record.kind, "dir": record.dir }),
        )
        .await;
    let mut view = record.view();
    if let Some(secret) = &record.secret {
        view["secret"] = json!(secret);
    }
    Ok((StatusCode::CREATED, Json(view)))
}

pub(crate) async fn delete_trigger(
    State(state): State<Arc<AppState>>,
    AxPath(id): AxPath<Uuid>,
) -> ApiResult<StatusCode> {
    if !state
        .store
        .delete_trigger(id)
        .await
        .map_err(ApiError::internal)?
    {
        return Err(ApiError::not_found("trigger not found"));
    }
    let _ = state
        .store
        .audit
        .append(
            None,
            AuditPhase::Performed,
            "merion.trigger.deleted",
            None,
            json!({ "trigger_id": id }),
        )
        .await;
    Ok(StatusCode::NO_CONTENT)
}

/// Signed ingress: the body is the file, `x-zyvor-filename` names it.
/// Runs synchronously, like an upload, and answers with the run's result.
pub(crate) async fn trigger_hook(
    State(state): State<Arc<AppState>>,
    AxPath(id): AxPath<Uuid>,
    headers: HeaderMap,
    body: Bytes,
) -> ApiResult<(StatusCode, Json<Value>)> {
    let Some(mut t) = state.store.get_trigger(id).await else {
        return Err(ApiError::not_found("trigger not found"));
    };
    let Some(secret) = t.secret.clone().filter(|_| t.kind == TriggerKind::Webhook) else {
        return Err(ApiError::not_found("trigger not found"));
    };
    if !t.enabled {
        return Err(ApiError::conflict("trigger is disabled"));
    }
    let presented = headers
        .get("x-zyvor-signature")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    if !signature_matches(&secret, &body, presented) {
        return Err(ApiError::unauthorized("invalid webhook signature"));
    }
    let filename = headers
        .get("x-zyvor-filename")
        .and_then(|v| v.to_str().ok())
        .and_then(clean_filename);
    let result = demos::run_use_case(
        state.clone(),
        t.use_case.clone(),
        filename,
        Some(body.to_vec()),
        None,
        None,
    )
    .await;
    t.last_run_at = Some(Utc::now());
    match &result {
        Ok(_) => {
            t.runs += 1;
            t.last_error = None;
        }
        Err(e) => t.last_error = Some(e.message().to_string()),
    }
    let _ = state.store.save_trigger_if_present(t).await;
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    fn accepts() -> Vec<String> {
        vec!["csv".into(), "txt".into()]
    }

    fn tmp() -> PathBuf {
        let p = std::env::temp_dir().join(format!("zyvor-triggers-{}", Uuid::new_v4()));
        std::fs::create_dir_all(&p).unwrap();
        p
    }

    /// `now` far enough after the fixtures' mtime that they count as settled.
    fn later() -> DateTime<Utc> {
        Utc::now() + Duration::seconds(60)
    }

    #[test]
    fn dir_names_are_single_plain_components() {
        for ok in ["inbox", "a-b_c.d", "x1"] {
            assert!(valid_dir_name(ok), "{ok}");
        }
        for bad in [
            "",
            ".hidden",
            "..",
            "a/b",
            "a\\b",
            "a b",
            "é",
            &"x".repeat(65),
        ] {
            assert!(!valid_dir_name(bad), "{bad}");
        }
    }

    #[test]
    fn watch_dir_is_created_inside_the_root() {
        let root = tmp();
        let dir = resolve_watch_dir(&root, "inbox").unwrap();
        assert!(dir.is_dir());
        assert!(dir.starts_with(root.canonicalize().unwrap()));
        assert!(resolve_watch_dir(&root, "../escape").is_err());
    }

    #[cfg(unix)]
    #[test]
    fn a_symlinked_watch_dir_that_leaves_the_root_is_refused() {
        let root = tmp();
        let outside = tmp();
        std::os::unix::fs::symlink(&outside, root.join("sneaky")).unwrap();
        let err = resolve_watch_dir(&root, "sneaky").unwrap_err();
        assert!(err.contains("outside"), "{err}");
    }

    #[test]
    fn pending_files_pick_new_settled_regular_files_of_the_right_type() {
        let dir = tmp();
        std::fs::write(dir.join("a.csv"), "x,y\n1,2\n").unwrap();
        std::fs::write(dir.join("b.txt"), "hi").unwrap();
        std::fs::write(dir.join("c.exe"), "MZ").unwrap();
        std::fs::write(dir.join(".d.csv"), "x").unwrap();
        std::fs::write(dir.join("empty.csv"), "").unwrap();
        std::fs::create_dir(dir.join("sub.csv")).unwrap();

        let found = pending_files(&dir, &[], &accepts(), 1024, later()).unwrap();
        let names: Vec<_> = found.iter().map(|p| p.name.as_str()).collect();
        assert_eq!(names, ["a.csv", "b.txt", "empty.csv"]);
        assert!(found[0].skip.is_none());
        assert_eq!(found[2].skip.as_deref(), Some("empty file"));
    }

    #[test]
    fn pending_files_skip_seen_unsettled_and_oversize() {
        let dir = tmp();
        std::fs::write(dir.join("big.csv"), "x".repeat(50)).unwrap();
        std::fs::write(dir.join("ok.csv"), "x").unwrap();

        let found = pending_files(&dir, &[], &accepts(), 10, later()).unwrap();
        let big = found.iter().find(|p| p.name == "big.csv").unwrap();
        assert!(big.skip.as_deref().unwrap().contains("accepts up to 10"));

        let seen: Vec<_> = found.iter().map(|p| p.key.clone()).collect();
        assert!(pending_files(&dir, &seen, &accepts(), 10, later())
            .unwrap()
            .is_empty());
        // Just written: not settled yet.
        assert!(pending_files(&dir, &[], &accepts(), 10, Utc::now())
            .unwrap()
            .is_empty());
    }

    #[cfg(unix)]
    #[test]
    fn symlinks_in_a_watched_folder_are_ignored() {
        let dir = tmp();
        let outside = tmp();
        std::fs::write(outside.join("secret.csv"), "s").unwrap();
        std::os::unix::fs::symlink(outside.join("secret.csv"), dir.join("link.csv")).unwrap();
        assert!(pending_files(&dir, &[], &accepts(), 1024, later())
            .unwrap()
            .is_empty());
    }

    #[test]
    fn seen_list_is_bounded() {
        let mut seen = Vec::new();
        for i in 0..(SEEN_CAP + 10) {
            remember(&mut seen, i.to_string());
        }
        assert_eq!(seen.len(), SEEN_CAP);
        assert_eq!(seen[0], "10");
    }

    #[test]
    fn filenames_are_reduced_to_a_safe_base_name() {
        assert_eq!(clean_filename("../../etc/a.csv").as_deref(), Some("a.csv"));
        assert_eq!(clean_filename("C:\\x\\b.txt").as_deref(), Some("b.txt"));
        assert_eq!(clean_filename("a\nb.csv"), None);
        assert_eq!(clean_filename("dir/"), None);
        assert_eq!(clean_filename(&"x".repeat(200)), None);
    }

    #[test]
    fn the_view_never_carries_the_secret_or_the_seen_list() {
        let t = TriggerRecord {
            id: Uuid::new_v4(),
            use_case: "csv-clean".into(),
            kind: TriggerKind::Webhook,
            enabled: true,
            created_at: Utc::now(),
            runs: 0,
            last_run_at: None,
            last_error: None,
            secret: Some("s3cret".into()),
            dir: None,
            interval_seconds: None,
            cron: None,
            next_scan_at: None,
            seen: vec!["a".into(), "b".into()],
        };
        let v = t.view();
        assert!(v.get("secret").is_none());
        assert_eq!(v["seen"], 2);
        assert_eq!(v["hook"], format!("/v1/triggers/{}/hook", t.id));
        assert!(!v.to_string().contains("s3cret"));
    }
}
