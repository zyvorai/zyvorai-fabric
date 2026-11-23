// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: LicenseRef-Zyvor-Production-2.0

//! VM configuration history and "what changed since last known good".
//!
//! Every time a VM's configuration is created or changed through the API a
//! revision is recorded: a redacted, bounded, content-hashed copy of the VM's
//! configuration (the VM record plus its boot, CPU model, display, watchdog,
//! serial, volume and start-option documents). Runtime state (power state,
//! IP, pid, VNC port, timestamps, last error) is deliberately not part of it.
//!
//! * Secrets are never stored. Values under secret-looking keys (passwords,
//!   tokens, keys, credentials ...), cloud-init commands and file contents,
//!   kernel arguments, SSH public keys, private key blocks and JWT-like
//!   strings are replaced by `[redacted:<12 hex>]`, a salted SHA-256 prefix
//!   (the salt is a random per-install value kept in `config_history_meta`).
//!   The marker lets a diff say "this value changed" without showing it. A
//!   short, low-entropy secret could still be guessed by someone holding both
//!   the store and the salt; that is stated in the docs, not hidden.
//! * Revisions are bounded: strings, arrays, objects, depth and the whole
//!   document are capped; control characters are replaced.
//! * Every revision writes a `VM_CONFIG_REVISION` audit event (through the
//!   same hash-chained writer as every other event; its details carry the
//!   revision number, content hash and cause, never configuration values) and
//!   stores that event's id, seq and hash. Revisions also chain to each other
//!   per VM (`prev_hash` / `rev_hash`).
//! * Capture is idempotent: an unchanged content hash records nothing. Reads
//!   of the history also observe the live configuration, so a change that
//!   bypassed the hooked handlers is still captured (as `system:observed`).
//! * Nothing here changes a VM and nothing reverts anything.
//!
//! "Known good": a revision is marked good automatically when the VM stayed in
//! the Running state (per its start/stop/error events) for
//! `good_after_minutes` after the revision, or explicitly by an operator
//! (`POST .../mark-good`). The automatic signal is VM lifecycle state, not
//! guest or application health.

use axum::{
    body::Bytes,
    extract::{Path, Query, State},
    http::StatusCode,
    Json,
};
use chrono::{DateTime, SecondsFormat, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};
use sha2::{Digest, Sha256};
use state_store::StateStore;
use std::collections::{BTreeMap, BTreeSet};
use std::sync::{Arc, Mutex};

use crate::api::events::{VMEvent, VMEventType};
use crate::api_error::json_error;
use crate::audit_trail::{self, AuditLog, AuditStatus};
use crate::server::AppState;
use security::{RequireRead, RequireWrite};

pub const DIR: &str = "vm_config_history";
const INDEX: &str = "vm_config_history_index";
const META: &str = "config_history_meta";

const MAX_STR: usize = 256;
const MAX_ITEMS: usize = 64;
const MAX_KEYS: usize = 128;
const MAX_KEY_LEN: usize = 64;
const MAX_DEPTH: usize = 8;
/// Largest stored configuration document (serialized, after redaction).
pub const MAX_REV_BYTES: usize = 64 * 1024;
const MAX_ACTOR: usize = 128;
const MAX_CAUSE: usize = 160;
const MAX_NOTE: usize = 256;
const MAX_EVICTED_TRACKED: usize = 2048;
const DEFAULT_LIMIT: usize = 50;
const MAX_LIMIT: usize = 200;

static CAPTURE_LOCK: Mutex<()> = Mutex::new(());

// ----------------------------------------------------------------------------
// Redaction and bounding
// ----------------------------------------------------------------------------

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

fn sha256_hex(text: &str) -> String {
    hex(&Sha256::digest(text.as_bytes()))
}

fn salted_mark(salt: &str, text: &str) -> String {
    let mut h = Sha256::new();
    h.update(b"zyvor-config-history-v1\0");
    h.update(salt.as_bytes());
    h.update([0u8]);
    h.update(text.as_bytes());
    format!("[redacted:{}]", hex(&h.finalize()[..6]))
}

/// True when a field name suggests the value is a secret (or contains one).
pub fn is_secret_key(key: &str) -> bool {
    let k = key.to_ascii_lowercase();
    const NEEDLES: [&str; 12] = [
        "password",
        "passwd",
        "passphrase",
        "secret",
        "token",
        "credential",
        "private",
        "apikey",
        "api_key",
        "api-key",
        "cookie",
        "pwd",
    ];
    if NEEDLES.iter().any(|n| k.contains(n)) {
        return true;
    }
    if k == "pass" || k.ends_with("_pass") || k.ends_with("-pass") {
        return true;
    }
    if k.contains("auth") && !k.contains("author") {
        return true;
    }
    k.contains("key") && !k.contains("keymap")
}

/// Keys whose text is free-form and may embed secrets: kept only as a hash.
fn is_opaque_key(key: &str) -> bool {
    let k = key.to_ascii_lowercase();
    k == "content"
        || [
            "runcmd",
            "user_data",
            "userdata",
            "kernel_args",
            "environment",
            "script",
        ]
        .iter()
        .any(|n| k.contains(n))
}

fn clean_chars(s: &str) -> String {
    s.chars()
        .map(|c| {
            let bidi = matches!(c, '\u{202A}'..='\u{202E}' | '\u{2066}'..='\u{2069}' | '\u{200B}'..='\u{200F}');
            if c.is_control() || bidi {
                '\u{FFFD}'
            } else {
                c
            }
        })
        .collect()
}

fn truncate_chars(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        s.to_string()
    } else {
        s.chars().take(max).collect()
    }
}

/// Replace the values of `password=...` style assignments and URL userinfo
/// passwords inside free text.
fn scrub_inline(s: &str) -> String {
    const WORDS: [&str; 9] = [
        "password",
        "passwd",
        "secret",
        "token",
        "apikey",
        "api_key",
        "api-key",
        "passphrase",
        "pwd",
    ];
    let lower = s.to_ascii_lowercase();
    let bytes = s.as_bytes();
    let mut ranges: Vec<(usize, usize)> = Vec::new();
    for w in WORDS {
        let mut from = 0;
        while let Some(pos) = lower[from..].find(w) {
            let start = from + pos;
            let mut i = start + w.len();
            // rest of the identifier (db_password_hash, token_file ...)
            while i < bytes.len()
                && (bytes[i].is_ascii_alphanumeric() || bytes[i] == b'_' || bytes[i] == b'-')
            {
                i += 1;
            }
            while i < bytes.len() && (bytes[i] == b' ' || bytes[i] == b'"' || bytes[i] == b'\'') {
                i += 1;
            }
            if i < bytes.len() && (bytes[i] == b'=' || bytes[i] == b':') {
                i += 1;
                while i < bytes.len() && (bytes[i] == b' ' || bytes[i] == b'"' || bytes[i] == b'\'')
                {
                    i += 1;
                }
                let vstart = i;
                while i < bytes.len()
                    && !matches!(bytes[i], b' ' | b';' | b',' | b'&' | b'"' | b'\'' | b'\n')
                {
                    i += 1;
                }
                if i > vstart {
                    ranges.push((vstart, i));
                }
            }
            from = start + w.len();
        }
    }
    // scheme://user:password@host
    let mut from = 0;
    while let Some(pos) = s[from..].find("://") {
        let auth_start = from + pos + 3;
        let rest = &s[auth_start..];
        let end = rest
            .find(|c: char| c == '/' || c.is_whitespace())
            .unwrap_or(rest.len());
        if let Some(at) = rest[..end].rfind('@') {
            if let Some(colon) = rest[..at].find(':') {
                ranges.push((auth_start + colon + 1, auth_start + at));
            }
        }
        from = auth_start;
    }
    if ranges.is_empty() {
        return s.to_string();
    }
    ranges.sort();
    let mut out = String::new();
    let mut cur = 0;
    for (a, b) in ranges {
        if a < cur {
            if b > cur {
                cur = b;
            }
            continue;
        }
        out.push_str(&s[cur..a]);
        out.push_str("[redacted]");
        cur = b;
    }
    out.push_str(&s[cur..]);
    out
}

fn looks_secret_blob(s: &str) -> bool {
    let t = s.trim_start();
    s.contains("PRIVATE KEY")
        || t.starts_with("ssh-")
        || t.starts_with("ecdsa-")
        || t.starts_with("sk-ssh-")
        || t.starts_with("sk-ecdsa-")
        || (t.starts_with("eyJ") && t.matches('.').count() == 2)
}

