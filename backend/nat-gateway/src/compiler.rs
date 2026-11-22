// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: LicenseRef-Zyvor-Production-2.0

use std::collections::HashMap;

use crate::models::*;

/// A snapshot of a VM's state for NAT compilation.
pub struct VMSnapshot {
    pub name: String,
    pub labels: HashMap<String, String>,
    pub ip: Option<String>,
}

/// Compiles NAT rules and gateways against VM state.
pub struct NatCompiler;

impl Default for NatCompiler {
    fn default() -> Self {
        Self::new()
    }
}

impl NatCompiler {
    pub fn new() -> Self {
        Self
    }

    /// Compile a masquerade rule.
    pub fn compile_masquerade(
        &self,
        rule: &NatRule,
        all_vms: &[VMSnapshot],
    ) -> Option<CompiledNatRule> {
        if !rule.enabled {
            return None;
        }

        let vm_ips: Vec<String> = all_vms
            .iter()
            .filter(|vm| rule.selector.matches(&vm.labels) && vm.ip.is_some())
            .filter_map(|vm| vm.ip.clone())
            .collect();

        if vm_ips.is_empty() {
            return None;
        }

        let source_match = rule
            .source_cidr
            .clone()
            .or_else(|| Some(format!("{{ {} }}", vm_ips.join(", "))));

        Some(CompiledNatRule {
            rule_type: NatRuleType::Masquerade,
            chain: NatChain::Postrouting,
            source_match,
            dest_match: rule.dest_cidr.clone(),
            protocol: rule.protocol.clone(),
            dest_port: rule.dest_port,
            dest_port_end: rule.dest_port_end,
            action: "masquerade".to_string(),
            outbound_interface: rule.outbound_interface.clone(),
            vm_ips,
        })
    }

    /// Compile an SNAT rule using a pool.
    pub fn compile_snat(
        &self,
        rule: &NatRule,
        pools: &[NatPool],
        all_vms: &[VMSnapshot],
    ) -> Option<CompiledNatRule> {
        if !rule.enabled {
            return None;
        }

        let vm_ips: Vec<String> = all_vms
            .iter()
            .filter(|vm| rule.selector.matches(&vm.labels) && vm.ip.is_some())
            .filter_map(|vm| vm.ip.clone())
            .collect();

        if vm_ips.is_empty() {
            return None;
        }

        let snat_target = if let Some(ref pool_id) = rule.pool_id {
            pools
                .iter()
                .find(|p| &p.id == pool_id)
                .map(|p| p.ip_ranges.join("-"))
                .unwrap_or_else(|| rule.translate_to.clone().unwrap_or_default())
        } else {
            rule.translate_to.clone().unwrap_or_default()
        };

        let source_match = rule
            .source_cidr
            .clone()
            .or_else(|| Some(format!("{{ {} }}", vm_ips.join(", "))));

        Some(CompiledNatRule {
            rule_type: NatRuleType::Snat,
            chain: NatChain::Postrouting,
            source_match,
            dest_match: rule.dest_cidr.clone(),
            protocol: rule.protocol.clone(),
            dest_port: rule.dest_port,
            dest_port_end: rule.dest_port_end,
            action: format!("snat to {}", snat_target),
            outbound_interface: rule.outbound_interface.clone(),
            vm_ips,
        })
    }

    /// Compile a DNAT rule.
    pub fn compile_dnat(&self, rule: &NatRule) -> Option<CompiledNatRule> {
        if !rule.enabled {
            return None;
        }

        let translate = rule.translate_to.clone().unwrap_or_default();
        let action = if let Some(port) = rule.translate_port {
            format!("dnat to {}:{}", translate, port)
        } else {
            format!("dnat to {}", translate)
        };

        Some(CompiledNatRule {
            rule_type: NatRuleType::Dnat,
            chain: NatChain::Prerouting,
            source_match: rule.source_cidr.clone(),
            dest_match: rule.dest_cidr.clone(),
            protocol: rule.protocol.clone(),
            dest_port: rule.dest_port,
            dest_port_end: rule.dest_port_end,
            action,
            outbound_interface: None,
            vm_ips: vec![],
        })
    }

