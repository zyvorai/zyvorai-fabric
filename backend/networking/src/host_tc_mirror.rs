// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: LicenseRef-Zyvor-Production-2.0

use anyhow::{Context, Result};
use std::collections::HashSet;
use std::process::Command;

/// A tc mirred/mirror rule discovered on a host interface.
#[derive(Debug, Clone)]
pub struct DiscoveredTcMirror {
    pub key: String,
    pub source_iface: String,
    pub collector_iface: String,
    pub direction: String,
}

/// Discover active tc mirror filters via `tc filter show`.
pub fn discover_host_tc_mirrors() -> Result<Vec<DiscoveredTcMirror>> {
    let ifaces = list_host_interfaces()?;
    let mut out = Vec::new();
    let mut seen = HashSet::new();

    for iface in ifaces {
        for direction in ["ingress", "egress"] {
            let output = Command::new("tc")
                .args(["filter", "show", "dev", &iface, direction])
                .output()
                .with_context(|| format!("tc filter show dev {iface} {direction}"))?;

            if !output.status.success() {
                continue;
            }
            let text = String::from_utf8_lossy(&output.stdout);
            for mirror in parse_tc_mirror_text(&iface, direction, &text) {
                if seen.insert(mirror.key.clone()) {
                    out.push(mirror);
                }
            }
        }
    }

    Ok(out)
}

fn list_host_interfaces() -> Result<Vec<String>> {
    let output = Command::new("ip")
        .args(["-j", "link", "show"])
        .output()
        .context("ip -j link show")?;

    if !output.status.success() {
        return Ok(Vec::new());
    }

    let entries: Vec<serde_json::Value> =
        serde_json::from_slice(&output.stdout).unwrap_or_default();

    Ok(entries
        .iter()
        .filter_map(|e| e.get("ifname").and_then(|v| v.as_str()).map(str::to_string))
        .filter(|n| n != "lo")
        .collect())
}

fn parse_tc_mirror_text(
    source_iface: &str,
    direction: &str,
    text: &str,
) -> Vec<DiscoveredTcMirror> {
    let mut out = Vec::new();

    for line in text.lines() {
        let trimmed = line.trim();
        if !trimmed.contains("mirred") || !trimmed.to_ascii_lowercase().contains("mirror") {
            continue;
        }
        let Some(collector_iface) = extract_mirror_target(trimmed) else {
            continue;
        };
        let key = format!("{source_iface}:{direction}:{collector_iface}");
        out.push(DiscoveredTcMirror {
            key,
            source_iface: source_iface.to_string(),
            collector_iface,
            direction: direction.to_string(),
        });
    }

    out
}

fn extract_mirror_target(line: &str) -> Option<String> {
    let lower = line.to_ascii_lowercase();
    for needle in ["device ", "dev "] {
        if let Some(idx) = lower.find(needle) {
            let rest = line[idx + needle.len()..].trim();
            let target = rest.split_whitespace().next()?.trim_end_matches(')');
            if !target.is_empty() {
                return Some(target.to_string());
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_mirror_line() {
        let text = r#"
filter parent ffff: protocol all pref 49152 matchall
action order 1: mirred (Egress Redirect to device eth1) pipe
action order 2: mirred (Egress Mirror to device mon0) pipe
"#;
        let mirrors = parse_tc_mirror_text("eth0", "ingress", text);
        assert_eq!(mirrors.len(), 1);
        assert_eq!(mirrors[0].collector_iface, "mon0");
    }
}
