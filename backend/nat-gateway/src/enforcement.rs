// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: LicenseRef-Zyvor-Production-2.0

use anyhow::{anyhow, Result};
use tracing;

use crate::models::{CompiledNatRule, NatChain, NatProtocol};

/// Known safe nftables NAT actions.
const ALLOWED_NAT_ACTIONS: &[&str] = &["masquerade", "accept", "drop", "reject"];

/// Validate that a NAT action string is safe for use in an nft command.
///
/// Accepts exact matches from the allowlist (`masquerade`, `accept`, `drop`, `reject`)
/// or `snat to`/`dnat to` followed by a valid IP address (optionally with `:port`).
fn validate_nat_action(action: &str) -> Result<()> {
    if action.is_empty() {
        return Err(anyhow!("NAT action must not be empty"));
    }

    // Check exact matches first
    if ALLOWED_NAT_ACTIONS.contains(&action) {
        return Ok(());
    }

    // Check "snat to <ip>[:<port>]" or "dnat to <ip>[:<port>]"
    for prefix in &["snat to ", "dnat to "] {
        if let Some(target) = action.strip_prefix(prefix) {
            // Target may be "ip:port" or just "ip"
            let ip_part = if let Some((ip, port)) = target.rsplit_once(':') {
                // Validate port
                port.parse::<u16>()
                    .map_err(|_| anyhow!("Invalid port in NAT action: '{}'", port))?;
                ip
            } else {
                target
            };

            // Validate IP address
            ip_part
                .parse::<std::net::IpAddr>()
                .map_err(|_| anyhow!("Invalid IP address in NAT action: '{}'", ip_part))?;

            return Ok(());
        }
    }

    Err(anyhow!(
        "Invalid NAT action: '{}'. Must be one of {:?}, or 'snat to <ip>[:<port>]' / 'dnat to <ip>[:<port>]'",
        action,
        ALLOWED_NAT_ACTIONS
    ))
}

/// nftables table name for NAT.
const TABLE_NAME: &str = "zyvor-fabricd_nat";

/// Generates and applies nftables rules in the `zyvor-fabricd_nat` table.
///
/// nftables structure:
/// ```text
/// table ip zyvor-fabricd_nat {
///     chain nat_prerouting { type nat hook prerouting priority -100; }
///     chain nat_postrouting { type nat hook postrouting priority 100; }
/// }
/// ```
pub struct NatEnforcer;

impl Default for NatEnforcer {
    fn default() -> Self {
        Self::new()
    }
}

impl NatEnforcer {
    pub fn new() -> Self {
        Self
    }

    /// Ensure the nftables table and chains exist.
    pub fn ensure_table(&self) -> Result<()> {
        // Create table (ignore if exists)
        let _ = run_nft(&format!("add table ip {}", TABLE_NAME));

        // Create prerouting chain
        let _ = run_nft(&format!(
            "add chain ip {} nat_prerouting {{ type nat hook prerouting priority -100; }}",
            TABLE_NAME
        ));

        // Create postrouting chain
        let _ = run_nft(&format!(
            "add chain ip {} nat_postrouting {{ type nat hook postrouting priority 100; }}",
            TABLE_NAME
        ));

        tracing::debug!("Ensured nftables table {}", TABLE_NAME);
        Ok(())
    }

    /// Sync all NAT rules by flushing and reapplying.
    pub fn sync_rules(&self, rules: &[CompiledNatRule]) -> Result<()> {
        // Flush existing rules
        let _ = run_nft(&format!("flush chain ip {} nat_prerouting", TABLE_NAME));
        let _ = run_nft(&format!("flush chain ip {} nat_postrouting", TABLE_NAME));

        for rule in rules {
            let rule_str = self.build_nat_rule_string(rule)?;
            let chain = match rule.chain {
                NatChain::Prerouting => "nat_prerouting",
                NatChain::Postrouting => "nat_postrouting",
            };
            run_nft(&format!(
                "add rule ip {} {} {}",
                TABLE_NAME, chain, rule_str
            ))?;
        }

        tracing::debug!("Synced {} NAT rules", rules.len());
        Ok(())
    }

