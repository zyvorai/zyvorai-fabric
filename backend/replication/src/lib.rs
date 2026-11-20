// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: LicenseRef-Zyvor-Production-1.0

use anyhow::{anyhow, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::{Arc, RwLock};

// ---------------------------------------------------------------------------
// Enums
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReplicationStatus {
    Active,
    Paused,
    Error,
    Initial,
    Seeding,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SiteType {
    Primary,
    Recovery,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SiteStatus {
    Connected,
    Disconnected,
    Error,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConsistencyType {
    CrashConsistent,
    AppConsistent,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InstanceStatus {
    Available,
    Expired,
    Corrupted,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SyncEventType {
    SyncStarted,
    SyncCompleted,
    SyncFailed,
    RpoViolation,
    Paused,
    Resumed,
}

// ---------------------------------------------------------------------------
// Data models
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReplicationConfig {
    pub id: String,
    pub vm_name: String,
    pub source_site_id: String,
    pub target_site_id: String,
    pub rpo_minutes: u32,
    pub bandwidth_limit_mbps: Option<u32>,
    pub compression_enabled: bool,
    pub encryption_enabled: bool,
    pub quiesce_guest: bool,
    pub status: ReplicationStatus,
    pub last_sync: Option<DateTime<Utc>>,
    pub next_sync: Option<DateTime<Utc>>,
    pub created: DateTime<Utc>,
    pub updated: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReplicationSite {
    #[serde(default)]
    pub id: String,
    pub name: String,
    /// `endpoint` on the wire — matches the frontend's field name; only the
    /// Rust identifier says "address".
    #[serde(rename = "endpoint")]
    pub address: String,
    pub site_type: SiteType,
    #[serde(default = "default_site_status")]
    pub status: SiteStatus,
    #[serde(default)]
    pub datacenter_id: Option<String>,
    /// Always overwritten by `register_site`'s handler on create — defaults
    /// exist only so a client registering a new site doesn't need to
    /// fabricate values for fields the server owns.
    #[serde(default = "Utc::now")]
    pub created: DateTime<Utc>,
    #[serde(default = "Utc::now")]
    pub updated: DateTime<Utc>,
}

fn default_site_status() -> SiteStatus {
    SiteStatus::Connected
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReplicationInstance {
    pub id: String,
    pub replication_id: String,
    pub vm_name: String,
    pub snapshot_time: DateTime<Utc>,
    pub size_bytes: u64,
    pub consistency_type: ConsistencyType,
    pub status: InstanceStatus,
    pub expires: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReplicationMetrics {
    pub replication_id: String,
    pub vm_name: String,
    pub rpo_actual_minutes: u32,
    pub rpo_violation: bool,
    pub bytes_transferred_last_sync: u64,
    pub sync_duration_secs: u64,
    pub total_bytes_transferred: u64,
    pub sync_count: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncEvent {
    pub id: String,
    pub replication_id: String,
    pub event_type: SyncEventType,
    pub details: Option<String>,
    pub timestamp: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReplicationHealthSummary {
    #[serde(rename = "total_replications")]
    pub total: u32,
    pub active: u32,
    pub paused: u32,
    pub error: u32,
    pub rpo_violations: u32,
    pub avg_rpo_minutes: f64,
    pub total_bytes_transferred_24h: u64,
    pub sites: Vec<SiteHealth>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SiteHealth {
    pub site_id: String,
    pub site_name: String,
    pub replication_count: u32,
    pub health: String,
}

// ---------------------------------------------------------------------------
// ReplicationManager
// ---------------------------------------------------------------------------

#[derive(Clone)]
pub struct ReplicationManager {
    sites: Arc<RwLock<HashMap<String, ReplicationSite>>>,
    replications: Arc<RwLock<HashMap<String, ReplicationConfig>>>,
    instances: Arc<RwLock<HashMap<String, ReplicationInstance>>>,
    metrics: Arc<RwLock<HashMap<String, ReplicationMetrics>>>,
    events: Arc<RwLock<Vec<SyncEvent>>>,
}

impl ReplicationManager {
    pub fn new() -> Self {
        Self {
            sites: Arc::new(RwLock::new(HashMap::new())),
            replications: Arc::new(RwLock::new(HashMap::new())),
            instances: Arc::new(RwLock::new(HashMap::new())),
            metrics: Arc::new(RwLock::new(HashMap::new())),
            events: Arc::new(RwLock::new(Vec::new())),
        }
    }

    // -----------------------------------------------------------------------
    // Sites
    // -----------------------------------------------------------------------

    /// Register a new replication site.
    pub fn register_site(&self, site: ReplicationSite) -> Result<ReplicationSite> {
        let mut sites = self
            .sites
            .write()
            .map_err(|e| anyhow!("lock poisoned: {}", e))?;
        if sites.contains_key(&site.id) {
            return Err(anyhow!("site already exists: {}", site.id));
        }
        sites.insert(site.id.clone(), site.clone());
        tracing::info!("Registered replication site: {} ({})", site.name, site.id);
        Ok(site)
    }

    /// Get a site by ID.
    pub fn get_site(&self, id: &str) -> Option<ReplicationSite> {
        let sites = self.sites.read().ok()?;
        sites.get(id).cloned()
    }

    /// List all registered sites.
    pub fn list_sites(&self) -> Vec<ReplicationSite> {
        let sites = self.sites.read().unwrap_or_else(|e| e.into_inner());
        sites.values().cloned().collect()
    }

    /// Update the connection status of a site.
    pub fn update_site_status(&self, id: &str, status: SiteStatus) -> Result<()> {
        let mut sites = self
            .sites
            .write()
            .map_err(|e| anyhow!("lock poisoned: {}", e))?;
        let site = sites
            .get_mut(id)
            .ok_or_else(|| anyhow!("site not found: {}", id))?;
        site.status = status;
        site.updated = Utc::now();
        tracing::info!("Updated site {} status to {:?}", id, site.status);
        Ok(())
    }

    /// Remove a site by ID.
    pub fn remove_site(&self, id: &str) -> Result<()> {
        let mut sites = self
            .sites
            .write()
            .map_err(|e| anyhow!("lock poisoned: {}", e))?;
        sites
            .remove(id)
            .ok_or_else(|| anyhow!("site not found: {}", id))?;
        tracing::info!("Removed replication site: {}", id);
        Ok(())
    }

    // -----------------------------------------------------------------------
    // Replication configuration
    // -----------------------------------------------------------------------

    /// Configure (create) a new replication for a VM.
    pub fn configure_replication(&self, config: ReplicationConfig) -> Result<ReplicationConfig> {
        let mut replications = self
            .replications
            .write()
            .map_err(|e| anyhow!("lock poisoned: {}", e))?;
        if replications.contains_key(&config.id) {
            return Err(anyhow!("replication already exists: {}", config.id));
        }
        replications.insert(config.id.clone(), config.clone());
        tracing::info!(
            "Configured replication {} for VM {}",
            config.id,
            config.vm_name
        );
        Ok(config)
    }

    /// Get a replication config by ID.
    pub fn get_replication(&self, id: &str) -> Option<ReplicationConfig> {
        let replications = self.replications.read().ok()?;
        replications.get(id).cloned()
    }

    /// Find the replication config for a given VM name.
    pub fn get_replication_by_vm(&self, vm_name: &str) -> Option<ReplicationConfig> {
        let replications = self.replications.read().ok()?;
        replications
            .values()
            .find(|r| r.vm_name == vm_name)
            .cloned()
    }

    /// List replications, optionally filtered by source or target site ID.
    pub fn list_replications(&self, site_id: Option<&str>) -> Vec<ReplicationConfig> {
        let replications = self.replications.read().unwrap_or_else(|e| e.into_inner());
        replications
            .values()
            .filter(|r| match site_id {
                Some(sid) => r.source_site_id == sid || r.target_site_id == sid,
                None => true,
            })
            .cloned()
            .collect()
    }

    /// Pause an active replication.
    pub fn pause_replication(&self, id: &str) -> Result<()> {
        let mut replications = self
            .replications
            .write()
            .map_err(|e| anyhow!("lock poisoned: {}", e))?;
        let repl = replications
            .get_mut(id)
            .ok_or_else(|| anyhow!("replication not found: {}", id))?;
        if repl.status != ReplicationStatus::Active {
            return Err(anyhow!(
                "cannot pause replication in {:?} state",
                repl.status
            ));
        }
        repl.status = ReplicationStatus::Paused;
        repl.updated = Utc::now();
        tracing::info!("Paused replication {}", id);

        // Record event outside the lock scope
        drop(replications);
        self.record_event(id, SyncEventType::Paused, None)?;
        Ok(())
    }

    /// Resume a paused replication.
    pub fn resume_replication(&self, id: &str) -> Result<()> {
        let mut replications = self
            .replications
            .write()
            .map_err(|e| anyhow!("lock poisoned: {}", e))?;
        let repl = replications
            .get_mut(id)
            .ok_or_else(|| anyhow!("replication not found: {}", id))?;
        if repl.status != ReplicationStatus::Paused {
            return Err(anyhow!(
                "cannot resume replication in {:?} state",
                repl.status
            ));
        }
        repl.status = ReplicationStatus::Active;
        repl.updated = Utc::now();
        tracing::info!("Resumed replication {}", id);

        drop(replications);
        self.record_event(id, SyncEventType::Resumed, None)?;
        Ok(())
    }

    /// Remove a replication configuration entirely.
    pub fn remove_replication(&self, id: &str) -> Result<()> {
        let mut replications = self
            .replications
            .write()
            .map_err(|e| anyhow!("lock poisoned: {}", e))?;
        replications
            .remove(id)
            .ok_or_else(|| anyhow!("replication not found: {}", id))?;
        tracing::info!("Removed replication {}", id);
        Ok(())
    }

    /// Update the status of a replication.
    pub fn update_replication_status(&self, id: &str, status: ReplicationStatus) -> Result<()> {
        let mut replications = self
            .replications
            .write()
            .map_err(|e| anyhow!("lock poisoned: {}", e))?;
        let repl = replications
            .get_mut(id)
            .ok_or_else(|| anyhow!("replication not found: {}", id))?;
        repl.status = status;
        repl.updated = Utc::now();
        Ok(())
    }

    // -----------------------------------------------------------------------
    // Sync lifecycle
    // -----------------------------------------------------------------------

    /// Begin a synchronization cycle for a replication.
    ///
    /// This validates the replication is active, records the sync-started event,
    /// and then attempts to perform the actual data synchronization using rsync.
    /// If the VM image directory exists locally, rsync is invoked to transfer
    /// changed blocks to the target site. If rsync is unavailable or the source
    /// path does not exist, the sync event is still recorded (metadata-only sync)
    /// so that callers can later complete or fail the sync via the normal lifecycle.
    pub fn start_sync(&self, replication_id: &str) -> Result<SyncEvent> {
        // Validate that the replication exists and is active, and extract info
        // we need for the actual sync operation.
        let (vm_name, target_address, compression, bandwidth_limit) = {
            let replications = self
                .replications
                .read()
                .map_err(|e| anyhow!("lock poisoned: {}", e))?;
            let repl = replications
                .get(replication_id)
                .ok_or_else(|| anyhow!("replication not found: {}", replication_id))?;
            if repl.status != ReplicationStatus::Active {
                return Err(anyhow!(
                    "cannot sync replication in {:?} state",
                    repl.status
                ));
            }
            (
                repl.vm_name.clone(),
                repl.target_site_id.clone(),
                repl.compression_enabled,
                repl.bandwidth_limit_mbps,
            )
        };

        // Look up the target site address for rsync destination.
        let target_host = {
            let sites = self
                .sites
                .read()
                .map_err(|e| anyhow!("lock poisoned: {}", e))?;
            sites
                .get(&target_address)
                .map(|s| s.address.clone())
                .unwrap_or_else(|| target_address.clone())
        };

        let event = SyncEvent {
            id: uuid::Uuid::new_v4().to_string(),
            replication_id: replication_id.to_string(),
            event_type: SyncEventType::SyncStarted,
            details: None,
            timestamp: Utc::now(),
        };

        {
            let mut events = self
                .events
                .write()
                .map_err(|e| anyhow!("lock poisoned: {}", e))?;
            events.push(event.clone());
        }

        tracing::info!("Sync started for replication {}", replication_id);

        // Attempt real data synchronization using rsync.
        let source_path = format!("/var/lib/zyvor-fabricd/images/{}", vm_name);
        if std::path::Path::new(&source_path).exists() {
            let mut rsync_args = vec!["-a".to_string(), "--partial".to_string()];

            if compression {
                rsync_args.push("-z".to_string());
            }
            if let Some(bw) = bandwidth_limit {
                // rsync --bwlimit is in KBps, we have MBps
                rsync_args.push(format!("--bwlimit={}", bw * 1024));
            }

            // Ensure trailing slash so rsync syncs directory contents
            let source_with_slash = if source_path.ends_with('/') {
                source_path.clone()
            } else {
                format!("{}/", source_path)
            };
            rsync_args.push(source_with_slash);
            rsync_args.push(format!(
                "{}:/var/lib/zyvor-fabricd/images/{}/",
                target_host, vm_name
            ));

            let output = std::process::Command::new("rsync")
                .args(&rsync_args)
                .output();

            match output {
                Ok(out) if out.status.success() => {
                    tracing::info!(
                        "rsync completed successfully for replication {} (VM '{}')",
                        replication_id,
                        vm_name
                    );
                }
                Ok(out) => {
                    let stderr = String::from_utf8_lossy(&out.stderr);
                    tracing::warn!(
                        "rsync returned non-zero for replication {} (VM '{}'): {}",
                        replication_id,
                        vm_name,
                        stderr
                    );
                    // Record the data transfer size even on partial failure
                }
                Err(e) => {
                    tracing::warn!(
                        "Failed to execute rsync for replication {} (VM '{}'): {} \
                         (rsync may not be installed; metadata sync recorded)",
                        replication_id,
                        vm_name,
                        e
                    );
                }
            }

            // Record the source data size for metrics regardless of rsync outcome.
            let source_size: u64 = std::fs::read_dir(&source_path)
                .map(|entries| {
                    entries
                        .filter_map(|e| e.ok())
                        .filter_map(|e| e.metadata().ok())
                        .map(|m| m.len())
                        .sum()
                })
                .unwrap_or(0);

            tracing::info!(
                "Replication {} source data size: {} bytes",
                replication_id,
                source_size
            );
        } else {
            tracing::debug!(
                "Source path '{}' does not exist for VM '{}', recording metadata-only sync",
                source_path,
                vm_name
            );
        }

        Ok(event)
    }

    /// Mark a synchronization as successfully completed and create a recovery
    /// instance (point-in-time snapshot).
    pub fn complete_sync(
        &self,
        replication_id: &str,
        bytes_transferred: u64,
    ) -> Result<ReplicationInstance> {
        let now = Utc::now();

        // Update the replication timestamps
        let vm_name = {
            let mut replications = self
                .replications
                .write()
                .map_err(|e| anyhow!("lock poisoned: {}", e))?;
            let repl = replications
                .get_mut(replication_id)
                .ok_or_else(|| anyhow!("replication not found: {}", replication_id))?;
            repl.last_sync = Some(now);
            repl.next_sync = Some(now + chrono::Duration::minutes(repl.rpo_minutes as i64));
            repl.updated = now;
            repl.vm_name.clone()
        };

        // Determine consistency type based on quiesce_guest setting
        let consistency = {
            let replications = self
                .replications
                .read()
                .map_err(|e| anyhow!("lock poisoned: {}", e))?;
            let repl = replications
                .get(replication_id)
                .ok_or_else(|| anyhow!("Replication '{}' not found", replication_id))?;
            if repl.quiesce_guest {
                ConsistencyType::AppConsistent
            } else {
                ConsistencyType::CrashConsistent
            }
        };

        // Create recovery instance
        let instance = ReplicationInstance {
            id: uuid::Uuid::new_v4().to_string(),
            replication_id: replication_id.to_string(),
            vm_name: vm_name.clone(),
            snapshot_time: now,
            size_bytes: bytes_transferred,
            consistency_type: consistency,
            status: InstanceStatus::Available,
            expires: Some(now + chrono::Duration::days(7)),
        };

        {
            let mut instances = self
                .instances
                .write()
                .map_err(|e| anyhow!("lock poisoned: {}", e))?;
            instances.insert(instance.id.clone(), instance.clone());
        }

        // Update metrics
        {
            let mut metrics_map = self
                .metrics
                .write()
                .map_err(|e| anyhow!("lock poisoned: {}", e))?;
            let m = metrics_map
                .entry(replication_id.to_string())
                .or_insert_with(|| ReplicationMetrics {
                    replication_id: replication_id.to_string(),
                    vm_name: vm_name.clone(),
                    rpo_actual_minutes: 0,
                    rpo_violation: false,
                    bytes_transferred_last_sync: 0,
                    sync_duration_secs: 0,
                    total_bytes_transferred: 0,
                    sync_count: 0,
                });
            m.bytes_transferred_last_sync = bytes_transferred;
            m.total_bytes_transferred += bytes_transferred;
            m.sync_count += 1;
        }

        // Record completion event
        self.record_event(
            replication_id,
            SyncEventType::SyncCompleted,
            Some(format!("{} bytes transferred", bytes_transferred)),
        )?;

        tracing::info!(
            "Sync completed for replication {} ({} bytes)",
            replication_id,
            bytes_transferred
        );
        Ok(instance)
    }

    /// Record a failed synchronization.
    pub fn fail_sync(&self, replication_id: &str, error: &str) -> Result<()> {
        {
            let mut replications = self
                .replications
                .write()
                .map_err(|e| anyhow!("lock poisoned: {}", e))?;
            let repl = replications
                .get_mut(replication_id)
                .ok_or_else(|| anyhow!("replication not found: {}", replication_id))?;
            repl.status = ReplicationStatus::Error;
            repl.updated = Utc::now();
        }

        self.record_event(
            replication_id,
            SyncEventType::SyncFailed,
            Some(error.to_string()),
        )?;

        tracing::warn!("Sync failed for replication {}: {}", replication_id, error);
        Ok(())
    }

    /// Return all sync events for a given replication, ordered chronologically.
    pub fn get_sync_events(&self, replication_id: &str) -> Vec<SyncEvent> {
        let events = self.events.read().unwrap_or_else(|e| e.into_inner());
        let mut matched: Vec<SyncEvent> = events
            .iter()
            .filter(|e| e.replication_id == replication_id)
            .cloned()
            .collect();
        matched.sort_by_key(|e| e.timestamp);
        matched
    }

    // -----------------------------------------------------------------------
    // Recovery instances
    // -----------------------------------------------------------------------

    /// List all recovery instances for a replication.
    pub fn list_instances(&self, replication_id: &str) -> Vec<ReplicationInstance> {
        let instances = self.instances.read().unwrap_or_else(|e| e.into_inner());
        let mut matched: Vec<ReplicationInstance> = instances
            .values()
            .filter(|i| i.replication_id == replication_id)
            .cloned()
            .collect();
        matched.sort_by_key(|i| i.snapshot_time);
        matched
    }

    /// Get a single recovery instance by its ID.
    pub fn get_instance(&self, id: &str) -> Option<ReplicationInstance> {
        let instances = self.instances.read().ok()?;
        instances.get(id).cloned()
    }

    /// Expire old recovery instances for a replication, keeping the most recent
    /// `keep_count` instances. Returns the number of instances expired.
    pub fn expire_old_instances(&self, replication_id: &str, keep_count: usize) -> Result<u32> {
        let mut instances = self
            .instances
            .write()
            .map_err(|e| anyhow!("lock poisoned: {}", e))?;

        // Collect instances for this replication, sorted newest first
        let mut matching: Vec<(String, DateTime<Utc>)> = instances
            .iter()
            .filter(|(_, i)| i.replication_id == replication_id)
            .map(|(id, i)| (id.clone(), i.snapshot_time))
            .collect();
        matching.sort_by_key(|a| std::cmp::Reverse(a.1));

        let mut expired_count: u32 = 0;
        for (idx, (id, _)) in matching.iter().enumerate() {
            if idx >= keep_count {
                if let Some(inst) = instances.get_mut(id) {
                    inst.status = InstanceStatus::Expired;
                    expired_count += 1;
                }
            }
        }

        tracing::info!(
            "Expired {} instances for replication {} (kept {})",
            expired_count,
            replication_id,
            keep_count
        );
        Ok(expired_count)
    }

    // -----------------------------------------------------------------------
    // Metrics & monitoring
    // -----------------------------------------------------------------------

    /// Get replication metrics for a given replication.
    pub fn get_metrics(&self, replication_id: &str) -> Option<ReplicationMetrics> {
        let metrics = self.metrics.read().ok()?;
        metrics.get(replication_id).cloned()
    }

    /// Update (replace) metrics for a replication.
    pub fn update_metrics(&self, replication_id: &str, metrics: ReplicationMetrics) -> Result<()> {
        let mut metrics_map = self
            .metrics
            .write()
            .map_err(|e| anyhow!("lock poisoned: {}", e))?;
        metrics_map.insert(replication_id.to_string(), metrics);
        Ok(())
    }

    /// Return all replication configs whose actual RPO exceeds the configured RPO.
    pub fn check_rpo_violations(&self) -> Vec<ReplicationConfig> {
        let replications = self.replications.read().unwrap_or_else(|e| e.into_inner());
        let now = Utc::now();

        replications
            .values()
            .filter(|r| {
                if r.status != ReplicationStatus::Active {
                    return false;
                }
                match r.last_sync {
                    Some(last) => {
                        let elapsed = (now - last).num_minutes() as u32;
                        elapsed > r.rpo_minutes
                    }
                    // Never synced means RPO is violated if not in initial/seeding
                    None => true,
                }
            })
            .cloned()
            .collect()
    }

    /// Produce an aggregate health summary across all replications.
    pub fn get_replication_health_summary(&self) -> ReplicationHealthSummary {
        let replications = self.replications.read().unwrap_or_else(|e| e.into_inner());
        let now = Utc::now();

        let mut summary = ReplicationHealthSummary {
            total: replications.len() as u32,
            active: 0,
            paused: 0,
            error: 0,
            rpo_violations: 0,
            avg_rpo_minutes: 0.0,
            total_bytes_transferred_24h: 0,
            sites: Vec::new(),
        };

        let mut active_rpo_sum = 0u64;
        for r in replications.values() {
            match r.status {
                ReplicationStatus::Active => {
                    summary.active += 1;
                    active_rpo_sum += r.rpo_minutes as u64;
                    // Check for RPO violation
                    let violated = match r.last_sync {
                        Some(last) => (now - last).num_minutes() as u32 > r.rpo_minutes,
                        None => true,
                    };
                    if violated {
                        summary.rpo_violations += 1;
                    }
                }
                ReplicationStatus::Paused => summary.paused += 1,
                ReplicationStatus::Error => summary.error += 1,
                _ => {}
            }
        }
        if summary.active > 0 {
            summary.avg_rpo_minutes = active_rpo_sum as f64 / summary.active as f64;
        }

        summary
    }

    // -----------------------------------------------------------------------
    // Internal helpers
    // -----------------------------------------------------------------------

    fn record_event(
        &self,
        replication_id: &str,
        event_type: SyncEventType,
        details: Option<String>,
    ) -> Result<()> {
        let event = SyncEvent {
            id: uuid::Uuid::new_v4().to_string(),
            replication_id: replication_id.to_string(),
            event_type,
            details,
            timestamp: Utc::now(),
        };
        let mut events = self
            .events
            .write()
            .map_err(|e| anyhow!("lock poisoned: {}", e))?;
        events.push(event);
        Ok(())
    }
}

impl Default for ReplicationManager {
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

    fn make_site(id: &str, name: &str, site_type: SiteType) -> ReplicationSite {
        let now = Utc::now();
        ReplicationSite {
            id: id.to_string(),
            name: name.to_string(),
            address: format!("https://{}.example.com", id),
            site_type,
            status: SiteStatus::Connected,
            datacenter_id: None,
            created: now,
            updated: now,
        }
    }

    fn make_replication(
        id: &str,
        vm_name: &str,
        source: &str,
        target: &str,
        rpo: u32,
    ) -> ReplicationConfig {
        let now = Utc::now();
        ReplicationConfig {
            id: id.to_string(),
            vm_name: vm_name.to_string(),
            source_site_id: source.to_string(),
            target_site_id: target.to_string(),
            rpo_minutes: rpo,
            bandwidth_limit_mbps: None,
            compression_enabled: true,
            encryption_enabled: true,
            quiesce_guest: false,
            status: ReplicationStatus::Active,
            last_sync: None,
            next_sync: None,
            created: now,
            updated: now,
        }
    }

    #[test]
    fn test_register_and_get_site() {
        let mgr = ReplicationManager::new();
        let site = make_site("site-1", "Primary DC", SiteType::Primary);
        let registered = mgr.register_site(site.clone()).unwrap();
        assert_eq!(registered.id, "site-1");

        let fetched = mgr.get_site("site-1").unwrap();
        assert_eq!(fetched.name, "Primary DC");
        assert_eq!(fetched.site_type, SiteType::Primary);
    }

    #[test]
    fn test_register_duplicate_site_fails() {
        let mgr = ReplicationManager::new();
        let site = make_site("site-1", "Primary DC", SiteType::Primary);
        mgr.register_site(site.clone()).unwrap();
        let result = mgr.register_site(site);
        assert!(result.is_err());
    }

    #[test]
    fn test_list_and_remove_sites() {
        let mgr = ReplicationManager::new();
        mgr.register_site(make_site("s1", "Site A", SiteType::Primary))
            .unwrap();
        mgr.register_site(make_site("s2", "Site B", SiteType::Recovery))
            .unwrap();

        assert_eq!(mgr.list_sites().len(), 2);

        mgr.remove_site("s1").unwrap();
        assert_eq!(mgr.list_sites().len(), 1);
        assert!(mgr.get_site("s1").is_none());
    }

    #[test]
    fn test_update_site_status() {
        let mgr = ReplicationManager::new();
        mgr.register_site(make_site("s1", "Site A", SiteType::Primary))
            .unwrap();

        mgr.update_site_status("s1", SiteStatus::Disconnected)
            .unwrap();
        let site = mgr.get_site("s1").unwrap();
        assert_eq!(site.status, SiteStatus::Disconnected);
    }

    #[test]
    fn test_configure_and_get_replication() {
        let mgr = ReplicationManager::new();
        let config = make_replication("r1", "web-server", "s1", "s2", 15);
        let created = mgr.configure_replication(config).unwrap();
        assert_eq!(created.vm_name, "web-server");

        let fetched = mgr.get_replication("r1").unwrap();
        assert_eq!(fetched.rpo_minutes, 15);

        let by_vm = mgr.get_replication_by_vm("web-server").unwrap();
        assert_eq!(by_vm.id, "r1");
    }

    #[test]
    fn test_list_replications_with_site_filter() {
        let mgr = ReplicationManager::new();
        mgr.configure_replication(make_replication("r1", "vm1", "s1", "s2", 15))
            .unwrap();
        mgr.configure_replication(make_replication("r2", "vm2", "s1", "s3", 60))
            .unwrap();
        mgr.configure_replication(make_replication("r3", "vm3", "s2", "s3", 5))
            .unwrap();

        let all = mgr.list_replications(None);
        assert_eq!(all.len(), 3);

        let s1_only = mgr.list_replications(Some("s1"));
        assert_eq!(s1_only.len(), 2);

        let s3_only = mgr.list_replications(Some("s3"));
        assert_eq!(s3_only.len(), 2);
    }

    #[test]
    fn test_pause_and_resume_replication() {
        let mgr = ReplicationManager::new();
        mgr.configure_replication(make_replication("r1", "vm1", "s1", "s2", 15))
            .unwrap();

        mgr.pause_replication("r1").unwrap();
        assert_eq!(
            mgr.get_replication("r1").unwrap().status,
            ReplicationStatus::Paused
        );

        // Cannot pause again
        assert!(mgr.pause_replication("r1").is_err());

        mgr.resume_replication("r1").unwrap();
        assert_eq!(
            mgr.get_replication("r1").unwrap().status,
            ReplicationStatus::Active
        );

        // Cannot resume when active
        assert!(mgr.resume_replication("r1").is_err());
    }

    #[test]
    fn test_sync_lifecycle() {
        let mgr = ReplicationManager::new();
        mgr.configure_replication(make_replication("r1", "vm1", "s1", "s2", 15))
            .unwrap();

        // Start sync
        let event = mgr.start_sync("r1").unwrap();
        assert_eq!(event.event_type, SyncEventType::SyncStarted);

        // Complete sync
        let instance = mgr.complete_sync("r1", 1024 * 1024).unwrap();
        assert_eq!(instance.replication_id, "r1");
        assert_eq!(instance.vm_name, "vm1");
        assert_eq!(instance.size_bytes, 1024 * 1024);
        assert_eq!(instance.status, InstanceStatus::Available);
        assert_eq!(instance.consistency_type, ConsistencyType::CrashConsistent);

        // Verify last_sync and next_sync are set
        let repl = mgr.get_replication("r1").unwrap();
        assert!(repl.last_sync.is_some());
        assert!(repl.next_sync.is_some());

        // Verify events
        let events = mgr.get_sync_events("r1");
        assert_eq!(events.len(), 2); // SyncStarted + SyncCompleted
    }

    #[test]
    fn test_fail_sync_sets_error_status() {
        let mgr = ReplicationManager::new();
        mgr.configure_replication(make_replication("r1", "vm1", "s1", "s2", 15))
            .unwrap();

        mgr.fail_sync("r1", "network timeout").unwrap();

        let repl = mgr.get_replication("r1").unwrap();
        assert_eq!(repl.status, ReplicationStatus::Error);

        let events = mgr.get_sync_events("r1");
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].event_type, SyncEventType::SyncFailed);
        assert_eq!(events[0].details.as_deref(), Some("network timeout"));
    }

    #[test]
    fn test_recovery_instances_and_expiry() {
        let mgr = ReplicationManager::new();
        mgr.configure_replication(make_replication("r1", "vm1", "s1", "s2", 15))
            .unwrap();

        // Create multiple recovery instances via complete_sync
        mgr.complete_sync("r1", 100).unwrap();
        mgr.complete_sync("r1", 200).unwrap();
        mgr.complete_sync("r1", 300).unwrap();
        mgr.complete_sync("r1", 400).unwrap();
        mgr.complete_sync("r1", 500).unwrap();

        let instances = mgr.list_instances("r1");
        assert_eq!(instances.len(), 5);

        // Keep only the 2 most recent, expire the rest
        let expired = mgr.expire_old_instances("r1", 2).unwrap();
        assert_eq!(expired, 3);

        // Verify statuses
        let instances = mgr.list_instances("r1");
        let available_count = instances
            .iter()
            .filter(|i| i.status == InstanceStatus::Available)
            .count();
        let expired_count = instances
            .iter()
            .filter(|i| i.status == InstanceStatus::Expired)
            .count();
        assert_eq!(available_count, 2);
        assert_eq!(expired_count, 3);
    }

    #[test]
    fn test_rpo_violation_detection() {
        let mgr = ReplicationManager::new();
        // Active replication with no last_sync -> RPO violation
        mgr.configure_replication(make_replication("r1", "vm1", "s1", "s2", 15))
            .unwrap();

        // Active replication with recent sync -> no violation
        let mut recent = make_replication("r2", "vm2", "s1", "s2", 60);
        recent.last_sync = Some(Utc::now());
        mgr.configure_replication(recent).unwrap();

        // Paused replication -> not checked
        let mut paused = make_replication("r3", "vm3", "s1", "s2", 5);
        paused.status = ReplicationStatus::Paused;
        mgr.configure_replication(paused).unwrap();

        let violations = mgr.check_rpo_violations();
        assert_eq!(violations.len(), 1);
        assert_eq!(violations[0].id, "r1");
    }

    #[test]
    fn test_metrics_tracking() {
        let mgr = ReplicationManager::new();
        mgr.configure_replication(make_replication("r1", "vm1", "s1", "s2", 15))
            .unwrap();

        // Metrics are created on first complete_sync
        mgr.complete_sync("r1", 1000).unwrap();
        let m = mgr.get_metrics("r1").unwrap();
        assert_eq!(m.sync_count, 1);
        assert_eq!(m.bytes_transferred_last_sync, 1000);
        assert_eq!(m.total_bytes_transferred, 1000);

        // Second sync accumulates
        mgr.complete_sync("r1", 2000).unwrap();
        let m = mgr.get_metrics("r1").unwrap();
        assert_eq!(m.sync_count, 2);
        assert_eq!(m.bytes_transferred_last_sync, 2000);
        assert_eq!(m.total_bytes_transferred, 3000);

        // Manual metrics update
        let custom = ReplicationMetrics {
            replication_id: "r1".to_string(),
            vm_name: "vm1".to_string(),
            rpo_actual_minutes: 20,
            rpo_violation: true,
            bytes_transferred_last_sync: 0,
            sync_duration_secs: 45,
            total_bytes_transferred: 5000,
            sync_count: 10,
        };
        mgr.update_metrics("r1", custom).unwrap();
        let m = mgr.get_metrics("r1").unwrap();
        assert_eq!(m.rpo_actual_minutes, 20);
        assert!(m.rpo_violation);
    }

    #[test]
    fn test_health_summary() {
        let mgr = ReplicationManager::new();

        // Active with no sync (RPO violation)
        mgr.configure_replication(make_replication("r1", "vm1", "s1", "s2", 15))
            .unwrap();

        // Active with recent sync (no violation)
        let mut recent = make_replication("r2", "vm2", "s1", "s2", 60);
        recent.last_sync = Some(Utc::now());
        mgr.configure_replication(recent).unwrap();

        // Paused
        let mut paused = make_replication("r3", "vm3", "s1", "s2", 5);
        paused.status = ReplicationStatus::Paused;
        mgr.configure_replication(paused).unwrap();

        // Error
        let mut errored = make_replication("r4", "vm4", "s1", "s2", 30);
        errored.status = ReplicationStatus::Error;
        mgr.configure_replication(errored).unwrap();

        let summary = mgr.get_replication_health_summary();
        assert_eq!(summary.total, 4);
        assert_eq!(summary.active, 2);
        assert_eq!(summary.paused, 1);
        assert_eq!(summary.error, 1);
        assert_eq!(summary.rpo_violations, 1);
    }
}
