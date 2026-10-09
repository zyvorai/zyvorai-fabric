// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: Apache-2.0

use anyhow::{Context, Result};
use clap::{CommandFactory, Parser, Subcommand, ValueEnum};
use clap_complete::{generate, Shell};
use reqwest::Client;
use serde::Serialize;
use std::collections::HashMap;
use std::io;
use std::sync::Mutex;
use tabled::{Table, Tabled};
use vm_model::{CreateVMRequest, VM};

use crate::style::{self, ColorMode};

static API_BASE: Mutex<String> = Mutex::new(String::new());

const DEFAULT_ROOT: &str = "http://localhost:9095";
const DEFAULT_ADDR: &str = "localhost:9095";

/// True when neither `--server` nor `ZYVOR_FABRIC_URL` / `FABRIC_URL` picked the server.
fn server_is_default(server: Option<&str>) -> bool {
    server.is_none()
        && std::env::var("ZYVOR_FABRIC_URL").is_err()
        && std::env::var("FABRIC_URL").is_err()
}

/// A default install serves TLS on 9095 (`[tls] enabled`, self-signed), while the
/// Docker config serves plain HTTP. With no server given, decide which one this is.
///
/// Sends one plain-HTTP request and looks at the reply: an HTTP server answers
/// `HTTP/…`; a TLS listener answers a plaintext request with a TLS alert or just
/// closes. No certificate is involved, so nothing is trusted or skipped here.
async fn port_speaks_tls(addr: &str) -> bool {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    let probe = async {
        let mut stream = tokio::net::TcpStream::connect(addr).await.ok()?;
        stream
            .write_all(b"GET /health HTTP/1.0\r\nHost: localhost\r\n\r\n")
            .await
            .ok()?;
        let mut buf = [0u8; 5];
        // A reset or an empty read is what a TLS listener does with plaintext.
        let n = stream.read(&mut buf).await.unwrap_or(0);
        Some(!buf[..n].starts_with(b"HTTP/"))
    };
    matches!(
        tokio::time::timeout(std::time::Duration::from_secs(1), probe).await,
        Ok(Some(true))
    )
}

/// Fabric API root including `/api` (override with `--server` / `ZYVOR_FABRIC_URL` / `FABRIC_URL`).
fn resolve_api_base(server: Option<&str>) -> String {
    let root = server
        .map(|s| s.to_string())
        .or_else(|| std::env::var("ZYVOR_FABRIC_URL").ok())
        .or_else(|| std::env::var("FABRIC_URL").ok())
        .unwrap_or_else(|| DEFAULT_ROOT.to_string());
    let root = root.trim_end_matches('/').to_string();
    if root.ends_with("/api") {
        root
    } else {
        format!("{root}/api")
    }
}

fn set_api_base(base: String) {
    *API_BASE.lock().expect("api base lock") = base;
}

fn api_base() -> String {
    let guard = API_BASE.lock().expect("api base lock");
    if guard.is_empty() {
        resolve_api_base(None)
    } else {
        guard.clone()
    }
}

fn auth_token(token_flag: Option<&str>) -> Option<String> {
    token_flag.map(|s| s.to_string()).or_else(|| {
        std::env::var("ZYVOR_FABRIC_TOKEN")
            .or_else(|_| std::env::var("FABRIC_TOKEN"))
            .ok()
    })
}

// ─── Output format ───────────────────────────────────────────────────────────

#[derive(Clone, Copy, ValueEnum)]
pub enum OutputFormat {
    Table,
    Json,
    Yaml,
}

// ─── Main CLI ────────────────────────────────────────────────────────────────

#[derive(Parser)]
#[command(name = "fabricctl")]
#[command(about = "Zyvor Fabric CLI", long_about = None)]
// clap's --version auto-wiring needs the "cargo" feature (this workspace
// doesn't enable it, backend/Cargo.toml:75) -- pass the version explicitly
// via the plain `env!` std macro instead, which needs no clap feature.
// Without this, --version doesn't exist at all ("unexpected argument"),
// found live while validating this repo's first non-Ubuntu CI build.
#[command(version = env!("CARGO_PKG_VERSION"))]
pub struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,

    /// Output format
    #[arg(long, short = 'o', default_value = "table", global = true)]
    output: OutputFormat,

    /// Colorize output (auto detects TTY; honors NO_COLOR)
    #[arg(long, default_value = "auto", global = true, value_enum)]
    color: ColorMode,

    /// Fabric API URL (overrides ZYVOR_FABRIC_URL / FABRIC_URL; default http://localhost:9095, or https when that answers)
    #[arg(long, global = true)]
    server: Option<String>,

    /// Bearer token (overrides ZYVOR_FABRIC_TOKEN / FABRIC_TOKEN)
    #[arg(long, global = true)]
    token: Option<String>,
}

impl Cli {
    /// Build clap Command with Cilium-style grouped root `--help`.
    pub fn command_with_grouped_help() -> clap::Command {
        let mode = peek_color_mode();
        let choice = match mode {
            ColorMode::Always => clap::ColorChoice::Always,
            ColorMode::Never => clap::ColorChoice::Never,
            ColorMode::Auto => clap::ColorChoice::Auto,
        };
        Self::command()
            .color(choice)
            .override_help(crate::help::render_root_help(mode.enabled()))
    }
}

/// Peek `--color` from argv before full parse (for help rendering).
fn peek_color_mode() -> ColorMode {
    let mut args = std::env::args().skip(1);
    while let Some(a) = args.next() {
        if a == "--color" {
            if let Some(v) = args.next() {
                return ColorMode::from_str(&v, true).unwrap_or(ColorMode::Auto);
            }
        } else if let Some(v) = a.strip_prefix("--color=") {
            return ColorMode::from_str(v, true).unwrap_or(ColorMode::Auto);
        }
    }
    ColorMode::Auto
}

#[derive(Subcommand)]
enum Commands {
    // ─── VM Management ───────────────────────────────────────────────────
    /// List all VMs
    List,
    /// Get VM information
    Info { name: String },
    /// Fork a running VM into copies that share its memory snapshot (flux-vm KVM engine)
    Fork {
        name: String,
        /// Children to start (1-32)
        #[arg(long, default_value = "1", value_parser = clap::value_parser!(u32).range(1..=32))]
        count: u32,
        /// Name prefix for the children
        #[arg(long)]
        prefix: Option<String>,
        /// Wait for a first command in every child and report the time FluxVM measured
        #[arg(long)]
        ready: bool,
        /// Idempotency key, so retrying returns the same children
        #[arg(long)]
        idempotency_key: Option<String>,
    },
    /// Show a VM's real memory footprint (PSS) and balloon
    Memory { name: String },
    /// Show or set a VM's memory balloon (Beta; flux-vm KVM engine only)
    Balloon {
        name: String,
        /// Inflate to this many MiB taken from the guest; 0 deflates. Omit to just show it.
        #[arg(long)]
        mib: Option<u64>,
    },
    /// Create a new VM
    Create {
        name: String,
        #[arg(long)]
        image: String,
        #[arg(long, default_value = "2")]
        cpus: u32,
        #[arg(long, default_value = "2048")]
        memory: u64,
        #[arg(long, default_value = "20")]
        disk: u64,
        #[arg(long)]
        hostname: Option<String>,
        #[arg(long, value_delimiter = ',')]
        tags: Option<Vec<String>>,
        #[arg(long, value_parser = parse_label)]
        label: Option<Vec<(String, String)>>,
        /// Optional tenant id (stored as labels.tenant and passed to FluxVM)
        #[arg(long)]
        tenant: Option<String>,
        /// Host NIC for a bridge-less FluxVM tap (`l2-uplink`). Mutually exclusive with bridged networking.
        #[arg(long)]
        direct_uplink: Option<String>,
        /// `l2-uplink` (default) or `peer-veth`.
        #[arg(long)]
        direct_mode: Option<String>,
        /// Guest IPv4 for `l2-uplink` ARP steering. Repeat up to 8 times.
        #[arg(long = "direct-guest-ip")]
        direct_guest_ip: Vec<String>,
    },
    /// Set a bridge-less direct uplink on an existing VM (recreates it in FluxVM)
    DirectUplink {
        name: String,
        #[arg(long)]
        uplink: String,
        #[arg(long)]
        direct_mode: Option<String>,
        #[arg(long = "direct-guest-ip")]
        direct_guest_ip: Vec<String>,
    },
    /// Start a VM
    Start { name: String },
    /// Stop a VM
    Stop { name: String },
    /// Restart a VM
    Restart { name: String },
    /// Delete a VM
    Delete { name: String },
    /// Get VM metrics
    Metrics { name: String },

    /// VM edge dataplane (FluxVM Network Fabric) — not Fabric SDN network-policies
    #[command(subcommand)]
    Dataplane(DataplaneCmd),

    // ─── Config import (JSON/YAML) ───────────────────────────────────────
    /// Apply configuration from a JSON or YAML file
    Apply {
        /// Path to JSON or YAML config file
        #[arg(short, long)]
        file: String,
    },
    /// Export current config to JSON or YAML
    Export {
        /// Resource type to export (vm, firewall, network-policy, service, qos, dns, vpn, mirror, nat, monitor)
        resource: String,
    },

    // ─── Network Policies ────────────────────────────────────────────────
    /// Manage network policies
    #[command(subcommand)]
    Policy(PolicyCmd),

    // ─── Firewall ────────────────────────────────────────────────────────
    /// Manage VM firewall profiles and zones
    #[command(subcommand)]
    Firewall(FirewallCmd),

    // ─── Service Mesh ────────────────────────────────────────────────────
    /// Manage service mesh services
    #[command(subcommand)]
    Service(ServiceCmd),

    // ─── QoS / Traffic Shaping ───────────────────────────────────────────
    /// Manage QoS / traffic shaping policies
    #[command(subcommand)]
    Qos(QosCmd),

    // ─── DNS ─────────────────────────────────────────────────────────────
    /// Manage DNS zones and policies
    #[command(subcommand)]
    Dns(DnsCmd),

    // ─── VPN Mesh ────────────────────────────────────────────────────────
    /// Manage VPN tunnels and networks
    #[command(subcommand)]
    Vpn(VpnCmd),

    // ─── Packet Mirror ───────────────────────────────────────────────────
    /// Manage packet mirror sessions
    #[command(subcommand)]
    Mirror(MirrorCmd),

    // ─── Agent runtime ───────────────────────────────────────────────────
    /// Review and decide agent approval requests
    #[command(subcommand)]
    Approval(ApprovalCmd),

    /// Publish and inspect agent skills
    #[command(subcommand)]
    Skill(SkillCmd),

    /// Show the tamper-evident agent action journal
    AgentAudit {
        /// Only entries for this session id
        #[arg(short, long)]
        session: Option<String>,
        /// Maximum number of entries (newest last)
        #[arg(short, long, default_value_t = 50)]
        limit: usize,
    },

    // ─── NAT Gateway ─────────────────────────────────────────────────────
    /// Manage NAT rules, pools, and gateways
    #[command(subcommand)]
    Nat(NatCmd),

    // ─── Network Monitor ─────────────────────────────────────────────────
    /// Manage network monitoring policies and view alerts
    #[command(subcommand)]
    Monitor(MonitorCmd),

    // ─── Ceph ────────────────────────────────────────────────────────────
    /// Manage Ceph storage pools and RBD images
    #[command(subcommand)]
    Ceph(CephCmd),

    // ─── Network (networkd) ──────────────────────────────────────────────
    /// Manage networkd bridges, VLANs, bonds, taps, port-forwards
    #[command(subcommand)]
    Net(NetCmd),

    /// FluxVM runtime contract (capabilities + native migration)
    #[command(subcommand)]
    Runtime(RuntimeCmd),

    // ─── ContainerGroup (FluxVM Secure Containers) ──────────────────────
    /// Manage ContainerGroup workloads (FluxVM Secure Containers)
    #[command(subcommand)]
    ContainerGroup(ContainerGroupCmd),

    /// Fabric AI Workloads (preview): models, deployments, endpoints, GPUs
    #[command(subcommand)]
    Ai(AiCmd),

    // ─── Meta (Cilium-parity) ────────────────────────────────────────────
    /// Display Fabric / dataplane status
    Status,
    /// Show effective CLI configuration
    Config,
    /// Generate shell completion scripts
    Completion {
        #[arg(value_enum)]
        shell: Shell,
    },
}

// ─── Sub-command enums ───────────────────────────────────────────────────────

#[derive(Subcommand)]
enum DataplaneCmd {
    /// Show dataplane attach/schema status
    Status { name: String },
    /// Get or set per-VM edge policy
    #[command(subcommand)]
    Policy(DataplanePolicyCmd),
    /// Show allow/drop counters
    Stats { name: String },
    /// List recent sampled flows
    Flows {
        name: String,
        #[arg(long, default_value = "100")]
        limit: usize,
    },
    /// Declared + group-merged effective policy
    Effective { name: String },
    /// Explain whether dest:port would drop under current policy
    Explain {
        name: String,
        dest: String,
        #[arg(long, default_value_t = 0)]
        port: u16,
        #[arg(long, default_value = "any")]
        proto: String,
    },
    /// Show which live flows would drop if Guard were enabled
    DryRun {
        name: String,
        #[arg(long, default_value_t = 64)]
        limit: usize,
    },
    /// Cluster dataplane health
    Health,
    /// Guest IP → identity cache
    Ipcache,
    /// Re-resolve FQDN allowlists into CIDRs
    RefreshDns,
    /// Snapshot identities/groups/CNPs/labeled VMs
    Observe,
    /// Hubble-style packet flows (JSON from Fabric; color/plain via --style)
    ///
    /// Uses `--style` (not global `-o/--output`) because `fabricctl` already
    /// reserves `-o` for table|json|yaml. Default: color on TTY, else plain.
    Hubble {
        #[arg(long = "style", default_value = "auto")]
        style: String,
        #[arg(long, default_value_t = 64)]
        limit: usize,
    },
    /// Reserved + group identities
    Identities,
    /// CiliumEndpoint-*shaped* VM views (`identity_source`)
    Endpoints,
    /// Maglev/eBPF Service Fabric VIPs (FluxVM edge — not Fabric SDN)
    #[command(subcommand)]
    Service(DataplaneServiceCmd),
    /// Security groups (FluxVM edge — not Fabric SDN)
    #[command(subcommand)]
    Group(DataplaneGroupCmd),
    /// CNP documents compiled onto security groups
    #[command(subcommand)]
    Cnp(DataplaneCnpCmd),
}

#[derive(Subcommand)]
enum DataplaneServiceCmd {
    List,
    Get {
        name: String,
    },
    /// Host Maglev/service dataplane status (schema v4)
    Status,
    /// Host Maglev/service counters
    Stats,
    /// Backend health report
    Health,
    /// Run active health reconcile
    Reconcile,
    /// Expire conntrack / reverse-NAT state
    Gc,
    /// VIP advertisement snapshot
    Advertisements,
    /// FluxScope service flows
    Flows {
        #[arg(long, default_value_t = 256)]
        limit: u32,
    },
    /// Export service flows via OTLP/HTTP JSON
    ExportTelemetry {
        #[arg(long, default_value_t = 1024)]
        limit: u32,
    },
    /// Export HA conntrack/NAT delta batch
    Delta {
        name: String,
        #[arg(long, default_value_t = 0)]
        after_seq: u64,
        #[arg(long, default_value_t = 1024)]
        max_entries: u32,
    },
    /// Create/update from a JSON file (NetworkServiceSpec shape)
    Apply {
        #[arg(short, long)]
        file: String,
    },
    Delete {
        name: String,
    },
}

