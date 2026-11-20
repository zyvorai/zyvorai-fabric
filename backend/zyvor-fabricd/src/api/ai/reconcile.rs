// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: LicenseRef-Zyvor-Production-1.0

//! Inference deployment reconciler (preview).
//!
//! Places a free NVIDIA GPU, binds it via FluxVM, creates a QEMU VM with
//! VFIO + bridged tap + cloud-init vLLM unit, probes guest health, then
//! upserts Maglev backends. Scale-down / delete drains Maglev, deletes the
//! VM, and releases the GPU.
//!
//! Without a GPU or when FluxVM inventory is empty, the deployment stays
//! `Pending` with a clear message — suitable for GPU-less smoke tests of the
//! REST layer. Set `FLUXVM_AI_DRY_RUN=1` to skip bind/create and only update
//! status bookkeeping.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex, OnceLock};
use vm_model::{BindMount, CloudInitFile, VMStartOptions, VM};
use zyvor_fabric_fluxvm_client::{
    FluxVmClient, GpuBindRequest, GpuReleaseRequest, HostGpu, NetworkServiceSpec, VmRecord,
};

use crate::server::AppState;

use super::ipam;
use super::maglev::{build_maglev_service_spec, drain_backend};
use super::placement::place_gpus;
use super::types::{
    InferenceDeployment, InferenceEndpoint, InferenceProfile, InferenceReplica, ModelArtifact,
};
use super::{
    fluxvm_client, STORE_DEPLOYMENTS, STORE_ENDPOINTS, STORE_MODELS, STORE_NODES, STORE_PROFILES,
};

const GUEST_MODEL_PATH: &str = "/models";
const VLLM_UNIT_PATH: &str = "/etc/systemd/system/vllm.service";
const HEALTH_PATH: &str = "/health";
const STORE_GPU_RESERVATIONS: &str = "ai_gpu_reservations";
const STORE_MANAGED_VMS: &str = "ai_managed_vms";
const STORE_MAGLEV: &str = "ai_maglev_services";
const STORE_DEPLOYMENT_LEASES: &str = "ai_deployment_leases";
const STORE_ENDPOINT_LEASES: &str = "ai_endpoint_leases";

#[derive(Debug, Clone, Serialize, Deserialize)]
struct GpuReservation {
    bdf: String,
    deployment: String,
    vm_name: String,
    #[serde(default)]
    owner: String,
    #[serde(default)]
    created_unix: i64,
    /// `0` after the VM is running. Until then the reservation expires.
    #[serde(default)]
    expires_unix: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ManagedVm {
    vm_name: String,
    deployment: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct MaglevRecord {
    endpoint: String,
    service: String,
}

fn deployment_locks() -> &'static Mutex<HashMap<String, Arc<tokio::sync::Mutex<()>>>> {
    static LOCKS: OnceLock<Mutex<HashMap<String, Arc<tokio::sync::Mutex<()>>>>> = OnceLock::new();
    LOCKS.get_or_init(|| Mutex::new(HashMap::new()))
}

fn deployment_lock(name: &str) -> Arc<tokio::sync::Mutex<()>> {
    let mut map = deployment_locks().lock().unwrap_or_else(|e| e.into_inner());
    map.entry(name.to_string())
        .or_insert_with(|| Arc::new(tokio::sync::Mutex::new(())))
        .clone()
}

/// Serialize create, scale, autoscaler, and the periodic loop for one deployment.
/// The in-process mutex is the fast path. The file lease excludes another
/// fabricd on the same state directory. It is not an etcd lease.
pub async fn locked_reconcile(state: &AppState, name: &str) -> Result<(), String> {
    let lock = deployment_lock(name);
    let _guard = lock.lock().await;
    let _lease = hold_reconcile_lease(
        state,
        STORE_DEPLOYMENT_LEASES,
        name,
        &format!("deployment/{name}"),
    )?;
    reconcile_deployment(state, name).await
}

/// Periodic reconciler. The first tick is startup recovery.
pub async fn run_ai_reconcile_controller(state: Arc<AppState>) {
    let mut interval = tokio::time::interval(std::time::Duration::from_secs(15));
    let sem = Arc::new(tokio::sync::Semaphore::new(4));
    loop {
        interval.tick().await;
        recover_reservations(&state).await;
        let names: Vec<String> = state
            .store
            .list_entities::<InferenceDeployment>(STORE_DEPLOYMENTS)
            .unwrap_or_default()
            .into_iter()
            .map(|d| d.name)
            .collect();
        let mut tasks = Vec::new();
        for name in names {
            let state = state.clone();
            let sem = sem.clone();
            tasks.push(tokio::spawn(async move {
                let Ok(_permit) = sem.acquire().await else {
                    return;
                };
                if let Err(e) = locked_reconcile(&state, &name).await {
                    tracing::warn!(deployment = %name, "AI reconcile tick: {e}");
                    record_reconcile_error(&state, &name, e);
                }
            }));
        }
        for task in tasks {
            let _ = task.await;
        }
        retry_deleting_endpoints(&state).await;
        sweep_orphans(&state).await;
    }
}

/// Spawn a background reconcile for a deployment (create / scale).
pub fn enqueue_deployment_reconcile(state: Arc<AppState>, name: String) {
    tokio::spawn(async move {
        if let Err(err) = super::raft::start(std::path::Path::new(&state.config.storage.path)).await
        {
            tracing::error!("AI raft member did not start: {err}");
        }
        if !super::raft::may_place() {
            return;
        }
        if let Err(e) = locked_reconcile(&state, &name).await {
            tracing::warn!(deployment = %name, "AI reconcile failed: {e}");
            record_reconcile_error(&state, &name, e);
        }
    });
}

fn record_reconcile_error(state: &AppState, name: &str, error: String) {
    if let Ok(Some(mut dep)) = state
        .store
        .get_entity::<InferenceDeployment>(STORE_DEPLOYMENTS, name)
    {
        dep.status.phase = if is_hard_failure(&error) {
            "Failed".into()
        } else if is_retryable_fluxvm(&error) {
            "Pending".into()
        } else {
            "Failed".into()
        };
        dep.status.message = Some(error);
        dep.updated = Utc::now();
        let _ = state.store.save_entity(STORE_DEPLOYMENTS, name, &dep);
    }
}

/// FluxVM connect and timeout errors are retried. A missing profile or model is not.
pub fn is_retryable_fluxvm(error: &str) -> bool {
    if is_hard_failure(error) {
        return false;
    }
    let lower = error.to_ascii_lowercase();
    lower.contains("timed out")
        || lower.contains("timeout")
        || lower.contains("connection")
        || lower.contains("connect")
        || lower.contains("temporarily")
        || lower.contains("lease held")
        || lower.contains("no local_path")
}

pub fn is_hard_failure(error: &str) -> bool {
    let lower = error.to_ascii_lowercase();
    (lower.contains("profile") || lower.contains("model")) && lower.contains("not found")
}

/// Three failed probes replace the replica. A success clears the streak.
pub fn note_health(streak: u32, healthy: bool) -> (u32, bool) {
    if healthy {
        (0, false)
    } else {
        let next = streak.saturating_add(1);
        (next, next >= 3)
    }
}

/// Release the VIP when Maglev is confirmed absent. A successful delete is not
/// required: DELETE 404 and a follow-up GET 404 both mean the service is gone.
pub fn should_release_vip(confirmed_absent: bool) -> Result<(), &'static str> {
    if !confirmed_absent {
        return Err("Maglev service still present; VIP retained");
    }
    Ok(())
}

