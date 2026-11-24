// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: LicenseRef-Zyvor-Production-2.0

//! Snapshot hygiene report.
//!
//! `GET /api/snapshots/hygiene` is a read-only report of snapshots that have
//! outlived their usefulness. Snapshots are not backups: they grow with every
//! write to the disk, and long chains slow I/O and make deletion risky.
//! VMware's guidance (KB 318825) is to keep a snapshot no longer than 72 hours,
//! to use only 2 to 3 snapshots in a chain and never more than the 32 that
//! are supported. Those are the defaults here and can be overridden by query.
//!
//! Rules
//! * Stale: age (from `created`) above `max_age_hours` (default 72). A future
//!   dated snapshot counts as age 0.
//! * Protected: tagged `keep`. A protected stale snapshot is listed but its
//!   bytes are not counted as reclaimable.
//! * Chain depth: number of snapshots from a snapshot up to its root through
//!   `parent_id` (a missing parent ends the walk and is reported as orphaned;
//!   a cycle is cut). The VM depth is the longest chain. It is flagged above
//!   `max_chain` (default 3) and critical above 32.
//! * Reclaimable bytes: the sum of `size_bytes` of stale, unprotected
//!   snapshots. It is an upper bound: merging a snapshot frees only what the
//!   neighbouring snapshots do not share.
//! * Tenant-scoped tokens see only their tenant's VMs.
//! * Nothing is deleted. Fabric does not have a snapshot-quiesce or merge
//!   estimate, so the report says what to look at, not what a delete frees.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use axum::{
    extract::{Query, State},
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::api::snapshots::VMSnapshot;
use crate::api_error::json_error;
use crate::server::AppState;
use security::RequireRead;

