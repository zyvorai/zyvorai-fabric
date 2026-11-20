// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: LicenseRef-Zyvor-Production-1.0

use anyhow::{bail, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use uuid::Uuid;

// ---------------------------------------------------------------------------
// Enums
// ---------------------------------------------------------------------------

/// The type of baseline defining which category of updates it tracks.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum BaselineType {
    Patch,
    Upgrade,
    Extension,
}

/// Severity classification for patches.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum PatchSeverity {
    Critical,
    Important,
    Moderate,
    Low,
}

/// Status of a remediation task progressing through its lifecycle.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum RemediationStatus {
    Pending,
    Evacuating,
    Updating,
    Rebooting,
    Restoring,
    Completed,
    Failed,
}

/// Individual actions performed during remediation.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum RemediationAction {
    EvacuateVMs,
    ApplyPatches,
    RebootHost,
    ValidateHost,
    RestoreVMs,
}

/// Status of an individual remediation step.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum StepStatus {
    Pending,
    Running,
    Completed,
    Failed,
}

/// Status of a rolling update plan across a cluster.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum RollingUpdateStatus {
    Planned,
    InProgress,
    Paused,
    Completed,
    Failed,
}

// ---------------------------------------------------------------------------
// Data models
// ---------------------------------------------------------------------------

/// A baseline defines a set of package requirements that hosts must satisfy
/// to be considered compliant.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Baseline {
    /// Unique identifier (UUID v4).
    pub id: String,
    /// Human-readable name.
    pub name: String,
    /// Optional description of this baseline's purpose.
    pub description: Option<String>,
    /// The category of updates this baseline tracks.
    pub baseline_type: BaselineType,
    /// Package requirements that hosts must satisfy.
    pub packages: Vec<PackageRequirement>,
    pub created: DateTime<Utc>,
    pub updated: Option<DateTime<Utc>>,
}

/// A single package requirement within a baseline.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PackageRequirement {
    /// Package name (e.g. "kernel", "openssl").
    pub name: String,
    /// Minimum acceptable version (inclusive).
    pub min_version: Option<String>,
    /// Exact version required; takes precedence over min_version.
    pub exact_version: Option<String>,
    /// Severity of the patch this requirement represents.
    pub severity: Option<PatchSeverity>,
}

/// A logical grouping of baselines that can be applied together.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BaselineGroup {
    /// Unique identifier (UUID v4).
    pub id: String,
    /// Human-readable name.
    pub name: String,
    /// Baseline IDs belonging to this group.
    pub baseline_ids: Vec<String>,
    pub created: DateTime<Utc>,
    pub updated: Option<DateTime<Utc>>,
}

/// Compliance status of a single host against a baseline.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HostComplianceStatus {
    pub host_id: String,
    pub hostname: String,
    pub baseline_id: String,
    pub baseline_name: String,
    /// Whether the host satisfies all package requirements.
    pub compliant: bool,
    /// Patches required by the baseline but not installed on the host.
    pub missing_patches: Vec<MissingPatch>,
    /// Patches from the baseline that are already installed.
    pub installed_patches: Vec<InstalledPatch>,
    /// When this compliance scan was performed.
    pub scanned_at: DateTime<Utc>,
}

/// A patch required by the baseline but missing from the host.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MissingPatch {
    pub package_name: String,
    pub required_version: String,
    pub current_version: Option<String>,
    pub severity: Option<PatchSeverity>,
}

/// A patch that is installed on the host.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InstalledPatch {
    pub package_name: String,
    pub version: String,
    pub installed_at: Option<DateTime<Utc>>,
}

/// A task that remediates a non-compliant host by applying missing patches.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RemediationTask {
    /// Unique identifier (UUID v4).
    pub id: String,
    pub host_id: String,
    pub hostname: String,
    pub baseline_id: String,
    /// Current overall status of the remediation.
    pub status: RemediationStatus,
    /// Ordered steps that make up this remediation.
    pub steps: Vec<RemediationStep>,
    pub started: Option<DateTime<Utc>>,
    pub completed: Option<DateTime<Utc>>,
    pub error: Option<String>,
}

/// An individual step within a remediation task.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RemediationStep {
    /// 1-based step number.
    pub step: u32,
    /// The action to perform.
    pub action: RemediationAction,
    /// Current status of this step.
    pub status: StepStatus,
    /// Optional human-readable details or output.
    pub details: Option<String>,
}

/// A plan for rolling updates across a cluster, updating hosts one (or a few)
/// at a time.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RollingUpdatePlan {
    /// Unique identifier (UUID v4).
    pub id: String,
    /// Human-readable name.
    pub name: String,
    /// The cluster this plan targets.
    pub cluster_id: String,
    /// The baseline to apply.
    pub baseline_id: String,
    /// Ordered list of host IDs to update.
    pub host_order: Vec<String>,
    /// Maximum number of hosts updated concurrently (default 1).
    pub max_concurrent: u32,
    /// Seconds to pause between finishing one host and starting the next
    /// (default 60).
    pub pause_between_hosts_secs: u32,
    /// Current status of the rolling update.
    pub status: RollingUpdateStatus,
    /// Index into `host_order` of the host currently being updated.
    pub current_host_index: u32,
    pub started: Option<DateTime<Utc>>,
    pub completed: Option<DateTime<Utc>>,
}

