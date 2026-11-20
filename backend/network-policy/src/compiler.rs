// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: LicenseRef-Zyvor-Production-1.0

use std::collections::{HashMap, HashSet};

use crate::identity::IdentityAllocator;
use crate::models::*;

/// Compiles network policies into resolved rules by matching label selectors
/// against actual VM labels and resolving them to identity IDs.
pub struct PolicyCompiler {
    allocator: IdentityAllocator,
}

/// A snapshot of a VM's relevant state for policy compilation.
pub struct VMSnapshot {
    pub name: String,
    pub labels: HashMap<String, String>,
    pub ip: Option<String>,
}

impl PolicyCompiler {
    pub fn new(allocator: IdentityAllocator) -> Self {
        Self { allocator }
    }

    /// Compile a single policy against the current VM state.
    pub fn compile_policy(
        &self,
        policy: &NetworkPolicy,
        all_vms: &[VMSnapshot],
    ) -> Vec<CompiledRule> {
        if !policy.enabled {
            return vec![];
        }

        let mut rules = Vec::new();

        // Find VMs that match the endpoint_selector (the "subject" VMs)
        let subject_vms: Vec<&VMSnapshot> = all_vms
            .iter()
            .filter(|vm| policy.endpoint_selector.matches(&vm.labels))
            .collect();

        if subject_vms.is_empty() {
            return vec![];
        }

        // Get or allocate identity IDs for subject VMs
        let mut subject_identity_ids = HashSet::new();
        for vm in &subject_vms {
            if let Ok(id) = self.allocator.allocate_or_get(&vm.labels, &vm.name) {
                subject_identity_ids.insert(id);
                if let Some(ref ip) = vm.ip {
                    let _ = self.allocator.update_ip_mapping(ip, id);
                }
            }
        }

        // Compile ingress rules
        for ingress_rule in &policy.ingress {
            let peer_ids = self.resolve_peers(&ingress_rule.from, all_vms);

            for &dst_id in &subject_identity_ids {
                for &src_id in &peer_ids {
                    if ingress_rule.to_ports.is_empty() {
                        // No port restriction → allow all
                        rules.push(CompiledRule {
                            direction: Direction::Ingress,
                            src_identity: src_id,
                            dst_identity: dst_id,
                            protocol: PolicyProtocol::Any,
                            port: 0,
                            end_port: None,
                            policy_name: policy.name.clone(),
                        });
                    } else {
                        for port_rule in &ingress_rule.to_ports {
                            rules.push(CompiledRule {
                                direction: Direction::Ingress,
                                src_identity: src_id,
                                dst_identity: dst_id,
                                protocol: port_rule.protocol.clone(),
                                port: port_rule.port,
                                end_port: port_rule.end_port,
                                policy_name: policy.name.clone(),
                            });
                        }
                    }
                }
            }
        }

        // Compile egress rules
        for egress_rule in &policy.egress {
            let peer_ids = self.resolve_peers(&egress_rule.to, all_vms);

            for &src_id in &subject_identity_ids {
                for &dst_id in &peer_ids {
                    if egress_rule.to_ports.is_empty() {
                        rules.push(CompiledRule {
                            direction: Direction::Egress,
                            src_identity: src_id,
                            dst_identity: dst_id,
                            protocol: PolicyProtocol::Any,
                            port: 0,
                            end_port: None,
                            policy_name: policy.name.clone(),
                        });
                    } else {
                        for port_rule in &egress_rule.to_ports {
                            rules.push(CompiledRule {
                                direction: Direction::Egress,
                                src_identity: src_id,
                                dst_identity: dst_id,
                                protocol: port_rule.protocol.clone(),
                                port: port_rule.port,
                                end_port: port_rule.end_port,
                                policy_name: policy.name.clone(),
                            });
                        }
                    }
                }
            }
        }

        rules
    }

    /// Compile all policies and deduplicate rules.
    pub fn compile_all(
        &self,
        policies: &[NetworkPolicy],
        all_vms: &[VMSnapshot],
    ) -> Vec<CompiledRule> {
        let mut seen = HashSet::new();
        let mut rules = Vec::new();

        for policy in policies {
            for rule in self.compile_policy(policy, all_vms) {
                if seen.insert(rule.clone()) {
                    rules.push(rule);
                }
            }
        }

        rules
    }

