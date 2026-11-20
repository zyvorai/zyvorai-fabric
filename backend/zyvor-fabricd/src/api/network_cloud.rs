// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: LicenseRef-Zyvor-Production-1.0

use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::sync::Arc;
use tokio::process::Command;

use crate::server::AppState;
use networking::models::AdoptHostRequest;
use security::{RequireAdmin, RequireRead, RequireWrite};

// ============================================================================
// Floating IP
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FloatingIp {
    pub id: String,
    pub address: String,
    pub interface: String,
    pub assigned_vm: Option<String>,
    #[serde(default = "default_managed_true")]
    pub managed: bool,
    pub created: DateTime<Utc>,
}

fn default_managed_true() -> bool {
    true
}

#[derive(Debug, Deserialize)]
pub struct CreateFloatingIpRequest {
    pub address: String,
    pub interface: String,
}

#[derive(Debug, Deserialize)]
pub struct AssignFloatingIpRequest {
    pub vm_name: String,
}

/// POST /api/floating-ips - Create a floating IP (Admin only)
pub async fn create_floating_ip(
    RequireAdmin(_claims): RequireAdmin,
    State(state): State<Arc<AppState>>,
    Json(req): Json<CreateFloatingIpRequest>,
) -> Result<(StatusCode, Json<FloatingIp>), (StatusCode, Json<serde_json::Value>)> {
    tracing::debug!("network_cloud::{}", stringify!(create_floating_ip));
    // Validate IP address format
    crate::validation::validate_ip_address(&req.address)
        .map_err(|msg| (StatusCode::BAD_REQUEST, Json(json!({ "error": msg }))))?;
    // Validate interface name
    crate::validation::validate_hostname(&req.interface).map_err(|msg| {
        (
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": format!("Invalid interface name: {}", msg) })),
        )
    })?;
    let fip = FloatingIp {
        id: uuid::Uuid::new_v4().to_string(),
        address: req.address,
        interface: req.interface,
        assigned_vm: None,
        managed: true,
        created: Utc::now(),
    };

    state
        .store
        .save_entity("floating_ips", &fip.id, &fip)
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": e.to_string() })),
            )
        })?;

    Ok((StatusCode::CREATED, Json(fip)))
}

/// GET /api/floating-ips - List floating IPs
pub async fn list_floating_ips(
    RequireRead(_claims): RequireRead,
    State(state): State<Arc<AppState>>,
) -> Json<Vec<FloatingIp>> {
    tracing::debug!("network_cloud::{}", stringify!(list_floating_ips));
    let ips: Vec<FloatingIp> = state
        .store
        .list_entities("floating_ips")
        .unwrap_or_else(|e| {
            tracing::error!("Storage error: {}", e);
            Vec::new()
        });
    Json(super::network_cloud_discover::merge_floating_ips(
        &state, ips,
    ))
}

/// POST /api/floating-ips/:id/assign - Assign floating IP to a VM (Admin only)
pub async fn assign_floating_ip(
    RequireAdmin(_claims): RequireAdmin,
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    Json(req): Json<AssignFloatingIpRequest>,
) -> Result<Json<FloatingIp>, (StatusCode, Json<serde_json::Value>)> {
    tracing::debug!("network_cloud::{}", stringify!(assign_floating_ip));
    crate::validation::validate_vm_name(&req.vm_name)
        .map_err(|(s, m)| (s, Json(json!({"error": m}))))?;
    let mut fip = match state.store.get_entity::<FloatingIp>("floating_ips", &id) {
        Ok(Some(f)) => f,
        Ok(None) => {
            return Err((
                StatusCode::NOT_FOUND,
                Json(json!({ "error": "Floating IP not found" })),
            ))
        }
        Err(e) => {
            return Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": e.to_string() })),
            ))
        }
    };

    // Remove from previous VM if assigned
    if let Some(ref _old_vm) = fip.assigned_vm {
        if let Err(e) = Command::new("ip")
            .args([
                "addr",
                "del",
                &format!("{}/32", fip.address),
                "dev",
                &fip.interface,
            ])
            .output()
            .await
        {
            tracing::warn!("Command failed: {}", e);
        }
    }

    // Add IP to the interface associated with the new VM
    let output = Command::new("ip")
        .args([
            "addr",
            "add",
            &format!("{}/32", fip.address),
            "dev",
            &fip.interface,
        ])
        .output()
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": e.to_string() })),
            )
        })?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        // Ignore "already exists" errors
        if !stderr.contains("RTNETLINK answers: File exists") {
            return Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": format!("Failed to add IP: {}", stderr) })),
            ));
        }
    }

    fip.assigned_vm = Some(req.vm_name);
    state
        .store
        .save_entity("floating_ips", &id, &fip)
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": e.to_string() })),
            )
        })?;

    Ok(Json(fip))
}

