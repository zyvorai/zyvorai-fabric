// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: LicenseRef-Zyvor-Production-2.0

//! Snapshot health scoreboard.
//!
//! `GET /api/snapshots/health` is a read-only fleet view of backup posture:
//! for every VM, how old its newest snapshot is against its recovery-point
//! objective (RPO), whether any snapshot chain is broken, and how much space
//! the snapshots are using and are projected to use.
//!
//! Rules
//! * RPO target: the policy interval (`interval_secs`), or for a cron policy
//!   the gap between the next two scheduled runs. A VM with no enabled policy
//!   is judged against `default_rpo_secs` (query parameter, default 86400,
//!   minimum 60) and flagged.
//! * Status by newest-snapshot age (any snapshot counts): `healthy` when age
//!   <= target, `at_risk` when <= 2x target, `breached` beyond that,
//!   `no_snapshots` when there is none. A snapshot dated in the future counts
//!   as age 0.
//! * Orphaned: a snapshot whose `parent_id` names a snapshot that no longer
//!   exists (a broken chain).
//! * Growth: bytes of snapshots created in the last 7 days divided by 7.
//!   `projected_bytes_30d` is current usage plus 30 days of that growth,
//!   capped by what `keep_last` retention allows when the policy sets it.
//!   Fabric does not know free disk space, so this is a forecast of snapshot
//!   usage, not of the disk filling.
//! * Tenant-scoped tokens see only their tenant's VMs.

use std::collections::BTreeSet;
use std::sync::Arc;

use axum::{
    extract::{Query, State},
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};

use crate::api::snapshots::VMSnapshot;
use crate::api_error::json_error;
use crate::server::AppState;
use crate::snapshot_policy::{CronExpr, SnapshotPolicy};
use security::RequireRead;

pub const DEFAULT_RPO_SECS: u64 = 86_400;
const MIN_RPO_SECS: u64 = 60;
const GROWTH_WINDOW_DAYS: i64 = 7;
const FORECAST_DAYS: u64 = 30;

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct VmHealth {
    pub vm: String,
    /// `healthy`, `at_risk`, `breached` or `no_snapshots`.
    pub status: &'static str,
    pub rpo_target_secs: u64,
    /// `policy` or `default`.
    pub rpo_source: &'static str,
    pub newest_snapshot_at: Option<DateTime<Utc>>,
    pub age_secs: Option<u64>,
    pub snapshot_count: usize,
    pub total_bytes: u64,
    pub orphaned: Vec<String>,
    pub has_policy: bool,
    pub policy_enabled: bool,
    pub growth_bytes_per_day: u64,
    pub projected_bytes_30d: u64,
    pub findings: Vec<String>,
}

fn rank(status: &str) -> u8 {
    match status {
        "breached" => 0,
        "no_snapshots" => 1,
        "at_risk" => 2,
        _ => 3,
    }
}

/// RPO target in seconds for an enabled policy, if it can be determined.
fn policy_rpo(p: &SnapshotPolicy, now: DateTime<Utc>) -> Option<u64> {
    if !p.spec.enabled {
        return None;
    }
    if let Some(i) = p.spec.interval_secs {
        return Some(i);
    }
    let c = CronExpr::parse(p.spec.cron.as_deref()?).ok()?;
    let first = c.next_after(now)?;
    let second = c.next_after(first)?;
    u64::try_from((second - first).num_seconds()).ok()
}

