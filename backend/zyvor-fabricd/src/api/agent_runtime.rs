// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: Apache-2.0

//! Thin reverse-proxy from Fabric JWT-auth'd `/api/agents|sessions/*`
//! onto the sibling `zyvor-fabric-agent-runtime` service (`:9096`).
//!
//! Multi-tenant / multi-user rules (Keep-style personal agents):
//! - Deploy / session write: `RequireWrite` (Admin or User), not Admin-only.
//! - Non-admin with a JWT `tenant` claim: agent names are namespaced
//!   `t.{tenant}.{name}` and list/get are filtered to that prefix.
//! - Session create always stamps `user_id` from JWT `sub` for non-admins
//!   (admins may override). List/get/mutate sessions are scoped to that user
//!   for non-admins.

use axum::{
    body::Body,
    extract::{
        ws::{Message as AxumMessage, WebSocket, WebSocketUpgrade},
        Path, Query, State,
    },
    http::{header, Method, StatusCode},
    response::{IntoResponse, Response},
    Json,
};
use futures::{SinkExt, StreamExt};
use security::{Claims, RequireAdmin, RequireRead, RequireWrite, Role};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::sync::Arc;
use tokio_tungstenite::tungstenite::{
    client::IntoClientRequest, http::header::AUTHORIZATION as WS_AUTHORIZATION,
    Message as TungsteniteMessage,
};

use crate::server::AppState;

fn not_configured() -> Response {
    (
        StatusCode::SERVICE_UNAVAILABLE,
        Json(json!({
            "error": "agent_runtime.base_url is not configured — start zyvor-fabric-agent-runtime and set [agent_runtime] in zyvor-fabricd.toml"
        })),
    )
        .into_response()
}

#[allow(clippy::result_large_err)] // axum Response is intentionally large as Err
fn upstream(state: &AppState) -> Result<(String, Option<String>), Response> {
    let base = agent_runtime_base_url(&state.config.agent_runtime).ok_or_else(not_configured)?;
    let token = std::env::var("ZYVOR_FABRICD_AGENT_RUNTIME_TOKEN")
        .ok()
        .filter(|s| !s.is_empty())
        .or_else(|| state.config.agent_runtime.token.clone());
    Ok((base, token))
}

/// Resolve configured agent-runtime base URL (trimmed, no trailing slash).
pub(crate) fn agent_runtime_base_url(cfg: &crate::config::AgentRuntimeConfig) -> Option<String> {
    cfg.base_url
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(|s| s.trim_end_matches('/').to_string())
}

async fn proxy(
    state: &AppState,
    method: Method,
    path: &str,
    query: Option<&HashMap<String, String>>,
    body: Option<serde_json::Value>,
    accept: Option<&str>,
) -> Response {
    let (base, token) = match upstream(state) {
        Ok(v) => v,
        Err(resp) => return resp,
    };
    // CodeQL SSRF barrier: only allowlisted path shapes reach the outbound client.
    if !path.starts_with('/') || path.contains("..") || !input_guard::COMMAND_ARGS.contains(path) {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": "invalid agent-runtime path" })),
        )
            .into_response();
    }
    let mut url = format!("{base}{path}");
    if let Some(q) = query {
        if !q.is_empty() {
            let qs: Vec<String> = q
                .iter()
                .map(|(k, v)| format!("{}={}", urlencoding_encode(k), urlencoding_encode(v)))
                .collect();
            url.push('?');
            url.push_str(&qs.join("&"));
        }
    }

    let mut req = state.http_client.request(method, &url);
    if let Some(tok) = token {
        req = req.header(header::AUTHORIZATION, format!("Bearer {tok}"));
    }
    if let Some(a) = accept {
        req = req.header(header::ACCEPT, a);
    }
    if let Some(b) = body {
        req = req.json(&b);
    }

    match req.send().await {
        Ok(resp) => {
            let status =
                StatusCode::from_u16(resp.status().as_u16()).unwrap_or(StatusCode::BAD_GATEWAY);
            let mut headers = axum::http::HeaderMap::new();
            if let Some(ct) = resp.headers().get(header::CONTENT_TYPE) {
                headers.insert(header::CONTENT_TYPE, ct.clone());
            }
            match resp.bytes().await {
                Ok(bytes) => {
                    let mut out = Response::new(Body::from(bytes));
                    *out.status_mut() = status;
                    *out.headers_mut() = headers;
                    out
                }
                Err(e) => (
                    StatusCode::BAD_GATEWAY,
                    Json(json!({ "error": format!("agent-runtime body: {e}") })),
                )
                    .into_response(),
            }
        }
        Err(e) => (
            StatusCode::BAD_GATEWAY,
            Json(json!({ "error": format!("agent-runtime unreachable: {e}") })),
        )
            .into_response(),
    }
}

fn urlencoding_encode(s: &str) -> String {
    s.chars()
        .map(|c| match c {
            'A'..='Z' | 'a'..='z' | '0'..='9' | '-' | '_' | '.' | '~' => c.to_string(),
            _ => format!("%{:02X}", c as u8),
        })
        .collect()
}

/// Sanitize JWT subject / tenant into agent-runtime name components.
pub(crate) fn sanitize_name_component(raw: &str, max: usize) -> String {
    let mut out = String::new();
    for b in raw.bytes() {
        let c = if b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.') {
            b as char
        } else {
            '-'
        };
        if out.is_empty() && !(c.is_ascii_alphanumeric()) {
            continue;
        }
        out.push(c.to_ascii_lowercase());
        if out.len() >= max {
            break;
        }
    }
    if out.is_empty() {
        "user".into()
    } else {
        out
    }
}

