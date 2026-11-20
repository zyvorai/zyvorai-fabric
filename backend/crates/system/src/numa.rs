// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: LicenseRef-Zyvor-Production-1.0

use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum NumaError {
    #[error("Failed to read NUMA topology: {0}")]
    TopologyRead(String),

    #[error("Failed to parse NUMA info: {0}")]
    ParseError(String),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Invalid NUMA node: {0}")]
    InvalidNode(u32),

    #[error("NUMA not available on this system")]
    NotAvailable,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NumaTopology {
    pub nodes: Vec<NumaNode>,
    pub distances: Vec<Vec<u32>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NumaNode {
    pub id: u32,
    pub cpus: Vec<u32>,
    pub memory_total_mb: u64,
    pub memory_free_mb: u64,
    pub hugepages_2mb_total: u32,
    pub hugepages_2mb_free: u32,
    pub hugepages_1gb_total: u32,
    pub hugepages_1gb_free: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NumaPlacement {
    pub numa_node: u32,
    pub cpu_affinity: Vec<u32>,
}

impl NumaTopology {
    /// Detect NUMA topology from /sys/devices/system/node
    pub fn detect() -> Result<Self, NumaError> {
        let node_path = PathBuf::from("/sys/devices/system/node");

        if !node_path.exists() {
            return Err(NumaError::NotAvailable);
        }

        let mut node_ids = Vec::new();

        // Find all NUMA nodes
        for entry in fs::read_dir(&node_path)? {
            let entry = entry?;
            let name = entry.file_name();
            let name_lossy = name.to_string_lossy();
            let name_str = name_lossy.as_ref();

            if name_str.starts_with("node") && name_str[4..].chars().all(|c| c.is_numeric()) {
                if let Ok(node_id) = name_str[4..].parse::<u32>() {
                    node_ids.push(node_id);
                }
            }
        }

        if node_ids.is_empty() {
            return Err(NumaError::NotAvailable);
        }

        node_ids.sort();

        // Build detailed node info
        let mut nodes = Vec::new();
        for node_id in &node_ids {
            let node = Self::read_node_info(*node_id, &node_path)?;
            nodes.push(node);
        }

        // Read inter-node distances
        let distances = Self::read_distances(&node_ids, &node_path)?;

        Ok(Self { nodes, distances })
    }

    fn read_node_info(id: u32, base_path: &Path) -> Result<NumaNode, NumaError> {
        let node_path = base_path.join(format!("node{}", id));

        // Read CPU list
        let cpulist_path = node_path.join("cpulist");
        let cpulist = if cpulist_path.exists() {
            fs::read_to_string(&cpulist_path)?
        } else {
            String::new()
        };

        let cpus = Self::parse_cpulist(&cpulist);

        // Read memory info from meminfo
        let (memory_total, memory_free) = Self::read_meminfo(&node_path)?;

        // Read hugepage info
        let (hugepages_2mb_total, hugepages_2mb_free) =
            Self::read_hugepage_info(&node_path, "hugepages-2048kB")?;

        let (hugepages_1gb_total, hugepages_1gb_free) =
            Self::read_hugepage_info(&node_path, "hugepages-1048576kB")?;

        Ok(NumaNode {
            id,
            cpus,
            memory_total_mb: memory_total / 1024,
            memory_free_mb: memory_free / 1024,
            hugepages_2mb_total,
            hugepages_2mb_free,
            hugepages_1gb_total,
            hugepages_1gb_free,
        })
    }

    fn parse_cpulist(cpulist: &str) -> Vec<u32> {
        let mut cpus = Vec::new();

        for part in cpulist.trim().split(',') {
            if part.contains('-') {
                let range: Vec<&str> = part.split('-').collect();
                if range.len() == 2 {
                    if let (Ok(start), Ok(end)) = (range[0].parse::<u32>(), range[1].parse::<u32>())
                    {
                        for cpu in start..=end {
                            cpus.push(cpu);
                        }
                    }
                }
            } else if !part.is_empty() {
                if let Ok(cpu) = part.parse::<u32>() {
                    cpus.push(cpu);
                }
            }
        }

        cpus
    }

    fn read_meminfo(node_path: &Path) -> Result<(u64, u64), NumaError> {
        let meminfo_path = node_path.join("meminfo");

        if !meminfo_path.exists() {
            return Ok((0, 0));
        }

        let content = fs::read_to_string(&meminfo_path)?;

        let mut total = 0u64;
        let mut free = 0u64;

        for line in content.lines() {
            if line.contains("MemTotal:") {
                total = Self::parse_memory_line(line);
            } else if line.contains("MemFree:") {
                free = Self::parse_memory_line(line);
            }
        }

        Ok((total, free))
    }

    fn parse_memory_line(line: &str) -> u64 {
        // Format: "Node X MemTotal:       12345678 kB"
        line.split_whitespace()
            .rev()
            .nth(1)
            .and_then(|s| s.parse().ok())
            .unwrap_or(0)
    }

    fn read_hugepage_info(node_path: &Path, size: &str) -> Result<(u32, u32), NumaError> {
        let hugepage_path = node_path.join("hugepages").join(size);

        if !hugepage_path.exists() {
            return Ok((0, 0));
        }

        let total = Self::read_u32_file(&hugepage_path.join("nr_hugepages")).unwrap_or(0);
        let free = Self::read_u32_file(&hugepage_path.join("free_hugepages")).unwrap_or(0);

        Ok((total, free))
    }

    fn read_u32_file(path: &PathBuf) -> Result<u32, NumaError> {
        if !path.exists() {
            return Ok(0);
        }

        let content = fs::read_to_string(path)?;
        content
            .trim()
            .parse()
            .map_err(|e| NumaError::ParseError(format!("{}: {}", path.display(), e)))
    }

    fn read_distances(node_ids: &[u32], base_path: &Path) -> Result<Vec<Vec<u32>>, NumaError> {
        let num_nodes = node_ids.len();
        let mut distances = vec![vec![0u32; num_nodes]; num_nodes];

        for (i, &from_node) in node_ids.iter().enumerate() {
            let distance_path = base_path
                .join(format!("node{}", from_node))
                .join("distance");

            if !distance_path.exists() {
                // If distance file doesn't exist, assume uniform distance
                for (j, slot) in distances[i].iter_mut().enumerate() {
                    *slot = if i == j { 10 } else { 20 };
                }
                continue;
            }

            let content = fs::read_to_string(&distance_path)?;
            let dist_values: Vec<u32> = content
                .split_whitespace()
                .filter_map(|s| s.parse().ok())
                .collect();

            for (j, &dist) in dist_values.iter().enumerate() {
                if j < num_nodes {
                    distances[i][j] = dist;
                }
            }
        }

        Ok(distances)
    }

    /// Find the best NUMA node for a VM with given requirements
    pub fn find_best_node(&self, memory_mb: u64, cpus: u32) -> Option<u32> {
        self.nodes
            .iter()
            .filter(|n| n.memory_free_mb >= memory_mb && n.cpus.len() >= cpus as usize)
            .max_by_key(|n| n.memory_free_mb)
            .map(|n| n.id)
    }

    /// Get node by ID
    pub fn get_node(&self, node_id: u32) -> Option<&NumaNode> {
        self.nodes.iter().find(|n| n.id == node_id)
    }

    /// Get distance between two nodes
    pub fn get_distance(&self, from: u32, to: u32) -> Option<u32> {
        if from >= self.nodes.len() as u32 || to >= self.nodes.len() as u32 {
            return None;
        }

        Some(self.distances[from as usize][to as usize])
    }

    /// Check if the system has NUMA support
    pub fn is_numa_available() -> bool {
        PathBuf::from("/sys/devices/system/node").exists()
    }

    /// Get total system memory across all nodes
    pub fn total_memory_mb(&self) -> u64 {
        self.nodes.iter().map(|n| n.memory_total_mb).sum()
    }

    /// Get total free memory across all nodes
    pub fn free_memory_mb(&self) -> u64 {
        self.nodes.iter().map(|n| n.memory_free_mb).sum()
    }

    /// Create a placement recommendation
    pub fn recommend_placement(&self, memory_mb: u64, cpus: u32) -> Option<NumaPlacement> {
        let node_id = self.find_best_node(memory_mb, cpus)?;
        let node = self.get_node(node_id)?;

        Some(NumaPlacement {
            numa_node: node_id,
            cpu_affinity: node.cpus.clone(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_cpulist() {
        let cpus = NumaTopology::parse_cpulist("0-3,8,10-12");
        assert_eq!(cpus, vec![0, 1, 2, 3, 8, 10, 11, 12]);
    }

    #[test]
    fn test_parse_memory_line() {
        let line = "Node 0 MemTotal:       16384000 kB";
        let mem = NumaTopology::parse_memory_line(line);
        assert_eq!(mem, 16384000);
    }

    #[test]
    fn test_numa_availability() {
        // This will vary based on the system
        let available = NumaTopology::is_numa_available();
        println!("NUMA available: {}", available);
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn test_detect_numa() {
        // Only run if NUMA is available
        if NumaTopology::is_numa_available() {
            let result = NumaTopology::detect();
            if let Ok(topology) = result {
                assert!(!topology.nodes.is_empty());
                println!("Detected {} NUMA nodes", topology.nodes.len());
            }
        }
    }
}
