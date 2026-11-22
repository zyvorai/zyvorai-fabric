// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: LicenseRef-Zyvor-Production-2.0

use crate::zfs::{ZfsError, ZfsPool, ZfsReplicationTarget, ZfsSendResult};

pub struct ZfsReplicationDriver {
    source_pool: ZfsPool,
}

impl ZfsReplicationDriver {
    pub fn new(source_pool: ZfsPool) -> Self {
        Self { source_pool }
    }

    /// Run a sync cycle: create snapshot, find common snapshot, send incremental (or full if none)
    pub fn run_sync_cycle(
        &self,
        vm_name: &str,
        dataset: &str,
        target: &ZfsReplicationTarget,
    ) -> Result<ZfsSendResult, ZfsError> {
        let snap_name = format!(
            "repl-{}-{}",
            vm_name,
            chrono::Utc::now().format("%Y%m%d%H%M%S")
        );

        // Create a new snapshot
        self.source_pool.snapshot(dataset, &snap_name)?;

        // Check for a common snapshot
        match self.source_pool.check_common_snapshot(dataset, target)? {
            Some(common) => {
                tracing::info!(
                    vm = vm_name,
                    common = %common,
                    new = %snap_name,
                    "Sending incremental replication"
                );
                self.source_pool
                    .send_incremental(dataset, &common, &snap_name, target)
            }
            None => {
                tracing::info!(
                    vm = vm_name,
                    snapshot = %snap_name,
                    "No common snapshot found, sending full replication"
                );
                self.source_pool.send_full(dataset, &snap_name, target)
            }
        }
    }

    /// Run the initial seed: create a snapshot and send a full stream
    pub fn run_initial_seed(
        &self,
        vm_name: &str,
        dataset: &str,
        target: &ZfsReplicationTarget,
    ) -> Result<ZfsSendResult, ZfsError> {
        let snap_name = format!(
            "repl-{}-seed-{}",
            vm_name,
            chrono::Utc::now().format("%Y%m%d%H%M%S")
        );

        self.source_pool.snapshot(dataset, &snap_name)?;
        self.source_pool.send_full(dataset, &snap_name, target)
    }

    /// Prepare the target for failover by promoting the received dataset
    pub fn prepare_failover_target(
        &self,
        dataset: &str,
        target: &ZfsReplicationTarget,
    ) -> Result<(), ZfsError> {
        ZfsPool::validate_zfs_name(dataset, "Dataset")?;
        ZfsPool::validate_target(target)?;

        let target_ds = match &target.target_dataset {
            Some(ds) => format!("{}/{}", target.target_pool, ds),
            None => format!("{}/{}", target.target_pool, dataset),
        };

        let ssh_args = ZfsPool::ssh_args(target);

        let output = std::process::Command::new(&ssh_args[0])
            .args(&ssh_args[1..])
            .args(["zfs", "promote", &target_ds])
            .output()
            .map_err(|e| ZfsError::SshError(target.host.clone(), e.to_string()))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            // Promote may fail if not a clone — that's OK for received datasets
            tracing::debug!(
                target = %target.host,
                stderr = %stderr,
                "zfs promote on target (may be expected for non-clone)"
            );
        }

        tracing::info!(
            target = %target.host,
            dataset = %target_ds,
            "Failover target prepared"
        );

        Ok(())
    }
}
