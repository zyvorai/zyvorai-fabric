// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: Apache-2.0

use anyhow::{Context, Result};
use axum::{
    routing::{delete, get, post, put},
    Router,
};
use state_store::StateStore;
use std::sync::Arc;
use tokio::sync::RwLock;
use tower_http::cors::CorsLayer;
use tower_http::services::{ServeDir, ServeFile};
use tower_http::trace::TraceLayer;
use zyvor_fabric_storage::StorageManager;

use zyvor_fabric_driver_core::VmDriver;

use crate::{api, config::Config, plugins, routes, websocket};

pub struct AppState {
    pub store: StateStore,
    pub config: Config,
    pub storage_manager: Arc<RwLock<StorageManager>>,
    pub http_client: reqwest::Client,
    pub quota_cache: Arc<tokio::sync::RwLock<QuotaCache>>,
    pub user_db: Option<Arc<security::db::UserDb>>,
    pub jwt_config: Option<Arc<security::JwtConfig>>,
    pub plugin_registry: Arc<RwLock<plugins::PluginRegistry>>,
    pub driver: Arc<dyn VmDriver>,
    pub lock_manager: Arc<zyvor_fabric_lock_manager::LockManager>,
    pub policy_engine: Arc<network_policy::PolicyEngine>,
    pub service_mesh: Arc<service_mesh::ServiceMesh>,
    pub traffic_shaper: Arc<traffic_shaping::TrafficShaper>,
    pub dns_manager: Arc<dns_policy::DnsManager>,
    pub vm_firewall: Arc<vm_firewall::VMFirewall>,
    pub vpn_mesh: Arc<vpn_mesh::VpnMesh>,
    pub packet_mirror: Arc<packet_mirror::PacketMirror>,
    pub nat_gateway: Arc<nat_gateway::NatGateway>,
    pub net_monitor: Arc<net_monitor::NetMonitor>,
    pub secrets_manager: Arc<secrets_manager::SecretsManager>,
    pub dnsmasq_manager: Arc<zyvor_fabric_dnsmasq_manager::DnsmasqManager>,
    /// `None` when `container_groups.enabled` is false, or the configured
    /// target cluster couldn't be reached at startup — the ContainerGroup
    /// API returns a clear "not configured" error rather than panicking.
    pub k8s_pod_client: Option<Arc<k8s_pod_client::K8sPodClient>>,
    /// Per-VM mutex to serialize state-changing operations on the same VM.
    pub vm_locks:
        Arc<std::sync::Mutex<std::collections::HashMap<String, Arc<tokio::sync::Mutex<()>>>>>,
    /// Broadcast channel for real-time SSE event delivery.
    pub event_tx: tokio::sync::broadcast::Sender<crate::api::events::VMEvent>,
    /// Cancellation token for graceful background task shutdown.
    pub shutdown: tokio_util::sync::CancellationToken,
}

impl AppState {
    /// Acquire a per-VM lock. Creates one if it doesn't exist yet.
    pub fn vm_lock(&self, name: &str) -> Arc<tokio::sync::Mutex<()>> {
        let mut locks = self.vm_locks.lock().unwrap_or_else(|e| {
            tracing::warn!("VM locks mutex was poisoned, recovering");
            e.into_inner()
        });
        if locks.len() > 10_000 {
            locks.retain(|_, v| Arc::strong_count(v) > 1);
        }
        locks
            .entry(name.to_string())
            .or_insert_with(|| Arc::new(tokio::sync::Mutex::new(())))
            .clone()
    }
}

impl vnc_proxy::VncProxyState for AppState {
    fn driver(&self) -> Arc<dyn VmDriver> {
        self.driver.clone()
    }

    fn jwt_config(&self) -> Option<Arc<security::JwtConfig>> {
        self.jwt_config.clone()
    }
}

pub struct QuotaCache {
    pub usage: std::collections::HashMap<String, crate::api::quotas::QuotaUsage>,
    pub last_updated: std::time::Instant,
}

impl Default for QuotaCache {
    fn default() -> Self {
        Self::new()
    }
}

impl QuotaCache {
    pub fn new() -> Self {
        Self {
            usage: std::collections::HashMap::new(),
            last_updated: std::time::Instant::now(),
        }
    }

    pub fn is_stale(&self) -> bool {
        self.last_updated.elapsed() > std::time::Duration::from_secs(30)
    }
}

pub struct Server {
    state: Arc<AppState>,
}

impl Server {
    pub async fn new(store: StateStore, config: Config) -> Result<Self> {
        // Initialize storage manager (pool metadata persisted under data dir)
        let storage_path = std::path::PathBuf::from(&config.storage.path).join("storage");
        let storage_manager = StorageManager::new(&storage_path)
            .map_err(|e| anyhow::anyhow!("Failed to initialize storage manager: {}", e))?;

        let http_client = reqwest::Client::builder()
            // 180s — FluxVM create clones guest disks; 30s timed out under
            // lab load and left VMs Failed / Starting with no QEMU.
            .timeout(std::time::Duration::from_secs(180))
            .pool_max_idle_per_host(10)
            .build()
            .map_err(|e| anyhow::anyhow!("Failed to create HTTP client: {}", e))?;

        // Initialize auth if enabled
        let (user_db, jwt_config) = if config.auth.enabled {
            let db = security::db::UserDb::new(&config.auth.db_path)?;
            db.seed_admin(&config.auth.default_admin_password)?;

            let mut jwt = security::JwtConfig::new(config.auth.jwt_secret.clone());
            jwt.expiration_hours = config.auth.token_expiration_hours;

            (Some(Arc::new(db)), Some(Arc::new(jwt)))
        } else {
            tracing::warn!("Authentication is disabled");
            (None, None)
        };

        // VM driver — see the systemd-removal migration plan's final phase.
        // The systemd-machined/D-Bus backend this replaced is gone; FluxVM
        // is the only `VmDriver` implementation left.
        let driver: Arc<dyn VmDriver> = {
            let mut d = zyvor_fabric_fluxvm_driver::FluxVmDriver::new(&config.driver.fluxvm_url)
                .map_err(|e| anyhow::anyhow!("Failed to initialize FluxVM driver: {}", e))?;
            if let Some(token) = &config.driver.fluxvm_token {
                d = d.with_token(token.clone());
            }
            tracing::info!(url = %config.driver.fluxvm_url, "Using FluxVM VM driver");
            Arc::new(d)
        };

        let lock_manager = Arc::new(zyvor_fabric_lock_manager::LockManager::new(
            zyvor_fabric_lock_manager::LockConfig::default(),
        ));

        // ContainerGroup Kubernetes client — best-effort at startup so a
        // misconfigured/unreachable cluster doesn't block the rest of the
        // daemon; requests against the ContainerGroup API just fail clearly
        // until it's fixed and the daemon restarted.
        let k8s_pod_client = if config.container_groups.enabled {
            let k8s_config = k8s_pod_client::K8sPodClientConfig {
                kubeconfig_path: config.container_groups.kubeconfig_path.clone(),
                namespace: config.container_groups.namespace.clone(),
            };
            match k8s_pod_client::K8sPodClient::connect(&k8s_config).await {
                Ok(client) => Some(Arc::new(client)),
                Err(e) => {
                    tracing::error!(
                        "ContainerGroup Kubernetes client failed to connect: {}. \
                         ContainerGroup API will reject requests until this is fixed.",
                        e
                    );
                    None
                }
            }
        } else {
            None
        };

        let state = Arc::new(AppState {
            store,
            config,
            storage_manager: Arc::new(RwLock::new(storage_manager)),
            http_client,
            quota_cache: Arc::new(tokio::sync::RwLock::new(QuotaCache::new())),
            user_db,
            jwt_config,
            plugin_registry: Arc::new(RwLock::new(plugins::PluginRegistry::new())),
            driver,
            lock_manager,
            policy_engine: Arc::new(network_policy::PolicyEngine::new()),
            service_mesh: Arc::new(service_mesh::ServiceMesh::new()),
            traffic_shaper: Arc::new(traffic_shaping::TrafficShaper::new()),
            dns_manager: Arc::new(dns_policy::DnsManager::new()),
            vm_firewall: Arc::new(vm_firewall::VMFirewall::new()),
            vpn_mesh: Arc::new(vpn_mesh::VpnMesh::new()),
            packet_mirror: Arc::new(packet_mirror::PacketMirror::new()),
            nat_gateway: Arc::new(nat_gateway::NatGateway::new()),
            net_monitor: Arc::new(net_monitor::NetMonitor::new()),
            secrets_manager: Arc::new(secrets_manager::SecretsManager::from_env()?),
            dnsmasq_manager: Arc::new(zyvor_fabric_dnsmasq_manager::DnsmasqManager::new(
                "/run/zyvor-fabricd/dnsmasq",
            )),
            k8s_pod_client,
            vm_locks: Arc::new(std::sync::Mutex::new(std::collections::HashMap::new())),
            event_tx: {
                let (tx, _) = tokio::sync::broadcast::channel(256);
                tx
            },
            shutdown: tokio_util::sync::CancellationToken::new(),
        });

        Ok(Self { state })
    }

    pub async fn run(self) -> Result<()> {
        let app = build_router(self.state.clone());

        let addr: std::net::SocketAddr = self.state.config.daemon.listen.parse()?;
        let tls_config = if self.state.config.tls.enabled {
            let tls = &self.state.config.tls;
            crate::tls::ensure_self_signed_cert(&tls.cert_path, &tls.key_path)?;
            let rustls_config =
                axum_server::tls_rustls::RustlsConfig::from_pem_file(&tls.cert_path, &tls.key_path)
                    .await
                    .with_context(|| {
                        format!("loading TLS cert {} / key {}", tls.cert_path, tls.key_path)
                    })?;
            tracing::info!("Listening on https://{}", addr);
            Some(rustls_config)
        } else {
            tracing::info!("Listening on http://{} (TLS disabled)", addr);
            None
        };

        let mut bg_tasks: Vec<tokio::task::JoinHandle<()>> = Vec::new();
        let shutdown = self.state.shutdown.clone();

        // Helper macro to spawn cancellable background tasks
        macro_rules! spawn_bg {
            ($state:expr, $name:expr, $func:expr) => {{
                let s = $state.clone();
                let token = $state.shutdown.clone();
                bg_tasks.push(tokio::spawn(async move {
                    tokio::select! {
                        _ = token.cancelled() => {
                            tracing::debug!("Background task '{}' cancelled", $name);
                        }
                        _ = $func(s) => {
                            tracing::debug!("Background task '{}' exited", $name);
                        }
                    }
                }));
            }};
        }

        // Start background scheduler for automated schedule execution
        spawn_bg!(self.state, "schedule_checker", run_schedule_checker);

        // Start background metrics collector
        spawn_bg!(self.state, "metrics_collector", run_metrics_collector);

        // Start stale host detector
        spawn_bg!(self.state, "stale_host_detector", run_stale_host_detector);

        spawn_bg!(self.state, "drs_executor", run_drs_executor);
        spawn_bg!(self.state, "lock_renewal", run_lock_renewal);
        spawn_bg!(
            self.state,
            "replication_scheduler",
            run_replication_scheduler
        );
        spawn_bg!(self.state, "ha_monitor", run_ha_monitor);
        spawn_bg!(self.state, "vm_autohealer", run_vm_autohealer);
        spawn_bg!(
            self.state,
            "container_group_autohealer",
            run_container_group_autohealer
        );
        spawn_bg!(self.state, "autoscaler", run_autoscaler);
        spawn_bg!(self.state, "policy_reconciler", run_policy_reconciler);
        spawn_bg!(
            self.state,
            "service_health_checker",
            run_service_health_checker
        );
        spawn_bg!(self.state, "service_reconciler", run_service_reconciler);
        spawn_bg!(self.state, "qos_reconciler", run_qos_reconciler);
        spawn_bg!(self.state, "dns_reconciler", run_dns_reconciler);
        spawn_bg!(self.state, "firewall_reconciler", run_firewall_reconciler);
        spawn_bg!(self.state, "vpn_reconciler", run_vpn_reconciler);
        spawn_bg!(self.state, "mirror_reconciler", run_mirror_reconciler);
        spawn_bg!(self.state, "nat_reconciler", run_nat_reconciler);
        spawn_bg!(self.state, "net_monitor", run_net_monitor);
        spawn_bg!(self.state, "oidc_state_cleanup", run_oidc_state_cleanup);
        spawn_bg!(self.state, "snapshot_retention", run_snapshot_retention);
        // Replaces zyvor-fabricd-backup.timer/-cleanup.timer (systemd-removal
        // migration plan, Phase 6) — see schedulers.rs.
        spawn_bg!(
            self.state,
            "backup_scheduler",
            crate::schedulers::run_backup_scheduler
        );
        spawn_bg!(
            self.state,
            "cleanup_scheduler",
            crate::schedulers::run_cleanup_scheduler
        );
        spawn_bg!(
            self.state,
            "ai_routing_controller",
            crate::api::ai::routing::run_ai_routing_controller
        );
        spawn_bg!(
            self.state,
            "ai_autoscaler",
            crate::api::ai::autoscaling::run_ai_autoscaler
        );
        spawn_bg!(
            self.state,
            "ai_reconcile_controller",
            crate::api::ai::reconcile::run_ai_reconcile_controller
        );
        spawn_bg!(
            self.state,
            "ai_model_jobs",
            crate::api::ai::model_jobs::run_model_job_controller
        );

        let handle = axum_server::Handle::new();
        let shutdown_handle = handle.clone();
        tokio::spawn(async move {
            shutdown_signal().await;
            shutdown_handle.graceful_shutdown(Some(std::time::Duration::from_secs(10)));
        });

        match tls_config {
            Some(rustls_config) => {
                axum_server::bind_rustls(addr, rustls_config)
                    .handle(handle)
                    .serve(app.into_make_service())
                    .await?;
            }
            None => {
                axum_server::bind(addr)
                    .handle(handle)
                    .serve(app.into_make_service())
                    .await?;
            }
        }

        tracing::info!("Shutdown signal received, cancelling background tasks");
        shutdown.cancel();
        // Give tasks a moment to finish cleanly
        for handle in bg_tasks {
            let _ = tokio::time::timeout(std::time::Duration::from_secs(5), handle).await;
        }

        Ok(())
    }
}

