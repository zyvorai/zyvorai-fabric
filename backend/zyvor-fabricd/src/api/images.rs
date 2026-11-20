// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: LicenseRef-Zyvor-Production-1.0

use axum::{extract::State, http::StatusCode, Json};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::sync::Arc;

use crate::server::AppState;
use security::{RequireAdmin, RequireRead, RequireWrite};

fn validate_image_format(format: &str) -> Result<(), (StatusCode, Json<serde_json::Value>)> {
    crate::validation::validate_image_format(format).map_err(|(s, v)| (s, Json(v)))
}

/// Shared job-state enum -- also used by cloud-image downloads below, not
/// just image builds (mkosi builds themselves have been removed).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum BuildState {
    Pending,
    Building,
    Completed,
    Failed,
}

/// GET /api/images/list - List available VM images
pub async fn list_images(
    RequireRead(claims): RequireRead,
    State(state): State<Arc<AppState>>,
) -> Json<Vec<ImageInfo>> {
    tracing::debug!("images::{}", stringify!(list_images));
    let show_path = claims.role.can_manage();
    let mut images = Vec::new();

    // Scan the configured image directory plus FluxVM's own image store
    // (VM lifecycle -- and image storage -- is handled entirely by
    // FluxVM; /var/lib/machines was systemd-machined's directory and no
    // longer exists post-migration).
    for dir in &[
        state.config.storage.image_path.as_str(),
        "/var/lib/fluxvm/images",
    ] {
        let mut entries = match tokio::fs::read_dir(dir).await {
            Ok(e) => e,
            Err(_) => continue,
        };
        while let Ok(Some(entry)) = entries.next_entry().await {
            let path = entry.path();
            let name = entry.file_name().to_string_lossy().to_string();

            if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
                if matches!(ext, "raw" | "qcow2" | "img") {
                    let metadata = entry.metadata().await.ok();
                    let size = metadata.as_ref().map(|m| m.len()).unwrap_or(0);
                    let mod_time = metadata
                        .and_then(|m| m.modified().ok())
                        .map(DateTime::<Utc>::from)
                        .unwrap_or_else(Utc::now);
                    images.push(ImageInfo {
                        name: name.trim_end_matches(&format!(".{}", ext)).to_string(),
                        path: if show_path {
                            path.display().to_string()
                        } else {
                            String::new()
                        },
                        format: ext.to_string(),
                        size_bytes: size,
                        mod_time,
                    });
                }
            }
        }
    }

    Json(images)
}

#[derive(Debug, Serialize)]
pub struct ImageInfo {
    pub name: String,
    pub path: String,
    pub format: String,
    pub size_bytes: u64,
    pub mod_time: DateTime<Utc>,
}

// ============================================================================
// Cloud Image Download
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CloudImage {
    pub name: String,
    pub distro: String,
    pub version: String,
    pub url: String,
    pub format: String,
    pub arch: String,
}

