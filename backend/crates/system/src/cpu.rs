// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: LicenseRef-Zyvor-Production-1.0

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum CpuError {
    #[error("Failed to read CPU topology: {0}")]
    TopologyRead(String),

    #[error("Failed to parse CPU info: {0}")]
    ParseError(String),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Invalid CPU ID: {0}")]
    InvalidCpuId(u32),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CpuTopology {
    pub total_cpus: u32,
    pub sockets: u32,
    pub cores_per_socket: u32,
    pub threads_per_core: u32,
    pub cpus: Vec<CpuCore>,
    pub online_cpus: Vec<u32>,
    pub offline_cpus: Vec<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CpuCore {
    pub id: u32,
    pub socket_id: u32,
    pub core_id: u32,
    pub thread_id: u32,
    pub online: bool,
    pub numa_node: Option<u32>,
}

impl CpuTopology {
    /// Detect CPU topology from /sys/devices/system/cpu
    pub fn detect() -> Result<Self, CpuError> {
        let cpu_path = PathBuf::from("/sys/devices/system/cpu");

        if !cpu_path.exists() {
            return Err(CpuError::TopologyRead("CPU sysfs not found".to_string()));
        }

        // Count total CPUs
        let mut cpu_ids = Vec::new();
        for entry in fs::read_dir(&cpu_path)? {
            let entry = entry?;
            let name = entry.file_name();
            let name_lossy = name.to_string_lossy();
            let name_str = name_lossy.as_ref();

            if name_str.starts_with("cpu") && name_str[3..].chars().all(|c| c.is_numeric()) {
                if let Ok(cpu_id) = name_str[3..].parse::<u32>() {
                    cpu_ids.push(cpu_id);
                }
            }
        }

        cpu_ids.sort();

        let total_cpus = cpu_ids.len() as u32;

        // Read online CPUs
        let online_cpus = Self::read_cpu_list(&cpu_path.join("online"))?;
        let offline_cpus = Self::read_cpu_list(&cpu_path.join("offline"))?;

        // Build detailed CPU info
        let mut cpus = Vec::new();
        let mut socket_ids = Vec::new();
        let mut cores_per_socket_map: HashMap<u32, Vec<u32>> = HashMap::new();

        for cpu_id in &cpu_ids {
            let cpu_info = Self::read_cpu_info(*cpu_id, &cpu_path)?;

            if !socket_ids.contains(&cpu_info.socket_id) {
                socket_ids.push(cpu_info.socket_id);
            }

            cores_per_socket_map
                .entry(cpu_info.socket_id)
                .or_default()
                .push(cpu_info.core_id);

            cpus.push(cpu_info);
        }

        let sockets = socket_ids.len() as u32;

        // Calculate cores per socket (use the max)
        let cores_per_socket = cores_per_socket_map
            .values()
            .map(|cores| {
                let mut unique_cores = cores.clone();
                unique_cores.sort();
                unique_cores.dedup();
                unique_cores.len() as u32
            })
            .max()
            .unwrap_or(1);

        let threads_per_core = if cores_per_socket > 0 && sockets > 0 {
            total_cpus / (sockets * cores_per_socket)
        } else {
            1
        };

        Ok(Self {
            total_cpus,
            sockets,
            cores_per_socket,
            threads_per_core,
            cpus,
            online_cpus,
            offline_cpus,
        })
    }

    fn read_cpu_info(cpu_id: u32, base_path: &Path) -> Result<CpuCore, CpuError> {
        let cpu_path = base_path.join(format!("cpu{}", cpu_id));

        // Read topology info
        let topology_path = cpu_path.join("topology");

        let physical_package_id =
            Self::read_u32(&topology_path.join("physical_package_id")).unwrap_or(0);

        let core_id = Self::read_u32(&topology_path.join("core_id")).unwrap_or(0);

        // Determine thread ID (simple heuristic)
        let thread_siblings_list =
            Self::read_file(&topology_path.join("thread_siblings_list")).unwrap_or_default();
        let siblings: Vec<u32> = Self::parse_cpu_list_string(&thread_siblings_list);
        let thread_id = siblings.iter().position(|&id| id == cpu_id).unwrap_or(0) as u32;

        // Check if CPU is online
        let online_path = cpu_path.join("online");
        let online = if online_path.exists() {
            Self::read_u32(&online_path).unwrap_or(1) == 1
        } else {
            true // CPU0 doesn't have online file, always online
        };

        // Read NUMA node
        let numa_node = if let Ok(node_str) = Self::read_file(&cpu_path.join("node")) {
            node_str
                .trim()
                .strip_prefix("node")
                .and_then(|s| s.parse().ok())
        } else {
            None
        };

        Ok(CpuCore {
            id: cpu_id,
            socket_id: physical_package_id,
            core_id,
            thread_id,
            online,
            numa_node,
        })
    }

    fn read_cpu_list(path: &PathBuf) -> Result<Vec<u32>, CpuError> {
        if !path.exists() {
            return Ok(Vec::new());
        }

        let content = fs::read_to_string(path)?;
        Ok(Self::parse_cpu_list_string(&content))
    }

    fn parse_cpu_list_string(content: &str) -> Vec<u32> {
        let mut cpus = Vec::new();

        for part in content.trim().split(',') {
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

    fn read_u32(path: &PathBuf) -> Result<u32, CpuError> {
        let content = fs::read_to_string(path)?;
        content
            .trim()
            .parse()
            .map_err(|e| CpuError::ParseError(format!("{}: {}", path.display(), e)))
    }

    fn read_file(path: &PathBuf) -> Result<String, CpuError> {
        Ok(fs::read_to_string(path)?)
    }

    /// Get CPUs for a specific socket
    pub fn get_cpus_for_socket(&self, socket_id: u32) -> Vec<u32> {
        self.cpus
            .iter()
            .filter(|cpu| cpu.socket_id == socket_id)
            .map(|cpu| cpu.id)
            .collect()
    }

    /// Get CPUs for a specific core
    pub fn get_cpus_for_core(&self, socket_id: u32, core_id: u32) -> Vec<u32> {
        self.cpus
            .iter()
            .filter(|cpu| cpu.socket_id == socket_id && cpu.core_id == core_id)
            .map(|cpu| cpu.id)
            .collect()
    }

    /// Get CPUs for a specific NUMA node
    pub fn get_cpus_for_numa_node(&self, node_id: u32) -> Vec<u32> {
        self.cpus
            .iter()
            .filter(|cpu| cpu.numa_node == Some(node_id))
            .map(|cpu| cpu.id)
            .collect()
    }

    /// Check if a CPU is online
    pub fn is_cpu_online(&self, cpu_id: u32) -> bool {
        self.online_cpus.contains(&cpu_id)
    }

    /// Validate CPU affinity list
    pub fn validate_affinity(&self, cpu_list: &[u32]) -> Result<(), CpuError> {
        for cpu_id in cpu_list {
            if *cpu_id >= self.total_cpus {
                return Err(CpuError::InvalidCpuId(*cpu_id));
            }
            if !self.is_cpu_online(*cpu_id) {
                return Err(CpuError::InvalidCpuId(*cpu_id));
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_cpu_list() {
        let cpus = CpuTopology::parse_cpu_list_string("0-3,8,10-12");
        assert_eq!(cpus, vec![0, 1, 2, 3, 8, 10, 11, 12]);
    }

    #[test]
    fn test_parse_single_cpu() {
        let cpus = CpuTopology::parse_cpu_list_string("5");
        assert_eq!(cpus, vec![5]);
    }

    #[test]
    fn test_parse_empty() {
        let cpus = CpuTopology::parse_cpu_list_string("");
        assert_eq!(cpus, Vec::<u32>::new());
    }

    // Only run topology detection on Linux
    #[cfg(target_os = "linux")]
    #[test]
    fn test_detect_topology() {
        let result = CpuTopology::detect();

        // This test might fail in CI without proper /sys
        if let Ok(topology) = result {
            assert!(topology.total_cpus > 0);
            assert!(topology.sockets > 0);
        }
    }
}
