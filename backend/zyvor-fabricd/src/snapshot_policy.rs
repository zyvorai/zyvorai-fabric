// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: LicenseRef-Zyvor-Production-2.0

//! Per-VM snapshot policies: a schedule (fixed interval or a 5-field UTC
//! cron expression) plus retention (keep-last-N and/or max-age).
//!
//! Scope and safety rules:
//! * A policy only ever creates and prunes snapshots whose name starts with
//!   its `name_prefix` (default `auto`) followed by `-`. Hand-made snapshots
//!   are never touched.
//! * Pruning never deletes the newest managed snapshot, nor any snapshot
//!   tagged `keep` (`tags` on the snapshot).
//! * With both `keep_last` and `max_age_secs`, a snapshot is kept if EITHER
//!   rule keeps it. `keep_last = 0` is rejected; at least one rule is
//!   required.
//! * Clock skew: a snapshot dated in the future counts as age 0, and a
//!   `last_run` in the future is treated as "just ran", so a clock stepped
//!   backwards neither mass-deletes snapshots nor runs the policy in a loop.
//! * No pre-snapshot quiesce: the driver has no filesystem freeze support.
//!
//! The scheduler is opt-in (`[snapshots] enabled = true`).

use std::sync::Arc;

use async_trait::async_trait;
use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use chrono::{DateTime, Datelike, Duration, TimeZone, Timelike, Utc};
use serde::{Deserialize, Serialize};

use crate::api::snapshots::{CreateSnapshotRequest, SnapshotType, VMSnapshot};
use crate::api_error::json_error;
use crate::server::AppState;
use security::{RequireRead, RequireWrite};

const STORE_KEY: &str = "snapshot_policies";
const MIN_INTERVAL_SECS: u64 = 60;
const MIN_MAX_AGE_SECS: u64 = 60;
/// Tag that protects a snapshot from pruning.
pub const KEEP_TAG: &str = "keep";

// ───────────────────────── model ─────────────────────────

/// What a client sends with `PUT`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SnapshotPolicyInput {
    #[serde(default)]
    pub enabled: bool,
    /// Take a snapshot every N seconds (>= 60). Exactly one of
    /// `interval_secs` / `cron`.
    #[serde(default)]
    pub interval_secs: Option<u64>,
    /// 5-field cron (`min hour dom mon dow`, UTC): `*`, `*/n`, `a-b`, `a,b`.
    #[serde(default)]
    pub cron: Option<String>,
    /// Keep the newest N managed snapshots (N >= 1).
    #[serde(default)]
    pub keep_last: Option<u32>,
    /// Delete managed snapshots older than this many seconds (>= 60).
    #[serde(default)]
    pub max_age_secs: Option<u64>,
    /// Name prefix of managed snapshots (default `auto`).
    #[serde(default = "default_prefix")]
    pub name_prefix: String,
    #[serde(default = "default_type")]
    pub snapshot_type: SnapshotType,
}

fn default_prefix() -> String {
    "auto".to_string()
}

fn default_type() -> SnapshotType {
    SnapshotType::Disk
}

/// A stored policy: the input plus bookkeeping.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SnapshotPolicy {
    pub vm_name: String,
    #[serde(flatten)]
    pub spec: SnapshotPolicyInput,
    pub updated_at: DateTime<Utc>,
    #[serde(default)]
    pub last_run: Option<DateTime<Utc>>,
    #[serde(default)]
    pub last_result: Option<String>,
}

pub fn validate_input(p: &SnapshotPolicyInput) -> Result<(), String> {
    match (&p.interval_secs, &p.cron) {
        (Some(_), Some(_)) => return Err("Set either interval_secs or cron, not both".into()),
        (None, None) => return Err("One of interval_secs or cron is required".into()),
        (Some(i), None) if *i < MIN_INTERVAL_SECS => {
            return Err(format!(
                "interval_secs must be at least {MIN_INTERVAL_SECS}"
            ))
        }
        (None, Some(c)) => {
            CronExpr::parse(c)?;
        }
        _ => {}
    }
    if p.keep_last == Some(0) {
        return Err("keep_last must be at least 1 (0 would delete every snapshot)".into());
    }
    if let Some(a) = p.max_age_secs {
        if a < MIN_MAX_AGE_SECS {
            return Err(format!("max_age_secs must be at least {MIN_MAX_AGE_SECS}"));
        }
    }
    if p.keep_last.is_none() && p.max_age_secs.is_none() {
        return Err("At least one retention rule (keep_last or max_age_secs) is required".into());
    }
    if p.name_prefix.is_empty() || p.name_prefix.len() > 32 {
        return Err("name_prefix must be 1 to 32 characters".into());
    }
    crate::validation::validate_snapshot_name(&format!("{}-20260101T000000Z", p.name_prefix))
        .map_err(|(_, m)| format!("name_prefix: {m}"))?;
    Ok(())
}

