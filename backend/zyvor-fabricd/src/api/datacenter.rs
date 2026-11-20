// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: LicenseRef-Zyvor-Production-1.0

use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use chrono::Utc;
use std::sync::Arc;
use uuid::Uuid;

use crate::server::AppState;
use datacenter::{
    Cluster, CreateClusterRequest, CreateDatacenterRequest, Datacenter, DatacenterStatus,
    DatacenterSummary, HostHeartbeat, HostInfo, HostStatus, RegisterHostRequest,
    UpdateClusterRequest, UpdateDatacenterRequest, UpdateHostRequest,
};
use security::{RequireAdmin, RequireRead, RequireWrite};

// ============================================================================
// Datacenter handlers
// ============================================================================

pub async fn list_datacenters(
    RequireRead(_claims): RequireRead,
    State(state): State<Arc<AppState>>,
) -> impl IntoResponse {
    tracing::debug!("datacenter::{}", stringify!(list_datacenters));
    let items: Vec<Datacenter> = state
        .store
        .list_entities("datacenters")
        .unwrap_or_else(|e| {
            tracing::error!("Storage error: {}", e);
            Vec::new()
        });
    Json(items)
}

pub async fn create_datacenter(
    RequireWrite(_claims): RequireWrite,
    State(state): State<Arc<AppState>>,
    Json(req): Json<CreateDatacenterRequest>,
) -> impl IntoResponse {
    tracing::debug!("datacenter::{}", stringify!(create_datacenter));
    if let Err((status, msg)) = crate::validation::validate_entity_name(&req.name) {
        return (status, Json(serde_json::json!({"error": msg}))).into_response();
    }
    let now = Utc::now();
    let dc = Datacenter {
        id: Uuid::new_v4().to_string(),
        name: req.name,
        description: req.description,
        clusters: Vec::new(),
        created_at: now,
        updated_at: now,
        status: DatacenterStatus::Active,
    };
    match state.store.save_entity("datacenters", &dc.id, &dc) {
        Ok(_) => (StatusCode::CREATED, Json(dc)).into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": e.to_string()})),
        )
            .into_response(),
    }
}

pub async fn get_datacenter(
    RequireRead(_claims): RequireRead,
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    tracing::debug!("datacenter::{}", stringify!(get_datacenter));
    match state.store.get_entity::<Datacenter>("datacenters", &id) {
        Ok(Some(dc)) => Json(dc).into_response(),
        Ok(None) => (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({"error": "Datacenter not found"})),
        )
            .into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": e.to_string()})),
        )
            .into_response(),
    }
}

pub async fn update_datacenter(
    RequireWrite(_claims): RequireWrite,
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    Json(req): Json<UpdateDatacenterRequest>,
) -> impl IntoResponse {
    tracing::debug!("datacenter::{}", stringify!(update_datacenter));
    if let Some(ref name) = req.name {
        if let Err((status, msg)) = crate::validation::validate_entity_name(name) {
            return (status, Json(serde_json::json!({"error": msg}))).into_response();
        }
    }
    let dc = match state.store.get_entity::<Datacenter>("datacenters", &id) {
        Ok(Some(dc)) => dc,
        Ok(None) => {
            return (
                StatusCode::NOT_FOUND,
                Json(serde_json::json!({"error": "Datacenter not found"})),
            )
                .into_response()
        }
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"error": e.to_string()})),
            )
                .into_response()
        }
    };
    let mut dc = dc;
    if let Some(name) = req.name {
        dc.name = name;
    }
    if let Some(description) = req.description {
        dc.description = description;
    }
    if let Some(status) = req.status {
        dc.status = status;
    }
    dc.updated_at = Utc::now();
    if let Err(e) = state.store.save_entity("datacenters", &dc.id, &dc) {
        tracing::error!("Failed to save entity: {}", e);
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": e.to_string()})),
        )
            .into_response();
    }
    Json(dc).into_response()
}

