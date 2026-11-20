// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: LicenseRef-Zyvor-Production-1.0

use anyhow::{bail, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use uuid::Uuid;

// ---------------------------------------------------------------------------
// Share enums
// ---------------------------------------------------------------------------

/// CPU share levels controlling proportional resource allocation.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum CpuShares {
    Low,
    Normal,
    High,
    Custom(u32),
}

impl CpuShares {
    /// Return the numeric share value.
    pub fn value(&self) -> u32 {
        match self {
            CpuShares::Low => 1000,
            CpuShares::Normal => 2000,
            CpuShares::High => 4000,
            CpuShares::Custom(v) => *v,
        }
    }
}

/// Memory share levels controlling proportional resource allocation.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum MemoryShares {
    Low,
    Normal,
    High,
    Custom(u32),
}

impl MemoryShares {
    /// Return the numeric share value.
    pub fn value(&self) -> u32 {
        match self {
            MemoryShares::Low => 1000,
            MemoryShares::Normal => 2000,
            MemoryShares::High => 4000,
            MemoryShares::Custom(v) => *v,
        }
    }
}

// ---------------------------------------------------------------------------
// Core data models
// ---------------------------------------------------------------------------

/// A hierarchical resource pool that partitions cluster CPU and memory
/// capacity, similar to vSphere Resource Pools.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourcePool {
    /// Unique identifier (UUID v4).
    pub id: String,
    /// Human-readable name.
    pub name: String,
    /// Parent pool id; `None` for root pools.
    pub parent_id: Option<String>,
    /// The cluster this pool belongs to.
    pub cluster_id: String,

    // -- CPU settings --
    pub cpu_shares: CpuShares,
    /// Guaranteed minimum CPU in MHz.
    pub cpu_reservation_mhz: u64,
    /// Hard upper limit; `None` means unlimited.
    pub cpu_limit_mhz: Option<u64>,
    /// Whether the pool may borrow unreserved capacity from its parent.
    pub cpu_expandable_reservation: bool,

    // -- Memory settings --
    pub memory_shares: MemoryShares,
    /// Guaranteed minimum memory in MB.
    pub memory_reservation_mb: u64,
    /// Hard upper limit; `None` means unlimited.
    pub memory_limit_mb: Option<u64>,
    /// Whether the pool may borrow unreserved capacity from its parent.
    pub memory_expandable_reservation: bool,

    /// VM names assigned to this pool.
    pub vms: Vec<String>,
    /// Child pool IDs.
    pub children: Vec<String>,

    pub created: DateTime<Utc>,
    pub updated: Option<DateTime<Utc>>,
}

/// Compact summary of a resource pool, including live usage data.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourcePoolSummary {
    pub id: String,
    pub name: String,
    pub parent_id: Option<String>,
    pub cluster_id: String,
    pub cpu_reservation_mhz: u64,
    pub cpu_limit_mhz: Option<u64>,
    pub cpu_used_mhz: u64,
    pub memory_reservation_mb: u64,
    pub memory_limit_mb: Option<u64>,
    pub memory_used_mb: u64,
    pub vm_count: usize,
    pub child_pool_count: usize,
}

/// Request to create a new resource pool.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateResourcePoolRequest {
    pub name: String,
    pub parent_id: Option<String>,
    pub cluster_id: String,
    pub cpu_shares: CpuShares,
    pub cpu_reservation_mhz: u64,
    pub cpu_limit_mhz: Option<u64>,
    pub cpu_expandable_reservation: bool,
    pub memory_shares: MemoryShares,
    pub memory_reservation_mb: u64,
    pub memory_limit_mb: Option<u64>,
    pub memory_expandable_reservation: bool,
}

/// Request to update an existing resource pool.  All fields are optional so
/// callers may perform partial updates.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct UpdateResourcePoolRequest {
    pub name: Option<String>,
    pub cpu_shares: Option<CpuShares>,
    pub cpu_reservation_mhz: Option<u64>,
    pub cpu_limit_mhz: Option<Option<u64>>,
    pub cpu_expandable_reservation: Option<bool>,
    pub memory_shares: Option<MemoryShares>,
    pub memory_reservation_mb: Option<u64>,
    pub memory_limit_mb: Option<Option<u64>>,
    pub memory_expandable_reservation: Option<bool>,
}

/// Result of an admission control check.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AdmissionControlResult {
    /// Whether the request is allowed.
    pub allowed: bool,
    /// Human-readable denial reason when `allowed` is `false`.
    pub reason: Option<String>,
    /// Available CPU capacity in the pool (MHz).
    pub available_cpu_mhz: u64,
    /// Available memory capacity in the pool (MB).
    pub available_memory_mb: u64,
}

// ---------------------------------------------------------------------------
// Error type
// ---------------------------------------------------------------------------

