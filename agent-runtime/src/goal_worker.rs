// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: Apache-2.0

//! The goal worker: runs an **opted-in** goal's plan one step at a time, so a goal can finish without someone calling `advance` by hand.
//!
//! A goal is only ever run automatically when it says so (`autorun: true`); nothing else changes for goals that do not. For such a goal the
//! worker looks at the first step that is not done or skipped and does exactly one thing:
//!
//! * **Pending**: start a session of the goal's agent for that step (its own `input`, or the goal and step titles). The session request id is
//!   `goal:<goal>:<step>:<attempt>`, so a restart in the middle of starting one finds the session it already made instead of starting a second.
//! * **Running**: watch that session. It completed: the step is done, or, for a step marked `requires_approval`, an approval is opened for the
//!   person (bound to the step's session) and the step waits. It failed, was cancelled or expired: try again after a growing delay, up to
//!   `max_attempts` (default 3), then block the step and the goal and say why.
//! * **Waiting for approval**: approved: the step is done. Denied or expired: the step and goal stay blocked (the worker never moves past a
//!   refusal). Still pending: nothing.
//!
//! What it does not do: it never widens a session's authority (a step's session is an ordinary session, with every policy, quota and approval
//! of one started by hand), never approves anything, never runs two steps of a goal at once, and never retries a step it has blocked. A step
//! that must not run twice should not be marked retryable work: put the irreversible action behind an approval at the egress broker
//! (credential `requires_approval`), which is where a real send or purchase is decided. Goals with `autorun` are not advanced by hand
//! (`advance` is refused), so two hands never move one goal.

use crate::{
    app::{self, ApiError},
    audit::AuditPhase,
    goals::{BlockedOn, GoalRecord, GoalStatus, PlanStep, PlanStepStatus},
    model::{ApprovalKind, ApprovalRecord, ApprovalStatus, CreateSessionRequest, SessionStatus},
    AppState,
};
use axum::{extract::State, Json};
use chrono::{DateTime, Duration as Days, Utc};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{sync::Arc, time::Duration};
use uuid::Uuid;

/// What the worker knows about the step it is looking at, read from the store before deciding.
#[derive(Debug, Clone, Default)]
pub struct StepView {
    /// The step's session: its status and error, if it still exists.
    pub session: Option<(SessionStatus, Option<String>)>,
    /// The status of the step's approval, if it has one.
    pub approval: Option<ApprovalStatus>,
}

/// The one thing to do for a step.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Move {
    Nothing,
    /// Start a session for attempt number `attempt` (1-based).
    Start {
        attempt: u32,
    },
    /// The step's session finished; the step is done.
    Done,
    /// The step's session finished and the step needs the person's approval: open one.
    OpenApproval,
    /// Try again at `at`, remembering why the last attempt ended.
    Retry {
        error: String,
        at: DateTime<Utc>,
    },
    /// Stop on this step, and say why.
    Block {
        reason: String,
        on: BlockedOn,
    },
}

/// The index of the step the worker is on: the first that is not done or skipped.
pub fn current_step(goal: &GoalRecord) -> Option<usize> {
    goal.plan
        .iter()
        .position(|s| !matches!(s.status, PlanStepStatus::Done | PlanStepStatus::Skipped))
}

/// Seconds to wait before attempt `attempt + 1`: `base` doubled for each earlier failure, at most ten minutes.
pub fn backoff_secs(base: u64, attempts_so_far: u32) -> u64 {
    let doubled = base.saturating_mul(1u64 << attempts_so_far.saturating_sub(1).min(20));
    doubled.min(600)
}