pub async fn delete_datacenter(
    RequireAdmin(_claims): RequireAdmin,
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    tracing::debug!("datacenter::{}", stringify!(delete_datacenter));
    let clusters: Vec<Cluster> = state.store.list_entities("clusters").unwrap_or_default();
    if clusters.iter().any(|c| c.datacenter_id == id) {
        return (
            StatusCode::CONFLICT,
            Json(serde_json::json!({"error": "Cannot delete datacenter with active clusters"})),
        )
            .into_response();
    }
    if let Err(e) = state.store.delete_entity("datacenters", &id) {
        tracing::error!("Failed to delete entity: {}", e);
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": e.to_string()})),
        )
            .into_response();
    }
    StatusCode::NO_CONTENT.into_response()
}

pub async fn get_datacenter_summary(
    RequireRead(_claims): RequireRead,
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    tracing::debug!("datacenter::{}", stringify!(get_datacenter_summary));
    let dc = match state.store.get_entity::<Datacenter>("datacenters", &id) {
        Ok(Some(dc)) => dc,
        Ok(None) => {
            return (
                StatusCode::NOT_FOUND,
                Json(serde_json::json!({"error": "Datacenter not found"})),
            )
                .into_response()
        }
        Err(_) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"error": "Failed to load datacenter"})),
            )
                .into_response()
        }
    };
    let hosts: Vec<HostInfo> = state.store.list_entities("hosts").unwrap_or_else(|e| {
        tracing::error!("Storage error: {}", e);
        Vec::new()
    });
    let dc_hosts: Vec<&HostInfo> = hosts.iter().filter(|h| h.datacenter_id == id).collect();
    let summary = DatacenterSummary {
        id: dc.id,
        name: dc.name,
        cluster_count: dc.clusters.len(),
        host_count: dc_hosts.len(),
        vm_count: dc_hosts.iter().map(|h| h.vm_count).sum(),
        total_cpus: dc_hosts.iter().map(|h| h.cpus).sum(),
        total_memory_mb: dc_hosts.iter().map(|h| h.memory_mb).sum(),
    };
    Json(summary).into_response()
}

// ============================================================================
// Cluster handlers
// ============================================================================

pub async fn list_clusters(
    RequireRead(_claims): RequireRead,
    State(state): State<Arc<AppState>>,
) -> impl IntoResponse {
    tracing::debug!("datacenter::{}", stringify!(list_clusters));
    let items: Vec<Cluster> = state.store.list_entities("clusters").unwrap_or_else(|e| {
        tracing::error!("Storage error: {}", e);
        Vec::new()
    });
    Json(items)
}

pub async fn create_cluster(
    RequireWrite(_claims): RequireWrite,
    State(state): State<Arc<AppState>>,
    Json(req): Json<CreateClusterRequest>,
) -> impl IntoResponse {
    tracing::debug!("datacenter::{}", stringify!(create_cluster));
    if let Err((status, msg)) = crate::validation::validate_entity_name(&req.name) {
        return (status, Json(serde_json::json!({"error": msg}))).into_response();
    }
    let now = Utc::now();
    let cluster = Cluster {
        id: Uuid::new_v4().to_string(),
        name: req.name,
        description: req.description,
        datacenter_id: req.datacenter_id,
        hosts: Vec::new(),
        ha_enabled: req.ha_enabled,
        drs_enabled: req.drs_enabled,
        drs_mode: req.drs_mode,
        evc_mode: req.evc_mode,
        created_at: now,
        updated_at: now,
        status: datacenter::ClusterStatus::Active,
    };
    match state.store.save_entity("clusters", &cluster.id, &cluster) {
        Ok(_) => (StatusCode::CREATED, Json(cluster)).into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": e.to_string()})),
        )
            .into_response(),
    }
}

