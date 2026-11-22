// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: LicenseRef-Zyvor-Production-2.0

//! Router tests for `POST /api/vms/{name}/preflight`.

use axum::{
    body::Body,
    http::{Request, StatusCode},
    Router,
};
use serde_json::{json, Value};
use std::sync::Arc;
use tokio::sync::RwLock;
use tower::ServiceExt;

use zyvor_fabricd::config::{
    AuthConfig, Config, ControllerConfig, DaemonConfig, DriverConfig, NetworkConfig, StorageConfig,
    TlsConfig,
};
use zyvor_fabricd::server::{AppState, QuotaCache};

mod common;

async fn json_body(resp: axum::response::Response) -> Value {
    let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX)
        .await
        .unwrap();
    serde_json::from_slice(&bytes).unwrap()
}

fn post(uri: &str, body: Option<Value>) -> Request<Body> {
    let mut b = Request::builder().method("POST").uri(uri);
    match body {
        Some(v) => {
            b = b.header("content-type", "application/json");
            b.body(Body::from(v.to_string())).unwrap()
        }
        None => b.body(Body::empty()).unwrap(),
    }
}

async fn create_vm(app: &Router, name: &str) {
    let req = post(
        "/api/vms",
        Some(json!({"name": name, "image": "test.qcow2", "cpus": 1, "memory": 512})),
    );
    let resp = app.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::CREATED);
}

async fn vm_state(app: &Router, name: &str) -> Value {
    let req = Request::builder()
        .method("GET")
        .uri(format!("/api/vms/{name}"))
        .body(Body::empty())
        .unwrap();
    let resp = app.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    json_body(resp).await
}

#[tokio::test]
async fn preflight_returns_findings_and_can_start() {
    let app = common::create_test_app().await;
    create_vm(&app, "pf-normal").await;

    let resp = app
        .clone()
        .oneshot(post("/api/vms/pf-normal/preflight", None))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body = json_body(resp).await;
    assert_eq!(body["vm"], "pf-normal");
    assert_eq!(body["backend"], "fluxvm");
    assert!(body["findings"].is_array());
    // A normal small VM must not be blocked.
    assert_eq!(body["can_start"], true, "{body}");
    assert_eq!(body["summary"]["blockers"], 0);
}

#[tokio::test]
async fn preflight_reports_unsupported_options_as_blockers() {
    let app = common::create_test_app().await;
    create_vm(&app, "pf-tpm").await;

    let resp = app
        .clone()
        .oneshot(post(
            "/api/vms/pf-tpm/preflight",
            Some(json!({"tpm": true})),
        ))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body = json_body(resp).await;
    assert_eq!(body["can_start"], false);
    let f = &body["findings"][0];
    assert_eq!(f["id"], "backend.tpm");
    assert_eq!(f["severity"], "blocker");
    assert!(f["hint"].as_str().unwrap().len() > 3);
}

#[tokio::test]
async fn preflight_is_read_only() {
    let app = common::create_test_app().await;
    create_vm(&app, "pf-readonly").await;
    let before = vm_state(&app, "pf-readonly").await;

    for _ in 0..2 {
        let resp = app
            .clone()
            .oneshot(post("/api/vms/pf-readonly/preflight", None))
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
    }
    let after = vm_state(&app, "pf-readonly").await;
    assert_eq!(before, after, "preflight must not mutate the VM");
    assert_eq!(after["state"], "stopped");
}