pub fn is_fluxvm_not_found(err: &str) -> bool {
    let lower = err.to_ascii_lowercase();
    lower.contains("404") || lower.contains("not found")
}

/// `get_ok` means the service was read and is still present.
/// Absence is a not-found GET, a not-found DELETE, or a DELETE that succeeded
/// when the GET does not show the service is still there.
pub fn maglev_is_absent(delete_error: Option<&str>, get_ok: bool, get_error: Option<&str>) -> bool {
    if get_ok {
        return false;
    }
    if get_error.is_some_and(is_fluxvm_not_found) {
        return true;
    }
    delete_error.is_none() || delete_error.is_some_and(is_fluxvm_not_found)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EndpointTeardownPlan {
    pub release_vip: bool,
    pub delete_endpoint: bool,
}

/// Already-absent Maglev (DELETE or GET 404) releases the VIP and drops the endpoint.
pub fn plan_endpoint_teardown(
    delete_error: Option<&str>,
    get_ok: bool,
    get_error: Option<&str>,
) -> Result<EndpointTeardownPlan, &'static str> {
    should_release_vip(maglev_is_absent(delete_error, get_ok, get_error))?;
    Ok(EndpointTeardownPlan {
        release_vip: true,
        delete_endpoint: true,
    })
}

/// First unused ordinal. Identity is this number, not `replicas.len()`.
pub fn next_replica_ordinal(replicas: &[InferenceReplica]) -> u32 {
    let used: HashSet<u32> = replicas.iter().map(|r| r.ordinal).collect();
    (0..).find(|n| !used.contains(n)).unwrap()
}

pub fn replica_vm_name(deployment: &str, ordinal: u32) -> String {
    format!("{deployment}-{ordinal}")
}

pub fn replica_id_for(deployment: &str, ordinal: u32) -> String {
    format!("{deployment}-r{ordinal:06}")
}

/// Old records have no `replica_id`. Fill it from the VM name so restart
/// keeps the same ordinal instead of treating every replica as 0.
pub fn ensure_replica_identity(deployment: &str, rep: &mut InferenceReplica) {
    if !rep.replica_id.is_empty() {
        return;
    }
    if let Some(n) = ordinal_from_vm_name(deployment, &rep.vm_name) {
        rep.ordinal = n;
    }
    rep.replica_id = replica_id_for(deployment, rep.ordinal);
}

fn ordinal_from_vm_name(deployment: &str, vm_name: &str) -> Option<u32> {
    let rest = vm_name.strip_prefix(&format!("{deployment}-"))?;
    if rest.is_empty() || !rest.chars().all(|c| c.is_ascii_digit()) {
        return None;
    }
    rest.parse().ok()
}

/// Spawn a background Maglev upsert for an endpoint.
pub fn enqueue_endpoint_reconcile(state: Arc<AppState>, name: String) {
    tokio::spawn(async move {
        if let Err(e) = reconcile_endpoint(&state, &name).await {
            tracing::warn!(endpoint = %name, "AI endpoint reconcile failed: {e}");
        }
    });
}

/// Mean error rate and time-to-first-token across ready replicas that have metrics.
pub fn mean_replica_signals(replicas: &[InferenceReplica]) -> (f64, f64) {
    let ready: Vec<_> = replicas
        .iter()
        .filter(|r| r.ready)
        .filter_map(|r| r.metrics.as_ref())
        .collect();
    if ready.is_empty() {
        return (0.0, 0.0);
    }
    let n = ready.len() as f64;
    let err = ready.iter().map(|m| m.http_error_rate).sum::<f64>() / n;
    let ttft = ready.iter().map(|m| m.ttft_ms).sum::<f64>() / n;
    (err, ttft)
}

