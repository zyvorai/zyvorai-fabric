// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: LicenseRef-Zyvor-Production-1.0
//!
//! OpenAI-compatible AI gateway (preview).
//!
//! Clients point `OPENAI_BASE_URL` at:
//!   `https://<fabric>/api/ai/openai/<endpoint-name>`
//! and send `Authorization: Bearer <fvai_…>` (InferenceApiKey).
//! Fabric validates the key, enforces request quotas, then proxies to the
//! Maglev VIP or a ready replica. JWT is not required on this path.

use axum::{
    body::Body,
    extract::{ws::WebSocket, FromRequestParts, Path, Request, State},
    http::{header, request::Parts, HeaderMap, HeaderName, HeaderValue, Method, StatusCode},
    response::{IntoResponse, Response},
    Json,
};
use chrono::Utc;
use futures::StreamExt;
use std::sync::Arc;
use std::time::Duration;

use crate::server::AppState;

use super::keys::{self};
use super::types::{InferenceApiKey, InferenceDeployment, InferenceEndpoint};
use super::{audit, STORE_DEPLOYMENTS, STORE_ENDPOINTS};

/// ANY /api/ai/openai/{endpoint}/{*path}
pub async fn openai_gateway(
    State(state): State<Arc<AppState>>,
    Path((endpoint_name, path)): Path<(String, String)>,
    req: Request,
) -> Response {
    match gateway_inner(&state, &endpoint_name, &path, req).await {
        Ok(resp) => resp,
        Err((status, msg)) => (
            status,
            Json(serde_json::json!({
                "error": {
                    "message": msg,
                    "type": "fabric_ai_gateway",
                    "code": status.as_u16(),
                }
            })),
        )
            .into_response(),
    }
}

/// ANY /api/ai/openai/{endpoint}  (empty path → /v1 models hint)
pub async fn openai_gateway_root(
    State(state): State<Arc<AppState>>,
    Path(endpoint_name): Path<String>,
    req: Request,
) -> Response {
    openai_gateway(State(state), Path((endpoint_name, String::new())), req).await
}

