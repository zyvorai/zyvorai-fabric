// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: LicenseRef-Zyvor-Production-2.0

use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;
use std::process::Command;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum NfsError {
    #[error("Failed to mount NFS: {0}")]
    MountFailed(String),

    #[error("Failed to unmount NFS: {0}")]
    UnmountFailed(String),

    #[error("NFS server unreachable: {0}")]
    ServerUnreachable(String),

    #[error("Invalid NFS path: {0}")]
    InvalidPath(String),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Mount point already exists: {0}")]
    MountPointExists(String),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NfsConfig {
    pub server: String,
    pub export_path: String,
    pub mount_path: PathBuf,
    pub mount_options: Vec<String>,
    pub auto_start: bool,
    pub nfs_version: NfsVersion,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum NfsVersion {
    V3,
    V4,
    V4_1,
    V4_2,
}

impl NfsVersion {
    pub fn as_str(&self) -> &str {
        match self {
            NfsVersion::V3 => "3",
            NfsVersion::V4 => "4",
            NfsVersion::V4_1 => "4.1",
            NfsVersion::V4_2 => "4.2",
        }
    }
}

impl Default for NfsConfig {
    fn default() -> Self {
        Self {
            server: String::new(),
            export_path: String::new(),
            mount_path: PathBuf::new(),
            mount_options: vec![
                "rw".to_string(),
                "hard".to_string(),
                "intr".to_string(),
                "rsize=8192".to_string(),
                "wsize=8192".to_string(),
            ],
            auto_start: true,
            nfs_version: NfsVersion::V4,
        }
    }
}

pub struct NfsPool {
    config: NfsConfig,
    mounted: bool,
}

impl NfsPool {
    pub fn new(config: NfsConfig) -> Result<Self, NfsError> {
        // Validate server and path
        if config.server.is_empty() {
            return Err(NfsError::InvalidPath("Server cannot be empty".to_string()));
        }
        if !config
            .server
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | ':' | '_'))
        {
            return Err(NfsError::InvalidPath(format!(
                "Server '{}' contains invalid characters",
                config.server
            )));
        }

        if config.export_path.is_empty() {
            return Err(NfsError::InvalidPath(
                "Export path cannot be empty".to_string(),
            ));
        }
        if !config.export_path.starts_with('/') {
            return Err(NfsError::InvalidPath(
                "Export path must be absolute".to_string(),
            ));
        }

        // Validate mount options — reject shell metacharacters
        for opt in &config.mount_options {
            if opt
                .chars()
                .any(|c| matches!(c, ';' | '|' | '&' | '$' | '`' | '\'' | '"' | '\\' | '\n'))
            {
                return Err(NfsError::InvalidPath(format!(
                    "Mount option '{}' contains invalid characters",
                    opt
                )));
            }
        }

        Ok(Self {
            config,
            mounted: false,
        })
    }

    /// Check if NFS server is reachable
    pub fn check_server(&self) -> Result<bool, NfsError> {
        let output = Command::new("ping")
            .args(["-c", "1", "-W", "2", &self.config.server])
            .output()
            .map_err(|e| NfsError::ServerUnreachable(e.to_string()))?;

        Ok(output.status.success())
    }

    /// Check if NFS export exists
    pub fn check_export(&self) -> Result<bool, NfsError> {
        let output = Command::new("showmount")
            .args(["-e", &self.config.server])
            .output()
            .map_err(|e| NfsError::ServerUnreachable(e.to_string()))?;

        if !output.status.success() {
            return Ok(false);
        }

        let exports = String::from_utf8_lossy(&output.stdout);
        Ok(exports.contains(&self.config.export_path))
    }

    /// Mount the NFS share
    pub fn mount(&mut self) -> Result<(), NfsError> {
        if self.mounted {
            return Ok(());
        }

        // Create mount point if it doesn't exist
        if !self.config.mount_path.exists() {
            fs::create_dir_all(&self.config.mount_path)?;
        } else if self.is_mounted()? {
            return Err(NfsError::MountPointExists(
                self.config.mount_path.display().to_string(),
            ));
        }

        // Build mount command
        let source = format!("{}:{}", self.config.server, self.config.export_path);
        let mut mount_opts = self.config.mount_options.clone();
        mount_opts.push(format!("vers={}", self.config.nfs_version.as_str()));

        let options = mount_opts.join(",");

        let output = Command::new("mount")
            .args([
                "-t",
                "nfs",
                "-o",
                &options,
                &source,
                self.config.mount_path.to_str().unwrap(),
            ])
            .output()
            .map_err(|e| NfsError::MountFailed(e.to_string()))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(NfsError::MountFailed(stderr.to_string()));
        }

        self.mounted = true;
        Ok(())
    }

    /// Unmount the NFS share
    pub fn unmount(&mut self) -> Result<(), NfsError> {
        if !self.mounted {
            return Ok(());
        }

        let output = Command::new("umount")
            .arg(self.config.mount_path.to_str().unwrap())
            .output()
            .map_err(|e| NfsError::UnmountFailed(e.to_string()))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(NfsError::UnmountFailed(stderr.to_string()));
        }

        self.mounted = false;
        Ok(())
    }

    /// Force unmount (lazy unmount)
    pub fn force_unmount(&mut self) -> Result<(), NfsError> {
        let output = Command::new("umount")
            .args(["-l", self.config.mount_path.to_str().unwrap()])
            .output()
            .map_err(|e| NfsError::UnmountFailed(e.to_string()))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(NfsError::UnmountFailed(stderr.to_string()));
        }

        self.mounted = false;
        Ok(())
    }

    /// Check if mount point is currently mounted
    pub fn is_mounted(&self) -> Result<bool, NfsError> {
        let output = Command::new("findmnt")
            .args([
                "-n",
                "-o",
                "SOURCE",
                self.config.mount_path.to_str().unwrap(),
            ])
            .output()
            .map_err(NfsError::Io)?;

        Ok(output.status.success())
    }

    /// Get mount statistics
    pub fn get_stats(&self) -> Result<NfsStats, NfsError> {
        if !self.is_mounted()? {
            return Err(NfsError::MountFailed("Not mounted".to_string()));
        }

        // Get filesystem stats
        let output = Command::new("df")
            .args(["-k", self.config.mount_path.to_str().unwrap()])
            .output()?;

        let df_output = String::from_utf8_lossy(&output.stdout);
        let stats = self.parse_df_output(&df_output)?;

        Ok(stats)
    }

    fn parse_df_output(&self, output: &str) -> Result<NfsStats, NfsError> {
        let lines: Vec<&str> = output.lines().collect();
        if lines.len() < 2 {
            return Err(NfsError::InvalidPath("Invalid df output".to_string()));
        }

        let parts: Vec<&str> = lines[1].split_whitespace().collect();
        if parts.len() < 6 {
            return Err(NfsError::InvalidPath(
                "Invalid df output format".to_string(),
            ));
        }

        Ok(NfsStats {
            total_kb: parts[1].parse().unwrap_or(0),
            used_kb: parts[2].parse().unwrap_or(0),
            available_kb: parts[3].parse().unwrap_or(0),
            use_percent: parts[4].trim_end_matches('%').parse().unwrap_or(0),
            mount_point: parts[5].to_string(),
        })
    }

    /// Health check
    pub fn health_check(&self) -> Result<NfsHealth, NfsError> {
        let server_reachable = self.check_server()?;
        let is_mounted = self.is_mounted()?;

        let status = if server_reachable && is_mounted {
            NfsHealthStatus::Healthy
        } else if !server_reachable {
            NfsHealthStatus::ServerUnreachable
        } else {
            NfsHealthStatus::Unmounted
        };

        Ok(NfsHealth {
            status,
            server_reachable,
            is_mounted,
            last_check: chrono::Utc::now(),
        })
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NfsStats {
    pub total_kb: u64,
    pub used_kb: u64,
    pub available_kb: u64,
    pub use_percent: u32,
    pub mount_point: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NfsHealth {
    pub status: NfsHealthStatus,
    pub server_reachable: bool,
    pub is_mounted: bool,
    pub last_check: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum NfsHealthStatus {
    Healthy,
    ServerUnreachable,
    Unmounted,
    Degraded,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_nfs_config_default() {
        let config = NfsConfig::default();
        assert!(config.auto_start);
        assert!(config.mount_options.contains(&"rw".to_string()));
    }

    #[test]
    fn test_nfs_version_string() {
        assert_eq!(NfsVersion::V3.as_str(), "3");
        assert_eq!(NfsVersion::V4.as_str(), "4");
        assert_eq!(NfsVersion::V4_1.as_str(), "4.1");
    }

    #[test]
    fn test_invalid_config() {
        let config = NfsConfig {
            server: String::new(),
            export_path: String::new(),
            ..Default::default()
        };

        let result = NfsPool::new(config);
        assert!(result.is_err());
    }
}
