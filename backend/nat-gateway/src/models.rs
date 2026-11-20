// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: LicenseRef-Zyvor-Production-1.0

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use uuid::Uuid;

/// Type of NAT rule.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum NatRuleType {
    Masquerade,
    Snat,
    Dnat,
    Hairpin,
}

/// Protocol for NAT matching.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(rename_all = "lowercase")]
pub enum NatProtocol {
    Tcp,
    Udp,
    #[default]
    Any,
}

/// nftables chain type.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum NatChain {
    Prerouting,
    Postrouting,
}

/// Selects endpoints by matching labels (AND semantics).
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct LabelSelector {
    #[serde(default)]
    pub match_labels: HashMap<String, String>,
}

impl LabelSelector {
    pub fn matches(&self, labels: &HashMap<String, String>) -> bool {
        self.match_labels
            .iter()
            .all(|(k, v)| labels.get(k) == Some(v))
    }
}

fn default_true() -> bool {
    true
}

/// A pool of IP addresses for SNAT.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NatPool {
    pub id: Uuid,
    pub name: String,
    pub ip_ranges: Vec<String>,
    #[serde(default)]
    pub port_range: Option<String>,
    pub created: DateTime<Utc>,
    pub updated: DateTime<Utc>,
}

/// A NAT rule definition.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NatRule {
    pub id: Uuid,
    pub name: String,
    #[serde(default)]
    pub description: String,
    pub rule_type: NatRuleType,
    #[serde(default)]
    pub selector: LabelSelector,
    #[serde(default)]
    pub protocol: NatProtocol,
    #[serde(default)]
    pub source_cidr: Option<String>,
    #[serde(default)]
    pub dest_cidr: Option<String>,
    #[serde(default)]
    pub dest_port: Option<u16>,
    #[serde(default)]
    pub dest_port_end: Option<u16>,
    #[serde(default)]
    pub translate_to: Option<String>,
    #[serde(default)]
    pub translate_port: Option<u16>,
    #[serde(default)]
    pub pool_id: Option<Uuid>,
    #[serde(default)]
    pub outbound_interface: Option<String>,
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default = "default_true")]
    pub managed: bool,
    pub created: DateTime<Utc>,
    pub updated: DateTime<Utc>,
}

/// A NAT gateway configuration for a VM subnet.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NatGatewayConfig {
    pub id: Uuid,
    pub name: String,
    #[serde(default)]
    pub description: String,
    pub subnet: String,
    pub outbound_interface: String,
    #[serde(default)]
    pub selector: LabelSelector,
    #[serde(default = "default_true")]
    pub enabled: bool,
    pub created: DateTime<Utc>,
    pub updated: DateTime<Utc>,
}

/// API request to create a NAT rule.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateNatRuleRequest {
    pub name: String,
    #[serde(default)]
    pub description: String,
    pub rule_type: NatRuleType,
    #[serde(default)]
    pub selector: LabelSelector,
    #[serde(default)]
    pub protocol: NatProtocol,
    #[serde(default)]
    pub source_cidr: Option<String>,
    #[serde(default)]
    pub dest_cidr: Option<String>,
    #[serde(default)]
    pub dest_port: Option<u16>,
    #[serde(default)]
    pub dest_port_end: Option<u16>,
    #[serde(default)]
    pub translate_to: Option<String>,
    #[serde(default)]
    pub translate_port: Option<u16>,
    #[serde(default)]
    pub pool_id: Option<Uuid>,
    #[serde(default)]
    pub outbound_interface: Option<String>,
    #[serde(default = "default_true")]
    pub enabled: bool,
}

/// API request to create a NAT pool.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateNatPoolRequest {
    pub name: String,
    pub ip_ranges: Vec<String>,
    #[serde(default)]
    pub port_range: Option<String>,
}

/// API request to create a NAT gateway.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateNatGatewayRequest {
    pub name: String,
    #[serde(default)]
    pub description: String,
    pub subnet: String,
    pub outbound_interface: String,
    #[serde(default)]
    pub selector: LabelSelector,
    #[serde(default = "default_true")]
    pub enabled: bool,
}

/// A compiled NAT rule ready for nftables enforcement.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompiledNatRule {
    pub rule_type: NatRuleType,
    pub chain: NatChain,
    pub source_match: Option<String>,
    pub dest_match: Option<String>,
    pub protocol: NatProtocol,
    pub dest_port: Option<u16>,
    pub dest_port_end: Option<u16>,
    pub action: String,
    pub outbound_interface: Option<String>,
    pub vm_ips: Vec<String>,
}

