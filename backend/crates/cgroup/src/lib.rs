// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: LicenseRef-Zyvor-Production-2.0

//! Unified cgroup v2 management for zyvor-fabricd.
//!
//! Provides a clean API for all cgroup v2 controllers with consistent path handling.
//!
//! # Usage
//!
//! ```no_run
//! use zyvor_fabric_cgroup::CgroupManager;
//!
//! let mgr = CgroupManager::for_machine("myvm").unwrap();
//! let mem_usage = mgr.memory().get_current().unwrap();
//! let cpu_stat = mgr.cpu().get_stat().unwrap();
//! ```

mod error;
mod util;

pub mod cpu;
pub mod cpuset;
pub mod freezer;
pub mod io;
pub mod memory;
pub mod pids;
pub mod pressure;

pub use cpu::{CpuController, CpuMax, CpuStat};
pub use cpuset::{format_set, parse_set, CpusetController};
pub use error::{CgroupError, Result};
pub use freezer::FreezerController;
pub use io::{DeviceId, IoController, IoMax, IoStat};
pub use memory::{MemoryController, MemoryEvents, MemoryStats};
pub use pids::PidsController;
pub use pressure::{PressureRecord, PressureStats};

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

const CGROUP_ROOT: &str = "/sys/fs/cgroup/machine.slice";

/// Resolved cgroup path.
#[derive(Debug, Clone)]
pub struct CgroupPath(PathBuf);

impl CgroupPath {
    /// Create a cgroup path for a machine scope: `machine-{name}.scope`.
    pub fn for_machine(name: &str) -> Self {
        let path = PathBuf::from(CGROUP_ROOT).join(format!("machine-{name}.scope"));
        Self(path)
    }

    /// Create a cgroup path from an arbitrary path.
    pub fn from_path(path: PathBuf) -> Self {
        Self(path)
    }

    /// Check whether the cgroup directory exists.
    pub fn exists(&self) -> bool {
        self.0.is_dir()
    }

    /// Get the underlying path.
    pub fn path(&self) -> &Path {
        &self.0
    }
}

/// Cgroup-level events from cgroup.events.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CgroupEvents {
    pub populated: bool,
    pub frozen: bool,
}

/// Entry point for cgroup v2 management.
///
/// Provides access to all controllers for a single cgroup.
pub struct CgroupManager {
    path: CgroupPath,
}

impl CgroupManager {
    /// Create a manager for a machine scope (validates path exists).
    pub fn for_machine(name: &str) -> Result<Self> {
        let path = CgroupPath::for_machine(name);
        if !path.exists() {
            return Err(CgroupError::NotFound(path.0));
        }
        Ok(Self { path })
    }

    /// Create a manager from an arbitrary path (validates path exists).
    pub fn from_path(path: PathBuf) -> Result<Self> {
        let cpath = CgroupPath::from_path(path);
        if !cpath.exists() {
            return Err(CgroupError::NotFound(cpath.0));
        }
        Ok(Self { path: cpath })
    }

    /// Get the cgroup path.
    pub fn path(&self) -> &Path {
        self.path.path()
    }

    /// CPU controller.
    pub fn cpu(&self) -> CpuController {
        CpuController::new(self.path.0.clone())
    }

    /// Memory controller.
    pub fn memory(&self) -> MemoryController {
        MemoryController::new(self.path.0.clone())
    }

    /// I/O controller.
    pub fn io(&self) -> IoController {
        IoController::new(self.path.0.clone())
    }

    /// PIDs controller.
    pub fn pids(&self) -> PidsController {
        PidsController::new(self.path.0.clone())
    }

    /// Cpuset controller.
    pub fn cpuset(&self) -> CpusetController {
        CpusetController::new(self.path.0.clone())
    }

    /// Freezer controller.
    pub fn freezer(&self) -> FreezerController {
        FreezerController::new(self.path.0.clone())
    }

    /// Read cgroup.controllers to list available controllers.
    pub fn available_controllers(&self) -> Result<Vec<String>> {
        let file = self.path.0.join("cgroup.controllers");
        let content = util::read_cgroup_file(&file)?;
        Ok(content.split_whitespace().map(String::from).collect())
    }

    /// Read cgroup.events.
    pub fn events(&self) -> Result<CgroupEvents> {
        let file = self.path.0.join("cgroup.events");
        let content = util::read_cgroup_file(&file)?;
        let mut populated = false;
        let mut frozen = false;
        for line in content.lines() {
            let line = line.trim();
            if let Some(val) = line.strip_prefix("populated ") {
                populated = val.trim() == "1";
            } else if let Some(val) = line.strip_prefix("frozen ") {
                frozen = val.trim() == "1";
            }
        }
        Ok(CgroupEvents { populated, frozen })
    }
}
