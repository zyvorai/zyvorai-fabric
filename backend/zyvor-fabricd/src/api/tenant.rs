// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: LicenseRef-Zyvor-Production-2.0

use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::sync::Arc;

use crate::server::AppState;
use security::{RequireAdmin, RequireRead};

// ============================================================================
// Multi-Tenancy — Project/Namespace Isolation
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Project {
    pub id: String,
    pub name: String,
    pub description: String,
    pub owner: String,
    pub members: Vec<ProjectMember>,
    pub quota: Option<ProjectQuota>,
    pub created: DateTime<Utc>,
    pub updated: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectMember {
    pub user_id: String,
    pub role: ProjectRole,
    pub added: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ProjectRole {
    Owner,
    Admin,
    Member,
    Viewer,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectQuota {
    pub max_vms: u32,
    pub max_cpus: u32,
    pub max_memory_mb: u64,
    pub max_storage_gb: u64,
}

#[derive(Debug, Deserialize)]
pub struct CreateProjectRequest {
    pub name: String,
    #[serde(default)]
    pub description: String,
    pub quota: Option<ProjectQuota>,
}

#[derive(Debug, Deserialize)]
pub struct AddMemberRequest {
    pub user_id: String,
    pub role: ProjectRole,
}

// ============================================================================
// Handlers
// ============================================================================

/// GET /api/projects - List all projects
pub async fn list_projects(
    RequireRead(_claims): RequireRead,
    State(state): State<Arc<AppState>>,
) -> Result<Json<Vec<Project>>, (StatusCode, Json<serde_json::Value>)> {
    let projects = state
        .store
        .list_entities::<Project>("projects")
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"error": e.to_string()})),
            )
        })?;
    Ok(Json(projects))
}

/// POST /api/projects - Create a project
pub async fn create_project(
    RequireAdmin(claims): RequireAdmin,
    State(state): State<Arc<AppState>>,
    Json(req): Json<CreateProjectRequest>,
) -> Result<(StatusCode, Json<Project>), (StatusCode, Json<serde_json::Value>)> {
    crate::validation::validate_entity_name(&req.name)
        .map_err(|(s, m)| (s, Json(json!({"error": m}))))?;
    let now = Utc::now();
    let project = Project {
        id: uuid::Uuid::new_v4().to_string(),
        name: req.name,
        description: req.description,
        owner: claims.sub.clone(),
        members: vec![ProjectMember {
            user_id: claims.sub,
            role: ProjectRole::Owner,
            added: now,
        }],
        quota: req.quota,
        created: now,
        updated: now,
    };

    state
        .store
        .save_entity("projects", &project.id, &project)
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"error": e.to_string()})),
            )
        })?;

    Ok((StatusCode::CREATED, Json(project)))
}

/// GET /api/projects/:id - Get project details
pub async fn get_project(
    RequireRead(_claims): RequireRead,
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<Project>, (StatusCode, Json<serde_json::Value>)> {
    let project = state
        .store
        .get_entity::<Project>("projects", &id)
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"error": e.to_string()})),
            )
        })?
        .ok_or_else(|| {
            (
                StatusCode::NOT_FOUND,
                Json(json!({"error": "Project not found"})),
            )
        })?;
    Ok(Json(project))
}

/// DELETE /api/projects/:id - Delete a project
pub async fn delete_project(
    RequireAdmin(_claims): RequireAdmin,
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<StatusCode, (StatusCode, Json<serde_json::Value>)> {
    state.store.delete_entity("projects", &id).map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error": e.to_string()})),
        )
    })?;
    Ok(StatusCode::NO_CONTENT)
}

/// POST /api/projects/:id/members - Add a member
pub async fn add_member(
    RequireAdmin(_claims): RequireAdmin,
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    Json(req): Json<AddMemberRequest>,
) -> Result<Json<Project>, (StatusCode, Json<serde_json::Value>)> {
    let mut project = state
        .store
        .get_entity::<Project>("projects", &id)
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"error": e.to_string()})),
            )
        })?
        .ok_or_else(|| {
            (
                StatusCode::NOT_FOUND,
                Json(json!({"error": "Project not found"})),
            )
        })?;

    // Limit total members to prevent unbounded growth
    if project.members.len() >= 1000 {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(json!({"error": "Project has reached the maximum number of members (1000)"})),
        ));
    }

    // Check if user already a member
    if project.members.iter().any(|m| m.user_id == req.user_id) {
        return Err((
            StatusCode::CONFLICT,
            Json(json!({"error": "User is already a member"})),
        ));
    }

    project.members.push(ProjectMember {
        user_id: req.user_id,
        role: req.role,
        added: Utc::now(),
    });
    project.updated = Utc::now();

    state
        .store
        .save_entity("projects", &project.id, &project)
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"error": e.to_string()})),
            )
        })?;

    Ok(Json(project))
}

/// DELETE /api/projects/:id/members/:user_id - Remove a member
pub async fn remove_member(
    RequireAdmin(_claims): RequireAdmin,
    State(state): State<Arc<AppState>>,
    Path((id, user_id)): Path<(String, String)>,
) -> Result<Json<Project>, (StatusCode, Json<serde_json::Value>)> {
    let mut project = state
        .store
        .get_entity::<Project>("projects", &id)
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"error": e.to_string()})),
            )
        })?
        .ok_or_else(|| {
            (
                StatusCode::NOT_FOUND,
                Json(json!({"error": "Project not found"})),
            )
        })?;

    // Cannot remove the project owner
    if user_id == project.owner {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(json!({"error": "Cannot remove the project owner. Transfer ownership first."})),
        ));
    }

    let before = project.members.len();
    project.members.retain(|m| m.user_id != user_id);

    if project.members.len() == before {
        return Err((
            StatusCode::NOT_FOUND,
            Json(json!({"error": "Member not found"})),
        ));
    }

    project.updated = Utc::now();
    state
        .store
        .save_entity("projects", &project.id, &project)
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"error": e.to_string()})),
            )
        })?;

    Ok(Json(project))
}

/// GET /api/projects/:id/vms - List VMs in a project
pub async fn list_project_vms(
    RequireRead(_claims): RequireRead,
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<Vec<vm_model::VM>>, (StatusCode, Json<serde_json::Value>)> {
    // Verify project exists
    let _project = state
        .store
        .get_entity::<Project>("projects", &id)
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"error": e.to_string()})),
            )
        })?
        .ok_or_else(|| {
            (
                StatusCode::NOT_FOUND,
                Json(json!({"error": "Project not found"})),
            )
        })?;

    // List VMs tagged with this project
    let all_vms = state.store.list_vms().map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error": e.to_string()})),
        )
    })?;

    let project_vms: Vec<vm_model::VM> = all_vms
        .into_iter()
        .filter(|vm| {
            vm.labels
                .as_ref()
                .map(|l| l.get("project") == Some(&id))
                .unwrap_or(false)
        })
        .collect();

    Ok(Json(project_vms))
}
