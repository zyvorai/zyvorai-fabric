// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: LicenseRef-Zyvor-Production-1.0

use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

use zyvor_fabric_storage::{NfsConfig, NfsVersion, StoragePool, StoragePoolType};

use crate::server::AppState;
use security::{RequireAdmin, RequireRead, RequireWrite};

// Request/Response types
#[derive(Debug, Deserialize)]
pub struct CreateLocalPoolRequest {
    pub name: String,
    pub path: String,
    pub auto_start: bool,
}

#[derive(Debug, Deserialize)]
pub struct CreateNfsPoolRequest {
    pub name: String,
    pub config: NfsConfigDto,
}

#[derive(Debug, Deserialize)]
pub struct CreateLvmPoolRequest {
    pub name: String,
    pub volume_group: String,
    #[serde(default)]
    pub auto_start: bool,
}

#[derive(Debug, Deserialize)]
pub struct CreateLvmThinPoolRequest {
    pub name: String,
    pub volume_group: String,
    pub thin_pool: String,
    #[serde(default)]
    pub auto_start: bool,
}

#[derive(Debug, Deserialize)]
pub struct CreateZfsPoolRequest {
    pub name: String,
    pub zpool: String,
    #[serde(default)]
    pub dataset: Option<String>,
    #[serde(default)]
    pub auto_start: bool,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct NfsConfigDto {
    pub server: String,
    pub export_path: String,
    pub mount_path: String,
    pub mount_options: Vec<String>,
    pub auto_start: bool,
    pub nfs_version: NfsVersionDto,
}

#[derive(Debug, Deserialize, Serialize)]
pub enum NfsVersionDto {
    V3,
    V4,
    #[serde(rename = "V4_1")]
    V4_1,
    #[serde(rename = "V4_2")]
    V4_2,
}

impl From<NfsVersionDto> for NfsVersion {
    fn from(dto: NfsVersionDto) -> Self {
        match dto {
            NfsVersionDto::V3 => NfsVersion::V3,
            NfsVersionDto::V4 => NfsVersion::V4,
            NfsVersionDto::V4_1 => NfsVersion::V4_1,
            NfsVersionDto::V4_2 => NfsVersion::V4_2,
        }
    }
}

impl From<NfsVersion> for NfsVersionDto {
    fn from(version: NfsVersion) -> Self {
        match version {
            NfsVersion::V3 => NfsVersionDto::V3,
            NfsVersion::V4 => NfsVersionDto::V4,
            NfsVersion::V4_1 => NfsVersionDto::V4_1,
            NfsVersion::V4_2 => NfsVersionDto::V4_2,
        }
    }
}

// API Handlers

/// GET /api/storage/pools - List all storage pools
pub async fn list_pools(
    RequireRead(_claims): RequireRead,
    State(state): State<Arc<AppState>>,
) -> Result<Json<Vec<StoragePool>>, (StatusCode, String)> {
    tracing::debug!("storage::{}", stringify!(list_pools));
    // Clone the pool list under the lock, then release it before returning
    let pools = {
        let manager = state.storage_manager.read().await;
        manager.list_pools().await
    };
    Ok(Json(pools))
}

/// GET /api/storage/pools/:name - Get storage pool details
pub async fn get_pool(
    RequireRead(_claims): RequireRead,
    State(state): State<Arc<AppState>>,
    Path(name): Path<String>,
) -> Result<Json<StoragePool>, (StatusCode, String)> {
    tracing::debug!("storage::{}", stringify!(get_pool));
    let result = {
        let manager = state.storage_manager.read().await;
        manager.get_pool(&name).await
    };

    match result {
        Ok(pool) => Ok(Json(pool)),
        Err(e) => Err((StatusCode::NOT_FOUND, format!("Pool not found: {}", e))),
    }
}

/// POST /api/storage/pools/local - Create local storage pool
pub async fn create_local_pool(
    RequireWrite(_claims): RequireWrite,
    State(state): State<Arc<AppState>>,
    Json(req): Json<CreateLocalPoolRequest>,
) -> Result<Json<StoragePool>, (StatusCode, String)> {
    tracing::debug!("storage::{}", stringify!(create_local_pool));

    // Validate pool name
    crate::validation::validate_entity_name(&req.name)
        .map_err(|(_, msg)| (StatusCode::BAD_REQUEST, msg))?;

    // Validate path is within allowed directories
    crate::validation::validate_host_path(&req.path)
        .map_err(|(_, msg)| (StatusCode::BAD_REQUEST, msg))?;

    let path = std::path::PathBuf::from(&req.path);
    let result = {
        let manager = state.storage_manager.read().await;
        manager
            .create_local_pool(req.name, path, req.auto_start)
            .await
    };

    match result {
        Ok(pool) => Ok(Json(pool)),
        Err(e) => Err((
            StatusCode::BAD_REQUEST,
            format!("Failed to create pool: {}", e),
        )),
    }
}

/// POST /api/storage/pools/nfs - Create NFS storage pool
pub async fn create_nfs_pool(
    RequireWrite(_claims): RequireWrite,
    State(state): State<Arc<AppState>>,
    Json(req): Json<CreateNfsPoolRequest>,
) -> Result<Json<StoragePool>, (StatusCode, String)> {
    tracing::debug!("storage::{}", stringify!(create_nfs_pool));

    // Validate pool name
    crate::validation::validate_entity_name(&req.name)
        .map_err(|(_, msg)| (StatusCode::BAD_REQUEST, msg))?;

    // Validate NFS server hostname
    crate::validation::validate_hostname(&req.config.server).map_err(|msg| {
        (
            StatusCode::BAD_REQUEST,
            format!("Invalid NFS server: {}", msg),
        )
    })?;

    // Validate mount path
    crate::validation::validate_host_path(&req.config.mount_path)
        .map_err(|(_, msg)| (StatusCode::BAD_REQUEST, msg))?;

    // Validate export path (must be absolute, no traversal)
    crate::validation::validate_machine_path(&req.config.export_path)
        .map_err(|(_, msg)| (StatusCode::BAD_REQUEST, msg))?;

    // Validate mount options against allowlist
    const ALLOWED_NFS_OPTIONS: &[&str] = &[
        "ro",
        "rw",
        "sync",
        "async",
        "noatime",
        "nodiratime",
        "soft",
        "hard",
        "intr",
        "nointr",
        "tcp",
        "udp",
        "nolock",
        "lock",
        "vers=3",
        "vers=4",
        "vers=4.1",
        "nfsvers=3",
        "nfsvers=4",
        "nfsvers=4.1",
        "actimeo=0",
    ];
    for opt in &req.config.mount_options {
        if !ALLOWED_NFS_OPTIONS.contains(&opt.as_str()) {
            return Err((
                StatusCode::BAD_REQUEST,
                format!("Invalid NFS mount option: '{}'", opt),
            ));
        }
    }

    let nfs_config = NfsConfig {
        server: req.config.server,
        export_path: req.config.export_path,
        mount_path: std::path::PathBuf::from(&req.config.mount_path),
        mount_options: req.config.mount_options,
        auto_start: req.config.auto_start,
        nfs_version: req.config.nfs_version.into(),
    };

    let result = {
        let manager = state.storage_manager.read().await;
        manager.create_nfs_pool(req.name, nfs_config).await
    };

    match result {
        Ok(pool) => Ok(Json(pool)),
        Err(e) => Err((
            StatusCode::BAD_REQUEST,
            format!("Failed to create NFS pool: {}", e),
        )),
    }
}

/// DELETE /api/storage/pools/:name - Delete storage pool
pub async fn delete_pool(
    RequireAdmin(_claims): RequireAdmin,
    State(state): State<Arc<AppState>>,
    Path(name): Path<String>,
) -> Result<StatusCode, (StatusCode, String)> {
    tracing::debug!("storage::{}", stringify!(delete_pool));
    let result = {
        let manager = state.storage_manager.read().await;
        manager.delete_pool(&name).await
    };

    match result {
        Ok(_) => Ok(StatusCode::NO_CONTENT),
        Err(e) => Err((
            StatusCode::BAD_REQUEST,
            format!("Failed to delete pool: {}", e),
        )),
    }
}

/// POST /api/storage/pools/:name/start - Start storage pool
pub async fn start_pool(
    RequireWrite(_claims): RequireWrite,
    State(state): State<Arc<AppState>>,
    Path(name): Path<String>,
) -> Result<StatusCode, (StatusCode, String)> {
    tracing::debug!("storage::{}", stringify!(start_pool));
    let result = {
        let manager = state.storage_manager.read().await;
        manager.start_pool(&name).await
    };
    match result {
        Ok(_) => Ok(StatusCode::OK),
        Err(e) => Err((
            StatusCode::BAD_REQUEST,
            format!("Failed to start pool: {}", e),
        )),
    }
}

/// POST /api/storage/pools/:name/stop - Stop storage pool
pub async fn stop_pool(
    RequireWrite(_claims): RequireWrite,
    State(state): State<Arc<AppState>>,
    Path(name): Path<String>,
) -> Result<StatusCode, (StatusCode, String)> {
    tracing::debug!("storage::{}", stringify!(stop_pool));
    let result = {
        let manager = state.storage_manager.read().await;
        manager.stop_pool(&name).await
    };
    match result {
        Ok(_) => Ok(StatusCode::OK),
        Err(e) => Err((
            StatusCode::BAD_REQUEST,
            format!("Failed to stop pool: {}", e),
        )),
    }
}

/// GET /api/storage/pools/:name/health - Get pool health.
///
/// NFS: a live mount/reachability check. Ceph: proxied from Atlas, since
/// zyvor-fabricd has no direct Ceph driver of its own -- calling
/// `get_nfs_health` for a Ceph pool would always 404 (it only ever looks in
/// the NFS pool map), which is what happened here before this branch existed.
pub async fn get_pool_health(
    RequireRead(_claims): RequireRead,
    State(state): State<Arc<AppState>>,
    Path(name): Path<String>,
) -> Result<impl IntoResponse, (StatusCode, String)> {
    tracing::debug!("storage::{}", stringify!(get_pool_health));

    let pool = {
        let manager = state.storage_manager.read().await;
        manager
            .get_pool(&name)
            .await
            .map_err(|e| (StatusCode::NOT_FOUND, format!("Pool not found: {}", e)))?
    };

    if let StoragePoolType::Ceph { pool_name, .. } = &pool.pool_type {
        let atlas_pool = fetch_atlas_pool(&state, pool_name).await?;
        return Ok(Json(serde_json::json!({
            "status": atlas_health_status(atlas_pool.health),
            "detail": format!("Atlas pool '{}': {:?}", atlas_pool.name, atlas_pool.health),
        }))
        .into_response());
    }

    let result = {
        let manager = state.storage_manager.read().await;
        manager.get_nfs_health(&name).await
    };
    match result {
        Ok(health) => Ok(Json(health).into_response()),
        Err(e) => Err((
            StatusCode::NOT_FOUND,
            format!("Failed to get health: {}", e),
        )),
    }
}

/// GET /api/storage/pools/:name/stats - Get pool stats.
///
/// NFS: live `df` of the mount. Ceph: proxied from Atlas's pool capacity
/// (used_bytes/max_bytes) -- same reasoning as get_pool_health. `objects` is
/// always 0 for Ceph pools: Atlas's typed pool API doesn't expose a
/// per-pool object count.
pub async fn get_pool_stats(
    RequireRead(_claims): RequireRead,
    State(state): State<Arc<AppState>>,
    Path(name): Path<String>,
) -> Result<impl IntoResponse, (StatusCode, String)> {
    tracing::debug!("storage::{}", stringify!(get_pool_stats));

    let pool = {
        let manager = state.storage_manager.read().await;
        manager
            .get_pool(&name)
            .await
            .map_err(|e| (StatusCode::NOT_FOUND, format!("Pool not found: {}", e)))?
    };

    if let StoragePoolType::Ceph { pool_name, .. } = &pool.pool_type {
        let atlas_pool = fetch_atlas_pool(&state, pool_name).await?;
        let total_bytes = atlas_pool.max_bytes.unwrap_or(0);
        let used_bytes = atlas_pool.used_bytes.unwrap_or(0);
        return Ok(Json(serde_json::json!({
            "total_bytes": total_bytes,
            "used_bytes": used_bytes,
            "available_bytes": (total_bytes - used_bytes).max(0),
            "objects": 0,
        }))
        .into_response());
    }

    let result = {
        let manager = state.storage_manager.read().await;
        manager.get_nfs_stats(&name).await
    };
    match result {
        Ok(stats) => Ok(Json(stats).into_response()),
        Err(e) => Err((StatusCode::NOT_FOUND, format!("Failed to get stats: {}", e))),
    }
}

#[derive(Debug, Deserialize, Clone, Copy, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
enum AtlasHealth {
    Ok,
    Warn,
    Critical,
    #[default]
    Unknown,
}

fn atlas_health_status(h: AtlasHealth) -> &'static str {
    match h {
        AtlasHealth::Ok => "Ok",
        AtlasHealth::Warn | AtlasHealth::Unknown => "Warn",
        AtlasHealth::Critical => "Error",
    }
}