pub(crate) fn tenant_agent_prefix(tenant: &str) -> String {
    format!("t.{}", sanitize_name_component(tenant, 40))
}

pub(crate) fn scope_agent_name(claims: &Claims, name: &str) -> String {
    if claims.role == Role::Admin {
        return name.to_string();
    }
    match claims.tenant.as_deref() {
        Some(t) if !t.is_empty() => {
            let prefix = tenant_agent_prefix(t);
            let bare = name.strip_prefix(&format!("{prefix}.")).unwrap_or(name);
            let bare = sanitize_name_component(bare, 40);
            format!("{prefix}.{bare}")
        }
        _ => name.to_string(),
    }
}

pub(crate) fn agent_visible(claims: &Claims, name: &str) -> bool {
    if claims.role == Role::Admin {
        return true;
    }
    match claims.tenant.as_deref() {
        Some(t) if !t.is_empty() => {
            let prefix = format!("{}.", tenant_agent_prefix(t));
            name.starts_with(&prefix)
        }
        _ => true,
    }
}

pub(crate) fn session_user_id(claims: &Claims) -> String {
    // agent-runtime MAX_USER_ID_CHARS = 32
    sanitize_name_component(&claims.sub, 32)
}

fn json_response(status: StatusCode, value: Value) -> Response {
    (status, Json(value)).into_response()
}

// The error is a ready-to-send axum `Response` (large by design), returned straight to the client.
#[allow(clippy::result_large_err)]
async fn proxy_json(
    state: &AppState,
    method: Method,
    path: &str,
    query: Option<&HashMap<String, String>>,
    body: Option<Value>,
) -> Result<(StatusCode, Value), Response> {
    let resp = proxy(state, method, path, query, body, None).await;
    let status = resp.status();
    let bytes = axum::body::to_bytes(resp.into_body(), 16 * 1024 * 1024)
        .await
        .map_err(|e| {
            (
                StatusCode::BAD_GATEWAY,
                Json(json!({ "error": format!("agent-runtime body: {e}") })),
            )
                .into_response()
        })?;
    let value = if bytes.is_empty() {
        Value::Null
    } else {
        serde_json::from_slice(&bytes)
            .unwrap_or_else(|_| json!({ "raw": String::from_utf8_lossy(&bytes) }))
    };
    Ok((status, value))
}

pub async fn list_agents(
    RequireRead(claims): RequireRead,
    State(state): State<Arc<AppState>>,
) -> Response {
    match proxy_json(&state, Method::GET, "/v1/agents", None, None).await {
        Ok((status, mut value)) => {
            if let Some(items) = value.get_mut("items").and_then(|v| v.as_array_mut()) {
                items.retain(|item| {
                    item.get("name")
                        .and_then(|n| n.as_str())
                        .is_some_and(|n| agent_visible(&claims, n))
                });
            }
            json_response(status, value)
        }
        Err(resp) => resp,
    }
}

pub async fn deploy_agent(
    RequireWrite(claims): RequireWrite,
    State(state): State<Arc<AppState>>,
    Json(mut body): Json<Value>,
) -> Response {
    if let Some(name) = body.get("name").and_then(|v| v.as_str()) {
        let scoped = scope_agent_name(&claims, name);
        body["name"] = Value::String(scoped);
    }
    // Non-admin deploys default to per-user home so sessions need user_id.
    if claims.role != Role::Admin {
        if let Some(manifest) = body.get_mut("manifest").and_then(|m| m.as_object_mut()) {
            if !manifest.contains_key("home_volume") {
                manifest.insert("home_volume".into(), json!({ "per_user": true }));
            }
        }
    }
    match proxy_json(&state, Method::POST, "/v1/agents", None, Some(body)).await {
        Ok((status, value)) => json_response(status, value),
        Err(resp) => resp,
    }
}

pub async fn get_agent(
    RequireRead(claims): RequireRead,
    State(state): State<Arc<AppState>>,
    Path(name): Path<String>,
) -> Response {
    let scoped = scope_agent_name(&claims, &name);
    if !agent_visible(&claims, &scoped) {
        return (
            StatusCode::NOT_FOUND,
            Json(json!({ "error": "agent not found" })),
        )
            .into_response();
    }
    proxy(
        &state,
        Method::GET,
        &format!("/v1/agents/{scoped}"),
        None,
        None,
        None,
    )
    .await
}

pub async fn list_sessions(
    RequireRead(claims): RequireRead,
    State(state): State<Arc<AppState>>,
) -> Response {
    match proxy_json(&state, Method::GET, "/v1/sessions", None, None).await {
        Ok((status, mut value)) => {
            if claims.role != Role::Admin {
                let uid = session_user_id(&claims);
                if let Some(items) = value.get_mut("items").and_then(|v| v.as_array_mut()) {
                    items.retain(|item| {
                        let agent_ok = item
                            .get("agent")
                            .and_then(|n| n.as_str())
                            .is_some_and(|n| agent_visible(&claims, n));
                        let user_ok = item
                            .get("user_id")
                            .and_then(|u| u.as_str())
                            .is_none_or(|u| u == uid);
                        agent_ok && user_ok
                    });
                }
            }
            json_response(status, value)
        }
        Err(resp) => resp,
    }
}

pub async fn create_session(
    RequireWrite(claims): RequireWrite,
    State(state): State<Arc<AppState>>,
    Json(mut body): Json<Value>,
) -> Response {
    if let Some(agent) = body.get("agent").and_then(|v| v.as_str()) {
        body["agent"] = Value::String(scope_agent_name(&claims, agent));
    }
    let uid = session_user_id(&claims);
    if claims.role != Role::Admin {
        body["user_id"] = Value::String(uid);
    } else if body.get("user_id").and_then(|v| v.as_str()).is_none() {
        // Admin runs without user_id unless the agent requires per-user home.
        // Leave absent; runtime will 400 if required.
    }
    match proxy_json(&state, Method::POST, "/v1/sessions", None, Some(body)).await {
        Ok((status, value)) => json_response(status, value),
        Err(resp) => resp,
    }
}

