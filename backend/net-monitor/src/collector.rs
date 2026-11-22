// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: LicenseRef-Zyvor-Production-2.0

use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

use chrono::Utc;

use crate::models::*;

/// A snapshot of a VM's state for metric collection.
pub struct VMSnapshot {
    pub name: String,
    pub labels: HashMap<String, String>,
    pub tap_interface: Option<String>,
}

/// Previous counter readings keyed by interface name.
type PreviousCounters = Arc<RwLock<HashMap<String, (InterfaceCounters, chrono::DateTime<Utc>)>>>;

/// Collects network interface counters and computes rates.
pub struct MetricsCollector {
    /// Previous counter readings keyed by interface name.
    previous: PreviousCounters,
    /// Current computed metrics keyed by VM name.
    metrics: Arc<RwLock<HashMap<String, NetworkMetrics>>>,
}

impl Default for MetricsCollector {
    fn default() -> Self {
        Self::new()
    }
}

impl MetricsCollector {
    pub fn new() -> Self {
        Self {
            previous: Arc::new(RwLock::new(HashMap::new())),
            metrics: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Read counters from `/sys/class/net/<iface>/statistics/*`.
    pub fn read_counters(&self, interface: &str) -> anyhow::Result<InterfaceCounters> {
        Ok(InterfaceCounters {
            rx_bytes: self.read_sysfs_counter(interface, "rx_bytes")?,
            tx_bytes: self.read_sysfs_counter(interface, "tx_bytes")?,
            rx_packets: self.read_sysfs_counter(interface, "rx_packets")?,
            tx_packets: self.read_sysfs_counter(interface, "tx_packets")?,
            rx_errors: self.read_sysfs_counter(interface, "rx_errors")?,
            tx_errors: self.read_sysfs_counter(interface, "tx_errors")?,
            rx_dropped: self.read_sysfs_counter(interface, "rx_dropped")?,
            tx_dropped: self.read_sysfs_counter(interface, "tx_dropped")?,
        })
    }

    /// Read a single counter from sysfs.
    pub fn read_sysfs_counter(&self, iface: &str, counter: &str) -> anyhow::Result<u64> {
        let path = format!("/sys/class/net/{}/statistics/{}", iface, counter);
        let content = std::fs::read_to_string(&path)
            .map_err(|e| anyhow::anyhow!("Failed to read {}: {}", path, e))?;
        content
            .trim()
            .parse::<u64>()
            .map_err(|e| anyhow::anyhow!("Failed to parse {}: {}", path, e))
    }

    /// Collect metrics for VMs matching any policy.
    pub async fn collect_for_vms(
        &self,
        policies: &[MonitorPolicy],
        all_vms: &[VMSnapshot],
    ) -> Vec<NetworkMetrics> {
        let now = Utc::now();
        let mut collected = Vec::new();

        // Find unique VMs matching any enabled policy
        let mut matched_vms: HashMap<String, &VMSnapshot> = HashMap::new();
        for policy in policies {
            if !policy.enabled {
                continue;
            }
            for vm in all_vms {
                if policy.selector.matches(&vm.labels) && vm.tap_interface.is_some() {
                    matched_vms.entry(vm.name.clone()).or_insert(vm);
                }
            }
        }

        for (vm_name, vm) in &matched_vms {
            let iface = match &vm.tap_interface {
                Some(i) => i,
                None => continue,
            };

            let counters = match self.read_counters(iface) {
                Ok(c) => c,
                Err(e) => {
                    tracing::debug!("Failed to read counters for {}: {}", iface, e);
                    continue;
                }
            };

            let mut prev_map = self.previous.write().await;
            let (rx_bps, tx_bps, rx_pps, tx_pps) =
                if let Some((prev, prev_time)) = prev_map.get(iface.as_str()) {
                    let elapsed = (now - *prev_time).num_milliseconds() as f64 / 1000.0;
                    Self::compute_rate(prev, &counters, elapsed)
                } else {
                    (0.0, 0.0, 0.0, 0.0)
                };

            prev_map.insert(iface.clone(), (counters.clone(), now));

            let metric = NetworkMetrics {
                vm_name: vm_name.clone(),
                interface: iface.clone(),
                counters,
                rx_bps,
                tx_bps,
                rx_pps,
                tx_pps,
                sampled_at: now,
            };

            self.metrics
                .write()
                .await
                .insert(vm_name.clone(), metric.clone());

            collected.push(metric);
        }

        collected
    }

    /// Compute rates from previous and current counters.
    pub fn compute_rate(
        prev: &InterfaceCounters,
        curr: &InterfaceCounters,
        elapsed_secs: f64,
    ) -> (f64, f64, f64, f64) {
        if elapsed_secs <= 0.0 {
            return (0.0, 0.0, 0.0, 0.0);
        }

        // Handle counter wraparound (u64 won't wrap often, but handle gracefully)
        let rx_bytes_diff = curr.rx_bytes.wrapping_sub(prev.rx_bytes);
        let tx_bytes_diff = curr.tx_bytes.wrapping_sub(prev.tx_bytes);
        let rx_packets_diff = curr.rx_packets.wrapping_sub(prev.rx_packets);
        let tx_packets_diff = curr.tx_packets.wrapping_sub(prev.tx_packets);

        let rx_bps = rx_bytes_diff as f64 / elapsed_secs;
        let tx_bps = tx_bytes_diff as f64 / elapsed_secs;
        let rx_pps = rx_packets_diff as f64 / elapsed_secs;
        let tx_pps = tx_packets_diff as f64 / elapsed_secs;

        (rx_bps, tx_bps, rx_pps, tx_pps)
    }

    /// Get metrics for a specific VM.
    pub async fn get_vm_metrics(&self, name: &str) -> Option<NetworkMetrics> {
        self.metrics.read().await.get(name).cloned()
    }

    /// Get all current metrics.
    pub async fn get_all_metrics(&self) -> Vec<NetworkMetrics> {
        self.metrics.read().await.values().cloned().collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_collector() {
        let collector = MetricsCollector::new();
        // Should be able to create without panic
        assert!(Arc::strong_count(&collector.previous) >= 1);
    }

    #[test]
    fn test_compute_rate() {
        let prev = InterfaceCounters {
            rx_bytes: 1000,
            tx_bytes: 500,
            rx_packets: 10,
            tx_packets: 5,
            ..Default::default()
        };
        let curr = InterfaceCounters {
            rx_bytes: 2000,
            tx_bytes: 1500,
            rx_packets: 20,
            tx_packets: 15,
            ..Default::default()
        };

        let (rx_bps, tx_bps, rx_pps, tx_pps) = MetricsCollector::compute_rate(&prev, &curr, 1.0);
        assert_eq!(rx_bps, 1000.0);
        assert_eq!(tx_bps, 1000.0);
        assert_eq!(rx_pps, 10.0);
        assert_eq!(tx_pps, 10.0);
    }

    #[test]
    fn test_zero_elapsed() {
        let prev = InterfaceCounters::default();
        let curr = InterfaceCounters {
            rx_bytes: 1000,
            ..Default::default()
        };

        let (rx_bps, tx_bps, rx_pps, tx_pps) = MetricsCollector::compute_rate(&prev, &curr, 0.0);
        assert_eq!(rx_bps, 0.0);
        assert_eq!(tx_bps, 0.0);
        assert_eq!(rx_pps, 0.0);
        assert_eq!(tx_pps, 0.0);
    }

    #[test]
    fn test_counter_wraparound() {
        let prev = InterfaceCounters {
            rx_bytes: u64::MAX - 100,
            tx_bytes: u64::MAX - 50,
            rx_packets: u64::MAX - 10,
            tx_packets: u64::MAX - 5,
            ..Default::default()
        };
        let curr = InterfaceCounters {
            rx_bytes: 100,
            tx_bytes: 50,
            rx_packets: 10,
            tx_packets: 5,
            ..Default::default()
        };

        let (rx_bps, tx_bps, _rx_pps, _tx_pps) = MetricsCollector::compute_rate(&prev, &curr, 1.0);
        // wrapping_sub should handle this
        assert!(rx_bps > 0.0);
        assert!(tx_bps > 0.0);
    }

    #[tokio::test]
    async fn test_get_vm_metrics_empty() {
        let collector = MetricsCollector::new();
        let result = collector.get_vm_metrics("nonexistent").await;
        assert!(result.is_none());
    }

    #[test]
    fn test_sysfs_counter_format() {
        // Verify the path format is correct
        let iface = "tap-web-1";
        let counter = "rx_bytes";
        let path = format!("/sys/class/net/{}/statistics/{}", iface, counter);
        assert_eq!(path, "/sys/class/net/tap-web-1/statistics/rx_bytes");
    }

    #[test]
    fn test_rate_calculation() {
        let prev = InterfaceCounters {
            rx_bytes: 0,
            tx_bytes: 0,
            rx_packets: 0,
            tx_packets: 0,
            ..Default::default()
        };
        let curr = InterfaceCounters {
            rx_bytes: 10_000_000,
            tx_bytes: 5_000_000,
            rx_packets: 10_000,
            tx_packets: 5_000,
            ..Default::default()
        };

        let (rx_bps, tx_bps, rx_pps, tx_pps) = MetricsCollector::compute_rate(&prev, &curr, 10.0);
        assert_eq!(rx_bps, 1_000_000.0); // ~1 MB/s
        assert_eq!(tx_bps, 500_000.0);
        assert_eq!(rx_pps, 1_000.0);
        assert_eq!(tx_pps, 500.0);
    }

    #[tokio::test]
    async fn test_metric_storage() {
        let collector = MetricsCollector::new();
        let all = collector.get_all_metrics().await;
        assert!(all.is_empty());
    }
}
