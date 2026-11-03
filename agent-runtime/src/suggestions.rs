// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: Apache-2.0

//! Suggestions: things an agent thinks you might want done, that you decide about.
//!
//! An agent that asked for it (`"suggestions": true` in its manifest) and runs for a person can emit a `suggestion.propose` event
//! (`{title, reason, agent?}`), for example from a scheduled run that looks at your calendar. The host keeps it as a *suggestion* for that
//! person. Nothing happens because of one: accepting creates an ordinary [goal](crate::goals) for the person (no plan, nothing running;
//! they can ask for a plan and accept that separately), and dismissing remembers it so the same suggestion is not made again.
//!
//! * **Opt-in, off by default, per person.** Until they turn suggestions on, proposals are refused.
//! * **Bounded.** At most [`MAX_PENDING`] waiting per person, short plain-text title and reason, no text that looks like a secret, and a
//!   suggestion with the same title as one already waiting or dismissed is refused, so an agent cannot nag.
//! * **Untrusted content.** A session that had read untrusted content marks its suggestion `tainted`; accepting it needs `confirm_tainted`.
//! * **Private.** Every route is scoped to the caller (another person's suggestion is a 404); the operator names `user_id` and every
//!   operator access is journaled. The journal records that a suggestion was proposed or decided (ids and counts), never its text.

