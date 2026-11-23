// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: LicenseRef-Zyvor-Production-2.0

//! Autostart reconciliation: start the VMs that are flagged to run but are
//! not running, in a configurable order and with delays, using the same
//! pre-flight check as `POST /vms/{name}/preflight` so VMs the backend
//! cannot run are reported rather than started.
//!
//! A VM opts in with the tag `autostart` or the label `autostart=true`
//! (an explicit falsy label such as `autostart=false` wins over the tag).
//! Optional ordering and spacing, lowest order first, ties by name:
//!
//! * order: label `autostart.order=N` or tag `autostart-order:N` (default 1000)
//! * delay after this VM starts: label `autostart.delay=SECS` or tag
//!   `autostart-delay:SECS` (default `[autostart] start_delay_secs`)
//!
//! The pass is driven through the small [`Host`] trait so the logic is unit
//! tested with a fake backend; `Arc<AppState>` is the real implementation.

use std::sync::Arc;
use std::time::Duration;

use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use security::{RequireRead, RequireWrite};
use serde::{Deserialize, Serialize};
use vm_model::{VMState, VM};

use crate::api_error::json_error;
use crate::config::AutostartConfig;
use crate::preflight::{BackendCaps, Finding, Severity};
use crate::server::AppState;

const DEFAULT_ORDER: i64 = 1000;

/// Only one live reconciliation at a time (startup pass vs. manual call).
static LIVE_RUN: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

/// A VM's autostart settings, parsed from its tags and labels.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AutostartPolicy {
    pub order: i64,
    pub delay_secs: Option<u64>,
}

fn truthy(v: &str) -> bool {
    matches!(
        v.trim().to_ascii_lowercase().as_str(),
        "true" | "1" | "yes" | "on"
    )
}

fn falsy(v: &str) -> bool {
    matches!(
        v.trim().to_ascii_lowercase().as_str(),
        "false" | "0" | "no" | "off"
    )
}

