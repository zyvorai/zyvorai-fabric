// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: LicenseRef-Zyvor-Production-2.0

//! Tenant model policy. No policy record means the existing tenant filter stands.

use axum::{extract::State, http::StatusCode, Json};
use chrono::Timelike;
use std::sync::Arc;

use crate::server::AppState;

use super::STORE_POLICIES;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct TenantAiPolicy {
    pub id: String,
    #[serde(default)]
    pub allowed_models: Vec<String>,
    #[serde(default)]
    pub allowed_source_prefixes: Vec<String>,
    #[serde(default)]
    pub allowed_sites: Vec<String>,
    /// `0` means no GPU cap.
    #[serde(default)]
    pub max_gpus: u32,
    #[serde(default)]
    pub require_signature: bool,
    /// `0` means no context cap.
    #[serde(default)]
    pub max_context_tokens: u32,
    /// Prompts are not stored. This flag must stay false.
    #[serde(default)]
    pub prompt_logging: bool,
    /// Inclusive UTC hour. Unset together with `deploy_hour_end` means no window.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub deploy_hour_start: Option<u8>,
    /// Exclusive UTC hour, except when it equals the start, which allows that hour only.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub deploy_hour_end: Option<u8>,
}

pub struct DeployCheck<'a> {
    pub model: &'a str,
    pub source: &'a str,
    pub site: &'a str,
    pub gpus: u32,
    pub has_signature: bool,
    pub context_tokens: u32,
    pub hour: u8,
}

pub fn evaluate(policy: &TenantAiPolicy, check: &DeployCheck<'_>) -> Result<(), String> {
    if policy.prompt_logging {
        return Err("prompt logging is not permitted".into());
    }
    if !allows(policy, check.model, check.source) {
        return Err(format!(
            "tenant '{}' is not allowed to deploy model '{}'",
            policy.id, check.model
        ));
    }
    if !policy.allowed_sites.is_empty()
        && !check.site.is_empty()
        && !policy.allowed_sites.iter().any(|site| site == check.site)
    {
        return Err(format!("site '{}' is outside tenant policy", check.site));
    }
    if policy.max_gpus > 0 && check.gpus > policy.max_gpus {
        return Err(format!(
            "requested {} GPUs exceeds tenant cap {}",
            check.gpus, policy.max_gpus
        ));
    }
    if policy.require_signature && !check.has_signature {
        return Err("tenant policy requires a model signature".into());
    }
    if policy.max_context_tokens > 0 && check.context_tokens > policy.max_context_tokens {
        return Err("context length exceeds tenant policy".into());
    }
    deploy_window(policy.deploy_hour_start, policy.deploy_hour_end, check.hour)
        .map_err(|msg| msg.to_string())?;
    Ok(())
}

/// Both bounds absent means always open. A start after the end wraps past midnight.
pub fn deploy_window(start: Option<u8>, end: Option<u8>, hour: u8) -> Result<(), &'static str> {
    match (start, end) {
        (None, None) => Ok(()),
        (Some(start), Some(end)) if start <= 23 && end <= 23 && hour <= 23 => {
            let open = if start == end {
                hour == start
            } else if start < end {
                hour >= start && hour < end
            } else {
                hour >= start || hour < end
            };
            if open {
                Ok(())
            } else {
                Err("deployment is outside the tenant time window")
            }
        }
        _ => Err("tenant deploy window hours must be 0 to 23"),
    }
}

pub fn rbac_allows(granted: &[&str], action: &str) -> bool {
    const ACTIONS: &[&str] = &[
        "create",
        "read",
        "update",
        "delete",
        "deploy",
        "scale",
        "infer",
        "promote",
        "rollback",
        "administer",
    ];
    if !ACTIONS.contains(&action) {
        return false;
    }
    granted
        .iter()
        .any(|grant| *grant == action || *grant == "administer")
}

pub fn allows(policy: &TenantAiPolicy, model: &str, source: &str) -> bool {
    let model_ok =
        policy.allowed_models.is_empty() || policy.allowed_models.iter().any(|name| name == model);
    let source_ok = policy.allowed_source_prefixes.is_empty()
        || policy
            .allowed_source_prefixes
            .iter()
            .any(|prefix| source.starts_with(prefix));
    model_ok && source_ok
}

