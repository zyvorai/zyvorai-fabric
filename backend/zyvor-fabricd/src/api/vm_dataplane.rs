// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: LicenseRef-Zyvor-Production-2.0

//! Per-VM FluxVM Network Fabric dataplane — orthogonal to Fabric's
//! label→nftables `/network-policies` SDN. Proxies FluxVM's
//! `/v1/vms/{id}/network/*` and `/v1/network/{groups,cnp,…}` via
//! `VmDataplaneDriver` (schema v4).

use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    Json,
};
use serde::Deserialize;
use std::sync::Arc;
use zyvor_fabric_driver_core::{
    CiliumEndpointView, DataplaneHealth, DataplaneStats, DataplaneStatus, FlowRecord, IdentityInfo,
    IpcacheEntry, NetworkServiceSpec, NetworkServiceStatus, SecurityGroup, VmNetworkPolicy,
};

use crate::server::AppState;
use crate::validation::validate_vm_name;
use security::{RequireAdmin, RequireRead};
use service_lb::remote_identity::RemoteIpcacheClient;

#[derive(Debug, Deserialize)]
pub struct FlowsQuery {
    pub limit: Option<usize>,
}

#[derive(Debug, serde::Serialize)]
pub struct FlowListResponse {
    pub items: Vec<FlowRecord>,
}

#[derive(Debug, serde::Serialize)]
pub struct GroupListResponse {
    pub items: Vec<SecurityGroup>,
}

#[derive(Debug, serde::Serialize)]
pub struct ServiceListResponse {
    pub items: Vec<NetworkServiceSpec>,
}

#[derive(Debug, serde::Serialize)]
pub struct IdentityListResponse {
    pub items: Vec<IdentityInfo>,
}

#[derive(Debug, serde::Serialize)]
pub struct EndpointListResponse {
    pub items: Vec<CiliumEndpointView>,
}

#[derive(Debug, serde::Serialize)]
pub struct IpcacheListResponse {
    pub items: Vec<IpcacheEntry>,
}

#[derive(Debug, serde::Serialize)]
pub struct RefreshDnsResponse {
    pub refreshed: usize,
}

#[derive(Debug, serde::Serialize)]
pub struct DeletedResponse {
    pub deleted: String,
}

fn map_driver_err(
    status: StatusCode,
    ctx: &str,
    e: impl std::fmt::Display,
) -> (StatusCode, Json<serde_json::Value>) {
    crate::api_error::json_error(status, format!("{ctx}: {e}"))
}

/// GET /api/vms/:name/dataplane/status
pub async fn dataplane_status(
    RequireRead(_claims): RequireRead,
    State(state): State<Arc<AppState>>,
    Path(name): Path<String>,
) -> Result<Json<DataplaneStatus>, (StatusCode, Json<serde_json::Value>)> {
    validate_vm_name(&name).map_err(|(s, m)| crate::api_error::json_error(s, m))?;
    let status = state.driver.dataplane_status(&name).await.map_err(|e| {
        map_driver_err(
            StatusCode::NOT_FOUND,
            &format!("Dataplane status for VM '{name}'"),
            e,
        )
    })?;
    Ok(Json(status))
}

/// GET /api/vms/:name/dataplane/policy
pub async fn get_dataplane_policy(
    RequireRead(_claims): RequireRead,
    State(state): State<Arc<AppState>>,
    Path(name): Path<String>,
) -> Result<Json<VmNetworkPolicy>, (StatusCode, Json<serde_json::Value>)> {
    validate_vm_name(&name).map_err(|(s, m)| crate::api_error::json_error(s, m))?;
    let policy = state
        .driver
        .get_dataplane_policy(&name)
        .await
        .map_err(|e| {
            map_driver_err(
                StatusCode::NOT_FOUND,
                &format!("Dataplane policy for VM '{name}'"),
                e,
            )
        })?;
    Ok(Json(policy))
}

/// POST /api/vms/:name/dataplane/policy
pub async fn set_dataplane_policy(
    RequireAdmin(_claims): RequireAdmin,
    State(state): State<Arc<AppState>>,
    Path(name): Path<String>,
    Json(policy): Json<VmNetworkPolicy>,
) -> Result<Json<VmNetworkPolicy>, (StatusCode, Json<serde_json::Value>)> {
    validate_vm_name(&name).map_err(|(s, m)| crate::api_error::json_error(s, m))?;
    let saved = state
        .driver
        .set_dataplane_policy(&name, &policy)
        .await
        .map_err(|e| {
            map_driver_err(
                StatusCode::INTERNAL_SERVER_ERROR,
                &format!("Failed to set dataplane policy for VM '{name}'"),
                e,
            )
        })?;
    tracing::info!("Updated VM edge dataplane policy for '{name}'");
    Ok(Json(saved))
}

/// POST /api/vms/:name/dataplane/policy/control
/// Cilium-style Guard / Audit / Open / Invert / Block / Allow.
pub async fn dataplane_policy_control(
    RequireAdmin(_claims): RequireAdmin,
    State(state): State<Arc<AppState>>,
    Path(name): Path<String>,
    Json(req): Json<zyvor_fabric_driver_core::policy_control::ControlRequest>,
) -> Result<Json<VmNetworkPolicy>, (StatusCode, Json<serde_json::Value>)> {
    validate_vm_name(&name).map_err(|(s, m)| crate::api_error::json_error(s, m))?;
    let current = state
        .driver
        .get_dataplane_policy(&name)
        .await
        .map_err(|e| {
            map_driver_err(
                StatusCode::NOT_FOUND,
                &format!("Dataplane policy for VM '{name}'"),
                e,
            )
        })?;
    let next = zyvor_fabric_driver_core::policy_control::apply_control(current, &req)
        .map_err(|m| crate::api_error::json_error(StatusCode::BAD_REQUEST, m))?;
    let saved = state
        .driver
        .set_dataplane_policy(&name, &next)
        .await
        .map_err(|e| {
            map_driver_err(
                StatusCode::INTERNAL_SERVER_ERROR,
                &format!("Failed to apply dataplane control for VM '{name}'"),
                e,
            )
        })?;
    tracing::info!("Applied dataplane flow control for '{name}'");
    Ok(Json(saved))
}

#[derive(Debug, Deserialize)]
pub struct ExplainQuery {
    pub dest: String,
    pub port: Option<u16>,
    pub proto: Option<String>,
}

/// GET /api/vms/:name/dataplane/explain?dest=&port=&proto=
pub async fn dataplane_explain(
    RequireRead(_claims): RequireRead,
    State(state): State<Arc<AppState>>,
    Path(name): Path<String>,
    Query(q): Query<ExplainQuery>,
) -> Result<
    Json<zyvor_fabric_driver_core::policy_control::ExplainResult>,
    (StatusCode, Json<serde_json::Value>),
