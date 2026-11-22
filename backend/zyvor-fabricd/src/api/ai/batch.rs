// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: LicenseRef-Zyvor-Production-2.0

//! Control-plane batch jobs.
//!
//! A job stays queued until a ready replica exists, then running until a
//! worker reports the outcome. Fabric does not mark it succeeded on its own
//! and does not store the prompt.

use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use chrono::Utc;
use security::{RequireRead, RequireWrite};
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::sync::Arc;

use crate::server::AppState;

use super::types::InferenceDeployment;
use super::{audit, err, STORE_DEPLOYMENTS};

pub const STORE_BATCHES: &str = "ai_batches";

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BatchState {
    Queued,
    Running,
    Succeeded,
    Failed,
    Cancelled,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct BatchJob {
    pub id: String,
    pub deployment: String,
    pub input_sha256: String,
    pub state: BatchState,
    #[serde(default)]
    pub message: String,
    pub created_unix: i64,
    pub updated_unix: i64,
}

#[derive(Debug, Deserialize)]
pub struct CreateBatchRequest {
    pub deployment: String,
    pub input_sha256: String,
}

#[derive(Debug, Deserialize)]
pub struct FinishBatchRequest {
    pub ok: bool,
    #[serde(default)]
    pub message: String,
}

pub fn digest_sha256(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

pub fn valid_digest(value: &str) -> bool {
    value.len() == 64 && value.chars().all(|c| c.is_ascii_hexdigit())
}

pub fn submit(id: String, deployment: String, input_sha256: String, now_unix: i64) -> BatchJob {
    BatchJob {
        id,
        deployment,
        input_sha256,
        state: BatchState::Queued,
        message: String::new(),
        created_unix: now_unix,
        updated_unix: now_unix,
    }
}

pub fn claim(mut job: BatchJob, ready: bool, now_unix: i64) -> Result<BatchJob, &'static str> {
    if job.state != BatchState::Queued {
        return Err("batch is not queued");
    }
    if !ready {
        return Err("no ready inference replica");
    }
    job.state = BatchState::Running;
    job.updated_unix = now_unix;
    job.message = "claimed".into();
    Ok(job)
}

pub fn finish(
    mut job: BatchJob,
    ok: bool,
    message: String,
    now_unix: i64,
) -> Result<BatchJob, &'static str> {
    if job.state != BatchState::Running {
        return Err("batch is not running");
    }
    job.state = if ok {
        BatchState::Succeeded
    } else {
        BatchState::Failed
    };
    job.message = message;
    job.updated_unix = now_unix;
    Ok(job)
}

pub fn cancel(mut job: BatchJob, now_unix: i64) -> Result<BatchJob, &'static str> {
    if matches!(
        job.state,
        BatchState::Succeeded | BatchState::Failed | BatchState::Cancelled
    ) {
        return Err("batch is already finished");
    }
    job.state = BatchState::Cancelled;
    job.updated_unix = now_unix;
    job.message = "cancelled".into();
    Ok(job)
}

