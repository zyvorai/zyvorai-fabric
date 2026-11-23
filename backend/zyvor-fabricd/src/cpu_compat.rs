// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: LicenseRef-Zyvor-Production-2.0

//! CPU model and feature inventory with a start/migration compatibility check.
//!
//! * Host inventory: vendor, model name and feature flags. The daemon can read
//!   its own `/proc/cpuinfo` (bounded); other hosts are reported with
//!   `PUT /api/hosts/{id}/cpu-inventory`, because the host agent heartbeat does
//!   not carry CPU data. A host without an inventory is "unknown", never a
//!   blocker.
//! * VM requirement: derived from the VM's stored `vm_cpu_config` (the
//!   `/api/vms/{name}/cpu-model` document). A VM with no stored config has no
//!   requirement. In `custom` mode the required features are the named model's
//!   features (a built-in approximation of the QEMU model definitions) plus
//!   `features_add` minus `features_remove`. In `host-model` and
//!   `host-passthrough` modes a *running* VM needs the target to offer what the
//!   source host offers (same vendor, feature superset); a stopped VM has no
//!   requirement because the guest CPU is resolved at its next start.
//! * Only a curated list of guest-visible ISA features is compared, so kernel
//!   synthesised flags and mitigation flags never cause a false blocker.
//!
//! Everything here is pure except the `/proc/cpuinfo` reader and the handlers.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    Json,
};
use security::{RequireRead, RequireWrite};
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::api::drain_plan::HOST_LABEL;
use crate::api::vm_advanced_config::CpuModelConfig;
use crate::api_error::json_error;
use crate::server::AppState;
use datacenter::HostInfo;

pub const ENTITY: &str = "host_cpu";
const MAX_CPUINFO_BYTES: u64 = 256 * 1024;
const MAX_FLAGS: usize = 1024;
const MAX_FLAG_LEN: usize = 48;
const MAX_VMS_LISTED: usize = 2000;

// ---------------------------------------------------------------------------
// Feature tables (cpuinfo flag names)
// ---------------------------------------------------------------------------

const V2: &[&str] = &[
    "cx16", "lahf_lm", "popcnt", "pni", "sse4_1", "sse4_2", "ssse3",
];
const V3_EXTRA: &[&str] = &[
    "avx", "avx2", "bmi1", "bmi2", "f16c", "fma", "abm", "movbe", "xsave",
];
const V4_EXTRA: &[&str] = &["avx512f", "avx512bw", "avx512cd", "avx512dq", "avx512vl"];
const WESTMERE_EXTRA: &[&str] = &["pclmulqdq", "aes"];
const SANDY_EXTRA: &[&str] = &["avx", "xsave"];
const IVY_EXTRA: &[&str] = &["f16c", "rdrand", "fsgsbase"];
const HASWELL_EXTRA: &[&str] = &[
    "avx2", "bmi1", "bmi2", "fma", "movbe", "abm", "erms", "invpcid",
];
const BROADWELL_EXTRA: &[&str] = &["rdseed", "adx", "smap"];
const SKYLAKE_EXTRA: &[&str] = &[
    "avx512f", "avx512dq", "avx512cd", "avx512bw", "avx512vl", "clwb", "pku",
];
const CASCADE_EXTRA: &[&str] = &["avx512_vnni"];
const ICELAKE_EXTRA: &[&str] = &[
    "avx512_vbmi",
    "avx512_vbmi2",
    "gfni",
    "vaes",
    "vpclmulqdq",
    "avx512_bitalg",
    "avx512_vpopcntdq",
];
const EPYC_EXTRA: &[&str] = &[
    "avx2",
    "bmi1",
    "bmi2",
    "fma",
    "f16c",
    "movbe",
    "abm",
    "sse4a",
    "rdseed",
    "adx",
    "smap",
    "clflushopt",
    "sha_ni",
    "aes",
    "pclmulqdq",
    "xsave",
];
const EPYC_ROME_EXTRA: &[&str] = &["clwb", "wbnoinvd"];
const EPYC_MILAN_EXTRA: &[&str] = &["vaes", "vpclmulqdq", "pku"];

