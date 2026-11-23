// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: LicenseRef-Zyvor-Production-2.0

//! Router tests for VM configuration history, the diff, last known good and
//! the secret-leak guarantees.

use axum::{
    body::Body,
    http::{Request, StatusCode},
    Router,
};
use chrono::{Duration, Utc};
use serde_json::{json, Value};
use std::sync::{Arc, Mutex};
use tower::ServiceExt;
use zyvor_fabricd::api::events::{VMEvent, VMEventType};
use zyvor_fabricd::config::{AuditConfig, ConfigHistoryConfig};
use zyvor_fabricd::config_history::{capture_at, Captured, Revision};
use zyvor_fabricd::server::AppState;

mod common;

async fn call(app: &Router, method: &str, uri: &str, body: Option<Value>) -> (StatusCode, Value) {
    let b = Request::builder().method(method).uri(uri);
    let r = match body {
        Some(v) => b
            .header("content-type", "application/json")
            .body(Body::from(v.to_string()))
            .unwrap(),
        None => b.body(Body::empty()).unwrap(),
    };
    let resp = app.clone().oneshot(r).await.unwrap();
    let s = resp.status();
    let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX)
        .await
        .unwrap();
    (s, serde_json::from_slice(&bytes).unwrap_or(Value::Null))
}

async fn mk(app: &Router, name: &str, extra: Value) {
    let mut body = json!({"name": name, "image": "t.qcow2", "cpus": 2, "memory": 1024});
    if let (Some(b), Some(e)) = (body.as_object_mut(), extra.as_object()) {
        for (k, v) in e {
            b.insert(k.clone(), v.clone());
        }
    }
    let (s, v) = call(app, "POST", "/api/vms", Some(body)).await;
    assert_eq!(s, StatusCode::CREATED, "{v}");
}

fn revs_of(state: &AppState, vm: &str) -> Vec<Revision> {
    zyvor_fabricd::config_history::load_revisions(&state.store, vm)
}

fn put_vm(state: &AppState, vm: &vm_model::VM) {
    state.store.save_vm(vm).unwrap();
}

fn event(state: &AppState, vm: &str, t: VMEventType, minutes_ago: i64) {
    let e = VMEvent {
        id: uuid::Uuid::new_v4().to_string(),
        event_type: t,
        vm_name: vm.to_string(),
        detail: None,
        timestamp: Utc::now() - Duration::minutes(minutes_ago),
    };
    state.store.save_entity("vm_events", &e.id, &e).unwrap();
}

#[tokio::test]
async fn records_create_and_edits_and_dedupes() {
    let app = common::create_test_app().await;
    mk(&app, "hist-a", json!({})).await;

    let (s, h) = call(&app, "GET", "/api/vms/hist-a/history", None).await;
    assert_eq!(s, StatusCode::OK, "{h}");
    assert_eq!(h["current_rev"], 1);
    assert_eq!(h["revisions"][0]["actor"], "test-admin");
    assert_eq!(h["revisions"][0]["cause"], "CREATE");
    assert_eq!(h["chain"]["status"], "ok");
    assert_eq!(h["revisions"][0]["audit"]["link"], "unchained");

    let tag = json!({"tag": "prod"});
    call(&app, "POST", "/api/vms/hist-a/tags", Some(tag.clone())).await;
    // Same edit again changes nothing, so no new revision.
    call(&app, "POST", "/api/vms/hist-a/tags", Some(tag)).await;
    let (_, h) = call(&app, "GET", "/api/vms/hist-a/history", None).await;
    assert_eq!(h["current_rev"], 2);
    assert_eq!(h["revisions"][0]["cause"], "ADD_TAG");
    assert_eq!(h["revisions"][0]["changed_fields"], 1);

    call(&app, "DELETE", "/api/vms/hist-a/tags/prod", None).await;
    let (_, d) = call(
        &app,
        "GET",
        "/api/vms/hist-a/history/diff?from=1&to=2",
        None,
    )
    .await;
    assert_eq!(d["identical"], false);
    assert_eq!(d["changes"][0]["path"], "tags");
    assert!(d["changes"][0]["summary"]
        .as_str()
        .unwrap()
        .contains("prod"));

    // Back to the original: different revisions, identical configuration.
    let (_, d) = call(
        &app,
        "GET",
        "/api/vms/hist-a/history/diff?from=1&to=3",
        None,
    )
    .await;
    assert_eq!(d["identical"], true);
    assert_eq!(d["identical_content_hash"], true);
    assert_eq!(d["changes"].as_array().unwrap().len(), 0);
    // A revision against itself.
    let (_, d) = call(
        &app,
        "GET",
        "/api/vms/hist-a/history/diff?from=2&to=2",
        None,
    )
    .await;
    assert_eq!(d["identical"], true);

    let (s, r) = call(&app, "GET", "/api/vms/hist-a/history/2", None).await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(r["config"]["tags"][0], "prod");
    assert!(
        r["config"].get("state").is_none(),
        "runtime state must not be recorded"
    );
    assert!(r["config"].get("ip").is_none());
}