pub async fn reconcile_deployment(state: &AppState, name: &str) -> Result<(), String> {
    let mut dep: InferenceDeployment = state
        .store
        .get_entity(STORE_DEPLOYMENTS, name)
        .map_err(|e| e.to_string())?
        .ok_or_else(|| format!("deployment '{name}' not found"))?;
    for rep in &mut dep.status.replicas {
        ensure_replica_identity(&dep.name, rep);
    }

    let profile: InferenceProfile = state
        .store
        .get_entity(STORE_PROFILES, &dep.profile)
        .map_err(|e| e.to_string())?
        .ok_or_else(|| format!("profile '{}' not found", dep.profile))?;

    if profile.gpu.count == 0
        || profile.gpu.count > 8
        || dep.gpus_per_replica != profile.gpu.count.max(1)
    {
        return Err(format!(
            "gpu.count must be 1..=8 and match gpus_per_replica (profile {}, deployment {})",
            profile.gpu.count, dep.gpus_per_replica
        ));
    }
    super::runtime::require_supported(&profile.runtime)?;

    let model: ModelArtifact = state
        .store
        .get_entity(STORE_MODELS, &dep.model)
        .map_err(|e| e.to_string())?
        .ok_or_else(|| format!("model '{}' not found", dep.model))?;

    let model_host = model
        .local_path
        .clone()
        .ok_or_else(|| "model has no local_path; re-create ModelArtifact".to_string())?;

    replace_replicas_on_lost_nodes(state, &mut dep).await;

    // Scale down first. A rollout keeps the previous replica set, and extra
    // replicas are chosen from the revision that is no longer current.
    let now_unix = Utc::now().timestamp();
    let retain = super::revisions::latest_rollout(state, &dep.name)
        .as_ref()
        .is_some_and(|run| super::revisions::retain_second_fleet(run, now_unix));
    if !retain {
        while dep.status.replicas.len() > dep.replicas as usize {
            let idx = dep
                .status
                .replicas
                .iter()
                .rposition(|r| r.revision != 0 && r.revision != dep.revision)
                .unwrap_or(dep.status.replicas.len() - 1);
            let victim = dep.status.replicas.remove(idx);
            if let Err(e) = remove_replica(state, &dep, &victim).await {
                tracing::warn!("scale-down replica {}: {e}", victim.vm_name);
                dep.status
                    .replicas
                    .insert(idx.min(dep.status.replicas.len()), victim);
                break;
            }
        }
    }

    let dry_run = std::env::var("FLUXVM_AI_DRY_RUN")
        .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
        .unwrap_or(false);

    // Drop replicas whose VM disappeared, then refill guest IPs before scale-up.
    if !dry_run {
        if let Ok(client) = fluxvm_client(state) {
            if let Ok(vms) = client.list_vms().await {
                let live: HashSet<String> = vms.iter().map(|v| v.name.clone()).collect();
                let mut kept = Vec::new();
                for rep in std::mem::take(&mut dep.status.replicas) {
                    if live.contains(&rep.vm_name) {
                        kept.push(rep);
                    } else if let Err(e) = release_missing_vm(state, &client, &rep).await {
                        dep.status.message = Some(e);
                        kept.push(rep);
                    }
                }
                for rep in &mut kept {
                    if rep.address.is_none() {
                        if let Some(vm) = vms.iter().find(|v| v.name == rep.vm_name) {
                            if let Some(ip) = vm.guest_ip.clone() {
                                rep.address = Some(ip);
                            }
                        }
                    }
                }
                dep.status.replicas = kept;
            }
        }
    }

    if !dry_run {
        apply_health(state, &mut dep).await;
    }

    // Scale up. The ordinal is the first unused number, not `replicas.len()`.
    while dep.status.replicas.len() < dep.replicas as usize {
        let ordinal = next_replica_ordinal(&dep.status.replicas);
        match create_replica(state, &dep, &profile, &model_host, ordinal, dry_run).await {
            Ok(rep) => dep.status.replicas.push(rep),
            Err(e) => {
                dep.status.phase = "Pending".into();
                dep.status.message = Some(e.clone());
                dep.updated = Utc::now();
                state
                    .store
                    .save_entity(STORE_DEPLOYMENTS, &dep.name, &dep)
                    .map_err(|se| se.to_string())?;
                return Err(e);
            }
        }
    }

    let ready = dep.status.replicas.iter().filter(|r| r.ready).count();
    dep.status.phase = if ready == dep.replicas as usize && dep.replicas > 0 {
        "Ready".into()
    } else if dry_run {
        "DryRun".into()
    } else {
        "Pending".into()
    };
    dep.status.message = Some(format!(
        "{ready}/{} replicas ready{}",
        dep.replicas,
        if dry_run { " (FLUXVM_AI_DRY_RUN)" } else { "" }
    ));
    dep.updated = Utc::now();
    super::revisions::sync_replica_sets(state, &mut dep);
    if let Some(mut run) = super::revisions::active_rollout(state, &dep.name) {
        if matches!(
            run.strategy,
            super::types::RolloutStrategy::BlueGreen | super::types::RolloutStrategy::Canary
        ) {
            let mut new_count = dep
                .status
                .replicas
                .iter()
                .filter(|r| r.revision == run.to_revision)
                .count();
            let cap = (dep.replicas as usize).saturating_mul(2);
            while new_count < dep.replicas as usize && dep.status.replicas.len() < cap {
                let ordinal = next_replica_ordinal(&dep.status.replicas);
                match create_replica(state, &dep, &profile, &model_host, ordinal, dry_run).await {
                    Ok(mut rep) => {
                        rep.revision = run.to_revision;
                        rep.deployment = dep.name.clone();
                        dep.status.replicas.push(rep);
                        new_count += 1;
                    }
                    Err(e) => {
                        dep.status.message = Some(e);
                        break;
                    }
                }
            }
        }
        let (mean_error, mean_ttft) = mean_replica_signals(&dep.status.replicas);
        let promote = super::revisions::advance_rollout(
            &mut run,
            &mut dep.status.replicas,
            Utc::now().timestamp(),
            dep.replicas,
            mean_error,
            mean_ttft,
        );
        if promote {
            if let Some(rev) = super::revisions::list_revisions(state, &dep.name)
                .into_iter()
                .find(|r| r.revision == run.to_revision)
            {
                dep.model = rev.model;
                dep.revision = rev.revision;
            }
            super::revisions::note_revision_states(
                state,
                &dep.name,
                run.from_revision,
                run.to_revision,
                super::types::RevisionState::Superseded,
                super::types::RevisionState::Active,
            );
        }
        run.updated_at = Utc::now();
        let _ = state
            .store
            .save_entity(super::STORE_ROLLOUTS, &run.id, &run);
        if run.message.as_deref() == Some("create one new-revision replica")
            && dep.status.replicas.len() < dep.replicas as usize + run.max_surge.max(1) as usize
        {
            dep.status.message = run.message.clone();
            let ordinal = next_replica_ordinal(&dep.status.replicas);
            if let Ok(mut rep) =
                create_replica(state, &dep, &profile, &model_host, ordinal, dry_run).await
            {
                rep.revision = run.to_revision;
                rep.deployment = dep.name.clone();
                dep.status.replicas.push(rep);
            }
        } else if run.message.as_deref() == Some("remove one drained old replica") {
            if let Some(idx) = dep
                .status
                .replicas
                .iter()
                .position(|r| r.revision == run.from_revision && r.draining)
            {
                let victim = dep.status.replicas.remove(idx);
                if super::revisions::protect_last_healthy(
                    dep.status
                        .replicas
                        .iter()
                        .filter(|r| r.revision == run.from_revision && r.ready && !r.draining)
                        .count() as u32,
                    dep.status
                        .replicas
                        .iter()
                        .filter(|r| r.revision == run.to_revision && r.ready)
                        .count() as u32,
                ) {
                    dep.status.replicas.insert(idx, victim);
                } else if let Err(e) = remove_replica(state, &dep, &victim).await {
                    tracing::warn!("rollout remove {}: {e}", victim.vm_name);
                    dep.status.replicas.insert(idx, victim);
                }
            }
        } else if let Some(msg) = run.message.clone() {
            dep.status.message = Some(msg);
        }
        super::revisions::sync_replica_sets(state, &mut dep);
    }
    state
        .store
        .save_entity(STORE_DEPLOYMENTS, &dep.name, &dep)
        .map_err(|e| e.to_string())?;

    // Refresh any endpoints that point at this deployment.
    let endpoints: Vec<InferenceEndpoint> = state
        .store
        .list_entities(STORE_ENDPOINTS)
        .unwrap_or_default();
    for ep in endpoints.into_iter().filter(|e| e.deployment == dep.name) {
        let _ = reconcile_endpoint(state, &ep.name).await;
    }

    Ok(())
}

fn registered_nodes(state: &AppState) -> Vec<super::types::InferenceNode> {
    state.store.list_entities(STORE_NODES).unwrap_or_default()
}

fn site_replica_counts(
    dep: &InferenceDeployment,
    nodes: &[super::types::InferenceNode],
) -> Vec<(String, u32)> {
    let mut counts: Vec<(String, u32)> = Vec::new();
    for rep in &dep.status.replicas {
        let site = rep.site.clone().filter(|s| !s.is_empty()).or_else(|| {
            nodes
                .iter()
                .find(|n| n.id == rep.host)
                .map(|n| n.site.clone())
                .filter(|s| !s.is_empty())
        });
        let Some(site) = site else { continue };
        if let Some((_, n)) = counts.iter_mut().find(|(s, _)| s == &site) {
            *n = n.saturating_add(1);
        } else {
            counts.push((site, 1));
        }
    }
    counts
}

pub(crate) fn schedule_request(
    state: &AppState,
    dep: &InferenceDeployment,
    profile: &InferenceProfile,
) -> super::scheduler::ScheduleRequest {
    let nodes = registered_nodes(state);
    let deployments: Vec<InferenceDeployment> = state
        .store
        .list_entities(STORE_DEPLOYMENTS)
        .unwrap_or_default();
    let mut allocated = Vec::new();
    for other in &deployments {
        for rep in &other.status.replicas {
            if !rep.host.is_empty() && !rep.bdf.is_empty() {
                allocated.push((rep.host.clone(), rep.bdf.clone()));
            }
        }
    }
    let occupied_domains = dep
        .status
        .replicas
        .iter()
        .filter_map(|rep| nodes.iter().find(|n| n.id == rep.host))
        .map(|n| n.failure_domain.clone())
        .filter(|d| !d.is_empty())
        .collect();
    super::scheduler::ScheduleRequest {
        vendor: profile.gpu.vendor.clone(),
        minimum_vram_gib: profile.gpu.minimum_vram_gib,
        preferred_site: dep.preferred_site.clone(),
        residency: dep.residency.clone(),
        model: dep.model.clone(),
        cpu: profile.cpu,
        memory_gib: profile.memory_gib,
        allocated,
        occupied_domains,
        allowed_sites: dep.allowed_sites.clone(),
        max_replicas_per_site: dep.max_replicas_per_site,
        site_counts: site_replica_counts(dep, &nodes),
        mig_profile: String::new(),
        require_nvlink: false,
    }
}