// ───────────────────────── cron ─────────────────────────

#[derive(Debug, Clone, PartialEq)]
pub struct CronExpr {
    minutes: u64,
    hours: u64,
    dom: u64,
    months: u64,
    dow: u64,
    dom_star: bool,
    dow_star: bool,
}

fn parse_field(f: &str, lo: u32, hi: u32, name: &str) -> Result<(u64, bool), String> {
    let mut mask = 0u64;
    let star = f == "*";
    for part in f.split(',') {
        let (range, step) = match part.split_once('/') {
            Some((r, s)) => {
                let st: u32 = s
                    .parse()
                    .map_err(|_| format!("bad step in {name}: {part}"))?;
                if st == 0 {
                    return Err(format!("step must be > 0 in {name}"));
                }
                (r, st)
            }
            None => (part, 1),
        };
        let (a, b) = if range == "*" {
            (lo, hi)
        } else if let Some((a, b)) = range.split_once('-') {
            (
                a.parse()
                    .map_err(|_| format!("bad value in {name}: {part}"))?,
                b.parse()
                    .map_err(|_| format!("bad value in {name}: {part}"))?,
            )
        } else {
            let v: u32 = range
                .parse()
                .map_err(|_| format!("bad value in {name}: {part}"))?;
            if part.contains('/') {
                (v, hi)
            } else {
                (v, v)
            }
        };
        if a < lo || b > hi || a > b {
            return Err(format!("{name} out of range {lo}-{hi}: {part}"));
        }
        let mut v = a;
        while v <= b {
            mask |= 1 << v;
            v += step;
        }
    }
    Ok((mask, star))
}

impl CronExpr {
    pub fn parse(s: &str) -> Result<Self, String> {
        let f: Vec<&str> = s.split_whitespace().collect();
        if f.len() != 5 {
            return Err(
                "cron must have 5 fields: minute hour day-of-month month day-of-week".into(),
            );
        }
        let (minutes, _) = parse_field(f[0], 0, 59, "minute")?;
        let (hours, _) = parse_field(f[1], 0, 23, "hour")?;
        let (dom, dom_star) = parse_field(f[2], 1, 31, "day-of-month")?;
        let (months, _) = parse_field(f[3], 1, 12, "month")?;
        let (mut dow, dow_star) = parse_field(f[4], 0, 7, "day-of-week")?;
        if dow & (1 << 7) != 0 {
            dow |= 1; // 7 == Sunday
        }
        Ok(Self {
            minutes,
            hours,
            dom,
            months,
            dow: dow & 0x7f,
            dom_star,
            dow_star,
        })
    }

    fn day_matches(&self, d: chrono::NaiveDate) -> bool {
        if self.months & (1 << d.month()) == 0 {
            return false;
        }
        let dom_ok = self.dom & (1 << d.day()) != 0;
        let dow_ok = self.dow & (1 << d.weekday().num_days_from_sunday()) != 0;
        match (self.dom_star, self.dow_star) {
            (false, false) => dom_ok || dow_ok,
            (false, true) => dom_ok,
            (true, false) => dow_ok,
            (true, true) => true,
        }
    }

    /// First matching minute strictly after `t`.
    pub fn next_after(&self, t: DateTime<Utc>) -> Option<DateTime<Utc>> {
        let start = t.with_second(0)?.with_nanosecond(0)? + Duration::minutes(1);
        let mut day = start.date_naive();
        for i in 0..(366 * 5) {
            if self.day_matches(day) {
                for h in 0..24u32 {
                    if self.hours & (1 << h) == 0 {
                        continue;
                    }
                    for m in 0..60u32 {
                        if self.minutes & (1 << m) == 0 {
                            continue;
                        }
                        let c = Utc.from_utc_datetime(&day.and_hms_opt(h, m, 0)?);
                        if c >= start {
                            return Some(c);
                        }
                    }
                }
            }
            day = day.succ_opt()?;
            let _ = i;
        }
        None
    }
}

// ───────────────────────── scheduling ─────────────────────────

/// Source of "now"; tests substitute a controllable clock.
pub trait Clock: Send + Sync {
    fn now(&self) -> DateTime<Utc>;
}

pub struct SystemClock;

impl Clock for SystemClock {
    fn now(&self) -> DateTime<Utc> {
        Utc::now()
    }
}

