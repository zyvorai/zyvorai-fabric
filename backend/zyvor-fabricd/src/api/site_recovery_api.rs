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
use replication::ReplicationSite;
use security::{RequireAdmin, RequireRead, RequireWrite};
use site_recovery::{
    DrDashboard, DrHealth, DrSiteHealth, ExecutionStatus, ExecutionType, PlanStatus,
    RecoveryExecution, RecoveryPlan, SiteRecoveryManager,
};

// ============================================================================
// Recovery plan handlers
// ============================================================================

pub async fn list_plans(
    RequireRead(_claims): RequireRead,
    State(state): State<Arc<AppState>>,
) -> impl IntoResponse {
    tracing::debug!("site_recovery_api::{}", stringify!(list_plans));
    let items: Vec<RecoveryPlan> =
        state
            .store
            .list_entities("recovery_plans")
            .unwrap_or_else(|e| {
                tracing::error!("Storage error: {}", e);
                Vec::new()
            });
    Json(items)
}

pub async fn create_plan(
    RequireWrite(_claims): RequireWrite,
    State(state): State<Arc<AppState>>,
    Json(mut plan): Json<RecoveryPlan>,
) -> impl IntoResponse {
    tracing::debug!("site_recovery_api::{}", stringify!(create_plan));
    if let Err((status, msg)) = crate::validation::validate_entity_name(&plan.name) {
        return (status, Json(serde_json::json!({"error": msg}))).into_response();
    }
    plan.id = Uuid::new_v4().to_string();
    plan.created = Utc::now();
    plan.updated = None;
    plan.status = PlanStatus::Ready;
    plan.priority_groups.sort_by_key(|g| g.priority);
    match state.store.save_entity("recovery_plans", &plan.id, &plan) {
        Ok(_) => (StatusCode::CREATED, Json(plan)).into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": e.to_string()})),
        )
            .into_response(),
    }
}

pub async fn get_plan(
    RequireRead(_claims): RequireRead,
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    tracing::debug!("site_recovery_api::{}", stringify!(get_plan));
    match state
        .store
        .get_entity::<RecoveryPlan>("recovery_plans", &id)
    {
        Ok(Some(p)) => Json(p).into_response(),
        Ok(None) => (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({"error": "Recovery plan not found"})),
        )
            .into_response(),
        Err(_) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": "Internal server error"})),
        )
            .into_response(),
    }
}

pub async fn update_plan(
    RequireWrite(_claims): RequireWrite,
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    Json(mut plan): Json<RecoveryPlan>,
) -> impl IntoResponse {
    tracing::debug!("site_recovery_api::{}", stringify!(update_plan));
    let existing = match state
        .store
        .get_entity::<RecoveryPlan>("recovery_plans", &id)
    {
        Ok(Some(p)) => p,
        Ok(None) => {
            return (
                StatusCode::NOT_FOUND,
                Json(serde_json::json!({"error": "Recovery plan not found"})),
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
    plan.id = existing.id;
    plan.created = existing.created;
    plan.updated = Some(Utc::now());
    plan.priority_groups.sort_by_key(|g| g.priority);
    if let Err(e) = state.store.save_entity("recovery_plans", &id, &plan) {
        tracing::error!("Failed to save entity: {}", e);
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": e.to_string()})),
        )
            .into_response();
    }
    Json(plan).into_response()
}

pub async fn delete_plan(
    RequireAdmin(_claims): RequireAdmin,
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    tracing::debug!("site_recovery_api::{}", stringify!(delete_plan));
    if let Err(e) = state.store.delete_entity("recovery_plans", &id) {
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
// Execution handlers
// ============================================================================

fn start_execution(
    state: &Arc<AppState>,
    plan_id: &str,
    exec_type: ExecutionType,
) -> Result<RecoveryExecution, (StatusCode, Json<serde_json::Value>)> {
    let plan = state
        .store
        .get_entity::<RecoveryPlan>("recovery_plans", plan_id)
        .map_err(|_| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"error": "Internal server error"})),
            )
        })?
        .ok_or_else(|| {
            (
                StatusCode::NOT_FOUND,
                Json(serde_json::json!({"error": "Recovery plan not found"})),
            )
        })?;

    let steps = SiteRecoveryManager::generate_recovery_steps(&plan, exec_type.clone());
    let execution = RecoveryExecution {
        id: Uuid::new_v4().to_string(),
        plan_id: plan.id.clone(),
        plan_name: plan.name.clone(),
        execution_type: exec_type,
        status: ExecutionStatus::Running,
        started: Utc::now(),
        completed: None,
        steps,
        rto_actual_minutes: None,
        vms_recovered: 0,
        vms_failed: 0,
        error: None,
        initiated_by: "api".to_string(),
    };
    state
        .store
        .save_entity("recovery_executions", &execution.id, &execution)
        .map_err(|_| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"error": "Internal server error"})),
            )
        })?;
    Ok(execution)
}

