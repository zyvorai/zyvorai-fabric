// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: LicenseRef-Zyvor-Production-2.0

use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use zyvor_fabric_system::{CpuTopology, NumaTopology};

use crate::firmware::Firmware;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VmConfig {
    pub name: String,
    pub cpu: CpuConfig,
    pub memory: MemoryConfig,
    pub firmware: Firmware,
    pub disks: Vec<DiskConfig>,
    pub network: Vec<NetworkConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CpuConfig {
    pub count: u32,
    pub pinning: Option<CpuPinning>,
    pub shares: u32,                // CPU shares (relative weight, default 1024)
    pub quota: Option<u64>,         // CPU quota in microseconds per period
    pub period: u64,                // CPU period (default 100000 = 100ms)
    pub affinity: Option<Vec<u32>>, // Physical CPU cores
}

impl Default for CpuConfig {
    fn default() -> Self {
        Self {
            count: 1,
            pinning: None,
            shares: 1024,
            quota: None,
            period: 100000, // 100ms
            affinity: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum CpuPinning {
    Auto,                  // Let scheduler decide
    Explicit(Vec<CpuPin>), // Manual vCPU to physical CPU mapping
    NumaNode(u32),         // Pin to all CPUs in NUMA node
    Socket(u32),           // Pin to all CPUs in socket
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CpuPin {
    pub vcpu_id: u32,
    pub physical_cpu: u32,
}

impl CpuPinning {
    /// Validate pinning configuration against system topology
    pub fn validate(&self, topology: &CpuTopology) -> Result<(), String> {
        match self {
            CpuPinning::Auto => Ok(()),

            CpuPinning::Explicit(pins) => {
                for pin in pins {
                    if pin.physical_cpu >= topology.total_cpus {
                        return Err(format!(
                            "Physical CPU {} does not exist (max: {})",
                            pin.physical_cpu,
                            topology.total_cpus - 1
                        ));
                    }

                    if !topology.is_cpu_online(pin.physical_cpu) {
                        return Err(format!("Physical CPU {} is offline", pin.physical_cpu));
                    }
                }
                Ok(())
            }

            CpuPinning::NumaNode(node_id) => {
                let numa =
                    NumaTopology::detect().map_err(|e| format!("NUMA not available: {}", e))?;

                if numa.get_node(*node_id).is_none() {
                    return Err(format!("NUMA node {} does not exist", node_id));
                }

                Ok(())
            }

            CpuPinning::Socket(socket_id) => {
                if *socket_id >= topology.sockets {
                    return Err(format!(
                        "Socket {} does not exist (max: {})",
                        socket_id,
                        topology.sockets - 1
                    ));
                }
                Ok(())
            }
        }
    }

    /// Get list of physical CPUs for this pinning configuration
    pub fn get_cpu_list(&self, topology: &CpuTopology) -> Vec<u32> {
        match self {
            CpuPinning::Auto => Vec::new(),

            CpuPinning::Explicit(pins) => pins.iter().map(|p| p.physical_cpu).collect(),

            CpuPinning::NumaNode(node_id) => topology.get_cpus_for_numa_node(*node_id),

            CpuPinning::Socket(socket_id) => topology.get_cpus_for_socket(*socket_id),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryConfig {
    pub size_mb: u64,
    pub max_mb: Option<u64>, // Hard limit (for ballooning)
    pub balloon: bool,       // Enable memory ballooning
    pub hugepages: Option<HugepageSize>,
    pub numa_node: Option<u32>, // NUMA placement
    pub swap_enabled: bool,     // Allow swap
}

impl Default for MemoryConfig {
    fn default() -> Self {
        Self {
            size_mb: 1024,
            max_mb: None,
            balloon: false,
            hugepages: None,
            numa_node: None,
            swap_enabled: true,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum HugepageSize {
    Size2MB,
    Size1GB,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiskConfig {
    pub path: PathBuf,
    pub format: DiskFormat,
    pub bus: DiskBus,
    pub readonly: bool,
    pub bootindex: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum DiskFormat {
    Qcow2,
    Raw,
    Vmdk,
    Vdi,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum DiskBus {
    Virtio,
    Sata,
    Ide,
    Scsi,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetworkConfig {
    pub interface: String,
    pub bridge: Option<String>,
    pub mac_address: Option<String>,
    pub model: NetworkModel,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum NetworkModel {
    Virtio,
    E1000,
    Rtl8139,
}

impl VmConfig {
    pub fn new(name: String) -> Self {
        Self {
            name,
            cpu: CpuConfig::default(),
            memory: MemoryConfig::default(),
            firmware: Firmware::BIOS,
            disks: Vec::new(),
            network: Vec::new(),
        }
    }

    /// Enable UEFI firmware
    pub fn with_uefi(mut self, secure_boot: bool) -> Self {
        self.firmware = Firmware::UEFI { secure_boot };
        self
    }

    /// Set CPU count and optional pinning
    pub fn with_cpu(mut self, count: u32, pinning: Option<CpuPinning>) -> Self {
        self.cpu.count = count;
        self.cpu.pinning = pinning;
        self
    }

    /// Set memory size and configuration
    pub fn with_memory(mut self, size_mb: u64, hugepages: Option<HugepageSize>) -> Self {
        self.memory.size_mb = size_mb;
        self.memory.hugepages = hugepages;
        self
    }

    /// Add a disk
    pub fn add_disk(mut self, disk: DiskConfig) -> Self {
        self.disks.push(disk);
        self
    }

    /// Add a network interface
    pub fn add_network(mut self, network: NetworkConfig) -> Self {
        self.network.push(network);
        self
    }

    /// Validate the entire configuration
    pub fn validate(&self) -> Result<(), String> {
        // Validate CPU configuration
        if self.cpu.count == 0 {
            return Err("CPU count must be at least 1".to_string());
        }

        if let Some(ref pinning) = self.cpu.pinning {
            let topology = CpuTopology::detect()
                .map_err(|e| format!("Failed to detect CPU topology: {}", e))?;

            pinning.validate(&topology)?;

            // Check if we have enough CPUs for pinning
            if let CpuPinning::Explicit(pins) = pinning {
                if pins.len() < self.cpu.count as usize {
                    return Err(format!(
                        "Not enough CPU pins ({}) for {} vCPUs",
                        pins.len(),
                        self.cpu.count
                    ));
                }
            }
        }

        // Validate memory configuration
        if self.memory.size_mb == 0 {
            return Err("Memory size must be greater than 0".to_string());
        }

        if let Some(max_mb) = self.memory.max_mb {
            if max_mb < self.memory.size_mb {
                return Err("Max memory must be greater than or equal to size".to_string());
            }
        }

        // Validate NUMA placement if specified
        if let Some(numa_node) = self.memory.numa_node {
            let numa = NumaTopology::detect().map_err(|e| format!("NUMA not available: {}", e))?;

            if numa.get_node(numa_node).is_none() {
                return Err(format!("NUMA node {} does not exist", numa_node));
            }
        }

        // Validate disks
        if self.disks.is_empty() {
            return Err("At least one disk is required".to_string());
        }

        for disk in &self.disks {
            if !disk.path.exists() {
                return Err(format!("Disk image not found: {}", disk.path.display()));
            }
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_cpu_config() {
        let cpu = CpuConfig::default();
        assert_eq!(cpu.count, 1);
        assert_eq!(cpu.shares, 1024);
        assert_eq!(cpu.period, 100000);
    }

    #[test]
    fn test_default_memory_config() {
        let mem = MemoryConfig::default();
        assert_eq!(mem.size_mb, 1024);
        assert!(!mem.balloon);
        assert!(mem.swap_enabled);
    }

    #[test]
    fn test_vm_config_builder() {
        let config = VmConfig::new("test-vm".to_string())
            .with_cpu(4, None)
            .with_memory(4096, None)
            .with_uefi(false);

        assert_eq!(config.name, "test-vm");
        assert_eq!(config.cpu.count, 4);
        assert_eq!(config.memory.size_mb, 4096);
        assert_eq!(config.firmware, Firmware::UEFI { secure_boot: false });
    }

    #[test]
    fn test_cpu_pinning_explicit() {
        let pinning = CpuPinning::Explicit(vec![
            CpuPin {
                vcpu_id: 0,
                physical_cpu: 0,
            },
            CpuPin {
                vcpu_id: 1,
                physical_cpu: 2,
            },
        ]);

        assert_eq!(
            pinning,
            CpuPinning::Explicit(vec![
                CpuPin {
                    vcpu_id: 0,
                    physical_cpu: 0
                },
                CpuPin {
                    vcpu_id: 1,
                    physical_cpu: 2
                },
            ])
        );
    }

    #[test]
    fn test_validate_basic_config() {
        let mut config = VmConfig::new("test".to_string());
        config.cpu.count = 2;
        config.memory.size_mb = 2048;

        // This will fail because no disks
        let result = config.validate();
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("disk"));
    }
}