async fn gateway_inner(
    state: &AppState,
    endpoint_name: &str,
    path: &str,
    req: Request,
) -> Result<Response, (StatusCode, String)> {
    let method = req.method().clone();
    if method == Method::OPTIONS {
        return Ok(cors_response());
    }

    let secret = extract_bearer(req.headers()).ok_or((
        StatusCode::UNAUTHORIZED,
        "missing or invalid Authorization: Bearer <api-key>".into(),
    ))?;
    if let Err(msg) = reject_control_plane_jwt(&secret) {
        return Err((StatusCode::UNAUTHORIZED, msg.into()));
    }
    let inference_jwt = inference_audience(&secret);

    let upstream_hint = if path.is_empty() {
        "/v1/models".to_string()
    } else if path.starts_with('/') {
        path.to_string()
    } else {
        format!("/{path}")
    };
    if !inference_path_allowed(&upstream_hint) {
        return Err((
            StatusCode::NOT_FOUND,
            format!("inference path '{upstream_hint}' is not served"),
        ));
    }

    let mut key = if inference_jwt {
        let (_sub, tenant) =
            crate::api::external_auth::verify_inference_token(&state.http_client, state, &secret)
                .await
                .map_err(|err| (StatusCode::UNAUTHORIZED, err))?;
        oidc_inference_key(endpoint_name, &tenant)
    } else {
        let key = keys::verify_api_key(state, &secret)
            .ok_or((StatusCode::UNAUTHORIZED, "invalid API key".into()))?;
        keys::ensure_current(&key, Utc::now().timestamp())
            .map_err(|e| (StatusCode::UNAUTHORIZED, e.to_string()))?;
        key
    };

    if key.endpoint != endpoint_name {
        return Err((
            StatusCode::FORBIDDEN,
            format!("API key is not scoped to endpoint '{endpoint_name}'"),
        ));
    }

    let ep: InferenceEndpoint = state
        .store
        .get_entity(STORE_ENDPOINTS, endpoint_name)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
        .ok_or((
            StatusCode::NOT_FOUND,
            format!("InferenceEndpoint '{endpoint_name}' not found"),
        ))?;

    let dep: InferenceDeployment = state
        .store
        .get_entity(STORE_DEPLOYMENTS, &ep.deployment)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
        .ok_or((
            StatusCode::BAD_GATEWAY,
            format!("deployment '{}' not found", ep.deployment),
        ))?;

    if key.prefix == "oidc" {
        match (&ep.tenant, &key.tenant) {
            (Some(expected), Some(got)) if expected == got => {}
            (Some(_), _) => {
                return Err((
                    StatusCode::FORBIDDEN,
                    "inference token tenant does not match the endpoint".into(),
                ));
            }
            (None, _) => {}
        }
    }

    if let Some(ref model_scope) = key.model {
        if &dep.model != model_scope {
            return Err((
                StatusCode::FORBIDDEN,
                format!("API key is scoped to model '{model_scope}'"),
            ));
        }
    }

    let (parts, body) = req.into_parts();
    let limit = body_limit();
    let body_bytes = axum::body::to_bytes(body, limit).await.map_err(|_| {
        (
            StatusCode::PAYLOAD_TOO_LARGE,
            format!("body exceeds {limit} bytes"),
        )
    })?;
    let tokens = keys::requested_tokens(&body_bytes);
    let context = context_tokens(&body_bytes);
    let cap = tighter_context_limit(
        context_limit(),
        policy_context_cap(state, key.tenant.as_deref()),
    );
    if !context_allowed(context, cap) {
        return Err((
            StatusCode::PAYLOAD_TOO_LARGE,
            format!("context of {context} tokens exceeds {cap}"),
        ));
    }
    let (prompt_tokens, generated_tokens) = token_parts(&body_bytes);
    if let Err(msg) = generation_allowed(
        prompt_tokens,
        generated_tokens,
        max_prompt_tokens(),
        max_generated_tokens(),
    ) {
        return Err((StatusCode::PAYLOAD_TOO_LARGE, msg.into()));
    }
    if let Some(daily) = daily_tokens() {
        admit_daily_routed(state, tokens, daily).await?;
    }
    let mut streams = StreamHold {
        store: state.store.clone(),
        ids: Vec::new(),
        remote: None,
        replicated: super::raft::peers_configured(),
    };
    if let Some((rpm, tpm, cap)) = traffic_caps(global_rpm(), global_tpm(), global_streams()) {
        let admitted = admit_scope_routed(
            state,
            &mut streams.remote,
            "global",
            "fabric",
            tokens,
            rpm,
            tpm,
            cap,
        )
        .await?;
        if let Some(id) = admitted {
            streams.ids.push(id);
        }
    }
    if let Some((rpm, tpm, cap)) = traffic_caps(tenant_rpm(), tenant_tpm(), tenant_streams()) {
        if let Some(tenant) = tenant_rate_name(key.tenant.as_deref())
            .map_err(|e| (StatusCode::BAD_REQUEST, e.to_string()))?
        {
            let admitted = admit_scope_routed(
                state,
                &mut streams.remote,
                "tenant",
                tenant,
                tokens,
                rpm,
                tpm,
                cap,
            )
            .await?;
            if let Some(id) = admitted {
                streams.ids.push(id);
            }
        }
    }
    if let Some((rpm, tpm, cap)) = traffic_caps(project_rpm(), project_tpm(), project_streams()) {
        if let Some(project) = project_id(
            parts
                .headers
                .get("x-project-id")
                .and_then(|value| value.to_str().ok()),
        )
        .map_err(|e| (StatusCode::BAD_REQUEST, e.to_string()))?
        {
            let admitted = admit_scope_routed(
                state,
                &mut streams.remote,
                "project",
                project,
                tokens,
                rpm,
                tpm,
                cap,
            )
            .await?;
            if let Some(id) = admitted {
                streams.ids.push(id);
            }
        }
    }
    if let Some((rpm, tpm, cap)) = traffic_caps(gateway_rpm(), gateway_tpm(), gateway_streams()) {
        let admitted = admit_scope_routed(
            state,
            &mut streams.remote,
            "endpoint",
            endpoint_name,
            tokens,
            rpm,
            tpm,
            cap,
        )
        .await?;
        if let Some(id) = admitted {
            streams.ids.push(id);
        }
    }
    if let Some((rpm, tpm, cap)) = traffic_caps(model_rpm(), model_tpm(), model_streams()) {
        let admitted = admit_scope_routed(
            state,
            &mut streams.remote,
            "model",
            &dep.model,
            tokens,
            rpm,
            tpm,
            cap,
        )
        .await?;
        if let Some(id) = admitted {
            streams.ids.push(id);
        }
    }
    if let Some((rpm, tpm, cap)) = traffic_caps(user_rpm(), user_tpm(), user_streams()) {
        if let Some(user) = user_id(
            parts
                .headers
                .get("x-user-id")
                .and_then(|value| value.to_str().ok()),
        )
        .map_err(|e| (StatusCode::BAD_REQUEST, e.to_string()))?
        {
            let admitted = admit_scope_routed(
                state,
                &mut streams.remote,
                "user",
                user,
                tokens,
                rpm,
                tpm,
                cap,
            )
            .await?;
            if let Some(id) = admitted {
                streams.ids.push(id);
            }
        }
    }
    if super::circuit::is_open(&load_breaker(state, endpoint_name), Utc::now().timestamp()) {
        return Err((
            StatusCode::SERVICE_UNAVAILABLE,
            "inference circuit is open".into(),
        ));
    }
    request_deadline_secs(
        parts
            .headers
            .get("x-request-timeout")
            .and_then(|v| v.to_str().ok()),
        120,
        300,
    )
    .map_err(|e| (StatusCode::BAD_REQUEST, e.to_string()))?;
    let priority = parse_priority(
        parts
            .headers
            .get("x-request-priority")
            .and_then(|v| v.to_str().ok()),
    )
    .map_err(|e| (StatusCode::BAD_REQUEST, e.to_string()))?;
    let depths: Vec<u32> = dep
        .status
        .replicas
        .iter()
        .filter(|rep| rep.ready)
        .map(|rep| {
            rep.metrics
                .as_ref()
                .map(|metrics| metrics.queue_depth)
                .unwrap_or(0)
        })
        .collect();
    if shed_queue(&depths, shed_limit_for(shed_limit(), priority)) {
        return Err((
            StatusCode::SERVICE_UNAVAILABLE,
            "inference queue is shedding load".into(),
        ));
    }
    if key.prefix != "oidc" {
        key = keys::consume_request(state, &key.id, tokens)
            .await
            .map_err(|e| {
                let status =
                    if e.contains("quota") || e.contains("token") || e.contains("concurrency") {
                        StatusCode::TOO_MANY_REQUESTS
                    } else {
                        StatusCode::INTERNAL_SERVER_ERROR
                    };
                (status, e)
            })?;
    }
    let upstream_path_hint = upstream_hint.clone();
    let dry = dry_run_enabled();
    let mut ep = ep;
    let mut dep = dep;
    let mut route_label: Option<(String, String)> = None;
    if let Some(router) = super::router::router_for(state, endpoint_name) {
        let input = super::router::RouteInput::from_request(
            &body_bytes,
            &super::router::header_pairs(&parts.headers),
        );
        let decision = super::router::decide(&router, &input, rand::random::<u64>());
        let mut chosen = None;
        for idx in &decision.order {
            let cand = &router.candidates[*idx];
            let target_ep: Option<InferenceEndpoint> = state
                .store
                .get_entity(STORE_ENDPOINTS, &cand.endpoint)
                .ok()
                .flatten();
            let target_dep: Option<InferenceDeployment> = target_ep.as_ref().and_then(|e| {
                state
                    .store
                    .get_entity(STORE_DEPLOYMENTS, &e.deployment)
                    .ok()
                    .flatten()
            });
            if let (Some(e), Some(d)) = (target_ep, target_dep) {
                if dry || !upstream_targets(&e, &d).is_empty() {
                    chosen = Some((e, d, cand.name.clone()));
                    break;
                }
            }
        }
        let (e, d, variant) = chosen.ok_or((
            StatusCode::SERVICE_UNAVAILABLE,
            "no router candidate has a ready backend".to_string(),
        ))?;
        ep = e;
        dep = d;
        route_label = Some((endpoint_name.to_string(), variant));
    }
    let mut trace_ctx = None;
    let mut upstream_body = body_bytes.clone();
    if upstream_path_hint.contains("/v1/") && method == Method::POST {
        if let Some(policy) = super::traces::policy_for(state, endpoint_name) {
            if policy.capture_token_ids {
                upstream_body = super::traces::inject_capture_params(&body_bytes).into();
            }
            trace_ctx = Some(super::traces::TraceCtx {
                id: uuid::Uuid::new_v4().to_string(),
                started: std::time::Instant::now(),
                policy,
                endpoint: endpoint_name.to_string(),
                router: route_label.as_ref().map(|(r, _)| r.clone()),
                variant: route_label.as_ref().map(|(_, v)| v.clone()),
                target_endpoint: ep.name.clone(),
                model: dep.model.clone(),
                path: upstream_path_hint.clone(),
                tenant: key.tenant.clone(),
                user: parts
                    .headers
                    .get("x-user-id")
                    .and_then(|v| v.to_str().ok())
                    .map(str::to_string),
                project: parts
                    .headers
                    .get("x-project-id")
                    .and_then(|v| v.to_str().ok())
                    .map(str::to_string),
                request_body: body_bytes.to_vec(),
            });
        }
    }
    let trace_id = trace_ctx.as_ref().map(|c| c.id.clone());
    let req = Request::from_parts(parts.clone(), Body::from(upstream_body));
    let upstream_path = if path.is_empty() {
        "/v1/models".to_string()
    } else if path.starts_with('/') {
        path.to_string()
    } else {
        format!("/{path}")
    };

    if websocket_upgrade(&parts.headers) {
        let switched = proxy_websocket(
            parts,
            &ep,
            &dep,
            &upstream_path,
            streams,
            ConcurrencyGuard {
                store: state.store.clone(),
                key_id: if key.prefix == "oidc" {
                    String::new()
                } else {
                    key.id.clone()
                },
            },
        )
        .await;
        let status = switched
            .as_ref()
            .map(|resp| resp.status().as_u16())
            .unwrap_or(502);
        super::otel::emit_gateway(endpoint_name, status, tokens);
        return switched;
    }

    let resource = format!("ai/openai/{endpoint_name}/{path}");
    audit(
        state,
        &format!("apikey:{}", key.prefix),
        "INFER",
        &resource,
        "ATTEMPT",
    );

    let dry_run = dry;

    if dry_run && !janus_replica_ready(&dep) {
        audit(
            state,
            &format!("apikey:{}", key.prefix),
            "INFER",
            &resource,
            "SUCCESS",
        );
        if key.prefix != "oidc" {
            keys::release_concurrency(state, &key.id);
        }
        return Ok(dry_run_response(
            &method,
            &upstream_path,
            &dep.model,
            &body_bytes,
        ));
    }

    let proxied = proxy_upstream(
        &method,
        &ep,
        &dep,
        &upstream_path,
        req,
        ConcurrencyGuard {
            store: state.store.clone(),
            key_id: if key.prefix == "oidc" {
                String::new()
            } else {
                key.id.clone()
            },
        },
        streams,
        trace_ctx.map(|ctx| (state.store.clone(), ctx)),
    )
    .await;
    let outcome = match &proxied {
        Ok(resp) if resp.status().is_success() => "SUCCESS",
        _ => "FAILED",
    };
    let status = proxied
        .as_ref()
        .map(|resp| resp.status().as_u16())
        .unwrap_or(502);
    super::otel::emit_gateway(endpoint_name, status, tokens);
    record_breaker(state, endpoint_name, outcome != "SUCCESS");
    audit(
        state,
        &format!("apikey:{}", key.prefix),
        "INFER",
        &resource,
        outcome,
    );
    proxied.map(|mut resp| {
        if let Some((_, variant)) = &route_label {
            if let Ok(v) = HeaderValue::from_str(variant) {
                resp.headers_mut().insert("x-fabric-variant", v);
            }
        }
        if let Some(id) = trace_id.as_deref().and_then(|id| HeaderValue::from_str(id).ok()) {
            resp.headers_mut().insert("x-fabric-trace-id", id);
        }
        resp
    })
}

struct StreamHold {
    store: state_store::StateStore,
    ids: Vec<String>,
    remote: Option<String>,
    replicated: bool,
}