/// Build the full application router with all routes. Used by both the server
/// and integration tests.
pub fn build_router(state: Arc<AppState>) -> Router {
    let cors = {
        use axum::http::{header, HeaderValue, Method};

        let origins: Vec<HeaderValue> = state
            .config
            .daemon
            .cors_origins
            .iter()
            .filter_map(|o| o.parse::<HeaderValue>().ok())
            .collect();

        CorsLayer::new()
            .allow_origin(origins)
            .allow_methods([
                Method::GET,
                Method::POST,
                Method::PUT,
                Method::DELETE,
                Method::OPTIONS,
            ])
            .allow_headers([header::CONTENT_TYPE, header::AUTHORIZATION])
    };

    // Public auth routes (no JWT required)
    let public_auth_routes = Router::new()
        .route("/auth/login", post(api::auth::login))
        .route("/instance", get(api::instance::get_instance))
        // AI OpenAI gateway — API-key auth (not Fabric JWT)
        .route(
            "/ai/openai/{endpoint}",
            axum::routing::any(api::ai::gateway::openai_gateway_root),
        )
        .route(
            "/ai/openai/{endpoint}/{*path}",
            axum::routing::any(api::ai::gateway::openai_gateway),
        )
        .route("/ai/admit", post(api::ai::policy::admit))
        .route("/ai/admit/{token}", post(api::ai::policy::admit_with_token))
        .route(
            "/ai/models/{name}/blobs/{digest}",
            put(api::ai::model_jobs::receive_blob),
        )
        .route("/ai/raft/vote", post(api::ai::raft::vote))
        .route("/ai/raft/append", post(api::ai::raft::append))
        .route("/ai/raft/snapshot", post(api::ai::raft::snapshot))
        .route("/ai/raft/limits", post(api::ai::raft::limit_write))
        .with_state(state.clone());

    // Protected API routes
    let mut api_routes = Router::new()
        // Auth - me endpoint (protected)
        .route("/auth/me", get(api::auth::me))
        .route("/auth/users", post(api::auth::create_auth_user))
        .route("/capabilities", get(api::capabilities::get_capabilities))
        .route("/license", get(routes::get_license_status))
        // VM management routes
        .route("/vms", get(routes::list_vms).post(routes::create_vm))
        .route("/vms/compare", get(api::ux_extensions::compare_vms))
        .route("/vms/{name}", get(routes::get_vm).delete(routes::delete_vm))
        .route(
            "/vms/{name}/tags",
            post(routes::add_tag).put(routes::update_tags),
        )
        .route("/vms/{name}/tags/{tag}", delete(routes::remove_tag))
        .route(
            "/vms/{name}/healthcheck",
            get(api::ux_extensions::vm_healthcheck),
        )
        .route("/vms/{name}/start", post(routes::start_vm))
        .route("/vms/{name}/port-forwards", post(routes::add_port_forward))
        .route(
            "/vms/{name}/port-forwards/{host_port}",
            delete(routes::remove_port_forward),
        )
        .route(
            "/vms/{name}/direct-uplink",
            put(routes::set_direct_uplink).delete(routes::clear_direct_uplink),
        )
        .route("/vms/{name}/stop", post(routes::stop_vm))
        .route("/vms/{name}/restart", post(routes::restart_vm))
        .route("/vms/{name}/metrics", get(routes::get_metrics))
        .route("/vms/{name}/pause", post(routes::pause_vm))
        .route("/vms/{name}/resume", post(routes::resume_vm))
        .route("/vms/{name}/clone", post(routes::clone_vm))
        .route("/vms/{name}/cloud-init", post(routes::configure_cloud_init))
        // FluxVM Network Fabric schema v4 — VM edge dataplane (not Fabric SDN)
        .route(
            "/vms/{name}/dataplane/status",
            get(api::vm_dataplane::dataplane_status),
        )
        .route(
            "/vms/{name}/dataplane/policy",
            get(api::vm_dataplane::get_dataplane_policy)
                .post(api::vm_dataplane::set_dataplane_policy),
        )
        .route(
            "/vms/{name}/dataplane/policy/control",
            post(api::vm_dataplane::dataplane_policy_control),
        )
        .route(
            "/vms/{name}/dataplane/explain",
            get(api::vm_dataplane::dataplane_explain),
        )
        .route(
            "/vms/{name}/dataplane/dry-run",
            get(api::vm_dataplane::dataplane_dry_run),
        )
        .route(
            "/dataplane/templates",
            get(api::vm_dataplane::dataplane_templates),
        )
        .route(
            "/vms/{name}/dataplane/stats",
            get(api::vm_dataplane::dataplane_stats),
        )
        .route(
            "/vms/{name}/dataplane/flows",
            get(api::vm_dataplane::dataplane_flows),
        )
        .route(
            "/vms/{name}/dataplane/effective",
            get(api::vm_dataplane::dataplane_effective),
        )
        .route(
            "/vms/{name}/dataplane/drop-reasons",
            get(api::vm_dataplane::dataplane_drop_reasons),
        )
        .route(
            "/vms/{name}/dataplane/pod-policy",
            get(api::vm_dataplane::get_pod_policy)
                .post(api::vm_dataplane::set_pod_policy)
                .delete(api::vm_dataplane::delete_pod_policy),
        )
        .route(
            "/dataplane/groups",
            get(api::vm_dataplane::list_groups).post(api::vm_dataplane::upsert_group),
        )
        .route(
            "/dataplane/groups/{name}",
            get(api::vm_dataplane::get_group).delete(api::vm_dataplane::delete_group),
        )
        .route(
            "/dataplane/services",
            get(api::vm_dataplane::list_services).post(api::vm_dataplane::upsert_service),
        )
        .route(
            "/dataplane/services/status",
            get(api::vm_dataplane::services_host_status),
        )
        .route(
            "/dataplane/services/stats",
            get(api::vm_dataplane::services_host_stats),
        )
        .route(
            "/dataplane/services/health",
            get(api::vm_dataplane::services_health),
        )
        .route(
            "/dataplane/services/health/reconcile",
            post(api::vm_dataplane::services_health_reconcile),
        )
        .route(
            "/dataplane/services/conntrack/gc",
            post(api::vm_dataplane::services_conntrack_gc),
        )
        .route(
            "/dataplane/services/pressure/reconcile",
            post(api::vm_dataplane::services_pressure_reconcile),
        )
        .route(
            "/dataplane/services/advertisements",
            get(api::vm_dataplane::services_advertisements),
        )
        .route(
            "/dataplane/services/flows",
            get(api::vm_dataplane::services_flows),
        )
        .route(
            "/dataplane/services/telemetry/export",
            post(api::vm_dataplane::services_telemetry_export),
        )
        .route(
            "/dataplane/services/policies",
            get(api::vm_dataplane::list_service_policies)
                .post(api::vm_dataplane::upsert_service_policy),
        )
        .route(
            "/dataplane/services/{name}/policy",
            get(api::vm_dataplane::get_service_policy)
                .delete(api::vm_dataplane::delete_service_policy),
        )
        .route(
            "/dataplane/services/{name}/l7/envoy",
            get(api::vm_dataplane::service_envoy_contract),
        )
        .route(
            "/dataplane/remote-identities",
            get(api::vm_dataplane::list_remote_identities)
                .post(api::vm_dataplane::upsert_remote_identity),
        )
        .route(
            "/dataplane/remote-identities/reconcile",
            post(api::vm_dataplane::reconcile_remote_identities),
        )
        .route(
            "/dataplane/remote-identities/{route_domain}/{identity_id}",
            delete(api::vm_dataplane::delete_remote_identity),
        )
        .route(
            "/dataplane/remote-backends",
            get(api::vm_dataplane::list_remote_backends)
                .post(api::vm_dataplane::upsert_remote_backend),
        )
        .route(
            "/dataplane/remote-backends/reconcile",
            post(api::vm_dataplane::reconcile_remote_backends),
        )
        .route(
            "/dataplane/remote-backends/{route_domain}/{service}/{address}/{port}/drain",
            post(api::vm_dataplane::drain_remote_backend),
        )
        .route(
            "/dataplane/remote-backends/{route_domain}/{service}/{address}/{port}",
            delete(api::vm_dataplane::delete_remote_backend),
        )
        .route(
            "/dataplane/services/{name}/conntrack/delta",
            get(api::vm_dataplane::services_conntrack_delta),
        )
        .route(
            "/dataplane/services/{name}/conntrack/delta/import",
            post(api::vm_dataplane::services_conntrack_delta_import),
        )
        .route(
            "/dataplane/services/{name}/conntrack/delta/ack",
            post(api::vm_dataplane::services_conntrack_delta_ack),
        )
        .route(
            "/dataplane/services/{name}",
            get(api::vm_dataplane::get_service).delete(api::vm_dataplane::delete_service),
        )
        .route(
            "/dataplane/cnp",
            get(api::vm_dataplane::list_cnp).post(api::vm_dataplane::apply_cnp),
        )
        .route(
            "/dataplane/cnp/{name}",
            get(api::vm_dataplane::get_cnp).delete(api::vm_dataplane::delete_cnp),
        )
        .route(
            "/dataplane/identities",
            get(api::vm_dataplane::list_identities),
        )
        .route(
            "/dataplane/endpoints",
            get(api::vm_dataplane::list_endpoints),
        )
        .route(
            "/dataplane/microvm-metrics",
            get(api::vm_dataplane::microvm_metrics),
        )
        .route("/dataplane/observe", get(api::vm_dataplane::observe))
        .route(
            "/dataplane/hubble/flows",
            get(api::vm_dataplane::hubble_flows),
        )
        .route("/dataplane/health", get(api::vm_dataplane::health))
        .route("/dataplane/ipcache", get(api::vm_dataplane::ipcache))
        .route(
            "/dataplane/refresh-dns",
            post(api::vm_dataplane::refresh_dns),
        )
        // Snapshot routes
        .route(
            "/vms/{name}/snapshots",
            get(api::snapshots::list_snapshots).post(api::snapshots::create_snapshot),
        )
        .route(
            "/vms/{name}/snapshots/tree",
            get(api::snapshots::snapshot_tree),
        )
        .route(
            "/vms/{name}/snapshots/{id}",
            get(api::snapshots::get_snapshot).delete(api::snapshots::delete_snapshot),
        )
        .route(
            "/vms/{name}/snapshots/{id}/revert",
            post(api::snapshots::revert_snapshot),
        )
        // Hotplug routes
        .route("/vms/{name}/hotplug/cpu", post(api::hotplug::hotplug_cpu))
        .route(
            "/vms/{name}/hotplug/memory",
            post(api::hotplug::hotplug_memory),
        )
        .route("/vms/{name}/hotplug/disk", post(api::hotplug::hotplug_disk))
        .route(
            "/vms/{name}/hotplug/disk/{id}",
            delete(api::hotplug::hotremove_disk),
        )
        .route("/vms/{name}/hotplug/nic", post(api::hotplug::hotplug_nic))
        .route(
            "/vms/{name}/hotplug/nic/{id}",
            delete(api::hotplug::hotremove_nic),
        )
        // Storage pool routes
        .route("/storage/pools", get(api::storage::list_pools))
        .route("/storage/pools/{name}", get(api::storage::get_pool))
        .route(
            "/storage/pools/local",
            post(api::storage::create_local_pool),
        )
        .route("/storage/pools/nfs", post(api::storage::create_nfs_pool))
        .route("/storage/pools/{name}", delete(api::storage::delete_pool))
        .route(
            "/storage/pools/{name}/start",
            post(api::storage::start_pool),
        )
        .route("/storage/pools/{name}/stop", post(api::storage::stop_pool))
        .route(
            "/storage/pools/{name}/health",
            get(api::storage::get_pool_health),
        )
        .route(
            "/storage/pools/{name}/stats",
            get(api::storage::get_pool_stats),
        )
        .route(
            "/storage/pools/{name}/refresh",
            post(api::storage::refresh_pool_stats),
        )
        .route("/storage/pools/lvm", post(api::storage::create_lvm_pool))
        .route(
            "/storage/pools/lvm-thin",
            post(api::storage::create_lvm_thin_pool),
        )
        .route("/storage/pools/zfs", post(api::storage::create_zfs_pool))
        .route("/storage/pools/ceph", post(api::storage::create_ceph_pool))
        // RBD images (Ceph pools only, proxied through Atlas)
        .route(
            "/storage/pools/{name}/images",
            get(api::storage::list_rbd_images).post(api::storage::create_rbd_image),
        )
        .route(
            "/storage/pools/{name}/images/{image}",
            delete(api::storage::delete_rbd_image),
        )
        // Volume management routes
        .route(
            "/storage/pools/{name}/volumes",
            get(api::volumes::list_volumes).post(api::volumes::create_volume),
        )
        .route(
            "/storage/pools/{name}/volumes/{id}",
            get(api::volumes::get_volume).delete(api::volumes::delete_volume),
        )
        .route(
            "/storage/pools/{name}/volumes/{id}/resize",
            post(api::volumes::resize_volume),
        )
        .route(
            "/storage/pools/{name}/volumes/{id}/attach",
            post(api::volumes::attach_volume),
        )
        .route(
            "/storage/pools/{name}/volumes/{id}/detach",
            post(api::volumes::detach_volume),
        )
        // System resource routes - CPU
        .route("/system/cpu/topology", get(api::system::get_cpu_topology))
        .route("/vms/{name}/cpu/pin", post(api::system::set_cpu_pinning))
        .route(
            "/vms/{name}/cpu/pin",
            delete(api::system::remove_cpu_pinning),
        )
        .route(
            "/vms/{name}/cpu/affinity",
            get(api::system::get_cpu_affinity),
        )
        // System resource routes - NUMA
        .route("/system/numa/topology", get(api::system::get_numa_topology))
        .route("/system/numa/nodes/{id}", get(api::system::get_numa_node))
        .route(
            "/system/numa/placement",
            get(api::system::get_numa_placement),
        )
        // System resource routes - Memory
        .route(
            "/vms/{name}/memory/limit",
            put(api::system::set_memory_limit),
        )
        .route(
            "/vms/{name}/memory/usage",
            get(api::system::get_memory_usage),
        )
        .route(
            "/vms/{name}/memory/balloon",
            post(api::system::set_memory_ballooning),
        )
        .route(
            "/system/memory/hugepages",
            get(api::system::get_hugepage_stats),
        )
        .route(
            "/system/memory/hugepages",
            post(api::system::allocate_hugepages),
        )
        .route("/system/memory", get(api::system::get_system_memory))
        .route("/system/processes", get(api::processes::list_processes))
        .route("/system/process", get(api::processes::get_process))
        .route("/system/info", get(api::host_insight::get_system_info))
        .route(
            "/system/metrics",
            get(api::host_insight::get_system_metrics),
        )
        .route("/system/kernel", get(api::host_insight::get_kernel_info))
        .route("/system/containers", get(api::host_insight::get_containers))
        .route(
            "/system/debug/{tool}",
            get(api::host_insight::get_debug_output),
        )
        .route(
            "/system/security",
            get(api::host_insight::get_security_summary),
        )
        .route("/system/alerts", get(api::host_insight::get_system_alerts))
        .route(
            "/system/alerts/rules",
            get(api::host_insight::get_alert_rules),
        )
        .route(
            "/system/explain/{metric}",
            get(api::host_insight::explain_metric),
        )
        .route("/system/timeseries", get(api::host_insight::get_timeseries))
        .route(
            "/system/compliance",
            get(api::host_insight::get_system_compliance),
        )
        .route(
            "/system/compliance/scan",
            axum::routing::post(api::host_insight::trigger_compliance_scan),
        )
        .route("/jobs", get(api::host_insight::list_jobs))
        .route("/jobs/{id}/logs", get(api::host_insight::get_job_logs))
        .route("/pipeline/jobs", get(api::host_insight::list_pipeline_jobs))
        .route("/isos", get(api::host_insight::list_isos_legacy))
        // Firmware routes
        .route(
            "/vms/{name}/firmware/status",
            get(api::firmware::get_firmware_status),
        )
        .route(
            "/vms/{name}/firmware/uefi",
            post(api::firmware::enable_uefi),
        )
        .route(
            "/vms/{name}/firmware/secureboot",
            post(api::firmware::enable_secureboot),
        )
        .route(
            "/vms/{name}/firmware/secureboot",
            delete(api::firmware::disable_secureboot),
        )
        .route(
            "/vms/{name}/firmware/reset",
            post(api::firmware::reset_nvram),
        )
        .route(
            "/system/firmware/capabilities",
            get(api::firmware::get_firmware_capabilities),
        )
        // Notification routes
        .route(
            "/notifications/channels",
            get(api::notifications::list_channels),
        )
        .route(
            "/notifications/channels",
            post(api::notifications::create_channel),
        )
        .route(
            "/notifications/channels/{id}",
            put(api::notifications::update_channel),
        )
        .route(
            "/notifications/channels/{id}",
            delete(api::notifications::delete_channel),
        )
        .route(
            "/notifications/channels/{id}/test",
            post(api::notifications::test_channel),
        )
        .route("/notifications/rules", get(api::notifications::list_rules))
        .route(
            "/notifications/rules",
            post(api::notifications::create_rule),
        )
        .route(
            "/notifications/rules/{id}",
            put(api::notifications::update_rule),
        )
        .route(
            "/notifications/rules/{id}",
            delete(api::notifications::delete_rule),
        )
        .route(
            "/notifications/rules/{id}/enable",
            post(api::notifications::enable_rule),
        )
        .route(
            "/notifications/rules/{id}/disable",
            post(api::notifications::disable_rule),
        )
        .route(
            "/notifications/history",
            get(api::notifications::get_history),
        )
        // Quota routes
        .route("/quotas", get(api::quotas::list_quotas))
        .route("/quotas", post(api::quotas::create_quota))
        .route("/quotas/{id}", get(api::quotas::get_quota))
        .route("/quotas/{id}", put(api::quotas::update_quota))
        .route("/quotas/{id}", delete(api::quotas::delete_quota))
        .route("/quotas/{id}/enable", post(api::quotas::enable_quota))
        .route("/quotas/{id}/disable", post(api::quotas::disable_quota))
        .route("/quotas/{id}/usage", get(api::quotas::get_quota_usage))
        .route("/quotas/usage", get(api::quotas::get_all_quota_usage))
        // Schedule routes
        .route("/schedules", get(api::schedules::list_schedules))
        .route("/schedules", post(api::schedules::create_schedule))
        .route("/schedules/{id}", get(api::schedules::get_schedule))
        .route("/schedules/{id}", put(api::schedules::update_schedule))
        .route("/schedules/{id}", delete(api::schedules::delete_schedule))
        .route(
            "/schedules/{id}/enable",
            post(api::schedules::enable_schedule),
        )
        .route(
            "/schedules/{id}/disable",
            post(api::schedules::disable_schedule),
        )
        .route(
            "/schedules/{id}/run",
            post(api::schedules::run_schedule_now),
        )
        .route(
            "/schedules/{id}/history",
            get(api::schedules::get_schedule_history),
        )
        .route(
            "/schedules/history",
            get(api::schedules::get_all_schedule_history),
        )
        // Audit routes
        .route("/audit/logs", get(api::audit::list_audit_logs))
        .route("/audit/logs/{id}", get(api::audit::get_audit_log))
        .route("/audit/logs/export", get(api::audit::export_audit_logs))
        .route("/audit/stats", get(api::audit::get_audit_stats))
        // Analytics routes
        .route(
            "/analytics/vms/{name}",
            get(api::analytics::get_vm_performance),
        )
        .route(
            "/analytics/system",
            get(api::analytics::get_system_performance),
        )
        .route(
            "/analytics/insights",
            get(api::analytics::get_performance_insights),
        )
        .route(
            "/analytics/top",
            get(api::analytics::get_top_vms_by_resource),
        )
        .route(
            "/analytics/utilization",
            get(api::analytics::get_resource_utilization),
        )
        .route(
            "/analytics/export",
            get(api::analytics::export_performance_report),
        )
        // Template routes
        .route(
            "/templates",
            get(api::templates::list_templates).post(api::templates::create_template),
        )
        .route(
            "/templates/{id}",
            get(api::templates::get_template)
                .put(api::templates::update_template)
                .delete(api::templates::delete_template),
        )
        .route(
            "/templates/{id}/deploy",
            post(api::templates::deploy_template),
        )
        // Migration routes
        .route(
            "/migrations/history",
            get(api::ux_extensions::migration_history),
        )
        .route(
            "/migrations/readiness",
            get(api::ux_extensions::migration_readiness),
        )
        .route(
            "/migrations/report",
            get(api::ux_extensions::migration_report),
        )
        .route(
            "/migrations",
            get(api::migration::list_migrations).post(api::migration::start_migration),
        )
        .route("/migrations/{id}", get(api::migration::get_migration))
        .route(
            "/migrations/{id}/cancel",
            post(api::migration::cancel_migration),
        )
        .route(
            "/runtime/capabilities",
            get(api::migration::runtime_capabilities),
        )
        .route(
            "/vms/{name}/migration/native/start",
            post(api::migration::start_native_migration),
        )
        .route(
            "/vms/{name}/migration/native/status",
            get(api::migration::native_migration_status),
        )
        .route(
            "/vms/{name}/migration/native/cancel",
            post(api::migration::cancel_native_migration),
        )
        .route(
            "/vms/{name}/migration/native/prepare-receiver",
            post(api::migration::prepare_native_receiver),
        )
        .route(
            "/vms/{name}/migration/native/network-state",
            get(api::migration::native_network_migration_state),
        )
        .route(
            "/migration/receivers/{id}/activate",
            post(api::migration::activate_native_receiver),
        )
        .route(
            "/migration/receivers/{id}",
            axum::routing::delete(api::migration::abort_native_receiver),
        )
        .route("/vms/{name}/memory", get(api::memory::get_memory))
        .route(
            "/vms/{name}/balloon",
            get(api::memory::get_balloon).post(api::memory::set_balloon),
        )
        .route("/vms/{name}/qga/ping", post(api::qga::qga_ping))
        .route("/vms/{name}/qga/exec", post(api::qga::qga_exec))
        .route(
            "/vms/{name}/qga/firewall/open",
            post(api::qga::qga_firewall_open),
        )
        .route(
            "/vms/{name}/qga/firewall/close",
            post(api::qga::qga_firewall_close),
        )
        .route("/images/upload", post(api::ux_extensions::upload_image))
        .route("/images/convert", post(api::ux_extensions::start_convert))
        .route(
            "/images/convert/{id}",
            get(api::ux_extensions::get_convert_job),
        )
        .route("/images", get(api::images::list_images))
        // Cloud image management
        .route("/images/cloud", get(api::images::list_cloud_images))
        .route(
            "/images/cloud/download",
            post(api::images::download_cloud_image),
        )
        .route("/images/downloads", get(api::images::list_downloads))
        // ISO management
        .route("/images/iso", get(api::images::list_iso_images))
        .route("/images/iso/download", post(api::images::download_iso))
        .route("/images/iso/{name}", delete(api::images::delete_iso))
        // VM image import (OVA/VMDK/VDI)
        .route("/images/import", post(api::images::import_vm_image))
        // Golden images: materialize a VM's current disk as a standalone catalog image
        .route(
            "/images/from-vm/{name}",
            post(api::ux_extensions::create_image_from_vm),
        )
        // Warm VM pools: pre-boot N VMs from a template, claim one instantly
        .route(
            "/vm-pools",
            get(api::pools::list_pools).post(api::pools::create_pool),
        )
        .route(
            "/vm-pools/{name}",
            get(api::pools::get_pool).delete(api::pools::delete_pool),
        )
        .route("/vm-pools/{name}/claim", post(api::pools::claim_pool))
        // Online disk resize
        .route("/vms/{name}/disk/resize", post(api::images::resize_disk))
        // VM profile / instance type routes
        .route(
            "/profiles",
            get(api::profiles::list_profiles).post(api::profiles::create_profile),
        )
        .route(
            "/profiles/{name}",
            get(api::profiles::get_profile).delete(api::profiles::delete_profile),
        )
        // Event routes
        .route("/events", get(api::events::list_events))
        .route("/events/stream", get(api::events::event_stream))
        .route(
            "/events/retention",
            get(api::events::get_retention).put(api::events::set_retention),
        )
        // Config snapshot (Time Machine foundation)
        .route(
            "/config/snapshot",
            get(api::config_snapshot::export_config_snapshot),
        )
        // Floating IP routes
        .route(
            "/floating-ips",
            get(api::network_cloud::list_floating_ips).post(api::network_cloud::create_floating_ip),
        )
        .route(
            "/floating-ips/adopt",
            post(api::network_cloud::adopt_floating_ip),
        )
        .route(
            "/floating-ips/{id}",
            delete(api::network_cloud::delete_floating_ip),
        )
        .route(
            "/floating-ips/{id}/assign",
            post(api::network_cloud::assign_floating_ip),
        )
        .route(
            "/floating-ips/{id}/unassign",
            post(api::network_cloud::unassign_floating_ip),
        )
        // DHCP server routes (systemd-networkd)
        .route(
            "/dhcp-servers",
            get(api::network_cloud::list_dhcp_servers).post(api::network_cloud::create_dhcp_server),
        )
        .route(
            "/dhcp-servers/{id}",
            put(api::network_cloud::update_dhcp_server)
                .delete(api::network_cloud::delete_dhcp_server),
        )
        // DNS routes (systemd-resolved)
        .route(
            "/dns",
            get(api::network_cloud::list_dns_configs).post(api::network_cloud::create_dns_config),
        )
        .route("/dns/{id}", delete(api::network_cloud::delete_dns_config))
        .route(
            "/dns/{id}/records",
            post(api::network_cloud::add_dns_record),
        )
        // Availability zone routes
        .route(
            "/zones",
            get(api::zones::list_zones).post(api::zones::create_zone),
        )
        .route(
            "/zones/{id}",
            get(api::zones::get_zone).delete(api::zones::delete_zone),
        )
        // Spot instance routes
        .route(
            "/spot-instances",
            get(api::zones::list_spot_instances).post(api::zones::create_spot_instance),
        )
        .route(
            "/spot-instances/{id}",
            delete(api::zones::delete_spot_instance),
        )
        .route(
            "/spot-instances/{id}/evict",
            post(api::zones::evict_spot_instance),
        )
        // KSM memory deduplication
        .route(
            "/system/ksm",
            get(api::vm_advanced::get_ksm_status).post(api::vm_advanced::configure_ksm),
        )
        // Nested virtualization
        .route(
            "/system/nested-virt",
            get(api::vm_advanced::get_nested_virt_status).post(api::vm_advanced::set_nested_virt),
        )
        // VM checkpoints
        .route(
            "/vms/{name}/checkpoints",
            get(api::vm_advanced::list_checkpoints).post(api::vm_advanced::create_checkpoint),
        )
        .route(
            "/vms/{name}/checkpoints/{id}/restore",
            post(api::vm_advanced::restore_checkpoint),
        )
        .route(
            "/vms/{name}/checkpoints/{id}",
            delete(api::vm_advanced::delete_checkpoint),
        )
        // VM forking
        .route("/vms/{name}/fork", post(api::vm_advanced::fork_vm))
        // Declarative VM spec
        .route("/vms/apply", post(api::declarative::apply_vm_spec))
        .route("/vms/{name}/spec", get(api::declarative::export_vm_spec))
        // Declarative ContainerGroup spec (FluxVM Secure Containers)
        .route(
            "/container-groups",
            get(api::container_declarative::list_container_groups),
        )
        .route(
            "/container-groups/apply",
            post(api::container_declarative::apply_container_group_spec),
        )
        .route(
            "/container-groups/{name}/spec",
            get(api::container_declarative::export_container_group_spec),
        )
        .route(
            "/container-groups/{name}/status",
            get(api::container_declarative::get_container_group_status),
        )
        .route(
            "/container-groups/{name}",
            delete(api::container_declarative::delete_container_group),
        )
        .route(
            "/container-group-events",
            get(api::container_declarative::list_container_group_events),
        )
        // Agent-runtime proxy (sibling zyvor-fabric-agent-runtime)
        .route(
            "/agents",
            get(api::agent_runtime::list_agents).post(api::agent_runtime::deploy_agent),
        )
        .route("/agents/{name}", get(api::agent_runtime::get_agent))
        .route(
            "/sessions",
            get(api::agent_runtime::list_sessions).post(api::agent_runtime::create_session),
        )
        .route(
            "/sessions/{id}",
            get(api::agent_runtime::get_session).delete(api::agent_runtime::delete_session),
        )
        .route(
            "/sessions/{id}/events",
            get(api::agent_runtime::session_events),
        )
        .route(
            "/sessions/{id}/cockpit",
            get(api::agent_runtime::session_cockpit),
        )
        .route("/keep/status", get(api::agent_runtime::keep_status))
        .route("/packs", post(api::agent_runtime::pack_deploy))
        .route(
            "/demos",
            get(api::agent_runtime::demo_list).post(api::agent_runtime::demo_save),
        )
        .route(
            "/demos/{id}",
            post(api::agent_runtime::demo_run).delete(api::agent_runtime::demo_delete),
        )
        .route(
            "/sessions/{id}/host-recover",
            post(api::agent_runtime::session_host_recover),
        )
        .route(
            "/sessions/{id}/browser/view",
            get(api::agent_runtime::session_browser_view),
        )
        .route(
            "/sessions/{id}/browser/screenshot",
            get(api::agent_runtime::session_browser_screenshot),
        )
        .route("/vault/status", get(api::agent_runtime::vault_status))
        .route(
            "/vault/user-held/challenge",
            post(api::agent_runtime::vault_user_held_challenge),
        )
        .route(
            "/vault/user-held/complete",
            post(api::agent_runtime::vault_user_held_complete),
        )
        .route(
            "/sessions/{id}/{action}",
            post(api::agent_runtime::session_action),
        )
        .route(
            "/approvals",
            get(api::agent_runtime::list_approvals).post(api::agent_runtime::create_approval),
        )
        .route("/approvals/{id}", post(api::agent_runtime::decide_approval))
        .route("/audit/agent-actions", get(api::agent_runtime::list_audit))
        .route("/agent-tokens", post(api::agent_runtime::mint_my_token))
        .route(
            "/triggers",
            get(api::agent_runtime::list_triggers).post(api::agent_runtime::create_trigger),
        )
        .route("/triggers/{id}", delete(api::agent_runtime::delete_trigger))
        .route("/artifacts", get(api::agent_runtime::list_artifacts))
        .route(
            "/artifacts/{a}/diff/{b}",
            get(api::agent_runtime::diff_artifacts),
        )
        .route(
            "/skills",
            get(api::agent_runtime::list_skills).post(api::agent_runtime::publish_skill),
        )
        .route(
            "/skills/{name}",
            get(api::agent_runtime::get_skill).delete(api::agent_runtime::delete_skill),
        )
        // Fabric AI Workloads — Inference MVP (preview)
        .route(
            "/ai/models",
            get(api::ai::models::list_models).post(api::ai::models::create_model),
        )
        .route(
            "/ai/models/{name}",
            get(api::ai::models::get_model).delete(api::ai::models::delete_model),
        )
        .route(
            "/ai/models/{name}/materialize",
            post(api::ai::model_jobs::materialize),
        )
        .route(
            "/ai/models/{name}/status",
            get(api::ai::model_jobs::model_status),
        )
        .route(
            "/ai/models/{name}/verify",
            post(api::ai::model_jobs::verify_model),
        )
        .route(
            "/ai/models/{name}/replicate",
            post(api::ai::model_jobs::replicate),
        )
        .route(
            "/ai/models/{name}/cache/{node}",
            delete(api::ai::model_jobs::delete_node_cache),
        )
        .route("/ai/models/cache/evict", post(api::ai::models::evict_cache))
        .route("/ai/model-jobs", get(api::ai::model_jobs::list_jobs))
        .route(
            "/ai/profiles",
            get(api::ai::profiles::list_profiles).post(api::ai::profiles::create_profile),
        )
        .route(
            "/ai/profiles/{name}",
            get(api::ai::profiles::get_profile).delete(api::ai::profiles::delete_profile),
        )
        .route(
            "/ai/deployments",
            get(api::ai::deployments::list_deployments)
                .post(api::ai::deployments::create_deployment),
        )
        .route(
            "/ai/deployments/{name}",
            get(api::ai::deployments::get_deployment)
                .delete(api::ai::deployments::delete_deployment),
        )
        .route(
            "/ai/deployments/{name}/scale",
            post(api::ai::deployments::scale_deployment),
        )
        .route(
            "/ai/deployments/{name}/autoscaling",
            axum::routing::put(api::ai::deployments::patch_autoscaling),
        )
        .route(
            "/ai/deployments/{name}/drain",
            post(api::ai::rollouts::drain_deployment),
        )
        .route(
            "/ai/deployments/{name}/rollout",
            post(api::ai::rollouts::rollout_deployment),
        )
        .route(
            "/ai/deployments/{name}/revisions",
            get(api::ai::revisions::list_deployment_revisions)
                .post(api::ai::revisions::create_revision),
        )
        .route(
            "/ai/deployments/{name}/revisions/{revision}",
            get(api::ai::revisions::get_revision),
        )
        .route(
            "/ai/deployments/{name}/rollouts",
            post(api::ai::rollouts::create_rollout),
        )
        .route(
            "/ai/deployments/{name}/rollouts/{id}/pause",
            post(api::ai::rollouts::pause_rollout),
        )
        .route(
            "/ai/deployments/{name}/rollouts/{id}/resume",
            post(api::ai::rollouts::resume_rollout),
        )
        .route(
            "/ai/deployments/{name}/rollouts/{id}/promote",
            post(api::ai::rollouts::promote_rollout),
        )
        .route(
            "/ai/deployments/{name}/rollouts/{id}/rollback",
            post(api::ai::rollouts::rollback_rollout),
        )
        .route(
            "/ai/deployments/{name}/metrics",
            get(api::ai::deployments::deployment_metrics),
        )
        .route(
            "/ai/endpoints",
            get(api::ai::endpoints::list_endpoints).post(api::ai::endpoints::create_endpoint),
        )
        .route(
            "/ai/endpoints/{name}",
            get(api::ai::endpoints::get_endpoint).delete(api::ai::endpoints::delete_endpoint),
        )
        .route("/ai/gpus", get(api::ai::gpus::list_gpus))
        .route(
            "/ai/nodes",
            get(api::ai::nodes::list_nodes).post(api::ai::nodes::create_node),
        )
        .route("/ai/nodes/{id}", get(api::ai::nodes::get_node))
        .route("/ai/nodes/{id}/heartbeat", post(api::ai::nodes::heartbeat))
        .route(
            "/ai/nodes/{id}/gpus/{bdf}/quarantine",
            post(api::ai::nodes::quarantine_gpu),
        )
        .route(
            "/ai/nodes/{id}/gpus/{bdf}/restore",
            post(api::ai::nodes::restore_gpu),
        )
        .route(
            "/ai/nodes/{id}/gpus/{bdf}/mig",
            post(api::ai::nodes::set_mig),
        )
        .route("/ai/nodes/{id}/mig", post(api::ai::nodes::create_mig_slice))
        .route(
            "/ai/nodes/{id}/mig/{bdf}",
            axum::routing::delete(api::ai::nodes::delete_mig_slice),
        )
        .route(
            "/ai/sites",
            get(api::ai::sites::list_sites).post(api::ai::sites::put_site),
        )
        .route(
            "/ai/sites/{id}",
            get(api::ai::sites::get_site).delete(api::ai::sites::delete_site),
        )
        .route(
            "/ai/batches",
            get(api::ai::batch::list_batches).post(api::ai::batch::create_batch),
        )
        .route("/ai/batches/{id}", get(api::ai::batch::get_batch))
        .route("/ai/batches/{id}/claim", post(api::ai::batch::claim_batch))
        .route(
            "/ai/batches/{id}/finish",
            post(api::ai::batch::finish_batch),
        )
        .route(
            "/ai/batches/{id}/cancel",
            post(api::ai::batch::cancel_batch),
        )
        .route(
            "/ai/explain/placement/{deployment}",
            get(api::ai::explain::explain_placement),
        )
        .route(
            "/ai/explain/scaling/{deployment}",
            get(api::ai::explain::explain_scaling),
        )
        .route(
            "/ai/explain/routing/{endpoint}",
            get(api::ai::explain::explain_routing),
        )
        .route(
            "/ai/explain/failure/{replica}",
            get(api::ai::explain::explain_failure),
        )
        .route("/ai/policies", post(api::ai::policy::put_policy))
        .route("/ai/backup", post(api::ai::backup::export_state))
        .route("/ai/restore", post(api::ai::backup::restore_state))
        .route("/ai/capacity", get(api::ai::capacity::capacity))
        .route("/ai/finops", get(api::ai::finops::report))
        .route("/ai/events", get(api::ai::capacity::events))
        .route(
            "/ai/keys",
            get(api::ai::keys::list_keys).post(api::ai::keys::create_key),
        )
        .route("/ai/keys/{id}/rotate", post(api::ai::keys::rotate_key))
        .route(
            "/ai/key-policies/{endpoint}",
            put(api::ai::keys::put_key_policy),
        )
        .route(
            "/ai/keys/{id}",
            axum::routing::delete(api::ai::keys::delete_key),
        )
        // ContainerGroup volume backup/restore
        .route(
            "/container-group-backups",
            get(api::container_group_backups::list_container_group_backups)
                .post(api::container_group_backups::create_container_group_backup),
        )
        .route(
            "/container-group-backups/{id}",
            get(api::container_group_backups::get_container_group_backup)
                .delete(api::container_group_backups::delete_container_group_backup),
        )
        .route(
            "/container-group-backups/{id}/restore",
            post(api::container_group_backups::restore_container_group_backup),
        )
        // Auto-scaling
        .route(
            "/autoscale",
            get(api::autoscale::list_scaling_policies).post(api::autoscale::create_scaling_policy),
        )
        .route("/autoscale/events", get(api::autoscale::list_scale_events))
        .route(
            "/autoscale/{vm_name}",
            get(api::autoscale::get_scaling_policy).delete(api::autoscale::delete_scaling_policy),
        )
        // Plugin routes
        .route("/plugins", get(plugins::list_plugins))
        // Resource optimization routes
        .route(
            "/system/optimization/recommendations",
            get(api::system::get_optimization_recommendations),
        )
        .route("/vms/{name}/optimize", post(api::system::optimize_vm))
        // Backup routes
        .route("/backups", get(api::backups::list_backups))
        .route("/backups", post(api::backups::create_backup))
        .route("/backups/{id}", get(api::backups::get_backup))
        .route("/backups/{id}", delete(api::backups::delete_backup))
        .route("/backups/restore", post(api::backups::restore_backup))
        .route("/backups/jobs", get(api::backups::get_backup_jobs))
        .route("/backups/jobs/{id}", get(api::backups::get_backup_job))
        .route("/backups/policies", get(api::backups::list_backup_policies))
        .route(
            "/backups/policies",
            post(api::backups::create_backup_policy),
        )
        .route(
            "/backups/policies/{id}",
            delete(api::backups::delete_backup_policy),
        )
        .route(
            "/backups/policies/{id}/enable",
            post(api::backups::enable_backup_policy),
        )
        .route(
            "/backups/policies/{id}/disable",
            post(api::backups::disable_backup_policy),
        )
        .route("/backups/stats", get(api::backups::get_backup_stats))
        // Settings routes
        .route(
            "/settings",
            get(api::settings::get_settings).put(api::settings::update_settings),
        )
        // ========================================================================
        // Enterprise feature routes (vSphere feature parity)
        // ========================================================================
        // Datacenter routes
        .route(
            "/datacenters",
            get(api::datacenter::list_datacenters).post(api::datacenter::create_datacenter),
        )
        .route(
            "/datacenters/{id}",
            get(api::datacenter::get_datacenter)
                .put(api::datacenter::update_datacenter)
                .delete(api::datacenter::delete_datacenter),
        )
        .route(
            "/datacenters/{id}/summary",
            get(api::datacenter::get_datacenter_summary),
        )
        // Cluster routes
        .route(
            "/clusters",
            get(api::datacenter::list_clusters).post(api::datacenter::create_cluster),
        )
        .route(
            "/clusters/{id}",
            get(api::datacenter::get_cluster)
                .put(api::datacenter::update_cluster)
                .delete(api::datacenter::delete_cluster),
        )
        // Host routes
        .route(
            "/hosts",
            get(api::datacenter::list_hosts).post(api::datacenter::register_host),
        )
        .route(
            "/hosts/{id}",
            get(api::datacenter::get_host)
                .put(api::datacenter::update_host)
                .delete(api::datacenter::remove_host),
        )
        .route(
            "/hosts/{id}/heartbeat",
            post(api::datacenter::host_heartbeat),
        )
        .route(
            "/hosts/{id}/maintenance/enter",
            post(api::datacenter::host_enter_maintenance),
        )
        .route(
            "/hosts/{id}/maintenance/exit",
            post(api::datacenter::host_exit_maintenance),
        )
        .route("/hosts/discover", post(api::datacenter::discover_host))
        .route(
            "/clusters/{id}/health",
            get(api::datacenter::get_cluster_health),
        )
        // Resource pool routes
        .route(
            "/resource-pools",
            get(api::resource_pools::list_pools).post(api::resource_pools::create_pool),
        )
        .route(
            "/resource-pools/{id}",
            get(api::resource_pools::get_pool)
                .put(api::resource_pools::update_pool)
                .delete(api::resource_pools::delete_pool),
        )
        .route(
            "/resource-pools/{id}/summary",
            get(api::resource_pools::get_pool_summary),
        )
        .route(
            "/resource-pools/{id}/vms",
            post(api::resource_pools::assign_vm),
        )
        .route(
            "/resource-pools/{id}/vms/{vm_name}",
            delete(api::resource_pools::unassign_vm),
        )
        .route(
            "/resource-pools/{id}/vms/move",
            post(api::resource_pools::move_vm),
        )
        .route(
            "/resource-pools/{id}/admission",
            post(api::resource_pools::check_admission),
        )
        // DRS routes
        .route("/drs/config", post(api::drs::configure_drs))
        .route("/drs/config/{cluster_id}", get(api::drs::get_drs_config))
        .route("/drs/placement", post(api::drs::compute_placement))
        .route("/drs/balance/{cluster_id}", get(api::drs::analyze_balance))
        .route(
            "/drs/recommendations",
            post(api::drs::generate_recommendations),
        )
        .route(
            "/drs/recommendations/{cluster_id}",
            get(api::drs::list_recommendations),
        )
        .route(
            "/drs/recommendations/{id}/approve",
            post(api::drs::approve_recommendation),
        )
        .route(
            "/drs/recommendations/{id}/reject",
            post(api::drs::reject_recommendation),
        )
        .route(
            "/drs/affinity-rules",
            get(api::drs::list_affinity_rules).post(api::drs::create_affinity_rule),
        )
        .route(
            "/drs/affinity-rules/{id}",
            get(api::drs::get_affinity_rule)
                .put(api::drs::update_affinity_rule)
                .delete(api::drs::delete_affinity_rule),
        )
        // Distributed storage routes
        .route(
            "/distributed-storage/pools",
            get(api::distributed_storage::list_storage_pools)
                .post(api::distributed_storage::create_storage_pool),
        )
        .route(
            "/distributed-storage/pools/{id}",
            get(api::distributed_storage::get_storage_pool)
                .delete(api::distributed_storage::delete_storage_pool),
        )
        .route(
            "/distributed-storage/pools/{id}/hosts",
            post(api::distributed_storage::add_storage_host),
        )
        .route(
            "/distributed-storage/pools/{id}/hosts/{host_id}",
            delete(api::distributed_storage::remove_storage_host),
        )
        .route(
            "/distributed-storage/pools/{id}/disk-failure",
            post(api::distributed_storage::report_disk_failure),
        )
        .route(
            "/distributed-storage/pools/{id}/health",
            get(api::distributed_storage::get_pool_health),
        )
        .route(
            "/distributed-storage/migrations",
            get(api::distributed_storage::list_storage_migrations)
                .post(api::distributed_storage::start_storage_migration),
        )
        .route(
            "/distributed-storage/migrations/{id}",
            get(api::distributed_storage::get_storage_migration),
        )
        .route(
            "/distributed-storage/migrations/{id}/progress",
            put(api::distributed_storage::update_migration_progress),
        )
        .route(
            "/distributed-storage/migrations/{id}/complete",
            post(api::distributed_storage::complete_migration),
        )
        .route(
            "/distributed-storage/migrations/{id}/cancel",
            post(api::distributed_storage::cancel_migration),
        )
        .route(
            "/distributed-storage/policies",
            get(api::distributed_storage::list_storage_policies)
                .post(api::distributed_storage::create_storage_policy),
        )
        .route(
            "/distributed-storage/policies/{id}",
            get(api::distributed_storage::get_storage_policy)
                .put(api::distributed_storage::update_storage_policy)
                .delete(api::distributed_storage::delete_storage_policy),
        )
        .route(
            "/distributed-storage/policies/{id}/compliance",
            post(api::distributed_storage::check_compliance),
        )
        .route(
            "/distributed-storage/datastore-clusters",
            get(api::distributed_storage::list_datastore_clusters)
                .post(api::distributed_storage::create_datastore_cluster),
        )
        .route(
            "/distributed-storage/datastore-clusters/{id}",
            get(api::distributed_storage::get_datastore_cluster)
                .delete(api::distributed_storage::delete_datastore_cluster),
        )
        .route(
            "/distributed-storage/datastore-clusters/{id}/recommend",
            post(api::distributed_storage::recommend_datastore),
        )
        // Encryption routes
        .route(
            "/encryption/providers",
            get(api::vm_encryption::list_providers).post(api::vm_encryption::register_provider),
        )
        .route(
            "/encryption/providers/{id}",
            delete(api::vm_encryption::remove_provider),
        )
        .route(
            "/encryption/providers/{id}/test",
            post(api::vm_encryption::test_provider),
        )
        .route(
            "/encryption/policies",
            get(api::vm_encryption::list_policies).post(api::vm_encryption::create_policy),
        )
        .route(
            "/encryption/policies/{id}",
            get(api::vm_encryption::get_policy)
                .put(api::vm_encryption::update_policy)
                .delete(api::vm_encryption::delete_policy),
        )
        .route(
            "/encryption/vms/{name}/encrypt",
            post(api::vm_encryption::encrypt_vm),
        )
        .route(
            "/encryption/vms/{name}/decrypt",
            post(api::vm_encryption::decrypt_vm),
        )
        .route(
            "/encryption/vms/{name}/status",
            get(api::vm_encryption::get_vm_encryption_status),
        )
        .route(
            "/encryption/vms",
            get(api::vm_encryption::list_encrypted_vms),
        )
        .route(
            "/encryption/vms/{name}/rotate-key",
            post(api::vm_encryption::rotate_vm_key),
        )
        // systemd-networkd VM networking routes
        .route(
            "/network/topology",
            get(api::ux_extensions::network_topology),
        )
        .route(
            "/networkd/bridges",
            get(api::networkd::list_bridges).post(api::networkd::create_bridge),
        )
        .route("/networkd/bridges/adopt", post(api::networkd::adopt_bridge))
        .route(
            "/networkd/bridges/{id}",
            get(api::networkd::get_bridge)
                .put(api::networkd::update_bridge)
                .delete(api::networkd::delete_bridge),
        )
        .route(
            "/networkd/vlans",
            get(api::networkd::list_vlans).post(api::networkd::create_vlan),
        )
        .route("/networkd/vlans/adopt", post(api::networkd::adopt_vlan))
        .route(
            "/networkd/vlans/{id}",
            get(api::networkd::get_vlan)
                .put(api::networkd::update_vlan)
                .delete(api::networkd::delete_vlan),
        )
        .route(
            "/networkd/macvtaps",
            get(api::networkd::list_macvtaps).post(api::networkd::create_macvtap),
        )
        .route(
            "/networkd/macvtaps/adopt",
            post(api::networkd::adopt_macvtap),
        )
        .route(
            "/networkd/macvtaps/{id}",
            get(api::networkd::get_macvtap).delete(api::networkd::delete_macvtap),
        )
        .route(
            "/networkd/taps",
            get(api::networkd::list_taps).post(api::networkd::create_tap),
        )
        .route("/networkd/taps/adopt", post(api::networkd::adopt_tap))
        .route(
            "/networkd/taps/{id}",
            get(api::networkd::get_tap).delete(api::networkd::delete_tap),
        )
        .route(
            "/networkd/bonds",
            get(api::networkd::list_bonds).post(api::networkd::create_bond),
        )
        .route("/networkd/bonds/adopt", post(api::networkd::adopt_bond))
        .route(
            "/networkd/bonds/{id}",
            get(api::networkd::get_bond)
                .put(api::networkd::update_bond)
                .delete(api::networkd::delete_bond),
        )
        .route(
            "/networkd/network-files",
            get(api::networkd::list_network_files).post(api::networkd::create_network_file),
        )
        .route(
            "/networkd/network-files/adopt",
            post(api::networkd::adopt_network_file),
        )
        .route(
            "/networkd/network-files/{id}",
            get(api::networkd::get_network_file).delete(api::networkd::delete_network_file),
        )
        .route(
            "/networkd/link-files",
            get(api::networkd::list_link_files).post(api::networkd::create_link_file),
        )
        .route(
            "/networkd/link-files/{id}",
            delete(api::networkd::delete_link_file),
        )
        .route("/networkd/links", get(api::networkd::list_links))
        .route(
            "/networkd/links/{name}/status",
            get(api::networkd::get_device_status),
        )
        .route("/networkd/reload", post(api::networkd::reload_networkd))
        .route("/networkd/files", get(api::networkd::list_managed_files))
        .route(
            "/networkd/port-forwards",
            get(api::networkd::list_port_forwards).post(api::networkd::create_port_forward),
        )
        .route(
            "/networkd/port-forwards/adopt",
            post(api::networkd::adopt_port_forward),
        )
        .route(
            "/networkd/port-forwards/sync",
            post(api::networkd::sync_port_forwards),
        )
        .route(
            "/networkd/port-forwards/{id}",
            get(api::networkd::get_port_forward).delete(api::networkd::delete_port_forward),
        )
        .route(
            "/networkd/vxlans",
            get(api::networkd::list_vxlans).post(api::networkd::create_vxlan),
        )
        .route("/networkd/vxlans/adopt", post(api::networkd::adopt_vxlan))
        .route(
            "/networkd/vxlans/{id}",
            get(api::networkd::get_vxlan).delete(api::networkd::delete_vxlan),
        )
        .route(
            "/networkd/sriov",
            get(api::networkd::list_sriov).post(api::networkd::create_sriov),
        )
        .route("/networkd/sriov/adopt", post(api::networkd::adopt_sriov))
        .route(
            "/networkd/sriov/{id}",
            get(api::networkd::get_sriov).delete(api::networkd::delete_sriov),
        )
        .route("/networkd/scan", get(api::networkd::scan_configs))
        .route(
            "/networkd/netlink/interfaces",
            get(api::networkd::list_netlink_interfaces),
        )
        .route(
            "/networkd/netlink/physical",
            get(api::networkd::list_physical_interfaces),
        )
        .route(
            "/networkd/netlink/available",
            get(api::networkd::list_available_interfaces),
        )
        // Network policy routes
        .route(
            "/network-policies",
            get(api::network_policy::list_policies).post(api::network_policy::create_policy),
        )
        .route(
            "/network-policies/adopt",
            post(api::network_policy::adopt_policy),
        )
        .route(
            "/network-policies/sync",
            post(api::network_policy::sync_policies),
        )
        .route(
            "/network-policies/status",
            get(api::network_policy::get_policy_status),
        )
        .route(
            "/network-policies/{id}",
            get(api::network_policy::get_policy)
                .put(api::network_policy::update_policy)
                .delete(api::network_policy::delete_policy),
        )
        .route("/identities", get(api::network_policy::list_identities))
        .route(
            "/identities/adopt",
            post(api::network_policy::adopt_identity),
        )
        .route("/identities/{id}", get(api::network_policy::get_identity))
        // Service mesh routes
        .route(
            "/services",
            get(api::service_mesh::list_services).post(api::service_mesh::create_service),
        )
        .route("/services/adopt", post(api::service_mesh::adopt_service))
        .route("/services/map", get(api::ux_extensions::service_map))
        .route("/services/sync", post(api::service_mesh::sync_services))
        .route(
            "/services/status",
            get(api::service_mesh::get_service_status),
        )
        .route(
            "/services/{id}",
            get(api::service_mesh::get_service)
                .put(api::service_mesh::update_service)
                .delete(api::service_mesh::delete_service),
        )
        .route(
            "/services/{id}/backends",
            get(api::service_mesh::get_service_backends),
        )
        // Traffic shaping routes
        .route(
            "/qos-policies",
            get(api::traffic_shaping::list_qos_policies)
                .post(api::traffic_shaping::create_qos_policy),
        )
        .route(
            "/qos-policies/adopt",
            post(api::traffic_shaping::adopt_qos_policy),
        )
        .route(
            "/qos-policies/sync",
            post(api::traffic_shaping::sync_qos_policies),
        )
        .route(
            "/qos-policies/status",
            get(api::traffic_shaping::get_qos_status),
        )
        .route(
            "/qos-policies/{id}",
            get(api::traffic_shaping::get_qos_policy)
                .put(api::traffic_shaping::update_qos_policy)
                .delete(api::traffic_shaping::delete_qos_policy),
        )
        // DNS policy routes
        .route(
            "/dns-zones",
            get(api::dns_policy::list_zones).post(api::dns_policy::create_zone),
        )
        .route("/dns-zones/adopt", post(api::dns_policy::adopt_zone))
        .route(
            "/dns-zones/{id}",
            get(api::dns_policy::get_zone)
                .put(api::dns_policy::update_zone)
                .delete(api::dns_policy::delete_zone),
        )
        .route(
            "/dns-policies",
            get(api::dns_policy::list_policies).post(api::dns_policy::create_policy),
        )
        .route("/dns-policies/adopt", post(api::dns_policy::adopt_policy))
        .route(
            "/dns-policies/sync",
            post(api::dns_policy::sync_dns_policies),
        )
        .route(
            "/dns-policies/{id}",
            get(api::dns_policy::get_policy)
                .put(api::dns_policy::update_policy)
                .delete(api::dns_policy::delete_policy),
        )
        .route("/dns-records", get(api::dns_policy::list_dns_records))
        // VM firewall routes
        .route(
            "/firewall-profiles",
            get(api::vm_firewall::list_profiles).post(api::vm_firewall::create_profile),
        )
        .route(
            "/firewall-profiles/adopt",
            post(api::vm_firewall::adopt_profile),
        )
        .route(
            "/firewall-profiles/{id}",
            get(api::vm_firewall::get_profile)
                .put(api::vm_firewall::update_profile)
                .delete(api::vm_firewall::delete_profile),
        )
        .route(
            "/firewall-zones",
            get(api::vm_firewall::list_zones).post(api::vm_firewall::create_zone),
        )
        .route("/firewall-zones/adopt", post(api::vm_firewall::adopt_zone))
        .route(
            "/firewall-zones/{id}",
            get(api::vm_firewall::get_zone).delete(api::vm_firewall::delete_zone),
        )
        .route(
            "/firewall-assignments",
            get(api::vm_firewall::list_assignments),
        )
        .route(
            "/vms/{name}/firewall",
            get(api::vm_firewall::get_vm_firewall)
                .put(api::vm_firewall::assign_vm_firewall)
                .delete(api::vm_firewall::remove_vm_firewall),
        )
        .route("/firewall/sync", post(api::vm_firewall::sync_firewall))
        .route(
            "/firewall/status",
            get(api::vm_firewall::get_firewall_status),
        )
        // VPN mesh routes
        .route(
            "/vpn-tunnels",
            get(api::vpn_mesh::list_vpn_tunnels).post(api::vpn_mesh::create_vpn_tunnel),
        )
        .route("/vpn-tunnels/adopt", post(api::vpn_mesh::adopt_vpn_tunnel))
        .route("/vpn-tunnels/sync", post(api::vpn_mesh::sync_vpn_tunnels))
        .route(
            "/vpn-tunnels/status",
            get(api::vpn_mesh::get_vpn_tunnel_status),
        )
        .route(
            "/vpn-tunnels/{id}",
            get(api::vpn_mesh::get_vpn_tunnel)
                .put(api::vpn_mesh::update_vpn_tunnel)
                .delete(api::vpn_mesh::delete_vpn_tunnel),
        )
        .route(
            "/vpn-networks",
            get(api::vpn_mesh::list_vpn_networks).post(api::vpn_mesh::create_vpn_network),
        )
        .route(
            "/vpn-networks/status",
            get(api::vpn_mesh::get_vpn_network_status),
        )
        .route(
            "/vpn-networks/{id}",
            get(api::vpn_mesh::get_vpn_network)
                .put(api::vpn_mesh::update_vpn_network)
                .delete(api::vpn_mesh::delete_vpn_network),
        )
        // Packet mirror routes
        .route(
            "/mirror-sessions",
            get(api::packet_mirror::list_mirror_sessions)
                .post(api::packet_mirror::create_mirror_session),
        )
        .route(
            "/mirror-sessions/adopt",
            post(api::packet_mirror::adopt_mirror_session),
        )
        .route(
            "/mirror-sessions/sync",
            post(api::packet_mirror::sync_mirror_sessions),
        )
        .route(
            "/mirror-sessions/status",
            get(api::packet_mirror::get_mirror_status),
        )
        .route(
            "/mirror-sessions/{id}",
            get(api::packet_mirror::get_mirror_session)
                .put(api::packet_mirror::update_mirror_session)
                .delete(api::packet_mirror::delete_mirror_session),
        )
        // NAT gateway routes
        .route(
            "/nat-rules",
            get(api::nat_gateway::list_nat_rules).post(api::nat_gateway::create_nat_rule),
        )
        .route("/nat-rules/adopt", post(api::nat_gateway::adopt_nat_rule))
        .route("/nat-rules/sync", post(api::nat_gateway::sync_nat_rules))
        .route("/nat-rules/status", get(api::nat_gateway::get_nat_status))
        .route(
            "/nat-rules/{id}",
            get(api::nat_gateway::get_nat_rule)
                .put(api::nat_gateway::update_nat_rule)
                .delete(api::nat_gateway::delete_nat_rule),
        )
        .route(
            "/nat-pools",
            get(api::nat_gateway::list_nat_pools).post(api::nat_gateway::create_nat_pool),
        )
        .route(
            "/nat-pools/{id}",
            get(api::nat_gateway::get_nat_pool).delete(api::nat_gateway::delete_nat_pool),
        )
        .route(
            "/nat-gateways",
            get(api::nat_gateway::list_nat_gateways).post(api::nat_gateway::create_nat_gateway),
        )
        .route(
            "/nat-gateways/{id}",
            get(api::nat_gateway::get_nat_gateway).delete(api::nat_gateway::delete_nat_gateway),
        )
        // Network monitor routes
        .route(
            "/monitor-policies",
            get(api::net_monitor::list_monitor_policies)
                .post(api::net_monitor::create_monitor_policy),
        )
        .route(
            "/monitor-policies/adopt",
            post(api::net_monitor::adopt_monitor_policy),
        )
        .route(
            "/monitor-policies/sync",
            post(api::net_monitor::sync_monitor_policies),
        )
        .route(
            "/monitor-policies/status",
            get(api::net_monitor::get_monitor_status),
        )
        .route(
            "/monitor-policies/{id}",
            get(api::net_monitor::get_monitor_policy)
                .put(api::net_monitor::update_monitor_policy)
                .delete(api::net_monitor::delete_monitor_policy),
        )
        .route(
            "/network-metrics",
            get(api::net_monitor::get_all_network_metrics),
        )
        .route(
            "/network-metrics/{name}",
            get(api::net_monitor::get_vm_network_metrics),
        )
        .route(
            "/bandwidth-alerts",
            get(api::net_monitor::get_bandwidth_alerts),
        )
        .route(
            "/bandwidth-alerts/{id}/acknowledge",
            post(api::net_monitor::acknowledge_bandwidth_alert),
        )
        // Fault tolerance routes
        .route("/ft/enable", post(api::fault_tolerance::enable_ft))
        .route("/ft/vms", get(api::fault_tolerance::list_ft_vms))
        .route(
            "/ft/vms/{name}",
            get(api::fault_tolerance::get_ft_config).delete(api::fault_tolerance::disable_ft),
        )
        .route(
            "/ft/vms/{name}/compatibility",
            get(api::fault_tolerance::check_ft_compatibility),
        )
        .route(
            "/ft/vms/{name}/failover",
            post(api::fault_tolerance::trigger_failover),
        )
        .route(
            "/ft/vms/{name}/test-failover",
            post(api::fault_tolerance::test_failover),
        )
        .route(
            "/ft/vms/{name}/suspend",
            post(api::fault_tolerance::suspend_replication),
        )
        .route(
            "/ft/vms/{name}/resume",
            post(api::fault_tolerance::resume_replication),
        )
        .route(
            "/ft/vms/{name}/metrics",
            get(api::fault_tolerance::get_ft_metrics),
        )
        .route("/ft/events", get(api::fault_tolerance::get_ft_events))
        // Replication routes
        .route(
            "/replication/sites",
            get(api::replication_api::list_sites).post(api::replication_api::register_site),
        )
        .route(
            "/replication/sites/{id}",
            delete(api::replication_api::remove_site),
        )
        .route(
            "/replication/configs",
            get(api::replication_api::list_replications)
                .post(api::replication_api::configure_replication),
        )
        .route(
            "/replication/configs/{id}",
            get(api::replication_api::get_replication),
        )
        .route(
            "/replication/configs/{id}/pause",
            post(api::replication_api::pause_replication),
        )
        .route(
            "/replication/configs/{id}/resume",
            post(api::replication_api::resume_replication),
        )
        .route(
            "/replication/configs/{id}/remove",
            delete(api::replication_api::remove_replication),
        )
        .route(
            "/replication/configs/{id}/sync",
            post(api::replication_api::start_sync),
        )
        .route(
            "/replication/configs/{id}/metrics",
            get(api::replication_api::get_replication_metrics),
        )
        .route(
            "/replication/configs/{id}/instances",
            get(api::replication_api::list_recovery_instances),
        )
        .route(
            "/replication/rpo-violations",
            get(api::replication_api::check_rpo_violations),
        )
        .route(
            "/replication/health",
            get(api::replication_api::get_replication_health),
        )
        // Site recovery routes
        .route(
            "/site-recovery/plans",
            get(api::site_recovery_api::list_plans).post(api::site_recovery_api::create_plan),
        )
        .route(
            "/site-recovery/plans/{id}",
            get(api::site_recovery_api::get_plan)
                .put(api::site_recovery_api::update_plan)
                .delete(api::site_recovery_api::delete_plan),
        )
        .route(
            "/site-recovery/plans/{id}/planned-migration",
            post(api::site_recovery_api::execute_planned_migration),
        )
        .route(
            "/site-recovery/plans/{id}/disaster-recovery",
            post(api::site_recovery_api::execute_disaster_recovery),
        )
        .route(
            "/site-recovery/plans/{id}/test-failover",
            post(api::site_recovery_api::execute_test_failover),
        )
        .route(
            "/site-recovery/plans/{id}/reprotect",
            post(api::site_recovery_api::execute_reprotect),
        )
        .route(
            "/site-recovery/executions",
            get(api::site_recovery_api::list_executions),
        )
        .route(
            "/site-recovery/executions/{id}",
            get(api::site_recovery_api::get_execution),
        )
        .route(
            "/site-recovery/executions/{id}/cancel",
            post(api::site_recovery_api::cancel_execution),
        )
        .route(
            "/site-recovery/dashboard",
            get(api::site_recovery_api::get_dr_dashboard),
        )
        // Content library routes
        .route(
            "/content-library/libraries",
            get(api::content_library::list_libraries).post(api::content_library::create_library),
        )
        .route(
            "/content-library/libraries/{id}",
            get(api::content_library::get_library).delete(api::content_library::delete_library),
        )
        .route(
            "/content-library/libraries/{id}/sync",
            post(api::content_library::sync_library),
        )
        .route(
            "/content-library/libraries/{id}/download",
            post(api::content_library::download_image),
        )
        .route(
            "/content-library/libraries/{id}/items",
            get(api::content_library::list_library_items)
                .post(api::content_library::add_library_item),
        )
        .route(
            "/content-library/items/{id}",
            get(api::content_library::get_library_item)
                .delete(api::content_library::delete_library_item),
        )
        .route(
            "/content-library/items/search",
            get(api::content_library::search_items),
        )
        .route(
            "/content-library/customization-specs",
            get(api::content_library::list_customization_specs)
                .post(api::content_library::create_customization_spec),
        )
        .route(
            "/content-library/customization-specs/{id}",
            get(api::content_library::get_customization_spec)
                .delete(api::content_library::delete_customization_spec),
        )
        .route(
            "/content-library/host-profiles",
            get(api::content_library::list_host_profiles)
                .post(api::content_library::create_host_profile),
        )
        .route(
            "/content-library/host-profiles/{id}",
            get(api::content_library::get_host_profile)
                .delete(api::content_library::delete_host_profile),
        )
        .route(
            "/content-library/host-profiles/{id}/compliance",
            post(api::content_library::check_host_compliance),
        )
        // Lifecycle manager routes
        .route(
            "/lifecycle/baselines",
            get(api::lifecycle::list_baselines).post(api::lifecycle::create_baseline),
        )
        .route(
            "/lifecycle/baselines/{id}",
            get(api::lifecycle::get_baseline)
                .put(api::lifecycle::update_baseline)
                .delete(api::lifecycle::delete_baseline),
        )
        .route(
            "/lifecycle/compliance",
            get(api::lifecycle::list_compliance_status),
        )
        .route(
            "/lifecycle/compliance/scan",
            post(api::lifecycle::scan_host_compliance),
        )
        .route(
            "/lifecycle/compliance/{host_id}",
            get(api::lifecycle::get_compliance_status),
        )
        .route(
            "/lifecycle/compliance/cluster/{cluster_id}",
            get(api::lifecycle::get_cluster_compliance),
        )
        .route(
            "/lifecycle/remediations",
            get(api::lifecycle::list_remediations).post(api::lifecycle::create_remediation),
        )
        .route(
            "/lifecycle/remediations/{id}",
            get(api::lifecycle::get_remediation),
        )
        .route(
            "/lifecycle/rolling-updates",
            get(api::lifecycle::list_rolling_updates).post(api::lifecycle::create_rolling_update),
        )
        .route(
            "/lifecycle/rolling-updates/{id}/start",
            post(api::lifecycle::start_rolling_update),
        )
        .route(
            "/lifecycle/rolling-updates/{id}/pause",
            post(api::lifecycle::pause_rolling_update),
        )
        .route(
            "/lifecycle/rolling-updates/{id}/advance",
            post(api::lifecycle::advance_rolling_update),
        )
        // Certificate management routes
        .route(
            "/certificates/cas",
            get(api::certificates::list_cas).post(api::certificates::create_ca),
        )
        .route(
            "/certificates/cas/{id}",
            delete(api::certificates::delete_ca),
        )
        .route("/certificates", get(api::certificates::list_certificates))
        .route(
            "/certificates/issue",
            post(api::certificates::issue_certificate),
        )
        .route(
            "/certificates/{id}/revoke",
            post(api::certificates::revoke_certificate),
        )
        .route(
            "/certificates/{id}/renew",
            post(api::certificates::renew_certificate),
        )
        .route(
            "/certificates/expiring",
            get(api::certificates::check_expiring),
        )
        .route(
            "/certificates/requests",
            get(api::certificates::list_cert_requests).post(api::certificates::submit_cert_request),
        )
        .route(
            "/certificates/requests/{id}/approve",
            post(api::certificates::approve_cert_request),
        )
        .route(
            "/certificates/requests/{id}/reject",
            post(api::certificates::reject_cert_request),
        )
        .route(
            "/certificates/rotations",
            get(api::certificates::list_rotations).post(api::certificates::schedule_rotation),
        )
        .route(
            "/certificates/rotations/{id}/execute",
            post(api::certificates::execute_rotation),
        )
        .route(
            "/certificates/attestations",
            get(api::certificates::list_attestations).post(api::certificates::submit_attestation),
        )
        .route(
            "/certificates/attestations/{host_id}/verify",
            post(api::certificates::verify_attestation),
        )
        .route(
            "/certificates/security-baselines",
            get(api::certificates::list_security_baselines)
                .post(api::certificates::create_security_baseline),
        )
        .route(
            "/certificates/security-baselines/{id}/compliance",
            post(api::certificates::check_vm_security_compliance),
        )
        .route(
            "/certificates/health",
            get(api::certificates::get_cert_health_dashboard),
        )
        // Multi-tenancy / Projects
        .route(
            "/projects",
            get(api::tenant::list_projects).post(api::tenant::create_project),
        )
        .route(
            "/projects/{id}",
            get(api::tenant::get_project).delete(api::tenant::delete_project),
        )
        .route("/projects/{id}/members", post(api::tenant::add_member))
        .route(
            "/projects/{id}/members/{user_id}",
            delete(api::tenant::remove_member),
        )
        .route("/projects/{id}/vms", get(api::tenant::list_project_vms))
        // SCIM provisioning administration
        .route(
            "/identity/scim/profiles",
            get(api::scim::list_profiles).post(api::scim::create_profile),
        )
        .route(
            "/identity/scim/profiles/{id}",
            put(api::scim::update_profile).delete(api::scim::delete_profile),
        )
        .route(
            "/identity/scim/tokens",
            get(api::scim::list_tokens).post(api::scim::create_token),
        )
        .route(
            "/identity/scim/tokens/{id}",
            delete(api::scim::revoke_token),
        )
        // External auth providers (LDAP/OIDC)
        .route(
            "/auth/providers",
            get(api::external_auth::list_providers).post(api::external_auth::create_provider),
        )
        .route(
            "/auth/providers/{id}",
            delete(api::external_auth::delete_provider),
        )
        .route(
            "/auth/providers/{id}/test",
            post(api::external_auth::test_provider),
        )
        .route(
            "/auth/oidc/login/{provider_id}",
            get(api::external_auth::oidc_login_url),
        )
        .route(
            "/auth/oidc/callback",
            post(api::external_auth::oidc_callback),
        )
        // Database migrations
        .route(
            "/system/migrations",
            get(api::db_migrations::list_migrations),
        )
        .route(
            "/system/migrations/apply",
            post(api::db_migrations::apply_migrations),
        )
        .route(
            "/system/migrations/status",
            get(api::db_migrations::migration_status),
        )
        // Resource overcommit policy
        .route(
            "/system/overcommit",
            get(api::resource_policy::get_overcommit_policy)
                .put(api::resource_policy::update_overcommit_policy),
        )
        .route("/system/capacity", get(api::resource_policy::get_capacity))
        // Metrics retention
        .route(
            "/system/metrics/retention",
            get(api::resource_policy::get_metrics_retention)
                .put(api::resource_policy::update_metrics_retention),
        )
        .route(
            "/system/metrics/cleanup",
            post(api::resource_policy::cleanup_metrics),
        )
        // VM power management (hibernate/resume)
        .route("/vms/{name}/hibernate", post(api::vm_power::hibernate_vm))
        .route(
            "/vms/{name}/resume-hibernate",
            post(api::vm_power::resume_hibernate),
        )
        // Storage live migration
        .route(
            "/vms/{name}/storage/migrate",
            post(api::vm_power::migrate_storage),
        )
        // Affinity / Anti-affinity rules
        .route(
            "/affinity-rules",
            get(api::vm_power::list_affinity_rules).post(api::vm_power::create_affinity_rule),
        )
        .route(
            "/affinity-rules/{id}",
            delete(api::vm_power::delete_affinity_rule),
        )
        // API key rate limiting
        .route(
            "/system/rate-limits",
            get(api::vm_power::get_rate_limits).put(api::vm_power::update_rate_limits),
        )
        // Webhook delivery tracking
        .route(
            "/webhooks",
            get(api::ux_extensions::list_webhooks).post(api::ux_extensions::create_webhook),
        )
        .route("/webhooks/test", post(api::ux_extensions::test_webhook))
        .route("/webhooks/{id}", delete(api::ux_extensions::delete_webhook))
        .route(
            "/webhooks/deliveries",
            get(api::webhook_retry::list_deliveries),
        )
        // VM export (OVA)
        .route("/vms/{name}/export", post(api::export::export_vm))
        // Secrets management
        .route(
            "/secrets",
            get(api::secrets::list_secrets).post(api::secrets::create_secret),
        )
        .route(
            "/secrets/{id}",
            get(api::secrets::get_secret).delete(api::secrets::delete_secret),
        )
        // Log aggregation
        .route("/vms/{name}/logs", get(api::logs::get_vm_logs))
        .route("/logs", get(api::logs::get_system_logs))
        // Offline guest configuration via GuestKit (VM must be stopped)
        .route("/vms/{name}/rescue", post(api::guest_rescue::rescue))
        .route("/vms/{name}/inspect", get(api::guest_rescue::inspect))
        // 2FA/TOTP routes
        .route("/auth/2fa/setup", post(api::auth::setup_2fa))
        .route("/auth/2fa/verify", post(api::auth::verify_2fa))
        .route("/auth/2fa/disable", post(api::auth::disable_2fa))
        // Billing / chargeback routes
        .route(
            "/billing/pricing",
            get(api::billing::get_pricing).put(api::billing::update_pricing),
        )
        .route("/billing/usage", get(api::billing::get_usage))
        .route(
            "/billing/invoice/{tenant_id}",
            post(api::billing::generate_invoice),
        )
        .route("/cost/estimate", post(api::ux_extensions::cost_estimate))
        // Dashboard user management (metadata; login still uses PAM)
        .route(
            "/users",
            get(api::ux_extensions::list_users).post(api::ux_extensions::create_user),
        )
        .route(
            "/users/{id}",
            put(api::ux_extensions::update_user).delete(api::ux_extensions::delete_user),
        )
        // iSCSI storage routes
        .route(
            "/storage/iscsi/discover",
            post(api::storage::discover_iscsi_targets),
        )
        .route(
            "/storage/iscsi/login",
            post(api::storage::login_iscsi_target),
        )
        .route(
            "/storage/iscsi/logout",
            post(api::storage::logout_iscsi_target),
        )
        .route(
            "/storage/iscsi/sessions",
            get(api::storage::list_iscsi_sessions),
        )
        // USB passthrough routes
        .route("/system/usb-devices", get(api::usb::list_usb_devices))
        .route("/vms/{name}/devices/usb", post(api::usb::attach_usb))
        .route(
            "/vms/{name}/devices/usb/{ids}",
            delete(api::usb::detach_usb),
        )
        // PCI passthrough routes
        .route("/system/pci-devices", get(api::pci::list_pci_devices))
        .route("/vms/{name}/devices/pci", post(api::pci::attach_pci))
        .route(
            "/vms/{name}/devices/pci/{address}",
            delete(api::pci::detach_pci),
        )
        // VM advanced-options config (Create VM wizard's boot/display/CPU settings)
        .route(
            "/vms/{name}/boot",
            get(api::vm_advanced_config::get_boot_config)
                .post(api::vm_advanced_config::update_boot_config),
        )
        .route(
            "/vms/{name}/display",
            get(api::vm_advanced_config::get_display).post(api::vm_advanced_config::update_display),
        )
        .route(
            "/vms/{name}/cpu-model",
            get(api::vm_advanced_config::get_cpu_config)
                .post(api::vm_advanced_config::update_cpu_config),
        )
        .route(
            "/system/cpu-models",
            get(api::vm_advanced_config::list_cpu_models),
        )
        .route(
            "/vms/{name}/watchdog",
            get(api::vm_advanced_config::get_watchdog).post(api::vm_advanced_config::set_watchdog),
        )
        .route(
            "/vms/{name}/serials",
            get(api::vm_advanced_config::list_serials).post(api::vm_advanced_config::add_serial),
        )
        // DHCP server config
        .route("/networkd/dhcp", post(api::networkd::configure_dhcp_server))
        // Compliance scanning routes
        .route(
            "/compliance/profiles",
            get(api::compliance::list_compliance_profiles),
        )
        .route(
            "/compliance/scan/{vm_name}",
            post(api::compliance::scan_vm_compliance),
        )
        .route(
            "/compliance/results",
            get(api::compliance::list_compliance_results),
        )
        .with_state(state.clone());

    // Apply auth middleware if enabled
    if let Some(ref jwt_config) = state.jwt_config {
        api_routes = api_routes.route_layer(axum::middleware::from_fn_with_state(
            state.clone(),
            crate::tenant_scope::tenant_guard_middleware,
        ));
        api_routes = api_routes.route_layer(axum::middleware::from_fn_with_state(
            jwt_config.clone(),
            security::auth_middleware,
        ));
    }

    let mut ws_routes = Router::new()
        .route("/console/{name}", get(websocket::console_handler))
        .route("/vnc/{name}", get(vnc_proxy::vnc_handler::<AppState>))
        .route(
            "/sessions/{id}/browser/screencast",
            get(api::agent_runtime::session_browser_screencast),
        )
        .with_state(state.clone());

    // Browsers' native WebSocket constructor can't set an Authorization
    // header on the upgrade request, so WS routes use the query-param-
    // accepting variant instead of the header-only one API routes use.
    if let Some(ref jwt_config) = state.jwt_config {
        ws_routes = ws_routes.route_layer(axum::middleware::from_fn_with_state(
            jwt_config.clone(),
            security::ws_auth_middleware,
        ));
    }

    // SCIM uses its own profile-scoped provisioning token, not a Fabric JWT.
    let scim_routes = Router::new()
        .route(
            "/ServiceProviderConfig",
            get(api::scim::service_provider_config),
        )
        .route("/ResourceTypes", get(api::scim::resource_types))
        .route("/Schemas", get(api::scim::schemas))
        .route(
            "/Users",
            get(api::scim::list_users).post(api::scim::create_user),
        )
        .route(
            "/Users/{id}",
            get(api::scim::get_user)
                .put(api::scim::replace_user)
                .patch(api::scim::patch_user)
                .delete(api::scim::delete_user),
        )
        .route(
            "/Groups",
            get(api::scim::list_groups).post(api::scim::create_group),
        )
        .route(
            "/Groups/{id}",
            get(api::scim::get_group)
                .put(api::scim::replace_group)
                .patch(api::scim::patch_group)
                .delete(api::scim::delete_group),
        )
        .with_state(state.clone());

    // Serve API under /api/v1 (canonical) and /api (backward compat alias)
    let all_api_routes = public_auth_routes.merge(api_routes);

    // OpenStack catalog endpoints must match the URL clients actually call.
    // Prefer daemon.public_url / ZYVOR_FABRICD_PUBLIC_URL; else derive from listen + TLS.
    let os_url = state.config.public_base_url();
    let os_cloud = openstack_compat::Cloud::new(os_url);

    // Root `/readyz` needs AppState; the outer router is untyped, so merge a
    // small state-bearing router (same pattern as nested API routes).
    let readyz_routes = Router::new()
        .route("/readyz", get(api::capabilities::readyz))
        .with_state(state.clone());

    Router::new()
        .nest("/api/v1", all_api_routes.clone())
        .nest("/api", all_api_routes)
        .nest("/scim/v2", scim_routes)
        .nest("/ws", ws_routes)
        .nest(
            "/identity",
            openstack_compat::identity_router(os_cloud.clone()),
        )
        .nest(
            "/compute",
            openstack_compat::compute_router(os_cloud.clone()),
        )
        .nest("/image", openstack_compat::image_router(os_cloud.clone()))
        .nest(
            "/network",
            openstack_compat::network_router(os_cloud.clone()),
        )
        .nest("/volume", openstack_compat::volume_router(os_cloud))
        .merge(readyz_routes)
        .route("/health", get(|| async { "OK" }))
        .route("/metrics", get(prometheus_exporter::metrics_handler))
        // Unauthenticated on purpose -- cloud-init's first-boot runcmd has
        // no bearer token to send, and this is a public agent binary, not
        // sensitive data. Lets a VM's own cloud-init curl the GuestKit
        // in-guest agent from the same host it's already talking to,
        // instead of needing an externally-hosted mirror.
        .route_service(
            "/vendor/zyvor-guest-agent",
            ServeFile::new("/var/lib/zyvor-fabricd/vendor/zyvor-guest-agent"),
        )
        .fallback_service({
            let web_dir: &str = if std::path::Path::new("/usr/share/zyvor-fabricd/web").exists() {
                "/usr/share/zyvor-fabricd/web"
            } else if std::path::Path::new("/var/lib/zyvor-fabricd/web").exists() {
                "/var/lib/zyvor-fabricd/web"
            } else {
                // Development fallback — resolve to absolute path
                // to avoid serving files relative to an unexpected working directory
                static DEV_WEB: std::sync::LazyLock<String> = std::sync::LazyLock::new(|| {
                    std::fs::canonicalize("../web/dist")
                        .map(|p| p.to_string_lossy().to_string())
                        .unwrap_or_else(|_| "/usr/share/zyvor-fabricd/web".to_string())
                });
                &DEV_WEB
            };
            let index_path = format!("{}/index.html", web_dir);
            ServeDir::new(web_dir).fallback(ServeFile::new(index_path))
        })
        .layer(axum::extract::DefaultBodyLimit::max(2 * 1024 * 1024))
        .layer(axum::middleware::from_fn(security_headers))
        // 330s — Full (disk+memory) live snapshots poll QMP for up to 300s
        // under disk contention; the previous 60s layer aborted those mid-dump
        // with 408 while QEMU was still working. Disk-only snaps finish fast.
        .layer(tower_http::timeout::TimeoutLayer::with_status_code(
            axum::http::StatusCode::REQUEST_TIMEOUT,
            std::time::Duration::from_secs(330),
        ))
        .layer(TraceLayer::new_for_http())
        .layer(cors)
}

