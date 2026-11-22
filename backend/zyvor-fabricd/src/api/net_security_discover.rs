// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: LicenseRef-Zyvor-Production-2.0

use std::collections::HashSet;

use chrono::Utc;
use dns_policy::models::{DnsPolicy, DnsRecordType, DnsZone};
use nat_gateway::models::{NatProtocol, NatRule, NatRuleType};
use net_monitor::models::{
    AlertAction, AlertSeverity, BandwidthThreshold, MonitorPolicy, ThresholdUnit, TrafficDirection,
};
use network_policy::identity::IdentityAllocator;
use network_policy::models::{NetworkPolicy, SecurityIdentity};
use networking::host_dns::{self, DiscoveredDnsZone};
use networking::host_firewalld::{self, DiscoveredFirewalldZone};
use networking::host_identities::{self, DiscoveredHostIdentity};
use networking::host_monitor_tc::{self, DiscoveredHostMonitor};
use networking::host_nat::{self, DiscoveredHostNatRule};
use networking::host_nft_filter::{self, DiscoveredNftFilterChain};
use networking::host_nft_policy::{self, DiscoveredNftPolicyChain};
use networking::host_resolv::{self, DiscoveredDnsUpstream};
use networking::host_services::{self, HostListener};
use networking::host_tc::{self, DiscoveredTcQdisc};
use networking::host_tc_mirror::{self, DiscoveredTcMirror};
use networking::host_wireguard::{self, DiscoveredWireGuard};
use packet_mirror::models::{CollectorType, MirrorDirection, MirrorSession};
use service_mesh::models::{LoadBalancerAlgorithm, Service, ServicePort, ServiceProtocol};
use traffic_shaping::models::{BandwidthRate, BandwidthUnit, QoSPolicy, TrafficClass};
use uuid::Uuid;
use vm_firewall::models::{FirewallAction, FirewallProfile, FirewallZone};
use vpn_mesh::models::VpnTunnel;

use crate::server::AppState;

pub fn is_host_managed_nat(rule: &NatRule) -> bool {
    !rule.managed
}

pub fn is_host_managed_service(service: &Service) -> bool {
    !service.managed
}

pub fn is_host_managed_zone(zone: &FirewallZone) -> bool {
    !zone.managed
}

pub fn is_host_managed_qos(policy: &QoSPolicy) -> bool {
    !policy.managed
}

pub fn is_host_managed_profile(profile: &FirewallProfile) -> bool {
    !profile.managed
}

pub fn is_host_managed_vpn_tunnel(tunnel: &VpnTunnel) -> bool {
    !tunnel.managed
}

pub fn is_host_managed_dns_zone(zone: &DnsZone) -> bool {
    !zone.managed
}

pub fn is_host_managed_dns_policy(policy: &DnsPolicy) -> bool {
    !policy.managed
}

pub fn is_host_managed_mirror(session: &MirrorSession) -> bool {
    !session.managed
}

pub fn is_host_managed_monitor(policy: &MonitorPolicy) -> bool {
    !policy.managed
}

pub fn is_host_managed_network_policy(policy: &NetworkPolicy) -> bool {
    !policy.managed
}

pub fn is_host_managed_identity(identity: &SecurityIdentity) -> bool {
    !identity.managed
}

fn host_nat_uuid(key: &str) -> Uuid {
    Uuid::new_v5(
        &Uuid::NAMESPACE_DNS,
        format!("zyvor-fabricd:host:nat:{key}").as_bytes(),
    )
}

fn host_service_uuid(key: &str) -> Uuid {
    Uuid::new_v5(
        &Uuid::NAMESPACE_DNS,
        format!("zyvor-fabricd:host:svc:{key}").as_bytes(),
    )
}

fn host_zone_uuid(name: &str) -> Uuid {
    Uuid::new_v5(
        &Uuid::NAMESPACE_DNS,
        format!("zyvor-fabricd:host:fwzone:{name}").as_bytes(),
    )
}

fn host_qos_uuid(key: &str) -> Uuid {
    Uuid::new_v5(
        &Uuid::NAMESPACE_DNS,
        format!("zyvor-fabricd:host:qos:{key}").as_bytes(),
    )
}

