// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: LicenseRef-Zyvor-Production-2.0

use anyhow::{Context, Result};
use std::process::Command;

/// A traffic-control qdisc discovered on a host interface.
#[derive(Debug, Clone)]
pub struct DiscoveredTcQdisc {
    pub key: String,
    pub name: String,
    pub description: String,
    pub interface: String,
    pub kind: String,
    pub rate_kbit: Option<u64>,
    pub ceil_kbit: Option<u64>,
}

/// Discover active qdiscs via `tc -j qdisc show` (text fallback).
pub fn discover_host_tc_qdiscs() -> Result<Vec<DiscoveredTcQdisc>> {
    let output = Command::new("tc")
        .args(["-j", "qdisc", "show"])
        .output()
        .context("Failed to run tc -j qdisc show")?;

    if output.status.success() {
        let json: serde_json::Value =
            serde_json::from_slice(&output.stdout).unwrap_or(serde_json::Value::Array(vec![]));
        let parsed = parse_tc_json_qdiscs(&json);
        if !parsed.is_empty() {
            return Ok(parsed);
        }
    }

    let text_out = Command::new("tc")
        .args(["qdisc", "show"])
        .output()
        .context("Failed to run tc qdisc show")?;
    if !text_out.status.success() {
        let stderr = String::from_utf8_lossy(&text_out.stderr);
        if stderr.contains("not found") {
            return Ok(Vec::new());
        }
        return Err(anyhow::anyhow!("tc qdisc show failed: {stderr}"));
    }
    Ok(parse_tc_text_qdiscs(&String::from_utf8_lossy(
        &text_out.stdout,
    )))
}

fn parse_tc_json_qdiscs(json: &serde_json::Value) -> Vec<DiscoveredTcQdisc> {
    let Some(items) = json.as_array() else {
        return Vec::new();
    };
    let mut out = Vec::new();
    let mut seen = std::collections::HashSet::new();

    for item in items {
        let kind = item
            .get("kind")
            .and_then(|k| k.as_str())
            .unwrap_or("unknown");
        if kind == "noqueue" {
            continue;
        }
        let dev = item
            .get("dev")
            .and_then(|d| d.as_str())
            .unwrap_or("unknown")
            .to_string();
        if dev == "lo" {
            continue;
        }
        let handle = item.get("handle").and_then(|h| h.as_str()).unwrap_or("0:");
        let key = format!("{dev}:{handle}:{kind}");
        if !seen.insert(key.clone()) {
            continue;
        }
        let (rate_kbit, ceil_kbit) = extract_tc_rates(item);
        out.push(DiscoveredTcQdisc {
            name: format!("host-tc-{dev}-{kind}"),
            description: format!("Host tc qdisc {kind} on {dev} (handle {handle})"),
            interface: dev.clone(),
            kind: kind.to_string(),
            rate_kbit,
            ceil_kbit,
            key,
        });
    }
    out
}

fn extract_tc_rates(item: &serde_json::Value) -> (Option<u64>, Option<u64>) {
    let Some(opts) = item.get("options") else {
        return (None, None);
    };
    if let Some(rate) = opts.get("rate").and_then(|r| r.as_u64()) {
        // tc JSON rate is typically bytes/s
        let kbit = rate.saturating_mul(8) / 1000;
        let ceil = opts
            .get("ceil")
            .or_else(|| opts.get("peakrate"))
            .and_then(|c| c.as_u64())
            .map(|c| c.saturating_mul(8) / 1000)
            .unwrap_or(kbit);
        return (Some(kbit.max(1)), Some(ceil.max(1)));
    }
    (None, None)
}

fn parse_tc_text_qdiscs(text: &str) -> Vec<DiscoveredTcQdisc> {
    let mut out = Vec::new();
    let mut seen = std::collections::HashSet::new();

    for line in text.lines() {
        let line = line.trim();
        if !line.starts_with("qdisc ") {
            continue;
        }
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() < 4 {
            continue;
        }
        let kind = parts[1];
        if kind == "noqueue" {
            continue;
        }
        let handle = parts[2];
        let dev = parts
            .iter()
            .position(|&p| p == "dev")
            .and_then(|i| parts.get(i + 1))
            .map(|s| s.to_string())
            .unwrap_or_else(|| "unknown".to_string());
        if dev == "lo" {
            continue;
        }
        let key = format!("{dev}:{handle}:{kind}");
        if !seen.insert(key.clone()) {
            continue;
        }
        let rate_kbit = parse_rate_token(line, "rate");
        let ceil_kbit = parse_rate_token(line, "ceil").or(rate_kbit);
        out.push(DiscoveredTcQdisc {
            name: format!("host-tc-{dev}-{kind}"),
            description: format!("Host tc qdisc {kind} on {dev}"),
            interface: dev.clone(),
            kind: kind.to_string(),
            rate_kbit,
            ceil_kbit,
            key,
        });
    }
    out
}

fn parse_rate_token(line: &str, token: &str) -> Option<u64> {
    let idx = line.find(token)?;
    let rest = &line[idx + token.len()..];
    let word = rest.split_whitespace().next()?;
    parse_bitrate(word)
}

fn parse_bitrate(s: &str) -> Option<u64> {
    let s = s.trim();
    if let Some(num) = s.strip_suffix("bit") {
        return num.parse().ok();
    }
    if let Some(num) = s.strip_suffix("kbit") {
        return num.parse().ok();
    }
    if let Some(num) = s.strip_suffix("mbit") {
        return num.parse::<u64>().ok().map(|n| n * 1000);
    }
    if let Some(num) = s.strip_suffix("gbit") {
        return num.parse::<u64>().ok().map(|n| n * 1_000_000);
    }
    None
}