pub async fn get_cluster(
    RequireRead(_claims): RequireRead,
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    tracing::debug!("datacenter::{}", stringify!(get_cluster));
    match state.store.get_entity::<Cluster>("clusters", &id) {
        Ok(Some(c)) => Json(c).into_response(),
        Ok(None) => (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({"error": "Cluster not found"})),
        )
            .into_response(),
        Err(_) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": "Failed to load cluster"})),
        )
            .into_response(),
    }
}

pub async fn update_cluster(
    RequireWrite(_claims): RequireWrite,
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    Json(req): Json<UpdateClusterRequest>,
) -> impl IntoResponse {
    tracing::debug!("datacenter::{}", stringify!(update_cluster));
    if let Some(ref name) = req.name {
        if let Err((status, msg)) = crate::validation::validate_entity_name(name) {
            return (status, Json(serde_json::json!({"error": msg}))).into_response();
        }
    }
    let mut cluster = match state.store.get_entity::<Cluster>("clusters", &id) {
        Ok(Some(c)) => c,
        Ok(None) => {
            return (
                StatusCode::NOT_FOUND,
                Json(serde_json::json!({"error": "Cluster not found"})),
            )
                .into_response()
        }
        Err(_) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"error": "Failed to load cluster"})),
            )
                .into_response()
        }
    };
    if let Some(name) = req.name {
        cluster.name = name;
    }
    if let Some(desc) = req.description {
        cluster.description = desc;
    }
    if let Some(ha) = req.ha_enabled {
        cluster.ha_enabled = ha;
    }
    if let Some(drs) = req.drs_enabled {
        cluster.drs_enabled = drs;
    }
    if let Some(mode) = req.drs_mode {
        cluster.drs_mode = mode;
    }
    if let Some(evc) = req.evc_mode {
        cluster.evc_mode = evc;
    }
    if let Some(status) = req.status {
        cluster.status = status;
    }
    cluster.updated_at = Utc::now();
    if let Err(e) = state.store.save_entity("clusters", &cluster.id, &cluster) {
        tracing::error!("Failed to save entity: {}", e);
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": e.to_string()})),
        )
            .into_response();
    }
    Json(cluster).into_response()
}

pub async fn delete_cluster(
    RequireAdmin(_claims): RequireAdmin,
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    tracing::debug!("datacenter::{}", stringify!(delete_cluster));
    let hosts: Vec<HostInfo> = state.store.list_entities("hosts").unwrap_or_default();
    if hosts.iter().any(|h| h.cluster_id == id) {
        return (
            StatusCode::CONFLICT,
            Json(serde_json::json!({"error": "Cannot delete cluster with active hosts"})),
        )
            .into_response();
    }
    if let Err(e) = state.store.delete_entity("clusters", &id) {
        tracing::error!("Failed to delete entity: {}", e);
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": e.to_string()})),
        )
            .into_response();
    }
    StatusCode::NO_CONTENT.into_response()
}

// ============================================================================
// Host handlers
// ============================================================================

pub async fn list_hosts(
    RequireRead(_claims): RequireRead,
    State(state): State<Arc<AppState>>,
) -> impl IntoResponse {
    tracing::debug!("datacenter::{}", stringify!(list_hosts));
    let items: Vec<HostInfo> = state.store.list_entities("hosts").unwrap_or_else(|e| {
        tracing::error!("Storage error: {}", e);
        Vec::new()
    });
    Json(items)
}