// The error is a ready-to-send axum `Response` (large by design), returned straight to the client.
#[allow(clippy::result_large_err)]
async fn session_owned(state: &AppState, claims: &Claims, id: &str) -> Result<Value, Response> {
    let (status, value) = proxy_json(
        state,
        Method::GET,
        &format!("/v1/sessions/{id}"),
        None,
        None,
    )
    .await?;
    if !status.is_success() {
        return Err(json_response(status, value));
    }
    if claims.role == Role::Admin {
        return Ok(value);
    }
    let uid = session_user_id(claims);
    let agent_ok = value
        .get("agent")
        .and_then(|n| n.as_str())
        .is_some_and(|n| agent_visible(claims, n));
    let user_ok = value
        .get("user_id")
        .and_then(|u| u.as_str())
        .is_none_or(|u| u == uid);
    if agent_ok && user_ok {
        Ok(value)
    } else {
        Err((
            StatusCode::NOT_FOUND,
            Json(json!({ "error": "session not found" })),
        )
            .into_response())
    }
}

pub async fn get_session(
    RequireRead(claims): RequireRead,
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Response {
    match session_owned(&state, &claims, &id).await {
        Ok(value) => json_response(StatusCode::OK, value),
        Err(resp) => resp,
    }
}

pub async fn delete_session(
    RequireWrite(claims): RequireWrite,
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Response {
    if let Err(resp) = session_owned(&state, &claims, &id).await {
        return resp;
    }
    proxy(
        &state,
        Method::DELETE,
        &format!("/v1/sessions/{id}"),
        None,
        None,
        None,
    )
    .await
}

pub async fn session_action(
    RequireWrite(claims): RequireWrite,
    State(state): State<Arc<AppState>>,
    Path((id, action)): Path<(String, String)>,
    body: Result<Json<Value>, axum::extract::rejection::JsonRejection>,
) -> Response {
    let allowed = matches!(action.as_str(), "steer" | "cancel" | "hibernate" | "resume");
    if !allowed {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": "unknown session action" })),
        )
            .into_response();
    }
    if let Err(resp) = session_owned(&state, &claims, &id).await {
        return resp;
    }
    let payload = body.ok().map(|Json(v)| v);
    proxy(
        &state,
        Method::POST,
        &format!("/v1/sessions/{id}/{action}"),
        None,
        payload,
        None,
    )
    .await
}

pub async fn session_events(
    RequireRead(claims): RequireRead,
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    Query(query): Query<HashMap<String, String>>,
) -> Response {
    if let Err(resp) = session_owned(&state, &claims, &id).await {
        return resp;
    }
    proxy(
        &state,
        Method::GET,
        &format!("/v1/sessions/{id}/events"),
        Some(&query),
        None,
        Some("text/event-stream"),
    )
    .await
}

/// Keep cockpit: goal, artifacts, pending approvals, last decisions.
pub async fn session_cockpit(
    RequireRead(claims): RequireRead,
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Response {
    if let Err(resp) = session_owned(&state, &claims, &id).await {
        return resp;
    }
    proxy(
        &state,
        Method::GET,
        &format!("/v1/sessions/{id}/cockpit"),
        None,
        None,
        None,
    )
    .await
}

/// The one-click Keep demos the console can offer (`pdf-brief`, `csv-clean`, ...).
pub async fn demo_list(
    RequireRead(_claims): RequireRead,
    State(state): State<Arc<AppState>>,
) -> Response {
    proxy(&state, Method::GET, "/v1/demos", None, None, None).await
}

/// Keep readiness for the console and `merionctl doctor`: Keep mode, signers,
/// FluxVM readiness and demo counts.
pub async fn merion_status(
    RequireRead(_claims): RequireRead,
    State(state): State<Arc<AppState>>,
) -> Response {
    proxy(&state, Method::GET, "/v1/keep/status", None, None, None).await
}

/// A signed pack made by `fabric-agent pack bundle` / `merionctl bundle`.
///
/// `deploy_json` is the exact string that was signed. It is forwarded as raw
/// bytes, never parsed and re-serialised, so the Ed25519 signature still
/// matches in Keep mode. The signer seed never leaves the author's machine:
/// the console only uploads the result.
#[derive(Debug)]
struct MerionPack {
    name: String,
    deploy_json: String,
    signature: Option<String>,
    policy: Option<String>,
    policy_signature: Option<String>,
}

fn parse_merion_pack(bytes: &[u8]) -> Result<MerionPack, String> {
    let v: Value = serde_json::from_slice(bytes).map_err(|e| format!("not valid JSON: {e}"))?;
    if v.get("format").and_then(Value::as_str) != Some("merionpack/1") {
        return Err("not a merionpack/1 file".into());
    }
    let text = |k: &str| v.get(k).and_then(Value::as_str).map(str::to_string);
    let name = text("name").ok_or("missing name")?;
    let deploy_json = text("deploy_json").ok_or("missing deploy_json")?;
    // The name is used in an upstream path and must match what is being signed.
    if !valid_demo_id(&name) {
        return Err("invalid pack name".into());
    }
    let deployed_name = serde_json::from_str::<Value>(&deploy_json)
        .map_err(|e| format!("deploy_json is not valid JSON: {e}"))?
        .get("name")
        .and_then(Value::as_str)
        .map(str::to_string);
    if deployed_name.as_deref() != Some(name.as_str()) {
        return Err("pack name does not match the name inside deploy_json".into());
    }
    Ok(MerionPack {
        name,
        deploy_json,
        signature: text("signature").filter(|s| !s.is_empty()),
        policy: text("policy").filter(|s| !s.trim().is_empty()),
        policy_signature: text("policy_signature").filter(|s| !s.is_empty()),
    })
}

/// Deploy a signed pack (agent + optional policy) from the console. Admin only,
/// like every other console agent deploy, and the name is not re-scoped because
/// that would change the signed bytes.
pub async fn pack_deploy(
    RequireAdmin(_claims): RequireAdmin,
    State(state): State<Arc<AppState>>,
    request: axum::http::Request<Body>,
) -> Response {
    let bytes = match axum::body::to_bytes(request.into_body(), 32 * 1024 * 1024).await {
        Ok(b) => b,
        Err(e) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(json!({ "error": format!("read body: {e}") })),
            )
                .into_response();
        }
    };
    let pack = match parse_merion_pack(&bytes) {
        Ok(p) => p,
        Err(e) => {
            return (StatusCode::BAD_REQUEST, Json(json!({ "error": e }))).into_response();
        }
    };
    let (base, token) = match upstream(&state) {
        Ok(v) => v,
        Err(resp) => return resp,
    };

    let mut req = state
        .http_client
        .post(format!("{base}/v1/agents"))
        .header(header::CONTENT_TYPE, "application/json")
        .body(pack.deploy_json.clone().into_bytes());
    if let Some(tok) = &token {
        req = req.header(header::AUTHORIZATION, format!("Bearer {tok}"));
    }
    if let Some(sig) = &pack.signature {
        req = req.header("x-merion-manifest-signature", sig);
    }
    let resp = match req.send().await {
        Ok(r) => r,
        Err(e) => {
            return (
                StatusCode::BAD_GATEWAY,
                Json(json!({ "error": format!("agent-runtime unreachable: {e}") })),
            )
                .into_response();
        }
    };
    let status = StatusCode::from_u16(resp.status().as_u16()).unwrap_or(StatusCode::BAD_GATEWAY);
    let deployed: Value = resp.json().await.unwrap_or(Value::Null);
    if !status.is_success() {
        return json_response(
            status,
            json!({ "error": deployed.get("error").cloned().unwrap_or(deployed), "step": "deploy" }),
        );
    }

    let mut policy_state = Value::Null;
    if let Some(yaml) = &pack.policy {
        let mut req = state
            .http_client
            .put(format!("{base}/v1/agents/{}/policy", pack.name))
            .header(header::CONTENT_TYPE, "application/yaml")
            .body(yaml.clone().into_bytes());
        if let Some(tok) = &token {
            req = req.header(header::AUTHORIZATION, format!("Bearer {tok}"));
        }
        if let Some(sig) = &pack.policy_signature {
            req = req.header("x-merion-policy-signature", sig);
        }
        match req.send().await {
            Ok(r) if r.status().is_success() => policy_state = json!("applied"),
            Ok(r) => {
                let code =
                    StatusCode::from_u16(r.status().as_u16()).unwrap_or(StatusCode::BAD_GATEWAY);
                let body: Value = r.json().await.unwrap_or(Value::Null);
                return json_response(
                    code,
                    json!({
                        "error": body.get("error").cloned().unwrap_or(body),
                        "step": "policy",
                        "deployed": deployed,
                    }),
                );
            }
            Err(e) => {
                return (
                    StatusCode::BAD_GATEWAY,
                    Json(json!({ "error": format!("agent-runtime unreachable: {e}"), "step": "policy", "deployed": deployed })),
                )
                    .into_response();
            }
        }
    }
    json_response(
        StatusCode::CREATED,
        json!({ "name": pack.name, "deployed": deployed, "policy": policy_state, "signed": pack.signature.is_some() }),
    )
}

