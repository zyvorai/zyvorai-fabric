// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: LicenseRef-Zyvor-Production-2.0

//! Router tests for the CPU inventory and compatibility endpoints, and their
//! integration with the drain planner and preflight.

use axum::{
    body::Body,
    http::{Request, StatusCode},
    Router,
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

async fn call(app: &Router, method: &str, uri: &str, body: Option<Value>) -> (StatusCode, Value) {
    let resp = app.clone().oneshot(req(method, uri, body)).await.unwrap();
    let s = resp.status();
    (s, json_of(resp).await)
}

async fn add_host(app: &Router, name: &str, cluster: &str) -> String {
    let (s, v) = call(
        app,
        "POST",
        "/api/hosts",
        Some(
            json!({"hostname": name, "address": "10.0.0.1", "cluster_id": cluster,
            "cpus": 8, "memory_mb": 8192, "agent_version": "t"}),
        ),
    )
    .await;
    assert_eq!(s, StatusCode::CREATED);
    v["id"].as_str().unwrap().to_string()
}

const OLD: &[&str] = &["fpu", "sse4_2", "ssse3", "popcnt"];
const NEW: &[&str] = &[
    "sse4_2",
    "sse4_1",
    "ssse3",
    "popcnt",
    "pni",
    "cx16",
    "lahf_lm",
    "pclmulqdq",
    "aes",
    "avx",
    "xsave",
    "f16c",
    "rdrand",
    "fsgsbase",
    "avx2",
    "bmi1",
    "bmi2",
    "fma",
    "movbe",
    "abm",
    "erms",
    "invpcid",
];

#[tokio::test]
async fn put_inventory_validates_and_round_trips() {
    let app = common::create_test_app().await;
    let id = add_host(&app, "cc-h0", "cc-c0").await;
    let uri = format!("/api/hosts/{id}/cpu-inventory");

    let (s, _) = call(&app, "GET", &uri, None).await;
    assert_eq!(s, StatusCode::NOT_FOUND);
    let (s, _) = call(
        &app,
        "PUT",
        "/api/hosts/nope/cpu-inventory",
        Some(json!({"vendor": "x", "model_name": "y", "flags": ["avx"]})),
    )
    .await;
    assert_eq!(s, StatusCode::NOT_FOUND);

    for bad in [
        json!({"vendor": "x", "model_name": "y", "flags": []}),
        json!({"vendor": "x", "model_name": "y", "flags": ["a b"]}),
        json!({"vendor": "x\u{1}y", "model_name": "y", "flags": ["avx"]}),
        json!({"vendor": "x", "model_name": "y", "flags": ["avx"], "extra": 1}),
    ] {
        let (s, _) = call(&app, "PUT", &uri, Some(bad)).await;
        assert!(
            s == StatusCode::BAD_REQUEST || s == StatusCode::UNPROCESSABLE_ENTITY,
            "{s}"
        );
    }

    let (s, v) = call(
        &app,
        "PUT",
        &uri,
        Some(json!({"vendor": "GenuineIntel", "model_name": "Xeon", "flags": ["AVX2", "aes"]})),
    )
    .await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(v["source"], "reported");
    let (s, v) = call(&app, "GET", &uri, None).await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(v["flags"], json!(["aes", "avx2"]));
}

#[tokio::test]
async fn from_local_reads_proc_cpuinfo_or_reports_unavailable() {
    let app = common::create_test_app().await;
    let id = add_host(&app, "cc-local", "cc-cl").await;
    let (s, v) = call(
        &app,
        "PUT",
        &format!("/api/hosts/{id}/cpu-inventory"),
        Some(json!({"from_local": true})),
    )
    .await;
    assert!(s == StatusCode::OK || s == StatusCode::SERVICE_UNAVAILABLE);
    if s == StatusCode::OK {
        assert_eq!(v["source"], "local");
        assert!(!v["flags"].as_array().unwrap().is_empty());
    }
}

#[tokio::test]
async fn compat_inventory_and_drain_plan_agree() {
    let app = common::create_test_app().await;
    let a = add_host(&app, "cc-a", "cc-c1").await;
    let b = add_host(&app, "cc-b", "cc-c1").await;
    let c = add_host(&app, "cc-c", "cc-c1").await;
    for (id, flags) in [(&a, NEW), (&b, OLD)] {
        let (s, _) = call(
            &app,
            "PUT",
            &format!("/api/hosts/{id}/cpu-inventory"),
            Some(json!({"vendor": "GenuineIntel", "model_name": "m", "flags": flags})),
        )
        .await;
        assert_eq!(s, StatusCode::OK);
    }
    let (s, _) = call(
        &app,
        "POST",
        "/api/vms",
        Some(
            json!({"name": "cc-vm", "image": "t.qcow2", "cpus": 1, "memory": 512,
            "labels": {"fabric.zyvor.dev/host": "cc-a"}}),
        ),
    )
    .await;
    assert_eq!(s, StatusCode::CREATED);

    // No stored config: nothing to check.
    let (s, v) = call(&app, "GET", "/api/vms/cc-vm/cpu-compat", None).await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(v["configured"], false);

    let (s, _) = call(
        &app,
        "POST",
        "/api/vms/cc-vm/cpu-model",
        Some(json!({"mode": "custom", "model": "Haswell"})),
    )
    .await;
    assert_eq!(s, StatusCode::OK);

    let (s, v) = call(&app, "GET", "/api/vms/cc-vm/cpu-compat", None).await;
    assert_eq!(s, StatusCode::OK);
    let results = v["results"].as_array().unwrap();
    let verdict = |id: &str| {
        results.iter().find(|r| r["host_id"] == id).unwrap()["result"]["verdict"]
            .as_str()
            .unwrap()
            .to_string()
    };
    assert_eq!(verdict(&a), "compatible");
    assert_eq!(verdict(&b), "incompatible");
    assert_eq!(verdict(&c), "unknown");
    let (s, _) = call(&app, "GET", "/api/vms/cc-vm/cpu-compat?host=nope", None).await;
    assert_eq!(s, StatusCode::NOT_FOUND);
    let (s, one) = call(&app, "GET", "/api/vms/cc-vm/cpu-compat?host=cc-b", None).await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(one["results"].as_array().unwrap().len(), 1);
    assert!(one["results"][0]["result"]["missing"]
        .as_array()
        .unwrap()
        .contains(&json!("avx2")));

    // Drain plan: b is incompatible, c has no inventory so VM goes to c (with a warning).
    let (s, plan) = call(&app, "POST", &format!("/api/hosts/{a}/drain-plan"), None).await;
    assert_eq!(s, StatusCode::OK);
    let act = &plan["actions"][0];
    assert_eq!(act["target"], c.as_str());
    assert!(act["warnings"][0]
        .as_str()
        .unwrap()
        .contains("not verified"));

    // Without c, the plan is blocked with the cpu reason.
    let (s, _) = call(
        &app,
        "PUT",
        &format!("/api/hosts/{c}/cpu-inventory"),
        Some(json!({"vendor": "AuthenticAMD", "model_name": "m", "flags": NEW})),
    )
    .await;
    assert_eq!(s, StatusCode::OK);
    let (_, plan) = call(&app, "POST", &format!("/api/hosts/{a}/drain-plan"), None).await;
    // vendor differs only matters if the VM names a vendor; NEW flags suffice, so still c
    assert_eq!(plan["actions"][0]["target"], c.as_str());
    let (_, _) = call(
        &app,
        "POST",
        "/api/vms/cc-vm/cpu-model",
        Some(json!({"vendor": "intel"})),
    )
    .await;
    let (_, plan) = call(&app, "POST", &format!("/api/hosts/{a}/drain-plan"), None).await;
    assert_eq!(plan["can_drain"], false);
    assert_eq!(plan["actions"][0]["reasons"], json!(["cpu_incompatible"]));

    // Inventory overview lists hosts, the VM config and a cluster baseline.
    let (s, inv) = call(&app, "GET", "/api/cpu/inventory", None).await;
    assert_eq!(s, StatusCode::OK);
    let cl = inv["clusters"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["cluster_id"] == "cc-c1")
        .unwrap();
    assert_eq!(cl["hosts_total"], 3);
    assert_eq!(cl["hosts_known"], 3);
    assert!(inv["vms"]
        .as_array()
        .unwrap()
        .iter()
        .any(|v| v["name"] == "cc-vm" && v["configured"] == true));
}