impl Drop for StreamHold {
    fn drop(&mut self) {
        if self.ids.is_empty() {
            return;
        }
        if let Some(addr) = self.remote.clone() {
            let ids = std::mem::take(&mut self.ids);
            tokio::spawn(async move {
                for id in ids {
                    let _ = remote_limit(&addr, super::raft::LeaseCommand::Release { id }).await;
                }
            });
            return;
        }
        if self.replicated {
            let ids = std::mem::take(&mut self.ids);
            tokio::spawn(async move {
                for id in ids {
                    let _ =
                        super::raft::write_counter(super::raft::LeaseCommand::Release { id }).await;
                }
            });
            return;
        }
        for id in &self.ids {
            release_stream(&self.store, id);
        }
    }
}

fn release_stream(store: &state_store::StateStore, id: &str) {
    let _ = store.update_entity_exclusive(
        super::STORE_RATE_COUNTERS,
        id,
        |counter: super::limits::RateCounter| {
            Ok::<_, String>(super::limits::release_inflight(counter))
        },
    );
}

struct ConcurrencyGuard {
    store: state_store::StateStore,
    key_id: String,
}

impl Drop for ConcurrencyGuard {
    fn drop(&mut self) {
        if !self.key_id.is_empty() {
            keys::release_concurrency_store(&self.store, &self.key_id);
        }
    }
}

fn inference_client(
) -> Result<(reqwest::Client, super::upstream::BackendTransport), (StatusCode, String)> {
    let transport = super::upstream::backend_transport(
        std::env::var("FLUXVM_AI_BACKEND_TLS").ok().as_deref() == Some("1"),
        std::env::var("FLUXVM_AI_BACKEND_CLIENT_CERT")
            .ok()
            .as_deref(),
        std::env::var("FLUXVM_AI_BACKEND_CLIENT_KEY")
            .ok()
            .as_deref(),
        std::env::var("FLUXVM_AI_BACKEND_CA").ok().as_deref(),
    )
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    let mut builder = reqwest::Client::builder().connect_timeout(Duration::from_secs(10));
    if let super::upstream::BackendTransport::Mtls { cert, key, ca } = &transport {
        let mut pem = std::fs::read(key).map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("backend client key: {e}"),
            )
        })?;
        if !pem.ends_with(b"\n") {
            pem.push(b'\n');
        }
        pem.extend(std::fs::read(cert).map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("backend client cert: {e}"),
            )
        })?);
        let identity = reqwest::Identity::from_pem(&pem).map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("backend client identity: {e}"),
            )
        })?;
        let ca_pem = std::fs::read(ca).map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("backend CA: {e}"),
            )
        })?;
        let root = reqwest::Certificate::from_pem(&ca_pem).map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("backend CA: {e}"),
            )
        })?;
        builder = builder.identity(identity).add_root_certificate(root);
    }
    let client = builder
        .build()
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    Ok((client, transport))
}

async fn proxy_upstream(
    method: &Method,
    ep: &InferenceEndpoint,
    dep: &InferenceDeployment,
    upstream_path: &str,
    req: Request,
    guard: ConcurrencyGuard,
    streams: StreamHold,
    trace: Option<(state_store::StateStore, super::traces::TraceCtx)>,
) -> Result<Response, (StatusCode, String)> {
    let targets = upstream_targets(ep, dep);
    if targets.is_empty() {
        return Err((
            StatusCode::SERVICE_UNAVAILABLE,
            "no ready inference backend (set VIP or wait for replicas)".into(),
        ));
    }

    let session = req
        .headers()
        .get("x-session-id")
        .and_then(|value| value.to_str().ok())
        .map(str::to_string);
    let incoming_type = req
        .headers()
        .get(header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("application/json")
        .to_string();
    let deadline = Duration::from_secs(
        request_deadline_secs(
            req.headers()
                .get("x-request-timeout")
                .and_then(|value| value.to_str().ok()),
            120,
            300,
        )
        .map_err(|e| (StatusCode::BAD_REQUEST, e.to_string()))?,
    );
    let body_bytes = axum::body::to_bytes(req.into_body(), body_limit())
        .await
        .map_err(|_| {
            (
                StatusCode::PAYLOAD_TOO_LARGE,
                format!("body exceeds {} bytes", body_limit()),
            )
        })?;
    let streaming = wants_stream(&body_bytes);
    let vip_first = ep.vip.as_deref().is_some_and(|vip| !vip.is_empty());
    let mut targets = targets;
    apply_affinity(
        &mut targets,
        vip_first,
        &affinity_key(session.as_deref(), &body_bytes),
    );
    let (client, transport) = inference_client()?;

    let primary = targets[0].clone();
    let mut result = post_upstream(
        &client,
        method,
        &super::upstream::upstream_origin(&transport, &primary, upstream_path),
        &incoming_type,
        deadline,
        &body_bytes,
    )
    .await;
    if result.is_err() {
        if let Some(alt) = retry_before_stream(streaming, false, &primary, &targets) {
            result = post_upstream(
                &client,
                method,
                &super::upstream::upstream_origin(&transport, alt, upstream_path),
                &incoming_type,
                deadline,
                &body_bytes,
            )
            .await;
        }
    }
    let upstream_resp = result.map_err(|e| (StatusCode::BAD_GATEWAY, format!("upstream: {e}")))?;

    let status =
        StatusCode::from_u16(upstream_resp.status().as_u16()).unwrap_or(StatusCode::BAD_GATEWAY);
    let mut builder = Response::builder().status(status);
    if let Some(headers) = builder.headers_mut() {
        copy_response_headers(upstream_resp.headers(), headers);
    }
    let idle = idle_timeout();
    let mut sink = trace
        .map(|(store, ctx)| super::traces::TraceSink::new(store, ctx, status.as_u16(), streaming));
    let stream = async_stream::stream! {
        let _guard = guard;
        let _streams = streams;
        let mut chunks = upstream_resp.bytes_stream();
        loop {
            match tokio::time::timeout(idle, chunks.next()).await {
                Ok(Some(Ok(bytes))) => {
                    if let Some(sink) = sink.as_mut() {
                        sink.push(&bytes);
                    }
                    yield Ok::<_, std::io::Error>(bytes)
                }
                Ok(Some(Err(e))) => {
                    if let Some(sink) = sink.as_mut() {
                        sink.status = 502;
                    }
                    yield Err(std::io::Error::other(e));
                    break;
                }
                Ok(None) => break,
                Err(_) => {
                    if let Some(sink) = sink.as_mut() {
                        sink.status = 504;
                    }
                    yield Err(std::io::Error::new(
                        std::io::ErrorKind::TimedOut,
                        "upstream idle timeout",
                    ));
                    break;
                }
            }
        }
    };
    builder
        .body(Body::from_stream(stream))
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))
}

