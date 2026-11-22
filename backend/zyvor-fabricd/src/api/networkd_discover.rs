// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: LicenseRef-Zyvor-Production-2.0

use std::collections::HashSet;

use networking::host_discovery::{self, HostDeviceType, HostNetDevice};
use networking::host_vxlan_sriov::{self, DiscoveredSriovPf, DiscoveredVxlan};
use networking::models::{
    BondConfig, BondMode, BridgeConfig, DhcpMode, LinkFileConfig, MacvtapConfig, MacvtapMode,
    NetworkFileConfig, PortForwardConfig, SriovConfig, TapConfig, VlanConfig, VxlanConfig,
};
use networking::nftables::NftManager;

use crate::api::networkd::networkd_manager;
use crate::server::AppState;

pub fn is_host_managed_id(id: &str) -> bool {
    id.starts_with("host:")
}

pub fn discover_host(state: &AppState) -> Vec<HostNetDevice> {
    networkd_manager(state)
        .discover_host_interfaces()
        .unwrap_or_else(|e| {
            tracing::warn!("host interface discovery failed: {}", e);
            Vec::new()
        })
}

pub fn merge_bridges(state: &AppState, mut items: Vec<BridgeConfig>) -> Vec<BridgeConfig> {
    let known: HashSet<String> = items.iter().map(|b| b.name.clone()).collect();
    for d in discover_host(state)
        .into_iter()
        .filter(|d| d.device_type == HostDeviceType::Bridge)
    {
        if known.contains(&d.name) {
            continue;
        }
        items.push(BridgeConfig {
            id: format!("host:{}", d.name),
            name: d.name,
            stp: None,
            forward_delay_sec: None,
            hello_time_sec: None,
            max_age_sec: None,
            vlan_filtering: None,
            mtu: None,
            mac_address: d.mac,
            addresses: d.addresses,
            gateway: None,
            dns: Vec::new(),
            dhcp: DhcpMode::default(),
            created: String::new(),
            updated: String::new(),
            managed: false,
            operational_state: Some(d.operstate),
        });
    }
    items
}

pub fn merge_bonds(state: &AppState, mut items: Vec<BondConfig>) -> Vec<BondConfig> {
    let known: HashSet<String> = items.iter().map(|b| b.name.clone()).collect();
    for d in discover_host(state)
        .into_iter()
        .filter(|d| d.device_type == HostDeviceType::Bond)
    {
        if known.contains(&d.name) {
            continue;
        }
        items.push(BondConfig {
            id: format!("host:{}", d.name),
            name: d.name,
            mode: BondMode::default(),
            mii_monitor_sec: None,
            up_delay_sec: None,
            down_delay_sec: None,
            lacp_rate: None,
            transmit_hash_policy: None,
            min_links: None,
            primary_slave: None,
            slave_interfaces: Vec::new(),
            mtu: None,
            mac_address: d.mac,
            addresses: d.addresses,
            gateway: None,
            dns: Vec::new(),
            dhcp: DhcpMode::default(),
            routes: Vec::new(),
            created: String::new(),
            updated: String::new(),
            managed: false,
            operational_state: Some(d.operstate),
        });
    }
    items
}

pub fn merge_vlans(state: &AppState, mut items: Vec<VlanConfig>) -> Vec<VlanConfig> {
    let known: HashSet<String> = items.iter().map(|v| v.name.clone()).collect();
    for d in discover_host(state)
        .into_iter()
        .filter(|d| d.device_type == HostDeviceType::Vlan)
    {
        if known.contains(&d.name) {
            continue;
        }
        items.push(VlanConfig {
            id: format!("host:{}", d.name),
            name: d.name.clone(),
            vlan_id: d.vlan_id.unwrap_or(0),
            parent_interface: d.parent.clone().unwrap_or_else(|| "unknown".into()),
            mtu: None,
            addresses: d.addresses,
            gateway: None,
            dns: Vec::new(),
            dhcp: DhcpMode::default(),
            created: String::new(),
            updated: String::new(),
            managed: false,
            operational_state: Some(d.operstate),
        });
    }
    items
}