#[derive(Subcommand)]
enum DataplaneGroupCmd {
    List,
    Get {
        name: String,
    },
    /// Create/update from a JSON file (SecurityGroup shape)
    Create {
        #[arg(short, long)]
        file: String,
    },
    Delete {
        name: String,
    },
}

#[derive(Subcommand)]
enum DataplaneCnpCmd {
    List,
    Get {
        name: String,
    },
    /// Apply a CNP JSON document
    Apply {
        #[arg(short, long)]
        file: String,
    },
    Delete {
        name: String,
    },
}

#[derive(Subcommand)]
enum DataplanePolicyCmd {
    /// Get current policy
    Get { name: String },
    /// Set policy from a JSON file
    Set {
        name: String,
        #[arg(short, long)]
        file: String,
    },
    /// Enforce default-deny (Cilium-style Guard)
    Guard { name: String },
    /// Evaluate policy without dropping (audit)
    Audit { name: String },
    /// Default allow
    Open { name: String },
    /// Swap allow/deny CIDRs and flip default allow
    Invert { name: String },
    /// Add a deny CIDR (host becomes /32)
    Block {
        name: String,
        #[arg(long)]
        cidr: String,
    },
    /// Add an allow CIDR and optional port
    Allow {
        name: String,
        #[arg(long)]
        cidr: Option<String>,
        #[arg(long)]
        port: Option<String>,
        #[arg(long)]
        entity: Option<String>,
    },
}

#[derive(Subcommand)]
enum PolicyCmd {
    /// List network policies
    List,
    /// Get a network policy
    Get { id: String },
    /// Create a network policy from JSON/YAML file
    Create {
        #[arg(short, long)]
        file: String,
    },
    /// Delete a network policy
    Delete { id: String },
    /// Sync network policies
    Sync,
    /// Show policy enforcement status
    Status,
}

#[derive(Subcommand)]
enum FirewallCmd {
    /// List firewall profiles
    List,
    /// Get a firewall profile
    Get { id: String },
    /// Create a firewall profile from JSON/YAML file
    Create {
        #[arg(short, long)]
        file: String,
    },
    /// Delete a firewall profile
    Delete { id: String },
    /// List firewall zones
    Zones,
    /// Assign firewall to a VM
    Assign {
        /// VM name
        vm: String,
        /// Profile ID
        #[arg(long)]
        profile: String,
    },
    /// Remove firewall from a VM
    Unassign { vm: String },
    /// Sync firewall rules
    Sync,
    /// Show firewall status
    Status,
}

#[derive(Subcommand)]
enum ServiceCmd {
    /// List services
    List,
    /// Get a service
    Get { id: String },
    /// Create a service from JSON/YAML file
    Create {
        #[arg(short, long)]
        file: String,
    },
    /// Delete a service
    Delete { id: String },
    /// Show service backends
    Backends { id: String },
    /// Sync services
    Sync,
    /// Show service mesh status
    Status,
}

#[derive(Subcommand)]
enum QosCmd {
    /// List QoS policies
    List,
    /// Get a QoS policy
    Get { id: String },
    /// Create a QoS policy from JSON/YAML file
    Create {
        #[arg(short, long)]
        file: String,
    },
    /// Delete a QoS policy
    Delete { id: String },
    /// Sync QoS policies
    Sync,
    /// Show QoS status
    Status,
}

#[derive(Subcommand)]
enum DnsCmd {
    /// List DNS zones
    Zones,
    /// List DNS policies
    Policies,
    /// Get a DNS zone
    GetZone { id: String },
    /// Create a DNS zone from JSON/YAML file
    CreateZone {
        #[arg(short, long)]
        file: String,
    },
    /// Delete a DNS zone
    DeleteZone { id: String },
    /// Create a DNS policy from JSON/YAML file
    CreatePolicy {
        #[arg(short, long)]
        file: String,
    },
    /// Delete a DNS policy
    DeletePolicy { id: String },
    /// List DNS records
    Records,
    /// Sync DNS
    Sync,
}

#[derive(Subcommand)]
enum ApprovalCmd {
    /// List agent approval requests
    List,
    /// Approve a pending request
    Approve {
        id: String,
        #[arg(short, long)]
        comment: Option<String>,
        /// For egress requests: `once` (default) or `session` (all later requests to that host)
        #[arg(long, value_parser = ["once", "session"])]
        scope: Option<String>,
    },
    /// Deny a pending request
    Deny {
        id: String,
        #[arg(short, long)]
        comment: Option<String>,
    },
}

#[derive(Subcommand)]
enum SkillCmd {
    /// List skills (current version of each)
    List,
    /// Show a skill and its versions
    Show { name: String },
    /// Publish a skill from a directory containing SKILL.md
    Publish {
        /// Directory to read; its files keep their relative paths
        dir: String,
        /// Skill name (defaults to the directory name)
        #[arg(short, long)]
        name: Option<String>,
        #[arg(short, long)]
        description: Option<String>,
        /// Restrict the skill to agents whose skill_scope the operator allows
        #[arg(short, long)]
        scope: Option<String>,
    },
    /// Delete a skill (refused while a deployed agent uses it)
    Delete { name: String },
}

#[derive(Subcommand)]
enum VpnCmd {
    /// List VPN tunnels
    Tunnels,
    /// List VPN networks
    Networks,
    /// Get a VPN tunnel
    GetTunnel { id: String },
    /// Create a VPN tunnel from JSON/YAML file
    CreateTunnel {
        #[arg(short, long)]
        file: String,
    },
    /// Delete a VPN tunnel
    DeleteTunnel { id: String },
    /// Create a VPN network from JSON/YAML file
    CreateNetwork {
        #[arg(short, long)]
        file: String,
    },
    /// Delete a VPN network
    DeleteNetwork { id: String },
    /// Sync VPN tunnels
    Sync,
    /// Show VPN status
    Status,
}

#[derive(Subcommand)]
enum MirrorCmd {
    /// List mirror sessions
    List,
    /// Get a mirror session
    Get { id: String },
    /// Create a mirror session from JSON/YAML file
    Create {
        #[arg(short, long)]
        file: String,
    },
    /// Delete a mirror session
    Delete { id: String },
    /// Sync mirror sessions
    Sync,
    /// Show mirror status
    Status,
}

#[derive(Subcommand)]
enum NatCmd {
    /// List NAT rules
    Rules,
    /// List NAT pools
    Pools,
    /// List NAT gateways
    Gateways,
    /// Get a NAT rule
    GetRule { id: String },
    /// Create a NAT rule from JSON/YAML file
    CreateRule {
        #[arg(short, long)]
        file: String,
    },
    /// Delete a NAT rule
    DeleteRule { id: String },
    /// Create a NAT pool from JSON/YAML file
    CreatePool {
        #[arg(short, long)]
        file: String,
    },
    /// Delete a NAT pool
    DeletePool { id: String },
    /// Create a NAT gateway from JSON/YAML file
    CreateGateway {
        #[arg(short, long)]
        file: String,
    },
    /// Delete a NAT gateway
    DeleteGateway { id: String },
    /// Sync NAT rules
    Sync,
    /// Show NAT status
    Status,
}

#[derive(Subcommand)]
enum MonitorCmd {
    /// List monitor policies
    List,
    /// Get a monitor policy
    Get { id: String },
    /// Create a monitor policy from JSON/YAML file
    Create {
        #[arg(short, long)]
        file: String,
    },
    /// Delete a monitor policy
    Delete { id: String },
    /// Show live network metrics
    Metrics,
    /// Show VM-specific metrics
    VmMetrics { name: String },
    /// List bandwidth alerts
    Alerts,
    /// Sync monitor policies
    Sync,
    /// Show monitor status
    Status,
}

#[derive(Subcommand)]
enum CephCmd {
    /// Create a Ceph storage pool
    Create {
        /// Pool name in zyvor-fabricd
        name: String,
        /// Ceph monitor addresses (comma-separated)
        #[arg(long)]
        monitors: String,
        /// Ceph pool name (e.g. "rbd")
        #[arg(long)]
        pool: String,
        /// Ceph user (default: admin)
        #[arg(long)]
        user: Option<String>,
        /// Path to keyring file
        #[arg(long)]
        keyring: Option<String>,
        /// Auto-start on daemon startup
        #[arg(long, default_value = "true")]
        auto_start: bool,
    },
    /// List all storage pools (shows Ceph pools)
    Pools,
    /// Get Ceph cluster health
    Health { name: String },
    /// Get Ceph pool stats
    Stats { name: String },
    /// List RBD images in a Ceph pool
    Images { name: String },
    /// Create an RBD image
    CreateImage {
        /// Storage pool name
        pool: String,
        /// Image name
        name: String,
        /// Image size in MB
        #[arg(long)]
        size: u64,
    },
    /// Delete an RBD image
    DeleteImage {
        /// Storage pool name
        pool: String,
        /// Image name
        name: String,
    },
    /// Delete a Ceph storage pool
    Delete { name: String },
}

#[derive(Subcommand)]
enum NetCmd {
    /// List bridges
    Bridges,
    /// List VLANs
    Vlans,
    /// List bonds
    Bonds,
    /// List taps
    Taps,
    /// List port forwards
    Forwards,
    /// Create a bridge from JSON/YAML file
    CreateBridge {
        #[arg(short, long)]
        file: String,
    },
    /// Create a VLAN from JSON/YAML file
    CreateVlan {
        #[arg(short, long)]
        file: String,
    },
    /// Create a bond from JSON/YAML file
    CreateBond {
        #[arg(short, long)]
        file: String,
    },
    /// Create a port forward from JSON/YAML file
    CreateForward {
        #[arg(short, long)]
        file: String,
    },
    /// Sync port forwards
    SyncForwards,
    /// Reload networkd
    Reload,
    /// Show link status
    Links,
}

#[derive(Subcommand)]
enum RuntimeCmd {
    /// Show FluxVM `/v1/runtime/capabilities` via Fabric proxy
    Capabilities,
    /// Native FluxVM migration transport (prepared-target / source-side)
    #[command(subcommand)]
    Migrate(RuntimeMigrateCmd),
}

#[derive(Subcommand)]
enum RuntimeMigrateCmd {
    /// Start source-side migration to a prepared URI
    Start {
        name: String,
        /// QEMU migration URI, e.g. tcp:10.0.0.2:4444
        #[arg(long)]
        uri: String,
        #[arg(long, default_value = "pre-copy")]
        mode: String,
        #[arg(long)]
        shared_storage: bool,
        #[arg(long)]
        bandwidth_mbps: Option<u64>,
        #[arg(long)]
        multifd_channels: Option<u8>,
        #[arg(long)]
        transfer_network_state: bool,
    },
    /// Arm an incoming QEMU receiver on the target FluxVM node
    PrepareReceiver {
        name: String,
        #[arg(long)]
        disk_path: String,
        #[arg(long)]
        listen_host: String,
        #[arg(long)]
        target_node: Option<String>,
        #[arg(long)]
        receiver_ttl_seconds: Option<u64>,
    },
    /// Activate a prepared receiver after VMM transport completes
    ActivateReceiver {
        id: String,
        #[arg(long)]
        target_node: Option<String>,
    },
    /// Abort a prepared receiver
    AbortReceiver {
        id: String,
        #[arg(long)]
        target_node: Option<String>,
    },
    Status {
        name: String,
    },
    Cancel {
        name: String,
    },
}

#[derive(Subcommand)]
enum ContainerGroupCmd {
    /// List all ContainerGroups
    List,
    /// Show a ContainerGroup's stored spec
    Info { name: String },
    /// Apply a ContainerGroup spec from a JSON or YAML file
    Apply {
        /// Path to a JSON or YAML file with a ContainerGroupSpec (either the
        /// bare spec, or `{"spec": {...}}`)
        #[arg(short, long)]
        file: String,
    },
    /// Delete a ContainerGroup
    Delete { name: String },
    /// List recent ContainerGroup audit events (create/apply/delete/quota/placement)
    Events,
    /// Manage backups of a ContainerGroup's hostPath volumes
    #[command(subcommand)]
    Backup(ContainerGroupBackupCmd),
}

#[derive(Subcommand)]
enum ContainerGroupBackupCmd {
    /// Tar+gzip a ContainerGroup's hostPath volumes into a new backup
    Create {
        container_group_name: String,
        #[arg(long, default_value = "30")]
        retention_days: u32,
    },
    /// List backups
    List,
    /// Show a backup's details
    Info { id: String },
    /// Delete a backup (and its archive file)
    Delete { id: String },
    /// Restore a backup back onto its original host paths
    Restore { id: String },
}

/// Fabric AI Workloads CLI (preview). OpenAI-compatible inference on GPU VMs.
#[derive(Subcommand)]
enum AiCmd {
    /// Register or list model artifacts
    #[command(subcommand)]
    Model(AiModelCmd),
    /// Inference profiles (runtime + GPU/CPU shape)
    #[command(subcommand)]
    Profile(AiProfileCmd),
    /// Deploy a model with a profile
    Deploy {
        /// Model artifact name (or use --model)
        model: Option<String>,
        #[arg(long)]
        name: Option<String>,
        #[arg(long)]
        model_name: Option<String>,
        #[arg(long, default_value = "vllm")]
        runtime: String,
        #[arg(long, default_value = "default")]
        profile: String,
        #[arg(long, default_value_t = 1)]
        gpu: u32,
        #[arg(long, default_value_t = 24)]
        vram: u32,
        #[arg(long, default_value_t = 8)]
        cpu: u32,
        #[arg(long, default_value_t = 32)]
        memory: u32,
        #[arg(long, default_value_t = 1)]
        replicas: u32,
    },
    /// Deployment scale / info / list / delete
    #[command(subcommand)]
    Deployment(AiDeploymentCmd),
    /// Expose or list OpenAI-compatible endpoints
    #[command(subcommand)]
    Endpoint(AiEndpointCmd),
    /// Endpoint API keys (Phase 4)
    #[command(subcommand)]
    Key(AiKeyCmd),
    /// List host GPUs (FluxVM inventory + Fabric allocation)
    Gpus,
    /// Inference nodes (Janus lab inventory or registered hosts)
    #[command(subcommand, visible_alias = "nodes")]
    Node(AiNodeCmd),
    /// Aggregate GPU / replica capacity
    Capacity,
    /// Recent AI audit events
    Events,
}

#[derive(Subcommand)]
enum AiModelCmd {
    /// List model artifacts
    List,
    /// Register a model (`hf://org/name` or local path)
    Add {
        name: String,
        #[arg(long)]
        source: String,
        #[arg(long)]
        revision: Option<String>,
        #[arg(long, default_value = "safetensors")]
        format: String,
    },
    /// Show one model
    Info { name: String },
    /// Delete a model artifact
    Delete { name: String },
}

