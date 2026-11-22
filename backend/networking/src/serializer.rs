// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: LicenseRef-Zyvor-Production-2.0

use crate::models::*;

// ─── Bridge ───────────────────────────────────────────────────────────────────

/// Generate the .netdev file for a bridge
pub fn bridge_netdev(cfg: &BridgeConfig) -> String {
    let mut out = String::new();
    out.push_str("[NetDev]\n");
    out.push_str(&format!("Name={}\n", cfg.name));
    out.push_str("Kind=bridge\n");
    if let Some(mtu) = cfg.mtu {
        out.push_str(&format!("MTUBytes={}\n", mtu));
    }
    if let Some(ref mac) = cfg.mac_address {
        out.push_str(&format!("MACAddress={}\n", mac));
    }

    // [Bridge] section
    let has_bridge_opts = cfg.stp.is_some()
        || cfg.forward_delay_sec.is_some()
        || cfg.hello_time_sec.is_some()
        || cfg.max_age_sec.is_some()
        || cfg.vlan_filtering.is_some();

    if has_bridge_opts {
        out.push_str("\n[Bridge]\n");
        if let Some(stp) = cfg.stp {
            out.push_str(&format!("STP={}\n", if stp { "yes" } else { "no" }));
        }
        if let Some(fd) = cfg.forward_delay_sec {
            out.push_str(&format!("ForwardDelaySec={}\n", fd));
        }
        if let Some(ht) = cfg.hello_time_sec {
            out.push_str(&format!("HelloTimeSec={}\n", ht));
        }
        if let Some(ma) = cfg.max_age_sec {
            out.push_str(&format!("MaxAgeSec={}\n", ma));
        }
        if let Some(vf) = cfg.vlan_filtering {
            out.push_str(&format!(
                "VLANFiltering={}\n",
                if vf { "yes" } else { "no" }
            ));
        }
    }

    out
}

/// Generate the .network file for a bridge
pub fn bridge_network(cfg: &BridgeConfig) -> String {
    let mut out = String::new();
    out.push_str("[Match]\n");
    out.push_str(&format!("Name={}\n", cfg.name));

    out.push_str("\n[Network]\n");
    for addr in &cfg.addresses {
        out.push_str(&format!("Address={}\n", addr));
    }
    if let Some(ref gw) = cfg.gateway {
        out.push_str(&format!("Gateway={}\n", gw));
    }
    for d in &cfg.dns {
        out.push_str(&format!("DNS={}\n", d));
    }
    if cfg.dhcp != DhcpMode::No {
        out.push_str(&format!("DHCP={}\n", cfg.dhcp.as_str()));
    }

    out
}

// ─── VLAN ─────────────────────────────────────────────────────────────────────

/// Generate the .netdev file for a VLAN
pub fn vlan_netdev(cfg: &VlanConfig) -> String {
    let mut out = String::new();
    out.push_str("[NetDev]\n");
    out.push_str(&format!("Name={}\n", cfg.name));
    out.push_str("Kind=vlan\n");
    if let Some(mtu) = cfg.mtu {
        out.push_str(&format!("MTUBytes={}\n", mtu));
    }

    out.push_str("\n[VLAN]\n");
    out.push_str(&format!("Id={}\n", cfg.vlan_id));

    out
}

/// Generate the .network file for the VLAN's parent interface to attach the VLAN
pub fn vlan_parent_network(cfg: &VlanConfig, _prefix: &str) -> String {
    let mut out = String::new();
    out.push_str("[Match]\n");
    out.push_str(&format!("Name={}\n", cfg.parent_interface));

    out.push_str("\n[Network]\n");
    out.push_str(&format!("VLAN={}\n", cfg.name));

    out
}

/// Generate the .network file for the VLAN interface itself
pub fn vlan_network(cfg: &VlanConfig) -> String {
    let mut out = String::new();
    out.push_str("[Match]\n");
    out.push_str(&format!("Name={}\n", cfg.name));

    out.push_str("\n[Network]\n");
    for addr in &cfg.addresses {
        out.push_str(&format!("Address={}\n", addr));
    }
    if let Some(ref gw) = cfg.gateway {
        out.push_str(&format!("Gateway={}\n", gw));
    }
    for d in &cfg.dns {
        out.push_str(&format!("DNS={}\n", d));
    }
    if cfg.dhcp != DhcpMode::No {
        out.push_str(&format!("DHCP={}\n", cfg.dhcp.as_str()));
    }

    out
}

