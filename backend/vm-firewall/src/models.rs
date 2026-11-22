// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: LicenseRef-Zyvor-Production-2.0

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Firewall action for a rule.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(rename_all = "lowercase")]
pub enum FirewallAction {
    #[default]
    Accept,
    Drop,
    Reject,
    Log,
}

/// Protocol for firewall rules.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(rename_all = "lowercase")]
pub enum FirewallProtocol {
    Tcp,
    Udp,
    Icmp,
    #[default]
    Any,
}

/// Time unit for rate limiting.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum RatePer {
    Second,
    Minute,
    Hour,
}

/// Rate limit configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RateLimit {
    pub rate: u32,
    pub per: RatePer,
}

impl RateLimit {
    /// Format for nftables, e.g. "5/minute".
    pub fn to_nft_string(&self) -> String {
        let unit = match self.per {
            RatePer::Second => "second",
            RatePer::Minute => "minute",
            RatePer::Hour => "hour",
        };
        format!("{}/{}", self.rate, unit)
    }
}

/// A single firewall rule within a profile.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FirewallRule {
    pub priority: u16,
    pub action: FirewallAction,
    #[serde(default)]
    pub protocol: FirewallProtocol,
    #[serde(default)]
    pub source_cidr: Option<String>,
    #[serde(default)]
    pub dest_port: Option<u16>,
    #[serde(default)]
    pub dest_port_end: Option<u16>,
    #[serde(default)]
    pub rate_limit: Option<RateLimit>,
    #[serde(default)]
    pub log_prefix: Option<String>,
    #[serde(default)]
    pub description: String,
}

fn default_drop() -> FirewallAction {
    FirewallAction::Drop
}

/// A firewall profile containing an ordered set of rules.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FirewallProfile {
    pub id: Uuid,
    pub name: String,
    #[serde(default)]
    pub description: String,
    #[serde(default = "default_drop")]
    pub default_action: FirewallAction,
    #[serde(default)]
    pub rules: Vec<FirewallRule>,
    #[serde(default = "default_true")]
    pub managed: bool,
    pub created: DateTime<Utc>,
    pub updated: DateTime<Utc>,
}

fn default_true() -> bool {
    true
}

/// A firewall zone (e.g. trusted, untrusted, dmz).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FirewallZone {
    pub id: Uuid,
    pub name: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub default_profile_id: Option<Uuid>,
    #[serde(default = "default_true")]
    pub managed: bool,
    pub created: DateTime<Utc>,
    pub updated: DateTime<Utc>,
}

/// Assignment of a firewall profile to a VM.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VMFirewallAssignment {
    pub vm_name: String,
    pub profile_id: Uuid,
    #[serde(default)]
    pub zone_id: Option<Uuid>,
}

/// API request to create a firewall profile.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateFirewallProfileRequest {
    pub name: String,
    #[serde(default)]
    pub description: String,
    #[serde(default = "default_drop")]
    pub default_action: FirewallAction,
    #[serde(default)]
    pub rules: Vec<FirewallRule>,
}

/// API request to create a firewall zone.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateFirewallZoneRequest {
    pub name: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub default_profile_id: Option<Uuid>,
}

/// API request to assign a firewall profile to a VM.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AssignFirewallRequest {
    pub profile_id: Uuid,
    #[serde(default)]
    pub zone_id: Option<Uuid>,
}

/// A compiled firewall chain ready for nftables enforcement.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompiledFirewallChain {
    pub vm_name: String,
    pub vm_ip: String,
    pub chain_name: String,
    pub default_action: FirewallAction,
    pub rules: Vec<FirewallRule>,
}

