// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: LicenseRef-Zyvor-Production-1.0

use std::collections::HashMap;

use crate::models::*;

/// A snapshot of a VM's state for traffic classification.
pub struct VMSnapshot {
    pub name: String,
    pub labels: HashMap<String, String>,
    pub ip: Option<String>,
}

/// First user-assignable class ID (matching identity convention).
const CLASS_ID_BASE: u16 = 256;

/// Resolves QoS policies against VM state to produce compiled rules.
pub struct TrafficClassifier;

impl Default for TrafficClassifier {
    fn default() -> Self {
        Self::new()
    }
}

impl TrafficClassifier {
    pub fn new() -> Self {
        Self
    }

    /// Classify a single policy against VM state.
    pub fn classify(
        &self,
        policy: &QoSPolicy,
        all_vms: &[VMSnapshot],
        class_id: u16,
    ) -> Option<CompiledQoSRule> {
        if !policy.enabled {
            return None;
        }

        let vm_ips: Vec<String> = all_vms
            .iter()
            .filter(|vm| policy.selector.matches(&vm.labels) && vm.ip.is_some())
            .filter_map(|vm| vm.ip.clone())
            .collect();

        if vm_ips.is_empty() {
            return None;
        }

        Some(CompiledQoSRule {
            interface: policy.interface.clone(),
            class_id,
            rate: policy.traffic_class.guaranteed_rate.to_tc_string(),
            ceil: policy.traffic_class.max_rate.to_tc_string(),
            burst: policy.traffic_class.burst.clone(),
            priority: policy.traffic_class.priority,
            vm_ips,
        })
    }