/// Null, `[]` and `{}` are stored as absent, so "no tags" and "empty tag
/// list" are the same configuration.
fn is_empty_value(v: &Value) -> bool {
    match v {
        Value::Null => true,
        Value::Array(a) => a.is_empty(),
        Value::Object(m) => m.is_empty(),
        _ => false,
    }
}

struct Sanitizer<'a> {
    salt: &'a str,
    truncated: bool,
}

impl Sanitizer<'_> {
    fn key(&mut self, k: &str) -> String {
        let k = clean_chars(k);
        if k.chars().count() > MAX_KEY_LEN {
            self.truncated = true;
            format!(
                "{}~{}",
                truncate_chars(&k, MAX_KEY_LEN),
                &sha256_hex(&k)[..8]
            )
        } else {
            k
        }
    }

    fn string(&mut self, s: &str) -> String {
        if looks_secret_blob(s) {
            return salted_mark(self.salt, s);
        }
        let s = scrub_inline(&clean_chars(s));
        if s.chars().count() > MAX_STR {
            self.truncated = true;
            let n = s.chars().count() - MAX_STR;
            format!(
                "{}...[+{n} chars {}]",
                truncate_chars(&s, MAX_STR),
                salted_mark(self.salt, &s)
            )
        } else {
            s
        }
    }

    fn redact(&mut self, v: &Value, depth: usize) -> Value {
        if depth > MAX_DEPTH {
            self.truncated = true;
            return Value::String("[depth limit]".into());
        }
        match v {
            Value::Null => Value::Null,
            Value::Array(a) => {
                if a.len() > MAX_ITEMS {
                    self.truncated = true;
                }
                let mut out: Vec<Value> = a
                    .iter()
                    .take(MAX_ITEMS)
                    .map(|x| self.redact(x, depth + 1))
                    .collect();
                if a.len() > MAX_ITEMS {
                    out.push(Value::String(format!(
                        "[+{} more items]",
                        a.len() - MAX_ITEMS
                    )));
                }
                Value::Array(out)
            }
            Value::Object(m) => {
                let mut out = Map::new();
                for (k, x) in m.iter().take(MAX_KEYS) {
                    let k = self.key(k);
                    let nv = self.redact(x, depth + 1);
                    if is_empty_value(&nv) {
                        continue;
                    }
                    out.insert(k, nv);
                }
                if m.len() > MAX_KEYS {
                    self.truncated = true;
                    out.insert("_truncated_keys".into(), json!(m.len() - MAX_KEYS));
                }
                Value::Object(out)
            }
            other => Value::String(salted_mark(self.salt, &other.to_string())),
        }
    }

    fn walk(&mut self, key: Option<&str>, v: &Value, depth: usize) -> Value {
        if depth > MAX_DEPTH {
            self.truncated = true;
            return Value::String("[depth limit]".into());
        }
        if let Some(k) = key {
            if is_secret_key(k) || is_opaque_key(k) {
                return self.redact(v, depth);
            }
        }
        match v {
            Value::String(s) => Value::String(self.string(s)),
            Value::Array(a) => {
                if a.len() > MAX_ITEMS {
                    self.truncated = true;
                }
                let mut out: Vec<Value> = a
                    .iter()
                    .take(MAX_ITEMS)
                    .map(|x| self.walk(None, x, depth + 1))
                    .collect();
                if a.len() > MAX_ITEMS {
                    out.push(Value::String(format!(
                        "[+{} more items]",
                        a.len() - MAX_ITEMS
                    )));
                }
                Value::Array(out)
            }
            Value::Object(m) => {
                let mut out = Map::new();
                for (k, x) in m.iter().take(MAX_KEYS) {
                    let nk = self.key(k);
                    let nv = self.walk(Some(k), x, depth + 1);
                    if is_empty_value(&nv) {
                        continue;
                    }
                    out.insert(nk, nv);
                }
                if m.len() > MAX_KEYS {
                    self.truncated = true;
                    out.insert("_truncated_keys".into(), json!(m.len() - MAX_KEYS));
                }
                Value::Object(out)
            }
            other => other.clone(),
        }
    }
}

/// Sorted-key copy, so equal documents serialize identically.
fn canon(v: &Value) -> Value {
    match v {
        Value::Object(m) => {
            let sorted: BTreeMap<&String, Value> = m.iter().map(|(k, x)| (k, canon(x))).collect();
            Value::Object(sorted.into_iter().map(|(k, x)| (k.clone(), x)).collect())
        }
        Value::Array(a) => Value::Array(a.iter().map(canon).collect()),
        o => o.clone(),
    }
}

/// Redact and bound a raw configuration document. Returns the stored form and
/// whether anything was cut.
pub fn sanitize_config(raw: &Value, salt: &str) -> (Value, bool) {
    let mut s = Sanitizer {
        salt,
        truncated: false,
    };
    let doc = canon(&s.walk(None, raw, 0));
    let size = serde_json::to_vec(&doc)
        .map(|v| v.len())
        .unwrap_or(usize::MAX);
    if size <= MAX_REV_BYTES {
        return (doc, s.truncated);
    }
    // Too big even after bounding: keep a hash and size per top-level section.
    let mut sections = Map::new();
    if let Value::Object(m) = &doc {
        for (k, x) in m {
            let text = x.to_string();
            sections.insert(
                k.clone(),
                json!({"bytes": text.len(), "hash": salted_mark(salt, &text)}),
            );
        }
    }
    (json!({"_oversize": true, "sections": sections}), true)
}

pub fn content_hash(doc: &Value) -> String {
    sha256_hex(&canon(doc).to_string())
}

