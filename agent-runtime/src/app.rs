// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: Apache-2.0

use crate::{
    audit::AuditPhase,
    authz::{self, Principal, Scope, UserRoute},
    model::{
        ApprovalKind, ApprovalRecord, ApprovalStatus, CreateApprovalRequest, CreateSessionRequest,
        DecideApprovalRequest, DelegateRequest, DeployAgentRequest, EventsQuery,
        GuestEventsResponse, GuestStatusResponse, SessionRecord, SessionStartMode,
        SessionStartPolicy, SessionStatus, SessionView, SteerRequest, WarmPoolReconcileResult,
        WarmPoolView, MAX_EGRESS_APPROVAL_SECONDS, MIN_EGRESS_APPROVAL_SECONDS,
    },
    pool, AppState,
};
use anyhow::{Context, Result};
use axum::{
    body::Bytes,
    extract::{Extension, Path, Query, State},
    http::{HeaderMap, StatusCode},
    middleware::{self, Next},
    response::{
        sse::{Event, KeepAlive, Sse},
        IntoResponse, Response,
    },
    routing::{get, post},
    Json, Router,
};
use chrono::Utc;
use reqwest::Method;
use serde_json::{json, Value};
use std::{collections::HashMap, convert::Infallible, sync::Arc, time::Duration};
use uuid::Uuid;

pub(crate) const WORKER: &[u8] = include_bytes!("worker.mjs");
pub(crate) const HARNESS: &[u8] = include_bytes!("harness.mjs");

/// Upper bound on how long a single FluxVM resume/create call may run while
/// holding a session's per-agent creation lock. Strictly less than the
/// FluxVm HTTP client's own 180s request timeout, so this fires first and
/// deterministically: a hung FluxVM call becomes a bounded, lock-releasing
/// error instead of blocking every future session creation for that agent
/// until the process is restarted.
const SANDBOX_START_TIMEOUT: Duration = Duration::from_secs(120);

/// Upper bound on a single guest_request() call inside provision_guest's
/// health-check retry loop (and the final /run call). Shorter than
/// FluxVmClient's own 180s reqwest timeout, for the same reason
/// SANDBOX_START_TIMEOUT is shorter than it: a single stuck call must not be
/// able to block the retry loop past its own guest_start_timeout_secs
/// deadline, which is only ever checked *between* attempts.
const HEALTH_CHECK_ATTEMPT_TIMEOUT: Duration = Duration::from_secs(15);

/// Create a fresh sandbox for a session, attaching the agent's home volume if
/// it has one. A volume that is still attached elsewhere is a conflict (the
/// caller can retry once the other session ends), not a gateway failure.
/// A `user_id` must be well formed, and is mandatory when the agent's home
/// volume is per user (otherwise every user would share one volume).
pub(crate) fn check_session_user(
    agent: &crate::model::AgentRecord,
    user_id: Option<&str>,
) -> ApiResult<()> {
    if let Some(user) = user_id {
        crate::model::validate_user_id(user).map_err(ApiError::bad_request)?;
    }
    if agent
        .manifest
        .home_volume
        .as_ref()
        .is_some_and(|v| v.per_user)
        && user_id.is_none()
    {
        return Err(ApiError::bad_request(
            "this agent's home volume is per user: user_id is required",
        ));
    }
    Ok(())
}

async fn create_cold_sandbox(
    state: &AppState,
    agent: &crate::model::AgentRecord,
    session_id: Uuid,
    user_id: Option<&str>,
) -> ApiResult<crate::fluxvm::SandboxRecord> {
    let name = format!("agent-{}", &session_id.simple().to_string()[..12]);
    let volumes: Vec<crate::fluxvm::SandboxVolume> = agent
        .manifest
        .home_volume
        .iter()
        .filter_map(|v| {
            Some(crate::fluxvm::SandboxVolume {
                name: agent.manifest.home_volume_for(&agent.name, user_id)?,
                guest_path: v.guest_path.clone(),
            })
        })
        .collect();
    let sandbox = with_start_timeout(state.fluxvm.create_sandbox(
        name,
        &agent.manifest.template,
        None,
        agent.manifest.runtime_port,
        &crate::fluxvm::SandboxOptions {
            volumes: &volumes,
            resources: agent.manifest.resources,
            confidential: agent.manifest.confidential,
            security_profile: state.config.security_profile.as_deref(),
            gpus: agent.manifest.gpus,
        },
    ))
    .await
    .map_err(|error| {
        if !volumes.is_empty() && error.to_string().contains("already attached") {
            ApiError::conflict("the agent's home volume is attached to another sandbox")
        } else if error.to_string().contains("GPU(s) requested") {
            // FluxVM had fewer free GPUs than the agent needs: try again later.
            ApiError::unavailable(format!("no GPU is free for this agent: {error}"))
        } else {
            ApiError::bad_gateway(error)
        }
    })?;
    check_confidential(state, agent, &sandbox).await?;
    check_gpus(state, agent, &sandbox, session_id).await?;
    Ok(sandbox)
}

/// Enforce `gpus` on a freshly created sandbox. An older FluxVM ignores the request and would
/// start a CPU-only cell for an agent that needs a GPU, so a record with fewer devices than asked
/// for is refused and the sandbox deleted. The devices FluxVM assigned are journaled.
async fn check_gpus(
    state: &AppState,
    agent: &crate::model::AgentRecord,
    sandbox: &crate::fluxvm::SandboxRecord,
    session_id: Uuid,
) -> ApiResult<()> {
    let Some(wanted) = agent.manifest.gpus else {
        return Ok(());
    };
    let devices: Vec<String> = sandbox
        .request
        .as_ref()
        .map(|r| r.vfio_devices.clone())
        .unwrap_or_default();
    if devices.len() < usize::from(wanted) {
        let _ = state.fluxvm.delete(sandbox.id).await;
        return Err(ApiError::bad_gateway(format!(
            "this agent needs {wanted} GPU(s) but FluxVM assigned {}; the cell was deleted (does FluxVM support `gpus`?)",
            devices.len()
        )));
    }
    let _ = state
        .store
        .audit
        .append(
            Some(session_id),
            AuditPhase::Performed,
            "keep.cell.gpus",
            Some(agent.name.clone()),
            json!({"devices": devices}),
        )
        .await;
    Ok(())
}

/// Enforce `confidential: required` on a freshly created sandbox. FluxVM refuses
/// a required launch it cannot do; this also refuses when an older FluxVM ignored
/// the request and reported nothing, and deletes the sandbox so it never runs
/// unprotected. `auto` accepts whatever happened and the outcome is recorded.
async fn check_confidential(
    state: &AppState,
    agent: &crate::model::AgentRecord,
    sandbox: &crate::fluxvm::SandboxRecord,
) -> ApiResult<()> {
    if agent.manifest.confidential != crate::model::Confidential::Required {
        return Ok(());
    }
    let active = sandbox.confidential.as_ref().is_some_and(|c| c.active);
    if active {
        return Ok(());
    }
    let reason = sandbox
        .confidential
        .as_ref()
        .map(|c| c.reason.clone())
        .unwrap_or_else(|| "FluxVM did not report a confidential status (too old?)".into());
    let _ = state.fluxvm.delete(sandbox.id).await;
    Err(ApiError::unavailable(format!(
        "confidential VM required but not active: {reason}"
    )))
}

async fn with_start_timeout<T>(
    fut: impl std::future::Future<Output = anyhow::Result<T>>,
) -> anyhow::Result<T> {
    with_timeout(SANDBOX_START_TIMEOUT, fut).await
}

async fn with_timeout<T>(
    duration: Duration,
    fut: impl std::future::Future<Output = anyhow::Result<T>>,
) -> anyhow::Result<T> {
    match tokio::time::timeout(duration, fut).await {
        Ok(result) => result,
        Err(_) => Err(anyhow::anyhow!(
            "FluxVM did not respond to a sandbox resume/create within {}s",
            duration.as_secs()
        )),
    }
}

#[derive(Debug)]
pub(crate) struct ApiError {
    status: StatusCode,
    message: String,
}

impl ApiError {
    pub(crate) fn message(&self) -> &str {
        &self.message
    }
    pub(crate) fn status(&self) -> StatusCode {
        self.status
    }
    pub(crate) fn bad_request(e: impl std::fmt::Display) -> Self {
        Self {
            status: StatusCode::BAD_REQUEST,
            message: e.to_string(),
        }
    }
    pub(crate) fn not_found(message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::NOT_FOUND,
            message: message.into(),
        }
    }
    pub(crate) fn conflict(message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::CONFLICT,
            message: message.into(),
        }
    }
    pub(crate) fn bad_gateway(e: impl std::fmt::Display) -> Self {
        Self {
            status: StatusCode::BAD_GATEWAY,
            message: e.to_string(),
        }
    }
    fn unavailable(message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::SERVICE_UNAVAILABLE,
            message: message.into(),
        }
    }
    pub(crate) fn too_many(message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::TOO_MANY_REQUESTS,
            message: message.into(),
        }
    }
    pub(crate) fn unauthorized(message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::UNAUTHORIZED,
            message: message.into(),
        }
    }
    pub(crate) fn forbidden(e: impl std::fmt::Display) -> Self {
        Self {
            status: StatusCode::FORBIDDEN,
            message: e.to_string(),
        }
    }
    pub(crate) fn internal(e: impl std::fmt::Display) -> Self {
        Self {
            status: StatusCode::INTERNAL_SERVER_ERROR,
            message: e.to_string(),
        }
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        (self.status, Json(json!({"error": self.message}))).into_response()
    }
}

pub(crate) type ApiResult<T> = Result<T, ApiError>;

pub fn public_router(state: Arc<AppState>) -> Router {
    let protected = Router::new()
        .route("/v1/agents", get(list_agents).post(deploy_agent))
        .route("/v1/agents/{name}", get(get_agent))
        .route(
            "/v1/agents/{name}/warm-pool",
            get(get_warm_pool).post(reconcile_warm_pool),
        )
        .route(
            "/v1/sessions",
            get(list_sessions).post(create_session_route),
        )
        // AG-UI: a chat client's run becomes a Keep session (see agui.rs). It cannot approve or deny anything.
        .route("/v1/agui", post(crate::agui::agui_run))
        .route("/v1/sessions/{id}", get(get_session).delete(delete_session))
        .route("/v1/sessions/{id}/steer", post(steer_session))
        .route("/v1/sessions/{id}/cancel", post(cancel_session))
        .route("/v1/sessions/{id}/untaint", post(untaint_session))
        .route("/v1/sessions/{id}/host-recover", post(host_recover_session))
        .route(
            "/v1/sessions/{id}/agent-pause",
            post(crate::browser::agent_pause),
        )
        .route(
            "/v1/sessions/{id}/agent-resume",
            post(crate::browser::agent_resume),
        )
        .route(
            "/v1/sessions/{id}/browser/view",
            get(crate::browser::browser_view),
        )
        .route(
            "/v1/sessions/{id}/browser/screenshot",
            get(crate::browser::browser_screenshot),
        )
        .route(
            "/v1/sessions/{id}/browser/screencast",
            get(crate::browser::browser_screencast),
        )
        .route(
            "/v1/sessions/{id}/browser/tool",
            post(crate::browser::browser_tool),
        )
        .route(
            "/v1/sessions/{id}/browser/fill-secret",
            post(crate::browser::browser_fill_secret),
        )
        .route(
            "/v1/sessions/{id}/browser/script",
            get(crate::browser::browse_script),
        )
        .route(
            "/v1/sessions/{id}/browser/checkout",
            get(crate::browser::browse_checkout),
        )
        .route(
            "/v1/sessions/{id}/browser/profile",
            get(crate::browser::profile_inspect),
        )
        .route(
            "/v1/sessions/{id}/browser/doctor",
            get(crate::browser::browser_doctor),
        )
        .route(
            "/v1/sessions/{id}/browser/{*path}",
            get(crate::browser::devtools),
        )
        .route(
            "/v1/workstations",
            get(crate::workstations::list_workstations),
        )
        .route(
            "/v1/workstations/{agent}/{user_id}",
            get(crate::workstations::get_workstation)
                .put(crate::workstations::put_workstation)
                .delete(crate::workstations::delete_workstation),
        )
        .route("/v1/sessions/{id}/hibernate", post(hibernate_session))
        .route("/v1/sessions/{id}/resume", post(resume_session))
        .route("/v1/sessions/{id}/events", get(stream_events))
        .route("/v1/sessions/{id}/delegate", post(delegate_session))
        .route(
            "/v1/schedules",
            get(crate::schedules::list_schedules).post(crate::schedules::create_schedule),
        )
        .route(
            "/v1/schedules/{id}",
            axum::routing::delete(crate::schedules::delete_schedule),
        )
        .route(
            "/v1/webhooks",
            get(crate::schedules::list_webhooks).post(crate::schedules::create_webhook),
        )
        .route(
            "/v1/webhooks/{id}",
            axum::routing::delete(crate::schedules::delete_webhook),
        )
        .route(
            "/v1/loops",
            get(crate::schedules::list_loops).post(crate::schedules::create_loop),
        )
        .route(
            "/v1/loops/{id}",
            axum::routing::delete(crate::schedules::delete_loop),
        )
        .route(
            "/v1/goals",
            get(crate::goals::list_goals).post(crate::goals::create_goal_route),
        )
        .route(
            "/v1/goals/{id}",
            get(crate::goals::get_goal_route).patch(crate::goals::patch_goal_route),
        )
        .route("/v1/goals/{id}/advance", post(crate::goals::advance_step))
        .route(
            "/v1/goals/{id}/plan",
            post(crate::goal_plan::start_planning),
        )
        .route(
            "/v1/goals/{id}/plan/accept",
            post(crate::goal_plan::accept_plan),
        )
        .route(
            "/v1/goals/{id}/plan/reject",
            post(crate::goal_plan::reject_plan),
        )
        .route(
            "/v1/threads",
            get(crate::threads::list_threads).post(crate::threads::create_thread),
        )
        .route(
            "/v1/threads/{id}",
            get(crate::threads::get_thread).delete(crate::threads::delete_thread),
        )
        .route(
            "/v1/threads/{id}/messages",
            get(crate::threads::thread_messages),
        )
        .route(
            "/v1/memory",
            get(crate::memory::get_memory)
                .post(crate::memory::add_memory)
                .delete(crate::memory::forget_all),
        )
        .route(
            "/v1/memory/settings",
            axum::routing::put(crate::memory::put_settings),
        )
        .route(
            "/v1/memory/{id}",
            axum::routing::patch(crate::memory::patch_memory).delete(crate::memory::delete_memory),
        )
        .route("/v1/memory/{id}/accept", post(crate::memory::accept_memory))
        .route("/v1/memory/{id}/reject", post(crate::memory::reject_memory))
        .route("/v1/suggestions", get(crate::suggestions::list))
        .route(
            "/v1/suggestions/settings",
            axum::routing::put(crate::suggestions::put_settings),
        )
        .route(
            "/v1/suggestions/{id}/accept",
            post(crate::suggestions::accept),
        )
        .route(
            "/v1/suggestions/{id}/dismiss",
            post(crate::suggestions::dismiss),
        )
        .route("/v1/goals/{id}/browse", post(crate::goals::goal_browse))
        .route("/v1/keep/status", get(crate::demos::keep_status))
        .route(
            "/v1/demos",
            get(crate::demos::demo_list).post(crate::demos::demo_save),
        )
        .route(
            "/v1/demos/{id}",
            post(crate::demos::demo_run)
                .layer(axum::extract::DefaultBodyLimit::max(
                    crate::demos::MAX_BATCH_BYTES + 1024 * 1024,
                ))
                .delete(crate::demos::demo_delete),
        )
        .route("/v1/user-tokens", post(mint_user_token))
        .route("/v1/users/{id}/revoke-tokens", post(revoke_user_tokens))
        .route(
            "/v1/users/{id}/devices",
            get(list_devices).post(enroll_device),
        )
        .route(
            "/v1/users/{id}/devices/{device}",
            axum::routing::delete(remove_device),
        )
        .route("/v1/usage", get(usage_route))
        .route("/v1/inbox", get(inbox))
        .route("/v1/receipts", get(crate::receipts::list_receipts))
        .route("/v1/connections", get(crate::connections::list_connections))
        .route(
            "/v1/connections/{name}",
            axum::routing::put(crate::connections::put_connection)
                .delete(crate::connections::delete_connection),
        )
        .route("/v1/whoami", get(whoami))
        .route("/v1/model-grants", get(crate::model_call::list_grants))
        .route(
            "/v1/model-grants/{key}",
            axum::routing::delete(crate::model_call::revoke_grant),
        )
        .route(
            "/v1/triggers",
            get(crate::triggers::list_triggers).post(crate::triggers::create_trigger),
        )
        .route(
            "/v1/triggers/{id}",
            axum::routing::delete(crate::triggers::delete_trigger),
        )
        .route(
            "/v1/artifacts",
            get(crate::goals::list_artifacts).post(crate::goals::create_artifact),
        )
        .route("/v1/artifacts/{id}", get(crate::goals::get_artifact))
        .route(
            "/v1/artifacts/{a}/diff/{b}",
            get(crate::goals::diff_artifacts),
        )
        .route("/v1/approvals", get(list_approvals).post(create_approval))
        .route("/v1/approvals/{id}", post(decide_approval))
        .route("/v1/sessions/{id}/speculate", post(speculate_session))
        .route("/v1/audit", get(list_audit))
        .route("/v1/export/audit", get(export_audit))
        .route(
            "/v1/agents/{name}/policy",
            get(get_agent_policy).put(put_agent_policy),
        )
        .route(
            "/v1/agents/{name}/policy-suggestions",
            get(get_policy_suggestions),
        )
        .route(
            "/v1/agents/{name}/binary-pins",
            get(list_binary_pins).delete(clear_binary_pins),
        )
        .route("/v1/sessions/{id}/cockpit", get(session_cockpit))
        .route("/v1/export-tokens", post(mint_export_token))
        .route("/v1/vault/status", get(vault_status))
        .route("/v1/vault/unwrap-tokens", post(mint_unwrap_token))
        .route("/v1/vault/unwrap", post(unlock_vault))
        .route("/v1/vault/user-held/challenge", post(user_held_challenge))
        .route("/v1/vault/user-held/complete", post(user_held_complete))
        .route("/v1/agents/{name}/pack", get(pack_agent))
        .route("/v1/skills", get(list_skills).post(publish_skill))
        .route("/v1/skills/{name}", get(get_skill).delete(delete_skill))
        .route("/mcp", post(crate::mcp::handle))
        .route_layer(middleware::from_fn_with_state(state.clone(), api_auth));

    Router::new()
        .route("/healthz", get(|| async { Json(json!({"ok": true})) }))
        .route("/v1/hooks/{id}", post(crate::schedules::webhook_ingress))
        .route(
            "/v1/triggers/{id}/hook",
            post(crate::triggers::trigger_hook).layer(axum::extract::DefaultBodyLimit::max(
                crate::demos::MAX_BATCH_BYTES,
            )),
        )
        .route("/keep/cockpit", get(cockpit_page))
        .route("/keep/browser", get(crate::browser::browser_page))
        .merge(protected)
        .with_state(state)
}

