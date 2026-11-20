// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: LicenseRef-Zyvor-Production-1.0

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use uuid::Uuid;

/// Unit for bandwidth rate specification.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(rename_all = "lowercase")]
pub enum BandwidthUnit {
    Kbit,
    #[default]
    Mbit,
    Gbit,
}

/// A bandwidth rate with value and unit.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BandwidthRate {
    pub value: u64,
    #[serde(default)]
    pub unit: BandwidthUnit,
}

impl BandwidthRate {
    /// Convert to tc-compatible string, e.g. "100mbit".
    pub fn to_tc_string(&self) -> String {
        let suffix = match self.unit {
            BandwidthUnit::Kbit => "kbit",
            BandwidthUnit::Mbit => "mbit",
            BandwidthUnit::Gbit => "gbit",
        };
        format!("{}{}", self.value, suffix)
    }
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

fn default_priority() -> u8 {
    4
}

/// A traffic class defining QoS parameters.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrafficClass {
    pub name: String,
    pub guaranteed_rate: BandwidthRate,
    pub max_rate: BandwidthRate,
    #[serde(default)]
    pub burst: Option<String>,
    #[serde(default = "default_priority")]
    pub priority: u8,
}

fn default_true() -> bool {
    true
}

/// A QoS policy that maps VMs to traffic classes.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QoSPolicy {
    pub id: Uuid,
    pub name: String,
    #[serde(default)]
    pub description: String,
    pub interface: String,
    pub selector: LabelSelector,
    pub traffic_class: TrafficClass,
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default = "default_true")]
    pub managed: bool,
    pub created: DateTime<Utc>,
    pub updated: DateTime<Utc>,
}

/// API request to create a QoS policy.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateQoSPolicyRequest {
    pub name: String,
    #[serde(default)]
    pub description: String,
    pub interface: String,
    pub selector: LabelSelector,
    pub traffic_class: TrafficClass,
    #[serde(default = "default_true")]
    pub enabled: bool,
}

/// A compiled QoS rule ready for tc enforcement.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompiledQoSRule {
    pub interface: String,
    pub class_id: u16,
    pub rate: String,
    pub ceil: String,
    pub burst: Option<String>,
    pub priority: u8,
    pub vm_ips: Vec<String>,
}

/// Status report for a single QoS policy.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QoSStatus {
    pub policy_id: Uuid,
    pub name: String,
    pub matching_vms: usize,
    pub enforced: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_bandwidth_to_tc_string() {
        let rate_kbit = BandwidthRate {
            value: 512,
            unit: BandwidthUnit::Kbit,
        };
        assert_eq!(rate_kbit.to_tc_string(), "512kbit");

        let rate_mbit = BandwidthRate {
            value: 100,
            unit: BandwidthUnit::Mbit,
        };
        assert_eq!(rate_mbit.to_tc_string(), "100mbit");

        let rate_gbit = BandwidthRate {
            value: 1,
            unit: BandwidthUnit::Gbit,
        };
        assert_eq!(rate_gbit.to_tc_string(), "1gbit");
    }

    #[test]
    fn test_label_matching() {
        let mut selector = LabelSelector::default();
        selector
            .match_labels
            .insert("tier".to_string(), "premium".to_string());

        let mut labels = HashMap::new();
        labels.insert("tier".to_string(), "premium".to_string());
        assert!(selector.matches(&labels));

        let empty = HashMap::new();
        assert!(!selector.matches(&empty));

        let empty_selector = LabelSelector::default();
        assert!(empty_selector.matches(&labels));
    }

    #[test]
    fn test_default_priority() {
        let tc: TrafficClass = serde_json::from_str(
            r#"{"name":"default","guaranteed_rate":{"value":100,"unit":"mbit"},"max_rate":{"value":500,"unit":"mbit"}}"#,
        )
        .unwrap();
        assert_eq!(tc.priority, 4);
    }

    #[test]
    fn test_serde_roundtrip() {
        let policy = QoSPolicy {
            id: Uuid::new_v4(),
            name: "premium-policy".to_string(),
            description: "Premium QoS".to_string(),
            interface: "br0".to_string(),
            selector: LabelSelector::default(),
            traffic_class: TrafficClass {
                name: "premium".to_string(),
                guaranteed_rate: BandwidthRate {
                    value: 100,
                    unit: BandwidthUnit::Mbit,
                },
                max_rate: BandwidthRate {
                    value: 500,
                    unit: BandwidthUnit::Mbit,
                },
                burst: Some("15k".to_string()),
                priority: 1,
            },
            enabled: true,
            managed: true,
            created: Utc::now(),
            updated: Utc::now(),
        };

        let json = serde_json::to_string(&policy).unwrap();
        let deserialized: QoSPolicy = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.name, "premium-policy");
        assert_eq!(deserialized.traffic_class.priority, 1);
    }

    #[test]
    fn test_rate_validation() {
        let rate = BandwidthRate {
            value: 0,
            unit: BandwidthUnit::Mbit,
        };
        assert_eq!(rate.to_tc_string(), "0mbit");

        let large = BandwidthRate {
            value: 10000,
            unit: BandwidthUnit::Mbit,
        };
        assert_eq!(large.to_tc_string(), "10000mbit");
    }

    #[test]
    fn test_burst_default() {
        let tc: TrafficClass = serde_json::from_str(
            r#"{"name":"basic","guaranteed_rate":{"value":10,"unit":"mbit"},"max_rate":{"value":50,"unit":"mbit"}}"#,
        )
        .unwrap();
        assert!(tc.burst.is_none());
    }
}