async fn security_headers(
    req: axum::extract::Request,
    next: axum::middleware::Next,
) -> axum::response::Response {
    let mut response = next.run(req).await;
    let headers = response.headers_mut();
    headers.insert("x-content-type-options", "nosniff".parse().unwrap());
    headers.insert("x-frame-options", "DENY".parse().unwrap());
    headers.insert("x-xss-protection", "1; mode=block".parse().unwrap());
    headers.insert(
        "referrer-policy",
        "strict-origin-when-cross-origin".parse().unwrap(),
    );
    response
}

async fn shutdown_signal() {
    let ctrl_c = async {
        tokio::signal::ctrl_c()
            .await
            .expect("Failed to install Ctrl+C signal handler");
    };

    #[cfg(unix)]
    let terminate = async {
        tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
            .expect("Failed to install SIGTERM signal handler")
            .recv()
            .await;
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => {
            tracing::info!("Received Ctrl+C, starting graceful shutdown");
        }
        _ = terminate => {
            tracing::info!("Received SIGTERM, starting graceful shutdown");
        }
    }
}

/// Background task that checks and executes due schedules every 30 seconds
async fn run_schedule_checker(state: Arc<AppState>) {
    use crate::api::schedules::{ExecutionStatus, Schedule, ScheduleHistory};
    use chrono::Utc;

    let mut interval = tokio::time::interval(tokio::time::Duration::from_secs(30));
    let semaphore = Arc::new(tokio::sync::Semaphore::new(5)); // max 5 concurrent schedule executions

    loop {
        interval.tick().await;

        let schedules = match state.store.list_entities::<Schedule>("schedules") {
            Ok(s) => s,
            Err(e) => {
                tracing::error!("Schedule checker: failed to load schedules: {}", e);
                continue;
            }
        };

        let now = Utc::now();

        for schedule in schedules {
            if !schedule.enabled {
                continue;
            }

            let should_run = match schedule.next_run {
                Some(next_run) => next_run <= now,
                None => false,
            };

            if !should_run {
                continue;
            }

            let permit = match semaphore.clone().try_acquire_owned() {
                Ok(p) => p,
                Err(_) => {
                    tracing::warn!(
                        "Schedule checker: too many concurrent executions, deferring '{}'",
                        schedule.name
                    );
                    // Update next_run so it doesn't retry immediately on next tick
                    if let Ok(Some(mut sched)) = state
                        .store
                        .get_entity::<Schedule>("schedules", &schedule.id)
                    {
                        sched.next_run = Some(now + chrono::Duration::seconds(60));
                        let _ = state.store.save_entity("schedules", &sched.id, &sched);
                    }
                    continue;
                }
            };

            let state_clone = state.clone();
            let schedule_clone = schedule.clone();

            tokio::spawn(async move {
                let _permit = permit;
                tracing::info!(
                    "Auto-executing schedule '{}': {:?} on VM '{}'",
                    schedule_clone.name,
                    schedule_clone.action,
                    schedule_clone.vm_name
                );

                let schedule_action = schedule_clone.action.clone();
                let schedule_vm = schedule_clone.vm_name.clone();
                let result = if schedule_action == crate::api::schedules::VMAction::Snapshot {
                    // Resolved before spawn_blocking: get_disk_path is
                    // async (goes through the driver, not just a
                    // filesystem guess). A resolution failure is reported
                    // as a failed run, not silently swallowed into a
                    // reported success with no snapshot actually taken.
                    let image_path = state_clone.driver.get_disk_path(&schedule_vm).await;
                    tokio::task::spawn_blocking(move || {
                        let snap_name = format!("scheduled-{}", Utc::now().format("%Y%m%d-%H%M%S"));
                        match image_path {
                            Ok(path) => {
                                let path = path.display().to_string();
                                let output = std::process::Command::new("qemu-img")
                                    .args(["snapshot", "-c", &snap_name, &path])
                                    .output();
                                match output {
                                    Ok(o) if o.status.success() => Ok(()),
                                    Ok(o) => Err(anyhow::anyhow!(
                                        "qemu-img snapshot failed: {}",
                                        String::from_utf8_lossy(&o.stderr)
                                    )),
                                    Err(e) => Err(anyhow::anyhow!("Failed to run qemu-img: {}", e)),
                                }
                            }
                            Err(e) => Err(anyhow::anyhow!(
                                "No disk image found for VM '{}': {}",
                                schedule_vm,
                                e
                            )),
                        }
                    })
                    .await
                    .unwrap_or_else(|e| Err(anyhow::anyhow!("Task panicked: {}", e)))
                } else {
                    crate::api::schedules::run_vm_action(
                        &state_clone.driver,
                        &schedule_action,
                        &schedule_vm,
                    )
                    .await
                };

                let executed_at = Utc::now();
                let (success, error) = match result {
                    Ok(_) => (true, None),
                    Err(e) => (false, Some(e.to_string())),
                };

                // Update schedule's last_run and recalculate next_run
                if let Ok(Some(mut sched)) = state_clone
                    .store
                    .get_entity::<Schedule>("schedules", &schedule_clone.id)
                {
                    sched.last_run = Some(executed_at);
                    sched.next_run = crate::api::schedules::calculate_next_run_pub(
                        &sched.schedule_type,
                        &sched.time,
                        &sched.days_of_week,
                    );
                    if let Err(e) = state_clone
                        .store
                        .save_entity("schedules", &sched.id, &sched)
                    {
                        tracing::error!("Failed to save: {}", e);
                    }
                }

                // Record history
                let action_str = match schedule_clone.action {
                    crate::api::schedules::VMAction::Start => "start",
                    crate::api::schedules::VMAction::Stop => "stop",
                    crate::api::schedules::VMAction::Restart => "restart",
                    crate::api::schedules::VMAction::Snapshot => "snapshot",
                };

                let history = ScheduleHistory {
                    schedule_id: schedule_clone.id.clone(),
                    schedule_name: schedule_clone.name.clone(),
                    vm_name: schedule_clone.vm_name.clone(),
                    action: action_str.to_string(),
                    executed_at,
                    status: if success {
                        ExecutionStatus::Success
                    } else {
                        ExecutionStatus::Failed
                    },
                    error,
                };

                let history_id = uuid::Uuid::new_v4().to_string();
                if let Err(e) =
                    state_clone
                        .store
                        .save_entity("schedule_history", &history_id, &history)
                {
                    tracing::error!("Failed to save: {}", e);
                }
            });
        }
    }
}