#[derive(Debug, thiserror::Error)]
pub enum ResourcePoolError {
    #[error("resource pool not found: {0}")]
    NotFound(String),
    #[error("parent pool not found: {0}")]
    ParentNotFound(String),
    #[error("cannot delete pool {0}: pool has child pools")]
    HasChildren(String),
    #[error("cannot delete pool {0}: pool has assigned VMs")]
    HasVMs(String),
    #[error("VM {0} not found in pool {1}")]
    VmNotInPool(String, String),
    #[error("VM {0} already assigned to pool {1}")]
    VmAlreadyAssigned(String, String),
    #[error("admission denied: {0}")]
    AdmissionDenied(String),
}

// ---------------------------------------------------------------------------
// ResourcePoolManager
// ---------------------------------------------------------------------------

/// Thread-safe manager for a hierarchy of resource pools.
///
/// Internally stores pools in a `HashMap` behind an `Arc<RwLock<...>>` so it
/// can be shared across threads cheaply.
#[derive(Clone)]
pub struct ResourcePoolManager {
    pools: Arc<RwLock<HashMap<String, ResourcePool>>>,
}

impl ResourcePoolManager {
    /// Create a new, empty manager.
    pub fn new() -> Self {
        Self {
            pools: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    // -- Pool CRUD ----------------------------------------------------------

    /// Create a new resource pool and return it.
    pub fn create_pool(&self, req: CreateResourcePoolRequest) -> Result<ResourcePool> {
        let mut pools = self.pools.write().unwrap_or_else(|e| e.into_inner());

        // If a parent is specified, validate it exists and belongs to the same
        // cluster, then register the new pool as a child.
        if let Some(ref parent_id) = req.parent_id {
            let parent = pools
                .get(parent_id)
                .ok_or_else(|| ResourcePoolError::ParentNotFound(parent_id.clone()))?;
            if parent.cluster_id != req.cluster_id {
                bail!(
                    "parent pool {} belongs to cluster {}, not {}",
                    parent_id,
                    parent.cluster_id,
                    req.cluster_id
                );
            }
        }

        let id = Uuid::new_v4().to_string();
        let now = Utc::now();

        let pool = ResourcePool {
            id: id.clone(),
            name: req.name,
            parent_id: req.parent_id.clone(),
            cluster_id: req.cluster_id,
            cpu_shares: req.cpu_shares,
            cpu_reservation_mhz: req.cpu_reservation_mhz,
            cpu_limit_mhz: req.cpu_limit_mhz,
            cpu_expandable_reservation: req.cpu_expandable_reservation,
            memory_shares: req.memory_shares,
            memory_reservation_mb: req.memory_reservation_mb,
            memory_limit_mb: req.memory_limit_mb,
            memory_expandable_reservation: req.memory_expandable_reservation,
            vms: Vec::new(),
            children: Vec::new(),
            created: now,
            updated: None,
        };

        // Register as child of parent.
        if let Some(ref parent_id) = req.parent_id {
            if let Some(parent) = pools.get_mut(parent_id) {
                parent.children.push(id.clone());
                parent.updated = Some(now);
            }
        }

        tracing::info!(pool_id = %id, name = %pool.name, "created resource pool");
        pools.insert(id, pool.clone());

        Ok(pool)
    }

    /// Get a single pool by id.
    pub fn get_pool(&self, id: &str) -> Option<ResourcePool> {
        let pools = self.pools.read().unwrap_or_else(|e| e.into_inner());
        pools.get(id).cloned()
    }

    /// List pools, optionally filtered by cluster id.
    pub fn list_pools(&self, cluster_id: Option<&str>) -> Vec<ResourcePool> {
        let pools = self.pools.read().unwrap_or_else(|e| e.into_inner());
        pools
            .values()
            .filter(|p| match cluster_id {
                Some(cid) => p.cluster_id == cid,
                None => true,
            })
            .cloned()
            .collect()
    }

    /// Apply a partial update to an existing pool.
    pub fn update_pool(&self, id: &str, req: UpdateResourcePoolRequest) -> Result<ResourcePool> {
        let mut pools = self.pools.write().unwrap_or_else(|e| e.into_inner());
        let pool = pools
            .get_mut(id)
            .ok_or_else(|| ResourcePoolError::NotFound(id.to_string()))?;

        if let Some(name) = req.name {
            pool.name = name;
        }
        if let Some(shares) = req.cpu_shares {
            pool.cpu_shares = shares;
        }
        if let Some(v) = req.cpu_reservation_mhz {
            pool.cpu_reservation_mhz = v;
        }
        if let Some(v) = req.cpu_limit_mhz {
            pool.cpu_limit_mhz = v;
        }
        if let Some(v) = req.cpu_expandable_reservation {
            pool.cpu_expandable_reservation = v;
        }
        if let Some(shares) = req.memory_shares {
            pool.memory_shares = shares;
        }
        if let Some(v) = req.memory_reservation_mb {
            pool.memory_reservation_mb = v;
        }
        if let Some(v) = req.memory_limit_mb {
            pool.memory_limit_mb = v;
        }
        if let Some(v) = req.memory_expandable_reservation {
            pool.memory_expandable_reservation = v;
        }

        pool.updated = Some(Utc::now());

        tracing::info!(pool_id = %id, "updated resource pool");
        Ok(pool.clone())
    }

    /// Delete a pool.  Fails if the pool has children or assigned VMs.
    pub fn delete_pool(&self, id: &str) -> Result<()> {
        let mut pools = self.pools.write().unwrap_or_else(|e| e.into_inner());

        // Validate the pool exists and has no dependents.
        {
            let pool = pools
                .get(id)
                .ok_or_else(|| ResourcePoolError::NotFound(id.to_string()))?;

            if !pool.children.is_empty() {
                bail!(ResourcePoolError::HasChildren(id.to_string()));
            }
            if !pool.vms.is_empty() {
                bail!(ResourcePoolError::HasVMs(id.to_string()));
            }
        }

        // Remove from parent's children list.
        let parent_id = pools.get(id).and_then(|p| p.parent_id.clone());
        if let Some(pid) = parent_id {
            if let Some(parent) = pools.get_mut(&pid) {
                parent.children.retain(|c| c != id);
                parent.updated = Some(Utc::now());
            }
        }

        pools.remove(id);
        tracing::info!(pool_id = %id, "deleted resource pool");

        Ok(())
    }

    // -- Summary ------------------------------------------------------------

    /// Build a summary for the given pool, including derived usage figures.
    pub fn get_pool_summary(&self, id: &str) -> Result<ResourcePoolSummary> {
        let pools = self.pools.read().unwrap_or_else(|e| e.into_inner());
        let pool = pools
            .get(id)
            .ok_or_else(|| ResourcePoolError::NotFound(id.to_string()))?;

        let (cpu_used, mem_used) = Self::compute_used_resources(&pools, pool);

        Ok(ResourcePoolSummary {
            id: pool.id.clone(),
            name: pool.name.clone(),
            parent_id: pool.parent_id.clone(),
            cluster_id: pool.cluster_id.clone(),
            cpu_reservation_mhz: pool.cpu_reservation_mhz,
            cpu_limit_mhz: pool.cpu_limit_mhz,
            cpu_used_mhz: cpu_used,
            memory_reservation_mb: pool.memory_reservation_mb,
            memory_limit_mb: pool.memory_limit_mb,
            memory_used_mb: mem_used,
            vm_count: pool.vms.len(),
            child_pool_count: pool.children.len(),
        })
    }

    // -- VM assignment ------------------------------------------------------

    /// Assign a VM to a pool.
    pub fn assign_vm(&self, pool_id: &str, vm_name: &str) -> Result<()> {
        let mut pools = self.pools.write().unwrap_or_else(|e| e.into_inner());
        let pool = pools
            .get_mut(pool_id)
            .ok_or_else(|| ResourcePoolError::NotFound(pool_id.to_string()))?;

        if pool.vms.contains(&vm_name.to_string()) {
            bail!(ResourcePoolError::VmAlreadyAssigned(
                vm_name.to_string(),
                pool_id.to_string(),
            ));
        }

        pool.vms.push(vm_name.to_string());
        pool.updated = Some(Utc::now());

        tracing::info!(pool_id = %pool_id, vm = %vm_name, "assigned VM to pool");
        Ok(())
    }

    /// Remove a VM from a pool.
    pub fn unassign_vm(&self, pool_id: &str, vm_name: &str) -> Result<()> {
        let mut pools = self.pools.write().unwrap_or_else(|e| e.into_inner());
        let pool = pools
            .get_mut(pool_id)
            .ok_or_else(|| ResourcePoolError::NotFound(pool_id.to_string()))?;

        let before_len = pool.vms.len();
        pool.vms.retain(|v| v != vm_name);
        if pool.vms.len() == before_len {
            bail!(ResourcePoolError::VmNotInPool(
                vm_name.to_string(),
                pool_id.to_string(),
            ));
        }

        pool.updated = Some(Utc::now());
        tracing::info!(pool_id = %pool_id, vm = %vm_name, "unassigned VM from pool");
        Ok(())
    }

    /// Move a VM from one pool to another.
    pub fn move_vm(&self, from_pool: &str, to_pool: &str, vm_name: &str) -> Result<()> {
        let mut pools = self.pools.write().unwrap_or_else(|e| e.into_inner());

        // Validate source pool and remove VM.
        {
            let src = pools
                .get_mut(from_pool)
                .ok_or_else(|| ResourcePoolError::NotFound(from_pool.to_string()))?;

            let before_len = src.vms.len();
            src.vms.retain(|v| v != vm_name);
            if src.vms.len() == before_len {
                bail!(ResourcePoolError::VmNotInPool(
                    vm_name.to_string(),
                    from_pool.to_string(),
                ));
            }
            src.updated = Some(Utc::now());
        }

        // Validate destination pool and add VM.
        {
            let dst = pools
                .get_mut(to_pool)
                .ok_or_else(|| ResourcePoolError::NotFound(to_pool.to_string()))?;

            if dst.vms.contains(&vm_name.to_string()) {
                bail!(ResourcePoolError::VmAlreadyAssigned(
                    vm_name.to_string(),
                    to_pool.to_string(),
                ));
            }
            dst.vms.push(vm_name.to_string());
            dst.updated = Some(Utc::now());
        }

        tracing::info!(
            from = %from_pool, to = %to_pool, vm = %vm_name,
            "moved VM between pools"
        );
        Ok(())
    }

    // -- Admission control --------------------------------------------------

    /// Check whether admitting `cpu_mhz` / `memory_mb` into the given pool
    /// would violate reservation or limit constraints.
    pub fn check_admission(
        &self,
        pool_id: &str,
        cpu_mhz: u64,
        memory_mb: u64,
    ) -> AdmissionControlResult {
        let pools = self.pools.read().unwrap_or_else(|e| e.into_inner());

        let pool = match pools.get(pool_id) {
            Some(p) => p,
            None => {
                return AdmissionControlResult {
                    allowed: false,
                    reason: Some(format!("pool {} not found", pool_id)),
                    available_cpu_mhz: 0,
                    available_memory_mb: 0,
                };
            }
        };

        let (avail_cpu, avail_mem) = Self::compute_available(&pools, pool);

        // Check CPU limit.
        if let Some(limit) = pool.cpu_limit_mhz {
            let (used_cpu, _) = Self::compute_used_resources(&pools, pool);
            if used_cpu + cpu_mhz > limit {
                return AdmissionControlResult {
                    allowed: false,
                    reason: Some(format!(
                        "CPU request {} MHz would exceed pool limit {} MHz (currently used: {} MHz)",
                        cpu_mhz, limit, used_cpu
                    )),
                    available_cpu_mhz: avail_cpu,
                    available_memory_mb: avail_mem,
                };
            }
        }

        // Check memory limit.
        if let Some(limit) = pool.memory_limit_mb {
            let (_, used_mem) = Self::compute_used_resources(&pools, pool);
            if used_mem + memory_mb > limit {
                return AdmissionControlResult {
                    allowed: false,
                    reason: Some(format!(
                        "memory request {} MB would exceed pool limit {} MB (currently used: {} MB)",
                        memory_mb, limit, used_mem
                    )),
                    available_cpu_mhz: avail_cpu,
                    available_memory_mb: avail_mem,
                };
            }
        }

        // Check non-expandable reservation: the new workload's demand must
        // fit within the pool's own reservation if expandable is off.
        if !pool.cpu_expandable_reservation {
            let child_cpu_reservations = Self::sum_child_reservations_cpu(&pools, pool);
            let remaining = pool
                .cpu_reservation_mhz
                .saturating_sub(child_cpu_reservations);
            if cpu_mhz > remaining {
                return AdmissionControlResult {
                    allowed: false,
                    reason: Some(format!(
                        "CPU request {} MHz exceeds non-expandable reservation (remaining: {} MHz)",
                        cpu_mhz, remaining
                    )),
                    available_cpu_mhz: avail_cpu,
                    available_memory_mb: avail_mem,
                };
            }
        }

        if !pool.memory_expandable_reservation {
            let child_mem_reservations = Self::sum_child_reservations_mem(&pools, pool);
            let remaining = pool
                .memory_reservation_mb
                .saturating_sub(child_mem_reservations);
            if memory_mb > remaining {
                return AdmissionControlResult {
                    allowed: false,
                    reason: Some(format!(
                        "memory request {} MB exceeds non-expandable reservation (remaining: {} MB)",
                        memory_mb, remaining
                    )),
                    available_cpu_mhz: avail_cpu,
                    available_memory_mb: avail_mem,
                };
            }
        }

        AdmissionControlResult {
            allowed: true,
            reason: None,
            available_cpu_mhz: avail_cpu,
            available_memory_mb: avail_mem,
        }
    }

    // -- Tree traversal -----------------------------------------------------

    /// Return the pool identified by `root_id` together with all of its
    /// descendants (breadth-first).
    pub fn get_pool_tree(&self, root_id: &str) -> Vec<ResourcePool> {
        let pools = self.pools.read().unwrap_or_else(|e| e.into_inner());
        let mut result = Vec::new();

        let root = match pools.get(root_id) {
            Some(p) => p.clone(),
            None => return result,
        };

        let mut queue = std::collections::VecDeque::new();
        queue.push_back(root);

        while let Some(current) = queue.pop_front() {
            for child_id in &current.children {
                if let Some(child) = pools.get(child_id) {
                    queue.push_back(child.clone());
                }
            }
            result.push(current);
        }

        result
    }

    // -- Effective reservation ----------------------------------------------

    /// Return the *effective* CPU and memory reservation for a pool.
    ///
    /// If the pool has expandable reservation enabled and a parent exists, the
    /// effective reservation includes the parent's unreserved capacity
    /// (recursively).
    pub fn get_effective_reservation(&self, pool_id: &str) -> (u64, u64) {
        let pools = self.pools.read().unwrap_or_else(|e| e.into_inner());
        Self::effective_reservation_inner(&pools, pool_id)
    }

    fn effective_reservation_inner(
        pools: &HashMap<String, ResourcePool>,
        pool_id: &str,
    ) -> (u64, u64) {
        let pool = match pools.get(pool_id) {
            Some(p) => p,
            None => return (0, 0),
        };

        let mut eff_cpu = pool.cpu_reservation_mhz;
        let mut eff_mem = pool.memory_reservation_mb;

        // If expandable and there is a parent, add the parent's unreserved
        // capacity (i.e. parent reservation minus sum of sibling
        // reservations, then our own share of that surplus).
        if let Some(ref parent_id) = pool.parent_id {
            if let Some(parent) = pools.get(parent_id) {
                if pool.cpu_expandable_reservation {
                    let siblings_cpu = Self::sum_child_reservations_cpu(pools, parent);
                    let parent_unreserved = parent.cpu_reservation_mhz.saturating_sub(siblings_cpu);
                    eff_cpu += parent_unreserved;
                }
                if pool.memory_expandable_reservation {
                    let siblings_mem = Self::sum_child_reservations_mem(pools, parent);
                    let parent_unreserved =
                        parent.memory_reservation_mb.saturating_sub(siblings_mem);
                    eff_mem += parent_unreserved;
                }
            }
        }

        (eff_cpu, eff_mem)
    }

    // -- Available resources ------------------------------------------------

    /// Return the remaining (available) CPU and memory in a pool after
    /// accounting for child reservations and VM reservations.
    pub fn get_available_resources(&self, pool_id: &str) -> (u64, u64) {
        let pools = self.pools.read().unwrap_or_else(|e| e.into_inner());
        match pools.get(pool_id) {
            Some(pool) => Self::compute_available(&pools, pool),
            None => (0, 0),
        }
    }

    // -- Internal helpers ---------------------------------------------------

    /// Sum CPU reservations of all direct children of `pool`.
    fn sum_child_reservations_cpu(
        pools: &HashMap<String, ResourcePool>,
        pool: &ResourcePool,
    ) -> u64 {
        pool.children
            .iter()
            .filter_map(|cid| pools.get(cid))
            .map(|c| c.cpu_reservation_mhz)
            .sum()
    }

    /// Sum memory reservations of all direct children of `pool`.
    fn sum_child_reservations_mem(
        pools: &HashMap<String, ResourcePool>,
        pool: &ResourcePool,
    ) -> u64 {
        pool.children
            .iter()
            .filter_map(|cid| pools.get(cid))
            .map(|c| c.memory_reservation_mb)
            .sum()
    }

    /// Compute total used resources (child reservations + per-VM estimates).
    ///
    /// Uses the pool's CPU share level to derive a per-VM reservation estimate:
    /// - Low shares: 500 MHz / 512 MB per VM (lightweight workloads)
    /// - Normal shares: 1000 MHz / 1024 MB per VM (standard workloads)
    /// - High shares: 2000 MHz / 2048 MB per VM (compute-intensive workloads)
    /// - Custom: proportional to the custom share value
    ///
    /// The estimate is capped so VMs never exceed the pool's remaining capacity
    /// after child reservations.
    fn compute_used_resources(
        pools: &HashMap<String, ResourcePool>,
        pool: &ResourcePool,
    ) -> (u64, u64) {
        let child_cpu: u64 = Self::sum_child_reservations_cpu(pools, pool);
        let child_mem: u64 = Self::sum_child_reservations_mem(pools, pool);

        let vm_count = pool.vms.len() as u64;
        if vm_count == 0 {
            return (child_cpu, child_mem);
        }

        // Derive per-VM estimates from the pool's share level
        let per_vm_cpu = match &pool.cpu_shares {
            CpuShares::Low => 500,
            CpuShares::Normal => 1000,
            CpuShares::High => 2000,
            CpuShares::Custom(v) => (*v as u64) / 2,
        };
        let per_vm_mem = match &pool.memory_shares {
            MemoryShares::Low => 512,
            MemoryShares::Normal => 1024,
            MemoryShares::High => 2048,
            MemoryShares::Custom(v) => (*v as u64) / 2,
        };

        // Cap so VMs don't exceed the pool's remaining capacity
        let remaining_cpu = pool.cpu_reservation_mhz.saturating_sub(child_cpu);
        let remaining_mem = pool.memory_reservation_mb.saturating_sub(child_mem);

        let total_vm_cpu = (vm_count * per_vm_cpu).min(remaining_cpu);
        let total_vm_mem = (vm_count * per_vm_mem).min(remaining_mem);

        (child_cpu + total_vm_cpu, child_mem + total_vm_mem)
    }

    /// Compute available (remaining) resources for a pool.
    fn compute_available(pools: &HashMap<String, ResourcePool>, pool: &ResourcePool) -> (u64, u64) {
        let child_cpu = Self::sum_child_reservations_cpu(pools, pool);
        let child_mem = Self::sum_child_reservations_mem(pools, pool);

        let base_cpu = pool.cpu_limit_mhz.unwrap_or(pool.cpu_reservation_mhz);
        let base_mem = pool.memory_limit_mb.unwrap_or(pool.memory_reservation_mb);

        let avail_cpu = base_cpu.saturating_sub(child_cpu);
        let avail_mem = base_mem.saturating_sub(child_mem);

        (avail_cpu, avail_mem)
    }
}

impl Default for ResourcePoolManager {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    /// Helper: build a basic create-pool request.
    fn root_pool_request(name: &str, cluster: &str) -> CreateResourcePoolRequest {
        CreateResourcePoolRequest {
            name: name.to_string(),
            parent_id: None,
            cluster_id: cluster.to_string(),
            cpu_shares: CpuShares::Normal,
            cpu_reservation_mhz: 4000,
            cpu_limit_mhz: Some(8000),
            cpu_expandable_reservation: true,
            memory_shares: MemoryShares::Normal,
            memory_reservation_mb: 8192,
            memory_limit_mb: Some(16384),
            memory_expandable_reservation: true,
        }
    }

    fn child_pool_request(name: &str, parent_id: &str, cluster: &str) -> CreateResourcePoolRequest {
        CreateResourcePoolRequest {
            name: name.to_string(),
            parent_id: Some(parent_id.to_string()),
            cluster_id: cluster.to_string(),
            cpu_shares: CpuShares::Normal,
            cpu_reservation_mhz: 1000,
            cpu_limit_mhz: Some(2000),
            cpu_expandable_reservation: true,
            memory_shares: MemoryShares::Normal,
            memory_reservation_mb: 2048,
            memory_limit_mb: Some(4096),
            memory_expandable_reservation: true,
        }
    }

    // -- 1. Root pool creation -----------------------------------------------

    #[test]
    fn test_create_root_pool() {
        let mgr = ResourcePoolManager::new();
        let pool = mgr.create_pool(root_pool_request("prod", "c1")).unwrap();

        assert_eq!(pool.name, "prod");
        assert!(pool.parent_id.is_none());
        assert_eq!(pool.cluster_id, "c1");
        assert_eq!(pool.cpu_reservation_mhz, 4000);
        assert!(pool.vms.is_empty());
        assert!(pool.children.is_empty());
        assert!(pool.updated.is_none());
    }

    // -- 2. Nested pool creation ---------------------------------------------

    #[test]
    fn test_create_nested_pool() {
        let mgr = ResourcePoolManager::new();
        let root = mgr.create_pool(root_pool_request("prod", "c1")).unwrap();
        let child = mgr
            .create_pool(child_pool_request("web-tier", &root.id, "c1"))
            .unwrap();

        assert_eq!(child.parent_id.as_deref(), Some(root.id.as_str()));

        // Parent should list child.
        let parent = mgr.get_pool(&root.id).unwrap();
        assert!(parent.children.contains(&child.id));
    }

    // -- 3. Create pool with invalid parent ----------------------------------

    #[test]
    fn test_create_pool_invalid_parent() {
        let mgr = ResourcePoolManager::new();
        let req = child_pool_request("orphan", "nonexistent", "c1");
        let result = mgr.create_pool(req);
        assert!(result.is_err());
    }

    // -- 4. VM assignment / unassignment -------------------------------------

    #[test]
    fn test_assign_and_unassign_vm() {
        let mgr = ResourcePoolManager::new();
        let pool = mgr.create_pool(root_pool_request("prod", "c1")).unwrap();

        mgr.assign_vm(&pool.id, "vm-1").unwrap();
        mgr.assign_vm(&pool.id, "vm-2").unwrap();

        let p = mgr.get_pool(&pool.id).unwrap();
        assert_eq!(p.vms.len(), 2);
        assert!(p.vms.contains(&"vm-1".to_string()));

        mgr.unassign_vm(&pool.id, "vm-1").unwrap();
        let p = mgr.get_pool(&pool.id).unwrap();
        assert_eq!(p.vms.len(), 1);
        assert!(!p.vms.contains(&"vm-1".to_string()));
    }

    // -- 5. Duplicate VM assignment should fail ------------------------------

    #[test]
    fn test_assign_vm_duplicate() {
        let mgr = ResourcePoolManager::new();
        let pool = mgr.create_pool(root_pool_request("prod", "c1")).unwrap();

        mgr.assign_vm(&pool.id, "vm-1").unwrap();
        let result = mgr.assign_vm(&pool.id, "vm-1");
        assert!(result.is_err());
    }

    // -- 6. Admission control - accept ---------------------------------------

    #[test]
    fn test_admission_control_accept() {
        let mgr = ResourcePoolManager::new();
        let pool = mgr.create_pool(root_pool_request("prod", "c1")).unwrap();

        let result = mgr.check_admission(&pool.id, 1000, 2048);
        assert!(result.allowed);
        assert!(result.reason.is_none());
        assert!(result.available_cpu_mhz > 0);
        assert!(result.available_memory_mb > 0);
    }

    // -- 7. Admission control - reject (exceeds limit) -----------------------

    #[test]
    fn test_admission_control_reject_limit() {
        let mgr = ResourcePoolManager::new();
        let pool = mgr.create_pool(root_pool_request("prod", "c1")).unwrap();

        // The pool has cpu_limit_mhz=8000 and memory_limit_mb=16384.
        // Request more than the limit.
        let result = mgr.check_admission(&pool.id, 9000, 1024);
        assert!(!result.allowed);
        assert!(result.reason.is_some());
    }

    // -- 8. Admission control - reject (non-expandable) ----------------------

    #[test]
    fn test_admission_control_reject_non_expandable() {
        let mgr = ResourcePoolManager::new();
        let req = CreateResourcePoolRequest {
            name: "strict".to_string(),
            parent_id: None,
            cluster_id: "c1".to_string(),
            cpu_shares: CpuShares::Normal,
            cpu_reservation_mhz: 2000,
            cpu_limit_mhz: None,
            cpu_expandable_reservation: false,
            memory_shares: MemoryShares::Normal,
            memory_reservation_mb: 4096,
            memory_limit_mb: None,
            memory_expandable_reservation: false,
        };
        let pool = mgr.create_pool(req).unwrap();

        // Request more CPU than the non-expandable reservation.
        let result = mgr.check_admission(&pool.id, 3000, 1024);
        assert!(!result.allowed);
        assert!(result.reason.as_deref().unwrap().contains("non-expandable"));
    }

    // -- 9. Delete pool with children should fail ----------------------------

    #[test]
    fn test_delete_pool_with_children_fails() {
        let mgr = ResourcePoolManager::new();
        let root = mgr.create_pool(root_pool_request("prod", "c1")).unwrap();
        let _child = mgr
            .create_pool(child_pool_request("web-tier", &root.id, "c1"))
            .unwrap();

        let result = mgr.delete_pool(&root.id);
        assert!(result.is_err());
        let err_msg = format!("{}", result.unwrap_err());
        assert!(err_msg.contains("child"));
    }

    // -- 10. Delete pool with VMs should fail --------------------------------

    #[test]
    fn test_delete_pool_with_vms_fails() {
        let mgr = ResourcePoolManager::new();
        let pool = mgr.create_pool(root_pool_request("prod", "c1")).unwrap();
        mgr.assign_vm(&pool.id, "vm-1").unwrap();

        let result = mgr.delete_pool(&pool.id);
        assert!(result.is_err());
        let err_msg = format!("{}", result.unwrap_err());
        assert!(err_msg.contains("VM"));
    }

    // -- 11. Pool tree traversal ---------------------------------------------

    #[test]
    fn test_pool_tree_traversal() {
        let mgr = ResourcePoolManager::new();
        let root = mgr.create_pool(root_pool_request("prod", "c1")).unwrap();
        let child_a = mgr
            .create_pool(child_pool_request("web", &root.id, "c1"))
            .unwrap();
        let _child_b = mgr
            .create_pool(child_pool_request("db", &root.id, "c1"))
            .unwrap();
        let _grandchild = mgr
            .create_pool(child_pool_request("web-frontend", &child_a.id, "c1"))
            .unwrap();

        let tree = mgr.get_pool_tree(&root.id);
        assert_eq!(tree.len(), 4);
        // Root should be first.
        assert_eq!(tree[0].id, root.id);
    }

    // -- 12. Effective reservation calculation -------------------------------

    #[test]
    fn test_effective_reservation() {
        let mgr = ResourcePoolManager::new();
        let root = mgr.create_pool(root_pool_request("prod", "c1")).unwrap();
        // root: cpu_reservation=4000, mem_reservation=8192

        let child = mgr
            .create_pool(child_pool_request("web", &root.id, "c1"))
            .unwrap();
        // child: cpu_reservation=1000, mem_reservation=2048, expandable=true

        let (eff_cpu, eff_mem) = mgr.get_effective_reservation(&child.id);

        // Effective = own reservation + parent unreserved.
        // Parent unreserved CPU = 4000 - 1000 (sum of children) = 3000
        // Effective CPU = 1000 + 3000 = 4000
        assert_eq!(eff_cpu, 4000);
        // Parent unreserved MEM = 8192 - 2048 = 6144
        // Effective MEM = 2048 + 6144 = 8192
        assert_eq!(eff_mem, 8192);
    }

    // -- 13. CpuShares / MemoryShares value calculation ----------------------

    #[test]
    fn test_share_values() {
        assert_eq!(CpuShares::Low.value(), 1000);
        assert_eq!(CpuShares::Normal.value(), 2000);
        assert_eq!(CpuShares::High.value(), 4000);
        assert_eq!(CpuShares::Custom(3000).value(), 3000);

        assert_eq!(MemoryShares::Low.value(), 1000);
        assert_eq!(MemoryShares::Normal.value(), 2000);
        assert_eq!(MemoryShares::High.value(), 4000);
        assert_eq!(MemoryShares::Custom(5000).value(), 5000);
    }

    // -- 14. Move VM between pools -------------------------------------------

    #[test]
    fn test_move_vm() {
        let mgr = ResourcePoolManager::new();
        let pool_a = mgr.create_pool(root_pool_request("pool-a", "c1")).unwrap();
        let pool_b = mgr.create_pool(root_pool_request("pool-b", "c1")).unwrap();

        mgr.assign_vm(&pool_a.id, "vm-1").unwrap();
        mgr.move_vm(&pool_a.id, &pool_b.id, "vm-1").unwrap();

        let a = mgr.get_pool(&pool_a.id).unwrap();
        let b = mgr.get_pool(&pool_b.id).unwrap();
        assert!(!a.vms.contains(&"vm-1".to_string()));
        assert!(b.vms.contains(&"vm-1".to_string()));
    }

    // -- 15. List pools with cluster filter ----------------------------------

    #[test]
    fn test_list_pools_filter() {
        let mgr = ResourcePoolManager::new();
        mgr.create_pool(root_pool_request("prod", "c1")).unwrap();
        mgr.create_pool(root_pool_request("dev", "c2")).unwrap();

        let all = mgr.list_pools(None);
        assert_eq!(all.len(), 2);

        let c1_only = mgr.list_pools(Some("c1"));
        assert_eq!(c1_only.len(), 1);
        assert_eq!(c1_only[0].name, "prod");
    }

    // -- 16. Pool summary ----------------------------------------------------

    #[test]
    fn test_pool_summary() {
        let mgr = ResourcePoolManager::new();
        let root = mgr.create_pool(root_pool_request("prod", "c1")).unwrap();
        let _child = mgr
            .create_pool(child_pool_request("web", &root.id, "c1"))
            .unwrap();
        mgr.assign_vm(&root.id, "vm-1").unwrap();

        let summary = mgr.get_pool_summary(&root.id).unwrap();
        assert_eq!(summary.vm_count, 1);
        assert_eq!(summary.child_pool_count, 1);
        assert_eq!(summary.cpu_reservation_mhz, 4000);
    }

    // -- 17. Update pool partial update --------------------------------------

    #[test]
    fn test_update_pool() {
        let mgr = ResourcePoolManager::new();
        let pool = mgr.create_pool(root_pool_request("prod", "c1")).unwrap();

        let updated = mgr
            .update_pool(
                &pool.id,
                UpdateResourcePoolRequest {
                    name: Some("production".to_string()),
                    cpu_reservation_mhz: Some(6000),
                    ..Default::default()
                },
            )
            .unwrap();

        assert_eq!(updated.name, "production");
        assert_eq!(updated.cpu_reservation_mhz, 6000);
        // Unchanged fields remain the same.
        assert_eq!(updated.memory_reservation_mb, 8192);
        assert!(updated.updated.is_some());
    }

    // -- 18. Available resources ---------------------------------------------

    #[test]
    fn test_available_resources() {
        let mgr = ResourcePoolManager::new();
        let root = mgr.create_pool(root_pool_request("prod", "c1")).unwrap();
        // root: cpu_limit=8000, mem_limit=16384

        let _child = mgr
            .create_pool(child_pool_request("web", &root.id, "c1"))
            .unwrap();
        // child: cpu_reservation=1000, mem_reservation=2048

        let (avail_cpu, avail_mem) = mgr.get_available_resources(&root.id);
        // available = limit - child_reservations
        assert_eq!(avail_cpu, 8000 - 1000);
        assert_eq!(avail_mem, 16384 - 2048);
    }

    // -- 19. Delete empty pool succeeds --------------------------------------

    #[test]
    fn test_delete_empty_pool() {
        let mgr = ResourcePoolManager::new();
        let pool = mgr.create_pool(root_pool_request("temp", "c1")).unwrap();
        let id = pool.id.clone();

        mgr.delete_pool(&id).unwrap();
        assert!(mgr.get_pool(&id).is_none());
    }
}
