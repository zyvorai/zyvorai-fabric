// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: LicenseRef-Zyvor-Production-1.0

use anyhow::Result;
use tracing;

use crate::models::*;

const TABLE_NAME: &str = "zyvor-fabricd_services";

/// Generates and applies nftables DNAT rules for service load balancing.
///
/// nftables structure:
/// ```text
/// table ip zyvor-fabricd_services {
///     chain svc_dnat {
///         type nat hook prerouting priority -100;
///         ip daddr 10.0.0.100 tcp dport 80 dnat to numgen inc mod 2 map { 0: 10.0.0.5, 1: 10.0.0.6 }
///     }
/// }
/// ```
pub struct ServiceEnforcer;

impl Default for ServiceEnforcer {
    fn default() -> Self {
        Self::new()
    }
}

impl ServiceEnforcer {
    pub fn new() -> Self {
        Self
    }

    /// Ensure the services table and DNAT chain exist.
    pub fn ensure_table(&self) -> Result<()> {
        run_nft(&format!("add table ip {}", TABLE_NAME))?;
        run_nft(&format!(
            "add chain ip {} svc_dnat {{ type nat hook prerouting priority -100; }}",
            TABLE_NAME
        ))?;
        tracing::debug!("Service mesh table ensured");
        Ok(())
    }

    /// Sync DNAT rules: flush and re-add all rules.
    pub fn sync_rules(&self, rules: &[CompiledDnatRule]) -> Result<()> {
        self.ensure_table()?;

        // Flush existing rules
        run_nft(&format!("flush chain ip {} svc_dnat", TABLE_NAME))?;

        // Add each DNAT rule
        for rule in rules {
            let rule_str = self.build_dnat_rule(rule);
            run_nft(&format!("add rule ip {} svc_dnat {}", TABLE_NAME, rule_str))?;
        }

        tracing::info!("Synced {} service DNAT rules", rules.len());
        Ok(())
    }

    /// Remove the entire services table.
    pub fn cleanup(&self) -> Result<()> {
        let _ = run_nft(&format!("delete table ip {}", TABLE_NAME));
        tracing::info!("Cleaned up services table");
        Ok(())
    }

    /// Build an nftables DNAT rule string from a compiled rule.
    pub fn build_dnat_rule(&self, rule: &CompiledDnatRule) -> String {
        let proto = match rule.protocol {
            ServiceProtocol::Tcp => "tcp",
            ServiceProtocol::Udp => "udp",
        };

        let dnat_target = if rule.backend_ips.len() == 1 {
            // Single backend: direct DNAT
            if rule.target_port == rule.port {
                rule.backend_ips[0].clone()
            } else {
                format!("{}:{}", rule.backend_ips[0], rule.target_port)
            }
        } else {
            // Multiple backends: use numgen for load balancing
            let map_entries: String = match rule.algorithm {
                LoadBalancerAlgorithm::RoundRobin | LoadBalancerAlgorithm::Random => rule
                    .backend_ips
                    .iter()
                    .enumerate()
                    .map(|(i, ip)| {
                        if rule.target_port == rule.port {
                            format!("{}: {}", i, ip)
                        } else {
                            format!("{}: {}:{}", i, ip, rule.target_port)
                        }
                    })
                    .collect::<Vec<_>>()
                    .join(", "),
                LoadBalancerAlgorithm::IpHash => rule
                    .backend_ips
                    .iter()
                    .enumerate()
                    .map(|(i, ip)| {
                        if rule.target_port == rule.port {
                            format!("{}: {}", i, ip)
                        } else {
                            format!("{}: {}:{}", i, ip, rule.target_port)
                        }
                    })
                    .collect::<Vec<_>>()
                    .join(", "),
            };

            let numgen = match rule.algorithm {
                LoadBalancerAlgorithm::RoundRobin => {
                    format!("numgen inc mod {}", rule.backend_ips.len())
                }
                LoadBalancerAlgorithm::Random => {
                    format!("numgen random mod {}", rule.backend_ips.len())
                }
                LoadBalancerAlgorithm::IpHash => {
                    format!("jhash ip saddr mod {}", rule.backend_ips.len())
                }
            };

            format!("{} map {{ {} }}", numgen, map_entries)
        };

        format!(
            "ip daddr {} {} dport {} dnat to {}",
            rule.virtual_ip, proto, rule.port, dnat_target
        )
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

    fn make_enforcer() -> ServiceEnforcer {
        ServiceEnforcer::new()
    }

    #[test]
    fn test_build_round_robin_dnat() {
        let enforcer = make_enforcer();
        let rule = CompiledDnatRule {
            virtual_ip: "10.0.0.100".to_string(),
            port: 80,
            target_port: 80,
            protocol: ServiceProtocol::Tcp,
            backend_ips: vec!["10.0.0.5".to_string(), "10.0.0.6".to_string()],
            algorithm: LoadBalancerAlgorithm::RoundRobin,
        };

        let result = enforcer.build_dnat_rule(&rule);
        assert!(result.contains("ip daddr 10.0.0.100"));
        assert!(result.contains("tcp dport 80"));
        assert!(result.contains("numgen inc mod 2"));
        assert!(result.contains("0: 10.0.0.5"));
        assert!(result.contains("1: 10.0.0.6"));
    }

    #[test]
    fn test_single_backend() {
        let enforcer = make_enforcer();
        let rule = CompiledDnatRule {
            virtual_ip: "10.0.0.100".to_string(),
            port: 80,
            target_port: 8080,
            protocol: ServiceProtocol::Tcp,
            backend_ips: vec!["10.0.0.5".to_string()],
            algorithm: LoadBalancerAlgorithm::RoundRobin,
        };

        let result = enforcer.build_dnat_rule(&rule);
        assert!(result.contains("dnat to 10.0.0.5:8080"));
        assert!(!result.contains("numgen"));
    }

    #[test]
    fn test_udp_rule() {
        let enforcer = make_enforcer();
        let rule = CompiledDnatRule {
            virtual_ip: "10.0.0.100".to_string(),
            port: 53,
            target_port: 53,
            protocol: ServiceProtocol::Udp,
            backend_ips: vec!["10.0.0.5".to_string()],
            algorithm: LoadBalancerAlgorithm::RoundRobin,
        };

        let result = enforcer.build_dnat_rule(&rule);
        assert!(result.contains("udp dport 53"));
    }

    #[test]
    fn test_multiple_ports() {
        let enforcer = make_enforcer();

        let rule80 = CompiledDnatRule {
            virtual_ip: "10.0.0.100".to_string(),
            port: 80,
            target_port: 80,
            protocol: ServiceProtocol::Tcp,
            backend_ips: vec!["10.0.0.5".to_string()],
            algorithm: LoadBalancerAlgorithm::RoundRobin,
        };
        let rule443 = CompiledDnatRule {
            virtual_ip: "10.0.0.100".to_string(),
            port: 443,
            target_port: 443,
            protocol: ServiceProtocol::Tcp,
            backend_ips: vec!["10.0.0.5".to_string()],
            algorithm: LoadBalancerAlgorithm::RoundRobin,
        };

        let r80 = enforcer.build_dnat_rule(&rule80);
        let r443 = enforcer.build_dnat_rule(&rule443);
        assert!(r80.contains("tcp dport 80"));
        assert!(r443.contains("tcp dport 443"));
    }

    #[test]
    fn test_cleanup_is_safe() {
        let enforcer = make_enforcer();
        // cleanup() should not panic even if table doesn't exist
        let result = enforcer.cleanup();
        assert!(result.is_ok());
    }
}
