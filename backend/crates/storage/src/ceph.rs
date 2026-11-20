// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: LicenseRef-Zyvor-Production-1.0

use serde::{Deserialize, Serialize};
use std::process::Command;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum CephError {
    #[error("Ceph pool not found: {0}")]
    PoolNotFound(String),

    #[error("Ceph cluster unreachable: {0}")]
    ClusterUnreachable(String),

    #[error("Ceph command failed: {0}")]
    CommandFailed(String),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Parse error: {0}")]
    Parse(String),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CephPool {
    pub monitors: Vec<String>,
    pub pool_name: String,
    pub user: String,
    pub keyring: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CephStats {
    pub total_bytes: u64,
    pub used_bytes: u64,
    pub available_bytes: u64,
    pub objects: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CephHealth {
    pub status: CephHealthStatus,
    pub detail: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum CephHealthStatus {
    Ok,
    Warn,
    Error,
}

impl CephPool {
    pub fn new(monitors: Vec<String>, pool_name: String) -> Result<Self, CephError> {
        if monitors.is_empty() {
            return Err(CephError::ClusterUnreachable(
                "No monitors specified".to_string(),
            ));
        }

        Ok(Self {
            monitors,
            pool_name,
            user: "admin".to_string(),
            keyring: None,
        })
    }

    pub fn with_auth(mut self, user: String, keyring: String) -> Self {
        self.user = user;
        self.keyring = Some(keyring);
        self
    }

    /// Check if the Ceph cluster is reachable
    pub fn check_cluster(&self) -> Result<bool, CephError> {
        let mut cmd = Command::new("ceph");
        cmd.args(["--connect-timeout", "5", "health"]);
        self.add_auth_args(&mut cmd)?;

        match cmd.output() {
            Ok(output) => Ok(output.status.success()),
            Err(_) => Ok(false),
        }
    }

    /// Get pool statistics
    pub fn get_stats(&self) -> Result<CephStats, CephError> {
        let pool_name = input_guard::vet!(
            &self.pool_name,
            CephError::CommandFailed("invalid Ceph pool".into())
        );
        let mut cmd = input_guard::argv_checked!(
            pool_name,
            CephError::CommandFailed("invalid Ceph pool".into()),
            |pool_name| {
                let mut cmd = Command::new("ceph");
                cmd.args(["osd", "pool", "stats", pool_name, "-f", "json"]);
                cmd
            }
        );
        self.add_auth_args(&mut cmd)?;

        let output = cmd.output()?;
        if !output.status.success() {
            return Err(CephError::CommandFailed(
                String::from_utf8_lossy(&output.stderr).to_string(),
            ));
        }

        // Get cluster-level df for capacity info
        let mut df_cmd = Command::new("ceph");
        df_cmd.args(["df", "-f", "json"]);
        self.add_auth_args(&mut df_cmd)?;

        let df_output = df_cmd.output()?;
        if !df_output.status.success() {
            // Return defaults if df fails
            return Ok(CephStats {
                total_bytes: 0,
                used_bytes: 0,
                available_bytes: 0,
                objects: 0,
            });
        }

        let df_json: serde_json::Value = serde_json::from_slice(&df_output.stdout)
            .map_err(|e| CephError::Parse(e.to_string()))?;

        // Parse pool stats from df output
        let mut total = 0u64;
        let mut used = 0u64;
        let mut available = 0u64;
        let mut objects = 0u64;

        if let Some(stats) = df_json.get("stats") {
            total = stats["total_bytes"].as_u64().unwrap_or(0);
            used = stats["total_used_bytes"].as_u64().unwrap_or(0);
            available = stats["total_avail_bytes"].as_u64().unwrap_or(0);
        }

        if let Some(pools) = df_json.get("pools").and_then(|p| p.as_array()) {
            for pool in pools {
                if pool["name"].as_str() == Some(&self.pool_name) {
                    if let Some(pool_stats) = pool.get("stats") {
                        objects = pool_stats["objects"].as_u64().unwrap_or(0);
                        // Use pool-specific used if available
                        if let Some(pool_used) = pool_stats["bytes_used"].as_u64() {
                            used = pool_used;
                        }
                    }
                }
            }
        }

        Ok(CephStats {
            total_bytes: total,
            used_bytes: used,
            available_bytes: available,
            objects,
        })
    }

    /// Get cluster health
    pub fn health_check(&self) -> Result<CephHealth, CephError> {
        let mut cmd = Command::new("ceph");
        cmd.args(["health", "-f", "json"]);
        self.add_auth_args(&mut cmd)?;

        let output = cmd.output()?;
        if !output.status.success() {
            return Ok(CephHealth {
                status: CephHealthStatus::Error,
                detail: String::from_utf8_lossy(&output.stderr).to_string(),
            });
        }

        let json: serde_json::Value =
            serde_json::from_slice(&output.stdout).map_err(|e| CephError::Parse(e.to_string()))?;

        let status_str = json["status"].as_str().unwrap_or("HEALTH_ERR");
        let status = match status_str {
            "HEALTH_OK" => CephHealthStatus::Ok,
            "HEALTH_WARN" => CephHealthStatus::Warn,
            _ => CephHealthStatus::Error,
        };

        Ok(CephHealth {
            status,
            detail: status_str.to_string(),
        })
    }

    /// Create an RBD image in this pool
    pub fn create_rbd_image(&self, name: &str, size_mb: u64) -> Result<(), CephError> {
        let mut cmd = Command::new("rbd");
        cmd.args([
            "create",
            &format!("{}/{}", self.pool_name, name),
            "--size",
            &size_mb.to_string(),
            "--image-format",
            "2",
        ]);
        self.add_auth_args(&mut cmd)?;

        let output = cmd.output()?;
        if !output.status.success() {
            return Err(CephError::CommandFailed(
                String::from_utf8_lossy(&output.stderr).to_string(),
            ));
        }
        Ok(())
    }

    /// Delete an RBD image from this pool
    pub fn delete_rbd_image(&self, name: &str) -> Result<(), CephError> {
        let mut cmd = Command::new("rbd");
        cmd.args(["rm", &format!("{}/{}", self.pool_name, name)]);
        self.add_auth_args(&mut cmd)?;

        let output = cmd.output()?;
        if !output.status.success() {
            return Err(CephError::CommandFailed(
                String::from_utf8_lossy(&output.stderr).to_string(),
            ));
        }
        Ok(())
    }

    /// List RBD images in this pool
    pub fn list_rbd_images(&self) -> Result<Vec<String>, CephError> {
        let mut cmd = Command::new("rbd");
        cmd.args(["ls", &self.pool_name, "-f", "json"]);
        self.add_auth_args(&mut cmd)?;

        let output = cmd.output()?;
        if !output.status.success() {
            return Err(CephError::CommandFailed(
                String::from_utf8_lossy(&output.stderr).to_string(),
            ));
        }

        let images: Vec<String> = serde_json::from_slice(&output.stdout).unwrap_or_default();
        Ok(images)
    }

    fn add_auth_args(&self, cmd: &mut Command) -> Result<(), CephError> {
        if !self.monitors.is_empty() {
            let monitors = self.monitors.join(",");
            let monitors = input_guard::vet!(
                &monitors,
                CephError::CommandFailed("invalid Ceph monitors".into())
            );
            input_guard::argv_checked!(
                monitors,
                CephError::CommandFailed("invalid Ceph monitors".into()),
                |monitors| {
                    cmd.arg("-m").arg(monitors);
                }
            );
        }
        let user = input_guard::vet!(
            &self.user,
            CephError::CommandFailed("invalid Ceph user".into())
        );
        input_guard::argv_checked!(
            user,
            CephError::CommandFailed("invalid Ceph user".into()),
            |user| {
                cmd.arg("--id").arg(user);
            }
        );
        if let Some(ref keyring) = self.keyring {
            let keyring = input_guard::vet!(
                keyring,
                CephError::CommandFailed("invalid Ceph keyring path".into())
            );
            input_guard::argv_checked!(
                keyring,
                CephError::CommandFailed("invalid Ceph keyring path".into()),
                |keyring| {
                    cmd.arg("--keyring").arg(keyring);
                }
            );
        }
        Ok(())
    }
}