fn host_profile_uuid(key: &str) -> Uuid {
    Uuid::new_v5(
        &Uuid::NAMESPACE_DNS,
        format!("zyvor-fabricd:host:fwprof:{key}").as_bytes(),
    )
}

fn host_vpn_uuid(interface_name: &str) -> Uuid {
    Uuid::new_v5(
        &Uuid::NAMESPACE_DNS,
        format!("zyvor-fabricd:host:vpn:{interface_name}").as_bytes(),
    )
}

fn host_dns_zone_uuid(name: &str) -> Uuid {
    Uuid::new_v5(
        &Uuid::NAMESPACE_DNS,
        format!("zyvor-fabricd:host:dnszone:{name}").as_bytes(),
    )
}

fn host_dns_policy_uuid(key: &str) -> Uuid {
    Uuid::new_v5(
        &Uuid::NAMESPACE_DNS,
        format!("zyvor-fabricd:host:dnspol:{key}").as_bytes(),
    )
}

fn host_dns_upstream_zone_id() -> Uuid {
    Uuid::new_v5(
        &Uuid::NAMESPACE_DNS,
        b"zyvor-fabricd:host:dns:upstream-zone",
    )
}

fn host_mirror_uuid(key: &str) -> Uuid {
    Uuid::new_v5(
        &Uuid::NAMESPACE_DNS,
        format!("zyvor-fabricd:host:mirror:{key}").as_bytes(),
    )
}

fn host_monitor_uuid(key: &str) -> Uuid {
    Uuid::new_v5(
        &Uuid::NAMESPACE_DNS,
        format!("zyvor-fabricd:host:monitor:{key}").as_bytes(),
    )
}

fn host_network_policy_uuid(key: &str) -> Uuid {
    Uuid::new_v5(
        &Uuid::NAMESPACE_DNS,
        format!("zyvor-fabricd:host:netpol:{key}").as_bytes(),
    )
}

fn discovered_to_nat_rule(d: DiscoveredHostNatRule) -> NatRule {
    let rule_type = match d.rule_type.as_str() {
        "snat" => NatRuleType::Snat,
        "dnat" => NatRuleType::Dnat,
        "hairpin" => NatRuleType::Hairpin,
        _ => NatRuleType::Masquerade,
    };
    let protocol = match d.protocol.as_deref() {
        Some("tcp") => NatProtocol::Tcp,
        Some("udp") => NatProtocol::Udp,
        _ => NatProtocol::Any,
    };
    let now = Utc::now();
    NatRule {
        id: host_nat_uuid(&d.key),
        name: d.name,
        description: d.description,
        rule_type,
        selector: Default::default(),
        protocol,
        source_cidr: d.source_cidr,
        dest_cidr: d.dest_cidr,
        dest_port: d.dest_port,
        dest_port_end: None,
        translate_to: d.translate_to,
        translate_port: d.translate_port,
        pool_id: None,
        outbound_interface: d.outbound_interface,
        enabled: true,
        managed: false,
        created: now,
        updated: now,
    }
}

fn listener_to_service(l: HostListener) -> Service {
    let protocol = if l.protocol == "udp" {
        ServiceProtocol::Udp
    } else {
        ServiceProtocol::Tcp
    };
    let bind_ip = if l.bind_address.is_empty() || l.bind_address == "*" {
        "0.0.0.0".to_string()
    } else {
        l.bind_address.clone()
    };
    let now = Utc::now();
    Service {
        id: host_service_uuid(&l.key),
        name: l.name,
        description: l.description,
        virtual_ip: bind_ip,
        selector: Default::default(),
        ports: vec![ServicePort {
            port: l.port,
            target_port: Some(l.port),
            protocol,
        }],
        algorithm: LoadBalancerAlgorithm::RoundRobin,
        health_check: Default::default(),
        enabled: true,
        managed: false,
        created: now,
        updated: now,
    }
}

pub fn discover_host_nat_rules() -> Vec<NatRule> {
    host_nat::discover_host_nat_rules()
        .unwrap_or_else(|e| {
            tracing::warn!("host NAT discovery failed: {}", e);
            Vec::new()
        })
        .into_iter()
        .map(discovered_to_nat_rule)
        .collect()
}