fn idle_timeout() -> Duration {
    let secs = std::env::var("FLUXVM_AI_GATEWAY_IDLE_SECS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(60u64);
    Duration::from_secs(secs)
}

fn body_limit() -> usize {
    std::env::var("FLUXVM_AI_GATEWAY_MAX_BODY")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(1024 * 1024)
}

fn global_rpm() -> Option<u64> {
    env_limit("FLUXVM_AI_GLOBAL_RPM")
}

fn tenant_rpm() -> Option<u64> {
    env_limit("FLUXVM_AI_TENANT_RPM")
}

fn gateway_rpm() -> Option<u64> {
    env_limit("FLUXVM_AI_GATEWAY_RPM")
}

fn project_rpm() -> Option<u64> {
    env_limit("FLUXVM_AI_PROJECT_RPM")
}

fn model_rpm() -> Option<u64> {
    env_limit("FLUXVM_AI_MODEL_RPM")
}

fn user_rpm() -> Option<u64> {
    env_limit("FLUXVM_AI_USER_RPM")
}

fn global_tpm() -> Option<u64> {
    env_limit("FLUXVM_AI_GLOBAL_TPM")
}

fn tenant_tpm() -> Option<u64> {
    env_limit("FLUXVM_AI_TENANT_TPM")
}

fn project_tpm() -> Option<u64> {
    env_limit("FLUXVM_AI_PROJECT_TPM")
}

fn gateway_tpm() -> Option<u64> {
    env_limit("FLUXVM_AI_GATEWAY_TPM")
}

fn model_tpm() -> Option<u64> {
    env_limit("FLUXVM_AI_MODEL_TPM")
}

fn user_tpm() -> Option<u64> {
    env_limit("FLUXVM_AI_USER_TPM")
}

fn global_streams() -> Option<u32> {
    env_streams("FLUXVM_AI_GLOBAL_STREAMS")
}

fn tenant_streams() -> Option<u32> {
    env_streams("FLUXVM_AI_TENANT_STREAMS")
}

fn project_streams() -> Option<u32> {
    env_streams("FLUXVM_AI_PROJECT_STREAMS")
}

fn gateway_streams() -> Option<u32> {
    env_streams("FLUXVM_AI_GATEWAY_STREAMS")
}

fn model_streams() -> Option<u32> {
    env_streams("FLUXVM_AI_MODEL_STREAMS")
}

fn user_streams() -> Option<u32> {
    env_streams("FLUXVM_AI_USER_STREAMS")
}

fn env_streams(name: &str) -> Option<u32> {
    env_limit(name).and_then(|value| u32::try_from(value).ok())
}

/// One stored window when any cap is set. Zero means that cap is off.
pub fn traffic_caps(
    rpm: Option<u64>,
    tpm: Option<u64>,
    streams: Option<u32>,
) -> Option<(u64, u64, u32)> {
    let rpm = rpm.unwrap_or(0);
    let tpm = tpm.unwrap_or(0);
    let streams = streams.unwrap_or(0);
    if rpm == 0 && tpm == 0 && streams == 0 {
        None
    } else {
        Some((rpm, tpm, streams))
    }
}

/// One stored window when either cap is set. Zero means that cap is off.
pub fn rate_window(rpm: Option<u64>, tpm: Option<u64>) -> Option<(u64, u64)> {
    let rpm = rpm.unwrap_or(0);
    let tpm = tpm.unwrap_or(0);
    if rpm == 0 && tpm == 0 {
        None
    } else {
        Some((rpm, tpm))
    }
}

fn env_limit(name: &str) -> Option<u64> {
    std::env::var(name)
        .ok()
        .and_then(|value| value.parse().ok())
        .filter(|n| *n > 0)
}

/// A key with no tenant skips the tenant window. A tenant id is a counter name.
pub fn tenant_rate_name(tenant: Option<&str>) -> Result<Option<&str>, &'static str> {
    let Some(raw) = tenant.map(str::trim).filter(|value| !value.is_empty()) else {
        return Ok(None);
    };
    if raw.len() > 128 || raw.contains('/') || raw.contains("..") {
        return Err("tenant id cannot be used as a rate-limit scope");
    }
    Ok(Some(raw))
}

/// `x-user-id` is optional. When present it must be a short stable identifier.
pub fn user_id(header: Option<&str>) -> Result<Option<&str>, &'static str> {
    labeled_id(
        header,
        "x-user-id must be 1 to 128 letters, digits, or . _ @ -",
    )
}

/// `x-project-id` uses the same shape as `x-user-id`.
pub fn project_id(header: Option<&str>) -> Result<Option<&str>, &'static str> {
    labeled_id(
        header,
        "x-project-id must be 1 to 128 letters, digits, or . _ @ -",
    )
}

fn labeled_id<'a>(
    header: Option<&'a str>,
    bad: &'static str,
) -> Result<Option<&'a str>, &'static str> {
    let Some(raw) = header.map(str::trim).filter(|value| !value.is_empty()) else {
        return Ok(None);
    };
    let ok = raw.len() <= 128
        && !raw.contains("..")
        && raw
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.' | '@'));
    if ok {
        Ok(Some(raw))
    } else {
        Err(bad)
    }
}

/// `x-request-timeout` is a whole number of seconds. Missing means `default_secs`.
pub fn request_deadline_secs(
    header: Option<&str>,
    default_secs: u64,
    max_secs: u64,
) -> Result<u64, &'static str> {
    let Some(raw) = header.map(str::trim).filter(|value| !value.is_empty()) else {
        return Ok(default_secs);
    };
    let secs: u64 = raw
        .parse()
        .map_err(|_| "x-request-timeout must be a whole number of seconds")?;
    if secs == 0 || secs > max_secs {
        return Err("x-request-timeout is outside the allowed range");
    }
    Ok(secs)
}

/// Shed when any ready replica's queue is above `limit`. `0` disables shedding.
pub fn shed_queue(queue_depths: &[u32], limit: u32) -> bool {
    limit > 0 && queue_depths.iter().any(|depth| *depth > limit)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RequestPriority {
    Low,
    Normal,
    High,
}

pub fn parse_priority(header: Option<&str>) -> Result<RequestPriority, &'static str> {
    match header.map(str::trim).filter(|value| !value.is_empty()) {
        None => Ok(RequestPriority::Normal),
        Some(value) if value.eq_ignore_ascii_case("low") => Ok(RequestPriority::Low),
        Some(value) if value.eq_ignore_ascii_case("normal") => Ok(RequestPriority::Normal),
        Some(value) if value.eq_ignore_ascii_case("high") => Ok(RequestPriority::High),
        Some(_) => Err("x-request-priority must be low, normal, or high"),
    }
}

/// Low sheds at half the base queue, high at double. A base of 0 disables shedding.
pub fn shed_limit_for(base: u32, priority: RequestPriority) -> u32 {
    if base == 0 {
        return 0;
    }
    match priority {
        RequestPriority::Low => (base / 2).max(1),
        RequestPriority::Normal => base,
        RequestPriority::High => base.saturating_mul(2),
    }
}

fn shed_limit() -> u32 {
    std::env::var("FLUXVM_AI_SHED_QUEUE")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(0)
}

/// Prompt characters divided by 4, plus `max_tokens`. The prompt text is not kept.
pub fn context_tokens(body: &[u8]) -> u32 {
    let (prompt, generated) = token_parts(body);
    u32::saturating_add(prompt, generated)
}

pub fn token_parts(body: &[u8]) -> (u32, u32) {
    let Ok(value) = serde_json::from_slice::<serde_json::Value>(body) else {
        return (0, 0);
    };
    let generated = value
        .get("max_tokens")
        .and_then(|v| v.as_u64())
        .unwrap_or(0)
        .min(u64::from(u32::MAX)) as u32;
    let prompt = ((prompt_chars(&value) as u64) / 4).min(u64::from(u32::MAX)) as u32;
    (prompt, generated)
}

pub fn generation_allowed(
    prompt: u32,
    generated: u32,
    max_prompt: u32,
    max_generated: u32,
) -> Result<(), &'static str> {
    if max_prompt > 0 && prompt > max_prompt {
        return Err("prompt exceeds the maximum prompt tokens");
    }
    if max_generated > 0 && generated > max_generated {
        return Err("max_tokens exceeds the maximum generated tokens");
    }
    Ok(())
}

fn max_prompt_tokens() -> u32 {
    env_u32("FLUXVM_AI_MAX_PROMPT_TOKENS")
}

fn max_generated_tokens() -> u32 {
    env_u32("FLUXVM_AI_MAX_GENERATED_TOKENS")
}

fn daily_tokens() -> Option<u64> {
    env_limit("FLUXVM_AI_DAILY_TOKENS")
}

fn env_u32(name: &str) -> u32 {
    std::env::var(name)
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(0)
}

fn prompt_chars(value: &serde_json::Value) -> usize {
    let mut count = 0usize;
    for key in ["prompt", "input"] {
        if let Some(text) = value.get(key).and_then(|v| v.as_str()) {
            count = count.saturating_add(text.chars().count());
        }
    }
    if let Some(messages) = value.get("messages").and_then(|v| v.as_array()) {
        for message in messages {
            if let Some(content) = message.get("content") {
                count = count.saturating_add(content_chars(content));
            }
        }
    }
    count
}

fn content_chars(value: &serde_json::Value) -> usize {
    match value {
        serde_json::Value::String(text) => text.chars().count(),
        serde_json::Value::Array(parts) => parts
            .iter()
            .filter_map(|part| part.get("text").and_then(|text| text.as_str()))
            .map(|text| text.chars().count())
            .fold(0usize, usize::saturating_add),
        _ => 0,
    }
}

pub fn context_allowed(tokens: u32, limit: u32) -> bool {
    limit == 0 || tokens <= limit
}

