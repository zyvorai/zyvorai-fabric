// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: LicenseRef-Zyvor-Production-2.0

//! Filter-then-score placement across registered inference nodes.
//!
//! An empty node list means "use the local FluxVM inventory", which is the
//! single-host path. A heartbeat older than `HEARTBEAT_TTL_SECS` is Offline.

use super::types::{InferenceNode, NodeGpu, NodeState};

pub const HEARTBEAT_TTL_SECS: i64 = 60;

#[derive(Debug, Clone)]
pub struct ScheduleRequest {
    pub vendor: String,
    pub minimum_vram_gib: u32,
    pub preferred_site: Option<String>,
    pub residency: Option<String>,
    pub model: String,
    pub cpu: u32,
    pub memory_gib: u32,
    /// `node_id/bdf` already claimed by a replica.
    pub allocated: Vec<(String, String)>,
    /// Failure domains that already host a replica of this deployment.
    pub occupied_domains: Vec<String>,
    /// Empty means every site is allowed.
    pub allowed_sites: Vec<String>,
    /// `0` means no per-site cap.
    pub max_replicas_per_site: u32,
    pub site_counts: Vec<(String, u32)>,
    /// Empty means a full GPU, not a MIG slice.
    pub mig_profile: String,
    /// When true, only NVLink-attached devices are eligible.
    pub require_nvlink: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct ScheduleChoice {
    pub node_id: String,
    pub bdf: String,
    pub score: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Placement {
    /// No nodes are registered. The caller uses local FluxVM inventory.
    Legacy,
    Chosen(ScheduleChoice),
}

pub fn effective_state(node: &InferenceNode, now_unix: i64) -> NodeState {
    if node.heartbeat_unix > 0 && now_unix.saturating_sub(node.heartbeat_unix) > HEARTBEAT_TTL_SECS
    {
        return NodeState::Offline;
    }
    node.state
}

pub fn decide(
    nodes: &[InferenceNode],
    req: &ScheduleRequest,
    now_unix: i64,
) -> Result<Placement, String> {
    if nodes.is_empty() {
        return Ok(Placement::Legacy);
    }
    schedule(nodes, req, now_unix).map(Placement::Chosen)
}

pub fn schedule(
    nodes: &[InferenceNode],
    req: &ScheduleRequest,
    now_unix: i64,
) -> Result<ScheduleChoice, String> {
    let mut best: Option<ScheduleChoice> = None;
    for node in nodes {
        if let Ok(choice) = consider(node, req, now_unix) {
            let replace = best.as_ref().is_none_or(|cur| {
                choice.score > cur.score
                    || (choice.score == cur.score && choice.node_id < cur.node_id)
            });
            if replace {
                best = Some(choice);
            }
        }
    }
    best.ok_or_else(|| "no eligible inference node".into())
}

/// Why each node was skipped, plus the winner. Same rules as `schedule`.
pub fn explain(
    nodes: &[InferenceNode],
    req: &ScheduleRequest,
    now_unix: i64,
) -> (Option<ScheduleChoice>, Vec<(String, String)>) {
    let mut rejected = Vec::new();
    let mut best: Option<ScheduleChoice> = None;
    for node in nodes {
        match consider(node, req, now_unix) {
            Ok(choice) => {
                let replace = best.as_ref().is_none_or(|cur| {
                    choice.score > cur.score
                        || (choice.score == cur.score && choice.node_id < cur.node_id)
                });
                if replace {
                    best = Some(choice);
                }
            }
            Err(reason) => rejected.push((node.id.clone(), reason)),
        }
    }
    (best, rejected)
}

fn consider(
    node: &InferenceNode,
    req: &ScheduleRequest,
    now_unix: i64,
) -> Result<ScheduleChoice, String> {
    if effective_state(node, now_unix) != NodeState::Ready {
        return Err("node is not ready".into());
    }
    if !node.taints.is_empty() {
        return Err("node is tainted".into());
    }
    if !req.allowed_sites.is_empty() && !req.allowed_sites.iter().any(|s| s == &node.site) {
        return Err("site is not allowed".into());
    }
    if req.max_replicas_per_site > 0 {
        let placed = req
            .site_counts
            .iter()
            .find(|(site, _)| site == &node.site)
            .map(|(_, n)| *n)
            .unwrap_or(0);
        if placed >= req.max_replicas_per_site {
            return Err("site is at max_replicas_per_site".into());
        }
    }
    if let Some(residency) = req.residency.as_deref().filter(|s| !s.is_empty()) {
        if node.site != residency {
            return Err("site is outside residency".into());
        }
    }
    if node.cpu_free > 0 && node.cpu_free < req.cpu {
        return Err("not enough free CPU".into());
    }
    if node.memory_gib_free > 0 && node.memory_gib_free < req.memory_gib {
        return Err("not enough free memory".into());
    }
    let gpu = free_gpu(node, req).ok_or_else(|| "no free matching GPU".to_string())?;
    let mut score = i64::from(gpu.vram_gib);
    if node.cached_models.iter().any(|m| m == &req.model) {
        score += 1_000;
    }
    if req
        .preferred_site
        .as_deref()
        .is_some_and(|site| !site.is_empty() && site == node.site)
    {
        score += 100;
    }
    if !node.failure_domain.is_empty()
        && !req
            .occupied_domains
            .iter()
            .any(|d| d == &node.failure_domain)
    {
        score += 50;
    }
    Ok(ScheduleChoice {
        node_id: node.id.clone(),
        bdf: gpu.bdf.clone(),
        score,
    })
}

fn free_gpu<'a>(node: &'a InferenceNode, req: &ScheduleRequest) -> Option<&'a NodeGpu> {
    let vendor = req.vendor.trim().to_ascii_lowercase();
    node.gpus.iter().find(|gpu| {
        gpu.vendor.eq_ignore_ascii_case(&vendor)
            && gpu.vram_gib >= req.minimum_vram_gib
            && gpu.healthy
            && (!req.require_nvlink || gpu.nvlink)
            && (req.mig_profile.is_empty() || gpu.mig_profile == req.mig_profile)
            && !node.gpus.iter().any(|other| other.parent_bdf == gpu.bdf)
            && !req
                .allocated
                .iter()
                .any(|(id, bdf)| id == &node.id && bdf.eq_ignore_ascii_case(&gpu.bdf))
    })
}

