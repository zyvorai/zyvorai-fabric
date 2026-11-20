// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: LicenseRef-Zyvor-Production-1.0

use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Arc;
use zyvor_fabric_driver_core::VmDriver;

// ZYVOR_RUNTIME_BOUNDARY_V1
pub mod runtime;
pub use runtime::{
    progress_percent, NativeMigrationOptions, PrepareReceiverOptions, PreparedTarget,
    RuntimeMigrationManager,
};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MigrationConfig {
    pub vm_name: String,
    pub source_node: String,
    pub target_node: String,
    pub live: bool,
    pub compress: bool,
    pub bandwidth_mbps: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MigrationStatus {
    pub vm_name: String,
    pub status: MigrationState,
    pub progress_percent: u8,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MigrationState {
    Preparing,
    Copying,
    Syncing,
    Completed,
    Failed,
}

pub struct MigrationManager {
    #[allow(dead_code)]
    workspace_dir: PathBuf,
    /// Drives the *source* (local) VM's pause-for-final-sync step — see
    /// `live_sync`. The target node's own start is still a raw SSH+CLI
    /// call (see `live_sync`'s doc comment) since it's a different host,
    /// outside what a local `Arc<dyn VmDriver>` can reach. Currently unread:
    /// `live_sync` is intentionally stubbed out under the FluxVM runtime
    /// boundary (see its doc comment) and never reaches the code that
    /// would call `self.driver` — kept, not deleted, since `new()` still
    /// takes and stores a driver for when that path is un-stubbed.
    #[allow(dead_code)]
    driver: Arc<dyn VmDriver>,
}

impl MigrationManager {
    pub fn new<P: AsRef<Path>>(workspace_dir: P, driver: Arc<dyn VmDriver>) -> Result<Self> {
        let workspace_dir = workspace_dir.as_ref().to_path_buf();
        std::fs::create_dir_all(&workspace_dir)?;
        Ok(Self {
            workspace_dir,
            driver,
        })
    }

    /// Start VM migration
    pub async fn migrate_vm(&self, config: &MigrationConfig) -> Result<MigrationStatus> {
        if config.live {
            anyhow::bail!(
                "legacy rsync/machinectl live migration is disabled under the FluxVM runtime boundary; use RuntimeMigrationManager with a prepared FluxVM target"
            );
        }
        tracing::info!(
            "Starting {} migration of VM {} from {} to {}",
            if config.live { "live" } else { "offline" },
            config.vm_name,
            config.source_node,
            config.target_node
        );

        let mut status = MigrationStatus {
            vm_name: config.vm_name.clone(),
            status: MigrationState::Preparing,
            progress_percent: 0,
            error: None,
        };

        // Step 1: Prepare target node
        status.status = MigrationState::Preparing;
        self.prepare_target(config).await?;

        // Step 2: Copy VM state and disk
        status.status = MigrationState::Copying;
        status.progress_percent = 20;
        self.copy_vm_data(config).await?;

        // Step 3: Live migration (if enabled)
        if config.live {
            status.status = MigrationState::Syncing;
            status.progress_percent = 60;
            self.live_sync(config).await?;
        }

        // Step 4: Complete migration
        status.status = MigrationState::Completed;
        status.progress_percent = 100;

        tracing::info!("Migration of VM {} completed successfully", config.vm_name);

        Ok(status)
    }

    /// Prepare target node for migration
    async fn prepare_target(&self, config: &MigrationConfig) -> Result<()> {
        tracing::info!("Preparing target node {}", config.target_node);

        // Create VM directory on target
        let output = Command::new("ssh")
            .arg(&config.target_node)
            .args([
                "mkdir",
                "-p",
                &format!("/var/lib/zyvor-fabricd/vms/{}", config.vm_name),
            ])
            .output()?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(anyhow::anyhow!("Failed to prepare target: {}", stderr));
        }

        Ok(())
    }

    /// Copy VM data to target node
    async fn copy_vm_data(&self, config: &MigrationConfig) -> Result<()> {
        tracing::info!("Copying VM data to target node");

        let source_path = format!("/var/lib/zyvor-fabricd/vms/{}/", config.vm_name);
        let target_path = format!(
            "{}:/var/lib/zyvor-fabricd/vms/{}/",
            config.target_node, config.vm_name
        );

        let mut cmd = Command::new("rsync");
        cmd.args(["-avz", "--progress"]);

        if config.compress {
            cmd.arg("-z");
        }

        if let Some(bw) = config.bandwidth_mbps {
            cmd.arg(format!("--bwlimit={}", bw * 1024)); // Convert to KB/s
        }

        cmd.arg(&source_path).arg(&target_path);

        let output = cmd.output()?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(anyhow::anyhow!("Failed to copy VM data: {}", stderr));
        }

        tracing::info!("VM data copied successfully");

        Ok(())
    }

    /// Perform live synchronization using iterative rsync + final cutover.
    ///
    /// The source (local) VM's pause-for-final-sync goes through the
    /// active FluxVM `VmDriver`. Starting the VM on the *target* node shells
    /// `ssh <target> fabricctl start <vm>` — a local `Arc<dyn VmDriver>` only
    /// talks to this host's FluxVM, not a remote one.
    async fn live_sync(&self, _config: &MigrationConfig) -> Result<()> {
        anyhow::bail!(
            "legacy live_sync is intentionally disabled: FluxVM owns VMM migration transport; Fabric owns orchestration. Use RuntimeMigrationManager after the target runtime is prepared"
        )
    }

    /// Cancel ongoing migration
    pub async fn cancel_migration(&self, vm_name: &str) -> Result<()> {
        // Validate vm_name to prevent regex injection in pgrep/pkill patterns
        if vm_name.is_empty()
            || !vm_name
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_'))
        {
            return Err(anyhow::anyhow!("Invalid VM name for migration"));
        }

        tracing::info!("Cancelling migration for VM {}", vm_name);

        // Kill rsync processes for this VM
        let output = Command::new("pkill")
            .args(["-f", &format!("rsync.*{}", vm_name)])
            .output()?;

        if output.status.success() {
            tracing::info!("Migration cancelled successfully");
        }

        Ok(())
    }

    /// Get migration status
    pub async fn get_migration_status(&self, vm_name: &str) -> Result<MigrationStatus> {
        // Validate vm_name to prevent regex injection in pgrep/pkill patterns
        if vm_name.is_empty()
            || !vm_name
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_'))
        {
            return Err(anyhow::anyhow!("Invalid VM name for migration"));
        }

        // Check if migration is in progress
        let output = Command::new("pgrep")
            .args(["-f", &format!("rsync.*{}", vm_name)])
            .output()?;

        if output.status.success() && !output.stdout.is_empty() {
            Ok(MigrationStatus {
                vm_name: vm_name.to_string(),
                status: MigrationState::Copying,
                progress_percent: 50,
                error: None,
            })
        } else {
            Ok(MigrationStatus {
                vm_name: vm_name.to_string(),
                status: MigrationState::Completed,
                progress_percent: 100,
                error: None,
            })
        }
    }
}
