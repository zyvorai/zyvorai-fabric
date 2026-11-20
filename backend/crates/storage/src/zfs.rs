// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: LicenseRef-Zyvor-Production-1.0

use serde::{Deserialize, Serialize};
use std::process::Command;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum ZfsError {
    #[error("ZFS pool not found: {0}")]
    PoolNotFound(String),

    #[error("Dataset not found: {0}")]
    DatasetNotFound(String),

    #[error("Dataset already exists: {0}")]
    DatasetExists(String),

    #[error("Snapshot not found: {0}")]
    SnapshotNotFound(String),

    #[error("Command failed: {0}")]
    CommandFailed(String),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Parse error: {0}")]
    ParseError(String),

    #[error("Replication error: {0}")]
    ReplicationError(String),

    #[error("SSH error connecting to {0}: {1}")]
    SshError(String, String),

    #[error("Clone error: {0}")]
    CloneError(String),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ZfsVolume {
    pub name: String,
    pub used_bytes: u64,
    pub available_bytes: u64,
    pub refer_bytes: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ZfsStats {
    pub size_bytes: u64,
    pub alloc_bytes: u64,
    pub free_bytes: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ZfsSnapshot {
    pub name: String,
    pub creation: String,
    pub used_bytes: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ZfsSendResult {
    pub dataset: String,
    pub from_snapshot: Option<String>,
    pub to_snapshot: String,
    pub bytes_sent: u64,
    pub duration_secs: u64,
    pub incremental: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ZfsReplicationTarget {
    pub host: String,
    pub ssh_port: u16,
    pub ssh_user: String,
    pub target_pool: String,
    pub target_dataset: Option<String>,
    pub bandwidth_limit_kbps: Option<u64>,
    pub compress: bool,
}

impl Default for ZfsReplicationTarget {
    fn default() -> Self {
        Self {
            host: String::new(),
            ssh_port: 22,
            ssh_user: "root".to_string(),
            target_pool: String::new(),
            target_dataset: None,
            bandwidth_limit_kbps: None,
            compress: false,
        }
    }
}

pub struct ZfsPool {
    zpool: String,
    dataset: Option<String>,
}

impl ZfsPool {
    /// Create a new ZFS pool handle, validating the pool exists
    pub fn new(zpool: &str, dataset: Option<String>) -> Result<Self, ZfsError> {
        let zpool = input_guard::vet!(zpool, ZfsError::CommandFailed("invalid zpool name".into()));
        if let Some(ds) = &dataset {
            let _ = input_guard::vet!(ds, ZfsError::CommandFailed("invalid dataset name".into()));
        }
        let output = input_guard::argv_checked!(
            zpool,
            ZfsError::CommandFailed("invalid zpool name".into()),
            |zpool| Command::new("zpool").args(["status", zpool]).output()?
        );

        if !output.status.success() {
            return Err(ZfsError::PoolNotFound(zpool.to_string()));
        }

        Ok(Self {
            zpool: zpool.to_string(),
            dataset,
        })
    }

    pub fn zpool_name(&self) -> &str {
        &self.zpool
    }

    /// Get the base path (zpool or zpool/dataset)
    fn base_path(&self) -> String {
        match &self.dataset {
            Some(ds) => format!("{}/{}", self.zpool, ds),
            None => self.zpool.clone(),
        }
    }

    /// Create a ZFS volume (zvol)
    pub fn create_volume(&self, name: &str, size: &str) -> Result<(), ZfsError> {
        let full_path = format!("{}/{}", self.base_path(), name);
        let output = Command::new("zfs")
            .args(["create", "-V", size, &full_path])
            .output()?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(ZfsError::CommandFailed(format!(
                "zfs create zvol failed: {}",
                stderr
            )));
        }

        tracing::info!(zpool = %self.zpool, zvol = %name, size = %size, "Created ZFS volume");
        Ok(())
    }

    /// Create a ZFS dataset (filesystem)
    pub fn create_dataset(&self, name: &str) -> Result<(), ZfsError> {
        let full_path = format!("{}/{}", self.base_path(), name);
        let output = Command::new("zfs").args(["create", &full_path]).output()?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(ZfsError::CommandFailed(format!(
                "zfs create dataset failed: {}",
                stderr
            )));
        }

        tracing::info!(zpool = %self.zpool, dataset = %name, "Created ZFS dataset");
        Ok(())
    }

    /// Delete a ZFS volume or dataset
    pub fn delete_volume(&self, name: &str) -> Result<(), ZfsError> {
        let full_path = format!("{}/{}", self.base_path(), name);
        let output = Command::new("zfs").args(["destroy", &full_path]).output()?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(ZfsError::CommandFailed(format!(
                "zfs destroy failed: {}",
                stderr
            )));
        }

        tracing::info!(zpool = %self.zpool, name = %name, "Deleted ZFS volume/dataset");
        Ok(())
    }

    /// Resize a ZFS volume
    pub fn resize_volume(&self, name: &str, new_size: &str) -> Result<(), ZfsError> {
        let full_path = format!("{}/{}", self.base_path(), name);
        let property = format!("volsize={}", new_size);
        let output = Command::new("zfs")
            .args(["set", &property, &full_path])
            .output()?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(ZfsError::CommandFailed(format!(
                "zfs set volsize failed: {}",
                stderr
            )));
        }

        tracing::info!(zpool = %self.zpool, name = %name, new_size = %new_size, "Resized ZFS volume");
        Ok(())
    }

    /// List all volumes/datasets under this pool
    pub fn list_volumes(&self) -> Result<Vec<ZfsVolume>, ZfsError> {
        let base = self.base_path();
        let output = Command::new("zfs")
            .args(["list", "-H", "-o", "name,used,avail,refer", "-r", &base])
            .output()?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(ZfsError::CommandFailed(format!(
                "zfs list failed: {}",
                stderr
            )));
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        let mut volumes = Vec::new();

        for line in stdout.lines() {
            let parts: Vec<&str> = line.split('\t').collect();
            if parts.len() >= 4 {
                volumes.push(ZfsVolume {
                    name: parts[0].to_string(),
                    used_bytes: parse_zfs_size(parts[1]),
                    available_bytes: parse_zfs_size(parts[2]),
                    refer_bytes: parse_zfs_size(parts[3]),
                });
            }
        }

        Ok(volumes)
    }

    /// Get pool statistics
    pub fn get_stats(&self) -> Result<ZfsStats, ZfsError> {
        let zpool = input_guard::vet!(
            &self.zpool,
            ZfsError::CommandFailed("invalid zpool name".into())
        );
        let output = input_guard::argv_checked!(
            zpool,
            ZfsError::CommandFailed("invalid zpool name".into()),
            |zpool| {
                Command::new("zpool")
                    .args(["list", "-H", "-o", "size,alloc,free", zpool])
                    .output()?
            }
        );

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(ZfsError::CommandFailed(format!(
                "zpool list failed: {}",
                stderr
            )));
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        let parts: Vec<&str> = stdout.trim().split('\t').collect();

        if parts.len() < 3 {
            return Err(ZfsError::ParseError(
                "Unexpected zpool list output".to_string(),
            ));
        }

        Ok(ZfsStats {
            size_bytes: parse_zfs_size(parts[0]),
            alloc_bytes: parse_zfs_size(parts[1]),
            free_bytes: parse_zfs_size(parts[2]),
        })
    }

    /// Create a ZFS snapshot
    pub fn snapshot(&self, dataset: &str, snap_name: &str) -> Result<(), ZfsError> {
        let snap_path = format!("{}/{}@{}", self.base_path(), dataset, snap_name);
        let output = Command::new("zfs")
            .args(["snapshot", &snap_path])
            .output()?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(ZfsError::CommandFailed(format!(
                "zfs snapshot failed: {}",
                stderr
            )));
        }

        tracing::info!(
            zpool = %self.zpool, dataset = %dataset, snapshot = %snap_name,
            "Created ZFS snapshot"
        );
        Ok(())
    }

    /// Rollback to a ZFS snapshot
    pub fn rollback(&self, dataset: &str, snap_name: &str) -> Result<(), ZfsError> {
        let snap_path = format!("{}/{}@{}", self.base_path(), dataset, snap_name);
        let output = Command::new("zfs")
            .args(["rollback", &snap_path])
            .output()?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(ZfsError::CommandFailed(format!(
                "zfs rollback failed: {}",
                stderr
            )));
        }

        tracing::info!(
            zpool = %self.zpool, dataset = %dataset, snapshot = %snap_name,
            "Rolled back to ZFS snapshot"
        );
        Ok(())
    }

    /// List ZFS snapshots
    pub fn list_snapshots(&self) -> Result<Vec<ZfsSnapshot>, ZfsError> {
        let base = self.base_path();
        let output = Command::new("zfs")
            .args([
                "list",
                "-t",
                "snapshot",
                "-H",
                "-o",
                "name,creation,used",
                "-r",
                &base,
            ])
            .output()?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(ZfsError::CommandFailed(format!(
                "zfs list snapshots failed: {}",
                stderr
            )));
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        let mut snapshots = Vec::new();

        for line in stdout.lines() {
            let parts: Vec<&str> = line.split('\t').collect();
            if parts.len() >= 3 {
                snapshots.push(ZfsSnapshot {
                    name: parts[0].to_string(),
                    creation: parts[1].to_string(),
                    used_bytes: parse_zfs_size(parts[2]),
                });
            }
        }

        Ok(snapshots)
    }

    /// Get the device path for a ZFS volume
    pub fn device_path(&self, zvol_name: &str) -> String {
        format!("/dev/zvol/{}/{}", self.base_path(), zvol_name)
    }

    // -- Replication methods ------------------------------------------------

    /// Validate a ZFS name (dataset, snapshot, pool) to prevent injection.
    /// ZFS names may contain alphanumeric, hyphens, underscores, dots, colons, and slashes.
    pub(crate) fn validate_zfs_name(name: &str, label: &str) -> Result<(), ZfsError> {
        if name.is_empty() || name.len() > 256 {
            return Err(ZfsError::CommandFailed(format!(
                "{} must be 1-256 characters",
                label
            )));
        }
        if !name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.' | ':' | '/' | '@'))
        {
            return Err(ZfsError::CommandFailed(format!(
                "{} '{}' contains invalid characters",
                label, name
            )));
        }
        Ok(())
    }

    /// Validate a replication target's fields to prevent command injection.
    pub(crate) fn validate_target(target: &ZfsReplicationTarget) -> Result<(), ZfsError> {
        // Validate hostname — only allow safe characters
        if target.host.is_empty()
            || !target
                .host
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | ':'))
        {
            return Err(ZfsError::SshError(
                target.host.clone(),
                "Invalid hostname".to_string(),
            ));
        }
        // Validate SSH user — only allow safe characters
        if target.ssh_user.is_empty()
            || !target
                .ssh_user
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'))
        {
            return Err(ZfsError::SshError(
                target.host.clone(),
                format!("Invalid SSH user: {}", target.ssh_user),
            ));
        }
        Self::validate_zfs_name(&target.target_pool, "Target pool")?;
        if let Some(ref ds) = target.target_dataset {
            Self::validate_zfs_name(ds, "Target dataset")?;
        }
        Ok(())
    }

    /// Build SSH command arguments for a replication target.
    pub(crate) fn ssh_args(target: &ZfsReplicationTarget) -> Vec<String> {
        vec![
            "ssh".to_string(),
            "-p".to_string(),
            target.ssh_port.to_string(),
            "-o".to_string(),
            "ConnectTimeout=10".to_string(),
            format!("{}@{}", target.ssh_user, target.host),
        ]
    }

    /// Build the target dataset path on the remote host
    fn target_path(target: &ZfsReplicationTarget, dataset: &str) -> String {
        match &target.target_dataset {
            Some(ds) => format!("{}/{}", target.target_pool, ds),
            None => format!("{}/{}", target.target_pool, dataset),
        }
    }

    /// Pipe `zfs send` stdout into `ssh <target> zfs recv` using proper process piping.
    fn pipe_zfs_send_recv(
        send_args: &[&str],
        target: &ZfsReplicationTarget,
        target_ds: &str,
    ) -> Result<std::process::Output, ZfsError> {
        use std::process::Stdio;

        let send_child = Command::new("zfs")
            .args(send_args)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()?;

        let send_stdout = send_child.stdout.ok_or_else(|| {
            ZfsError::CommandFailed("Failed to capture stdout for zfs send".to_string())
        })?;

        let ssh_args = Self::ssh_args(target);
        let output = Command::new(&ssh_args[0])
            .args(&ssh_args[1..])
            .args(["zfs", "recv", "-F", target_ds])
            .stdin(send_stdout)
            .output()?;

        Ok(output)
    }

    /// Send a full ZFS snapshot to a remote host
    pub fn send_full(
        &self,
        dataset: &str,
        snap_name: &str,
        target: &ZfsReplicationTarget,
    ) -> Result<ZfsSendResult, ZfsError> {
        Self::validate_target(target)?;
        Self::validate_zfs_name(dataset, "Dataset")?;
        Self::validate_zfs_name(snap_name, "Snapshot")?;

        let snap_path = format!("{}/{}@{}", self.base_path(), dataset, snap_name);
        let target_ds = Self::target_path(target, dataset);

        let start = std::time::Instant::now();
        let output = Self::pipe_zfs_send_recv(&[&snap_path], target, &target_ds)?;
        let duration = start.elapsed().as_secs();

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(ZfsError::ReplicationError(format!(
                "zfs send full failed: {}",
                stderr
            )));
        }

        tracing::info!(
            zpool = %self.zpool,
            dataset = %dataset,
            snapshot = %snap_name,
            target = %target.host,
            duration_secs = duration,
            "Full ZFS send completed"
        );

        Ok(ZfsSendResult {
            dataset: dataset.to_string(),
            from_snapshot: None,
            to_snapshot: snap_name.to_string(),
            bytes_sent: 0,
            duration_secs: duration,
            incremental: false,
        })
    }

    /// Send an incremental ZFS snapshot to a remote host
    pub fn send_incremental(
        &self,
        dataset: &str,
        from_snap: &str,
        to_snap: &str,
        target: &ZfsReplicationTarget,
    ) -> Result<ZfsSendResult, ZfsError> {
        Self::validate_target(target)?;
        Self::validate_zfs_name(dataset, "Dataset")?;
        Self::validate_zfs_name(from_snap, "From snapshot")?;
        Self::validate_zfs_name(to_snap, "To snapshot")?;

        let from_path = format!("{}/{}@{}", self.base_path(), dataset, from_snap);
        let to_path = format!("{}/{}@{}", self.base_path(), dataset, to_snap);
        let target_ds = Self::target_path(target, dataset);

        let start = std::time::Instant::now();
        let output = Self::pipe_zfs_send_recv(&["-i", &from_path, &to_path], target, &target_ds)?;
        let duration = start.elapsed().as_secs();

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(ZfsError::ReplicationError(format!(
                "zfs send incremental failed: {}",
                stderr
            )));
        }

        tracing::info!(
            zpool = %self.zpool,
            dataset = %dataset,
            from = %from_snap,
            to = %to_snap,
            target = %target.host,
            duration_secs = duration,
            "Incremental ZFS send completed"
        );

        Ok(ZfsSendResult {
            dataset: dataset.to_string(),
            from_snapshot: Some(from_snap.to_string()),
            to_snapshot: to_snap.to_string(),
            bytes_sent: 0,
            duration_secs: duration,
            incremental: true,
        })
    }

    /// Estimate the size of a ZFS send stream (dry-run)
    pub fn estimate_send_size(
        &self,
        dataset: &str,
        from_snap: Option<&str>,
        to_snap: &str,
    ) -> Result<u64, ZfsError> {
        Self::validate_zfs_name(dataset, "Dataset")?;
        Self::validate_zfs_name(to_snap, "To snapshot")?;
        if let Some(from) = from_snap {
            Self::validate_zfs_name(from, "From snapshot")?;
        }

        let to_path = format!("{}/{}@{}", self.base_path(), dataset, to_snap);

        let mut args = vec!["send", "-nv"];
        let from_path;
        if let Some(from) = from_snap {
            from_path = format!("{}/{}@{}", self.base_path(), dataset, from);
            args.extend_from_slice(&["-i", &from_path]);
        }
        args.push(&to_path);

        let output = Command::new("zfs").args(&args).output()?;

        // zfs send -nv outputs to stderr
        let stderr = String::from_utf8_lossy(&output.stderr);

        // Parse "estimated size is X" from stderr
        for line in stderr.lines() {
            if let Some(rest) = line.strip_prefix("estimated size is ") {
                return Ok(parse_zfs_size(rest.trim()));
            }
            // Also handle the "size" line in newer ZFS versions
            let trimmed = line.trim();
            if trimmed.starts_with("size") {
                let parts: Vec<&str> = trimmed.split_whitespace().collect();
                if parts.len() >= 2 {
                    return Ok(parse_zfs_size(parts[1]));
                }
            }
        }

        // If we can't parse the size, return 0 rather than error
        Ok(0)
    }

    /// Clone a ZFS dataset from a snapshot
    pub fn clone_from_snapshot(
        &self,
        dataset: &str,
        snap_name: &str,
        clone_name: &str,
    ) -> Result<(), ZfsError> {
        let snap_path = format!("{}/{}@{}", self.base_path(), dataset, snap_name);
        let clone_path = format!("{}/{}", self.base_path(), clone_name);

        let output = Command::new("zfs")
            .args(["clone", &snap_path, &clone_path])
            .output()?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(ZfsError::CloneError(format!(
                "zfs clone failed: {}",
                stderr
            )));
        }

        tracing::info!(
            zpool = %self.zpool,
            snapshot = %snap_path,
            clone = %clone_name,
            "Created ZFS clone"
        );
        Ok(())
    }

    /// Promote a ZFS clone to a standalone dataset
    pub fn promote_clone(&self, clone_name: &str) -> Result<(), ZfsError> {
        let clone_path = format!("{}/{}", self.base_path(), clone_name);

        let output = Command::new("zfs")
            .args(["promote", &clone_path])
            .output()?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(ZfsError::CloneError(format!(
                "zfs promote failed: {}",
                stderr
            )));
        }

        tracing::info!(zpool = %self.zpool, clone = %clone_name, "Promoted ZFS clone");
        Ok(())
    }

    /// Destroy all snapshots for a dataset before the specified snapshot, returning count destroyed
    pub fn destroy_snapshots_before(
        &self,
        dataset: &str,
        keep_snap: &str,
    ) -> Result<u32, ZfsError> {
        let snapshots = self.list_snapshots()?;
        let dataset_prefix = format!("{}/{}", self.base_path(), dataset);
        let keep_full = format!("{}@{}", dataset_prefix, keep_snap);

        let mut destroyed = 0u32;
        for snap in &snapshots {
            if !snap.name.starts_with(&dataset_prefix) {
                continue;
            }
            if snap.name == keep_full {
                break; // Stop once we reach the snap to keep
            }

            let output = Command::new("zfs").args(["destroy", &snap.name]).output()?;

            if output.status.success() {
                destroyed += 1;
                tracing::debug!(snapshot = %snap.name, "Destroyed old snapshot");
            }
        }

        tracing::info!(
            dataset = %dataset,
            keep = %keep_snap,
            destroyed = destroyed,
            "Snapshot garbage collection completed"
        );
        Ok(destroyed)
    }

    /// Check for a common snapshot between local and remote datasets
    pub fn check_common_snapshot(
        &self,
        dataset: &str,
        target: &ZfsReplicationTarget,
    ) -> Result<Option<String>, ZfsError> {
        Self::validate_target(target)?;
        Self::validate_zfs_name(dataset, "Dataset")?;

        // List local snapshots
        let local_snaps = self.list_snapshots()?;
        let dataset_prefix = format!("{}/{}", self.base_path(), dataset);
        let local_names: Vec<String> = local_snaps
            .iter()
            .filter(|s| s.name.starts_with(&dataset_prefix))
            .filter_map(|s| s.name.split('@').nth(1).map(String::from))
            .collect();

        // List remote snapshots via SSH (using proper argument passing)
        let target_ds = Self::target_path(target, dataset);
        let ssh_args = Self::ssh_args(target);

        let output = Command::new(&ssh_args[0])
            .args(&ssh_args[1..])
            .args([
                "zfs", "list", "-t", "snapshot", "-H", "-o", "name", "-r", &target_ds,
            ])
            .output()?;

        if !output.status.success() {
            // Remote dataset might not exist yet — no common snapshot
            return Ok(None);
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        let remote_names: Vec<String> = stdout
            .lines()
            .filter_map(|l| l.split('@').nth(1).map(String::from))
            .collect();

        // Find the latest common snapshot (iterate local in reverse)
        for name in local_names.iter().rev() {
            if remote_names.contains(name) {
                return Ok(Some(name.clone()));
            }
        }

        Ok(None)
    }
}