#[derive(Subcommand)]
enum AiProfileCmd {
    List,
    Add {
        name: String,
        #[arg(long, default_value = "vllm")]
        runtime: String,
        #[arg(long, default_value = "nvidia")]
        vendor: String,
        #[arg(long, default_value_t = 1)]
        gpu: u32,
        #[arg(long, default_value_t = 24)]
        vram: u32,
        #[arg(long, default_value_t = 8)]
        cpu: u32,
        #[arg(long, default_value_t = 32)]
        memory: u32,
    },
    Info {
        name: String,
    },
    Delete {
        name: String,
    },
}

#[derive(Subcommand)]
enum AiDeploymentCmd {
    List,
    Info {
        name: String,
    },
    Scale {
        name: String,
        #[arg(long)]
        replicas: u32,
    },
    /// Configure queue/TTFT autoscaling
    Autoscale {
        name: String,
        #[arg(long)]
        enable: bool,
        #[arg(long, default_value_t = 1)]
        min: u32,
        #[arg(long, default_value_t = 4)]
        max: u32,
        #[arg(long, default_value_t = false)]
        scale_to_zero: bool,
        #[arg(long, default_value_t = 20)]
        scale_out_queue: u32,
        #[arg(long, default_value_t = 30)]
        scale_out_seconds: u64,
        #[arg(long, default_value_t = 2)]
        scale_in_queue: u32,
        #[arg(long, default_value_t = 600)]
        scale_in_seconds: u64,
    },
    /// Gracefully drain Maglev backends
    Drain {
        name: String,
        #[arg(long)]
        replica: Option<String>,
        #[arg(long, default_value_t = 30)]
        grace_seconds: u64,
    },
    /// Start a rolling / canary / blue-green rollout
    Rollout {
        name: String,
        #[arg(long, default_value = "rolling")]
        strategy: String,
        #[arg(long)]
        target_model: Option<String>,
        #[arg(long, default_value_t = 10)]
        canary_percent: u8,
    },
    /// Show last scraped AI metrics + Maglev weights
    Metrics {
        name: String,
    },
    Delete {
        name: String,
    },
}

#[derive(Subcommand)]
enum AiEndpointCmd {
    List,
    /// Expose a deployment as an OpenAI-compatible Maglev VIP
    Expose {
        /// Deployment name
        deployment: String,
        #[arg(long)]
        name: Option<String>,
        #[arg(long, default_value_t = 8000)]
        port: u16,
        #[arg(long)]
        vip: Option<String>,
        #[arg(long, default_value_t = true)]
        openai_compatible: bool,
        /// Maglev weight strategy
        #[arg(long, default_value = "least_queue")]
        routing: String,
        #[arg(long)]
        preferred_site: Option<String>,
        #[arg(long)]
        residency: Option<String>,
    },
    Info {
        name: String,
    },
    Delete {
        name: String,
    },
}

#[derive(Subcommand)]
enum AiKeyCmd {
    List,
    Create {
        name: String,
        #[arg(long)]
        endpoint: String,
        #[arg(long)]
        model: Option<String>,
        #[arg(long)]
        request_quota: Option<u64>,
    },
    Delete {
        id: String,
    },
}

#[derive(Subcommand)]
enum AiNodeCmd {
    /// List registered inference nodes and their GPUs
    List,
    /// Create a Janus MIG slice record (no nvidia-smi)
    MigCreate {
        /// Inference node id (for example node-0)
        node: String,
        #[arg(long)]
        parent_bdf: String,
        #[arg(long)]
        profile: String,
    },
    /// Delete a Janus MIG slice record
    MigDelete {
        /// Inference node id
        node: String,
        /// Slice BDF (for example janus:node-0:gpu-0--1g.10gb--0)
        bdf: String,
    },
}

// ─── Table row types ─────────────────────────────────────────────────────────

#[derive(Tabled)]
struct VMRow {
    name: String,
    state: String,
    cpus: u32,
    memory: String,
    disk: String,
    image: String,
}

#[derive(Tabled)]
struct ResourceRow {
    id: String,
    name: String,
    status: String,
    info: String,
}

// ─── Helpers ─────────────────────────────────────────────────────────────────

fn parse_label(s: &str) -> Result<(String, String), String> {
    let parts: Vec<&str> = s.splitn(2, '=').collect();
    if parts.len() != 2 {
        return Err(format!("Invalid label format '{}', use key=value", s));
    }
    Ok((parts[0].to_string(), parts[1].to_string()))
}

fn load_config_file(path: &str) -> Result<serde_json::Value> {
    tracing::debug!("Loading config file: {}", path);
    let content =
        std::fs::read_to_string(path).with_context(|| format!("Failed to read file: {}", path))?;
    if path.ends_with(".yaml") || path.ends_with(".yml") {
        tracing::debug!("Parsing as YAML");
        let val: serde_json::Value =
            serde_yaml::from_str(&content).with_context(|| "Failed to parse YAML")?;
        Ok(val)
    } else {
        tracing::debug!("Parsing as JSON");
        let val: serde_json::Value =
            serde_json::from_str(&content).with_context(|| "Failed to parse JSON")?;
        Ok(val)
    }
}

fn format_output<T: Serialize>(val: &T, fmt: OutputFormat) -> Result<String> {
    match fmt {
        OutputFormat::Json => Ok(serde_json::to_string_pretty(val)?),
        OutputFormat::Yaml => Ok(serde_yaml::to_string(val)?),
        OutputFormat::Table => Ok(serde_json::to_string_pretty(val)?),
    }
}

fn print_kv_table(val: &serde_json::Value) {
    if let Some(obj) = val.as_object() {
        let width = obj.keys().map(|k| k.len()).max().unwrap_or(4).max(4);
        for (k, v) in obj {
            let rendered = match v {
                serde_json::Value::String(s) => style::status_cell(s),
                serde_json::Value::Bool(b) => style::status_cell(if *b { "true" } else { "false" }),
                serde_json::Value::Null => style::paint_global(style::DIM, "-"),
                serde_json::Value::Number(n) => n.to_string(),
                other if other.is_array() || other.is_object() => {
                    serde_json::to_string(other).unwrap_or_default()
                }
                other => other.to_string(),
            };
            println!("  {:width$}: {}", k, rendered, width = width);
        }
    } else {
        println!("{}", serde_json::to_string_pretty(val).unwrap_or_default());
    }
}

fn print_value(val: &serde_json::Value, fmt: OutputFormat) {
    match fmt {
        OutputFormat::Json => println!("{}", serde_json::to_string_pretty(val).unwrap_or_default()),
        OutputFormat::Yaml => println!("{}", serde_yaml::to_string(val).unwrap_or_default()),
        OutputFormat::Table => {
            if let Some(items) = val.as_array() {
                print_resources(items, fmt);
            } else if let Some(items) = val.get("items").and_then(|v| v.as_array()) {
                print_resources(items, fmt);
            } else if val.is_object() {
                print_kv_table(val);
            } else {
                println!("{}", serde_json::to_string_pretty(val).unwrap_or_default());
            }
        }
    }
}

fn print_resources(items: &[serde_json::Value], fmt: OutputFormat) {
    match fmt {
        OutputFormat::Table => {
            if items.is_empty() {
                println!("No resources found");
                return;
            }
            let rows: Vec<ResourceRow> = items
                .iter()
                .map(|v| {
                    let status_raw = v
                        .get("enabled")
                        .map(|e| {
                            if e.as_bool().unwrap_or(false) {
                                "active"
                            } else {
                                "disabled"
                            }
                        })
                        .or_else(|| v.get("status").and_then(|s| s.as_str()))
                        .or_else(|| v.get("state").and_then(|s| s.as_str()))
                        .unwrap_or("active");
                    ResourceRow {
                        id: v
                            .get("id")
                            .and_then(|v| v.as_str())
                            .unwrap_or("-")
                            .to_string(),
                        name: v
                            .get("name")
                            .and_then(|v| v.as_str())
                            .unwrap_or("-")
                            .to_string(),
                        status: style::status_cell(status_raw),
                        info: extract_info(v),
                    }
                })
                .collect();
            println!("{}", Table::new(rows));
        }
        OutputFormat::Json => println!(
            "{}",
            serde_json::to_string_pretty(items).unwrap_or_default()
        ),
        OutputFormat::Yaml => println!("{}", serde_yaml::to_string(items).unwrap_or_default()),
    }
}

fn extract_info(v: &serde_json::Value) -> String {
    // Try to build a short info string from common fields
    let mut parts = Vec::new();
    if let Some(t) = v.get("nat_type").and_then(|v| v.as_str()) {
        parts.push(format!("type={}", t));
    }
    if let Some(t) = v.get("topology").and_then(|v| v.as_str()) {
        parts.push(format!("topo={}", t));
    }
    if let Some(t) = v.get("algorithm").and_then(|v| v.as_str()) {
        parts.push(format!("algo={}", t));
    }
    if let Some(t) = v.get("direction").and_then(|v| v.as_str()) {
        parts.push(format!("dir={}", t));
    }
    if let Some(t) = v.get("domain").and_then(|v| v.as_str()) {
        parts.push(format!("domain={}", t));
    }
    if let Some(t) = v.get("priority").and_then(|v| v.as_u64()) {
        parts.push(format!("pri={}", t));
    }
    if let Some(t) = v.get("default_action").and_then(|v| v.as_str()) {
        parts.push(format!("default={}", t));
    }
    if let Some(a) = v.get("rules").and_then(|v| v.as_array()) {
        parts.push(format!("rules={}", a.len()));
    }
    if let Some(a) = v.get("backends").and_then(|v| v.as_array()) {
        parts.push(format!("backends={}", a.len()));
    }
    if let Some(a) = v.get("peers").and_then(|v| v.as_array()) {
        parts.push(format!("peers={}", a.len()));
    }
    if let Some(a) = v.get("thresholds").and_then(|v| v.as_array()) {
        parts.push(format!("thresholds={}", a.len()));
    }
    if parts.is_empty() {
        v.get("description")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string()
    } else {
        parts.join(", ")
    }
}

async fn api_get(client: &Client, path: &str) -> Result<serde_json::Value> {
    tracing::debug!("GET {}{}", api_base(), path);
    let res = client.get(format!("{}{}", api_base(), path)).send().await?;
    tracing::debug!("Response: {}", res.status());
    if !res.status().is_success() {
        let status = res.status();
        let body = res.text().await.unwrap_or_default();
        anyhow::bail!("{}: {}", status, body);
    }
    Ok(res.json().await?)
}

async fn api_post(
    client: &Client,
    path: &str,
    body: &serde_json::Value,
) -> Result<serde_json::Value> {
    tracing::debug!("POST {}{}", api_base(), path);
    let res = client
        .post(format!("{}{}", api_base(), path))
        .json(body)
        .send()
        .await?;
    tracing::debug!("Response: {}", res.status());
    if !res.status().is_success() {
        let status = res.status();
        let body = res.text().await.unwrap_or_default();
        anyhow::bail!("{}: {}", status, body);
    }
    Ok(res.json().await?)
}

async fn dataplane_control(
    client: &Client,
    name: &str,
    action: &str,
    cidr: Option<&str>,
    port: Option<&str>,
    entity: Option<&str>,
    fmt: OutputFormat,
) -> Result<()> {
    let mut body = serde_json::json!({ "action": action });
    if let Some(cidr) = cidr {
        body["cidr"] = serde_json::Value::String(cidr.to_string());
    }
    if let Some(port) = port {
        body["port"] = serde_json::Value::String(port.to_string());
    }
    if let Some(entity) = entity {
        body["entity"] = serde_json::Value::String(entity.to_string());
    }
    let val = api_post(
        client,
        &format!("/vms/{}/dataplane/policy/control", name),
        &body,
    )
    .await?;
    println!("Applied dataplane {action} on '{name}'");
    if !matches!(fmt, OutputFormat::Table) {
        print_value(&val, fmt);
    }
    Ok(())
}

async fn api_post_empty(client: &Client, path: &str) -> Result<serde_json::Value> {
    tracing::debug!("POST {}{} (empty body)", api_base(), path);
    let res = client
        .post(format!("{}{}", api_base(), path))
        .send()
        .await?;
    if !res.status().is_success() {
        let status = res.status();
        let body = res.text().await.unwrap_or_default();
        anyhow::bail!("{}: {}", status, body);
    }
    Ok(res
        .json()
        .await
        .unwrap_or(serde_json::json!({"status": "ok"})))
}

async fn api_put(
    client: &Client,
    path: &str,
    body: &serde_json::Value,
) -> Result<serde_json::Value> {
    tracing::debug!("PUT {}{}", api_base(), path);
    let res = client
        .put(format!("{}{}", api_base(), path))
        .json(body)
        .send()
        .await?;
    if !res.status().is_success() {
        let status = res.status();
        let body = res.text().await.unwrap_or_default();
        anyhow::bail!("{}: {}", status, body);
    }
    Ok(res.json().await?)
}

/// Read a skill directory into API `files` entries. Symlinks are refused so a
/// skill cannot smuggle in files from outside the directory; the server enforces
/// the file-count and size limits.
fn read_skill_dir(root: &std::path::Path) -> Result<Vec<serde_json::Value>> {
    use base64::Engine;
    use std::os::unix::fs::PermissionsExt;

    fn walk(
        root: &std::path::Path,
        dir: &std::path::Path,
        out: &mut Vec<serde_json::Value>,
    ) -> Result<()> {
        let mut entries: Vec<_> = std::fs::read_dir(dir)
            .with_context(|| format!("reading {}", dir.display()))?
            .collect::<std::io::Result<_>>()?;
        entries.sort_by_key(|e| e.file_name());
        for entry in entries {
            let path = entry.path();
            let kind = entry.file_type()?;
            if kind.is_symlink() {
                anyhow::bail!(
                    "{} is a symlink; skills may not contain symlinks",
                    path.display()
                );
            }
            if kind.is_dir() {
                walk(root, &path, out)?;
                continue;
            }
            let rel = path
                .strip_prefix(root)?
                .to_str()
                .context("skill file paths must be valid UTF-8")?
                .to_string();
            let mode = std::fs::metadata(&path)?.permissions().mode();
            out.push(serde_json::json!({
                "path": rel,
                "content_base64": base64::engine::general_purpose::STANDARD
                    .encode(std::fs::read(&path)?),
                "executable": mode & 0o111 != 0,
            }));
        }
        Ok(())
    }

    let mut out = Vec::new();
    walk(root, root, &mut out)?;
    if out.is_empty() {
        anyhow::bail!("{} contains no files", root.display());
    }
    Ok(out)
}

async fn api_delete(client: &Client, path: &str) -> Result<()> {
    tracing::debug!("DELETE {}{}", api_base(), path);
    let res = client
        .delete(format!("{}{}", api_base(), path))
        .send()
        .await?;
    if !res.status().is_success() {
        let status = res.status();
        let body = res.text().await.unwrap_or_default();
        anyhow::bail!("{}: {}", status, body);
    }
    Ok(())
}