// ─── Macvtap ──────────────────────────────────────────────────────────────────

/// Generate the .netdev file for a macvtap device
pub fn macvtap_netdev(cfg: &MacvtapConfig) -> String {
    let mut out = String::new();
    out.push_str("[NetDev]\n");
    out.push_str(&format!("Name={}\n", cfg.name));
    out.push_str("Kind=macvtap\n");
    if let Some(mtu) = cfg.mtu {
        out.push_str(&format!("MTUBytes={}\n", mtu));
    }
    if let Some(ref mac) = cfg.mac_address {
        out.push_str(&format!("MACAddress={}\n", mac));
    }

    out.push_str("\n[MACVTAP]\n");
    out.push_str(&format!("Mode={}\n", cfg.mode.as_str()));

    out
}

/// Generate the .network file for the macvtap's parent interface
pub fn macvtap_parent_network(cfg: &MacvtapConfig) -> String {
    let mut out = String::new();
    out.push_str("[Match]\n");
    out.push_str(&format!("Name={}\n", cfg.parent_interface));

    out.push_str("\n[Network]\n");
    out.push_str(&format!("MACVTAP={}\n", cfg.name));

    out
}

/// Generate the .network file for the macvtap interface itself (minimal)
pub fn macvtap_network(cfg: &MacvtapConfig) -> String {
    let mut out = String::new();
    out.push_str("[Match]\n");
    out.push_str(&format!("Name={}\n", cfg.name));

    out.push_str("\n[Network]\n");

    out
}

// ─── Tap ──────────────────────────────────────────────────────────────────────

/// Generate the .netdev file for a tap device
pub fn tap_netdev(cfg: &TapConfig) -> String {
    let mut out = String::new();
    out.push_str("[NetDev]\n");
    out.push_str(&format!("Name={}\n", cfg.name));
    out.push_str("Kind=tap\n");
    if let Some(mtu) = cfg.mtu {
        out.push_str(&format!("MTUBytes={}\n", mtu));
    }
    if let Some(ref mac) = cfg.mac_address {
        out.push_str(&format!("MACAddress={}\n", mac));
    }

    out.push_str("\n[Tap]\n");
    if let Some(ref user) = cfg.user {
        out.push_str(&format!("User={}\n", user));
    }
    if let Some(ref group) = cfg.group {
        out.push_str(&format!("Group={}\n", group));
    }
    if let Some(mq) = cfg.multi_queue {
        out.push_str(&format!("MultiQueue={}\n", if mq { "yes" } else { "no" }));
    }
    if let Some(vnet) = cfg.vnet_hdr {
        out.push_str(&format!("VNetHeader={}\n", if vnet { "yes" } else { "no" }));
    }

    out
}

/// Generate the .network file for a tap device
pub fn tap_network(cfg: &TapConfig) -> String {
    let mut out = String::new();
    out.push_str("[Match]\n");
    out.push_str(&format!("Name={}\n", cfg.name));

    out.push_str("\n[Network]\n");
    if let Some(ref bridge) = cfg.bridge {
        out.push_str(&format!("Bridge={}\n", bridge));
    }

    out
}

// ─── Route helper ─────────────────────────────────────────────────────────────

/// Append [Route] sections for a list of route entries
fn append_routes(out: &mut String, routes: &[RouteEntry]) {
    for route in routes {
        out.push_str("\n[Route]\n");
        out.push_str(&format!("Destination={}\n", route.destination));
        if let Some(ref gw) = route.gateway {
            out.push_str(&format!("Gateway={}\n", gw));
        }
        if let Some(metric) = route.metric {
            out.push_str(&format!("Metric={}\n", metric));
        }
        if let Some(ref scope) = route.scope {
            out.push_str(&format!("Scope={}\n", scope));
        }
    }
}

// ─── Bond ─────────────────────────────────────────────────────────────────────

