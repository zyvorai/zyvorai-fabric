// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: LicenseRef-Zyvor-Production-1.0

use anyhow::{Context, Result};
use std::process::Command;

/// A firewalld zone discovered on the host.
#[derive(Debug, Clone)]
pub struct DiscoveredFirewalldZone {
    pub name: String,
    pub description: String,
    pub target: Option<String>,
    pub interfaces: Vec<String>,
    pub sources: Vec<String>,
}

/// Discover zones from firewalld (`firewall-cmd --get-zones`).
pub fn discover_firewalld_zones() -> Result<Vec<DiscoveredFirewalldZone>> {
    let zones_out = Command::new("firewall-cmd")
        .args(["--get-zones"])
        .output()
        .context("Failed to run firewall-cmd --get-zones")?;

    if !zones_out.status.success() {
        let stderr = String::from_utf8_lossy(&zones_out.stderr);
        if stderr.contains("not found") || stderr.contains("Can't run") {
            return Ok(Vec::new());
        }
        return Err(anyhow::anyhow!("firewall-cmd --get-zones failed: {stderr}"));
    }

    let zones_text = String::from_utf8_lossy(&zones_out.stdout);
    let mut zones = Vec::new();

    for name in zones_text.split_whitespace() {
        let name = name.to_string();
        let (target, interfaces, sources) = zone_details(&name)?;
        let description = format!(
            "Host firewalld zone (target: {}, interfaces: [{}], sources: [{}])",
            target.as_deref().unwrap_or("default"),
            interfaces.join(", "),
            sources.join(", ")
        );
        zones.push(DiscoveredFirewalldZone {
            name,
            description,
            target,
            interfaces,
            sources,
        });
    }

    Ok(zones)
}

fn zone_details(name: &str) -> Result<(Option<String>, Vec<String>, Vec<String>)> {
    let target = Command::new("firewall-cmd")
        .args(["--zone", name, "--get-target"])
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .filter(|s| !s.is_empty());

    let interfaces = list_zone_items(name, "--list-interfaces")?;
    let sources = list_zone_items(name, "--list-sources")?;
    Ok((target, interfaces, sources))
}

fn list_zone_items(zone: &str, flag: &str) -> Result<Vec<String>> {
    let output = Command::new("firewall-cmd")
        .args(["--zone", zone, flag])
        .output()
        .with_context(|| format!("firewall-cmd --zone {zone} {flag}"))?;

    if !output.status.success() {
        return Ok(Vec::new());
    }

    let text = String::from_utf8_lossy(&output.stdout);
    Ok(text.split_whitespace().map(str::to_string).collect())
}

#[cfg(test)]
mod tests {
    #[test]
    fn parse_zones_line() {
        let zones: Vec<_> = "public home internal"
            .split_whitespace()
            .map(str::to_string)
            .collect();
        assert_eq!(zones, vec!["public", "home", "internal"]);
    }
}