async fn api_post_void(client: &Client, path: &str) -> Result<()> {
    tracing::debug!("POST {}{} (void)", api_base(), path);
    let res = client
        .post(format!("{}{}", api_base(), path))
        .send()
        .await?;
    if !res.status().is_success() {
        let status = res.status();
        let body = res.text().await.unwrap_or_default();
        anyhow::bail!("{}: {}", status, body);
    }
    Ok(())
}

fn print_config(
    server: Option<&str>,
    token: Option<&str>,
    color: ColorMode,
    fmt: OutputFormat,
) -> Result<()> {
    let base = resolve_api_base(server);
    let token_set = auth_token(token).is_some();
    let cfg = serde_json::json!({
        "server": base,
        "token_set": token_set,
        "color": format!("{:?}", color).to_ascii_lowercase(),
        "version": env!("CARGO_PKG_VERSION"),
    });
    match fmt {
        OutputFormat::Table => {
            println!("{}", style::heading("⚙️  fabricctl config"));
            println!("  server:    {}", base);
            println!(
                "  token:     {}",
                if token_set {
                    style::paint_global(style::GREEN, "set")
                } else {
                    style::paint_global(style::YELLOW, "unset")
                }
            );
            println!("  color:     {:?}", color);
            println!("  version:   {}", env!("CARGO_PKG_VERSION"));
        }
        _ => println!("{}", format_output(&cfg, fmt)?),
    }
    Ok(())
}

async fn run_status(client: &Client, fmt: OutputFormat, token_set: bool) -> Result<()> {
    let mut ok = true;
    let mut lines: Vec<(String, String)> = Vec::new();

    // API reachability via VM list (lightweight)
    let api_result = client
        .get(format!("{}/vms?limit=1", api_base()))
        .send()
        .await;
    match api_result {
        Ok(res) if res.status().is_success() => {
            lines.push(("Fabric API".into(), "ok".into()));
        }
        Ok(res) if res.status().as_u16() == 401 || res.status().as_u16() == 403 => {
            lines.push(("Fabric API".into(), "reachable (auth required)".into()));
            lines.push(("Auth".into(), "missing or invalid token".into()));
            ok = false;
        }
        Ok(res) => {
            lines.push(("Fabric API".into(), format!("error ({})", res.status())));
            ok = false;
        }
        Err(e) => {
            lines.push(("Fabric API".into(), format!("unreachable ({e})")));
            ok = false;
        }
    }

    if token_set {
        lines.push(("Auth token".into(), "set".into()));
    } else {
        lines.push(("Auth token".into(), "unset".into()));
    }

    match api_get(client, "/dataplane/health").await {
        Ok(val) => {
            let summary = val
                .get("status")
                .or_else(|| val.get("state"))
                .or_else(|| val.get("ok"))
                .map(|v| match v {
                    serde_json::Value::Bool(true) => "ok".to_string(),
                    serde_json::Value::Bool(false) => "degraded".to_string(),
                    serde_json::Value::String(s) => s.clone(),
                    other => other.to_string(),
                })
                .unwrap_or_else(|| "ok".to_string());
            let healthy = summary.to_ascii_lowercase().contains("ok")
                || summary.to_ascii_lowercase().contains("healthy")
                || summary == "true";
            if !healthy {
                ok = false;
            }
            lines.push(("Dataplane".into(), summary));
            if matches!(fmt, OutputFormat::Json | OutputFormat::Yaml) {
                // fall through to structured dump below
                let report = serde_json::json!({
                    "fabric_api": lines.iter().find(|(k, _)| k == "Fabric API").map(|(_, v)| v),
                    "auth_token": if token_set { "set" } else { "unset" },
                    "dataplane": val,
                    "ok": ok,
                });
                print_value(&report, fmt);
                if !ok {
                    anyhow::bail!("fabricctl status: unhealthy");
                }
                return Ok(());
            }
        }
        Err(e) => {
            lines.push(("Dataplane".into(), format!("unavailable ({e})")));
            ok = false;
        }
    }

    match fmt {
        OutputFormat::Table => {
            println!("{}", style::heading("❤️  fabricctl status"));
            for (name, detail) in &lines {
                let lower = detail.to_ascii_lowercase();
                let line = if lower.contains("ok")
                    || lower == "set"
                    || lower.contains("healthy")
                    || lower.contains("reachable (auth")
                {
                    if lower.contains("auth required") || lower.contains("missing") {
                        style::check_warn(&format!("{name}: {detail}"))
                    } else {
                        style::check_ok(&format!("{name}: {detail}"))
                    }
                } else if lower.contains("unset") {
                    style::check_warn(&format!("{name}: {detail}"))
                } else {
                    style::check_fail(&format!("{name}: {detail}"))
                };
                println!("{line}");
            }
        }
        _ => {
            let report = serde_json::json!({
                "checks": lines.iter().map(|(k, v)| serde_json::json!({k: v})).collect::<Vec<_>>(),
                "ok": ok,
            });
            print_value(&report, fmt);
        }
    }

    if !ok {
        anyhow::bail!("fabricctl status: unhealthy");
    }
    Ok(())
}

// ─── Execution ───────────────────────────────────────────────────────────────