/// Well-known cloud image URLs
fn cloud_image_catalog() -> Vec<CloudImage> {
    vec![
        CloudImage {
            name: "ubuntu-24.04".into(),
            distro: "ubuntu".into(),
            version: "24.04".into(),
            url: "https://cloud-images.ubuntu.com/noble/current/noble-server-cloudimg-amd64.img".into(),
            format: "qcow2".into(),
            arch: "amd64".into(),
        },
        CloudImage {
            name: "ubuntu-22.04".into(),
            distro: "ubuntu".into(),
            version: "22.04".into(),
            url: "https://cloud-images.ubuntu.com/jammy/current/jammy-server-cloudimg-amd64.img".into(),
            format: "qcow2".into(),
            arch: "amd64".into(),
        },
        CloudImage {
            name: "fedora-41".into(),
            distro: "fedora".into(),
            version: "41".into(),
            url: "https://download.fedoraproject.org/pub/fedora/linux/releases/41/Cloud/x86_64/images/Fedora-Cloud-Base-Generic-41-1.4.x86_64.qcow2".into(),
            format: "qcow2".into(),
            arch: "x86_64".into(),
        },
        CloudImage {
            name: "debian-12".into(),
            distro: "debian".into(),
            version: "12".into(),
            url: "https://cloud.debian.org/images/cloud/bookworm/latest/debian-12-generic-amd64.qcow2".into(),
            format: "qcow2".into(),
            arch: "amd64".into(),
        },
        CloudImage {
            name: "alma-9".into(),
            distro: "almalinux".into(),
            version: "9".into(),
            url: "https://repo.almalinux.org/almalinux/9/cloud/x86_64/images/AlmaLinux-9-GenericCloud-latest.x86_64.qcow2".into(),
            format: "qcow2".into(),
            arch: "x86_64".into(),
        },
        CloudImage {
            // Flatcar doesn't publish a dedicated "qemu" qcow2 anymore -- the
            // kubevirt image is the maintained qcow2 build and is generic
            // QEMU/KVM + cloud-init compatible (KubeVirt runs guests via
            // QEMU/libvirt under the hood), so it's the correct pick here.
            name: "flatcar-stable".into(),
            distro: "flatcar".into(),
            version: "stable".into(),
            url: "https://stable.release.flatcar-linux.net/amd64-usr/current/flatcar_production_kubevirt_image.qcow2".into(),
            format: "qcow2".into(),
            arch: "amd64".into(),
        },
    ]
}

/// GET /api/images/cloud - List available cloud images for download
pub async fn list_cloud_images(RequireRead(_claims): RequireRead) -> Json<Vec<CloudImage>> {
    tracing::debug!("images::{}", stringify!(list_cloud_images));
    Json(cloud_image_catalog())
}

