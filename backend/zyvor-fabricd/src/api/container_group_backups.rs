// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: LicenseRef-Zyvor-Production-2.0

//! Backup/restore for `ContainerGroup`'s hostPath volumes.
//!
//! Deliberately not a reuse of `api::backups`'s VM backup path: that
//! mechanism is qcow2-specific end to end (`qemu-img convert` against the
//! VM's live disk image). A ContainerGroup has no disk image at all --
//! `ContainerSpec.volume_mounts` are host-directory bind mounts (see
//! `container_declarative::ContainerGroupSpec`), so the only thing that
//! makes sense to snapshot is those host directories, tarred/gzipped the
//! way the (otherwise unused) `backup` crate already does it for an
//! arbitrary directory tree.
//!
//! v1 scope, deliberately: synchronous backup/restore (no async job/progress
//! polling like `api::backups::BackupJob` -- ContainerGroup volumes are
//! expected to be far smaller than a VM disk for a developer-preview
//! feature; revisit if that stops being true), no scheduling/retention
//! reaper (mirrors `api::backups::BackupPolicy.next_run`, which is also
//! never actually consumed today), and no replication/site-recovery
//! integration (those crates are entirely VM-image-path-specific and out of
//! scope for this slice).

use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::collections::BTreeSet;
use std::sync::Arc;
use uuid::Uuid;

use crate::api::container_declarative::ContainerGroupSpec;
use crate::server::AppState;
use security::{RequireRead, RequireWrite};

/// Backup directory, read once from the CONTAINER_GROUP_BACKUP_DIR
/// environment variable (or default) -- separate from `api::backups`'s
/// `BACKUP_DIR` since these are plain tarballs, not qcow2 images, and
/// shouldn't be mixed into the same directory tree.
static BACKUP_DIR: std::sync::LazyLock<String> = std::sync::LazyLock::new(|| {
    std::env::var("CONTAINER_GROUP_BACKUP_DIR")
        .unwrap_or_else(|_| "/var/lib/zyvor-fabricd/container-group-backups".to_string())
});

