// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: LicenseRef-Zyvor-Production-2.0

//! Configuration snapshot history and diff (Time Machine, light).
//!
//! `diff_snapshots` is a pure structural diff of two snapshot documents (the
//! JSON shape produced by `GET /api/config/snapshot`). The handlers store
//! snapshots as state-store entities and expose list / diff routes. Nothing
//! here touches VMs or networking; it only records and compares.

use axum::{
    extract::{Query, State},
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::sync::Arc;

use crate::api_error::json_error;
use crate::server::AppState;
use security::{RequireRead, RequireWrite};

/// State-store directory holding stored snapshots.
pub const SNAPSHOT_DIR: &str = "config_snapshots";
/// Maximum number of stored snapshots; the oldest are dropped beyond this.
pub const MAX_SNAPSHOTS: usize = 50;
/// Replacement for secret values in diff output.
pub const REDACTED: &str = "***REDACTED***";

/// (section name in the snapshot, identifying key fields tried in order)
const SECTIONS: [(&str, &[&str]); 3] = [
    ("vms", &["name", "id"]),
    ("network_policies", &["id", "name"]),
    ("storage_pools", &["name", "id"]),
];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ChangeKind {
    Added,
    Removed,
    Changed,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Change {
    /// Snapshot section: `vms`, `network_policies` or `storage_pools`.
    pub section: String,
    /// VM name, policy id or pool name.
    pub key: String,
    pub kind: ChangeKind,
    /// Dotted field path for `changed`; absent for `added` / `removed`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub field: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub old: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub new: Option<Value>,
}

/// True when a field name looks like it holds a secret.
pub fn is_secret_key(key: &str) -> bool {
    let k = key.to_ascii_lowercase();
    ["password", "secret", "token", "key", "credential"]
        .iter()
        .any(|needle| k.contains(needle))
}

/// Copy of `v` with every value under a secret-looking key replaced.
pub fn redact(v: &Value) -> Value {
    match v {
        Value::Object(m) => Value::Object(
            m.iter()
                .map(|(k, val)| {
                    if is_secret_key(k) {
                        (k.clone(), Value::String(REDACTED.into()))
                    } else {
                        (k.clone(), redact(val))
                    }
                })
                .collect(),
        ),
        Value::Array(a) => Value::Array(a.iter().map(redact).collect()),
        other => other.clone(),
    }
}

fn entity_key(item: &Value, key_fields: &[&str]) -> Option<String> {
    for f in key_fields {
        match item.get(*f) {
            Some(Value::String(s)) if !s.is_empty() => return Some(s.clone()),
            Some(Value::Number(n)) => return Some(n.to_string()),
            _ => {}
        }
    }
    None
}

/// Index a section by entity key. Accepts an array of objects or an object
/// map (keyed by its own keys). Items without a key are skipped; on duplicate
/// keys the last one wins.
fn index_section<'a>(
    section: Option<&'a Value>,
    key_fields: &[&str],
) -> BTreeMap<String, &'a Value> {
    let mut out = BTreeMap::new();
    match section {
        Some(Value::Array(items)) => {
            for item in items {
                if let Some(k) = entity_key(item, key_fields) {
                    out.insert(k, item);
                }
            }
        }
        Some(Value::Object(map)) => {
            for (k, item) in map {
                out.insert(k.clone(), item);
            }
        }
        _ => {}
    }
    out
}

/// Collect differing leaf paths. Objects are descended into; everything else
/// (scalars, arrays) is compared whole.
fn diff_values(
    path: &str,
    a: &Value,
    b: &Value,
    out: &mut Vec<(String, Option<Value>, Option<Value>)>,
) {
    match (a, b) {
        (Value::Object(x), Value::Object(y)) => {
            let mut keys: Vec<&String> = x.keys().chain(y.keys()).collect();
            keys.sort();
            keys.dedup();
            for k in keys {
                let p = if path.is_empty() {
                    k.clone()
                } else {
                    format!("{path}.{k}")
                };
                match (x.get(k), y.get(k)) {
                    (Some(l), Some(r)) => diff_values(&p, l, r, out),
                    (l, r) => out.push((p, l.cloned(), r.cloned())),
                }
            }
        }
        _ => {
            if a != b {
                out.push((path.to_string(), Some(a.clone()), Some(b.clone())));
            }
        }
    }
}