// ----------------------------------------------------------------------------
// Stored records
// ----------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditRef {
    pub id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub seq: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hash: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GoodMark {
    /// `auto` (the VM stayed Running long enough) or `operator`.
    pub source: String,
    pub at: DateTime<Utc>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub by: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub healthy_minutes: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Revision {
    pub vm: String,
    pub rev: u64,
    pub timestamp: DateTime<Utc>,
    pub actor: String,
    pub cause: String,
    pub content_hash: String,
    pub prev_hash: String,
    pub rev_hash: String,
    pub audit: AuditRef,
    pub truncated: bool,
    pub config: Value,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub good: Option<GoodMark>,
}

impl Revision {
    fn compute_hash(&self) -> String {
        let canon = json!([
            "zyvor-config-rev-v1",
            self.vm,
            self.rev,
            self.timestamp.to_rfc3339_opts(SecondsFormat::Nanos, true),
            self.actor,
            self.cause,
            self.content_hash,
            self.prev_hash,
            self.audit.id,
            self.audit.hash,
        ]);
        sha256_hex(&canon.to_string())
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct Index {
    next_rev: u64,
    head_rev: u64,
    head_rev_hash: String,
    head_content_hash: String,
    #[serde(default)]
    evicted: Vec<u64>,
    #[serde(default)]
    evicted_total: u64,
}

#[derive(Serialize, Deserialize)]
struct Salt {
    salt: String,
}

fn rev_id(rev: u64) -> String {
    format!("r{rev:010}")
}

fn dir_for(vm: &str) -> String {
    format!("{DIR}/{vm}")
}

fn get_salt(store: &StateStore) -> anyhow::Result<String> {
    if let Some(s) = store.get_entity::<Salt>(META, "salt")? {
        return Ok(s.salt);
    }
    let salt = format!(
        "{}{}",
        uuid::Uuid::new_v4().simple(),
        uuid::Uuid::new_v4().simple()
    );
    store.save_entity(META, "salt", &Salt { salt: salt.clone() })?;
    Ok(salt)
}

pub fn load_revisions(store: &StateStore, vm: &str) -> Vec<Revision> {
    let mut v: Vec<Revision> = store.list_entities(&dir_for(vm)).unwrap_or_default();
    v.sort_by_key(|r| r.rev);
    v
}

fn load_index(store: &StateStore, vm: &str) -> Index {
    store
        .get_entity::<Index>(INDEX, vm)
        .ok()
        .flatten()
        .unwrap_or_default()
}

/// The raw (unredacted) configuration document of a VM, read straight from
/// the stores. Never leaves this module un-sanitized.
fn build_raw_config(state: &AppState, vm: &vm_model::VM) -> Value {
    let mut doc = match serde_json::to_value(vm) {
        Ok(Value::Object(m)) => m,
        _ => Map::new(),
    };
    for runtime in [
        "name",
        "state",
        "ip",
        "pid",
        "vnc_port",
        "created",
        "updated",
        "last_error",
    ] {
        doc.remove(runtime);
    }
    for (key, coll) in [
        ("boot", "vm_boot_config"),
        ("cpu_model", "vm_cpu_config"),
        ("display", "vm_display_config"),
        ("watchdog", "vm_watchdog_config"),
        ("serials", "vm_serial_config"),
        ("volumes", "vm_volumes"),
        ("start_options", "vm_start_options"),
    ] {
        if let Ok(Some(v)) = state.store.get_entity::<Value>(coll, &vm.name) {
            doc.insert(key.into(), v);
        }
    }
    Value::Object(doc)
}

fn clean_label(s: &str, max: usize) -> String {
    truncate_chars(&clean_chars(s), max)
}

// ----------------------------------------------------------------------------
// Capture
// ----------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Captured {
    Recorded(u64),
    Unchanged,
    Skipped,
}

/// Record a revision of `vm_name`'s current configuration if it differs from
/// the newest revision. Never fails the caller: errors are logged (without
/// configuration values) and reported as `Skipped`.
pub fn capture(state: &AppState, vm_name: &str, actor: &str, cause: &str) -> Captured {
    capture_at(state, vm_name, actor, cause, Utc::now())
}

/// `capture` with an explicit revision time. Production callers use
/// `capture`; this exists so tests can build a timeline.
#[doc(hidden)]
pub fn capture_at(
    state: &AppState,
    vm_name: &str,
    actor: &str,
    cause: &str,
    at: DateTime<Utc>,
) -> Captured {
    match capture_inner(state, vm_name, actor, cause, at) {
        Ok(c) => c,
        Err(e) => {
            tracing::warn!("config history: capture for VM '{}' failed: {}", vm_name, e);
            Captured::Skipped
        }
    }
}

fn capture_inner(
    state: &AppState,
    vm_name: &str,
    actor: &str,
    cause: &str,
    at: DateTime<Utc>,
) -> anyhow::Result<Captured> {
    let Some(vm) = state.store.get_vm(vm_name)? else {
        return Ok(Captured::Skipped);
    };
    let _guard = CAPTURE_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let salt = get_salt(&state.store)?;
    let raw = build_raw_config(state, &vm);
    let (doc, truncated) = sanitize_config(&raw, &salt);
    let chash = content_hash(&doc);
    let mut index = load_index(&state.store, vm_name);
    if index.head_rev > 0 && index.head_content_hash == chash {
        return Ok(Captured::Unchanged);
    }
    let first = index.head_rev == 0;
    let rev = index.next_rev.max(index.head_rev) + 1;
    let actor = clean_label(actor, MAX_ACTOR);
    let cause = if first && cause == "observed" {
        "baseline: first observation of this VM".to_string()
    } else {
        clean_label(cause, MAX_CAUSE)
    };

    let mut log = AuditLog::new(
        &actor,
        "VM_CONFIG_REVISION",
        "vm",
        &format!("vm/{vm_name}"),
        AuditStatus::Success,
    );
    log.details = Some(format!(
        "rev={rev} content_hash={chash} cause={}",
        cause.replace(' ', "_")
    ));
    let log = audit_trail::append(&state.store, state.config.audit.chain, log)?;

    let mut r = Revision {
        vm: vm_name.to_string(),
        rev,
        timestamp: at,
        actor,
        cause,
        content_hash: chash.clone(),
        prev_hash: if index.head_rev == 0 {
            audit_trail::GENESIS.to_string()
        } else {
            index.head_rev_hash.clone()
        },
        rev_hash: String::new(),
        audit: AuditRef {
            id: log.id.clone(),
            seq: log.seq,
            hash: log.hash.clone(),
        },
        truncated,
        config: doc,
        good: None,
    };
    r.rev_hash = r.compute_hash();
    state
        .store
        .save_entity(&dir_for(vm_name), &rev_id(rev), &r)?;
    index.next_rev = rev;
    index.head_rev = rev;
    index.head_rev_hash = r.rev_hash.clone();
    index.head_content_hash = chash;
    evict(state, vm_name, &mut index);
    state.store.save_entity(INDEX, vm_name, &index)?;
    tracing::info!(
        "config history: VM '{}' revision {} recorded ({})",
        vm_name,
        rev,
        r.content_hash
    );
    Ok(Captured::Recorded(rev))
}

/// Keep at most `max_revisions`: oldest first, never the newest revision or
/// the last known good one.
fn evict(state: &AppState, vm: &str, index: &mut Index) {
    let cap = state.config.config_history.max_revisions_clamped();
    let revs = load_revisions(&state.store, vm);
    if revs.len() <= cap {
        return;
    }
    let newest = revs.last().map(|r| r.rev);
    let good = revs.iter().rev().find(|r| r.good.is_some()).map(|r| r.rev);
    let mut excess = revs.len() - cap;
    for r in &revs {
        if excess == 0 {
            break;
        }
        if Some(r.rev) == newest || Some(r.rev) == good {
            continue;
        }
        if state
            .store
            .delete_entity(&dir_for(vm), &rev_id(r.rev))
            .is_ok()
        {
            index.evicted.push(r.rev);
            index.evicted_total += 1;
            excess -= 1;
        }
    }
    if index.evicted.len() > MAX_EVICTED_TRACKED {
        let drop = index.evicted.len() - MAX_EVICTED_TRACKED;
        index.evicted.drain(..drop);
    }
}

// ----------------------------------------------------------------------------
// Integrity
// ----------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize)]
pub struct ChainReport {
    /// `ok`, `broken` or `empty`.
    pub status: &'static str,
    pub revisions: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub first_break: Option<ChainBreak>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ChainBreak {
    pub rev: u64,
    pub reason: &'static str,
}

pub fn verify_chain(store: &StateStore, vm: &str, revs: &[Revision]) -> ChainReport {
    let index = load_index(store, vm);
    let evicted: BTreeSet<u64> = index.evicted.iter().copied().collect();
    let mut report = ChainReport {
        status: "ok",
        revisions: revs.len(),
        first_break: None,
    };
    if revs.is_empty() {
        report.status = if index.head_rev == 0 {
            "empty"
        } else {
            "broken"
        };
        if index.head_rev != 0 {
            report.first_break = Some(ChainBreak {
                rev: index.head_rev,
                reason: "truncated",
            });
        }
        return report;
    }
    let by_rev: BTreeMap<u64, &Revision> = revs.iter().map(|r| (r.rev, r)).collect();
    let fail = |rev: u64, reason: &'static str| ChainReport {
        status: "broken",
        revisions: revs.len(),
        first_break: Some(ChainBreak { rev, reason }),
    };
    for r in revs {
        if r.compute_hash() != r.rev_hash {
            return fail(r.rev, "rev_hash_mismatch");
        }
        if content_hash(&r.config) != r.content_hash {
            return fail(r.rev, "content_hash_mismatch");
        }
        if r.rev == 1 {
            if r.prev_hash != audit_trail::GENESIS {
                return fail(r.rev, "prev_hash_mismatch");
            }
            continue;
        }
        match by_rev.get(&(r.rev - 1)) {
            Some(p) => {
                if r.prev_hash != p.rev_hash {
                    return fail(r.rev, "prev_hash_mismatch");
                }
            }
            None => {
                let first = revs.first().map(|x| x.rev) == Some(r.rev);
                if !(evicted.contains(&(r.rev - 1)) || (first && index.evicted_total > 0)) {
                    return fail(r.rev, "revision_missing");
                }
            }
        }
    }
    if let Some(last) = revs.last() {
        if last.rev != index.head_rev || last.rev_hash != index.head_rev_hash {
            return fail(index.head_rev, "head_mismatch");
        }
    }
    report
}

fn audit_link_state(store: &StateStore, r: &Revision) -> &'static str {
    match store.get_entity::<AuditLog>(audit_trail::LOGS, &r.audit.id) {
        Ok(Some(log)) => match (&r.audit.hash, &log.hash) {
            (Some(a), Some(b)) if a == b && log.compute_hash() == *b => "ok",
            (None, None) => "unchained",
            _ => "mismatch",
        },
        Ok(None) => "missing_or_pruned",
        Err(_) => "unreadable",
    }
}

// ----------------------------------------------------------------------------
// Health signal and known good
// ----------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Lifecycle {
    Up,
    Down,
}

