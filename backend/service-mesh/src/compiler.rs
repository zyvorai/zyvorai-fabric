// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: LicenseRef-Zyvor-Production-1.0

use std::collections::HashMap;

use crate::health::HealthChecker;
use crate::models::*;

/// A snapshot of a VM's state for service compilation.
pub struct VMSnapshot {
    pub name: String,
    pub labels: HashMap<String, String>,
    pub ip: Option<String>,
}

/// Compiles services into DNAT rules by resolving label selectors against VMs
/// and filtering to healthy backends.
pub struct ServiceCompiler {
    health_checker: HealthChecker,
}

impl ServiceCompiler {
    pub fn new(health_checker: HealthChecker) -> Self {
        Self { health_checker }
    }

    /// Access the underlying health checker.
    pub fn health_checker(&self) -> &HealthChecker {
        &self.health_checker
    }

    /// Compile a single service into DNAT rules.
    pub async fn compile_service(
        &self,
        service: &Service,
        all_vms: &[VMSnapshot],
    ) -> Vec<CompiledDnatRule> {
        if !service.enabled {
            return vec![];
        }

        // Resolve selector → matching VMs
        let matching_vms: Vec<&VMSnapshot> = all_vms
            .iter()
            .filter(|vm| service.selector.matches(&vm.labels) && vm.ip.is_some())
            .collect();

        if matching_vms.is_empty() {
            return vec![];
        }

        // Update backends in health checker
        let new_backends: Vec<Backend> = matching_vms
            .iter()
            .map(|vm| Backend {
                vm_name: vm.name.clone(),
                ip: vm.ip.clone().unwrap_or_default(),
                health: BackendHealth::Unknown,
                consecutive_successes: 0,
                consecutive_failures: 0,
                last_check: None,
            })
            .collect();

        // Merge with existing backends to preserve health state
        let existing = self.health_checker.get_all_backends(&service.name).await;
        let merged: Vec<Backend> = new_backends
            .into_iter()
            .map(|new| {
                if let Some(existing_backend) = existing.iter().find(|e| e.ip == new.ip) {
                    existing_backend.clone()
                } else {
                    new
                }
            })
            .collect();

        self.health_checker
            .update_backends(&service.name, merged)
            .await;

        // Get healthy backends for DNAT rules
        let healthy = self
            .health_checker
            .get_healthy_backends(&service.name)
            .await;

        // If no healthy backends, use all backends (graceful degradation)
        let backend_ips: Vec<String> = if healthy.is_empty() {
            let all = self.health_checker.get_all_backends(&service.name).await;
            all.into_iter().map(|b| b.ip).collect()
        } else {
            healthy.into_iter().map(|b| b.ip).collect()
        };

        if backend_ips.is_empty() {
            return vec![];
        }

        // Generate DNAT rules for each service port
        service
            .ports
            .iter()
            .map(|sp| CompiledDnatRule {
                virtual_ip: service.virtual_ip.clone(),
                port: sp.port,
                target_port: sp.effective_target_port(),
                protocol: sp.protocol.clone(),
                backend_ips: backend_ips.clone(),
                algorithm: service.algorithm.clone(),
            })
            .collect()
    }