/// Generate the .netdev file for a bond
pub fn bond_netdev(cfg: &BondConfig) -> String {
    let mut out = String::new();
    out.push_str("[NetDev]\n");
    out.push_str(&format!("Name={}\n", cfg.name));
    out.push_str("Kind=bond\n");
    if let Some(mtu) = cfg.mtu {
        out.push_str(&format!("MTUBytes={}\n", mtu));
    }
    if let Some(ref mac) = cfg.mac_address {
        out.push_str(&format!("MACAddress={}\n", mac));
    }

    out.push_str("\n[Bond]\n");
    out.push_str(&format!("Mode={}\n", cfg.mode.as_str()));
    if let Some(mii) = cfg.mii_monitor_sec {
        out.push_str(&format!("MIIMonitorSec={}\n", mii));
    }
    if let Some(up) = cfg.up_delay_sec {
        out.push_str(&format!("UpDelaySec={}\n", up));
    }
    if let Some(down) = cfg.down_delay_sec {
        out.push_str(&format!("DownDelaySec={}\n", down));
    }
    if let Some(ref rate) = cfg.lacp_rate {
        out.push_str(&format!("LACPTransmitRate={}\n", rate.as_str()));
    }
    if let Some(ref policy) = cfg.transmit_hash_policy {
        out.push_str(&format!("TransmitHashPolicy={}\n", policy.as_str()));
    }
    if let Some(min) = cfg.min_links {
        out.push_str(&format!("MinLinks={}\n", min));
    }

    out
}

/// Generate the .network file for the bond interface itself
pub fn bond_network(cfg: &BondConfig) -> String {
    let mut out = String::new();
    out.push_str("[Match]\n");
    out.push_str(&format!("Name={}\n", cfg.name));

    out.push_str("\n[Network]\n");
    for addr in &cfg.addresses {
        out.push_str(&format!("Address={}\n", addr));
    }
    if let Some(ref gw) = cfg.gateway {
        out.push_str(&format!("Gateway={}\n", gw));
    }
    for d in &cfg.dns {
        out.push_str(&format!("DNS={}\n", d));
    }
    if cfg.dhcp != DhcpMode::No {
        out.push_str(&format!("DHCP={}\n", cfg.dhcp.as_str()));
    }

    append_routes(&mut out, &cfg.routes);

    out
}

/// Generate a .network file for a slave interface to attach it to the bond
pub fn bond_slave_network(slave_name: &str, bond_name: &str) -> String {
    let mut out = String::new();
    out.push_str("[Match]\n");
    out.push_str(&format!("Name={}\n", slave_name));

    out.push_str("\n[Network]\n");
    out.push_str(&format!("Bond={}\n", bond_name));

    out
}

// ─── Network file (physical interface config) ────────────────────────────────

/// Generate a .network file for configuring an existing physical interface
pub fn network_file(cfg: &NetworkFileConfig) -> String {
    let mut out = String::new();
    out.push_str("[Match]\n");
    out.push_str(&format!("Name={}\n", cfg.match_name));
    if let Some(ref mac) = cfg.match_mac {
        out.push_str(&format!("MACAddress={}\n", mac));
    }

    out.push_str("\n[Network]\n");
    for addr in &cfg.addresses {
        out.push_str(&format!("Address={}\n", addr));
    }
    if let Some(ref gw) = cfg.gateway {
        out.push_str(&format!("Gateway={}\n", gw));
    }
    for d in &cfg.dns {
        out.push_str(&format!("DNS={}\n", d));
    }
    if cfg.dhcp != DhcpMode::No {
        out.push_str(&format!("DHCP={}\n", cfg.dhcp.as_str()));
    }
    if let Some(ref bridge) = cfg.bridge {
        out.push_str(&format!("Bridge={}\n", bridge));
    }
    if let Some(ref bond) = cfg.bond {
        out.push_str(&format!("Bond={}\n", bond));
    }

    if let Some(mtu) = cfg.mtu {
        out.push_str("\n[Link]\n");
        out.push_str(&format!("MTUBytes={}\n", mtu));
    }

    append_routes(&mut out, &cfg.routes);

    out
}

// ─── Link file (.link) ──────────────────────────────────────────────────────

/// Generate a .link file for renaming/configuring a physical interface
pub fn link_file(cfg: &LinkFileConfig) -> String {
    let mut out = String::new();
    out.push_str("[Match]\n");
    if let Some(ref mac) = cfg.match_mac {
        out.push_str(&format!("MACAddress={}\n", mac));
    }
    if let Some(ref path) = cfg.match_path {
        out.push_str(&format!("Path={}\n", path));
    }
    if let Some(ref driver) = cfg.match_driver {
        out.push_str(&format!("Driver={}\n", driver));
    }
    if let Some(ref orig) = cfg.match_original_name {
        out.push_str(&format!("OriginalName={}\n", orig));
    }

    out.push_str("\n[Link]\n");
    if let Some(ref name) = cfg.name {
        out.push_str(&format!("Name={}\n", name));
    }
    if let Some(mtu) = cfg.mtu {
        out.push_str(&format!("MTUBytes={}\n", mtu));
    }
    if let Some(ref mac) = cfg.mac_address {
        out.push_str(&format!("MACAddress={}\n", mac));
    }
    if let Some(ref wol) = cfg.wake_on_lan {
        out.push_str(&format!("WakeOnLan={}\n", wol));
    }

    out
}