#[tokio::test]
async fn bad_and_missing_inputs() {
    let app = common::create_test_app().await;
    mk(&app, "hist-b", json!({})).await;
    for (uri, want) in [
        ("/api/vms/hist-b/history/99", StatusCode::NOT_FOUND),
        ("/api/vms/hist-b/history/0", StatusCode::BAD_REQUEST),
        ("/api/vms/hist-b/history/abc", StatusCode::BAD_REQUEST),
        ("/api/vms/hist-b/history/-1", StatusCode::BAD_REQUEST),
        ("/api/vms/hist-b/history/diff", StatusCode::BAD_REQUEST),
        (
            "/api/vms/hist-b/history/diff?from=x",
            StatusCode::BAD_REQUEST,
        ),
        (
            "/api/vms/hist-b/history/diff?from=1&to=99",
            StatusCode::NOT_FOUND,
        ),
        (
            "/api/vms/hist-b/history/diff?from=99",
            StatusCode::NOT_FOUND,
        ),
        ("/api/vms/hist-b/history?limit=0", StatusCode::BAD_REQUEST),
        (
            "/api/vms/hist-b/history?limit=100000",
            StatusCode::BAD_REQUEST,
        ),
        ("/api/vms/no-such-vm/history", StatusCode::NOT_FOUND),
        (
            "/api/vms/no-such-vm/history/since-good",
            StatusCode::NOT_FOUND,
        ),
        ("/api/vms/-bad/history", StatusCode::BAD_REQUEST),
    ] {
        let (s, _) = call(&app, "GET", uri, None).await;
        assert_eq!(s, want, "{uri}");
    }
    let (s, _) = call(&app, "POST", "/api/vms/hist-b/history/99/mark-good", None).await;
    assert_eq!(s, StatusCode::NOT_FOUND);
    let (s, _) = call(&app, "POST", "/api/vms/hist-b/history/x/mark-good", None).await;
    assert_eq!(s, StatusCode::BAD_REQUEST);
    let r = Request::builder()
        .method("POST")
        .uri("/api/vms/hist-b/history/1/mark-good")
        .header("content-type", "application/json")
        .body(Body::from("{not json"))
        .unwrap();
    let resp = app.clone().oneshot(r).await.unwrap();
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    // A note that is not a string.
    let (s, _) = call(
        &app,
        "POST",
        "/api/vms/hist-b/history/1/mark-good",
        Some(json!({"note": 5})),
    )
    .await;
    assert_eq!(s, StatusCode::BAD_REQUEST);
}

