// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: LicenseRef-Zyvor-Production-1.0

use anyhow::{anyhow, Result};
use std::path::{Path, PathBuf};
use std::process::Command;
use tokio::fs;

#[derive(Debug, Clone)]
pub struct TPMConfig {
    pub vm_name: String,
    pub tpm_version: TPMVersion,
    pub state_dir: PathBuf,
}

#[derive(Debug, Clone, Copy)]
pub enum TPMVersion {
    TPM12,
    TPM20,
}

impl TPMVersion {
    pub fn as_str(&self) -> &str {
        match self {
            TPMVersion::TPM12 => "1.2",
            TPMVersion::TPM20 => "2.0",
        }
    }
}

pub struct TPMManager {
    base_dir: PathBuf,
}

impl TPMManager {
    pub fn new<P: AsRef<Path>>(base_dir: P) -> Result<Self> {
        let base_dir = base_dir.as_ref().to_path_buf();
        std::fs::create_dir_all(&base_dir)?;
        Ok(Self { base_dir })
    }

    /// Create a new vTPM for a VM
    pub async fn create_vtpm(&self, config: &TPMConfig) -> Result<PathBuf> {
        let tpm_dir = self.base_dir.join(&config.vm_name);
        fs::create_dir_all(&tpm_dir).await?;

        // Initialize TPM state using swtpm
        self.init_swtpm(&tpm_dir, config.tpm_version).await?;

        Ok(tpm_dir)
    }

    /// Start swtpm daemon for a VM
    pub async fn start_swtpm(&self, vm_name: &str, tpm_version: TPMVersion) -> Result<u32> {
        let tpm_dir = self.base_dir.join(vm_name);
        let socket_path = tpm_dir.join("swtpm-sock");

        let mut cmd = Command::new("swtpm");
        cmd.arg("socket")
            .arg("--tpmstate")
            .arg(format!("dir={}", tpm_dir.display()))
            .arg("--ctrl")
            .arg(format!("type=unixio,path={}", socket_path.display()))
            .arg("--tpm2");

        if matches!(tpm_version, TPMVersion::TPM12) {
            cmd.arg("--tpm");
        }

        let child = cmd
            .arg("--log")
            .arg(format!("file={}/swtpm.log", tpm_dir.display()))
            .spawn()?;

        let pid = child.id();

        tracing::info!("Started swtpm for VM {} with PID {}", vm_name, pid);

        Ok(pid)
    }

    /// Stop swtpm daemon
    pub async fn stop_swtpm(&self, pid: u32) -> Result<()> {
        let output = Command::new("kill").arg(pid.to_string()).output()?;

        if output.status.success() {
            Ok(())
        } else {
            Err(anyhow!("Failed to stop swtpm"))
        }
    }

    /// Initialize TPM state
    async fn init_swtpm(&self, tpm_dir: &Path, version: TPMVersion) -> Result<()> {
        let mut cmd = Command::new("swtpm_setup");
        cmd.arg("--tpm-state").arg(tpm_dir).arg("--tpm2");

        if matches!(version, TPMVersion::TPM12) {
            cmd.arg("--tpm");
        }

        cmd.arg("--create-ek-cert").arg("--create-platform-cert");

        let output = cmd.output()?;

        if output.status.success() {
            tracing::info!("TPM initialized at {}", tpm_dir.display());
            Ok(())
        } else {
            let stderr = String::from_utf8_lossy(&output.stderr);
            Err(anyhow!("Failed to initialize TPM: {}", stderr))
        }
    }

    /// Get socket path for a VM's TPM
    pub fn get_socket_path(&self, vm_name: &str) -> PathBuf {
        self.base_dir.join(vm_name).join("swtpm-sock")
    }

    /// Delete TPM state
    pub async fn delete_vtpm(&self, vm_name: &str) -> Result<()> {
        let tpm_dir = self.base_dir.join(vm_name);
        if tpm_dir.exists() {
            fs::remove_dir_all(&tpm_dir).await?;
            tracing::info!("Deleted TPM state for VM {}", vm_name);
        }
        Ok(())
    }
}

/// Generate TPM device arguments for QEMU/systemd-vmspawn
pub fn generate_tpm_args(socket_path: &Path) -> Vec<String> {
    vec![
        "-chardev".to_string(),
        format!("socket,id=chrtpm,path={}", socket_path.display()),
        "-tpmdev".to_string(),
        "emulator,id=tpm0,chardev=chrtpm".to_string(),
        "-device".to_string(),
        "tpm-tis,tpmdev=tpm0".to_string(),
    ]
}