pub fn enforce(
    state: &AppState,
    tenant: Option<&str>,
    model: &str,
    source: &str,
) -> Result<(), String> {
    let Some(tenant) = tenant.filter(|t| !t.is_empty()) else {
        return Ok(());
    };
    let Some(policy) = state
        .store
        .get_entity::<TenantAiPolicy>(STORE_POLICIES, tenant)
        .ok()
        .flatten()
    else {
        return Ok(());
    };
    evaluate(
        &policy,
        &DeployCheck {
            model,
            source,
            site: "",
            gpus: 0,
            has_signature: true,
            context_tokens: 0,
            hour: chrono::Timelike::hour(&chrono::Utc::now()) as u8,
        },
    )
}

#[derive(Debug, serde::Deserialize)]
pub struct AdmitRequest {
    #[serde(default)]
    pub tenant: Option<String>,
    pub model: String,
    #[serde(default)]
    pub source: String,
    #[serde(default)]
    pub site: String,
    #[serde(default)]
    pub gpus: u32,
    #[serde(default)]
    pub has_signature: bool,
    #[serde(default)]
    pub context_tokens: u32,
}

/// POST /api/ai/admit
pub async fn admit(
    State(state): State<Arc<AppState>>,
    Json(body): Json<serde_json::Value>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    admit_body(&state, None, body).await
}

/// POST /api/ai/admit/{token}
pub async fn admit_with_token(
    State(state): State<Arc<AppState>>,
    axum::extract::Path(token): axum::extract::Path<String>,
    Json(body): Json<serde_json::Value>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    admit_body(&state, Some(token.as_str()), body).await
}

async fn admit_body(
    state: &AppState,
    token: Option<&str>,
    body: serde_json::Value,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    if let Err(message) = admit_token_ok(token) {
        return Err(super::err(StatusCode::UNAUTHORIZED, message));
    }
    let review = body.get("kind").and_then(|kind| kind.as_str()) == Some("AdmissionReview");
    let request = if review {
        admit_from_review(&body)
    } else {
        serde_json::from_value(body.clone())
            .map_err(|err| super::err(StatusCode::BAD_REQUEST, err.to_string()))?
    };
    let decision = admit_decision(state, &request);
    if review {
        let allowed = decision.is_ok();
        let message = decision.err().unwrap_or_default();
        return Ok(Json(serde_json::json!({
            "apiVersion": "admission.k8s.io/v1",
            "kind": "AdmissionReview",
            "response": {
                "uid": body.pointer("/request/uid").and_then(|v| v.as_str()).unwrap_or(""),
                "allowed": allowed,
                "status": { "message": message }
            }
        })));
    }
    decision.map_err(|message| super::err(StatusCode::FORBIDDEN, message))?;
    Ok(Json(serde_json::json!({"allowed": true})))
}

fn admit_token_ok(token: Option<&str>) -> Result<(), &'static str> {
    let expected = std::env::var("FLUXVM_AI_ADMIT_TOKEN").ok();
    match (expected.as_deref(), token) {
        (Some(expected), Some(token)) if expected == token && !expected.is_empty() => Ok(()),
        (Some(_), None) => Err("admit token is required"),
        (Some(_), Some(_)) => Err("admit token mismatch"),
        (None, None) => Ok(()),
        (None, Some(_)) => Err("admit token is not configured"),
    }
}

fn admit_from_review(body: &serde_json::Value) -> AdmitRequest {
    let spec = body.pointer("/request/object/spec");
    AdmitRequest {
        tenant: spec
            .and_then(|spec| spec.get("tenant"))
            .and_then(|v| v.as_str())
            .map(str::to_string),
        model: spec
            .and_then(|spec| spec.get("model"))
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string(),
        source: spec
            .and_then(|spec| spec.get("source"))
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string(),
        site: spec
            .and_then(|spec| spec.get("site"))
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string(),
        gpus: spec
            .and_then(|spec| spec.get("gpus"))
            .and_then(|v| v.as_u64())
            .unwrap_or(1) as u32,
        has_signature: spec
            .and_then(|spec| spec.get("has_signature"))
            .and_then(|v| v.as_bool())
            .unwrap_or(false),
        context_tokens: spec
            .and_then(|spec| spec.get("context_tokens"))
            .and_then(|v| v.as_u64())
            .unwrap_or(0) as u32,
    }
}

