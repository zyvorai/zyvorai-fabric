// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: LicenseRef-Zyvor-Production-2.0

//! Fabric orchestration for FluxVM Service Fabric v6+.
//!
//! Fabric owns service intent, node selection, rollout and rollback. FluxVM
//! owns every TC/XDP program and BPF map. v6+ adds optional multi-site
//! `site_id` / `route_domain` fencing for anycast VIP advertise and policy
//! fan-out on top of v5 durable edge leases and sequence/ack HA deltas.
//! This crate never writes bpffs or invokes `tc`, `ip`, or `bpftool` directly.

pub mod policy;
pub mod remote_backend;
pub mod remote_identity;

use anyhow::{bail, Context, Result};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, HashMap, HashSet},
    fs,
    io::Write,
    net::IpAddr,
    path::{Path, PathBuf},
    process::Command,
};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum ServiceProtocol {
    Tcp,
    Udp,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "lowercase")]
pub enum ServiceAlgorithm {
    #[default]
    Maglev,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "lowercase")]
pub enum ServiceMode {
    #[default]
    Nat,
    Dsr,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "kebab-case")]
pub enum ServiceExposure {
    #[default]
    EastWest,
    NorthSouth,
    Both,
}

impl ServiceExposure {
    pub fn north_south(self) -> bool {
        matches!(self, Self::NorthSouth | Self::Both)
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "lowercase")]
pub enum BackendState {
    #[default]
    Ready,
    Draining,
    Unhealthy,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum HealthCheckKind {
    Tcp,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct ServiceHealthCheck {
    pub kind: HealthCheckKind,
    pub timeout_ms: u64,
    pub unhealthy_threshold: u32,
    pub healthy_threshold: u32,
}

impl Default for ServiceHealthCheck {
    fn default() -> Self {
        Self {
            kind: HealthCheckKind::Tcp,
            timeout_ms: 500,
            unhealthy_threshold: 3,
            healthy_threshold: 2,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ServiceBackend {
    pub address: IpAddr,
    pub port: u16,
    #[serde(default = "default_weight")]
    pub weight: u16,
    #[serde(default = "default_enabled")]
    pub enabled: bool,
    #[serde(default)]
    pub state: BackendState,
    #[serde(default)]
    pub drain_until_unix_ms: Option<u64>,
}
fn default_weight() -> u16 {
    1
}
fn default_enabled() -> bool {
    true
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ServiceSpec {
    pub name: String,
    pub vip: IpAddr,
    pub port: u16,
    pub protocol: ServiceProtocol,
    #[serde(default)]
    pub algorithm: ServiceAlgorithm,
    #[serde(default)]
    pub mode: ServiceMode,
    #[serde(default)]
    pub exposure: ServiceExposure,
    #[serde(default)]
    pub backends: Vec<ServiceBackend>,
    #[serde(default)]
    pub maglev_table_size: Option<u32>,
    #[serde(default)]
    pub snat_address: Option<IpAddr>,
    #[serde(default)]
    pub health_check: Option<ServiceHealthCheck>,
    #[serde(default)]
    pub advertise: bool,
    #[serde(default)]
    pub max_egress_mbps: Option<u32>,
    #[serde(default)]
    pub flow_sample_rate: u32,
    #[serde(default)]
    pub host_routing: bool,
    /// Optional site ownership for multi-site anycast fencing. Missing/`None`
    /// is treated as the default single-site domain (backward compatible).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub site_id: Option<String>,
    /// Optional route-domain ownership (ClusterMesh-like). Missing/`None` is
    /// treated as the default domain alongside [`Self::site_id`].
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub route_domain: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct NodeTarget {
    pub name: String,
    pub base_url: String,
    #[serde(default)]
    pub token: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct InterfaceOffloadStatus {
    #[serde(default)]
    pub gro: Option<bool>,
    #[serde(default)]
    pub gso: Option<bool>,
    #[serde(default)]
    pub rx_checksumming: Option<bool>,
    #[serde(default)]
    pub tx_checksumming: Option<bool>,
    #[serde(default)]
    pub combined_channels: Option<u32>,
    #[serde(default)]
    pub rx_channels: Option<u32>,
    #[serde(default)]
    pub tx_channels: Option<u32>,
    #[serde(default)]
    pub notes: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct HostInterfaceStatus {
    pub interface: String,
    pub tc_program_pinned: bool,
    pub xdp_requested: bool,
    pub xdp_program_pinned: bool,
    #[serde(default)]
    pub xdp_mode: String,
    #[serde(default)]
    pub offload: InterfaceOffloadStatus,
    pub pin_dir: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct HostServiceStatus {
    pub schema_version: u32,
    #[serde(default)]
    pub program_generation: u32,
    pub north_south_interfaces: Vec<String>,
    pub xdp_acceleration: bool,
    #[serde(default)]
    pub cgroup_connect: bool,
    #[serde(default)]
    pub cgroup_connect_attached: bool,
    #[serde(default)]
    pub map_tier: String,
    pub interfaces: Vec<HostInterfaceStatus>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RawMapEntry {
    pub map: String,
    pub key_hex: String,
    pub value_hex: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ConntrackSnapshot {
    pub schema_version: u32,
    pub service: String,
    pub service_id: u32,
    pub created_unix_ms: u64,
    pub entries: Vec<RawMapEntry>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum HaDeltaOperation {
    Upsert,
    Delete,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ConntrackDeltaEntry {
    pub seq: u64,
    pub operation: HaDeltaOperation,
    pub map: String,
    pub key_hex: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub value_hex: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ConntrackDeltaBatch {
    pub schema_version: u32,
    pub service: String,
    pub service_id: u32,
    pub after_seq: u64,
    pub last_seq: u64,
    pub acked_seq: u64,
    pub reset_required: bool,
    #[serde(default)]
    pub snapshot_barrier: bool,
    pub truncated: bool,
    pub entries: Vec<ConntrackDeltaEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct HaJournalStatus {
    pub service: String,
    pub service_id: u32,
    pub last_seq: u64,
    pub acked_seq: u64,
    pub first_available_seq: u64,
    pub retained_entries: usize,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct HaDeltaApplyReport {
    pub service: String,
    pub applied_entries: usize,
    pub deleted_entries: usize,
    pub last_applied_seq: u64,
    pub snapshot_barrier: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct VipAdvertisement {
    pub service: String,
    pub vip: IpAddr,
    pub prefix_len: u8,
    pub advertise: bool,
    pub reason: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AdvertisementSnapshot {
    pub schema_version: u32,
    pub generation: u64,
    pub items: Vec<VipAdvertisement>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct EdgeLease {
    pub service: String,
    pub node: String,
    pub epoch: u64,
    pub expires_unix_ms: u64,
    /// Site that issued this lease; used with [`Self::route_domain`] to fence
    /// anycast VIP advertise and policy fan-out across sites.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub site_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub route_domain: Option<String>,
}

impl EdgeLease {
    pub fn active(&self, service: &str, now_unix_ms: u64) -> bool {
        self.service == service && self.expires_unix_ms > now_unix_ms
    }
}

/// Canonical keys for site/route fencing. Missing values map to `"default"` so
/// pre-multi-site leases and specs share one single-site domain.
pub fn domain_key(site_id: Option<&str>, route_domain: Option<&str>) -> (String, String) {
    (
        site_id.unwrap_or("default").to_string(),
        route_domain.unwrap_or("default").to_string(),
    )
}

pub fn lease_matches_service_domain(lease: &EdgeLease, spec: &ServiceSpec) -> bool {
    domain_key(lease.site_id.as_deref(), lease.route_domain.as_deref())
        == domain_key(spec.site_id.as_deref(), spec.route_domain.as_deref())
}

/// Active leases that may drive anycast VIP advertise for `spec`.
pub fn active_domain_leases<'a>(
    spec: &ServiceSpec,
    leases: &'a [EdgeLease],
    now_unix_ms: u64,
) -> Vec<&'a EdgeLease> {
    let mut active: Vec<&EdgeLease> = leases
        .iter()
        .filter(|lease| lease.active(&spec.name, now_unix_ms))
        .filter(|lease| lease_matches_service_domain(lease, spec))
        .collect();
    active.sort_by(|a, b| a.node.cmp(&b.node));
    active
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct DurableLeaseState {
    pub generation: u64,
    /// Per-service fencing epoch high-water marks. These survive lease
    /// release/expiry so a later owner can never reuse an old epoch (ABA).
    #[serde(default)]
    pub epochs: BTreeMap<String, u64>,
    pub leases: Vec<EdgeLease>,
}

#[derive(Debug, Clone)]
pub struct DurableLeaseStore {
    path: PathBuf,
}

impl DurableLeaseStore {
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self { path: path.into() }
    }

    pub fn load(&self) -> Result<DurableLeaseState> {
        if !self.path.exists() {
            return Ok(DurableLeaseState::default());
        }
        let raw = fs::read(&self.path)
            .with_context(|| format!("reading lease store {}", self.path.display()))?;
        serde_json::from_slice(&raw)
            .with_context(|| format!("parsing lease store {}", self.path.display()))
    }

    pub fn save(&self, state: &DurableLeaseState) -> Result<()> {
        atomic_write(&self.path, &serde_json::to_vec_pretty(state)?)
    }

    pub fn active(&self, service: &str, now_unix_ms: u64) -> Result<Vec<EdgeLease>> {
        let mut out: Vec<_> = self
            .load()?
            .leases
            .into_iter()
            .filter(|lease| lease.active(service, now_unix_ms))
            .collect();
        out.sort_by(|a, b| a.node.cmp(&b.node));
        Ok(out)
    }

    pub fn renew(
        &self,
        service: &str,
        node: &str,
        ttl_ms: u64,
        now_unix_ms: u64,
    ) -> Result<EdgeLease> {
        self.renew_in_domain(service, node, ttl_ms, now_unix_ms, None, None)
    }

    /// Acquire or replace a lease stamped with optional site/route-domain fencing.
    pub fn renew_in_domain(
        &self,
        service: &str,
        node: &str,
        ttl_ms: u64,
        now_unix_ms: u64,
        site_id: Option<String>,
        route_domain: Option<String>,
    ) -> Result<EdgeLease> {
        validate_name(service)?;
        if node.is_empty() {
            bail!("lease node must not be empty");
        }
        if ttl_ms == 0 {
            bail!("lease ttl_ms must be non-zero");
        }
        let mut state = self.load()?;
        let next_epoch = state
            .epochs
            .get(service)
            .copied()
            .unwrap_or(0)
            .saturating_add(1);
        state.epochs.insert(service.to_string(), next_epoch);
        state
            .leases
            .retain(|l| !(l.service == service && l.node == node));
        let lease = EdgeLease {
            service: service.to_string(),
            node: node.to_string(),
            epoch: next_epoch,
            expires_unix_ms: now_unix_ms.saturating_add(ttl_ms),
            site_id,
            route_domain,
        };
        state.leases.push(lease.clone());
        state.generation = state.generation.saturating_add(1);
        self.save(&state)?;
        Ok(lease)
    }

    pub fn extend(
        &self,
        service: &str,
        node: &str,
        ttl_ms: u64,
        now_unix_ms: u64,
    ) -> Result<EdgeLease> {
        if ttl_ms == 0 {
            bail!("lease ttl_ms must be non-zero");
        }
        let mut state = self.load()?;
        let lease = state
            .leases
            .iter_mut()
            .find(|l| l.service == service && l.node == node)
            .context("cannot extend a missing edge lease")?;
        lease.expires_unix_ms = now_unix_ms.saturating_add(ttl_ms);
        let out = lease.clone();
        state.generation = state.generation.saturating_add(1);
        self.save(&state)?;
        Ok(out)
    }

    pub fn prune_expired(&self, now_unix_ms: u64) -> Result<usize> {
        let mut state = self.load()?;
        let before = state.leases.len();
        state.leases.retain(|l| l.expires_unix_ms > now_unix_ms);
        let removed = before.saturating_sub(state.leases.len());
        if removed > 0 {
            state.generation = state.generation.saturating_add(1);
            self.save(&state)?;
        }
        Ok(removed)
    }

    pub fn release(&self, service: &str, node: &str) -> Result<bool> {
        let mut state = self.load()?;
        let before = state.leases.len();
        state
            .leases
            .retain(|l| !(l.service == service && l.node == node));
        let changed = state.leases.len() != before;
        if changed {
            state.generation = state.generation.saturating_add(1);
            self.save(&state)?;
        }
        Ok(changed)
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct StreamingReplicationState {
    pub generation: u64,
    #[serde(default)]
    pub cursors: BTreeMap<String, u64>,
}

#[derive(Debug, Clone)]
pub struct StreamingReplicationStore {
    path: PathBuf,
}

impl StreamingReplicationStore {
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self { path: path.into() }
    }
    pub fn load(&self) -> Result<StreamingReplicationState> {
        if !self.path.exists() {
            return Ok(StreamingReplicationState::default());
        }
        let raw = fs::read(&self.path)?;
        serde_json::from_slice(&raw).context("parsing streaming replication cursor store")
    }
    pub fn save(&self, state: &StreamingReplicationState) -> Result<()> {
        atomic_write(&self.path, &serde_json::to_vec_pretty(state)?)
    }
}

fn replication_cursor_key(service: &str, source: &str, target: &str) -> String {
    format!("{service}|{source}|{target}")
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct BgpVipIntent {
    pub service: String,
    pub vip: IpAddr,
    pub prefix_len: u8,
    pub edge_nodes: Vec<String>,
    pub lease_epoch: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ConntrackReplicationReport {
    pub service: String,
    pub source_node: String,
    pub replicated_nodes: Vec<String>,
    pub entries: usize,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct StreamingReplicationReport {
    pub service: String,
    pub source_node: String,
    pub replicated_nodes: Vec<String>,
    pub full_resync_nodes: Vec<String>,
    pub delta_entries: usize,
    pub acked_seq: u64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct EdgeReconcileReport {
    pub service: String,
    pub withdrawn_nodes: Vec<String>,
    pub replacement_nodes: Vec<String>,
    pub state_seeded_nodes: Vec<String>,
    pub active_nodes: Vec<String>,
    pub lease_epoch: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ApplyReport {
    pub service: String,
    pub applied_nodes: Vec<String>,
    pub rolled_back_nodes: Vec<String>,
}

pub fn validate_spec(spec: &ServiceSpec) -> Result<()> {
    validate_name(&spec.name)?;
    if spec.port == 0 {
        bail!("service port must be non-zero");
    }
    if spec.backends.is_empty() || !spec.backends.iter().any(|b| b.enabled) {
        bail!("service requires at least one enabled backend");
    }
    let v4 = spec.vip.is_ipv4();
    for backend in &spec.backends {
        if backend.address.is_ipv4() != v4 {
            bail!(
                "backend {} family does not match VIP {}",
                backend.address,
                spec.vip
            );
        }
        if backend.port == 0 {
            bail!("backend {} has port zero", backend.address);
        }
        if backend.weight == 0 || backend.weight > 32 {
            bail!("backend weight must be in 1..=32");
        }
        if backend.state != BackendState::Draining && backend.drain_until_unix_ms.is_some() {
            bail!("drain_until_unix_ms is valid only for a draining backend");
        }
        if spec.mode == ServiceMode::Dsr && backend.port != spec.port {
            bail!("DSR backend port must equal the VIP service port");
        }
    }
    if let Some(snat) = spec.snat_address {
        if snat.is_ipv4() != v4 {
            bail!("SNAT family does not match VIP family");
        }
        if spec.mode == ServiceMode::Dsr {
            bail!("DSR cannot be combined with SNAT");
        }
    }
    if spec.exposure.north_south() && spec.mode == ServiceMode::Nat && spec.snat_address.is_none() {
        bail!("north-south NAT requires snat_address");
    }
    if let Some(health) = &spec.health_check {
        if health.timeout_ms == 0 || health.timeout_ms > 30_000 {
            bail!("health timeout_ms must be in 1..=30000");
        }
        if health.unhealthy_threshold == 0 || health.healthy_threshold == 0 {
            bail!("health thresholds must be greater than zero");
        }
    }
    if spec.advertise && !spec.exposure.north_south() {
        bail!("advertise=true requires north-south or both exposure");
    }
    if spec.max_egress_mbps == Some(0) || spec.max_egress_mbps.is_some_and(|v| v > 1_000_000) {
        bail!("max_egress_mbps must be in 1..=1000000 when set");
    }
    if spec.flow_sample_rate > 1_000_000_000 {
        bail!("flow_sample_rate must be <= 1000000000");
    }
    if let Some(size) = spec.maglev_table_size {
        if ![251, 509, 1021, 2039, 4093, 8191, 16381].contains(&size) {
            bail!("unsupported Maglev table size {size}");
        }
    }
    Ok(())
}

#[async_trait]
pub trait ServiceNodeClient: Send + Sync {
    async fn get_service(&self, node: &NodeTarget, name: &str) -> Result<Option<ServiceSpec>>;
    /// List Maglev service specs on a node (`GET /v1/network/services`).
    async fn list_services(&self, node: &NodeTarget) -> Result<Vec<ServiceSpec>>;
    async fn get_status(&self, node: &NodeTarget) -> Result<HostServiceStatus>;
    async fn get_advertisements(&self, node: &NodeTarget) -> Result<AdvertisementSnapshot>;
    async fn upsert_service(&self, node: &NodeTarget, spec: &ServiceSpec) -> Result<()>;
    async fn delete_service(&self, node: &NodeTarget, name: &str) -> Result<()>;
    async fn export_conntrack(&self, node: &NodeTarget, name: &str) -> Result<ConntrackSnapshot>;
    async fn import_conntrack(
        &self,
        node: &NodeTarget,
        name: &str,
        snapshot: &ConntrackSnapshot,
    ) -> Result<usize>;
    async fn export_conntrack_delta(
        &self,
        _node: &NodeTarget,
        _name: &str,
        _after_seq: u64,
        _max_entries: usize,
    ) -> Result<ConntrackDeltaBatch> {
        bail!("streaming conntrack delta export not supported by this client")
    }
    async fn import_conntrack_delta(
        &self,
        _node: &NodeTarget,
        _name: &str,
        _batch: &ConntrackDeltaBatch,
    ) -> Result<HaDeltaApplyReport> {
        bail!("streaming conntrack delta import not supported by this client")
    }
    async fn ack_conntrack_delta(
        &self,
        _node: &NodeTarget,
        _name: &str,
        _ack_seq: u64,
    ) -> Result<HaJournalStatus> {
        bail!("streaming conntrack delta ack not supported by this client")
    }
}

#[derive(Clone, Default)]
pub struct FluxVmHttpClient {
    http: reqwest::Client,
}

impl FluxVmHttpClient {
    pub fn new() -> Result<Self> {
        Ok(Self {
            http: reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(30))
                .build()?,
        })
    }

    fn url(node: &NodeTarget, path: &str) -> Result<reqwest::Url> {
        let base = reqwest::Url::parse(&node.base_url)
            .with_context(|| format!("invalid FluxVM URL for node {}", node.name))?;
        base.join(path).context("joining FluxVM service URL")
    }

    fn auth(&self, node: &NodeTarget, req: reqwest::RequestBuilder) -> reqwest::RequestBuilder {
        match &node.token {
            Some(token) => req.bearer_auth(token),
            None => req,
        }
    }

    async fn expect_ok(resp: reqwest::Response, operation: &str) -> Result<reqwest::Response> {
        if resp.status().is_success() {
            return Ok(resp);
        }
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        bail!("{operation} failed: HTTP {status}: {body}")
    }
}

#[async_trait]
impl ServiceNodeClient for FluxVmHttpClient {
    async fn get_service(&self, node: &NodeTarget, name: &str) -> Result<Option<ServiceSpec>> {
        validate_name(name)?;
        let resp = self
            .auth(
                node,
                self.http
                    .get(Self::url(node, &format!("/v1/network/services/{name}"))?),
            )
            .send()
            .await?;
        if resp.status() == reqwest::StatusCode::NOT_FOUND {
            return Ok(None);
        }
        let resp = Self::expect_ok(resp, "get service").await?;
        Ok(Some(resp.json().await?))
    }

    async fn list_services(&self, node: &NodeTarget) -> Result<Vec<ServiceSpec>> {
        let resp = self
            .auth(
                node,
                self.http.get(Self::url(node, "/v1/network/services")?),
            )
            .send()
            .await?;
        let resp = Self::expect_ok(resp, "list services").await?;
        #[derive(Deserialize)]
        struct ListReply {
            items: Vec<ServiceSpec>,
        }
        Ok(resp.json::<ListReply>().await?.items)
    }

    async fn get_status(&self, node: &NodeTarget) -> Result<HostServiceStatus> {
        let resp = self
            .auth(
                node,
                self.http
                    .get(Self::url(node, "/v1/network/services/status")?),
            )
            .send()
            .await?;
        let resp = Self::expect_ok(resp, "get service dataplane status").await?;
        Ok(resp.json().await?)
    }

    async fn get_advertisements(&self, node: &NodeTarget) -> Result<AdvertisementSnapshot> {
        let resp = self
            .auth(
                node,
                self.http
                    .get(Self::url(node, "/v1/network/services/advertisements")?),
            )
            .send()
            .await?;
        let resp = Self::expect_ok(resp, "get service advertisements").await?;
        Ok(resp.json().await?)
    }

    async fn upsert_service(&self, node: &NodeTarget, spec: &ServiceSpec) -> Result<()> {
        validate_spec(spec)?;
        let resp = self
            .auth(
                node,
                self.http.post(Self::url(node, "/v1/network/services")?),
            )
            .json(spec)
            .send()
            .await?;
        Self::expect_ok(resp, "upsert service").await?;
        Ok(())
    }

    async fn delete_service(&self, node: &NodeTarget, name: &str) -> Result<()> {
        validate_name(name)?;
        let resp = self
            .auth(
                node,
                self.http
                    .delete(Self::url(node, &format!("/v1/network/services/{name}"))?),
            )
            .send()
            .await?;
        if resp.status() == reqwest::StatusCode::NOT_FOUND {
            return Ok(());
        }
        Self::expect_ok(resp, "delete service").await?;
        Ok(())
    }

    async fn export_conntrack(&self, node: &NodeTarget, name: &str) -> Result<ConntrackSnapshot> {
        validate_name(name)?;
        let resp = self
            .auth(
                node,
                self.http.get(Self::url(
                    node,
                    &format!("/v1/network/services/{name}/conntrack/export"),
                )?),
            )
            .send()
            .await?;
        let resp = Self::expect_ok(resp, "export service conntrack").await?;
        Ok(resp.json().await?)
    }

    async fn import_conntrack(
        &self,
        node: &NodeTarget,
        name: &str,
        snapshot: &ConntrackSnapshot,
    ) -> Result<usize> {
        validate_name(name)?;
        let resp = self
            .auth(
                node,
                self.http.post(Self::url(
                    node,
                    &format!("/v1/network/services/{name}/conntrack/import"),
                )?),
            )
            .json(snapshot)
            .send()
            .await?;
        let resp = Self::expect_ok(resp, "import service conntrack").await?;
        #[derive(Deserialize)]
        struct ImportReply {
            written: usize,
        }
        Ok(resp.json::<ImportReply>().await?.written)
    }

    async fn export_conntrack_delta(
        &self,
        node: &NodeTarget,
        name: &str,
        after_seq: u64,
        max_entries: usize,
    ) -> Result<ConntrackDeltaBatch> {
        validate_name(name)?;
        let mut url = Self::url(
            node,
            &format!("/v1/network/services/{name}/conntrack/delta"),
        )?;
        url.query_pairs_mut()
            .append_pair("after_seq", &after_seq.to_string())
            .append_pair("max_entries", &max_entries.to_string());
        let resp = self.auth(node, self.http.get(url)).send().await?;
        let resp = Self::expect_ok(resp, "export service conntrack delta").await?;
        Ok(resp.json().await?)
    }

    async fn import_conntrack_delta(
        &self,
        node: &NodeTarget,
        name: &str,
        batch: &ConntrackDeltaBatch,
    ) -> Result<HaDeltaApplyReport> {
        validate_name(name)?;
        let resp = self
            .auth(
                node,
                self.http.post(Self::url(
                    node,
                    &format!("/v1/network/services/{name}/conntrack/delta/import"),
                )?),
            )
            .json(batch)
            .send()
            .await?;
        let resp = Self::expect_ok(resp, "import service conntrack delta").await?;
        Ok(resp.json().await?)
    }

    async fn ack_conntrack_delta(
        &self,
        node: &NodeTarget,
        name: &str,
        ack_seq: u64,
    ) -> Result<HaJournalStatus> {
        validate_name(name)?;
        let resp = self
            .auth(
                node,
                self.http.post(Self::url(
                    node,
                    &format!("/v1/network/services/{name}/conntrack/delta/ack"),
                )?),
            )
            .json(&serde_json::json!({"ack_seq": ack_seq}))
            .send()
            .await?;
        let resp = Self::expect_ok(resp, "ack service conntrack delta").await?;
        Ok(resp.json().await?)
    }
}

pub struct ServiceOrchestrator<C> {
    client: C,
}

impl<C> ServiceOrchestrator<C>
where
    C: ServiceNodeClient,
{
    pub fn new(client: C) -> Self {
        Self { client }
    }

    pub async fn apply(&self, spec: &ServiceSpec, nodes: &[NodeTarget]) -> Result<ApplyReport> {
        validate_spec(spec)?;
        if nodes.is_empty() {
            bail!("service apply requires at least one FluxVM node");
        }

        // North-south intent must only fan out to nodes actually configured
        // as service edges. Fabric checks this before taking any snapshot or
        // mutating service state, so a topology mistake has zero side effects.
        if spec.exposure.north_south() {
            for node in nodes {
                let status = self.client.get_status(node).await?;
                if status.schema_version < 3 {
                    bail!(
                        "node '{}' exposes service schema {}, need v3 for lifecycle/HA service intent",
                        node.name,
                        status.schema_version
                    );
                }
                if status.north_south_interfaces.is_empty() {
                    bail!(
                        "node '{}' has no FluxVM north-south interfaces configured",
                        node.name
                    );
                }
            }
        }

        let mut before = Vec::with_capacity(nodes.len());
        for node in nodes {
            before.push((
                node.clone(),
                self.client.get_service(node, &spec.name).await?,
            ));
        }

        let mut applied: Vec<NodeTarget> = Vec::new();
        for node in nodes {
            if let Err(error) = self.client.upsert_service(node, spec).await {
                let rolled = self.rollback(&spec.name, &applied, &before).await;
                bail!(
                    "service '{}' failed on node '{}': {error:#}; rolled back {:?}",
                    spec.name,
                    node.name,
                    rolled
                );
            }
            applied.push(node.clone());
        }
        Ok(ApplyReport {
            service: spec.name.clone(),
            applied_nodes: applied.into_iter().map(|n| n.name).collect(),
            rolled_back_nodes: Vec::new(),
        })
    }

    pub async fn delete(&self, name: &str, nodes: &[NodeTarget]) -> Result<ApplyReport> {
        validate_name(name)?;
        if nodes.is_empty() {
            bail!("service delete requires at least one FluxVM node");
        }
        let mut before = Vec::with_capacity(nodes.len());
        for node in nodes {
            before.push((node.clone(), self.client.get_service(node, name).await?));
        }
        let mut applied = Vec::new();
        for node in nodes {
            if let Err(error) = self.client.delete_service(node, name).await {
                let rolled = self.rollback(name, &applied, &before).await;
                bail!(
                    "service delete '{}' failed on node '{}': {error:#}; rolled back {:?}",
                    name,
                    node.name,
                    rolled
                );
            }
            applied.push(node.clone());
        }
        Ok(ApplyReport {
            service: name.into(),
            applied_nodes: applied.into_iter().map(|n| n.name).collect(),
            rolled_back_nodes: Vec::new(),
        })
    }

    /// Apply one service to all nodes while only active Fabric edge leases
    /// are allowed to advertise the VIP. The same transactional rollback
    /// semantics as `apply` are preserved.
    pub async fn apply_with_edge_leases(
        &self,
        spec: &ServiceSpec,
        nodes: &[NodeTarget],
        leases: &[EdgeLease],
        now_unix_ms: u64,
    ) -> Result<ApplyReport> {
        validate_spec(spec)?;
        validate_leases(&spec.name, nodes, leases, now_unix_ms)?;
        if nodes.is_empty() {
            bail!("service apply requires at least one FluxVM node");
        }
        if spec.exposure.north_south() {
            for node in nodes {
                let status = self.client.get_status(node).await?;
                if status.schema_version < 3 || status.north_south_interfaces.is_empty() {
                    bail!("node '{}' is not a Service Fabric v3 edge", node.name);
                }
            }
        }
        let mut before = Vec::with_capacity(nodes.len());
        for node in nodes {
            before.push((
                node.clone(),
                self.client.get_service(node, &spec.name).await?,
            ));
        }
        // Cross-site leases are ignored for anycast; only owning-domain leases advertise.
        let active: HashSet<&str> = active_domain_leases(spec, leases, now_unix_ms)
            .into_iter()
            .map(|lease| lease.node.as_str())
            .collect();
        let mut applied = Vec::new();
        for node in nodes {
            let mut node_spec = spec.clone();
            let advertise = spec.exposure.north_south() && active.contains(node.name.as_str());
            // Fail closed: never apply advertise=true from a mismatched route domain.
            if advertise {
                let lease = leases
                    .iter()
                    .find(|l| l.active(&spec.name, now_unix_ms) && l.node == node.name);
                match lease {
                    Some(l) if lease_matches_service_domain(l, spec) => {}
                    Some(l) => bail!(
                        "refuse advertise=true across mismatched route domains: lease node '{}' domain {:?} {:?} vs service {:?} {:?}",
                        l.node,
                        l.site_id,
                        l.route_domain,
                        spec.site_id,
                        spec.route_domain
                    ),
                    None => bail!(
                        "refuse advertise=true without an owning-domain lease on node '{}'",
                        node.name
                    ),
                }
            }
            node_spec.advertise = advertise;
            if let Err(error) = self.client.upsert_service(node, &node_spec).await {
                let rolled = self.rollback(&spec.name, &applied, &before).await;
                bail!(
                    "leased service '{}' failed on node '{}': {error:#}; rolled back {:?}",
                    spec.name,
                    node.name,
                    rolled
                );
            }
            applied.push(node.clone());
        }
        Ok(ApplyReport {
            service: spec.name.clone(),
            applied_nodes: applied.into_iter().map(|n| n.name).collect(),
            rolled_back_nodes: Vec::new(),
        })
    }

    /// Build BGP/ECMP intent only from nodes that have both an active Fabric
    /// lease and a positive FluxVM local advertisement decision. This combines
    /// distributed fencing with node-local backend health.
    pub async fn healthy_bgp_vip_intent(
        &self,
        spec: &ServiceSpec,
        nodes: &[NodeTarget],
        leases: &[EdgeLease],
        now_unix_ms: u64,
    ) -> Result<BgpVipIntent> {
        validate_spec(spec)?;
        validate_leases(&spec.name, nodes, leases, now_unix_ms)?;
        let domain_leases = active_domain_leases(spec, leases, now_unix_ms);
        let active: HashSet<&str> = domain_leases
            .iter()
            .map(|lease| lease.node.as_str())
            .collect();
        let mut edge_nodes = Vec::new();
        for node in nodes
            .iter()
            .filter(|node| active.contains(node.name.as_str()))
        {
            let snapshot = self.client.get_advertisements(node).await?;
            let local_ok = snapshot
                .items
                .iter()
                .any(|item| item.service == spec.name && item.vip == spec.vip && item.advertise);
            if local_ok {
                edge_nodes.push(node.name.clone());
            }
        }
        edge_nodes.sort();
        let lease_epoch = domain_leases
            .iter()
            .map(|lease| lease.epoch)
            .max()
            .unwrap_or(0);
        Ok(BgpVipIntent {
            service: spec.name.clone(),
            vip: spec.vip,
            prefix_len: if spec.vip.is_ipv4() { 32 } else { 128 },
            edge_nodes,
            lease_epoch,
        })
    }

    /// Withdraw an edge before fencing or maintenance. Existing dataplane
    /// service state remains installed; only VIP advertisement is disabled.
    pub async fn withdraw_node(&self, name: &str, node: &NodeTarget) -> Result<()> {
        let Some(mut spec) = self.client.get_service(node, name).await? else {
            return Ok(());
        };
        spec.advertise = false;
        self.client.upsert_service(node, &spec).await
    }

    /// Replicate the whitelisted service conntrack/NAT maps to standby edge
    /// nodes. Fabric decides when this is required; FluxVM validates every
    /// imported map and service id.
    pub async fn replicate_conntrack(
        &self,
        name: &str,
        source: &NodeTarget,
        targets: &[NodeTarget],
    ) -> Result<ConntrackReplicationReport> {
        let snapshot = self.client.export_conntrack(source, name).await?;
        let entries = snapshot.entries.len();
        let mut replicated_nodes = Vec::new();
        for target in targets {
            self.client
                .import_conntrack(target, name, &snapshot)
                .await?;
            replicated_nodes.push(target.name.clone());
        }
        Ok(ConntrackReplicationReport {
            service: name.into(),
            source_node: source.name.clone(),
            replicated_nodes,
            entries,
        })
    }

    /// Replicate bounded sequence/ack deltas to warm standbys. Each target
    /// carries its own durable cursor. A journal gap triggers one full
    /// snapshot followed by a replay-cursor barrier; source compaction only
    /// advances to the minimum cursor acknowledged by every target.
    pub async fn replicate_conntrack_streaming(
        &self,
        name: &str,
        source: &NodeTarget,
        targets: &[NodeTarget],
        store: &StreamingReplicationStore,
        max_entries: usize,
    ) -> Result<StreamingReplicationReport> {
        validate_name(name)?;
        if targets.is_empty() {
            bail!("streaming HA replication requires at least one target");
        }
        let mut state = store.load()?;
        let mut report = StreamingReplicationReport {
            service: name.to_string(),
            source_node: source.name.clone(),
            ..StreamingReplicationReport::default()
        };
        let mut target_cursors = Vec::new();

        for target in targets {
            let key = replication_cursor_key(name, &source.name, &target.name);
            let mut cursor = *state.cursors.get(&key).unwrap_or(&0);
            loop {
                let batch = self
                    .client
                    .export_conntrack_delta(source, name, cursor, max_entries)
                    .await?;
                if batch.reset_required {
                    let snapshot = self.client.export_conntrack(source, name).await?;
                    self.client
                        .import_conntrack(target, name, &snapshot)
                        .await?;
                    let mut barrier = batch.clone();
                    barrier.reset_required = false;
                    barrier.snapshot_barrier = true;
                    barrier.entries.clear();
                    barrier.truncated = false;
                    self.client
                        .import_conntrack_delta(target, name, &barrier)
                        .await?;
                    cursor = batch.last_seq;
                    report.full_resync_nodes.push(target.name.clone());
                    break;
                }
                if batch.entries.is_empty() {
                    cursor = cursor.max(batch.after_seq);
                    break;
                }
                let applied = self
                    .client
                    .import_conntrack_delta(target, name, &batch)
                    .await?;
                report.delta_entries = report.delta_entries.saturating_add(
                    applied
                        .applied_entries
                        .saturating_add(applied.deleted_entries),
                );
                cursor = applied.last_applied_seq;
                if !batch.truncated {
                    break;
                }
            }
            state.cursors.insert(key, cursor);
            state.generation = state.generation.saturating_add(1);
            store.save(&state)?;
            target_cursors.push(cursor);
            report.replicated_nodes.push(target.name.clone());
        }
        let ack = target_cursors.into_iter().min().unwrap_or(0);
        let ack_status = self.client.ack_conntrack_delta(source, name, ack).await?;
        report.acked_seq = ack_status.acked_seq;
        Ok(report)
    }

    /// Durable edge-controller iteration: withdraw unhealthy leased edges
    /// before releasing them, then acquire deterministic replacements and
    /// verify local advertisement readiness before considering them active.
    pub async fn reconcile_edge_leases(
        &self,
        spec: &ServiceSpec,
        nodes: &[NodeTarget],
        store: &DurableLeaseStore,
        desired_edges: usize,
        ttl_ms: u64,
        now_unix_ms: u64,
    ) -> Result<EdgeReconcileReport> {
        validate_spec(spec)?;
        if !spec.exposure.north_south() {
            bail!("edge lease reconciliation requires north-south exposure");
        }
        if desired_edges == 0 || desired_edges > nodes.len() {
            bail!("desired_edges must be in 1..=node count");
        }
        let known: HashMap<&str, &NodeTarget> =
            nodes.iter().map(|n| (n.name.as_str(), n)).collect();
        let mut report = EdgeReconcileReport {
            service: spec.name.clone(),
            ..EdgeReconcileReport::default()
        };
        let _ = store.prune_expired(now_unix_ms)?;

        for lease in store.active(&spec.name, now_unix_ms)? {
            if !lease_matches_service_domain(&lease, spec) {
                continue;
            }
            let Some(node) = known.get(lease.node.as_str()).copied() else {
                store.release(&spec.name, &lease.node)?;
                continue;
            };
            let healthy = self
                .client
                .get_advertisements(node)
                .await
                .ok()
                .is_some_and(|snapshot| {
                    snapshot.items.iter().any(|item| {
                        item.service == spec.name && item.vip == spec.vip && item.advertise
                    })
                });
            if !healthy {
                self.withdraw_node(&spec.name, node).await?;
                store.release(&spec.name, &node.name)?;
                report.withdrawn_nodes.push(node.name.clone());
            } else {
                // Keep the fencing epoch stable while refreshing only expiry.
                store.extend(&spec.name, &node.name, ttl_ms, now_unix_ms)?;
            }
        }

        let mut active: Vec<_> = store
            .active(&spec.name, now_unix_ms)?
            .into_iter()
            .filter(|lease| lease_matches_service_domain(lease, spec))
            .collect();
        let active_names: HashSet<String> = active.iter().map(|l| l.node.clone()).collect();
        // Prefer the first surviving active edge as a one-time state seed for
        // a replacement. Continuous delta replication keeps normal standbys warm.
        let seed_source = active
            .iter()
            .find_map(|lease| known.get(lease.node.as_str()).copied());
        let mut candidates: Vec<_> = nodes
            .iter()
            .filter(|n| !active_names.contains(&n.name))
            .collect();
        candidates.sort_by(|a, b| a.name.cmp(&b.name));
        for node in candidates {
            if active.len() >= desired_edges {
                break;
            }
            let lease = store.renew_in_domain(
                &spec.name,
                &node.name,
                ttl_ms,
                now_unix_ms,
                spec.site_id.clone(),
                spec.route_domain.clone(),
            )?;
            let mut staged = spec.clone();
            staged.advertise = false;
            if let Err(error) = self.client.upsert_service(node, &staged).await {
                store.release(&spec.name, &node.name)?;
                tracing::warn!(service=%spec.name, node=%node.name, %error, "replacement edge staging failed");
                continue;
            }
            if let Some(source) = seed_source {
                let seeded = async {
                    let snapshot = self.client.export_conntrack(source, &spec.name).await?;
                    self.client
                        .import_conntrack(node, &spec.name, &snapshot)
                        .await?;
                    Ok::<(), anyhow::Error>(())
                }
                .await;
                if let Err(error) = seeded {
                    store.release(&spec.name, &node.name)?;
                    tracing::warn!(service=%spec.name, node=%node.name, %error, "replacement edge state seed failed");
                    continue;
                }
                report.state_seeded_nodes.push(node.name.clone());
            }
            let mut advertised = staged;
            advertised.advertise = true;
            if let Err(error) = self.client.upsert_service(node, &advertised).await {
                store.release(&spec.name, &node.name)?;
                tracing::warn!(service=%spec.name, node=%node.name, %error, "replacement edge advertise failed");
                continue;
            }
            let ready = self
                .client
                .get_advertisements(node)
                .await
                .ok()
                .is_some_and(|snapshot| {
                    snapshot.items.iter().any(|item| {
                        item.service == spec.name && item.vip == spec.vip && item.advertise
                    })
                });
            if !ready {
                self.withdraw_node(&spec.name, node).await?;
                store.release(&spec.name, &node.name)?;
                continue;
            }
            report.replacement_nodes.push(node.name.clone());
            active.push(lease);
        }
        if active.len() < desired_edges {
            bail!(
                "only {} healthy edge leases available; desired {}",
                active.len(),
                desired_edges
            );
        }
        active.sort_by(|a, b| a.node.cmp(&b.node));
        report.lease_epoch = active.iter().map(|l| l.epoch).max().unwrap_or(0);
        report.active_nodes = active.into_iter().map(|l| l.node).collect();
        Ok(report)
    }

    async fn rollback(
        &self,
        name: &str,
        applied: &[NodeTarget],
        before: &[(NodeTarget, Option<ServiceSpec>)],
    ) -> Vec<String> {
        let previous: HashMap<&str, &Option<ServiceSpec>> = before
            .iter()
            .map(|(node, spec)| (node.name.as_str(), spec))
            .collect();
        let mut rolled = Vec::new();
        for node in applied.iter().rev() {
            let result = match previous
                .get(node.name.as_str())
                .and_then(|spec| spec.as_ref())
            {
                Some(spec) => self.client.upsert_service(node, spec).await,
                None => self.client.delete_service(node, name).await,
            };
            if result.is_ok() {
                rolled.push(node.name.clone());
            }
        }
        rolled
    }
}

pub fn validate_leases(
    service: &str,
    nodes: &[NodeTarget],
    leases: &[EdgeLease],
    now_unix_ms: u64,
) -> Result<()> {
    let known: HashSet<&str> = nodes.iter().map(|n| n.name.as_str()).collect();
    let mut seen = HashSet::new();
    for lease in leases.iter().filter(|l| l.active(service, now_unix_ms)) {
        if !known.contains(lease.node.as_str()) {
            bail!("active edge lease references unknown node '{}'", lease.node);
        }
        if !seen.insert(lease.node.as_str()) {
            bail!("duplicate active edge lease for node '{}'", lease.node);
        }
    }
    Ok(())
}

pub fn bgp_vip_intent(
    spec: &ServiceSpec,
    leases: &[EdgeLease],
    now_unix_ms: u64,
) -> Result<BgpVipIntent> {
    validate_spec(spec)?;
    if !spec.exposure.north_south() {
        bail!("BGP VIP intent requires north-south or both exposure");
    }
    // Cross-site leases are ignored for anycast; only the owning domain advertises.
    let active = active_domain_leases(spec, leases, now_unix_ms);
    let lease_epoch = active.iter().map(|l| l.epoch).max().unwrap_or(0);
    Ok(BgpVipIntent {
        service: spec.name.clone(),
        vip: spec.vip,
        prefix_len: if spec.vip.is_ipv4() { 32 } else { 128 },
        edge_nodes: active.into_iter().map(|l| l.node.clone()).collect(),
        lease_epoch,
    })
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum BgpAdapterKind {
    Frr,
    Bird,
    File,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct BgpAdapterConfig {
    pub kind: BgpAdapterKind,
    #[serde(default = "default_vtysh")]
    pub frr_vtysh: PathBuf,
    #[serde(default)]
    pub local_asn: u32,
    #[serde(default = "default_birdc")]
    pub birdc: PathBuf,
    #[serde(default = "default_bird_include")]
    pub bird_include: PathBuf,
    #[serde(default = "default_intent_path")]
    pub file_path: PathBuf,
}

fn default_vtysh() -> PathBuf {
    "/usr/bin/vtysh".into()
}
fn default_birdc() -> PathBuf {
    "/usr/sbin/birdc".into()
}
fn default_bird_include() -> PathBuf {
    "/run/zyvor/fabric-bird-routes.conf".into()
}
fn default_intent_path() -> PathBuf {
    "/run/zyvor/bgp-service-intent.json".into()
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct BgpApplyReport {
    pub adapter: BgpAdapterKind,
    pub advertised: Vec<String>,
    pub withdrawn: Vec<String>,
}

fn host_prefix(ip: IpAddr) -> String {
    format!("{ip}/{}", if ip.is_ipv4() { 32 } else { 128 })
}

fn intent_prefixes(intent: &BgpVipIntent) -> Vec<String> {
    if intent.edge_nodes.is_empty() {
        Vec::new()
    } else {
        vec![host_prefix(intent.vip)]
    }
}

pub(crate) fn atomic_write(path: &Path, bytes: &[u8]) -> Result<()> {
    let parent = path.parent().context("atomic write path has no parent")?;
    fs::create_dir_all(parent)?;
    let tmp = path.with_extension("tmp");
    {
        let mut f = fs::File::create(&tmp)?;
        f.write_all(bytes)?;
        f.sync_all()?;
    }
    fs::rename(&tmp, path)?;
    fs::File::open(parent)?.sync_all()?;
    Ok(())
}

pub fn render_frr_commands(
    local_asn: u32,
    desired: &BgpVipIntent,
    previous: Option<&BgpVipIntent>,
) -> Result<Vec<String>> {
    if local_asn == 0 {
        bail!("FRR local_asn must be non-zero");
    }
    let wanted: HashSet<String> = intent_prefixes(desired).into_iter().collect();
    let before: HashSet<String> = previous
        .map(intent_prefixes)
        .unwrap_or_default()
        .into_iter()
        .collect();
    let mut commands = vec![
        "configure terminal".to_string(),
        format!("router bgp {local_asn}"),
    ];
    for family in [4u8, 6u8] {
        let afi = if family == 4 {
            "ipv4 unicast"
        } else {
            "ipv6 unicast"
        };
        let changed = wanted.iter().chain(before.iter()).any(|p| {
            p.split('/')
                .next()
                .and_then(|ip| ip.parse::<IpAddr>().ok())
                .is_some_and(|ip| {
                    if family == 4 {
                        ip.is_ipv4()
                    } else {
                        ip.is_ipv6()
                    }
                })
        });
        if !changed {
            continue;
        }
        commands.push(format!("address-family {afi}"));
        let mut withdrawn: Vec<_> = before.difference(&wanted).cloned().collect();
        withdrawn.sort();
        let mut advertised: Vec<_> = wanted.difference(&before).cloned().collect();
        advertised.sort();
        for prefix in withdrawn {
            let ip: IpAddr = prefix.split('/').next().unwrap().parse()?;
            if (family == 4 && ip.is_ipv4()) || (family == 6 && ip.is_ipv6()) {
                commands.push(format!("no network {prefix}"));
            }
        }
        for prefix in advertised {
            let ip: IpAddr = prefix.split('/').next().unwrap().parse()?;
            if (family == 4 && ip.is_ipv4()) || (family == 6 && ip.is_ipv6()) {
                commands.push(format!("network {prefix}"));
            }
        }
        commands.push("exit-address-family".into());
    }
    commands.push("end".into());
    Ok(commands)
}

pub fn render_bird_routes(intent: &BgpVipIntent) -> String {
    // Intended to be included inside an operator-owned BIRD `protocol static`
    // block. Fabric owns only these route statements, not the BGP session.
    let mut routes = intent_prefixes(intent);
    routes.sort();
    let mut out = String::from("# generated by Zyvor Fabric Service Fabric v4\n");
    for prefix in routes {
        out.push_str(&format!("route {prefix} blackhole;\n"));
    }
    out
}

pub fn reconcile_bgp(
    cfg: &BgpAdapterConfig,
    desired: &BgpVipIntent,
    previous: Option<&BgpVipIntent>,
) -> Result<BgpApplyReport> {
    let wanted: HashSet<String> = intent_prefixes(desired).into_iter().collect();
    let before: HashSet<String> = previous
        .map(intent_prefixes)
        .unwrap_or_default()
        .into_iter()
        .collect();
    let mut advertised: Vec<_> = wanted.difference(&before).cloned().collect();
    advertised.sort();
    let mut withdrawn: Vec<_> = before.difference(&wanted).cloned().collect();
    withdrawn.sort();
    match cfg.kind {
        BgpAdapterKind::File => {
            atomic_write(&cfg.file_path, &serde_json::to_vec_pretty(desired)?)?;
        }
        BgpAdapterKind::Bird => {
            atomic_write(&cfg.bird_include, render_bird_routes(desired).as_bytes())?;
            let out = Command::new(&cfg.birdc)
                .arg("configure")
                .output()
                .with_context(|| format!("running {} configure", cfg.birdc.display()))?;
            if !out.status.success() {
                bail!(
                    "birdc configure failed: {}",
                    String::from_utf8_lossy(&out.stderr).trim()
                );
            }
        }
        BgpAdapterKind::Frr => {
            // One vtysh process receives the whole configuration transaction;
            // mode changes such as `configure terminal` and `router bgp` are
            // therefore preserved across commands. Arguments are passed
            // directly -- never through a shell.
            let commands = render_frr_commands(cfg.local_asn, desired, previous)?;
            let mut cmd = Command::new(&cfg.frr_vtysh);
            for command in &commands {
                cmd.arg("-c").arg(command);
            }
            let out = cmd.output().with_context(|| {
                format!("running FRR transaction via {}", cfg.frr_vtysh.display())
            })?;
            if !out.status.success() {
                bail!(
                    "FRR transaction failed: {}",
                    String::from_utf8_lossy(&out.stderr).trim()
                );
            }
        }
    }
    Ok(BgpApplyReport {
        adapter: cfg.kind,
        advertised,
        withdrawn,
    })
}

fn validate_name(name: &str) -> Result<()> {
    if name.is_empty()
        || name.len() > 63
        || !name
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, b'-' | b'_' | b'.'))
    {
        bail!("invalid FluxVM service name {name:?}");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use tokio::sync::Mutex;

    #[derive(Clone, Default)]
    struct FakeClient {
        state: Arc<Mutex<HashMap<(String, String), ServiceSpec>>>,
        status: Arc<Mutex<HashMap<String, HostServiceStatus>>>,
        fail_upsert_on: Arc<Mutex<Option<String>>>,
        fail_delete_on: Arc<Mutex<Option<String>>>,
        snapshots: Arc<Mutex<HashMap<(String, String), ConntrackSnapshot>>>,
        advertisements: Arc<Mutex<HashMap<String, AdvertisementSnapshot>>>,
    }

    #[async_trait]
    impl ServiceNodeClient for FakeClient {
        async fn get_service(&self, node: &NodeTarget, name: &str) -> Result<Option<ServiceSpec>> {
            Ok(self
                .state
                .lock()
                .await
                .get(&(node.name.clone(), name.into()))
                .cloned())
        }

        async fn list_services(&self, node: &NodeTarget) -> Result<Vec<ServiceSpec>> {
            let guard = self.state.lock().await;
            let mut out: Vec<_> = guard
                .iter()
                .filter_map(|((n, _), spec)| {
                    if n == &node.name {
                        Some(spec.clone())
                    } else {
                        None
                    }
                })
                .collect();
            out.sort_by(|a, b| a.name.cmp(&b.name));
            Ok(out)
        }

        async fn get_status(&self, node: &NodeTarget) -> Result<HostServiceStatus> {
            Ok(self
                .status
                .lock()
                .await
                .get(&node.name)
                .cloned()
                .unwrap_or(HostServiceStatus {
                    schema_version: 3,
                    program_generation: 6,
                    north_south_interfaces: vec!["eno1".into()],
                    xdp_acceleration: true,
                    cgroup_connect: false,
                    cgroup_connect_attached: false,
                    map_tier: String::new(),
                    interfaces: Vec::new(),
                }))
        }

        async fn get_advertisements(&self, node: &NodeTarget) -> Result<AdvertisementSnapshot> {
            Ok(self
                .advertisements
                .lock()
                .await
                .get(&node.name)
                .cloned()
                .unwrap_or(AdvertisementSnapshot {
                    schema_version: 3,
                    generation: 1,
                    items: vec![VipAdvertisement {
                        service: "payments".into(),
                        vip: "203.0.113.20".parse().unwrap(),
                        prefix_len: 32,
                        advertise: true,
                        reason: "test".into(),
                    }],
                }))
        }

        async fn upsert_service(&self, node: &NodeTarget, spec: &ServiceSpec) -> Result<()> {
            if self.fail_upsert_on.lock().await.as_deref() == Some(node.name.as_str()) {
                bail!("injected upsert failure");
            }
            self.state
                .lock()
                .await
                .insert((node.name.clone(), spec.name.clone()), spec.clone());
            Ok(())
        }

        async fn delete_service(&self, node: &NodeTarget, name: &str) -> Result<()> {
            if self.fail_delete_on.lock().await.as_deref() == Some(node.name.as_str()) {
                bail!("injected delete failure");
            }
            self.state
                .lock()
                .await
                .remove(&(node.name.clone(), name.into()));
            Ok(())
        }

        async fn export_conntrack(
            &self,
            node: &NodeTarget,
            name: &str,
        ) -> Result<ConntrackSnapshot> {
            Ok(self
                .snapshots
                .lock()
                .await
                .get(&(node.name.clone(), name.into()))
                .cloned()
                .unwrap_or(ConntrackSnapshot {
                    schema_version: 3,
                    service: name.into(),
                    service_id: 1,
                    created_unix_ms: 1,
                    entries: vec![RawMapEntry {
                        map: "fluxvm_fct4".into(),
                        key_hex: "00".into(),
                        value_hex: "00".into(),
                    }],
                }))
        }

        async fn import_conntrack(
            &self,
            node: &NodeTarget,
            name: &str,
            snapshot: &ConntrackSnapshot,
        ) -> Result<usize> {
            self.snapshots
                .lock()
                .await
                .insert((node.name.clone(), name.into()), snapshot.clone());
            Ok(snapshot.entries.len())
        }
    }

    fn nodes() -> Vec<NodeTarget> {
        ["a", "b", "c"]
            .into_iter()
            .map(|name| NodeTarget {
                name: name.into(),
                base_url: format!("http://{name}:7788"),
                token: None,
            })
            .collect()
    }

    fn service(port: u16) -> ServiceSpec {
        ServiceSpec {
            name: "payments".into(),
            vip: "10.40.0.100".parse().unwrap(),
            port,
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
            site_id: None,
            route_domain: None,
        }
    }

    #[test]
    fn validates_ipv6_north_south_nat() {
        let mut s = service(443);
        s.vip = "2001:db8:40::100".parse().unwrap();
        s.backends[0].address = "2001:db8:40:1::21".parse().unwrap();
        s.exposure = ServiceExposure::NorthSouth;
        s.snat_address = Some("2001:db8:ffff::10".parse().unwrap());
        assert!(validate_spec(&s).is_ok());
    }

    #[test]
    fn dsr_rejects_snat_and_port_translation() {
        let mut s = service(443);
        s.mode = ServiceMode::Dsr;
        assert!(validate_spec(&s).is_err());
        s.backends[0].port = 443;
        assert!(validate_spec(&s).is_ok());
        s.snat_address = Some("192.0.2.5".parse().unwrap());
        assert!(validate_spec(&s).is_err());
    }

    #[tokio::test]
    async fn applies_to_every_node() {
        let client = FakeClient::default();
        let orch = ServiceOrchestrator::new(client.clone());
        let report = orch.apply(&service(443), &nodes()).await.unwrap();
        assert_eq!(report.applied_nodes.len(), 3);
        assert_eq!(client.state.lock().await.len(), 3);
    }

    #[tokio::test]
    async fn north_south_preflight_is_side_effect_free() {
        let client = FakeClient::default();
        let ns = nodes();
        client.status.lock().await.insert(
            "b".into(),
            HostServiceStatus {
                schema_version: 3,
                program_generation: 6,
                north_south_interfaces: Vec::new(),
                xdp_acceleration: false,
                cgroup_connect: false,
                cgroup_connect_attached: false,
                map_tier: String::new(),
                interfaces: Vec::new(),
            },
        );
        let mut s = service(443);
        s.exposure = ServiceExposure::NorthSouth;
        s.snat_address = Some("192.0.2.10".parse().unwrap());
        let orch = ServiceOrchestrator::new(client.clone());
        assert!(orch.apply(&s, &ns).await.is_err());
        assert!(client.state.lock().await.is_empty());
    }

    #[tokio::test]
    async fn failed_fanout_rolls_back_previously_updated_nodes() {
        let client = FakeClient::default();
        let ns = nodes();
        let old = service(443);
        for node in &ns {
            client
                .state
                .lock()
                .await
                .insert((node.name.clone(), old.name.clone()), old.clone());
        }
        *client.fail_upsert_on.lock().await = Some("b".into());
        let orch = ServiceOrchestrator::new(client.clone());
        assert!(orch.apply(&service(8443), &ns).await.is_err());
        let state = client.state.lock().await;
        assert_eq!(
            state.get(&("a".into(), "payments".into())).unwrap().port,
            443
        );
        assert_eq!(
            state.get(&("b".into(), "payments".into())).unwrap().port,
            443
        );
        assert_eq!(
            state.get(&("c".into(), "payments".into())).unwrap().port,
            443
        );
    }

    #[test]
    fn draining_backend_and_health_contract_validate() {
        let mut s = service(443);
        s.backends[0].state = BackendState::Draining;
        s.backends[0].drain_until_unix_ms = Some(10_000);
        s.health_check = Some(ServiceHealthCheck::default());
        assert!(validate_spec(&s).is_ok());
    }

    #[test]
    fn bgp_intent_uses_only_live_leases() {
        let mut s = service(443);
        s.exposure = ServiceExposure::NorthSouth;
        s.snat_address = Some("192.0.2.10".parse().unwrap());
        let leases = vec![
            EdgeLease {
                service: "payments".into(),
                node: "a".into(),
                epoch: 7,
                expires_unix_ms: 200,
                site_id: None,
                route_domain: None,
            },
            EdgeLease {
                service: "payments".into(),
                node: "b".into(),
                epoch: 6,
                expires_unix_ms: 50,
                site_id: None,
                route_domain: None,
            },
        ];
        let intent = bgp_vip_intent(&s, &leases, 100).unwrap();
        assert_eq!(intent.edge_nodes, vec!["a"]);
        assert_eq!(intent.lease_epoch, 7);
    }

    #[tokio::test]
    async fn leased_apply_advertises_only_active_edge() {
        let client = FakeClient::default();
        let ns = nodes();
        let mut s = service(443);
        s.exposure = ServiceExposure::NorthSouth;
        s.snat_address = Some("192.0.2.10".parse().unwrap());
        let leases = vec![EdgeLease {
            service: "payments".into(),
            node: "b".into(),
            epoch: 9,
            expires_unix_ms: 200,
            site_id: None,
            route_domain: None,
        }];
        ServiceOrchestrator::new(client.clone())
            .apply_with_edge_leases(&s, &ns, &leases, 100)
            .await
            .unwrap();
        let state = client.state.lock().await;
        assert!(
            !state
                .get(&("a".into(), "payments".into()))
                .unwrap()
                .advertise
        );
        assert!(
            state
                .get(&("b".into(), "payments".into()))
                .unwrap()
                .advertise
        );
        assert!(
            !state
                .get(&("c".into(), "payments".into()))
                .unwrap()
                .advertise
        );
    }

    #[tokio::test]
    async fn conntrack_replication_fans_out() {
        let client = FakeClient::default();
        let ns = nodes();
        let report = ServiceOrchestrator::new(client.clone())
            .replicate_conntrack("payments", &ns[0], &ns[1..])
            .await
            .unwrap();
        assert_eq!(report.replicated_nodes, vec!["b", "c"]);
        assert_eq!(client.snapshots.lock().await.len(), 2);
    }

    #[tokio::test]
    async fn healthy_bgp_intent_requires_lease_and_local_readiness() {
        let client = FakeClient::default();
        let ns = nodes();
        let mut s = service(443);
        s.vip = "203.0.113.20".parse().unwrap();
        s.exposure = ServiceExposure::NorthSouth;
        s.snat_address = Some("192.0.2.10".parse().unwrap());
        client.advertisements.lock().await.insert(
            "b".into(),
            AdvertisementSnapshot {
                schema_version: 3,
                generation: 2,
                items: vec![VipAdvertisement {
                    service: "payments".into(),
                    vip: s.vip,
                    prefix_len: 32,
                    advertise: false,
                    reason: "no ready backend".into(),
                }],
            },
        );
        let leases = vec![
            EdgeLease {
                service: "payments".into(),
                node: "a".into(),
                epoch: 11,
                expires_unix_ms: 200,
                site_id: None,
                route_domain: None,
            },
            EdgeLease {
                service: "payments".into(),
                node: "b".into(),
                epoch: 11,
                expires_unix_ms: 200,
                site_id: None,
                route_domain: None,
            },
        ];
        let intent = ServiceOrchestrator::new(client)
            .healthy_bgp_vip_intent(&s, &ns, &leases, 100)
            .await
            .unwrap();
        assert_eq!(intent.edge_nodes, vec!["a"]);
    }

    #[test]
    fn frr_renderer_adds_and_withdraws_without_shell() {
        let mut s = service(443);
        s.exposure = ServiceExposure::NorthSouth;
        s.snat_address = Some("192.0.2.10".parse().unwrap());
        let mut old = bgp_vip_intent(
            &s,
            &[EdgeLease {
                service: "payments".into(),
                node: "a".into(),
                epoch: 1,
                expires_unix_ms: 200,
                site_id: None,
                route_domain: None,
            }],
            100,
        )
        .unwrap();
        let new = BgpVipIntent {
            edge_nodes: Vec::new(),
            lease_epoch: 2,
            ..old.clone()
        };
        let cmds = render_frr_commands(65001, &new, Some(&old)).unwrap();
        assert!(cmds.iter().any(|c| c == "no network 10.40.0.100/32"));
        old.edge_nodes.clear();
        assert!(render_bird_routes(&old).contains("generated by Zyvor"));
    }

    #[test]
    fn v5_replication_cursor_key_is_scoped() {
        assert_ne!(
            replication_cursor_key("svc", "a", "b"),
            replication_cursor_key("svc", "a", "c")
        );
        assert_ne!(
            replication_cursor_key("svc", "a", "b"),
            replication_cursor_key("other", "a", "b")
        );
    }

    #[test]
    fn v5_delta_batch_wire_contract() {
        let b = ConntrackDeltaBatch {
            schema_version: 4,
            service: "payments".into(),
            service_id: 7,
            after_seq: 9,
            last_seq: 10,
            acked_seq: 8,
            reset_required: false,
            snapshot_barrier: false,
            truncated: false,
            entries: vec![ConntrackDeltaEntry {
                seq: 10,
                operation: HaDeltaOperation::Delete,
                map: "fluxvm_fct4".into(),
                key_hex: "00".into(),
                value_hex: None,
            }],
        };
        let raw = serde_json::to_value(&b).unwrap();
        assert_eq!(raw["entries"][0]["operation"], "delete");
        assert_eq!(raw["last_seq"], 10);
    }

    #[test]
    fn same_site_lease_advertises_ok() {
        let mut s = service(443);
        s.exposure = ServiceExposure::NorthSouth;
        s.snat_address = Some("192.0.2.10".parse().unwrap());
        s.site_id = Some("site-a".into());
        s.route_domain = Some("rd-1".into());
        let leases = vec![EdgeLease {
            service: "payments".into(),
            node: "a".into(),
            epoch: 3,
            expires_unix_ms: 200,
            site_id: Some("site-a".into()),
            route_domain: Some("rd-1".into()),
        }];
        let intent = bgp_vip_intent(&s, &leases, 100).unwrap();
        assert_eq!(intent.edge_nodes, vec!["a"]);
        assert_eq!(intent.lease_epoch, 3);
    }

    #[test]
    fn cross_site_lease_ignored_for_anycast() {
        let mut s = service(443);
        s.exposure = ServiceExposure::NorthSouth;
        s.snat_address = Some("192.0.2.10".parse().unwrap());
        s.site_id = Some("site-a".into());
        s.route_domain = Some("rd-1".into());
        let leases = vec![
            EdgeLease {
                service: "payments".into(),
                node: "a".into(),
                epoch: 4,
                expires_unix_ms: 200,
                site_id: Some("site-a".into()),
                route_domain: Some("rd-1".into()),
            },
            EdgeLease {
                service: "payments".into(),
                node: "b".into(),
                epoch: 9,
                expires_unix_ms: 200,
                site_id: Some("site-b".into()),
                route_domain: Some("rd-2".into()),
            },
        ];
        let intent = bgp_vip_intent(&s, &leases, 100).unwrap();
        assert_eq!(intent.edge_nodes, vec!["a"]);
        assert_eq!(intent.lease_epoch, 4);
    }

    #[test]
    fn default_path_unaffected_by_site_fencing() {
        let mut s = service(443);
        s.exposure = ServiceExposure::NorthSouth;
        s.snat_address = Some("192.0.2.10".parse().unwrap());
        // No site_id / route_domain on service or leases → default domain.
        let leases = vec![
            EdgeLease {
                service: "payments".into(),
                node: "a".into(),
                epoch: 7,
                expires_unix_ms: 200,
                site_id: None,
                route_domain: None,
            },
            EdgeLease {
                service: "payments".into(),
                node: "c".into(),
                epoch: 2,
                expires_unix_ms: 200,
                site_id: None,
                route_domain: None,
            },
        ];
        let intent = bgp_vip_intent(&s, &leases, 100).unwrap();
        assert_eq!(intent.edge_nodes, vec!["a", "c"]);
        assert_eq!(intent.lease_epoch, 7);
        assert!(lease_matches_service_domain(&leases[0], &s));
    }

    #[tokio::test]
    async fn leased_apply_ignores_cross_site_for_advertise() {
        let client = FakeClient::default();
        let ns = nodes();
        let mut s = service(443);
        s.exposure = ServiceExposure::NorthSouth;
        s.snat_address = Some("192.0.2.10".parse().unwrap());
        s.site_id = Some("site-a".into());
        s.route_domain = Some("rd-1".into());
        let leases = vec![
            EdgeLease {
                service: "payments".into(),
                node: "a".into(),
                epoch: 1,
                expires_unix_ms: 200,
                site_id: Some("site-a".into()),
                route_domain: Some("rd-1".into()),
            },
            EdgeLease {
                service: "payments".into(),
                node: "b".into(),
                epoch: 2,
                expires_unix_ms: 200,
                site_id: Some("site-b".into()),
                route_domain: Some("rd-other".into()),
            },
        ];
        ServiceOrchestrator::new(client.clone())
            .apply_with_edge_leases(&s, &ns, &leases, 100)
            .await
            .unwrap();
        let state = client.state.lock().await;
        assert!(
            state
                .get(&("a".into(), "payments".into()))
                .unwrap()
                .advertise
        );
        assert!(
            !state
                .get(&("b".into(), "payments".into()))
                .unwrap()
                .advertise
        );
    }
}
