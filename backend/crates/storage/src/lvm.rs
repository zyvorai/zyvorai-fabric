// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: LicenseRef-Zyvor-Production-2.0

use serde::{Deserialize, Serialize};
use std::process::Command;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum LvmError {
    #[error("Volume group not found: {0}")]
    VgNotFound(String),

    #[error("Logical volume not found: {0}")]
    LvNotFound(String),

    #[error("Logical volume already exists: {0}")]
    LvExists(String),

    #[error("Thin pool not found: {0}/{1}")]
    ThinPoolNotFound(String, String),

    #[error("Command failed: {0}")]
    CommandFailed(String),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Parse error: {0}")]
    ParseError(String),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LvmVolume {
    pub name: String,
    pub size_bytes: u64,
    pub attributes: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LvmStats {
    pub vg_size_bytes: u64,
    pub vg_free_bytes: u64,
}

pub struct LvmPool {
    vg_name: String,
}

/// Validate an LVM name to prevent argument injection.
/// LVM names may contain alphanumeric, hyphens, underscores, dots, and plus signs.
fn validate_lvm_name(name: &str, label: &str) -> Result<(), LvmError> {
    if name.is_empty() || name.len() > 128 {
        return Err(LvmError::CommandFailed(format!(
            "{} must be 1-128 characters",
            label
        )));
    }
    if name.starts_with('-') {
        return Err(LvmError::CommandFailed(format!(
            "{} must not start with a hyphen",
            label
        )));
    }
    if !name
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.' | '+'))
    {
        return Err(LvmError::CommandFailed(format!(
            "{} '{}' contains invalid characters",
            label, name
        )));
    }
    Ok(())
}

/// Validate an LVM size string (e.g. "10G", "500M").
fn validate_lvm_size(size: &str) -> Result<(), LvmError> {
    if size.is_empty() || size.len() > 20 {
        return Err(LvmError::CommandFailed("Invalid size string".to_string()));
    }
    if !size
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '.' || c == '+' || c == '-')
    {
        return Err(LvmError::CommandFailed(format!(
            "Size '{}' contains invalid characters",
            size
        )));
    }
    Ok(())
}

impl LvmPool {
    /// Create a new LVM pool, validating the VG exists
    pub fn new(vg_name: &str) -> Result<Self, LvmError> {
        validate_lvm_name(vg_name, "VG name")?;
        let vg_name = input_guard::vet!(vg_name, LvmError::CommandFailed("invalid VG name".into()));
        let output = input_guard::argv_checked!(
            vg_name,
            LvmError::CommandFailed("invalid VG name".into()),
            |vg_name| {
                Command::new("vgs")
                    .args([
                        "--noheadings",
                        "--nosuffix",
                        "--units",
                        "b",
                        "-o",
                        "vg_name",
                        vg_name,
                    ])
                    .output()?
            }
        );

        if !output.status.success() {
            return Err(LvmError::VgNotFound(vg_name.to_string()));
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        if stdout.trim().is_empty() {
            return Err(LvmError::VgNotFound(vg_name.to_string()));
        }

        Ok(Self {
            vg_name: vg_name.to_string(),
        })
    }

    pub fn vg_name(&self) -> &str {
        &self.vg_name
    }

    /// Create a regular logical volume
    pub fn create_volume(&self, name: &str, size: &str) -> Result<(), LvmError> {
        validate_lvm_name(name, "LV name")?;
        validate_lvm_size(size)?;
        let output = Command::new("lvcreate")
            .args(["-n", name, "-L", size, &self.vg_name])
            .output()?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(LvmError::CommandFailed(format!(
                "lvcreate failed: {}",
                stderr
            )));
        }

        tracing::info!(vg = %self.vg_name, lv = %name, size = %size, "Created logical volume");
        Ok(())
    }

    /// Create a thin-provisioned logical volume
    pub fn create_thin_volume(
        &self,
        name: &str,
        size: &str,
        thin_pool: &str,
    ) -> Result<(), LvmError> {
        validate_lvm_name(name, "LV name")?;
        validate_lvm_name(thin_pool, "Thin pool name")?;
        validate_lvm_size(size)?;
        let pool_path = format!("{}/{}", self.vg_name, thin_pool);
        let output = Command::new("lvcreate")
            .args(["-V", size, "-T", &pool_path, "-n", name])
            .output()?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(LvmError::CommandFailed(format!(
                "lvcreate thin failed: {}",
                stderr
            )));
        }

        tracing::info!(
            vg = %self.vg_name, lv = %name, thin_pool = %thin_pool,
            size = %size, "Created thin logical volume"
        );
        Ok(())
    }

    /// Delete a logical volume
    pub fn delete_volume(&self, name: &str) -> Result<(), LvmError> {
        validate_lvm_name(name, "LV name")?;
        let lv_path = format!("{}/{}", self.vg_name, name);
        let output = Command::new("lvremove").args(["-f", &lv_path]).output()?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(LvmError::CommandFailed(format!(
                "lvremove failed: {}",
                stderr
            )));
        }

        tracing::info!(vg = %self.vg_name, lv = %name, "Deleted logical volume");
        Ok(())
    }

    /// Resize a logical volume
    pub fn resize_volume(&self, name: &str, new_size: &str) -> Result<(), LvmError> {
        validate_lvm_name(name, "LV name")?;
        validate_lvm_size(new_size)?;
        let lv_path = format!("{}/{}", self.vg_name, name);
        let output = Command::new("lvresize")
            .args(["-L", new_size, &lv_path])
            .output()?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(LvmError::CommandFailed(format!(
                "lvresize failed: {}",
                stderr
            )));
        }

        tracing::info!(vg = %self.vg_name, lv = %name, new_size = %new_size, "Resized logical volume");
        Ok(())
    }

    /// List all logical volumes in the VG
    pub fn list_volumes(&self) -> Result<Vec<LvmVolume>, LvmError> {
        let output = Command::new("lvs")
            .args([
                "--noheadings",
                "--nosuffix",
                "--units",
                "b",
                "-o",
                "lv_name,lv_size,lv_attr",
                &self.vg_name,
            ])
            .output()?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(LvmError::CommandFailed(format!("lvs failed: {}", stderr)));
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        let mut volumes = Vec::new();

        for line in stdout.lines() {
            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.len() >= 3 {
                let size_bytes: u64 = parts[1]
                    .parse()
                    .map_err(|_| LvmError::ParseError(format!("Invalid size: {}", parts[1])))?;

                volumes.push(LvmVolume {
                    name: parts[0].to_string(),
                    size_bytes,
                    attributes: parts[2].to_string(),
                });
            }
        }

        Ok(volumes)
    }

    /// Get VG statistics
    pub fn get_stats(&self) -> Result<LvmStats, LvmError> {
        let vg_name = input_guard::vet!(
            &self.vg_name,
            LvmError::CommandFailed("invalid VG name".into())
        );
        let output = input_guard::argv_checked!(
            vg_name,
            LvmError::CommandFailed("invalid VG name".into()),
            |vg_name| {
                Command::new("vgs")
                    .args([
                        "--noheadings",
                        "--nosuffix",
                        "--units",
                        "b",
                        "-o",
                        "vg_size,vg_free",
                        vg_name,
                    ])
                    .output()?
            }
        );

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(LvmError::CommandFailed(format!("vgs failed: {}", stderr)));
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        let parts: Vec<&str> = stdout.split_whitespace().collect();

        if parts.len() < 2 {
            return Err(LvmError::ParseError("Unexpected vgs output".to_string()));
        }

        let vg_size_bytes: u64 = parts[0]
            .parse()
            .map_err(|_| LvmError::ParseError(format!("Invalid vg_size: {}", parts[0])))?;
        let vg_free_bytes: u64 = parts[1]
            .parse()
            .map_err(|_| LvmError::ParseError(format!("Invalid vg_free: {}", parts[1])))?;

        Ok(LvmStats {
            vg_size_bytes,
            vg_free_bytes,
        })
    }

    /// Create a snapshot of an existing LV
    pub fn snapshot(&self, source: &str, snap_name: &str, size: &str) -> Result<(), LvmError> {
        validate_lvm_name(source, "Source LV name")?;
        validate_lvm_name(snap_name, "Snapshot name")?;
        validate_lvm_size(size)?;
        let source_path = format!("{}/{}", self.vg_name, source);
        let output = Command::new("lvcreate")
            .args(["-s", "-n", snap_name, "-L", size, &source_path])
            .output()?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(LvmError::CommandFailed(format!(
                "lvcreate snapshot failed: {}",
                stderr
            )));
        }

        tracing::info!(
            vg = %self.vg_name, source = %source, snapshot = %snap_name,
            "Created LVM snapshot"
        );
        Ok(())
    }

    /// Get the device path for a logical volume
    pub fn device_path(&self, lv_name: &str) -> String {
        format!("/dev/{}/{}", self.vg_name, lv_name)
    }
}
