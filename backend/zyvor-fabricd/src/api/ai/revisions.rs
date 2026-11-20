// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: LicenseRef-Zyvor-Production-1.0

//! Immutable deployment revisions and the rollout step function.
//!
//! A deployment keeps one active revision and may run a second revision
//! during a rollout. `status.replicas` is the union of those replica sets.

use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use chrono::Utc;
use security::RequireRead;
use security::RequireWrite;
use std::sync::Arc;

use crate::server::AppState;

use super::types::{
    CanaryStep, CreateRevisionRequest, InferenceDeployment, InferenceDeploymentRevision,
    InferenceReplica, InferenceReplicaSet, RevisionState, RolloutPhase, RolloutRun,
    RolloutStrategy,
};
use super::{audit, err, STORE_REPLICA_SETS, STORE_REVISIONS, STORE_ROLLOUTS};

pub fn revision_id(deployment: &str, revision: u64) -> String {
    format!("{deployment}--{revision}")
}

pub fn replica_set_id(deployment: &str, revision: u64) -> String {
    format!("{deployment}--set-{revision}")
}

pub fn default_canary_steps() -> Vec<CanaryStep> {
    vec![
        CanaryStep {
            weight: 5,
            duration_secs: 300,
        },
        CanaryStep {
            weight: 20,
            duration_secs: 600,
        },
        CanaryStep {
            weight: 50,
            duration_secs: 900,
        },
        CanaryStep {
            weight: 100,
            duration_secs: 0,
        },
    ]
}

pub fn seed_revision(
    state: &AppState,
    dep: &InferenceDeployment,
    model_digest: &str,
) -> Result<(), String> {
    let rev = InferenceDeploymentRevision {
        id: revision_id(&dep.name, dep.revision),
        deployment: dep.name.clone(),
        revision: dep.revision,
        model: dep.model.clone(),
        model_digest: model_digest.to_string(),
        profile: dep.profile.clone(),
        runtime_config_digest: String::new(),
        desired_replicas: dep.replicas,
        created_at: dep.created,
        state: RevisionState::Provisioning,
    };
    state
        .store
        .save_entity(STORE_REVISIONS, &rev.id, &rev)
        .map_err(|e| e.to_string())?;
    let set = InferenceReplicaSet {
        id: replica_set_id(&dep.name, dep.revision),
        deployment: dep.name.clone(),
        revision: dep.revision,
        desired: dep.replicas,
        ready: 0,
        replicas: vec![],
    };
    state
        .store
        .save_entity(STORE_REPLICA_SETS, &set.id, &set)
        .map_err(|e| e.to_string())?;
    Ok(())
}

/// Write replica sets from the deployment's replica list and publish the union
/// back onto `status.replicas`.
pub fn sync_replica_sets(state: &AppState, dep: &mut InferenceDeployment) {
    let mut by_rev: std::collections::BTreeMap<u64, Vec<InferenceReplica>> =
        std::collections::BTreeMap::new();
    for mut rep in dep.status.replicas.drain(..) {
        if rep.revision == 0 {
            rep.revision = dep.revision;
        }
        if rep.deployment.is_empty() {
            rep.deployment = dep.name.clone();
        }
        by_rev.entry(rep.revision).or_default().push(rep);
    }
    if by_rev.is_empty() {
        by_rev.insert(dep.revision, vec![]);
    }
    let mut union = Vec::new();
    for (revision, replicas) in by_rev {
        let ready = replicas.iter().filter(|r| r.ready).count() as u32;
        let id = replica_set_id(&dep.name, revision);
        let stored_desired = state
            .store
            .get_entity::<InferenceReplicaSet>(STORE_REPLICA_SETS, &id)
            .ok()
            .flatten()
            .map(|set| set.desired)
            .unwrap_or(0);
        let desired = (replicas.len() as u32).max(stored_desired);
        let set = InferenceReplicaSet {
            id: replica_set_id(&dep.name, revision),
            deployment: dep.name.clone(),
            revision,
            desired,
            ready,
            replicas: replicas.clone(),
        };
        let _ = state.store.save_entity(STORE_REPLICA_SETS, &set.id, &set);
        union.extend(replicas);
    }
    dep.status.replicas = union;
}

