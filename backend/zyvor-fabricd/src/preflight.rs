// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: LicenseRef-Zyvor-Production-2.0

//! Pre-flight start check: "can this VM start here?".
//!
//! [`check`] is a pure function over a VM, the start options that would be
//! used, the backend's capabilities and a snapshot of host facts. The
//! `POST /api/vms/{name}/preflight` handler only gathers that snapshot (all
//! reads) and calls it: it never starts, mutates or reserves anything.
//!
//! Severity semantics: a `Blocker` is something the start path itself would
//! reject or that cannot possibly work; a `Warning` is likely trouble or a
//! policy breach the start path does not enforce; `Info` is context only.
//! Rules that depend on this host's filesystem (image path, NIC names) are
//! Warnings, never Blockers, because the FluxVM daemon may live elsewhere.

use std::sync::Arc;

use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use security::RequireRead;
use serde::Serialize;
use vm_model::{VMStartOptions, VMState, VM};

use crate::api::resource_policy::OvercommitPolicy;
use crate::api_error::json_error;
use crate::server::AppState;
use crate::validation::validate_vm_name;

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum Severity {
    Blocker,
    Warning,
    Info,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct Finding {
    pub id: &'static str,
    pub severity: Severity,
    pub message: String,
    /// Plain-text remediation suggestion.
    pub hint: String,
}

impl Finding {
    fn new(
        id: &'static str,
        severity: Severity,
        message: impl Into<String>,
        hint: impl Into<String>,
    ) -> Self {
        Self {
            id,
            severity,
            message: message.into(),
            hint: hint.into(),
        }
    }
}

/// What the active VM backend refuses at start time. For FluxVM this mirrors
/// `translate_start_options` in `fluxvm-driver/src/lifecycle.rs`, which bails
/// on exactly these options (and on a vCPU count that does not fit a `u8`).
#[derive(Debug, Clone, PartialEq)]
pub struct BackendCaps {
    pub name: &'static str,
    pub tpm: bool,
    pub secure_boot: bool,
    pub vsock: bool,
    pub directory_boot: bool,
    pub extra_drives: bool,
    pub bind_users: bool,
    pub credentials: bool,
    pub smbios11: bool,
    pub max_vcpus: u32,
}

impl BackendCaps {
    pub fn fluxvm() -> Self {
        Self {
            name: "fluxvm",
            tpm: false,
            secure_boot: false,
            vsock: false,
            directory_boot: false,
            extra_drives: false,
            bind_users: false,
            credentials: false,
            smbios11: false,
            max_vcpus: u8::MAX as u32,
        }
    }

    /// No known restrictions: used for backends whose limits are not modelled
    /// here, so the check never invents a Blocker for them.
    pub fn permissive(name: &'static str) -> Self {
        Self {
            name,
            tpm: true,
            secure_boot: true,
            vsock: true,
            directory_boot: true,
            extra_drives: true,
            bind_users: true,
            credentials: true,
            smbios11: true,
            max_vcpus: u32::MAX,
        }
    }

    pub fn for_backend(name: &'static str) -> Self {
        match name {
            "fluxvm" => Self::fluxvm(),
            other => Self::permissive(other),
        }
    }
}

/// Host totals plus what other (non-stopped) VMs already hold.
#[derive(Debug, Clone, PartialEq)]
pub struct Capacity {
    pub physical_cpus: u32,
    pub physical_memory_mb: u64,
    /// CPUs allocated to Running/Starting/Paused VMs other than this one.
    pub other_active_cpus: u32,
    pub other_active_memory_mb: u64,
}

/// An enabled quota that applies to the VM, with usage already recomputed
/// live. Usage includes this VM (it is already in the store).
#[derive(Debug, Clone, PartialEq)]
pub struct QuotaView {
    pub name: String,
    pub max_cpus: u32,
    pub max_memory_mb: u64,
    pub max_disk_gb: u64,
    pub max_vms: u32,
    pub used_cpus: u32,
    pub used_memory_mb: u64,
    pub used_disk_gb: u64,
    pub used_vms: u32,
}

#[derive(Debug, Clone, Default)]
pub struct HostInputs {
    pub capacity: Option<Capacity>,
    pub overcommit: OvercommitPolicy,
    pub quotas: Vec<QuotaView>,
    /// Whether the VM's `image` resolves to a file on this host (None = not probed).
    pub image_found: Option<bool>,
    /// Whether the VM's own provisioned disk exists on this host (None = not probed).
    pub disk_found: Option<bool>,
    /// Host network interface names (None = not probed).
    pub host_nics: Option<Vec<String>>,
}

/// Options the backend rejects outright; shared with the start route so it
/// can return the same text instead of a generic driver failure.
pub fn capability_findings(vm: &VM, opts: &VMStartOptions, caps: &BackendCaps) -> Vec<Finding> {
    let mut out = Vec::new();
    let name = caps.name;
    let mut reject = |id: &'static str, what: &str, hint: &str| {
        out.push(Finding::new(
            id,
            Severity::Blocker,
            format!("The {name} backend does not support {what}."),
            hint,
        ));
    };
    if !caps.directory_boot && opts.directory.is_some() {
        reject(
            "backend.directory_boot",
            "directory-based boot",
            "Boot from a disk image instead of a directory.",
        );
    }
    if !caps.tpm && opts.tpm == Some(true) {
        reject(
            "backend.tpm",
            "TPM",
            "Remove the TPM option, or start without tpm=true.",
        );
    }
    if !caps.secure_boot && opts.secure_boot == Some(true) {
        reject(
            "backend.secure_boot",
            "Secure Boot",
            "Remove the Secure Boot option; it is not available on this backend yet.",
        );
    }
    if !caps.vsock && opts.vsock == Some(true) {
        reject(
            "backend.vsock",
            "raw vsock passthrough",
            "Use the in-guest vsock agent instead of the vsock option.",
        );
    }
    if !caps.extra_drives && !opts.extra_drives.is_empty() {
        reject(
            "backend.extra_drives",
            "extra drives at start",
            "Remove extra_drives and attach volumes after the VM is created.",
        );
    }
    if !caps.bind_users && !opts.bind_users.is_empty() {
        reject(
            "backend.bind_users",
            "bind users",
            "Remove bind_users from the start options.",
        );
    }
    if !caps.credentials && (!opts.credentials.is_empty() || !opts.load_credentials.is_empty()) {
        reject(
            "backend.credentials",
            "systemd credentials",
            "Pass the data through cloud-init instead of credentials.",
        );
    }
    if !caps.smbios11 && !opts.smbios11.is_empty() {
        reject(
            "backend.smbios11",
            "SMBIOS injection",
            "Remove smbios11 from the start options.",
        );
    }
    if vm.cpus > caps.max_vcpus {
        out.push(Finding::new(
            "backend.vcpu_limit",
            Severity::Blocker,
            format!(
                "The VM has {} vCPUs but the {name} backend supports at most {}.",
                vm.cpus, caps.max_vcpus
            ),
            "Reduce the vCPU count.",
        ));
    }
    out
}

