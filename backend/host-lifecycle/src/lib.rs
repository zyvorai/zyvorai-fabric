// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: LicenseRef-Zyvor-Production-1.0

//! Host maintenance and VM evacuation orchestration for Fabric.
//!
//! This crate provides the production-oriented layer that sits between the
//! datacenter inventory and a concrete migration backend:
//!
//! * maintenance preflight checks;
//! * deterministic evacuation planning;
//! * capacity reservation and placement constraints;
//! * live/cold migration policy;
//! * pinned-workload protection;
//! * asynchronous execution with bounded parallelism;
//! * explicit job state transitions and failure reporting.
//!
//! The executor is intentionally abstract. Fabric's API/server can bind it to
//! FluxVM, a remote host agent, or another hypervisor implementation without
//! coupling the planning/state machine to a transport.

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use datacenter::HostStatus;
use futures::{stream, StreamExt};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap};
use std::sync::{Arc, RwLock};
use uuid::Uuid;

/// Strategy used when choosing how a workload is evacuated.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EvacuationStrategy {
    /// Every workload must support live migration. Any workload that does not
    /// support it blocks the plan.
    LiveOnly,
    /// Use live migration when possible and fall back to a cold migration for
    /// workloads that cannot move live.
    PreferLive,
    /// Always use a cold migration.
    ColdOnly,
}

/// Concrete migration mode selected for one workload.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MigrationMode {
    Live,
    Cold,
}

/// State of an evacuation assignment.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AssignmentStatus {
    Planned,
    Running,
    Completed,
    Failed,
}

/// Overall state of a host maintenance job.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MaintenanceJobStatus {
    /// Preflight found blockers. The job cannot be executed as-is.
    Blocked,
    /// Plan is valid and ready to execute.
    Planned,
    /// The source host is cordoned and migrations are in progress.
    Running,
    /// All workloads are evacuated and the host is in maintenance mode.
    Maintenance,
    /// Maintenance is finished and the host has returned to service.
    Completed,
    /// Operator cancelled the job before execution.
    Cancelled,
    /// Execution failed and requires inspection/recovery.
    Failed,
}

/// Stable machine-readable reason for a preflight blocker.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BlockerCode {
    SourceHostUnavailable,
    WorkloadNotOnSource,
    LiveMigrationRequired,
    PinnedToSource,
    PinnedTargetUnavailable,
    NoEligibleTarget,
    InsufficientCapacity,
}

/// A normalized workload that currently resides on the source host.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Workload {
    pub vm_id: String,
    pub source_host_id: String,
    pub cpus: u32,
    pub memory_mb: u64,
    /// Whether the underlying VM/backend can perform a live migration.
    pub live_migratable: bool,
    /// Optional hard host pin. A VM pinned to the source blocks maintenance
    /// unless `allow_pinned` is explicitly enabled.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pinned_host_id: Option<String>,
    /// Required target labels. Every entry must match the target host.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub required_labels: BTreeMap<String, String>,
}

/// A potential evacuation target with already-calculated free capacity.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HostCandidate {
    pub host_id: String,
    pub cluster_id: String,
    pub status: HostStatus,
    pub free_cpus: u32,
    pub free_memory_mb: u64,
    /// Scheduler/administrator gate for new placements.
    pub accepts_new_workloads: bool,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub labels: BTreeMap<String, String>,
}

impl HostCandidate {
    pub fn new(
        host_id: impl Into<String>,
        cluster_id: impl Into<String>,
        status: HostStatus,
        free_cpus: u32,
        free_memory_mb: u64,
    ) -> Self {
        Self {
            host_id: host_id.into(),
            cluster_id: cluster_id.into(),
            status,
            free_cpus,
            free_memory_mb,
            accepts_new_workloads: true,
            labels: BTreeMap::new(),
        }
    }
}

/// Policy applied while generating and executing an evacuation plan.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EvacuationPolicy {
    pub strategy: EvacuationStrategy,
    /// Maximum migrations that may execute concurrently.
    pub max_parallel: u32,
    /// Percentage of each target host's free CPU capacity that remains
    /// reserved and cannot be consumed by the evacuation.
    pub reserve_cpu_percent: u8,
    /// Percentage of each target host's free memory that remains reserved.
    pub reserve_memory_percent: u8,
    /// Allow targets outside the source cluster. Disabled by default because
    /// cross-cluster migration may violate storage/network assumptions.
    pub allow_cross_cluster: bool,
    /// Allow evacuation of a workload that is explicitly pinned to the source
    /// host. Disabled by default because host pinning is normally a hard
    /// placement constraint.
    pub allow_pinned: bool,
    /// Build a real plan but refuse execution. Useful for UI/CLI preflight.
    pub dry_run: bool,
}