/// The smaller positive cap. Zero means that side sets no cap.
pub fn tighter_context_limit(gateway: u32, policy: u32) -> u32 {
    match (gateway, policy) {
        (0, 0) => 0,
        (0, policy) => policy,
        (gateway, 0) => gateway,
        (gateway, policy) => gateway.min(policy),
    }
}

fn context_limit() -> u32 {
    std::env::var("FLUXVM_AI_MAX_CONTEXT")
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(32_768)
}

fn policy_context_cap(state: &AppState, tenant: Option<&str>) -> u32 {
    let Some(tenant) = tenant.filter(|tenant| !tenant.is_empty()) else {
        return 0;
    };
    state
        .store
        .get_entity::<super::policy::TenantAiPolicy>(super::STORE_POLICIES, tenant)
        .ok()
        .flatten()
        .map(|policy| policy.max_context_tokens)
        .unwrap_or(0)
}

fn cors_origin() -> String {
    std::env::var("FLUXVM_AI_CORS_ORIGIN").unwrap_or_else(|_| "*".into())
}

fn cors_origin_value() -> HeaderValue {
    HeaderValue::from_str(&cors_origin()).unwrap_or_else(|_| HeaderValue::from_static("*"))
}

fn cors_response() -> Response {
    Response::builder()
        .status(StatusCode::NO_CONTENT)
        .header(header::ACCESS_CONTROL_ALLOW_ORIGIN, cors_origin_value())
        .header(
            header::ACCESS_CONTROL_ALLOW_HEADERS,
            "authorization, content-type",
        )
        .header(header::ACCESS_CONTROL_ALLOW_METHODS, "GET, POST, OPTIONS")
        .body(Body::empty())
        .unwrap()
}

pub fn inference_path_allowed(path: &str) -> bool {
    let path = path.trim_start_matches('/');
    matches!(
        path,
        "" | "v1/models"
            | "v1/chat/completions"
            | "v1/completions"
            | "v1/embeddings"
            | "v1/rerank"
            | "v1/batches"
    ) || path.starts_with("v1/batches/")
}

pub fn reject_control_plane_jwt(secret: &str) -> Result<(), &'static str> {
    let jwt = secret.split('.').count() == 3 && !secret.starts_with("fvai_");
    let accept = std::env::var("FLUXVM_AI_GATEWAY_ACCEPT_JWT")
        .map(|v| v == "1")
        .unwrap_or(false);
    if jwt && !accept && !inference_audience(secret) {
        Err("inference gateway accepts scoped API keys")
    } else {
        Ok(())
    }
}

/// Unverified audience peek. It only chooses the verification path.
pub fn inference_audience(secret: &str) -> bool {
    let Some(payload) = secret.split('.').nth(1) else {
        return false;
    };
    let Ok(bytes) =
        base64::Engine::decode(&base64::engine::general_purpose::URL_SAFE_NO_PAD, payload).or_else(
            |_| base64::Engine::decode(&base64::engine::general_purpose::URL_SAFE, payload),
        )
    else {
        return false;
    };
    let Ok(value) = serde_json::from_slice::<serde_json::Value>(&bytes) else {
        return false;
    };
    match value.get("aud") {
        Some(serde_json::Value::String(aud)) => aud == "fabric-inference",
        Some(serde_json::Value::Array(items)) => items
            .iter()
            .any(|item| item.as_str() == Some("fabric-inference")),
        _ => false,
    }
}

fn oidc_inference_key(endpoint: &str, tenant: &str) -> super::types::InferenceApiKey {
    super::types::InferenceApiKey {
        id: format!("oidc:{tenant}"),
        name: "oidc".into(),
        endpoint: endpoint.to_string(),
        model: None,
        tenant: Some(tenant.to_string()),
        secret_hash: String::new(),
        prefix: "oidc".into(),
        request_quota: None,
        tokens_per_minute: None,
        max_concurrent: None,
        requests_used: 0,
        tokens_used_window: 0,
        window_started_unix: 0,
        inflight: 0,
        not_before_unix: 0,
        not_after_unix: 0,
        successor_id: None,
        created: Utc::now(),
        last_used: None,
    }
}

fn load_breaker(state: &AppState, endpoint: &str) -> super::circuit::Breaker {
    state
        .store
        .get_entity(super::STORE_CIRCUITS, endpoint)
        .ok()
        .flatten()
        .unwrap_or_default()
}

fn record_breaker(state: &AppState, endpoint: &str, failed: bool) {
    let now = Utc::now().timestamp();
    let next = super::circuit::observe(load_breaker(state, endpoint), failed, now);
    let _ = state
        .store
        .save_entity(super::STORE_CIRCUITS, endpoint, &next);
}

async fn remote_limit(
    addr: &str,
    command: super::raft::LeaseCommand,
) -> Result<super::raft::LeaseResponse, (StatusCode, String)> {
    let token = std::env::var("FLUXVM_AI_RAFT_TOKEN").unwrap_or_default();
    let url = format!("http://{addr}/api/ai/raft/limits");
    let response = reqwest::Client::new()
        .post(url)
        .header("x-raft-token", token)
        .json(&command)
        .send()
        .await
        .map_err(|err| {
            (
                StatusCode::SERVICE_UNAVAILABLE,
                format!("raft limit: {err}"),
            )
        })?;
    if response.status() == StatusCode::UNAUTHORIZED {
        return Err((StatusCode::UNAUTHORIZED, "raft token rejected".into()));
    }
    if !response.status().is_success() {
        return Err((
            StatusCode::SERVICE_UNAVAILABLE,
            format!("raft limit returned {}", response.status()),
        ));
    }
    response.json().await.map_err(|err| {
        (
            StatusCode::SERVICE_UNAVAILABLE,
            format!("raft limit: {err}"),
        )
    })
}

fn limit_reply(
    response: super::raft::LeaseResponse,
) -> Result<Option<String>, (StatusCode, String)> {
    match response {
        super::raft::LeaseResponse::Admitted { stream_id } => Ok(stream_id),
        super::raft::LeaseResponse::Rejected { message } => {
            Err((StatusCode::TOO_MANY_REQUESTS, message))
        }
        super::raft::LeaseResponse::Released => Ok(None),
        super::raft::LeaseResponse::Leader { .. }
        | super::raft::LeaseResponse::AuditLinked { .. } => Err((
            StatusCode::SERVICE_UNAVAILABLE,
            "unexpected raft limit reply".into(),
        )),
    }
}

// One argument per limit the caller already has; a params struct would only move the list.
#[allow(clippy::too_many_arguments)]
async fn admit_scope_routed(
    state: &AppState,
    remote: &mut Option<String>,
    scope: &str,
    name: &str,
    tokens: u64,
    rpm: u64,
    tpm: u64,
    cap: u32,
) -> Result<Option<String>, (StatusCode, String)> {
    let id = super::limits::counter_id(scope, name);
    match super::raft::counter_home() {
        super::raft::CounterHome::Local if super::raft::peers_configured() => {
            let response = super::raft::write_counter(super::raft::LeaseCommand::Admit {
                id,
                now: 0,
                tokens,
                rpm,
                tpm,
                streams: cap,
            })
            .await
            .map_err(|err| (StatusCode::SERVICE_UNAVAILABLE, err))?;
            limit_reply(response)
        }
        super::raft::CounterHome::Local => admit_scope(state, scope, name, tokens, rpm, tpm, cap)
            .map_err(|err| (StatusCode::TOO_MANY_REQUESTS, err)),
        super::raft::CounterHome::Leader(addr) => {
            if remote.is_none() {
                *remote = Some(addr.clone());
            }
            let response = remote_limit(
                &addr,
                super::raft::LeaseCommand::Admit {
                    id,
                    now: 0,
                    tokens,
                    rpm,
                    tpm,
                    streams: cap,
                },
            )
            .await?;
            limit_reply(response)
        }
        super::raft::CounterHome::Unavailable => Err((
            StatusCode::SERVICE_UNAVAILABLE,
            "AI raft leader is not elected; rate counters are not split".into(),
        )),
    }
}