async fn api_auth(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    mut request: axum::extract::Request,
    next: Next,
) -> Response {
    let Some(expected) = state.config.api_token.as_deref() else {
        request.extensions_mut().insert(Principal::Operator);
        return next.run(request).await;
    };
    let header_tok = headers
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "));
    // Browsers cannot set Authorization on WebSocket; allow ?token= for WS upgrades.
    // Only the operator token may travel in a query string, never a user token.
    let query_tok = request.uri().query().and_then(|q| {
        q.split('&').find_map(|pair| {
            let (k, v) = pair.split_once('=')?;
            (k == "token").then_some(v)
        })
    });
    let unauthorized = |message: &str| {
        (StatusCode::UNAUTHORIZED, Json(json!({ "error": message }))).into_response()
    };
    let forbidden =
        |message: &str| (StatusCode::FORBIDDEN, Json(json!({ "error": message }))).into_response();
    let principal = if header_tok
        .or(query_tok)
        .is_some_and(|v| constant_time_eq(v.as_bytes(), expected.as_bytes()))
    {
        Principal::Operator
    } else if let Some(token) = header_tok.filter(|t| t.starts_with(authz::TOKEN_PREFIX)) {
        let Some(key) = authz::signing_key_for(&state) else {
            return unauthorized("missing or invalid bearer token");
        };
        let now = Utc::now();
        let claims = match authz::verify(&key, token, now, None) {
            Ok(c) => c,
            Err(why) => return unauthorized(&format!("invalid user token: {why}")),
        };
        if state
            .store
            .token_floor(&claims.user_id)
            .await
            .is_some_and(|floor| claims.issued_at < floor.timestamp())
        {
            return unauthorized("invalid user token: token revoked");
        }
        Principal::User {
            id: claims.user_id,
            scopes: claims.scopes,
        }
    } else {
        return unauthorized("missing or invalid bearer token");
    };

    if let Principal::User { id, scopes } = &principal {
        let route = authz::user_route(request.method(), request.uri().path());
        let (needed, allowed) = match &route {
            UserRoute::Denied => return forbidden("this route is not available to user tokens"),
            UserRoute::Open(s) => (*s, true),
            UserRoute::Session(sid, s) => (*s, authz::owns_session(&state, id, *sid).await),
            UserRoute::Approval(aid, s) => (*s, authz::owns_approval(&state, id, *aid).await),
            UserRoute::OwnUser(uid, s) => (*s, uid == id),
            UserRoute::Thread(tid, s) => (*s, authz::owns_thread(&state, id, *tid).await),
            UserRoute::Goal(gid, s) => (*s, authz::owns_goal(&state, id, *gid).await),
            UserRoute::Artifacts(ids, s) => {
                let mut all = true;
                for a in ids {
                    all &= authz::owns_artifact(&state, id, *a).await;
                }
                (*s, all)
            }
        };
        if !scopes.contains(&needed) {
            return forbidden(&format!("this token lacks the {needed:?} scope").to_lowercase());
        }
        // Not yours and not there look the same, so ids cannot be probed.
        if !allowed {
            return (StatusCode::NOT_FOUND, Json(json!({ "error": "not found" }))).into_response();
        }
    }
    request.extensions_mut().insert(principal);
    next.run(request).await
}

async fn deploy_agent(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    body: Bytes,
) -> ApiResult<(StatusCode, Json<crate::model::AgentRecord>)> {
    let signature = headers
        .get("x-keep-manifest-signature")
        .and_then(|value| value.to_str().ok());
    state
        .policy_trust
        .verify_deployment(&body, signature)
        .map_err(ApiError::forbidden)?;
    let mut req: DeployAgentRequest =
        serde_json::from_slice(&body).map_err(ApiError::bad_request)?;
    if req.manifest.template.trim().is_empty() {
        return Err(ApiError::bad_request("manifest.template is required"));
    }
    if req
        .manifest
        .egress_allow_hosts
        .iter()
        .any(|h| h.trim().is_empty())
    {
        return Err(ApiError::bad_request("egress allow hosts may not be empty"));
    }
    if req.manifest.max_concurrent_sessions == Some(0) {
        return Err(ApiError::bad_request(
            "max_concurrent_sessions must be greater than zero",
        ));
    }
    if let Some(seconds) = req.manifest.egress_approval_timeout_seconds {
        if !(MIN_EGRESS_APPROVAL_SECONDS..=MAX_EGRESS_APPROVAL_SECONDS).contains(&seconds) {
            return Err(ApiError::bad_request(format!(
                "egress_approval_timeout_seconds must be between {MIN_EGRESS_APPROVAL_SECONDS} and {MAX_EGRESS_APPROVAL_SECONDS}"
            )));
        }
    }
    if let Err(message) = req.manifest.validate_confidential() {
        return Err(ApiError::bad_request(message));
    }
    if let Err(message) = req.manifest.validate_gpus() {
        return Err(ApiError::bad_request(message));
    }
    if let Err(message) = req.manifest.validate_cell_backend() {
        return Err(ApiError::bad_request(message));
    }
    if let Err(message) = req.manifest.validate_egress_policy() {
        return Err(ApiError::bad_request(message));
    }
    if let Err(message) = req.manifest.validate_model_socket() {
        return Err(ApiError::bad_request(message));
    }
    if let Err(message) = req
        .manifest
        .validate_resources(state.config.max_vcpus, state.config.max_memory_mib)
    {
        return Err(ApiError::bad_request(message));
    }
    if let Err(message) = req.manifest.validate_home_volume(&req.name) {
        return Err(ApiError::bad_request(message));
    }
    if req.manifest.warm_pool_size > 64 {
        return Err(ApiError::bad_request("warm_pool_size may not exceed 64"));
    }
    for credential in &req.manifest.credentials {
        if state.credentials.descriptor(credential).is_none() {
            return Err(ApiError::bad_request(format!(
                "credential '{credential}' is not configured on this Fabric host"
            )));
        }
    }
    // Pin skills to exact versions now, so republishing a skill later never
    // changes what this immutable agent version mounts.
    req.manifest.skills = state
        .store
        .skills
        .pin(
            &req.manifest.skills,
            req.manifest.skill_scope.as_deref(),
            &state.skill_scopes,
        )
        .await
        .map_err(ApiError::bad_request)?;
    let record = state
        .store
        .deploy_agent(req)
        .await
        .map_err(ApiError::bad_request)?;
    if record.manifest.warm_pool_size > 0 {
        let pool_state = state.clone();
        let agent_name = record.name.clone();
        tokio::spawn(async move {
            if let Err(error) = pool::reconcile_agent(&pool_state, &agent_name).await {
                tracing::warn!(agent = %agent_name, %error, "initial warm-pool reconciliation failed");
            }
        });
    }
    Ok((StatusCode::CREATED, Json(record)))
}

async fn list_agents(State(state): State<Arc<AppState>>) -> Json<Value> {
    Json(json!({"items": state.store.list_agents().await}))
}

async fn get_agent(
    State(state): State<Arc<AppState>>,
    Path(name): Path<String>,
) -> ApiResult<Json<crate::model::AgentRecord>> {
    state
        .store
        .get_agent(&name)
        .await
        .map(Json)
        .ok_or_else(|| ApiError::not_found("agent not found"))
}

async fn get_warm_pool(
    State(state): State<Arc<AppState>>,
    Path(name): Path<String>,
) -> ApiResult<Json<WarmPoolView>> {
    if state.store.get_agent(&name).await.is_none() {
        return Err(ApiError::not_found("agent not found"));
    }
    pool::pool_view(&state, &name)
        .await
        .map(Json)
        .map_err(ApiError::internal)
}

async fn reconcile_warm_pool(
    State(state): State<Arc<AppState>>,
    Path(name): Path<String>,
) -> ApiResult<Json<WarmPoolReconcileResult>> {
    if state.store.get_agent(&name).await.is_none() {
        return Err(ApiError::not_found("agent not found"));
    }
    pool::reconcile_agent(&state, &name)
        .await
        .map(Json)
        .map_err(ApiError::bad_gateway)
}

/// `POST /v1/sessions`. A user token can only start a session for itself.
pub(crate) async fn create_session_route(
    State(state): State<Arc<AppState>>,
    Extension(principal): Extension<Principal>,
    Json(mut req): Json<CreateSessionRequest>,
) -> ApiResult<(StatusCode, Json<SessionView>)> {
    if let Some(uid) = principal.user() {
        if req.user_id.as_deref().is_some_and(|u| u != uid) {
            return Err(ApiError::forbidden(
                "a user token can only start its own sessions",
            ));
        }
        req.user_id = Some(uid.to_string());
        crate::usage::check_run_quota(&state, uid, crate::usage::Limits::from_env()).await?;
    }
    create_session(State(state), Json(req)).await
}

pub(crate) async fn create_session(
    State(state): State<Arc<AppState>>,
    Json(req): Json<CreateSessionRequest>,
) -> ApiResult<(StatusCode, Json<SessionView>)> {
    let admission_started = std::time::Instant::now();
    let admitted_at = Utc::now();
    let agent = state
        .store
        .get_agent(&req.agent)
        .await
        .ok_or_else(|| ApiError::not_found("agent not found"))?;

    let request_id = req
        .request_id
        .as_deref()
        .map(str::trim)
        .filter(|v| !v.is_empty())
        .map(str::to_string);
    if let Some(value) = request_id.as_deref() {
        validate_request_id(value)?;
    }
    check_session_user(&agent, req.user_id.as_deref())?;
    let ttl = req.ttl_seconds.or(agent.manifest.ttl_seconds);
    let start_policy = req.start_policy;
    let expires_at = match ttl {
        Some(seconds) => {
            let seconds = i64::try_from(seconds)
                .map_err(|_| ApiError::bad_request("ttl_seconds is too large"))?;
            Some(
                admitted_at
                    .checked_add_signed(chrono::Duration::seconds(seconds))
                    .ok_or_else(|| ApiError::bad_request("ttl_seconds is too large"))?,
            )
        }
        None => None,
    };
    if let Some(parent_id) = req.parent_session_id {
        let parent = state
            .store
            .get_session(parent_id)
            .await
            .ok_or_else(|| ApiError::bad_request("parent session not found"))?;
        if parent.status.is_terminal() {
            return Err(ApiError::conflict("parent session is not active"));
        }
        crate::schedules::delegation_allowed(&state, parent_id).await?;
    }

    // The lock protects idempotency, quota admission and single-use warm-pool
    // claims. Warm starts only perform a cheap resume while locked; cold starts
    // still preserve the existing duplicate-VM safety contract. Scoped per
    // agent (not a single global lock) so a slow/hung FluxVM call for one
    // agent can't block session creation for every other agent, and bounded
    // by `with_start_timeout` so a hang can't hold this lock forever either.
    let (record, input, prewarmed, _session_guard) = {
        let agent_lock = state.session_create_lock(&agent.name);
        let _guard = agent_lock.lock().await;

        if let Some(value) = request_id.as_deref() {
            if let Some(existing) = state
                .store
                .find_session_by_request_id(&agent.name, value)
                .await
            {
                if existing.input != req.input {
                    return Err(ApiError::conflict(
                        "request_id was already used for this agent with different input",
                    ));
                }
                return Ok((StatusCode::OK, Json(existing.into())));
            }
        }

        if let Some(max) = agent.manifest.max_concurrent_sessions {
            let active = state.store.count_non_terminal_for_agent(&agent.name).await;
            if active >= max {
                return Err(ApiError::too_many(format!(
                    "agent '{}' reached max_concurrent_sessions ({max})",
                    agent.name
                )));
            }
        }

        if let Some(user) = req.user_id.as_deref() {
            if state
                .store
                .count_non_terminal_for_user(&agent.name, user)
                .await
                > 0
            {
                return Err(ApiError::conflict(format!(
                    "user '{user}' already has an active session for agent '{}'",
                    agent.name
                )));
            }
        }

        let id = Uuid::new_v4();
        // Reserve the per-session operation lock before the record becomes
        // visible. Sync/expiry/control requests can discover Creating state,
        // but cannot race provisioning or overwrite its terminal transition.
        let operation_guard = state.session_lock(id).lock_owned().await;
        let worker_digest = pool::worker_digest_sha256(agent.manifest.runtime);
        let warm =
            if agent.manifest.warm_pool_size > 0 && start_policy != SessionStartPolicy::ColdOnly {
                state
                    .store
                    .claim_warm_sandbox(
                        &agent.name,
                        &agent.version,
                        &worker_digest,
                        agent.manifest.runtime_port,
                        id,
                    )
                    .await
                    .map_err(ApiError::internal)?
            } else {
                None
            };

        if warm.is_none() && start_policy == SessionStartPolicy::RequireWarm {
            return Err(ApiError::too_many(format!(
                "agent '{}' warm pool is exhausted; retry after replenishment",
                agent.name
            )));
        }

        let mut confidential: Option<crate::model::ConfidentialStatus> = None;
        let (sandbox_id, start_mode, prewarmed) = if let Some(warm) = warm {
            match with_start_timeout(state.fluxvm.resume(warm.sandbox_id)).await {
                Ok(()) => (warm.sandbox_id, SessionStartMode::Warm, true),
                Err(error) => {
                    tracing::warn!(
                        sandbox = %warm.sandbox_id,
                        %error,
                        "warm sandbox resume failed; falling back to cold start"
                    );
                    if let Err(cleanup_error) = pool::discard_claimed(&state, warm.sandbox_id).await
                    {
                        tracing::warn!(
                            sandbox = %warm.sandbox_id,
                            error = %cleanup_error,
                            "failed to clean up unusable warm sandbox; durable claim retained"
                        );
                    }
                    if start_policy == SessionStartPolicy::RequireWarm {
                        return Err(ApiError::unavailable(
                            "claimed warm sandbox could not resume; retry after pool replenishment",
                        ));
                    }
                    let sandbox =
                        create_cold_sandbox(&state, &agent, id, req.user_id.as_deref()).await?;
                    confidential = sandbox.confidential.clone();
                    (sandbox.id, SessionStartMode::Cold, false)
                }
            }
        } else {
            let sandbox = create_cold_sandbox(&state, &agent, id, req.user_id.as_deref()).await?;
            confidential = sandbox.confidential.clone();
            (sandbox.id, SessionStartMode::Cold, false)
        };

        let record = SessionRecord {
            id,
            agent: agent.name.clone(),
            agent_version: agent.version.clone(),
            sandbox_id,
            status: SessionStatus::Creating,
            input: req.input.clone(),
            created_at: admitted_at,
            updated_at: Utc::now(),
            last_event_seq: 0,
            guest_event_cursor: 0,
            request_id: request_id.clone(),
            start_policy,
            start_mode,
            startup_ms: None,
            expires_at,
            sandbox_released: false,
            capability_token: random_capability(),
            error: None,
            parent_session_id: req.parent_session_id,
            user_id: req.user_id.clone(),
            tainted_by: vec![],
            confidential: confidential.clone(),
            agent_paused_reason: None,
            browse: {
                let mut b = crate::browse_ifc::BrowseState::default();
                if agent.manifest.browser_port.is_some() {
                    let tenant = req.user_id.as_deref().unwrap_or(agent.name.as_str());
                    b.network_identity =
                        Some(crate::browse_ifc::browser_network_identity(tenant, id));
                    b.limits = crate::browse_ifc::BrowseLimits::with_defaults();
                }
                b
            },
        };
        if let Err(error) = state.store.save_session(record.clone()).await {
            if prewarmed {
                let _ = pool::discard_claimed(&state, sandbox_id).await;
            } else {
                let _ = state.fluxvm.delete(sandbox_id).await;
            }
            return Err(ApiError::internal(error));
        }
        if prewarmed {
            // The session record is now the durable owner. Never return this VM
            // to the pool, even if subsequent agent provisioning fails. If the
            // pool-file write fails, continue: restart reconciliation can see
            // the persisted Claiming record and the matching durable session.
            if let Err(error) = state.store.forget_warm_sandbox(sandbox_id).await {
                tracing::warn!(
                    session = %id,
                    sandbox = %sandbox_id,
                    %error,
                    "session owns warm sandbox but pool record cleanup was deferred"
                );
            }
        }
        if let Err(error) = state
            .store
            .append_event(
                id,
                "session.created",
                json!({
                    "sandbox_id": sandbox_id,
                    "agent_version": agent.version.clone(),
                    "request_id": request_id.clone(),
                    "start_policy": start_policy,
                    "start_mode": start_mode,
                    "expires_at": record.expires_at.as_ref(),
                    "parent_session_id": record.parent_session_id,
                    "confidential": record.confidential,
                }),
            )
            .await
        {
            // Admission is not complete until its first durable journal record
            // exists. If persistence fails after VM allocation, fail closed and
            // reclaim the single-use sandbox rather than leaving an invisible
            // Creating session behind. State persistence itself is best effort
            // here because the original error may be a storage failure.
            let released = state.fluxvm.delete(sandbox_id).await.is_ok();
            let message = format!("persisting session.created: {error:#}");
            let _ = state
                .store
                .update_session(id, |s| {
                    s.status = SessionStatus::Failed;
                    s.sandbox_released = released;
                    s.error = Some(message.clone());
                })
                .await;
            return Err(ApiError::internal(error));
        }
        if prewarmed {
            // This event is observability-only: the durable session.created
            // record already contains start_mode=warm and owns the sandbox. A
            // secondary journal failure must not turn a valid admission into
            // an API error that the caller may retry and duplicate.
            if let Err(error) = state
                .store
                .append_event(
                    id,
                    "session.warm-pool.claimed",
                    json!({"sandbox_id": sandbox_id}),
                )
                .await
            {
                tracing::warn!(
                    session = %id,
                    sandbox = %sandbox_id,
                    %error,
                    "failed to journal warm-pool claim"
                );
            }
        }
        (record, req.input.clone(), prewarmed, operation_guard)
    };

    // Provisioning (guest boot, health-check retries, bundle push) can take up
    // to guest_start_timeout_secs, far longer than any upstream proxy or
    // browser is willing to hold a single request open. Awaiting it inline
    // here would mean: once the caller's connection is closed by an impatient
    // timeout, axum drops this handler's future mid-flight, and none of the
    // failure-handling below (marking the session Failed, releasing the
    // sandbox) ever runs -- the session is then stuck in Creating forever,
    // with no further writes to it, ever. So provisioning is detached into
    // its own task; the client observes the outcome via session.running /
    // session.failed events (or by polling), never by blocking this request.
    let response_record = record.clone();
    tokio::spawn(async move {
        let _session_guard = _session_guard;
        if let Err(error) = provision_guest(&state, &record, &agent, input, prewarmed).await {
            let message = format!("{error:#}");
            // Journal the terminal event before setting terminal=true so an SSE
            // consumer cannot observe the state transition and miss the reason.
            let _ = state
                .store
                .append_event(
                    record.id,
                    "session.failed",
                    json!({"error": message.clone()}),
                )
                .await;
            let _ = state
                .store
                .update_session(record.id, |s| {
                    s.status = SessionStatus::Failed;
                    s.error = Some(message.clone());
                })
                .await;
            if let Some(failed) = state.store.get_session(record.id).await {
                let _ = try_release_sandbox(&state, &failed).await;
            }
            return;
        }

        let startup_ms = u64::try_from(admission_started.elapsed().as_millis()).unwrap_or(u64::MAX);
        let updated = match state
            .store
            .update_session(record.id, |s| {
                s.status = SessionStatus::Running;
                s.startup_ms = Some(startup_ms);
            })
            .await
        {
            Ok(updated) => updated,
            Err(error) => {
                tracing::error!(session = %record.id, %error, "failed to mark session running after successful provisioning");
                return;
            }
        };
        if let Err(error) = state
            .store
            .append_event(
                record.id,
                "session.running",
                json!({"start_mode": updated.start_mode, "startup_ms": startup_ms}),
            )
            .await
        {
            tracing::warn!(session = %record.id, %error, "failed to journal session.running event");
        }
    });

    Ok((StatusCode::CREATED, Json(response_record.into())))
}

