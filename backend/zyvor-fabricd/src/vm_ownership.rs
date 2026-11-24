// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: LicenseRef-Zyvor-Production-2.0

//! VM ownership, notes and lease expiry.
//!
//! Mature platforms let operators record who owns a VM and why it exists
//! (vCenter custom attributes and notes, the Proxmox notes field). Fabric had
//! only free-form tags, which are short and have no owner or expiry meaning.
//! This stores a small structured record per VM and reports the VMs nobody
//! owns or whose lease has run out.
//!
//! * `GET /api/vms/{name}/ownership`: the record (empty fields when none).
//! * `PUT /api/vms/{name}/ownership`: replace it (write role).
//! * `DELETE /api/vms/{name}/ownership`: clear it (write role).
//! * `POST /api/vms/{name}/ownership/extend`: move `expires_at` forward by
//!   `days` from the later of now and the current expiry (write role).
//! * `GET /api/ownership/report?within_days=14`: per-VM status `owned`,
//!   `expiring`, `expired` or `unowned`.
//!
//! Rules
//! * A VM is `unowned` when it has no owner (an expiry alone does not make
//!   it owned). `expired` means `expires_at` is in the past, whether or not
//!   there is an owner; `expiring` means expiry falls within `within_days`.
//!   Precedence: expired, then expiring, then unowned, then owned.
//! * Nothing is stopped or deleted when a lease expires. Fabric only reports;
//!   the reclaim report uses these records to rank candidates.
//! * Text is bounded (owner 128, contact 256, purpose 256, notes 4096 bytes)
//!   and control characters are rejected (notes may contain newlines/tabs).
//! * `expires_at` must be RFC 3339 and at most 10 years ahead.
//! * Tenant-scoped tokens see and edit only their tenant's VMs.
//! * Deleting a VM removes its record.

use std::sync::Arc;

use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};

use crate::api_error::json_error;
use crate::server::AppState;
use security::{RequireRead, RequireWrite};

pub const STORE_KEY: &str = "vm_ownership";
const MAX_OWNER: usize = 128;
const MAX_CONTACT: usize = 256;
const MAX_PURPOSE: usize = 256;
const MAX_NOTES: usize = 4096;
const MAX_EXTEND_DAYS: i64 = 365;
const MAX_AHEAD_DAYS: i64 = 3650;
pub const DEFAULT_WITHIN_DAYS: i64 = 14;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct Ownership {
    pub vm_name: String,
    #[serde(default)]
    pub owner: Option<String>,
    #[serde(default)]
    pub contact: Option<String>,
    #[serde(default)]
    pub purpose: Option<String>,
    #[serde(default)]
    pub notes: Option<String>,
    #[serde(default)]
    pub expires_at: Option<DateTime<Utc>>,
    #[serde(default)]
    pub updated_at: Option<DateTime<Utc>>,
    #[serde(default)]
    pub updated_by: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct OwnershipInput {
    #[serde(default)]
    pub owner: Option<String>,
    #[serde(default)]
    pub contact: Option<String>,
    #[serde(default)]
    pub purpose: Option<String>,
    #[serde(default)]
    pub notes: Option<String>,
    #[serde(default)]
    pub expires_at: Option<DateTime<Utc>>,
}

fn clean(
    field: &str,
    v: Option<String>,
    max: usize,
    multiline: bool,
) -> Result<Option<String>, String> {
    let Some(v) = v else { return Ok(None) };
    let v = v.trim().to_string();
    if v.is_empty() {
        return Ok(None);
    }
    if v.len() > max {
        return Err(format!("{field} must be at most {max} bytes"));
    }
    let bad = v
        .chars()
        .any(|c| c.is_control() && !(multiline && matches!(c, '\n' | '\t' | '\r')));
    if bad {
        return Err(format!("{field} must not contain control characters"));
    }
    Ok(Some(v))
}

/// Validate an input against `now` and build the stored record.
pub fn build(
    vm: &str,
    input: OwnershipInput,
    by: &str,
    now: DateTime<Utc>,
) -> Result<Ownership, String> {
    if let Some(e) = input.expires_at {
        if e > now + Duration::days(MAX_AHEAD_DAYS) {
            return Err("expires_at must be within 10 years".to_string());
        }
    }
    Ok(Ownership {
        vm_name: vm.to_string(),
        owner: clean("owner", input.owner, MAX_OWNER, false)?,
        contact: clean("contact", input.contact, MAX_CONTACT, false)?,
        purpose: clean("purpose", input.purpose, MAX_PURPOSE, false)?,
        notes: clean("notes", input.notes, MAX_NOTES, true)?,
        expires_at: input.expires_at,
        updated_at: Some(now),
        updated_by: Some(by.to_string()),
    })
}

/// `expired`, `expiring`, `unowned` or `owned`.
pub fn status_of(o: Option<&Ownership>, now: DateTime<Utc>, within_days: i64) -> &'static str {
    if let Some(e) = o.and_then(|o| o.expires_at) {
        if e <= now {
            return "expired";
        }
        if e <= now + Duration::days(within_days) {
            return "expiring";
        }
    }
    match o.and_then(|o| o.owner.as_deref()) {
        Some(_) => "owned",
        None => "unowned",
    }
}

