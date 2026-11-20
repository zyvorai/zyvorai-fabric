// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: LicenseRef-Zyvor-Production-1.0

use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::process::Command;
use tokio::fs;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Volume {
    pub id: String,
    pub name: String,
    pub path: PathBuf,
    pub size_gb: u64,
    pub format: VolumeFormat,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum VolumeFormat {
    Qcow2,
    Raw,
    Vmdk,
    Vdi,
}

impl VolumeFormat {
    pub fn as_str(&self) -> &str {
        match self {
            VolumeFormat::Qcow2 => "qcow2",
            VolumeFormat::Raw => "raw",
            VolumeFormat::Vmdk => "vmdk",
            VolumeFormat::Vdi => "vdi",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Snapshot {
    pub id: String,
    pub volume_id: String,
    pub name: String,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

pub struct VolumeManager {
    base_dir: PathBuf,
}

impl VolumeManager {
    pub fn new<P: AsRef<Path>>(base_dir: P) -> Result<Self> {
        let base_dir = base_dir.as_ref().to_path_buf();
        std::fs::create_dir_all(&base_dir)?;
        Ok(Self { base_dir })
    }

    /// Create a new volume
    pub async fn create_volume(
        &self,
        name: &str,
        size_gb: u64,
        format: VolumeFormat,
    ) -> Result<Volume> {
        let id = uuid::Uuid::new_v4().to_string();
        let filename = format!("{}.{}", name, format.as_str());
        let path = self.base_dir.join(&filename);

        // Create volume using qemu-img
        let output = Command::new("qemu-img")
            .arg("create")
            .arg("-f")
            .arg(format.as_str())
            .arg(&path)
            .arg(format!("{}G", size_gb))
            .output()?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(anyhow::anyhow!("Failed to create volume: {}", stderr));
        }

        let volume = Volume {
            id,
            name: name.to_string(),
            path,
            size_gb,
            format,
            created_at: chrono::Utc::now(),
        };

        tracing::info!("Created volume: {} ({}GB)", name, size_gb);

        Ok(volume)
    }

    /// Create a snapshot
    pub async fn create_snapshot(&self, volume: &Volume, snapshot_name: &str) -> Result<Snapshot> {
        let snapshot_id = uuid::Uuid::new_v4().to_string();

        // For qcow2, use internal snapshots
        if matches!(volume.format, VolumeFormat::Qcow2) {
            let output = Command::new("qemu-img")
                .arg("snapshot")
                .arg("-c")
                .arg(snapshot_name)
                .arg(&volume.path)
                .output()?;

            if !output.status.success() {
                let stderr = String::from_utf8_lossy(&output.stderr);
                return Err(anyhow::anyhow!("Failed to create snapshot: {}", stderr));
            }
        }

        let snapshot = Snapshot {
            id: snapshot_id,
            volume_id: volume.id.clone(),
            name: snapshot_name.to_string(),
            created_at: chrono::Utc::now(),
        };

        tracing::info!("Created snapshot: {} for volume {}", snapshot_name, volume.name);

        Ok(snapshot)
    }

    /// Clone a volume
    pub async fn clone_volume(&self, source: &Volume, new_name: &str) -> Result<Volume> {
        let id = uuid::Uuid::new_v4().to_string();
        let filename = format!("{}.{}", new_name, source.format.as_str());
        let dest_path = self.base_dir.join(&filename);

        // Clone using qemu-img convert
        let output = Command::new("qemu-img")
            .arg("convert")
            .arg("-O")
            .arg(source.format.as_str())
            .arg(&source.path)
            .arg(&dest_path)
            .output()?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(anyhow::anyhow!("Failed to clone volume: {}", stderr));
        }

        let volume = Volume {
            id,
            name: new_name.to_string(),
            path: dest_path,
            size_gb: source.size_gb,
            format: source.format.clone(),
            created_at: chrono::Utc::now(),
        };

        tracing::info!("Cloned volume: {} -> {}", source.name, new_name);

        Ok(volume)
    }

    /// Delete a volume
    pub async fn delete_volume(&self, volume: &Volume) -> Result<()> {
        if volume.path.exists() {
            fs::remove_file(&volume.path).await?;
            tracing::info!("Deleted volume: {}", volume.name);
        }
        Ok(())
    }

    /// Resize a volume
    pub async fn resize_volume(&self, volume: &Volume, new_size_gb: u64) -> Result<()> {
        let output = Command::new("qemu-img")
            .arg("resize")
            .arg(&volume.path)
            .arg(format!("{}G", new_size_gb))
            .output()?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(anyhow::anyhow!("Failed to resize volume: {}", stderr));
        }

        tracing::info!("Resized volume: {} to {}GB", volume.name, new_size_gb);

        Ok(())
    }

    /// Get volume info
    pub async fn get_volume_info(&self, volume: &Volume) -> Result<VolumeInfo> {
        let output = Command::new("qemu-img")
            .arg("info")
            .arg("--output=json")
            .arg(&volume.path)
            .output()?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(anyhow::anyhow!("Failed to get volume info: {}", stderr));
        }

        let info: VolumeInfo = serde_json::from_slice(&output.stdout)?;
        Ok(info)
    }
}

#[derive(Debug, Deserialize)]
pub struct VolumeInfo {
    pub format: String,
    #[serde(rename = "virtual-size")]
    pub virtual_size: u64,
    #[serde(rename = "actual-size")]
    pub actual_size: u64,
}