/// Blocks until the guest's vsock-based FluxVM guest agent accepts a
/// command, or `guest_start_timeout_secs` elapses. Only meaningful right
/// after a cold `create_sandbox()`; a resumed/prewarmed sandbox's channel is
/// already up.
pub(crate) async fn wait_for_guest_agent_ready(state: &AppState, sandbox_id: Uuid) -> Result<()> {
    let deadline =
        tokio::time::Instant::now() + Duration::from_secs(state.config.guest_start_timeout_secs);
    loop {
        // Prefer agent/ping over process/exec: ping is cheap and reliable once
        // vsock is up; mkdir-via-exec has been observed to hang past the
        // attempt timeout on cold boots even when ping already returns 200.
        let attempt = with_timeout(
            HEALTH_CHECK_ATTEMPT_TIMEOUT,
            state.fluxvm.agent_ping(sandbox_id),
        )
        .await;
        match attempt {
            Ok(_) => return Ok(()),
            Err(error) if tokio::time::Instant::now() < deadline => {
                tracing::debug!(sandbox = %sandbox_id, %error, "guest agent not ready yet, retrying");
                tokio::time::sleep(Duration::from_millis(500)).await;
            }
            Err(error) => {
                return Err(error).context("guest agent did not become ready before timeout")
            }
        }
    }
}

/// Write the agent's pinned skills into the sandbox: base skills under
/// `/opt/zyvor/skills`, scoped ones under `/opt/zyvor/skills-scoped`, each root
/// with an `INDEX.json`. The scope policy is checked again here because the
/// operator may have tightened it since the agent was deployed; a skill the
/// agent may no longer use fails the session rather than being mounted.
async fn mount_skills(
    state: &AppState,
    session: &SessionRecord,
    agent: &crate::model::AgentRecord,
) -> Result<()> {
    use crate::skills::{index_json, mount_plan, split_ref, BASE_MOUNT, SCOPED_MOUNT};
    if agent.manifest.skills.is_empty() {
        return Ok(());
    }
    let mut bundles = Vec::new();
    for pin in &agent.manifest.skills {
        let (name, version) = split_ref(pin);
        let bundle = state
            .store
            .skills
            .get(name, version)
            .await
            .with_context(|| format!("loading pinned skill {pin}"))?;
        anyhow::ensure!(
            state
                .skill_scopes
                .allows(agent.manifest.skill_scope.as_deref(), bundle.record.scope.as_deref()),
            "skill {pin} is not permitted for this agent's skill_scope under the current scope policy"
        );
        bundles.push(bundle);
    }
    for bundle in &bundles {
        for (path, bytes, mode) in mount_plan(bundle)? {
            with_timeout(
                HEALTH_CHECK_ATTEMPT_TIMEOUT,
                state
                    .fluxvm
                    .fs_write(session.sandbox_id, &path, &bytes, mode),
            )
            .await
            .with_context(|| format!("writing skill file {path}"))?;
        }
    }
    for (root, scoped) in [(BASE_MOUNT, false), (SCOPED_MOUNT, true)] {
        let group: Vec<_> = bundles
            .iter()
            .filter(|b| b.record.scope.is_some() == scoped)
            .collect();
        if group.is_empty() {
            continue;
        }
        with_timeout(
            HEALTH_CHECK_ATTEMPT_TIMEOUT,
            state.fluxvm.fs_write(
                session.sandbox_id,
                &format!("{root}/INDEX.json"),
                &index_json(&group),
                0o444,
            ),
        )
        .await
        .with_context(|| format!("writing {root}/INDEX.json"))?;
    }
    Ok(())
}

async fn provision_guest(
    state: &AppState,
    session: &SessionRecord,
    agent: &crate::model::AgentRecord,
    input: Value,
    prewarmed: bool,
) -> Result<()> {
    let bundle = state
        .store
        .agent_bundle(&agent.name, &agent.version)
        .await?;
    if !prewarmed {
        // create_sandbox() returns as soon as the VM process is launched, not
        // once the guest has finished booting -- the in-guest fluxvm-guest-agent
        // only starts listening on its vsock channel partway through boot. The
        // very first guest-agent call after a cold create therefore routinely
        // races that boot and fails with "connecting to vsock proxy socket ...
        // No such file or directory" on a real (non-mocked) FluxVM backend.
        // Retry until the channel comes up rather than failing the session for
        // a timing issue that resolves itself within a few seconds.
        wait_for_guest_agent_ready(state, session.sandbox_id).await?;
        with_timeout(
            HEALTH_CHECK_ATTEMPT_TIMEOUT,
            state.fluxvm.fs_write(
                session.sandbox_id,
                "/opt/zyvor/worker.mjs",
                pool::entrypoint_bytes(agent.manifest.runtime),
                0o755,
            ),
        )
        .await?;
    }
    let host = match state.config.egress_advertise_host.as_deref() {
        Some(v) => v.to_string(),
        None => {
            with_timeout(
                HEALTH_CHECK_ATTEMPT_TIMEOUT,
                state.fluxvm.default_gateway(session.sandbox_id),
            )
            .await?
        }
    };
    // Confine before any agent code is written to or run in the guest, and fail
    // the session rather than run it unconfined.
    if state.config.confine_all || agent.manifest.confinement == crate::model::Confinement::Strict {
        let gateway = crate::confine::parse_gateway(&host)?;
        let mut fqdns = agent.manifest.egress_allow_hosts.clone();
        if let Some(b) = &agent.manifest.browser {
            fqdns.extend(b.allow_hosts.iter().cloned());
        }
        fqdns.sort();
        fqdns.dedup();
        let policy = crate::confine::strict_policy(
            gateway,
            state.config.egress_listen.port(),
            state.config.proxy_listen.map(|addr| addr.port()),
            &fqdns,
            Some(&session.id.to_string()),
            Some(&agent.name),
        );
        with_timeout(
            HEALTH_CHECK_ATTEMPT_TIMEOUT,
            state.fluxvm.set_network_policy(session.sandbox_id, &policy),
        )
        .await
        .context("applying sandbox network confinement")?;
    }
    with_timeout(
        HEALTH_CHECK_ATTEMPT_TIMEOUT,
        state.fluxvm.fs_write(
            session.sandbox_id,
            "/opt/zyvor/agent/bundle.mjs",
            &bundle,
            0o644,
        ),
    )
    .await?;

    if agent.manifest.inner_container == crate::model::InnerContainer::Strict {
        with_timeout(
            HEALTH_CHECK_ATTEMPT_TIMEOUT,
            state.fluxvm.fs_write(
                session.sandbox_id,
                crate::contain::GUEST_PATH,
                crate::contain::SCRIPT.as_bytes(),
                0o755,
            ),
        )
        .await?;
        with_timeout(
            HEALTH_CHECK_ATTEMPT_TIMEOUT,
            state.fluxvm.fs_write(
                session.sandbox_id,
                crate::contain::SECCOMP_GUEST_PATH,
                &crate::contain::seccomp_filter(),
                0o644,
            ),
        )
        .await?;
    }
    // Rules that name binaries need the guest to say which program owns a connection.
    if agent
        .manifest
        .egress_rules
        .iter()
        .any(|r| !r.binaries.is_empty())
    {
        with_timeout(
            HEALTH_CHECK_ATTEMPT_TIMEOUT,
            state.fluxvm.fs_write(
                session.sandbox_id,
                crate::binary_id::GUEST_PATH,
                crate::binary_id::SCRIPT.as_bytes(),
                0o755,
            ),
        )
        .await?;
    }
    mount_skills(state, session, agent).await?;

    let broker = format!(
        "http://{}:{}",
        format_host(&host),
        state.config.egress_listen.port()
    );
    // Basic credentials for the CONNECT proxy: the same session id and
    // capability the JSON broker takes, so the guest gains no new secret.
    let proxy = state.config.proxy_listen.map(|addr| {
        format!(
            "http://{}:{}@{}:{}",
            session.id,
            session.capability_token,
            format_host(&host),
            addr.port()
        )
    });
    let proxy_env = proxy
        .map(|url| format!("ZYVOR_EGRESS_PROXY={} ", shell_quote(&url)))
        .unwrap_or_default();
    let credentials =
        serde_json::to_string(&agent.manifest.credentials).unwrap_or_else(|_| "[]".into());
    let launcher = crate::contain::launcher_prefix(agent.manifest.inner_container);
    // With interception on, an agent that holds intercepted credentials gets the
    // CA in its trust stores and a surrogate for each: never the real secret.
    let surrogates = match &state.mitm {
        Some(_) => crate::mitm::surrogates(
            &state.credentials,
            &agent.manifest.credentials,
            &session.capability_token,
        ),
        None => Default::default(),
    };
    let mut mitm_env = String::new();
    if let (Some(mitm), false) = (&state.mitm, surrogates.is_empty()) {
        let installed = with_timeout(
            HEALTH_CHECK_ATTEMPT_TIMEOUT,
            state.fluxvm.fs_write(
                session.sandbox_id,
                crate::mitm::GUEST_CA_PATH,
                mitm.ca_pem().as_bytes(),
                0o644,
            ),
        )
        .await;
        match installed {
            Ok(()) => {
                if let Err(error) = with_timeout(
                    HEALTH_CHECK_ATTEMPT_TIMEOUT,
                    state
                        .fluxvm
                        .process(session.sandbox_id, crate::mitm::GUEST_INSTALL, Some(10)),
                )
                .await
                {
                    tracing::warn!(%error, "could not install the interception CA in the guest");
                }
            }
            Err(error) => {
                tracing::warn!(%error, "could not write the interception CA to the guest")
            }
        }
        mitm_env = format!(
            "NODE_EXTRA_CA_CERTS={} ZYVOR_SURROGATES={} ",
            crate::mitm::GUEST_CA_PATH,
            shell_quote(&serde_json::to_string(&surrogates).unwrap_or_else(|_| "{}".into())),
        );
    }
    let command = format!(
        "mkdir -p /opt/zyvor/agent; {proxy_env}{mitm_env}ZYVOR_SESSION_ID={} ZYVOR_EGRESS_CAPABILITY={} ZYVOR_EGRESS_BROKER={} ZYVOR_AGENT_PORT={} ZYVOR_AGENT_RUNTIME={} ZYVOR_HARNESS_CREDENTIALS={} ZYVOR_MODEL_BASE_URL={} ZYVOR_MODEL_NAME={} ZYVOR_MODEL_CREDENTIAL={} nohup {launcher}node /opt/zyvor/worker.mjs >/tmp/zyvor-agent.log 2>&1 </dev/null &",
        shell_quote(&session.id.to_string()),
        shell_quote(&session.capability_token),
        shell_quote(&broker),
        agent.manifest.runtime_port,
        shell_quote(agent.manifest.runtime.as_str()),
        shell_quote(&credentials),
        // The agent's model socket: where `ctx.model.chat()` and the CLI harnesses send model calls.
        shell_quote(
            agent
                .manifest
                .model_socket
                .as_ref()
                .map_or("", |s| s.base_url.as_str())
        ),
        shell_quote(
            agent
                .manifest
                .model_socket
                .as_ref()
                .and_then(|s| s.model.as_deref())
                .unwrap_or("")
        ),
        shell_quote(
            agent
                .manifest
                .model_socket
                .as_ref()
                .and_then(|s| s.credential.as_deref())
                .unwrap_or("")
        ),
    );
    with_timeout(
        HEALTH_CHECK_ATTEMPT_TIMEOUT,
        state.fluxvm.process(session.sandbox_id, &command, Some(10)),
    )
    .await?;

    let deadline =
        tokio::time::Instant::now() + Duration::from_secs(state.config.guest_start_timeout_secs);
    loop {
        // A single guest_request() has been observed to hang well past even
        // FluxVmClient's own 180s reqwest timeout (root cause not fully
        // understood -- reqwest connection-pool/keep-alive interaction is
        // the leading suspect, see fluxvm-api's sandbox_proxy_inner). Bound
        // every individual attempt here too, defensively: without this, one
        // stuck call blocks the loop from ever reaching the deadline check
        // below, no matter how short guest_start_timeout_secs is.
        let attempt = with_timeout(
            HEALTH_CHECK_ATTEMPT_TIMEOUT,
            state.fluxvm.guest_request(
                session.sandbox_id,
                agent.manifest.runtime_port,
                Method::GET,
                "health",
                None,
            ),
        )
        .await;
        match attempt {
            Ok(v) if v.get("ok").and_then(Value::as_bool) == Some(true) => break,
            _ if tokio::time::Instant::now() < deadline => {
                tokio::time::sleep(Duration::from_millis(200)).await
            }
            _ => anyhow::bail!("guest agent worker did not become ready before timeout"),
        }
    }
    // The user's memory, only for an agent that asked for it (manifest `memory`), goes in the run request, never in the session's stored input.
    let memory = crate::memory::context_for_session(state, session, &agent.manifest).await;
    with_timeout(
        HEALTH_CHECK_ATTEMPT_TIMEOUT,
        state.fluxvm.guest_request(
            session.sandbox_id,
            agent.manifest.runtime_port,
            Method::POST,
            "run",
            Some(&json!({"input": input, "memory": memory})),
        ),
    )
    .await?;
    Ok(())
}

async fn list_sessions(
    State(state): State<Arc<AppState>>,
    Extension(principal): Extension<Principal>,
    Query(query): Query<HashMap<String, String>>,
) -> Json<Value> {
    // A user sees only their own sessions, whatever `user_id` they ask for.
    let own = principal.user().map(str::to_string);
    let user = own.as_ref().or_else(|| query.get("user_id"));
    let items: Vec<SessionView> = state
        .store
        .list_sessions()
        .await
        .into_iter()
        .filter(|s| user.is_none_or(|u| s.user_id.as_deref() == Some(u.as_str())))
        .map(Into::into)
        .collect();
    Json(json!({"items": items}))
}

async fn get_session(
    State(state): State<Arc<AppState>>,
    Path(id): Path<Uuid>,
) -> ApiResult<Json<SessionView>> {
    state
        .store
        .get_session(id)
        .await
        .map(|v| Json(v.into()))
        .ok_or_else(|| ApiError::not_found("session not found"))
}

pub(crate) async fn steer_session(
    State(state): State<Arc<AppState>>,
    Path(id): Path<Uuid>,
    Json(req): Json<SteerRequest>,
) -> ApiResult<(StatusCode, Json<Value>)> {
    let lock = state.session_lock(id);
    let _guard = lock.lock().await;
    let session = require_session(&state, id).await?;
    if session.status != SessionStatus::Running {
        return Err(ApiError::conflict("session is not running"));
    }
    let agent = state
        .store
        .get_agent_version(&session.agent, &session.agent_version)
        .await
        .map_err(ApiError::internal)?;
    let message = req.message;
    let value = state
        .fluxvm
        .guest_request(
            session.sandbox_id,
            agent.manifest.runtime_port,
            Method::POST,
            "steer",
            Some(&json!({"message": message.clone()})),
        )
        .await
        .map_err(ApiError::bad_gateway)?;
    state
        .store
        .append_event(id, "session.steer.requested", message)
        .await
        .map_err(ApiError::internal)?;
    Ok((StatusCode::ACCEPTED, Json(value)))
}

