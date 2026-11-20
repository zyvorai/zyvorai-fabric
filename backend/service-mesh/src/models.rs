// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: LicenseRef-Zyvor-Production-1.0

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use uuid::Uuid;

/// Load balancing algorithm for distributing traffic across backends.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum LoadBalancerAlgorithm {
    #[default]
    RoundRobin,
    Random,
    IpHash,
}

/// Type of health check to perform against backends.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum HealthCheckType {
    #[default]
    Tcp,
    Http,
}

/// Health check configuration for a service's backends.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthCheck {
    #[serde(default)]
    pub check_type: HealthCheckType,
    #[serde(default)]
    pub http_path: String,
    #[serde(default)]
    pub expected_status_codes: Vec<u16>,
    #[serde(default = "default_interval")]
    pub interval_secs: u64,
    #[serde(default = "default_timeout")]
    pub timeout_secs: u64,
    #[serde(default = "default_threshold")]
    pub unhealthy_threshold: u32,
    #[serde(default = "default_threshold")]
    pub healthy_threshold: u32,
}

fn default_interval() -> u64 {
    10
}

fn default_timeout() -> u64 {
    5
}

fn default_threshold() -> u32 {
    3
}

impl Default for HealthCheck {
    fn default() -> Self {
        Self {
            check_type: HealthCheckType::Tcp,
            http_path: String::new(),
            expected_status_codes: vec![200],
            interval_secs: default_interval(),
            timeout_secs: default_timeout(),
            unhealthy_threshold: default_threshold(),
            healthy_threshold: default_threshold(),
        }
    }
}

/// Selects endpoints by matching labels (AND semantics).
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct LabelSelector {
    #[serde(default)]
    pub match_labels: HashMap<String, String>,
}

impl LabelSelector {
    /// Returns true if all match_labels are present with matching values.
    /// An empty selector matches everything.
    pub fn matches(&self, labels: &HashMap<String, String>) -> bool {
        self.match_labels
            .iter()
            .all(|(k, v)| labels.get(k) == Some(v))
    }
}

/// Protocol for service ports.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum ServiceProtocol {
    #[default]
    Tcp,
    Udp,
}

/// A port mapping for a service.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServicePort {
    pub port: u16,
    #[serde(default)]
    pub target_port: Option<u16>,
    #[serde(default)]
    pub protocol: ServiceProtocol,
}

impl ServicePort {
    /// Returns the effective target port (defaults to port if not set).
    pub fn effective_target_port(&self) -> u16 {
        self.target_port.unwrap_or(self.port)
    }
}

fn default_true() -> bool {
    true
}

/// A service definition with virtual IP, label selector, and load balancing config.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Service {
    pub id: Uuid,
    pub name: String,
    #[serde(default)]
    pub description: String,
    pub virtual_ip: String,
    pub selector: LabelSelector,
    pub ports: Vec<ServicePort>,
    #[serde(default)]
    pub algorithm: LoadBalancerAlgorithm,
    #[serde(default)]
    pub health_check: HealthCheck,
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default = "default_true")]
    pub managed: bool,
    pub created: DateTime<Utc>,
    pub updated: DateTime<Utc>,
}

/// API request to create a service.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateServiceRequest {
    pub name: String,
    #[serde(default)]
    pub description: String,
    pub virtual_ip: String,
    pub selector: LabelSelector,
    pub ports: Vec<ServicePort>,
    #[serde(default)]
    pub algorithm: LoadBalancerAlgorithm,
    #[serde(default)]
    pub health_check: HealthCheck,
    #[serde(default = "default_true")]
    pub enabled: bool,
}

/// Health state of a backend VM.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum BackendHealth {
    Healthy,
    Unhealthy,
    #[default]
    Unknown,
}

/// A backend VM serving traffic for a service.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Backend {
    pub vm_name: String,
    pub ip: String,
    #[serde(default)]
    pub health: BackendHealth,
    #[serde(default)]
    pub consecutive_successes: u32,
    #[serde(default)]
    pub consecutive_failures: u32,
    #[serde(default)]
    pub last_check: Option<DateTime<Utc>>,
}

/// A compiled DNAT rule ready for nftables enforcement.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompiledDnatRule {
    pub virtual_ip: String,
    pub port: u16,
    pub target_port: u16,
    pub protocol: ServiceProtocol,
    pub backend_ips: Vec<String>,
    pub algorithm: LoadBalancerAlgorithm,
}

/// Status report for a single service.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServiceStatus {
    pub service_id: Uuid,
    pub name: String,
    pub healthy_backends: usize,
    pub total_backends: usize,
    pub active: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_label_matching() {
        let mut selector = LabelSelector::default();
        selector
            .match_labels
            .insert("app".to_string(), "web".to_string());

        let mut labels = HashMap::new();
        labels.insert("app".to_string(), "web".to_string());
        labels.insert("env".to_string(), "prod".to_string());
        assert!(selector.matches(&labels));

        let empty_labels = HashMap::new();
        assert!(!selector.matches(&empty_labels));

        let empty_selector = LabelSelector::default();
        assert!(empty_selector.matches(&labels));
        assert!(empty_selector.matches(&empty_labels));
    }

    #[test]
    fn test_default_algorithm() {
        let algo = LoadBalancerAlgorithm::default();
        assert_eq!(algo, LoadBalancerAlgorithm::RoundRobin);
    }

    #[test]
    fn test_health_check_defaults() {
        let hc = HealthCheck::default();
        assert_eq!(hc.check_type, HealthCheckType::Tcp);
        assert_eq!(hc.interval_secs, 10);
        assert_eq!(hc.timeout_secs, 5);
        assert_eq!(hc.unhealthy_threshold, 3);
        assert_eq!(hc.healthy_threshold, 3);
    }

    #[test]
    fn test_serde_roundtrip() {
        let service = Service {
            id: Uuid::new_v4(),
            name: "my-service".to_string(),
            description: "Test service".to_string(),
            virtual_ip: "10.0.0.100".to_string(),
            selector: LabelSelector::default(),
            ports: vec![ServicePort {
                port: 80,
                target_port: Some(8080),
                protocol: ServiceProtocol::Tcp,
            }],
            algorithm: LoadBalancerAlgorithm::RoundRobin,
            health_check: HealthCheck::default(),
            enabled: true,
            managed: true,
            created: Utc::now(),
            updated: Utc::now(),
        };

        let json = serde_json::to_string(&service).unwrap();
        let deserialized: Service = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.name, "my-service");
        assert_eq!(deserialized.ports.len(), 1);
    }

    #[test]
    fn test_service_port_target_port_default() {
        let port = ServicePort {
            port: 80,
            target_port: None,
            protocol: ServiceProtocol::Tcp,
        };
        assert_eq!(port.effective_target_port(), 80);

        let port_with_target = ServicePort {
            port: 80,
            target_port: Some(8080),
            protocol: ServiceProtocol::Tcp,
        };
        assert_eq!(port_with_target.effective_target_port(), 8080);
    }

    #[test]
    fn test_backend_health_states() {
        let health = BackendHealth::default();
        assert_eq!(health, BackendHealth::Unknown);

        let healthy: BackendHealth = serde_json::from_str(r#""healthy""#).unwrap();
        assert_eq!(healthy, BackendHealth::Healthy);

        let unhealthy: BackendHealth = serde_json::from_str(r#""unhealthy""#).unwrap();
        assert_eq!(unhealthy, BackendHealth::Unhealthy);
    }
}