> {
    validate_vm_name(&name).map_err(|(s, m)| crate::api_error::json_error(s, m))?;
    let policy = state
        .driver
        .get_dataplane_policy(&name)
        .await
        .map_err(|e| {
            map_driver_err(
                StatusCode::NOT_FOUND,
                &format!("Dataplane policy for VM '{name}'"),
                e,
            )
        })?;
    Ok(Json(zyvor_fabric_driver_core::policy_control::explain(
        &policy,
        &q.dest,
        q.port.unwrap_or(0),
        q.proto.as_deref().unwrap_or("any"),
    )))
}

/// GET /api/vms/:name/dataplane/dry-run
pub async fn dataplane_dry_run(
    RequireRead(_claims): RequireRead,
    State(state): State<Arc<AppState>>,
    Path(name): Path<String>,
    Query(q): Query<FlowsQuery>,
) -> Result<
    Json<zyvor_fabric_driver_core::policy_control::DryRunReport>,
    (StatusCode, Json<serde_json::Value>),
> {
    validate_vm_name(&name).map_err(|(s, m)| crate::api_error::json_error(s, m))?;
    let policy = state
        .driver
        .get_dataplane_policy(&name)
        .await
        .map_err(|e| {
            map_driver_err(
                StatusCode::NOT_FOUND,
                &format!("Dataplane policy for VM '{name}'"),
                e,
            )
        })?;
    let flows = state
        .driver
        .dataplane_flows(&name, q.limit)
        .await
        .unwrap_or_default();
    Ok(Json(
        zyvor_fabric_driver_core::policy_control::dry_run_guard(&policy, &flows),
    ))
}

/// GET /api/dataplane/templates
pub async fn dataplane_templates(RequireRead(_claims): RequireRead) -> Json<serde_json::Value> {
    Json(serde_json::json!({
        "items": ["open", "guard", "web", "dns-only", "no-world"]
    }))
}

/// GET /api/vms/:name/dataplane/stats
pub async fn dataplane_stats(
    RequireRead(_claims): RequireRead,
    State(state): State<Arc<AppState>>,
    Path(name): Path<String>,
) -> Result<Json<DataplaneStats>, (StatusCode, Json<serde_json::Value>)> {
    validate_vm_name(&name).map_err(|(s, m)| crate::api_error::json_error(s, m))?;
    let stats = state.driver.dataplane_stats(&name).await.map_err(|e| {
        map_driver_err(
            StatusCode::NOT_FOUND,
            &format!("Dataplane stats for VM '{name}'"),
            e,
        )
    })?;
    Ok(Json(stats))
}

/// GET /api/vms/:name/dataplane/flows?limit=
pub async fn dataplane_flows(
    RequireRead(_claims): RequireRead,
    State(state): State<Arc<AppState>>,
    Path(name): Path<String>,
    Query(q): Query<FlowsQuery>,
) -> Result<Json<FlowListResponse>, (StatusCode, Json<serde_json::Value>)> {
    validate_vm_name(&name).map_err(|(s, m)| crate::api_error::json_error(s, m))?;
    let items = state
        .driver
        .dataplane_flows(&name, q.limit)
        .await
        .map_err(|e| {
            map_driver_err(
                StatusCode::NOT_FOUND,
                &format!("Dataplane flows for VM '{name}'"),
                e,
            )
        })?;
    Ok(Json(FlowListResponse { items }))
}

/// GET /api/vms/:name/dataplane/effective
pub async fn dataplane_effective(
    RequireRead(_claims): RequireRead,
    State(state): State<Arc<AppState>>,
    Path(name): Path<String>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    validate_vm_name(&name).map_err(|(s, m)| crate::api_error::json_error(s, m))?;
    let body = state.driver.dataplane_effective(&name).await.map_err(|e| {
        map_driver_err(
            StatusCode::NOT_FOUND,
            &format!("Dataplane effective policy for VM '{name}'"),
            e,
        )
    })?;
    Ok(Json(body))
}

/// GET /api/vms/:name/dataplane/drop-reasons?limit=
pub async fn dataplane_drop_reasons(
    RequireRead(_claims): RequireRead,
    State(state): State<Arc<AppState>>,
    Path(name): Path<String>,
    Query(q): Query<FlowsQuery>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    validate_vm_name(&name).map_err(|(s, m)| crate::api_error::json_error(s, m))?;
    let items = state
        .driver
        .dataplane_drop_reasons(&name, q.limit)
        .await
        .map_err(|e| {
            map_driver_err(
                StatusCode::NOT_FOUND,
                &format!("Dataplane drop-reasons for VM '{name}'"),
                e,
            )
        })?;
    Ok(Json(serde_json::json!({ "items": items })))
}

/// GET /api/vms/:name/dataplane/pod-policy
pub async fn get_pod_policy(
    RequireRead(_claims): RequireRead,
    State(state): State<Arc<AppState>>,
    Path(name): Path<String>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    validate_vm_name(&name).map_err(|(s, m)| crate::api_error::json_error(s, m))?;
    let policy = state
        .driver
        .get_pod_network_policy(&name)
        .await
        .map_err(|e| {
            map_driver_err(
                StatusCode::NOT_FOUND,
                &format!("Pod policy for VM '{name}'"),
                e,
            )
        })?;
    Ok(Json(policy.unwrap_or(serde_json::Value::Null)))
}

/// POST /api/vms/:name/dataplane/pod-policy
pub async fn set_pod_policy(
    RequireAdmin(_claims): RequireAdmin,
    State(state): State<Arc<AppState>>,
    Path(name): Path<String>,
    Json(policy): Json<serde_json::Value>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    validate_vm_name(&name).map_err(|(s, m)| crate::api_error::json_error(s, m))?;
    state
        .driver
        .set_pod_network_policy(&name, &policy)
        .await
        .map_err(|e| {
            map_driver_err(
                StatusCode::BAD_REQUEST,
                &format!("Set pod policy for VM '{name}'"),
                e,
            )
        })?;
    Ok(Json(serde_json::json!({ "ok": true })))
}

/// DELETE /api/vms/:name/dataplane/pod-policy
pub async fn delete_pod_policy(
    RequireAdmin(_claims): RequireAdmin,
    State(state): State<Arc<AppState>>,
    Path(name): Path<String>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    validate_vm_name(&name).map_err(|(s, m)| crate::api_error::json_error(s, m))?;
    state
        .driver
        .delete_pod_network_policy(&name)
        .await
        .map_err(|e| {
            map_driver_err(
                StatusCode::NOT_FOUND,
                &format!("Delete pod policy for VM '{name}'"),
                e,
            )
        })?;
    Ok(Json(serde_json::json!({ "ok": true })))
}

