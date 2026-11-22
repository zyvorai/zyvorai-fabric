// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: LicenseRef-Zyvor-Production-2.0

use anyhow::{Context, Result};
use std::process::Command;

/// A TCP/UDP listener discovered on the host via `ss`.
#[derive(Debug, Clone)]
pub struct HostListener {
    pub key: String,
    pub name: String,
    pub description: String,
    pub protocol: String,
    pub bind_address: String,
    pub port: u16,
    pub process: Option<String>,
}

/// Discover listening sockets on the host (`ss -H -tulnp`).
pub fn discover_host_listeners() -> Result<Vec<HostListener>> {
    let output = Command::new("ss")
        .args(["-H", "-tulnp"])
        .output()
        .context("Failed to run ss -H -tulnp")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        if stderr.contains("not found") {
            return Ok(Vec::new());
        }
        return Err(anyhow::anyhow!("ss failed: {stderr}"));
    }

    let text = String::from_utf8_lossy(&output.stdout);
    Ok(parse_ss_output(&text))
}

fn parse_ss_output(text: &str) -> Vec<HostListener> {
    let mut listeners = Vec::new();
    let mut seen = std::collections::HashSet::new();

    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        if let Some(l) = parse_ss_line(line) {
            if seen.insert(l.key.clone()) {
                listeners.push(l);
            }
        }
    }

    listeners
}

fn parse_ss_line(line: &str) -> Option<HostListener> {
    let mut parts = line.split_whitespace();
    let proto = parts.next()?.to_lowercase();
    let _state = parts.next()?;
    let _recv = parts.next()?;
    let _send = parts.next()?;
    let local = parts.next()?;
    let _peer = parts.next()?;

    let (bind_address, port) = parse_local_addr(local)?;
    let process = line
        .split("users:((")
        .nth(1)
        .and_then(|rest| rest.split('"').nth(1))
        .map(str::to_string);

    let name = process
        .as_ref()
        .map(|p| format!("host-{proto}-{port}-{p}"))
        .unwrap_or_else(|| format!("host-{proto}-{port}"));

    let key = format!("{proto}:{bind_address}:{port}");
    let description = process
        .as_ref()
        .map(|p| format!("Host listener {proto}/{port} ({p})"))
        .unwrap_or_else(|| format!("Host listener {proto}/{port}"));

    Some(HostListener {
        key,
        name,
        description,
        protocol: proto,
        bind_address,
        port,
        process,
    })
}

fn parse_local_addr(local: &str) -> Option<(String, u16)> {
    if local.starts_with('[') {
        let end = local.find("]:")?;
        let addr = local[1..end].to_string();
        let port: u16 = local[end + 2..].parse().ok()?;
        return Some((addr, port));
    }
    let colon = local.rfind(':')?;
    let addr = local[..colon].to_string();
    let port: u16 = local[colon + 1..].parse().ok()?;
    Some((addr, port))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_tcp_line() {
        let line = r#"tcp LISTEN 0 128 0.0.0.0:22 0.0.0.0:* users:(("sshd",pid=1,fd=3))"#;
        let l = parse_ss_line(line).unwrap();
        assert_eq!(l.port, 22);
        assert_eq!(l.process.as_deref(), Some("sshd"));
    }
}