/// Inventory snapshot of a host's OS, kernel, and installed packages.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateInventory {
    pub host_id: String,
    pub os_version: String,
    pub kernel_version: String,
    pub installed_packages: Vec<InstalledPatch>,
    pub last_updated: DateTime<Utc>,
}

/// Aggregated compliance summary for a cluster.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComplianceSummary {
    pub total_hosts: u32,
    pub compliant_hosts: u32,
    pub non_compliant_hosts: u32,
    pub critical_missing: u32,
}

// ---------------------------------------------------------------------------
// Error type
// ---------------------------------------------------------------------------

#[derive(Debug, thiserror::Error)]
pub enum LifecycleError {
    #[error("baseline not found: {0}")]
    BaselineNotFound(String),
    #[error("baseline group not found: {0}")]
    BaselineGroupNotFound(String),
    #[error("remediation task not found: {0}")]
    RemediationNotFound(String),
    #[error("remediation step not found: task {0}, step {1}")]
    StepNotFound(String, u32),
    #[error("rolling update not found: {0}")]
    RollingUpdateNotFound(String),
    #[error("invalid state transition for rolling update {0}: {1}")]
    InvalidStateTransition(String, String),
}

// ---------------------------------------------------------------------------
// LifecycleManager
// ---------------------------------------------------------------------------

/// Thread-safe manager for host OS lifecycle operations including baselines,
/// compliance scanning, remediation, rolling updates, and inventory.
#[derive(Clone)]
pub struct LifecycleManager {
    baselines: Arc<RwLock<HashMap<String, Baseline>>>,
    baseline_groups: Arc<RwLock<HashMap<String, BaselineGroup>>>,
    compliance: Arc<RwLock<HashMap<String, Vec<HostComplianceStatus>>>>,
    remediations: Arc<RwLock<HashMap<String, RemediationTask>>>,
    rolling_updates: Arc<RwLock<HashMap<String, RollingUpdatePlan>>>,
    inventories: Arc<RwLock<HashMap<String, UpdateInventory>>>,
}