fn lifecycle_of(t: &VMEventType) -> Option<Lifecycle> {
    match t {
        VMEventType::Started | VMEventType::Resumed | VMEventType::AutoHealed => {
            Some(Lifecycle::Up)
        }
        VMEventType::Stopped
        | VMEventType::Paused
        | VMEventType::Error
        | VMEventType::Deleted
        | VMEventType::Created => Some(Lifecycle::Down),
        _ => None,
    }
}

/// Longest stretch (seconds) the VM was continuously up inside
/// `[win_start, win_end]`. `events` are the VM's lifecycle events oldest first.
/// A stretch still open at the end only counts when the VM is running now.
pub fn healthy_overlap_secs(
    events: &[(DateTime<Utc>, Lifecycle)],
    win_start: DateTime<Utc>,
    win_end: DateTime<Utc>,
    now: DateTime<Utc>,
    running_now: bool,
) -> i64 {
    let mut best = 0i64;
    let mut up_since: Option<DateTime<Utc>> = None;
    let mut consider = |s: DateTime<Utc>, e: DateTime<Utc>| {
        let a = s.max(win_start);
        let b = e.min(win_end);
        if b > a {
            best = best.max((b - a).num_seconds());
        }
    };
    for (ts, kind) in events {
        match kind {
            Lifecycle::Up => {
                if up_since.is_none() {
                    up_since = Some(*ts);
                }
            }
            Lifecycle::Down => {
                if let Some(s) = up_since.take() {
                    consider(s, *ts);
                }
            }
        }
    }
    if let (Some(s), true) = (up_since, running_now) {
        consider(s, now);
    }
    best
}

fn vm_events(store: &StateStore, vm: &str) -> Vec<(DateTime<Utc>, Lifecycle)> {
    let all: Vec<VMEvent> = store.list_entities("vm_events").unwrap_or_default();
    let mut v: Vec<(DateTime<Utc>, Lifecycle)> = all
        .iter()
        .filter(|e| e.vm_name == vm)
        .filter_map(|e| lifecycle_of(&e.event_type).map(|l| (e.timestamp, l)))
        .collect();
    v.sort_by_key(|x| x.0);
    v
}

/// Mark revisions good that the VM has run through for long enough. Returns
/// the refreshed list.
fn refresh_good(state: &AppState, vm: &str, mut revs: Vec<Revision>) -> Vec<Revision> {
    let need = state.config.config_history.good_after_minutes_clamped() as i64 * 60;
    let current = state.store.get_vm(vm).ok().flatten();
    let running_now = current
        .as_ref()
        .is_some_and(|v| v.state == vm_model::VMState::Running && v.last_error.is_none());
    let events = vm_events(&state.store, vm);
    if events.is_empty() {
        return revs;
    }
    let now = Utc::now();
    let starts: Vec<DateTime<Utc>> = revs.iter().map(|r| r.timestamp).collect();
    for (i, r) in revs.iter_mut().enumerate() {
        if r.good.is_some() {
            continue;
        }
        let end = starts.get(i + 1).copied().unwrap_or(now);
        let secs = healthy_overlap_secs(&events, r.timestamp, end, now, running_now);
        if secs >= need {
            r.good = Some(GoodMark {
                source: "auto".into(),
                at: now,
                by: None,
                note: None,
                healthy_minutes: Some(secs / 60),
            });
            if let Err(e) = state.store.save_entity(&dir_for(vm), &rev_id(r.rev), &*r) {
                tracing::warn!(
                    "config history: could not persist good mark for '{}': {}",
                    vm,
                    e
                );
            }
        }
    }
    revs
}

fn last_good(revs: &[Revision]) -> Option<&Revision> {
    revs.iter().rev().find(|r| r.good.is_some())
}

// ----------------------------------------------------------------------------
// Diff
// ----------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct Risk {
    /// `high`, `medium` or `low`.
    pub level: &'static str,
    pub reason: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct FieldChange {
    pub path: String,
    pub label: String,
    /// `added`, `removed` or `changed`.
    pub kind: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub from: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub to: Option<Value>,
    /// What changed, in operator language.
    pub summary: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub risk: Option<Risk>,
    /// The value is hidden (a redaction marker), only "it changed" is known.
    pub hidden: bool,
}

fn is_marker(v: &Value) -> bool {
    v.as_str().is_some_and(|s| s.starts_with("[redacted"))
}

fn is_scalar(v: &Value) -> bool {
    !matches!(v, Value::Object(_) | Value::Array(_))
}

fn diff_into(
    path: &str,
    a: &Value,
    b: &Value,
    out: &mut Vec<(String, Option<Value>, Option<Value>)>,
) {
    if a == b {
        return;
    }
    match (a, b) {
        (Value::Object(x), Value::Object(y)) => {
            let keys: BTreeSet<&String> = x.keys().chain(y.keys()).collect();
            for k in keys {
                let p = if path.is_empty() {
                    k.to_string()
                } else {
                    format!("{path}.{k}")
                };
                match (x.get(k), y.get(k)) {
                    (Some(l), Some(r)) => diff_into(&p, l, r, out),
                    (Some(l), None) => out.push((p, Some(l.clone()), None)),
                    (None, Some(r)) => out.push((p, None, Some(r.clone()))),
                    (None, None) => {}
                }
            }
        }
        (Value::Array(x), Value::Array(y))
            if x.iter().all(is_scalar) && y.iter().all(is_scalar) =>
        {
            out.push((path.to_string(), Some(a.clone()), Some(b.clone())));
        }
        (Value::Array(x), Value::Array(y)) => {
            for i in 0..x.len().max(y.len()) {
                let p = format!("{path}[{i}]");
                match (x.get(i), y.get(i)) {
                    (Some(l), Some(r)) => diff_into(&p, l, r, out),
                    (Some(l), None) => out.push((p, Some(l.clone()), None)),
                    (None, Some(r)) => out.push((p, None, Some(r.clone()))),
                    (None, None) => {}
                }
            }
        }
        _ => out.push((path.to_string(), Some(a.clone()), Some(b.clone()))),
    }
}

fn show(v: &Value) -> String {
    let s = match v {
        Value::String(s) => s.clone(),
        Value::Null => "none".into(),
        other => other.to_string(),
    };
    let s = clean_chars(&s);
    if s.chars().count() > 80 {
        format!("{}...", truncate_chars(&s, 80))
    } else {
        s
    }
}

fn set_items(v: &Value) -> BTreeSet<String> {
    v.as_array()
        .map(|a| a.iter().map(show).collect())
        .unwrap_or_default()
}

fn risk(level: &'static str, reason: &str) -> Option<Risk> {
    Some(Risk {
        level,
        reason: reason.to_string(),
    })
}

/// Normalize `volumes[2].host` to `volumes[].host` for rule matching.
fn shape(path: &str) -> String {
    let mut out = String::new();
    let mut skip = false;
    for c in path.chars() {
        match c {
            '[' => {
                skip = true;
                out.push('[');
            }
            ']' => {
                skip = false;
                out.push(']');
            }
            _ if skip => {}
            _ => out.push(c),
        }
    }
    out
}

fn num(v: &Option<Value>) -> Option<f64> {
    v.as_ref().and_then(|x| x.as_f64())
}