/// A demo id becomes part of the upstream path, so accept slug characters only.
fn valid_demo_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 48
        && id
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
}

fn bad_demo_id() -> Response {
    (
        StatusCode::BAD_REQUEST,
        Json(json!({ "error": "invalid demo id" })),
    )
        .into_response()
}

/// Create or replace a user-defined use case (a declarative spec: an extractor
/// enum plus bounded rules, so it carries no code).
pub async fn demo_save(
    RequireWrite(_claims): RequireWrite,
    State(state): State<Arc<AppState>>,
    Json(body): Json<Value>,
) -> Response {
    match proxy_json(&state, Method::POST, "/v1/demos", None, Some(body)).await {
        Ok((status, value)) => json_response(status, value),
        Err(resp) => resp,
    }
}

/// Remove a user-defined use case. Built-ins are refused by agent-runtime.
pub async fn demo_delete(
    RequireWrite(_claims): RequireWrite,
    State(state): State<Arc<AppState>>,
    Path(demo_id): Path<String>,
) -> Response {
    if !valid_demo_id(&demo_id) {
        return bad_demo_id();
    }
    proxy(
        &state,
        Method::DELETE,
        &format!("/v1/demos/{demo_id}"),
        None,
        None,
        None,
    )
    .await
}

/// Run one Keep demo: multipart file passthrough to agent-runtime `/v1/demos/{id}`.
/// `pdf-brief` is one id among several, so the original route keeps working.
pub async fn demo_run(
    RequireWrite(_claims): RequireWrite,
    State(state): State<Arc<AppState>>,
    Path(demo_id): Path<String>,
    request: axum::http::Request<Body>,
) -> Response {
    if !valid_demo_id(&demo_id) {
        return bad_demo_id();
    }
    let (base, token) = match upstream(&state) {
        Ok(v) => v,
        Err(resp) => return resp,
    };
    let path_owned = format!("/v1/demos/{demo_id}");
    let path: &str = path_owned.as_str();
    // CodeQL SSRF barrier: the same check, on the same `&str` shape, as `proxy`. Passing `&path` (a reference to a
    // `String`) hides the guarded value from the barrier model, so the finding stayed open.
    if !path.starts_with('/') || path.contains("..") || !input_guard::COMMAND_ARGS.contains(path) {
        return bad_demo_id();
    }
    let url = format!("{base}{path}");
    let ct = request.headers().get(header::CONTENT_TYPE).cloned();
    let body = match axum::body::to_bytes(request.into_body(), 32 * 1024 * 1024).await {
        Ok(b) => b,
        Err(e) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(json!({ "error": format!("read body: {e}") })),
            )
                .into_response();
        }
    };
    let mut req = state.http_client.post(&url).body(body);
    if let Some(tok) = token {
        req = req.header(header::AUTHORIZATION, format!("Bearer {tok}"));
    }
    if let Some(c) = ct {
        req = req.header(header::CONTENT_TYPE, c);
    }
    match req.send().await {
        Ok(resp) => {
            let status =
                StatusCode::from_u16(resp.status().as_u16()).unwrap_or(StatusCode::BAD_GATEWAY);
            let mut headers = axum::http::HeaderMap::new();
            if let Some(ct) = resp.headers().get(header::CONTENT_TYPE) {
                headers.insert(header::CONTENT_TYPE, ct.clone());
            }
            match resp.bytes().await {
                Ok(bytes) => {
                    let mut out = Response::new(Body::from(bytes));
                    *out.status_mut() = status;
                    *out.headers_mut() = headers;
                    out
                }
                Err(e) => (
                    StatusCode::BAD_GATEWAY,
                    Json(json!({ "error": format!("agent-runtime body: {e}") })),
                )
                    .into_response(),
            }
        }
        Err(e) => (
            StatusCode::BAD_GATEWAY,
            Json(json!({ "error": format!("agent-runtime unreachable: {e}") })),
        )
            .into_response(),
    }
}