pub async fn register_host(
    RequireAdmin(_claims): RequireAdmin,
    State(state): State<Arc<AppState>>,
    Json(req): Json<RegisterHostRequest>,
) -> impl IntoResponse {
    tracing::debug!("datacenter::{}", stringify!(register_host));
    if let Err(msg) = crate::validation::validate_hostname(&req.hostname) {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"error": msg})),
        )
            .into_response();
    }
    if let Err(msg) = crate::validation::validate_hostname(&req.address) {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"error": format!("Invalid address: {}", msg)})),
        )
            .into_response();
    }
    let now = Utc::now();
    let host = HostInfo {
        id: Uuid::new_v4().to_string(),
        hostname: req.hostname,
        address: req.address,
        cluster_id: req.cluster_id.clone(),
        datacenter_id: String::new(),
        cpus: req.cpus,
        memory_mb: req.memory_mb,
        status: HostStatus::Connected,
        last_heartbeat: now,
        vm_count: 0,
        cpu_usage_pct: 0.0,
        memory_usage_pct: 0.0,
        agent_version: req.agent_version,
        created_at: now,
        updated_at: now,
        secure_containers_ready: req.secure_containers_ready,
        secure_containers: None,
    };
    match state.store.save_entity("hosts", &host.id, &host) {
        Ok(_) => (StatusCode::CREATED, Json(host)).into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": e.to_string()})),
        )
            .into_response(),
    }
}

pub async fn get_host(
    RequireRead(_claims): RequireRead,
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    tracing::debug!("datacenter::{}", stringify!(get_host));
    match state.store.get_entity::<HostInfo>("hosts", &id) {
        Ok(Some(h)) => Json(h).into_response(),
        Ok(None) => (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({"error": "Host not found"})),
        )
            .into_response(),
        Err(_) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": "Failed to load host"})),
        )
            .into_response(),
    }
}

pub async fn update_host(
    RequireWrite(_claims): RequireWrite,
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    Json(req): Json<UpdateHostRequest>,
) -> impl IntoResponse {
    tracing::debug!("datacenter::{}", stringify!(update_host));
    if let Some(ref hostname) = req.hostname {
        if let Err(msg) = crate::validation::validate_hostname(hostname) {
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({"error": msg})),
            )
                .into_response();
        }
    }
    if let Some(ref address) = req.address {
        if let Err(msg) = crate::validation::validate_hostname(address) {
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({"error": format!("Invalid address: {}", msg)})),
            )
                .into_response();
        }
    }
    let mut host = match state.store.get_entity::<HostInfo>("hosts", &id) {
        Ok(Some(h)) => h,
        Ok(None) => {
            return (
                StatusCode::NOT_FOUND,
                Json(serde_json::json!({"error": "Host not found"})),
            )
                .into_response()
        }
        Err(_) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"error": "Failed to load host"})),
            )
                .into_response()
        }
    };
    if let Some(hostname) = req.hostname {
        host.hostname = hostname;
    }
    if let Some(address) = req.address {
        host.address = address;
    }
    if let Some(cpus) = req.cpus {
        host.cpus = cpus;
    }
    if let Some(memory_mb) = req.memory_mb {
        host.memory_mb = memory_mb;
    }
    if let Some(agent_version) = req.agent_version {
        host.agent_version = agent_version;
    }
    if let Some(status) = req.status {
        host.status = status;
    }
    host.updated_at = Utc::now();
    if let Err(e) = state.store.save_entity("hosts", &host.id, &host) {
        tracing::error!("Failed to save entity: {}", e);
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": e.to_string()})),
        )
            .into_response();
    }
    Json(host).into_response()
}

pub async fn remove_host(
    RequireAdmin(_claims): RequireAdmin,
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    tracing::debug!("datacenter::{}", stringify!(remove_host));
    if let Err(e) = state.store.delete_entity("hosts", &id) {
        tracing::error!("Failed to delete host: {}", e);
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": e.to_string()})),
        )
            .into_response();
    }
    StatusCode::NO_CONTENT.into_response()
}