#[derive(Debug, Deserialize)]
struct AtlasPoolInfo {
    name: String,
    used_bytes: Option<i64>,
    max_bytes: Option<i64>,
    #[serde(default)]
    health: AtlasHealth,
}

/// Fetch a single Ceph pool's normalized info from Atlas by its raw Ceph pool name.
async fn fetch_atlas_pool(
    state: &Arc<AppState>,
    ceph_pool_name: &str,
) -> Result<AtlasPoolInfo, (StatusCode, String)> {
    let atlas_base_url = state.config.storage.atlas_base_url.clone().ok_or_else(|| {
        (
            StatusCode::SERVICE_UNAVAILABLE,
            "Atlas storage control plane is not configured (storage.atlas_base_url)".to_string(),
        )
    })?;
    let url = atlas_url(&atlas_base_url, &["pools"])?;
    let resp = state.http_client.get(url).send().await.map_err(|e| {
        (
            StatusCode::BAD_GATEWAY,
            format!("Atlas request failed: {}", e),
        )
    })?;
    let status = resp.status();
    if !status.is_success() {
        let body = resp.text().await.unwrap_or_default();
        return Err((
            StatusCode::BAD_GATEWAY,
            format!("Atlas returned {}: {}", status, body),
        ));
    }
    let pools: Vec<AtlasPoolInfo> = resp.json().await.map_err(|e| {
        (
            StatusCode::BAD_GATEWAY,
            format!("Failed to parse Atlas response: {}", e),
        )
    })?;
    pools
        .into_iter()
        .find(|p| p.name == ceph_pool_name)
        .ok_or_else(|| {
            (
                StatusCode::NOT_FOUND,
                format!("Atlas has no pool named '{}'", ceph_pool_name),
            )
        })
}