/// GET /api/dataplane/groups
pub async fn list_groups(
    RequireRead(_claims): RequireRead,
    State(state): State<Arc<AppState>>,
) -> Result<Json<GroupListResponse>, (StatusCode, Json<serde_json::Value>)> {
    let items = state
        .driver
        .dataplane_list_groups()
        .await
        .map_err(|e| map_driver_err(StatusCode::BAD_GATEWAY, "Dataplane groups", e))?;
    Ok(Json(GroupListResponse { items }))
}

/// GET /api/dataplane/groups/:name
pub async fn get_group(
    RequireRead(_claims): RequireRead,
    State(state): State<Arc<AppState>>,
    Path(name): Path<String>,
) -> Result<Json<SecurityGroup>, (StatusCode, Json<serde_json::Value>)> {
    let group = state.driver.dataplane_get_group(&name).await.map_err(|e| {
        map_driver_err(
            StatusCode::NOT_FOUND,
            &format!("Dataplane group '{name}'"),
            e,
        )
    })?;
    Ok(Json(group))
}

/// POST /api/dataplane/groups
pub async fn upsert_group(
    RequireAdmin(_claims): RequireAdmin,
    State(state): State<Arc<AppState>>,
    Json(group): Json<SecurityGroup>,
) -> Result<Json<SecurityGroup>, (StatusCode, Json<serde_json::Value>)> {
    let saved = state
        .driver
        .dataplane_upsert_group(&group)
        .await
        .map_err(|e| {
            map_driver_err(
                StatusCode::INTERNAL_SERVER_ERROR,
                &format!("Failed to upsert dataplane group '{}'", group.name),
                e,
            )
        })?;
    tracing::info!("Upserted edge dataplane group '{}'", saved.name);
    Ok(Json(saved))
}

/// DELETE /api/dataplane/groups/:name
pub async fn delete_group(
    RequireAdmin(_claims): RequireAdmin,
    State(state): State<Arc<AppState>>,
    Path(name): Path<String>,
) -> Result<Json<DeletedResponse>, (StatusCode, Json<serde_json::Value>)> {
    state
        .driver
        .dataplane_delete_group(&name)
        .await
        .map_err(|e| {
            map_driver_err(
                StatusCode::INTERNAL_SERVER_ERROR,
                &format!("Failed to delete dataplane group '{name}'"),
                e,
            )
        })?;
    Ok(Json(DeletedResponse { deleted: name }))
}

/// GET /api/dataplane/services — Maglev/eBPF Service Fabric (not SDN `/api/services`).
pub async fn list_services(
    RequireRead(_claims): RequireRead,
    State(state): State<Arc<AppState>>,
) -> Result<Json<ServiceListResponse>, (StatusCode, Json<serde_json::Value>)> {
    let items = state
        .driver
        .dataplane_list_services()
        .await
        .map_err(|e| map_driver_err(StatusCode::BAD_GATEWAY, "Dataplane services", e))?;
    Ok(Json(ServiceListResponse { items }))
}

/// GET /api/dataplane/services/:name
pub async fn get_service(
    RequireRead(_claims): RequireRead,
    State(state): State<Arc<AppState>>,
    Path(name): Path<String>,
) -> Result<Json<NetworkServiceSpec>, (StatusCode, Json<serde_json::Value>)> {
    let service = state
        .driver
        .dataplane_get_service(&name)
        .await
        .map_err(|e| {
            map_driver_err(
                StatusCode::NOT_FOUND,
                &format!("Dataplane service '{name}'"),
                e,
            )
        })?;
    Ok(Json(service))
}

/// POST /api/dataplane/services
pub async fn upsert_service(
    RequireAdmin(_claims): RequireAdmin,
    State(state): State<Arc<AppState>>,
    Json(service): Json<NetworkServiceSpec>,
) -> Result<Json<NetworkServiceStatus>, (StatusCode, Json<serde_json::Value>)> {
    let spec = to_service_lb_spec(&service)?;
    let nodes = service_nodes(&state);
    let orch =
        service_lb::ServiceOrchestrator::new(service_lb::FluxVmHttpClient::new().map_err(|e| {
            map_driver_err(StatusCode::INTERNAL_SERVER_ERROR, "service-lb client", e)
        })?);
    let report = orch.apply(&spec, &nodes).await.map_err(|e| {
        map_driver_err(
            StatusCode::INTERNAL_SERVER_ERROR,
            &format!("Failed to upsert dataplane service '{}'", service.name),
            e,
        )
    })?;
    tracing::info!(
        service = %report.service,
        nodes = ?report.applied_nodes,
        "Upserted Maglev service via service-lb"
    );
    // Authoritative status from the local FluxVM node (idempotent re-upsert).
    let status = state
        .driver
        .dataplane_upsert_service(&service)
        .await
        .map_err(|e| {
            map_driver_err(
                StatusCode::BAD_GATEWAY,
                &format!(
                    "Service '{}' applied but status refresh failed",
                    service.name
                ),
                e,
            )
        })?;
    Ok(Json(status))
}

/// DELETE /api/dataplane/services/:name
pub async fn delete_service(
    RequireAdmin(_claims): RequireAdmin,
    State(state): State<Arc<AppState>>,
    Path(name): Path<String>,
) -> Result<Json<DeletedResponse>, (StatusCode, Json<serde_json::Value>)> {
    let nodes = service_nodes(&state);
    let orch =
        service_lb::ServiceOrchestrator::new(service_lb::FluxVmHttpClient::new().map_err(|e| {
            map_driver_err(StatusCode::INTERNAL_SERVER_ERROR, "service-lb client", e)
        })?);
    let report = orch.delete(&name, &nodes).await.map_err(|e| {
        map_driver_err(
            StatusCode::INTERNAL_SERVER_ERROR,
            &format!("Failed to delete dataplane service '{name}'"),
            e,
        )
    })?;
    tracing::info!(
        service = %name,
        nodes = ?report.applied_nodes,
        "Deleted Maglev service via service-lb"
    );
    Ok(Json(DeletedResponse { deleted: name }))
}

/// GET /api/dataplane/services/status — FluxVM host service dataplane status (schema v4).
pub async fn services_host_status(
    RequireRead(_claims): RequireRead,
    State(state): State<Arc<AppState>>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let nodes = service_nodes(&state);
    let client = service_lb::FluxVmHttpClient::new()
        .map_err(|e| map_driver_err(StatusCode::INTERNAL_SERVER_ERROR, "service-lb client", e))?;
    use service_lb::ServiceNodeClient;
    // Primary node status is enough for the console; multi-node topology is
    // visible via each node's FluxVM URL when configured.
    let status = client
        .get_status(&nodes[0])
        .await
        .map_err(|e| map_driver_err(StatusCode::BAD_GATEWAY, "FluxVM service host status", e))?;
    Ok(Json(serde_json::to_value(status).unwrap_or_default()))
}

