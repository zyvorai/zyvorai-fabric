// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: LicenseRef-Zyvor-Production-2.0

use serde::{Deserialize, Serialize};

/// Types of network devices managed by systemd-networkd
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum NetDevKind {
    Bridge,
    Bond,
    Vlan,
    Macvtap,
    Tap,
}

impl NetDevKind {
    pub fn as_str(&self) -> &str {
        match self {
            NetDevKind::Bridge => "bridge",
            NetDevKind::Bond => "bond",
            NetDevKind::Vlan => "vlan",
            NetDevKind::Macvtap => "macvtap",
            NetDevKind::Tap => "tap",
        }
    }
}

impl std::fmt::Display for NetDevKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Macvtap / macvlan mode
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MacvtapMode {
    Private,
    Vepa,
    Bridge,
    Passthru,
    Source,
}

impl MacvtapMode {
    pub fn as_str(&self) -> &str {
        match self {
            MacvtapMode::Private => "private",
            MacvtapMode::Vepa => "vepa",
            MacvtapMode::Bridge => "bridge",
            MacvtapMode::Passthru => "passthru",
            MacvtapMode::Source => "source",
        }
    }
}

/// DHCP configuration mode
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DhcpMode {
    Yes,
    #[default]
    No,
    Ipv4,
    Ipv6,
}

impl DhcpMode {
    pub fn as_str(&self) -> &str {
        match self {
            DhcpMode::Yes => "yes",
            DhcpMode::No => "no",
            DhcpMode::Ipv4 => "ipv4",
            DhcpMode::Ipv6 => "ipv6",
        }
    }
}

/// Bond mode
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum BondMode {
    #[default]
    BalanceRr,
    ActiveBackup,
    BalanceXor,
    Broadcast,
    #[serde(rename = "802.3ad")]
    Ieee8023ad,
    BalanceTlb,
    BalanceAlb,
}

impl BondMode {
    pub fn as_str(&self) -> &str {
        match self {
            BondMode::BalanceRr => "balance-rr",
            BondMode::ActiveBackup => "active-backup",
            BondMode::BalanceXor => "balance-xor",
            BondMode::Broadcast => "broadcast",
            BondMode::Ieee8023ad => "802.3ad",
            BondMode::BalanceTlb => "balance-tlb",
            BondMode::BalanceAlb => "balance-alb",
        }
    }
}

/// LACP rate for 802.3ad mode
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LacpRate {
    Slow,
    Fast,
}

impl LacpRate {
    pub fn as_str(&self) -> &str {
        match self {
            LacpRate::Slow => "slow",
            LacpRate::Fast => "fast",
        }
    }
}

/// Transmit hash policy for bond
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum TransmitHashPolicy {
    Layer2,
    #[serde(rename = "layer3+4")]
    Layer34,
    #[serde(rename = "layer2+3")]
    Layer23,
    Encap23,
    Encap34,
}

impl TransmitHashPolicy {
    pub fn as_str(&self) -> &str {
        match self {
            TransmitHashPolicy::Layer2 => "layer2",
            TransmitHashPolicy::Layer34 => "layer3+4",
            TransmitHashPolicy::Layer23 => "layer2+3",
            TransmitHashPolicy::Encap23 => "encap2+3",
            TransmitHashPolicy::Encap34 => "encap3+4",
        }
    }
}

/// A static route entry
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RouteEntry {
    pub destination: String,
    #[serde(default)]
    pub gateway: Option<String>,
    #[serde(default)]
    pub metric: Option<u32>,
    #[serde(default)]
    pub scope: Option<String>,
}

// ─── Bridge ───────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BridgeConfig {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub stp: Option<bool>,
    #[serde(default)]
    pub forward_delay_sec: Option<u32>,
    #[serde(default)]
    pub hello_time_sec: Option<u32>,
    #[serde(default)]
    pub max_age_sec: Option<u32>,
    #[serde(default)]
    pub vlan_filtering: Option<bool>,
    #[serde(default)]
    pub mtu: Option<u16>,
    #[serde(default)]
    pub mac_address: Option<String>,
    #[serde(default)]
    pub addresses: Vec<String>,
    #[serde(default)]
    pub gateway: Option<String>,
    #[serde(default)]
    pub dns: Vec<String>,
    #[serde(default)]
    pub dhcp: DhcpMode,
    #[serde(default)]
    pub created: String,
    #[serde(default)]
    pub updated: String,
    /// False when discovered on the host but not created via zyvor-fabricd.
    #[serde(default = "default_true")]
    pub managed: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub operational_state: Option<String>,
}

fn default_true() -> bool {
    true
}