/// POST /api/storage/pools/:name/refresh - Refresh pool statistics
pub async fn refresh_pool_stats(
    RequireWrite(_claims): RequireWrite,
    State(state): State<Arc<AppState>>,
    Path(name): Path<String>,
) -> Result<StatusCode, (StatusCode, String)> {
    tracing::debug!("storage::{}", stringify!(refresh_pool_stats));
    let result = {
        let manager = state.storage_manager.read().await;
        manager.refresh_pool_stats(&name).await
    };
    match result {
        Ok(_) => Ok(StatusCode::OK),
        Err(e) => Err((
            StatusCode::BAD_REQUEST,
            format!("Failed to refresh stats: {}", e),
        )),
    }
}

/// POST /api/storage/pools/lvm - Create LVM storage pool
pub async fn create_lvm_pool(
    RequireWrite(_claims): RequireWrite,
    State(state): State<Arc<AppState>>,
    Json(req): Json<CreateLvmPoolRequest>,
) -> Result<Json<StoragePool>, (StatusCode, String)> {
    tracing::debug!("storage::{}", stringify!(create_lvm_pool));
    crate::validation::validate_entity_name(&req.name)
        .map_err(|(_, msg)| (StatusCode::BAD_REQUEST, msg))?;
    crate::validation::validate_hostname(&req.volume_group).map_err(|m| {
        (
            StatusCode::BAD_REQUEST,
            format!("Invalid volume group name: {}", m),
        )
    })?;
    let result = {
        let manager = state.storage_manager.read().await;
        manager
            .create_lvm_pool(req.name, req.volume_group, req.auto_start)
            .await
    };
    match result {
        Ok(pool) => Ok(Json(pool)),
        Err(e) => Err((
            StatusCode::BAD_REQUEST,
            format!("Failed to create LVM pool: {}", e),
        )),
    }
}

