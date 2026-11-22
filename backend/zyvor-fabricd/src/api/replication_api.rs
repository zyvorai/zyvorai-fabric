// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: LicenseRef-Zyvor-Production-2.0

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
use replication::{
    ReplicationConfig, ReplicationHealthSummary, ReplicationInstance, ReplicationMetrics,
    ReplicationSite, ReplicationStatus, SiteHealth,
};
use security::{RequireAdmin, RequireRead, RequireWrite};

// ============================================================================
// Site handlers
// ============================================================================

pub async fn list_sites(
    RequireRead(_claims): RequireRead,
    State(state): State<Arc<AppState>>,
) -> impl IntoResponse {
    tracing::debug!("replication_api::{}", stringify!(list_sites));
    let items: Vec<ReplicationSite> = state
        .store
        .list_entities("replication_sites")
        .unwrap_or_else(|e| {
            tracing::error!("Storage error: {}", e);
            Vec::new()
        });
    Json(items)
}

pub async fn register_site(
    RequireWrite(_claims): RequireWrite,
    State(state): State<Arc<AppState>>,
    Json(mut site): Json<ReplicationSite>,
) -> impl IntoResponse {
    tracing::debug!("replication_api::{}", stringify!(register_site));
    if let Err((status, msg)) = crate::validation::validate_entity_name(&site.name) {
        return (status, Json(serde_json::json!({"error": msg}))).into_response();
    }
    if site.id.is_empty() {
        site.id = Uuid::new_v4().to_string();
    }
    let now = Utc::now();
    site.created = now;
    site.updated = now;
    match state
        .store
        .save_entity("replication_sites", &site.id, &site)
    {
        Ok(_) => (StatusCode::CREATED, Json(site)).into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": e.to_string()})),
        )
            .into_response(),
    }
}

pub async fn remove_site(
    RequireAdmin(_claims): RequireAdmin,
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    tracing::debug!("replication_api::{}", stringify!(remove_site));
    if let Err(e) = state.store.delete_entity("replication_sites", &id) {
        tracing::error!("Failed to delete entity: {}", e);
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": e.to_string()})),
        )
            .into_response();
    }
    (
        StatusCode::NO_CONTENT,
        Json(serde_json::json!({"status": "deleted"})),
    )
        .into_response()
}

// ============================================================================
// Replication configuration handlers
// ============================================================================

pub async fn list_replications(
    RequireRead(_claims): RequireRead,
    State(state): State<Arc<AppState>>,
) -> impl IntoResponse {
    tracing::debug!("replication_api::{}", stringify!(list_replications));
    let items: Vec<ReplicationConfig> =
        state
            .store
            .list_entities("replications")
            .unwrap_or_else(|e| {
                tracing::error!("Storage error: {}", e);
                Vec::new()
            });
    Json(items)
}

pub async fn configure_replication(
    RequireWrite(_claims): RequireWrite,
    State(state): State<Arc<AppState>>,
    Json(mut config): Json<ReplicationConfig>,
) -> impl IntoResponse {
    tracing::debug!("replication_api::{}", stringify!(configure_replication));
    if config.id.is_empty() {
        config.id = Uuid::new_v4().to_string();
    }
    let now = Utc::now();
    config.created = now;
    config.updated = now;
    match state.store.save_entity("replications", &config.id, &config) {
        Ok(_) => (StatusCode::CREATED, Json(config)).into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": e.to_string()})),
        )
            .into_response(),
    }
}

pub async fn get_replication(
    RequireRead(_claims): RequireRead,
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    tracing::debug!("replication_api::{}", stringify!(get_replication));
    match state
        .store
        .get_entity::<ReplicationConfig>("replications", &id)
    {
        Ok(Some(r)) => Json(r).into_response(),
        Ok(None) => (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({"error": "Replication not found"})),
        )
            .into_response(),
        Err(_) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": "Internal server error"})),
        )
            .into_response(),
    }
}

pub async fn pause_replication(
    RequireWrite(_claims): RequireWrite,
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    tracing::debug!("replication_api::{}", stringify!(pause_replication));
    let mut repl = match state
        .store
        .get_entity::<ReplicationConfig>("replications", &id)
    {
        Ok(Some(r)) => r,
        Ok(None) => {
            return (
                StatusCode::NOT_FOUND,
                Json(serde_json::json!({"error": "Replication not found"})),
            )
                .into_response()
        }
        Err(_) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"error": "Internal server error"})),
            )
                .into_response()
        }
    };
    repl.status = ReplicationStatus::Paused;
    repl.updated = Utc::now();
    if let Err(e) = state.store.save_entity("replications", &repl.id, &repl) {
        tracing::error!("Failed to save entity: {}", e);
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": e.to_string()})),
        )
            .into_response();
    }
    (
        StatusCode::OK,
        Json(serde_json::json!({"status": "replication paused"})),
    )
        .into_response()
}

