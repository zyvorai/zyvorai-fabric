// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: Apache-2.0

//! ClusterMesh datapath lifecycle v2: cross-site remote backend / endpoint mesh.
//!
//! Fabric owns a durable catalog keyed by
//! `(route_domain, service, vip_or_*, address:port)`. Reconcile merges
//! same-domain **Ready** and active **Draining** remotes into FluxVM Maglev
//! service upserts on owning-domain nodes (local backends preserved).
//! Unhealthy remotes stay catalogued but are skipped. Expired drains
//! (`drain_until_unix_ms` in the past) are skipped when `now_unix_ms > 0`.
//!
//! Optional `vip` selects Maglev targets by VIP (multi-VIP / distinct Maglev
//! service names per VIP). Geneve/VXLAN tunnels remain out of scope.

use crate::{
    domain_key, BackendState, EdgeLease, NodeTarget, ServiceBackend, ServiceNodeClient, ServiceSpec,
};
use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, HashSet},
    fs,
    net::IpAddr,
    path::{Path, PathBuf},
};

const MAX_LABELS: usize = 64;
const MAX_WEIGHT: u32 = 32;
const DEFAULT_DRAIN_UPSERT_MS: u64 = 60_000;

/// Published remote (peer-site) backend for a service in a route domain.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RemoteBackend {
    pub service: String,
    #[serde(default = "default_site")]
    pub site_id: String,
    #[serde(default = "default_route_domain")]
    pub route_domain: String,
    /// When set, Maglev merge matches by VIP (service name ignored for target
    /// selection). When unset, match by Maglev service name (back-compat).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub vip: Option<String>,
    /// Backend IP as string (IPv4/IPv6); parsed on merge.
    pub address: String,
    pub port: u16,
    /// Maglev weight (1..=32). Operators typically lower weight when starting
    /// drain for weighted handoff.
    #[serde(default = "default_remote_weight")]
    pub weight: u32,
    #[serde(default)]
    pub state: BackendState,
    /// Required for Draining inject; set on upsert/begin_drain when omitted.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub drain_until_unix_ms: Option<u64>,
    #[serde(default)]
    pub labels: BTreeMap<String, String>,
    /// Filled by Fabric on upsert when omitted / zero.
    #[serde(default)]
    pub updated_unix_ms: u64,
}

fn default_site() -> String {
    "default".into()
}
fn default_route_domain() -> String {
    "default".into()
}
fn default_remote_weight() -> u32 {
    1
}

/// Canonical catalog key segment for `(address, port)`.
pub fn backend_key(address: &str, port: u16) -> String {
    format!("{address}:{port}")
}

fn vip_key_segment(vip: Option<&str>) -> &str {
    match vip {
        Some(v) if !v.is_empty() => v,
        _ => "*",
    }
}

fn directory_key(
    route_domain: &str,
    service: &str,
    vip: Option<&str>,
    address: &str,
    port: u16,
) -> String {
    format!(
        "{route_domain}|{service}|{}|{}",
        vip_key_segment(vip),
        backend_key(address, port)
    )
}

fn applied_key(route_domain: &str, service: &str) -> String {
    format!("{route_domain}|{service}")
}