/// POST /api/storage/pools/lvm-thin - Create LVM thin storage pool
pub async fn create_lvm_thin_pool(
    RequireWrite(_claims): RequireWrite,
    State(state): State<Arc<AppState>>,
    Json(req): Json<CreateLvmThinPoolRequest>,
) -> Result<Json<StoragePool>, (StatusCode, String)> {
    tracing::debug!("storage::{}", stringify!(create_lvm_thin_pool));
    crate::validation::validate_entity_name(&req.name)
        .map_err(|(_, msg)| (StatusCode::BAD_REQUEST, msg))?;
    crate::validation::validate_hostname(&req.volume_group).map_err(|m| {
        (
            StatusCode::BAD_REQUEST,
            format!("Invalid volume group name: {}", m),
        )
    })?;
    crate::validation::validate_hostname(&req.thin_pool).map_err(|m| {
        (
            StatusCode::BAD_REQUEST,
            format!("Invalid thin pool name: {}", m),
        )
    })?;
    let result = {
        let manager = state.storage_manager.read().await;
        manager
            .create_lvm_thin_pool(req.name, req.volume_group, req.thin_pool, req.auto_start)
            .await
    };
    match result {
        Ok(pool) => Ok(Json(pool)),
        Err(e) => Err((
            StatusCode::BAD_REQUEST,
            format!("Failed to create LVM thin pool: {}", e),
        )),
    }
}