    /// Compile a hairpin NAT rule.
    pub fn compile_hairpin(
        &self,
        rule: &NatRule,
        all_vms: &[VMSnapshot],
    ) -> Option<CompiledNatRule> {
        if !rule.enabled {
            return None;
        }

        let vm_ips: Vec<String> = all_vms
            .iter()
            .filter(|vm| rule.selector.matches(&vm.labels) && vm.ip.is_some())
            .filter_map(|vm| vm.ip.clone())
            .collect();

        let translate = rule.translate_to.clone().unwrap_or_default();
        let action = if let Some(port) = rule.translate_port {
            format!("dnat to {}:{}", translate, port)
        } else {
            format!("dnat to {}", translate)
        };

        Some(CompiledNatRule {
            rule_type: NatRuleType::Hairpin,
            chain: NatChain::Prerouting,
            source_match: rule.source_cidr.clone(),
            dest_match: rule.dest_cidr.clone(),
            protocol: rule.protocol.clone(),
            dest_port: rule.dest_port,
            dest_port_end: rule.dest_port_end,
            action,
            outbound_interface: None,
            vm_ips,
        })
    }

    /// Compile a NAT gateway config into a masquerade rule.
    pub fn compile_gateway(
        &self,
        gw: &NatGatewayConfig,
        all_vms: &[VMSnapshot],
    ) -> Option<CompiledNatRule> {
        if !gw.enabled {
            return None;
        }

        let vm_ips: Vec<String> = all_vms
            .iter()
            .filter(|vm| gw.selector.matches(&vm.labels) && vm.ip.is_some())
            .filter_map(|vm| vm.ip.clone())
            .collect();

        Some(CompiledNatRule {
            rule_type: NatRuleType::Masquerade,
            chain: NatChain::Postrouting,
            source_match: Some(gw.subnet.clone()),
            dest_match: None,
            protocol: NatProtocol::Any,
            dest_port: None,
            dest_port_end: None,
            action: "masquerade".to_string(),
            outbound_interface: Some(gw.outbound_interface.clone()),
            vm_ips,
        })
    }