impl Cli {
    pub async fn run(self) -> Result<()> {
        style::set_color_enabled(self.color.enabled());
        set_api_base(resolve_api_base(self.server.as_deref()));

        let Some(command) = self.command else {
            crate::help::print_root_help(self.color.enabled());
            return Ok(());
        };

        // Meta commands that do not need an HTTP client
        if let Commands::Completion { shell } = &command {
            let mut cmd = Self::command_with_grouped_help();
            generate(*shell, &mut cmd, "fabricctl", &mut io::stdout());
            return Ok(());
        }
        if matches!(command, Commands::Config) {
            return print_config(
                self.server.as_deref(),
                self.token.as_deref(),
                self.color,
                self.output,
            );
        }

        if server_is_default(self.server.as_deref()) && port_speaks_tls(DEFAULT_ADDR).await {
            set_api_base(DEFAULT_ROOT.replacen("http://", "https://", 1) + "/api");
        }
        let base = api_base();
        let mut headers = reqwest::header::HeaderMap::new();
        if let Some(token) = auth_token(self.token.as_deref()) {
            let value = format!("Bearer {token}");
            headers.insert(
                reqwest::header::AUTHORIZATION,
                value
                    .parse()
                    .context("invalid ZYVOR_FABRIC_TOKEN / FABRIC_TOKEN / --token")?,
            );
        }
        let client = Client::builder()
            .default_headers(headers)
            .danger_accept_invalid_certs(base.starts_with("https://"))
            .build()
            .context("build HTTP client")?;
        let fmt = self.output;
        let color_on = self.color.enabled();

        match command {
            Commands::Status => {
                let token_set = auth_token(self.token.as_deref()).is_some();
                return run_status(&client, fmt, token_set).await;
            }
            Commands::Config | Commands::Completion { .. } => unreachable!(),
            // ── VM Management ────────────────────────────────────────────
            Commands::List => {
                #[derive(serde::Deserialize)]
                struct VmListResponse {
                    items: Vec<VM>,
                }
                let resp: VmListResponse = client
                    .get(format!("{}/vms", api_base()))
                    .send()
                    .await?
                    .error_for_status()
                    .context("list VMs")?
                    .json()
                    .await
                    .context("decode VM list")?;
                let vms = resp.items;

                match fmt {
                    OutputFormat::Table => {
                        if vms.is_empty() {
                            println!("No VMs found");
                            return Ok(());
                        }
                        let rows: Vec<VMRow> = vms
                            .into_iter()
                            .map(|vm| {
                                let state_raw = format!("{:?}", vm.state);
                                VMRow {
                                    name: vm.name,
                                    state: style::status_cell(&state_raw),
                                    cpus: vm.cpus,
                                    memory: format!("{}MB", vm.memory),
                                    disk: format!("{}GB", vm.disk),
                                    image: vm.image,
                                }
                            })
                            .collect();
                        println!("{}", Table::new(rows));
                    }
                    OutputFormat::Json => println!("{}", serde_json::to_string_pretty(&vms)?),
                    OutputFormat::Yaml => println!("{}", serde_yaml::to_string(&vms)?),
                }
            }

            Commands::Info { name } => {
                let vm: VM = client
                    .get(format!("{}/vms/{}", api_base(), name))
                    .send()
                    .await?
                    .json()
                    .await?;

                match fmt {
                    OutputFormat::Table => {
                        let state_raw = format!("{:?}", vm.state);
                        println!("Name:     {}", vm.name);
                        println!("State:    {}", style::status_cell(&state_raw));
                        println!("CPUs:     {}", vm.cpus);
                        println!("Memory:   {}MB", vm.memory);
                        println!("Disk:     {}GB", vm.disk);
                        println!("Image:    {}", vm.image);
                        if let Some(ip) = &vm.ip {
                            println!("IP:       {}", ip);
                        }
                        if let Some(host) = &vm.hostname {
                            println!("Hostname: {}", host);
                        }
                        if let Some(tags) = &vm.tags {
                            println!("Tags:     {}", tags.join(", "));
                        }
                        if let Some(labels) = &vm.labels {
                            let l: Vec<String> =
                                labels.iter().map(|(k, v)| format!("{}={}", k, v)).collect();
                            println!("Labels:   {}", l.join(", "));
                        }
                    }
                    _ => println!("{}", format_output(&vm, fmt)?),
                }
            }

            Commands::Fork {
                name,
                count,
                prefix,
                ready,
                idempotency_key,
            } => {
                let resp = client
                    .post(format!("{}/vms/{}/fork", api_base(), name))
                    .json(&serde_json::json!({
                        "count": count,
                        "name_prefix": prefix,
                        "ready": ready,
                        "idempotency_key": idempotency_key,
                    }))
                    .send()
                    .await?;
                if !resp.status().is_success() {
                    let status = resp.status();
                    let body = resp.text().await.unwrap_or_default();
                    anyhow::bail!("fork: {status}: {body}");
                }
                let out: serde_json::Value = resp.json().await?;
                match fmt {
                    OutputFormat::Table => print_fork(&out),
                    _ => println!("{}", format_output(&out, fmt)?),
                }
            }

            Commands::Memory { name } => {
                let mem: serde_json::Value = client
                    .get(format!("{}/vms/{}/memory", api_base(), name))
                    .send()
                    .await?
                    .error_for_status()
                    .context("get VM memory")?
                    .json()
                    .await?;
                match fmt {
                    OutputFormat::Table => print_memory(&mem),
                    _ => println!("{}", format_output(&mem, fmt)?),
                }
            }

            Commands::Balloon { name, mib } => {
                let url = format!("{}/vms/{}/balloon", api_base(), name);
                let req = match mib {
                    Some(m) => client
                        .post(url)
                        .json(&serde_json::json!({ "balloon_mib": m })),
                    None => client.get(url),
                };
                let resp = req.send().await?;
                if !resp.status().is_success() {
                    let status = resp.status();
                    let body = resp.text().await.unwrap_or_default();
                    anyhow::bail!("balloon: {status}: {body}");
                }
                let b: serde_json::Value = resp.json().await?;
                match fmt {
                    OutputFormat::Table => println!(
                        "balloon: target {} MiB, reached {} MiB of {} MiB",
                        b["target_mib"], b["actual_mib"], b["memory_mib"]
                    ),
                    _ => println!("{}", format_output(&b, fmt)?),
                }
            }

            Commands::Create {
                name,
                image,
                cpus,
                memory,
                disk,
                hostname,
                tags,
                label,
                tenant,
                direct_uplink,
                direct_mode,
                direct_guest_ip,
            } => {
                let labels =
                    label.map(|pairs| pairs.into_iter().collect::<HashMap<String, String>>());
                let req = CreateVMRequest {
                    name: name.clone(),
                    image,
                    cpus,
                    memory,
                    disk,
                    hostname,
                    tags,
                    labels,
                    tenant,
                    port_forwards: Vec::new(),
                    network_tap: false,
                    network_static_ip: false,
                    direct_uplink,
                    direct_mode,
                    direct_guest_ips: direct_guest_ip,
                    storage: None,
                    enable_qga: false,
                    hyperv: false,
                };
                let vm: VM = client
                    .post(format!("{}/vms", api_base()))
                    .json(&req)
                    .send()
                    .await?
                    .json()
                    .await?;
                match fmt {
                    OutputFormat::Table => println!("VM '{}' created successfully", name),
                    _ => println!("{}", format_output(&vm, fmt)?),
                }
            }

            Commands::DirectUplink {
                name,
                uplink,
                direct_mode,
                direct_guest_ip,
            } => {
                let mut body = serde_json::json!({ "uplink": uplink });
                if let Some(mode) = direct_mode {
                    body["mode"] = serde_json::json!(mode);
                }
                if !direct_guest_ip.is_empty() {
                    body["guest_ips"] = serde_json::json!(direct_guest_ip);
                }
                let vm: VM = client
                    .put(format!("{}/vms/{}/direct-uplink", api_base(), name))
                    .json(&body)
                    .send()
                    .await?
                    .json()
                    .await?;
                match fmt {
                    OutputFormat::Table => {
                        println!(
                            "VM '{}' direct uplink set to {}",
                            name,
                            vm.direct_uplink.as_deref().unwrap_or(&uplink)
                        );
                    }
                    _ => println!("{}", format_output(&vm, fmt)?),
                }
            }

            Commands::Start { name } => {
                client
                    .post(format!("{}/vms/{}/start", api_base(), name))
                    .send()
                    .await?;
                println!("VM '{}' started", name);
            }

            Commands::Stop { name } => {
                client
                    .post(format!("{}/vms/{}/stop", api_base(), name))
                    .send()
                    .await?;
                println!("VM '{}' stopped", name);
            }

            Commands::Restart { name } => {
                client
                    .post(format!("{}/vms/{}/restart", api_base(), name))
                    .send()
                    .await?;
                println!("VM '{}' restarted", name);
            }

            Commands::Delete { name } => {
                client
                    .delete(format!("{}/vms/{}", api_base(), name))
                    .send()
                    .await?;
                println!("VM '{}' deleted", name);
            }

            Commands::Metrics { name } => {
                let val = api_get(&client, &format!("/vms/{}/metrics", name)).await?;
                print_value(&val, fmt);
            }

            Commands::Dataplane(cmd) => match cmd {
                DataplaneCmd::Status { name } => {
                    let val = api_get(&client, &format!("/vms/{}/dataplane/status", name)).await?;
                    print_value(&val, fmt);
                }
                DataplaneCmd::Policy(pol) => match pol {
                    DataplanePolicyCmd::Get { name } => {
                        let val =
                            api_get(&client, &format!("/vms/{}/dataplane/policy", name)).await?;
                        print_value(&val, fmt);
                    }
                    DataplanePolicyCmd::Set { name, file } => {
                        let policy = load_config_file(&file)?;
                        let val =
                            api_post(&client, &format!("/vms/{}/dataplane/policy", name), &policy)
                                .await?;
                        println!("Updated dataplane policy for '{}'", name);
                        if !matches!(fmt, OutputFormat::Table) {
                            print_value(&val, fmt);
                        }
                    }
                    DataplanePolicyCmd::Guard { name } => {
                        dataplane_control(&client, &name, "guard", None, None, None, fmt).await?;
                    }
                    DataplanePolicyCmd::Audit { name } => {
                        dataplane_control(&client, &name, "audit", None, None, None, fmt).await?;
                    }
                    DataplanePolicyCmd::Open { name } => {
                        dataplane_control(&client, &name, "open", None, None, None, fmt).await?;
                    }
                    DataplanePolicyCmd::Invert { name } => {
                        dataplane_control(&client, &name, "invert", None, None, None, fmt).await?;
                    }
                    DataplanePolicyCmd::Block { name, cidr } => {
                        dataplane_control(&client, &name, "block", Some(&cidr), None, None, fmt)
                            .await?;
                    }
                    DataplanePolicyCmd::Allow {
                        name,
                        cidr,
                        port,
                        entity,
                    } => {
                        dataplane_control(
                            &client,
                            &name,
                            "allow",
                            cidr.as_deref(),
                            port.as_deref(),
                            entity.as_deref(),
                            fmt,
                        )
                        .await?;
                    }
                },
                DataplaneCmd::Stats { name } => {
                    let val = api_get(&client, &format!("/vms/{}/dataplane/stats", name)).await?;
                    print_value(&val, fmt);
                }
                DataplaneCmd::Flows { name, limit } => {
                    let val = api_get(
                        &client,
                        &format!("/vms/{}/dataplane/flows?limit={}", name, limit),
                    )
                    .await?;
                    print_value(&val, fmt);
                }
                DataplaneCmd::Effective { name } => {
                    let val =
                        api_get(&client, &format!("/vms/{}/dataplane/effective", name)).await?;
                    print_value(&val, fmt);
                }
                DataplaneCmd::Explain {
                    name,
                    dest,
                    port,
                    proto,
                } => {
                    let val = api_get(
                        &client,
                        &format!(
                            "/vms/{}/dataplane/explain?dest={}&port={}&proto={}",
                            name, dest, port, proto
                        ),
                    )
                    .await?;
                    print_value(&val, fmt);
                }
                DataplaneCmd::DryRun { name, limit } => {
                    let val = api_get(
                        &client,
                        &format!("/vms/{}/dataplane/dry-run?limit={}", name, limit),
                    )
                    .await?;
                    print_value(&val, fmt);
                }
                DataplaneCmd::Health => {
                    let val = api_get(&client, "/dataplane/health").await?;
                    print_value(&val, fmt);
                }
                DataplaneCmd::Ipcache => {
                    let val = api_get(&client, "/dataplane/ipcache").await?;
                    print_value(&val, fmt);
                }
                DataplaneCmd::RefreshDns => {
                    let val = api_post_empty(&client, "/dataplane/refresh-dns").await?;
                    print_value(&val, fmt);
                }
                DataplaneCmd::Observe => {
                    let val = api_get(&client, "/dataplane/observe").await?;
                    print_value(&val, fmt);
                }
                DataplaneCmd::Hubble { style, limit } => {
                    let val = api_get(&client, &format!("/dataplane/hubble/flows?limit={}", limit))
                        .await?;
                    let style = if style.eq_ignore_ascii_case("auto") {
                        crate::packetflow::default_hubble_style(color_on).to_string()
                    } else {
                        style
                    };
                    // Global `-o json` also forces JSON (same as --style json).
                    if matches!(fmt, OutputFormat::Json) || style.eq_ignore_ascii_case("json") {
                        print_value(&val, OutputFormat::Json);
                    } else {
                        println!("{}", crate::packetflow::render_hubble_json(&val, &style));
                    }
                }
                DataplaneCmd::Identities => {
                    let val = api_get(&client, "/dataplane/identities").await?;
                    print_value(&val, fmt);
                }
                DataplaneCmd::Endpoints => {
                    let val = api_get(&client, "/dataplane/endpoints").await?;
                    print_value(&val, fmt);
                }
                DataplaneCmd::Service(s) => match s {
                    DataplaneServiceCmd::List => {
                        let val = api_get(&client, "/dataplane/services").await?;
                        print_value(&val, fmt);
                    }
                    DataplaneServiceCmd::Get { name } => {
                        let val =
                            api_get(&client, &format!("/dataplane/services/{}", name)).await?;
                        print_value(&val, fmt);
                    }
                    DataplaneServiceCmd::Status => {
                        let val = api_get(&client, "/dataplane/services/status").await?;
                        print_value(&val, fmt);
                    }
                    DataplaneServiceCmd::Stats => {
                        let val = api_get(&client, "/dataplane/services/stats").await?;
                        print_value(&val, fmt);
                    }
                    DataplaneServiceCmd::Health => {
                        let val = api_get(&client, "/dataplane/services/health").await?;
                        print_value(&val, fmt);
                    }
                    DataplaneServiceCmd::Reconcile => {
                        let val =
                            api_post_empty(&client, "/dataplane/services/health/reconcile").await?;
                        println!("Reconciled dataplane service health");
                        if !matches!(fmt, OutputFormat::Table) {
                            print_value(&val, fmt);
                        }
                    }
                    DataplaneServiceCmd::Gc => {
                        let val =
                            api_post_empty(&client, "/dataplane/services/conntrack/gc").await?;
                        println!("Garbage-collected service conntrack");
                        if !matches!(fmt, OutputFormat::Table) {
                            print_value(&val, fmt);
                        }
                    }
                    DataplaneServiceCmd::Advertisements => {
                        let val = api_get(&client, "/dataplane/services/advertisements").await?;
                        print_value(&val, fmt);
                    }
                    DataplaneServiceCmd::Flows { limit } => {
                        let val = api_get(
                            &client,
                            &format!("/dataplane/services/flows?limit={}", limit),
                        )
                        .await?;
                        print_value(&val, fmt);
                    }
                    DataplaneServiceCmd::ExportTelemetry { limit } => {
                        let val = api_post_empty(
                            &client,
                            &format!("/dataplane/services/telemetry/export?limit={}", limit),
                        )
                        .await?;
                        println!("Exported dataplane service telemetry");
                        if !matches!(fmt, OutputFormat::Table) {
                            print_value(&val, fmt);
                        }
                    }
                    DataplaneServiceCmd::Delta {
                        name,
                        after_seq,
                        max_entries,
                    } => {
                        let val = api_get(
                            &client,
                            &format!(
                                "/dataplane/services/{}/conntrack/delta?after_seq={}&max_entries={}",
                                name, after_seq, max_entries
                            ),
                        )
                        .await?;
                        print_value(&val, fmt);
                    }
                    DataplaneServiceCmd::Apply { file } => {
                        let service = load_config_file(&file)?;
                        let val = api_post(&client, "/dataplane/services", &service).await?;
                        println!("Upserted dataplane service");
                        if !matches!(fmt, OutputFormat::Table) {
                            print_value(&val, fmt);
                        }
                    }
                    DataplaneServiceCmd::Delete { name } => {
                        api_delete(&client, &format!("/dataplane/services/{}", name)).await?;
                        println!("Deleted dataplane service '{}'", name);
                    }
                },
                DataplaneCmd::Group(g) => match g {
                    DataplaneGroupCmd::List => {
                        let val = api_get(&client, "/dataplane/groups").await?;
                        print_value(&val, fmt);
                    }
                    DataplaneGroupCmd::Get { name } => {
                        let val = api_get(&client, &format!("/dataplane/groups/{}", name)).await?;
                        print_value(&val, fmt);
                    }
                    DataplaneGroupCmd::Create { file } => {
                        let group = load_config_file(&file)?;
                        let val = api_post(&client, "/dataplane/groups", &group).await?;
                        println!("Upserted dataplane group");
                        if !matches!(fmt, OutputFormat::Table) {
                            print_value(&val, fmt);
                        }
                    }
                    DataplaneGroupCmd::Delete { name } => {
                        api_delete(&client, &format!("/dataplane/groups/{}", name)).await?;
                        println!("Deleted dataplane group '{}'", name);
                    }
                },
                DataplaneCmd::Cnp(c) => match c {
                    DataplaneCnpCmd::List => {
                        let val = api_get(&client, "/dataplane/cnp").await?;
                        print_value(&val, fmt);
                    }
                    DataplaneCnpCmd::Get { name } => {
                        let val = api_get(&client, &format!("/dataplane/cnp/{}", name)).await?;
                        print_value(&val, fmt);
                    }
                    DataplaneCnpCmd::Apply { file } => {
                        let doc = load_config_file(&file)?;
                        let val = api_post(&client, "/dataplane/cnp", &doc).await?;
                        println!("Applied dataplane CNP");
                        if !matches!(fmt, OutputFormat::Table) {
                            print_value(&val, fmt);
                        }
                    }
                    DataplaneCnpCmd::Delete { name } => {
                        api_delete(&client, &format!("/dataplane/cnp/{}", name)).await?;
                        println!("Deleted dataplane CNP '{}'", name);
                    }
                },
            },

            // ── Apply / Export ────────────────────────────────────────────
            Commands::Apply { file } => {
                let config = load_config_file(&file)?;
                let kind = config.get("kind").and_then(|v| v.as_str())
                    .context("Config must have a 'kind' field (vm, firewall-profile, network-policy, service, qos-policy, dns-zone, dns-policy, vpn-tunnel, vpn-network, mirror-session, nat-rule, nat-pool, nat-gateway, monitor-policy, bridge, vlan, bond, port-forward)")?;

                let spec = config.get("spec").unwrap_or(&config);
                let (method, path) = match kind {
                    "vm" => ("POST", "/vms"),
                    "firewall-profile" => ("POST", "/firewall-profiles"),
                    "firewall-zone" => ("POST", "/firewall-zones"),
                    "network-policy" => ("POST", "/network-policies"),
                    "service" => ("POST", "/services"),
                    "qos-policy" => ("POST", "/qos-policies"),
                    "dns-zone" => ("POST", "/dns-zones"),
                    "dns-policy" => ("POST", "/dns-policies"),
                    "vpn-tunnel" => ("POST", "/vpn-tunnels"),
                    "vpn-network" => ("POST", "/vpn-networks"),
                    "mirror-session" => ("POST", "/mirror-sessions"),
                    "nat-rule" => ("POST", "/nat-rules"),
                    "nat-pool" => ("POST", "/nat-pools"),
                    "nat-gateway" => ("POST", "/nat-gateways"),
                    "monitor-policy" => ("POST", "/monitor-policies"),
                    "ceph-pool" => ("POST", "/storage/pools/ceph"),
                    "bridge" => ("POST", "/networkd/bridges"),
                    "vlan" => ("POST", "/networkd/vlans"),
                    "bond" => ("POST", "/networkd/bonds"),
                    "port-forward" => ("POST", "/networkd/port-forwards"),
                    "model-artifact" | "ModelArtifact" => ("POST", "/ai/models"),
                    "inference-profile" | "InferenceProfile" => ("POST", "/ai/profiles"),
                    "inference-deployment" | "InferenceDeployment" => ("POST", "/ai/deployments"),
                    "inference-endpoint" | "InferenceEndpoint" => ("POST", "/ai/endpoints"),
                    _ => anyhow::bail!("Unknown resource kind: {}", kind),
                };

                let result = if method == "POST" {
                    api_post(&client, path, spec).await?
                } else {
                    api_put(&client, path, spec).await?
                };
                println!("Applied {} successfully", kind);
                if !matches!(fmt, OutputFormat::Table) {
                    print_value(&result, fmt);
                }
            }

            Commands::Export { resource } => {
                let path = match resource.as_str() {
                    "vm" | "vms" => "/vms",
                    "firewall" | "firewall-profiles" => "/firewall-profiles",
                    "firewall-zones" => "/firewall-zones",
                    "network-policy" | "network-policies" => "/network-policies",
                    "service" | "services" => "/services",
                    "qos" | "qos-policies" => "/qos-policies",
                    "dns-zone" | "dns-zones" => "/dns-zones",
                    "dns-policy" | "dns-policies" => "/dns-policies",
                    "vpn-tunnel" | "vpn-tunnels" => "/vpn-tunnels",
                    "vpn-network" | "vpn-networks" => "/vpn-networks",
                    "mirror" | "mirror-sessions" => "/mirror-sessions",
                    "nat-rule" | "nat-rules" => "/nat-rules",
                    "nat-pool" | "nat-pools" => "/nat-pools",
                    "nat-gateway" | "nat-gateways" => "/nat-gateways",
                    "monitor" | "monitor-policies" => "/monitor-policies",
                    "bridge" | "bridges" => "/networkd/bridges",
                    "vlan" | "vlans" => "/networkd/vlans",
                    "bond" | "bonds" => "/networkd/bonds",
                    "port-forward" | "port-forwards" => "/networkd/port-forwards",
                    _ => anyhow::bail!("Unknown resource: {}", resource),
                };
                let val = api_get(&client, path).await?;
                print_value(&val, fmt);
            }

            // ── Network Policies ─────────────────────────────────────────
            Commands::Policy(cmd) => match cmd {
                PolicyCmd::List => {
                    let val = api_get(&client, "/network-policies").await?;
                    print_resources(val.as_array().unwrap_or(&vec![]), fmt);
                }
                PolicyCmd::Get { id } => {
                    let val = api_get(&client, &format!("/network-policies/{}", id)).await?;
                    print_value(&val, fmt);
                }
                PolicyCmd::Create { file } => {
                    let body = load_config_file(&file)?;
                    let val = api_post(&client, "/network-policies", &body).await?;
                    println!("Network policy created");
                    if !matches!(fmt, OutputFormat::Table) {
                        print_value(&val, fmt);
                    }
                }
                PolicyCmd::Delete { id } => {
                    api_delete(&client, &format!("/network-policies/{}", id)).await?;
                    println!("Network policy '{}' deleted", id);
                }
                PolicyCmd::Sync => {
                    let val = api_post_empty(&client, "/network-policies/sync").await?;
                    print_value(&val, fmt);
                }
                PolicyCmd::Status => {
                    let val = api_get(&client, "/network-policies/status").await?;
                    print_value(&val, fmt);
                }
            },

            // ── Firewall ─────────────────────────────────────────────────
            Commands::Firewall(cmd) => match cmd {
                FirewallCmd::List => {
                    let val = api_get(&client, "/firewall-profiles").await?;
                    print_resources(val.as_array().unwrap_or(&vec![]), fmt);
                }
                FirewallCmd::Get { id } => {
                    let val = api_get(&client, &format!("/firewall-profiles/{}", id)).await?;
                    print_value(&val, fmt);
                }
                FirewallCmd::Create { file } => {
                    let body = load_config_file(&file)?;
                    let val = api_post(&client, "/firewall-profiles", &body).await?;
                    println!("Firewall profile created");
                    if !matches!(fmt, OutputFormat::Table) {
                        print_value(&val, fmt);
                    }
                }
                FirewallCmd::Delete { id } => {
                    api_delete(&client, &format!("/firewall-profiles/{}", id)).await?;
                    println!("Firewall profile '{}' deleted", id);
                }
                FirewallCmd::Zones => {
                    let val = api_get(&client, "/firewall-zones").await?;
                    print_resources(val.as_array().unwrap_or(&vec![]), fmt);
                }
                FirewallCmd::Assign { vm, profile } => {
                    let body = serde_json::json!({ "profile_id": profile });
                    api_put(&client, &format!("/vms/{}/firewall", vm), &body).await?;
                    println!("Firewall profile '{}' assigned to VM '{}'", profile, vm);
                }
                FirewallCmd::Unassign { vm } => {
                    api_delete(&client, &format!("/vms/{}/firewall", vm)).await?;
                    println!("Firewall removed from VM '{}'", vm);
                }
                FirewallCmd::Sync => {
                    let val = api_post_empty(&client, "/firewall/sync").await?;
                    print_value(&val, fmt);
                }
                FirewallCmd::Status => {
                    let val = api_get(&client, "/firewall/status").await?;
                    print_value(&val, fmt);
                }
            },

            // ── Service Mesh ─────────────────────────────────────────────
            Commands::Service(cmd) => match cmd {
                ServiceCmd::List => {
                    let val = api_get(&client, "/services").await?;
                    print_resources(val.as_array().unwrap_or(&vec![]), fmt);
                }
                ServiceCmd::Get { id } => {
                    let val = api_get(&client, &format!("/services/{}", id)).await?;
                    print_value(&val, fmt);
                }
                ServiceCmd::Create { file } => {
                    let body = load_config_file(&file)?;
                    let val = api_post(&client, "/services", &body).await?;
                    println!("Service created");
                    if !matches!(fmt, OutputFormat::Table) {
                        print_value(&val, fmt);
                    }
                }
                ServiceCmd::Delete { id } => {
                    api_delete(&client, &format!("/services/{}", id)).await?;
                    println!("Service '{}' deleted", id);
                }
                ServiceCmd::Backends { id } => {
                    let val = api_get(&client, &format!("/services/{}/backends", id)).await?;
                    print_value(&val, fmt);
                }
                ServiceCmd::Sync => {
                    let val = api_post_empty(&client, "/services/sync").await?;
                    print_value(&val, fmt);
                }
                ServiceCmd::Status => {
                    let val = api_get(&client, "/services/status").await?;
                    print_value(&val, fmt);
                }
            },

            // ── QoS ─────────────────────────────────────────────────────
            Commands::Qos(cmd) => match cmd {
                QosCmd::List => {
                    let val = api_get(&client, "/qos-policies").await?;
                    print_resources(val.as_array().unwrap_or(&vec![]), fmt);
                }
                QosCmd::Get { id } => {
                    let val = api_get(&client, &format!("/qos-policies/{}", id)).await?;
                    print_value(&val, fmt);
                }
                QosCmd::Create { file } => {
                    let body = load_config_file(&file)?;
                    let val = api_post(&client, "/qos-policies", &body).await?;
                    println!("QoS policy created");
                    if !matches!(fmt, OutputFormat::Table) {
                        print_value(&val, fmt);
                    }
                }
                QosCmd::Delete { id } => {
                    api_delete(&client, &format!("/qos-policies/{}", id)).await?;
                    println!("QoS policy '{}' deleted", id);
                }
                QosCmd::Sync => {
                    let val = api_post_empty(&client, "/qos-policies/sync").await?;
                    print_value(&val, fmt);
                }
                QosCmd::Status => {
                    let val = api_get(&client, "/qos-policies/status").await?;
                    print_value(&val, fmt);
                }
            },

            // ── DNS ──────────────────────────────────────────────────────
            Commands::Dns(cmd) => match cmd {
                DnsCmd::Zones => {
                    let val = api_get(&client, "/dns-zones").await?;
                    print_resources(val.as_array().unwrap_or(&vec![]), fmt);
                }
                DnsCmd::Policies => {
                    let val = api_get(&client, "/dns-policies").await?;
                    print_resources(val.as_array().unwrap_or(&vec![]), fmt);
                }
                DnsCmd::GetZone { id } => {
                    let val = api_get(&client, &format!("/dns-zones/{}", id)).await?;
                    print_value(&val, fmt);
                }
                DnsCmd::CreateZone { file } => {
                    let body = load_config_file(&file)?;
                    let val = api_post(&client, "/dns-zones", &body).await?;
                    println!("DNS zone created");
                    if !matches!(fmt, OutputFormat::Table) {
                        print_value(&val, fmt);
                    }
                }
                DnsCmd::DeleteZone { id } => {
                    api_delete(&client, &format!("/dns-zones/{}", id)).await?;
                    println!("DNS zone '{}' deleted", id);
                }
                DnsCmd::CreatePolicy { file } => {
                    let body = load_config_file(&file)?;
                    let val = api_post(&client, "/dns-policies", &body).await?;
                    println!("DNS policy created");
                    if !matches!(fmt, OutputFormat::Table) {
                        print_value(&val, fmt);
                    }
                }
                DnsCmd::DeletePolicy { id } => {
                    api_delete(&client, &format!("/dns-policies/{}", id)).await?;
                    println!("DNS policy '{}' deleted", id);
                }
                DnsCmd::Records => {
                    let val = api_get(&client, "/dns-records").await?;
                    print_value(&val, fmt);
                }
                DnsCmd::Sync => {
                    let val = api_post_empty(&client, "/dns-policies/sync").await?;
                    print_value(&val, fmt);
                }
            },

            // ── VPN ──────────────────────────────────────────────────────
            Commands::Vpn(cmd) => match cmd {
                VpnCmd::Tunnels => {
                    let val = api_get(&client, "/vpn-tunnels").await?;
                    print_resources(val.as_array().unwrap_or(&vec![]), fmt);
                }
                VpnCmd::Networks => {
                    let val = api_get(&client, "/vpn-networks").await?;
                    print_resources(val.as_array().unwrap_or(&vec![]), fmt);
                }
                VpnCmd::GetTunnel { id } => {
                    let val = api_get(&client, &format!("/vpn-tunnels/{}", id)).await?;
                    print_value(&val, fmt);
                }
                VpnCmd::CreateTunnel { file } => {
                    let body = load_config_file(&file)?;
                    let val = api_post(&client, "/vpn-tunnels", &body).await?;
                    println!("VPN tunnel created");
                    if !matches!(fmt, OutputFormat::Table) {
                        print_value(&val, fmt);
                    }
                }
                VpnCmd::DeleteTunnel { id } => {
                    api_delete(&client, &format!("/vpn-tunnels/{}", id)).await?;
                    println!("VPN tunnel '{}' deleted", id);
                }
                VpnCmd::CreateNetwork { file } => {
                    let body = load_config_file(&file)?;
                    let val = api_post(&client, "/vpn-networks", &body).await?;
                    println!("VPN network created");
                    if !matches!(fmt, OutputFormat::Table) {
                        print_value(&val, fmt);
                    }
                }
                VpnCmd::DeleteNetwork { id } => {
                    api_delete(&client, &format!("/vpn-networks/{}", id)).await?;
                    println!("VPN network '{}' deleted", id);
                }
                VpnCmd::Sync => {
                    let val = api_post_empty(&client, "/vpn-tunnels/sync").await?;
                    print_value(&val, fmt);
                }
                VpnCmd::Status => {
                    let val = api_get(&client, "/vpn-tunnels/status").await?;
                    print_value(&val, fmt);
                }
            },

            // ── Mirror ───────────────────────────────────────────────────
            Commands::Mirror(cmd) => match cmd {
                MirrorCmd::List => {
                    let val = api_get(&client, "/mirror-sessions").await?;
                    print_resources(val.as_array().unwrap_or(&vec![]), fmt);
                }
                MirrorCmd::Get { id } => {
                    let val = api_get(&client, &format!("/mirror-sessions/{}", id)).await?;
                    print_value(&val, fmt);
                }
                MirrorCmd::Create { file } => {
                    let body = load_config_file(&file)?;
                    let val = api_post(&client, "/mirror-sessions", &body).await?;
                    println!("Mirror session created");
                    if !matches!(fmt, OutputFormat::Table) {
                        print_value(&val, fmt);
                    }
                }
                MirrorCmd::Delete { id } => {
                    api_delete(&client, &format!("/mirror-sessions/{}", id)).await?;
                    println!("Mirror session '{}' deleted", id);
                }
                MirrorCmd::Sync => {
                    let val = api_post_empty(&client, "/mirror-sessions/sync").await?;
                    print_value(&val, fmt);
                }
                MirrorCmd::Status => {
                    let val = api_get(&client, "/mirror-sessions/status").await?;
                    print_value(&val, fmt);
                }
            },

            // ── Agent approvals & audit ─────────────────────────────────
            Commands::Approval(cmd) => match cmd {
                ApprovalCmd::List => {
                    let val = api_get(&client, "/approvals").await?;
                    let items = val.get("items").and_then(|v| v.as_array());
                    print_resources(items.unwrap_or(&vec![]), fmt);
                }
                ApprovalCmd::Approve { id, comment, scope } => {
                    let body = serde_json::json!({
                        "decision": "approved",
                        "comment": comment,
                        "scope": scope,
                    });
                    let val = api_post(&client, &format!("/approvals/{}", id), &body).await?;
                    println!("Approval '{}' approved", id);
                    if !matches!(fmt, OutputFormat::Table) {
                        print_value(&val, fmt);
                    }
                }
                ApprovalCmd::Deny { id, comment } => {
                    let body = serde_json::json!({"decision": "denied", "comment": comment});
                    let val = api_post(&client, &format!("/approvals/{}", id), &body).await?;
                    println!("Approval '{}' denied", id);
                    if !matches!(fmt, OutputFormat::Table) {
                        print_value(&val, fmt);
                    }
                }
            },
            Commands::Skill(cmd) => match cmd {
                SkillCmd::List => {
                    let val = api_get(&client, "/skills").await?;
                    let items = val.get("items").and_then(|v| v.as_array());
                    print_resources(items.unwrap_or(&vec![]), fmt);
                }
                SkillCmd::Show { name } => {
                    let val = api_get(&client, &format!("/skills/{}", name)).await?;
                    print_value(&val, fmt);
                }
                SkillCmd::Publish {
                    dir,
                    name,
                    description,
                    scope,
                } => {
                    let files = read_skill_dir(std::path::Path::new(&dir))?;
                    let name = match name {
                        Some(n) => n,
                        None => std::fs::canonicalize(&dir)?
                            .file_name()
                            .and_then(|n| n.to_str())
                            .context("cannot derive a skill name from the directory; pass --name")?
                            .to_string(),
                    };
                    let body = serde_json::json!({
                        "name": name,
                        "description": description,
                        "scope": scope,
                        "files": files,
                    });
                    let val = api_post(&client, "/skills", &body).await?;
                    println!(
                        "Skill '{}' published as version {}",
                        name,
                        val.get("version").and_then(|v| v.as_str()).unwrap_or("?")
                    );
                    if !matches!(fmt, OutputFormat::Table) {
                        print_value(&val, fmt);
                    }
                }
                SkillCmd::Delete { name } => {
                    api_delete(&client, &format!("/skills/{}", name)).await?;
                    println!("Skill '{}' deleted", name);
                }
            },
            Commands::AgentAudit { session, limit } => {
                let mut path = format!("/audit/agent-actions?limit={}", limit);
                if let Some(session) = session {
                    path.push_str(&format!("&session_id={}", session));
                }
                let val = api_get(&client, &path).await?;
                let items = val.get("items").and_then(|v| v.as_array());
                print_resources(items.unwrap_or(&vec![]), fmt);
                if let Some(chain) = val.get("chain") {
                    if chain.get("chain_ok") == Some(&serde_json::Value::Bool(false)) {
                        eprintln!(
                            "WARNING: audit chain verification FAILED at entry {}",
                            chain.get("broken_at").unwrap_or(&serde_json::Value::Null)
                        );
                    }
                }
            }

            // ── NAT ─────────────────────────────────────────────────────
            Commands::Nat(cmd) => match cmd {
                NatCmd::Rules => {
                    let val = api_get(&client, "/nat-rules").await?;
                    print_resources(val.as_array().unwrap_or(&vec![]), fmt);
                }
                NatCmd::Pools => {
                    let val = api_get(&client, "/nat-pools").await?;
                    print_resources(val.as_array().unwrap_or(&vec![]), fmt);
                }
                NatCmd::Gateways => {
                    let val = api_get(&client, "/nat-gateways").await?;
                    print_resources(val.as_array().unwrap_or(&vec![]), fmt);
                }
                NatCmd::GetRule { id } => {
                    let val = api_get(&client, &format!("/nat-rules/{}", id)).await?;
                    print_value(&val, fmt);
                }
                NatCmd::CreateRule { file } => {
                    let body = load_config_file(&file)?;
                    let val = api_post(&client, "/nat-rules", &body).await?;
                    println!("NAT rule created");
                    if !matches!(fmt, OutputFormat::Table) {
                        print_value(&val, fmt);
                    }
                }
                NatCmd::DeleteRule { id } => {
                    api_delete(&client, &format!("/nat-rules/{}", id)).await?;
                    println!("NAT rule '{}' deleted", id);
                }
                NatCmd::CreatePool { file } => {
                    let body = load_config_file(&file)?;
                    let val = api_post(&client, "/nat-pools", &body).await?;
                    println!("NAT pool created");
                    if !matches!(fmt, OutputFormat::Table) {
                        print_value(&val, fmt);
                    }
                }
                NatCmd::DeletePool { id } => {
                    api_delete(&client, &format!("/nat-pools/{}", id)).await?;
                    println!("NAT pool '{}' deleted", id);
                }
                NatCmd::CreateGateway { file } => {
                    let body = load_config_file(&file)?;
                    let val = api_post(&client, "/nat-gateways", &body).await?;
                    println!("NAT gateway created");
                    if !matches!(fmt, OutputFormat::Table) {
                        print_value(&val, fmt);
                    }
                }
                NatCmd::DeleteGateway { id } => {
                    api_delete(&client, &format!("/nat-gateways/{}", id)).await?;
                    println!("NAT gateway '{}' deleted", id);
                }
                NatCmd::Sync => {
                    let val = api_post_empty(&client, "/nat-rules/sync").await?;
                    print_value(&val, fmt);
                }
                NatCmd::Status => {
                    let val = api_get(&client, "/nat-rules/status").await?;
                    print_value(&val, fmt);
                }
            },

            // ── Monitor ──────────────────────────────────────────────────
            Commands::Monitor(cmd) => match cmd {
                MonitorCmd::List => {
                    let val = api_get(&client, "/monitor-policies").await?;
                    print_resources(val.as_array().unwrap_or(&vec![]), fmt);
                }
                MonitorCmd::Get { id } => {
                    let val = api_get(&client, &format!("/monitor-policies/{}", id)).await?;
                    print_value(&val, fmt);
                }
                MonitorCmd::Create { file } => {
                    let body = load_config_file(&file)?;
                    let val = api_post(&client, "/monitor-policies", &body).await?;
                    println!("Monitor policy created");
                    if !matches!(fmt, OutputFormat::Table) {
                        print_value(&val, fmt);
                    }
                }
                MonitorCmd::Delete { id } => {
                    api_delete(&client, &format!("/monitor-policies/{}", id)).await?;
                    println!("Monitor policy '{}' deleted", id);
                }
                MonitorCmd::Metrics => {
                    let val = api_get(&client, "/network-metrics").await?;
                    print_value(&val, fmt);
                }
                MonitorCmd::VmMetrics { name } => {
                    let val = api_get(&client, &format!("/network-metrics/{}", name)).await?;
                    print_value(&val, fmt);
                }
                MonitorCmd::Alerts => {
                    let val = api_get(&client, "/bandwidth-alerts").await?;
                    print_value(&val, fmt);
                }
                MonitorCmd::Sync => {
                    let val = api_post_empty(&client, "/monitor-policies/sync").await?;
                    print_value(&val, fmt);
                }
                MonitorCmd::Status => {
                    let val = api_get(&client, "/monitor-policies/status").await?;
                    print_value(&val, fmt);
                }
            },

            // ── Ceph ──────────────────────────────────────────────────────
            Commands::Ceph(cmd) => match cmd {
                CephCmd::Create {
                    name,
                    monitors,
                    pool,
                    user,
                    keyring,
                    auto_start,
                } => {
                    let body = serde_json::json!({
                        "name": name,
                        "monitors": monitors.split(',').map(|s| s.trim()).collect::<Vec<_>>(),
                        "pool_name": pool,
                        "user": user,
                        "keyring": keyring,
                        "auto_start": auto_start,
                    });
                    let val = api_post(&client, "/storage/pools/ceph", &body).await?;
                    println!("Ceph pool '{}' created", name);
                    if !matches!(fmt, OutputFormat::Table) {
                        print_value(&val, fmt);
                    }
                }
                CephCmd::Pools => {
                    let val = api_get(&client, "/storage/pools").await?;
                    print_value(&val, fmt);
                }
                CephCmd::Health { name } => {
                    let val = api_get(&client, &format!("/storage/pools/{}/health", name)).await?;
                    print_value(&val, fmt);
                }
                CephCmd::Stats { name } => {
                    let val = api_get(&client, &format!("/storage/pools/{}/stats", name)).await?;
                    print_value(&val, fmt);
                }
                CephCmd::Images { name } => {
                    let val = api_get(&client, &format!("/storage/pools/{}/images", name)).await?;
                    print_value(&val, fmt);
                }
                CephCmd::CreateImage { pool, name, size } => {
                    let body = serde_json::json!({ "name": name, "size_mb": size });
                    api_post(&client, &format!("/storage/pools/{}/images", pool), &body).await?;
                    println!(
                        "RBD image '{}' created in pool '{}' ({}MB)",
                        name, pool, size
                    );
                }
                CephCmd::DeleteImage { pool, name } => {
                    api_delete(&client, &format!("/storage/pools/{}/images/{}", pool, name))
                        .await?;
                    println!("RBD image '{}' deleted from pool '{}'", name, pool);
                }
                CephCmd::Delete { name } => {
                    api_delete(&client, &format!("/storage/pools/{}", name)).await?;
                    println!("Storage pool '{}' deleted", name);
                }
            },

            // ── Networkd ─────────────────────────────────────────────────
            Commands::Net(cmd) => match cmd {
                NetCmd::Bridges => {
                    let val = api_get(&client, "/networkd/bridges").await?;
                    print_resources(val.as_array().unwrap_or(&vec![]), fmt);
                }
                NetCmd::Vlans => {
                    let val = api_get(&client, "/networkd/vlans").await?;
                    print_resources(val.as_array().unwrap_or(&vec![]), fmt);
                }
                NetCmd::Bonds => {
                    let val = api_get(&client, "/networkd/bonds").await?;
                    print_resources(val.as_array().unwrap_or(&vec![]), fmt);
                }
                NetCmd::Taps => {
                    let val = api_get(&client, "/networkd/taps").await?;
                    print_resources(val.as_array().unwrap_or(&vec![]), fmt);
                }
                NetCmd::Forwards => {
                    let val = api_get(&client, "/networkd/port-forwards").await?;
                    print_resources(val.as_array().unwrap_or(&vec![]), fmt);
                }
                NetCmd::CreateBridge { file } => {
                    let body = load_config_file(&file)?;
                    let val = api_post(&client, "/networkd/bridges", &body).await?;
                    println!("Bridge created");
                    if !matches!(fmt, OutputFormat::Table) {
                        print_value(&val, fmt);
                    }
                }
                NetCmd::CreateVlan { file } => {
                    let body = load_config_file(&file)?;
                    let val = api_post(&client, "/networkd/vlans", &body).await?;
                    println!("VLAN created");
                    if !matches!(fmt, OutputFormat::Table) {
                        print_value(&val, fmt);
                    }
                }
                NetCmd::CreateBond { file } => {
                    let body = load_config_file(&file)?;
                    let val = api_post(&client, "/networkd/bonds", &body).await?;
                    println!("Bond created");
                    if !matches!(fmt, OutputFormat::Table) {
                        print_value(&val, fmt);
                    }
                }
                NetCmd::CreateForward { file } => {
                    let body = load_config_file(&file)?;
                    let val = api_post(&client, "/networkd/port-forwards", &body).await?;
                    println!("Port forward created");
                    if !matches!(fmt, OutputFormat::Table) {
                        print_value(&val, fmt);
                    }
                }
                NetCmd::SyncForwards => {
                    let val = api_post_empty(&client, "/networkd/port-forwards/sync").await?;
                    print_value(&val, fmt);
                }
                NetCmd::Reload => {
                    api_post_void(&client, "/networkd/reload").await?;
                    println!("networkd reloaded");
                }
                NetCmd::Links => {
                    let val = api_get(&client, "/networkd/links").await?;
                    print_value(&val, fmt);
                }
            },

            Commands::Runtime(cmd) => match cmd {
                RuntimeCmd::Capabilities => {
                    let val = api_get(&client, "/runtime/capabilities").await?;
                    print_value(&val, fmt);
                }
                RuntimeCmd::Migrate(m) => match m {
                    RuntimeMigrateCmd::Start {
                        name,
                        uri,
                        mode,
                        shared_storage,
                        bandwidth_mbps,
                        multifd_channels,
                        transfer_network_state,
                    } => {
                        let body = serde_json::json!({
                            "target_uri": uri,
                            "mode": mode,
                            "shared_storage_confirmed": shared_storage,
                            "bandwidth_mbps": bandwidth_mbps,
                            "multifd_channels": multifd_channels,
                            "transfer_network_state": transfer_network_state,
                        });
                        let val = api_post(
                            &client,
                            &format!("/vms/{}/migration/native/start", name),
                            &body,
                        )
                        .await?;
                        println!("Started native migration for '{}'", name);
                        if !matches!(fmt, OutputFormat::Table) {
                            print_value(&val, fmt);
                        }
                    }
                    RuntimeMigrateCmd::PrepareReceiver {
                        name,
                        disk_path,
                        listen_host,
                        target_node,
                        receiver_ttl_seconds,
                    } => {
                        let body = serde_json::json!({
                            "disk_path": disk_path,
                            "listen_host": listen_host,
                            "target_node": target_node,
                            "receiver_ttl_seconds": receiver_ttl_seconds,
                        });
                        let val = api_post(
                            &client,
                            &format!("/vms/{}/migration/native/prepare-receiver", name),
                            &body,
                        )
                        .await?;
                        println!("Prepared migration receiver for '{}'", name);
                        print_value(&val, fmt);
                    }
                    RuntimeMigrateCmd::ActivateReceiver { id, target_node } => {
                        let body = serde_json::json!({ "target_node": target_node });
                        let val = api_post(
                            &client,
                            &format!("/migration/receivers/{}/activate", id),
                            &body,
                        )
                        .await?;
                        println!("Activated migration receiver '{}'", id);
                        print_value(&val, fmt);
                    }
                    RuntimeMigrateCmd::AbortReceiver { id, target_node } => {
                        let path = match target_node {
                            Some(n) => format!("/migration/receivers/{}?target_node={}", id, n),
                            None => format!("/migration/receivers/{}", id),
                        };
                        api_delete(&client, &path).await?;
                        println!("Aborted migration receiver '{}'", id);
                    }
                    RuntimeMigrateCmd::Status { name } => {
                        let val =
                            api_get(&client, &format!("/vms/{}/migration/native/status", name))
                                .await?;
                        print_value(&val, fmt);
                    }
                    RuntimeMigrateCmd::Cancel { name } => {
                        let val = api_post_empty(
                            &client,
                            &format!("/vms/{}/migration/native/cancel", name),
                        )
                        .await?;
                        println!("Cancelled native migration for '{}'", name);
                        if !matches!(fmt, OutputFormat::Table) {
                            print_value(&val, fmt);
                        }
                    }
                },
            },

            Commands::ContainerGroup(cmd) => match cmd {
                ContainerGroupCmd::List => {
                    let val = api_get(&client, "/container-groups").await?;
                    print_value(&val, fmt);
                }
                ContainerGroupCmd::Info { name } => {
                    let val = api_get(&client, &format!("/container-groups/{name}/spec")).await?;
                    print_value(&val, fmt);
                }
                ContainerGroupCmd::Apply { file } => {
                    let config = load_config_file(&file)?;
                    let spec = config.get("spec").unwrap_or(&config);
                    let val = api_post(&client, "/container-groups/apply", spec).await?;
                    println!("Applied ContainerGroup successfully");
                    if !matches!(fmt, OutputFormat::Table) {
                        print_value(&val, fmt);
                    }
                }
                ContainerGroupCmd::Delete { name } => {
                    api_delete(&client, &format!("/container-groups/{name}")).await?;
                    println!("Deleted ContainerGroup '{name}'");
                }
                ContainerGroupCmd::Events => {
                    let val = api_get(&client, "/container-group-events").await?;
                    print_value(&val, fmt);
                }
                ContainerGroupCmd::Backup(cmd) => match cmd {
                    ContainerGroupBackupCmd::Create {
                        container_group_name,
                        retention_days,
                    } => {
                        let body = serde_json::json!({
                            "container_group_name": container_group_name,
                            "retention_days": retention_days,
                        });
                        let val = api_post(&client, "/container-group-backups", &body).await?;
                        println!("Created backup for ContainerGroup '{container_group_name}'");
                        if !matches!(fmt, OutputFormat::Table) {
                            print_value(&val, fmt);
                        }
                    }
                    ContainerGroupBackupCmd::List => {
                        let val = api_get(&client, "/container-group-backups").await?;
                        print_value(&val, fmt);
                    }
                    ContainerGroupBackupCmd::Info { id } => {
                        let val =
                            api_get(&client, &format!("/container-group-backups/{id}")).await?;
                        print_value(&val, fmt);
                    }
                    ContainerGroupBackupCmd::Delete { id } => {
                        api_delete(&client, &format!("/container-group-backups/{id}")).await?;
                        println!("Deleted backup '{id}'");
                    }
                    ContainerGroupBackupCmd::Restore { id } => {
                        let val = api_post_empty(
                            &client,
                            &format!("/container-group-backups/{id}/restore"),
                        )
                        .await?;
                        println!("Restored backup '{id}'");
                        print_value(&val, fmt);
                    }
                },
            },

            Commands::Ai(cmd) => match cmd {
                AiCmd::Model(sub) => match sub {
                    AiModelCmd::List => {
                        let val = api_get(&client, "/ai/models").await?;
                        print_value(&val, fmt);
                    }
                    AiModelCmd::Info { name } => {
                        let val = api_get(&client, &format!("/ai/models/{name}")).await?;
                        print_value(&val, fmt);
                    }
                    AiModelCmd::Add {
                        name,
                        source,
                        revision,
                        format: model_format,
                    } => {
                        let body = serde_json::json!({
                            "name": name,
                            "source": source,
                            "revision": revision,
                            "format": model_format,
                        });
                        let val = api_post(&client, "/ai/models", &body).await?;
                        println!("Registered model artifact '{name}'");
                        if !matches!(fmt, OutputFormat::Table) {
                            print_value(&val, fmt);
                        }
                    }
                    AiModelCmd::Delete { name } => {
                        api_delete(&client, &format!("/ai/models/{name}")).await?;
                        println!("Deleted model artifact '{name}'");
                    }
                },
                AiCmd::Profile(sub) => match sub {
                    AiProfileCmd::List => {
                        let val = api_get(&client, "/ai/profiles").await?;
                        print_value(&val, fmt);
                    }
                    AiProfileCmd::Info { name } => {
                        let val = api_get(&client, &format!("/ai/profiles/{name}")).await?;
                        print_value(&val, fmt);
                    }
                    AiProfileCmd::Add {
                        name,
                        runtime,
                        vendor,
                        gpu,
                        vram,
                        cpu,
                        memory,
                    } => {
                        let body = serde_json::json!({
                            "name": name,
                            "runtime": runtime,
                            "gpu": {
                                "vendor": vendor,
                                "count": gpu,
                                "minimum_vram_gib": vram,
                            },
                            "cpu": cpu,
                            "memory_gib": memory,
                        });
                        let val = api_post(&client, "/ai/profiles", &body).await?;
                        println!("Created inference profile '{name}'");
                        if !matches!(fmt, OutputFormat::Table) {
                            print_value(&val, fmt);
                        }
                    }
                    AiProfileCmd::Delete { name } => {
                        api_delete(&client, &format!("/ai/profiles/{name}")).await?;
                        println!("Deleted profile '{name}'");
                    }
                },
                AiCmd::Deploy {
                    model,
                    name,
                    model_name,
                    runtime,
                    profile,
                    gpu,
                    vram,
                    cpu,
                    memory,
                    replicas,
                } => {
                    let model = model
                        .or(model_name)
                        .context("provide a model name (positional or --model-name)")?;
                    let dep_name = name.unwrap_or_else(|| model.clone());
                    // Ensure a profile exists (create/upsert-style for the MVP CLI).
                    let profile_body = serde_json::json!({
                        "name": profile,
                        "runtime": runtime,
                        "gpu": {
                            "vendor": "nvidia",
                            "count": gpu,
                            "minimum_vram_gib": vram,
                        },
                        "cpu": cpu,
                        "memory_gib": memory,
                    });
                    let _ = api_post(&client, "/ai/profiles", &profile_body).await;
                    let body = serde_json::json!({
                        "name": dep_name,
                        "model": model,
                        "profile": profile,
                        "replicas": replicas,
                    });
                    let val = api_post(&client, "/ai/deployments", &body).await?;
                    println!("Deployed '{dep_name}' (model={model}, profile={profile}, replicas={replicas})");
                    if !matches!(fmt, OutputFormat::Table) {
                        print_value(&val, fmt);
                    }
                }
                AiCmd::Deployment(sub) => match sub {
                    AiDeploymentCmd::List => {
                        let val = api_get(&client, "/ai/deployments").await?;
                        print_value(&val, fmt);
                    }
                    AiDeploymentCmd::Info { name } => {
                        let val = api_get(&client, &format!("/ai/deployments/{name}")).await?;
                        print_value(&val, fmt);
                    }
                    AiDeploymentCmd::Scale { name, replicas } => {
                        let body = serde_json::json!({ "replicas": replicas });
                        let val =
                            api_post(&client, &format!("/ai/deployments/{name}/scale"), &body)
                                .await?;
                        println!("Scaled deployment '{name}' to {replicas} replicas");
                        if !matches!(fmt, OutputFormat::Table) {
                            print_value(&val, fmt);
                        }
                    }
                    AiDeploymentCmd::Autoscale {
                        name,
                        enable,
                        min,
                        max,
                        scale_to_zero,
                        scale_out_queue,
                        scale_out_seconds,
                        scale_in_queue,
                        scale_in_seconds,
                    } => {
                        let body = serde_json::json!({
                            "autoscaling": {
                                "enabled": enable,
                                "min_replicas": min,
                                "max_replicas": max,
                                "scale_to_zero": scale_to_zero,
                                "scale_out_queue": scale_out_queue,
                                "scale_out_seconds": scale_out_seconds,
                                "scale_in_queue": scale_in_queue,
                                "scale_in_seconds": scale_in_seconds,
                            }
                        });
                        let val = api_put(
                            &client,
                            &format!("/ai/deployments/{name}/autoscaling"),
                            &body,
                        )
                        .await?;
                        println!("Updated autoscaling on '{name}' (enabled={enable})");
                        if !matches!(fmt, OutputFormat::Table) {
                            print_value(&val, fmt);
                        }
                    }
                    AiDeploymentCmd::Drain {
                        name,
                        replica,
                        grace_seconds,
                    } => {
                        let body = serde_json::json!({
                            "replica": replica,
                            "grace_seconds": grace_seconds,
                        });
                        let val =
                            api_post(&client, &format!("/ai/deployments/{name}/drain"), &body)
                                .await?;
                        println!("Draining deployment '{name}'");
                        if !matches!(fmt, OutputFormat::Table) {
                            print_value(&val, fmt);
                        }
                    }
                    AiDeploymentCmd::Rollout {
                        name,
                        strategy,
                        target_model,
                        canary_percent,
                    } => {
                        let body = serde_json::json!({
                            "strategy": strategy,
                            "target_model": target_model,
                            "canary_percent": canary_percent,
                        });
                        let val =
                            api_post(&client, &format!("/ai/deployments/{name}/rollout"), &body)
                                .await?;
                        println!("Rollout started for '{name}' ({strategy})");
                        if !matches!(fmt, OutputFormat::Table) {
                            print_value(&val, fmt);
                        }
                    }
                    AiDeploymentCmd::Metrics { name } => {
                        let val =
                            api_get(&client, &format!("/ai/deployments/{name}/metrics")).await?;
                        print_value(&val, fmt);
                    }
                    AiDeploymentCmd::Delete { name } => {
                        api_delete(&client, &format!("/ai/deployments/{name}")).await?;
                        println!("Deleted deployment '{name}'");
                    }
                },
                AiCmd::Endpoint(sub) => match sub {
                    AiEndpointCmd::List => {
                        let val = api_get(&client, "/ai/endpoints").await?;
                        print_value(&val, fmt);
                    }
                    AiEndpointCmd::Info { name } => {
                        let val = api_get(&client, &format!("/ai/endpoints/{name}")).await?;
                        print_value(&val, fmt);
                    }
                    AiEndpointCmd::Expose {
                        deployment,
                        name,
                        port,
                        vip,
                        openai_compatible: _,
                        routing,
                        preferred_site,
                        residency,
                    } => {
                        let ep_name = name.unwrap_or_else(|| format!("{deployment}-openai"));
                        let body = serde_json::json!({
                            "name": ep_name,
                            "deployment": deployment,
                            "protocol": "openai",
                            "port": port,
                            "vip": vip,
                            "routing_strategy": routing,
                            "preferred_site": preferred_site,
                            "residency": residency,
                        });
                        let val = api_post(&client, "/ai/endpoints", &body).await?;
                        println!("Exposed endpoint '{ep_name}' for deployment '{deployment}' (routing={routing})");
                        if !matches!(fmt, OutputFormat::Table) {
                            print_value(&val, fmt);
                        }
                    }
                    AiEndpointCmd::Delete { name } => {
                        api_delete(&client, &format!("/ai/endpoints/{name}")).await?;
                        println!("Deleted endpoint '{name}'");
                    }
                },
                AiCmd::Key(sub) => match sub {
                    AiKeyCmd::List => {
                        let val = api_get(&client, "/ai/keys").await?;
                        print_value(&val, fmt);
                    }
                    AiKeyCmd::Create {
                        name,
                        endpoint,
                        model,
                        request_quota,
                    } => {
                        let body = serde_json::json!({
                            "name": name,
                            "endpoint": endpoint,
                            "model": model,
                            "request_quota": request_quota,
                        });
                        let val = api_post(&client, "/ai/keys", &body).await?;
                        println!("Created API key (secret shown once)");
                        print_value(&val, fmt);
                    }
                    AiKeyCmd::Delete { id } => {
                        api_delete(&client, &format!("/ai/keys/{id}")).await?;
                        println!("Deleted API key '{id}'");
                    }
                },
                AiCmd::Gpus => {
                    let val = api_get(&client, "/ai/gpus").await?;
                    print_value(&val, fmt);
                }
                AiCmd::Node(sub) => match sub {
                    AiNodeCmd::List => {
                        let val = api_get(&client, "/ai/nodes").await?;
                        print_value(&val, fmt);
                    }
                    AiNodeCmd::MigCreate {
                        node,
                        parent_bdf,
                        profile,
                    } => {
                        let body = serde_json::json!({
                            "parent_bdf": parent_bdf,
                            "profile": profile,
                        });
                        let val =
                            api_post(&client, &format!("/ai/nodes/{node}/mig"), &body).await?;
                        print_value(&val, fmt);
                    }
                    AiNodeCmd::MigDelete { node, bdf } => {
                        // Janus BDFs contain ':' — percent-encode for the path segment.
                        let encoded = bdf.replace(':', "%3A");
                        api_delete(&client, &format!("/ai/nodes/{node}/mig/{encoded}")).await?;
                        println!("Deleted MIG slice '{bdf}' on node '{node}'");
                    }
                },
                AiCmd::Capacity => {
                    let val = api_get(&client, "/ai/capacity").await?;
                    print_value(&val, fmt);
                }
                AiCmd::Events => {
                    let val = api_get(&client, "/ai/events").await?;
                    print_value(&val, fmt);
                }
            },
        }

        Ok(())
    }
}