/// True when `host` is offline or gone and some other node is Ready.
pub fn should_replace_lost(nodes: &[InferenceNode], host: &str, now_unix: i64) -> bool {
    if host.is_empty() || nodes.is_empty() {
        return false;
    }
    let lost = nodes
        .iter()
        .find(|n| n.id == host)
        .map(|n| effective_state(n, now_unix) == NodeState::Offline)
        .unwrap_or(true);
    if !lost {
        return false;
    }
    nodes
        .iter()
        .any(|n| n.id != host && effective_state(n, now_unix) == NodeState::Ready)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn node(id: &str, site: &str, domain: &str, vram: u32, cached: bool) -> InferenceNode {
        InferenceNode {
            id: id.into(),
            site: site.into(),
            failure_domain: domain.into(),
            state: NodeState::Ready,
            heartbeat_unix: 1_000,
            gpus: vec![NodeGpu {
                bdf: format!("{id}-gpu"),
                vendor: "nvidia".into(),
                vram_gib: vram,
                model: "L40S".into(),
                healthy: true,
                mig_profile: String::new(),
                parent_bdf: String::new(),
                nvlink: true,
                temperature_c: 0,
                power_watts: 0,
                ecc_errors: 0,
            }],
            taints: vec![],
            cpu_free: 16,
            memory_gib_free: 64,
            cached_models: if cached { vec!["qwen".into()] } else { vec![] },
        }
    }

    fn req() -> ScheduleRequest {
        ScheduleRequest {
            vendor: "nvidia".into(),
            minimum_vram_gib: 24,
            preferred_site: Some("pune".into()),
            residency: None,
            model: "qwen".into(),
            cpu: 4,
            memory_gib: 16,
            allocated: vec![],
            occupied_domains: vec![],
            allowed_sites: vec![],
            max_replicas_per_site: 0,
            site_counts: vec![],
            mig_profile: String::new(),
            require_nvlink: false,
        }
    }

    #[test]
    fn empty_inventory_stays_on_the_local_host() {
        assert_eq!(decide(&[], &req(), 1_010).unwrap(), Placement::Legacy);
    }

    #[test]
    fn stale_heartbeat_is_offline_and_not_scheduled() {
        let mut n = node("a", "pune", "rack1", 48, false);
        assert_eq!(effective_state(&n, 1_000 + 61), NodeState::Offline);
        assert!(schedule(&[n.clone()], &req(), 1_000 + 61).is_err());
        n.heartbeat_unix = 1_050;
        assert_eq!(schedule(&[n], &req(), 1_060).unwrap().node_id, "a");
    }

    #[test]
    fn cache_and_site_beat_a_larger_remote_gpu() {
        let local = node("local", "pune", "rack1", 24, true);
        let remote = node("remote", "blr", "rack9", 80, false);
        let choice = schedule(&[remote, local], &req(), 1_010).unwrap();
        assert_eq!(choice.node_id, "local");
    }

    #[test]
    fn residency_excludes_other_sites() {
        let mut request = req();
        request.residency = Some("pune".into());
        let remote = node("remote", "blr", "rack9", 80, true);
        assert!(schedule(&[remote], &request, 1_010).is_err());
    }

    #[test]
    fn taint_and_allocated_gpu_are_filtered() {
        let mut tainted = node("a", "pune", "rack1", 48, false);
        tainted.taints = vec!["dedicated".into()];
        assert!(schedule(&[tainted], &req(), 1_010).is_err());
        let free = node("b", "pune", "rack1", 48, false);
        let mut request = req();
        request.allocated = vec![("b".into(), "b-gpu".into())];
        assert!(schedule(&[free], &request, 1_010).is_err());
    }

    #[test]
    fn lost_node_is_replaced_only_when_another_is_ready() {
        let mut dead = node("dead", "pune", "rack1", 48, false);
        dead.heartbeat_unix = 1;
        let live = node("live", "pune", "rack2", 48, false);
        assert!(should_replace_lost(&[dead.clone(), live], "dead", 1_000));
        assert!(!should_replace_lost(&[dead], "dead", 1_000));
        assert!(!should_replace_lost(&[], "", 1_000));
    }

    #[test]
    fn allowed_site_cap_and_explain_agree() {
        let local = node("local", "pune", "rack1", 48, false);
        let mut request = req();
        request.allowed_sites = vec!["pune".into()];
        request.max_replicas_per_site = 1;
        request.site_counts = vec![("pune".into(), 1)];
        let (chosen, rejected) = explain(&[local], &request, 1_010);
        assert!(chosen.is_none());
        assert_eq!(rejected[0].1, "site is at max_replicas_per_site");
    }
}