fn firewalld_to_zone(d: DiscoveredFirewalldZone) -> FirewallZone {
    let now = Utc::now();
    FirewallZone {
        id: host_zone_uuid(&d.name),
        name: d.name,
        description: d.description,
        default_profile_id: None,
        managed: false,
        created: now,
        updated: now,
    }
}

pub fn discover_host_firewall_zones() -> Vec<FirewallZone> {
    host_firewalld::discover_firewalld_zones()
        .unwrap_or_else(|e| {
            tracing::warn!("firewalld zone discovery failed: {}", e);
            Vec::new()
        })
        .into_iter()
        .map(firewalld_to_zone)
        .collect()
}

pub fn discover_host_services() -> Vec<Service> {
    host_services::discover_host_listeners()
        .unwrap_or_else(|e| {
            tracing::warn!("host listener discovery failed: {}", e);
            Vec::new()
        })
        .into_iter()
        .map(listener_to_service)
        .collect()
}

pub fn merge_nat_rules(_state: &AppState, mut items: Vec<NatRule>) -> Vec<NatRule> {
    let mut known_ids: HashSet<Uuid> = items.iter().map(|r| r.id).collect();
    let mut known_names: HashSet<String> = items.iter().map(|r| r.name.clone()).collect();

    for host in discover_host_nat_rules() {
        if known_ids.contains(&host.id) || known_names.contains(&host.name) {
            continue;
        }
        known_ids.insert(host.id);
        known_names.insert(host.name.clone());
        items.push(host);
    }
    items
}

pub fn merge_services(_state: &AppState, mut items: Vec<Service>) -> Vec<Service> {
    let mut known_ids: HashSet<Uuid> = items.iter().map(|s| s.id).collect();
    let mut known_names: HashSet<String> = items.iter().map(|s| s.name.clone()).collect();

    for host in discover_host_services() {
        if known_ids.contains(&host.id) || known_names.contains(&host.name) {
            continue;
        }
        known_ids.insert(host.id);
        known_names.insert(host.name.clone());
        items.push(host);
    }
    items
}

pub fn find_host_nat_rule(_state: &AppState, id: &str) -> Option<NatRule> {
    let uuid = Uuid::parse_str(id).ok()?;
    merge_nat_rules(_state, vec![])
        .into_iter()
        .find(|r| r.id == uuid)
}

pub fn find_host_service(_state: &AppState, id: &str) -> Option<Service> {
    let uuid = Uuid::parse_str(id).ok()?;
    merge_services(_state, vec![])
        .into_iter()
        .find(|s| s.id == uuid)
}

pub fn merge_firewall_zones(_state: &AppState, mut items: Vec<FirewallZone>) -> Vec<FirewallZone> {
    let mut known_ids: HashSet<Uuid> = items.iter().map(|z| z.id).collect();
    let mut known_names: HashSet<String> = items.iter().map(|z| z.name.clone()).collect();

    for host in discover_host_firewall_zones() {
        if known_ids.contains(&host.id) || known_names.contains(&host.name) {
            continue;
        }
        known_ids.insert(host.id);
        known_names.insert(host.name.clone());
        items.push(host);
    }
    items
}

pub fn find_host_firewall_zone(_state: &AppState, id: &str) -> Option<FirewallZone> {
    let uuid = Uuid::parse_str(id).ok()?;
    merge_firewall_zones(_state, vec![])
        .into_iter()
        .find(|z| z.id == uuid)
}

fn tc_to_qos(d: DiscoveredTcQdisc) -> QoSPolicy {
    let rate = d.rate_kbit.unwrap_or(100).max(1);
    let ceil = d.ceil_kbit.unwrap_or(rate).max(rate);
    let now = Utc::now();
    QoSPolicy {
        id: host_qos_uuid(&d.key),
        name: d.name,
        description: d.description,
        interface: d.interface,
        selector: Default::default(),
        traffic_class: TrafficClass {
            name: d.kind,
            guaranteed_rate: BandwidthRate {
                value: rate,
                unit: BandwidthUnit::Kbit,
            },
            max_rate: BandwidthRate {
                value: ceil,
                unit: BandwidthUnit::Kbit,
            },
            burst: None,
            priority: 4,
        },
        enabled: true,
        managed: false,
        created: now,
        updated: now,
    }
}

