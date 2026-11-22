// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: LicenseRef-Zyvor-Production-2.0

use std::path::PathBuf;

use crate::error::Result;
use crate::util::{read_cgroup_file, write_cgroup_file};

/// Controller for the cgroup v2 freezer.
pub struct FreezerController {
    path: PathBuf,
}

impl FreezerController {
    pub fn new(path: PathBuf) -> Self {
        Self { path }
    }

    /// Freeze the cgroup (write 1 to cgroup.freeze).
    pub fn freeze(&self) -> Result<()> {
        write_cgroup_file(&self.path.join("cgroup.freeze"), "1")
    }

    /// Thaw (unfreeze) the cgroup (write 0 to cgroup.freeze).
    pub fn thaw(&self) -> Result<()> {
        write_cgroup_file(&self.path.join("cgroup.freeze"), "0")
    }

    /// Check if the cgroup is frozen by reading cgroup.events.
    pub fn is_frozen(&self) -> Result<bool> {
        let file = self.path.join("cgroup.events");
        let content = read_cgroup_file(&file)?;
        for line in content.lines() {
            let line = line.trim();
            if let Some(value) = line.strip_prefix("frozen ") {
                return Ok(value.trim() == "1");
            }
        }
        Ok(false)
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn test_parse_frozen_state() {
        let content = "populated 1\nfrozen 1\n";
        let frozen = content.lines().any(|l| {
            l.trim()
                .strip_prefix("frozen ")
                .is_some_and(|v| v.trim() == "1")
        });
        assert!(frozen);

        let content2 = "populated 1\nfrozen 0\n";
        let frozen2 = content2.lines().any(|l| {
            l.trim()
                .strip_prefix("frozen ")
                .is_some_and(|v| v.trim() == "1")
        });
        assert!(!frozen2);
    }
}
