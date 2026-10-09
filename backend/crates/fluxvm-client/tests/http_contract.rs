// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: Apache-2.0

//! HTTP contract tests against a wiremock FluxVM peer.

use chrono::{Duration, Utc};
use serde_json::json;
use uuid::Uuid;
use wiremock::matchers::{body_partial_json, header, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};
use zyvor_fabric_fluxvm_client::{
    CreateVmRequest, FluxVmClient, MigrationReceiverRequest, VmStatus,
};

fn sample_create_req() -> CreateVmRequest {
    serde_json::from_value(json!({
        "name": "recv",
        "backend": "qemu",
        "image": "/tmp/base.qcow2",
        "vcpus": 1,
        "memory_mib": 512
    }))
    .expect("CreateVmRequest")
}

#[tokio::test]
async fn migration_receiver_crud_paths_and_auth() {
    let server = MockServer::start().await;
    let id = Uuid::parse_str("00000000-0000-0000-0000-000000000001").unwrap();
    let expires = Utc::now() + Duration::seconds(300);

    Mock::given(method("POST"))
        .and(path("/v1/migration/receivers"))
        .and(header("authorization", "Bearer test-token"))
        .respond_with(ResponseTemplate::new(201).set_body_json(json!({
            "id": id,
            "status": "receiving",
            "port": 4444,
            "expires_at": expires
        })))
        .mount(&server)
        .await;

    Mock::given(method("GET"))
        .and(path(format!("/v1/migration/receivers/{id}")))
        .and(header("authorization", "Bearer test-token"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "id": id,
            "status": "receiving",
            "port": 4444,
            "expires_at": expires
        })))
        .mount(&server)
        .await;

    Mock::given(method("POST"))
        .and(path(format!("/v1/migration/receivers/{id}/activate")))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "id": id,
            "name": "recv",
            "backend": "qemu",
            "status": "running",
            "pid": null,
            "created_at": "2026-01-01T00:00:00Z",
            "expires_at": null,
            "workspace": "/tmp",
            "disk": "/tmp/disk.qcow2",
            "seed_disk": null,
            "tap_name": null,
            "control_socket": null,
            "log_path": "/tmp/log",
            "error": null,
            "request": sample_create_req(),
            "virtiofsd_pids": [],
            "dhcp_leasefile": null
        })))
        .mount(&server)
        .await;

    Mock::given(method("DELETE"))
        .and(path(format!("/v1/migration/receivers/{id}")))
        .respond_with(ResponseTemplate::new(204))
        .mount(&server)
        .await;

    let client = FluxVmClient::new(server.uri())
        .unwrap()
        .with_token("test-token");

    let created = client
        .create_migration_receiver(&MigrationReceiverRequest {
            spec: sample_create_req(),
            disk_path: "/tmp/disk.qcow2".into(),
            receiver_ttl_seconds: Some(60),
        })
        .await
        .unwrap();
    assert_eq!(created.port, 4444);

    let got = client.get_migration_receiver(id).await.unwrap();
    assert_eq!(got.id, id);

    let _ = client.activate_migration_receiver(id).await.unwrap();
    client.abort_migration_receiver(id).await.unwrap();
}

#[tokio::test]
async fn network_drop_reasons_and_pod_policy_paths() {
    let server = MockServer::start().await;
    let id = Uuid::new_v4();

    Mock::given(method("GET"))
        .and(path(format!("/v1/vms/{id}/network/drop-reasons")))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "items": [{
                "identity": 1,
                "family": 4,
                "source": "10.0.0.1",
                "destination": "10.0.0.2",
                "source_port": 1,
                "destination_port": 80,
                "protocol": 6,
                "reason_code": 8,
                "reason": "default-deny",
                "action": "drop",
                "packets": 1,
                "bytes": 64,
                "last_seen_ns": 1
            }]
        })))
        .mount(&server)
        .await;

    Mock::given(method("GET"))
        .and(path(format!("/v1/vms/{id}/network/pod-policy")))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!(null)))
        .mount(&server)
        .await;

    Mock::given(method("DELETE"))
        .and(path(format!("/v1/vms/{id}/network/pod-policy")))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"ok": true})))
        .mount(&server)
        .await;

    let client = FluxVmClient::new(server.uri()).unwrap();
    let drops = client.network_drop_reasons(id, Some(10)).await.unwrap();
    assert_eq!(drops.len(), 1);
    assert_eq!(drops[0].reason, "default-deny");

    let policy = client.get_pod_network_policy(id).await.unwrap();
    assert!(policy.is_none());
    client.delete_pod_network_policy(id).await.unwrap();
}