pub const DEFAULT_MAX_AGE_HOURS: u64 = 72;
pub const DEFAULT_MAX_CHAIN: usize = 3;
/// Longest snapshot chain the hypervisor family supports.
pub const HARD_MAX_CHAIN: usize = 32;
const MAX_AGE_HOURS_LIMIT: u64 = 87_600;
const KEEP_TAG: &str = "keep";

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct StaleSnapshot {
    pub id: String,
    pub name: String,
    pub age_hours: u64,
    pub size_bytes: u64,
    pub protected: bool,
    pub chain_depth: usize,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct VmHygiene {
    pub vm: String,
    /// `ok`, `warning` or `critical`.
    pub status: &'static str,
    pub snapshot_count: usize,
    pub total_bytes: u64,
    pub oldest_age_hours: Option<u64>,
    pub max_chain_depth: usize,
    pub orphaned: Vec<String>,
    pub stale: Vec<StaleSnapshot>,
    pub reclaimable_bytes: u64,
    pub findings: Vec<String>,
}

/// Chain length of `id`: itself plus every ancestor present in `by_id`.
fn depth_of(id: &str, by_id: &BTreeMap<&str, &VMSnapshot>) -> usize {
    let mut seen = BTreeSet::new();
    let mut cur = Some(id);
    let mut depth = 0;
    while let Some(c) = cur {
        if !seen.insert(c) {
            break; // cycle
        }
        let Some(s) = by_id.get(c) else { break };
        depth += 1;
        cur = s.parent_id.as_deref();
    }
    depth
}

pub fn assess(
    vm: &str,
    snaps: &[VMSnapshot],
    now: DateTime<Utc>,
    max_age_hours: u64,
    max_chain: usize,
) -> VmHygiene {
    let by_id: BTreeMap<&str, &VMSnapshot> = snaps.iter().map(|s| (s.id.as_str(), s)).collect();
    let age_h = |s: &VMSnapshot| (now - s.created).num_hours().max(0) as u64;

    let mut orphaned: Vec<String> = snaps
        .iter()
        .filter(|s| {
            s.parent_id
                .as_deref()
                .is_some_and(|p| !by_id.contains_key(p))
        })
        .map(|s| s.id.clone())
        .collect();
    orphaned.sort();

    let mut stale: Vec<StaleSnapshot> = snaps
        .iter()
        .filter(|s| age_h(s) > max_age_hours)
        .map(|s| StaleSnapshot {
            id: s.id.clone(),
            name: s.name.clone(),
            age_hours: age_h(s),
            size_bytes: s.size_bytes,
            protected: s.tags.iter().any(|t| t == KEEP_TAG),
            chain_depth: depth_of(&s.id, &by_id),
        })
        .collect();
    stale.sort_by(|a, b| b.age_hours.cmp(&a.age_hours).then(a.id.cmp(&b.id)));

    let max_depth = snaps
        .iter()
        .map(|s| depth_of(&s.id, &by_id))
        .max()
        .unwrap_or(0);
    let reclaimable: u64 = stale
        .iter()
        .filter(|s| !s.protected)
        .fold(0u64, |a, s| a.saturating_add(s.size_bytes));
    let unprotected = stale.iter().filter(|s| !s.protected).count();

    let mut findings = Vec::new();
    let mut status = "ok";
    if unprotected > 0 {
        status = "warning";
        findings.push(format!(
            "{unprotected} snapshot(s) older than {max_age_hours}h and not tagged keep"
        ));
    }
    if max_depth > max_chain {
        status = "warning";
        findings.push(format!(
            "snapshot chain is {max_depth} deep, more than the recommended {max_chain}"
        ));
    }
    if max_depth > HARD_MAX_CHAIN {
        status = "critical";
        findings.push(format!(
            "snapshot chain is {max_depth} deep, above the supported {HARD_MAX_CHAIN}"
        ));
    }
    if !orphaned.is_empty() {
        status = "critical";
        findings.push(format!(
            "{} snapshot(s) have a missing parent (broken chain)",
            orphaned.len()
        ));
    }

    VmHygiene {
        vm: vm.to_string(),
        status,
        snapshot_count: snaps.len(),
        total_bytes: snaps
            .iter()
            .fold(0u64, |a, s| a.saturating_add(s.size_bytes)),
        oldest_age_hours: snaps.iter().map(age_h).max(),
        max_chain_depth: max_depth,
        orphaned,
        stale,
        reclaimable_bytes: reclaimable,
        findings,
    }
}

fn rank(status: &str) -> u8 {
    match status {
        "critical" => 0,
        "warning" => 1,
        _ => 2,
    }
}

#[derive(Debug, Clone, Serialize, PartialEq, Default)]
pub struct HygieneSummary {
    pub vms_with_snapshots: usize,
    pub ok: usize,
    pub warning: usize,
    pub critical: usize,
    pub stale_snapshots: usize,
    pub protected_stale: usize,
    pub total_bytes: u64,
    pub reclaimable_bytes: u64,
}

pub fn summarize(vms: &[VmHygiene]) -> HygieneSummary {
    let mut s = HygieneSummary::default();
    for v in vms {
        s.vms_with_snapshots += 1;
        match v.status {
            "critical" => s.critical += 1,
            "warning" => s.warning += 1,
            _ => s.ok += 1,
        }
        s.stale_snapshots += v.stale.len();
        s.protected_stale += v.stale.iter().filter(|x| x.protected).count();
        s.total_bytes = s.total_bytes.saturating_add(v.total_bytes);
        s.reclaimable_bytes = s.reclaimable_bytes.saturating_add(v.reclaimable_bytes);
    }
    s
}

/// Worst status first, then most reclaimable bytes, then name.
pub fn sort_worst_first(v: &mut [VmHygiene]) {
    v.sort_by(|a, b| {
        rank(a.status)
            .cmp(&rank(b.status))
            .then(b.reclaimable_bytes.cmp(&a.reclaimable_bytes))
            .then_with(|| a.vm.cmp(&b.vm))
    });
}

#[derive(Debug, Serialize)]
pub struct HygieneReport {
    pub generated_at: DateTime<Utc>,
    pub max_age_hours: u64,
    pub max_chain: usize,
    pub summary: HygieneSummary,
    pub vms: Vec<VmHygiene>,
}

#[derive(Debug, Deserialize)]
pub struct HygieneQuery {
    #[serde(default)]
    pub max_age_hours: Option<u64>,
    #[serde(default)]
    pub max_chain: Option<usize>,
}

/// Validate the thresholds; returns `(max_age_hours, max_chain)`.
pub fn thresholds(max_age: Option<u64>, chain: Option<usize>) -> Result<(u64, usize), String> {
    let age = max_age.unwrap_or(DEFAULT_MAX_AGE_HOURS);
    if age == 0 || age > MAX_AGE_HOURS_LIMIT {
        return Err(format!("max_age_hours must be 1..={MAX_AGE_HOURS_LIMIT}"));
    }
    let chain = chain.unwrap_or(DEFAULT_MAX_CHAIN);
    if chain == 0 || chain > HARD_MAX_CHAIN {
        return Err(format!("max_chain must be 1..={HARD_MAX_CHAIN}"));
    }
    Ok((age, chain))
}

/// Assess every VM the caller may see. VMs without snapshots are left out.
pub fn collect(
    state: &AppState,
    tenant: Option<&str>,
    now: DateTime<Utc>,
    max_age_hours: u64,
    max_chain: usize,
) -> Result<Vec<VmHygiene>, String> {
    let vms = state.store.list_vms().map_err(|e| e.to_string())?;
    let mut out = Vec::new();
    for vm in vms {
        if let Some(t) = tenant {
            if crate::tenant_scope::vm_tenant(&vm).as_deref() != Some(t) {
                continue;
            }
        }
        let snaps: Vec<VMSnapshot> = state
            .store
            .list_entities(&format!("snapshots_{}", vm.name))
            .unwrap_or_default();
        if snaps.is_empty() {
            continue;
        }
        out.push(assess(&vm.name, &snaps, now, max_age_hours, max_chain));
    }
    sort_worst_first(&mut out);
    Ok(out)
}

/// GET /api/snapshots/hygiene
pub async fn snapshot_hygiene(
    RequireRead(claims): RequireRead,
    State(state): State<Arc<AppState>>,
    Query(q): Query<HygieneQuery>,
) -> impl IntoResponse {
    let (age, chain) = match thresholds(q.max_age_hours, q.max_chain) {
        Ok(t) => t,
        Err(e) => return json_error(StatusCode::BAD_REQUEST, e).into_response(),
    };
    let now = Utc::now();
    match collect(&state, claims.tenant.as_deref(), now, age, chain) {
        Ok(vms) => Json(HygieneReport {
            generated_at: now,
            max_age_hours: age,
            max_chain: chain,
            summary: summarize(&vms),
            vms,
        })
        .into_response(),
        Err(e) => json_error(StatusCode::INTERNAL_SERVER_ERROR, e).into_response(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::snapshots::SnapshotType;
    use chrono::{Duration, TimeZone};

    fn now() -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 6, 1, 12, 0, 0).unwrap()
    }

    fn snap(id: &str, age_h: i64, parent: Option<&str>, bytes: u64, tags: &[&str]) -> VMSnapshot {
        VMSnapshot {
            id: id.into(),
            vm_name: "vm".into(),
            name: format!("n-{id}"),
            description: None,
            snapshot_type: SnapshotType::Disk,
            parent_id: parent.map(String::from),
            size_bytes: bytes,
            created: now() - Duration::hours(age_h),
            tags: tags.iter().map(|t| t.to_string()).collect(),
        }
    }

    #[test]
    fn fresh_short_chain_is_ok() {
        let s = vec![snap("a", 1, None, 10, &[]), snap("b", 0, Some("a"), 5, &[])];
        let h = assess("vm", &s, now(), 72, 3);
        assert_eq!(h.status, "ok");
        assert_eq!(h.max_chain_depth, 2);
        assert!(h.stale.is_empty());
        assert_eq!(h.reclaimable_bytes, 0);
    }

    #[test]
    fn stale_counts_unprotected_bytes_only() {
        let s = vec![
            snap("a", 100, None, 10, &[]),
            snap("b", 200, None, 20, &["keep"]),
            snap("c", 72, None, 40, &[]), // exactly at limit is not stale
        ];
        let h = assess("vm", &s, now(), 72, 3);
        assert_eq!(h.status, "warning");
        assert_eq!(h.stale.len(), 2);
        assert_eq!(h.stale[0].id, "b"); // oldest first
        assert!(h.stale[0].protected);
        assert_eq!(h.reclaimable_bytes, 10);
        assert_eq!(h.oldest_age_hours, Some(200));
    }

    #[test]
    fn deep_chain_warns_and_cycle_terminates() {
        let mut s = vec![snap("s0", 1, None, 1, &[])];
        for i in 1..5 {
            s.push(snap(
                &format!("s{i}"),
                1,
                Some(&format!("s{}", i - 1)),
                1,
                &[],
            ));
        }
        let h = assess("vm", &s, now(), 72, 3);
        assert_eq!(h.max_chain_depth, 5);
        assert_eq!(h.status, "warning");
        let cyc = vec![
            snap("x", 1, Some("y"), 1, &[]),
            snap("y", 1, Some("x"), 1, &[]),
        ];
        let h = assess("vm", &cyc, now(), 72, 3);
        assert_eq!(h.max_chain_depth, 2);
        assert_eq!(h.status, "ok");
    }

    #[test]
    fn over_hard_limit_and_orphans_are_critical() {
        let mut s = vec![snap("s0", 1, None, 1, &[])];
        for i in 1..=33 {
            s.push(snap(
                &format!("s{i}"),
                1,
                Some(&format!("s{}", i - 1)),
                1,
                &[],
            ));
        }
        let h = assess("vm", &s, now(), 72, 3);
        assert_eq!(h.status, "critical");
        let o = vec![snap("a", 1, Some("gone"), 1, &[])];
        let h = assess("vm", &o, now(), 72, 3);
        assert_eq!(h.status, "critical");
        assert_eq!(h.orphaned, vec!["a".to_string()]);
    }

    #[test]
    fn future_snapshot_is_age_zero() {
        let h = assess("vm", &[snap("a", -50, None, 1, &[])], now(), 1, 3);
        assert_eq!(h.oldest_age_hours, Some(0));
        assert_eq!(h.status, "ok");
    }

    #[test]
    fn threshold_validation() {
        assert_eq!(thresholds(None, None).unwrap(), (72, 3));
        assert!(thresholds(Some(0), None).is_err());
        assert!(thresholds(Some(87_601), None).is_err());
        assert!(thresholds(None, Some(0)).is_err());
        assert!(thresholds(None, Some(33)).is_err());
        assert!(thresholds(Some(1), Some(32)).is_ok());
    }

    #[test]
    fn sorting_and_summary() {
        let a = assess("a", &[snap("1", 100, None, 5, &[])], now(), 72, 3);
        let b = assess("b", &[snap("1", 1, Some("gone"), 5, &[])], now(), 72, 3);
        let c = assess("c", &[snap("1", 1, None, 5, &[])], now(), 72, 3);
        let mut v = vec![c, a, b];
        sort_worst_first(&mut v);
        assert_eq!(
            v.iter().map(|x| x.vm.as_str()).collect::<Vec<_>>(),
            ["b", "a", "c"]
        );
        let s = summarize(&v);
        assert_eq!((s.critical, s.warning, s.ok), (1, 1, 1));
        assert_eq!(s.reclaimable_bytes, 5);
    }
}
