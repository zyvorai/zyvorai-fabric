// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: Apache-2.0

//! A plan that an agent proposes and the person accepts.
//!
//! A goal can start with no plan. `POST /v1/goals/{id}/plan` runs a **planner agent** (an ordinary deployed agent, usually one with a model socket)
//! as a session for the goal's user, given the goal's title and description. The planner answers by emitting a `goal.plan_proposed` event with
//! its steps. That is all it can do: the steps are stored on the goal as a *proposal*, and **nothing runs until the person accepts them**
//! (`POST /v1/goals/{id}/plan/accept`), or throws them away (`.../plan/reject`). A model can be wrong or be led astray, so:
//!
//! * a proposal is bounded (at most [`MAX_STEPS`] steps, short plain-text titles, a step input of at most [`MAX_INPUT_BYTES`]) and only the
//!   planning session that was started for that goal can make one, once;
//! * a planner that had read untrusted content is marked `tainted` and its plan needs an explicit `confirm_tainted` to accept;
//! * accepting puts the steps on the goal and changes nothing else: each step still runs as an ordinary session with every policy, quota
//!   and approval of one, and running steps automatically is still the goal's own opt-in (`autorun`, which the person may set while accepting);
//! * the journal records that a plan was requested, proposed (how many steps) and accepted or rejected, never the text.

