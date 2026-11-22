// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: LicenseRef-Zyvor-Production-2.0

use axum::{
    extract::{Query, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::sync::Arc;

use crate::server::AppState;
use security::{RequireRead, RequireWrite};

// ============================================================================
// Declarative VM Configuration (TOML/YAML-style via JSON API)
// ============================================================================

/// Declarative VM specification — full VM definition in a single request
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VMSpec {
    pub name: String,
    pub image: String,
    pub resources: ResourceSpec,
    #[serde(default)]
    pub network: Vec<NetworkSpec>,
    #[serde(default)]
    pub volumes: Vec<VolumeMount>,
    #[serde(default)]
    pub credentials: Vec<CredentialSpec>,
    #[serde(default)]
    pub start_options: StartOptionsSpec,
    #[serde(default)]
    pub autoscale: Option<AutoScaleSpec>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hostname: Option<String>,
    #[serde(default)]
    pub tags: Vec<String>,
    /// Host NIC for a bridge-less FluxVM tap. Mutually exclusive with bridged networking.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub direct_uplink: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub direct_mode: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub direct_guest_ips: Vec<String>,
    /// Auto-start VM after creation
    #[serde(default)]
    pub auto_start: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourceSpec {
    pub cpus: u32,
    /// Memory with unit suffix: "2G", "512M", "4096" (MB)
    pub memory: String,
    /// Disk with unit suffix: "20G", "50G"
    #[serde(default = "default_disk")]
    pub disk: String,
}

fn default_disk() -> String {
    "20G".to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetworkSpec {
    #[serde(rename = "type")]
    pub net_type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ip: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cid: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VolumeMount {
    pub host: String,
    pub guest: String,
    /// Mount type: "9p", "virtiofs"
    #[serde(rename = "type", default = "default_mount_type")]
    pub mount_type: String,
    #[serde(default)]
    pub readonly: bool,
}

fn default_mount_type() -> String {
    "virtiofs".to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CredentialSpec {
    pub id: String,
    /// Inline value or file path prefixed with @
    pub value: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct StartOptionsSpec {
    #[serde(default)]
    pub kvm: Option<bool>,
    #[serde(default)]
    pub secure_boot: Option<bool>,
    #[serde(default)]
    pub vsock: Option<bool>,
    #[serde(default)]
    pub tpm: Option<bool>,
    /// Console mode: "interactive", "read-only", "native", "gui"
    #[serde(default)]
    pub console: Option<vm_model::ConsoleMode>,
    /// Create a TAP device for networking
    #[serde(default)]
    pub network_tap: bool,
    /// Use user mode networking
    #[serde(default)]
    pub network_user_mode: bool,
    /// Manager scope: "system" or "user"
    #[serde(default)]
    pub scope: Option<vm_model::ManagerScope>,
    /// Use directory instead of image
    #[serde(skip_serializing_if = "Option::is_none")]
    pub directory: Option<String>,
    /// Bind mounts from host into VM
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub bind_mounts: Vec<vm_model::BindMount>,
    /// Credentials to pass (ID -> value)
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub set_credentials: Vec<vm_model::VMCredential>,
    /// Forward VM journal to host
    #[serde(skip_serializing_if = "Option::is_none")]
    pub forward_journal: Option<String>,
    /// Generate and pass SSH key to VM
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pass_ssh_key: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub direct_uplink: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub direct_mode: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub direct_guest_ips: Vec<String>,
}

impl From<StartOptionsSpec> for vm_model::VMStartOptions {
    fn from(spec: StartOptionsSpec) -> Self {
        vm_model::VMStartOptions {
            kvm: spec.kvm,
            secure_boot: spec.secure_boot,
            vsock: spec.vsock,
            tpm: spec.tpm,
            console: spec.console,
            network_tap: spec.network_tap,
            network_user_mode: spec.network_user_mode,
            scope: spec.scope,
            directory: spec.directory,
            bind_mounts: spec.bind_mounts,
            credentials: spec.set_credentials,
            forward_journal: spec.forward_journal,
            pass_ssh_key: spec.pass_ssh_key,
            direct_uplink: spec.direct_uplink,
            direct_mode: spec.direct_mode,
            direct_guest_ips: spec.direct_guest_ips,
            ..Default::default()
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AutoScaleSpec {
    #[serde(default)]
    pub cpu_threshold: Option<f64>,
    #[serde(default)]
    pub memory_threshold: Option<f64>,
    #[serde(default)]
    pub max_cpus: Option<u32>,
    #[serde(default)]
    pub max_memory_mb: Option<u64>,
    #[serde(default)]
    pub min_cpus: Option<u32>,
    #[serde(default)]
    pub min_memory_mb: Option<u64>,
    #[serde(default = "default_cooldown")]
    pub cooldown_secs: u64,
}

fn default_cooldown() -> u64 {
    300
}

#[derive(Debug, Serialize)]
pub struct ApplyResult {
    pub vm_name: String,
    pub created: bool,
    pub started: bool,
    pub volumes_configured: usize,
    pub autoscale_enabled: bool,
    pub warnings: Vec<String>,
}

/// Parse memory string like "2G", "512M", "4096" to MB.
/// Returns an error on invalid input instead of silently defaulting.
pub(crate) fn parse_memory_mb(s: &str) -> Result<u64, String> {
    let s = s.trim();
    if let Some(gb) = s
        .strip_suffix('G')
        .or_else(|| s.strip_suffix("GB"))
        .or_else(|| s.strip_suffix("g"))
    {
        let val = gb
            .trim()
            .parse::<u64>()
            .map_err(|_| format!("Invalid memory value: '{}'", s))?;
        Ok(val * 1024)
    } else if let Some(mb) = s
        .strip_suffix('M')
        .or_else(|| s.strip_suffix("MB"))
        .or_else(|| s.strip_suffix("m"))
    {
        mb.trim()
            .parse::<u64>()
            .map_err(|_| format!("Invalid memory value: '{}'", s))
    } else {
        s.parse::<u64>().map_err(|_| {
            format!(
                "Invalid memory value: '{}'. Use format like '2G', '512M', or raw MB",
                s
            )
        })
    }
}

/// Parse disk string like "20G", "100G" to GB.
/// Returns an error on invalid input instead of silently defaulting.
pub(crate) fn parse_disk_gb(s: &str) -> Result<u64, String> {
    let s = s.trim();
    if let Some(gb) = s
        .strip_suffix('G')
        .or_else(|| s.strip_suffix("GB"))
        .or_else(|| s.strip_suffix("g"))
    {
        gb.trim()
            .parse::<u64>()
            .map_err(|_| format!("Invalid disk size: '{}'", s))
    } else if let Some(tb) = s
        .strip_suffix('T')
        .or_else(|| s.strip_suffix("TB"))
        .or_else(|| s.strip_suffix("t"))
    {
        let val = tb
            .trim()
            .parse::<u64>()
            .map_err(|_| format!("Invalid disk size: '{}'", s))?;
        Ok(val * 1024)
    } else {
        s.parse::<u64>().map_err(|_| {
            format!(
                "Invalid disk size: '{}'. Use format like '20G', '1T', or raw GB",
                s
            )
        })
    }
}

// ============================================================================
// Handlers
// ============================================================================

/// Optional query parameters for `POST /api/vms/apply`. Both are additive:
/// with neither present the apply behaves exactly as it always has.
#[derive(Debug, Default, Deserialize)]
pub struct ApplyQuery {
    /// Return the plan and change nothing.
    #[serde(default)]
    pub dry_run: bool,
    /// Refuse (409) unless the live plan for this spec hashes to this value.
    #[serde(default)]
    pub expected_plan_hash: Option<String>,
}

type ApiError = (StatusCode, Json<serde_json::Value>);

/// Rebuild the declarative spec for a stored VM (shared by export and plan).
fn build_vm_spec(store: &state_store::StateStore, vm: vm_model::VM) -> VMSpec {
    let name = vm.name.clone();
    let volumes: Vec<VolumeMount> = store
        .get_entity("vm_volumes", &name)
        .ok()
        .flatten()
        .unwrap_or_default();
    let autoscale: Option<AutoScaleSpec> =
        store.get_entity("autoscale_policies", &name).ok().flatten();
    let memory_str = if vm.memory >= 1024 && vm.memory & 1023 == 0 {
        format!("{}G", vm.memory / 1024)
    } else {
        format!("{}M", vm.memory)
    };
    let start_options: StartOptionsSpec = store
        .get_entity("vm_start_options", &name)
        .ok()
        .flatten()
        .unwrap_or_default();
    VMSpec {
        name: vm.name,
        image: vm.image,
        resources: ResourceSpec {
            cpus: vm.cpus,
            memory: memory_str,
            disk: format!("{}G", vm.disk),
        },
        network: Vec::new(),
        volumes,
        credentials: Vec::new(),
        start_options,
        autoscale,
        hostname: vm.hostname,
        tags: vm.tags.unwrap_or_default(),
        direct_uplink: vm.direct_uplink,
        direct_mode: vm.direct_mode,
        direct_guest_ips: vm.direct_guest_ips,
        auto_start: false,
    }
}

fn internal_err(e: impl ToString) -> ApiError {
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        Json(json!({"error": e.to_string()})),
    )
}

/// Best-effort audit entry for a plan/dry run, recorded as a read.
async fn audit_plan_read(state: &AppState, user: &str, resource: &str, details: &str) {
    let _ = crate::api::audit::log_audit_event(
        state,
        user,
        "read",
        "vm_apply_plan",
        resource,
        crate::api::audit::AuditStatus::Success,
        None,
        Some(details),
        None,
    )
    .await;
}

/// POST /api/vms/apply - Apply a declarative VM spec (create + configure)
///
/// `?dry_run=true` returns the plan and changes nothing.
/// `?expected_plan_hash=<hash>` refuses with 409 if the live plan differs.
pub async fn apply_vm_spec(
    RequireWrite(claims): RequireWrite,
    State(state): State<Arc<AppState>>,
    Query(query): Query<ApplyQuery>,
    Json(spec): Json<VMSpec>,
) -> Result<Response, ApiError> {
    tracing::debug!("declarative::{}", stringify!(apply_vm_spec));
    crate::validation::validate_vm_name(&spec.name)
        .map_err(|(s, m)| (s, Json(json!({"error": m}))))?;

    if spec.credentials.len() > 100 {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(json!({"error": "credentials count must not exceed 100"})),
        ));
    }
    if spec.tags.len() > 100 {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(json!({"error": "tags count must not exceed 100"})),
        ));
    }
    if spec.resources.cpus < 1 || spec.resources.cpus > 1024 {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(json!({"error": "cpus must be between 1 and 1024"})),
        ));
    }

    // Validate volume host and guest paths
    for vol in &spec.volumes {
        crate::validation::validate_host_path(&vol.host).map_err(|(s, m)| {
            (
                s,
                Json(json!({"error": format!("Invalid volume host path: {}", m)})),
            )
        })?;
        crate::validation::validate_machine_path(&vol.guest).map_err(|(s, m)| {
            (
                s,
                Json(json!({"error": format!("Invalid volume guest path: {}", m)})),
            )
        })?;
    }

    // Validate credential file paths
    for cred in &spec.credentials {
        if let Some(file_path) = cred.value.strip_prefix('@') {
            crate::validation::validate_host_path(file_path).map_err(|(s, m)| {
                (
                    s,
                    Json(json!({"error": format!("Invalid credential file path: {}", m)})),
                )
            })?;
        }
    }

    let mut warnings = Vec::new();

    let memory_mb = parse_memory_mb(&spec.resources.memory)
        .map_err(|e| (StatusCode::BAD_REQUEST, Json(json!({"error": e}))))?;
    let disk_gb = parse_disk_gb(&spec.resources.disk)
        .map_err(|e| (StatusCode::BAD_REQUEST, Json(json!({"error": e}))))?;

    // Dry run / plan-hash guard (both optional; absent = unchanged behaviour).
    if query.dry_run || query.expected_plan_hash.is_some() {
        let current: Vec<VMSpec> = match state.store.get_vm(&spec.name) {
            Ok(Some(vm)) => vec![build_vm_spec(&state.store, vm)],
            Ok(None) => Vec::new(),
            Err(e) => return Err(internal_err(e)),
        };
        let live = super::declarative_plan::plan(
            &current,
            &super::declarative_plan::DesiredSpec {
                vms: vec![spec.clone()],
                prune: false,
            },
        );
        if query.dry_run {
            audit_plan_read(&state, &claims.sub, &spec.name, "dry run").await;
            return Ok((StatusCode::OK, Json(live)).into_response());
        }
        if let Some(expected) = &query.expected_plan_hash {
            if *expected != live.plan_hash {
                return Err((
                    StatusCode::CONFLICT,
                    Json(json!({
                        "error": "plan changed since it was reviewed; re-run the dry run",
                        "expected_plan_hash": expected,
                        "actual_plan_hash": live.plan_hash,
                        "plan": live,
                    })),
                ));
            }
        }
    }

    // Check if VM already exists
    let vm_exists = matches!(state.store.get_vm(&spec.name), Ok(Some(_)));

    if !vm_exists {
        // Reject before creating anything if this would exceed a tag-scoped
        // quota. `VMSpec` has no `tenant` field today (unlike
        // `ContainerGroupSpec`), so only tag-based quotas apply here.
        super::quotas::check_quota_enforcement(
            &state,
            spec.resources.cpus,
            memory_mb,
            disk_gb,
            &spec.tags,
            None,
            1,
            0,
            None,
            0,
            None,
        )
        .await
        .map_err(|e| (StatusCode::FORBIDDEN, Json(json!({"error": e}))))?;

        // Create the VM
        let req = vm_model::CreateVMRequest {
            name: spec.name.clone(),
            image: spec.image.clone(),
            cpus: spec.resources.cpus,
            memory: memory_mb,
            disk: disk_gb,
            hostname: spec.hostname.clone(),
            tags: if spec.tags.is_empty() {
                None
            } else {
                Some(spec.tags.clone())
            },
            labels: None,
            tenant: None,
            port_forwards: Vec::new(),
            network_tap: spec.start_options.network_tap,
            network_static_ip: false,
            direct_uplink: spec
                .direct_uplink
                .clone()
                .or_else(|| spec.start_options.direct_uplink.clone()),
            direct_mode: spec
                .direct_mode
                .clone()
                .or_else(|| spec.start_options.direct_mode.clone()),
            direct_guest_ips: if spec.direct_guest_ips.is_empty() {
                spec.start_options.direct_guest_ips.clone()
            } else {
                spec.direct_guest_ips.clone()
            },
            storage: None,
            enable_qga: false,
            hyperv: false,
        };
        if let Err(errors) = req.validate() {
            return Err((
                StatusCode::BAD_REQUEST,
                Json(json!({"error": errors.join("; ")})),
            ));
        }

        let vm = vm_model::VM::from_request(&req);

        state.store.save_vm(&vm).map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": e.to_string() })),
            )
        })?;
    } else if spec.direct_uplink.is_some() || spec.start_options.direct_uplink.is_some() {
        let mut existing = state.store.get_vm(&spec.name).map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"error": e.to_string()})),
            )
        })?;
        let Some(vm) = existing.as_mut() else {
            return Err((
                StatusCode::NOT_FOUND,
                Json(json!({"error": "VM not found"})),
            ));
        };
        vm.direct_uplink = spec
            .direct_uplink
            .clone()
            .or_else(|| spec.start_options.direct_uplink.clone());
        vm.direct_mode = spec
            .direct_mode
            .clone()
            .or_else(|| spec.start_options.direct_mode.clone());
        vm.direct_guest_ips = if spec.direct_guest_ips.is_empty() {
            spec.start_options.direct_guest_ips.clone()
        } else {
            spec.direct_guest_ips.clone()
        };
        let errors = vm_model::direct_uplink_errors(
            vm.direct_uplink.as_deref(),
            vm.direct_mode.as_deref(),
            &vm.direct_guest_ips,
            vm.network_tap || spec.start_options.network_tap,
            !vm.port_forwards.is_empty() || spec.start_options.network_user_mode,
        );
        if !errors.is_empty() {
            return Err((
                StatusCode::BAD_REQUEST,
                Json(json!({"error": errors.join("; ")})),
            ));
        }
        state.store.save_vm(vm).map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"error": e.to_string()})),
            )
        })?;
    }

    // Store volume mount config
    if !spec.volumes.is_empty() {
        state
            .store
            .save_entity("vm_volumes", &spec.name, &spec.volumes)
            .map_err(|e| {
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(json!({ "error": e.to_string() })),
                )
            })?;
    }

    // Store autoscale policy
    let autoscale_enabled = if let Some(ref policy) = spec.autoscale {
        state
            .store
            .save_entity("autoscale_policies", &spec.name, policy)
            .map_err(|e| {
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(json!({ "error": e.to_string() })),
                )
            })?;
        true
    } else {
        false
    };

    // Persist start options for export
    let has_non_default_opts = spec.start_options.kvm.is_some()
        || spec.start_options.secure_boot.is_some()
        || spec.start_options.vsock.is_some()
        || spec.start_options.tpm.is_some()
        || spec.start_options.console.is_some()
        || spec.start_options.network_tap
        || spec.start_options.network_user_mode
        || spec.start_options.scope.is_some()
        || spec.start_options.directory.is_some()
        || !spec.start_options.bind_mounts.is_empty()
        || !spec.start_options.set_credentials.is_empty()
        || spec.start_options.forward_journal.is_some()
        || spec.start_options.pass_ssh_key.is_some()
        || spec.start_options.direct_uplink.is_some()
        || !spec.start_options.direct_guest_ips.is_empty();
    if has_non_default_opts {
        state
            .store
            .save_entity("vm_start_options", &spec.name, &spec.start_options)
            .map_err(|e| {
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(json!({ "error": e.to_string() })),
                )
            })?;
    } else {
        // Clear stale options when re-applying with all defaults
        let _ = state.store.delete_entity("vm_start_options", &spec.name);
    }

    // Start VM if requested
    let mut started = false;
    if spec.auto_start {
        let mut start_opts: vm_model::VMStartOptions = spec.start_options.clone().into();

        // Merge top-level VMSpec.credentials into start options
        for cred in &spec.credentials {
            if let Some(file_path) = cred.value.strip_prefix('@') {
                start_opts.load_credentials.push(vm_model::LoadCredential {
                    id: cred.id.clone(),
                    path: file_path.to_string(),
                });
            } else {
                start_opts.credentials.push(vm_model::VMCredential {
                    id: cred.id.clone(),
                    value: cred.value.clone(),
                });
            }
        }

        // Validate start options before attempting start
        if let Err(validation_errors) = start_opts.validate() {
            return Err((
                StatusCode::BAD_REQUEST,
                Json(
                    json!({"error": format!("Invalid start options: {}", validation_errors.join("; "))}),
                ),
            ));
        }

        let vm = match state.store.get_vm(&spec.name) {
            Ok(Some(vm)) => vm,
            Ok(None) => {
                return Err((
                    StatusCode::NOT_FOUND,
                    Json(json!({"error": "VM not found after creation"})),
                ))
            }
            Err(e) => {
                return Err((
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(json!({"error": e.to_string()})),
                ))
            }
        };

        match state.driver.start_with_options(&vm, &start_opts).await {
            Ok(_) => {
                started = true;
                if let Ok(Some(mut vm)) = state.store.get_vm(&spec.name) {
                    vm.state = vm_model::VMState::Running;
                    if let Err(e) = state.store.save_vm(&vm) {
                        tracing::error!("Failed to save VM: {}", e);
                    }
                }
            }
            Err(e) => {
                warnings.push(format!("Failed to start VM: {}", e));
            }
        }
    }

    // Record event
    crate::api::events::record_event(
        &state,
        crate::api::events::VMEventType::Created,
        &spec.name,
        Some("Created via declarative spec".to_string()),
    );

    Ok((
        StatusCode::OK,
        Json(ApplyResult {
            vm_name: spec.name,
            created: !vm_exists,
            started,
            volumes_configured: spec.volumes.len(),
            autoscale_enabled,
            warnings,
        }),
    )
        .into_response())
}