pub fn merge_macvtaps(state: &AppState, mut items: Vec<MacvtapConfig>) -> Vec<MacvtapConfig> {
    let known: HashSet<String> = items.iter().map(|m| m.name.clone()).collect();
    for d in discover_host(state)
        .into_iter()
        .filter(|d| d.device_type == HostDeviceType::Macvtap)
    {
        if known.contains(&d.name) {
            continue;
        }
        items.push(MacvtapConfig {
            id: format!("host:{}", d.name),
            name: d.name,
            parent_interface: d.parent.clone().unwrap_or_else(|| "-".into()),
            mode: MacvtapMode::Bridge,
            mtu: None,
            mac_address: d.mac,
            created: String::new(),
            updated: String::new(),
            managed: false,
            operational_state: Some(d.operstate),
        });
    }
    items
}

pub fn merge_taps(state: &AppState, mut items: Vec<TapConfig>) -> Vec<TapConfig> {
    let known: HashSet<String> = items.iter().map(|t| t.name.clone()).collect();
    for d in discover_host(state)
        .into_iter()
        .filter(|d| d.device_type == HostDeviceType::Tap)
    {
        if known.contains(&d.name) {
            continue;
        }
        items.push(TapConfig {
            id: format!("host:{}", d.name),
            name: d.name,
            user: None,
            group: None,
            multi_queue: None,
            vnet_hdr: None,
            bridge: None,
            mtu: None,
            mac_address: d.mac,
            created: String::new(),
            updated: String::new(),
            managed: false,
            operational_state: Some(d.operstate),
        });
    }
    items
}

pub fn merge_netfiles(
    state: &AppState,
    mut items: Vec<NetworkFileConfig>,
) -> Vec<NetworkFileConfig> {
    let known: HashSet<String> = items.iter().map(|n| n.match_name.clone()).collect();
    for d in discover_host(state).into_iter().filter(|d| {
        matches!(
            d.device_type,
            HostDeviceType::Physical | HostDeviceType::Veth
        )
    }) {
        if known.contains(&d.name) {
            continue;
        }
        let desc = match d.device_type {
            HostDeviceType::Veth => Some("Container virtual interface".into()),
            _ => Some("Host physical interface".into()),
        };
        items.push(NetworkFileConfig {
            id: format!("host:{}", d.name),
            match_name: d.name,
            match_mac: d.mac,
            addresses: d.addresses,
            gateway: None,
            dns: Vec::new(),
            dhcp: DhcpMode::default(),
            bridge: None,
            bond: None,
            mtu: None,
            routes: Vec::new(),
            description: desc,
            created: String::new(),
            updated: String::new(),
            managed: false,
            operational_state: Some(d.operstate),
        });
    }
    items
}

pub fn merge_link_files(state: &AppState, mut items: Vec<LinkFileConfig>) -> Vec<LinkFileConfig> {
    let known_files: HashSet<String> = items.iter().filter_map(|l| l.source_file.clone()).collect();
    let dir = &state.config.network.networkd_config_dir;
    for filename in host_discovery::list_systemd_network_files(dir) {
        if known_files.contains(&filename) {
            continue;
        }
        items.push(LinkFileConfig {
            id: format!("host:{}", filename),
            match_mac: None,
            match_path: None,
            match_driver: None,
            match_original_name: None,
            name: None,
            mtu: None,
            mac_address: None,
            wake_on_lan: None,
            description: Some("systemd-networkd file on host".into()),
            created: String::new(),
            updated: String::new(),
            managed: false,
            source_file: Some(filename),
        });
    }
    items
}

pub fn find_host_bridge(state: &AppState, id: &str) -> Option<BridgeConfig> {
    merge_bridges(state, vec![])
        .into_iter()
        .find(|b| b.id == id)
}

pub fn find_host_bond(state: &AppState, id: &str) -> Option<BondConfig> {
    merge_bonds(state, vec![]).into_iter().find(|b| b.id == id)
}

pub fn find_host_vlan(state: &AppState, id: &str) -> Option<VlanConfig> {
    merge_vlans(state, vec![]).into_iter().find(|v| v.id == id)
}

pub fn find_host_macvtap(state: &AppState, id: &str) -> Option<MacvtapConfig> {
    merge_macvtaps(state, vec![])
        .into_iter()
        .find(|m| m.id == id)
}

pub fn find_host_tap(state: &AppState, id: &str) -> Option<TapConfig> {
    merge_taps(state, vec![]).into_iter().find(|t| t.id == id)
}