#[tokio::test]
async fn pause_resume_and_qga_ping_paths() {
    let server = MockServer::start().await;
    let id = Uuid::new_v4();

    Mock::given(method("POST"))
        .and(path(format!("/v1/vms/{id}/pause")))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "id": id,
            "name": "vm",
            "backend": "qemu",
            "status": "paused",
            "pid": null,
            "created_at": "2026-01-01T00:00:00Z",
            "expires_at": null,
            "workspace": "/tmp",
            "disk": "/tmp/disk.qcow2",
            "seed_disk": null,
            "tap_name": null,
            "control_socket": null,
            "log_path": "/tmp/log",
            "error": null,
            "request": sample_create_req(),
            "virtiofsd_pids": [],
            "dhcp_leasefile": null
        })))
        .mount(&server)
        .await;

    Mock::given(method("POST"))
        .and(path(format!("/v1/vms/{id}/resume")))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "id": id,
            "name": "vm",
            "backend": "qemu",
            "status": "running",
            "pid": null,
            "created_at": "2026-01-01T00:00:00Z",
            "expires_at": null,
            "workspace": "/tmp",
            "disk": "/tmp/disk.qcow2",
            "seed_disk": null,
            "tap_name": null,
            "control_socket": null,
            "log_path": "/tmp/log",
            "error": null,
            "request": sample_create_req(),
            "virtiofsd_pids": [],
            "dhcp_leasefile": null
        })))
        .mount(&server)
        .await;

    Mock::given(method("POST"))
        .and(path(format!("/v1/vms/{id}/qga/ping")))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"ok": true})))
        .mount(&server)
        .await;

    let client = FluxVmClient::new(server.uri()).unwrap();
    let paused = client.pause_vm(id).await.unwrap();
    assert_eq!(paused.status, VmStatus::Paused);
    let resumed = client.resume_vm(id).await.unwrap();
    assert_eq!(resumed.status, VmStatus::Running);
    client.qga_ping(id).await.unwrap();
}

fn vm_record(id: Uuid, name: &str, request: &CreateVmRequest) -> serde_json::Value {
    json!({
        "id": id,
        "name": name,
        "backend": "qemu",
        "status": "stopped",
        "pid": null,
        "created_at": "2026-01-01T00:00:00Z",
        "expires_at": null,
        "workspace": "/tmp",
        "disk": "/tmp/disk.qcow2",
        "seed_disk": null,
        "tap_name": null,
        "control_socket": null,
        "log_path": "/tmp/log",
        "error": null,
        "request": request,
        "virtiofsd_pids": [],
        "dhcp_leasefile": null
    })
}

#[tokio::test]
async fn create_vm_posts_direct_l2_uplink() {
    let server = MockServer::start().await;
    let id = Uuid::parse_str("00000000-0000-0000-0000-0000000000aa").unwrap();
    let req: CreateVmRequest = serde_json::from_value(json!({
        "name": "uplink",
        "backend": "qemu",
        "image": "/tmp/base.qcow2",
        "network": {
            "mode": "tap",
            "mac": "02:00:00:00:0a:0a",
            "direct": {
                "outer": "enp1s0",
                "mode": "l2-uplink",
                "guest_ips": ["192.168.1.50"]
            }
        }
    }))
    .expect("CreateVmRequest");

    Mock::given(method("POST"))
        .and(path("/v1/vms"))
        .and(body_partial_json(json!({
            "network": {
                "mode": "tap",
                "direct": {
                    "outer": "enp1s0",
                    "mode": "l2-uplink",
                    "guest_ips": ["192.168.1.50"]
                }
            }
        })))
        .respond_with(ResponseTemplate::new(201).set_body_json(vm_record(id, "uplink", &req)))
        .mount(&server)
        .await;

    let client = FluxVmClient::new(server.uri()).unwrap();
    let created = client.create_vm(&req).await.expect("create");
    assert_eq!(created.id, id);
    assert_eq!(created.name, "uplink");
}