#[tokio::test]
async fn preflight_unknown_vm_is_404() {
    let app = common::create_test_app().await;
    let resp = app
        .oneshot(post("/api/vms/no-such-vm/preflight", None))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn start_rejects_unsupported_backend_options_with_clear_4xx() {
    let app = common::create_test_app().await;
    create_vm(&app, "pf-start-tpm").await;

    let resp = app
        .clone()
        .oneshot(post(
            "/api/vms/pf-start-tpm/start",
            Some(json!({"secure_boot": true})),
        ))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    let body = json_body(resp).await;
    assert!(
        body.to_string().contains("Secure Boot"),
        "unexpected error body: {body}"
    );
    // The VM must not have been moved to Starting.
    assert_eq!(vm_state(&app, "pf-start-tpm").await["state"], "stopped");
}

// ─── Auth ────────────────────────────────────────────────────────────────────

async fn jwt_app() -> (Router, Arc<security::JwtConfig>) {
    let tmp_dir =
        std::env::temp_dir().join(format!("zyvor-fabricd-test-pf-jwt-{}", std::process::id()));
    let store_dir = tmp_dir.join("store");
    let storage_dir = tmp_dir.join("storage");
    std::fs::create_dir_all(&store_dir).unwrap();
    std::fs::create_dir_all(&storage_dir).unwrap();

    let store = state_store::StateStore::new(&store_dir).unwrap();
    let config = Config {
        daemon: DaemonConfig {
            listen: "127.0.0.1:0".to_string(),
            cors_origins: vec![],
            public_url: None,
        },
        storage: StorageConfig {
            path: tmp_dir.to_string_lossy().to_string(),
            image_path: tmp_dir.join("images").to_string_lossy().to_string(),
            atlas_base_url: None,
        },
        network: NetworkConfig {
            bridge: "br-test".to_string(),
            networkd_config_dir: tmp_dir.join("networkd").to_string_lossy().to_string(),
            networkd_file_prefix: "50-zyvor-fabricd-".to_string(),
            hubble_ui_url: None,
        },
        controller: ControllerConfig::default(),
        auth: AuthConfig {
            enabled: false,
            ..AuthConfig::default()
        },
        driver: DriverConfig::default(),
        tls: TlsConfig {
            enabled: false,
            ..TlsConfig::default()
        },
        container_groups: zyvor_fabricd::config::ContainerGroupsConfig::default(),
        agent_runtime: zyvor_fabricd::config::AgentRuntimeConfig::default(),
        metrics: zyvor_fabricd::config::MetricsConfig::default(),
        autostart: zyvor_fabricd::config::AutostartConfig::default(),
    };
    let storage_manager = zyvor_fabric_storage::StorageManager::new(&storage_dir).unwrap();
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    drop(listener);
    let driver =
        zyvor_fabric_fluxvm_driver::FluxVmDriver::new(&format!("http://127.0.0.1:{port}")).unwrap();
    let jwt = Arc::new(security::JwtConfig::new(
        "preflight-test-secret-0123456789abcdef".into(),
    ));

    let state = Arc::new(AppState {
        store,
        config,
        storage_manager: Arc::new(RwLock::new(storage_manager)),
        http_client: reqwest::Client::new(),
        quota_cache: Arc::new(tokio::sync::RwLock::new(QuotaCache::new())),
        user_db: None,
        jwt_config: Some(jwt.clone()),
        plugin_registry: Arc::new(RwLock::new(zyvor_fabricd::plugins::PluginRegistry::new())),
        driver: Arc::new(driver),
        dnsmasq_manager: Arc::new(zyvor_fabric_dnsmasq_manager::DnsmasqManager::new(
            tmp_dir.join("dnsmasq"),
        )),
        lock_manager: Arc::new(zyvor_fabric_lock_manager::LockManager::new(
            zyvor_fabric_lock_manager::LockConfig::default(),
        )),
        policy_engine: Arc::new(network_policy::PolicyEngine::new()),
        service_mesh: Arc::new(service_mesh::ServiceMesh::new()),
        traffic_shaper: Arc::new(traffic_shaping::TrafficShaper::new()),
        dns_manager: Arc::new(dns_policy::DnsManager::new()),
        vm_firewall: Arc::new(vm_firewall::VMFirewall::new()),
        vpn_mesh: Arc::new(vpn_mesh::VpnMesh::new()),
        packet_mirror: Arc::new(packet_mirror::PacketMirror::new()),
        nat_gateway: Arc::new(nat_gateway::NatGateway::new()),
        net_monitor: Arc::new(net_monitor::NetMonitor::new()),
        secrets_manager: Arc::new(secrets_manager::SecretsManager::new()),
        event_tx: {
            let (tx, _) = tokio::sync::broadcast::channel(256);
            tx
        },
        vm_locks: Arc::new(std::sync::Mutex::new(std::collections::HashMap::new())),
        shutdown: tokio_util::sync::CancellationToken::new(),
        k8s_pod_client: None,
    });
    (zyvor_fabricd::server::build_router(state), jwt)
}

fn with_token(mut req: Request<Body>, token: &str) -> Request<Body> {
    req.headers_mut()
        .insert("authorization", format!("Bearer {token}").parse().unwrap());
    req
}

#[tokio::test]
async fn preflight_requires_authentication_but_only_read_role() {
    let (app, jwt) = jwt_app().await;
    let admin = jwt.generate_token("admin", security::Role::Admin).unwrap();
    let viewer = jwt
        .generate_token("viewer", security::Role::Viewer)
        .unwrap();

    let create = with_token(
        post(
            "/api/vms",
            Some(json!({"name": "pf-auth", "image": "test.qcow2", "cpus": 1, "memory": 512})),
        ),
        &admin,
    );
    let resp = app.clone().oneshot(create).await.unwrap();
    assert_eq!(resp.status(), StatusCode::CREATED);

    // No token: rejected.
    let resp = app
        .clone()
        .oneshot(post("/api/vms/pf-auth/preflight", None))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);

    // Garbage token: rejected.
    let resp = app
        .clone()
        .oneshot(with_token(post("/api/vms/pf-auth/preflight", None), "nope"))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);

    // Viewer (read-only) is enough.
    let resp = app
        .clone()
        .oneshot(with_token(
            post("/api/vms/pf-auth/preflight", None),
            &viewer,
        ))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
}