pub async fn resume_replication(
    RequireWrite(_claims): RequireWrite,
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    tracing::debug!("replication_api::{}", stringify!(resume_replication));
    let mut repl = match state
        .store
        .get_entity::<ReplicationConfig>("replications", &id)
    {
        Ok(Some(r)) => r,
        Ok(None) => {
            return (
                StatusCode::NOT_FOUND,
                Json(serde_json::json!({"error": "Replication not found"})),
            )
                .into_response()
        }
        Err(_) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"error": "Internal server error"})),
            )
                .into_response()
        }
    };
    repl.status = ReplicationStatus::Active;
    repl.updated = Utc::now();
    if let Err(e) = state.store.save_entity("replications", &repl.id, &repl) {
        tracing::error!("Failed to save entity: {}", e);
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": e.to_string()})),
        )
            .into_response();
    }
    (
        StatusCode::OK,
        Json(serde_json::json!({"status": "replication resumed"})),
    )
        .into_response()
}

pub async fn remove_replication(
    RequireAdmin(_claims): RequireAdmin,
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    tracing::debug!("replication_api::{}", stringify!(remove_replication));
    if let Err(e) = state.store.delete_entity("replications", &id) {
        tracing::error!("Failed to delete entity: {}", e);
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": e.to_string()})),
        )
            .into_response();
    }
    (
        StatusCode::NO_CONTENT,
        Json(serde_json::json!({"status": "deleted"})),
    )
        .into_response()
}

#[derive(serde::Deserialize)]
pub struct StartSyncRequest {
    pub replication_id: String,
}

pub async fn start_sync(
    RequireWrite(_claims): RequireWrite,
    State(state): State<Arc<AppState>>,
    Json(req): Json<StartSyncRequest>,
) -> impl IntoResponse {
    tracing::debug!("replication_api::{}", stringify!(start_sync));
    let repl = match state
        .store
        .get_entity::<ReplicationConfig>("replications", &req.replication_id)
    {
        Ok(Some(r)) => r,
        Ok(None) => {
            return (
                StatusCode::NOT_FOUND,
                Json(serde_json::json!({"error": "Replication not found"})),
            )
                .into_response()
        }
        Err(_) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"error": "Internal server error"})),
            )
                .into_response()
        }
    };
    if repl.status != ReplicationStatus::Active {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({
                "error": format!("Cannot sync replication in {:?} state", repl.status)
            })),
        )
            .into_response();
    }

    // Resolve the target site address for rsync
    let target_address = state
        .store
        .get_entity::<ReplicationSite>("replication_sites", &repl.target_site_id)
        .ok()
        .flatten()
        .map(|s| s.address.clone())
        .unwrap_or_else(|| repl.target_site_id.clone());

    // Perform the actual sync in a background task. Resolve the VM's
    // actual, live disk (a single qcow2 file, not a naming-convention
    // guess at a directory) via the driver -- real (FluxVM-managed) VMs
    // are almost never stored at /var/lib/zyvor-fabricd/images/<name>, so
    // the old guess found nothing for virtually every real VM, and even
    // when something existed there it was treated as a directory
    // (read_dir) when disks are files.
    let repl_id = repl.id.clone();
    let vm_name = repl.vm_name.clone();
    let compression = repl.compression_enabled;
    let bandwidth_limit = repl.bandwidth_limit_mbps;
    let store = state.store.clone();
    let disk_path = state.driver.get_disk_path(&vm_name).await.ok();

    tokio::task::spawn_blocking(move || {
        let bytes_synced: u64;

        if let Some(source_path) = disk_path.as_ref().filter(|p| p.exists()) {
            let source_path = source_path.display().to_string();
            bytes_synced = std::fs::metadata(&source_path)
                .map(|m| m.len())
                .unwrap_or(0);

            let mut rsync_args = vec!["-a".to_string(), "--partial".to_string()];
            if compression {
                rsync_args.push("-z".to_string());
            }
            if let Some(bw) = bandwidth_limit {
                rsync_args.push(format!("--bwlimit={}", bw * 1024));
            }
            rsync_args.push(source_path.clone());
            rsync_args.push(format!(
                "{}:/var/lib/zyvor-fabricd/images/{}.qcow2",
                target_address, vm_name
            ));

            match std::process::Command::new("rsync")
                .args(&rsync_args)
                .output()
            {
                Ok(out) if out.status.success() => {
                    tracing::info!(
                        "rsync completed for replication {} (VM '{}')",
                        repl_id,
                        vm_name
                    );
                }
                Ok(out) => {
                    let stderr = String::from_utf8_lossy(&out.stderr);
                    tracing::warn!("rsync non-zero for replication {}: {}", repl_id, stderr);
                }
                Err(e) => {
                    tracing::warn!("rsync not available for replication {}: {}", repl_id, e);
                }
            }
        } else {
            tracing::warn!(
                "No disk found for VM '{}', skipping sync for replication {}",
                vm_name,
                repl_id
            );
            return;
        }

        // Update the replication last_sync timestamp and record metrics
        if let Ok(Some(mut r)) = store.get_entity::<ReplicationConfig>("replications", &repl_id) {
            r.last_sync = Some(Utc::now());
            r.next_sync = Some(Utc::now() + chrono::Duration::minutes(r.rpo_minutes as i64));
            r.updated = Utc::now();
            let _ = store.save_entity("replications", &repl_id, &r);
        }

        // Save metrics
        let metrics = replication::ReplicationMetrics {
            replication_id: repl_id.clone(),
            vm_name: vm_name.clone(),
            rpo_actual_minutes: 0,
            rpo_violation: false,
            bytes_transferred_last_sync: bytes_synced,
            sync_duration_secs: 0,
            total_bytes_transferred: bytes_synced,
            sync_count: 1,
        };
        let _ = store.save_entity("replication_metrics", &repl_id, &metrics);
    });

    Json(serde_json::json!({"status": "sync_started", "replication_id": repl.id})).into_response()
}