/// Parse ZFS human-readable sizes (e.g., "1.5G", "500M", "2T") to bytes.
/// Falls back to 0 on parse failure.
fn parse_zfs_size(s: &str) -> u64 {
    let s = s.trim();
    if s == "-" || s == "0" {
        return 0;
    }

    // Try to parse as a plain number first
    if let Ok(v) = s.parse::<u64>() {
        return v;
    }

    let (num_str, multiplier) = if let Some(n) = s.strip_suffix('T') {
        (n, 1024u64 * 1024 * 1024 * 1024)
    } else if let Some(n) = s.strip_suffix('G') {
        (n, 1024u64 * 1024 * 1024)
    } else if let Some(n) = s.strip_suffix('M') {
        (n, 1024u64 * 1024)
    } else if let Some(n) = s.strip_suffix('K') {
        (n, 1024u64)
    } else if let Some(n) = s.strip_suffix('B') {
        (n, 1u64)
    } else {
        (s, 1u64)
    };

    num_str
        .parse::<f64>()
        .map(|v| (v * multiplier as f64) as u64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_zfs_size() {
        assert_eq!(parse_zfs_size("0"), 0);
        assert_eq!(parse_zfs_size("-"), 0);
        assert_eq!(parse_zfs_size("1024"), 1024);
        assert_eq!(parse_zfs_size("1K"), 1024);
        assert_eq!(parse_zfs_size("1M"), 1024 * 1024);
        assert_eq!(parse_zfs_size("1G"), 1024 * 1024 * 1024);
        assert_eq!(parse_zfs_size("1T"), 1024u64 * 1024 * 1024 * 1024);
        assert_eq!(
            parse_zfs_size("1.5G"),
            (1.5 * 1024.0 * 1024.0 * 1024.0) as u64
        );
    }
}