/// Render `POST /vms/{name}/fork`. Times are what FluxVM measured on that host; none are estimated.
fn print_fork(out: &serde_json::Value) {
    let kids = out["children"]
        .as_array()
        .map(Vec::as_slice)
        .unwrap_or_default();
    println!(
        "Forked {} into {} VM(s) in {} ms",
        out["parent"].as_str().unwrap_or("?"),
        kids.len(),
        out["elapsed_ms"]
    );
    for k in kids {
        println!(
            "  {}  {}",
            k["name"].as_str().unwrap_or("?"),
            k["id"].as_str().unwrap_or("?")
        );
    }
    match out
        .get("first_command_ms")
        .and_then(serde_json::Value::as_u64)
    {
        Some(ms) => println!("First command ran in every child within {ms} ms (slowest child)"),
        None => println!("First-command time not measured (use --ready)"),
    }
}

/// Render `GET /vms/{name}/memory`. PSS is the honest per-VM cost: pages shared with other
/// VMs are divided between them, so summing PSS over VMs does not double count.
fn print_memory(m: &serde_json::Value) {
    let mib = |kib: &serde_json::Value| kib.as_u64().map(|k| k as f64 / 1024.0);
    println!("Configured: {} MiB", m["configured_mib"]);
    match m.get("usage").filter(|u| !u.is_null()) {
        Some(u) => {
            for (label, key) in [
                ("PSS", "pss_kib"),
                ("Private", "private_kib"),
                ("Shared", "shared_kib"),
                ("RSS", "rss_kib"),
            ] {
                if let Some(v) = mib(&u[key]) {
                    println!("{label:<11} {v:.1} MiB");
                }
            }
        }
        None => println!("Usage:      unavailable (no VMM process or unreadable smaps)"),
    }
    match m.get("balloon").filter(|b| !b.is_null()) {
        Some(b) => println!(
            "Balloon:    target {} MiB, reached {} MiB",
            b["target_mib"], b["actual_mib"]
        ),
        None => println!("Balloon:    none"),
    }
}

