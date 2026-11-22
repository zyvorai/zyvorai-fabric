// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: LicenseRef-Zyvor-Production-2.0

use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Node {
    pub id: String,
    pub hostname: String,
    pub total_cpus: u32,
    pub total_memory_mb: u64,
    pub used_cpus: u32,
    pub used_memory_mb: u64,
    pub vm_count: u32,
    pub is_healthy: bool,
}

impl Node {
    pub fn available_cpus(&self) -> u32 {
        self.total_cpus.saturating_sub(self.used_cpus)
    }

    pub fn available_memory_mb(&self) -> u64 {
        self.total_memory_mb.saturating_sub(self.used_memory_mb)
    }

    pub fn cpu_utilization(&self) -> f64 {
        (self.used_cpus as f64 / self.total_cpus as f64) * 100.0
    }

    pub fn memory_utilization(&self) -> f64 {
        (self.used_memory_mb as f64 / self.total_memory_mb as f64) * 100.0
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VMRequest {
    pub name: String,
    pub cpus: u32,
    pub memory_mb: u64,
    pub affinity: Option<Vec<String>>, // Preferred nodes
    pub anti_affinity: Option<Vec<String>>, // Nodes to avoid
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum SchedulingStrategy {
    BinPacking,      // Pack VMs tightly on fewer nodes
    Spread,          // Spread VMs across nodes
    Balanced,        // Balance resource usage
    LeastLoaded,     // Place on least loaded node
}

pub struct Scheduler {
    nodes: Arc<RwLock<HashMap<String, Node>>>,
    strategy: SchedulingStrategy,
}

impl Scheduler {
    pub fn new(strategy: SchedulingStrategy) -> Self {
        Self {
            nodes: Arc::new(RwLock::new(HashMap::new())),
            strategy,
        }
    }

    /// Register a node
    pub async fn register_node(&self, node: Node) -> Result<()> {
        let mut nodes = self.nodes.write().await;
        nodes.insert(node.id.clone(), node);
        tracing::info!("Node registered: {}", node.id);
        Ok(())
    }

    /// Unregister a node
    pub async fn unregister_node(&self, node_id: &str) -> Result<()> {
        let mut nodes = self.nodes.write().await;
        nodes.remove(node_id);
        tracing::info!("Node unregistered: {}", node_id);
        Ok(())
    }

    /// Schedule a VM on the best available node
    pub async fn schedule_vm(&self, request: &VMRequest) -> Result<String> {
        let nodes = self.nodes.read().await;

        // Filter healthy nodes that can accommodate the VM
        let mut candidates: Vec<&Node> = nodes
            .values()
            .filter(|n| n.is_healthy)
            .filter(|n| n.available_cpus() >= request.cpus)
            .filter(|n| n.available_memory_mb() >= request.memory_mb)
            .collect();

        // Apply affinity rules
        if let Some(preferred_nodes) = &request.affinity {
            candidates.retain(|n| preferred_nodes.contains(&n.id));
        }

        // Apply anti-affinity rules
        if let Some(avoided_nodes) = &request.anti_affinity {
            candidates.retain(|n| !avoided_nodes.contains(&n.id));
        }

        if candidates.is_empty() {
            return Err(anyhow::anyhow!("No suitable node found for VM {}", request.name));
        }

        // Select best node based on strategy
        let selected_node = match self.strategy {
            SchedulingStrategy::BinPacking => {
                self.select_bin_packing(&candidates)
            }
            SchedulingStrategy::Spread => {
                self.select_spread(&candidates)
            }
            SchedulingStrategy::Balanced => {
                self.select_balanced(&candidates)
            }
            SchedulingStrategy::LeastLoaded => {
                self.select_least_loaded(&candidates)
            }
        };

        tracing::info!(
            "Scheduled VM {} on node {}",
            request.name,
            selected_node.id
        );

        Ok(selected_node.id.clone())
    }

    /// Bin packing: Choose node with highest utilization
    fn select_bin_packing(&self, candidates: &[&Node]) -> &Node {
        candidates
            .iter()
            .max_by(|a, b| {
                let a_util = a.cpu_utilization() + a.memory_utilization();
                let b_util = b.cpu_utilization() + b.memory_utilization();
                a_util.partial_cmp(&b_util).unwrap()
            })
            .unwrap()
    }

    /// Spread: Choose node with lowest VM count
    fn select_spread(&self, candidates: &[&Node]) -> &Node {
        candidates
            .iter()
            .min_by_key(|n| n.vm_count)
            .unwrap()
    }

    /// Balanced: Choose node with most balanced CPU/Memory usage
    fn select_balanced(&self, candidates: &[&Node]) -> &Node {
        candidates
            .iter()
            .min_by(|a, b| {
                let a_diff = (a.cpu_utilization() - a.memory_utilization()).abs();
                let b_diff = (b.cpu_utilization() - b.memory_utilization()).abs();
                a_diff.partial_cmp(&b_diff).unwrap()
            })
            .unwrap()
    }

    /// Least loaded: Choose node with lowest overall utilization
    fn select_least_loaded(&self, candidates: &[&Node]) -> &Node {
        candidates
            .iter()
            .min_by(|a, b| {
                let a_util = a.cpu_utilization() + a.memory_utilization();
                let b_util = b.cpu_utilization() + b.memory_utilization();
                a_util.partial_cmp(&b_util).unwrap()
            })
            .unwrap()
    }

    /// Update node resource usage
    pub async fn update_node_usage(
        &self,
        node_id: &str,
        used_cpus: u32,
        used_memory_mb: u64,
        vm_count: u32,
    ) -> Result<()> {
        let mut nodes = self.nodes.write().await;

        if let Some(node) = nodes.get_mut(node_id) {
            node.used_cpus = used_cpus;
            node.used_memory_mb = used_memory_mb;
            node.vm_count = vm_count;
        }

        Ok(())
    }

    /// Get all nodes
    pub async fn get_nodes(&self) -> Vec<Node> {
        let nodes = self.nodes.read().await;
        nodes.values().cloned().collect()
    }

    /// Get node by ID
    pub async fn get_node(&self, node_id: &str) -> Option<Node> {
        let nodes = self.nodes.read().await;
        nodes.get(node_id).cloned()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_scheduler() {
        let scheduler = Scheduler::new(SchedulingStrategy::Balanced);

        // Register nodes
        let node1 = Node {
            id: "node1".to_string(),
            hostname: "node1.local".to_string(),
            total_cpus: 8,
            total_memory_mb: 16384,
            used_cpus: 2,
            used_memory_mb: 4096,
            vm_count: 2,
            is_healthy: true,
        };

        scheduler.register_node(node1).await.unwrap();

        // Schedule VM
        let request = VMRequest {
            name: "test-vm".to_string(),
            cpus: 2,
            memory_mb: 2048,
            affinity: None,
            anti_affinity: None,
        };

        let node_id = scheduler.schedule_vm(&request).await.unwrap();
        assert_eq!(node_id, "node1");
    }
}