/// Features of a named model as cpuinfo flag names. Approximates the QEMU
/// definitions; unknown names return `None`.
pub fn model_features(model: &str) -> Option<BTreeSet<&'static str>> {
    let norm: String = model
        .trim()
        .to_ascii_lowercase()
        .chars()
        .filter(|c| c.is_ascii_alphanumeric())
        .collect();
    let mut s: BTreeSet<&'static str> = BTreeSet::new();
    let mut add = |parts: &[&[&'static str]]| {
        for p in parts {
            s.extend(p.iter().copied());
        }
    };
    match norm.as_str() {
        "qemu64" | "kvm64" | "qemu32" | "kvm32" => {}
        "x8664v2" | "nehalem" => add(&[V2]),
        "x8664v3" => add(&[V2, V3_EXTRA]),
        "x8664v4" => add(&[V2, V3_EXTRA, V4_EXTRA]),
        "westmere" => add(&[V2, WESTMERE_EXTRA]),
        "sandybridge" => add(&[V2, WESTMERE_EXTRA, SANDY_EXTRA]),
        "ivybridge" => add(&[V2, WESTMERE_EXTRA, SANDY_EXTRA, IVY_EXTRA]),
        "haswell" => add(&[V2, WESTMERE_EXTRA, SANDY_EXTRA, IVY_EXTRA, HASWELL_EXTRA]),
        "broadwell" => add(&[
            V2,
            WESTMERE_EXTRA,
            SANDY_EXTRA,
            IVY_EXTRA,
            HASWELL_EXTRA,
            BROADWELL_EXTRA,
        ]),
        "skylakeserver" => add(&[
            V2,
            WESTMERE_EXTRA,
            SANDY_EXTRA,
            IVY_EXTRA,
            HASWELL_EXTRA,
            BROADWELL_EXTRA,
            SKYLAKE_EXTRA,
        ]),
        "cascadelakeserver" => add(&[
            V2,
            WESTMERE_EXTRA,
            SANDY_EXTRA,
            IVY_EXTRA,
            HASWELL_EXTRA,
            BROADWELL_EXTRA,
            SKYLAKE_EXTRA,
            CASCADE_EXTRA,
        ]),
        "icelakeserver" => add(&[
            V2,
            WESTMERE_EXTRA,
            SANDY_EXTRA,
            IVY_EXTRA,
            HASWELL_EXTRA,
            BROADWELL_EXTRA,
            SKYLAKE_EXTRA,
            CASCADE_EXTRA,
            ICELAKE_EXTRA,
        ]),
        "epyc" => add(&[V2, EPYC_EXTRA]),
        "epycrome" => add(&[V2, EPYC_EXTRA, EPYC_ROME_EXTRA]),
        "epycmilan" => add(&[V2, EPYC_EXTRA, EPYC_ROME_EXTRA, EPYC_MILAN_EXTRA]),
        _ => return None,
    }
    Some(s)
}

const MODEL_NAMES: &[&str] = &[
    "qemu64",
    "x86-64-v2",
    "Nehalem",
    "Westmere",
    "SandyBridge",
    "IvyBridge",
    "Haswell",
    "x86-64-v3",
    "Broadwell",
    "x86-64-v4",
    "Skylake-Server",
    "Cascadelake-Server",
    "Icelake-Server",
    "EPYC",
    "EPYC-Rome",
    "EPYC-Milan",
];

/// Every feature the check is willing to compare.
fn known_flags() -> BTreeSet<&'static str> {
    let mut k = BTreeSet::new();
    for m in MODEL_NAMES {
        if let Some(f) = model_features(m) {
            k.extend(f);
        }
    }
    k
}

/// Map QEMU/libvirt feature spellings to cpuinfo flag names.
pub fn normalize_feature(f: &str) -> String {
    let l = f.trim().to_ascii_lowercase().replace(['.', '-'], "_");
    match l.as_str() {
        "sse3" => "pni".into(),
        "lzcnt" => "abm".into(),
        "sse4a" => "sse4a".into(),
        "avx512_f" => "avx512f".into(),
        _ => l,
    }
}

// ---------------------------------------------------------------------------
// Host CPU
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct HostCpu {
    pub vendor: String,
    pub model_name: String,
    pub flags: BTreeSet<String>,
}

