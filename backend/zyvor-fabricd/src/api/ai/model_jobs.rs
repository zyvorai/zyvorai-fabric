// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: LicenseRef-Zyvor-Production-2.0

//! Background, content-addressed model materialization.
//!
//! HTTP handlers only record a job. This loop downloads, resumes a partial
//! tree, and lets a second job for the same digest join the first.

use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use chrono::Utc;
use security::{RequireRead, RequireWrite};
use serde::Deserialize;
use std::path::{Path as FsPath, PathBuf};
use std::sync::Arc;
use subtle::ConstantTimeEq;

use crate::server::AppState;

use super::model_cache::{self, sha256_file};
use super::types::{ModelArtifact, ModelJob, ModelJobState};
use super::{audit, err, STORE_MODELS, STORE_MODEL_JOBS};

pub fn source_is_local(source: &str) -> bool {
    std::env::var("FLUXVM_AI_MODEL_DIR").is_ok()
        || source.starts_with("file://")
        || source.starts_with('/')
}

/// Bytes already on disk. A later download continues from this offset.
pub fn resume_offset(path: &FsPath) -> u64 {
    if path.is_file() {
        return std::fs::metadata(path).map(|m| m.len()).unwrap_or(0);
    }
    if path.is_dir() {
        return dir_bytes(path);
    }
    0
}

fn dir_bytes(path: &FsPath) -> u64 {
    let mut total = 0u64;
    let Ok(entries) = std::fs::read_dir(path) else {
        return 0;
    };
    for entry in entries.flatten() {
        let p = entry.path();
        if p.is_dir() {
            total = total.saturating_add(dir_bytes(&p));
        } else if let Ok(meta) = std::fs::metadata(&p) {
            total = total.saturating_add(meta.len());
        }
    }
    total
}

/// Refuse the job when free space is below the declared object size.
pub fn admit_disk(free: u64, declared: Option<u64>) -> Result<(), String> {
    if let Some(need) = declared {
        if free < need {
            return Err(format!("free disk {free} is below declared size {need}"));
        }
    }
    Ok(())
}

/// Another in-flight or ready job for the same digest or source+revision.
pub fn find_join<'a>(jobs: &'a [ModelJob], job: &ModelJob) -> Option<&'a ModelJob> {
    let digest = job.digest.as_deref().or(job.checksum.as_deref());
    if let Some(digest) = digest {
        if let Some(other) = jobs.iter().find(|other| {
            other.id != job.id
                && other.digest.as_deref().or(other.checksum.as_deref()) == Some(digest)
                && matches!(
                    other.state,
                    ModelJobState::Downloading
                        | ModelJobState::Verifying
                        | ModelJobState::Scanning
                        | ModelJobState::Optimizing
                        | ModelJobState::Ready
                )
        }) {
            return Some(other);
        }
    }
    jobs.iter().find(|other| {
        other.id != job.id
            && other.source == job.source
            && other.revision == job.revision
            && other.created_at <= job.created_at
            && matches!(
                other.state,
                ModelJobState::Resolving
                    | ModelJobState::Downloading
                    | ModelJobState::Verifying
                    | ModelJobState::Scanning
                    | ModelJobState::Optimizing
                    | ModelJobState::Ready
            )
    })
}

pub fn register(state: &AppState, model: &ModelArtifact) -> Result<ModelJob, String> {
    let existing: Vec<ModelJob> = state
        .store
        .list_entities(STORE_MODEL_JOBS)
        .unwrap_or_default();
    let mut job = ModelJob {
        id: format!("{}--{}", model.name, uuid::Uuid::new_v4().simple()),
        model: model.name.clone(),
        source: model.source.clone(),
        revision: model.revision.clone(),
        checksum: model.checksum.clone(),
        state: ModelJobState::Registered,
        digest: model.checksum.clone(),
        message: None,
        retries: 0,
        bytes_written: 0,
        joined_job: None,
        local_path: None,
        optimize: model.optimize.clone(),
        derived_digest: None,
        created_at: Utc::now(),
        updated_at: Utc::now(),
    };
    if let Some(other) = find_join(&existing, &job) {
        job.joined_job = Some(other.id.clone());
        job.digest = other.digest.clone().or(job.digest);
        job.state = if other.state == ModelJobState::Ready {
            ModelJobState::Ready
        } else {
            ModelJobState::Resolving
        };
        job.message = Some(format!("joined {}", other.id));
        if other.state == ModelJobState::Ready {
            copy_ready_model(state, &job.model, other)?;
        }
    }
    state
        .store
        .save_entity(STORE_MODEL_JOBS, &job.id, &job)
        .map_err(|e| e.to_string())?;
    Ok(job)
}

