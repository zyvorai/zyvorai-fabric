// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: LicenseRef-Zyvor-Production-1.0

//! SCIM 2.0 provisioning API for enterprise identity providers.
//!
//! The `/scim/v2/*` endpoints use dedicated provisioning bearer tokens rather
//! than Fabric session JWTs. Token administration is protected by Fabric's
//! normal Admin RBAC and is intentionally separate from the SCIM data plane.

use axum::{
    extract::{Path, Query, State},
    http::{header, HeaderMap, StatusCode},
    Json,
};
use chrono::Utc;
use enterprise_identity::{
    apply_group_patch, apply_user_patch, group_matches_filter, mint_scim_token, parse_filter,
    resolve_role, to_scim_group, to_scim_user, token_matches, user_matches_filter,
    CreateProvisioningProfile, CreatedScimToken, ProvisionedGroup, ProvisionedUser,
    ProvisioningDecision, ProvisioningProfile, ScimErrorResponse, ScimGroupInput,
    ScimGroupResource, ScimListResponse, ScimPatchRequest, ScimTokenRecord, ScimTokenView,
    ScimUserInput, ScimUserResource, SCIM_GROUP_SCHEMA, SCIM_USER_SCHEMA,
};
use security::RequireAdmin;
use serde::Deserialize;
use serde_json::json;
use std::collections::HashSet;
use std::sync::Arc;

use crate::server::AppState;

type ScimApiError = (StatusCode, Json<ScimErrorResponse>);

fn scim_error(
    status: StatusCode,
    scim_type: Option<&str>,
    detail: impl Into<String>,
) -> ScimApiError {
    (
        status,
        Json(ScimErrorResponse::new(status.as_u16(), scim_type, detail)),
    )
}

fn store_error(e: impl std::fmt::Display) -> ScimApiError {
    tracing::error!("SCIM storage error: {e}");
    scim_error(
        StatusCode::INTERNAL_SERVER_ERROR,
        None,
        "Internal provisioning storage error",
    )
}

// ============================================================================
// Provisioning profiles and token administration (normal Fabric admin JWT)
// ============================================================================

pub async fn list_profiles(
    RequireAdmin(_claims): RequireAdmin,
    State(state): State<Arc<AppState>>,
) -> Result<Json<Vec<ProvisioningProfile>>, ScimApiError> {
    let profiles = state
        .store
        .list_entities::<ProvisioningProfile>("scim_profiles")
        .map_err(store_error)?;
    Ok(Json(profiles))
}

pub async fn create_profile(
    RequireAdmin(_claims): RequireAdmin,
    State(state): State<Arc<AppState>>,
    Json(req): Json<CreateProvisioningProfile>,
) -> Result<(StatusCode, Json<ProvisioningProfile>), ScimApiError> {
    if req.name.trim().is_empty() {
        return Err(scim_error(
            StatusCode::BAD_REQUEST,
            Some("invalidValue"),
            "Profile name cannot be empty",
        ));
    }

    if let Some(ref auth_provider_id) = req.auth_provider_id {
        // Keep this dependency loose: auth providers are persisted as JSON and
        // existence is enough to prevent typos/orphaned links.
        let providers: Vec<serde_json::Value> = state
            .store
            .list_entities("auth_providers")
            .map_err(store_error)?;
        let exists = providers.iter().any(|p| {
            p.get("id")
                .and_then(|v| v.as_str())
                .is_some_and(|v| v == auth_provider_id.as_str())
        });
        if !exists {
            return Err(scim_error(
                StatusCode::BAD_REQUEST,
                Some("invalidValue"),
                format!("Auth provider '{auth_provider_id}' does not exist"),
            ));
        }
    }

    let profile = ProvisioningProfile::create(req);
    state
        .store
        .save_entity("scim_profiles", &profile.id, &profile)
        .map_err(store_error)?;
    Ok((StatusCode::CREATED, Json(profile)))
}