/// Whether the policy should run at `now`. A disabled or malformed policy is
/// never due. The anchor is `last_run`, else `updated_at`, clamped to `now`
/// so a clock stepped backwards cannot make it run continuously.
pub fn is_due(p: &SnapshotPolicy, now: DateTime<Utc>) -> bool {
    if !p.spec.enabled {
        return false;
    }
    let anchor = p.last_run.unwrap_or(p.updated_at).min(now);
    if let Some(i) = p.spec.interval_secs {
        return now >= anchor + Duration::seconds(i.min(i64::MAX as u64) as i64);
    }
    if let Some(c) = &p.spec.cron {
        return CronExpr::parse(c)
            .ok()
            .and_then(|c| c.next_after(anchor))
            .is_some_and(|n| n <= now);
    }
    false
}

// ───────────────────────── retention ─────────────────────────

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct Decision {
    pub id: String,
    pub name: String,
    pub created: DateTime<Utc>,
    /// `keep` or `delete`
    pub action: &'static str,
    pub reason: String,
}

fn is_managed(s: &VMSnapshot, prefix: &str) -> bool {
    s.name.starts_with(&format!("{prefix}-"))
}

/// Decide the fate of each managed snapshot. Unmanaged snapshots are absent
/// from the result.
pub fn plan_prune(
    snaps: &[VMSnapshot],
    spec: &SnapshotPolicyInput,
    now: DateTime<Utc>,
) -> Vec<Decision> {
    let mut managed: Vec<&VMSnapshot> = snaps
        .iter()
        .filter(|s| is_managed(s, &spec.name_prefix))
        .collect();
    managed.sort_by(|a, b| b.created.cmp(&a.created).then_with(|| b.id.cmp(&a.id)));
    let mut rank = 0u32;
    let mut out = Vec::new();
    for (i, s) in managed.iter().enumerate() {
        let tagged = s.tags.iter().any(|t| t == KEEP_TAG);
        let (action, reason) = if i == 0 {
            if !tagged {
                rank += 1;
            }
            ("keep", "newest snapshot is never pruned".to_string())
        } else if tagged {
            ("keep", "tagged keep".to_string())
        } else {
            rank += 1;
            let age = (now - s.created).max(Duration::zero());
            let by_count = spec.keep_last.is_some_and(|n| rank <= n);
            let by_age = spec
                .max_age_secs
                .is_some_and(|m| age < Duration::seconds(m.min(i64::MAX as u64) as i64));
            if by_count {
                (
                    "keep",
                    format!("within newest {}", spec.keep_last.unwrap_or(0)),
                )
            } else if by_age {
                ("keep", "younger than max_age".to_string())
            } else {
                ("delete", "beyond retention".to_string())
            }
        };
        out.push(Decision {
            id: s.id.clone(),
            name: s.name.clone(),
            created: s.created,
            action,
            reason,
        });
    }
    out
}

// ───────────────────────── execution ─────────────────────────

#[async_trait]
pub trait SnapshotBackend: Send + Sync {
    async fn list(&self, vm: &str) -> Result<Vec<VMSnapshot>, String>;
    async fn create(&self, vm: &str, name: &str, kind: SnapshotType) -> Result<VMSnapshot, String>;
    async fn delete(&self, vm: &str, id: &str) -> Result<(), String>;
}

#[derive(Debug, Clone, Serialize, Default)]
pub struct RunReport {
    pub vm_name: String,
    pub dry_run: bool,
    pub created: Option<String>,
    pub decisions: Vec<Decision>,
    pub deleted: Vec<String>,
    pub errors: Vec<String>,
}

pub fn snapshot_name(prefix: &str, now: DateTime<Utc>) -> String {
    format!("{prefix}-{}", now.format("%Y%m%dT%H%M%SZ"))
}

/// Create one snapshot, then prune. On a dry run nothing is changed and the
/// report describes what would happen. If creation fails nothing is pruned.
pub async fn run_policy(
    vm: &str,
    spec: &SnapshotPolicyInput,
    backend: &dyn SnapshotBackend,
    clock: &dyn Clock,
    dry_run: bool,
) -> RunReport {
    let now = clock.now();
    let mut report = RunReport {
        vm_name: vm.to_string(),
        dry_run,
        ..Default::default()
    };
    let mut snaps = match backend.list(vm).await {
        Ok(s) => s,
        Err(e) => {
            report.errors.push(format!("list snapshots: {e}"));
            return report;
        }
    };
    let name = snapshot_name(&spec.name_prefix, now);
    if dry_run {
        snaps.push(VMSnapshot {
            id: "(planned)".into(),
            vm_name: vm.into(),
            name: name.clone(),
            description: None,
            snapshot_type: spec.snapshot_type.clone(),
            parent_id: None,
            size_bytes: 0,
            created: now,
            tags: vec![],
        });
        report.created = Some(name);
    } else {
        match backend.create(vm, &name, spec.snapshot_type.clone()).await {
            Ok(s) => {
                report.created = Some(s.name.clone());
                snaps.push(s);
            }
            Err(e) => {
                report.errors.push(format!("create snapshot: {e}"));
                return report;
            }
        }
    }
    report.decisions = plan_prune(&snaps, spec, now);
    if !dry_run {
        for d in report.decisions.iter().filter(|d| d.action == "delete") {
            match backend.delete(vm, &d.id).await {
                Ok(()) => report.deleted.push(d.name.clone()),
                Err(e) => report.errors.push(format!("delete {}: {e}", d.name)),
            }
        }
    }
    report
}