fn copy_ready_model(state: &AppState, model_name: &str, other: &ModelJob) -> Result<(), String> {
    let Some(mut model) = state
        .store
        .get_entity::<ModelArtifact>(STORE_MODELS, model_name)
        .map_err(|e| e.to_string())?
    else {
        return Ok(());
    };
    if let Some(source) = state
        .store
        .get_entity::<ModelArtifact>(STORE_MODELS, &other.model)
        .ok()
        .flatten()
    {
        model.local_path = source.local_path;
        model.checksum = model.checksum.or(source.checksum).or(other.digest.clone());
        model.updated = Utc::now();
        state
            .store
            .save_entity(STORE_MODELS, &model.name, &model)
            .map_err(|e| e.to_string())?;
    }
    Ok(())
}

/// Advance one job a single state. Safe to call again after a restart.
pub fn drive_job(state: &AppState, id: &str) {
    let Ok(Some(mut job)) = state.store.get_entity::<ModelJob>(STORE_MODEL_JOBS, id) else {
        return;
    };
    if matches!(job.state, ModelJobState::Ready | ModelJobState::Failed) {
        return;
    }
    let result = step(state, &mut job);
    if let Err(msg) = result {
        job.retries = job.retries.saturating_add(1);
        job.message = Some(msg);
        job.state = if job.retries < 3 {
            ModelJobState::Registered
        } else {
            ModelJobState::Failed
        };
    }
    job.updated_at = Utc::now();
    let _ = state.store.save_entity(STORE_MODEL_JOBS, &job.id, &job);
}

fn step(state: &AppState, job: &mut ModelJob) -> Result<(), String> {
    match job.state {
        ModelJobState::Registered | ModelJobState::Resolving => {
            job.state = ModelJobState::Resolving;
            let jobs: Vec<ModelJob> = state
                .store
                .list_entities(STORE_MODEL_JOBS)
                .unwrap_or_default();
            if let Some(other) = find_join(&jobs, job) {
                job.joined_job = Some(other.id.clone());
                job.digest = other.digest.clone().or(job.digest.clone());
                job.message = Some(format!("joined {}", other.id));
                if other.state == ModelJobState::Ready {
                    copy_ready_model(state, &job.model, other)?;
                    job.state = ModelJobState::Ready;
                }
                return Ok(());
            }
            let state_dir = PathBuf::from(&state.config.storage.path);
            let declared = state
                .store
                .get_entity::<ModelArtifact>(STORE_MODELS, &job.model)
                .ok()
                .flatten()
                .and_then(|m| m.size_bytes);
            let free = free_bytes(&state_dir)?;
            admit_disk(free, declared)?;
            job.state = ModelJobState::Downloading;
            Ok(())
        }
        ModelJobState::Downloading => {
            let state_dir = PathBuf::from(&state.config.storage.path);
            let dest_name = job
                .digest
                .clone()
                .filter(|d| !d.is_empty())
                .unwrap_or_else(|| format!("partial-{}", job.id));
            let partial = state_dir
                .join("ai-models")
                .join(model_cache::sanitize_name(&dest_name));
            job.bytes_written = resume_offset(&partial);
            let (path, verified) = model_cache::materialize_model(
                &dest_name,
                &job.source,
                job.checksum.as_deref(),
                job.revision.as_deref(),
                &state_dir,
            )?;
            job.bytes_written = resume_offset(&path);
            if let Some(hex) = verified.or_else(|| {
                if path.is_file() {
                    sha256_file(&path).ok()
                } else {
                    None
                }
            }) {
                job.digest = Some(hex);
            }
            job.state = ModelJobState::Verifying;
            job.message = Some(path.to_string_lossy().into_owned());
            Ok(())
        }
        ModelJobState::Verifying => {
            let path = job
                .message
                .clone()
                .ok_or_else(|| "model job has no downloaded path".to_string())?;
            let verified = model_cache::verify_checksum_if_requested(
                FsPath::new(&path),
                job.checksum.as_deref(),
            )?;
            let scan = model_cache::scan_model_format(FsPath::new(&path))?;
            if !scan.blocked.is_empty() {
                return Err(format!("unsafe model file: {}", scan.blocked.join(", ")));
            }
            if let Some(hex) = verified.or(job.digest.clone()) {
                job.digest = Some(hex);
            }
            job.local_path = Some(path);
            job.state = ModelJobState::Scanning;
            job.message = Some("scanning".into());
            Ok(())
        }
        ModelJobState::Scanning => {
            let path = job
                .local_path
                .clone()
                .ok_or_else(|| "model job has no downloaded path".to_string())?;
            let scan = model_cache::scan_model_format(FsPath::new(&path))?;
            if !scan.blocked.is_empty() {
                return Err(format!("unsafe model format: {}", scan.blocked.join(", ")));
            }
            super::supply::file_count_allowed(scan.files, super::supply::max_files())?;
            if let Some(model) = state
                .store
                .get_entity::<ModelArtifact>(STORE_MODELS, &job.model)
                .map_err(|e| e.to_string())?
            {
                if let Some(signature) = model.signature.as_deref() {
                    super::supply::require_signature(job.digest.as_deref(), signature)?;
                }
            }
            if job.optimize.as_deref().is_some_and(|kind| !kind.is_empty()) {
                job.state = ModelJobState::Optimizing;
                job.message = Some("optimizing".into());
            } else {
                publish_ready(state, job)?;
            }
            Ok(())
        }
        ModelJobState::Optimizing => {
            let kind = super::supply::optimization_kind(job.optimize.as_deref().unwrap_or(""))?;
            let source = job
                .digest
                .clone()
                .filter(|d| !d.is_empty())
                .ok_or_else(|| "optimization requires a source digest".to_string())?;
            let derived = super::supply::derived_digest(&source, kind);
            write_derived_artifact(state, job, kind, &derived)?;
            job.derived_digest = Some(derived);
            publish_ready(state, job)?;
            Ok(())
        }
        ModelJobState::Ready | ModelJobState::Failed => Ok(()),
    }
}

