// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: LicenseRef-Zyvor-Production-1.0

//! Declarative `ContainerGroup` workload — OCI containers scheduled by
//! fabric's own placement (`container_placement`) onto a Kubernetes cluster,
//! executed via FluxVM's Secure Containers `RuntimeClass fluxvm`. Sibling to
//! `declarative::VMSpec`, reusing its `ResourceSpec`/`VolumeMount` shapes
//! where they already match a container's needs.
//!
//! v1 scope, deliberately: one target cluster (no multi-site resolution —
//! matches the VM path's own single-`fluxvm_url` maturity today), fixed
//! replica count (no horizontal autoscaling loop), no `AffinityRule`
//! cross-referencing (see `container_placement`), ephemeral/hostPath volumes
//! only (no CSI/virtiofs-backed volume that could move with a rescheduled
//! Pod).

use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::sync::Arc;

use super::container_placement;
use super::declarative::{parse_memory_mb, ResourceSpec, VolumeMount};
use crate::server::AppState;
use security::{RequireRead, RequireWrite};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnvVarSpec {
    pub name: String,
    pub value: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContainerSpec {
    pub name: String,
    /// OCI image reference — the one real divergence from `VMSpec.image`
    /// (a qcow2/raw base image name).
    pub image: String,
    #[serde(default)]
    pub command: Vec<String>,
    #[serde(default)]
    pub args: Vec<String>,
    #[serde(default)]
    pub env: Vec<EnvVarSpec>,
    pub resources: ResourceSpec,
    #[serde(default)]
    pub volume_mounts: Vec<VolumeMount>,
    /// Restarted by Kubernetes when this fails past `failure_threshold` —
    /// without one, a hung/deadlocked container just keeps running forever.
    #[serde(default)]
    pub liveness_probe: Option<ProbeSpec>,
    /// Taken out of Service/DNS rotation (but not restarted) while this
    /// fails — lets a container signal "up but not ready yet" separately
    /// from "should be killed".
    #[serde(default)]
    pub readiness_probe: Option<ProbeSpec>,
}

/// Mirrors `k8s_openapi::api::core::v1::Probe`'s tunables — the same knobs
/// a Kubernetes manifest would expose, no fabric-specific defaults layered
/// on top beyond what the Kubernetes API itself already defaults
/// server-side when a field is left unset.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProbeSpec {
    #[serde(flatten)]
    pub check: ProbeCheckSpec,
    #[serde(default)]
    pub initial_delay_secs: Option<u32>,
    #[serde(default)]
    pub period_secs: Option<u32>,
    #[serde(default)]
    pub timeout_secs: Option<u32>,
    #[serde(default)]
    pub success_threshold: Option<u32>,
    #[serde(default)]
    pub failure_threshold: Option<u32>,
}

