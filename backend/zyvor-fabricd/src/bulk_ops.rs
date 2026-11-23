// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: LicenseRef-Zyvor-Production-2.0

//! Bulk VM operations by selector, with a dry-run plan and a plan hash.
//!
//! `POST /api/vms/bulk` selects VMs (all given criteria must match: tags,
//! labels, exact names, name prefix; an empty selector is rejected) and plans
//! one action for each: `start`, `stop`, `snapshot`, `tag-add` or
//! `tag-remove`. The default is a dry run. To apply, repeat the same request
//! with `dry_run: false` and the `plan_hash` of the dry run; the plan is
//! recomputed first and the request is refused with 409 when it differs.
//!
//! Safety
//! * At most `max_vms` (default 10, hard limit 50) VMs may be acted on; a
//!   larger plan is reported as `over_cap` and cannot be applied.
//! * VMs are processed one at a time in name order, through the same guarded
//!   code as the single-VM routes (per-VM lock, start preflight, snapshot
//!   validation). The first failure stops the run (remaining VMs are
//!   `not_attempted`) unless `continue_on_error` is set.
//! * Each VM gets an audit entry (`BULK_<ACTION>`) and the run gets a
//!   `BULK_VMS` entry carrying the plan hash.
//! * One bulk run at a time per daemon; a second request gets 409.
//! * Tenant-scoped tokens are refused.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::{Arc, Mutex};

use axum::{extract::State, http::StatusCode, response::IntoResponse, Json};
use security::RequireWrite;
use serde::{Deserialize, Serialize};
use serde_json::json;
use sha2::{Digest, Sha256};
use vm_model::{VMState, VM};

use crate::api_error::json_error;
use crate::server::AppState;

pub const DEFAULT_MAX_VMS: usize = 10;
pub const HARD_MAX_VMS: usize = 50;
const MAX_MATCHED: usize = 500;
const MAX_CRITERIA: usize = 32;
const MAX_TEXT: usize = 128;

/// Daemons (AppState instances, keyed by address) with a bulk run in flight.
/// Keyed per instance so independent daemons in one process (tests) do not
/// block each other.
static RUNNING: Mutex<BTreeSet<usize>> = Mutex::new(BTreeSet::new());