// ─── VXLAN ────────────────────────────────────────────────────────────────────

/// Generate the .netdev file for a VXLAN device
pub fn vxlan_netdev(cfg: &VxlanConfig) -> String {
    let mut out = String::new();
    out.push_str("[NetDev]\n");
    out.push_str(&format!("Name={}\n", cfg.name));
    out.push_str("Kind=vxlan\n");
    if let Some(mtu) = cfg.mtu {
        out.push_str(&format!("MTUBytes={}\n", mtu));
    }

    out.push_str("\n[VXLAN]\n");
    out.push_str(&format!("VNI={}\n", cfg.vni));
    if let Some(ref remote) = cfg.remote {
        out.push_str(&format!("Remote={}\n", remote));
    }
    if let Some(ref local) = cfg.local {
        out.push_str(&format!("Local={}\n", local));
    }
    if let Some(port) = cfg.port {
        out.push_str(&format!("DestinationPort={}\n", port));
    }

    out
}

/// Generate the .network file for a VXLAN's parent interface
pub fn vxlan_parent_network(cfg: &VxlanConfig) -> String {
    let mut out = String::new();
    if let Some(ref parent) = cfg.parent_interface {
        out.push_str("[Match]\n");
        out.push_str(&format!("Name={}\n", parent));
        out.push_str("\n[Network]\n");
        out.push_str(&format!("VXLAN={}\n", cfg.name));
    }
    out
}

