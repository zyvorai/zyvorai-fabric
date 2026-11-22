// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: LicenseRef-Zyvor-Production-2.0

use anyhow::Result;
use tracing;

use crate::identity::IdentityAllocator;
use crate::models::*;

/// Generates and applies nftables rules using named sets for identity-based enforcement.
///
/// nftables structure:
/// ```text
/// table ip zyvor-fabricd_policy {
///     set identity_256 { type ipv4_addr; elements = { 10.0.0.5, 10.0.0.6 } }
///     chain policy_forward {
///         type filter hook forward priority 0; policy drop;
///         ct state established,related accept
///         ip saddr @identity_257 ip daddr @identity_256 tcp dport 80 accept
///     }
/// }
/// ```
pub struct PolicyEnforcer {
    allocator: IdentityAllocator,
}

const TABLE_NAME: &str = "zyvor-fabricd_policy";

impl PolicyEnforcer {
    pub fn new(allocator: IdentityAllocator) -> Self {
        Self { allocator }
    }

    /// Ensure the policy table and forward chain exist.
    pub fn ensure_policy_chains(&self) -> Result<()> {
        // Create table (idempotent via 'add')
        run_nft(&format!("add table ip {}", TABLE_NAME))?;

        // Create forward chain for inter-VM traffic
        // Using 'add' is idempotent; if it already exists, nft won't error
        run_nft(&format!(
            "add chain ip {} policy_forward {{ type filter hook forward priority 0; policy accept; }}",
            TABLE_NAME
        ))?;

        tracing::debug!("Policy chains ensured");
        Ok(())
    }

    /// Create or update identity sets with current IP mappings.
    pub fn sync_identity_sets(&self) -> Result<()> {
        let identities = self.allocator.list_identities();
        let ip_map = self.allocator.get_ip_map();

        // Build reverse map: identity_id → [ips]
        let mut id_ips: std::collections::HashMap<u32, Vec<String>> =
            std::collections::HashMap::new();
        for (ip, id) in &ip_map {
            id_ips.entry(*id).or_default().push(ip.clone());
        }

        for identity in &identities {
            let set_name = format!("identity_{}", identity.id);

            // Create the set (idempotent)
            run_nft(&format!(
                "add set ip {} {} {{ type ipv4_addr; }}",
                TABLE_NAME, set_name
            ))?;

            // Flush existing elements
            run_nft(&format!("flush set ip {} {}", TABLE_NAME, set_name))?;

            // Add current IPs
            if let Some(ips) = id_ips.get(&identity.id) {
                if !ips.is_empty() {
                    let elements = ips.join(", ");
                    run_nft(&format!(
                        "add element ip {} {} {{ {} }}",
                        TABLE_NAME, set_name, elements
                    ))?;
                }
            }
        }

        tracing::debug!("Synced {} identity sets", identities.len());
        Ok(())
    }

    /// Apply compiled rules by flushing the policy chain and adding new rules.
    pub fn apply_compiled_rules(&self, rules: &[CompiledRule]) -> Result<()> {
        // Flush the forward chain
        run_nft(&format!("flush chain ip {} policy_forward", TABLE_NAME))?;

        // Always allow established/related connections
        run_nft(&format!(
            "add rule ip {} policy_forward ct state established,related accept",
            TABLE_NAME
        ))?;

        // Add each compiled rule
        for rule in rules {
            let rule_str = self.build_rule_string(rule);
            run_nft(&format!(
                "add rule ip {} policy_forward {}",
                TABLE_NAME, rule_str
            ))?;
        }

        tracing::info!("Applied {} policy rules", rules.len());
        Ok(())
    }

    /// Set the default policy on the forward chain.
    /// When policies exist: default drop (whitelist mode).
    /// When no policies: default accept (no impact).
    pub fn set_default_policy(&self, has_policies: bool) -> Result<()> {
        let policy = if has_policies { "drop" } else { "accept" };

        // Recreate chain with new policy
        // 'add' is idempotent, but to change the policy we need to delete and recreate
        let _ = run_nft(&format!("delete chain ip {} policy_forward", TABLE_NAME));
        run_nft(&format!(
            "add chain ip {} policy_forward {{ type filter hook forward priority 0; policy {}; }}",
            TABLE_NAME, policy
        ))?;

        tracing::info!("Default forward policy set to {}", policy);
        Ok(())
    }

    /// Full sync: chains → sets → rules → default policy.
    pub fn sync_all(&self, rules: &[CompiledRule]) -> Result<()> {
        self.ensure_policy_chains()?;
        self.sync_identity_sets()?;
        self.set_default_policy(!rules.is_empty())?;
        self.apply_compiled_rules(rules)?;
        Ok(())
    }

    /// Remove all policy chains and sets.
    pub fn cleanup(&self) -> Result<()> {
        let _ = run_nft(&format!("delete table ip {}", TABLE_NAME));
        tracing::info!("Cleaned up policy table");
        Ok(())
    }