/// POST /api/storage/pools/zfs - Create ZFS storage pool
pub async fn create_zfs_pool(
    RequireWrite(_claims): RequireWrite,
    State(state): State<Arc<AppState>>,
    Json(req): Json<CreateZfsPoolRequest>,
) -> Result<Json<StoragePool>, (StatusCode, String)> {
    tracing::debug!("storage::{}", stringify!(create_zfs_pool));
    crate::validation::validate_entity_name(&req.name)
        .map_err(|(_, msg)| (StatusCode::BAD_REQUEST, msg))?;
    crate::validation::validate_hostname(&req.zpool).map_err(|m| {
        (
            StatusCode::BAD_REQUEST,
            format!("Invalid zpool name: {}", m),
        )
    })?;
    let result = {
        let manager = state.storage_manager.read().await;
        manager
            .create_zfs_pool(req.name, req.zpool, req.dataset, req.auto_start)
            .await
    };
    match result {
        Ok(pool) => Ok(Json(pool)),
        Err(e) => Err((
            StatusCode::BAD_REQUEST,
            format!("Failed to create ZFS pool: {}", e),
        )),
    }
}

#[derive(Debug, Deserialize)]
pub struct CreateCephPoolRequest {
    pub name: String,
    pub monitors: Vec<String>,
    pub pool_name: String,
    pub user: Option<String>,
    pub keyring: Option<String>,
    #[serde(default)]
    pub auto_start: bool,
}

pub async fn create_ceph_pool(
    RequireWrite(_claims): RequireWrite,
    State(state): State<Arc<AppState>>,
    Json(req): Json<CreateCephPoolRequest>,
) -> Result<Json<StoragePool>, (StatusCode, String)> {
    tracing::debug!("storage::{}", stringify!(create_ceph_pool));
    crate::validation::validate_entity_name(&req.name)
        .map_err(|(_, msg)| (StatusCode::BAD_REQUEST, msg))?;
    for monitor in &req.monitors {
        crate::validation::validate_hostname(monitor).map_err(|m| {
            (
                StatusCode::BAD_REQUEST,
                format!("Invalid Ceph monitor: {}", m),
            )
        })?;
    }
    let result = {
        let manager = state.storage_manager.read().await;
        manager
            .create_ceph_pool(
                req.name,
                req.monitors,
                req.pool_name,
                req.user,
                req.keyring,
                req.auto_start,
            )
            .await
    };
    match result {
        Ok(pool) => Ok(Json(pool)),
        Err(e) => Err((
            StatusCode::BAD_REQUEST,
            format!("Failed to create Ceph pool: {}", e),
        )),
    }
}

// ============================================================================
// RBD images (Ceph pools only, proxied through the Atlas storage control
// plane -- zyvor-fabricd has no direct Ceph/RBD driver of its own).
// ============================================================================

fn atlas_url(base: &str, segments: &[&str]) -> Result<reqwest::Url, (StatusCode, String)> {
    let mut url = reqwest::Url::parse(base).map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Invalid storage.atlas_base_url: {}", e),
        )
    })?;
    {
        let mut push = url.path_segments_mut().map_err(|_| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                "storage.atlas_base_url cannot be a base URL".to_string(),
            )
        })?;
        for seg in segments {
            push.push(seg);
        }
    }
    Ok(url)
}