/// Generate the .network file for the VXLAN interface itself
pub fn vxlan_network(cfg: &VxlanConfig) -> String {
    let mut out = String::new();
    out.push_str("[Match]\n");
    out.push_str(&format!("Name={}\n", cfg.name));

    out.push_str("\n[Network]\n");
    for addr in &cfg.addresses {
        out.push_str(&format!("Address={}\n", addr));
    }
    if let Some(ref gw) = cfg.gateway {
        out.push_str(&format!("Gateway={}\n", gw));
    }
    for d in &cfg.dns {
        out.push_str(&format!("DNS={}\n", d));
    }
    if cfg.dhcp != DhcpMode::No {
        out.push_str(&format!("DHCP={}\n", cfg.dhcp.as_str()));
    }

    out
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn make_bridge() -> BridgeConfig {
        BridgeConfig {
            id: "test-id".into(),
            name: "br0".into(),
            stp: Some(true),
            forward_delay_sec: Some(15),
            hello_time_sec: None,
            max_age_sec: None,
            vlan_filtering: Some(true),
            mtu: Some(1500),
            mac_address: Some("52:54:00:aa:bb:cc".into()),
            addresses: vec!["10.0.0.1/24".into()],
            gateway: Some("10.0.0.254".into()),
            dns: vec!["8.8.8.8".into()],
            dhcp: DhcpMode::No,
            created: String::new(),
            updated: String::new(),
            managed: true,
            operational_state: None,
        }
    }

    #[test]
    fn test_bridge_netdev() {
        let cfg = make_bridge();
        let out = bridge_netdev(&cfg);
        assert!(out.contains("[NetDev]\n"));
        assert!(out.contains("Name=br0\n"));
        assert!(out.contains("Kind=bridge\n"));
        assert!(out.contains("MTUBytes=1500\n"));
        assert!(out.contains("MACAddress=52:54:00:aa:bb:cc\n"));
        assert!(out.contains("[Bridge]\n"));
        assert!(out.contains("STP=yes\n"));
        assert!(out.contains("ForwardDelaySec=15\n"));
        assert!(out.contains("VLANFiltering=yes\n"));
    }

    #[test]
    fn test_bridge_netdev_minimal() {
        let cfg = BridgeConfig {
            id: "id".into(),
            name: "br-min".into(),
            stp: None,
            forward_delay_sec: None,
            hello_time_sec: None,
            max_age_sec: None,
            vlan_filtering: None,
            mtu: None,
            mac_address: None,
            addresses: vec![],
            gateway: None,
            dns: vec![],
            dhcp: DhcpMode::No,
            created: String::new(),
            updated: String::new(),
            managed: true,
            operational_state: None,
        };
        let out = bridge_netdev(&cfg);
        assert!(out.contains("Name=br-min\n"));
        assert!(out.contains("Kind=bridge\n"));
        assert!(!out.contains("[Bridge]"));
        assert!(!out.contains("MTUBytes"));
    }

    #[test]
    fn test_bridge_network() {
        let cfg = make_bridge();
        let out = bridge_network(&cfg);
        assert!(out.contains("[Match]\n"));
        assert!(out.contains("Name=br0\n"));
        assert!(out.contains("[Network]\n"));
        assert!(out.contains("Address=10.0.0.1/24\n"));
        assert!(out.contains("Gateway=10.0.0.254\n"));
        assert!(out.contains("DNS=8.8.8.8\n"));
        assert!(!out.contains("DHCP="));
    }

    #[test]
    fn test_bridge_network_dhcp() {
        let mut cfg = make_bridge();
        cfg.dhcp = DhcpMode::Yes;
        let out = bridge_network(&cfg);
        assert!(out.contains("DHCP=yes\n"));
    }

    #[test]
    fn test_vlan_netdev() {
        let cfg = VlanConfig {
            id: "id".into(),
            name: "vlan100".into(),
            vlan_id: 100,
            parent_interface: "eth0".into(),
            mtu: Some(1400),
            addresses: vec![],
            gateway: None,
            dns: vec![],
            dhcp: DhcpMode::No,
            created: String::new(),
            updated: String::new(),
            managed: true,
            operational_state: None,
        };
        let out = vlan_netdev(&cfg);
        assert!(out.contains("Name=vlan100\n"));
        assert!(out.contains("Kind=vlan\n"));
        assert!(out.contains("MTUBytes=1400\n"));
        assert!(out.contains("[VLAN]\n"));
        assert!(out.contains("Id=100\n"));
    }

    #[test]
    fn test_vlan_parent_network() {
        let cfg = VlanConfig {
            id: "id".into(),
            name: "vlan100".into(),
            vlan_id: 100,
            parent_interface: "eth0".into(),
            mtu: None,
            addresses: vec![],
            gateway: None,
            dns: vec![],
            dhcp: DhcpMode::No,
            created: String::new(),
            updated: String::new(),
            managed: true,
            operational_state: None,
        };
        let out = vlan_parent_network(&cfg, "50-zyvor-fabricd-");
        assert!(out.contains("Name=eth0\n"));
        assert!(out.contains("VLAN=vlan100\n"));
    }

    #[test]
    fn test_vlan_network_with_addresses() {
        let cfg = VlanConfig {
            id: "id".into(),
            name: "vlan200".into(),
            vlan_id: 200,
            parent_interface: "eth0".into(),
            mtu: None,
            addresses: vec!["192.168.200.1/24".into()],
            gateway: Some("192.168.200.254".into()),
            dns: vec!["1.1.1.1".into(), "8.8.8.8".into()],
            dhcp: DhcpMode::Ipv4,
            created: String::new(),
            updated: String::new(),
            managed: true,
            operational_state: None,
        };
        let out = vlan_network(&cfg);
        assert!(out.contains("Name=vlan200\n"));
        assert!(out.contains("Address=192.168.200.1/24\n"));
        assert!(out.contains("Gateway=192.168.200.254\n"));
        assert!(out.contains("DNS=1.1.1.1\n"));
        assert!(out.contains("DNS=8.8.8.8\n"));
        assert!(out.contains("DHCP=ipv4\n"));
    }

    #[test]
    fn test_macvtap_netdev() {
        let cfg = MacvtapConfig {
            id: "id".into(),
            name: "mvt0".into(),
            parent_interface: "eth0".into(),
            mode: MacvtapMode::Bridge,
            mtu: Some(1500),
            mac_address: Some("52:54:00:11:22:33".into()),
            created: String::new(),
            updated: String::new(),
            managed: true,
            operational_state: None,
        };
        let out = macvtap_netdev(&cfg);
        assert!(out.contains("Name=mvt0\n"));
        assert!(out.contains("Kind=macvtap\n"));
        assert!(out.contains("MTUBytes=1500\n"));
        assert!(out.contains("MACAddress=52:54:00:11:22:33\n"));
        assert!(out.contains("[MACVTAP]\n"));
        assert!(out.contains("Mode=bridge\n"));
    }

    #[test]
    fn test_macvtap_passthru() {
        let cfg = MacvtapConfig {
            id: "id".into(),
            name: "mvt1".into(),
            parent_interface: "eno1".into(),
            mode: MacvtapMode::Passthru,
            mtu: None,
            mac_address: None,
            created: String::new(),
            updated: String::new(),
            managed: true,
            operational_state: None,
        };
        let out = macvtap_netdev(&cfg);
        assert!(out.contains("Mode=passthru\n"));
        assert!(!out.contains("MTUBytes"));
        assert!(!out.contains("MACAddress"));
    }

    #[test]
    fn test_macvtap_parent_network() {
        let cfg = MacvtapConfig {
            id: "id".into(),
            name: "mvt0".into(),
            parent_interface: "eth0".into(),
            mode: MacvtapMode::Bridge,
            mtu: None,
            mac_address: None,
            created: String::new(),
            updated: String::new(),
            managed: true,
            operational_state: None,
        };
        let out = macvtap_parent_network(&cfg);
        assert!(out.contains("Name=eth0\n"));
        assert!(out.contains("MACVTAP=mvt0\n"));
    }

    #[test]
    fn test_tap_netdev() {
        let cfg = TapConfig {
            id: "id".into(),
            name: "tap0".into(),
            user: Some("qemu".into()),
            group: Some("kvm".into()),
            multi_queue: Some(true),
            vnet_hdr: Some(true),
            bridge: None,
            mtu: Some(1500),
            mac_address: None,
            created: String::new(),
            updated: String::new(),
            managed: true,
            operational_state: None,
        };
        let out = tap_netdev(&cfg);
        assert!(out.contains("Name=tap0\n"));
        assert!(out.contains("Kind=tap\n"));
        assert!(out.contains("MTUBytes=1500\n"));
        assert!(out.contains("[Tap]\n"));
        assert!(out.contains("User=qemu\n"));
        assert!(out.contains("Group=kvm\n"));
        assert!(out.contains("MultiQueue=yes\n"));
        assert!(out.contains("VNetHeader=yes\n"));
    }

    #[test]
    fn test_tap_netdev_minimal() {
        let cfg = TapConfig {
            id: "id".into(),
            name: "tap-min".into(),
            user: None,
            group: None,
            multi_queue: None,
            vnet_hdr: None,
            bridge: None,
            mtu: None,
            mac_address: None,
            created: String::new(),
            updated: String::new(),
            managed: true,
            operational_state: None,
        };
        let out = tap_netdev(&cfg);
        assert!(out.contains("Name=tap-min\n"));
        assert!(out.contains("Kind=tap\n"));
        assert!(out.contains("[Tap]\n"));
        assert!(!out.contains("User="));
        assert!(!out.contains("MultiQueue"));
    }

    #[test]
    fn test_tap_network_with_bridge() {
        let cfg = TapConfig {
            id: "id".into(),
            name: "tap0".into(),
            user: None,
            group: None,
            multi_queue: None,
            vnet_hdr: None,
            bridge: Some("br0".into()),
            mtu: None,
            mac_address: None,
            created: String::new(),
            updated: String::new(),
            managed: true,
            operational_state: None,
        };
        let out = tap_network(&cfg);
        assert!(out.contains("Name=tap0\n"));
        assert!(out.contains("Bridge=br0\n"));
    }

    #[test]
    fn test_tap_network_no_bridge() {
        let cfg = TapConfig {
            id: "id".into(),
            name: "tap1".into(),
            user: None,
            group: None,
            multi_queue: None,
            vnet_hdr: None,
            bridge: None,
            mtu: None,
            mac_address: None,
            created: String::new(),
            updated: String::new(),
            managed: true,
            operational_state: None,
        };
        let out = tap_network(&cfg);
        assert!(out.contains("Name=tap1\n"));
        assert!(!out.contains("Bridge="));
    }

    // ── Bond tests ───────────────────────────────────────────────────────

    #[test]
    fn test_bond_netdev_8023ad() {
        let cfg = BondConfig {
            id: "id".into(),
            name: "bond0".into(),
            mode: BondMode::Ieee8023ad,
            mii_monitor_sec: Some(100),
            up_delay_sec: Some(200),
            down_delay_sec: Some(200),
            lacp_rate: Some(LacpRate::Fast),
            transmit_hash_policy: Some(TransmitHashPolicy::Layer34),
            min_links: Some(1),
            primary_slave: None,
            slave_interfaces: vec!["eth0".into(), "eth1".into()],
            mtu: Some(9000),
            mac_address: None,
            addresses: vec![],
            gateway: None,
            dns: vec![],
            dhcp: DhcpMode::No,
            routes: vec![],
            created: String::new(),
            updated: String::new(),
            managed: true,
            operational_state: None,
        };
        let out = bond_netdev(&cfg);
        assert!(out.contains("Name=bond0\n"));
        assert!(out.contains("Kind=bond\n"));
        assert!(out.contains("MTUBytes=9000\n"));
        assert!(out.contains("[Bond]\n"));
        assert!(out.contains("Mode=802.3ad\n"));
        assert!(out.contains("MIIMonitorSec=100\n"));
        assert!(out.contains("UpDelaySec=200\n"));
        assert!(out.contains("DownDelaySec=200\n"));
        assert!(out.contains("LACPTransmitRate=fast\n"));
        assert!(out.contains("TransmitHashPolicy=layer3+4\n"));
        assert!(out.contains("MinLinks=1\n"));
    }

    #[test]
    fn test_bond_netdev_active_backup() {
        let cfg = BondConfig {
            id: "id".into(),
            name: "bond1".into(),
            mode: BondMode::ActiveBackup,
            mii_monitor_sec: Some(100),
            up_delay_sec: None,
            down_delay_sec: None,
            lacp_rate: None,
            transmit_hash_policy: None,
            min_links: None,
            primary_slave: None,
            slave_interfaces: vec![],
            mtu: None,
            mac_address: None,
            addresses: vec![],
            gateway: None,
            dns: vec![],
            dhcp: DhcpMode::No,
            routes: vec![],
            created: String::new(),
            updated: String::new(),
            managed: true,
            operational_state: None,
        };
        let out = bond_netdev(&cfg);
        assert!(out.contains("Mode=active-backup\n"));
        assert!(!out.contains("LACPTransmitRate"));
        assert!(!out.contains("TransmitHashPolicy"));
    }

    #[test]
    fn test_bond_network_with_routes() {
        let cfg = BondConfig {
            id: "id".into(),
            name: "bond0".into(),
            mode: BondMode::BalanceRr,
            mii_monitor_sec: None,
            up_delay_sec: None,
            down_delay_sec: None,
            lacp_rate: None,
            transmit_hash_policy: None,
            min_links: None,
            primary_slave: None,
            slave_interfaces: vec![],
            mtu: None,
            mac_address: None,
            addresses: vec!["10.0.0.1/24".into()],
            gateway: Some("10.0.0.254".into()),
            dns: vec!["8.8.8.8".into()],
            dhcp: DhcpMode::No,
            routes: vec![RouteEntry {
                destination: "172.16.0.0/12".into(),
                gateway: Some("10.0.0.1".into()),
                metric: Some(100),
                scope: None,
            }],
            created: String::new(),
            updated: String::new(),
            managed: true,
            operational_state: None,
        };
        let out = bond_network(&cfg);
        assert!(out.contains("Name=bond0\n"));
        assert!(out.contains("Address=10.0.0.1/24\n"));
        assert!(out.contains("Gateway=10.0.0.254\n"));
        assert!(out.contains("[Route]\n"));
        assert!(out.contains("Destination=172.16.0.0/12\n"));
        assert!(out.contains("Gateway=10.0.0.1\n"));
        assert!(out.contains("Metric=100\n"));
    }

    #[test]
    fn test_bond_slave_network() {
        let out = bond_slave_network("eth0", "bond0");
        assert!(out.contains("Name=eth0\n"));
        assert!(out.contains("Bond=bond0\n"));
    }

    // ── Network file tests ───────────────────────────────────────────────

    #[test]
    fn test_network_file_bridge_member() {
        let cfg = NetworkFileConfig {
            id: "id".into(),
            match_name: "enp3s0".into(),
            match_mac: None,
            addresses: vec![],
            gateway: None,
            dns: vec![],
            dhcp: DhcpMode::No,
            bridge: Some("br0".into()),
            bond: None,
            mtu: None,
            routes: vec![],
            description: None,
            created: String::new(),
            updated: String::new(),
            managed: true,
            operational_state: None,
        };
        let out = network_file(&cfg);
        assert!(out.contains("Name=enp3s0\n"));
        assert!(out.contains("Bridge=br0\n"));
        assert!(!out.contains("Bond="));
    }

    #[test]
    fn test_network_file_static_ip_with_routes() {
        let cfg = NetworkFileConfig {
            id: "id".into(),
            match_name: "eth0".into(),
            match_mac: Some("aa:bb:cc:dd:ee:ff".into()),
            addresses: vec!["192.168.1.10/24".into()],
            gateway: Some("192.168.1.1".into()),
            dns: vec!["1.1.1.1".into()],
            dhcp: DhcpMode::No,
            bridge: None,
            bond: None,
            mtu: Some(9000),
            routes: vec![RouteEntry {
                destination: "10.0.0.0/8".into(),
                gateway: Some("192.168.1.1".into()),
                metric: None,
                scope: Some("global".into()),
            }],
            description: None,
            created: String::new(),
            updated: String::new(),
            managed: true,
            operational_state: None,
        };
        let out = network_file(&cfg);
        assert!(out.contains("Name=eth0\n"));
        assert!(out.contains("MACAddress=aa:bb:cc:dd:ee:ff\n"));
        assert!(out.contains("Address=192.168.1.10/24\n"));
        assert!(out.contains("Gateway=192.168.1.1\n"));
        assert!(out.contains("DNS=1.1.1.1\n"));
        assert!(out.contains("[Link]\n"));
        assert!(out.contains("MTUBytes=9000\n"));
        assert!(out.contains("[Route]\n"));
        assert!(out.contains("Destination=10.0.0.0/8\n"));
        assert!(out.contains("Scope=global\n"));
    }

    #[test]
    fn test_network_file_dhcp() {
        let cfg = NetworkFileConfig {
            id: "id".into(),
            match_name: "eno1".into(),
            match_mac: None,
            addresses: vec![],
            gateway: None,
            dns: vec![],
            dhcp: DhcpMode::Yes,
            bridge: None,
            bond: None,
            mtu: None,
            routes: vec![],
            description: None,
            created: String::new(),
            updated: String::new(),
            managed: true,
            operational_state: None,
        };
        let out = network_file(&cfg);
        assert!(out.contains("Name=eno1\n"));
        assert!(out.contains("DHCP=yes\n"));
        assert!(!out.contains("[Link]"));
    }

    // ── Link file tests ─────────────────────────────────────────────────

    #[test]
    fn test_link_file_rename() {
        let cfg = LinkFileConfig {
            id: "id".into(),
            match_mac: Some("00:11:22:33:44:55".into()),
            match_path: None,
            match_driver: None,
            match_original_name: None,
            name: Some("lan0".into()),
            mtu: Some(9000),
            mac_address: None,
            wake_on_lan: Some("magic".into()),
            description: None,
            created: String::new(),
            updated: String::new(),
            managed: true,
            source_file: None,
        };
        let out = link_file(&cfg);
        assert!(out.contains("[Match]\n"));
        assert!(out.contains("MACAddress=00:11:22:33:44:55\n"));
        assert!(out.contains("[Link]\n"));
        assert!(out.contains("Name=lan0\n"));
        assert!(out.contains("MTUBytes=9000\n"));
        assert!(out.contains("WakeOnLan=magic\n"));
    }

    #[test]
    fn test_link_file_by_driver() {
        let cfg = LinkFileConfig {
            id: "id".into(),
            match_mac: None,
            match_path: None,
            match_driver: Some("virtio_net".into()),
            match_original_name: Some("en*".into()),
            name: None,
            mtu: Some(1500),
            mac_address: Some("52:54:00:ff:ff:ff".into()),
            wake_on_lan: None,
            description: None,
            created: String::new(),
            updated: String::new(),
            managed: true,
            source_file: None,
        };
        let out = link_file(&cfg);
        assert!(out.contains("Driver=virtio_net\n"));
        assert!(out.contains("OriginalName=en*\n"));
        assert!(out.contains("MTUBytes=1500\n"));
        assert!(out.contains("MACAddress=52:54:00:ff:ff:ff\n"));
        assert!(!out.contains("\nName="));
        assert!(!out.contains("WakeOnLan"));
    }
}