pub async fn update_profile(
    RequireAdmin(_claims): RequireAdmin,
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    Json(req): Json<CreateProvisioningProfile>,
) -> Result<Json<ProvisioningProfile>, ScimApiError> {
    let current = state
        .store
        .get_entity::<ProvisioningProfile>("scim_profiles", &id)
        .map_err(store_error)?
        .ok_or_else(|| {
            scim_error(
                StatusCode::NOT_FOUND,
                None,
                "Provisioning profile not found",
            )
        })?;

    let updated = ProvisioningProfile {
        id: current.id,
        name: req.name,
        auth_provider_id: req.auth_provider_id,
        enabled: req.enabled,
        require_provisioned_user: req.require_provisioned_user,
        default_role: req.default_role,
        group_role_mapping: req.group_role_mapping,
        created: current.created,
        updated: Utc::now(),
    };

    state
        .store
        .save_entity("scim_profiles", &id, &updated)
        .map_err(store_error)?;

    // Profile role mappings may have changed; recompute every managed user.
    let users: Vec<ProvisionedUser> = state
        .store
        .list_entities("scim_users")
        .map_err(store_error)?;
    for user in users
        .into_iter()
        .filter(|u| u.profile_id == id && !u.deleted)
    {
        reconcile_user_role(&state, &updated, &user.id)?;
    }

    Ok(Json(updated))
}