pub async fn host_heartbeat(
    RequireWrite(_claims): RequireWrite,
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    Json(hb): Json<HostHeartbeat>,
) -> impl IntoResponse {
    tracing::debug!("datacenter::{}", stringify!(host_heartbeat));
    if !hb.cpu_usage_pct.is_finite() || hb.cpu_usage_pct < 0.0 || hb.cpu_usage_pct > 100.0 {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"error": "cpu_usage_pct must be between 0.0 and 100.0"})),
        )
            .into_response();
    }
    if !hb.memory_usage_pct.is_finite() || hb.memory_usage_pct < 0.0 || hb.memory_usage_pct > 100.0
    {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"error": "memory_usage_pct must be between 0.0 and 100.0"})),
        )
            .into_response();
    }
    let mut host = match state.store.get_entity::<HostInfo>("hosts", &id) {
        Ok(Some(h)) => h,
        Ok(None) => {
            return (
                StatusCode::NOT_FOUND,
                Json(serde_json::json!({"error": "Host not found"})),
            )
                .into_response()
        }
        Err(_) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"error": "Failed to load host"})),
            )
                .into_response()
        }
    };
    apply_host_heartbeat_metrics(&mut host, &hb);
    host.last_heartbeat = Utc::now();
    host.updated_at = host.last_heartbeat;
    if let Err(e) = state.store.save_entity("hosts", &host.id, &host) {
        tracing::error!("Failed to save entity: {}", e);
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": e.to_string()})),
        )
            .into_response();
    }
    StatusCode::OK.into_response()
}

pub async fn host_enter_maintenance(
    RequireAdmin(_claims): RequireAdmin,
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    tracing::debug!("datacenter::{}", stringify!(host_enter_maintenance));
    let mut host = match state.store.get_entity::<HostInfo>("hosts", &id) {
        Ok(Some(h)) => h,
        Ok(None) => {
            return (
                StatusCode::NOT_FOUND,
                Json(serde_json::json!({"error": "Host not found"})),
            )
                .into_response()
        }
        Err(_) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"error": "Failed to load host"})),
            )
                .into_response()
        }
    };
    host.status = HostStatus::Maintenance;
    host.updated_at = Utc::now();
    if let Err(e) = state.store.save_entity("hosts", &host.id, &host) {
        tracing::error!("Failed to save entity: {}", e);
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": e.to_string()})),
        )
            .into_response();
    }
    StatusCode::OK.into_response()
}

pub async fn host_exit_maintenance(
    RequireAdmin(_claims): RequireAdmin,
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    tracing::debug!("datacenter::{}", stringify!(host_exit_maintenance));
    let mut host = match state.store.get_entity::<HostInfo>("hosts", &id) {
        Ok(Some(h)) => h,
        Ok(None) => {
            return (
                StatusCode::NOT_FOUND,
                Json(serde_json::json!({"error": "Host not found"})),
            )
                .into_response()
        }
        Err(_) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"error": "Failed to load host"})),
            )
                .into_response()
        }
    };
    host.status = HostStatus::Connected;
    host.updated_at = Utc::now();
    if let Err(e) = state.store.save_entity("hosts", &host.id, &host) {
        tracing::error!("Failed to save entity: {}", e);
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": e.to_string()})),
        )
            .into_response();
    }
    StatusCode::OK.into_response()
}

// ============================================================================
// Host discovery & cluster health
// ============================================================================

#[derive(serde::Deserialize)]
pub struct DiscoverHostRequest {
    pub address: String,
    pub port: Option<u16>,
    pub cluster_id: Option<String>,
}

#[derive(serde::Serialize)]
pub struct DiscoverHostResult {
    pub reachable: bool,
    pub hostname: Option<String>,
    pub cpus: Option<u32>,
    pub memory_mb: Option<u64>,
    pub already_registered: bool,
}

