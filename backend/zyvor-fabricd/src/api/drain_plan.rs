// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: LicenseRef-Zyvor-Production-2.0

//! Host drain planner.
//!
//! `POST /api/hosts/{id}/drain-plan` answers "if I put this host into
//! maintenance, where would its VMs go?" without moving, stopping or
//! changing anything. The planning maths lives in the pure [`plan_drain`]
//! function; the handler only gathers inputs from the store.
//!
//! Inputs and rules
//! * A VM belongs to a host when its label [`HOST_LABEL`] equals the host's
//!   id or hostname. Fabric does not otherwise track VM placement, so VMs
//!   without the label are not considered (the plan warns with their count).
//! * Targets are other hosts in the same cluster with status `connected`.
//!   Capacity is `floor(total * ratio)` using the overcommit policy when it is
//!   enabled; load is the vCPUs and memory of running VMs assigned to the host.
//! * The label [`EVICTION_LABEL`] picks a per-VM strategy (same idea as
//!   KubeVirt eviction strategies): `migrate` (default), `migrate-if-possible`
//!   (stop the VM when it cannot be moved), `stop` (always stop) or `pin`
//!   (never move: the drain is blocked).
//! * DRS affinity rules (`affinity_rules`) are honoured: `required` rules are
//!   hard constraints, `preferred` rules only steer the choice of target.
//! * Targets are chosen first by fewest soft-rule violations, then by most
//!   free memory afterwards, then by host id, so the result is deterministic.
//!   VMs are placed largest memory first.
//! * `plan_hash` is a SHA-256 over the canonical plan, so a client can tell
//!   whether the plan changed between two calls. There is no apply step.

use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use serde::Serialize;
use serde_json::json;
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use super::resource_policy::OvercommitPolicy;
use crate::cpu_compat::{self, CpuRequirement, CpuVerdict, HostCpu};
use crate::server::AppState;
use datacenter::{HostInfo, HostStatus};
use predictive_drs::{AffinityRule, AffinityRuleType, RuleEnforcement};
use security::RequireRead;
use vm_model::VMState;

pub const HOST_LABEL: &str = "fabric.zyvor.dev/host";
pub const EVICTION_LABEL: &str = "fabric.zyvor.dev/eviction";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Eviction {
    Migrate,
    MigrateIfPossible,
    Stop,
    Pin,
}

