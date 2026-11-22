// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: LicenseRef-Zyvor-Production-2.0

//! Capacity what-if simulator.
//!
//! `POST /api/capacity/whatif` answers "if I add these VMs, which fit?"
//! without creating, reserving or changing anything. The maths lives in the
//! pure [`fit_vms`] function; `web/src/lib/capacity.ts` is a line-for-line
//! mirror and both are tested against `web/src/lib/fixtures/capacity-whatif.json`.

use axum::{extract::State, http::StatusCode, Json};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::sync::Arc;

use super::resource_policy::{num_cpus, total_memory_mb, OvercommitPolicy};
use crate::server::AppState;
use security::RequireRead;

/// Largest number of VM entries / expanded instances one request may simulate.
pub const MAX_ENTRIES: usize = 200;
pub const MAX_INSTANCES: u64 = 5000;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Host {
    pub name: String,
    pub vcpus: u64,
    pub memory_mb: u64,
    /// `None` means disk capacity is unknown and is not checked.
    pub disk_gb: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ExistingVm {
    /// Host the VM is charged to. `None` (or an unknown name) charges the first host.
    pub host: Option<String>,
    pub vcpus: u64,
    pub memory_mb: u64,
    pub disk_gb: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct NewVmSpec {
    pub name: String,
    pub vcpus: u64,
    pub memory_mb: u64,
    pub disk_gb: u64,
    pub count: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Overcommit {
    pub cpu_ratio: f64,
    pub memory_ratio: f64,
    pub storage_ratio: f64,
}

impl Overcommit {
    pub fn none() -> Self {
        Self {
            cpu_ratio: 1.0,
            memory_ratio: 1.0,
            storage_ratio: 1.0,
        }
    }
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct Usage {
    pub vcpus: u64,
    pub memory_mb: u64,
    pub disk_gb: u64,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct Room {
    pub vcpus: u64,
    pub memory_mb: u64,
    pub disk_gb: Option<u64>,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct Placement {
    pub name: String,
    pub index: u32,
    pub host: String,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct Unplaced {
    pub name: String,
    pub index: u32,
    /// Any of `vcpu`, `memory`, `disk` (no host has enough of that resource),
    /// `no_single_host` (each resource fits somewhere, but not all on one host)
    /// or `no_hosts`.
    pub reasons: Vec<String>,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct HostReport {
    pub name: String,
    pub effective: Room,
    pub used_before: Usage,
    pub used_after: Usage,
    pub headroom_after: Room,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct FitReport {
    pub requested: u64,
    pub fitted: u64,
    pub all_fit: bool,
    pub placements: Vec<Placement>,
    pub unplaced: Vec<Unplaced>,
    pub hosts: Vec<HostReport>,
}

fn scale(total: u64, ratio: f64) -> u64 {
    (total as f64 * ratio).floor() as u64
}

/// Simulate placing `new_vms` onto `hosts` with first-fit-decreasing.
///
/// VMs are ordered by memory, then vCPUs, then disk (all descending), then
/// name and instance index, so the result is deterministic. Each goes on the
/// first host (in the given order) with room for all three resources.
/// Effective capacity is `floor(total * ratio)`, matching `get_capacity`.
pub fn fit_vms(
    hosts: &[Host],
    existing_vms: &[ExistingVm],
    new_vms: &[NewVmSpec],
    overcommit: &Overcommit,
) -> FitReport {
    let eff: Vec<Room> = hosts
        .iter()
        .map(|h| Room {
            vcpus: scale(h.vcpus, overcommit.cpu_ratio),
            memory_mb: scale(h.memory_mb, overcommit.memory_ratio),
            disk_gb: h.disk_gb.map(|d| scale(d, overcommit.storage_ratio)),
        })
        .collect();
    let mut used: Vec<Usage> = hosts
        .iter()
        .map(|_| Usage {
            vcpus: 0,
            memory_mb: 0,
            disk_gb: 0,
        })
        .collect();
    for vm in existing_vms {
        if hosts.is_empty() {
            break;
        }
        let i = vm
            .host
            .as_ref()
            .and_then(|n| hosts.iter().position(|h| &h.name == n))
            .unwrap_or(0);
        used[i].vcpus += vm.vcpus;
        used[i].memory_mb += vm.memory_mb;
        used[i].disk_gb += vm.disk_gb;
    }
    let used_before = used.clone();

    let mut queue: Vec<(&NewVmSpec, u32)> = new_vms
        .iter()
        .flat_map(|s| (1..=s.count).map(move |i| (s, i)))
        .collect();
    queue.sort_by(|(a, ai), (b, bi)| {
        b.memory_mb
            .cmp(&a.memory_mb)
            .then(b.vcpus.cmp(&a.vcpus))
            .then(b.disk_gb.cmp(&a.disk_gb))
            .then(a.name.cmp(&b.name))
            .then(ai.cmp(bi))
    });
    let requested = queue.len() as u64;

    let free = |i: usize, used: &[Usage]| -> (u64, u64, Option<u64>) {
        (
            eff[i].vcpus.saturating_sub(used[i].vcpus),
            eff[i].memory_mb.saturating_sub(used[i].memory_mb),
            eff[i].disk_gb.map(|d| d.saturating_sub(used[i].disk_gb)),
        )
    };

    let mut placements = Vec::new();
    let mut unplaced = Vec::new();
    for (spec, index) in queue {
        let target = (0..hosts.len()).find(|&i| {
            let (c, m, d) = free(i, &used);
            spec.vcpus <= c && spec.memory_mb <= m && d.is_none_or(|d| spec.disk_gb <= d)
        });
        match target {
            Some(i) => {
                used[i].vcpus += spec.vcpus;
                used[i].memory_mb += spec.memory_mb;
                used[i].disk_gb += spec.disk_gb;
                placements.push(Placement {
                    name: spec.name.clone(),
                    index,
                    host: hosts[i].name.clone(),
                });
            }
            None => {
                let mut reasons = Vec::new();
                if hosts.is_empty() {
                    reasons.push("no_hosts".to_string());
                } else {
                    let all = |f: &dyn Fn(usize) -> bool| (0..hosts.len()).all(f);
                    if all(&|i| spec.vcpus > free(i, &used).0) {
                        reasons.push("vcpu".to_string());
                    }
                    if all(&|i| spec.memory_mb > free(i, &used).1) {
                        reasons.push("memory".to_string());
                    }
                    if all(&|i| free(i, &used).2.is_some_and(|d| spec.disk_gb > d)) {
                        reasons.push("disk".to_string());
                    }
                    if reasons.is_empty() {
                        reasons.push("no_single_host".to_string());
                    }
                }
                unplaced.push(Unplaced {
                    name: spec.name.clone(),
                    index,
                    reasons,
                });
            }
        }
    }

    let host_reports = (0..hosts.len())
        .map(|i| {
            let (c, m, d) = free(i, &used);
            HostReport {
                name: hosts[i].name.clone(),
                effective: eff[i].clone(),
                used_before: used_before[i].clone(),
                used_after: used[i].clone(),
                headroom_after: Room {
                    vcpus: c,
                    memory_mb: m,
                    disk_gb: d,
                },
            }
        })
        .collect();

    FitReport {
        requested,
        fitted: placements.len() as u64,
        all_fit: unplaced.is_empty(),
        placements,
        unplaced,
        hosts: host_reports,
    }
}

// ============================================================================
// HTTP handler
// ============================================================================

#[derive(Debug, Deserialize)]
pub struct WhatIfRequest {
    pub vms: Vec<NewVmSpec>,
    /// Total VM disk capacity in GB. Omit when unknown: disk is then not checked.
    #[serde(default)]
    pub disk_gb_total: Option<u64>,
}

type ApiErr = (StatusCode, Json<serde_json::Value>);

fn bad(msg: &str) -> ApiErr {
    (StatusCode::BAD_REQUEST, Json(json!({ "error": msg })))
}

pub fn validate_request(req: &WhatIfRequest) -> Result<(), &'static str> {
    if req.vms.is_empty() {
        return Err("vms must not be empty");
    }
    if req.vms.len() > MAX_ENTRIES {
        return Err("too many vms entries (max 200)");
    }
    let mut total: u64 = 0;
    for v in &req.vms {
        if v.name.trim().is_empty() || v.name.len() > 128 {
            return Err("each vm needs a name of 1-128 characters");
        }
        if v.count == 0 {
            return Err("count must be at least 1");
        }
        if v.vcpus == 0 || v.memory_mb == 0 {
            return Err("vcpus and memory_mb must be at least 1");
        }
        total += u64::from(v.count);
    }
    if total > MAX_INSTANCES {
        return Err("too many VM instances in one request (max 5000)");
    }
    Ok(())
}

/// POST /api/capacity/whatif - simulate adding VMs; read-only.
pub async fn capacity_whatif(
    RequireRead(_claims): RequireRead,
    State(state): State<Arc<AppState>>,
    Json(req): Json<WhatIfRequest>,
) -> Result<Json<FitReport>, ApiErr> {
    validate_request(&req).map_err(bad)?;

    let ise = |e: String| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": e })),
        )
    };
    // Same sources as get_capacity: overcommit policy, stored VMs, host cpu/memory.
    let policy = state
        .store
        .get_entity::<OvercommitPolicy>("config", "overcommit")
        .map_err(|e| ise(e.to_string()))?
        .unwrap_or_default();
    let vms = state.store.list_vms().map_err(|e| ise(e.to_string()))?;

    let host = Host {
        name: "local".to_string(),
        vcpus: u64::from(num_cpus().await),
        memory_mb: total_memory_mb().await,
        disk_gb: req.disk_gb_total,
    };
    let existing: Vec<ExistingVm> = vms
        .iter()
        .map(|v| ExistingVm {
            host: None,
            vcpus: u64::from(v.cpus),
            memory_mb: v.memory,
            disk_gb: v.disk,
        })
        .collect();
    let overcommit = if policy.enabled {
        Overcommit {
            cpu_ratio: policy.cpu_ratio,
            memory_ratio: policy.memory_ratio,
            storage_ratio: policy.storage_ratio,
        }
    } else {
        Overcommit::none()
    };

    Ok(Json(fit_vms(&[host], &existing, &req.vms, &overcommit)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::Value;

    #[derive(Deserialize)]
    struct Input {
        hosts: Vec<Host>,
        existing: Vec<ExistingVm>,
        new_vms: Vec<NewVmSpec>,
        overcommit: Overcommit,
    }

    #[test]
    fn shared_fixtures_match() {
        let path = concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../web/src/lib/fixtures/capacity-whatif.json"
        );
        let raw = std::fs::read_to_string(path).expect("fixture file");
        let doc: Value = serde_json::from_str(&raw).unwrap();
        let cases = doc["cases"].as_array().unwrap();
        assert!(cases.len() >= 5);
        for case in cases {
            let input: Input = serde_json::from_value(case["input"].clone()).unwrap();
            let got = fit_vms(
                &input.hosts,
                &input.existing,
                &input.new_vms,
                &input.overcommit,
            );
            assert_eq!(
                serde_json::to_value(&got).unwrap(),
                case["expected"],
                "case: {}",
                case["name"]
            );
        }
    }

    fn host(vcpus: u64, mem: u64) -> Host {
        Host {
            name: "h".into(),
            vcpus,
            memory_mb: mem,
            disk_gb: Some(100),
        }
    }

    fn spec(name: &str, count: u32) -> NewVmSpec {
        NewVmSpec {
            name: name.into(),
            vcpus: 1,
            memory_mb: 1024,
            disk_gb: 10,
            count,
        }
    }

    #[test]
    fn no_hosts_reports_no_hosts() {
        let r = fit_vms(&[], &[], &[spec("a", 1)], &Overcommit::none());
        assert!(!r.all_fit);
        assert_eq!(r.unplaced[0].reasons, vec!["no_hosts"]);
    }

    #[test]
    fn deterministic_regardless_of_input_order() {
        let a = [spec("a", 2), spec("b", 2)];
        let b = [spec("b", 2), spec("a", 2)];
        let h = [host(4, 4096)];
        assert_eq!(
            fit_vms(&h, &[], &a, &Overcommit::none()),
            fit_vms(&h, &[], &b, &Overcommit::none())
        );
    }

    #[test]
    fn inputs_are_not_modified_and_report_is_pure() {
        let h = [host(2, 2048)];
        let first = fit_vms(&h, &[], &[spec("a", 5)], &Overcommit::none());
        let second = fit_vms(&h, &[], &[spec("a", 5)], &Overcommit::none());
        assert_eq!(first, second);
        assert_eq!(first.fitted, 2);
        assert_eq!(first.requested, 5);
    }

    #[test]
    fn validation_rejects_bad_requests() {
        let ok = WhatIfRequest {
            vms: vec![spec("a", 1)],
            disk_gb_total: None,
        };
        assert!(validate_request(&ok).is_ok());
        let empty = WhatIfRequest {
            vms: vec![],
            disk_gb_total: None,
        };
        assert!(validate_request(&empty).is_err());
        let zero = WhatIfRequest {
            vms: vec![spec("a", 0)],
            disk_gb_total: None,
        };
        assert!(validate_request(&zero).is_err());
        let huge = WhatIfRequest {
            vms: (0..6).map(|_| spec("a", 1000)).collect(),
            disk_gb_total: None,
        };
        assert!(validate_request(&huge).is_err());
    }
}