/// POST /api/hosts/discover - Probe a host to check reachability and gather info
pub async fn discover_host(
    RequireRead(_claims): RequireRead,
    State(state): State<Arc<AppState>>,
    Json(req): Json<DiscoverHostRequest>,
) -> impl IntoResponse {
    tracing::debug!("datacenter::{}", stringify!(discover_host));
    let address = req.address.trim();
    let port = req.port.unwrap_or(9095);
    // Check the whole URL. A host-only check leaves the port (and the
    // formatted string) tainted for CodeQL's request-forgery query.
    let base_owned = format!("http://{address}:{port}");
    let base = input_guard::sanitize_probe_url(&base_owned);
    if base.is_empty() {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"error": "Invalid address"})),
        )
            .into_response();
    }
    let url = format!("{base}/health");

    // Check if already registered
    let hosts: Vec<HostInfo> = state.store.list_entities("hosts").unwrap_or_else(|e| {
        tracing::error!("Storage error: {}", e);
        Vec::new()
    });
    let already_registered = hosts.iter().any(|h| h.address == req.address);

    // Probe the host
    let reachable = match state
        .http_client
        .get(&url)
        .timeout(std::time::Duration::from_secs(5))
        .send()
        .await
    {
        Ok(resp) => resp.status().is_success(),
        Err(_) => false,
    };

    let (hostname, cpus, memory_mb) = if reachable {
        // Try to get system info from the remote host
        let info_url = format!("{base}/api/system/cpu/topology");
        let cpu_info = state
            .http_client
            .get(&info_url)
            .send()
            .await
            .ok()
            .filter(|r| r.status().is_success());

        let cpus = if let Some(resp) = cpu_info {
            resp.json::<serde_json::Value>()
                .await
                .ok()
                .and_then(|v| v["total_cpus"].as_u64().map(|c| c as u32))
        } else {
            None
        };

        let mem_url = format!("{base}/api/system/memory");
        let mem_info = state
            .http_client
            .get(&mem_url)
            .send()
            .await
            .ok()
            .filter(|r| r.status().is_success());

        let memory_mb = if let Some(resp) = mem_info {
            resp.json::<serde_json::Value>()
                .await
                .ok()
                .and_then(|v| v["total_kb"].as_u64().map(|k| k / 1024))
        } else {
            None
        };

        (Some(req.address.clone()), cpus, memory_mb)
    } else {
        (None, None, None)
    };

    Json(DiscoverHostResult {
        reachable,
        hostname,
        cpus,
        memory_mb,
        already_registered,
    })
    .into_response()
}

#[derive(serde::Serialize)]
pub struct ClusterHealth {
    pub cluster_id: String,
    pub total_hosts: u32,
    pub connected_hosts: u32,
    pub disconnected_hosts: u32,
    pub maintenance_hosts: u32,
    pub total_vms: u32,
    pub avg_cpu_usage: f64,
    pub avg_memory_usage: f64,
    pub health_status: String,
}

/// GET /api/clusters/:id/health - Get cluster health summary
pub async fn get_cluster_health(
    RequireRead(_claims): RequireRead,
    State(state): State<Arc<AppState>>,
    Path(cluster_id): Path<String>,
) -> impl IntoResponse {
    tracing::debug!("datacenter::{}", stringify!(get_cluster_health));
    // Verify cluster exists
    match state.store.get_entity::<Cluster>("clusters", &cluster_id) {
        Ok(Some(_)) => {}
        Ok(None) => {
            return (
                StatusCode::NOT_FOUND,
                Json(serde_json::json!({"error": "Cluster not found"})),
            )
                .into_response()
        }
        Err(_) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"error": "Failed to load cluster"})),
            )
                .into_response()
        }
    }

    let hosts: Vec<HostInfo> = state.store.list_entities("hosts").unwrap_or_else(|e| {
        tracing::error!("Storage error: {}", e);
        Vec::new()
    });
    let cluster_hosts: Vec<&HostInfo> = hosts
        .iter()
        .filter(|h| h.cluster_id == cluster_id)
        .collect();

    let total = cluster_hosts.len() as u32;
    let connected = cluster_hosts
        .iter()
        .filter(|h| matches!(h.status, HostStatus::Connected))
        .count() as u32;
    let disconnected = cluster_hosts
        .iter()
        .filter(|h| {
            matches!(
                h.status,
                HostStatus::Disconnected | HostStatus::NotResponding
            )
        })
        .count() as u32;
    let maintenance = cluster_hosts
        .iter()
        .filter(|h| matches!(h.status, HostStatus::Maintenance))
        .count() as u32;
    let total_vms: u32 = cluster_hosts.iter().map(|h| h.vm_count).sum();

    let (avg_cpu, avg_mem) = if !cluster_hosts.is_empty() {
        let cpu: f64 =
            cluster_hosts.iter().map(|h| h.cpu_usage_pct).sum::<f64>() / cluster_hosts.len() as f64;
        let mem: f64 = cluster_hosts
            .iter()
            .map(|h| h.memory_usage_pct)
            .sum::<f64>()
            / cluster_hosts.len() as f64;
        (cpu, mem)
    } else {
        (0.0, 0.0)
    };

    let health_status = if disconnected > 0 {
        "degraded"
    } else if total == 0 {
        "empty"
    } else {
        "healthy"
    };

    (
        StatusCode::OK,
        Json(ClusterHealth {
            cluster_id,
            total_hosts: total,
            connected_hosts: connected,
            disconnected_hosts: disconnected,
            maintenance_hosts: maintenance,
            total_vms,
            avg_cpu_usage: avg_cpu,
            avg_memory_usage: avg_mem,
            health_status: health_status.to_string(),
        }),
    )
        .into_response()
}

