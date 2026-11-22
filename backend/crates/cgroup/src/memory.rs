// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: LicenseRef-Zyvor-Production-2.0

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::error::Result;
use crate::pressure::PressureStats;
use crate::util::{
    lookup_key, read_cgroup_file, read_flat_keyed, read_u64, read_u64_or_max, write_cgroup_file,
};

/// Memory statistics from memory.stat.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryStats {
    pub anon: u64,
    pub file: u64,
    pub kernel: u64,
    pub kernel_stack: u64,
    pub pagetables: u64,
    pub sec_pagetables: u64,
    pub shmem: u64,
    pub sock: u64,
    pub slab: u64,
    pub slab_reclaimable: u64,
    pub slab_unreclaimable: u64,
    pub pgfault: u64,
    pub pgmajfault: u64,
    pub file_mapped: u64,
    pub file_dirty: u64,
    pub file_writeback: u64,
    pub anon_thp: u64,
    pub inactive_anon: u64,
    pub active_anon: u64,
    pub inactive_file: u64,
    pub active_file: u64,
    pub unevictable: u64,
    pub workingset_refault_anon: u64,
    pub workingset_refault_file: u64,
    pub workingset_activate_anon: u64,
    pub workingset_activate_file: u64,
    pub workingset_nodereclaim: u64,
    pub pgrefill: u64,
    pub pgscan: u64,
    pub pgsteal: u64,
    pub pgactivate: u64,
    pub pgdeactivate: u64,
    pub pglazyfree: u64,
    pub pglazyfreed: u64,
    pub thp_fault_alloc: u64,
    pub thp_collapse_alloc: u64,
}

/// Memory events from memory.events.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryEvents {
    pub low: u64,
    pub high: u64,
    pub max: u64,
    pub oom: u64,
    pub oom_kill: u64,
    pub oom_group_kill: u64,
}

/// Controller for the memory cgroup v2 subsystem.
pub struct MemoryController {
    path: PathBuf,
}

impl MemoryController {
    pub fn new(path: PathBuf) -> Self {
        Self { path }
    }

    /// Read memory.current (bytes).
    pub fn get_current(&self) -> Result<u64> {
        read_u64(&self.path.join("memory.current"))
    }

    /// Read memory.max (bytes, u64::MAX = unlimited).
    pub fn get_max(&self) -> Result<u64> {
        read_u64_or_max(&self.path.join("memory.max"))
    }

    /// Set memory.max (u64::MAX writes "max").
    pub fn set_max(&self, bytes: u64) -> Result<()> {
        let file = self.path.join("memory.max");
        let value = if bytes == u64::MAX {
            "max".to_string()
        } else {
            bytes.to_string()
        };
        write_cgroup_file(&file, &value)
    }

    /// Read memory.min (bytes).
    pub fn get_min(&self) -> Result<u64> {
        read_u64(&self.path.join("memory.min"))
    }

    /// Set memory.min.
    pub fn set_min(&self, bytes: u64) -> Result<()> {
        write_cgroup_file(&self.path.join("memory.min"), &bytes.to_string())
    }

    /// Read memory.low (bytes).
    pub fn get_low(&self) -> Result<u64> {
        read_u64(&self.path.join("memory.low"))
    }

    /// Set memory.low.
    pub fn set_low(&self, bytes: u64) -> Result<()> {
        write_cgroup_file(&self.path.join("memory.low"), &bytes.to_string())
    }

    /// Read memory.high (bytes, u64::MAX = unlimited).
    pub fn get_high(&self) -> Result<u64> {
        read_u64_or_max(&self.path.join("memory.high"))
    }

    /// Set memory.high (u64::MAX writes "max").
    pub fn set_high(&self, bytes: u64) -> Result<()> {
        let file = self.path.join("memory.high");
        let value = if bytes == u64::MAX {
            "max".to_string()
        } else {
            bytes.to_string()
        };
        write_cgroup_file(&file, &value)
    }

    /// Read memory.swap.max (bytes, u64::MAX = unlimited).
    pub fn get_swap_max(&self) -> Result<u64> {
        read_u64_or_max(&self.path.join("memory.swap.max"))
    }

    /// Set memory.swap.max (u64::MAX writes "max").
    pub fn set_swap_max(&self, bytes: u64) -> Result<()> {
        let file = self.path.join("memory.swap.max");
        let value = if bytes == u64::MAX {
            "max".to_string()
        } else {
            bytes.to_string()
        };
        write_cgroup_file(&file, &value)
    }

