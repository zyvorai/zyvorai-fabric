// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: LicenseRef-Zyvor-Production-2.0

use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use uuid::Uuid;

use crate::nfs::NfsConfig;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum StoragePoolType {
    Local,
    Directory {
        path: PathBuf,
    },
    NFS {
        server: String,
        export_path: String,
        mount_options: Vec<String>,
    },
    #[allow(dead_code)]
    Ceph {
        monitors: Vec<String>,
        pool_name: String,
    },
    LVM {
        volume_group: String,
    },
    LVMThin {
        volume_group: String,
        thin_pool: String,
    },
    ZFS {
        zpool: String,
        dataset: Option<String>,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoragePool {
    pub id: String,
    pub name: String,
    pub pool_type: StoragePoolType,
    pub path: PathBuf,
    pub capacity: u64,
    pub available: u64,
    pub state: PoolState,
    pub auto_start: bool,
    pub created: chrono::DateTime<chrono::Utc>,
    pub updated: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum PoolState {
    Inactive,
    Starting,
    Active,
    Stopping,
    Degraded,
    Failed,
}

impl StoragePool {
    pub fn new(name: String, pool_type: StoragePoolType, path: PathBuf) -> Self {
        let now = chrono::Utc::now();
        Self {
            id: Uuid::new_v4().to_string(),
            name,
            pool_type,
            path,
            capacity: 0,
            available: 0,
            state: PoolState::Inactive,
            auto_start: false,
            created: now,
            updated: now,
        }
    }

    pub fn is_active(&self) -> bool {
        self.state == PoolState::Active
    }

    pub fn is_nfs(&self) -> bool {
        matches!(self.pool_type, StoragePoolType::NFS { .. })
    }

    pub fn is_local(&self) -> bool {
        matches!(
            self.pool_type,
            StoragePoolType::Local | StoragePoolType::Directory { .. }
        )
    }

    pub fn is_lvm(&self) -> bool {
        matches!(
            self.pool_type,
            StoragePoolType::LVM { .. } | StoragePoolType::LVMThin { .. }
        )
    }

    pub fn is_zfs(&self) -> bool {
        matches!(self.pool_type, StoragePoolType::ZFS { .. })
    }

    pub fn usage_percent(&self) -> f64 {
        if self.capacity == 0 {
            return 0.0;
        }
        ((self.capacity - self.available) as f64 / self.capacity as f64) * 100.0
    }

    pub fn update_stats(&mut self, capacity: u64, available: u64) {
        self.capacity = capacity;
        self.available = available;
        self.updated = chrono::Utc::now();
    }
}

impl From<NfsConfig> for StoragePoolType {
    fn from(config: NfsConfig) -> Self {
        StoragePoolType::NFS {
            server: config.server,
            export_path: config.export_path,
            mount_options: config.mount_options,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_storage_pool_creation() {
        let pool = StoragePool::new(
            "test-pool".to_string(),
            StoragePoolType::Local,
            PathBuf::from("/var/lib/zyvor-fabricd/storage"),
        );

        assert_eq!(pool.name, "test-pool");
        assert_eq!(pool.state, PoolState::Inactive);
        assert!(!pool.is_active());
    }

    #[test]
    fn test_usage_percent() {
        let mut pool = StoragePool::new(
            "test".to_string(),
            StoragePoolType::Local,
            PathBuf::from("/tmp"),
        );

        pool.update_stats(100, 50);
        assert_eq!(pool.usage_percent(), 50.0);
    }

    #[test]
    fn test_pool_type_checks() {
        let local_pool = StoragePool::new(
            "local".to_string(),
            StoragePoolType::Local,
            PathBuf::from("/tmp"),
        );
        assert!(local_pool.is_local());
        assert!(!local_pool.is_nfs());

        let nfs_pool = StoragePool::new(
            "nfs".to_string(),
            StoragePoolType::NFS {
                server: "192.168.1.1".to_string(),
                export_path: "/export".to_string(),
                mount_options: vec![],
            },
            PathBuf::from("/mnt/nfs"),
        );
        assert!(nfs_pool.is_nfs());
        assert!(!nfs_pool.is_local());
    }
}
