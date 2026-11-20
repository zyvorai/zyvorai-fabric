// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: LicenseRef-Zyvor-Production-1.0

//! Inference node inventory. Scheduling reads these records; a missing list
//! keeps placement on the local FluxVM host.

use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use chrono::Utc;
use security::{RequireRead, RequireWrite};
use std::sync::Arc;

use crate::server::AppState;

use super::types::{CreateNodeRequest, InferenceNode, NodeState};
use super::{audit, err, STORE_NODES};

/// GET /api/ai/nodes
pub async fn list_nodes(
    RequireRead(_claims): RequireRead,
    State(state): State<Arc<AppState>>,
) -> Result<Json<Vec<InferenceNode>>, (StatusCode, Json<serde_json::Value>)> {
    let mut nodes: Vec<InferenceNode> = state
        .store
        .list_entities(STORE_NODES)
        .map_err(|e| err(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    nodes.sort_by(|a, b| a.id.cmp(&b.id));
    Ok(Json(nodes))
}

/// GET /api/ai/nodes/{id}
pub async fn get_node(
    RequireRead(_claims): RequireRead,
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<InferenceNode>, (StatusCode, Json<serde_json::Value>)> {
    state
        .store
        .get_entity(STORE_NODES, &id)
        .map_err(|e| err(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
        .ok_or_else(|| err(StatusCode::NOT_FOUND, "inference node not found"))
        .map(Json)
}

/// POST /api/ai/nodes
pub async fn create_node(
    RequireWrite(claims): RequireWrite,
    State(state): State<Arc<AppState>>,
    Json(req): Json<CreateNodeRequest>,
) -> Result<(StatusCode, Json<InferenceNode>), (StatusCode, Json<serde_json::Value>)> {
    crate::validation::validate_entity_name(&req.id).map_err(|(s, m)| err(s, m))?;
    if state
        .store
        .get_entity::<InferenceNode>(STORE_NODES, &req.id)
        .ok()
        .flatten()
        .is_some()
    {
        return Err(err(
            StatusCode::CONFLICT,
            format!("inference node '{}' already exists", req.id),
        ));
    }
    let node = InferenceNode {
        id: req.id,
        site: req.site,
        failure_domain: req.failure_domain,
        state: NodeState::Ready,
        heartbeat_unix: Utc::now().timestamp(),
        gpus: req.gpus,
        taints: req.taints,
        cpu_free: req.cpu_free,
        memory_gib_free: req.memory_gib_free,
        cached_models: req.cached_models,
    };
    state
        .store
        .save_entity(STORE_NODES, &node.id, &node)
        .map_err(|e| err(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    audit(
        &state,
        &claims.sub,
        "CREATE",
        &format!("ai/nodes/{}", node.id),
        "SUCCESS",
    );
    Ok((StatusCode::CREATED, Json(node)))
}

/// POST /api/ai/nodes/{id}/heartbeat
pub async fn heartbeat(
    RequireWrite(_claims): RequireWrite,
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<InferenceNode>, (StatusCode, Json<serde_json::Value>)> {
    let mut node = state
        .store
        .get_entity::<InferenceNode>(STORE_NODES, &id)
        .map_err(|e| err(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
        .ok_or_else(|| err(StatusCode::NOT_FOUND, "inference node not found"))?;
    node.heartbeat_unix = Utc::now().timestamp();
    if matches!(node.state, NodeState::Joining | NodeState::Offline) {
        node.state = NodeState::Ready;
    }
    if let Some(text) = super::gpu_probe::query_nvidia_smi() {
        let samples = super::gpu_probe::parse_nvidia_smi(&text);
        super::gpu_probe::apply_samples(&mut node.gpus, &samples);
    }
    state
        .store
        .save_entity(STORE_NODES, &node.id, &node)
        .map_err(|e| err(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    Ok(Json(node))
}

#[derive(Debug, serde::Deserialize)]
pub struct MigRequest {
    pub profile: String,
}

fn allocated_bdfs(state: &AppState) -> Vec<(String, String)> {
    let deployments: Vec<super::types::InferenceDeployment> = state
        .store
        .list_entities(super::STORE_DEPLOYMENTS)
        .unwrap_or_default();
    deployments
        .iter()
        .flat_map(|dep| dep.status.replicas.iter())
        .filter(|rep| !rep.bdf.is_empty())
        .map(|rep| (rep.host.clone(), rep.bdf.clone()))
        .collect()
}

fn set_gpu_health(
    state: &AppState,
    id: &str,
    bdf: &str,
    healthy: bool,
) -> Result<Json<InferenceNode>, (StatusCode, Json<serde_json::Value>)> {
    let mut node = state
        .store
        .get_entity::<InferenceNode>(STORE_NODES, id)
        .map_err(|e| err(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
        .ok_or_else(|| err(StatusCode::NOT_FOUND, "inference node not found"))?;
    let gpu = node
        .gpus
        .iter_mut()
        .find(|gpu| gpu.bdf.eq_ignore_ascii_case(bdf))
        .ok_or_else(|| err(StatusCode::NOT_FOUND, "GPU not found on node"))?;
    gpu.healthy = healthy;
    state
        .store
        .save_entity(STORE_NODES, id, &node)
        .map_err(|e| err(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    Ok(Json(node))
}

/// POST /api/ai/nodes/{id}/gpus/{bdf}/quarantine
pub async fn quarantine_gpu(
    RequireWrite(_claims): RequireWrite,
    State(state): State<Arc<AppState>>,
    Path((id, bdf)): Path<(String, String)>,
) -> Result<Json<InferenceNode>, (StatusCode, Json<serde_json::Value>)> {
    set_gpu_health(&state, &id, &bdf, false)
}

/// POST /api/ai/nodes/{id}/gpus/{bdf}/restore
pub async fn restore_gpu(
    RequireWrite(_claims): RequireWrite,
    State(state): State<Arc<AppState>>,
    Path((id, bdf)): Path<(String, String)>,
) -> Result<Json<InferenceNode>, (StatusCode, Json<serde_json::Value>)> {
    set_gpu_health(&state, &id, &bdf, true)
}

/// POST /api/ai/nodes/{id}/gpus/{bdf}/mig
pub async fn set_mig(
    RequireWrite(_claims): RequireWrite,
    State(state): State<Arc<AppState>>,
    Path((id, bdf)): Path<(String, String)>,
    Json(body): Json<MigRequest>,
) -> Result<Json<InferenceNode>, (StatusCode, Json<serde_json::Value>)> {
    if !super::gpu_orch::mig_reconfigure_allowed(&bdf, &allocated_bdfs(&state)) {
        return Err(err(
            StatusCode::CONFLICT,
            "MIG profile cannot change while the GPU is allocated",
        ));
    }
    let mut node = state
        .store
        .get_entity::<InferenceNode>(STORE_NODES, &id)
        .map_err(|e| err(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
        .ok_or_else(|| err(StatusCode::NOT_FOUND, "inference node not found"))?;
    let gpu = node
        .gpus
        .iter_mut()
        .find(|gpu| gpu.bdf.eq_ignore_ascii_case(&bdf))
        .ok_or_else(|| err(StatusCode::NOT_FOUND, "GPU not found on node"))?;
    gpu.mig_profile = body.profile;
    state
        .store
        .save_entity(STORE_NODES, &id, &node)
        .map_err(|e| err(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    Ok(Json(node))
}

#[derive(Debug, serde::Deserialize)]
pub struct CreateMigSlice {
    pub parent_bdf: String,
    pub profile: String,
}

/// POST /api/ai/nodes/{id}/mig
///
/// Janus BDFs create inventory records only. PCI BDFs require
/// `FLUXVM_AI_PCI_MIG=1` and run `nvidia-smi mig` unless
/// `FLUXVM_AI_PCI_MIG_RECORD_ONLY=1`.
pub async fn create_mig_slice(
    RequireWrite(_claims): RequireWrite,
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    Json(body): Json<CreateMigSlice>,
) -> Result<Json<InferenceNode>, (StatusCode, Json<serde_json::Value>)> {
    let profile = super::gpu_orch::janus_mig_profile(&body.profile)
        .ok_or_else(|| err(StatusCode::BAD_REQUEST, "unknown MIG profile"))?;
    let janus = super::janus::is_janus_bdf(&body.parent_bdf);
    let pci = super::pci_mig::is_pci_bdf(&body.parent_bdf);
    if !janus && !pci {
        return Err(err(
            StatusCode::BAD_REQUEST,
            "parent_bdf must be a Janus id or a PCI BDF",
        ));
    }
    if pci && !super::pci_mig::pci_mig_enabled() {
        return Err(err(
            StatusCode::BAD_REQUEST,
            "PCI MIG create requires FLUXVM_AI_PCI_MIG=1",
        ));
    }
    if !super::gpu_orch::mig_reconfigure_allowed(&body.parent_bdf, &allocated_bdfs(&state)) {
        return Err(err(
            StatusCode::CONFLICT,
            "MIG profile cannot change while the GPU is allocated",
        ));
    }
    let mut node = state
        .store
        .get_entity::<InferenceNode>(STORE_NODES, &id)
        .map_err(|e| err(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
        .ok_or_else(|| err(StatusCode::NOT_FOUND, "inference node not found"))?;
    let parent = node
        .gpus
        .iter()
        .find(|gpu| gpu.bdf == body.parent_bdf && gpu.parent_bdf.is_empty())
        .cloned()
        .ok_or_else(|| err(StatusCode::NOT_FOUND, "parent GPU not found"))?;
    if janus && !parent.model.starts_with("janus:") {
        return Err(err(
            StatusCode::BAD_REQUEST,
            "MIG create is only available for a Janus GPU",
        ));
    }
    let existing = node
        .gpus
        .iter()
        .filter(|gpu| gpu.parent_bdf == body.parent_bdf && gpu.mig_profile == profile.name)
        .count() as u32;
    if !super::gpu_orch::slice_fits(parent.vram_gib, profile, existing) {
        return Err(err(
            StatusCode::CONFLICT,
            "MIG slice does not fit the parent GPU",
        ));
    }
    if pci {
        super::pci_mig::apply_create(&body.parent_bdf, profile)
            .map_err(|e| err(StatusCode::BAD_GATEWAY, e))?;
    }
    node.gpus.push(super::types::NodeGpu {
        bdf: super::gpu_orch::slice_bdf(&body.parent_bdf, profile.name, existing),
        vendor: parent.vendor.clone(),
        vram_gib: profile.memory_gib,
        model: parent.model.clone(),
        healthy: true,
        mig_profile: profile.name.to_string(),
        parent_bdf: body.parent_bdf,
        nvlink: false,
        temperature_c: 0,
        power_watts: 0,
        ecc_errors: 0,
    });
    state
        .store
        .save_entity(STORE_NODES, &node.id, &node)
        .map_err(|e| err(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    Ok(Json(node))
}

/// DELETE /api/ai/nodes/{id}/mig/{bdf}
pub async fn delete_mig_slice(
    RequireWrite(_claims): RequireWrite,
    State(state): State<Arc<AppState>>,
    Path((id, bdf)): Path<(String, String)>,
) -> Result<Json<InferenceNode>, (StatusCode, Json<serde_json::Value>)> {
    if allocated_bdfs(&state)
        .iter()
        .any(|(host, held)| host == &id && held.eq_ignore_ascii_case(&bdf))
    {
        return Err(err(StatusCode::CONFLICT, "MIG slice is still allocated"));
    }
    let mut node = state
        .store
        .get_entity::<InferenceNode>(STORE_NODES, &id)
        .map_err(|e| err(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
        .ok_or_else(|| err(StatusCode::NOT_FOUND, "inference node not found"))?;
    let slice = node
        .gpus
        .iter()
        .find(|gpu| gpu.bdf == bdf && !gpu.parent_bdf.is_empty())
        .cloned()
        .ok_or_else(|| err(StatusCode::NOT_FOUND, "MIG slice not found"))?;
    let janus = super::janus::is_janus_bdf(&slice.bdf);
    let pci = super::pci_mig::is_pci_bdf(&slice.parent_bdf);
    if !janus && pci && !super::pci_mig::pci_mig_enabled() {
        return Err(err(
            StatusCode::BAD_REQUEST,
            "PCI MIG delete requires FLUXVM_AI_PCI_MIG=1",
        ));
    }
    if pci && super::pci_mig::pci_mig_enabled() {
        super::pci_mig::apply_destroy(&slice.bdf).map_err(|e| err(StatusCode::BAD_GATEWAY, e))?;
    }
    if !janus && !pci {
        return Err(err(StatusCode::NOT_FOUND, "MIG slice not found"));
    }
    node.gpus.retain(|gpu| gpu.bdf != bdf);
    state
        .store
        .save_entity(STORE_NODES, &node.id, &node)
        .map_err(|e| err(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    Ok(Json(node))
}
