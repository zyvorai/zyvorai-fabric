// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: Apache-2.0

//! Thin REST client for [FluxVM](https://github.com/zyvorai/fluxvm)'s
//! `/v1/vms...` API — the disposable-VM control plane that is replacing
//! systemd-machined/systemd-vmspawn as zyvor-fabricd's VM lifecycle backend.
//!
//! This crate only wraps the wire protocol (request/response types + HTTP
//! calls); it does not implement `driver-core`'s `VMDriver` trait family —
//! that mapping lives in a separate `fluxvm-driver` crate so the raw
//! client can be reused/tested independently of that trait boundary.
//!
//! The DTOs below mirror `fluxvm-core::model` and `fluxvm-api::router`.
//! Because integration is out-of-process (REST, not a Cargo path/git
//! dependency on FluxVM's own crates), these types must be kept in sync
//! by hand when FluxVM's API changes — that's the deliberate trade for
//! not coupling zyvor-fabric's build to FluxVM's crate versions. FluxVM
//! has grown a bearer-token auth layer (`Role::Admin`/`Role::ReadOnly`)
//! since this client was first written; `FluxVmClient::with_token` covers
//! it, and stays a no-op against a deployment that leaves `auth.tokens`
//! empty (auth off — today's default posture, see the migration plan's
//! "Auth boundary" note).
//!
//! Wire types track FluxVM 0.4.x (`CreateVmRequest.storage`, jailer/vsock/
//! QGA bookkeeping on `VmRecord`, migration receivers, network-migration
//! state, pod-policy, drop-reasons, QGA). Network Fabric REST types
//! (`VmNetworkPolicy`, groups/CNP/health/ipcache, …), `NetworkSpec::Tap.netns`,
//! and the bridge-less `direct` tap (`DirectSpec`, dataplane schema 11) are
//! mirrored here. Per-direction `ppstat` counters are BPF/observer-only until
//! FluxVM exposes them on `/v1/vms/{id}/network/stats`.

use std::net::IpAddr;
use std::path::PathBuf;

use anyhow::{bail, Context, Result};
use chrono::{DateTime, Utc};
use futures::{Stream, TryStreamExt};
use serde::{Deserialize, Serialize};
use tokio::io::AsyncBufReadExt;
use uuid::Uuid;

// ============================================================================
// Wire types (mirror fluxvm-core::model)
// ============================================================================

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum BackendKind {
    Qemu,
    CloudHypervisor,
    Firecracker,
    /// In-tree FluxVM hypervisor (agent-sandbox execution track).
    FluxVm,
    /// Resolved to a concrete backend server-side; never appears on a
    /// stored `VmRecord`, only ever sent on a `CreateVmRequest`.
    Auto,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case", tag = "mode")]
pub enum NetworkSpec {
    None,
    User {
        #[serde(default)]
        forwards: Vec<PortForward>,
    },
    Tap {
        #[serde(default)]
        tap_name: Option<String>,
        #[serde(default)]
        bridge: Option<String>,
        #[serde(default)]
        mac: Option<String>,
        /// Give the VM its own network namespace with a per-namespace
        /// dnsmasq DHCP server, instead of a tap on a shared host bridge
        /// (`bridge` is ignored when this is true) — see FluxVM's
        /// `fluxvm_network::netns`. Mirrors `fluxvm_core::model::
        /// NetworkSpec::Tap.netns`.
        #[serde(default)]
        netns: bool,
        /// Bridge-less attach. Mutually exclusive with `bridge` and `netns`.
        /// Absent means a bridged or netns tap. Mirrors
        /// `fluxvm_core::model::NetworkSpec::Tap.direct`.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        direct: Option<DirectSpec>,
    },
    Macvtap {
        parent: String,
        #[serde(default)]
        macvtap_mode: Option<String>,
        #[serde(default)]
        mac: Option<String>,
    },
}

impl Default for NetworkSpec {
    fn default() -> Self {
        Self::User { forwards: vec![] }
    }
}

/// How a bridge-less tap is paired with its outer device.
/// Mirrors `fluxvm_core::model::DirectMode`.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "kebab-case")]
pub enum DirectMode {
    /// Outer device is a veth (a CNI pod's `eth0`).
    #[default]
    PeerVeth,
    /// Outer device is an unenslaved physical or bond NIC. Frames are steered
    /// by destination MAC, and ARP requests by `guest_ips`.
    L2Uplink,
}

/// Bridge-less attach of a VM tap to an outer device.
/// Mirrors `fluxvm_core::model::DirectSpec`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DirectSpec {
    /// Interface the redirect pairs with the tap.
    pub outer: String,
    /// Network namespace holding both devices. `None` is the host namespace.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub netns_path: Option<String>,
    #[serde(default)]
    pub mode: DirectMode,
    /// IPv4 addresses for `l2-uplink` ARP steering. At most 8.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub guest_ips: Vec<String>,
}

/// `POST /v1/vms/{id}/hotplug/nic`. Exactly one of `bridge` or `direct`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct HotplugNicRequest {
    /// Host bridge for a bridged NIC. Empty when `direct` is set.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub bridge: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mac: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub direct: Option<DirectSpec>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PortForward {
    pub host_port: u16,
    pub guest_port: u16,
    #[serde(default = "default_tcp")]
    pub protocol: String,
}
fn default_tcp() -> String {
    "tcp".into()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentSpec {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default = "default_agent_port")]
    pub port: u32,
    /// Shared secret the guest agent requires on every request. Leave unset
    /// on a request with `enabled: true` and FluxVM generates one and
    /// burns it into the VM's disk before boot.
    #[serde(default)]
    pub token: Option<String>,
}
fn default_agent_port() -> u32 {
    17777
}