/// Keep browser live listing (tabs only; no CDP WebSocket to the operator).
pub async fn session_browser_view(
    RequireRead(claims): RequireRead,
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Response {
    if let Err(resp) = session_owned(&state, &claims, &id).await {
        return resp;
    }
    proxy(
        &state,
        Method::GET,
        &format!("/v1/sessions/{id}/browser/view"),
        None,
        None,
        None,
    )
    .await
}

/// Keep JPEG screenshot via host CDP bridge (no operator CDP / no input).
pub async fn session_browser_screenshot(
    RequireRead(claims): RequireRead,
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Response {
    if let Err(resp) = session_owned(&state, &claims, &id).await {
        return resp;
    }
    proxy(
        &state,
        Method::GET,
        &format!("/v1/sessions/{id}/browser/screenshot"),
        None,
        None,
        None,
    )
    .await
}

/// Keep read-only screencast WebSocket → agent-runtime (frames only; no input).
/// Mounted under `/ws/sessions/{id}/browser/screencast` (JWT via `?token=`).
pub async fn session_browser_screencast(
    ws: WebSocketUpgrade,
    RequireRead(claims): RequireRead,
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Response {
    if let Err(resp) = session_owned(&state, &claims, &id).await {
        return resp;
    }
    let (base, token) = match upstream(&state) {
        Ok(v) => v,
        Err(resp) => return resp,
    };
    let ws_base = match http_base_to_ws(&base) {
        Ok(u) => u,
        Err(msg) => {
            return (StatusCode::BAD_GATEWAY, Json(json!({ "error": msg }))).into_response();
        }
    };
    let url = format!("{ws_base}/v1/sessions/{id}/browser/screencast");
    let mut request = match url.as_str().into_client_request() {
        Ok(r) => r,
        Err(e) => {
            return (
                StatusCode::BAD_GATEWAY,
                Json(json!({ "error": format!("screencast request: {e}") })),
            )
                .into_response();
        }
    };
    if let Some(tok) = token.as_deref() {
        match format!("Bearer {tok}").parse() {
            Ok(v) => {
                request.headers_mut().insert(WS_AUTHORIZATION, v);
            }
            Err(e) => {
                return (
                    StatusCode::BAD_GATEWAY,
                    Json(json!({ "error": format!("screencast auth header: {e}") })),
                )
                    .into_response();
            }
        }
    }
    let upstream = match tokio_tungstenite::connect_async(request).await {
        Ok((stream, _)) => stream,
        Err(e) => {
            return (
                StatusCode::BAD_GATEWAY,
                Json(json!({ "error": format!("agent-runtime screencast: {e}") })),
            )
                .into_response();
        }
    };
    ws.on_upgrade(move |socket| bridge_screencast(socket, upstream))
        .into_response()
}

fn http_base_to_ws(base: &str) -> Result<String, String> {
    if let Some(rest) = base.strip_prefix("https://") {
        Ok(format!("wss://{rest}"))
    } else if let Some(rest) = base.strip_prefix("http://") {
        Ok(format!("ws://{rest}"))
    } else {
        Err(format!("unsupported agent-runtime URL scheme: {base}"))
    }
}

async fn bridge_screencast(
    mut down: WebSocket,
    upstream: tokio_tungstenite::WebSocketStream<
        tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>,
    >,
) {
    let (mut up_sink, mut up_stream) = upstream.split();
    loop {
        tokio::select! {
            incoming = down.recv() => {
                let Some(Ok(message)) = incoming else { break };
                let outgoing = match message {
                    AxumMessage::Text(text) => TungsteniteMessage::text(text.to_string()),
                    AxumMessage::Binary(bytes) => TungsteniteMessage::Binary(bytes),
                    AxumMessage::Ping(bytes) => TungsteniteMessage::Ping(bytes),
                    AxumMessage::Pong(bytes) => TungsteniteMessage::Pong(bytes),
                    AxumMessage::Close(_) => {
                        let _ = up_sink.send(TungsteniteMessage::Close(None)).await;
                        break;
                    }
                };
                if up_sink.send(outgoing).await.is_err() {
                    break;
                }
            }
            outgoing = up_stream.next() => {
                let Some(Ok(message)) = outgoing else { break };
                let incoming = match message {
                    TungsteniteMessage::Text(text) => AxumMessage::Text(text.to_string().into()),
                    TungsteniteMessage::Binary(bytes) => AxumMessage::Binary(bytes),
                    TungsteniteMessage::Ping(bytes) => AxumMessage::Ping(bytes),
                    TungsteniteMessage::Pong(bytes) => AxumMessage::Pong(bytes),
                    TungsteniteMessage::Close(_) => {
                        let _ = down.send(AxumMessage::Close(None)).await;
                        break;
                    }
                    TungsteniteMessage::Frame(_) => continue,
                };
                if down.send(incoming).await.is_err() {
                    break;
                }
            }
        }
    }
}

/// Keep dual-key host recover (forbidden on confidential; measured needs both keys).
pub async fn session_host_recover(
    RequireWrite(claims): RequireWrite,
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    Json(body): Json<Value>,
) -> Response {
    if let Err(resp) = session_owned(&state, &claims, &id).await {
        return resp;
    }
    proxy(
        &state,
        Method::POST,
        &format!("/v1/sessions/{id}/host-recover"),
        None,
        Some(body),
        None,
    )
    .await
}

/// Run a command in the session's VM without keeping its file changes.
///
/// Keep returns a `changeset` approval; a person settles it through
/// `POST /approvals/{id}`. The agent cannot approve its own changeset.
pub async fn session_speculate(
    RequireWrite(claims): RequireWrite,
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    Json(body): Json<Value>,
) -> Response {
    if let Err(resp) = session_owned(&state, &claims, &id).await {
        return resp;
    }
    proxy(
        &state,
        Method::POST,
        &format!("/v1/sessions/{id}/speculate"),
        None,
        Some(body),
        None,
    )
    .await
}

/// Keep vault status (credential names + user-held flags; never secret values).
pub async fn vault_status(
    RequireRead(_claims): RequireRead,
    State(state): State<Arc<AppState>>,
) -> Response {
    proxy(&state, Method::GET, "/v1/vault/status", None, None, None).await
}

/// Mint user-held unwrap challenge (complete still fail-closed without SNP/TDX).
pub async fn vault_user_held_challenge(
    RequireWrite(_claims): RequireWrite,
    State(state): State<Arc<AppState>>,
    Json(body): Json<Value>,
) -> Response {
    proxy(
        &state,
        Method::POST,
        "/v1/vault/user-held/challenge",
        None,
        Some(body),
        None,
    )
    .await
}

/// Complete user-held unwrap (proxied; agent-runtime enforces launch-verified gate).
pub async fn vault_user_held_complete(
    RequireWrite(_claims): RequireWrite,
    State(state): State<Arc<AppState>>,
    Json(body): Json<Value>,
) -> Response {
    proxy(
        &state,
        Method::POST,
        "/v1/vault/user-held/complete",
        None,
        Some(body),
        None,
    )
    .await
}

pub async fn list_approvals(
    RequireRead(claims): RequireRead,
    State(state): State<Arc<AppState>>,
) -> Response {
    if claims.role == Role::Admin {
        return proxy(&state, Method::GET, "/v1/approvals", None, None, None).await;
    }
    // Everyone else sees only approvals whose session they own.
    let (status, mut value) =
        match proxy_json(&state, Method::GET, "/v1/approvals", None, None).await {
            Ok(v) => v,
            Err(resp) => return resp,
        };
    if !status.is_success() {
        return json_response(status, value);
    }
    let mut owned: HashMap<String, bool> = HashMap::new();
    let mut keep: Vec<Value> = Vec::new();
    for item in value["items"].as_array().cloned().unwrap_or_default() {
        let Some(sid) = item["session_id"].as_str().map(str::to_string) else {
            continue;
        };
        // The id goes into an upstream path, so it must be a UUID.
        if uuid::Uuid::parse_str(&sid).is_err() {
            continue;
        }
        let ok = match owned.get(&sid) {
            Some(v) => *v,
            None => {
                let v = session_owned(&state, &claims, &sid).await.is_ok();
                owned.insert(sid.clone(), v);
                v
            }
        };
        if ok {
            keep.push(item);
        }
    }
    value["items"] = Value::Array(keep);
    json_response(StatusCode::OK, value)
}

pub async fn create_approval(
    RequireWrite(_claims): RequireWrite,
    State(state): State<Arc<AppState>>,
    Json(body): Json<Value>,
) -> Response {
    proxy(
        &state,
        Method::POST,
        "/v1/approvals",
        None,
        Some(body),
        None,
    )
    .await
}

/// Approve or deny a pending approval. The id is validated as a UUID because
/// it is interpolated into the upstream path.
pub async fn decide_approval(
    RequireWrite(claims): RequireWrite,
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    Json(body): Json<Value>,
) -> Response {
    if uuid::Uuid::parse_str(&id).is_err() {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": "approval id must be a UUID" })),
        )
            .into_response();
    }
    // A non-admin may decide only an approval that belongs to one of their own sessions.
    if claims.role != Role::Admin {
        let (status, value) =
            match proxy_json(&state, Method::GET, "/v1/approvals", None, None).await {
                Ok(v) => v,
                Err(resp) => return resp,
            };
        if !status.is_success() {
            return json_response(status, value);
        }
        let session = value["items"]
            .as_array()
            .and_then(|items| items.iter().find(|a| a["id"].as_str() == Some(id.as_str())))
            .and_then(|a| a["session_id"].as_str())
            .filter(|sid| uuid::Uuid::parse_str(sid).is_ok())
            .map(str::to_string);
        let owned = match session {
            Some(sid) => session_owned(&state, &claims, &sid).await.is_ok(),
            None => false,
        };
        if !owned {
            return json_response(
                StatusCode::NOT_FOUND,
                json!({ "error": "approval not found" }),
            );
        }
    }
    proxy(
        &state,
        Method::POST,
        &format!("/v1/approvals/{id}"),
        None,
        Some(body),
        None,
    )
    .await
}

/// Hash-chained journal of planned, approved, denied and performed agent actions.
pub async fn list_audit(
    RequireRead(claims): RequireRead,
    State(state): State<Arc<AppState>>,
    Query(query): Query<HashMap<String, String>>,
) -> Response {
    // The journal is one chain for every user. A non-admin reads it one owned session at a time.
    if claims.role != Role::Admin {
        let Some(sid) = query.get("session_id") else {
            return json_response(
                StatusCode::BAD_REQUEST,
                json!({ "error": "session_id is required" }),
            );
        };
        if uuid::Uuid::parse_str(sid).is_err() {
            return json_response(
                StatusCode::BAD_REQUEST,
                json!({ "error": "session_id must be a UUID" }),
            );
        }
        if let Err(resp) = session_owned(&state, &claims, sid).await {
            return resp;
        }
    }
    proxy(&state, Method::GET, "/v1/audit", Some(&query), None, None).await
}

/// A runtime user token for the logged-in user, for a phone or another client that talks to the
/// runtime directly. The user id is always the caller's own; scopes and lifetime are optional.
pub async fn mint_my_token(
    RequireWrite(claims): RequireWrite,
    State(state): State<Arc<AppState>>,
    Json(body): Json<Value>,
) -> Response {
    let mut req = json!({ "user_id": session_user_id(&claims) });
    for key in ["scopes", "ttl_seconds"] {
        if let Some(v) = body.get(key) {
            req[key] = v.clone();
        }
    }
    proxy(
        &state,
        Method::POST,
        "/v1/user-tokens",
        None,
        Some(req),
        None,
    )
    .await
}

/// Run history: artifacts across sessions. Admin only, because bodies are
/// derived from users' documents (a user sees their own via the session cockpit).
pub async fn list_artifacts(
    RequireAdmin(_): RequireAdmin,
    State(state): State<Arc<AppState>>,
    Query(query): Query<HashMap<String, String>>,
) -> Response {
    proxy(
        &state,
        Method::GET,
        "/v1/artifacts",
        Some(&query),
        None,
        None,
    )
    .await
}

/// Line diff of two artifacts. Both ids are validated as UUIDs because they are
/// interpolated into the upstream path.
pub async fn diff_artifacts(
    RequireAdmin(_): RequireAdmin,
    State(state): State<Arc<AppState>>,
    Path((a, b)): Path<(String, String)>,
) -> Response {
    if uuid::Uuid::parse_str(&a).is_err() || uuid::Uuid::parse_str(&b).is_err() {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": "artifact ids must be UUIDs" })),
        )
            .into_response();
    }
    proxy(
        &state,
        Method::GET,
        &format!("/v1/artifacts/{a}/diff/{b}"),
        None,
        None,
        None,
    )
    .await
}

/// Triggers that start a use case without an upload (signed webhooks, watched folders). Admin only.
pub async fn list_triggers(
    RequireAdmin(_): RequireAdmin,
    State(state): State<Arc<AppState>>,
) -> Response {
    proxy(&state, Method::GET, "/v1/triggers", None, None, None).await
}

pub async fn create_trigger(
    RequireAdmin(_): RequireAdmin,
    State(state): State<Arc<AppState>>,
    Json(body): Json<Value>,
) -> Response {
    proxy(&state, Method::POST, "/v1/triggers", None, Some(body), None).await
}

pub async fn delete_trigger(
    RequireAdmin(_): RequireAdmin,
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Response {
    if uuid::Uuid::parse_str(&id).is_err() {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": "trigger id must be a UUID" })),
        )
            .into_response();
    }
    proxy(
        &state,
        Method::DELETE,
        &format!("/v1/triggers/{id}"),
        None,
        None,
        None,
    )
    .await
}

/// Skill names go into the upstream path, so they are restricted to the
/// characters the runtime itself accepts.
fn valid_skill_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 80
        && !name.starts_with('.')
        && name
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.'))
}