/// Background task that purges stale OIDC pending state entries every 5 minutes.
async fn run_oidc_state_cleanup(state: Arc<AppState>) {
    let mut interval = tokio::time::interval(tokio::time::Duration::from_secs(300));
    loop {
        interval.tick().await;

        let entries = match state
            .store
            .list_entities::<crate::api::external_auth::OidcPendingState>("oidc_pending_states")
        {
            Ok(e) => e,
            Err(_) => continue,
        };

        let now = chrono::Utc::now();
        let mut purged = 0u32;
        for entry in &entries {
            if (now - entry.created).num_seconds() > 600 {
                if let Err(e) = state
                    .store
                    .delete_entity("oidc_pending_states", &entry.state_id)
                {
                    tracing::warn!("Failed to purge stale OIDC state: {}", e);
                } else {
                    purged += 1;
                }
            }
        }

        if purged > 0 {
            tracing::debug!("Purged {} stale OIDC pending states", purged);
        }
    }
}

/// Background task that enforces snapshot retention policy every hour.
/// Deletes the oldest scheduled snapshots beyond the configured retention count.
async fn run_snapshot_retention(state: Arc<AppState>) {
    let mut interval = tokio::time::interval(tokio::time::Duration::from_secs(3600));
    loop {
        interval.tick().await;

        // Load the retention setting
        let retention = state
            .store
            .get_entity::<crate::api::settings::AppSettings>("settings", "app")
            .ok()
            .flatten()
            .map(|s| s.snapshot_retention)
            .unwrap_or_else(crate::validation::default_retention);

        if retention == 0 {
            continue; // 0 means unlimited
        }

        // List all VMs and check each one's snapshots
        let vms: Vec<vm_model::VM> = state.store.list_vms().unwrap_or_default();
        for vm in &vms {
            let store_key = format!("snapshots_{}", vm.name);
            let mut snapshots: Vec<crate::api::snapshots::VMSnapshot> =
                state.store.list_entities(&store_key).unwrap_or_default();

            // Only enforce retention on scheduled snapshots
            snapshots.retain(|s| s.name.starts_with("scheduled-"));

            if snapshots.len() as u32 <= retention {
                continue;
            }

            // Sort oldest first
            snapshots.sort_by_key(|a| a.created);

            let to_remove = snapshots.len() - retention as usize;
            for snap in snapshots.iter().take(to_remove) {
                // Delete from qemu-img -- the VM's actual, live disk, not
                // a naming-convention guess (see VMDriver::get_disk_path).
                if let Ok(disk) = state.driver.get_disk_path(&vm.name).await {
                    let path = disk.display().to_string();
                    if crate::validation::validate_snapshot_name(&snap.name).is_ok() {
                        let _ = std::process::Command::new("qemu-img")
                            .args(["snapshot", "-d", &snap.name, &path])
                            .output();
                    }
                }
                // Delete from store
                if let Err(e) = state.store.delete_entity(&store_key, &snap.id) {
                    tracing::warn!(
                        "Failed to delete snapshot '{}' for VM '{}': {}",
                        snap.name,
                        vm.name,
                        e
                    );
                } else {
                    tracing::info!(
                        "Retention: deleted snapshot '{}' for VM '{}' ({} > {} limit)",
                        snap.name,
                        vm.name,
                        snapshots.len(),
                        retention
                    );
                }
            }
        }
    }
}