#[cfg(test)]
mod memory_cli_tests {
    use super::*;

    #[test]
    fn memory_and_balloon_parse() {
        let cli = Cli::try_parse_from(["fabricctl", "memory", "vm1"]).unwrap();
        assert!(matches!(cli.command, Some(Commands::Memory { .. })));
        let cli = Cli::try_parse_from(["fabricctl", "balloon", "vm1", "--mib", "256"]).unwrap();
        match cli.command {
            Some(Commands::Balloon { name, mib }) => {
                assert_eq!((name.as_str(), mib), ("vm1", Some(256)))
            }
            _ => panic!("expected Balloon"),
        }
        let cli = Cli::try_parse_from(["fabricctl", "balloon", "vm1"]).unwrap();
        assert!(matches!(
            cli.command,
            Some(Commands::Balloon { mib: None, .. })
        ));
        assert!(Cli::try_parse_from(["fabricctl", "balloon", "vm1", "--mib", "-1"]).is_err());
    }

    #[test]
    fn fork_parses_and_bounds_count() {
        let cli =
            Cli::try_parse_from(["fabricctl", "fork", "vm1", "--count", "16", "--ready"]).unwrap();
        match cli.command {
            Some(Commands::Fork {
                name,
                count,
                ready,
                prefix,
                ..
            }) => {
                assert_eq!(
                    (name.as_str(), count, ready, prefix),
                    ("vm1", 16, true, None)
                );
            }
            _ => panic!("expected Fork"),
        }
        assert!(Cli::try_parse_from(["fabricctl", "fork", "vm1", "--count", "0"]).is_err());
        assert!(Cli::try_parse_from(["fabricctl", "fork", "vm1", "--count", "33"]).is_err());
        assert!(matches!(
            Cli::try_parse_from(["fabricctl", "fork", "vm1"])
                .unwrap()
                .command,
            Some(Commands::Fork { count: 1, .. })
        ));
    }
}