async fn replace_replicas_on_lost_nodes(state: &AppState, dep: &mut InferenceDeployment) {
    let nodes = registered_nodes(state);
    if nodes.is_empty() {
        return;
    }
    let now = Utc::now().timestamp();
    let mut kept = Vec::new();
    for rep in std::mem::take(&mut dep.status.replicas) {
        if super::scheduler::should_replace_lost(&nodes, &rep.host, now) {
            if let Err(e) = remove_replica(state, dep, &rep).await {
                tracing::warn!("node-loss replace {}: {e}", rep.vm_name);
                kept.push(rep);
            }
        } else {
            kept.push(rep);
        }
    }
    dep.status.replicas = kept;
}

async fn create_replica(
    state: &AppState,
    dep: &InferenceDeployment,
    profile: &InferenceProfile,
    model_host: &str,
    ordinal: u32,
    dry_run: bool,
) -> Result<InferenceReplica, String> {
    let vm_name = replica_vm_name(&dep.name, ordinal);
    let replica_id = replica_id_for(&dep.name, ordinal);
    crate::validation::validate_vm_name(&vm_name).map_err(|(_, m)| m)?;

    let client = match fluxvm_client(state) {
        Ok(c) => c,
        Err((_, body)) => {
            return Err(body
                .0
                .get("error")
                .and_then(|v| v.as_str())
                .unwrap_or("FluxVM client error")
                .to_string());
        }
    };

    let inventory = client.list_host_gpus().await.unwrap_or_default();
    let nvidia = inventory
        .iter()
        .any(|gpu| super::janus::is_nvidia(gpu.vendor_id, &gpu.vendor));
    if !nvidia {
        if let Some(url) = super::janus::janus_url() {
            super::janus::sync_nodes(state, &url).await?;
        }
    }
    let gpu_req = profile.gpu.clone();
    let scheduled = super::scheduler::decide(
        &registered_nodes(state),
        &schedule_request(state, dep, profile),
        Utc::now().timestamp(),
    )?;
    let scheduled_host = match &scheduled {
        super::scheduler::Placement::Legacy => String::new(),
        super::scheduler::Placement::Chosen(choice) => choice.node_id.clone(),
    };
    let scheduled_bdf = match &scheduled {
        super::scheduler::Placement::Legacy => None,
        super::scheduler::Placement::Chosen(choice) => Some(choice.bdf.clone()),
    };

    if !nvidia {
        if let Some(url) = super::janus::janus_url() {
            let site = dep.preferred_site.clone().or_else(|| Some("janus".into()));
            let ready = scheduled_bdf.is_some();
            return Ok(InferenceReplica {
                replica_id,
                ordinal,
                vm_name,
                bdf: scheduled_bdf.unwrap_or_else(|| "janus:unplaced".into()),
                ready,
                address: Some(super::janus::replica_hostport(&url)),
                metrics: None,
                maglev_weight: None,
                site,
                cost_tier: Some(1),
                draining: false,
                unhealthy_streak: 0,
                deployment: String::new(),
                revision: 0,
                model_digest: String::new(),
                profile_digest: String::new(),
                host: scheduled_host,
                generation: 0,
                lifecycle: String::new(),
                health: String::new(),
                created_at: None,
            });
        }
    }

    if dry_run {
        let site = dep
            .preferred_site
            .clone()
            .or_else(|| std::env::var("FLUXVM_AI_SITE").ok());
        return Ok(InferenceReplica {
            replica_id,
            ordinal,
            vm_name,
            bdf: scheduled_bdf.unwrap_or_else(|| format!("dry-run-{ordinal}")),
            ready: true,
            address: Some(format!("10.255.0.{}", ordinal + 1)),
            metrics: None,
            maglev_weight: None,
            site,
            cost_tier: Some(1),
            draining: false,
            unhealthy_streak: 0,
            deployment: String::new(),
            revision: 0,
            model_digest: String::new(),
            profile_digest: String::new(),
            host: scheduled_host,
            generation: 0,
            lifecycle: String::new(),
            health: String::new(),
            created_at: None,
        });
    }

    if inventory.is_empty() {
        return Err(
            "no host GPUs available from FluxVM (set FLUXVM_AI_DRY_RUN=1 to exercise REST without GPUs)"
                .into(),
        );
    }
    if let Some(ref want) = scheduled_bdf {
        if !inventory.iter().any(|g| g.bdf.eq_ignore_ascii_case(want)) {
            return Err(format!(
                "scheduled onto node {scheduled_host} GPU {want}, which this FluxVM inventory does not have"
            ));
        }
    }

    let mut reserved: Vec<(String, Option<u32>)> = Vec::new();
    for _ in 0..32 {
        reserved.clear();
        let mut allocated = allocated_bdfs(state);
        if gpu_req.count <= 1 {
            if let Some(ref want) = scheduled_bdf {
                for gpu in &inventory {
                    if !gpu.bdf.eq_ignore_ascii_case(want) {
                        allocated.insert(gpu.bdf.to_ascii_lowercase());
                    }
                }
            }
        }
        let picked = place_gpus(&inventory, &gpu_req, &allocated)?;
        if let Some(ref want) = scheduled_bdf {
            if !picked.iter().any(|gpu| gpu.bdf.eq_ignore_ascii_case(want)) {
                return Err(format!(
                    "scheduled GPU {want} was not in the reserved group"
                ));
            }
        }
        let mut conflict = false;
        for gpu in &picked {
            let bdf = gpu.bdf.clone();
            let now = Utc::now().timestamp();
            let reservation = GpuReservation {
                bdf: bdf.clone(),
                deployment: dep.name.clone(),
                vm_name: vm_name.clone(),
                owner: fabricd_owner().to_string(),
                created_unix: now,
                expires_unix: now.saturating_add(ipam::RESERVATION_TTL_SECS),
            };
            match state
                .store
                .try_create_entity(STORE_GPU_RESERVATIONS, &bdf, &reservation)
            {
                Ok(_) => reserved.push((bdf, gpu.vram_gib)),
                Err(e) if state_store::is_entity_conflict(&e) => {
                    let _ = release_stale_gpu_reservation(state, &bdf);
                    for (held, _) in reserved.drain(..) {
                        let _ = state.store.delete_entity(STORE_GPU_RESERVATIONS, &held);
                    }
                    conflict = true;
                    break;
                }
                Err(e) => return Err(e.to_string()),
            }
        }
        if !conflict {
            break;
        }
    }
    let need = gpu_req.count.max(1) as usize;
    if reserved.len() != need {
        return Err("could not reserve a free GPU group".into());
    }

    for (bdf, vram_gib) in &reserved {
        if let Err(e) = client
            .bind_host_gpu(&GpuBindRequest {
                bdf: bdf.clone(),
                vram_gib: *vram_gib,
            })
            .await
        {
            for (held, _) in &reserved {
                let _ = client
                    .release_host_gpu(&GpuReleaseRequest {
                        bdf: held.clone(),
                        restore_driver: false,
                    })
                    .await;
                let _ = state.store.delete_entity(STORE_GPU_RESERVATIONS, held);
            }
            return Err(format!("GPU bind {bdf}: {e}"));
        }
    }
    let bdf = reserved[0].0.clone();
    let vfio_devices: Vec<String> = reserved.iter().map(|(bdf, _)| bdf.clone()).collect();

    let image = std::env::var("FLUXVM_AI_IMAGE").unwrap_or_else(|_| {
        format!(
            "{}/ai-vllm-cuda.qcow2",
            state.config.storage.image_path.trim_end_matches('/')
        )
    });

    let mut labels = std::collections::HashMap::new();
    labels.insert("role".into(), "ai-inference".into());
    labels.insert("deployment".into(), dep.name.clone());
    if let Some(ref t) = dep.tenant {
        labels.insert("tenant".into(), t.clone());
    }

    let exec = super::runtime::guest_exec(&profile.runtime, GUEST_MODEL_PATH, need as u32)?;
    let vllm_unit = inference_systemd_unit(&exec);
    let vm = VM {
        name: vm_name.clone(),
        state: vm_model::VMState::Stopped,
        cpus: profile.cpu,
        memory: (profile.memory_gib as u64).saturating_mul(1024),
        disk: 40,
        image: image.clone(),
        ip: None,
        pid: None,
        mac_address: None,
        hostname: Some(vm_name.clone()),
        tags: Some(vec!["ai".into(), "inference".into()]),
        labels: Some(labels),
        vnc_port: None,
        created: Utc::now(),
        updated: None,
        last_error: None,
        port_forwards: vec![],
        network_tap: true, // bridged tap — not direct
        network_static_ip: false,
        direct_uplink: None,
        direct_mode: None,
        direct_guest_ips: vec![],
        ssh_authorized_keys: vec![],
        cloud_init_packages: vec![],
        cloud_init_runcmd: vec![
            "systemctl daemon-reload".into(),
            "systemctl enable --now vllm.service".into(),
        ],
        cloud_init_write_files: vec![CloudInitFile {
            path: VLLM_UNIT_PATH.into(),
            content: vllm_unit,
            permissions: Some("0644".into()),
        }],
        storage: None,
        enable_qga: false,
        hyperv: false,
    };

    state.store.save_vm(&vm).map_err(|e| e.to_string())?;

    let opts = VMStartOptions {
        network_tap: true,
        bind_mounts: vec![BindMount {
            source: model_host.to_string(),
            destination: Some(GUEST_MODEL_PATH.into()),
            read_only: true,
        }],
        vfio_devices,
        cloud_init_write_files: vm.cloud_init_write_files.clone(),
        cloud_init_runcmd: vm.cloud_init_runcmd.clone(),
        ssh_authorized_keys: vec!["ssh-ed25519 AAAA fabric-ai-placeholder".into()],
        ..Default::default()
    };

    if let Err(e) = state.driver.start_with_options(&vm, &opts).await {
        for (held, _) in &reserved {
            match client
                .release_host_gpu(&GpuReleaseRequest {
                    bdf: held.clone(),
                    restore_driver: false,
                })
                .await
            {
                Ok(_) => {
                    let _ = state.store.delete_entity(STORE_GPU_RESERVATIONS, held);
                }
                Err(release_err) => {
                    let _ = state.store.delete_vm(&vm_name);
                    return Err(format!(
                        "start VM {vm_name}: {e}; GPU release {held} failed: {release_err}"
                    ));
                }
            }
        }
        let _ = state.store.delete_vm(&vm_name);
        return Err(format!("start VM {vm_name}: {e}"));
    }

    let _ = state.store.save_entity(
        STORE_MANAGED_VMS,
        &vm_name,
        &ManagedVm {
            vm_name: vm_name.clone(),
            deployment: dep.name.clone(),
        },
    );
    for (held, _) in &reserved {
        let _ = state.store.save_entity(
            STORE_GPU_RESERVATIONS,
            held,
            &GpuReservation {
                bdf: held.clone(),
                deployment: dep.name.clone(),
                vm_name: vm_name.clone(),
                owner: fabricd_owner().to_string(),
                created_unix: Utc::now().timestamp(),
                expires_unix: 0,
            },
        );
    }

    // Best-effort guest IP from FluxVM record.
    let address = match client.find_by_name(&vm_name).await {
        Ok(Some(rec)) => rec.guest_ip,
        _ => None,
    };

    Ok(InferenceReplica {
        replica_id,
        ordinal,
        vm_name,
        bdf,
        ready: false,
        address,
        metrics: None,
        maglev_weight: None,
        site: dep
            .preferred_site
            .clone()
            .or_else(|| std::env::var("FLUXVM_AI_SITE").ok()),
        cost_tier: Some(1),
        draining: false,
        unhealthy_streak: 0,
        deployment: dep.name.clone(),
        revision: 0,
        model_digest: String::new(),
        profile_digest: String::new(),
        host: scheduled_host,
        generation: 0,
        lifecycle: "provisioning".into(),
        health: "unknown".into(),
        created_at: Some(Utc::now()),
    })
}