use crate::{
    app::{self, ApiError, ApiResult},
    audit::AuditPhase,
    authz::Principal,
    goals::{GoalRecord, GoalStatus, PlanStep, PlanStepStatus},
    model::{CreateSessionRequest, SessionRecord},
    AppState,
};
use axum::{
    extract::{Path, State},
    http::StatusCode,
    Extension, Json,
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::sync::Arc;
use uuid::Uuid;

/// An optional JSON body: an empty body means "all defaults" whatever the content type says (a client that always sends
/// `content-type: application/json` must not get a 400 for having nothing to say).
pub(crate) fn optional_json<T: serde::de::DeserializeOwned + Default>(
    body: &[u8],
) -> Result<T, ApiError> {
    if body.iter().all(u8::is_ascii_whitespace) {
        return Ok(T::default());
    }
    serde_json::from_slice(body).map_err(|e| {
        ApiError::bad_request(format!("the body is not valid JSON for this request: {e}"))
    })
}

pub const MAX_STEPS: usize = 10;
pub const MAX_TITLE_CHARS: usize = 120;
pub const MAX_INPUT_BYTES: usize = 4096;
pub const MAX_REASON_CHARS: usize = 240;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProposedStep {
    pub title: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub input: Option<Value>,
    /// Ask the person before the step counts as done (see `PlanStep::requires_approval`).
    #[serde(default)]
    pub requires_approval: bool,
    /// Why this step needs a yes, in the planner's own words (shown on the approval, alongside its usual prompt).
    /// Only meaningful with `requires_approval: true` — a bare bool with no explanation was the whole gap this closes.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub approval_reason: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProposedPlan {
    pub steps: Vec<ProposedStep>,
    /// The planning session that proposed it.
    pub session_id: Uuid,
    pub proposed_at: DateTime<Utc>,
    /// The planner had read untrusted content: accepting needs `confirm_tainted`.
    #[serde(default)]
    pub tainted: bool,
}

/// Plain text only: no control, zero-width or direction-changing characters, and at most `max` characters.
fn clean(text: &str, max: usize) -> String {
    text.chars()
        .filter(|c| {
            !(c.is_control()
                || matches!(c, '\u{200B}'..='\u{200F}' | '\u{202A}'..='\u{202E}' | '\u{2060}'..='\u{2064}' | '\u{2066}'..='\u{2069}' | '\u{FEFF}'))
        })
        .take(max)
        .collect::<String>()
        .trim()
        .to_string()
}

/// The steps of a `goal.plan_proposed` event, or why they are not acceptable.
pub fn validate_steps(data: &Value) -> Result<Vec<ProposedStep>, String> {
    let Some(list) = data.get("steps").and_then(Value::as_array) else {
        return Err("the proposal has no steps list".into());
    };
    if list.is_empty() || list.len() > MAX_STEPS {
        return Err(format!("a plan has 1 to {MAX_STEPS} steps"));
    }
    let mut out = Vec::new();
    for (i, s) in list.iter().enumerate() {
        let title = clean(
            s.get("title").and_then(Value::as_str).unwrap_or_default(),
            MAX_TITLE_CHARS,
        );
        if title.is_empty() {
            return Err(format!("step {} has no title", i + 1));
        }
        let input = match s.get("input") {
            None | Some(Value::Null) => None,
            Some(v @ Value::Object(_)) => {
                if v.to_string().len() > MAX_INPUT_BYTES {
                    return Err(format!(
                        "the input of step {} is over {MAX_INPUT_BYTES} bytes",
                        i + 1
                    ));
                }
                Some(v.clone())
            }
            Some(_) => return Err(format!("the input of step {} must be an object", i + 1)),
        };
        let requires_approval = match s.get("requires_approval") {
            None | Some(Value::Null) => false,
            Some(Value::Bool(b)) => *b,
            Some(_) => {
                return Err(format!(
                    "requires_approval of step {} must be true or false",
                    i + 1
                ))
            }
        };
        let approval_reason = match s.get("approval_reason") {
            None | Some(Value::Null) => None,
            Some(Value::String(text)) => {
                if !requires_approval {
                    return Err(format!(
                        "step {} has an approval_reason but requires_approval is not true",
                        i + 1
                    ));
                }
                let cleaned = clean(text, MAX_REASON_CHARS);
                if cleaned.is_empty() {
                    return Err(format!("approval_reason of step {} is empty", i + 1));
                }
                Some(cleaned)
            }
            Some(_) => {
                return Err(format!(
                    "approval_reason of step {} must be a string",
                    i + 1
                ))
            }
        };
        out.push(ProposedStep {
            title,
            input,
            requires_approval,
            approval_reason,
        });
    }
    Ok(out)
}

#[derive(Debug, Default, Deserialize)]
pub struct StartPlanning {
    /// The planner agent to use; the host's `ZYVOR_AGENT_PLANNER_AGENT` when omitted.
    #[serde(default)]
    pub planner: Option<String>,
}

/// `POST /v1/goals/{id}/plan`: ask the planner agent to propose steps. Only for a goal with no plan yet.
pub(crate) async fn start_planning(
    State(state): State<Arc<AppState>>,
    Path(id): Path<Uuid>,
    body: axum::body::Bytes,
) -> ApiResult<(StatusCode, Json<Value>)> {
    let body: StartPlanning = optional_json(&body)?;
    let mut goal = state
        .store
        .get_goal(id)
        .await
        .ok_or_else(|| ApiError::not_found("goal not found"))?;
    if matches!(goal.status, GoalStatus::Done | GoalStatus::Cancelled) {
        return Err(ApiError::conflict(
            "a finished or cancelled goal cannot be planned",
        ));
    }
    if !goal.plan.is_empty() {
        return Err(ApiError::conflict("this goal already has a plan"));
    }
    if let Some(sid) = goal.planning_session_id {
        if state
            .store
            .get_session(sid)
            .await
            .is_some_and(|s| !s.status.is_terminal())
        {
            return Err(ApiError::conflict(
                "a planner is already working on this goal",
            ));
        }
    }
    let planner = body
        .planner
        .or_else(|| state.config.planner_agent.clone())
        .ok_or_else(|| {
            ApiError::bad_request(
                "no planner agent: name one, or set ZYVOR_AGENT_PLANNER_AGENT on the host",
            )
        })?;
    if state.store.get_agent(&planner).await.is_none() {
        return Err(ApiError::not_found("the planner agent is not deployed"));
    }
    if let Some(user) = goal.user_id.as_deref() {
        crate::usage::check_run_quota(&state, user, crate::usage::Limits::from_env()).await?;
    }
    let req = CreateSessionRequest {
        agent: planner,
        input: json!({
            "purpose": "plan",
            "max_steps": MAX_STEPS,
            "goal": {"id": goal.id, "title": goal.title, "description": goal.description, "agent": goal.agent},
        }),
        ttl_seconds: None,
        request_id: Some(format!("goal-plan:{}:{}", goal.id, Uuid::new_v4())),
        start_policy: Default::default(),
        parent_session_id: None,
        user_id: goal.user_id.clone(),
    };
    let (_, Json(view)) = app::create_session(State(state.clone()), Json(req)).await?;
    // an earlier proposal stays until the new one arrives (or the person decides on it)
    goal.planning_session_id = Some(view.id);
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
            Some(view.id),
            AuditPhase::Planned,
            "merion.goal.plan_requested",
            Some(goal.agent.clone()),
            json!({"goal_id": goal.id}),
        )
        .await;
    Ok((
        StatusCode::ACCEPTED,
        Json(json!({"goal_id": goal.id, "session_id": view.id})),
    ))
}