fn path_is_secret(path: &str) -> bool {
    path.split('.').any(is_secret_key)
}

/// Structural diff of two snapshots, keyed by VM name / policy id / pool name.
///
/// Output is sorted by section (vms, network_policies, storage_pools), then
/// key, then field, so equal inputs always give identical output. Secret
/// values are redacted in the output; a change to a secret is still reported
/// (as redacted-to-redacted) so it is visible that it changed.
pub fn diff_snapshots(a: &Value, b: &Value) -> Vec<Change> {
    let mut changes = Vec::new();
    for (section, key_fields) in SECTIONS {
        let left = index_section(a.get(section), key_fields);
        let right = index_section(b.get(section), key_fields);
        let mut keys: Vec<&String> = left.keys().chain(right.keys()).collect();
        keys.sort();
        keys.dedup();
        for key in keys {
            match (left.get(key), right.get(key)) {
                (Some(l), None) => changes.push(Change {
                    section: section.into(),
                    key: key.clone(),
                    kind: ChangeKind::Removed,
                    field: None,
                    old: Some(redact(l)),
                    new: None,
                }),
                (None, Some(r)) => changes.push(Change {
                    section: section.into(),
                    key: key.clone(),
                    kind: ChangeKind::Added,
                    field: None,
                    old: None,
                    new: Some(redact(r)),
                }),
                (Some(l), Some(r)) => {
                    let mut diffs = Vec::new();
                    diff_values("", l, r, &mut diffs);
                    for (field, old, new) in diffs {
                        let secret = path_is_secret(&field);
                        let cleanse = |v: Option<Value>| {
                            v.map(|v| {
                                if secret {
                                    Value::String(REDACTED.into())
                                } else {
                                    redact(&v)
                                }
                            })
                        };
                        changes.push(Change {
                            section: section.into(),
                            key: key.clone(),
                            kind: ChangeKind::Changed,
                            field: Some(field),
                            old: cleanse(old),
                            new: cleanse(new),
                        });
                    }
                }
                (None, None) => {}
            }
        }
    }
    changes
}

// ============================================================================
// Storage + handlers
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoredSnapshot {
    pub id: String,
    pub created_at: String,
    pub created_by: String,
    pub snapshot: Value,
}

#[derive(Debug, Serialize)]
pub struct SnapshotMeta {
    pub id: String,
    pub created_at: String,
    pub created_by: String,
    pub vms: usize,
    pub network_policies: usize,
    pub storage_pools: usize,
}

fn section_len(v: &Value, name: &str) -> usize {
    match v.get(name) {
        Some(Value::Array(a)) => a.len(),
        Some(Value::Object(m)) => m.len(),
        _ => 0,
    }
}

impl From<&StoredSnapshot> for SnapshotMeta {
    fn from(s: &StoredSnapshot) -> Self {
        SnapshotMeta {
            id: s.id.clone(),
            created_at: s.created_at.clone(),
            created_by: s.created_by.clone(),
            vms: section_len(&s.snapshot, "vms"),
            network_policies: section_len(&s.snapshot, "network_policies"),
            storage_pools: section_len(&s.snapshot, "storage_pools"),
        }
    }
}

/// Stored snapshots, oldest first (ties broken by id).
fn load_all(state: &AppState) -> Result<Vec<StoredSnapshot>, anyhow::Error> {
    let mut all: Vec<StoredSnapshot> = state.store.list_entities(SNAPSHOT_DIR)?;
    all.sort_by(|a, b| (&a.created_at, &a.id).cmp(&(&b.created_at, &b.id)));
    Ok(all)
}

