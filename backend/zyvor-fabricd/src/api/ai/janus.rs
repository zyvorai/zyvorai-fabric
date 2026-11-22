// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: LicenseRef-Zyvor-Production-2.0

//! Janus is a simulated GPU inventory and OpenAI-compatible upstream.
//!
//! Devices are labeled `janus:` and are not PCI NVIDIA adapters.

use chrono::Utc;

use crate::server::AppState;

use super::types::{InferenceNode, NodeGpu, NodeState};
use super::STORE_NODES;

#[derive(Debug, serde::Deserialize)]
struct Inventory {
    nodes: Vec<InventoryNode>,
}

#[derive(Debug, serde::Deserialize)]
struct InventoryNode {
    id: String,
    gpus: Vec<InventoryGpu>,
}

#[derive(Debug, serde::Deserialize)]
struct InventoryGpu {
    id: String,
    profile: String,
    memory_gb: u32,
    #[serde(default)]
    mig: bool,
}

pub fn janus_url() -> Option<String> {
    std::env::var("FLUXVM_AI_JANUS_URL")
        .ok()
        .map(|value| value.trim().trim_end_matches('/').to_string())
        .filter(|value| !value.is_empty())
}

/// A display controller is not an inference GPU. Janus is used when none of
/// the host devices are NVIDIA.
pub fn is_nvidia(vendor_id: u16, vendor: &str) -> bool {
    vendor_id == 0x10de || vendor.eq_ignore_ascii_case("nvidia")
}

/// `http://127.0.0.1:30818` becomes `127.0.0.1:30818`.
pub fn replica_hostport(url: &str) -> String {
    url.trim_start_matches("https://")
        .trim_start_matches("http://")
        .trim_end_matches('/')
        .to_string()
}

pub fn is_janus_bdf(bdf: &str) -> bool {
    bdf.starts_with("janus:")
}

pub fn nodes_from_inventory(body: &[u8], now_unix: i64) -> Result<Vec<InferenceNode>, String> {
    let inventory: Inventory = serde_json::from_slice(body).map_err(|err| err.to_string())?;
    let mut nodes = Vec::new();
    for node in inventory.nodes {
        if node.id.is_empty() || node.id.contains('/') || node.id.contains("..") {
            return Err(format!("janus node id '{}' is not usable", node.id));
        }
        let gpus = node
            .gpus
            .into_iter()
            .map(|gpu| NodeGpu {
                bdf: gpu.id,
                vendor: "nvidia".into(),
                vram_gib: gpu.memory_gb,
                model: format!("janus:{}", gpu.profile),
                healthy: true,
                mig_profile: String::new(),
                parent_bdf: String::new(),
                nvlink: gpu.mig,
                temperature_c: 0,
                power_watts: 0,
                ecc_errors: 0,
            })
            .collect();
        nodes.push(InferenceNode {
            id: node.id,
            site: "janus".into(),
            failure_domain: "janus".into(),
            state: NodeState::Ready,
            heartbeat_unix: now_unix,
            gpus,
            taints: Vec::new(),
            cpu_free: 0,
            memory_gib_free: 0,
            cached_models: Vec::new(),
        });
    }
    Ok(nodes)
}

pub async fn sync_nodes(state: &AppState, url: &str) -> Result<(), String> {
    let response = state
        .http_client
        .get(format!("{url}/api/cluster?config=single_gpu"))
        .send()
        .await
        .map_err(|err| err.to_string())?
        .error_for_status()
        .map_err(|err| err.to_string())?;
    let body = response.bytes().await.map_err(|err| err.to_string())?;
    let fresh = nodes_from_inventory(&body, Utc::now().timestamp())?;
    for mut node in fresh {
        if let Some(existing) = state
            .store
            .get_entity::<InferenceNode>(STORE_NODES, &node.id)
            .ok()
            .flatten()
        {
            let slices: Vec<NodeGpu> = existing
                .gpus
                .into_iter()
                .filter(|gpu| !gpu.parent_bdf.is_empty() && is_janus_bdf(&gpu.bdf))
                .collect();
            node.gpus.extend(slices);
            node.cached_models = existing.cached_models;
        }
        state
            .store
            .save_entity(STORE_NODES, &node.id, &node)
            .map_err(|err| err.to_string())?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn inventory_becomes_a_schedulable_node() {
        let body = br#"{
            "config":"single_gpu",
            "nodes":[{"id":"node-0","gpus":[{"id":"janus:node-0:gpu-0","profile":"H100_80GB","memory_gb":80,"mig":true,"source":"janus"}]}],
            "mig_profiles":[{"name":"1g.10gb","memory_gb":10,"max_per_gpu":7,"hardware":"H100_80GB"}]
        }"#;
        let nodes = nodes_from_inventory(body, 50).unwrap();
        assert_eq!(nodes[0].id, "node-0");
        assert_eq!(nodes[0].gpus[0].bdf, "janus:node-0:gpu-0");
        assert_eq!(nodes[0].gpus[0].model, "janus:H100_80GB");
        assert_eq!(
            replica_hostport("http://127.0.0.1:30818/"),
            "127.0.0.1:30818"
        );
        assert!(is_janus_bdf("janus:node-0:gpu-0"));
        assert!(is_nvidia(0x10de, "0x10de"));
        assert!(!is_nvidia(0x102b, "0x102b"));
    }
}