#[cfg(test)]
mod container_group_cli_tests {
    use super::*;

    #[test]
    fn container_group_list_parses() {
        let cli = Cli::try_parse_from(["fabricctl", "container-group", "list"]).unwrap();
        assert!(matches!(
            cli.command,
            Some(Commands::ContainerGroup(ContainerGroupCmd::List))
        ));
    }

    #[test]
    fn container_group_apply_requires_a_file() {
        assert!(Cli::try_parse_from(["fabricctl", "container-group", "apply"]).is_err());
        let cli = Cli::try_parse_from(["fabricctl", "container-group", "apply", "-f", "cg.yaml"])
            .unwrap();
        match cli.command {
            Some(Commands::ContainerGroup(ContainerGroupCmd::Apply { file })) => {
                assert_eq!(file, "cg.yaml");
            }
            _ => panic!("expected ContainerGroup::Apply"),
        }
    }

    #[test]
    fn container_group_delete_requires_a_name() {
        assert!(Cli::try_parse_from(["fabricctl", "container-group", "delete"]).is_err());
        let cli = Cli::try_parse_from(["fabricctl", "container-group", "delete", "web"]).unwrap();
        match cli.command {
            Some(Commands::ContainerGroup(ContainerGroupCmd::Delete { name })) => {
                assert_eq!(name, "web");
            }
            _ => panic!("expected ContainerGroup::Delete"),
        }
    }

    #[test]
    fn container_group_backup_create_defaults_retention_days_to_30() {
        let cli = Cli::try_parse_from(["fabricctl", "container-group", "backup", "create", "web"])
            .unwrap();
        match cli.command {
            Some(Commands::ContainerGroup(ContainerGroupCmd::Backup(
                ContainerGroupBackupCmd::Create {
                    container_group_name,
                    retention_days,
                },
            ))) => {
                assert_eq!(container_group_name, "web");
                assert_eq!(retention_days, 30);
            }
            _ => panic!("expected ContainerGroup::Backup::Create"),
        }
    }

    #[test]
    fn container_group_backup_restore_requires_an_id() {
        assert!(
            Cli::try_parse_from(["fabricctl", "container-group", "backup", "restore"]).is_err()
        );
        let cli = Cli::try_parse_from([
            "fabricctl",
            "container-group",
            "backup",
            "restore",
            "backup-1",
        ])
        .unwrap();
        match cli.command {
            Some(Commands::ContainerGroup(ContainerGroupCmd::Backup(
                ContainerGroupBackupCmd::Restore { id },
            ))) => {
                assert_eq!(id, "backup-1");
            }
            _ => panic!("expected ContainerGroup::Backup::Restore"),
        }
    }

    #[test]
    fn meta_commands_parse() {
        assert!(matches!(
            Cli::try_parse_from(["fabricctl", "status"])
                .unwrap()
                .command,
            Some(Commands::Status)
        ));
        assert!(matches!(
            Cli::try_parse_from(["fabricctl", "config"])
                .unwrap()
                .command,
            Some(Commands::Config)
        ));
        let cli = Cli::try_parse_from(["fabricctl", "completion", "zsh"]).unwrap();
        assert!(matches!(
            cli.command,
            Some(Commands::Completion { shell: Shell::Zsh })
        ));
    }

    #[test]
    fn bare_fabricctl_has_no_command() {
        let cli = Cli::try_parse_from(["fabricctl"]).unwrap();
        assert!(cli.command.is_none());
    }
}

#[cfg(test)]
mod default_server_probe_tests {
    use super::port_speaks_tls;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpListener;

    /// One-shot listener: reads the request, writes `reply`, closes.
    async fn serve_once(reply: &'static [u8]) -> String {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap().to_string();
        tokio::spawn(async move {
            let (mut sock, _) = listener.accept().await.unwrap();
            let mut buf = [0u8; 256];
            let _ = sock.read(&mut buf).await;
            let _ = sock.write_all(reply).await;
        });
        addr
    }

    #[tokio::test]
    async fn plain_http_server_is_not_tls() {
        let addr = serve_once(b"HTTP/1.1 200 OK\r\ncontent-length: 0\r\n\r\n").await;
        assert!(!port_speaks_tls(&addr).await);
    }

    #[tokio::test]
    async fn tls_alert_reply_is_tls() {
        // What a TLS listener sends back to plaintext: a fatal alert record.
        let addr = serve_once(&[0x15, 0x03, 0x01, 0x00, 0x02, 0x02, 0x0a]).await;
        assert!(port_speaks_tls(&addr).await);
    }

    #[tokio::test]
    async fn silent_close_is_tls() {
        let addr = serve_once(b"").await;
        assert!(port_speaks_tls(&addr).await);
    }

    #[tokio::test]
    async fn nothing_listening_is_not_tls() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap().to_string();
        drop(listener);
        assert!(!port_speaks_tls(&addr).await);
    }
}