fn nft_chain_to_profile(d: DiscoveredNftFilterChain) -> FirewallProfile {
    let default_action = if d.default_action.eq_ignore_ascii_case("accept") {
        FirewallAction::Accept
    } else {
        FirewallAction::Drop
    };
    let now = Utc::now();
    FirewallProfile {
        id: host_profile_uuid(&d.key),
        name: d.name,
        description: d.description,
        default_action,
        rules: Vec::new(),
        managed: false,
        created: now,
        updated: now,
    }
}

pub fn discover_host_qos_policies() -> Vec<QoSPolicy> {
    host_tc::discover_host_tc_qdiscs()
        .unwrap_or_else(|e| {
            tracing::warn!("host tc discovery failed: {}", e);
            Vec::new()
        })
        .into_iter()
        .map(tc_to_qos)
        .collect()
}

pub fn discover_host_firewall_profiles() -> Vec<FirewallProfile> {
    host_nft_filter::discover_host_nft_filter_chains()
        .unwrap_or_else(|e| {
            tracing::warn!("host nft filter discovery failed: {}", e);
            Vec::new()
        })
        .into_iter()
        .map(nft_chain_to_profile)
        .collect()
}

pub fn merge_qos_policies(_state: &AppState, mut items: Vec<QoSPolicy>) -> Vec<QoSPolicy> {
    let mut known_ids: HashSet<Uuid> = items.iter().map(|p| p.id).collect();
    let mut known_names: HashSet<String> = items.iter().map(|p| p.name.clone()).collect();

    for host in discover_host_qos_policies() {
        if known_ids.contains(&host.id) || known_names.contains(&host.name) {
            continue;
        }
        known_ids.insert(host.id);
        known_names.insert(host.name.clone());
        items.push(host);
    }
    items
}

pub fn merge_firewall_profiles(
    _state: &AppState,
    mut items: Vec<FirewallProfile>,
) -> Vec<FirewallProfile> {
    let mut known_ids: HashSet<Uuid> = items.iter().map(|p| p.id).collect();
    let mut known_names: HashSet<String> = items.iter().map(|p| p.name.clone()).collect();

    for host in discover_host_firewall_profiles() {
        if known_ids.contains(&host.id) || known_names.contains(&host.name) {
            continue;
        }
        known_ids.insert(host.id);
        known_names.insert(host.name.clone());
        items.push(host);
    }
    items
}

pub fn find_host_qos_policy(_state: &AppState, id: &str) -> Option<QoSPolicy> {
    let uuid = Uuid::parse_str(id).ok()?;
    merge_qos_policies(_state, vec![])
        .into_iter()
        .find(|p| p.id == uuid)
}

pub fn find_host_firewall_profile(_state: &AppState, id: &str) -> Option<FirewallProfile> {
    let uuid = Uuid::parse_str(id).ok()?;
    merge_firewall_profiles(_state, vec![])
        .into_iter()
        .find(|p| p.id == uuid)
}

fn wireguard_to_tunnel(d: DiscoveredWireGuard) -> VpnTunnel {
    let now = Utc::now();
    let listen_port = d.listen_port.unwrap_or(51820);
    let address = d.address.unwrap_or_else(|| "0.0.0.0/32".to_string());
    VpnTunnel {
        id: host_vpn_uuid(&d.interface_name),
        name: d.interface_name.clone(),
        description: format!(
            "Host WireGuard interface ({} peer{})",
            d.peer_count,
            if d.peer_count == 1 { "" } else { "s" }
        ),
        interface_name: d.interface_name,
        listen_port,
        address,
        private_key_ref: String::new(),
        peers: Vec::new(),
        enabled: false,
        managed: false,
        created: now,
        updated: now,
    }
}

