// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: LicenseRef-Zyvor-Production-2.0

use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;
use thiserror::Error;
use zyvor_fabric_cgroup::CgroupManager;

#[derive(Debug, Error)]
pub enum MemoryError {
    #[error("Failed to set memory limit: {0}")]
    SetLimitFailed(String),

    #[error("Failed to read memory stats: {0}")]
    ReadStatsFailed(String),

    #[error("cgroup not found: {0}")]
    CgroupNotFound(String),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Parse error: {0}")]
    ParseError(String),

    #[error("Hugepage allocation failed: {0}")]
    HugepageAllocationFailed(String),
}

impl From<zyvor_fabric_cgroup::CgroupError> for MemoryError {
    fn from(e: zyvor_fabric_cgroup::CgroupError) -> Self {
        match &e {
            zyvor_fabric_cgroup::CgroupError::NotFound(_) => {
                MemoryError::CgroupNotFound(e.to_string())
            }
            zyvor_fabric_cgroup::CgroupError::ReadFailed { .. } => {
                MemoryError::ReadStatsFailed(e.to_string())
            }
            zyvor_fabric_cgroup::CgroupError::WriteFailed { .. } => {
                MemoryError::SetLimitFailed(e.to_string())
            }
            zyvor_fabric_cgroup::CgroupError::ParseError { .. } => {
                MemoryError::ParseError(e.to_string())
            }
            _ => MemoryError::SetLimitFailed(e.to_string()),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum HugepageSize {
    Size2MB,
    Size1GB,
}

impl HugepageSize {
    pub fn as_kb(&self) -> u64 {
        match self {
            HugepageSize::Size2MB => 2048,
            HugepageSize::Size1GB => 1048576,
        }
    }

    pub fn as_str(&self) -> &str {
        match self {
            HugepageSize::Size2MB => "hugepages-2048kB",
            HugepageSize::Size1GB => "hugepages-1048576kB",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum OvercommitPolicy {
    None,         // No overcommit, 1:1 allocation
    Conservative, // 1.5x overcommit
    Aggressive,   // 2x overcommit
}

impl OvercommitPolicy {
    pub fn multiplier(&self) -> f64 {
        match self {
            OvercommitPolicy::None => 1.0,
            OvercommitPolicy::Conservative => 1.5,
            OvercommitPolicy::Aggressive => 2.0,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryStats {
    pub current_bytes: u64,
    pub max_bytes: u64,
    pub swap_current_bytes: u64,
    pub swap_max_bytes: u64,
    pub limit_bytes: u64,
    pub usage_percent: f64,
}

pub struct MemoryController {
    vm_name: String,
    mgr: Option<CgroupManager>,
}

impl MemoryController {
    pub fn new(vm_name: &str) -> Self {
        let mgr = CgroupManager::for_machine(vm_name).ok();
        Self {
            vm_name: vm_name.to_string(),
            mgr,
        }
    }

    /// Build a controller from an already-resolved cgroup path -- for
    /// drivers (FluxVM) whose real cgroup path isn't derivable from the
    /// VM name alone. See `VmDriver::get_cgroup_path`.
    pub fn for_path(vm_name: &str, cgroup_path: &std::path::Path) -> Self {
        let mgr = CgroupManager::from_path(cgroup_path.to_path_buf()).ok();
        Self {
            vm_name: vm_name.to_string(),
            mgr,
        }
    }

    /// Check if cgroup exists
    pub fn exists(&self) -> bool {
        self.mgr.is_some()
    }

    fn manager(&self) -> Result<&CgroupManager, MemoryError> {
        self.mgr
            .as_ref()
            .ok_or_else(|| MemoryError::CgroupNotFound(self.vm_name.clone()))
    }

    /// Set memory limit in bytes
    pub fn set_limit(&self, limit_bytes: u64) -> Result<(), MemoryError> {
        self.manager()?.memory().set_max(limit_bytes)?;
        Ok(())
    }

    /// Set swap limit in bytes
    pub fn set_swap_limit(&self, limit_bytes: u64) -> Result<(), MemoryError> {
        self.manager()?.memory().set_swap_max(limit_bytes)?;
        Ok(())
    }

    /// Disable swap for this VM
    pub fn disable_swap(&self) -> Result<(), MemoryError> {
        self.set_swap_limit(0)
    }

    /// Get current memory usage
    pub fn get_current_usage(&self) -> Result<u64, MemoryError> {
        Ok(self.manager()?.memory().get_current()?)
    }

    /// Get memory limit
    pub fn get_limit(&self) -> Result<u64, MemoryError> {
        Ok(self.manager()?.memory().get_max()?)
    }

    /// Get swap usage
    pub fn get_swap_usage(&self) -> Result<u64, MemoryError> {
        match self.manager()?.memory().get_swap_current() {
            Ok(v) => Ok(v),
            Err(_) => Ok(0),
        }
    }

    /// Get comprehensive memory statistics
    pub fn get_stats(&self) -> Result<MemoryStats, MemoryError> {
        let current_bytes = self.get_current_usage()?;
        let limit_bytes = self.get_limit()?;
        let swap_current_bytes = self.get_swap_usage()?;

        let usage_percent = if limit_bytes == u64::MAX {
            0.0
        } else {
            (current_bytes as f64 / limit_bytes as f64) * 100.0
        };

        let swap_max_bytes = self.manager()?.memory().get_swap_max().unwrap_or(0);

        Ok(MemoryStats {
            current_bytes,
            max_bytes: limit_bytes,
            swap_current_bytes,
            swap_max_bytes,
            limit_bytes,
            usage_percent,
        })
    }

    /// Enable OOM killer for this cgroup
    pub fn enable_oom_killer(&self, enable: bool) -> Result<(), MemoryError> {
        let mgr = self.manager()?;
        // Not all kernels support memory.oom.group — silently ignore if unavailable
        let _ = mgr.memory().set_oom_group(enable);
        Ok(())
    }
}

/// Hugepage management
pub struct HugepageManager;

impl HugepageManager {
    /// Allocate hugepages
    pub fn allocate(size: HugepageSize, count: u32) -> Result<(), MemoryError> {
        let path = PathBuf::from(format!("/sys/kernel/mm/hugepages/{}", size.as_str()));

        if !path.exists() {
            return Err(MemoryError::HugepageAllocationFailed(format!(
                "Hugepage size {} not supported",
                size.as_str()
            )));
        }

        let nr_path = path.join("nr_hugepages");
        let current: u32 = fs::read_to_string(&nr_path)?
            .trim()
            .parse()
            .map_err(|e| MemoryError::ParseError(format!("Failed to parse nr_hugepages: {}", e)))?;

        let needed = current + count;

        fs::write(&nr_path, needed.to_string())
            .map_err(|e| MemoryError::HugepageAllocationFailed(e.to_string()))?;

        Ok(())
    }

    /// Get hugepage statistics
    pub fn get_stats(size: HugepageSize) -> Result<HugepageStats, MemoryError> {
        let path = PathBuf::from(format!("/sys/kernel/mm/hugepages/{}", size.as_str()));

        if !path.exists() {
            return Ok(HugepageStats {
                size,
                total: 0,
                free: 0,
                reserved: 0,
                surplus: 0,
            });
        }

        let total = Self::read_u32(&path.join("nr_hugepages"))?;
        let free = Self::read_u32(&path.join("free_hugepages"))?;
        let reserved = Self::read_u32(&path.join("resv_hugepages"))?;
        let surplus = Self::read_u32(&path.join("surplus_hugepages"))?;

        Ok(HugepageStats {
            size,
            total,
            free,
            reserved,
            surplus,
        })
    }

    fn read_u32(path: &PathBuf) -> Result<u32, MemoryError> {
        if !path.exists() {
            return Ok(0);
        }

        let content = fs::read_to_string(path)?;
        content.trim().parse().map_err(|e| {
            MemoryError::ParseError(format!("Failed to parse {}: {}", path.display(), e))
        })
    }

    /// Get total system memory info
    pub fn get_system_memory() -> Result<SystemMemory, MemoryError> {
        let meminfo = fs::read_to_string("/proc/meminfo")?;

        let mut total_kb = 0u64;
        let mut free_kb = 0u64;
        let mut available_kb = 0u64;
        let mut buffers_kb = 0u64;
        let mut cached_kb = 0u64;

        for line in meminfo.lines() {
            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.len() < 2 {
                continue;
            }

            let value: u64 = parts[1].parse().unwrap_or(0);

            match parts[0] {
                "MemTotal:" => total_kb = value,
                "MemFree:" => free_kb = value,
                "MemAvailable:" => available_kb = value,
                "Buffers:" => buffers_kb = value,
                "Cached:" => cached_kb = value,
                _ => {}
            }
        }

        Ok(SystemMemory {
            total_kb,
            free_kb,
            available_kb,
            buffers_kb,
            cached_kb,
        })
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HugepageStats {
    pub size: HugepageSize,
    pub total: u32,
    pub free: u32,
    pub reserved: u32,
    pub surplus: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SystemMemory {
    pub total_kb: u64,
    pub free_kb: u64,
    pub available_kb: u64,
    pub buffers_kb: u64,
    pub cached_kb: u64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_hugepage_size() {
        assert_eq!(HugepageSize::Size2MB.as_kb(), 2048);
        assert_eq!(HugepageSize::Size1GB.as_kb(), 1048576);
    }

    #[test]
    fn test_overcommit_multiplier() {
        assert_eq!(OvercommitPolicy::None.multiplier(), 1.0);
        assert_eq!(OvercommitPolicy::Conservative.multiplier(), 1.5);
        assert_eq!(OvercommitPolicy::Aggressive.multiplier(), 2.0);
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn test_system_memory() {
        let result = HugepageManager::get_system_memory();
        if let Ok(mem) = result {
            assert!(mem.total_kb > 0);
            println!("Total memory: {} MB", mem.total_kb / 1024);
        }
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn test_hugepage_stats() {
        let result = HugepageManager::get_stats(HugepageSize::Size2MB);
        if let Ok(stats) = result {
            println!("2MB hugepages: total={}, free={}", stats.total, stats.free);
        }
    }
}