/// GET /api/dataplane/services/stats — FluxVM host service counters.
pub async fn services_host_stats(
    RequireRead(_claims): RequireRead,
    State(state): State<Arc<AppState>>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    fluxvm_json_get(&state, "/v1/network/services/stats").await
}

/// GET /api/dataplane/services/health — backend health report (schema v3).
pub async fn services_health(
    RequireRead(_claims): RequireRead,
    State(state): State<Arc<AppState>>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    fluxvm_json_get(&state, "/v1/network/services/health").await
}

/// POST /api/dataplane/services/health/reconcile — run active health probes.
pub async fn services_health_reconcile(
    RequireAdmin(_claims): RequireAdmin,
    State(state): State<Arc<AppState>>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    fluxvm_json_post_empty(&state, "/v1/network/services/health/reconcile").await
}

/// POST /api/dataplane/services/conntrack/gc — expire forward/reverse state.
pub async fn services_conntrack_gc(
    RequireAdmin(_claims): RequireAdmin,
    State(state): State<Arc<AppState>>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    fluxvm_json_post_empty(&state, "/v1/network/services/conntrack/gc").await
}

/// POST /api/dataplane/services/pressure/reconcile — adaptive map pressure controller.
pub async fn services_pressure_reconcile(
    RequireAdmin(_claims): RequireAdmin,
    State(state): State<Arc<AppState>>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    fluxvm_json_post_empty(&state, "/v1/network/services/pressure/reconcile").await
}

/// GET /api/dataplane/services/advertisements — VIP advertise snapshot.
pub async fn services_advertisements(
    RequireRead(_claims): RequireRead,
    State(state): State<Arc<AppState>>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    fluxvm_json_get(&state, "/v1/network/services/advertisements").await
}

/// GET /api/dataplane/services/flows — FluxScope service flow records (schema v4).
pub async fn services_flows(
    RequireRead(_claims): RequireRead,
    State(state): State<Arc<AppState>>,
    axum::extract::Query(q): axum::extract::Query<std::collections::HashMap<String, String>>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let limit = q.get("limit").cloned().unwrap_or_else(|| "256".into());
    fluxvm_json_get(&state, &format!("/v1/network/services/flows?limit={limit}")).await
}

/// POST /api/dataplane/services/telemetry/export — OTLP/HTTP JSON export.
pub async fn services_telemetry_export(
    RequireAdmin(_claims): RequireAdmin,
    State(state): State<Arc<AppState>>,
    axum::extract::Query(q): axum::extract::Query<std::collections::HashMap<String, String>>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let limit = q.get("limit").cloned().unwrap_or_else(|| "1024".into());
    fluxvm_json_post_empty(
        &state,
        &format!("/v1/network/services/telemetry/export?limit={limit}"),
    )
    .await
}

/// GET /api/dataplane/services/:name/conntrack/delta — HA delta export (v5).
pub async fn services_conntrack_delta(
    RequireAdmin(_claims): RequireAdmin,
    State(state): State<Arc<AppState>>,
    Path(name): Path<String>,
    axum::extract::Query(q): axum::extract::Query<std::collections::HashMap<String, String>>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let after = q.get("after_seq").cloned().unwrap_or_else(|| "0".into());
    let max = q
        .get("max_entries")
        .cloned()
        .unwrap_or_else(|| "1024".into());
    fluxvm_json_get(
        &state,
        &format!("/v1/network/services/{name}/conntrack/delta?after_seq={after}&max_entries={max}"),
    )
    .await
}

/// POST /api/dataplane/services/:name/conntrack/delta/import — apply HA delta batch.
pub async fn services_conntrack_delta_import(
    RequireAdmin(_claims): RequireAdmin,
    State(state): State<Arc<AppState>>,
    Path(name): Path<String>,
    Json(body): Json<serde_json::Value>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    fluxvm_json_post_json(
        &state,
        &format!("/v1/network/services/{name}/conntrack/delta/import"),
        &body,
    )
    .await
}

/// POST /api/dataplane/services/:name/conntrack/delta/ack — advance source journal watermark.
pub async fn services_conntrack_delta_ack(
    RequireAdmin(_claims): RequireAdmin,
    State(state): State<Arc<AppState>>,
    Path(name): Path<String>,
    Json(body): Json<serde_json::Value>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    fluxvm_json_post_json(
        &state,
        &format!("/v1/network/services/{name}/conntrack/delta/ack"),
        &body,
    )
    .await
}

async fn fluxvm_json_get(
    state: &AppState,
    path: &str,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let url = format!(
        "{}{}",
        state.config.driver.fluxvm_url.trim_end_matches('/'),
        path
    );
    let mut req = reqwest::Client::new().get(&url);
    if let Some(token) = state.config.driver.fluxvm_token.as_deref() {
        req = req.bearer_auth(token);
    }
    let resp = req
        .send()
        .await
        .map_err(|e| map_driver_err(StatusCode::BAD_GATEWAY, &format!("FluxVM GET {path}"), e))?;
    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        return Err((
            StatusCode::BAD_GATEWAY,
            Json(serde_json::json!({ "error": format!("FluxVM {path} HTTP {status}: {body}") })),
        ));
    }
    let body = resp
        .json::<serde_json::Value>()
        .await
        .map_err(|e| map_driver_err(StatusCode::BAD_GATEWAY, &format!("FluxVM {path} JSON"), e))?;
    Ok(Json(body))
}

async fn fluxvm_json_post_empty(
    state: &AppState,
    path: &str,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let url = format!(
        "{}{}",
        state.config.driver.fluxvm_url.trim_end_matches('/'),
        path
    );
    let mut req = reqwest::Client::new().post(&url);
    if let Some(token) = state.config.driver.fluxvm_token.as_deref() {
        req = req.bearer_auth(token);
    }
    let resp = req
        .send()
        .await
        .map_err(|e| map_driver_err(StatusCode::BAD_GATEWAY, &format!("FluxVM POST {path}"), e))?;
    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        return Err((
            StatusCode::BAD_GATEWAY,
            Json(serde_json::json!({ "error": format!("FluxVM {path} HTTP {status}: {body}") })),
        ));
    }
    let body = resp
        .json::<serde_json::Value>()
        .await
        .map_err(|e| map_driver_err(StatusCode::BAD_GATEWAY, &format!("FluxVM {path} JSON"), e))?;
    Ok(Json(body))
}