/// Adopt a host-discovered entity (id must start with `host:`).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AdoptHostRequest {
    pub host_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateBridgeRequest {
    pub name: String,
    #[serde(default)]
    pub stp: Option<bool>,
    #[serde(default)]
    pub forward_delay_sec: Option<u32>,
    #[serde(default)]
    pub hello_time_sec: Option<u32>,
    #[serde(default)]
    pub max_age_sec: Option<u32>,
    #[serde(default)]
    pub vlan_filtering: Option<bool>,
    #[serde(default)]
    pub mtu: Option<u16>,
    #[serde(default)]
    pub mac_address: Option<String>,
    #[serde(default)]
    pub addresses: Vec<String>,
    #[serde(default)]
    pub gateway: Option<String>,
    #[serde(default)]
    pub dns: Vec<String>,
    #[serde(default)]
    pub dhcp: DhcpMode,
}

// ─── VLAN ─────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VlanConfig {
    pub id: String,
    pub name: String,
    pub vlan_id: u16,
    pub parent_interface: String,
    #[serde(default)]
    pub mtu: Option<u16>,
    #[serde(default)]
    pub addresses: Vec<String>,
    #[serde(default)]
    pub gateway: Option<String>,
    #[serde(default)]
    pub dns: Vec<String>,
    #[serde(default)]
    pub dhcp: DhcpMode,
    #[serde(default)]
    pub created: String,
    #[serde(default)]
    pub updated: String,
    #[serde(default = "default_true")]
    pub managed: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub operational_state: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateVlanRequest {
    pub name: String,
    pub vlan_id: u16,
    pub parent_interface: String,
    #[serde(default)]
    pub mtu: Option<u16>,
    #[serde(default)]
    pub addresses: Vec<String>,
    #[serde(default)]
    pub gateway: Option<String>,
    #[serde(default)]
    pub dns: Vec<String>,
    #[serde(default)]
    pub dhcp: DhcpMode,
}

// ─── Macvtap ──────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MacvtapConfig {
    pub id: String,
    pub name: String,
    pub parent_interface: String,
    #[serde(default = "default_macvtap_mode")]
    pub mode: MacvtapMode,
    #[serde(default)]
    pub mtu: Option<u16>,
    #[serde(default)]
    pub mac_address: Option<String>,
    #[serde(default)]
    pub created: String,
    #[serde(default)]
    pub updated: String,
    #[serde(default = "default_true")]
    pub managed: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub operational_state: Option<String>,
}