/// POST /api/floating-ips/:id/unassign - Unassign floating IP from VM (Admin only)
pub async fn unassign_floating_ip(
    RequireAdmin(_claims): RequireAdmin,
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<FloatingIp>, (StatusCode, Json<serde_json::Value>)> {
    tracing::debug!("network_cloud::{}", stringify!(unassign_floating_ip));
    let mut fip = match state.store.get_entity::<FloatingIp>("floating_ips", &id) {
        Ok(Some(f)) => f,
        Ok(None) => {
            return Err((
                StatusCode::NOT_FOUND,
                Json(json!({ "error": "Floating IP not found" })),
            ))
        }
        Err(e) => {
            return Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": e.to_string() })),
            ))
        }
    };

    if let Err(e) = Command::new("ip")
        .args([
            "addr",
            "del",
            &format!("{}/32", fip.address),
            "dev",
            &fip.interface,
        ])
        .output()
        .await
    {
        tracing::warn!("Command failed: {}", e);
    }

    fip.assigned_vm = None;
    state
        .store
        .save_entity("floating_ips", &id, &fip)
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": e.to_string() })),
            )
        })?;

    Ok(Json(fip))
}

/// DELETE /api/floating-ips/:id - Delete a floating IP (Admin only)
pub async fn delete_floating_ip(
    RequireAdmin(_claims): RequireAdmin,
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<StatusCode, (StatusCode, Json<serde_json::Value>)> {
    tracing::debug!("network_cloud::{}", stringify!(delete_floating_ip));
    if let Some(host) = super::network_cloud_discover::find_host_floating_ip(&state, &id) {
        if super::network_cloud_discover::is_host_managed_floating_ip(&host) {
            return Err((
                StatusCode::BAD_REQUEST,
                Json(json!({
                    "error": "This floating IP exists on the host and is not managed by zyvor-fabricd"
                })),
            ));
        }
    }
    if let Ok(Some(fip)) = state.store.get_entity::<FloatingIp>("floating_ips", &id) {
        if let Err(e) = Command::new("ip")
            .args([
                "addr",
                "del",
                &format!("{}/32", fip.address),
                "dev",
                &fip.interface,
            ])
            .output()
            .await
        {
            tracing::warn!("Command failed: {}", e);
        }
    }

    state
        .store
        .delete_entity("floating_ips", &id)
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": e.to_string() })),
            )
        })?;

    Ok(StatusCode::NO_CONTENT)
}

/// POST /api/floating-ips/adopt - Import a host secondary address into zyvor-fabricd
pub async fn adopt_floating_ip(
    RequireAdmin(_claims): RequireAdmin,
    State(state): State<Arc<AppState>>,
    Json(req): Json<AdoptHostRequest>,
) -> Result<Json<FloatingIp>, (StatusCode, Json<serde_json::Value>)> {
    tracing::debug!("network_cloud::{}", stringify!(adopt_floating_ip));
    let host = match super::network_cloud_discover::find_host_floating_ip(&state, &req.host_id) {
        Some(f) if super::network_cloud_discover::is_host_managed_floating_ip(&f) => f,
        _ => {
            return Err((
                StatusCode::NOT_FOUND,
                Json(json!({ "error": "Host floating IP not found" })),
            ));
        }
    };

    let stored: Vec<FloatingIp> = state
        .store
        .list_entities("floating_ips")
        .unwrap_or_default();
    if stored
        .iter()
        .any(|f| f.address == host.address && f.interface == host.interface)
    {
        return Err((
            StatusCode::CONFLICT,
            Json(json!({
                "error": format!(
                    "Floating IP {} on {} is already managed by zyvor-fabricd",
                    host.address, host.interface
                )
            })),
        ));
    }

    let fip = FloatingIp {
        id: uuid::Uuid::new_v4().to_string(),
        address: host.address,
        interface: host.interface,
        assigned_vm: host.assigned_vm,
        managed: true,
        created: Utc::now(),
    };

    state
        .store
        .save_entity("floating_ips", &fip.id, &fip)
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": e.to_string() })),
            )
        })?;

    Ok(Json(fip))
}