async fn remove_replica(
    state: &AppState,
    dep: &InferenceDeployment,
    rep: &InferenceReplica,
) -> Result<(), String> {
    // Drain Maglev backends that reference this replica.
    let endpoints: Vec<InferenceEndpoint> = state
        .store
        .list_entities::<InferenceEndpoint>(STORE_ENDPOINTS)
        .unwrap_or_default()
        .into_iter()
        .filter(|e| e.deployment == dep.name)
        .collect();

    if let Ok(client) = fluxvm_client(state) {
        for ep in &endpoints {
            if let Ok(mut spec) = client.get_network_service(&ep.name).await {
                if let Some(addr) = &rep.address {
                    drain_backend(&mut spec, addr);
                    let _ = client.upsert_network_service(&spec).await;
                }
            }
        }

        if let Err(e) = state.driver.delete(&rep.vm_name).await {
            tracing::warn!("delete VM {}: {e}", rep.vm_name);
        }
        let _ = state.store.delete_vm(&rep.vm_name);

        if !rep.bdf.is_empty() && !rep.bdf.starts_with("dry-run-") {
            release_vm_gpus(state, &client, &rep.vm_name, &rep.bdf).await?;
        }
        let _ = state.store.delete_entity(STORE_MANAGED_VMS, &rep.vm_name);
    } else {
        let _ = state.driver.delete(&rep.vm_name).await;
        let _ = state.store.delete_vm(&rep.vm_name);
    }
    Ok(())
}

pub async fn teardown_deployment(
    state: &AppState,
    dep: &InferenceDeployment,
) -> Result<(), String> {
    for rep in &dep.status.replicas {
        remove_replica(state, dep, rep).await?;
    }
    Ok(())
}

