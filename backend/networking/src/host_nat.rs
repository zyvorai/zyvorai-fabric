// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: LicenseRef-Zyvor-Production-1.0

use anyhow::{Context, Result};
use std::process::Command;

use crate::nftables::NftManager;

/// NAT rule discovered from the host nftables ruleset (not zyvor-fabricd-managed).
#[derive(Debug, Clone)]
pub struct DiscoveredHostNatRule {
    pub key: String,
    pub name: String,
    pub rule_type: String,
    pub description: String,
    pub protocol: Option<String>,
    pub source_cidr: Option<String>,
    pub dest_cidr: Option<String>,
    pub dest_port: Option<u16>,
    pub translate_to: Option<String>,
    pub translate_port: Option<u16>,
    pub outbound_interface: Option<String>,
}

/// Discover masquerade, SNAT, and DNAT rules from nftables, with iptables-save fallback.
pub fn discover_host_nat_rules() -> Result<Vec<DiscoveredHostNatRule>> {
    let nft = NftManager::new();
    let mut rules = parse_nat_rules_from_ruleset(&nft.list_ruleset().unwrap_or_default());
    if rules.is_empty() {
        if let Ok(ipt) = discover_iptables_nat_rules() {
            rules = ipt;
        }
    }
    Ok(rules)
}

/// Fallback: parse `iptables-save -t nat` when nft discovery is empty.
fn discover_iptables_nat_rules() -> Result<Vec<DiscoveredHostNatRule>> {
    let output = Command::new("iptables-save")
        .arg("-t")
        .arg("nat")
        .output()
        .context("Failed to run iptables-save -t nat")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        if stderr.contains("not found") {
            return Ok(Vec::new());
        }
        return Err(anyhow::anyhow!("iptables-save failed: {stderr}"));
    }

    let text = String::from_utf8_lossy(&output.stdout);
    let mut rules = Vec::new();
    let mut seen = std::collections::HashSet::new();

    for line in text.lines() {
        let line = line.trim();
        if !line.starts_with("-A ") {
            continue;
        }
        if let Some(rule) = parse_iptables_nat_line(line) {
            if seen.insert(rule.key.clone()) {
                rules.push(rule);
            }
        }
    }
    Ok(rules)
}

fn parse_iptables_nat_line(line: &str) -> Option<DiscoveredHostNatRule> {
    let chain = line.split_whitespace().nth(1)?;
    let (rule_type, translate_to, translate_port, dest_port) = if line.contains("MASQUERADE") {
        ("masquerade", None, None, None)
    } else if line.contains("SNAT") {
        let to = extract_iptables_to(line);
        ("snat", to.0, to.1, None)
    } else if line.contains("DNAT") {
        let to = extract_iptables_to(line);
        ("dnat", to.0, to.1, extract_dport(line))
    } else {
        return None;
    };

    let key = format!("iptables:{chain}:{line}");
    Some(DiscoveredHostNatRule {
        key: key.clone(),
        name: format!("iptables-{chain}-{rule_type}"),
        rule_type: rule_type.to_string(),
        description: format!("Host iptables NAT ({chain})"),
        protocol: if line.contains(" tcp ") || line.contains(" -p tcp") {
            Some("tcp".to_string())
        } else if line.contains(" udp ") || line.contains(" -p udp") {
            Some("udp".to_string())
        } else {
            None
        },
        source_cidr: None,
        dest_cidr: None,
        dest_port,
        translate_to,
        translate_port,
        outbound_interface: None,
    })
}

fn extract_iptables_to(line: &str) -> (Option<String>, Option<u16>) {
    let marker = if line.contains("--to-destination") {
        "--to-destination"
    } else if line.contains("--to-source") {
        "--to-source"
    } else {
        return (None, None);
    };
    let Some(rest) = line.split(marker).nth(1) else {
        return (None, None);
    };
    let Some(value) = rest.split_whitespace().next() else {
        return (None, None);
    };
    if let Some((ip, port)) = value.split_once(':') {
        return (Some(ip.to_string()), port.parse().ok());
    }
    (Some(value.to_string()), None)
}

fn extract_dport(line: &str) -> Option<u16> {
    line.split_whitespace()
        .collect::<Vec<_>>()
        .windows(2)
        .find(|w| w[0] == "--dport")
        .and_then(|w| w[1].parse().ok())
}

fn parse_nat_rules_from_ruleset(json: &serde_json::Value) -> Vec<DiscoveredHostNatRule> {
    let Some(items) = json.get("nftables").and_then(|v| v.as_array()) else {
        return Vec::new();
    };

    let mut rules = Vec::new();
    let mut seen = std::collections::HashSet::new();

    for item in items {
        let Some(rule) = item.get("rule") else {
            continue;
        };
        let table = rule.get("table").and_then(|t| t.as_str()).unwrap_or("?");
        let chain = rule.get("chain").and_then(|c| c.as_str()).unwrap_or("?");
        let handle = rule.get("handle").and_then(|h| h.as_u64()).unwrap_or(0);
        let exprs = rule.get("expr").and_then(|e| e.as_array());
        let rule_comment = rule.get("comment").and_then(|c| c.as_str());

        if let Some(parsed) = parse_nat_exprs(exprs, rule_comment, table, chain) {
            let key = format!(
                "{}:{}:{}:{}:{}:{}",
                parsed.rule_type,
                table,
                chain,
                handle,
                parsed.dest_port.unwrap_or(0),
                parsed.translate_to.as_deref().unwrap_or("")
            );
            if !seen.insert(key.clone()) {
                continue;
            }
            rules.push(DiscoveredHostNatRule {
                key,
                name: parsed.name,
                rule_type: parsed.rule_type,
                description: format!("Host nftables NAT ({table}/{chain})"),
                protocol: parsed.protocol,
                source_cidr: parsed.source_cidr,
                dest_cidr: parsed.dest_cidr,
                dest_port: parsed.dest_port,
                translate_to: parsed.translate_to,
                translate_port: parsed.translate_port,
                outbound_interface: parsed.outbound_interface,
            });
        }
    }

    rules
}