fn publish_ready(state: &AppState, job: &mut ModelJob) -> Result<(), String> {
    if let Some(mut model) = state
        .store
        .get_entity::<ModelArtifact>(STORE_MODELS, &job.model)
        .map_err(|e| e.to_string())?
    {
        model.local_path = job.local_path.clone();
        model.checksum = job.digest.clone().or(model.checksum);
        model.updated = Utc::now();
        state
            .store
            .save_entity(STORE_MODELS, &model.name, &model)
            .map_err(|e| e.to_string())?;
    }
    job.state = ModelJobState::Ready;
    job.message = Some("ready".into());
    Ok(())
}

fn write_derived_artifact(
    state: &AppState,
    job: &ModelJob,
    kind: &str,
    derived: &str,
) -> Result<(), String> {
    let name = format!("{}--{kind}", job.model);
    let now = Utc::now();
    let mut artifact = state
        .store
        .get_entity::<ModelArtifact>(STORE_MODELS, &name)
        .map_err(|e| e.to_string())?
        .unwrap_or(ModelArtifact {
            name: name.clone(),
            source: job.source.clone(),
            revision: job.revision.clone(),
            checksum: None,
            format: kind.to_string(),
            size_bytes: None,
            tenant: None,
            local_path: job.local_path.clone(),
            license: None,
            residency: None,
            require_checksum: true,
            signature: None,
            optimize: None,
            derived_from: Some(job.model.clone()),
            created: now,
            updated: now,
        });
    artifact.checksum = Some(derived.to_string());
    artifact.local_path = job.local_path.clone();
    artifact.derived_from = Some(job.model.clone());
    artifact.updated = now;
    state
        .store
        .save_entity(STORE_MODELS, &name, &artifact)
        .map_err(|e| e.to_string())
}

fn free_bytes(path: &FsPath) -> Result<u64, String> {
    let dir = if path.is_dir() {
        path.to_path_buf()
    } else {
        path.parent().unwrap_or(path).to_path_buf()
    };
    if !dir.exists() {
        std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    }
    let c = std::ffi::CString::new(dir.to_string_lossy().as_bytes().to_vec())
        .map_err(|_| "model dir path contains a nul".to_string())?;
    let mut stat: libc::statvfs = unsafe { std::mem::zeroed() };
    let rc = unsafe { libc::statvfs(c.as_ptr(), &mut stat) };
    if rc != 0 {
        return Err(format!(
            "statvfs {}: {}",
            dir.display(),
            std::io::Error::last_os_error()
        ));
    }
    Ok((stat.f_bavail as u64).saturating_mul(stat.f_frsize as u64))
}

