// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: LicenseRef-Zyvor-Production-1.0

use std::path::PathBuf;

use crate::error::Result;
use crate::util::{read_u64, read_u64_or_max, write_cgroup_file};

/// Controller for the pids cgroup v2 subsystem.
pub struct PidsController {
    path: PathBuf,
}

impl PidsController {
    pub fn new(path: PathBuf) -> Self {
        Self { path }
    }

    /// Read pids.max (u64::MAX = "max" = unlimited).
    pub fn get_max(&self) -> Result<u64> {
        read_u64_or_max(&self.path.join("pids.max"))
    }

    /// Set pids.max (u64::MAX writes "max").
    pub fn set_max(&self, max: u64) -> Result<()> {
        let file = self.path.join("pids.max");
        let value = if max == u64::MAX {
            "max".to_string()
        } else {
            max.to_string()
        };
        write_cgroup_file(&file, &value)
    }

    /// Read pids.current.
    pub fn get_current(&self) -> Result<u64> {
        read_u64(&self.path.join("pids.current"))
    }
}