/// POST /api/config/snapshots — store the current configuration snapshot.
pub async fn create_snapshot(
    RequireWrite(claims): RequireWrite,
    State(state): State<Arc<AppState>>,
) -> impl IntoResponse {
    let live = crate::api::config_snapshot::collect_snapshot(&state).await;
    let now = Utc::now();
    let stored = StoredSnapshot {
        id: format!(
            "snap-{}-{}",
            now.format("%Y%m%dT%H%M%S%3f"),
            &uuid::Uuid::new_v4().simple().to_string()[..8]
        ),
        created_at: now.to_rfc3339(),
        created_by: claims.sub.clone(),
        snapshot: live,
    };
    if let Err(e) = state.store.save_entity(SNAPSHOT_DIR, &stored.id, &stored) {
        tracing::error!("failed to store config snapshot: {e}");
        crate::api::ai::audit(
            &state,
            &claims.sub,
            "config.snapshot.create",
            &stored.id,
            "failed",
        );
        return json_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "Failed to store snapshot",
        )
        .into_response();
    }
    // Cap history: drop the oldest beyond MAX_SNAPSHOTS.
    if let Ok(all) = load_all(&state) {
        let excess = all.len().saturating_sub(MAX_SNAPSHOTS);
        for old in all.iter().take(excess) {
            if let Err(e) = state.store.delete_entity(SNAPSHOT_DIR, &old.id) {
                tracing::warn!("failed to prune snapshot {}: {e}", old.id);
            }
        }
    }
    crate::api::ai::audit(
        &state,
        &claims.sub,
        "config.snapshot.create",
        &stored.id,
        "success",
    );
    (StatusCode::CREATED, Json(SnapshotMeta::from(&stored))).into_response()
}

/// GET /api/config/snapshots — metadata only, newest first.
pub async fn list_snapshots(
    RequireRead(_claims): RequireRead,
    State(state): State<Arc<AppState>>,
) -> impl IntoResponse {
    match load_all(&state) {
        Ok(all) => {
            let metas: Vec<SnapshotMeta> = all.iter().rev().map(SnapshotMeta::from).collect();
            Json(json!({ "snapshots": metas, "max": MAX_SNAPSHOTS })).into_response()
        }
        Err(e) => {
            tracing::error!("failed to list config snapshots: {e}");
            json_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                "Failed to list snapshots",
            )
            .into_response()
        }
    }
}

#[derive(Debug, Deserialize)]
pub struct DiffQuery {
    pub from: Option<String>,
    pub to: Option<String>,
}

async fn resolve(state: &AppState, which: &str) -> Result<Value, (StatusCode, Json<Value>)> {
    if which == "current" {
        return Ok(crate::api::config_snapshot::collect_snapshot(state).await);
    }
    match state
        .store
        .get_entity::<StoredSnapshot>(SNAPSHOT_DIR, which)
    {
        Ok(Some(s)) => Ok(s.snapshot),
        Ok(None) => Err(json_error(
            StatusCode::NOT_FOUND,
            format!("Snapshot not found: {which}"),
        )),
        Err(_) => Err(json_error(StatusCode::BAD_REQUEST, "Invalid snapshot id")),
    }
}