// ───────────────────────── real backend ─────────────────────────

pub struct AppBackend(pub Arc<AppState>);

async fn body_json<T: serde::de::DeserializeOwned>(
    resp: axum::response::Response,
) -> Result<T, String> {
    let bytes = axum::body::to_bytes(resp.into_body(), 4 * 1024 * 1024)
        .await
        .map_err(|e| e.to_string())?;
    serde_json::from_slice(&bytes).map_err(|e| e.to_string())
}

async fn error_text(resp: axum::response::Response) -> String {
    let status = resp.status();
    match axum::body::to_bytes(resp.into_body(), 64 * 1024).await {
        Ok(b) => format!("{status}: {}", String::from_utf8_lossy(&b)),
        Err(_) => status.to_string(),
    }
}

#[async_trait]
impl SnapshotBackend for AppBackend {
    async fn list(&self, vm: &str) -> Result<Vec<VMSnapshot>, String> {
        self.0
            .store
            .list_entities(&format!("snapshots_{vm}"))
            .map_err(|e| e.to_string())
    }

    async fn create(&self, vm: &str, name: &str, kind: SnapshotType) -> Result<VMSnapshot, String> {
        let req = CreateSnapshotRequest {
            name: name.to_string(),
            description: Some("Created by snapshot policy".into()),
            snapshot_type: kind,
            tags: vec![],
        };
        let resp = crate::api::snapshots::create_snapshot_core(&self.0, vm.to_string(), req).await;
        if resp.status() == StatusCode::CREATED {
            body_json(resp).await
        } else {
            Err(error_text(resp).await)
        }
    }

    async fn delete(&self, vm: &str, id: &str) -> Result<(), String> {
        let resp =
            crate::api::snapshots::delete_snapshot_core(&self.0, vm.to_string(), id.to_string())
                .await;
        if resp.status().is_success() {
            Ok(())
        } else {
            Err(error_text(resp).await)
        }
    }
}

// ───────────────────────── storage + scheduler ─────────────────────────

fn load_policy(state: &AppState, vm: &str) -> Result<Option<SnapshotPolicy>, String> {
    state
        .store
        .get_entity::<SnapshotPolicy>(STORE_KEY, vm)
        .map_err(|e| e.to_string())
}

fn save_policy(state: &AppState, p: &SnapshotPolicy) -> Result<(), String> {
    state
        .store
        .save_entity(STORE_KEY, &p.vm_name, p)
        .map_err(|e| e.to_string())
}

/// Run one policy and record the outcome (last_run, audit).
async fn run_and_record(
    state: &Arc<AppState>,
    mut policy: SnapshotPolicy,
    actor: &str,
    dry_run: bool,
) -> RunReport {
    let report = run_policy(
        &policy.vm_name,
        &policy.spec,
        &AppBackend(state.clone()),
        &SystemClock,
        dry_run,
    )
    .await;
    if !dry_run {
        policy.last_run = Some(Utc::now());
        policy.last_result = Some(if report.errors.is_empty() {
            format!(
                "ok: created {}, deleted {}",
                report.created.as_deref().unwrap_or("-"),
                report.deleted.len()
            )
        } else {
            format!("errors: {}", report.errors.join("; "))
        });
        if let Err(e) = save_policy(state, &policy) {
            tracing::warn!(
                "snapshot policy: saving last_run for {}: {e}",
                policy.vm_name
            );
        }
        let resource = format!("vm/{}", policy.vm_name);
        let status = if report.errors.is_empty() {
            "SUCCESS"
        } else {
            "FAILURE"
        };
        crate::routes::audit(state, actor, "SNAPSHOT_POLICY_RUN", &resource, status);
        for name in &report.deleted {
            crate::routes::audit(
                state,
                actor,
                "SNAPSHOT_PRUNE",
                &format!("vm/{}/snapshot/{name}", policy.vm_name),
                "SUCCESS",
            );
        }
    }
    report
}