/// Stored per host under [`ENTITY`].
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct HostCpuRecord {
    pub host_id: String,
    #[serde(flatten)]
    pub cpu: HostCpu,
    /// `local` (read from the daemon host) or `reported`.
    pub source: String,
    pub updated: chrono::DateTime<chrono::Utc>,
}

fn clean_text(s: &str, max: usize) -> Result<String, String> {
    let t = s.trim();
    if t.chars().count() > max {
        return Err(format!("value longer than {max} characters"));
    }
    if t.chars().any(|c| c.is_control()) {
        return Err("control characters are not allowed".into());
    }
    Ok(t.to_string())
}

/// Validate and normalise untrusted flag names.
pub fn sanitize_flags<I, S>(flags: I) -> Result<BTreeSet<String>, String>
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    let mut out = BTreeSet::new();
    for f in flags {
        let f = f.as_ref().trim().to_ascii_lowercase();
        if f.is_empty() {
            continue;
        }
        if f.len() > MAX_FLAG_LEN
            || !f
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '.' | '-'))
        {
            return Err(format!(
                "invalid flag name (max {MAX_FLAG_LEN} chars of a-z 0-9 _ . -)"
            ));
        }
        out.insert(f);
        if out.len() > MAX_FLAGS {
            return Err(format!("more than {MAX_FLAGS} flags"));
        }
    }
    Ok(out)
}

pub fn validated_cpu(vendor: &str, model_name: &str, flags: &[String]) -> Result<HostCpu, String> {
    let vendor = clean_text(vendor, 64).map_err(|e| format!("vendor: {e}"))?;
    let model_name = clean_text(model_name, 128).map_err(|e| format!("model_name: {e}"))?;
    let flags = sanitize_flags(flags)?;
    if flags.is_empty() {
        return Err("flags must not be empty".into());
    }
    Ok(HostCpu {
        vendor,
        model_name,
        flags,
    })
}

/// Parse the first processor block of `/proc/cpuinfo` (x86 `flags`, or the
/// Arm `Features` line). Malformed or oversized flag lists yield `None`.
pub fn parse_cpuinfo(text: &str) -> Option<HostCpu> {
    let mut cpu = HostCpu::default();
    let mut have_flags = false;
    for line in text.lines() {
        if line.trim().is_empty() {
            if have_flags {
                break;
            }
            continue;
        }
        let Some((k, v)) = line.split_once(':') else {
            continue;
        };
        let (k, v) = (k.trim(), v.trim());
        match k {
            "vendor_id" if cpu.vendor.is_empty() => {
                cpu.vendor = clean_text(v, 64).unwrap_or_default()
            }
            "model name" if cpu.model_name.is_empty() => {
                cpu.model_name = clean_text(v, 128).unwrap_or_default()
            }
            "flags" | "Features" if !have_flags => {
                cpu.flags = sanitize_flags(v.split_whitespace()).ok()?;
                have_flags = true;
            }
            _ => {}
        }
    }
    have_flags.then_some(cpu)
}

/// Read the daemon host's CPU, bounded to 256 KiB.
pub async fn read_local_cpu() -> Option<HostCpu> {
    use tokio::io::AsyncReadExt;
    let f = tokio::fs::File::open("/proc/cpuinfo").await.ok()?;
    let mut buf = Vec::new();
    f.take(MAX_CPUINFO_BYTES).read_to_end(&mut buf).await.ok()?;
    parse_cpuinfo(&String::from_utf8_lossy(&buf))
}

// ---------------------------------------------------------------------------
// Requirement and check
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Default, Serialize, PartialEq, Eq)]
pub struct CpuRequirement {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub vendor: Option<String>,
    pub features: BTreeSet<String>,
    /// Why the requirement is empty or partial (unknown model, unknown source
    /// host CPU, features that are not compared).
    pub notes: Vec<String>,
}

impl CpuRequirement {
    pub fn is_empty(&self) -> bool {
        self.vendor.is_none() && self.features.is_empty()
    }
}

