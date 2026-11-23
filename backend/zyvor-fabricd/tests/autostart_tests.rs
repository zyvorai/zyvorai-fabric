// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: LicenseRef-Zyvor-Production-2.0

//! Router tests for `POST /api/vms/autostart/reconcile`.

use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use serde_json::{json, Value};
use tower::ServiceExt;

mod common;

fn post(uri: &str, body: Option<Value>) -> Request<Body> {
    let b = Request::builder().method("POST").uri(uri);
    match body {
        Some(v) => b
            .header("content-type", "application/json")
            .body(Body::from(v.to_string()))
            .unwrap(),
        None => b.body(Body::empty()).unwrap(),
    }
}

#[tokio::test]
async fn dry_run_reports_flagged_vms_and_changes_nothing() {
    let app = common::create_test_app().await;
    for (name, tags) in [("as-on", json!(["autostart"])), ("as-off", json!([]))] {
        let resp = app
            .clone()
            .oneshot(post(
                "/api/vms",
                Some(json!({"name": name, "image": "t.qcow2", "cpus": 1, "memory": 512, "tags": tags})),
            ))
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::CREATED);
    }

    let resp = app
        .clone()
        .oneshot(post("/api/vms/autostart/reconcile?dry_run=true", None))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX)
        .await
        .unwrap();
    let body: Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(body["dry_run"], true);
    assert_eq!(body["trigger"], "manual");
    let started: Vec<_> = body["started"]
        .as_array()
        .unwrap()
        .iter()
        .map(|s| s["vm"].as_str().unwrap().to_string())
        .collect();
    assert!(started.contains(&"as-on".to_string()), "{body}");
    assert!(!started.contains(&"as-off".to_string()), "{body}");

    let resp = app
        .oneshot(
            Request::builder()
                .uri("/api/vms/as-on")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX)
        .await
        .unwrap();
    let vm: Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(vm["state"], "stopped");
}

async fn json_body(resp: axum::response::Response) -> Value {
    let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX)
        .await
        .unwrap();
    serde_json::from_slice(&bytes).unwrap()
}

fn get(uri: &str) -> Request<Body> {
    Request::builder().uri(uri).body(Body::empty()).unwrap()
}

#[tokio::test]
async fn live_reconcile_is_a_job_with_progress_and_history() {
    let app = common::create_test_app().await;
    let resp = app
        .clone()
        .oneshot(post(
            "/api/vms",
            Some(json!({"name": "job-vm", "image": "t.qcow2", "cpus": 1, "memory": 512, "tags": ["autostart"]})),
        ))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::CREATED);

    let resp = app
        .clone()
        .oneshot(post("/api/vms/autostart/reconcile", None))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::ACCEPTED);
    let accepted = json_body(resp).await;
    let id = accepted["id"].as_str().unwrap().to_string();
    assert_eq!(accepted["state"], "running");

    // Poll until the run is no longer running.
    let mut run = Value::Null;
    for _ in 0..100 {
        let resp = app
            .clone()
            .oneshot(get(&format!("/api/vms/autostart/reconcile/{id}")))
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        run = json_body(resp).await;
        if run["state"] != "running" {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }
    assert_eq!(run["state"], "completed", "{run}");
    assert_eq!(run["trigger"], "manual");
    assert_eq!(run["report"]["dry_run"], false);
    assert_eq!(run["report"]["summary"]["candidates"], 1, "{run}");
    let vms = run["vms"].as_array().unwrap();
    assert_eq!(vms.len(), 1);
    assert_eq!(vms[0]["vm"], "job-vm");
    assert_ne!(vms[0]["status"], "pending");

    let resp = app
        .clone()
        .oneshot(get("/api/vms/autostart/reconcile"))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let list = json_body(resp).await;
    assert!(
        list["runs"]
            .as_array()
            .unwrap()
            .iter()
            .any(|r| r["id"] == id.as_str() && r["state"] == "completed"),
        "{list}"
    );

    let resp = app
        .oneshot(get("/api/vms/autostart/reconcile/does-not-exist"))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
}