fn bad_skill_name() -> Response {
    (
        StatusCode::BAD_REQUEST,
        Json(json!({ "error": "invalid skill name" })),
    )
        .into_response()
}

pub async fn list_skills(
    RequireRead(_): RequireRead,
    State(state): State<Arc<AppState>>,
) -> Response {
    proxy(&state, Method::GET, "/v1/skills", None, None, None).await
}

pub async fn publish_skill(
    RequireAdmin(_): RequireAdmin,
    State(state): State<Arc<AppState>>,
    Json(body): Json<Value>,
) -> Response {
    proxy(&state, Method::POST, "/v1/skills", None, Some(body), None).await
}

pub async fn get_skill(
    RequireRead(_): RequireRead,
    State(state): State<Arc<AppState>>,
    Path(name): Path<String>,
) -> Response {
    if !valid_skill_name(&name) {
        return bad_skill_name();
    }
    proxy(
        &state,
        Method::GET,
        &format!("/v1/skills/{name}"),
        None,
        None,
        None,
    )
    .await
}

pub async fn delete_skill(
    RequireAdmin(_): RequireAdmin,
    State(state): State<Arc<AppState>>,
    Path(name): Path<String>,
) -> Response {
    if !valid_skill_name(&name) {
        return bad_skill_name();
    }
    proxy(
        &state,
        Method::DELETE,
        &format!("/v1/skills/{name}"),
        None,
        None,
        None,
    )
    .await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::AgentRuntimeConfig;

    fn claims(role: Role, sub: &str, tenant: Option<&str>) -> Claims {
        Claims {
            sub: sub.into(),
            role,
            exp: usize::MAX,
            jti: "t".into(),
            tenant: tenant.map(str::to_string),
        }
    }

    #[test]
    fn skill_names_cannot_reach_other_upstream_paths() {
        assert!(valid_skill_name("notes-1.2_x"));
        for bad in ["", ".hidden", "a/b", "..", "a b", "a?x=1", "a#b", "%2e%2e"] {
            assert!(!valid_skill_name(bad), "{bad:?}");
        }
    }

    #[test]
    fn agent_runtime_base_url_none_when_unset() {
        assert!(agent_runtime_base_url(&AgentRuntimeConfig::default()).is_none());
    }

    #[test]
    fn agent_runtime_base_url_trims_slash_and_whitespace() {
        let cfg = AgentRuntimeConfig {
            base_url: Some("  http://127.0.0.1:9096/  ".into()),
            token: None,
        };
        assert_eq!(
            agent_runtime_base_url(&cfg).as_deref(),
            Some("http://127.0.0.1:9096")
        );
    }

    #[test]
    fn http_base_to_ws_rewrites_scheme() {
        assert_eq!(
            http_base_to_ws("http://127.0.0.1:9096").unwrap(),
            "ws://127.0.0.1:9096"
        );
        assert_eq!(
            http_base_to_ws("https://agents.example").unwrap(),
            "wss://agents.example"
        );
        assert!(http_base_to_ws("ftp://nope").is_err());
    }

    #[test]
    fn urlencoding_encode_leaves_safe_chars() {
        assert_eq!(urlencoding_encode("after"), "after");
        assert_eq!(urlencoding_encode("a b"), "a%20b");
    }

    #[test]
    fn tenant_user_agent_names_are_prefixed() {
        let c = claims(Role::User, "Alice/One", Some("Acme Corp"));
        assert_eq!(scope_agent_name(&c, "research"), "t.acme-corp.research");
        assert!(agent_visible(&c, "t.acme-corp.research"));
        assert!(!agent_visible(&c, "t.other.research"));
        assert!(!agent_visible(&c, "research"));
    }

    #[test]
    fn admin_sees_all_agents_unprefixed() {
        let c = claims(Role::Admin, "admin", Some("acme"));
        assert_eq!(scope_agent_name(&c, "research"), "research");
        assert!(agent_visible(&c, "t.other.x"));
    }

    #[test]
    fn session_user_id_sanitizes_subject() {
        let c = claims(Role::User, "Alice/One", None);
        assert_eq!(session_user_id(&c), "alice-one");
        let uuid = claims(
            Role::User,
            "8bb0203c-1798-4884-8602-b80ca2c02fe9",
            Some("acme"),
        );
        let uid = session_user_id(&uuid);
        assert!(uid.len() <= 32);
        assert!(uid.chars().next().unwrap().is_ascii_alphanumeric());
        assert_eq!(&uid, "8bb0203c-1798-4884-8602-b80ca2c0");
    }
}