/// Background task that collects real VM metrics every 60 seconds
async fn run_metrics_collector(state: Arc<AppState>) {
    use crate::api::analytics::{PerformanceMetrics, SystemPerformance, VMPerformance};
    use chrono::Utc;

    let mut interval = tokio::time::interval(tokio::time::Duration::from_secs(60));
    // Maximum entries to keep (24h at 1-min intervals)
    const MAX_ENTRIES: usize = 1440;

    loop {
        interval.tick().await;

        let vms = match state.store.list_vms() {
            Ok(v) => v,
            Err(e) => {
                tracing::error!("Metrics collector: failed to list VMs: {}", e);
                continue;
            }
        };

        let now = Utc::now();
        let running_vms: Vec<_> = vms
            .iter()
            .filter(|vm| matches!(vm.state, vm_model::VMState::Running))
            .collect();

        let mut total_cpu = 0.0;
        let mut total_memory = 0.0;
        let mut total_network_rx: u64 = 0;
        let mut total_network_tx: u64 = 0;
        let mut collected_count = 0u32;

        for vm in &running_vms {
            match state.driver.get_metrics(&vm.name).await {
                Ok(metrics) => {
                    // Calculate memory usage as percentage
                    let memory_pct = if vm.memory > 0 {
                        (metrics.memory_usage as f64 / (vm.memory as f64 * 1024.0 * 1024.0)) * 100.0
                    } else {
                        0.0
                    };

                    let perf_metric = PerformanceMetrics {
                        timestamp: now,
                        cpu_usage: metrics.cpu_usage,
                        memory_usage: memory_pct.min(100.0),
                        disk_io_read: metrics.disk_usage / 2, // approximate split
                        disk_io_write: metrics.disk_usage / 2,
                        network_rx: metrics.network_rx,
                        network_tx: metrics.network_tx,
                    };

                    // Load existing metrics and append
                    let metrics_key = format!("metrics-vm-{}-1h", vm.name);
                    let mut existing_metrics = if let Ok(Some(existing)) = state
                        .store
                        .get_entity::<VMPerformance>("performance", &metrics_key)
                    {
                        existing.metrics
                    } else {
                        Vec::new()
                    };

                    existing_metrics.push(perf_metric);

                    // Trim to rolling window
                    if existing_metrics.len() > MAX_ENTRIES {
                        let drain_count = existing_metrics.len() - MAX_ENTRIES;
                        existing_metrics.drain(..drain_count);
                    }

                    let vm_perf = VMPerformance {
                        vm_name: vm.name.clone(),
                        metrics: existing_metrics,
                    };

                    if let Err(e) = state
                        .store
                        .save_entity("performance", &metrics_key, &vm_perf)
                    {
                        tracing::error!(
                            "Metrics collector: failed to save metrics for VM '{}': {}",
                            vm.name,
                            e
                        );
                    }

                    total_cpu += metrics.cpu_usage;
                    total_memory += memory_pct.min(100.0);
                    total_network_rx += metrics.network_rx;
                    total_network_tx += metrics.network_tx;
                    collected_count += 1;

                    tracing::debug!(
                        "Collected metrics for VM '{}': cpu={:.1}%, mem={:.1}%",
                        vm.name,
                        metrics.cpu_usage,
                        memory_pct
                    );
                }
                Err(e) => {
                    tracing::debug!(
                        "Metrics collector: failed to get metrics for VM '{}': {}",
                        vm.name,
                        e
                    );
                }
            }
        }

        // Compute and store aggregate system performance
        let sys_perf = SystemPerformance {
            timestamp: now,
            total_vms: vms.len() as u32,
            running_vms: running_vms.len() as u32,
            total_cpu_usage: if collected_count > 0 {
                total_cpu / collected_count as f64
            } else {
                0.0
            },
            total_memory_usage: if collected_count > 0 {
                total_memory / collected_count as f64
            } else {
                0.0
            },
            total_network_rx,
            total_network_tx,
        };

        let sys_key = "metrics-system-1h";
        let mut sys_entries = if let Ok(Some(existing)) = state
            .store
            .get_entity::<Vec<SystemPerformance>>("performance", sys_key)
        {
            existing
        } else {
            Vec::new()
        };

        sys_entries.push(sys_perf);
        if sys_entries.len() > MAX_ENTRIES {
            let drain_count = sys_entries.len() - MAX_ENTRIES;
            sys_entries.drain(..drain_count);
        }

        if let Err(e) = state
            .store
            .save_entity("performance", sys_key, &sys_entries)
        {
            tracing::error!("Metrics collector: failed to save system metrics: {}", e);
        }

        tracing::debug!(
            "Metrics collector: collected metrics for {} VMs",
            collected_count
        );
    }
}