fn default_macvtap_mode() -> MacvtapMode {
    MacvtapMode::Bridge
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateMacvtapRequest {
    pub name: String,
    pub parent_interface: String,
    #[serde(default = "default_macvtap_mode")]
    pub mode: MacvtapMode,
    #[serde(default)]
    pub mtu: Option<u16>,
    #[serde(default)]
    pub mac_address: Option<String>,
}

// ─── Tap ──────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TapConfig {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub user: Option<String>,
    #[serde(default)]
    pub group: Option<String>,
    #[serde(default)]
    pub multi_queue: Option<bool>,
    #[serde(default)]
    pub vnet_hdr: Option<bool>,
    /// Bridge to attach this tap to
    #[serde(default)]
    pub bridge: Option<String>,
    #[serde(default)]
    pub mtu: Option<u16>,
    #[serde(default)]
    pub mac_address: Option<String>,
    #[serde(default)]
    pub created: String,
    #[serde(default)]
    pub updated: String,
    #[serde(default = "default_true")]
    pub managed: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub operational_state: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateTapRequest {
    pub name: String,
    #[serde(default)]
    pub user: Option<String>,
    #[serde(default)]
    pub group: Option<String>,
    #[serde(default)]
    pub multi_queue: Option<bool>,
    #[serde(default)]
    pub vnet_hdr: Option<bool>,
    #[serde(default)]
    pub bridge: Option<String>,
    #[serde(default)]
    pub mtu: Option<u16>,
    #[serde(default)]
    pub mac_address: Option<String>,
}

// ─── Bond ─────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BondConfig {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub mode: BondMode,
    #[serde(default)]
    pub mii_monitor_sec: Option<u32>,
    #[serde(default)]
    pub up_delay_sec: Option<u32>,
    #[serde(default)]
    pub down_delay_sec: Option<u32>,
    #[serde(default)]
    pub lacp_rate: Option<LacpRate>,
    #[serde(default)]
    pub transmit_hash_policy: Option<TransmitHashPolicy>,
    #[serde(default)]
    pub min_links: Option<u32>,
    #[serde(default)]
    pub primary_slave: Option<String>,
    /// Interfaces to enslave into this bond
    #[serde(default)]
    pub slave_interfaces: Vec<String>,
    #[serde(default)]
    pub mtu: Option<u16>,
    #[serde(default)]
    pub mac_address: Option<String>,
    #[serde(default)]
    pub addresses: Vec<String>,
    #[serde(default)]
    pub gateway: Option<String>,
    #[serde(default)]
    pub dns: Vec<String>,
    #[serde(default)]
    pub dhcp: DhcpMode,
    #[serde(default)]
    pub routes: Vec<RouteEntry>,
    #[serde(default)]
    pub created: String,
    #[serde(default)]
    pub updated: String,
    #[serde(default = "default_true")]
    pub managed: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub operational_state: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateBondRequest {
    pub name: String,
    #[serde(default)]
    pub mode: BondMode,
    #[serde(default)]
    pub mii_monitor_sec: Option<u32>,
    #[serde(default)]
    pub up_delay_sec: Option<u32>,
    #[serde(default)]
    pub down_delay_sec: Option<u32>,
    #[serde(default)]
    pub lacp_rate: Option<LacpRate>,
    #[serde(default)]
    pub transmit_hash_policy: Option<TransmitHashPolicy>,
    #[serde(default)]
    pub min_links: Option<u32>,
    #[serde(default)]
    pub primary_slave: Option<String>,
    #[serde(default)]
    pub slave_interfaces: Vec<String>,
    #[serde(default)]
    pub mtu: Option<u16>,
    #[serde(default)]
    pub mac_address: Option<String>,
    #[serde(default)]
    pub addresses: Vec<String>,
    #[serde(default)]
    pub gateway: Option<String>,
    #[serde(default)]
    pub dns: Vec<String>,
    #[serde(default)]
    pub dhcp: DhcpMode,
    #[serde(default)]
    pub routes: Vec<RouteEntry>,
}

// ─── Network file (for physical interfaces) ──────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetworkFileConfig {
    pub id: String,
    /// Interface name to match
    pub match_name: String,
    #[serde(default)]
    pub match_mac: Option<String>,
    #[serde(default)]
    pub addresses: Vec<String>,
    #[serde(default)]
    pub gateway: Option<String>,
    #[serde(default)]
    pub dns: Vec<String>,
    #[serde(default)]
    pub dhcp: DhcpMode,
    /// Attach this interface to a bridge
    #[serde(default)]
    pub bridge: Option<String>,
    /// Attach this interface to a bond
    #[serde(default)]
    pub bond: Option<String>,
    #[serde(default)]
    pub mtu: Option<u16>,
    #[serde(default)]
    pub routes: Vec<RouteEntry>,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub created: String,
    #[serde(default)]
    pub updated: String,
    #[serde(default = "default_true")]
    pub managed: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub operational_state: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateNetworkFileRequest {
    pub match_name: String,
    #[serde(default)]
    pub match_mac: Option<String>,
    #[serde(default)]
    pub addresses: Vec<String>,
    #[serde(default)]
    pub gateway: Option<String>,
    #[serde(default)]
    pub dns: Vec<String>,
    #[serde(default)]
    pub dhcp: DhcpMode,
    #[serde(default)]
    pub bridge: Option<String>,
    #[serde(default)]
    pub bond: Option<String>,
    #[serde(default)]
    pub mtu: Option<u16>,
    #[serde(default)]
    pub routes: Vec<RouteEntry>,
    #[serde(default)]
    pub description: Option<String>,
}

// ─── Link file (.link) ───────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LinkFileConfig {
    pub id: String,
    /// Original MAC to match
    #[serde(default)]
    pub match_mac: Option<String>,
    /// Original name/path/driver to match
    #[serde(default)]
    pub match_path: Option<String>,
    #[serde(default)]
    pub match_driver: Option<String>,
    #[serde(default)]
    pub match_original_name: Option<String>,
    /// Rename the interface
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub mtu: Option<u16>,
    #[serde(default)]
    pub mac_address: Option<String>,
    #[serde(default)]
    pub wake_on_lan: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub created: String,
    #[serde(default)]
    pub updated: String,
    #[serde(default = "default_true")]
    pub managed: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_file: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateLinkFileRequest {
    #[serde(default)]
    pub match_mac: Option<String>,
    #[serde(default)]
    pub match_path: Option<String>,
    #[serde(default)]
    pub match_driver: Option<String>,
    #[serde(default)]
    pub match_original_name: Option<String>,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub mtu: Option<u16>,
    #[serde(default)]
    pub mac_address: Option<String>,
    #[serde(default)]
    pub wake_on_lan: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
}

// ─── Device status from networkctl ────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeviceStatus {
    pub name: String,
    pub kind: String,
    pub state: String,
    pub address: Option<String>,
    pub driver: Option<String>,
    pub mtu: Option<u32>,
    pub mac_address: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LinkInfo {
    pub index: u32,
    pub name: String,
    pub kind: String,
    pub operational_state: String,
    pub setup_state: String,
}

/// Wrapper enum for all managed device types
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "device_type", rename_all = "lowercase")]
pub enum NetworkDevice {
    Bridge(BridgeConfig),
    Bond(BondConfig),
    Vlan(VlanConfig),
    Macvtap(MacvtapConfig),
    Tap(TapConfig),
}