// ============================================================================
// DHCP Server (via systemd-networkd [DHCPServer] section)
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DhcpServerConfig {
    pub id: String,
    pub bridge: String,
    pub pool_offset: u32,
    pub pool_size: u32,
    pub default_lease_time_sec: u32,
    pub max_lease_time_sec: u32,
    pub dns_servers: Vec<String>,
    pub gateway: Option<String>,
    pub domain: Option<String>,
    pub enabled: bool,
    pub created: DateTime<Utc>,
}

#[derive(Debug, Deserialize)]
pub struct CreateDhcpServerRequest {
    pub bridge: String,
    #[serde(default = "default_pool_offset")]
    pub pool_offset: u32,
    #[serde(default = "default_pool_size")]
    pub pool_size: u32,
    #[serde(default = "default_lease_time")]
    pub default_lease_time_sec: u32,
    #[serde(default = "default_max_lease_time")]
    pub max_lease_time_sec: u32,
    #[serde(default)]
    pub dns_servers: Vec<String>,
    pub gateway: Option<String>,
    pub domain: Option<String>,
}

fn default_pool_offset() -> u32 {
    100
}
fn default_pool_size() -> u32 {
    100
}
fn default_lease_time() -> u32 {
    3600
}
fn default_max_lease_time() -> u32 {
    7200
}

/// POST /api/dhcp-servers - Enable DHCP server on a bridge via dnsmasq (Admin only)
pub async fn create_dhcp_server(
    RequireAdmin(_claims): RequireAdmin,
    State(state): State<Arc<AppState>>,
    Json(req): Json<CreateDhcpServerRequest>,
) -> Result<(StatusCode, Json<DhcpServerConfig>), (StatusCode, Json<serde_json::Value>)> {
    tracing::debug!("network_cloud::{}", stringify!(create_dhcp_server));
    // Validate bridge name to prevent path traversal
    crate::validation::validate_hostname(&req.bridge).map_err(|msg| {
        (
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": format!("Invalid bridge name: {}", msg) })),
        )
    })?;

    // Validate DNS servers
    for dns in &req.dns_servers {
        crate::validation::validate_ip_address(dns).map_err(|msg| {
            (
                StatusCode::BAD_REQUEST,
                Json(json!({ "error": format!("Invalid DNS server: {}", msg) })),
            )
        })?;
    }

    // Validate gateway if provided
    if let Some(ref gw) = req.gateway {
        crate::validation::validate_ip_address(gw).map_err(|msg| {
            (
                StatusCode::BAD_REQUEST,
                Json(json!({ "error": format!("Invalid gateway: {}", msg) })),
            )
        })?;
    }

    let config = DhcpServerConfig {
        id: uuid::Uuid::new_v4().to_string(),
        bridge: req.bridge.clone(),
        pool_offset: req.pool_offset,
        pool_size: req.pool_size,
        default_lease_time_sec: req.default_lease_time_sec,
        max_lease_time_sec: req.max_lease_time_sec,
        dns_servers: req.dns_servers.clone(),
        gateway: req.gateway.clone(),
        domain: req.domain.clone(),
        enabled: true,
        created: Utc::now(),
    };

    // Start (or restart) a per-bridge dnsmasq DHCP server — replaces
    // systemd-networkd's [DHCPServer] .network-file directive.
    let gateway: std::net::Ipv4Addr = config
        .gateway
        .as_deref()
        .unwrap_or("0.0.0.0")
        .parse()
        .map_err(|_| {
            (
                StatusCode::BAD_REQUEST,
                Json(json!({ "error": "gateway is required and must be a valid IPv4 address for the DHCP server's own /24" })),
            )
        })?;
    // With no explicit dns_servers, default to advertising this bridge's
    // own gateway -- now that it also answers DNS (zone_hosts_dir below),
    // that's the one address that resolves both zone records and (via
    // dnsmasq's upstream forwarding) everything else, with no extra
    // per-VM configuration.
    let dns_servers = if config.dns_servers.is_empty() {
        vec![gateway.to_string()]
    } else {
        config.dns_servers.clone()
    };
    let dhcp_cfg = zyvor_fabric_dnsmasq_manager::DhcpConfig {
        bridge: config.bridge.clone(),
        gateway,
        pool_offset: config.pool_offset,
        pool_size: config.pool_size,
        default_lease_time_sec: config.default_lease_time_sec,
        dns_servers,
        domain: config.domain.clone(),
        // Every bridge with DHCP configured also gets DNS Zone/Policy
        // records served locally on the same dnsmasq instance (see
        // DhcpConfig::zone_hosts_dir's doc comment) -- VMs on this bridge
        // already get this instance pushed as their nameserver via the
        // dns-server DHCP option below, so this makes zone records
        // resolvable with no extra per-VM configuration.
        zone_hosts_dir: Some(::dns_policy::enforcement::DNS_DIR.into()),
    };
    state.dnsmasq_manager.start(&dhcp_cfg).await.map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": format!("Failed to start DHCP server: {:#}", e) })),
        )
    })?;

    state
        .store
        .save_entity("dhcp_servers", &config.id, &config)
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": e.to_string() })),
            )
        })?;

    Ok((StatusCode::CREATED, Json(config)))
}