pub async fn execute_planned_migration(
    RequireWrite(_claims): RequireWrite,
    State(state): State<Arc<AppState>>,
    Path(plan_id): Path<String>,
) -> impl IntoResponse {
    tracing::debug!(
        "site_recovery_api::{}",
        stringify!(execute_planned_migration)
    );
    match start_execution(&state, &plan_id, ExecutionType::PlannedMigration) {
        Ok(exec) => (StatusCode::CREATED, Json(exec)).into_response(),
        Err((sc, body)) => (sc, body).into_response(),
    }
}

pub async fn execute_disaster_recovery(
    RequireWrite(_claims): RequireWrite,
    State(state): State<Arc<AppState>>,
    Path(plan_id): Path<String>,
) -> impl IntoResponse {
    tracing::debug!(
        "site_recovery_api::{}",
        stringify!(execute_disaster_recovery)
    );
    match start_execution(&state, &plan_id, ExecutionType::DisasterRecovery) {
        Ok(exec) => (StatusCode::CREATED, Json(exec)).into_response(),
        Err((sc, body)) => (sc, body).into_response(),
    }
}

pub async fn execute_test_failover(
    RequireWrite(_claims): RequireWrite,
    State(state): State<Arc<AppState>>,
    Path(plan_id): Path<String>,
) -> impl IntoResponse {
    tracing::debug!("site_recovery_api::{}", stringify!(execute_test_failover));
    match start_execution(&state, &plan_id, ExecutionType::TestFailover) {
        Ok(exec) => (StatusCode::CREATED, Json(exec)).into_response(),
        Err((sc, body)) => (sc, body).into_response(),
    }
}

pub async fn execute_reprotect(
    RequireWrite(_claims): RequireWrite,
    State(state): State<Arc<AppState>>,
    Path(plan_id): Path<String>,
) -> impl IntoResponse {
    tracing::debug!("site_recovery_api::{}", stringify!(execute_reprotect));
    match start_execution(&state, &plan_id, ExecutionType::Reprotect) {
        Ok(exec) => (StatusCode::CREATED, Json(exec)).into_response(),
        Err((sc, body)) => (sc, body).into_response(),
    }
}

pub async fn list_executions(
    RequireRead(_claims): RequireRead,
    State(state): State<Arc<AppState>>,
) -> impl IntoResponse {
    tracing::debug!("site_recovery_api::{}", stringify!(list_executions));
    let items: Vec<RecoveryExecution> = state
        .store
        .list_entities("recovery_executions")
        .unwrap_or_else(|e| {
            tracing::error!("Storage error: {}", e);
            Vec::new()
        });
    Json(items)
}

pub async fn get_execution(
    RequireRead(_claims): RequireRead,
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    tracing::debug!("site_recovery_api::{}", stringify!(get_execution));
    match state
        .store
        .get_entity::<RecoveryExecution>("recovery_executions", &id)
    {
        Ok(Some(e)) => Json(e).into_response(),
        Ok(None) => (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({"error": "Recovery execution not found"})),
        )
            .into_response(),
        Err(_) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": "Internal server error"})),
        )
            .into_response(),
    }
}

