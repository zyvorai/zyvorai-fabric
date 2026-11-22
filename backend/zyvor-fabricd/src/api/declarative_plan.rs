// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: LicenseRef-Zyvor-Production-2.0

//! Terraform-style plan for the declarative VM apply path.
//!
//! [`plan`] is pure: it compares the VMs that currently exist (as exported
//! specs) with a desired set and says what an apply would do, without
//! touching any state. The comparison mirrors what `apply_vm_spec` really
//! reconciles for an existing VM, so the plan never promises a change the
//! apply would not make. Differences in creation-only fields (image, cpus,
//! memory, disk, hostname, tags) are still listed, marked `applied: false`,
//! so drift is visible but does not turn a noop into an update.
//!
//! Secret-looking values are redacted in everything the plan returns. The
//! `plan_hash` additionally covers a digest of the unredacted values, so a
//! changed secret changes the hash without being disclosed.

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;

use super::declarative::{parse_disk_gb, parse_memory_mb, VMSpec};

pub const REDACTED: &str = "[REDACTED]";

/// What the caller wants: a set of VM specs, and whether VMs missing from the
/// set should be deleted (`prune`) or only reported as unmanaged.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct DesiredSpec {
    #[serde(default)]
    pub vms: Vec<VMSpec>,
    #[serde(default)]
    pub prune: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Action {
    Create,
    Update,
    Delete,
    Noop,
    /// Exists but is not in the desired set and pruning was not requested.
    Unmanaged,
}

