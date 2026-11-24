// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: LicenseRef-Zyvor-Production-2.0

//! Reclamation report: what could be cleaned up to give capacity back.
//!
//! VMware Aria Operations flags powered-off VMs after a configurable number
//! of days and reports orphaned or oversized resources, leaving the delete to
//! the operator. `GET /api/reclaim/report` does the same with what Fabric
//! knows, read-only:
//!
//! * Idle VMs: state `Stopped` or `Failed` whose last recorded change is at
//!   least `stopped_days` old (default 30). Fabric does not store when a VM
//!   was stopped, so "idle" is measured from the VM record's `updated` time
//!   (set on lifecycle changes and edits), falling back to `created`. A tag
//!   edit therefore restarts the clock; the report says which date it used.
//! * Expired leases: any VM whose ownership `expires_at` has passed (see
//!   `vm_ownership`), including running ones.
//! * Stale snapshots: the reclaimable bytes from the snapshot hygiene report
//!   (`snapshot_age_hours`, default 72), per candidate and fleet-wide.
//!
//! `reclaimable_disk_gb` counts the provisioned disk of idle (stopped or
//! failed) VMs only; it is what deleting them would free if disks are fully
//! allocated, an upper bound for thin-provisioned storage. A running VM with
//! an expired lease is listed for review with 0 reclaimable. Nothing is
//! stopped or deleted. Tenant-scoped tokens see only their tenant's VMs.

use std::collections::BTreeMap;
use std::sync::Arc;

