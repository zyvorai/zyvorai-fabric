// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: LicenseRef-Zyvor-Production-2.0

//! Fabric AI Workloads — Inference plane (Beta on a single cluster).
//!
//! REST resources for model artifacts, inference profiles, deployments, and
//! OpenAI-compatible endpoints backed by QEMU GPU VMs via FluxVM. Multi-site
//! HA store remains Preview; APIs may still change before GA.
//!
//! Golden image: bake CUDA + vLLM into a qcow2; cloud-init only writes the
//! systemd unit that points vLLM at the mounted model path. Set
//! `FLUXVM_AI_IMAGE` to that qcow2. For model materialization without a
//! Hugging Face download, set `FLUXVM_AI_MODEL_DIR` to a verified local tree.

pub mod autoscaling;
pub mod backup;
pub mod batch;
pub mod capacity;
pub mod circuit;
pub mod deployments;
pub mod eligibility;
pub mod endpoints;
pub mod explain;
pub mod federation;
pub mod finops;
pub mod gateway;
pub mod gpu_orch;
pub mod gpu_probe;
pub mod gpus;
pub mod ipam;
pub mod janus;
pub mod judge;
pub mod keys;
pub mod lifecycle;
pub mod limits;
pub mod maglev;
pub mod model_cache;
pub mod model_jobs;
pub mod models;
pub mod nodes;
pub mod otel;
pub mod pci_mig;
pub mod placement;
pub mod policy;
pub mod profiles;
pub mod raft;
pub mod reconcile;
pub mod revisions;
pub mod rollouts;
pub mod router;
pub mod routing;
pub mod runtime;
pub mod scheduler;
pub mod sites;
pub mod supply;
pub mod traces;
pub mod types;
pub mod upstream;

pub(crate) const STORE_CIRCUITS: &str = "ai_gateway_circuits";
pub(crate) const STORE_RATE_COUNTERS: &str = "ai_rate_counters";
pub(crate) const STORE_REPLICATIONS: &str = "ai_model_replications";

use crate::server::AppState;
use zyvor_fabric_fluxvm_client::FluxVmClient;

pub(crate) const STORE_MODELS: &str = "ai_model_artifacts";
pub(crate) const STORE_PROFILES: &str = "ai_inference_profiles";
pub(crate) const STORE_DEPLOYMENTS: &str = "ai_inference_deployments";
pub(crate) const STORE_ENDPOINTS: &str = "ai_inference_endpoints";
pub(crate) const STORE_REVISIONS: &str = "ai_deployment_revisions";
pub(crate) const STORE_REPLICA_SETS: &str = "ai_replica_sets";
pub(crate) const STORE_ROLLOUTS: &str = "ai_rollouts";
pub(crate) const STORE_MODEL_JOBS: &str = "ai_model_jobs";
pub(crate) const STORE_NODES: &str = "ai_inference_nodes";
pub(crate) const STORE_SITES: &str = "ai_sites";
pub(crate) const STORE_POLICIES: &str = "ai_tenant_policies";
pub(crate) const STORE_EDGE: &str = "ai_edge_snapshots";

pub(crate) fn fluxvm_client(
    state: &AppState,
) -> Result<FluxVmClient, (axum::http::StatusCode, axum::Json<serde_json::Value>)> {
    let client = FluxVmClient::new(&state.config.driver.fluxvm_url).map_err(|e| {
        (
            axum::http::StatusCode::BAD_GATEWAY,
            axum::Json(serde_json::json!({ "error": format!("FluxVM client: {e}") })),
        )
    })?;
    Ok(match state.config.driver.fluxvm_token.as_deref() {
        Some(t) if !t.is_empty() => client.with_token(t.to_owned()),
        _ => client,
    })
}

pub(crate) fn audit(state: &AppState, user: &str, action: &str, resource: &str, status: &str) {
    let entry = security::AuditLog::new(
        user.to_string(),
        action.to_string(),
        resource.to_string(),
        status.to_string(),
    );
    if let Err(e) = entry.log() {
        tracing::warn!("Failed to write audit log: {}", e);
    }
    if let Err(e) = state.store.save_entity("audit_logs", &entry.id, &entry) {
        tracing::warn!("Failed to persist audit log: {}", e);
    }
    record_audit_chain(state, &format!("{user}|{action}|{resource}|{status}"));
}

#[derive(Clone, serde::Serialize, serde::Deserialize)]
struct AuditChain {
    prev: String,
}

pub fn chain_hash(prev: &str, line: &str) -> String {
    use sha2::{Digest, Sha256};
    let digest = Sha256::digest(format!("{prev}\n{line}").as_bytes());
    digest.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn record_audit_chain(state: &AppState, line: &str) {
    if raft::peers_configured() {
        let line = line.to_string();
        tokio::spawn(async move {
            match raft::counter_home() {
                raft::CounterHome::Local => {
                    let _ = raft::write_audit_link(line).await;
                }
                raft::CounterHome::Leader(addr) => {
                    let _ = remote_audit_link(&addr, line).await;
                }
                raft::CounterHome::Unavailable => {}
            }
        });
        return;
    }
    const STORE: &str = "ai_audit_chain";
    const ID: &str = "ai";
    if state
        .store
        .get_entity::<AuditChain>(STORE, ID)
        .ok()
        .flatten()
        .is_none()
    {
        let _ = state.store.save_entity(
            STORE,
            ID,
            &AuditChain {
                prev: String::new(),
            },
        );
    }
    let line = line.to_string();
    let _ = state
        .store
        .update_entity_exclusive(STORE, ID, |mut link: AuditChain| {
            link.prev = chain_hash(&link.prev, &line);
            Ok::<_, String>(link)
        });
}

async fn remote_audit_link(addr: &str, line: String) -> Result<(), String> {
    let token = std::env::var("FLUXVM_AI_RAFT_TOKEN").unwrap_or_default();
    let url = format!("http://{addr}/api/ai/raft/limits");
    let client = reqwest::Client::new();
    let response = client
        .post(url)
        .header("x-raft-token", token)
        .json(&raft::LeaseCommand::AuditLink { line })
        .send()
        .await
        .map_err(|err| err.to_string())?;
    if !response.status().is_success() {
        return Err(format!("raft audit returned {}", response.status()));
    }
    Ok(())
}

pub(crate) fn err(
    status: axum::http::StatusCode,
    msg: impl Into<String>,
) -> (axum::http::StatusCode, axum::Json<serde_json::Value>) {
    (
        status,
        axum::Json(serde_json::json!({ "error": msg.into() })),
    )
}

#[cfg(test)]
mod tests {
    use super::chain_hash;

    #[test]
    fn audit_chain_depends_on_the_previous_hash() {
        let first = chain_hash("", "create|model");
        let second = chain_hash(&first, "create|model");
        assert_ne!(first, second);
        assert_eq!(first, chain_hash("", "create|model"));
    }
}