pub fn discover_host_vpn_tunnels() -> Vec<VpnTunnel> {
    host_wireguard::discover_wireguard_interfaces()
        .unwrap_or_else(|e| {
            tracing::warn!("host WireGuard discovery failed: {}", e);
            Vec::new()
        })
        .into_iter()
        .map(wireguard_to_tunnel)
        .collect()
}

pub fn merge_vpn_tunnels(_state: &AppState, mut items: Vec<VpnTunnel>) -> Vec<VpnTunnel> {
    let mut known_ids: HashSet<Uuid> = items.iter().map(|t| t.id).collect();
    let mut known_ifaces: HashSet<String> =
        items.iter().map(|t| t.interface_name.clone()).collect();

    for host in discover_host_vpn_tunnels() {
        if known_ids.contains(&host.id) || known_ifaces.contains(&host.interface_name) {
            continue;
        }
        known_ids.insert(host.id);
        known_ifaces.insert(host.interface_name.clone());
        items.push(host);
    }
    items
}

pub fn find_host_vpn_tunnel(_state: &AppState, id: &str) -> Option<VpnTunnel> {
    let uuid = Uuid::parse_str(id).ok()?;
    merge_vpn_tunnels(_state, vec![])
        .into_iter()
        .find(|t| t.id == uuid)
}

fn dns_zone_from_discovered(d: DiscoveredDnsZone) -> DnsZone {
    let now = Utc::now();
    DnsZone {
        id: host_dns_zone_uuid(&d.name),
        name: d.name,
        description: d.description,
        managed: false,
        created: now,
        updated: now,
    }
}

pub fn discover_host_dns_zones() -> Vec<DnsZone> {
    host_dns::discover_host_dns_zones()
        .unwrap_or_else(|e| {
            tracing::warn!("host DNS zone discovery failed: {}", e);
            Vec::new()
        })
        .into_iter()
        .map(dns_zone_from_discovered)
        .collect()
}

pub fn merge_dns_zones(_state: &AppState, mut items: Vec<DnsZone>) -> Vec<DnsZone> {
    let mut known_ids: HashSet<Uuid> = items.iter().map(|z| z.id).collect();
    let mut known_names: HashSet<String> = items.iter().map(|z| z.name.clone()).collect();

    for host in discover_host_dns_zones() {
        if known_ids.contains(&host.id) || known_names.contains(&host.name) {
            continue;
        }
        known_ids.insert(host.id);
        known_names.insert(host.name.clone());
        items.push(host);
    }
    items
}

pub fn find_host_dns_zone(_state: &AppState, id: &str) -> Option<DnsZone> {
    let uuid = Uuid::parse_str(id).ok()?;
    merge_dns_zones(_state, vec![])
        .into_iter()
        .find(|z| z.id == uuid)
}

fn dns_policy_from_upstream(d: DiscoveredDnsUpstream) -> DnsPolicy {
    let now = Utc::now();
    let key = format!("{}:{}", d.source, d.server);
    DnsPolicy {
        id: host_dns_policy_uuid(&key),
        name: format!("host-upstream-{}", d.server.replace(':', "-")),
        description: format!("Host DNS upstream {} ({})", d.server, d.source),
        zone_id: host_dns_upstream_zone_id(),
        selector: Default::default(),
        record_template: d.server,
        record_type: DnsRecordType::A,
        enabled: false,
        managed: false,
        created: now,
        updated: now,
    }
}

pub fn discover_host_dns_policies() -> Vec<DnsPolicy> {
    host_resolv::discover_host_dns_upstreams()
        .unwrap_or_else(|e| {
            tracing::warn!("host DNS upstream discovery failed: {}", e);
            Vec::new()
        })
        .into_iter()
        .map(dns_policy_from_upstream)
        .collect()
}

pub fn merge_dns_policies(_state: &AppState, mut items: Vec<DnsPolicy>) -> Vec<DnsPolicy> {
    let mut known_ids: HashSet<Uuid> = items.iter().map(|p| p.id).collect();
    let mut known_names: HashSet<String> = items.iter().map(|p| p.name.clone()).collect();

    for host in discover_host_dns_policies() {
        if known_ids.contains(&host.id) || known_names.contains(&host.name) {
            continue;
        }
        known_ids.insert(host.id);
        known_names.insert(host.name.clone());
        items.push(host);
    }
    items
}

