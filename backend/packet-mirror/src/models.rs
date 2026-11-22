// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: LicenseRef-Zyvor-Production-2.0

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use uuid::Uuid;

/// Direction of traffic to mirror.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum MirrorDirection {
    Ingress,
    Egress,
    #[default]
    Both,
}

/// Type of mirror collector.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum CollectorType {
    #[default]
    Interface,
    RemoteIp,
}

/// Optional filter for mirrored traffic.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MirrorFilter {
    #[serde(default)]
    pub protocol: Option<String>,
    #[serde(default)]
    pub src_cidr: Option<String>,
    #[serde(default)]
    pub dst_cidr: Option<String>,
    #[serde(default)]
    pub dst_port: Option<u16>,
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

/// A mirror session that captures traffic from matched VMs.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MirrorSession {
    pub id: Uuid,
    pub name: String,
    #[serde(default)]
    pub description: String,
    pub selector: LabelSelector,
    #[serde(default)]
    pub collector_type: CollectorType,
    pub collector_target: String,
    #[serde(default)]
    pub direction: MirrorDirection,
    #[serde(default)]
    pub filter: Option<MirrorFilter>,
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default = "default_true")]
    pub managed: bool,
    pub created: DateTime<Utc>,
    pub updated: DateTime<Utc>,
}

/// API request to create a mirror session.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateMirrorSessionRequest {
    pub name: String,
    #[serde(default)]
    pub description: String,
    pub selector: LabelSelector,
    #[serde(default)]
    pub collector_type: CollectorType,
    pub collector_target: String,
    #[serde(default)]
    pub direction: MirrorDirection,
    #[serde(default)]
    pub filter: Option<MirrorFilter>,
    #[serde(default = "default_true")]
    pub enabled: bool,
}

/// A compiled mirror rule ready for tc enforcement.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompiledMirrorRule {
    pub source_interface: String,
    pub vm_name: String,
    pub collector_target: String,
    pub direction: MirrorDirection,
    pub filter: Option<MirrorFilter>,
    pub session_name: String,
}

/// Status report for a mirror session.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MirrorStatus {
    pub session_id: Uuid,
    pub name: String,
    pub matching_vms: usize,
    pub active_mirrors: usize,
    pub enforced: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_direction_default() {
        let session: MirrorSession = serde_json::from_str(
            r#"{"id":"00000000-0000-0000-0000-000000000000","name":"test","selector":{},"collector_target":"mon0","enabled":true,"created":"2024-01-01T00:00:00Z","updated":"2024-01-01T00:00:00Z"}"#,
        ).unwrap();
        assert_eq!(session.direction, MirrorDirection::Both);
    }

    #[test]
    fn test_collector_type_serde() {
        let json = serde_json::to_string(&CollectorType::Interface).unwrap();
        let deserialized: CollectorType = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized, CollectorType::Interface);

        let json2 = serde_json::to_string(&CollectorType::RemoteIp).unwrap();
        let deserialized2: CollectorType = serde_json::from_str(&json2).unwrap();
        assert_eq!(deserialized2, CollectorType::RemoteIp);
    }

    #[test]
    fn test_label_matching() {
        let mut selector = LabelSelector::default();
        selector
            .match_labels
            .insert("env".to_string(), "debug".to_string());

        let mut labels = HashMap::new();
        labels.insert("env".to_string(), "debug".to_string());
        assert!(selector.matches(&labels));

        let empty = HashMap::new();
        assert!(!selector.matches(&empty));
    }

    #[test]
    fn test_filter_serde() {
        let filter = MirrorFilter {
            protocol: Some("tcp".to_string()),
            src_cidr: None,
            dst_cidr: Some("10.0.0.0/24".to_string()),
            dst_port: Some(80),
        };

        let json = serde_json::to_string(&filter).unwrap();
        let deserialized: MirrorFilter = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.protocol, Some("tcp".to_string()));
        assert!(deserialized.src_cidr.is_none());
        assert_eq!(deserialized.dst_port, Some(80));
    }

    #[test]
    fn test_session_roundtrip() {
        let session = MirrorSession {
            id: Uuid::new_v4(),
            name: "debug-mirror".to_string(),
            description: "Mirror for debugging".to_string(),
            selector: LabelSelector::default(),
            collector_type: CollectorType::Interface,
            collector_target: "mon0".to_string(),
            direction: MirrorDirection::Ingress,
            filter: None,
            enabled: true,
            managed: true,
            created: Utc::now(),
            updated: Utc::now(),
        };

        let json = serde_json::to_string(&session).unwrap();
        let deserialized: MirrorSession = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.name, "debug-mirror");
        assert_eq!(deserialized.direction, MirrorDirection::Ingress);
    }

    #[test]
    fn test_direction_variants() {
        let ingress: MirrorDirection = serde_json::from_str(r#""ingress""#).unwrap();
        assert_eq!(ingress, MirrorDirection::Ingress);

        let egress: MirrorDirection = serde_json::from_str(r#""egress""#).unwrap();
        assert_eq!(egress, MirrorDirection::Egress);

        let both: MirrorDirection = serde_json::from_str(r#""both""#).unwrap();
        assert_eq!(both, MirrorDirection::Both);
    }
}