#[derive(Clone)]
struct Buf(Arc<Mutex<Vec<u8>>>);
impl std::io::Write for Buf {
    fn write(&mut self, b: &[u8]) -> std::io::Result<usize> {
        self.0.lock().unwrap().extend_from_slice(b);
        Ok(b.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

#[tokio::test]
async fn secrets_are_never_stored_returned_or_logged() {
    const PW: &str = "s3cr3tPW-9x";
    const TOKEN: &str = "tok_ABC123xyzQ";
    const KEY: &str = "AAAAC3NzaC1lZDI1NTE5AAAAIsecretkeybody";
    const RUN: &str = "mysqladmin -pRunCmdSecret42 status";

    let logs = Arc::new(Mutex::new(Vec::new()));
    let sink = logs.clone();
    let sub = tracing_subscriber::fmt()
        .with_max_level(tracing::Level::TRACE)
        .with_ansi(false)
        .with_writer(move || Buf(sink.clone()))
        .finish();
    let _guard = tracing::subscriber::set_default(sub);

    let (app, state) = common::create_test_app_with_history(
        "secrets",
        ConfigHistoryConfig::default(),
        AuditConfig {
            chain: true,
            ..Default::default()
        },
    )
    .await;
    mk(
        &app,
        "sec-a",
        json!({"labels": {"db_password": PW, "note": format!("login password={PW} ok")}}),
    )
    .await;
    let user_data = format!(
        "#cloud-config\nssh_authorized_keys:\n  - ssh-ed25519 {KEY} me@example.com\nruncmd:\n  - echo {TOKEN} > /etc/t\n  - {RUN}\n"
    );
    call(
        &app,
        "POST",
        "/api/vms/sec-a/cloud-init",
        Some(json!({"instance_id": "i-1", "hostname": "sec-a", "user_data": user_data})),
    )
    .await;
    call(
        &app,
        "POST",
        "/api/vms/sec-a/display",
        Some(json!({"password": PW, "keymap": "en-us"})),
    )
    .await;
    call(
        &app,
        "POST",
        "/api/vms/sec-a/boot",
        Some(json!({"kernel_args": format!("root=/dev/vda token={TOKEN}")})),
    )
    .await;

    let mut texts: Vec<String> = Vec::new();
    let (_, h) = call(&app, "GET", "/api/vms/sec-a/history", None).await;
    let n = h["current_rev"].as_u64().unwrap();
    assert!(n >= 3, "expected several revisions, got {h}");
    texts.push(h.to_string());
    for r in 1..=n {
        let (_, v) = call(&app, "GET", &format!("/api/vms/sec-a/history/{r}"), None).await;
        texts.push(v.to_string());
        let (_, d) = call(
            &app,
            "GET",
            &format!("/api/vms/sec-a/history/diff?from=1&to={r}"),
            None,
        )
        .await;
        texts.push(d.to_string());
    }
    let (_, sg) = call(&app, "GET", "/api/vms/sec-a/history/since-good", None).await;
    texts.push(sg.to_string());
    // What is on disk: revisions and every audit event.
    for r in revs_of(&state, "sec-a") {
        texts.push(serde_json::to_string(&r).unwrap());
    }
    let audit: Vec<Value> = state.store.list_entities("audit_logs").unwrap();
    assert!(audit.iter().any(|e| e["action"] == "VM_CONFIG_REVISION"));
    texts.push(serde_json::to_string(&audit).unwrap());
    texts.push(String::from_utf8_lossy(&logs.lock().unwrap()).to_string());

    for secret in [PW, TOKEN, KEY, "RunCmdSecret42"] {
        for t in &texts {
            assert!(!t.contains(secret), "leaked {secret:?} in: {t}");
        }
    }
    // The change is still visible as a hidden change.
    let last = revs_of(&state, "sec-a").pop().unwrap();
    assert!(last.config.to_string().contains("[redacted:"));
    assert_eq!(last.config["display"]["keymap"], "en-us");
}

#[tokio::test]
async fn timeline_auto_good_ranking_and_mark_good() {
    let (app, state) = common::create_test_app_with_history(
        "timeline",
        ConfigHistoryConfig::default(),
        AuditConfig::default(),
    )
    .await;
    let name = "tl-a";
    let mut vm = vm_model::VM::new(name.to_string(), "img".into(), 4, 4096);
    vm.state = vm_model::VMState::Running;
    put_vm(&state, &vm);

    // Before anything is recorded: no known good.
    event(&state, name, VMEventType::Started, 90);
    let t = |m: i64| Utc::now() - Duration::minutes(m);
    assert_eq!(
        capture_at(&state, name, "alice", "CREATE", t(60)),
        Captured::Recorded(1)
    );
    vm.memory = 2048;
    put_vm(&state, &vm);
    assert_eq!(
        capture_at(&state, name, "bob", "resize memory", t(40)),
        Captured::Recorded(2)
    );
    event(&state, name, VMEventType::Error, 39);
    vm.cpus = 2;
    vm.tags = Some(vec!["canary".into()]);
    put_vm(&state, &vm);
    assert_eq!(
        capture_at(&state, name, "carol", "resize cpus", t(20)),
        Captured::Recorded(3)
    );
    event(&state, name, VMEventType::Error, 10);
    assert_eq!(
        capture_at(&state, name, "carol", "again", t(19)),
        Captured::Unchanged
    );

    let (s, h) = call(&app, "GET", &format!("/api/vms/{name}/history"), None).await;
    assert_eq!(s, StatusCode::OK, "{h}");
    assert_eq!(h["last_good_rev"], 1, "{h}");
    assert_eq!(h["revisions"][2]["good"]["source"], "auto");
    assert!(h["revisions"][0]["good"].is_null());
    assert_eq!(h["revisions"][0]["high_risk_changes"], 1);

    let (_, sg) = call(
        &app,
        "GET",
        &format!("/api/vms/{name}/history/since-good"),
        None,
    )
    .await;
    assert_eq!(sg["status"], "changed", "{sg}");
    assert_eq!(sg["good_rev"], 1);
    assert_eq!(sg["current_rev"], 3);
    let culprits = sg["culprits"].as_array().unwrap();
    assert_eq!(culprits.len(), 3, "{sg}");
    // Both high-risk shrinks outrank the tag; the newer change is first.
    assert_eq!(culprits[0]["path"], "cpus");
    assert_eq!(culprits[0]["changed_in"][0]["actor"], "carol");
    assert_eq!(culprits[1]["path"], "memory");
    assert_eq!(culprits[1]["changed_in"][0]["actor"], "bob");
    assert_eq!(culprits[2]["path"], "tags");
    let summary = sg["summary"].as_str().unwrap();
    assert!(
        summary.contains("since revision 1") && summary.contains("2 high-risk"),
        "{summary}"
    );
    assert!(summary.contains("Most likely culprit"), "{summary}");

    // An operator marks revision 2 good: only the cpu and tag changes remain.
    let (s, m) = call(
        &app,
        "POST",
        &format!("/api/vms/{name}/history/2/mark-good"),
        Some(json!({"note": "verified by hand password=hunter2"})),
    )
    .await;
    assert_eq!(s, StatusCode::OK, "{m}");
    assert_eq!(m["good"]["source"], "operator");
    assert_eq!(m["good"]["by"], "test-admin");
    assert!(!m.to_string().contains("hunter2"));
    let (_, sg) = call(
        &app,
        "GET",
        &format!("/api/vms/{name}/history/since-good"),
        None,
    )
    .await;
    assert_eq!(sg["good_rev"], 2);
    assert_eq!(sg["culprits"].as_array().unwrap().len(), 2);

    // Marking the newest revision good: nothing changed since.
    call(
        &app,
        "POST",
        &format!("/api/vms/{name}/history/3/mark-good"),
        None,
    )
    .await;
    let (_, sg) = call(
        &app,
        "GET",
        &format!("/api/vms/{name}/history/since-good"),
        None,
    )
    .await;
    assert_eq!(sg["status"], "unchanged");

    // The change to the VM outside the hooked handlers is observed on read.
    vm.memory = 8192;
    put_vm(&state, &vm);
    let (_, h) = call(&app, "GET", &format!("/api/vms/{name}/history"), None).await;
    assert_eq!(h["current_rev"], 4);
    assert_eq!(h["revisions"][0]["actor"], "system:observed");
    let marks = state
        .store
        .list_entities::<Value>("audit_logs")
        .unwrap()
        .iter()
        .filter(|e| e["action"] == "VM_CONFIG_MARK_GOOD")
        .count();
    assert_eq!(marks, 2);
}

#[tokio::test]
async fn no_known_good_is_explained() {
    let app = common::create_test_app().await;
    mk(&app, "ng-a", json!({})).await;
    let (s, sg) = call(&app, "GET", "/api/vms/ng-a/history/since-good", None).await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(sg["status"], "no_known_good");
    assert!(sg["message"].as_str().unwrap().contains("mark-good"));
}

#[tokio::test]
async fn retention_cap_evicts_oldest_but_keeps_newest_and_good() {
    let (app, state) = common::create_test_app_with_history(
        "cap",
        ConfigHistoryConfig {
            max_revisions: 5,
            good_after_minutes: 15,
        },
        AuditConfig::default(),
    )
    .await;
    let name = "cap-a";
    let mut vm = vm_model::VM::new(name.to_string(), "img".into(), 1, 512);
    put_vm(&state, &vm);
    for i in 1..=5u32 {
        vm.cpus = i;
        put_vm(&state, &vm);
        capture_at(
            &state,
            name,
            "a",
            "edit",
            Utc::now() - Duration::minutes(100 - i as i64),
        );
    }
    let (s, _) = call(
        &app,
        "POST",
        &format!("/api/vms/{name}/history/2/mark-good"),
        None,
    )
    .await;
    assert_eq!(s, StatusCode::OK);
    for i in 6..=12u32 {
        vm.cpus = i;
        put_vm(&state, &vm);
        capture_at(
            &state,
            name,
            "a",
            "edit",
            Utc::now() - Duration::minutes(100 - i as i64),
        );
    }
    let revs = revs_of(&state, name);
    let nums: Vec<u64> = revs.iter().map(|r| r.rev).collect();
    assert_eq!(nums.len(), 5, "{nums:?}");
    assert!(nums.contains(&12), "newest kept: {nums:?}");
    assert!(nums.contains(&2), "last known good is pinned: {nums:?}");
    assert!(!nums.contains(&1), "{nums:?}");

    let (_, h) = call(&app, "GET", &format!("/api/vms/{name}/history"), None).await;
    assert_eq!(h["retention"]["kept"], 5);
    assert_eq!(h["retention"]["max_revisions"], 5);
    assert_eq!(h["retention"]["evicted_total"], 7);
    assert_eq!(
        h["chain"]["status"], "ok",
        "eviction must not look like tampering: {h}"
    );
    let (s, _) = call(&app, "GET", &format!("/api/vms/{name}/history/1"), None).await;
    assert_eq!(s, StatusCode::NOT_FOUND);
    let (_, sg) = call(
        &app,
        "GET",
        &format!("/api/vms/{name}/history/since-good"),
        None,
    )
    .await;
    assert_eq!(sg["good_rev"], 2);
}

#[tokio::test]
async fn tampering_with_a_revision_is_detected() {
    let (app, state) = common::create_test_app_with_history(
        "tamper",
        ConfigHistoryConfig::default(),
        AuditConfig::default(),
    )
    .await;
    let name = "tp-a";
    let mut vm = vm_model::VM::new(name.to_string(), "img".into(), 1, 512);
    put_vm(&state, &vm);
    for i in 1..=4u32 {
        vm.cpus = i;
        put_vm(&state, &vm);
        capture_at(
            &state,
            name,
            "a",
            "edit",
            Utc::now() - Duration::minutes(50 - i as i64),
        );
    }
    let (_, h) = call(&app, "GET", &format!("/api/vms/{name}/history"), None).await;
    assert_eq!(h["chain"]["status"], "ok");

    // Edit the stored configuration of revision 2.
    let dir = format!("vm_config_history/{name}");
    let mut r2: Revision = state
        .store
        .get_entity(&dir, "r0000000002")
        .unwrap()
        .unwrap();
    r2.config["cpus"] = json!(64);
    state.store.save_entity(&dir, "r0000000002", &r2).unwrap();
    let (_, h) = call(&app, "GET", &format!("/api/vms/{name}/history"), None).await;
    assert_eq!(h["chain"]["status"], "broken");
    assert_eq!(h["chain"]["first_break"]["rev"], 2);
    assert_eq!(h["chain"]["first_break"]["reason"], "content_hash_mismatch");

    // Remove a middle revision instead.
    r2.config["cpus"] = json!(2);
    state.store.save_entity(&dir, "r0000000002", &r2).unwrap();
    state.store.delete_entity(&dir, "r0000000003").unwrap();
    let (_, h) = call(&app, "GET", &format!("/api/vms/{name}/history"), None).await;
    assert_eq!(h["chain"]["status"], "broken");
    assert_eq!(h["chain"]["first_break"]["reason"], "revision_missing");
}

#[tokio::test]
async fn hostile_configuration_is_bounded_and_clean() {
    let (app, state) = common::create_test_app_with_history(
        "hostile",
        ConfigHistoryConfig::default(),
        AuditConfig::default(),
    )
    .await;
    let name = "ho-a";
    let mut vm = vm_model::VM::new(name.to_string(), "img".into(), 1, 512);
    let mut labels = std::collections::HashMap::new();
    labels.insert("huge".to_string(), "A".repeat(200_000));
    labels.insert(
        "ctl".to_string(),
        "x\u{0}y\u{1b}[2J\nz\u{202e}w".to_string(),
    );
    for i in 0..2000 {
        labels.insert(format!("k{i}"), format!("v{i}"));
    }
    labels.insert("K".repeat(500), "long key".to_string());
    vm.labels = Some(labels);
    vm.tags = Some((0..5000).map(|i| format!("t{i}")).collect());
    put_vm(&state, &vm);
    assert_eq!(
        capture_at(&state, name, "a\nb\u{1b}[0m", "c\u{0}d", Utc::now()),
        Captured::Recorded(1)
    );

    let (s, r) = call(&app, "GET", &format!("/api/vms/{name}/history/1"), None).await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(r["truncated"], true);
    assert!(r.to_string().len() < 200_000, "stored revision is bounded");
    assert!(!r["actor"].as_str().unwrap().chars().any(|c| c.is_control()));
    assert!(!r["cause"].as_str().unwrap().chars().any(|c| c.is_control()));
    let ctl = r["config"]["labels"]["ctl"].as_str().unwrap();
    assert!(
        !ctl.chars().any(|c| c.is_control() || c == '\u{202e}'),
        "{ctl:?}"
    );
    let (_, h) = call(&app, "GET", &format!("/api/vms/{name}/history"), None).await;
    assert_eq!(h["chain"]["status"], "ok");
}

#[tokio::test]
async fn revision_is_chained_to_the_audit_log() {
    let (app, state) = common::create_test_app_with_history(
        "chain",
        ConfigHistoryConfig::default(),
        AuditConfig {
            chain: true,
            ..Default::default()
        },
    )
    .await;
    mk(&app, "ch-a", json!({})).await;
    call(
        &app,
        "POST",
        "/api/vms/ch-a/tags",
        Some(json!({"tag": "x"})),
    )
    .await;
    let (_, h) = call(&app, "GET", "/api/vms/ch-a/history", None).await;
    let rows = h["revisions"].as_array().unwrap();
    assert_eq!(rows.len(), 2);
    for r in rows {
        assert_eq!(r["audit"]["link"], "ok", "{r}");
        assert!(r["audit"]["seq"].as_u64().unwrap() >= 1);
        assert_eq!(r["audit"]["hash"].as_str().unwrap().len(), 64);
    }
    // The audit event it points at exists and is the revision event.
    let id = rows[0]["audit"]["id"].as_str().unwrap();
    let ev: Value = state.store.get_entity("audit_logs", id).unwrap().unwrap();
    assert_eq!(ev["action"], "VM_CONFIG_REVISION");
    assert_eq!(ev["resource_name"], "vm/ch-a");
    assert!(ev["details"].as_str().unwrap().contains("rev=2"));
    let (s, v) = call(&app, "GET", "/api/audit/verify", None).await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(v["status"], "ok", "{v}");
}

#[tokio::test]
async fn read_only_roles_can_read_but_not_mark_good() {
    let app = common::create_test_app_with_role(security::Role::Viewer).await;
    let (s, _) = call(&app, "GET", "/api/vms/nobody/history", None).await;
    assert_eq!(s, StatusCode::NOT_FOUND);
    let (s, _) = call(&app, "POST", "/api/vms/nobody/history/1/mark-good", None).await;
    assert_eq!(s, StatusCode::FORBIDDEN);
}