impl Default for EvacuationPolicy {
    fn default() -> Self {
        Self {
            strategy: EvacuationStrategy::PreferLive,
            max_parallel: 2,
            reserve_cpu_percent: 10,
            reserve_memory_percent: 10,
            allow_cross_cluster: false,
            allow_pinned: false,
            dry_run: false,
        }
    }
}

/// Input required to plan maintenance for one host.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MaintenanceRequest {
    pub source_host_id: String,
    pub source_cluster_id: String,
    pub source_status: HostStatus,
    #[serde(default)]
    pub workloads: Vec<Workload>,
    #[serde(default)]
    pub targets: Vec<HostCandidate>,
    #[serde(default)]
    pub policy: EvacuationPolicy,
}

/// One preflight issue that prevents safe execution.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EvacuationBlocker {
    pub code: BlockerCode,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub vm_id: Option<String>,
    pub message: String,
}

/// One VM migration selected by the planner.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EvacuationAssignment {
    pub vm_id: String,
    pub source_host_id: String,
    pub target_host_id: String,
    pub cpus: u32,
    pub memory_mb: u64,
    pub mode: MigrationMode,
    pub status: AssignmentStatus,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

/// Result of the maintenance preflight/planning phase.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EvacuationPlan {
    pub id: String,
    pub source_host_id: String,
    pub source_cluster_id: String,
    pub policy: EvacuationPolicy,
    pub assignments: Vec<EvacuationAssignment>,
    pub blockers: Vec<EvacuationBlocker>,
    pub created_at: DateTime<Utc>,
}

impl EvacuationPlan {
    pub fn is_executable(&self) -> bool {
        self.blockers.is_empty() && !self.policy.dry_run
    }
}

/// Persistent/loggable job wrapper around an evacuation plan.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MaintenanceJob {
    pub id: String,
    pub source_host_id: String,
    pub status: MaintenanceJobStatus,
    pub plan: EvacuationPlan,
    pub created_at: DateTime<Utc>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub started_at: Option<DateTime<Utc>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub maintenance_at: Option<DateTime<Utc>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub completed_at: Option<DateTime<Utc>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

/// Errors produced by the planner/manager itself.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum HostLifecycleError {
    #[error("invalid evacuation policy: {0}")]
    InvalidPolicy(String),
    #[error("maintenance job not found: {0}")]
    JobNotFound(String),
    #[error("maintenance job already active for host {0}")]
    ActiveJobExists(String),
    #[error("job {job_id} is blocked by {blocker_count} preflight issue(s)")]
    JobBlocked {
        job_id: String,
        blocker_count: usize,
    },
    #[error("dry-run job cannot be executed: {0}")]
    DryRun(String),
    #[error("invalid state transition for job {job_id}: {message}")]
    InvalidState { job_id: String, message: String },
    #[error("assignment {vm_id} not found in job {job_id}")]
    AssignmentNotFound { job_id: String, vm_id: String },
    #[error("execution failed for job {job_id}: {message}")]
    ExecutionFailed { job_id: String, message: String },
}

#[derive(Debug, Clone)]
struct MutableTarget {
    host_id: String,
    free_cpus: u32,
    free_memory_mb: u64,
    labels: BTreeMap<String, String>,
}

/// Stateless deterministic evacuation planner.
pub struct EvacuationPlanner;