/// Clear a session's taint after an operator has looked at what it read. The
/// agent cannot call this: it sits behind the operator token, on the public
/// router only.
async fn untaint_session(
    State(state): State<Arc<AppState>>,
    Path(id): Path<Uuid>,
) -> ApiResult<Json<SessionView>> {
    require_session(&state, id).await?;
    let hosts = state
        .store
        .untaint_session(id)
        .await
        .map_err(ApiError::internal)?;
    if !hosts.is_empty() {
        if let Err(error) = state
            .store
            .audit
            .append(
                Some(id),
                AuditPhase::Approved,
                "session.untainted",
                None,
                json!({"was_tainted_by": hosts}),
            )
            .await
        {
            tracing::error!(%error, "failed to write audit entry");
        }
    }
    let session = require_session(&state, id).await?;
    Ok(Json(session.into()))
}

pub(crate) async fn cancel_session(
    State(state): State<Arc<AppState>>,
    Path(id): Path<Uuid>,
) -> ApiResult<(StatusCode, Json<SessionView>)> {
    let lock = state.session_lock(id);
    let _guard = lock.lock().await;
    let session = require_session(&state, id).await?;
    if session.status.is_terminal() {
        return Err(ApiError::conflict("session is already terminal"));
    }
    if let Ok(agent) = state
        .store
        .get_agent_version(&session.agent, &session.agent_version)
        .await
    {
        let _ = state
            .fluxvm
            .guest_request(
                session.sandbox_id,
                agent.manifest.runtime_port,
                Method::POST,
                "cancel",
                Some(&json!({})),
            )
            .await;
    }
    // Persist the terminal event before the terminal state so an SSE client can
    // never observe terminal=true and exit before the event reaches the journal.
    state
        .store
        .append_event(id, "session.cancelled", json!({}))
        .await
        .map_err(ApiError::internal)?;
    let updated = state
        .store
        .update_session(id, |s| s.status = SessionStatus::Cancelled)
        .await
        .map_err(ApiError::internal)?;
    let _ = try_release_sandbox(&state, &updated).await;
    let latest = state.store.get_session(id).await.unwrap_or(updated);
    Ok((StatusCode::ACCEPTED, Json(latest.into())))
}

async fn hibernate_session(
    State(state): State<Arc<AppState>>,
    Path(id): Path<Uuid>,
) -> ApiResult<Json<SessionView>> {
    hibernate_session_inner(&state, id).await.map(Json)
}

async fn hibernate_session_inner(state: &AppState, id: Uuid) -> ApiResult<SessionView> {
    let lock = state.session_lock(id);
    let _guard = lock.lock().await;
    let session = require_session(state, id).await?;
    if session.status != SessionStatus::Running {
        return Err(ApiError::conflict("only running sessions can hibernate"));
    }
    let agent = state
        .store
        .get_agent_version(&session.agent, &session.agent_version)
        .await
        .map_err(ApiError::internal)?;

    let transitioned = state
        .store
        .compare_and_set_status(id, SessionStatus::Running, SessionStatus::Hibernating)
        .await
        .map_err(ApiError::internal)?;
    if transitioned.is_none() {
        return Err(ApiError::conflict(
            "session state changed while hibernating",
        ));
    }

    let operation = async {
        state
            .fluxvm
            .guest_request(
                session.sandbox_id,
                agent.manifest.runtime_port,
                Method::POST,
                "checkpoint",
                Some(&json!({})),
            )
            .await
            .map_err(ApiError::bad_gateway)?;
        tokio::fs::create_dir_all(&state.config.snapshot_dir)
            .await
            .map_err(ApiError::internal)?;
        let snapshot = state.config.snapshot_dir.join(format!("{id}.snapshot"));
        let snapshot_string = snapshot.to_string_lossy().into_owned();
        state
            .fluxvm
            .snapshot(session.sandbox_id, &snapshot_string)
            .await
            .map_err(ApiError::bad_gateway)?;
        state
            .fluxvm
            .pause(session.sandbox_id)
            .await
            .map_err(ApiError::bad_gateway)?;
        Ok::<_, ApiError>(snapshot)
    }
    .await;

    let snapshot = match operation {
        Ok(snapshot) => snapshot,
        Err(error) => {
            let _ = state
                .store
                .update_session(id, |s| s.status = SessionStatus::Running)
                .await;
            let _ = state
                .store
                .append_event(
                    id,
                    "session.hibernate.failed",
                    json!({"error": error.message.clone()}),
                )
                .await;
            return Err(error);
        }
    };

    let updated = state
        .store
        .update_session(id, |s| s.status = SessionStatus::Hibernated)
        .await
        .map_err(ApiError::internal)?;
    state
        .store
        .append_event(
            id,
            "session.hibernated",
            json!({"snapshot": snapshot.file_name().and_then(|s| s.to_str())}),
        )
        .await
        .map_err(ApiError::internal)?;
    Ok(updated.into())
}

async fn resume_session(
    State(state): State<Arc<AppState>>,
    Path(id): Path<Uuid>,
) -> ApiResult<Json<SessionView>> {
    let lock = state.session_lock(id);
    let _guard = lock.lock().await;
    let session = require_session(&state, id).await?;
    if session.status != SessionStatus::Hibernated {
        return Err(ApiError::conflict("session is not hibernated"));
    }
    state
        .fluxvm
        .resume(session.sandbox_id)
        .await
        .map_err(ApiError::bad_gateway)?;
    let updated = state
        .store
        .update_session(id, |s| s.status = SessionStatus::Running)
        .await
        .map_err(ApiError::internal)?;
    state
        .store
        .append_event(id, "session.resumed", json!({}))
        .await
        .map_err(ApiError::internal)?;
    Ok(Json(updated.into()))
}

async fn delete_session(
    State(state): State<Arc<AppState>>,
    Path(id): Path<Uuid>,
) -> ApiResult<StatusCode> {
    let lock = state.session_lock(id);
    let _guard = lock.lock().await;
    let session = require_session(&state, id).await?;
    state
        .fluxvm
        .delete(session.sandbox_id)
        .await
        .map_err(ApiError::bad_gateway)?;
    state
        .store
        .append_event(id, "session.deleted", json!({}))
        .await
        .map_err(ApiError::internal)?;
    state
        .store
        .update_session(id, |s| {
            if !s.status.is_terminal() {
                s.status = SessionStatus::Cancelled;
            }
            s.sandbox_released = true;
        })
        .await
        .map_err(ApiError::internal)?;
    Ok(StatusCode::NO_CONTENT)
}

async fn stream_events(
    State(state): State<Arc<AppState>>,
    Path(id): Path<Uuid>,
    Query(query): Query<EventsQuery>,
) -> ApiResult<Sse<impl futures_core::Stream<Item = Result<Event, Infallible>>>> {
    require_session(&state, id).await?;
    let stream = async_stream::stream! {
        let mut cursor = query.after;
        loop {
            match state.store.events_after(id, cursor).await {
                Ok(events) => {
                    for item in events {
                        cursor = item.seq;
                        let data = serde_json::to_string(&item).unwrap_or_else(|_| "{}".into());
                        yield Ok(Event::default().id(item.seq.to_string()).event(item.kind.clone()).data(data));
                    }
                }
                Err(error) => {
                    yield Ok(Event::default().event("error").data(json!({"error": error.to_string()}).to_string()));
                    break;
                }
            }
            let terminal = state.store.get_session(id).await.map(|s| s.status.is_terminal()).unwrap_or(true);
            if terminal { break; }
            tokio::time::sleep(Duration::from_millis(250)).await;
        }
    };
    Ok(Sse::new(stream).keep_alive(KeepAlive::default()))
}

async fn require_session(state: &AppState, id: Uuid) -> ApiResult<SessionRecord> {
    state
        .store
        .get_session(id)
        .await
        .ok_or_else(|| ApiError::not_found("session not found"))
}

pub async fn sync_loop(state: Arc<AppState>) {
    let interval = Duration::from_millis(state.config.sync_interval_ms.max(50));
    loop {
        let sessions = state.store.active_sessions().await;
        for candidate in sessions {
            let lock = state.session_lock(candidate.id);
            let _guard = lock.lock().await;
            let Some(session) = state.store.get_session(candidate.id).await else {
                continue;
            };
            if !matches!(
                session.status,
                SessionStatus::Creating | SessionStatus::Running
            ) {
                continue;
            }
            let session_id = session.id;
            if let Err(error) = sync_session(&state, session).await {
                tracing::debug!(session = %session_id, %error, "agent session sync skipped");
                continue;
            }
            if let Some(updated) = state.store.get_session(session_id).await {
                if updated.status.is_terminal() && !updated.sandbox_released {
                    if let Err(error) = try_release_sandbox(&state, &updated).await {
                        tracing::debug!(session = %updated.id, %error, "terminal sandbox cleanup deferred");
                    }
                }
            }
        }
        tokio::time::sleep(interval).await;
    }
}

async fn sync_session(state: &Arc<AppState>, session: SessionRecord) -> Result<()> {
    let agent = state
        .store
        .get_agent_version(&session.agent, &session.agent_version)
        .await?;
    let path = format!("events?after={}", session.guest_event_cursor);
    let value = state
        .fluxvm
        .guest_request(
            session.sandbox_id,
            agent.manifest.runtime_port,
            Method::GET,
            &path,
            None,
        )
        .await?;
    let events: GuestEventsResponse = serde_json::from_value(value)?;
    let mut cursor = session.guest_event_cursor;
    for event in events.items {
        if event.seq <= cursor {
            continue;
        }
        cursor = event.seq;
        state
            .store
            .append_event(session.id, event.kind.clone(), event.data.clone())
            .await?;
        match event.kind.as_str() {
            "session.result" => {
                state
                    .store
                    .update_session(session.id, |s| s.status = SessionStatus::Completed)
                    .await?;
            }
            "session.failed" => {
                let error = event
                    .data
                    .get("error")
                    .and_then(Value::as_str)
                    .map(str::to_string);
                state
                    .store
                    .update_session(session.id, |s| {
                        s.status = SessionStatus::Failed;
                        s.error = error.clone();
                    })
                    .await?;
            }
            "session.cancelled" => {
                state
                    .store
                    .update_session(session.id, |s| s.status = SessionStatus::Cancelled)
                    .await?;
            }
            "delegate.request" => {
                spawn_delegation(state, session.id, &event.data);
            }
            "approval.requested" => {
                record_approval_request(state, session.id, event.seq, &event.data).await?;
            }
            "goal.plan_proposed" => {
                crate::goal_plan::record_proposal(state, &session, &event.data).await;
            }
            "suggestion.propose" => {
                crate::suggestions::record_proposal(state, &session, &agent.manifest, &event.data)
                    .await;
            }
            "memory.propose" => {
                crate::memory::record_proposal(state, &session, &agent.manifest, &event.data).await;
            }
            "card.propose" => {
                crate::card::record_card(state, &session, &agent.manifest, &event.data).await;
            }
            _ => {}
        }
        state
            .store
            .update_session(session.id, |s| s.guest_event_cursor = cursor)
            .await?;
    }
    if !state
        .store
        .get_session(session.id)
        .await
        .is_some_and(|s| s.status.is_terminal())
    {
        let value = state
            .fluxvm
            .guest_request(
                session.sandbox_id,
                agent.manifest.runtime_port,
                Method::GET,
                "status",
                None,
            )
            .await?;
        let guest: GuestStatusResponse = serde_json::from_value(value)?;
        if guest.status == "failed" {
            let message = guest
                .error
                .clone()
                .unwrap_or_else(|| "guest worker reported failure".to_string());
            state
                .store
                .append_event(session.id, "session.failed", json!({"error": message}))
                .await?;
            state
                .store
                .update_session(session.id, |s| {
                    s.status = SessionStatus::Failed;
                    s.error = guest.error.clone();
                })
                .await?;
        }
    }
    Ok(())
}

pub async fn auto_hibernate_loop(state: Arc<AppState>) {
    let interval = Duration::from_millis(state.config.idle_scan_interval_ms.max(250));
    loop {
        tokio::time::sleep(interval).await;
        for session in state.store.list_sessions().await {
            if session.status != SessionStatus::Running {
                continue;
            }
            let agent = match state
                .store
                .get_agent_version(&session.agent, &session.agent_version)
                .await
            {
                Ok(agent) => agent,
                Err(error) => {
                    tracing::warn!(session = %session.id, %error, "auto-hibernate skipped: agent version missing");
                    continue;
                }
            };
            let Some(idle_secs) = agent.manifest.idle_hibernate_seconds.filter(|v| *v > 0) else {
                continue;
            };
            let idle_for = Utc::now()
                .signed_duration_since(session.updated_at)
                .num_seconds();
            if idle_for < idle_secs as i64 {
                continue;
            }

            // Never freeze an agent merely because it has been quiet. The guest
            // explicitly reports `waiting` only while blocked in ctx.nextSteer().
            let guest = match state
                .fluxvm
                .guest_request(
                    session.sandbox_id,
                    agent.manifest.runtime_port,
                    Method::GET,
                    "status",
                    None,
                )
                .await
                .and_then(|value| {
                    serde_json::from_value::<GuestStatusResponse>(value)
                        .map_err(anyhow::Error::from)
                }) {
                Ok(guest) => guest,
                Err(_) => continue,
            };
            if guest.status != "waiting" {
                continue;
            }

            match hibernate_session_inner(&state, session.id).await {
                Ok(_) => {
                    tracing::info!(session = %session.id, idle_secs, "auto-hibernated waiting agent session")
                }
                Err(error) if error.status == StatusCode::CONFLICT => {}
                Err(error) => {
                    tracing::warn!(session = %session.id, error = %error.message, "auto-hibernate failed")
                }
            }
        }
    }
}

pub async fn expiry_loop(state: Arc<AppState>) {
    let interval = Duration::from_millis(state.config.expiry_scan_interval_ms.max(250));
    loop {
        tokio::time::sleep(interval).await;
        let now = Utc::now();
        for candidate in state.store.list_sessions().await {
            if candidate.status.is_terminal()
                || candidate
                    .expires_at
                    .as_ref()
                    .is_none_or(|deadline| deadline > &now)
            {
                continue;
            }
            let lock = state.session_lock(candidate.id);
            let _guard = lock.lock().await;
            let session = match state.store.get_session(candidate.id).await {
                Some(session) => session,
                None => continue,
            };
            if session.status.is_terminal()
                || session
                    .expires_at
                    .as_ref()
                    .is_none_or(|deadline| deadline > &Utc::now())
            {
                continue;
            }
            if let Err(error) = state.fluxvm.delete(session.sandbox_id).await {
                tracing::warn!(
                    session = %session.id,
                    sandbox = %session.sandbox_id,
                    %error,
                    "expired session sandbox delete failed; will retry"
                );
                continue;
            }
            if let Err(error) = state
                .store
                .append_event(
                    session.id,
                    "session.expired",
                    json!({"expires_at": session.expires_at.as_ref()}),
                )
                .await
            {
                tracing::warn!(session = %session.id, %error, "failed to journal session expiry");
                continue;
            }
            if let Err(error) = state
                .store
                .update_session(session.id, |s| {
                    s.status = SessionStatus::Expired;
                    s.sandbox_released = true;
                })
                .await
            {
                tracing::warn!(session = %session.id, %error, "failed to persist session expiry");
                continue;
            }
            tracing::info!(session = %session.id, "expired agent session");
        }
    }
}

async fn try_release_sandbox(state: &AppState, session: &SessionRecord) -> Result<()> {
    if session.sandbox_released {
        return Ok(());
    }
    state.fluxvm.delete(session.sandbox_id).await?;
    state
        .store
        .update_session(session.id, |s| s.sandbox_released = true)
        .await?;
    tracing::info!(
        session = %session.id,
        sandbox = %session.sandbox_id,
        "released terminal agent sandbox"
    );
    Ok(())
}

pub async fn terminal_cleanup_loop(state: Arc<AppState>) {
    let interval = Duration::from_millis(state.config.sync_interval_ms.max(250));
    loop {
        tokio::time::sleep(interval).await;
        for candidate in state.store.list_sessions().await {
            if !candidate.status.is_terminal() || candidate.sandbox_released {
                continue;
            }
            let lock = state.session_lock(candidate.id);
            let _guard = lock.lock().await;
            let Some(session) = state.store.get_session(candidate.id).await else {
                continue;
            };
            if !session.status.is_terminal() || session.sandbox_released {
                continue;
            }
            if let Err(error) = try_release_sandbox(&state, &session).await {
                tracing::warn!(
                    session = %session.id,
                    sandbox = %session.sandbox_id,
                    %error,
                    "terminal agent sandbox cleanup failed; will retry"
                );
            }
        }
    }
}

fn spawn_delegation(state: &Arc<AppState>, parent: Uuid, data: &Value) {
    let target = data
        .get("agent")
        .and_then(Value::as_str)
        .unwrap_or("")
        .trim()
        .to_string();
    if target.is_empty() {
        return;
    }
    let input = data.get("input").cloned().unwrap_or(Value::Null);
    let state = Arc::clone(state);
    tokio::spawn(async move {
        let user_id = state
            .store
            .get_session(parent)
            .await
            .and_then(|p| p.user_id);
        let request = CreateSessionRequest {
            agent: target.clone(),
            input,
            ttl_seconds: None,
            request_id: Some(format!("delegate:{parent}:{}", Uuid::new_v4().simple())),
            start_policy: SessionStartPolicy::PreferWarm,
            parent_session_id: Some(parent),
            user_id,
        };
        match create_session(State(state.clone()), Json(request)).await {
            Ok((_, Json(view))) => {
                let _ = state
                    .store
                    .append_event(
                        parent,
                        "session.delegated",
                        json!({"session_id": view.id, "agent": view.agent}),
                    )
                    .await;
            }
            Err(error) => {
                let _ = state
                    .store
                    .append_event(
                        parent,
                        "session.delegate.failed",
                        json!({"agent": target, "error": error.message()}),
                    )
                    .await;
            }
        }
    });
}