pub fn list_revisions(state: &AppState, deployment: &str) -> Vec<InferenceDeploymentRevision> {
    let mut rows: Vec<InferenceDeploymentRevision> = state
        .store
        .list_entities(STORE_REVISIONS)
        .unwrap_or_default();
    rows.retain(|r| r.deployment == deployment);
    rows.sort_by_key(|r| r.revision);
    rows
}

pub fn active_rollout(state: &AppState, deployment: &str) -> Option<RolloutRun> {
    let mut rows: Vec<RolloutRun> = state
        .store
        .list_entities(STORE_ROLLOUTS)
        .unwrap_or_default();
    rows.retain(|r| r.deployment == deployment && r.phase == RolloutPhase::Progressing);
    rows.pop()
}

pub fn latest_rollout(state: &AppState, deployment: &str) -> Option<RolloutRun> {
    let mut rows: Vec<RolloutRun> = state
        .store
        .list_entities(STORE_ROLLOUTS)
        .unwrap_or_default();
    rows.retain(|r| r.deployment == deployment);
    rows.sort_by_key(|r| r.to_revision);
    rows.pop()
}

/// Keep the previous replica set while a rollout is in progress, paused, or
/// inside the blue/green rollback window.
pub fn retain_second_fleet(run: &RolloutRun, now_unix: i64) -> bool {
    match run.phase {
        RolloutPhase::Progressing | RolloutPhase::Paused | RolloutPhase::RollingBack => true,
        RolloutPhase::Succeeded if run.strategy == RolloutStrategy::BlueGreen => {
            let started = run
                .promoted_at
                .map(|t| t.timestamp())
                .unwrap_or(run.updated_at.timestamp());
            now_unix.saturating_sub(started) < run.rollback_window_secs as i64
        }
        _ => false,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RollingAction {
    AddNew,
    WaitHealthy,
    DrainOld,
    /// A drained old replica can be removed. The last ready old replica stays
    /// when the new revision has none ready.
    RemoveDrained,
    Complete,
    /// A failed new revision must not remove the last ready old replica.
    KeepLastHealthy,
}

#[derive(Debug, Clone, Copy)]
pub struct FleetCounts {
    pub old_total: u32,
    pub old_ready: u32,
    pub new_total: u32,
    pub new_ready: u32,
    pub new_ready_long_enough: u32,
    pub old_draining: u32,
    pub desired: u32,
}

/// One rolling step. Calling it again with the same fleet repeats the same
/// action, so a restart resumes instead of resetting.
pub fn step_rolling(max_surge: u32, max_unavailable: u32, fleet: FleetCounts) -> RollingAction {
    if fleet.new_total >= fleet.desired && fleet.old_total == 0 && fleet.new_ready >= fleet.desired
    {
        return RollingAction::Complete;
    }
    let surge_room = fleet
        .desired
        .saturating_add(max_surge.max(1))
        .saturating_sub(fleet.old_total.saturating_add(fleet.new_total));
    if fleet.new_total < fleet.desired
        && surge_room > 0
        && fleet.new_ready_long_enough == fleet.new_total
    {
        return RollingAction::AddNew;
    }
    if fleet.new_total > fleet.new_ready_long_enough {
        return RollingAction::WaitHealthy;
    }
    if fleet.old_total > 0 {
        if protect_last_healthy(fleet.old_ready, fleet.new_ready) {
            return RollingAction::KeepLastHealthy;
        }
        if fleet.old_draining > 0 {
            return RollingAction::RemoveDrained;
        }
        let unavailable = fleet.old_total.saturating_sub(fleet.old_ready);
        if unavailable < max_unavailable.max(1) || fleet.new_ready > 0 {
            if fleet.old_ready == 0 {
                return RollingAction::KeepLastHealthy;
            }
            return RollingAction::DrainOld;
        }
        return RollingAction::WaitHealthy;
    }
    if fleet.new_ready >= fleet.desired {
        RollingAction::Complete
    } else {
        RollingAction::WaitHealthy
    }
}

/// True when removing one more old replica would leave no ready replica at all.
pub fn protect_last_healthy(old_ready: u32, new_ready: u32) -> bool {
    old_ready <= 1 && new_ready == 0
}

/// Maglev weights for a canary step. `weight` is the percent of traffic for
/// the new revision (1–100).
pub fn canary_weights(step_weight: u8) -> (u16, u16) {
    let pct = u32::from(step_weight.clamp(1, 100));
    let new_w = ((pct * 32) / 100).max(1) as u16;
    let old_w = (((100 - pct) * 32) / 100).max(1) as u16;
    (old_w, new_w)
}

pub fn apply_weights(replicas: &mut [InferenceReplica], revision: u64, weight: u16) {
    for rep in replicas.iter_mut() {
        if rep.revision == revision && !rep.draining {
            rep.maglev_weight = Some(weight);
        }
    }
}

/// Blue/green cuts traffic in one assignment: old weight 0 (draining), new 32.
pub fn blue_green_cutover(replicas: &mut [InferenceReplica], from: u64, to: u64) -> bool {
    let new_ready = replicas
        .iter()
        .filter(|r| r.revision == to && r.ready)
        .count();
    let new_total = replicas.iter().filter(|r| r.revision == to).count();
    if new_total == 0 || new_ready < new_total {
        return false;
    }
    for rep in replicas.iter_mut() {
        if rep.revision == from {
            rep.draining = true;
            rep.maglev_weight = Some(1);
        } else if rep.revision == to {
            rep.draining = false;
            rep.maglev_weight = Some(32);
        }
    }
    true
}

/// Rollback restores the previous revision's weight and does not drop its last
/// ready replica.
pub fn plan_rollback(from_revision: u64, to_revision: u64) -> (u64, RolloutPhase) {
    let _ = to_revision;
    (from_revision, RolloutPhase::RollingBack)
}

pub fn restore_old_weights(replicas: &mut [InferenceReplica], from: u64, to: u64) {
    for rep in replicas.iter_mut() {
        if rep.revision == from {
            rep.draining = false;
            rep.maglev_weight = Some(32);
            rep.lifecycle = "ready".into();
        } else if rep.revision == to {
            rep.draining = true;
            rep.maglev_weight = Some(1);
            rep.lifecycle = "draining".into();
        }
    }
}

/// Advance a stored rollout one tick. Returns whether the deployment model
/// pointer should move to the new revision.
pub fn advance_rollout(
    run: &mut RolloutRun,
    replicas: &mut [InferenceReplica],
    now_unix: i64,
    desired: u32,
    mean_error: f64,
    mean_ttft: f64,
) -> bool {
    if run.phase == RolloutPhase::Paused || run.phase == RolloutPhase::Succeeded {
        return false;
    }
    if (run.rollback_error_rate > 0.0 && mean_error > run.rollback_error_rate)
        || (run.rollback_ttft_ms > 0.0 && mean_ttft > run.rollback_ttft_ms)
    {
        run.phase = RolloutPhase::Paused;
        run.message = Some("paused: error rate or TTFT exceeded rollback thresholds".into());
        return false;
    }
    let old: Vec<_> = replicas
        .iter()
        .filter(|r| r.revision == run.from_revision)
        .collect();
    let new: Vec<_> = replicas
        .iter()
        .filter(|r| r.revision == run.to_revision)
        .collect();
    let fleet = FleetCounts {
        old_total: old.len() as u32,
        old_ready: old.iter().filter(|r| r.ready && !r.draining).count() as u32,
        new_total: new.len() as u32,
        new_ready: new.iter().filter(|r| r.ready).count() as u32,
        new_ready_long_enough: new
            .iter()
            .filter(|r| r.ready && ready_long_enough(r, run.min_ready_secs, now_unix))
            .count() as u32,
        old_draining: old.iter().filter(|r| r.draining).count() as u32,
        desired: desired.max(1),
    };
    match run.strategy {
        RolloutStrategy::Canary => advance_canary(run, replicas, now_unix),
        RolloutStrategy::BlueGreen => {
            if blue_green_cutover(replicas, run.from_revision, run.to_revision) {
                run.phase = RolloutPhase::Succeeded;
                run.promoted_at = Some(Utc::now());
                run.message = Some("blue/green cutover; old fleet retained".into());
                true
            } else {
                run.message = Some("waiting for the new fleet".into());
                false
            }
        }
        RolloutStrategy::Rolling => match step_rolling(run.max_surge, run.max_unavailable, fleet) {
            RollingAction::AddNew => {
                run.message = Some("create one new-revision replica".into());
                false
            }
            RollingAction::WaitHealthy => {
                run.message = Some("waiting for minimum healthy duration".into());
                false
            }
            RollingAction::DrainOld => {
                if protect_last_healthy(fleet.old_ready, fleet.new_ready) {
                    run.message = Some("keeping the last healthy old replica".into());
                    return false;
                }
                if let Some(rep) = replicas
                    .iter_mut()
                    .find(|r| r.revision == run.from_revision && !r.draining && r.ready)
                {
                    rep.draining = true;
                    rep.maglev_weight = Some(1);
                    rep.lifecycle = "draining".into();
                }
                run.message = Some("draining one old replica".into());
                false
            }
            RollingAction::RemoveDrained => {
                run.message = Some("remove one drained old replica".into());
                false
            }
            RollingAction::KeepLastHealthy => {
                run.message = Some("keeping the last healthy old replica".into());
                false
            }
            RollingAction::Complete => {
                run.phase = RolloutPhase::Succeeded;
                run.message = Some("rolling update complete".into());
                true
            }
        },
    }
}

fn advance_canary(run: &mut RolloutRun, replicas: &mut [InferenceReplica], now_unix: i64) -> bool {
    if run.steps.is_empty() {
        run.steps = default_canary_steps();
    }
    let new_ready = replicas
        .iter()
        .filter(|r| r.revision == run.to_revision && r.ready)
        .count();
    if new_ready == 0 {
        run.message = Some("waiting for the new fleet".into());
        return false;
    }
    let idx = run.step_index as usize;
    if idx >= run.steps.len() {
        run.phase = RolloutPhase::Succeeded;
        run.message = Some("canary promoted".into());
        return true;
    }
    let step = run.steps[idx].clone();
    let (old_w, new_w) = canary_weights(step.weight);
    apply_weights(replicas, run.from_revision, old_w);
    apply_weights(replicas, run.to_revision, new_w);
    if run.step_started_unix == 0 {
        run.step_started_unix = now_unix;
    }
    let held = now_unix.saturating_sub(run.step_started_unix);
    if step.duration_secs > 0 && held < step.duration_secs as i64 {
        run.message = Some(format!("canary weight {} held for {held}s", step.weight));
        return false;
    }
    run.message = Some(format!("canary weight {}", step.weight));
    if step.weight >= 100 {
        run.phase = RolloutPhase::Succeeded;
        run.promoted_at = Some(Utc::now());
        return true;
    }
    run.step_index = run.step_index.saturating_add(1);
    run.step_started_unix = now_unix;
    false
}

fn ready_long_enough(rep: &InferenceReplica, min_secs: u64, now_unix: i64) -> bool {
    if min_secs == 0 {
        return true;
    }
    let Some(created) = rep.created_at else {
        return true;
    };
    now_unix.saturating_sub(created.timestamp()) >= min_secs as i64
}

pub fn note_revision_states(
    state: &AppState,
    deployment: &str,
    from: u64,
    to: u64,
    from_state: RevisionState,
    to_state: RevisionState,
) {
    for (revision, next) in [(from, from_state), (to, to_state)] {
        let id = revision_id(deployment, revision);
        let Ok(Some(mut row)) = state
            .store
            .get_entity::<InferenceDeploymentRevision>(STORE_REVISIONS, &id)
        else {
            continue;
        };
        row.state = next;
        let _ = state.store.save_entity(STORE_REVISIONS, &id, &row);
    }
}

/// POST /api/ai/deployments/{name}/revisions
pub async fn create_revision(
    RequireWrite(claims): RequireWrite,
    State(state): State<Arc<AppState>>,
    Path(name): Path<String>,
    Json(req): Json<CreateRevisionRequest>,
) -> Result<(StatusCode, Json<InferenceDeploymentRevision>), (StatusCode, Json<serde_json::Value>)>
{
    let dep = super::rollouts::load_scoped(&state, &claims, &name)?;
    let next = list_revisions(&state, &dep.name)
        .iter()
        .map(|r| r.revision)
        .max()
        .unwrap_or(dep.revision)
        .saturating_add(1);
    let model = req.model.unwrap_or(dep.model);
    let profile = req.profile.unwrap_or(dep.profile);
    let rev = InferenceDeploymentRevision {
        id: revision_id(&dep.name, next),
        deployment: dep.name.clone(),
        revision: next,
        model,
        model_digest: String::new(),
        profile,
        runtime_config_digest: String::new(),
        desired_replicas: dep.replicas,
        created_at: Utc::now(),
        state: RevisionState::Pending,
    };
    state
        .store
        .save_entity(STORE_REVISIONS, &rev.id, &rev)
        .map_err(|e| err(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    let set = InferenceReplicaSet {
        id: replica_set_id(&dep.name, next),
        deployment: dep.name,
        revision: next,
        desired: 0,
        ready: 0,
        replicas: vec![],
    };
    state
        .store
        .save_entity(STORE_REPLICA_SETS, &set.id, &set)
        .map_err(|e| err(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    audit(
        &state,
        &claims.sub,
        "CREATE",
        &format!("ai/deployments/{}/revisions/{}", rev.deployment, next),
        "SUCCESS",
    );
    Ok((StatusCode::CREATED, Json(rev)))
}

/// GET /api/ai/deployments/{name}/revisions
pub async fn list_deployment_revisions(
    RequireRead(claims): RequireRead,
    State(state): State<Arc<AppState>>,
    Path(name): Path<String>,
) -> Result<Json<Vec<InferenceDeploymentRevision>>, (StatusCode, Json<serde_json::Value>)> {
    let _ = super::rollouts::load_scoped(&state, &claims, &name)?;
    Ok(Json(list_revisions(&state, &name)))
}

/// GET /api/ai/deployments/{name}/revisions/{revision}
pub async fn get_revision(
    RequireRead(claims): RequireRead,
    State(state): State<Arc<AppState>>,
    Path((name, revision)): Path<(String, u64)>,
) -> Result<Json<InferenceDeploymentRevision>, (StatusCode, Json<serde_json::Value>)> {
    let _ = super::rollouts::load_scoped(&state, &claims, &name)?;
    let id = revision_id(&name, revision);
    state
        .store
        .get_entity(STORE_REVISIONS, &id)
        .map_err(|e| err(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
        .ok_or_else(|| {
            err(
                StatusCode::NOT_FOUND,
                format!("revision {revision} not found"),
            )
        })
        .map(Json)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rep(revision: u64, ready: bool) -> InferenceReplica {
        InferenceReplica {
            replica_id: format!("r{revision}"),
            ordinal: revision as u32,
            vm_name: format!("dep-{revision}"),
            bdf: "dry-run".into(),
            ready,
            address: None,
            metrics: None,
            maglev_weight: None,
            site: None,
            cost_tier: None,
            draining: false,
            unhealthy_streak: 0,
            deployment: "dep".into(),
            revision,
            model_digest: "m".into(),
            profile_digest: "p".into(),
            host: String::new(),
            generation: 1,
            lifecycle: "ready".into(),
            health: if ready { "healthy" } else { "unhealthy" }.into(),
            created_at: None,
        }
    }

    #[test]
    fn rolling_keeps_old_and_new_and_refuses_the_last_healthy() {
        let action = step_rolling(
            1,
            1,
            FleetCounts {
                old_total: 1,
                old_ready: 1,
                new_total: 0,
                new_ready: 0,
                new_ready_long_enough: 0,
                old_draining: 0,
                desired: 1,
            },
        );
        assert_eq!(action, RollingAction::AddNew);
        assert!(protect_last_healthy(1, 0));
        assert_eq!(
            step_rolling(
                1,
                1,
                FleetCounts {
                    old_total: 1,
                    old_ready: 1,
                    new_total: 1,
                    new_ready: 0,
                    new_ready_long_enough: 0,
                    old_draining: 0,
                    desired: 1,
                },
            ),
            RollingAction::WaitHealthy
        );
    }

    #[test]
    fn restart_repeats_the_stored_step() {
        let fleet = FleetCounts {
            old_total: 1,
            old_ready: 1,
            new_total: 1,
            new_ready: 1,
            new_ready_long_enough: 1,
            old_draining: 0,
            desired: 1,
        };
        let first = step_rolling(1, 1, fleet);
        let second = step_rolling(1, 1, fleet);
        assert_eq!(first, second);
        assert_eq!(first, RollingAction::DrainOld);
    }

    #[test]
    fn canary_weights_follow_the_new_revision() {
        let (old, new) = canary_weights(5);
        assert!(new < old);
        let (old, new) = canary_weights(100);
        assert_eq!(new, 32);
        assert_eq!(old, 1);
    }

    #[test]
    fn rollback_restores_the_previous_revision() {
        let (active, phase) = plan_rollback(3, 4);
        assert_eq!(active, 3);
        assert_eq!(phase, RolloutPhase::RollingBack);
        let mut replicas = vec![rep(3, true), rep(4, true)];
        restore_old_weights(&mut replicas, 3, 4);
        assert_eq!(replicas[0].maglev_weight, Some(32));
        assert!(!replicas[0].draining);
        assert!(replicas[1].draining);
        assert_eq!(replicas.iter().filter(|r| r.revision == 3).count(), 1);
    }

    #[test]
    fn blue_green_waits_until_the_new_fleet_is_ready() {
        let mut replicas = vec![rep(1, true), rep(2, false)];
        assert!(!blue_green_cutover(&mut replicas, 1, 2));
        replicas[1].ready = true;
        assert!(blue_green_cutover(&mut replicas, 1, 2));
        assert_eq!(replicas[1].maglev_weight, Some(32));
        assert!(replicas[0].draining);
    }

    fn sample_run(strategy: RolloutStrategy) -> RolloutRun {
        RolloutRun {
            id: "dep--rollout-2".into(),
            deployment: "dep".into(),
            strategy,
            from_revision: 1,
            to_revision: 2,
            max_surge: 1,
            max_unavailable: 1,
            min_ready_secs: 0,
            phase: RolloutPhase::Progressing,
            step_index: 0,
            steps: vec![CanaryStep {
                weight: 5,
                duration_secs: 300,
            }],
            rollback_error_rate: 0.15,
            rollback_ttft_ms: 2000.0,
            rollback_window_secs: 600,
            step_started_unix: 0,
            promoted_at: None,
            message: None,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        }
    }

    #[test]
    fn canary_holds_the_step_for_its_duration() {
        let mut run = sample_run(RolloutStrategy::Canary);
        let mut replicas = vec![rep(1, true), rep(2, true)];
        assert!(!advance_rollout(
            &mut run,
            &mut replicas,
            1_000,
            1,
            0.0,
            0.0
        ));
        assert_eq!(run.step_index, 0);
        assert_eq!(run.phase, RolloutPhase::Progressing);
        assert!(!advance_rollout(
            &mut run,
            &mut replicas,
            1_100,
            1,
            0.0,
            0.0
        ));
        assert_eq!(run.step_index, 0);
        let (old_w, new_w) = canary_weights(5);
        assert_eq!(replicas[0].maglev_weight, Some(old_w));
        assert_eq!(replicas[1].maglev_weight, Some(new_w));
    }

    #[test]
    fn threshold_pauses_instead_of_dropping_the_old_replica() {
        let mut run = sample_run(RolloutStrategy::Canary);
        let mut replicas = vec![rep(1, true), rep(2, true)];
        assert!(!advance_rollout(
            &mut run,
            &mut replicas,
            1_000,
            1,
            0.9,
            0.0
        ));
        assert_eq!(run.phase, RolloutPhase::Paused);
        assert_eq!(replicas.iter().filter(|r| r.revision == 1).count(), 1);
        assert!(!replicas[0].draining);
    }
}
