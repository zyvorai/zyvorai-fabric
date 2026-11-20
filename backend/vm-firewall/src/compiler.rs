// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: LicenseRef-Zyvor-Production-1.0

use std::collections::HashMap;

use crate::models::*;

/// A snapshot of a VM's state for firewall compilation.
pub struct VMSnapshot {
    pub name: String,
    pub ip: Option<String>,
}

/// Compiles VM firewall assignments and profiles into per-VM nftables chains.
pub struct FirewallCompiler;

impl Default for FirewallCompiler {
    fn default() -> Self {
        Self::new()
    }
}

impl FirewallCompiler {
    pub fn new() -> Self {
        Self
    }

    /// Compile a single VM's firewall chain.
    pub fn compile_vm(
        &self,
        vm_name: &str,
        vm_ip: &str,
        profile: &FirewallProfile,
    ) -> CompiledFirewallChain {
        let mut sorted_rules = profile.rules.clone();
        sorted_rules.sort_by_key(|r| r.priority);

        // Sanitize VM name for nft chain name (replace dashes with underscores)
        let chain_name = format!("vm_{}_in", vm_name.replace('-', "_"));

        CompiledFirewallChain {
            vm_name: vm_name.to_string(),
            vm_ip: vm_ip.to_string(),
            chain_name,
            default_action: profile.default_action.clone(),
            rules: sorted_rules,
        }
    }

    /// Compile all VM assignments into firewall chains.
    pub fn compile_all(
        &self,
        assignments: &[VMFirewallAssignment],
        profiles: &HashMap<Uuid, FirewallProfile>,
        vms: &[VMSnapshot],
    ) -> Vec<CompiledFirewallChain> {
        let vm_map: HashMap<&str, &VMSnapshot> =
            vms.iter().map(|vm| (vm.name.as_str(), vm)).collect();

        assignments
            .iter()
            .filter_map(|assignment| {
                let profile = profiles.get(&assignment.profile_id)?;
                let vm = vm_map.get(assignment.vm_name.as_str())?;
                let ip = vm.ip.as_ref()?;

                Some(self.compile_vm(&assignment.vm_name, ip, profile))
            })
            .collect()
    }
}

