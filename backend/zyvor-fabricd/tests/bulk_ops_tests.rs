// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: LicenseRef-Zyvor-Production-2.0

//! Router tests for `POST /api/vms/bulk`.

use axum::{
    body::Body,
    http::{Request, StatusCode},
    Router,
};
use serde_json::{json, Value};
use tower::ServiceExt;

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

async fn mk(app: &Router, name: &str, tags: &[&str]) {
    let (s, _) = call(
        app,
        "POST",
        "/api/vms",
        Some(json!({"name": name, "image": "t.qcow2", "cpus": 1, "memory": 512, "tags": tags})),
    )
    .await;
    assert_eq!(s, StatusCode::CREATED);
}

async fn tags_of(app: &Router, name: &str) -> Vec<String> {
    let (_, v) = call(app, "GET", &format!("/api/vms/{name}"), None).await;
    serde_json::from_value(v["tags"].clone()).unwrap_or_default()
}

#[tokio::test]
async fn rejects_bad_requests() {
    let app = common::create_test_app().await;
    for body in [
        json!({"selector": {}, "action": "stop"}),
        json!({"selector": {"tags": ["x"]}, "action": "explode"}),
        json!({"selector": {"tags": ["x"]}, "action": "tag-add"}),
        json!({"selector": {"tags": ["x"]}, "action": "snapshot", "snapshot_prefix": "-bad"}),
        json!({"selector": {"tags": ["x"]}, "action": "snapshot", "snapshot_prefix": "a b"}),
        json!({"selector": {"tags": ["x"]}, "action": "stop", "max_vms": 0}),
        json!({"selector": {"tags": ["x"]}, "action": "stop", "plan_hash": "zz"}),
        json!({"selector": {"tags": ["x"]}, "action": "stop", "unknown": 1}),
        json!({"selector": {"tags": [""]}, "action": "stop"}),
        // apply without a plan hash
        json!({"selector": {"tags": ["x"]}, "action": "stop", "dry_run": false}),
    ] {
        let (s, _) = call(&app, "POST", "/api/vms/bulk", Some(body.clone())).await;
        assert!(
            s == StatusCode::BAD_REQUEST || s == StatusCode::UNPROCESSABLE_ENTITY,
            "{body} -> {s}"
        );
    }
}

#[tokio::test]
async fn dry_run_is_default_and_changes_nothing() {
    let app = common::create_test_app().await;
    mk(&app, "bk-a1", &["bk-grp"]).await;
    mk(&app, "bk-a2", &["bk-grp"]).await;
    mk(&app, "bk-other", &[]).await;
    let (s, v) = call(
        &app,
        "POST",
        "/api/vms/bulk",
        Some(json!({"selector": {"tags": ["bk-grp"]}, "action": "tag-add", "tag": "canary"})),
    )
    .await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(v["dry_run"], true);
    assert_eq!(v["can_apply"], true);
    assert_eq!(v["plan"]["summary"]["matched"], 2);
    assert_eq!(v["plan"]["items"][0]["vm"], "bk-a1");
    assert_eq!(v["plan"]["items"][0]["will"], "tag-add");
    assert_eq!(v["plan"]["plan_hash"].as_str().unwrap().len(), 64);
    assert!(!tags_of(&app, "bk-a1").await.contains(&"canary".to_string()));
}