/// `Some` when the VM is flagged for autostart.
pub fn autostart_policy(vm: &VM) -> Option<AutostartPolicy> {
    let label = |k: &str| {
        vm.labels
            .as_ref()
            .and_then(|l| l.get(k))
            .map(String::as_str)
    };
    let tag_val = |prefix: &str| {
        vm.tags
            .iter()
            .flatten()
            .find_map(|t| t.strip_prefix(prefix).map(str::to_string))
    };
    let flag = label("autostart");
    if flag.is_some_and(falsy) {
        return None;
    }
    let tagged = vm.tags.iter().flatten().any(|t| t == "autostart");
    if !(tagged || flag.is_some_and(truthy)) {
        return None;
    }
    let order = label("autostart.order")
        .map(str::to_string)
        .or_else(|| tag_val("autostart-order:"))
        .and_then(|s| s.trim().parse::<i64>().ok())
        .unwrap_or(DEFAULT_ORDER);
    let delay_secs = label("autostart.delay")
        .map(str::to_string)
        .or_else(|| tag_val("autostart-delay:"))
        .and_then(|s| s.trim().parse::<u64>().ok());
    Some(AutostartPolicy { order, delay_secs })
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct StartedVm {
    pub vm: String,
    pub order: i64,
    /// Seconds waited (or, in a dry run, that would be waited) before the
    /// next start; 0 for the last VM.
    pub delay_after_secs: u64,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct SkippedVm {
    pub vm: String,
    /// `already_running`, `stopping`, `paused`, `preflight_blocked` or
    /// `shutdown`.
    pub code: &'static str,
    pub reason: String,
    /// Blocking pre-flight findings for `preflight_blocked`.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub findings: Vec<Finding>,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct FailedVm {
    pub vm: String,
    pub reason: String,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct ReportSummary {
    pub candidates: usize,
    pub started: usize,
    pub skipped: usize,
    pub failed: usize,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct AutostartReport {
    pub dry_run: bool,
    /// `startup` or `manual`.
    pub trigger: &'static str,
    /// In a dry run these are the VMs that would be started.
    pub started: Vec<StartedVm>,
    pub skipped: Vec<SkippedVm>,
    pub failed: Vec<FailedVm>,
    pub summary: ReportSummary,
}

/// What reconciliation needs from the system.
#[allow(async_fn_in_trait)]
pub trait Host {
    fn list_vms(&self) -> Result<Vec<VM>, String>;
    /// The backend's own view of the VM, not the stored one.
    async fn actual_state(&self, name: &str) -> VMState;
    async fn preflight(&self, vm: &VM) -> Vec<Finding>;
    async fn start(&self, vm: &VM) -> Result<(), String>;
    /// Wait between starts; `false` when interrupted by shutdown.
    async fn pause(&self, d: Duration) -> bool;
}

fn blocker_text(findings: &[Finding]) -> String {
    findings
        .iter()
        .filter(|f| f.severity == Severity::Blocker)
        .map(|f| f.message.clone())
        .collect::<Vec<_>>()
        .join(" ")
}

fn note_skip(sink: &mut (dyn FnMut(Progress) + Send), s: &SkippedVm) {
    sink(Progress::Vm {
        vm: s.vm.clone(),
        status: "skipped",
        code: Some(s.code),
        reason: Some(s.reason.clone()),
    });
}

fn shutdown_skip(name: String) -> SkippedVm {
    SkippedVm {
        vm: name,
        code: "shutdown",
        reason: "The daemon is shutting down.".into(),
        findings: vec![],
    }
}

/// A progress notification emitted while a pass runs.
#[derive(Debug, Clone)]
pub enum Progress {
    /// The ordered candidate list (name, order), before anything is touched.
    Candidates(Vec<(String, i64)>),
    /// A VM's status changed: `checking`, `waiting`, `starting`, `started`,
    /// `skipped` (with `code`) or `failed`.
    Vm {
        vm: String,
        status: &'static str,
        code: Option<&'static str>,
        reason: Option<String>,
    },
}

fn note(sink: &mut (dyn FnMut(Progress) + Send), vm: &str, status: &'static str) {
    sink(Progress::Vm {
        vm: vm.to_string(),
        status,
        code: None,
        reason: None,
    });
}

/// Compare every flagged VM against its actual state and start the ones that
/// should be running. With `dry_run` nothing is started or waited for.
pub async fn reconcile<H: Host>(
    host: &H,
    cfg: &AutostartConfig,
    dry_run: bool,
    trigger: &'static str,
) -> Result<AutostartReport, String> {
    reconcile_with(host, cfg, dry_run, trigger, &mut |_| {}).await
}

/// [`reconcile`] that reports each step to `sink` as it happens.
pub async fn reconcile_with<H: Host>(
    host: &H,
    cfg: &AutostartConfig,
    dry_run: bool,
    trigger: &'static str,
    sink: &mut (dyn FnMut(Progress) + Send),
) -> Result<AutostartReport, String> {
    let mut candidates: Vec<(VM, AutostartPolicy)> = host
        .list_vms()?
        .into_iter()
        .filter_map(|vm| autostart_policy(&vm).map(|p| (vm, p)))
        .collect();
    candidates.sort_by(|a, b| (a.1.order, &a.0.name).cmp(&(b.1.order, &b.0.name)));

    let total = candidates.len();
    sink(Progress::Candidates(
        candidates
            .iter()
            .map(|(vm, p)| (vm.name.clone(), p.order))
            .collect(),
    ));
    let mut started = Vec::new();
    let mut skipped = Vec::new();
    let mut failed = Vec::new();
    let mut interrupted = false;

    // The previous started VM's delay is applied lazily, only once another
    // VM actually needs starting.
    let mut pending_delay: u64 = 0;

    for (mut vm, policy) in candidates {
        if interrupted {
            let s = shutdown_skip(vm.name);
            note_skip(sink, &s);
            skipped.push(s);
            continue;
        }
        note(sink, &vm.name, "checking");
        let actual = host.actual_state(&vm.name).await;
        let skip = |code: &'static str, reason: &str| SkippedVm {
            vm: vm.name.clone(),
            code,
            reason: reason.to_string(),
            findings: vec![],
        };
        match actual {
            VMState::Running | VMState::Starting => {
                let s = skip("already_running", "The VM is already running.");
                note_skip(sink, &s);
                skipped.push(s);
                continue;
            }
            VMState::Stopping => {
                let s = skip("stopping", "The VM is stopping; not started.");
                note_skip(sink, &s);
                skipped.push(s);
                continue;
            }
            VMState::Paused => {
                let s = skip("paused", "The VM is paused; resume it instead.");
                note_skip(sink, &s);
                skipped.push(s);
                continue;
            }
            VMState::Stopped | VMState::Failed | VMState::Unknown => {}
        }
        // The stored state can be stale after a host reboot; judge on the
        // backend's view.
        vm.state = actual;

        let findings = host.preflight(&vm).await;
        if findings.iter().any(|f| f.severity == Severity::Blocker) {
            let s = SkippedVm {
                vm: vm.name.clone(),
                code: "preflight_blocked",
                reason: blocker_text(&findings),
                findings: findings
                    .into_iter()
                    .filter(|f| f.severity == Severity::Blocker)
                    .collect(),
            };
            note_skip(sink, &s);
            skipped.push(s);
            continue;
        }

        if !dry_run {
            let wait = std::mem::take(&mut pending_delay);
            if wait > 0 {
                note(sink, &vm.name, "waiting");
                if !host.pause(Duration::from_secs(wait)).await {
                    interrupted = true;
                    let s = shutdown_skip(vm.name);
                    note_skip(sink, &s);
                    skipped.push(s);
                    continue;
                }
            }
            note(sink, &vm.name, "starting");
            if let Err(reason) = host.start(&vm).await {
                sink(Progress::Vm {
                    vm: vm.name.clone(),
                    status: "failed",
                    code: None,
                    reason: Some(reason.clone()),
                });
                failed.push(FailedVm {
                    vm: vm.name,
                    reason,
                });
                continue;
            }
        }
        note(sink, &vm.name, "started");
        pending_delay = policy
            .delay_secs
            .unwrap_or(cfg.start_delay_secs)
            .min(cfg.max_delay_secs);
        started.push(StartedVm {
            vm: vm.name,
            order: policy.order,
            delay_after_secs: pending_delay,
        });
    }
    // Nothing follows the last started VM.
    if let Some(last) = started.last_mut() {
        last.delay_after_secs = 0;
    }

    let summary = ReportSummary {
        candidates: total,
        started: started.len(),
        skipped: skipped.len(),
        failed: failed.len(),
    };
    Ok(AutostartReport {
        dry_run,
        trigger,
        started,
        skipped,
        failed,
        summary,
    })
}

impl Host for Arc<AppState> {
    fn list_vms(&self) -> Result<Vec<VM>, String> {
        self.store.list_vms().map_err(|e| e.to_string())
    }

    async fn actual_state(&self, name: &str) -> VMState {
        self.driver
            .get_state(name)
            .await
            .unwrap_or(VMState::Unknown)
    }

    async fn preflight(&self, vm: &VM) -> Vec<Finding> {
        let opts = crate::routes::stored_start_options(vm);
        let caps = BackendCaps::for_backend(self.driver.backend_name());
        let host = crate::preflight::gather_host_inputs(self, vm, &opts).await;
        crate::preflight::check(vm, &opts, &caps, &host)
    }

    async fn start(&self, vm: &VM) -> Result<(), String> {
        let _lock = self.vm_lock(&vm.name).lock_owned().await;
        let mut current = match self.store.get_vm(&vm.name) {
            Ok(Some(v)) => v,
            Ok(None) => return Err("VM not found".into()),
            Err(e) => return Err(e.to_string()),
        };
        current.state = VMState::Starting;
        self.store
            .save_vm(&current)
            .map_err(|e| format!("Failed to save VM state: {e}"))?;
        crate::routes::audit(
            self,
            "autostart",
            "START",
            &format!("vm/{}", vm.name),
            "ACCEPTED",
        );
        crate::routes::run_start_task(self, &vm.name, None).await
    }

    async fn pause(&self, d: Duration) -> bool {
        tokio::select! {
            _ = self.shutdown.cancelled() => false,
            _ = tokio::time::sleep(d) => true,
        }
    }
}

// ─── Run history ────────────────────────────────────────────────────────────

/// Recent runs kept in memory (oldest dropped first).
const MAX_RUNS: usize = 20;

/// One VM's progress within a run.
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct VmProgress {
    pub vm: String,
    pub order: i64,
    /// `pending`, `checking`, `waiting`, `starting`, `started`, `skipped`
    /// or `failed`.
    pub status: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub code: Option<&'static str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

/// A live reconciliation run, as returned by the job endpoints.
#[derive(Debug, Clone, Serialize)]
pub struct RunRecord {
    pub id: String,
    /// `startup` or `manual`.
    pub trigger: &'static str,
    /// `running`, `completed` or `failed`.
    pub state: &'static str,
    pub started_at: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub finished_at: Option<String>,
    pub vms: Vec<VmProgress>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub report: Option<AutostartReport>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

/// Compact row for the run list.
#[derive(Debug, Clone, Serialize)]
pub struct RunSummary {
    pub id: String,
    pub trigger: &'static str,
    pub state: &'static str,
    pub started_at: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub finished_at: Option<String>,
    pub candidates: usize,
    pub started: usize,
    pub skipped: usize,
    pub failed: usize,
}

impl RunRecord {
    fn summary(&self) -> RunSummary {
        let count = |s: &str| self.vms.iter().filter(|v| v.status == s).count();
        RunSummary {
            id: self.id.clone(),
            trigger: self.trigger,
            state: self.state,
            started_at: self.started_at.clone(),
            finished_at: self.finished_at.clone(),
            candidates: self.vms.len(),
            started: count("started"),
            skipped: count("skipped"),
            failed: count("failed"),
        }
    }
}

static RUNS: std::sync::Mutex<Vec<RunRecord>> = std::sync::Mutex::new(Vec::new());

fn runs() -> std::sync::MutexGuard<'static, Vec<RunRecord>> {
    RUNS.lock().unwrap_or_else(|e| e.into_inner())
}

fn begin_run(trigger: &'static str) -> String {
    let id = uuid::Uuid::new_v4().to_string();
    let mut r = runs();
    r.push(RunRecord {
        id: id.clone(),
        trigger,
        state: "running",
        started_at: chrono::Utc::now().to_rfc3339(),
        finished_at: None,
        vms: vec![],
        report: None,
        error: None,
    });
    let excess = r.len().saturating_sub(MAX_RUNS);
    r.drain(..excess);
    id
}

fn record_progress(id: &str, p: Progress) {
    let mut r = runs();
    let Some(run) = r.iter_mut().find(|x| x.id == id) else {
        return;
    };
    match p {
        Progress::Candidates(list) => {
            run.vms = list
                .into_iter()
                .map(|(vm, order)| VmProgress {
                    vm,
                    order,
                    status: "pending",
                    code: None,
                    reason: None,
                })
                .collect();
        }
        Progress::Vm {
            vm,
            status,
            code,
            reason,
        } => {
            if let Some(v) = run.vms.iter_mut().find(|v| v.vm == vm) {
                v.status = status;
                v.code = code;
                v.reason = reason;
            }
        }
    }
}

fn finish_run(id: &str, result: &Result<AutostartReport, String>) {
    let mut r = runs();
    if let Some(run) = r.iter_mut().find(|x| x.id == id) {
        run.finished_at = Some(chrono::Utc::now().to_rfc3339());
        match result {
            Ok(rep) => {
                run.state = "completed";
                run.report = Some(rep.clone());
            }
            Err(e) => {
                run.state = "failed";
                run.error = Some(e.clone());
            }
        }
    }
}

/// Run a live pass, recording it in the history under `id`.
async fn run_recorded(
    state: &Arc<AppState>,
    id: &str,
    trigger: &'static str,
) -> Result<AutostartReport, String> {
    let cfg = state.config.autostart.clone();
    let result = reconcile_with(state, &cfg, false, trigger, &mut |p| record_progress(id, p)).await;
    finish_run(id, &result);
    result
}

/// Background task: one reconciliation shortly after daemon start.
pub async fn run_startup_reconcile(state: Arc<AppState>) {
    let cfg = state.config.autostart.clone();
    if !cfg.enabled {
        return;
    }
    if !state
        .pause(Duration::from_secs(cfg.startup_delay_secs))
        .await
    {
        return;
    }
    let _guard = LIVE_RUN.lock().await;
    let id = begin_run("startup");
    match run_recorded(&state, &id, "startup").await {
        Ok(r) => tracing::info!(
            "Autostart reconcile: {} started, {} skipped, {} failed",
            r.summary.started,
            r.summary.skipped,
            r.summary.failed
        ),
        Err(e) => tracing::error!("Autostart reconcile failed: {e}"),
    }
}

#[derive(Debug, Deserialize)]
pub struct ReconcileQuery {
    #[serde(default)]
    pub dry_run: bool,
}

/// POST /api/vms/autostart/reconcile?dry_run=true|false
///
/// A dry run answers 200 with the report. A live run answers 202 with a job
/// id; follow it with `GET /api/vms/autostart/reconcile/{id}`.
pub async fn reconcile_autostart(
    RequireWrite(claims): RequireWrite,
    State(state): State<Arc<AppState>>,
    Query(q): Query<ReconcileQuery>,
) -> impl IntoResponse {
    if claims.tenant.is_some() {
        return json_error(
            StatusCode::FORBIDDEN,
            "Autostart reconciliation covers every VM and is not available to tenant-scoped users",
        )
        .into_response();
    }
    if q.dry_run {
        return match reconcile(&state, &state.config.autostart, true, "manual").await {
            Ok(report) => (StatusCode::OK, Json(report)).into_response(),
            Err(e) => json_error(StatusCode::INTERNAL_SERVER_ERROR, e).into_response(),
        };
    }
    let Ok(guard) = LIVE_RUN.try_lock() else {
        return json_error(
            StatusCode::CONFLICT,
            "An autostart reconciliation is already running",
        )
        .into_response();
    };
    let id = begin_run("manual");
    let job = id.clone();
    let actor = claims.sub.clone();
    tokio::spawn(async move {
        let _guard = guard;
        let ok = run_recorded(&state, &job, "manual").await.is_ok();
        crate::routes::audit(
            &state,
            &actor,
            "AUTOSTART_RECONCILE",
            "vms",
            if ok { "SUCCESS" } else { "FAILURE" },
        );
    });
    (
        StatusCode::ACCEPTED,
        Json(serde_json::json!({
            "id": id,
            "state": "running",
            "status_url": format!("/api/vms/autostart/reconcile/{id}"),
        })),
    )
        .into_response()
}

/// GET /api/vms/autostart/reconcile: recent live runs, newest first.
pub async fn list_autostart_runs(RequireRead(claims): RequireRead) -> impl IntoResponse {
    if claims.tenant.is_some() {
        return json_error(
            StatusCode::FORBIDDEN,
            "Autostart runs are not available to tenant-scoped users",
        )
        .into_response();
    }
    let list: Vec<RunSummary> = runs().iter().rev().map(RunRecord::summary).collect();
    Json(serde_json::json!({ "runs": list })).into_response()
}

/// GET /api/vms/autostart/reconcile/{id}: progress and, once done, the report.
pub async fn get_autostart_run(
    RequireRead(claims): RequireRead,
    Path(id): Path<String>,
) -> impl IntoResponse {
    if claims.tenant.is_some() {
        return json_error(
            StatusCode::FORBIDDEN,
            "Autostart runs are not available to tenant-scoped users",
        )
        .into_response();
    }
    let found = runs().iter().find(|r| r.id == id).cloned();
    match found {
        Some(r) => Json(r).into_response(),
        None => json_error(StatusCode::NOT_FOUND, "Autostart run not found").into_response(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use std::sync::Mutex;

    struct Fake {
        vms: Vec<VM>,
        actual: HashMap<String, VMState>,
        blocked: Vec<&'static str>,
        fail: Vec<&'static str>,
        log: Mutex<Vec<String>>,
        interrupt_pause: bool,
    }

    impl Fake {
        fn new(vms: Vec<VM>) -> Self {
            Self {
                vms,
                actual: HashMap::new(),
                blocked: vec![],
                fail: vec![],
                log: Mutex::new(vec![]),
                interrupt_pause: false,
            }
        }
        fn log(&self) -> Vec<String> {
            self.log.lock().unwrap().clone()
        }
    }

    impl Host for Fake {
        fn list_vms(&self) -> Result<Vec<VM>, String> {
            Ok(self.vms.clone())
        }
        async fn actual_state(&self, name: &str) -> VMState {
            self.actual.get(name).cloned().unwrap_or(VMState::Stopped)
        }
        async fn preflight(&self, vm: &VM) -> Vec<Finding> {
            if self.blocked.contains(&vm.name.as_str()) {
                vec![Finding {
                    id: "backend.tpm",
                    severity: Severity::Blocker,
                    message: "The backend does not support TPM.".into(),
                    hint: "Remove TPM.".into(),
                }]
            } else {
                vec![Finding {
                    id: "capacity.cpu",
                    severity: Severity::Warning,
                    message: "tight".into(),
                    hint: String::new(),
                }]
            }
        }
        async fn start(&self, vm: &VM) -> Result<(), String> {
            self.log.lock().unwrap().push(format!("start {}", vm.name));
            if self.fail.contains(&vm.name.as_str()) {
                Err("driver said no".into())
            } else {
                Ok(())
            }
        }
        async fn pause(&self, d: Duration) -> bool {
            self.log
                .lock()
                .unwrap()
                .push(format!("pause {}", d.as_secs()));
            !self.interrupt_pause
        }
    }

    fn vm(name: &str, tags: &[&str]) -> VM {
        let mut v = VM::new(name.into(), "img.qcow2".into(), 1, 512);
        v.state = VMState::Stopped;
        v.tags = Some(tags.iter().map(|s| s.to_string()).collect());
        v
    }

    fn cfg(delay: u64) -> AutostartConfig {
        AutostartConfig {
            enabled: true,
            startup_delay_secs: 0,
            start_delay_secs: delay,
            max_delay_secs: 600,
        }
    }

    #[test]
    fn policy_parsing() {
        assert!(autostart_policy(&vm("a", &[])).is_none());
        assert_eq!(
            autostart_policy(&vm("a", &["autostart"])),
            Some(AutostartPolicy {
                order: 1000,
                delay_secs: None
            })
        );
        let p = autostart_policy(&vm(
            "a",
            &["autostart", "autostart-order:5", "autostart-delay:7"],
        ))
        .unwrap();
        assert_eq!((p.order, p.delay_secs), (5, Some(7)));

        let mut v = vm("a", &[]);
        v.labels = Some(HashMap::from([
            ("autostart".into(), "True".into()),
            ("autostart.order".into(), "-1".into()),
        ]));
        assert_eq!(autostart_policy(&v).unwrap().order, -1);
        // explicit false label beats the tag
        let mut v = vm("a", &["autostart"]);
        v.labels = Some(HashMap::from([("autostart".into(), "false".into())]));
        assert!(autostart_policy(&v).is_none());
    }

    #[tokio::test]
    async fn starts_in_order_with_delays_and_skips_the_rest() {
        let mut f = Fake::new(vec![
            vm("c", &["autostart", "autostart-order:3"]),
            vm(
                "a",
                &["autostart", "autostart-order:1", "autostart-delay:9"],
            ),
            vm("b", &["autostart", "autostart-order:1"]),
            vm("up", &["autostart"]),
            vm("tpm", &["autostart"]),
            vm("plain", &[]),
        ]);
        f.actual.insert("up".into(), VMState::Running);
        f.blocked = vec!["tpm"];

        let r = reconcile(&f, &cfg(2), false, "manual").await.unwrap();
        assert_eq!(
            f.log(),
            ["start a", "pause 9", "start b", "pause 2", "start c"]
        );
        let names: Vec<_> = r.started.iter().map(|s| s.vm.as_str()).collect();
        assert_eq!(names, ["a", "b", "c"]);
        assert_eq!(r.started[0].delay_after_secs, 9);
        assert_eq!(r.started[2].delay_after_secs, 0);
        assert_eq!(r.summary.candidates, 5);
        let codes: Vec<_> = r.skipped.iter().map(|s| (s.vm.as_str(), s.code)).collect();
        assert_eq!(
            codes,
            [("tpm", "preflight_blocked"), ("up", "already_running")]
        );
        assert!(r.skipped[0].reason.contains("TPM"));
        assert_eq!(r.skipped[0].findings.len(), 1);
        assert!(r.failed.is_empty());
    }

    #[tokio::test]
    async fn dry_run_starts_and_waits_for_nothing() {
        let f = Fake::new(vec![vm("a", &["autostart"]), vm("b", &["autostart"])]);
        let r = reconcile(&f, &cfg(5), true, "manual").await.unwrap();
        assert!(f.log().is_empty());
        assert!(r.dry_run);
        assert_eq!(r.started.len(), 2);
        assert_eq!(r.started[0].delay_after_secs, 5);
    }

    #[tokio::test]
    async fn failure_is_reported_and_does_not_stop_the_pass() {
        let mut f = Fake::new(vec![vm("a", &["autostart"]), vm("b", &["autostart"])]);
        f.fail = vec!["a"];
        let r = reconcile(&f, &cfg(0), false, "startup").await.unwrap();
        assert_eq!(r.failed.len(), 1);
        assert_eq!(r.failed[0].reason, "driver said no");
        assert_eq!(r.started.len(), 1);
        assert_eq!(r.started[0].vm, "b");
        assert_eq!(r.trigger, "startup");
    }

    #[tokio::test]
    async fn stale_stored_running_state_is_judged_on_actual_state() {
        let mut stale = vm("a", &["autostart"]);
        stale.state = VMState::Running; // store says running, backend says stopped
        let f = Fake::new(vec![stale]);
        let r = reconcile(&f, &cfg(0), false, "startup").await.unwrap();
        assert_eq!(r.started.len(), 1);
    }

    #[tokio::test]
    async fn paused_and_stopping_are_skipped() {
        let mut f = Fake::new(vec![vm("p", &["autostart"]), vm("s", &["autostart"])]);
        f.actual.insert("p".into(), VMState::Paused);
        f.actual.insert("s".into(), VMState::Stopping);
        let r = reconcile(&f, &cfg(0), false, "manual").await.unwrap();
        assert!(f.log().is_empty());
        assert_eq!(r.skipped[0].code, "paused");
        assert_eq!(r.skipped[1].code, "stopping");
    }

    #[tokio::test]
    async fn delay_is_capped_and_shutdown_interrupts() {
        let mut f = Fake::new(vec![
            vm(
                "a",
                &["autostart", "autostart-order:1", "autostart-delay:99999"],
            ),
            vm("b", &["autostart", "autostart-order:2"]),
            vm("c", &["autostart", "autostart-order:3"]),
        ]);
        f.interrupt_pause = true;
        let r = reconcile(&f, &cfg(0), false, "manual").await.unwrap();
        assert_eq!(f.log(), ["start a", "pause 600"]);
        assert_eq!(r.started.len(), 1);
        assert!(r.skipped.iter().all(|s| s.code == "shutdown"));
        assert_eq!(r.skipped.len(), 2);
    }

    #[tokio::test]
    async fn nothing_flagged_is_an_empty_report() {
        let f = Fake::new(vec![vm("a", &[])]);
        let r = reconcile(&f, &cfg(0), false, "manual").await.unwrap();
        assert_eq!(r.summary.candidates, 0);
    }

    #[tokio::test]
    async fn progress_is_reported_per_vm_as_it_happens() {
        let mut f = Fake::new(vec![
            vm("a", &["autostart", "autostart-order:1"]),
            vm("b", &["autostart", "autostart-order:2"]),
            vm("up", &["autostart", "autostart-order:3"]),
        ]);
        f.actual.insert("up".into(), VMState::Running);
        f.fail = vec!["b"];
        let mut seen = Vec::new();
        reconcile_with(&f, &cfg(1), false, "manual", &mut |p| match p {
            Progress::Candidates(c) => seen.push(format!("candidates {}", c.len())),
            Progress::Vm { vm, status, .. } => seen.push(format!("{vm} {status}")),
        })
        .await
        .unwrap();
        assert_eq!(
            seen,
            [
                "candidates 3",
                "a checking",
                "a starting",
                "a started",
                "b checking",
                "b waiting",
                "b starting",
                "b failed",
                "up checking",
                "up skipped",
            ]
        );
    }

    #[test]
    fn run_history_is_bounded_and_records_progress() {
        let first = begin_run("manual");
        record_progress(&first, Progress::Candidates(vec![("a".into(), 1)]));
        record_progress(
            &first,
            Progress::Vm {
                vm: "a".into(),
                status: "started",
                code: None,
                reason: None,
            },
        );
        finish_run(&first, &Err("boom".into()));
        let rec = runs().iter().find(|r| r.id == first).cloned().unwrap();
        assert_eq!(rec.state, "failed");
        assert_eq!(rec.summary().started, 1);
        for _ in 0..(MAX_RUNS + 3) {
            begin_run("startup");
        }
        let r = runs();
        assert_eq!(r.len(), MAX_RUNS);
        assert!(r.iter().all(|x| x.id != first));
    }
}