/// Background task: wake every `tick_secs`, run every due policy.
pub async fn run_snapshot_scheduler(state: Arc<AppState>) {
    let cfg = state.config.snapshots.clone();
    if !cfg.enabled {
        return;
    }
    let tick = std::time::Duration::from_secs(cfg.tick_secs.max(5));
    loop {
        tokio::select! {
            _ = state.shutdown.cancelled() => return,
            _ = tokio::time::sleep(tick) => {}
        }
        let policies: Vec<SnapshotPolicy> =
            state.store.list_entities(STORE_KEY).unwrap_or_default();
        let now = Utc::now();
        for p in policies {
            if state.shutdown.is_cancelled() {
                return;
            }
            if is_due(&p, now) {
                run_and_record(&state, p, "snapshot-scheduler", false).await;
            }
        }
    }
}

// ───────────────────────── handlers ─────────────────────────

fn check_vm(state: &AppState, vm: &str) -> Result<(), Box<axum::response::Response>> {
    if let Err((s, m)) = crate::validation::validate_vm_name(vm) {
        return Err(Box::new(json_error(s, m).into_response()));
    }
    match state.store.get_vm(vm) {
        Ok(Some(_)) => Ok(()),
        _ => Err(Box::new(
            json_error(StatusCode::NOT_FOUND, "VM not found").into_response(),
        )),
    }
}

/// GET /api/vms/{name}/snapshot-policy
pub async fn get_policy(
    RequireRead(_c): RequireRead,
    State(state): State<Arc<AppState>>,
    Path(vm): Path<String>,
) -> impl IntoResponse {
    if let Err(r) = check_vm(&state, &vm) {
        return *r;
    }
    match load_policy(&state, &vm) {
        Ok(Some(p)) => Json(p).into_response(),
        Ok(None) => {
            json_error(StatusCode::NOT_FOUND, "No snapshot policy for this VM").into_response()
        }
        Err(e) => json_error(StatusCode::INTERNAL_SERVER_ERROR, e).into_response(),
    }
}

/// PUT /api/vms/{name}/snapshot-policy
pub async fn put_policy(
    RequireWrite(claims): RequireWrite,
    State(state): State<Arc<AppState>>,
    Path(vm): Path<String>,
    Json(spec): Json<SnapshotPolicyInput>,
) -> impl IntoResponse {
    if let Err(r) = check_vm(&state, &vm) {
        return *r;
    }
    if let Err(e) = validate_input(&spec) {
        return json_error(StatusCode::BAD_REQUEST, e).into_response();
    }
    let prev = load_policy(&state, &vm).ok().flatten();
    let policy = SnapshotPolicy {
        vm_name: vm.clone(),
        spec,
        updated_at: Utc::now(),
        last_run: prev.as_ref().and_then(|p| p.last_run),
        last_result: prev.and_then(|p| p.last_result),
    };
    if let Err(e) = save_policy(&state, &policy) {
        return json_error(StatusCode::INTERNAL_SERVER_ERROR, e).into_response();
    }
    crate::routes::audit(
        &state,
        &claims.sub,
        "SNAPSHOT_POLICY_SET",
        &format!("vm/{vm}"),
        "SUCCESS",
    );
    Json(policy).into_response()
}

