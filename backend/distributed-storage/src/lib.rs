// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: LicenseRef-Zyvor-Production-1.0

use anyhow::{anyhow, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use uuid::Uuid;

// ---------------------------------------------------------------------------
// Enums
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PoolStatus {
    Online,
    Degraded,
    Rebuilding,
    Offline,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PoolHealth {
    Healthy,
    Warning,
    Critical,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DiskType {
    Ssd,
    Hdd,
    Nvme,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DiskStatus {
    Active,
    Failed,
    Rebuilding,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StorageTier {
    Gold,
    Silver,
    Bronze,
    Custom(String),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MigrationStatus {
    Pending,
    InProgress,
    Completed,
    Failed,
    Cancelled,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AutomationLevel {
    Manual,
    FullyAutomated,
}

// ---------------------------------------------------------------------------
// Data models
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiskContribution {
    pub disk_id: String,
    pub path: String,
    pub capacity_gb: u64,
    pub disk_type: DiskType,
    pub status: DiskStatus,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StorageHost {
    pub host_id: String,
    pub hostname: String,
    pub disks: Vec<DiskContribution>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FaultDomain {
    pub id: String,
    pub name: String,
    pub host_ids: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DistributedStoragePool {
    pub id: String,
    pub name: String,
    pub cluster_id: String,
    pub hosts: Vec<StorageHost>,
    pub replication_factor: u8,
    pub erasure_coding: bool,
    pub fault_domains: Vec<FaultDomain>,
    pub total_capacity_gb: u64,
    pub used_capacity_gb: u64,
    pub free_capacity_gb: u64,
    pub status: PoolStatus,
    pub health: PoolHealth,
    pub created: DateTime<Utc>,
    pub updated: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoragePolicy {
    pub id: String,
    pub name: String,
    pub description: String,
    pub replication_factor: u8,
    pub disk_type_required: Option<DiskType>,
    pub encryption_required: bool,
    pub iops_limit: Option<u64>,
    pub throughput_limit_mbps: Option<u64>,
    pub tier: StorageTier,
    pub created: DateTime<Utc>,
    pub updated: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StorageMigration {
    pub id: String,
    pub vm_name: String,
    pub source_pool_id: String,
    pub target_pool_id: String,
    pub disk_size_gb: u64,
    pub bytes_transferred: u64,
    pub progress_pct: f64,
    pub status: MigrationStatus,
    pub started: Option<DateTime<Utc>>,
    pub completed: Option<DateTime<Utc>>,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DatastoreCluster {
    pub id: String,
    pub name: String,
    pub cluster_id: String,
    pub datastore_ids: Vec<String>,
    pub storage_drs_enabled: bool,
    pub space_threshold_pct: u8,
    pub io_latency_threshold_ms: Option<u32>,
    pub automation_level: AutomationLevel,
    pub created: DateTime<Utc>,
    pub updated: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComplianceReport {
    pub vm_name: String,
    pub policy_id: String,
    pub policy_name: String,
    pub compliant: bool,
    pub violations: Vec<String>,
    pub checked_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PoolHealthReport {
    pub pool_id: String,
    pub status: PoolStatus,
    pub health: PoolHealth,
    pub failed_disks: u32,
    pub rebuilding_disks: u32,
    pub capacity_used_pct: f64,
}

// ---------------------------------------------------------------------------
// Request structs
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreatePoolRequest {
    pub name: String,
    pub cluster_id: String,
    pub hosts: Vec<StorageHost>,
    pub replication_factor: u8,
    pub erasure_coding: bool,
    pub fault_domains: Vec<FaultDomain>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateDatastoreClusterRequest {
    pub name: String,
    pub cluster_id: String,
    pub datastore_ids: Vec<String>,
    pub storage_drs_enabled: bool,
    pub space_threshold_pct: u8,
    pub io_latency_threshold_ms: Option<u32>,
    pub automation_level: AutomationLevel,
}

// ---------------------------------------------------------------------------
// In-memory store
// ---------------------------------------------------------------------------

#[derive(Debug, Default)]
struct Store {
    pools: HashMap<String, DistributedStoragePool>,
    policies: HashMap<String, StoragePolicy>,
    migrations: HashMap<String, StorageMigration>,
    datastore_clusters: HashMap<String, DatastoreCluster>,
}

// ---------------------------------------------------------------------------
// DistributedStorageManager
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct DistributedStorageManager {
    store: Arc<RwLock<Store>>,
}

impl DistributedStorageManager {
    /// Create a new DistributedStorageManager with empty in-memory stores.
    pub fn new() -> Self {
        Self {
            store: Arc::new(RwLock::new(Store::default())),
        }
    }

    // -- Pool management -----------------------------------------------------

    /// Create a new distributed storage pool.
    pub fn create_pool(&self, req: CreatePoolRequest) -> Result<DistributedStoragePool> {
        if req.replication_factor == 0 || req.replication_factor > 3 {
            return Err(anyhow!(
                "replication_factor must be 1, 2, or 3; got {}",
                req.replication_factor
            ));
        }

        let total_capacity_gb: u64 = req
            .hosts
            .iter()
            .flat_map(|h| h.disks.iter())
            .map(|d| d.capacity_gb)
            .sum();

        let now = Utc::now();
        let pool = DistributedStoragePool {
            id: Uuid::new_v4().to_string(),
            name: req.name,
            cluster_id: req.cluster_id,
            hosts: req.hosts,
            replication_factor: req.replication_factor,
            erasure_coding: req.erasure_coding,
            fault_domains: req.fault_domains,
            total_capacity_gb,
            used_capacity_gb: 0,
            free_capacity_gb: total_capacity_gb,
            status: PoolStatus::Online,
            health: PoolHealth::Healthy,
            created: now,
            updated: now,
        };

        let mut store = self
            .store
            .write()
            .map_err(|e| anyhow!("lock poisoned: {e}"))?;
        tracing::info!(id = %pool.id, name = %pool.name, "created distributed storage pool");
        store.pools.insert(pool.id.clone(), pool.clone());
        Ok(pool)
    }

    /// Retrieve a pool by ID.
    pub fn get_pool(&self, id: &str) -> Option<DistributedStoragePool> {
        let store = self.store.read().unwrap_or_else(|e| e.into_inner());
        store.pools.get(id).cloned()
    }

    /// List pools, optionally filtered by cluster ID.
    pub fn list_pools(&self, cluster_id: Option<&str>) -> Vec<DistributedStoragePool> {
        let store = self.store.read().unwrap_or_else(|e| e.into_inner());
        store
            .pools
            .values()
            .filter(|p| match cluster_id {
                Some(cid) => p.cluster_id == cid,
                None => true,
            })
            .cloned()
            .collect()
    }

    /// Delete a pool by ID.
    pub fn delete_pool(&self, id: &str) -> Result<()> {
        let mut store = self
            .store
            .write()
            .map_err(|e| anyhow!("lock poisoned: {e}"))?;
        store
            .pools
            .remove(id)
            .ok_or_else(|| anyhow!("pool not found: {id}"))?;
        tracing::info!(id = %id, "deleted distributed storage pool");
        Ok(())
    }

    /// Add a host to an existing pool and recalculate capacity.
    pub fn add_host_to_pool(&self, pool_id: &str, host: StorageHost) -> Result<()> {
        let mut store = self
            .store
            .write()
            .map_err(|e| anyhow!("lock poisoned: {e}"))?;
        let pool = store
            .pools
            .get_mut(pool_id)
            .ok_or_else(|| anyhow!("pool not found: {pool_id}"))?;

        let added_capacity: u64 = host.disks.iter().map(|d| d.capacity_gb).sum();
        pool.hosts.push(host);
        pool.total_capacity_gb += added_capacity;
        pool.free_capacity_gb += added_capacity;
        pool.updated = Utc::now();

        tracing::info!(pool_id = %pool_id, added_gb = added_capacity, "added host to pool");
        Ok(())
    }

    /// Remove a host from a pool and recalculate capacity.
    pub fn remove_host_from_pool(&self, pool_id: &str, host_id: &str) -> Result<()> {
        let mut store = self
            .store
            .write()
            .map_err(|e| anyhow!("lock poisoned: {e}"))?;
        let pool = store
            .pools
            .get_mut(pool_id)
            .ok_or_else(|| anyhow!("pool not found: {pool_id}"))?;

        let idx = pool
            .hosts
            .iter()
            .position(|h| h.host_id == host_id)
            .ok_or_else(|| anyhow!("host not found in pool: {host_id}"))?;

        let removed = pool.hosts.remove(idx);
        let removed_capacity: u64 = removed.disks.iter().map(|d| d.capacity_gb).sum();
        pool.total_capacity_gb = pool.total_capacity_gb.saturating_sub(removed_capacity);
        pool.free_capacity_gb = pool.free_capacity_gb.saturating_sub(removed_capacity);
        pool.updated = Utc::now();

        tracing::info!(pool_id = %pool_id, host_id = %host_id, "removed host from pool");
        Ok(())
    }

    /// Report a disk failure within a pool, updating the disk status and pool health.
    pub fn report_disk_failure(&self, pool_id: &str, host_id: &str, disk_id: &str) -> Result<()> {
        let mut store = self
            .store
            .write()
            .map_err(|e| anyhow!("lock poisoned: {e}"))?;
        let pool = store
            .pools
            .get_mut(pool_id)
            .ok_or_else(|| anyhow!("pool not found: {pool_id}"))?;

        let host = pool
            .hosts
            .iter_mut()
            .find(|h| h.host_id == host_id)
            .ok_or_else(|| anyhow!("host not found in pool: {host_id}"))?;

        let disk = host
            .disks
            .iter_mut()
            .find(|d| d.disk_id == disk_id)
            .ok_or_else(|| anyhow!("disk not found: {disk_id}"))?;

        disk.status = DiskStatus::Failed;

        // Recalculate pool health based on failed/rebuilding disks.
        let (failed, rebuilding, _total) = count_disk_states(pool);
        if failed > 0 {
            pool.status = PoolStatus::Degraded;
        }
        if failed as u64 >= pool.total_capacity_gb / 100 || failed > 2 {
            pool.health = PoolHealth::Critical;
        } else if failed > 0 {
            pool.health = PoolHealth::Warning;
        }
        pool.updated = Utc::now();

        tracing::warn!(
            pool_id = %pool_id,
            host_id = %host_id,
            disk_id = %disk_id,
            failed_disks = failed,
            rebuilding_disks = rebuilding,
            "disk failure reported"
        );
        Ok(())
    }

    /// Get a health report for a pool.
    pub fn get_pool_health(&self, pool_id: &str) -> Result<PoolHealthReport> {
        let store = self
            .store
            .read()
            .map_err(|e| anyhow!("lock poisoned: {e}"))?;
        let pool = store
            .pools
            .get(pool_id)
            .ok_or_else(|| anyhow!("pool not found: {pool_id}"))?;

        let (failed, rebuilding, _total) = count_disk_states(pool);
        let capacity_used_pct = if pool.total_capacity_gb > 0 {
            (pool.used_capacity_gb as f64 / pool.total_capacity_gb as f64) * 100.0
        } else {
            0.0
        };

        Ok(PoolHealthReport {
            pool_id: pool.id.clone(),
            status: pool.status.clone(),
            health: pool.health.clone(),
            failed_disks: failed,
            rebuilding_disks: rebuilding,
            capacity_used_pct,
        })
    }

    // -- Storage vMotion (migrations) ----------------------------------------

    /// Start a storage migration (storage vMotion) for a VM between pools.
    ///
    /// In addition to tracking migration metadata, this method attempts the
    /// actual data movement using `qemu-img convert` if the source disk image
    /// exists on the filesystem. The migration copies the VM's qcow2 image
    /// from the source pool directory to the target pool directory. If
    /// `qemu-img` is not available or the source path does not exist, the
    /// migration is still recorded (metadata-only) so it can be completed or
    /// failed through the normal lifecycle methods.
    pub fn start_storage_migration(
        &self,
        vm_name: &str,
        source_pool: &str,
        target_pool: &str,
        disk_size_gb: u64,
    ) -> Result<StorageMigration> {
        let store_read = self
            .store
            .read()
            .map_err(|e| anyhow!("lock poisoned: {e}"))?;
        if !store_read.pools.contains_key(source_pool) {
            return Err(anyhow!("source pool not found: {source_pool}"));
        }
        if !store_read.pools.contains_key(target_pool) {
            return Err(anyhow!("target pool not found: {target_pool}"));
        }
        let target = &store_read.pools[target_pool];
        if target.free_capacity_gb < disk_size_gb {
            return Err(anyhow!(
                "insufficient space in target pool: need {} GB, have {} GB free",
                disk_size_gb,
                target.free_capacity_gb
            ));
        }
        drop(store_read);

        let now = Utc::now();
        let migration = StorageMigration {
            id: Uuid::new_v4().to_string(),
            vm_name: vm_name.to_string(),
            source_pool_id: source_pool.to_string(),
            target_pool_id: target_pool.to_string(),
            disk_size_gb,
            bytes_transferred: 0,
            progress_pct: 0.0,
            status: MigrationStatus::InProgress,
            started: Some(now),
            completed: None,
            error: None,
        };

        {
            let mut store = self
                .store
                .write()
                .map_err(|e| anyhow!("lock poisoned: {e}"))?;
            tracing::info!(
                id = %migration.id,
                vm = %vm_name,
                source = %source_pool,
                target = %target_pool,
                size_gb = disk_size_gb,
                "started storage migration"
            );
            store
                .migrations
                .insert(migration.id.clone(), migration.clone());
        }

        // Attempt the actual data movement using qemu-img convert.
        let source_path = format!(
            "/var/lib/zyvor-fabricd/storage/{}/{}.qcow2",
            source_pool, vm_name
        );
        let dest_dir = format!("/var/lib/zyvor-fabricd/storage/{}", target_pool);
        let dest_path = format!("{}/{}.qcow2", dest_dir, vm_name);

        if std::path::Path::new(&source_path).exists() {
            // Ensure the destination directory exists
            if let Err(e) = std::fs::create_dir_all(&dest_dir) {
                tracing::error!(
                    id = %migration.id,
                    "Failed to create destination directory '{}': {}",
                    dest_dir, e
                );
                // Update migration status to failed
                if let Ok(mut store) = self.store.write() {
                    if let Some(m) = store.migrations.get_mut(&migration.id) {
                        m.status = MigrationStatus::Failed;
                        m.error = Some(format!("Failed to create destination: {}", e));
                        m.completed = Some(Utc::now());
                    }
                }
                return Ok(migration);
            }

            // Use qemu-img convert to copy and optionally optimize the image.
            // --reflink=auto would be ideal for btrfs/xfs, but qemu-img convert
            // handles format conversion and deduplication of zero blocks.
            let output = std::process::Command::new("qemu-img")
                .args([
                    "convert",
                    "-f",
                    "qcow2",
                    "-O",
                    "qcow2",
                    "-p",
                    &source_path,
                    &dest_path,
                ])
                .output();

            match output {
                Ok(out) if out.status.success() => {
                    // Get the actual size of the transferred file
                    let bytes_transferred = std::fs::metadata(&dest_path)
                        .map(|m| m.len())
                        .unwrap_or(disk_size_gb * 1024 * 1024 * 1024);

                    if let Ok(mut store) = self.store.write() {
                        if let Some(m) = store.migrations.get_mut(&migration.id) {
                            m.bytes_transferred = bytes_transferred;
                            m.progress_pct = 100.0;
                            m.status = MigrationStatus::Completed;
                            m.completed = Some(Utc::now());
                        }
                    }
                    tracing::info!(
                        id = %migration.id,
                        vm = %vm_name,
                        bytes = bytes_transferred,
                        "storage migration completed via qemu-img convert"
                    );
                }
                Ok(out) => {
                    let stderr = String::from_utf8_lossy(&out.stderr);
                    tracing::error!(
                        id = %migration.id,
                        "qemu-img convert failed for VM '{}': {}",
                        vm_name, stderr
                    );
                    // Clean up partial destination file
                    let _ = std::fs::remove_file(&dest_path);
                    if let Ok(mut store) = self.store.write() {
                        if let Some(m) = store.migrations.get_mut(&migration.id) {
                            m.status = MigrationStatus::Failed;
                            m.error = Some(format!("qemu-img convert failed: {}", stderr));
                            m.completed = Some(Utc::now());
                        }
                    }
                }
                Err(e) => {
                    tracing::warn!(
                        id = %migration.id,
                        "Failed to execute qemu-img for VM '{}': {} \
                         (qemu-img may not be installed; migration recorded as in-progress)",
                        vm_name, e
                    );
                    // Leave status as InProgress for the caller to manage manually
                }
            }
        } else {
            tracing::debug!(
                id = %migration.id,
                "Source path '{}' does not exist for VM '{}', \
                 migration recorded as in-progress (metadata only)",
                source_path, vm_name
            );
        }

        Ok(migration)
    }

    /// Update migration progress with the number of bytes transferred so far.
    pub fn update_migration_progress(&self, id: &str, bytes_transferred: u64) -> Result<()> {
        let mut store = self
            .store
            .write()
            .map_err(|e| anyhow!("lock poisoned: {e}"))?;
        let migration = store
            .migrations
            .get_mut(id)
            .ok_or_else(|| anyhow!("migration not found: {id}"))?;

        if migration.status != MigrationStatus::InProgress {
            return Err(anyhow!(
                "cannot update progress for migration in {:?} state",
                migration.status
            ));
        }

        migration.bytes_transferred = bytes_transferred;
        let total_bytes = migration.disk_size_gb * 1024 * 1024 * 1024;
        migration.progress_pct = if total_bytes > 0 {
            ((bytes_transferred as f64) / (total_bytes as f64) * 100.0).min(100.0)
        } else {
            100.0
        };

        tracing::debug!(
            id = %id,
            progress = %migration.progress_pct,
            "updated migration progress"
        );
        Ok(())
    }

    /// Mark a migration as completed and update pool capacities.
    pub fn complete_migration(&self, id: &str) -> Result<()> {
        let mut store = self
            .store
            .write()
            .map_err(|e| anyhow!("lock poisoned: {e}"))?;
        let migration = store
            .migrations
            .get_mut(id)
            .ok_or_else(|| anyhow!("migration not found: {id}"))?;

        if migration.status != MigrationStatus::InProgress {
            return Err(anyhow!(
                "cannot complete migration in {:?} state",
                migration.status
            ));
        }

        let disk_size = migration.disk_size_gb;
        let total_bytes = disk_size * 1024 * 1024 * 1024;
        migration.bytes_transferred = total_bytes;
        migration.progress_pct = 100.0;
        migration.status = MigrationStatus::Completed;
        migration.completed = Some(Utc::now());

        let source_id = migration.source_pool_id.clone();
        let target_id = migration.target_pool_id.clone();

        // Update source pool: free up space.
        if let Some(source) = store.pools.get_mut(&source_id) {
            source.used_capacity_gb = source.used_capacity_gb.saturating_sub(disk_size);
            source.free_capacity_gb = source.total_capacity_gb - source.used_capacity_gb;
            source.updated = Utc::now();
        }

        // Update target pool: consume space.
        if let Some(target) = store.pools.get_mut(&target_id) {
            target.used_capacity_gb += disk_size;
            target.free_capacity_gb = target
                .total_capacity_gb
                .saturating_sub(target.used_capacity_gb);
            target.updated = Utc::now();
        }

        tracing::info!(id = %id, "completed storage migration");
        Ok(())
    }

    /// Cancel an in-progress migration.
    pub fn cancel_migration(&self, id: &str) -> Result<()> {
        let mut store = self
            .store
            .write()
            .map_err(|e| anyhow!("lock poisoned: {e}"))?;
        let migration = store
            .migrations
            .get_mut(id)
            .ok_or_else(|| anyhow!("migration not found: {id}"))?;

        if migration.status != MigrationStatus::InProgress
            && migration.status != MigrationStatus::Pending
        {
            return Err(anyhow!(
                "cannot cancel migration in {:?} state",
                migration.status
            ));
        }

        migration.status = MigrationStatus::Cancelled;
        migration.completed = Some(Utc::now());

        tracing::info!(id = %id, "cancelled storage migration");
        Ok(())
    }

    /// Get a migration by ID.
    pub fn get_migration(&self, id: &str) -> Option<StorageMigration> {
        let store = self.store.read().unwrap_or_else(|e| e.into_inner());
        store.migrations.get(id).cloned()
    }

    /// List migrations, optionally filtered by VM name.
    pub fn list_migrations(&self, vm_name: Option<&str>) -> Vec<StorageMigration> {
        let store = self.store.read().unwrap_or_else(|e| e.into_inner());
        store
            .migrations
            .values()
            .filter(|m| match vm_name {
                Some(name) => m.vm_name == name,
                None => true,
            })
            .cloned()
            .collect()
    }

    // -- Storage Policies (SPBM) ---------------------------------------------

    /// Create a new storage policy.
    pub fn create_policy(&self, policy: StoragePolicy) -> Result<StoragePolicy> {
        let mut store = self
            .store
            .write()
            .map_err(|e| anyhow!("lock poisoned: {e}"))?;

        if store.policies.values().any(|p| p.name == policy.name) {
            return Err(anyhow!("policy with name '{}' already exists", policy.name));
        }

        tracing::info!(id = %policy.id, name = %policy.name, "created storage policy");
        store.policies.insert(policy.id.clone(), policy.clone());
        Ok(policy)
    }

    /// Get a policy by ID.
    pub fn get_policy(&self, id: &str) -> Option<StoragePolicy> {
        let store = self.store.read().unwrap_or_else(|e| e.into_inner());
        store.policies.get(id).cloned()
    }

    /// List all storage policies.
    pub fn list_policies(&self) -> Vec<StoragePolicy> {
        let store = self.store.read().unwrap_or_else(|e| e.into_inner());
        store.policies.values().cloned().collect()
    }

    /// Update an existing storage policy.
    pub fn update_policy(&self, id: &str, policy: StoragePolicy) -> Result<StoragePolicy> {
        let mut store = self
            .store
            .write()
            .map_err(|e| anyhow!("lock poisoned: {e}"))?;
        if !store.policies.contains_key(id) {
            return Err(anyhow!("policy not found: {id}"));
        }

        tracing::info!(id = %id, name = %policy.name, "updated storage policy");
        store.policies.insert(id.to_string(), policy.clone());
        Ok(policy)
    }

    /// Delete a storage policy by ID.
    pub fn delete_policy(&self, id: &str) -> Result<()> {
        let mut store = self
            .store
            .write()
            .map_err(|e| anyhow!("lock poisoned: {e}"))?;
        store
            .policies
            .remove(id)
            .ok_or_else(|| anyhow!("policy not found: {id}"))?;
        tracing::info!(id = %id, "deleted storage policy");
        Ok(())
    }

    /// Check whether a VM's current pool is compliant with a storage policy.
    pub fn check_compliance(
        &self,
        vm_name: &str,
        policy_id: &str,
        current_pool: &DistributedStoragePool,
    ) -> ComplianceReport {
        let store = self.store.read().unwrap_or_else(|e| e.into_inner());
        let policy = store.policies.get(policy_id);

        let (policy_name, violations) = match policy {
            Some(policy) => {
                let mut violations = Vec::new();

                // Check replication factor.
                if current_pool.replication_factor < policy.replication_factor {
                    violations.push(format!(
                        "replication factor {} is below required {}",
                        current_pool.replication_factor, policy.replication_factor
                    ));
                }

                // Check disk type requirement.
                if let Some(ref required_type) = policy.disk_type_required {
                    let all_match = current_pool
                        .hosts
                        .iter()
                        .flat_map(|h| h.disks.iter())
                        .all(|d| d.disk_type == *required_type);
                    if !all_match {
                        violations.push(format!(
                            "pool contains disks that are not {:?}",
                            required_type
                        ));
                    }
                }

                // Check pool health.
                if current_pool.health == PoolHealth::Critical {
                    violations.push("pool health is critical".to_string());
                }

                (policy.name.clone(), violations)
            }
            None => {
                let violations = vec![format!("policy not found: {policy_id}")];
                ("unknown".to_string(), violations)
            }
        };

        let compliant = violations.is_empty();

        ComplianceReport {
            vm_name: vm_name.to_string(),
            policy_id: policy_id.to_string(),
            policy_name,
            compliant,
            violations,
            checked_at: Utc::now(),
        }
    }

    /// Find all pools compatible with a given storage policy.
    pub fn find_compatible_pools(&self, policy_id: &str) -> Vec<DistributedStoragePool> {
        let store = self.store.read().unwrap_or_else(|e| e.into_inner());
        let policy = match store.policies.get(policy_id) {
            Some(p) => p.clone(),
            None => return Vec::new(),
        };

        store
            .pools
            .values()
            .filter(|pool| {
                // Must meet replication factor.
                if pool.replication_factor < policy.replication_factor {
                    return false;
                }

                // Must be online or at least not offline.
                if pool.status == PoolStatus::Offline {
                    return false;
                }

                // Must not be critically unhealthy.
                if pool.health == PoolHealth::Critical {
                    return false;
                }

                // If a disk type is required, all disks must match.
                if let Some(ref required_type) = policy.disk_type_required {
                    let all_match = pool
                        .hosts
                        .iter()
                        .flat_map(|h| h.disks.iter())
                        .all(|d| d.disk_type == *required_type);
                    if !all_match {
                        return false;
                    }
                }

                true
            })
            .cloned()
            .collect()
    }

    // -- Datastore Clusters --------------------------------------------------

    /// Create a new datastore cluster.
    pub fn create_datastore_cluster(
        &self,
        req: CreateDatastoreClusterRequest,
    ) -> Result<DatastoreCluster> {
        let now = Utc::now();
        let ds_cluster = DatastoreCluster {
            id: Uuid::new_v4().to_string(),
            name: req.name,
            cluster_id: req.cluster_id,
            datastore_ids: req.datastore_ids,
            storage_drs_enabled: req.storage_drs_enabled,
            space_threshold_pct: req.space_threshold_pct,
            io_latency_threshold_ms: req.io_latency_threshold_ms,
            automation_level: req.automation_level,
            created: now,
            updated: now,
        };

        let mut store = self
            .store
            .write()
            .map_err(|e| anyhow!("lock poisoned: {e}"))?;
        tracing::info!(id = %ds_cluster.id, name = %ds_cluster.name, "created datastore cluster");
        store
            .datastore_clusters
            .insert(ds_cluster.id.clone(), ds_cluster.clone());
        Ok(ds_cluster)
    }

    /// Get a datastore cluster by ID.
    pub fn get_datastore_cluster(&self, id: &str) -> Option<DatastoreCluster> {
        let store = self.store.read().unwrap_or_else(|e| e.into_inner());
        store.datastore_clusters.get(id).cloned()
    }

    /// List datastore clusters, optionally filtered by cluster ID.
    pub fn list_datastore_clusters(&self, cluster_id: Option<&str>) -> Vec<DatastoreCluster> {
        let store = self.store.read().unwrap_or_else(|e| e.into_inner());
        store
            .datastore_clusters
            .values()
            .filter(|dc| match cluster_id {
                Some(cid) => dc.cluster_id == cid,
                None => true,
            })
            .cloned()
            .collect()
    }

    /// Delete a datastore cluster by ID.
    pub fn delete_datastore_cluster(&self, id: &str) -> Result<()> {
        let mut store = self
            .store
            .write()
            .map_err(|e| anyhow!("lock poisoned: {e}"))?;
        store
            .datastore_clusters
            .remove(id)
            .ok_or_else(|| anyhow!("datastore cluster not found: {id}"))?;
        tracing::info!(id = %id, "deleted datastore cluster");
        Ok(())
    }

    /// Recommend the best datastore (pool) within a datastore cluster for a given
    /// workload size. Returns the pool_id of the pool with the most free space that
    /// can accommodate the requested size.
    pub fn recommend_datastore(&self, ds_cluster_id: &str, size_gb: u64) -> Result<String> {
        let store = self
            .store
            .read()
            .map_err(|e| anyhow!("lock poisoned: {e}"))?;
        let ds_cluster = store
            .datastore_clusters
            .get(ds_cluster_id)
            .ok_or_else(|| anyhow!("datastore cluster not found: {ds_cluster_id}"))?;

        // Gather pools that belong to this datastore cluster and have enough space.
        let mut candidates: Vec<&DistributedStoragePool> = ds_cluster
            .datastore_ids
            .iter()
            .filter_map(|pid| store.pools.get(pid))
            .filter(|pool| pool.status != PoolStatus::Offline && pool.free_capacity_gb >= size_gb)
            .collect();

        if candidates.is_empty() {
            return Err(anyhow!(
                "no suitable datastore found in cluster '{}' for {} GB",
                ds_cluster.name,
                size_gb
            ));
        }

        // Pick the pool with the most free capacity (storage DRS: balance by space).
        candidates.sort_by_key(|a| std::cmp::Reverse(a.free_capacity_gb));

        let recommended = candidates[0];
        tracing::info!(
            ds_cluster_id = %ds_cluster_id,
            recommended_pool = %recommended.id,
            free_gb = recommended.free_capacity_gb,
            "recommended datastore"
        );
        Ok(recommended.id.clone())
    }
}

impl Default for DistributedStorageManager {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Count the number of failed, rebuilding, and total disks across all hosts in a pool.
fn count_disk_states(pool: &DistributedStoragePool) -> (u32, u32, u32) {
    let mut failed = 0u32;
    let mut rebuilding = 0u32;
    let mut total = 0u32;
    for host in &pool.hosts {
        for disk in &host.disks {
            total += 1;
            match disk.status {
                DiskStatus::Failed => failed += 1,
                DiskStatus::Rebuilding => rebuilding += 1,
                DiskStatus::Active => {}
            }
        }
    }
    (failed, rebuilding, total)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // -- Helpers -------------------------------------------------------------

    fn make_disk(id: &str, capacity_gb: u64, disk_type: DiskType) -> DiskContribution {
        DiskContribution {
            disk_id: id.to_string(),
            path: format!("/dev/{id}"),
            capacity_gb,
            disk_type,
            status: DiskStatus::Active,
        }
    }

    fn make_host(host_id: &str, hostname: &str, disks: Vec<DiskContribution>) -> StorageHost {
        StorageHost {
            host_id: host_id.to_string(),
            hostname: hostname.to_string(),
            disks,
        }
    }

    fn make_pool_request(name: &str, cluster_id: &str) -> CreatePoolRequest {
        CreatePoolRequest {
            name: name.to_string(),
            cluster_id: cluster_id.to_string(),
            hosts: vec![
                make_host(
                    "host-1",
                    "node-1.local",
                    vec![
                        make_disk("ssd-1", 500, DiskType::Ssd),
                        make_disk("ssd-2", 500, DiskType::Ssd),
                    ],
                ),
                make_host(
                    "host-2",
                    "node-2.local",
                    vec![make_disk("ssd-3", 1000, DiskType::Ssd)],
                ),
            ],
            replication_factor: 2,
            erasure_coding: false,
            fault_domains: vec![FaultDomain {
                id: "fd-1".to_string(),
                name: "rack-1".to_string(),
                host_ids: vec!["host-1".to_string(), "host-2".to_string()],
            }],
        }
    }

    fn make_policy(id: &str, name: &str) -> StoragePolicy {
        let now = Utc::now();
        StoragePolicy {
            id: id.to_string(),
            name: name.to_string(),
            description: "test policy".to_string(),
            replication_factor: 2,
            disk_type_required: Some(DiskType::Ssd),
            encryption_required: false,
            iops_limit: None,
            throughput_limit_mbps: None,
            tier: StorageTier::Gold,
            created: now,
            updated: now,
        }
    }

    fn setup_manager_with_pool() -> (DistributedStorageManager, DistributedStoragePool) {
        let mgr = DistributedStorageManager::new();
        let pool = mgr
            .create_pool(make_pool_request("vsan-pool", "cluster-1"))
            .unwrap();
        (mgr, pool)
    }

    // -- Pool tests ----------------------------------------------------------

    #[test]
    fn test_create_pool_and_capacity() {
        let (mgr, pool) = setup_manager_with_pool();
        assert_eq!(pool.name, "vsan-pool");
        assert_eq!(pool.cluster_id, "cluster-1");
        assert_eq!(pool.replication_factor, 2);
        assert_eq!(pool.total_capacity_gb, 2000); // 500 + 500 + 1000
        assert_eq!(pool.used_capacity_gb, 0);
        assert_eq!(pool.free_capacity_gb, 2000);
        assert_eq!(pool.status, PoolStatus::Online);
        assert_eq!(pool.health, PoolHealth::Healthy);
        assert_eq!(pool.hosts.len(), 2);

        let fetched = mgr.get_pool(&pool.id).unwrap();
        assert_eq!(fetched.id, pool.id);
    }

    #[test]
    fn test_create_pool_invalid_replication_factor() {
        let mgr = DistributedStorageManager::new();
        let mut req = make_pool_request("bad-pool", "cluster-1");
        req.replication_factor = 0;
        assert!(mgr.create_pool(req).is_err());

        let mut req = make_pool_request("bad-pool", "cluster-1");
        req.replication_factor = 4;
        assert!(mgr.create_pool(req).is_err());
    }

    #[test]
    fn test_list_pools_with_filter() {
        let mgr = DistributedStorageManager::new();
        mgr.create_pool(make_pool_request("pool-a", "cluster-1"))
            .unwrap();
        mgr.create_pool(make_pool_request("pool-b", "cluster-2"))
            .unwrap();

        assert_eq!(mgr.list_pools(None).len(), 2);
        assert_eq!(mgr.list_pools(Some("cluster-1")).len(), 1);
        assert_eq!(mgr.list_pools(Some("cluster-2")).len(), 1);
        assert_eq!(mgr.list_pools(Some("nonexistent")).len(), 0);
    }

    #[test]
    fn test_delete_pool() {
        let (mgr, pool) = setup_manager_with_pool();
        mgr.delete_pool(&pool.id).unwrap();
        assert!(mgr.get_pool(&pool.id).is_none());
        assert!(mgr.delete_pool(&pool.id).is_err());
    }

    #[test]
    fn test_add_and_remove_host_from_pool() {
        let (mgr, pool) = setup_manager_with_pool();
        assert_eq!(pool.total_capacity_gb, 2000);

        let new_host = make_host(
            "host-3",
            "node-3.local",
            vec![make_disk("nvme-1", 2000, DiskType::Nvme)],
        );
        mgr.add_host_to_pool(&pool.id, new_host).unwrap();

        let updated = mgr.get_pool(&pool.id).unwrap();
        assert_eq!(updated.hosts.len(), 3);
        assert_eq!(updated.total_capacity_gb, 4000);
        assert_eq!(updated.free_capacity_gb, 4000);

        mgr.remove_host_from_pool(&pool.id, "host-3").unwrap();
        let updated = mgr.get_pool(&pool.id).unwrap();
        assert_eq!(updated.hosts.len(), 2);
        assert_eq!(updated.total_capacity_gb, 2000);
    }

    #[test]
    fn test_disk_failure_and_pool_health() {
        let (mgr, pool) = setup_manager_with_pool();

        // Report a disk failure.
        mgr.report_disk_failure(&pool.id, "host-1", "ssd-1")
            .unwrap();

        let health = mgr.get_pool_health(&pool.id).unwrap();
        assert_eq!(health.failed_disks, 1);
        assert_eq!(health.status, PoolStatus::Degraded);
        assert_eq!(health.health, PoolHealth::Warning);

        // Report more failures to push to critical.
        mgr.report_disk_failure(&pool.id, "host-1", "ssd-2")
            .unwrap();
        mgr.report_disk_failure(&pool.id, "host-2", "ssd-3")
            .unwrap();

        let health = mgr.get_pool_health(&pool.id).unwrap();
        assert_eq!(health.failed_disks, 3);
        assert_eq!(health.health, PoolHealth::Critical);
    }

    #[test]
    fn test_pool_health_report_capacity() {
        let (mgr, pool) = setup_manager_with_pool();
        let health = mgr.get_pool_health(&pool.id).unwrap();
        assert_eq!(health.pool_id, pool.id);
        assert_eq!(health.failed_disks, 0);
        assert_eq!(health.rebuilding_disks, 0);
        assert!((health.capacity_used_pct - 0.0).abs() < f64::EPSILON);
    }

    // -- Migration tests -----------------------------------------------------

    #[test]
    fn test_storage_migration_lifecycle() {
        let mgr = DistributedStorageManager::new();
        let source = mgr
            .create_pool(make_pool_request("source-pool", "cluster-1"))
            .unwrap();
        let target = mgr
            .create_pool(make_pool_request("target-pool", "cluster-1"))
            .unwrap();

        // Start migration.
        let migration = mgr
            .start_storage_migration("test-vm", &source.id, &target.id, 100)
            .unwrap();
        assert_eq!(migration.vm_name, "test-vm");
        assert_eq!(migration.status, MigrationStatus::InProgress);
        assert_eq!(migration.progress_pct, 0.0);
        assert!(migration.started.is_some());

        // Update progress (50 GB out of 100 GB).
        let half_bytes = 50 * 1024 * 1024 * 1024;
        mgr.update_migration_progress(&migration.id, half_bytes)
            .unwrap();
        let m = mgr.get_migration(&migration.id).unwrap();
        assert!((m.progress_pct - 50.0).abs() < 0.1);

        // Complete migration.
        mgr.complete_migration(&migration.id).unwrap();
        let m = mgr.get_migration(&migration.id).unwrap();
        assert_eq!(m.status, MigrationStatus::Completed);
        assert!((m.progress_pct - 100.0).abs() < f64::EPSILON);
        assert!(m.completed.is_some());

        // Target pool should have used capacity now.
        let target_updated = mgr.get_pool(&target.id).unwrap();
        assert_eq!(target_updated.used_capacity_gb, 100);
        assert_eq!(target_updated.free_capacity_gb, 1900);
    }

    #[test]
    fn test_cancel_migration() {
        let mgr = DistributedStorageManager::new();
        let source = mgr
            .create_pool(make_pool_request("source", "cluster-1"))
            .unwrap();
        let target = mgr
            .create_pool(make_pool_request("target", "cluster-1"))
            .unwrap();

        let migration = mgr
            .start_storage_migration("vm-cancel", &source.id, &target.id, 50)
            .unwrap();

        mgr.cancel_migration(&migration.id).unwrap();
        let m = mgr.get_migration(&migration.id).unwrap();
        assert_eq!(m.status, MigrationStatus::Cancelled);
        assert!(m.completed.is_some());

        // Cannot cancel again.
        assert!(mgr.cancel_migration(&migration.id).is_err());
    }

    #[test]
    fn test_list_migrations_by_vm() {
        let mgr = DistributedStorageManager::new();
        let p1 = mgr.create_pool(make_pool_request("p1", "c1")).unwrap();
        let p2 = mgr.create_pool(make_pool_request("p2", "c1")).unwrap();

        mgr.start_storage_migration("vm-a", &p1.id, &p2.id, 10)
            .unwrap();
        mgr.start_storage_migration("vm-b", &p1.id, &p2.id, 20)
            .unwrap();

        assert_eq!(mgr.list_migrations(None).len(), 2);
        assert_eq!(mgr.list_migrations(Some("vm-a")).len(), 1);
        assert_eq!(mgr.list_migrations(Some("vm-b")).len(), 1);
        assert_eq!(mgr.list_migrations(Some("vm-c")).len(), 0);
    }

    // -- Policy tests --------------------------------------------------------

    #[test]
    fn test_policy_crud() {
        let mgr = DistributedStorageManager::new();
        let policy = make_policy("pol-1", "gold-policy");

        // Create.
        let created = mgr.create_policy(policy.clone()).unwrap();
        assert_eq!(created.name, "gold-policy");

        // Get.
        let fetched = mgr.get_policy("pol-1").unwrap();
        assert_eq!(fetched.tier, StorageTier::Gold);

        // List.
        assert_eq!(mgr.list_policies().len(), 1);

        // Update.
        let mut updated_policy = fetched.clone();
        updated_policy.replication_factor = 3;
        updated_policy.tier = StorageTier::Silver;
        let updated = mgr.update_policy("pol-1", updated_policy).unwrap();
        assert_eq!(updated.replication_factor, 3);
        assert_eq!(updated.tier, StorageTier::Silver);

        // Delete.
        mgr.delete_policy("pol-1").unwrap();
        assert!(mgr.get_policy("pol-1").is_none());
        assert!(mgr.delete_policy("pol-1").is_err());
    }

    #[test]
    fn test_duplicate_policy_name_rejected() {
        let mgr = DistributedStorageManager::new();
        mgr.create_policy(make_policy("pol-1", "unique-name"))
            .unwrap();
        assert!(mgr
            .create_policy(make_policy("pol-2", "unique-name"))
            .is_err());
    }

    #[test]
    fn test_compliance_check_compliant() {
        let (mgr, pool) = setup_manager_with_pool();
        let policy = make_policy("pol-ssd", "ssd-replicated");
        mgr.create_policy(policy).unwrap();

        let report = mgr.check_compliance("my-vm", "pol-ssd", &pool);
        assert!(report.compliant);
        assert!(report.violations.is_empty());
        assert_eq!(report.vm_name, "my-vm");
        assert_eq!(report.policy_name, "ssd-replicated");
    }

    #[test]
    fn test_compliance_check_violations() {
        let mgr = DistributedStorageManager::new();

        // Create a pool with replication_factor=1 and HDD disks.
        let pool = mgr
            .create_pool(CreatePoolRequest {
                name: "cheap-pool".to_string(),
                cluster_id: "cluster-1".to_string(),
                hosts: vec![make_host(
                    "host-1",
                    "node-1.local",
                    vec![make_disk("hdd-1", 4000, DiskType::Hdd)],
                )],
                replication_factor: 1,
                erasure_coding: false,
                fault_domains: vec![],
            })
            .unwrap();

        // Policy requires replication_factor=2 and SSD disks.
        let policy = make_policy("pol-strict", "strict-gold");
        mgr.create_policy(policy).unwrap();

        let report = mgr.check_compliance("my-vm", "pol-strict", &pool);
        assert!(!report.compliant);
        assert!(report.violations.len() >= 2);
        assert!(report.violations.iter().any(|v| v.contains("replication")));
        assert!(report.violations.iter().any(|v| v.contains("Ssd")));
    }

    #[test]
    fn test_find_compatible_pools() {
        let mgr = DistributedStorageManager::new();

        // Create an SSD pool with replication 2.
        mgr.create_pool(make_pool_request("ssd-pool", "cluster-1"))
            .unwrap();

        // Create an HDD pool with replication 1.
        mgr.create_pool(CreatePoolRequest {
            name: "hdd-pool".to_string(),
            cluster_id: "cluster-1".to_string(),
            hosts: vec![make_host(
                "host-hdd",
                "node-hdd.local",
                vec![make_disk("hdd-1", 8000, DiskType::Hdd)],
            )],
            replication_factor: 1,
            erasure_coding: false,
            fault_domains: vec![],
        })
        .unwrap();

        // Policy requires SSD + replication 2.
        let policy = make_policy("pol-gold", "gold-ssd");
        mgr.create_policy(policy).unwrap();

        let compatible = mgr.find_compatible_pools("pol-gold");
        assert_eq!(compatible.len(), 1);
        assert_eq!(compatible[0].name, "ssd-pool");
    }

    // -- Datastore cluster tests ---------------------------------------------

    #[test]
    fn test_datastore_cluster_crud() {
        let mgr = DistributedStorageManager::new();
        let ds = mgr
            .create_datastore_cluster(CreateDatastoreClusterRequest {
                name: "ds-cluster-1".to_string(),
                cluster_id: "cluster-1".to_string(),
                datastore_ids: vec!["pool-a".to_string(), "pool-b".to_string()],
                storage_drs_enabled: true,
                space_threshold_pct: 80,
                io_latency_threshold_ms: Some(25),
                automation_level: AutomationLevel::FullyAutomated,
            })
            .unwrap();

        assert_eq!(ds.name, "ds-cluster-1");
        assert!(ds.storage_drs_enabled);
        assert_eq!(ds.space_threshold_pct, 80);
        assert_eq!(ds.automation_level, AutomationLevel::FullyAutomated);

        // Get.
        let fetched = mgr.get_datastore_cluster(&ds.id).unwrap();
        assert_eq!(fetched.datastore_ids.len(), 2);

        // List.
        assert_eq!(mgr.list_datastore_clusters(None).len(), 1);
        assert_eq!(mgr.list_datastore_clusters(Some("cluster-1")).len(), 1);
        assert_eq!(mgr.list_datastore_clusters(Some("other")).len(), 0);

        // Delete.
        mgr.delete_datastore_cluster(&ds.id).unwrap();
        assert!(mgr.get_datastore_cluster(&ds.id).is_none());
    }

    #[test]
    fn test_recommend_datastore() {
        let mgr = DistributedStorageManager::new();

        // Create two pools with different free space.
        let pool_small = mgr
            .create_pool(CreatePoolRequest {
                name: "small-pool".to_string(),
                cluster_id: "cluster-1".to_string(),
                hosts: vec![make_host(
                    "host-s",
                    "node-s.local",
                    vec![make_disk("d1", 500, DiskType::Ssd)],
                )],
                replication_factor: 1,
                erasure_coding: false,
                fault_domains: vec![],
            })
            .unwrap();

        let pool_large = mgr
            .create_pool(CreatePoolRequest {
                name: "large-pool".to_string(),
                cluster_id: "cluster-1".to_string(),
                hosts: vec![make_host(
                    "host-l",
                    "node-l.local",
                    vec![make_disk("d2", 5000, DiskType::Ssd)],
                )],
                replication_factor: 1,
                erasure_coding: false,
                fault_domains: vec![],
            })
            .unwrap();

        let ds = mgr
            .create_datastore_cluster(CreateDatastoreClusterRequest {
                name: "ds-cluster".to_string(),
                cluster_id: "cluster-1".to_string(),
                datastore_ids: vec![pool_small.id.clone(), pool_large.id.clone()],
                storage_drs_enabled: true,
                space_threshold_pct: 80,
                io_latency_threshold_ms: None,
                automation_level: AutomationLevel::FullyAutomated,
            })
            .unwrap();

        // Should recommend the large pool (most free space).
        let recommended = mgr.recommend_datastore(&ds.id, 100).unwrap();
        assert_eq!(recommended, pool_large.id);

        // Request that exceeds all pools should fail.
        assert!(mgr.recommend_datastore(&ds.id, 10000).is_err());
    }
}