use axum::{
    extract::{Query, State},
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use vm_model::VMState;

use crate::api_error::json_error;
use crate::server::AppState;
use crate::snapshot_hygiene::VmHygiene;
use crate::vm_ownership::Ownership;
use security::RequireRead;

pub const DEFAULT_STOPPED_DAYS: i64 = 30;
const MAX_STOPPED_DAYS: i64 = 3650;

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct Candidate {
    pub vm: String,
    pub state: String,
    pub reasons: Vec<String>,
    /// `updated` or `created`: which timestamp the idle time is based on.
    pub idle_basis: &'static str,
    pub idle_days: i64,
    pub disk_gb: u64,
    pub reclaimable_disk_gb: u64,
    pub reclaimable_snapshot_bytes: u64,
    pub owner: Option<String>,
    pub contact: Option<String>,
    pub lease_expires_at: Option<DateTime<Utc>>,
}

/// Minimal VM facts the evaluation needs (keeps the logic testable).
#[derive(Debug, Clone)]
pub struct VmFacts {
    pub name: String,
    pub state: VMState,
    pub disk_gb: u64,
    pub updated: Option<DateTime<Utc>>,
    pub created: DateTime<Utc>,
}

fn is_idle_state(s: &VMState) -> bool {
    matches!(s, VMState::Stopped | VMState::Failed)
}

pub fn evaluate(
    vm: &VmFacts,
    ownership: Option<&Ownership>,
    hygiene: Option<&VmHygiene>,
    now: DateTime<Utc>,
    stopped_days: i64,
) -> Option<Candidate> {
    let (basis, since) = match vm.updated {
        Some(u) => ("updated", u),
        None => ("created", vm.created),
    };
    let idle_days = (now - since).num_days().max(0);
    let mut reasons = Vec::new();
    let idle = is_idle_state(&vm.state) && idle_days >= stopped_days;
    if idle {
        reasons.push(format!(
            "{:?} with no recorded change for {idle_days} days",
            vm.state
        ));
    }
    let lease = ownership.and_then(|o| o.expires_at);
    let expired = lease.is_some_and(|e| e <= now);
    if expired {
        reasons.push("lease expired".to_string());
    }
    if reasons.is_empty() {
        return None;
    }
    Some(Candidate {
        vm: vm.name.clone(),
        state: format!("{:?}", vm.state),
        reasons,
        idle_basis: basis,
        idle_days,
        disk_gb: vm.disk_gb,
        reclaimable_disk_gb: if is_idle_state(&vm.state) {
            vm.disk_gb
        } else {
            0
        },
        reclaimable_snapshot_bytes: hygiene.map_or(0, |h| h.reclaimable_bytes),
        owner: ownership.and_then(|o| o.owner.clone()),
        contact: ownership.and_then(|o| o.contact.clone()),
        lease_expires_at: lease,
    })
}

/// Expired leases first, then longest idle, then name.
pub fn sort_candidates(c: &mut [Candidate]) {
    let expired = |c: &Candidate| c.reasons.iter().any(|r| r == "lease expired");
    c.sort_by(|a, b| {
        expired(b)
            .cmp(&expired(a))
            .then(b.idle_days.cmp(&a.idle_days))
            .then_with(|| a.vm.cmp(&b.vm))
    });
}

#[derive(Debug, Serialize, Default, PartialEq)]
pub struct Summary {
    pub candidates: usize,
    pub idle_vms: usize,
    pub expired_leases: usize,
    pub unowned_candidates: usize,
    pub reclaimable_disk_gb: u64,
    pub reclaimable_snapshot_bytes_candidates: u64,
    /// Across every VM, not only candidates.
    pub stale_snapshot_bytes_total: u64,
}

pub fn summarize(c: &[Candidate], stale_total: u64) -> Summary {
    let mut s = Summary {
        candidates: c.len(),
        stale_snapshot_bytes_total: stale_total,
        ..Default::default()
    };
    for x in c {
        if x.reasons.iter().any(|r| r == "lease expired") {
            s.expired_leases += 1;
        }
        if x.reasons.iter().any(|r| r.contains("no recorded change")) {
            s.idle_vms += 1;
        }
        if x.owner.is_none() {
            s.unowned_candidates += 1;
        }
        s.reclaimable_disk_gb = s.reclaimable_disk_gb.saturating_add(x.reclaimable_disk_gb);
        s.reclaimable_snapshot_bytes_candidates = s
            .reclaimable_snapshot_bytes_candidates
            .saturating_add(x.reclaimable_snapshot_bytes);
    }
    s
}

#[derive(Debug, Serialize)]
pub struct ReclaimReport {
    pub generated_at: DateTime<Utc>,
    pub stopped_days: i64,
    pub snapshot_age_hours: u64,
    pub summary: Summary,
    pub candidates: Vec<Candidate>,
}

#[derive(Debug, Deserialize)]
pub struct ReclaimQuery {
    #[serde(default)]
    pub stopped_days: Option<i64>,
    #[serde(default)]
    pub snapshot_age_hours: Option<u64>,
}

/// GET /api/reclaim/report
pub async fn reclaim_report(
    RequireRead(claims): RequireRead,
    State(state): State<Arc<AppState>>,
    Query(q): Query<ReclaimQuery>,
) -> impl IntoResponse {
    let stopped_days = q.stopped_days.unwrap_or(DEFAULT_STOPPED_DAYS);
    if !(1..=MAX_STOPPED_DAYS).contains(&stopped_days) {
        return json_error(
            StatusCode::BAD_REQUEST,
            format!("stopped_days must be 1..={MAX_STOPPED_DAYS}"),
        )
        .into_response();
    }
    let (age, chain) = match crate::snapshot_hygiene::thresholds(q.snapshot_age_hours, None) {
        Ok(t) => t,
        Err(e) => {
            return json_error(
                StatusCode::BAD_REQUEST,
                e.replace("max_age_hours", "snapshot_age_hours"),
            )
            .into_response()
        }
    };
    let now = Utc::now();
    let tenant = claims.tenant.as_deref();
    let vms = match state.store.list_vms() {
        Ok(v) => v,
        Err(e) => {
            return json_error(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response()
        }
    };
    let hygiene: BTreeMap<String, VmHygiene> =
        match crate::snapshot_hygiene::collect(&state, tenant, now, age, chain) {
            Ok(h) => h.into_iter().map(|h| (h.vm.clone(), h)).collect(),
            Err(e) => return json_error(StatusCode::INTERNAL_SERVER_ERROR, e).into_response(),
        };
    let stale_total = hygiene
        .values()
        .fold(0u64, |a, h| a.saturating_add(h.reclaimable_bytes));

    let mut cands = Vec::new();
    for vm in vms {
        if let Some(t) = tenant {
            if crate::tenant_scope::vm_tenant(&vm).as_deref() != Some(t) {
                continue;
            }
        }
        let own = state
            .store
            .get_entity::<Ownership>(crate::vm_ownership::STORE_KEY, &vm.name)
            .ok()
            .flatten();
        let facts = VmFacts {
            name: vm.name.clone(),
            state: vm.state.clone(),
            disk_gb: vm.disk,
            updated: vm.updated,
            created: vm.created,
        };
        if let Some(c) = evaluate(
            &facts,
            own.as_ref(),
            hygiene.get(&vm.name),
            now,
            stopped_days,
        ) {
            cands.push(c);
        }
    }
    sort_candidates(&mut cands);
    Json(ReclaimReport {
        generated_at: now,
        stopped_days,
        snapshot_age_hours: age,
        summary: summarize(&cands, stale_total),
        candidates: cands,
    })
    .into_response()
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{Duration, TimeZone};

    fn now() -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 6, 1, 12, 0, 0).unwrap()
    }

    fn facts(state: VMState, upd_days: Option<i64>, disk: u64) -> VmFacts {
        VmFacts {
            name: "v".into(),
            state,
            disk_gb: disk,
            updated: upd_days.map(|d| now() - Duration::days(d)),
            created: now() - Duration::days(400),
        }
    }

    fn lease(days_from_now: i64) -> Ownership {
        Ownership {
            vm_name: "v".into(),
            owner: Some("o".into()),
            expires_at: Some(now() + Duration::days(days_from_now)),
            ..Default::default()
        }
    }

    #[test]
    fn running_vm_is_not_a_candidate() {
        assert!(evaluate(
            &facts(VMState::Running, Some(100), 10),
            None,
            None,
            now(),
            30
        )
        .is_none());
    }

    #[test]
    fn idle_threshold_and_basis() {
        assert!(evaluate(
            &facts(VMState::Stopped, Some(29), 10),
            None,
            None,
            now(),
            30
        )
        .is_none());
        let c = evaluate(
            &facts(VMState::Stopped, Some(30), 10),
            None,
            None,
            now(),
            30,
        )
        .unwrap();
        assert_eq!(c.idle_basis, "updated");
        assert_eq!(c.reclaimable_disk_gb, 10);
        let c = evaluate(&facts(VMState::Failed, None, 5), None, None, now(), 30).unwrap();
        assert_eq!(c.idle_basis, "created");
        assert_eq!(c.idle_days, 400);
    }

    #[test]
    fn expired_lease_on_running_vm_has_no_reclaimable_disk() {
        let o = lease(-1);
        let c = evaluate(
            &facts(VMState::Running, Some(1), 50),
            Some(&o),
            None,
            now(),
            30,
        )
        .unwrap();
        assert_eq!(c.reclaimable_disk_gb, 0);
        assert_eq!(c.reasons, vec!["lease expired".to_string()]);
        let live = lease(5);
        assert!(evaluate(
            &facts(VMState::Running, Some(1), 50),
            Some(&live),
            None,
            now(),
            30
        )
        .is_none());
    }

    #[test]
    fn sorting_and_summary() {
        let o = lease(-1);
        let a = evaluate(
            &facts(VMState::Stopped, Some(90), 10),
            None,
            None,
            now(),
            30,
        )
        .unwrap();
        let mut b = evaluate(
            &facts(VMState::Running, Some(1), 20),
            Some(&o),
            None,
            now(),
            30,
        )
        .unwrap();
        b.vm = "b".into();
        let mut c = evaluate(
            &facts(VMState::Stopped, Some(40), 30),
            None,
            None,
            now(),
            30,
        )
        .unwrap();
        c.vm = "c".into();
        let mut v = vec![c, a, b];
        sort_candidates(&mut v);
        assert_eq!(
            v.iter().map(|x| x.vm.as_str()).collect::<Vec<_>>(),
            ["b", "v", "c"]
        );
        let s = summarize(&v, 77);
        assert_eq!(s.candidates, 3);
        assert_eq!(s.idle_vms, 2);
        assert_eq!(s.expired_leases, 1);
        assert_eq!(s.unowned_candidates, 2);
        assert_eq!(s.reclaimable_disk_gb, 40);
        assert_eq!(s.stale_snapshot_bytes_total, 77);
    }
}