impl Eviction {
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "migrate" => Some(Self::Migrate),
            "migrate-if-possible" => Some(Self::MigrateIfPossible),
            "stop" => Some(Self::Stop),
            "pin" => Some(Self::Pin),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct DrainHost {
    pub id: String,
    pub cluster_id: String,
    /// Eligible as a target (status `connected`).
    pub schedulable: bool,
    pub vcpus: u64,
    pub memory_mb: u64,
    /// CPU inventory of the host, when one was recorded.
    pub cpu: Option<HostCpu>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct DrainVm {
    pub name: String,
    pub host_id: String,
    pub running: bool,
    pub vcpus: u64,
    pub memory_mb: u64,
    pub eviction: Eviction,
    /// CPU requirement derived from the VM's stored CPU config, if any.
    pub cpu: Option<CpuRequirement>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RuleKind {
    Affinity,
    AntiAffinity,
    HostAffinity,
    HostAntiAffinity,
}

#[derive(Debug, Clone, PartialEq)]
pub struct DrainRule {
    pub name: String,
    pub kind: RuleKind,
    pub required: bool,
    pub vms: Vec<String>,
    pub host_ids: Vec<String>,
}

impl DrainRule {
    pub fn from_drs(r: &AffinityRule) -> Option<Self> {
        if !r.enabled {
            return None;
        }
        Some(Self {
            name: r.name.clone(),
            kind: match r.rule_type {
                AffinityRuleType::VmToVmAffinity => RuleKind::Affinity,
                AffinityRuleType::VmToVmAntiAffinity => RuleKind::AntiAffinity,
                AffinityRuleType::VmToHostAffinity => RuleKind::HostAffinity,
                AffinityRuleType::VmToHostAntiAffinity => RuleKind::HostAntiAffinity,
            },
            required: r.enforcement == RuleEnforcement::Required,
            vms: r.vm_names.clone(),
            host_ids: r.host_ids.clone().unwrap_or_default(),
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Ratios {
    pub cpu: f64,
    pub memory: f64,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct DrainAction {
    /// 1-based execution order.
    pub order: u32,
    pub vm: String,
    /// `migrate` (running VM, live move), `relocate` (stopped VM, cold move),
    /// `stop` or `blocked`.
    pub action: &'static str,
    pub target: Option<String>,
    pub reasons: Vec<String>,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct TargetReport {
    pub host_id: String,
    pub vcpus_used_before: u64,
    pub vcpus_used_after: u64,
    pub vcpus_capacity: u64,
    pub memory_mb_used_before: u64,
    pub memory_mb_used_after: u64,
    pub memory_mb_capacity: u64,
}

#[derive(Debug, Clone, Serialize, PartialEq, Default)]
pub struct DrainSummary {
    pub migrate: u32,
    pub relocate: u32,
    pub stop: u32,
    pub blocked: u32,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct DrainPlan {
    pub host_id: String,
    pub dry_run: bool,
    /// True when no VM is blocked (stops count as drainable).
    pub can_drain: bool,
    pub actions: Vec<DrainAction>,
    pub summary: DrainSummary,
    pub targets: Vec<TargetReport>,
    pub warnings: Vec<String>,
    pub plan_hash: String,
}

fn scale(total: u64, ratio: f64) -> u64 {
    (total as f64 * ratio).floor() as u64
}

struct Ctx<'a> {
    source: &'a str,
    rules: &'a [DrainRule],
    /// Current (or planned) host of every VM that is not stopping.
    loc: BTreeMap<String, String>,
}

/// Hard rejection reasons and the soft-violation count for putting `vm` on `host`.
fn rule_check(ctx: &Ctx, vm: &str, host: &str) -> (Vec<String>, u32) {
    let mut hard = Vec::new();
    let mut soft = 0u32;
    for r in ctx.rules.iter().filter(|r| r.vms.iter().any(|v| v == vm)) {
        let violated = match r.kind {
            RuleKind::AntiAffinity => r
                .vms
                .iter()
                .any(|m| m != vm && ctx.loc.get(m).is_some_and(|h| h == host)),
            RuleKind::Affinity => {
                let partners: BTreeSet<&String> = r
                    .vms
                    .iter()
                    .filter(|m| *m != vm)
                    .filter_map(|m| ctx.loc.get(m))
                    .filter(|h| h.as_str() != ctx.source)
                    .collect();
                !partners.is_empty() && !partners.iter().any(|h| h.as_str() == host)
            }
            RuleKind::HostAffinity => !r.host_ids.iter().any(|h| h == host),
            RuleKind::HostAntiAffinity => r.host_ids.iter().any(|h| h == host),
        };
        if violated {
            if r.required {
                hard.push(format!("rule:{}", r.name));
            } else {
                soft += 1;
            }
        }
    }
    (hard, soft)
}

pub fn plan_drain(
    source_id: &str,
    hosts: &[DrainHost],
    vms: &[DrainVm],
    rules: &[DrainRule],
    ratios: Ratios,
    mut warnings: Vec<String>,
) -> DrainPlan {
    let source_cluster = hosts
        .iter()
        .find(|h| h.id == source_id)
        .map(|h| h.cluster_id.clone())
        .unwrap_or_default();
    let mut candidates: Vec<&DrainHost> = hosts
        .iter()
        .filter(|h| h.id != source_id && h.schedulable && h.cluster_id == source_cluster)
        .collect();
    candidates.sort_by(|a, b| a.id.cmp(&b.id));

    let cap = |h: &DrainHost| {
        (
            scale(h.vcpus, ratios.cpu),
            scale(h.memory_mb, ratios.memory),
        )
    };
    let mut used: BTreeMap<String, (u64, u64)> =
        candidates.iter().map(|h| (h.id.clone(), (0, 0))).collect();
    for v in vms.iter().filter(|v| v.running) {
        if let Some(u) = used.get_mut(&v.host_id) {
            u.0 += v.vcpus;
            u.1 += v.memory_mb;
        }
    }
    let before = used.clone();

    let mut on_source: Vec<&DrainVm> = vms.iter().filter(|v| v.host_id == source_id).collect();
    on_source.sort_by(|a, b| {
        b.memory_mb
            .cmp(&a.memory_mb)
            .then(b.vcpus.cmp(&a.vcpus))
            .then(a.name.cmp(&b.name))
    });

    // VMs we plan to stop are removed from the location map so they do not
    // attract or repel anything.
    let always_stop: BTreeSet<&str> = on_source
        .iter()
        .filter(|v| v.running && v.eviction == Eviction::Stop)
        .map(|v| v.name.as_str())
        .collect();
    let mut ctx = Ctx {
        source: source_id,
        rules,
        loc: vms
            .iter()
            .filter(|v| !always_stop.contains(v.name.as_str()))
            .map(|v| (v.name.clone(), v.host_id.clone()))
            .collect(),
    };

    struct Done {
        vm: String,
        action: &'static str,
        target: Option<String>,
        reasons: Vec<String>,
        warnings: Vec<String>,
    }
    let mut migrates: Vec<Done> = Vec::new();
    let mut relocates: Vec<Done> = Vec::new();
    let mut stops: Vec<Done> = Vec::new();
    let mut blocked: Vec<Done> = Vec::new();

    // Migrations first (running), then relocations (stopped); the sort above
    // is by size, so stable partitioning keeps largest-first inside each group.
    let ordered = on_source
        .iter()
        .filter(|v| v.running && v.eviction != Eviction::Stop && v.eviction != Eviction::Pin)
        .chain(
            on_source
                .iter()
                .filter(|v| !v.running && v.eviction != Eviction::Pin),
        );

    for v in on_source.iter().filter(|v| v.eviction == Eviction::Pin) {
        blocked.push(Done {
            vm: v.name.clone(),
            action: "blocked",
            target: None,
            reasons: vec!["pinned".into()],
            warnings: vec![],
        });
    }
    for v in on_source
        .iter()
        .filter(|v| v.running && v.eviction == Eviction::Stop)
    {
        stops.push(Done {
            vm: v.name.clone(),
            action: "stop",
            target: None,
            reasons: vec!["eviction=stop".into()],
            warnings: vec![],
        });
    }

    for v in ordered {
        let mut rejected: BTreeSet<String> = BTreeSet::new();
        let mut cpu_details: BTreeSet<String> = BTreeSet::new();
        let mut best: Option<(u32, i128, &DrainHost)> = None;
        if candidates.is_empty() {
            rejected.insert("no_target_hosts".into());
        }
        for h in &candidates {
            let (cpu_cap, mem_cap) = cap(h);
            let (uc, um) = used[&h.id];
            if v.running {
                if uc + v.vcpus > cpu_cap {
                    rejected.insert("vcpu".into());
                    continue;
                }
                if um + v.memory_mb > mem_cap {
                    rejected.insert("memory".into());
                    continue;
                }
            }
            let mut cpu_unknown = false;
            if let Some(req) = &v.cpu {
                match cpu_compat::check(req, h.cpu.as_ref()) {
                    CpuVerdict::Compatible => {}
                    CpuVerdict::Unknown => cpu_unknown = true,
                    CpuVerdict::Incompatible {
                        missing,
                        vendor_mismatch,
                    } => {
                        rejected.insert("cpu_incompatible".into());
                        cpu_details.insert(format!(
                            "host {} {}",
                            h.id,
                            cpu_compat::describe_incompatible(&missing, vendor_mismatch)
                        ));
                        continue;
                    }
                }
            }
            let (hard, soft) = rule_check(&ctx, &v.name, &h.id);
            if !hard.is_empty() {
                rejected.extend(hard);
                continue;
            }
            let free_after = i128::from(mem_cap) - i128::from(um) - i128::from(v.memory_mb);
            // Prefer a target whose CPU was verified over an unverified one.
            let key = soft * 2 + u32::from(cpu_unknown);
            let better = match &best {
                None => true,
                Some((bs, bf, _)) => key < *bs || (key == *bs && free_after > *bf),
            };
            if better {
                best = Some((key, free_after, h));
            }
        }
        match best {
            Some((key, _, h)) => {
                let soft = key / 2;
                let unverified = key % 2 == 1;
                if v.running {
                    let u = used.get_mut(&h.id).expect("candidate tracked");
                    u.0 += v.vcpus;
                    u.1 += v.memory_mb;
                }
                ctx.loc.insert(v.name.clone(), h.id.clone());
                let mut w = Vec::new();
                if soft > 0 {
                    w.push(format!("{soft} preferred rule(s) not satisfied on target"));
                }
                if unverified {
                    w.push(format!(
                        "CPU compatibility with host {} not verified (no CPU inventory)",
                        h.id
                    ));
                }
                let d = Done {
                    vm: v.name.clone(),
                    action: if v.running { "migrate" } else { "relocate" },
                    target: Some(h.id.clone()),
                    reasons: vec![],
                    warnings: w,
                };
                if v.running {
                    migrates.push(d)
                } else {
                    relocates.push(d)
                }
            }
            None => {
                let reasons: Vec<String> = rejected.into_iter().collect();
                if v.running && v.eviction == Eviction::MigrateIfPossible {
                    ctx.loc.remove(&v.name);
                    stops.push(Done {
                        vm: v.name.clone(),
                        action: "stop",
                        target: None,
                        reasons: vec!["eviction=migrate-if-possible".into()],
                        warnings: vec![format!("cannot be moved ({})", reasons.join(", "))],
                    });
                } else {
                    blocked.push(Done {
                        vm: v.name.clone(),
                        action: "blocked",
                        target: None,
                        reasons,
                        warnings: cpu_details.into_iter().collect(),
                    });
                }
            }
        }
    }

    stops.sort_by(|a, b| a.vm.cmp(&b.vm));
    blocked.sort_by(|a, b| a.vm.cmp(&b.vm));
    let mut summary = DrainSummary::default();
    let mut actions = Vec::new();
    for d in migrates
        .into_iter()
        .chain(relocates)
        .chain(stops)
        .chain(blocked)
    {
        match d.action {
            "migrate" => summary.migrate += 1,
            "relocate" => summary.relocate += 1,
            "stop" => summary.stop += 1,
            _ => summary.blocked += 1,
        }
        actions.push(DrainAction {
            order: actions.len() as u32 + 1,
            vm: d.vm,
            action: d.action,
            target: d.target,
            reasons: d.reasons,
            warnings: d.warnings,
        });
    }

    let targets: Vec<TargetReport> = candidates
        .iter()
        .map(|h| {
            let (cc, mc) = cap(h);
            TargetReport {
                host_id: h.id.clone(),
                vcpus_used_before: before[&h.id].0,
                vcpus_used_after: used[&h.id].0,
                vcpus_capacity: cc,
                memory_mb_used_before: before[&h.id].1,
                memory_mb_used_after: used[&h.id].1,
                memory_mb_capacity: mc,
            }
        })
        .collect();
    if candidates.is_empty() && !on_source.is_empty() {
        warnings.push("no other connected host in the same cluster to receive VMs".into());
    }

    let canon = json!({
        "host_id": source_id,
        "actions": actions,
        "summary": summary,
        "targets": targets,
    });
    // serde_json maps are BTreeMap-backed, so key order is canonical.
    let mut h = Sha256::new();
    h.update(serde_json::to_vec(&canon).unwrap_or_default());
    let plan_hash: String = h.finalize().iter().map(|b| format!("{b:02x}")).collect();

    DrainPlan {
        host_id: source_id.to_string(),
        dry_run: true,
        can_drain: summary.blocked == 0,
        actions,
        summary,
        targets,
        warnings,
        plan_hash,
    }
}

// ============================================================================
// HTTP handler
// ============================================================================

type ApiErr = (StatusCode, Json<serde_json::Value>);

fn err(code: StatusCode, msg: impl Into<String>) -> ApiErr {
    (code, Json(json!({ "error": msg.into() })))
}

/// POST /api/hosts/{id}/drain-plan - read-only, never moves or stops anything.
pub async fn drain_plan(
    RequireRead(claims): RequireRead,
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<DrainPlan>, ApiErr> {
    if claims.tenant.is_some() {
        return Err(err(
            StatusCode::FORBIDDEN,
            "Host operations are not available to tenant-scoped tokens",
        ));
    }
    let ise = |e: String| err(StatusCode::INTERNAL_SERVER_ERROR, e);
    let hosts: Vec<HostInfo> = state
        .store
        .list_entities("hosts")
        .map_err(|e| ise(e.to_string()))?;
    let source = hosts
        .iter()
        .find(|h| h.id == id || h.hostname == id)
        .ok_or_else(|| err(StatusCode::NOT_FOUND, "Host not found"))?;
    let policy = state
        .store
        .get_entity::<OvercommitPolicy>("config", "overcommit")
        .map_err(|e| ise(e.to_string()))?
        .unwrap_or_default();
    let ratios = if policy.enabled {
        Ratios {
            cpu: policy.cpu_ratio,
            memory: policy.memory_ratio,
        }
    } else {
        Ratios {
            cpu: 1.0,
            memory: 1.0,
        }
    };
    let rules: Vec<DrainRule> = state
        .store
        .list_entities::<AffinityRule>("affinity_rules")
        .unwrap_or_default()
        .iter()
        .filter_map(DrainRule::from_drs)
        .collect();
    let all_vms = state.store.list_vms().map_err(|e| ise(e.to_string()))?;

    let mut warnings = Vec::new();
    let mut unassigned = 0usize;
    let mut vms = Vec::new();
    for v in &all_vms {
        let label = v.labels.as_ref().and_then(|l| l.get(HOST_LABEL));
        let Some(label) = label else {
            unassigned += 1;
            continue;
        };
        let Some(host) = hosts
            .iter()
            .find(|h| &h.id == label)
            .or_else(|| hosts.iter().find(|h| &h.hostname == label))
        else {
            warnings.push(format!("VM '{}' names unknown host '{label}'", v.name));
            continue;
        };
        let eviction = match v.labels.as_ref().and_then(|l| l.get(EVICTION_LABEL)) {
            None => Eviction::Migrate,
            Some(s) => Eviction::parse(s).unwrap_or_else(|| {
                warnings.push(format!(
                    "VM '{}' has invalid {EVICTION_LABEL} '{s}'; using migrate",
                    v.name
                ));
                Eviction::Migrate
            }),
        };
        let running = matches!(
            v.state,
            VMState::Running | VMState::Paused | VMState::Starting
        );
        let cpu = cpu_compat::stored_vm_cpu(&state, &v.name).map(|cfg| {
            let src = state
                .store
                .get_entity::<cpu_compat::HostCpuRecord>(cpu_compat::ENTITY, &host.id)
                .ok()
                .flatten();
            cpu_compat::requirement(&cfg, src.as_ref().map(|r| &r.cpu), running)
        });
        vms.push(DrainVm {
            name: v.name.clone(),
            host_id: host.id.clone(),
            cpu,
            running,
            vcpus: u64::from(v.cpus),
            memory_mb: v.memory,
            eviction,
        });
    }
    if unassigned > 0 {
        warnings.push(format!(
            "{unassigned} VM(s) have no {HOST_LABEL} label and were not considered"
        ));
    }
    let dh: Vec<DrainHost> = hosts
        .iter()
        .map(|h| DrainHost {
            id: h.id.clone(),
            cluster_id: h.cluster_id.clone(),
            schedulable: h.status == HostStatus::Connected,
            vcpus: u64::from(h.cpus),
            memory_mb: h.memory_mb,
            cpu: state
                .store
                .get_entity::<cpu_compat::HostCpuRecord>(cpu_compat::ENTITY, &h.id)
                .ok()
                .flatten()
                .map(|r| r.cpu),
        })
        .collect();
    Ok(Json(plan_drain(
        &source.id, &dh, &vms, &rules, ratios, warnings,
    )))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn host(id: &str, cpus: u64, mem: u64) -> DrainHost {
        DrainHost {
            id: id.into(),
            cluster_id: "c1".into(),
            schedulable: true,
            vcpus: cpus,
            memory_mb: mem,
            cpu: None,
        }
    }

    fn vm(name: &str, host: &str, mem: u64) -> DrainVm {
        DrainVm {
            name: name.into(),
            host_id: host.into(),
            running: true,
            vcpus: 1,
            memory_mb: mem,
            eviction: Eviction::Migrate,
            cpu: None,
        }
    }

    const R1: Ratios = Ratios {
        cpu: 1.0,
        memory: 1.0,
    };

    fn rule(name: &str, kind: RuleKind, required: bool, vms: &[&str]) -> DrainRule {
        DrainRule {
            name: name.into(),
            kind,
            required,
            vms: vms.iter().map(|s| s.to_string()).collect(),
            host_ids: vec![],
        }
    }

    fn act<'a>(p: &'a DrainPlan, vm: &str) -> &'a DrainAction {
        p.actions.iter().find(|a| a.vm == vm).unwrap()
    }

    #[test]
    fn migrates_largest_first_to_roomiest_target() {
        let hosts = [host("a", 8, 8192), host("b", 8, 8192), host("c", 8, 4096)];
        let vms = [vm("small", "a", 512), vm("big", "a", 4096)];
        let p = plan_drain("a", &hosts, &vms, &[], R1, vec![]);
        assert!(p.can_drain && p.dry_run);
        assert_eq!(p.actions[0].vm, "big");
        assert_eq!(p.actions[0].order, 1);
        assert_eq!(p.actions[0].target.as_deref(), Some("b"));
        // b now has less room than c? b: 8192-4096=4096 free vs c 4096 free -> tie, next VM
        // goes to the roomier-after host; deterministic either way.
        assert_eq!(p.summary.migrate, 2);
        assert_eq!(p.targets.len(), 2);
    }

    #[test]
    fn capacity_blocks_and_names_the_resource() {
        let hosts = [host("a", 8, 8192), host("b", 8, 1024)];
        let vms = [vm("big", "a", 4096)];
        let p = plan_drain("a", &hosts, &vms, &[], R1, vec![]);
        assert!(!p.can_drain);
        assert_eq!(act(&p, "big").action, "blocked");
        assert_eq!(act(&p, "big").reasons, vec!["memory"]);
    }

    #[test]
    fn existing_load_counts_but_stopped_vms_do_not() {
        let hosts = [host("a", 8, 8192), host("b", 8, 2048)];
        let mut stopped = vm("idle", "b", 2048);
        stopped.running = false;
        let vms = [vm("x", "a", 2048), stopped];
        assert_eq!(
            plan_drain("a", &hosts, &vms, &[], R1, vec![])
                .summary
                .migrate,
            1
        );
        let vms = [vm("x", "a", 2048), vm("busy", "b", 1)];
        let p = plan_drain("a", &hosts, &vms, &[], R1, vec![]);
        assert_eq!(p.summary.blocked, 1);
    }

    #[test]
    fn overcommit_raises_capacity() {
        let hosts = [host("a", 8, 8192), host("b", 8, 1024)];
        let vms = [vm("x", "a", 1500)];
        let r = Ratios {
            cpu: 1.0,
            memory: 2.0,
        };
        assert!(plan_drain("a", &hosts, &vms, &[], r, vec![]).can_drain);
    }

    #[test]
    fn unschedulable_and_other_cluster_hosts_are_not_targets() {
        let mut down = host("b", 8, 8192);
        down.schedulable = false;
        let mut other = host("c", 8, 8192);
        other.cluster_id = "c2".into();
        let hosts = [host("a", 8, 8192), down, other];
        let p = plan_drain("a", &hosts, &[vm("x", "a", 1)], &[], R1, vec![]);
        assert!(!p.can_drain);
        assert_eq!(act(&p, "x").reasons, vec!["no_target_hosts"]);
        assert!(p.warnings.iter().any(|w| w.contains("no other connected")));
    }

    #[test]
    fn required_anti_affinity_is_a_hard_constraint() {
        let hosts = [host("a", 8, 8192), host("b", 8, 8192)];
        let vms = [vm("web1", "a", 100), vm("web2", "b", 100)];
        let rules = [rule(
            "spread",
            RuleKind::AntiAffinity,
            true,
            &["web1", "web2"],
        )];
        let p = plan_drain("a", &hosts, &vms, &rules, R1, vec![]);
        assert_eq!(act(&p, "web1").action, "blocked");
        assert_eq!(act(&p, "web1").reasons, vec!["rule:spread"]);
    }

    #[test]
    fn two_anti_affine_vms_on_source_land_on_different_hosts() {
        let hosts = [host("a", 8, 8192), host("b", 8, 8192), host("c", 8, 8192)];
        let vms = [vm("w1", "a", 200), vm("w2", "a", 100)];
        let rules = [rule("spread", RuleKind::AntiAffinity, true, &["w1", "w2"])];
        let p = plan_drain("a", &hosts, &vms, &rules, R1, vec![]);
        assert_ne!(act(&p, "w1").target, act(&p, "w2").target);
        assert!(p.can_drain);
    }

    #[test]
    fn preferred_anti_affinity_steers_but_does_not_block() {
        let hosts = [host("a", 8, 8192), host("b", 8, 8192), host("c", 8, 1000)];
        let vms = [vm("w1", "b", 100), vm("w2", "a", 100)];
        let rules = [rule("soft", RuleKind::AntiAffinity, false, &["w1", "w2"])];
        let p = plan_drain("a", &hosts, &vms, &rules, R1, vec![]);
        assert_eq!(act(&p, "w2").target.as_deref(), Some("c"));
        // when only the conflicting host exists it is used, with a warning
        let hosts = [host("a", 8, 8192), host("b", 8, 8192)];
        let p = plan_drain("a", &hosts, &vms, &rules, R1, vec![]);
        assert_eq!(act(&p, "w2").target.as_deref(), Some("b"));
        assert_eq!(act(&p, "w2").warnings.len(), 1);
    }

    #[test]
    fn affinity_follows_partner_and_host_rules_apply() {
        let hosts = [host("a", 8, 8192), host("b", 8, 8192), host("c", 8, 8192)];
        let vms = [vm("app", "a", 100), vm("db", "c", 100)];
        let rules = [rule("together", RuleKind::Affinity, true, &["app", "db"])];
        let p = plan_drain("a", &hosts, &vms, &rules, R1, vec![]);
        assert_eq!(act(&p, "app").target.as_deref(), Some("c"));

        let mut only_b = rule("pin-to-b", RuleKind::HostAffinity, true, &["app"]);
        only_b.host_ids = vec!["b".into()];
        let p = plan_drain("a", &hosts, &[vm("app", "a", 100)], &[only_b], R1, vec![]);
        assert_eq!(act(&p, "app").target.as_deref(), Some("b"));
        let mut not_bc = rule("avoid", RuleKind::HostAntiAffinity, true, &["app"]);
        not_bc.host_ids = vec!["b".into(), "c".into()];
        let p = plan_drain("a", &hosts, &[vm("app", "a", 100)], &[not_bc], R1, vec![]);
        assert!(!p.can_drain);
    }

    #[test]
    fn eviction_strategies() {
        let hosts = [host("a", 8, 8192), host("b", 8, 100)];
        let mut stop = vm("s", "a", 10);
        stop.eviction = Eviction::Stop;
        let mut pin = vm("p", "a", 10);
        pin.eviction = Eviction::Pin;
        let mut mip = vm("m", "a", 5000);
        mip.eviction = Eviction::MigrateIfPossible;
        let mut cold = vm("c", "a", 5000);
        cold.running = false;
        let p = plan_drain("a", &hosts, &[stop, pin, mip, cold], &[], R1, vec![]);
        assert_eq!(act(&p, "s").action, "stop");
        assert_eq!(act(&p, "p").action, "blocked");
        assert_eq!(act(&p, "m").action, "stop");
        assert!(!act(&p, "m").warnings.is_empty());
        // stopped VMs move cold and need no capacity
        assert_eq!(act(&p, "c").action, "relocate");
        assert!(!p.can_drain);
        // execution order: migrate/relocate before stop before blocked
        let order: Vec<&str> = p.actions.iter().map(|a| a.action).collect();
        assert_eq!(order, vec!["relocate", "stop", "stop", "blocked"]);
    }

    #[test]
    fn hash_is_stable_hex_and_sensitive() {
        let hosts = [host("a", 8, 8192), host("b", 8, 8192)];
        let vms = [vm("x", "a", 100)];
        let p1 = plan_drain("a", &hosts, &vms, &[], R1, vec![]);
        let p2 = plan_drain("a", &hosts, &vms, &[], R1, vec![]);
        assert_eq!(p1.plan_hash, p2.plan_hash);
        assert_eq!(p1.plan_hash.len(), 64);
        let vms2 = [vm("x", "a", 200)];
        assert_ne!(
            p1.plan_hash,
            plan_drain("a", &hosts, &vms2, &[], R1, vec![]).plan_hash
        );
    }

    #[test]
    fn empty_source_is_trivially_drainable() {
        let p = plan_drain("a", &[host("a", 1, 1)], &[], &[], R1, vec![]);
        assert!(p.can_drain && p.actions.is_empty());
    }

    fn cpu_host(id: &str, flags: &[&str]) -> DrainHost {
        let mut h = host(id, 8, 8192);
        h.cpu = Some(HostCpu {
            vendor: "GenuineIntel".into(),
            model_name: "t".into(),
            flags: flags.iter().map(|s| s.to_string()).collect(),
        });
        h
    }

    fn req(flags: &[&str]) -> CpuRequirement {
        CpuRequirement {
            vendor: None,
            features: flags.iter().map(|s| s.to_string()).collect(),
            notes: vec![],
        }
    }

    #[test]
    fn cpu_incompatible_blocks_with_reason_and_detail() {
        let hosts = [cpu_host("a", &["avx2"]), cpu_host("b", &["sse4_2"])];
        let mut v = vm("x", "a", 100);
        v.cpu = Some(req(&["avx2"]));
        let p = plan_drain("a", &hosts, &[v], &[], R1, vec![]);
        assert!(!p.can_drain);
        assert_eq!(act(&p, "x").reasons, vec!["cpu_incompatible"]);
        assert!(act(&p, "x").warnings[0].contains("lacks avx2"));
    }

    #[test]
    fn cpu_check_steers_to_the_compatible_host() {
        let hosts = [
            cpu_host("a", &["avx2"]),
            cpu_host("b", &["sse4_2"]),
            cpu_host("c", &["avx2"]),
        ];
        let mut v = vm("x", "a", 100);
        v.cpu = Some(req(&["avx2"]));
        let p = plan_drain("a", &hosts, &[v], &[], R1, vec![]);
        assert_eq!(act(&p, "x").target.as_deref(), Some("c"));
        assert!(act(&p, "x").warnings.is_empty());
    }

    #[test]
    fn unknown_target_cpu_is_a_warning_not_a_blocker_and_verified_wins() {
        let hosts = [host("a", 8, 8192), host("b", 8, 99999)];
        let mut v = vm("x", "a", 100);
        v.cpu = Some(req(&["avx2"]));
        let p = plan_drain("a", &hosts, &[v.clone()], &[], R1, vec![]);
        assert!(p.can_drain);
        assert!(act(&p, "x").warnings[0].contains("not verified"));
        // a verified host beats a roomier unverified one
        let hosts = [
            host("a", 8, 8192),
            host("b", 8, 99999),
            cpu_host("c", &["avx2"]),
        ];
        let p = plan_drain("a", &hosts, &[v], &[], R1, vec![]);
        assert_eq!(act(&p, "x").target.as_deref(), Some("c"));
    }

    #[test]
    fn cpu_incompatible_with_migrate_if_possible_stops_the_vm() {
        let hosts = [cpu_host("a", &["avx2"]), cpu_host("b", &[])];
        let mut v = vm("x", "a", 100);
        v.cpu = Some(req(&["avx2"]));
        v.eviction = Eviction::MigrateIfPossible;
        let p = plan_drain("a", &hosts, &[v], &[], R1, vec![]);
        assert_eq!(act(&p, "x").action, "stop");
        assert!(act(&p, "x").warnings[0].contains("cpu_incompatible"));
    }

    #[test]
    fn eviction_parse() {
        assert_eq!(Eviction::parse("pin"), Some(Eviction::Pin));
        assert_eq!(Eviction::parse("nope"), None);
    }
}