impl Action {
    fn rank(self) -> u8 {
        match self {
            Action::Create => 0,
            Action::Update => 1,
            Action::Delete => 2,
            Action::Noop => 3,
            Action::Unmanaged => 4,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct FieldChange {
    pub field: String,
    /// Redacted previous value (`null` for a create).
    pub before: Value,
    /// Redacted desired value (`null` for a delete).
    pub after: Value,
    /// False when an apply would not change this field on an existing VM.
    pub applied: bool,
    #[serde(skip)]
    raw_digest: String,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct PlanEntry {
    pub name: String,
    pub action: Action,
    pub changes: Vec<FieldChange>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct PlanSummary {
    pub create: usize,
    pub update: usize,
    pub delete: usize,
    pub noop: usize,
    pub unmanaged: usize,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Plan {
    pub entries: Vec<PlanEntry>,
    pub summary: PlanSummary,
    /// True when an apply would create, update or delete anything.
    pub has_changes: bool,
    /// SHA-256 (hex) over the canonical plan.
    pub plan_hash: String,
}

// ----------------------------------------------------------------------------
// Redaction
// ----------------------------------------------------------------------------

fn is_secret_key(key: &str) -> bool {
    let k = key.to_ascii_lowercase();
    [
        "password",
        "passwd",
        "secret",
        "token",
        "api_key",
        "apikey",
        "private_key",
        "passphrase",
    ]
    .iter()
    .any(|p| k.contains(p))
}

fn looks_secret(s: &str) -> bool {
    s.starts_with("-----BEGIN")
        || s.starts_with("sk-")
        || s.starts_with("ghp_")
        || s.starts_with("github_pat_")
        || s.starts_with("xoxb-")
        || s.starts_with("AKIA")
        || (s.starts_with("eyJ") && s.matches('.').count() == 2)
}

/// Redact secret-looking content: values under secret-named keys, the
/// `value` of credential entries, and token-shaped strings.
pub fn redact(v: &Value) -> Value {
    redact_inner(v, false)
}

fn redact_inner(v: &Value, in_cred: bool) -> Value {
    match v {
        Value::Object(m) => Value::Object(
            m.iter()
                .map(|(k, val)| {
                    let hide = !val.is_null() && (is_secret_key(k) || (in_cred && k == "value"));
                    let nv = if hide {
                        json!(REDACTED)
                    } else {
                        redact_inner(
                            val,
                            in_cred || k.to_ascii_lowercase().contains("credential"),
                        )
                    };
                    (k.clone(), nv)
                })
                .collect(),
        ),
        Value::Array(a) => Value::Array(a.iter().map(|x| redact_inner(x, in_cred)).collect()),
        Value::String(s) if looks_secret(s) => json!(REDACTED),
        other => other.clone(),
    }
}

// ----------------------------------------------------------------------------
// Field extraction
// ----------------------------------------------------------------------------

/// Normalised, comparable fields of a spec as (name, value).
fn fields(spec: &VMSpec) -> Vec<(&'static str, Value)> {
    let so = &spec.start_options;
    let uplink = spec
        .direct_uplink
        .clone()
        .or_else(|| so.direct_uplink.clone());
    let mode = spec.direct_mode.clone().or_else(|| so.direct_mode.clone());
    let guest_ips = if spec.direct_guest_ips.is_empty() {
        so.direct_guest_ips.clone()
    } else {
        spec.direct_guest_ips.clone()
    };
    let mem = parse_memory_mb(&spec.resources.memory)
        .map(|m| json!(m))
        .unwrap_or_else(|_| json!(spec.resources.memory));
    let disk = parse_disk_gb(&spec.resources.disk)
        .map(|d| json!(d))
        .unwrap_or_else(|_| json!(spec.resources.disk));
    let mut tags = spec.tags.clone();
    tags.sort();
    vec![
        ("image", json!(spec.image)),
        ("cpus", json!(spec.resources.cpus)),
        ("memory_mb", mem),
        ("disk_gb", disk),
        ("hostname", json!(spec.hostname)),
        ("tags", json!(tags)),
        ("direct_uplink", json!(uplink)),
        ("direct_mode", json!(mode)),
        ("direct_guest_ips", json!(guest_ips)),
        ("volumes", json!(spec.volumes)),
        ("autoscale", json!(spec.autoscale)),
        ("start_options", json!(so)),
    ]
}

/// Whether `apply_vm_spec` reconciles `field` on an existing VM.
fn is_applied(field: &str, desired: &VMSpec) -> bool {
    match field {
        "direct_uplink" | "direct_mode" | "direct_guest_ips" => {
            desired.direct_uplink.is_some() || desired.start_options.direct_uplink.is_some()
        }
        // apply only writes volumes / autoscale when the spec carries them
        "volumes" => !desired.volumes.is_empty(),
        "autoscale" => desired.autoscale.is_some(),
        "start_options" => true,
        _ => false,
    }
}

fn digest(before: &Value, after: &Value) -> String {
    let mut h = Sha256::new();
    h.update(serde_json::to_vec(&json!([before, after])).unwrap_or_default());
    h.finalize().iter().map(|b| format!("{b:02x}")).collect()
}

fn change(field: &str, before: &Value, after: &Value, applied: bool) -> FieldChange {
    FieldChange {
        field: field.to_string(),
        before: redact(before),
        after: redact(after),
        applied,
        raw_digest: digest(before, after),
    }
}

// ----------------------------------------------------------------------------
// plan()
// ----------------------------------------------------------------------------

/// Compute the plan to move `current` to `desired`. Pure and deterministic:
/// entries are ordered create, update, delete, noop, unmanaged, then by name.
/// If `desired.vms` repeats a name the last occurrence wins.
pub fn plan(current: &[VMSpec], desired: &DesiredSpec) -> Plan {
    let cur: BTreeMap<&str, &VMSpec> = current.iter().map(|s| (s.name.as_str(), s)).collect();
    let des: BTreeMap<&str, &VMSpec> = desired.vms.iter().map(|s| (s.name.as_str(), s)).collect();
    let mut entries = Vec::new();

    for (name, d) in &des {
        match cur.get(name) {
            None => {
                let changes = fields(d)
                    .iter()
                    .map(|(f, v)| change(f, &Value::Null, v, true))
                    .collect();
                entries.push(PlanEntry {
                    name: (*name).into(),
                    action: Action::Create,
                    changes,
                });
            }
            Some(c) => {
                let before: BTreeMap<_, _> = fields(c).into_iter().collect();
                let mut changes = Vec::new();
                for (f, after) in fields(d) {
                    let b = before.get(f).cloned().unwrap_or(Value::Null);
                    if b != after {
                        changes.push(change(f, &b, &after, is_applied(f, d)));
                    }
                }
                let action = if changes.iter().any(|c| c.applied) {
                    Action::Update
                } else {
                    Action::Noop
                };
                entries.push(PlanEntry {
                    name: (*name).into(),
                    action,
                    changes,
                });
            }
        }
    }
    for (name, c) in &cur {
        if des.contains_key(name) {
            continue;
        }
        if desired.prune {
            let changes = fields(c)
                .iter()
                .map(|(f, v)| change(f, v, &Value::Null, true))
                .collect();
            entries.push(PlanEntry {
                name: (*name).into(),
                action: Action::Delete,
                changes,
            });
        } else {
            entries.push(PlanEntry {
                name: (*name).into(),
                action: Action::Unmanaged,
                changes: vec![],
            });
        }
    }

    entries.sort_by(|a, b| (a.action.rank(), &a.name).cmp(&(b.action.rank(), &b.name)));

    let mut summary = PlanSummary::default();
    for e in &entries {
        match e.action {
            Action::Create => summary.create += 1,
            Action::Update => summary.update += 1,
            Action::Delete => summary.delete += 1,
            Action::Noop => summary.noop += 1,
            Action::Unmanaged => summary.unmanaged += 1,
        }
    }
    let has_changes = summary.create + summary.update + summary.delete > 0;
    let plan_hash = hash_plan(&entries, &summary);
    Plan {
        entries,
        summary,
        has_changes,
        plan_hash,
    }
}

fn hash_plan(entries: &[PlanEntry], summary: &PlanSummary) -> String {
    let canon = json!({
        "entries": entries.iter().map(|e| json!({
            "name": e.name,
            "action": e.action,
            "changes": e.changes.iter().map(|c| json!({
                "field": c.field, "before": c.before, "after": c.after,
                "applied": c.applied, "raw": c.raw_digest,
            })).collect::<Vec<_>>(),
        })).collect::<Vec<_>>(),
        "summary": summary,
    });
    // serde_json maps are BTreeMap-backed (no preserve_order), so key order is canonical.
    let mut h = Sha256::new();
    h.update(serde_json::to_vec(&canon).unwrap_or_default());
    h.finalize().iter().map(|b| format!("{b:02x}")).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::declarative::{AutoScaleSpec, ResourceSpec, StartOptionsSpec, VolumeMount};

    fn spec(name: &str) -> VMSpec {
        VMSpec {
            name: name.into(),
            image: "ubuntu".into(),
            resources: ResourceSpec {
                cpus: 2,
                memory: "2G".into(),
                disk: "20G".into(),
            },
            network: vec![],
            volumes: vec![],
            credentials: vec![],
            start_options: StartOptionsSpec::default(),
            autoscale: None,
            hostname: None,
            tags: vec![],
            direct_uplink: None,
            direct_mode: None,
            direct_guest_ips: vec![],
            auto_start: false,
        }
    }

    fn want(vms: Vec<VMSpec>, prune: bool) -> DesiredSpec {
        DesiredSpec { vms, prune }
    }

    fn vol(host: &str) -> VolumeMount {
        VolumeMount {
            host: host.into(),
            guest: "/mnt".into(),
            mount_type: "virtiofs".into(),
            readonly: false,
        }
    }

    #[test]
    fn empty_is_noop_without_changes() {
        let p = plan(&[], &want(vec![], false));
        assert!(p.entries.is_empty() && !p.has_changes);
    }

    #[test]
    fn create_lists_all_fields() {
        let p = plan(&[], &want(vec![spec("a")], false));
        assert_eq!(p.entries[0].action, Action::Create);
        assert!(p.has_changes && p.summary.create == 1);
        let img = p.entries[0]
            .changes
            .iter()
            .find(|c| c.field == "image")
            .unwrap();
        assert_eq!(img.before, Value::Null);
        assert_eq!(img.after, json!("ubuntu"));
    }

    #[test]
    fn identical_is_noop() {
        let p = plan(&[spec("a")], &want(vec![spec("a")], false));
        assert_eq!(p.entries[0].action, Action::Noop);
        assert!(!p.has_changes);
    }

    #[test]
    fn memory_units_are_normalised() {
        let mut d = spec("a");
        d.resources.memory = "2048".into();
        let p = plan(&[spec("a")], &want(vec![d], false));
        assert_eq!(p.entries[0].action, Action::Noop);
        assert!(p.entries[0].changes.is_empty());
    }

    #[test]
    fn applied_field_change_is_update_with_before_after() {
        let mut d = spec("a");
        d.volumes = vec![vol("/srv/x")];
        let mut c = spec("a");
        c.volumes = vec![vol("/srv/old")];
        let p = plan(&[c], &want(vec![d], false));
        assert_eq!(p.entries[0].action, Action::Update);
        let ch = &p.entries[0].changes[0];
        assert_eq!(ch.field, "volumes");
        assert!(ch.applied);
        assert_eq!(ch.before[0]["host"], "/srv/old");
        assert_eq!(ch.after[0]["host"], "/srv/x");
    }

    #[test]
    fn creation_only_drift_is_noop_but_listed() {
        let mut d = spec("a");
        d.resources.cpus = 8;
        d.image = "debian".into();
        let p = plan(&[spec("a")], &want(vec![d], false));
        assert_eq!(p.entries[0].action, Action::Noop);
        assert!(!p.has_changes);
        assert_eq!(p.entries[0].changes.len(), 2);
        assert!(p.entries[0].changes.iter().all(|c| !c.applied));
    }

    #[test]
    fn empty_volumes_and_missing_autoscale_do_not_clear() {
        let mut c = spec("a");
        c.volumes = vec![vol("/srv/x")];
        c.autoscale = Some(AutoScaleSpec {
            cpu_threshold: Some(0.8),
            memory_threshold: None,
            max_cpus: None,
            max_memory_mb: None,
            min_cpus: None,
            min_memory_mb: None,
            cooldown_secs: 300,
        });
        let p = plan(&[c], &want(vec![spec("a")], false));
        assert_eq!(p.entries[0].action, Action::Noop);
        assert!(p.entries[0].changes.iter().all(|c| !c.applied));
    }

    #[test]
    fn direct_uplink_only_applied_when_desired_sets_it() {
        let mut c = spec("a");
        c.direct_uplink = Some("eth0".into());
        let p = plan(&[c.clone()], &want(vec![spec("a")], false));
        assert_eq!(p.entries[0].action, Action::Noop);
        let mut d = spec("a");
        d.direct_uplink = Some("eth1".into());
        let p = plan(&[c], &want(vec![d], false));
        assert_eq!(p.entries[0].action, Action::Update);
    }

    #[test]
    fn start_options_change_is_update() {
        let mut d = spec("a");
        d.start_options.kvm = Some(true);
        let p = plan(&[spec("a")], &want(vec![d], false));
        assert_eq!(p.entries[0].action, Action::Update);
        assert_eq!(p.entries[0].changes[0].field, "start_options");
    }

    #[test]
    fn unmanaged_reported_never_deleted_without_prune() {
        let p = plan(&[spec("a"), spec("b")], &want(vec![spec("a")], false));
        let b = p.entries.iter().find(|e| e.name == "b").unwrap();
        assert_eq!(b.action, Action::Unmanaged);
        assert_eq!(p.summary.unmanaged, 1);
        assert_eq!(p.summary.delete, 0);
        assert!(!p.has_changes);
    }

    #[test]
    fn prune_deletes_missing() {
        let p = plan(&[spec("a"), spec("b")], &want(vec![spec("a")], true));
        let b = p.entries.iter().find(|e| e.name == "b").unwrap();
        assert_eq!(b.action, Action::Delete);
        assert!(p.has_changes && p.summary.delete == 1 && p.summary.unmanaged == 0);
        assert_eq!(b.changes[0].after, Value::Null);
    }

    #[test]
    fn ordering_is_action_then_name_and_input_order_independent() {
        let mut upd = spec("m");
        upd.start_options.kvm = Some(true);
        let cur = vec![spec("z-del"), spec("m"), spec("noop"), spec("u-extra")];
        let d1 = want(
            vec![spec("b-new"), spec("a-new"), upd.clone(), spec("noop")],
            true,
        );
        let d2 = want(vec![spec("noop"), upd, spec("a-new"), spec("b-new")], true);
        let p1 = plan(&cur, &d1);
        let p2 = plan(&cur, &d2);
        let order: Vec<_> = p1
            .entries
            .iter()
            .map(|e| (e.action, e.name.as_str()))
            .collect();
        assert_eq!(
            order,
            vec![
                (Action::Create, "a-new"),
                (Action::Create, "b-new"),
                (Action::Update, "m"),
                (Action::Delete, "u-extra"),
                (Action::Delete, "z-del"),
                (Action::Noop, "noop"),
            ]
        );
        assert_eq!(p1, p2);
        assert_eq!(p1.plan_hash, p2.plan_hash);
    }

    #[test]
    fn hash_is_stable_hex_and_sensitive() {
        let a = plan(&[], &want(vec![spec("a")], false));
        let b = plan(&[], &want(vec![spec("a")], false));
        assert_eq!(a.plan_hash, b.plan_hash);
        assert_eq!(a.plan_hash.len(), 64);
        assert!(a.plan_hash.chars().all(|c| c.is_ascii_hexdigit()));
        let mut s = spec("a");
        s.resources.cpus = 3;
        assert_ne!(a.plan_hash, plan(&[], &want(vec![s], false)).plan_hash);
        // prune flips unmanaged -> delete, so the hash differs
        let cur = [spec("x")];
        assert_ne!(
            plan(&cur, &want(vec![], false)).plan_hash,
            plan(&cur, &want(vec![], true)).plan_hash
        );
    }

    #[test]
    fn secrets_redacted_but_change_still_detected() {
        let mk = |v: &str| {
            let mut s = spec("a");
            s.start_options.set_credentials = vec![vm_model::VMCredential {
                id: "db".into(),
                value: v.into(),
            }];
            s
        };
        let p1 = plan(&[mk("hunter2")], &want(vec![mk("hunter3")], false));
        let out = serde_json::to_string(&p1).unwrap();
        assert!(!out.contains("hunter2") && !out.contains("hunter3"));
        assert!(out.contains(REDACTED));
        assert_eq!(p1.entries[0].action, Action::Update);
        let p2 = plan(&[mk("hunter2")], &want(vec![mk("hunter4")], false));
        assert_ne!(p1.plan_hash, p2.plan_hash, "hash covers unredacted values");
        // create also redacts
        let c = serde_json::to_string(&plan(&[], &want(vec![mk("hunter9")], false))).unwrap();
        assert!(!c.contains("hunter9"));
    }

    #[test]
    fn redact_helper_rules() {
        let v = json!({"password": "p", "nested": {"api_token": "t"}, "ok": "fine",
            "jwt": "eyJhbGci.eyJzdWIi.sig", "k": "-----BEGIN PRIVATE KEY-----x", "none": null,
            "pass_ssh_key": true, "set_credentials": [{"id": "i", "value": "v"}]});
        let r = redact(&v);
        assert_eq!(r["password"], REDACTED);
        assert_eq!(r["nested"]["api_token"], REDACTED);
        assert_eq!(r["ok"], "fine");
        assert_eq!(r["jwt"], REDACTED);
        assert_eq!(r["k"], REDACTED);
        assert_eq!(r["pass_ssh_key"], true);
        assert!(r["none"].is_null());
        assert_eq!(r["set_credentials"][0]["id"], "i");
        assert_eq!(r["set_credentials"][0]["value"], REDACTED);
    }

    #[test]
    fn duplicate_desired_names_last_wins() {
        let mut second = spec("a");
        second.resources.cpus = 9;
        let p = plan(&[], &want(vec![spec("a"), second], false));
        assert_eq!(p.entries.len(), 1);
        let c = p.entries[0]
            .changes
            .iter()
            .find(|c| c.field == "cpus")
            .unwrap();
        assert_eq!(c.after, json!(9));
    }
}