impl EvacuationPlanner {
    pub fn plan(request: &MaintenanceRequest) -> Result<EvacuationPlan, HostLifecycleError> {
        validate_policy(&request.policy)?;

        let mut blockers = Vec::new();

        if request.source_status != HostStatus::Connected
            && request.source_status != HostStatus::Maintenance
        {
            blockers.push(EvacuationBlocker {
                code: BlockerCode::SourceHostUnavailable,
                vm_id: None,
                message: format!(
                    "source host {} is not connected (status: {:?})",
                    request.source_host_id, request.source_status
                ),
            });
        }

        let mut targets: Vec<MutableTarget> = request
            .targets
            .iter()
            .filter(|target| target.host_id != request.source_host_id)
            .filter(|target| target.status == HostStatus::Connected)
            .filter(|target| target.accepts_new_workloads)
            .filter(|target| {
                request.policy.allow_cross_cluster || target.cluster_id == request.source_cluster_id
            })
            .map(|target| MutableTarget {
                host_id: target.host_id.clone(),
                free_cpus: capacity_after_reserve_u32(
                    target.free_cpus,
                    request.policy.reserve_cpu_percent,
                ),
                free_memory_mb: capacity_after_reserve_u64(
                    target.free_memory_mb,
                    request.policy.reserve_memory_percent,
                ),
                labels: target.labels.clone(),
            })
            .collect();

        // Keep tie-breaking deterministic across HashMap/API ordering changes.
        targets.sort_by(|a, b| a.host_id.cmp(&b.host_id));

        // Largest workloads first reduces fragmentation and makes a plan more
        // likely to succeed when capacity is tight.
        let mut workloads = request.workloads.clone();
        workloads.sort_by(|a, b| {
            b.memory_mb
                .cmp(&a.memory_mb)
                .then_with(|| b.cpus.cmp(&a.cpus))
                .then_with(|| a.vm_id.cmp(&b.vm_id))
        });

        let mut assignments = Vec::with_capacity(workloads.len());

        for workload in workloads {
            if workload.source_host_id != request.source_host_id {
                blockers.push(EvacuationBlocker {
                    code: BlockerCode::WorkloadNotOnSource,
                    vm_id: Some(workload.vm_id.clone()),
                    message: format!(
                        "workload {} reports source host {}, expected {}",
                        workload.vm_id, workload.source_host_id, request.source_host_id
                    ),
                });
                continue;
            }

            let mode = match request.policy.strategy {
                EvacuationStrategy::LiveOnly if !workload.live_migratable => {
                    blockers.push(EvacuationBlocker {
                        code: BlockerCode::LiveMigrationRequired,
                        vm_id: Some(workload.vm_id.clone()),
                        message: format!(
                            "workload {} cannot live-migrate but policy is live_only",
                            workload.vm_id
                        ),
                    });
                    continue;
                }
                EvacuationStrategy::LiveOnly | EvacuationStrategy::PreferLive
                    if workload.live_migratable =>
                {
                    MigrationMode::Live
                }
                EvacuationStrategy::PreferLive | EvacuationStrategy::ColdOnly => {
                    MigrationMode::Cold
                }
                // The remaining LiveOnly case is handled by the first arm.
                EvacuationStrategy::LiveOnly => unreachable!(),
            };

            if workload.pinned_host_id.as_deref() == Some(request.source_host_id.as_str())
                && !request.policy.allow_pinned
            {
                blockers.push(EvacuationBlocker {
                    code: BlockerCode::PinnedToSource,
                    vm_id: Some(workload.vm_id.clone()),
                    message: format!(
                        "workload {} is pinned to source host {}; set allow_pinned=true to override",
                        workload.vm_id, request.source_host_id
                    ),
                });
                continue;
            }

            let pinned_target = workload
                .pinned_host_id
                .as_deref()
                .filter(|host_id| *host_id != request.source_host_id);

            let mut best_index: Option<usize> = None;
            let mut matching_target_seen = false;

            for (index, target) in targets.iter().enumerate() {
                if let Some(pinned) = pinned_target {
                    if target.host_id != pinned {
                        continue;
                    }
                }

                if !labels_match(&target.labels, &workload.required_labels) {
                    continue;
                }

                matching_target_seen = true;

                if target.free_cpus < workload.cpus || target.free_memory_mb < workload.memory_mb {
                    continue;
                }

                match best_index {
                    None => best_index = Some(index),
                    Some(current_index) => {
                        let current = &targets[current_index];
                        // Spread evacuations toward the host with the most
                        // remaining memory, then CPU. host_id is the stable
                        // tie-breaker (ascending because targets are sorted).
                        let target_is_better = target.free_memory_mb > current.free_memory_mb
                            || (target.free_memory_mb == current.free_memory_mb
                                && target.free_cpus > current.free_cpus);
                        if target_is_better {
                            best_index = Some(index);
                        }
                    }
                }
            }

            let Some(index) = best_index else {
                let (code, message) = if pinned_target.is_some() && !matching_target_seen {
                    (
                        BlockerCode::PinnedTargetUnavailable,
                        format!(
                            "pinned target {} for workload {} is not eligible or does not satisfy labels",
                            pinned_target.unwrap_or_default(),
                            workload.vm_id
                        ),
                    )
                } else if targets.is_empty() || !matching_target_seen {
                    (
                        BlockerCode::NoEligibleTarget,
                        format!(
                            "no eligible evacuation target for workload {}",
                            workload.vm_id
                        ),
                    )
                } else {
                    (
                        BlockerCode::InsufficientCapacity,
                        format!(
                            "eligible targets do not have {} CPU(s) and {} MiB available for workload {} after reserve",
                            workload.cpus, workload.memory_mb, workload.vm_id
                        ),
                    )
                };

                blockers.push(EvacuationBlocker {
                    code,
                    vm_id: Some(workload.vm_id.clone()),
                    message,
                });
                continue;
            };

            let target = &mut targets[index];
            target.free_cpus -= workload.cpus;
            target.free_memory_mb -= workload.memory_mb;

            assignments.push(EvacuationAssignment {
                vm_id: workload.vm_id,
                source_host_id: request.source_host_id.clone(),
                target_host_id: target.host_id.clone(),
                cpus: workload.cpus,
                memory_mb: workload.memory_mb,
                mode,
                status: AssignmentStatus::Planned,
                error: None,
            });
        }

        Ok(EvacuationPlan {
            id: Uuid::new_v4().to_string(),
            source_host_id: request.source_host_id.clone(),
            source_cluster_id: request.source_cluster_id.clone(),
            policy: request.policy.clone(),
            assignments,
            blockers,
            created_at: Utc::now(),
        })
    }
}