    /// Full sync: ensure table + sync rules.
    pub fn sync_all(&self, rules: &[CompiledNatRule]) -> Result<()> {
        self.ensure_table()?;
        self.sync_rules(rules)?;
        tracing::info!("NAT sync complete: {} rules applied", rules.len());
        Ok(())
    }

    /// Remove the nftables table.
    pub fn cleanup(&self) -> Result<()> {
        let _ = run_nft(&format!("delete table ip {}", TABLE_NAME));
        tracing::info!("Cleaned up nftables table {}", TABLE_NAME);
        Ok(())
    }

    /// Build a single nftables rule string from a compiled rule.
    ///
    /// Validates that the action is from a known allowlist to prevent
    /// nft command injection.
    pub fn build_nat_rule_string(&self, rule: &CompiledNatRule) -> Result<String> {
        // Validate action before interpolating into the nft command
        validate_nat_action(&rule.action)?;

        let mut parts: Vec<String> = Vec::new();

        // Output interface match
        if let Some(ref oif) = rule.outbound_interface {
            parts.push(format!("oifname \"{}\"", oif));
        }

        // Protocol match
        match rule.protocol {
            NatProtocol::Tcp => parts.push("ip protocol tcp".to_string()),
            NatProtocol::Udp => parts.push("ip protocol udp".to_string()),
            NatProtocol::Any => {}
        }

        // Source match
        if let Some(ref src) = rule.source_match {
            parts.push(format!("ip saddr {}", src));
        }

        // Destination match
        if let Some(ref dst) = rule.dest_match {
            parts.push(format!("ip daddr {}", dst));
        }

        // Destination port match
        if let Some(port) = rule.dest_port {
            let proto = match rule.protocol {
                NatProtocol::Tcp => "tcp",
                NatProtocol::Udp => "udp",
                NatProtocol::Any => "tcp",
            };
            if let Some(end_port) = rule.dest_port_end {
                parts.push(format!("{} dport {}-{}", proto, port, end_port));
            } else {
                parts.push(format!("{} dport {}", proto, port));
            }
        }

        // Action
        parts.push(rule.action.clone());

        Ok(parts.join(" "))
    }
}

