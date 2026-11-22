// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: LicenseRef-Zyvor-Production-2.0

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use uuid::Uuid;

/// Direction of traffic for monitoring.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(rename_all = "lowercase")]
pub enum TrafficDirection {
    Rx,
    Tx,
    #[default]
    Both,
}

/// Unit for bandwidth thresholds.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(rename_all = "lowercase")]
pub enum ThresholdUnit {
    Bps,
    Kbps,
    #[default]
    Mbps,
    Gbps,
}

/// Severity level for bandwidth alerts.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(rename_all = "lowercase")]
pub enum AlertSeverity {
    Info,
    #[default]
    Warning,
    Critical,
}

/// Action to take when threshold is breached.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(rename_all = "lowercase")]
pub enum AlertAction {
    #[default]
    Log,
    Event,
    Webhook,
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

/// A bandwidth threshold with direction and severity.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BandwidthThreshold {
    pub value: u64,
    #[serde(default)]
    pub unit: ThresholdUnit,
    #[serde(default)]
    pub direction: TrafficDirection,
    #[serde(default)]
    pub severity: AlertSeverity,
}

impl BandwidthThreshold {
    /// Convert threshold to bytes per second.
    pub fn to_bps(&self) -> u64 {
        match self.unit {
            ThresholdUnit::Bps => self.value,
            ThresholdUnit::Kbps => self.value * 1_000,
            ThresholdUnit::Mbps => self.value * 1_000_000,
            ThresholdUnit::Gbps => self.value * 1_000_000_000,
        }
    }
}

fn default_sample_interval() -> u64 {
    10
}

fn default_true() -> bool {
    true
}

/// A monitoring policy that defines thresholds and alerts.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MonitorPolicy {
    pub id: Uuid,
    pub name: String,
    #[serde(default)]
    pub description: String,
    pub selector: LabelSelector,
    pub thresholds: Vec<BandwidthThreshold>,
    #[serde(default)]
    pub action: AlertAction,
    #[serde(default)]
    pub webhook_url: Option<String>,
    #[serde(default = "default_sample_interval")]
    pub sample_interval_secs: u64,
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default = "default_true")]
    pub managed: bool,
    pub created: DateTime<Utc>,
    pub updated: DateTime<Utc>,
}

/// API request to create a monitoring policy.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateMonitorPolicyRequest {
    pub name: String,
    #[serde(default)]
    pub description: String,
    pub selector: LabelSelector,
    pub thresholds: Vec<BandwidthThreshold>,
    #[serde(default)]
    pub action: AlertAction,
    #[serde(default)]
    pub webhook_url: Option<String>,
    #[serde(default = "default_sample_interval")]
    pub sample_interval_secs: u64,
    #[serde(default = "default_true")]
    pub enabled: bool,
}

/// Raw counters from a network interface.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct InterfaceCounters {
    pub rx_bytes: u64,
    pub tx_bytes: u64,
    pub rx_packets: u64,
    pub tx_packets: u64,
    pub rx_errors: u64,
    pub tx_errors: u64,
    pub rx_dropped: u64,
    pub tx_dropped: u64,
}

/// Computed network metrics for a VM.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetworkMetrics {
    pub vm_name: String,
    pub interface: String,
    pub counters: InterfaceCounters,
    pub rx_bps: f64,
    pub tx_bps: f64,
    pub rx_pps: f64,
    pub tx_pps: f64,
    pub sampled_at: DateTime<Utc>,
}

/// An alert generated when a threshold is breached.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BandwidthAlert {
    pub id: Uuid,
    pub policy_name: String,
    pub vm_name: String,
    pub direction: TrafficDirection,
    pub threshold_bps: u64,
    pub actual_bps: f64,
    pub severity: AlertSeverity,
    pub triggered_at: DateTime<Utc>,
}