/// POST /api/vms/plan - Plan a set of declarative VM specs against the live
/// state without changing anything. With `"prune": true` VMs missing from the
/// set are planned as deletes; otherwise they are reported as unmanaged.
/// (Apply itself is per-VM and never deletes; a delete entry is advisory.)
pub async fn plan_vm_specs(
    RequireRead(claims): RequireRead,
    State(state): State<Arc<AppState>>,
    Json(desired): Json<super::declarative_plan::DesiredSpec>,
) -> Result<Json<super::declarative_plan::Plan>, ApiError> {
    tracing::debug!("declarative::{}", stringify!(plan_vm_specs));
    if desired.vms.len() > 1000 {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(json!({"error": "vms count must not exceed 1000"})),
        ));
    }
    let mut seen = std::collections::HashSet::new();
    for v in &desired.vms {
        crate::validation::validate_vm_name(&v.name)
            .map_err(|(s, m)| (s, Json(json!({"error": m}))))?;
        if !seen.insert(v.name.as_str()) {
            return Err((
                StatusCode::BAD_REQUEST,
                Json(json!({"error": format!("duplicate VM name in spec: {}", v.name)})),
            ));
        }
    }
    let current: Vec<VMSpec> = state
        .store
        .list_vms()
        .map_err(internal_err)?
        .into_iter()
        .map(|vm| build_vm_spec(&state.store, vm))
        .collect();
    let live = super::declarative_plan::plan(&current, &desired);
    audit_plan_read(
        &state,
        &claims.sub,
        "*",
        &format!(
            "plan of {} VM spec(s), prune={}",
            desired.vms.len(),
            desired.prune
        ),
    )
    .await;
    Ok(Json(live))
}

/// GET /api/vms/:name/spec - Export VM as declarative spec
pub async fn export_vm_spec(
    RequireRead(_claims): RequireRead,
    State(state): State<Arc<AppState>>,
    axum::extract::Path(name): axum::extract::Path<String>,
) -> Result<Json<VMSpec>, (StatusCode, Json<serde_json::Value>)> {
    tracing::debug!("declarative::{}", stringify!(export_vm_spec));
    crate::validation::validate_vm_name(&name).map_err(|(s, m)| (s, Json(json!({"error": m}))))?;
    let vm = match state.store.get_vm(&name) {
        Ok(Some(vm)) => vm,
        Ok(None) => {
            return Err((
                StatusCode::NOT_FOUND,
                Json(json!({ "error": "VM not found" })),
            ))
        }
        Err(e) => {
            return Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": e.to_string() })),
            ))
        }
    };

    let spec = build_vm_spec(&state.store, vm);

    Ok(Json(spec))
}