pub async fn cancel_execution(
    RequireWrite(_claims): RequireWrite,
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    tracing::debug!("site_recovery_api::{}", stringify!(cancel_execution));
    let mut exec = match state
        .store
        .get_entity::<RecoveryExecution>("recovery_executions", &id)
    {
        Ok(Some(e)) => e,
        Ok(None) => {
            return (
                StatusCode::NOT_FOUND,
                Json(serde_json::json!({"error": "Recovery execution not found"})),
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
    exec.status = ExecutionStatus::Cancelled;
    exec.completed = Some(Utc::now());
    if let Err(e) = state
        .store
        .save_entity("recovery_executions", &exec.id, &exec)
    {
        tracing::error!("Failed to save entity: {}", e);
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": e.to_string()})),
        )
            .into_response();
    }
    (
        StatusCode::OK,
        Json(serde_json::json!({"status": "execution cancelled"})),
    )
        .into_response()
}

// ============================================================================
// Dashboard
// ============================================================================

pub async fn get_dr_dashboard(
    RequireRead(_claims): RequireRead,
    State(state): State<Arc<AppState>>,
) -> impl IntoResponse {
    tracing::debug!("site_recovery_api::{}", stringify!(get_dr_dashboard));
    let plans: Vec<RecoveryPlan> =
        state
            .store
            .list_entities("recovery_plans")
            .unwrap_or_else(|e| {
                tracing::error!("Storage error: {}", e);
                Vec::new()
            });
    let total_plans = plans.len() as u32;
    let ready_plans = plans
        .iter()
        .filter(|p| p.status == PlanStatus::Ready)
        .count() as u32;
    let failed_plans = plans
        .iter()
        .filter(|p| p.status == PlanStatus::Failed)
        .count() as u32;
    let mut protected_vms = std::collections::HashSet::new();
    for plan in &plans {
        for group in &plan.priority_groups {
            for vm in &group.vm_names {
                protected_vms.insert(vm.clone());
            }
        }
    }
    let rpo_violations = plans
        .iter()
        .filter(|p| p.last_tested.is_none() && p.last_executed.is_none())
        .count() as u32;
    let overall_health = if failed_plans > 0 {
        DrHealth::Critical
    } else if rpo_violations > 0 {
        DrHealth::Warning
    } else {
        DrHealth::Healthy
    };

    let plans_tested = plans.iter().filter(|p| p.last_tested.is_some()).count() as u32;
    let rto_targets: Vec<u32> = plans.iter().filter_map(|p| p.rto_minutes).collect();
    let avg_rto_seconds = if rto_targets.is_empty() {
        0.0
    } else {
        (rto_targets.iter().sum::<u32>() as f64 / rto_targets.len() as f64) * 60.0
    };

    // Site recovery plans reference sites by id (source_site_id/target_site_id)
    // from the same site registry replication uses — reuse it for names rather
    // than maintaining a second, redundant site list.
    let known_sites: Vec<ReplicationSite> = state
        .store
        .list_entities("replication_sites")
        .unwrap_or_else(|e| {
            tracing::error!("Storage error: {}", e);
            Vec::new()
        });
    let sites: Vec<DrSiteHealth> = known_sites
        .into_iter()
        .map(|s| {
            let site_plans: Vec<&RecoveryPlan> = plans
                .iter()
                .filter(|p| p.source_site_id == s.id || p.target_site_id == s.id)
                .collect();
            let site_protected_vms: std::collections::HashSet<&str> = site_plans
                .iter()
                .flat_map(|p| p.priority_groups.iter())
                .flat_map(|g| g.vm_names.iter())
                .map(|v| v.as_str())
                .collect();
            let status = if site_plans.iter().any(|p| p.status == PlanStatus::Failed) {
                "critical"
            } else if site_plans.is_empty() {
                "unused"
            } else {
                "healthy"
            };
            DrSiteHealth {
                site_id: s.id,
                site_name: s.name,
                status: status.to_string(),
                protected_vms: site_protected_vms.len() as u32,
                plans: site_plans.len() as u32,
            }
        })
        .collect();

    let mut recent_executions: Vec<RecoveryExecution> = state
        .store
        .list_entities("recovery_executions")
        .unwrap_or_else(|e| {
            tracing::error!("Storage error: {}", e);
            Vec::new()
        });
    recent_executions.sort_by_key(|a| std::cmp::Reverse(a.started));
    recent_executions.truncate(5);

    let dashboard = DrDashboard {
        total_plans,
        ready_plans,
        failed_plans,
        protected_vms: protected_vms.len() as u32,
        unprotected_vms: 0,
        rpo_violations,
        last_test_results: Vec::new(),
        overall_health,
        plans_tested,
        plans_untested: total_plans - plans_tested,
        avg_rto_seconds,
        avg_rpo_minutes: 0.0,
        sites,
        recent_executions,
    };
    Json(dashboard)
}
