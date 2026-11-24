// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: LicenseRef-Zyvor-Production-2.0

//! Router tests for snapshot hygiene, VM ownership and the reclaim report.

use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use chrono::{Duration, Utc};
use serde_json::{json, Value};
use tower::ServiceExt;
use zyvor_fabricd::api::snapshots::{SnapshotType, VMSnapshot};
use zyvor_fabricd::config::{AuditConfig, ConfigHistoryConfig};

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

async fn make_vm(app: &axum::Router, name: &str) {
    let resp = app
        .clone()
        .oneshot(req(
            "POST",
            "/api/vms",
            Some(json!({"name": name, "image": "t.qcow2", "cpus": 1, "memory": 512})),
        ))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::CREATED);
}

fn snap(vm: &str, id: &str, age_h: i64, parent: Option<&str>, tags: &[&str]) -> VMSnapshot {
    VMSnapshot {
        id: id.into(),
        vm_name: vm.into(),
        name: format!("s-{id}"),
        description: None,
        snapshot_type: SnapshotType::Disk,
        parent_id: parent.map(String::from),
        size_bytes: 1000,
        created: Utc::now() - Duration::hours(age_h),
        tags: tags.iter().map(|t| t.to_string()).collect(),
    }
}

#[tokio::test]
async fn hygiene_reports_stale_and_validates_thresholds() {
    let (app, state) = common::create_test_app_with_history(
        "hyg",
        ConfigHistoryConfig::default(),
        AuditConfig::default(),
    )
    .await;
    make_vm(&app, "hy-vm").await;
    let key = "snapshots_hy-vm";
    for s in [
        snap("hy-vm", "a", 200, None, &[]),
        snap("hy-vm", "b", 100, Some("a"), &["keep"]),
        snap("hy-vm", "c", 1, Some("b"), &[]),
    ] {
        state.store.save_entity(key, &s.id, &s).unwrap();
    }

    let resp = app
        .clone()
        .oneshot(req("GET", "/api/snapshots/hygiene", None))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let v = json_of(resp).await;
    assert_eq!(v["max_age_hours"], 72);
    let vm = v["vms"]
        .as_array()
        .unwrap()
        .iter()
        .find(|x| x["vm"] == "hy-vm")
        .unwrap()
        .clone();
    assert_eq!(vm["status"], "warning");
    assert_eq!(vm["snapshot_count"], 3);
    assert_eq!(vm["max_chain_depth"], 3);
    assert_eq!(vm["stale"].as_array().unwrap().len(), 2);
    assert_eq!(vm["reclaimable_bytes"], 1000); // keep-tagged one excluded
    assert!(v["summary"]["protected_stale"].as_u64().unwrap() >= 1);

    // Tighter chain limit and age change the verdict.
    let resp = app
        .clone()
        .oneshot(req(
            "GET",
            "/api/snapshots/hygiene?max_age_hours=500&max_chain=2",
            None,
        ))
        .await
        .unwrap();
    let v = json_of(resp).await;
    let vm = v["vms"]
        .as_array()
        .unwrap()
        .iter()
        .find(|x| x["vm"] == "hy-vm")
        .unwrap()
        .clone();
    assert!(vm["stale"].as_array().unwrap().is_empty());
    assert_eq!(vm["status"], "warning"); // chain 3 > 2

    for bad in ["max_age_hours=0", "max_chain=0", "max_chain=33"] {
        let resp = app
            .clone()
            .oneshot(req("GET", &format!("/api/snapshots/hygiene?{bad}"), None))
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST, "{bad}");
    }
}

#[tokio::test]
async fn ownership_crud_extend_and_report() {
    let app = common::create_test_app().await;
    make_vm(&app, "ow-vm").await;
    make_vm(&app, "ow-bare").await;
    let uri = "/api/vms/ow-vm/ownership";

    let resp = app.clone().oneshot(req("GET", uri, None)).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    assert!(json_of(resp).await["owner"].is_null());

    let resp = app
        .clone()
        .oneshot(req("GET", "/api/vms/nope/ownership", None))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);

    let soon = (Utc::now() + Duration::days(3)).to_rfc3339();
    let resp = app
        .clone()
        .oneshot(req(
            "PUT",
            uri,
            Some(json!({"owner": " alice ", "contact": "alice@example.com",
                "purpose": "build runner", "notes": "line1\nline2", "expires_at": soon})),
        ))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let v = json_of(resp).await;
    assert_eq!(v["owner"], "alice");
    assert_eq!(v["updated_by"], "test-admin");

    // Bad input is a 400 and changes nothing.
    for body in [
        json!({"owner": "a\u{7}b"}),
        json!({"owner": "x".repeat(200)}),
        json!({"expires_at": (Utc::now() + Duration::days(5000)).to_rfc3339()}),
    ] {
        let resp = app
            .clone()
            .oneshot(req("PUT", uri, Some(body)))
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    }
    let resp = app.clone().oneshot(req("GET", uri, None)).await.unwrap();
    assert_eq!(json_of(resp).await["owner"], "alice");

    // Report: ow-vm is expiring, ow-bare is unowned.
    let resp = app
        .clone()
        .oneshot(req("GET", "/api/ownership/report?within_days=7", None))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let v = json_of(resp).await;
    let status_of = |n: &str| {
        v["vms"]
            .as_array()
            .unwrap()
            .iter()
            .find(|x| x["vm"] == n)
            .unwrap()["status"]
            .clone()
    };
    assert_eq!(status_of("ow-vm"), "expiring");
    assert_eq!(status_of("ow-bare"), "unowned");

    // Extending 30 days moves it out of the 7-day window.
    let resp = app
        .clone()
        .oneshot(req(
            "POST",
            &format!("{uri}/extend"),
            Some(json!({"days": 30})),
        ))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    for days in [0, 400] {
        let resp = app
            .clone()
            .oneshot(req(
                "POST",
                &format!("{uri}/extend"),
                Some(json!({"days": days})),
            ))
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    }
    let resp = app
        .clone()
        .oneshot(req("GET", "/api/ownership/report?within_days=7", None))
        .await
        .unwrap();
    let v = json_of(resp).await;
    let row = v["vms"]
        .as_array()
        .unwrap()
        .iter()
        .find(|x| x["vm"] == "ow-vm")
        .unwrap()
        .clone();
    assert_eq!(row["status"], "owned");
    assert!(row["days_to_expiry"].as_i64().unwrap() >= 32);

    let resp = app
        .clone()
        .oneshot(req("GET", "/api/ownership/report?within_days=0", None))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);

    let resp = app.clone().oneshot(req("DELETE", uri, None)).await.unwrap();
    assert_eq!(resp.status(), StatusCode::NO_CONTENT);
    let resp = app.clone().oneshot(req("GET", uri, None)).await.unwrap();
    assert!(json_of(resp).await["owner"].is_null());
}

