// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: LicenseRef-Zyvor-Production-1.0

use anyhow::{Context, Result};
use std::process::Command;

/// A WireGuard interface discovered on the host.
#[derive(Debug, Clone)]
pub struct DiscoveredWireGuard {
    pub interface_name: String,
    pub listen_port: Option<u16>,
    pub address: Option<String>,
    pub peer_count: usize,
    pub public_key: Option<String>,
}

/// Discover WireGuard interfaces via `wg show all dump`.
pub fn discover_wireguard_interfaces() -> Result<Vec<DiscoveredWireGuard>> {
    let output = Command::new("wg")
        .args(["show", "all", "dump"])
        .output()
        .context("wg show all dump")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        if stderr.contains("not found") {
            return Ok(Vec::new());
        }
        return Err(anyhow::anyhow!("wg show failed: {stderr}"));
    }

    Ok(parse_wg_dump(&String::from_utf8_lossy(&output.stdout)))
}

fn parse_wg_dump(text: &str) -> Vec<DiscoveredWireGuard> {
    let mut interfaces: std::collections::HashMap<String, DiscoveredWireGuard> =
        std::collections::HashMap::new();

    for line in text.lines() {
        let cols: Vec<&str> = line.split('\t').collect();
        if cols.is_empty() {
            continue;
        }
        match cols[0] {
            "interface" if cols.len() >= 4 => {
                let name = cols[1].to_string();
                let pubkey = cols[2].to_string();
                let port: Option<u16> = cols[3].parse().ok();
                interfaces.insert(
                    name.clone(),
                    DiscoveredWireGuard {
                        interface_name: name,
                        listen_port: port,
                        address: None,
                        peer_count: 0,
                        public_key: Some(pubkey),
                    },
                );
            }
            "peer" if cols.len() >= 2 => {
                // peer lines reference interface by context — wg dump groups by interface
                // Format: peer <pubkey> <preshared> <endpoint> <allowed_ips> ...
                if let Some(last) = interfaces.values_mut().last() {
                    last.peer_count += 1;
                }
            }
            _ => {}
        }
    }

    // Re-parse with interface grouping (dump lists interface then its peers)
    let mut result = Vec::new();
    let mut current: Option<DiscoveredWireGuard> = None;

    for line in text.lines() {
        let cols: Vec<&str> = line.split('\t').collect();
        if cols.first() == Some(&"interface") && cols.len() >= 4 {
            if let Some(iface) = current.take() {
                result.push(iface);
            }
            current = Some(DiscoveredWireGuard {
                interface_name: cols[1].to_string(),
                public_key: Some(cols[2].to_string()),
                listen_port: cols[3].parse().ok(),
                address: None,
                peer_count: 0,
            });
        } else if cols.first() == Some(&"peer") {
            if let Some(ref mut iface) = current {
                iface.peer_count += 1;
            }
        }
    }
    if let Some(iface) = current {
        result.push(iface);
    }

    if result.is_empty() {
        result = interfaces.into_values().collect();
    }

    for iface in &mut result {
        if let Ok(out) = Command::new("ip")
            .args(["-j", "addr", "show", &iface.interface_name])
            .output()
        {
            if out.status.success() {
                if let Ok(entries) = serde_json::from_slice::<Vec<serde_json::Value>>(&out.stdout) {
                    if let Some(e) = entries.first() {
                        iface.address = e
                            .get("addr_info")
                            .and_then(|a| a.as_array())
                            .and_then(|arr| arr.first())
                            .and_then(|a| {
                                let local = a.get("local")?.as_str()?;
                                let prefix = a.get("prefixlen")?.as_u64()?;
                                Some(format!("{}/{}", local, prefix))
                            });
                    }
                }
            }
        }
    }

    result
}