pub fn admit_decision(state: &AppState, request: &AdmitRequest) -> Result<(), String> {
    let Some(tenant) = request
        .tenant
        .as_deref()
        .filter(|tenant| !tenant.is_empty())
    else {
        return Ok(());
    };
    let Some(policy) = state
        .store
        .get_entity::<TenantAiPolicy>(STORE_POLICIES, tenant)
        .ok()
        .flatten()
    else {
        return Ok(());
    };
    evaluate(
        &policy,
        &DeployCheck {
            model: &request.model,
            source: &request.source,
            site: &request.site,
            gpus: request.gpus,
            has_signature: request.has_signature,
            context_tokens: request.context_tokens,
            hour: Timelike::hour(&chrono::Utc::now()) as u8,
        },
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn policy_blocks_an_unlisted_model() {
        let policy = TenantAiPolicy {
            id: "acme".into(),
            allowed_models: vec!["qwen".into()],
            allowed_source_prefixes: vec!["hf://Qwen/".into()],
            allowed_sites: vec!["pune-1".into()],
            max_gpus: 2,
            require_signature: true,
            max_context_tokens: 8192,
            prompt_logging: false,
            deploy_hour_start: Some(9),
            deploy_hour_end: Some(17),
        };
        assert!(allows(&policy, "qwen", "hf://Qwen/Qwen3-8B"));
        assert!(!allows(&policy, "other", "hf://Qwen/Qwen3-8B"));
        assert!(!allows(&policy, "qwen", "hf://secret/weights"));
        assert!(evaluate(
            &policy,
            &DeployCheck {
                model: "qwen",
                source: "hf://Qwen/Qwen3-8B",
                site: "pune-1",
                gpus: 2,
                has_signature: true,
                context_tokens: 1024,
                hour: 10,
            }
        )
        .is_ok());
        assert!(evaluate(
            &policy,
            &DeployCheck {
                model: "qwen",
                source: "hf://Qwen/Qwen3-8B",
                site: "pune-1",
                gpus: 4,
                has_signature: false,
                context_tokens: 1024,
                hour: 10,
            }
        )
        .is_err());
        assert!(rbac_allows(&["scale"], "scale"));
        assert!(!rbac_allows(&["read"], "rollback"));
        assert!(rbac_allows(&["administer"], "promote"));
        assert!(deploy_window(None, None, 3).is_ok());
        assert!(deploy_window(Some(9), Some(17), 9).is_ok());
        assert!(deploy_window(Some(9), Some(17), 17).is_err());
        assert!(deploy_window(Some(22), Some(6), 23).is_ok());
        assert!(deploy_window(Some(22), Some(6), 12).is_err());
        assert!(deploy_window(Some(4), Some(4), 4).is_ok());
    }
}

pub async fn put_policy(
    security::RequireWrite(claims): security::RequireWrite,
    axum::extract::State(state): axum::extract::State<std::sync::Arc<crate::server::AppState>>,
    axum::Json(policy): axum::Json<TenantAiPolicy>,
) -> Result<axum::Json<TenantAiPolicy>, (axum::http::StatusCode, axum::Json<serde_json::Value>)> {
    crate::validation::validate_entity_name(&policy.id).map_err(|(s, m)| super::err(s, m))?;
    state
        .store
        .save_entity(STORE_POLICIES, &policy.id, &policy)
        .map_err(|e| super::err(axum::http::StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    super::audit(
        &state,
        &claims.sub,
        "UPSERT",
        &format!("ai/policies/{}", policy.id),
        "SUCCESS",
    );
    Ok(axum::Json(policy))
}
