// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: LicenseRef-Zyvor-Production-2.0

use anyhow::{Context, Result};
use std::collections::HashSet;
use std::fs;
use std::path::Path;
use std::process::Command;

/// An upstream DNS resolver discovered on the host.
#[derive(Debug, Clone)]
pub struct DiscoveredDnsUpstream {
    pub server: String,
    pub source: String,
}

/// Discover upstream DNS servers from resolvectl, resolved.conf, and resolv.conf.
pub fn discover_host_dns_upstreams() -> Result<Vec<DiscoveredDnsUpstream>> {
    let mut servers = HashSet::new();
    let mut out = Vec::new();

    for (server, source) in discover_resolvectl_servers().unwrap_or_default() {
        if servers.insert(server.clone()) {
            out.push(DiscoveredDnsUpstream { server, source });
        }
    }

    for (server, source) in discover_resolved_conf_servers().unwrap_or_default() {
        if servers.insert(server.clone()) {
            out.push(DiscoveredDnsUpstream { server, source });
        }
    }

    for (server, source) in discover_resolv_conf_servers().unwrap_or_default() {
        if servers.insert(server.clone()) {
            out.push(DiscoveredDnsUpstream { server, source });
        }
    }

    Ok(out)
}

fn discover_resolvectl_servers() -> Result<Vec<(String, String)>> {
    let output = match Command::new("resolvectl").arg("status").output() {
        Ok(o) => o,
        Err(_) => return Ok(Vec::new()),
    };

    if !output.status.success() {
        return Ok(Vec::new());
    }

    let text = String::from_utf8_lossy(&output.stdout);
    let mut out = Vec::new();

    for line in text.lines() {
        let trimmed = line.trim();
        for prefix in [
            "DNS Servers:",
            "DNS Server:",
            "Fallback DNS Servers:",
            "Fallback DNS Server:",
        ] {
            if let Some(rest) = trimmed.strip_prefix(prefix) {
                for part in rest.split_whitespace() {
                    let server = part.trim_end_matches('#').trim();
                    if is_plausible_server(server) {
                        out.push((server.to_string(), "systemd-resolved".to_string()));
                    }
                }
            }
        }
    }

    Ok(out)
}

fn discover_resolved_conf_servers() -> Result<Vec<(String, String)>> {
    let path = Path::new("/etc/systemd/resolved.conf");
    if !path.exists() {
        return Ok(Vec::new());
    }
    let content = fs::read_to_string(path).context("read resolved.conf")?;
    let mut out = Vec::new();

    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with('#') || trimmed.is_empty() {
            continue;
        }
        if let Some(rest) = trimmed.strip_prefix("DNS=") {
            for part in rest.split_whitespace() {
                let server = part.trim();
                if is_plausible_server(server) {
                    out.push((server.to_string(), "resolved.conf".to_string()));
                }
            }
        }
    }

    Ok(out)
}

fn discover_resolv_conf_servers() -> Result<Vec<(String, String)>> {
    let path = Path::new("/etc/resolv.conf");
    if !path.exists() {
        return Ok(Vec::new());
    }
    let content = fs::read_to_string(path).context("read resolv.conf")?;
    let mut out = Vec::new();

    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with('#') {
            continue;
        }
        if let Some(rest) = trimmed.strip_prefix("nameserver") {
            let server = rest.trim();
            if is_plausible_server(server) {
                out.push((server.to_string(), "resolv.conf".to_string()));
            }
        }
    }

    Ok(out)
}

fn is_plausible_server(server: &str) -> bool {
    !server.is_empty()
        && server != "127.0.0.53"
        && server != "127.0.0.54"
        && server.parse::<std::net::IpAddr>().is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_resolv_conf() {
        let text = "nameserver 8.8.8.8\nnameserver 1.1.1.1\n";
        let mut out = Vec::new();
        for line in text.lines() {
            if let Some(rest) = line.trim().strip_prefix("nameserver") {
                out.push(rest.trim().to_string());
            }
        }
        assert_eq!(out, vec!["8.8.8.8", "1.1.1.1"]);
    }

    #[test]
    fn skips_loopback_stub() {
        assert!(!is_plausible_server("127.0.0.53"));
        assert!(is_plausible_server("8.8.8.8"));
    }
}