/// The decision for one step. Pure: it reads nothing and changes nothing, so every path can be tested.
pub fn next_move(
    step: &PlanStep,
    max_attempts: u32,
    retry_base_secs: u64,
    now: DateTime<Utc>,
    view: &StepView,
) -> Move {
    match step.status {
        PlanStepStatus::Done | PlanStepStatus::Skipped => Move::Nothing,
        PlanStepStatus::Pending => match step.next_attempt_at {
            Some(at) if at > now => Move::Nothing,
            _ => Move::Start {
                attempt: step.attempts + 1,
            },
        },
        PlanStepStatus::Running => match &view.session {
            Some((SessionStatus::Completed, _))
                if step.requires_approval && step.approval_id.is_none() =>
            {
                Move::OpenApproval
            }
            Some((SessionStatus::Completed, _)) => Move::Done,
            Some((status, error)) if status.is_terminal() => {
                let why = error.clone().unwrap_or_else(|| {
                    format!(
                        "the session ended: {}",
                        format!("{status:?}").to_lowercase()
                    )
                });
                fail_or_retry(step, max_attempts, retry_base_secs, now, why)
            }
            Some(_) => Move::Nothing,
            None => fail_or_retry(
                step,
                max_attempts,
                retry_base_secs,
                now,
                "the step's session no longer exists".into(),
            ),
        },
        PlanStepStatus::Blocked => match (step.blocked_on, step.approval_id, &view.approval) {
            (Some(BlockedOn::Approval), Some(_), Some(ApprovalStatus::Approved)) => Move::Done,
            (Some(BlockedOn::Approval), Some(_), Some(ApprovalStatus::Denied)) => block_once(
                step,
                "the approval was denied; the goal stays blocked",
                BlockedOn::Rejected,
            ),
            (Some(BlockedOn::Approval), Some(_), Some(ApprovalStatus::Expired) | None) => {
                block_once(
                    step,
                    "the approval expired or is missing; the goal stays blocked",
                    BlockedOn::Rejected,
                )
            }
            // waiting for a decision, or blocked for a reason the worker never retries
            _ => Move::Nothing,
        },
    }
}

fn fail_or_retry(
    step: &PlanStep,
    max_attempts: u32,
    base: u64,
    now: DateTime<Utc>,
    why: String,
) -> Move {
    if step.attempts < max_attempts {
        Move::Retry {
            error: why,
            at: now + Days::seconds(backoff_secs(base, step.attempts) as i64),
        }
    } else {
        Move::Block {
            reason: format!(
                "failed after {} attempt{}: {why}",
                step.attempts,
                if step.attempts == 1 { "" } else { "s" }
            ),
            on: BlockedOn::Failure,
        }
    }
}

/// A block that is only written once (not again on every tick).
fn block_once(step: &PlanStep, reason: &str, on: BlockedOn) -> Move {
    if step.detail.as_deref() == Some(reason) && step.blocked_on == Some(on) {
        Move::Nothing
    } else {
        Move::Block {
            reason: reason.into(),
            on,
        }
    }
}

/// What a step's agent is given when the plan does not say.
pub fn default_input(goal: &GoalRecord, step: &PlanStep) -> Value {
    json!({
        "message": step.title,
        "goal": { "id": goal.id, "title": goal.title, "description": goal.description },
        "step": { "id": step.id, "title": step.title, "detail": step.detail },
    })
}

fn sha256_hex(v: &Value) -> String {
    let mut h = Sha256::new();
    h.update(v.to_string().as_bytes());
    h.finalize().iter().map(|b| format!("{b:02x}")).collect()
}

/// Recomputes the goal's status from its steps (the same rule as `advance`), leaving a cancelled goal cancelled.
fn refresh_status(goal: &mut GoalRecord) {
    if goal.status == GoalStatus::Cancelled {
        return;
    }
    goal.status = if goal
        .plan
        .iter()
        .all(|s| matches!(s.status, PlanStepStatus::Done | PlanStepStatus::Skipped))
    {
        GoalStatus::Done
    } else if goal
        .plan
        .iter()
        .any(|s| matches!(s.status, PlanStepStatus::Blocked))
    {
        GoalStatus::Blocked
    } else {
        GoalStatus::Open
    };
    goal.updated_at = Utc::now();
}

async fn journal(
    state: &AppState,
    session: Option<Uuid>,
    action: &str,
    goal: &GoalRecord,
    data: Value,
) {
    let mut data = data;
    data["goal_id"] = json!(goal.id);
    let _ = state
        .store
        .audit
        .append(
            session,
            AuditPhase::Performed,
            action,
            Some(goal.agent.clone()),
            data,
        )
        .await;
}

/// Looks at one autorun goal and does at most one thing. Returns what it did (for tests and logs).
pub async fn tick_goal(state: &Arc<AppState>, goal_id: Uuid, now: DateTime<Utc>) -> Move {
    let Some(goal) = state.store.get_goal(goal_id).await else {
        return Move::Nothing;
    };
    if !goal.autorun || matches!(goal.status, GoalStatus::Done | GoalStatus::Cancelled) {
        // a cancelled goal stops its running step's session, once
        if goal.status == GoalStatus::Cancelled {
            stop_running_session(state, &goal).await;
        }
        return Move::Nothing;
    }
    let Some(idx) = current_step(&goal) else {
        // no steps left: the goal is done (a goal with an empty plan has nothing to run)
        return Move::Nothing;
    };
    let step = goal.plan[idx].clone();
    let view = StepView {
        session: match step.session_id {
            Some(sid) => state
                .store
                .get_session(sid)
                .await
                .map(|s| (s.status, s.error)),
            None => None,
        },
        approval: match step.approval_id {
            Some(aid) => state.store.get_approval(aid).await.map(|a| a.status),
            None => None,
        },
    };
    let mv = next_move(
        &step,
        goal.max_attempts,
        state.config.goal_retry_base_secs,
        now,
        &view,
    );
    apply(state, &goal, idx, &step, &mv, now).await;
    mv
}