async fn admit_daily_routed(
    state: &AppState,
    tokens: u64,
    daily: u64,
) -> Result<(), (StatusCode, String)> {
    let id = super::limits::counter_id("global", "day");
    match super::raft::counter_home() {
        super::raft::CounterHome::Local if super::raft::peers_configured() => {
            let response = super::raft::write_counter(super::raft::LeaseCommand::AdmitDay {
                id,
                now: 0,
                tokens,
                daily,
            })
            .await
            .map_err(|err| (StatusCode::SERVICE_UNAVAILABLE, err))?;
            limit_reply(response).map(|_| ())
        }
        super::raft::CounterHome::Local => {
            admit_daily(state, tokens, daily).map_err(|err| (StatusCode::TOO_MANY_REQUESTS, err))
        }
        super::raft::CounterHome::Leader(addr) => {
            let response = remote_limit(
                &addr,
                super::raft::LeaseCommand::AdmitDay {
                    id,
                    now: 0,
                    tokens,
                    daily,
                },
            )
            .await?;
            limit_reply(response).map(|_| ())
        }
        super::raft::CounterHome::Unavailable => Err((
            StatusCode::SERVICE_UNAVAILABLE,
            "AI raft leader is not elected; rate counters are not split".into(),
        )),
    }
}

fn admit_scope(
    state: &AppState,
    scope: &str,
    name: &str,
    tokens: u64,
    rpm: u64,
    tpm: u64,
    streams: u32,
) -> Result<Option<String>, String> {
    let id = super::limits::counter_id(scope, name);
    let current = state
        .store
        .get_entity::<super::limits::RateCounter>(super::STORE_RATE_COUNTERS, &id)
        .ok()
        .flatten()
        .unwrap_or(super::limits::RateCounter {
            id: id.clone(),
            window_started_unix: 0,
            requests: 0,
            tokens: 0,
            day_started_unix: 0,
            day_tokens: 0,
            inflight: 0,
        });
    if state
        .store
        .get_entity::<super::limits::RateCounter>(super::STORE_RATE_COUNTERS, &id)
        .ok()
        .flatten()
        .is_none()
    {
        state
            .store
            .save_entity(super::STORE_RATE_COUNTERS, &id, &current)
            .map_err(|e| e.to_string())?;
    }
    let updated = state
        .store
        .update_entity_exclusive(
            super::STORE_RATE_COUNTERS,
            &id,
            |counter: super::limits::RateCounter| {
                super::limits::admit(counter, Utc::now().timestamp(), tokens, rpm, tpm, streams)
            },
        )
        .map_err(|e| e.to_string())?;
    let _ = updated;
    Ok(if streams > 0 { Some(id) } else { None })
}

fn admit_daily(state: &AppState, tokens: u64, daily: u64) -> Result<(), String> {
    let id = super::limits::counter_id("global", "day");
    let current = super::limits::RateCounter {
        id: id.clone(),
        window_started_unix: 0,
        requests: 0,
        tokens: 0,
        day_started_unix: 0,
        day_tokens: 0,
        inflight: 0,
    };
    if state
        .store
        .get_entity::<super::limits::RateCounter>(super::STORE_RATE_COUNTERS, &id)
        .ok()
        .flatten()
        .is_none()
    {
        state
            .store
            .save_entity(super::STORE_RATE_COUNTERS, &id, &current)
            .map_err(|e| e.to_string())?;
    }
    state
        .store
        .update_entity_exclusive(
            super::STORE_RATE_COUNTERS,
            &id,
            |counter: super::limits::RateCounter| {
                super::limits::admit_day(counter, Utc::now().timestamp(), tokens, daily)
            },
        )
        .map(|_| ())
        .map_err(|e| e.to_string())
}

fn copy_response_headers(from: &HeaderMap, to: &mut HeaderMap) {
    for (name, value) in from {
        if is_hop(name.as_str()) {
            continue;
        }
        if let (Ok(n), Ok(v)) = (
            HeaderName::from_bytes(name.as_str().as_bytes()),
            HeaderValue::from_bytes(value.as_bytes()),
        ) {
            to.insert(n, v);
        }
    }
    to.insert(header::ACCESS_CONTROL_ALLOW_ORIGIN, cors_origin_value());
}

fn is_hop(name: &str) -> bool {
    matches!(
        name.to_ascii_lowercase().as_str(),
        "connection"
            | "keep-alive"
            | "proxy-authenticate"
            | "proxy-authorization"
            | "te"
            | "trailers"
            | "transfer-encoding"
            | "upgrade"
            | "content-length"
    )
}

fn extract_bearer(headers: &axum::http::HeaderMap) -> Option<String> {
    let raw = headers.get(header::AUTHORIZATION)?.to_str().ok()?;
    let token = raw
        .strip_prefix("Bearer ")
        .or_else(|| raw.strip_prefix("bearer "))?
        .trim();
    if token.is_empty() {
        None
    } else {
        Some(token.to_string())
    }
}

fn upstream_targets(ep: &InferenceEndpoint, dep: &InferenceDeployment) -> Vec<String> {
    let mut targets = Vec::new();
    if let Some(vip) = ep.vip.as_deref().filter(|vip| !vip.is_empty()) {
        targets.push(format!("{vip}:{}", ep.port));
    }
    for rep in &dep.status.replicas {
        if !rep.ready || rep.draining {
            continue;
        }
        if let Some(addr) = rep.address.as_deref().filter(|addr| !addr.is_empty()) {
            let target = if addr.contains(':') {
                addr.to_string()
            } else {
                format!("{addr}:{}", ep.port)
            };
            if !targets.iter().any(|existing| existing == &target) {
                targets.push(target);
            }
        }
    }
    targets
}

/// One other backend, and only when no response has started.
/// Streaming requests are not retried.
pub fn retry_before_stream<'a>(
    streaming: bool,
    response_started: bool,
    tried: &str,
    candidates: &'a [String],
) -> Option<&'a str> {
    if streaming || response_started || tried.is_empty() {
        return None;
    }
    candidates
        .iter()
        .find(|candidate| candidate.as_str() != tried)
        .map(String::as_str)
}

pub fn affinity_index(key: &str, len: usize) -> Option<usize> {
    if key.is_empty() || len == 0 {
        return None;
    }
    let mut hash = 2166136261u32;
    for byte in key.as_bytes() {
        hash ^= u32::from(*byte);
        hash = hash.wrapping_mul(16777619);
    }
    Some(hash as usize % len)
}

/// The same key sticks to the same replica even if the slice is rotated.
pub fn sticky_target(replicas: &[String], key: &str) -> Option<String> {
    if replicas.is_empty() {
        return None;
    }
    let mut ordered = replicas.to_vec();
    ordered.sort();
    let index = affinity_index(key, ordered.len())?;
    Some(ordered[index].clone())
}

/// True when a ready replica already points at the configured Janus upstream.
fn dry_run_enabled() -> bool {
    std::env::var("FLUXVM_AI_DRY_RUN")
        .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
        .unwrap_or(false)
}

fn janus_replica_ready(dep: &InferenceDeployment) -> bool {
    let Some(url) = super::janus::janus_url() else {
        return false;
    };
    let hostport = super::janus::replica_hostport(&url);
    dep.status
        .replicas
        .iter()
        .any(|rep| rep.ready && rep.address.as_deref() == Some(hostport.as_str()))
}