#[derive(Debug, Deserialize)]
pub struct DownloadCloudImageRequest {
    pub name: String,
    /// Optional: override the URL from the catalog
    pub url: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DownloadStatus {
    pub id: String,
    pub name: String,
    pub state: BuildState,
    pub output_path: Option<String>,
    pub error: Option<String>,
    pub started: DateTime<Utc>,
    pub completed: Option<DateTime<Utc>>,
}

/// Maximum download size (100 GB).
const MAX_DOWNLOAD_BYTES: u64 = 100 * 1024 * 1024 * 1024;

/// Helper to stream an HTTP response body to a file on disk.
async fn stream_response_to_file(
    response: reqwest::Response,
    dest_path: &str,
) -> Result<u64, String> {
    use tokio::io::AsyncWriteExt;

    let mut file = tokio::fs::File::create(dest_path)
        .await
        .map_err(|e| format!("Failed to create file: {}", e))?;

    let mut written: u64 = 0;
    let mut stream = response.bytes_stream();

    use futures::StreamExt;
    while let Some(chunk_result) = stream.next().await {
        let chunk = chunk_result.map_err(|e| format!("Download stream error: {}", e))?;
        file.write_all(&chunk)
            .await
            .map_err(|e| format!("Failed to write chunk: {}", e))?;
        written += chunk.len() as u64;
        if written > MAX_DOWNLOAD_BYTES {
            drop(file);
            let _ = tokio::fs::remove_file(dest_path).await;
            return Err(format!(
                "Download exceeds maximum size of {} bytes",
                MAX_DOWNLOAD_BYTES
            ));
        }
    }

    file.flush()
        .await
        .map_err(|e| format!("Failed to flush file: {}", e))?;
    Ok(written)
}

/// Helper to mark a download as failed in the store.
async fn mark_download_failed(
    store: &state_store::StateStore,
    collection: &str,
    dl_id: &str,
    error: String,
) {
    if let Ok(Some(mut s)) = store.get_entity::<DownloadStatus>(collection, dl_id) {
        s.state = BuildState::Failed;
        s.error = Some(error);
        s.completed = Some(Utc::now());
        if let Err(e) = store.save_entity(collection, dl_id, &s) {
            tracing::error!("Failed to save download state: {}", e);
        }
    }
}

/// POST /api/images/cloud/download - Download a cloud image
pub async fn download_cloud_image(
    RequireWrite(_claims): RequireWrite,
    State(state): State<Arc<AppState>>,
    Json(req): Json<DownloadCloudImageRequest>,
) -> Result<(StatusCode, Json<DownloadStatus>), (StatusCode, Json<serde_json::Value>)> {
    tracing::debug!("images::{}", stringify!(download_cloud_image));

    crate::validation::validate_vm_name(&req.name)
        .map_err(|(s, m)| crate::api_error::json_error(s, m))?;

    let catalog = cloud_image_catalog();
    let url = if let Some(ref custom_url) = req.url {
        // Validate user-provided URL against SSRF
        crate::api::notifications::validate_external_url_public(custom_url)
            .map_err(|e| crate::api_error::json_error(StatusCode::BAD_REQUEST, e))?;
        custom_url.clone()
    } else {
        catalog
            .iter()
            .find(|img| img.name == req.name)
            .map(|img| img.url.clone())
            .ok_or_else(|| {
                crate::api_error::json_error(
                    StatusCode::NOT_FOUND,
                    format!("Cloud image '{}' not found in catalog", req.name),
                )
            })?
    };

    let download_id = uuid::Uuid::new_v4().to_string();
    let status = DownloadStatus {
        id: download_id.clone(),
        name: req.name.clone(),
        state: BuildState::Pending,
        output_path: None,
        error: None,
        started: Utc::now(),
        completed: None,
    };

    state
        .store
        .save_entity("image_downloads", &download_id, &status)
        .map_err(|e| {
            crate::api_error::json_error(StatusCode::INTERNAL_SERVER_ERROR, e.to_string())
        })?;

    // Spawn background download
    let state_clone = state.clone();
    let dl_id = download_id.clone();
    let image_name = req.name.clone();

    let dl_id_log = download_id.clone();
    let handle = tokio::spawn(async move {
        // Update state
        if let Ok(Some(mut s)) = state_clone
            .store
            .get_entity::<DownloadStatus>("image_downloads", &dl_id)
        {
            s.state = BuildState::Building;
            if let Err(e) = state_clone.store.save_entity("image_downloads", &dl_id, &s) {
                tracing::error!("Failed to save download state: {}", e);
            }
        }

        let dest_dir = "/var/lib/zyvor-fabricd/images";
        if let Err(e) = tokio::fs::create_dir_all(dest_dir).await {
            tracing::error!("Failed to create dir: {}", e);
            return;
        }

        // Determine extension from URL (strip query params, validate against allowlist)
        let raw_ext = url
            .rsplit('.')
            .next()
            .and_then(|e| e.split('?').next())
            .unwrap_or("qcow2");
        let ext = if crate::validation::ALLOWED_IMAGE_FORMATS.contains(&raw_ext) {
            raw_ext
        } else {
            "qcow2"
        };
        let dest_path = format!("{}/{}.{}", dest_dir, image_name, ext);

        // Download using streaming to avoid loading entire image into memory
        match state_clone.http_client.get(&url).send().await {
            Ok(response) => {
                if !response.status().is_success() {
                    let status_code = response.status();
                    mark_download_failed(
                        &state_clone.store,
                        "image_downloads",
                        &dl_id,
                        format!("HTTP {}", status_code),
                    )
                    .await;
                    return;
                }

                match stream_response_to_file(response, &dest_path).await {
                    Ok(_) => {
                        tracing::info!("Downloaded cloud image '{}' to {}", image_name, dest_path);
                        if let Ok(Some(mut s)) = state_clone
                            .store
                            .get_entity::<DownloadStatus>("image_downloads", &dl_id)
                        {
                            s.state = BuildState::Completed;
                            s.output_path = Some(dest_path);
                            s.completed = Some(Utc::now());
                            if let Err(e) =
                                state_clone.store.save_entity("image_downloads", &dl_id, &s)
                            {
                                tracing::error!("Failed to save download state: {}", e);
                            }
                        }
                    }
                    Err(e) => {
                        tracing::error!("Failed to download image: {}", e);
                        // Clean up partial file
                        let _ = tokio::fs::remove_file(&dest_path).await;
                        mark_download_failed(&state_clone.store, "image_downloads", &dl_id, e)
                            .await;
                    }
                }
            }
            Err(e) => {
                tracing::error!("Failed to start download: {}", e);
                mark_download_failed(&state_clone.store, "image_downloads", &dl_id, e.to_string())
                    .await;
            }
        }
    });
    tokio::spawn(async move {
        if let Err(e) = handle.await {
            tracing::error!(
                "Background cloud image download task '{}' panicked: {}",
                dl_id_log,
                e
            );
        }
    });

    Ok((StatusCode::ACCEPTED, Json(status)))
}

/// GET /api/images/downloads - List download status
pub async fn list_downloads(
    RequireRead(_claims): RequireRead,
    State(state): State<Arc<AppState>>,
) -> Result<Json<Vec<DownloadStatus>>, (StatusCode, Json<serde_json::Value>)> {
    tracing::debug!("images::{}", stringify!(list_downloads));
    let downloads = state
        .store
        .list_entities::<DownloadStatus>("image_downloads")
        .map_err(|e| {
            crate::api_error::json_error(StatusCode::INTERNAL_SERVER_ERROR, e.to_string())
        })?;
    Ok(Json(downloads))
}

// ============================================================================
// ISO Management
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IsoImage {
    pub id: String,
    pub name: String,
    pub path: String,
    pub size_bytes: u64,
    pub uploaded: DateTime<Utc>,
}

/// Derive a deterministic UUID from a file path so IDs are stable across calls.
/// Uses UUID v5 (SHA-1 based) which is deterministic and stable across Rust versions,
/// unlike DefaultHasher which may change between releases.
fn stable_id_from_path(path: &str) -> String {
    uuid::Uuid::new_v5(&uuid::Uuid::NAMESPACE_URL, path.as_bytes()).to_string()
}

/// GET /api/images/iso - List available ISO images
pub async fn list_iso_images(RequireRead(_claims): RequireRead) -> Json<Vec<IsoImage>> {
    tracing::debug!("images::{}", stringify!(list_iso_images));
    let mut isos = Vec::new();
    let iso_dir = "/var/lib/zyvor-fabricd/iso";

    let mut entries = match tokio::fs::read_dir(iso_dir).await {
        Ok(e) => e,
        Err(_) => return Json(isos),
    };

    while let Ok(Some(entry)) = entries.next_entry().await {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) == Some("iso") {
            let name = entry.file_name().to_string_lossy().to_string();
            let metadata = entry.metadata().await;
            let size = metadata.as_ref().map(|m| m.len()).unwrap_or(0);
            let modified = metadata
                .and_then(|m| m.modified())
                .map(DateTime::<Utc>::from)
                .unwrap_or_else(|_| Utc::now());

            let path_str = path.display().to_string();
            isos.push(IsoImage {
                id: stable_id_from_path(&path_str),
                name: name.trim_end_matches(".iso").to_string(),
                path: path_str,
                size_bytes: size,
                uploaded: modified,
            });
        }
    }