impl Default for AgentSpec {
    fn default() -> Self {
        Self {
            enabled: false,
            port: default_agent_port(),
            token: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct CloudInitSpec {
    #[serde(default)]
    pub hostname: Option<String>,
    #[serde(default)]
    pub user: Option<String>,
    #[serde(default)]
    pub ssh_authorized_keys: Vec<String>,
    #[serde(default)]
    pub packages: Vec<String>,
    #[serde(default)]
    pub runcmd: Vec<String>,
    /// Configure the guest's network address statically via cloud-init
    /// instead of DHCP -- only meaningful for `NetworkSpec::Tap { netns:
    /// true, .. }`. Mirrors `fluxvm_core::model::CloudInitSpec.
    /// static_network`.
    #[serde(default)]
    pub static_network: bool,
    #[serde(default)]
    pub write_files: Vec<CloudInitFile>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CloudInitFile {
    pub path: String,
    pub content: String,
    #[serde(default)]
    pub permissions: Option<String>,
}

/// Pluggable disk backend for FluxVM create (`CreateVmRequest.storage`).
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "kebab-case")]
pub enum StorageBackend {
    #[default]
    Default,
    LvmThin,
    Nbd,
    CephRbd,
}

/// QEMU Guest Agent enablement at create time.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct QgaSpec {
    #[serde(default)]
    pub enabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateVmRequest {
    pub name: String,
    pub backend: BackendKind,
    pub image: PathBuf,
    #[serde(default = "default_vcpus")]
    pub vcpus: u8,
    #[serde(default = "default_memory")]
    pub memory_mib: u64,
    #[serde(default)]
    pub max_vcpus: Option<u8>,
    #[serde(default)]
    pub max_memory_mib: Option<u64>,
    #[serde(default)]
    pub disk_size_gib: Option<u64>,
    #[serde(default)]
    pub kernel: Option<PathBuf>,
    #[serde(default)]
    pub initrd: Option<PathBuf>,
    #[serde(default)]
    pub firmware: Option<PathBuf>,
    #[serde(default)]
    pub kernel_args: Option<String>,
    #[serde(default)]
    pub loadvm_tag: Option<String>,
    #[serde(default)]
    pub network: NetworkSpec,
    #[serde(default)]
    pub cloud_init: Option<CloudInitSpec>,
    #[serde(default)]
    pub ttl_seconds: Option<u64>,
    #[serde(default)]
    pub extra_args: Vec<String>,
    #[serde(default)]
    pub agent: Option<AgentSpec>,
    #[serde(default)]
    pub qga: Option<QgaSpec>,
    #[serde(default)]
    pub hyperv: bool,
    #[serde(default)]
    pub storage: StorageBackend,
    #[serde(default)]
    pub shared_folders: Vec<SharedFolder>,
    #[serde(default)]
    pub numa_node: Option<u8>,
    #[serde(default)]
    pub cpuset: Option<String>,
    #[serde(default)]
    pub hugepages: Option<bool>,
    #[serde(default)]
    pub vfio_devices: Vec<String>,
    #[serde(default)]
    pub pod_uid: Option<String>,
    /// Server-forced for migration receivers; callers must leave false.
    #[serde(default)]
    pub migration_incoming: bool,
    /// First-class FluxVM tenant id (optional). Prefer this over stuffing
    /// `tenant=` into labels when talking to schema-aware FluxVM builds.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tenant: Option<String>,
}

/// A host directory shared into the guest via virtiofs, declared at create
/// time — see FluxVM's own `fluxvm_core::model::SharedFolder` doc
/// comment for why this replaces `machinectl bind`'s live mount instead of
/// having a live equivalent.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SharedFolder {
    pub host_path: PathBuf,
    pub guest_path: String,
    #[serde(default)]
    pub read_only: bool,
}

fn default_vcpus() -> u8 {
    2
}
fn default_memory() -> u64 {
    2048
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum VmStatus {
    Creating,
    Running,
    Paused,
    Stopped,
    Failed,
    /// Incoming QEMU migration receiver (not yet activated).
    Receiving,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VmRecord {
    pub id: Uuid,
    pub name: String,
    pub backend: BackendKind,
    pub status: VmStatus,
    pub pid: Option<u32>,
    pub created_at: DateTime<Utc>,
    pub expires_at: Option<DateTime<Utc>>,
    pub workspace: PathBuf,
    pub disk: PathBuf,
    pub seed_disk: Option<PathBuf>,
    pub tap_name: Option<String>,
    pub control_socket: Option<PathBuf>,
    pub log_path: PathBuf,
    pub error: Option<String>,
    pub request: CreateVmRequest,
    #[serde(default)]
    pub guest_cid: Option<u32>,
    /// cgroup v2 path the launched VMM process was migrated into, once
    /// `VmManager` has done so — `None` until the first successful launch
    /// completes cgroup setup, or if cgroup delegation failed for this VM.
    #[serde(default)]
    pub cgroup_path: Option<PathBuf>,
    /// Set for `NetworkSpec::Tap { netns: true, .. }` — the VM's private
    /// network namespace name. Mirrors `fluxvm_core::model::VmRecord.netns`.
    #[serde(default)]
    pub netns: Option<String>,
    /// The guest's DHCP-leased IP on its own private subnet, resolved by
    /// FluxVM on every read for `netns: true` VMs — `None` for every
    /// other networking mode, or until the guest completes a DHCP
    /// handshake. Mirrors `fluxvm_core::model::VmRecord.guest_ip`.
    #[serde(default)]
    pub guest_ip: Option<String>,
    #[serde(default)]
    pub jail_path: Option<PathBuf>,
    #[serde(default)]
    pub vsock_socket: Option<PathBuf>,
    #[serde(default)]
    pub qga_socket: Option<PathBuf>,
    #[serde(default)]
    pub lvm_lv: Option<PathBuf>,
    #[serde(default)]
    pub nbd_pid: Option<u32>,
    #[serde(default)]
    pub virtiofsd_pids: Vec<u32>,
    #[serde(default)]
    pub dhcp_leasefile: Option<PathBuf>,
}

/// `POST /v1/migration/receivers` — arm a prepared-target QEMU incoming.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MigrationReceiverRequest {
    pub spec: CreateVmRequest,
    pub disk_path: PathBuf,
    #[serde(default)]
    pub receiver_ttl_seconds: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MigrationReceiverInfo {
    pub id: Uuid,
    pub status: VmStatus,
    /// Local TCP port for `tcp:<host>:<port>` migration URI.
    pub port: u16,
    pub expires_at: DateTime<Utc>,
}

/// cgroup v2 resource-control settings to apply to a running VM. Mirrors
/// `fluxvm_core::model::ResourcePatch`.
#[derive(Debug, Clone, Default, Serialize)]
pub struct ResourcePatch {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cpu_quota_percent: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub memory_max_bytes: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub io_weight: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pids_max: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cpuset_cpus: Option<Vec<u32>>,
}

/// Mirrors `fluxvm_core::model::VmMetrics`.
#[derive(Debug, Clone, Deserialize)]
pub struct VmMetrics {
    pub cpu_usage_percent: f64,
    pub memory_usage_bytes: u64,
    pub disk_read_bytes: u64,
    pub disk_write_bytes: u64,
}

/// Mirrors `fluxvm_cgroup::PressureRecord`.
#[derive(Debug, Clone, Deserialize)]
pub struct PressureRecord {
    pub avg10: f64,
    pub avg60: f64,
    pub avg300: f64,
    pub total: u64,
}

/// Mirrors `fluxvm_core::model::VmPressure`.
#[derive(Debug, Clone, Deserialize)]
pub struct VmPressure {
    pub cpu_some: Option<PressureRecord>,
    pub memory_some: Option<PressureRecord>,
    pub memory_full: Option<PressureRecord>,
    pub io_some: Option<PressureRecord>,
    pub io_full: Option<PressureRecord>,
}

#[derive(Debug, Deserialize)]
struct VmListResponse {
    items: Vec<VmRecord>,
}

// ZYVOR_RUNTIME_BOUNDARY_V1: typed mirror of FluxVM's node-local migration contract.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "kebab-case")]
pub enum MigrationMode {
    #[default]
    PreCopy,
    PostCopy,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct MigrationStartRequest {
    pub destination: String,
    #[serde(default)]
    pub mode: MigrationMode,
    #[serde(default)]
    pub bandwidth_mbps: Option<u64>,
    #[serde(default)]
    pub max_downtime_ms: Option<u64>,
    #[serde(default)]
    pub multifd_channels: Option<u8>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum MigrationPhase {
    None,
    Setup,
    Active,
    PostcopyActive,
    Completed,
    Failed,
    Cancelled,
    Unknown,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct MigrationStatus {
    pub phase: MigrationPhase,
    pub status: String,
    #[serde(default)]
    pub ram_transferred: Option<u64>,
    #[serde(default)]
    pub ram_remaining: Option<u64>,
    #[serde(default)]
    pub ram_total: Option<u64>,
    #[serde(default)]
    pub total_time_ms: Option<u64>,
    #[serde(default)]
    pub downtime_ms: Option<u64>,
    #[serde(default)]
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeMigrationCapability {
    pub backend: BackendKind,
    pub live: bool,
    pub pre_copy: bool,
    pub post_copy: bool,
    pub multifd: bool,
    pub requires_shared_storage: bool,
    pub transports: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeSnapshotCapability {
    pub backend: BackendKind,
    pub memory: bool,
    pub disk: bool,
    pub portable: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeCapabilities {
    pub api_version: String,
    pub scope: String,
    pub orchestration_owner: String,
    pub migration: Vec<RuntimeMigrationCapability>,
    pub snapshot: Vec<RuntimeSnapshotCapability>,
}

/// A warm pool: `size` VMs pre-booted from `template`, then paused, ready
/// to be handed out instantly by `claim_pool` instead of cold-created.
/// Mirrors `fluxvm_core::model::PoolRecord`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PoolRecord {
    pub name: String,
    pub size: usize,
    pub template: CreateVmRequest,
    #[serde(default)]
    pub members: Vec<Uuid>,
}

#[derive(Debug, Serialize)]
struct PoolSpecRequest {
    name: String,
    size: usize,
    template: CreateVmRequest,
}

#[derive(Debug, Deserialize)]
struct PoolListResponse {
    items: Vec<PoolRecord>,
}

// ============================================================================
// Network Fabric schema v4 wire types (mirror fluxvm-network::dataplane)
// ============================================================================

/// Per-VM edge policy for FluxVM's TC/eBPF Network Fabric dataplane.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct VmNetworkPolicy {
    pub default_allow: bool,
    pub allow_cidrs: Vec<String>,
    pub allow_ports: Vec<String>,
    pub max_egress_mbps: Option<u32>,
    pub max_egress_pps: Option<u32>,
    pub sample_rate: u32,
    #[serde(default)]
    pub deny_cidrs: Vec<String>,
    #[serde(default)]
    pub allow_icmp: bool,
    #[serde(default)]
    pub groups: Vec<String>,
    #[serde(default)]
    pub labels: Vec<String>,
    #[serde(default)]
    pub allow_fqdns: Vec<String>,
    #[serde(default)]
    pub entities: Vec<String>,
    #[serde(default)]
    pub audit_mode: bool,
    /// Keep / strict: drop UDP and SCTP (except DHCP) at the host TC program.
    #[serde(default)]
    pub deny_udp: bool,
}

impl Default for VmNetworkPolicy {
    fn default() -> Self {
        Self {
            default_allow: true,
            allow_cidrs: Vec::new(),
            allow_ports: Vec::new(),
            max_egress_mbps: None,
            max_egress_pps: None,
            sample_rate: 0,
            deny_cidrs: Vec::new(),
            allow_icmp: false,
            groups: Vec::new(),
            labels: Vec::new(),
            allow_fqdns: Vec::new(),
            entities: Vec::new(),
            audit_mode: false,
            deny_udp: false,
        }
    }
}

/// Live dataplane attach + schema status for one VM.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DataplaneStatus {
    pub mode: String,
    pub required: bool,
    pub attached: bool,
    pub interface: Option<String>,
    pub identity: u32,
    pub pin_dir: Option<String>,
    pub schema_version: Option<u32>,
    pub schema_compatible: bool,
    pub policy_synced: bool,
    #[serde(default)]
    pub pod_ingress_required: bool,
    #[serde(default)]
    pub pod_ingress_attached: bool,
    pub policy: VmNetworkPolicy,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct PodDirectionCounters {
    pub allowed: u64,
    pub dropped: u64,
    pub audited: u64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct PodPolicyStats {
    pub egress: PodDirectionCounters,
    pub ingress: PodDirectionCounters,
    pub directional: bool,
}

/// Allow/drop counters from the attached eBPF program.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct DataplaneStats {
    pub allowed_packets: u64,
    pub allowed_bytes: u64,
    pub dropped_packets: u64,
    pub dropped_bytes: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pod_policy: Option<PodPolicyStats>,
}

/// One sampled flow from the dataplane flow exporter.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct FlowRecord {
    pub identity: u32,
    pub family: u8,
    pub source: String,
    pub destination: String,
    pub source_port: u16,
    pub destination_port: u16,
    pub protocol: u8,
    pub verdict: String,
    pub packets: u64,
    pub bytes: u64,
    pub last_seen_ns: u64,
}

#[derive(Debug, Deserialize)]
struct FlowListResponse {
    items: Vec<FlowRecord>,
}

/// Drop-reason histogram row from `GET …/network/drop-reasons`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DropReasonRecord {
    pub identity: u32,
    pub family: u8,
    pub source: String,
    pub destination: String,
    pub source_port: u16,
    pub destination_port: u16,
    pub protocol: u8,
    pub reason_code: u32,
    pub reason: String,
    pub action: String,
    pub packets: u64,
    pub bytes: u64,
    pub last_seen_ns: u64,
}

#[derive(Debug, Deserialize)]
struct DropReasonListResponse {
    items: Vec<DropReasonRecord>,
}

/// Secure Containers / NetworkPolicy v2 pod edge policy.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum PodPolicyProtocol {
    Tcp,
    Udp,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PodPeerPortRule {
    pub address: IpAddr,
    pub protocol: PodPolicyProtocol,
    pub port: u16,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PodPolicyRule {
    pub direction: String,
    pub cidr: String,
    #[serde(default)]
    pub protocol: String,
    #[serde(default)]
    pub port_start: u16,
    #[serde(default)]
    pub port_end: u16,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct PodNetworkPolicy {
    #[serde(default)]
    pub schema_version: u32,
    #[serde(default)]
    pub default_deny: bool,
    #[serde(default)]
    pub audit_mode: bool,
    #[serde(default)]
    pub allow_addresses: Vec<IpAddr>,
    #[serde(default)]
    pub deny_addresses: Vec<IpAddr>,
    #[serde(default)]
    pub allow_port_rules: Vec<PodPeerPortRule>,
    #[serde(default)]
    pub egress_isolated: bool,
    #[serde(default)]
    pub ingress_isolated: bool,
    #[serde(default)]
    pub rules: Vec<PodPolicyRule>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum DataplaneMigrationPhase {
    Running,
    Quiescing,
    Restoring,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RawMapEntry {
    pub key: Vec<u8>,
    pub value: Vec<u8>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct VmNetworkStateSnapshot {
    pub schema_version: u32,
    pub vm_id: Uuid,
    pub identity: u32,
    pub dataplane_schema_version: u32,
    pub exported_at_unix_ms: u64,
    #[serde(default)]
    pub policy_fingerprint: Option<u64>,
    pub conntrack: Vec<RawMapEntry>,
    #[serde(default)]
    pub flows: Vec<RawMapEntry>,
    #[serde(default)]
    pub drop_reasons: Vec<RawMapEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct MigrationStateStatus {
    pub vm_id: Uuid,
    pub identity: u32,
    pub phase: DataplaneMigrationPhase,
    pub generation: u32,
    #[serde(default)]
    pub dataplane_schema_version: Option<u32>,
    pub schema_compatible: bool,
}

#[derive(Debug, Serialize)]
pub struct QgaExecRequest {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub args: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub powershell: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timeout_seconds: Option<u64>,
}

#[derive(Debug, Serialize)]
pub struct QgaFirewallOpenRequest {
    pub name: String,
    pub port: u16,
    #[serde(default = "default_tcp")]
    pub protocol: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timeout_seconds: Option<u64>,
}

#[derive(Debug, Serialize)]
pub struct QgaFirewallCloseRequest {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timeout_seconds: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct QgaExecResult {
    pub exit_code: i64,
    pub stdout: String,
    pub stderr: String,
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct OkResponse {
    ok: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SecurityGroup {
    pub name: String,
    #[serde(default)]
    pub labels: Vec<String>,
    #[serde(default)]
    pub policy: VmNetworkPolicy,
    #[serde(default)]
    pub identity: u32,
    #[serde(default)]
    pub priority: u32,
    #[serde(default)]
    pub description: String,
}

#[derive(Debug, Deserialize)]
struct GroupListResponse {
    items: Vec<SecurityGroup>,
}

/// Maglev/eBPF Service Fabric VIP (FluxVM `/v1/network/services`).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum NetworkServiceProtocol {
    Tcp,
    Udp,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "lowercase")]
pub enum NetworkServiceAlgorithm {
    #[default]
    Maglev,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "lowercase")]
pub enum NetworkServiceMode {
    #[default]
    Nat,
    Dsr,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "kebab-case")]
pub enum NetworkServiceExposure {
    #[default]
    EastWest,
    NorthSouth,
    Both,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct NetworkServiceBackend {
    pub address: String,
    pub port: u16,
    #[serde(default = "default_service_weight")]
    pub weight: u16,
    #[serde(default = "default_service_enabled")]
    pub enabled: bool,
    #[serde(default)]
    pub state: NetworkBackendState,
    #[serde(default)]
    pub drain_until_unix_ms: Option<u64>,
}

fn default_service_weight() -> u16 {
    1
}
fn default_service_enabled() -> bool {
    true
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "lowercase")]
pub enum NetworkBackendState {
    #[default]
    Ready,
    Draining,
    Unhealthy,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum NetworkHealthCheckKind {
    Tcp,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct NetworkServiceHealthCheck {
    #[serde(default = "default_client_health_kind")]
    pub kind: NetworkHealthCheckKind,
    #[serde(default = "default_client_health_timeout")]
    pub timeout_ms: u64,
    #[serde(default = "default_client_unhealthy")]
    pub unhealthy_threshold: u32,
    #[serde(default = "default_client_healthy")]
    pub healthy_threshold: u32,
}
fn default_client_health_kind() -> NetworkHealthCheckKind {
    NetworkHealthCheckKind::Tcp
}
fn default_client_health_timeout() -> u64 {
    500
}
fn default_client_unhealthy() -> u32 {
    3
}
fn default_client_healthy() -> u32 {
    2
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct NetworkServiceSpec {
    pub name: String,
    pub vip: String,
    pub port: u16,
    pub protocol: NetworkServiceProtocol,
    #[serde(default)]
    pub algorithm: NetworkServiceAlgorithm,
    #[serde(default)]
    pub mode: NetworkServiceMode,
    #[serde(default)]
    pub exposure: NetworkServiceExposure,
    #[serde(default)]
    pub backends: Vec<NetworkServiceBackend>,
    #[serde(default)]
    pub maglev_table_size: Option<u32>,
    #[serde(default)]
    pub snat_address: Option<String>,
    #[serde(default)]
    pub health_check: Option<NetworkServiceHealthCheck>,
    #[serde(default)]
    pub advertise: bool,
    #[serde(default)]
    pub max_egress_mbps: Option<u32>,
    #[serde(default)]
    pub flow_sample_rate: u32,
    #[serde(default)]
    pub host_routing: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct NetworkServiceStatus {
    pub schema_version: u32,
    pub service_id: u32,
    pub name: String,
    pub active_backends: usize,
    pub maglev_table_size: u32,
    #[serde(default)]
    pub family: Option<String>,
    #[serde(default)]
    pub mode: Option<NetworkServiceMode>,
    #[serde(default)]
    pub exposure: Option<NetworkServiceExposure>,
    #[serde(default)]
    pub snat_address: Option<String>,
}

#[derive(Debug, Deserialize)]
struct NetworkServiceListResponse {
    items: Vec<NetworkServiceSpec>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DataplaneHealth {
    pub mode: String,
    pub required: bool,
    pub default_allow: bool,
    pub bpf_object_present: bool,
    pub pin_root_present: bool,
    pub bpffs_present: bool,
    pub cilium_socket_present: bool,
    pub groups: usize,
    pub policies: usize,
    pub ipcache_entries: usize,
    pub ok: bool,
    #[serde(default)]
    pub notes: Vec<String>,
}

/// Typed `GET /readyz` body (optional fields may be absent).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ReadyzResponse {
    pub ok: bool,
    #[serde(default)]
    pub kvm: Option<bool>,
    #[serde(default)]
    pub state_dir: Option<PathBuf>,
    #[serde(default)]
    pub dataplane: Option<DataplaneHealth>,
    #[serde(default)]
    pub secure_containers: Option<SecureContainersStatus>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SecureContainersStatus {
    pub available: bool,
    pub shim_installed: bool,
    pub guest_image_present: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct IpcacheEntry {
    pub ip: String,
    pub identity: u32,
    pub vm_id: Uuid,
}

#[derive(Debug, Deserialize)]
struct IpcacheListResponse {
    items: Vec<IpcacheEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct IdentityInfo {
    pub id: u32,
    pub name: String,
    #[serde(default)]
    pub labels: Vec<String>,
    #[serde(default)]
    pub reserved: bool,
}

#[derive(Debug, Deserialize)]
struct IdentityListResponse {
    items: Vec<IdentityInfo>,
}

/// CiliumEndpoint-*shaped* VM edge view from FluxVM (not a real Cilium CEP CR).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CiliumEndpointView {
    pub id: u32,
    pub uuid: Uuid,
    pub identity: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub identity_source: Option<String>,
    #[serde(rename = "identity-labels", default)]
    pub identity_labels: Vec<String>,
    pub networking: serde_json::Value,
    pub state: String,
    pub policy: serde_json::Value,
}

#[derive(Debug, Deserialize)]
struct EndpointListResponse {
    items: Vec<CiliumEndpointView>,
}

#[derive(Debug, Deserialize)]
struct CnpListResponse {
    items: Vec<serde_json::Value>,
}

#[derive(Debug, Deserialize)]
struct RefreshDnsResponse {
    refreshed: usize,
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct DeletedResponse {
    deleted: String,
}

// ============================================================================
// Host GPU inventory (mirror fluxvm_core::gpu)
// ============================================================================

/// One PCI function that looks like a GPU (VGA or 3D controller).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct HostGpu {
    pub bdf: String,
    pub vendor_id: u16,
    pub device_id: u16,
    pub vendor: String,
    pub class_id: u32,
    pub driver: Option<String>,
    pub iommu_group: Option<u32>,
    pub iommu_members: Vec<String>,
    pub group_bound_to_vfio: bool,
    pub group_held: bool,
    pub numa_node: Option<u8>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub vram_gib: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub previous_driver: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GpuBindRequest {
    pub bdf: String,
    #[serde(default)]
    pub vram_gib: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GpuReleaseRequest {
    pub bdf: String,
    #[serde(default)]
    pub restore_driver: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GpuBindResult {
    pub bdf: String,
    pub iommu_group: u32,
    pub members: Vec<String>,
    pub previous_drivers: std::collections::BTreeMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GpuPreflight {
    pub iommu_enabled: bool,
    pub vfio_pci_loaded: bool,
    pub gpu_count: usize,
    pub issues: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct HostGpuListResponse {
    items: Vec<HostGpu>,
}

/// Applied to the VM handed back by a pool claim, replacing whatever the
/// template said for these two fields. Mirrors
/// `fluxvm_core::model::ClaimOverrides`.
#[derive(Debug, Clone, Serialize, Default)]
pub struct ClaimOverrides {
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub ttl_seconds: Option<u64>,
}

/// One entry in FluxVM's image catalog. Mirrors
/// `fluxvm_image::catalog::CatalogEntry` on the wire, plus
/// `signature_valid` which only `GET /v1/images/catalog`'s
/// `CatalogListEntry` wrapper adds (`None` when the client's own requests —
/// add/rename/clone/export — return a bare `CatalogEntry` with no
/// verification result attached).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CatalogEntry {
    pub name: String,
    pub source: String,
    pub sha256: String,
    pub format: String,
    #[serde(default)]
    pub distro: Option<String>,
    #[serde(default)]
    pub version: Option<String>,
    #[serde(default)]
    pub arch: Option<String>,
    #[serde(default)]
    pub signature: Option<String>,
    #[serde(default)]
    pub signature_valid: Option<bool>,
    #[serde(default)]
    pub read_only: bool,
}

#[derive(Debug, Deserialize)]
struct CatalogListResponse {
    items: Vec<CatalogEntry>,
}

#[derive(Debug, Serialize)]
struct AddCatalogEntryRequest {
    name: String,
    source: String,
    format: String,
}

#[derive(Debug, Serialize)]
struct RenameCatalogEntryRequest {
    new_name: String,
}

#[derive(Debug, Serialize)]
struct CloneCatalogEntryRequest {
    target_name: String,
}

#[derive(Debug, Serialize)]
struct ExportCatalogEntryRequest {
    path: PathBuf,
}

#[derive(Debug, Serialize)]
struct SetCatalogReadOnlyRequest {
    read_only: bool,
}

#[derive(Debug, Deserialize)]
struct CleanCatalogResponse {
    removed: Vec<String>,
}

#[derive(Debug, Serialize)]
struct ExecRequest {
    command: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    timeout_seconds: Option<u64>,
}

#[derive(Debug, Serialize)]
struct PutFileRequest {
    path: String,
    content_base64: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    mode: Option<u32>,
}

#[derive(Debug, Serialize)]
struct GetFileRequest {
    path: String,
}

/// Mirrors `fluxvm_guest_protocol::AgentResponse`. `Error` is an
/// agent/protocol-level failure (bad token, malformed request) — a command
/// that ran but exited non-zero is still `Exec` with that `exit_code`, not
/// this variant.
#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "result", rename_all = "kebab-case")]
pub enum AgentResponse {
    Pong,
    Exec {
        exit_code: i32,
        stdout: String,
        stderr: String,
    },
    FileWritten,
    FileContent {
        content_base64: String,
        mode: u32,
    },
    ShuttingDown,
    Error {
        message: String,
    },
}

// ============================================================================
// Client
// ============================================================================

/// Attach an `Idempotency-Key` header. FluxVM accepts 1-255 visible ASCII
/// characters and rejects anything else, so check here for a clearer error.
pub fn with_idempotency_key(
    builder: reqwest::RequestBuilder,
    key: &str,
) -> Result<reqwest::RequestBuilder> {
    validate_idempotency_key(key)?;
    Ok(builder.header("Idempotency-Key", key))
}

pub fn validate_idempotency_key(key: &str) -> Result<()> {
    if key.is_empty() || key.len() > 255 || !key.bytes().all(|b| (0x21..=0x7e).contains(&b)) {
        bail!("Idempotency-Key must be 1-255 visible ASCII characters");
    }
    Ok(())
}

/// A REST client for one `fluxvm serve` instance.
#[derive(Clone)]
pub struct FluxVmClient {
    base_url: reqwest::Url,
    http: reqwest::Client,
    /// Bearer token sent on every request once FluxVM's `auth.tokens` is
    /// non-empty. `None` is correct (and required) against a deployment
    /// that leaves auth disabled — there's nothing to send.
    token: Option<String>,
}

impl FluxVmClient {
    /// `base_url` is FluxVM's listen address, e.g. `http://127.0.0.1:7788`.
    pub fn new(base_url: impl AsRef<str>) -> Result<Self> {
        let base_url = reqwest::Url::parse(base_url.as_ref())
            .with_context(|| format!("invalid FluxVM base URL: {}", base_url.as_ref()))?;
        let http = reqwest::Client::builder()
            // Match fabricd: FluxVM create/start can clone large disks.
            .timeout(std::time::Duration::from_secs(180))
            .build()
            .context("failed to build FluxVM HTTP client")?;
        Ok(Self {
            base_url,
            http,
            token: None,
        })
    }

    /// Attach a bearer token, required once the target instance has
    /// `auth.tokens` configured (see `fluxvm_core::config::AuthConfig`).
    pub fn with_token(mut self, token: impl Into<String>) -> Self {
        self.token = Some(token.into());
        self
    }

    fn url(&self, path: &str) -> Result<reqwest::Url> {
        self.base_url
            .join(path)
            .with_context(|| format!("failed to build FluxVM URL for {path}"))
    }

    fn authed(&self, builder: reqwest::RequestBuilder) -> reqwest::RequestBuilder {
        match &self.token {
            Some(t) => builder.bearer_auth(t),
            None => builder,
        }
    }

    /// `GET /healthz` — used at startup and by capability probes. Unlike
    /// every other endpoint, `/healthz` is reachable without a bearer token
    /// even when auth is enabled (see `fluxvm_api::auth_middleware`), so
    /// this deliberately does not go through `authed()`.
    pub async fn healthy(&self) -> bool {
        matches!(
            self.http.get(self.url("/healthz").unwrap()).send().await,
            Ok(resp) if resp.status().is_success()
        )
    }

    /// `GET /readyz` — readiness (state dir + dataplane when required).
    /// Unauthenticated like `/healthz`.
    pub async fn readyz(&self) -> Result<serde_json::Value> {
        let resp = self.http.get(self.url("/readyz")?).send().await?;
        Self::parse(resp).await
    }

    /// Typed `GET /readyz` (same path as [`Self::readyz`]).
    pub async fn readyz_typed(&self) -> Result<ReadyzResponse> {
        let resp = self.http.get(self.url("/readyz")?).send().await?;
        Self::parse(resp).await
    }

    /// `GET /readyz` boolean convenience for probes.
    pub async fn ready(&self) -> bool {
        match self.readyz().await {
            Ok(v) => v.get("ok").and_then(|x| x.as_bool()).unwrap_or(false),
            Err(_) => false,
        }
    }

    /// FluxVM's stable node-local feature contract. Fabric uses this instead
    /// of assuming that every runtime/version supports every operation.
    pub async fn runtime_capabilities(&self) -> Result<RuntimeCapabilities> {
        let resp = self
            .authed(self.http.get(self.url("/v1/runtime/capabilities")?))
            .send()
            .await?;
        Self::parse(resp).await
    }

    pub async fn start_migration(
        &self,
        id: Uuid,
        request: &MigrationStartRequest,
    ) -> Result<MigrationStatus> {
        let resp = self
            .authed(
                self.http
                    .post(self.url(&format!("/v1/vms/{id}/migration/start"))?),
            )
            .json(request)
            .send()
            .await?;
        Self::parse(resp).await
    }

    pub async fn migration_status(&self, id: Uuid) -> Result<MigrationStatus> {
        let resp = self
            .authed(
                self.http
                    .get(self.url(&format!("/v1/vms/{id}/migration/status"))?),
            )
            .send()
            .await?;
        Self::parse(resp).await
    }

    pub async fn cancel_migration(&self, id: Uuid) -> Result<MigrationStatus> {
        let resp = self
            .authed(
                self.http
                    .post(self.url(&format!("/v1/vms/{id}/migration/cancel"))?),
            )
            .send()
            .await?;
        Self::parse(resp).await
    }

    /// `POST /v1/migration/receivers` — prepare an incoming QEMU target.
    pub async fn create_migration_receiver(
        &self,
        req: &MigrationReceiverRequest,
    ) -> Result<MigrationReceiverInfo> {
        let resp = self
            .authed(self.http.post(self.url("/v1/migration/receivers")?))
            .json(req)
            .send()
            .await?;
        Self::parse(resp).await
    }

    /// `GET /v1/migration/receivers/{id}`
    pub async fn get_migration_receiver(&self, id: Uuid) -> Result<MigrationReceiverInfo> {
        let resp = self
            .authed(
                self.http
                    .get(self.url(&format!("/v1/migration/receivers/{id}"))?),
            )
            .send()
            .await?;
        Self::parse(resp).await
    }

    /// `DELETE /v1/migration/receivers/{id}`
    pub async fn abort_migration_receiver(&self, id: Uuid) -> Result<()> {
        let resp = self
            .authed(
                self.http
                    .delete(self.url(&format!("/v1/migration/receivers/{id}"))?),
            )
            .send()
            .await?;
        Self::expect_no_content(resp).await
    }

    /// `POST /v1/migration/receivers/{id}/activate`
    pub async fn activate_migration_receiver(&self, id: Uuid) -> Result<VmRecord> {
        let resp = self
            .authed(
                self.http
                    .post(self.url(&format!("/v1/migration/receivers/{id}/activate"))?),
            )
            .send()
            .await?;
        Self::parse(resp).await
    }

    /// `POST /v1/vms/{id}/hotplug/nic` — QEMU only. A `direct` body is required
    /// for a bridge-less tap: FluxVM creates the tap, attaches the dataplane,
    /// and hands the fd to QEMU. Bridged hotplug may still use QMP.
    pub async fn hotplug_nic(
        &self,
        id: Uuid,
        req: &HotplugNicRequest,
    ) -> Result<serde_json::Value> {
        let resp = self
            .authed(
                self.http
                    .post(self.url(&format!("/v1/vms/{id}/hotplug/nic"))?),
            )
            .json(req)
            .send()
            .await?;
        Self::parse(resp).await
    }

    pub async fn create_vm(&self, req: &CreateVmRequest) -> Result<VmRecord> {
        let resp = self
            .authed(self.http.post(self.url("/v1/vms")?))
            .json(req)
            .send()
            .await?;
        Self::parse(resp).await
    }

    /// `POST /v1/vms` with an `Idempotency-Key`: a retry with the same key
    /// replays FluxVM's first answer instead of creating a second VM.
    pub async fn create_vm_idempotent(&self, req: &CreateVmRequest, key: &str) -> Result<VmRecord> {
        let resp = self
            .authed(with_idempotency_key(
                self.http.post(self.url("/v1/vms")?),
                key,
            )?)
            .json(req)
            .send()
            .await?;
        Self::parse(resp).await
    }

    pub async fn list_vms(&self) -> Result<Vec<VmRecord>> {
        let resp = self
            .authed(self.http.get(self.url("/v1/vms")?))
            .send()
            .await?;
        let body: VmListResponse = Self::parse(resp).await?;
        Ok(body.items)
    }

    /// `GET /v1/vms?tenant=` — server-side tenant filter.
    pub async fn list_vms_by_tenant(&self, tenant: &str) -> Result<Vec<VmRecord>> {
        let mut url = self.url("/v1/vms")?;
        url.query_pairs_mut().append_pair("tenant", tenant);
        let resp = self.authed(self.http.get(url)).send().await?;
        let body: VmListResponse = Self::parse(resp).await?;
        Ok(body.items)
    }

    /// Find a VM by exact name via the server-side `?name=` filter — needed
    /// because `driver-core`'s `VMDriver` trait is keyed by name
    /// (systemd-machined's model) while `VmRecord` is keyed by `Uuid`.
    pub async fn find_by_name(&self, name: &str) -> Result<Option<VmRecord>> {
        let mut url = self.url("/v1/vms")?;
        url.query_pairs_mut().append_pair("name", name);
        let resp = self.authed(self.http.get(url)).send().await?;
        let body: VmListResponse = Self::parse(resp).await?;
        Ok(body.items.into_iter().next())
    }

    pub async fn get_vm(&self, id: Uuid) -> Result<VmRecord> {
        let resp = self
            .authed(self.http.get(self.url(&format!("/v1/vms/{id}"))?))
            .send()
            .await?;
        Self::parse(resp).await
    }

    /// `POST /v1/vms/{id}/start` — relaunch a `Stopped` VM from its existing
    /// disk (image cloning/cloud-init reseed are skipped server-side).
    /// Idempotent: a VM already `Running` is returned unchanged.
    pub async fn start_vm(&self, id: Uuid) -> Result<VmRecord> {
        let resp = self
            .authed(self.http.post(self.url(&format!("/v1/vms/{id}/start"))?))
            .send()
            .await?;
        Self::parse(resp).await
    }

    pub async fn stop_vm(&self, id: Uuid) -> Result<VmRecord> {
        let resp = self
            .authed(self.http.post(self.url(&format!("/v1/vms/{id}/stop"))?))
            .send()
            .await?;
        Self::parse(resp).await
    }

    /// `POST /v1/vms/{id}/start-from-snapshot` — like `start_vm`, but
    /// restores CPU/memory/device state from an existing internal
    /// (`snapshot-save`) tag on the VM's own disk via QEMU's `-loadvm`,
    /// instead of an ordinary cold boot. A one-shot override for this
    /// launch only.
    pub async fn start_vm_from_snapshot(&self, id: Uuid, tag: &str) -> Result<VmRecord> {
        let resp = self
            .authed(
                self.http
                    .post(self.url(&format!("/v1/vms/{id}/start-from-snapshot"))?),
            )
            .json(&serde_json::json!({"tag": tag}))
            .send()
            .await?;
        Self::parse(resp).await
    }

    pub async fn pause_vm(&self, id: Uuid) -> Result<VmRecord> {
        let resp = self
            .authed(self.http.post(self.url(&format!("/v1/vms/{id}/pause"))?))
            .send()
            .await?;
        Self::parse(resp).await
    }

    pub async fn resume_vm(&self, id: Uuid) -> Result<VmRecord> {
        let resp = self
            .authed(self.http.post(self.url(&format!("/v1/vms/{id}/resume"))?))
            .send()
            .await?;
        Self::parse(resp).await
    }

    /// `DELETE /v1/vms/{id}` with an `Idempotency-Key`, so a retried delete
    /// replays the first outcome.
    pub async fn delete_vm_idempotent(&self, id: Uuid, key: &str) -> Result<()> {
        let resp = self
            .authed(with_idempotency_key(
                self.http.delete(self.url(&format!("/v1/vms/{id}"))?),
                key,
            )?)
            .send()
            .await?;
        if resp.status().is_success() {
            Ok(())
        } else {
            bail!("DELETE /v1/vms/{id} failed: {}", resp.status())
        }
    }

    pub async fn delete_vm(&self, id: Uuid) -> Result<()> {
        let resp = self
            .authed(self.http.delete(self.url(&format!("/v1/vms/{id}"))?))
            .send()
            .await?;
        if resp.status().is_success() {
            Ok(())
        } else {
            bail!("DELETE /v1/vms/{id} failed: {}", resp.status())
        }
    }

    /// `POST /v1/vms/{id}/agent` — exec a command over the in-guest vsock
    /// agent (requires `CreateVmRequest.agent.enabled`; FluxVM itself
    /// returns a clear error for a VM that doesn't have it, rather than a
    /// silent hang).
    pub async fn agent_exec(
        &self,
        id: Uuid,
        command: impl Into<String>,
        timeout_seconds: Option<u64>,
    ) -> Result<AgentResponse> {
        let resp = self
            .authed(self.http.post(self.url(&format!("/v1/vms/{id}/agent"))?))
            .json(&ExecRequest {
                command: command.into(),
                timeout_seconds,
            })
            .send()
            .await?;
        Self::parse(resp).await
    }

    /// `POST /v1/vms/{id}/agent/put-file`
    pub async fn agent_put_file(
        &self,
        id: Uuid,
        path: &str,
        content_base64: &str,
        mode: Option<u32>,
    ) -> Result<AgentResponse> {
        let resp = self
            .authed(
                self.http
                    .post(self.url(&format!("/v1/vms/{id}/agent/put-file"))?),
            )
            .json(&PutFileRequest {
                path: path.to_string(),
                content_base64: content_base64.to_string(),
                mode,
            })
            .send()
            .await?;
        Self::parse(resp).await
    }

    /// `POST /v1/vms/{id}/agent/get-file`
    pub async fn agent_get_file(&self, id: Uuid, path: &str) -> Result<AgentResponse> {
        let resp = self
            .authed(
                self.http
                    .post(self.url(&format!("/v1/vms/{id}/agent/get-file"))?),
            )
            .json(&GetFileRequest {
                path: path.to_string(),
            })
            .send()
            .await?;
        Self::parse(resp).await
    }

    /// `GET /v1/vms/{id}/console?cols=..&rows=..` — dials FluxVM's
    /// interactive-console WebSocket and returns a raw byte stream: reads
    /// yield whatever the guest's shell wrote, writes go straight to its
    /// stdin, with no framing on this side either (WS binary frames only,
    /// unwrapped transparently by [`ConsoleWs`]).
    pub async fn open_console(&self, id: Uuid, cols: u16, rows: u16) -> Result<ConsoleWs> {
        let mut ws_url = self.url(&format!("/v1/vms/{id}/console"))?;
        ws_url
            .set_scheme(if self.base_url.scheme() == "https" {
                "wss"
            } else {
                "ws"
            })
            .map_err(|_| {
                anyhow::anyhow!("failed to convert FluxVM base URL to a ws(s):// scheme")
            })?;
        ws_url
            .query_pairs_mut()
            .append_pair("cols", &cols.to_string())
            .append_pair("rows", &rows.to_string());

        let mut request = tokio_tungstenite::tungstenite::http::Request::builder()
            .uri(ws_url.as_str())
            .header("Host", ws_url.host_str().unwrap_or_default())
            .header("Connection", "Upgrade")
            .header("Upgrade", "websocket")
            .header("Sec-WebSocket-Version", "13")
            .header(
                "Sec-WebSocket-Key",
                tokio_tungstenite::tungstenite::handshake::client::generate_key(),
            );
        if let Some(token) = &self.token {
            request = request.header("Authorization", format!("Bearer {token}"));
        }
        let request = request
            .body(())
            .context("building console WebSocket request")?;

        let (stream, _response) = tokio_tungstenite::connect_async(request)
            .await
            .map_err(|e| {
                // tungstenite's `Error::Http` Display only prints the status
                // line ("HTTP error: 400 Bad Request") -- the actual reason
                // (e.g. FluxVM's own `{"error": "connect(vsock ...): ..."}`
                // body) is right there in the response but silently dropped
                // unless pulled out explicitly. Without this, every console
                // failure looked identical to the browser regardless of cause.
                if let tokio_tungstenite::tungstenite::Error::Http(resp) = &e {
                    if let Some(body) = resp.body() {
                        let detail = String::from_utf8_lossy(body);
                        let detail = serde_json::from_str::<serde_json::Value>(&detail)
                            .ok()
                            .and_then(|v| {
                                v.get("error").and_then(|e| e.as_str()).map(str::to_owned)
                            })
                            .unwrap_or_else(|| detail.into_owned());
                        return anyhow::anyhow!(
                            "connecting to console WebSocket for VM {id}: {} {}: {detail}",
                            resp.status().as_u16(),
                            resp.status().canonical_reason().unwrap_or(""),
                        );
                    }
                }
                anyhow::Error::new(e)
                    .context(format!("connecting to console WebSocket for VM {id}"))
            })?;
        Ok(ConsoleWs {
            stream,
            read_buf: Vec::new(),
        })
    }

    /// `POST /v1/vms/{id}/resources` — apply a partial cgroup resource
    /// patch to a running VM. Only fields set on `patch` are changed.
    pub async fn set_resources(&self, id: Uuid, patch: &ResourcePatch) -> Result<()> {
        let resp = self
            .authed(
                self.http
                    .post(self.url(&format!("/v1/vms/{id}/resources"))?),
            )
            .json(patch)
            .send()
            .await?;
        Self::expect_no_content(resp).await
    }

    /// `GET /v1/vms/{id}/cpuset`
    pub async fn get_cpuset(&self, id: Uuid) -> Result<Vec<u32>> {
        let resp = self
            .authed(self.http.get(self.url(&format!("/v1/vms/{id}/cpuset"))?))
            .send()
            .await?;
        #[derive(Deserialize)]
        struct CpusetResponse {
            cpus: Vec<u32>,
        }
        let body: CpusetResponse = Self::parse(resp).await?;
        Ok(body.cpus)
    }

    /// `POST /v1/vms/{id}/freeze` — suspend the VM's cgroup via the v2
    /// freezer (`cgroup.freeze`), independent of guest-level pause/resume.
    pub async fn freeze(&self, id: Uuid) -> Result<()> {
        let resp = self
            .authed(self.http.post(self.url(&format!("/v1/vms/{id}/freeze"))?))
            .send()
            .await?;
        Self::expect_no_content(resp).await
    }

    pub async fn thaw(&self, id: Uuid) -> Result<()> {
        let resp = self
            .authed(self.http.post(self.url(&format!("/v1/vms/{id}/thaw"))?))
            .send()
            .await?;
        Self::expect_no_content(resp).await
    }

    pub async fn is_frozen(&self, id: Uuid) -> Result<bool> {
        let resp = self
            .authed(self.http.get(self.url(&format!("/v1/vms/{id}/frozen"))?))
            .send()
            .await?;
        let body: serde_json::Value = Self::parse(resp).await?;
        Ok(body
            .get("frozen")
            .and_then(|v| v.as_bool())
            .unwrap_or(false))
    }

    /// `GET /v1/vms/{id}/stats` — point-in-time cgroup usage.
    pub async fn stats(&self, id: Uuid) -> Result<VmMetrics> {
        let resp = self
            .authed(self.http.get(self.url(&format!("/v1/vms/{id}/stats"))?))
            .send()
            .await?;
        Self::parse(resp).await
    }

    /// `GET /v1/vms/{id}/pressure` — PSI (cpu/memory/io) for the VM's cgroup.
    pub async fn pressure(&self, id: Uuid) -> Result<VmPressure> {
        let resp = self
            .authed(self.http.get(self.url(&format!("/v1/vms/{id}/pressure"))?))
            .send()
            .await?;
        Self::parse(resp).await
    }

    /// `GET /v1/vms/{id}/logs?lines=N&follow=true` — tail the VM's captured
    /// console output, one line per stream item. `follow` streams
    /// indefinitely (until the caller drops the returned stream), so this
    /// overrides the client's default 30s request timeout in that case.
    pub async fn stream_logs(
        &self,
        id: Uuid,
        lines: u32,
        follow: bool,
    ) -> Result<impl Stream<Item = Result<String>>> {
        let mut url = self.url(&format!("/v1/vms/{id}/logs"))?;
        url.query_pairs_mut()
            .append_pair("lines", &lines.to_string())
            .append_pair("follow", &follow.to_string());

        let mut builder = self.authed(self.http.get(url));
        if follow {
            builder = builder.timeout(std::time::Duration::from_secs(30 * 24 * 3600));
        }
        let resp = builder.send().await?;
        let status = resp.status();
        if !status.is_success() {
            let body = resp.text().await.unwrap_or_default();
            bail!("FluxVM request failed: {status} — {body}");
        }

        let byte_stream = resp.bytes_stream().map_err(std::io::Error::other);
        let reader = tokio_util::io::StreamReader::new(byte_stream);
        let mut lines_reader = reader.lines();

        Ok(async_stream::stream! {
            loop {
                match lines_reader.next_line().await {
                    Ok(Some(line)) => yield Ok(line),
                    Ok(None) => break,
                    Err(e) => {
                        yield Err(anyhow::anyhow!(e));
                        break;
                    }
                }
            }
        })
    }

    /// `GET /v1/images/catalog`
    pub async fn list_catalog(&self) -> Result<Vec<CatalogEntry>> {
        let resp = self
            .authed(self.http.get(self.url("/v1/images/catalog")?))
            .send()
            .await?;
        let body: CatalogListResponse = Self::parse(resp).await?;
        Ok(body.items)
    }

    /// `POST /v1/images/catalog`
    pub async fn add_catalog_entry(
        &self,
        name: &str,
        source: &str,
        format: &str,
    ) -> Result<CatalogEntry> {
        let req = AddCatalogEntryRequest {
            name: name.to_string(),
            source: source.to_string(),
            format: format.to_string(),
        };
        let resp = self
            .authed(self.http.post(self.url("/v1/images/catalog")?))
            .json(&req)
            .send()
            .await?;
        Self::parse(resp).await
    }

    /// `DELETE /v1/images/catalog/{name}`
    pub async fn remove_catalog_entry(&self, name: &str) -> Result<()> {
        let resp = self
            .authed(
                self.http
                    .delete(self.url(&format!("/v1/images/catalog/{name}"))?),
            )
            .send()
            .await?;
        Self::expect_no_content(resp).await
    }

    /// `POST /v1/images/catalog/{name}/rename`
    pub async fn rename_catalog_entry(&self, name: &str, new_name: &str) -> Result<CatalogEntry> {
        let req = RenameCatalogEntryRequest {
            new_name: new_name.to_string(),
        };
        let resp = self
            .authed(
                self.http
                    .post(self.url(&format!("/v1/images/catalog/{name}/rename"))?),
            )
            .json(&req)
            .send()
            .await?;
        Self::parse(resp).await
    }

    /// `POST /v1/images/catalog/{name}/clone`
    pub async fn clone_catalog_entry(&self, name: &str, target_name: &str) -> Result<CatalogEntry> {
        let req = CloneCatalogEntryRequest {
            target_name: target_name.to_string(),
        };
        let resp = self
            .authed(
                self.http
                    .post(self.url(&format!("/v1/images/catalog/{name}/clone"))?),
            )
            .json(&req)
            .send()
            .await?;
        Self::parse(resp).await
    }

    /// `POST /v1/images/catalog/{name}/export`
    pub async fn export_catalog_entry(&self, name: &str, path: &std::path::Path) -> Result<()> {
        let req = ExportCatalogEntryRequest {
            path: path.to_path_buf(),
        };
        let resp = self
            .authed(
                self.http
                    .post(self.url(&format!("/v1/images/catalog/{name}/export"))?),
            )
            .json(&req)
            .send()
            .await?;
        Self::expect_no_content(resp).await
    }

    /// `POST /v1/images/catalog/{name}/read-only`
    pub async fn set_catalog_read_only(&self, name: &str, read_only: bool) -> Result<CatalogEntry> {
        let req = SetCatalogReadOnlyRequest { read_only };
        let resp = self
            .authed(
                self.http
                    .post(self.url(&format!("/v1/images/catalog/{name}/read-only"))?),
            )
            .json(&req)
            .send()
            .await?;
        Self::parse(resp).await
    }

    /// `POST /v1/images/catalog/clean` — returns the filenames removed.
    pub async fn clean_catalog(&self) -> Result<Vec<String>> {
        let resp = self
            .authed(self.http.post(self.url("/v1/images/catalog/clean")?))
            .send()
            .await?;
        let body: CleanCatalogResponse = Self::parse(resp).await?;
        Ok(body.removed)
    }

    /// `POST /v1/pools` — pre-boots `size` VMs from `template`, then pauses
    /// each once ready. Members sit paused (booted, not cold) until
    /// claimed.
    pub async fn create_pool(
        &self,
        name: &str,
        size: usize,
        template: CreateVmRequest,
    ) -> Result<PoolRecord> {
        let req = PoolSpecRequest {
            name: name.to_string(),
            size,
            template,
        };
        let resp = self
            .authed(self.http.post(self.url("/v1/pools")?))
            .json(&req)
            .send()
            .await?;
        Self::parse(resp).await
    }

    /// `GET /v1/pools`
    pub async fn list_pools(&self) -> Result<Vec<PoolRecord>> {
        let resp = self
            .authed(self.http.get(self.url("/v1/pools")?))
            .send()
            .await?;
        let body: PoolListResponse = Self::parse(resp).await?;
        Ok(body.items)
    }

    /// `GET /v1/pools/{name}`
    pub async fn get_pool(&self, name: &str) -> Result<PoolRecord> {
        let resp = self
            .authed(self.http.get(self.url(&format!("/v1/pools/{name}"))?))
            .send()
            .await?;
        Self::parse(resp).await
    }

    /// `DELETE /v1/pools/{name}` — also tears down every member VM.
    pub async fn delete_pool(&self, name: &str) -> Result<()> {
        let resp = self
            .authed(self.http.delete(self.url(&format!("/v1/pools/{name}"))?))
            .send()
            .await?;
        Self::expect_no_content(resp).await
    }

    /// `POST /v1/pools/{name}/claim` — resumes one ready (already-booted,
    /// paused) member, applies `overrides`, and triggers a backfill to
    /// replace it. Fails if the pool has no ready member right now rather
    /// than falling back to a slow cold create.
    pub async fn claim_pool(&self, name: &str, overrides: ClaimOverrides) -> Result<VmRecord> {
        let resp = self
            .authed(
                self.http
                    .post(self.url(&format!("/v1/pools/{name}/claim"))?),
            )
            .json(&overrides)
            .send()
            .await?;
        Self::parse(resp).await
    }

    /// `GET /v1/vms/{id}/network/policy`
    pub async fn get_network_policy(&self, id: Uuid) -> Result<VmNetworkPolicy> {
        let resp = self
            .authed(
                self.http
                    .get(self.url(&format!("/v1/vms/{id}/network/policy"))?),
            )
            .send()
            .await?;
        Self::parse(resp).await
    }

    /// `POST /v1/vms/{id}/network/policy` — requires admin when FluxVM auth is on.
    pub async fn set_network_policy(
        &self,
        id: Uuid,
        policy: &VmNetworkPolicy,
    ) -> Result<VmNetworkPolicy> {
        let resp = self
            .authed(
                self.http
                    .post(self.url(&format!("/v1/vms/{id}/network/policy"))?),
            )
            .json(policy)
            .send()
            .await?;
        Self::parse(resp).await
    }

    /// `GET /v1/vms/{id}/network/status`
    pub async fn network_status(&self, id: Uuid) -> Result<DataplaneStatus> {
        let resp = self
            .authed(
                self.http
                    .get(self.url(&format!("/v1/vms/{id}/network/status"))?),
            )
            .send()
            .await?;
        Self::parse(resp).await
    }

    /// `GET /v1/vms/{id}/network/stats`
    pub async fn network_stats(&self, id: Uuid) -> Result<DataplaneStats> {
        let resp = self
            .authed(
                self.http
                    .get(self.url(&format!("/v1/vms/{id}/network/stats"))?),
            )
            .send()
            .await?;
        Self::parse(resp).await
    }

    /// `GET /v1/vms/{id}/network/flows?limit=` — FluxVM defaults `limit` to 100
    /// and clamps to `[1, 4096]`.
    pub async fn network_flows(&self, id: Uuid, limit: Option<usize>) -> Result<Vec<FlowRecord>> {
        let mut url = self.url(&format!("/v1/vms/{id}/network/flows"))?;
        if let Some(limit) = limit {
            url.query_pairs_mut()
                .append_pair("limit", &limit.to_string());
        }
        let resp = self.authed(self.http.get(url)).send().await?;
        let body: FlowListResponse = Self::parse(resp).await?;
        Ok(body.items)
    }

    /// `GET /v1/vms/{id}/network/effective`
    pub async fn network_effective(&self, id: Uuid) -> Result<serde_json::Value> {
        let resp = self
            .authed(
                self.http
                    .get(self.url(&format!("/v1/vms/{id}/network/effective"))?),
            )
            .send()
            .await?;
        Self::parse(resp).await
    }

    /// `GET /v1/vms/{id}/network/drop-reasons?limit=`
    pub async fn network_drop_reasons(
        &self,
        id: Uuid,
        limit: Option<usize>,
    ) -> Result<Vec<DropReasonRecord>> {
        let mut url = self.url(&format!("/v1/vms/{id}/network/drop-reasons"))?;
        if let Some(limit) = limit {
            url.query_pairs_mut()
                .append_pair("limit", &limit.to_string());
        }
        let resp = self.authed(self.http.get(url)).send().await?;
        let body: DropReasonListResponse = Self::parse(resp).await?;
        Ok(body.items)
    }

    /// `GET /v1/vms/{id}/network/pod-policy` — may be JSON `null`.
    pub async fn get_pod_network_policy(&self, id: Uuid) -> Result<Option<PodNetworkPolicy>> {
        let resp = self
            .authed(
                self.http
                    .get(self.url(&format!("/v1/vms/{id}/network/pod-policy"))?),
            )
            .send()
            .await?;
        let status = resp.status();
        let bytes = resp
            .bytes()
            .await
            .context("failed to read FluxVM response body")?;
        if !status.is_success() {
            let body = String::from_utf8_lossy(&bytes);
            bail!("FluxVM request failed: {status} — {body}");
        }
        if bytes.as_ref() == b"null" || bytes.is_empty() {
            return Ok(None);
        }
        serde_json::from_slice(&bytes)
            .map(Some)
            .with_context(|| format!("failed to parse FluxVM pod-policy ({status})"))
    }

    /// `POST /v1/vms/{id}/network/pod-policy`
    pub async fn set_pod_network_policy(&self, id: Uuid, policy: &PodNetworkPolicy) -> Result<()> {
        let resp = self
            .authed(
                self.http
                    .post(self.url(&format!("/v1/vms/{id}/network/pod-policy"))?),
            )
            .json(policy)
            .send()
            .await?;
        let _: OkResponse = Self::parse(resp).await?;
        Ok(())
    }

    /// `DELETE /v1/vms/{id}/network/pod-policy`
    pub async fn delete_pod_network_policy(&self, id: Uuid) -> Result<()> {
        let resp = self
            .authed(
                self.http
                    .delete(self.url(&format!("/v1/vms/{id}/network/pod-policy"))?),
            )
            .send()
            .await?;
        let _: OkResponse = Self::parse(resp).await?;
        Ok(())
    }

    /// `GET /v1/vms/{id}/network/migration/state`
    pub async fn network_migration_state(&self, id: Uuid) -> Result<MigrationStateStatus> {
        let resp = self
            .authed(
                self.http
                    .get(self.url(&format!("/v1/vms/{id}/network/migration/state"))?),
            )
            .send()
            .await?;
        Self::parse(resp).await
    }

    /// `POST /v1/vms/{id}/network/migration/quiesce`
    pub async fn network_migration_quiesce(&self, id: Uuid) -> Result<MigrationStateStatus> {
        let resp = self
            .authed(
                self.http
                    .post(self.url(&format!("/v1/vms/{id}/network/migration/quiesce"))?),
            )
            .send()
            .await?;
        Self::parse(resp).await
    }

    /// `GET /v1/vms/{id}/network/migration/export`
    pub async fn network_migration_export(&self, id: Uuid) -> Result<VmNetworkStateSnapshot> {
        let resp = self
            .authed(
                self.http
                    .get(self.url(&format!("/v1/vms/{id}/network/migration/export"))?),
            )
            .send()
            .await?;
        Self::parse(resp).await
    }

    /// `POST /v1/vms/{id}/network/migration/restore`
    pub async fn network_migration_restore(
        &self,
        id: Uuid,
        snapshot: &VmNetworkStateSnapshot,
    ) -> Result<MigrationStateStatus> {
        let resp = self
            .authed(
                self.http
                    .post(self.url(&format!("/v1/vms/{id}/network/migration/restore"))?),
            )
            .json(snapshot)
            .send()
            .await?;
        Self::parse(resp).await
    }

    /// `POST /v1/vms/{id}/network/migration/resume`
    pub async fn network_migration_resume(&self, id: Uuid) -> Result<MigrationStateStatus> {
        let resp = self
            .authed(
                self.http
                    .post(self.url(&format!("/v1/vms/{id}/network/migration/resume"))?),
            )
            .send()
            .await?;
        Self::parse(resp).await
    }

    /// `POST /v1/vms/{id}/qga/ping`
    pub async fn qga_ping(&self, id: Uuid) -> Result<()> {
        let resp = self
            .authed(self.http.post(self.url(&format!("/v1/vms/{id}/qga/ping"))?))
            .send()
            .await?;
        let _: OkResponse = Self::parse(resp).await?;
        Ok(())
    }

    /// `POST /v1/vms/{id}/qga/exec`
    pub async fn qga_exec(&self, id: Uuid, req: &QgaExecRequest) -> Result<QgaExecResult> {
        let resp = self
            .authed(self.http.post(self.url(&format!("/v1/vms/{id}/qga/exec"))?))
            .json(req)
            .send()
            .await?;
        Self::parse(resp).await
    }

    /// `POST /v1/vms/{id}/qga/firewall/open`
    pub async fn qga_firewall_open(
        &self,
        id: Uuid,
        req: &QgaFirewallOpenRequest,
    ) -> Result<QgaExecResult> {
        let resp = self
            .authed(
                self.http
                    .post(self.url(&format!("/v1/vms/{id}/qga/firewall/open"))?),
            )
            .json(req)
            .send()
            .await?;
        Self::parse(resp).await
    }

    /// `POST /v1/vms/{id}/qga/firewall/close`
    pub async fn qga_firewall_close(
        &self,
        id: Uuid,
        req: &QgaFirewallCloseRequest,
    ) -> Result<QgaExecResult> {
        let resp = self
            .authed(
                self.http
                    .post(self.url(&format!("/v1/vms/{id}/qga/firewall/close"))?),
            )
            .json(req)
            .send()
            .await?;
        Self::parse(resp).await
    }

    /// `GET /v1/network/groups`
    pub async fn list_network_groups(&self) -> Result<Vec<SecurityGroup>> {
        let resp = self
            .authed(self.http.get(self.url("/v1/network/groups")?))
            .send()
            .await?;
        let body: GroupListResponse = Self::parse(resp).await?;
        Ok(body.items)
    }

    /// `GET /v1/network/groups/{name}`
    pub async fn get_network_group(&self, name: &str) -> Result<SecurityGroup> {
        let resp = self
            .authed(
                self.http
                    .get(self.url(&format!("/v1/network/groups/{name}"))?),
            )
            .send()
            .await?;
        Self::parse(resp).await
    }

    /// `POST /v1/network/groups`
    pub async fn upsert_network_group(&self, group: &SecurityGroup) -> Result<SecurityGroup> {
        let resp = self
            .authed(self.http.post(self.url("/v1/network/groups")?))
            .json(group)
            .send()
            .await?;
        Self::parse(resp).await
    }

    /// `DELETE /v1/network/groups/{name}`
    pub async fn delete_network_group(&self, name: &str) -> Result<()> {
        let resp = self
            .authed(
                self.http
                    .delete(self.url(&format!("/v1/network/groups/{name}"))?),
            )
            .send()
            .await?;
        let _: DeletedResponse = Self::parse(resp).await?;
        Ok(())
    }

    /// `GET /v1/network/services` — Maglev/eBPF Service Fabric catalog.
    pub async fn list_network_services(&self) -> Result<Vec<NetworkServiceSpec>> {
        let resp = self
            .authed(self.http.get(self.url("/v1/network/services")?))
            .send()
            .await?;
        let body: NetworkServiceListResponse = Self::parse(resp).await?;
        Ok(body.items)
    }

    /// `GET /v1/network/services/{name}`
    pub async fn get_network_service(&self, name: &str) -> Result<NetworkServiceSpec> {
        let resp = self
            .authed(
                self.http
                    .get(self.url(&format!("/v1/network/services/{name}"))?),
            )
            .send()
            .await?;
        Self::parse(resp).await
    }

    /// `POST /v1/network/services`
    pub async fn upsert_network_service(
        &self,
        service: &NetworkServiceSpec,
    ) -> Result<NetworkServiceStatus> {
        let resp = self
            .authed(self.http.post(self.url("/v1/network/services")?))
            .json(service)
            .send()
            .await?;
        Self::parse(resp).await
    }

    /// `DELETE /v1/network/services/{name}`
    pub async fn delete_network_service(&self, name: &str) -> Result<()> {
        let resp = self
            .authed(
                self.http
                    .delete(self.url(&format!("/v1/network/services/{name}"))?),
            )
            .send()
            .await?;
        let _: DeletedResponse = Self::parse(resp).await?;
        Ok(())
    }

    /// `GET /v1/network/cnp`
    pub async fn list_cnp(&self) -> Result<Vec<serde_json::Value>> {
        let resp = self
            .authed(self.http.get(self.url("/v1/network/cnp")?))
            .send()
            .await?;
        let body: CnpListResponse = Self::parse(resp).await?;
        Ok(body.items)
    }

    /// `GET /v1/network/cnp/{name}`
    pub async fn get_cnp(&self, name: &str) -> Result<serde_json::Value> {
        let resp = self
            .authed(self.http.get(self.url(&format!("/v1/network/cnp/{name}"))?))
            .send()
            .await?;
        Self::parse(resp).await
    }

    /// `POST /v1/network/cnp` — returns compiled security group JSON.
    pub async fn apply_cnp(&self, doc: &serde_json::Value) -> Result<serde_json::Value> {
        let resp = self
            .authed(self.http.post(self.url("/v1/network/cnp")?))
            .json(doc)
            .send()
            .await?;
        Self::parse(resp).await
    }

    /// `DELETE /v1/network/cnp/{name}`
    pub async fn delete_cnp(&self, name: &str) -> Result<()> {
        let resp = self
            .authed(
                self.http
                    .delete(self.url(&format!("/v1/network/cnp/{name}"))?),
            )
            .send()
            .await?;
        let _: DeletedResponse = Self::parse(resp).await?;
        Ok(())
    }

    /// `GET /v1/network/identities`
    pub async fn list_identities(&self) -> Result<Vec<IdentityInfo>> {
        let resp = self
            .authed(self.http.get(self.url("/v1/network/identities")?))
            .send()
            .await?;
        let body: IdentityListResponse = Self::parse(resp).await?;
        Ok(body.items)
    }

    /// `GET /v1/network/endpoints` — CEP-*shaped* views (`identity_source` when mode=cilium).
    pub async fn list_endpoints(&self) -> Result<Vec<CiliumEndpointView>> {
        let resp = self
            .authed(self.http.get(self.url("/v1/network/endpoints")?))
            .send()
            .await?;
        let body: EndpointListResponse = Self::parse(resp).await?;
        Ok(body.items)
    }

    /// `GET /v1/network/observe`
    pub async fn network_observe(&self) -> Result<serde_json::Value> {
        let resp = self
            .authed(self.http.get(self.url("/v1/network/observe")?))
            .send()
            .await?;
        Self::parse(resp).await
    }

    /// `GET /v1/network/hubble/flows` — Hubble-lite JSON (hops when FluxVM packet-flow PR is in).
    pub async fn hubble_flows(
        &self,
        limit: Option<usize>,
        verdict: Option<&str>,
        protocol: Option<&str>,
    ) -> Result<serde_json::Value> {
        let mut url = self.url("/v1/network/hubble/flows")?;
        {
            let mut q = url.query_pairs_mut();
            if let Some(limit) = limit {
                q.append_pair("limit", &limit.to_string());
            }
            if let Some(verdict) = verdict {
                q.append_pair("verdict", verdict);
            }
            if let Some(protocol) = protocol {
                q.append_pair("protocol", protocol);
            }
        }
        let resp = self.authed(self.http.get(url)).send().await?;
        Self::parse(resp).await
    }

    /// `GET /v1/network/hubble/flows/text`
    pub async fn hubble_flows_text(
        &self,
        output: &str,
        detailed: bool,
        limit: Option<usize>,
    ) -> Result<String> {
        let mut url = self.url("/v1/network/hubble/flows/text")?;
        {
            let mut q = url.query_pairs_mut();
            q.append_pair("output", output);
            if detailed {
                q.append_pair("detailed", "true");
            }
            if let Some(limit) = limit {
                q.append_pair("limit", &limit.to_string());
            }
        }
        let resp = self.authed(self.http.get(url)).send().await?;
        let status = resp.status();
        let body = resp.text().await?;
        if !status.is_success() {
            anyhow::bail!("fluxvm hubble text {status}: {body}");
        }
        Ok(body)
    }

    /// `GET /v1/network/health`
    pub async fn network_health(&self) -> Result<DataplaneHealth> {
        let resp = self
            .authed(self.http.get(self.url("/v1/network/health")?))
            .send()
            .await?;
        Self::parse(resp).await
    }

    /// `GET /v1/network/ipcache`
    pub async fn network_ipcache(&self) -> Result<Vec<IpcacheEntry>> {
        let resp = self
            .authed(self.http.get(self.url("/v1/network/ipcache")?))
            .send()
            .await?;
        let body: IpcacheListResponse = Self::parse(resp).await?;
        Ok(body.items)
    }

    /// `POST /v1/network/refresh-dns`
    pub async fn refresh_fqdn_policies(&self) -> Result<usize> {
        let resp = self
            .authed(self.http.post(self.url("/v1/network/refresh-dns")?))
            .send()
            .await?;
        let body: RefreshDnsResponse = Self::parse(resp).await?;
        Ok(body.refreshed)
    }

    /// `GET /v1/host/gpus` — host GPU inventory for Fabric AI placement.
    pub async fn list_host_gpus(&self) -> Result<Vec<HostGpu>> {
        let resp = self
            .authed(self.http.get(self.url("/v1/host/gpus")?))
            .send()
            .await?;
        let body: HostGpuListResponse = Self::parse(resp).await?;
        Ok(body.items)
    }

    /// `GET /v1/host/gpus/preflight`
    pub async fn host_gpu_preflight(&self) -> Result<GpuPreflight> {
        let resp = self
            .authed(self.http.get(self.url("/v1/host/gpus/preflight")?))
            .send()
            .await?;
        Self::parse(resp).await
    }

    /// `POST /v1/host/gpus/bind` — explicit VFIO bind (never a side effect of VM create).
    pub async fn bind_host_gpu(&self, req: &GpuBindRequest) -> Result<GpuBindResult> {
        let resp = self
            .authed(self.http.post(self.url("/v1/host/gpus/bind")?))
            .json(req)
            .send()
            .await?;
        Self::parse(resp).await
    }

    /// `POST /v1/host/gpus/release`
    pub async fn release_host_gpu(&self, req: &GpuReleaseRequest) -> Result<HostGpu> {
        let resp = self
            .authed(self.http.post(self.url("/v1/host/gpus/release")?))
            .json(req)
            .send()
            .await?;
        Self::parse(resp).await
    }

    async fn expect_no_content(resp: reqwest::Response) -> Result<()> {
        let status = resp.status();
        if status.is_success() {
            Ok(())
        } else {
            let body = resp.text().await.unwrap_or_default();
            bail!("FluxVM request failed: {status} — {body}")
        }
    }

    async fn parse<T: serde::de::DeserializeOwned>(resp: reqwest::Response) -> Result<T> {
        let status = resp.status();
        let bytes = resp
            .bytes()
            .await
            .context("failed to read FluxVM response body")?;
        if !status.is_success() {
            let body = String::from_utf8_lossy(&bytes);
            bail!("FluxVM request failed: {status} — {body}");
        }
        serde_json::from_slice(&bytes)
            .with_context(|| format!("failed to parse FluxVM response ({status})"))
    }
}

/// A live console WebSocket, adapted to plain `AsyncRead`/`AsyncWrite` —
/// callers (e.g. `zyvor-fabricd`'s own browser-facing console WebSocket)
/// just read/write raw bytes; the WS binary-frame boundary underneath is
/// invisible on this side, matching FluxVM's own console protocol (see
/// `fluxvm_api::relay_console`'s doc comment on the other end of this
/// connection).
pub struct ConsoleWs {
    stream: tokio_tungstenite::WebSocketStream<
        tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>,
    >,
    read_buf: Vec<u8>,
}

impl tokio::io::AsyncRead for ConsoleWs {
    fn poll_read(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
        buf: &mut tokio::io::ReadBuf<'_>,
    ) -> std::task::Poll<std::io::Result<()>> {
        use futures::StreamExt;
        loop {
            if !self.read_buf.is_empty() {
                let n = std::cmp::min(self.read_buf.len(), buf.remaining());
                buf.put_slice(&self.read_buf[..n]);
                self.read_buf.drain(..n);
                return std::task::Poll::Ready(Ok(()));
            }
            match std::task::ready!(self.stream.poll_next_unpin(cx)) {
                Some(Ok(tokio_tungstenite::tungstenite::Message::Binary(data))) => {
                    self.read_buf = data.to_vec();
                }
                Some(Ok(tokio_tungstenite::tungstenite::Message::Text(t))) => {
                    self.read_buf = t.as_bytes().to_vec();
                }
                Some(Ok(tokio_tungstenite::tungstenite::Message::Close(_))) | None => {
                    return std::task::Poll::Ready(Ok(()));
                }
                Some(Ok(_)) => continue, // ping/pong/frame — not payload data
                Some(Err(e)) => return std::task::Poll::Ready(Err(std::io::Error::other(e))),
            }
        }
    }
}

impl tokio::io::AsyncWrite for ConsoleWs {
    fn poll_write(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
        buf: &[u8],
    ) -> std::task::Poll<std::io::Result<usize>> {
        use futures::SinkExt;
        if let Err(e) = std::task::ready!(self.stream.poll_ready_unpin(cx)) {
            return std::task::Poll::Ready(Err(std::io::Error::other(e)));
        }
        if let Err(e) =
            self.stream
                .start_send_unpin(tokio_tungstenite::tungstenite::Message::Binary(
                    buf.to_vec().into(),
                ))
        {
            return std::task::Poll::Ready(Err(std::io::Error::other(e)));
        }
        // `start_send` only queues the frame in the WS sink; nothing puts
        // it on the wire until a flush. Callers that just call
        // `write_all` — the normal, expected-to-be-sufficient pattern for
        // any other AsyncWrite (a socket, a file) — would otherwise have
        // their bytes sit queued forever with no error and no visible
        // symptom until the connection eventually tears down. Best-effort
        // opportunistic flush here (ignoring `Pending`/errors, which the
        // caller's own next real write/flush/drop will surface) matches
        // the semantics callers actually expect.
        let _ = self.stream.poll_flush_unpin(cx);
        std::task::Poll::Ready(Ok(buf.len()))
    }
    fn poll_flush(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<std::io::Result<()>> {
        use futures::SinkExt;
        self.stream
            .poll_flush_unpin(cx)
            .map_err(std::io::Error::other)
    }
    fn poll_shutdown(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<std::io::Result<()>> {
        use futures::SinkExt;
        self.stream
            .poll_close_unpin(cx)
            .map_err(std::io::Error::other)
    }
}

#[cfg(test)]
mod runtime_boundary_contract_tests {
    use super::*;

    #[test]
    fn decodes_fluxvm_runtime_capabilities_v1() {
        let caps: RuntimeCapabilities = serde_json::from_value(serde_json::json!({
            "apiVersion": "runtime.fluxvm.zyvor.io/v1",
            "scope": "node-local",
            "orchestrationOwner": "zyvor-fabric",
            "migration": [{
                "backend": "qemu",
                "live": true,
                "preCopy": true,
                "postCopy": true,
                "multifd": true,
                "requiresSharedStorage": true,
                "transports": ["tcp", "unix"]
            }],
            "snapshot": []
        }))
        .unwrap();
        assert_eq!(caps.scope, "node-local");
        assert_eq!(caps.migration[0].backend, BackendKind::Qemu);
        assert!(caps.migration[0].requires_shared_storage);
    }

    #[test]
    fn decodes_postcopy_status() {
        let status: MigrationStatus = serde_json::from_value(serde_json::json!({
            "phase": "postcopy-active",
            "status": "postcopy-active",
            "ram_transferred": 100,
            "ram_remaining": 20,
            "ram_total": 120,
            "total_time_ms": 50,
            "downtime_ms": 2,
            "error": null
        }))
        .unwrap();
        assert_eq!(status.phase, MigrationPhase::PostcopyActive);
    }

    #[test]
    fn decodes_dataplane_status_with_pod_ingress_fields() {
        let status: DataplaneStatus = serde_json::from_value(serde_json::json!({
            "mode": "ebpf",
            "required": true,
            "attached": true,
            "interface": "tap0",
            "identity": 7,
            "pin_dir": "/sys/fs/bpf/fluxvm/vms/x",
            "schema_version": 9,
            "schema_compatible": true,
            "policy_synced": true,
            "pod_ingress_required": true,
            "pod_ingress_attached": false,
            "policy": {
                "default_allow": false,
                "allow_cidrs": [],
                "allow_ports": []
            }
        }))
        .unwrap();
        assert!(status.pod_ingress_required);
        assert!(!status.pod_ingress_attached);
        assert_eq!(status.schema_version, Some(9));
    }

    #[test]
    fn dataplane_status_pod_ingress_defaults_when_absent() {
        let status: DataplaneStatus = serde_json::from_value(serde_json::json!({
            "mode": "legacy",
            "required": false,
            "attached": false,
            "interface": null,
            "identity": 1,
            "pin_dir": null,
            "schema_version": null,
            "schema_compatible": true,
            "policy_synced": true,
            "policy": {
                "default_allow": true,
                "allow_cidrs": [],
                "allow_ports": []
            }
        }))
        .unwrap();
        assert!(!status.pod_ingress_required);
        assert!(!status.pod_ingress_attached);
    }

    #[test]
    fn decodes_dataplane_stats_with_pod_policy() {
        let stats: DataplaneStats = serde_json::from_value(serde_json::json!({
            "allowed_packets": 10,
            "allowed_bytes": 100,
            "dropped_packets": 1,
            "dropped_bytes": 8,
            "pod_policy": {
                "egress": {"allowed": 4, "dropped": 1, "audited": 0},
                "ingress": {"allowed": 6, "dropped": 0, "audited": 2},
                "directional": true
            }
        }))
        .unwrap();
        let pp = stats.pod_policy.expect("pod_policy");
        assert!(pp.directional);
        assert_eq!(pp.egress.allowed, 4);
        assert_eq!(pp.ingress.audited, 2);
    }

    #[test]
    fn decodes_drop_reason_list() {
        let list: DropReasonListResponse = serde_json::from_value(serde_json::json!({
            "items": [{
                "identity": 1,
                "family": 4,
                "source": "10.0.0.1",
                "destination": "10.0.0.2",
                "source_port": 1,
                "destination_port": 80,
                "protocol": 6,
                "reason_code": 8,
                "reason": "default-deny",
                "action": "drop",
                "packets": 2,
                "bytes": 64,
                "last_seen_ns": 9
            }]
        }))
        .unwrap();
        assert_eq!(list.items[0].reason, "default-deny");
    }

    #[test]
    fn decodes_migration_receiver_info_not_vm_record() {
        // Regression: GET /v1/migration/receivers/{id} returns MigrationReceiverInfo,
        // not a full VmRecord.
        let info: MigrationReceiverInfo = serde_json::from_value(serde_json::json!({
            "id": "00000000-0000-0000-0000-000000000001",
            "status": "receiving",
            "port": 4444,
            "expires_at": "2026-01-01T01:00:00Z"
        }))
        .unwrap();
        assert_eq!(info.port, 4444);
        assert_eq!(info.status, VmStatus::Receiving);
    }

    #[test]
    fn decodes_readyz_secure_containers_nested() {
        let rz: ReadyzResponse = serde_json::from_value(serde_json::json!({
            "ok": true,
            "kvm": true,
            "secure_containers": {
                "available": true,
                "shim_installed": true,
                "guest_image_present": false
            }
        }))
        .unwrap();
        let sc = rz.secure_containers.expect("secure_containers");
        assert!(sc.available && sc.shim_installed);
        assert!(!sc.guest_image_present);
    }
}

#[cfg(test)]
mod idempotency_key_tests {
    use super::*;

    #[test]
    fn validation_matches_fluxvm() {
        assert!(validate_idempotency_key("create-vm-7f3a").is_ok());
        assert!(validate_idempotency_key(&"k".repeat(255)).is_ok());
        assert!(validate_idempotency_key("").is_err());
        assert!(validate_idempotency_key(&"k".repeat(256)).is_err());
        assert!(validate_idempotency_key("has space").is_err());
        assert!(validate_idempotency_key("tab\tkey").is_err());
        assert!(validate_idempotency_key("naïve").is_err());
    }
}
