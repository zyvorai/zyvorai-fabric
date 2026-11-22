// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: LicenseRef-Zyvor-Production-2.0

//! Site advertisement and residency-safe routing (Phase 11 preview).
//!
//! A site outside residency is never a failover target. When the preferred
//! site is saturated, traffic spills only to `failover_sites`.

use super::types::InferenceNode;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct AiSite {
    pub id: String,
    #[serde(default)]
    pub residency: String,
    #[serde(default)]
    pub latency_ms: u32,
    #[serde(default)]
    pub cost_class: u32,
    #[serde(default)]
    pub gpu_free: u32,
    #[serde(default = "default_reachable")]
    pub reachable: bool,
    #[serde(default)]
    pub models: Vec<String>,
    #[serde(default)]
    pub last_sync_unix: i64,
    #[serde(default)]
    pub generation: u64,
    /// fabricd base URL of this site. Empty means bytes are not copied.
    #[serde(default)]
    pub peer_url: String,
}

fn default_reachable() -> bool {
    true
}

#[derive(Debug, Clone)]
pub struct SitePolicy<'a> {
    pub residency: Option<&'a str>,
    pub allowed: &'a [String],
    pub preferred: Option<&'a str>,
    pub failover: &'a [String],
}

pub fn eligible(site: &AiSite, policy: &SitePolicy<'_>) -> bool {
    if !site.reachable || site.gpu_free == 0 {
        return false;
    }
    if let Some(residency) = policy.residency.filter(|s| !s.is_empty()) {
        if site.residency != residency {
            return false;
        }
    }
    policy.allowed.is_empty() || policy.allowed.iter().any(|id| id == &site.id)
}

/// Pick a site. Saturated preferred sites spill only to listed failovers.
pub fn pick_site(
    policy: &SitePolicy<'_>,
    sites: &[AiSite],
    preferred_saturated: bool,
) -> Result<String, String> {
    if !preferred_saturated {
        if let Some(preferred) = policy.preferred.filter(|s| !s.is_empty()) {
            if sites
                .iter()
                .any(|site| site.id == preferred && eligible(site, policy))
            {
                return Ok(preferred.to_string());
            }
        }
    }
    let failover = sites
        .iter()
        .find(|site| policy.failover.iter().any(|id| id == &site.id) && eligible(site, policy));
    failover
        .map(|site| site.id.clone())
        .ok_or_else(|| "no site inside residency and failover policy".into())
}

pub fn meets_minimum_sites(distinct: usize, minimum: u32) -> bool {
    minimum == 0 || distinct >= minimum as usize
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct EdgeSnapshot {
    pub generation: u64,
    pub model_digest: String,
    pub fail_closed_unix: i64,
}

/// A disconnected site may serve the last snapshot until the fail-closed time.
pub fn edge_may_serve(snapshot: &EdgeSnapshot, now_unix: i64) -> bool {
    snapshot.generation > 0 && now_unix < snapshot.fail_closed_unix
}

pub fn distinct_node_sites(nodes: &[InferenceNode]) -> usize {
    let mut sites: Vec<&str> = nodes
        .iter()
        .map(|n| n.site.as_str())
        .filter(|s| !s.is_empty())
        .collect();
    sites.sort_unstable();
    sites.dedup();
    sites.len()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn site(id: &str, residency: &str, gpu: u32) -> AiSite {
        AiSite {
            id: id.into(),
            residency: residency.into(),
            latency_ms: 10,
            cost_class: 1,
            gpu_free: gpu,
            reachable: true,
            models: vec!["qwen".into()],
            last_sync_unix: 1,
            generation: 3,
            peer_url: String::new(),
        }
    }

    #[test]
    fn spillover_stays_inside_residency_and_failover() {
        let sites = vec![
            site("pune-1", "india", 0),
            site("bhubaneswar-1", "india", 2),
            site("frankfurt-1", "eu", 8),
        ];
        let allowed = vec!["pune-1".into(), "bhubaneswar-1".into()];
        let failover = vec!["bhubaneswar-1".into(), "frankfurt-1".into()];
        let policy = SitePolicy {
            residency: Some("india"),
            allowed: &allowed,
            preferred: Some("pune-1"),
            failover: &failover,
        };
        assert_eq!(pick_site(&policy, &sites, true).unwrap(), "bhubaneswar-1");
        assert!(pick_site(&policy, &sites[..1], true).is_err());
    }

    #[test]
    fn edge_fails_closed_after_the_snapshot_expires() {
        let snap = EdgeSnapshot {
            generation: 4,
            model_digest: "abc".into(),
            fail_closed_unix: 1_000,
        };
        assert!(edge_may_serve(&snap, 999));
        assert!(!edge_may_serve(&snap, 1_000));
        assert!(meets_minimum_sites(2, 2));
        assert!(!meets_minimum_sites(1, 2));
    }
}