    /// Build the nft rule arguments for a compiled rule.
    pub fn build_rule_args(&self, rule: &CompiledRule) -> Vec<String> {
        let mut args = Vec::new();

        // Source identity set
        if rule.src_identity == IDENTITY_WORLD {
            // World identity: no source restriction
        } else {
            args.push("ip".to_string());
            args.push("saddr".to_string());
            args.push(format!("@identity_{}", rule.src_identity));
        }

        // Destination identity set
        if rule.dst_identity == IDENTITY_WORLD {
            // World identity: no destination restriction
        } else {
            args.push("ip".to_string());
            args.push("daddr".to_string());
            args.push(format!("@identity_{}", rule.dst_identity));
        }

        // Protocol and port
        match rule.protocol {
            PolicyProtocol::Tcp => {
                args.push("tcp".to_string());
                args.push("dport".to_string());
                if let Some(end_port) = rule.end_port {
                    args.push(format!("{}-{}", rule.port, end_port));
                } else {
                    args.push(rule.port.to_string());
                }
            }
            PolicyProtocol::Udp => {
                args.push("udp".to_string());
                args.push("dport".to_string());
                if let Some(end_port) = rule.end_port {
                    args.push(format!("{}-{}", rule.port, end_port));
                } else {
                    args.push(rule.port.to_string());
                }
            }
            PolicyProtocol::Any => {
                // No protocol restriction; only add port if non-zero
                if rule.port > 0 {
                    args.push("meta".to_string());
                    args.push("l4proto".to_string());
                    args.push("{ tcp, udp }".to_string());
                    args.push("th".to_string());
                    args.push("dport".to_string());
                    if let Some(end_port) = rule.end_port {
                        args.push(format!("{}-{}", rule.port, end_port));
                    } else {
                        args.push(rule.port.to_string());
                    }
                }
            }
        }

        args.push("accept".to_string());
        args
    }

    /// Build a full nft rule string from a compiled rule.
    fn build_rule_string(&self, rule: &CompiledRule) -> String {
        self.build_rule_args(rule).join(" ")
    }
}

/// Execute an nft command passed as a single rule string.
/// Uses `nft` with the full string as a single argument (not split on whitespace)
/// to correctly handle nftables syntax containing braces and commas.
fn run_nft(cmd: &str) -> Result<()> {
    let output = std::process::Command::new("nft").arg(cmd).output();

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

    fn make_enforcer() -> PolicyEnforcer {
        PolicyEnforcer::new(IdentityAllocator::new())
    }

    #[test]
    fn test_build_rule_args_tcp() {
        let enforcer = make_enforcer();
        let rule = CompiledRule {
            direction: Direction::Ingress,
            src_identity: 257,
            dst_identity: 256,
            protocol: PolicyProtocol::Tcp,
            port: 80,
            end_port: None,
            policy_name: "test".to_string(),
        };

        let args = enforcer.build_rule_args(&rule);
        assert!(args.contains(&"ip".to_string()));
        assert!(args.contains(&"@identity_257".to_string()));
        assert!(args.contains(&"@identity_256".to_string()));
        assert!(args.contains(&"tcp".to_string()));
        assert!(args.contains(&"80".to_string()));
        assert!(args.contains(&"accept".to_string()));
    }

    #[test]
    fn test_build_rule_args_udp() {
        let enforcer = make_enforcer();
        let rule = CompiledRule {
            direction: Direction::Ingress,
            src_identity: 258,
            dst_identity: 256,
            protocol: PolicyProtocol::Udp,
            port: 53,
            end_port: None,
            policy_name: "test".to_string(),
        };

        let args = enforcer.build_rule_args(&rule);
        assert!(args.contains(&"udp".to_string()));
        assert!(args.contains(&"53".to_string()));
    }

    #[test]
    fn test_build_rule_args_any_no_port() {
        let enforcer = make_enforcer();
        let rule = CompiledRule {
            direction: Direction::Ingress,
            src_identity: 257,
            dst_identity: 256,
            protocol: PolicyProtocol::Any,
            port: 0,
            end_port: None,
            policy_name: "test".to_string(),
        };

        let args = enforcer.build_rule_args(&rule);
        // Should not contain protocol-specific args
        assert!(!args.contains(&"tcp".to_string()));
        assert!(!args.contains(&"udp".to_string()));
        assert!(args.contains(&"accept".to_string()));
    }

    #[test]
    fn test_build_rule_args_port_range() {
        let enforcer = make_enforcer();
        let rule = CompiledRule {
            direction: Direction::Ingress,
            src_identity: 257,
            dst_identity: 256,
            protocol: PolicyProtocol::Tcp,
            port: 8000,
            end_port: Some(8100),
            policy_name: "test".to_string(),
        };

        let args = enforcer.build_rule_args(&rule);
        assert!(args.contains(&"8000-8100".to_string()));
    }

    #[test]
    fn test_build_rule_args_world_identity() {
        let enforcer = make_enforcer();
        let rule = CompiledRule {
            direction: Direction::Ingress,
            src_identity: IDENTITY_WORLD,
            dst_identity: 256,
            protocol: PolicyProtocol::Tcp,
            port: 443,
            end_port: None,
            policy_name: "test".to_string(),
        };

        let args = enforcer.build_rule_args(&rule);
        // World source → no @identity_0 set reference
        assert!(!args.contains(&"@identity_0".to_string()));
        // But destination should still be referenced
        assert!(args.contains(&"@identity_256".to_string()));
    }
}
