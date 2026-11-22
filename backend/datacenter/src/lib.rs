// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: LicenseRef-Zyvor-Production-2.0

use anyhow::{anyhow, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use uuid::Uuid;

// ---------------------------------------------------------------------------
// Status enums
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DatacenterStatus {
    Active,
    Maintenance,
    Disabled,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ClusterStatus {
    Active,
    Maintenance,
    Degraded,
    Offline,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HostStatus {
    Connected,
    Disconnected,
    Maintenance,
    NotResponding,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DrsMode {
    #[default]
    Manual,
    PartiallyAutomated,
    FullyAutomated,
}

// ---------------------------------------------------------------------------
// Core data models
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Datacenter {
    pub id: String,
    pub name: String,
    pub description: String,
    pub clusters: Vec<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub status: DatacenterStatus,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Cluster {
    pub id: String,
    pub name: String,
    pub description: String,
    pub datacenter_id: String,
    pub hosts: Vec<String>,
    pub ha_enabled: bool,
    pub drs_enabled: bool,
    pub drs_mode: DrsMode,
    pub evc_mode: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub status: ClusterStatus,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HostInfo {
    pub id: String,
    pub hostname: String,
    pub address: String,
    pub cluster_id: String,
    pub datacenter_id: String,
    pub cpus: u32,
    pub memory_mb: u64,
    pub status: HostStatus,
    pub last_heartbeat: DateTime<Utc>,
    pub vm_count: u32,
    pub cpu_usage_pct: f64,
    pub memory_usage_pct: f64,
    pub agent_version: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    /// Whether this host's containerd is configured with the FluxVM Secure
    /// Containers runtime (`RuntimeClass fluxvm`) and can accept
    /// `ContainerGroup` placement. Reported by the host agent on heartbeat.
    #[serde(default)]
    pub secure_containers_ready: bool,
    /// Nested FluxVM `/readyz.secure_containers` fields when the agent
    /// probes the local daemon. Kept optional for older agents.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub secure_containers: Option<SecureContainersDetail>,
}

/// Nested Secure Containers readiness from FluxVM `/readyz`.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct SecureContainersDetail {
    pub available: bool,
    pub shim_installed: bool,
    pub guest_image_present: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DatacenterSummary {
    pub id: String,
    pub name: String,
    pub cluster_count: usize,
    pub host_count: usize,
    pub vm_count: u32,
    pub total_cpus: u32,
    pub total_memory_mb: u64,
}

// ---------------------------------------------------------------------------
// Heartbeat payload
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HostHeartbeat {
    pub cpu_usage_pct: f64,
    pub memory_usage_pct: f64,
    pub vm_count: u32,
    pub uptime_secs: u64,
    #[serde(default)]
    pub secure_containers_ready: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub secure_containers: Option<SecureContainersDetail>,
}

// ---------------------------------------------------------------------------
// Request structs
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateDatacenterRequest {
    pub name: String,
    #[serde(default)]
    pub description: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateDatacenterRequest {
    pub name: Option<String>,
    pub description: Option<String>,
    pub status: Option<DatacenterStatus>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateClusterRequest {
    pub name: String,
    #[serde(default)]
    pub description: String,
    pub datacenter_id: String,
    #[serde(default)]
    pub ha_enabled: bool,
    #[serde(default)]
    pub drs_enabled: bool,
    #[serde(default)]
    pub drs_mode: DrsMode,
    #[serde(default)]
    pub evc_mode: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateClusterRequest {
    pub name: Option<String>,
    pub description: Option<String>,
    pub ha_enabled: Option<bool>,
    pub drs_enabled: Option<bool>,
    pub drs_mode: Option<DrsMode>,
    pub evc_mode: Option<Option<String>>,
    pub status: Option<ClusterStatus>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegisterHostRequest {
    pub hostname: String,
    pub address: String,
    pub cluster_id: String,
    pub cpus: u32,
    pub memory_mb: u64,
    pub agent_version: String,
    #[serde(default)]
    pub secure_containers_ready: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateHostRequest {
    pub hostname: Option<String>,
    pub address: Option<String>,
    pub cpus: Option<u32>,
    pub memory_mb: Option<u64>,
    pub agent_version: Option<String>,
    pub status: Option<HostStatus>,
}

// ---------------------------------------------------------------------------
// In-memory store
// ---------------------------------------------------------------------------

#[derive(Debug, Default)]
struct Store {
    datacenters: HashMap<String, Datacenter>,
    clusters: HashMap<String, Cluster>,
    hosts: HashMap<String, HostInfo>,
}

// ---------------------------------------------------------------------------
// DatacenterManager
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct DatacenterManager {
    store: Arc<RwLock<Store>>,
}

impl DatacenterManager {
    /// Create a new DatacenterManager with empty in-memory stores.
    pub fn new() -> Self {
        Self {
            store: Arc::new(RwLock::new(Store::default())),
        }
    }

    // -- Datacenter operations -----------------------------------------------

    /// Create a new datacenter.
    pub fn create_datacenter(&self, req: CreateDatacenterRequest) -> Result<Datacenter> {
        let now = Utc::now();
        let dc = Datacenter {
            id: Uuid::new_v4().to_string(),
            name: req.name,
            description: req.description,
            clusters: Vec::new(),
            created_at: now,
            updated_at: now,
            status: DatacenterStatus::Active,
        };

        let mut store = self
            .store
            .write()
            .map_err(|e| anyhow!("lock poisoned: {e}"))?;
        tracing::info!(id = %dc.id, name = %dc.name, "created datacenter");
        store.datacenters.insert(dc.id.clone(), dc.clone());
        Ok(dc)
    }

    /// Retrieve a datacenter by ID.
    pub fn get_datacenter(&self, id: &str) -> Result<Option<Datacenter>> {
        let store = self
            .store
            .read()
            .map_err(|e| anyhow!("lock poisoned: {e}"))?;
        Ok(store.datacenters.get(id).cloned())
    }

    /// List all datacenters.
    pub fn list_datacenters(&self) -> Vec<Datacenter> {
        let store = self.store.read().unwrap_or_else(|e| e.into_inner());
        store.datacenters.values().cloned().collect()
    }

    /// Update an existing datacenter.
    pub fn update_datacenter(&self, id: &str, req: UpdateDatacenterRequest) -> Result<Datacenter> {
        let mut store = self
            .store
            .write()
            .map_err(|e| anyhow!("lock poisoned: {e}"))?;
        let dc = store
            .datacenters
            .get_mut(id)
            .ok_or_else(|| anyhow!("datacenter not found: {id}"))?;

        if let Some(name) = req.name {
            dc.name = name;
        }
        if let Some(description) = req.description {
            dc.description = description;
        }
        if let Some(status) = req.status {
            dc.status = status;
        }
        dc.updated_at = Utc::now();

        tracing::info!(id = %dc.id, "updated datacenter");
        Ok(dc.clone())
    }

    /// Delete a datacenter and all its clusters and hosts.
    pub fn delete_datacenter(&self, id: &str) -> Result<()> {
        let mut store = self
            .store
            .write()
            .map_err(|e| anyhow!("lock poisoned: {e}"))?;

        let dc = store
            .datacenters
            .remove(id)
            .ok_or_else(|| anyhow!("datacenter not found: {id}"))?;

        // Cascade-delete clusters belonging to this datacenter.
        let cluster_ids: Vec<String> = dc.clusters.clone();
        for cid in &cluster_ids {
            if let Some(cluster) = store.clusters.remove(cid) {
                // Cascade-delete hosts belonging to this cluster.
                for hid in &cluster.hosts {
                    store.hosts.remove(hid);
                }
            }
        }

        tracing::info!(id = %id, "deleted datacenter");
        Ok(())
    }

    /// Produce an aggregate summary for a datacenter.
    pub fn get_datacenter_summary(&self, id: &str) -> Result<DatacenterSummary> {
        let store = self
            .store
            .read()
            .map_err(|e| anyhow!("lock poisoned: {e}"))?;
        let dc = store
            .datacenters
            .get(id)
            .ok_or_else(|| anyhow!("datacenter not found: {id}"))?;

        let clusters: Vec<&Cluster> = dc
            .clusters
            .iter()
            .filter_map(|cid| store.clusters.get(cid))
            .collect();

        let hosts: Vec<&HostInfo> = clusters
            .iter()
            .flat_map(|c| c.hosts.iter())
            .filter_map(|hid| store.hosts.get(hid))
            .collect();

        let vm_count: u32 = hosts.iter().map(|h| h.vm_count).sum();
        let total_cpus: u32 = hosts.iter().map(|h| h.cpus).sum();
        let total_memory_mb: u64 = hosts.iter().map(|h| h.memory_mb).sum();

        Ok(DatacenterSummary {
            id: dc.id.clone(),
            name: dc.name.clone(),
            cluster_count: clusters.len(),
            host_count: hosts.len(),
            vm_count,
            total_cpus,
            total_memory_mb,
        })
    }

    // -- Cluster operations --------------------------------------------------

    /// Create a new cluster within a datacenter.
    pub fn create_cluster(&self, req: CreateClusterRequest) -> Result<Cluster> {
        let mut store = self
            .store
            .write()
            .map_err(|e| anyhow!("lock poisoned: {e}"))?;

        // Verify that the parent datacenter exists.
        let dc = store
            .datacenters
            .get_mut(&req.datacenter_id)
            .ok_or_else(|| anyhow!("datacenter not found: {}", req.datacenter_id))?;

        let now = Utc::now();
        let cluster = Cluster {
            id: Uuid::new_v4().to_string(),
            name: req.name,
            description: req.description,
            datacenter_id: req.datacenter_id,
            hosts: Vec::new(),
            ha_enabled: req.ha_enabled,
            drs_enabled: req.drs_enabled,
            drs_mode: req.drs_mode,
            evc_mode: req.evc_mode,
            created_at: now,
            updated_at: now,
            status: ClusterStatus::Active,
        };

        dc.clusters.push(cluster.id.clone());
        dc.updated_at = Utc::now();

        tracing::info!(id = %cluster.id, name = %cluster.name, datacenter = %cluster.datacenter_id, "created cluster");
        store.clusters.insert(cluster.id.clone(), cluster.clone());
        Ok(cluster)
    }

    /// Retrieve a cluster by ID.
    pub fn get_cluster(&self, id: &str) -> Result<Option<Cluster>> {
        let store = self
            .store
            .read()
            .map_err(|e| anyhow!("lock poisoned: {e}"))?;
        Ok(store.clusters.get(id).cloned())
    }

    /// List clusters, optionally filtered by datacenter ID.
    pub fn list_clusters(&self, datacenter_id: Option<&str>) -> Vec<Cluster> {
        let store = self.store.read().unwrap_or_else(|e| e.into_inner());
        store
            .clusters
            .values()
            .filter(|c| match datacenter_id {
                Some(dc_id) => c.datacenter_id == dc_id,
                None => true,
            })
            .cloned()
            .collect()
    }

    /// Update an existing cluster.
    pub fn update_cluster(&self, id: &str, req: UpdateClusterRequest) -> Result<Cluster> {
        let mut store = self
            .store
            .write()
            .map_err(|e| anyhow!("lock poisoned: {e}"))?;
        let cluster = store
            .clusters
            .get_mut(id)
            .ok_or_else(|| anyhow!("cluster not found: {id}"))?;

        if let Some(name) = req.name {
            cluster.name = name;
        }
        if let Some(description) = req.description {
            cluster.description = description;
        }
        if let Some(ha) = req.ha_enabled {
            cluster.ha_enabled = ha;
        }
        if let Some(drs) = req.drs_enabled {
            cluster.drs_enabled = drs;
        }
        if let Some(mode) = req.drs_mode {
            cluster.drs_mode = mode;
        }
        if let Some(evc) = req.evc_mode {
            cluster.evc_mode = evc;
        }
        if let Some(status) = req.status {
            cluster.status = status;
        }
        cluster.updated_at = Utc::now();

        tracing::info!(id = %cluster.id, "updated cluster");
        Ok(cluster.clone())
    }

    /// Delete a cluster and all its hosts.
    pub fn delete_cluster(&self, id: &str) -> Result<()> {
        let mut store = self
            .store
            .write()
            .map_err(|e| anyhow!("lock poisoned: {e}"))?;

        let cluster = store
            .clusters
            .remove(id)
            .ok_or_else(|| anyhow!("cluster not found: {id}"))?;

        // Remove the cluster reference from its parent datacenter.
        if let Some(dc) = store.datacenters.get_mut(&cluster.datacenter_id) {
            dc.clusters.retain(|cid| cid != id);
            dc.updated_at = Utc::now();
        }

        // Cascade-delete hosts belonging to this cluster.
        for hid in &cluster.hosts {
            store.hosts.remove(hid);
        }

        tracing::info!(id = %id, "deleted cluster");
        Ok(())
    }

    // -- Host operations -----------------------------------------------------

    /// Register a new host within a cluster.
    pub fn register_host(&self, req: RegisterHostRequest) -> Result<HostInfo> {
        let mut store = self
            .store
            .write()
            .map_err(|e| anyhow!("lock poisoned: {e}"))?;

        // Verify the parent cluster exists and resolve the datacenter ID.
        let cluster = store
            .clusters
            .get_mut(&req.cluster_id)
            .ok_or_else(|| anyhow!("cluster not found: {}", req.cluster_id))?;

        let datacenter_id = cluster.datacenter_id.clone();
        let now = Utc::now();
        let host = HostInfo {
            id: Uuid::new_v4().to_string(),
            hostname: req.hostname,
            address: req.address,
            cluster_id: req.cluster_id,
            datacenter_id,
            cpus: req.cpus,
            memory_mb: req.memory_mb,
            status: HostStatus::Connected,
            last_heartbeat: now,
            vm_count: 0,
            cpu_usage_pct: 0.0,
            memory_usage_pct: 0.0,
            agent_version: req.agent_version,
            created_at: now,
            updated_at: now,
            secure_containers_ready: req.secure_containers_ready,
            secure_containers: None,
        };

        cluster.hosts.push(host.id.clone());
        cluster.updated_at = Utc::now();

        tracing::info!(id = %host.id, hostname = %host.hostname, cluster = %host.cluster_id, "registered host");
        store.hosts.insert(host.id.clone(), host.clone());
        Ok(host)
    }

    /// Retrieve a host by ID.
    pub fn get_host(&self, id: &str) -> Result<Option<HostInfo>> {
        let store = self
            .store
            .read()
            .map_err(|e| anyhow!("lock poisoned: {e}"))?;
        Ok(store.hosts.get(id).cloned())
    }

    /// List hosts, optionally filtered by cluster ID.
    pub fn list_hosts(&self, cluster_id: Option<&str>) -> Vec<HostInfo> {
        let store = self.store.read().unwrap_or_else(|e| e.into_inner());
        store
            .hosts
            .values()
            .filter(|h| match cluster_id {
                Some(cid) => h.cluster_id == cid,
                None => true,
            })
            .cloned()
            .collect()
    }

    /// Update an existing host's configuration.
    pub fn update_host(&self, id: &str, req: UpdateHostRequest) -> Result<HostInfo> {
        let mut store = self
            .store
            .write()
            .map_err(|e| anyhow!("lock poisoned: {e}"))?;
        let host = store
            .hosts
            .get_mut(id)
            .ok_or_else(|| anyhow!("host not found: {id}"))?;

        if let Some(hostname) = req.hostname {
            host.hostname = hostname;
        }
        if let Some(address) = req.address {
            host.address = address;
        }
        if let Some(cpus) = req.cpus {
            host.cpus = cpus;
        }
        if let Some(memory_mb) = req.memory_mb {
            host.memory_mb = memory_mb;
        }
        if let Some(agent_version) = req.agent_version {
            host.agent_version = agent_version;
        }
        if let Some(status) = req.status {
            host.status = status;
        }
        host.updated_at = Utc::now();

        tracing::info!(id = %host.id, "updated host");
        Ok(host.clone())
    }

    /// Remove a host from its cluster and from the store.
    pub fn remove_host(&self, id: &str) -> Result<()> {
        let mut store = self
            .store
            .write()
            .map_err(|e| anyhow!("lock poisoned: {e}"))?;

        let host = store
            .hosts
            .remove(id)
            .ok_or_else(|| anyhow!("host not found: {id}"))?;

        // Remove the host reference from its parent cluster.
        if let Some(cluster) = store.clusters.get_mut(&host.cluster_id) {
            cluster.hosts.retain(|hid| hid != id);
            cluster.updated_at = Utc::now();
        }

        tracing::info!(id = %id, "removed host");
        Ok(())
    }

    /// Process a heartbeat from a host, updating its metrics and timestamp.
    pub fn update_host_heartbeat(&self, id: &str, metrics: HostHeartbeat) -> Result<()> {
        let mut store = self
            .store
            .write()
            .map_err(|e| anyhow!("lock poisoned: {e}"))?;
        let host = store
            .hosts
            .get_mut(id)
            .ok_or_else(|| anyhow!("host not found: {id}"))?;

        host.cpu_usage_pct = metrics.cpu_usage_pct;
        host.memory_usage_pct = metrics.memory_usage_pct;
        host.vm_count = metrics.vm_count;
        host.secure_containers_ready = metrics
            .secure_containers
            .as_ref()
            .map(|d| d.available)
            .unwrap_or(metrics.secure_containers_ready);
        host.secure_containers = metrics.secure_containers.clone();
        host.last_heartbeat = Utc::now();
        host.updated_at = host.last_heartbeat;

        tracing::debug!(id = %id, cpu = %metrics.cpu_usage_pct, mem = %metrics.memory_usage_pct, "host heartbeat");
        Ok(())
    }

    /// Convenience: list all hosts that belong to a specific cluster.
    pub fn get_cluster_hosts(&self, cluster_id: &str) -> Vec<HostInfo> {
        self.list_hosts(Some(cluster_id))
    }

    /// Put a host into maintenance mode.
    pub fn enter_maintenance_mode(&self, host_id: &str) -> Result<()> {
        let mut store = self
            .store
            .write()
            .map_err(|e| anyhow!("lock poisoned: {e}"))?;
        let host = store
            .hosts
            .get_mut(host_id)
            .ok_or_else(|| anyhow!("host not found: {host_id}"))?;

        host.status = HostStatus::Maintenance;
        host.updated_at = Utc::now();

        tracing::info!(id = %host_id, "host entered maintenance mode");
        Ok(())
    }

    /// Take a host out of maintenance mode (returns it to Connected status).
    pub fn exit_maintenance_mode(&self, host_id: &str) -> Result<()> {
        let mut store = self
            .store
            .write()
            .map_err(|e| anyhow!("lock poisoned: {e}"))?;
        let host = store
            .hosts
            .get_mut(host_id)
            .ok_or_else(|| anyhow!("host not found: {host_id}"))?;

        host.status = HostStatus::Connected;
        host.updated_at = Utc::now();

        tracing::info!(id = %host_id, "host exited maintenance mode");
        Ok(())
    }
}

impl Default for DatacenterManager {
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

    /// Helper: create a manager with one datacenter already inserted.
    fn setup_with_datacenter() -> (DatacenterManager, Datacenter) {
        let mgr = DatacenterManager::new();
        let dc = mgr
            .create_datacenter(CreateDatacenterRequest {
                name: "dc-east".into(),
                description: "US East datacenter".into(),
            })
            .unwrap();
        (mgr, dc)
    }

    /// Helper: create a manager with a datacenter and a cluster.
    fn setup_with_cluster() -> (DatacenterManager, Datacenter, Cluster) {
        let (mgr, dc) = setup_with_datacenter();
        let cluster = mgr
            .create_cluster(CreateClusterRequest {
                name: "prod-cluster".into(),
                description: "Production workloads".into(),
                datacenter_id: dc.id.clone(),
                ha_enabled: true,
                drs_enabled: true,
                drs_mode: DrsMode::FullyAutomated,
                evc_mode: None,
            })
            .unwrap();
        (mgr, dc, cluster)
    }

    /// Helper: create a manager with a datacenter, cluster, and host.
    fn setup_with_host() -> (DatacenterManager, Datacenter, Cluster, HostInfo) {
        let (mgr, dc, cluster) = setup_with_cluster();
        let host = mgr
            .register_host(RegisterHostRequest {
                hostname: "esxi-01.lab.local".into(),
                address: "10.0.0.11".into(),
                cluster_id: cluster.id.clone(),
                cpus: 32,
                memory_mb: 131072,
                agent_version: "0.1.0".into(),
                secure_containers_ready: false,
            })
            .unwrap();
        (mgr, dc, cluster, host)
    }

    #[test]
    fn test_create_and_get_datacenter() {
        let (mgr, dc) = setup_with_datacenter();
        let fetched = mgr.get_datacenter(&dc.id).unwrap().unwrap();
        assert_eq!(fetched.name, "dc-east");
        assert_eq!(fetched.status, DatacenterStatus::Active);
        assert!(fetched.clusters.is_empty());
    }

    #[test]
    fn test_list_datacenters() {
        let mgr = DatacenterManager::new();
        mgr.create_datacenter(CreateDatacenterRequest {
            name: "dc-east".into(),
            description: "East".into(),
        })
        .unwrap();
        mgr.create_datacenter(CreateDatacenterRequest {
            name: "dc-west".into(),
            description: "West".into(),
        })
        .unwrap();

        let dcs = mgr.list_datacenters();
        assert_eq!(dcs.len(), 2);
    }

    #[test]
    fn test_update_datacenter() {
        let (mgr, dc) = setup_with_datacenter();
        let updated = mgr
            .update_datacenter(
                &dc.id,
                UpdateDatacenterRequest {
                    name: Some("dc-east-renamed".into()),
                    description: None,
                    status: Some(DatacenterStatus::Maintenance),
                },
            )
            .unwrap();
        assert_eq!(updated.name, "dc-east-renamed");
        assert_eq!(updated.status, DatacenterStatus::Maintenance);
        assert!(updated.updated_at >= dc.updated_at);
    }

    #[test]
    fn test_delete_datacenter_cascades() {
        let (mgr, _dc, _cluster, host) = setup_with_host();
        let dc_id = _dc.id.clone();

        // Verify host exists first.
        assert!(mgr.get_host(&host.id).unwrap().is_some());

        mgr.delete_datacenter(&dc_id).unwrap();

        // Datacenter, cluster, and host should all be gone.
        assert!(mgr.get_datacenter(&dc_id).unwrap().is_none());
        assert!(mgr.get_cluster(&_cluster.id).unwrap().is_none());
        assert!(mgr.get_host(&host.id).unwrap().is_none());
    }

    #[test]
    fn test_create_cluster_requires_valid_datacenter() {
        let mgr = DatacenterManager::new();
        let result = mgr.create_cluster(CreateClusterRequest {
            name: "orphan-cluster".into(),
            description: "should fail".into(),
            datacenter_id: "nonexistent".into(),
            ha_enabled: false,
            drs_enabled: false,
            drs_mode: DrsMode::Manual,
            evc_mode: None,
        });
        assert!(result.is_err());
    }

    #[test]
    fn test_list_clusters_with_filter() {
        let mgr = DatacenterManager::new();
        let dc1 = mgr
            .create_datacenter(CreateDatacenterRequest {
                name: "dc-1".into(),
                description: "first".into(),
            })
            .unwrap();
        let dc2 = mgr
            .create_datacenter(CreateDatacenterRequest {
                name: "dc-2".into(),
                description: "second".into(),
            })
            .unwrap();

        mgr.create_cluster(CreateClusterRequest {
            name: "cluster-a".into(),
            description: "".into(),
            datacenter_id: dc1.id.clone(),
            ha_enabled: false,
            drs_enabled: false,
            drs_mode: DrsMode::Manual,
            evc_mode: None,
        })
        .unwrap();
        mgr.create_cluster(CreateClusterRequest {
            name: "cluster-b".into(),
            description: "".into(),
            datacenter_id: dc2.id.clone(),
            ha_enabled: false,
            drs_enabled: false,
            drs_mode: DrsMode::Manual,
            evc_mode: None,
        })
        .unwrap();

        assert_eq!(mgr.list_clusters(None).len(), 2);
        assert_eq!(mgr.list_clusters(Some(&dc1.id)).len(), 1);
        assert_eq!(mgr.list_clusters(Some(&dc2.id)).len(), 1);
    }

    #[test]
    fn test_register_and_get_host() {
        let (mgr, _dc, cluster, host) = setup_with_host();
        let fetched = mgr.get_host(&host.id).unwrap().unwrap();
        assert_eq!(fetched.hostname, "esxi-01.lab.local");
        assert_eq!(fetched.address, "10.0.0.11");
        assert_eq!(fetched.cluster_id, cluster.id);
        assert_eq!(fetched.cpus, 32);
        assert_eq!(fetched.memory_mb, 131072);
        assert_eq!(fetched.status, HostStatus::Connected);

        // The cluster should now reference the host.
        let cl = mgr.get_cluster(&cluster.id).unwrap().unwrap();
        assert!(cl.hosts.contains(&host.id));
    }

    #[test]
    fn test_list_hosts_with_cluster_filter() {
        let (mgr, dc, cluster) = setup_with_cluster();

        // Create a second cluster and register hosts in each.
        let cluster2 = mgr
            .create_cluster(CreateClusterRequest {
                name: "dev-cluster".into(),
                description: "Dev workloads".into(),
                datacenter_id: dc.id.clone(),
                ha_enabled: false,
                drs_enabled: false,
                drs_mode: DrsMode::Manual,
                evc_mode: None,
            })
            .unwrap();

        mgr.register_host(RegisterHostRequest {
            hostname: "host-prod-1".into(),
            address: "10.0.1.1".into(),
            cluster_id: cluster.id.clone(),
            cpus: 16,
            memory_mb: 65536,
            agent_version: "0.1.0".into(),
            secure_containers_ready: false,
        })
        .unwrap();
        mgr.register_host(RegisterHostRequest {
            hostname: "host-dev-1".into(),
            address: "10.0.2.1".into(),
            cluster_id: cluster2.id.clone(),
            cpus: 8,
            memory_mb: 32768,
            agent_version: "0.1.0".into(),
            secure_containers_ready: false,
        })
        .unwrap();

        assert_eq!(mgr.list_hosts(None).len(), 2);
        assert_eq!(mgr.list_hosts(Some(&cluster.id)).len(), 1);
        assert_eq!(mgr.list_hosts(Some(&cluster2.id)).len(), 1);
        assert_eq!(mgr.get_cluster_hosts(&cluster.id).len(), 1);
    }

    #[test]
    fn test_host_heartbeat_updates_metrics() {
        let (mgr, _dc, _cluster, host) = setup_with_host();

        mgr.update_host_heartbeat(
            &host.id,
            HostHeartbeat {
                cpu_usage_pct: 42.5,
                memory_usage_pct: 71.3,
                vm_count: 12,
                uptime_secs: 86400,
                secure_containers_ready: false,
                secure_containers: None,
            },
        )
        .unwrap();

        let updated = mgr.get_host(&host.id).unwrap().unwrap();
        assert!((updated.cpu_usage_pct - 42.5).abs() < f64::EPSILON);
        assert!((updated.memory_usage_pct - 71.3).abs() < f64::EPSILON);
        assert_eq!(updated.vm_count, 12);
        assert!(updated.last_heartbeat > host.last_heartbeat);
    }

    #[test]
    fn test_host_heartbeat_persists_secure_containers() {
        let (mgr, _dc, _cluster, host) = setup_with_host();

        mgr.update_host_heartbeat(
            &host.id,
            HostHeartbeat {
                cpu_usage_pct: 10.0,
                memory_usage_pct: 20.0,
                vm_count: 1,
                uptime_secs: 100,
                secure_containers_ready: false,
                secure_containers: Some(SecureContainersDetail {
                    available: true,
                    shim_installed: true,
                    guest_image_present: true,
                }),
            },
        )
        .unwrap();

        let updated = mgr.get_host(&host.id).unwrap().unwrap();
        assert!(updated.secure_containers_ready);
        let sc = updated.secure_containers.expect("nested secure_containers");
        assert!(sc.shim_installed);
        assert!(sc.guest_image_present);
    }

    #[test]
    fn test_maintenance_mode_toggle() {
        let (mgr, _dc, _cluster, host) = setup_with_host();
        assert_eq!(host.status, HostStatus::Connected);

        mgr.enter_maintenance_mode(&host.id).unwrap();
        let h = mgr.get_host(&host.id).unwrap().unwrap();
        assert_eq!(h.status, HostStatus::Maintenance);

        mgr.exit_maintenance_mode(&host.id).unwrap();
        let h = mgr.get_host(&host.id).unwrap().unwrap();
        assert_eq!(h.status, HostStatus::Connected);
    }

    #[test]
    fn test_datacenter_summary() {
        let (mgr, dc, _cluster, _host) = setup_with_host();

        // Send a heartbeat so the host has some VM count.
        mgr.update_host_heartbeat(
            &_host.id,
            HostHeartbeat {
                cpu_usage_pct: 10.0,
                memory_usage_pct: 20.0,
                vm_count: 5,
                uptime_secs: 3600,
                secure_containers_ready: false,
                secure_containers: None,
            },
        )
        .unwrap();

        let summary = mgr.get_datacenter_summary(&dc.id).unwrap();
        assert_eq!(summary.name, "dc-east");
        assert_eq!(summary.cluster_count, 1);
        assert_eq!(summary.host_count, 1);
        assert_eq!(summary.vm_count, 5);
        assert_eq!(summary.total_cpus, 32);
        assert_eq!(summary.total_memory_mb, 131072);
    }

    #[test]
    fn test_remove_host_cleans_cluster() {
        let (mgr, _dc, cluster, host) = setup_with_host();

        // Host should be in the cluster's host list.
        let cl = mgr.get_cluster(&cluster.id).unwrap().unwrap();
        assert_eq!(cl.hosts.len(), 1);

        mgr.remove_host(&host.id).unwrap();

        // Host should be gone.
        assert!(mgr.get_host(&host.id).unwrap().is_none());

        // Cluster's host list should now be empty.
        let cl = mgr.get_cluster(&cluster.id).unwrap().unwrap();
        assert!(cl.hosts.is_empty());
    }

    #[test]
    fn test_delete_cluster_cascades_hosts() {
        let (mgr, _dc, cluster, host) = setup_with_host();

        mgr.delete_cluster(&cluster.id).unwrap();

        assert!(mgr.get_cluster(&cluster.id).unwrap().is_none());
        assert!(mgr.get_host(&host.id).unwrap().is_none());

        // The datacenter should no longer reference the cluster.
        let dc = mgr.get_datacenter(&_dc.id).unwrap().unwrap();
        assert!(dc.clusters.is_empty());
    }

    #[test]
    fn test_update_cluster_fields() {
        let (mgr, _dc, cluster) = setup_with_cluster();

        let updated = mgr
            .update_cluster(
                &cluster.id,
                UpdateClusterRequest {
                    name: Some("renamed-cluster".into()),
                    description: None,
                    ha_enabled: Some(false),
                    drs_enabled: None,
                    drs_mode: Some(DrsMode::Manual),
                    evc_mode: Some(Some("intel-skylake".into())),
                    status: Some(ClusterStatus::Degraded),
                },
            )
            .unwrap();

        assert_eq!(updated.name, "renamed-cluster");
        assert!(!updated.ha_enabled);
        assert!(updated.drs_enabled); // unchanged
        assert_eq!(updated.drs_mode, DrsMode::Manual);
        assert_eq!(updated.evc_mode, Some("intel-skylake".into()));
        assert_eq!(updated.status, ClusterStatus::Degraded);
    }

    #[test]
    fn test_nonexistent_entity_errors() {
        let mgr = DatacenterManager::new();

        assert!(mgr.get_datacenter("no-such-id").unwrap().is_none());
        assert!(mgr.get_cluster("no-such-id").unwrap().is_none());
        assert!(mgr.get_host("no-such-id").unwrap().is_none());

        assert!(mgr.delete_datacenter("no-such-id").is_err());
        assert!(mgr.delete_cluster("no-such-id").is_err());
        assert!(mgr.remove_host("no-such-id").is_err());
        assert!(mgr.enter_maintenance_mode("no-such-id").is_err());
        assert!(mgr.exit_maintenance_mode("no-such-id").is_err());
        assert!(mgr
            .update_host_heartbeat(
                "no-such-id",
                HostHeartbeat {
                    cpu_usage_pct: 0.0,
                    memory_usage_pct: 0.0,
                    vm_count: 0,
                    uptime_secs: 0,
                    secure_containers_ready: false,
                    secure_containers: None,
                }
            )
            .is_err());
    }
}