#[tokio::test]
async fn apply_requires_matching_hash_then_tags_and_audits() {
    let app = common::create_test_app().await;
    mk(&app, "bka-1", &["bka-grp"]).await;
    mk(&app, "bka-2", &["bka-grp", "canary"]).await;
    let plan_req = json!({"selector": {"tags": ["bka-grp"]}, "action": "tag-add", "tag": "canary"});
    let (_, v) = call(&app, "POST", "/api/vms/bulk", Some(plan_req.clone())).await;
    let hash = v["plan"]["plan_hash"].as_str().unwrap().to_string();
    assert_eq!(v["plan"]["summary"]["skipped"], 1);

    let mut apply = plan_req.clone();
    apply["dry_run"] = json!(false);
    apply["plan_hash"] = json!("0".repeat(64));
    let (s, v) = call(&app, "POST", "/api/vms/bulk", Some(apply.clone())).await;
    assert_eq!(s, StatusCode::CONFLICT);
    assert_eq!(v["plan"]["plan_hash"], hash.as_str());
    assert!(!tags_of(&app, "bka-1").await.contains(&"canary".to_string()));

    apply["plan_hash"] = json!(hash);
    let (s, v) = call(&app, "POST", "/api/vms/bulk", Some(apply.clone())).await;
    assert_eq!(s, StatusCode::OK, "{v}");
    assert_eq!(v["summary"]["ok"], 1);
    assert_eq!(v["summary"]["skipped"], 1);
    assert!(tags_of(&app, "bka-1").await.contains(&"canary".to_string()));

    // The state changed, so the same hash is now stale.
    let (s, _) = call(&app, "POST", "/api/vms/bulk", Some(apply)).await;
    assert_eq!(s, StatusCode::CONFLICT);

    // tag-remove round trip
    let rm = json!({"selector": {"names": ["bka-1"]}, "action": "tag-remove", "tag": "canary"});
    let (_, v) = call(&app, "POST", "/api/vms/bulk", Some(rm.clone())).await;
    let mut rm_apply = rm;
    rm_apply["dry_run"] = json!(false);
    rm_apply["plan_hash"] = v["plan"]["plan_hash"].clone();
    let (s, _) = call(&app, "POST", "/api/vms/bulk", Some(rm_apply)).await;
    assert_eq!(s, StatusCode::OK);
    assert!(!tags_of(&app, "bka-1").await.contains(&"canary".to_string()));

    // audit entries exist for the per-VM action and the run
    let (_, a) = call(&app, "GET", "/api/audit/export?action=BULK_TAG_ADD", None).await;
    assert!(a.to_string().contains("bka-1") || a.is_null());
}

#[tokio::test]
async fn cap_blocks_apply_and_lists_over_cap() {
    let app = common::create_test_app().await;
    for i in 0..3 {
        mk(&app, &format!("bkc-{i}"), &["bkc-grp"]).await;
    }
    let req =
        json!({"selector": {"tags": ["bkc-grp"]}, "action": "tag-add", "tag": "x", "max_vms": 2});
    let (_, v) = call(&app, "POST", "/api/vms/bulk", Some(req.clone())).await;
    assert_eq!(v["plan"]["over_cap"], true);
    assert_eq!(v["can_apply"], false);
    let mut apply = req;
    apply["dry_run"] = json!(false);
    apply["plan_hash"] = v["plan"]["plan_hash"].clone();
    let (s, _) = call(&app, "POST", "/api/vms/bulk", Some(apply)).await;
    assert_eq!(s, StatusCode::BAD_REQUEST);
    assert!(!tags_of(&app, "bkc-0").await.contains(&"x".to_string()));
}

#[tokio::test]
async fn stop_skips_stopped_vms_and_snapshot_failure_aborts_unless_continue() {
    let app = common::create_test_app().await;
    mk(&app, "bks-1", &["bks-grp"]).await;
    mk(&app, "bks-2", &["bks-grp"]).await;
    let (_, v) = call(
        &app,
        "POST",
        "/api/vms/bulk",
        Some(json!({"selector": {"tags": ["bks-grp"]}, "action": "stop"})),
    )
    .await;
    assert_eq!(v["plan"]["summary"]["actionable"], 0);
    assert_eq!(v["can_apply"], false);
    assert_eq!(v["plan"]["items"][0]["reason"], "not_running");

    // VMs created in tests have no disk, so snapshots fail per VM.
    let snap =
        json!({"selector": {"tags": ["bks-grp"]}, "action": "snapshot", "snapshot_prefix": "pre"});
    let (_, v) = call(&app, "POST", "/api/vms/bulk", Some(snap.clone())).await;
    let hash = v["plan"]["plan_hash"].clone();
    let mut apply = snap.clone();
    apply["dry_run"] = json!(false);
    apply["plan_hash"] = hash.clone();
    let (s, v) = call(&app, "POST", "/api/vms/bulk", Some(apply.clone())).await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(v["summary"]["failed"], 1);
    assert_eq!(v["summary"]["not_attempted"], 1);
    assert_eq!(v["results"][1]["status"], "not_attempted");

    apply["continue_on_error"] = json!(true);
    let (_, v) = call(&app, "POST", "/api/vms/bulk", Some(apply)).await;
    assert_eq!(v["summary"]["failed"], 2);
    assert_eq!(v["summary"]["not_attempted"], 0);
}