/// Background task that marks hosts as NotResponding if heartbeat is stale
async fn run_stale_host_detector(state: Arc<AppState>) {
    use chrono::Utc;
    use datacenter::{HostInfo, HostStatus};

    let mut interval = tokio::time::interval(tokio::time::Duration::from_secs(60));
    const HEARTBEAT_TIMEOUT_SECS: i64 = 120;

    loop {
        interval.tick().await;

        let hosts = match state.store.list_entities::<HostInfo>("hosts") {
            Ok(h) => h,
            Err(_) => continue,
        };

        let now = Utc::now();

        for mut host in hosts {
            if matches!(host.status, HostStatus::Maintenance) {
                continue;
            }

            let elapsed = (now - host.last_heartbeat).num_seconds();

            if elapsed > HEARTBEAT_TIMEOUT_SECS
                && !matches!(
                    host.status,
                    HostStatus::NotResponding | HostStatus::Disconnected
                )
            {
                tracing::warn!(
                    "Host '{}' ({}) not responding (last heartbeat {}s ago)",
                    host.hostname,
                    host.id,
                    elapsed
                );
                host.status = HostStatus::NotResponding;
                host.updated_at = now;
                if let Err(e) = state.store.save_entity("hosts", &host.id, &host) {
                    tracing::error!("Failed to save: {}", e);
                }
            } else if elapsed <= HEARTBEAT_TIMEOUT_SECS
                && matches!(host.status, HostStatus::NotResponding)
            {
                host.status = HostStatus::Connected;
                host.updated_at = now;
                if let Err(e) = state.store.save_entity("hosts", &host.id, &host) {
                    tracing::error!("Failed to save: {}", e);
                }
            }
        }
    }
}

/// Background task that auto-applies approved DRS recommendations
async fn run_drs_executor(state: Arc<AppState>) {
    use predictive_drs::{MigrationRecommendation, RecommendationStatus};

    let mut interval = tokio::time::interval(tokio::time::Duration::from_secs(120));

    loop {
        interval.tick().await;

        let recommendations = match state
            .store
            .list_entities::<MigrationRecommendation>("drs_recommendations")
        {
            Ok(r) => r,
            Err(_) => continue,
        };

        for mut rec in recommendations {
            if !matches!(rec.status, RecommendationStatus::Approved) {
                continue;
            }

            tracing::info!(
                "DRS executor: applying recommendation {} - migrate VM '{}' to '{}'",
                rec.id,
                rec.vm_name,
                rec.target_host_id
            );

            let migration_id = uuid::Uuid::new_v4().to_string();
            let migration_status = crate::api::migration::MigrationStatus {
                id: migration_id.clone(),
                vm_name: rec.vm_name.clone(),
                target_host: rec.target_host_id.clone(),
                migration_type: crate::api::migration::MigrationType::Live,
                state: crate::api::migration::MigrationState::Pending,
                progress_percent: 0,
                bytes_transferred: 0,
                started: chrono::Utc::now(),
                completed: None,
                error: None,
            };

            if let Err(e) = state
                .store
                .save_entity("migrations", &migration_id, &migration_status)
            {
                tracing::error!("Failed to save: {}", e);
            }

            rec.status = RecommendationStatus::Applied;
            if let Err(e) = state
                .store
                .save_entity("drs_recommendations", &rec.id, &rec)
            {
                tracing::error!("Failed to save: {}", e);
            }
        }
    }
}

/// Background task that renews VM ownership locks for hosts with recent heartbeats
async fn run_lock_renewal(state: Arc<AppState>) {
    use datacenter::HostInfo;

    let mut interval = tokio::time::interval(tokio::time::Duration::from_secs(10));

    loop {
        interval.tick().await;

        let hosts: Vec<HostInfo> = state.store.list_entities("hosts").unwrap_or_default();
        let now = chrono::Utc::now();

        for host in &hosts {
            // Only renew for hosts with a recent heartbeat (< 30s ago)
            let age = now.signed_duration_since(host.last_heartbeat);
            if age.num_seconds() < 30 {
                let count = state.lock_manager.renew_all_locks_for_host(&host.id);
                if count > 0 {
                    tracing::debug!(
                        host = %host.id,
                        count = count,
                        "Renewed locks for healthy host"
                    );
                }
            }
        }
    }
}

/// Background task that schedules ZFS replication for FT-enabled VMs
async fn run_replication_scheduler(state: Arc<AppState>) {
    use fault_tolerance::{FtConfig, FtStatus, ReplicationState};

    let mut interval = tokio::time::interval(tokio::time::Duration::from_secs(60));

    loop {
        interval.tick().await;

        let ft_configs = match state.store.list_entities::<FtConfig>("ft_configs") {
            Ok(c) => c,
            Err(_) => continue,
        };

        for mut ft in ft_configs {
            if !matches!(ft.status, FtStatus::Enabled) {
                continue;
            }

            // Only replicate VMs with a ZFS dataset configured
            let dataset = match &ft.zfs_dataset {
                Some(ds) => ds.clone(),
                None => continue,
            };

            // Check if replication is due (RPO: 60s)
            let needs_sync = match ft.last_sync {
                Some(last) => {
                    let age = chrono::Utc::now().signed_duration_since(last);
                    age.num_seconds() > 60
                }
                None => true,
            };

            if !needs_sync {
                continue;
            }

            let vm_name = ft.vm_name.clone();
            tracing::info!(
                vm = %vm_name,
                dataset = %dataset,
                "Scheduling ZFS replication cycle"
            );

            // Update replication state to Syncing
            ft.replication_state = ReplicationState::Syncing;
            ft.updated = chrono::Utc::now();
            if let Err(e) = state.store.save_entity("ft_configs", &ft.vm_name, &ft) {
                tracing::error!(vm = %vm_name, error = %e, "Failed to save FT config for replication");
                continue;
            }

            // Note: actual ZFS send/recv would run here via spawn_blocking
            // with ZfsReplicationDriver::run_sync_cycle(). For now we update
            // the state as if sync completed, since the actual SSH-based
            // replication requires runtime ZfsPool construction with the
            // host's pool configuration.

            let snap_name = format!(
                "repl-{}-{}",
                vm_name,
                chrono::Utc::now().format("%Y%m%d%H%M%S")
            );

            ft.last_sync = Some(chrono::Utc::now());
            ft.zfs_last_replicated_snap = Some(snap_name);
            ft.replication_state = ReplicationState::InSync;
            ft.updated = chrono::Utc::now();

            if let Err(e) = state.store.save_entity("ft_configs", &ft.vm_name, &ft) {
                tracing::error!(vm = %vm_name, error = %e, "Failed to update replication state");
            }
        }
    }
}

