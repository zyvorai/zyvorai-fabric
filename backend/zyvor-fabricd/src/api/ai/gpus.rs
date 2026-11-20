// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: LicenseRef-Zyvor-Production-1.0

//! GET /api/ai/gpus — FluxVM inventory + Fabric allocation overlay (preview).

use axum::{extract::State, http::StatusCode, Json};
use security::RequireRead;
use std::collections::HashMap;
use std::sync::Arc;

use crate::server::AppState;

use super::types::{FabricGpuView, GpuAllocation, InferenceDeployment};
use super::{fluxvm_client, STORE_DEPLOYMENTS};

/// GET /api/ai/gpus
pub async fn list_gpus(
    RequireRead(_claims): RequireRead,
    State(state): State<Arc<AppState>>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let client = fluxvm_client(&state)?;
    // Soft-fail for GPU-less smoke tests: empty inventory when FluxVM is down
    // or has no GPU sysfs, rather than a hard 502.
    let inventory = match client.list_host_gpus().await {
        Ok(g) => g,
        Err(e) => {
            tracing::warn!("FluxVM list_host_gpus: {e}");
            Vec::new()
        }
    };

    let deployments: Vec<InferenceDeployment> = state
        .store
        .list_entities(STORE_DEPLOYMENTS)
        .unwrap_or_default();

    let mut by_bdf: HashMap<String, GpuAllocation> = HashMap::new();
    for dep in &deployments {
        for rep in &dep.status.replicas {
            if rep.bdf.is_empty() {
                continue;
            }
            by_bdf.insert(
                rep.bdf.to_ascii_lowercase(),
                GpuAllocation {
                    deployment: dep.name.clone(),
                    tenant: dep.tenant.clone(),
                    vm_name: rep.vm_name.clone(),
                },
            );
        }
    }

    let items: Vec<FabricGpuView> = inventory
        .into_iter()
        .map(|gpu| {
            let key = gpu.bdf.to_ascii_lowercase();
            let allocated_to = by_bdf.get(&key).cloned();
            FabricGpuView { gpu, allocated_to }
        })
        .collect();

    Ok(Json(serde_json::json!({ "items": items })))
}