/// Requirement of a VM with a stored CPU config. `source` is the CPU of the
/// host the VM runs on (needed for host-model/host-passthrough); `running`
/// says whether the guest CPU is already fixed.
pub fn requirement(
    cfg: &CpuModelConfig,
    source: Option<&HostCpu>,
    running: bool,
) -> CpuRequirement {
    let known = known_flags();
    let mut req = CpuRequirement::default();
    match cfg.mode.as_str() {
        "custom" => {
            match model_features(&cfg.model) {
                Some(f) => req.features.extend(f.into_iter().map(String::from)),
                None => req.notes.push(format!(
                    "CPU model '{}' is not in the built-in table; its features are not checked",
                    cfg.model
                )),
            }
            for f in &cfg.features_add {
                let n = normalize_feature(f);
                if known.contains(n.as_str()) {
                    req.features.insert(n);
                } else {
                    req.notes
                        .push(format!("feature '{f}' is not in the compared set; ignored"));
                }
            }
            for f in &cfg.features_remove {
                req.features.remove(&normalize_feature(f));
            }
            if let Some(v) = cfg.vendor.as_deref().filter(|v| !v.trim().is_empty()) {
                let v = v.trim();
                if v.eq_ignore_ascii_case("intel") {
                    req.vendor = Some("GenuineIntel".into());
                } else if v.eq_ignore_ascii_case("amd") {
                    req.vendor = Some("AuthenticAMD".into());
                } else {
                    req.vendor = Some(v.to_string());
                }
            }
        }
        "host-model" | "host-passthrough" => {
            if !running {
                req.notes.push(
                    "stopped VM: the guest CPU is resolved from the host at next start".into(),
                );
            } else if let Some(src) = source {
                req.vendor = Some(src.vendor.clone()).filter(|v| !v.is_empty());
                req.features = src
                    .flags
                    .iter()
                    .filter(|f| known.contains(f.as_str()))
                    .cloned()
                    .collect();
                if cfg.mode == "host-model" {
                    for f in &cfg.features_remove {
                        req.features.remove(&normalize_feature(f));
                    }
                }
            } else {
                req.notes.push(
                    "source host CPU is unknown; host-derived requirement not checked".into(),
                );
            }
        }
        other => req
            .notes
            .push(format!("unknown CPU mode '{other}'; not checked")),
    }
    req
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(tag = "verdict", rename_all = "snake_case")]
pub enum CpuVerdict {
    Compatible,
    /// The target has no CPU inventory.
    Unknown,
    Incompatible {
        missing: Vec<String>,
        vendor_mismatch: bool,
    },
}

pub fn check(req: &CpuRequirement, target: Option<&HostCpu>) -> CpuVerdict {
    if req.is_empty() {
        return CpuVerdict::Compatible;
    }
    let Some(t) = target else {
        return CpuVerdict::Unknown;
    };
    let vendor_mismatch = req
        .vendor
        .as_deref()
        .is_some_and(|v| !t.vendor.is_empty() && !v.eq_ignore_ascii_case(&t.vendor));
    let missing: Vec<String> = req
        .features
        .iter()
        .filter(|f| !t.flags.contains(*f))
        .cloned()
        .collect();
    if vendor_mismatch || !missing.is_empty() {
        CpuVerdict::Incompatible {
            missing,
            vendor_mismatch,
        }
    } else {
        CpuVerdict::Compatible
    }
}

/// Short human text for a failed check (at most 8 features listed).
pub fn describe_incompatible(missing: &[String], vendor_mismatch: bool) -> String {
    let mut parts = Vec::new();
    if vendor_mismatch {
        parts.push("CPU vendor differs".to_string());
    }
    if !missing.is_empty() {
        let shown: Vec<&str> = missing.iter().take(8).map(String::as_str).collect();
        let more = missing.len().saturating_sub(8);
        parts.push(if more > 0 {
            format!("lacks {} (+{more} more)", shown.join(", "))
        } else {
            format!("lacks {}", shown.join(", "))
        });
    }
    parts.join("; ")
}

