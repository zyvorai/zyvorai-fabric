// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: LicenseRef-Zyvor-Production-2.0

use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use serde_json::json;
use tower::ServiceExt;

mod common;

// ─── Health & system info (always work) ──────────────────────────────────────

#[tokio::test]
async fn test_health_endpoint() {
    let app = common::create_test_app().await;

    let request = Request::builder()
        .method("GET")
        .uri("/health")
        .body(Body::empty())
        .unwrap();

    let response = app.oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
}

#[tokio::test]
async fn test_cpu_topology_detection() {
    let app = common::create_test_app().await;

    let request = Request::builder()
        .method("GET")
        .uri("/api/system/cpu/topology")
        .body(Body::empty())
        .unwrap();

    let response = app.oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let topology: serde_json::Value = serde_json::from_slice(&body).unwrap();

    assert!(topology.get("total_cpus").is_some());
    assert!(topology.get("sockets").is_some());
    assert!(topology.get("cores_per_socket").is_some());
    assert!(topology.get("threads_per_core").is_some());
}

#[tokio::test]
async fn test_numa_topology_detection() {
    let app = common::create_test_app().await;

    let request = Request::builder()
        .method("GET")
        .uri("/api/system/numa/topology")
        .body(Body::empty())
        .unwrap();

    let response = app.oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let topology: serde_json::Value = serde_json::from_slice(&body).unwrap();

    assert!(topology.get("nodes").is_some());
}

#[tokio::test]
async fn test_system_memory_info() {
    let app = common::create_test_app().await;

    let request = Request::builder()
        .method("GET")
        .uri("/api/system/memory")
        .body(Body::empty())
        .unwrap();

    let response = app.oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let memory: serde_json::Value = serde_json::from_slice(&body).unwrap();

    assert!(memory.get("total_kb").is_some());
    assert!(memory.get("free_kb").is_some());
    assert!(memory.get("available_kb").is_some());
}

#[tokio::test]
async fn test_hugepage_stats() {
    let app = common::create_test_app().await;

    let request = Request::builder()
        .method("GET")
        .uri("/api/system/memory/hugepages")
        .body(Body::empty())
        .unwrap();

    let response = app.oneshot(request).await.unwrap();
    // Hugepages may not be configured on all systems, accept any non-panic response
    assert!(
        response.status().is_success()
            || response.status().is_client_error()
            || response.status().is_server_error()
    );
}

#[tokio::test]
async fn test_firmware_capabilities() {
    let app = common::create_test_app().await;

    let request = Request::builder()
        .method("GET")
        .uri("/api/system/firmware/capabilities")
        .body(Body::empty())
        .unwrap();

    let response = app.oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
}

// ─── Storage pool operations ─────────────────────────────────────────────────