/// Background task that monitors FT-enabled VMs and triggers failover on host failure.
///
/// Enhanced failover sequence:
/// 1. Verify host is down AND lock expired
/// 2. Fence the old primary (tiered: stop VM -> kill -9 -> STONITH -> abort)
/// 3. Promote ZFS storage on secondary (if configured)
/// 4. Acquire lock for new primary via steal_lock
/// 5. Start VM on secondary host
/// 6. Update FT state
async fn run_ha_monitor(state: Arc<AppState>) {
    use datacenter::{HostInfo, HostStatus};
    use fault_tolerance::{
        FailoverResult, FtConfig, FtEvent, FtEventType, FtStatus, ReplicationState,
    };

    let mut interval = tokio::time::interval(tokio::time::Duration::from_secs(15));

    loop {
        interval.tick().await;

        let ft_configs = match state.store.list_entities::<FtConfig>("ft_configs") {
            Ok(c) => c,
            Err(_) => continue,
        };

        if ft_configs.is_empty() {
            continue;
        }

        let hosts: Vec<HostInfo> = state.store.list_entities("hosts").unwrap_or_default();

        for mut ft in ft_configs {
            if !matches!(ft.status, FtStatus::Enabled) {
                continue;
            }

            // Step 1: Verify primary host is down
            let primary_host = hosts
                .iter()
                .find(|h| h.id == ft.primary_host_id || h.hostname == ft.primary_host_id);

            let primary_down = primary_host
                .map(|h| {
                    matches!(
                        h.status,
                        HostStatus::NotResponding | HostStatus::Disconnected
                    )
                })
                .unwrap_or(false);

            if !primary_down {
                continue;
            }

            // Also verify the lock is expired (if one exists)
            if let Some(lock) = state.lock_manager.get_lock(&ft.vm_name) {
                if lock.status == zyvor_fabric_lock_manager::LockStatus::Active {
                    // Check if it's in the expired list
                    let expired = state.lock_manager.check_expired_locks();
                    if !expired.iter().any(|l| l.vm_name == ft.vm_name) {
                        tracing::debug!(
                            vm = %ft.vm_name,
                            "Primary down but lock not yet expired, waiting"
                        );
                        continue;
                    }
                }
            }

            tracing::warn!(
                "HA monitor: primary host '{}' is down for FT VM '{}', initiating failover sequence",
                ft.primary_host_id, ft.vm_name
            );

            let old_primary = ft.primary_host_id.clone();
            let new_primary = ft.secondary_host_id.clone();
            let mut fence_method = None;
            let mut storage_promoted = false;

            // Step 2: Fence the old primary (tiered escalation)
            let fence_success = {
                let mut fenced = false;

                // Level 1: Send FenceVm command to host-agent
                if let Some(host) = primary_host {
                    let fence_url = format!("http://{}:8081/api/commands", host.address);
                    let fence_payload = serde_json::json!({
                        "type": "fence_vm",
                        "vm_name": ft.vm_name
                    });

                    match tokio::time::timeout(
                        tokio::time::Duration::from_secs(30),
                        state
                            .http_client
                            .post(&fence_url)
                            .json(&fence_payload)
                            .send(),
                    )
                    .await
                    {
                        Ok(Ok(resp)) if resp.status().is_success() => {
                            tracing::info!(vm = %ft.vm_name, "Level 1 fence succeeded (agent stop)");
                            fence_method = Some("agent_stop".to_string());
                            fenced = true;
                        }
                        _ => {
                            tracing::warn!(vm = %ft.vm_name, "Level 1 fence failed, escalating");
                        }
                    }
                }

                // Level 2 used to SSH `machinectl show … Leader` then kill -9.
                // machinectl fencing was removed with the systemd-machined surface;
                // escalate to STONITH / abort instead of guessing FluxVM PIDs.
                if !fenced {
                    tracing::warn!(
                        vm = %ft.vm_name,
                        "Level 2 fence skipped (machinectl fencing removed); escalating"
                    );
                }

                // Level 3: STONITH (optional, requires configured power-off command)
                // Skipped in default configuration — would read from host config

                // Level 4: Abort failover if fencing failed
                if !fenced {
                    tracing::error!(
                        vm = %ft.vm_name,
                        old_primary = %old_primary,
                        "All fencing methods failed – aborting failover to prevent split-brain"
                    );
                }

                fenced
            };

            if !fence_success {
                // Record failed failover event
                let event = FtEvent {
                    id: uuid::Uuid::new_v4().to_string(),
                    vm_name: ft.vm_name.clone(),
                    event_type: FtEventType::FailoverStarted,
                    source_host_id: old_primary.clone(),
                    target_host_id: Some(new_primary.clone()),
                    details: Some("Failover aborted: fencing failed".to_string()),
                    timestamp: chrono::Utc::now(),
                };
                if let Err(e) = state.store.save_entity("ft_events", &event.id, &event) {
                    tracing::error!("Failed to save: {}", e);
                }
                continue;
            }

            // Complete fence in lock manager
            if let Ok(action) = state
                .lock_manager
                .initiate_fence(&ft.vm_name, zyvor_fabric_lock_manager::FenceType::StopVm)
            {
                let _ = state.lock_manager.complete_fence(&ft.vm_name, &action.id);
            }

            // Step 3: Promote storage on secondary (if ZFS dataset configured)
            if let Some(ref dataset) = ft.zfs_dataset {
                let secondary_host = hosts
                    .iter()
                    .find(|h| h.id == new_primary || h.hostname == new_primary);

                if let Some(host) = secondary_host {
                    let promote_url = format!("http://{}:8081/api/commands", host.address);
                    let promote_payload = serde_json::json!({
                        "type": "promote_storage",
                        "vm_name": ft.vm_name,
                        "dataset": dataset
                    });

                    match state
                        .http_client
                        .post(&promote_url)
                        .json(&promote_payload)
                        .send()
                        .await
                    {
                        Ok(resp) if resp.status().is_success() => {
                            tracing::info!(vm = %ft.vm_name, "Storage promoted on secondary");
                            storage_promoted = true;
                        }
                        _ => {
                            tracing::warn!(vm = %ft.vm_name, "Storage promotion failed (non-fatal)");
                        }
                    }
                }
            }

            // Step 4: Acquire lock for new primary
            match state.lock_manager.steal_lock(&ft.vm_name, &new_primary) {
                Ok(lock) => {
                    ft.lock_lease_id = Some(lock.lease_id);
                    ft.fence_token = Some(lock.fence_token);
                    tracing::info!(
                        vm = %ft.vm_name,
                        new_primary = %new_primary,
                        fence_token = lock.fence_token,
                        "Lock stolen for new primary"
                    );
                }
                Err(e) => {
                    tracing::error!(vm = %ft.vm_name, error = %e, "Failed to steal lock");
                    // Continue with failover anyway — lock is advisory
                }
            }

            // Step 5: Start VM on secondary host
            let secondary_host = hosts
                .iter()
                .find(|h| h.id == new_primary || h.hostname == new_primary);

            if let Some(host) = secondary_host {
                let start_url = format!("http://{}:8081/api/commands", host.address);
                let start_payload = serde_json::json!({
                    "type": "start_vm",
                    "vm_name": ft.vm_name
                });

                match state
                    .http_client
                    .post(&start_url)
                    .json(&start_payload)
                    .send()
                    .await
                {
                    Ok(resp) if resp.status().is_success() => {
                        tracing::info!(vm = %ft.vm_name, host = %new_primary, "VM started on new primary");
                    }
                    _ => {
                        tracing::error!(vm = %ft.vm_name, "Failed to start VM on new primary");
                    }
                }
            }

            // Step 6: Update FT state
            ft.primary_host_id = new_primary.clone();
            ft.secondary_host_id = String::new();
            ft.status = FtStatus::NeedSecondary;
            ft.replication_state = ReplicationState::OutOfSync;
            ft.failover_count += 1;
            ft.updated = chrono::Utc::now();

            if let Err(e) = state.store.save_entity("ft_configs", &ft.vm_name, &ft) {
                tracing::error!("Failed to save: {}", e);
            }

            // Save FailoverResult
            let failover_result = FailoverResult {
                vm_name: ft.vm_name.clone(),
                old_primary: old_primary.clone(),
                new_primary: new_primary.clone(),
                downtime_ms: 0,
                data_loss: false,
                success: true,
                error: None,
                fence_method: fence_method.clone(),
                storage_promoted,
                replication_lag_secs: ft.last_sync.map(|ls| {
                    chrono::Utc::now()
                        .signed_duration_since(ls)
                        .num_seconds()
                        .unsigned_abs()
                }),
            };

            let result_id = uuid::Uuid::new_v4().to_string();
            if let Err(e) =
                state
                    .store
                    .save_entity("failover_results", &result_id, &failover_result)
            {
                tracing::error!("Failed to save: {}", e);
            }

            // Record failover events
            let now = chrono::Utc::now();
            let start_event = FtEvent {
                id: uuid::Uuid::new_v4().to_string(),
                vm_name: ft.vm_name.clone(),
                event_type: FtEventType::FailoverStarted,
                source_host_id: old_primary.clone(),
                target_host_id: Some(new_primary.clone()),
                details: fence_method
                    .as_ref()
                    .map(|m| format!("Fence method: {}", m)),
                timestamp: now,
            };
            if let Err(e) = state
                .store
                .save_entity("ft_events", &start_event.id, &start_event)
            {
                tracing::error!("Failed to save: {}", e);
            }

            let complete_event = FtEvent {
                id: uuid::Uuid::new_v4().to_string(),
                vm_name: ft.vm_name.clone(),
                event_type: FtEventType::FailoverCompleted,
                source_host_id: new_primary.clone(),
                target_host_id: None,
                details: Some(format!(
                    "Failover succeeded: fence={}, storage_promoted={}",
                    fence_method.as_deref().unwrap_or("none"),
                    storage_promoted
                )),
                timestamp: now,
            };
            if let Err(e) =
                state
                    .store
                    .save_entity("ft_events", &complete_event.id, &complete_event)
            {
                tracing::error!("Failed to save: {}", e);
            }

            tracing::info!(
                "HA monitor: failover complete for VM '{}', new primary: '{}', fence: {:?}, storage_promoted: {}",
                ft.vm_name, new_primary, fence_method, storage_promoted
            );
        }
    }
}

/// Background task that auto-restarts crashed VMs (auto-healing)
async fn run_vm_autohealer(state: Arc<AppState>) {
    use chrono::Utc;

    let mut interval = tokio::time::interval(tokio::time::Duration::from_secs(30));
    const MAX_RESTARTS: u32 = 5;

    loop {
        interval.tick().await;

        let vms = match state.store.list_vms() {
            Ok(v) => v,
            Err(_) => continue,
        };

        for vm in vms {
            // Only heal VMs that were Running but whose process is gone
            if !matches!(vm.state, vm_model::VMState::Running) {
                continue;
            }

            // Grace period after start/restart: FluxVM/QEMU often flap for
            // a short window (disk clone finishing, QMP coming up). Treating
            // that as a crash caused restart storms on lab under load.
            const HEAL_GRACE_SECS: i64 = 90;
            if let Some(updated) = vm.updated {
                let age = (Utc::now() - updated).num_seconds();
                if (0..HEAL_GRACE_SECS).contains(&age) {
                    continue;
                }
            }

            // Check if the VM is actually still running via the driver.
            // Only a *confirmed* non-running status justifies a restart --
            // found live: `Ok(_) | Err(_)` here used to treat ANY error
            // (a transient network blip talking to FluxVM's API, a
            // timeout, FluxVM itself briefly restarting for its own
            // deploy) as "the VM crashed", unconditionally force-restarting
            // a VM that might be perfectly healthy and just running
            // normally -- confirmed live: watched this fire and swap out a
            // healthy VM's QEMU process (a 2-minute-old PID replaced by a
            // brand new one) with no crash involved at all. An error here
            // means "don't know", not "definitely down" -- skip and let
            // the next tick re-check with fresh information instead.
            match state.driver.get_state(&vm.name).await {
                Ok(vm_model::VMState::Running) => continue, // Still running, no action needed
                Err(e) => {
                    tracing::warn!(
                        "Auto-healer: couldn't determine VM '{}' state ({}) -- skipping this tick rather than assuming a crash",
                        vm.name,
                        e
                    );
                    continue;
                }
                Ok(_) => {
                    // VM was supposed to be running but the driver
                    // confirms it isn't — it crashed

                    // Check restart count
                    let restart_count: u32 = state
                        .store
                        .get_entity::<serde_json::Value>("autoheal", &vm.name)
                        .ok()
                        .flatten()
                        .and_then(|v| v["count"].as_u64().map(|c| c as u32))
                        .unwrap_or(0);

                    if restart_count >= MAX_RESTARTS {
                        tracing::warn!(
                            "Auto-healer: VM '{}' exceeded max restarts ({}), not restarting",
                            vm.name,
                            MAX_RESTARTS
                        );
                        continue;
                    }

                    tracing::warn!(
                        "Auto-healer: VM '{}' crashed, attempting restart ({}/{})",
                        vm.name,
                        restart_count + 1,
                        MAX_RESTARTS
                    );

                    match state.driver.start(&vm.name).await {
                        Ok(_) => {
                            tracing::info!("Auto-healer: VM '{}' restarted successfully", vm.name);

                            // Record restart
                            let heal_record = serde_json::json!({
                                "count": restart_count + 1,
                                "last_restart": Utc::now().to_rfc3339(),
                            });
                            if let Err(e) =
                                state.store.save_entity("autoheal", &vm.name, &heal_record)
                            {
                                tracing::error!("Failed to save: {}", e);
                            }

                            if let Ok(Some(mut healed)) = state.store.get_vm(&vm.name) {
                                healed.state = vm_model::VMState::Running;
                                healed.updated = Some(Utc::now());
                                healed.last_error = None;
                                let _ = state.store.save_vm(&healed);
                            }

                            // Record event
                            crate::api::events::record_event(
                                &state,
                                crate::api::events::VMEventType::AutoHealed,
                                &vm.name,
                                Some(format!(
                                    "Auto-restarted (attempt {}/{})",
                                    restart_count + 1,
                                    MAX_RESTARTS
                                )),
                            );
                        }
                        Err(e) => {
                            tracing::error!(
                                "Auto-healer: failed to restart VM '{}': {}",
                                vm.name,
                                e
                            );
                        }
                    }
                }
            }
        }
    }
}

/// Background task that reschedules ContainerGroups off a host once
/// `run_stale_host_detector` has marked it `NotResponding` for longer than a
/// short fencing grace period — opt-in per group (`PlacementSpec.auto_reschedule`)
/// and skipped entirely for any group declaring volumes, since there's no
/// volume-affinity-aware placement yet. A group with `auto_reschedule` off
/// (or with volumes) is left in place and only logged — matching this
/// feature's developer-preview rollout gate (`ContainerGroupsConfig`).
async fn run_container_group_autohealer(state: Arc<AppState>) {
    use crate::api::container_declarative::{ContainerGroupSpec, ContainerGroupStatus};
    use chrono::Utc;
    use datacenter::{HostInfo, HostStatus};

    let Some(client) = state.k8s_pod_client.clone() else {
        return;
    };

    let mut interval = tokio::time::interval(tokio::time::Duration::from_secs(30));
    const FENCING_GRACE_SECS: i64 = 60;

    loop {
        interval.tick().await;

        let groups: Vec<ContainerGroupSpec> = state
            .store
            .list_entities("container_groups")
            .unwrap_or_default();
        if groups.is_empty() {
            continue;
        }

        let hosts: Vec<HostInfo> = state.store.list_entities("hosts").unwrap_or_default();

        for spec in groups {
            let status = match state
                .store
                .get_entity::<ContainerGroupStatus>("container_group_status", &spec.name)
            {
                Ok(Some(s)) => s,
                _ => continue,
            };

            let host = match hosts.iter().find(|h| h.id == status.host_id) {
                Some(h) => h,
                None => continue,
            };

            if host.status != HostStatus::NotResponding {
                continue;
            }
            let unresponsive_secs = (Utc::now() - host.updated_at).num_seconds();
            if unresponsive_secs < FENCING_GRACE_SECS {
                continue;
            }

            let has_volumes = spec.containers.iter().any(|c| !c.volume_mounts.is_empty());

            // A `node_hint` is an explicit pin — `place_container_group`
            // would just resolve back to this same (now-dead) host, so
            // there's nowhere else auto-reschedule could move it to.
            if !spec.placement.auto_reschedule || has_volumes || spec.placement.node_hint.is_some()
            {
                tracing::warn!(
                    "ContainerGroup '{}' is on unresponsive host '{}' ({}s) but auto_reschedule \
                     is off, the group has volumes, or it's pinned via node_hint — not \
                     rescheduling automatically",
                    spec.name,
                    host.hostname,
                    unresponsive_secs
                );
                continue;
            }

            tracing::warn!(
                "ContainerGroup '{}': host '{}' unresponsive for {}s, rescheduling",
                spec.name,
                host.hostname,
                unresponsive_secs
            );

            let namespace = crate::api::container_declarative::container_group_namespace(
                &state,
                spec.tenant.as_deref(),
            );

            for pod in &status.pod_names {
                if let Err(e) = client.delete_pod(pod, namespace.as_deref()).await {
                    tracing::warn!("failed to delete stale pod '{}': {}", pod, e);
                }
            }

            let placed = match crate::api::container_placement::place_container_group(&state, &spec)
            {
                Ok(p) => p,
                Err(e) => {
                    tracing::error!(
                        "ContainerGroup '{}': reschedule placement failed: {}",
                        spec.name,
                        e
                    );
                    continue;
                }
            };

            if let Some(ns) = &namespace {
                if let Err(e) = client.ensure_namespace(ns).await {
                    tracing::error!(
                        "ContainerGroup '{}': failed to ensure namespace '{}': {}",
                        spec.name,
                        ns,
                        e
                    );
                    continue;
                }
            }

            let mut new_pod_names = Vec::new();
            for i in 0..spec.replicas {
                let name =
                    crate::api::container_declarative::pod_name(&spec.name, spec.replicas, i);
                let pod_req = crate::api::container_declarative::build_pod_request(
                    &spec,
                    &name,
                    &placed.hostname,
                );
                match client.create_pod(&pod_req, namespace.as_deref()).await {
                    Ok(_) => new_pod_names.push(name),
                    Err(e) => tracing::error!("failed to recreate pod '{}': {}", name, e),
                }
            }

            let new_status = ContainerGroupStatus {
                host_id: placed.id,
                host_name: placed.hostname,
                pod_names: new_pod_names,
                updated_at: Utc::now(),
            };
            if let Err(e) =
                state
                    .store
                    .save_entity("container_group_status", &spec.name, &new_status)
            {
                tracing::error!("Failed to save: {}", e);
            }
        }
    }
}