fn rank(status: &str) -> u8 {
    match status {
        "expired" => 0,
        "expiring" => 1,
        "unowned" => 2,
        _ => 3,
    }
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct VmOwnershipRow {
    pub vm: String,
    pub status: &'static str,
    pub state: String,
    pub owner: Option<String>,
    pub contact: Option<String>,
    pub purpose: Option<String>,
    pub expires_at: Option<DateTime<Utc>>,
    /// Negative when already expired.
    pub days_to_expiry: Option<i64>,
}

pub fn row(
    vm: &str,
    state: &str,
    o: Option<&Ownership>,
    now: DateTime<Utc>,
    within_days: i64,
) -> VmOwnershipRow {
    VmOwnershipRow {
        vm: vm.to_string(),
        status: status_of(o, now, within_days),
        state: state.to_string(),
        owner: o.and_then(|o| o.owner.clone()),
        contact: o.and_then(|o| o.contact.clone()),
        purpose: o.and_then(|o| o.purpose.clone()),
        expires_at: o.and_then(|o| o.expires_at),
        days_to_expiry: o
            .and_then(|o| o.expires_at)
            .map(|e| (e - now).num_seconds().div_euclid(86_400)),
    }
}

#[derive(Debug, Serialize, Default, PartialEq)]
pub struct OwnershipSummary {
    pub vms: usize,
    pub owned: usize,
    pub expiring: usize,
    pub expired: usize,
    pub unowned: usize,
}

pub fn summarize(rows: &[VmOwnershipRow]) -> OwnershipSummary {
    let mut s = OwnershipSummary {
        vms: rows.len(),
        ..Default::default()
    };
    for r in rows {
        match r.status {
            "expired" => s.expired += 1,
            "expiring" => s.expiring += 1,
            "unowned" => s.unowned += 1,
            _ => s.owned += 1,
        }
    }
    s
}

pub fn sort_rows(rows: &mut [VmOwnershipRow]) {
    rows.sort_by(|a, b| {
        rank(a.status)
            .cmp(&rank(b.status))
            .then(a.expires_at.cmp(&b.expires_at))
            .then_with(|| a.vm.cmp(&b.vm))
    });
}

/// 404 when the VM is missing (also for another tenant VM, so names do not
/// leak), 400 for an invalid name.
fn check_vm(
    state: &AppState,
    name: &str,
    tenant: Option<&str>,
) -> Result<(), (StatusCode, String)> {
    crate::validation::validate_vm_name(name)?;
    match state.store.get_vm(name) {
        Ok(Some(vm)) => match tenant {
            Some(t) if crate::tenant_scope::vm_tenant(&vm).as_deref() != Some(t) => {
                Err((StatusCode::NOT_FOUND, "VM not found".to_string()))
            }
            _ => Ok(()),
        },
        Ok(None) => Err((StatusCode::NOT_FOUND, "VM not found".to_string())),
        Err(e) => Err((StatusCode::INTERNAL_SERVER_ERROR, e.to_string())),
    }
}

fn load(state: &AppState, name: &str) -> Option<Ownership> {
    state
        .store
        .get_entity::<Ownership>(STORE_KEY, name)
        .ok()
        .flatten()
}

/// GET /api/vms/{name}/ownership
pub async fn get_ownership(
    RequireRead(claims): RequireRead,
    State(state): State<Arc<AppState>>,
    Path(name): Path<String>,
) -> impl IntoResponse {
    if let Err((s, m)) = check_vm(&state, &name, claims.tenant.as_deref()) {
        return json_error(s, m).into_response();
    }
    let o = load(&state, &name).unwrap_or(Ownership {
        vm_name: name,
        ..Default::default()
    });
    Json(o).into_response()
}

/// PUT /api/vms/{name}/ownership
pub async fn put_ownership(
    RequireWrite(claims): RequireWrite,
    State(state): State<Arc<AppState>>,
    Path(name): Path<String>,
    Json(input): Json<OwnershipInput>,
) -> impl IntoResponse {
    if let Err((s, m)) = check_vm(&state, &name, claims.tenant.as_deref()) {
        return json_error(s, m).into_response();
    }
    let rec = match build(&name, input, &claims.sub, Utc::now()) {
        Ok(r) => r,
        Err(e) => return json_error(StatusCode::BAD_REQUEST, e).into_response(),
    };
    if let Err(e) = state.store.save_entity(STORE_KEY, &name, &rec) {
        return json_error(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response();
    }
    crate::routes::audit(
        &state,
        &claims.sub,
        "VM_OWNERSHIP_SET",
        &format!("vm/{name}"),
        "SUCCESS",
    );
    Json(rec).into_response()
}

/// DELETE /api/vms/{name}/ownership
pub async fn delete_ownership(
    RequireWrite(claims): RequireWrite,
    State(state): State<Arc<AppState>>,
    Path(name): Path<String>,
) -> impl IntoResponse {
    if let Err((s, m)) = check_vm(&state, &name, claims.tenant.as_deref()) {
        return json_error(s, m).into_response();
    }
    if load(&state, &name).is_none() {
        return StatusCode::NO_CONTENT.into_response();
    }
    if let Err(e) = state.store.delete_entity(STORE_KEY, &name) {
        return json_error(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response();
    }
    crate::routes::audit(
        &state,
        &claims.sub,
        "VM_OWNERSHIP_DELETE",
        &format!("vm/{name}"),
        "SUCCESS",
    );
    StatusCode::NO_CONTENT.into_response()
}

#[derive(Debug, Deserialize)]
pub struct ExtendRequest {
    pub days: i64,
}

/// New expiry after extending by `days` (1..=365) from the later of now and
/// the current expiry.
pub fn extended(
    current: Option<DateTime<Utc>>,
    days: i64,
    now: DateTime<Utc>,
) -> Result<DateTime<Utc>, String> {
    if !(1..=MAX_EXTEND_DAYS).contains(&days) {
        return Err(format!("days must be 1..={MAX_EXTEND_DAYS}"));
    }
    let base = current.map_or(now, |c| c.max(now));
    let new = base + Duration::days(days);
    if new > now + Duration::days(MAX_AHEAD_DAYS) {
        return Err("expires_at must be within 10 years".to_string());
    }
    Ok(new)
}

/// POST /api/vms/{name}/ownership/extend
pub async fn extend_ownership(
    RequireWrite(claims): RequireWrite,
    State(state): State<Arc<AppState>>,
    Path(name): Path<String>,
    Json(req): Json<ExtendRequest>,
) -> impl IntoResponse {
    if let Err((s, m)) = check_vm(&state, &name, claims.tenant.as_deref()) {
        return json_error(s, m).into_response();
    }
    let now = Utc::now();
    let mut rec = load(&state, &name).unwrap_or(Ownership {
        vm_name: name.clone(),
        ..Default::default()
    });
    rec.expires_at = match extended(rec.expires_at, req.days, now) {
        Ok(e) => Some(e),
        Err(e) => return json_error(StatusCode::BAD_REQUEST, e).into_response(),
    };
    rec.updated_at = Some(now);
    rec.updated_by = Some(claims.sub.clone());
    if let Err(e) = state.store.save_entity(STORE_KEY, &name, &rec) {
        return json_error(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response();
    }
    crate::routes::audit(
        &state,
        &claims.sub,
        "VM_OWNERSHIP_EXTEND",
        &format!("vm/{name}"),
        "SUCCESS",
    );
    Json(rec).into_response()
}

#[derive(Debug, Deserialize)]
pub struct ReportQuery {
    #[serde(default)]
    pub within_days: Option<i64>,
}

#[derive(Debug, Serialize)]
pub struct OwnershipReport {
    pub generated_at: DateTime<Utc>,
    pub within_days: i64,
    pub summary: OwnershipSummary,
    pub vms: Vec<VmOwnershipRow>,
}

pub fn validate_within(v: Option<i64>) -> Result<i64, String> {
    let d = v.unwrap_or(DEFAULT_WITHIN_DAYS);
    if !(1..=365).contains(&d) {
        return Err("within_days must be 1..=365".to_string());
    }
    Ok(d)
}

/// Rows for every VM the caller may see.
pub fn collect(
    state: &AppState,
    tenant: Option<&str>,
    now: DateTime<Utc>,
    within_days: i64,
) -> Result<Vec<VmOwnershipRow>, String> {
    let vms = state.store.list_vms().map_err(|e| e.to_string())?;
    let mut rows = Vec::new();
    for vm in vms {
        if let Some(t) = tenant {
            if crate::tenant_scope::vm_tenant(&vm).as_deref() != Some(t) {
                continue;
            }
        }
        let o = load(state, &vm.name);
        rows.push(row(
            &vm.name,
            &format!("{:?}", vm.state),
            o.as_ref(),
            now,
            within_days,
        ));
    }
    sort_rows(&mut rows);
    Ok(rows)
}

/// GET /api/ownership/report
pub async fn ownership_report(
    RequireRead(claims): RequireRead,
    State(state): State<Arc<AppState>>,
    Query(q): Query<ReportQuery>,
) -> impl IntoResponse {
    let within = match validate_within(q.within_days) {
        Ok(d) => d,
        Err(e) => return json_error(StatusCode::BAD_REQUEST, e).into_response(),
    };
    let now = Utc::now();
    match collect(&state, claims.tenant.as_deref(), now, within) {
        Ok(rows) => Json(OwnershipReport {
            generated_at: now,
            within_days: within,
            summary: summarize(&rows),
            vms: rows,
        })
        .into_response(),
        Err(e) => json_error(StatusCode::INTERNAL_SERVER_ERROR, e).into_response(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    fn now() -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 6, 1, 12, 0, 0).unwrap()
    }

    fn input(owner: Option<&str>, exp: Option<DateTime<Utc>>) -> OwnershipInput {
        OwnershipInput {
            owner: owner.map(String::from),
            contact: None,
            purpose: None,
            notes: None,
            expires_at: exp,
        }
    }

    #[test]
    fn build_trims_and_drops_empty() {
        let mut i = input(Some("  alice  "), None);
        i.contact = Some("   ".into());
        let r = build("vm", i, "admin", now()).unwrap();
        assert_eq!(r.owner.as_deref(), Some("alice"));
        assert_eq!(r.contact, None);
        assert_eq!(r.updated_by.as_deref(), Some("admin"));
    }

    #[test]
    fn build_rejects_bad_text_and_dates() {
        assert!(build("vm", input(Some("a\u{7}b"), None), "a", now()).is_err());
        assert!(build("vm", input(Some(&"x".repeat(129)), None), "a", now()).is_err());
        let mut i = input(None, None);
        i.notes = Some("line1\nline2\tok".into());
        assert!(build("vm", i, "a", now()).is_ok());
        let mut i = input(None, None);
        i.notes = Some("x".repeat(4097));
        assert!(build("vm", i, "a", now()).is_err());
        let far = now() + Duration::days(4000);
        assert!(build("vm", input(None, Some(far)), "a", now()).is_err());
    }

    #[test]
    fn status_precedence() {
        let mk = |owner: Option<&str>, exp: Option<DateTime<Utc>>| {
            build("vm", input(owner, exp), "a", now()).unwrap()
        };
        assert_eq!(status_of(None, now(), 14), "unowned");
        assert_eq!(status_of(Some(&mk(Some("a"), None)), now(), 14), "owned");
        assert_eq!(status_of(Some(&mk(None, None)), now(), 14), "unowned");
        let soon = Some(now() + Duration::days(3));
        assert_eq!(status_of(Some(&mk(Some("a"), soon)), now(), 14), "expiring");
        assert_eq!(status_of(Some(&mk(None, soon)), now(), 14), "expiring");
        let past = Some(now() - Duration::days(1));
        assert_eq!(status_of(Some(&mk(Some("a"), past)), now(), 14), "expired");
        let far = Some(now() + Duration::days(60));
        assert_eq!(status_of(Some(&mk(Some("a"), far)), now(), 14), "owned");
        // Expiry exactly now counts as expired.
        assert_eq!(
            status_of(Some(&mk(Some("a"), Some(now()))), now(), 14),
            "expired"
        );
    }

    #[test]
    fn extend_rules() {
        assert!(extended(None, 0, now()).is_err());
        assert!(extended(None, 366, now()).is_err());
        assert_eq!(extended(None, 7, now()).unwrap(), now() + Duration::days(7));
        // Expired lease extends from now, a live one from its expiry.
        assert_eq!(
            extended(Some(now() - Duration::days(30)), 7, now()).unwrap(),
            now() + Duration::days(7)
        );
        assert_eq!(
            extended(Some(now() + Duration::days(10)), 7, now()).unwrap(),
            now() + Duration::days(17)
        );
    }

    #[test]
    fn rows_sort_and_summarize() {
        let a = build("a", input(Some("o"), None), "x", now()).unwrap();
        let b = build(
            "b",
            input(Some("o"), Some(now() - Duration::days(2))),
            "x",
            now(),
        )
        .unwrap();
        let c = build(
            "c",
            input(Some("o"), Some(now() + Duration::days(2))),
            "x",
            now(),
        )
        .unwrap();
        let mut rows = vec![
            row("a", "Running", Some(&a), now(), 14),
            row("d", "Stopped", None, now(), 14),
            row("b", "Stopped", Some(&b), now(), 14),
            row("c", "Running", Some(&c), now(), 14),
        ];
        sort_rows(&mut rows);
        assert_eq!(
            rows.iter().map(|r| r.vm.as_str()).collect::<Vec<_>>(),
            ["b", "c", "d", "a"]
        );
        assert_eq!(rows[0].days_to_expiry, Some(-2));
        let s = summarize(&rows);
        assert_eq!((s.expired, s.expiring, s.unowned, s.owned), (1, 1, 1, 1));
    }

    #[test]
    fn within_validation() {
        assert_eq!(validate_within(None).unwrap(), 14);
        assert!(validate_within(Some(0)).is_err());
        assert!(validate_within(Some(366)).is_err());
    }
}
