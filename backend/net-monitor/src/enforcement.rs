// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: LicenseRef-Zyvor-Production-2.0

use std::sync::Arc;
use tokio::sync::RwLock;

use chrono::Utc;
use uuid::Uuid;

use crate::collector::VMSnapshot;
use crate::models::*;

/// Evaluates bandwidth thresholds and generates alerts.
pub struct AlertEvaluator {
    /// Active alerts.
    alerts: Arc<RwLock<Vec<BandwidthAlert>>>,
}

impl Default for AlertEvaluator {
    fn default() -> Self {
        Self::new()
    }
}

impl AlertEvaluator {
    pub fn new() -> Self {
        Self {
            alerts: Arc::new(RwLock::new(Vec::new())),
        }
    }

    /// Evaluate all policies against current metrics.
    pub async fn evaluate(
        &self,
        policies: &[MonitorPolicy],
        metrics: &[NetworkMetrics],
        all_vms: &[VMSnapshot],
    ) -> Vec<BandwidthAlert> {
        let mut new_alerts = Vec::new();

        for policy in policies {
            if !policy.enabled {
                continue;
            }

            for metric in metrics {
                // Check if this VM matches the policy
                let vm = all_vms.iter().find(|v| v.name == metric.vm_name);
                if let Some(vm) = vm {
                    if !policy.selector.matches(&vm.labels) {
                        continue;
                    }
                }

                let alerts = self.check_thresholds(policy, metric);
                for alert in &alerts {
                    self.fire_alert(alert, policy);
                }
                new_alerts.extend(alerts);
            }
        }

        // Store active alerts
        let mut active = self.alerts.write().await;
        active.extend(new_alerts.clone());

        // Keep only last 1000 alerts
        if active.len() > 1000 {
            let drain_count = active.len() - 1000;
            active.drain(..drain_count);
        }

        new_alerts
    }

    /// Check a single VM's metrics against a policy's thresholds.
    pub fn check_thresholds(
        &self,
        policy: &MonitorPolicy,
        metric: &NetworkMetrics,
    ) -> Vec<BandwidthAlert> {
        let mut alerts = Vec::new();

        for threshold in &policy.thresholds {
            let threshold_bps = threshold.to_bps();

            match threshold.direction {
                TrafficDirection::Rx => {
                    if metric.rx_bps > threshold_bps as f64 {
                        alerts.push(BandwidthAlert {
                            id: Uuid::new_v4(),
                            policy_name: policy.name.clone(),
                            vm_name: metric.vm_name.clone(),
                            direction: TrafficDirection::Rx,
                            threshold_bps,
                            actual_bps: metric.rx_bps,
                            severity: threshold.severity.clone(),
                            triggered_at: Utc::now(),
                        });
                    }
                }
                TrafficDirection::Tx => {
                    if metric.tx_bps > threshold_bps as f64 {
                        alerts.push(BandwidthAlert {
                            id: Uuid::new_v4(),
                            policy_name: policy.name.clone(),
                            vm_name: metric.vm_name.clone(),
                            direction: TrafficDirection::Tx,
                            threshold_bps,
                            actual_bps: metric.tx_bps,
                            severity: threshold.severity.clone(),
                            triggered_at: Utc::now(),
                        });
                    }
                }
                TrafficDirection::Both => {
                    if metric.rx_bps > threshold_bps as f64 {
                        alerts.push(BandwidthAlert {
                            id: Uuid::new_v4(),
                            policy_name: policy.name.clone(),
                            vm_name: metric.vm_name.clone(),
                            direction: TrafficDirection::Rx,
                            threshold_bps,
                            actual_bps: metric.rx_bps,
                            severity: threshold.severity.clone(),
                            triggered_at: Utc::now(),
                        });
                    }
                    if metric.tx_bps > threshold_bps as f64 {
                        alerts.push(BandwidthAlert {
                            id: Uuid::new_v4(),
                            policy_name: policy.name.clone(),
                            vm_name: metric.vm_name.clone(),
                            direction: TrafficDirection::Tx,
                            threshold_bps,
                            actual_bps: metric.tx_bps,
                            severity: threshold.severity.clone(),
                            triggered_at: Utc::now(),
                        });
                    }
                }
            }
        }

        alerts
    }