    Json(isos)
}

#[derive(Debug, Deserialize)]
pub struct UploadIsoRequest {
    pub name: String,
    pub url: String,
}

/// POST /api/images/iso/download - Download an ISO from URL
pub async fn download_iso(
    RequireWrite(_claims): RequireWrite,
    State(state): State<Arc<AppState>>,
    Json(req): Json<UploadIsoRequest>,
) -> Result<(StatusCode, Json<DownloadStatus>), (StatusCode, Json<serde_json::Value>)> {
    tracing::debug!("images::{}", stringify!(download_iso));

    // Validate name
    crate::validation::validate_vm_name(&req.name)
        .map_err(|(s, m)| crate::api_error::json_error(s, m))?;

    // Validate URL against SSRF
    crate::api::notifications::validate_external_url_public(&req.url)
        .map_err(|e| crate::api_error::json_error(StatusCode::BAD_REQUEST, e))?;

    let download_id = uuid::Uuid::new_v4().to_string();
    let status = DownloadStatus {
        id: download_id.clone(),
        name: req.name.clone(),
        state: BuildState::Pending,
        output_path: None,
        error: None,
        started: Utc::now(),
        completed: None,
    };

    state
        .store
        .save_entity("iso_downloads", &download_id, &status)
        .map_err(|e| {
            crate::api_error::json_error(StatusCode::INTERNAL_SERVER_ERROR, e.to_string())
        })?;

    let state_clone = state.clone();
    let dl_id = download_id.clone();
    let iso_name = req.name.clone();
    let url = req.url.clone();

    let dl_id_log = download_id.clone();
    let handle = tokio::spawn(async move {
        let iso_dir = "/var/lib/zyvor-fabricd/iso";
        if let Err(e) = tokio::fs::create_dir_all(iso_dir).await {
            tracing::error!("Failed to create ISO dir: {}", e);
            return;
        }
        let dest_path = format!("{}/{}.iso", iso_dir, iso_name);

        match state_clone.http_client.get(&url).send().await {
            Ok(response) if response.status().is_success() => {
                // Stream to disk instead of buffering in memory
                match stream_response_to_file(response, &dest_path).await {
                    Ok(_) => {
                        tracing::info!("Downloaded ISO '{}' to {}", iso_name, dest_path);
                        if let Ok(Some(mut s)) = state_clone
                            .store
                            .get_entity::<DownloadStatus>("iso_downloads", &dl_id)
                        {
                            s.state = BuildState::Completed;
                            s.output_path = Some(dest_path);
                            s.completed = Some(Utc::now());
                            if let Err(e) =
                                state_clone.store.save_entity("iso_downloads", &dl_id, &s)
                            {
                                tracing::error!("Failed to save download state: {}", e);
                            }
                        }
                    }
                    Err(e) => {
                        tracing::error!("Failed to download ISO: {}", e);
                        let _ = tokio::fs::remove_file(&dest_path).await;
                        mark_download_failed(&state_clone.store, "iso_downloads", &dl_id, e).await;
                    }
                }
            }
            Ok(response) => {
                mark_download_failed(
                    &state_clone.store,
                    "iso_downloads",
                    &dl_id,
                    format!("HTTP {}", response.status()),
                )
                .await;
            }
            Err(e) => {
                mark_download_failed(&state_clone.store, "iso_downloads", &dl_id, e.to_string())
                    .await;
            }
        }
    });
    tokio::spawn(async move {
        if let Err(e) = handle.await {
            tracing::error!(
                "Background ISO download task '{}' panicked: {}",
                dl_id_log,
                e
            );
        }
    });

    Ok((StatusCode::ACCEPTED, Json(status)))
}

