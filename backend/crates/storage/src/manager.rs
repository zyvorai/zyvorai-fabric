// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: LicenseRef-Zyvor-Production-2.0

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use thiserror::Error;
use tokio::sync::RwLock;

use crate::ceph::{CephError, CephPool};
use crate::lvm::{LvmError, LvmPool};
use crate::nfs::{NfsConfig, NfsError, NfsHealth, NfsPool, NfsStats};
use crate::pool::{PoolState, StoragePool, StoragePoolType};
use crate::zfs::{ZfsError, ZfsPool};

#[derive(Debug, Error)]
pub enum StorageError {
    #[error("Pool not found: {0}")]
    PoolNotFound(String),

    #[error("Pool already exists: {0}")]
    PoolExists(String),

    #[error("NFS error: {0}")]
    Nfs(#[from] NfsError),

    #[error("LVM error: {0}")]
    Lvm(#[from] LvmError),

    #[error("ZFS error: {0}")]
    Zfs(#[from] ZfsError),

    #[error("Ceph error: {0}")]
    Ceph(#[from] CephError),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),

    #[error("Pool is not active: {0}")]
    PoolNotActive(String),

    #[error("Invalid pool type: {0}")]
    InvalidPoolType(String),
}

pub struct StorageManager {
    pools: Arc<RwLock<HashMap<String, StoragePool>>>,
    nfs_pools: Arc<RwLock<HashMap<String, NfsPool>>>,
    state_file: PathBuf,
}

impl StorageManager {
    pub fn new(state_dir: &Path) -> Result<Self, StorageError> {
        fs::create_dir_all(state_dir)?;
        let state_file = state_dir.join("storage_pools.json");

        let manager = Self {
            pools: Arc::new(RwLock::new(HashMap::new())),
            nfs_pools: Arc::new(RwLock::new(HashMap::new())),
            state_file,
        };

        // Load existing pools
        if manager.state_file.exists() {
            manager.load_state()?;
        }

        Ok(manager)
    }

    /// Create a new local storage pool
    pub async fn create_local_pool(
        &self,
        name: String,
        path: PathBuf,
        auto_start: bool,
    ) -> Result<StoragePool, StorageError> {
        let mut pools = self.pools.write().await;

        if pools.contains_key(&name) {
            return Err(StorageError::PoolExists(name));
        }

        // Create directory if it doesn't exist
        if !path.exists() {
            fs::create_dir_all(&path)?;
        }

        let mut pool = StoragePool::new(name.clone(), StoragePoolType::Local, path.clone());
        pool.auto_start = auto_start;
        pool.state = PoolState::Active;

        // Get disk stats
        self.update_pool_stats(&mut pool)?;

        pools.insert(name.clone(), pool.clone());
        self.save_state(&pools)?;

        Ok(pool)
    }

    /// Create a new directory storage pool
    pub async fn create_directory_pool(
        &self,
        name: String,
        path: PathBuf,
        auto_start: bool,
    ) -> Result<StoragePool, StorageError> {
        let mut pools = self.pools.write().await;

        if pools.contains_key(&name) {
            return Err(StorageError::PoolExists(name));
        }

        if !path.exists() {
            fs::create_dir_all(&path)?;
        }

        let mut pool = StoragePool::new(
            name.clone(),
            StoragePoolType::Directory { path: path.clone() },
            path.clone(),
        );
        pool.auto_start = auto_start;
        pool.state = PoolState::Active;

        self.update_pool_stats(&mut pool)?;

        pools.insert(name.clone(), pool.clone());
        self.save_state(&pools)?;

        Ok(pool)
    }

    /// Create a new NFS storage pool
    pub async fn create_nfs_pool(
        &self,
        name: String,
        config: NfsConfig,
    ) -> Result<StoragePool, StorageError> {
        let mut pools = self.pools.write().await;
        let mut nfs_pools = self.nfs_pools.write().await;

        if pools.contains_key(&name) {
            return Err(StorageError::PoolExists(name));
        }

        // Create and mount NFS pool
        let mut nfs_pool = NfsPool::new(config.clone())?;

        // Check if server is reachable
        if !nfs_pool.check_server()? {
            return Err(StorageError::Nfs(NfsError::ServerUnreachable(
                config.server.clone(),
            )));
        }

        // Mount the NFS share
        nfs_pool.mount()?;

        // Create storage pool entry
        let pool_type = StoragePoolType::NFS {
            server: config.server.clone(),
            export_path: config.export_path.clone(),
            mount_options: config.mount_options.clone(),
        };

        let mut pool = StoragePool::new(name.clone(), pool_type, config.mount_path.clone());
        pool.auto_start = config.auto_start;
        pool.state = PoolState::Active;

        // Get NFS stats
        if let Ok(stats) = nfs_pool.get_stats() {
            pool.update_stats(stats.total_kb * 1024, stats.available_kb * 1024);
        }

        pools.insert(name.clone(), pool.clone());
        nfs_pools.insert(name.clone(), nfs_pool);
        self.save_state(&pools)?;

        Ok(pool)
    }

    /// Create a new LVM storage pool
    pub async fn create_lvm_pool(
        &self,
        name: String,
        volume_group: String,
        auto_start: bool,
    ) -> Result<StoragePool, StorageError> {
        let mut pools = self.pools.write().await;

        if pools.contains_key(&name) {
            return Err(StorageError::PoolExists(name));
        }

        // Validate VG exists
        let lvm_pool = LvmPool::new(&volume_group)?;
        let stats = lvm_pool.get_stats()?;

        let device_path = PathBuf::from(format!("/dev/{}", volume_group));
        let mut pool = StoragePool::new(
            name.clone(),
            StoragePoolType::LVM { volume_group },
            device_path,
        );
        pool.auto_start = auto_start;
        pool.state = PoolState::Active;
        pool.update_stats(stats.vg_size_bytes, stats.vg_free_bytes);

        pools.insert(name.clone(), pool.clone());
        self.save_state(&pools)?;

        Ok(pool)
    }

    /// Create a new LVM thin-provisioning storage pool
    pub async fn create_lvm_thin_pool(
        &self,
        name: String,
        volume_group: String,
        thin_pool: String,
        auto_start: bool,
    ) -> Result<StoragePool, StorageError> {
        let mut pools = self.pools.write().await;

        if pools.contains_key(&name) {
            return Err(StorageError::PoolExists(name));
        }

        // Validate VG exists
        let lvm_pool = LvmPool::new(&volume_group)?;
        let stats = lvm_pool.get_stats()?;

        let device_path = PathBuf::from(format!("/dev/{}", volume_group));
        let mut pool = StoragePool::new(
            name.clone(),
            StoragePoolType::LVMThin {
                volume_group,
                thin_pool,
            },
            device_path,
        );
        pool.auto_start = auto_start;
        pool.state = PoolState::Active;
        pool.update_stats(stats.vg_size_bytes, stats.vg_free_bytes);

        pools.insert(name.clone(), pool.clone());
        self.save_state(&pools)?;

        Ok(pool)
    }

    /// Create a new ZFS storage pool
    pub async fn create_zfs_pool(
        &self,
        name: String,
        zpool: String,
        dataset: Option<String>,
        auto_start: bool,
    ) -> Result<StoragePool, StorageError> {
        let mut pools = self.pools.write().await;

        if pools.contains_key(&name) {
            return Err(StorageError::PoolExists(name));
        }

        // Validate zpool exists
        let zfs_pool = ZfsPool::new(&zpool, dataset.clone())?;
        let stats = zfs_pool.get_stats()?;

        let mount_path = match &dataset {
            Some(ds) => PathBuf::from(format!("/{}/{}", zpool, ds)),
            None => PathBuf::from(format!("/{}", zpool)),
        };

        let mut pool = StoragePool::new(
            name.clone(),
            StoragePoolType::ZFS { zpool, dataset },
            mount_path,
        );
        pool.auto_start = auto_start;
        pool.state = PoolState::Active;
        pool.update_stats(stats.size_bytes, stats.free_bytes);

        pools.insert(name.clone(), pool.clone());
        self.save_state(&pools)?;

        Ok(pool)
    }

    /// Create a new Ceph RBD storage pool
    pub async fn create_ceph_pool(
        &self,
        name: String,
        monitors: Vec<String>,
        ceph_pool_name: String,
        user: Option<String>,
        keyring: Option<String>,
        auto_start: bool,
    ) -> Result<StoragePool, StorageError> {
        let mut pools = self.pools.write().await;

        if pools.contains_key(&name) {
            return Err(StorageError::PoolExists(name));
        }

        let mut ceph_pool = CephPool::new(monitors.clone(), ceph_pool_name.clone())?;
        if let Some(user) = user {
            ceph_pool = ceph_pool.with_auth(user, keyring.unwrap_or_default());
        }

        // Verify cluster is reachable
        if !ceph_pool.check_cluster()? {
            return Err(StorageError::Ceph(CephError::ClusterUnreachable(
                monitors.join(","),
            )));
        }

        let stats = ceph_pool.get_stats()?;

        let mut pool = StoragePool::new(
            name.clone(),
            StoragePoolType::Ceph {
                monitors,
                pool_name: ceph_pool_name,
            },
            std::path::PathBuf::from("/dev/rbd"),
        );
        pool.auto_start = auto_start;
        pool.state = PoolState::Active;
        pool.update_stats(stats.total_bytes, stats.available_bytes);

        pools.insert(name.clone(), pool.clone());
        self.save_state(&pools)?;

        Ok(pool)
    }

    /// Delete a storage pool
    pub async fn delete_pool(&self, name: &str) -> Result<(), StorageError> {
        let mut pools = self.pools.write().await;
        let mut nfs_pools = self.nfs_pools.write().await;

        let pool = pools
            .get(name)
            .ok_or_else(|| StorageError::PoolNotFound(name.to_string()))?;

        // If it's an NFS pool, unmount it first
        if pool.is_nfs() {
            if let Some(mut nfs_pool) = nfs_pools.remove(name) {
                nfs_pool.unmount()?;
            }
        }

        pools.remove(name);
        self.save_state(&pools)?;

        Ok(())
    }

    /// Start a storage pool
    pub async fn start_pool(&self, name: &str) -> Result<(), StorageError> {
        let mut pools = self.pools.write().await;
        let mut nfs_pools = self.nfs_pools.write().await;

        let pool = pools
            .get_mut(name)
            .ok_or_else(|| StorageError::PoolNotFound(name.to_string()))?;

        if pool.state == PoolState::Active {
            return Ok(());
        }

        pool.state = PoolState::Starting;

        // If it's an NFS pool, mount it
        if pool.is_nfs() {
            if let Some(nfs_pool) = nfs_pools.get_mut(name) {
                nfs_pool.mount()?;

                // Update stats
                if let Ok(stats) = nfs_pool.get_stats() {
                    pool.update_stats(stats.total_kb * 1024, stats.available_kb * 1024);
                }
            }
        }

        pool.state = PoolState::Active;
        self.save_state(&pools)?;

        Ok(())
    }

    /// Stop a storage pool
    pub async fn stop_pool(&self, name: &str) -> Result<(), StorageError> {
        let mut pools = self.pools.write().await;
        let mut nfs_pools = self.nfs_pools.write().await;

        let pool = pools
            .get_mut(name)
            .ok_or_else(|| StorageError::PoolNotFound(name.to_string()))?;

        if pool.state == PoolState::Inactive {
            return Ok(());
        }

        pool.state = PoolState::Stopping;

        // If it's an NFS pool, unmount it
        if pool.is_nfs() {
            if let Some(nfs_pool) = nfs_pools.get_mut(name) {
                nfs_pool.unmount()?;
            }
        }

        pool.state = PoolState::Inactive;
        self.save_state(&pools)?;

        Ok(())
    }

    /// Get a storage pool
    pub async fn get_pool(&self, name: &str) -> Result<StoragePool, StorageError> {
        let pools = self.pools.read().await;
        pools
            .get(name)
            .cloned()
            .ok_or_else(|| StorageError::PoolNotFound(name.to_string()))
    }

    /// List all storage pools
    pub async fn list_pools(&self) -> Vec<StoragePool> {
        let pools = self.pools.read().await;
        pools.values().cloned().collect()
    }

    /// Get NFS pool health
    pub async fn get_nfs_health(&self, name: &str) -> Result<NfsHealth, StorageError> {
        let nfs_pools = self.nfs_pools.read().await;
        let nfs_pool = nfs_pools
            .get(name)
            .ok_or_else(|| StorageError::PoolNotFound(name.to_string()))?;

        Ok(nfs_pool.health_check()?)
    }

    /// Get NFS pool stats
    pub async fn get_nfs_stats(&self, name: &str) -> Result<NfsStats, StorageError> {
        let nfs_pools = self.nfs_pools.read().await;
        let nfs_pool = nfs_pools
            .get(name)
            .ok_or_else(|| StorageError::PoolNotFound(name.to_string()))?;

        Ok(nfs_pool.get_stats()?)
    }

    /// Update pool statistics
    pub async fn refresh_pool_stats(&self, name: &str) -> Result<(), StorageError> {
        let mut pools = self.pools.write().await;
        let nfs_pools = self.nfs_pools.read().await;

        let pool = pools
            .get_mut(name)
            .ok_or_else(|| StorageError::PoolNotFound(name.to_string()))?;

        if pool.is_nfs() {
            if let Some(nfs_pool) = nfs_pools.get(name) {
                if let Ok(stats) = nfs_pool.get_stats() {
                    pool.update_stats(stats.total_kb * 1024, stats.available_kb * 1024);
                }
            }
        } else {
            self.update_pool_stats(pool)?;
        }

        Ok(())
    }

    fn update_pool_stats(&self, pool: &mut StoragePool) -> Result<(), StorageError> {
        let path = pool.path.to_str().ok_or_else(|| {
            StorageError::Io(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "storage path is not valid UTF-8",
            ))
        })?;
        let path = input_guard::vet!(
            path,
            StorageError::Io(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "rejected unsafe storage path",
            ))
        );
        let output = input_guard::argv_checked!(
            path,
            StorageError::Io(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "rejected unsafe storage path",
            )),
            |path| {
                std::process::Command::new("df")
                    .args(["-k", path])
                    .output()?
            }
        );

        let df_output = String::from_utf8_lossy(&output.stdout);
        let lines: Vec<&str> = df_output.lines().collect();

        if lines.len() >= 2 {
            let parts: Vec<&str> = lines[1].split_whitespace().collect();
            if parts.len() >= 4 {
                let total_kb: u64 = parts[1].parse().unwrap_or(0);
                let available_kb: u64 = parts[3].parse().unwrap_or(0);

                pool.update_stats(total_kb * 1024, available_kb * 1024);
            }
        }

        Ok(())
    }

    fn save_state(&self, pools: &HashMap<String, StoragePool>) -> Result<(), StorageError> {
        let json = serde_json::to_string_pretty(pools)?;
        fs::write(&self.state_file, json)?;
        Ok(())
    }

    fn load_state(&self) -> Result<(), StorageError> {
        let json = fs::read_to_string(&self.state_file)?;
        let saved_pools: HashMap<String, StoragePool> = serde_json::from_str(&json)?;

        // Restore NFS pools on startup
        for (name, pool) in saved_pools {
            tracing::info!("Restoring storage pool: {}", name);

            match &pool.pool_type {
                StoragePoolType::NFS {
                    server,
                    export_path,
                    mount_options,
                } => {
                    // Create NfsConfig from saved pool data
                    let nfs_config = NfsConfig {
                        server: server.clone(),
                        export_path: export_path.clone(),
                        mount_path: pool.path.clone(),
                        mount_options: mount_options.clone(),
                        auto_start: true, // Assume auto-start for restored pools
                        nfs_version: crate::nfs::NfsVersion::V4, // Default to NFSv4
                    };

                    match NfsPool::new(nfs_config) {
                        Ok(mut nfs_pool) => {
                            // Attempt to mount the NFS pool
                            match nfs_pool.mount() {
                                Ok(_) => {
                                    tracing::info!(
                                        "NFS pool '{}' mounted successfully on startup",
                                        name
                                    );
                                }
                                Err(e) => {
                                    tracing::warn!(
                                        "Failed to mount NFS pool '{}' on startup: {}",
                                        name,
                                        e
                                    );
                                    // Continue loading other pools even if one fails
                                }
                            }
                        }
                        Err(e) => {
                            tracing::warn!("Failed to create NFS pool '{}': {}", name, e);
                        }
                    }
                }
                StoragePoolType::Local | StoragePoolType::Directory { .. } => {
                    tracing::info!("Local storage pool '{}' restored", name);
                }
                StoragePoolType::Ceph {
                    ref monitors,
                    ref pool_name,
                } => {
                    tracing::info!(
                        "Ceph storage pool '{}' restored (monitors: {}, pool: {})",
                        name,
                        monitors.join(","),
                        pool_name
                    );
                }
                StoragePoolType::LVM { ref volume_group } => {
                    tracing::info!(
                        "LVM storage pool '{}' restored (VG: {})",
                        name,
                        volume_group
                    );
                }
                StoragePoolType::LVMThin {
                    ref volume_group,
                    ref thin_pool,
                } => {
                    tracing::info!(
                        "LVM thin storage pool '{}' restored (VG: {}, thin: {})",
                        name,
                        volume_group,
                        thin_pool
                    );
                }
                StoragePoolType::ZFS {
                    ref zpool,
                    ref dataset,
                } => {
                    tracing::info!(
                        "ZFS storage pool '{}' restored (zpool: {}, dataset: {:?})",
                        name,
                        zpool,
                        dataset
                    );
                }
            }
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[tokio::test]
    async fn test_create_local_pool() {
        let temp_dir = tempdir().unwrap();
        let state_dir = temp_dir.path().to_path_buf();
        let pool_path = state_dir.join("pool1");

        let manager = StorageManager::new(&state_dir).unwrap();

        let result = manager
            .create_local_pool("test-pool".to_string(), pool_path.clone(), true)
            .await;

        assert!(result.is_ok());
        let pool = result.unwrap();
        assert_eq!(pool.name, "test-pool");
        assert!(pool.is_active());
        assert!(pool_path.exists());
    }

    #[tokio::test]
    async fn test_duplicate_pool() {
        let temp_dir = tempdir().unwrap();
        let state_dir = temp_dir.path().to_path_buf();
        let pool_path = state_dir.join("pool1");

        let manager = StorageManager::new(&state_dir).unwrap();

        manager
            .create_local_pool("test-pool".to_string(), pool_path.clone(), true)
            .await
            .unwrap();

        let result = manager
            .create_local_pool("test-pool".to_string(), pool_path, true)
            .await;

        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_list_pools() {
        let temp_dir = tempdir().unwrap();
        let state_dir = temp_dir.path().to_path_buf();

        let manager = StorageManager::new(&state_dir).unwrap();

        manager
            .create_local_pool("pool1".to_string(), state_dir.join("pool1"), true)
            .await
            .unwrap();

        manager
            .create_directory_pool("pool2".to_string(), state_dir.join("pool2"), true)
            .await
            .unwrap();

        let pools = manager.list_pools().await;
        assert_eq!(pools.len(), 2);
    }
}