async fn fluxvm_json_post_json(
    state: &AppState,
    path: &str,
    body: &serde_json::Value,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let url = format!(
        "{}{}",
        state.config.driver.fluxvm_url.trim_end_matches('/'),
        path
    );
    let mut req = reqwest::Client::new().post(&url).json(body);
    if let Some(token) = state.config.driver.fluxvm_token.as_deref() {
        req = req.bearer_auth(token);
    }
    let resp = req
        .send()
        .await
        .map_err(|e| map_driver_err(StatusCode::BAD_GATEWAY, &format!("FluxVM POST {path}"), e))?;
    if !resp.status().is_success() {
        let status = resp.status();
        let text = resp.text().await.unwrap_or_default();
        return Err((
            StatusCode::BAD_GATEWAY,
            Json(serde_json::json!({ "error": format!("FluxVM {path} HTTP {status}: {text}") })),
        ));
    }
    let out = resp
        .json::<serde_json::Value>()
        .await
        .map_err(|e| map_driver_err(StatusCode::BAD_GATEWAY, &format!("FluxVM {path} JSON"), e))?;
    Ok(Json(out))
}

fn service_nodes(state: &AppState) -> Vec<service_lb::NodeTarget> {
    let mut nodes = vec![service_lb::NodeTarget {
        name: "local".into(),
        base_url: state.config.driver.fluxvm_url.clone(),
        token: state.config.driver.fluxvm_token.clone(),
    }];
    for n in &state.config.driver.fluxvm_nodes {
        nodes.push(service_lb::NodeTarget {
            name: n.name.clone(),
            base_url: n.url.clone(),
            token: n
                .token
                .clone()
                .or_else(|| state.config.driver.fluxvm_token.clone()),
        });
    }
    nodes
}

fn to_service_lb_spec(
    service: &NetworkServiceSpec,
) -> Result<service_lb::ServiceSpec, (StatusCode, Json<serde_json::Value>)> {
    use service_lb::{
        BackendState, HealthCheckKind, ServiceAlgorithm, ServiceBackend, ServiceExposure,
        ServiceHealthCheck, ServiceMode, ServiceProtocol,
    };
    use std::net::IpAddr;

    let bad = |msg: String| {
        (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({ "error": msg })),
        )
    };

    let vip: IpAddr = service
        .vip
        .parse()
        .map_err(|_| bad(format!("invalid VIP '{}'", service.vip)))?;
    let protocol = match service.protocol {
        zyvor_fabric_driver_core::NetworkServiceProtocol::Tcp => ServiceProtocol::Tcp,
        zyvor_fabric_driver_core::NetworkServiceProtocol::Udp => ServiceProtocol::Udp,
    };
    let mode = match service.mode {
        zyvor_fabric_driver_core::NetworkServiceMode::Nat => ServiceMode::Nat,
        zyvor_fabric_driver_core::NetworkServiceMode::Dsr => ServiceMode::Dsr,
    };
    let exposure = match service.exposure {
        zyvor_fabric_driver_core::NetworkServiceExposure::EastWest => ServiceExposure::EastWest,
        zyvor_fabric_driver_core::NetworkServiceExposure::NorthSouth => ServiceExposure::NorthSouth,
        zyvor_fabric_driver_core::NetworkServiceExposure::Both => ServiceExposure::Both,
    };
    let snat_address = match &service.snat_address {
        Some(raw) => Some(
            raw.parse::<IpAddr>()
                .map_err(|_| bad(format!("invalid SNAT address '{raw}'")))?,
        ),
        None => None,
    };
    let health_check = service.health_check.as_ref().map(|h| ServiceHealthCheck {
        kind: match h.kind {
            zyvor_fabric_driver_core::NetworkHealthCheckKind::Tcp => HealthCheckKind::Tcp,
        },
        timeout_ms: h.timeout_ms,
        unhealthy_threshold: h.unhealthy_threshold,
        healthy_threshold: h.healthy_threshold,
    });
    const MAX_SERVICE_BACKENDS: usize = 1024;
    let backend_count = service.backends.len();
    if backend_count > MAX_SERVICE_BACKENDS {
        return Err(bad("service has too many backends".to_string()));
    }
    let mut backends = Vec::with_capacity(backend_count);
    for b in &service.backends {
        let address: IpAddr = b
            .address
            .parse()
            .map_err(|_| bad(format!("invalid backend address '{}'", b.address)))?;
        backends.push(ServiceBackend {
            address,
            port: b.port,
            weight: b.weight,
            enabled: b.enabled,
            state: match b.state {
                zyvor_fabric_driver_core::NetworkBackendState::Ready => BackendState::Ready,
                zyvor_fabric_driver_core::NetworkBackendState::Draining => BackendState::Draining,
                zyvor_fabric_driver_core::NetworkBackendState::Unhealthy => BackendState::Unhealthy,
            },
            drain_until_unix_ms: b.drain_until_unix_ms,
        });
    }
    Ok(service_lb::ServiceSpec {
        name: service.name.clone(),
        vip,
        port: service.port,
        protocol,
        algorithm: ServiceAlgorithm::Maglev,
        mode,
        exposure,
        backends,
        maglev_table_size: service.maglev_table_size,
        snat_address,
        health_check,
        advertise: service.advertise,
        max_egress_mbps: service.max_egress_mbps,
        flow_sample_rate: service.flow_sample_rate,
        host_routing: service.host_routing,
        site_id: service.site_id.clone(),
        route_domain: service.route_domain.clone(),
    })
}

/// GET /api/dataplane/cnp
pub async fn list_cnp(
    RequireRead(_claims): RequireRead,
    State(state): State<Arc<AppState>>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let body = state
        .driver
        .dataplane_list_cnp()
        .await
        .map_err(|e| map_driver_err(StatusCode::BAD_GATEWAY, "Dataplane CNP list", e))?;
    Ok(Json(body))
}

/// GET /api/dataplane/cnp/:name
pub async fn get_cnp(
    RequireRead(_claims): RequireRead,
    State(state): State<Arc<AppState>>,
    Path(name): Path<String>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let body = state.driver.dataplane_get_cnp(&name).await.map_err(|e| {
        map_driver_err(StatusCode::NOT_FOUND, &format!("Dataplane CNP '{name}'"), e)
    })?;
    Ok(Json(body))
}

/// POST /api/dataplane/cnp
pub async fn apply_cnp(
    RequireAdmin(_claims): RequireAdmin,
    State(state): State<Arc<AppState>>,
    Json(doc): Json<serde_json::Value>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let body = state.driver.dataplane_apply_cnp(&doc).await.map_err(|e| {
        map_driver_err(
            StatusCode::INTERNAL_SERVER_ERROR,
            "Failed to apply dataplane CNP",
            e,
        )
    })?;
    tracing::info!("Applied edge dataplane CNP");
    Ok(Json(body))
}

