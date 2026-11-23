// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: LicenseRef-Zyvor-Production-2.0

//! Router tests for the host drain plan and snapshot health endpoints.

use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use serde_json::{json, Value};
use tower::ServiceExt;

mod common;

fn req(method: &str, uri: &str, body: Option<Value>) -> Request<Body> {
    let b = Request::builder().method(method).uri(uri);
    match body {
        Some(v) => b
            .header("content-type", "application/json")
            .body(Body::from(v.to_string()))
            .unwrap(),
        None => b.body(Body::empty()).unwrap(),
    }
}

async fn json_of(resp: axum::response::Response) -> Value {
    let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX)
        .await
        .unwrap();
    serde_json::from_slice(&bytes).unwrap_or(Value::Null)
}

#[tokio::test]
async fn drain_plan_unknown_host_is_404() {
    let app = common::create_test_app().await;
    let resp = app
        .oneshot(req("POST", "/api/hosts/no-such-host/drain-plan", None))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn drain_plan_relocates_labelled_vms_and_changes_nothing() {
    let app = common::create_test_app().await;
    let mut ids = Vec::new();
    for name in ["fi-h1", "fi-h2"] {
        let resp = app
            .clone()
            .oneshot(req(
                "POST",
                "/api/hosts",
                Some(
                    json!({"hostname": name, "address": "10.0.0.1", "cluster_id": "fi-cluster",
                    "cpus": 8, "memory_mb": 8192, "agent_version": "t"}),
                ),
            ))
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::CREATED);
        ids.push(json_of(resp).await["id"].as_str().unwrap().to_string());
    }
    let resp = app
        .clone()
        .oneshot(req(
            "POST",
            "/api/vms",
            Some(
                json!({"name": "fi-vm", "image": "t.qcow2", "cpus": 1, "memory": 512,
                "labels": {"fabric.zyvor.dev/host": "fi-h1"}}),
            ),
        ))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::CREATED);

    let uri = format!("/api/hosts/{}/drain-plan", ids[0]);
    let resp = app.clone().oneshot(req("POST", &uri, None)).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let plan = json_of(resp).await;
    assert_eq!(plan["dry_run"], true);
    assert_eq!(plan["can_drain"], true);
    let a = &plan["actions"][0];
    assert_eq!(a["vm"], "fi-vm");
    assert_eq!(a["target"], ids[1].as_str());
    assert_eq!(plan["plan_hash"].as_str().unwrap().len(), 64);

    // same state, same hash; hostname works as the id
    let again = json_of(
        app.clone()
            .oneshot(req("POST", "/api/hosts/fi-h1/drain-plan", None))
            .await
            .unwrap(),
    )
    .await;
    assert_eq!(again["plan_hash"], plan["plan_hash"]);

    // the VM was not touched
    let vm = json_of(
        app.oneshot(req("GET", "/api/vms/fi-vm", None))
            .await
            .unwrap(),
    )
    .await;
    assert_eq!(vm["labels"]["fabric.zyvor.dev/host"], "fi-h1");
}

#[tokio::test]
async fn snapshot_health_lists_vm_without_snapshots_and_validates_query() {
    let app = common::create_test_app().await;
    let resp = app
        .clone()
        .oneshot(req(
            "POST",
            "/api/vms",
            Some(json!({"name": "sh-vm", "image": "t.qcow2", "cpus": 1, "memory": 512})),
        ))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::CREATED);

    let resp = app
        .clone()
        .oneshot(req("GET", "/api/snapshots/health", None))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let v = json_of(resp).await;
    let row = v["vms"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["vm"] == "sh-vm")
        .expect("vm listed");
    assert_eq!(row["status"], "no_snapshots");
    assert_eq!(row["rpo_source"], "default");
    assert!(v["summary"]["no_snapshots"].as_u64().unwrap() >= 1);

    let resp = app
        .oneshot(req("GET", "/api/snapshots/health?default_rpo_secs=5", None))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
}