/// Status report for a firewall profile.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FirewallStatus {
    pub profile_id: Uuid,
    pub name: String,
    pub assigned_vms: usize,
    pub rules_count: usize,
    pub enforced: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rule_priority_ordering() {
        let mut rules = [
            FirewallRule {
                priority: 100,
                action: FirewallAction::Accept,
                protocol: FirewallProtocol::Tcp,
                source_cidr: None,
                dest_port: Some(80),
                dest_port_end: None,
                rate_limit: None,
                log_prefix: None,
                description: "HTTP".to_string(),
            },
            FirewallRule {
                priority: 10,
                action: FirewallAction::Accept,
                protocol: FirewallProtocol::Tcp,
                source_cidr: None,
                dest_port: Some(22),
                dest_port_end: None,
                rate_limit: None,
                log_prefix: None,
                description: "SSH".to_string(),
            },
        ];

        rules.sort_by_key(|r| r.priority);
        assert_eq!(rules[0].priority, 10);
        assert_eq!(rules[1].priority, 100);
    }

    #[test]
    fn test_action_serde() {
        let accept: FirewallAction = serde_json::from_str(r#""accept""#).unwrap();
        assert_eq!(accept, FirewallAction::Accept);

        let drop_action: FirewallAction = serde_json::from_str(r#""drop""#).unwrap();
        assert_eq!(drop_action, FirewallAction::Drop);

        let reject: FirewallAction = serde_json::from_str(r#""reject""#).unwrap();
        assert_eq!(reject, FirewallAction::Reject);

        let log: FirewallAction = serde_json::from_str(r#""log""#).unwrap();
        assert_eq!(log, FirewallAction::Log);
    }

    #[test]
    fn test_rate_limit_format() {
        let limit = RateLimit {
            rate: 5,
            per: RatePer::Minute,
        };
        assert_eq!(limit.to_nft_string(), "5/minute");

        let limit_sec = RateLimit {
            rate: 100,
            per: RatePer::Second,
        };
        assert_eq!(limit_sec.to_nft_string(), "100/second");

        let limit_hour = RateLimit {
            rate: 1000,
            per: RatePer::Hour,
        };
        assert_eq!(limit_hour.to_nft_string(), "1000/hour");
    }

    #[test]
    fn test_protocol_variants() {
        let tcp: FirewallProtocol = serde_json::from_str(r#""tcp""#).unwrap();
        assert_eq!(tcp, FirewallProtocol::Tcp);

        let udp: FirewallProtocol = serde_json::from_str(r#""udp""#).unwrap();
        assert_eq!(udp, FirewallProtocol::Udp);

        let icmp: FirewallProtocol = serde_json::from_str(r#""icmp""#).unwrap();
        assert_eq!(icmp, FirewallProtocol::Icmp);

        let any: FirewallProtocol = serde_json::from_str(r#""any""#).unwrap();
        assert_eq!(any, FirewallProtocol::Any);
    }

    #[test]
    fn test_default_action() {
        let profile: FirewallProfile = serde_json::from_str(
            r#"{"id":"00000000-0000-0000-0000-000000000001","name":"test","rules":[],"created":"2024-01-01T00:00:00Z","updated":"2024-01-01T00:00:00Z"}"#,
        )
        .unwrap();
        assert_eq!(profile.default_action, FirewallAction::Drop);
    }

    #[test]
    fn test_profile_roundtrip() {
        let profile = FirewallProfile {
            id: Uuid::new_v4(),
            name: "web-server".to_string(),
            description: "Web server profile".to_string(),
            default_action: FirewallAction::Drop,
            rules: vec![
                FirewallRule {
                    priority: 10,
                    action: FirewallAction::Accept,
                    protocol: FirewallProtocol::Tcp,
                    source_cidr: None,
                    dest_port: Some(80),
                    dest_port_end: None,
                    rate_limit: None,
                    log_prefix: None,
                    description: "HTTP".to_string(),
                },
                FirewallRule {
                    priority: 20,
                    action: FirewallAction::Accept,
                    protocol: FirewallProtocol::Tcp,
                    source_cidr: None,
                    dest_port: Some(443),
                    dest_port_end: None,
                    rate_limit: None,
                    log_prefix: None,
                    description: "HTTPS".to_string(),
                },
            ],
            managed: true,
            created: Utc::now(),
            updated: Utc::now(),
        };

        let json = serde_json::to_string(&profile).unwrap();
        let deserialized: FirewallProfile = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.name, "web-server");
        assert_eq!(deserialized.rules.len(), 2);
        assert_eq!(deserialized.default_action, FirewallAction::Drop);
    }
}