    /// Resolve peer selectors to identity IDs.
    fn resolve_peers(&self, peers: &[PeerSelector], all_vms: &[VMSnapshot]) -> HashSet<u32> {
        let mut ids = HashSet::new();

        if peers.is_empty() {
            // Empty peer list means "all" — for simplicity, match all known identities
            for vm in all_vms {
                if let Ok(id) = self.allocator.allocate_or_get(&vm.labels, &vm.name) {
                    ids.insert(id);
                }
            }
            return ids;
        }

        for peer in peers {
            match peer {
                PeerSelector::Endpoint(selector) => {
                    for vm in all_vms {
                        if selector.matches(&vm.labels) {
                            if let Ok(id) = self.allocator.allocate_or_get(&vm.labels, &vm.name) {
                                ids.insert(id);
                            }
                        }
                    }
                }
                PeerSelector::Cidr(_) => {
                    ids.insert(IDENTITY_WORLD);
                }
            }
        }

        ids
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

    fn make_policy(
        name: &str,
        selector: &[(&str, &str)],
        ingress: Vec<IngressRule>,
        egress: Vec<EgressRule>,
    ) -> NetworkPolicy {
        NetworkPolicy {
            id: Uuid::new_v4(),
            name: name.to_string(),
            description: String::new(),
            endpoint_selector: LabelSelector {
                match_labels: selector
                    .iter()
                    .map(|(k, v)| (k.to_string(), v.to_string()))
                    .collect(),
            },
            ingress,
            egress,
            enabled: true,
            managed: true,
            created: Utc::now(),
            updated: Utc::now(),
        }
    }

    fn label_selector(pairs: &[(&str, &str)]) -> LabelSelector {
        LabelSelector {
            match_labels: pairs
                .iter()
                .map(|(k, v)| (k.to_string(), v.to_string()))
                .collect(),
        }
    }

    #[test]
    fn test_simple_ingress() {
        let alloc = IdentityAllocator::new();
        let compiler = PolicyCompiler::new(alloc);

        let vms = vec![
            make_vm("web-1", &[("app", "web")], Some("10.0.0.5")),
            make_vm("api-1", &[("app", "api")], Some("10.0.0.10")),
        ];

        let policy = make_policy(
            "allow-api-to-web",
            &[("app", "web")],
            vec![IngressRule {
                from: vec![PeerSelector::Endpoint(label_selector(&[("app", "api")]))],
                to_ports: vec![PortRule {
                    protocol: PolicyProtocol::Tcp,
                    port: 80,
                    end_port: None,
                }],
            }],
            vec![],
        );

        let rules = compiler.compile_policy(&policy, &vms);
        assert_eq!(rules.len(), 1);
        assert_eq!(rules[0].direction, Direction::Ingress);
        assert_eq!(rules[0].port, 80);
        assert_eq!(rules[0].protocol, PolicyProtocol::Tcp);
    }

    #[test]
    fn test_egress() {
        let alloc = IdentityAllocator::new();
        let compiler = PolicyCompiler::new(alloc);

        let vms = vec![
            make_vm("web-1", &[("app", "web")], Some("10.0.0.5")),
            make_vm("db-1", &[("app", "db")], Some("10.0.0.20")),
        ];

        let policy = make_policy(
            "allow-web-to-db",
            &[("app", "web")],
            vec![],
            vec![EgressRule {
                to: vec![PeerSelector::Endpoint(label_selector(&[("app", "db")]))],
                to_ports: vec![PortRule {
                    protocol: PolicyProtocol::Tcp,
                    port: 5432,
                    end_port: None,
                }],
            }],
        );

        let rules = compiler.compile_policy(&policy, &vms);
        assert_eq!(rules.len(), 1);
        assert_eq!(rules[0].direction, Direction::Egress);
        assert_eq!(rules[0].port, 5432);
    }

    #[test]
    fn test_no_matches() {
        let alloc = IdentityAllocator::new();
        let compiler = PolicyCompiler::new(alloc);

        let vms = vec![make_vm("web-1", &[("app", "web")], Some("10.0.0.5"))];

        let policy = make_policy(
            "no-match",
            &[("app", "nonexistent")],
            vec![IngressRule {
                from: vec![],
                to_ports: vec![],
            }],
            vec![],
        );

        let rules = compiler.compile_policy(&policy, &vms);
        assert!(rules.is_empty());
    }

    #[test]
    fn test_cidr_peer() {
        let alloc = IdentityAllocator::new();
        let compiler = PolicyCompiler::new(alloc);

        let vms = vec![make_vm("web-1", &[("app", "web")], Some("10.0.0.5"))];

        let policy = make_policy(
            "allow-external",
            &[("app", "web")],
            vec![IngressRule {
                from: vec![PeerSelector::Cidr("0.0.0.0/0".to_string())],
                to_ports: vec![PortRule {
                    protocol: PolicyProtocol::Tcp,
                    port: 443,
                    end_port: None,
                }],
            }],
            vec![],
        );

        let rules = compiler.compile_policy(&policy, &vms);
        assert_eq!(rules.len(), 1);
        assert_eq!(rules[0].src_identity, IDENTITY_WORLD);
    }

    #[test]
    fn test_multiple_ports() {
        let alloc = IdentityAllocator::new();
        let compiler = PolicyCompiler::new(alloc);

        let vms = vec![
            make_vm("web-1", &[("app", "web")], Some("10.0.0.5")),
            make_vm("client-1", &[("app", "client")], Some("10.0.0.30")),
        ];

        let policy = make_policy(
            "multi-port",
            &[("app", "web")],
            vec![IngressRule {
                from: vec![PeerSelector::Endpoint(label_selector(&[("app", "client")]))],
                to_ports: vec![
                    PortRule {
                        protocol: PolicyProtocol::Tcp,
                        port: 80,
                        end_port: None,
                    },
                    PortRule {
                        protocol: PolicyProtocol::Tcp,
                        port: 443,
                        end_port: None,
                    },
                ],
            }],
            vec![],
        );

        let rules = compiler.compile_policy(&policy, &vms);
        assert_eq!(rules.len(), 2);
    }

    #[test]
    fn test_dedup() {
        let alloc = IdentityAllocator::new();
        let compiler = PolicyCompiler::new(alloc);

        let vms = vec![
            make_vm("web-1", &[("app", "web")], Some("10.0.0.5")),
            make_vm("api-1", &[("app", "api")], Some("10.0.0.10")),
        ];

        let ingress = vec![IngressRule {
            from: vec![PeerSelector::Endpoint(label_selector(&[("app", "api")]))],
            to_ports: vec![PortRule {
                protocol: PolicyProtocol::Tcp,
                port: 80,
                end_port: None,
            }],
        }];

        let policy1 = make_policy("policy-a", &[("app", "web")], ingress.clone(), vec![]);
        let policy2 = make_policy("policy-a", &[("app", "web")], ingress, vec![]);

        let rules = compiler.compile_all(&[policy1, policy2], &vms);
        // Same name + same rule = deduplicated
        assert_eq!(rules.len(), 1);
    }

    #[test]
    fn test_port_range() {
        let alloc = IdentityAllocator::new();
        let compiler = PolicyCompiler::new(alloc);

        let vms = vec![
            make_vm("web-1", &[("app", "web")], Some("10.0.0.5")),
            make_vm("api-1", &[("app", "api")], Some("10.0.0.10")),
        ];

        let policy = make_policy(
            "port-range",
            &[("app", "web")],
            vec![IngressRule {
                from: vec![PeerSelector::Endpoint(label_selector(&[("app", "api")]))],
                to_ports: vec![PortRule {
                    protocol: PolicyProtocol::Tcp,
                    port: 8000,
                    end_port: Some(8100),
                }],
            }],
            vec![],
        );

        let rules = compiler.compile_policy(&policy, &vms);
        assert_eq!(rules.len(), 1);
        assert_eq!(rules[0].port, 8000);
        assert_eq!(rules[0].end_port, Some(8100));
    }

    #[test]
    fn test_any_protocol() {
        let alloc = IdentityAllocator::new();
        let compiler = PolicyCompiler::new(alloc);

        let vms = vec![
            make_vm("web-1", &[("app", "web")], Some("10.0.0.5")),
            make_vm("api-1", &[("app", "api")], Some("10.0.0.10")),
        ];

        let policy = make_policy(
            "any-proto",
            &[("app", "web")],
            vec![IngressRule {
                from: vec![PeerSelector::Endpoint(label_selector(&[("app", "api")]))],
                to_ports: vec![PortRule {
                    protocol: PolicyProtocol::Any,
                    port: 53,
                    end_port: None,
                }],
            }],
            vec![],
        );

        let rules = compiler.compile_policy(&policy, &vms);
        assert_eq!(rules.len(), 1);
        assert_eq!(rules[0].protocol, PolicyProtocol::Any);
    }
}