/// Resolve a zyvor-fabric storage pool name to its Ceph pool_name, and
/// confirm Atlas is configured to talk to on its behalf.
async fn resolve_ceph_pool(
    state: &Arc<AppState>,
    name: &str,
) -> Result<(String, String), (StatusCode, String)> {
    let atlas_base_url = state.config.storage.atlas_base_url.clone().ok_or_else(|| {
        (
            StatusCode::SERVICE_UNAVAILABLE,
            "Atlas storage control plane is not configured (storage.atlas_base_url) -- RBD image operations are unavailable".to_string(),
        )
    })?;

    let pool = {
        let manager = state.storage_manager.read().await;
        manager
            .get_pool(name)
            .await
            .map_err(|e| (StatusCode::NOT_FOUND, format!("Pool not found: {}", e)))?
    };

    match pool.pool_type {
        StoragePoolType::Ceph { pool_name, .. } => Ok((atlas_base_url, pool_name)),
        _ => Err((
            StatusCode::BAD_REQUEST,
            format!("Pool '{}' is not a Ceph pool", name),
        )),
    }
}

#[derive(Debug, Deserialize)]
struct AtlasRbdListResponse {
    images: Vec<String>,
}

/// GET /api/storage/pools/:name/images - List RBD images in a Ceph pool
pub async fn list_rbd_images(
    RequireRead(_claims): RequireRead,
    State(state): State<Arc<AppState>>,
    Path(name): Path<String>,
) -> Result<Json<Vec<String>>, (StatusCode, String)> {
    tracing::debug!("storage::{}", stringify!(list_rbd_images));
    let (atlas_base_url, pool_name) = resolve_ceph_pool(&state, &name).await?;

    let mut url = atlas_url(&atlas_base_url, &["rbd-images"])?;
    url.query_pairs_mut().append_pair("pool", &pool_name);

    let resp = state.http_client.get(url).send().await.map_err(|e| {
        (
            StatusCode::BAD_GATEWAY,
            format!("Atlas request failed: {}", e),
        )
    })?;

    let status = resp.status();
    if !status.is_success() {
        let body = resp.text().await.unwrap_or_default();
        return Err((
            StatusCode::BAD_GATEWAY,
            format!("Atlas returned {}: {}", status, body),
        ));
    }

    let parsed: AtlasRbdListResponse = resp.json().await.map_err(|e| {
        (
            StatusCode::BAD_GATEWAY,
            format!("Failed to parse Atlas response: {}", e),
        )
    })?;

    Ok(Json(parsed.images))
}

#[derive(Debug, Deserialize)]
pub struct CreateRbdImageRequest {
    pub name: String,
    pub size_mb: u64,
}

/// POST /api/storage/pools/:name/images - Create an RBD image in a Ceph pool.
///
/// Atlas provisions asynchronously via a job queue -- a successful response
/// here means Atlas *accepted* the create request, not that the image
/// exists yet. There's no synchronous "wait for it to land" equivalent.
pub async fn create_rbd_image(
    RequireWrite(_claims): RequireWrite,
    State(state): State<Arc<AppState>>,
    Path(name): Path<String>,
    Json(req): Json<CreateRbdImageRequest>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    tracing::debug!("storage::{}", stringify!(create_rbd_image));
    if req.name.trim().is_empty() {
        return Err((
            StatusCode::BAD_REQUEST,
            "Image name is required".to_string(),
        ));
    }
    if req.size_mb == 0 {
        return Err((StatusCode::BAD_REQUEST, "size_mb must be > 0".to_string()));
    }
    let (atlas_base_url, pool_name) = resolve_ceph_pool(&state, &name).await?;
    let url = atlas_url(&atlas_base_url, &["rbd-images"])?;

    let resp = state
        .http_client
        .post(url)
        .json(&serde_json::json!({
            "name": req.name,
            "size_bytes": (req.size_mb as i64).saturating_mul(1024 * 1024),
            "pool": pool_name,
        }))
        .send()
        .await
        .map_err(|e| {
            (
                StatusCode::BAD_GATEWAY,
                format!("Atlas request failed: {}", e),
            )
        })?;

    let status = resp.status();
    let body: serde_json::Value = resp.json().await.unwrap_or(serde_json::Value::Null);
    if !status.is_success() {
        return Err((
            StatusCode::BAD_GATEWAY,
            format!("Atlas rejected RBD image create: {} {}", status, body),
        ));
    }

    Ok(Json(body))
}