/// Execute an nft command.
fn run_nft(cmd: &str) -> Result<()> {
    let output = std::process::Command::new("nft")
        .args(cmd.split_whitespace())
        .output();

    match output {
        Ok(out) if out.status.success() => Ok(()),
        Ok(out) => {
            let stderr = String::from_utf8_lossy(&out.stderr);
            tracing::warn!("nft command failed: {} — {}", cmd, stderr);
            Err(anyhow::anyhow!("nft failed: {}", stderr))
        }
        Err(e) => {
            tracing::warn!("Failed to execute nft: {}", e);
            Err(anyhow::anyhow!("Failed to execute nft: {}", e))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::NatRuleType;

    fn make_enforcer() -> NatEnforcer {
        NatEnforcer::new()
    }

    #[allow(clippy::too_many_arguments)]
    fn make_rule(
        rule_type: NatRuleType,
        chain: NatChain,
        source: Option<&str>,
        dest: Option<&str>,
        protocol: NatProtocol,
        dest_port: Option<u16>,
        dest_port_end: Option<u16>,
        action: &str,
        oif: Option<&str>,
    ) -> CompiledNatRule {
        CompiledNatRule {
            rule_type,
            chain,
            source_match: source.map(|s| s.to_string()),
            dest_match: dest.map(|s| s.to_string()),
            protocol,
            dest_port,
            dest_port_end,
            action: action.to_string(),
            outbound_interface: oif.map(|s| s.to_string()),
            vm_ips: vec![],
        }
    }

    #[test]
    fn test_masquerade_rule() {
        let enforcer = make_enforcer();
        let rule = make_rule(
            NatRuleType::Masquerade,
            NatChain::Postrouting,
            Some("10.0.0.0/24"),
            None,
            NatProtocol::Any,
            None,
            None,
            "masquerade",
            Some("eth0"),
        );

        let result = enforcer.build_nat_rule_string(&rule).unwrap();
        assert!(result.contains("oifname \"eth0\""));
        assert!(result.contains("ip saddr 10.0.0.0/24"));
        assert!(result.contains("masquerade"));
    }

    #[test]
    fn test_snat_rule() {
        let enforcer = make_enforcer();
        let rule = make_rule(
            NatRuleType::Snat,
            NatChain::Postrouting,
            Some("10.0.0.0/24"),
            None,
            NatProtocol::Any,
            None,
            None,
            "snat to 203.0.113.10",
            Some("eth0"),
        );

        let result = enforcer.build_nat_rule_string(&rule).unwrap();
        assert!(result.contains("snat to 203.0.113.10"));
        assert!(result.contains("ip saddr 10.0.0.0/24"));
    }

    #[test]
    fn test_dnat_rule() {
        let enforcer = make_enforcer();
        let rule = make_rule(
            NatRuleType::Dnat,
            NatChain::Prerouting,
            None,
            Some("203.0.113.1"),
            NatProtocol::Tcp,
            Some(80),
            None,
            "dnat to 10.0.0.5:8080",
            None,
        );

        let result = enforcer.build_nat_rule_string(&rule).unwrap();
        assert!(result.contains("ip daddr 203.0.113.1"));
        assert!(result.contains("tcp dport 80"));
        assert!(result.contains("dnat to 10.0.0.5:8080"));
    }

    #[test]
    fn test_hairpin_rule() {
        let enforcer = make_enforcer();
        let rule = make_rule(
            NatRuleType::Hairpin,
            NatChain::Prerouting,
            Some("10.0.0.0/24"),
            Some("203.0.113.1"),
            NatProtocol::Tcp,
            Some(80),
            None,
            "dnat to 10.0.0.5:80",
            None,
        );

        let result = enforcer.build_nat_rule_string(&rule).unwrap();
        assert!(result.contains("ip saddr 10.0.0.0/24"));
        assert!(result.contains("ip daddr 203.0.113.1"));
        assert!(result.contains("dnat to 10.0.0.5:80"));
    }

    #[test]
    fn test_port_range() {
        let enforcer = make_enforcer();
        let rule = make_rule(
            NatRuleType::Dnat,
            NatChain::Prerouting,
            None,
            Some("203.0.113.1"),
            NatProtocol::Tcp,
            Some(8000),
            Some(9000),
            "dnat to 10.0.0.5",
            None,
        );

        let result = enforcer.build_nat_rule_string(&rule).unwrap();
        assert!(result.contains("tcp dport 8000-9000"));
    }

    #[test]
    fn test_protocol_in_rule() {
        let enforcer = make_enforcer();
        let rule = make_rule(
            NatRuleType::Dnat,
            NatChain::Prerouting,
            None,
            None,
            NatProtocol::Udp,
            Some(53),
            None,
            "dnat to 10.0.0.5",
            None,
        );

        let result = enforcer.build_nat_rule_string(&rule).unwrap();
        assert!(result.contains("ip protocol udp"));
        assert!(result.contains("udp dport 53"));
    }

    #[test]
    fn test_invalid_nat_action() {
        let enforcer = make_enforcer();
        let rule = make_rule(
            NatRuleType::Masquerade,
            NatChain::Postrouting,
            None,
            None,
            NatProtocol::Any,
            None,
            None,
            "; drop table",
            None,
        );
        assert!(enforcer.build_nat_rule_string(&rule).is_err());
    }

    #[test]
    fn test_cleanup_safe() {
        let enforcer = make_enforcer();
        let result = enforcer.cleanup();
        // cleanup should always succeed (ignores errors)
        assert!(result.is_ok());
    }
}