/// Describe one differing field: operator-language sentence, label, risk.
fn describe(
    path: &str,
    from: &Option<Value>,
    to: &Option<Value>,
) -> (String, String, Option<Risk>, bool) {
    let hidden = from.as_ref().is_some_and(is_marker)
        || to.as_ref().is_some_and(is_marker)
        || (from.is_none()
            && to
                .as_ref()
                .is_some_and(|v| v.as_array().is_some_and(|a| a.iter().any(is_marker))))
        || (to.is_none()
            && from
                .as_ref()
                .is_some_and(|v| v.as_array().is_some_and(|a| a.iter().any(is_marker))));
    let sh = shape(path);
    let f = from.clone().unwrap_or(Value::Null);
    let t = to.clone().unwrap_or(Value::Null);
    let added = from.is_none();
    let removed = to.is_none();
    let (fs, ts) = (show(&f), show(&t));
    let unit_change =
        |label: &str, unit: &str, shrink: Option<(&'static str, &str)>, grow_note: &str| {
            let (a, b) = (num(from), num(to));
            let mut r = None;
            let text = match (a, b) {
                (Some(a), Some(b)) if b < a => {
                    if let Some((lvl, why)) = shrink {
                        r = risk(lvl, why);
                    }
                    format!(
                        "{label} decreased from {fs} to {ts} {unit} (a reduction of {} {unit})",
                        fmt_num(a - b)
                    )
                }
                (Some(a), Some(b)) => format!(
                    "{label} increased from {fs} to {ts} {unit} (+{} {unit}){grow_note}",
                    fmt_num(b - a)
                ),
                _ => format!("{label} changed from {fs} to {ts}"),
            };
            (text, r)
        };

    match sh.as_str() {
        "cpus" => {
            let (s, r) = unit_change("vCPU count", "vCPUs", Some(("high", "Fewer vCPUs can starve the workload; thread pools and licences are often sized for the old count.")), "");
            return ("vCPUs".into(), s, r, hidden);
        }
        "memory" => {
            let (s, r) = unit_change(
                "Memory",
                "MB",
                Some((
                    "high",
                    "Less memory can trigger out-of-memory kills and swapping in the guest.",
                )),
                "",
            );
            return ("Memory (MB)".into(), s, r, hidden);
        }
        "disk" => {
            let (s, r) = unit_change(
                "Disk size",
                "GB",
                Some((
                    "high",
                    "A smaller disk can truncate data or fail to apply; shrinking is rarely safe.",
                )),
                "",
            );
            return ("Disk size (GB)".into(), s, r, hidden);
        }
        "image" => {
            return (
                "Base image".into(),
                format!("Base image changed from {fs} to {ts}"),
                risk("medium", "A different base image is a different OS template; the VM may not boot or behave as before."),
                hidden,
            );
        }
        "mac_address" => {
            return (
                "MAC address".into(),
                format!("NIC MAC address changed from {fs} to {ts}"),
                risk("high", "A new MAC changes the guest's interface identity: DHCP reservations, MAC-bound licences and interface naming can break."),
                hidden,
            );
        }
        "storage" => {
            return (
                "Storage backend".into(),
                format!("Storage backend changed from {fs} to {ts}"),
                risk(
                    "high",
                    "A different storage backend changes where and how the disk is stored.",
                ),
                hidden,
            );
        }
        "hostname" => {
            return (
                "Hostname".into(),
                format!("Hostname changed from {fs} to {ts}"),
                risk(
                    "low",
                    "Guests that pin their hostname in config may need it updated.",
                ),
                hidden,
            );
        }
        "network_tap" | "network_static_ip" | "direct_uplink" | "direct_mode"
        | "direct_guest_ips" => {
            return (
                format!("Network attachment ({sh})"),
                format!("Network attachment setting {sh} changed from {fs} to {ts}"),
                risk("medium", "Changes how the VM attaches to the network; applies when the VM is next (re)created."),
                hidden,
            );
        }
        "tags" => {
            let (a, b) = (set_items(&f), set_items(&t));
            let add: Vec<_> = b.difference(&a).cloned().collect();
            let rem: Vec<_> = a.difference(&b).cloned().collect();
            return (
                "Tags".into(),
                format!(
                    "Tags changed: added [{}], removed [{}]",
                    add.join(", "),
                    rem.join(", ")
                ),
                None,
                hidden,
            );
        }
        "ssh_authorized_keys" => {
            let (a, b) = (set_items(&f), set_items(&t));
            let add = b.difference(&a).count();
            let rem = a.difference(&b).count();
            return (
                "SSH authorized keys".into(),
                format!("SSH authorized keys changed: {add} added, {rem} removed (key text is not stored)"),
                risk("low", "Changes who can log in to the guest."),
                true,
            );
        }
        "cloud_init_packages" => {
            let (a, b) = (set_items(&f), set_items(&t));
            let add: Vec<_> = b.difference(&a).cloned().collect();
            let rem: Vec<_> = a.difference(&b).cloned().collect();
            return (
                "Cloud-init packages".into(),
                format!(
                    "Cloud-init packages changed: added [{}], removed [{}]",
                    add.join(", "),
                    rem.join(", ")
                ),
                risk("low", "Applies on the next (re)creation only."),
                hidden,
            );
        }
        "cloud_init_runcmd" => {
            let n = |v: &Value| v.as_array().map_or(0, |a| a.len());
            return (
                "Cloud-init commands".into(),
                format!(
                    "Cloud-init commands changed ({} before, {} after; command text is not stored)",
                    n(&f),
                    n(&t)
                ),
                risk("low", "Applies on the next (re)creation only."),
                true,
            );
        }
        "boot.firmware" => {
            return ("Firmware".into(), format!("Firmware changed from {fs} to {ts}"), risk("high", "BIOS and UEFI guests are not interchangeable; the installed OS may no longer boot."), hidden);
        }
        "boot.secure_boot" => {
            return (
                "Secure Boot".into(),
                format!("Secure Boot changed from {fs} to {ts}"),
                risk(
                    "medium",
                    "Unsigned kernels or drivers stop loading when Secure Boot is turned on.",
                ),
                hidden,
            );
        }
        "boot.boot_order" => {
            return (
                "Boot order".into(),
                format!(
                    "Boot order changed from [{}] to [{}]",
                    set_list(&f),
                    set_list(&t)
                ),
                risk(
                    "medium",
                    "A different first boot device can boot an installer, the network or nothing.",
                ),
                hidden,
            );
        }
        "boot.kernel" | "boot.initrd" | "boot.kernel_args" => {
            return (
                format!("Direct boot ({})", &sh[5..]),
                format!(
                    "Direct-boot setting {} changed (value {})",
                    &sh[5..],
                    if hidden {
                        "hidden"
                    } else {
                        "shown in the revision"
                    }
                ),
                risk("medium", "Changes what the VM boots."),
                hidden,
            );
        }
        "cpu_model.mode" | "cpu_model.model" | "cpu_model.vendor" => {
            return ("CPU model".into(), format!("CPU model setting {} changed from {fs} to {ts}", &sh[10..]), risk("high", "The guest sees different CPU features; this can break live migration to older hosts and software licensed to a CPU."), hidden);
        }
        "cpu_model.features_add" | "cpu_model.features_remove" => {
            return (
                "CPU features".into(),
                format!(
                    "CPU feature list {} changed from [{}] to [{}]",
                    &sh[10..],
                    set_list(&f),
                    set_list(&t)
                ),
                risk("medium", "Guest-visible CPU features changed."),
                hidden,
            );
        }
        "machine_type" | "machine" | "boot.machine_type" => {
            return (
                "Machine type".into(),
                format!("Machine type changed from {fs} to {ts}"),
                risk(
                    "high",
                    "The virtual chipset changes device layout and guest drivers.",
                ),
                hidden,
            );
        }
        _ => {}
    }
    if sh == "volumes" || sh.starts_with("volumes[") || sh.starts_with("disks[") || sh == "disks" {
        if sh.ends_with("bus") {
            return ("Disk bus".into(), format!("Disk bus of {path} changed from {fs} to {ts}"), risk("high", "Changing the disk bus (for example virtio to sata) can make the guest lose its boot disk."), hidden);
        }
        if removed {
            return (
                "Disk / volume".into(),
                format!("{path} was removed (a disk or volume is no longer attached)"),
                risk(
                    "high",
                    "Removing a disk or volume can take data away from the guest.",
                ),
                hidden,
            );
        }
        if added {
            return (
                "Disk / volume".into(),
                format!("{path} was added"),
                None,
                hidden,
            );
        }
        return (
            "Disk / volume".into(),
            format!("{path} changed from {fs} to {ts}"),
            risk("low", "A volume setting changed."),
            hidden,
        );
    }
    if sh.starts_with("nics[") || sh.starts_with("network[") || sh.starts_with("interfaces[") {
        if sh.ends_with(".model") {
            return ("NIC model".into(), format!("NIC model of {path} changed from {fs} to {ts}"), risk("high", "A different NIC model renames the guest interface and needs different drivers."), hidden);
        }
        if sh.ends_with(".mac") || sh.ends_with(".mac_address") {
            return (
                "NIC MAC".into(),
                format!("MAC of {path} changed from {fs} to {ts}"),
                risk(
                    "high",
                    "A new MAC can break DHCP reservations and interface naming.",
                ),
                hidden,
            );
        }
        if removed {
            return (
                "Network interface".into(),
                format!("{path} was removed"),
                risk("high", "The VM lost a network interface."),
                hidden,
            );
        }
    }
    if sh.starts_with("port_forwards[") || sh == "port_forwards" {
        if removed {
            return (
                "Port forward".into(),
                format!("Port forward {path} was removed ({fs})"),
                risk("medium", "Clients using that forward lose access."),
                hidden,
            );
        }
        if added {
            return (
                "Port forward".into(),
                format!("Port forward {path} was added ({ts})"),
                None,
                hidden,
            );
        }
        return (
            "Port forward".into(),
            format!("Port forward {path} changed from {fs} to {ts}"),
            risk("medium", "Clients using the old forward may lose access."),
            hidden,
        );
    }
    if let Some(k) = sh.strip_prefix("labels.") {
        let r = if k == "fabric.zyvor.dev/host" {
            risk("medium", "The VM is now associated with a different host; placement and drain plans follow this label.")
        } else if k == "fabric.zyvor.dev/eviction" {
            risk(
                "low",
                "Changes how this VM is treated when its host is drained.",
            )
        } else {
            None
        };
        let text = if hidden {
            format!("Label {k} changed (value hidden)")
        } else if added {
            format!("Label {k} was added with value {ts}")
        } else if removed {
            format!("Label {k} was removed (was {fs})")
        } else {
            format!("Label {k} changed from {fs} to {ts}")
        };
        return (format!("Label {k}"), text, r, hidden);
    }
    if sh.starts_with("cloud_init_write_files") {
        return (
            "Cloud-init files".into(),
            format!("Cloud-init file setting {path} changed (file content is not stored)"),
            risk("low", "Applies on the next (re)creation only."),
            true,
        );
    }
    let label = sh.replace('_', " ");
    let text = if hidden {
        format!("{path} changed (value hidden)")
    } else if added {
        format!("{path} was added with value {ts}")
    } else if removed {
        format!("{path} was removed (was {fs})")
    } else {
        format!("{path} changed from {fs} to {ts}")
    };
    (label, text, None, hidden)
}