pub fn find_host_netfile(state: &AppState, id: &str) -> Option<NetworkFileConfig> {
    merge_netfiles(state, vec![])
        .into_iter()
        .find(|n| n.id == id)
}

pub fn find_host_link_file(state: &AppState, id: &str) -> Option<LinkFileConfig> {
    merge_link_files(state, vec![])
        .into_iter()
        .find(|l| l.id == id)
}

pub fn find_host_port_forward(id: &str) -> Option<PortForwardConfig> {
    let nft = NftManager::new();
    let mut all = nft.discover_dnat_rules().unwrap_or_default();
    all.extend(nft.discover_external_dnat_rules().unwrap_or_default());
    all.into_iter().find(|p| p.id == id)
}

pub fn merge_vxlans(_state: &AppState, mut items: Vec<VxlanConfig>) -> Vec<VxlanConfig> {
    let mut known_names: HashSet<String> = items.iter().map(|v| v.name.clone()).collect();

    for d in host_vxlan_sriov::discover_host_vxlans().unwrap_or_else(|e| {
        tracing::warn!("host VXLAN discovery failed: {}", e);
        Vec::new()
    }) {
        if known_names.contains(&d.name) {
            continue;
        }
        known_names.insert(d.name.clone());
        items.push(vxlan_from_discovered(d));
    }
    items
}

pub fn merge_sriov(_state: &AppState, mut items: Vec<SriovConfig>) -> Vec<SriovConfig> {
    let mut known_pfs: HashSet<String> = items.iter().map(|s| s.pf_name.clone()).collect();

    for d in host_vxlan_sriov::discover_host_sriov().unwrap_or_else(|e| {
        tracing::warn!("host SR-IOV discovery failed: {}", e);
        Vec::new()
    }) {
        if known_pfs.contains(&d.pf_name) {
            continue;
        }
        known_pfs.insert(d.pf_name.clone());
        items.push(sriov_from_discovered(d));
    }
    items
}

fn vxlan_from_discovered(d: DiscoveredVxlan) -> VxlanConfig {
    VxlanConfig {
        id: format!("host:{}", d.name),
        name: d.name,
        vni: d.vni,
        remote: d.remote,
        local: d.local,
        port: d.port,
        parent_interface: d.parent_interface,
        mtu: None,
        addresses: d.addresses,
        gateway: None,
        dns: Vec::new(),
        dhcp: DhcpMode::default(),
        created: String::new(),
        updated: String::new(),
        managed: false,
    }
}

fn sriov_from_discovered(d: DiscoveredSriovPf) -> SriovConfig {
    SriovConfig {
        id: format!("host:{}", d.pf_name),
        pf_name: d.pf_name,
        num_vfs: d.num_vfs,
        vf_configs: Vec::new(),
        created: String::new(),
        updated: String::new(),
        managed: false,
    }
}

pub fn find_host_vxlan(_state: &AppState, id: &str) -> Option<VxlanConfig> {
    merge_vxlans(_state, vec![])
        .into_iter()
        .find(|v| v.id == id)
}

pub fn find_host_sriov(_state: &AppState, id: &str) -> Option<SriovConfig> {
    merge_sriov(_state, vec![]).into_iter().find(|s| s.id == id)
}

pub fn merge_port_forwards(mut items: Vec<PortForwardConfig>) -> Vec<PortForwardConfig> {
    let mut known_names: HashSet<String> = items.iter().map(|p| p.name.clone()).collect();
    let mut known_rules: HashSet<(u16, String, u16)> = items
        .iter()
        .map(|p| (p.host_port, p.guest_ip.clone(), p.guest_port))
        .collect();

    let nft = NftManager::new();
    let mut discovered = nft.discover_dnat_rules().unwrap_or_else(|e| {
        tracing::warn!("zyvor-fabricd nftables discovery failed: {}", e);
        Vec::new()
    });
    let external = nft.discover_external_dnat_rules().unwrap_or_else(|e| {
        tracing::warn!("external nftables discovery failed: {}", e);
        Vec::new()
    });
    discovered.extend(external);

    for d in discovered {
        if known_names.contains(&d.name) {
            continue;
        }
        if known_rules.contains(&(d.host_port, d.guest_ip.clone(), d.guest_port)) {
            continue;
        }
        known_names.insert(d.name.clone());
        known_rules.insert((d.host_port, d.guest_ip.clone(), d.guest_port));
        items.push(d);
    }
    items
}