/// PUT /api/dhcp-servers/{id} - Update an existing DHCP server config
/// (Admin only). Stops the old bridge's dnsmasq instance (in case `bridge`
/// itself changed) and starts a fresh one with the new config, same as
/// create -- dnsmasq_manager.start() is itself idempotent (tears down any
/// prior instance for the target bridge first), so this is safe even if
/// `bridge` is unchanged.
pub async fn update_dhcp_server(
    RequireAdmin(_claims): RequireAdmin,
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    Json(req): Json<CreateDhcpServerRequest>,
) -> Result<Json<DhcpServerConfig>, (StatusCode, Json<serde_json::Value>)> {
    tracing::debug!("network_cloud::{}", stringify!(update_dhcp_server));
    let existing = state
        .store
        .get_entity::<DhcpServerConfig>("dhcp_servers", &id)
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": e.to_string() })),
            )
        })?
        .ok_or_else(|| {
            (
                StatusCode::NOT_FOUND,
                Json(json!({ "error": "DHCP server config not found" })),
            )
        })?;

    crate::validation::validate_hostname(&req.bridge).map_err(|msg| {
        (
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": format!("Invalid bridge name: {}", msg) })),
        )
    })?;
    for dns in &req.dns_servers {
        crate::validation::validate_ip_address(dns).map_err(|msg| {
            (
                StatusCode::BAD_REQUEST,
                Json(json!({ "error": format!("Invalid DNS server: {}", msg) })),
            )
        })?;
    }
    if let Some(ref gw) = req.gateway {
        crate::validation::validate_ip_address(gw).map_err(|msg| {
            (
                StatusCode::BAD_REQUEST,
                Json(json!({ "error": format!("Invalid gateway: {}", msg) })),
            )
        })?;
    }

    let gateway: std::net::Ipv4Addr = req
        .gateway
        .as_deref()
        .unwrap_or("0.0.0.0")
        .parse()
        .map_err(|_| {
            (
                StatusCode::BAD_REQUEST,
                Json(json!({ "error": "gateway is required and must be a valid IPv4 address for the DHCP server's own /24" })),
            )
        })?;

    if existing.bridge != req.bridge {
        if let Err(e) = state.dnsmasq_manager.stop(&existing.bridge).await {
            tracing::warn!(
                "Failed to stop DHCP server for {}: {:#}",
                existing.bridge,
                e
            );
        }
    }

    let config = DhcpServerConfig {
        id: existing.id,
        bridge: req.bridge.clone(),
        pool_offset: req.pool_offset,
        pool_size: req.pool_size,
        default_lease_time_sec: req.default_lease_time_sec,
        max_lease_time_sec: req.max_lease_time_sec,
        dns_servers: req.dns_servers.clone(),
        gateway: req.gateway.clone(),
        domain: req.domain.clone(),
        enabled: true,
        created: existing.created,
    };

    let dns_servers = if config.dns_servers.is_empty() {
        vec![gateway.to_string()]
    } else {
        config.dns_servers.clone()
    };
    let dhcp_cfg = zyvor_fabric_dnsmasq_manager::DhcpConfig {
        bridge: config.bridge.clone(),
        gateway,
        pool_offset: config.pool_offset,
        pool_size: config.pool_size,
        default_lease_time_sec: config.default_lease_time_sec,
        dns_servers,
        domain: config.domain.clone(),
        zone_hosts_dir: Some(::dns_policy::enforcement::DNS_DIR.into()),
    };
    state.dnsmasq_manager.start(&dhcp_cfg).await.map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": format!("Failed to restart DHCP server: {:#}", e) })),
        )
    })?;

    state
        .store
        .save_entity("dhcp_servers", &config.id, &config)
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": e.to_string() })),
            )
        })?;

    Ok(Json(config))
}