pub fn assess(
    vm: &str,
    snaps: &[VMSnapshot],
    policy: Option<&SnapshotPolicy>,
    now: DateTime<Utc>,
    default_rpo_secs: u64,
) -> VmHealth {
    let mut findings = Vec::new();
    let (rpo, source) = match policy.and_then(|p| policy_rpo(p, now)) {
        Some(r) => (r.max(1), "policy"),
        None => (default_rpo_secs, "default"),
    };
    match policy {
        None => findings.push("no snapshot policy".to_string()),
        Some(p) if !p.spec.enabled => findings.push("snapshot policy is disabled".to_string()),
        Some(p) => {
            if let Some(r) = p.last_result.as_deref().filter(|r| *r != "ok") {
                findings.push(format!("last policy run reported: {r}"));
            }
        }
    }

    let newest = snaps.iter().map(|s| s.created).max();
    let age_secs = newest.map(|t| (now - t).num_seconds().max(0) as u64);
    let status = match age_secs {
        None => "no_snapshots",
        Some(a) if a <= rpo => "healthy",
        Some(a) if a <= rpo.saturating_mul(2) => "at_risk",
        Some(_) => "breached",
    };
    match status {
        "no_snapshots" => findings.push("VM has no snapshots".to_string()),
        "at_risk" | "breached" => findings.push(format!(
            "newest snapshot is {}s old, target is {rpo}s",
            age_secs.unwrap_or(0)
        )),
        _ => {}
    }

    let ids: BTreeSet<&str> = snaps.iter().map(|s| s.id.as_str()).collect();
    let mut orphaned: Vec<String> = snaps
        .iter()
        .filter(|s| s.parent_id.as_deref().is_some_and(|p| !ids.contains(p)))
        .map(|s| s.id.clone())
        .collect();
    orphaned.sort();
    if !orphaned.is_empty() {
        findings.push(format!(
            "{} snapshot(s) have a missing parent (broken chain)",
            orphaned.len()
        ));
    }

    let total_bytes: u64 = snaps.iter().map(|s| s.size_bytes).sum();
    let window_start = now - Duration::days(GROWTH_WINDOW_DAYS);
    let recent: u64 = snaps
        .iter()
        .filter(|s| s.created > window_start)
        .map(|s| s.size_bytes)
        .sum();
    let per_day = recent / GROWTH_WINDOW_DAYS as u64;
    let mut projected = total_bytes.saturating_add(per_day.saturating_mul(FORECAST_DAYS));
    if let Some(p) = policy.filter(|p| p.spec.enabled) {
        if let Some(keep) = p.spec.keep_last {
            let prefix = format!("{}-", p.spec.name_prefix);
            let managed: Vec<&VMSnapshot> = snaps
                .iter()
                .filter(|s| s.name.starts_with(&prefix))
                .collect();
            if !managed.is_empty() {
                let managed_bytes: u64 = managed.iter().map(|s| s.size_bytes).sum();
                let avg = managed_bytes / managed.len() as u64;
                let cap = (total_bytes - managed_bytes).saturating_add(avg * u64::from(keep));
                projected = projected.min(cap);
            }
        }
    }

    VmHealth {
        vm: vm.to_string(),
        status,
        rpo_target_secs: rpo,
        rpo_source: source,
        newest_snapshot_at: newest,
        age_secs,
        snapshot_count: snaps.len(),
        total_bytes,
        orphaned,
        has_policy: policy.is_some(),
        policy_enabled: policy.is_some_and(|p| p.spec.enabled),
        growth_bytes_per_day: per_day,
        projected_bytes_30d: projected,
        findings,
    }
}

#[derive(Debug, Clone, Serialize, PartialEq, Default)]
pub struct HealthSummary {
    pub vms: usize,
    pub healthy: usize,
    pub at_risk: usize,
    pub breached: usize,
    pub no_snapshots: usize,
    pub without_policy: usize,
    pub orphaned_snapshots: usize,
    pub total_bytes: u64,
    pub projected_bytes_30d: u64,
}

pub fn summarize(vms: &[VmHealth]) -> HealthSummary {
    let mut s = HealthSummary {
        vms: vms.len(),
        ..Default::default()
    };
    for v in vms {
        match v.status {
            "healthy" => s.healthy += 1,
            "at_risk" => s.at_risk += 1,
            "breached" => s.breached += 1,
            _ => s.no_snapshots += 1,
        }
        if !v.policy_enabled {
            s.without_policy += 1;
        }
        s.orphaned_snapshots += v.orphaned.len();
        s.total_bytes = s.total_bytes.saturating_add(v.total_bytes);
        s.projected_bytes_30d = s.projected_bytes_30d.saturating_add(v.projected_bytes_30d);
    }
    s
}

/// Worst first: breached, no snapshots, at risk, healthy; then by VM name.
pub fn sort_worst_first(v: &mut [VmHealth]) {
    v.sort_by(|a, b| {
        rank(a.status)
            .cmp(&rank(b.status))
            .then_with(|| a.vm.cmp(&b.vm))
    });
}

