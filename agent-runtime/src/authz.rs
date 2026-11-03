// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: Apache-2.0

//! Who is calling, and what they may touch.
//!
//! The runtime has always had one operator token that can do everything. A product that serves
//! many people (a phone vendor's agent service, say) needs the layer above it to hand each person
//! a credential that reaches **only that person's** sessions, approvals, artifacts and audit rows.
//! This module is that credential and the rules behind it.
//!
//! - A **user token** is minted by the operator (`POST /v1/user-tokens`), bound to one `user_id`,
//!   a set of scopes and an expiry. It is stateless (HMAC-signed), so any shard can verify it, and
//!   can be cut off early per user (`POST /v1/users/{id}/revoke-tokens`).
//! - The auth middleware turns a request into a [`Principal`]. A user principal may call only the
//!   routes listed in [`user_route`]; everything else is refused, so a new operator route is closed
//!   to users by default.
//! - For routes that name one object (a session, an approval, an artifact) the middleware also
//!   proves the object belongs to the user before the handler runs. Handlers that list things
//!   filter by the principal.
//!
//! Tokens are accepted in the `Authorization` header only, never in a query string.

use crate::{schedules::hmac_sha256, AppState};
use axum::http::Method;
use base64::Engine;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

pub const TOKEN_PREFIX: &str = "kut1.";
/// Environment variable that sets the user-token signing secret. Without it the key is derived
/// from the operator token, so a shard needs no extra configuration.
pub const SECRET_ENV: &str = "ZYVOR_AGENT_USER_TOKEN_SECRET";
pub const MAX_TTL_SECONDS: u64 = 7 * 24 * 3600;
pub const DEFAULT_TTL_SECONDS: u64 = 3600;

/// What a token may do. Reading covers every `GET` a user is allowed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Scope {
    /// List and read the user's own sessions, approvals, artifacts, audit and usage.
    Read,
    /// Start use cases and sessions, and steer or cancel them.
    Run,
    /// Decide the user's own approvals.
    Approve,
}

impl Scope {
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "read" => Some(Scope::Read),
            "run" => Some(Scope::Run),
            "approve" => Some(Scope::Approve),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Principal {
    /// The operator token, or no auth configured (a loopback dev instance).
    Operator,
    User {
        id: String,
        scopes: Vec<Scope>,
    },
}

impl Principal {
    /// The user id this caller is limited to, or `None` for the operator.
    pub fn user(&self) -> Option<&str> {
        match self {
            Principal::User { id, .. } => Some(id),
            Principal::Operator => None,
        }
    }