use crate::{
    app::ApiError,
    audit::AuditPhase,
    authz::Principal,
    model::{validate_user_id, AgentManifest, SessionRecord},
    store::atomic_write,
    AppState,
};
use anyhow::{bail, Context, Result};
use axum::{
    extract::{Path, Query, State},
    Extension, Json,
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{collections::HashMap, path::PathBuf, sync::Arc};
use tokio::{
    fs,
    sync::{Mutex, RwLock},
};
use uuid::Uuid;

pub const MAX_PENDING: usize = 20;
/// Decided suggestions kept per person, so a dismissed one is not proposed again.
pub const MAX_DECIDED: usize = 200;
pub const MAX_TITLE_CHARS: usize = 120;
pub const MAX_REASON_CHARS: usize = 500;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Status {
    Pending,
    Accepted,
    Dismissed,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Suggestion {
    pub id: Uuid,
    pub title: String,
    pub reason: String,
    /// The agent that would do it: the goal made on accept uses it.
    pub agent: String,
    /// The agent that made the suggestion.
    pub proposed_by: String,
    pub session_id: Uuid,
    /// Made by a session that had read untrusted content.
    #[serde(default)]
    pub tainted: bool,
    pub status: Status,
    /// Digest of the normalized title, used to refuse repeats.
    pub fingerprint: String,
    pub created_at: DateTime<Utc>,
    #[serde(default)]
    pub decided_at: Option<DateTime<Utc>>,
    /// The goal made when it was accepted.
    #[serde(default)]
    pub goal_id: Option<Uuid>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct UserSuggestions {
    pub enabled: bool,
    /// This person's own retention, in days, in place of the operator's `ZYVOR_AGENT_SUGGESTION_RETENTION_DAYS`. `None` defers to it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub retention_days: Option<u32>,
    pub items: Vec<Suggestion>,
}

/// A person's `retention_days` is 1 to this many days (about 10 years) — long enough that it is not a practical limit, short enough that
/// a mistyped value cannot mean "forever" by accident.
pub const MAX_RETENTION_DAYS: u32 = 3650;

pub struct SuggestionStore {
    root: PathBuf,
    users: RwLock<HashMap<String, UserSuggestions>>,
    write: Mutex<()>,
}

fn fingerprint(title: &str) -> String {
    let norm: String = title
        .to_lowercase()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    hex::encode(Sha256::digest(norm.as_bytes()))
}

/// Plain text: no control, zero-width or direction-changing characters, at most `max` characters.
fn clean(text: &str, max: usize) -> String {
    text.chars()
        .filter(|c| {
            !((c.is_control() && *c != '\n')
                || matches!(c, '\u{200B}'..='\u{200F}' | '\u{202A}'..='\u{202E}' | '\u{2060}'..='\u{2064}' | '\u{2066}'..='\u{2069}' | '\u{FEFF}'))
        })
        .take(max)
        .collect::<String>()
        .trim()
        .to_string()
}

impl SuggestionStore {
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
            users.insert(
                name.to_string(),
                serde_json::from_slice(&raw)
                    .with_context(|| format!("decoding {}", path.display()))?,
            );
        }
        Ok(Self {
            root,
            users: RwLock::new(users),
            write: Mutex::new(()),
        })
    }

    pub async fn get(&self, user: &str) -> UserSuggestions {
        self.users
            .read()
            .await
            .get(user)
            .cloned()
            .unwrap_or_default()
    }

    async fn edit<T>(
        &self,
        user: &str,
        change: impl FnOnce(&mut UserSuggestions) -> Result<T>,
    ) -> Result<T> {
        validate_user_id(user).map_err(anyhow::Error::msg)?;
        let _guard = self.write.lock().await;
        let mut s = self.get(user).await;
        let out = change(&mut s)?;
        // keep the newest decided ones only
        let mut decided = s
            .items
            .iter()
            .filter(|i| i.status != Status::Pending)
            .count();
        if decided > MAX_DECIDED {
            s.items.sort_by_key(|i| i.created_at);
            s.items.retain(|i| {
                if i.status != Status::Pending && decided > MAX_DECIDED {
                    decided -= 1;
                    false
                } else {
                    true
                }
            });
        }
        atomic_write(
            self.root.join(format!("{user}.json")),
            &serde_json::to_vec_pretty(&s)?,
        )
        .await?;
        self.users.write().await.insert(user.to_string(), s);
        Ok(out)
    }

    pub async fn set_enabled(&self, user: &str, enabled: bool) -> Result<()> {
        self.edit(user, |s| {
            s.enabled = enabled;
            Ok(())
        })
        .await
    }

    /// `None` clears it (back to the operator's default).
    pub async fn set_retention_days(&self, user: &str, retention_days: Option<u32>) -> Result<()> {
        self.edit(user, |s| {
            s.retention_days = retention_days;
            Ok(())
        })
        .await
    }

    /// An agent's suggestion. Needs suggestions to be on for the person; refuses a repeat and a full list.
    #[allow(clippy::too_many_arguments)]
    pub async fn propose(
        &self,
        user: &str,
        title: &str,
        reason: &str,
        agent: &str,
        proposed_by: &str,
        session_id: Uuid,
        tainted: bool,
    ) -> Result<Suggestion> {
        let title = clean(title, MAX_TITLE_CHARS);
        let reason = clean(reason, MAX_REASON_CHARS);
        if title.is_empty() {
            bail!("the suggestion has no title");
        }
        for text in [&title, &reason] {
            let found = crate::l7::scan(text);
            if !found.is_empty() {
                bail!(
                    "the text looks like it contains a secret ({}); a suggestion never carries credentials",
                    found.join(", ")
                );
            }
        }
        let fp = fingerprint(&title);
        self.edit(user, |s| {
            if !s.enabled {
                bail!("suggestions are off for this person");
            }
            if s.items.iter().any(|i| i.fingerprint == fp) {
                bail!("the same suggestion was already made");
            }
            if s.items
                .iter()
                .filter(|i| i.status == Status::Pending)
                .count()
                >= MAX_PENDING
            {
                bail!("too many suggestions are waiting for review");
            }
            let item = Suggestion {
                id: Uuid::new_v4(),
                title,
                reason,
                agent: agent.to_string(),
                proposed_by: proposed_by.to_string(),
                session_id,
                tainted,
                status: Status::Pending,
                fingerprint: fp,
                created_at: Utc::now(),
                decided_at: None,
                goal_id: None,
            };
            s.items.push(item.clone());
            Ok(item)
        })
        .await
    }

    /// Forgets suggestions from before each person's own cutoff: their own `retention_days` (`PUT /v1/suggestions/settings`) when they set
    /// one, otherwise `default_days` (the operator's `ZYVOR_AGENT_SUGGESTION_RETENTION_DAYS`); a person with neither is not swept at all.
    /// "Before its cutoff" means: nobody decided it (by when it was made) or it was decided (by when it was decided) that long ago.
    /// Returns (undecided, decided). A forgotten *dismissed* suggestion can be proposed again, since the memory of the dismissal goes with it.
    pub async fn purge_before(
        &self,
        now: DateTime<Utc>,
        default_days: Option<u64>,
    ) -> Result<(usize, usize)> {
        let _guard = self.write.lock().await;
        let mut changed: Vec<(String, UserSuggestions)> = Vec::new();
        let (mut undecided, mut decided) = (0usize, 0usize);
        for (user, s) in self.users.read().await.iter() {
            let Some(days) = s.retention_days.map(u64::from).or(default_days) else {
                continue;
            };
            let cutoff = now - chrono::Duration::days(days as i64);
            let mut next = s.clone();
            next.items.retain(|i| {
                let when = i.decided_at.unwrap_or(i.created_at);
                if when >= cutoff {
                    return true;
                }
                if i.status == Status::Pending {
                    undecided += 1;
                } else {
                    decided += 1;
                }
                false
            });
            if next.items.len() != s.items.len() {
                changed.push((user.clone(), next));
            }
        }
        for (user, s) in changed {
            atomic_write(
                self.root.join(format!("{user}.json")),
                &serde_json::to_vec_pretty(&s)?,
            )
            .await?;
            self.users.write().await.insert(user, s);
        }
        Ok((undecided, decided))
    }

    /// Marks a pending suggestion accepted (with its goal) or dismissed. `None` when there is no such pending suggestion.
    pub async fn decide(
        &self,
        user: &str,
        id: Uuid,
        goal_id: Option<Uuid>,
    ) -> Result<Option<Suggestion>> {
        self.edit(user, |s| {
            let Some(i) = s
                .items
                .iter_mut()
                .find(|i| i.id == id && i.status == Status::Pending)
            else {
                return Ok(None);
            };
            i.status = if goal_id.is_some() {
                Status::Accepted
            } else {
                Status::Dismissed
            };
            i.goal_id = goal_id;
            i.decided_at = Some(Utc::now());
            Ok(Some(i.clone()))
        })
        .await
    }
}

/// An agent's `suggestion.propose` event: stored as a suggestion for the session's person, or a `suggestion.refused` event saying why not
/// (never the text).
pub async fn record_proposal(
    state: &AppState,
    session: &SessionRecord,
    manifest: &AgentManifest,
    data: &Value,
) {
    let refuse = |reason: String| async move {
        let _ = state
            .store
            .append_event(
                session.id,
                "suggestion.refused",
                json!({ "reason": reason }),
            )
            .await;
    };
    let Some(user) = session.user_id.as_deref() else {
        return refuse("the session has no user".into()).await;
    };
    if !manifest.suggestions {
        return refuse(
            "the agent did not ask to make suggestions (manifest \"suggestions\": true)".into(),
        )
        .await;
    }
    let text = |k: &str| data.get(k).and_then(Value::as_str).unwrap_or_default();
    let agent = match data.get("agent").and_then(Value::as_str) {
        Some(a) if !a.is_empty() => a.to_string(),
        _ => session.agent.clone(),
    };
    if !crate::agui::valid_agent_name(&agent) {
        return refuse("agent must be a deployed agent's name".into()).await;
    }
    let tainted = !session.tainted_by.is_empty();
    match state
        .store
        .suggestions
        .propose(
            user,
            text("title"),
            text("reason"),
            &agent,
            &session.agent,
            session.id,
            tainted,
        )
        .await
    {
        Ok(item) => {
            let _ = state
                .store
                .audit
                .append(
                    Some(session.id),
                    AuditPhase::Planned,
                    "merion.suggestion.proposed",
                    Some(session.agent.clone()),
                    json!({ "user_id": user, "suggestion_id": item.id, "tainted": tainted }),
                )
                .await;
            // the person is told there is something to look at (generic text; the suggestion itself only if the operator allows it)
            crate::notify::notify_user(
                state,
                user,
                crate::notify::Notice {
                    event: "suggestion.proposed",
                    title: "Your agent has an idea".into(),
                    body: "Open Keep to make it a goal or dismiss it.".into(),
                    detail_title: format!("Suggestion from {}", session.agent),
                    detail_body: item.title.chars().take(140).collect(),
                    data: json!({ "suggestion_id": item.id, "tainted": tainted }),
                },
            );
        }
        Err(e) => refuse(e.to_string()).await,
    }
}

// ---- HTTP -----------------------------------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
pub struct UserQuery {
    /// The operator names whose suggestions it means; a user token is always its own and this is ignored.
    #[serde(default)]
    pub user_id: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct SettingsRequest {
    pub enabled: bool,
    /// This person's own retention, replacing the operator's default; 1 to `MAX_RETENTION_DAYS` days, or omitted/null to defer to it.
    #[serde(default)]
    pub retention_days: Option<u32>,
}

#[derive(Debug, Default, Deserialize)]
pub struct AcceptRequest {
    /// Needed when the suggestion is `tainted`.
    #[serde(default)]
    pub confirm_tainted: bool,
}

async fn subject(
    state: &AppState,
    principal: &Principal,
    q: &UserQuery,
    what: &str,
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
                    "merion.suggestion.operator_access",
                    None,
                    json!({ "user_id": user, "action": what }),
                )
                .await;
            Ok(user)
        }
    }
}