/// DELETE /api/images/iso/:name - Delete an ISO
pub async fn delete_iso(
    RequireAdmin(_claims): RequireAdmin,
    axum::extract::Path(name): axum::extract::Path<String>,
) -> Result<StatusCode, (StatusCode, Json<serde_json::Value>)> {
    tracing::debug!("images::{}", stringify!(delete_iso));
    crate::validation::validate_vm_name(&name)
        .map_err(|(s, m)| crate::api_error::json_error(s, m))?;
    let name = input_guard::vet_component!(
        &name,
        crate::api_error::json_error(StatusCode::BAD_REQUEST, "invalid ISO name")
    );
    let path = format!("/var/lib/zyvor-fabricd/iso/{name}.iso");
    if let Err(e) = tokio::fs::remove_file(&path).await {
        if e.kind() != std::io::ErrorKind::NotFound {
            return Err(crate::api_error::json_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                e.to_string(),
            ));
        }
    }
    Ok(StatusCode::NO_CONTENT)
}

// ============================================================================
// Online Disk Resize
// ============================================================================

#[derive(Debug, Deserialize)]
pub struct ResizeDiskRequest {
    /// New size (e.g. "50G", "100G")
    pub size: String,
    /// If true, resize while VM is running via QMP block_resize
    #[serde(default)]
    pub online: bool,
}