pub async fn reconcile_endpoint(state: &AppState, name: &str) -> Result<(), String> {
    let _lease = acquire_endpoint_lease(state, name)?;
    let mut ep: InferenceEndpoint = state
        .store
        .get_entity(STORE_ENDPOINTS, name)
        .map_err(|e| e.to_string())?
        .ok_or_else(|| format!("endpoint '{name}' not found"))?;

    let dep: InferenceDeployment = state
        .store
        .get_entity(STORE_DEPLOYMENTS, &ep.deployment)
        .map_err(|e| e.to_string())?
        .ok_or_else(|| format!("deployment '{}' not found", ep.deployment))?;

    let profile: Option<InferenceProfile> = state
        .store
        .get_entity(STORE_PROFILES, &dep.profile)
        .ok()
        .flatten();
    let max_egress = profile.as_ref().and_then(|p| p.max_egress_mbps);

    let vip = if let Some(existing) = ep.vip.clone() {
        ipam::adopt(state, &ep.name, &existing)?
    } else {
        ipam::reserve(state, &ep.name)?
    };

    let spec = build_maglev_service_spec(&ep.name, &vip, ep.port, &dep.status.replicas, max_egress);

    // Janus (and any non-IP upstream) is proxied by the OpenAI gateway. Maglev
    // only accepts IP backends, so an empty table means leave VIP reserved and
    // skip the FluxVM upsert instead of logging a 422 every tick. Janus BDFs
    // also skip Maglev even when the hostport parses as 127.0.0.1:port — the
    // lab FluxVM dataplane is not the path for that simulator. Dry-run BDFs
    // (`dry-run-*`) are synthetic and must not hit Maglev either.
    let serving: Vec<_> = dep
        .status
        .replicas
        .iter()
        .filter(|r| super::eligibility::replica_serving(r))
        .collect();
    let skip_maglev = !serving.is_empty()
        && serving
            .iter()
            .all(|r| super::janus::is_janus_bdf(&r.bdf) || r.bdf.starts_with("dry-run-"));
    if spec.backends.is_empty() || skip_maglev {
        ep.vip = Some(vip);
        ep.updated = Utc::now();
        let _ = state.store.save_entity(STORE_ENDPOINTS, &ep.name, &ep);
        ipam::pin(state, &ep.name);
        return Ok(());
    }

    let client = match fluxvm_client(state) {
        Ok(c) => c,
        Err(_) => {
            // GPU-less / FluxVM-down: persist desired VIP and leave service_id unset.
            ep.vip = Some(vip);
            ep.updated = Utc::now();
            let _ = state.store.save_entity(STORE_ENDPOINTS, &ep.name, &ep);
            ipam::pin(state, &ep.name);
            return Ok(());
        }
    };

    match client.upsert_network_service(&spec).await {
        Ok(status) => {
            ep.service_id = Some(status.service_id);
            ep.vip = Some(vip);
            ep.updated = Utc::now();
            let _ = state.store.save_entity(
                STORE_MAGLEV,
                &ep.name,
                &MaglevRecord {
                    endpoint: ep.name.clone(),
                    service: ep.name.clone(),
                },
            );
            state
                .store
                .save_entity(STORE_ENDPOINTS, &ep.name, &ep)
                .map_err(|e| e.to_string())?;
            ipam::pin(state, &ep.name);
            Ok(())
        }
        Err(e) => {
            tracing::warn!("Maglev upsert {}: {e}", ep.name);
            ep.vip = Some(vip);
            ep.updated = Utc::now();
            let _ = state.store.save_entity(STORE_ENDPOINTS, &ep.name, &ep);
            ipam::pin(state, &ep.name);
            Err(e.to_string())
        }
    }
}

pub async fn teardown_endpoint(state: &AppState, ep: &InferenceEndpoint) -> Result<(), String> {
    let _lease = acquire_endpoint_lease(state, &ep.name)?;
    let mut ep = ep.clone();
    ep.phase = "Deleting".into();
    ep.updated = Utc::now();
    state
        .store
        .save_entity(STORE_ENDPOINTS, &ep.name, &ep)
        .map_err(|e| e.to_string())?;

    let recorded = state
        .store
        .get_entity::<MaglevRecord>(STORE_MAGLEV, &ep.name)
        .ok()
        .flatten();
    if recorded.is_some() {
        let client = fluxvm_client(state).map_err(|(_, body)| {
            body.0
                .get("error")
                .and_then(|v| v.as_str())
                .unwrap_or("FluxVM unavailable; VIP retained")
                .to_string()
        })?;
        let delete_error = match client.delete_network_service(&ep.name).await {
            Ok(_) => None,
            Err(e) => Some(e.to_string()),
        };
        let (get_ok, get_error) = match client.get_network_service(&ep.name).await {
            Ok(_) => (true, None),
            Err(e) => (false, Some(e.to_string())),
        };
        plan_endpoint_teardown(delete_error.as_deref(), get_ok, get_error.as_deref())
            .map_err(|e| e.to_string())?;
    }

    let _ = state.store.delete_entity(STORE_MAGLEV, &ep.name);
    ipam::release(state, &ep.name);
    Ok(())
}

/// Same-host reconcile lease. The flock excludes another fabricd on this
/// state directory. Health replacement, scale, and autoscaling all enter
/// through `locked_reconcile`. Multi-node HA still needs a transactional store.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ReconcileLease {
    pub resource: String,
    pub owner: String,
    pub fencing_token: u64,
    pub acquired_at: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
}

struct HeldReconcileLease {
    _guard: state_store::FileLease,
}

fn hold_reconcile_lease(
    state: &AppState,
    subdir: &str,
    id: &str,
    resource: &str,
) -> Result<HeldReconcileLease, String> {
    let guard = state
        .store
        .try_acquire_lease(subdir, id, fabricd_owner(), 120, resource)
        .map_err(|e| {
            if e.to_string().contains("lease held") {
                format!("lease held: {e}")
            } else {
                e.to_string()
            }
        })?;
    let record = ReconcileLease {
        resource: guard.resource.clone(),
        owner: guard.owner.clone(),
        fencing_token: guard.fencing_token,
        acquired_at: DateTime::from_timestamp(guard.acquired_unix, 0).unwrap_or_else(Utc::now),
        expires_at: DateTime::from_timestamp(guard.expires_unix, 0).unwrap_or_else(Utc::now),
    };
    tracing::debug!(
        resource = %record.resource,
        token = record.fencing_token,
        "reconcile lease"
    );
    Ok(HeldReconcileLease { _guard: guard })
}

fn fabricd_owner() -> &'static str {
    static OWNER: OnceLock<String> = OnceLock::new();
    OWNER.get_or_init(|| format!("fabricd-{}-{}", std::process::id(), Utc::now().timestamp()))
}

fn acquire_endpoint_lease(state: &AppState, name: &str) -> Result<HeldReconcileLease, String> {
    hold_reconcile_lease(
        state,
        STORE_ENDPOINT_LEASES,
        name,
        &format!("endpoint/{name}"),
    )
}

fn replica_bdfs(state: &AppState) -> HashSet<String> {
    state
        .store
        .list_entities::<InferenceDeployment>(STORE_DEPLOYMENTS)
        .unwrap_or_default()
        .into_iter()
        .flat_map(|d| d.status.replicas.into_iter())
        .map(|r| r.bdf.to_ascii_lowercase())
        .filter(|b| !b.is_empty() && !b.starts_with("dry-run-"))
        .collect()
}

