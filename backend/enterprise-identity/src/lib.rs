// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: LicenseRef-Zyvor-Production-1.0

//! Enterprise identity primitives for Zyvor Fabric.
//!
//! This crate deliberately contains no HTTP or persistence code. It provides
//! SCIM 2.0 resource models, provisioning models, filter/patch handling,
//! provisioning-token helpers, and deterministic role resolution. The API
//! server persists these types through Fabric's existing `StateStore`.

use chrono::{DateTime, Utc};
use security::Role;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::collections::{HashMap, HashSet};
use uuid::Uuid;

pub const SCIM_USER_SCHEMA: &str = "urn:ietf:params:scim:schemas:core:2.0:User";
pub const SCIM_GROUP_SCHEMA: &str = "urn:ietf:params:scim:schemas:core:2.0:Group";
pub const SCIM_LIST_SCHEMA: &str = "urn:ietf:params:scim:api:messages:2.0:ListResponse";
pub const SCIM_PATCH_SCHEMA: &str = "urn:ietf:params:scim:api:messages:2.0:PatchOp";
pub const SCIM_ERROR_SCHEMA: &str = "urn:ietf:params:scim:api:messages:2.0:Error";
pub const FABRIC_USER_EXTENSION: &str = "urn:zyvor:params:scim:schemas:extension:fabric:2.0:User";