fn effective(physical: f64, overcommit: &OvercommitPolicy, ratio: f64) -> f64 {
    if overcommit.enabled {
        physical * ratio
    } else {
        physical
    }
}

/// Run every rule. Pure: no I/O, no clock. Findings are ordered
/// Blocker, Warning, Info (stable within a severity).
pub fn check(
    vm: &VM,
    opts: &VMStartOptions,
    caps: &BackendCaps,
    host: &HostInputs,
) -> Vec<Finding> {
    let mut out = Vec::new();

    // VM state
    match vm.state {
        VMState::Running | VMState::Starting | VMState::Stopping => out.push(Finding::new(
            "vm.state",
            Severity::Blocker,
            format!(
                "The VM is already {} and cannot be started.",
                format!("{:?}", vm.state).to_lowercase()
            ),
            "Wait for the current transition to finish, or stop the VM first.",
        )),
        VMState::Paused => out.push(Finding::new(
            "vm.state",
            Severity::Warning,
            "The VM is paused rather than stopped.",
            "Resume the VM instead of starting it.",
        )),
        VMState::Failed => out.push(Finding::new(
            "vm.last_error",
            Severity::Info,
            match &vm.last_error {
                Some(e) => format!("The previous start failed: {e}"),
                None => "The previous start failed.".to_string(),
            },
            "Fix the cause above, then start again.",
        )),
        VMState::Stopped | VMState::Unknown => {}
    }

    // Backend capabilities
    out.extend(capability_findings(vm, opts, caps));

    // Image / disk
    if vm.image.trim().is_empty() {
        out.push(Finding::new(
            "vm.image.missing",
            Severity::Blocker,
            "The VM has no image reference.",
            "Set an image for the VM before starting it.",
        ));
    } else if host.disk_found != Some(true) && host.image_found == Some(false) {
        out.push(Finding::new(
            "vm.image.not_found",
            Severity::Warning,
            format!(
                "Image '{}' was not found on this host and the VM has no provisioned disk here.",
                vm.image
            ),
            "Check the image path or import the image; ignore if FluxVM resolves images elsewhere.",
        ));
    }

    // Networking
    let uplink = opts
        .direct_uplink
        .as_deref()
        .filter(|s| !s.trim().is_empty());
    let uplink = uplink.or(vm.direct_uplink.as_deref().filter(|s| !s.trim().is_empty()));
    let (mode, ips) = if opts
        .direct_uplink
        .as_deref()
        .is_some_and(|s| !s.trim().is_empty())
    {
        (opts.direct_mode.as_deref(), &opts.direct_guest_ips)
    } else {
        (vm.direct_mode.as_deref(), &vm.direct_guest_ips)
    };
    let errors = vm_model::direct_uplink_errors(
        uplink,
        mode,
        ips,
        opts.network_tap,
        opts.network_user_mode || !opts.port_forwards.is_empty(),
    );
    for e in errors {
        out.push(Finding::new(
            "net.direct_uplink.invalid",
            Severity::Blocker,
            format!("Direct uplink configuration is invalid: {e}"),
            "Correct the direct uplink settings (uplink, mode, guest IPs, networking mode).",
        ));
    }
    if let (Some(uplink), Some(nics)) = (uplink, host.host_nics.as_ref()) {
        if !nics.iter().any(|n| n == uplink.trim()) {
            out.push(Finding::new(
                "net.direct_uplink.missing",
                Severity::Warning,
                format!("Host network interface '{}' was not found.", uplink.trim()),
                "Pick an uplink that exists on the host (see the host's network interfaces).",
            ));
        }
    }

    // Storage backend kind (the driver silently maps unknown values to default)
    if let Some(kind) = opts.storage.as_deref().or(vm.storage.as_deref()) {
        let k = kind.trim().to_ascii_lowercase();
        let known = [
            "", "default", "lvm-thin", "lvm_thin", "lvm", "nbd", "ceph-rbd", "ceph_rbd", "ceph",
            "rbd",
        ];
        if !known.contains(&k.as_str()) {
            out.push(Finding::new(
                "storage.backend.unknown",
                Severity::Warning,
                format!(
                    "Storage backend '{kind}' is not recognised; the default backend will be used."
                ),
                "Use one of: default, lvm-thin, nbd, ceph-rbd.",
            ));
        }
    }

    // Host capacity vs overcommit policy
    if let Some(cap) = &host.capacity {
        let oc = &host.overcommit;
        let eff_mem = effective(cap.physical_memory_mb as f64, oc, oc.memory_ratio);
        let eff_cpu = effective(cap.physical_cpus as f64, oc, oc.cpu_ratio);
        let policy = if oc.enabled {
            format!("with {}:1 overcommit", oc.memory_ratio)
        } else {
            "with overcommit disabled".to_string()
        };
        if cap.physical_memory_mb > 0 {
            if vm.memory as f64 > eff_mem {
                out.push(Finding::new(
                    "capacity.memory",
                    Severity::Blocker,
                    format!(
                        "The VM wants {} MB but the host can offer at most {} MB ({policy}).",
                        vm.memory, eff_mem as u64
                    ),
                    "Reduce the VM memory or raise the memory overcommit ratio.",
                ));
            } else if (cap.other_active_memory_mb + vm.memory) as f64 > eff_mem {
                out.push(Finding::new(
                    "capacity.memory.shared",
                    Severity::Warning,
                    format!(
                        "Running VMs hold {} MB; adding {} MB exceeds the host's {} MB ({policy}).",
                        cap.other_active_memory_mb, vm.memory, eff_mem as u64
                    ),
                    "Stop another VM, reduce memory, or raise the overcommit ratio.",
                ));
            }
        }
        if cap.physical_cpus > 0 {
            let cpu_policy = if oc.enabled {
                format!("with {}:1 overcommit", oc.cpu_ratio)
            } else {
                "with overcommit disabled".to_string()
            };
            if vm.cpus as f64 > eff_cpu {
                out.push(Finding::new(
                    "capacity.cpu",
                    Severity::Warning,
                    format!(
                        "The VM has {} vCPUs but the host offers {} ({cpu_policy}).",
                        vm.cpus, eff_cpu as u64
                    ),
                    "Reduce the vCPU count or raise the CPU overcommit ratio.",
                ));
            } else if (cap.other_active_cpus + vm.cpus) as f64 > eff_cpu {
                out.push(Finding::new(
                    "capacity.cpu.shared",
                    Severity::Warning,
                    format!(
                        "Running VMs hold {} vCPUs; adding {} exceeds the host's {} ({cpu_policy}).",
                        cap.other_active_cpus, vm.cpus, eff_cpu as u64
                    ),
                    "Stop another VM, reduce vCPUs, or raise the overcommit ratio.",
                ));
            }
        }
    }

    // Quotas (usage already includes this VM; start does not enforce quotas,
    // so an exceeded quota is a Warning, not a Blocker).
    for q in &host.quotas {
        let mut over = Vec::new();
        if q.used_cpus > q.max_cpus {
            over.push(format!("CPUs {}/{}", q.used_cpus, q.max_cpus));
        }
        if q.used_memory_mb > q.max_memory_mb {
            over.push(format!(
                "memory {}/{} MB",
                q.used_memory_mb, q.max_memory_mb
            ));
        }
        if q.used_disk_gb > q.max_disk_gb {
            over.push(format!("disk {}/{} GB", q.used_disk_gb, q.max_disk_gb));
        }
        if q.used_vms > q.max_vms {
            over.push(format!("VMs {}/{}", q.used_vms, q.max_vms));
        }
        if !over.is_empty() {
            out.push(Finding::new(
                "quota.exceeded",
                Severity::Warning,
                format!("Quota '{}' is exceeded: {}.", q.name, over.join(", ")),
                "Raise the quota or free resources held by other VMs in the same tenant/project.",
            ));
        }
    }

    out.sort_by_key(|f| f.severity);
    out
}

