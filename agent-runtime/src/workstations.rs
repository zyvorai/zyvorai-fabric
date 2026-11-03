// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: Apache-2.0

//! Always-on per-user workstations.
//!
//! A session is a task; a workstation is the promise that a user's agent VM is
//! there. An operator declares one (`PUT /v1/workstations/{agent}/{user_id}`,
//! only for agents deployed with `persistent: true`) and this loop keeps a
//! session running for it: when the session ends, fails, or expires, it starts
//! another, backing off exponentially while starts keep failing. The user's home
//! volume (`home_volume.per_user`) is what carries state across restarts.

use crate::{
    app::{self, ApiError},
    model::{CreateSessionRequest, SessionRecord, WorkstationRecord},
    AppState,
};
use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use chrono::{DateTime, Utc};
use serde_json::{json, Value};
use std::{sync::Arc, time::Duration};
use uuid::Uuid;

const TICK: Duration = Duration::from_secs(2);
const FIRST_RETRY: Duration = Duration::from_secs(5);
const MAX_RETRY: Duration = Duration::from_secs(300);

#[derive(Debug, PartialEq, Eq)]
pub enum Action {
    /// Its session is alive.
    Running,
    /// A start failed recently; wait for the retry time.
    Backoff,
    /// No live session and no reason to wait.
    Start,
}

pub fn backoff(consecutive_failures: u32) -> Duration {
    let exponent = consecutive_failures.saturating_sub(1).min(16);
    FIRST_RETRY.saturating_mul(1u32 << exponent).min(MAX_RETRY)
}

pub fn decide(
    record: &WorkstationRecord,
    active: Option<&SessionRecord>,
    now: DateTime<Utc>,
) -> Action {
    if active.is_some_and(|s| !s.status.is_terminal()) {
        return Action::Running;
    }
    if record.next_attempt_at.is_some_and(|at| at > now) {
        return Action::Backoff;
    }
    Action::Start
}

pub async fn workstation_loop(state: Arc<AppState>) {
    loop {
        for record in state.store.list_workstations().await {
            reconcile(&state, record, Utc::now()).await;
        }
        tokio::time::sleep(TICK).await;
    }
}

/// Bring one workstation to "a session is running", if it is due.
pub async fn reconcile(state: &Arc<AppState>, mut record: WorkstationRecord, now: DateTime<Utc>) {
    let active = match record.active_session_id {
        Some(id) => state.store.get_session(id).await,
        None => None,
    };
    if decide(&record, active.as_ref(), now) != Action::Start {
        return;
    }
    // The previous session ended on its own: say why, once, in the record.
    if let Some(ended) = &active {
        record.last_error = Some(match &ended.error {
            Some(error) => format!("session {} ended: {error}", ended.id),
            None => format!("session {} ended ({:?})", ended.id, ended.status),
        });
    }
    let attempt = record.restarts.saturating_add(record.consecutive_failures);
    let request = CreateSessionRequest {
        agent: record.agent.clone(),
        input: json!({"workstation": true, "user_id": record.user_id}),
        ttl_seconds: None,
        request_id: Some(format!("workstation:{}:{attempt}", record.id)),
        start_policy: Default::default(),
        parent_session_id: None,
        user_id: Some(record.user_id.clone()),
    };
    match app::create_session(State(state.clone()), Json(request)).await {
        Ok((_, Json(view))) => {
            record.active_session_id = Some(view.id);
            record.restarts = record.restarts.saturating_add(1);
            record.consecutive_failures = 0;
            record.next_attempt_at = None;
        }
        Err(error) => {
            record.consecutive_failures = record.consecutive_failures.saturating_add(1);
            record.last_error = Some(error.message().to_string());
            let delay = chrono::Duration::from_std(backoff(record.consecutive_failures))
                .unwrap_or_else(|_| chrono::Duration::seconds(300));
            record.next_attempt_at = Some(now + delay);
            tracing::warn!(
                agent = %record.agent, user = %record.user_id,
                failures = record.consecutive_failures, error = %error.message(),
                "workstation start failed"
            );
        }
    }
    if let Err(error) = state.store.save_workstation(record).await {
        tracing::error!(%error, "failed to save workstation");
    }
}