/// DELETE /api/vms/{name}/snapshot-policy
pub async fn delete_policy(
    RequireWrite(claims): RequireWrite,
    State(state): State<Arc<AppState>>,
    Path(vm): Path<String>,
) -> impl IntoResponse {
    if let Err((s, m)) = crate::validation::validate_vm_name(&vm) {
        return json_error(s, m).into_response();
    }
    match load_policy(&state, &vm) {
        Ok(Some(_)) => {}
        Ok(None) => {
            return json_error(StatusCode::NOT_FOUND, "No snapshot policy for this VM")
                .into_response()
        }
        Err(e) => return json_error(StatusCode::INTERNAL_SERVER_ERROR, e).into_response(),
    }
    if let Err(e) = state.store.delete_entity(STORE_KEY, &vm) {
        return json_error(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response();
    }
    crate::routes::audit(
        &state,
        &claims.sub,
        "SNAPSHOT_POLICY_DELETE",
        &format!("vm/{vm}"),
        "SUCCESS",
    );
    StatusCode::NO_CONTENT.into_response()
}

#[derive(Debug, Deserialize)]
pub struct RunQuery {
    #[serde(default)]
    pub dry_run: bool,
}

/// POST /api/vms/{name}/snapshot-policy/run?dry_run=
///
/// Runs the policy now regardless of schedule: create one snapshot, prune.
/// A real run deletes snapshots, so it needs admin; a dry run needs write.
pub async fn run_policy_now(
    RequireWrite(claims): RequireWrite,
    State(state): State<Arc<AppState>>,
    Path(vm): Path<String>,
    Query(q): Query<RunQuery>,
) -> impl IntoResponse {
    if let Err(r) = check_vm(&state, &vm) {
        return *r;
    }
    if !q.dry_run && !claims.role.can_manage() {
        return json_error(
            StatusCode::FORBIDDEN,
            "Running a snapshot policy can delete snapshots and needs admin; use dry_run=true to preview",
        )
        .into_response();
    }
    let policy = match load_policy(&state, &vm) {
        Ok(Some(p)) => p,
        Ok(None) => {
            return json_error(StatusCode::NOT_FOUND, "No snapshot policy for this VM")
                .into_response()
        }
        Err(e) => return json_error(StatusCode::INTERNAL_SERVER_ERROR, e).into_response(),
    };
    let report = run_and_record(&state, policy, &claims.sub, q.dry_run).await;
    Json(report).into_response()
}

// ───────────────────────── tests ─────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    struct FakeClock(Mutex<DateTime<Utc>>);
    impl FakeClock {
        fn at(s: &str) -> Self {
            Self(Mutex::new(s.parse().unwrap()))
        }
        fn set(&self, s: &str) {
            *self.0.lock().unwrap() = s.parse().unwrap();
        }
    }
    impl Clock for FakeClock {
        fn now(&self) -> DateTime<Utc> {
            *self.0.lock().unwrap()
        }
    }

    #[derive(Default)]
    struct Fake {
        snaps: Mutex<Vec<VMSnapshot>>,
        fail_create: bool,
        fail_delete: Vec<String>,
        clock: Option<Arc<FakeClock>>,
    }

    #[async_trait]
    impl SnapshotBackend for Fake {
        async fn list(&self, _vm: &str) -> Result<Vec<VMSnapshot>, String> {
            Ok(self.snaps.lock().unwrap().clone())
        }
        async fn create(
            &self,
            vm: &str,
            name: &str,
            k: SnapshotType,
        ) -> Result<VMSnapshot, String> {
            if self.fail_create {
                return Err("boom".into());
            }
            let s = VMSnapshot {
                id: format!("id-{name}"),
                vm_name: vm.into(),
                name: name.into(),
                description: None,
                snapshot_type: k,
                parent_id: None,
                size_bytes: 0,
                created: self.clock.as_ref().unwrap().now(),
                tags: vec![],
            };
            self.snaps.lock().unwrap().push(s.clone());
            Ok(s)
        }
        async fn delete(&self, _vm: &str, id: &str) -> Result<(), String> {
            if self.fail_delete.iter().any(|d| d == id) {
                return Err("nope".into());
            }
            self.snaps.lock().unwrap().retain(|s| s.id != id);
            Ok(())
        }
    }

    fn snap(name: &str, created: &str, tags: &[&str]) -> VMSnapshot {
        VMSnapshot {
            id: format!("id-{name}"),
            vm_name: "vm".into(),
            name: name.into(),
            description: None,
            snapshot_type: SnapshotType::Disk,
            parent_id: None,
            size_bytes: 0,
            created: created.parse().unwrap(),
            tags: tags.iter().map(|t| t.to_string()).collect(),
        }
    }

    fn spec(keep_last: Option<u32>, max_age: Option<u64>) -> SnapshotPolicyInput {
        SnapshotPolicyInput {
            enabled: true,
            interval_secs: Some(3600),
            cron: None,
            keep_last,
            max_age_secs: max_age,
            name_prefix: "auto".into(),
            snapshot_type: SnapshotType::Disk,
        }
    }

    fn deleted(d: &[Decision]) -> Vec<&str> {
        d.iter()
            .filter(|d| d.action == "delete")
            .map(|d| d.name.as_str())
            .collect()
    }

    #[test]
    fn validation_rules() {
        assert!(validate_input(&spec(Some(3), None)).is_ok());
        assert!(validate_input(&spec(Some(0), None))
            .unwrap_err()
            .contains("keep_last"));
        assert!(validate_input(&spec(None, None))
            .unwrap_err()
            .contains("retention"));
        assert!(validate_input(&spec(None, Some(10))).is_err());
        let mut s = spec(Some(1), None);
        s.interval_secs = Some(5);
        assert!(validate_input(&s).is_err());
        s.interval_secs = None;
        assert!(validate_input(&s).unwrap_err().contains("required"));
        s.cron = Some("0 3 * * *".into());
        assert!(validate_input(&s).is_ok());
        s.interval_secs = Some(3600);
        assert!(validate_input(&s).unwrap_err().contains("not both"));
        let mut s = spec(Some(1), None);
        s.name_prefix = "bad prefix!".into();
        assert!(validate_input(&s).is_err());
        s.name_prefix = "-x".into();
        assert!(validate_input(&s).is_err());
    }

    #[test]
    fn cron_parse_and_next() {
        assert!(CronExpr::parse("* * * *").is_err());
        assert!(CronExpr::parse("60 * * * *").is_err());
        assert!(CronExpr::parse("*/0 * * * *").is_err());
        let c = CronExpr::parse("30 2 * * *").unwrap();
        let t: DateTime<Utc> = "2026-03-01T02:30:00Z".parse().unwrap();
        assert_eq!(
            c.next_after(t).unwrap().to_rfc3339(),
            "2026-03-02T02:30:00+00:00"
        );
        let t: DateTime<Utc> = "2026-03-01T01:00:00Z".parse().unwrap();
        assert_eq!(
            c.next_after(t).unwrap().to_rfc3339(),
            "2026-03-01T02:30:00+00:00"
        );
        let c = CronExpr::parse("*/15 * * * *").unwrap();
        let t: DateTime<Utc> = "2026-03-01T10:46:10Z".parse().unwrap();
        assert_eq!(
            c.next_after(t).unwrap().to_rfc3339(),
            "2026-03-01T11:00:00+00:00"
        );
        // Mondays (2026-03-02 is a Monday), 7 == Sunday
        let c = CronExpr::parse("0 0 * * 1").unwrap();
        let t: DateTime<Utc> = "2026-03-02T00:00:00Z".parse().unwrap();
        assert_eq!(
            c.next_after(t).unwrap().to_rfc3339(),
            "2026-03-09T00:00:00+00:00"
        );
        let c = CronExpr::parse("0 0 * * 7").unwrap();
        assert_eq!(
            c.next_after(t).unwrap().to_rfc3339(),
            "2026-03-08T00:00:00+00:00"
        );
        // Feb 29 only
        let c = CronExpr::parse("0 0 29 2 *").unwrap();
        assert_eq!(
            c.next_after(t).unwrap().to_rfc3339(),
            "2028-02-29T00:00:00+00:00"
        );
    }

    fn policy(spec: SnapshotPolicyInput, last: Option<&str>) -> SnapshotPolicy {
        SnapshotPolicy {
            vm_name: "vm".into(),
            spec,
            updated_at: "2026-03-01T00:00:00Z".parse().unwrap(),
            last_run: last.map(|l| l.parse().unwrap()),
            last_result: None,
        }
    }

    #[test]
    fn due_interval_and_skew() {
        let p = policy(spec(Some(3), None), Some("2026-03-01T10:00:00Z"));
        assert!(!is_due(&p, "2026-03-01T10:59:59Z".parse().unwrap()));
        assert!(is_due(&p, "2026-03-01T11:00:00Z".parse().unwrap()));
        // clock stepped back before last_run: not due, and a full interval
        // from "now" is still required
        assert!(!is_due(&p, "2026-03-01T09:00:00Z".parse().unwrap()));
        // never run: anchored at updated_at, not due immediately
        let p = policy(spec(Some(3), None), None);
        assert!(!is_due(&p, "2026-03-01T00:30:00Z".parse().unwrap()));
        assert!(is_due(&p, "2026-03-01T01:00:00Z".parse().unwrap()));
        // disabled
        let mut s = spec(Some(3), None);
        s.enabled = false;
        assert!(!is_due(
            &policy(s, None),
            "2027-01-01T00:00:00Z".parse().unwrap()
        ));
    }

    #[test]
    fn due_cron() {
        let mut s = spec(Some(3), None);
        s.interval_secs = None;
        s.cron = Some("0 3 * * *".into());
        let p = policy(s, Some("2026-03-01T03:00:00Z"));
        assert!(!is_due(&p, "2026-03-02T02:59:00Z".parse().unwrap()));
        assert!(is_due(&p, "2026-03-02T03:00:00Z".parse().unwrap()));
    }

    #[test]
    fn prune_keep_last_and_newest_protected() {
        let snaps = vec![
            snap("auto-1", "2026-03-01T01:00:00Z", &[]),
            snap("auto-2", "2026-03-01T02:00:00Z", &[]),
            snap("auto-3", "2026-03-01T03:00:00Z", &[]),
            snap("manual", "2026-02-01T00:00:00Z", &[]),
        ];
        let now = "2026-03-01T04:00:00Z".parse().unwrap();
        let d = plan_prune(&snaps, &spec(Some(2), None), now);
        assert_eq!(deleted(&d), vec!["auto-1"]);
        assert_eq!(d.len(), 3, "unmanaged snapshot is not listed");
        // max_age so tight everything is old: newest still kept
        let d = plan_prune(&snaps, &spec(None, Some(60)), now);
        assert_eq!(deleted(&d), vec!["auto-2", "auto-1"]);
    }

    #[test]
    fn prune_keep_tag_and_union() {
        let snaps = vec![
            snap("auto-old", "2026-01-01T00:00:00Z", &["keep"]),
            snap("auto-mid", "2026-02-01T00:00:00Z", &[]),
            snap("auto-new", "2026-03-01T00:00:00Z", &[]),
        ];
        let now = "2026-03-01T01:00:00Z".parse().unwrap();
        let d = plan_prune(&snaps, &spec(Some(1), None), now);
        assert_eq!(deleted(&d), vec!["auto-mid"]);
        let old = d.iter().find(|d| d.name == "auto-old").unwrap();
        assert_eq!(old.reason, "tagged keep");
        // union: beyond count but younger than max_age survives
        let d = plan_prune(&snaps, &spec(Some(1), Some(40 * 86400)), now);
        assert!(deleted(&d).is_empty());
    }

    #[test]
    fn prune_clock_skew_future_snapshots_are_kept() {
        let snaps = vec![
            snap("auto-a", "2026-03-01T00:00:00Z", &[]),
            snap("auto-b", "2026-03-05T00:00:00Z", &[]),
        ];
        // "now" is earlier than both: ages clamp to 0, nothing is old
        let now = "2026-02-01T00:00:00Z".parse().unwrap();
        let d = plan_prune(&snaps, &spec(None, Some(3600)), now);
        assert!(deleted(&d).is_empty());
    }

    #[tokio::test]
    async fn run_creates_then_prunes_with_fake_clock() {
        let clock = Arc::new(FakeClock::at("2026-03-01T01:00:00Z"));
        let be = Fake {
            clock: Some(clock.clone()),
            ..Default::default()
        };
        let s = spec(Some(2), None);
        for h in ["01", "02", "03", "04"] {
            clock.set(&format!("2026-03-01T{h}:00:00Z"));
            let r = run_policy("vm", &s, &be, clock.as_ref(), false).await;
            assert!(r.errors.is_empty(), "{:?}", r.errors);
        }
        let names: Vec<String> = be
            .snaps
            .lock()
            .unwrap()
            .iter()
            .map(|s| s.name.clone())
            .collect();
        assert_eq!(
            names,
            vec!["auto-20260301T030000Z", "auto-20260301T040000Z"]
        );
    }

    #[tokio::test]
    async fn dry_run_changes_nothing() {
        let clock = Arc::new(FakeClock::at("2026-03-01T05:00:00Z"));
        let be = Fake {
            clock: Some(clock.clone()),
            snaps: Mutex::new(vec![
                snap("auto-1", "2026-03-01T01:00:00Z", &[]),
                snap("auto-2", "2026-03-01T02:00:00Z", &[]),
            ]),
            ..Default::default()
        };
        let r = run_policy("vm", &spec(Some(1), None), &be, clock.as_ref(), true).await;
        assert_eq!(r.created.as_deref(), Some("auto-20260301T050000Z"));
        assert_eq!(deleted(&r.decisions), vec!["auto-2", "auto-1"]);
        assert!(r.deleted.is_empty());
        assert_eq!(be.snaps.lock().unwrap().len(), 2);
    }

    #[tokio::test]
    async fn create_failure_skips_prune_and_delete_failure_continues() {
        let clock = Arc::new(FakeClock::at("2026-03-01T05:00:00Z"));
        let old = vec![
            snap("auto-1", "2026-03-01T01:00:00Z", &[]),
            snap("auto-2", "2026-03-01T02:00:00Z", &[]),
            snap("auto-3", "2026-03-01T03:00:00Z", &[]),
        ];
        let be = Fake {
            clock: Some(clock.clone()),
            snaps: Mutex::new(old.clone()),
            fail_create: true,
            ..Default::default()
        };
        let r = run_policy("vm", &spec(Some(1), None), &be, clock.as_ref(), false).await;
        assert_eq!(r.errors.len(), 1);
        assert_eq!(be.snaps.lock().unwrap().len(), 3);

        let be = Fake {
            clock: Some(clock.clone()),
            snaps: Mutex::new(old),
            fail_delete: vec!["id-auto-2".into()],
            ..Default::default()
        };
        let r = run_policy("vm", &spec(Some(1), None), &be, clock.as_ref(), false).await;
        assert_eq!(r.errors.len(), 1);
        assert_eq!(r.deleted.len(), 2);
    }
}