fn matches_endpoint(
    b: &RemoteBackend,
    route_domain: &str,
    service: &str,
    address: &str,
    port: u16,
) -> bool {
    b.route_domain == route_domain && b.service == service && b.address == address && b.port == port
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct RemoteBackendState {
    pub generation: u64,
    #[serde(default)]
    pub backends: BTreeMap<String, RemoteBackend>,
    /// Backend keys last injected per `(route_domain|service)` so a delete +
    /// reconcile can strip stale remotes from node service upserts.
    #[serde(default)]
    pub applied: BTreeMap<String, Vec<String>>,
}

/// Durable Fabric-owned remote backend catalog.
#[derive(Debug, Clone)]
pub struct RemoteBackendDirectory {
    path: PathBuf,
}

impl RemoteBackendDirectory {
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self { path: path.into() }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn load(&self) -> Result<RemoteBackendState> {
        if !self.path.exists() {
            return Ok(RemoteBackendState::default());
        }
        let raw = fs::read(&self.path)
            .with_context(|| format!("reading remote backend store {}", self.path.display()))?;
        serde_json::from_slice(&raw)
            .with_context(|| format!("parsing remote backend store {}", self.path.display()))
    }

    pub fn save(&self, state: &RemoteBackendState) -> Result<()> {
        crate::atomic_write(&self.path, &serde_json::to_vec_pretty(state)?)
    }

    pub fn upsert(&self, mut backend: RemoteBackend) -> Result<RemoteBackend> {
        if backend.site_id.is_empty() {
            backend.site_id = default_site();
        }
        if backend.route_domain.is_empty() {
            backend.route_domain = default_route_domain();
        }
        if backend.state == BackendState::Draining && backend.drain_until_unix_ms.is_none() {
            backend.drain_until_unix_ms =
                Some(now_unix_ms().saturating_add(DEFAULT_DRAIN_UPSERT_MS));
        }
        validate_remote_backend(&backend)?;
        let key = directory_key(
            &backend.route_domain,
            &backend.service,
            backend.vip.as_deref(),
            &backend.address,
            backend.port,
        );
        let mut state = self.load()?;
        state.backends.insert(key, backend.clone());
        state.generation = state.generation.saturating_add(1);
        self.save(&state)?;
        Ok(backend)
    }

    pub fn get(
        &self,
        route_domain: &str,
        service: &str,
        address: &str,
        port: u16,
    ) -> Result<Option<RemoteBackend>> {
        let rd = normalize_rd(route_domain);
        Ok(self
            .load()?
            .backends
            .into_values()
            .find(|b| matches_endpoint(b, &rd, service, address, port)))
    }

    pub fn list(
        &self,
        service: Option<&str>,
        site_id: Option<&str>,
        route_domain: Option<&str>,
    ) -> Result<Vec<RemoteBackend>> {
        let state = self.load()?;
        let mut out: Vec<_> = state
            .backends
            .into_values()
            .filter(|b| {
                if let Some(svc) = service {
                    if b.service != svc {
                        return false;
                    }
                }
                if let Some(site) = site_id {
                    if b.site_id != site {
                        return false;
                    }
                }
                if let Some(rd) = route_domain {
                    if b.route_domain != normalize_rd(rd) {
                        return false;
                    }
                }
                true
            })
            .collect();
        out.sort_by(|a, b| {
            (
                &a.route_domain,
                &a.service,
                a.vip.as_deref().unwrap_or(""),
                &a.address,
                a.port,
            )
                .cmp(&(
                    &b.route_domain,
                    &b.service,
                    b.vip.as_deref().unwrap_or(""),
                    &b.address,
                    b.port,
                ))
        });
        Ok(out)
    }

    /// Delete all catalog rows matching `(route_domain, service, address, port)`
    /// regardless of `vip`. Returns one removed row (if any).
    pub fn delete(
        &self,
        route_domain: &str,
        service: &str,
        address: &str,
        port: u16,
    ) -> Result<Option<RemoteBackend>> {
        let rd = normalize_rd(route_domain);
        let mut state = self.load()?;
        let keys: Vec<String> = state
            .backends
            .iter()
            .filter(|(_, b)| matches_endpoint(b, &rd, service, address, port))
            .map(|(k, _)| k.clone())
            .collect();
        let mut removed = None;
        for key in keys {
            removed = state.backends.remove(&key).or(removed);
        }
        if removed.is_some() {
            state.generation = state.generation.saturating_add(1);
            self.save(&state)?;
        }
        Ok(removed)
    }

    /// Mark matching catalog row(s) Draining with deadline and optional weight.
    /// Updates all VIP variants for `(route_domain, service, address, port)`.
    pub fn begin_drain(
        &self,
        route_domain: &str,
        service: &str,
        address: &str,
        port: u16,
        drain_until_unix_ms: u64,
        weight: Option<u32>,
    ) -> Result<RemoteBackend> {
        let rd = normalize_rd(route_domain);
        let mut state = self.load()?;
        let keys: Vec<String> = state
            .backends
            .iter()
            .filter(|(_, b)| matches_endpoint(b, &rd, service, address, port))
            .map(|(k, _)| k.clone())
            .collect();
        if keys.is_empty() {
            bail!(
                "remote backend {address}:{port} not found for service {service} in route_domain {rd}"
            );
        }
        let mut first = None;
        for key in keys {
            let Some(entry) = state.backends.get_mut(&key) else {
                continue;
            };
            entry.state = BackendState::Draining;
            entry.drain_until_unix_ms = Some(drain_until_unix_ms);
            if let Some(w) = weight {
                entry.weight = w;
            }
            entry.updated_unix_ms = now_unix_ms();
            validate_remote_backend(entry)?;
            if first.is_none() {
                first = Some(entry.clone());
            }
        }
        state.generation = state.generation.saturating_add(1);
        self.save(&state)?;
        first.context("begin_drain updated no rows")
    }

    fn record_applied(&self, route_domain: &str, service: &str, keys: Vec<String>) -> Result<()> {
        let mut state = self.load()?;
        let ak = applied_key(route_domain, service);
        if keys.is_empty() {
            state.applied.remove(&ak);
        } else {
            state.applied.insert(ak, keys);
        }
        state.generation = state.generation.saturating_add(1);
        self.save(&state)
    }

    fn applied_keys(&self, route_domain: &str, service: &str) -> Result<HashSet<String>> {
        let state = self.load()?;
        Ok(state
            .applied
            .get(&applied_key(route_domain, service))
            .cloned()
            .unwrap_or_default()
            .into_iter()
            .collect())
    }
}

fn normalize_rd(route_domain: &str) -> String {
    if route_domain.is_empty() {
        default_route_domain()
    } else {
        route_domain.to_string()
    }
}

fn now_unix_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

pub fn validate_remote_backend(b: &RemoteBackend) -> Result<()> {
    if b.service.is_empty() || b.service.len() > 128 {
        bail!("remote backend service name invalid");
    }
    if b.port == 0 {
        bail!("remote backend port must be non-zero");
    }
    if b.weight == 0 || b.weight > MAX_WEIGHT {
        bail!("remote backend weight must be in 1..={MAX_WEIGHT}");
    }
    if b.labels.len() > MAX_LABELS {
        bail!("remote backend labels bounded to {MAX_LABELS}");
    }
    if b.address.is_empty() || b.address.len() > 128 {
        bail!("invalid remote backend address length");
    }
    if b.address.parse::<IpAddr>().is_err() {
        bail!("invalid remote backend address {:?}", b.address);
    }
    if let Some(ref vip) = b.vip {
        if vip.is_empty() || vip.parse::<IpAddr>().is_err() {
            bail!("invalid remote backend vip {:?}", vip);
        }
    }
    if b.state == BackendState::Draining && b.drain_until_unix_ms.is_none() {
        bail!("draining remote backend requires drain_until_unix_ms");
    }
    Ok(())
}

/// Match remote to Maglev spec: VIP when set, else service name.
pub fn remote_matches_spec(remote: &RemoteBackend, spec: &ServiceSpec) -> bool {
    if let Some(ref vip) = remote.vip {
        return spec.vip.to_string() == *vip;
    }
    remote.service == spec.name
}

fn remote_active_for_merge(remote: &RemoteBackend, now_unix_ms: u64) -> bool {
    match remote.state {
        BackendState::Ready => true,
        BackendState::Unhealthy => false,
        BackendState::Draining => {
            let until = remote.drain_until_unix_ms.unwrap_or(0);
            // now_unix_ms == 0 → do not expire
            !(now_unix_ms > 0 && until > 0 && now_unix_ms > until)
        }
    }
}

/// Same-domain Ready remotes for Maglev merge (cross-domain ignored).
pub fn same_domain_ready_remotes(
    route_domain: Option<&str>,
    service: &str,
    directory: &RemoteBackendDirectory,
) -> Result<Vec<RemoteBackend>> {
    let want_rd = route_domain.unwrap_or("default");
    Ok(directory
        .list(Some(service), None, Some(want_rd))?
        .into_iter()
        .filter(|b| b.state == BackendState::Ready)
        .collect())
}

/// Strip previously applied remote keys, then append same-domain Ready and
/// active Draining remotes that do not collide with local `(address, port)`.
///
/// `now_unix_ms`: when non-zero, skip Draining remotes whose
/// `drain_until_unix_ms` is in the past. Zero disables expiry.
///
/// Callers must already filter `remotes` to the owning route domain. FluxVM
/// service specs typically omit `route_domain` (Fabric-only), so this merge
/// does **not** re-fence on `local.route_domain`.
pub fn merge_remote_backends(
    local: &ServiceSpec,
    remotes: &[RemoteBackend],
    previously_applied: &HashSet<String>,
    now_unix_ms: u64,
) -> Result<ServiceSpec> {
    let mut out = local.clone();
    out.backends
        .retain(|b| !previously_applied.contains(&backend_key(&b.address.to_string(), b.port)));
    let local_keys: HashSet<String> = out
        .backends
        .iter()
        .map(|b| backend_key(&b.address.to_string(), b.port))
        .collect();

    for remote in remotes {
        if !remote_matches_spec(remote, local) {
            continue;
        }
        if !remote_active_for_merge(remote, now_unix_ms) {
            continue;
        }
        let key = backend_key(&remote.address, remote.port);
        if local_keys.contains(&key) {
            // Preserve local entry; do not clobber.
            continue;
        }
        let address: IpAddr = remote
            .address
            .parse()
            .with_context(|| format!("remote backend address {}", remote.address))?;
        let weight = remote.weight.min(MAX_WEIGHT) as u16;
        let (state, drain_until_unix_ms) = match remote.state {
            BackendState::Ready => (BackendState::Ready, None),
            BackendState::Draining => {
                let until = remote
                    .drain_until_unix_ms
                    .unwrap_or_else(|| now_unix_ms.saturating_add(DEFAULT_DRAIN_UPSERT_MS));
                (BackendState::Draining, Some(until))
            }
            BackendState::Unhealthy => continue,
        };
        out.backends.push(ServiceBackend {
            address,
            port: remote.port,
            weight: weight.max(1),
            enabled: true,
            state,
            drain_until_unix_ms,
        });
    }
    Ok(out)
}

/// Keys that would be injected by [`merge_remote_backends`] for tracking.
pub fn injected_remote_keys(
    local: &ServiceSpec,
    remotes: &[RemoteBackend],
    previously_applied: &HashSet<String>,
    now_unix_ms: u64,
) -> Result<Vec<String>> {
    let merged = merge_remote_backends(local, remotes, previously_applied, now_unix_ms)?;
    let local_after_strip: HashSet<String> = {
        let mut tmp = local.clone();
        tmp.backends
            .retain(|b| !previously_applied.contains(&backend_key(&b.address.to_string(), b.port)));
        tmp.backends
            .iter()
            .map(|b| backend_key(&b.address.to_string(), b.port))
            .collect()
    };
    let mut keys: Vec<String> = merged
        .backends
        .iter()
        .map(|b| backend_key(&b.address.to_string(), b.port))
        .filter(|k| !local_after_strip.contains(k))
        .collect();
    keys.sort();
    keys.dedup();
    Ok(keys)
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RemoteBackendReconcileReport {
    pub route_domain: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub service: Option<String>,
    pub services: Vec<String>,
    pub remote_backends: usize,
    pub applied_nodes: Vec<String>,
    pub deleted: bool,
}

pub struct RemoteBackendOrchestrator<C> {
    directory: RemoteBackendDirectory,
    client: C,
}

impl<C: ServiceNodeClient> RemoteBackendOrchestrator<C> {
    pub fn new(directory: RemoteBackendDirectory, client: C) -> Self {
        Self { directory, client }
    }

    pub fn directory(&self) -> &RemoteBackendDirectory {
        &self.directory
    }

    /// Merge same-domain Ready + active Draining remotes into matching Maglev
    /// specs on owning-domain leased nodes (falls back to all nodes when no leases).
    pub async fn reconcile(
        &self,
        service: Option<&str>,
        site_id: Option<&str>,
        route_domain: Option<&str>,
        nodes: &[NodeTarget],
        leases: &[EdgeLease],
        now_unix_ms: u64,
    ) -> Result<RemoteBackendReconcileReport> {
        if nodes.is_empty() {
            bail!("remote backend reconcile requires at least one FluxVM node");
        }
        let rd = route_domain.unwrap_or("default");
        let remotes = self.directory.list(service, site_id, Some(rd))?;
        let remote_count = remotes
            .iter()
            .filter(|b| remote_active_for_merge(b, now_unix_ms))
            .count();

        let mut must_touch: HashSet<String> = remotes.iter().map(|b| b.service.clone()).collect();
        if let Some(svc) = service {
            must_touch.insert(svc.to_string());
        }

        let targets = reconcile_targets(nodes, site_id, Some(rd), leases, service, now_unix_ms);
        if targets.is_empty() {
            bail!("remote backend reconcile has no nodes in the owning site/route-domain");
        }

        let mut applied_nodes = HashSet::new();
        let mut services_out = HashSet::new();
        let mut applied_by_svc: BTreeMap<String, Vec<String>> = BTreeMap::new();

        for node in &targets {
            let mut specs = self.client.list_services(node).await?;
            for name in &must_touch {
                if !specs.iter().any(|s| s.name == *name) {
                    if let Some(spec) = self.client.get_service(node, name).await? {
                        specs.push(spec);
                    }
                }
            }

            for current in specs {
                if let Some(spec_rd) = current.route_domain.as_deref() {
                    if spec_rd != rd {
                        continue;
                    }
                }
                if let Some(want_site) = site_id {
                    if let Some(spec_site) = current.site_id.as_deref() {
                        if spec_site != want_site {
                            continue;
                        }
                    }
                }

                let svc_remotes: Vec<RemoteBackend> = remotes
                    .iter()
                    .filter(|b| remote_matches_spec(b, &current))
                    .cloned()
                    .collect();
                let force_strip = must_touch.contains(&current.name);
                if svc_remotes.is_empty() && !force_strip {
                    continue;
                }

                let prev = self.directory.applied_keys(rd, &current.name)?;
                let merged = merge_remote_backends(&current, &svc_remotes, &prev, now_unix_ms)?;
                let injected = injected_remote_keys(&current, &svc_remotes, &prev, now_unix_ms)?;
                self.client
                    .upsert_service(node, &merged)
                    .await
                    .with_context(|| {
                        format!(
                            "upsert service {} with remote backends on node {}",
                            current.name, node.name
                        )
                    })?;
                applied_nodes.insert(node.name.clone());
                services_out.insert(current.name.clone());
                applied_by_svc.insert(current.name.clone(), injected);
            }
        }

        for (svc_name, keys) in applied_by_svc {
            self.directory.record_applied(rd, &svc_name, keys)?;
        }

        let mut services: Vec<_> = services_out.into_iter().collect();
        services.sort();
        let mut nodes_out: Vec<_> = applied_nodes.into_iter().collect();
        nodes_out.sort();
        Ok(RemoteBackendReconcileReport {
            route_domain: rd.to_string(),
            service: service.map(|s| s.to_string()),
            services,
            remote_backends: remote_count,
            applied_nodes: nodes_out,
            deleted: false,
        })
    }

    /// Mark remote Draining (weighted handoff) and reconcile onto FluxVM nodes.
    #[allow(clippy::too_many_arguments)]
    pub async fn drain_and_reconcile(
        &self,
        route_domain: &str,
        service: &str,
        address: &str,
        port: u16,
        drain_until_unix_ms: u64,
        weight: Option<u32>,
        nodes: &[NodeTarget],
        site_id: Option<&str>,
        leases: &[EdgeLease],
        now_unix_ms: u64,
    ) -> Result<RemoteBackendReconcileReport> {
        self.directory.begin_drain(
            route_domain,
            service,
            address,
            port,
            drain_until_unix_ms,
            weight,
        )?;
        let rd = normalize_rd(route_domain);
        self.reconcile(
            Some(service),
            site_id,
            Some(&rd),
            nodes,
            leases,
            now_unix_ms,
        )
        .await
    }

    /// Delete catalog entry; next reconcile strips it via applied-key tracking.
    /// Optionally runs an immediate reconcile for the service.
    #[allow(clippy::too_many_arguments)]
    pub async fn delete_and_reconcile(
        &self,
        route_domain: &str,
        service: &str,
        address: &str,
        port: u16,
        nodes: &[NodeTarget],
        site_id: Option<&str>,
        leases: &[EdgeLease],
        now_unix_ms: u64,
        reconcile_now: bool,
    ) -> Result<RemoteBackendReconcileReport> {
        let removed = self
            .directory
            .delete(route_domain, service, address, port)?;
        if removed.is_none() {
            bail!(
                "remote backend {address}:{port} not found for service {service} in route_domain {route_domain}"
            );
        }
        let rd = normalize_rd(route_domain);
        if reconcile_now && !nodes.is_empty() {
            let mut report = self
                .reconcile(
                    Some(service),
                    site_id,
                    Some(&rd),
                    nodes,
                    leases,
                    now_unix_ms,
                )
                .await?;
            report.deleted = true;
            return Ok(report);
        }
        Ok(RemoteBackendReconcileReport {
            route_domain: rd,
            service: Some(service.to_string()),
            services: vec![service.to_string()],
            remote_backends: 0,
            applied_nodes: Vec::new(),
            deleted: true,
        })
    }
}

fn reconcile_targets<'a>(
    nodes: &'a [NodeTarget],
    site_id: Option<&str>,
    route_domain: Option<&str>,
    leases: &[EdgeLease],
    service_for_leases: Option<&str>,
    now_unix_ms: u64,
) -> Vec<&'a NodeTarget> {
    if site_id.is_none() && route_domain.is_none() {
        return nodes.iter().collect();
    }
    let Some(service) = service_for_leases else {
        return nodes.iter().collect();
    };
    let want = domain_key(site_id, route_domain);
    let leased: HashSet<&str> = leases
        .iter()
        .filter(|lease| lease.active(service, now_unix_ms))
        .filter(|lease| domain_key(lease.site_id.as_deref(), lease.route_domain.as_deref()) == want)
        .map(|lease| lease.node.as_str())
        .collect();
    if leased.is_empty() {
        return nodes.iter().collect();
    }
    nodes
        .iter()
        .filter(|node| leased.contains(node.name.as_str()))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{ServiceAlgorithm, ServiceExposure, ServiceMode, ServiceProtocol};
    use async_trait::async_trait;
    use std::sync::{Arc, Mutex};
    use tempfile::tempdir;

    fn local_spec(rd: &str) -> ServiceSpec {
        ServiceSpec {
            name: "payments".into(),
            vip: "10.40.0.100".parse().unwrap(),
            port: 443,
            protocol: ServiceProtocol::Tcp,
            algorithm: ServiceAlgorithm::Maglev,
            mode: ServiceMode::Nat,
            exposure: ServiceExposure::EastWest,
            backends: vec![ServiceBackend {
                address: "10.40.1.21".parse().unwrap(),
                port: 8443,
                weight: 1,
                enabled: true,
                state: BackendState::Ready,
                drain_until_unix_ms: None,
            }],
            maglev_table_size: Some(251),
            snat_address: None,
            health_check: None,
            advertise: false,
            max_egress_mbps: None,
            flow_sample_rate: 0,
            host_routing: false,
            site_id: Some("site-a".into()),
            route_domain: Some(rd.into()),
        }
    }

    fn local_spec_vip(rd: &str, name: &str, vip: &str) -> ServiceSpec {
        let mut s = local_spec(rd);
        s.name = name.into();
        s.vip = vip.parse().unwrap();
        s
    }

    fn remote(rd: &str, addr: &str, port: u16, state: BackendState) -> RemoteBackend {
        RemoteBackend {
            service: "payments".into(),
            site_id: "site-b".into(),
            route_domain: rd.into(),
            vip: None,
            address: addr.into(),
            port,
            weight: 1,
            state,
            drain_until_unix_ms: None,
            labels: BTreeMap::new(),
            updated_unix_ms: 1,
        }
    }

    #[test]
    fn same_domain_ready_merge() {
        let local = local_spec("rd-1");
        let remotes = vec![remote("rd-1", "10.50.1.9", 8443, BackendState::Ready)];
        let merged = merge_remote_backends(&local, &remotes, &HashSet::new(), 0).unwrap();
        assert_eq!(merged.backends.len(), 2);
        assert!(merged
            .backends
            .iter()
            .any(|b| b.address.to_string() == "10.50.1.9"));
        assert!(merged
            .backends
            .iter()
            .any(|b| b.address.to_string() == "10.40.1.21"));
    }

    #[test]
    fn cross_domain_remote_ignored() {
        // Domain fencing happens at catalog list / same_domain_ready_remotes —
        // merge assumes remotes are already scoped. Simulate orchestrator filter.
        let dir = RemoteBackendDirectory::new(tempdir().unwrap().path().join("rb.json"));
        dir.upsert(remote("rd-1", "10.50.1.8", 8443, BackendState::Ready))
            .unwrap();
        dir.upsert(remote("rd-other", "10.50.1.9", 8443, BackendState::Ready))
            .unwrap();
        let remotes = same_domain_ready_remotes(Some("rd-1"), "payments", &dir).unwrap();
        assert_eq!(remotes.len(), 1);
        assert_eq!(remotes[0].address, "10.50.1.8");
        let local = local_spec("rd-1");
        let merged = merge_remote_backends(&local, &remotes, &HashSet::new(), 0).unwrap();
        assert_eq!(merged.backends.len(), 2);
        assert!(!merged
            .backends
            .iter()
            .any(|b| b.address.to_string() == "10.50.1.9"));
    }

    #[test]
    fn draining_remote_included_with_weight_and_deadline() {
        let local = local_spec("rd-1");
        let mut draining = remote("rd-1", "10.50.1.9", 8443, BackendState::Draining);
        draining.weight = 2;
        draining.drain_until_unix_ms = Some(9_000_000);
        let remotes = vec![
            draining,
            remote("rd-1", "10.50.1.10", 8443, BackendState::Unhealthy),
        ];
        let merged = merge_remote_backends(&local, &remotes, &HashSet::new(), 1_000).unwrap();
        assert_eq!(merged.backends.len(), 2);
        let inj = merged
            .backends
            .iter()
            .find(|b| b.address.to_string() == "10.50.1.9")
            .unwrap();
        assert_eq!(inj.state, BackendState::Draining);
        assert_eq!(inj.weight, 2);
        assert_eq!(inj.drain_until_unix_ms, Some(9_000_000));
        assert!(!merged
            .backends
            .iter()
            .any(|b| b.address.to_string() == "10.50.1.10"));
    }

    #[test]
    fn expired_drain_skipped_when_now_past_deadline() {
        let local = local_spec("rd-1");
        let mut draining = remote("rd-1", "10.50.1.9", 8443, BackendState::Draining);
        draining.drain_until_unix_ms = Some(5_000);
        let remotes = vec![draining];
        let merged = merge_remote_backends(&local, &remotes, &HashSet::new(), 5_001).unwrap();
        assert_eq!(merged.backends.len(), 1);
        let kept = merge_remote_backends(&local, &remotes, &HashSet::new(), 0).unwrap();
        assert_eq!(kept.backends.len(), 2);
    }

    #[test]
    fn multi_vip_remote_only_merges_matching_vip_spec() {
        let east = local_spec_vip("rd-1", "payments-east", "10.40.0.100");
        let west = local_spec_vip("rd-1", "payments-west", "10.40.0.200");
        let mut remote = remote("rd-1", "10.50.1.9", 8443, BackendState::Ready);
        remote.vip = Some("10.40.0.100".into());
        remote.service = "payments".into(); // ignored when vip set
        let remotes = vec![remote];
        let merged_east = merge_remote_backends(&east, &remotes, &HashSet::new(), 0).unwrap();
        let merged_west = merge_remote_backends(&west, &remotes, &HashSet::new(), 0).unwrap();
        assert_eq!(merged_east.backends.len(), 2);
        assert_eq!(merged_west.backends.len(), 1);
    }

    #[test]
    fn remote_without_vip_merges_by_service_name() {
        let local = local_spec("rd-1");
        let remotes = vec![remote("rd-1", "10.50.1.9", 8443, BackendState::Ready)];
        assert!(remote_matches_spec(&remotes[0], &local));
        let other = local_spec_vip("rd-1", "other", "10.40.0.100");
        assert!(!remote_matches_spec(&remotes[0], &other));
        let merged = merge_remote_backends(&local, &remotes, &HashSet::new(), 0).unwrap();
        assert_eq!(merged.backends.len(), 2);
    }

    #[test]
    fn local_backend_not_clobbered() {
        let local = local_spec("rd-1");
        // Same address:port as local — remote must not replace local weight/state.
        let mut remotes = vec![remote("rd-1", "10.40.1.21", 8443, BackendState::Ready)];
        remotes[0].weight = 8;
        let merged = merge_remote_backends(&local, &remotes, &HashSet::new(), 0).unwrap();
        assert_eq!(merged.backends.len(), 1);
        assert_eq!(merged.backends[0].weight, 1);
    }

    #[test]
    fn delete_removes_from_next_reconcile() {
        let dir = tempdir().unwrap();
        let store = RemoteBackendDirectory::new(dir.path().join("remote-backends.json"));
        store
            .upsert(remote("rd-1", "10.50.1.9", 8443, BackendState::Ready))
            .unwrap();
        let local = local_spec("rd-1");
        let remotes = store.list(Some("payments"), None, Some("rd-1")).unwrap();
        let prev = HashSet::new();
        let merged = merge_remote_backends(&local, &remotes, &prev, 0).unwrap();
        assert_eq!(merged.backends.len(), 2);
        let injected = injected_remote_keys(&local, &remotes, &prev, 0).unwrap();
        store
            .record_applied("rd-1", "payments", injected.clone())
            .unwrap();

        store.delete("rd-1", "payments", "10.50.1.9", 8443).unwrap();
        let remotes2 = store.list(Some("payments"), None, Some("rd-1")).unwrap();
        assert!(remotes2.is_empty());
        let prev2 = store.applied_keys("rd-1", "payments").unwrap();
        // Simulate node still holding previous merge as "current".
        let after_delete = merge_remote_backends(&merged, &remotes2, &prev2, 0).unwrap();
        assert_eq!(after_delete.backends.len(), 1);
        assert_eq!(after_delete.backends[0].address.to_string(), "10.40.1.21");
    }

    #[test]
    fn delete_all_vip_variants_for_endpoint() {
        let dir = tempdir().unwrap();
        let store = RemoteBackendDirectory::new(dir.path().join("rb.json"));
        let mut a = remote("rd-1", "10.50.1.9", 8443, BackendState::Ready);
        a.vip = Some("10.40.0.100".into());
        let mut b = remote("rd-1", "10.50.1.9", 8443, BackendState::Ready);
        b.vip = Some("10.40.0.200".into());
        store.upsert(a).unwrap();
        store.upsert(b).unwrap();
        assert_eq!(
            store
                .list(Some("payments"), None, Some("rd-1"))
                .unwrap()
                .len(),
            2
        );
        store.delete("rd-1", "payments", "10.50.1.9", 8443).unwrap();
        assert!(store
            .list(Some("payments"), None, Some("rd-1"))
            .unwrap()
            .is_empty());
    }

    #[derive(Clone, Default)]
    struct FakeSvc {
        state: Arc<Mutex<BTreeMap<String, ServiceSpec>>>,
    }

    #[async_trait]
    impl ServiceNodeClient for FakeSvc {
        async fn get_service(&self, node: &NodeTarget, name: &str) -> Result<Option<ServiceSpec>> {
            Ok(self
                .state
                .lock()
                .unwrap()
                .get(&format!("{}|{name}", node.name))
                .cloned())
        }
        async fn list_services(&self, node: &NodeTarget) -> Result<Vec<ServiceSpec>> {
            let prefix = format!("{}|", node.name);
            let mut out: Vec<_> = self
                .state
                .lock()
                .unwrap()
                .iter()
                .filter_map(|(k, v)| {
                    if k.starts_with(&prefix) {
                        Some(v.clone())
                    } else {
                        None
                    }
                })
                .collect();
            out.sort_by(|a, b| a.name.cmp(&b.name));
            Ok(out)
        }
        async fn get_status(&self, _node: &NodeTarget) -> Result<crate::HostServiceStatus> {
            bail!("unused")
        }
        async fn get_advertisements(
            &self,
            _node: &NodeTarget,
        ) -> Result<crate::AdvertisementSnapshot> {
            bail!("unused")
        }
        async fn upsert_service(&self, node: &NodeTarget, spec: &ServiceSpec) -> Result<()> {
            self.state
                .lock()
                .unwrap()
                .insert(format!("{}|{}", node.name, spec.name), spec.clone());
            Ok(())
        }
        async fn delete_service(&self, _node: &NodeTarget, _name: &str) -> Result<()> {
            Ok(())
        }
        async fn export_conntrack(
            &self,
            _node: &NodeTarget,
            _name: &str,
        ) -> Result<crate::ConntrackSnapshot> {
            bail!("unused")
        }
        async fn import_conntrack(
            &self,
            _node: &NodeTarget,
            _name: &str,
            _snapshot: &crate::ConntrackSnapshot,
        ) -> Result<usize> {
            bail!("unused")
        }
    }

    #[tokio::test]
    async fn orchestrator_reconcile_merges_and_delete_strips() {
        let dir = tempdir().unwrap();
        let store = RemoteBackendDirectory::new(dir.path().join("rb.json"));
        let fake = FakeSvc::default();
        let node = NodeTarget {
            name: "edge-a".into(),
            base_url: "http://a".into(),
            token: None,
        };
        fake.state
            .lock()
            .unwrap()
            .insert("edge-a|payments".into(), local_spec("rd-1"));
        store
            .upsert(remote("rd-1", "10.50.1.9", 8443, BackendState::Ready))
            .unwrap();
        // Cross-domain noise
        store
            .upsert(remote("rd-2", "10.50.1.99", 8443, BackendState::Ready))
            .unwrap();

        let orch = RemoteBackendOrchestrator::new(store, fake.clone());
        let report = orch
            .reconcile(
                Some("payments"),
                None,
                Some("rd-1"),
                &[node.clone()],
                &[],
                0,
            )
            .await
            .unwrap();
        assert_eq!(report.remote_backends, 1);
        let applied = fake
            .state
            .lock()
            .unwrap()
            .get("edge-a|payments")
            .cloned()
            .unwrap();
        assert_eq!(applied.backends.len(), 2);

        orch.delete_and_reconcile(
            "rd-1",
            "payments",
            "10.50.1.9",
            8443,
            &[node],
            None,
            &[],
            0,
            true,
        )
        .await
        .unwrap();
        let after = fake
            .state
            .lock()
            .unwrap()
            .get("edge-a|payments")
            .cloned()
            .unwrap();
        assert_eq!(after.backends.len(), 1);
        assert_eq!(after.backends[0].address.to_string(), "10.40.1.21");
    }

    #[tokio::test]
    async fn begin_drain_and_reconcile_injects_draining() {
        let dir = tempdir().unwrap();
        let store = RemoteBackendDirectory::new(dir.path().join("rb.json"));
        let fake = FakeSvc::default();
        let node = NodeTarget {
            name: "edge-a".into(),
            base_url: "http://a".into(),
            token: None,
        };
        fake.state
            .lock()
            .unwrap()
            .insert("edge-a|payments".into(), local_spec("rd-1"));
        store
            .upsert(remote("rd-1", "10.50.1.9", 8443, BackendState::Ready))
            .unwrap();

        let orch = RemoteBackendOrchestrator::new(store, fake.clone());
        let report = orch
            .drain_and_reconcile(
                "rd-1",
                "payments",
                "10.50.1.9",
                8443,
                8_000_000,
                Some(2),
                &[node],
                None,
                &[],
                1_000,
            )
            .await
            .unwrap();
        assert_eq!(report.remote_backends, 1);
        let applied = fake
            .state
            .lock()
            .unwrap()
            .get("edge-a|payments")
            .cloned()
            .unwrap();
        let inj = applied
            .backends
            .iter()
            .find(|b| b.address.to_string() == "10.50.1.9")
            .unwrap();
        assert_eq!(inj.state, BackendState::Draining);
        assert_eq!(inj.weight, 2);
        assert_eq!(inj.drain_until_unix_ms, Some(8_000_000));
    }

    #[tokio::test]
    async fn reconcile_multi_vip_via_list_services() {
        let dir = tempdir().unwrap();
        let store = RemoteBackendDirectory::new(dir.path().join("rb.json"));
        let fake = FakeSvc::default();
        let node = NodeTarget {
            name: "edge-a".into(),
            base_url: "http://a".into(),
            token: None,
        };
        fake.state.lock().unwrap().insert(
            "edge-a|payments-east".into(),
            local_spec_vip("rd-1", "payments-east", "10.40.0.100"),
        );
        fake.state.lock().unwrap().insert(
            "edge-a|payments-west".into(),
            local_spec_vip("rd-1", "payments-west", "10.40.0.200"),
        );
        let mut remote = remote("rd-1", "10.50.1.9", 8443, BackendState::Ready);
        remote.service = "payments".into();
        remote.vip = Some("10.40.0.100".into());
        store.upsert(remote).unwrap();

        let orch = RemoteBackendOrchestrator::new(store, fake.clone());
        let report = orch
            .reconcile(None, None, Some("rd-1"), &[node], &[], 0)
            .await
            .unwrap();
        assert!(report.services.iter().any(|s| s == "payments-east"));
        let east = fake
            .state
            .lock()
            .unwrap()
            .get("edge-a|payments-east")
            .cloned()
            .unwrap();
        let west = fake
            .state
            .lock()
            .unwrap()
            .get("edge-a|payments-west")
            .cloned()
            .unwrap();
        assert_eq!(east.backends.len(), 2);
        assert_eq!(west.backends.len(), 1);
    }
}