pub async fn run_model_job_controller(state: Arc<AppState>) {
    let mut interval = tokio::time::interval(std::time::Duration::from_secs(5));
    loop {
        interval.tick().await;
        let jobs: Vec<ModelJob> = state
            .store
            .list_entities(STORE_MODEL_JOBS)
            .unwrap_or_default();
        for job in jobs {
            if matches!(job.state, ModelJobState::Ready | ModelJobState::Failed) {
                continue;
            }
            let state = state.clone();
            let id = job.id.clone();
            let _ = tokio::task::spawn_blocking(move || drive_job(&state, &id)).await;
        }
    }
}

/// Drive a local or stub source to a terminal state before the handler returns.
pub fn drive_until_terminal(state: &AppState, id: &str) -> Option<ModelJob> {
    for _ in 0..8 {
        drive_job(state, id);
        if let Ok(Some(job)) = state.store.get_entity::<ModelJob>(STORE_MODEL_JOBS, id) {
            if matches!(job.state, ModelJobState::Ready | ModelJobState::Failed) {
                return Some(job);
            }
        }
    }
    state.store.get_entity(STORE_MODEL_JOBS, id).ok().flatten()
}

/// POST /api/ai/models/{name}/materialize
pub async fn materialize(
    RequireWrite(_claims): RequireWrite,
    State(state): State<Arc<AppState>>,
    Path(name): Path<String>,
) -> Result<(StatusCode, Json<ModelJob>), (StatusCode, Json<serde_json::Value>)> {
    let model = state
        .store
        .get_entity::<ModelArtifact>(STORE_MODELS, &name)
        .map_err(|e| err(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
        .ok_or_else(|| err(StatusCode::NOT_FOUND, "ModelArtifact not found"))?;
    let job = register(&state, &model).map_err(|e| err(StatusCode::INTERNAL_SERVER_ERROR, e))?;
    audit(
        &state,
        "model-job",
        "CREATE",
        &format!("ai/model-jobs/{}", job.id),
        "SUCCESS",
    );
    Ok((StatusCode::ACCEPTED, Json(job)))
}

/// GET /api/ai/model-jobs
pub async fn list_jobs(
    RequireRead(_claims): RequireRead,
    State(state): State<Arc<AppState>>,
) -> Result<Json<Vec<ModelJob>>, (StatusCode, Json<serde_json::Value>)> {
    let mut jobs: Vec<ModelJob> = state
        .store
        .list_entities(STORE_MODEL_JOBS)
        .map_err(|e| err(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    jobs.sort_by_key(|job| job.created_at);
    Ok(Json(jobs))
}

/// GET /api/ai/models/{name}/status
pub async fn model_status(
    RequireRead(_claims): RequireRead,
    State(state): State<Arc<AppState>>,
    Path(name): Path<String>,
) -> Result<Json<ModelJob>, (StatusCode, Json<serde_json::Value>)> {
    let mut jobs: Vec<ModelJob> = state
        .store
        .list_entities(STORE_MODEL_JOBS)
        .map_err(|e| err(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    jobs.retain(|j| j.model == name);
    jobs.sort_by_key(|job| job.created_at);
    jobs.pop()
        .ok_or_else(|| err(StatusCode::NOT_FOUND, "no model job for this artifact"))
        .map(Json)
}

/// POST /api/ai/models/{name}/verify
///
/// Re-checks the stored checksum and refuses pickle weights. This does not
/// verify a signature.
pub async fn verify_model(
    RequireRead(_claims): RequireRead,
    State(state): State<Arc<AppState>>,
    Path(name): Path<String>,
) -> Result<(StatusCode, Json<serde_json::Value>), (StatusCode, Json<serde_json::Value>)> {
    let model = state
        .store
        .get_entity::<ModelArtifact>(STORE_MODELS, &name)
        .map_err(|e| err(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
        .ok_or_else(|| err(StatusCode::NOT_FOUND, "ModelArtifact not found"))?;
    let path = model
        .local_path
        .as_deref()
        .ok_or_else(|| err(StatusCode::BAD_REQUEST, "model has no local_path"))?;
    let path = FsPath::new(path);
    let state_dir = PathBuf::from(&state.config.storage.path);
    let path =
        model_cache::confine(path, &state_dir).map_err(|e| err(StatusCode::BAD_REQUEST, e))?;
    if let Err(e) = model_cache::verify_checksum_if_requested(&path, model.checksum.as_deref()) {
        return Err(err(StatusCode::BAD_REQUEST, e));
    }
    let scan =
        model_cache::scan_model_format(&path).map_err(|e| err(StatusCode::BAD_REQUEST, e))?;
    let ok = scan.blocked.is_empty();
    let status = if ok {
        StatusCode::OK
    } else {
        StatusCode::BAD_REQUEST
    };
    Ok((
        status,
        Json(serde_json::json!({
            "name": name,
            "ok": ok,
            "safetensors": scan.safetensors,
            "files": scan.files,
            "blocked": scan.blocked,
        })),
    ))
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ReplicationRecord {
    pub id: String,
    pub model: String,
    pub site: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub digest: Option<String>,
    pub state: String,
}

#[derive(Debug, Deserialize)]
pub struct ReplicateRequest {
    pub site: String,
}

/// POST /api/ai/models/{name}/replicate
///
/// Records replication to a site. The record is ready when a node at that
/// site already lists the model, or when the site `peer_url` accepts the
/// digest bytes and returns 201. A missing peer, file, or token stays pending.
pub async fn replicate(
    RequireWrite(_claims): RequireWrite,
    State(state): State<Arc<AppState>>,
    Path(name): Path<String>,
    Json(body): Json<ReplicateRequest>,
) -> Result<(StatusCode, Json<ReplicationRecord>), (StatusCode, Json<serde_json::Value>)> {
    crate::validation::validate_entity_name(&body.site).map_err(|(s, m)| err(s, m))?;
    let model = state
        .store
        .get_entity::<ModelArtifact>(STORE_MODELS, &name)
        .map_err(|e| err(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
        .ok_or_else(|| err(StatusCode::NOT_FOUND, "ModelArtifact not found"))?;
    let nodes: Vec<super::types::InferenceNode> = state
        .store
        .list_entities(super::STORE_NODES)
        .unwrap_or_default();
    let cached = nodes.iter().any(|node| {
        node.site == body.site && node.cached_models.iter().any(|cached| cached == &name)
    });
    let copied = if cached {
        false
    } else {
        copy_to_peer(&state, &model, &body.site).await
    };
    let record = ReplicationRecord {
        id: format!("{name}--{}", body.site),
        model: name,
        site: body.site,
        digest: model.checksum,
        state: if cached || copied { "ready" } else { "pending" }.into(),
    };
    state
        .store
        .save_entity(super::STORE_REPLICATIONS, &record.id, &record)
        .map_err(|e| err(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    Ok((StatusCode::ACCEPTED, Json(record)))
}

async fn copy_to_peer(state: &AppState, model: &ModelArtifact, site_id: &str) -> bool {
    let Ok(token) = std::env::var("FLUXVM_AI_REPLICATE_TOKEN") else {
        return false;
    };
    if token.is_empty() {
        return false;
    }
    let Some(raw) = model
        .checksum
        .as_deref()
        .filter(|value| valid_digest(value))
    else {
        return false;
    };
    let digest = raw.to_ascii_lowercase();
    let Ok(Some(site)) = state
        .store
        .get_entity::<super::federation::AiSite>(super::STORE_SITES, site_id)
    else {
        return false;
    };
    let peer = site.peer_url.trim().trim_end_matches('/');
    if peer.is_empty() {
        return false;
    }
    let Some(path) = local_digest_file(state, model, &digest) else {
        return false;
    };
    let Ok(got) = sha256_file(&path) else {
        return false;
    };
    if got != digest {
        return false;
    }
    let Ok(file) = tokio::fs::File::open(&path).await else {
        return false;
    };
    let url = format!("{peer}/api/ai/models/{}/blobs/{digest}", model.name);
    let stream = tokio_util::io::ReaderStream::new(file);
    let response = state
        .http_client
        .put(url)
        .header("x-replicate-token", token)
        .body(reqwest::Body::wrap_stream(stream))
        .send()
        .await;
    let Ok(response) = response else {
        return false;
    };
    if response.status() != reqwest::StatusCode::CREATED {
        return false;
    }
    let Ok(body) = response.json::<serde_json::Value>().await else {
        return false;
    };
    body.get("digest").and_then(|value| value.as_str()) == Some(digest.as_str())
}

fn local_digest_file(state: &AppState, model: &ModelArtifact, digest: &str) -> Option<PathBuf> {
    let state_dir = PathBuf::from(&state.config.storage.path);
    let candidates = [
        model.local_path.clone().map(PathBuf::from),
        Some(
            state_dir
                .join("ai-models")
                .join(model_cache::sanitize_name(&model.name)),
        ),
        Some(state_dir.join("ai-models").join("blobs").join(digest)),
    ];
    candidates.into_iter().flatten().find(|path| path.is_file())
}

pub fn valid_digest(value: &str) -> bool {
    value.len() == 64 && value.chars().all(|c| c.is_ascii_hexdigit())
}

/// PUT /api/ai/models/{name}/blobs/{digest}
pub async fn receive_blob(
    State(state): State<Arc<AppState>>,
    Path((name, digest)): Path<(String, String)>,
    headers: axum::http::HeaderMap,
    body: axum::body::Body,
) -> Result<(StatusCode, Json<serde_json::Value>), (StatusCode, Json<serde_json::Value>)> {
    if !replicate_token_ok(
        headers
            .get("x-replicate-token")
            .and_then(|value| value.to_str().ok()),
    ) {
        return Err(err(StatusCode::UNAUTHORIZED, "replicate token is required"));
    }
    if !valid_digest(&digest) {
        return Err(err(
            StatusCode::BAD_REQUEST,
            "digest must be 64 hex characters",
        ));
    }
    crate::validation::validate_entity_name(&name).map_err(|(s, m)| err(s, m))?;
    let state_dir = PathBuf::from(&state.config.storage.path);
    let root = state_dir.join("ai-models").join("blobs");
    std::fs::create_dir_all(&root)
        .map_err(|e| err(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    let root_s = root.to_string_lossy();
    let partial_path = format!("{root_s}/.{digest}.partial");
    let dest_path = format!("{root_s}/{digest}");
    // CodeQL path-injection barrier: filesystem ops only inside fs_checked!.
    input_guard::fs_checked!(
        partial_path,
        err(StatusCode::BAD_REQUEST, "invalid blob path"),
        |partial_path| {
            input_guard::fs_checked!(
                dest_path,
                err(StatusCode::BAD_REQUEST, "invalid blob path"),
                |dest_path| {
                    if let Err(error) =
                        write_hashed_body(body, FsPath::new(&partial_path), &digest).await
                    {
                        let _ = std::fs::remove_file(&partial_path);
                        return Err(err(StatusCode::BAD_REQUEST, error));
                    }
                    std::fs::rename(&partial_path, &dest_path)
                        .map_err(|e| err(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
                    if model_cache::confine(FsPath::new(&dest_path), &state_dir).is_err() {
                        let _ = std::fs::remove_file(&dest_path);
                        return Err(err(
                            StatusCode::BAD_REQUEST,
                            "blob path escaped the model directory",
                        ));
                    }
                    if let Ok(Some(mut model)) =
                        state.store.get_entity::<ModelArtifact>(STORE_MODELS, &name)
                    {
                        if model.local_path.is_none() {
                            model.local_path = Some(dest_path.clone());
                            let _ = state.store.save_entity(STORE_MODELS, &name, &model);
                        }
                    }
                    Ok((
                        StatusCode::CREATED,
                        Json(serde_json::json!({ "digest": digest.to_ascii_lowercase() })),
                    ))
                }
            )
        }
    )
}

fn replicate_token_ok(got: Option<&str>) -> bool {
    let Ok(expected) = std::env::var("FLUXVM_AI_REPLICATE_TOKEN") else {
        return false;
    };
    let Some(got) = got else {
        return false;
    };
    expected.len() == got.len() && bool::from(expected.as_bytes().ct_eq(got.as_bytes()))
}

async fn write_hashed_body(
    body: axum::body::Body,
    path: &FsPath,
    digest: &str,
) -> Result<(), String> {
    use futures::StreamExt;
    use sha2::{Digest, Sha256};
    use tokio::io::AsyncWriteExt;
    let path_s = path.to_string_lossy().into_owned();
    let digest = digest.to_ascii_lowercase();
    // CodeQL path-injection barrier at the File::create sink.
    input_guard::fs_checked!(path_s, "invalid blob path".to_string(), |path_s| {
        async move {
            let mut file = tokio::fs::File::create(&path_s)
                .await
                .map_err(|err| err.to_string())?;
            let mut hasher = Sha256::new();
            let mut total = 0u64;
            let mut stream = body.into_data_stream();
            while let Some(chunk) = stream.next().await {
                let chunk = chunk.map_err(|err| err.to_string())?;
                total = total.saturating_add(chunk.len() as u64);
                if total > 8 * 1024 * 1024 * 1024 {
                    return Err("model blob exceeds 8 GiB".into());
                }
                hasher.update(&chunk);
                file.write_all(&chunk)
                    .await
                    .map_err(|err| err.to_string())?;
            }
            file.flush().await.map_err(|err| err.to_string())?;
            let got: String = hasher
                .finalize()
                .iter()
                .map(|byte| format!("{byte:02x}"))
                .collect();
            if got != digest {
                return Err("blob digest does not match".into());
            }
            Ok(())
        }
    })
    .await
}

/// DELETE /api/ai/models/{name}/cache/{node}
pub async fn delete_node_cache(
    RequireWrite(_claims): RequireWrite,
    State(state): State<Arc<AppState>>,
    Path((name, node)): Path<(String, String)>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let deployments: Vec<super::types::InferenceDeployment> = state
        .store
        .list_entities(super::STORE_DEPLOYMENTS)
        .unwrap_or_default();
    let referenced = deployments.iter().any(|dep| {
        dep.model == name
            && dep
                .status
                .replicas
                .iter()
                .any(|rep| rep.host == node || (node == "local" && rep.host.is_empty()))
    });
    if referenced {
        return Err(err(
            StatusCode::CONFLICT,
            "model cache is referenced by a running replica",
        ));
    }
    if node != "local" {
        let mut stored = state
            .store
            .get_entity::<super::types::InferenceNode>(super::STORE_NODES, &node)
            .map_err(|e| err(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
            .ok_or_else(|| err(StatusCode::NOT_FOUND, "inference node not found"))?;
        stored.cached_models.retain(|cached| cached != &name);
        state
            .store
            .save_entity(super::STORE_NODES, &node, &stored)
            .map_err(|e| err(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    }
    let mut removed = false;
    let local = node == "local"
        || std::env::var("FLUXVM_AI_NODE_ID")
            .ok()
            .is_some_and(|id| id == node);
    if local {
        if let Some(mut model) = state
            .store
            .get_entity::<ModelArtifact>(STORE_MODELS, &name)
            .ok()
            .flatten()
        {
            if let Some(path) = model.local_path.clone() {
                let path = FsPath::new(&path);
                let state_dir = PathBuf::from(&state.config.storage.path);
                if model_cache::is_managed_cache(path, &state_dir) {
                    if path.is_dir() {
                        let _ = std::fs::remove_dir_all(path);
                    } else {
                        let _ = std::fs::remove_file(path);
                    }
                    removed = true;
                }
            }
            model.local_path = None;
            model.updated = Utc::now();
            let _ = state.store.save_entity(STORE_MODELS, &name, &model);
        }
    }
    Ok(Json(serde_json::json!({
        "model": name,
        "node": node,
        "bytes_removed": removed,
    })))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn job(id: &str, digest: Option<&str>, state: ModelJobState) -> ModelJob {
        ModelJob {
            id: id.into(),
            model: "m".into(),
            source: "hf://org/model".into(),
            revision: Some("main".into()),
            checksum: digest.map(str::to_string),
            state,
            digest: digest.map(str::to_string),
            message: None,
            retries: 0,
            bytes_written: 0,
            joined_job: None,
            local_path: None,
            optimize: None,
            derived_digest: None,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        }
    }

    #[test]
    fn disk_admission_refuses_a_short_volume() {
        assert!(admit_disk(10, Some(11)).is_err());
        assert!(admit_disk(11, Some(11)).is_ok());
        assert!(admit_disk(0, None).is_ok());
    }

    #[test]
    fn second_job_joins_the_same_digest() {
        let first = job("a", Some("abc"), ModelJobState::Downloading);
        let second = job("b", Some("abc"), ModelJobState::Registered);
        let jobs = [first];
        let joined = find_join(&jobs, &second).unwrap();
        assert_eq!(joined.id, "a");
    }

    #[test]
    fn resume_offset_is_the_partial_file_length() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("partial.bin");
        std::fs::write(&file, b"abcdef").unwrap();
        assert_eq!(resume_offset(&file), 6);
        assert_eq!(resume_offset(&dir.path().join("missing")), 0);
    }
}
