// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: LicenseRef-Zyvor-Production-2.0

//! JWT tenant scoping for VM and ContainerGroup routes (mirrors FluxVM
//! token-tenant enforcement).

use axum::{
    extract::{Request, State},
    http::StatusCode,
    middleware::Next,
    response::{IntoResponse, Response},
    Json,
};
use security::Claims;
use serde_json::json;
use std::sync::Arc;
use vm_model::VM;

use crate::api::container_declarative::ContainerGroupSpec;
use crate::server::AppState;

pub fn vm_tenant(vm: &VM) -> Option<String> {
    if let Some(ref labels) = vm.labels {
        if let Some(t) = labels.get("tenant") {
            return Some(t.clone());
        }
    }
    vm.tags.as_ref().and_then(|tags| {
        tags.iter()
            .find_map(|t| t.strip_prefix("tenant:").map(|s| s.to_string()))
    })
}

/// When the JWT carries `tenant`, `/api/vms/{name}…` and
/// `/api/container-groups/{name}…` are scoped to that tenant.
pub async fn tenant_guard_middleware(
    State(state): State<Arc<AppState>>,
    req: Request,
    next: Next,
) -> Response {
    let Some(claims) = req.extensions().get::<Claims>().cloned() else {
        return next.run(req).await;
    };
    let Some(tenant) = claims.tenant.clone() else {
        return next.run(req).await;
    };

    let path = req.uri().path();
    // Paths are nested under /api or /api/v1 — strip those prefixes.
    let rest = path
        .strip_prefix("/api/v1")
        .or_else(|| path.strip_prefix("/api"))
        .unwrap_or(path);
    if let Some(name) = extract_vm_name(rest) {
        match state.store.get_vm(&name) {
            Ok(Some(vm)) if vm_tenant(&vm).as_deref() == Some(tenant.as_str()) => {}
            _ => {
                return (
                    StatusCode::NOT_FOUND,
                    Json(json!({"error": "VM not found"})),
                )
                    .into_response();
            }
        }
    }
    if let Some(name) = extract_container_group_name(rest) {
        match container_group_tenant(&state, &name) {
            Some(t) if t == tenant => {}
            _ => {
                return (
                    StatusCode::NOT_FOUND,
                    Json(json!({"error": "ContainerGroup not found"})),
                )
                    .into_response();
            }
        }
    }
    next.run(req).await
}

fn extract_vm_name(path: &str) -> Option<String> {
    let rest = path.strip_prefix("/vms/")?;
    let name = rest.split('/').next()?;
    if name.is_empty() || name == "compare" {
        return None;
    }
    Some(name.to_string())
}

fn extract_container_group_name(path: &str) -> Option<String> {
    let rest = path.strip_prefix("/container-groups/")?;
    let name = rest.split('/').next()?;
    if name.is_empty() || name == "apply" {
        return None;
    }
    Some(name.to_string())
}

fn container_group_tenant(state: &AppState, name: &str) -> Option<String> {
    state
        .store
        .get_entity::<ContainerGroupSpec>("container_groups", name)
        .ok()
        .flatten()
        .and_then(|spec| spec.tenant)
}

pub fn apply_create_tenant(
    claims: &Claims,
    mut tenant: Option<String>,
) -> Result<Option<String>, (StatusCode, String)> {
    if let Some(ref claim_tenant) = claims.tenant {
        if let Some(ref body) = tenant {
            if body != claim_tenant {
                return Err((
                    StatusCode::FORBIDDEN,
                    format!("token tenant '{claim_tenant}' cannot create VM for tenant '{body}'"),
                ));
            }
        }
        tenant = Some(claim_tenant.clone());
    }
    Ok(tenant)
}

pub fn apply_list_tenant_filter(
    claims: &Claims,
    query_tenant: Option<String>,
) -> Result<Option<String>, (StatusCode, String)> {
    if let Some(ref claim_tenant) = claims.tenant {
        if let Some(ref qt) = query_tenant {
            if qt != claim_tenant {
                return Err((
                    StatusCode::FORBIDDEN,
                    format!("token tenant '{claim_tenant}' cannot list tenant '{qt}'"),
                ));
            }
        }
        return Ok(Some(claim_tenant.clone()));
    }
    Ok(query_tenant)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn claims_with_tenant(tenant: Option<&str>) -> Claims {
        Claims {
            sub: "user-1".to_string(),
            role: security::Role::User,
            exp: 0,
            jti: String::new(),
            tenant: tenant.map(str::to_string),
        }
    }

    #[test]
    fn extract_container_group_name_matches_the_name_and_its_subpaths() {
        assert_eq!(
            extract_container_group_name("/container-groups/web"),
            Some("web".to_string())
        );
        assert_eq!(
            extract_container_group_name("/container-groups/web/spec"),
            Some("web".to_string())
        );
    }

    #[test]
    fn extract_container_group_name_excludes_the_apply_collection_route() {
        assert_eq!(
            extract_container_group_name("/container-groups/apply"),
            None
        );
    }

    #[test]
    fn extract_container_group_name_ignores_unrelated_paths() {
        assert_eq!(extract_container_group_name("/vms/web"), None);
        assert_eq!(
            extract_container_group_name("/container-group-events"),
            None
        );
    }

    #[test]
    fn apply_create_tenant_stamps_the_claim_tenant_when_the_body_omits_one() {
        let claims = claims_with_tenant(Some("acme"));
        assert_eq!(
            apply_create_tenant(&claims, None).unwrap(),
            Some("acme".to_string())
        );
    }

    #[test]
    fn apply_create_tenant_rejects_a_body_tenant_that_does_not_match_the_claim() {
        let claims = claims_with_tenant(Some("acme"));
        let err = apply_create_tenant(&claims, Some("other".to_string())).unwrap_err();
        assert_eq!(err.0, StatusCode::FORBIDDEN);
    }

    #[test]
    fn apply_create_tenant_leaves_a_body_tenant_alone_when_the_jwt_carries_none() {
        let claims = claims_with_tenant(None);
        assert_eq!(
            apply_create_tenant(&claims, Some("acme".to_string())).unwrap(),
            Some("acme".to_string())
        );
    }
}