    /// Classify all policies and assign sequential class IDs.
    pub fn classify_all(
        &self,
        policies: &[QoSPolicy],
        all_vms: &[VMSnapshot],
    ) -> Vec<CompiledQoSRule> {
        let mut rules = Vec::new();
        let mut class_id = CLASS_ID_BASE;

        // Sort by priority for consistent ordering
        let mut sorted_policies: Vec<&QoSPolicy> = policies.iter().collect();
        sorted_policies.sort_by_key(|p| p.traffic_class.priority);

        for policy in sorted_policies {
            if let Some(rule) = self.classify(policy, all_vms, class_id) {
                rules.push(rule);
                class_id += 1;
            }
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

    fn make_policy(
        name: &str,
        interface: &str,
        selector: &[(&str, &str)],
        rate: u64,
        ceil: u64,
        priority: u8,
        burst: Option<&str>,
    ) -> QoSPolicy {
        QoSPolicy {
            id: Uuid::new_v4(),
            name: name.to_string(),
            description: String::new(),
            interface: interface.to_string(),
            selector: LabelSelector {
                match_labels: selector
                    .iter()
                    .map(|(k, v)| (k.to_string(), v.to_string()))
                    .collect(),
            },
            traffic_class: TrafficClass {
                name: name.to_string(),
                guaranteed_rate: BandwidthRate {
                    value: rate,
                    unit: BandwidthUnit::Mbit,
                },
                max_rate: BandwidthRate {
                    value: ceil,
                    unit: BandwidthUnit::Mbit,
                },
                burst: burst.map(|s| s.to_string()),
                priority,
            },
            enabled: true,
            managed: true,
            created: Utc::now(),
            updated: Utc::now(),
        }
    }

    #[test]
    fn test_basic_classification() {
        let classifier = TrafficClassifier::new();
        let vms = vec![make_vm("web-1", &[("tier", "premium")], Some("10.0.0.5"))];
        let policy = make_policy("premium", "br0", &[("tier", "premium")], 100, 500, 1, None);

        let result = classifier.classify(&policy, &vms, 256);
        assert!(result.is_some());
        let rule = result.unwrap();
        assert_eq!(rule.interface, "br0");
        assert_eq!(rule.class_id, 256);
        assert_eq!(rule.rate, "100mbit");
        assert_eq!(rule.ceil, "500mbit");
        assert_eq!(rule.vm_ips.len(), 1);
    }

    #[test]
    fn test_no_matches() {
        let classifier = TrafficClassifier::new();
        let vms = vec![make_vm("web-1", &[("tier", "basic")], Some("10.0.0.5"))];
        let policy = make_policy("premium", "br0", &[("tier", "premium")], 100, 500, 1, None);

        let result = classifier.classify(&policy, &vms, 256);
        assert!(result.is_none());
    }

    #[test]
    fn test_multiple_vms_same_class() {
        let classifier = TrafficClassifier::new();
        let vms = vec![
            make_vm("web-1", &[("tier", "premium")], Some("10.0.0.5")),
            make_vm("web-2", &[("tier", "premium")], Some("10.0.0.6")),
        ];
        let policy = make_policy("premium", "br0", &[("tier", "premium")], 100, 500, 1, None);

        let result = classifier.classify(&policy, &vms, 256).unwrap();
        assert_eq!(result.vm_ips.len(), 2);
    }

    #[test]
    fn test_priority_ordering() {
        let classifier = TrafficClassifier::new();
        let vms = vec![
            make_vm("web-1", &[("tier", "premium")], Some("10.0.0.5")),
            make_vm("db-1", &[("tier", "basic")], Some("10.0.0.20")),
        ];
        let policies = vec![
            make_policy("basic", "br0", &[("tier", "basic")], 10, 50, 7, None),
            make_policy("premium", "br0", &[("tier", "premium")], 100, 500, 1, None),
        ];

        let rules = classifier.classify_all(&policies, &vms);
        assert_eq!(rules.len(), 2);
        // Premium (priority 1) should come first
        assert_eq!(rules[0].priority, 1);
        assert_eq!(rules[1].priority, 7);
    }

    #[test]
    fn test_different_interfaces() {
        let classifier = TrafficClassifier::new();
        let vms = vec![
            make_vm("web-1", &[("net", "public")], Some("10.0.0.5")),
            make_vm("db-1", &[("net", "private")], Some("10.0.1.5")),
        ];
        let policies = vec![
            make_policy("public", "br0", &[("net", "public")], 100, 500, 1, None),
            make_policy("private", "br1", &[("net", "private")], 50, 100, 2, None),
        ];

        let rules = classifier.classify_all(&policies, &vms);
        assert_eq!(rules.len(), 2);
        assert_eq!(rules[0].interface, "br0");
        assert_eq!(rules[1].interface, "br1");
    }

    #[test]
    fn test_dedup_class_ids() {
        let classifier = TrafficClassifier::new();
        let vms = vec![
            make_vm("web-1", &[("tier", "a")], Some("10.0.0.5")),
            make_vm("web-2", &[("tier", "b")], Some("10.0.0.6")),
        ];
        let policies = vec![
            make_policy("tier-a", "br0", &[("tier", "a")], 100, 500, 1, None),
            make_policy("tier-b", "br0", &[("tier", "b")], 50, 200, 2, None),
        ];

        let rules = classifier.classify_all(&policies, &vms);
        assert_eq!(rules.len(), 2);
        // Class IDs should be sequential
        assert_eq!(rules[0].class_id, 256);
        assert_eq!(rules[1].class_id, 257);
    }

    #[test]
    fn test_burst_propagation() {
        let classifier = TrafficClassifier::new();
        let vms = vec![make_vm("web-1", &[("tier", "premium")], Some("10.0.0.5"))];
        let policy = make_policy(
            "premium",
            "br0",
            &[("tier", "premium")],
            100,
            500,
            1,
            Some("15k"),
        );

        let result = classifier.classify(&policy, &vms, 256).unwrap();
        assert_eq!(result.burst, Some("15k".to_string()));
    }

    #[test]
    fn test_disabled_policy() {
        let classifier = TrafficClassifier::new();
        let vms = vec![make_vm("web-1", &[("tier", "premium")], Some("10.0.0.5"))];
        let mut policy = make_policy("premium", "br0", &[("tier", "premium")], 100, 500, 1, None);
        policy.enabled = false;

        let result = classifier.classify(&policy, &vms, 256);
        assert!(result.is_none());
    }
}