pub async fn get_replication_metrics(
    RequireRead(_claims): RequireRead,
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    tracing::debug!("replication_api::{}", stringify!(get_replication_metrics));
    match state
        .store
        .get_entity::<ReplicationMetrics>("replication_metrics", &id)
    {
        Ok(Some(m)) => Json(m).into_response(),
        Ok(None) => (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({"error": "Replication metrics not found"})),
        )
            .into_response(),
        Err(_) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": "Internal server error"})),
        )
            .into_response(),
    }
}

pub async fn check_rpo_violations(
    RequireRead(_claims): RequireRead,
    State(state): State<Arc<AppState>>,
) -> impl IntoResponse {
    tracing::debug!("replication_api::{}", stringify!(check_rpo_violations));
    let replications: Vec<ReplicationConfig> = state
        .store
        .list_entities("replications")
        .unwrap_or_else(|e| {
            tracing::error!("Storage error: {}", e);
            Vec::new()
        });
    let now = Utc::now();
    let violations: Vec<_> = replications
        .into_iter()
        .filter(|r| {
            r.status == ReplicationStatus::Active
                && r.last_sync
                    .is_none_or(|last| (now - last).num_minutes() as u32 > r.rpo_minutes)
        })
        .collect();
    Json(violations)
}

pub async fn get_replication_health(
    RequireRead(_claims): RequireRead,
    State(state): State<Arc<AppState>>,
) -> impl IntoResponse {
    tracing::debug!("replication_api::{}", stringify!(get_replication_health));
    let replications: Vec<ReplicationConfig> = state
        .store
        .list_entities("replications")
        .unwrap_or_else(|e| {
            tracing::error!("Storage error: {}", e);
            Vec::new()
        });
    let now = Utc::now();
    let mut summary = ReplicationHealthSummary {
        total: replications.len() as u32,
        active: 0,
        paused: 0,
        error: 0,
        rpo_violations: 0,
        avg_rpo_minutes: 0.0,
        total_bytes_transferred_24h: 0,
        sites: Vec::new(),
    };
    let mut active_rpo_sum = 0u64;
    for r in &replications {
        match r.status {
            ReplicationStatus::Active => {
                summary.active += 1;
                active_rpo_sum += r.rpo_minutes as u64;
                if r.last_sync
                    .is_none_or(|last| (now - last).num_minutes() as u32 > r.rpo_minutes)
                {
                    summary.rpo_violations += 1;
                }
            }
            ReplicationStatus::Paused => summary.paused += 1,
            ReplicationStatus::Error => summary.error += 1,
            _ => {}
        }
    }
    if summary.active > 0 {
        summary.avg_rpo_minutes = active_rpo_sum as f64 / summary.active as f64;
    }

    let sites: Vec<ReplicationSite> = state
        .store
        .list_entities("replication_sites")
        .unwrap_or_else(|e| {
            tracing::error!("Storage error: {}", e);
            Vec::new()
        });
    summary.sites = sites
        .into_iter()
        .map(|s| {
            let site_replications: Vec<&ReplicationConfig> = replications
                .iter()
                .filter(|r| r.target_site_id == s.id || r.source_site_id == s.id)
                .collect();
            let health = if site_replications
                .iter()
                .any(|r| r.status == ReplicationStatus::Error)
            {
                "critical"
            } else if site_replications.iter().any(|r| {
                r.status == ReplicationStatus::Active
                    && r.last_sync
                        .is_none_or(|last| (now - last).num_minutes() as u32 > r.rpo_minutes)
            }) {
                "degraded"
            } else {
                "healthy"
            };
            SiteHealth {
                site_id: s.id,
                site_name: s.name,
                replication_count: site_replications.len() as u32,
                health: health.to_string(),
            }
        })
        .collect();

    Json(summary)
}

pub async fn list_recovery_instances(
    RequireRead(_claims): RequireRead,
    State(state): State<Arc<AppState>>,
) -> impl IntoResponse {
    tracing::debug!("replication_api::{}", stringify!(list_recovery_instances));
    let items: Vec<ReplicationInstance> = state
        .store
        .list_entities("recovery_instances")
        .unwrap_or_else(|e| {
            tracing::error!("Storage error: {}", e);
            Vec::new()
        });
    Json(items)
}
