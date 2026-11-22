// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: LicenseRef-Zyvor-Production-2.0

use axum::{
    extract::{Multipart, Path, Query, State},
    http::StatusCode,
    Json,
};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use serde_json::json;
use sha2::{Digest, Sha256};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;
use tokio::io::AsyncWriteExt;
use uuid::Uuid;

use crate::api::migration::{MigrationState, MigrationStatus};
use crate::server::AppState;
use security::{RequireAdmin, RequireRead, RequireWrite};
use service_mesh::models::Service;
use vm_model::VM;

const USERS_STORE: &str = "dashboard_users";
const WEBHOOKS_STORE: &str = "webhooks";
const CONVERT_JOBS_STORE: &str = "image_convert_jobs";

// ============================================================================
// Dashboard users
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
struct DashboardUserRecord {
    id: String,
    username: String,
    role: String,
    enabled: bool,
    password_hash: String,
    created_at: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    last_login: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct CreateUserRequest {
    username: String,
    password: String,
    role: String,
}

#[derive(Debug, Deserialize)]
pub struct UpdateUserRequest {
    #[serde(default)]
    enabled: Option<bool>,
    #[serde(default)]
    role: Option<String>,
    #[serde(default)]
    password: Option<String>,
}

fn hash_password(password: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(password.as_bytes());
    // sha2 0.11 / digest hybrid-array: GenericArray no longer implements LowerHex.
    hasher
        .finalize()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

fn user_response(u: &DashboardUserRecord) -> serde_json::Value {
    json!({
        "id": u.id,
        "username": u.username,
        "role": u.role,
        "enabled": u.enabled,
        "created_at": u.created_at,
        "last_login": u.last_login
    })
}

pub async fn list_users(
    RequireRead(_claims): RequireRead,
    State(state): State<Arc<AppState>>,
) -> Result<Json<Vec<serde_json::Value>>, (StatusCode, Json<serde_json::Value>)> {
    let users: Vec<DashboardUserRecord> =
        state.store.list_entities(USERS_STORE).map_err(store_err)?;
    Ok(Json(users.iter().map(user_response).collect()))
}

pub async fn create_user(
    RequireAdmin(_claims): RequireAdmin,
    State(state): State<Arc<AppState>>,
    Json(req): Json<CreateUserRequest>,
) -> Result<(StatusCode, Json<serde_json::Value>), (StatusCode, Json<serde_json::Value>)> {
    if !req
        .username
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
    {
        return Err(bad_request("Invalid username"));
    }
    if req.password.len() < 8 {
        return Err(bad_request("Password must be at least 8 characters"));
    }
    let role = req.role.to_lowercase();
    if !matches!(role.as_str(), "admin" | "operator" | "viewer") {
        return Err(bad_request("Role must be admin, operator, or viewer"));
    }
    let existing: Vec<DashboardUserRecord> =
        state.store.list_entities(USERS_STORE).unwrap_or_default();
    if existing.iter().any(|u| u.username == req.username) {
        return Err(bad_request("Username already exists"));
    }
    let record = DashboardUserRecord {
        id: Uuid::new_v4().to_string(),
        username: req.username,
        role,
        enabled: true,
        password_hash: hash_password(&req.password),
        created_at: Utc::now().to_rfc3339(),
        last_login: None,
    };
    state
        .store
        .save_entity(USERS_STORE, &record.id, &record)
        .map_err(store_err)?;
    Ok((StatusCode::CREATED, Json(user_response(&record))))
}

pub async fn update_user(
    RequireAdmin(_claims): RequireAdmin,
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    Json(req): Json<UpdateUserRequest>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let mut user = state
        .store
        .get_entity::<DashboardUserRecord>(USERS_STORE, &id)
        .map_err(store_err)?
        .ok_or_else(|| not_found("User not found"))?;
    if let Some(enabled) = req.enabled {
        user.enabled = enabled;
    }
    if let Some(role) = req.role {
        let role = role.to_lowercase();
        if !matches!(role.as_str(), "admin" | "operator" | "viewer") {
            return Err(bad_request("Invalid role"));
        }
        user.role = role;
    }
    if let Some(password) = req.password {
        if password.len() < 8 {
            return Err(bad_request("Password must be at least 8 characters"));
        }
        user.password_hash = hash_password(&password);
    }
    state
        .store
        .save_entity(USERS_STORE, &id, &user)
        .map_err(store_err)?;
    Ok(Json(user_response(&user)))
}

pub async fn delete_user(
    RequireAdmin(_claims): RequireAdmin,
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<StatusCode, (StatusCode, Json<serde_json::Value>)> {
    if state
        .store
        .get_entity::<DashboardUserRecord>(USERS_STORE, &id)
        .map_err(store_err)?
        .is_none()
    {
        return Err(not_found("User not found"));
    }
    state
        .store
        .delete_entity(USERS_STORE, &id)
        .map_err(store_err)?;
    Ok(StatusCode::NO_CONTENT)
}

// ============================================================================
// Webhooks
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WebhookConfig {
    pub id: String,
    pub url: String,
    pub events: Vec<String>,
    #[serde(rename = "type")]
    pub webhook_type: String,
    pub enabled: bool,
}

#[derive(Debug, Deserialize)]
pub struct CreateWebhookRequest {
    url: String,
    events: Vec<String>,
    #[serde(rename = "type")]
    webhook_type: String,
    #[serde(default = "default_enabled")]
    enabled: bool,
}

fn default_enabled() -> bool {
    true
}

#[derive(Debug, Deserialize)]
pub struct TestWebhookRequest {
    webhook_id: String,
}

pub async fn list_webhooks(
    RequireRead(_claims): RequireRead,
    State(state): State<Arc<AppState>>,
) -> Result<Json<Vec<WebhookConfig>>, (StatusCode, Json<serde_json::Value>)> {
    let hooks: Vec<WebhookConfig> = state
        .store
        .list_entities(WEBHOOKS_STORE)
        .map_err(store_err)?;
    Ok(Json(hooks))
}

pub async fn create_webhook(
    RequireAdmin(_claims): RequireAdmin,
    State(state): State<Arc<AppState>>,
    Json(req): Json<CreateWebhookRequest>,
) -> Result<(StatusCode, Json<WebhookConfig>), (StatusCode, Json<serde_json::Value>)> {
    if req.url.is_empty() || !req.url.starts_with("http") {
        return Err(bad_request("URL must be http or https"));
    }
    if req.events.is_empty() {
        return Err(bad_request("Select at least one event"));
    }
    let hook = WebhookConfig {
        id: Uuid::new_v4().to_string(),
        url: req.url,
        events: req.events,
        webhook_type: req.webhook_type,
        enabled: req.enabled,
    };
    state
        .store
        .save_entity(WEBHOOKS_STORE, &hook.id, &hook)
        .map_err(store_err)?;
    Ok((StatusCode::CREATED, Json(hook)))
}

pub async fn delete_webhook(
    RequireAdmin(_claims): RequireAdmin,
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<StatusCode, (StatusCode, Json<serde_json::Value>)> {
    state
        .store
        .delete_entity(WEBHOOKS_STORE, &id)
        .map_err(store_err)?;
    Ok(StatusCode::NO_CONTENT)
}

pub async fn test_webhook(
    RequireAdmin(_claims): RequireAdmin,
    State(state): State<Arc<AppState>>,
    Json(req): Json<TestWebhookRequest>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let hook = state
        .store
        .get_entity::<WebhookConfig>(WEBHOOKS_STORE, &req.webhook_id)
        .map_err(store_err)?
        .ok_or_else(|| not_found("Webhook not found"))?;
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(10))
        .build()
        .map_err(|e| internal_err(e.to_string()))?;
    let payload = json!({
        "event": "webhook.test",
        "timestamp": Utc::now().to_rfc3339(),
        "source": "zyvor-fabricd"
    });
    let resp = client
        .post(&hook.url)
        .json(&payload)
        .send()
        .await
        .map_err(|e| internal_err(format!("Request failed: {}", e)))?;
    let ok = resp.status().is_success();
    Ok(Json(json!({
        "ok": ok,
        "status": resp.status().as_u16(),
        "message": if ok { "Webhook delivered successfully" } else { "Webhook returned non-success status" }
    })))
}

// ============================================================================
// VM compare & healthcheck
// ============================================================================

#[derive(Debug, Deserialize)]
pub struct CompareQuery {
    #[serde(default)]
    pub source: String,
    #[serde(default)]
    pub target: String,
}

fn vm_field(label: &str, a: &str, b: &str) -> serde_json::Value {
    json!({
        "label": label,
        "source": a,
        "target": b,
        "match": a == b
    })
}

pub async fn compare_vms(
    RequireRead(_claims): RequireRead,
    State(state): State<Arc<AppState>>,
    Query(q): Query<CompareQuery>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    if q.source.is_empty() || q.target.is_empty() {
        return Err(bad_request(
            "source and target query parameters are required",
        ));
    }
    let source = load_vm(&state, &q.source)?;
    let target = load_vm(&state, &q.target)?;
    let fields = vec![
        vm_field(
            "State",
            &format!("{:?}", source.state),
            &format!("{:?}", target.state),
        ),
        vm_field("CPUs", &source.cpus.to_string(), &target.cpus.to_string()),
        vm_field(
            "Memory (MB)",
            &source.memory.to_string(),
            &target.memory.to_string(),
        ),
        vm_field(
            "Disk (GB)",
            &source.disk.to_string(),
            &target.disk.to_string(),
        ),
        vm_field("Image", &source.image, &target.image),
        vm_field(
            "IP",
            &source.ip.clone().unwrap_or_default(),
            &target.ip.clone().unwrap_or_default(),
        ),
    ];
    Ok(Json(json!({
        "source_name": source.name,
        "target_name": target.name,
        "fields": fields,
        "timestamp": Utc::now().to_rfc3339()
    })))
}

pub async fn vm_healthcheck(
    RequireRead(_claims): RequireRead,
    State(state): State<Arc<AppState>>,
    Path(name): Path<String>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let vm = load_vm(&state, &name)?;
    let mut checks = Vec::new();
    let state_ok = matches!(vm.state, vm_model::VMState::Running);
    checks.push(json!({
        "name": "VM state",
        "status": if state_ok { "pass" } else { "warning" },
        "message": format!("VM is {:?}", vm.state),
    }));
    let disk_ok = vm.disk > 0;
    checks.push(json!({
        "name": "Disk allocation",
        "status": if disk_ok { "pass" } else { "fail" },
        "message": format!("{} GB configured", vm.disk),
    }));
    let mem_ok = vm.memory >= 512;
    checks.push(json!({
        "name": "Memory",
        "status": if mem_ok { "pass" } else { "warning" },
        "message": format!("{} MB allocated", vm.memory),
    }));
    // The VM's actual, live disk (not a guess at the dead
    // /var/lib/machines convention, which hasn't existed since the
    // systemd-machined removal and made this check report "warning" for
    // virtually every real VM regardless of whether its disk was fine).
    let image_exists = state.driver.get_disk_path(&vm.name).await.is_ok();
    checks.push(json!({
        "name": "Disk image",
        "status": if image_exists { "pass" } else { "warning" },
        "message": if image_exists { "Image file found on host" } else { "Image file not found in default paths" },
    }));
    let has_fail = checks.iter().any(|c| c["status"] == "fail");
    let has_warn = checks.iter().any(|c| c["status"] == "warning");
    let overall = if has_fail {
        "fail"
    } else if has_warn {
        "warning"
    } else {
        "pass"
    };
    Ok(Json(json!({
        "vm": name,
        "overall": overall,
        "checks": checks,
        "timestamp": Utc::now().to_rfc3339()
    })))
}

fn load_vm(state: &AppState, name: &str) -> Result<VM, (StatusCode, Json<serde_json::Value>)> {
    state
        .store
        .get_vm(name)
        .map_err(store_err)?
        .ok_or_else(|| not_found(format!("VM '{}' not found", name)))
}

// ============================================================================
// Migration UX endpoints
// ============================================================================

fn migration_status_str(state: &MigrationState) -> &'static str {
    match state {
        MigrationState::Completed => "completed",
        MigrationState::Failed => "failed",
        MigrationState::Cancelled => "cancelled",
        MigrationState::Pending
        | MigrationState::PreCheck
        | MigrationState::Syncing
        | MigrationState::Switching => "running",
    }
}

fn migration_duration(m: &MigrationStatus) -> String {
    let end = m.completed.unwrap_or_else(Utc::now);
    let secs = (end - m.started).num_seconds().max(0);
    if secs < 60 {
        format!("{}s", secs)
    } else if secs < 3600 {
        format!("{}m", secs / 60)
    } else {
        format!("{}h {}m", secs / 3600, (secs % 3600) / 60)
    }
}

fn migration_entry(m: &MigrationStatus) -> serde_json::Value {
    json!({
        "id": m.id,
        "name": format!("{} → {}", m.vm_name, m.target_host),
        "vm_name": m.vm_name,
        "status": migration_status_str(&m.state),
        "error": m.error,
        "started_at": m.started.to_rfc3339(),
        "completed_at": m.completed.map(|t| t.to_rfc3339()),
        "duration": migration_duration(m),
        "output_path": null
    })
}

pub async fn migration_history(
    RequireRead(_claims): RequireRead,
    State(state): State<Arc<AppState>>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let migrations: Vec<MigrationStatus> =
        state.store.list_entities("migrations").map_err(store_err)?;
    let history: Vec<serde_json::Value> = migrations.iter().map(migration_entry).collect();
    Ok(Json(json!({ "history": history })))
}

pub async fn migration_readiness(
    RequireRead(_claims): RequireRead,
    State(state): State<Arc<AppState>>,
) -> Json<serde_json::Value> {
    let mut checks = Vec::new();
    let rsync = tokio::process::Command::new("which")
        .arg("rsync")
        .output()
        .await
        .map(|o| o.status.success())
        .unwrap_or(false);
    checks.push(json!({
        "name": "rsync",
        "status": if rsync { "ok" } else { "error" },
        "message": if rsync { "rsync is available" } else { "rsync not found on PATH" },
    }));
    let qemu = tokio::process::Command::new("which")
        .arg("qemu-img")
        .output()
        .await
        .map(|o| o.status.success())
        .unwrap_or(false);
    checks.push(json!({
        "name": "qemu-img",
        "status": if qemu { "ok" } else { "warning" },
        "message": if qemu { "qemu-img is available" } else { "qemu-img not found" },
    }));
    let vm_count = state.store.list_vms().map(|v| v.len()).unwrap_or(0);
    checks.push(json!({
        "name": "VM inventory",
        "status": if vm_count > 0 { "ok" } else { "warning" },
        "message": format!("{} VM(s) registered", vm_count),
    }));
    let active: Vec<MigrationStatus> = state
        .store
        .list_entities::<MigrationStatus>("migrations")
        .unwrap_or_default()
        .into_iter()
        .filter(|m| {
            matches!(
                m.state,
                MigrationState::Pending
                    | MigrationState::PreCheck
                    | MigrationState::Syncing
                    | MigrationState::Switching
            )
        })
        .collect();
    checks.push(json!({
        "name": "Active migrations",
        "status": if active.len() < 5 { "ok" } else { "warning" },
        "message": format!("{} migration(s) in progress", active.len()),
    }));
    Json(json!({ "checks": checks }))
}

pub async fn migration_report(
    RequireRead(_claims): RequireRead,
    State(state): State<Arc<AppState>>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let migrations: Vec<MigrationStatus> =
        state.store.list_entities("migrations").map_err(store_err)?;
    let successful = migrations
        .iter()
        .filter(|m| m.state == MigrationState::Completed)
        .count();
    let failed = migrations
        .iter()
        .filter(|m| m.state == MigrationState::Failed)
        .count();
    let running = migrations
        .iter()
        .filter(|m| {
            matches!(
                m.state,
                MigrationState::Pending
                    | MigrationState::PreCheck
                    | MigrationState::Syncing
                    | MigrationState::Switching
            )
        })
        .count();
    let total_secs: i64 = migrations
        .iter()
        .filter_map(|m| m.completed.map(|c| (c - m.started).num_seconds()))
        .sum();
    let completed_count = migrations.iter().filter(|m| m.completed.is_some()).count();
    let avg_secs = if completed_count > 0 {
        total_secs / completed_count as i64
    } else {
        0
    };
    let avg_duration = if avg_secs < 60 {
        format!("{}s", avg_secs)
    } else {
        format!("{}m", avg_secs / 60)
    };
    let entries: Vec<serde_json::Value> = migrations.iter().map(migration_entry).collect();
    Ok(Json(json!({
        "total": migrations.len(),
        "successful": successful,
        "failed": failed,
        "running": running,
        "avg_duration": avg_duration,
        "migrations": entries,
        "timestamp": Utc::now().to_rfc3339()
    })))
}

// ============================================================================
// Image upload & convert
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ConvertJob {
    id: String,
    status: String,
    progress: u32,
    source_path: String,
    output_path: String,
    format: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<String>,
    /// Offline boot-readiness score (0-100) from GuestKit's `doctor` analysis
    /// -- this project doesn't use qemu-guest-agent for in-guest/image work,
    /// GuestKit's offline disk inspection is the tool for that. Only set for
    /// golden-image jobs (see create_image_from_vm); None for plain format
    /// conversions and if the guestkit binary isn't available.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    boot_score: Option<f64>,
}

#[derive(Debug, Deserialize)]
pub struct ConvertRequest {
    source_path: String,
    output_path: String,
    format: String,
}

pub async fn upload_image(
    RequireWrite(_claims): RequireWrite,
    mut multipart: Multipart,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let mut dest_dir = PathBuf::from("/var/lib/zyvor-fabricd/images");
    let mut file_name: Option<String> = None;
    let mut file_data: Option<Vec<u8>> = None;

    while let Some(field) = multipart
        .next_field()
        .await
        .map_err(|e| bad_request(e.to_string()))?
    {
        let name = field.name().unwrap_or("").to_string();
        if name == "dest_dir" {
            let dir = field.text().await.map_err(|e| bad_request(e.to_string()))?;
            if !dir.is_empty() && dir.starts_with('/') && !dir.contains("..") {
                dest_dir = PathBuf::from(dir);
            }
        } else if name == "file" {
            file_name = field.file_name().map(|s| s.to_string());
            file_data = Some(
                field
                    .bytes()
                    .await
                    .map_err(|e| bad_request(e.to_string()))?
                    .to_vec(),
            );
        }
    }

    let data = file_data.ok_or_else(|| bad_request("No file uploaded"))?;
    let original_name = file_name.unwrap_or_else(|| "upload.img".to_string());
    if original_name.contains("..") || original_name.contains('/') {
        return Err(bad_request("Invalid file name"));
    }

    tokio::fs::create_dir_all(&dest_dir)
        .await
        .map_err(|e| internal_err(e.to_string()))?;
    let dest_path = dest_dir.join(&original_name);
    let mut f = tokio::fs::File::create(&dest_path)
        .await
        .map_err(|e| internal_err(e.to_string()))?;
    f.write_all(&data)
        .await
        .map_err(|e| internal_err(e.to_string()))?;

    let ext = dest_path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("img")
        .to_string();
    let size_bytes = data.len() as u64;
    let stem = dest_path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("upload")
        .to_string();

    Ok(Json(json!({
        "name": stem,
        "path": dest_path.display().to_string(),
        "size_bytes": size_bytes,
        "format": ext
    })))
}

pub async fn start_convert(
    RequireWrite(_claims): RequireWrite,
    State(state): State<Arc<AppState>>,
    Json(req): Json<ConvertRequest>,
) -> Result<(StatusCode, Json<serde_json::Value>), (StatusCode, Json<serde_json::Value>)> {
    if req.source_path.is_empty() || req.output_path.is_empty() {
        return Err(bad_request("source_path and output_path are required"));
    }
    if req.source_path.contains("..") || req.output_path.contains("..") {
        return Err(bad_request("Invalid path"));
    }
    let job_id = Uuid::new_v4().to_string();
    let job = ConvertJob {
        id: job_id.clone(),
        status: "pending".into(),
        progress: 0,
        source_path: req.source_path.clone(),
        output_path: req.output_path.clone(),
        format: req.format.clone(),
        error: None,
        boot_score: None,
    };
    state
        .store
        .save_entity(CONVERT_JOBS_STORE, &job_id, &job)
        .map_err(store_err)?;

    let state_clone = state.clone();
    tokio::spawn(async move {
        run_convert_job(state_clone, job_id, req).await;
    });

    Ok((
        StatusCode::ACCEPTED,
        Json(json!({ "job_id": job.id, "id": job.id })),
    ))
}

async fn run_convert_job(state: Arc<AppState>, job_id: String, req: ConvertRequest) {
    let update = |status: &str, progress: u32, error: Option<String>| {
        if let Ok(Some(mut job)) = state
            .store
            .get_entity::<ConvertJob>(CONVERT_JOBS_STORE, &job_id)
        {
            job.status = status.into();
            job.progress = progress;
            job.error = error;
            let _ = state.store.save_entity(CONVERT_JOBS_STORE, &job_id, &job);
        }
    };
    update("running", 10, None);
    let fmt = match req.format.as_str() {
        "qcow2" => "qcow2",
        "vmdk" => "vmdk",
        "vhd" => "vpc",
        "vhdx" => "vhdx",
        "raw" => "raw",
        _ => {
            update("failed", 100, Some("Unsupported format".into()));
            return;
        }
    };
    // No -f flag: qemu-img auto-probes the source format on its own.
    // "auto" is not a real format driver name -- passing "-f auto"
    // fails with "Unknown driver 'auto'".
    let output = tokio::process::Command::new("qemu-img")
        .args(["convert", "-O", fmt, &req.source_path, &req.output_path])
        .output()
        .await;
    match output {
        Ok(o) if o.status.success() => update("completed", 100, None),
        Ok(o) => {
            let err = String::from_utf8_lossy(&o.stderr).to_string();
            update("failed", 100, Some(err));
        }
        Err(e) => update("failed", 100, Some(e.to_string())),
    }
}

pub async fn get_convert_job(
    RequireRead(_claims): RequireRead,
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let job = state
        .store
        .get_entity::<ConvertJob>(CONVERT_JOBS_STORE, &id)
        .map_err(store_err)?
        .ok_or_else(|| not_found("Job not found"))?;
    Ok(Json(json!({
        "id": job.id,
        "status": job.status,
        "progress": job.progress,
        "error": job.error,
        "output_path": job.output_path,
        "boot_score": job.boot_score
    })))
}

// ============================================================================
// Golden images (materialize a VM's current disk as a standalone catalog image)
// ============================================================================

#[derive(Debug, Deserialize)]
pub struct CreateGoldenImageRequest {
    pub name: String,
}

/// POST /api/images/from-vm/:name - Materialize a VM's current disk as a new,
/// independent catalog image (not a CoW fork -- the source VM can be deleted
/// or changed afterward without affecting the new image). Runs as an async
/// job via the same qemu-img convert pipeline as /images/convert.
pub async fn create_image_from_vm(
    RequireWrite(_claims): RequireWrite,
    State(state): State<Arc<AppState>>,
    Path(vm_name): Path<String>,
    Json(req): Json<CreateGoldenImageRequest>,
) -> Result<(StatusCode, Json<serde_json::Value>), (StatusCode, Json<serde_json::Value>)> {
    crate::validation::validate_vm_name(&vm_name)
        .map_err(|(s, m)| crate::api_error::json_error(s, m))?;
    crate::validation::validate_vm_name(&req.name)
        .map_err(|(s, m)| crate::api_error::json_error(s, m))?;

    // The VM's actual, live disk (not a naming-convention guess -- see
    // VMDriver::get_disk_path's doc comment).
    let source_path = state
        .driver
        .get_disk_path(&vm_name)
        .await
        .map_err(|e| not_found(format!("No disk image found for VM '{}': {}", vm_name, e)))?
        .display()
        .to_string();

    // Configurable via storage.image_path in the daemon config (same
    // directory the catalog listing at GET /images scans), not hardcoded.
    let dest_dir = state
        .config
        .storage
        .image_path
        .trim_end_matches('/')
        .to_string();
    if dest_dir.contains("..") {
        return Err(crate::api_error::json_error(
            StatusCode::BAD_REQUEST,
            "invalid image directory",
        ));
    }
    let name = input_guard::vet_component!(
        &req.name,
        crate::api_error::json_error(StatusCode::BAD_REQUEST, "invalid image name")
    );
    tokio::fs::create_dir_all(&dest_dir)
        .await
        .map_err(|e| internal_err(format!("Failed to create directory: {}", e)))?;
    let output_path = format!("{dest_dir}/{name}.qcow2");

    if tokio::fs::metadata(&output_path).await.is_ok() {
        return Err(crate::api_error::json_error(
            StatusCode::CONFLICT,
            format!("An image named '{}' already exists", req.name),
        ));
    }

    let job_id = Uuid::new_v4().to_string();
    let job = ConvertJob {
        id: job_id.clone(),
        status: "pending".into(),
        progress: 0,
        source_path: source_path.clone(),
        output_path: output_path.clone(),
        format: "qcow2".into(),
        error: None,
        boot_score: None,
    };
    state
        .store
        .save_entity(CONVERT_JOBS_STORE, &job_id, &job)
        .map_err(store_err)?;

    let state_clone = state.clone();
    let convert_req = ConvertRequest {
        source_path,
        output_path,
        format: "qcow2".into(),
    };
    let job_id_clone = job_id.clone();
    tokio::spawn(async move {
        run_convert_job(state_clone.clone(), job_id_clone.clone(), convert_req).await;
        // Bake in the GuestKit in-guest agent, then certify the image with
        // GuestKit's offline `doctor` analysis (boot-readiness score) --
        // this project doesn't use qemu-guest-agent for guest/image work,
        // GuestKit is the tool for that. Both steps are best-effort: if the
        // vendor binaries aren't present or either step fails, the golden
        // image is still created, it just misses that step.
        if let Ok(Some(completed_job)) = state_clone
            .store
            .get_entity::<ConvertJob>(CONVERT_JOBS_STORE, &job_id_clone)
        {
            if completed_job.status == "completed" {
                inject_guest_agent(&completed_job.output_path).await;
                score_golden_image(&state_clone, &job_id_clone, &completed_job.output_path).await;
            }
        }
    });

    tracing::info!("Creating golden image '{}' from VM '{}'", req.name, vm_name);
    Ok((
        StatusCode::ACCEPTED,
        Json(json!({ "job_id": job.id, "id": job.id })),
    ))
}

/// Vendor location for the pre-built GuestKit in-guest agent artifacts --
/// not part of this project's own build (they come from the separate
/// guestkit repo: a musl-static `zyvor-guest-agent` binary for the guest,
/// built with `cargo build --release --bin zyvor-guest-agent --features
/// agent --no-default-features --target x86_64-unknown-linux-musl`; and a
/// native `guestkit` CLI *also* built with `--features agent`, since the
/// offline-only CLI normally installed system-wide refuses `agent-inject`
/// without that feature). Both are placed here manually for now.
const GUESTKIT_AGENT_CLI: &str = "/var/lib/zyvor-fabricd/vendor/guestkit-agent-cli";
const GUESTKIT_AGENT_BINARY: &str = "/var/lib/zyvor-fabricd/vendor/zyvor-guest-agent";

/// Offline-injects the GuestKit in-guest agent (systemd unit + binary) into
/// a newly materialized golden image, so VMs booted from it are reachable
/// over the agent's virtio-serial channel without depending on network
/// access at boot (unlike a cloud-init curl install). No-ops quietly if
/// the vendor binaries aren't present on this host.
async fn inject_guest_agent(image_path: &str) {
    if !tokio::fs::try_exists(GUESTKIT_AGENT_CLI)
        .await
        .unwrap_or(false)
        || !tokio::fs::try_exists(GUESTKIT_AGENT_BINARY)
            .await
            .unwrap_or(false)
    {
        tracing::debug!("GuestKit agent vendor binaries not present, skipping agent injection");
        return;
    }
    let output = tokio::process::Command::new(GUESTKIT_AGENT_CLI)
        .args([
            "agent-inject",
            image_path,
            "--agent-binary",
            GUESTKIT_AGENT_BINARY,
            "-q",
        ])
        .output()
        .await;
    match output {
        Ok(o) if o.status.success() => {
            tracing::info!("Injected GuestKit agent into '{}'", image_path)
        }
        Ok(o) => tracing::warn!(
            "guestkit agent-inject failed for '{}': {}",
            image_path,
            String::from_utf8_lossy(&o.stderr)
        ),
        Err(e) => tracing::debug!(
            "guestkit-agent-cli not runnable, skipping agent injection: {}",
            e
        ),
    }
}

/// Runs `guestkit doctor` (read-only) against a newly materialized golden
/// image and stores the boot-readiness score on its job entity.
async fn score_golden_image(state: &Arc<AppState>, job_id: &str, image_path: &str) {
    let output = tokio::process::Command::new("guestkit")
        .args([
            "doctor", image_path, "--target", "kvm", "-o", "json", "-R", "-q",
        ])
        .output()
        .await;

    let score = match output {
        Ok(o) if o.status.success() => serde_json::from_slice::<serde_json::Value>(&o.stdout)
            .ok()
            .and_then(|v| v.get("bootability")?.get("score")?.as_f64()),
        Ok(o) => {
            tracing::warn!(
                "guestkit doctor exited non-zero for '{}': {}",
                image_path,
                String::from_utf8_lossy(&o.stderr)
            );
            None
        }
        Err(e) => {
            tracing::debug!(
                "guestkit not available, skipping boot-readiness score: {}",
                e
            );
            None
        }
    };

    if let Some(score) = score {
        if let Ok(Some(mut job)) = state
            .store
            .get_entity::<ConvertJob>(CONVERT_JOBS_STORE, job_id)
        {
            job.boot_score = Some(score);
            if let Err(e) = state.store.save_entity(CONVERT_JOBS_STORE, job_id, &job) {
                tracing::warn!("Failed to save boot score: {}", e);
            }
        }
    }
}

// ============================================================================
// Cost estimate
// ============================================================================

#[derive(Debug, Deserialize)]
pub struct CostEstimateRequest {
    vm_count: u32,
    avg_size_gb: f64,
    include_snapshots: bool,
    duration_months: u32,
}

pub async fn cost_estimate(
    RequireRead(_claims): RequireRead,
    State(state): State<Arc<AppState>>,
    Json(req): Json<CostEstimateRequest>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let pricing = state
        .store
        .get_entity::<billing::PricingRule>("billing", "default")
        .map_err(store_err)?
        .unwrap_or_default();

    let multiplier = if req.include_snapshots { 1.5 } else { 1.0 };
    let total_gb = req.vm_count as f64 * req.avg_size_gb * multiplier;

    let providers = [
        ("AWS S3", 0.023, "bg-orange-500"),
        ("Azure Blob", 0.018, "bg-blue-500"),
        ("GCS", 0.020, "bg-red-500"),
    ];
    let estimates: Vec<serde_json::Value> = providers
        .iter()
        .map(|(name, price, color)| {
            let monthly = total_gb * price;
            json!({
                "name": name,
                "pricePerGB": price,
                "color": color,
                "monthly": monthly,
                "annual": monthly * 12.0,
                "total": monthly * req.duration_months as f64
            })
        })
        .collect();

    // Flat $0.10/GB/month baseline — matches the rate the UI labels this
    // figure with. This estimates on-premises hardware TCO, which has
    // nothing to do with the tenant's own configured cloud storage billing
    // rate (pricing.storage_gb_per_hour): mixing the two in previously via
    // max() let a tenant's own billing rate silently outweigh the labeled
    // $0.10/GB/mo figure, showing a number the label didn't match.
    let on_prem = total_gb * 0.10;
    Ok(Json(json!({
        "estimates": estimates,
        "on_prem_monthly": on_prem,
        "pricing_source": pricing.name,
        "currency": pricing.currency
    })))
}

// ============================================================================
// Service map & network topology
// ============================================================================

pub async fn service_map(
    RequireRead(_claims): RequireRead,
    State(state): State<Arc<AppState>>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let services: Vec<Service> = state.store.list_entities("services").map_err(store_err)?;
    let vms = state.store.list_vms().map_err(store_err)?;
    let mut nodes = Vec::new();
    let mut links = Vec::new();

    for svc in &services {
        let status = if svc.enabled { "healthy" } else { "degraded" };
        nodes.push(json!({
            "name": svc.name,
            "type": "api",
            "vm": svc.selector.match_labels.get("vm").cloned().unwrap_or_else(|| "host".into()),
            "port": svc.ports.first().map(|p| p.port).unwrap_or(0),
            "status": status
        }));
    }
    for vm in &vms {
        nodes.push(json!({
            "name": vm.name.clone(),
            "type": "vm",
            "vm": vm.name,
            "status": format!("{:?}", vm.state).to_lowercase()
        }));
    }
    for svc in &services {
        if let Some(vm) = svc.selector.match_labels.get("vm") {
            links.push(json!({
                "from": svc.name,
                "to": vm,
                "protocol": "tcp",
                "port": svc.ports.first().map(|p| p.port).unwrap_or(80)
            }));
        }
    }

    Ok(Json(json!({
        "nodes": nodes,
        "links": links,
        "timestamp": Utc::now().to_rfc3339()
    })))
}

pub async fn network_topology(
    RequireRead(_claims): RequireRead,
    State(state): State<Arc<AppState>>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let vms = state.store.list_vms().map_err(store_err)?;
    let networks: Vec<serde_json::Value> =
        vec![json!({ "name": "default", "state": "active", "autostart": "yes" })];
    let mut vm_nodes = Vec::new();

    for vm in vms {
        let iface = json!({
            "type": "network",
            "source": "default",
            "model": "virtio",
            "mac": vm.mac_address.clone().unwrap_or_else(|| "52:54:00:00:00:00".into())
        });
        vm_nodes.push(json!({
            "name": vm.name,
            "state": format!("{:?}", vm.state).to_lowercase(),
            "interfaces": [iface]
        }));
    }

    Ok(Json(json!({
        "vms": vm_nodes,
        "networks": networks
    })))
}

// ============================================================================
// Helpers
// ============================================================================

fn store_err(e: impl std::fmt::Display) -> (StatusCode, Json<serde_json::Value>) {
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        Json(json!({ "error": e.to_string() })),
    )
}

fn bad_request(msg: impl Into<String>) -> (StatusCode, Json<serde_json::Value>) {
    (
        StatusCode::BAD_REQUEST,
        Json(json!({ "error": msg.into() })),
    )
}

fn not_found(msg: impl Into<String>) -> (StatusCode, Json<serde_json::Value>) {
    (StatusCode::NOT_FOUND, Json(json!({ "error": msg.into() })))
}

fn internal_err(msg: impl Into<String>) -> (StatusCode, Json<serde_json::Value>) {
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        Json(json!({ "error": msg.into() })),
    )
}
