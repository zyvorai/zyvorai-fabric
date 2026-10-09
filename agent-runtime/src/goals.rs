// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: Apache-2.0

//! Keep product loop: goals → plan steps → artifacts → approval for consequential actions.
//!
//! Sessions still do the work; goals orchestrate status and evidence. Mutating
//! Fabric calls stay behind the existing approvals / egress ask path.

use crate::{
    app::{ApiError, ApiResult},
    audit::AuditPhase,
    authz::{self, Principal},
    model::{ApprovalKind, ApprovalRecord, ApprovalStatus},
    AppState,
};
use axum::{
    extract::{Extension, Path, Query, State},
    http::StatusCode,
    Json,
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::sync::Arc;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum GoalStatus {
    Open,
    Blocked,
    Done,
    Cancelled,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum PlanStepStatus {
    #[default]
    Pending,
    Running,
    Blocked,
    Done,
    Skipped,
}

/// Why a plan step is blocked, for the goal worker.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum BlockedOn {
    /// Waiting for the person's decision on the step's approval.
    Approval,
    /// The step's session failed on every attempt.
    Failure,
    /// The person refused the approval (or it expired); the worker never goes past it.
    Rejected,
}

fn default_max_attempts() -> u32 {
    3
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct PlanStep {
    pub id: String,
    pub title: String,
    #[serde(default)]
    pub status: PlanStepStatus,
    /// When true, completing this step opens an approval before marking done.
    #[serde(default)]
    pub requires_approval: bool,
    /// Why, in the planner's own words; carried onto the approval this step opens (`goal_worker::open_approval`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub approval_reason: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub approval_id: Option<Uuid>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub artifact_id: Option<Uuid>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
    /// What the goal worker gives the agent for this step (any JSON). Without it the agent gets the goal and step titles.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub input: Option<Value>,
    /// The session the goal worker started for this step (the latest attempt).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session_id: Option<Uuid>,
    /// How many sessions the worker has started for this step.
    #[serde(default)]
    pub attempts: u32,
    /// After a failed attempt, the earliest moment the worker tries again.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub next_attempt_at: Option<DateTime<Utc>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_error: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub blocked_on: Option<BlockedOn>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GoalRecord {
    pub id: Uuid,
    pub title: String,
    #[serde(default)]
    pub description: String,
    /// Pack / agent name this goal is for (e.g. `infra-ops`).
    pub agent: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub user_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session_id: Option<Uuid>,
    pub status: GoalStatus,
    #[serde(default)]
    pub plan: Vec<PlanStep>,
    #[serde(default)]
    pub artifact_ids: Vec<Uuid>,
    /// Goal-bound tabs: browser open must be ⊆ this list when non-empty.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub allow_hosts: Vec<String>,
    /// Let the goal worker run the plan step by step (see [`crate::goal_worker`]). Off unless the goal says so.
    #[serde(default)]
    pub autorun: bool,
    /// The planning session started for this goal that may still propose a plan (see [`crate::goal_plan`]).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub planning_session_id: Option<Uuid>,
    /// A plan an agent proposed, waiting for the person to accept or reject it. Nothing in it runs until accepted.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub proposed_plan: Option<crate::goal_plan::ProposedPlan>,
    /// How many sessions the worker may start for one step before it blocks the goal (1 to 10).
    #[serde(default = "default_max_attempts")]
    pub max_attempts: u32,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[cfg(test)]
pub(crate) fn test_goal(plan: Vec<PlanStep>) -> GoalRecord {
    let now = Utc::now();
    GoalRecord {
        id: Uuid::new_v4(),
        title: "Trip".into(),
        description: "plan a trip".into(),
        agent: "chat".into(),
        user_id: None,
        session_id: None,
        status: GoalStatus::Open,
        plan,
        artifact_ids: vec![],
        allow_hosts: vec![],
        autorun: true,
        planning_session_id: None,
        proposed_plan: None,
        max_attempts: 3,
        created_at: now,
        updated_at: now,
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArtifactRecord {
    pub id: Uuid,
    pub kind: String,
    pub title: String,
    /// Markdown or plain text body (no secrets).
    pub body: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub content_type: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub goal_id: Option<Uuid>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session_id: Option<Uuid>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent: Option<String>,
    #[serde(default)]
    pub metadata: Value,
    pub created_at: DateTime<Utc>,
    /// After this instant the artifact is hidden and swept. `None` keeps it until deleted.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expires_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Deserialize)]
pub struct CreateGoalRequest {
    pub title: String,
    #[serde(default)]
    pub description: String,
    pub agent: String,
    #[serde(default)]
    pub user_id: Option<String>,
    #[serde(default)]
    pub session_id: Option<Uuid>,
    #[serde(default)]
    pub plan: Vec<CreatePlanStep>,
    #[serde(default)]
    pub allow_hosts: Vec<String>,
    /// Let the goal worker run the plan (needs a plan and a deployed agent).
    #[serde(default)]
    pub autorun: bool,
    #[serde(default)]
    pub max_attempts: Option<u32>,
}

#[derive(Debug, Deserialize)]
pub struct CreatePlanStep {
    pub title: String,
    #[serde(default)]
    pub requires_approval: bool,
    /// Why this step needs a yes; shown on the approval it opens, alongside the usual prompt.
    #[serde(default)]
    pub approval_reason: Option<String>,
    #[serde(default)]
    pub detail: Option<String>,
    /// What the agent is given for this step when the goal worker runs it (JSON, at most 16 KiB).
    #[serde(default)]
    pub input: Option<Value>,
}

#[derive(Debug, Deserialize)]
pub struct PatchGoalRequest {
    #[serde(default)]
    pub status: Option<GoalStatus>,
    #[serde(default)]
    pub session_id: Option<Uuid>,
    #[serde(default)]
    pub plan: Option<Vec<PlanStep>>,
    #[serde(default)]
    pub allow_hosts: Option<Vec<String>>,
    /// Pause (`false`) or resume (`true`) the goal worker for this goal.
    #[serde(default)]
    pub autorun: Option<bool>,
    #[serde(default)]
    pub max_attempts: Option<u32>,
}

#[derive(Debug, Deserialize)]
pub struct AdvanceStepRequest {
    pub step_id: String,
    pub status: PlanStepStatus,
    #[serde(default)]
    pub artifact_id: Option<Uuid>,
    #[serde(default)]
    pub detail: Option<String>,
    /// Prompt used when opening an approval for `requires_approval` steps.
    #[serde(default)]
    pub approval_prompt: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct CreateArtifactRequest {
    pub kind: String,
    pub title: String,
    pub body: String,
    #[serde(default)]
    pub content_type: Option<String>,
    #[serde(default)]
    pub goal_id: Option<Uuid>,
    #[serde(default)]
    pub session_id: Option<Uuid>,
    #[serde(default)]
    pub agent: Option<String>,
    #[serde(default)]
    pub metadata: Option<Value>,
    /// Keep the artifact for this many seconds (1 s to 10 years).
    #[serde(default)]
    pub ttl_seconds: Option<i64>,
}

#[derive(Debug, Deserialize)]
pub struct ListQuery {
    #[serde(default)]
    pub agent: Option<String>,
    #[serde(default)]
    pub goal_id: Option<Uuid>,
    #[serde(default)]
    pub session_id: Option<Uuid>,
    #[serde(default)]
    pub kind: Option<String>,
    #[serde(default)]
    pub limit: Option<usize>,
    /// Artifacts produced by this use case (matches `metadata.demo`).
    #[serde(default)]
    pub use_case: Option<String>,
    /// Only artifacts created at or after this instant (RFC 3339).
    #[serde(default)]
    pub since: Option<DateTime<Utc>>,
    /// Goals of this user only. Honoured for the operator; a user token always sees only its own.
    #[serde(default)]
    pub user_id: Option<String>,
}

pub(crate) async fn list_goals(
    State(state): State<Arc<AppState>>,
    Extension(principal): Extension<Principal>,
    Query(q): Query<ListQuery>,
) -> Json<Value> {
    let mut items = state.store.list_goals().await;
    // A user sees only their own goals.
    if let Some(user) = principal.user() {
        items.retain(|g| g.user_id.as_deref() == Some(user));
    } else if let Some(user) = q.user_id.as_deref() {
        items.retain(|g| g.user_id.as_deref() == Some(user));
    }
    if let Some(agent) = q.agent.as_deref() {
        items.retain(|g| g.agent == agent);
    }
    if let Some(sid) = q.session_id {
        items.retain(|g| g.session_id == Some(sid));
    }
    let limit = q.limit.unwrap_or(100).clamp(1, 500);
    items.truncate(limit);
    Json(json!({ "items": items }))
}

/// The most steps, goals and automatic goals a user token may have (the operator has no such limits).
pub const USER_MAX_STEPS: usize = 20;
pub const USER_MAX_GOALS: usize = 100;
pub const USER_MAX_ACTIVE_AUTORUN: usize = 5;

/// A user's non-finished goals that the worker is running.
pub(crate) async fn active_autorun(state: &AppState, user: &str) -> usize {
    state
        .store
        .list_goals()
        .await
        .iter()
        .filter(|g| {
            g.user_id.as_deref() == Some(user)
                && g.autorun
                && matches!(g.status, GoalStatus::Open | GoalStatus::Blocked)
        })
        .count()
}

/// The route: a user token creates goals only for itself.
pub(crate) async fn create_goal_route(
    State(state): State<Arc<AppState>>,
    Extension(principal): Extension<Principal>,
    Json(req): Json<CreateGoalRequest>,
) -> ApiResult<(StatusCode, Json<GoalRecord>)> {
    let mut req = req;
    if let Some(user) = principal.user() {
        if req.user_id.as_deref().is_some_and(|u| u != user) {
            return Err(ApiError::bad_request(
                "a user token can only create goals for itself",
            ));
        }
        if req.session_id.is_some() || !req.allow_hosts.is_empty() {
            return Err(ApiError::forbidden(
                "session_id and allow_hosts are set by the operator",
            ));
        }
        if req.plan.len() > USER_MAX_STEPS {
            return Err(ApiError::bad_request(format!(
                "a goal may have at most {USER_MAX_STEPS} steps"
            )));
        }
        let mine = state
            .store
            .list_goals()
            .await
            .iter()
            .filter(|g| g.user_id.as_deref() == Some(user))
            .count();
        if mine >= USER_MAX_GOALS {
            return Err(ApiError::too_many(format!(
                "at most {USER_MAX_GOALS} goals; delete or finish some first"
            )));
        }
        if req.autorun && active_autorun(&state, user).await >= USER_MAX_ACTIVE_AUTORUN {
            return Err(ApiError::too_many(format!(
                "at most {USER_MAX_ACTIVE_AUTORUN} goals may run automatically at once"
            )));
        }
        // a user goal always names a deployed agent, whether or not it runs by itself
        if state.store.get_agent(req.agent.trim()).await.is_none() {
            return Err(ApiError::not_found("agent not found"));
        }
        req.user_id = Some(user.to_string());
    }
    create_goal(State(state), Json(req)).await
}

pub(crate) async fn create_goal(
    State(state): State<Arc<AppState>>,
    Json(req): Json<CreateGoalRequest>,
) -> ApiResult<(StatusCode, Json<GoalRecord>)> {
    if req.title.trim().is_empty() {
        return Err(ApiError::bad_request("title is required"));
    }
    if req.agent.trim().is_empty() {
        return Err(ApiError::bad_request("agent is required"));
    }
    let now = Utc::now();
    let plan: Vec<PlanStep> = req
        .plan
        .into_iter()
        .enumerate()
        .map(|(i, s)| PlanStep {
            id: format!("s{}", i + 1),
            title: s.title,
            status: PlanStepStatus::Pending,
            requires_approval: s.requires_approval,
            approval_reason: s.approval_reason,
            approval_id: None,
            artifact_id: None,
            detail: s.detail,
            input: s.input,
            ..Default::default()
        })
        .collect();
    if plan.iter().any(|s| {
        s.input
            .as_ref()
            .is_some_and(|v| v.to_string().len() > 16 * 1024)
    }) {
        return Err(ApiError::bad_request("a step input may be at most 16 KiB"));
    }
    if plan.iter().any(|s| {
        s.approval_reason
            .as_ref()
            .is_some_and(|r| r.chars().count() > crate::goal_plan::MAX_REASON_CHARS)
    }) {
        return Err(ApiError::bad_request(format!(
            "a step's approval_reason may be at most {} characters",
            crate::goal_plan::MAX_REASON_CHARS
        )));
    }
    let max_attempts = req.max_attempts.unwrap_or_else(default_max_attempts);
    if !(1..=10).contains(&max_attempts) {
        return Err(ApiError::bad_request("max_attempts must be 1 to 10"));
    }
    if req.autorun {
        if plan.is_empty() {
            return Err(ApiError::bad_request("an autorun goal needs a plan"));
        }
        if state.store.get_agent(req.agent.trim()).await.is_none() {
            return Err(ApiError::not_found(
                "an autorun goal needs a deployed agent",
            ));
        }
        if let Some(user) = req.user_id.as_deref() {
            crate::model::validate_user_id(user).map_err(ApiError::bad_request)?;
        }
    }
    let record = GoalRecord {
        id: Uuid::new_v4(),
        title: req.title.trim().to_string(),
        description: req.description,
        agent: req.agent,
        user_id: req.user_id,
        session_id: req.session_id,
        status: GoalStatus::Open,
        plan,
        artifact_ids: vec![],
        allow_hosts: req.allow_hosts,
        autorun: req.autorun,
        planning_session_id: None,
        proposed_plan: None,
        max_attempts,
        created_at: now,
        updated_at: now,
    };
    state
        .store
        .save_goal(record.clone())
        .await
        .map_err(ApiError::internal)?;
    let _ = state
        .store
        .audit
        .append(
            None,
            AuditPhase::Performed,
            "keep.goal.created",
            Some(record.agent.clone()),
            json!({ "goal_id": record.id, "title": record.title }),
        )
        .await;
    Ok((StatusCode::CREATED, Json(record)))
}

pub(crate) async fn get_goal(
    State(state): State<Arc<AppState>>,
    Path(id): Path<Uuid>,
) -> ApiResult<Json<GoalRecord>> {
    state
        .store
        .get_goal(id)
        .await
        .map(Json)
        .ok_or_else(|| ApiError::not_found("goal not found"))
}

/// The route: a goal that is not yours looks like one that does not exist (the middleware already says so; this is the second check).
pub(crate) async fn get_goal_route(
    State(state): State<Arc<AppState>>,
    Extension(principal): Extension<Principal>,
    Path(id): Path<Uuid>,
) -> ApiResult<Json<GoalRecord>> {
    let Json(goal) = get_goal(State(state), Path(id)).await?;
    match principal.user() {
        Some(user) if goal.user_id.as_deref() != Some(user) => {
            Err(ApiError::not_found("goal not found"))
        }
        _ => Ok(Json(goal)),
    }
}

/// The route: a user may cancel their goal and switch its automatic running on or off (within the limits); the plan, the bound session and the
/// allowed hosts stay the operator's.
pub(crate) async fn patch_goal_route(
    State(state): State<Arc<AppState>>,
    Extension(principal): Extension<Principal>,
    Path(id): Path<Uuid>,
    Json(req): Json<PatchGoalRequest>,
) -> ApiResult<Json<GoalRecord>> {
    if let Some(user) = principal.user() {
        let goal = state
            .store
            .get_goal(id)
            .await
            .filter(|g| g.user_id.as_deref() == Some(user))
            .ok_or_else(|| ApiError::not_found("goal not found"))?;
        if req.session_id.is_some() || req.plan.is_some() || req.allow_hosts.is_some() {
            return Err(ApiError::forbidden(
                "the plan, session_id and allow_hosts are set by the operator",
            ));
        }
        if req
            .status
            .as_ref()
            .is_some_and(|s| *s != GoalStatus::Cancelled)
        {
            return Err(ApiError::forbidden(
                "a user can cancel a goal; other statuses follow from its steps",
            ));
        }
        if req.autorun == Some(true)
            && !goal.autorun
            && active_autorun(&state, user).await >= USER_MAX_ACTIVE_AUTORUN
        {
            return Err(ApiError::too_many(format!(
                "at most {USER_MAX_ACTIVE_AUTORUN} goals may run automatically at once"
            )));
        }
        if req.autorun == Some(true)
            && matches!(goal.status, GoalStatus::Done | GoalStatus::Cancelled)
        {
            return Err(ApiError::conflict(
                "a finished or cancelled goal cannot be run again",
            ));
        }
    }
    patch_goal(State(state), Path(id), Json(req)).await
}

pub(crate) async fn patch_goal(
    State(state): State<Arc<AppState>>,
    Path(id): Path<Uuid>,
    Json(req): Json<PatchGoalRequest>,
) -> ApiResult<Json<GoalRecord>> {
    let mut goal = state
        .store
        .get_goal(id)
        .await
        .ok_or_else(|| ApiError::not_found("goal not found"))?;
    if let Some(status) = req.status {
        goal.status = status;
    }
    if let Some(session_id) = req.session_id {
        goal.session_id = Some(session_id);
    }
    if let Some(plan) = req.plan {
        goal.plan = plan;
    }
    if let Some(hosts) = req.allow_hosts {
        goal.allow_hosts = hosts;
    }
    if let Some(autorun) = req.autorun {
        goal.autorun = autorun;
    }
    if let Some(n) = req.max_attempts {
        if !(1..=10).contains(&n) {
            return Err(ApiError::bad_request("max_attempts must be 1 to 10"));
        }
        goal.max_attempts = n;
    }
    goal.updated_at = Utc::now();
    state
        .store
        .save_goal(goal.clone())
        .await
        .map_err(ApiError::internal)?;
    Ok(Json(goal))
}

#[derive(Debug, Deserialize)]
pub struct GoalBrowseRequest {
    pub url: String,
}

/// Open a URL under this goal's allow_hosts (binds session.browse.goal_id).
pub(crate) async fn goal_browse(
    State(state): State<Arc<AppState>>,
    Path(id): Path<Uuid>,
    Json(req): Json<GoalBrowseRequest>,
) -> ApiResult<Json<Value>> {
    let goal = state
        .store
        .get_goal(id)
        .await
        .ok_or_else(|| ApiError::not_found("goal not found"))?;
    let session_id = goal
        .session_id
        .ok_or_else(|| ApiError::bad_request("goal needs session_id before browse"))?;
    if let Some(host) = crate::browse_ifc::host_from_url(&req.url) {
        if !goal.allow_hosts.is_empty()
            && !crate::policy::host_matches_list(&host, &goal.allow_hosts)
        {
            return Err(ApiError::forbidden("host outside goal.allow_hosts"));
        }
    }
    let _ = state
        .store
        .update_session(session_id, |s| {
            s.browse.goal_id = Some(id);
        })
        .await
        .map_err(ApiError::internal)?;
    let result = crate::browser::driver_call(
        &state,
        session_id,
        json!({ "tool": "open", "url": req.url }),
    )
    .await?;
    Ok(Json(json!({
        "goal_id": id,
        "session_id": session_id,
        "result": result,
    })))
}

/// Advance a plan step; opens an approval when moving a `requires_approval` step to done/blocked.
pub(crate) async fn advance_step(
    State(state): State<Arc<AppState>>,
    Path(id): Path<Uuid>,
    Json(req): Json<AdvanceStepRequest>,
) -> ApiResult<Json<GoalRecord>> {
    let mut goal = state
        .store
        .get_goal(id)
        .await
        .ok_or_else(|| ApiError::not_found("goal not found"))?;
    if goal.status == GoalStatus::Cancelled {
        return Err(ApiError::conflict("the goal is cancelled"));
    }
    if goal.autorun {
        return Err(ApiError::conflict(
            "the goal worker runs this goal; pause it (autorun false) to advance it by hand",
        ));
    }
    let step = goal
        .plan
        .iter_mut()
        .find(|s| s.id == req.step_id)
        .ok_or_else(|| ApiError::not_found("plan step not found"))?;

    if step.requires_approval
        && matches!(req.status, PlanStepStatus::Done)
        && step.approval_id.is_none()
    {
        let session_id = goal
            .session_id
            .ok_or_else(|| ApiError::bad_request("goal needs session_id before approval steps"))?;
        let prompt = req
            .approval_prompt
            .clone()
            .unwrap_or_else(|| format!("Approve goal step: {}", step.title));
        let approval = ApprovalRecord {
            id: Uuid::new_v4(),
            session_id,
            kind: ApprovalKind::Send,
            subject: Some(format!("goal:{}:{}", goal.id, step.id)),
            planned_action: Some(json!({
                "goal_id": goal.id,
                "step_id": step.id,
                "title": step.title,
                "agent": goal.agent,
            })),
            prompt,
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
            .save_approval(approval.clone())
            .await
            .map_err(ApiError::internal)?;
        crate::notify::approval_requested(&state, &approval);
        step.approval_id = Some(approval.id);
        step.status = PlanStepStatus::Blocked;
        let step_id = step.id.clone();
        let approval_id = approval.id;
        goal.status = GoalStatus::Blocked;
        goal.updated_at = Utc::now();
        state
            .store
            .save_goal(goal.clone())
            .await
            .map_err(ApiError::internal)?;
        let _ = state
            .store
            .audit
            .append(
                Some(session_id),
                AuditPhase::Planned,
                "keep.goal.step.approval",
                Some(goal.agent.clone()),
                json!({ "goal_id": goal.id, "step_id": step_id, "approval_id": approval_id }),
            )
            .await;
        return Ok(Json(goal));
    }

    // An approval step is done only once its approval was granted. A pending, denied or expired approval must
    // not be skipped by asking again (the request above only opens the approval the first time).
    if step.requires_approval && matches!(req.status, PlanStepStatus::Done) {
        if let Some(approval_id) = step.approval_id {
            let approval = state.store.get_approval(approval_id).await;
            match approval.as_ref().map(|a| &a.status) {
                Some(ApprovalStatus::Approved) => {}
                Some(ApprovalStatus::Pending) => {
                    return Err(ApiError::conflict(
                        "the approval for this step is still pending",
                    ));
                }
                other => {
                    let reason = match other {
                        Some(ApprovalStatus::Denied) => "denied",
                        Some(ApprovalStatus::Expired) => "expired",
                        _ => "missing",
                    };
                    step.status = PlanStepStatus::Blocked;
                    step.detail = Some(format!("approval {reason}"));
                    goal.status = GoalStatus::Blocked;
                    goal.updated_at = Utc::now();
                    state
                        .store
                        .save_goal(goal)
                        .await
                        .map_err(ApiError::internal)?;
                    return Err(ApiError::conflict(format!(
                        "the approval for this step was {reason}; the step stays blocked"
                    )));
                }
            }
        }
    }

    step.status = req.status;
    if let Some(aid) = req.artifact_id {
        step.artifact_id = Some(aid);
        if !goal.artifact_ids.contains(&aid) {
            goal.artifact_ids.push(aid);
        }
    }
    if let Some(detail) = req.detail {
        step.detail = Some(detail);
    }
    if goal
        .plan
        .iter()
        .all(|s| matches!(s.status, PlanStepStatus::Done | PlanStepStatus::Skipped))
    {
        goal.status = GoalStatus::Done;
    } else if goal
        .plan
        .iter()
        .any(|s| matches!(s.status, PlanStepStatus::Blocked))
    {
        goal.status = GoalStatus::Blocked;
    } else {
        goal.status = GoalStatus::Open;
    }
    goal.updated_at = Utc::now();
    state
        .store
        .save_goal(goal.clone())
        .await
        .map_err(ApiError::internal)?;
    Ok(Json(goal))
}

pub(crate) async fn list_artifacts(
    State(state): State<Arc<AppState>>,
    Extension(principal): Extension<Principal>,
    Query(q): Query<ListQuery>,
) -> Json<Value> {
    let mut items = state.store.list_artifacts().await;
    // A user sees only artifacts made in their own sessions.
    if let Some(uid) = principal.user() {
        let mine = authz::session_ids_of(&state, uid).await;
        items.retain(|a| a.session_id.is_some_and(|sid| mine.contains(&sid)));
    }
    if let Some(gid) = q.goal_id {
        items.retain(|a| a.goal_id == Some(gid));
    }
    if let Some(sid) = q.session_id {
        items.retain(|a| a.session_id == Some(sid));
    }
    if let Some(agent) = q.agent.as_deref() {
        items.retain(|a| a.agent.as_deref() == Some(agent));
    }
    if let Some(kind) = q.kind.as_deref() {
        items.retain(|a| a.kind == kind);
    }
    if let Some(uc) = q.use_case.as_deref() {
        items.retain(|a| a.metadata.get("demo").and_then(Value::as_str) == Some(uc));
    }
    if let Some(since) = q.since {
        items.retain(|a| a.created_at >= since);
    }
    let limit = q.limit.unwrap_or(100).clamp(1, 500);
    items.truncate(limit);
    Json(json!({ "items": items }))
}

pub(crate) async fn create_artifact(
    State(state): State<Arc<AppState>>,
    Json(req): Json<CreateArtifactRequest>,
) -> ApiResult<(StatusCode, Json<ArtifactRecord>)> {
    if req.title.trim().is_empty() || req.kind.trim().is_empty() {
        return Err(ApiError::bad_request("kind and title are required"));
    }
    if req.body.len() > 512 * 1024 {
        return Err(ApiError::bad_request("artifact body exceeds 512 KiB"));
    }
    // Soft secret scan — refuse obvious key material in artifacts.
    let lower = req.body.to_ascii_lowercase();
    for needle in [
        "api_key=",
        "begin private key",
        "password=",
        "authorization: bearer ",
    ] {
        if lower.contains(needle) {
            return Err(ApiError::bad_request(
                "artifact body looks like it contains a secret",
            ));
        }
    }
    let expires_at = match req.ttl_seconds {
        None => None,
        Some(s) if (1..=315_360_000).contains(&s) => {
            Some(Utc::now() + chrono::Duration::seconds(s))
        }
        Some(_) => {
            return Err(ApiError::bad_request(
                "ttl_seconds must be between 1 and 315360000",
            ))
        }
    };
    let record = ArtifactRecord {
        id: Uuid::new_v4(),
        kind: req.kind,
        title: req.title.trim().to_string(),
        body: req.body,
        content_type: req.content_type.or_else(|| Some("text/markdown".into())),
        goal_id: req.goal_id,
        session_id: req.session_id,
        agent: req.agent,
        metadata: req.metadata.unwrap_or(Value::Null),
        created_at: Utc::now(),
        expires_at,
    };
    state
        .store
        .save_artifact(record.clone())
        .await
        .map_err(ApiError::internal)?;
    if let Some(gid) = record.goal_id {
        if let Some(mut goal) = state.store.get_goal(gid).await {
            if !goal.artifact_ids.contains(&record.id) {
                goal.artifact_ids.push(record.id);
                goal.updated_at = Utc::now();
                let _ = state.store.save_goal(goal).await;
            }
        }
    }
    let _ = state
        .store
        .audit
        .append(
            record.session_id,
            AuditPhase::Performed,
            "keep.artifact.created",
            record.agent.clone(),
            json!({ "artifact_id": record.id, "kind": record.kind, "title": record.title }),
        )
        .await;
    Ok((StatusCode::CREATED, Json(record)))
}

pub(crate) async fn get_artifact(
    State(state): State<Arc<AppState>>,
    Path(id): Path<Uuid>,
) -> ApiResult<Json<ArtifactRecord>> {
    state
        .store
        .get_artifact(id)
        .await
        .map(Json)
        .ok_or_else(|| ApiError::not_found("artifact not found"))
}

/// Line diff of two artifacts' bodies: `a` is treated as the older side.
pub(crate) async fn diff_artifacts(
    State(state): State<Arc<AppState>>,
    Path((a, b)): Path<(Uuid, Uuid)>,
) -> ApiResult<Json<Value>> {
    let left = state
        .store
        .get_artifact(a)
        .await
        .ok_or_else(|| ApiError::not_found("artifact not found"))?;
    let right = state
        .store
        .get_artifact(b)
        .await
        .ok_or_else(|| ApiError::not_found("artifact not found"))?;
    let lines = crate::artifact_diff::diff_lines(&left.body, &right.body).ok_or_else(|| {
        ApiError::bad_request(format!(
            "artifact too large to diff (over {} lines)",
            crate::artifact_diff::MAX_DIFF_LINES
        ))
    })?;
    let summary = crate::artifact_diff::summarize(&lines);
    Ok(Json(json!({
        "a": { "id": left.id, "title": left.title, "created_at": left.created_at },
        "b": { "id": right.id, "title": right.title, "created_at": right.created_at },
        "summary": summary,
        "lines": lines,
    })))
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::{config::Config, AppState};
    use axum::extract::{Path, Query, State};

    pub(crate) async fn test_state() -> Arc<AppState> {
        test_state_with_fluxvm("http://127.0.0.1:1").await
    }

    /// Like [`test_state`], with FluxVM calls going to `fluxvm_url`.
    pub(crate) async fn test_state_with_fluxvm(fluxvm_url: &str) -> Arc<AppState> {
        let root = std::env::temp_dir().join(format!("zyvor-goals-{}", Uuid::new_v4()));
        let config = Config {
            listen: "127.0.0.1:0".parse().unwrap(),
            egress_listen: "127.0.0.1:0".parse().unwrap(),
            state_dir: root.join("state"),
            snapshot_dir: root.join("snap"),
            fluxvm_url: fluxvm_url.into(),
            fluxvm_token: None,
            api_token: None,
            credentials_file: None,
            skill_scopes_file: None,
            sentinel: None,
            guard: None,
            approval_webhook: None,
            proxy_listen: None,
            proxy_connect_ports: vec![443],
            mitm_ca_dir: None,
            extra_ca_files: vec![],
            confine_all: false,
            security_profile: None,
            recover_key_a: None,
            recover_key_b: None,
            max_vcpus: None,
            max_memory_mib: None,
            egress_advertise_host: None,
            sync_interval_ms: 300,
            guest_start_timeout_secs: 30,
            thread_retention_days: None,
            receipt_retention_days: None,
            memory_proposal_retention_days: None,
            suggestion_retention_days: None,
            goal_tick_ms: 5000,
            goal_retry_base_secs: 15,
            planner_agent: None,
            event_retention_days: None,
            idle_scan_interval_ms: 1000,
            warm_pool_reconcile_interval_ms: 2000,
            warm_pool_max_create_per_tick: 2,
            warm_pool_claim_stale_secs: 300,
            expiry_scan_interval_ms: 1000,
        };
        AppState::from_config(config).await.unwrap()
    }

    #[test]
    fn plan_step_defaults_pending() {
        let s: PlanStep = serde_json::from_value(json!({
            "id": "s1", "title": "Investigate"
        }))
        .unwrap();
        assert_eq!(s.status, PlanStepStatus::Pending);
        assert!(!s.requires_approval);
    }

    #[tokio::test]
    async fn create_goal_and_artifact_roundtrip() {
        let state = test_state().await;
        let (status, Json(goal)) = create_goal(
            State(state.clone()),
            Json(CreateGoalRequest {
                title: "Investigate alerts".into(),
                description: "infra-ops demo".into(),
                agent: "infra-ops".into(),
                user_id: None,
                session_id: None,
                plan: vec![
                    CreatePlanStep {
                        title: "Read alerts".into(),
                        requires_approval: false,
                        approval_reason: None,
                        detail: None,
                        input: None,
                    },
                    CreatePlanStep {
                        title: "Restart VM".into(),
                        requires_approval: true,
                        approval_reason: None,
                        detail: None,
                        input: None,
                    },
                ],
                allow_hosts: vec![],
                autorun: false,
                max_attempts: None,
            }),
        )
        .await
        .unwrap();
        assert_eq!(status, StatusCode::CREATED);
        assert_eq!(goal.plan.len(), 2);
        assert!(goal.plan[1].requires_approval);

        let (st, Json(art)) = create_artifact(
            State(state.clone()),
            Json(CreateArtifactRequest {
                kind: "incident-timeline".into(),
                title: "Timeline".into(),
                body: "# Alerts\n\nNo secrets here.\n".into(),
                content_type: None,
                goal_id: Some(goal.id),
                session_id: None,
                agent: Some("infra-ops".into()),
                metadata: None,
                ttl_seconds: None,
            }),
        )
        .await
        .unwrap();
        assert_eq!(st, StatusCode::CREATED);

        let Json(listed) = list_artifacts(
            State(state.clone()),
            Extension(Principal::Operator),
            Query(ListQuery {
                agent: None,
                goal_id: Some(goal.id),
                session_id: None,
                kind: None,
                limit: None,
                use_case: None,
                since: None,
                user_id: None,
            }),
        )
        .await;
        let items = listed["items"].as_array().unwrap();
        assert_eq!(items.len(), 1);
        assert_eq!(items[0]["id"], json!(art.id));

        let Json(g2) = get_goal(State(state.clone()), Path(goal.id)).await.unwrap();
        assert!(g2.artifact_ids.contains(&art.id));
    }

    /// A goal whose only step needs approval, after the first `advance` to Done opened the approval.
    async fn goal_blocked_on_approval(state: &Arc<AppState>) -> GoalRecord {
        let session_id = Uuid::new_v4();
        // Minimal session so advance can bind approval.
        use crate::model::{SessionRecord, SessionStartMode, SessionStartPolicy, SessionStatus};
        let now = Utc::now();
        state
            .store
            .save_session(SessionRecord {
                id: session_id,
                agent: "infra-ops".into(),
                agent_version: "v".into(),
                sandbox_id: Uuid::new_v4(),
                status: SessionStatus::Running,
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
            })
            .await
            .unwrap();

        let (_, Json(goal)) = create_goal(
            State(state.clone()),
            Json(CreateGoalRequest {
                title: "Remediate".into(),
                description: String::new(),
                agent: "infra-ops".into(),
                user_id: None,
                session_id: Some(session_id),
                plan: vec![CreatePlanStep {
                    title: "Restart".into(),
                    requires_approval: true,
                    approval_reason: None,
                    detail: None,
                    input: None,
                }],
                allow_hosts: vec![],
                autorun: false,
                max_attempts: None,
            }),
        )
        .await
        .unwrap();

        let Json(blocked) = advance_step(
            State(state.clone()),
            Path(goal.id),
            Json(AdvanceStepRequest {
                step_id: "s1".into(),
                status: PlanStepStatus::Done,
                artifact_id: None,
                detail: None,
                approval_prompt: Some("Restart vm-1?".into()),
            }),
        )
        .await
        .unwrap();
        blocked
    }

    fn advance_done(goal: &GoalRecord) -> AdvanceStepRequest {
        AdvanceStepRequest {
            step_id: goal.plan[0].id.clone(),
            status: PlanStepStatus::Done,
            artifact_id: None,
            detail: None,
            approval_prompt: None,
        }
    }

    #[tokio::test]
    async fn requires_approval_step_opens_approval() {
        let state = test_state().await;
        let blocked = goal_blocked_on_approval(&state).await;
        assert_eq!(blocked.status, GoalStatus::Blocked);
        assert_eq!(blocked.plan[0].status, PlanStepStatus::Blocked);
        assert!(blocked.plan[0].approval_id.is_some());
        assert_eq!(state.store.list_approvals().await.len(), 1);
    }

    #[tokio::test]
    async fn done_is_refused_while_the_approval_is_pending() {
        let state = test_state().await;
        let blocked = goal_blocked_on_approval(&state).await;
        let err = advance_step(
            State(state.clone()),
            Path(blocked.id),
            Json(advance_done(&blocked)),
        )
        .await
        .unwrap_err();
        assert_eq!(err.status(), StatusCode::CONFLICT);
        let goal = state.store.get_goal(blocked.id).await.unwrap();
        assert_eq!(goal.plan[0].status, PlanStepStatus::Blocked);
        assert_eq!(goal.status, GoalStatus::Blocked);
    }

    #[tokio::test]
    async fn done_is_refused_after_the_approval_is_denied_and_the_step_stays_blocked() {
        let state = test_state().await;
        let blocked = goal_blocked_on_approval(&state).await;
        let approval_id = blocked.plan[0].approval_id.unwrap();
        state
            .store
            .transition_approval(approval_id, ApprovalStatus::Denied, Some("no".into()), None)
            .await
            .unwrap();
        let err = advance_step(
            State(state.clone()),
            Path(blocked.id),
            Json(advance_done(&blocked)),
        )
        .await
        .unwrap_err();
        assert_eq!(err.status(), StatusCode::CONFLICT);
        let goal = state.store.get_goal(blocked.id).await.unwrap();
        assert_eq!(goal.plan[0].status, PlanStepStatus::Blocked);
        assert_eq!(goal.plan[0].detail.as_deref(), Some("approval denied"));
        assert_eq!(goal.status, GoalStatus::Blocked);
    }

    #[tokio::test]
    async fn done_goes_through_once_the_approval_is_approved() {
        let state = test_state().await;
        let blocked = goal_blocked_on_approval(&state).await;
        let approval_id = blocked.plan[0].approval_id.unwrap();
        state
            .store
            .transition_approval(approval_id, ApprovalStatus::Approved, None, None)
            .await
            .unwrap();
        let Json(done) = advance_step(
            State(state.clone()),
            Path(blocked.id),
            Json(advance_done(&blocked)),
        )
        .await
        .unwrap();
        assert_eq!(done.plan[0].status, PlanStepStatus::Done);
        assert_eq!(done.status, GoalStatus::Done);
    }

    #[tokio::test]
    async fn a_cancelled_goal_cannot_be_advanced() {
        let state = test_state().await;
        let blocked = goal_blocked_on_approval(&state).await;
        let mut goal = state.store.get_goal(blocked.id).await.unwrap();
        goal.status = GoalStatus::Cancelled;
        state.store.save_goal(goal).await.unwrap();
        let err = advance_step(
            State(state.clone()),
            Path(blocked.id),
            Json(advance_done(&blocked)),
        )
        .await
        .unwrap_err();
        assert_eq!(err.status(), StatusCode::CONFLICT);
        assert_eq!(
            state.store.get_goal(blocked.id).await.unwrap().status,
            GoalStatus::Cancelled
        );
    }

    #[tokio::test]
    async fn artifact_rejects_secret_looking_body() {
        let state = test_state().await;
        let err = create_artifact(
            State(state),
            Json(CreateArtifactRequest {
                kind: "report".into(),
                title: "Bad".into(),
                body: "api_key=sk-live-xxx".into(),
                content_type: None,
                goal_id: None,
                session_id: None,
                agent: None,
                metadata: None,
                ttl_seconds: None,
            }),
        )
        .await
        .unwrap_err();
        assert!(err.message().contains("secret"));
    }

    fn artifact_req(
        title: &str,
        body: &str,
        demo: &str,
        ttl: Option<i64>,
    ) -> CreateArtifactRequest {
        CreateArtifactRequest {
            kind: "report".into(),
            title: title.into(),
            body: body.into(),
            content_type: None,
            goal_id: None,
            session_id: None,
            agent: None,
            metadata: Some(json!({ "demo": demo })),
            ttl_seconds: ttl,
        }
    }

    #[tokio::test]
    async fn artifact_ttl_out_of_range_is_refused() {
        let state = test_state().await;
        for bad in [0, -5, 315_360_001] {
            let err = create_artifact(
                State(state.clone()),
                Json(artifact_req("t", "b", "d", Some(bad))),
            )
            .await
            .unwrap_err();
            assert!(err.message().contains("ttl_seconds"), "{bad}");
        }
    }

    #[tokio::test]
    async fn expired_artifact_is_hidden_and_swept() {
        let state = test_state().await;
        let (_, Json(live)) = create_artifact(
            State(state.clone()),
            Json(artifact_req("live", "x", "d", Some(3600))),
        )
        .await
        .unwrap();
        let (_, Json(mut gone)) = create_artifact(
            State(state.clone()),
            Json(artifact_req("gone", "y", "d", None)),
        )
        .await
        .unwrap();
        gone.expires_at = Some(Utc::now() - chrono::Duration::seconds(1));
        state.store.save_artifact(gone.clone()).await.unwrap();

        assert!(state.store.get_artifact(gone.id).await.is_none());
        let ids: Vec<_> = state
            .store
            .list_artifacts()
            .await
            .iter()
            .map(|a| a.id)
            .collect();
        assert_eq!(ids, vec![live.id]);
    }

    #[tokio::test]
    async fn list_filters_by_use_case_and_since() {
        let state = test_state().await;
        let _ = create_artifact(
            State(state.clone()),
            Json(artifact_req("a", "1", "pdf-brief", None)),
        )
        .await
        .unwrap();
        let _ = create_artifact(
            State(state.clone()),
            Json(artifact_req("b", "2", "log-triage", None)),
        )
        .await
        .unwrap();
        let q = |use_case: Option<&str>, since| ListQuery {
            agent: None,
            goal_id: None,
            session_id: None,
            kind: None,
            limit: None,
            use_case: use_case.map(String::from),
            since,
            user_id: None,
        };
        let Json(l) = list_artifacts(
            State(state.clone()),
            Extension(Principal::Operator),
            Query(q(Some("log-triage"), None)),
        )
        .await;
        assert_eq!(l["items"].as_array().unwrap().len(), 1);
        assert_eq!(l["items"][0]["title"], "b");
        let future = Utc::now() + chrono::Duration::seconds(60);
        let Json(l) = list_artifacts(
            State(state),
            Extension(Principal::Operator),
            Query(q(None, Some(future))),
        )
        .await;
        assert!(l["items"].as_array().unwrap().is_empty());
    }

    #[tokio::test]
    async fn diff_endpoint_compares_two_runs() {
        let state = test_state().await;
        let (_, Json(a)) = create_artifact(
            State(state.clone()),
            Json(artifact_req("r1", "a\nb", "d", None)),
        )
        .await
        .unwrap();
        let (_, Json(b)) = create_artifact(
            State(state.clone()),
            Json(artifact_req("r2", "a\nc", "d", None)),
        )
        .await
        .unwrap();
        let Json(d) = diff_artifacts(State(state.clone()), Path((a.id, b.id)))
            .await
            .unwrap();
        assert_eq!(d["summary"]["added"], 1);
        assert_eq!(d["summary"]["removed"], 1);
        assert_eq!(d["summary"]["unchanged"], 1);
        let err = diff_artifacts(State(state), Path((a.id, Uuid::new_v4())))
            .await
            .unwrap_err();
        assert!(err.message().contains("not found"));
    }

    async fn with_agent(state: &Arc<AppState>, name: &str) {
        state
            .store
            .deploy_agent(crate::model::DeployAgentRequest {
                name: name.into(),
                bundle_base64: base64::Engine::encode(
                    &base64::engine::general_purpose::STANDARD,
                    "export default 1",
                ),
                manifest: crate::egress::ask_tests::manifest(
                    crate::model::EgressMode::Deny,
                    Some(30),
                ),
            })
            .await
            .unwrap();
    }

    fn autorun_request(agent: &str, plan: Vec<CreatePlanStep>) -> CreateGoalRequest {
        CreateGoalRequest {
            title: "Trip".into(),
            description: String::new(),
            agent: agent.into(),
            user_id: None,
            session_id: None,
            plan,
            allow_hosts: vec![],
            autorun: true,
            max_attempts: None,
        }
    }

    fn step_of(title: &str) -> CreatePlanStep {
        CreatePlanStep {
            title: title.into(),
            requires_approval: false,
            approval_reason: None,
            detail: None,
            input: None,
        }
    }

    #[tokio::test]
    async fn an_autorun_goal_needs_a_plan_a_deployed_agent_and_sane_limits() {
        let state = test_state().await;
        with_agent(&state, "chat").await;
        let err = create_goal(State(state.clone()), Json(autorun_request("chat", vec![])))
            .await
            .unwrap_err();
        assert_eq!(err.status(), StatusCode::BAD_REQUEST, "no plan");
        let err = create_goal(
            State(state.clone()),
            Json(autorun_request("nobody", vec![step_of("a")])),
        )
        .await
        .unwrap_err();
        assert_eq!(err.status(), StatusCode::NOT_FOUND, "no such agent");
        for bad in [0, 11] {
            let mut r = autorun_request("chat", vec![step_of("a")]);
            r.max_attempts = Some(bad);
            assert_eq!(
                create_goal(State(state.clone()), Json(r))
                    .await
                    .unwrap_err()
                    .status(),
                StatusCode::BAD_REQUEST,
                "max_attempts {bad}"
            );
        }
        let mut big = step_of("a");
        big.input = Some(json!({"x": "y".repeat(17 * 1024)}));
        assert_eq!(
            create_goal(
                State(state.clone()),
                Json(autorun_request("chat", vec![big]))
            )
            .await
            .unwrap_err()
            .status(),
            StatusCode::BAD_REQUEST,
            "a step input over 16 KiB"
        );
        let mut ok = autorun_request(
            "chat",
            vec![
                step_of("Find flights"),
                CreatePlanStep {
                    input: Some(json!({"q": "goa"})),
                    ..step_of("Book")
                },
            ],
        );
        ok.max_attempts = Some(2);
        let (st, Json(goal)) = create_goal(State(state.clone()), Json(ok)).await.unwrap();
        assert_eq!(st, StatusCode::CREATED);
        assert!(goal.autorun && goal.max_attempts == 2);
        assert_eq!(goal.plan[1].input, Some(json!({"q": "goa"})));
        assert_eq!(goal.plan[0].attempts, 0);
        // a goal that does not say autorun is unchanged: no worker touches it
        let mut plain = autorun_request("nobody", vec![]);
        plain.autorun = false;
        let (_, Json(g)) = create_goal(State(state.clone()), Json(plain))
            .await
            .unwrap();
        assert!(!g.autorun && g.max_attempts == 3);
    }

    #[tokio::test]
    async fn an_autorun_goal_is_not_advanced_by_hand_until_it_is_paused() {
        let state = test_state().await;
        with_agent(&state, "chat").await;
        let (_, Json(goal)) = create_goal(
            State(state.clone()),
            Json(autorun_request("chat", vec![step_of("a")])),
        )
        .await
        .unwrap();
        let req = || AdvanceStepRequest {
            step_id: "s1".into(),
            status: PlanStepStatus::Done,
            artifact_id: None,
            detail: None,
            approval_prompt: None,
        };
        let err = advance_step(State(state.clone()), Path(goal.id), Json(req()))
            .await
            .unwrap_err();
        assert_eq!(
            err.status(),
            StatusCode::CONFLICT,
            "the worker owns this goal"
        );
        let patch = PatchGoalRequest {
            status: None,
            session_id: None,
            plan: None,
            allow_hosts: None,
            autorun: Some(false),
            max_attempts: None,
        };
        let Json(paused) = patch_goal(State(state.clone()), Path(goal.id), Json(patch))
            .await
            .unwrap();
        assert!(!paused.autorun);
        let Json(after) = advance_step(State(state.clone()), Path(goal.id), Json(req()))
            .await
            .unwrap();
        assert_eq!(
            after.status,
            GoalStatus::Done,
            "paused, a person can move it"
        );
    }
}