    /// Compile all NAT rules and gateways.
    pub fn compile_all(
        &self,
        rules: &[NatRule],
        gateways: &[NatGatewayConfig],
        pools: &[NatPool],
        all_vms: &[VMSnapshot],
    ) -> Vec<CompiledNatRule> {
        let mut compiled = Vec::new();

        for rule in rules {
            let result = match rule.rule_type {
                NatRuleType::Masquerade => self.compile_masquerade(rule, all_vms),
                NatRuleType::Snat => self.compile_snat(rule, pools, all_vms),
                NatRuleType::Dnat => self.compile_dnat(rule),
                NatRuleType::Hairpin => self.compile_hairpin(rule, all_vms),
            };
            if let Some(r) = result {
                compiled.push(r);
            }
        }

        for gw in gateways {
            if let Some(r) = self.compile_gateway(gw, all_vms) {
                compiled.push(r);
            }
        }

        compiled
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use uuid::Uuid;

    fn make_vm(name: &str, labels: &[(&str, &str)], ip: Option<&str>) -> VMSnapshot {
        VMSnapshot {
            name: name.to_string(),
            labels: labels
                .iter()
                .map(|(k, v)| (k.to_string(), v.to_string()))
                .collect(),
            ip: ip.map(|s| s.to_string()),
        }
    }

    fn make_rule(
        name: &str,
        rule_type: NatRuleType,
        selector: &[(&str, &str)],
        enabled: bool,
    ) -> NatRule {
        NatRule {
            id: Uuid::new_v4(),
            name: name.to_string(),
            description: String::new(),
            rule_type,
            selector: LabelSelector {
                match_labels: selector
                    .iter()
                    .map(|(k, v)| (k.to_string(), v.to_string()))
                    .collect(),
            },
            protocol: NatProtocol::Any,
            source_cidr: None,
            dest_cidr: None,
            dest_port: None,
            dest_port_end: None,
            translate_to: None,
            translate_port: None,
            pool_id: None,
            outbound_interface: Some("eth0".to_string()),
            enabled,
            managed: true,
            created: Utc::now(),
            updated: Utc::now(),
        }
    }

    #[test]
    fn test_masquerade() {
        let compiler = NatCompiler::new();
        let vms = vec![make_vm("web-1", &[("zone", "internal")], Some("10.0.0.5"))];
        let rule = make_rule(
            "masq",
            NatRuleType::Masquerade,
            &[("zone", "internal")],
            true,
        );

        let result = compiler.compile_masquerade(&rule, &vms);
        assert!(result.is_some());
        let compiled = result.unwrap();
        assert_eq!(compiled.rule_type, NatRuleType::Masquerade);
        assert_eq!(compiled.chain, NatChain::Postrouting);
        assert_eq!(compiled.action, "masquerade");
        assert_eq!(compiled.vm_ips.len(), 1);
    }

    #[test]
    fn test_snat_with_pool() {
        let compiler = NatCompiler::new();
        let pool_id = Uuid::new_v4();
        let vms = vec![make_vm("web-1", &[("zone", "internal")], Some("10.0.0.5"))];
        let pools = vec![NatPool {
            id: pool_id,
            name: "public".to_string(),
            ip_ranges: vec!["203.0.113.10".to_string()],
            port_range: None,
            created: Utc::now(),
            updated: Utc::now(),
        }];
        let mut rule = make_rule("snat", NatRuleType::Snat, &[("zone", "internal")], true);
        rule.pool_id = Some(pool_id);

        let result = compiler.compile_snat(&rule, &pools, &vms);
        assert!(result.is_some());
        let compiled = result.unwrap();
        assert_eq!(compiled.rule_type, NatRuleType::Snat);
        assert!(compiled.action.starts_with("snat to"));
    }

    #[test]
    fn test_dnat() {
        let compiler = NatCompiler::new();
        let mut rule = make_rule("dnat", NatRuleType::Dnat, &[], true);
        rule.dest_cidr = Some("203.0.113.1".to_string());
        rule.dest_port = Some(80);
        rule.translate_to = Some("10.0.0.5".to_string());
        rule.translate_port = Some(8080);
        rule.protocol = NatProtocol::Tcp;

        let result = compiler.compile_dnat(&rule);
        assert!(result.is_some());
        let compiled = result.unwrap();
        assert_eq!(compiled.rule_type, NatRuleType::Dnat);
        assert_eq!(compiled.chain, NatChain::Prerouting);
        assert_eq!(compiled.action, "dnat to 10.0.0.5:8080");
    }

    #[test]
    fn test_hairpin() {
        let compiler = NatCompiler::new();
        let vms = vec![make_vm("web-1", &[("zone", "internal")], Some("10.0.0.5"))];
        let mut rule = make_rule(
            "hairpin",
            NatRuleType::Hairpin,
            &[("zone", "internal")],
            true,
        );
        rule.translate_to = Some("10.0.0.5".to_string());

        let result = compiler.compile_hairpin(&rule, &vms);
        assert!(result.is_some());
        let compiled = result.unwrap();
        assert_eq!(compiled.rule_type, NatRuleType::Hairpin);
        assert!(compiled.action.starts_with("dnat to"));
    }

    #[test]
    fn test_gateway() {
        let compiler = NatCompiler::new();
        let vms = vec![make_vm("web-1", &[("zone", "internal")], Some("10.0.0.5"))];
        let gw = NatGatewayConfig {
            id: Uuid::new_v4(),
            name: "default-gw".to_string(),
            description: String::new(),
            subnet: "10.0.0.0/24".to_string(),
            outbound_interface: "eth0".to_string(),
            selector: LabelSelector::default(),
            enabled: true,
            created: Utc::now(),
            updated: Utc::now(),
        };

        let result = compiler.compile_gateway(&gw, &vms);
        assert!(result.is_some());
        let compiled = result.unwrap();
        assert_eq!(compiled.source_match, Some("10.0.0.0/24".to_string()));
        assert_eq!(compiled.outbound_interface, Some("eth0".to_string()));
    }

    #[test]
    fn test_disabled_rule() {
        let compiler = NatCompiler::new();
        let vms = vec![make_vm("web-1", &[("zone", "internal")], Some("10.0.0.5"))];
        let rule = make_rule(
            "masq",
            NatRuleType::Masquerade,
            &[("zone", "internal")],
            false,
        );

        let result = compiler.compile_masquerade(&rule, &vms);
        assert!(result.is_none());
    }

    #[test]
    fn test_no_matching_vms() {
        let compiler = NatCompiler::new();
        let vms = vec![make_vm("web-1", &[("zone", "public")], Some("10.0.0.5"))];
        let rule = make_rule(
            "masq",
            NatRuleType::Masquerade,
            &[("zone", "internal")],
            true,
        );

        let result = compiler.compile_masquerade(&rule, &vms);
        assert!(result.is_none());
    }

    #[test]
    fn test_compile_all() {
        let compiler = NatCompiler::new();
        let vms = vec![make_vm("web-1", &[("zone", "internal")], Some("10.0.0.5"))];
        let rules = vec![make_rule(
            "masq",
            NatRuleType::Masquerade,
            &[("zone", "internal")],
            true,
        )];
        let gateways = vec![NatGatewayConfig {
            id: Uuid::new_v4(),
            name: "gw".to_string(),
            description: String::new(),
            subnet: "10.0.0.0/24".to_string(),
            outbound_interface: "eth0".to_string(),
            selector: LabelSelector::default(),
            enabled: true,
            created: Utc::now(),
            updated: Utc::now(),
        }];

        let result = compiler.compile_all(&rules, &gateways, &[], &vms);
        assert_eq!(result.len(), 2);
    }
}