    /// Fire an alert (log, event, or webhook).
    pub fn fire_alert(&self, alert: &BandwidthAlert, policy: &MonitorPolicy) {
        match policy.action {
            AlertAction::Log => {
                tracing::warn!(
                    "Bandwidth alert: VM '{}' {:?} {:.0} bps exceeds {} bps threshold (policy: {}, severity: {:?})",
                    alert.vm_name,
                    alert.direction,
                    alert.actual_bps,
                    alert.threshold_bps,
                    alert.policy_name,
                    alert.severity,
                );
            }
            AlertAction::Event => {
                tracing::info!(
                    "Bandwidth event: VM '{}' {:?} threshold breach (policy: {})",
                    alert.vm_name,
                    alert.direction,
                    alert.policy_name,
                );
            }
            AlertAction::Webhook => {
                if let Some(ref url) = policy.webhook_url {
                    tracing::info!(
                        "Would send webhook to {} for VM '{}' bandwidth alert",
                        url,
                        alert.vm_name,
                    );
                }
            }
        }
    }

    /// Get all active alerts.
    pub async fn get_active_alerts(&self) -> Vec<BandwidthAlert> {
        self.alerts.read().await.clone()
    }

    /// Get alerts for a specific VM.
    pub async fn get_vm_alerts(&self, name: &str) -> Vec<BandwidthAlert> {
        self.alerts
            .read()
            .await
            .iter()
            .filter(|a| a.vm_name == name)
            .cloned()
            .collect()
    }

    /// Clear alerts for a specific VM.
    pub async fn clear_vm_alerts(&self, name: &str) {
        let mut alerts = self.alerts.write().await;
        alerts.retain(|a| a.vm_name != name);
    }