/// POST /api/vms/:name/disk/resize - Resize a VM's disk image
pub async fn resize_disk(
    RequireWrite(_claims): RequireWrite,
    State(state): State<Arc<AppState>>,
    axum::extract::Path(vm_name): axum::extract::Path<String>,
    Json(req): Json<ResizeDiskRequest>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    tracing::debug!("images::{}", stringify!(resize_disk));
    crate::validation::validate_vm_name(&vm_name)
        .map_err(|(s, m)| crate::api_error::json_error(s, m))?;

    // Validate size format: must be a positive number followed by optional unit
    let size_valid = {
        let s = req.size.trim();
        if let Some(num_str) = s.strip_suffix(|c: char| "GMTKgmtk".contains(c)) {
            num_str
                .trim()
                .parse::<u64>()
                .map(|n| n > 0)
                .unwrap_or(false)
        } else {
            s.parse::<u64>().map(|n| n > 0).unwrap_or(false)
        }
    };
    if !size_valid {
        return Err(crate::api_error::json_error(
            StatusCode::BAD_REQUEST,
            "Size must be a positive number with optional unit suffix (e.g. '50G', '100M')",
        ));
    }

    // The VM's actual, live disk (not a naming-convention guess -- see
    // VMDriver::get_disk_path's doc comment). Getting this wrong here in
    // particular would resize the wrong file entirely.
    let image_path = state.driver.get_disk_path(&vm_name).await.map_err(|e| {
        crate::api_error::json_error(
            StatusCode::NOT_FOUND,
            format!("No disk image found for VM '{}': {}", vm_name, e),
        )
    })?;
    let image_path = image_path.display().to_string();
    let image_path = input_guard::vet!(
        &image_path,
        crate::api_error::json_error(StatusCode::BAD_REQUEST, "invalid disk path")
    );
    let size = input_guard::vet!(
        &req.size,
        crate::api_error::json_error(StatusCode::BAD_REQUEST, "invalid disk size")
    );

    // Resize with qemu-img
    let output = input_guard::argv_checked!(
        &image_path,
        crate::api_error::json_error(StatusCode::BAD_REQUEST, "invalid disk path"),
        |image_path| {
            input_guard::argv_checked!(
                &size,
                crate::api_error::json_error(StatusCode::BAD_REQUEST, "invalid disk size"),
                |size| {
                    tokio::process::Command::new("qemu-img")
                        .args(["resize", image_path, size])
                        .output()
                        .await
                        .map_err(|e| {
                            crate::api_error::json_error(
                                StatusCode::INTERNAL_SERVER_ERROR,
                                e.to_string(),
                            )
                        })?
                }
            )
        }
    );

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(crate::api_error::json_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("qemu-img resize failed: {}", stderr),
        ));
    }

    // If online and VM is running, also resize the block device via QMP
    if req.online {
        if let Ok(Some(vm)) = state.store.get_vm(&vm_name) {
            if vm.state == vm_model::VMState::Running {
                let qmp = state
                    .driver
                    .get_control_socket(&vm_name)
                    .await
                    .ok()
                    .flatten()
                    .map(|p| crate::qmp::QmpClient::for_socket(p.to_string_lossy().into_owned()));
                if let Some(qmp) = qmp {
                    // Parse size to bytes for QMP
                    let size_bytes = match parse_size_to_bytes(&req.size) {
                        Ok(b) => b,
                        Err(e) => {
                            tracing::warn!("Failed to parse size for QMP resize: {}", e);
                            return Ok(Json(
                                json!({"status": "resized", "vm": vm_name, "new_size": req.size, "warning": "Offline resize succeeded but online resize skipped due to size parse error"}),
                            ));
                        }
                    };
                    if let Err(e) = qmp.execute(
                        "block_resize",
                        json!({"device": "virtio0", "size": size_bytes}),
                    ) {
                        tracing::warn!(
                            "Online resize via QMP failed (offline resize succeeded): {}",
                            e
                        );
                    }
                }
            }
        }
    }

    tracing::info!("Resized disk for VM '{}' to {}", vm_name, req.size);
    Ok(Json(
        json!({"status": "resized", "vm": vm_name, "new_size": req.size}),
    ))
}

