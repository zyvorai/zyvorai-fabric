// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: LicenseRef-Zyvor-Production-2.0

use anyhow::{Context, Result};
use std::process::Command;

/// A secondary/global address on a host interface (candidate floating IP).
#[derive(Debug, Clone)]
pub struct DiscoveredFloatingIp {
    pub address: String,
    pub interface: String,
    pub prefixlen: u8,
}

/// Discover secondary global addresses via `ip -j addr show`.
pub fn discover_host_floating_ips() -> Result<Vec<DiscoveredFloatingIp>> {
    let output = Command::new("ip")
        .args(["-j", "addr", "show"])
        .output()
        .context("ip -j addr show")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(anyhow::anyhow!("ip -j addr show failed: {stderr}"));
    }

    let entries: Vec<serde_json::Value> =
        serde_json::from_slice(&output.stdout).unwrap_or_default();

    let mut out = Vec::new();
    let mut seen = std::collections::HashSet::new();

    for entry in entries {
        let iface = match entry.get("ifname").and_then(|v| v.as_str()) {
            Some(n) if n != "lo" => n.to_string(),
            _ => continue,
        };
        let Some(addrs) = entry.get("addr_info").and_then(|a| a.as_array()) else {
            continue;
        };
        for (idx, addr) in addrs.iter().enumerate() {
            let scope = addr.get("scope").and_then(|s| s.as_str()).unwrap_or("");
            if scope != "global" {
                continue;
            }
            let local = match addr.get("local").and_then(|l| l.as_str()) {
                Some(l) => l.to_string(),
                None => continue,
            };
            if local.starts_with("169.254.") {
                continue;
            }
            let prefixlen = addr.get("prefixlen").and_then(|p| p.as_u64()).unwrap_or(32) as u8;
            let secondary = addr
                .get("secondary")
                .and_then(|s| s.as_bool())
                .unwrap_or(false)
                || idx > 0;
            if !secondary {
                continue;
            }
            let key = format!("{iface}:{local}");
            if !seen.insert(key) {
                continue;
            }
            out.push(DiscoveredFloatingIp {
                address: local,
                interface: iface.clone(),
                prefixlen,
            });
        }
    }

    Ok(out)
}
