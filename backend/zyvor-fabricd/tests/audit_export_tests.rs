// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: LicenseRef-Zyvor-Production-2.0

//! Router tests for audit export, verification and retention.

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

async fn text_of(resp: axum::response::Response) -> String {
    let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX)
        .await
        .unwrap();
    String::from_utf8_lossy(&bytes).to_string()
}

async fn json_of(resp: axum::response::Response) -> Value {
    serde_json::from_str(&text_of(resp).await).unwrap_or(Value::Null)
}

#[tokio::test]
async fn export_verify_and_retention_flow() {
    let app = common::create_test_app_with_audit(zyvor_fabricd::config::AuditConfig {
        chain: true,
        ..Default::default()
    })
    .await;

    for i in 0..4 {
        let resp = app
            .clone()
            .oneshot(req(
                "POST",
                "/api/vms",
                Some(json!({"name": format!("ax-vm{i}"), "image": "t.qcow2", "cpus": 1, "memory": 512})),
            ))
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::CREATED);
    }

    // NDJSON export: chained events, oldest first, actor filter is exact.
    let resp = app
        .clone()
        .oneshot(req(
            "GET",
            "/api/audit/export?actor=TEST-ADMIN&action=create",
            None,
        ))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    assert_eq!(resp.headers()["content-type"], "application/x-ndjson");
    assert!(resp.headers()["content-disposition"]
        .to_str()
        .unwrap()
        .contains("audit-export-"));
    let body = text_of(resp).await;
    let lines: Vec<Value> = body
        .lines()
        .map(|l| serde_json::from_str(l).unwrap())
        .collect();
    assert_eq!(lines.len(), 4, "{body}");
    assert!(lines
        .iter()
        .all(|l| l["user"] == "test-admin" && l["hash"].is_string()));
    assert!(lines
        .windows(2)
        .all(|w| w[0]["seq"].as_u64() < w[1]["seq"].as_u64()));

    // CSV has a header and the same rows.
    let resp = app
        .clone()
        .oneshot(req(
            "GET",
            "/api/audit/export?format=csv&action=CREATE",
            None,
        ))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    assert!(resp.headers()["content-type"]
        .to_str()
        .unwrap()
        .starts_with("text/csv"));
    let csv = text_of(resp).await;
    assert!(csv.starts_with("id,timestamp,user,action,"));
    assert_eq!(csv.lines().count(), 5);

    // Bad inputs.
    for uri in [
        "/api/audit/export?format=xml",
        "/api/audit/export?from=yesterday",
        "/api/audit/export?from=2026-03-02&to=2026-03-01",
    ] {
        let resp = app.clone().oneshot(req("GET", uri, None)).await.unwrap();
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST, "{uri}");
    }
    // A future window matches nothing.
    let resp = app
        .clone()
        .oneshot(req("GET", "/api/audit/export?from=2999-01-01", None))
        .await
        .unwrap();
    assert_eq!(text_of(resp).await, "");

    // The exports above were themselves audited and the chain is intact.
    let resp = app
        .clone()
        .oneshot(req("GET", "/api/audit/verify", None))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let v = json_of(resp).await;
    assert_eq!(v["ok"], true, "{v}");
    assert_eq!(v["status"], "ok");
    assert!(v["chained"].as_u64().unwrap() >= 7);
    assert!(v["first_break"].is_null());

    let resp = app
        .clone()
        .oneshot(req("GET", "/api/audit/export?action=AUDIT_EXPORT", None))
        .await
        .unwrap();
    assert!(text_of(resp).await.lines().count() >= 3);

    // Retention: needs a limit, previews by default, then really prunes.
    let resp = app
        .clone()
        .oneshot(req("POST", "/api/audit/retention", None))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    let resp = app
        .clone()
        .oneshot(req("POST", "/api/audit/retention?max_events=3", None))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let r = json_of(resp).await;
    assert_eq!(r["dry_run"], true);
    assert!(r["pruned"].as_u64().unwrap() >= 4);
    assert!(r["event_id"].is_null());

    let resp = app
        .clone()
        .oneshot(req(
            "POST",
            "/api/audit/retention?max_events=3&dry_run=false",
            None,
        ))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let r = json_of(resp).await;
    assert_eq!(r["dry_run"], false);
    assert!(r["event_id"].is_string());

    let resp = app
        .clone()
        .oneshot(req("GET", "/api/audit/export?action=AUDIT_RETENTION", None))
        .await
        .unwrap();
    let body = text_of(resp).await;
    assert_eq!(body.lines().count(), 1);
    assert!(body.contains("pruned"));

    let resp = app
        .clone()
        .oneshot(req("GET", "/api/audit/verify", None))
        .await
        .unwrap();
    let v = json_of(resp).await;
    assert_eq!(v["ok"], true, "{v}");
    assert!(v["anchor"]["seq"].as_u64().unwrap() >= 4);
}

#[tokio::test]
async fn export_is_refused_over_the_cap() {
    let app = common::create_test_app_with_audit(zyvor_fabricd::config::AuditConfig {
        max_export_events: 1,
        ..Default::default()
    })
    .await;
    for i in 0..2 {
        let resp = app
            .clone()
            .oneshot(req(
                "POST",
                "/api/vms",
                Some(json!({"name": format!("cap-vm{i}"), "image": "t.qcow2", "cpus": 1, "memory": 512})),
            ))
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::CREATED);
    }
    let resp = app
        .oneshot(req("GET", "/api/audit/export", None))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::PAYLOAD_TOO_LARGE);
}

#[tokio::test]
async fn roles_are_enforced() {
    // Read-only auditor: may export and verify, may not prune.
    let viewer = common::create_test_app_with_role(security::Role::Viewer).await;
    for uri in ["/api/audit/export", "/api/audit/verify"] {
        let resp = viewer.clone().oneshot(req("GET", uri, None)).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK, "{uri}");
    }
    let resp = viewer
        .oneshot(req("POST", "/api/audit/retention?max_events=1", None))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::FORBIDDEN);

    // A write-capable non-admin user may do none of it.
    let user = common::create_test_app_with_role(security::Role::User).await;
    for uri in ["/api/audit/export", "/api/audit/verify"] {
        let resp = user.clone().oneshot(req("GET", uri, None)).await.unwrap();
        assert_eq!(resp.status(), StatusCode::FORBIDDEN, "{uri}");
    }
    let resp = user
        .oneshot(req("POST", "/api/audit/retention?max_events=1", None))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::FORBIDDEN);
}