async fn record_approval_request(
    state: &AppState,
    session_id: Uuid,
    seq: u64,
    data: &Value,
) -> Result<()> {
    if state
        .store
        .approval_for_event(session_id, seq)
        .await
        .is_some()
    {
        return Ok(());
    }
    let prompt = data
        .get("prompt")
        .and_then(Value::as_str)
        .unwrap_or("")
        .trim()
        .to_string();
    if prompt.is_empty() {
        return Ok(());
    }
    let record = ApprovalRecord {
        id: Uuid::new_v4(),
        session_id,
        kind: ApprovalKind::Custom,
        subject: None,
        planned_action: None,
        prompt,
        status: ApprovalStatus::Pending,
        comment: None,
        created_at: Utc::now(),
        decided_at: None,
        source_seq: Some(seq),
        grant_scope: None,
        preview: None,
        broker_held: false,
    };
    state.store.save_approval(record.clone()).await?;
    audit_approval_planned(state, &record).await;
    Ok(())
}

/// Append the "planned" journal entry for a freshly opened approval. Audit
/// failures are logged, not propagated: an unwritable journal must not wedge
/// a session, and the failure is itself visible in the logs.
pub(crate) async fn audit_approval_planned(state: &AppState, record: &ApprovalRecord) {
    let result = state
        .store
        .audit
        .append(
            Some(record.session_id),
            AuditPhase::Planned,
            format!("approval.{}", record.kind.as_str()),
            record.subject.clone(),
            json!({
                "approval_id": record.id,
                "prompt": record.prompt,
                "planned_action": record.planned_action,
            }),
        )
        .await;
    if let Err(error) = result {
        tracing::error!(%error, approval_id = %record.id, "failed to write audit entry");
    }
    crate::notify::approval_requested(state, record);
}

async fn delegate_session(
    State(state): State<Arc<AppState>>,
    Path(id): Path<Uuid>,
    Json(req): Json<DelegateRequest>,
) -> ApiResult<(StatusCode, Json<SessionView>)> {
    let parent = require_session(&state, id).await?;
    if parent.status != SessionStatus::Running {
        return Err(ApiError::conflict("session is not running"));
    }
    let (status, Json(view)) = create_session(
        State(state.clone()),
        Json(CreateSessionRequest {
            agent: req.agent,
            input: req.input,
            ttl_seconds: None,
            request_id: Some(format!("delegate:{id}:{}", Uuid::new_v4().simple())),
            start_policy: SessionStartPolicy::PreferWarm,
            parent_session_id: Some(id),
            user_id: parent.user_id.clone(),
        }),
    )
    .await?;
    let _ = state
        .store
        .append_event(
            id,
            "session.delegated",
            json!({"session_id": view.id, "agent": view.agent}),
        )
        .await;
    Ok((status, Json(view)))
}

#[derive(Debug, serde::Deserialize)]
struct AuditQuery {
    #[serde(default)]
    session_id: Option<Uuid>,
    #[serde(default)]
    limit: Option<usize>,
    /// `export/audit` only: `ocsf` returns newline-delimited OCSF events instead of JSON.
    #[serde(default)]
    format: Option<String>,
}

/// Default and maximum page size for `GET /v1/audit`.
const AUDIT_DEFAULT_LIMIT: usize = 200;
const AUDIT_MAX_LIMIT: usize = 5000;

async fn list_audit(
    State(state): State<Arc<AppState>>,
    Extension(principal): Extension<Principal>,
    Query(query): Query<AuditQuery>,
) -> ApiResult<Json<Value>> {
    // Operator console: recent journal only (not a trajectory export).
    let limit = query
        .limit
        .unwrap_or(AUDIT_DEFAULT_LIMIT)
        .clamp(1, AUDIT_MAX_LIMIT.min(500));
    let chain = state
        .store
        .audit
        .verify()
        .await
        .map_err(ApiError::internal)?;
    if let Some(uid) = principal.user() {
        // Only this user's rows. The chain verdict is global, but its size is not shown.
        let mut mine = authz::session_ids_of(&state, uid).await;
        if let Some(sid) = query.session_id {
            mine.retain(|id| *id == sid);
        }
        let items = state
            .store
            .audit
            .list_for_sessions(&mine, limit)
            .await
            .map_err(ApiError::internal)?;
        return Ok(Json(
            json!({"items": items, "chain": {"chain_ok": chain.chain_ok}, "export": false}),
        ));
    }
    let items = state
        .store
        .audit
        .list(query.session_id, limit)
        .await
        .map_err(ApiError::internal)?;
    Ok(Json(
        json!({"items": items, "chain": chain, "export": false}),
    ))
}

/// Full audit/trajectory export — requires X-Keep-Export-Token (training default off).
async fn export_audit(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Query(query): Query<AuditQuery>,
) -> ApiResult<Response> {
    let ocsf = match query.format.as_deref() {
        None | Some("json") => false,
        Some("ocsf") => true,
        Some(other) => {
            return Err(ApiError::bad_request(format!(
                "unknown format `{other}`; use json or ocsf"
            )))
        }
    };
    let export = headers
        .get("x-keep-export-token")
        .and_then(|v| v.to_str().ok());
    state
        .export_tokens
        .authorize(export, "audit")
        .await
        .map_err(ApiError::forbidden)?;
    let limit = query
        .limit
        .unwrap_or(AUDIT_MAX_LIMIT)
        .clamp(1, AUDIT_MAX_LIMIT);
    let items = state
        .store
        .audit
        .list(query.session_id, limit)
        .await
        .map_err(ApiError::internal)?;
    let chain = state
        .store
        .audit
        .verify()
        .await
        .map_err(ApiError::internal)?;
    let _ = state
        .store
        .audit
        .append(
            query.session_id,
            AuditPhase::Performed,
            "keep.audit.exported",
            None,
            json!({"limit": limit}),
        )
        .await;
    if ocsf {
        // The chain verdict rides in a header so the body stays pure NDJSON.
        let verdict = if chain.chain_ok { "ok" } else { "broken" };
        return Ok((
            [
                (
                    axum::http::header::CONTENT_TYPE,
                    "application/x-ndjson; charset=utf-8",
                ),
                (
                    axum::http::HeaderName::from_static("x-keep-audit-chain"),
                    verdict,
                ),
            ],
            crate::ocsf::to_ndjson(&items),
        )
            .into_response());
    }
    Ok(Json(json!({"items": items, "chain": chain, "export": true})).into_response())
}

#[derive(Debug, serde::Deserialize)]
struct PinQuery {
    /// Clear only this program path; absent clears every pin of the agent.
    #[serde(default)]
    path: Option<String>,
}

/// Programs whose hash the runtime remembered for an agent (trust on first use). Operator only.
async fn list_binary_pins(
    State(state): State<Arc<AppState>>,
    Path(name): Path<String>,
) -> ApiResult<Json<Value>> {
    if state.store.get_agent(&name).await.is_none() {
        return Err(ApiError::not_found("agent not found"));
    }
    let pins = crate::binary_id::list_pins(&state.config.state_dir, &name)
        .await
        .map_err(ApiError::internal)?;
    Ok(Json(json!({
        "agent": name,
        "pins": pins.into_iter().map(|(path, sha256)| json!({"path": path, "sha256": sha256})).collect::<Vec<_>>(),
    })))
}

/// Forget remembered hashes so a rebuilt program is trusted afresh. Operator only, and journaled.
async fn clear_binary_pins(
    State(state): State<Arc<AppState>>,
    Path(name): Path<String>,
    Query(query): Query<PinQuery>,
) -> ApiResult<Json<Value>> {
    if state.store.get_agent(&name).await.is_none() {
        return Err(ApiError::not_found("agent not found"));
    }
    let removed =
        crate::binary_id::clear_pins(&state.config.state_dir, &name, query.path.as_deref())
            .await
            .map_err(ApiError::internal)?;
    let _ = state
        .store
        .audit
        .append(
            None,
            AuditPhase::Performed,
            "keep.binary_pins.cleared",
            Some(name.clone()),
            json!({"removed": removed, "path": query.path}),
        )
        .await;
    Ok(Json(json!({"agent": name, "removed": removed})))
}

/// Journal rows scanned for `policy-suggestions` unless `?limit=` says otherwise.
const SUGGEST_SCAN_DEFAULT: usize = 5000;
const SUGGEST_SCAN_MAX: usize = 50_000;

/// Keep: draft allow rules from an agent's denied egress. Operator only (user tokens never
/// reach this route). Nothing is applied: load a reviewed policy with `PUT …/policy`.
async fn get_policy_suggestions(
    State(state): State<Arc<AppState>>,
    Path(name): Path<String>,
    Query(query): Query<AuditQuery>,
) -> ApiResult<Json<Value>> {
    let agent = state
        .store
        .get_agent(&name)
        .await
        .ok_or_else(|| ApiError::not_found("agent not found"))?;
    let current = crate::policy::KeepPolicy::from_manifest(&agent.manifest);
    let sessions: std::collections::HashSet<Uuid> = state
        .store
        .list_sessions()
        .await
        .into_iter()
        .filter(|s| s.agent == name)
        .map(|s| s.id)
        .collect();
    let scan = query
        .limit
        .unwrap_or(SUGGEST_SCAN_DEFAULT)
        .clamp(1, SUGGEST_SCAN_MAX);
    let entries = state
        .store
        .audit
        .list(None, scan)
        .await
        .map_err(ApiError::internal)?;
    let suggestions = crate::advisor::suggest(&entries, &sessions, &current);
    let candidate = crate::advisor::candidate_policy(&current, &suggestions);
    let candidate_yaml = if suggestions.iter().any(|s| !s.needs_ack) {
        Some(candidate.to_yaml().map_err(ApiError::internal)?)
    } else {
        None
    };
    Ok(Json(json!({
        "agent": name,
        "scanned_rows": entries.len(),
        "suggestions": suggestions,
        "candidate_yaml": candidate_yaml,
        "note": "a draft, not applied: read it, sign it, then PUT it; suggestions with a High finding are not in candidate_yaml",
    })))
}

/// Keep: readable Sentinel policy as `keep.policy.yaml`.
async fn get_agent_policy(
    State(state): State<Arc<AppState>>,
    Path(name): Path<String>,
) -> ApiResult<Response> {
    let agent = state
        .store
        .get_agent(&name)
        .await
        .ok_or_else(|| ApiError::not_found("agent not found"))?;
    let yaml = crate::policy::KeepPolicy::from_manifest(&agent.manifest)
        .to_yaml()
        .map_err(ApiError::internal)?;
    Ok((
        [(
            axum::http::header::CONTENT_TYPE,
            "application/x-yaml; charset=utf-8",
        )],
        yaml,
    )
        .into_response())
}

/// Keep: replace policy from YAML; redeploys a new agent version (sessions keep old contract).
async fn put_agent_policy(
    State(state): State<Arc<AppState>>,
    Path(name): Path<String>,
    headers: HeaderMap,
    body: String,
) -> ApiResult<Json<Value>> {
    let sig = headers
        .get("x-keep-policy-signature")
        .and_then(|v| v.to_str().ok());
    state
        .policy_trust
        .verify_yaml(body.as_bytes(), sig)
        .map_err(ApiError::forbidden)?;
    let agent = state
        .store
        .get_agent(&name)
        .await
        .ok_or_else(|| ApiError::not_found("agent not found"))?;
    let policy = crate::policy::KeepPolicy::from_yaml(&body).map_err(ApiError::bad_request)?;
    policy.enforceable().map_err(ApiError::bad_request)?;
    let current = crate::policy::KeepPolicy::from_manifest(&agent.manifest);
    let risks = crate::policy_lint::review_change(Some(&current), &policy);
    let acked = headers
        .get("x-keep-policy-ack-risk")
        .and_then(|v| v.to_str().ok())
        == Some("1");
    if crate::policy_lint::needs_ack(&risks) && !acked {
        return Err(ApiError::conflict(format!(
            "policy change widens access ({}); review it and resend with X-Keep-Policy-Ack-Risk: 1",
            crate::policy_lint::summarize_high(&risks)
        )));
    }
    let mut manifest = agent.manifest.clone();
    policy.apply_to_manifest(&mut manifest);
    let bundle_path = state
        .config
        .state_dir
        .join("agents")
        .join(&name)
        .join(&agent.version)
        .join("bundle.mjs");
    let bundle = tokio::fs::read(&bundle_path)
        .await
        .map_err(ApiError::internal)?;
    let record = state
        .store
        .deploy_agent(crate::model::DeployAgentRequest {
            name: name.clone(),
            bundle_base64: base64::Engine::encode(
                &base64::engine::general_purpose::STANDARD,
                bundle,
            ),
            manifest,
        })
        .await
        .map_err(ApiError::bad_request)?;
    let _ = state
        .store
        .audit
        .append(
            None,
            AuditPhase::Performed,
            "keep.policy.set",
            Some(name),
            json!({
                "version": record.version,
                "risk_codes": risks.iter().map(|r| r.code).collect::<Vec<_>>(),
                "risk_acknowledged": acked,
            }),
        )
        .await;
    Ok(Json(json!({
        "name": record.name,
        "version": record.version,
        "policy": crate::policy::KeepPolicy::from_manifest(&record.manifest),
        "risks": risks,
    })))
}

/// Keep cockpit: taint paint + last Sentinel/audit decisions for a session.
async fn session_cockpit(
    State(state): State<Arc<AppState>>,
    Path(id): Path<Uuid>,
) -> ApiResult<Json<Value>> {
    let session = state
        .store
        .get_session(id)
        .await
        .ok_or_else(|| ApiError::not_found("session not found"))?;
    let decisions = state
        .store
        .audit
        .list(Some(id), 20)
        .await
        .map_err(ApiError::internal)?;
    let pending: Vec<_> = state
        .store
        .list_approvals()
        .await
        .into_iter()
        .filter(|a| a.session_id == id && a.status == crate::model::ApprovalStatus::Pending)
        .collect();
    let schedules = state.store.list_schedules().await;
    let upcoming: Vec<_> = schedules
        .into_iter()
        .filter(|s| s.agent == session.agent)
        .take(10)
        .collect();
    let goals = state.store.list_goals().await;
    let active_goal = goals
        .iter()
        .find(|g| {
            g.session_id == Some(id)
                && matches!(
                    g.status,
                    crate::goals::GoalStatus::Open | crate::goals::GoalStatus::Blocked
                )
        })
        .cloned()
        .or_else(|| {
            goals
                .iter()
                .find(|g| {
                    g.agent == session.agent
                        && matches!(
                            g.status,
                            crate::goals::GoalStatus::Open | crate::goals::GoalStatus::Blocked
                        )
                })
                .cloned()
        });
    let recent_artifacts: Vec<_> = state
        .store
        .list_artifacts()
        .await
        .into_iter()
        .filter(|a| {
            a.session_id == Some(id)
                || active_goal
                    .as_ref()
                    .map(|g| a.goal_id == Some(g.id))
                    .unwrap_or(false)
                || a.agent.as_deref() == Some(session.agent.as_str())
        })
        .take(10)
        .map(|a| {
            json!({
                "id": a.id,
                "kind": a.kind,
                "title": a.title,
                "href": format!("/v1/artifacts/{}", a.id),
                "created_at": a.created_at,
            })
        })
        .collect();
    let (snp, tdx) = state.launch_verified_flags().await;
    let receipt = crate::attestation::build_receipt(
        state.config.security_profile.as_deref(),
        Some(format!("{}@{}", session.agent, session.agent_version)),
        session.confidential.as_ref(),
        snp,
        tdx,
    );
    let browser_cap = crate::browser::browser_capability(&state, id).await;
    let badge = browser_cap.get("badge").cloned();
    let egress_connects = crate::demos::session_egress_connects(&state, id).await;
    let drop_reasons = state
        .fluxvm
        .drop_reasons(session.sandbox_id, Some(20))
        .await
        .unwrap_or_else(|_| json!({ "items": [] }));
    Ok(Json(json!({
        "session_id": id,
        "agent": session.agent,
        "status": session.status,
        "tainted_by": session.tainted_by,
        "taint_visible": !session.tainted_by.is_empty(),
        "egress_connects": egress_connects,
        "drop_reasons": drop_reasons,
        "pending_approvals": pending,
        "last_decisions": decisions,
        "upcoming_cron": upcoming,
        "active_goal": active_goal.as_ref().map(|g| json!({
            "id": g.id,
            "title": g.title,
            "status": g.status,
            "href": format!("/v1/goals/{}", g.id),
            "plan": g.plan,
            "allow_hosts": g.allow_hosts,
        })),
        "recent_artifacts": recent_artifacts,
        "model_socket": state.store.get_agent(&session.agent).await.map(|a| a.manifest.model_socket),
        "browser_port": state.store.get_agent(&session.agent).await.and_then(|a| a.manifest.browser_port),
        "browser_view": format!("/v1/sessions/{id}/browser/view"),
        "browser_screenshot": format!("/v1/sessions/{id}/browser/screenshot"),
        "browser_screencast": format!("/v1/sessions/{id}/browser/screencast"),
        "browser_page": format!("/keep/browser?session={id}"),
        "browser": browser_cap,
        "agent_paused_reason": session.agent_paused_reason,
        "browse": {
            "goal_id": session.browse.goal_id,
            "steps": session.browse.steps.len(),
            "network_identity": session.browse.network_identity,
            "cookie_jar": session.browse.cookie_jar,
            "origins": session.browse.limits.origins_seen,
        },
        "badge": badge,
        "security_profile": receipt.security_profile,
        "evidence_class": receipt.evidence_class,
        "honesty": receipt.honesty,
        "attestation": receipt,
        "vault": {
            "secret_backend": crate::unwrap_tokens::SecretBackendKind::HostEnv.as_str(),
            "unwrap_required": state.vault_unwrap_required,
            "unlocked": state.vault_is_unlocked(),
            "honesty": "Secrets still come from host env until Keep 0.2 user-held unwrap.",
        },
    })))
}

#[derive(Debug, serde::Deserialize)]
struct HostRecoverRequest {
    /// First operator recover key (`ZYVOR_AGENT_RECOVER_KEY_A`).
    key_a: String,
    /// Second operator recover key (`ZYVOR_AGENT_RECOVER_KEY_B`).
    key_b: String,
}