fn view(s: &UserSuggestions) -> Value {
    let (pending, decided): (Vec<_>, Vec<_>) =
        s.items.iter().partition(|i| i.status == Status::Pending);
    json!({ "enabled": s.enabled, "retention_days": s.retention_days, "pending": pending, "decided": decided.into_iter().rev().take(20).collect::<Vec<_>>() })
}

pub(crate) async fn list(
    State(state): State<Arc<AppState>>,
    Extension(principal): Extension<Principal>,
    Query(q): Query<UserQuery>,
) -> Result<Json<Value>, ApiError> {
    let user = subject(&state, &principal, &q, "read").await?;
    Ok(Json(view(&state.store.suggestions.get(&user).await)))
}

pub(crate) async fn put_settings(
    State(state): State<Arc<AppState>>,
    Extension(principal): Extension<Principal>,
    Query(q): Query<UserQuery>,
    Json(req): Json<SettingsRequest>,
) -> Result<Json<Value>, ApiError> {
    let user = subject(&state, &principal, &q, "settings").await?;
    if req
        .retention_days
        .is_some_and(|d| d == 0 || d > MAX_RETENTION_DAYS)
    {
        return Err(ApiError::bad_request(format!(
            "retention_days must be 1 to {MAX_RETENTION_DAYS}"
        )));
    }
    state
        .store
        .suggestions
        .set_enabled(&user, req.enabled)
        .await
        .map_err(ApiError::internal)?;
    state
        .store
        .suggestions
        .set_retention_days(&user, req.retention_days)
        .await
        .map_err(ApiError::internal)?;
    Ok(Json(view(&state.store.suggestions.get(&user).await)))
}