    /// Read memory.swap.current (bytes).
    pub fn get_swap_current(&self) -> Result<u64> {
        read_u64(&self.path.join("memory.swap.current"))
    }

    /// Read memory.stat.
    pub fn get_stat(&self) -> Result<MemoryStats> {
        let file = self.path.join("memory.stat");
        let entries = read_flat_keyed(&file)?;
        Ok(MemoryStats {
            anon: lookup_key(&entries, "anon"),
            file: lookup_key(&entries, "file"),
            kernel: lookup_key(&entries, "kernel"),
            kernel_stack: lookup_key(&entries, "kernel_stack"),
            pagetables: lookup_key(&entries, "pagetables"),
            sec_pagetables: lookup_key(&entries, "sec_pagetables"),
            shmem: lookup_key(&entries, "shmem"),
            sock: lookup_key(&entries, "sock"),
            slab: lookup_key(&entries, "slab"),
            slab_reclaimable: lookup_key(&entries, "slab_reclaimable"),
            slab_unreclaimable: lookup_key(&entries, "slab_unreclaimable"),
            pgfault: lookup_key(&entries, "pgfault"),
            pgmajfault: lookup_key(&entries, "pgmajfault"),
            file_mapped: lookup_key(&entries, "file_mapped"),
            file_dirty: lookup_key(&entries, "file_dirty"),
            file_writeback: lookup_key(&entries, "file_writeback"),
            anon_thp: lookup_key(&entries, "anon_thp"),
            inactive_anon: lookup_key(&entries, "inactive_anon"),
            active_anon: lookup_key(&entries, "active_anon"),
            inactive_file: lookup_key(&entries, "inactive_file"),
            active_file: lookup_key(&entries, "active_file"),
            unevictable: lookup_key(&entries, "unevictable"),
            workingset_refault_anon: lookup_key(&entries, "workingset_refault_anon"),
            workingset_refault_file: lookup_key(&entries, "workingset_refault_file"),
            workingset_activate_anon: lookup_key(&entries, "workingset_activate_anon"),
            workingset_activate_file: lookup_key(&entries, "workingset_activate_file"),
            workingset_nodereclaim: lookup_key(&entries, "workingset_nodereclaim"),
            pgrefill: lookup_key(&entries, "pgrefill"),
            pgscan: lookup_key(&entries, "pgscan"),
            pgsteal: lookup_key(&entries, "pgsteal"),
            pgactivate: lookup_key(&entries, "pgactivate"),
            pgdeactivate: lookup_key(&entries, "pgdeactivate"),
            pglazyfree: lookup_key(&entries, "pglazyfree"),
            pglazyfreed: lookup_key(&entries, "pglazyfreed"),
            thp_fault_alloc: lookup_key(&entries, "thp_fault_alloc"),
            thp_collapse_alloc: lookup_key(&entries, "thp_collapse_alloc"),
        })
    }

    /// Read memory.events.
    pub fn get_events(&self) -> Result<MemoryEvents> {
        let file = self.path.join("memory.events");
        let entries = read_flat_keyed(&file)?;
        Ok(MemoryEvents {
            low: lookup_key(&entries, "low"),
            high: lookup_key(&entries, "high"),
            max: lookup_key(&entries, "max"),
            oom: lookup_key(&entries, "oom"),
            oom_kill: lookup_key(&entries, "oom_kill"),
            oom_group_kill: lookup_key(&entries, "oom_group_kill"),
        })
    }

    /// Read memory.pressure (PSI).
    pub fn get_pressure(&self) -> Result<PressureStats> {
        crate::pressure::read_pressure(&self.path.join("memory.pressure"))
    }

    /// Read memory.oom.group (0 or 1).
    pub fn get_oom_group(&self) -> Result<bool> {
        let file = self.path.join("memory.oom.group");
        let content = read_cgroup_file(&file)?;
        Ok(content.trim() == "1")
    }

    /// Set memory.oom.group (true = 1, false = 0).
    pub fn set_oom_group(&self, enable: bool) -> Result<()> {
        let file = self.path.join("memory.oom.group");
        write_cgroup_file(&file, if enable { "1" } else { "0" })
    }
}