pub async fn create_batch(
    RequireWrite(claims): RequireWrite,
    State(state): State<Arc<AppState>>,
    Json(req): Json<CreateBatchRequest>,
) -> Result<(StatusCode, Json<BatchJob>), (StatusCode, Json<serde_json::Value>)> {
    if !valid_digest(&req.input_sha256) {
        return Err(err(
            StatusCode::BAD_REQUEST,
            "input_sha256 must be 64 hex characters",
        ));
    }
    if state
        .store
        .get_entity::<InferenceDeployment>(STORE_DEPLOYMENTS, &req.deployment)
        .map_err(|e| err(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
        .is_none()
    {
        return Err(err(StatusCode::BAD_REQUEST, "deployment not found"));
    }
    let now = Utc::now().timestamp();
    let id = format!("batch_{}", &uuid::Uuid::new_v4().simple().to_string()[..12]);
    let job = submit(
        id,
        req.deployment,
        req.input_sha256.to_ascii_lowercase(),
        now,
    );
    state
        .store
        .save_entity(STORE_BATCHES, &job.id, &job)
        .map_err(|e| err(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    audit(
        &state,
        &claims.sub,
        "CREATE",
        &format!("ai/batches/{}", job.id),
        "SUCCESS",
    );
    Ok((StatusCode::CREATED, Json(job)))
}

pub async fn list_batches(
    RequireRead(_claims): RequireRead,
    State(state): State<Arc<AppState>>,
) -> Result<Json<Vec<BatchJob>>, (StatusCode, Json<serde_json::Value>)> {
    let mut jobs: Vec<BatchJob> = state
        .store
        .list_entities(STORE_BATCHES)
        .map_err(|e| err(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    jobs.sort_by(|a, b| a.id.cmp(&b.id));
    Ok(Json(jobs))
}

pub async fn get_batch(
    RequireRead(_claims): RequireRead,
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<BatchJob>, (StatusCode, Json<serde_json::Value>)> {
    state
        .store
        .get_entity(STORE_BATCHES, &id)
        .map_err(|e| err(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
        .ok_or_else(|| err(StatusCode::NOT_FOUND, "batch not found"))
        .map(Json)
}

pub async fn claim_batch(
    RequireWrite(claims): RequireWrite,
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<BatchJob>, (StatusCode, Json<serde_json::Value>)> {
    let job = load(&state, &id)?;
    let dep = state
        .store
        .get_entity::<InferenceDeployment>(STORE_DEPLOYMENTS, &job.deployment)
        .map_err(|e| err(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
        .ok_or_else(|| err(StatusCode::BAD_REQUEST, "deployment not found"))?;
    let ready = dep
        .status
        .replicas
        .iter()
        .any(|replica| replica.ready && !replica.draining);
    let next =
        claim(job, ready, Utc::now().timestamp()).map_err(|e| err(StatusCode::CONFLICT, e))?;
    state
        .store
        .save_entity(STORE_BATCHES, &next.id, &next)
        .map_err(|e| err(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    audit(
        &state,
        &claims.sub,
        "CLAIM",
        &format!("ai/batches/{}", next.id),
        "SUCCESS",
    );
    Ok(Json(next))
}

pub async fn finish_batch(
    RequireWrite(claims): RequireWrite,
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    Json(req): Json<FinishBatchRequest>,
) -> Result<Json<BatchJob>, (StatusCode, Json<serde_json::Value>)> {
    let job = load(&state, &id)?;
    let next = finish(job, req.ok, req.message, Utc::now().timestamp())
        .map_err(|e| err(StatusCode::CONFLICT, e))?;
    state
        .store
        .save_entity(STORE_BATCHES, &next.id, &next)
        .map_err(|e| err(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    audit(
        &state,
        &claims.sub,
        "FINISH",
        &format!("ai/batches/{}", next.id),
        "SUCCESS",
    );
    Ok(Json(next))
}

pub async fn cancel_batch(
    RequireWrite(claims): RequireWrite,
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<BatchJob>, (StatusCode, Json<serde_json::Value>)> {
    let job = load(&state, &id)?;
    let next = cancel(job, Utc::now().timestamp()).map_err(|e| err(StatusCode::CONFLICT, e))?;
    state
        .store
        .save_entity(STORE_BATCHES, &next.id, &next)
        .map_err(|e| err(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    audit(
        &state,
        &claims.sub,
        "CANCEL",
        &format!("ai/batches/{}", next.id),
        "SUCCESS",
    );
    Ok(Json(next))
}

fn load(state: &AppState, id: &str) -> Result<BatchJob, (StatusCode, Json<serde_json::Value>)> {
    state
        .store
        .get_entity(STORE_BATCHES, id)
        .map_err(|e| err(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
        .ok_or_else(|| err(StatusCode::NOT_FOUND, "batch not found"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn claim_requires_a_ready_replica_and_finish_is_explicit() {
        let job = submit("batch_1".into(), "qwen".into(), "ab".repeat(32), 10);
        assert!(claim(job.clone(), false, 11).is_err());
        let running = claim(job, true, 11).unwrap();
        assert_eq!(running.state, BatchState::Running);
        assert!(
            finish(running.clone(), true, "ok".into(), 12)
                .unwrap()
                .state
                == BatchState::Succeeded
        );
        assert!(cancel(running, 12).is_ok());
        let done = finish(
            claim(
                submit("batch_2".into(), "qwen".into(), "cd".repeat(32), 1),
                true,
                2,
            )
            .unwrap(),
            false,
            "upstream".into(),
            3,
        )
        .unwrap();
        assert_eq!(done.state, BatchState::Failed);
        assert!(finish(done, true, "again".into(), 4).is_err());
        assert!(valid_digest(&digest_sha256(b"prompt")));
        assert!(!valid_digest("nope"));
    }
}