/// `POST /v1/suggestions/{id}/accept`: makes an ordinary goal for the person (no plan, not running) and marks the suggestion accepted.
pub(crate) async fn accept(
    State(state): State<Arc<AppState>>,
    Extension(principal): Extension<Principal>,
    Path(id): Path<Uuid>,
    Query(q): Query<UserQuery>,
    body: axum::body::Bytes,
) -> Result<Json<Value>, ApiError> {
    let user = subject(&state, &principal, &q, "accept").await?;
    let req: AcceptRequest = crate::goal_plan::optional_json(&body)?;
    let suggestion = state
        .store
        .suggestions
        .get(&user)
        .await
        .items
        .into_iter()
        .find(|i| i.id == id && i.status == Status::Pending)
        .ok_or_else(|| ApiError::not_found("suggestion not found"))?;
    if suggestion.tainted && !req.confirm_tainted {
        return Err(ApiError::conflict(
            "the agent had read untrusted content; read it and accept with confirm_tainted: true",
        ));
    }
    let create = crate::goals::CreateGoalRequest {
        title: suggestion.title.clone(),
        description: suggestion.reason.clone(),
        agent: suggestion.agent.clone(),
        user_id: Some(user.clone()),
        session_id: None,
        plan: vec![],
        allow_hosts: vec![],
        autorun: false,
        max_attempts: None,
    };
    let (_, Json(goal)) =
        crate::goals::create_goal_route(State(state.clone()), Extension(principal), Json(create))
            .await?;
    let done = state
        .store
        .suggestions
        .decide(&user, id, Some(goal.id))
        .await
        .map_err(ApiError::internal)?
        .ok_or_else(|| ApiError::conflict("the suggestion was decided meanwhile"))?;
    let _ = state
        .store
        .audit
        .append(
            None,
            AuditPhase::Performed,
            "merion.suggestion.accepted",
            Some(goal.agent.clone()),
            json!({ "user_id": user, "suggestion_id": id, "goal_id": goal.id }),
        )
        .await;
    Ok(Json(json!({ "suggestion": done, "goal": goal })))
}

pub(crate) async fn dismiss(
    State(state): State<Arc<AppState>>,
    Extension(principal): Extension<Principal>,
    Path(id): Path<Uuid>,
    Query(q): Query<UserQuery>,
) -> Result<Json<Suggestion>, ApiError> {
    let user = subject(&state, &principal, &q, "dismiss").await?;
    let done = state
        .store
        .suggestions
        .decide(&user, id, None)
        .await
        .map_err(ApiError::internal)?
        .ok_or_else(|| ApiError::not_found("suggestion not found"))?;
    let _ = state
        .store
        .audit
        .append(
            None,
            AuditPhase::Performed,
            "merion.suggestion.dismissed",
            None,
            json!({ "user_id": user, "suggestion_id": id }),
        )
        .await;
    Ok(Json(done))
}