    /// Compile all services into DNAT rules.
    pub async fn compile_all(
        &self,
        services: &[Service],
        all_vms: &[VMSnapshot],
    ) -> Vec<CompiledDnatRule> {
        let mut rules = Vec::new();
        for service in services {
            rules.extend(self.compile_service(service, all_vms).await);
        }
        rules
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

    fn make_service(
        name: &str,
        vip: &str,
        selector: &[(&str, &str)],
        ports: Vec<ServicePort>,
        algorithm: LoadBalancerAlgorithm,
    ) -> Service {
        Service {
            id: Uuid::new_v4(),
            name: name.to_string(),
            description: String::new(),
            virtual_ip: vip.to_string(),
            selector: LabelSelector {
                match_labels: selector
                    .iter()
                    .map(|(k, v)| (k.to_string(), v.to_string()))
                    .collect(),
            },
            ports,
            algorithm,
            health_check: HealthCheck::default(),
            enabled: true,
            managed: true,
            created: Utc::now(),
            updated: Utc::now(),
        }
    }

    #[tokio::test]
    async fn test_basic_dnat() {
        let checker = HealthChecker::new();
        let compiler = ServiceCompiler::new(checker);

        let vms = vec![
            make_vm("web-1", &[("app", "web")], Some("10.0.0.5")),
            make_vm("web-2", &[("app", "web")], Some("10.0.0.6")),
        ];

        let service = make_service(
            "web-svc",
            "10.0.0.100",
            &[("app", "web")],
            vec![ServicePort {
                port: 80,
                target_port: Some(8080),
                protocol: ServiceProtocol::Tcp,
            }],
            LoadBalancerAlgorithm::RoundRobin,
        );

        let rules = compiler.compile_service(&service, &vms).await;
        assert_eq!(rules.len(), 1);
        assert_eq!(rules[0].virtual_ip, "10.0.0.100");
        assert_eq!(rules[0].port, 80);
        assert_eq!(rules[0].target_port, 8080);
        assert_eq!(rules[0].backend_ips.len(), 2);
    }

    #[tokio::test]
    async fn test_no_backends() {
        let checker = HealthChecker::new();
        let compiler = ServiceCompiler::new(checker);

        let vms = vec![make_vm("db-1", &[("app", "db")], Some("10.0.0.20"))];

        let service = make_service(
            "web-svc",
            "10.0.0.100",
            &[("app", "web")],
            vec![ServicePort {
                port: 80,
                target_port: None,
                protocol: ServiceProtocol::Tcp,
            }],
            LoadBalancerAlgorithm::RoundRobin,
        );

        let rules = compiler.compile_service(&service, &vms).await;
        assert!(rules.is_empty());
    }

    #[tokio::test]
    async fn test_multiple_ports() {
        let checker = HealthChecker::new();
        let compiler = ServiceCompiler::new(checker);

        let vms = vec![make_vm("web-1", &[("app", "web")], Some("10.0.0.5"))];

        let service = make_service(
            "web-svc",
            "10.0.0.100",
            &[("app", "web")],
            vec![
                ServicePort {
                    port: 80,
                    target_port: None,
                    protocol: ServiceProtocol::Tcp,
                },
                ServicePort {
                    port: 443,
                    target_port: None,
                    protocol: ServiceProtocol::Tcp,
                },
            ],
            LoadBalancerAlgorithm::RoundRobin,
        );

        let rules = compiler.compile_service(&service, &vms).await;
        assert_eq!(rules.len(), 2);
        assert_eq!(rules[0].port, 80);
        assert_eq!(rules[1].port, 443);
    }

    #[tokio::test]
    async fn test_unhealthy_filtered() {
        let checker = HealthChecker::new();

        // Pre-populate with one healthy and one unhealthy backend
        let healthy = Backend {
            vm_name: "web-1".to_string(),
            ip: "10.0.0.5".to_string(),
            health: BackendHealth::Healthy,
            consecutive_successes: 3,
            consecutive_failures: 0,
            last_check: None,
        };
        let unhealthy = Backend {
            vm_name: "web-2".to_string(),
            ip: "10.0.0.6".to_string(),
            health: BackendHealth::Unhealthy,
            consecutive_successes: 0,
            consecutive_failures: 3,
            last_check: None,
        };
        checker
            .update_backends("web-svc", vec![healthy, unhealthy])
            .await;

        let compiler = ServiceCompiler::new(checker);

        let vms = vec![
            make_vm("web-1", &[("app", "web")], Some("10.0.0.5")),
            make_vm("web-2", &[("app", "web")], Some("10.0.0.6")),
        ];

        let service = make_service(
            "web-svc",
            "10.0.0.100",
            &[("app", "web")],
            vec![ServicePort {
                port: 80,
                target_port: None,
                protocol: ServiceProtocol::Tcp,
            }],
            LoadBalancerAlgorithm::RoundRobin,
        );

        let rules = compiler.compile_service(&service, &vms).await;
        assert_eq!(rules.len(), 1);
        // Only healthy backend should be included
        assert_eq!(rules[0].backend_ips.len(), 1);
        assert_eq!(rules[0].backend_ips[0], "10.0.0.5");
    }

    #[tokio::test]
    async fn test_round_robin() {
        let checker = HealthChecker::new();
        let compiler = ServiceCompiler::new(checker);

        let vms = vec![make_vm("web-1", &[("app", "web")], Some("10.0.0.5"))];

        let service = make_service(
            "web-svc",
            "10.0.0.100",
            &[("app", "web")],
            vec![ServicePort {
                port: 80,
                target_port: None,
                protocol: ServiceProtocol::Tcp,
            }],
            LoadBalancerAlgorithm::RoundRobin,
        );

        let rules = compiler.compile_service(&service, &vms).await;
        assert_eq!(rules[0].algorithm, LoadBalancerAlgorithm::RoundRobin);
    }

    #[tokio::test]
    async fn test_ip_hash() {
        let checker = HealthChecker::new();
        let compiler = ServiceCompiler::new(checker);

        let vms = vec![make_vm("web-1", &[("app", "web")], Some("10.0.0.5"))];

        let service = make_service(
            "web-svc",
            "10.0.0.100",
            &[("app", "web")],
            vec![ServicePort {
                port: 80,
                target_port: None,
                protocol: ServiceProtocol::Tcp,
            }],
            LoadBalancerAlgorithm::IpHash,
        );

        let rules = compiler.compile_service(&service, &vms).await;
        assert_eq!(rules[0].algorithm, LoadBalancerAlgorithm::IpHash);
    }

    #[tokio::test]
    async fn test_empty_selector() {
        let checker = HealthChecker::new();
        let compiler = ServiceCompiler::new(checker);

        let vms = vec![
            make_vm("web-1", &[("app", "web")], Some("10.0.0.5")),
            make_vm("db-1", &[("app", "db")], Some("10.0.0.20")),
        ];

        // Empty selector matches all VMs
        let service = make_service(
            "all-svc",
            "10.0.0.100",
            &[],
            vec![ServicePort {
                port: 80,
                target_port: None,
                protocol: ServiceProtocol::Tcp,
            }],
            LoadBalancerAlgorithm::RoundRobin,
        );

        let rules = compiler.compile_service(&service, &vms).await;
        assert_eq!(rules.len(), 1);
        assert_eq!(rules[0].backend_ips.len(), 2);
    }

    #[tokio::test]
    async fn test_disabled_service() {
        let checker = HealthChecker::new();
        let compiler = ServiceCompiler::new(checker);

        let vms = vec![make_vm("web-1", &[("app", "web")], Some("10.0.0.5"))];

        let mut service = make_service(
            "web-svc",
            "10.0.0.100",
            &[("app", "web")],
            vec![ServicePort {
                port: 80,
                target_port: None,
                protocol: ServiceProtocol::Tcp,
            }],
            LoadBalancerAlgorithm::RoundRobin,
        );
        service.enabled = false;

        let rules = compiler.compile_service(&service, &vms).await;
        assert!(rules.is_empty());
    }
}
