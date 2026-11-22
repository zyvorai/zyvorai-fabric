// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: LicenseRef-Zyvor-Production-2.0

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap};
use uuid::Uuid;

/// Reserved identity for external/world traffic
pub const IDENTITY_WORLD: u32 = 0;

/// First user-assignable identity ID (0-255 reserved)
pub const IDENTITY_USER_MIN: u32 = 256;

/// A security identity groups endpoints with the same labels under a single numeric ID.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SecurityIdentity {
    pub id: u32,
    pub labels: BTreeMap<String, String>,
    pub endpoints: Vec<String>,
    #[serde(default)]
    pub description: String,
    #[serde(default = "default_true")]
    pub managed: bool,
    pub created: DateTime<Utc>,
    pub updated: DateTime<Utc>,
}

/// Selects endpoints by matching labels (AND semantics).
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct LabelSelector {
    #[serde(default)]
    pub match_labels: HashMap<String, String>,
}

impl LabelSelector {
    /// Returns true if all match_labels are present in the given labels with matching values.
    /// An empty selector matches everything.
    pub fn matches(&self, labels: &HashMap<String, String>) -> bool {
        self.match_labels
            .iter()
            .all(|(k, v)| labels.get(k) == Some(v))
    }
}

/// A network policy that controls traffic between identity-grouped endpoints.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetworkPolicy {
    pub id: Uuid,
    pub name: String,
    #[serde(default)]
    pub description: String,
    pub endpoint_selector: LabelSelector,
    #[serde(default)]
    pub ingress: Vec<IngressRule>,
    #[serde(default)]
    pub egress: Vec<EgressRule>,
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default = "default_true")]
    pub managed: bool,
    pub created: DateTime<Utc>,
    pub updated: DateTime<Utc>,
}

fn default_true() -> bool {
    true
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IngressRule {
    #[serde(default)]
    pub from: Vec<PeerSelector>,
    #[serde(default)]
    pub to_ports: Vec<PortRule>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EgressRule {
    #[serde(default)]
    pub to: Vec<PeerSelector>,
    #[serde(default)]
    pub to_ports: Vec<PortRule>,
}

/// Identifies a traffic peer — either by endpoint labels or by CIDR.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PeerSelector {
    Endpoint(LabelSelector),
    Cidr(#[serde(deserialize_with = "deserialize_validated_cidr")] String),
}

/// Validate CIDR notation on deserialization.
fn deserialize_validated_cidr<'de, D>(deserializer: D) -> std::result::Result<String, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let s = String::deserialize(deserializer)?;
    validate_cidr(&s).map_err(serde::de::Error::custom)?;
    Ok(s)
}

/// Validate that a string is a valid CIDR (e.g. "10.0.0.0/24" or "::1/128").
pub fn validate_cidr(s: &str) -> std::result::Result<(), String> {
    let parts: Vec<&str> = s.splitn(2, '/').collect();
    if parts.len() != 2 {
        return Err(format!("Invalid CIDR '{}': missing /prefix", s));
    }
    parts[0]
        .parse::<std::net::IpAddr>()
        .map_err(|_| format!("Invalid CIDR '{}': bad IP address", s))?;
    let prefix: u8 = parts[1]
        .parse()
        .map_err(|_| format!("Invalid CIDR '{}': bad prefix length", s))?;
    let max_prefix = if parts[0].contains(':') { 128 } else { 32 };
    if prefix > max_prefix {
        return Err(format!(
            "Invalid CIDR '{}': prefix length {} exceeds maximum {}",
            s, prefix, max_prefix
        ));
    }
    Ok(())
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PortRule {
    #[serde(default)]
    pub protocol: PolicyProtocol,
    pub port: u16,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub end_port: Option<u16>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash, Default)]
#[serde(rename_all = "lowercase")]
pub enum PolicyProtocol {
    #[default]
    Tcp,
    Udp,
    Any,
}

/// API request to create a network policy.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateNetworkPolicyRequest {
    pub name: String,
    #[serde(default)]
    pub description: String,
    pub endpoint_selector: LabelSelector,
    #[serde(default)]
    pub ingress: Vec<IngressRule>,
    #[serde(default)]
    pub egress: Vec<EgressRule>,
    #[serde(default = "default_true")]
    pub enabled: bool,
}

/// A compiled (resolved) rule ready for nftables enforcement.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub struct CompiledRule {
    pub direction: Direction,
    pub src_identity: u32,
    pub dst_identity: u32,
    pub protocol: PolicyProtocol,
    pub port: u16,
    pub end_port: Option<u16>,
    pub policy_name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "lowercase")]
pub enum Direction {
    Ingress,
    Egress,
}

/// Status report for a single network policy.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PolicyStatus {
    pub policy_id: Uuid,
    pub policy_name: String,
    pub matching_endpoints: usize,
    pub compiled_rules_count: usize,
    pub enforced: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_synced: Option<DateTime<Utc>>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_label_selector_and_matching() {
        let mut selector = LabelSelector::default();
        selector
            .match_labels
            .insert("app".to_string(), "web".to_string());
        selector
            .match_labels
            .insert("env".to_string(), "prod".to_string());

        let mut labels = HashMap::new();
        labels.insert("app".to_string(), "web".to_string());
        labels.insert("env".to_string(), "prod".to_string());
        labels.insert("version".to_string(), "v2".to_string());

        assert!(selector.matches(&labels));
    }

    #[test]
    fn test_empty_selector_matches_all() {
        let selector = LabelSelector::default();
        let mut labels = HashMap::new();
        labels.insert("anything".to_string(), "value".to_string());
        assert!(selector.matches(&labels));
        assert!(selector.matches(&HashMap::new()));
    }

    #[test]
    fn test_missing_key_no_match() {
        let mut selector = LabelSelector::default();
        selector
            .match_labels
            .insert("app".to_string(), "web".to_string());

        let labels = HashMap::new();
        assert!(!selector.matches(&labels));
    }

    #[test]
    fn test_wrong_value_no_match() {
        let mut selector = LabelSelector::default();
        selector
            .match_labels
            .insert("app".to_string(), "web".to_string());

        let mut labels = HashMap::new();
        labels.insert("app".to_string(), "db".to_string());
        assert!(!selector.matches(&labels));
    }

    #[test]
    fn test_network_policy_serde_roundtrip() {
        let policy = NetworkPolicy {
            id: Uuid::new_v4(),
            name: "allow-web".to_string(),
            description: "Allow web traffic".to_string(),
            endpoint_selector: LabelSelector::default(),
            ingress: vec![IngressRule {
                from: vec![PeerSelector::Endpoint(LabelSelector::default())],
                to_ports: vec![PortRule {
                    protocol: PolicyProtocol::Tcp,
                    port: 80,
                    end_port: None,
                }],
            }],
            egress: vec![],
            enabled: true,
            managed: true,
            created: Utc::now(),
            updated: Utc::now(),
        };

        let json = serde_json::to_string(&policy).unwrap();
        let deserialized: NetworkPolicy = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.name, "allow-web");
        assert_eq!(deserialized.ingress.len(), 1);
    }

    #[test]
    fn test_protocol_defaults_to_tcp() {
        let rule: PortRule = serde_json::from_str(r#"{"port": 443}"#).unwrap();
        assert_eq!(rule.protocol, PolicyProtocol::Tcp);
        assert_eq!(rule.port, 443);
    }
}