/// What a probe actually checks. Covers the three most common Kubernetes
/// probe mechanisms; gRPC probes are a natural follow-up if a customer
/// asks for one, left out for a first version to keep the API surface
/// small.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ProbeCheckSpec {
    Http { path: String, port: u16 },
    Tcp { port: u16 },
    Exec { command: Vec<String> },
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PlacementSpec {
    /// Pin directly to a host (matched by id or hostname), bypassing
    /// `predictive_drs` scoring.
    #[serde(default)]
    pub node_hint: Option<String>,
    /// If true, an autohealer may delete and recreate this group's Pods on
    /// a new host after its current host goes `NotResponding`. Default
    /// false — Secure Containers is still developer-preview upstream, and
    /// there's no volume-affinity-aware placement yet, so auto-reschedule
    /// is opt-in rather than a default behavior.
    #[serde(default)]
    pub auto_reschedule: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContainerGroupSpec {
    pub name: String,
    pub containers: Vec<ContainerSpec>,
    #[serde(default = "default_replicas")]
    pub replicas: u32,
    #[serde(default)]
    pub placement: PlacementSpec,
    #[serde(default = "default_restart_policy")]
    pub restart_policy: String,
    #[serde(default)]
    pub tags: Vec<String>,
    /// Owning tenant. When the caller's JWT carries a `tenant` claim, this
    /// must match it (or be unset) at create time — see
    /// `tenant_scope::apply_create_tenant` — and `tenant_scope`'s
    /// middleware then scopes all by-name reads/writes to it, the same way
    /// VM routes are scoped via `tenant_scope::vm_tenant`.
    #[serde(default)]
    pub tenant: Option<String>,
    /// Names of `kubernetes.io/dockerconfigjson` Secrets, already present in
    /// the target namespace, to pull this group's images with. Pod-level in
    /// the Kubernetes API (applies to every container in the group), so it
    /// lives here rather than on `ContainerSpec`. Fabric does not create or
    /// manage these Secrets itself — provisioning them is the operator's job
    /// (e.g. via a separate `kubectl create secret docker-registry`).
    #[serde(default)]
    pub image_pull_secrets: Vec<String>,
    /// Kubernetes `NetworkPolicy` isolating this group's Pods. `None`
    /// (default) creates no policy at all — unrestricted, matching today's
    /// behavior. A standard K8s `NetworkPolicy` rather than fabric's own
    /// Cilium-identity-based `VmNetworkPolicy` (which VMs use): that engine
    /// keys policy on a FluxVM VM id, and a ContainerGroup's underlying
    /// per-Pod microVM id is only knowable by polling FluxVM and matching
    /// `pod_uid` after the fact — real, but meaningfully more machinery
    /// than a v1 needs. `NetworkPolicy` is portable to any CNI that
    /// enforces it and needs no such correlation.
    #[serde(default)]
    pub network_policy: Option<NetworkPolicySpec>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct NetworkPolicySpec {
    /// `None` leaves ingress unrestricted by this policy. `Some(vec![])`
    /// denies all ingress. `Some(rules)` allows only what the rules
    /// describe.
    #[serde(default)]
    pub ingress: Option<Vec<NetworkPolicyRuleSpec>>,
    /// Same semantics as `ingress`, for outbound traffic.
    #[serde(default)]
    pub egress: Option<Vec<NetworkPolicyRuleSpec>>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct NetworkPolicyRuleSpec {
    /// Allow from/to another ContainerGroup's Pods in the same namespace,
    /// by name.
    #[serde(default)]
    pub from_container_groups: Vec<String>,
    #[serde(default)]
    pub from_cidrs: Vec<String>,
    /// Empty means all ports for whatever this rule matches.
    #[serde(default)]
    pub ports: Vec<u16>,
}

fn default_replicas() -> u32 {
    1
}

fn default_restart_policy() -> String {
    "Always".to_string()
}

impl ContainerGroupSpec {
    /// Total cpus/memory-MB across every container, times replica count.
    /// Callers must validate each container's `resources.memory` string
    /// before relying on this — a parse failure here degrades to 0 rather
    /// than panicking, since by placement time `apply_container_group_spec`
    /// has already rejected an invalid spec.
    pub fn total_resources(&self) -> (u32, u64) {
        let mut cpus: u32 = 0;
        let mut memory_mb: u64 = 0;
        for c in &self.containers {
            cpus = cpus.saturating_add(c.resources.cpus);
            memory_mb = memory_mb.saturating_add(parse_memory_mb(&c.resources.memory).unwrap_or(0));
        }
        let replicas = self.replicas.max(1);
        (
            cpus.saturating_mul(replicas),
            memory_mb.saturating_mul(replicas as u64),
        )
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct ContainerGroupStatus {
    pub(crate) host_id: String,
    pub(crate) host_name: String,
    pub(crate) pod_names: Vec<String>,
    pub(crate) updated_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Serialize)]
pub struct ContainerGroupApplyResult {
    pub name: String,
    pub host_id: String,
    pub host_name: String,
    pub replicas_created: usize,
    pub warnings: Vec<String>,
}

/// Audit trail for ContainerGroup lifecycle actions — mirrors `events::VMEvent`/
/// `record_event`, persisted to its own `container_group_events` collection.
/// Deliberately not wired into the SSE stream or notification rules (that's
/// real-time alerting; this is queryable audit history) — a natural follow-up
/// once ContainerGroup needs the same "notify on event" behavior VMs have.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContainerGroupEvent {
    pub id: String,
    pub event_type: ContainerGroupEventType,
    pub container_group_name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tenant: Option<String>,
    /// `Claims.sub` of whoever triggered this action.
    pub actor: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
    pub timestamp: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ContainerGroupEventType {
    Created,
    Applied,
    Deleted,
    PlacementFailed,
    QuotaExceeded,
}

fn record_container_group_event(
    state: &Arc<AppState>,
    event_type: ContainerGroupEventType,
    container_group_name: &str,
    tenant: Option<String>,
    actor: String,
    detail: Option<String>,
) {
    let event = ContainerGroupEvent {
        id: uuid::Uuid::new_v4().to_string(),
        event_type,
        container_group_name: container_group_name.to_string(),
        tenant,
        actor,
        detail,
        timestamp: chrono::Utc::now(),
    };
    if let Err(e) = state
        .store
        .save_entity("container_group_events", &event.id, &event)
    {
        tracing::error!("Failed to record ContainerGroup event: {}", e);
    }
}

fn err(status: StatusCode, msg: impl Into<String>) -> (StatusCode, Json<serde_json::Value>) {
    (status, Json(json!({"error": msg.into()})))
}

/// Whether `name` is a valid Kubernetes object name (RFC 1123 DNS
/// subdomain) — used for `image_pull_secrets` entries, which reference an
/// existing `Secret` by name rather than one fabric itself validated at
/// creation time the way `validate_vm_name` covers VM/ContainerGroup names.
fn is_valid_k8s_object_name(name: &str) -> bool {
    if name.is_empty() || name.len() > 253 {
        return false;
    }
    let valid_chars = name
        .chars()
        .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-' || c == '.');
    let valid_ends = !name.starts_with('-')
        && !name.ends_with('-')
        && !name.starts_with('.')
        && !name.ends_with('.');
    valid_chars && valid_ends
}

/// Whether `tenant` is safe to use as a Kubernetes *namespace* name
/// component. Namespace names are DNS-1123 *labels* (max 63 characters,
/// lowercase alphanumeric or `-`, no dots) — stricter than a generic
/// object name, and unlike `image_pull_secrets` entries a tenant is
/// combined with a prefix (`{base}-{tenant}`), so it's capped well short
/// of 63 to leave the base namespace name room.
pub(crate) fn is_valid_tenant_name(tenant: &str) -> bool {
    if tenant.is_empty() || tenant.len() > 40 {
        return false;
    }
    let valid_chars = tenant
        .chars()
        .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-');
    valid_chars && !tenant.starts_with('-') && !tenant.ends_with('-')
}

/// The namespace a ContainerGroup's Pod(s) belong in: the shared default
/// unless the group has a tenant *and* `namespace_per_tenant` is on, in
/// which case each tenant gets its own `{namespace}-{tenant}` namespace
/// (auto-created via `K8sPodClient::ensure_namespace` before first use) —
/// real isolation instead of every tenant's Pods sharing one namespace.
/// Returns `None` (use the client's own default) rather than a namespace
/// name so callers don't need to duplicate the base-namespace lookup.
pub(crate) fn container_group_namespace(state: &AppState, tenant: Option<&str>) -> Option<String> {
    if !state.config.container_groups.namespace_per_tenant {
        return None;
    }
    tenant.map(|t| format!("{}-{t}", state.config.container_groups.namespace))
}

fn validate_spec(spec: &ContainerGroupSpec) -> Result<(), (StatusCode, Json<serde_json::Value>)> {
    crate::validation::validate_vm_name(&spec.name).map_err(|(s, m)| err(s, m))?;

    if spec.containers.is_empty() {
        return Err(err(StatusCode::BAD_REQUEST, "containers must not be empty"));
    }
    if spec.containers.len() > 16 {
        return Err(err(
            StatusCode::BAD_REQUEST,
            "containers count must not exceed 16",
        ));
    }
    if spec.replicas < 1 || spec.replicas > 64 {
        return Err(err(
            StatusCode::BAD_REQUEST,
            "replicas must be between 1 and 64",
        ));
    }
    if spec.tags.len() > 100 {
        return Err(err(
            StatusCode::BAD_REQUEST,
            "tags count must not exceed 100",
        ));
    }
    if !matches!(
        spec.restart_policy.as_str(),
        "Always" | "Never" | "OnFailure"
    ) {
        return Err(err(
            StatusCode::BAD_REQUEST,
            "restart_policy must be one of Always, Never, OnFailure",
        ));
    }
    if spec.image_pull_secrets.len() > 16 {
        return Err(err(
            StatusCode::BAD_REQUEST,
            "image_pull_secrets count must not exceed 16",
        ));
    }
    for secret_name in &spec.image_pull_secrets {
        if !is_valid_k8s_object_name(secret_name) {
            return Err(err(
                StatusCode::BAD_REQUEST,
                format!(
                    "invalid image_pull_secrets entry '{secret_name}': must be a valid \
                     Kubernetes object name (lowercase alphanumeric, '-' or '.', \
                     1-253 characters, not starting or ending with '-' or '.')"
                ),
            ));
        }
    }

    for c in &spec.containers {
        if c.name.is_empty() || c.image.is_empty() {
            return Err(err(
                StatusCode::BAD_REQUEST,
                "each container needs a non-empty name and image",
            ));
        }
        if c.resources.cpus < 1 || c.resources.cpus > 1024 {
            return Err(err(
                StatusCode::BAD_REQUEST,
                format!("container '{}': cpus must be between 1 and 1024", c.name),
            ));
        }
        parse_memory_mb(&c.resources.memory).map_err(|e| {
            err(
                StatusCode::BAD_REQUEST,
                format!("container '{}': {}", c.name, e),
            )
        })?;
        for vol in &c.volume_mounts {
            crate::validation::validate_host_path(&vol.host).map_err(|(s, m)| {
                err(
                    s,
                    format!("container '{}': invalid volume host path: {}", c.name, m),
                )
            })?;
            crate::validation::validate_machine_path(&vol.guest).map_err(|(s, m)| {
                err(
                    s,
                    format!("container '{}': invalid volume guest path: {}", c.name, m),
                )
            })?;
        }
        for (probe_name, probe) in [
            ("liveness_probe", &c.liveness_probe),
            ("readiness_probe", &c.readiness_probe),
        ] {
            if let Some(probe) = probe {
                validate_probe(probe).map_err(|m| {
                    err(
                        StatusCode::BAD_REQUEST,
                        format!("container '{}': {probe_name}: {m}", c.name),
                    )
                })?;
            }
        }
    }

    if let Some(policy) = &spec.network_policy {
        for (direction, rules) in [("ingress", &policy.ingress), ("egress", &policy.egress)] {
            let Some(rules) = rules else { continue };
            for rule in rules {
                for group in &rule.from_container_groups {
                    crate::validation::validate_vm_name(group).map_err(|(s, m)| {
                        err(s, format!("network_policy.{direction}: invalid from_container_groups entry '{group}': {m}"))
                    })?;
                }
                for cidr in &rule.from_cidrs {
                    crate::validation::validate_cidr(cidr).map_err(|m| {
                        err(
                            StatusCode::BAD_REQUEST,
                            format!("network_policy.{direction}: invalid from_cidrs entry '{cidr}': {m}"),
                        )
                    })?;
                }
                if rule.ports.contains(&0) {
                    return Err(err(
                        StatusCode::BAD_REQUEST,
                        format!("network_policy.{direction}: port must be between 1 and 65535"),
                    ));
                }
            }
        }
    }

    Ok(())
}

fn validate_probe(probe: &ProbeSpec) -> Result<(), String> {
    match &probe.check {
        ProbeCheckSpec::Http { path, port } => {
            if !path.starts_with('/') {
                return Err("http path must start with '/'".to_string());
            }
            if *port == 0 {
                return Err("port must be between 1 and 65535".to_string());
            }
        }
        ProbeCheckSpec::Tcp { port } => {
            if *port == 0 {
                return Err("port must be between 1 and 65535".to_string());
            }
        }
        ProbeCheckSpec::Exec { command } => {
            if command.is_empty() {
                return Err("exec command must not be empty".to_string());
            }
        }
    }
    Ok(())
}

fn build_probe_request(probe: &ProbeSpec) -> k8s_pod_client::ProbeSpec {
    let check = match &probe.check {
        ProbeCheckSpec::Http { path, port } => k8s_pod_client::ProbeCheck::Http {
            path: path.clone(),
            port: *port,
        },
        ProbeCheckSpec::Tcp { port } => k8s_pod_client::ProbeCheck::Tcp { port: *port },
        ProbeCheckSpec::Exec { command } => k8s_pod_client::ProbeCheck::Exec {
            command: command.clone(),
        },
    };
    k8s_pod_client::ProbeSpec {
        check,
        initial_delay_secs: probe.initial_delay_secs,
        period_secs: probe.period_secs,
        timeout_secs: probe.timeout_secs,
        success_threshold: probe.success_threshold,
        failure_threshold: probe.failure_threshold,
    }
}

fn build_network_policy_rule(rule: &NetworkPolicyRuleSpec) -> k8s_pod_client::NetworkPolicyRule {
    let peer_label_selectors = rule
        .from_container_groups
        .iter()
        .map(|group| {
            let mut labels = std::collections::BTreeMap::new();
            labels.insert(
                "fabric.zyvor.dev/container-group".to_string(),
                group.clone(),
            );
            labels
        })
        .collect();
    k8s_pod_client::NetworkPolicyRule {
        peer_label_selectors,
        cidrs: rule.from_cidrs.clone(),
        ports: rule.ports.clone(),
    }
}

pub(crate) fn build_network_policy_request(
    spec: &ContainerGroupSpec,
) -> Option<k8s_pod_client::NetworkPolicyRequest> {
    let policy = spec.network_policy.as_ref()?;
    let mut pod_selector_labels = std::collections::BTreeMap::new();
    pod_selector_labels.insert(
        "fabric.zyvor.dev/container-group".to_string(),
        spec.name.clone(),
    );
    Some(k8s_pod_client::NetworkPolicyRequest {
        name: spec.name.clone(),
        pod_selector_labels,
        ingress: policy
            .ingress
            .as_ref()
            .map(|rules| rules.iter().map(build_network_policy_rule).collect()),
        egress: policy
            .egress
            .as_ref()
            .map(|rules| rules.iter().map(build_network_policy_rule).collect()),
    })
}

pub(crate) fn build_pod_request(
    spec: &ContainerGroupSpec,
    name: &str,
    node_name: &str,
) -> k8s_pod_client::PodRequest {
    let mut containers = Vec::new();
    let mut volumes = Vec::new();

    for c in &spec.containers {
        // Already validated in `validate_spec`.
        let memory_mb = parse_memory_mb(&c.resources.memory).unwrap_or(0);

        let mut volume_mounts = Vec::new();
        for (idx, vm) in c.volume_mounts.iter().enumerate() {
            let vol_name = format!("{}-vol-{}", c.name, idx);
            volumes.push(k8s_pod_client::PodVolumeSource {
                name: vol_name.clone(),
                host_path: vm.host.clone(),
            });
            volume_mounts.push(k8s_pod_client::PodVolumeMount {
                name: vol_name,
                mount_path: vm.guest.clone(),
                read_only: vm.readonly,
            });
        }

        containers.push(k8s_pod_client::PodContainerSpec {
            name: c.name.clone(),
            image: c.image.clone(),
            command: c.command.clone(),
            args: c.args.clone(),
            env: c
                .env
                .iter()
                .map(|e| (e.name.clone(), e.value.clone()))
                .collect(),
            cpu_millis: c.resources.cpus.saturating_mul(1000),
            memory_mb,
            volume_mounts,
            liveness_probe: c.liveness_probe.as_ref().map(build_probe_request),
            readiness_probe: c.readiness_probe.as_ref().map(build_probe_request),
        });
    }

    let mut labels = std::collections::BTreeMap::new();
    labels.insert(
        "fabric.zyvor.dev/container-group".to_string(),
        spec.name.clone(),
    );

    k8s_pod_client::PodRequest {
        name: name.to_string(),
        node_name: node_name.to_string(),
        labels,
        containers,
        volumes,
        restart_policy: spec.restart_policy.clone(),
        image_pull_secrets: spec.image_pull_secrets.clone(),
    }
}

pub(crate) fn pod_name(group_name: &str, replicas: u32, index: u32) -> String {
    if replicas <= 1 {
        group_name.to_string()
    } else {
        format!("{group_name}-{index}")
    }
}

/// Pod names a previous apply created that the current desired set no
/// longer wants — e.g. `replicas` went down, or dropped to 1 (renaming
/// `group-0` to the bare `group`). Deliberately compares against the full
/// *desired* set, not which creates actually succeeded this round: a
/// transient failure to (re-)create a still-desired pod must not make it
/// look stale and get deleted too.
fn stale_pod_names(previous: &[String], desired: &[String]) -> Vec<String> {
    previous
        .iter()
        .filter(|name| !desired.contains(name))
        .cloned()
        .collect()
}

/// POST /api/container-groups/apply — create/update a ContainerGroup:
/// place it on a Secure-Containers-capable host, then create its Pod(s)
/// there via the Kubernetes API.
pub async fn apply_container_group_spec(
    RequireWrite(claims): RequireWrite,
    State(state): State<Arc<AppState>>,
    Json(mut spec): Json<ContainerGroupSpec>,
) -> Result<(StatusCode, Json<ContainerGroupApplyResult>), (StatusCode, Json<serde_json::Value>)> {
    tracing::debug!(
        "container_declarative::{}",
        stringify!(apply_container_group_spec)
    );
    validate_spec(&spec)?;

    spec.tenant = crate::tenant_scope::apply_create_tenant(&claims, spec.tenant.clone())
        .map_err(|(s, m)| err(s, m))?;
    if let Some(tenant) = spec.tenant.as_deref() {
        if !is_valid_tenant_name(tenant) {
            return Err(err(
                StatusCode::BAD_REQUEST,
                format!(
                    "invalid tenant '{tenant}': must be 1-40 characters of lowercase \
                     alphanumerics or '-', not starting or ending with '-' \
                     (it becomes part of a Kubernetes namespace name)"
                ),
            ));
        }
    }

    let is_new = matches!(
        state
            .store
            .get_entity::<ContainerGroupSpec>("container_groups", &spec.name),
        Ok(None)
    );

    // Reject before touching Kubernetes at all if this would exceed a
    // tenant/tag-scoped quota. `exclude_container_group` drops this group's
    // own previous spec (if any) from the usage count first, since
    // `total_resources()`/`spec.replicas` below are this apply's full new
    // footprint, not an incremental add over the old one.
    let (req_cpus, req_memory_mb) = spec.total_resources();
    if let Err(e) = super::quotas::check_quota_enforcement(
        &state,
        req_cpus,
        req_memory_mb,
        0,
        &spec.tags,
        spec.tenant.as_deref(),
        0,
        spec.replicas,
        Some(spec.name.as_str()),
        0,
        None,
    )
    .await
    {
        record_container_group_event(
            &state,
            ContainerGroupEventType::QuotaExceeded,
            &spec.name,
            spec.tenant.clone(),
            claims.sub.clone(),
            Some(e.clone()),
        );
        return Err(err(StatusCode::FORBIDDEN, e));
    }

    // Pods from a previous apply that a lower `replicas` (or any other spec
    // change) no longer wants — collected now, before they're overwritten,
    // so they can be deleted once the new desired set exists. Without this,
    // scaling a group down (or deleting it entirely between two applies that
    // race) leaked every dropped Pod forever: nothing ever revisited
    // `ContainerGroupStatus.pod_names` from the *previous* apply.
    let previous_pod_names: Vec<String> = state
        .store
        .get_entity::<ContainerGroupStatus>("container_group_status", &spec.name)
        .ok()
        .flatten()
        .map(|status| status.pod_names)
        .unwrap_or_default();

    let client = state.k8s_pod_client.clone().ok_or_else(|| {
        err(
            StatusCode::SERVICE_UNAVAILABLE,
            "ContainerGroup support is not enabled/configured on this fabric instance \
             (set container_groups.enabled = true and a reachable kubeconfig)",
        )
    })?;

    let namespace = container_group_namespace(&state, spec.tenant.as_deref());
    if let Some(ns) = &namespace {
        client
            .ensure_namespace(ns)
            .await
            .map_err(|e| err(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    }

    let placed = container_placement::place_container_group(&state, &spec).map_err(|e| {
        record_container_group_event(
            &state,
            ContainerGroupEventType::PlacementFailed,
            &spec.name,
            spec.tenant.clone(),
            claims.sub.clone(),
            Some(e.clone()),
        );
        err(StatusCode::BAD_REQUEST, e)
    })?;

    let desired_pod_names: Vec<String> = (0..spec.replicas)
        .map(|i| pod_name(&spec.name, spec.replicas, i))
        .collect();

    let mut warnings = Vec::new();
    let mut created_pod_names = Vec::new();

    for name in &desired_pod_names {
        let pod_req = build_pod_request(&spec, name, &placed.hostname);
        match client.create_pod(&pod_req, namespace.as_deref()).await {
            Ok(_) => created_pod_names.push(name.clone()),
            Err(e) => warnings.push(format!("failed to create pod '{name}': {e}")),
        }
    }

    // Delete any Pod this apply no longer wants (e.g. `replicas` went down).
    for stale in stale_pod_names(&previous_pod_names, &desired_pod_names) {
        if let Err(e) = client.delete_pod(&stale, namespace.as_deref()).await {
            warnings.push(format!("failed to delete stale pod '{stale}': {e}"));
        }
    }

    match build_network_policy_request(&spec) {
        Some(policy_req) => {
            if let Err(e) = client
                .apply_network_policy(&policy_req, namespace.as_deref())
                .await
            {
                warnings.push(format!("failed to apply network policy: {e}"));
            }
        }
        // No policy this apply -- remove any the group had before rather
        // than leaving a stale one enforcing rules the current spec no
        // longer declares.
        None => {
            if let Err(e) = client
                .delete_network_policy(&spec.name, namespace.as_deref())
                .await
            {
                warnings.push(format!("failed to remove stale network policy: {e}"));
            }
        }
    }

    state
        .store
        .save_entity("container_groups", &spec.name, &spec)
        .map_err(|e| err(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let status = ContainerGroupStatus {
        host_id: placed.id.clone(),
        host_name: placed.hostname.clone(),
        pod_names: created_pod_names.clone(),
        updated_at: chrono::Utc::now(),
    };
    state
        .store
        .save_entity("container_group_status", &spec.name, &status)
        .map_err(|e| err(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    record_container_group_event(
        &state,
        if is_new {
            ContainerGroupEventType::Created
        } else {
            ContainerGroupEventType::Applied
        },
        &spec.name,
        spec.tenant.clone(),
        claims.sub.clone(),
        None,
    );

    Ok((
        StatusCode::OK,
        Json(ContainerGroupApplyResult {
            name: spec.name,
            host_id: placed.id,
            host_name: placed.hostname,
            replicas_created: created_pod_names.len(),
            warnings,
        }),
    ))
}

/// GET /api/container-groups — list all ContainerGroup specs, scoped to the
/// caller's tenant when their JWT carries one (mirrors
/// `list_container_group_events`/`list_container_group_backups`).
pub async fn list_container_groups(
    RequireRead(claims): RequireRead,
    State(state): State<Arc<AppState>>,
) -> Result<Json<Vec<ContainerGroupSpec>>, (StatusCode, Json<serde_json::Value>)> {
    tracing::debug!(
        "container_declarative::{}",
        stringify!(list_container_groups)
    );
    let mut groups: Vec<ContainerGroupSpec> = state
        .store
        .list_entities("container_groups")
        .map_err(|e| err(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    if let Some(tenant) = claims.tenant.as_deref() {
        groups.retain(|g| g.tenant.as_deref() == Some(tenant));
    }
    groups.sort_by(|a, b| a.name.cmp(&b.name));

    Ok(Json(groups))
}

/// GET /api/container-groups/:name/spec — export a ContainerGroup's stored spec.
pub async fn export_container_group_spec(
    RequireRead(_claims): RequireRead,
    State(state): State<Arc<AppState>>,
    Path(name): Path<String>,
) -> Result<Json<ContainerGroupSpec>, (StatusCode, Json<serde_json::Value>)> {
    tracing::debug!(
        "container_declarative::{}",
        stringify!(export_container_group_spec)
    );
    match state
        .store
        .get_entity::<ContainerGroupSpec>("container_groups", &name)
    {
        Ok(Some(spec)) => Ok(Json(spec)),
        Ok(None) => Err(err(StatusCode::NOT_FOUND, "ContainerGroup not found")),
        Err(e) => Err(err(StatusCode::INTERNAL_SERVER_ERROR, e.to_string())),
    }
}

#[derive(Debug, Serialize)]
pub struct ContainerGroupLivePod {
    pub name: String,
    pub phase: Option<String>,
    pub node_name: Option<String>,
    pub pod_ip: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct ContainerGroupLiveStatus {
    pub name: String,
    pub host_id: Option<String>,
    pub host_name: Option<String>,
    pub updated_at: Option<chrono::DateTime<chrono::Utc>>,
    pub pods: Vec<ContainerGroupLivePod>,
}

/// GET /api/container-groups/:name/status — placement host + live Pod phase/node.
pub async fn get_container_group_status(
    RequireRead(_claims): RequireRead,
    State(state): State<Arc<AppState>>,
    Path(name): Path<String>,
) -> Result<Json<ContainerGroupLiveStatus>, (StatusCode, Json<serde_json::Value>)> {
    let spec = state
        .store
        .get_entity::<ContainerGroupSpec>("container_groups", &name)
        .map_err(|e| err(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
        .ok_or_else(|| err(StatusCode::NOT_FOUND, "ContainerGroup not found"))?;

    let stored = state
        .store
        .get_entity::<ContainerGroupStatus>("container_group_status", &name)
        .map_err(|e| err(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let namespace = container_group_namespace(&state, spec.tenant.as_deref());
    let mut pods = Vec::new();
    if let (Some(client), Some(status)) = (state.k8s_pod_client.clone(), stored.as_ref()) {
        for pod_name in &status.pod_names {
            match client.get_pod_status(pod_name, namespace.as_deref()).await {
                Ok(Some(view)) => pods.push(ContainerGroupLivePod {
                    name: pod_name.clone(),
                    phase: view.phase,
                    node_name: view.node_name,
                    pod_ip: view.pod_ip,
                }),
                Ok(None) => pods.push(ContainerGroupLivePod {
                    name: pod_name.clone(),
                    phase: Some("NotFound".into()),
                    node_name: None,
                    pod_ip: None,
                }),
                Err(e) => {
                    tracing::warn!(
                        "failed to fetch pod '{}' for ContainerGroup '{}': {}",
                        pod_name,
                        name,
                        e
                    );
                    pods.push(ContainerGroupLivePod {
                        name: pod_name.clone(),
                        phase: Some("Unknown".into()),
                        node_name: None,
                        pod_ip: None,
                    });
                }
            }
        }
    }

    Ok(Json(ContainerGroupLiveStatus {
        name,
        host_id: stored.as_ref().map(|s| s.host_id.clone()),
        host_name: stored.as_ref().map(|s| s.host_name.clone()),
        updated_at: stored.as_ref().map(|s| s.updated_at),
        pods,
    }))
}

/// DELETE /api/container-groups/:name — delete a ContainerGroup's Pod(s) and
/// stored spec/status.
pub async fn delete_container_group(
    RequireWrite(claims): RequireWrite,
    State(state): State<Arc<AppState>>,
    Path(name): Path<String>,
) -> Result<StatusCode, (StatusCode, Json<serde_json::Value>)> {
    tracing::debug!(
        "container_declarative::{}",
        stringify!(delete_container_group)
    );

    let tenant = state
        .store
        .get_entity::<ContainerGroupSpec>("container_groups", &name)
        .ok()
        .flatten()
        .and_then(|spec| spec.tenant);

    let namespace = container_group_namespace(&state, tenant.as_deref());

    let status = state
        .store
        .get_entity::<ContainerGroupStatus>("container_group_status", &name)
        .map_err(|e| err(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    if let Some(client) = state.k8s_pod_client.clone() {
        if let Some(status) = status {
            for pod in &status.pod_names {
                if let Err(e) = client.delete_pod(pod, namespace.as_deref()).await {
                    tracing::warn!(
                        "failed to delete pod '{}' for ContainerGroup '{}': {}",
                        pod,
                        name,
                        e
                    );
                }
            }
        }
        if let Err(e) = client
            .delete_network_policy(&name, namespace.as_deref())
            .await
        {
            tracing::warn!(
                "failed to delete network policy for ContainerGroup '{}': {}",
                name,
                e
            );
        }
    }

    let _ = state.store.delete_entity("container_group_status", &name);
    state
        .store
        .delete_entity("container_groups", &name)
        .map_err(|e| err(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    record_container_group_event(
        &state,
        ContainerGroupEventType::Deleted,
        &name,
        tenant,
        claims.sub,
        None,
    );

    Ok(StatusCode::NO_CONTENT)
}

/// GET /api/container-group-events — list recent ContainerGroup audit events,
/// scoped to the caller's tenant when their JWT carries one (closing the gap
/// `events::list_events` itself has today).
pub async fn list_container_group_events(
    RequireRead(claims): RequireRead,
    State(state): State<Arc<AppState>>,
) -> Json<Vec<ContainerGroupEvent>> {
    tracing::debug!(
        "container_declarative::{}",
        stringify!(list_container_group_events)
    );
    let mut events: Vec<ContainerGroupEvent> = state
        .store
        .list_entities("container_group_events")
        .unwrap_or_default();

    if let Some(tenant) = claims.tenant.as_deref() {
        events.retain(|e| e.tenant.as_deref() == Some(tenant));
    }

    events.sort_by_key(|e| std::cmp::Reverse(e.timestamp));
    events.truncate(100);

    Json(events)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn container_group_event_omits_absent_tenant_and_detail_from_json() {
        let event = ContainerGroupEvent {
            id: "e1".to_string(),
            event_type: ContainerGroupEventType::Created,
            container_group_name: "web".to_string(),
            tenant: None,
            actor: "user-1".to_string(),
            detail: None,
            timestamp: chrono::Utc::now(),
        };
        let json = serde_json::to_value(&event).unwrap();
        let obj = json.as_object().unwrap();
        assert!(!obj.contains_key("tenant"));
        assert!(!obj.contains_key("detail"));
        assert_eq!(json["actor"], "user-1");
    }

    #[test]
    fn quota_exceeded_event_type_serializes_as_snake_case() {
        let event = ContainerGroupEvent {
            id: "e1".to_string(),
            event_type: ContainerGroupEventType::QuotaExceeded,
            container_group_name: "web".to_string(),
            tenant: Some("acme".to_string()),
            actor: "user-1".to_string(),
            detail: Some("CPU quota exceeded".to_string()),
            timestamp: chrono::Utc::now(),
        };
        let json = serde_json::to_value(&event).unwrap();
        assert_eq!(json["event_type"], "quota_exceeded");
    }

    #[test]
    fn container_group_spec_tenant_defaults_to_none_when_omitted() {
        let spec: ContainerGroupSpec = serde_json::from_str(
            r#"{"name": "web", "containers": [{"name": "app", "image": "nginx", "resources": {"cpus": 1, "memory": "512M"}}]}"#,
        )
        .unwrap();
        assert!(spec.tenant.is_none());
    }

    #[test]
    fn pod_name_is_the_bare_group_name_for_a_single_replica() {
        assert_eq!(pod_name("web", 1, 0), "web");
    }

    #[test]
    fn pod_name_is_suffixed_by_index_for_multiple_replicas() {
        assert_eq!(pod_name("web", 3, 2), "web-2");
    }

    #[test]
    fn stale_pod_names_finds_pods_dropped_by_a_lower_replica_count() {
        let previous = vec![
            "web-0".to_string(),
            "web-1".to_string(),
            "web-2".to_string(),
        ];
        let desired = vec!["web-0".to_string()];
        assert_eq!(
            stale_pod_names(&previous, &desired),
            vec!["web-1".to_string(), "web-2".to_string()]
        );
    }

    #[test]
    fn stale_pod_names_finds_the_renamed_pod_when_scaling_down_to_a_single_replica() {
        // pod_name() drops the "-0" suffix once replicas == 1, so the old
        // "web-0" is a different name than the new bare "web" and must be
        // cleaned up even though the group logically still has "replica 0".
        let previous = vec!["web-0".to_string()];
        let desired = vec!["web".to_string()];
        assert_eq!(
            stale_pod_names(&previous, &desired),
            vec!["web-0".to_string()]
        );
    }

    #[test]
    fn stale_pod_names_is_empty_when_nothing_was_dropped() {
        let previous = vec!["web-0".to_string(), "web-1".to_string()];
        let desired = vec!["web-0".to_string(), "web-1".to_string()];
        assert!(stale_pod_names(&previous, &desired).is_empty());
    }

    #[test]
    fn container_group_spec_image_pull_secrets_defaults_to_empty_when_omitted() {
        let spec: ContainerGroupSpec = serde_json::from_str(
            r#"{"name": "web", "containers": [{"name": "app", "image": "nginx", "resources": {"cpus": 1, "memory": "512M"}}]}"#,
        )
        .unwrap();
        assert!(spec.image_pull_secrets.is_empty());
    }

    #[test]
    fn is_valid_k8s_object_name_accepts_typical_secret_names() {
        assert!(is_valid_k8s_object_name("registry-creds"));
        assert!(is_valid_k8s_object_name("my.registry.creds"));
        assert!(is_valid_k8s_object_name("a"));
    }

    #[test]
    fn is_valid_k8s_object_name_rejects_uppercase_underscores_and_bad_edges() {
        assert!(!is_valid_k8s_object_name(""));
        assert!(!is_valid_k8s_object_name("Registry-Creds"));
        assert!(!is_valid_k8s_object_name("registry_creds"));
        assert!(!is_valid_k8s_object_name("-registry-creds"));
        assert!(!is_valid_k8s_object_name("registry-creds-"));
        assert!(!is_valid_k8s_object_name(".registry"));
    }

    #[test]
    fn validate_spec_rejects_an_invalid_image_pull_secret_name() {
        let spec: ContainerGroupSpec = serde_json::from_str(
            r#"{"name": "web", "image_pull_secrets": ["Bad_Name"], "containers": [{"name": "app", "image": "nginx", "resources": {"cpus": 1, "memory": "512M"}}]}"#,
        )
        .unwrap();
        let result = validate_spec(&spec);
        assert!(result.is_err());
    }

    #[test]
    fn is_valid_tenant_name_accepts_typical_tenant_names() {
        assert!(is_valid_tenant_name("acme"));
        assert!(is_valid_tenant_name("acme-corp-2"));
        assert!(is_valid_tenant_name("a"));
    }

    #[test]
    fn is_valid_tenant_name_rejects_uppercase_underscores_dots_and_bad_edges() {
        assert!(!is_valid_tenant_name(""));
        assert!(!is_valid_tenant_name("Acme"));
        assert!(!is_valid_tenant_name("acme_corp"));
        assert!(!is_valid_tenant_name("acme.corp"));
        assert!(!is_valid_tenant_name("-acme"));
        assert!(!is_valid_tenant_name("acme-"));
    }

    #[test]
    fn is_valid_tenant_name_rejects_names_over_40_characters() {
        let too_long = "a".repeat(41);
        assert!(!is_valid_tenant_name(&too_long));
        let just_right = "a".repeat(40);
        assert!(is_valid_tenant_name(&just_right));
    }

    #[test]
    fn probe_check_spec_deserializes_from_a_tagged_json_shape() {
        let probe: ProbeSpec = serde_json::from_str(
            r#"{"type": "http", "path": "/healthz", "port": 8080, "initial_delay_secs": 5}"#,
        )
        .unwrap();
        assert!(matches!(
            probe.check,
            ProbeCheckSpec::Http { ref path, port } if path == "/healthz" && port == 8080
        ));
        assert_eq!(probe.initial_delay_secs, Some(5));
    }

    #[test]
    fn container_spec_probes_default_to_none_when_omitted() {
        let spec: ContainerGroupSpec = serde_json::from_str(
            r#"{"name": "web", "containers": [{"name": "app", "image": "nginx", "resources": {"cpus": 1, "memory": "512M"}}]}"#,
        )
        .unwrap();
        assert!(spec.containers[0].liveness_probe.is_none());
        assert!(spec.containers[0].readiness_probe.is_none());
    }

    #[test]
    fn validate_probe_rejects_an_http_path_without_a_leading_slash() {
        let probe = ProbeSpec {
            check: ProbeCheckSpec::Http {
                path: "healthz".to_string(),
                port: 8080,
            },
            initial_delay_secs: None,
            period_secs: None,
            timeout_secs: None,
            success_threshold: None,
            failure_threshold: None,
        };
        assert!(validate_probe(&probe).is_err());
    }

    #[test]
    fn validate_probe_rejects_a_zero_tcp_port() {
        let probe = ProbeSpec {
            check: ProbeCheckSpec::Tcp { port: 0 },
            initial_delay_secs: None,
            period_secs: None,
            timeout_secs: None,
            success_threshold: None,
            failure_threshold: None,
        };
        assert!(validate_probe(&probe).is_err());
    }

    #[test]
    fn validate_probe_rejects_an_empty_exec_command() {
        let probe = ProbeSpec {
            check: ProbeCheckSpec::Exec { command: vec![] },
            initial_delay_secs: None,
            period_secs: None,
            timeout_secs: None,
            success_threshold: None,
            failure_threshold: None,
        };
        assert!(validate_probe(&probe).is_err());
    }

    #[test]
    fn validate_probe_accepts_a_well_formed_probe() {
        let probe = ProbeSpec {
            check: ProbeCheckSpec::Tcp { port: 5432 },
            initial_delay_secs: Some(5),
            period_secs: Some(10),
            timeout_secs: None,
            success_threshold: None,
            failure_threshold: Some(3),
        };
        assert!(validate_probe(&probe).is_ok());
    }

    fn sample_container_group_spec() -> ContainerGroupSpec {
        serde_json::from_str(
            r#"{"name": "web", "containers": [{"name": "app", "image": "nginx", "resources": {"cpus": 1, "memory": "512M"}}]}"#,
        )
        .unwrap()
    }

    #[test]
    fn network_policy_defaults_to_none_when_omitted() {
        let spec = sample_container_group_spec();
        assert!(spec.network_policy.is_none());
        assert!(build_network_policy_request(&spec).is_none());
    }

    #[test]
    fn build_network_policy_request_maps_a_container_group_peer() {
        let mut spec = sample_container_group_spec();
        spec.network_policy = Some(NetworkPolicySpec {
            ingress: Some(vec![NetworkPolicyRuleSpec {
                from_container_groups: vec!["frontend".to_string()],
                from_cidrs: vec![],
                ports: vec![8080],
            }]),
            egress: None,
        });
        let req = build_network_policy_request(&spec).expect("network policy request");
        assert_eq!(req.name, "web");
        let ingress = req.ingress.expect("ingress rules");
        assert_eq!(
            ingress[0].peer_label_selectors[0]["fabric.zyvor.dev/container-group"],
            "frontend"
        );
        assert_eq!(ingress[0].ports, vec![8080]);
        assert!(req.egress.is_none());
    }

    #[test]
    fn validate_spec_rejects_a_malformed_cidr_in_network_policy() {
        let mut spec = sample_container_group_spec();
        spec.network_policy = Some(NetworkPolicySpec {
            ingress: Some(vec![NetworkPolicyRuleSpec {
                from_container_groups: vec![],
                from_cidrs: vec!["not-a-cidr".to_string()],
                ports: vec![],
            }]),
            egress: None,
        });
        assert!(validate_spec(&spec).is_err());
    }

    #[test]
    fn validate_spec_rejects_a_zero_port_in_network_policy() {
        let mut spec = sample_container_group_spec();
        spec.network_policy = Some(NetworkPolicySpec {
            ingress: None,
            egress: Some(vec![NetworkPolicyRuleSpec {
                from_container_groups: vec![],
                from_cidrs: vec![],
                ports: vec![0],
            }]),
        });
        assert!(validate_spec(&spec).is_err());
    }

    #[test]
    fn validate_spec_accepts_a_deny_all_network_policy() {
        let mut spec = sample_container_group_spec();
        spec.network_policy = Some(NetworkPolicySpec {
            ingress: Some(vec![]),
            egress: None,
        });
        assert!(validate_spec(&spec).is_ok());
    }
}