fn parse_size_to_bytes(size: &str) -> Result<u64, String> {
    let s = size.trim();
    let s_upper = s.to_uppercase();
    let result = if let Some(n) = s_upper.strip_suffix('T') {
        n.trim()
            .parse::<u64>()
            .map_err(|e| format!("Invalid size number: {}", e))?
            * 1024
            * 1024
            * 1024
            * 1024
    } else if let Some(n) = s_upper.strip_suffix('G') {
        n.trim()
            .parse::<u64>()
            .map_err(|e| format!("Invalid size number: {}", e))?
            * 1024
            * 1024
            * 1024
    } else if let Some(n) = s_upper.strip_suffix('M') {
        n.trim()
            .parse::<u64>()
            .map_err(|e| format!("Invalid size number: {}", e))?
            * 1024
            * 1024
    } else if let Some(n) = s_upper.strip_suffix('K') {
        n.trim()
            .parse::<u64>()
            .map_err(|e| format!("Invalid size number: {}", e))?
            * 1024
    } else {
        s.parse::<u64>()
            .map_err(|e| format!("Invalid size number: {}", e))?
    };
    if result == 0 {
        return Err("Size must be greater than zero".to_string());
    }
    Ok(result)
}

// ============================================================================
// VM Import (OVA/VMDK)
// ============================================================================

#[derive(Debug, Deserialize)]
pub struct ImportVMRequest {
    /// Path to the source image file (OVA, VMDK, VDI, VHD)
    pub source_path: String,
    /// Name for the imported VM
    pub name: String,
    /// Target format (default: qcow2)
    #[serde(default = "default_qcow2")]
    pub target_format: String,
    /// CPUs for the imported VM
    #[serde(default = "default_cpus")]
    pub cpus: u32,
    /// Memory in MB
    #[serde(default = "default_memory")]
    pub memory: u64,
}

fn default_qcow2() -> String {
    "qcow2".into()
}
fn default_cpus() -> u32 {
    2
}
fn default_memory() -> u64 {
    2048
}

#[derive(Debug, Serialize)]
pub struct ImportResult {
    pub vm_name: String,
    pub image_path: String,
    pub source_format: String,
    pub target_format: String,
    pub size_bytes: u64,
}