struct ParsedNat {
    name: String,
    rule_type: String,
    protocol: Option<String>,
    source_cidr: Option<String>,
    dest_cidr: Option<String>,
    dest_port: Option<u16>,
    translate_to: Option<String>,
    translate_port: Option<u16>,
    outbound_interface: Option<String>,
}

fn parse_nat_exprs(
    exprs: Option<&Vec<serde_json::Value>>,
    rule_comment: Option<&str>,
    table: &str,
    chain: &str,
) -> Option<ParsedNat> {
    let exprs = exprs?;

    // `nft -j` puts a rule's comment as a field of the rule object itself,
    // not nested inside any `expr[]` element -- the loop below used to look
    // for `expr.comment` and could never find one, so every discovered NAT
    // rule fell back to its generic "{table}-{chain}-{rule_type}" name
    // instead of the name it was actually created with.
    let name = rule_comment
        .filter(|c| !c.starts_with("vm-nat-"))
        .map(str::to_string);
    let mut rule_type = None;
    let mut protocol = None;
    let mut source_cidr = None;
    let mut dest_cidr = None;
    let mut dest_port = None;
    let mut translate_to = None;
    let mut translate_port = None;
    let mut outbound_interface = None;

    for expr in exprs {
        if expr.get("masquerade").is_some() {
            rule_type = Some("masquerade".to_string());
            continue;
        }
        if let Some(dnat) = expr.get("dnat") {
            rule_type = Some("dnat".to_string());
            translate_to = dnat
                .get("addr")
                .and_then(|v| v.as_str())
                .map(str::to_string);
            translate_port = dnat.get("port").and_then(|v| v.as_u64()).map(|p| p as u16);
            continue;
        }
        if let Some(snat) = expr.get("snat") {
            rule_type = Some("snat".to_string());
            translate_to = snat
                .get("addr")
                .and_then(|v| v.as_str())
                .map(str::to_string);
            translate_port = snat.get("port").and_then(|v| v.as_u64()).map(|p| p as u16);
            continue;
        }
        if let Some(m) = expr.get("match") {
            if let Some(payload) = m.get("left").and_then(|l| l.get("payload")) {
                let proto = payload.get("protocol").and_then(|p| p.as_str());
                let field = payload.get("field").and_then(|f| f.as_str());
                if field == Some("dport") {
                    dest_port = m.get("right").and_then(|r| r.as_u64()).map(|p| p as u16);
                    if proto == Some("tcp") {
                        protocol = Some("tcp".to_string());
                    } else if proto == Some("udp") {
                        protocol = Some("udp".to_string());
                    }
                }
            }
            if let Some(ip) = m.get("left").and_then(|l| l.get("payload")) {
                let field = ip.get("field").and_then(|f| f.as_str());
                if let Some(right) = m.get("right").and_then(|r| r.as_str()) {
                    match field {
                        Some("saddr") => source_cidr = Some(right.to_string()),
                        Some("daddr") => dest_cidr = Some(right.to_string()),
                        _ => {}
                    }
                }
            }
            if let Some(meta) = m.get("left").and_then(|l| l.get("meta")) {
                let key = meta.get("key").and_then(|k| k.as_str());
                if key == Some("oifname") || key == Some("iifname") {
                    if let Some(iface) = m.get("right").and_then(|r| r.as_str()) {
                        outbound_interface = Some(iface.to_string());
                    }
                }
            }
        }
    }

    let rule_type = rule_type?;
    let name = name.unwrap_or_else(|| format!("{table}-{chain}-{rule_type}"));

    Some(ParsedNat {
        name,
        rule_type,
        protocol,
        source_cidr,
        dest_cidr,
        dest_port,
        translate_to,
        translate_port,
        outbound_interface,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_masquerade_rule() {
        let json = serde_json::json!({
            "nftables": [{
                "rule": {
                    "family": "ip",
                    "table": "nat",
                    "chain": "postrouting",
                    "handle": 1,
                    "expr": [
                        {"match": {"op": "==", "left": {"payload": {"protocol": "ip", "field": "daddr"}}, "right": "192.168.122.0/24"}},
                        {"masquerade": null}
                    ]
                }
            }]
        });
        let rules = parse_nat_rules_from_ruleset(&json);
        assert_eq!(rules.len(), 1);
        assert_eq!(rules[0].rule_type, "masquerade");
    }
}