/// GET /api/dhcp-servers - List DHCP server configs
pub async fn list_dhcp_servers(
    RequireRead(_claims): RequireRead,
    State(state): State<Arc<AppState>>,
) -> Json<Vec<DhcpServerConfig>> {
    tracing::debug!("network_cloud::{}", stringify!(list_dhcp_servers));
    let configs: Vec<DhcpServerConfig> =
        state
            .store
            .list_entities("dhcp_servers")
            .unwrap_or_else(|e| {
                tracing::error!("Storage error: {}", e);
                Vec::new()
            });
    Json(configs)
}

/// DELETE /api/dhcp-servers/:id - Remove DHCP server (Admin only)
pub async fn delete_dhcp_server(
    RequireAdmin(_claims): RequireAdmin,
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<StatusCode, (StatusCode, Json<serde_json::Value>)> {
    tracing::debug!("network_cloud::{}", stringify!(delete_dhcp_server));
    if let Ok(Some(config)) = state
        .store
        .get_entity::<DhcpServerConfig>("dhcp_servers", &id)
    {
        if let Err(e) = state.dnsmasq_manager.stop(&config.bridge).await {
            tracing::warn!("Failed to stop DHCP server for {}: {:#}", config.bridge, e);
        }
    }

    state
        .store
        .delete_entity("dhcp_servers", &id)
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": e.to_string() })),
            )
        })?;

    Ok(StatusCode::NO_CONTENT)
}

// ============================================================================
// DNS (via systemd-resolved integration)
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DnsConfig {
    pub id: String,
    pub domain: String,
    pub upstream_servers: Vec<String>,
    pub search_domains: Vec<String>,
    pub records: Vec<DnsRecord>,
    pub enabled: bool,
    pub created: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DnsRecord {
    pub name: String,
    pub record_type: String,
    pub value: String,
}

#[derive(Debug, Deserialize)]
pub struct CreateDnsConfigRequest {
    pub domain: String,
    #[serde(default)]
    pub upstream_servers: Vec<String>,
    #[serde(default)]
    pub search_domains: Vec<String>,
}

#[derive(Debug, Deserialize)]
pub struct AddDnsRecordRequest {
    pub name: String,
    pub record_type: String,
    pub value: String,
}

/// POST /api/dns - Create DNS configuration (Admin only)
pub async fn create_dns_config(
    RequireAdmin(_claims): RequireAdmin,
    State(state): State<Arc<AppState>>,
    Json(req): Json<CreateDnsConfigRequest>,
) -> Result<(StatusCode, Json<DnsConfig>), (StatusCode, Json<serde_json::Value>)> {
    tracing::debug!("network_cloud::{}", stringify!(create_dns_config));
    // Validate domain name
    crate::validation::validate_hostname(&req.domain).map_err(|msg| {
        (
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": format!("Invalid domain: {}", msg) })),
        )
    })?;
    // Validate upstream servers
    for server in &req.upstream_servers {
        crate::validation::validate_hostname(server).map_err(|msg| {
            (
                StatusCode::BAD_REQUEST,
                Json(json!({ "error": format!("Invalid upstream server: {}", msg) })),
            )
        })?;
    }
    // Validate search domains
    for domain in &req.search_domains {
        crate::validation::validate_hostname(domain).map_err(|msg| {
            (
                StatusCode::BAD_REQUEST,
                Json(json!({ "error": format!("Invalid search domain: {}", msg) })),
            )
        })?;
    }

    let config = DnsConfig {
        id: uuid::Uuid::new_v4().to_string(),
        domain: req.domain,
        upstream_servers: req.upstream_servers,
        search_domains: req.search_domains,
        records: Vec::new(),
        enabled: true,
        created: Utc::now(),
    };

    state
        .store
        .save_entity("dns_configs", &config.id, &config)
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": e.to_string() })),
            )
        })?;

    // Regenerate /etc/resolv.conf from every enabled DnsConfig — replaces
    // pushing this one config's search-domains/upstream-servers into
    // systemd-resolved via `resolvectl domain`/`resolvectl dns`.
    // `/etc/resolv.conf` is host-wide and single, so this aggregates across
    // all stored configs rather than letting each call clobber the last;
    // written atomically (temp file + rename), which also correctly
    // replaces a symlink there (e.g. one systemd-resolved left behind)
    // with a plain file, same as any other resolv.conf-managing tool does.
    let all_configs: Vec<DnsConfig> = state.store.list_entities("dns_configs").unwrap_or_default();
    if let Err(e) = write_resolv_conf(&all_configs, RESOLV_CONF_PATH).await {
        tracing::warn!("Failed to update {}: {:#}", RESOLV_CONF_PATH, e);
    }

    Ok((StatusCode::CREATED, Json(config)))
}