/// DELETE /api/dataplane/cnp/:name
pub async fn delete_cnp(
    RequireAdmin(_claims): RequireAdmin,
    State(state): State<Arc<AppState>>,
    Path(name): Path<String>,
) -> Result<Json<DeletedResponse>, (StatusCode, Json<serde_json::Value>)> {
    state
        .driver
        .dataplane_delete_cnp(&name)
        .await
        .map_err(|e| {
            map_driver_err(
                StatusCode::INTERNAL_SERVER_ERROR,
                &format!("Failed to delete dataplane CNP '{name}'"),
                e,
            )
        })?;
    Ok(Json(DeletedResponse { deleted: name }))
}

/// GET /api/dataplane/identities
pub async fn list_identities(
    RequireRead(_claims): RequireRead,
    State(state): State<Arc<AppState>>,
) -> Result<Json<IdentityListResponse>, (StatusCode, Json<serde_json::Value>)> {
    let items = state
        .driver
        .dataplane_list_identities()
        .await
        .map_err(|e| map_driver_err(StatusCode::BAD_GATEWAY, "Dataplane identities", e))?;
    Ok(Json(IdentityListResponse { items }))
}

/// GET /api/dataplane/endpoints — FluxVM CEP-*shaped* views (+ identity_source).
pub async fn list_endpoints(
    RequireRead(_claims): RequireRead,
    State(state): State<Arc<AppState>>,
) -> Result<Json<EndpointListResponse>, (StatusCode, Json<serde_json::Value>)> {
    let items = state
        .driver
        .dataplane_list_endpoints()
        .await
        .map_err(|e| map_driver_err(StatusCode::BAD_GATEWAY, "Dataplane endpoints", e))?;
    Ok(Json(EndpointListResponse { items }))
}

/// GET /api/dataplane/microvm-metrics — Prometheus text from FluxVM MicroVM histograms.
pub async fn microvm_metrics(
    RequireRead(_claims): RequireRead,
    State(state): State<Arc<AppState>>,
) -> Result<axum::response::Response, (StatusCode, Json<serde_json::Value>)> {
    use axum::response::IntoResponse;
    let url = state
        .config
        .driver
        .microvm_metrics_url
        .as_deref()
        .unwrap_or("http://127.0.0.1:9108/metrics");
    if url.is_empty() || url == "off" {
        return Ok((
            [(
                axum::http::header::CONTENT_TYPE,
                "text/plain; charset=utf-8",
            )],
            "# microvm metrics disabled\n",
        )
            .into_response());
    }
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(3))
        .build()
        .map_err(|e| map_driver_err(StatusCode::INTERNAL_SERVER_ERROR, "metrics client", e))?;
    match client.get(url).send().await {
        Ok(resp) if resp.status().is_success() => {
            let body = resp
                .text()
                .await
                .unwrap_or_else(|_| "# empty metrics body\n".into());
            Ok((
                [(
                    axum::http::header::CONTENT_TYPE,
                    "text/plain; version=0.0.4",
                )],
                body,
            )
                .into_response())
        }
        Ok(resp) => Err(map_driver_err(
            StatusCode::BAD_GATEWAY,
            "MicroVM metrics",
            format!("upstream HTTP {}", resp.status()),
        )),
        Err(e) => Err(map_driver_err(
            StatusCode::BAD_GATEWAY,
            "MicroVM metrics unreachable",
            e,
        )),
    }
}

/// GET /api/dataplane/observe
pub async fn observe(
    RequireRead(_claims): RequireRead,
    State(state): State<Arc<AppState>>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let body = state
        .driver
        .dataplane_observe()
        .await
        .map_err(|e| map_driver_err(StatusCode::BAD_GATEWAY, "Dataplane observe", e))?;
    Ok(Json(body))
}

/// GET /api/dataplane/hubble/flows?limit=
pub async fn hubble_flows(
    RequireRead(_claims): RequireRead,
    State(state): State<Arc<AppState>>,
    Query(q): Query<FlowsQuery>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let body = state
        .driver
        .dataplane_hubble_flows(q.limit)
        .await
        .map_err(|e| map_driver_err(StatusCode::BAD_GATEWAY, "Dataplane Hubble flows", e))?;
    Ok(Json(body))
}

/// GET /api/dataplane/health
pub async fn health(
    RequireRead(_claims): RequireRead,
    State(state): State<Arc<AppState>>,
) -> Result<Json<DataplaneHealth>, (StatusCode, Json<serde_json::Value>)> {
    let body = state
        .driver
        .dataplane_health()
        .await
        .map_err(|e| map_driver_err(StatusCode::BAD_GATEWAY, "Dataplane health", e))?;
    Ok(Json(body))
}

/// GET /api/dataplane/ipcache
pub async fn ipcache(
    RequireRead(_claims): RequireRead,
    State(state): State<Arc<AppState>>,
) -> Result<Json<IpcacheListResponse>, (StatusCode, Json<serde_json::Value>)> {
    let items = state
        .driver
        .dataplane_ipcache()
        .await
        .map_err(|e| map_driver_err(StatusCode::BAD_GATEWAY, "Dataplane ipcache", e))?;
    Ok(Json(IpcacheListResponse { items }))
}

/// POST /api/dataplane/refresh-dns
pub async fn refresh_dns(
    RequireAdmin(_claims): RequireAdmin,
    State(state): State<Arc<AppState>>,
) -> Result<Json<RefreshDnsResponse>, (StatusCode, Json<serde_json::Value>)> {
    let refreshed = state.driver.dataplane_refresh_dns().await.map_err(|e| {
        map_driver_err(
            StatusCode::INTERNAL_SERVER_ERROR,
            "Dataplane refresh-dns",
            e,
        )
    })?;
    Ok(Json(RefreshDnsResponse { refreshed }))
}

// ZYVOR_SERVICE_FABRIC_V6_POLICY_API
pub async fn list_service_policies(
    RequireRead(_claims): RequireRead,
    State(state): State<Arc<AppState>>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    fluxvm_json_get(&state, "/v1/network/services/policies").await
}

pub async fn get_service_policy(
    RequireRead(_claims): RequireRead,
    State(state): State<Arc<AppState>>,
    Path(name): Path<String>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    fluxvm_json_get(&state, &format!("/v1/network/services/{name}/policy")).await
}