/// Highest named model every given host can run (most features wins), plus
/// the number of compared features all hosts share.
pub fn baseline(hosts: &[&HostCpu]) -> (Option<&'static str>, usize) {
    if hosts.is_empty() {
        return (None, 0);
    }
    let known = known_flags();
    let common = known
        .iter()
        .filter(|f| hosts.iter().all(|h| h.flags.contains(**f)))
        .count();
    let mut best: Option<(&'static str, usize)> = None;
    for m in MODEL_NAMES {
        let f = model_features(m).expect("table model");
        if f.iter().all(|x| hosts.iter().all(|h| h.flags.contains(*x)))
            && best.is_none_or(|(_, n)| f.len() > n)
        {
            best = Some((m, f.len()));
        }
    }
    (best.map(|b| b.0), common)
}

// ---------------------------------------------------------------------------
// Handlers
// ---------------------------------------------------------------------------

type ApiErr = (StatusCode, Json<serde_json::Value>);

fn ise(e: impl ToString) -> ApiErr {
    json_error(StatusCode::INTERNAL_SERVER_ERROR, e.to_string())
}

fn deny_tenant(claims: &security::Claims) -> Result<(), ApiErr> {
    if claims.tenant.is_some() {
        return Err(json_error(
            StatusCode::FORBIDDEN,
            "Host operations are not available to tenant-scoped tokens",
        ));
    }
    Ok(())
}

fn find_host<'a>(hosts: &'a [HostInfo], id: &str) -> Option<&'a HostInfo> {
    hosts.iter().find(|h| h.id == id || h.hostname == id)
}

fn host_cpu(state: &AppState, host_id: &str) -> Option<HostCpuRecord> {
    state
        .store
        .get_entity::<HostCpuRecord>(ENTITY, host_id)
        .ok()
        .flatten()
}

/// The stored CPU document of a VM, only when one was explicitly saved.
pub fn stored_vm_cpu(state: &AppState, vm: &str) -> Option<CpuModelConfig> {
    state
        .store
        .get_entity::<CpuModelConfig>("vm_cpu_config", vm)
        .ok()
        .flatten()
}

/// GET /api/cpu/inventory - host CPUs, VM CPU configs and per-cluster baseline.
pub async fn inventory(
    RequireRead(claims): RequireRead,
    State(state): State<Arc<AppState>>,
) -> Result<Json<serde_json::Value>, ApiErr> {
    deny_tenant(&claims)?;
    let hosts: Vec<HostInfo> = state.store.list_entities("hosts").map_err(ise)?;
    let mut rows = Vec::new();
    let mut by_cluster: BTreeMap<String, (usize, Vec<HostCpu>)> = BTreeMap::new();
    for h in &hosts {
        let rec = host_cpu(&state, &h.id);
        let e = by_cluster.entry(h.cluster_id.clone()).or_default();
        e.0 += 1;
        if let Some(r) = &rec {
            e.1.push(r.cpu.clone());
        }
        rows.push(json!({
            "host_id": h.id, "hostname": h.hostname, "cluster_id": h.cluster_id,
            "cpu": rec,
        }));
    }
    let clusters: Vec<_> = by_cluster
        .iter()
        .map(|(id, (total, cpus))| {
            let refs: Vec<&HostCpu> = cpus.iter().collect();
            let (model, common) = baseline(&refs);
            json!({
                "cluster_id": id, "hosts_total": total, "hosts_known": cpus.len(),
                "baseline_model": model, "common_features": common,
            })
        })
        .collect();
    let all = state.store.list_vms().map_err(ise)?;
    let truncated = all.len() > MAX_VMS_LISTED;
    let vms: Vec<_> = all
        .iter()
        .take(MAX_VMS_LISTED)
        .map(|v| match stored_vm_cpu(&state, &v.name) {
            Some(c) => json!({"name": v.name, "configured": true, "model": c.model,
                "mode": c.mode, "features_add": c.features_add,
                "features_remove": c.features_remove}),
            None => json!({"name": v.name, "configured": false}),
        })
        .collect();
    Ok(Json(json!({
        "hosts": rows, "clusters": clusters, "vms": vms, "vms_truncated": truncated,
        "notes": [
            "Only a curated set of guest-visible ISA features is compared.",
            "Hosts without an inventory are reported as unknown, never as incompatible.",
        ],
    })))
}