/// GET /api/config/diff?from=<id|current>&to=<id|current>
pub async fn diff_endpoint(
    RequireRead(_claims): RequireRead,
    State(state): State<Arc<AppState>>,
    Query(q): Query<DiffQuery>,
) -> impl IntoResponse {
    let (Some(from), Some(to)) = (q.from, q.to) else {
        return json_error(
            StatusCode::BAD_REQUEST,
            "Both `from` and `to` are required (snapshot id or `current`)",
        )
        .into_response();
    };
    let a = match resolve(&state, &from).await {
        Ok(v) => v,
        Err(e) => return e.into_response(),
    };
    let b = match resolve(&state, &to).await {
        Ok(v) => v,
        Err(e) => return e.into_response(),
    };
    let changes = diff_snapshots(&a, &b);
    Json(json!({ "from": from, "to": to, "change_count": changes.len(), "changes": changes }))
        .into_response()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn snap(vms: Value, pols: Value, pools: Value) -> Value {
        json!({"version":"1","vms":vms,"network_policies":pols,"storage_pools":pools})
    }

    #[test]
    fn empty_and_identical_have_no_changes() {
        assert!(diff_snapshots(&json!({}), &json!({})).is_empty());
        let s = snap(
            json!([{"name":"a","cpus":2}]),
            json!([{"id":"p1"}]),
            json!([{"name":"x"}]),
        );
        assert!(diff_snapshots(&s, &s).is_empty());
        // exported_at / event counts are not diffed
        let mut t = s.clone();
        t["exported_at"] = json!("later");
        t["recent_events_count"] = json!(9);
        assert!(diff_snapshots(&s, &t).is_empty());
    }

    #[test]
    fn empty_to_populated_is_all_added() {
        let b = snap(
            json!([{"name":"a"}]),
            json!([{"id":"p1"}]),
            json!([{"name":"x"}]),
        );
        let c = diff_snapshots(&json!({}), &b);
        assert_eq!(c.len(), 3);
        assert!(c
            .iter()
            .all(|c| c.kind == ChangeKind::Added && c.old.is_none() && c.new.is_some()));
        assert_eq!(
            c.iter()
                .map(|c| (c.section.as_str(), c.key.as_str()))
                .collect::<Vec<_>>(),
            vec![
                ("vms", "a"),
                ("network_policies", "p1"),
                ("storage_pools", "x")
            ]
        );
    }

    #[test]
    fn added_and_removed() {
        let a = snap(json!([{"name":"old","cpus":1}]), json!([]), json!([]));
        let b = snap(json!([{"name":"new","cpus":2}]), json!([]), json!([]));
        let c = diff_snapshots(&a, &b);
        assert_eq!(c.len(), 2);
        assert_eq!((c[0].key.as_str(), c[0].kind), ("new", ChangeKind::Added));
        assert_eq!(c[0].new, Some(json!({"name":"new","cpus":2})));
        assert_eq!((c[1].key.as_str(), c[1].kind), ("old", ChangeKind::Removed));
        assert_eq!(c[1].old, Some(json!({"name":"old","cpus":1})));
    }

    #[test]
    fn changed_fields_with_old_and_new() {
        let a = snap(
            json!([{"name":"a","cpus":2,"mem":{"mb":512,"balloon":true},"gone":1}]),
            json!([]),
            json!([]),
        );
        let b = snap(
            json!([{"name":"a","cpus":4,"mem":{"mb":1024,"balloon":true},"fresh":"y"}]),
            json!([]),
            json!([]),
        );
        let c = diff_snapshots(&a, &b);
        let got: Vec<_> = c
            .iter()
            .map(|c| (c.field.clone().unwrap(), c.old.clone(), c.new.clone()))
            .collect();
        assert_eq!(
            got,
            vec![
                ("cpus".to_string(), Some(json!(2)), Some(json!(4))),
                ("fresh".to_string(), None, Some(json!("y"))),
                ("gone".to_string(), Some(json!(1)), None),
                ("mem.mb".to_string(), Some(json!(512)), Some(json!(1024))),
            ]
        );
        assert!(c
            .iter()
            .all(|c| c.kind == ChangeKind::Changed && c.section == "vms" && c.key == "a"));
    }

    #[test]
    fn arrays_are_compared_whole() {
        let a = snap(json!([]), json!([{"id":"p","rules":[1,2]}]), json!([]));
        let b = snap(json!([]), json!([{"id":"p","rules":[1,2,3]}]), json!([]));
        let c = diff_snapshots(&a, &b);
        assert_eq!(c.len(), 1);
        assert_eq!(c[0].field.as_deref(), Some("rules"));
        assert_eq!(c[0].old, Some(json!([1, 2])));
        assert_eq!(c[0].new, Some(json!([1, 2, 3])));
    }

    #[test]
    fn input_order_does_not_matter_and_output_is_sorted() {
        let a = snap(
            json!([{"name":"z","c":1},{"name":"b","c":1}]),
            json!([{"id":"q","c":1},{"id":"a","c":1}]),
            json!([{"name":"m","c":1}]),
        );
        let b = snap(
            json!([{"name":"b","c":2},{"name":"z","c":2}]),
            json!([{"id":"a","c":2},{"id":"q","c":2}]),
            json!([{"name":"m","c":2}]),
        );
        let c = diff_snapshots(&a, &b);
        let order: Vec<_> = c
            .iter()
            .map(|c| (c.section.as_str(), c.key.as_str()))
            .collect();
        assert_eq!(
            order,
            vec![
                ("vms", "b"),
                ("vms", "z"),
                ("network_policies", "a"),
                ("network_policies", "q"),
                ("storage_pools", "m")
            ]
        );
        assert_eq!(c, diff_snapshots(&a, &b));
    }

    #[test]
    fn redacts_secrets_everywhere() {
        let a = snap(
            json!([{"name":"a","admin_password":"hunter2","net":{"api_token":"t1"},"ssh_key":"k1"}]),
            json!([{"id":"p","Secret":"s"}]),
            json!([{"name":"x","credentials":{"user":"u","pw":"p"}}]),
        );
        let b = snap(
            json!([{"name":"a","admin_password":"hunter3","net":{"api_token":"t2"},"ssh_key":"k2","extra":{"token":"nt"}}]),
            json!([{"id":"p","Secret":"s2"}]),
            json!([{"name":"x","credentials":{"user":"u","pw":"q"}}]),
        );
        let c = diff_snapshots(&a, &b);
        let text = serde_json::to_string(&c).unwrap();
        for leaked in [
            "hunter2", "hunter3", "\"t1\"", "\"t2\"", "k1", "k2", "\"nt\"", "\"s2\"", "\"p\"",
            "\"q\"",
        ] {
            // "p" appears as policy id key; only check non-id leaks
            if leaked == "\"p\"" {
                continue;
            }
            assert!(!text.contains(leaked), "leaked {leaked}: {text}");
        }
        assert!(c
            .iter()
            .any(|c| c.field.as_deref() == Some("admin_password")
                && c.old == Some(json!(REDACTED))
                && c.new == Some(json!(REDACTED))));
        assert!(c
            .iter()
            .any(|c| c.field.as_deref() == Some("credentials.pw")));
        // changes to non-secret siblings stay visible
        let c2 = diff_snapshots(
            &snap(
                json!([{"name":"a","cpus":1,"password":"x"}]),
                json!([]),
                json!([]),
            ),
            &snap(
                json!([{"name":"a","cpus":2,"password":"x"}]),
                json!([]),
                json!([]),
            ),
        );
        assert_eq!(c2.len(), 1);
        assert_eq!(c2[0].field.as_deref(), Some("cpus"));
    }

    #[test]
    fn added_and_removed_entities_are_redacted() {
        let b = snap(
            json!([{"name":"a","password":"pw","nested":[{"token":"t"}]}]),
            json!([]),
            json!([]),
        );
        let c = diff_snapshots(&json!({}), &b);
        let text = serde_json::to_string(&c).unwrap();
        assert!(!text.contains("\"pw\"") && !text.contains("\"t\""));
        assert!(text.contains(REDACTED));
        let r = diff_snapshots(&b, &json!({}));
        assert!(!serde_json::to_string(&r).unwrap().contains("\"pw\""));
    }

    #[test]
    fn tolerates_odd_shapes() {
        // keyless items skipped, object-map sections, non-array sections
        let a = json!({"vms":[{"cpus":1},{"name":"a"}],"storage_pools":{"p":{"size":1}},"network_policies":"junk"});
        let b =
            json!({"vms":[{"name":"a"}],"storage_pools":{"p":{"size":2}},"network_policies":null});
        let c = diff_snapshots(&a, &b);
        assert_eq!(c.len(), 1);
        assert_eq!(
            (
                c[0].section.as_str(),
                c[0].key.as_str(),
                c[0].field.as_deref()
            ),
            ("storage_pools", "p", Some("size"))
        );
    }

    #[test]
    fn id_fallback_key_for_vms() {
        let a = snap(json!([{"id":"vm-1","cpus":1}]), json!([]), json!([]));
        let b = snap(json!([{"id":"vm-1","cpus":2}]), json!([]), json!([]));
        assert_eq!(diff_snapshots(&a, &b)[0].key, "vm-1");
    }

    #[test]
    fn secret_key_matching() {
        for k in [
            "password",
            "Password",
            "api_key",
            "SECRET",
            "auth_token",
            "credentials",
            "keyfile",
        ] {
            assert!(is_secret_key(k), "{k}");
        }
        for k in ["name", "cpus", "id"] {
            assert!(!is_secret_key(k), "{k}");
        }
    }
}
