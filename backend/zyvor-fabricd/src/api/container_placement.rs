// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: LicenseRef-Zyvor-Production-2.0

//! Host placement for `ContainerGroup` workloads. Deliberately reuses the
//! same live infrastructure VM placement already has — `datacenter::HostInfo`
//! (heartbeat-updated, liveness-checked every 60s by `run_stale_host_detector`
//! in `server.rs`) and `predictive_drs::DrsManager` (the generic bin-packing
//! scorer already exposed for VMs at `POST /api/drs/placement`) — rather than
//! inventing a second scheduler. The only gap this fills: `compute_placement`
//! reads a `host_snapshots` collection nothing in this codebase populates
//! today, so snapshots are built here directly from the live `HostInfo`
//! registry instead.

use datacenter::{HostInfo, HostStatus};
use predictive_drs::{DrsManager, HostSnapshot, PlacementRequest};

use super::container_declarative::ContainerGroupSpec;
use crate::server::AppState;

pub struct PlacedHost {
    pub id: String,
    pub hostname: String,
}

/// Picks the host a new ContainerGroup's Pods should be pinned to.
/// `node_hint`, if set, short-circuits straight to that host (matched by id
/// or hostname) without scoring — affinity-rule cross-referencing via
/// `predictive_drs::AffinityRule` is a deferred v2 (that engine keys
/// placement decisions on `vm_names` today, not container group names).
pub fn place_container_group(
    state: &AppState,
    spec: &ContainerGroupSpec,
) -> Result<PlacedHost, String> {
    let hosts: Vec<HostInfo> = state.store.list_entities("hosts").unwrap_or_default();

    if let Some(hint) = spec.placement.node_hint.as_deref() {
        return hosts
            .into_iter()
            .find(|h| h.id == hint || h.hostname == hint)
            .map(|h| PlacedHost {
                id: h.id,
                hostname: h.hostname,
            })
            .ok_or_else(|| format!("node_hint '{hint}' does not match any registered host"));
    }

    let capable: Vec<&HostInfo> = secure_containers_capable_hosts(&hosts);

    if capable.is_empty() {
        return Err(
            "no Secure-Containers-capable host is currently connected (register one via \
             POST /api/hosts, or heartbeat secure_containers_ready=true)"
                .to_string(),
        );
    }

    let snapshots: Vec<HostSnapshot> = capable
        .iter()
        .map(|h| {
            let total_cpu_mhz = h.cpus as u64 * 1000;
            let used_cpu_mhz =
                ((h.cpu_usage_pct.clamp(0.0, 100.0) / 100.0) * total_cpu_mhz as f64) as u64;
            let used_memory_mb =
                ((h.memory_usage_pct.clamp(0.0, 100.0) / 100.0) * h.memory_mb as f64) as u64;
            HostSnapshot {
                host_id: h.id.clone(),
                hostname: h.hostname.clone(),
                total_cpu_mhz,
                used_cpu_mhz,
                total_memory_mb: h.memory_mb,
                used_memory_mb,
                // Ephemeral container storage isn't tracked per-host yet;
                // requesting 0 disk_gb below keeps this filter a no-op
                // rather than silently misreporting capacity.
                total_disk_gb: 0,
                used_disk_gb: 0,
                vm_names: Vec::new(),
            }
        })
        .collect();

    let (cpus, memory_mb) = spec.total_resources();

    let request = PlacementRequest {
        vm_name: spec.name.clone(),
        cpus,
        memory_mb,
        disk_gb: 0,
        strategy: None,
        affinity_rules: Vec::new(),
    };

    let mgr = DrsManager::new();
    mgr.compute_placement(&snapshots, &request)
        .map(|result| PlacedHost {
            id: result.host_id,
            hostname: result.host_name,
        })
        .map_err(|e| e.to_string())
}

/// Hosts eligible for ContainerGroup placement.
pub(crate) fn secure_containers_capable_hosts(hosts: &[HostInfo]) -> Vec<&HostInfo> {
    hosts
        .iter()
        .filter(|h| h.status == HostStatus::Connected && h.secure_containers_ready)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use datacenter::SecureContainersDetail;

    fn host(id: &str, status: HostStatus, ready: bool) -> HostInfo {
        HostInfo {
            id: id.into(),
            hostname: id.into(),
            address: "127.0.0.1".into(),
            cluster_id: "c1".into(),
            datacenter_id: "d1".into(),
            cpus: 4,
            memory_mb: 8192,
            status,
            last_heartbeat: Utc::now(),
            vm_count: 0,
            cpu_usage_pct: 0.0,
            memory_usage_pct: 0.0,
            agent_version: "t".into(),
            created_at: Utc::now(),
            updated_at: Utc::now(),
            secure_containers_ready: ready,
            secure_containers: ready.then_some(SecureContainersDetail {
                available: true,
                shim_installed: true,
                guest_image_present: true,
            }),
        }
    }

    #[test]
    fn capable_hosts_require_connected_and_ready() {
        let hosts = vec![
            host("a", HostStatus::Connected, true),
            host("b", HostStatus::Connected, false),
            host("c", HostStatus::Disconnected, true),
            host("d", HostStatus::Maintenance, true),
        ];
        let capable = secure_containers_capable_hosts(&hosts);
        assert_eq!(capable.len(), 1);
        assert_eq!(capable[0].id, "a");
    }
}
