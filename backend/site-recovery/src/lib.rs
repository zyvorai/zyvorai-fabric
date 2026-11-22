// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: LicenseRef-Zyvor-Production-2.0

use anyhow::{bail, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use uuid::Uuid;

// ---------------------------------------------------------------------------
// Enums
// ---------------------------------------------------------------------------

/// Status of a recovery plan.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PlanStatus {
    Ready,
    InProgress,
    Completed,
    Failed,
    Testing,
}

/// Type of recovery script hook.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ScriptType {
    PreRecovery,
    PostRecovery,
    PreGroup,
    PostGroup,
}

/// Type of recovery execution.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExecutionType {
    PlannedMigration,
    DisasterRecovery,
    TestFailover,
    Reprotect,
}

/// Status of a recovery execution.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExecutionStatus {
    Running,
    Completed,
    Failed,
    Cancelled,
    RollingBack,
}

/// Type of an individual recovery step.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StepType {
    PowerOff,
    Sync,
    Recover,
    PowerOn,
    RunScript,
    NetworkConfig,
    WaitDelay,
}

/// Status of an individual recovery step.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StepStatus {
    Pending,
    Running,
    Completed,
    Failed,
    Skipped,
}

/// Overall DR health indicator.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DrHealth {
    Healthy,
    Warning,
    Critical,
}

// ---------------------------------------------------------------------------
// Data models
// ---------------------------------------------------------------------------

/// A script to run during recovery (pre/post hooks).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecoveryScript {
    pub name: String,
    pub script_type: ScriptType,
    pub command: String,
    #[serde(default = "default_script_timeout")]
    pub timeout_secs: u32,
}

fn default_script_timeout() -> u32 {
    300
}

/// Mapping from source to target network, with optional isolated test network.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetworkMapping {
    pub source_network: String,
    pub target_network: String,
    pub test_network: Option<String>,
}

/// An ordered group of VMs recovered together at the same priority level.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PriorityGroup {
    /// Priority level (1 = highest, recovered first).
    pub priority: u32,
    pub name: String,
    pub vm_names: Vec<String>,
    /// Seconds to wait after this group starts before proceeding to next group.
    pub startup_delay_secs: u32,
    pub pre_action: Option<RecoveryScript>,
    pub post_action: Option<RecoveryScript>,
}

/// A disaster recovery plan defining how VMs failover between sites.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecoveryPlan {
    pub id: String,
    pub name: String,
    pub description: Option<String>,
    pub source_site_id: String,
    pub target_site_id: String,
    pub priority_groups: Vec<PriorityGroup>,
    pub pre_scripts: Vec<RecoveryScript>,
    pub post_scripts: Vec<RecoveryScript>,
    pub network_mappings: Vec<NetworkMapping>,
    /// Recovery Time Objective target in minutes.
    pub rto_minutes: Option<u32>,
    pub last_tested: Option<DateTime<Utc>>,
    pub last_executed: Option<DateTime<Utc>>,
    pub status: PlanStatus,
    pub created: DateTime<Utc>,
    pub updated: Option<DateTime<Utc>>,
}

/// An individual step within a recovery execution.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecoveryStep {
    pub step_number: u32,
    pub description: String,
    pub vm_name: Option<String>,
    pub step_type: StepType,
    pub status: StepStatus,
    pub started: Option<DateTime<Utc>>,
    pub completed: Option<DateTime<Utc>>,
    pub error: Option<String>,
    /// Additional context for step execution (e.g. script command, delay
    /// seconds, target site id for sync operations).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub details: Option<String>,
}

/// A running or completed recovery execution.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecoveryExecution {
    pub id: String,
    pub plan_id: String,
    pub plan_name: String,
    pub execution_type: ExecutionType,
    pub status: ExecutionStatus,
    pub started: DateTime<Utc>,
    pub completed: Option<DateTime<Utc>>,
    pub steps: Vec<RecoveryStep>,
    pub rto_actual_minutes: Option<u32>,
    pub vms_recovered: u32,
    pub vms_failed: u32,
    pub error: Option<String>,
    pub initiated_by: String,
}

/// Result of a past test failover.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TestResult {
    pub plan_id: String,
    pub plan_name: String,
    pub tested_at: DateTime<Utc>,
    pub success: bool,
    pub duration_secs: u64,
    pub vms_tested: u32,
    pub vms_failed: u32,
}

/// High-level DR dashboard summary.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DrDashboard {
    pub total_plans: u32,
    pub ready_plans: u32,
    pub failed_plans: u32,
    #[serde(rename = "total_protected_vms")]
    pub protected_vms: u32,
    #[serde(rename = "total_unprotected_vms")]
    pub unprotected_vms: u32,
    pub rpo_violations: u32,
    pub last_test_results: Vec<TestResult>,
    pub overall_health: DrHealth,
    /// Count of plans with at least one recorded test run (`last_tested`).
    pub plans_tested: u32,
    pub plans_untested: u32,
    /// Average of each plan's `rto_minutes` target, in seconds (the unit
    /// the frontend's RTO gauge displays), across plans that set one.
    pub avg_rto_seconds: f64,
    /// RPO is a replication-layer concept (see `replication::ReplicationHealthSummary`)
    /// with no equivalent on a recovery plan — always 0 here rather than a
    /// fabricated number until site-recovery plans track their own RPO target.
    pub avg_rpo_minutes: f64,
    pub sites: Vec<DrSiteHealth>,
    pub recent_executions: Vec<RecoveryExecution>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DrSiteHealth {
    pub site_id: String,
    pub site_name: String,
    pub status: String,
    pub protected_vms: u32,
    pub plans: u32,
}

// ---------------------------------------------------------------------------
// Error type
// ---------------------------------------------------------------------------

#[derive(Debug, thiserror::Error)]
pub enum SiteRecoveryError {
    #[error("recovery plan not found: {0}")]
    PlanNotFound(String),
    #[error("execution not found: {0}")]
    ExecutionNotFound(String),
    #[error("plan is not in ready state: {0}")]
    PlanNotReady(String),
    #[error("execution is not running: {0}")]
    ExecutionNotRunning(String),
    #[error("step {0} not found in execution {1}")]
    StepNotFound(u32, String),
}

// ---------------------------------------------------------------------------
// Script path validation
// ---------------------------------------------------------------------------

/// Allowed directory prefixes for recovery scripts.  Scripts must be
/// absolute paths under one of these directories.  This prevents arbitrary
/// command injection -- only pre-validated admin-defined scripts located in
/// trusted directories are permitted.
const ALLOWED_SCRIPT_PREFIXES: &[&str] = &[
    "/usr/local/bin/",
    "/usr/local/sbin/",
    "/var/lib/zyvor-fabricd/scripts/",
    "/opt/zyvor-fabricd/scripts/",
];