pub fn find_host_dns_policy(_state: &AppState, id: &str) -> Option<DnsPolicy> {
    let uuid = Uuid::parse_str(id).ok()?;
    merge_dns_policies(_state, vec![])
        .into_iter()
        .find(|p| p.id == uuid)
}

fn mirror_from_discovered(d: DiscoveredTcMirror) -> MirrorSession {
    let direction = match d.direction.as_str() {
        "egress" => MirrorDirection::Egress,
        "ingress" => MirrorDirection::Ingress,
        _ => MirrorDirection::Both,
    };
    let now = Utc::now();
    MirrorSession {
        id: host_mirror_uuid(&d.key),
        name: format!("host-mirror-{}-{}", d.source_iface, d.collector_iface),
        description: format!(
            "Host tc mirror {} → {} ({})",
            d.source_iface, d.collector_iface, d.direction
        ),
        selector: Default::default(),
        collector_type: CollectorType::Interface,
        collector_target: d.collector_iface,
        direction,
        filter: None,
        enabled: false,
        managed: false,
        created: now,
        updated: now,
    }
}

pub fn discover_host_mirror_sessions() -> Vec<MirrorSession> {
    host_tc_mirror::discover_host_tc_mirrors()
        .unwrap_or_else(|e| {
            tracing::warn!("host tc mirror discovery failed: {}", e);
            Vec::new()
        })
        .into_iter()
        .map(mirror_from_discovered)
        .collect()
}

pub fn merge_mirror_sessions(
    _state: &AppState,
    mut items: Vec<MirrorSession>,
) -> Vec<MirrorSession> {
    let mut known_ids: HashSet<Uuid> = items.iter().map(|s| s.id).collect();
    let mut known_keys: HashSet<String> = items
        .iter()
        .map(|s| format!("{}:{}", s.name, s.collector_target))
        .collect();

    for host in discover_host_mirror_sessions() {
        let key = format!("{}:{}", host.name, host.collector_target);
        if known_ids.contains(&host.id) || known_keys.contains(&key) {
            continue;
        }
        known_ids.insert(host.id);
        known_keys.insert(key);
        items.push(host);
    }
    items
}

pub fn find_host_mirror_session(_state: &AppState, id: &str) -> Option<MirrorSession> {
    let uuid = Uuid::parse_str(id).ok()?;
    merge_mirror_sessions(_state, vec![])
        .into_iter()
        .find(|s| s.id == uuid)
}

fn monitor_from_discovered(d: DiscoveredHostMonitor) -> MonitorPolicy {
    let now = Utc::now();
    MonitorPolicy {
        id: host_monitor_uuid(&d.key),
        name: d.name,
        description: d.description,
        selector: Default::default(),
        thresholds: vec![BandwidthThreshold {
            value: d.rate_mbps,
            unit: ThresholdUnit::Mbps,
            direction: TrafficDirection::Both,
            severity: AlertSeverity::Warning,
        }],
        action: AlertAction::Log,
        webhook_url: None,
        sample_interval_secs: 10,
        enabled: false,
        managed: false,
        created: now,
        updated: now,
    }
}

pub fn discover_host_monitor_policies() -> Vec<MonitorPolicy> {
    host_monitor_tc::discover_host_monitor_from_tc()
        .into_iter()
        .map(monitor_from_discovered)
        .collect()
}

pub fn merge_monitor_policies(
    _state: &AppState,
    mut items: Vec<MonitorPolicy>,
) -> Vec<MonitorPolicy> {
    let mut known_ids: HashSet<Uuid> = items.iter().map(|p| p.id).collect();
    let mut known_names: HashSet<String> = items.iter().map(|p| p.name.clone()).collect();

    for host in discover_host_monitor_policies() {
        if known_ids.contains(&host.id) || known_names.contains(&host.name) {
            continue;
        }
        known_ids.insert(host.id);
        known_names.insert(host.name.clone());
        items.push(host);
    }
    items
}