pub(crate) async fn put_workstation(
    State(state): State<Arc<AppState>>,
    Path((agent_name, user_id)): Path<(String, String)>,
) -> Result<(StatusCode, Json<WorkstationRecord>), ApiError> {
    let Some(agent) = state.store.get_agent(&agent_name).await else {
        return Err(ApiError::not_found("agent not found"));
    };
    if !agent.manifest.persistent {
        return Err(ApiError::conflict(
            "agent was not deployed with persistent: true",
        ));
    }
    crate::app::check_session_user(&agent, Some(&user_id))?;
    if let Some(existing) = state.store.find_workstation(&agent_name, &user_id).await {
        return Ok((StatusCode::OK, Json(existing)));
    }
    let record = WorkstationRecord {
        id: Uuid::new_v4(),
        agent: agent_name,
        user_id,
        created_at: Utc::now(),
        active_session_id: None,
        restarts: 0,
        consecutive_failures: 0,
        last_error: None,
        next_attempt_at: None,
    };
    state
        .store
        .save_workstation(record.clone())
        .await
        .map_err(ApiError::internal)?;
    Ok((StatusCode::CREATED, Json(record)))
}

pub(crate) async fn list_workstations(State(state): State<Arc<AppState>>) -> Json<Value> {
    Json(json!({"items": state.store.list_workstations().await}))
}

pub(crate) async fn get_workstation(
    State(state): State<Arc<AppState>>,
    Path((agent, user_id)): Path<(String, String)>,
) -> Result<Json<WorkstationRecord>, ApiError> {
    state
        .store
        .find_workstation(&agent, &user_id)
        .await
        .map(Json)
        .ok_or_else(|| ApiError::not_found("workstation not found"))
}