/// Validate that a script command starts with an allowed absolute path.
///
/// The command may contain arguments (e.g. "/usr/local/bin/notify.sh start"),
/// so we extract the first whitespace-delimited token and validate it.
/// Path traversal components (`..`) are rejected outright.
fn validate_script_path(command: &str) -> Result<()> {
    let script_path = command.split_whitespace().next().unwrap_or("");

    if script_path.is_empty() {
        bail!("script command is empty");
    }

    if !script_path.starts_with('/') {
        bail!(
            "script path must be absolute (starts with /), got: {}",
            script_path
        );
    }

    // Reject path traversal.
    if script_path.contains("..") {
        bail!(
            "script path must not contain '..' components: {}",
            script_path
        );
    }

    if !ALLOWED_SCRIPT_PREFIXES
        .iter()
        .any(|prefix| script_path.starts_with(prefix))
    {
        bail!(
            "script path '{}' is not under an allowed directory ({})",
            script_path,
            ALLOWED_SCRIPT_PREFIXES.join(", ")
        );
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// SiteRecoveryManager
// ---------------------------------------------------------------------------

/// Thread-safe manager for disaster recovery plans and executions.
///
/// Stores plans, executions, and test results in `HashMap`s behind
/// `Arc<RwLock<...>>` so the manager can be shared across threads.
#[derive(Clone)]
pub struct SiteRecoveryManager {
    plans: Arc<RwLock<HashMap<String, RecoveryPlan>>>,
    executions: Arc<RwLock<HashMap<String, RecoveryExecution>>>,
    test_results: Arc<RwLock<Vec<TestResult>>>,
    /// VM driver used by `execute_step`'s `PowerOff`/`PowerOn` steps. `None`
    /// for every other method (plan/execution bookkeeping needs no driver) —
    /// `run_execution` itself requires one, see `with_driver`.
    driver: Option<Arc<dyn zyvor_fabric_driver_core::VmDriver>>,
}

impl SiteRecoveryManager {
    /// Create a new, empty manager with no driver (sufficient for
    /// everything except `run_execution`).
    pub fn new() -> Self {
        Self {
            plans: Arc::new(RwLock::new(HashMap::new())),
            executions: Arc::new(RwLock::new(HashMap::new())),
            test_results: Arc::new(RwLock::new(Vec::new())),
            driver: None,
        }
    }

    /// Attach the VM driver `run_execution` uses for `PowerOff`/`PowerOn`
    /// recovery steps.
    pub fn with_driver(mut self, driver: Arc<dyn zyvor_fabric_driver_core::VmDriver>) -> Self {
        self.driver = Some(driver);
        self
    }

    // -- Plan CRUD ----------------------------------------------------------

    /// Create a new recovery plan. The plan's `id`, `created`, and `status`
    /// fields are set automatically.
    pub fn create_plan(&self, mut plan: RecoveryPlan) -> Result<RecoveryPlan> {
        let mut plans = self.plans.write().unwrap_or_else(|e| e.into_inner());

        plan.id = Uuid::new_v4().to_string();
        plan.created = Utc::now();
        plan.updated = None;
        plan.status = PlanStatus::Ready;

        // Sort priority groups by priority (ascending = highest priority first).
        plan.priority_groups.sort_by_key(|g| g.priority);

        tracing::info!(plan_id = %plan.id, name = %plan.name, "created recovery plan");
        plans.insert(plan.id.clone(), plan.clone());

        Ok(plan)
    }

    /// Get a single plan by id.
    pub fn get_plan(&self, id: &str) -> Option<RecoveryPlan> {
        let plans = self.plans.read().unwrap_or_else(|e| e.into_inner());
        plans.get(id).cloned()
    }

    /// List all recovery plans.
    pub fn list_plans(&self) -> Vec<RecoveryPlan> {
        let plans = self.plans.read().unwrap_or_else(|e| e.into_inner());
        plans.values().cloned().collect()
    }

    /// Replace a plan with an updated version. Preserves `id` and `created`.
    pub fn update_plan(&self, id: &str, mut plan: RecoveryPlan) -> Result<RecoveryPlan> {
        let mut plans = self.plans.write().unwrap_or_else(|e| e.into_inner());
        let existing = plans
            .get(id)
            .ok_or_else(|| SiteRecoveryError::PlanNotFound(id.to_string()))?;

        plan.id = existing.id.clone();
        plan.created = existing.created;
        plan.updated = Some(Utc::now());
        plan.priority_groups.sort_by_key(|g| g.priority);

        tracing::info!(plan_id = %id, name = %plan.name, "updated recovery plan");
        plans.insert(id.to_string(), plan.clone());

        Ok(plan)
    }

    /// Delete a recovery plan.
    pub fn delete_plan(&self, id: &str) -> Result<()> {
        let mut plans = self.plans.write().unwrap_or_else(|e| e.into_inner());
        if plans.remove(id).is_none() {
            bail!(SiteRecoveryError::PlanNotFound(id.to_string()));
        }
        tracing::info!(plan_id = %id, "deleted recovery plan");
        Ok(())
    }

    // -- Execution ----------------------------------------------------------

    /// Execute a planned (graceful) migration. Source VMs are powered off and
    /// synced before recovery on the target site.
    pub fn execute_planned_migration(
        &self,
        plan_id: &str,
        initiated_by: &str,
    ) -> Result<RecoveryExecution> {
        self.start_execution(plan_id, initiated_by, ExecutionType::PlannedMigration)
    }

    /// Execute a disaster recovery (forced failover). No graceful shutdown of
    /// source VMs; uses the latest available replica.
    pub fn execute_disaster_recovery(
        &self,
        plan_id: &str,
        initiated_by: &str,
    ) -> Result<RecoveryExecution> {
        self.start_execution(plan_id, initiated_by, ExecutionType::DisasterRecovery)
    }

    /// Execute a non-disruptive test failover. Uses an isolated test network
    /// so the production environment is unaffected.
    pub fn execute_test_failover(
        &self,
        plan_id: &str,
        initiated_by: &str,
    ) -> Result<RecoveryExecution> {
        self.start_execution(plan_id, initiated_by, ExecutionType::TestFailover)
    }

    /// Execute a reprotect operation (reverse replication direction so the
    /// recovered site becomes the new primary).
    pub fn execute_reprotect(
        &self,
        plan_id: &str,
        initiated_by: &str,
    ) -> Result<RecoveryExecution> {
        self.start_execution(plan_id, initiated_by, ExecutionType::Reprotect)
    }

    /// Cancel a running execution.
    pub fn cancel_execution(&self, execution_id: &str) -> Result<()> {
        let mut executions = self.executions.write().unwrap_or_else(|e| e.into_inner());
        let exec = executions
            .get_mut(execution_id)
            .ok_or_else(|| SiteRecoveryError::ExecutionNotFound(execution_id.to_string()))?;

        if exec.status != ExecutionStatus::Running {
            bail!(SiteRecoveryError::ExecutionNotRunning(
                execution_id.to_string()
            ));
        }

        exec.status = ExecutionStatus::Cancelled;
        exec.completed = Some(Utc::now());

        // Mark remaining pending steps as skipped.
        for step in &mut exec.steps {
            if step.status == StepStatus::Pending {
                step.status = StepStatus::Skipped;
            }
        }

        // Restore plan status to ready.
        let plan_id = exec.plan_id.clone();
        drop(executions);

        let mut plans = self.plans.write().unwrap_or_else(|e| e.into_inner());
        if let Some(plan) = plans.get_mut(&plan_id) {
            plan.status = PlanStatus::Ready;
            plan.updated = Some(Utc::now());
        }

        tracing::info!(execution_id = %execution_id, "cancelled recovery execution");
        Ok(())
    }

    /// Get a single execution by id.
    pub fn get_execution(&self, id: &str) -> Option<RecoveryExecution> {
        let executions = self.executions.read().unwrap_or_else(|e| e.into_inner());
        executions.get(id).cloned()
    }

    /// List executions, optionally filtered by plan id.
    pub fn list_executions(&self, plan_id: Option<&str>) -> Vec<RecoveryExecution> {
        let executions = self.executions.read().unwrap_or_else(|e| e.into_inner());
        executions
            .values()
            .filter(|e| match plan_id {
                Some(pid) => e.plan_id == pid,
                None => true,
            })
            .cloned()
            .collect()
    }

    /// Update the status of a specific step within an execution.
    pub fn update_step_status(
        &self,
        execution_id: &str,
        step_number: u32,
        status: StepStatus,
        error: Option<String>,
    ) -> Result<()> {
        let mut executions = self.executions.write().unwrap_or_else(|e| e.into_inner());
        let exec = executions
            .get_mut(execution_id)
            .ok_or_else(|| SiteRecoveryError::ExecutionNotFound(execution_id.to_string()))?;

        let step = exec
            .steps
            .iter_mut()
            .find(|s| s.step_number == step_number)
            .ok_or_else(|| {
                SiteRecoveryError::StepNotFound(step_number, execution_id.to_string())
            })?;

        let now = Utc::now();

        match status {
            StepStatus::Running => {
                step.started = Some(now);
            }
            StepStatus::Completed | StepStatus::Failed | StepStatus::Skipped => {
                step.completed = Some(now);
                if step.started.is_none() {
                    step.started = Some(now);
                }
            }
            StepStatus::Pending => {}
        }

        step.status = status;
        step.error = error;

        // Update VM counters based on step outcomes.
        if step.step_type == StepType::PowerOn {
            match step.status {
                StepStatus::Completed => exec.vms_recovered += 1,
                StepStatus::Failed => exec.vms_failed += 1,
                _ => {}
            }
        }

        tracing::debug!(
            execution_id = %execution_id,
            step = step_number,
            "updated step status"
        );
        Ok(())
    }

    /// Mark an execution as completed (success or failure).
    pub fn complete_execution(&self, execution_id: &str, success: bool) -> Result<()> {
        let mut executions = self.executions.write().unwrap_or_else(|e| e.into_inner());
        let exec = executions
            .get_mut(execution_id)
            .ok_or_else(|| SiteRecoveryError::ExecutionNotFound(execution_id.to_string()))?;

        let now = Utc::now();
        exec.completed = Some(now);
        exec.status = if success {
            ExecutionStatus::Completed
        } else {
            ExecutionStatus::Failed
        };

        // Calculate actual RTO.
        let duration = now - exec.started;
        exec.rto_actual_minutes = Some(duration.num_minutes().max(0) as u32);

        if !success && exec.error.is_none() {
            exec.error = Some("execution completed with failures".to_string());
        }

        let plan_id = exec.plan_id.clone();
        let plan_name = exec.plan_name.clone();
        let exec_type = exec.execution_type.clone();
        let vms_recovered = exec.vms_recovered;
        let vms_failed = exec.vms_failed;

        drop(executions);

        // Update plan status and timestamps.
        let mut plans = self.plans.write().unwrap_or_else(|e| e.into_inner());
        if let Some(plan) = plans.get_mut(&plan_id) {
            plan.status = if success {
                PlanStatus::Completed
            } else {
                PlanStatus::Failed
            };
            plan.updated = Some(now);

            match exec_type {
                ExecutionType::TestFailover => {
                    plan.last_tested = Some(now);
                    // Record test result.
                    let result = TestResult {
                        plan_id: plan_id.clone(),
                        plan_name: plan_name.clone(),
                        tested_at: now,
                        success,
                        duration_secs: 0, // filled below
                        vms_tested: vms_recovered + vms_failed,
                        vms_failed,
                    };
                    drop(plans);
                    let mut results = self.test_results.write().unwrap_or_else(|e| e.into_inner());
                    results.push(result);
                }
                _ => {
                    plan.last_executed = Some(now);
                }
            }
        }

        tracing::info!(
            execution_id = %execution_id,
            success = success,
            "completed recovery execution"
        );
        Ok(())
    }

    // -- Step generation ----------------------------------------------------

    /// Generate an ordered list of recovery steps from a plan and execution
    /// type. Steps are ordered: pre-scripts, then per-group (pre-action,
    /// per-VM power-off/sync/recover/network-config/power-on, post-action,
    /// wait-delay), then post-scripts.
    ///
    /// Each step's `details` field carries the metadata needed for execution
    /// (e.g. the script command, delay duration, or target site id).
    pub fn generate_recovery_steps(
        plan: &RecoveryPlan,
        exec_type: ExecutionType,
    ) -> Vec<RecoveryStep> {
        let mut steps = Vec::new();
        let mut step_num: u32 = 1;

        // Pre-recovery scripts.
        for script in &plan.pre_scripts {
            steps.push(RecoveryStep {
                step_number: step_num,
                description: format!("Run pre-recovery script: {}", script.name),
                vm_name: None,
                step_type: StepType::RunScript,
                status: StepStatus::Pending,
                started: None,
                completed: None,
                error: None,
                details: Some(script.command.clone()),
            });
            step_num += 1;
        }

        // Process each priority group in order.
        let mut sorted_groups = plan.priority_groups.clone();
        sorted_groups.sort_by_key(|g| g.priority);

        for group in &sorted_groups {
            // Pre-group action.
            if let Some(ref pre) = group.pre_action {
                steps.push(RecoveryStep {
                    step_number: step_num,
                    description: format!("Run pre-group script for group '{}'", group.name),
                    vm_name: None,
                    step_type: StepType::RunScript,
                    status: StepStatus::Pending,
                    started: None,
                    completed: None,
                    error: None,
                    details: Some(pre.command.clone()),
                });
                step_num += 1;
            }

            // Per-VM steps.
            for vm_name in &group.vm_names {
                // For planned migration: power off source first.
                if exec_type == ExecutionType::PlannedMigration {
                    steps.push(RecoveryStep {
                        step_number: step_num,
                        description: format!("Power off source VM '{}'", vm_name),
                        vm_name: Some(vm_name.clone()),
                        step_type: StepType::PowerOff,
                        status: StepStatus::Pending,
                        started: None,
                        completed: None,
                        error: None,
                        details: None,
                    });
                    step_num += 1;

                    steps.push(RecoveryStep {
                        step_number: step_num,
                        description: format!("Final sync for VM '{}'", vm_name),
                        vm_name: Some(vm_name.clone()),
                        step_type: StepType::Sync,
                        status: StepStatus::Pending,
                        started: None,
                        completed: None,
                        error: None,
                        details: Some(plan.target_site_id.clone()),
                    });
                    step_num += 1;
                }

                // Recover VM on target site.
                steps.push(RecoveryStep {
                    step_number: step_num,
                    description: format!("Recover VM '{}' on target site", vm_name),
                    vm_name: Some(vm_name.clone()),
                    step_type: StepType::Recover,
                    status: StepStatus::Pending,
                    started: None,
                    completed: None,
                    error: None,
                    details: Some(plan.target_site_id.clone()),
                });
                step_num += 1;

                // Network configuration.
                if !plan.network_mappings.is_empty() {
                    steps.push(RecoveryStep {
                        step_number: step_num,
                        description: format!("Configure network for VM '{}'", vm_name),
                        vm_name: Some(vm_name.clone()),
                        step_type: StepType::NetworkConfig,
                        status: StepStatus::Pending,
                        started: None,
                        completed: None,
                        error: None,
                        details: None,
                    });
                    step_num += 1;
                }

                // Power on recovered VM.
                steps.push(RecoveryStep {
                    step_number: step_num,
                    description: format!("Power on recovered VM '{}'", vm_name),
                    vm_name: Some(vm_name.clone()),
                    step_type: StepType::PowerOn,
                    status: StepStatus::Pending,
                    started: None,
                    completed: None,
                    error: None,
                    details: None,
                });
                step_num += 1;
            }

            // Post-group action.
            if let Some(ref post) = group.post_action {
                steps.push(RecoveryStep {
                    step_number: step_num,
                    description: format!("Run post-group script for group '{}'", group.name),
                    vm_name: None,
                    step_type: StepType::RunScript,
                    status: StepStatus::Pending,
                    started: None,
                    completed: None,
                    error: None,
                    details: Some(post.command.clone()),
                });
                step_num += 1;
            }

            // Startup delay between groups.
            if group.startup_delay_secs > 0 {
                steps.push(RecoveryStep {
                    step_number: step_num,
                    description: format!(
                        "Wait {} seconds after group '{}'",
                        group.startup_delay_secs, group.name
                    ),
                    vm_name: None,
                    step_type: StepType::WaitDelay,
                    status: StepStatus::Pending,
                    started: None,
                    completed: None,
                    error: None,
                    details: Some(group.startup_delay_secs.to_string()),
                });
                step_num += 1;
            }
        }

        // Post-recovery scripts.
        for script in &plan.post_scripts {
            steps.push(RecoveryStep {
                step_number: step_num,
                description: format!("Run post-recovery script: {}", script.name),
                vm_name: None,
                step_type: StepType::RunScript,
                status: StepStatus::Pending,
                started: None,
                completed: None,
                error: None,
                details: Some(script.command.clone()),
            });
            step_num += 1;
        }

        steps
    }

    /// Execute a single recovery step, dispatching `PowerOff`/`PowerOn` to
    /// `self.driver` (see `with_driver`) rather than shelling out to
    /// machinectl/systemd-vmspawn directly.
    ///
    /// Updates the step's `status`, `started`, `completed`, and `error`
    /// fields in place.  Returns `Ok(())` on success or an error describing
    /// what went wrong.
    pub async fn execute_step(&self, step: &mut RecoveryStep) -> Result<()> {
        step.status = StepStatus::Running;
        step.started = Some(Utc::now());

        let target = step.vm_name.as_deref().unwrap_or("unknown");

        let result = match step.step_type {
            StepType::PowerOff => {
                tracing::info!(vm = target, "step {}: powering off VM", step.step_number);
                let driver = self
                    .driver
                    .as_ref()
                    .ok_or_else(|| anyhow::anyhow!("recovery execution requires a VM driver — construct via SiteRecoveryManager::with_driver()"))?;
                driver
                    .poweroff(target)
                    .await
                    .map_err(|e| anyhow::anyhow!("failed to power off '{target}': {e:#}"))
            }
            StepType::PowerOn => {
                tracing::info!(vm = target, "step {}: powering on VM", step.step_number);
                let driver = self
                    .driver
                    .as_ref()
                    .ok_or_else(|| anyhow::anyhow!("recovery execution requires a VM driver — construct via SiteRecoveryManager::with_driver()"))?;
                driver
                    .start(target)
                    .await
                    .map_err(|e| anyhow::anyhow!("failed to start '{target}': {e:#}"))
            }
            StepType::Sync => {
                // Sync the VM image to the target site using rsync.
                let target_site = step.details.as_deref().unwrap_or("localhost");
                let source = format!("/var/lib/machines/{}.raw", target);
                let dest = format!("{}:/var/lib/machines/{}.raw", target_site, target);
                tracing::info!(
                    vm = target,
                    dest = %dest,
                    "step {}: syncing VM image", step.step_number
                );
                let output = std::process::Command::new("rsync")
                    .args(["-avz", "--partial", &source, &dest])
                    .output()
                    .map_err(|e| anyhow::anyhow!("failed to run rsync: {e}"))?;
                if output.status.success() {
                    Ok(())
                } else {
                    let stderr = String::from_utf8_lossy(&output.stderr);
                    Err(anyhow::anyhow!("rsync failed: {}", stderr.trim()))
                }
            }
            StepType::WaitDelay => {
                let secs = step
                    .details
                    .as_deref()
                    .and_then(|s| s.parse::<u64>().ok())
                    .unwrap_or(5);
                tracing::info!("step {}: waiting {} seconds", step.step_number, secs);
                std::thread::sleep(std::time::Duration::from_secs(secs));
                Ok(())
            }
            StepType::RunScript => {
                if let Some(ref script) = step.details {
                    // Validate that the script path is an absolute path under
                    // allowed directories.  This prevents arbitrary command
                    // injection -- only pre-validated admin-defined scripts
                    // are permitted.
                    validate_script_path(script)?;

                    tracing::info!("step {}: running script: {}", step.step_number, script);
                    let output = std::process::Command::new("/bin/sh")
                        .args(["-c", script])
                        .output()
                        .map_err(|e| anyhow::anyhow!("failed to run script: {e}"))?;
                    if output.status.success() {
                        Ok(())
                    } else {
                        let stderr = String::from_utf8_lossy(&output.stderr);
                        Err(anyhow::anyhow!("script failed: {}", stderr.trim()))
                    }
                } else {
                    tracing::warn!(
                        "step {}: RunScript step has no command, skipping",
                        step.step_number
                    );
                    Ok(())
                }
            }
            StepType::Recover => {
                // Recovery = verify image exists, then prepare it for start.
                let image_path = format!("/var/lib/machines/{}.raw", target);
                tracing::info!(
                    vm = target,
                    "step {}: recovering VM (verifying image at {})",
                    step.step_number,
                    image_path
                );
                if std::path::Path::new(&image_path).exists() {
                    Ok(())
                } else {
                    Err(anyhow::anyhow!("VM image not found at {image_path}"))
                }
            }
            StepType::NetworkConfig => {
                // Network reconfiguration is environment-specific; log and
                // succeed for now.  Real implementations would call networkctl
                // or equivalent.
                tracing::info!(
                    vm = target,
                    "step {}: network configuration (no-op in current implementation)",
                    step.step_number
                );
                Ok(())
            }
        };

        match result {
            Ok(()) => {
                step.status = StepStatus::Completed;
                step.completed = Some(Utc::now());
                Ok(())
            }
            Err(e) => {
                step.status = StepStatus::Failed;
                step.error = Some(e.to_string());
                step.completed = Some(Utc::now());
                Err(e)
            }
        }
    }

    /// Run all pending steps in an execution sequentially.  Stops on the
    /// first failure and marks subsequent steps as skipped.
    ///
    /// Returns `Ok(true)` if all steps completed, `Ok(false)` if a step
    /// failed (execution is marked as failed), or `Err` on internal errors.
    pub async fn run_execution(&self, execution_id: &str) -> Result<bool> {
        // Collect the step numbers we need to execute.
        let step_numbers: Vec<u32> = {
            let executions = self.executions.read().unwrap_or_else(|e| e.into_inner());
            let exec = executions
                .get(execution_id)
                .ok_or_else(|| SiteRecoveryError::ExecutionNotFound(execution_id.to_string()))?;
            exec.steps
                .iter()
                .filter(|s| s.status == StepStatus::Pending)
                .map(|s| s.step_number)
                .collect()
        };

        let mut all_ok = true;

        for step_num in step_numbers {
            // Extract the step, execute it, then put it back.
            let mut step = {
                let executions = self.executions.read().unwrap_or_else(|e| e.into_inner());
                let exec = executions.get(execution_id).ok_or_else(|| {
                    SiteRecoveryError::ExecutionNotFound(execution_id.to_string())
                })?;
                exec.steps
                    .iter()
                    .find(|s| s.step_number == step_num)
                    .cloned()
                    .ok_or_else(|| {
                        SiteRecoveryError::StepNotFound(step_num, execution_id.to_string())
                    })?
            };

            let exec_result = self.execute_step(&mut step).await;

            // Write the step result back.
            {
                let mut executions = self.executions.write().unwrap_or_else(|e| e.into_inner());
                let exec = executions.get_mut(execution_id).ok_or_else(|| {
                    SiteRecoveryError::ExecutionNotFound(execution_id.to_string())
                })?;

                if let Some(s) = exec.steps.iter_mut().find(|s| s.step_number == step_num) {
                    s.status = step.status.clone();
                    s.started = step.started;
                    s.completed = step.completed;
                    s.error = step.error.clone();

                    // Update VM counters for PowerOn steps.
                    if s.step_type == StepType::PowerOn {
                        match s.status {
                            StepStatus::Completed => exec.vms_recovered += 1,
                            StepStatus::Failed => exec.vms_failed += 1,
                            _ => {}
                        }
                    }
                }
            }

            if exec_result.is_err() {
                all_ok = false;
                // Skip remaining steps.
                let mut executions = self.executions.write().unwrap_or_else(|e| e.into_inner());
                if let Some(exec) = executions.get_mut(execution_id) {
                    for s in &mut exec.steps {
                        if s.status == StepStatus::Pending {
                            s.status = StepStatus::Skipped;
                        }
                    }
                }
                break;
            }
        }

        self.complete_execution(execution_id, all_ok)?;
        Ok(all_ok)
    }

    /// Clean up resources created by a test failover (tear down the isolated
    /// test environment). Marks the execution as completed.
    pub fn cleanup_test_failover(&self, execution_id: &str) -> Result<()> {
        let mut executions = self.executions.write().unwrap_or_else(|e| e.into_inner());
        let exec = executions
            .get_mut(execution_id)
            .ok_or_else(|| SiteRecoveryError::ExecutionNotFound(execution_id.to_string()))?;

        if exec.execution_type != ExecutionType::TestFailover {
            bail!("execution {} is not a test failover", execution_id);
        }

        let now = Utc::now();
        exec.status = ExecutionStatus::Completed;
        exec.completed = Some(now);

        let duration = now - exec.started;
        exec.rto_actual_minutes = Some(duration.num_minutes().max(0) as u32);

        let plan_id = exec.plan_id.clone();
        let plan_name = exec.plan_name.clone();
        let vms_recovered = exec.vms_recovered;
        let vms_failed = exec.vms_failed;
        let success = exec.vms_failed == 0;

        drop(executions);

        // Update plan and record test result.
        let mut plans = self.plans.write().unwrap_or_else(|e| e.into_inner());
        if let Some(plan) = plans.get_mut(&plan_id) {
            plan.status = PlanStatus::Ready;
            plan.last_tested = Some(now);
            plan.updated = Some(now);
        }
        drop(plans);

        let result = TestResult {
            plan_id,
            plan_name,
            tested_at: now,
            success,
            duration_secs: duration.num_seconds().max(0) as u64,
            vms_tested: vms_recovered + vms_failed,
            vms_failed,
        };

        let mut results = self.test_results.write().unwrap_or_else(|e| e.into_inner());
        results.push(result);

        tracing::info!(
            execution_id = %execution_id,
            "cleaned up test failover environment"
        );
        Ok(())
    }

    // -- Dashboard ----------------------------------------------------------

    /// Build a high-level DR dashboard from current state.
    pub fn get_dashboard(&self) -> DrDashboard {
        let plans = self.plans.read().unwrap_or_else(|e| e.into_inner());
        let results = self.test_results.read().unwrap_or_else(|e| e.into_inner());

        let total_plans = plans.len() as u32;
        let ready_plans = plans
            .values()
            .filter(|p| p.status == PlanStatus::Ready)
            .count() as u32;
        let failed_plans = plans
            .values()
            .filter(|p| p.status == PlanStatus::Failed)
            .count() as u32;

        // Count protected VMs (VMs in any plan's priority groups).
        let mut protected_vm_set = std::collections::HashSet::new();
        for plan in plans.values() {
            for group in &plan.priority_groups {
                for vm in &group.vm_names {
                    protected_vm_set.insert(vm.clone());
                }
            }
        }
        let protected_vms = protected_vm_set.len() as u32;

        // RPO violations: count plans that have never been tested or executed.
        let rpo_violations = plans
            .values()
            .filter(|p| p.last_tested.is_none() && p.last_executed.is_none())
            .count() as u32;

        // Determine overall health.
        let overall_health = if failed_plans > 0 {
            DrHealth::Critical
        } else if rpo_violations > 0 || ready_plans < total_plans {
            DrHealth::Warning
        } else {
            DrHealth::Healthy
        };

        let plans_tested = plans.values().filter(|p| p.last_tested.is_some()).count() as u32;
        let rto_targets: Vec<u32> = plans.values().filter_map(|p| p.rto_minutes).collect();
        let avg_rto_seconds = if rto_targets.is_empty() {
            0.0
        } else {
            (rto_targets.iter().sum::<u32>() as f64 / rto_targets.len() as f64) * 60.0
        };

        DrDashboard {
            total_plans,
            ready_plans,
            failed_plans,
            protected_vms,
            unprotected_vms: 0, // Would require external VM inventory.
            rpo_violations,
            last_test_results: results.clone(),
            overall_health,
            plans_tested,
            plans_untested: total_plans - plans_tested,
            avg_rto_seconds,
            avg_rpo_minutes: 0.0,
            sites: Vec::new(),
            recent_executions: Vec::new(),
        }
    }

    /// Get test results, optionally filtered by plan id.
    pub fn get_test_results(&self, plan_id: Option<&str>) -> Vec<TestResult> {
        let results = self.test_results.read().unwrap_or_else(|e| e.into_inner());
        results
            .iter()
            .filter(|r| match plan_id {
                Some(pid) => r.plan_id == pid,
                None => true,
            })
            .cloned()
            .collect()
    }

    // -- Internal helpers ---------------------------------------------------

    /// Common logic to start a recovery execution.
    fn start_execution(
        &self,
        plan_id: &str,
        initiated_by: &str,
        exec_type: ExecutionType,
    ) -> Result<RecoveryExecution> {
        let mut plans = self.plans.write().unwrap_or_else(|e| e.into_inner());
        let plan = plans
            .get_mut(plan_id)
            .ok_or_else(|| SiteRecoveryError::PlanNotFound(plan_id.to_string()))?;

        if plan.status != PlanStatus::Ready && plan.status != PlanStatus::Completed {
            bail!(SiteRecoveryError::PlanNotReady(plan_id.to_string()));
        }

        let steps = Self::generate_recovery_steps(plan, exec_type.clone());

        // Transition plan status.
        match exec_type {
            ExecutionType::TestFailover => plan.status = PlanStatus::Testing,
            _ => plan.status = PlanStatus::InProgress,
        }
        plan.updated = Some(Utc::now());

        let execution = RecoveryExecution {
            id: Uuid::new_v4().to_string(),
            plan_id: plan.id.clone(),
            plan_name: plan.name.clone(),
            execution_type: exec_type,
            status: ExecutionStatus::Running,
            started: Utc::now(),
            completed: None,
            steps,
            rto_actual_minutes: None,
            vms_recovered: 0,
            vms_failed: 0,
            error: None,
            initiated_by: initiated_by.to_string(),
        };

        drop(plans);

        let mut executions = self.executions.write().unwrap_or_else(|e| e.into_inner());
        executions.insert(execution.id.clone(), execution.clone());

        tracing::info!(
            execution_id = %execution.id,
            plan_id = %plan_id,
            exec_type = ?execution.execution_type,
            initiated_by = %initiated_by,
            "started recovery execution"
        );

        Ok(execution)
    }
}

impl Default for SiteRecoveryManager {
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

    /// Helper: build a simple recovery plan with two priority groups.
    fn sample_plan() -> RecoveryPlan {
        RecoveryPlan {
            id: String::new(),
            name: "dc-failover".to_string(),
            description: Some("Primary to secondary DC failover".to_string()),
            source_site_id: "site-a".to_string(),
            target_site_id: "site-b".to_string(),
            priority_groups: vec![
                PriorityGroup {
                    priority: 1,
                    name: "databases".to_string(),
                    vm_names: vec!["db-1".to_string(), "db-2".to_string()],
                    startup_delay_secs: 30,
                    pre_action: Some(RecoveryScript {
                        name: "quiesce-db".to_string(),
                        script_type: ScriptType::PreGroup,
                        command: "/usr/local/bin/quiesce-db.sh".to_string(),
                        timeout_secs: 120,
                    }),
                    post_action: None,
                },
                PriorityGroup {
                    priority: 2,
                    name: "app-servers".to_string(),
                    vm_names: vec!["app-1".to_string(), "app-2".to_string()],
                    startup_delay_secs: 10,
                    pre_action: None,
                    post_action: Some(RecoveryScript {
                        name: "verify-app".to_string(),
                        script_type: ScriptType::PostGroup,
                        command: "/usr/local/bin/verify-app.sh".to_string(),
                        timeout_secs: 60,
                    }),
                },
            ],
            pre_scripts: vec![RecoveryScript {
                name: "notify-start".to_string(),
                script_type: ScriptType::PreRecovery,
                command: "/usr/local/bin/notify.sh start".to_string(),
                timeout_secs: 30,
            }],
            post_scripts: vec![RecoveryScript {
                name: "notify-end".to_string(),
                script_type: ScriptType::PostRecovery,
                command: "/usr/local/bin/notify.sh end".to_string(),
                timeout_secs: 30,
            }],
            network_mappings: vec![NetworkMapping {
                source_network: "prod-vlan-100".to_string(),
                target_network: "dr-vlan-200".to_string(),
                test_network: Some("test-vlan-999".to_string()),
            }],
            rto_minutes: Some(60),
            last_tested: None,
            last_executed: None,
            status: PlanStatus::Ready,
            created: Utc::now(),
            updated: None,
        }
    }

    // -- 1. Plan creation ---------------------------------------------------

    #[test]
    fn test_create_plan() {
        let mgr = SiteRecoveryManager::new();
        let plan = mgr.create_plan(sample_plan()).unwrap();

        assert!(!plan.id.is_empty());
        assert_eq!(plan.name, "dc-failover");
        assert_eq!(plan.source_site_id, "site-a");
        assert_eq!(plan.target_site_id, "site-b");
        assert_eq!(plan.status, PlanStatus::Ready);
        assert_eq!(plan.priority_groups.len(), 2);
        assert!(plan.updated.is_none());

        // Verify plan is retrievable.
        let fetched = mgr.get_plan(&plan.id).unwrap();
        assert_eq!(fetched.name, plan.name);
    }

    // -- 2. Plan CRUD (list, update, delete) --------------------------------

    #[test]
    fn test_plan_crud_lifecycle() {
        let mgr = SiteRecoveryManager::new();
        let plan = mgr.create_plan(sample_plan()).unwrap();
        let plan_id = plan.id.clone();

        // List.
        assert_eq!(mgr.list_plans().len(), 1);

        // Update.
        let mut updated = plan.clone();
        updated.name = "updated-failover".to_string();
        updated.rto_minutes = Some(30);
        let result = mgr.update_plan(&plan_id, updated).unwrap();
        assert_eq!(result.name, "updated-failover");
        assert_eq!(result.rto_minutes, Some(30));
        assert!(result.updated.is_some());
        assert_eq!(result.id, plan_id); // id preserved

        // Delete.
        mgr.delete_plan(&plan_id).unwrap();
        assert!(mgr.get_plan(&plan_id).is_none());
        assert!(mgr.list_plans().is_empty());
    }

    // -- 3. Delete nonexistent plan fails -----------------------------------

    #[test]
    fn test_delete_nonexistent_plan() {
        let mgr = SiteRecoveryManager::new();
        let result = mgr.delete_plan("nonexistent");
        assert!(result.is_err());
    }

    // -- 4. Step generation for planned migration ---------------------------

    #[test]
    fn test_generate_steps_planned_migration() {
        let plan = sample_plan();
        let steps =
            SiteRecoveryManager::generate_recovery_steps(&plan, ExecutionType::PlannedMigration);

        // Expected steps:
        // 1 pre-recovery script
        // group 1 (databases, 2 VMs): pre-action + 2*(poweroff+sync+recover+network+poweron) + delay = 1 + 10 + 1 = 12
        // group 2 (app-servers, 2 VMs): 2*(poweroff+sync+recover+network+poweron) + post-action + delay = 10 + 1 + 1 = 12
        // 1 post-recovery script
        // Total = 1 + 12 + 12 + 1 = 26

        assert!(!steps.is_empty());

        // First step should be pre-recovery script.
        assert_eq!(steps[0].step_type, StepType::RunScript);
        assert!(steps[0].description.contains("pre-recovery"));

        // Last step should be post-recovery script.
        let last = steps.last().unwrap();
        assert_eq!(last.step_type, StepType::RunScript);
        assert!(last.description.contains("post-recovery"));

        // Planned migration should include PowerOff and Sync steps.
        let power_off_count = steps
            .iter()
            .filter(|s| s.step_type == StepType::PowerOff)
            .count();
        assert_eq!(power_off_count, 4); // 4 VMs total

        let sync_count = steps
            .iter()
            .filter(|s| s.step_type == StepType::Sync)
            .count();
        assert_eq!(sync_count, 4);

        // All steps should be pending.
        assert!(steps.iter().all(|s| s.status == StepStatus::Pending));

        // Step numbers should be sequential.
        for (i, step) in steps.iter().enumerate() {
            assert_eq!(step.step_number, (i + 1) as u32);
        }
    }

    // -- 5. Step generation for disaster recovery (no poweroff/sync) --------

    #[test]
    fn test_generate_steps_disaster_recovery() {
        let plan = sample_plan();
        let steps =
            SiteRecoveryManager::generate_recovery_steps(&plan, ExecutionType::DisasterRecovery);

        // DR should NOT include PowerOff or Sync steps.
        let power_off_count = steps
            .iter()
            .filter(|s| s.step_type == StepType::PowerOff)
            .count();
        assert_eq!(power_off_count, 0);

        let sync_count = steps
            .iter()
            .filter(|s| s.step_type == StepType::Sync)
            .count();
        assert_eq!(sync_count, 0);

        // Should still have Recover and PowerOn steps.
        let recover_count = steps
            .iter()
            .filter(|s| s.step_type == StepType::Recover)
            .count();
        assert_eq!(recover_count, 4);

        let power_on_count = steps
            .iter()
            .filter(|s| s.step_type == StepType::PowerOn)
            .count();
        assert_eq!(power_on_count, 4);
    }

    // -- 6. Execute planned migration lifecycle -----------------------------

    #[test]
    fn test_execute_planned_migration_lifecycle() {
        let mgr = SiteRecoveryManager::new();
        let plan = mgr.create_plan(sample_plan()).unwrap();
        let plan_id = plan.id.clone();

        // Start execution.
        let exec = mgr
            .execute_planned_migration(&plan_id, "admin@example.com")
            .unwrap();
        assert_eq!(exec.status, ExecutionStatus::Running);
        assert_eq!(exec.execution_type, ExecutionType::PlannedMigration);
        assert_eq!(exec.initiated_by, "admin@example.com");
        assert!(!exec.steps.is_empty());

        // Plan should be in-progress.
        let plan_state = mgr.get_plan(&plan_id).unwrap();
        assert_eq!(plan_state.status, PlanStatus::InProgress);

        // Complete execution.
        mgr.complete_execution(&exec.id, true).unwrap();
        let finished = mgr.get_execution(&exec.id).unwrap();
        assert_eq!(finished.status, ExecutionStatus::Completed);
        assert!(finished.completed.is_some());
        assert!(finished.rto_actual_minutes.is_some());

        // Plan should be completed.
        let plan_state = mgr.get_plan(&plan_id).unwrap();
        assert_eq!(plan_state.status, PlanStatus::Completed);
        assert!(plan_state.last_executed.is_some());
    }

    // -- 7. Execute disaster recovery and fail ------------------------------

    #[test]
    fn test_execute_disaster_recovery_failure() {
        let mgr = SiteRecoveryManager::new();
        let plan = mgr.create_plan(sample_plan()).unwrap();

        let exec = mgr
            .execute_disaster_recovery(&plan.id, "on-call@example.com")
            .unwrap();
        assert_eq!(exec.execution_type, ExecutionType::DisasterRecovery);

        // Complete with failure.
        mgr.complete_execution(&exec.id, false).unwrap();
        let finished = mgr.get_execution(&exec.id).unwrap();
        assert_eq!(finished.status, ExecutionStatus::Failed);
        assert!(finished.error.is_some());

        // Plan should be failed.
        let plan_state = mgr.get_plan(&plan.id).unwrap();
        assert_eq!(plan_state.status, PlanStatus::Failed);
    }

    // -- 8. Test failover and cleanup ---------------------------------------

    #[test]
    fn test_failover_and_cleanup() {
        let mgr = SiteRecoveryManager::new();
        let plan = mgr.create_plan(sample_plan()).unwrap();

        let exec = mgr
            .execute_test_failover(&plan.id, "tester@example.com")
            .unwrap();
        assert_eq!(exec.execution_type, ExecutionType::TestFailover);

        // Plan should be in testing state.
        let plan_state = mgr.get_plan(&plan.id).unwrap();
        assert_eq!(plan_state.status, PlanStatus::Testing);

        // Clean up test failover.
        mgr.cleanup_test_failover(&exec.id).unwrap();
        let finished = mgr.get_execution(&exec.id).unwrap();
        assert_eq!(finished.status, ExecutionStatus::Completed);

        // Plan should be back to ready.
        let plan_state = mgr.get_plan(&plan.id).unwrap();
        assert_eq!(plan_state.status, PlanStatus::Ready);
        assert!(plan_state.last_tested.is_some());

        // Test result should be recorded.
        let results = mgr.get_test_results(Some(&plan.id));
        assert_eq!(results.len(), 1);
        assert!(results[0].success);
    }

    // -- 9. Dashboard summary -----------------------------------------------

    #[test]
    fn test_dashboard_summary() {
        let mgr = SiteRecoveryManager::new();

        // Empty dashboard.
        let dash = mgr.get_dashboard();
        assert_eq!(dash.total_plans, 0);
        assert_eq!(dash.overall_health, DrHealth::Healthy);

        // Add a plan.
        let plan = mgr.create_plan(sample_plan()).unwrap();

        let dash = mgr.get_dashboard();
        assert_eq!(dash.total_plans, 1);
        assert_eq!(dash.ready_plans, 1);
        assert_eq!(dash.failed_plans, 0);
        assert_eq!(dash.protected_vms, 4); // db-1, db-2, app-1, app-2
                                           // Plan has never been tested or executed: RPO violation.
        assert_eq!(dash.rpo_violations, 1);
        assert_eq!(dash.overall_health, DrHealth::Warning);

        // Simulate a failed plan.
        let exec = mgr.execute_disaster_recovery(&plan.id, "admin").unwrap();
        mgr.complete_execution(&exec.id, false).unwrap();

        let dash = mgr.get_dashboard();
        assert_eq!(dash.failed_plans, 1);
        assert_eq!(dash.overall_health, DrHealth::Critical);
    }

    // -- 10. Step status updates --------------------------------------------

    #[test]
    fn test_step_status_updates() {
        let mgr = SiteRecoveryManager::new();
        let plan = mgr.create_plan(sample_plan()).unwrap();

        let exec = mgr.execute_planned_migration(&plan.id, "admin").unwrap();

        // Update first step to running.
        mgr.update_step_status(&exec.id, 1, StepStatus::Running, None)
            .unwrap();
        let e = mgr.get_execution(&exec.id).unwrap();
        assert_eq!(e.steps[0].status, StepStatus::Running);
        assert!(e.steps[0].started.is_some());

        // Complete first step.
        mgr.update_step_status(&exec.id, 1, StepStatus::Completed, None)
            .unwrap();
        let e = mgr.get_execution(&exec.id).unwrap();
        assert_eq!(e.steps[0].status, StepStatus::Completed);
        assert!(e.steps[0].completed.is_some());

        // Fail a non-existent step.
        let result = mgr.update_step_status(&exec.id, 9999, StepStatus::Failed, None);
        assert!(result.is_err());
    }

    // -- 11. Cancellation ---------------------------------------------------

    #[test]
    fn test_cancel_execution() {
        let mgr = SiteRecoveryManager::new();
        let plan = mgr.create_plan(sample_plan()).unwrap();

        let exec = mgr.execute_planned_migration(&plan.id, "admin").unwrap();

        // Cancel it.
        mgr.cancel_execution(&exec.id).unwrap();
        let cancelled = mgr.get_execution(&exec.id).unwrap();
        assert_eq!(cancelled.status, ExecutionStatus::Cancelled);
        assert!(cancelled.completed.is_some());

        // All pending steps should be skipped.
        assert!(cancelled
            .steps
            .iter()
            .all(|s| s.status == StepStatus::Skipped));

        // Plan should be back to ready.
        let plan_state = mgr.get_plan(&plan.id).unwrap();
        assert_eq!(plan_state.status, PlanStatus::Ready);

        // Cannot cancel again.
        let result = mgr.cancel_execution(&exec.id);
        assert!(result.is_err());
    }

    // -- 12. List executions with filter ------------------------------------

    #[test]
    fn test_list_executions_filter() {
        let mgr = SiteRecoveryManager::new();
        let plan_a = mgr.create_plan(sample_plan()).unwrap();

        let mut plan_b_data = sample_plan();
        plan_b_data.name = "secondary-failover".to_string();
        let plan_b = mgr.create_plan(plan_b_data).unwrap();

        let _exec_a = mgr.execute_planned_migration(&plan_a.id, "admin").unwrap();
        let _exec_b = mgr.execute_test_failover(&plan_b.id, "admin").unwrap();

        // All executions.
        assert_eq!(mgr.list_executions(None).len(), 2);

        // Filter by plan.
        assert_eq!(mgr.list_executions(Some(&plan_a.id)).len(), 1);
        assert_eq!(mgr.list_executions(Some(&plan_b.id)).len(), 1);
        assert_eq!(mgr.list_executions(Some("nonexistent")).len(), 0);
    }

    // -- 13. Reprotect execution --------------------------------------------

    #[test]
    fn test_reprotect_execution() {
        let mgr = SiteRecoveryManager::new();
        let plan = mgr.create_plan(sample_plan()).unwrap();

        let exec = mgr.execute_reprotect(&plan.id, "admin").unwrap();
        assert_eq!(exec.execution_type, ExecutionType::Reprotect);
        assert_eq!(exec.status, ExecutionStatus::Running);

        mgr.complete_execution(&exec.id, true).unwrap();
        let finished = mgr.get_execution(&exec.id).unwrap();
        assert_eq!(finished.status, ExecutionStatus::Completed);
    }

    // -- 14. Cannot execute plan that is not ready --------------------------

    #[test]
    fn test_cannot_execute_non_ready_plan() {
        let mgr = SiteRecoveryManager::new();
        let plan = mgr.create_plan(sample_plan()).unwrap();

        // Start first execution (plan becomes InProgress).
        let _exec = mgr.execute_planned_migration(&plan.id, "admin").unwrap();

        // Second execution should fail because plan is InProgress.
        let result = mgr.execute_disaster_recovery(&plan.id, "admin");
        assert!(result.is_err());
    }
}