/// Status report for a monitoring policy.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MonitorStatus {
    pub policy_id: Uuid,
    pub name: String,
    pub matching_vms: usize,
    pub active_alerts: usize,
    pub enforced: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_threshold_to_bps() {
        let bps = BandwidthThreshold {
            value: 100,
            unit: ThresholdUnit::Bps,
            direction: TrafficDirection::Rx,
            severity: AlertSeverity::Info,
        };
        assert_eq!(bps.to_bps(), 100);

        let kbps = BandwidthThreshold {
            value: 100,
            unit: ThresholdUnit::Kbps,
            direction: TrafficDirection::Rx,
            severity: AlertSeverity::Info,
        };
        assert_eq!(kbps.to_bps(), 100_000);

        let mbps = BandwidthThreshold {
            value: 100,
            unit: ThresholdUnit::Mbps,
            direction: TrafficDirection::Rx,
            severity: AlertSeverity::Info,
        };
        assert_eq!(mbps.to_bps(), 100_000_000);

        let gbps = BandwidthThreshold {
            value: 1,
            unit: ThresholdUnit::Gbps,
            direction: TrafficDirection::Rx,
            severity: AlertSeverity::Info,
        };
        assert_eq!(gbps.to_bps(), 1_000_000_000);
    }

    #[test]
    fn test_direction_default() {
        let threshold: BandwidthThreshold =
            serde_json::from_str(r#"{"value":100,"unit":"mbps","severity":"warning"}"#).unwrap();
        assert_eq!(threshold.direction, TrafficDirection::Both);
    }

    #[test]
    fn test_severity_variants() {
        let info: AlertSeverity = serde_json::from_str(r#""info""#).unwrap();
        assert_eq!(info, AlertSeverity::Info);

        let warn: AlertSeverity = serde_json::from_str(r#""warning""#).unwrap();
        assert_eq!(warn, AlertSeverity::Warning);

        let crit: AlertSeverity = serde_json::from_str(r#""critical""#).unwrap();
        assert_eq!(crit, AlertSeverity::Critical);
    }

    #[test]
    fn test_action_default() {
        let policy: MonitorPolicy = serde_json::from_str(
            r#"{"id":"00000000-0000-0000-0000-000000000000","name":"test","selector":{},"thresholds":[],"enabled":true,"created":"2024-01-01T00:00:00Z","updated":"2024-01-01T00:00:00Z"}"#,
        ).unwrap();
        assert_eq!(policy.action, AlertAction::Log);
    }

    #[test]
    fn test_label_matching() {
        let mut selector = LabelSelector::default();
        selector
            .match_labels
            .insert("tier".to_string(), "production".to_string());

        let mut labels = HashMap::new();
        labels.insert("tier".to_string(), "production".to_string());
        assert!(selector.matches(&labels));

        let empty = HashMap::new();
        assert!(!selector.matches(&empty));
    }

    #[test]
    fn test_policy_roundtrip() {
        let policy = MonitorPolicy {
            id: Uuid::new_v4(),
            name: "high-bandwidth".to_string(),
            description: "Alert on high bandwidth".to_string(),
            selector: LabelSelector::default(),
            thresholds: vec![BandwidthThreshold {
                value: 100,
                unit: ThresholdUnit::Mbps,
                direction: TrafficDirection::Both,
                severity: AlertSeverity::Warning,
            }],
            action: AlertAction::Log,
            webhook_url: None,
            sample_interval_secs: 10,
            enabled: true,
            managed: true,
            created: Utc::now(),
            updated: Utc::now(),
        };

        let json = serde_json::to_string(&policy).unwrap();
        let deserialized: MonitorPolicy = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.name, "high-bandwidth");
        assert_eq!(deserialized.thresholds.len(), 1);
        assert_eq!(deserialized.sample_interval_secs, 10);
    }

    #[test]
    fn test_counters_default() {
        let counters = InterfaceCounters::default();
        assert_eq!(counters.rx_bytes, 0);
        assert_eq!(counters.tx_bytes, 0);
        assert_eq!(counters.rx_packets, 0);
        assert_eq!(counters.tx_packets, 0);
        assert_eq!(counters.rx_errors, 0);
        assert_eq!(counters.tx_errors, 0);
        assert_eq!(counters.rx_dropped, 0);
        assert_eq!(counters.tx_dropped, 0);
    }
}
