// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: LicenseRef-Zyvor-Production-2.0

//! Audit trail core: one writer for every audit event, an optional hash
//! chain over the stored events, integrity verification, filtered export
//! rows and age/size retention.
//!
//! Events live one per file in the `audit_logs` collection. When the chain is
//! active each new event carries `seq` (1, 2, 3, ...), `prev_hash` (the
//! previous event's `hash`, or 64 zeros for the first) and `hash`
//! (SHA-256 over the event's own fields, including `seq` and `prev_hash`).
//! The chain head (`seq`, `hash` of the newest event) is kept in
//! `audit_chain/head`. Retention prunes the oldest events and records the
//! last pruned event in `audit_chain/anchor`, so a verifier can still anchor
//! the first remaining event.
//!
//! This is tamper EVIDENCE, not tamper PROOFING: the hashes are unkeyed, so
//! someone who can rewrite the whole store (and the head file) can rebuild a
//! consistent chain. Copy exports off the host to defend against that.

use chrono::{DateTime, Duration, NaiveDate, SecondsFormat, Utc};
use serde::{Deserialize, Deserializer, Serialize};
use serde_json::json;
use sha2::{Digest, Sha256};
use state_store::StateStore;
use std::sync::Mutex;

use crate::config::AuditConfig;
use crate::server::AppState;

pub const LOGS: &str = "audit_logs";
pub const CHAIN: &str = "audit_chain";
const HEAD_ID: &str = "head";
const ANCHOR_ID: &str = "anchor";
/// `prev_hash` of the very first chained event.
pub const GENESIS: &str = "0000000000000000000000000000000000000000000000000000000000000000";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AuditStatus {
    Success,
    Failed,
}

impl AuditStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            AuditStatus::Success => "success",
            AuditStatus::Failed => "failed",
        }
    }
}

/// Older writers stored free-form status strings ("SUCCESS", "FAILED").
fn lenient_status<'de, D: Deserializer<'de>>(d: D) -> Result<AuditStatus, D::Error> {
    let s = String::deserialize(d)?;
    Ok(match s.to_ascii_lowercase().as_str() {
        "failed" | "fail" | "failure" | "error" | "denied" => AuditStatus::Failed,
        _ => AuditStatus::Success,
    })
}

/// One audit event. Also reads the older `security::AuditLog` shape
/// (`user_id`, `resource`) that `routes::audit` used to write, so those
/// events are listed and exported instead of silently skipped.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditLog {
    pub id: String,
    pub timestamp: DateTime<Utc>,
    #[serde(alias = "user_id")]
    pub user: String,
    pub action: String,
    #[serde(default)]
    pub resource_type: String,
    #[serde(default, alias = "resource")]
    pub resource_name: String,
    #[serde(deserialize_with = "lenient_status")]
    pub status: AuditStatus,
    #[serde(default)]
    pub ip_address: Option<String>,
    #[serde(default)]
    pub details: Option<String>,
    #[serde(default)]
    pub error: Option<String>,
    /// Tenant the event belongs to, when the writer knows it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tenant: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub seq: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub prev_hash: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hash: Option<String>,
}