/// DELETE /api/storage/pools/:name/images/:image - Delete an RBD image from a Ceph pool
pub async fn delete_rbd_image(
    RequireWrite(_claims): RequireWrite,
    State(state): State<Arc<AppState>>,
    Path((name, image)): Path<(String, String)>,
) -> Result<StatusCode, (StatusCode, String)> {
    tracing::debug!("storage::{}", stringify!(delete_rbd_image));
    let (atlas_base_url, pool_name) = resolve_ceph_pool(&state, &name).await?;
    let url = atlas_url(&atlas_base_url, &["rbd-images", &pool_name, &image])?;

    let resp = state.http_client.delete(url).send().await.map_err(|e| {
        (
            StatusCode::BAD_GATEWAY,
            format!("Atlas request failed: {}", e),
        )
    })?;

    let status = resp.status();
    if !status.is_success() {
        let body = resp.text().await.unwrap_or_default();
        return Err((
            StatusCode::BAD_GATEWAY,
            format!("Atlas returned {}: {}", status, body),
        ));
    }
    Ok(StatusCode::NO_CONTENT)
}

// ============================================================================
// iSCSI endpoints
// ============================================================================

#[derive(Debug, Deserialize)]
pub struct IscsiDiscoverRequest {
    pub portal: String,
}

#[derive(Debug, Deserialize)]
pub struct IscsiLoginRequest {
    pub portal: String,
    pub target_iqn: String,
}

#[derive(Debug, Deserialize)]
pub struct IscsiLogoutRequest {
    pub portal: String,
    pub target_iqn: String,
}

/// POST /api/storage/iscsi/discover - Discover iSCSI targets on a portal
pub async fn discover_iscsi_targets(
    RequireAdmin(_claims): RequireAdmin,
    Json(req): Json<IscsiDiscoverRequest>,
) -> Result<Json<Vec<String>>, (StatusCode, Json<serde_json::Value>)> {
    tracing::debug!("storage::{}", stringify!(discover_iscsi_targets));
    crate::validation::validate_hostname(&req.portal)
        .map_err(|e| crate::api_error::json_error(StatusCode::BAD_REQUEST, e))?;
    zyvor_fabric_storage::iscsi::discover_targets(&req.portal)
        .map(Json)
        .map_err(|e| {
            crate::api_error::json_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Discovery failed: {}", e),
            )
        })
}

/// POST /api/storage/iscsi/login - Login to an iSCSI target
pub async fn login_iscsi_target(
    RequireAdmin(_claims): RequireAdmin,
    Json(req): Json<IscsiLoginRequest>,
) -> Result<StatusCode, (StatusCode, Json<serde_json::Value>)> {
    tracing::debug!("storage::{}", stringify!(login_iscsi_target));
    crate::validation::validate_hostname(&req.portal)
        .map_err(|e| crate::api_error::json_error(StatusCode::BAD_REQUEST, e))?;
    zyvor_fabric_storage::iscsi::login_target(&req.portal, &req.target_iqn)
        .map(|_| StatusCode::OK)
        .map_err(|e| crate::api_error::json_error(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))
}

/// POST /api/storage/iscsi/logout - Logout from an iSCSI target
pub async fn logout_iscsi_target(
    RequireAdmin(_claims): RequireAdmin,
    Json(req): Json<IscsiLogoutRequest>,
) -> Result<StatusCode, (StatusCode, Json<serde_json::Value>)> {
    tracing::debug!("storage::{}", stringify!(logout_iscsi_target));
    crate::validation::validate_hostname(&req.portal)
        .map_err(|e| crate::api_error::json_error(StatusCode::BAD_REQUEST, e))?;
    zyvor_fabric_storage::iscsi::logout_target(&req.portal, &req.target_iqn)
        .map(|_| StatusCode::OK)
        .map_err(|e| crate::api_error::json_error(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))
}

/// GET /api/storage/iscsi/sessions - List active iSCSI sessions
pub async fn list_iscsi_sessions(
    RequireRead(_claims): RequireRead,
) -> Result<
    Json<Vec<zyvor_fabric_storage::iscsi::IscsiTarget>>,
    (StatusCode, Json<serde_json::Value>),
> {
    tracing::debug!("storage::{}", stringify!(list_iscsi_sessions));
    zyvor_fabric_storage::iscsi::list_sessions()
        .map(Json)
        .map_err(|e| crate::api_error::json_error(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_nfs_version_conversion() {
        let v4 = NfsVersionDto::V4;
        let nfs_version: NfsVersion = v4.into();
        assert!(matches!(nfs_version, NfsVersion::V4));
    }
}