/// Stop keeping a workstation up, and cancel the session it has running. The
/// user's home volume is left alone.
pub(crate) async fn delete_workstation(
    State(state): State<Arc<AppState>>,
    Path((agent, user_id)): Path<(String, String)>,
) -> Result<StatusCode, ApiError> {
    let Some(record) = state.store.find_workstation(&agent, &user_id).await else {
        return Err(ApiError::not_found("workstation not found"));
    };
    state
        .store
        .delete_workstation(record.id)
        .await
        .map_err(ApiError::internal)?;
    if let Some(id) = record.active_session_id {
        if state
            .store
            .get_session(id)
            .await
            .is_some_and(|s| !s.status.is_terminal())
        {
            let _ = app::cancel_session(State(state.clone()), Path(id)).await;
        }
    }
    Ok(StatusCode::NO_CONTENT)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        egress::ask_tests::{manifest, state_and_session_cfg},
        model::{DeployAgentRequest, EgressMode, SessionStatus},
    };
    use base64::Engine;

    fn record(agent: &str) -> WorkstationRecord {
        WorkstationRecord {
            id: Uuid::new_v4(),
            agent: agent.into(),
            user_id: "alice".into(),
            created_at: Utc::now(),
            active_session_id: None,
            restarts: 0,
            consecutive_failures: 0,
            last_error: None,
            next_attempt_at: None,
        }
    }

    fn session(status: SessionStatus) -> SessionRecord {
        let now = Utc::now();
        SessionRecord {
            id: Uuid::new_v4(),
            agent: "a".into(),
            agent_version: "v".into(),
            sandbox_id: Uuid::new_v4(),
            status,
            input: json!({}),
            created_at: now,
            updated_at: now,
            last_event_seq: 0,
            guest_event_cursor: 0,
            request_id: None,
            start_policy: Default::default(),
            start_mode: Default::default(),
            startup_ms: None,
            expires_at: None,
            sandbox_released: false,
            capability_token: "cap".into(),
            error: None,
            parent_session_id: None,
            user_id: Some("alice".into()),
            tainted_by: vec![],
            writes: 0,
            confidential: None,
            agent_paused_reason: None,
            browse: Default::default(),
        }
    }

    #[test]
    fn backoff_doubles_from_five_seconds_and_caps() {
        let secs = |n| backoff(n).as_secs();
        assert_eq!((secs(1), secs(2), secs(3), secs(4)), (5, 10, 20, 40));
        assert_eq!(secs(7), 300);
        assert_eq!(secs(500), 300);
        assert_eq!(secs(0), 5);
    }

    #[test]
    fn decides_between_running_waiting_and_starting() {
        let now = Utc::now();
        let mut r = record("a");
        assert_eq!(decide(&r, None, now), Action::Start);
        assert_eq!(
            decide(&r, Some(&session(SessionStatus::Running)), now),
            Action::Running
        );
        assert_eq!(
            decide(&r, Some(&session(SessionStatus::Creating)), now),
            Action::Running
        );
        // An ended session is replaced.
        assert_eq!(
            decide(&r, Some(&session(SessionStatus::Failed)), now),
            Action::Start
        );
        assert_eq!(
            decide(&r, Some(&session(SessionStatus::Expired)), now),
            Action::Start
        );
        r.next_attempt_at = Some(now + chrono::Duration::seconds(30));
        assert_eq!(decide(&r, None, now), Action::Backoff);
        // A live session wins over a stale backoff.
        assert_eq!(
            decide(&r, Some(&session(SessionStatus::Running)), now),
            Action::Running
        );
        r.next_attempt_at = Some(now - chrono::Duration::seconds(1));
        assert_eq!(decide(&r, None, now), Action::Start);
    }

    async fn persistent_state() -> (Arc<AppState>, String) {
        let (state, session) =
            state_and_session_cfg(|c| c.api_token = Some("operator".into())).await;
        let mut m = manifest(EgressMode::Deny, None);
        m.persistent = true;
        let agent = state
            .store
            .deploy_agent(DeployAgentRequest {
                name: "desk".into(),
                bundle_base64: base64::engine::general_purpose::STANDARD.encode("export default 1"),
                manifest: m,
            })
            .await
            .unwrap();
        let _ = (session, &agent);
        (state, "desk".into())
    }

    #[tokio::test]
    async fn a_failed_start_is_recorded_and_retried_only_after_the_backoff() {
        let (state, agent) = persistent_state().await;
        let stored = record(&agent);
        state.store.save_workstation(stored.clone()).await.unwrap();
        let now = Utc::now();

        // FluxVM is unreachable in tests, so the real create_session path fails.
        reconcile(&state, stored.clone(), now).await;
        let after = state.store.find_workstation(&agent, "alice").await.unwrap();
        assert_eq!(after.consecutive_failures, 1);
        assert!(after.last_error.is_some());
        assert_eq!(after.restarts, 0);
        let due = after.next_attempt_at.unwrap();
        assert!(due > now && due <= now + chrono::Duration::seconds(6));

        // Inside the backoff window nothing is attempted.
        reconcile(&state, after.clone(), now).await;
        assert_eq!(
            state
                .store
                .find_workstation(&agent, "alice")
                .await
                .unwrap()
                .consecutive_failures,
            1
        );

        // Once it is due, it tries again and the delay doubles.
        reconcile(&state, after, due + chrono::Duration::seconds(1)).await;
        let again = state.store.find_workstation(&agent, "alice").await.unwrap();
        assert_eq!(again.consecutive_failures, 2);
        let gap = again.next_attempt_at.unwrap() - (due + chrono::Duration::seconds(1));
        assert!(gap.num_seconds() >= 9, "{gap:?}");
    }

    #[tokio::test]
    async fn a_live_session_is_left_alone() {
        let (state, agent) = persistent_state().await;
        let live = session(SessionStatus::Running);
        state.store.save_session(live.clone()).await.unwrap();
        let mut stored = record(&agent);
        stored.active_session_id = Some(live.id);
        state.store.save_workstation(stored.clone()).await.unwrap();
        reconcile(&state, stored, Utc::now()).await;
        let after = state.store.find_workstation(&agent, "alice").await.unwrap();
        assert_eq!((after.consecutive_failures, after.restarts), (0, 0));
        assert!(after.last_error.is_none());
    }

    #[tokio::test]
    async fn an_ended_session_is_explained_in_the_record() {
        let (state, agent) = persistent_state().await;
        let mut ended = session(SessionStatus::Failed);
        ended.error = Some("guest worker crashed".into());
        state.store.save_session(ended.clone()).await.unwrap();
        let mut stored = record(&agent);
        stored.active_session_id = Some(ended.id);
        state.store.save_workstation(stored.clone()).await.unwrap();
        reconcile(&state, stored, Utc::now()).await;
        // The replacement start also fails (no FluxVM), which overwrites the note,
        // so check the attempt happened rather than the ended-session text.
        let after = state.store.find_workstation(&agent, "alice").await.unwrap();
        assert_eq!(after.consecutive_failures, 1);
    }

    async fn call(
        state: &Arc<AppState>,
        method: &str,
        uri: &str,
        bearer: &str,
    ) -> (StatusCode, Value) {
        use tower::ServiceExt;
        let request = axum::http::Request::builder()
            .method(method)
            .uri(uri)
            .header("authorization", format!("Bearer {bearer}"))
            .body(axum::body::Body::empty())
            .unwrap();
        let response = crate::app::public_router(state.clone())
            .oneshot(request)
            .await
            .unwrap();
        let status = response.status();
        let bytes = axum::body::to_bytes(response.into_body(), 1 << 20)
            .await
            .unwrap();
        (
            status,
            serde_json::from_slice(&bytes).unwrap_or(Value::Null),
        )
    }

    #[tokio::test]
    async fn the_operator_api_manages_workstations() {
        let (state, agent) = persistent_state().await;
        let uri = format!("/v1/workstations/{agent}/alice");
        // The agent's own credential cannot.
        assert_eq!(
            call(&state, "PUT", &uri, "cap").await.0,
            StatusCode::UNAUTHORIZED
        );
        let (created, body) = call(&state, "PUT", &uri, "operator").await;
        assert_eq!(created, StatusCode::CREATED);
        assert_eq!(body["user_id"], "alice");
        // Idempotent.
        assert_eq!(
            call(&state, "PUT", &uri, "operator").await.0,
            StatusCode::OK
        );
        assert_eq!(
            call(&state, "GET", &uri, "operator").await.0,
            StatusCode::OK
        );
        let (_, list) = call(&state, "GET", "/v1/workstations", "operator").await;
        assert_eq!(list["items"].as_array().unwrap().len(), 1);
        // Bad ids and unknown agents are refused.
        assert_eq!(
            call(
                &state,
                "PUT",
                &format!("/v1/workstations/{agent}/Bad%20Id"),
                "operator"
            )
            .await
            .0,
            StatusCode::BAD_REQUEST
        );
        assert_eq!(
            call(&state, "PUT", "/v1/workstations/nope/alice", "operator")
                .await
                .0,
            StatusCode::NOT_FOUND
        );
        assert_eq!(
            call(&state, "DELETE", &uri, "operator").await.0,
            StatusCode::NO_CONTENT
        );
        assert_eq!(
            call(&state, "GET", &uri, "operator").await.0,
            StatusCode::NOT_FOUND
        );
    }

    #[tokio::test]
    async fn only_persistent_agents_can_have_workstations() {
        let (state, _agent) = persistent_state().await;
        state
            .store
            .deploy_agent(DeployAgentRequest {
                name: "task".into(),
                bundle_base64: base64::engine::general_purpose::STANDARD.encode("export default 1"),
                manifest: manifest(EgressMode::Deny, None),
            })
            .await
            .unwrap();
        let (status, _) = call(&state, "PUT", "/v1/workstations/task/alice", "operator").await;
        assert_eq!(status, StatusCode::CONFLICT);
    }
}