/// Break-glass host recover. Confidential profiles always refuse. Measured/standard
/// require both configured recover keys; still software-test honesty (host could
/// already see guest memory).
async fn host_recover_session(
    State(state): State<Arc<AppState>>,
    Path(id): Path<Uuid>,
    Json(req): Json<HostRecoverRequest>,
) -> ApiResult<Json<Value>> {
    let session = state
        .store
        .get_session(id)
        .await
        .ok_or_else(|| ApiError::not_found("session not found"))?;
    let (snp, tdx) = state.launch_verified_flags().await;
    let receipt = crate::attestation::build_receipt(
        state.config.security_profile.as_deref(),
        Some(format!("{}@{}", session.agent, session.agent_version)),
        session.confidential.as_ref(),
        snp,
        tdx,
    );
    if !receipt.host_recover_allowed {
        let _ = state
            .store
            .audit
            .append(
                Some(id),
                AuditPhase::Denied,
                "keep.host_recover.refused",
                None,
                json!({
                    "reason": "confidential",
                    "security_profile": receipt.security_profile,
                    "evidence_class": receipt.evidence_class,
                }),
            )
            .await;
        return Err(ApiError::forbidden(
            "host recover is forbidden on confidential profiles (Keep 0.2)",
        ));
    }
    let (Some(expect_a), Some(expect_b)) = (
        state.config.recover_key_a.as_deref(),
        state.config.recover_key_b.as_deref(),
    ) else {
        return Err(ApiError::unavailable(
            "host recover keys not configured (set ZYVOR_AGENT_RECOVER_KEY_A and _B)",
        ));
    };
    if req.key_a.is_empty()
        || req.key_b.is_empty()
        || req.key_a != expect_a
        || req.key_b != expect_b
        || req.key_a == req.key_b
    {
        let _ = state
            .store
            .audit
            .append(
                Some(id),
                AuditPhase::Denied,
                "keep.host_recover.bad_keys",
                None,
                json!({"security_profile": receipt.security_profile}),
            )
            .await;
        return Err(ApiError::forbidden(
            "host recover requires two distinct configured operator keys",
        ));
    }
    let _ = state
        .store
        .audit
        .append(
            Some(id),
            AuditPhase::Performed,
            "keep.host_recover.granted",
            None,
            json!({
                "security_profile": receipt.security_profile,
                "evidence_class": receipt.evidence_class,
                "honesty": "software-test: host could already see guest memory",
            }),
        )
        .await;
    Ok(Json(json!({
        "session_id": id,
        "granted": true,
        "evidence_class": receipt.evidence_class,
        "honesty": "Break-glass recorded. Evidence remains software-test until Keep 0.2 hardware attestation; the host could already see this VM.",
        "attestation": receipt,
    })))
}

#[derive(Debug, serde::Deserialize)]
struct MintExportTokenRequest {
    /// What may leave the box (e.g. `trajectory:read:7d`). Empty is refused.
    scope: String,
    /// Lifetime in seconds (max 86400).
    #[serde(default = "default_export_ttl")]
    ttl_seconds: u64,
}

fn default_export_ttl() -> u64 {
    3600
}

/// Keep: training default off — mint an explicit scoped export token or nothing leaves.
async fn mint_export_token(
    State(state): State<Arc<AppState>>,
    Json(req): Json<MintExportTokenRequest>,
) -> ApiResult<Json<Value>> {
    let (token, record) = state
        .export_tokens
        .mint(&req.scope, req.ttl_seconds)
        .await
        .map_err(ApiError::bad_request)?;
    let _ = state
        .store
        .audit
        .append(
            None,
            AuditPhase::Performed,
            "keep.export_token.minted",
            None,
            json!({"scope": record.scope, "expires_at": record.expires_at}),
        )
        .await;
    Ok(Json(json!({
        "token": token,
        "scope": record.scope,
        "expires_at": record.expires_at,
        "id": record.id,
        "note": "Present as X-Keep-Export-Token. Without it, GET /v1/audit and pack export are refused.",
    })))
}

#[derive(Debug, serde::Deserialize)]
struct MintUnwrapTokenRequest {
    #[serde(default = "default_vault_scope")]
    scope: String,
    #[serde(default = "default_export_ttl")]
    ttl_seconds: u64,
}

fn default_vault_scope() -> String {
    "vault".into()
}

#[derive(Debug, serde::Deserialize)]
struct UnlockVaultRequest {
    token: String,
}

async fn mint_unwrap_token(
    State(state): State<Arc<AppState>>,
    Json(req): Json<MintUnwrapTokenRequest>,
) -> ApiResult<Json<Value>> {
    let (token, record) = state
        .unwrap_tokens
        .mint(&req.scope, req.ttl_seconds)
        .await
        .map_err(ApiError::bad_request)?;
    let _ = state
        .store
        .audit
        .append(
            None,
            AuditPhase::Performed,
            "keep.unwrap_token.minted",
            None,
            json!({
                "scope": record.scope,
                "expires_at": record.expires_at,
                "honesty": "software-test host-env vault ceremony",
            }),
        )
        .await;
    Ok(Json(json!({
        "token": token,
        "scope": record.scope,
        "expires_at": record.expires_at,
        "id": record.id,
        "secret_backend": crate::unwrap_tokens::SecretBackendKind::HostEnv.as_str(),
        "honesty": "Unlock still reads secrets from host env. Keep 0.2 needs user-held unwrap on attested hardware.",
    })))
}

async fn unlock_vault(
    State(state): State<Arc<AppState>>,
    Json(req): Json<UnlockVaultRequest>,
) -> ApiResult<Json<Value>> {
    state
        .unwrap_tokens
        .authorize(Some(&req.token))
        .await
        .map_err(ApiError::forbidden)?;
    // Lease matches token mint TTL upper bound when required; otherwise no-op unlock.
    let until = Utc::now() + chrono::Duration::hours(1);
    state.unlock_vault_until(until);
    let _ = state
        .store
        .audit
        .append(
            None,
            AuditPhase::Performed,
            "keep.vault.unlocked",
            None,
            json!({
                "until": until,
                "secret_backend": crate::unwrap_tokens::SecretBackendKind::HostEnv.as_str(),
            }),
        )
        .await;
    Ok(Json(json!({
        "unlocked": true,
        "until": until,
        "secret_backend": crate::unwrap_tokens::SecretBackendKind::HostEnv.as_str(),
        "honesty": "Host can still read secrets. This is not Keep 0.2 attested unwrap.",
    })))
}

async fn vault_status(State(state): State<Arc<AppState>>) -> ApiResult<Json<Value>> {
    let names: Vec<String> = state.credentials.names();
    let sources = state.credentials.source_status();
    let backend = crate::unwrap_tokens::SecretBackendKind::from_env();
    let (snp, tdx) = state.launch_verified_flags().await;
    Ok(Json(json!({
        "secret_backend": backend.as_str(),
        "unwrap_required": state.vault_unwrap_required,
        "unlocked": state.vault_is_unlocked(),
        "credential_names": names,
        "sources": sources,
        "sources_honesty": (!sources.is_empty()).then_some(
            "Credentials with a source are read from it (a file or Vault), not from the environment. The value is still in this process's memory while it is used, so the host operator can read it."
        ),
        "user_held": {
            "challenge": "/v1/vault/user-held/challenge",
            "complete": "/v1/vault/user-held/complete",
            "attestation_required": true,
            "snp_launch_verified": snp,
            "tdx_launch_verified": tdx,
            "key_broker": "stub",
        },
        "honesty": "Secrets still come from host env until Keep 0.2 user-held unwrap on attested hardware. Complete is refused while SNP/TDX verified flags are false.",
    })))
}

#[derive(Debug, serde::Deserialize)]
struct UserHeldChallengeRequest {
    #[serde(default = "default_uh_ttl")]
    ttl_seconds: u64,
}

fn default_uh_ttl() -> u64 {
    600
}

#[derive(Debug, serde::Deserialize)]
struct UserHeldCompleteRequest {
    challenge_id: Uuid,
    nonce: String,
    /// Reserved for WebAuthn / YubiKey assertion JSON (opaque until hardware).
    #[serde(default)]
    assertion: Option<Value>,
}

/// Mint a user-held unwrap challenge (phone/YubiKey ceremony design).
async fn user_held_challenge(
    State(state): State<Arc<AppState>>,
    Json(req): Json<UserHeldChallengeRequest>,
) -> ApiResult<Json<Value>> {
    let challenge = state
        .user_held_challenges
        .mint(req.ttl_seconds)
        .await
        .map_err(ApiError::bad_request)?;
    let _ = state
        .store
        .audit
        .append(
            None,
            AuditPhase::Performed,
            "keep.vault.user_held.challenge",
            None,
            json!({
                "id": challenge.id,
                "expires_at": challenge.expires_at,
                "honesty": "challenge only; complete refused without SNP/TDX",
            }),
        )
        .await;
    Ok(Json(json!({
        "id": challenge.id,
        "nonce": challenge.nonce,
        "expires_at": challenge.expires_at,
        "secret_backend": crate::unwrap_tokens::SecretBackendKind::from_env().as_str(),
        "honesty": "Present nonce to phone/YubiKey ceremony. POST /v1/vault/user-held/complete refuses until FluxVM snp/tdx_launch_verified. Secrets remain host-env.",
    })))
}

/// Complete user-held unwrap — fail-closed without verified confidential launch.
async fn user_held_complete(
    State(state): State<Arc<AppState>>,
    Json(req): Json<UserHeldCompleteRequest>,
) -> ApiResult<Json<Value>> {
    let challenge = state
        .user_held_challenges
        .take(req.challenge_id, &req.nonce)
        .await
        .map_err(ApiError::bad_request)?;
    let assertion_present = req.assertion.is_some();
    let (snp, tdx) = state.launch_verified_flags().await;
    if let Err(e) = crate::unwrap_tokens::user_held_complete_allowed(snp, tdx) {
        let _ = state
            .store
            .audit
            .append(
                None,
                AuditPhase::Denied,
                "keep.vault.user_held.complete_refused",
                None,
                json!({
                    "challenge_id": challenge.id,
                    "snp_launch_verified": snp,
                    "tdx_launch_verified": tdx,
                    "reason": e.to_string(),
                }),
            )
            .await;
        return Err(ApiError::forbidden(e));
    }
    // Key-broker stub: after verified launch, unlock host-env vault lease.
    // Wrapped disk-key release (confidential-agent-vms.md) is not implemented.
    let until = Utc::now() + chrono::Duration::hours(1);
    state.unlock_vault_until(until);
    let _ = state
        .store
        .audit
        .append(
            None,
            AuditPhase::Performed,
            "keep.vault.user_held.complete",
            None,
            json!({
                "challenge_id": challenge.id,
                "snp_launch_verified": snp,
                "tdx_launch_verified": tdx,
                "assertion_present": assertion_present,
                "key_broker": "stub",
                "until": until,
            }),
        )
        .await;
    Ok(Json(json!({
        "unlocked": true,
        "until": until,
        "snp_launch_verified": snp,
        "tdx_launch_verified": tdx,
        "key_broker": "stub",
        "secret_backend": crate::unwrap_tokens::SecretBackendKind::from_env().as_str(),
        "honesty": "Launch verified on this host; vault lease granted via key-broker stub. Wrapped disk key / LUKS release is not implemented — secrets may still come from host env.",
    })))
}

/// Keep pack: policy + agent pin + credential *names* (never secrets) + FluxVM migrate notes.
async fn pack_agent(
    State(state): State<Arc<AppState>>,
    Path(name): Path<String>,
    headers: HeaderMap,
) -> ApiResult<Json<Value>> {
    let export = headers
        .get("x-keep-export-token")
        .and_then(|v| v.to_str().ok());
    state
        .export_tokens
        .authorize(export, "pack")
        .await
        .map_err(ApiError::forbidden)?;
    let agent = state
        .store
        .get_agent(&name)
        .await
        .ok_or_else(|| ApiError::not_found("agent not found"))?;
    let policy = crate::policy::KeepPolicy::from_manifest(&agent.manifest);
    let pack = crate::policy::KeepPackManifest {
        version: 1,
        agent: agent.name.clone(),
        agent_version: agent.version.clone(),
        packed_at: Utc::now().to_rfc3339(),
        model_socket: agent.manifest.model_socket.clone(),
        cell_backend: agent.manifest.cell_backend,
        credential_names: agent.manifest.credentials.clone(),
        fluxvm_notes: [
            (
                "disk".into(),
                "Copy the FluxVM qcow2 / workspace for this agent's sandboxes separately; keepctl unpack restores policy only.".into(),
            ),
            (
                "vault".into(),
                "Credential *secrets* stay in host env / credentials file — never in the pack. Re-point ZYVOR_AGENT_CREDENTIALS_FILE on the destination.".into(),
            ),
            (
                "honesty".into(),
                "Measured evidence is software-test until Keep 0.2 + hardware.".into(),
            ),
        ]
        .into_iter()
        .collect(),
    };
    Ok(Json(json!({
        "pack": pack,
        "policy_yaml": policy.to_yaml().map_err(ApiError::internal)?,
        "agent": agent,
    })))
}

