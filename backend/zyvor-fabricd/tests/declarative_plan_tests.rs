// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: LicenseRef-Zyvor-Production-2.0

//! Router-level tests for the declarative dry-run / plan-hash behaviour.
//! The test app shares one on-disk store per process, so every test uses
//! its own VM names.

use axum::{
    body::Body,
    http::{Request, StatusCode},
    Router,
};
use serde_json::{json, Value};
use tower::ServiceExt;

#[allow(dead_code)]
mod common;

async fn call(app: &Router, method: &str, uri: &str, body: Option<Value>) -> (StatusCode, Value) {
    let mut b = Request::builder().method(method).uri(uri);
    let req = match body {
        Some(v) => {
            b = b.header("content-type", "application/json");
            b.body(Body::from(v.to_string())).unwrap()
        }
        None => b.body(Body::empty()).unwrap(),
    };
    let res = app.clone().oneshot(req).await.unwrap();
    let status = res.status();
    let bytes = axum::body::to_bytes(res.into_body(), usize::MAX)
        .await
        .unwrap();
    let v = serde_json::from_slice(&bytes).unwrap_or(Value::Null);
    (status, v)
}

fn spec(name: &str) -> Value {
    json!({"name": name, "image": "ubuntu-22.04",
           "resources": {"cpus": 2, "memory": "2G", "disk": "20G"}})
}

#[tokio::test]
async fn dry_run_creates_nothing_then_real_apply_creates() {
    let app = common::create_test_app().await;
    let n = "plan-dry-new";

    let (st, plan) = call(&app, "POST", "/api/vms/apply?dry_run=true", Some(spec(n))).await;
    assert_eq!(st, StatusCode::OK);
    assert_eq!(plan["entries"][0]["action"], "create");
    assert_eq!(plan["has_changes"], true);
    assert_eq!(plan["plan_hash"].as_str().unwrap().len(), 64);

    // store unchanged: the VM does not exist
    let (st, _) = call(&app, "GET", &format!("/api/vms/{n}/spec"), None).await;
    assert_eq!(st, StatusCode::NOT_FOUND);

    // real apply with no new params behaves as before
    let (st, res) = call(&app, "POST", "/api/vms/apply", Some(spec(n))).await;
    assert_eq!(st, StatusCode::OK);
    assert_eq!(res["vm_name"], n);
    assert_eq!(res["created"], true);

    let (st, _) = call(&app, "GET", &format!("/api/vms/{n}/spec"), None).await;
    assert_eq!(st, StatusCode::OK);
}

#[tokio::test]
async fn dry_run_on_existing_vm_does_not_mutate() {
    let app = common::create_test_app().await;
    let n = "plan-dry-existing";
    let (st, _) = call(&app, "POST", "/api/vms/apply", Some(spec(n))).await;
    assert_eq!(st, StatusCode::OK);

    let mut changed = spec(n);
    changed["start_options"] = json!({"kvm": true});
    changed["autoscale"] = json!({"cpu_threshold": 0.9});
    let (st, plan) = call(&app, "POST", "/api/vms/apply?dry_run=true", Some(changed)).await;
    assert_eq!(st, StatusCode::OK);
    assert_eq!(plan["entries"][0]["action"], "update");
    let fields: Vec<_> = plan["entries"][0]["changes"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| c["field"].as_str().unwrap().to_string())
        .collect();
    assert!(fields.contains(&"start_options".to_string()));
    assert!(fields.contains(&"autoscale".to_string()));

    let (_, exported) = call(&app, "GET", &format!("/api/vms/{n}/spec"), None).await;
    assert_eq!(exported["start_options"]["kvm"], Value::Null);
    assert!(exported["autoscale"].is_null());
}

#[tokio::test]
async fn expected_plan_hash_mismatch_is_409_and_match_applies() {
    let app = common::create_test_app().await;
    let n = "plan-hash-guard";

    let (_, plan) = call(&app, "POST", "/api/vms/apply?dry_run=true", Some(spec(n))).await;
    let hash = plan["plan_hash"].as_str().unwrap().to_string();

    // wrong hash: refused, nothing created
    let (st, body) = call(
        &app,
        "POST",
        "/api/vms/apply?expected_plan_hash=deadbeef",
        Some(spec(n)),
    )
    .await;
    assert_eq!(st, StatusCode::CONFLICT);
    assert_eq!(body["actual_plan_hash"], hash);
    let (st, _) = call(&app, "GET", &format!("/api/vms/{n}/spec"), None).await;
    assert_eq!(st, StatusCode::NOT_FOUND);

    // right hash: applied
    let (st, res) = call(
        &app,
        "POST",
        &format!("/api/vms/apply?expected_plan_hash={hash}"),
        Some(spec(n)),
    )
    .await;
    assert_eq!(st, StatusCode::OK);
    assert_eq!(res["created"], true);

    // the live plan has moved on (now a noop), so the old hash is stale
    let (st, _) = call(
        &app,
        "POST",
        &format!("/api/vms/apply?expected_plan_hash={hash}"),
        Some(spec(n)),
    )
    .await;
    assert_eq!(st, StatusCode::CONFLICT);
}

#[tokio::test]
async fn plan_route_reports_unmanaged_and_prunes_only_on_request() {
    let app = common::create_test_app().await;
    let (st, _) = call(
        &app,
        "POST",
        "/api/vms/apply",
        Some(spec("plan-route-keep")),
    )
    .await;
    assert_eq!(st, StatusCode::OK);
    let (st, _) = call(
        &app,
        "POST",
        "/api/vms/apply",
        Some(spec("plan-route-extra")),
    )
    .await;
    assert_eq!(st, StatusCode::OK);

    let find = |plan: &Value, name: &str| -> Value {
        plan["entries"]
            .as_array()
            .unwrap()
            .iter()
            .find(|e| e["name"] == name)
            .cloned()
            .unwrap()
    };

    let body = json!({"vms": [spec("plan-route-keep")]});
    let (st, plan) = call(&app, "POST", "/api/vms/plan", Some(body)).await;
    assert_eq!(st, StatusCode::OK);
    assert_eq!(find(&plan, "plan-route-keep")["action"], "noop");
    assert_eq!(find(&plan, "plan-route-extra")["action"], "unmanaged");

    let body = json!({"vms": [spec("plan-route-keep")], "prune": true});
    let (_, plan) = call(&app, "POST", "/api/vms/plan", Some(body)).await;
    assert_eq!(find(&plan, "plan-route-extra")["action"], "delete");

    // planning deleted nothing
    let (st, _) = call(&app, "GET", "/api/vms/plan-route-extra/spec", None).await;
    assert_eq!(st, StatusCode::OK);

    // duplicate names rejected
    let body = json!({"vms": [spec("dup-a"), spec("dup-a")]});
    let (st, _) = call(&app, "POST", "/api/vms/plan", Some(body)).await;
    assert_eq!(st, StatusCode::BAD_REQUEST);
}