#[derive(Debug, thiserror::Error)]
pub enum IdentityError {
    #[error("invalid SCIM filter: {0}")]
    InvalidFilter(String),
    #[error("invalid SCIM patch: {0}")]
    InvalidPatch(String),
    #[error("invalid role: {0}")]
    InvalidRole(String),
    #[error("resource not found: {0}")]
    NotFound(String),
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ScimName {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub formatted: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub family_name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub given_name: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ScimEmail {
    pub value: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub r#type: Option<String>,
    #[serde(default)]
    pub primary: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ScimGroupRef {
    pub value: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub display: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct FabricUserExtension {
    pub effective_role: Role,
    #[serde(default)]
    pub managed: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScimUserInput {
    #[serde(default = "default_user_schemas")]
    pub schemas: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub external_id: Option<String>,
    pub user_name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<ScimName>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub display_name: Option<String>,
    #[serde(default)]
    pub emails: Vec<ScimEmail>,
    #[serde(default = "default_true")]
    pub active: bool,
}

fn default_user_schemas() -> Vec<String> {
    vec![SCIM_USER_SCHEMA.to_string()]
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScimGroupInput {
    #[serde(default = "default_group_schemas")]
    pub schemas: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub external_id: Option<String>,
    pub display_name: String,
    #[serde(default)]
    pub members: Vec<ScimGroupMember>,
}

fn default_group_schemas() -> Vec<String> {
    vec![SCIM_GROUP_SCHEMA.to_string()]
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScimMeta {
    pub resource_type: String,
    pub created: DateTime<Utc>,
    pub last_modified: DateTime<Utc>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub location: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScimUserResource {
    pub schemas: Vec<String>,
    pub id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub external_id: Option<String>,
    pub user_name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<ScimName>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub display_name: Option<String>,
    #[serde(default)]
    pub emails: Vec<ScimEmail>,
    pub active: bool,
    #[serde(default)]
    pub groups: Vec<ScimGroupRef>,
    pub meta: ScimMeta,
    #[serde(
        rename = "urn:zyvor:params:scim:schemas:extension:fabric:2.0:User",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub fabric: Option<FabricUserExtension>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ScimGroupMember {
    pub value: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub display: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScimGroupResource {
    pub schemas: Vec<String>,
    pub id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub external_id: Option<String>,
    pub display_name: String,
    #[serde(default)]
    pub members: Vec<ScimGroupMember>,
    pub meta: ScimMeta,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScimListResponse<T> {
    pub schemas: Vec<String>,
    pub total_results: usize,
    pub start_index: usize,
    pub items_per_page: usize,
    #[serde(rename = "Resources")]
    pub resources: Vec<T>,
}

impl<T> ScimListResponse<T> {
    pub fn new(total: usize, start_index: usize, resources: Vec<T>) -> Self {
        Self {
            schemas: vec![SCIM_LIST_SCHEMA.to_string()],
            total_results: total,
            start_index,
            items_per_page: resources.len(),
            resources,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScimErrorResponse {
    pub schemas: Vec<String>,
    pub status: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scim_type: Option<String>,
    pub detail: String,
}

impl ScimErrorResponse {
    pub fn new(status: u16, scim_type: Option<&str>, detail: impl Into<String>) -> Self {
        Self {
            schemas: vec![SCIM_ERROR_SCHEMA.to_string()],
            status: status.to_string(),
            scim_type: scim_type.map(str::to_string),
            detail: detail.into(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScimPatchRequest {
    pub schemas: Vec<String>,
    #[serde(rename = "Operations")]
    pub operations: Vec<ScimPatchOperation>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScimPatchOperation {
    pub op: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub value: Option<Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProvisioningProfile {
    pub id: String,
    pub name: String,
    /// Existing Fabric auth-provider ID (OIDC/SAML/LDAP) this provisioning
    /// profile governs. When set, login can require a matching active SCIM user.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub auth_provider_id: Option<String>,
    pub enabled: bool,
    pub require_provisioned_user: bool,
    pub default_role: Role,
    /// Exact group display-name -> Fabric role mapping. Matching is
    /// case-insensitive; highest privilege wins if a user is in multiple groups.
    #[serde(default)]
    pub group_role_mapping: HashMap<String, Role>,
    pub created: DateTime<Utc>,
    pub updated: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateProvisioningProfile {
    pub name: String,
    #[serde(default)]
    pub auth_provider_id: Option<String>,
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default)]
    pub require_provisioned_user: bool,
    #[serde(default = "default_viewer_role")]
    pub default_role: Role,
    #[serde(default)]
    pub group_role_mapping: HashMap<String, Role>,
}

fn default_true() -> bool {
    true
}

fn default_viewer_role() -> Role {
    Role::Viewer
}

impl ProvisioningProfile {
    pub fn create(req: CreateProvisioningProfile) -> Self {
        let now = Utc::now();
        Self {
            id: Uuid::new_v4().to_string(),
            name: req.name,
            auth_provider_id: req.auth_provider_id,
            enabled: req.enabled,
            require_provisioned_user: req.require_provisioned_user,
            default_role: req.default_role,
            group_role_mapping: req.group_role_mapping,
            created: now,
            updated: now,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProvisionedUser {
    pub id: String,
    pub profile_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub external_id: Option<String>,
    pub user_name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<ScimName>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub display_name: Option<String>,
    #[serde(default)]
    pub emails: Vec<ScimEmail>,
    pub active: bool,
    #[serde(default)]
    pub group_ids: Vec<String>,
    pub effective_role: Role,
    #[serde(default)]
    pub deleted: bool,
    pub created: DateTime<Utc>,
    pub updated: DateTime<Utc>,
}

impl ProvisionedUser {
    pub fn new(profile: &ProvisioningProfile, resource: &ScimUserInput) -> Self {
        let now = Utc::now();
        Self {
            id: Uuid::new_v4().to_string(),
            profile_id: profile.id.clone(),
            external_id: resource.external_id.clone(),
            user_name: resource.user_name.clone(),
            name: resource.name.clone(),
            display_name: resource.display_name.clone(),
            emails: resource.emails.clone(),
            active: resource.active,
            group_ids: Vec::new(),
            effective_role: profile.default_role.clone(),
            deleted: false,
            created: now,
            updated: now,
        }
    }

    pub fn primary_email(&self) -> Option<&str> {
        self.emails
            .iter()
            .find(|e| e.primary)
            .or_else(|| self.emails.first())
            .map(|e| e.value.as_str())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProvisionedGroup {
    pub id: String,
    pub profile_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub external_id: Option<String>,
    pub display_name: String,
    #[serde(default)]
    pub member_ids: Vec<String>,
    #[serde(default)]
    pub deleted: bool,
    pub created: DateTime<Utc>,
    pub updated: DateTime<Utc>,
}

impl ProvisionedGroup {
    pub fn new(profile_id: &str, resource: &ScimGroupInput) -> Self {
        let now = Utc::now();
        Self {
            id: Uuid::new_v4().to_string(),
            profile_id: profile_id.to_string(),
            external_id: resource.external_id.clone(),
            display_name: resource.display_name.clone(),
            member_ids: resource.members.iter().map(|m| m.value.clone()).collect(),
            deleted: false,
            created: now,
            updated: now,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScimTokenRecord {
    pub id: String,
    pub profile_id: String,
    pub name: String,
    /// SHA-256 hex of the bearer token. Plaintext is returned only once.
    pub token_hash: String,
    pub created: DateTime<Utc>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_used: Option<DateTime<Utc>>,
    #[serde(default)]
    pub revoked: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScimTokenView {
    pub id: String,
    pub profile_id: String,
    pub name: String,
    pub created: DateTime<Utc>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_used: Option<DateTime<Utc>>,
    pub revoked: bool,
}

impl From<&ScimTokenRecord> for ScimTokenView {
    fn from(value: &ScimTokenRecord) -> Self {
        Self {
            id: value.id.clone(),
            profile_id: value.profile_id.clone(),
            name: value.name.clone(),
            created: value.created,
            last_used: value.last_used,
            revoked: value.revoked,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreatedScimToken {
    pub record: ScimTokenView,
    /// Secret shown once. Never persist this value.
    pub token: String,
}

pub fn mint_scim_token(profile_id: &str, name: &str) -> (ScimTokenRecord, CreatedScimToken) {
    let secret = format!(
        "fscim_{}_{}",
        Uuid::new_v4().simple(),
        Uuid::new_v4().simple()
    );
    let now = Utc::now();
    let record = ScimTokenRecord {
        id: Uuid::new_v4().to_string(),
        profile_id: profile_id.to_string(),
        name: name.to_string(),
        token_hash: hash_token(&secret),
        created: now,
        last_used: None,
        revoked: false,
    };
    let created = CreatedScimToken {
        record: ScimTokenView::from(&record),
        token: secret,
    };
    (record, created)
}

pub fn hash_token(token: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(token.as_bytes());
    // sha2 0.11 / digest hybrid-array: GenericArray no longer implements LowerHex.
    hasher
        .finalize()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

/// Constant-time compare for two equal-length ASCII strings.
pub fn constant_time_eq(a: &str, b: &str) -> bool {
    if a.len() != b.len() {
        return false;
    }
    let mut diff = 0u8;
    for (x, y) in a.as_bytes().iter().zip(b.as_bytes()) {
        diff |= x ^ y;
    }
    diff == 0
}

pub fn token_matches(record: &ScimTokenRecord, plaintext: &str) -> bool {
    !record.revoked && constant_time_eq(&record.token_hash, &hash_token(plaintext))
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScimFilter {
    pub attribute: String,
    pub value: String,
}

/// Parse the subset of SCIM filters required by Entra ID / Okta discovery:
/// `userName eq "alice"`, `externalId eq "123"`, `displayName eq "Ops"`,
/// or `id eq "..."`.
pub fn parse_filter(raw: &str) -> Result<ScimFilter, IdentityError> {
    let raw = raw.trim();
    let mut parts = raw.splitn(3, char::is_whitespace).filter(|s| !s.is_empty());
    let attribute = parts
        .next()
        .ok_or_else(|| IdentityError::InvalidFilter(raw.to_string()))?;
    let op = parts
        .next()
        .ok_or_else(|| IdentityError::InvalidFilter(raw.to_string()))?;
    let value = parts
        .next()
        .ok_or_else(|| IdentityError::InvalidFilter(raw.to_string()))?
        .trim();

    if !op.eq_ignore_ascii_case("eq") {
        return Err(IdentityError::InvalidFilter(format!(
            "only 'eq' is supported, got {op}"
        )));
    }

    let allowed = ["userName", "externalId", "displayName", "id"];
    if !allowed.iter().any(|a| a.eq_ignore_ascii_case(attribute)) {
        return Err(IdentityError::InvalidFilter(format!(
            "unsupported attribute {attribute}"
        )));
    }

    let value = if value.len() >= 2 && value.starts_with('"') && value.ends_with('"') {
        value[1..value.len() - 1].replace("\\\"", "\"")
    } else {
        value.to_string()
    };

    Ok(ScimFilter {
        attribute: attribute.to_string(),
        value,
    })
}

pub fn user_matches_filter(user: &ProvisionedUser, filter: &ScimFilter) -> bool {
    match filter.attribute.to_ascii_lowercase().as_str() {
        "username" => user.user_name.eq_ignore_ascii_case(&filter.value),
        "externalid" => user
            .external_id
            .as_deref()
            .is_some_and(|v| v == filter.value),
        "displayname" => user
            .display_name
            .as_deref()
            .is_some_and(|v| v.eq_ignore_ascii_case(&filter.value)),
        "id" => user.id == filter.value,
        _ => false,
    }
}

pub fn group_matches_filter(group: &ProvisionedGroup, filter: &ScimFilter) -> bool {
    match filter.attribute.to_ascii_lowercase().as_str() {
        "externalid" => group
            .external_id
            .as_deref()
            .is_some_and(|v| v == filter.value),
        "displayname" => group.display_name.eq_ignore_ascii_case(&filter.value),
        "id" => group.id == filter.value,
        _ => false,
    }
}

fn parse_emails(value: &Value) -> Result<Vec<ScimEmail>, IdentityError> {
    serde_json::from_value(value.clone())
        .map_err(|e| IdentityError::InvalidPatch(format!("invalid emails: {e}")))
}

pub fn apply_user_patch(
    user: &mut ProvisionedUser,
    patch: &ScimPatchRequest,
) -> Result<(), IdentityError> {
    if !patch.schemas.iter().any(|s| s == SCIM_PATCH_SCHEMA) {
        return Err(IdentityError::InvalidPatch(
            "PatchOp schema is required".to_string(),
        ));
    }

    for op in &patch.operations {
        let action = op.op.to_ascii_lowercase();
        let path = op.path.as_deref().unwrap_or("").trim();

        match (action.as_str(), path) {
            ("replace" | "add", "active") => {
                let active =
                    op.value.as_ref().and_then(Value::as_bool).ok_or_else(|| {
                        IdentityError::InvalidPatch("active must be boolean".into())
                    })?;
                user.active = active;
            }
            ("replace" | "add", "userName") => {
                let name =
                    op.value.as_ref().and_then(Value::as_str).ok_or_else(|| {
                        IdentityError::InvalidPatch("userName must be string".into())
                    })?;
                if name.trim().is_empty() {
                    return Err(IdentityError::InvalidPatch(
                        "userName cannot be empty".into(),
                    ));
                }
                user.user_name = name.to_string();
            }
            ("replace" | "add", "displayName") => {
                let display = op.value.as_ref().and_then(Value::as_str).ok_or_else(|| {
                    IdentityError::InvalidPatch("displayName must be string".into())
                })?;
                user.display_name = Some(display.to_string());
            }
            ("remove", "displayName") => user.display_name = None,
            ("replace" | "add", "emails") => {
                let value = op
                    .value
                    .as_ref()
                    .ok_or_else(|| IdentityError::InvalidPatch("emails value missing".into()))?;
                user.emails = parse_emails(value)?;
            }
            ("remove", "emails") => user.emails.clear(),
            ("replace" | "add", "name.givenName") => {
                let value = op.value.as_ref().and_then(Value::as_str).ok_or_else(|| {
                    IdentityError::InvalidPatch("givenName must be string".into())
                })?;
                user.name
                    .get_or_insert(ScimName {
                        formatted: None,
                        family_name: None,
                        given_name: None,
                    })
                    .given_name = Some(value.to_string());
            }
            ("replace" | "add", "name.familyName") => {
                let value = op.value.as_ref().and_then(Value::as_str).ok_or_else(|| {
                    IdentityError::InvalidPatch("familyName must be string".into())
                })?;
                user.name
                    .get_or_insert(ScimName {
                        formatted: None,
                        family_name: None,
                        given_name: None,
                    })
                    .family_name = Some(value.to_string());
            }
            // Entra may send a path-less replace object.
            ("replace" | "add", "") => {
                let obj = op
                    .value
                    .as_ref()
                    .and_then(Value::as_object)
                    .ok_or_else(|| IdentityError::InvalidPatch("value must be object".into()))?;
                if let Some(v) = obj.get("active").and_then(Value::as_bool) {
                    user.active = v;
                }
                if let Some(v) = obj.get("userName").and_then(Value::as_str) {
                    if !v.trim().is_empty() {
                        user.user_name = v.to_string();
                    }
                }
                if let Some(v) = obj.get("displayName").and_then(Value::as_str) {
                    user.display_name = Some(v.to_string());
                }
                if let Some(v) = obj.get("emails") {
                    user.emails = parse_emails(v)?;
                }
            }
            _ => {
                return Err(IdentityError::InvalidPatch(format!(
                    "unsupported operation '{}' on path '{}'",
                    op.op, path
                )))
            }
        }
    }

    user.updated = Utc::now();
    Ok(())
}

fn members_from_value(value: &Value) -> Result<Vec<String>, IdentityError> {
    let members: Vec<ScimGroupMember> = serde_json::from_value(value.clone())
        .map_err(|e| IdentityError::InvalidPatch(format!("invalid members: {e}")))?;
    Ok(members.into_iter().map(|m| m.value).collect())
}

pub fn apply_group_patch(
    group: &mut ProvisionedGroup,
    patch: &ScimPatchRequest,
) -> Result<(), IdentityError> {
    if !patch.schemas.iter().any(|s| s == SCIM_PATCH_SCHEMA) {
        return Err(IdentityError::InvalidPatch(
            "PatchOp schema is required".to_string(),
        ));
    }

    for op in &patch.operations {
        let action = op.op.to_ascii_lowercase();
        let path = op.path.as_deref().unwrap_or("").trim();

        if path == "displayName" {
            match action.as_str() {
                "add" | "replace" => {
                    let value = op.value.as_ref().and_then(Value::as_str).ok_or_else(|| {
                        IdentityError::InvalidPatch("displayName must be string".into())
                    })?;
                    group.display_name = value.to_string();
                }
                _ => {
                    return Err(IdentityError::InvalidPatch(
                        "displayName cannot be removed".into(),
                    ))
                }
            }
            continue;
        }

        if path == "members" || path.is_empty() {
            match action.as_str() {
                "add" => {
                    let value = op.value.as_ref().ok_or_else(|| {
                        IdentityError::InvalidPatch("members value missing".into())
                    })?;
                    let members_value = if path.is_empty() {
                        value.get("members").unwrap_or(value)
                    } else {
                        value
                    };
                    for id in members_from_value(members_value)? {
                        if !group.member_ids.contains(&id) {
                            group.member_ids.push(id);
                        }
                    }
                }
                "replace" => {
                    let value = op.value.as_ref().ok_or_else(|| {
                        IdentityError::InvalidPatch("members value missing".into())
                    })?;
                    let members_value = if path.is_empty() {
                        value.get("members").unwrap_or(value)
                    } else {
                        value
                    };
                    group.member_ids = members_from_value(members_value)?;
                }
                "remove" => {
                    if let Some(value) = &op.value {
                        let remove = members_from_value(value)?;
                        let remove: HashSet<_> = remove.into_iter().collect();
                        group.member_ids.retain(|id| !remove.contains(id));
                    } else {
                        group.member_ids.clear();
                    }
                }
                _ => {
                    return Err(IdentityError::InvalidPatch(format!(
                        "unsupported group patch operation {}",
                        op.op
                    )))
                }
            }
        } else if let Some(member_id) = parse_member_remove_path(path) {
            if action != "remove" {
                return Err(IdentityError::InvalidPatch(
                    "filtered members path only supports remove".into(),
                ));
            }
            group.member_ids.retain(|id| id != &member_id);
        } else {
            return Err(IdentityError::InvalidPatch(format!(
                "unsupported group patch path {path}"
            )));
        }
    }

    group.updated = Utc::now();
    Ok(())
}

/// Parse `members[value eq "<id>"]`, the form emitted by Entra/Okta for
/// individual membership removals.
pub fn parse_member_remove_path(path: &str) -> Option<String> {
    let prefix = "members[value eq \"";
    let suffix = "\"]";
    path.strip_prefix(prefix)
        .and_then(|s| s.strip_suffix(suffix))
        .map(str::to_string)
}

fn role_rank(role: &Role) -> u8 {
    match role {
        Role::Viewer => 0,
        Role::User => 1,
        Role::Admin => 2,
    }
}

/// Resolve effective role from group memberships. A profile's default role is
/// the floor; mapped group roles can only raise/lower according to the highest
/// matching mapped role. Matching group names is case-insensitive.
pub fn resolve_role(
    profile: &ProvisioningProfile,
    groups: impl IntoIterator<Item = String>,
) -> Role {
    let mut role = profile.default_role.clone();
    let mut rank = role_rank(&role);

    for group in groups {
        if let Some(mapped) = profile
            .group_role_mapping
            .iter()
            .find(|(name, _)| name.eq_ignore_ascii_case(&group))
            .map(|(_, role)| role)
        {
            let candidate_rank = role_rank(mapped);
            if candidate_rank > rank {
                rank = candidate_rank;
                role = mapped.clone();
            }
        }
    }
    role
}

#[derive(Debug, Clone, PartialEq)]
pub enum ProvisioningDecision {
    /// No enabled provisioning profile governs this auth provider.
    NotManaged,
    /// The user is provisioned and active; use this role.
    Allow(Role),
    /// Provisioning governs this provider and login must be refused.
    Deny(String),
}

pub fn to_scim_user(
    user: &ProvisionedUser,
    groups: &[ProvisionedGroup],
    base_url: &str,
) -> ScimUserResource {
    let group_refs = groups
        .iter()
        .filter(|g| !g.deleted && g.member_ids.contains(&user.id))
        .map(|g| ScimGroupRef {
            value: g.id.clone(),
            display: Some(g.display_name.clone()),
        })
        .collect();

    ScimUserResource {
        schemas: vec![
            SCIM_USER_SCHEMA.to_string(),
            FABRIC_USER_EXTENSION.to_string(),
        ],
        id: user.id.clone(),
        external_id: user.external_id.clone(),
        user_name: user.user_name.clone(),
        name: user.name.clone(),
        display_name: user.display_name.clone(),
        emails: user.emails.clone(),
        active: user.active && !user.deleted,
        groups: group_refs,
        meta: ScimMeta {
            resource_type: "User".into(),
            created: user.created,
            last_modified: user.updated,
            location: Some(format!(
                "{}/Users/{}",
                base_url.trim_end_matches('/'),
                user.id
            )),
        },
        fabric: Some(FabricUserExtension {
            effective_role: user.effective_role.clone(),
            managed: true,
        }),
    }
}

pub fn to_scim_group(
    group: &ProvisionedGroup,
    users: &[ProvisionedUser],
    base_url: &str,
) -> ScimGroupResource {
    let members = group
        .member_ids
        .iter()
        .filter_map(|id| users.iter().find(|u| &u.id == id && !u.deleted))
        .map(|u| ScimGroupMember {
            value: u.id.clone(),
            display: Some(u.user_name.clone()),
        })
        .collect();

    ScimGroupResource {
        schemas: vec![SCIM_GROUP_SCHEMA.to_string()],
        id: group.id.clone(),
        external_id: group.external_id.clone(),
        display_name: group.display_name.clone(),
        members,
        meta: ScimMeta {
            resource_type: "Group".into(),
            created: group.created,
            last_modified: group.updated,
            location: Some(format!(
                "{}/Groups/{}",
                base_url.trim_end_matches('/'),
                group.id
            )),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn profile() -> ProvisioningProfile {
        ProvisioningProfile {
            id: "p1".into(),
            name: "entra".into(),
            auth_provider_id: Some("oidc-1".into()),
            enabled: true,
            require_provisioned_user: true,
            default_role: Role::Viewer,
            group_role_mapping: HashMap::from([
                ("Fabric Admins".into(), Role::Admin),
                ("Fabric Operators".into(), Role::User),
            ]),
            created: Utc::now(),
            updated: Utc::now(),
        }
    }

    fn user() -> ProvisionedUser {
        ProvisionedUser {
            id: "u1".into(),
            profile_id: "p1".into(),
            external_id: Some("ext-1".into()),
            user_name: "alice@example.com".into(),
            name: None,
            display_name: Some("Alice".into()),
            emails: vec![],
            active: true,
            group_ids: vec![],
            effective_role: Role::Viewer,
            deleted: false,
            created: Utc::now(),
            updated: Utc::now(),
        }
    }

    #[test]
    fn parses_entra_filter() {
        let f = parse_filter("userName eq \"alice@example.com\"").unwrap();
        assert_eq!(f.attribute, "userName");
        assert_eq!(f.value, "alice@example.com");
        assert!(user_matches_filter(&user(), &f));
    }

    #[test]
    fn rejects_unsupported_filter_operator() {
        assert!(parse_filter("userName co \"alice\"").is_err());
    }

    #[test]
    fn highest_group_role_wins() {
        let p = profile();
        assert_eq!(
            resolve_role(&p, vec!["Fabric Operators".into(), "Fabric Admins".into()]),
            Role::Admin
        );
    }

    #[test]
    fn token_is_hash_only_and_matches() {
        let (record, created) = mint_scim_token("p1", "entra-token");
        assert_ne!(record.token_hash, created.token);
        assert!(token_matches(&record, &created.token));
        assert!(!token_matches(&record, "wrong"));
    }

    #[test]
    fn user_patch_deactivates() {
        let mut u = user();
        let patch = ScimPatchRequest {
            schemas: vec![SCIM_PATCH_SCHEMA.into()],
            operations: vec![ScimPatchOperation {
                op: "Replace".into(),
                path: Some("active".into()),
                value: Some(Value::Bool(false)),
            }],
        };
        apply_user_patch(&mut u, &patch).unwrap();
        assert!(!u.active);
    }

    #[test]
    fn group_filtered_remove_is_parsed() {
        assert_eq!(
            parse_member_remove_path("members[value eq \"u-123\"]"),
            Some("u-123".into())
        );
    }
}
