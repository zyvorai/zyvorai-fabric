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
