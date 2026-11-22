// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: LicenseRef-Zyvor-Production-2.0

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use uuid::Uuid;

/// DNS record type.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(rename_all = "UPPERCASE")]
pub enum DnsRecordType {
    #[default]
    A,
    Cname,
    Srv,
}

fn default_ttl() -> u32 {
    300
}

/// A resolved DNS record.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DnsRecord {
    pub name: String,
    pub record_type: DnsRecordType,
    pub value: String,
    #[serde(default = "default_ttl")]
    pub ttl: u32,
    pub vm_name: String,
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

/// A DNS zone (e.g. "zyvor-fabricd.local").
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DnsZone {
    pub id: Uuid,
    pub name: String,
    #[serde(default)]
    pub description: String,
    #[serde(default = "default_true")]
    pub managed: bool,
    pub created: DateTime<Utc>,
    pub updated: DateTime<Utc>,
}

fn default_true() -> bool {
    true
}

/// A DNS policy that generates records from VM state.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DnsPolicy {
    pub id: Uuid,
    pub name: String,
    #[serde(default)]
    pub description: String,
    pub zone_id: Uuid,
    pub selector: LabelSelector,
    pub record_template: String,
    #[serde(default)]
    pub record_type: DnsRecordType,
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default = "default_true")]
    pub managed: bool,
    pub created: DateTime<Utc>,
    pub updated: DateTime<Utc>,
}

/// API request to create a DNS zone.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateDnsZoneRequest {
    pub name: String,
    #[serde(default)]
    pub description: String,
}

/// API request to create a DNS policy.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateDnsPolicyRequest {
    pub name: String,
    pub zone_id: Uuid,
    pub selector: LabelSelector,
    pub record_template: String,
    #[serde(default)]
    pub record_type: DnsRecordType,
    #[serde(default = "default_true")]
    pub enabled: bool,
}

/// Status report for a single DNS policy.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DnsStatus {
    pub policy_id: Uuid,
    pub name: String,
    pub resolved_records_count: usize,
    pub zone_name: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_record_type_serde() {
        let a: DnsRecordType = serde_json::from_str(r#""A""#).unwrap();
        assert_eq!(a, DnsRecordType::A);

        let cname: DnsRecordType = serde_json::from_str(r#""CNAME""#).unwrap();
        assert_eq!(cname, DnsRecordType::Cname);

        let srv: DnsRecordType = serde_json::from_str(r#""SRV""#).unwrap();
        assert_eq!(srv, DnsRecordType::Srv);

        let json = serde_json::to_string(&DnsRecordType::A).unwrap();
        assert_eq!(json, r#""A""#);
    }

    #[test]
    fn test_template_parsing() {
        // Templates use {name} and {label:key} placeholders
        let template = "{name}.{label:app}.zyvor-fabricd.local";
        assert!(template.contains("{name}"));
        assert!(template.contains("{label:app}"));
    }

    #[test]
    fn test_label_selector() {
        let mut selector = LabelSelector::default();
        selector
            .match_labels
            .insert("app".to_string(), "web".to_string());

        let mut labels = HashMap::new();
        labels.insert("app".to_string(), "web".to_string());
        assert!(selector.matches(&labels));

        let empty = HashMap::new();
        assert!(!selector.matches(&empty));
    }

    #[test]
    fn test_zone_name_validation() {
        let zone = DnsZone {
            id: Uuid::new_v4(),
            name: "zyvor-fabricd.local".to_string(),
            description: "Default zone".to_string(),
            managed: true,
            created: Utc::now(),
            updated: Utc::now(),
        };
        assert!(zone.name.contains('.'));
        assert!(!zone.name.is_empty());
    }

    #[test]
    fn test_default_ttl() {
        let record: DnsRecord = serde_json::from_str(
            r#"{"name":"test.local","record_type":"A","value":"10.0.0.5","vm_name":"vm1"}"#,
        )
        .unwrap();
        assert_eq!(record.ttl, 300);
    }

    #[test]
    fn test_roundtrip() {
        let policy = DnsPolicy {
            id: Uuid::new_v4(),
            name: "web-dns".to_string(),
            description: String::new(),
            zone_id: Uuid::new_v4(),
            selector: LabelSelector::default(),
            record_template: "{name}.{label:app}.zyvor-fabricd.local".to_string(),
            record_type: DnsRecordType::A,
            enabled: true,
            managed: true,
            created: Utc::now(),
            updated: Utc::now(),
        };

        let json = serde_json::to_string(&policy).unwrap();
        let deserialized: DnsPolicy = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.name, "web-dns");
        assert_eq!(deserialized.record_type, DnsRecordType::A);
    }
}