pub async fn upsert_service_policy(
    RequireAdmin(_claims): RequireAdmin,
    State(state): State<Arc<AppState>>,
    Json(spec): Json<service_lb::policy::ServicePolicySpec>,
) -> Result<Json<service_lb::policy::PolicyApplyReport>, (StatusCode, Json<serde_json::Value>)> {
    service_lb::policy::validate_policy(&spec)
        .map_err(|e| map_driver_err(StatusCode::BAD_REQUEST, "service policy", e))?;
    let nodes = service_nodes(&state);
    let client = service_lb::policy::FluxVmPolicyHttpClient::new()
        .map_err(|e| map_driver_err(StatusCode::INTERNAL_SERVER_ERROR, "policy client", e))?;
    // Soft merge: fan any directory identities referenced by the policy into
    // node ipcache (cross-domain rows still publish if listed; callers that
    // need route_domain fencing use the library apply_with_remote_directory).
    let directory = remote_identity_directory(&state);
    let needed: std::collections::HashSet<u32> = spec
        .allow_identities
        .iter()
        .chain(spec.deny_identities.iter())
        .copied()
        .collect();
    if !needed.is_empty() {
        if let Ok(items) = directory.list(None, None) {
            let remotes: Vec<_> = items
                .into_iter()
                .filter(|r| needed.contains(&r.identity_id))
                .collect();
            if !remotes.is_empty() {
                let remote = service_lb::remote_identity::FluxVmRemoteIpcacheHttpClient::new()
                    .map_err(|e| {
                        map_driver_err(
                            StatusCode::INTERNAL_SERVER_ERROR,
                            "remote ipcache client",
                            e,
                        )
                    })?;
                for node in &nodes {
                    for entry in &remotes {
                        remote
                            .upsert_remote(node, entry.identity_id, &entry.cidrs)
                            .await
                            .map_err(|e| {
                                map_driver_err(
                                    StatusCode::BAD_GATEWAY,
                                    "remote identity fan-out",
                                    e,
                                )
                            })?;
                    }
                    let _ = remote.reconcile_policies(node).await;
                }
            }
        }
    }
    let report = service_lb::policy::PolicyOrchestrator::new(client)
        .apply(&spec, &nodes)
        .await
        .map_err(|e| {
            map_driver_err(
                StatusCode::BAD_GATEWAY,
                "distributed service policy apply",
                e,
            )
        })?;
    Ok(Json(report))
}

pub async fn delete_service_policy(
    RequireAdmin(_claims): RequireAdmin,
    State(state): State<Arc<AppState>>,
    Path(name): Path<String>,
) -> Result<Json<service_lb::policy::PolicyApplyReport>, (StatusCode, Json<serde_json::Value>)> {
    let nodes = service_nodes(&state);
    let client = service_lb::policy::FluxVmPolicyHttpClient::new()
        .map_err(|e| map_driver_err(StatusCode::INTERNAL_SERVER_ERROR, "policy client", e))?;
    let report = service_lb::policy::PolicyOrchestrator::new(client)
        .delete(&name, &nodes)
        .await
        .map_err(|e| {
            map_driver_err(
                StatusCode::BAD_GATEWAY,
                "distributed service policy delete",
                e,
            )
        })?;
    Ok(Json(report))
}

pub async fn service_envoy_contract(
    RequireRead(_claims): RequireRead,
    State(state): State<Arc<AppState>>,
    Path(name): Path<String>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    fluxvm_json_get(&state, &format!("/v1/network/services/{name}/l7/envoy")).await
}

fn remote_identity_directory(
    state: &AppState,
) -> service_lb::remote_identity::RemoteIdentityDirectory {
    let path = std::path::PathBuf::from(&state.config.storage.path)
        .join("service-fabric")
        .join("remote-identities.json");
    service_lb::remote_identity::RemoteIdentityDirectory::new(path)
}

#[derive(Debug, Deserialize)]
pub struct RemoteIdentityListQuery {
    pub site_id: Option<String>,
    pub route_domain: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct RemoteIdentityReconcileBody {
    pub site_id: Option<String>,
    pub route_domain: Option<String>,
    pub service: Option<String>,
}

/// GET /api/dataplane/remote-identities
pub async fn list_remote_identities(
    RequireRead(_claims): RequireRead,
    State(state): State<Arc<AppState>>,
    Query(q): Query<RemoteIdentityListQuery>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let dir = remote_identity_directory(&state);
    let items = dir
        .list(q.site_id.as_deref(), q.route_domain.as_deref())
        .map_err(|e| map_driver_err(StatusCode::INTERNAL_SERVER_ERROR, "remote identities", e))?;
    Ok(Json(serde_json::json!({ "items": items })))
}

/// POST /api/dataplane/remote-identities — upsert into Fabric directory (no fan-out).
pub async fn upsert_remote_identity(
    RequireAdmin(_claims): RequireAdmin,
    State(state): State<Arc<AppState>>,
    Json(mut identity): Json<service_lb::remote_identity::RemoteIdentity>,
) -> Result<Json<service_lb::remote_identity::RemoteIdentity>, (StatusCode, Json<serde_json::Value>)>
{
    if identity.updated_unix_ms == 0 {
        identity.updated_unix_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis() as u64)
            .unwrap_or(0);
    }
    let dir = remote_identity_directory(&state);
    let stored = dir
        .upsert(identity)
        .map_err(|e| map_driver_err(StatusCode::BAD_REQUEST, "remote identity", e))?;
    Ok(Json(stored))
}

/// DELETE /api/dataplane/remote-identities/{route_domain}/{identity_id}
/// Removes directory entry and unfans remote ipcache on FluxVM nodes.
pub async fn delete_remote_identity(
    RequireAdmin(_claims): RequireAdmin,
    State(state): State<Arc<AppState>>,
    Path((route_domain, identity_id)): Path<(String, u32)>,
) -> Result<
    Json<service_lb::remote_identity::RemoteReconcileReport>,
    (StatusCode, Json<serde_json::Value>),
> {
    let nodes = service_nodes(&state);
    let dir = remote_identity_directory(&state);
    let client =
        service_lb::remote_identity::FluxVmRemoteIpcacheHttpClient::new().map_err(|e| {
            map_driver_err(
                StatusCode::INTERNAL_SERVER_ERROR,
                "remote ipcache client",
                e,
            )
        })?;
    let orch = service_lb::remote_identity::RemoteIdentityOrchestrator::new(dir, client);
    let report = orch
        .delete_and_unfan(&route_domain, identity_id, &nodes, None, &[], None, 0)
        .await
        .map_err(|e| map_driver_err(StatusCode::BAD_GATEWAY, "remote identity delete", e))?;
    Ok(Json(report))
}

/// POST /api/dataplane/remote-identities/reconcile — fan directory → FluxVM ipcache.
pub async fn reconcile_remote_identities(
    RequireAdmin(_claims): RequireAdmin,
    State(state): State<Arc<AppState>>,
    Json(body): Json<RemoteIdentityReconcileBody>,
) -> Result<
    Json<service_lb::remote_identity::RemoteReconcileReport>,
    (StatusCode, Json<serde_json::Value>),