impl LifecycleManager {
    /// Create a new, empty lifecycle manager.
    pub fn new() -> Self {
        Self {
            baselines: Arc::new(RwLock::new(HashMap::new())),
            baseline_groups: Arc::new(RwLock::new(HashMap::new())),
            compliance: Arc::new(RwLock::new(HashMap::new())),
            remediations: Arc::new(RwLock::new(HashMap::new())),
            rolling_updates: Arc::new(RwLock::new(HashMap::new())),
            inventories: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    // -----------------------------------------------------------------------
    // Baselines
    // -----------------------------------------------------------------------

    /// Create a new baseline and store it.
    pub fn create_baseline(&self, mut baseline: Baseline) -> Result<Baseline> {
        let mut store = self.baselines.write().unwrap_or_else(|e| e.into_inner());

        if baseline.id.is_empty() {
            baseline.id = Uuid::new_v4().to_string();
        }
        baseline.created = Utc::now();
        baseline.updated = None;

        tracing::info!(baseline_id = %baseline.id, name = %baseline.name, "created baseline");
        store.insert(baseline.id.clone(), baseline.clone());
        Ok(baseline)
    }

    /// Retrieve a baseline by ID.
    pub fn get_baseline(&self, id: &str) -> Option<Baseline> {
        let store = self.baselines.read().unwrap_or_else(|e| e.into_inner());
        store.get(id).cloned()
    }

    /// List all baselines.
    pub fn list_baselines(&self) -> Vec<Baseline> {
        let store = self.baselines.read().unwrap_or_else(|e| e.into_inner());
        store.values().cloned().collect()
    }

    /// Replace an existing baseline with a new version.
    pub fn update_baseline(&self, id: &str, mut baseline: Baseline) -> Result<Baseline> {
        let mut store = self.baselines.write().unwrap_or_else(|e| e.into_inner());

        if !store.contains_key(id) {
            bail!(LifecycleError::BaselineNotFound(id.to_string()));
        }

        baseline.id = id.to_string();
        baseline.updated = Some(Utc::now());

        tracing::info!(baseline_id = %id, "updated baseline");
        store.insert(id.to_string(), baseline.clone());
        Ok(baseline)
    }

    /// Delete a baseline by ID.
    pub fn delete_baseline(&self, id: &str) -> Result<()> {
        let mut store = self.baselines.write().unwrap_or_else(|e| e.into_inner());

        if store.remove(id).is_none() {
            bail!(LifecycleError::BaselineNotFound(id.to_string()));
        }

        tracing::info!(baseline_id = %id, "deleted baseline");
        Ok(())
    }

    /// Create a new baseline group.
    pub fn create_baseline_group(&self, mut group: BaselineGroup) -> Result<BaselineGroup> {
        let mut store = self
            .baseline_groups
            .write()
            .unwrap_or_else(|e| e.into_inner());

        if group.id.is_empty() {
            group.id = Uuid::new_v4().to_string();
        }
        group.created = Utc::now();
        group.updated = None;

        tracing::info!(group_id = %group.id, name = %group.name, "created baseline group");
        store.insert(group.id.clone(), group.clone());
        Ok(group)
    }

    /// List all baseline groups.
    pub fn list_baseline_groups(&self) -> Vec<BaselineGroup> {
        let store = self
            .baseline_groups
            .read()
            .unwrap_or_else(|e| e.into_inner());
        store.values().cloned().collect()
    }

    // -----------------------------------------------------------------------
    // Compliance scanning
    // -----------------------------------------------------------------------

    /// Scan a single host against a baseline and return its compliance status.
    ///
    /// The caller provides the list of packages currently installed on the
    /// host.  The method compares them against the baseline's requirements
    /// and records any missing or outdated packages.
    pub fn scan_host_compliance(
        &self,
        host_id: &str,
        hostname: &str,
        baseline_id: &str,
        installed: &[InstalledPatch],
    ) -> Result<HostComplianceStatus> {
        let baselines = self.baselines.read().unwrap_or_else(|e| e.into_inner());
        let baseline = baselines
            .get(baseline_id)
            .ok_or_else(|| LifecycleError::BaselineNotFound(baseline_id.to_string()))?
            .clone();
        drop(baselines);

        let installed_map: HashMap<&str, &InstalledPatch> = installed
            .iter()
            .map(|p| (p.package_name.as_str(), p))
            .collect();

        let mut missing_patches = Vec::new();
        let mut matched_installed = Vec::new();

        for req in &baseline.packages {
            match installed_map.get(req.name.as_str()) {
                Some(inst) => {
                    // Check version compliance.
                    let version_ok = if let Some(ref exact) = req.exact_version {
                        inst.version == *exact
                    } else if let Some(ref min) = req.min_version {
                        inst.version >= *min
                    } else {
                        // No version constraint -- presence is sufficient.
                        true
                    };

                    if version_ok {
                        matched_installed.push((*inst).clone());
                    } else {
                        let required_version = req
                            .exact_version
                            .clone()
                            .or_else(|| req.min_version.clone())
                            .unwrap_or_default();

                        missing_patches.push(MissingPatch {
                            package_name: req.name.clone(),
                            required_version,
                            current_version: Some(inst.version.clone()),
                            severity: req.severity.clone(),
                        });
                    }
                }
                None => {
                    let required_version = req
                        .exact_version
                        .clone()
                        .or_else(|| req.min_version.clone())
                        .unwrap_or_default();

                    missing_patches.push(MissingPatch {
                        package_name: req.name.clone(),
                        required_version,
                        current_version: None,
                        severity: req.severity.clone(),
                    });
                }
            }
        }

        let compliant = missing_patches.is_empty();

        let status = HostComplianceStatus {
            host_id: host_id.to_string(),
            hostname: hostname.to_string(),
            baseline_id: baseline_id.to_string(),
            baseline_name: baseline.name.clone(),
            compliant,
            missing_patches,
            installed_patches: matched_installed,
            scanned_at: Utc::now(),
        };

        // Store the result.
        let mut compliance = self.compliance.write().unwrap_or_else(|e| e.into_inner());
        compliance
            .entry(host_id.to_string())
            .or_default()
            .push(status.clone());

        tracing::info!(
            host_id = %host_id,
            baseline_id = %baseline_id,
            compliant = %compliant,
            "scanned host compliance"
        );
        Ok(status)
    }

    /// Scan multiple hosts in a cluster against a baseline.
    ///
    /// Each tuple in `hosts` is (host_id, hostname, installed_patches).
    pub fn scan_cluster_compliance(
        &self,
        _cluster_id: &str,
        hosts: &[(String, String, Vec<InstalledPatch>)],
        baseline_id: &str,
    ) -> Vec<HostComplianceStatus> {
        hosts
            .iter()
            .filter_map(|(host_id, hostname, installed)| {
                self.scan_host_compliance(host_id, hostname, baseline_id, installed)
                    .ok()
            })
            .collect()
    }

    /// Retrieve all stored compliance statuses for a host.
    pub fn get_compliance_status(&self, host_id: &str) -> Vec<HostComplianceStatus> {
        let compliance = self.compliance.read().unwrap_or_else(|e| e.into_inner());
        compliance.get(host_id).cloned().unwrap_or_default()
    }

    /// Build an aggregated compliance summary for a cluster.
    ///
    /// This examines all stored compliance records whose host IDs appear in
    /// the compliance store.  For a production system the cluster-to-host
    /// mapping would come from a separate registry; here we summarise all
    /// known records.
    pub fn get_cluster_compliance_summary(&self, _cluster_id: &str) -> ComplianceSummary {
        let compliance = self.compliance.read().unwrap_or_else(|e| e.into_inner());

        let mut total_hosts: u32 = 0;
        let mut compliant_hosts: u32 = 0;
        let mut non_compliant_hosts: u32 = 0;
        let mut critical_missing: u32 = 0;

        for statuses in compliance.values() {
            if let Some(latest) = statuses.last() {
                total_hosts += 1;
                if latest.compliant {
                    compliant_hosts += 1;
                } else {
                    non_compliant_hosts += 1;
                    critical_missing += latest
                        .missing_patches
                        .iter()
                        .filter(|p| p.severity == Some(PatchSeverity::Critical))
                        .count() as u32;
                }
            }
        }

        ComplianceSummary {
            total_hosts,
            compliant_hosts,
            non_compliant_hosts,
            critical_missing,
        }
    }

    // -----------------------------------------------------------------------
    // Remediation
    // -----------------------------------------------------------------------

    /// Create a new remediation task for a host.
    ///
    /// The task is initialized with the standard five-step remediation
    /// workflow: evacuate VMs, apply patches, reboot, validate, restore VMs.
    pub fn create_remediation(
        &self,
        host_id: &str,
        hostname: &str,
        baseline_id: &str,
    ) -> Result<RemediationTask> {
        let id = Uuid::new_v4().to_string();

        let steps = vec![
            RemediationStep {
                step: 1,
                action: RemediationAction::EvacuateVMs,
                status: StepStatus::Pending,
                details: None,
            },
            RemediationStep {
                step: 2,
                action: RemediationAction::ApplyPatches,
                status: StepStatus::Pending,
                details: None,
            },
            RemediationStep {
                step: 3,
                action: RemediationAction::RebootHost,
                status: StepStatus::Pending,
                details: None,
            },
            RemediationStep {
                step: 4,
                action: RemediationAction::ValidateHost,
                status: StepStatus::Pending,
                details: None,
            },
            RemediationStep {
                step: 5,
                action: RemediationAction::RestoreVMs,
                status: StepStatus::Pending,
                details: None,
            },
        ];

        let task = RemediationTask {
            id: id.clone(),
            host_id: host_id.to_string(),
            hostname: hostname.to_string(),
            baseline_id: baseline_id.to_string(),
            status: RemediationStatus::Pending,
            steps,
            started: None,
            completed: None,
            error: None,
        };

        let mut store = self.remediations.write().unwrap_or_else(|e| e.into_inner());
        tracing::info!(
            task_id = %id,
            host_id = %host_id,
            baseline_id = %baseline_id,
            "created remediation task"
        );
        store.insert(id, task.clone());
        Ok(task)
    }

    /// Retrieve a remediation task by ID.
    pub fn get_remediation(&self, id: &str) -> Option<RemediationTask> {
        let store = self.remediations.read().unwrap_or_else(|e| e.into_inner());
        store.get(id).cloned()
    }

    /// List remediation tasks, optionally filtered by host ID.
    pub fn list_remediations(&self, host_id: Option<&str>) -> Vec<RemediationTask> {
        let store = self.remediations.read().unwrap_or_else(|e| e.into_inner());
        store
            .values()
            .filter(|t| match host_id {
                Some(hid) => t.host_id == hid,
                None => true,
            })
            .cloned()
            .collect()
    }

    /// Update the status and details of a specific remediation step.
    pub fn update_remediation_step(
        &self,
        id: &str,
        step: u32,
        status: StepStatus,
        details: Option<String>,
    ) -> Result<()> {
        let mut store = self.remediations.write().unwrap_or_else(|e| e.into_inner());
        let task = store
            .get_mut(id)
            .ok_or_else(|| LifecycleError::RemediationNotFound(id.to_string()))?;

        let remediation_step = task
            .steps
            .iter_mut()
            .find(|s| s.step == step)
            .ok_or_else(|| LifecycleError::StepNotFound(id.to_string(), step))?;

        remediation_step.status = status;
        remediation_step.details = details;

        // Update the overall task status based on the step being updated.
        if task.started.is_none() {
            task.started = Some(Utc::now());
        }

        // Derive overall status from the step action being updated.
        match remediation_step.action {
            RemediationAction::EvacuateVMs => task.status = RemediationStatus::Evacuating,
            RemediationAction::ApplyPatches => task.status = RemediationStatus::Updating,
            RemediationAction::RebootHost => task.status = RemediationStatus::Rebooting,
            RemediationAction::ValidateHost | RemediationAction::RestoreVMs => {
                task.status = RemediationStatus::Restoring
            }
        }

        tracing::info!(task_id = %id, step = %step, "updated remediation step");
        Ok(())
    }

    /// Mark a remediation task as completed (successfully or with failure).
    pub fn complete_remediation(
        &self,
        id: &str,
        success: bool,
        error: Option<String>,
    ) -> Result<()> {
        let mut store = self.remediations.write().unwrap_or_else(|e| e.into_inner());
        let task = store
            .get_mut(id)
            .ok_or_else(|| LifecycleError::RemediationNotFound(id.to_string()))?;

        task.completed = Some(Utc::now());

        if success {
            task.status = RemediationStatus::Completed;
        } else {
            task.status = RemediationStatus::Failed;
            task.error = error;
        }

        tracing::info!(task_id = %id, success = %success, "completed remediation");
        Ok(())
    }

    // -----------------------------------------------------------------------
    // Rolling updates
    // -----------------------------------------------------------------------

    /// Create a new rolling update plan.
    pub fn create_rolling_update(&self, mut plan: RollingUpdatePlan) -> Result<RollingUpdatePlan> {
        let mut store = self
            .rolling_updates
            .write()
            .unwrap_or_else(|e| e.into_inner());

        if plan.id.is_empty() {
            plan.id = Uuid::new_v4().to_string();
        }
        plan.status = RollingUpdateStatus::Planned;
        plan.current_host_index = 0;
        plan.started = None;
        plan.completed = None;

        if plan.max_concurrent == 0 {
            plan.max_concurrent = 1;
        }

        tracing::info!(plan_id = %plan.id, name = %plan.name, "created rolling update plan");
        store.insert(plan.id.clone(), plan.clone());
        Ok(plan)
    }

    /// Retrieve a rolling update plan by ID.
    pub fn get_rolling_update(&self, id: &str) -> Option<RollingUpdatePlan> {
        let store = self
            .rolling_updates
            .read()
            .unwrap_or_else(|e| e.into_inner());
        store.get(id).cloned()
    }

    /// List rolling update plans, optionally filtered by cluster ID.
    pub fn list_rolling_updates(&self, cluster_id: Option<&str>) -> Vec<RollingUpdatePlan> {
        let store = self
            .rolling_updates
            .read()
            .unwrap_or_else(|e| e.into_inner());
        store
            .values()
            .filter(|p| match cluster_id {
                Some(cid) => p.cluster_id == cid,
                None => true,
            })
            .cloned()
            .collect()
    }

    /// Start a planned rolling update.
    pub fn start_rolling_update(&self, id: &str) -> Result<()> {
        let mut store = self
            .rolling_updates
            .write()
            .unwrap_or_else(|e| e.into_inner());
        let plan = store
            .get_mut(id)
            .ok_or_else(|| LifecycleError::RollingUpdateNotFound(id.to_string()))?;

        if plan.status != RollingUpdateStatus::Planned {
            bail!(LifecycleError::InvalidStateTransition(
                id.to_string(),
                format!("cannot start from {:?}", plan.status),
            ));
        }

        plan.status = RollingUpdateStatus::InProgress;
        plan.started = Some(Utc::now());

        tracing::info!(plan_id = %id, "started rolling update");
        Ok(())
    }

    /// Pause a running rolling update.
    pub fn pause_rolling_update(&self, id: &str) -> Result<()> {
        let mut store = self
            .rolling_updates
            .write()
            .unwrap_or_else(|e| e.into_inner());
        let plan = store
            .get_mut(id)
            .ok_or_else(|| LifecycleError::RollingUpdateNotFound(id.to_string()))?;

        if plan.status != RollingUpdateStatus::InProgress {
            bail!(LifecycleError::InvalidStateTransition(
                id.to_string(),
                format!("cannot pause from {:?}", plan.status),
            ));
        }

        plan.status = RollingUpdateStatus::Paused;

        tracing::info!(plan_id = %id, "paused rolling update");
        Ok(())
    }

    /// Resume a paused rolling update.
    pub fn resume_rolling_update(&self, id: &str) -> Result<()> {
        let mut store = self
            .rolling_updates
            .write()
            .unwrap_or_else(|e| e.into_inner());
        let plan = store
            .get_mut(id)
            .ok_or_else(|| LifecycleError::RollingUpdateNotFound(id.to_string()))?;

        if plan.status != RollingUpdateStatus::Paused {
            bail!(LifecycleError::InvalidStateTransition(
                id.to_string(),
                format!("cannot resume from {:?}", plan.status),
            ));
        }

        plan.status = RollingUpdateStatus::InProgress;

        tracing::info!(plan_id = %id, "resumed rolling update");
        Ok(())
    }

    /// Advance the rolling update to the next host.
    ///
    /// Returns the host ID of the next host to update, or `None` if all
    /// hosts have been processed.
    pub fn advance_rolling_update(&self, id: &str) -> Result<Option<String>> {
        let mut store = self
            .rolling_updates
            .write()
            .unwrap_or_else(|e| e.into_inner());
        let plan = store
            .get_mut(id)
            .ok_or_else(|| LifecycleError::RollingUpdateNotFound(id.to_string()))?;

        if plan.status != RollingUpdateStatus::InProgress {
            bail!(LifecycleError::InvalidStateTransition(
                id.to_string(),
                format!("cannot advance from {:?}", plan.status),
            ));
        }

        let index = plan.current_host_index as usize;
        if index >= plan.host_order.len() {
            // All hosts processed.
            plan.status = RollingUpdateStatus::Completed;
            plan.completed = Some(Utc::now());
            tracing::info!(plan_id = %id, "rolling update completed (all hosts done)");
            return Ok(None);
        }

        let host_id = plan.host_order[index].clone();
        plan.current_host_index += 1;

        tracing::info!(
            plan_id = %id,
            host_id = %host_id,
            index = %index,
            "advanced rolling update to next host"
        );
        Ok(Some(host_id))
    }

    /// Mark a rolling update as completed.
    pub fn complete_rolling_update(&self, id: &str) -> Result<()> {
        let mut store = self
            .rolling_updates
            .write()
            .unwrap_or_else(|e| e.into_inner());
        let plan = store
            .get_mut(id)
            .ok_or_else(|| LifecycleError::RollingUpdateNotFound(id.to_string()))?;

        plan.status = RollingUpdateStatus::Completed;
        plan.completed = Some(Utc::now());

        tracing::info!(plan_id = %id, "completed rolling update");
        Ok(())
    }

    // -----------------------------------------------------------------------
    // Inventory
    // -----------------------------------------------------------------------

    /// Store or update the inventory snapshot for a host.
    pub fn update_host_inventory(&self, inventory: UpdateInventory) -> Result<()> {
        let mut store = self.inventories.write().unwrap_or_else(|e| e.into_inner());

        tracing::info!(
            host_id = %inventory.host_id,
            os = %inventory.os_version,
            kernel = %inventory.kernel_version,
            "updated host inventory"
        );
        store.insert(inventory.host_id.clone(), inventory);
        Ok(())
    }

    /// Retrieve the inventory snapshot for a host.
    pub fn get_host_inventory(&self, host_id: &str) -> Option<UpdateInventory> {
        let store = self.inventories.read().unwrap_or_else(|e| e.into_inner());
        store.get(host_id).cloned()
    }
}

impl Default for LifecycleManager {
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

    // -- Helpers ------------------------------------------------------------

    fn make_baseline(name: &str) -> Baseline {
        Baseline {
            id: String::new(),
            name: name.to_string(),
            description: Some("test baseline".to_string()),
            baseline_type: BaselineType::Patch,
            packages: vec![
                PackageRequirement {
                    name: "openssl".to_string(),
                    min_version: Some("3.0.10".to_string()),
                    exact_version: None,
                    severity: Some(PatchSeverity::Critical),
                },
                PackageRequirement {
                    name: "kernel".to_string(),
                    min_version: Some("6.1.50".to_string()),
                    exact_version: None,
                    severity: Some(PatchSeverity::Important),
                },
                PackageRequirement {
                    name: "bash".to_string(),
                    exact_version: Some("5.2.15".to_string()),
                    min_version: None,
                    severity: Some(PatchSeverity::Low),
                },
            ],
            created: Utc::now(),
            updated: None,
        }
    }

    fn compliant_patches() -> Vec<InstalledPatch> {
        vec![
            InstalledPatch {
                package_name: "openssl".to_string(),
                version: "3.0.12".to_string(),
                installed_at: Some(Utc::now()),
            },
            InstalledPatch {
                package_name: "kernel".to_string(),
                version: "6.1.55".to_string(),
                installed_at: Some(Utc::now()),
            },
            InstalledPatch {
                package_name: "bash".to_string(),
                version: "5.2.15".to_string(),
                installed_at: Some(Utc::now()),
            },
        ]
    }

    fn non_compliant_patches() -> Vec<InstalledPatch> {
        vec![
            InstalledPatch {
                package_name: "openssl".to_string(),
                version: "3.0.09".to_string(), // below min 3.0.10 (string comparison)
                installed_at: Some(Utc::now()),
            },
            // kernel missing entirely
            InstalledPatch {
                package_name: "bash".to_string(),
                version: "5.2.10".to_string(), // not exact 5.2.15
                installed_at: Some(Utc::now()),
            },
        ]
    }

    fn make_rolling_update_plan(baseline_id: &str) -> RollingUpdatePlan {
        RollingUpdatePlan {
            id: String::new(),
            name: "cluster-update-1".to_string(),
            cluster_id: "cluster-01".to_string(),
            baseline_id: baseline_id.to_string(),
            host_order: vec![
                "host-1".to_string(),
                "host-2".to_string(),
                "host-3".to_string(),
            ],
            max_concurrent: 1,
            pause_between_hosts_secs: 60,
            status: RollingUpdateStatus::Planned,
            current_host_index: 0,
            started: None,
            completed: None,
        }
    }

    // -- 1. Baseline CRUD: create and get ------------------------------------

    #[test]
    fn test_create_and_get_baseline() {
        let mgr = LifecycleManager::new();
        let baseline = mgr.create_baseline(make_baseline("security-2024")).unwrap();

        assert!(!baseline.id.is_empty());
        assert_eq!(baseline.name, "security-2024");
        assert_eq!(baseline.packages.len(), 3);

        let fetched = mgr.get_baseline(&baseline.id).unwrap();
        assert_eq!(fetched.name, baseline.name);
    }

    // -- 2. Baseline CRUD: list and delete -----------------------------------

    #[test]
    fn test_list_and_delete_baselines() {
        let mgr = LifecycleManager::new();
        mgr.create_baseline(make_baseline("bl-1")).unwrap();
        mgr.create_baseline(make_baseline("bl-2")).unwrap();

        assert_eq!(mgr.list_baselines().len(), 2);

        let id = mgr.list_baselines()[0].id.clone();
        mgr.delete_baseline(&id).unwrap();
        assert_eq!(mgr.list_baselines().len(), 1);
        assert!(mgr.get_baseline(&id).is_none());
    }

    // -- 3. Baseline CRUD: update --------------------------------------------

    #[test]
    fn test_update_baseline() {
        let mgr = LifecycleManager::new();
        let baseline = mgr.create_baseline(make_baseline("original")).unwrap();
        let id = baseline.id.clone();

        let mut updated_baseline = make_baseline("updated");
        updated_baseline.description = Some("updated description".to_string());

        let result = mgr.update_baseline(&id, updated_baseline).unwrap();
        assert_eq!(result.name, "updated");
        assert_eq!(result.description.as_deref(), Some("updated description"));
        assert!(result.updated.is_some());

        // Updating a nonexistent baseline should fail.
        let err = mgr.update_baseline("nonexistent", make_baseline("x"));
        assert!(err.is_err());
    }

    // -- 4. Baseline group creation ------------------------------------------

    #[test]
    fn test_baseline_group() {
        let mgr = LifecycleManager::new();
        let bl1 = mgr.create_baseline(make_baseline("bl-1")).unwrap();
        let bl2 = mgr.create_baseline(make_baseline("bl-2")).unwrap();

        let group = mgr
            .create_baseline_group(BaselineGroup {
                id: String::new(),
                name: "production-baselines".to_string(),
                baseline_ids: vec![bl1.id.clone(), bl2.id.clone()],
                created: Utc::now(),
                updated: None,
            })
            .unwrap();

        assert!(!group.id.is_empty());
        assert_eq!(group.baseline_ids.len(), 2);

        let groups = mgr.list_baseline_groups();
        assert_eq!(groups.len(), 1);
    }

    // -- 5. Compliance scan: compliant host ----------------------------------

    #[test]
    fn test_scan_host_compliant() {
        let mgr = LifecycleManager::new();
        let baseline = mgr.create_baseline(make_baseline("sec")).unwrap();

        let status = mgr
            .scan_host_compliance("h1", "host-01.lab", &baseline.id, &compliant_patches())
            .unwrap();

        assert!(status.compliant);
        assert!(status.missing_patches.is_empty());
        assert_eq!(status.installed_patches.len(), 3);
        assert_eq!(status.baseline_name, "sec");
    }

    // -- 6. Compliance scan: non-compliant host ------------------------------

    #[test]
    fn test_scan_host_non_compliant() {
        let mgr = LifecycleManager::new();
        let baseline = mgr.create_baseline(make_baseline("sec")).unwrap();

        let status = mgr
            .scan_host_compliance("h2", "host-02.lab", &baseline.id, &non_compliant_patches())
            .unwrap();

        assert!(!status.compliant);
        // openssl version too low, kernel missing, bash wrong exact version
        assert_eq!(status.missing_patches.len(), 3);

        let kernel_missing = status
            .missing_patches
            .iter()
            .find(|p| p.package_name == "kernel")
            .unwrap();
        assert!(kernel_missing.current_version.is_none());
        assert_eq!(kernel_missing.severity, Some(PatchSeverity::Important));

        let openssl_missing = status
            .missing_patches
            .iter()
            .find(|p| p.package_name == "openssl")
            .unwrap();
        assert_eq!(openssl_missing.current_version.as_deref(), Some("3.0.09"));
    }

    // -- 7. Cluster compliance summary ---------------------------------------

    #[test]
    fn test_cluster_compliance_summary() {
        let mgr = LifecycleManager::new();
        let baseline = mgr.create_baseline(make_baseline("sec")).unwrap();

        mgr.scan_host_compliance("h1", "host-01", &baseline.id, &compliant_patches())
            .unwrap();
        mgr.scan_host_compliance("h2", "host-02", &baseline.id, &non_compliant_patches())
            .unwrap();

        let summary = mgr.get_cluster_compliance_summary("cluster-01");
        assert_eq!(summary.total_hosts, 2);
        assert_eq!(summary.compliant_hosts, 1);
        assert_eq!(summary.non_compliant_hosts, 1);
        // openssl is Critical severity and is missing on h2
        assert!(summary.critical_missing >= 1);
    }

    // -- 8. Remediation step tracking ----------------------------------------

    #[test]
    fn test_remediation_step_tracking() {
        let mgr = LifecycleManager::new();
        let baseline = mgr.create_baseline(make_baseline("sec")).unwrap();

        let task = mgr
            .create_remediation("h1", "host-01", &baseline.id)
            .unwrap();
        assert_eq!(task.status, RemediationStatus::Pending);
        assert_eq!(task.steps.len(), 5);

        // Advance through step 1 (evacuate).
        mgr.update_remediation_step(
            &task.id,
            1,
            StepStatus::Running,
            Some("evacuating 3 VMs".to_string()),
        )
        .unwrap();

        let t = mgr.get_remediation(&task.id).unwrap();
        assert_eq!(t.status, RemediationStatus::Evacuating);
        assert!(t.started.is_some());

        mgr.update_remediation_step(&task.id, 1, StepStatus::Completed, None)
            .unwrap();

        // Advance step 2 (apply patches).
        mgr.update_remediation_step(
            &task.id,
            2,
            StepStatus::Running,
            Some("installing 5 packages".to_string()),
        )
        .unwrap();

        let t = mgr.get_remediation(&task.id).unwrap();
        assert_eq!(t.status, RemediationStatus::Updating);

        // Complete the task.
        mgr.complete_remediation(&task.id, true, None).unwrap();
        let t = mgr.get_remediation(&task.id).unwrap();
        assert_eq!(t.status, RemediationStatus::Completed);
        assert!(t.completed.is_some());
        assert!(t.error.is_none());
    }

    // -- 9. Remediation failure ----------------------------------------------

    #[test]
    fn test_remediation_failure() {
        let mgr = LifecycleManager::new();
        let baseline = mgr.create_baseline(make_baseline("sec")).unwrap();

        let task = mgr
            .create_remediation("h1", "host-01", &baseline.id)
            .unwrap();

        mgr.complete_remediation(
            &task.id,
            false,
            Some("disk full during patch install".to_string()),
        )
        .unwrap();

        let t = mgr.get_remediation(&task.id).unwrap();
        assert_eq!(t.status, RemediationStatus::Failed);
        assert_eq!(t.error.as_deref(), Some("disk full during patch install"));
    }

    // -- 10. Rolling update advancement --------------------------------------

    #[test]
    fn test_rolling_update_advancement() {
        let mgr = LifecycleManager::new();
        let baseline = mgr.create_baseline(make_baseline("sec")).unwrap();

        let plan = mgr
            .create_rolling_update(make_rolling_update_plan(&baseline.id))
            .unwrap();
        assert_eq!(plan.status, RollingUpdateStatus::Planned);

        // Start the rolling update.
        mgr.start_rolling_update(&plan.id).unwrap();
        let p = mgr.get_rolling_update(&plan.id).unwrap();
        assert_eq!(p.status, RollingUpdateStatus::InProgress);
        assert!(p.started.is_some());

        // Advance through all three hosts.
        let h1 = mgr.advance_rolling_update(&plan.id).unwrap();
        assert_eq!(h1, Some("host-1".to_string()));

        let h2 = mgr.advance_rolling_update(&plan.id).unwrap();
        assert_eq!(h2, Some("host-2".to_string()));

        let h3 = mgr.advance_rolling_update(&plan.id).unwrap();
        assert_eq!(h3, Some("host-3".to_string()));

        // No more hosts -- should return None and mark completed.
        let done = mgr.advance_rolling_update(&plan.id).unwrap();
        assert!(done.is_none());

        let p = mgr.get_rolling_update(&plan.id).unwrap();
        assert_eq!(p.status, RollingUpdateStatus::Completed);
        assert!(p.completed.is_some());
    }

    // -- 11. Rolling update pause and resume ---------------------------------

    #[test]
    fn test_rolling_update_pause_resume() {
        let mgr = LifecycleManager::new();
        let baseline = mgr.create_baseline(make_baseline("sec")).unwrap();

        let plan = mgr
            .create_rolling_update(make_rolling_update_plan(&baseline.id))
            .unwrap();

        mgr.start_rolling_update(&plan.id).unwrap();
        mgr.advance_rolling_update(&plan.id).unwrap();

        // Pause.
        mgr.pause_rolling_update(&plan.id).unwrap();
        let p = mgr.get_rolling_update(&plan.id).unwrap();
        assert_eq!(p.status, RollingUpdateStatus::Paused);

        // Cannot advance while paused.
        let err = mgr.advance_rolling_update(&plan.id);
        assert!(err.is_err());

        // Resume.
        mgr.resume_rolling_update(&plan.id).unwrap();
        let p = mgr.get_rolling_update(&plan.id).unwrap();
        assert_eq!(p.status, RollingUpdateStatus::InProgress);

        // Can advance again.
        let h2 = mgr.advance_rolling_update(&plan.id).unwrap();
        assert_eq!(h2, Some("host-2".to_string()));
    }

    // -- 12. Inventory management --------------------------------------------

    #[test]
    fn test_inventory_management() {
        let mgr = LifecycleManager::new();

        let inventory = UpdateInventory {
            host_id: "h1".to_string(),
            os_version: "Fedora 40".to_string(),
            kernel_version: "6.8.10-300.fc40".to_string(),
            installed_packages: vec![InstalledPatch {
                package_name: "openssl".to_string(),
                version: "3.0.12".to_string(),
                installed_at: Some(Utc::now()),
            }],
            last_updated: Utc::now(),
        };

        mgr.update_host_inventory(inventory).unwrap();

        let fetched = mgr.get_host_inventory("h1").unwrap();
        assert_eq!(fetched.os_version, "Fedora 40");
        assert_eq!(fetched.kernel_version, "6.8.10-300.fc40");
        assert_eq!(fetched.installed_packages.len(), 1);

        // Non-existent host returns None.
        assert!(mgr.get_host_inventory("h999").is_none());
    }

    // -- 13. List remediations with filter -----------------------------------

    #[test]
    fn test_list_remediations_filter() {
        let mgr = LifecycleManager::new();
        let baseline = mgr.create_baseline(make_baseline("sec")).unwrap();

        mgr.create_remediation("h1", "host-01", &baseline.id)
            .unwrap();
        mgr.create_remediation("h2", "host-02", &baseline.id)
            .unwrap();
        mgr.create_remediation("h1", "host-01", &baseline.id)
            .unwrap();

        let all = mgr.list_remediations(None);
        assert_eq!(all.len(), 3);

        let h1_only = mgr.list_remediations(Some("h1"));
        assert_eq!(h1_only.len(), 2);

        let h2_only = mgr.list_remediations(Some("h2"));
        assert_eq!(h2_only.len(), 1);
    }

    // -- 14. Delete nonexistent baseline fails -------------------------------

    #[test]
    fn test_delete_nonexistent_baseline_fails() {
        let mgr = LifecycleManager::new();
        let result = mgr.delete_baseline("nonexistent");
        assert!(result.is_err());
    }

    // -- 15. Rolling update invalid state transitions ------------------------

    #[test]
    fn test_rolling_update_invalid_transitions() {
        let mgr = LifecycleManager::new();
        let baseline = mgr.create_baseline(make_baseline("sec")).unwrap();

        let plan = mgr
            .create_rolling_update(make_rolling_update_plan(&baseline.id))
            .unwrap();

        // Cannot pause a planned (not started) update.
        assert!(mgr.pause_rolling_update(&plan.id).is_err());

        // Cannot resume a planned update.
        assert!(mgr.resume_rolling_update(&plan.id).is_err());

        // Start, then try to start again.
        mgr.start_rolling_update(&plan.id).unwrap();
        assert!(mgr.start_rolling_update(&plan.id).is_err());
    }
}