// ─── Port forwarding (nftables DNAT) ─────────────────────────────────────────

/// Protocol for port forwarding rules
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Protocol {
    #[default]
    Tcp,
    Udp,
    Both,
}

impl Protocol {
    pub fn as_str(&self) -> &str {
        match self {
            Protocol::Tcp => "tcp",
            Protocol::Udp => "udp",
            Protocol::Both => "both",
        }
    }
}

/// A port forwarding configuration (DNAT rule via nftables)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PortForwardConfig {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub protocol: Protocol,
    pub host_port: u16,
    pub guest_ip: String,
    pub guest_port: u16,
    /// Optional: restrict to traffic arriving on this interface
    #[serde(default)]
    pub interface: Option<String>,
    #[serde(default = "default_enabled")]
    pub enabled: bool,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub created: String,
    #[serde(default)]
    pub updated: String,
    /// False when discovered from live nftables but not stored in zyvor-fabricd.
    #[serde(default = "default_true")]
    pub managed: bool,
}

fn default_enabled() -> bool {
    true
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreatePortForwardRequest {
    pub name: String,
    #[serde(default)]
    pub protocol: Protocol,
    pub host_port: u16,
    pub guest_ip: String,
    pub guest_port: u16,
    #[serde(default)]
    pub interface: Option<String>,
    #[serde(default = "default_enabled")]
    pub enabled: bool,
    #[serde(default)]
    pub description: Option<String>,
}

// ─── VXLAN ────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VxlanConfig {
    pub id: String,
    pub name: String,
    pub vni: u32,
    #[serde(default)]
    pub remote: Option<String>,
    #[serde(default)]
    pub local: Option<String>,
    #[serde(default)]
    pub port: Option<u16>,
    #[serde(default)]
    pub parent_interface: Option<String>,
    #[serde(default)]
    pub mtu: Option<u32>,
    #[serde(default)]
    pub addresses: Vec<String>,
    #[serde(default)]
    pub gateway: Option<String>,
    #[serde(default)]
    pub dns: Vec<String>,
    #[serde(default)]
    pub dhcp: DhcpMode,
    #[serde(default)]
    pub created: String,
    #[serde(default)]
    pub updated: String,
    #[serde(default = "default_true")]
    pub managed: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateVxlanRequest {
    pub name: String,
    pub vni: u32,
    #[serde(default)]
    pub remote: Option<String>,
    #[serde(default)]
    pub local: Option<String>,
    #[serde(default)]
    pub port: Option<u16>,
    #[serde(default)]
    pub parent_interface: Option<String>,
    #[serde(default)]
    pub mtu: Option<u32>,
    #[serde(default)]
    pub addresses: Vec<String>,
    #[serde(default)]
    pub gateway: Option<String>,
    #[serde(default)]
    pub dns: Vec<String>,
    #[serde(default)]
    pub dhcp: DhcpMode,
}

// ─── SR-IOV ───────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SriovConfig {
    pub id: String,
    pub pf_name: String,
    pub num_vfs: u32,
    #[serde(default)]
    pub vf_configs: Vec<VfConfig>,
    #[serde(default)]
    pub created: String,
    #[serde(default)]
    pub updated: String,
    #[serde(default = "default_true")]
    pub managed: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VfConfig {
    pub vf_index: u32,
    #[serde(default)]
    pub mac_address: Option<String>,
    #[serde(default)]
    pub vlan: Option<u16>,
    #[serde(default)]
    pub qos: Option<u32>,
    #[serde(default)]
    pub spoofchk: Option<bool>,
    #[serde(default)]
    pub trust: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateSriovRequest {
    pub pf_name: String,
    pub num_vfs: u32,
    #[serde(default)]
    pub vf_configs: Vec<VfConfig>,
}

// ─── Parsed config file ──────────────────────────────────────────────────────

/// A parsed section from a systemd-networkd config file
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ParsedSection {
    pub name: String,
    pub entries: Vec<(String, String)>,
}

/// A fully parsed systemd-networkd config file
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ParsedConfigFile {
    pub filename: String,
    pub file_type: String,
    pub sections: Vec<ParsedSection>,
}