/// Apply heartbeat metrics onto a host record (CPU/mem/VMs + Secure Containers).
pub(crate) fn apply_host_heartbeat_metrics(host: &mut HostInfo, hb: &HostHeartbeat) {
    host.cpu_usage_pct = hb.cpu_usage_pct;
    host.memory_usage_pct = hb.memory_usage_pct;
    host.vm_count = hb.vm_count;
    host.secure_containers_ready = hb
        .secure_containers
        .as_ref()
        .map(|d| d.available)
        .unwrap_or(hb.secure_containers_ready);
    host.secure_containers = hb.secure_containers.clone();
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use datacenter::SecureContainersDetail;

    fn blank_host() -> HostInfo {
        HostInfo {
            id: "h1".into(),
            hostname: "h1".into(),
            address: "10.0.0.1".into(),
            cluster_id: "c1".into(),
            datacenter_id: "d1".into(),
            cpus: 8,
            memory_mb: 16384,
            status: HostStatus::Connected,
            last_heartbeat: Utc::now(),
            vm_count: 0,
            cpu_usage_pct: 0.0,
            memory_usage_pct: 0.0,
            agent_version: "t".into(),
            created_at: Utc::now(),
            updated_at: Utc::now(),
            secure_containers_ready: false,
            secure_containers: None,
        }
    }

    #[test]
    fn heartbeat_persists_nested_secure_containers_and_derives_ready() {
        let mut host = blank_host();
        let hb = HostHeartbeat {
            cpu_usage_pct: 12.0,
            memory_usage_pct: 34.0,
            vm_count: 2,
            uptime_secs: 99,
            secure_containers_ready: false,
            secure_containers: Some(SecureContainersDetail {
                available: true,
                shim_installed: true,
                guest_image_present: false,
            }),
        };
        apply_host_heartbeat_metrics(&mut host, &hb);
        assert!(host.secure_containers_ready);
        assert_eq!(
            host.secure_containers.as_ref().map(|d| d.shim_installed),
            Some(true)
        );
        assert_eq!(host.cpu_usage_pct, 12.0);
        assert_eq!(host.vm_count, 2);
    }

    #[test]
    fn heartbeat_falls_back_to_flat_ready_flag() {
        let mut host = blank_host();
        let hb = HostHeartbeat {
            cpu_usage_pct: 1.0,
            memory_usage_pct: 2.0,
            vm_count: 0,
            uptime_secs: 1,
            secure_containers_ready: true,
            secure_containers: None,
        };
        apply_host_heartbeat_metrics(&mut host, &hb);
        assert!(host.secure_containers_ready);
        assert!(host.secure_containers.is_none());
    }
}