    pub fn is_operator(&self) -> bool {
        matches!(self, Principal::Operator)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UserClaims {
    #[serde(rename = "u")]
    pub user_id: String,
    #[serde(rename = "s")]
    pub scopes: Vec<Scope>,
    #[serde(rename = "i")]
    pub issued_at: i64,
    #[serde(rename = "e")]
    pub expires_at: i64,
}

/// The signing key: the explicit secret if set, otherwise derived from the operator token.
/// `None` means user tokens are unavailable (no operator token and no secret).
pub fn signing_key(api_token: Option<&str>, secret_env: Option<&str>) -> Option<Vec<u8>> {
    if let Some(s) = secret_env.filter(|s| s.len() >= 16) {
        return Some(s.as_bytes().to_vec());
    }
    api_token
        .filter(|t| !t.is_empty())
        .map(|t| hmac_sha256(t.as_bytes(), b"merion-user-token-v1").to_vec())
}

pub fn signing_key_for(state: &AppState) -> Option<Vec<u8>> {
    signing_key(
        state.config.api_token.as_deref(),
        std::env::var(SECRET_ENV).ok().as_deref(),
    )
}

fn ct_eq(a: &[u8], b: &[u8]) -> bool {
    a.len() == b.len() && a.iter().zip(b).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
}

fn sign(key: &[u8], body: &str) -> String {
    hex::encode(hmac_sha256(key, format!("{TOKEN_PREFIX}{body}").as_bytes()))
}

/// Mint a token. `ttl_seconds` is clamped to `60..=MAX_TTL_SECONDS`.
pub fn mint(
    key: &[u8],
    user_id: &str,
    scopes: &[Scope],
    ttl_seconds: u64,
    now: DateTime<Utc>,
) -> Result<(String, UserClaims), String> {
    crate::model::validate_user_id(user_id)?;
    if scopes.is_empty() {
        return Err("at least one scope is required".into());
    }
    let ttl = ttl_seconds.clamp(60, MAX_TTL_SECONDS) as i64;
    let claims = UserClaims {
        user_id: user_id.to_string(),
        scopes: scopes.to_vec(),
        issued_at: now.timestamp(),
        expires_at: now.timestamp() + ttl,
    };
    let body = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .encode(serde_json::to_vec(&claims).map_err(|e| e.to_string())?);
    let token = format!("{TOKEN_PREFIX}{body}.{}", sign(key, &body));
    Ok((token, claims))
}

/// Check a token's signature and expiry. `not_before` is the per-user revocation floor: a token
/// issued before it is refused.
pub fn verify(
    key: &[u8],
    token: &str,
    now: DateTime<Utc>,
    not_before: Option<DateTime<Utc>>,
) -> Result<UserClaims, &'static str> {
    let rest = token.strip_prefix(TOKEN_PREFIX).ok_or("not a user token")?;
    let (body, sig) = rest.rsplit_once('.').ok_or("malformed token")?;
    if !ct_eq(sign(key, body).as_bytes(), sig.as_bytes()) {
        return Err("bad signature");
    }
    let raw = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(body)
        .map_err(|_| "malformed token")?;
    let claims: UserClaims = serde_json::from_slice(&raw).map_err(|_| "malformed token")?;
    if crate::model::validate_user_id(&claims.user_id).is_err() {
        return Err("malformed token");
    }
    if now.timestamp() >= claims.expires_at {
        return Err("token expired");
    }
    if not_before.is_some_and(|floor| claims.issued_at < floor.timestamp()) {
        return Err("token revoked");
    }
    Ok(claims)
}

/// How a user's request is treated.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UserRoute {
    /// Not for users.
    Denied,
    /// The handler filters by the principal (lists, usage, running a use case).
    Open(Scope),
    /// Names one session, which must be the user's.
    Session(Uuid, Scope),
    /// Names one approval, whose session must be the user's.
    Approval(Uuid, Scope),
    /// Names one or two artifacts, whose sessions must be the user's.
    Artifacts(Vec<Uuid>, Scope),
    /// A route under `/v1/users/{id}/…`: the id must be the caller's own.
    OwnUser(String, Scope),
    /// Names one conversation thread, which must be the user's.
    Thread(Uuid, Scope),
    /// Names one goal, which must be the user's.
    Goal(Uuid, Scope),
}

/// The only routes a user token can reach. Anything not listed is [`UserRoute::Denied`].
pub fn user_route(method: &Method, path: &str) -> UserRoute {
    let parts: Vec<&str> = path.trim_matches('/').split('/').collect();
    let read = *method == Method::GET;
    let write = matches!(*method, Method::POST | Method::DELETE);
    let id = |s: &str| Uuid::parse_str(s).ok();
    match parts.as_slice() {
        ["v1", "sessions"] if read => UserRoute::Open(Scope::Read),
        ["v1", "sessions"] if *method == Method::POST => UserRoute::Open(Scope::Run),
        // Recovering a session through the host is an operator action.
        ["v1", "sessions", _, "host-recover", ..] => UserRoute::Denied,
        // Restoring the cell to a snapshot discards everything it did since: an operator action.
        ["v1", "sessions", _, "changesets", ..] => UserRoute::Denied,
        ["v1", "sessions", sid, ..] => match id(sid) {
            Some(sid) if read => UserRoute::Session(sid, Scope::Read),
            Some(sid) if write => UserRoute::Session(sid, Scope::Run),
            _ => UserRoute::Denied,
        },
        // A user's own conversation threads: list and create, then read or forget one of their own.
        ["v1", "threads"] if read => UserRoute::Open(Scope::Read),
        ["v1", "threads"] if *method == Method::POST => UserRoute::Open(Scope::Run),
        ["v1", "threads", tid] | ["v1", "threads", tid, "messages"]
            if read || *method == Method::DELETE =>
        {
            match (id(tid), *method == Method::DELETE, parts.len()) {
                (Some(tid), false, _) => UserRoute::Thread(tid, Scope::Read),
                (Some(tid), true, 3) => UserRoute::Thread(tid, Scope::Run),
                _ => UserRoute::Denied,
            }
        }
        // A user's own memory (the handlers scope it to the caller): read it, add, edit, delete, forget all, turn it on or off, and decide
        // what an agent proposed.
        ["v1", "memory"] if read => UserRoute::Open(Scope::Read),
        ["v1", "memory"] if matches!(*method, Method::POST | Method::DELETE) => {
            UserRoute::Open(Scope::Run)
        }
        ["v1", "memory", "settings"] if *method == Method::PUT => UserRoute::Open(Scope::Run),
        ["v1", "memory", mid]
            if (*method == Method::PATCH || *method == Method::DELETE) && id(mid).is_some() =>
        {
            UserRoute::Open(Scope::Run)
        }
        ["v1", "memory", mid, "accept" | "reject"]
            if *method == Method::POST && id(mid).is_some() =>
        {
            UserRoute::Open(Scope::Run)
        }
        // A user's own goals: list, create for themselves, read one, and cancel or pause it. Advancing steps by hand and browsing under a goal stay
        // with the operator (not listed).
        ["v1", "goals"] if read => UserRoute::Open(Scope::Read),
        ["v1", "goals"] if *method == Method::POST => UserRoute::Open(Scope::Run),
        ["v1", "goals", gid] if read => match id(gid) {
            Some(gid) => UserRoute::Goal(gid, Scope::Read),
            None => UserRoute::Denied,
        },
        ["v1", "goals", gid] if *method == Method::PATCH => match id(gid) {
            Some(gid) => UserRoute::Goal(gid, Scope::Run),
            None => UserRoute::Denied,
        },
        // What was done after you approved: a person reads their own receipts (the handler scopes them).
        ["v1", "receipts"] if read => UserRoute::Open(Scope::Read),
        // A person's own outside-account connections (write-only tokens; the handlers scope them to the caller).
        ["v1", "connections"] if read => UserRoute::Open(Scope::Read),
        ["v1", "connections", name]
            if (*method == Method::PUT || *method == Method::DELETE)
                && !name.is_empty()
                && name.len() <= 32
                && name
                    .bytes()
                    .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-') =>
        {
            UserRoute::Open(Scope::Run)
        }
        ["v1", "whoami"] if read => UserRoute::Open(Scope::Read),
        // Ask for a plan, and accept or reject the one an agent proposed (nothing in a proposal runs until the person accepts it).
        ["v1", "goals", gid, "plan"] if *method == Method::POST => match id(gid) {
            Some(gid) => UserRoute::Goal(gid, Scope::Run),
            None => UserRoute::Denied,
        },
        ["v1", "goals", gid, "plan", "accept" | "reject"] if *method == Method::POST => {
            match id(gid) {
                Some(gid) => UserRoute::Goal(gid, Scope::Run),
                None => UserRoute::Denied,
            }
        }
        // Suggestions agents made to this person: read, turn on or off, accept (makes a goal) or dismiss.
        ["v1", "suggestions"] if read => UserRoute::Open(Scope::Read),
        ["v1", "suggestions", "settings"] if *method == Method::PUT => UserRoute::Open(Scope::Run),
        ["v1", "suggestions", sid, "accept" | "dismiss"]
            if *method == Method::POST && id(sid).is_some() =>
        {
            UserRoute::Open(Scope::Run)
        }
        ["v1", "approvals"] if read => UserRoute::Open(Scope::Read),
        ["v1", "approvals", aid] if *method == Method::POST => match id(aid) {
            Some(aid) => UserRoute::Approval(aid, Scope::Approve),
            None => UserRoute::Denied,
        },
        ["v1", "artifacts"] if read => UserRoute::Open(Scope::Read),
        ["v1", "artifacts", a] if read => match id(a) {
            Some(a) => UserRoute::Artifacts(vec![a], Scope::Read),
            None => UserRoute::Denied,
        },
        ["v1", "artifacts", a, "diff", b] if read => match (id(a), id(b)) {
            (Some(a), Some(b)) => UserRoute::Artifacts(vec![a, b], Scope::Read),
            _ => UserRoute::Denied,
        },
        // A user may list their own enrolled devices. Enrolling and removing them is an operator
        // action, so a stolen user token cannot add its own key.
        ["v1", "users", uid, "devices"] if read => {
            UserRoute::OwnUser((*uid).to_string(), Scope::Read)
        }
        // Auto-approval policies are set by the operator (the gateway), like device enrolment, so a stolen user token
        // cannot widen what skips a person's click. A user may read their own.
        ["v1", "users", uid, "changeset-policies"] if read => {
            UserRoute::OwnUser((*uid).to_string(), Scope::Read)
        }
        ["v1", "audit"] | ["v1", "usage"] | ["v1", "inbox"] | ["v1", "demos"] if read => {
            UserRoute::Open(Scope::Read)
        }
        ["v1", "demos", _] if *method == Method::POST => UserRoute::Open(Scope::Run),
        // A chat client's run: starts or steers the caller's own session, like POST /v1/sessions.
        ["v1", "agui"] if *method == Method::POST => UserRoute::Open(Scope::Run),
        _ => UserRoute::Denied,
    }
}

/// Does session `id` belong to `user`?
pub async fn owns_session(state: &AppState, user: &str, id: Uuid) -> bool {
    state
        .store
        .get_session(id)
        .await
        .is_some_and(|s| s.user_id.as_deref() == Some(user))
}

/// Does thread `id` belong to `user`?
pub async fn owns_thread(state: &AppState, user: &str, id: Uuid) -> bool {
    state
        .store
        .threads
        .get(id)
        .await
        .is_some_and(|t| t.user_id == user)
}

/// Does goal `id` belong to `user`?
pub async fn owns_goal(state: &AppState, user: &str, id: Uuid) -> bool {
    state
        .store
        .get_goal(id)
        .await
        .is_some_and(|g| g.user_id.as_deref() == Some(user))
}

/// Does approval `id` belong to `user` (through its session)?
pub async fn owns_approval(state: &AppState, user: &str, id: Uuid) -> bool {
    match state.store.get_approval(id).await {
        Some(a) => owns_session(state, user, a.session_id).await,
        None => false,
    }
}

/// Does artifact `id` belong to `user`? An artifact with no session belongs to nobody.
pub async fn owns_artifact(state: &AppState, user: &str, id: Uuid) -> bool {
    match state
        .store
        .get_artifact(id)
        .await
        .and_then(|a| a.session_id)
    {
        Some(sid) => owns_session(state, user, sid).await,
        None => false,
    }
}

/// The ids of every session that belongs to `user`.
pub async fn session_ids_of(state: &AppState, user: &str) -> std::collections::HashSet<Uuid> {
    state
        .store
        .list_sessions()
        .await
        .into_iter()
        .filter(|s| s.user_id.as_deref() == Some(user))
        .map(|s| s.id)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Duration;

    fn key() -> Vec<u8> {
        signing_key(Some(&crate::fixture::text("operator-token")), None).unwrap()
    }

    #[test]
    fn a_minted_token_verifies_and_carries_its_claims() {
        let now = Utc::now();
        let (t, claims) = mint(&key(), "ana", &[Scope::Read, Scope::Approve], 600, now).unwrap();
        assert!(t.starts_with("kut1."));
        let got = verify(&key(), &t, now + Duration::seconds(10), None).unwrap();
        assert_eq!(got, claims);
        assert_eq!(got.user_id, "ana");
        assert_eq!(got.scopes, vec![Scope::Read, Scope::Approve]);
    }

    #[test]
    fn expired_tampered_and_foreign_tokens_are_refused() {
        let now = Utc::now();
        let (t, _) = mint(&key(), "ana", &[Scope::Read], 60, now).unwrap();
        assert_eq!(
            verify(&key(), &t, now + Duration::seconds(61), None),
            Err("token expired")
        );
        // Change the user in the payload: the signature no longer matches.
        let (body, sig) = t.strip_prefix("kut1.").unwrap().rsplit_once('.').unwrap();
        let mut claims: UserClaims = serde_json::from_slice(
            &base64::engine::general_purpose::URL_SAFE_NO_PAD
                .decode(body)
                .unwrap(),
        )
        .unwrap();
        claims.user_id = "bob".into();
        let forged = base64::engine::general_purpose::URL_SAFE_NO_PAD
            .encode(serde_json::to_vec(&claims).unwrap());
        assert_eq!(
            verify(&key(), &format!("kut1.{forged}.{sig}"), now, None),
            Err("bad signature")
        );
        // Another shard's key.
        let other = signing_key(Some(&crate::fixture::text("other-operator")), None).unwrap();
        assert_eq!(verify(&other, &t, now, None), Err("bad signature"));
        for junk in ["", "kut1.", "kut1.a.b", "operator-token", "kut1.!!.00"] {
            assert!(verify(&key(), junk, now, None).is_err(), "{junk}");
        }
    }

    #[test]
    fn revoking_a_user_cuts_off_tokens_issued_before_the_floor() {
        let now = Utc::now();
        let (t, _) = mint(&key(), "ana", &[Scope::Read], 600, now).unwrap();
        assert!(verify(&key(), &t, now, Some(now - Duration::seconds(5))).is_ok());
        assert_eq!(
            verify(&key(), &t, now, Some(now + Duration::seconds(1))),
            Err("token revoked")
        );
        // A token minted after the floor works again.
        let (fresh, _) = mint(
            &key(),
            "ana",
            &[Scope::Read],
            600,
            now + Duration::seconds(2),
        )
        .unwrap();
        assert!(verify(
            &key(),
            &fresh,
            now + Duration::seconds(2),
            Some(now + Duration::seconds(1))
        )
        .is_ok());
    }

    #[test]
    fn minting_is_bounded_and_needs_a_valid_user_and_a_scope() {
        let now = Utc::now();
        assert!(mint(&key(), "Ana Silva", &[Scope::Read], 600, now).is_err());
        assert!(mint(&key(), "", &[Scope::Read], 600, now).is_err());
        assert!(mint(&key(), "ana", &[], 600, now).is_err());
        let (_, c) = mint(&key(), "ana", &[Scope::Read], u64::MAX, now).unwrap();
        assert_eq!(c.expires_at - c.issued_at, MAX_TTL_SECONDS as i64);
        let (_, c) = mint(&key(), "ana", &[Scope::Read], 1, now).unwrap();
        assert_eq!(c.expires_at - c.issued_at, 60);
    }

    #[test]
    fn the_key_needs_a_secret_or_an_operator_token() {
        assert!(signing_key(None, None).is_none());
        assert!(signing_key(Some(&crate::fixture::text("")), None).is_none());
        assert!(signing_key(None, Some("short")).is_none());
        let secret = crate::fixture::text("a-long-enough-secret");
        assert_ne!(
            signing_key(None, Some(&secret)),
            signing_key(Some(&secret), None)
        );
    }

    #[test]
    fn users_reach_only_the_listed_routes() {
        let sid = Uuid::new_v4();
        let aid = Uuid::new_v4();
        let cases: Vec<(Method, String, UserRoute)> = vec![
            (
                Method::GET,
                "/v1/sessions".into(),
                UserRoute::Open(Scope::Read),
            ),
            (
                Method::POST,
                "/v1/sessions".into(),
                UserRoute::Open(Scope::Run),
            ),
            (
                Method::GET,
                format!("/v1/sessions/{sid}/cockpit"),
                UserRoute::Session(sid, Scope::Read),
            ),
            (
                Method::GET,
                format!("/v1/sessions/{sid}/report"),
                UserRoute::Session(sid, Scope::Read),
            ),
            (
                Method::POST,
                format!("/v1/sessions/{sid}/cancel"),
                UserRoute::Session(sid, Scope::Run),
            ),
            (
                Method::POST,
                format!("/v1/sessions/{sid}/host-recover"),
                UserRoute::Denied,
            ),
            (
                Method::POST,
                format!("/v1/sessions/{sid}/changesets/{aid}/restore"),
                UserRoute::Denied,
            ),
            (
                Method::GET,
                "/v1/approvals".into(),
                UserRoute::Open(Scope::Read),
            ),
            (
                Method::POST,
                format!("/v1/approvals/{aid}"),
                UserRoute::Approval(aid, Scope::Approve),
            ),
            (Method::POST, "/v1/approvals".into(), UserRoute::Denied),
            (
                Method::GET,
                "/v1/suggestions".into(),
                UserRoute::Open(Scope::Read),
            ),
            (
                Method::PUT,
                "/v1/suggestions/settings".into(),
                UserRoute::Open(Scope::Run),
            ),
            (
                Method::POST,
                "/v1/suggestions/00000000-0000-4000-8000-000000000000/accept".into(),
                UserRoute::Open(Scope::Run),
            ),
            (
                Method::POST,
                "/v1/suggestions/00000000-0000-4000-8000-000000000000/dismiss".into(),
                UserRoute::Open(Scope::Run),
            ),
            (Method::POST, "/v1/suggestions".into(), UserRoute::Denied),
            (
                Method::POST,
                "/v1/suggestions/not-an-id/accept".into(),
                UserRoute::Denied,
            ),
            (
                Method::GET,
                "/v1/receipts".into(),
                UserRoute::Open(Scope::Read),
            ),
            (Method::POST, "/v1/receipts".into(), UserRoute::Denied),
            (Method::DELETE, "/v1/receipts".into(), UserRoute::Denied),
            (
                Method::GET,
                "/v1/connections".into(),
                UserRoute::Open(Scope::Read),
            ),
            (
                Method::PUT,
                "/v1/connections/google".into(),
                UserRoute::Open(Scope::Run),
            ),
            (
                Method::DELETE,
                "/v1/connections/google".into(),
                UserRoute::Open(Scope::Run),
            ),
            (
                Method::GET,
                "/v1/connections/google".into(),
                UserRoute::Denied,
            ),
            (
                Method::PUT,
                "/v1/connections/Bad_Name".into(),
                UserRoute::Denied,
            ),
            (Method::POST, "/v1/connections".into(), UserRoute::Denied),
            (
                Method::GET,
                "/v1/threads".into(),
                UserRoute::Open(Scope::Read),
            ),
            (
                Method::POST,
                "/v1/threads".into(),
                UserRoute::Open(Scope::Run),
            ),
            (
                Method::GET,
                format!("/v1/threads/{aid}"),
                UserRoute::Thread(aid, Scope::Read),
            ),
            (
                Method::GET,
                format!("/v1/threads/{aid}/messages"),
                UserRoute::Thread(aid, Scope::Read),
            ),
            (
                Method::DELETE,
                format!("/v1/threads/{aid}"),
                UserRoute::Thread(aid, Scope::Run),
            ),
            // Only reading a thread's messages exists; nothing may write or delete through that path.
            (
                Method::DELETE,
                format!("/v1/threads/{aid}/messages"),
                UserRoute::Denied,
            ),
            (
                Method::POST,
                format!("/v1/threads/{aid}/messages"),
                UserRoute::Denied,
            ),
            (
                Method::GET,
                "/v1/threads/not-a-uuid".into(),
                UserRoute::Denied,
            ),
            (
                Method::GET,
                "/v1/memory".into(),
                UserRoute::Open(Scope::Read),
            ),
            (
                Method::POST,
                "/v1/memory".into(),
                UserRoute::Open(Scope::Run),
            ),
            (
                Method::DELETE,
                "/v1/memory".into(),
                UserRoute::Open(Scope::Run),
            ),
            (
                Method::PUT,
                "/v1/memory/settings".into(),
                UserRoute::Open(Scope::Run),
            ),
            (
                Method::PATCH,
                format!("/v1/memory/{aid}"),
                UserRoute::Open(Scope::Run),
            ),
            (
                Method::DELETE,
                format!("/v1/memory/{aid}"),
                UserRoute::Open(Scope::Run),
            ),
            (
                Method::POST,
                format!("/v1/memory/{aid}/accept"),
                UserRoute::Open(Scope::Run),
            ),
            (
                Method::POST,
                format!("/v1/memory/{aid}/reject"),
                UserRoute::Open(Scope::Run),
            ),
            (
                Method::PATCH,
                "/v1/memory/not-a-uuid".into(),
                UserRoute::Denied,
            ),
            (
                Method::POST,
                format!("/v1/memory/{aid}/other"),
                UserRoute::Denied,
            ),
            (Method::GET, format!("/v1/memory/{aid}"), UserRoute::Denied),
            (Method::PUT, "/v1/memory".into(), UserRoute::Denied),
            (
                Method::GET,
                "/v1/goals".into(),
                UserRoute::Open(Scope::Read),
            ),
            (
                Method::GET,
                "/v1/whoami".into(),
                UserRoute::Open(Scope::Read),
            ),
            (Method::POST, "/v1/whoami".into(), UserRoute::Denied),
            (
                Method::POST,
                "/v1/goals".into(),
                UserRoute::Open(Scope::Run),
            ),
            (
                Method::GET,
                format!("/v1/goals/{aid}"),
                UserRoute::Goal(aid, Scope::Read),
            ),
            (
                Method::PATCH,
                format!("/v1/goals/{aid}"),
                UserRoute::Goal(aid, Scope::Run),
            ),
            (
                Method::PATCH,
                "/v1/goals/not-a-uuid".into(),
                UserRoute::Denied,
            ),
            (
                Method::POST,
                format!("/v1/goals/{aid}/advance"),
                UserRoute::Denied,
            ),
            (
                Method::POST,
                format!("/v1/goals/{aid}/browse"),
                UserRoute::Denied,
            ),
            (
                Method::DELETE,
                format!("/v1/goals/{aid}"),
                UserRoute::Denied,
            ),
            (Method::PUT, format!("/v1/goals/{aid}"), UserRoute::Denied),
            (Method::DELETE, "/v1/goals".into(), UserRoute::Denied),
            (Method::PUT, "/v1/goals".into(), UserRoute::Denied),
            (Method::PUT, format!("/v1/memory/{aid}"), UserRoute::Denied),
            (
                Method::GET,
                format!("/v1/artifacts/{aid}"),
                UserRoute::Artifacts(vec![aid], Scope::Read),
            ),
            (
                Method::GET,
                format!("/v1/artifacts/{aid}/diff/{sid}"),
                UserRoute::Artifacts(vec![aid, sid], Scope::Read),
            ),
            (Method::POST, "/v1/artifacts".into(), UserRoute::Denied),
            (
                Method::GET,
                "/v1/audit".into(),
                UserRoute::Open(Scope::Read),
            ),
            (
                Method::GET,
                "/v1/demos".into(),
                UserRoute::Open(Scope::Read),
            ),
            (
                Method::POST,
                "/v1/demos/csv-clean".into(),
                UserRoute::Open(Scope::Run),
            ),
            (Method::POST, "/v1/demos".into(), UserRoute::Denied),
            // AG-UI: a chat client may start or steer its own run; nothing else on that path
            (Method::POST, "/v1/agui".into(), UserRoute::Open(Scope::Run)),
            (Method::GET, "/v1/agui".into(), UserRoute::Denied),
            (Method::DELETE, "/v1/agui".into(), UserRoute::Denied),
            (Method::POST, "/v1/agui/x".into(), UserRoute::Denied),
            (
                Method::DELETE,
                "/v1/demos/csv-clean".into(),
                UserRoute::Denied,
            ),
        ];
        for (m, p, want) in cases {
            assert_eq!(user_route(&m, &p), want, "{m} {p}");
        }
        // Everything else is closed by default, including the routes an operator uses.
        for p in [
            "/v1/agents",
            "/v1/keep/status",
            "/v1/merion/status",
            "/v1/user-tokens",
            "/v1/triggers",
            "/v1/model-grants",
            "/v1/model-usage",
            "/v1/vault/status",
            "/v1/skills",
            "/v1/schedules",
            "/v1/webhooks",
            "/v1/workstations/a/b",
            "/v1/export/audit",
            "/v1/export-tokens",
            "/mcp",
            "/v1/packs",
        ] {
            for m in [Method::GET, Method::POST, Method::DELETE, Method::PUT] {
                assert_eq!(user_route(&m, p), UserRoute::Denied, "{m} {p}");
            }
        }
        // A path that only looks like a session id is refused.
        assert_eq!(
            user_route(&Method::GET, "/v1/sessions/not-a-uuid"),
            UserRoute::Denied
        );
        // Devices: read your own list; enrolling or removing one is not for users.
        assert_eq!(
            user_route(&Method::GET, "/v1/users/ana/devices"),
            UserRoute::OwnUser("ana".into(), Scope::Read)
        );
        for m in [Method::POST, Method::DELETE, Method::PUT] {
            for p in [
                "/v1/users/ana/devices",
                "/v1/users/ana/devices/phone",
                "/v1/users/ana/revoke-tokens",
                "/v1/users/ana/changeset-policies",
                "/v1/users/ana/changeset-policies/agent",
            ] {
                assert_eq!(user_route(&m, p), UserRoute::Denied, "{m} {p}");
            }
        }
    }
}