const RESOLV_CONF_PATH: &str = "/etc/resolv.conf";

/// Render and atomically write `/etc/resolv.conf` from every enabled
/// `DnsConfig`, deduplicating nameservers/search domains across configs in
/// first-seen order.
async fn write_resolv_conf(configs: &[DnsConfig], path: &str) -> std::io::Result<()> {
    let mut nameservers = Vec::new();
    let mut search_domains = Vec::new();
    for cfg in configs.iter().filter(|c| c.enabled) {
        for ns in &cfg.upstream_servers {
            if !nameservers.contains(ns) {
                nameservers.push(ns.clone());
            }
        }
        for sd in &cfg.search_domains {
            if !search_domains.contains(sd) {
                search_domains.push(sd.clone());
            }
        }
    }

    let mut content = String::from("# Managed by zyvor-fabricd — do not edit directly\n");
    if !search_domains.is_empty() {
        content.push_str(&format!("search {}\n", search_domains.join(" ")));
    }
    for ns in &nameservers {
        content.push_str(&format!("nameserver {ns}\n"));
    }

    let tmp_path = format!("{path}.zyvor-fabricd.tmp");
    tokio::fs::write(&tmp_path, &content).await?;
    tokio::fs::rename(&tmp_path, path).await
}

/// GET /api/dns - List DNS configurations
pub async fn list_dns_configs(
    RequireRead(_claims): RequireRead,
    State(state): State<Arc<AppState>>,
) -> Json<Vec<DnsConfig>> {
    tracing::debug!("network_cloud::{}", stringify!(list_dns_configs));
    let configs: Vec<DnsConfig> = state
        .store
        .list_entities("dns_configs")
        .unwrap_or_else(|e| {
            tracing::error!("Storage error: {}", e);
            Vec::new()
        });
    Json(configs)
}

/// POST /api/dns/:id/records - Add a DNS record
pub async fn add_dns_record(
    RequireWrite(_claims): RequireWrite,
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    Json(req): Json<AddDnsRecordRequest>,
) -> Result<Json<DnsConfig>, (StatusCode, Json<serde_json::Value>)> {
    tracing::debug!("network_cloud::{}", stringify!(add_dns_record));
    let mut config = match state.store.get_entity::<DnsConfig>("dns_configs", &id) {
        Ok(Some(c)) => c,
        Ok(None) => {
            return Err((
                StatusCode::NOT_FOUND,
                Json(json!({ "error": "DNS config not found" })),
            ))
        }
        Err(e) => {
            return Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": e.to_string() })),
            ))
        }
    };

    // Validate DNS record fields
    crate::validation::validate_hostname(&req.name).map_err(|msg| {
        (
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": format!("Invalid record name: {}", msg) })),
        )
    })?;
    crate::validation::validate_hostname(&req.value).map_err(|msg| {
        (
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": format!("Invalid record value: {}", msg) })),
        )
    })?;
    // Validate record type against allowlist
    let allowed_types = ["A", "AAAA", "CNAME", "MX", "TXT", "SRV", "NS", "PTR"];
    if !allowed_types.contains(&req.record_type.as_str()) {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(
                json!({ "error": format!("Invalid record type '{}'. Allowed: {}", req.record_type, allowed_types.join(", ")) }),
            ),
        ));
    }

    // Limit total records to prevent unbounded growth
    if config.records.len() >= 10000 {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(
                json!({ "error": "DNS configuration has reached the maximum number of records (10000)" }),
            ),
        ));
    }

    config.records.push(DnsRecord {
        name: req.name,
        record_type: req.record_type,
        value: req.value,
    });

    // Write /etc/hosts-style entries for A records
    update_hosts_file(&config).await;

    state
        .store
        .save_entity("dns_configs", &id, &config)
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": e.to_string() })),
            )
        })?;

    Ok(Json(config))
}