pub fn find_host_monitor_policy(_state: &AppState, id: &str) -> Option<MonitorPolicy> {
    let uuid = Uuid::parse_str(id).ok()?;
    merge_monitor_policies(_state, vec![])
        .into_iter()
        .find(|p| p.id == uuid)
}

fn network_policy_from_discovered(d: DiscoveredNftPolicyChain) -> NetworkPolicy {
    let now = Utc::now();
    NetworkPolicy {
        id: host_network_policy_uuid(&d.key),
        name: d.name,
        description: d.description,
        endpoint_selector: Default::default(),
        ingress: Vec::new(),
        egress: Vec::new(),
        enabled: false,
        managed: false,
        created: now,
        updated: now,
    }
}

pub fn discover_host_network_policies() -> Vec<NetworkPolicy> {
    host_nft_policy::discover_host_nft_policy_chains()
        .unwrap_or_else(|e| {
            tracing::warn!("host nft policy discovery failed: {}", e);
            Vec::new()
        })
        .into_iter()
        .map(network_policy_from_discovered)
        .collect()
}

pub fn merge_network_policies(
    _state: &AppState,
    mut items: Vec<NetworkPolicy>,
) -> Vec<NetworkPolicy> {
    let mut known_ids: HashSet<Uuid> = items.iter().map(|p| p.id).collect();
    let mut known_names: HashSet<String> = items.iter().map(|p| p.name.clone()).collect();

    for host in discover_host_network_policies() {
        if known_ids.contains(&host.id) || known_names.contains(&host.name) {
            continue;
        }
        known_ids.insert(host.id);
        known_names.insert(host.name.clone());
        items.push(host);
    }
    items
}

pub fn find_host_network_policy(_state: &AppState, id: &str) -> Option<NetworkPolicy> {
    let uuid = Uuid::parse_str(id).ok()?;
    merge_network_policies(_state, vec![])
        .into_iter()
        .find(|p| p.id == uuid)
}

const HOST_IDENTITY_BASE: u32 = 200_000;

fn host_identity_id(key: &str) -> u32 {
    let uuid = Uuid::new_v5(
        &Uuid::NAMESPACE_DNS,
        format!("zyvor-fabricd:host:identity:{key}").as_bytes(),
    );
    let bytes = uuid.as_bytes();
    let n = u32::from_be_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]);
    HOST_IDENTITY_BASE + (n % 100_000)
}

fn identity_from_discovered(d: DiscoveredHostIdentity) -> SecurityIdentity {
    let now = Utc::now();
    SecurityIdentity {
        id: host_identity_id(&d.key),
        labels: d.labels,
        endpoints: d.endpoints,
        description: d.description,
        managed: false,
        created: now,
        updated: now,
    }
}

pub fn discover_host_security_identities() -> Vec<SecurityIdentity> {
    host_identities::discover_host_identities()
        .unwrap_or_else(|e| {
            tracing::warn!("host identity discovery failed: {}", e);
            Vec::new()
        })
        .into_iter()
        .map(identity_from_discovered)
        .collect()
}

pub fn merge_identities(mut items: Vec<SecurityIdentity>) -> Vec<SecurityIdentity> {
    let mut known_ids: HashSet<u32> = items.iter().map(|i| i.id).collect();
    let mut known_keys: HashSet<String> = items
        .iter()
        .map(|i| {
            let map: std::collections::HashMap<String, String> = i
                .labels
                .iter()
                .map(|(k, v)| (k.clone(), v.clone()))
                .collect();
            IdentityAllocator::canonical_key(&map)
        })
        .collect();

    for host in discover_host_security_identities() {
        let key = {
            let map: std::collections::HashMap<String, String> = host
                .labels
                .iter()
                .map(|(k, v)| (k.clone(), v.clone()))
                .collect();
            IdentityAllocator::canonical_key(&map)
        };
        if known_ids.contains(&host.id) || known_keys.contains(&key) {
            continue;
        }
        known_ids.insert(host.id);
        known_keys.insert(key);
        items.push(host);
    }
    items
}

pub fn find_host_identity(id: u32) -> Option<SecurityIdentity> {
    merge_identities(vec![]).into_iter().find(|i| i.id == id)
}