#[cfg(test)]
mod keep_pack_tests {
    use super::*;

    fn pack(extra: &str) -> Vec<u8> {
        format!(
            r#"{{"format":"merionpack/1","name":"my-agent","deploy_json":"{{\"name\":\"my-agent\",\"bundle_base64\":\"AA==\"}}"{extra}}}"#
        )
        .into_bytes()
    }

    #[test]
    fn a_pack_keeps_the_signed_bytes_and_optional_parts() {
        let p = parse_merion_pack(&pack(
            r#","signature":"ab","policy":"version: 1\n","policy_signature":"cd""#,
        ))
        .unwrap();
        assert_eq!(p.name, "my-agent");
        // Byte-for-byte what was signed, not re-serialised.
        assert_eq!(
            p.deploy_json,
            r#"{"name":"my-agent","bundle_base64":"AA=="}"#
        );
        assert_eq!(p.signature.as_deref(), Some("ab"));
        assert_eq!(p.policy.as_deref(), Some("version: 1\n"));
        assert_eq!(p.policy_signature.as_deref(), Some("cd"));
        let bare = parse_merion_pack(&pack("")).unwrap();
        assert!(bare.signature.is_none() && bare.policy.is_none());
    }

    #[test]
    fn a_bad_pack_is_refused() {
        assert!(parse_merion_pack(b"nope").is_err());
        assert!(parse_merion_pack(br#"{"format":"other"}"#).is_err());
        // Name in the envelope must match the signed deploy body.
        let mismatched = br#"{"format":"merionpack/1","name":"a","deploy_json":"{\"name\":\"b\"}"}"#;
        assert!(parse_merion_pack(mismatched)
            .unwrap_err()
            .contains("does not match"));
        // Path-shaped names never reach the upstream URL.
        let evil = br#"{"format":"merionpack/1","name":"../x","deploy_json":"{\"name\":\"../x\"}"}"#;
        assert!(parse_merion_pack(evil).is_err());
    }
}