/// POST /api/images/import - Import a VM image (VMDK, VDI, VHD -> qcow2)
pub async fn import_vm_image(
    RequireWrite(_claims): RequireWrite,
    State(state): State<Arc<AppState>>,
    Json(req): Json<ImportVMRequest>,
) -> Result<(StatusCode, Json<ImportResult>), (StatusCode, Json<serde_json::Value>)> {
    tracing::debug!("images::{}", stringify!(import_vm_image));

    crate::validation::validate_vm_name(&req.name)
        .map_err(|(s, m)| crate::api_error::json_error(s, m))?;
    crate::validation::validate_host_path(&req.source_path)
        .map_err(|(s, m)| crate::api_error::json_error(s, m))?;

    // Validate target format
    validate_image_format(&req.target_format)?;

    // Acquire per-VM lock before duplicate check to prevent TOCTOU races
    let _lock = state.vm_lock(&req.name).lock_owned().await;

    // Check for duplicate VM name
    if let Ok(Some(_)) = state.store.get_vm(&req.name) {
        return Err(crate::api_error::json_error(
            StatusCode::CONFLICT,
            "VM with this name already exists",
        ));
    }

    // Detect source format from extension
    let source_format = std::path::Path::new(&req.source_path)
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("raw")
        .to_string();

    let dest_dir = "/var/lib/zyvor-fabricd/images";
    tokio::fs::create_dir_all(dest_dir).await.map_err(|e| {
        crate::api_error::json_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Failed to create directory: {}", e),
        )
    })?;
    let name = input_guard::vet_component!(
        &req.name,
        crate::api_error::json_error(StatusCode::BAD_REQUEST, "invalid image name")
    );
    let target_format = input_guard::vet_component!(
        &req.target_format,
        crate::api_error::json_error(StatusCode::BAD_REQUEST, "invalid image format")
    );
    let source_path = input_guard::vet!(
        &req.source_path,
        crate::api_error::json_error(StatusCode::BAD_REQUEST, "invalid source path")
    );
    let source_format = input_guard::vet_component!(
        &source_format,
        crate::api_error::json_error(StatusCode::BAD_REQUEST, "invalid source format")
    );
    let dest_path = format!("{dest_dir}/{name}.{target_format}");

    // Convert using qemu-img convert
    let output = input_guard::argv_checked!(
        source_format,
        crate::api_error::json_error(StatusCode::BAD_REQUEST, "invalid source format"),
        |source_format| {
            input_guard::argv_checked!(
                target_format,
                crate::api_error::json_error(StatusCode::BAD_REQUEST, "invalid image format"),
                |target_format| {
                    input_guard::argv_checked!(
                        source_path,
                        crate::api_error::json_error(
                            StatusCode::BAD_REQUEST,
                            "invalid source path"
                        ),
                        |source_path| {
                            input_guard::argv_checked!(
                                dest_path.as_str(),
                                crate::api_error::json_error(
                                    StatusCode::BAD_REQUEST,
                                    "invalid destination path"
                                ),
                                |dest_path| {
                                    tokio::process::Command::new("qemu-img")
                                        .args([
                                            "convert",
                                            "-f",
                                            source_format,
                                            "-O",
                                            target_format,
                                            source_path,
                                            dest_path,
                                        ])
                                        .output()
                                        .await
                                        .map_err(|e| {
                                            crate::api_error::json_error(
                                                StatusCode::INTERNAL_SERVER_ERROR,
                                                format!("qemu-img convert failed: {}", e),
                                            )
                                        })?
                                }
                            )
                        }
                    )
                }
            )
        }
    );

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(crate::api_error::json_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Image conversion failed: {}", stderr),
        ));
    }

    let size = tokio::fs::metadata(&dest_path)
        .await
        .map(|m| m.len())
        .unwrap_or(0);

    // Create VM entry
    let vm = vm_model::VM::new(req.name.clone(), dest_path.clone(), req.cpus, req.memory);
    state.store.save_vm(&vm).map_err(|e| {
        crate::api_error::json_error(StatusCode::INTERNAL_SERVER_ERROR, e.to_string())
    })?;

    tracing::info!(
        "Imported VM '{}' from {} ({} -> {})",
        req.name,
        req.source_path,
        source_format,
        req.target_format
    );

    Ok((
        StatusCode::CREATED,
        Json(ImportResult {
            vm_name: req.name,
            image_path: dest_path,
            source_format: source_format.to_string(),
            target_format: req.target_format,
            size_bytes: size,
        }),
    ))
}
