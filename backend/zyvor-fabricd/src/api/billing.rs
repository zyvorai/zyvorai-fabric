// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: LicenseRef-Zyvor-Production-2.0

use crate::api::container_declarative::ContainerGroupSpec;
use crate::server::AppState;
use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use security::{RequireAdmin, RequireRead};
use serde_json::json;
use std::sync::Arc;

/// GET /api/billing/pricing - Get current pricing rules
pub async fn get_pricing(
    RequireRead(_claims): RequireRead,
    State(state): State<Arc<AppState>>,
) -> Result<Json<billing::PricingRule>, (StatusCode, Json<serde_json::Value>)> {
    let pricing = state
        .store
        .get_entity::<billing::PricingRule>("billing", "default")
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"error": e.to_string()})),
            )
        })?
        .unwrap_or_default();
    Ok(Json(pricing))
}

/// PUT /api/billing/pricing - Update pricing rules
pub async fn update_pricing(
    RequireAdmin(_claims): RequireAdmin,
    State(state): State<Arc<AppState>>,
    Json(pricing): Json<billing::PricingRule>,
) -> Result<Json<billing::PricingRule>, (StatusCode, Json<serde_json::Value>)> {
    state
        .store
        .save_entity("billing", "default", &pricing)
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"error": e.to_string()})),
            )
        })?;
    Ok(Json(pricing))
}

/// GET /api/billing/usage - Get usage records
pub async fn get_usage(
    RequireRead(_claims): RequireRead,
    State(state): State<Arc<AppState>>,
) -> Result<Json<Vec<billing::UsageRecord>>, (StatusCode, Json<serde_json::Value>)> {
    let records = state
        .store
        .list_entities::<billing::UsageRecord>("billing_usage")
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"error": e.to_string()})),
            )
        })?;
    Ok(Json(records))
}

/// POST /api/billing/invoice/:tenant_id - Generate invoice for a tenant
pub async fn generate_invoice(
    RequireAdmin(_claims): RequireAdmin,
    State(state): State<Arc<AppState>>,
    Path(tenant_id): Path<String>,
) -> Result<(StatusCode, Json<billing::Invoice>), (StatusCode, Json<serde_json::Value>)> {
    // Validate tenant_id
    crate::validation::validate_entity_name(&tenant_id)
        .map_err(|(s, m)| (s, Json(json!({"error": m}))))?;

    let pricing = state
        .store
        .get_entity::<billing::PricingRule>("billing", "default")
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"error": e.to_string()})),
            )
        })?
        .unwrap_or_default();

    // Collect usage for all VMs belonging to this tenant
    let vms = state.store.list_vms().map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error": e.to_string()})),
        )
    })?;

    let mut total_usage = billing::UsageRecord {
        id: uuid::Uuid::new_v4().to_string(),
        tenant_id: tenant_id.clone(),
        vm_name: "aggregate".into(),
        period_start: chrono::Utc::now() - chrono::Duration::hours(720), // 30 days
        period_end: chrono::Utc::now(),
        cpu_hours: 0.0,
        memory_gb_hours: 0.0,
        storage_gb_hours: 0.0,
        network_bytes: 0,
    };

    // Filter VMs belonging to this tenant (by label "tenant" or tag matching tenant_id)
    let tenant_vms: Vec<&vm_model::VM> = vms
        .iter()
        .filter(|vm| {
            // Check labels for tenant assignment
            if let Some(ref labels) = vm.labels {
                if labels.get("tenant").map(|v| v.as_str()) == Some(tenant_id.as_str()) {
                    return true;
                }
            }
            // Check tags for tenant assignment
            if let Some(ref tags) = vm.tags {
                if tags.iter().any(|t| t == &tenant_id) {
                    return true;
                }
            }
            false
        })
        .collect();

    for vm in &tenant_vms {
        let hours = 720.0; // 30 days
        total_usage.cpu_hours += vm.cpus as f64 * hours;
        total_usage.memory_gb_hours += (vm.memory as f64 / 1024.0) * hours;
        total_usage.storage_gb_hours += vm.disk as f64 * hours;
    }

    // Collect usage for all ContainerGroups belonging to this tenant (by
    // `spec.tenant` or a tag matching the tenant id, mirroring the VM
    // filter above). ContainerGroup has no disk concept, so it only
    // contributes CPU/memory hours.
    let container_groups = state
        .store
        .list_entities::<ContainerGroupSpec>("container_groups")
        .unwrap_or_default();
    let tenant_groups: Vec<&ContainerGroupSpec> = container_groups
        .iter()
        .filter(|cg| {
            cg.tenant.as_deref() == Some(tenant_id.as_str())
                || cg.tags.iter().any(|t| t == &tenant_id)
        })
        .collect();

    for cg in &tenant_groups {
        let hours = 720.0; // 30 days
        let (cpus, memory_mb) = cg.total_resources();
        total_usage.cpu_hours += cpus as f64 * hours;
        total_usage.memory_gb_hours += (memory_mb as f64 / 1024.0) * hours;
    }

    let invoice = billing::calculate_cost(&total_usage, &pricing);

    // Save the invoice
    state
        .store
        .save_entity("billing_invoices", &invoice.id, &invoice)
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"error": e.to_string()})),
            )
        })?;

    Ok((StatusCode::CREATED, Json(invoice)))
}