/// GET /api/hosts/{id}/cpu-inventory
pub async fn get_host_cpu(
    RequireRead(claims): RequireRead,
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<HostCpuRecord>, ApiErr> {
    deny_tenant(&claims)?;
    let hosts: Vec<HostInfo> = state.store.list_entities("hosts").map_err(ise)?;
    let h = find_host(&hosts, &id)
        .ok_or_else(|| json_error(StatusCode::NOT_FOUND, "Host not found"))?;
    host_cpu(&state, &h.id)
        .map(Json)
        .ok_or_else(|| json_error(StatusCode::NOT_FOUND, "No CPU inventory for this host"))
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PutHostCpu {
    /// Capture the daemon host's own `/proc/cpuinfo` instead of the fields below.
    #[serde(default)]
    pub from_local: bool,
    #[serde(default)]
    pub vendor: String,
    #[serde(default)]
    pub model_name: String,
    #[serde(default)]
    pub flags: Vec<String>,
}

/// PUT /api/hosts/{id}/cpu-inventory
pub async fn put_host_cpu(
    RequireWrite(claims): RequireWrite,
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    Json(body): Json<PutHostCpu>,
) -> Result<Json<HostCpuRecord>, ApiErr> {
    deny_tenant(&claims)?;
    let hosts: Vec<HostInfo> = state.store.list_entities("hosts").map_err(ise)?;
    let h = find_host(&hosts, &id)
        .ok_or_else(|| json_error(StatusCode::NOT_FOUND, "Host not found"))?;
    let (cpu, source) = if body.from_local {
        let c = read_local_cpu().await.ok_or_else(|| {
            json_error(
                StatusCode::SERVICE_UNAVAILABLE,
                "Could not read CPU flags from /proc/cpuinfo on the daemon host",
            )
        })?;
        (c, "local")
    } else {
        (
            validated_cpu(&body.vendor, &body.model_name, &body.flags)
                .map_err(|e| json_error(StatusCode::BAD_REQUEST, e))?,
            "reported",
        )
    };
    let rec = HostCpuRecord {
        host_id: h.id.clone(),
        cpu,
        source: source.into(),
        updated: chrono::Utc::now(),
    };
    state.store.save_entity(ENTITY, &h.id, &rec).map_err(ise)?;
    crate::routes::audit(
        &state,
        &claims.sub,
        "SET_CPU_INVENTORY",
        &format!("host/{}", h.id),
        "SUCCESS",
    );
    Ok(Json(rec))
}

#[derive(Debug, Deserialize)]
pub struct CompatQuery {
    /// Host id or hostname; default is every host with a registered record.
    pub host: Option<String>,
}

/// GET /api/vms/{name}/cpu-compat?host=...
pub async fn vm_compat(
    RequireRead(claims): RequireRead,
    State(state): State<Arc<AppState>>,
    Path(name): Path<String>,
    Query(q): Query<CompatQuery>,
) -> Result<Json<serde_json::Value>, ApiErr> {
    deny_tenant(&claims)?;
    crate::validation::validate_vm_name(&name).map_err(|(s, m)| json_error(s, m))?;
    let vm = state
        .store
        .get_vm(&name)
        .map_err(ise)?
        .ok_or_else(|| json_error(StatusCode::NOT_FOUND, "VM not found"))?;
    let hosts: Vec<HostInfo> = state.store.list_entities("hosts").map_err(ise)?;
    let targets: Vec<&HostInfo> = match &q.host {
        Some(h) => vec![find_host(&hosts, h)
            .ok_or_else(|| json_error(StatusCode::NOT_FOUND, "Host not found"))?],
        None => hosts.iter().collect(),
    };
    let source = vm
        .labels
        .as_ref()
        .and_then(|l| l.get(HOST_LABEL))
        .and_then(|l| find_host(&hosts, l))
        .and_then(|h| host_cpu(&state, &h.id));
    let running = matches!(
        vm.state,
        vm_model::VMState::Running | vm_model::VMState::Paused | vm_model::VMState::Starting
    );
    let Some(cfg) = stored_vm_cpu(&state, &name) else {
        return Ok(Json(json!({
            "vm": name, "configured": false, "requirement": CpuRequirement::default(),
            "results": [], "notes": ["no CPU model stored for this VM; nothing to check"],
        })));
    };
    let req = requirement(&cfg, source.as_ref().map(|r| &r.cpu), running);
    let results: Vec<_> = targets
        .iter()
        .map(|h| {
            let rec = host_cpu(&state, &h.id);
            let v = check(&req, rec.as_ref().map(|r| &r.cpu));
            json!({"host_id": h.id, "hostname": h.hostname, "result": v})
        })
        .collect();
    Ok(Json(json!({
        "vm": name, "configured": true, "mode": cfg.mode, "model": cfg.model,
        "requirement": req, "results": results,
    })))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cpu(vendor: &str, flags: &[&str]) -> HostCpu {
        HostCpu {
            vendor: vendor.into(),
            model_name: "test".into(),
            flags: flags.iter().map(|s| s.to_string()).collect(),
        }
    }
    fn cfg(mode: &str, model: &str) -> CpuModelConfig {
        CpuModelConfig {
            model: model.into(),
            mode: mode.into(),
            ..Default::default()
        }
    }
    fn with(flag_sets: &[&[&str]]) -> HostCpu {
        let mut f: Vec<&str> = Vec::new();
        for s in flag_sets {
            f.extend(s.iter());
        }
        cpu("GenuineIntel", &f)
    }

    #[test]
    fn parses_first_block_only_and_ignores_garbage() {
        let t = "processor\t: 0\nvendor_id\t: GenuineIntel\nmodel name\t: Xeon X\nflags\t\t: fpu sse4_2 AVX2\n\nprocessor\t: 1\nflags\t: other\n";
        let c = parse_cpuinfo(t).unwrap();
        assert_eq!(c.vendor, "GenuineIntel");
        assert_eq!(c.model_name, "Xeon X");
        assert!(c.flags.contains("avx2") && !c.flags.contains("other"));
        assert!(parse_cpuinfo("nothing useful").is_none());
        assert!(parse_cpuinfo("").is_none());
        // hostile flag name rejects the whole record
        assert!(parse_cpuinfo("flags : ok bad;rm").is_none());
        let long = format!("flags : {}", "a".repeat(100));
        assert!(parse_cpuinfo(&long).is_none());
    }

    #[test]
    fn arm_features_line_is_accepted() {
        let c = parse_cpuinfo("Features : fp asimd aes\n").unwrap();
        assert!(c.flags.contains("asimd"));
    }

    #[test]
    fn sanitize_and_validate_hostile_input() {
        assert!(sanitize_flags(["avx2", "AES"]).unwrap().contains("aes"));
        assert!(sanitize_flags(["a b"]).is_err());
        assert!(sanitize_flags(["../x"]).is_err());
        let many: Vec<String> = (0..2000).map(|i| format!("f{i}")).collect();
        assert!(sanitize_flags(&many).is_err());
        assert!(validated_cpu("Intel", "x", &[]).is_err());
        assert!(validated_cpu("In\ntel", "x", &["avx".into()]).is_err());
        assert!(validated_cpu(&"v".repeat(65), "x", &["avx".into()]).is_err());
        assert!(validated_cpu("Intel", &"m".repeat(129), &["avx".into()]).is_err());
        assert!(validated_cpu("Intel", "m", &["avx".into()]).is_ok());
    }

    #[test]
    fn model_table_is_cumulative_and_name_insensitive() {
        let hw = model_features("haswell").unwrap();
        let bw = model_features("Broadwell").unwrap();
        assert!(hw.is_subset(&bw) && bw.len() > hw.len());
        assert_eq!(model_features("x86-64-v3"), model_features("X86_64_V3"));
        assert!(model_features("qemu64").unwrap().is_empty());
        assert!(model_features("NoSuchModel").is_none());
    }

    #[test]
    fn custom_model_blocks_older_host_and_names_features() {
        let req = requirement(&cfg("custom", "Haswell"), None, true);
        let old = with(&[V2, WESTMERE_EXTRA]);
        match check(&req, Some(&old)) {
            CpuVerdict::Incompatible {
                missing,
                vendor_mismatch,
            } => {
                assert!(missing.contains(&"avx2".to_string()) && !vendor_mismatch);
            }
            v => panic!("{v:?}"),
        }
        let new = with(&[
            V2,
            WESTMERE_EXTRA,
            SANDY_EXTRA,
            IVY_EXTRA,
            HASWELL_EXTRA,
            BROADWELL_EXTRA,
        ]);
        assert_eq!(check(&req, Some(&new)), CpuVerdict::Compatible);
        assert_eq!(check(&req, None), CpuVerdict::Unknown);
    }

    #[test]
    fn features_add_remove_and_unknown_are_conservative() {
        let mut c = cfg("custom", "qemu64");
        c.features_add = vec!["AVX2".into(), "sse4.1".into(), "made-up-feature".into()];
        c.features_remove = vec!["sse4.1".into()];
        let r = requirement(&c, None, true);
        assert!(r.features.contains("avx2") && !r.features.contains("sse4_1"));
        assert!(r.notes.iter().any(|n| n.contains("made-up-feature")));
        // unknown model: no requirement, so never a blocker
        let r = requirement(&cfg("custom", "Mystery"), None, true);
        assert!(r.is_empty() && !r.notes.is_empty());
        assert_eq!(check(&r, Some(&cpu("x", &["fpu"]))), CpuVerdict::Compatible);
    }

    #[test]
    fn vendor_mismatch_blocks() {
        let mut c = cfg("custom", "qemu64");
        c.vendor = Some("AMD".into());
        let r = requirement(&c, None, true);
        match check(&r, Some(&cpu("GenuineIntel", &["fpu"]))) {
            CpuVerdict::Incompatible {
                vendor_mismatch, ..
            } => assert!(vendor_mismatch),
            v => panic!("{v:?}"),
        }
        assert_eq!(
            check(&r, Some(&cpu("AuthenticAMD", &["fpu"]))),
            CpuVerdict::Compatible
        );
    }

    #[test]
    fn host_model_needs_source_superset_only_when_running() {
        let src = with(&[V2, V3_EXTRA, &["spec_ctrl", "md_clear"]]);
        let run = requirement(&cfg("host-passthrough", "x"), Some(&src), true);
        // mitigation flags are not compared
        assert!(!run.features.contains("md_clear"));
        let target_lacks = with(&[V2]);
        assert!(matches!(
            check(&run, Some(&target_lacks)),
            CpuVerdict::Incompatible { .. }
        ));
        let target_ok = with(&[V2, V3_EXTRA]);
        assert_eq!(check(&run, Some(&target_ok)), CpuVerdict::Compatible);
        let cold = requirement(&cfg("host-model", "x"), Some(&src), false);
        assert!(cold.is_empty());
        let nosrc = requirement(&cfg("host-model", "x"), None, true);
        assert!(nosrc.is_empty() && !nosrc.notes.is_empty());
    }

    #[test]
    fn host_model_feature_removal_applies_but_passthrough_ignores_it() {
        let src = with(&[V2, V3_EXTRA]);
        let mut c = cfg("host-model", "x");
        c.features_remove = vec!["avx2".into()];
        assert!(!requirement(&c, Some(&src), true).features.contains("avx2"));
        c.mode = "host-passthrough".into();
        assert!(requirement(&c, Some(&src), true).features.contains("avx2"));
    }

    #[test]
    fn baseline_is_best_common_model() {
        let a = with(&[V2, V3_EXTRA, V4_EXTRA]);
        let b = with(&[V2, V3_EXTRA]);
        let (m, common) = baseline(&[&a, &b]);
        assert_eq!(m, Some("x86-64-v3"));
        assert!(common >= V2.len());
        let weak = cpu("GenuineIntel", &["fpu"]);
        assert_eq!(baseline(&[&a, &weak]).0, Some("qemu64"));
        assert_eq!(baseline(&[]), (None, 0));
    }

    #[test]
    fn describe_caps_the_list() {
        let m: Vec<String> = (0..10).map(|i| format!("f{i}")).collect();
        let d = describe_incompatible(&m, true);
        assert!(d.contains("vendor") && d.contains("(+2 more)"));
    }

    #[test]
    fn unknown_mode_is_not_checked() {
        let r = requirement(&cfg("weird", "x"), None, true);
        assert!(r.is_empty());
    }
}