/// Status report for a NAT rule.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NatStatus {
    pub rule_id: Uuid,
    pub name: String,
    pub rule_type: NatRuleType,
    pub matching_vms: usize,
    pub enforced: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_nat_rule_type_serde() {
        let masq: NatRuleType = serde_json::from_str(r#""masquerade""#).unwrap();
        assert_eq!(masq, NatRuleType::Masquerade);

        let snat: NatRuleType = serde_json::from_str(r#""snat""#).unwrap();
        assert_eq!(snat, NatRuleType::Snat);

        let dnat: NatRuleType = serde_json::from_str(r#""dnat""#).unwrap();
        assert_eq!(dnat, NatRuleType::Dnat);

        let hairpin: NatRuleType = serde_json::from_str(r#""hairpin""#).unwrap();
        assert_eq!(hairpin, NatRuleType::Hairpin);
    }

    #[test]
    fn test_protocol_default() {
        let rule: NatRule = serde_json::from_str(
            r#"{"id":"00000000-0000-0000-0000-000000000000","name":"test","rule_type":"masquerade","enabled":true,"created":"2024-01-01T00:00:00Z","updated":"2024-01-01T00:00:00Z"}"#,
        ).unwrap();
        assert_eq!(rule.protocol, NatProtocol::Any);
    }

    #[test]
    fn test_label_matching() {
        let mut selector = LabelSelector::default();
        selector
            .match_labels
            .insert("zone".to_string(), "dmz".to_string());

        let mut labels = HashMap::new();
        labels.insert("zone".to_string(), "dmz".to_string());
        assert!(selector.matches(&labels));

        let empty = HashMap::new();
        assert!(!selector.matches(&empty));
    }

    #[test]
    fn test_pool_roundtrip() {
        let pool = NatPool {
            id: Uuid::new_v4(),
            name: "public-pool".to_string(),
            ip_ranges: vec!["203.0.113.10-203.0.113.20".to_string()],
            port_range: Some("1024-65535".to_string()),
            created: Utc::now(),
            updated: Utc::now(),
        };

        let json = serde_json::to_string(&pool).unwrap();
        let deserialized: NatPool = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.name, "public-pool");
        assert_eq!(deserialized.ip_ranges.len(), 1);
        assert!(deserialized.port_range.is_some());
    }

    #[test]
    fn test_rule_roundtrip() {
        let rule = NatRule {
            id: Uuid::new_v4(),
            name: "web-dnat".to_string(),
            description: "DNAT for web".to_string(),
            rule_type: NatRuleType::Dnat,
            selector: LabelSelector::default(),
            protocol: NatProtocol::Tcp,
            source_cidr: None,
            dest_cidr: Some("203.0.113.1".to_string()),
            dest_port: Some(80),
            dest_port_end: None,
            translate_to: Some("10.0.0.5".to_string()),
            translate_port: Some(8080),
            pool_id: None,
            outbound_interface: None,
            enabled: true,
            managed: true,
            created: Utc::now(),
            updated: Utc::now(),
        };

        let json = serde_json::to_string(&rule).unwrap();
        let deserialized: NatRule = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.name, "web-dnat");
        assert!(deserialized.managed);
        assert_eq!(deserialized.rule_type, NatRuleType::Dnat);
        assert_eq!(deserialized.protocol, NatProtocol::Tcp);
    }

    #[test]
    fn test_gateway_config_roundtrip() {
        let gw = NatGatewayConfig {
            id: Uuid::new_v4(),
            name: "default-gw".to_string(),
            description: "Default NAT gateway".to_string(),
            subnet: "10.0.0.0/24".to_string(),
            outbound_interface: "eth0".to_string(),
            selector: LabelSelector::default(),
            enabled: true,
            created: Utc::now(),
            updated: Utc::now(),
        };

        let json = serde_json::to_string(&gw).unwrap();
        let deserialized: NatGatewayConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.name, "default-gw");
        assert_eq!(deserialized.subnet, "10.0.0.0/24");
    }

    #[test]
    fn test_chain_variants() {
        let pre: NatChain = serde_json::from_str(r#""prerouting""#).unwrap();
        assert_eq!(pre, NatChain::Prerouting);

        let post: NatChain = serde_json::from_str(r#""postrouting""#).unwrap();
        assert_eq!(post, NatChain::Postrouting);
    }
}