/// Minimal Keep cockpit HTML (visible taint + last decisions). Auth via query token for phone browsers.
async fn cockpit_page(Query(q): Query<CockpitPageQuery>) -> impl IntoResponse {
    let session = q.session.unwrap_or_default();
    let html = format!(
        r#"<!DOCTYPE html>
<html lang="en"><head>
<meta charset="utf-8"/><meta name="viewport" content="width=device-width,initial-scale=1"/>
<title>Keep cockpit</title>
<style>
body{{font-family:ui-sans-serif,system-ui,sans-serif;margin:0;background:#0f1419;color:#e7ecf3}}
header{{padding:1rem 1.25rem;border-bottom:1px solid #243044}}
main{{display:grid;gap:1rem;padding:1rem;max-width:1100px;margin:0 auto}}
.card{{background:#162032;border:1px solid #243044;border-radius:12px;padding:1rem}}
.taint{{background:#3a1515;border-color:#7a2a2a}}
.ok{{color:#8dffa8}}.bad{{color:#ff8d8d}}
pre{{white-space:pre-wrap;word-break:break-word;font-size:12px}}
input,button{{font:inherit;padding:.5rem .75rem;border-radius:8px;border:1px solid #345}}
button{{background:#2b6cff;color:#fff;border:0;cursor:pointer}}
</style></head><body>
<header><strong>Keep cockpit</strong> · visible taint · last Sentinel decisions
<div style="opacity:.7;font-size:13px;margin-top:.35rem">Honesty: if FluxVM evidence is software-test, the host can still see the VM.</div>
</header>
<main>
<div class="card">
<label>API base <input id="base" value="" placeholder="http://127.0.0.1:9096" style="width:60%"/></label>
<label>Token <input id="token" type="password" style="width:40%"/></label>
<label>Session <input id="sid" value="{session}" style="width:50%"/></label>
<button id="go">Refresh</button>
</div>
<div id="status" class="card">Load a session.</div>
<div class="card"><h3>Last decisions</h3><pre id="decisions">—</pre></div>
</main>
<script>
const $=id=>document.getElementById(id);
async function refresh(){{
  const base=$('base').value.replace(/\/$/,'')||location.origin;
  const sid=$('sid').value.trim();
  const tok=$('token').value.trim();
  if(!sid){{$('status').textContent='session id required';return;}}
  const r=await fetch(base+'/v1/sessions/'+sid+'/cockpit',{{headers: tok?{{Authorization:'Bearer '+tok}}:{{}}}});
  const j=await r.json();
  if(!r.ok){{$('status').textContent=JSON.stringify(j);return;}}
  const tainted=(j.tainted_by||[]).length>0;
  $('status').className='card'+(tainted?' taint':'');
  $('status').innerHTML=`<div class="${{tainted?'bad':'ok'}}">${{tainted?'TAINTED':'clean'}}</div>
    <div>agent: ${{j.agent}} · status: ${{j.status}}</div>
    <div>tainted_by: ${{(j.tainted_by||[]).join(', ')||'—'}}</div>
    <div>pending approvals: ${{(j.pending_approvals||[]).length}}</div>
    <div style="margin-top:.5rem">evidence: <strong>${{(j.attestation&&j.attestation.evidence_class)||j.evidence_class||'software-test'}}</strong>
      · operator_can_read: ${{(j.attestation&&j.attestation.operator_can_read)!==false}}
      · host_recover: ${{(j.attestation&&j.attestation.host_recover_allowed)?'allowed (dual-key)':'forbidden'}}</div>
    <div style="opacity:.75;margin-top:.5rem">${{j.honesty||''}}</div>`;
  $('decisions').textContent=JSON.stringify(j.last_decisions||[],null,2);
}}
$('go').onclick=refresh;
const u=new URL(location.href); if(u.searchParams.get('token')) $('token').value=u.searchParams.get('token');
if(u.searchParams.get('base')) $('base').value=u.searchParams.get('base');
if($('sid').value) refresh();
</script></body></html>"#,
        session = html_escape(&session)
    );
    (
        [(axum::http::header::CONTENT_TYPE, "text/html; charset=utf-8")],
        html,
    )
}

#[derive(Debug, serde::Deserialize)]
struct CockpitPageQuery {
    #[serde(default)]
    session: Option<String>,
}

fn html_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

async fn list_skills(State(state): State<Arc<AppState>>) -> ApiResult<Json<Value>> {
    let items = state
        .store
        .skills
        .list()
        .await
        .map_err(ApiError::internal)?;
    Ok(Json(json!({"items": items})))
}

async fn publish_skill(
    State(state): State<Arc<AppState>>,
    Json(req): Json<crate::skills::PublishSkillRequest>,
) -> ApiResult<(StatusCode, Json<crate::skills::SkillRecord>)> {
    let record = state
        .store
        .skills
        .publish(req)
        .await
        .map_err(ApiError::bad_request)?;
    Ok((StatusCode::CREATED, Json(record)))
}

async fn get_skill(
    State(state): State<Arc<AppState>>,
    Path(name): Path<String>,
) -> ApiResult<Json<Value>> {
    let current = state
        .store
        .skills
        .get(&name, None)
        .await
        .map_err(|e| ApiError::not_found(e.to_string()))?;
    let versions = state
        .store
        .skills
        .versions(&name)
        .await
        .map_err(ApiError::internal)?;
    Ok(Json(
        json!({"current": current.record, "versions": versions}),
    ))
}

/// Refuses while a deployed agent still lists the skill: its pinned version
/// would vanish and every new session would fail at provisioning.
async fn delete_skill(
    State(state): State<Arc<AppState>>,
    Path(name): Path<String>,
) -> ApiResult<StatusCode> {
    if let Some(agent) = state.store.list_agents().await.into_iter().find(|a| {
        a.manifest
            .skills
            .iter()
            .any(|pin| crate::skills::split_ref(pin).0 == name)
    }) {
        return Err(ApiError::conflict(format!(
            "skill '{name}' is used by agent '{}'; redeploy the agent without it first",
            agent.name
        )));
    }
    match state.store.skills.delete(&name).await {
        Ok(true) => Ok(StatusCode::NO_CONTENT),
        Ok(false) => Err(ApiError::not_found("skill not found")),
        Err(e) => Err(ApiError::bad_request(e)),
    }
}

#[derive(Debug, serde::Deserialize)]
struct MintUserToken {
    user_id: String,
    /// Any of `read`, `run`, `approve`. Default: all three.
    #[serde(default)]
    scopes: Option<Vec<String>>,
    #[serde(default)]
    ttl_seconds: Option<u64>,
}

/// `POST /v1/user-tokens` (operator only): a credential that reaches one user's data.
async fn mint_user_token(
    State(state): State<Arc<AppState>>,
    Json(req): Json<MintUserToken>,
) -> ApiResult<(StatusCode, Json<Value>)> {
    let Some(key) = authz::signing_key_for(&state) else {
        return Err(ApiError::bad_request(format!(
            "user tokens need an operator token or {}",
            authz::SECRET_ENV
        )));
    };
    let scopes: Vec<Scope> = match &req.scopes {
        None => vec![Scope::Read, Scope::Run, Scope::Approve],
        Some(list) => list
            .iter()
            .map(|s| {
                Scope::parse(s).ok_or_else(|| ApiError::bad_request(format!("unknown scope {s:?}")))
            })
            .collect::<Result<_, _>>()?,
    };
    let (token, claims) = authz::mint(
        &key,
        &req.user_id,
        &scopes,
        req.ttl_seconds.unwrap_or(authz::DEFAULT_TTL_SECONDS),
        Utc::now(),
    )
    .map_err(ApiError::bad_request)?;
    let _ = state
        .store
        .audit
        .append(
            None,
            AuditPhase::Performed,
            "keep.user_token.minted",
            Some(req.user_id.clone()),
            json!({ "scopes": claims.scopes, "expires_at": claims.expires_at }),
        )
        .await;
    Ok((
        StatusCode::CREATED,
        Json(json!({
            "token": token,
            "user_id": claims.user_id,
            "scopes": claims.scopes,
            "expires_at": chrono::DateTime::from_timestamp(claims.expires_at, 0),
        })),
    ))
}

/// `POST /v1/users/{id}/revoke-tokens` (operator only): every token issued to the user up to now
/// stops working. Tokens minted after a second from now work again.
async fn revoke_user_tokens(
    State(state): State<Arc<AppState>>,
    Path(user): Path<String>,
) -> ApiResult<Json<Value>> {
    crate::model::validate_user_id(&user).map_err(ApiError::bad_request)?;
    let floor = Utc::now() + chrono::Duration::seconds(1);
    state
        .store
        .set_token_floor(&user, floor)
        .await
        .map_err(ApiError::internal)?;
    let _ = state
        .store
        .audit
        .append(
            None,
            AuditPhase::Performed,
            "keep.user_token.revoked",
            Some(user.clone()),
            json!({ "not_before": floor }),
        )
        .await;
    Ok(Json(json!({ "user_id": user, "not_before": floor })))
}

#[derive(Debug, serde::Deserialize)]
struct UsageQuery {
    #[serde(default)]
    user_id: Option<String>,
    #[serde(default)]
    since: Option<chrono::DateTime<Utc>>,
}

/// `GET /v1/usage?user_id=&since=`: what one user has used. A user token always reads its own.
async fn usage_route(
    State(state): State<Arc<AppState>>,
    Extension(principal): Extension<Principal>,
    Query(q): Query<UsageQuery>,
) -> ApiResult<Json<Value>> {
    let user = match principal.user() {
        Some(u) => u.to_string(),
        None => q
            .user_id
            .ok_or_else(|| ApiError::bad_request("user_id is required"))?,
    };
    let usage = crate::usage::usage_for(&state, &user, q.since).await;
    Ok(Json(json!({ "usage": usage, "limits": {
        "max_runs_per_day": crate::usage::Limits::from_env().max_runs_per_day,
        "max_artifacts": crate::usage::Limits::from_env().max_artifacts,
        "max_model_calls_per_day": crate::usage::Limits::from_env().max_model_calls_per_day,
    }})))
}

/// `GET /v1/inbox`: what a phone shows on open. The user's pending approvals and latest runs.
/// The operator passes `?user_id=`.
/// `GET /v1/whoami`: who the caller is, so a client (a chat page, a phone) need not guess: `{"role": "operator"}` or
/// `{"role": "user", "user_id": ..., "scopes": [...]}`.
async fn whoami(Extension(principal): Extension<Principal>) -> Json<Value> {
    match &principal {
        Principal::Operator => Json(json!({ "role": "operator" })),
        Principal::User { id, scopes } => {
            Json(json!({ "role": "user", "user_id": id, "scopes": scopes }))
        }
    }
}

async fn inbox(
    State(state): State<Arc<AppState>>,
    Extension(principal): Extension<Principal>,
    Query(q): Query<UsageQuery>,
) -> ApiResult<Json<Value>> {
    let user = match principal.user() {
        Some(u) => u.to_string(),
        None => q
            .user_id
            .ok_or_else(|| ApiError::bad_request("user_id is required"))?,
    };
    let mine = authz::session_ids_of(&state, &user).await;
    let signing_key = authz::signing_key_for(&state);
    let pending: Vec<Value> = state
        .store
        .list_approvals()
        .await
        .into_iter()
        .filter(|a| a.status == ApprovalStatus::Pending && mine.contains(&a.session_id))
        .map(|a| {
            let mut v = json!(a);
            // What the phone needs to sign this approval.
            if let Some(key) = &signing_key {
                v["sign"] = crate::devices::signing_info(key, &a);
            }
            v
        })
        .collect();
    let artifacts = state.store.list_artifacts().await;
    let mut sessions: Vec<_> = state
        .store
        .list_sessions()
        .await
        .into_iter()
        .filter(|s| s.user_id.as_deref() == Some(user.as_str()))
        .collect();
    sessions.sort_by_key(|s| std::cmp::Reverse(s.created_at));
    let recent: Vec<Value> = sessions
        .iter()
        .take(20)
        .map(|s| {
            json!({
                "session_id": s.id,
                "agent": s.agent,
                "status": s.status,
                "created_at": s.created_at,
                "artifacts": artifacts.iter()
                    .filter(|a| a.session_id == Some(s.id))
                    .map(|a| json!({"id": a.id, "title": a.title}))
                    .collect::<Vec<_>>(),
            })
        })
        .collect();
    // Memory an agent proposed, waiting for this user to accept or refuse it.
    let memory_proposals: Vec<Value> = state
        .store
        .memory
        .get(&user)
        .await
        .items
        .into_iter()
        .filter(|i| i.status == crate::memory::Status::Proposed)
        .map(|i| json!({ "id": i.id, "text": i.text, "kind": i.kind, "tainted": i.tainted, "session_id": i.source.session_id }))
        .collect();
    // Things an agent thinks you might want done, waiting for a decision.
    let suggestions: Vec<Value> = state
        .store
        .suggestions
        .get(&user)
        .await
        .items
        .into_iter()
        .filter(|i| i.status == crate::suggestions::Status::Pending)
        .map(|i| json!({ "id": i.id, "title": i.title, "reason": i.reason, "agent": i.agent, "tainted": i.tainted }))
        .collect();
    Ok(Json(
        json!({ "user_id": user, "pending_approvals": pending, "memory_proposals": memory_proposals, "suggestions": suggestions, "recent_runs": recent }),
    ))
}

/// `POST /v1/users/{id}/devices` (operator): enrol a phone's public key for a user. The gateway
/// does this after its own strong login, so a stolen user token cannot add a key of its own.
async fn enroll_device(
    State(state): State<Arc<AppState>>,
    Path(user): Path<String>,
    Json(req): Json<crate::devices::EnrollRequest>,
) -> ApiResult<(StatusCode, Json<Value>)> {
    crate::model::validate_user_id(&user).map_err(ApiError::bad_request)?;
    let record =
        crate::devices::build_record(&user, req, Utc::now()).map_err(ApiError::bad_request)?;
    state
        .store
        .save_device(record.clone())
        .await
        .map_err(ApiError::bad_request)?;
    let _ = state
        .store
        .audit
        .append(
            None,
            AuditPhase::Performed,
            "keep.device.enrolled",
            Some(user.clone()),
            json!({ "device_id": record.device_id, "alg": record.alg, "push_kind": record.push.as_ref().map(|p| p.kind.clone()) }),
        )
        .await;
    Ok((StatusCode::CREATED, Json(json!(record))))
}

/// `GET /v1/users/{id}/devices`: the operator, or the user for their own id.
async fn list_devices(State(state): State<Arc<AppState>>, Path(user): Path<String>) -> Json<Value> {
    Json(json!({ "items": state.store.list_devices(&user).await }))
}

/// `DELETE /v1/users/{id}/devices/{device}` (operator): a lost phone stops signing.
async fn remove_device(
    State(state): State<Arc<AppState>>,
    Path((user, device)): Path<(String, String)>,
) -> ApiResult<StatusCode> {
    if !state
        .store
        .delete_device(&user, &device)
        .await
        .map_err(ApiError::internal)?
    {
        return Err(ApiError::not_found("device not found"));
    }
    let _ = state
        .store
        .audit
        .append(
            None,
            AuditPhase::Performed,
            "keep.device.removed",
            Some(user),
            json!({ "device_id": device }),
        )
        .await;
    Ok(StatusCode::NO_CONTENT)
}

async fn list_approvals(
    State(state): State<Arc<AppState>>,
    Extension(principal): Extension<Principal>,
) -> Json<Value> {
    let mut items = state.store.list_approvals().await;
    if let Some(uid) = principal.user() {
        let mine = authz::session_ids_of(&state, uid).await;
        items.retain(|a| mine.contains(&a.session_id));
    }
    Json(json!({"items": items}))
}

async fn create_approval(
    State(state): State<Arc<AppState>>,
    Json(req): Json<CreateApprovalRequest>,
) -> ApiResult<(StatusCode, Json<ApprovalRecord>)> {
    let session = require_session(&state, req.session_id).await?;
    if session.status.is_terminal() {
        return Err(ApiError::conflict("session is not active"));
    }
    if req.prompt.trim().is_empty() {
        return Err(ApiError::bad_request("prompt is required"));
    }
    let record = ApprovalRecord {
        id: Uuid::new_v4(),
        session_id: req.session_id,
        kind: req.kind,
        subject: req.subject,
        planned_action: req.planned_action,
        prompt: req.prompt,
        status: ApprovalStatus::Pending,
        comment: None,
        created_at: Utc::now(),
        decided_at: None,
        source_seq: None,
        grant_scope: None,
        preview: None,
        broker_held: false,
    };
    state
        .store
        .save_approval(record.clone())
        .await
        .map_err(ApiError::internal)?;
    audit_approval_planned(&state, &record).await;
    Ok((StatusCode::CREATED, Json(record)))
}

/// Longest command a speculative run accepts. FluxVM runs it in a shell, so
/// this only bounds what lands in the audit journal and the approval card.
const MAX_SPECULATE_COMMAND_BYTES: usize = 8 * 1024;

#[derive(Debug, serde::Deserialize)]
struct SpeculateSessionRequest {
    command: String,
    #[serde(default)]
    timeout_seconds: Option<u64>,
    #[serde(default)]
    paths: Option<Vec<String>>,
    #[serde(default)]
    ttl_seconds: Option<u64>,
}

/// `POST /v1/sessions/{id}/speculate`: run a command in an isolated copy of the
/// session's sandbox and hold the file changes behind a pending approval.
/// Nothing reaches the sandbox until a person approves; the agent cannot.
async fn speculate_session(
    State(state): State<Arc<AppState>>,
    Path(id): Path<Uuid>,
    Json(req): Json<SpeculateSessionRequest>,
) -> ApiResult<(StatusCode, Json<Value>)> {
    let session = require_session(&state, id).await?;
    if session.status != SessionStatus::Running {
        return Err(ApiError::conflict("only running sessions can speculate"));
    }
    let command = req.command.trim();
    if command.is_empty() {
        return Err(ApiError::bad_request("command is required"));
    }
    if command.len() > MAX_SPECULATE_COMMAND_BYTES {
        return Err(ApiError::bad_request(format!(
            "command must be at most {MAX_SPECULATE_COMMAND_BYTES} bytes"
        )));
    }
    let changeset = state
        .fluxvm
        .speculate(
            session.sandbox_id,
            &crate::fluxvm::SpeculateRequest {
                command: command.to_string(),
                timeout_seconds: req.timeout_seconds,
                paths: req.paths,
                ttl_seconds: req.ttl_seconds,
            },
        )
        .await
        .map_err(|e| ApiError::bad_gateway(format!("speculate: {e:#}")))?;
    if changeset.state != "pending" {
        return Ok((
            StatusCode::OK,
            Json(json!({ "approval": null, "changeset": changeset })),
        ));
    }
    let record = ApprovalRecord {
        id: Uuid::new_v4(),
        session_id: id,
        kind: ApprovalKind::Changeset,
        subject: Some(changeset.id.to_string()),
        planned_action: Some(json!({
            "changeset_id": changeset.id,
            "command": changeset.command,
            "exit_code": changeset.exit_code,
            "paths": changeset.paths,
            "changes": changeset.changes,
            "side_effects": changeset.side_effects,
            "unstaged": changeset.unstaged,
            "expires_at": changeset.expires_at,
        })),
        prompt: format!("Apply the file changes from `{}`?", changeset.command),
        status: ApprovalStatus::Pending,
        comment: None,
        created_at: Utc::now(),
        decided_at: None,
        source_seq: None,
        grant_scope: None,
        preview: None,
        broker_held: false,
    };
    state
        .store
        .save_approval(record.clone())
        .await
        .map_err(ApiError::internal)?;
    audit_approval_planned(&state, &record).await;
    Ok((
        StatusCode::CREATED,
        Json(json!({ "approval": record, "changeset": changeset })),
    ))
}

/// Carry out a person's decision on a changeset approval. Approve means
/// FluxVM approve then apply; deny means reject. The approval is already
/// recorded, so a FluxVM failure is audited and reported, not hidden.
async fn settle_changeset(
    state: &AppState,
    session: &SessionRecord,
    record: &ApprovalRecord,
) -> ApiResult<()> {
    let Some(changeset_id) = record
        .planned_action
        .as_ref()
        .and_then(|a| a.get("changeset_id"))
        .and_then(Value::as_str)
        .and_then(|s| Uuid::parse_str(s).ok())
    else {
        return Err(ApiError::internal("changeset approval has no changeset_id"));
    };
    let approved = record.status == ApprovalStatus::Approved;
    let outcome = if approved {
        match state
            .fluxvm
            .approve_changeset(session.sandbox_id, changeset_id)
            .await
        {
            Ok(_) => {
                state
                    .fluxvm
                    .apply_changeset(session.sandbox_id, changeset_id)
                    .await
            }
            Err(e) => Err(e),
        }
    } else {
        state
            .fluxvm
            .reject_changeset(session.sandbox_id, changeset_id)
            .await
    };
    let (phase, action) = match (&outcome, approved) {
        (Ok(_), true) => (AuditPhase::Performed, "changeset.apply"),
        (Ok(_), false) => (AuditPhase::Performed, "changeset.reject"),
        (Err(_), true) => (AuditPhase::Failed, "changeset.apply"),
        (Err(_), false) => (AuditPhase::Failed, "changeset.reject"),
    };
    let detail = match &outcome {
        Ok(cs) => {
            json!({ "approval_id": record.id, "changeset_id": changeset_id, "state": cs.state })
        }
        Err(e) => {
            json!({ "approval_id": record.id, "changeset_id": changeset_id, "error": format!("{e:#}") })
        }
    };
    if let Err(error) = state
        .store
        .audit
        .append(
            Some(record.session_id),
            phase,
            action.to_string(),
            record.subject.clone(),
            detail,
        )
        .await
    {
        tracing::error!(%error, approval_id = %record.id, "failed to write audit entry");
    }
    outcome.map(|_| ()).map_err(|e| {
        ApiError::bad_gateway(format!(
            "your decision is recorded, but FluxVM could not {} the changeset: {e:#}",
            if approved { "apply" } else { "reject" }
        ))
    })
}

async fn decide_approval(
    State(state): State<Arc<AppState>>,
    Extension(principal): Extension<Principal>,
    Path(id): Path<Uuid>,
    Json(req): Json<DecideApprovalRequest>,
) -> ApiResult<Json<ApprovalRecord>> {
    if !matches!(
        req.decision,
        ApprovalStatus::Approved | ApprovalStatus::Denied
    ) {
        return Err(ApiError::bad_request("decision must be approved or denied"));
    }
    let Some(record) = state.store.get_approval(id).await else {
        return Err(ApiError::not_found("approval not found"));
    };
    if record.status != ApprovalStatus::Pending {
        return Err(ApiError::conflict("approval is already decided"));
    }
    let session = require_session(&state, record.session_id).await?;
    let session_running = session.status == SessionStatus::Running;
    if !session_running && (record.broker_held || record.kind == ApprovalKind::Egress) {
        return Err(ApiError::conflict("session is not running"));
    }
    // A phone-signed decision is verified before anything changes; a user token may be required to sign.
    let signed_by = crate::devices::check_decision(
        &state,
        &principal,
        &record,
        session.user_id.as_deref(),
        &req,
    )
    .await?;
    let scope = (record.kind == ApprovalKind::Egress).then(|| req.scope.unwrap_or_default());
    let Some(record) = state
        .store
        .transition_approval(id, req.decision, req.comment.clone(), scope)
        .await
        .map_err(ApiError::internal)?
    else {
        // Decided, or expired, between the read above and now.
        return Err(ApiError::conflict("approval is already decided"));
    };
    let phase = if record.status == ApprovalStatus::Approved {
        AuditPhase::Approved
    } else {
        AuditPhase::Denied
    };
    if let Err(error) = state
        .store
        .audit
        .append(
            Some(record.session_id),
            phase,
            format!("approval.{}", record.kind.as_str()),
            record.subject.clone(),
            json!({"approval_id": record.id, "comment": record.comment, "device_id": signed_by}),
        )
        .await
    {
        tracing::error!(%error, approval_id = %record.id, "failed to write audit entry");
    }
    let message = json!({
        "approval_id": record.id,
        "decision": record.status,
        "comment": record.comment,
    });
    // A changeset is held by the host, not the agent: carry out the decision on
    // FluxVM and leave the agent alone.
    if record.kind == ApprovalKind::Changeset {
        settle_changeset(&state, &session, &record).await?;
        return Ok(Json(record));
    }
    // An approval the broker is holding (egress, or a send/purchase/DLP/taint hold)
    // unblocks a request already in flight; the agent is not waiting for steering,
    // so there is nothing to send it.
    if session_running && record.kind != ApprovalKind::Egress && !record.broker_held {
        let _ = steer_session(
            State(state),
            Path(record.session_id),
            Json(SteerRequest { message }),
        )
        .await?;
    }
    Ok(Json(record))
}

fn validate_request_id(value: &str) -> ApiResult<()> {
    if value.len() > 128 {
        return Err(ApiError::bad_request(
            "request_id must be at most 128 bytes",
        ));
    }
    if !value
        .bytes()
        .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.' | b':'))
    {
        return Err(ApiError::bad_request(
            "request_id may contain only ASCII letters, digits, '-', '_', '.', and ':'",
        ));
    }
    Ok(())
}

fn random_capability() -> String {
    format!("{}{}", Uuid::new_v4().simple(), Uuid::new_v4().simple())
}

fn shell_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\\''"))
}

fn format_host(host: &str) -> String {
    if host.contains(':') && !host.starts_with('[') {
        format!("[{host}]")
    } else {
        host.to_string()
    }
}

fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    let mut diff = 0u8;
    for (&x, &y) in a.iter().zip(b) {
        diff |= x ^ y;
    }
    diff == 0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quotes_shell_values() {
        assert_eq!(shell_quote("a'b"), "'a'\\''b'");
    }

    #[test]
    fn brackets_ipv6_host() {
        assert_eq!(format_host("fd00::1"), "[fd00::1]");
        assert_eq!(format_host("10.0.0.1"), "10.0.0.1");
    }

    #[test]
    fn validates_idempotency_keys() {
        assert!(validate_request_id("ticket:INC-1042").is_ok());
        assert!(validate_request_id("bad key").is_err());
        assert!(validate_request_id(&"x".repeat(129)).is_err());
    }

    // Regression test for the bug this session's investigation found: a
    // hung FluxVM resume/create call held the per-agent creation lock
    // forever, permanently blocking that agent's session creation until
    // the whole process was restarted. `with_timeout` must convert a
    // never-resolving future into a bounded error instead of hanging.
    #[tokio::test(start_paused = true)]
    async fn with_timeout_bounds_a_call_that_never_resolves() {
        let never = std::future::pending::<anyhow::Result<()>>();
        let result = with_timeout(Duration::from_secs(5), never);
        tokio::time::advance(Duration::from_secs(6)).await;
        assert!(result.await.is_err());
    }

    #[tokio::test(start_paused = true)]
    async fn with_timeout_passes_through_a_call_that_resolves_in_time() {
        let fast = async { Ok::<_, anyhow::Error>(42) };
        let result = with_timeout(Duration::from_secs(5), fast).await;
        assert_eq!(result.unwrap(), 42);
    }

    #[tokio::test(start_paused = true)]
    async fn with_timeout_passes_through_the_inner_error_when_it_resolves_in_time() {
        let failing = async { Err::<(), _>(anyhow::anyhow!("boom")) };
        let result = with_timeout(Duration::from_secs(5), failing).await;
        assert_eq!(result.unwrap_err().to_string(), "boom");
    }

    // ---- confidential: auto / required ----

    async fn fluxvm_that_counts_deletes() -> (String, Arc<std::sync::atomic::AtomicUsize>) {
        let deleted = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let counter = deleted.clone();
        let app = Router::new().route(
            "/v1/vms/{id}",
            axum::routing::delete(move || {
                let counter = counter.clone();
                async move {
                    counter.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                    StatusCode::NO_CONTENT
                }
            }),
        );
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        (url, deleted)
    }

    fn agent_with(mode: crate::model::Confidential) -> crate::model::AgentRecord {
        let mut manifest = crate::egress::ask_tests::manifest(crate::model::EgressMode::Deny, None);
        manifest.confidential = mode;
        crate::model::AgentRecord {
            name: "a".into(),
            version: "v".into(),
            digest_sha256: String::new(),
            manifest,
            created_at: Utc::now(),
        }
    }

    fn sandbox(status: Option<crate::model::ConfidentialStatus>) -> crate::fluxvm::SandboxRecord {
        crate::fluxvm::SandboxRecord {
            id: Uuid::new_v4(),
            simulated: false,
            guest_ip: None,
            status: None,
            confidential: status,
            request: None,
        }
    }

    fn sandbox_with_devices(devices: &[&str]) -> crate::fluxvm::SandboxRecord {
        let mut s = sandbox(None);
        s.request = Some(crate::fluxvm::SandboxRequest {
            vfio_devices: devices.iter().map(|d| d.to_string()).collect(),
        });
        s
    }

    fn agent_with_gpus(n: Option<u8>) -> crate::model::AgentRecord {
        let mut agent = agent_with(crate::model::Confidential::Off);
        agent.manifest.gpus = n;
        agent
    }

    fn status(active: bool, reason: &str) -> Option<crate::model::ConfidentialStatus> {
        Some(crate::model::ConfidentialStatus {
            active,
            tech: active.then(|| "sev-snp".to_string()),
            reason: reason.into(),
        })
    }

    #[tokio::test]
    async fn required_confidential_refuses_and_deletes_a_sandbox_that_is_not_confidential() {
        let (url, deleted) = fluxvm_that_counts_deletes().await;
        let (state, _session) =
            crate::egress::ask_tests::state_and_session_cfg(|c| c.fluxvm_url = url).await;
        let agent = agent_with(crate::model::Confidential::Required);
        let count = || deleted.load(std::sync::atomic::Ordering::SeqCst);

        let inactive = check_confidential(
            &state,
            &agent,
            &sandbox(status(false, "no SEV-SNP or TDX on this host")),
        )
        .await;
        let message = inactive.unwrap_err().message().to_string();
        assert!(message.contains("no SEV-SNP or TDX"), "{message}");
        assert_eq!(count(), 1);

        // An older FluxVM that ignores the request reports nothing: also refused.
        let silent = check_confidential(&state, &agent, &sandbox(None)).await;
        assert!(silent.unwrap_err().message().contains("did not report"));
        assert_eq!(count(), 2);

        // An active confidential sandbox is kept.
        check_confidential(&state, &agent, &sandbox(status(true, "")))
            .await
            .unwrap();
        assert_eq!(count(), 2);
    }

    #[tokio::test]
    async fn auto_confidential_falls_back_to_a_normal_vm() {
        let (url, deleted) = fluxvm_that_counts_deletes().await;
        let (state, _session) =
            crate::egress::ask_tests::state_and_session_cfg(|c| c.fluxvm_url = url).await;
        for mode in [
            crate::model::Confidential::Auto,
            crate::model::Confidential::Off,
        ] {
            let agent = agent_with(mode);
            check_confidential(&state, &agent, &sandbox(status(false, "no hardware")))
                .await
                .unwrap();
            check_confidential(&state, &agent, &sandbox(None))
                .await
                .unwrap();
        }
        assert_eq!(deleted.load(std::sync::atomic::Ordering::SeqCst), 0);
    }

    #[tokio::test]
    async fn gpus_that_were_not_assigned_delete_the_cell_and_assigned_ones_are_journaled() {
        let (url, deleted) = fluxvm_that_counts_deletes().await;
        let (state, _session) =
            crate::egress::ask_tests::state_and_session_cfg(|c| c.fluxvm_url = url).await;
        let agent = agent_with_gpus(Some(2));
        let session = Uuid::new_v4();
        let count = || deleted.load(std::sync::atomic::Ordering::SeqCst);

        // An older FluxVM ignores `gpus` and returns a record with no devices.
        let old = check_gpus(&state, &agent, &sandbox(None), session).await;
        let message = old.unwrap_err().message().to_string();
        assert!(
            message.contains("needs 2 GPU(s) but FluxVM assigned 0"),
            "{message}"
        );
        assert_eq!(count(), 1);

        // Fewer than asked for is refused too.
        let short = check_gpus(
            &state,
            &agent,
            &sandbox_with_devices(&["0000:41:00.0"]),
            session,
        )
        .await;
        assert!(short.unwrap_err().message().contains("assigned 1"));
        assert_eq!(count(), 2);

        // Enough: kept, and the devices are journaled against the session.
        check_gpus(
            &state,
            &agent,
            &sandbox_with_devices(&["0000:41:00.0", "0000:81:00.0"]),
            session,
        )
        .await
        .unwrap();
        assert_eq!(count(), 2);
        let rows = state.store.audit.list(Some(session), 10).await.unwrap();
        let row = rows
            .iter()
            .find(|r| r.action == "keep.cell.gpus")
            .expect("a gpu journal row");
        assert_eq!(
            row.detail["devices"],
            json!(["0000:41:00.0", "0000:81:00.0"])
        );

        // An agent that asked for none is never inspected.
        check_gpus(&state, &agent_with_gpus(None), &sandbox(None), session)
            .await
            .unwrap();
        assert_eq!(count(), 2);
    }

    #[tokio::test]
    async fn no_free_gpu_is_a_503_the_caller_can_retry() {
        let app = Router::new().route(
            "/v1/sandboxes",
            axum::routing::post(|| async {
                (
                    StatusCode::SERVICE_UNAVAILABLE,
                    axum::Json(json!({"error": "2 GPU(s) requested but only 0 free (a free GPU is bound to vfio-pci and not in use)"})),
                )
            }),
        );
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        let (state, _session) =
            crate::egress::ask_tests::state_and_session_cfg(|c| c.fluxvm_url = url).await;
        let agent = agent_with_gpus(Some(2));
        let err = create_cold_sandbox(&state, &agent, Uuid::new_v4(), None)
            .await
            .unwrap_err();
        assert_eq!(err.status(), StatusCode::SERVICE_UNAVAILABLE);
        assert!(
            err.message().contains("no GPU is free"),
            "{}",
            err.message()
        );
        assert!(err.message().contains("only 0 free"), "{}", err.message());
    }

    /// A FluxVM that records which changeset calls reach it.
    #[derive(Clone, Default)]
    struct FakeFlux {
        calls: Arc<std::sync::Mutex<Vec<String>>>,
        fail_apply: bool,
    }

    fn fake_changeset(sandbox: &str, cs: &str, state: &str) -> Value {
        json!({
            "id": cs, "sandbox_id": sandbox, "state": state, "expires_at": 0,
            "command": "sed -i s/a/b/ notes.txt", "exit_code": 0,
            "stdout": "", "stderr": "", "paths": ["notes.txt"],
            "changes": { "modified": ["notes.txt"] }, "side_effects": {}, "unstaged": []
        })
    }

    async fn start_fake_flux(fake: FakeFlux) -> String {
        use axum::{extract::Path, routing::post, Router};
        let cs = "00000000-0000-0000-0000-00000000c0de";
        let record = fake.clone();
        let speculate = move |Path(id): Path<String>| {
            let record = record.clone();
            async move {
                record.calls.lock().unwrap().push("speculate".into());
                Json(fake_changeset(&id, cs, "pending"))
            }
        };
        let record = fake.clone();
        let verb = move |Path((id, cs, verb)): Path<(String, String, String)>| {
            let record = record.clone();
            async move {
                record.calls.lock().unwrap().push(verb.clone());
                if verb == "apply" && record.fail_apply {
                    return (
                        StatusCode::CONFLICT,
                        Json(json!({ "error": "base moved on" })),
                    );
                }
                let state = match verb.as_str() {
                    "approve" => "approved",
                    "apply" => "applied",
                    _ => "rejected",
                };
                (StatusCode::OK, Json(fake_changeset(&id, &cs, state)))
            }
        };
        let app = Router::new()
            .route("/v1/sandboxes/{id}/speculate", post(speculate))
            .route("/v1/sandboxes/{id}/changesets/{cs}/{verb}", post(verb));
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        url
    }

    async fn running_session(state: &AppState, status: SessionStatus) -> SessionRecord {
        let now = Utc::now();
        let record = SessionRecord {
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
            start_policy: SessionStartPolicy::PreferWarm,
            start_mode: SessionStartMode::Cold,
            startup_ms: None,
            expires_at: None,
            sandbox_released: false,
            capability_token: "cap".into(),
            error: None,
            parent_session_id: None,
            user_id: None,
            tainted_by: vec![],
            confidential: None,
            agent_paused_reason: None,
            browse: Default::default(),
        };
        state.store.save_session(record.clone()).await.unwrap();
        record
    }

    fn speculate_req(command: &str) -> Json<SpeculateSessionRequest> {
        Json(SpeculateSessionRequest {
            command: command.into(),
            timeout_seconds: None,
            paths: None,
            ttl_seconds: None,
        })
    }

    async fn decide(
        state: &Arc<AppState>,
        approval: Uuid,
        decision: ApprovalStatus,
    ) -> ApiResult<Json<ApprovalRecord>> {
        decide_approval(
            State(state.clone()),
            Extension(Principal::Operator),
            Path(approval),
            Json(DecideApprovalRequest {
                decision,
                scope: None,
                comment: None,
                device_id: None,
                signature: None,
            }),
        )
        .await
    }

    async fn speculate_for_test(state: &Arc<AppState>, session: Uuid) -> (Uuid, Vec<String>) {
        let (code, Json(body)) = speculate_session(
            State(state.clone()),
            Path(session),
            speculate_req("sed -i s/a/b/ notes.txt"),
        )
        .await
        .unwrap();
        assert_eq!(code, StatusCode::CREATED);
        assert_eq!(body["approval"]["kind"], "changeset");
        assert_eq!(body["approval"]["status"], "pending");
        let id = Uuid::parse_str(body["approval"]["id"].as_str().unwrap()).unwrap();
        (id, vec![])
    }

    #[tokio::test]
    async fn approving_a_changeset_approves_then_applies_it_on_fluxvm() {
        let fake = FakeFlux::default();
        let url = start_fake_flux(fake.clone()).await;
        let state = crate::goals::tests::test_state_with_fluxvm(&url).await;
        let session = running_session(&state, SessionStatus::Running).await;

        let (approval, _) = speculate_for_test(&state, session.id).await;
        // Speculating alone must not change anything on the sandbox.
        assert_eq!(*fake.calls.lock().unwrap(), ["speculate"]);

        let Json(done) = decide(&state, approval, ApprovalStatus::Approved)
            .await
            .unwrap();
        assert_eq!(done.status, ApprovalStatus::Approved);
        assert_eq!(
            *fake.calls.lock().unwrap(),
            ["speculate", "approve", "apply"]
        );
    }

    #[tokio::test]
    async fn denying_a_changeset_rejects_it_and_never_applies() {
        let fake = FakeFlux::default();
        let url = start_fake_flux(fake.clone()).await;
        let state = crate::goals::tests::test_state_with_fluxvm(&url).await;
        let session = running_session(&state, SessionStatus::Running).await;

        let (approval, _) = speculate_for_test(&state, session.id).await;
        let Json(done) = decide(&state, approval, ApprovalStatus::Denied)
            .await
            .unwrap();
        assert_eq!(done.status, ApprovalStatus::Denied);
        assert_eq!(*fake.calls.lock().unwrap(), ["speculate", "reject"]);
    }

    #[tokio::test]
    async fn a_failed_apply_is_reported_and_the_decision_stays_recorded() {
        let fake = FakeFlux {
            fail_apply: true,
            ..Default::default()
        };
        let url = start_fake_flux(fake.clone()).await;
        let state = crate::goals::tests::test_state_with_fluxvm(&url).await;
        let session = running_session(&state, SessionStatus::Running).await;

        let (approval, _) = speculate_for_test(&state, session.id).await;
        let err = decide(&state, approval, ApprovalStatus::Approved)
            .await
            .unwrap_err();
        assert_eq!(err.status, StatusCode::BAD_GATEWAY);
        assert!(err.message.contains("could not apply"), "{}", err.message);
        // The person's decision is not lost, and a second decision is refused.
        let stored = state.store.get_approval(approval).await.unwrap();
        assert_eq!(stored.status, ApprovalStatus::Approved);
        assert!(decide(&state, approval, ApprovalStatus::Approved)
            .await
            .is_err());
    }

    #[tokio::test]
    async fn speculate_needs_a_running_session_and_a_command() {
        let state = crate::goals::tests::test_state().await;
        let stopped = running_session(&state, SessionStatus::Completed).await;
        let err = speculate_session(State(state.clone()), Path(stopped.id), speculate_req("ls"))
            .await
            .unwrap_err();
        assert_eq!(err.status, StatusCode::CONFLICT);

        let running = running_session(&state, SessionStatus::Running).await;
        let err = speculate_session(State(state.clone()), Path(running.id), speculate_req("   "))
            .await
            .unwrap_err();
        assert_eq!(err.status, StatusCode::BAD_REQUEST);
        let long = "x".repeat(MAX_SPECULATE_COMMAND_BYTES + 1);
        let err = speculate_session(State(state), Path(running.id), speculate_req(&long))
            .await
            .unwrap_err();
        assert_eq!(err.status, StatusCode::BAD_REQUEST);
    }
}