use uuid::Uuid;

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;

    fn make_profile(
        name: &str,
        rules: Vec<FirewallRule>,
        default: FirewallAction,
    ) -> FirewallProfile {
        FirewallProfile {
            id: Uuid::new_v4(),
            name: name.to_string(),
            description: String::new(),
            default_action: default,
            rules,
            managed: true,
            created: Utc::now(),
            updated: Utc::now(),
        }
    }

    fn make_rule(
        priority: u16,
        action: FirewallAction,
        protocol: FirewallProtocol,
        port: Option<u16>,
    ) -> FirewallRule {
        FirewallRule {
            priority,
            action,
            protocol,
            source_cidr: None,
            dest_port: port,
            dest_port_end: None,
            rate_limit: None,
            log_prefix: None,
            description: String::new(),
        }
    }

    #[test]
    fn test_basic_compilation() {
        let compiler = FirewallCompiler::new();
        let profile = make_profile(
            "web",
            vec![make_rule(
                10,
                FirewallAction::Accept,
                FirewallProtocol::Tcp,
                Some(80),
            )],
            FirewallAction::Drop,
        );

        let chain = compiler.compile_vm("web-1", "10.0.0.5", &profile);
        assert_eq!(chain.vm_name, "web-1");
        assert_eq!(chain.vm_ip, "10.0.0.5");
        assert_eq!(chain.chain_name, "vm_web_1_in");
        assert_eq!(chain.rules.len(), 1);
    }

    #[test]
    fn test_priority_sorting() {
        let compiler = FirewallCompiler::new();
        let profile = make_profile(
            "web",
            vec![
                make_rule(100, FirewallAction::Drop, FirewallProtocol::Any, None),
                make_rule(10, FirewallAction::Accept, FirewallProtocol::Tcp, Some(80)),
                make_rule(50, FirewallAction::Accept, FirewallProtocol::Tcp, Some(443)),
            ],
            FirewallAction::Drop,
        );

        let chain = compiler.compile_vm("web-1", "10.0.0.5", &profile);
        assert_eq!(chain.rules[0].priority, 10);
        assert_eq!(chain.rules[1].priority, 50);
        assert_eq!(chain.rules[2].priority, 100);
    }

    #[test]
    fn test_default_action() {
        let compiler = FirewallCompiler::new();
        let profile = make_profile("strict", vec![], FirewallAction::Drop);

        let chain = compiler.compile_vm("vm-1", "10.0.0.5", &profile);
        assert_eq!(chain.default_action, FirewallAction::Drop);
    }

    #[test]
    fn test_no_rules() {
        let compiler = FirewallCompiler::new();
        let profile = make_profile("empty", vec![], FirewallAction::Accept);

        let chain = compiler.compile_vm("vm-1", "10.0.0.5", &profile);
        assert!(chain.rules.is_empty());
        assert_eq!(chain.default_action, FirewallAction::Accept);
    }

    #[test]
    fn test_rate_limit_inclusion() {
        let compiler = FirewallCompiler::new();
        let mut rule = make_rule(10, FirewallAction::Accept, FirewallProtocol::Tcp, Some(22));
        rule.rate_limit = Some(RateLimit {
            rate: 5,
            per: RatePer::Minute,
        });

        let profile = make_profile("ssh", vec![rule], FirewallAction::Drop);

        let chain = compiler.compile_vm("vm-1", "10.0.0.5", &profile);
        assert!(chain.rules[0].rate_limit.is_some());
        assert_eq!(
            chain.rules[0].rate_limit.as_ref().unwrap().to_nft_string(),
            "5/minute"
        );
    }

    #[test]
    fn test_log_prefix() {
        let compiler = FirewallCompiler::new();
        let mut rule = make_rule(999, FirewallAction::Log, FirewallProtocol::Any, None);
        rule.log_prefix = Some("vm-drop: ".to_string());

        let profile = make_profile("logged", vec![rule], FirewallAction::Drop);

        let chain = compiler.compile_vm("vm-1", "10.0.0.5", &profile);
        assert_eq!(chain.rules[0].log_prefix.as_deref(), Some("vm-drop: "));
    }

    #[test]
    fn test_icmp_rule() {
        let compiler = FirewallCompiler::new();
        let rule = make_rule(50, FirewallAction::Accept, FirewallProtocol::Icmp, None);

        let profile = make_profile("icmp", vec![rule], FirewallAction::Drop);

        let chain = compiler.compile_vm("vm-1", "10.0.0.5", &profile);
        assert_eq!(chain.rules[0].protocol, FirewallProtocol::Icmp);
    }

    #[test]
    fn test_multiple_vms() {
        let compiler = FirewallCompiler::new();
        let profile = make_profile(
            "web",
            vec![make_rule(
                10,
                FirewallAction::Accept,
                FirewallProtocol::Tcp,
                Some(80),
            )],
            FirewallAction::Drop,
        );

        let profile_id = profile.id;
        let mut profiles = HashMap::new();
        profiles.insert(profile_id, profile);

        let assignments = vec![
            VMFirewallAssignment {
                vm_name: "web-1".to_string(),
                profile_id,
                zone_id: None,
            },
            VMFirewallAssignment {
                vm_name: "web-2".to_string(),
                profile_id,
                zone_id: None,
            },
        ];

        let vms = vec![
            VMSnapshot {
                name: "web-1".to_string(),
                ip: Some("10.0.0.5".to_string()),
            },
            VMSnapshot {
                name: "web-2".to_string(),
                ip: Some("10.0.0.6".to_string()),
            },
        ];

        let chains = compiler.compile_all(&assignments, &profiles, &vms);
        assert_eq!(chains.len(), 2);
        assert_eq!(chains[0].vm_ip, "10.0.0.5");
        assert_eq!(chains[1].vm_ip, "10.0.0.6");
    }
}