fn validate_policy(policy: &EvacuationPolicy) -> Result<(), HostLifecycleError> {
    if policy.max_parallel == 0 {
        return Err(HostLifecycleError::InvalidPolicy(
            "max_parallel must be at least 1".to_string(),
        ));
    }
    if policy.reserve_cpu_percent >= 100 {
        return Err(HostLifecycleError::InvalidPolicy(
            "reserve_cpu_percent must be between 0 and 99".to_string(),
        ));
    }
    if policy.reserve_memory_percent >= 100 {
        return Err(HostLifecycleError::InvalidPolicy(
            "reserve_memory_percent must be between 0 and 99".to_string(),
        ));
    }
    Ok(())
}

fn capacity_after_reserve_u32(value: u32, reserve_percent: u8) -> u32 {
    value.saturating_mul(u32::from(100 - reserve_percent)) / 100
}

fn capacity_after_reserve_u64(value: u64, reserve_percent: u8) -> u64 {
    value.saturating_mul(u64::from(100 - reserve_percent)) / 100
}

fn labels_match(
    target_labels: &BTreeMap<String, String>,
    required_labels: &BTreeMap<String, String>,
) -> bool {
    required_labels
        .iter()
        .all(|(key, value)| target_labels.get(key) == Some(value))
}

/// Backend contract used by the orchestration state machine.
///
/// Implementations should make these calls idempotent wherever the backend
/// permits it. In particular, cordon/maintenance operations may be retried by
/// an API layer after a process restart.
#[async_trait]
pub trait EvacuationExecutor: Send + Sync {
    /// Prevent the scheduler from placing new workloads on the source host.
    async fn cordon_host(&self, host_id: &str) -> Result<(), String>;

    /// Execute a single migration assignment.
    async fn migrate(&self, assignment: &EvacuationAssignment) -> Result<(), String>;

    /// Put the already-evacuated host into maintenance mode.
    async fn enter_maintenance(&self, host_id: &str) -> Result<(), String>;

    /// Take the host out of maintenance mode.
    async fn exit_maintenance(&self, host_id: &str) -> Result<(), String>;

    /// Allow new scheduling onto the host again.
    async fn uncordon_host(&self, host_id: &str) -> Result<(), String>;
}

/// Thread-safe in-memory job manager.
///
/// The API layer can persist serialized `MaintenanceJob` records in
/// `StateStore`; this manager owns the transition rules and execution logic.
#[derive(Clone)]
pub struct HostLifecycleManager {
    jobs: Arc<RwLock<HashMap<String, MaintenanceJob>>>,
}