struct RunGuard(usize);
impl RunGuard {
    fn acquire(key: usize) -> Option<Self> {
        let mut set = RUNNING.lock().unwrap_or_else(|e| e.into_inner());
        set.insert(key).then(|| RunGuard(key))
    }
}
impl Drop for RunGuard {
    fn drop(&mut self) {
        RUNNING
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .remove(&self.0);
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Action {
    Start,
    Stop,
    Snapshot,
    TagAdd,
    TagRemove,
}

impl Action {
    fn audit_name(self) -> &'static str {
        match self {
            Action::Start => "BULK_START",
            Action::Stop => "BULK_STOP",
            Action::Snapshot => "BULK_SNAPSHOT",
            Action::TagAdd => "BULK_TAG_ADD",
            Action::TagRemove => "BULK_TAG_REMOVE",
        }
    }
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Selector {
    /// VM must carry every listed tag.
    #[serde(default)]
    pub tags: Vec<String>,
    /// VM must carry every `key: value` label.
    #[serde(default)]
    pub labels: BTreeMap<String, String>,
    /// VM name must be one of these.
    #[serde(default)]
    pub names: Vec<String>,
    #[serde(default)]
    pub name_prefix: Option<String>,
}

impl Selector {
    fn is_empty(&self) -> bool {
        self.tags.is_empty()
            && self.labels.is_empty()
            && self.names.is_empty()
            && self.name_prefix.as_deref().is_none_or(str::is_empty)
    }

    pub fn validate(&self) -> Result<(), String> {
        if self.is_empty() {
            return Err(
                "selector must set at least one of tags, labels, names, name_prefix".into(),
            );
        }
        if self.tags.len() > MAX_CRITERIA
            || self.labels.len() > MAX_CRITERIA
            || self.names.len() > MAX_CRITERIA * 4
        {
            return Err("selector has too many criteria".into());
        }
        let long = |s: &str| s.is_empty() || s.chars().count() > MAX_TEXT;
        if self.tags.iter().any(|t| long(t))
            || self.names.iter().any(|t| long(t))
            || self
                .labels
                .iter()
                .any(|(k, v)| long(k) || v.chars().count() > MAX_TEXT)
            || self
                .name_prefix
                .as_deref()
                .is_some_and(|p| p.chars().count() > MAX_TEXT)
        {
            return Err(format!("selector values must be 1-{MAX_TEXT} characters"));
        }
        Ok(())
    }

    pub fn matches(&self, vm: &VM) -> bool {
        if !self.names.is_empty() && !self.names.contains(&vm.name) {
            return false;
        }
        if let Some(p) = self.name_prefix.as_deref().filter(|p| !p.is_empty()) {
            if !vm.name.starts_with(p) {
                return false;
            }
        }
        let tags = vm.tags.as_deref().unwrap_or(&[]);
        if !self.tags.iter().all(|t| tags.contains(t)) {
            return false;
        }
        let labels = vm.labels.as_ref();
        self.labels
            .iter()
            .all(|(k, v)| labels.and_then(|l| l.get(k)).is_some_and(|x| x == v))
    }
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BulkRequest {
    pub selector: Selector,
    pub action: Action,
    /// Required for `tag-add` and `tag-remove`.
    #[serde(default)]
    pub tag: Option<String>,
    /// Snapshot name prefix; a UTC timestamp is appended. Default `bulk`.
    #[serde(default)]
    pub snapshot_prefix: Option<String>,
    #[serde(default = "yes")]
    pub dry_run: bool,
    #[serde(default)]
    pub plan_hash: Option<String>,
    #[serde(default)]
    pub max_vms: Option<usize>,
    #[serde(default)]
    pub continue_on_error: bool,
}

fn yes() -> bool {
    true
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct PlanItem {
    pub vm: String,
    pub state: String,
    /// The action that would run, or `skip`.
    pub will: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq, Default)]
pub struct PlanSummary {
    pub matched: usize,
    pub actionable: usize,
    pub skipped: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct BulkPlan {
    pub action: Action,
    pub items: Vec<PlanItem>,
    pub summary: PlanSummary,
    pub max_vms: usize,
    pub over_cap: bool,
    pub plan_hash: String,
}

fn is_active(s: &VMState) -> bool {
    matches!(s, VMState::Running | VMState::Starting | VMState::Paused)
}

/// Decide what the action would do to one VM. `blockers` are start preflight
/// blocker ids (only consulted for `start`).
pub fn plan_item(
    action: Action,
    vm: &VM,
    tag: Option<&str>,
    blockers: &[&'static str],
) -> PlanItem {
    let skip = |r: &str| PlanItem {
        vm: vm.name.clone(),
        state: format!("{:?}", vm.state).to_lowercase(),
        will: "skip",
        reason: Some(r.to_string()),
    };
    let go = |w: &'static str| PlanItem {
        vm: vm.name.clone(),
        state: format!("{:?}", vm.state).to_lowercase(),
        will: w,
        reason: None,
    };
    let has_tag = tag.is_some_and(|t| vm.tags.as_deref().unwrap_or(&[]).iter().any(|x| x == t));
    match action {
        Action::Start => {
            if matches!(vm.state, VMState::Stopping) {
                skip("state_transitioning")
            } else if is_active(&vm.state) {
                skip("already_running")
            } else if !blockers.is_empty() {
                skip(&format!("preflight_blocked: {}", blockers.join(", ")))
            } else {
                go("start")
            }
        }
        Action::Stop => match vm.state {
            VMState::Running | VMState::Paused => go("stop"),
            VMState::Starting | VMState::Stopping => skip("state_transitioning"),
            _ => skip("not_running"),
        },
        Action::Snapshot => go("snapshot"),
        Action::TagAdd => {
            if has_tag {
                skip("already_tagged")
            } else {
                go("tag-add")
            }
        }
        Action::TagRemove => {
            if has_tag {
                go("tag-remove")
            } else {
                skip("not_tagged")
            }
        }
    }
}

pub fn build_plan(
    action: Action,
    tag: Option<&str>,
    snapshot_prefix: &str,
    mut items: Vec<PlanItem>,
    max_vms: usize,
) -> BulkPlan {
    items.sort_by(|a, b| a.vm.cmp(&b.vm));
    let actionable = items.iter().filter(|i| i.will != "skip").count();
    let summary = PlanSummary {
        matched: items.len(),
        actionable,
        skipped: items.len() - actionable,
    };
    let canon = json!({
        "action": action, "tag": tag,
        "snapshot_prefix": (action == Action::Snapshot).then_some(snapshot_prefix),
        "items": items,
    });
    let mut h = Sha256::new();
    h.update(serde_json::to_vec(&canon).unwrap_or_default());
    let plan_hash: String = h.finalize().iter().map(|b| format!("{b:02x}")).collect();
    BulkPlan {
        action,
        over_cap: actionable > max_vms,
        items,
        summary,
        max_vms,
        plan_hash,
    }
}

#[derive(Debug, Serialize)]
pub struct VmResult {
    pub vm: String,
    /// `ok`, `failed`, `skipped` or `not_attempted`.
    pub status: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

type ApiErr = (StatusCode, Json<serde_json::Value>);

fn bad(msg: impl Into<String>) -> ApiErr {
    json_error(StatusCode::BAD_REQUEST, msg)
}

async fn make_plan(state: &Arc<AppState>, req: &BulkRequest) -> Result<(BulkPlan, usize), ApiErr> {
    let all = state
        .store
        .list_vms()
        .map_err(|e| json_error(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    let matched: Vec<&VM> = all.iter().filter(|v| req.selector.matches(v)).collect();
    if matched.len() > MAX_MATCHED {
        return Err(bad(format!(
            "selector matches {} VMs; narrow it to at most {MAX_MATCHED}",
            matched.len()
        )));
    }
    let max_vms = req.max_vms.unwrap_or(DEFAULT_MAX_VMS).min(HARD_MAX_VMS);
    let mut items = Vec::with_capacity(matched.len());
    for vm in matched {
        let blockers: Vec<&'static str> = if req.action == Action::Start && !is_active(&vm.state) {
            let opts = crate::routes::stored_start_options(vm);
            let caps = crate::preflight::BackendCaps::for_backend(state.driver.backend_name());
            let host = crate::preflight::gather_host_inputs(state, vm, &opts).await;
            crate::preflight::check(vm, &opts, &caps, &host)
                .into_iter()
                .filter(|f| f.severity == crate::preflight::Severity::Blocker)
                .map(|f| f.id)
                .collect()
        } else {
            Vec::new()
        };
        items.push(plan_item(req.action, vm, req.tag.as_deref(), &blockers));
    }
    let prefix = req.snapshot_prefix.as_deref().unwrap_or("bulk");
    Ok((
        build_plan(req.action, req.tag.as_deref(), prefix, items, max_vms),
        max_vms,
    ))
}

async fn run_one(
    state: &Arc<AppState>,
    user: &str,
    action: Action,
    vm: &str,
    tag: Option<&str>,
    snap_name: &str,
) -> Result<(), String> {
    match action {
        Action::Start => {
            let _lock = state.vm_lock(vm).lock_owned().await;
            let mut cur = state
                .store
                .get_vm(vm)
                .map_err(|e| e.to_string())?
                .ok_or("VM not found")?;
            if is_active(&cur.state) || cur.state == VMState::Stopping {
                return Err(format!("VM is already {:?}", cur.state));
            }
            cur.state = VMState::Starting;
            state
                .store
                .save_vm(&cur)
                .map_err(|e| format!("Failed to save VM state: {e}"))?;
            crate::routes::run_start_task(state, vm, None).await
        }
        Action::Stop => crate::routes::stop_core(state, user, vm).await,
        Action::Snapshot => {
            let req = crate::api::snapshots::CreateSnapshotRequest {
                name: snap_name.to_string(),
                description: Some("Created by bulk operation".into()),
                snapshot_type: crate::api::snapshots::SnapshotType::Disk,
                tags: vec![],
            };
            let resp =
                crate::api::snapshots::create_snapshot_core(state, vm.to_string(), req).await;
            if resp.status() == StatusCode::CREATED {
                Ok(())
            } else {
                let status = resp.status();
                let body = axum::body::to_bytes(resp.into_body(), 16 * 1024)
                    .await
                    .unwrap_or_default();
                Err(format!("{status}: {}", String::from_utf8_lossy(&body)))
            }
        }
        Action::TagAdd | Action::TagRemove => {
            let tag = tag.ok_or("tag is required")?;
            let _lock = state.vm_lock(vm).lock_owned().await;
            let mut cur = state
                .store
                .get_vm(vm)
                .map_err(|e| e.to_string())?
                .ok_or("VM not found")?;
            let tags = cur.tags.get_or_insert_with(Vec::new);
            if action == Action::TagAdd {
                if !tags.iter().any(|t| t == tag) {
                    tags.push(tag.to_string());
                }
            } else {
                tags.retain(|t| t != tag);
            }
            state.store.save_vm(&cur).map_err(|e| e.to_string())
        }
    }
}

/// POST /api/vms/bulk
pub async fn bulk(
    RequireWrite(claims): RequireWrite,
    State(state): State<Arc<AppState>>,
    Json(req): Json<BulkRequest>,
) -> impl IntoResponse {
    if claims.tenant.is_some() {
        return json_error(
            StatusCode::FORBIDDEN,
            "Bulk operations are not available to tenant-scoped tokens",
        )
        .into_response();
    }
    if let Err(e) = req.selector.validate() {
        return bad(e).into_response();
    }
    if matches!(req.action, Action::TagAdd | Action::TagRemove) {
        match req.tag.as_deref() {
            None => return bad("tag is required for tag-add and tag-remove").into_response(),
            Some(t) if t.is_empty() || t.chars().count() > 63 => {
                return bad("Tag must be between 1 and 63 characters").into_response()
            }
            _ => {}
        }
    }
    if let Some(p) = req.snapshot_prefix.as_deref() {
        if p.len() > 40 {
            return bad("snapshot_prefix must be at most 40 characters").into_response();
        }
        if let Err((_, m)) = crate::validation::validate_snapshot_name(p) {
            return bad(m).into_response();
        }
    }
    if req.max_vms == Some(0) {
        return bad("max_vms must be at least 1").into_response();
    }
    if let Some(h) = req.plan_hash.as_deref() {
        if h.len() != 64 || !h.chars().all(|c| c.is_ascii_hexdigit()) {
            return bad("plan_hash must be 64 hex characters").into_response();
        }
    }

    let (plan, _) = match make_plan(&state, &req).await {
        Ok(p) => p,
        Err(e) => return e.into_response(),
    };
    if req.dry_run {
        return Json(json!({
            "dry_run": true,
            "can_apply": !plan.over_cap && plan.summary.actionable > 0,
            "plan": plan,
        }))
        .into_response();
    }

    // Apply
    let Some(supplied) = req.plan_hash.as_deref() else {
        return bad("plan_hash from a dry run is required when dry_run is false").into_response();
    };
    if !supplied.eq_ignore_ascii_case(&plan.plan_hash) {
        return (
            StatusCode::CONFLICT,
            Json(json!({
                "error": "The plan changed since the dry run; review the new plan",
                "plan": plan,
            })),
        )
            .into_response();
    }
    if plan.over_cap {
        return bad(format!(
            "plan acts on {} VMs, above max_vms={}; narrow the selector or raise max_vms (hard limit {HARD_MAX_VMS})",
            plan.summary.actionable, plan.max_vms
        ))
        .into_response();
    }
    let Some(_guard) = RunGuard::acquire(Arc::as_ptr(&state) as usize) else {
        return json_error(
            StatusCode::CONFLICT,
            "Another bulk operation is already running",
        )
        .into_response();
    };

    let stamp = chrono::Utc::now().format("%Y%m%dT%H%M%SZ").to_string();
    let snap_name = format!(
        "{}-{stamp}",
        req.snapshot_prefix.as_deref().unwrap_or("bulk")
    );
    let mut results = Vec::new();
    let mut aborted = false;
    for item in &plan.items {
        if item.will == "skip" {
            results.push(VmResult {
                vm: item.vm.clone(),
                status: "skipped",
                error: item.reason.clone(),
            });
            continue;
        }
        if aborted {
            results.push(VmResult {
                vm: item.vm.clone(),
                status: "not_attempted",
                error: None,
            });
            continue;
        }
        let out = run_one(
            &state,
            &claims.sub,
            req.action,
            &item.vm,
            req.tag.as_deref(),
            &snap_name,
        )
        .await;
        crate::routes::audit(
            &state,
            &claims.sub,
            req.action.audit_name(),
            &format!("vm/{}", item.vm),
            if out.is_ok() { "SUCCESS" } else { "FAILED" },
        );
        match out {
            Ok(()) => results.push(VmResult {
                vm: item.vm.clone(),
                status: "ok",
                error: None,
            }),
            Err(e) => {
                if !req.continue_on_error {
                    aborted = true;
                }
                results.push(VmResult {
                    vm: item.vm.clone(),
                    status: "failed",
                    error: Some(e),
                });
            }
        }
    }
    let count = |s: &str| results.iter().filter(|r| r.status == s).count();
    let (ok, failed) = (count("ok"), count("failed"));
    crate::routes::audit(
        &state,
        &claims.sub,
        "BULK_VMS",
        &format!("vms/bulk/{}", &plan.plan_hash[..16]),
        if failed == 0 { "SUCCESS" } else { "FAILED" },
    );
    let names: BTreeSet<&str> = results.iter().map(|r| r.vm.as_str()).collect();
    Json(json!({
        "dry_run": false,
        "plan_hash": plan.plan_hash,
        "action": req.action,
        "summary": {
            "ok": ok, "failed": failed, "skipped": count("skipped"),
            "not_attempted": count("not_attempted"), "vms": names.len(),
        },
        "results": results,
    }))
    .into_response()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn vm(name: &str, state: VMState, tags: &[&str], labels: &[(&str, &str)]) -> VM {
        let mut v = VM::new(name.into(), "i.qcow2".into(), 1, 512);
        v.state = state;
        v.tags = Some(tags.iter().map(|s| s.to_string()).collect());
        v.labels = Some(
            labels
                .iter()
                .map(|(k, v)| (k.to_string(), v.to_string()))
                .collect(),
        );
        v
    }

    fn sel(tags: &[&str]) -> Selector {
        Selector {
            tags: tags.iter().map(|s| s.to_string()).collect(),
            ..Default::default()
        }
    }

    #[test]
    fn empty_selector_is_rejected() {
        assert!(Selector::default().validate().is_err());
        let mut s = Selector::default();
        s.name_prefix = Some(String::new());
        assert!(s.validate().is_err());
        assert!(sel(&["a"]).validate().is_ok());
    }

    #[test]
    fn hostile_selectors_are_rejected() {
        let mut s = sel(&["a"]);
        s.tags = (0..40).map(|i| format!("t{i}")).collect();
        assert!(s.validate().is_err());
        let s = sel(&[&"x".repeat(200)]);
        assert!(s.validate().is_err());
        let mut s = Selector::default();
        s.labels.insert("".into(), "v".into());
        assert!(s.validate().is_err());
        let mut s = Selector::default();
        s.names = vec!["".into()];
        assert!(s.validate().is_err());
    }

    #[test]
    fn criteria_are_anded() {
        let v = vm(
            "web-1",
            VMState::Running,
            &["prod", "web"],
            &[("tier", "a")],
        );
        assert!(sel(&["prod"]).matches(&v));
        assert!(sel(&["prod", "web"]).matches(&v));
        assert!(!sel(&["prod", "db"]).matches(&v));
        let mut s = sel(&["prod"]);
        s.labels.insert("tier".into(), "a".into());
        assert!(s.matches(&v));
        s.labels.insert("tier".into(), "b".into());
        assert!(!s.matches(&v));
        let mut s = Selector::default();
        s.name_prefix = Some("web".into());
        assert!(s.matches(&v));
        s.names = vec!["other".into()];
        assert!(!s.matches(&v));
        // a VM without tags or labels never matches a tag/label selector
        let bare = VM::new("b".into(), "i".into(), 1, 1);
        assert!(!sel(&["prod"]).matches(&bare));
    }

    #[test]
    fn item_rules_per_action() {
        let run = vm("r", VMState::Running, &["t"], &[]);
        let stop = vm("s", VMState::Stopped, &[], &[]);
        let stopping = vm("x", VMState::Stopping, &[], &[]);
        assert_eq!(
            plan_item(Action::Start, &run, None, &[]).reason.as_deref(),
            Some("already_running")
        );
        assert_eq!(plan_item(Action::Start, &stop, None, &[]).will, "start");
        assert!(plan_item(Action::Start, &stop, None, &["vm.image.missing"])
            .reason
            .unwrap()
            .contains("vm.image.missing"));
        assert_eq!(
            plan_item(Action::Start, &stopping, None, &[])
                .reason
                .as_deref(),
            Some("state_transitioning")
        );
        assert_eq!(plan_item(Action::Stop, &run, None, &[]).will, "stop");
        assert_eq!(
            plan_item(Action::Stop, &stop, None, &[]).reason.as_deref(),
            Some("not_running")
        );
        assert_eq!(
            plan_item(Action::Snapshot, &stop, None, &[]).will,
            "snapshot"
        );
        assert_eq!(
            plan_item(Action::TagAdd, &run, Some("t"), &[])
                .reason
                .as_deref(),
            Some("already_tagged")
        );
        assert_eq!(
            plan_item(Action::TagAdd, &stop, Some("t"), &[]).will,
            "tag-add"
        );
        assert_eq!(
            plan_item(Action::TagRemove, &run, Some("t"), &[]).will,
            "tag-remove"
        );
        assert_eq!(
            plan_item(Action::TagRemove, &stop, Some("t"), &[])
                .reason
                .as_deref(),
            Some("not_tagged")
        );
    }

    #[test]
    fn plan_is_sorted_hashed_capped() {
        let a = vm("b", VMState::Stopped, &[], &[]);
        let b = vm("a", VMState::Stopped, &[], &[]);
        let items = |v: &[&VM]| -> Vec<PlanItem> {
            v.iter()
                .map(|x| plan_item(Action::Start, x, None, &[]))
                .collect()
        };
        let p1 = build_plan(Action::Start, None, "bulk", items(&[&a, &b]), 10);
        let p2 = build_plan(Action::Start, None, "bulk", items(&[&b, &a]), 10);
        assert_eq!(p1.items[0].vm, "a");
        assert_eq!(p1.plan_hash, p2.plan_hash);
        assert_eq!(p1.plan_hash.len(), 64);
        assert_eq!(p1.summary.actionable, 2);
        assert!(!p1.over_cap);
        let capped = build_plan(Action::Start, None, "bulk", items(&[&a, &b]), 1);
        assert!(capped.over_cap);
        // a different action, tag or state changes the hash
        let stop = build_plan(Action::Stop, None, "bulk", items(&[&a, &b]), 10);
        assert_ne!(p1.plan_hash, stop.plan_hash);
        let run = vm("a", VMState::Running, &[], &[]);
        let p3 = build_plan(Action::Start, None, "bulk", items(&[&a, &run]), 10);
        assert_ne!(p1.plan_hash, p3.plan_hash);
        assert_eq!(p3.summary.skipped, 1);
    }

    #[test]
    fn snapshot_prefix_only_affects_snapshot_hash() {
        let a = vm("a", VMState::Stopped, &[], &[]);
        let it = |act| vec![plan_item(act, &a, None, &[])];
        assert_eq!(
            build_plan(Action::Start, None, "x", it(Action::Start), 5).plan_hash,
            build_plan(Action::Start, None, "y", it(Action::Start), 5).plan_hash
        );
        assert_ne!(
            build_plan(Action::Snapshot, None, "x", it(Action::Snapshot), 5).plan_hash,
            build_plan(Action::Snapshot, None, "y", it(Action::Snapshot), 5).plan_hash
        );
    }

    #[test]
    fn run_guard_is_exclusive() {
        let g = RunGuard::acquire(1).expect("free");
        assert!(RunGuard::acquire(1).is_none());
        // another instance is independent
        assert!(RunGuard::acquire(2).is_some());
        drop(g);
        assert!(RunGuard::acquire(1).is_some());
    }
}