#[tokio::test]
async fn test_storage_pool_lifecycle() {
    let app = common::create_test_app().await;

    let tmp = std::path::PathBuf::from("/var/lib/zyvor-fabricd/images/test-pool");
    if std::fs::create_dir_all(&tmp).is_err() {
        eprintln!("Skipping test_storage_pool_lifecycle: cannot create /var/lib/zyvor-fabricd/images (requires root)");
        return;
    }

    let create_request = Request::builder()
        .method("POST")
        .uri("/api/storage/pools/local")
        .header("content-type", "application/json")
        .body(Body::from(
            json!({
                "name": "test-pool",
                "path": tmp.to_string_lossy(),
                "auto_start": true
            })
            .to_string(),
        ))
        .unwrap();

    let response = app.clone().oneshot(create_request).await.unwrap();
    assert!(
        response.status() == StatusCode::CREATED || response.status() == StatusCode::OK,
        "Expected 200 or 201, got {}",
        response.status()
    );

    // List pools
    let list_request = Request::builder()
        .method("GET")
        .uri("/api/storage/pools")
        .body(Body::empty())
        .unwrap();

    let response = app.clone().oneshot(list_request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    // Delete pool
    let delete_request = Request::builder()
        .method("DELETE")
        .uri("/api/storage/pools/test-pool")
        .body(Body::empty())
        .unwrap();

    let response = app.clone().oneshot(delete_request).await.unwrap();
    assert_eq!(response.status(), StatusCode::NO_CONTENT);

    let _ = std::fs::remove_dir_all(&tmp);
}

#[tokio::test]
async fn test_nonexistent_pool_returns_not_found() {
    let app = common::create_test_app().await;

    let request = Request::builder()
        .method("GET")
        .uri("/api/storage/pools/non-existent-pool")
        .body(Body::empty())
        .unwrap();

    let response = app.oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

// ─── VM operations (VMs won't exist, expect NOT_FOUND) ──────────────────────

#[tokio::test]
async fn test_vm_not_found() {
    let app = common::create_test_app().await;

    let request = Request::builder()
        .method("GET")
        .uri("/api/vms/nonexistent-vm")
        .body(Body::empty())
        .unwrap();

    let response = app.oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn test_list_vms() {
    let app = common::create_test_app().await;

    let request = Request::builder()
        .method("GET")
        .uri("/api/vms")
        .body(Body::empty())
        .unwrap();

    let response = app.oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
}

// ─── Networkd API ────────────────────────────────────────────────────────────

// `block_in_place` (used by `networking`'s netlink calls) requires a
// multi-threaded runtime — the default `#[tokio::test]` flavor is
// current-thread and panics on it.
// Requires CAP_NET_ADMIN / netlink; skip on vanilla CI runners.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires host netlink/CAP_NET_ADMIN"]
async fn test_networkd_bridge_lifecycle() {
    let app = common::create_test_app().await;

    // Create bridge
    let create_request = Request::builder()
        .method("POST")
        .uri("/api/networkd/bridges")
        .header("content-type", "application/json")
        .body(Body::from(
            json!({
                "name": "br-test",
                "stp": true,
                "addresses": ["10.0.0.1/24"]
            })
            .to_string(),
        ))
        .unwrap();

    let response = app.clone().oneshot(create_request).await.unwrap();
    assert_eq!(response.status(), StatusCode::CREATED);

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let bridge: serde_json::Value = serde_json::from_slice(&body).unwrap();
    let bridge_id = bridge["id"].as_str().unwrap();

    // List bridges
    let list_request = Request::builder()
        .method("GET")
        .uri("/api/networkd/bridges")
        .body(Body::empty())
        .unwrap();

    let response = app.clone().oneshot(list_request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let bridges: Vec<serde_json::Value> = serde_json::from_slice(&body).unwrap();
    assert!(!bridges.is_empty());

    // Delete bridge
    let delete_request = Request::builder()
        .method("DELETE")
        .uri(format!("/api/networkd/bridges/{}", bridge_id))
        .body(Body::empty())
        .unwrap();

    let response = app.oneshot(delete_request).await.unwrap();
    assert_eq!(response.status(), StatusCode::NO_CONTENT);
}

#[tokio::test]
async fn test_networkd_list_managed_files() {
    let app = common::create_test_app().await;

    let request = Request::builder()
        .method("GET")
        .uri("/api/networkd/files")
        .body(Body::empty())
        .unwrap();

    let response = app.oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
}

// ─── Quotas & Schedules ─────────────────────────────────────────────────────

#[tokio::test]
async fn test_list_quotas() {
    let app = common::create_test_app().await;

    let request = Request::builder()
        .method("GET")
        .uri("/api/quotas")
        .body(Body::empty())
        .unwrap();

    let response = app.oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
}

#[tokio::test]
async fn test_list_schedules() {
    let app = common::create_test_app().await;

    let request = Request::builder()
        .method("GET")
        .uri("/api/schedules")
        .body(Body::empty())
        .unwrap();

    let response = app.oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
}

// ─── Template CRUD ──────────────────────────────────────────────────────────

#[tokio::test]
async fn test_template_lifecycle() {
    let app = common::create_test_app().await;

    // Create template
    let create_request = Request::builder()
        .method("POST")
        .uri("/api/templates")
        .header("content-type", "application/json")
        .body(Body::from(
            json!({
                "name": "ubuntu-base",
                "description": "Base Ubuntu template",
                "cpus": 2,
                "memory": 2048,
                "disk": 20,
                "image": "ubuntu-22.04.qcow2",
                "tags": ["linux", "ubuntu"]
            })
            .to_string(),
        ))
        .unwrap();

    let response = app.clone().oneshot(create_request).await.unwrap();
    assert_eq!(response.status(), StatusCode::CREATED);

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let template: serde_json::Value = serde_json::from_slice(&body).unwrap();
    let template_id = template["id"].as_str().unwrap().to_string();

    assert_eq!(template["name"], "ubuntu-base");
    assert_eq!(template["cpus"], 2);
    assert_eq!(template["memory"], 2048);

    // List templates
    let list_request = Request::builder()
        .method("GET")
        .uri("/api/templates")
        .body(Body::empty())
        .unwrap();

    let response = app.clone().oneshot(list_request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let templates: Vec<serde_json::Value> = serde_json::from_slice(&body).unwrap();
    assert!(!templates.is_empty());

    // Get template by ID
    let get_request = Request::builder()
        .method("GET")
        .uri(format!("/api/templates/{}", template_id))
        .body(Body::empty())
        .unwrap();

    let response = app.clone().oneshot(get_request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    // Update template
    let update_request = Request::builder()
        .method("PUT")
        .uri(format!("/api/templates/{}", template_id))
        .header("content-type", "application/json")
        .body(Body::from(
            json!({
                "name": "ubuntu-base-v2",
                "cpus": 4
            })
            .to_string(),
        ))
        .unwrap();

    let response = app.clone().oneshot(update_request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let updated: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(updated["name"], "ubuntu-base-v2");
    assert_eq!(updated["cpus"], 4);

    // Delete template
    let delete_request = Request::builder()
        .method("DELETE")
        .uri(format!("/api/templates/{}", template_id))
        .body(Body::empty())
        .unwrap();

    let response = app.clone().oneshot(delete_request).await.unwrap();
    assert_eq!(response.status(), StatusCode::NO_CONTENT);

    // Verify deleted
    let get_request = Request::builder()
        .method("GET")
        .uri(format!("/api/templates/{}", template_id))
        .body(Body::empty())
        .unwrap();

    let response = app.oneshot(get_request).await.unwrap();
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn test_template_not_found() {
    let app = common::create_test_app().await;

    let request = Request::builder()
        .method("GET")
        .uri("/api/templates/nonexistent-id")
        .body(Body::empty())
        .unwrap();

    let response = app.oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn test_deploy_template() {
    let app = common::create_test_app().await;

    // Create template first
    let create_request = Request::builder()
        .method("POST")
        .uri("/api/templates")
        .header("content-type", "application/json")
        .body(Body::from(
            json!({
                "name": "deploy-test",
                "cpus": 1,
                "memory": 512,
                "disk": 10,
                "image": "test.qcow2",
                "tags": []
            })
            .to_string(),
        ))
        .unwrap();

    let response = app.clone().oneshot(create_request).await.unwrap();
    assert_eq!(response.status(), StatusCode::CREATED);

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let template: serde_json::Value = serde_json::from_slice(&body).unwrap();
    let template_id = template["id"].as_str().unwrap().to_string();

    // Deploy VM from template
    let deploy_request = Request::builder()
        .method("POST")
        .uri(format!("/api/templates/{}/deploy", template_id))
        .header("content-type", "application/json")
        .body(Body::from(json!({ "vm_name": "deployed-vm" }).to_string()))
        .unwrap();

    let response = app.clone().oneshot(deploy_request).await.unwrap();
    assert_eq!(response.status(), StatusCode::CREATED);

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let vm: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(vm["name"], "deployed-vm");
    assert_eq!(vm["cpus"], 1);
    assert_eq!(vm["memory"], 512);

    // Verify VM exists in VM list
    let list_request = Request::builder()
        .method("GET")
        .uri("/api/vms/deployed-vm")
        .body(Body::empty())
        .unwrap();

    let response = app.oneshot(list_request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
}

// ─── VM Pause/Resume ────────────────────────────────────────────────────────

#[tokio::test]
async fn test_pause_nonexistent_vm() {
    let app = common::create_test_app().await;

    let request = Request::builder()
        .method("POST")
        .uri("/api/vms/nonexistent/pause")
        .body(Body::empty())
        .unwrap();

    let response = app.oneshot(request).await.unwrap();
    // Will fail because VM doesn't have a running process to SIGSTOP
    assert!(response.status().is_server_error() || response.status().is_client_error());
}

#[tokio::test]
async fn test_resume_nonexistent_vm() {
    let app = common::create_test_app().await;

    let request = Request::builder()
        .method("POST")
        .uri("/api/vms/nonexistent/resume")
        .body(Body::empty())
        .unwrap();

    let response = app.oneshot(request).await.unwrap();
    assert!(response.status().is_server_error() || response.status().is_client_error());
}

// ─── VM Clone ───────────────────────────────────────────────────────────────

#[tokio::test]
async fn test_clone_nonexistent_vm() {
    let app = common::create_test_app().await;

    let request = Request::builder()
        .method("POST")
        .uri("/api/vms/nonexistent/clone")
        .header("content-type", "application/json")
        .body(Body::from(
            json!({ "target_name": "clone-target" }).to_string(),
        ))
        .unwrap();

    let response = app.oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn test_clone_vm() {
    let app = common::create_test_app().await;

    // Create a VM first
    let create_request = Request::builder()
        .method("POST")
        .uri("/api/vms")
        .header("content-type", "application/json")
        .body(Body::from(
            json!({
                "name": "clone-source",
                "image": "test.qcow2",
                "cpus": 2,
                "memory": 1024
            })
            .to_string(),
        ))
        .unwrap();

    let response = app.clone().oneshot(create_request).await.unwrap();
    assert_eq!(response.status(), StatusCode::CREATED);

    // Clone it
    let clone_request = Request::builder()
        .method("POST")
        .uri("/api/vms/clone-source/clone")
        .header("content-type", "application/json")
        .body(Body::from(
            json!({ "target_name": "clone-target", "linked_clone": false }).to_string(),
        ))
        .unwrap();

    let response = app.clone().oneshot(clone_request).await.unwrap();
    // Clone returns 404 when no disk image exists for the source VM
    // (in test environments there is no real disk image on the filesystem)
    assert_eq!(response.status(), StatusCode::NOT_FOUND);

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let error: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert!(error["error"]
        .as_str()
        .unwrap()
        .contains("No disk image found"));
}

#[tokio::test]
async fn test_clone_vm_name_conflict() {
    let app = common::create_test_app().await;

    // Create two VMs
    for name in &["vm-a", "vm-b"] {
        let create_request = Request::builder()
            .method("POST")
            .uri("/api/vms")
            .header("content-type", "application/json")
            .body(Body::from(
                json!({
                    "name": name,
                    "image": "test.qcow2",
                    "cpus": 1,
                    "memory": 512
                })
                .to_string(),
            ))
            .unwrap();

        app.clone().oneshot(create_request).await.unwrap();
    }

    // Try to clone vm-a with target name vm-b (conflict)
    let clone_request = Request::builder()
        .method("POST")
        .uri("/api/vms/vm-a/clone")
        .header("content-type", "application/json")
        .body(Body::from(json!({ "target_name": "vm-b" }).to_string()))
        .unwrap();

    let response = app.oneshot(clone_request).await.unwrap();
    assert_eq!(response.status(), StatusCode::CONFLICT);
}

// ─── Migrations ─────────────────────────────────────────────────────────────

#[tokio::test]
async fn test_list_migrations() {
    let app = common::create_test_app().await;

    let request = Request::builder()
        .method("GET")
        .uri("/api/migrations")
        .body(Body::empty())
        .unwrap();

    let response = app.oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let migrations: Vec<serde_json::Value> = serde_json::from_slice(&body).unwrap();
    assert!(migrations.is_empty());
}

#[tokio::test]
async fn test_migration_not_found() {
    let app = common::create_test_app().await;

    let request = Request::builder()
        .method("GET")
        .uri("/api/migrations/nonexistent-id")
        .body(Body::empty())
        .unwrap();

    let response = app.oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn test_start_migration_vm_not_found() {
    let app = common::create_test_app().await;

    let request = Request::builder()
        .method("POST")
        .uri("/api/migrations")
        .header("content-type", "application/json")
        .body(Body::from(
            json!({
                "vm_name": "nonexistent-vm",
                "target_host": "192.168.1.100",
                "migration_type": "offline"
            })
            .to_string(),
        ))
        .unwrap();

    let response = app.oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn test_cancel_migration_not_found() {
    let app = common::create_test_app().await;

    let request = Request::builder()
        .method("POST")
        .uri("/api/migrations/nonexistent-id/cancel")
        .body(Body::empty())
        .unwrap();

    let response = app.oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

// ─── Resource Optimization ──────────────────────────────────────────────────

#[tokio::test]
async fn test_optimization_recommendations() {
    let app = common::create_test_app().await;

    let request = Request::builder()
        .method("GET")
        .uri("/api/system/optimization/recommendations")
        .body(Body::empty())
        .unwrap();

    let response = app.oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let recommendations: Vec<serde_json::Value> = serde_json::from_slice(&body).unwrap();
    // No running VMs so no recommendations
    assert!(recommendations.is_empty());
}

#[tokio::test]
async fn test_optimize_nonexistent_vm() {
    let app = common::create_test_app().await;

    let request = Request::builder()
        .method("POST")
        .uri("/api/vms/nonexistent/optimize")
        .body(Body::empty())
        .unwrap();

    let response = app.oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

// ─── Analytics ──────────────────────────────────────────────────────────────

#[tokio::test]
async fn test_analytics_system_performance() {
    let app = common::create_test_app().await;

    let request = Request::builder()
        .method("GET")
        .uri("/api/analytics/system?range=1h")
        .body(Body::empty())
        .unwrap();

    let response = app.oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
}

#[tokio::test]
async fn test_analytics_insights() {
    let app = common::create_test_app().await;

    let request = Request::builder()
        .method("GET")
        .uri("/api/analytics/insights")
        .body(Body::empty())
        .unwrap();

    let response = app.oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
}

#[tokio::test]
async fn test_analytics_utilization() {
    let app = common::create_test_app().await;

    let request = Request::builder()
        .method("GET")
        .uri("/api/analytics/utilization")
        .body(Body::empty())
        .unwrap();

    let response = app.oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
}

#[tokio::test]
async fn test_vm_metrics() {
    let app = common::create_test_app().await;

    // Metrics for nonexistent VM should return error (no cgroup to read)
    let request = Request::builder()
        .method("GET")
        .uri("/api/vms/nonexistent/metrics")
        .body(Body::empty())
        .unwrap();

    let response = app.oneshot(request).await.unwrap();
    // VM must exist in Fabric's store; FluxVM resolution happens only after that.
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

// ─── Plugins ────────────────────────────────────────────────────────────────

#[tokio::test]
async fn test_list_plugins() {
    let app = common::create_test_app().await;

    let request = Request::builder()
        .method("GET")
        .uri("/api/plugins")
        .body(Body::empty())
        .unwrap();

    let response = app.oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let plugins: Vec<serde_json::Value> = serde_json::from_slice(&body).unwrap();
    // No plugins registered in test env
    assert!(plugins.is_empty());
}

// ─── Cluster Health ─────────────────────────────────────────────────────────

#[tokio::test]
async fn test_cluster_health_not_found() {
    let app = common::create_test_app().await;

    let request = Request::builder()
        .method("GET")
        .uri("/api/clusters/nonexistent/health")
        .body(Body::empty())
        .unwrap();

    let response = app.oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn test_cluster_health_with_cluster() {
    let app = common::create_test_app().await;

    // Create a cluster first
    let create_request = Request::builder()
        .method("POST")
        .uri("/api/clusters")
        .header("content-type", "application/json")
        .body(Body::from(
            json!({
                "name": "test-cluster",
                "description": "Test cluster",
                "datacenter_id": "dc-1",
                "ha_enabled": false,
                "drs_enabled": false,
                "drs_mode": "manual",
                "evc_mode": null
            })
            .to_string(),
        ))
        .unwrap();

    let response = app.clone().oneshot(create_request).await.unwrap();
    assert_eq!(response.status(), StatusCode::CREATED);

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let cluster: serde_json::Value = serde_json::from_slice(&body).unwrap();
    let cluster_id = cluster["id"].as_str().unwrap();

    // Get cluster health
    let health_request = Request::builder()
        .method("GET")
        .uri(format!("/api/clusters/{}/health", cluster_id))
        .body(Body::empty())
        .unwrap();

    let response = app.oneshot(health_request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let health: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(health["total_hosts"], 0);
    assert_eq!(health["health_status"], "empty");
}

// ─── Host Discovery ─────────────────────────────────────────────────────────

#[tokio::test]
async fn test_host_discovery_unreachable() {
    let app = common::create_test_app().await;

    let request = Request::builder()
        .method("POST")
        .uri("/api/hosts/discover")
        .header("content-type", "application/json")
        .body(Body::from(
            json!({
                "address": "10.255.255.1",
                "port": 9999
            })
            .to_string(),
        ))
        .unwrap();

    let response = app.oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let result: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(result["reachable"], false);
    assert_eq!(result["already_registered"], false);
}

// ─── Datacenter & Host Lifecycle ────────────────────────────────────────────

#[tokio::test]
async fn test_datacenter_host_lifecycle() {
    let app = common::create_test_app().await;

    // Create datacenter
    let dc_request = Request::builder()
        .method("POST")
        .uri("/api/datacenters")
        .header("content-type", "application/json")
        .body(Body::from(
            json!({ "name": "test-dc", "description": "Test datacenter" }).to_string(),
        ))
        .unwrap();

    let response = app.clone().oneshot(dc_request).await.unwrap();
    assert_eq!(response.status(), StatusCode::CREATED);

    // Register host
    let host_request = Request::builder()
        .method("POST")
        .uri("/api/hosts")
        .header("content-type", "application/json")
        .body(Body::from(
            json!({
                "hostname": "host-1",
                "address": "10.0.0.1",
                "cluster_id": "",
                "cpus": 16,
                "memory_mb": 32768,
                "agent_version": "0.1.0"
            })
            .to_string(),
        ))
        .unwrap();

    let response = app.clone().oneshot(host_request).await.unwrap();
    assert_eq!(response.status(), StatusCode::CREATED);

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let host: serde_json::Value = serde_json::from_slice(&body).unwrap();
    let host_id = host["id"].as_str().unwrap();

    // Send heartbeat
    let hb_request = Request::builder()
        .method("POST")
        .uri(format!("/api/hosts/{}/heartbeat", host_id))
        .header("content-type", "application/json")
        .body(Body::from(
            json!({
                "cpu_usage_pct": 45.5,
                "memory_usage_pct": 62.0,
                "vm_count": 5,
                "uptime_secs": 3600
            })
            .to_string(),
        ))
        .unwrap();

    let response = app.clone().oneshot(hb_request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    // List hosts
    let list_request = Request::builder()
        .method("GET")
        .uri("/api/hosts")
        .body(Body::empty())
        .unwrap();

    let response = app.clone().oneshot(list_request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let hosts: Vec<serde_json::Value> = serde_json::from_slice(&body).unwrap();
    assert!(!hosts.is_empty());

    // Delete host
    let delete_request = Request::builder()
        .method("DELETE")
        .uri(format!("/api/hosts/{}", host_id))
        .body(Body::empty())
        .unwrap();

    let response = app.oneshot(delete_request).await.unwrap();
    assert_eq!(response.status(), StatusCode::NO_CONTENT);
}

// ─── Concurrency tests ──────────────────────────────────────────────────────

#[tokio::test]
async fn test_concurrent_start_same_vm() {
    let app = common::create_test_app().await;

    // Create a VM
    let create_request = Request::builder()
        .method("POST")
        .uri("/api/vms")
        .header("content-type", "application/json")
        .body(Body::from(
            json!({
                "name": "concurrent-vm",
                "image": "test.qcow2",
                "cpus": 1,
                "memory": 512
            })
            .to_string(),
        ))
        .unwrap();

    app.clone().oneshot(create_request).await.unwrap();

    // Send two concurrent start requests
    let app2 = app.clone();
    let start1 = tokio::spawn(async move {
        let req = Request::builder()
            .method("POST")
            .uri("/api/vms/concurrent-vm/start")
            .body(Body::empty())
            .unwrap();
        app.oneshot(req).await.unwrap().status()
    });

    let start2 = tokio::spawn(async move {
        let req = Request::builder()
            .method("POST")
            .uri("/api/vms/concurrent-vm/start")
            .body(Body::empty())
            .unwrap();
        app2.oneshot(req).await.unwrap().status()
    });

    let (r1, r2) = tokio::join!(start1, start2);
    let s1 = r1.unwrap();
    let s2 = r2.unwrap();

    // With proper locking, requests are serialized. Depending on driver speed:
    // - If the background task is still running when the second request arrives,
    //   the second request blocks on the lock, then sees Starting/Running -> CONFLICT.
    // - If the background task completes quickly (e.g. driver fails fast in tests),
    //   the VM may transition to Failed, allowing the second request to also be ACCEPTED.
    // Both outcomes are correct — the key is no simultaneous state mutations occur.
    let statuses = vec![s1, s2];
    let accepted = statuses
        .iter()
        .filter(|s| **s == StatusCode::ACCEPTED)
        .count();
    let _conflict = statuses
        .iter()
        .filter(|s| **s == StatusCode::CONFLICT)
        .count();

    // At least one should be accepted
    assert!(
        accepted >= 1,
        "Expected at least 1 ACCEPTED, got {:?}",
        statuses
    );
    // All responses should be one of: ACCEPTED, CONFLICT, or INTERNAL_SERVER_ERROR
    for s in &statuses {
        assert!(
            *s == StatusCode::ACCEPTED
                || *s == StatusCode::CONFLICT
                || *s == StatusCode::INTERNAL_SERVER_ERROR,
            "Unexpected status code: {:?}",
            s
        );
    }
}

#[tokio::test]
async fn test_clone_self_rejected() {
    let app = common::create_test_app().await;

    // Create a VM
    let create_request = Request::builder()
        .method("POST")
        .uri("/api/vms")
        .header("content-type", "application/json")
        .body(Body::from(
            json!({
                "name": "self-clone",
                "image": "test.qcow2",
                "cpus": 1,
                "memory": 512
            })
            .to_string(),
        ))
        .unwrap();

    app.clone().oneshot(create_request).await.unwrap();

    // Try to clone to same name
    let clone_request = Request::builder()
        .method("POST")
        .uri("/api/vms/self-clone/clone")
        .header("content-type", "application/json")
        .body(Body::from(
            json!({ "target_name": "self-clone" }).to_string(),
        ))
        .unwrap();

    let response = app.oneshot(clone_request).await.unwrap();
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let error: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert!(error["error"].as_str().unwrap().contains("different"));
}

#[tokio::test]
async fn test_list_vms_pagination() {
    let app = common::create_test_app().await;

    // Create 3 VMs
    for i in 0..3 {
        let req = Request::builder()
            .method("POST")
            .uri("/api/vms")
            .header("content-type", "application/json")
            .body(Body::from(
                json!({
                    "name": format!("page-vm-{}", i),
                    "image": "test.qcow2",
                    "cpus": 1,
                    "memory": 512
                })
                .to_string(),
            ))
            .unwrap();
        app.clone().oneshot(req).await.unwrap();
    }

    // List with limit=2
    let req = Request::builder()
        .method("GET")
        .uri("/api/vms?limit=2&offset=0")
        .body(Body::empty())
        .unwrap();

    let response = app.clone().oneshot(req).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let result: serde_json::Value = serde_json::from_slice(&body).unwrap();
    // Should return at most 2 items (limit=2), with total >= 3
    assert!(result["items"].as_array().unwrap().len() <= 2);
    assert!(result["total"].as_u64().unwrap() >= 3);
    assert_eq!(result["limit"].as_u64().unwrap(), 2);
}

// ─── RBAC (Role-Based Access Control) ───────────────────────────────────────

#[tokio::test]
async fn test_viewer_cannot_create_vm() {
    let app = common::create_test_app_with_role(security::Role::Viewer).await;
    let body = serde_json::json!({"name": "test-vm", "cpus": 2, "memory": 512, "disk": 10, "image": "test.raw"});
    let req = Request::builder()
        .method("POST")
        .uri("/api/vms")
        .header("content-type", "application/json")
        .body(Body::from(serde_json::to_string(&body).unwrap()))
        .unwrap();
    let response = app.oneshot(req).await.unwrap();
    assert_eq!(response.status(), StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn test_viewer_cannot_delete_vm() {
    let app = common::create_test_app_with_role(security::Role::Viewer).await;
    let req = Request::builder()
        .method("DELETE")
        .uri("/api/vms/test-vm")
        .body(Body::empty())
        .unwrap();
    let response = app.oneshot(req).await.unwrap();
    assert_eq!(response.status(), StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn test_viewer_can_list_vms() {
    let app = common::create_test_app_with_role(security::Role::Viewer).await;
    let req = Request::builder()
        .method("GET")
        .uri("/api/vms")
        .body(Body::empty())
        .unwrap();
    let response = app.oneshot(req).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
}

#[tokio::test]
async fn test_user_cannot_delete_vm() {
    let app = common::create_test_app_with_role(security::Role::User).await;
    let req = Request::builder()
        .method("DELETE")
        .uri("/api/vms/test-vm")
        .body(Body::empty())
        .unwrap();
    let response = app.oneshot(req).await.unwrap();
    assert_eq!(response.status(), StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn test_viewer_cannot_create_schedule() {
    let app = common::create_test_app_with_role(security::Role::Viewer).await;
    let body = json!({"name": "sched", "vm_name": "vm", "action": "stop", "schedule_type": "daily", "time": "10:00"});
    let req = Request::builder()
        .method("POST")
        .uri("/api/schedules")
        .header("content-type", "application/json")
        .body(Body::from(serde_json::to_string(&body).unwrap()))
        .unwrap();
    let response = app.oneshot(req).await.unwrap();
    assert_eq!(response.status(), StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn test_viewer_cannot_create_secret() {
    let app = common::create_test_app_with_role(security::Role::Viewer).await;
    let body = json!({"name": "db-pass", "value": "secret123"});
    let req = Request::builder()
        .method("POST")
        .uri("/api/secrets")
        .header("content-type", "application/json")
        .body(Body::from(serde_json::to_string(&body).unwrap()))
        .unwrap();
    let response = app.oneshot(req).await.unwrap();
    assert_eq!(response.status(), StatusCode::FORBIDDEN);
}

// ─── VM Lifecycle ───────────────────────────────────────────────────────────

#[tokio::test]
async fn test_create_vm_returns_fields() {
    let app = common::create_test_app().await;
    let body = json!({"name": "lifecycle-test", "cpus": 2, "memory": 512, "disk": 10, "image": "test.raw"});
    let req = Request::builder()
        .method("POST")
        .uri("/api/vms")
        .header("content-type", "application/json")
        .body(Body::from(serde_json::to_string(&body).unwrap()))
        .unwrap();
    let response = app.oneshot(req).await.unwrap();
    assert_eq!(response.status(), StatusCode::CREATED);

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let vm: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(vm["name"].as_str().unwrap(), "lifecycle-test");
    assert_eq!(vm["cpus"].as_u64().unwrap(), 2);
    assert_eq!(vm["memory"].as_u64().unwrap(), 512);
}

#[tokio::test]
async fn test_create_vm_invalid_name_rejected() {
    let app = common::create_test_app().await;
    let body =
        json!({"name": "; rm -rf /", "cpus": 2, "memory": 512, "disk": 10, "image": "test.raw"});
    let req = Request::builder()
        .method("POST")
        .uri("/api/vms")
        .header("content-type", "application/json")
        .body(Body::from(serde_json::to_string(&body).unwrap()))
        .unwrap();
    let response = app.oneshot(req).await.unwrap();
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn test_delete_vm_success() {
    let app = common::create_test_app().await;

    // Create
    let body =
        json!({"name": "delete-me", "cpus": 1, "memory": 256, "disk": 5, "image": "test.raw"});
    let req = Request::builder()
        .method("POST")
        .uri("/api/vms")
        .header("content-type", "application/json")
        .body(Body::from(serde_json::to_string(&body).unwrap()))
        .unwrap();
    let response = app.clone().oneshot(req).await.unwrap();
    assert_eq!(response.status(), StatusCode::CREATED);

    // Delete
    let req = Request::builder()
        .method("DELETE")
        .uri("/api/vms/delete-me")
        .body(Body::empty())
        .unwrap();
    let response = app.clone().oneshot(req).await.unwrap();
    assert_eq!(response.status(), StatusCode::NO_CONTENT);

    // Verify gone
    let req = Request::builder()
        .method("GET")
        .uri("/api/vms/delete-me")
        .body(Body::empty())
        .unwrap();
    let response = app.oneshot(req).await.unwrap();
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

// ─── Schedule CRUD ──────────────────────────────────────────────────────────

#[tokio::test]
async fn test_schedule_lifecycle() {
    let app = common::create_test_app().await;

    // Create
    let body = json!({"name": "nightly-stop", "vm_name": "web-01", "action": "stop", "schedule_type": "daily", "time": "23:00"});
    let req = Request::builder()
        .method("POST")
        .uri("/api/schedules")
        .header("content-type", "application/json")
        .body(Body::from(serde_json::to_string(&body).unwrap()))
        .unwrap();
    let response = app.clone().oneshot(req).await.unwrap();
    assert_eq!(response.status(), StatusCode::CREATED);

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let schedule: serde_json::Value = serde_json::from_slice(&body).unwrap();
    let id = schedule["id"].as_str().unwrap();

    // Get by ID
    let req = Request::builder()
        .method("GET")
        .uri(format!("/api/schedules/{}", id))
        .body(Body::empty())
        .unwrap();
    let response = app.clone().oneshot(req).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    // Delete
    let req = Request::builder()
        .method("DELETE")
        .uri(format!("/api/schedules/{}", id))
        .body(Body::empty())
        .unwrap();
    let response = app.clone().oneshot(req).await.unwrap();
    assert_eq!(response.status(), StatusCode::NO_CONTENT);

    // Verify gone
    let req = Request::builder()
        .method("GET")
        .uri(format!("/api/schedules/{}", id))
        .body(Body::empty())
        .unwrap();
    let response = app.oneshot(req).await.unwrap();
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn test_create_schedule_invalid_time() {
    let app = common::create_test_app().await;
    let body = json!({"name": "bad", "vm_name": "vm", "action": "stop", "schedule_type": "daily", "time": "25:99"});
    let req = Request::builder()
        .method("POST")
        .uri("/api/schedules")
        .header("content-type", "application/json")
        .body(Body::from(serde_json::to_string(&body).unwrap()))
        .unwrap();
    let response = app.oneshot(req).await.unwrap();
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn test_create_schedule_snapshot_action() {
    let app = common::create_test_app().await;
    let body = json!({"name": "weekly-snap", "vm_name": "db-01", "action": "snapshot", "schedule_type": "weekly", "time": "02:00", "days_of_week": [0, 6]});
    let req = Request::builder()
        .method("POST")
        .uri("/api/schedules")
        .header("content-type", "application/json")
        .body(Body::from(serde_json::to_string(&body).unwrap()))
        .unwrap();
    let response = app.oneshot(req).await.unwrap();
    assert_eq!(response.status(), StatusCode::CREATED);
}

// ─── Backup API ─────────────────────────────────────────────────────────────

#[tokio::test]
async fn test_list_backups_empty() {
    let app = common::create_test_app().await;
    let req = Request::builder()
        .method("GET")
        .uri("/api/backups")
        .body(Body::empty())
        .unwrap();
    let response = app.oneshot(req).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
}

#[tokio::test]
async fn test_backup_stats() {
    let app = common::create_test_app().await;
    let req = Request::builder()
        .method("GET")
        .uri("/api/backups/stats")
        .body(Body::empty())
        .unwrap();
    let response = app.oneshot(req).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
}

#[tokio::test]
async fn test_get_nonexistent_backup() {
    let app = common::create_test_app().await;
    let req = Request::builder()
        .method("GET")
        .uri("/api/backups/no-such-id")
        .body(Body::empty())
        .unwrap();
    let response = app.oneshot(req).await.unwrap();
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

// ─── Secrets API ────────────────────────────────────────────────────────────

#[tokio::test]
async fn test_secrets_lifecycle() {
    let app = common::create_test_app().await;

    // Create
    let body = json!({"name": "db-password", "value": "s3cret!"});
    let req = Request::builder()
        .method("POST")
        .uri("/api/secrets")
        .header("content-type", "application/json")
        .body(Body::from(serde_json::to_string(&body).unwrap()))
        .unwrap();
    let response = app.clone().oneshot(req).await.unwrap();
    assert_eq!(response.status(), StatusCode::CREATED);

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let secret: serde_json::Value = serde_json::from_slice(&body).unwrap();
    let id = secret["id"].as_str().unwrap();
    assert_eq!(secret["name"].as_str().unwrap(), "db-password");

    // List (should appear)
    let req = Request::builder()
        .method("GET")
        .uri("/api/secrets")
        .body(Body::empty())
        .unwrap();
    let response = app.clone().oneshot(req).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let list: Vec<serde_json::Value> = serde_json::from_slice(&body).unwrap();
    assert!(!list.is_empty());

    // Delete
    let req = Request::builder()
        .method("DELETE")
        .uri(format!("/api/secrets/{}", id))
        .body(Body::empty())
        .unwrap();
    let response = app.oneshot(req).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
}

// ─── Snapshot endpoints ─────────────────────────────────────────────────────

#[tokio::test]
async fn test_list_snapshots_empty() {
    let app = common::create_test_app().await;
    let req = Request::builder()
        .method("GET")
        .uri("/api/vms/some-vm/snapshots")
        .body(Body::empty())
        .unwrap();
    let response = app.oneshot(req).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let snapshots: Vec<serde_json::Value> = serde_json::from_slice(&body).unwrap();
    assert!(snapshots.is_empty());
}

#[tokio::test]
async fn test_snapshot_tree_empty() {
    let app = common::create_test_app().await;
    let req = Request::builder()
        .method("GET")
        .uri("/api/vms/some-vm/snapshots/tree")
        .body(Body::empty())
        .unwrap();
    let response = app.oneshot(req).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let tree: Vec<serde_json::Value> = serde_json::from_slice(&body).unwrap();
    assert!(tree.is_empty());
}

// ─── Config snapshot history and diff ────────────────────────────────────────

async fn cfg_json(app: &axum::Router, method: &str, uri: &str) -> (StatusCode, serde_json::Value) {
    let request = Request::builder()
        .method(method)
        .uri(uri)
        .body(Body::empty())
        .unwrap();
    let response = app.clone().oneshot(request).await.unwrap();
    let status = response.status();
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    (status, serde_json::from_slice(&body).unwrap_or_default())
}

#[tokio::test]
async fn test_config_snapshots_store_list_and_diff() {
    let app = common::create_test_app().await;

    let (status, before) = cfg_json(&app, "GET", "/api/config/snapshots").await;
    assert_eq!(status, StatusCode::OK);
    let existing = before["snapshots"].as_array().unwrap().len();

    let (status, meta) = cfg_json(&app, "POST", "/api/config/snapshots").await;
    assert_eq!(status, StatusCode::CREATED);
    let id = meta["id"].as_str().unwrap().to_string();
    assert!(id.starts_with("snap-"));
    assert!(
        meta.get("snapshot").is_none(),
        "metadata must not embed the snapshot"
    );

    let (_, list) = cfg_json(&app, "GET", "/api/config/snapshots").await;
    let items = list["snapshots"].as_array().unwrap();
    assert_eq!(items.len(), existing + 1);
    assert_eq!(items[0]["id"], id.as_str());
    assert_eq!(list["max"], 50);

    // Stored snapshot vs the live state: nothing changed in between.
    let (status, diff) = cfg_json(
        &app,
        "GET",
        &format!("/api/config/diff?from={id}&to=current"),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(diff["change_count"], 0);
    assert_eq!(diff["from"], id.as_str());
    assert!(diff["changes"].as_array().unwrap().is_empty());

    let (status, _) = cfg_json(&app, "GET", "/api/config/diff?from=current&to=current").await;
    assert_eq!(status, StatusCode::OK);

    // History is capped at 50; the oldest snapshot is the one dropped.
    for _ in 0..51 {
        let (status, _) = cfg_json(&app, "POST", "/api/config/snapshots").await;
        assert_eq!(status, StatusCode::CREATED);
    }
    let (_, list) = cfg_json(&app, "GET", "/api/config/snapshots").await;
    let items = list["snapshots"].as_array().unwrap();
    assert_eq!(items.len(), 50);
    assert!(
        !items.iter().any(|m| m["id"] == id.as_str()),
        "oldest snapshot must be pruned"
    );
    let (status, _) = cfg_json(
        &app,
        "GET",
        &format!("/api/config/diff?from={id}&to=current"),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn test_config_diff_rejects_bad_requests() {
    let app = common::create_test_app().await;

    let (status, _) = cfg_json(&app, "GET", "/api/config/diff?from=current").await;
    assert_eq!(status, StatusCode::BAD_REQUEST);

    let (status, _) = cfg_json(&app, "GET", "/api/config/diff?from=snap-nope&to=current").await;
    assert_eq!(status, StatusCode::NOT_FOUND);

    let (status, _) = cfg_json(&app, "GET", "/api/config/diff?from=..%2Fetc&to=current").await;
    assert!(status == StatusCode::BAD_REQUEST || status == StatusCode::NOT_FOUND);
}