/// DELETE /api/dns/:id - Delete DNS configuration
pub async fn delete_dns_config(
    RequireAdmin(_claims): RequireAdmin,
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<StatusCode, (StatusCode, Json<serde_json::Value>)> {
    tracing::debug!("network_cloud::{}", stringify!(delete_dns_config));
    state.store.delete_entity("dns_configs", &id).map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": e.to_string() })),
        )
    })?;

    let remaining: Vec<DnsConfig> = state.store.list_entities("dns_configs").unwrap_or_default();
    if let Err(e) = write_resolv_conf(&remaining, RESOLV_CONF_PATH).await {
        tracing::warn!("Failed to update {}: {:#}", RESOLV_CONF_PATH, e);
    }

    Ok(StatusCode::NO_CONTENT)
}

async fn update_hosts_file(config: &DnsConfig) {
    let hosts_path = "/etc/zyvor-fabricd-hosts";

    let mut content = String::from("# Managed by zyvor-fabricd - do not edit\n");
    for record in &config.records {
        if record.record_type == "A" {
            let value = record.value.replace(['\n', '\r'], "");
            let fqdn = if record.name.ends_with('.') {
                record.name.trim_end_matches('.').to_string()
            } else {
                format!("{}.{}", record.name, config.domain)
            };
            let fqdn = fqdn.replace(['\n', '\r'], "");
            content.push_str(&format!("{} {}\n", value, fqdn));
        }
    }

    if let Err(e) = tokio::fs::write(hosts_path, content).await {
        tracing::warn!("Failed to write hosts file: {}", e);
    }
}

#[cfg(test)]
mod resolv_conf_tests {
    use super::*;

    fn dns_config(upstream: &[&str], search: &[&str], enabled: bool) -> DnsConfig {
        DnsConfig {
            id: uuid::Uuid::new_v4().to_string(),
            domain: "vms.local".into(),
            upstream_servers: upstream.iter().map(|s| s.to_string()).collect(),
            search_domains: search.iter().map(|s| s.to_string()).collect(),
            records: Vec::new(),
            enabled,
            created: Utc::now(),
        }
    }

    #[tokio::test]
    async fn test_write_resolv_conf_aggregates_and_dedupes() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("resolv.conf");
        let configs = vec![
            dns_config(&["8.8.8.8", "1.1.1.1"], &["a.local"], true),
            dns_config(&["1.1.1.1", "9.9.9.9"], &["b.local"], true),
        ];

        write_resolv_conf(&configs, path.to_str().unwrap())
            .await
            .unwrap();
        let content = tokio::fs::read_to_string(&path).await.unwrap();

        assert_eq!(content.matches("nameserver 8.8.8.8").count(), 1);
        assert_eq!(
            content.matches("nameserver 1.1.1.1").count(),
            1,
            "duplicate nameserver should be deduped"
        );
        assert!(content.contains("nameserver 9.9.9.9"));
        assert!(content.contains("search a.local b.local"));
    }

    #[tokio::test]
    async fn test_write_resolv_conf_skips_disabled_configs() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("resolv.conf");
        let configs = vec![dns_config(&["8.8.8.8"], &["a.local"], false)];

        write_resolv_conf(&configs, path.to_str().unwrap())
            .await
            .unwrap();
        let content = tokio::fs::read_to_string(&path).await.unwrap();

        assert!(!content.contains("nameserver"));
        assert!(!content.contains("search"));
    }

    #[tokio::test]
    async fn test_write_resolv_conf_replaces_existing_symlink() {
        // /etc/resolv.conf is very often a symlink (systemd-resolved leaves
        // one pointing at its stub resolver) — writing must replace it with
        // a plain file, not follow it and clobber whatever it points to.
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("stub-resolv.conf");
        tokio::fs::write(&target, "should not be touched")
            .await
            .unwrap();
        let link = dir.path().join("resolv.conf");
        #[cfg(unix)]
        std::os::unix::fs::symlink(&target, &link).unwrap();

        write_resolv_conf(
            &[dns_config(&["8.8.8.8"], &[], true)],
            link.to_str().unwrap(),
        )
        .await
        .unwrap();

        assert!(
            !link.is_symlink(),
            "resolv.conf should now be a plain file, not the old symlink"
        );
        let target_content = tokio::fs::read_to_string(&target).await.unwrap();
        assert_eq!(target_content, "should not be touched");
    }
}