    /// Acknowledge (dismiss) a single active alert by id. Returns `true` if
    /// an alert with that id was found and removed.
    pub async fn acknowledge_alert(&self, id: Uuid) -> bool {
        let mut alerts = self.alerts.write().await;
        let before = alerts.len();
        alerts.retain(|a| a.id != id);
        alerts.len() != before
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;

    fn make_metric(vm_name: &str, rx_bps: f64, tx_bps: f64) -> NetworkMetrics {
        NetworkMetrics {
            vm_name: vm_name.to_string(),
            interface: format!("tap-{}", vm_name),
            counters: InterfaceCounters::default(),
            rx_bps,
            tx_bps,
            rx_pps: 0.0,
            tx_pps: 0.0,
            sampled_at: Utc::now(),
        }
    }

    fn make_policy(name: &str, thresholds: Vec<BandwidthThreshold>) -> MonitorPolicy {
        MonitorPolicy {
            id: Uuid::new_v4(),
            name: name.to_string(),
            description: String::new(),
            selector: LabelSelector::default(),
            thresholds,
            action: AlertAction::Log,
            webhook_url: None,
            sample_interval_secs: 10,
            enabled: true,
            managed: true,
            created: Utc::now(),
            updated: Utc::now(),
        }
    }

    fn make_threshold(
        value: u64,
        unit: ThresholdUnit,
        direction: TrafficDirection,
        severity: AlertSeverity,
    ) -> BandwidthThreshold {
        BandwidthThreshold {
            value,
            unit,
            direction,
            severity,
        }
    }

    #[test]
    fn test_no_breach() {
        let evaluator = AlertEvaluator::new();
        let metric = make_metric("web-1", 1_000.0, 500.0);
        let policy = make_policy(
            "test",
            vec![make_threshold(
                1,
                ThresholdUnit::Mbps,
                TrafficDirection::Rx,
                AlertSeverity::Warning,
            )],
        );

        let alerts = evaluator.check_thresholds(&policy, &metric);
        assert!(alerts.is_empty());
    }

    #[test]
    fn test_rx_breach() {
        let evaluator = AlertEvaluator::new();
        let metric = make_metric("web-1", 2_000_000.0, 500.0);
        let policy = make_policy(
            "test",
            vec![make_threshold(
                1,
                ThresholdUnit::Mbps,
                TrafficDirection::Rx,
                AlertSeverity::Warning,
            )],
        );

        let alerts = evaluator.check_thresholds(&policy, &metric);
        assert_eq!(alerts.len(), 1);
        assert_eq!(alerts[0].direction, TrafficDirection::Rx);
        assert_eq!(alerts[0].vm_name, "web-1");
    }

    #[test]
    fn test_tx_breach() {
        let evaluator = AlertEvaluator::new();
        let metric = make_metric("web-1", 500.0, 2_000_000.0);
        let policy = make_policy(
            "test",
            vec![make_threshold(
                1,
                ThresholdUnit::Mbps,
                TrafficDirection::Tx,
                AlertSeverity::Critical,
            )],
        );

        let alerts = evaluator.check_thresholds(&policy, &metric);
        assert_eq!(alerts.len(), 1);
        assert_eq!(alerts[0].direction, TrafficDirection::Tx);
        assert_eq!(alerts[0].severity, AlertSeverity::Critical);
    }

    #[test]
    fn test_both_direction() {
        let evaluator = AlertEvaluator::new();
        let metric = make_metric("web-1", 2_000_000.0, 2_000_000.0);
        let policy = make_policy(
            "test",
            vec![make_threshold(
                1,
                ThresholdUnit::Mbps,
                TrafficDirection::Both,
                AlertSeverity::Warning,
            )],
        );

        let alerts = evaluator.check_thresholds(&policy, &metric);
        assert_eq!(alerts.len(), 2); // One for Rx, one for Tx
    }

    #[test]
    fn test_severity_propagation() {
        let evaluator = AlertEvaluator::new();
        let metric = make_metric("web-1", 2_000_000.0, 500.0);
        let policy = make_policy(
            "test",
            vec![make_threshold(
                1,
                ThresholdUnit::Mbps,
                TrafficDirection::Rx,
                AlertSeverity::Critical,
            )],
        );

        let alerts = evaluator.check_thresholds(&policy, &metric);
        assert_eq!(alerts.len(), 1);
        assert_eq!(alerts[0].severity, AlertSeverity::Critical);
    }

    #[tokio::test]
    async fn test_alert_tracking() {
        let evaluator = AlertEvaluator::new();
        let metrics = vec![make_metric("web-1", 2_000_000.0, 500.0)];
        let policies = vec![make_policy(
            "test",
            vec![make_threshold(
                1,
                ThresholdUnit::Mbps,
                TrafficDirection::Rx,
                AlertSeverity::Warning,
            )],
        )];
        let vms: Vec<VMSnapshot> = vec![];

        let new_alerts = evaluator.evaluate(&policies, &metrics, &vms).await;
        assert_eq!(new_alerts.len(), 1);

        let active = evaluator.get_active_alerts().await;
        assert_eq!(active.len(), 1);
    }

    #[tokio::test]
    async fn test_clear_alerts() {
        let evaluator = AlertEvaluator::new();
        let metrics = vec![make_metric("web-1", 2_000_000.0, 500.0)];
        let policies = vec![make_policy(
            "test",
            vec![make_threshold(
                1,
                ThresholdUnit::Mbps,
                TrafficDirection::Rx,
                AlertSeverity::Warning,
            )],
        )];
        let vms: Vec<VMSnapshot> = vec![];

        evaluator.evaluate(&policies, &metrics, &vms).await;
        assert_eq!(evaluator.get_active_alerts().await.len(), 1);

        evaluator.clear_vm_alerts("web-1").await;
        assert_eq!(evaluator.get_active_alerts().await.len(), 0);
    }
}
