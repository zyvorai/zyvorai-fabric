// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: LicenseRef-Zyvor-Production-2.0

use reqwest::Client;
use serde_json::json;

const API_BASE: &str = "http://localhost:8080/api";

#[tokio::test]
async fn test_health_endpoint() {
    let client = Client::new();
    let resp = client
        .get("http://localhost:8080/health")
        .send()
        .await
        .unwrap();

    assert_eq!(resp.status(), 200);
    let body = resp.text().await.unwrap();
    assert_eq!(body, "OK");
}

#[tokio::test]
async fn test_list_vms() {
    let client = Client::new();
    let resp = client
        .get(format!("{}/vms", API_BASE))
        .send()
        .await
        .unwrap();

    assert_eq!(resp.status(), 200);
    let vms: serde_json::Value = resp.json().await.unwrap();
    assert!(vms.is_array());
}

#[tokio::test]
async fn test_create_vm() {
    let client = Client::new();

    let vm_req = json!({
        "name": "test-vm",
        "image": "/tmp/test.qcow2",
        "cpus": 2,
        "memory": 2048
    });

    let resp = client
        .post(format!("{}/vms", API_BASE))
        .json(&vm_req)
        .send()
        .await
        .unwrap();

    assert!(resp.status().is_success());
}

#[tokio::test]
async fn test_vm_lifecycle() {
    let client = Client::new();
    let vm_name = "lifecycle-test-vm";

    // Create VM
    let create_req = json!({
        "name": vm_name,
        "image": "/tmp/test.qcow2",
        "cpus": 1,
        "memory": 1024
    });

    let _ = client
        .post(format!("{}/vms", API_BASE))
        .json(&create_req)
        .send()
        .await
        .unwrap();

    // Get VM
    let resp = client
        .get(format!("{}/vms/{}", API_BASE, vm_name))
        .send()
        .await
        .unwrap();

    assert_eq!(resp.status(), 200);

    // Delete VM
    let resp = client
        .delete(format!("{}/vms/{}", API_BASE, vm_name))
        .send()
        .await
        .unwrap();

    assert_eq!(resp.status(), 204);
}

#[tokio::test]
async fn test_metrics_endpoint() {
    let client = Client::new();
    let resp = client
        .get("http://localhost:8080/metrics")
        .send()
        .await
        .unwrap();

    assert_eq!(resp.status(), 200);
    let body = resp.text().await.unwrap();
    assert!(body.contains("zyvor_fabricd_"));
}