#[tokio::test]
async fn create_and_delete_send_idempotency_key() {
    let server = MockServer::start().await;
    let id = Uuid::parse_str("00000000-0000-0000-0000-0000000000bb").unwrap();
    let req = sample_create_req();

    Mock::given(method("POST"))
        .and(path("/v1/vms"))
        .and(header("Idempotency-Key", "create-recv-1"))
        .respond_with(ResponseTemplate::new(201).set_body_json(vm_record(id, "recv", &req)))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("DELETE"))
        .and(path(format!("/v1/vms/{id}")))
        .and(header("Idempotency-Key", "delete-recv-1"))
        .respond_with(ResponseTemplate::new(204))
        .expect(1)
        .mount(&server)
        .await;

    let client = FluxVmClient::new(server.uri()).unwrap();
    let created = client
        .create_vm_idempotent(&req, "create-recv-1")
        .await
        .expect("create");
    assert_eq!(created.id, id);
    client
        .delete_vm_idempotent(id, "delete-recv-1")
        .await
        .expect("delete");

    // A malformed key is rejected before any request goes out.
    assert!(client.create_vm_idempotent(&req, "bad key").await.is_err());
}

#[tokio::test]
async fn host_gpu_list_bind_release_paths() {
    use zyvor_fabric_fluxvm_client::{GpuBindRequest, GpuReleaseRequest};

    let server = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path("/v1/host/gpus"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "items": [{
                "bdf": "0000:01:00.0",
                "vendor_id": 4318,
                "device_id": 9860,
                "vendor": "NVIDIA",
                "class_id": 770,
                "driver": "nvidia",
                "iommu_group": 12,
                "iommu_members": ["0000:01:00.0"],
                "group_bound_to_vfio": false,
                "group_held": false,
                "numa_node": 0,
                "vram_gib": 48
            }]
        })))
        .mount(&server)
        .await;

    Mock::given(method("POST"))
        .and(path("/v1/host/gpus/bind"))
        .and(body_partial_json(json!({ "bdf": "0000:01:00.0" })))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "bdf": "0000:01:00.0",
            "iommu_group": 12,
            "members": ["0000:01:00.0"],
            "previous_drivers": { "0000:01:00.0": "nvidia" }
        })))
        .mount(&server)
        .await;

    Mock::given(method("POST"))
        .and(path("/v1/host/gpus/release"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "bdf": "0000:01:00.0",
            "vendor_id": 4318,
            "device_id": 9860,
            "vendor": "NVIDIA",
            "class_id": 770,
            "driver": "vfio-pci",
            "iommu_group": 12,
            "iommu_members": ["0000:01:00.0"],
            "group_bound_to_vfio": true,
            "group_held": false,
            "numa_node": 0
        })))
        .mount(&server)
        .await;

    let client = FluxVmClient::new(server.uri()).unwrap();
    let gpus = client.list_host_gpus().await.unwrap();
    assert_eq!(gpus.len(), 1);
    assert_eq!(gpus[0].bdf, "0000:01:00.0");

    let bound = client
        .bind_host_gpu(&GpuBindRequest {
            bdf: "0000:01:00.0".into(),
            vram_gib: Some(48),
        })
        .await
        .unwrap();
    assert_eq!(bound.iommu_group, 12);

    let released = client
        .release_host_gpu(&GpuReleaseRequest {
            bdf: "0000:01:00.0".into(),
            restore_driver: false,
        })
        .await
        .unwrap();
    assert_eq!(released.bdf, "0000:01:00.0");
}