fn set_list(v: &Value) -> String {
    match v {
        Value::Array(a) => a.iter().map(show).collect::<Vec<_>>().join(", "),
        Value::Null => String::new(),
        o => show(o),
    }
}

fn fmt_num(n: f64) -> String {
    if n.fract() == 0.0 {
        format!("{}", n as i64)
    } else {
        format!("{n:.2}")
    }
}

/// Field-level diff of two stored configuration documents.
pub fn diff_configs(a: &Value, b: &Value) -> Vec<FieldChange> {
    let mut raw = Vec::new();
    diff_into("", a, b, &mut raw);
    raw.into_iter()
        .map(|(path, from, to)| {
            let (label, summary, risk, hidden) = describe(&path, &from, &to);
            let kind = match (&from, &to) {
                (None, Some(_)) => "added",
                (Some(_), None) => "removed",
                _ => "changed",
            };
            FieldChange {
                path,
                label,
                kind,
                from,
                to,
                summary,
                risk,
                hidden,
            }
        })
        .collect()
}

fn risk_rank(r: &Option<Risk>) -> u32 {
    match r.as_ref().map(|x| x.level) {
        Some("high") => 3,
        Some("medium") => 2,
        Some("low") => 1,
        _ => 0,
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct DiffCounts {
    pub total: usize,
    pub high: usize,
    pub medium: usize,
    pub low: usize,
}

fn counts(changes: &[FieldChange]) -> DiffCounts {
    let n = |lvl: &str| {
        changes
            .iter()
            .filter(|c| c.risk.as_ref().is_some_and(|r| r.level == lvl))
            .count()
    };
    DiffCounts {
        total: changes.len(),
        high: n("high"),
        medium: n("medium"),
        low: n("low"),
    }
}

// ----------------------------------------------------------------------------
// API
// ----------------------------------------------------------------------------

type ApiErr = (StatusCode, Json<Value>);

fn vm_name_ok(name: &str) -> Result<(), ApiErr> {
    crate::validation::validate_vm_name(name).map_err(|(s, m)| json_error(s, m))
}

fn parse_rev(s: &str) -> Result<u64, ApiErr> {
    s.trim()
        .parse::<u64>()
        .ok()
        .filter(|n| *n >= 1)
        .ok_or_else(|| {
            json_error(
                StatusCode::BAD_REQUEST,
                "revision must be a positive integer",
            )
        })
}

/// Make sure the newest revision reflects the live configuration, then load.
fn observed(state: &AppState, vm: &str) -> Result<Vec<Revision>, ApiErr> {
    capture(state, vm, "system:observed", "observed");
    let revs = load_revisions(&state.store, vm);
    if revs.is_empty() {
        return Err(json_error(
            StatusCode::NOT_FOUND,
            "VM not found or it has no recorded configuration history",
        ));
    }
    Ok(refresh_good(state, vm, revs))
}

fn rev_summary(store: &StateStore, r: &Revision, prev: Option<&Revision>) -> Value {
    let (changes, high) = match prev {
        Some(p) => {
            let d = diff_configs(&p.config, &r.config);
            let h = d.iter().filter(|c| risk_rank(&c.risk) == 3).count();
            (d.len(), h)
        }
        None => (0, 0),
    };
    json!({
        "rev": r.rev,
        "timestamp": r.timestamp,
        "actor": r.actor,
        "cause": r.cause,
        "content_hash": r.content_hash,
        "rev_hash": r.rev_hash,
        "truncated": r.truncated,
        "audit": {"id": r.audit.id, "seq": r.audit.seq, "hash": r.audit.hash, "link": audit_link_state(store, r)},
        "good": r.good,
        "changed_fields": changes,
        "high_risk_changes": high,
    })
}

#[derive(Debug, Deserialize)]
pub struct HistoryQuery {
    pub limit: Option<usize>,
}

/// GET /api/vms/{name}/history
pub async fn list_history(
    RequireRead(_claims): RequireRead,
    State(state): State<Arc<AppState>>,
    Path(name): Path<String>,
    Query(q): Query<HistoryQuery>,
) -> Result<Json<Value>, ApiErr> {
    vm_name_ok(&name)?;
    let limit = q.limit.unwrap_or(DEFAULT_LIMIT);
    if limit == 0 || limit > MAX_LIMIT {
        return Err(json_error(
            StatusCode::BAD_REQUEST,
            format!("limit must be 1..={MAX_LIMIT}"),
        ));
    }
    let revs = observed(&state, &name)?;
    let chain = verify_chain(&state.store, &name, &revs);
    let index = load_index(&state.store, &name);
    let rows: Vec<Value> = revs
        .iter()
        .enumerate()
        .rev()
        .take(limit)
        .map(|(i, r)| rev_summary(&state.store, r, if i > 0 { revs.get(i - 1) } else { None }))
        .collect();
    Ok(Json(json!({
        "vm": name,
        "exists": state.store.get_vm(&name).ok().flatten().is_some(),
        "current_rev": revs.last().map(|r| r.rev),
        "last_good_rev": last_good(&revs).map(|r| r.rev),
        "retention": {
            "max_revisions": state.config.config_history.max_revisions_clamped(),
            "kept": revs.len(),
            "evicted_total": index.evicted_total,
        },
        "good_after_minutes": state.config.config_history.good_after_minutes_clamped(),
        "good_signal": "A revision is marked good automatically when the VM stayed in the Running state (no stop, pause or error event) long enough after it. This is VM lifecycle state, not guest or application health; use mark-good to record a judgement you trust more.",
        "redaction": "Secrets, cloud-init commands and file contents, SSH keys and kernel arguments are stored only as salted hash markers.",
        "chain": chain,
        "truncated_list": revs.len() > limit,
        "revisions": rows,
    })))
}

/// GET /api/vms/{name}/history/{rev}
pub async fn get_revision(
    RequireRead(_claims): RequireRead,
    State(state): State<Arc<AppState>>,
    Path((name, rev)): Path<(String, String)>,
) -> Result<Json<Value>, ApiErr> {
    vm_name_ok(&name)?;
    let n = parse_rev(&rev)?;
    let revs = load_revisions(&state.store, &name);
    let Some(i) = revs.iter().position(|r| r.rev == n) else {
        return Err(json_error(
            StatusCode::NOT_FOUND,
            format!("revision {n} not found (never recorded, or evicted by the retention cap)"),
        ));
    };
    let r = &revs[i];
    let mut v = rev_summary(&state.store, r, if i > 0 { revs.get(i - 1) } else { None });
    if let Value::Object(m) = &mut v {
        m.insert("vm".into(), json!(name));
        m.insert("prev_hash".into(), json!(r.prev_hash));
        m.insert("config".into(), r.config.clone());
    }
    Ok(Json(v))
}

#[derive(Debug, Deserialize)]
pub struct DiffQuery {
    pub from: Option<String>,
    pub to: Option<String>,
}

fn find(revs: &[Revision], n: u64) -> Result<&Revision, ApiErr> {
    revs.iter().find(|r| r.rev == n).ok_or_else(|| {
        json_error(
            StatusCode::NOT_FOUND,
            format!("revision {n} not found (never recorded, or evicted by the retention cap)"),
        )
    })
}

fn who(r: &Revision) -> Value {
    json!({"rev": r.rev, "actor": r.actor, "at": r.timestamp, "cause": r.cause})
}

fn diff_body(vm: &str, a: &Revision, b: &Revision) -> Value {
    let changes = diff_configs(&a.config, &b.config);
    let c = counts(&changes);
    let summary = if changes.is_empty() {
        format!(
            "Revisions {} and {} have identical configuration.",
            a.rev, b.rev
        )
    } else {
        format!(
            "{} field(s) differ between revision {} and revision {}; {} high-risk, {} medium-risk, {} low-risk.",
            c.total, a.rev, b.rev, c.high, c.medium, c.low
        )
    };
    json!({
        "vm": vm,
        "from": who(a),
        "to": who(b),
        "identical": changes.is_empty(),
        "identical_content_hash": a.content_hash == b.content_hash,
        "counts": c,
        "summary": summary,
        "changes": changes,
    })
}

/// GET /api/vms/{name}/history/diff?from=&to=
pub async fn diff_history(
    RequireRead(_claims): RequireRead,
    State(state): State<Arc<AppState>>,
    Path(name): Path<String>,
    Query(q): Query<DiffQuery>,
) -> Result<Json<Value>, ApiErr> {
    vm_name_ok(&name)?;
    let from = parse_rev(
        q.from
            .as_deref()
            .ok_or_else(|| json_error(StatusCode::BAD_REQUEST, "from is required"))?,
    )?;
    let revs = observed(&state, &name)?;
    let to = match q.to.as_deref() {
        Some(t) => parse_rev(t)?,
        None => revs.last().map(|r| r.rev).unwrap_or(0),
    };
    let (a, b) = (find(&revs, from)?, find(&revs, to)?);
    Ok(Json(diff_body(&name, a, b)))
}

/// Rank the changed fields between a good revision and now, most likely
/// culprit first: risk level, then how recent the change was.
fn rank_culprits(revs: &[Revision], good_idx: usize) -> Vec<Value> {
    let last = revs.len() - 1;
    let total = diff_configs(&revs[good_idx].config, &revs[last].config);
    // Who touched each path, walking consecutive revisions after the good one.
    let mut touched: BTreeMap<String, Vec<(usize, Value)>> = BTreeMap::new();
    for i in good_idx + 1..=last {
        for c in diff_configs(&revs[i - 1].config, &revs[i].config) {
            touched.entry(c.path).or_default().push((i, who(&revs[i])));
        }
    }
    let mut rows: Vec<(u32, usize, FieldChange, Vec<Value>)> = total
        .into_iter()
        .map(|c| {
            let by = touched.get(&c.path).cloned().unwrap_or_default();
            let newest = by.iter().map(|(i, _)| *i).max().unwrap_or(good_idx);
            (
                risk_rank(&c.risk),
                newest,
                c,
                by.into_iter().map(|(_, w)| w).collect(),
            )
        })
        .collect();
    rows.sort_by(|x, y| {
        y.0.cmp(&x.0)
            .then(y.1.cmp(&x.1))
            .then(x.2.path.cmp(&y.2.path))
    });
    rows.into_iter()
        .enumerate()
        .map(|(n, (_, _, c, by))| {
            json!({
                "rank": n + 1,
                "path": c.path,
                "label": c.label,
                "kind": c.kind,
                "from": c.from,
                "to": c.to,
                "summary": c.summary,
                "risk": c.risk,
                "hidden": c.hidden,
                "changed_in": by,
            })
        })
        .collect()
}

fn since_good_summary(vm: &str, revs: &[Revision], good_idx: usize, culprits: &[Value]) -> String {
    let g = &revs[good_idx];
    let cur = &revs[revs.len() - 1];
    let mut actors: Vec<String> = revs[good_idx + 1..]
        .iter()
        .map(|r| r.actor.clone())
        .collect();
    actors.sort();
    actors.dedup();
    let how = match g.good.as_ref() {
        Some(m) if m.source == "operator" => format!(
            "marked good by {}",
            m.by.clone().unwrap_or_else(|| "an operator".into())
        ),
        Some(m) => format!(
            "marked good automatically after {} minute(s) of running",
            m.healthy_minutes.unwrap_or(0)
        ),
        None => "marked good".into(),
    };
    let high = culprits
        .iter()
        .filter(|c| c["risk"]["level"] == "high")
        .count();
    let med = culprits
        .iter()
        .filter(|c| c["risk"]["level"] == "medium")
        .count();
    let mut s = format!(
        "{vm}: since revision {} ({how}, {}), the configuration changed in {} revision(s) by {}, leaving {} differing field(s) in revision {}; {} high-risk and {} medium-risk.",
        g.rev,
        g.timestamp.to_rfc3339_opts(SecondsFormat::Secs, true),
        cur.rev - g.rev,
        actors.join(", "),
        culprits.len(),
        cur.rev,
        high,
        med
    );
    if let Some(top) = culprits.first() {
        s.push_str(&format!(
            " Most likely culprit: {}",
            top["summary"].as_str().unwrap_or("")
        ));
        if let Some(w) = top["changed_in"].as_array().and_then(|a| a.last()) {
            s.push_str(&format!(
                " (revision {} by {} at {})",
                w["rev"],
                w["actor"].as_str().unwrap_or("?"),
                w["at"].as_str().unwrap_or("?")
            ));
        }
        s.push('.');
    }
    s
}

/// GET /api/vms/{name}/history/since-good
pub async fn since_good(
    RequireRead(_claims): RequireRead,
    State(state): State<Arc<AppState>>,
    Path(name): Path<String>,
) -> Result<Json<Value>, ApiErr> {
    vm_name_ok(&name)?;
    let revs = observed(&state, &name)?;
    let cur = revs.last().map(|r| r.rev);
    let Some(gi) = revs.iter().rposition(|r| r.good.is_some()) else {
        return Ok(Json(json!({
            "vm": name,
            "status": "no_known_good",
            "current_rev": cur,
            "message": "No revision is marked good yet. Either the VM has not stayed Running for long enough after any revision, or its lifecycle events are missing. Mark a revision you trust with POST /api/vms/{name}/history/{rev}/mark-good, then ask again.",
            "good_after_minutes": state.config.config_history.good_after_minutes_clamped(),
        })));
    };
    let g = &revs[gi];
    if gi == revs.len() - 1 || g.content_hash == revs[revs.len() - 1].content_hash {
        return Ok(Json(json!({
            "vm": name,
            "status": "unchanged",
            "good_rev": g.rev,
            "current_rev": cur,
            "good": g.good,
            "summary": format!("{name}: the current configuration is identical to revision {} (last known good); nothing changed.", g.rev),
            "changes": [],
            "culprits": [],
        })));
    }
    let culprits = rank_culprits(&revs, gi);
    let summary = since_good_summary(&name, &revs, gi, &culprits);
    let diff = diff_body(&name, g, &revs[revs.len() - 1]);
    Ok(Json(json!({
        "vm": name,
        "status": "changed",
        "good_rev": g.rev,
        "current_rev": cur,
        "good": g.good,
        "summary": summary,
        "counts": diff["counts"],
        "culprits": culprits,
        "revisions_since": revs[gi + 1..].iter().map(who).collect::<Vec<_>>(),
        "note": "Read-only. Nothing was reverted; to undo a change, apply the old value through the normal VM settings.",
    })))
}

/// POST /api/vms/{name}/history/{rev}/mark-good  (optional body `{"note":"..."}`)
pub async fn mark_good(
    RequireWrite(claims): RequireWrite,
    State(state): State<Arc<AppState>>,
    Path((name, rev)): Path<(String, String)>,
    body: Bytes,
) -> Result<Json<Value>, ApiErr> {
    vm_name_ok(&name)?;
    let n = parse_rev(&rev)?;
    let note = if body.is_empty() {
        None
    } else {
        let v: Value = serde_json::from_slice(&body).map_err(|_| {
            json_error(
                StatusCode::BAD_REQUEST,
                "body must be JSON like {\"note\":\"text\"}",
            )
        })?;
        match v.get("note") {
            None | Some(Value::Null) => None,
            Some(Value::String(s)) => Some(clean_label(&scrub_inline(s), MAX_NOTE)),
            Some(_) => return Err(json_error(StatusCode::BAD_REQUEST, "note must be a string")),
        }
    };
    let revs = load_revisions(&state.store, &name);
    let mut r = find(&revs, n)?.clone();
    r.good = Some(GoodMark {
        source: "operator".into(),
        at: Utc::now(),
        by: Some(clean_label(&claims.sub, MAX_ACTOR)),
        note,
        healthy_minutes: None,
    });
    state
        .store
        .save_entity(&dir_for(&name), &rev_id(n), &r)
        .map_err(|e| json_error(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    crate::routes::audit(
        &state,
        &claims.sub,
        "VM_CONFIG_MARK_GOOD",
        &format!("vm/{name}"),
        "SUCCESS",
    );
    let prev = if let Some(i) = revs.iter().position(|x| x.rev == n) {
        if i > 0 {
            revs.get(i - 1)
        } else {
            None
        }
    } else {
        None
    };
    Ok(Json(rev_summary(&state.store, &r, prev)))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(min: i64) -> DateTime<Utc> {
        DateTime::<Utc>::from_timestamp(1_800_000_000 + min * 60, 0).unwrap()
    }

    #[test]
    fn secret_keys_and_inline_values_are_redacted() {
        let raw = json!({
            "cpus": 2,
            "hostname": "web password=hunter2 ok",
            "labels": {"db_password": "hunter2", "region": "eu"},
            "ssh_authorized_keys": ["ssh-ed25519 AAAAC3Nza hunter2@host"],
            "cloud_init_runcmd": ["echo hunter2 | passwd"],
            "cloud_init_write_files": [{"path": "/etc/x", "content": "token=hunter2"}],
            "boot": {"kernel_args": "root=/dev/vda secret=hunter2"},
            "display": {"keymap": "en-us"},
            "note": "see https://admin:hunter2@example.com/x",
            "blob": "-----BEGIN OPENSSH PRIVATE KEY----- hunter2",
        });
        let (doc, _) = sanitize_config(&raw, "salt");
        let text = doc.to_string();
        assert!(!text.contains("hunter2"), "{text}");
        assert!(text.contains("en-us"), "keymap must stay visible");
        assert!(text.contains("/etc/x"));
        assert_eq!(doc["labels"]["region"], "eu");
    }

    #[test]
    fn hostile_input_is_bounded() {
        let big = "A".repeat(100_000);
        let many: Vec<Value> = (0..10_000).map(|i| json!(i)).collect();
        let mut deep = json!("x");
        for _ in 0..50 {
            deep = json!({ "n": deep });
        }
        let raw = json!({"image": big, "tags": many, "deep": deep, "ctl": "a\u{0}b\u{1b}[31m\nc\u{202e}d"});
        let (doc, truncated) = sanitize_config(&raw, "s");
        assert!(truncated);
        assert!(serde_json::to_vec(&doc).unwrap().len() <= MAX_REV_BYTES);
        let ctl = doc["ctl"].as_str().unwrap();
        assert!(
            !ctl.chars().any(|c| c.is_control() || c == '\u{202e}'),
            "{ctl:?}"
        );
    }

    #[test]
    fn oversize_falls_back_to_section_hashes() {
        let mut m = Map::new();
        for i in 0..128 {
            m.insert(format!("k{i}"), Value::String("x".repeat(250)));
        }
        let mut big = Map::new();
        for i in 0..40 {
            big.insert(format!("s{i}"), Value::Object(m.clone()));
        }
        let (doc, truncated) = sanitize_config(&Value::Object(big), "s");
        assert!(truncated);
        assert_eq!(doc["_oversize"], true);
    }

    #[test]
    fn same_secret_same_marker_different_secret_different_marker() {
        let a = sanitize_config(&json!({"password": "one"}), "s").0;
        let b = sanitize_config(&json!({"password": "one"}), "s").0;
        let c = sanitize_config(&json!({"password": "two"}), "s").0;
        assert_eq!(a, b);
        assert_ne!(a, c);
        assert_ne!(a, sanitize_config(&json!({"password": "one"}), "other").0);
    }

    #[test]
    fn diff_flags_risky_changes() {
        let a = json!({"cpus": 4, "memory": 4096, "disk": 40, "mac_address": "52:54:00:00:00:01",
            "boot": {"firmware": "bios", "boot_order": ["disk", "network"]},
            "volumes": [{"host": "/a", "guest": "/a"}], "tags": ["x"]});
        let b = json!({"cpus": 2, "memory": 2048, "disk": 80, "mac_address": "52:54:00:00:00:02",
            "boot": {"firmware": "uefi", "boot_order": ["network", "disk"]},
            "volumes": [], "tags": ["x", "y"]});
        let d = diff_configs(&a, &b);
        let level = |p: &str| {
            d.iter()
                .find(|c| c.path == p)
                .and_then(|c| c.risk.as_ref().map(|r| r.level))
        };
        assert_eq!(level("cpus"), Some("high"));
        assert_eq!(level("memory"), Some("high"));
        assert_eq!(level("disk"), None, "growing a disk is not risky");
        assert_eq!(level("mac_address"), Some("high"));
        assert_eq!(level("boot.firmware"), Some("high"));
        assert_eq!(level("boot.boot_order"), Some("medium"));
        assert_eq!(level("volumes[0]"), Some("high"));
        assert_eq!(level("tags"), None);
        let cpu = d.iter().find(|c| c.path == "cpus").unwrap();
        assert!(
            cpu.summary.contains("decreased from 4 to 2"),
            "{}",
            cpu.summary
        );
    }

    #[test]
    fn identical_documents_have_no_changes() {
        let a = json!({"cpus": 1, "labels": {"a": "b"}});
        assert!(diff_configs(&a, &a.clone()).is_empty());
    }

    #[test]
    fn hidden_values_say_changed_not_the_value() {
        let a = sanitize_config(&json!({"labels": {"api_token": "one"}}), "s").0;
        let b = sanitize_config(&json!({"labels": {"api_token": "two"}}), "s").0;
        let d = diff_configs(&a, &b);
        assert_eq!(d.len(), 1);
        assert!(d[0].hidden);
        assert!(d[0].summary.contains("value hidden"), "{}", d[0].summary);
    }

    #[test]
    fn healthy_overlap_rules() {
        let ev = vec![
            (at(0), Lifecycle::Up),
            (at(30), Lifecycle::Down),
            (at(40), Lifecycle::Up),
        ];
        // Revision at minute 5, next revision at minute 25: 20 minutes up.
        assert_eq!(
            healthy_overlap_secs(&ev, at(5), at(25), at(100), true),
            20 * 60
        );
        // Revision at minute 25 until now: the 25..30 stretch (5 min) or 40..100 (60 min).
        assert_eq!(
            healthy_overlap_secs(&ev, at(25), at(100), at(100), true),
            60 * 60
        );
        // Not running now: the open stretch does not count.
        assert_eq!(
            healthy_overlap_secs(&ev, at(25), at(100), at(100), false),
            5 * 60
        );
        // No events at all.
        assert_eq!(healthy_overlap_secs(&[], at(0), at(100), at(100), true), 0);
    }
}