impl HostLifecycleManager {
    pub fn new() -> Self {
        Self {
            jobs: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Plan maintenance and create a new job.
    ///
    /// At most one non-terminal job may exist for a source host. A blocked job
    /// is considered active until it is cancelled, which prevents operators
    /// from accidentally creating multiple competing plans for the same host.
    pub fn create_job(
        &self,
        request: MaintenanceRequest,
    ) -> Result<MaintenanceJob, HostLifecycleError> {
        let plan = EvacuationPlanner::plan(&request)?;
        let mut jobs = self.jobs.write().unwrap_or_else(|error| error.into_inner());

        if jobs
            .values()
            .any(|job| job.source_host_id == request.source_host_id && !is_terminal(job.status))
        {
            return Err(HostLifecycleError::ActiveJobExists(request.source_host_id));
        }

        let now = Utc::now();
        let job = MaintenanceJob {
            id: Uuid::new_v4().to_string(),
            source_host_id: request.source_host_id,
            status: if plan.blockers.is_empty() {
                MaintenanceJobStatus::Planned
            } else {
                MaintenanceJobStatus::Blocked
            },
            plan,
            created_at: now,
            started_at: None,
            maintenance_at: None,
            completed_at: None,
            error: None,
        };

        tracing::info!(
            job_id = %job.id,
            host_id = %job.source_host_id,
            status = ?job.status,
            assignments = job.plan.assignments.len(),
            blockers = job.plan.blockers.len(),
            "created host maintenance job"
        );

        jobs.insert(job.id.clone(), job.clone());
        Ok(job)
    }

    pub fn get_job(&self, id: &str) -> Option<MaintenanceJob> {
        let jobs = self.jobs.read().unwrap_or_else(|error| error.into_inner());
        jobs.get(id).cloned()
    }

    pub fn list_jobs(&self, source_host_id: Option<&str>) -> Vec<MaintenanceJob> {
        let jobs = self.jobs.read().unwrap_or_else(|error| error.into_inner());
        let mut values: Vec<MaintenanceJob> = jobs
            .values()
            .filter(|job| source_host_id.is_none_or(|host_id| job.source_host_id == host_id))
            .cloned()
            .collect();
        values.sort_by_key(|job| std::cmp::Reverse(job.created_at));
        values
    }

    /// Cancel a job that has not started execution.
    pub fn cancel_job(&self, id: &str) -> Result<MaintenanceJob, HostLifecycleError> {
        let mut jobs = self.jobs.write().unwrap_or_else(|error| error.into_inner());
        let job = jobs
            .get_mut(id)
            .ok_or_else(|| HostLifecycleError::JobNotFound(id.to_string()))?;

        match job.status {
            MaintenanceJobStatus::Planned | MaintenanceJobStatus::Blocked => {
                job.status = MaintenanceJobStatus::Cancelled;
                job.completed_at = Some(Utc::now());
                tracing::info!(job_id = %id, "cancelled host maintenance job");
                Ok(job.clone())
            }
            status => Err(HostLifecycleError::InvalidState {
                job_id: id.to_string(),
                message: format!("cannot cancel from {status:?}"),
            }),
        }
    }

    /// Execute a planned maintenance job.
    ///
    /// The source host is cordoned before any migration starts. Migrations run
    /// with `policy.max_parallel` bounded concurrency. On the first migration
    /// error, pending work is cancelled by dropping the stream and the host is
    /// deliberately left cordoned; automatically uncordoning after a partial
    /// evacuation can create split placement and race with operator recovery.
    pub async fn execute_job<E: EvacuationExecutor>(
        &self,
        id: &str,
        executor: &E,
    ) -> Result<MaintenanceJob, HostLifecycleError> {
        let snapshot = self
            .get_job(id)
            .ok_or_else(|| HostLifecycleError::JobNotFound(id.to_string()))?;

        match snapshot.status {
            MaintenanceJobStatus::Blocked => {
                return Err(HostLifecycleError::JobBlocked {
                    job_id: id.to_string(),
                    blocker_count: snapshot.plan.blockers.len(),
                });
            }
            MaintenanceJobStatus::Planned => {}
            status => {
                return Err(HostLifecycleError::InvalidState {
                    job_id: id.to_string(),
                    message: format!("cannot execute from {status:?}"),
                });
            }
        }

        if snapshot.plan.policy.dry_run {
            return Err(HostLifecycleError::DryRun(id.to_string()));
        }

        self.set_job_running(id)?;

        if let Err(message) = executor.cordon_host(&snapshot.source_host_id).await {
            return self.execution_failure(id, format!("failed to cordon source host: {message}"));
        }

        let manager = self.clone();
        let job_id = id.to_string();
        let max_parallel = snapshot.plan.policy.max_parallel as usize;

        let migrations = stream::iter(snapshot.plan.assignments.clone())
            .map(|assignment| {
                let manager = manager.clone();
                let job_id = job_id.clone();
                async move {
                    manager.set_assignment_status(
                        &job_id,
                        &assignment.vm_id,
                        AssignmentStatus::Running,
                        None,
                    )?;

                    let result = executor.migrate(&assignment).await;
                    Ok::<_, HostLifecycleError>((assignment.vm_id, result))
                }
            })
            .buffer_unordered(max_parallel);

        futures::pin_mut!(migrations);
        while let Some(result) = migrations.next().await {
            let (vm_id, migration_result) = result?;
            match migration_result {
                Ok(()) => {
                    self.set_assignment_status(id, &vm_id, AssignmentStatus::Completed, None)?;
                }
                Err(message) => {
                    self.set_assignment_status(
                        id,
                        &vm_id,
                        AssignmentStatus::Failed,
                        Some(message.clone()),
                    )?;
                    return self.execution_failure(
                        id,
                        format!("migration of workload {vm_id} failed: {message}"),
                    );
                }
            }
        }

        if let Err(message) = executor.enter_maintenance(&snapshot.source_host_id).await {
            return self.execution_failure(
                id,
                format!("workloads evacuated but entering maintenance failed: {message}"),
            );
        }

        let mut jobs = self.jobs.write().unwrap_or_else(|error| error.into_inner());
        let job = jobs
            .get_mut(id)
            .ok_or_else(|| HostLifecycleError::JobNotFound(id.to_string()))?;
        job.status = MaintenanceJobStatus::Maintenance;
        job.maintenance_at = Some(Utc::now());
        job.error = None;

        tracing::info!(
            job_id = %id,
            host_id = %job.source_host_id,
            "host evacuation completed; host entered maintenance"
        );

        Ok(job.clone())
    }

    /// Return a host to service after operator maintenance is finished.
    pub async fn complete_maintenance<E: EvacuationExecutor>(
        &self,
        id: &str,
        executor: &E,
    ) -> Result<MaintenanceJob, HostLifecycleError> {
        let snapshot = self
            .get_job(id)
            .ok_or_else(|| HostLifecycleError::JobNotFound(id.to_string()))?;

        if snapshot.status != MaintenanceJobStatus::Maintenance {
            return Err(HostLifecycleError::InvalidState {
                job_id: id.to_string(),
                message: format!("cannot complete maintenance from {:?}", snapshot.status),
            });
        }

        if let Err(message) = executor.exit_maintenance(&snapshot.source_host_id).await {
            return self
                .execution_failure(id, format!("failed to exit maintenance mode: {message}"));
        }

        if let Err(message) = executor.uncordon_host(&snapshot.source_host_id).await {
            return self.execution_failure(
                id,
                format!("maintenance exited but failed to uncordon host: {message}"),
            );
        }

        let mut jobs = self.jobs.write().unwrap_or_else(|error| error.into_inner());
        let job = jobs
            .get_mut(id)
            .ok_or_else(|| HostLifecycleError::JobNotFound(id.to_string()))?;
        job.status = MaintenanceJobStatus::Completed;
        job.completed_at = Some(Utc::now());
        job.error = None;

        tracing::info!(
            job_id = %id,
            host_id = %job.source_host_id,
            "host maintenance completed; host returned to service"
        );

        Ok(job.clone())
    }

    fn set_job_running(&self, id: &str) -> Result<(), HostLifecycleError> {
        let mut jobs = self.jobs.write().unwrap_or_else(|error| error.into_inner());
        let job = jobs
            .get_mut(id)
            .ok_or_else(|| HostLifecycleError::JobNotFound(id.to_string()))?;

        if job.status != MaintenanceJobStatus::Planned {
            return Err(HostLifecycleError::InvalidState {
                job_id: id.to_string(),
                message: format!("cannot start from {:?}", job.status),
            });
        }

        job.status = MaintenanceJobStatus::Running;
        job.started_at = Some(Utc::now());
        job.error = None;
        Ok(())
    }

    fn set_assignment_status(
        &self,
        job_id: &str,
        vm_id: &str,
        status: AssignmentStatus,
        error: Option<String>,
    ) -> Result<(), HostLifecycleError> {
        let mut jobs = self
            .jobs
            .write()
            .unwrap_or_else(|lock_error| lock_error.into_inner());
        let job = jobs
            .get_mut(job_id)
            .ok_or_else(|| HostLifecycleError::JobNotFound(job_id.to_string()))?;
        let assignment = job
            .plan
            .assignments
            .iter_mut()
            .find(|assignment| assignment.vm_id == vm_id)
            .ok_or_else(|| HostLifecycleError::AssignmentNotFound {
                job_id: job_id.to_string(),
                vm_id: vm_id.to_string(),
            })?;

        assignment.status = status;
        assignment.error = error;
        Ok(())
    }

    fn execution_failure<T>(&self, id: &str, message: String) -> Result<T, HostLifecycleError> {
        let mut jobs = self.jobs.write().unwrap_or_else(|error| error.into_inner());
        if let Some(job) = jobs.get_mut(id) {
            job.status = MaintenanceJobStatus::Failed;
            job.error = Some(message.clone());
            job.completed_at = Some(Utc::now());
        }

        tracing::error!(job_id = %id, error = %message, "host maintenance execution failed");
        Err(HostLifecycleError::ExecutionFailed {
            job_id: id.to_string(),
            message,
        })
    }
}

impl Default for HostLifecycleManager {
    fn default() -> Self {
        Self::new()
    }
}

fn is_terminal(status: MaintenanceJobStatus) -> bool {
    matches!(
        status,
        MaintenanceJobStatus::Completed
            | MaintenanceJobStatus::Cancelled
            | MaintenanceJobStatus::Failed
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    fn host(id: &str, free_cpus: u32, free_memory_mb: u64) -> HostCandidate {
        HostCandidate::new(
            id,
            "cluster-a",
            HostStatus::Connected,
            free_cpus,
            free_memory_mb,
        )
    }

    fn workload(id: &str, cpus: u32, memory_mb: u64, live: bool) -> Workload {
        Workload {
            vm_id: id.to_string(),
            source_host_id: "host-a".to_string(),
            cpus,
            memory_mb,
            live_migratable: live,
            pinned_host_id: None,
            required_labels: BTreeMap::new(),
        }
    }

    fn request(workloads: Vec<Workload>, targets: Vec<HostCandidate>) -> MaintenanceRequest {
        MaintenanceRequest {
            source_host_id: "host-a".to_string(),
            source_cluster_id: "cluster-a".to_string(),
            source_status: HostStatus::Connected,
            workloads,
            targets,
            policy: EvacuationPolicy {
                reserve_cpu_percent: 0,
                reserve_memory_percent: 0,
                ..EvacuationPolicy::default()
            },
        }
    }

    #[test]
    fn plans_largest_first_and_spreads_across_capacity() {
        let req = request(
            vec![
                workload("vm-small", 2, 2_048, true),
                workload("vm-large", 4, 8_192, true),
            ],
            vec![host("host-b", 8, 12_288), host("host-c", 8, 10_240)],
        );

        let plan = EvacuationPlanner::plan(&req).unwrap();
        assert!(plan.blockers.is_empty());
        assert_eq!(plan.assignments.len(), 2);

        let large = plan
            .assignments
            .iter()
            .find(|assignment| assignment.vm_id == "vm-large")
            .unwrap();
        let small = plan
            .assignments
            .iter()
            .find(|assignment| assignment.vm_id == "vm-small")
            .unwrap();

        assert_eq!(large.target_host_id, "host-b");
        assert_eq!(small.target_host_id, "host-c");
        assert_eq!(large.mode, MigrationMode::Live);
    }

    #[test]
    fn live_only_blocks_non_live_workload() {
        let mut req = request(
            vec![workload("legacy-vm", 2, 2_048, false)],
            vec![host("host-b", 8, 16_384)],
        );
        req.policy.strategy = EvacuationStrategy::LiveOnly;

        let plan = EvacuationPlanner::plan(&req).unwrap();
        assert!(plan.assignments.is_empty());
        assert_eq!(plan.blockers.len(), 1);
        assert_eq!(plan.blockers[0].code, BlockerCode::LiveMigrationRequired);
    }

    #[test]
    fn pinned_source_blocks_by_default() {
        let mut vm = workload("db-1", 4, 8_192, true);
        vm.pinned_host_id = Some("host-a".to_string());

        let plan =
            EvacuationPlanner::plan(&request(vec![vm], vec![host("host-b", 16, 32_768)])).unwrap();

        assert!(plan.assignments.is_empty());
        assert_eq!(plan.blockers[0].code, BlockerCode::PinnedToSource);
    }

    #[test]
    fn label_constraint_is_enforced() {
        let mut vm = workload("gpu-vm", 4, 8_192, true);
        vm.required_labels
            .insert("gpu".to_string(), "nvidia".to_string());

        let mut wrong = host("host-b", 16, 32_768);
        wrong.labels.insert("gpu".to_string(), "amd".to_string());
        let mut right = host("host-c", 16, 32_768);
        right.labels.insert("gpu".to_string(), "nvidia".to_string());

        let plan = EvacuationPlanner::plan(&request(vec![vm], vec![wrong, right])).unwrap();
        assert!(plan.blockers.is_empty());
        assert_eq!(plan.assignments[0].target_host_id, "host-c");
    }

    #[test]
    fn reserve_capacity_can_block_an_otherwise_fitting_vm() {
        let mut req = request(
            vec![workload("vm-1", 8, 8_000, true)],
            vec![host("host-b", 8, 8_000)],
        );
        req.policy.reserve_cpu_percent = 10;
        req.policy.reserve_memory_percent = 10;

        let plan = EvacuationPlanner::plan(&req).unwrap();
        assert!(plan.assignments.is_empty());
        assert_eq!(plan.blockers[0].code, BlockerCode::InsufficientCapacity);
    }

    #[test]
    fn rejects_invalid_parallelism() {
        let mut req = request(Vec::new(), Vec::new());
        req.policy.max_parallel = 0;

        let err = EvacuationPlanner::plan(&req).unwrap_err();
        assert_eq!(
            err,
            HostLifecycleError::InvalidPolicy("max_parallel must be at least 1".to_string())
        );
    }

    struct FakeExecutor {
        calls: Mutex<Vec<String>>,
        fail_vm: Option<String>,
    }

    impl FakeExecutor {
        fn success() -> Self {
            Self {
                calls: Mutex::new(Vec::new()),
                fail_vm: None,
            }
        }

        fn failing(vm_id: &str) -> Self {
            Self {
                calls: Mutex::new(Vec::new()),
                fail_vm: Some(vm_id.to_string()),
            }
        }

        fn calls(&self) -> Vec<String> {
            self.calls
                .lock()
                .unwrap_or_else(|error| error.into_inner())
                .clone()
        }

        fn record(&self, call: String) {
            self.calls
                .lock()
                .unwrap_or_else(|error| error.into_inner())
                .push(call);
        }
    }

    #[async_trait]
    impl EvacuationExecutor for FakeExecutor {
        async fn cordon_host(&self, host_id: &str) -> Result<(), String> {
            self.record(format!("cordon:{host_id}"));
            Ok(())
        }

        async fn migrate(&self, assignment: &EvacuationAssignment) -> Result<(), String> {
            self.record(format!(
                "migrate:{}:{}:{:?}",
                assignment.vm_id, assignment.target_host_id, assignment.mode
            ));
            if self.fail_vm.as_deref() == Some(assignment.vm_id.as_str()) {
                Err("simulated migration failure".to_string())
            } else {
                Ok(())
            }
        }

        async fn enter_maintenance(&self, host_id: &str) -> Result<(), String> {
            self.record(format!("maintenance-enter:{host_id}"));
            Ok(())
        }

        async fn exit_maintenance(&self, host_id: &str) -> Result<(), String> {
            self.record(format!("maintenance-exit:{host_id}"));
            Ok(())
        }

        async fn uncordon_host(&self, host_id: &str) -> Result<(), String> {
            self.record(format!("uncordon:{host_id}"));
            Ok(())
        }
    }

    #[tokio::test]
    async fn executes_job_and_returns_host_to_service() {
        let manager = HostLifecycleManager::new();
        let job = manager
            .create_job(request(
                vec![
                    workload("vm-1", 2, 2_048, true),
                    workload("vm-2", 2, 2_048, false),
                ],
                vec![host("host-b", 8, 16_384)],
            ))
            .unwrap();
        let executor = FakeExecutor::success();

        let maintenance = manager.execute_job(&job.id, &executor).await.unwrap();
        assert_eq!(maintenance.status, MaintenanceJobStatus::Maintenance);
        assert!(maintenance
            .plan
            .assignments
            .iter()
            .all(|assignment| assignment.status == AssignmentStatus::Completed));

        let completed = manager
            .complete_maintenance(&job.id, &executor)
            .await
            .unwrap();
        assert_eq!(completed.status, MaintenanceJobStatus::Completed);

        let calls = executor.calls();
        assert_eq!(calls.first().map(String::as_str), Some("cordon:host-a"));
        assert!(calls.contains(&"maintenance-enter:host-a".to_string()));
        assert!(calls.contains(&"maintenance-exit:host-a".to_string()));
        assert_eq!(calls.last().map(String::as_str), Some("uncordon:host-a"));
    }

    #[tokio::test]
    async fn migration_failure_marks_job_failed_and_leaves_host_cordoned() {
        let manager = HostLifecycleManager::new();
        let job = manager
            .create_job(request(
                vec![workload("vm-1", 2, 2_048, true)],
                vec![host("host-b", 8, 16_384)],
            ))
            .unwrap();
        let executor = FakeExecutor::failing("vm-1");

        let result = manager.execute_job(&job.id, &executor).await;
        assert!(matches!(
            result,
            Err(HostLifecycleError::ExecutionFailed { .. })
        ));

        let failed = manager.get_job(&job.id).unwrap();
        assert_eq!(failed.status, MaintenanceJobStatus::Failed);
        assert_eq!(failed.plan.assignments[0].status, AssignmentStatus::Failed);
        assert!(!executor
            .calls()
            .iter()
            .any(|call| call.starts_with("uncordon:")));
    }

    #[test]
    fn blocks_second_active_job_for_same_host() {
        let manager = HostLifecycleManager::new();
        manager.create_job(request(Vec::new(), Vec::new())).unwrap();

        let err = manager
            .create_job(request(Vec::new(), Vec::new()))
            .unwrap_err();
        assert_eq!(
            err,
            HostLifecycleError::ActiveJobExists("host-a".to_string())
        );
    }

    #[test]
    fn blocked_job_can_be_cancelled() {
        let manager = HostLifecycleManager::new();
        let mut req = request(
            vec![workload("vm-1", 2, 2_048, false)],
            vec![host("host-b", 8, 16_384)],
        );
        req.policy.strategy = EvacuationStrategy::LiveOnly;

        let job = manager.create_job(req).unwrap();
        assert_eq!(job.status, MaintenanceJobStatus::Blocked);

        let cancelled = manager.cancel_job(&job.id).unwrap();
        assert_eq!(cancelled.status, MaintenanceJobStatus::Cancelled);
    }
}
