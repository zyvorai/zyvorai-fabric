// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: LicenseRef-Zyvor-Production-1.0

use anyhow::{Context, Result};
use std::collections::HashSet;
use std::fs;
use std::path::Path;
use std::process::Command;

/// A DNS zone discovered on the host (systemd-resolved domain or BIND zone).
#[derive(Debug, Clone)]
pub struct DiscoveredDnsZone {
    pub name: String,
    pub description: String,
}

/// Discover DNS zones from resolvectl and BIND config files.
pub fn discover_host_dns_zones() -> Result<Vec<DiscoveredDnsZone>> {
    let mut names = HashSet::new();
    let mut zones = Vec::new();

    for domain in discover_resolvectl_domains()? {
        if names.insert(domain.clone()) {
            zones.push(DiscoveredDnsZone {
                name: domain.clone(),
                description: "systemd-resolved DNS domain".to_string(),
            });
        }
    }

    for (name, desc) in discover_bind_zones()? {
        if names.insert(name.clone()) {
            zones.push(DiscoveredDnsZone {
                name,
                description: desc,
            });
        }
    }

    Ok(zones)
}

fn discover_resolvectl_domains() -> Result<Vec<String>> {
    let output = Command::new("resolvectl")
        .arg("status")
        .output()
        .context("resolvectl status")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        if stderr.contains("not found") {
            return Ok(Vec::new());
        }
        return Err(anyhow::anyhow!("resolvectl status failed: {stderr}"));
    }

    let text = String::from_utf8_lossy(&output.stdout);
    let mut domains = Vec::new();

    for line in text.lines() {
        let trimmed = line.trim();
        for prefix in ["Current DNS Domain:", "DNS Domain:", "Domains:"] {
            if let Some(rest) = trimmed.strip_prefix(prefix) {
                for part in rest.split_whitespace() {
                    let d = part.trim_end_matches('.');
                    if !d.is_empty() && d != "~." && d.contains('.') {
                        domains.push(d.to_string());
                    }
                }
            }
        }
    }

    Ok(domains)
}

fn discover_bind_zones() -> Result<Vec<(String, String)>> {
    let paths = ["/etc/bind/named.conf.local", "/etc/named.conf"];
    let mut zones = Vec::new();
    let mut seen = HashSet::new();

    for path in paths {
        let p = Path::new(path);
        if !p.exists() {
            continue;
        }
        let content = fs::read_to_string(p).with_context(|| format!("read {}", p.display()))?;
        for zone in parse_bind_zone_names(&content) {
            if seen.insert(zone.clone()) {
                zones.push((zone, format!("BIND zone in {}", p.display())));
            }
        }
    }

    Ok(zones)
}

fn parse_bind_zone_names(content: &str) -> Vec<String> {
    let mut out = Vec::new();
    for line in content.lines() {
        let trimmed = line.trim();
        if let Some(rest) = trimmed.strip_prefix("zone \"") {
            if let Some((name, _)) = rest.split_once('"') {
                if !name.is_empty() {
                    out.push(name.to_string());
                }
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_bind_zone() {
        let conf = r#"
zone "example.com" IN {
    type master;
};
zone "zyvor-fabricd.local" {
};
"#;
        let names = parse_bind_zone_names(conf);
        assert!(names.contains(&"example.com".to_string()));
        assert!(names.contains(&"zyvor-fabricd.local".to_string()));
    }
}