/// A planner's `goal.plan_proposed` event. Stored as a proposal on the goal the session was started for, or a `goal.plan_refused` event
/// saying why not (never the text).
pub async fn record_proposal(state: &AppState, session: &SessionRecord, data: &Value) {
    let refuse = |reason: String| async move {
        let _ = state
            .store
            .append_event(session.id, "goal.plan_refused", json!({"reason": reason}))
            .await;
    };
    let goal = state
        .store
        .list_goals()
        .await
        .into_iter()
        .find(|g| g.planning_session_id == Some(session.id));
    let Some(mut goal) = goal else {
        return refuse(
            "this session was not started to plan a goal, or it already proposed a plan".into(),
        )
        .await;
    };
    if !goal.plan.is_empty() {
        goal.planning_session_id = None;
        let _ = state.store.save_goal(goal).await;
        return refuse("the goal already has a plan".into()).await;
    }
    let steps = match validate_steps(data) {
        Ok(s) => s,
        Err(reason) => return refuse(reason).await,
    };
    let n = steps.len();
    let tainted = !session.tainted_by.is_empty();
    goal.proposed_plan = Some(ProposedPlan {
        steps,
        session_id: session.id,
        proposed_at: Utc::now(),
        tainted,
    });
    goal.planning_session_id = None; // one proposal per planning session
    goal.updated_at = Utc::now();
    if let Err(e) = state.store.save_goal(goal.clone()).await {
        return refuse(format!("could not store the proposal: {e}")).await;
    }
    let _ = state
        .store
        .audit
        .append(
            Some(session.id),
            AuditPhase::Performed,
            "merion.goal.plan_proposed",
            Some(session.agent.clone()),
            json!({"goal_id": goal.id, "steps": n, "tainted": tainted}),
        )
        .await;
}

#[derive(Debug, Default, Deserialize)]
pub struct AcceptPlan {
    /// Needed when the proposal is `tainted`.
    #[serde(default)]
    pub confirm_tainted: bool,
    /// Also let the goal worker run the accepted steps (the goal's own opt-in).
    #[serde(default)]
    pub autorun: Option<bool>,
}

/// `POST /v1/goals/{id}/plan/accept`: the proposal becomes the goal's plan.
pub(crate) async fn accept_plan(
    State(state): State<Arc<AppState>>,
    Extension(principal): Extension<Principal>,
    Path(id): Path<Uuid>,
    body: axum::body::Bytes,
) -> ApiResult<Json<GoalRecord>> {
    let req: AcceptPlan = optional_json(&body)?;
    let mut goal = state
        .store
        .get_goal(id)
        .await
        .ok_or_else(|| ApiError::not_found("goal not found"))?;
    let Some(proposal) = goal.proposed_plan.clone() else {
        return Err(ApiError::conflict("there is no proposed plan to accept"));
    };
    if !goal.plan.is_empty() || matches!(goal.status, GoalStatus::Done | GoalStatus::Cancelled) {
        return Err(ApiError::conflict("this goal cannot take a new plan"));
    }
    if proposal.tainted && !req.confirm_tainted {
        return Err(ApiError::conflict(
            "the planner had read untrusted content; read the steps and accept with confirm_tainted: true",
        ));
    }
    if req.autorun == Some(true) {
        if state.store.get_agent(goal.agent.trim()).await.is_none() {
            return Err(ApiError::not_found(
                "running automatically needs the goal's agent to be deployed",
            ));
        }
        if let Some(user) = principal.user() {
            if !goal.autorun
                && crate::goals::active_autorun(&state, user).await
                    >= crate::goals::USER_MAX_ACTIVE_AUTORUN
            {
                return Err(ApiError::too_many(format!(
                    "at most {} goals may run automatically at once",
                    crate::goals::USER_MAX_ACTIVE_AUTORUN
                )));
            }
        }
        goal.autorun = true;
    }
    goal.plan = proposal
        .steps
        .iter()
        .enumerate()
        .map(|(i, s)| PlanStep {
            id: format!("s{}", i + 1),
            title: s.title.clone(),
            status: PlanStepStatus::Pending,
            requires_approval: s.requires_approval,
            approval_reason: s.approval_reason.clone(),
            input: s.input.clone(),
            ..Default::default()
        })
        .collect();
    goal.proposed_plan = None;
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
            None,
            AuditPhase::Performed,
            "merion.goal.plan_accepted",
            Some(goal.agent.clone()),
            json!({"goal_id": goal.id, "steps": goal.plan.len(), "autorun": goal.autorun}),
        )
        .await;
    Ok(Json(goal))
}