fn default_retention_days() -> u32 {
    30
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BackupStatus {
    Completed,
    Failed,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContainerGroupBackup {
    pub id: String,
    pub container_group_name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tenant: Option<String>,
    /// Host directories archived, in the order they were appended to the
    /// tarball -- `restore_container_group_backup` extracts entry `i` back
    /// onto `volume_paths[i]`, so this order must never change once written.
    pub volume_paths: Vec<String>,
    pub size_bytes: u64,
    pub status: BackupStatus,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    pub archive_path: String,
    pub created: DateTime<Utc>,
    pub retention_days: u32,
    pub expires_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CreateContainerGroupBackupRequest {
    pub container_group_name: String,
    #[serde(default = "default_retention_days")]
    pub retention_days: u32,
}

#[derive(Debug, Serialize)]
pub struct RestoreResult {
    pub container_group_name: String,
    pub restored_paths: Vec<String>,
    pub warnings: Vec<String>,
}

fn err(status: StatusCode, msg: impl Into<String>) -> (StatusCode, Json<serde_json::Value>) {
    (status, Json(json!({"error": msg.into()})))
}

/// Host directories to archive for `spec`: every container's
/// `volume_mounts[].host`, deduplicated and in stable (sorted) order so a
/// re-backup of the same group always lays out the tarball identically.
fn collect_volume_paths(spec: &ContainerGroupSpec) -> Vec<String> {
    spec.containers
        .iter()
        .flat_map(|c| c.volume_mounts.iter().map(|vm| vm.host.clone()))
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect()
}

/// Tar+gzip every path in `volume_paths` into `archive_path`, each under an
/// archive entry named by its index (`"0"`, `"1"`, ...) rather than the real
/// path -- sidesteps encoding an arbitrary absolute host path as a safe
/// archive entry name. `restore_volume` reverses this using the same index.
fn write_archive(archive_path: &std::path::Path, volume_paths: &[String]) -> std::io::Result<()> {
    let file = std::fs::File::create(archive_path)?;
    let encoder = flate2::write::GzEncoder::new(file, flate2::Compression::default());
    let mut builder = tar::Builder::new(encoder);
    for (idx, path) in volume_paths.iter().enumerate() {
        builder.append_dir_all(idx.to_string(), path)?;
    }
    builder.into_inner()?.finish()?;
    Ok(())
}

/// Extract the archive entries under index `idx` back onto `dest`. Uses
/// `Entry::unpack` (which writes to an exact given path) rather than
/// `Entry::unpack_in` (which would append the entry's *archive* path --
/// `{idx}/...` -- onto `dest`, landing files one directory too deep, e.g.
/// `dest/0/file` instead of `dest/file`): each entry's path is stripped of
/// its `{idx}` prefix and rejoined onto `dest` before unpacking.
fn restore_volume(archive_path: &str, idx: usize, dest: &str) -> std::io::Result<()> {
    let file = std::fs::File::open(archive_path)?;
    let decoder = flate2::read::GzDecoder::new(file);
    let mut archive = tar::Archive::new(decoder);
    let prefix = idx.to_string();
    std::fs::create_dir_all(dest)?;
    for entry in archive.entries()? {
        let mut entry = entry?;
        let entry_path = entry.path()?.into_owned();
        let Ok(rel) = entry_path.strip_prefix(&prefix) else {
            continue;
        };
        if rel.as_os_str().is_empty() {
            // The `{idx}` directory entry itself -- already created above.
            continue;
        }
        let target = std::path::Path::new(dest).join(rel);
        if let Some(parent) = target.parent() {
            std::fs::create_dir_all(parent)?;
        }
        entry.unpack(&target)?;
    }
    Ok(())
}

/// POST /api/container-group-backups -- tar+gzip every hostPath volume of a
/// ContainerGroup into one archive.
pub async fn create_container_group_backup(
    RequireWrite(claims): RequireWrite,
    State(state): State<Arc<AppState>>,
    Json(req): Json<CreateContainerGroupBackupRequest>,
) -> Result<(StatusCode, Json<ContainerGroupBackup>), (StatusCode, Json<serde_json::Value>)> {
    let spec = state
        .store
        .get_entity::<ContainerGroupSpec>("container_groups", &req.container_group_name)
        .map_err(|e| err(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
        .ok_or_else(|| err(StatusCode::NOT_FOUND, "ContainerGroup not found"))?;

    if let Some(claim_tenant) = &claims.tenant {
        if spec.tenant.as_deref() != Some(claim_tenant.as_str()) {
            return Err(err(StatusCode::NOT_FOUND, "ContainerGroup not found"));
        }
    }

    let volume_paths = collect_volume_paths(&spec);
    if volume_paths.is_empty() {
        return Err(err(
            StatusCode::BAD_REQUEST,
            "ContainerGroup has no volume_mounts to back up",
        ));
    }

    let backup_id = Uuid::new_v4().to_string();
    let group_dir = std::path::Path::new(&*BACKUP_DIR).join(&spec.name);
    tokio::fs::create_dir_all(&group_dir)
        .await
        .map_err(|e| err(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    let archive_path = group_dir.join(format!("{backup_id}.tar.gz"));

    let missing: Vec<String> = volume_paths
        .iter()
        .filter(|p| !std::path::Path::new(p).exists())
        .cloned()
        .collect();
    if !missing.is_empty() {
        return Err(err(
            StatusCode::CONFLICT,
            format!(
                "volume path(s) not found on this host, refusing a partial backup: {}",
                missing.join(", ")
            ),
        ));
    }

    let volume_paths_for_archive = volume_paths.clone();
    let archive_path_for_archive = archive_path.clone();
    let archive_result = tokio::task::spawn_blocking(move || {
        write_archive(&archive_path_for_archive, &volume_paths_for_archive)
    })
    .await
    .map_err(|e| err(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    if let Err(e) = archive_result {
        let _ = tokio::fs::remove_file(&archive_path).await;
        return Err(err(
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("failed to create archive: {e}"),
        ));
    }

    let size_bytes = tokio::fs::metadata(&archive_path)
        .await
        .map(|m| m.len())
        .unwrap_or(0);

    let now = Utc::now();
    let backup = ContainerGroupBackup {
        id: backup_id,
        container_group_name: spec.name.clone(),
        tenant: spec.tenant.clone(),
        volume_paths,
        size_bytes,
        status: BackupStatus::Completed,
        error: None,
        archive_path: archive_path.display().to_string(),
        created: now,
        retention_days: req.retention_days,
        expires_at: now + Duration::days(req.retention_days as i64),
    };

    state
        .store
        .save_entity("container_group_backups", &backup.id, &backup)
        .map_err(|e| err(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    Ok((StatusCode::CREATED, Json(backup)))
}

/// GET /api/container-group-backups -- list, scoped to the caller's tenant
/// when their JWT carries one (mirrors
/// `container_declarative::list_container_group_events`).
pub async fn list_container_group_backups(
    RequireRead(claims): RequireRead,
    State(state): State<Arc<AppState>>,
) -> Result<Json<Vec<ContainerGroupBackup>>, (StatusCode, Json<serde_json::Value>)> {
    let mut backups: Vec<ContainerGroupBackup> = state
        .store
        .list_entities("container_group_backups")
        .map_err(|e| err(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    if let Some(tenant) = claims.tenant.as_deref() {
        backups.retain(|b| b.tenant.as_deref() == Some(tenant));
    }
    backups.sort_by_key(|b| std::cmp::Reverse(b.created));

    Ok(Json(backups))
}

fn load_backup_for_caller(
    state: &AppState,
    claims: &security::Claims,
    id: &str,
) -> Result<ContainerGroupBackup, (StatusCode, Json<serde_json::Value>)> {
    let backup = state
        .store
        .get_entity::<ContainerGroupBackup>("container_group_backups", id)
        .map_err(|e| err(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
        .ok_or_else(|| err(StatusCode::NOT_FOUND, "backup not found"))?;

    if let Some(claim_tenant) = &claims.tenant {
        if backup.tenant.as_deref() != Some(claim_tenant.as_str()) {
            return Err(err(StatusCode::NOT_FOUND, "backup not found"));
        }
    }
    Ok(backup)
}

/// GET /api/container-group-backups/:id
pub async fn get_container_group_backup(
    RequireRead(claims): RequireRead,
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<ContainerGroupBackup>, (StatusCode, Json<serde_json::Value>)> {
    load_backup_for_caller(&state, &claims, &id).map(Json)
}

/// DELETE /api/container-group-backups/:id
pub async fn delete_container_group_backup(
    RequireWrite(claims): RequireWrite,
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<StatusCode, (StatusCode, Json<serde_json::Value>)> {
    let backup = load_backup_for_caller(&state, &claims, &id)?;

    if let Err(e) = tokio::fs::remove_file(&backup.archive_path).await {
        if e.kind() != std::io::ErrorKind::NotFound {
            tracing::warn!(
                "failed to remove ContainerGroup backup archive '{}': {}",
                backup.archive_path,
                e
            );
        }
    }

    state
        .store
        .delete_entity("container_group_backups", &id)
        .map_err(|e| err(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    Ok(StatusCode::NO_CONTENT)
}

/// POST /api/container-group-backups/:id/restore -- extract the archive
/// back onto the exact host paths it was taken from. Restoring onto a
/// ContainerGroup whose Pods are still running and actively writing to the
/// same hostPath is the caller's call to make (mirrors `api::backups`,
/// which likewise doesn't stop/start the VM around a backup/restore); the
/// result's `warnings` surface anything that didn't extract cleanly rather
/// than failing the whole restore over one bad entry.
pub async fn restore_container_group_backup(
    RequireWrite(claims): RequireWrite,
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<RestoreResult>, (StatusCode, Json<serde_json::Value>)> {
    let backup = load_backup_for_caller(&state, &claims, &id)?;

    if !std::path::Path::new(&backup.archive_path).exists() {
        return Err(err(
            StatusCode::CONFLICT,
            format!("backup archive not found at '{}'", backup.archive_path),
        ));
    }

    let archive_path = backup.archive_path.clone();
    let volume_paths = backup.volume_paths.clone();
    let (restored_paths, warnings) = tokio::task::spawn_blocking(move || {
        let mut restored = Vec::new();
        let mut warnings = Vec::new();
        // Re-opens the archive per volume rather than sharing one
        // `tar::Archive` iterator: `tar` only supports a single forward pass
        // over entries, and there are at most a handful of volume mounts
        // per ContainerGroup, so the re-read cost is negligible.
        for (idx, dest) in volume_paths.iter().enumerate() {
            match restore_volume(&archive_path, idx, dest) {
                Ok(()) => restored.push(dest.clone()),
                Err(e) => warnings.push(format!("failed to restore '{dest}': {e}")),
            }
        }
        (restored, warnings)
    })
    .await
    .map_err(|e| err(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    Ok(Json(RestoreResult {
        container_group_name: backup.container_group_name,
        restored_paths,
        warnings,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::declarative::VolumeMount;

    fn sample_spec_with_mounts(mounts: Vec<(&str, &str)>) -> ContainerGroupSpec {
        let volume_mounts: Vec<VolumeMount> = mounts
            .into_iter()
            .map(|(host, guest)| VolumeMount {
                host: host.to_string(),
                guest: guest.to_string(),
                mount_type: "virtiofs".to_string(),
                readonly: false,
            })
            .collect();
        let spec_json = serde_json::json!({
            "name": "web",
            "containers": [{
                "name": "app",
                "image": "nginx",
                "resources": {"cpus": 1, "memory": "512M"},
            }],
        });
        let mut spec: ContainerGroupSpec = serde_json::from_value(spec_json).unwrap();
        spec.containers[0].volume_mounts = volume_mounts;
        spec
    }

    #[test]
    fn collect_volume_paths_dedupes_and_sorts() {
        let spec = sample_spec_with_mounts(vec![
            ("/data/b", "/mnt/b"),
            ("/data/a", "/mnt/a"),
            ("/data/b", "/mnt/b2"),
        ]);
        assert_eq!(
            collect_volume_paths(&spec),
            vec!["/data/a".to_string(), "/data/b".to_string()]
        );
    }

    #[test]
    fn collect_volume_paths_is_empty_for_a_volumeless_group() {
        let spec = sample_spec_with_mounts(vec![]);
        assert!(collect_volume_paths(&spec).is_empty());
    }

    #[test]
    fn backup_and_restore_round_trip_preserves_file_contents_at_the_top_level() {
        let src_dir = tempfile::tempdir().unwrap();
        let restore_dir = tempfile::tempdir().unwrap();
        let src_path = src_dir.path().join("vol0");
        std::fs::create_dir_all(&src_path).unwrap();
        std::fs::write(src_path.join("hello.txt"), b"hello world").unwrap();
        std::fs::create_dir_all(src_path.join("nested")).unwrap();
        std::fs::write(src_path.join("nested").join("deep.txt"), b"deep file").unwrap();

        let archive_path = restore_dir.path().join("test.tar.gz");
        write_archive(&archive_path, &[src_path.to_string_lossy().into_owned()]).unwrap();

        let dest_path = restore_dir.path().join("restored");
        restore_volume(
            archive_path.to_str().unwrap(),
            0,
            dest_path.to_str().unwrap(),
        )
        .unwrap();

        // Regression check: files must land directly under `dest`, not
        // nested under an extra `dest/0/...` -- see `restore_volume`'s doc
        // comment for why a plain `unpack_in` would get this wrong.
        assert_eq!(
            std::fs::read_to_string(dest_path.join("hello.txt")).unwrap(),
            "hello world"
        );
        assert_eq!(
            std::fs::read_to_string(dest_path.join("nested").join("deep.txt")).unwrap(),
            "deep file"
        );
        assert!(!dest_path.join("0").exists());
    }
}