async fn stop_running_session(state: &Arc<AppState>, goal: &GoalRecord) {
    for step in goal
        .plan
        .iter()
        .filter(|s| s.status == PlanStepStatus::Running)
    {
        if let Some(sid) = step.session_id {
            if state
                .store
                .get_session(sid)
                .await
                .is_some_and(|s| !s.status.is_terminal())
            {
                let _ = app::cancel_session(State(state.clone()), axum::extract::Path(sid)).await;
                journal(
                    state,
                    Some(sid),
                    "merion.goal.step.cancelled",
                    goal,
                    json!({ "step_id": step.id }),
                )
                .await;
            }
        }
    }
}

/// Applies a move to the stored goal, re-reading it first and giving up if it changed underneath (someone cancelled it, say).
async fn apply(
    state: &Arc<AppState>,
    goal: &GoalRecord,
    idx: usize,
    seen: &PlanStep,
    mv: &Move,
    now: DateTime<Utc>,
) {
    let step_id = seen.id.clone();
    // starting a session happens before the goal is saved, so the session exists when the goal says it does
    let started: Option<Result<Uuid, String>> = match mv {
        Move::Start { attempt } => Some(start_session(state, goal, seen, *attempt).await),
        _ => None,
    };
    let opened: Option<Result<Uuid, String>> = match mv {
        Move::OpenApproval => Some(open_approval(state, goal, seen).await),
        _ => None,
    };
    let Some(mut fresh) = state.store.get_goal(goal.id).await else {
        return;
    };
    if fresh.status == GoalStatus::Cancelled || !fresh.autorun {
        return;
    }
    let Some(step) = fresh.plan.iter_mut().find(|s| s.id == step_id) else {
        return;
    };
    if step.status != seen.status || step.attempts != seen.attempts {
        return; // the step moved while this tick was working; the next tick sees it
    }
    match mv {
        Move::Nothing => return,
        Move::Start { attempt } => match started {
            Some(Ok(sid)) => {
                step.status = PlanStepStatus::Running;
                step.session_id = Some(sid);
                step.attempts = *attempt;
                step.next_attempt_at = None;
                step.detail = Some(format!("attempt {attempt} running"));
                step.blocked_on = None;
                journal(
                    state,
                    Some(sid),
                    "merion.goal.step.started",
                    goal,
                    json!({ "step_id": step_id, "attempt": attempt }),
                )
                .await;
            }
            Some(Err(why)) => {
                // could not start (a quota, say): not an attempt; look again soon
                step.last_error = Some(why.clone());
                step.next_attempt_at =
                    Some(now + Days::seconds(state.config.goal_retry_base_secs.max(1) as i64 * 4));
                step.detail = Some(format!("waiting to start: {why}"));
            }
            None => return,
        },
        Move::Done => {
            step.status = PlanStepStatus::Done;
            step.detail = Some("done".into());
            step.blocked_on = None;
            journal(
                state,
                step.session_id,
                "merion.goal.step.done",
                goal,
                json!({ "step_id": step_id }),
            )
            .await;
        }
        Move::OpenApproval => match opened {
            Some(Ok(aid)) => {
                step.approval_id = Some(aid);
                step.status = PlanStepStatus::Blocked;
                step.blocked_on = Some(BlockedOn::Approval);
                step.detail = Some("waiting for your approval".into());
                journal(
                    state,
                    step.session_id,
                    "merion.goal.step.approval",
                    goal,
                    json!({ "step_id": step_id, "approval_id": aid }),
                )
                .await;
            }
            _ => return, // could not open it; try again next tick
        },
        Move::Retry { error, at } => {
            step.status = PlanStepStatus::Pending;
            step.next_attempt_at = Some(*at);
            step.last_error = Some(error.clone());
            step.detail = Some(format!("attempt {} failed; retrying", step.attempts));
            journal(
                state,
                step.session_id,
                "merion.goal.step.retry",
                goal,
                json!({ "step_id": step_id, "attempt": step.attempts }),
            )
            .await;
        }
        Move::Block { reason, on } => {
            step.status = PlanStepStatus::Blocked;
            step.blocked_on = Some(*on);
            step.detail = Some(reason.clone());
            journal(
                state,
                step.session_id,
                "merion.goal.step.blocked",
                goal,
                json!({ "step_id": step_id, "reason": reason }),
            )
            .await;
        }
    }
    let _ = idx;
    refresh_status(&mut fresh);
    let (now_status, user) = (fresh.status.clone(), fresh.user_id.clone());
    let saved = state.store.save_goal(fresh.clone()).await.is_ok();
    // tell the person when a goal stops on a problem or finishes (an approval has its own signed message)
    if let (true, Some(user)) = (saved, user) {
        let blocked_now = matches!(mv, Move::Block { .. }) && now_status == GoalStatus::Blocked;
        let finished_now = now_status == GoalStatus::Done && goal.status != GoalStatus::Done;
        if blocked_now || finished_now {
            let reason = match mv {
                Move::Block { reason, .. } => reason.clone(),
                _ => "all steps are done".to_string(),
            };
            crate::notify::notify_user(
                state,
                &user,
                crate::notify::Notice {
                    event: if blocked_now {
                        "goal.blocked"
                    } else {
                        "goal.done"
                    },
                    title: if blocked_now {
                        "A goal needs your attention".into()
                    } else {
                        "A goal is done".into()
                    },
                    body: "Open Keep to see what happened.".into(),
                    detail_title: fresh.title.clone(),
                    detail_body: reason,
                    data: json!({ "goal_id": fresh.id, "step_id": step_id }),
                },
            );
        }
    }
}

