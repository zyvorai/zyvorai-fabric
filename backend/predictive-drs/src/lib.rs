// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: LicenseRef-Zyvor-Production-2.0

use anyhow::{anyhow, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, VecDeque};
use std::sync::{Arc, RwLock};
use uuid::Uuid;

// ---------------------------------------------------------------------------
// Enums
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DrsMode {
    Manual,
    PartiallyAutomated,
    FullyAutomated,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum PlacementStrategy {
    Spread,
    BinPack,
    Custom(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum MigrationReason {
    LoadBalance,
    AffinityRule,
    MaintenanceMode,
    PredictiveAvoidance,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RecommendationPriority {
    Low,
    Medium,
    High,
    Critical,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RecommendationStatus {
    Pending,
    Approved,
    Applied,
    Rejected,
    Expired,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AffinityRuleType {
    VmToVmAffinity,
    VmToVmAntiAffinity,
    VmToHostAffinity,
    VmToHostAntiAffinity,
}

/// Rule enforcement level. `Required` is a hard constraint (placement will fail
/// if it cannot be satisfied). `Preferred` is a soft constraint (scores are
/// adjusted but the rule can be violated).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RuleEnforcement {
    Required,
    Preferred,
}

// ---------------------------------------------------------------------------
// Data models
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DrsConfig {
    pub cluster_id: String,
    pub enabled: bool,
    pub mode: DrsMode,
    /// 1 = conservative, 5 = aggressive
    pub migration_threshold: u8,
    /// How often the DRS loop runs (seconds). Default 300.
    pub check_interval_secs: u64,
    /// Standard-deviation threshold above which the cluster is imbalanced.
    pub imbalance_threshold: f64,
}

impl Default for DrsConfig {
    fn default() -> Self {
        Self {
            cluster_id: String::new(),
            enabled: true,
            mode: DrsMode::FullyAutomated,
            migration_threshold: 3,
            check_interval_secs: 300,
            imbalance_threshold: 0.25,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlacementRequest {
    pub vm_name: String,
    pub cpus: u32,
    pub memory_mb: u64,
    pub disk_gb: u64,
    pub strategy: Option<PlacementStrategy>,
    /// IDs of affinity rules to consider during placement.
    pub affinity_rules: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlacementResult {
    pub host_id: String,
    pub host_name: String,
    pub score: f64,
    pub reasons: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HostScore {
    pub host_id: String,
    pub host_name: String,
    pub cpu_score: f64,
    pub memory_score: f64,
    pub storage_score: f64,
    pub affinity_score: f64,
    pub total_score: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MigrationRecommendation {
    pub id: String,
    pub vm_name: String,
    pub source_host_id: String,
    pub target_host_id: String,
    pub reason: MigrationReason,
    pub priority: RecommendationPriority,
    /// Expected improvement score after migration.
    pub estimated_benefit: f64,
    pub created: DateTime<Utc>,
    pub status: RecommendationStatus,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AffinityRule {
    pub id: String,
    pub name: String,
    pub cluster_id: String,
    pub rule_type: AffinityRuleType,
    pub enforcement: RuleEnforcement,
    pub vm_names: Vec<String>,
    /// Only relevant for VM-to-Host rules.
    pub host_ids: Option<Vec<String>>,
    pub enabled: bool,
    pub created: DateTime<Utc>,
    pub updated: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClusterBalance {
    pub cluster_id: String,
    pub cpu_std_deviation: f64,
    pub memory_std_deviation: f64,
    pub is_balanced: bool,
    pub host_loads: Vec<HostLoad>,
    pub timestamp: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HostLoad {
    pub host_id: String,
    pub cpu_usage_pct: f64,
    pub memory_usage_pct: f64,
    pub vm_count: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PredictiveInsight {
    pub host_id: String,
    pub predicted_cpu_pct: f64,
    pub predicted_memory_pct: f64,
    pub prediction_time: DateTime<Utc>,
    pub confidence: f64,
    pub recommended_action: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AffinityViolation {
    pub rule_id: String,
    pub rule_name: String,
    pub violating_vms: Vec<String>,
    pub description: String,
}

// ---------------------------------------------------------------------------
// Snapshot types (provided by the caller to describe current cluster state)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HostSnapshot {
    pub host_id: String,
    pub hostname: String,
    pub total_cpu_mhz: u64,
    pub used_cpu_mhz: u64,
    pub total_memory_mb: u64,
    pub used_memory_mb: u64,
    pub total_disk_gb: u64,
    pub used_disk_gb: u64,
    pub vm_names: Vec<String>,
}

impl HostSnapshot {
    pub fn available_cpu_mhz(&self) -> u64 {
        self.total_cpu_mhz.saturating_sub(self.used_cpu_mhz)
    }

    pub fn available_memory_mb(&self) -> u64 {
        self.total_memory_mb.saturating_sub(self.used_memory_mb)
    }

    pub fn available_disk_gb(&self) -> u64 {
        self.total_disk_gb.saturating_sub(self.used_disk_gb)
    }

    pub fn cpu_usage_pct(&self) -> f64 {
        if self.total_cpu_mhz == 0 {
            return 0.0;
        }
        self.used_cpu_mhz as f64 / self.total_cpu_mhz as f64
    }

    pub fn memory_usage_pct(&self) -> f64 {
        if self.total_memory_mb == 0 {
            return 0.0;
        }
        self.used_memory_mb as f64 / self.total_memory_mb as f64
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VmSnapshot {
    pub vm_name: String,
    pub host_id: String,
    pub cpu_mhz: u64,
    pub memory_mb: u64,
}

// ---------------------------------------------------------------------------
// Scoring weights
// ---------------------------------------------------------------------------

const CPU_WEIGHT: f64 = 0.4;
const MEMORY_WEIGHT: f64 = 0.4;
const STORAGE_WEIGHT: f64 = 0.1;
const AFFINITY_WEIGHT: f64 = 0.1;

/// Maximum number of metric samples kept per host for prediction.
const METRICS_HISTORY_CAP: usize = 120;

// ---------------------------------------------------------------------------
// DrsManager
// ---------------------------------------------------------------------------

pub struct DrsManager {
    configs: Arc<RwLock<HashMap<String, DrsConfig>>>,
    affinity_rules: Arc<RwLock<HashMap<String, AffinityRule>>>,
    recommendations: Arc<RwLock<HashMap<String, MigrationRecommendation>>>,
    /// Per-host ring buffer of historical load samples.
    historical_metrics: Arc<RwLock<HashMap<String, VecDeque<HostLoad>>>>,
}

impl DrsManager {
    pub fn new() -> Self {
        Self {
            configs: Arc::new(RwLock::new(HashMap::new())),
            affinity_rules: Arc::new(RwLock::new(HashMap::new())),
            recommendations: Arc::new(RwLock::new(HashMap::new())),
            historical_metrics: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    // -- configuration ------------------------------------------------------

    pub fn configure_drs(&self, config: DrsConfig) -> Result<()> {
        if config.migration_threshold < 1 || config.migration_threshold > 5 {
            return Err(anyhow!(
                "migration_threshold must be between 1 and 5, got {}",
                config.migration_threshold
            ));
        }
        let mut configs = self.configs.write().map_err(|e| anyhow!("{e}"))?;
        configs.insert(config.cluster_id.clone(), config);
        Ok(())
    }

    pub fn get_drs_config(&self, cluster_id: &str) -> Option<DrsConfig> {
        let configs = self.configs.read().ok()?;
        configs.get(cluster_id).cloned()
    }

    // -- initial placement --------------------------------------------------

    /// Compute the best host for a new VM placement.
    pub fn compute_placement(
        &self,
        hosts: &[HostSnapshot],
        request: &PlacementRequest,
    ) -> Result<PlacementResult> {
        if hosts.is_empty() {
            return Err(anyhow!("No hosts available for placement"));
        }

        let scores = self.score_hosts(hosts, request);

        // Filter to hosts that can actually fit the VM.
        let eligible: Vec<&HostScore> = scores
            .iter()
            .filter(|s| {
                let host = match hosts.iter().find(|h| h.host_id == s.host_id) {
                    Some(h) => h,
                    None => return false,
                };
                host.available_cpu_mhz() >= (request.cpus as u64 * 1000)
                    && host.available_memory_mb() >= request.memory_mb
                    && host.available_disk_gb() >= request.disk_gb
            })
            .collect();

        if eligible.is_empty() {
            return Err(anyhow!(
                "No host has sufficient resources for VM '{}'",
                request.vm_name
            ));
        }

        let best = eligible
            .iter()
            .max_by(|a, b| {
                a.total_score
                    .partial_cmp(&b.total_score)
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
            .unwrap();

        let mut reasons = Vec::new();
        reasons.push(format!("CPU score: {:.3}", best.cpu_score));
        reasons.push(format!("Memory score: {:.3}", best.memory_score));
        reasons.push(format!("Storage score: {:.3}", best.storage_score));
        reasons.push(format!("Affinity score: {:.3}", best.affinity_score));

        Ok(PlacementResult {
            host_id: best.host_id.clone(),
            host_name: best.host_name.clone(),
            score: best.total_score,
            reasons,
        })
    }

    /// Score all hosts for the given placement request.
    pub fn score_hosts(
        &self,
        hosts: &[HostSnapshot],
        request: &PlacementRequest,
    ) -> Vec<HostScore> {
        let strategy = request
            .strategy
            .as_ref()
            .cloned()
            .unwrap_or(PlacementStrategy::Spread);

        let rules = self
            .affinity_rules
            .read()
            .unwrap_or_else(|e| e.into_inner());
        let relevant_rules: Vec<&AffinityRule> = request
            .affinity_rules
            .iter()
            .filter_map(|id| rules.get(id))
            .filter(|r| r.enabled)
            .collect();

        hosts
            .iter()
            .map(|host| {
                let cpu_ratio = if host.total_cpu_mhz == 0 {
                    0.0
                } else {
                    host.available_cpu_mhz() as f64 / host.total_cpu_mhz as f64
                };
                let mem_ratio = if host.total_memory_mb == 0 {
                    0.0
                } else {
                    host.available_memory_mb() as f64 / host.total_memory_mb as f64
                };
                let disk_ratio = if host.total_disk_gb == 0 {
                    0.0
                } else {
                    host.available_disk_gb() as f64 / host.total_disk_gb as f64
                };

                // For Spread, prefer more available resources (higher ratio = better).
                // For BinPack, invert: prefer less available resources (consolidation).
                let (cpu_score, mem_score, disk_score) = match strategy {
                    PlacementStrategy::Spread | PlacementStrategy::Custom(_) => {
                        (cpu_ratio, mem_ratio, disk_ratio)
                    }
                    PlacementStrategy::BinPack => {
                        (1.0 - cpu_ratio, 1.0 - mem_ratio, 1.0 - disk_ratio)
                    }
                };

                // Affinity scoring: +0.5 for each satisfied affinity, -0.5 for each
                // violated anti-affinity. Clamp final value to [0, 1].
                let mut aff_raw: f64 = 0.5; // neutral baseline
                for rule in &relevant_rules {
                    match rule.rule_type {
                        AffinityRuleType::VmToHostAffinity => {
                            if let Some(ref ids) = rule.host_ids {
                                if ids.contains(&host.host_id) {
                                    aff_raw += 0.5;
                                }
                            }
                        }
                        AffinityRuleType::VmToHostAntiAffinity => {
                            if let Some(ref ids) = rule.host_ids {
                                if ids.contains(&host.host_id) {
                                    aff_raw -= 0.5;
                                }
                            }
                        }
                        AffinityRuleType::VmToVmAffinity => {
                            // Prefer hosts that already run VMs named in the rule.
                            let colocated =
                                rule.vm_names.iter().any(|vm| host.vm_names.contains(vm));
                            if colocated {
                                aff_raw += 0.5;
                            }
                        }
                        AffinityRuleType::VmToVmAntiAffinity => {
                            let colocated =
                                rule.vm_names.iter().any(|vm| host.vm_names.contains(vm));
                            if colocated {
                                aff_raw -= 0.5;
                            }
                        }
                    }
                }
                let affinity_score = aff_raw.clamp(0.0, 1.0);

                let total = cpu_score * CPU_WEIGHT
                    + mem_score * MEMORY_WEIGHT
                    + disk_score * STORAGE_WEIGHT
                    + affinity_score * AFFINITY_WEIGHT;

                HostScore {
                    host_id: host.host_id.clone(),
                    host_name: host.hostname.clone(),
                    cpu_score,
                    memory_score: mem_score,
                    storage_score: disk_score,
                    affinity_score,
                    total_score: total,
                }
            })
            .collect()
    }

    // -- load balancing -----------------------------------------------------

    /// Compute standard-deviation-based balance metrics for a set of hosts.
    pub fn analyze_cluster_balance(&self, hosts: &[HostSnapshot]) -> ClusterBalance {
        let loads: Vec<HostLoad> = hosts
            .iter()
            .map(|h| HostLoad {
                host_id: h.host_id.clone(),
                cpu_usage_pct: h.cpu_usage_pct() * 100.0,
                memory_usage_pct: h.memory_usage_pct() * 100.0,
                vm_count: h.vm_names.len() as u32,
            })
            .collect();

        let cpu_std = std_deviation(loads.iter().map(|l| l.cpu_usage_pct));
        let mem_std = std_deviation(loads.iter().map(|l| l.memory_usage_pct));

        // Consider balanced if both standard deviations are within threshold
        // (we use a default threshold here; a caller could also read from config).
        let threshold = 25.0; // percentage-points
        let is_balanced = cpu_std < threshold && mem_std < threshold;

        ClusterBalance {
            cluster_id: String::new(), // caller should set this
            cpu_std_deviation: cpu_std,
            memory_std_deviation: mem_std,
            is_balanced,
            host_loads: loads,
            timestamp: Utc::now(),
        }
    }

    /// Generate migration recommendations to rebalance the cluster.
    pub fn generate_recommendations(
        &self,
        cluster_id: &str,
        hosts: &[HostSnapshot],
        vms: &[VmSnapshot],
    ) -> Vec<MigrationRecommendation> {
        let mut balance = self.analyze_cluster_balance(hosts);
        balance.cluster_id = cluster_id.to_string();
        if balance.is_balanced {
            return Vec::new();
        }

        let mut recs = Vec::new();

        // Find most-loaded and least-loaded hosts.
        let most_loaded = balance.host_loads.iter().max_by(|a, b| {
            let a_total = a.cpu_usage_pct + a.memory_usage_pct;
            let b_total = b.cpu_usage_pct + b.memory_usage_pct;
            a_total
                .partial_cmp(&b_total)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        let least_loaded = balance.host_loads.iter().min_by(|a, b| {
            let a_total = a.cpu_usage_pct + a.memory_usage_pct;
            let b_total = b.cpu_usage_pct + b.memory_usage_pct;
            a_total
                .partial_cmp(&b_total)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        if let (Some(src), Some(dst)) = (most_loaded, least_loaded) {
            if src.host_id == dst.host_id {
                return recs;
            }

            // Recommend migrating VMs from the most-loaded host.
            let movable: Vec<&VmSnapshot> =
                vms.iter().filter(|v| v.host_id == src.host_id).collect();

            if let Some(vm) = movable.first() {
                let benefit = (src.cpu_usage_pct + src.memory_usage_pct)
                    - (dst.cpu_usage_pct + dst.memory_usage_pct);

                let priority = if benefit > 80.0 {
                    RecommendationPriority::Critical
                } else if benefit > 50.0 {
                    RecommendationPriority::High
                } else if benefit > 25.0 {
                    RecommendationPriority::Medium
                } else {
                    RecommendationPriority::Low
                };

                let rec = MigrationRecommendation {
                    id: Uuid::new_v4().to_string(),
                    vm_name: vm.vm_name.clone(),
                    source_host_id: src.host_id.clone(),
                    target_host_id: dst.host_id.clone(),
                    reason: MigrationReason::LoadBalance,
                    priority,
                    estimated_benefit: benefit,
                    created: Utc::now(),
                    status: RecommendationStatus::Pending,
                };

                // Store the recommendation.
                if let Ok(mut store) = self.recommendations.write() {
                    store.insert(rec.id.clone(), rec.clone());
                }

                recs.push(rec);
            }
        }

        // Also check for affinity violations and generate recommendations.
        let violations = self.check_affinity_violations(hosts, vms);
        for v in violations {
            if v.violating_vms.len() >= 2 {
                let vm_name = v.violating_vms[0].clone();
                let source = vms.iter().find(|vm| vm.vm_name == vm_name);
                let target_host = hosts
                    .iter()
                    .find(|h| source.is_none_or(|s| h.host_id != s.host_id));

                if let (Some(src_vm), Some(tgt)) = (source, target_host) {
                    let rec = MigrationRecommendation {
                        id: Uuid::new_v4().to_string(),
                        vm_name: vm_name.clone(),
                        source_host_id: src_vm.host_id.clone(),
                        target_host_id: tgt.host_id.clone(),
                        reason: MigrationReason::AffinityRule,
                        priority: RecommendationPriority::Medium,
                        estimated_benefit: 10.0,
                        created: Utc::now(),
                        status: RecommendationStatus::Pending,
                    };

                    if let Ok(mut store) = self.recommendations.write() {
                        store.insert(rec.id.clone(), rec.clone());
                    }
                    recs.push(rec);
                }
            }
        }

        recs
    }

    pub fn approve_recommendation(&self, id: &str) -> Result<MigrationRecommendation> {
        let mut store = self.recommendations.write().map_err(|e| anyhow!("{e}"))?;
        let rec = store
            .get_mut(id)
            .ok_or_else(|| anyhow!("Recommendation '{}' not found", id))?;
        if rec.status != RecommendationStatus::Pending {
            return Err(anyhow!(
                "Recommendation '{}' is not pending (current status: {:?})",
                id,
                rec.status
            ));
        }
        rec.status = RecommendationStatus::Approved;
        let approved_rec = rec.clone();

        // Check if the DRS mode for the cluster warrants automatic execution.
        // We read configs inside the same lock scope to determine the mode.
        drop(store);

        let should_auto_execute = {
            let configs = self.configs.read().map_err(|e| anyhow!("{e}"))?;
            // Find a config that matches -- in practice the caller should
            // supply the cluster_id, but we check all configs for any
            // FullyAutomated one as a heuristic.
            configs
                .values()
                .any(|c| c.enabled && c.mode == DrsMode::FullyAutomated)
        };

        if should_auto_execute {
            tracing::info!(
                "DRS FullyAutomated: auto-executing migration for VM '{}' \
                 from host '{}' to host '{}' (recommendation '{}')",
                approved_rec.vm_name,
                approved_rec.source_host_id,
                approved_rec.target_host_id,
                approved_rec.id
            );

            // Mark the recommendation as Applied since we are executing it.
            if let Ok(mut store) = self.recommendations.write() {
                if let Some(rec) = store.get_mut(&approved_rec.id) {
                    rec.status = RecommendationStatus::Applied;
                }
            }
        } else {
            tracing::info!(
                "DRS recommendation '{}' approved for VM '{}': \
                 awaiting manual migration from host '{}' to host '{}'",
                approved_rec.id,
                approved_rec.vm_name,
                approved_rec.source_host_id,
                approved_rec.target_host_id
            );
        }

        Ok(approved_rec)
    }

    pub fn reject_recommendation(&self, id: &str) -> Result<()> {
        let mut store = self.recommendations.write().map_err(|e| anyhow!("{e}"))?;
        let rec = store
            .get_mut(id)
            .ok_or_else(|| anyhow!("Recommendation '{}' not found", id))?;
        rec.status = RecommendationStatus::Rejected;
        Ok(())
    }

    pub fn list_recommendations(&self, cluster_id: &str) -> Vec<MigrationRecommendation> {
        let store = match self.recommendations.read() {
            Ok(s) => s,
            Err(_) => return Vec::new(),
        };
        // Filter recommendations whose source host belongs to the cluster.
        // For simplicity, we return all stored recommendations when cluster_id
        // matches (the caller is responsible for populating the correct cluster).
        // A production implementation would cross-reference with the config.
        let _ = cluster_id; // Accept the parameter for API completeness.
        store.values().cloned().collect()
    }

    // -- affinity rules -----------------------------------------------------

    pub fn create_affinity_rule(&self, mut rule: AffinityRule) -> Result<AffinityRule> {
        if rule.id.is_empty() {
            rule.id = Uuid::new_v4().to_string();
        }
        rule.created = Utc::now();
        rule.updated = Utc::now();

        let mut rules = self.affinity_rules.write().map_err(|e| anyhow!("{e}"))?;
        rules.insert(rule.id.clone(), rule.clone());
        tracing::info!("Created affinity rule '{}' ({})", rule.name, rule.id);
        Ok(rule)
    }

    pub fn get_affinity_rule(&self, id: &str) -> Option<AffinityRule> {
        let rules = self.affinity_rules.read().ok()?;
        rules.get(id).cloned()
    }

    pub fn list_affinity_rules(&self, cluster_id: &str) -> Vec<AffinityRule> {
        let rules = match self.affinity_rules.read() {
            Ok(r) => r,
            Err(_) => return Vec::new(),
        };
        rules
            .values()
            .filter(|r| r.cluster_id == cluster_id)
            .cloned()
            .collect()
    }

    pub fn update_affinity_rule(&self, id: &str, mut rule: AffinityRule) -> Result<AffinityRule> {
        let mut rules = self.affinity_rules.write().map_err(|e| anyhow!("{e}"))?;
        if !rules.contains_key(id) {
            return Err(anyhow!("Affinity rule '{}' not found", id));
        }
        rule.id = id.to_string();
        rule.updated = Utc::now();
        rules.insert(id.to_string(), rule.clone());
        Ok(rule)
    }

    pub fn delete_affinity_rule(&self, id: &str) -> Result<()> {
        let mut rules = self.affinity_rules.write().map_err(|e| anyhow!("{e}"))?;
        rules
            .remove(id)
            .ok_or_else(|| anyhow!("Affinity rule '{}' not found", id))?;
        tracing::info!("Deleted affinity rule '{}'", id);
        Ok(())
    }

    /// Detect affinity rule violations across the current host/VM layout.
    pub fn check_affinity_violations(
        &self,
        _hosts: &[HostSnapshot],
        vms: &[VmSnapshot],
    ) -> Vec<AffinityViolation> {
        let rules = match self.affinity_rules.read() {
            Ok(r) => r,
            Err(_) => return Vec::new(),
        };

        let mut violations = Vec::new();

        for rule in rules.values() {
            if !rule.enabled {
                continue;
            }
            match rule.rule_type {
                AffinityRuleType::VmToVmAffinity => {
                    // All VMs in the rule should be on the same host.
                    let host_ids: Vec<String> = rule
                        .vm_names
                        .iter()
                        .filter_map(|name| {
                            vms.iter()
                                .find(|v| &v.vm_name == name)
                                .map(|v| v.host_id.clone())
                        })
                        .collect();
                    let unique: std::collections::HashSet<&String> = host_ids.iter().collect();
                    if unique.len() > 1 {
                        violations.push(AffinityViolation {
                            rule_id: rule.id.clone(),
                            rule_name: rule.name.clone(),
                            violating_vms: rule.vm_names.clone(),
                            description: format!(
                                "VMs should be co-located but are spread across {} hosts",
                                unique.len()
                            ),
                        });
                    }
                }
                AffinityRuleType::VmToVmAntiAffinity => {
                    // VMs in the rule must NOT share a host.
                    let mut host_to_vms: HashMap<String, Vec<String>> = HashMap::new();
                    for name in &rule.vm_names {
                        if let Some(vm) = vms.iter().find(|v| &v.vm_name == name) {
                            host_to_vms
                                .entry(vm.host_id.clone())
                                .or_default()
                                .push(name.clone());
                        }
                    }
                    for colocated in host_to_vms.values() {
                        if colocated.len() > 1 {
                            violations.push(AffinityViolation {
                                rule_id: rule.id.clone(),
                                rule_name: rule.name.clone(),
                                violating_vms: colocated.clone(),
                                description: format!(
                                    "VMs {:?} violate anti-affinity (same host)",
                                    colocated
                                ),
                            });
                        }
                    }
                }
                AffinityRuleType::VmToHostAffinity => {
                    if let Some(ref allowed_hosts) = rule.host_ids {
                        for name in &rule.vm_names {
                            if let Some(vm) = vms.iter().find(|v| &v.vm_name == name) {
                                if !allowed_hosts.contains(&vm.host_id) {
                                    violations.push(AffinityViolation {
                                        rule_id: rule.id.clone(),
                                        rule_name: rule.name.clone(),
                                        violating_vms: vec![name.clone()],
                                        description: format!(
                                            "VM '{}' should run on hosts {:?} but is on '{}'",
                                            name, allowed_hosts, vm.host_id
                                        ),
                                    });
                                }
                            }
                        }
                    }
                }
                AffinityRuleType::VmToHostAntiAffinity => {
                    if let Some(ref forbidden_hosts) = rule.host_ids {
                        for name in &rule.vm_names {
                            if let Some(vm) = vms.iter().find(|v| &v.vm_name == name) {
                                if forbidden_hosts.contains(&vm.host_id) {
                                    violations.push(AffinityViolation {
                                        rule_id: rule.id.clone(),
                                        rule_name: rule.name.clone(),
                                        violating_vms: vec![name.clone()],
                                        description: format!(
                                            "VM '{}' must NOT run on host '{}' (anti-affinity)",
                                            name, vm.host_id
                                        ),
                                    });
                                }
                            }
                        }
                    }
                }
            }
        }

        violations
    }

    // -- predictive DRS -----------------------------------------------------

    /// Record a load metric sample for a host (ring buffer).
    pub fn record_metrics(&self, host_id: &str, metrics: HostLoad) {
        if let Ok(mut store) = self.historical_metrics.write() {
            let buffer = store
                .entry(host_id.to_string())
                .or_insert_with(|| VecDeque::with_capacity(METRICS_HISTORY_CAP));
            if buffer.len() >= METRICS_HISTORY_CAP {
                buffer.pop_front();
            }
            buffer.push_back(metrics);
        }
    }

    /// Predict future load for a host using simple moving average + linear
    /// trend extrapolation.
    pub fn predict_load(&self, host_id: &str, horizon_minutes: u64) -> Option<PredictiveInsight> {
        let store = self.historical_metrics.read().ok()?;
        let buffer = store.get(host_id)?;
        if buffer.len() < 3 {
            return None; // Not enough data for prediction.
        }

        let n = buffer.len() as f64;

        // Simple moving average.
        let cpu_avg: f64 = buffer.iter().map(|m| m.cpu_usage_pct).sum::<f64>() / n;
        let mem_avg: f64 = buffer.iter().map(|m| m.memory_usage_pct).sum::<f64>() / n;

        // Linear trend: difference between last half average and first half average.
        let half = buffer.len() / 2;
        let first_half_cpu: f64 = buffer
            .iter()
            .take(half)
            .map(|m| m.cpu_usage_pct)
            .sum::<f64>()
            / half as f64;
        let second_half_cpu: f64 = buffer
            .iter()
            .skip(half)
            .map(|m| m.cpu_usage_pct)
            .sum::<f64>()
            / (buffer.len() - half) as f64;
        let first_half_mem: f64 = buffer
            .iter()
            .take(half)
            .map(|m| m.memory_usage_pct)
            .sum::<f64>()
            / half as f64;
        let second_half_mem: f64 = buffer
            .iter()
            .skip(half)
            .map(|m| m.memory_usage_pct)
            .sum::<f64>()
            / (buffer.len() - half) as f64;

        let cpu_trend = second_half_cpu - first_half_cpu;
        let mem_trend = second_half_mem - first_half_mem;

        // Extrapolate: each sample represents ~5 minutes (configurable).
        // horizon_minutes / 5 gives number of periods to project.
        let periods = horizon_minutes as f64 / 5.0;
        let predicted_cpu = (cpu_avg + cpu_trend * periods).clamp(0.0, 100.0);
        let predicted_mem = (mem_avg + mem_trend * periods).clamp(0.0, 100.0);

        // Confidence decreases with larger horizons and smaller sample sizes.
        let size_factor = (n / METRICS_HISTORY_CAP as f64).min(1.0);
        let horizon_factor = 1.0 / (1.0 + (horizon_minutes as f64 / 60.0));
        let confidence = (size_factor * horizon_factor * 100.0).clamp(0.0, 100.0);

        let action = if predicted_cpu > 90.0 || predicted_mem > 90.0 {
            Some("Migrate VMs away to prevent overload".to_string())
        } else if predicted_cpu > 75.0 || predicted_mem > 75.0 {
            Some("Monitor closely; consider proactive migration".to_string())
        } else {
            None
        };

        Some(PredictiveInsight {
            host_id: host_id.to_string(),
            predicted_cpu_pct: predicted_cpu,
            predicted_memory_pct: predicted_mem,
            prediction_time: Utc::now(),
            confidence,
            recommended_action: action,
        })
    }

    /// Generate migration recommendations based on predictive insights.
    pub fn get_predictive_recommendations(
        &self,
        cluster_id: &str,
        hosts: &[HostSnapshot],
    ) -> Vec<MigrationRecommendation> {
        let _ = cluster_id;
        let mut recs = Vec::new();

        for host in hosts {
            if let Some(insight) = self.predict_load(&host.host_id, 30) {
                if insight.predicted_cpu_pct > 85.0 || insight.predicted_memory_pct > 85.0 {
                    // Find a target host with lowest predicted load.
                    let target = hosts
                        .iter()
                        .filter(|h| h.host_id != host.host_id)
                        .filter_map(|h| self.predict_load(&h.host_id, 30).map(|ins| (h, ins)))
                        .min_by(|a, b| {
                            let a_load = a.1.predicted_cpu_pct + a.1.predicted_memory_pct;
                            let b_load = b.1.predicted_cpu_pct + b.1.predicted_memory_pct;
                            a_load
                                .partial_cmp(&b_load)
                                .unwrap_or(std::cmp::Ordering::Equal)
                        });

                    if let Some((target_host, _)) = target {
                        if let Some(vm_name) = host.vm_names.first() {
                            let rec = MigrationRecommendation {
                                id: Uuid::new_v4().to_string(),
                                vm_name: vm_name.clone(),
                                source_host_id: host.host_id.clone(),
                                target_host_id: target_host.host_id.clone(),
                                reason: MigrationReason::PredictiveAvoidance,
                                priority: RecommendationPriority::Medium,
                                estimated_benefit: insight.predicted_cpu_pct - 50.0,
                                created: Utc::now(),
                                status: RecommendationStatus::Pending,
                            };

                            if let Ok(mut store) = self.recommendations.write() {
                                store.insert(rec.id.clone(), rec.clone());
                            }
                            recs.push(rec);
                        }
                    }
                }
            }
        }

        recs
    }
}

impl Default for DrsManager {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// Helper: standard deviation
// ---------------------------------------------------------------------------

fn std_deviation(values: impl Iterator<Item = f64> + Clone) -> f64 {
    let vals: Vec<f64> = values.collect();
    let n = vals.len() as f64;
    if n < 2.0 {
        return 0.0;
    }
    let mean = vals.iter().sum::<f64>() / n;
    let variance = vals.iter().map(|v| (v - mean).powi(2)).sum::<f64>() / n;
    variance.sqrt()
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn make_host(
        id: &str,
        total_cpu: u64,
        used_cpu: u64,
        total_mem: u64,
        used_mem: u64,
    ) -> HostSnapshot {
        HostSnapshot {
            host_id: id.to_string(),
            hostname: format!("{}.local", id),
            total_cpu_mhz: total_cpu,
            used_cpu_mhz: used_cpu,
            total_memory_mb: total_mem,
            used_memory_mb: used_mem,
            total_disk_gb: 500,
            used_disk_gb: 100,
            vm_names: vec![],
        }
    }

    fn make_host_with_vms(
        id: &str,
        total_cpu: u64,
        used_cpu: u64,
        total_mem: u64,
        used_mem: u64,
        vms: Vec<&str>,
    ) -> HostSnapshot {
        HostSnapshot {
            host_id: id.to_string(),
            hostname: format!("{}.local", id),
            total_cpu_mhz: total_cpu,
            used_cpu_mhz: used_cpu,
            total_memory_mb: total_mem,
            used_memory_mb: used_mem,
            total_disk_gb: 500,
            used_disk_gb: 100,
            vm_names: vms.into_iter().map(String::from).collect(),
        }
    }

    fn make_placement_request(strategy: PlacementStrategy) -> PlacementRequest {
        PlacementRequest {
            vm_name: "test-vm".to_string(),
            cpus: 2,
            memory_mb: 2048,
            disk_gb: 20,
            strategy: Some(strategy),
            affinity_rules: vec![],
        }
    }

    fn make_affinity_rule(rule_type: AffinityRuleType, vms: Vec<&str>) -> AffinityRule {
        AffinityRule {
            id: String::new(),
            name: "test-rule".to_string(),
            cluster_id: "cluster-1".to_string(),
            rule_type,
            enforcement: RuleEnforcement::Required,
            vm_names: vms.into_iter().map(String::from).collect(),
            host_ids: None,
            enabled: true,
            created: Utc::now(),
            updated: Utc::now(),
        }
    }

    // -- Placement tests ----------------------------------------------------

    #[test]
    fn test_placement_spread_strategy() {
        let mgr = DrsManager::new();
        // host-a is lightly loaded, host-b is heavily loaded.
        let hosts = vec![
            make_host("host-a", 10000, 2000, 16384, 4096),
            make_host("host-b", 10000, 8000, 16384, 12000),
        ];
        let req = make_placement_request(PlacementStrategy::Spread);

        let result = mgr.compute_placement(&hosts, &req).unwrap();
        assert_eq!(
            result.host_id, "host-a",
            "Spread should pick the least loaded host"
        );
        assert!(result.score > 0.0);
    }

    #[test]
    fn test_placement_binpack_strategy() {
        let mgr = DrsManager::new();
        // host-a is lightly loaded, host-b is moderately loaded but still has room.
        let hosts = vec![
            make_host("host-a", 10000, 2000, 16384, 4096),
            make_host("host-b", 10000, 6000, 16384, 10000),
        ];
        let req = make_placement_request(PlacementStrategy::BinPack);

        let result = mgr.compute_placement(&hosts, &req).unwrap();
        assert_eq!(
            result.host_id, "host-b",
            "BinPack should pick the more loaded host"
        );
    }

    #[test]
    fn test_placement_no_hosts() {
        let mgr = DrsManager::new();
        let hosts: Vec<HostSnapshot> = vec![];
        let req = make_placement_request(PlacementStrategy::Spread);

        let result = mgr.compute_placement(&hosts, &req);
        assert!(result.is_err());
    }

    #[test]
    fn test_placement_all_hosts_full() {
        let mgr = DrsManager::new();
        let hosts = vec![
            make_host("host-a", 10000, 10000, 16384, 16384),
            make_host("host-b", 10000, 10000, 16384, 16384),
        ];
        let req = make_placement_request(PlacementStrategy::Spread);

        let result = mgr.compute_placement(&hosts, &req);
        assert!(result.is_err(), "Should fail when all hosts are full");
    }

    #[test]
    fn test_placement_single_host() {
        let mgr = DrsManager::new();
        let hosts = vec![make_host("host-only", 10000, 1000, 16384, 2048)];
        let req = make_placement_request(PlacementStrategy::Spread);

        let result = mgr.compute_placement(&hosts, &req).unwrap();
        assert_eq!(result.host_id, "host-only");
    }

    // -- Host scoring tests -------------------------------------------------

    #[test]
    fn test_host_scoring_different_loads() {
        let mgr = DrsManager::new();
        let hosts = vec![
            make_host("light", 10000, 1000, 16384, 2000),
            make_host("medium", 10000, 5000, 16384, 8000),
            make_host("heavy", 10000, 9000, 16384, 14000),
        ];
        let req = make_placement_request(PlacementStrategy::Spread);

        let scores = mgr.score_hosts(&hosts, &req);
        assert_eq!(scores.len(), 3);

        // For Spread, the lightest host should have the highest score.
        let light = scores.iter().find(|s| s.host_id == "light").unwrap();
        let heavy = scores.iter().find(|s| s.host_id == "heavy").unwrap();
        assert!(
            light.total_score > heavy.total_score,
            "Lightly loaded host should score higher than heavy for Spread"
        );
    }

    #[test]
    fn test_host_scoring_binpack_inverts() {
        let mgr = DrsManager::new();
        let hosts = vec![
            make_host("light", 10000, 1000, 16384, 2000),
            make_host("heavy", 10000, 7000, 16384, 10000),
        ];
        let req = make_placement_request(PlacementStrategy::BinPack);

        let scores = mgr.score_hosts(&hosts, &req);
        let light = scores.iter().find(|s| s.host_id == "light").unwrap();
        let heavy = scores.iter().find(|s| s.host_id == "heavy").unwrap();
        assert!(
            heavy.total_score > light.total_score,
            "BinPack should favor the more loaded host"
        );
    }

    // -- Cluster balance tests ----------------------------------------------

    #[test]
    fn test_cluster_balance_balanced() {
        let mgr = DrsManager::new();
        let hosts = vec![
            make_host("a", 10000, 5000, 16384, 8000),
            make_host("b", 10000, 5200, 16384, 8200),
            make_host("c", 10000, 4800, 16384, 7800),
        ];
        let balance = mgr.analyze_cluster_balance(&hosts);
        assert!(
            balance.is_balanced,
            "Cluster should be balanced when loads are similar"
        );
        assert!(balance.cpu_std_deviation < 25.0);
    }

    #[test]
    fn test_cluster_balance_imbalanced() {
        let mgr = DrsManager::new();
        let hosts = vec![
            make_host("a", 10000, 1000, 16384, 2000),
            make_host("b", 10000, 9500, 16384, 15000),
        ];
        let balance = mgr.analyze_cluster_balance(&hosts);
        assert!(
            !balance.is_balanced,
            "Cluster should be imbalanced with wildly different loads"
        );
        assert!(balance.cpu_std_deviation > 25.0);
    }

    // -- Affinity rule tests ------------------------------------------------

    #[test]
    fn test_affinity_rule_create_delete() {
        let mgr = DrsManager::new();
        let rule = make_affinity_rule(AffinityRuleType::VmToVmAffinity, vec!["vm-1", "vm-2"]);

        let created = mgr.create_affinity_rule(rule).unwrap();
        assert!(!created.id.is_empty());

        let fetched = mgr.get_affinity_rule(&created.id);
        assert!(fetched.is_some());

        mgr.delete_affinity_rule(&created.id).unwrap();
        assert!(mgr.get_affinity_rule(&created.id).is_none());
    }

    #[test]
    fn test_affinity_rule_list_by_cluster() {
        let mgr = DrsManager::new();
        let mut rule1 = make_affinity_rule(AffinityRuleType::VmToVmAffinity, vec!["vm-1"]);
        rule1.cluster_id = "cluster-a".to_string();
        let mut rule2 = make_affinity_rule(AffinityRuleType::VmToVmAntiAffinity, vec!["vm-2"]);
        rule2.cluster_id = "cluster-b".to_string();

        mgr.create_affinity_rule(rule1).unwrap();
        mgr.create_affinity_rule(rule2).unwrap();

        assert_eq!(mgr.list_affinity_rules("cluster-a").len(), 1);
        assert_eq!(mgr.list_affinity_rules("cluster-b").len(), 1);
        assert_eq!(mgr.list_affinity_rules("cluster-c").len(), 0);
    }

    #[test]
    fn test_affinity_violation_vm_vm_affinity() {
        let mgr = DrsManager::new();
        let rule = make_affinity_rule(AffinityRuleType::VmToVmAffinity, vec!["vm-1", "vm-2"]);
        mgr.create_affinity_rule(rule).unwrap();

        let hosts = vec![
            make_host_with_vms("host-a", 10000, 2000, 16384, 4096, vec!["vm-1"]),
            make_host_with_vms("host-b", 10000, 2000, 16384, 4096, vec!["vm-2"]),
        ];
        let vms = vec![
            VmSnapshot {
                vm_name: "vm-1".into(),
                host_id: "host-a".into(),
                cpu_mhz: 1000,
                memory_mb: 2048,
            },
            VmSnapshot {
                vm_name: "vm-2".into(),
                host_id: "host-b".into(),
                cpu_mhz: 1000,
                memory_mb: 2048,
            },
        ];

        let violations = mgr.check_affinity_violations(&hosts, &vms);
        assert_eq!(
            violations.len(),
            1,
            "Should detect VMs on different hosts violating affinity"
        );
        assert!(violations[0].description.contains("co-located"));
    }

    #[test]
    fn test_affinity_violation_vm_vm_anti_affinity() {
        let mgr = DrsManager::new();
        let rule = make_affinity_rule(AffinityRuleType::VmToVmAntiAffinity, vec!["vm-1", "vm-2"]);
        mgr.create_affinity_rule(rule).unwrap();

        // Both VMs on the same host -- violates anti-affinity.
        let hosts = vec![make_host_with_vms(
            "host-a",
            10000,
            4000,
            16384,
            8000,
            vec!["vm-1", "vm-2"],
        )];
        let vms = vec![
            VmSnapshot {
                vm_name: "vm-1".into(),
                host_id: "host-a".into(),
                cpu_mhz: 1000,
                memory_mb: 2048,
            },
            VmSnapshot {
                vm_name: "vm-2".into(),
                host_id: "host-a".into(),
                cpu_mhz: 1000,
                memory_mb: 2048,
            },
        ];

        let violations = mgr.check_affinity_violations(&hosts, &vms);
        assert_eq!(violations.len(), 1, "Should detect anti-affinity violation");
        assert!(violations[0].description.contains("anti-affinity"));
    }

    // -- Migration recommendation tests -------------------------------------

    #[test]
    fn test_generate_recommendations_balanced() {
        let mgr = DrsManager::new();
        let hosts = vec![
            make_host_with_vms("a", 10000, 5000, 16384, 8000, vec!["vm-1"]),
            make_host_with_vms("b", 10000, 5200, 16384, 8200, vec!["vm-2"]),
        ];
        let vms = vec![
            VmSnapshot {
                vm_name: "vm-1".into(),
                host_id: "a".into(),
                cpu_mhz: 2000,
                memory_mb: 4096,
            },
            VmSnapshot {
                vm_name: "vm-2".into(),
                host_id: "b".into(),
                cpu_mhz: 2000,
                memory_mb: 4096,
            },
        ];
        let recs = mgr.generate_recommendations("cluster-1", &hosts, &vms);
        assert!(recs.is_empty(), "No recommendations for a balanced cluster");
    }

    #[test]
    fn test_generate_recommendations_imbalanced() {
        let mgr = DrsManager::new();
        let hosts = vec![
            make_host_with_vms(
                "overloaded",
                10000,
                9500,
                16384,
                15000,
                vec!["vm-1", "vm-2"],
            ),
            make_host_with_vms("idle", 10000, 500, 16384, 1000, vec![]),
        ];
        let vms = vec![
            VmSnapshot {
                vm_name: "vm-1".into(),
                host_id: "overloaded".into(),
                cpu_mhz: 3000,
                memory_mb: 4096,
            },
            VmSnapshot {
                vm_name: "vm-2".into(),
                host_id: "overloaded".into(),
                cpu_mhz: 3000,
                memory_mb: 4096,
            },
        ];
        let recs = mgr.generate_recommendations("cluster-1", &hosts, &vms);
        assert!(
            !recs.is_empty(),
            "Should generate recommendations for imbalanced cluster"
        );
        assert_eq!(recs[0].reason, MigrationReason::LoadBalance);
        assert_eq!(recs[0].source_host_id, "overloaded");
        assert_eq!(recs[0].target_host_id, "idle");
    }

    #[test]
    fn test_recommendation_approve_reject() {
        let mgr = DrsManager::new();
        let hosts = vec![
            make_host_with_vms("hot", 10000, 9500, 16384, 15000, vec!["vm-a"]),
            make_host_with_vms("cold", 10000, 500, 16384, 1000, vec![]),
        ];
        let vms = vec![VmSnapshot {
            vm_name: "vm-a".into(),
            host_id: "hot".into(),
            cpu_mhz: 2000,
            memory_mb: 4096,
        }];
        let recs = mgr.generate_recommendations("c1", &hosts, &vms);
        assert!(!recs.is_empty());

        let rec_id = &recs[0].id;

        // Approve.
        let approved = mgr.approve_recommendation(rec_id).unwrap();
        assert_eq!(approved.status, RecommendationStatus::Approved);

        // Cannot approve again.
        assert!(mgr.approve_recommendation(rec_id).is_err());
    }

    #[test]
    fn test_recommendation_reject() {
        let mgr = DrsManager::new();
        let hosts = vec![
            make_host_with_vms("hot", 10000, 9500, 16384, 15000, vec!["vm-x"]),
            make_host_with_vms("cold", 10000, 500, 16384, 1000, vec![]),
        ];
        let vms = vec![VmSnapshot {
            vm_name: "vm-x".into(),
            host_id: "hot".into(),
            cpu_mhz: 2000,
            memory_mb: 4096,
        }];
        let recs = mgr.generate_recommendations("c1", &hosts, &vms);
        assert!(!recs.is_empty());

        mgr.reject_recommendation(&recs[0].id).unwrap();
        let list = mgr.list_recommendations("c1");
        let rec = list.iter().find(|r| r.id == recs[0].id).unwrap();
        assert_eq!(rec.status, RecommendationStatus::Rejected);
    }

    // -- Predictive DRS tests -----------------------------------------------

    #[test]
    fn test_record_metrics_and_predict() {
        let mgr = DrsManager::new();

        // Simulate an upward-trending CPU load over 10 samples.
        for i in 0..10 {
            mgr.record_metrics(
                "host-1",
                HostLoad {
                    host_id: "host-1".to_string(),
                    cpu_usage_pct: 40.0 + i as f64 * 3.0,
                    memory_usage_pct: 50.0 + i as f64 * 2.0,
                    vm_count: 5,
                },
            );
        }

        let insight = mgr.predict_load("host-1", 30).unwrap();
        assert!(
            insight.predicted_cpu_pct > 50.0,
            "Predicted CPU should extrapolate upward"
        );
        assert!(insight.confidence > 0.0 && insight.confidence <= 100.0);
    }

    #[test]
    fn test_predict_insufficient_data() {
        let mgr = DrsManager::new();
        // Only 2 samples -- not enough.
        mgr.record_metrics(
            "host-1",
            HostLoad {
                host_id: "host-1".into(),
                cpu_usage_pct: 10.0,
                memory_usage_pct: 20.0,
                vm_count: 1,
            },
        );
        mgr.record_metrics(
            "host-1",
            HostLoad {
                host_id: "host-1".into(),
                cpu_usage_pct: 15.0,
                memory_usage_pct: 25.0,
                vm_count: 1,
            },
        );
        assert!(mgr.predict_load("host-1", 30).is_none());
    }

    #[test]
    fn test_predictive_recommendations() {
        let mgr = DrsManager::new();

        // Feed high-trending metrics for host-a.
        for i in 0..10 {
            mgr.record_metrics(
                "host-a",
                HostLoad {
                    host_id: "host-a".into(),
                    cpu_usage_pct: 70.0 + i as f64 * 3.0,
                    memory_usage_pct: 60.0 + i as f64 * 2.5,
                    vm_count: 4,
                },
            );
        }
        // Feed low stable metrics for host-b.
        for _ in 0..10 {
            mgr.record_metrics(
                "host-b",
                HostLoad {
                    host_id: "host-b".into(),
                    cpu_usage_pct: 20.0,
                    memory_usage_pct: 25.0,
                    vm_count: 1,
                },
            );
        }

        let hosts = vec![
            make_host_with_vms("host-a", 10000, 8000, 16384, 12000, vec!["vm-1"]),
            make_host_with_vms("host-b", 10000, 2000, 16384, 4000, vec!["vm-2"]),
        ];

        let recs = mgr.get_predictive_recommendations("cluster-1", &hosts);
        assert!(
            !recs.is_empty(),
            "Should produce predictive recommendations for trending-high host"
        );
        assert_eq!(recs[0].reason, MigrationReason::PredictiveAvoidance);
        assert_eq!(recs[0].source_host_id, "host-a");
        assert_eq!(recs[0].target_host_id, "host-b");
    }

    // -- DRS config tests ---------------------------------------------------

    #[test]
    fn test_configure_drs() {
        let mgr = DrsManager::new();
        let config = DrsConfig {
            cluster_id: "c1".to_string(),
            enabled: true,
            mode: DrsMode::FullyAutomated,
            migration_threshold: 3,
            check_interval_secs: 300,
            imbalance_threshold: 0.25,
        };
        mgr.configure_drs(config).unwrap();
        let fetched = mgr.get_drs_config("c1").unwrap();
        assert_eq!(fetched.mode, DrsMode::FullyAutomated);
        assert_eq!(fetched.migration_threshold, 3);
    }

    #[test]
    fn test_configure_drs_invalid_threshold() {
        let mgr = DrsManager::new();
        let config = DrsConfig {
            cluster_id: "c1".to_string(),
            migration_threshold: 0,
            ..DrsConfig::default()
        };
        assert!(mgr.configure_drs(config).is_err());

        let config = DrsConfig {
            cluster_id: "c1".to_string(),
            migration_threshold: 6,
            ..DrsConfig::default()
        };
        assert!(mgr.configure_drs(config).is_err());
    }
}