> {
    let nodes = service_nodes(&state);
    let dir = remote_identity_directory(&state);
    let client =
        service_lb::remote_identity::FluxVmRemoteIpcacheHttpClient::new().map_err(|e| {
            map_driver_err(
                StatusCode::INTERNAL_SERVER_ERROR,
                "remote ipcache client",
                e,
            )
        })?;
    let orch = service_lb::remote_identity::RemoteIdentityOrchestrator::new(dir, client);
    let report = orch
        .reconcile(
            body.route_domain.as_deref(),
            body.site_id.as_deref(),
            &nodes,
            &[],
            body.service.as_deref(),
            0,
        )
        .await
        .map_err(|e| map_driver_err(StatusCode::BAD_GATEWAY, "remote identity reconcile", e))?;
    Ok(Json(report))
}

fn remote_backend_directory(
    state: &AppState,
) -> service_lb::remote_backend::RemoteBackendDirectory {
    let path = std::path::PathBuf::from(&state.config.storage.path)
        .join("service-fabric")
        .join("remote-backends.json");
    service_lb::remote_backend::RemoteBackendDirectory::new(path)
}

#[derive(Debug, Deserialize)]
pub struct RemoteBackendListQuery {
    pub service: Option<String>,
    pub site_id: Option<String>,
    pub route_domain: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct RemoteBackendReconcileBody {
    pub service: Option<String>,
    pub site_id: Option<String>,
    pub route_domain: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct RemoteBackendDrainBody {
    pub drain_until_unix_ms: Option<u64>,
    pub weight: Option<u32>,
}

fn now_unix_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

/// GET /api/dataplane/remote-backends
pub async fn list_remote_backends(
    RequireRead(_claims): RequireRead,
    State(state): State<Arc<AppState>>,
    Query(q): Query<RemoteBackendListQuery>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let dir = remote_backend_directory(&state);
    let items = dir
        .list(
            q.service.as_deref(),
            q.site_id.as_deref(),
            q.route_domain.as_deref(),
        )
        .map_err(|e| map_driver_err(StatusCode::INTERNAL_SERVER_ERROR, "remote backends", e))?;
    Ok(Json(serde_json::json!({ "items": items })))
}

/// POST /api/dataplane/remote-backends — upsert into Fabric catalog (no fan-out).
pub async fn upsert_remote_backend(
    RequireAdmin(_claims): RequireAdmin,
    State(state): State<Arc<AppState>>,
    Json(mut backend): Json<service_lb::remote_backend::RemoteBackend>,
) -> Result<Json<service_lb::remote_backend::RemoteBackend>, (StatusCode, Json<serde_json::Value>)>
{
    if backend.updated_unix_ms == 0 {
        backend.updated_unix_ms = now_unix_ms();
    }
    let dir = remote_backend_directory(&state);
    let stored = dir
        .upsert(backend)
        .map_err(|e| map_driver_err(StatusCode::BAD_REQUEST, "remote backend", e))?;
    Ok(Json(stored))
}

/// POST /api/dataplane/remote-backends/{route_domain}/{service}/{address}/{port}/drain
/// Weighted drain handoff: catalog → Draining, then reconcile onto FluxVM nodes.
pub async fn drain_remote_backend(
    RequireAdmin(_claims): RequireAdmin,
    State(state): State<Arc<AppState>>,
    Path((route_domain, service, address, port)): Path<(String, String, String, u16)>,
    Json(body): Json<RemoteBackendDrainBody>,
) -> Result<
    Json<service_lb::remote_backend::RemoteBackendReconcileReport>,
    (StatusCode, Json<serde_json::Value>),
> {
    let now = now_unix_ms();
    let drain_until = body
        .drain_until_unix_ms
        .unwrap_or_else(|| now.saturating_add(300_000));
    let nodes = service_nodes(&state);
    let dir = remote_backend_directory(&state);
    let client = service_lb::FluxVmHttpClient::new()
        .map_err(|e| map_driver_err(StatusCode::INTERNAL_SERVER_ERROR, "service-lb client", e))?;
    let orch = service_lb::remote_backend::RemoteBackendOrchestrator::new(dir, client);
    let report = orch
        .drain_and_reconcile(
            &route_domain,
            &service,
            &address,
            port,
            drain_until,
            body.weight,
            &nodes,
            None,
            &[],
            now,
        )
        .await
        .map_err(|e| map_driver_err(StatusCode::BAD_GATEWAY, "remote backend drain", e))?;
    Ok(Json(report))
}

/// DELETE /api/dataplane/remote-backends/{route_domain}/{service}/{address}/{port}
/// Removes catalog entry and re-reconciles the service onto FluxVM nodes.
pub async fn delete_remote_backend(
    RequireAdmin(_claims): RequireAdmin,
    State(state): State<Arc<AppState>>,
    Path((route_domain, service, address, port)): Path<(String, String, String, u16)>,
) -> Result<
    Json<service_lb::remote_backend::RemoteBackendReconcileReport>,
    (StatusCode, Json<serde_json::Value>),
> {
    let now = now_unix_ms();
    let nodes = service_nodes(&state);
    let dir = remote_backend_directory(&state);
    let client = service_lb::FluxVmHttpClient::new()
        .map_err(|e| map_driver_err(StatusCode::INTERNAL_SERVER_ERROR, "service-lb client", e))?;
    let orch = service_lb::remote_backend::RemoteBackendOrchestrator::new(dir, client);
    let report = orch
        .delete_and_reconcile(
            &route_domain,
            &service,
            &address,
            port,
            &nodes,
            None,
            &[],
            now,
            true,
        )
        .await
        .map_err(|e| map_driver_err(StatusCode::BAD_GATEWAY, "remote backend delete", e))?;
    Ok(Json(report))
}

/// POST /api/dataplane/remote-backends/reconcile — merge Ready + active Draining remotes into Maglev upserts.
pub async fn reconcile_remote_backends(
    RequireAdmin(_claims): RequireAdmin,
    State(state): State<Arc<AppState>>,
    Json(body): Json<RemoteBackendReconcileBody>,
) -> Result<
    Json<service_lb::remote_backend::RemoteBackendReconcileReport>,
    (StatusCode, Json<serde_json::Value>),
> {
    let now = now_unix_ms();
    let nodes = service_nodes(&state);
    let dir = remote_backend_directory(&state);
    let client = service_lb::FluxVmHttpClient::new()
        .map_err(|e| map_driver_err(StatusCode::INTERNAL_SERVER_ERROR, "service-lb client", e))?;
    let orch = service_lb::remote_backend::RemoteBackendOrchestrator::new(dir, client);
    let report = orch
        .reconcile(
            body.service.as_deref(),
            body.site_id.as_deref(),
            body.route_domain.as_deref(),
            &nodes,
            &[],
            now,
        )
        .await
        .map_err(|e| map_driver_err(StatusCode::BAD_GATEWAY, "remote backend reconcile", e))?;
    Ok(Json(report))
}