fn release_stale_gpu_reservation(state: &AppState, bdf: &str) -> bool {
    let rec = match state
        .store
        .get_entity::<GpuReservation>(STORE_GPU_RESERVATIONS, bdf)
    {
        Ok(Some(rec)) => rec,
        Ok(None) => return true,
        Err(_) => return false,
    };
    let held = replica_bdfs(state).contains(&rec.bdf.to_ascii_lowercase());
    if ipam::reservation_reclaimable(rec.expires_unix, held, Utc::now().timestamp()) {
        return state
            .store
            .delete_entity(STORE_GPU_RESERVATIONS, bdf)
            .is_ok();
    }
    false
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LiveCheck {
    Unknown,
    Active,
    Idle,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CorruptAction {
    Keep,
    Repair,
    Reclaim,
}

/// Unreadable JSON is not deleted until FluxVM shows the device is idle.
pub fn corrupt_reservation_action(check: LiveCheck) -> CorruptAction {
    match check {
        LiveCheck::Unknown => CorruptAction::Keep,
        LiveCheck::Active => CorruptAction::Repair,
        LiveCheck::Idle => CorruptAction::Reclaim,
    }
}

fn gpu_live_check(bdf: &str, gpus: &[HostGpu], vms: &[VmRecord]) -> LiveCheck {
    let active = gpus
        .iter()
        .any(|g| g.bdf.eq_ignore_ascii_case(bdf) && (g.group_bound_to_vfio || g.group_held))
        || vms.iter().any(|vm| {
            vm.request
                .vfio_devices
                .iter()
                .any(|d| d.eq_ignore_ascii_case(bdf))
        });
    if active {
        LiveCheck::Active
    } else {
        LiveCheck::Idle
    }
}

fn vip_live_check(vip: &str, services: &[NetworkServiceSpec]) -> LiveCheck {
    if services.iter().any(|s| s.vip == vip) {
        LiveCheck::Active
    } else {
        LiveCheck::Idle
    }
}

async fn recover_reservations(state: &AppState) {
    let gpu_ids = state
        .store
        .list_unreadable_entities(STORE_GPU_RESERVATIONS)
        .unwrap_or_default();
    let vip_ids = state
        .store
        .list_unreadable_entities(ipam::STORE_VIPS)
        .unwrap_or_default();
    let client = fluxvm_client(state).ok();
    let gpus = match &client {
        Some(c) => c.list_host_gpus().await.ok(),
        None => None,
    };
    let vms = match &client {
        Some(c) => c.list_vms().await.ok(),
        None => None,
    };
    let services = match &client {
        Some(c) => c.list_network_services().await.ok(),
        None => None,
    };
    for id in gpu_ids {
        let check = match (&gpus, &vms) {
            (Some(gpus), Some(vms)) => gpu_live_check(&id, gpus, vms),
            _ => LiveCheck::Unknown,
        };
        apply_corrupt_gpu(state, &id, check);
    }
    for id in vip_ids {
        let check = match &services {
            Some(services) => vip_live_check(&id, services),
            None => LiveCheck::Unknown,
        };
        apply_corrupt_vip(state, &id, check);
    }
    let now = Utc::now().timestamp();
    let held = replica_bdfs(state);
    let reservations: Vec<GpuReservation> = state
        .store
        .list_entities(STORE_GPU_RESERVATIONS)
        .unwrap_or_default();
    for rec in reservations {
        if ipam::reservation_reclaimable(
            rec.expires_unix,
            held.contains(&rec.bdf.to_ascii_lowercase()),
            now,
        ) {
            let _ = state.store.delete_entity(STORE_GPU_RESERVATIONS, &rec.bdf);
        }
    }
    let endpoints: HashSet<String> = state
        .store
        .list_entities::<InferenceEndpoint>(STORE_ENDPOINTS)
        .unwrap_or_default()
        .into_iter()
        .map(|e| e.name)
        .collect();
    let vips: Vec<ipam::VipAllocation> = state
        .store
        .list_entities(ipam::STORE_VIPS)
        .unwrap_or_default();
    for rec in vips {
        if ipam::reservation_reclaimable(rec.expires_unix, endpoints.contains(&rec.endpoint), now) {
            let _ = state.store.delete_entity(ipam::STORE_VIPS, &rec.vip);
        }
    }
}

fn apply_corrupt_gpu(state: &AppState, id: &str, check: LiveCheck) {
    match corrupt_reservation_action(check) {
        CorruptAction::Keep => {}
        CorruptAction::Reclaim => {
            let _ = state.store.delete_entity(STORE_GPU_RESERVATIONS, id);
        }
        CorruptAction::Repair => {
            let now = Utc::now().timestamp();
            let _ = state.store.save_entity(
                STORE_GPU_RESERVATIONS,
                id,
                &GpuReservation {
                    bdf: id.to_string(),
                    deployment: String::new(),
                    vm_name: String::new(),
                    owner: fabricd_owner().to_string(),
                    created_unix: now,
                    expires_unix: 0,
                },
            );
        }
    }
}

fn apply_corrupt_vip(state: &AppState, id: &str, check: LiveCheck) {
    match corrupt_reservation_action(check) {
        CorruptAction::Keep => {}
        CorruptAction::Reclaim => {
            let _ = state.store.delete_entity(ipam::STORE_VIPS, id);
        }
        CorruptAction::Repair => {
            let now = Utc::now().timestamp();
            let _ = state.store.save_entity(
                ipam::STORE_VIPS,
                id,
                &ipam::VipAllocation {
                    vip: id.to_string(),
                    endpoint: String::new(),
                    pool: "recovered".into(),
                    owner: fabricd_owner().to_string(),
                    created_unix: now,
                    expires_unix: 0,
                },
            );
        }
    }
}

fn allocated_bdfs(state: &AppState) -> HashSet<String> {
    let deps: Vec<InferenceDeployment> = state
        .store
        .list_entities(STORE_DEPLOYMENTS)
        .unwrap_or_default();
    let mut set: HashSet<String> = deps
        .into_iter()
        .flat_map(|d| d.status.replicas.into_iter())
        .filter(|r| !r.bdf.is_empty() && !r.bdf.starts_with("dry-run-"))
        .map(|r| r.bdf.to_ascii_lowercase())
        .collect();
    let reserved: Vec<GpuReservation> = state
        .store
        .list_entities(STORE_GPU_RESERVATIONS)
        .unwrap_or_default();
    for rec in reserved {
        if !rec.bdf.is_empty() && !rec.bdf.starts_with("dry-run-") {
            set.insert(rec.bdf.to_ascii_lowercase());
        }
    }
    if let Ok(unreadable) = state.store.list_unreadable_entities(STORE_GPU_RESERVATIONS) {
        for id in unreadable {
            set.insert(id.to_ascii_lowercase());
        }
    }
    set
}

async fn sweep_orphans(state: &AppState) {
    let dep_names: HashSet<String> = state
        .store
        .list_entities::<InferenceDeployment>(STORE_DEPLOYMENTS)
        .unwrap_or_default()
        .into_iter()
        .map(|d| d.name)
        .collect();
    let endpoint_names: HashSet<String> = state
        .store
        .list_entities::<InferenceEndpoint>(STORE_ENDPOINTS)
        .unwrap_or_default()
        .into_iter()
        .map(|e| e.name)
        .collect();

    let reservations: Vec<GpuReservation> = state
        .store
        .list_entities(STORE_GPU_RESERVATIONS)
        .unwrap_or_default();
    for rec in reservations {
        if rec.deployment.is_empty() || dep_names.contains(&rec.deployment) {
            continue;
        }
        let Ok(client) = fluxvm_client(state) else {
            continue;
        };
        match client
            .release_host_gpu(&GpuReleaseRequest {
                bdf: rec.bdf.clone(),
                restore_driver: false,
            })
            .await
        {
            Ok(_) => {
                if let Ok(Some(vm)) = client.find_by_name(&rec.vm_name).await {
                    let _ = client.delete_vm(vm.id).await;
                }
                let _ = state.store.delete_entity(STORE_GPU_RESERVATIONS, &rec.bdf);
            }
            Err(e) => {
                tracing::warn!(bdf = %rec.bdf, "orphan GPU release failed: {e}");
            }
        }
    }

    let managed: Vec<ManagedVm> = state
        .store
        .list_entities(STORE_MANAGED_VMS)
        .unwrap_or_default();
    for rec in managed {
        if dep_names.contains(&rec.deployment) {
            continue;
        }
        if let Ok(client) = fluxvm_client(state) {
            if let Ok(Some(vm)) = client.find_by_name(&rec.vm_name).await {
                let _ = client.delete_vm(vm.id).await;
            }
        }
        let _ = state.store.delete_entity(STORE_MANAGED_VMS, &rec.vm_name);
    }

    let services: Vec<MaglevRecord> = state.store.list_entities(STORE_MAGLEV).unwrap_or_default();
    for rec in services {
        if endpoint_names.contains(&rec.endpoint) {
            continue;
        }
        if let Ok(client) = fluxvm_client(state) {
            let _ = client.delete_network_service(&rec.service).await;
        }
        let _ = state.store.delete_entity(STORE_MAGLEV, &rec.endpoint);
        ipam::release(state, &rec.endpoint);
    }
}

fn inference_systemd_unit(exec: &str) -> String {
    format!(
        r#"[Unit]
Description=Fabric inference runtime
After=network-online.target
Wants=network-online.target

[Service]
Type=simple
ExecStart={exec}
Restart=on-failure
RestartSec=5

[Install]
WantedBy=multi-user.target
"#
    )
}

async fn release_vm_gpus(
    state: &AppState,
    client: &FluxVmClient,
    vm_name: &str,
    primary: &str,
) -> Result<(), String> {
    let mut bdfs = Vec::new();
    if !primary.is_empty() {
        bdfs.push(primary.to_string());
    }
    let rows: Vec<GpuReservation> = state
        .store
        .list_entities(STORE_GPU_RESERVATIONS)
        .unwrap_or_default();
    for row in rows {
        if row.vm_name == vm_name && !bdfs.iter().any(|bdf| bdf.eq_ignore_ascii_case(&row.bdf)) {
            bdfs.push(row.bdf);
        }
    }
    for bdf in bdfs {
        client
            .release_host_gpu(&GpuReleaseRequest {
                bdf: bdf.clone(),
                restore_driver: false,
            })
            .await
            .map_err(|e| format!("GPU release {bdf}: {e}"))?;
        let _ = state.store.delete_entity(STORE_GPU_RESERVATIONS, &bdf);
    }
    Ok(())
}

async fn release_missing_vm(
    state: &AppState,
    client: &FluxVmClient,
    rep: &InferenceReplica,
) -> Result<(), String> {
    if !rep.bdf.is_empty() && !rep.bdf.starts_with("dry-run-") {
        release_vm_gpus(state, client, &rep.vm_name, &rep.bdf).await?;
    }
    let _ = state.store.delete_entity(STORE_MANAGED_VMS, &rep.vm_name);
    Ok(())
}

async fn apply_health(state: &AppState, dep: &mut InferenceDeployment) {
    let mut replace = Vec::new();
    for rep in &mut dep.status.replicas {
        let Some(addr) = rep.address.clone() else {
            continue;
        };
        let healthy = probe_health(&addr, 8000).await;
        let (streak, replace_now) = note_health(rep.unhealthy_streak, healthy);
        rep.unhealthy_streak = streak;
        rep.ready = healthy;
        if replace_now {
            replace.push(rep.vm_name.clone());
        }
    }
    for name in replace {
        let Some(idx) = dep.status.replicas.iter().position(|r| r.vm_name == name) else {
            continue;
        };
        let rep = dep.status.replicas.remove(idx);
        if let Err(e) = remove_replica(state, dep, &rep).await {
            dep.status.message = Some(e);
            let at = idx.min(dep.status.replicas.len());
            dep.status.replicas.insert(at, rep);
        }
    }
}

async fn retry_deleting_endpoints(state: &AppState) {
    let endpoints: Vec<InferenceEndpoint> = state
        .store
        .list_entities(STORE_ENDPOINTS)
        .unwrap_or_default();
    for ep in endpoints.into_iter().filter(|e| e.phase == "Deleting") {
        match teardown_endpoint(state, &ep).await {
            Ok(_) => {
                let _ = state.store.delete_entity(STORE_ENDPOINTS, &ep.name);
            }
            Err(e) => tracing::warn!(endpoint = %ep.name, "endpoint delete retry: {e}"),
        }
    }
}

async fn probe_health(addr: &str, port: u16) -> bool {
    let url = format!("http://{addr}:{port}{HEALTH_PATH}");
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(2))
        .build();
    let Ok(client) = client else {
        return false;
    };
    matches!(client.get(&url).send().await, Ok(r) if r.status().is_success())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn health_replaces_after_three_failures() {
        assert_eq!(note_health(0, true), (0, false));
        assert_eq!(note_health(0, false), (1, false));
        assert_eq!(note_health(2, false), (3, true));
    }

    #[test]
    fn failed_maglev_delete_keeps_vip() {
        assert!(!maglev_is_absent(
            Some("connection refused"),
            false,
            Some("connection refused")
        ));
        assert!(!maglev_is_absent(None, true, None));
        assert!(should_release_vip(false).is_err());
    }

    #[test]
    fn already_absent_maglev_releases_vip() {
        let plan = plan_endpoint_teardown(Some("404 not found"), false, Some("not found")).unwrap();
        assert!(plan.release_vip);
        assert!(plan.delete_endpoint);
        assert!(maglev_is_absent(None, false, Some("404 not found")));
        assert!(plan_endpoint_teardown(
            Some("connection refused"),
            false,
            Some("connection refused")
        )
        .is_err());
        assert!(plan_endpoint_teardown(None, true, None).is_err());
    }

    fn sample_replica(ordinal: u32) -> InferenceReplica {
        InferenceReplica {
            replica_id: replica_id_for("dep", ordinal),
            ordinal,
            vm_name: replica_vm_name("dep", ordinal),
            bdf: format!("dry-run-{ordinal}"),
            ready: false,
            address: None,
            metrics: None,
            maglev_weight: None,
            site: None,
            cost_tier: None,
            draining: false,
            unhealthy_streak: 3,
            deployment: "dep".into(),
            revision: 1,
            model_digest: String::new(),
            profile_digest: String::new(),
            host: String::new(),
            generation: 1,
            lifecycle: "ready".into(),
            health: "unhealthy".into(),
            created_at: None,
        }
    }

    #[test]
    fn replacement_reclaims_freed_ordinal() {
        let live = vec![sample_replica(1)];
        let ordinal = next_replica_ordinal(&live);
        assert_eq!(ordinal, 0);
        assert_eq!(replica_vm_name("dep", ordinal), "dep-0");
        assert_ne!(replica_vm_name("dep", ordinal), live[0].vm_name);
    }

    #[test]
    fn restart_preserves_replica_identity() {
        let rep = sample_replica(1);
        let json = serde_json::to_string(&rep).unwrap();
        let back: InferenceReplica = serde_json::from_str(&json).unwrap();
        assert_eq!(back.replica_id, "dep-r000001");
        assert_eq!(back.ordinal, 1);
        assert_eq!(back.vm_name, "dep-1");
        let old = r#"{"vm_name":"dep-1","bdf":"x","ready":true}"#;
        let mut loaded: InferenceReplica = serde_json::from_str(old).unwrap();
        ensure_replica_identity("dep", &mut loaded);
        assert_eq!(loaded.ordinal, 1);
        assert_eq!(loaded.replica_id, "dep-r000001");
        assert_eq!(loaded.vm_name, "dep-1");
    }

    #[test]
    fn multiple_replacements_get_distinct_names() {
        let mut live = Vec::new();
        let first = next_replica_ordinal(&live);
        live.push(sample_replica(first));
        let second = next_replica_ordinal(&live);
        live.push(sample_replica(second));
        assert_eq!(first, 0);
        assert_eq!(second, 1);
        assert_ne!(live[0].vm_name, live[1].vm_name);
        assert_ne!(live[0].replica_id, live[1].replica_id);
    }

    #[test]
    fn unreadable_reservation_stays_until_hardware_is_idle() {
        assert_eq!(
            corrupt_reservation_action(LiveCheck::Unknown),
            CorruptAction::Keep
        );
        assert_eq!(
            corrupt_reservation_action(LiveCheck::Active),
            CorruptAction::Repair
        );
        assert_eq!(
            corrupt_reservation_action(LiveCheck::Idle),
            CorruptAction::Reclaim
        );
    }

    #[test]
    fn transient_fluxvm_errors_stay_pending() {
        assert!(is_retryable_fluxvm("connection refused"));
        assert!(is_retryable_fluxvm("request timed out"));
        assert!(is_retryable_fluxvm("deployment lease held"));
        assert!(!is_retryable_fluxvm("profile 'edge' not found"));
        assert!(is_hard_failure("model 'qwen' not found"));
    }
}