impl AuditLog {
    pub fn new(
        user: &str,
        action: &str,
        resource_type: &str,
        resource_name: &str,
        status: AuditStatus,
    ) -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            timestamp: Utc::now(),
            user: user.to_string(),
            action: action.to_string(),
            resource_type: resource_type.to_string(),
            resource_name: resource_name.to_string(),
            status,
            ip_address: None,
            details: None,
            error: None,
            tenant: None,
            seq: None,
            prev_hash: None,
            hash: None,
        }
    }

    /// SHA-256 over every field except `hash` itself, as a JSON array so the
    /// encoding is unambiguous.
    pub fn compute_hash(&self) -> String {
        let canon = json!([
            "zyvor-audit-v1",
            self.seq,
            self.prev_hash,
            self.id,
            self.timestamp.to_rfc3339_opts(SecondsFormat::Nanos, true),
            self.user,
            self.action,
            self.resource_type,
            self.resource_name,
            self.status.as_str(),
            self.ip_address,
            self.details,
            self.error,
            self.tenant,
        ]);
        Sha256::digest(canon.to_string().as_bytes())
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect()
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ChainPoint {
    pub seq: u64,
    pub hash: String,
}

static APPEND_LOCK: Mutex<()> = Mutex::new(());

fn get_point(store: &StateStore, id: &str) -> anyhow::Result<Option<ChainPoint>> {
    store.get_entity(CHAIN, id)
}

/// Store one event. The chain continues once it has started, whatever the
/// config says, so switching `[audit] chain` off cannot leave an unchained
/// hole in the middle of a chain.
pub fn append(
    store: &StateStore,
    chain_enabled: bool,
    mut log: AuditLog,
) -> anyhow::Result<AuditLog> {
    let _guard = APPEND_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let head = get_point(store, HEAD_ID)?;
    if chain_enabled || head.is_some() {
        log.seq = Some(head.as_ref().map_or(1, |h| h.seq + 1));
        log.prev_hash = Some(
            head.as_ref()
                .map_or_else(|| GENESIS.to_string(), |h| h.hash.clone()),
        );
        log.hash = Some(log.compute_hash());
    }
    store.save_entity(LOGS, &log.id, &log)?;
    if let (Some(seq), Some(hash)) = (log.seq, log.hash.clone()) {
        store.save_entity(CHAIN, HEAD_ID, &ChainPoint { seq, hash })?;
    }
    Ok(log)
}

/// Record an event from application code (replaces the two ad-hoc writers).
/// Failure is logged, never propagated: an audit write must not fail the
/// operation it describes.
pub fn record(state: &AppState, user: &str, action: &str, resource: &str, status: &str) {
    let status = match status.to_ascii_lowercase().as_str() {
        "failed" | "fail" | "failure" | "error" | "denied" => AuditStatus::Failed,
        _ => AuditStatus::Success,
    };
    let resource_type = resource.split('/').next().unwrap_or("").to_string();
    let log = AuditLog::new(user, action, &resource_type, resource, status);
    tracing::info!(
        "AUDIT: {} {} {} {} at {}",
        log.user,
        log.action,
        log.resource_name,
        log.status.as_str(),
        log.timestamp
    );
    if let Err(e) = append(&state.store, state.config.audit.chain, log) {
        tracing::warn!("Failed to persist audit log: {}", e);
    }
}

// ----------------------------------------------------------------------------
// Loading
// ----------------------------------------------------------------------------

pub struct Loaded {
    pub records: Vec<AuditLog>,
    /// Ids (or "?") of files that exist but cannot be read as an event.
    pub malformed: Vec<String>,
}

pub fn load_all(store: &StateStore) -> anyhow::Result<Loaded> {
    let values: Vec<serde_json::Value> = store.list_entities(LOGS)?;
    let mut records = Vec::with_capacity(values.len());
    let mut malformed = store.list_unreadable_entities(LOGS)?;
    for v in values {
        let id = v
            .get("id")
            .and_then(|x| x.as_str())
            .unwrap_or("?")
            .to_string();
        match serde_json::from_value::<AuditLog>(v) {
            Ok(r) => records.push(r),
            Err(_) => malformed.push(id),
        }
    }
    Ok(Loaded { records, malformed })
}

// ----------------------------------------------------------------------------
// Verification
// ----------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize)]
pub struct Break {
    /// `hash_mismatch`, `prev_hash_mismatch`, `sequence_gap`, `duplicate_seq`,
    /// `truncated`, `head_missing`, `head_mismatch`, `anchor_mismatch`,
    /// `unchained_inside_chain`.
    pub reason: &'static str,
    pub seq: Option<u64>,
    pub id: Option<String>,
    pub detail: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct VerifyReport {
    /// `ok`, `broken` or `not_chained` (no chained events exist yet).
    pub status: &'static str,
    pub ok: bool,
    pub chain_enabled: bool,
    pub total: usize,
    pub chained: usize,
    /// Events written before the chain started (or after, with the chain never
    /// enabled) that carry no hash.
    pub legacy_unchained: usize,
    pub malformed: Vec<String>,
    pub head: Option<ChainPoint>,
    pub anchor: Option<ChainPoint>,
    pub first_break: Option<Break>,
    pub checked_at: DateTime<Utc>,
}

pub fn verify(store: &StateStore, chain_enabled: bool) -> anyhow::Result<VerifyReport> {
    let loaded = load_all(store)?;
    let head = get_point(store, HEAD_ID)?;
    let anchor = get_point(store, ANCHOR_ID)?;
    Ok(verify_records(
        loaded,
        head,
        anchor,
        chain_enabled,
        Utc::now(),
    ))
}

pub fn verify_records(
    loaded: Loaded,
    head: Option<ChainPoint>,
    anchor: Option<ChainPoint>,
    chain_enabled: bool,
    now: DateTime<Utc>,
) -> VerifyReport {
    let total = loaded.records.len();
    let (mut chained, unchained): (Vec<&AuditLog>, Vec<&AuditLog>) =
        loaded.records.iter().partition(|r| r.seq.is_some());
    chained.sort_by(|a, b| (a.seq, a.timestamp, &a.id).cmp(&(b.seq, b.timestamp, &b.id)));

    let mut first_break: Option<Break> = None;
    let mk = |reason, r: &AuditLog, detail: String| Break {
        reason,
        seq: r.seq,
        id: Some(r.id.clone()),
        detail,
    };

    let (mut expected_seq, mut expected_prev) = match &anchor {
        Some(a) => (a.seq + 1, a.hash.clone()),
        None => (1, GENESIS.to_string()),
    };
    for r in &chained {
        let seq = r.seq.unwrap_or(0);
        if r.hash.as_deref() != Some(r.compute_hash().as_str()) {
            first_break = Some(mk(
                "hash_mismatch",
                r,
                format!("event {} was modified after it was written", r.id),
            ));
            break;
        }
        if let Some(a) = &anchor {
            if seq <= a.seq {
                // Left over from an interrupted prune; only the anchor itself is checkable.
                if seq == a.seq && r.hash.as_deref() != Some(a.hash.as_str()) {
                    first_break = Some(mk(
                        "anchor_mismatch",
                        r,
                        "event does not match the retention anchor".to_string(),
                    ));
                    break;
                }
                continue;
            }
        }
        if seq < expected_seq {
            first_break = Some(mk(
                "duplicate_seq",
                r,
                format!("sequence {seq} appears more than once"),
            ));
            break;
        }
        if seq > expected_seq {
            first_break = Some(mk(
                "sequence_gap",
                r,
                format!("events {expected_seq}..{} are missing (removed)", seq - 1),
            ));
            break;
        }
        if r.prev_hash.as_deref() != Some(expected_prev.as_str()) {
            first_break = Some(mk(
                "prev_hash_mismatch",
                r,
                "link to the previous event is broken (reordered or replaced)".to_string(),
            ));
            break;
        }
        expected_seq = seq + 1;
        expected_prev = r.hash.clone().unwrap_or_default();
    }

    if first_break.is_none() && (!chained.is_empty() || head.is_some()) {
        match (&head, chained.last()) {
            (None, _) => {
                first_break = Some(Break {
                    reason: "head_missing",
                    seq: None,
                    id: None,
                    detail: "chained events exist but the chain head is missing".to_string(),
                })
            }
            (Some(h), None) => {
                first_break = Some(Break {
                    reason: "truncated",
                    seq: Some(h.seq),
                    id: None,
                    detail: format!("all events up to {} are missing", h.seq),
                })
            }
            (Some(h), Some(last)) => {
                let lseq = last.seq.unwrap_or(0);
                if lseq < h.seq {
                    first_break = Some(Break {
                        reason: "truncated",
                        seq: Some(lseq),
                        id: Some(last.id.clone()),
                        detail: format!(
                            "newest event is {lseq} but the chain head is {} (tail removed)",
                            h.seq
                        ),
                    });
                } else if lseq == h.seq && last.hash.as_deref() != Some(h.hash.as_str()) {
                    first_break = Some(mk(
                        "head_mismatch",
                        last,
                        "newest event does not match the chain head".to_string(),
                    ));
                }
            }
        }
    }

    // Unchained events dated inside the chain's span: a stripped record.
    let legacy_unchained = unchained.len();
    if let (Some(first), Some(last)) = (chained.first(), chained.last()) {
        let (t0, t1) = (first.timestamp, last.timestamp);
        let mut inside: Vec<&&AuditLog> = unchained
            .iter()
            .filter(|r| r.timestamp > t0 && r.timestamp < t1)
            .collect();
        inside.sort_by_key(|r| r.timestamp);
        if first_break.is_none() {
            if let Some(r) = inside.first() {
                first_break = Some(Break {
                    reason: "unchained_inside_chain",
                    seq: None,
                    id: Some(r.id.clone()),
                    detail: "an event without a hash is dated inside the chain (its chain fields were stripped, or it was inserted)".to_string(),
                });
            }
        }
    }

    let chain_used = !chained.is_empty() || head.is_some();
    if first_break.is_none() && chain_used && !loaded.malformed.is_empty() {
        first_break = Some(Break {
            reason: "unreadable_record",
            seq: None,
            id: loaded.malformed.first().cloned(),
            detail: format!("{} stored event(s) cannot be read", loaded.malformed.len()),
        });
    }

    let status = if first_break.is_some() {
        "broken"
    } else if chain_used {
        "ok"
    } else {
        "not_chained"
    };
    VerifyReport {
        status,
        ok: first_break.is_none(),
        chain_enabled,
        total,
        chained: chained.len(),
        legacy_unchained,
        malformed: loaded.malformed.clone(),
        head,
        anchor,
        first_break,
        checked_at: now,
    }
}

// ----------------------------------------------------------------------------
// Export
// ----------------------------------------------------------------------------

/// Parse an RFC 3339 timestamp or a `YYYY-MM-DD` date. A date-only `to` means
/// the end of that day (the start of the next), so `to=2026-03-01` includes
/// all of March 1st. `to` is otherwise exclusive.
pub fn parse_bound(s: &str, is_to: bool) -> Result<DateTime<Utc>, String> {
    if let Ok(t) = DateTime::parse_from_rfc3339(s) {
        return Ok(t.with_timezone(&Utc));
    }
    if let Ok(d) = NaiveDate::parse_from_str(s, "%Y-%m-%d") {
        let d = if is_to { d.succ_opt().unwrap_or(d) } else { d };
        return Ok(d.and_hms_opt(0, 0, 0).unwrap_or_default().and_utc());
    }
    Err(format!(
        "invalid time '{s}': use RFC 3339 (2026-03-01T00:00:00Z) or YYYY-MM-DD"
    ))
}

#[derive(Debug, Default, Clone)]
pub struct ExportFilter {
    pub from: Option<DateTime<Utc>>,
    pub to: Option<DateTime<Utc>>,
    pub actor: Option<String>,
    pub action: Option<String>,
    /// For a tenant-scoped caller: (tenant, subject). They see events tagged
    /// with their tenant and events they performed themselves.
    pub tenant_scope: Option<(String, String)>,
}

impl ExportFilter {
    pub fn matches(&self, log: &AuditLog) -> bool {
        if self.from.is_some_and(|f| log.timestamp < f) {
            return false;
        }
        if self.to.is_some_and(|t| log.timestamp >= t) {
            return false;
        }
        if let Some(a) = &self.actor {
            if !log.user.eq_ignore_ascii_case(a) {
                return false;
            }
        }
        if let Some(a) = &self.action {
            if !log.action.eq_ignore_ascii_case(a) {
                return false;
            }
        }
        if let Some((tenant, sub)) = &self.tenant_scope {
            if log.tenant.as_deref() != Some(tenant.as_str()) && log.user != *sub {
                return false;
            }
        }
        true
    }
}

pub const CSV_HEADER: &str = "id,timestamp,user,action,resource_type,resource_name,status,ip_address,details,error,tenant,seq,prev_hash,hash\n";

pub fn csv_row(log: &AuditLog) -> String {
    use crate::validation::escape_csv_field as esc;
    let opt = |o: &Option<String>| esc(o.as_deref().unwrap_or(""));
    format!(
        "{},{},{},{},{},{},{},{},{},{},{},{},{},{}\n",
        esc(&log.id),
        esc(&log.timestamp.to_rfc3339_opts(SecondsFormat::Nanos, true)),
        esc(&log.user),
        esc(&log.action),
        esc(&log.resource_type),
        esc(&log.resource_name),
        log.status.as_str(),
        opt(&log.ip_address),
        opt(&log.details),
        opt(&log.error),
        opt(&log.tenant),
        log.seq.map(|s| s.to_string()).unwrap_or_default(),
        opt(&log.prev_hash),
        opt(&log.hash),
    )
}

pub fn ndjson_line(log: &AuditLog) -> String {
    let mut s = serde_json::to_string(log).unwrap_or_default();
    s.push('\n');
    s
}

/// Matching events oldest first; `Err(count_seen)` style overflow is reported
/// as `Ok((rows, true))` when more than `cap` events match.
pub fn collect_export(
    store: &StateStore,
    filter: &ExportFilter,
    cap: usize,
) -> anyhow::Result<(Vec<AuditLog>, bool)> {
    let mut rows = store.list_entities_filtered::<AuditLog, _>(
        LOGS,
        |l| filter.matches(l),
        cap.saturating_add(1),
    )?;
    let overflow = rows.len() > cap;
    rows.sort_by(|a, b| (a.timestamp, a.seq, &a.id).cmp(&(b.timestamp, b.seq, &b.id)));
    Ok((rows, overflow))
}

// ----------------------------------------------------------------------------
// Retention
// ----------------------------------------------------------------------------

#[derive(Debug, Clone, Default)]
pub struct RetentionLimits {
    pub max_age_days: Option<u64>,
    pub max_events: Option<u64>,
}

impl RetentionLimits {
    pub fn from_config(c: &AuditConfig) -> Self {
        Self {
            max_age_days: c.retention_max_age_days,
            max_events: c.retention_max_events,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct RetentionReport {
    pub dry_run: bool,
    pub max_age_days: Option<u64>,
    pub max_events: Option<u64>,
    pub total: usize,
    pub pruned: usize,
    pub remaining: usize,
    pub oldest_pruned: Option<DateTime<Utc>>,
    pub newest_pruned: Option<DateTime<Utc>>,
    /// How many of the pruned events were hash-chained.
    pub chained_pruned: usize,
    pub anchor: Option<ChainPoint>,
    /// Id of the `AUDIT_RETENTION` event written before anything was deleted.
    pub event_id: Option<String>,
}

pub struct RetentionPlan {
    pub prune_ids: Vec<String>,
    pub oldest: Option<DateTime<Utc>>,
    pub newest: Option<DateTime<Utc>>,
    pub chained: usize,
    pub anchor: Option<ChainPoint>,
}

/// Decide what to prune: a prefix of the oldest events, by age and/or count.
/// A future-dated event counts as age 0. Chained events are always pruned
/// lowest-seq first so the chain stays contiguous after the anchor.
pub fn plan_retention(
    records: &[AuditLog],
    limits: &RetentionLimits,
    now: DateTime<Utc>,
) -> Result<RetentionPlan, String> {
    if limits.max_age_days.is_none() && limits.max_events.is_none() {
        return Err("no retention limit set: give max_age_days and/or max_events".to_string());
    }
    if limits.max_age_days == Some(0) || limits.max_events == Some(0) {
        return Err("retention limits must be at least 1".to_string());
    }
    let mut sorted: Vec<&AuditLog> = records.iter().collect();
    sorted.sort_by(|a, b| (a.timestamp, a.seq, &a.id).cmp(&(b.timestamp, b.seq, &b.id)));

    let mut k = 0usize;
    if let Some(days) = limits.max_age_days {
        let cutoff = now - Duration::days(days.min(36_500) as i64);
        k = sorted.iter().take_while(|r| r.timestamp < cutoff).count();
    }
    if let Some(max) = limits.max_events {
        k = k.max(sorted.len().saturating_sub(max as usize));
    }
    // Never prune everything.
    k = k.min(sorted.len().saturating_sub(1));

    let selected = &sorted[..k];
    let n_chained = selected.iter().filter(|r| r.seq.is_some()).count();
    let mut prune: Vec<&AuditLog> = selected
        .iter()
        .copied()
        .filter(|r| r.seq.is_none())
        .collect();
    let mut chained_all: Vec<&AuditLog> =
        sorted.iter().copied().filter(|r| r.seq.is_some()).collect();
    chained_all.sort_by_key(|r| r.seq);
    let chained_prune: Vec<&AuditLog> = chained_all.into_iter().take(n_chained).collect();
    let anchor = chained_prune.last().and_then(|r| {
        Some(ChainPoint {
            seq: r.seq?,
            hash: r.hash.clone()?,
        })
    });
    prune.extend(chained_prune);

    Ok(RetentionPlan {
        oldest: prune.iter().map(|r| r.timestamp).min(),
        newest: prune.iter().map(|r| r.timestamp).max(),
        chained: n_chained,
        anchor,
        prune_ids: prune.iter().map(|r| r.id.clone()).collect(),
    })
}

pub enum RetentionError {
    /// Bad limits.
    Invalid(String),
    /// The chain does not verify; pruning would destroy evidence.
    IntegrityBroken(Box<VerifyReport>),
    Internal(anyhow::Error),
}

impl From<anyhow::Error> for RetentionError {
    fn from(e: anyhow::Error) -> Self {
        RetentionError::Internal(e)
    }
}

/// Plan and (unless `dry_run`) apply retention. A real run writes the
/// `AUDIT_RETENTION` event first; if that write fails nothing is deleted.
pub fn run_retention(
    store: &StateStore,
    chain_enabled: bool,
    actor: &str,
    limits: &RetentionLimits,
    dry_run: bool,
    force: bool,
    now: DateTime<Utc>,
) -> Result<RetentionReport, RetentionError> {
    let loaded = load_all(store)?;
    let plan = plan_retention(&loaded.records, limits, now).map_err(RetentionError::Invalid)?;
    let total = loaded.records.len();
    let mut report = RetentionReport {
        dry_run,
        max_age_days: limits.max_age_days,
        max_events: limits.max_events,
        total,
        pruned: plan.prune_ids.len(),
        remaining: total - plan.prune_ids.len(),
        oldest_pruned: plan.oldest,
        newest_pruned: plan.newest,
        chained_pruned: plan.chained,
        anchor: plan.anchor.clone(),
        event_id: None,
    };
    if dry_run || plan.prune_ids.is_empty() {
        return Ok(report);
    }

    if !force {
        let v = verify(store, chain_enabled)?;
        if !v.ok {
            return Err(RetentionError::IntegrityBroken(Box::new(v)));
        }
    }

    let mut ev = AuditLog::new(
        actor,
        "AUDIT_RETENTION",
        "audit",
        "audit_logs",
        AuditStatus::Success,
    );
    ev.details = Some(
        json!({
            "pruned": report.pruned,
            "chained_pruned": report.chained_pruned,
            "oldest_pruned": report.oldest_pruned,
            "newest_pruned": report.newest_pruned,
            "max_age_days": limits.max_age_days,
            "max_events": limits.max_events,
            "anchor_seq": plan.anchor.as_ref().map(|a| a.seq),
            "forced": force,
        })
        .to_string(),
    );
    let ev = append(store, chain_enabled, ev)?;
    report.event_id = Some(ev.id);

    if let Some(a) = &plan.anchor {
        store.save_entity(CHAIN, ANCHOR_ID, a)?;
    }
    for id in &plan.prune_ids {
        store.delete_entity(LOGS, id)?;
    }
    Ok(report)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn store() -> (tempfile::TempDir, StateStore) {
        let dir = tempfile::tempdir().unwrap();
        let s = StateStore::new(dir.path()).unwrap();
        (dir, s)
    }

    fn ev(n: usize) -> AuditLog {
        AuditLog::new(
            "alice",
            &format!("ACT{n}"),
            "vm",
            &format!("vm/{n}"),
            AuditStatus::Success,
        )
    }

    fn fill(s: &StateStore, chain: bool, n: usize) -> Vec<AuditLog> {
        (0..n)
            .map(|i| {
                let mut e = ev(i);
                e.timestamp = Utc::now() - Duration::minutes((n - i) as i64);
                append(s, chain, e).unwrap()
            })
            .collect()
    }

    fn rep(s: &StateStore) -> VerifyReport {
        verify(s, true).unwrap()
    }

    #[test]
    fn intact_chain_verifies() {
        let (_d, s) = store();
        let evs = fill(&s, true, 5);
        assert_eq!(evs[0].seq, Some(1));
        assert_eq!(evs[0].prev_hash.as_deref(), Some(GENESIS));
        assert_eq!(evs[1].prev_hash, evs[0].hash);
        let r = rep(&s);
        assert!(r.ok, "{:?}", r.first_break);
        assert_eq!(r.status, "ok");
        assert_eq!(r.chained, 5);
        assert_eq!(r.head.unwrap().seq, 5);
    }

    #[test]
    fn chain_off_writes_plain_events_and_reports_not_chained() {
        let (_d, s) = store();
        let evs = fill(&s, false, 3);
        assert!(evs.iter().all(|e| e.seq.is_none() && e.hash.is_none()));
        let r = rep(&s);
        assert_eq!(r.status, "not_chained");
        assert!(r.ok);
        assert_eq!(r.legacy_unchained, 3);
    }

    #[test]
    fn modified_event_is_reported_at_that_event() {
        let (_d, s) = store();
        let evs = fill(&s, true, 5);
        let mut t = evs[2].clone();
        t.action = "SOMETHING_ELSE".into();
        s.save_entity(LOGS, &t.id, &t).unwrap();
        let b = rep(&s).first_break.unwrap();
        assert_eq!(b.reason, "hash_mismatch");
        assert_eq!(b.seq, Some(3));
        assert_eq!(b.id.as_deref(), Some(evs[2].id.as_str()));
    }

    #[test]
    fn removed_middle_event_is_a_sequence_gap() {
        let (_d, s) = store();
        let evs = fill(&s, true, 5);
        s.delete_entity(LOGS, &evs[2].id).unwrap();
        let b = rep(&s).first_break.unwrap();
        assert_eq!(b.reason, "sequence_gap");
        assert_eq!(b.seq, Some(4));
    }

    #[test]
    fn removed_first_event_is_detected() {
        let (_d, s) = store();
        let evs = fill(&s, true, 3);
        s.delete_entity(LOGS, &evs[0].id).unwrap();
        assert_eq!(rep(&s).first_break.unwrap().reason, "sequence_gap");
    }

    #[test]
    fn removed_tail_is_truncation() {
        let (_d, s) = store();
        let evs = fill(&s, true, 4);
        s.delete_entity(LOGS, &evs[3].id).unwrap();
        let b = rep(&s).first_break.unwrap();
        assert_eq!(b.reason, "truncated");
    }

    #[test]
    fn removed_everything_is_truncation() {
        let (_d, s) = store();
        let evs = fill(&s, true, 2);
        for e in &evs {
            s.delete_entity(LOGS, &e.id).unwrap();
        }
        assert_eq!(rep(&s).first_break.unwrap().reason, "truncated");
    }

    #[test]
    fn reordered_events_are_detected() {
        let (_d, s) = store();
        let evs = fill(&s, true, 4);
        // Swap the sequence numbers of events 2 and 3.
        let (mut a, mut b) = (evs[1].clone(), evs[2].clone());
        std::mem::swap(&mut a.seq, &mut b.seq);
        s.save_entity(LOGS, &a.id, &a).unwrap();
        s.save_entity(LOGS, &b.id, &b).unwrap();
        let br = rep(&s).first_break.unwrap();
        assert_eq!(br.reason, "hash_mismatch");
        assert_eq!(br.seq, Some(2));
    }

    #[test]
    fn reordered_with_recomputed_hash_breaks_the_link() {
        let (_d, s) = store();
        let evs = fill(&s, true, 4);
        // Swap events 2 and 3 completely and re-hash each: the link to its
        // neighbour no longer matches.
        let (mut a, mut b) = (evs[1].clone(), evs[2].clone());
        std::mem::swap(&mut a.seq, &mut b.seq);
        a.hash = Some(a.compute_hash());
        b.hash = Some(b.compute_hash());
        s.save_entity(LOGS, &a.id, &a).unwrap();
        s.save_entity(LOGS, &b.id, &b).unwrap();
        let br = rep(&s).first_break.unwrap();
        assert_eq!(br.reason, "prev_hash_mismatch");
    }

    #[test]
    fn stripped_chain_fields_inside_the_chain_are_flagged() {
        let (_d, s) = store();
        let evs = fill(&s, true, 5);
        let mut t = evs[2].clone();
        t.seq = None;
        t.prev_hash = None;
        t.hash = None;
        s.save_entity(LOGS, &t.id, &t).unwrap();
        // The missing sequence number is found first.
        assert_eq!(rep(&s).first_break.unwrap().reason, "sequence_gap");
    }

    #[test]
    fn injected_unchained_event_inside_the_chain_is_flagged() {
        let (_d, s) = store();
        let evs = fill(&s, true, 4);
        let mut x = ev(99);
        x.timestamp = evs[1].timestamp + Duration::seconds(1);
        s.save_entity(LOGS, &x.id, &x).unwrap();
        assert_eq!(
            rep(&s).first_break.unwrap().reason,
            "unchained_inside_chain"
        );
    }

    #[test]
    fn missing_head_is_flagged() {
        let (_d, s) = store();
        fill(&s, true, 2);
        s.delete_entity(CHAIN, HEAD_ID).unwrap();
        assert_eq!(rep(&s).first_break.unwrap().reason, "head_missing");
    }

    #[test]
    fn legacy_events_before_the_chain_are_not_a_break() {
        let (_d, s) = store();
        // Old events, older than anything chained, including the old shape.
        for i in 0..3 {
            let mut e = ev(i);
            e.timestamp = Utc::now() - Duration::days(10);
            s.save_entity(LOGS, &e.id, &e).unwrap();
        }
        s.save_entity(
            LOGS,
            "old-shape",
            &json!({"id":"old-shape","timestamp":Utc::now() - Duration::days(9),
                "user_id":"bob","action":"CREATE","resource":"vm/x","status":"SUCCESS"}),
        )
        .unwrap();
        fill(&s, true, 3);
        let r = rep(&s);
        assert!(r.ok, "{:?}", r.first_break);
        assert_eq!(r.chained, 3);
        assert_eq!(r.legacy_unchained, 4);
        assert_eq!(r.total, 7);
    }

    #[test]
    fn old_shape_events_are_readable_with_normalised_fields() {
        let (_d, s) = store();
        s.save_entity(
            LOGS,
            "o1",
            &json!({"id":"o1","timestamp":"2026-01-02T03:04:05Z","user_id":"bob",
                "action":"DELETE","resource":"vm/x","status":"FAILED"}),
        )
        .unwrap();
        let l = load_all(&s).unwrap();
        let e = &l.records[0];
        assert_eq!(e.user, "bob");
        assert_eq!(e.resource_name, "vm/x");
        assert_eq!(e.status, AuditStatus::Failed);
    }

    #[test]
    fn chain_continues_after_the_config_flag_is_turned_off() {
        let (_d, s) = store();
        fill(&s, true, 2);
        let e = append(&s, false, ev(7)).unwrap();
        assert_eq!(e.seq, Some(3));
        assert!(rep(&s).ok);
    }

    #[test]
    fn unreadable_event_file_breaks_a_chained_store() {
        let (d, s) = store();
        fill(&s, true, 2);
        std::fs::write(d.path().join("audit_logs/garbage.json"), "{not json").unwrap();
        let r = rep(&s);
        assert_eq!(r.first_break.unwrap().reason, "unreadable_record");
        assert_eq!(r.malformed, vec!["garbage".to_string()]);
    }

    // ---- export ----

    #[test]
    fn export_filter_bounds_actor_action_and_tenant() {
        let mut e = ev(1);
        e.timestamp = "2026-03-01T12:00:00Z".parse().unwrap();
        e.tenant = Some("acme".into());
        let f = |from: Option<&str>, to: Option<&str>| ExportFilter {
            from: from.map(|s| parse_bound(s, false).unwrap()),
            to: to.map(|s| parse_bound(s, true).unwrap()),
            ..Default::default()
        };
        assert!(f(Some("2026-03-01"), Some("2026-03-01")).matches(&e)); // date-only `to` is the whole day
        assert!(!f(Some("2026-03-02"), None).matches(&e));
        assert!(!f(None, Some("2026-03-01T12:00:00Z")).matches(&e)); // exclusive
        let mut af = ExportFilter {
            actor: Some("ALICE".into()),
            action: Some("act1".into()),
            ..Default::default()
        };
        assert!(af.matches(&e));
        af.actor = Some("bob".into());
        assert!(!af.matches(&e));
        let tf = |t: &str, sub: &str| ExportFilter {
            tenant_scope: Some((t.into(), sub.into())),
            ..Default::default()
        };
        assert!(tf("acme", "zed").matches(&e));
        assert!(tf("other", "alice").matches(&e)); // their own action
        assert!(!tf("other", "zed").matches(&e));
        assert!(parse_bound("garbage", false).is_err());
    }

    #[test]
    fn csv_row_neutralises_formulas_and_escapes() {
        let mut e = ev(1);
        e.user = "=cmd|' /C calc'!A0".into();
        e.details = Some("line1\nline2, \"quoted\"".into());
        e.error = Some("@SUM(1)".into());
        e.resource_name = "\t=1+1".into();
        let row = csv_row(&e);
        assert!(row.contains("\"'=cmd|' /C calc'!A0\""));
        assert!(row.contains("\"line1\nline2, \"\"quoted\"\"\""));
        assert!(row.contains("\"'@SUM(1)\""));
        assert!(row.contains("\"'\t=1+1\""));
        assert!(row.ends_with('\n'));
    }

    #[test]
    fn export_is_sorted_and_bounded() {
        let (_d, s) = store();
        fill(&s, true, 5);
        let (rows, over) = collect_export(&s, &ExportFilter::default(), 10).unwrap();
        assert!(!over);
        assert!(rows.windows(2).all(|w| w[0].timestamp <= w[1].timestamp));
        let (_, over) = collect_export(&s, &ExportFilter::default(), 4).unwrap();
        assert!(over);
    }

    // ---- retention ----

    fn limits(age: Option<u64>, n: Option<u64>) -> RetentionLimits {
        RetentionLimits {
            max_age_days: age,
            max_events: n,
        }
    }

    #[test]
    fn retention_rejects_missing_or_zero_limits() {
        assert!(plan_retention(&[], &limits(None, None), Utc::now()).is_err());
        assert!(plan_retention(&[], &limits(Some(0), None), Utc::now()).is_err());
        assert!(plan_retention(&[], &limits(None, Some(0)), Utc::now()).is_err());
    }

    #[test]
    fn dry_run_changes_nothing_and_writes_no_event() {
        let (_d, s) = store();
        fill(&s, true, 6);
        let r = run_retention(
            &s,
            true,
            "admin",
            &limits(None, Some(2)),
            true,
            false,
            Utc::now(),
        )
        .ok()
        .unwrap();
        assert!(r.dry_run);
        assert_eq!(r.pruned, 4);
        assert!(r.event_id.is_none());
        let l = load_all(&s).unwrap();
        assert_eq!(l.records.len(), 6);
        assert!(l.records.iter().all(|e| e.action != "AUDIT_RETENTION"));
    }

    #[test]
    fn real_run_writes_the_event_first_and_the_chain_still_verifies() {
        let (_d, s) = store();
        fill(&s, true, 6);
        let r = run_retention(
            &s,
            true,
            "admin",
            &limits(None, Some(2)),
            false,
            false,
            Utc::now(),
        )
        .ok()
        .unwrap();
        assert_eq!(r.pruned, 4);
        assert_eq!(r.chained_pruned, 4);
        assert_eq!(r.anchor.as_ref().unwrap().seq, 4);
        let l = load_all(&s).unwrap();
        // 2 kept + the retention event itself.
        assert_eq!(l.records.len(), 3);
        let retention_ev = l
            .records
            .iter()
            .find(|e| e.action == "AUDIT_RETENTION")
            .unwrap();
        assert_eq!(Some(retention_ev.id.clone()), r.event_id);
        assert!(retention_ev
            .details
            .as_ref()
            .unwrap()
            .contains("\"pruned\":4"));
        let v = rep(&s);
        assert!(v.ok, "{:?}", v.first_break);
        assert_eq!(v.anchor.unwrap().seq, 4);
        // New events keep chaining after a prune.
        append(&s, true, ev(8)).unwrap();
        assert!(rep(&s).ok);
    }

    #[test]
    fn removing_an_event_after_a_prune_is_still_caught() {
        let (_d, s) = store();
        fill(&s, true, 6);
        run_retention(
            &s,
            true,
            "admin",
            &limits(None, Some(3)),
            false,
            false,
            Utc::now(),
        )
        .ok()
        .unwrap();
        let l = load_all(&s).unwrap();
        let mut chained: Vec<&AuditLog> = l.records.iter().collect();
        chained.sort_by_key(|e| e.seq);
        s.delete_entity(LOGS, &chained[1].id).unwrap();
        assert_eq!(rep(&s).first_break.unwrap().reason, "sequence_gap");
    }

    #[test]
    fn age_limit_prunes_old_events_and_future_dated_ones_survive() {
        let (_d, s) = store();
        let now = Utc::now();
        for (i, days) in [40i64, 35, 5, -3].iter().enumerate() {
            let mut e = ev(i);
            e.timestamp = now - Duration::days(*days);
            append(&s, false, e).unwrap();
        }
        let r = run_retention(
            &s,
            false,
            "admin",
            &limits(Some(30), None),
            false,
            false,
            now,
        )
        .ok()
        .unwrap();
        assert_eq!(r.pruned, 2);
        let l = load_all(&s).unwrap();
        assert_eq!(l.records.len(), 3); // 2 kept + retention event
    }

    #[test]
    fn retention_never_prunes_everything() {
        let (_d, s) = store();
        let mut e = ev(0);
        e.timestamp = Utc::now() - Duration::days(900);
        append(&s, false, e).unwrap();
        let r = run_retention(
            &s,
            false,
            "admin",
            &limits(Some(1), None),
            true,
            false,
            Utc::now(),
        )
        .ok()
        .unwrap();
        assert_eq!(r.pruned, 0);
    }

    #[test]
    fn legacy_and_chained_mix_prunes_legacy_first_and_keeps_chain_valid() {
        let (_d, s) = store();
        for i in 0..3 {
            let mut e = ev(i);
            e.timestamp = Utc::now() - Duration::days(10);
            append(&s, false, e).unwrap();
        }
        fill(&s, true, 4);
        // keep 3 of 7: prunes 3 legacy + 1 chained
        let r = run_retention(
            &s,
            true,
            "admin",
            &limits(None, Some(3)),
            false,
            false,
            Utc::now(),
        )
        .ok()
        .unwrap();
        assert_eq!(r.pruned, 4);
        assert_eq!(r.chained_pruned, 1);
        assert!(rep(&s).ok);
    }

    #[test]
    fn real_run_refuses_when_the_chain_is_broken_unless_forced() {
        let (_d, s) = store();
        let evs = fill(&s, true, 5);
        s.delete_entity(LOGS, &evs[2].id).unwrap();
        let lim = limits(None, Some(2));
        match run_retention(&s, true, "admin", &lim, false, false, Utc::now()) {
            Err(RetentionError::IntegrityBroken(v)) => {
                assert_eq!(v.first_break.unwrap().reason, "sequence_gap")
            }
            _ => panic!("expected the prune to be refused"),
        }
        assert_eq!(load_all(&s).unwrap().records.len(), 4);
        assert!(run_retention(&s, true, "admin", &lim, false, true, Utc::now()).is_ok());
    }
}