#[tokio::test]
async fn ownership_requires_write_role() {
    let app = common::create_test_app_with_role(security::Role::Viewer).await;
    let resp = app
        .clone()
        .oneshot(req(
            "PUT",
            "/api/vms/x/ownership",
            Some(json!({"owner": "a"})),
        ))
        .await
        .unwrap();
    assert!(
        resp.status() == StatusCode::FORBIDDEN || resp.status() == StatusCode::UNAUTHORIZED,
        "got {}",
        resp.status()
    );
    let resp = app
        .oneshot(req("GET", "/api/ownership/report", None))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
}

#[tokio::test]
async fn ownership_is_removed_with_the_vm() {
    let (app, state) = common::create_test_app_with_history(
        "own-del",
        ConfigHistoryConfig::default(),
        AuditConfig::default(),
    )
    .await;
    make_vm(&app, "od-vm").await;
    let resp = app
        .clone()
        .oneshot(req(
            "PUT",
            "/api/vms/od-vm/ownership",
            Some(json!({"owner": "a"})),
        ))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let resp = app
        .clone()
        .oneshot(req("DELETE", "/api/vms/od-vm", None))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::NO_CONTENT);
    let left = state
        .store
        .get_entity::<zyvor_fabricd::vm_ownership::Ownership>("vm_ownership", "od-vm")
        .unwrap();
    assert!(left.is_none());
}

#[tokio::test]
async fn reclaim_report_ranks_expired_lease_and_idle_vms() {
    let (app, state) = common::create_test_app_with_history(
        "reclaim",
        ConfigHistoryConfig::default(),
        AuditConfig::default(),
    )
    .await;
    make_vm(&app, "rc-idle").await;
    make_vm(&app, "rc-fresh").await;
    make_vm(&app, "rc-lease").await;

    // Make rc-idle look untouched for 90 days.
    let mut vm = state.store.get_vm("rc-idle").unwrap().unwrap();
    vm.updated = Some(Utc::now() - Duration::days(90));
    vm.disk = 40;
    state.store.save_vm(&vm).unwrap();
    let mut vm = state.store.get_vm("rc-fresh").unwrap().unwrap();
    vm.updated = Some(Utc::now() - Duration::days(2));
    state.store.save_vm(&vm).unwrap();
    let s = snap("rc-idle", "x", 500, None, &[]);
    state
        .store
        .save_entity("snapshots_rc-idle", &s.id, &s)
        .unwrap();

    // Expired lease on rc-lease (set directly: the API refuses nothing in the
    // past, but this keeps the test independent of extend).
    let resp = app
        .clone()
        .oneshot(req(
            "PUT",
            "/api/vms/rc-lease/ownership",
            Some(json!({"owner": "bob", "expires_at": (Utc::now() - Duration::days(1)).to_rfc3339()})),
        ))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    let resp = app
        .clone()
        .oneshot(req("GET", "/api/reclaim/report?stopped_days=30", None))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let v = json_of(resp).await;
    let names: Vec<&str> = v["candidates"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| c["vm"].as_str().unwrap())
        .collect();
    assert!(names.contains(&"rc-idle"));
    assert!(names.contains(&"rc-lease"));
    assert!(!names.contains(&"rc-fresh"));
    let idle = v["candidates"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["vm"] == "rc-idle")
        .unwrap()
        .clone();
    assert_eq!(idle["idle_basis"], "updated");
    assert!(idle["idle_days"].as_i64().unwrap() >= 89);
    assert_eq!(idle["reclaimable_snapshot_bytes"], 1000);
    assert!(v["summary"]["expired_leases"].as_u64().unwrap() >= 1);
    assert!(v["summary"]["stale_snapshot_bytes_total"].as_u64().unwrap() >= 1000);
    // Expired lease sorts before plain idle.
    assert!(
        names.iter().position(|n| *n == "rc-lease").unwrap()
            < names.iter().position(|n| *n == "rc-idle").unwrap()
    );

    for bad in [
        "stopped_days=0",
        "stopped_days=99999",
        "snapshot_age_hours=0",
    ] {
        let resp = app
            .clone()
            .oneshot(req("GET", &format!("/api/reclaim/report?{bad}"), None))
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST, "{bad}");
    }
}