async fn start_session(
    state: &Arc<AppState>,
    goal: &GoalRecord,
    step: &PlanStep,
    attempt: u32,
) -> Result<Uuid, String> {
    if let Some(user) = goal.user_id.as_deref() {
        crate::usage::check_run_quota(state, user, crate::usage::Limits::from_env())
            .await
            .map_err(|e: ApiError| e.message().to_string())?;
    }
    let input = step
        .input
        .clone()
        .unwrap_or_else(|| default_input(goal, step));
    let req = CreateSessionRequest {
        agent: goal.agent.clone(),
        input,
        ttl_seconds: None,
        request_id: Some(format!("goal:{}:{}:{attempt}", goal.id, step.id)),
        start_policy: Default::default(),
        parent_session_id: None,
        user_id: goal.user_id.clone(),
    };
    match app::create_session(State(state.clone()), Json(req)).await {
        Ok((_, Json(view))) => Ok(view.id),
        Err(e) => Err(e.message().to_string()),
    }
}

async fn open_approval(
    state: &Arc<AppState>,
    goal: &GoalRecord,
    step: &PlanStep,
) -> Result<Uuid, String> {
    let session_id = step
        .session_id
        .ok_or("the step has no session to attach the approval to")?;
    let input = step
        .input
        .clone()
        .unwrap_or_else(|| default_input(goal, step));
    let approval = ApprovalRecord {
        id: Uuid::new_v4(),
        session_id,
        kind: ApprovalKind::Send,
        subject: Some(format!("goal:{}:{}", goal.id, step.id)),
        planned_action: Some(json!({
            "goal_id": goal.id, "step_id": step.id, "title": step.title, "agent": goal.agent, "input_sha256": sha256_hex(&input),
            "approval_reason": step.approval_reason,
        })),
        prompt: match &step.approval_reason {
            Some(reason) => format!(
                "Goal '{}': step '{}' finished. Continue? The agent said: {}",
                goal.title, step.title, reason
            ),
            None => format!(
                "Goal '{}': step '{}' finished. Continue?",
                goal.title, step.title
            ),
        },
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
        .map_err(|e| e.to_string())?;
    crate::notify::approval_requested(state, &approval);
    Ok(approval.id)
}

/// The loop: every tick, look at each autorun goal.
pub async fn goal_loop(state: Arc<AppState>) {
    let tick = Duration::from_millis(state.config.goal_tick_ms.max(200));
    loop {
        tokio::time::sleep(tick).await;
        for goal in state.store.list_goals().await {
            if goal.autorun && !matches!(goal.status, GoalStatus::Done | GoalStatus::Cancelled) {
                tick_goal(&state, goal.id, Utc::now()).await;
            } else if goal.autorun && goal.status == GoalStatus::Cancelled {
                stop_running_session(&state, &goal).await;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::SessionStatus as S;

    fn step(status: PlanStepStatus) -> PlanStep {
        PlanStep {
            id: "s1".into(),
            title: "t".into(),
            status,
            ..Default::default()
        }
    }
    fn now() -> DateTime<Utc> {
        Utc::now()
    }
    fn session(status: S) -> StepView {
        StepView {
            session: Some((status, None)),
            approval: None,
        }
    }

    #[test]
    fn a_pending_step_starts_and_waits_out_its_retry_delay() {
        let n = now();
        assert_eq!(
            next_move(
                &step(PlanStepStatus::Pending),
                3,
                15,
                n,
                &StepView::default()
            ),
            Move::Start { attempt: 1 }
        );
        let mut retry = step(PlanStepStatus::Pending);
        retry.attempts = 1;
        retry.next_attempt_at = Some(n + Days::seconds(30));
        assert_eq!(
            next_move(&retry, 3, 15, n, &StepView::default()),
            Move::Nothing,
            "not yet"
        );
        assert_eq!(
            next_move(&retry, 3, 15, n + Days::seconds(31), &StepView::default()),
            Move::Start { attempt: 2 }
        );
    }

    #[test]
    fn a_running_step_waits_while_its_session_runs_and_finishes_when_it_completed() {
        let n = now();
        let s = step(PlanStepStatus::Running);
        for live in [S::Creating, S::Running, S::Hibernating, S::Hibernated] {
            assert_eq!(next_move(&s, 3, 15, n, &session(live)), Move::Nothing);
        }
        assert_eq!(next_move(&s, 3, 15, n, &session(S::Completed)), Move::Done);
    }

    #[test]
    fn a_step_that_needs_approval_opens_one_after_its_session_completed_and_only_once() {
        let n = now();
        let mut s = step(PlanStepStatus::Running);
        s.requires_approval = true;
        assert_eq!(
            next_move(&s, 3, 15, n, &session(S::Completed)),
            Move::OpenApproval
        );
        // once it is open the step is blocked on it; the worker only reads the decision
        s.status = PlanStepStatus::Blocked;
        s.blocked_on = Some(BlockedOn::Approval);
        s.approval_id = Some(Uuid::new_v4());
        let view = |a| StepView {
            session: None,
            approval: a,
        };
        assert_eq!(
            next_move(&s, 3, 15, n, &view(Some(ApprovalStatus::Pending))),
            Move::Nothing
        );
        assert_eq!(
            next_move(&s, 3, 15, n, &view(Some(ApprovalStatus::Approved))),
            Move::Done
        );
    }

    #[test]
    fn a_refused_approval_blocks_once_and_the_worker_never_goes_past_it() {
        let n = now();
        let mut s = step(PlanStepStatus::Blocked);
        s.blocked_on = Some(BlockedOn::Approval);
        s.approval_id = Some(Uuid::new_v4());
        for refused in [
            Some(ApprovalStatus::Denied),
            Some(ApprovalStatus::Expired),
            None,
        ] {
            let view = StepView {
                session: None,
                approval: refused,
            };
            let Move::Block { on, .. } = next_move(&s, 3, 15, n, &view) else {
                panic!("expected a block")
            };
            assert_eq!(on, BlockedOn::Rejected);
        }
        // once written, the same block is not written again every tick
        let Move::Block { reason, on } = next_move(
            &s,
            3,
            15,
            n,
            &StepView {
                session: None,
                approval: Some(ApprovalStatus::Denied),
            },
        ) else {
            panic!()
        };
        s.detail = Some(reason);
        s.blocked_on = Some(on);
        assert_eq!(
            next_move(
                &s,
                3,
                15,
                n,
                &StepView {
                    session: None,
                    approval: Some(ApprovalStatus::Denied)
                }
            ),
            Move::Nothing
        );
    }

    #[test]
    fn a_failed_session_is_retried_with_growing_delays_then_blocks_with_the_reason() {
        let n = now();
        let failed = |err: &str| StepView {
            session: Some((S::Failed, Some(err.into()))),
            approval: None,
        };
        let mut s = step(PlanStepStatus::Running);
        s.attempts = 1;
        let Move::Retry { error, at } = next_move(&s, 3, 15, n, &failed("boom")) else {
            panic!()
        };
        assert_eq!(error, "boom");
        assert_eq!((at - n).num_seconds(), 15);
        s.attempts = 2;
        let Move::Retry { at, .. } = next_move(&s, 3, 15, n, &failed("boom")) else {
            panic!()
        };
        assert_eq!((at - n).num_seconds(), 30, "the delay doubles");
        s.attempts = 3;
        let Move::Block { reason, on } = next_move(&s, 3, 15, n, &failed("boom")) else {
            panic!()
        };
        assert_eq!(on, BlockedOn::Failure);
        assert!(
            reason.contains("failed after 3 attempts") && reason.contains("boom"),
            "{reason}"
        );
        // cancelled and expired sessions count as failures; a vanished session too
        for st in [S::Cancelled, S::Expired] {
            s.attempts = 1;
            assert!(
                matches!(next_move(&s, 3, 15, n, &session(st)), Move::Retry { .. }),
                "{st:?}"
            );
        }
        assert!(matches!(
            next_move(&s, 3, 15, n, &StepView::default()),
            Move::Retry { .. }
        ));
        // max_attempts of 1 never retries
        s.attempts = 1;
        assert!(matches!(
            next_move(&s, 1, 15, n, &failed("x")),
            Move::Block { .. }
        ));
    }

    #[test]
    fn a_blocked_step_that_failed_is_never_retried_by_the_worker() {
        let mut s = step(PlanStepStatus::Blocked);
        s.blocked_on = Some(BlockedOn::Failure);
        assert_eq!(
            next_move(&s, 3, 15, now(), &StepView::default()),
            Move::Nothing
        );
        s.blocked_on = None; // blocked by hand
        assert_eq!(
            next_move(&s, 3, 15, now(), &StepView::default()),
            Move::Nothing
        );
    }

    #[test]
    fn the_worker_is_on_the_first_step_that_is_not_finished() {
        let mut g = crate::goals::test_goal(vec![
            step(PlanStepStatus::Done),
            step(PlanStepStatus::Skipped),
            step(PlanStepStatus::Pending),
            step(PlanStepStatus::Pending),
        ]);
        assert_eq!(current_step(&g), Some(2));
        g.plan[2].status = PlanStepStatus::Done;
        g.plan[3].status = PlanStepStatus::Done;
        assert_eq!(current_step(&g), None);
    }

    #[test]
    fn backoff_doubles_and_is_capped() {
        assert_eq!([1, 2, 3, 4].map(|n| backoff_secs(15, n)), [15, 30, 60, 120]);
        assert_eq!(backoff_secs(15, 30), 600);
        assert_eq!(backoff_secs(u64::MAX, 5), 600, "no overflow");
    }

    #[test]
    fn the_default_input_names_the_goal_and_the_step_and_is_stable() {
        let g = crate::goals::test_goal(vec![]);
        let s = PlanStep {
            id: "s2".into(),
            title: "Book".into(),
            detail: Some("window".into()),
            ..Default::default()
        };
        let a = default_input(&g, &s);
        assert_eq!(a["message"], "Book");
        assert_eq!(a["step"]["detail"], "window");
        assert_eq!(
            a,
            default_input(&g, &s),
            "the same input every time, so a retry of a start is the same request"
        );
    }

    #[tokio::test]
    async fn opening_an_approval_puts_the_planners_reason_in_the_prompt_and_the_planned_action() {
        let state = crate::goals::tests::test_state().await;
        let g = crate::goals::test_goal(vec![]);
        let with_reason = PlanStep {
            id: "s1".into(),
            title: "Book the flight".into(),
            requires_approval: true,
            approval_reason: Some("it costs money".into()),
            session_id: Some(Uuid::new_v4()),
            ..Default::default()
        };
        let id = open_approval(&state, &g, &with_reason).await.unwrap();
        let saved = state.store.get_approval(id).await.unwrap();
        assert!(
            saved.prompt.contains("The agent said: it costs money"),
            "{}",
            saved.prompt
        );
        assert_eq!(
            saved.planned_action.unwrap()["approval_reason"],
            "it costs money"
        );

        let without_reason = PlanStep {
            id: "s2".into(),
            title: "Book the flight".into(),
            requires_approval: true,
            session_id: Some(Uuid::new_v4()),
            ..Default::default()
        };
        let id = open_approval(&state, &g, &without_reason).await.unwrap();
        let saved = state.store.get_approval(id).await.unwrap();
        assert!(
            !saved.prompt.contains("The agent said"),
            "no reason, no invented one: {}",
            saved.prompt
        );
        assert_eq!(
            saved.planned_action.unwrap()["approval_reason"],
            Value::Null
        );
    }
}