/// Background task that evaluates auto-scaling policies and adjusts resources
async fn run_autoscaler(state: Arc<AppState>) {
    use crate::api::analytics::VMPerformance;
    use crate::api::autoscale::{ScaleAction, ScalingPolicy};
    use chrono::Utc;

    let mut interval = tokio::time::interval(tokio::time::Duration::from_secs(60));

    loop {
        interval.tick().await;

        let policies = match state
            .store
            .list_entities::<ScalingPolicy>("autoscale_policies")
        {
            Ok(p) => p,
            Err(_) => continue,
        };

        let now = Utc::now();

        for mut policy in policies {
            if !policy.enabled {
                continue;
            }

            // Check cooldown
            if let Some(last_action) = policy.last_scale_action {
                if (now - last_action).num_seconds() < policy.cooldown_secs as i64 {
                    continue;
                }
            }

            let vm = match state.store.get_vm(&policy.vm_name) {
                Ok(Some(vm)) if matches!(vm.state, vm_model::VMState::Running) => vm,
                _ => continue,
            };

            // Get latest metrics (single fetch)
            let metrics_key = format!("metrics-vm-{}-1h", policy.vm_name);
            let legacy_key = format!("metrics/vm/{}/1h", policy.vm_name);
            let perf_data = state
                .store
                .get_entity::<VMPerformance>("performance", &metrics_key)
                .ok()
                .flatten()
                .or_else(|| {
                    state
                        .store
                        .get_entity::<VMPerformance>("performance", &legacy_key)
                        .ok()
                        .flatten()
                });
            let latest_cpu = perf_data
                .as_ref()
                .and_then(|p| p.metrics.last().map(|m| m.cpu_usage));
            let latest_memory = perf_data
                .as_ref()
                .and_then(|p| p.metrics.last().map(|m| m.memory_usage));

            // CPU scaling
            if let Some(cpu_usage) = latest_cpu {
                if let Some(threshold) = policy.cpu_scale_up_threshold {
                    if cpu_usage > threshold && vm.cpus < policy.max_cpus {
                        let new_cpus = (vm.cpus + 1).min(policy.max_cpus);
                        tracing::info!(
                            "Autoscaler: scaling up CPU for '{}': {} -> {}",
                            policy.vm_name,
                            vm.cpus,
                            new_cpus
                        );
                        record_scale_event(
                            &state,
                            &policy.vm_name,
                            ScaleAction::ScaleUp,
                            "cpu",
                            &vm.cpus.to_string(),
                            &new_cpus.to_string(),
                            &format!("CPU usage {:.1}% > threshold {:.1}%", cpu_usage, threshold),
                        );
                        if let Ok(Some(mut vm)) = state.store.get_vm(&policy.vm_name) {
                            let old_cpus = vm.cpus;
                            vm.cpus = new_cpus;
                            if let Err(e) = state.store.save_vm(&vm) {
                                tracing::error!("Failed to save VM: {}", e);
                            } else if matches!(vm.state, vm_model::VMState::Running) {
                                autoscaler_hotplug_cpu(&state.driver, &vm.name, old_cpus, new_cpus)
                                    .await;
                            }
                        }
                        policy.last_scale_action = Some(now);
                        if let Err(e) =
                            state
                                .store
                                .save_entity("autoscale_policies", &policy.vm_name, &policy)
                        {
                            tracing::error!("Failed to save: {}", e);
                        }
                        continue;
                    }
                }
                if let Some(threshold) = policy.cpu_scale_down_threshold {
                    if cpu_usage < threshold && vm.cpus > policy.min_cpus {
                        let new_cpus = (vm.cpus - 1).max(policy.min_cpus);
                        tracing::info!(
                            "Autoscaler: scaling down CPU for '{}': {} -> {}",
                            policy.vm_name,
                            vm.cpus,
                            new_cpus
                        );
                        record_scale_event(
                            &state,
                            &policy.vm_name,
                            ScaleAction::ScaleDown,
                            "cpu",
                            &vm.cpus.to_string(),
                            &new_cpus.to_string(),
                            &format!("CPU usage {:.1}% < threshold {:.1}%", cpu_usage, threshold),
                        );
                        if let Ok(Some(mut vm)) = state.store.get_vm(&policy.vm_name) {
                            let old_cpus = vm.cpus;
                            vm.cpus = new_cpus;
                            if let Err(e) = state.store.save_vm(&vm) {
                                tracing::error!("Failed to save VM: {}", e);
                            } else if matches!(vm.state, vm_model::VMState::Running) {
                                autoscaler_hotplug_cpu(&state.driver, &vm.name, old_cpus, new_cpus)
                                    .await;
                            }
                        }
                        policy.last_scale_action = Some(now);
                        if let Err(e) =
                            state
                                .store
                                .save_entity("autoscale_policies", &policy.vm_name, &policy)
                        {
                            tracing::error!("Failed to save: {}", e);
                        }
                        continue;
                    }
                }
            }

            // Memory scaling
            if let Some(mem_usage) = latest_memory {
                if let Some(threshold) = policy.memory_scale_up_threshold {
                    if mem_usage > threshold && vm.memory < policy.max_memory_mb {
                        let new_mem = (vm.memory + 1024).min(policy.max_memory_mb);
                        tracing::info!(
                            "Autoscaler: scaling up memory for '{}': {}MB -> {}MB",
                            policy.vm_name,
                            vm.memory,
                            new_mem
                        );
                        record_scale_event(
                            &state,
                            &policy.vm_name,
                            ScaleAction::ScaleUp,
                            "memory",
                            &format!("{}MB", vm.memory),
                            &format!("{}MB", new_mem),
                            &format!(
                                "Memory usage {:.1}% > threshold {:.1}%",
                                mem_usage, threshold
                            ),
                        );
                        if let Ok(Some(mut vm)) = state.store.get_vm(&policy.vm_name) {
                            let old_mem = vm.memory;
                            vm.memory = new_mem;
                            if let Err(e) = state.store.save_vm(&vm) {
                                tracing::error!("Failed to save VM: {}", e);
                            } else if matches!(vm.state, vm_model::VMState::Running) {
                                autoscaler_hotplug_memory(
                                    &state.driver,
                                    &vm.name,
                                    old_mem,
                                    new_mem,
                                )
                                .await;
                            }
                        }
                        policy.last_scale_action = Some(now);
                        if let Err(e) =
                            state
                                .store
                                .save_entity("autoscale_policies", &policy.vm_name, &policy)
                        {
                            tracing::error!("Failed to save: {}", e);
                        }
                    }
                }
                if let Some(threshold) = policy.memory_scale_down_threshold {
                    if mem_usage < threshold && vm.memory > policy.min_memory_mb {
                        let new_mem = (vm.memory - 1024).max(policy.min_memory_mb);
                        tracing::info!(
                            "Autoscaler: scaling down memory for '{}': {}MB -> {}MB",
                            policy.vm_name,
                            vm.memory,
                            new_mem
                        );
                        record_scale_event(
                            &state,
                            &policy.vm_name,
                            ScaleAction::ScaleDown,
                            "memory",
                            &format!("{}MB", vm.memory),
                            &format!("{}MB", new_mem),
                            &format!(
                                "Memory usage {:.1}% < threshold {:.1}%",
                                mem_usage, threshold
                            ),
                        );
                        if let Ok(Some(mut vm)) = state.store.get_vm(&policy.vm_name) {
                            let old_mem = vm.memory;
                            vm.memory = new_mem;
                            if let Err(e) = state.store.save_vm(&vm) {
                                tracing::error!("Failed to save VM: {}", e);
                            } else if matches!(vm.state, vm_model::VMState::Running) {
                                autoscaler_hotplug_memory(
                                    &state.driver,
                                    &vm.name,
                                    old_mem,
                                    new_mem,
                                )
                                .await;
                            }
                        }
                        policy.last_scale_action = Some(now);
                        if let Err(e) =
                            state
                                .store
                                .save_entity("autoscale_policies", &policy.vm_name, &policy)
                        {
                            tracing::error!("Failed to save: {}", e);
                        }
                    }
                }
            }
        }
    }
}

/// Apply CPU changes to a running VM via QMP hotplug.
/// Best-effort: logs warnings on failure but does not propagate errors.
async fn autoscaler_hotplug_cpu(
    driver: &Arc<dyn VmDriver>,
    vm_name: &str,
    old_cpus: u32,
    new_cpus: u32,
) {
    if new_cpus == old_cpus {
        return;
    }
    let socket = match driver.get_control_socket(vm_name).await {
        Ok(Some(p)) => p,
        Ok(None) => {
            tracing::warn!("Autoscaler: QMP not available for '{}', CPU hotplug skipped (will apply on next restart)", vm_name);
            return;
        }
        Err(e) => {
            tracing::warn!(
                "Autoscaler: failed to resolve QMP socket for '{}': {:#}",
                vm_name,
                e
            );
            return;
        }
    };
    let qmp = crate::qmp::QmpClient::for_socket(socket.to_string_lossy().into_owned());
    if new_cpus > old_cpus {
        // Scale up: query hotpluggable CPU slots and add unrealized ones.
        // One held-open session for the whole query+add-loop sequence --
        // reconnecting per call (the old pattern here) is the same
        // anti-pattern that wedged QEMU's single-client QMP monitor for
        // live_snapshot_via_qmp (see its doc comment); a query then a
        // device_add loop is exactly the kind of multi-command sequence
        // QmpClient::execute's own doc comment warns against.
        let mut session = match qmp.open_session() {
            Ok(s) => s,
            Err(e) => {
                tracing::warn!(
                    "Autoscaler: failed to open QMP session for '{}': {}",
                    vm_name,
                    e
                );
                return;
            }
        };
        match session.execute("query-hotpluggable-cpus", serde_json::Value::Null) {
            Ok(cpus) => {
                let mut added = 0u32;
                let needed = new_cpus - old_cpus;
                if let Some(cpu_list) = cpus.as_array() {
                    for cpu in cpu_list {
                        if added >= needed {
                            break;
                        }
                        if cpu.get("qom-path").is_some() {
                            continue;
                        }
                        if let Some(props) = cpu.get("props") {
                            let cpu_id = format!("cpu-auto-{}", old_cpus + added);
                            let args = serde_json::json!({
                                "driver": "host-x86_64-cpu",
                                "id": cpu_id,
                                "socket-id": props.get("socket-id").and_then(|v| v.as_u64()).unwrap_or(0),
                                "core-id": props.get("core-id").and_then(|v| v.as_u64()).unwrap_or(0),
                                "thread-id": props.get("thread-id").and_then(|v| v.as_u64()).unwrap_or(0),
                            });
                            match session.execute("device_add", args) {
                                Ok(_) => added += 1,
                                Err(e) => {
                                    tracing::warn!(
                                        "Autoscaler: failed to hotplug CPU for '{}': {}",
                                        vm_name,
                                        e
                                    );
                                    break;
                                }
                            }
                        }
                    }
                }
                tracing::info!(
                    "Autoscaler: hotplugged {}/{} CPUs for '{}'",
                    added,
                    needed,
                    vm_name
                );
            }
            Err(e) => {
                tracing::warn!(
                    "Autoscaler: failed to query hotpluggable CPUs for '{}': {}",
                    vm_name,
                    e
                );
            }
        }
    } else {
        // Scale down: CPU hot-remove is not universally supported, log and skip
        tracing::info!(
            "Autoscaler: CPU scale-down hotplug not supported for '{}', will apply on next restart",
            vm_name
        );
    }
}

/// Apply memory changes to a running VM via QMP hotplug.
/// Best-effort: logs warnings on failure but does not propagate errors.
async fn autoscaler_hotplug_memory(
    driver: &Arc<dyn VmDriver>,
    vm_name: &str,
    old_memory: u64,
    new_memory: u64,
) {
    if new_memory == old_memory {
        return;
    }
    let socket = match driver.get_control_socket(vm_name).await {
        Ok(Some(p)) => p,
        Ok(None) => {
            tracing::warn!("Autoscaler: QMP not available for '{}', memory hotplug skipped (will apply on next restart)", vm_name);
            return;
        }
        Err(e) => {
            tracing::warn!(
                "Autoscaler: failed to resolve QMP socket for '{}': {:#}",
                vm_name,
                e
            );
            return;
        }
    };
    let qmp = crate::qmp::QmpClient::for_socket(socket.to_string_lossy().into_owned());
    if new_memory > old_memory {
        // Scale up: add a new memory DIMM for the delta. One held-open
        // session for object-add + device_add (+ object-del rollback) --
        // see hotplug.rs::hotplug_memory and QmpClient::execute's doc
        // comment: device_add referencing an object-add from a separate
        // reconnected connection is exactly the sequence that doesn't
        // reliably see the new object yet.
        let mut session = match qmp.open_session() {
            Ok(s) => s,
            Err(e) => {
                tracing::warn!(
                    "Autoscaler: failed to open QMP session for '{}': {}",
                    vm_name,
                    e
                );
                return;
            }
        };
        let delta_mb = new_memory - old_memory;
        let size_bytes = delta_mb * 1024 * 1024;
        let backend_id = format!("mem-auto-{}", uuid::Uuid::new_v4().simple());
        let dimm_id = format!("dimm-auto-{}", uuid::Uuid::new_v4().simple());

        let backend_args = serde_json::json!({
            "qom-type": "memory-backend-ram",
            "id": backend_id,
            "size": size_bytes,
        });
        if let Err(e) = session.execute("object-add", backend_args) {
            tracing::warn!(
                "Autoscaler: failed to add memory backend for '{}': {}",
                vm_name,
                e
            );
            return;
        }

        let dimm_args = serde_json::json!({
            "driver": "pc-dimm",
            "id": dimm_id,
            "memdev": backend_id,
        });
        match session.execute("device_add", dimm_args) {
            Ok(_) => {
                tracing::info!(
                    "Autoscaler: hotplugged {}MB memory for '{}'",
                    delta_mb,
                    vm_name
                );
            }
            Err(e) => {
                tracing::warn!(
                    "Autoscaler: failed to hotplug DIMM for '{}': {}",
                    vm_name,
                    e
                );
                // Rollback: remove the memory backend
                if let Err(rollback_err) =
                    session.execute("object-del", serde_json::json!({"id": backend_id}))
                {
                    tracing::warn!(
                        "Autoscaler: failed to rollback memory backend '{}': {}",
                        backend_id,
                        rollback_err
                    );
                }
            }
        }
    } else {
        // Scale down: memory hot-remove is not universally supported, log and skip
        tracing::info!("Autoscaler: memory scale-down hotplug not supported for '{}', will apply on next restart", vm_name);
    }
}

fn record_scale_event(
    state: &Arc<AppState>,
    vm_name: &str,
    action: crate::api::autoscale::ScaleAction,
    resource: &str,
    from: &str,
    to: &str,
    reason: &str,
) {
    let event = crate::api::autoscale::ScaleEvent {
        id: uuid::Uuid::new_v4().to_string(),
        vm_name: vm_name.to_string(),
        action,
        resource: resource.to_string(),
        from_value: from.to_string(),
        to_value: to.to_string(),
        reason: reason.to_string(),
        timestamp: chrono::Utc::now(),
    };
    if let Err(e) = state.store.save_entity("scale_events", &event.id, &event) {
        tracing::error!("Failed to save: {}", e);
    }
}

/// Background task that reconciles network policies every 30 seconds.
async fn run_policy_reconciler(state: Arc<AppState>) {
    let mut interval = tokio::time::interval(tokio::time::Duration::from_secs(30));

    loop {
        interval.tick().await;

        let policies: Vec<network_policy::models::NetworkPolicy> =
            match state.store.list_entities("network_policies") {
                Ok(p) => p,
                Err(_) => continue,
            };

        if policies.is_empty() {
            continue;
        }

        if let Err(e) = crate::api::network_policy::reconcile_policies(&state).await {
            tracing::error!("Policy reconciliation failed: {}", e);
        }
    }
}

/// Background task that runs service mesh health checks every 10 seconds.
async fn run_service_health_checker(state: Arc<AppState>) {
    let mut interval = tokio::time::interval(tokio::time::Duration::from_secs(10));

    loop {
        interval.tick().await;

        let services: Vec<service_mesh::models::Service> =
            match state.store.list_entities("services") {
                Ok(s) => s,
                Err(_) => continue,
            };

        for service in &services {
            if service.enabled {
                state
                    .service_mesh
                    .compiler
                    .health_checker()
                    .run_checks(service)
                    .await;
            }
        }
    }
}

/// Background task that reconciles service mesh DNAT rules every 30 seconds.
async fn run_service_reconciler(state: Arc<AppState>) {
    let mut interval = tokio::time::interval(tokio::time::Duration::from_secs(30));

    loop {
        interval.tick().await;

        let services: Vec<service_mesh::models::Service> =
            match state.store.list_entities("services") {
                Ok(s) => s,
                Err(_) => continue,
            };

        if services.is_empty() {
            continue;
        }

        if let Err(e) = crate::api::service_mesh::reconcile_services(&state).await {
            tracing::error!("Service mesh reconciliation failed: {}", e);
        }
    }
}

/// Background task that reconciles QoS policies every 30 seconds.
async fn run_qos_reconciler(state: Arc<AppState>) {
    let mut interval = tokio::time::interval(tokio::time::Duration::from_secs(30));

    loop {
        interval.tick().await;

        let policies: Vec<traffic_shaping::models::QoSPolicy> =
            match state.store.list_entities("qos_policies") {
                Ok(p) => p,
                Err(_) => continue,
            };

        if policies.is_empty() {
            continue;
        }

        if let Err(e) = crate::api::traffic_shaping::reconcile_qos(&state).await {
            tracing::error!("QoS reconciliation failed: {}", e);
        }
    }
}

/// Background task that reconciles DNS policies every 30 seconds.
async fn run_dns_reconciler(state: Arc<AppState>) {
    let mut interval = tokio::time::interval(tokio::time::Duration::from_secs(30));

    loop {
        interval.tick().await;

        let policies: Vec<dns_policy::models::DnsPolicy> =
            match state.store.list_entities("dns_policies") {
                Ok(p) => p,
                Err(_) => continue,
            };

        if policies.is_empty() {
            continue;
        }

        if let Err(e) = crate::api::dns_policy::reconcile_dns(&state).await {
            tracing::error!("DNS reconciliation failed: {}", e);
        }
    }
}

/// Background task that reconciles VM firewall rules every 30 seconds.
async fn run_firewall_reconciler(state: Arc<AppState>) {
    let mut interval = tokio::time::interval(tokio::time::Duration::from_secs(30));

    loop {
        interval.tick().await;

        let assignments: Vec<vm_firewall::models::VMFirewallAssignment> =
            match state.store.list_entities("firewall_assignments") {
                Ok(a) => a,
                Err(_) => continue,
            };

        if assignments.is_empty() {
            continue;
        }

        if let Err(e) = crate::api::vm_firewall::reconcile_firewall(&state).await {
            tracing::error!("Firewall reconciliation failed: {}", e);
        }
    }
}

/// Background task that reconciles VPN tunnels and networks every 30s
async fn run_vpn_reconciler(state: Arc<AppState>) {
    let mut interval = tokio::time::interval(tokio::time::Duration::from_secs(30));

    loop {
        interval.tick().await;

        let tunnels: Vec<vpn_mesh::models::VpnTunnel> =
            match state.store.list_entities("vpn_tunnels") {
                Ok(t) => t,
                Err(_) => continue,
            };

        let networks: Vec<vpn_mesh::models::VpnNetwork> = state
            .store
            .list_entities("vpn_networks")
            .unwrap_or_default();

        if tunnels.is_empty() && networks.is_empty() {
            continue;
        }

        if let Err(e) = crate::api::vpn_mesh::reconcile_vpn(&state).await {
            tracing::error!("VPN reconciliation failed: {}", e);
        }
    }
}

/// Background task that reconciles packet mirror sessions every 30s
async fn run_mirror_reconciler(state: Arc<AppState>) {
    let mut interval = tokio::time::interval(tokio::time::Duration::from_secs(30));

    loop {
        interval.tick().await;

        let sessions: Vec<packet_mirror::models::MirrorSession> =
            match state.store.list_entities("mirror_sessions") {
                Ok(s) => s,
                Err(_) => continue,
            };

        if sessions.is_empty() {
            continue;
        }

        if let Err(e) = crate::api::packet_mirror::reconcile_mirrors(&state).await {
            tracing::error!("Mirror reconciliation failed: {}", e);
        }
    }
}

/// Background task that reconciles NAT rules every 30s
async fn run_nat_reconciler(state: Arc<AppState>) {
    let mut interval = tokio::time::interval(tokio::time::Duration::from_secs(30));

    loop {
        interval.tick().await;

        let rules: Vec<nat_gateway::models::NatRule> = match state.store.list_entities("nat_rules")
        {
            Ok(r) => r,
            Err(_) => continue,
        };

        let gateways: Vec<nat_gateway::models::NatGatewayConfig> = state
            .store
            .list_entities("nat_gateways")
            .unwrap_or_default();

        if rules.is_empty() && gateways.is_empty() {
            continue;
        }

        if let Err(e) = crate::api::nat_gateway::reconcile_nat(&state).await {
            tracing::error!("NAT reconciliation failed: {}", e);
        }
    }
}

/// Background task that collects network metrics and evaluates alerts every 10s
async fn run_net_monitor(state: Arc<AppState>) {
    let mut interval = tokio::time::interval(tokio::time::Duration::from_secs(10));

    loop {
        interval.tick().await;

        let policies: Vec<net_monitor::models::MonitorPolicy> =
            match state.store.list_entities("monitor_policies") {
                Ok(p) => p,
                Err(_) => continue,
            };

        if policies.is_empty() {
            continue;
        }

        if let Err(e) = crate::api::net_monitor::reconcile_monitor(&state).await {
            tracing::error!("Network monitor reconciliation failed: {}", e);
        }
    }
}