/// `POST /v1/goals/{id}/plan/reject`: throw the proposal away.
pub(crate) async fn reject_plan(
    State(state): State<Arc<AppState>>,
    Path(id): Path<Uuid>,
) -> ApiResult<Json<GoalRecord>> {
    let mut goal = state
        .store
        .get_goal(id)
        .await
        .ok_or_else(|| ApiError::not_found("goal not found"))?;
    if goal.proposed_plan.is_none() {
        return Err(ApiError::conflict("there is no proposed plan to reject"));
    }
    goal.proposed_plan = None;
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
            None,
            AuditPhase::Performed,
            "merion.goal.plan_rejected",
            Some(goal.agent.clone()),
            json!({"goal_id": goal.id}),
        )
        .await;
    Ok(Json(goal))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_proposal_is_bounded_plain_text() {
        let ok = validate_steps(&json!({"steps": [
            {"title": "  Find the dates \u{202e}  ", "input": {"message": "dates?"}},
            {"title": "Book it", "requires_approval": true}
        ]}))
        .unwrap();
        assert_eq!(ok[0].title, "Find the dates");
        assert_eq!(ok[0].input, Some(json!({"message": "dates?"})));
        assert!(ok[1].requires_approval && ok[1].input.is_none());
        for (why, bad) in [
            ("no list", json!({})),
            ("empty", json!({"steps": []})),
            (
                "too many",
                json!({"steps": (0..=MAX_STEPS).map(|i| json!({"title": format!("s{i}")})).collect::<Vec<_>>()}),
            ),
            ("no title", json!({"steps": [{"title": "  \u{200b} "}]})),
            (
                "input not an object",
                json!({"steps": [{"title": "a", "input": "x"}]}),
            ),
            (
                "input too big",
                json!({"steps": [{"title": "a", "input": {"m": "x".repeat(MAX_INPUT_BYTES)}}]}),
            ),
            (
                "approval flag not a bool",
                json!({"steps": [{"title": "a", "requires_approval": "yes"}]}),
            ),
        ] {
            assert!(validate_steps(&bad).is_err(), "{why}");
        }
        let long = validate_steps(&json!({"steps": [{"title": "x".repeat(500)}]})).unwrap();
        assert_eq!(long[0].title.chars().count(), MAX_TITLE_CHARS);
    }

    #[test]
    fn an_approval_reason_needs_the_approval_flag_and_is_bounded_plain_text() {
        let ok = validate_steps(&json!({"steps": [
            {"title": "Book the flight", "requires_approval": true, "approval_reason": "  it costs money \u{202e} "}
        ]}))
        .unwrap();
        assert_eq!(ok[0].approval_reason.as_deref(), Some("it costs money"));

        let none = validate_steps(&json!({"steps": [{"title": "a"}]})).unwrap();
        assert_eq!(
            none[0].approval_reason, None,
            "no reason given, none stored"
        );

        for (why, bad) in [
            (
                "reason without the approval flag",
                json!({"steps": [{"title": "a", "approval_reason": "because"}]}),
            ),
            (
                "reason not a string",
                json!({"steps": [{"title": "a", "requires_approval": true, "approval_reason": 1}]}),
            ),
            (
                "reason empty after cleaning",
                json!({"steps": [{"title": "a", "requires_approval": true, "approval_reason": "\u{200b}"}]}),
            ),
        ] {
            assert!(validate_steps(&bad).is_err(), "{why}");
        }
        let long = validate_steps(&json!({"steps": [
            {"title": "a", "requires_approval": true, "approval_reason": "x".repeat(500)}
        ]}))
        .unwrap();
        assert_eq!(
            long[0].approval_reason.as_ref().unwrap().chars().count(),
            MAX_REASON_CHARS
        );
    }
}