#[derive(Debug, Serialize)]
pub struct PreflightSummary {
    pub blockers: usize,
    pub warnings: usize,
    pub infos: usize,
}

#[derive(Debug, Serialize)]
pub struct PreflightResponse {
    pub vm: String,
    pub backend: &'static str,
    pub can_start: bool,
    pub summary: PreflightSummary,
    pub findings: Vec<Finding>,
}

pub fn build_response(
    vm: &str,
    backend: &'static str,
    findings: Vec<Finding>,
) -> PreflightResponse {
    let count = |s: Severity| findings.iter().filter(|f| f.severity == s).count();
    let summary = PreflightSummary {
        blockers: count(Severity::Blocker),
        warnings: count(Severity::Warning),
        infos: count(Severity::Info),
    };
    PreflightResponse {
        vm: vm.to_string(),
        backend,
        can_start: summary.blockers == 0,
        summary,
        findings,
    }
}

/// POST /api/vms/{name}/preflight — read-only "can this VM start here?".
/// Accepts the same optional `VMStartOptions` body as `POST .../start`.
pub async fn preflight_vm(
    RequireRead(_claims): RequireRead,
    State(state): State<Arc<AppState>>,
    Path(name): Path<String>,
    body: Option<Json<VMStartOptions>>,
) -> impl IntoResponse {
    if let Err((status, msg)) = validate_vm_name(&name) {
        return json_error(status, msg).into_response();
    }
    let vm = match state.store.get_vm(&name) {
        Ok(Some(vm)) => vm,
        Ok(None) => return json_error(StatusCode::NOT_FOUND, "VM not found").into_response(),
        Err(e) => {
            return json_error(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response()
        }
    };
    let opts = match body {
        Some(Json(o)) => {
            if let Err(errors) = o.validate() {
                return json_error(
                    StatusCode::BAD_REQUEST,
                    format!("Invalid start options: {}", errors.join("; ")),
                )
                .into_response();
            }
            o
        }
        None => crate::routes::stored_start_options(&vm),
    };
    let backend = state.driver.backend_name();
    let caps = BackendCaps::for_backend(backend);
    let host = gather_host_inputs(&state, &vm, &opts).await;
    let findings = check(&vm, &opts, &caps, &host);
    (
        StatusCode::OK,
        Json(build_response(&name, backend, findings)),
    )
        .into_response()
}

pub(crate) async fn gather_host_inputs(
    state: &AppState,
    vm: &VM,
    opts: &VMStartOptions,
) -> HostInputs {
    let overcommit = state
        .store
        .get_entity::<OvercommitPolicy>("config", "overcommit")
        .ok()
        .flatten()
        .unwrap_or_default();

    let vms = state.store.list_vms().unwrap_or_default();
    let active = |v: &&VM| {
        v.name != vm.name
            && matches!(
                v.state,
                VMState::Running | VMState::Starting | VMState::Paused
            )
    };
    let capacity = Capacity {
        physical_cpus: crate::api::resource_policy::num_cpus().await,
        physical_memory_mb: crate::api::resource_policy::total_memory_mb().await,
        other_active_cpus: vms.iter().filter(active).map(|v| v.cpus).sum(),
        other_active_memory_mb: vms.iter().filter(active).map(|v| v.memory).sum(),
    };

    let tenant = crate::tenant_scope::vm_tenant(vm);
    let tags = vm.tags.clone().unwrap_or_default();
    let quotas = crate::api::quotas::applicable_quotas_with_usage(state, tenant.as_deref(), &tags)
        .into_iter()
        .map(|q| QuotaView {
            name: q.name,
            max_cpus: q.max_cpus,
            max_memory_mb: q.max_memory,
            max_disk_gb: q.max_disk,
            max_vms: q.max_vms,
            used_cpus: q.used_cpus,
            used_memory_mb: q.used_memory,
            used_disk_gb: q.used_disk,
            used_vms: q.used_vms,
        })
        .collect();

    let image_found = if vm.image.trim().is_empty() || vm.image.contains("..") {
        None
    } else {
        let p = crate::routes::resolve_image_source(state, &vm.image);
        Some(tokio::fs::try_exists(&p).await.unwrap_or(false))
    };
    let disk_found = Some(crate::validation::find_vm_image(&vm.name).is_some());

    let has_uplink = vm.direct_uplink.is_some() || opts.direct_uplink.is_some();
    let host_nics = if has_uplink {
        match tokio::fs::read_dir("/sys/class/net").await {
            Ok(mut rd) => {
                let mut v = Vec::new();
                while let Ok(Some(e)) = rd.next_entry().await {
                    v.push(e.file_name().to_string_lossy().into_owned());
                }
                Some(v)
            }
            Err(_) => None,
        }
    } else {
        None
    };

    HostInputs {
        capacity: Some(capacity),
        overcommit,
        quotas,
        image_found,
        disk_found,
        host_nics,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn vm() -> VM {
        let mut v = VM::new("web-1".into(), "ubuntu.qcow2".into(), 2, 2048);
        v.state = VMState::Stopped;
        v
    }

    fn host() -> HostInputs {
        HostInputs {
            capacity: Some(Capacity {
                physical_cpus: 16,
                physical_memory_mb: 65536,
                other_active_cpus: 4,
                other_active_memory_mb: 8192,
            }),
            overcommit: OvercommitPolicy::default(),
            quotas: vec![],
            image_found: Some(true),
            disk_found: Some(true),
            host_nics: None,
        }
    }

    fn ids(f: &[Finding]) -> Vec<&'static str> {
        f.iter().map(|f| f.id).collect()
    }

    fn run(v: &VM, o: &VMStartOptions, h: &HostInputs) -> Vec<Finding> {
        check(v, o, &BackendCaps::fluxvm(), h)
    }

    #[test]
    fn normal_vm_has_no_findings() {
        assert!(run(&vm(), &VMStartOptions::default(), &host()).is_empty());
    }

    #[test]
    fn unprobed_host_facts_never_produce_findings() {
        let h = HostInputs::default();
        assert!(run(&vm(), &VMStartOptions::default(), &h).is_empty());
    }

    #[test]
    fn state_rules() {
        let cases = [
            (VMState::Stopped, None),
            (VMState::Unknown, None),
            (VMState::Running, Some(Severity::Blocker)),
            (VMState::Starting, Some(Severity::Blocker)),
            (VMState::Stopping, Some(Severity::Blocker)),
            (VMState::Paused, Some(Severity::Warning)),
            (VMState::Failed, Some(Severity::Info)),
        ];
        for (state, want) in cases {
            let mut v = vm();
            v.state = state.clone();
            let f = run(&v, &VMStartOptions::default(), &host());
            match want {
                None => assert!(f.is_empty(), "{state:?}: {f:?}"),
                Some(sev) => {
                    assert_eq!(f.len(), 1, "{state:?}");
                    assert_eq!(f[0].severity, sev, "{state:?}");
                }
            }
        }
    }

    #[test]
    fn failed_state_surfaces_last_error() {
        let mut v = vm();
        v.state = VMState::Failed;
        v.last_error = Some("boom".into());
        let f = run(&v, &VMStartOptions::default(), &host());
        assert!(f[0].message.contains("boom"));
    }

    #[test]
    fn capability_rules_fluxvm() {
        type Mutate = fn(&mut VMStartOptions);
        let cases: Vec<(&str, Mutate)> = vec![
            ("backend.tpm", |o| o.tpm = Some(true)),
            ("backend.secure_boot", |o| o.secure_boot = Some(true)),
            ("backend.vsock", |o| o.vsock = Some(true)),
            ("backend.directory_boot", |o| {
                o.directory = Some("/d".into())
            }),
            ("backend.extra_drives", |o| {
                o.extra_drives = vec!["/x.img".into()]
            }),
            ("backend.bind_users", |o| o.bind_users = vec!["root".into()]),
            ("backend.credentials", |o| {
                o.load_credentials = vec![vm_model::LoadCredential {
                    id: "a".into(),
                    path: "/p".into(),
                }]
            }),
            ("backend.smbios11", |o| o.smbios11 = vec!["io.a=b".into()]),
        ];
        for (id, m) in cases {
            let mut o = VMStartOptions::default();
            m(&mut o);
            let f = run(&vm(), &o, &host());
            assert_eq!(ids(&f), vec![id], "{id}");
            assert_eq!(f[0].severity, Severity::Blocker);
            assert!(!f[0].hint.is_empty());
        }
    }

    #[test]
    fn explicit_false_options_are_not_blockers() {
        let o = VMStartOptions {
            tpm: Some(false),
            secure_boot: Some(false),
            vsock: Some(false),
            ..Default::default()
        };
        assert!(run(&vm(), &o, &host()).is_empty());
    }

    #[test]
    fn permissive_backend_accepts_everything() {
        let o = VMStartOptions {
            tpm: Some(true),
            secure_boot: Some(true),
            ..Default::default()
        };
        let f = check(&vm(), &o, &BackendCaps::permissive("other"), &host());
        assert!(f.is_empty());
    }

    #[test]
    fn vcpu_limit() {
        let mut v = vm();
        v.cpus = 255;
        assert!(run(&v, &VMStartOptions::default(), &HostInputs::default()).is_empty());
        v.cpus = 256;
        let f = run(&v, &VMStartOptions::default(), &HostInputs::default());
        assert_eq!(ids(&f), vec!["backend.vcpu_limit"]);
    }

    #[test]
    fn image_rules() {
        let mut v = vm();
        v.image = "  ".into();
        assert_eq!(
            ids(&run(&v, &VMStartOptions::default(), &host())),
            vec!["vm.image.missing"]
        );
        // Image absent on host but VM has its own disk: fine.
        let mut h = host();
        h.image_found = Some(false);
        assert!(run(&vm(), &VMStartOptions::default(), &h).is_empty());
        // Neither: warning only, never a blocker.
        h.disk_found = Some(false);
        let f = run(&vm(), &VMStartOptions::default(), &h);
        assert_eq!(ids(&f), vec!["vm.image.not_found"]);
        assert_eq!(f[0].severity, Severity::Warning);
    }

    #[test]
    fn direct_uplink_rules() {
        let mut v = vm();
        v.direct_uplink = Some("eth9".into());
        let mut h = host();
        h.host_nics = Some(vec!["lo".into(), "eth0".into()]);
        let f = run(&v, &VMStartOptions::default(), &h);
        assert_eq!(ids(&f), vec!["net.direct_uplink.missing"]);
        assert_eq!(f[0].severity, Severity::Warning);
        h.host_nics = Some(vec!["lo".into(), "eth9".into()]);
        assert!(run(&v, &VMStartOptions::default(), &h).is_empty());
        // Conflicting networking modes are invalid.
        let o = VMStartOptions {
            network_tap: true,
            ..Default::default()
        };
        let f = run(&v, &o, &h);
        assert!(ids(&f).contains(&"net.direct_uplink.invalid"));
        assert_eq!(f[0].severity, Severity::Blocker);
    }

    #[test]
    fn storage_backend_kind() {
        for (kind, bad) in [
            ("default", false),
            ("LVM-Thin", false),
            ("ceph", false),
            ("zfs", true),
        ] {
            let mut v = vm();
            v.storage = Some(kind.into());
            let f = run(&v, &VMStartOptions::default(), &host());
            assert_eq!(!f.is_empty(), bad, "{kind}");
            if bad {
                assert_eq!(f[0].severity, Severity::Warning);
            }
        }
    }

    #[test]
    fn capacity_rules() {
        // (vm cpus, vm mem, overcommit enabled, expected ids)
        let cases: Vec<(u32, u64, bool, Vec<&str>)> = vec![
            (2, 2048, false, vec![]),
            (2, 70000, false, vec!["capacity.memory"]),
            // 1.5x of 65536 = 98304: 70000 fits alone, not next to the running 8192.
            (2, 70000, true, vec![]),
            (2, 100000, true, vec!["capacity.memory"]),
            // 60000 fits alone but not next to the 8192 already running.
            (2, 60000, false, vec!["capacity.memory.shared"]),
            (17, 2048, false, vec!["capacity.cpu"]),
            (13, 2048, false, vec!["capacity.cpu.shared"]),
            (17, 2048, true, vec![]),
        ];
        for (cpus, mem, oc, want) in cases {
            let mut v = vm();
            v.cpus = cpus;
            v.memory = mem;
            let mut h = host();
            h.overcommit = OvercommitPolicy {
                enabled: oc,
                ..OvercommitPolicy::default()
            };
            let f = run(&v, &VMStartOptions::default(), &h);
            assert_eq!(ids(&f), want, "cpus={cpus} mem={mem} oc={oc}");
        }
    }

    #[test]
    fn memory_over_capacity_is_the_only_capacity_blocker() {
        let mut v = vm();
        v.cpus = 64;
        v.memory = 999_999;
        let f = run(&v, &VMStartOptions::default(), &host());
        let blockers: Vec<_> = f
            .iter()
            .filter(|f| f.severity == Severity::Blocker)
            .collect();
        assert_eq!(blockers.len(), 1);
        assert_eq!(blockers[0].id, "capacity.memory");
    }

    #[test]
    fn quota_rules() {
        let ok = QuotaView {
            name: "t".into(),
            max_cpus: 8,
            max_memory_mb: 8192,
            max_disk_gb: 100,
            max_vms: 4,
            used_cpus: 8,
            used_memory_mb: 8192,
            used_disk_gb: 100,
            used_vms: 4,
        };
        let mut h = host();
        h.quotas = vec![ok.clone()];
        assert!(
            run(&vm(), &VMStartOptions::default(), &h).is_empty(),
            "at limit is fine"
        );
        let mut over = ok;
        over.used_memory_mb = 9000;
        over.used_vms = 5;
        h.quotas = vec![over];
        let f = run(&vm(), &VMStartOptions::default(), &h);
        assert_eq!(ids(&f), vec!["quota.exceeded"]);
        assert_eq!(f[0].severity, Severity::Warning);
        assert!(f[0].message.contains("memory") && f[0].message.contains("VMs"));
    }

    #[test]
    fn findings_sorted_blockers_first_and_response_flag() {
        let mut v = vm();
        v.state = VMState::Failed;
        let o = VMStartOptions {
            tpm: Some(true),
            ..Default::default()
        };
        let f = run(&v, &o, &host());
        assert_eq!(f[0].severity, Severity::Blocker);
        assert_eq!(f.last().unwrap().severity, Severity::Info);
        let r = build_response("web-1", "fluxvm", f);
        assert!(!r.can_start);
        assert_eq!((r.summary.blockers, r.summary.infos), (1, 1));
        let r = build_response("web-1", "fluxvm", vec![]);
        assert!(r.can_start);
    }
}
