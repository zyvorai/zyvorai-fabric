// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: LicenseRef-Zyvor-Production-1.0

use crate::host_tc::{self, DiscoveredTcQdisc};

/// Host traffic shaping suitable for read-only monitor policy display.
#[derive(Debug, Clone)]
pub struct DiscoveredHostMonitor {
    pub key: String,
    pub name: String,
    pub description: String,
    pub interface: String,
    pub rate_mbps: u64,
}

/// Map tc qdiscs with rate limits to monitor-style discoveries.
pub fn discover_host_monitor_from_tc() -> Vec<DiscoveredHostMonitor> {
    host_tc::discover_host_tc_qdiscs()
        .unwrap_or_else(|e| {
            tracing::warn!("host tc monitor discovery failed: {}", e);
            Vec::new()
        })
        .into_iter()
        .filter_map(tc_qdisc_to_monitor)
        .collect()
}

fn tc_qdisc_to_monitor(d: DiscoveredTcQdisc) -> Option<DiscoveredHostMonitor> {
    let rate_kbit = d.rate_kbit?;
    if rate_kbit == 0 {
        return None;
    }
    let rate_mbps = rate_kbit.div_ceil(1000).max(1);
    Some(DiscoveredHostMonitor {
        key: format!("monitor:{}", d.key),
        name: format!("host-monitor-{}", d.interface),
        description: format!(
            "Host tc {} shaping on {} ({} Mbit/s)",
            d.kind, d.interface, rate_mbps
        ),
        interface: d.interface,
        rate_mbps,
    })
}