/// Bearer for the Janus OpenAI shim. Only set when the upstream URL is that host.
fn janus_upstream_key(url: &str) -> Option<String> {
    let configured = super::janus::janus_url()?;
    let hostport = super::janus::replica_hostport(&configured);
    if !url.contains(&hostport) {
        return None;
    }
    std::env::var("FLUXVM_AI_JANUS_API_KEY")
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

/// Session id wins. Otherwise the first 64 characters of the prompt are the prefix.
/// The key is used only to order backends and is not written to the audit log.
fn affinity_key(session: Option<&str>, body: &[u8]) -> String {
    if let Some(session) = session.map(str::trim).filter(|value| !value.is_empty()) {
        return session.to_string();
    }
    let Ok(value) = serde_json::from_slice::<serde_json::Value>(body) else {
        return String::new();
    };
    let mut text = String::new();
    if let Some(prompt) = value.get("prompt").and_then(|item| item.as_str()) {
        text.push_str(prompt);
    } else if let Some(messages) = value.get("messages").and_then(|item| item.as_array()) {
        for message in messages {
            if let Some(content) = message.get("content").and_then(|item| item.as_str()) {
                text.push_str(content);
                break;
            }
        }
    }
    text.chars().take(64).collect()
}

/// Keep a VIP in front. Move the sticky replica to the first replica slot.
pub fn apply_affinity(targets: &mut Vec<String>, vip_first: bool, key: &str) {
    let start = usize::from(vip_first && !targets.is_empty());
    if start >= targets.len() {
        return;
    }
    let Some(chosen) = sticky_target(&targets[start..], key) else {
        return;
    };
    if targets.get(start).is_some_and(|target| target == &chosen) {
        return;
    }
    let Some(pos) = targets.iter().position(|target| target == &chosen) else {
        return;
    };
    let chosen = targets.remove(pos);
    targets.insert(start, chosen);
}

async fn post_upstream(
    client: &reqwest::Client,
    method: &Method,
    url: &str,
    content_type: &str,
    deadline: Duration,
    body: &[u8],
) -> Result<reqwest::Response, String> {
    let mut request = client
        .request(
            reqwest::Method::from_bytes(method.as_str().as_bytes())
                .unwrap_or(reqwest::Method::POST),
            url,
        )
        .header(header::CONTENT_TYPE, content_type)
        .timeout(deadline)
        .body(body.to_vec());
    if let Some(key) = janus_upstream_key(url) {
        request = request.header(header::AUTHORIZATION, format!("Bearer {key}"));
    }
    request.send().await.map_err(|e| e.to_string())
}

fn dry_run_response(method: &Method, path: &str, model: &str, body_bytes: &[u8]) -> Response {
    if wants_stream(body_bytes) {
        let chunks = dry_run_sse_chunks(model);
        let stream = futures::stream::iter(
            chunks
                .into_iter()
                .map(|c| Ok::<_, std::io::Error>(axum::body::Bytes::from(c))),
        );
        return Response::builder()
            .status(StatusCode::OK)
            .header(header::CONTENT_TYPE, "text/event-stream")
            .header(header::ACCESS_CONTROL_ALLOW_ORIGIN, "*")
            .body(Body::from_stream(stream))
            .unwrap();
    }
    let body = if path.contains("chat/completions")
        || (method == Method::POST && path.contains("completions"))
    {
        serde_json::json!({
            "id": "chatcmpl-fabric-dry-run",
            "object": "chat.completion",
            "created": Utc::now().timestamp(),
            "model": model,
            "choices": [{
                "index": 0,
                "message": {
                    "role": "assistant",
                    "content": "Fabric AI gateway dry-run: API key accepted."
                },
                "finish_reason": "stop"
            }],
            "usage": {"prompt_tokens": 0, "completion_tokens": 8, "total_tokens": 8}
        })
    } else if path.contains("/models") {
        serde_json::json!({
            "object": "list",
            "data": [{"id": model, "object": "model", "owned_by": "fabric"}]
        })
    } else {
        serde_json::json!({
            "object": "fabric.ai.dry_run",
            "model": model,
            "path": path,
            "message": "API key accepted (FLUXVM_AI_DRY_RUN=1); no GPU upstream"
        })
    };
    (
        StatusCode::OK,
        [
            (header::CONTENT_TYPE, "application/json"),
            (header::ACCESS_CONTROL_ALLOW_ORIGIN, "*"),
        ],
        Json(body),
    )
        .into_response()
}

pub fn wants_stream(body: &[u8]) -> bool {
    let text = String::from_utf8_lossy(body);
    text.contains("\"stream\":true") || text.contains("\"stream\": true")
}

/// Two SSE chunks. The gateway yields them as separate stream items.
pub fn dry_run_sse_chunks(model: &str) -> Vec<String> {
    vec![
        format!("data: {{\"model\":\"{model}\",\"choices\":[{{\"delta\":{{\"content\":\"Fabric\"}}}}]}}\n\n"),
        "data: [DONE]\n\n".into(),
    ]
}

/// Touch key usage for unit tests / quota math.
pub fn would_reject_quota(key: &InferenceApiKey) -> bool {
    key.request_quota.is_some_and(|q| key.requests_used >= q)
}

fn mtls_websocket_config(
    cert_path: &str,
    key_path: &str,
    ca_path: &str,
) -> Result<rustls::ClientConfig, String> {
    let ca = std::fs::read(ca_path).map_err(|err| format!("backend CA: {err}"))?;
    let mut roots = rustls::RootCertStore::empty();
    let mut added = false;
    for item in rustls_pemfile::certs(&mut std::io::Cursor::new(ca)) {
        roots
            .add(item.map_err(|err| format!("backend CA: {err}"))?)
            .map_err(|err| format!("backend CA: {err}"))?;
        added = true;
    }
    if !added {
        return Err("backend CA: no certificates".into());
    }
    let cert_pem = std::fs::read(cert_path).map_err(|err| format!("backend client cert: {err}"))?;
    let certs = rustls_pemfile::certs(&mut std::io::Cursor::new(cert_pem))
        .collect::<Result<Vec<_>, _>>()
        .map_err(|err| format!("backend client cert: {err}"))?;
    let key_pem = std::fs::read(key_path).map_err(|err| format!("backend client key: {err}"))?;
    let key = rustls_pemfile::private_key(&mut std::io::Cursor::new(key_pem))
        .map_err(|err| format!("backend client key: {err}"))?
        .ok_or_else(|| "backend client key: empty".to_string())?;
    rustls::ClientConfig::builder_with_provider(Arc::new(
        rustls::crypto::aws_lc_rs::default_provider(),
    ))
    .with_safe_default_protocol_versions()
    .map_err(|err| err.to_string())?
    .with_root_certificates(roots)
    .with_client_auth_cert(certs, key)
    .map_err(|err| format!("backend client identity: {err}"))
}

fn websocket_upgrade(headers: &HeaderMap) -> bool {
    headers
        .get(header::UPGRADE)
        .and_then(|value| value.to_str().ok())
        .is_some_and(|value| value.eq_ignore_ascii_case("websocket"))
}

async fn proxy_websocket(
    mut parts: Parts,
    ep: &InferenceEndpoint,
    dep: &InferenceDeployment,
    upstream_path: &str,
    streams: StreamHold,
    guard: ConcurrencyGuard,
) -> Result<Response, (StatusCode, String)> {
    let targets = upstream_targets(ep, dep);
    let primary = targets.first().cloned().ok_or((
        StatusCode::SERVICE_UNAVAILABLE,
        "no ready inference backend (set VIP or wait for replicas)".into(),
    ))?;
    let transport = super::upstream::backend_transport(
        std::env::var("FLUXVM_AI_BACKEND_TLS").ok().as_deref() == Some("1"),
        std::env::var("FLUXVM_AI_BACKEND_CLIENT_CERT")
            .ok()
            .as_deref(),
        std::env::var("FLUXVM_AI_BACKEND_CLIENT_KEY")
            .ok()
            .as_deref(),
        std::env::var("FLUXVM_AI_BACKEND_CA").ok().as_deref(),
    )
    .map_err(|err| (StatusCode::INTERNAL_SERVER_ERROR, err.to_string()))?;
    let url = super::upstream::websocket_origin(&transport, &primary, upstream_path);
    let connected = match &transport {
        super::upstream::BackendTransport::Mtls { cert, key, ca } => {
            let config = mtls_websocket_config(cert, key, ca)
                .map_err(|err| (StatusCode::INTERNAL_SERVER_ERROR, err))?;
            tokio_tungstenite::connect_async_tls_with_config(
                url.as_str(),
                None,
                false,
                Some(tokio_tungstenite::Connector::Rustls(Arc::new(config))),
            )
            .await
            .map(|(stream, _)| stream)
        }
        _ => tokio_tungstenite::connect_async(&url)
            .await
            .map(|(stream, _)| stream),
    };
    let upstream = connected.map_err(|err| {
        (
            StatusCode::BAD_GATEWAY,
            format!("upstream websocket: {err}"),
        )
    })?;
    let upgrade = axum::extract::ws::WebSocketUpgrade::from_request_parts(&mut parts, &())
        .await
        .map_err(|err| (StatusCode::BAD_REQUEST, err.to_string()))?;
    Ok(upgrade.on_upgrade(move |socket| async move {
        let _guard = guard;
        let _streams = streams;
        bridge_websocket(socket, upstream).await;
    }))
}

async fn bridge_websocket(
    mut down: WebSocket,
    upstream: tokio_tungstenite::WebSocketStream<
        tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>,
    >,
) {
    use axum::extract::ws::Message as Down;
    use futures::SinkExt;
    use tokio_tungstenite::tungstenite::Message as Up;
    let (mut up_sink, mut up_stream) = upstream.split();
    loop {
        tokio::select! {
            incoming = down.recv() => {
                let Some(Ok(message)) = incoming else { break };
                let outgoing = match message {
                    Down::Text(text) => Up::text(text.to_string()),
                    Down::Binary(bytes) => Up::binary(bytes),
                    Down::Ping(bytes) => Up::Ping(bytes),
                    Down::Pong(bytes) => Up::Pong(bytes),
                    Down::Close(_) => {
                        let _ = up_sink.send(Up::Close(None)).await;
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
                    Up::Text(text) => Down::Text(text.to_string().into()),
                    Up::Binary(bytes) => Down::Binary(bytes),
                    Up::Ping(bytes) => Down::Ping(bytes),
                    Up::Pong(bytes) => Down::Pong(bytes),
                    Up::Close(_) => {
                        let _ = down.send(Down::Close(None)).await;
                        break;
                    }
                    Up::Frame(_) => continue,
                };
                if down.send(incoming).await.is_err() {
                    break;
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quota_gate() {
        let mut key = InferenceApiKey {
            id: "x".into(),
            name: "n".into(),
            endpoint: "e".into(),
            model: None,
            tenant: None,
            secret_hash: "h".into(),
            prefix: "fvai_".into(),
            request_quota: Some(2),
            tokens_per_minute: None,
            max_concurrent: None,
            requests_used: 2,
            tokens_used_window: 0,
            window_started_unix: 0,
            inflight: 0,
            not_before_unix: 0,
            not_after_unix: 0,
            successor_id: None,
            created: Utc::now(),
            last_used: None,
        };
        assert!(would_reject_quota(&key));
        key.requests_used = 1;
        assert!(!would_reject_quota(&key));
    }

    #[test]
    fn deadline_and_shed_gates() {
        assert_eq!(request_deadline_secs(None, 120, 300).unwrap(), 120);
        assert_eq!(request_deadline_secs(Some("30"), 120, 300).unwrap(), 30);
        assert!(request_deadline_secs(Some("0"), 120, 300).is_err());
        assert!(request_deadline_secs(Some("301"), 120, 300).is_err());
        assert!(!shed_queue(&[10, 20], 0));
        assert!(!shed_queue(&[10, 20], 20));
        assert!(shed_queue(&[10, 21], 20));
        assert_eq!(shed_limit_for(20, parse_priority(Some("low")).unwrap()), 10);
        assert_eq!(shed_limit_for(20, parse_priority(None).unwrap()), 20);
        assert_eq!(
            shed_limit_for(20, parse_priority(Some("HIGH")).unwrap()),
            40
        );
        assert_eq!(shed_limit_for(0, RequestPriority::Low), 0);
        assert!(parse_priority(Some("urgent")).is_err());
        assert_eq!(user_id(None).unwrap(), None);
        assert_eq!(user_id(Some("ada@lab")).unwrap(), Some("ada@lab"));
        assert!(user_id(Some("../root")).is_err());
        assert_eq!(tenant_rate_name(None).unwrap(), None);
        assert_eq!(tenant_rate_name(Some("acme")).unwrap(), Some("acme"));
        assert!(tenant_rate_name(Some("../acme")).is_err());
        assert_eq!(project_id(Some("lab-1")).unwrap(), Some("lab-1"));
        assert!(project_id(Some("a/b")).is_err());
    }

    #[test]
    fn session_affinity_keeps_the_vip_and_sticks_the_same_prefix() {
        let key = affinity_key(None, br#"{"prompt":"abcdefghijklmnopqrstuvwxyz"}"#);
        assert_eq!(key, "abcdefghijklmnopqrstuvwxyz");
        assert_eq!(
            affinity_key(Some("sess-1"), br#"{"prompt":"other"}"#),
            "sess-1"
        );
        assert_eq!(affinity_index("sess-1", 3), affinity_index("sess-1", 3));
        let replicas = vec![
            "10.0.0.2:8000".to_string(),
            "10.0.0.3:8000".to_string(),
            "10.0.0.4:8000".to_string(),
        ];
        let chosen = sticky_target(&replicas, "sess-1").unwrap();
        let mut targets = vec!["10.96.0.1:8000".to_string()];
        targets.extend(replicas);
        apply_affinity(&mut targets, true, "sess-1");
        assert_eq!(targets[0], "10.96.0.1:8000");
        assert_eq!(targets[1], chosen);
        apply_affinity(&mut targets, true, "sess-1");
        assert_eq!(targets[1], chosen);
    }

    #[test]
    fn retry_skips_streaming_and_a_started_response() {
        let targets = vec![String::from("10.0.0.1:8000"), String::from("10.0.0.2:8000")];
        assert_eq!(
            retry_before_stream(false, false, &targets[0], &targets),
            Some("10.0.0.2:8000")
        );
        assert_eq!(
            retry_before_stream(true, false, &targets[0], &targets),
            None
        );
        assert_eq!(
            retry_before_stream(false, true, &targets[0], &targets),
            None
        );
        assert_eq!(retry_before_stream(false, false, "", &targets), None);
    }

    #[test]
    fn context_counts_prompt_and_max_tokens() {
        let body = br#"{"messages":[{"role":"user","content":"hello world!!"}],"max_tokens":8}"#;
        assert_eq!(context_tokens(body), 11);
        assert!(context_allowed(11, 32));
        assert!(!context_allowed(33, 32));
        assert!(context_allowed(100, 0));
        assert_eq!(tighter_context_limit(32_768, 1024), 1024);
        assert_eq!(tighter_context_limit(0, 0), 0);
        assert_eq!(token_parts(body), (3, 8));
        assert!(generation_allowed(3, 8, 0, 0).is_ok());
        assert!(generation_allowed(3, 8, 2, 0).is_err());
        assert!(generation_allowed(3, 8, 0, 7).is_err());
        assert_eq!(rate_window(None, None), None);
        assert_eq!(rate_window(Some(10), None), Some((10, 0)));
        assert_eq!(rate_window(None, Some(40)), Some((0, 40)));
        assert_eq!(rate_window(Some(10), Some(40)), Some((10, 40)));
        assert_eq!(traffic_caps(None, None, None), None);
        assert_eq!(traffic_caps(None, None, Some(2)), Some((0, 0, 2)));
        assert_eq!(traffic_caps(Some(10), None, Some(2)), Some((10, 0, 2)));
    }

    #[test]
    fn dry_run_sse_is_two_chunks() {
        let body = br#"{"model":"m","stream": true}"#;
        assert!(wants_stream(body));
        assert!(!wants_stream(br#"{"stream":false}"#));
        let chunks = dry_run_sse_chunks("mistral");
        assert_eq!(chunks.len(), 2);
        assert!(chunks[0].starts_with("data: "));
        assert!(chunks[0].contains("Fabric"));
        assert!(!chunks[0].contains("[DONE]"));
        assert!(chunks[1].contains("[DONE]"));
    }

    #[test]
    fn gateway_serves_openai_paths_and_refuses_a_control_plane_jwt() {
        assert!(inference_path_allowed("/v1/chat/completions"));
        assert!(inference_path_allowed("/v1/rerank"));
        assert!(inference_path_allowed("/v1/batches/job-1"));
        assert!(!inference_path_allowed("/v1/admin"));
        assert!(reject_control_plane_jwt("fvai_abc.def.ghi").is_ok());
        assert!(reject_control_plane_jwt("aaa.bbb.ccc").is_err());
        let inference = format!(
            "e30.{}.e30",
            base64::Engine::encode(
                &base64::engine::general_purpose::URL_SAFE_NO_PAD,
                br#"{"aud":"fabric-inference"}"#
            )
        );
        let control_plane = format!(
            "e30.{}.e30",
            base64::Engine::encode(
                &base64::engine::general_purpose::URL_SAFE_NO_PAD,
                br#"{"aud":"zyvor-fabricd"}"#
            )
        );
        assert!(inference_audience(&inference));
        assert!(reject_control_plane_jwt(&inference).is_ok());
        assert!(reject_control_plane_jwt(&control_plane).is_err());
    }
}
