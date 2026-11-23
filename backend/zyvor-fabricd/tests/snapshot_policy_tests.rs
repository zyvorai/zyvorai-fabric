// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: LicenseRef-Zyvor-Production-2.0

//! Router tests for the snapshot policy endpoints.

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
async fn policy_lifecycle_and_validation() {
    let app = common::create_test_app().await;
    let resp = app
        .clone()
        .oneshot(req(
            "POST",
            "/api/vms",
            Some(json!({"name": "sp-vm", "image": "t.qcow2", "cpus": 1, "memory": 512})),
        ))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::CREATED);

    let uri = "/api/vms/sp-vm/snapshot-policy";
    let resp = app.clone().oneshot(req("GET", uri, None)).await.unwrap();
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);

    // keep_last = 0 is rejected
    let resp = app
        .clone()
        .oneshot(req(
            "PUT",
            uri,
            Some(json!({"enabled": true, "interval_secs": 3600, "keep_last": 0})),
        ))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);

    let resp = app
        .clone()
        .oneshot(req(
            "PUT",
            uri,
            Some(json!({"enabled": true, "cron": "0 3 * * *", "keep_last": 3})),
        ))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    let resp = app.clone().oneshot(req("GET", uri, None)).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let v = json_of(resp).await;
    assert_eq!(v["keep_last"], 3);
    assert_eq!(v["name_prefix"], "auto");

    // dry run plans a snapshot and changes nothing
    let resp = app
        .clone()
        .oneshot(req("POST", &format!("{uri}/run?dry_run=true"), None))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let v = json_of(resp).await;
    assert_eq!(v["dry_run"], true);
    assert!(v["created"].as_str().unwrap().starts_with("auto-"));
    let resp = app
        .clone()
        .oneshot(req("GET", "/api/vms/sp-vm/snapshots", None))
        .await
        .unwrap();
    assert_eq!(json_of(resp).await, json!([]));

    let resp = app.clone().oneshot(req("DELETE", uri, None)).await.unwrap();
    assert_eq!(resp.status(), StatusCode::NO_CONTENT);
}

#[tokio::test]
async fn unknown_vm_is_404() {
    let app = common::create_test_app().await;
    let resp = app
        .oneshot(req(
            "PUT",
            "/api/vms/nope/snapshot-policy",
            Some(json!({"interval_secs": 3600, "keep_last": 2})),
        ))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn policy_get_reports_next_run() {
    let app = common::create_test_app().await;
    let resp = app
        .clone()
        .oneshot(req(
            "POST",
            "/api/vms",
            Some(json!({"name": "nr-vm", "image": "t.qcow2", "cpus": 1, "memory": 512})),
        ))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::CREATED);
    let uri = "/api/vms/nr-vm/snapshot-policy";
    let resp = app
        .clone()
        .oneshot(req(
            "PUT",
            uri,
            Some(json!({"enabled": true, "interval_secs": 3600, "keep_last": 2})),
        ))
        .await
        .unwrap();
    assert!(json_of(resp).await["next_run"].is_string());
    let resp = app.clone().oneshot(req("GET", uri, None)).await.unwrap();
    assert!(json_of(resp).await["next_run"].is_string());
    // disabled: no next run
    let resp = app
        .clone()
        .oneshot(req(
            "PUT",
            uri,
            Some(json!({"enabled": false, "interval_secs": 3600, "keep_last": 2})),
        ))
        .await
        .unwrap();
    assert!(json_of(resp).await["next_run"].is_null());
}

#[tokio::test]
async fn snapshot_tags_endpoint_validates_and_404s() {
    let app = common::create_test_app().await;
    let uri = "/api/vms/some-vm/snapshots/nope/tags";
    let resp = app
        .clone()
        .oneshot(req("PUT", uri, Some(json!({"tags": ["keep"]}))))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
    let resp = app
        .clone()
        .oneshot(req("PUT", uri, Some(json!({"tags": ["bad tag"]}))))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
}