#[derive(Debug, Serialize)]
pub struct HealthReport {
    pub generated_at: DateTime<Utc>,
    pub default_rpo_secs: u64,
    pub summary: HealthSummary,
    pub vms: Vec<VmHealth>,
}

#[derive(Debug, Deserialize)]
pub struct HealthQuery {
    #[serde(default)]
    pub default_rpo_secs: Option<u64>,
}

/// GET /api/snapshots/health
pub async fn snapshot_health(
    RequireRead(claims): RequireRead,
    State(state): State<Arc<AppState>>,
    Query(q): Query<HealthQuery>,
) -> impl IntoResponse {
    let default_rpo = q.default_rpo_secs.unwrap_or(DEFAULT_RPO_SECS);
    if default_rpo < MIN_RPO_SECS {
        return json_error(
            StatusCode::BAD_REQUEST,
            "default_rpo_secs must be at least 60",
        )
        .into_response();
    }
    let vms = match state.store.list_vms() {
        Ok(v) => v,
        Err(e) => {
            return json_error(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response()
        }
    };
    let now = Utc::now();
    let mut out = Vec::new();
    for vm in vms {
        if let Some(t) = &claims.tenant {
            if crate::tenant_scope::vm_tenant(&vm).as_deref() != Some(t.as_str()) {
                continue;
            }
        }
        let snaps: Vec<VMSnapshot> = state
            .store
            .list_entities(&format!("snapshots_{}", vm.name))
            .unwrap_or_default();
        let policy = state
            .store
            .get_entity::<SnapshotPolicy>("snapshot_policies", &vm.name)
            .ok()
            .flatten();
        out.push(assess(&vm.name, &snaps, policy.as_ref(), now, default_rpo));
    }
    sort_worst_first(&mut out);
    Json(HealthReport {
        generated_at: now,
        default_rpo_secs: default_rpo,
        summary: summarize(&out),
        vms: out,
    })
    .into_response()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::snapshots::SnapshotType;
    use crate::snapshot_policy::SnapshotPolicyInput;
    use chrono::TimeZone;

    fn now() -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 6, 1, 12, 0, 0).unwrap()
    }

    fn snap(id: &str, name: &str, age_h: i64, parent: Option<&str>, bytes: u64) -> VMSnapshot {
        VMSnapshot {
            id: id.into(),
            vm_name: "vm".into(),
            name: name.into(),
            description: None,
            snapshot_type: SnapshotType::Disk,
            parent_id: parent.map(String::from),
            size_bytes: bytes,
            created: now() - Duration::hours(age_h),
            tags: vec![],
        }
    }

    fn policy(interval: Option<u64>, cron: Option<&str>, keep: Option<u32>) -> SnapshotPolicy {
        SnapshotPolicy {
            vm_name: "vm".into(),
            spec: SnapshotPolicyInput {
                enabled: true,
                interval_secs: interval,
                cron: cron.map(String::from),
                keep_last: keep,
                max_age_secs: None,
                name_prefix: "auto".into(),
                snapshot_type: SnapshotType::Disk,
            },
            updated_at: now() - Duration::days(30),
            last_run: None,
            last_result: None,
        }
    }

    #[test]
    fn status_thresholds_follow_policy_interval() {
        let p = policy(Some(3600), None, None);
        let h = |age| {
            assess(
                "vm",
                &[snap("a", "x", age, None, 1)],
                Some(&p),
                now(),
                86400,
            )
            .status
        };
        assert_eq!(h(0), "healthy");
        assert_eq!(h(1), "healthy"); // exactly the target
        assert_eq!(h(2), "at_risk"); // exactly 2x
        assert_eq!(h(3), "breached");
    }

    #[test]
    fn no_snapshots_and_default_target_are_flagged() {
        let v = assess("vm", &[], None, now(), 7200);
        assert_eq!(v.status, "no_snapshots");
        assert_eq!(v.rpo_source, "default");
        assert_eq!(v.rpo_target_secs, 7200);
        assert!(v.findings.iter().any(|f| f.contains("no snapshot policy")));
        assert!(v.findings.iter().any(|f| f.contains("no snapshots")));
    }

    #[test]
    fn cron_policy_uses_gap_between_runs() {
        let p = policy(None, Some("0 3 * * *"), None);
        let v = assess("vm", &[snap("a", "x", 1, None, 1)], Some(&p), now(), 60);
        assert_eq!(v.rpo_target_secs, 86_400);
        assert_eq!(v.rpo_source, "policy");
    }

    #[test]
    fn disabled_policy_falls_back_to_default() {
        let mut p = policy(Some(3600), None, None);
        p.spec.enabled = false;
        let v = assess("vm", &[snap("a", "x", 1, None, 1)], Some(&p), now(), 7200);
        assert_eq!(v.rpo_source, "default");
        assert!(v.findings.iter().any(|f| f.contains("disabled")));
        assert!(!v.policy_enabled && v.has_policy);
    }

    #[test]
    fn failed_last_run_is_reported() {
        let mut p = policy(Some(3600), None, None);
        p.last_result = Some("1 delete failed".into());
        let v = assess("vm", &[snap("a", "x", 0, None, 1)], Some(&p), now(), 60);
        assert!(v.findings.iter().any(|f| f.contains("1 delete failed")));
    }

    #[test]
    fn orphaned_chain_detected() {
        let snaps = [
            snap("a", "root", 5, None, 1),
            snap("b", "child", 4, Some("a"), 1),
            snap("c", "lost", 3, Some("gone"), 1),
        ];
        let v = assess("vm", &snaps, None, now(), 86400);
        assert_eq!(v.orphaned, vec!["c"]);
        assert!(v.findings.iter().any(|f| f.contains("broken chain")));
    }

    #[test]
    fn future_dated_snapshot_has_age_zero() {
        let v = assess("vm", &[snap("a", "x", -5, None, 1)], None, now(), 60);
        assert_eq!(v.age_secs, Some(0));
        assert_eq!(v.status, "healthy");
    }

    #[test]
    fn growth_and_retention_capped_forecast() {
        // 70 bytes created in the last week -> 10/day -> +300 over 30 days
        let snaps: Vec<_> = (0..7)
            .map(|i| snap(&format!("s{i}"), &format!("auto-{i}"), 1 + i * 20, None, 10))
            .collect();
        let v = assess("vm", &snaps, None, now(), 86400);
        assert_eq!(v.total_bytes, 70);
        assert_eq!(v.growth_bytes_per_day, 10);
        assert_eq!(v.projected_bytes_30d, 370);
        // keep_last=10 of avg 10 bytes caps managed usage at 100
        let p = policy(Some(3600), None, Some(10));
        let v = assess("vm", &snaps, Some(&p), now(), 86400);
        assert_eq!(v.projected_bytes_30d, 100);
    }

    #[test]
    fn old_snapshots_do_not_count_as_growth() {
        let v = assess(
            "vm",
            &[snap("a", "x", 24 * 20, None, 700)],
            None,
            now(),
            86400,
        );
        assert_eq!(v.growth_bytes_per_day, 0);
        assert_eq!(v.projected_bytes_30d, 700);
    }

    #[test]
    fn summary_and_ordering() {
        let p = policy(Some(3600), None, None);
        let mut vs = vec![
            assess("ok", &[snap("a", "x", 0, None, 5)], Some(&p), now(), 60),
            assess("none", &[], Some(&p), now(), 60),
            assess("late", &[snap("a", "x", 9, None, 5)], Some(&p), now(), 60),
            assess(
                "risk",
                &[snap("a", "x", 2, Some("zz"), 5)],
                Some(&p),
                now(),
                60,
            ),
        ];
        sort_worst_first(&mut vs);
        let names: Vec<&str> = vs.iter().map(|v| v.vm.as_str()).collect();
        assert_eq!(names, vec!["late", "none", "risk", "ok"]);
        let s = summarize(&vs);
        assert_eq!(
            (s.vms, s.healthy, s.at_risk, s.breached, s.no_snapshots),
            (4, 1, 1, 1, 1)
        );
        assert_eq!(s.orphaned_snapshots, 1);
        assert_eq!(s.without_policy, 0);
        assert_eq!(s.total_bytes, 15);
    }
}