pub async fn delete_profile(
    RequireAdmin(_claims): RequireAdmin,
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<StatusCode, ScimApiError> {
    let users: Vec<ProvisionedUser> = state
        .store
        .list_entities("scim_users")
        .map_err(store_error)?;
    let groups: Vec<ProvisionedGroup> = state
        .store
        .list_entities("scim_groups")
        .map_err(store_error)?;

    if users.iter().any(|u| u.profile_id == id && !u.deleted)
        || groups.iter().any(|g| g.profile_id == id && !g.deleted)
    {
        return Err(scim_error(
            StatusCode::CONFLICT,
            None,
            "Cannot delete a provisioning profile with active SCIM resources; disable it instead",
        ));
    }

    state
        .store
        .delete_entity("scim_profiles", &id)
        .map_err(store_error)?;
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateScimTokenRequest {
    pub profile_id: String,
    pub name: String,
}

pub async fn create_token(
    RequireAdmin(_claims): RequireAdmin,
    State(state): State<Arc<AppState>>,
    Json(req): Json<CreateScimTokenRequest>,
) -> Result<(StatusCode, Json<CreatedScimToken>), ScimApiError> {
    let _profile = state
        .store
        .get_entity::<ProvisioningProfile>("scim_profiles", &req.profile_id)
        .map_err(store_error)?
        .ok_or_else(|| {
            scim_error(
                StatusCode::NOT_FOUND,
                None,
                "Provisioning profile not found",
            )
        })?;

    if req.name.trim().is_empty() {
        return Err(scim_error(
            StatusCode::BAD_REQUEST,
            Some("invalidValue"),
            "Token name cannot be empty",
        ));
    }

    let (record, created) = mint_scim_token(&req.profile_id, &req.name);
    state
        .store
        .save_entity("scim_tokens", &record.id, &record)
        .map_err(store_error)?;
    Ok((StatusCode::CREATED, Json(created)))
}

pub async fn list_tokens(
    RequireAdmin(_claims): RequireAdmin,
    State(state): State<Arc<AppState>>,
) -> Result<Json<Vec<ScimTokenView>>, ScimApiError> {
    let records = state
        .store
        .list_entities::<ScimTokenRecord>("scim_tokens")
        .map_err(store_error)?;
    Ok(Json(records.iter().map(ScimTokenView::from).collect()))
}

pub async fn revoke_token(
    RequireAdmin(_claims): RequireAdmin,
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<StatusCode, ScimApiError> {
    let mut record = state
        .store
        .get_entity::<ScimTokenRecord>("scim_tokens", &id)
        .map_err(store_error)?
        .ok_or_else(|| scim_error(StatusCode::NOT_FOUND, None, "SCIM token not found"))?;
    record.revoked = true;
    state
        .store
        .save_entity("scim_tokens", &id, &record)
        .map_err(store_error)?;
    Ok(StatusCode::NO_CONTENT)
}

// ============================================================================
// SCIM bearer-token authentication
// ============================================================================

fn bearer_token(headers: &HeaderMap) -> Result<&str, ScimApiError> {
    headers
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .filter(|v| !v.trim().is_empty())
        .ok_or_else(|| scim_error(StatusCode::UNAUTHORIZED, None, "Missing SCIM bearer token"))
}

fn authorize_scim(
    state: &Arc<AppState>,
    headers: &HeaderMap,
) -> Result<(ScimTokenRecord, ProvisioningProfile), ScimApiError> {
    let secret = bearer_token(headers)?;
    let tokens = state
        .store
        .list_entities::<ScimTokenRecord>("scim_tokens")
        .map_err(store_error)?;
    let mut token = tokens
        .into_iter()
        .find(|record| token_matches(record, secret))
        .ok_or_else(|| scim_error(StatusCode::UNAUTHORIZED, None, "Invalid SCIM bearer token"))?;

    let profile = state
        .store
        .get_entity::<ProvisioningProfile>("scim_profiles", &token.profile_id)
        .map_err(store_error)?
        .ok_or_else(|| {
            scim_error(
                StatusCode::UNAUTHORIZED,
                None,
                "SCIM profile no longer exists",
            )
        })?;

    if !profile.enabled {
        return Err(scim_error(
            StatusCode::FORBIDDEN,
            None,
            "SCIM provisioning profile is disabled",
        ));
    }

    token.last_used = Some(Utc::now());
    if let Err(e) = state.store.save_entity("scim_tokens", &token.id, &token) {
        tracing::warn!("Failed to update SCIM token last_used: {e}");
    }
    Ok((token, profile))
}

// ============================================================================
// SCIM discovery endpoints
// ============================================================================

pub async fn service_provider_config(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
) -> Result<Json<serde_json::Value>, ScimApiError> {
    authorize_scim(&state, &headers)?;
    Ok(Json(json!({
        "schemas": ["urn:ietf:params:scim:schemas:core:2.0:ServiceProviderConfig"],
        "patch": { "supported": true },
        "bulk": { "supported": false, "maxOperations": 0, "maxPayloadSize": 0 },
        "filter": { "supported": true, "maxResults": 200 },
        "changePassword": { "supported": false },
        "sort": { "supported": false },
        "etag": { "supported": false },
        "authenticationSchemes": [{
            "type": "oauthbearertoken",
            "name": "SCIM Bearer Token",
            "description": "Zyvor Fabric provisioning token",
            "specUri": "https://www.rfc-editor.org/rfc/rfc6750"
        }]
    })))
}

pub async fn resource_types(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
) -> Result<Json<serde_json::Value>, ScimApiError> {
    authorize_scim(&state, &headers)?;
    Ok(Json(json!({
        "schemas": ["urn:ietf:params:scim:api:messages:2.0:ListResponse"],
        "totalResults": 2,
        "Resources": [
            {
                "schemas": ["urn:ietf:params:scim:schemas:core:2.0:ResourceType"],
                "id": "User",
                "name": "User",
                "endpoint": "/Users",
                "schema": SCIM_USER_SCHEMA,
                "schemaExtensions": [{
                    "schema": enterprise_identity::FABRIC_USER_EXTENSION,
                    "required": false
                }]
            },
            {
                "schemas": ["urn:ietf:params:scim:schemas:core:2.0:ResourceType"],
                "id": "Group",
                "name": "Group",
                "endpoint": "/Groups",
                "schema": SCIM_GROUP_SCHEMA
            }
        ]
    })))
}

pub async fn schemas(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
) -> Result<Json<serde_json::Value>, ScimApiError> {
    authorize_scim(&state, &headers)?;
    Ok(Json(json!({
        "schemas": ["urn:ietf:params:scim:api:messages:2.0:ListResponse"],
        "totalResults": 3,
        "Resources": [
            { "id": SCIM_USER_SCHEMA, "name": "User" },
            { "id": SCIM_GROUP_SCHEMA, "name": "Group" },
            { "id": enterprise_identity::FABRIC_USER_EXTENSION, "name": "Zyvor Fabric User" }
        ]
    })))
}

// ============================================================================
// Users
// ============================================================================

#[derive(Debug, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct ScimListQuery {
    #[serde(default)]
    pub filter: Option<String>,
    #[serde(default = "default_start_index")]
    pub start_index: usize,
    #[serde(default = "default_count")]
    pub count: usize,
}

fn default_start_index() -> usize {
    1
}
fn default_count() -> usize {
    100
}

fn scim_base_url() -> &'static str {
    "/scim/v2"
}

pub async fn create_user(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(input): Json<ScimUserInput>,
) -> Result<(StatusCode, Json<ScimUserResource>), ScimApiError> {
    let (_token, profile) = authorize_scim(&state, &headers)?;
    if input.user_name.trim().is_empty() {
        return Err(scim_error(
            StatusCode::BAD_REQUEST,
            Some("invalidValue"),
            "userName is required",
        ));
    }

    let users = users_for_profile(&state, &profile.id)?;
    if users
        .iter()
        .any(|u| !u.deleted && u.user_name.eq_ignore_ascii_case(&input.user_name))
    {
        return Err(scim_error(
            StatusCode::CONFLICT,
            Some("uniqueness"),
            "userName already exists",
        ));
    }
    if let Some(ref external_id) = input.external_id {
        if users
            .iter()
            .any(|u| !u.deleted && u.external_id.as_ref() == Some(external_id))
        {
            return Err(scim_error(
                StatusCode::CONFLICT,
                Some("uniqueness"),
                "externalId already exists",
            ));
        }
    }

    let user = ProvisionedUser::new(&profile, &input);
    state
        .store
        .save_entity("scim_users", &user.id, &user)
        .map_err(store_error)?;
    let groups = groups_for_profile(&state, &profile.id)?;
    Ok((
        StatusCode::CREATED,
        Json(to_scim_user(&user, &groups, scim_base_url())),
    ))
}

pub async fn list_users(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Query(q): Query<ScimListQuery>,
) -> Result<Json<ScimListResponse<ScimUserResource>>, ScimApiError> {
    let (_token, profile) = authorize_scim(&state, &headers)?;
    let groups = groups_for_profile(&state, &profile.id)?;
    let mut users = users_for_profile(&state, &profile.id)?;
    users.retain(|u| !u.deleted);

    if let Some(filter_raw) = q.filter.as_deref() {
        let filter = parse_filter(filter_raw).map_err(|e| {
            scim_error(
                StatusCode::BAD_REQUEST,
                Some("invalidFilter"),
                e.to_string(),
            )
        })?;
        users.retain(|u| user_matches_filter(u, &filter));
    }

    users.sort_by(|a, b| {
        a.user_name
            .to_ascii_lowercase()
            .cmp(&b.user_name.to_ascii_lowercase())
    });
    let total = users.len();
    let start = q.start_index.max(1) - 1;
    let count = q.count.clamp(1, 200);
    let resources = users
        .into_iter()
        .skip(start)
        .take(count)
        .map(|u| to_scim_user(&u, &groups, scim_base_url()))
        .collect();

    Ok(Json(ScimListResponse::new(total, start + 1, resources)))
}

pub async fn get_user(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Result<Json<ScimUserResource>, ScimApiError> {
    let (_token, profile) = authorize_scim(&state, &headers)?;
    let user = get_profile_user(&state, &profile.id, &id)?;
    if user.deleted {
        return Err(scim_error(StatusCode::NOT_FOUND, None, "User not found"));
    }
    let groups = groups_for_profile(&state, &profile.id)?;
    Ok(Json(to_scim_user(&user, &groups, scim_base_url())))
}

pub async fn replace_user(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(id): Path<String>,
    Json(input): Json<ScimUserInput>,
) -> Result<Json<ScimUserResource>, ScimApiError> {
    let (_token, profile) = authorize_scim(&state, &headers)?;
    let mut user = get_profile_user(&state, &profile.id, &id)?;
    if user.deleted {
        return Err(scim_error(StatusCode::NOT_FOUND, None, "User not found"));
    }

    user.external_id = input.external_id;
    user.user_name = input.user_name;
    user.name = input.name;
    user.display_name = input.display_name;
    user.emails = input.emails;
    user.active = input.active;
    user.updated = Utc::now();
    state
        .store
        .save_entity("scim_users", &id, &user)
        .map_err(store_error)?;
    reconcile_user_role(&state, &profile, &id)?;
    let user = get_profile_user(&state, &profile.id, &id)?;
    let groups = groups_for_profile(&state, &profile.id)?;
    Ok(Json(to_scim_user(&user, &groups, scim_base_url())))
}

pub async fn patch_user(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(id): Path<String>,
    Json(patch): Json<ScimPatchRequest>,
) -> Result<Json<ScimUserResource>, ScimApiError> {
    let (_token, profile) = authorize_scim(&state, &headers)?;
    let mut user = get_profile_user(&state, &profile.id, &id)?;
    if user.deleted {
        return Err(scim_error(StatusCode::NOT_FOUND, None, "User not found"));
    }
    apply_user_patch(&mut user, &patch)
        .map_err(|e| scim_error(StatusCode::BAD_REQUEST, Some("invalidValue"), e.to_string()))?;
    state
        .store
        .save_entity("scim_users", &id, &user)
        .map_err(store_error)?;
    let groups = groups_for_profile(&state, &profile.id)?;
    Ok(Json(to_scim_user(&user, &groups, scim_base_url())))
}

pub async fn delete_user(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Result<StatusCode, ScimApiError> {
    let (_token, profile) = authorize_scim(&state, &headers)?;
    let mut user = get_profile_user(&state, &profile.id, &id)?;
    user.active = false;
    user.deleted = true;
    user.group_ids.clear();
    user.updated = Utc::now();
    state
        .store
        .save_entity("scim_users", &id, &user)
        .map_err(store_error)?;

    let mut groups = groups_for_profile(&state, &profile.id)?;
    for group in &mut groups {
        if group.member_ids.iter().any(|member| member == &id) {
            group.member_ids.retain(|member| member != &id);
            group.updated = Utc::now();
            state
                .store
                .save_entity("scim_groups", &group.id, group)
                .map_err(store_error)?;
        }
    }
    Ok(StatusCode::NO_CONTENT)
}

// ============================================================================
// Groups
// ============================================================================

pub async fn create_group(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(input): Json<ScimGroupInput>,
) -> Result<(StatusCode, Json<ScimGroupResource>), ScimApiError> {
    let (_token, profile) = authorize_scim(&state, &headers)?;
    validate_group_members(
        &state,
        &profile.id,
        input.members.iter().map(|m| m.value.as_str()),
    )?;

    let groups = groups_for_profile(&state, &profile.id)?;
    if groups
        .iter()
        .any(|g| !g.deleted && g.display_name.eq_ignore_ascii_case(&input.display_name))
    {
        return Err(scim_error(
            StatusCode::CONFLICT,
            Some("uniqueness"),
            "Group displayName already exists",
        ));
    }

    let group = ProvisionedGroup::new(&profile.id, &input);
    state
        .store
        .save_entity("scim_groups", &group.id, &group)
        .map_err(store_error)?;
    reconcile_users(&state, &profile, group.member_ids.iter().cloned())?;
    let users = users_for_profile(&state, &profile.id)?;
    Ok((
        StatusCode::CREATED,
        Json(to_scim_group(&group, &users, scim_base_url())),
    ))
}

pub async fn list_groups(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Query(q): Query<ScimListQuery>,
) -> Result<Json<ScimListResponse<ScimGroupResource>>, ScimApiError> {
    let (_token, profile) = authorize_scim(&state, &headers)?;
    let users = users_for_profile(&state, &profile.id)?;
    let mut groups = groups_for_profile(&state, &profile.id)?;
    groups.retain(|g| !g.deleted);

    if let Some(filter_raw) = q.filter.as_deref() {
        let filter = parse_filter(filter_raw).map_err(|e| {
            scim_error(
                StatusCode::BAD_REQUEST,
                Some("invalidFilter"),
                e.to_string(),
            )
        })?;
        groups.retain(|g| group_matches_filter(g, &filter));
    }

    groups.sort_by(|a, b| {
        a.display_name
            .to_ascii_lowercase()
            .cmp(&b.display_name.to_ascii_lowercase())
    });
    let total = groups.len();
    let start = q.start_index.max(1) - 1;
    let count = q.count.clamp(1, 200);
    let resources = groups
        .into_iter()
        .skip(start)
        .take(count)
        .map(|g| to_scim_group(&g, &users, scim_base_url()))
        .collect();
    Ok(Json(ScimListResponse::new(total, start + 1, resources)))
}

pub async fn get_group(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Result<Json<ScimGroupResource>, ScimApiError> {
    let (_token, profile) = authorize_scim(&state, &headers)?;
    let group = get_profile_group(&state, &profile.id, &id)?;
    if group.deleted {
        return Err(scim_error(StatusCode::NOT_FOUND, None, "Group not found"));
    }
    let users = users_for_profile(&state, &profile.id)?;
    Ok(Json(to_scim_group(&group, &users, scim_base_url())))
}

pub async fn replace_group(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(id): Path<String>,
    Json(input): Json<ScimGroupInput>,
) -> Result<Json<ScimGroupResource>, ScimApiError> {
    let (_token, profile) = authorize_scim(&state, &headers)?;
    validate_group_members(
        &state,
        &profile.id,
        input.members.iter().map(|m| m.value.as_str()),
    )?;
    let mut group = get_profile_group(&state, &profile.id, &id)?;
    if group.deleted {
        return Err(scim_error(StatusCode::NOT_FOUND, None, "Group not found"));
    }
    let old_members: HashSet<String> = group.member_ids.iter().cloned().collect();
    group.external_id = input.external_id;
    group.display_name = input.display_name;
    group.member_ids = input.members.into_iter().map(|m| m.value).collect();
    group.updated = Utc::now();
    state
        .store
        .save_entity("scim_groups", &id, &group)
        .map_err(store_error)?;
    let affected = old_members
        .into_iter()
        .chain(group.member_ids.iter().cloned())
        .collect::<HashSet<_>>();
    reconcile_users(&state, &profile, affected.into_iter())?;
    let users = users_for_profile(&state, &profile.id)?;
    Ok(Json(to_scim_group(&group, &users, scim_base_url())))
}

pub async fn patch_group(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(id): Path<String>,
    Json(patch): Json<ScimPatchRequest>,
) -> Result<Json<ScimGroupResource>, ScimApiError> {
    let (_token, profile) = authorize_scim(&state, &headers)?;
    let mut group = get_profile_group(&state, &profile.id, &id)?;
    if group.deleted {
        return Err(scim_error(StatusCode::NOT_FOUND, None, "Group not found"));
    }
    let old_members: HashSet<String> = group.member_ids.iter().cloned().collect();
    apply_group_patch(&mut group, &patch)
        .map_err(|e| scim_error(StatusCode::BAD_REQUEST, Some("invalidValue"), e.to_string()))?;
    validate_group_members(
        &state,
        &profile.id,
        group.member_ids.iter().map(String::as_str),
    )?;
    state
        .store
        .save_entity("scim_groups", &id, &group)
        .map_err(store_error)?;
    let affected = old_members
        .into_iter()
        .chain(group.member_ids.iter().cloned())
        .collect::<HashSet<_>>();
    reconcile_users(&state, &profile, affected.into_iter())?;
    let users = users_for_profile(&state, &profile.id)?;
    Ok(Json(to_scim_group(&group, &users, scim_base_url())))
}

pub async fn delete_group(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Result<StatusCode, ScimApiError> {
    let (_token, profile) = authorize_scim(&state, &headers)?;
    let mut group = get_profile_group(&state, &profile.id, &id)?;
    let affected = group.member_ids.clone();
    group.member_ids.clear();
    group.deleted = true;
    group.updated = Utc::now();
    state
        .store
        .save_entity("scim_groups", &id, &group)
        .map_err(store_error)?;
    reconcile_users(&state, &profile, affected.into_iter())?;
    Ok(StatusCode::NO_CONTENT)
}

// ============================================================================
// Login integration
// ============================================================================

/// Evaluate SCIM provisioning policy for an external-auth login.
///
/// Call this after the OIDC/SAML/LDAP identity has been cryptographically
/// authenticated and `username` has been extracted. An enabled profile linked
/// to `auth_provider_id` may either override the role with the provisioned
/// effective role or deny login when the user is disabled/not provisioned.
pub fn provisioning_decision(
    state: &Arc<AppState>,
    auth_provider_id: &str,
    username: &str,
) -> anyhow::Result<ProvisioningDecision> {
    let profiles: Vec<ProvisioningProfile> = state.store.list_entities("scim_profiles")?;
    let profiles: Vec<_> = profiles
        .into_iter()
        .filter(|p| {
            p.enabled
                && p.auth_provider_id
                    .as_deref()
                    .is_some_and(|id| id == auth_provider_id)
        })
        .collect();

    if profiles.is_empty() {
        return Ok(ProvisioningDecision::NotManaged);
    }
    if profiles.len() > 1 {
        tracing::warn!(
            auth_provider_id,
            count = profiles.len(),
            "multiple SCIM profiles linked to one auth provider; using oldest"
        );
    }
    let profile = profiles
        .into_iter()
        .min_by(|a, b| a.created.cmp(&b.created))
        .expect("non-empty checked above");

    let users: Vec<ProvisionedUser> = state.store.list_entities("scim_users")?;
    let user = users
        .into_iter()
        .find(|u| u.profile_id == profile.id && u.user_name.eq_ignore_ascii_case(username));

    match user {
        Some(user) if !user.deleted && user.active => {
            Ok(ProvisioningDecision::Allow(user.effective_role))
        }
        Some(_) => Ok(ProvisioningDecision::Deny(
            "User has been deprovisioned by the enterprise identity provider".into(),
        )),
        None if profile.require_provisioned_user => Ok(ProvisioningDecision::Deny(
            "User is not provisioned for Zyvor Fabric".into(),
        )),
        None => Ok(ProvisioningDecision::NotManaged),
    }
}

// ============================================================================
// Persistence/reconciliation helpers
// ============================================================================

fn users_for_profile(
    state: &Arc<AppState>,
    profile_id: &str,
) -> Result<Vec<ProvisionedUser>, ScimApiError> {
    let users = state
        .store
        .list_entities::<ProvisionedUser>("scim_users")
        .map_err(store_error)?;
    Ok(users
        .into_iter()
        .filter(|u| u.profile_id == profile_id)
        .collect())
}

fn groups_for_profile(
    state: &Arc<AppState>,
    profile_id: &str,
) -> Result<Vec<ProvisionedGroup>, ScimApiError> {
    let groups = state
        .store
        .list_entities::<ProvisionedGroup>("scim_groups")
        .map_err(store_error)?;
    Ok(groups
        .into_iter()
        .filter(|g| g.profile_id == profile_id)
        .collect())
}

fn get_profile_user(
    state: &Arc<AppState>,
    profile_id: &str,
    id: &str,
) -> Result<ProvisionedUser, ScimApiError> {
    let user = state
        .store
        .get_entity::<ProvisionedUser>("scim_users", id)
        .map_err(store_error)?
        .ok_or_else(|| scim_error(StatusCode::NOT_FOUND, None, "User not found"))?;
    if user.profile_id != profile_id {
        return Err(scim_error(StatusCode::NOT_FOUND, None, "User not found"));
    }
    Ok(user)
}

fn get_profile_group(
    state: &Arc<AppState>,
    profile_id: &str,
    id: &str,
) -> Result<ProvisionedGroup, ScimApiError> {
    let group = state
        .store
        .get_entity::<ProvisionedGroup>("scim_groups", id)
        .map_err(store_error)?
        .ok_or_else(|| scim_error(StatusCode::NOT_FOUND, None, "Group not found"))?;
    if group.profile_id != profile_id {
        return Err(scim_error(StatusCode::NOT_FOUND, None, "Group not found"));
    }
    Ok(group)
}

fn validate_group_members<'a>(
    state: &Arc<AppState>,
    profile_id: &str,
    member_ids: impl Iterator<Item = &'a str>,
) -> Result<(), ScimApiError> {
    let users = users_for_profile(state, profile_id)?;
    for id in member_ids {
        if !users.iter().any(|u| u.id == id && !u.deleted) {
            return Err(scim_error(
                StatusCode::BAD_REQUEST,
                Some("invalidValue"),
                format!("Group member '{id}' is not an active resource in this profile"),
            ));
        }
    }
    Ok(())
}

fn reconcile_users(
    state: &Arc<AppState>,
    profile: &ProvisioningProfile,
    user_ids: impl Iterator<Item = String>,
) -> Result<(), ScimApiError> {
    let unique: HashSet<String> = user_ids.collect();
    for id in unique {
        // A user may have been deleted during the same transaction sequence.
        if let Ok(user) = get_profile_user(state, &profile.id, &id) {
            if !user.deleted {
                reconcile_user_role(state, profile, &id)?;
            }
        }
    }
    Ok(())
}

fn reconcile_user_role(
    state: &Arc<AppState>,
    profile: &ProvisioningProfile,
    user_id: &str,
) -> Result<(), ScimApiError> {
    let mut user = get_profile_user(state, &profile.id, user_id)?;
    if user.deleted {
        return Ok(());
    }
    let groups = groups_for_profile(state, &profile.id)?;
    let memberships: Vec<&ProvisionedGroup> = groups
        .iter()
        .filter(|g| !g.deleted && g.member_ids.iter().any(|id| id == user_id))
        .collect();

    user.group_ids = memberships.iter().map(|g| g.id.clone()).collect();
    user.effective_role = resolve_role(profile, memberships.iter().map(|g| g.display_name.clone()));
    user.updated = Utc::now();
    state
        .store
        .save_entity("scim_users", &user.id, &user)
        .map_err(store_error)?;
    Ok(())
}

// Compile-time smoke tests for structures that don't require AppState.
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn list_query_defaults_are_scim_friendly() {
        assert_eq!(default_start_index(), 1);
        assert_eq!(default_count(), 100);
    }

    #[test]
    fn error_has_scim_schema() {
        let e = ScimErrorResponse::new(400, Some("invalidValue"), "bad value");
        assert_eq!(e.status, "400");
        assert_eq!(e.schemas.len(), 1);
    }
}
