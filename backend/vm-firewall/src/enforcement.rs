// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: LicenseRef-Zyvor-Production-2.0

use anyhow::Result;
use tracing;

use crate::models::*;

const TABLE_NAME: &str = "zyvor-fabricd_firewall";

/// Generates and applies per-VM nftables firewall rules.
///
/// nftables structure:
/// ```text
/// table ip zyvor-fabricd_firewall {
///     chain vm_web1_in {
///         ct state established,related accept
///         tcp dport 80 accept
///         tcp dport 443 accept
///         tcp dport 22 limit rate 5/minute accept
///         log prefix "vm-web1-drop: " counter drop
///     }
///     chain fw_forward {
///         type filter hook forward priority 5;
///         ip daddr 10.0.0.5 jump vm_web1_in
///     }
/// }
/// ```
pub struct FirewallEnforcer;

impl Default for FirewallEnforcer {
    fn default() -> Self {
        Self::new()
    }
}

impl FirewallEnforcer {
    pub fn new() -> Self {
        Self
    }

    /// Ensure the firewall table exists.
    pub fn ensure_table(&self) -> Result<()> {
        run_nft(&format!("add table ip {}", TABLE_NAME))?;
        tracing::debug!("Firewall table ensured");
        Ok(())
    }

    /// Sync all VM firewall chains and the forward chain.
    pub fn sync_chains(&self, chains: &[CompiledFirewallChain]) -> Result<()> {
        self.ensure_table()?;

        // Delete the entire table and recreate to avoid stale chains
        let _ = run_nft(&format!("delete table ip {}", TABLE_NAME));
        run_nft(&format!("add table ip {}", TABLE_NAME))?;

        // Create per-VM chains
        for chain in chains {
            // Create the VM chain
            run_nft(&format!("add chain ip {} {}", TABLE_NAME, chain.chain_name))?;

            // Always allow established/related connections first
            run_nft(&format!(
                "add rule ip {} {} ct state established,related accept",
                TABLE_NAME, chain.chain_name
            ))?;

            // Add rules sorted by priority
            for rule in &chain.rules {
                let rule_str = self.build_rule_string(rule);
                run_nft(&format!(
                    "add rule ip {} {} {}",
                    TABLE_NAME, chain.chain_name, rule_str
                ))?;
            }

            // Add default action at the end
            let default = match chain.default_action {
                FirewallAction::Accept => "accept",
                FirewallAction::Drop => "counter drop",
                FirewallAction::Reject => "reject",
                FirewallAction::Log => "log counter drop",
            };
            run_nft(&format!(
                "add rule ip {} {} {}",
                TABLE_NAME, chain.chain_name, default
            ))?;
        }

        // Create the forward chain with jumps to per-VM chains
        run_nft(&format!(
            "add chain ip {} fw_forward {{ type filter hook forward priority 5; }}",
            TABLE_NAME
        ))?;

        for chain in chains {
            let jump_rule = self.build_jump_rule(&chain.vm_ip, &chain.chain_name)?;
            run_nft(&format!(
                "add rule ip {} fw_forward {}",
                TABLE_NAME, jump_rule
            ))?;
        }

        tracing::info!("Synced {} VM firewall chains", chains.len());
        Ok(())
    }

    /// Remove the firewall table.
    pub fn cleanup(&self) -> Result<()> {
        let _ = run_nft(&format!("delete table ip {}", TABLE_NAME));
        tracing::info!("Cleaned up firewall table");
        Ok(())
    }

    /// Build an nft rule string from a firewall rule.
    pub fn build_rule_string(&self, rule: &FirewallRule) -> String {
        let mut parts = Vec::new();

        // Source CIDR
        if let Some(ref cidr) = rule.source_cidr {
            parts.push(format!("ip saddr {}", cidr));
        }

        // Protocol and port
        match rule.protocol {
            FirewallProtocol::Tcp => {
                parts.push("tcp".to_string());
                if let Some(port) = rule.dest_port {
                    if let Some(end_port) = rule.dest_port_end {
                        parts.push(format!("dport {}-{}", port, end_port));
                    } else {
                        parts.push(format!("dport {}", port));
                    }
                }
            }
            FirewallProtocol::Udp => {
                parts.push("udp".to_string());
                if let Some(port) = rule.dest_port {
                    if let Some(end_port) = rule.dest_port_end {
                        parts.push(format!("dport {}-{}", port, end_port));
                    } else {
                        parts.push(format!("dport {}", port));
                    }
                }
            }
            FirewallProtocol::Icmp => {
                parts.push("ip protocol icmp".to_string());
            }
            FirewallProtocol::Any => {
                // No protocol restriction
                if let Some(port) = rule.dest_port {
                    parts.push("meta l4proto { tcp, udp }".to_string());
                    if let Some(end_port) = rule.dest_port_end {
                        parts.push(format!("th dport {}-{}", port, end_port));
                    } else {
                        parts.push(format!("th dport {}", port));
                    }
                }
            }
        }

        // Rate limit
        if let Some(ref limit) = rule.rate_limit {
            parts.push(format!("limit rate {}", limit.to_nft_string()));
        }

        // Log prefix
        if let Some(ref prefix) = rule.log_prefix {
            parts.push(format!("log prefix \"{}\"", prefix));
        }

        // Action
        let action = match rule.action {
            FirewallAction::Accept => "accept",
            FirewallAction::Drop => "counter drop",
            FirewallAction::Reject => "reject",
            FirewallAction::Log => "log counter",
        };
        parts.push(action.to_string());

        parts.join(" ")
    }

