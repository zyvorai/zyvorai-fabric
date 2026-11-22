// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: LicenseRef-Zyvor-Production-2.0

use std::collections::HashSet;

use chrono::Utc;
use networking::host_floating_ips::{self, DiscoveredFloatingIp};
use uuid::Uuid;

use super::network_cloud::FloatingIp;
use crate::server::AppState;

pub fn is_host_managed_floating_ip(fip: &FloatingIp) -> bool {
    !fip.managed
}

fn host_floating_ip_uuid(iface: &str, address: &str) -> Uuid {
    Uuid::new_v5(
        &Uuid::NAMESPACE_DNS,
        format!("zyvor-fabricd:host:fip:{iface}:{address}").as_bytes(),
    )
}

fn floating_ip_from_discovered(d: DiscoveredFloatingIp) -> FloatingIp {
    FloatingIp {
        id: host_floating_ip_uuid(&d.interface, &d.address).to_string(),
        address: d.address,
        interface: d.interface,
        assigned_vm: None,
        managed: false,
        created: Utc::now(),
    }
}

pub fn discover_host_floating_ips() -> Vec<FloatingIp> {
    host_floating_ips::discover_host_floating_ips()
        .unwrap_or_else(|e| {
            tracing::warn!("host floating IP discovery failed: {}", e);
            Vec::new()
        })
        .into_iter()
        .map(floating_ip_from_discovered)
        .collect()
}

pub fn merge_floating_ips(_state: &AppState, mut items: Vec<FloatingIp>) -> Vec<FloatingIp> {
    let mut known_ids: HashSet<String> = items.iter().map(|f| f.id.clone()).collect();
    let mut known_addrs: HashSet<String> = items
        .iter()
        .map(|f| format!("{}:{}", f.interface, f.address))
        .collect();

    for host in discover_host_floating_ips() {
        let key = format!("{}:{}", host.interface, host.address);
        if known_ids.contains(&host.id) || known_addrs.contains(&key) {
            continue;
        }
        known_ids.insert(host.id.clone());
        known_addrs.insert(key);
        items.push(host);
    }
    items
}

pub fn find_host_floating_ip(_state: &AppState, id: &str) -> Option<FloatingIp> {
    merge_floating_ips(_state, vec![])
        .into_iter()
        .find(|f| f.id == id)
}