    /// Build a jump rule for the forward chain.
    ///
    /// Validates that `vm_ip` is a valid IP address and `chain_name` contains
    /// only alphanumeric characters and underscores, to prevent nft command injection.
    pub fn build_jump_rule(&self, vm_ip: &str, chain_name: &str) -> Result<String> {
        // Validate vm_ip is a valid IP address
        vm_ip
            .parse::<std::net::IpAddr>()
            .map_err(|_| anyhow::anyhow!("Invalid VM IP address for jump rule: '{}'", vm_ip))?;

        // Validate chain_name is alphanumeric + underscores only
        if chain_name.is_empty()
            || !chain_name
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '_')
        {
            return Err(anyhow::anyhow!(
                "Invalid chain name for jump rule: '{}'. Must be alphanumeric and underscores only.",
                chain_name
            ));
        }

        Ok(format!("ip daddr {} jump {}", vm_ip, chain_name))
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

    fn make_enforcer() -> FirewallEnforcer {
        FirewallEnforcer::new()
    }

    #[test]
    fn test_build_tcp_accept_rule() {
        let enforcer = make_enforcer();
        let rule = FirewallRule {
            priority: 10,
            action: FirewallAction::Accept,
            protocol: FirewallProtocol::Tcp,
            source_cidr: None,
            dest_port: Some(80),
            dest_port_end: None,
            rate_limit: None,
            log_prefix: None,
            description: String::new(),
        };

        let result = enforcer.build_rule_string(&rule);
        assert_eq!(result, "tcp dport 80 accept");
    }

    #[test]
    fn test_rate_limit_rule() {
        let enforcer = make_enforcer();
        let rule = FirewallRule {
            priority: 20,
            action: FirewallAction::Accept,
            protocol: FirewallProtocol::Tcp,
            source_cidr: None,
            dest_port: Some(22),
            dest_port_end: None,
            rate_limit: Some(RateLimit {
                rate: 5,
                per: RatePer::Minute,
            }),
            log_prefix: None,
            description: String::new(),
        };

        let result = enforcer.build_rule_string(&rule);
        assert!(result.contains("tcp dport 22"));
        assert!(result.contains("limit rate 5/minute"));
        assert!(result.contains("accept"));
    }

    #[test]
    fn test_log_drop_rule() {
        let enforcer = make_enforcer();
        let rule = FirewallRule {
            priority: 999,
            action: FirewallAction::Drop,
            protocol: FirewallProtocol::Any,
            source_cidr: None,
            dest_port: None,
            dest_port_end: None,
            rate_limit: None,
            log_prefix: Some("vm-web1-drop: ".to_string()),
            description: String::new(),
        };

        let result = enforcer.build_rule_string(&rule);
        assert!(result.contains("log prefix \"vm-web1-drop: \""));
        assert!(result.contains("counter drop"));
    }

    #[test]
    fn test_icmp_rule() {
        let enforcer = make_enforcer();
        let rule = FirewallRule {
            priority: 50,
            action: FirewallAction::Accept,
            protocol: FirewallProtocol::Icmp,
            source_cidr: None,
            dest_port: None,
            dest_port_end: None,
            rate_limit: None,
            log_prefix: None,
            description: String::new(),
        };

        let result = enforcer.build_rule_string(&rule);
        assert!(result.contains("ip protocol icmp"));
        assert!(result.contains("accept"));
    }

    #[test]
    fn test_jump_rule() {
        let enforcer = make_enforcer();
        let result = enforcer.build_jump_rule("10.0.0.5", "vm_web1_in").unwrap();
        assert_eq!(result, "ip daddr 10.0.0.5 jump vm_web1_in");
    }

    #[test]
    fn test_jump_rule_invalid_ip() {
        let enforcer = make_enforcer();
        assert!(enforcer.build_jump_rule("not-an-ip", "vm_web1_in").is_err());
    }

    #[test]
    fn test_jump_rule_invalid_chain_name() {
        let enforcer = make_enforcer();
        assert!(enforcer.build_jump_rule("10.0.0.5", "chain;drop").is_err());
        assert!(enforcer.build_jump_rule("10.0.0.5", "").is_err());
    }
}
