// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: Apache-2.0

use crate::{
    audit::AuditPhase,
    credentials::{host_matches, CredentialVault, ResolveContext},
    model::{
        AgentManifest, ApprovalKind, ApprovalRecord, ApprovalStatus, EgressMode, EgressRequest,
        SessionRecord, DEFAULT_EGRESS_APPROVAL_SECONDS,
    },
    sentinel::{self, ReviewRequest, Verdict},
    AppState,
};
use axum::{
    extract::State,
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    Json,
};
use base64::Engine;
use reqwest::Url;
use serde_json::{json, Value};
use sha2::Digest;
use std::{collections::BTreeMap, sync::Arc};

/// The router the sandbox can reach: the JSON egress broker and nothing else.
/// Approvals, the journal and every other operator route live only on the
/// public router, behind the operator token, so an agent cannot decide its own
/// approvals through the port it is allowed to talk to.
pub fn broker_router(state: Arc<AppState>) -> axum::Router {
    axum::Router::new()
        .route("/v1/egress", axum::routing::post(proxy))
        .with_state(state)
}

/// The address a request came from, when the server was built with connection info. Router-level
/// tests are not, so this is `None` there rather than a failure.
pub struct MaybePeer(pub Option<std::net::SocketAddr>);

impl<S: Send + Sync> axum::extract::FromRequestParts<S> for MaybePeer {
    type Rejection = std::convert::Infallible;

    async fn from_request_parts(
        parts: &mut axum::http::request::Parts,
        _state: &S,
    ) -> Result<Self, Self::Rejection> {
        Ok(Self(
            parts
                .extensions
                .get::<axum::extract::ConnectInfo<std::net::SocketAddr>>()
                .map(|c| c.0),
        ))
    }
}

pub async fn proxy(
    State(state): State<Arc<AppState>>,
    MaybePeer(peer): MaybePeer,
    headers: HeaderMap,
    Json(request): Json<EgressRequest>,
) -> Response {
    let target = AuditTarget::from_request(&headers, &request);
    match proxy_inner(&state, &headers, request, peer).await {
        Ok(value) => {
            target
                .record(
                    &state,
                    AuditPhase::Performed,
                    json!({"status": value.get("status")}),
                )
                .await;
            (StatusCode::OK, Json(value)).into_response()
        }
        Err((status, message)) => {
            // 401 means the caller could not prove it owns the session, so its
            // claimed session id is not trustworthy enough to write to the journal.
            if status != StatusCode::UNAUTHORIZED {
                let phase = if status.is_server_error() {
                    AuditPhase::Failed
                } else {
                    AuditPhase::Denied
                };
                target
                    .record(&state, phase, json!({"reason": message}))
                    .await;
            }
            (status, Json(json!({"error": message}))).into_response()
        }
    }
}

/// What an egress call is about, captured before the request is consumed.
/// Query strings and credentials are deliberately excluded: they often carry secrets.
struct AuditTarget {
    session_id: Option<uuid::Uuid>,
    method: String,
    host: Option<String>,
    path: String,
}

impl AuditTarget {
    fn from_request(headers: &HeaderMap, request: &EgressRequest) -> Self {
        let url = Url::parse(&request.url).ok();
        Self {
            session_id: header(headers, "x-zyvor-session-id")
                .ok()
                .and_then(|id| uuid::Uuid::parse_str(id).ok()),
            method: request.method.to_ascii_uppercase(),
            host: url.as_ref().and_then(|u| u.host_str().map(str::to_string)),
            path: url.map(|u| u.path().to_string()).unwrap_or_default(),
        }
    }

    async fn record(&self, state: &AppState, phase: AuditPhase, mut detail: Value) {
        if let Some(object) = detail.as_object_mut() {
            object.insert("method".into(), json!(self.method));
            object.insert("path".into(), json!(self.path));
        }
        if let Err(error) = state
            .store
            .audit
            .append(
                self.session_id,
                phase,
                "egress.http",
                self.host.clone(),
                detail,
            )
            .await
        {
            tracing::error!(%error, "failed to write egress audit entry");
        }
    }
}

/// `peer` is the address the request arrived from, when known. It is only used to find the calling
/// program for rules that list `binaries`; without it such a rule refuses.
pub(crate) async fn proxy_inner(
    state: &AppState,
    headers: &HeaderMap,
    request: EgressRequest,
    peer: Option<std::net::SocketAddr>,
) -> Result<Value, (StatusCode, String)> {
    let session_id = header(headers, "x-zyvor-session-id")?;
    let capability = header(headers, "x-zyvor-egress-capability")?;
    let id = uuid::Uuid::parse_str(session_id)
        .map_err(|_| (StatusCode::UNAUTHORIZED, "invalid session id".to_string()))?;
    let session = state
        .store
        .get_session(id)
        .await
        .ok_or((StatusCode::UNAUTHORIZED, "unknown session".to_string()))?;
    if !constant_time_eq(session.capability_token.as_bytes(), capability.as_bytes()) {
        return Err((StatusCode::UNAUTHORIZED, "invalid egress capability".into()));
    }
    if session.status.is_terminal() {
        return Err((StatusCode::FORBIDDEN, "session is no longer active".into()));
    }

    let agent = state
        .store
        .get_agent_version(&session.agent, &session.agent_version)
        .await
        .map_err(|_| {
            (
                StatusCode::FORBIDDEN,
                "pinned agent deployment no longer exists".into(),
            )
        })?;

    let url = Url::parse(&request.url)
        .map_err(|e| (StatusCode::BAD_REQUEST, format!("invalid egress URL: {e}")))?;
    if !matches!(url.scheme(), "http" | "https") {
        return Err((
            StatusCode::BAD_REQUEST,
            "only http and https egress is supported".into(),
        ));
    }
    let host = url
        .host_str()
        .ok_or((StatusCode::BAD_REQUEST, "egress URL has no host".into()))?;
    // An `Idempotency-Key` on a request that needs approval makes it at-most-once (see `receipts`). It is also forwarded upstream.
    let idem = request
        .headers
        .iter()
        .find(|(name, _)| name.eq_ignore_ascii_case("idempotency-key"))
        .map(|(_, value)| value.trim().to_string());
    if idem
        .as_deref()
        .is_some_and(|k| !crate::receipts::valid_key(k))
    {
        return Err((
            StatusCode::BAD_REQUEST,
            format!(
                "Idempotency-Key must be 1 to {} printable characters without spaces",
                crate::receipts::MAX_KEY_LEN
            ),
        ));
    }
    let body = match request.body_base64 {
        Some(encoded) => {
            let body = base64::engine::general_purpose::STANDARD
                .decode(encoded.as_bytes())
                .map_err(|_| (StatusCode::BAD_REQUEST, "body_base64 is invalid".into()))?;
            if body.len() > 16 * 1024 * 1024 {
                return Err((
                    StatusCode::PAYLOAD_TOO_LARGE,
                    "egress body exceeds 16 MiB".into(),
                ));
            }
            Some(body)
        }
        None => None,
    };
    // Which program is calling? Only asked when a rule for this host names binaries, because it
    // costs a round trip into the guest. Any failure leaves `caller` empty, and such a rule then
    // refuses.
    let mut caller = None;
    let binary_rules = crate::l7::host_binary_rules(&agent.manifest.egress_rules, host);
    if !binary_rules.is_empty() {
        match crate::binary_id::attribute(state, &session, peer, state.config.egress_listen.port())
            .await
        {
            Ok(identity) => {
                // A rule that pins no hash gets trust on first use for the program it names.
                let unpinned = binary_rules
                    .iter()
                    .any(|b| b.sha256.is_none() && b.path == identity.path);
                if unpinned {
                    crate::binary_id::check_or_pin(&state.config.state_dir, &agent.name, &identity)
                        .await
                        .map_err(|message| (StatusCode::FORBIDDEN, message))?;
                }
                caller = Some(identity);
            }
            Err(reason) => {
                tracing::warn!(session = %session.id, %reason, "could not identify the calling program");
            }
        }
    }
    // Layer 7 policy runs first, so a request the agent may never send is refused
    // before an operator is asked about it.
    crate::l7::check_rules(
        &agent.manifest.egress_rules,
        host,
        &request.method,
        url.path(),
        body.as_deref().unwrap_or(&[]),
        caller.as_ref(),
    )
    .map_err(|message| (StatusCode::FORBIDDEN, message))?;
    let dlp_hits = if agent.manifest.dlp {
        let mut text = url.as_str().to_string();
        for value in request.headers.values() {
            text.push('\n');
            text.push_str(value);
        }
        if let Some(bytes) = &body {
            text.push('\n');
            text.push_str(&String::from_utf8_lossy(bytes));
        }
        crate::l7::scan(&text)
    } else {
        Vec::new()
    };
    // The operator's guard, if any, sees the request after the agent's own rules and before a
    // person is asked or a credential is added. It can only refuse.
    if let Some(guard) = &state.config.guard {
        let mut described = url.clone();
        described.set_query(None);
        described.set_fragment(None);
        let _ = described.set_username("");
        let _ = described.set_password(None);
        let decision = crate::guard::check(
            &state.egress_http,
            guard,
            &crate::guard::GuardRequest {
                session_id: session.id,
                agent: &agent.name,
                method: &request.method,
                url: described.as_str(),
                host,
                path: url.path(),
                header_names: request
                    .headers
                    .keys()
                    .map(|k| k.to_ascii_lowercase())
                    .collect(),
                dlp_hits: &dlp_hits,
                body: body.as_deref().unwrap_or(&[]),
            },
        )
        .await;
        if let crate::guard::Decision::Deny(reason) = decision {
            return Err((
                StatusCode::FORBIDDEN,
                format!("egress to {host} was refused by the egress guard: {reason}"),
            ));
        }
    }
    if !agent
        .manifest
        .egress_allow_hosts
        .iter()
        .any(|h| host_matches(h, host))
    {
        authorize_unlisted_host(state, &session, &agent.manifest, &url, &request.method).await?;
    }

    let port = url.port_or_known_default().unwrap_or(443);
    // Resolve once, validate, and pin the connection to that exact address
    // (rather than letting reqwest resolve the hostname a second,
    // independent time when it actually connects): otherwise an attacker
    // controlling DNS for `host` could return a public IP for this check
    // and a private/link-local one moments later for the real connection
    // (DNS rebinding), bypassing the allow_private_networks gate entirely.
    let pinned =
        resolve_and_validate_destination(host, port, agent.manifest.allow_private_networks).await?;

    let method = request
        .method
        .parse::<reqwest::Method>()
        .map_err(|_| (StatusCode::BAD_REQUEST, "invalid HTTP method".into()))?;
    let mut client_builder = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .timeout(std::time::Duration::from_secs(120))
        .resolve(host, pinned);
    for root in &state.extra_roots {
        client_builder = client_builder.add_root_certificate(root.clone());
    }
    let request_client = client_builder.build().map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("failed to construct pinned egress client: {e}"),
        )
    })?;
    let mut upstream = request_client.request(method.clone(), url.clone());
    for (name, value) in request.headers {
        if is_hop_or_secret_header(&name) || state.credentials.is_injection_header(&name) {
            continue;
        }
        let header_name =
            reqwest::header::HeaderName::from_bytes(name.as_bytes()).map_err(|_| {
                (
                    StatusCode::BAD_REQUEST,
                    format!("invalid request header name: {name}"),
                )
            })?;
        let header_value = reqwest::header::HeaderValue::from_str(&value).map_err(|_| {
            (
                StatusCode::BAD_REQUEST,
                format!("invalid request header value for {name}"),
            )
        })?;
        upstream = upstream.header(header_name, header_value);
    }

    let mut needs_approval: Option<(ApprovalKind, String)> = None;
    let mut preview: Option<crate::preview::Preview> = None;
    if let Some(name) = request.credential.as_deref() {
        if !agent.manifest.credentials.iter().any(|c| c == name) {
            return Err((
                StatusCode::FORBIDDEN,
                format!("credential '{name}' is not granted to this agent"),
            ));
        }
        let is_fabric = state
            .credentials
            .descriptor(name)
            .map(|d| d.kind.eq_ignore_ascii_case("fabric"))
            .unwrap_or(false);
        if url.scheme() != "https" && !is_fabric {
            return Err((
                StatusCode::FORBIDDEN,
                "credentials are injected only into HTTPS requests".into(),
            ));
        }
        if is_fabric && !matches!(url.scheme(), "http" | "https") {
            return Err((
                StatusCode::FORBIDDEN,
                "fabric credentials require http or https".into(),
            ));
        }
        let port = url
            .port_or_known_default()
            .unwrap_or(if is_fabric { 80 } else { 443 });
        let (descriptor, secret) = {
            if !state.vault_is_unlocked() {
                return Err((
                    StatusCode::FORBIDDEN,
                    "vault locked: POST /v1/vault/unwrap with a minted unwrap token \
                     (ZYVOR_AGENT_VAULT_UNWRAP_REQUIRED=1). Secrets still come from host env \
                     (software-test)."
                        .into(),
                ));
            }
            // A per-person credential (Google mail or calendar): mint an access token from THIS person's own connection first.
            if state.credentials.is_per_person(name) {
                let Some(user) = session.user_id.as_deref() else {
                    return Err((
                        StatusCode::FORBIDDEN,
                        format!("credential '{name}' belongs to a person's own connection and this session has no user"),
                    ));
                };
                let connection = state.credentials.connection_of(name).unwrap_or_default();
                let refresh_token = state
                    .store
                    .connections
                    .refresh_token(user, connection)
                    .await;
                let rotated = state
                    .credentials
                    .ensure_user_token(name, user, refresh_token.as_deref(), &state.egress_http)
                    .await
                    .map_err(|e| (StatusCode::FORBIDDEN, e.to_string()))?;
                // A provider that rotates refresh tokens (Microsoft) hands back a new one each time; keep it, or the connection dies when
                // the old one finally expires.
                if let (Some(new), Some(old)) = (rotated, refresh_token.as_deref()) {
                    match state
                        .store
                        .connections
                        .rotate(user, connection, old, &new)
                        .await
                    {
                        Ok(true) => {}
                        Ok(false) => {}
                        Err(e) => {
                            tracing::warn!(error = %e, credential = %name, "could not store the rotated refresh token")
                        }
                    }
                }
            }
            state
                .credentials
                .authorize_resolve(
                    name,
                    &ResolveContext {
                        host,
                        method: &method,
                        path: url.path(),
                        port,
                        user_id: session.user_id.as_deref(),
                    },
                )
                .await
                .map_err(|e| (StatusCode::FORBIDDEN, e.to_string()))?
        };
        if let Some(kind) = descriptor.approval_kind_for(&method) {
            needs_approval = Some((kind, name.to_string()));
            // The person decides what the host reads out of the real body, not what the agent says it is. A body the host cannot
            // render faithfully is not sent at all.
            if let Some(preview_kind) = descriptor.preview.as_deref() {
                preview = Some(
                    crate::preview::render(preview_kind, body.as_deref().unwrap_or_default(), url.query()).map_err(|e| {
                        (
                            StatusCode::UNPROCESSABLE_ENTITY,
                            format!("this request cannot be shown for approval, so it was not sent: {e:#}"),
                        )
                    })?,
                );
            }
        }
        if !secret.is_empty() {
            let header_name = reqwest::header::HeaderName::from_bytes(descriptor.header.as_bytes())
                .map_err(|_| {
                    (
                        StatusCode::BAD_GATEWAY,
                        "configured credential header is invalid".into(),
                    )
                })?;
            let header_value = reqwest::header::HeaderValue::from_str(&secret).map_err(|_| {
                (
                    StatusCode::BAD_GATEWAY,
                    "configured credential value cannot be represented as an HTTP header".into(),
                )
            })?;
            upstream = upstream.header(header_name, header_value);
        }
    }

    // Reasons a person must decide this request, gathered into one approval: a
    // credential that needs it, a secret-shaped string in the request, or a write
    // from a tainted session. This runs after every policy check and before
    // anything leaves the host.
    let mut reasons: Vec<String> = Vec::new();
    if !dlp_hits.is_empty() {
        reasons.push(format!("request contains: {}", dlp_hits.join(", ")));
    }
    if agent.manifest.taint.is_some()
        && !matches!(method, reqwest::Method::GET | reqwest::Method::HEAD)
    {
        if let Some(current) = state.store.get_session(session.id).await {
            if !current.tainted_by.is_empty() {
                reasons.push(format!(
                    "session is tainted by {}",
                    current.tainted_by.join(", ")
                ));
            }
        }
    }
    let gated = needs_approval.is_some() || !reasons.is_empty();
    let mut approval_id = None;
    let mut _in_flight = None;
    let body_sha256 = body
        .as_deref()
        .map(|b| hex::encode(sha2::Sha256::digest(b)))
        .unwrap_or_default();
    let body_len = body.as_ref().map_or(0, Vec::len);
    let request_fingerprint =
        crate::receipts::fingerprint(method.as_str(), url.as_str(), &body_sha256);
    if gated {
        let (kind, credential) = match needs_approval {
            Some((kind, name)) => (kind, Some(name)),
            None => (ApprovalKind::Send, None),
        };
        // At-most-once: a keyed request that was already performed is answered from its receipt, without asking again or sending again.
        if let Some(key) = idem.as_deref() {
            let scope = crate::receipts::scope_key(
                session.user_id.as_deref(),
                request.credential.as_deref(),
                key,
            );
            match state
                .store
                .receipts
                .begin(&scope, &request_fingerprint)
                .await
            {
                crate::receipts::Begin::Replay(receipt) => {
                    let _ = state
                        .store
                        .audit
                        .append(
                            Some(session.id),
                            AuditPhase::Performed,
                            "merion.action.replayed",
                            Some(session.agent.clone()),
                            json!({ "receipt_id": receipt.id, "credential": receipt.credential, "method": receipt.method, "host": host.to_ascii_lowercase() }),
                        )
                        .await;
                    return Ok(crate::receipts::replay_response(&receipt));
                }
                crate::receipts::Begin::Conflict(why) => return Err((StatusCode::CONFLICT, why)),
                crate::receipts::Begin::Proceed(guard) => _in_flight = Some(guard),
            }
        }
        approval_id = Some(
            hold_for_approval(
                state,
                &session,
                &agent.manifest,
                kind,
                &url,
                method.as_str(),
                credential.as_deref(),
                &reasons,
                body.as_deref(),
                preview.as_ref(),
            )
            .await?,
        );
    }
    if let Some(body) = body {
        upstream = upstream.body(body);
    }

    let response = upstream.send().await.map_err(|e| {
        (
            StatusCode::BAD_GATEWAY,
            format!("egress upstream failed: {e}"),
        )
    })?;
    let status = response.status().as_u16();
    taint_on_read(state, &session, &agent.manifest, host).await;
    let mut out_headers = BTreeMap::new();
    // `headers` collapses repeats (fine for a JSON caller); `header_list` keeps
    // every one, which the TLS-intercepting proxy needs for multiple Set-Cookie.
    let mut header_list: Vec<(String, String)> = Vec::new();
    for (name, value) in response.headers() {
        if let Ok(value) = value.to_str() {
            if !is_hop_or_secret_header(name.as_str()) {
                out_headers.insert(name.to_string(), value.to_string());
                header_list.push((name.to_string(), value.to_string()));
            }
        }
    }
    let body = response.bytes().await.map_err(|e| {
        (
            StatusCode::BAD_GATEWAY,
            format!("reading upstream body: {e}"),
        )
    })?;
    if body.len() > 16 * 1024 * 1024 {
        return Err((
            StatusCode::BAD_GATEWAY,
            "upstream response exceeds 16 MiB".into(),
        ));
    }

    tracing::info!(session = %id, agent = %session.agent, %host, credential = ?request.credential, status, "agent egress");
    // A receipt for what a person approved: recorded when the upstream answered with anything below 500 (a 5xx or a network error may be retried).
    let mut receipt_id = None;
    if gated && status < 500 {
        let mut target = url.clone();
        target.set_query(None);
        target.set_fragment(None);
        let _ = target.set_username("");
        let _ = target.set_password(None);
        let receipt = crate::receipts::Receipt {
            id: uuid::Uuid::new_v4(),
            at: chrono::Utc::now(),
            user_id: session.user_id.clone(),
            session_id: session.id,
            agent: session.agent.clone(),
            credential: request.credential.clone(),
            method: method.as_str().to_string(),
            url: target.to_string(),
            body_bytes: body_len,
            body_sha256: body_sha256.clone(),
            approval_id,
            idempotency_key: idem.clone(),
            status,
            fingerprint: request_fingerprint.clone(),
        };
        match state.store.receipts.record(receipt.clone()).await {
            Ok(()) => {
                receipt_id = Some(receipt.id);
                let _ = state
                    .store
                    .audit
                    .append(
                        Some(session.id),
                        AuditPhase::Performed,
                        "merion.action.performed",
                        Some(session.agent.clone()),
                        json!({ "receipt_id": receipt.id, "approval_id": approval_id, "credential": receipt.credential, "method": receipt.method, "host": host.to_ascii_lowercase(), "status": status, "keyed": idem.is_some() }),
                    )
                    .await;
            }
            Err(error) => tracing::error!(%error, "could not record an action receipt"),
        }
    }
    Ok(json!({
        "status": status,
        "headers": out_headers,
        "header_list": header_list,
        "body_base64": base64::engine::general_purpose::STANDARD.encode(body),
        "receipt_id": receipt_id,
    }))
}

/// Resolves `host`, rejects it if any returned address is blocked (unless
/// `allow_private_networks` opts an agent out of that check), and returns
/// the first address to pin the actual connection to. The caller must
/// connect only to this exact address for this request -- resolving `host`
/// again later would reopen the DNS-rebinding gap this function exists to
/// close.
pub(crate) async fn resolve_and_validate_destination(
    host: &str,
    port: u16,
    allow_private_networks: bool,
) -> Result<std::net::SocketAddr, (StatusCode, String)> {
    let addresses: Vec<std::net::SocketAddr> = tokio::net::lookup_host((host, port))
        .await
        .map_err(|e| {
            (
                StatusCode::BAD_GATEWAY,
                format!("DNS lookup failed for {host}: {e}"),
            )
        })?
        .collect();
    let Some(first) = addresses.first().copied() else {
        return Err((
            StatusCode::BAD_GATEWAY,
            format!("DNS lookup returned no addresses for {host}"),
        ));
    };
    if !allow_private_networks {
        for address in &addresses {
            let ip = address.ip();
            let blocked = match ip {
                std::net::IpAddr::V4(v4) => {
                    v4.is_private()
                        || v4.is_loopback()
                        || v4.is_link_local()
                        || v4.is_multicast()
                        || v4.is_unspecified()
                }
                std::net::IpAddr::V6(v6) => {
                    v6.is_loopback()
                        || v6.is_unique_local()
                        || v6.is_unicast_link_local()
                        || v6.is_multicast()
                        || v6.is_unspecified()
                }
            };
            if blocked {
                return Err((
                    StatusCode::FORBIDDEN,
                    format!(
                        "destination {host} resolves to blocked private/link-local address {ip}"
                    ),
                ));
            }
        }
    }
    Ok(first)
}

fn header<'a>(headers: &'a HeaderMap, name: &str) -> Result<&'a str, (StatusCode, String)> {
    headers
        .get(name)
        .and_then(|v| v.to_str().ok())
        .filter(|v| !v.is_empty())
        .ok_or((StatusCode::UNAUTHORIZED, format!("missing {name}")))
}

fn is_hop_or_secret_header(name: &str) -> bool {
    matches!(
        name.to_ascii_lowercase().as_str(),
        "host"
            | "authorization"
            | "proxy-authorization"
            | "content-length"
            | "connection"
            | "transfer-encoding"
    )
}

pub(crate) fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    let mut diff = 0u8;
    for (&x, &y) in a.iter().zip(b) {
        diff |= x ^ y;
    }
    diff == 0
}

#[allow(dead_code)]
fn _assert_send_sync(_: &CredentialVault) {}

/// Taint the session when it has read content from a host the agent's taint
/// policy does not trust. Journaled once per host.
pub(crate) async fn taint_on_read(
    state: &AppState,
    session: &SessionRecord,
    manifest: &AgentManifest,
    host: &str,
) {
    let Some(policy) = &manifest.taint else {
        return;
    };
    if policy.trusted_hosts.iter().any(|h| host_matches(h, host)) {
        return;
    }
    match state.store.taint_session(session.id, host).await {
        Ok(true) => {
            if let Err(error) = state
                .store
                .audit
                .append(
                    Some(session.id),
                    AuditPhase::Performed,
                    "session.tainted",
                    Some(host.to_string()),
                    json!({"reason": "read from an untrusted host"}),
                )
                .await
            {
                tracing::error!(%error, "failed to write audit entry");
            }
        }
        Ok(false) => {}
        Err(error) => tracing::error!(%error, "failed to taint session"),
    }
}

/// True when the agent tracks taint and this session is currently tainted.
async fn is_tainted(state: &AppState, session: &SessionRecord, manifest: &AgentManifest) -> bool {
    manifest.taint.is_some()
        && state
            .store
            .get_session(session.id)
            .await
            .is_some_and(|s| !s.tainted_by.is_empty())
}

enum SentinelOutcome {
    /// One request may proceed; nothing is remembered for later requests.
    Allow,
    Deny(String),
    /// Ask an operator, passing along what the reviewer said (if anything).
    Escalate(Option<String>),
}

/// Screen a request with the reviewer model. Every failure path escalates to
/// the operator rather than allowing.
async fn sentinel_screen(
    state: &AppState,
    session: &SessionRecord,
    manifest: &AgentManifest,
    url: &Url,
    method: &str,
    host: &str,
) -> SentinelOutcome {
    let Some(config) = state.config.sentinel.as_ref() else {
        return SentinelOutcome::Escalate(Some("sentinel is not configured".into()));
    };
    let request = ReviewRequest {
        agent: &session.agent,
        allowed_hosts: &manifest.egress_allow_hosts,
        method,
        host,
        path: url.path(),
    };
    let audit = |phase: AuditPhase, detail: Value| async move {
        if let Err(error) = state
            .store
            .audit
            .append(
                Some(session.id),
                phase,
                "sentinel.egress",
                Some(host.to_string()),
                detail,
            )
            .await
        {
            tracing::error!(%error, "failed to write audit entry");
        }
    };
    match sentinel::review(&state.egress_http, config, &request).await {
        Ok(review) => {
            let detail = json!({"verdict": format!("{:?}", review.verdict).to_lowercase(), "reason": review.reason, "method": method});
            match review.verdict {
                // A tainted session may have read attacker-written content, so the
                // reviewer's say-so is not enough: a person decides.
                Verdict::Allow if is_tainted(state, session, manifest).await => {
                    SentinelOutcome::Escalate(Some(
                        "session is tainted; a person must decide".into(),
                    ))
                }
                Verdict::Allow => {
                    audit(AuditPhase::Approved, detail).await;
                    SentinelOutcome::Allow
                }
                Verdict::Deny => {
                    audit(AuditPhase::Denied, detail).await;
                    SentinelOutcome::Deny(format!(
                        "egress to {host} was denied by sentinel: {}",
                        review.reason
                    ))
                }
                Verdict::Escalate => SentinelOutcome::Escalate(Some(review.reason)),
            }
        }
        Err(error) => {
            tracing::warn!(%error, host, "sentinel review failed; asking an operator");
            SentinelOutcome::Escalate(Some(format!("sentinel unavailable: {error}")))
        }
    }
}

/// Poll interval while a request waits for an operator decision.
/// The reason a deny-mode agent gets for a host outside its allowlist. `advisor` matches on it
/// to find denials worth suggesting a rule for, so keep the two in step.
pub(crate) const NOT_ALLOWLISTED: &str = "is not in this agent's egress allowlist";

const APPROVAL_POLL: std::time::Duration = std::time::Duration::from_millis(250);

/// Decide what to do with a request to a host that is not on the allowlist.
///
/// In `deny` mode this is a plain refusal. In `ask` mode the request is held
/// while an operator approves or denies it. An approval only lifts the
/// allowlist check: DNS pinning and the private-network gate still run after
/// this returns, so approving a host never permits SSRF into internal ranges.
pub(crate) async fn authorize_unlisted_host(
    state: &AppState,
    session: &SessionRecord,
    manifest: &AgentManifest,
    url: &Url,
    method: &str,
) -> Result<(), (StatusCode, String)> {
    let host = url
        .host_str()
        .map(str::to_ascii_lowercase)
        .unwrap_or_default();
    let refuse = |message: String| Err((StatusCode::FORBIDDEN, message));
    if manifest.egress_mode == EgressMode::Deny {
        return refuse(format!("host {host} {NOT_ALLOWLISTED}"));
    }
    if state
        .store
        .has_session_egress_grant(session.id, &host)
        .await
    {
        return Ok(());
    }

    let method = method.to_ascii_uppercase();
    let sentinel_note = if manifest.egress_mode == EgressMode::Sentinel {
        match sentinel_screen(state, session, manifest, url, &method, &host).await {
            SentinelOutcome::Allow => return Ok(()),
            SentinelOutcome::Deny(message) => return refuse(message),
            SentinelOutcome::Escalate(note) => note,
        }
    } else {
        None
    };
    let mut target = url.clone();
    target.set_query(None);
    target.set_fragment(None);
    let _ = target.set_username("");
    let _ = target.set_password(None);
    let candidate = ApprovalRecord {
        id: uuid::Uuid::new_v4(),
        session_id: session.id,
        kind: ApprovalKind::Egress,
        subject: Some(host.clone()),
        planned_action: Some(json!({
            "method": method,
            "url": target.as_str(),
            "scopes": ["once", "session"],
            "sentinel": sentinel_note,
        })),
        prompt: format!("Agent wants to {method} {target}, which is not on its egress allowlist"),
        status: ApprovalStatus::Pending,
        comment: None,
        created_at: chrono::Utc::now(),
        decided_at: None,
        source_seq: None,
        grant_scope: None,
        preview: None,
        broker_held: true,
    };
    let (approval, created) = state
        .store
        .open_egress_approval(session.id, &host, candidate)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("failed to open egress approval: {e}"),
            )
        })?;
    if created {
        crate::app::audit_approval_planned(state, &approval).await;
    }

    wait_for_decision(
        state,
        session,
        &approval,
        approval_timeout(manifest),
        &host,
        &format!("egress to {host} was denied by an operator"),
        &format!("egress approval for {host} expired"),
    )
    .await
}

fn approval_timeout(manifest: &AgentManifest) -> std::time::Duration {
    std::time::Duration::from_secs(
        manifest
            .egress_approval_timeout_seconds
            .unwrap_or(DEFAULT_EGRESS_APPROVAL_SECONDS),
    )
}

/// Hold the current request until an operator decides `approval`. Approved
/// returns `Ok`; a denial, timeout, or the session ending refuses with 403.
/// A decision and a timeout can land at the same instant: whichever transition
/// reaches the store first wins, and the loser re-reads the result.
pub(crate) async fn wait_for_decision(
    state: &AppState,
    session: &SessionRecord,
    approval: &ApprovalRecord,
    timeout: std::time::Duration,
    subject: &str,
    denied_message: &str,
    expired_message: &str,
) -> Result<(), (StatusCode, String)> {
    let refuse = |message: &str| Err((StatusCode::FORBIDDEN, message.to_string()));
    let deadline = tokio::time::Instant::now() + timeout;
    loop {
        let current = state.store.get_approval(approval.id).await;
        match current.as_ref().map(|r| r.status) {
            Some(ApprovalStatus::Approved) => return Ok(()),
            Some(ApprovalStatus::Denied) => return refuse(denied_message),
            Some(ApprovalStatus::Expired) | None => return refuse(expired_message),
            Some(ApprovalStatus::Pending) => {}
        }
        let session_over = state
            .store
            .get_session(session.id)
            .await
            .is_none_or(|s| s.status.is_terminal());
        let timed_out = tokio::time::Instant::now() >= deadline;
        if session_over || timed_out {
            let expired = state
                .store
                .transition_approval(
                    approval.id,
                    ApprovalStatus::Expired,
                    Some(if session_over {
                        "session ended".into()
                    } else {
                        "timed out".into()
                    }),
                    None,
                )
                .await
                .ok()
                .flatten();
            if expired.is_some() {
                if let Err(error) = state
                    .store
                    .audit
                    .append(
                        Some(session.id),
                        AuditPhase::Denied,
                        &format!("approval.{}", approval.kind.as_str()),
                        Some(subject.to_string()),
                        json!({"approval_id": approval.id, "reason": "expired"}),
                    )
                    .await
                {
                    tracing::error!(%error, "failed to write audit entry");
                }
                return refuse(expired_message);
            }
            continue;
        }
        tokio::time::sleep(APPROVAL_POLL).await;
    }
}

/// Open a fresh approval for one credentialed request and hold it until an
/// operator decides. Unlike unlisted-host approvals these are never shared or
/// remembered: each request needs its own decision, because what is being
/// approved (a send, a purchase) is specific to that request. The record shows
/// the method, URL without its query string, the credential name, and a digest
/// and length of the body, never the body or any header.
#[allow(clippy::too_many_arguments)]
async fn hold_for_approval(
    state: &AppState,
    session: &SessionRecord,
    manifest: &AgentManifest,
    kind: ApprovalKind,
    url: &Url,
    method: &str,
    credential: Option<&str>,
    reasons: &[String],
    body: Option<&[u8]>,
    preview: Option<&crate::preview::Preview>,
) -> Result<uuid::Uuid, (StatusCode, String)> {
    let host = url.host_str().unwrap_or_default().to_ascii_lowercase();
    let mut target = url.clone();
    target.set_query(None);
    target.set_fragment(None);
    let _ = target.set_username("");
    let _ = target.set_password(None);
    let (body_len, body_sha256) = match body {
        Some(bytes) => (bytes.len(), hex::encode(sha2::Sha256::digest(bytes))),
        None => (0, String::new()),
    };
    let mut planned = json!({
        "method": method,
        "url": target.as_str(),
        "credential": credential,
        "reasons": reasons,
        "body_bytes": body_len,
        "body_sha256": body_sha256,
        "scopes": ["once"],
    });
    if let Some(p) = preview {
        // What the phone signs covers the rendering the person read, not only the raw bytes.
        planned["preview_sha256"] = json!(p.sha256());
    }
    let approval = ApprovalRecord {
        id: uuid::Uuid::new_v4(),
        session_id: session.id,
        kind,
        subject: Some(host.clone()),
        planned_action: Some(planned),
        prompt: {
            let mut prompt = format!("Agent wants to {method} {target}");
            if let Some(name) = credential {
                prompt.push_str(&format!(" using credential '{name}'"));
            }
            if !reasons.is_empty() {
                prompt.push_str(&format!(" ({})", reasons.join("; ")));
            }
            prompt
        },
        status: ApprovalStatus::Pending,
        comment: None,
        created_at: chrono::Utc::now(),
        decided_at: None,
        source_seq: None,
        grant_scope: None,
        preview: preview.map(crate::preview::Preview::to_value),
        broker_held: true,
    };
    state
        .store
        .save_approval(approval.clone())
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("failed to open approval: {e}"),
            )
        })?;
    crate::app::audit_approval_planned(state, &approval).await;
    let what = kind.as_str();
    wait_for_decision(
        state,
        session,
        &approval,
        approval_timeout(manifest),
        &host,
        &format!("{what} to {host} was denied by an operator"),
        &format!("{what} approval for {host} expired"),
    )
    .await?;
    Ok(approval.id)
}

#[cfg(test)]
pub(crate) mod ask_tests {
    use super::*;
    use crate::{
        audit::AuditPhase,
        config::Config,
        model::{GrantScope, SessionStartMode, SessionStartPolicy, SessionStatus},
    };
    use std::sync::Arc;

    async fn state_and_session() -> (Arc<AppState>, SessionRecord) {
        state_and_session_with(None).await
    }

    pub(crate) async fn state_and_session_with(
        sentinel: Option<crate::sentinel::SentinelConfig>,
    ) -> (Arc<AppState>, SessionRecord) {
        state_and_session_cfg(|config| config.sentinel = sentinel).await
    }

    pub(crate) async fn state_and_session_cfg(
        tweak: impl FnOnce(&mut Config),
    ) -> (Arc<AppState>, SessionRecord) {
        let root = std::env::temp_dir().join(format!("zyvor-egress-ask-{}", uuid::Uuid::new_v4()));
        let mut config = Config {
            listen: "127.0.0.1:0".parse().unwrap(),
            egress_listen: "127.0.0.1:0".parse().unwrap(),
            state_dir: root.join("state"),
            snapshot_dir: root.join("snap"),
            fluxvm_url: "http://127.0.0.1:1".into(),
            fluxvm_token: None,
            api_token: None,
            credentials_file: None,
            model_chains_file: None,
            skill_scopes_file: None,
            sentinel: None,
            guard: None,
            approval_webhook: None,
            proxy_listen: None,
            proxy_connect_ports: vec![443],
            mitm_ca_dir: None,
            extra_ca_files: vec![],
            confine_all: false,
            security_profile: None,
            recover_key_a: None,
            recover_key_b: None,
            max_vcpus: None,
            max_memory_mib: None,
            egress_advertise_host: None,
            sync_interval_ms: 300,
            guest_start_timeout_secs: 30,
            thread_retention_days: None,
            receipt_retention_days: None,
            memory_proposal_retention_days: None,
            suggestion_retention_days: None,
            goal_tick_ms: 5000,
            goal_retry_base_secs: 15,
            planner_agent: None,
            event_retention_days: None,
            idle_scan_interval_ms: 1000,
            warm_pool_reconcile_interval_ms: 2000,
            warm_pool_max_create_per_tick: 2,
            warm_pool_claim_stale_secs: 300,
            expiry_scan_interval_ms: 1000,
        };
        tweak(&mut config);
        let state = AppState::from_config(config).await.unwrap();
        let now = chrono::Utc::now();
        let session = SessionRecord {
            id: uuid::Uuid::new_v4(),
            agent: "a".into(),
            agent_version: "v".into(),
            sandbox_id: uuid::Uuid::new_v4(),
            status: SessionStatus::Running,
            input: json!({}),
            created_at: now,
            updated_at: now,
            last_event_seq: 0,
            guest_event_cursor: 0,
            request_id: None,
            start_policy: SessionStartPolicy::PreferWarm,
            start_mode: SessionStartMode::Cold,
            startup_ms: None,
            expires_at: None,
            sandbox_released: false,
            capability_token: "cap".into(),
            error: None,
            parent_session_id: None,
            user_id: None,
            tainted_by: vec![],
            confidential: None,
            agent_paused_reason: None,
            browse: Default::default(),
        };
        state.store.save_session(session.clone()).await.unwrap();
        (state, session)
    }

    pub(crate) fn manifest(mode: EgressMode, timeout: Option<u64>) -> AgentManifest {
        AgentManifest {
            egress_rules: vec![],
            dlp: false,
            taint: None,
            confinement: Default::default(),
            resources: None,
            confidential: Default::default(),
            inner_container: Default::default(),
            persistent: false,
            browser_port: None,
            browser: None,
            template: "t".into(),
            credentials: vec![],
            egress_allow_hosts: vec![],
            allow_private_networks: false,
            runtime_port: 8080,
            memory: false,
            suggestions: false,
            card: false,
            ttl_seconds: None,
            max_concurrent_sessions: None,
            idle_hibernate_seconds: None,
            warm_pool_size: 0,
            runtime: Default::default(),
            egress_mode: mode,
            home_volume: None,
            skills: vec![],
            skill_scope: None,
            egress_approval_timeout_seconds: timeout,
            model_socket: None,
            cell_backend: None,
            gpus: None,
        }
    }

    fn url() -> Url {
        Url::parse("https://Example.com/path?token=secret").unwrap()
    }

    /// Wait until an egress approval for the session is pending, then return it.
    pub(crate) async fn wait_pending(state: &AppState, session: uuid::Uuid) -> ApprovalRecord {
        for _ in 0..200 {
            if let Some(found) = state
                .store
                .list_approvals()
                .await
                .into_iter()
                .find(|a| a.session_id == session && a.status == ApprovalStatus::Pending)
            {
                return found;
            }
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
        panic!("no pending approval appeared");
    }

    #[tokio::test]
    async fn deny_mode_refuses_without_opening_an_approval() {
        let (state, session) = state_and_session().await;
        let err = authorize_unlisted_host(
            &state,
            &session,
            &manifest(EgressMode::Deny, None),
            &url(),
            "GET",
        )
        .await
        .unwrap_err();
        assert_eq!(err.0, StatusCode::FORBIDDEN);
        assert!(err.1.contains("allowlist"));
        assert!(state.store.list_approvals().await.is_empty());
    }

    #[tokio::test]
    async fn ask_mode_approve_once_does_not_cover_later_requests() {
        let (state, session) = state_and_session().await;
        let m = manifest(EgressMode::Ask, Some(1));
        let task = {
            let (state, session, m) = (state.clone(), session.clone(), m.clone());
            tokio::spawn(async move {
                authorize_unlisted_host(&state, &session, &m, &url(), "GET").await
            })
        };
        let pending = wait_pending(&state, session.id).await;
        assert_eq!(pending.kind, ApprovalKind::Egress);
        assert_eq!(pending.subject.as_deref(), Some("example.com"));
        let planned = pending.planned_action.clone().unwrap().to_string();
        assert!(
            !planned.contains("secret"),
            "query string must not be stored: {planned}"
        );
        state
            .store
            .transition_approval(
                pending.id,
                ApprovalStatus::Approved,
                None,
                Some(GrantScope::Once),
            )
            .await
            .unwrap()
            .unwrap();
        assert!(task.await.unwrap().is_ok());

        // A later request is not covered and, with nobody deciding, expires.
        let err = authorize_unlisted_host(&state, &session, &m, &url(), "GET")
            .await
            .unwrap_err();
        assert_eq!(err.0, StatusCode::FORBIDDEN);
        assert!(err.1.contains("expired"));
    }

    #[tokio::test]
    async fn ask_mode_session_grant_covers_later_requests_immediately() {
        let (state, session) = state_and_session().await;
        let m = manifest(EgressMode::Ask, Some(30));
        let task = {
            let (state, session, m) = (state.clone(), session.clone(), m.clone());
            tokio::spawn(async move {
                authorize_unlisted_host(&state, &session, &m, &url(), "GET").await
            })
        };
        let pending = wait_pending(&state, session.id).await;
        state
            .store
            .transition_approval(
                pending.id,
                ApprovalStatus::Approved,
                None,
                Some(GrantScope::Session),
            )
            .await
            .unwrap();
        assert!(task.await.unwrap().is_ok());
        let started = std::time::Instant::now();
        assert!(
            authorize_unlisted_host(&state, &session, &m, &url(), "POST")
                .await
                .is_ok()
        );
        assert!(started.elapsed() < std::time::Duration::from_secs(1));
        assert_eq!(state.store.list_approvals().await.len(), 1);
    }

    #[tokio::test]
    async fn ask_mode_denied_by_operator() {
        let (state, session) = state_and_session().await;
        let m = manifest(EgressMode::Ask, Some(30));
        let task = {
            let (state, session, m) = (state.clone(), session.clone(), m.clone());
            tokio::spawn(async move {
                authorize_unlisted_host(&state, &session, &m, &url(), "GET").await
            })
        };
        let pending = wait_pending(&state, session.id).await;
        state
            .store
            .transition_approval(pending.id, ApprovalStatus::Denied, Some("no".into()), None)
            .await
            .unwrap();
        let err = task.await.unwrap().unwrap_err();
        assert_eq!(err.0, StatusCode::FORBIDDEN);
        assert!(err.1.contains("denied by an operator"));
    }

    #[tokio::test]
    async fn concurrent_requests_to_one_host_share_one_approval() {
        let (state, session) = state_and_session().await;
        let m = manifest(EgressMode::Ask, Some(30));
        let mut tasks = Vec::new();
        for _ in 0..3 {
            let (state, session, m) = (state.clone(), session.clone(), m.clone());
            tasks.push(tokio::spawn(async move {
                authorize_unlisted_host(&state, &session, &m, &url(), "GET").await
            }));
        }
        let pending = wait_pending(&state, session.id).await;
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        assert_eq!(state.store.list_approvals().await.len(), 1);
        state
            .store
            .transition_approval(
                pending.id,
                ApprovalStatus::Approved,
                None,
                Some(GrantScope::Once),
            )
            .await
            .unwrap();
        for task in tasks {
            assert!(task.await.unwrap().is_ok());
        }
    }

    #[test]
    fn default_manifest_serialization_omits_egress_mode_fields() {
        // Agent version ids hash the serialized manifest, so defaults must not
        // change the bytes of manifests deployed before these fields existed.
        let value = serde_json::to_value(manifest(EgressMode::Deny, None)).unwrap();
        assert!(value.get("egress_mode").is_none());
        assert!(value.get("egress_approval_timeout_seconds").is_none());
        assert!(value.get("skills").is_none());
        assert!(value.get("skill_scope").is_none());
        assert!(value.get("home_volume").is_none());
        let asked = serde_json::to_value(manifest(EgressMode::Ask, Some(30))).unwrap();
        assert_eq!(asked["egress_mode"], "ask");
        assert_eq!(asked["egress_approval_timeout_seconds"], 30);
    }

    #[tokio::test]
    async fn ending_the_session_expires_the_wait_and_is_journaled() {
        let (state, session) = state_and_session().await;
        let m = manifest(EgressMode::Ask, Some(30));
        let task = {
            let (state, session, m) = (state.clone(), session.clone(), m.clone());
            tokio::spawn(async move {
                authorize_unlisted_host(&state, &session, &m, &url(), "GET").await
            })
        };
        wait_pending(&state, session.id).await;
        let mut ended = session.clone();
        ended.status = SessionStatus::Cancelled;
        state.store.save_session(ended).await.unwrap();
        let err = task.await.unwrap().unwrap_err();
        assert!(err.1.contains("expired"));
        let entries = state.store.audit.list(Some(session.id), 100).await.unwrap();
        assert!(entries.iter().any(|e| e.phase == AuditPhase::Planned));
        assert!(entries
            .iter()
            .any(|e| e.phase == AuditPhase::Denied && e.detail["reason"] == "expired"));
        assert!(state.store.audit.verify().await.unwrap().chain_ok);
    }

    async fn reviewer(answer: &'static str) -> crate::sentinel::SentinelConfig {
        let app = axum::Router::new().route(
            "/v1/chat/completions",
            axum::routing::post(move || async move {
                Json(json!({"choices": [{"message": {"content": answer}}]}))
            }),
        );
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        crate::sentinel::SentinelConfig {
            url: format!("http://{addr}/v1"),
            model: "m".into(),
            api_key: None,
            timeout: std::time::Duration::from_secs(5),
            can_allow: false,
        }
    }

    #[tokio::test]
    async fn sentinel_deny_refuses_without_a_human_and_is_journaled() {
        let cfg = reviewer(r#"{"verdict":"deny","reason":"paste site"}"#).await;
        let (state, session) = state_and_session_with(Some(cfg)).await;
        let m = manifest(EgressMode::Sentinel, Some(30));
        let err = authorize_unlisted_host(&state, &session, &m, &url(), "GET")
            .await
            .unwrap_err();
        assert_eq!(err.0, StatusCode::FORBIDDEN);
        assert!(err.1.contains("sentinel") && err.1.contains("paste site"));
        assert!(state.store.list_approvals().await.is_empty());
        let entries = state.store.audit.list(Some(session.id), 50).await.unwrap();
        assert!(entries
            .iter()
            .any(|e| e.action == "sentinel.egress" && e.phase == AuditPhase::Denied));
    }

    #[tokio::test]
    async fn sentinel_escalate_asks_an_operator_and_passes_the_note() {
        let cfg = reviewer(r#"{"verdict":"escalate","reason":"unfamiliar host"}"#).await;
        let (state, session) = state_and_session_with(Some(cfg)).await;
        let m = manifest(EgressMode::Sentinel, Some(30));
        let waiter = {
            let (state, session, m) = (state.clone(), session.clone(), m.clone());
            tokio::spawn(async move {
                authorize_unlisted_host(&state, &session, &m, &url(), "GET").await
            })
        };
        let pending = wait_pending(&state, session.id).await;
        assert_eq!(
            pending.planned_action.as_ref().unwrap()["sentinel"],
            "unfamiliar host"
        );
        state
            .store
            .transition_approval(
                pending.id,
                ApprovalStatus::Approved,
                None,
                Some(GrantScope::Once),
            )
            .await
            .unwrap();
        assert!(waiter.await.unwrap().is_ok());
    }

    #[tokio::test]
    async fn sentinel_allow_needs_operator_opt_in_and_never_persists() {
        let mut cfg = reviewer(r#"{"verdict":"allow","reason":"docs site"}"#).await;
        cfg.can_allow = true;
        let (state, session) = state_and_session_with(Some(cfg)).await;
        let m = manifest(EgressMode::Sentinel, Some(30));
        authorize_unlisted_host(&state, &session, &m, &url(), "GET")
            .await
            .unwrap();
        assert!(state.store.list_approvals().await.is_empty());
        assert!(
            !state
                .store
                .has_session_egress_grant(session.id, "example.com")
                .await
        );
    }

    #[tokio::test]
    async fn sentinel_unconfigured_or_unreachable_falls_back_to_asking() {
        for cfg in [
            None,
            Some(crate::sentinel::SentinelConfig {
                url: "http://127.0.0.1:1/v1".into(),
                model: "m".into(),
                api_key: None,
                timeout: std::time::Duration::from_secs(1),
                can_allow: true,
            }),
        ] {
            let (state, session) = state_and_session_with(cfg).await;
            let m = manifest(EgressMode::Sentinel, Some(5));
            let waiter = {
                let (state, session, m) = (state.clone(), session.clone(), m.clone());
                tokio::spawn(async move {
                    authorize_unlisted_host(&state, &session, &m, &url(), "GET").await
                })
            };
            let pending = wait_pending(&state, session.id).await;
            state
                .store
                .transition_approval(pending.id, ApprovalStatus::Denied, None, None)
                .await
                .unwrap();
            assert!(waiter.await.unwrap().is_err());
        }
    }

    #[test]
    fn sentinel_mode_serializes_as_sentinel() {
        let value = serde_json::to_value(manifest(EgressMode::Sentinel, None)).unwrap();
        assert_eq!(value["egress_mode"], "sentinel");
    }

    // ---- credentialed requests that need a human (send / purchase) ----

    async fn upstream_counter() -> (u16, Arc<std::sync::atomic::AtomicUsize>) {
        let hits = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let counter = hits.clone();
        let app = axum::Router::new().route(
            "/send",
            axum::routing::any(move || {
                let counter = counter.clone();
                async move {
                    counter.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                    "ok"
                }
            }),
        );
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        (port, hits)
    }

    /// A state whose agent holds a `mail` credential that needs approval for POST.
    async fn approval_gated_agent(port: u16) -> (Arc<AppState>, SessionRecord) {
        approval_gated_agent_with(port, json!({})).await
    }

    /// The same, with extra fields merged into the `mail` descriptor (a `preview`, say).
    async fn approval_gated_agent_with(port: u16, extra: Value) -> (Arc<AppState>, SessionRecord) {
        let file = std::env::temp_dir().join(format!("zyvor-cred-{}.json", uuid::Uuid::new_v4()));
        let mut descriptor = json!({
            "host": "127.0.0.1", "header": "authorization", "kind": "fabric",
            "allowed_ports": [port], "requires_approval": ["POST"], "approval_kind": "send"
        });
        for (k, v) in extra.as_object().cloned().unwrap_or_default() {
            descriptor[k] = v;
        }
        std::fs::write(&file, json!({ "mail": descriptor }).to_string()).unwrap();
        let (state, session) =
            state_and_session_cfg(|config| config.credentials_file = Some(file)).await;
        let mut m = manifest(EgressMode::Deny, Some(30));
        m.credentials = vec!["mail".into()];
        m.egress_allow_hosts = vec!["127.0.0.1".into()];
        m.allow_private_networks = true;
        let deployed = state
            .store
            .deploy_agent(crate::model::DeployAgentRequest {
                name: session.agent.clone(),
                bundle_base64: base64::engine::general_purpose::STANDARD.encode("export default 1"),
                manifest: m,
            })
            .await
            .unwrap();
        let session = state
            .store
            .update_session(session.id, |s| s.agent_version = deployed.version.clone())
            .await
            .unwrap();
        (state, session)
    }

    async fn call(
        state: &AppState,
        session: &SessionRecord,
        port: u16,
        method: &str,
        body: Option<&str>,
    ) -> Result<Value, (StatusCode, String)> {
        let mut headers = HeaderMap::new();
        headers.insert(
            "x-zyvor-session-id",
            session.id.to_string().parse().unwrap(),
        );
        headers.insert("x-zyvor-egress-capability", "cap".parse().unwrap());
        proxy_inner(
            state,
            &headers,
            EgressRequest {
                url: format!("http://127.0.0.1:{port}/send?token=hunter2"),
                method: method.into(),
                headers: Default::default(),
                body_base64: body.map(|b| base64::engine::general_purpose::STANDARD.encode(b)),
                credential: Some("mail".into()),
            },
            None,
        )
        .await
    }

    async fn call_keyed(
        state: &AppState,
        session: &SessionRecord,
        port: u16,
        method: &str,
        body: Option<&str>,
        key: &str,
    ) -> Result<Value, (StatusCode, String)> {
        let mut headers = HeaderMap::new();
        headers.insert(
            "x-zyvor-session-id",
            session.id.to_string().parse().unwrap(),
        );
        headers.insert("x-zyvor-egress-capability", "cap".parse().unwrap());
        proxy_inner(
            state,
            &headers,
            EgressRequest {
                url: format!("http://127.0.0.1:{port}/send?token=hunter2"),
                method: method.into(),
                headers: [("Idempotency-Key".to_string(), key.to_string())].into(),
                body_base64: body.map(|b| base64::engine::general_purpose::STANDARD.encode(b)),
                credential: Some("mail".into()),
            },
            None,
        )
        .await
    }

    async fn approve(
        state: &AppState,
        session: uuid::Uuid,
        decision: ApprovalStatus,
    ) -> ApprovalRecord {
        let pending = wait_pending(state, session).await;
        state
            .store
            .transition_approval(pending.id, decision, None, Some(GrantScope::Once))
            .await
            .unwrap();
        pending
    }

    /// Approves a pending approval once more than `already` approvals exist.
    async fn approve_after(state: &AppState, session: uuid::Uuid, already: usize) {
        for _ in 0..200 {
            let all = state.store.list_approvals().await;
            if all.len() > already {
                if let Some(p) = all
                    .iter()
                    .find(|a| a.session_id == session && a.status == ApprovalStatus::Pending)
                {
                    state
                        .store
                        .transition_approval(
                            p.id,
                            ApprovalStatus::Approved,
                            None,
                            Some(GrantScope::Once),
                        )
                        .await
                        .unwrap();
                    return;
                }
            }
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
        panic!("no further approval appeared");
    }

    #[tokio::test]
    async fn a_keyed_request_is_performed_once_and_a_retry_is_answered_from_its_receipt() {
        let (port, count) = upstream_counter().await;
        let (state, session) = approval_gated_agent(port).await;
        let first = {
            let (state, session) = (state.clone(), session.clone());
            tokio::spawn(async move {
                call_keyed(
                    &state,
                    &session,
                    port,
                    "POST",
                    Some("send this once"),
                    "step-1:attempt-1",
                )
                .await
            })
        };
        let approval = approve(&state, session.id, ApprovalStatus::Approved).await;
        let reply = first.await.unwrap().unwrap();
        assert_eq!((reply["status"].clone(), hits(&count)), (json!(200), 1));
        assert!(reply["receipt_id"].is_string());

        // the retry (a goal step tried again, say): no new approval, nothing sent, the answer says it was a replay
        let retry = call_keyed(
            &state,
            &session,
            port,
            "POST",
            Some("send this once"),
            "step-1:attempt-1",
        )
        .await
        .unwrap();
        assert_eq!(retry["replayed"], true);
        assert_eq!(retry["status"], 200);
        assert_eq!(hits(&count), 1, "the retry was not sent");
        assert_eq!(
            state.store.list_approvals().await.len(),
            1,
            "and no second approval was opened"
        );

        // the receipt says what was approved and done, without the body or the query
        let receipts = state.store.receipts.list(None, 10).await;
        assert_eq!(receipts.len(), 1);
        let r = &receipts[0];
        assert_eq!(
            (
                r.approval_id,
                r.status,
                r.method.as_str(),
                r.credential.as_deref()
            ),
            (Some(approval.id), 200, "POST", Some("mail"))
        );
        assert_eq!(
            r.body_sha256,
            hex::encode(sha2::Sha256::digest(b"send this once"))
        );
        let stored = serde_json::to_string(&receipts).unwrap();
        assert!(
            !stored.contains("send this once") && !stored.contains("hunter2"),
            "{stored}"
        );
        let journal =
            serde_json::to_string(&state.store.audit.list(None, 100).await.unwrap()).unwrap();
        assert!(
            journal.contains("merion.action.performed") && journal.contains("merion.action.replayed")
        );
        assert!(
            !journal.contains("send this once"),
            "the journal never holds the body"
        );
    }

    #[tokio::test]
    async fn the_same_key_with_a_different_request_is_refused_and_nothing_is_sent() {
        let (port, count) = upstream_counter().await;
        let (state, session) = approval_gated_agent(port).await;
        let first = {
            let (state, session) = (state.clone(), session.clone());
            tokio::spawn(async move {
                call_keyed(&state, &session, port, "POST", Some("one"), "k").await
            })
        };
        approve(&state, session.id, ApprovalStatus::Approved).await;
        first.await.unwrap().unwrap();
        let err = call_keyed(&state, &session, port, "POST", Some("two"), "k")
            .await
            .unwrap_err();
        assert_eq!(err.0, StatusCode::CONFLICT);
        assert!(err.1.contains("different request"), "{}", err.1);
        assert_eq!(
            (hits(&count), state.store.list_approvals().await.len()),
            (1, 1),
            "nothing was sent and nobody was asked"
        );
    }

    #[tokio::test]
    async fn without_a_key_every_request_is_asked_about_and_gets_its_own_receipt() {
        let (port, count) = upstream_counter().await;
        let (state, session) = approval_gated_agent(port).await;
        for _ in 0..2 {
            let waiter = {
                let (state, session) = (state.clone(), session.clone());
                tokio::spawn(async move { call(&state, &session, port, "POST", Some("x")).await })
            };
            approve_after(&state, session.id, state.store.list_approvals().await.len()).await;
            waiter.await.unwrap().unwrap();
        }
        assert_eq!(hits(&count), 2);
        let receipts = state.store.receipts.list(None, 10).await;
        assert_eq!(receipts.len(), 2);
        assert!(receipts.iter().all(|r| r.idempotency_key.is_none()));
    }

    #[tokio::test]
    async fn a_denied_request_leaves_no_receipt_and_its_key_can_be_used_again() {
        let (port, count) = upstream_counter().await;
        let (state, session) = approval_gated_agent(port).await;
        let denied = {
            let (state, session) = (state.clone(), session.clone());
            tokio::spawn(
                async move { call_keyed(&state, &session, port, "POST", Some("x"), "k").await },
            )
        };
        approve(&state, session.id, ApprovalStatus::Denied).await;
        assert!(denied.await.unwrap().is_err());
        assert!(
            state.store.receipts.list(None, 10).await.is_empty(),
            "a refusal is not a performed action"
        );
        assert_eq!(hits(&count), 0);
        // the same key, asked again, opens a fresh approval and can go through
        let already = state.store.list_approvals().await.len();
        let again = {
            let (state, session) = (state.clone(), session.clone());
            tokio::spawn(
                async move { call_keyed(&state, &session, port, "POST", Some("x"), "k").await },
            )
        };
        approve_after(&state, session.id, already).await;
        again.await.unwrap().unwrap();
        assert_eq!(
            (
                hits(&count),
                state.store.receipts.list(None, 10).await.len()
            ),
            (1, 1)
        );
    }

    #[tokio::test]
    async fn a_bad_key_is_refused_before_anything_happens() {
        let (port, count) = upstream_counter().await;
        let (state, session) = approval_gated_agent(port).await;
        for bad in ["has space", "", &"k".repeat(200)] {
            let err = call_keyed(&state, &session, port, "POST", Some("x"), bad)
                .await
                .unwrap_err();
            assert_eq!(err.0, StatusCode::BAD_REQUEST, "{bad:?}");
        }
        assert_eq!(
            (hits(&count), state.store.list_approvals().await.len()),
            (0, 0)
        );
    }

    #[tokio::test]
    async fn the_same_key_while_the_first_is_still_waiting_is_refused_not_duplicated() {
        let (port, count) = upstream_counter().await;
        let (state, session) = approval_gated_agent(port).await;
        let first = {
            let (state, session) = (state.clone(), session.clone());
            tokio::spawn(
                async move { call_keyed(&state, &session, port, "POST", Some("x"), "k").await },
            )
        };
        wait_pending(&state, session.id).await;
        let err = call_keyed(&state, &session, port, "POST", Some("x"), "k")
            .await
            .unwrap_err();
        assert_eq!(err.0, StatusCode::CONFLICT);
        assert!(err.1.contains("in progress"), "{}", err.1);
        approve(&state, session.id, ApprovalStatus::Approved).await;
        first.await.unwrap().unwrap();
        assert_eq!(hits(&count), 1);
    }

    #[tokio::test]
    async fn one_persons_key_never_answers_for_another_person() {
        let (port, count) = upstream_counter().await;
        let (state, session) = approval_gated_agent(port).await;
        let ana = state
            .store
            .update_session(session.id, |s| s.user_id = Some("ana".into()))
            .await
            .unwrap();
        let mut other = ana.clone();
        other.id = uuid::Uuid::new_v4();
        other.user_id = Some("ben".into());
        state.store.save_session(other.clone()).await.unwrap();
        let a = {
            let (state, ana) = (state.clone(), ana.clone());
            tokio::spawn(async move {
                call_keyed(&state, &ana, port, "POST", Some("x"), "same-key").await
            })
        };
        approve(&state, ana.id, ApprovalStatus::Approved).await;
        a.await.unwrap().unwrap();
        // ben, same key and same request: not a replay of ana's; his own approval is asked for and it is sent
        let b = {
            let (state, other) = (state.clone(), other.clone());
            tokio::spawn(async move {
                call_keyed(&state, &other, port, "POST", Some("x"), "same-key").await
            })
        };
        approve(&state, other.id, ApprovalStatus::Approved).await;
        let reply = b.await.unwrap().unwrap();
        assert!(reply["replayed"].is_null(), "not a replay");
        assert_eq!(hits(&count), 2);
        let mine = state.store.receipts.list(Some("ben"), 10).await;
        assert_eq!(
            (
                mine.len(),
                state.store.receipts.list(Some("ana"), 10).await.len()
            ),
            (1, 1)
        );
    }

    fn mail_body(message: &str) -> String {
        json!({"message": {"raw": base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(message)}}).to_string()
    }

    #[tokio::test]
    async fn an_approval_shows_the_person_what_the_body_says_and_keeps_it_off_the_record() {
        let (port, hits) = upstream_counter().await;
        let (state, session) =
            approval_gated_agent_with(port, json!({"preview": "gmail-message"})).await;
        let message = "To: ana@example.com\nBcc: boss@example.com\nSubject: Quarterly numbers\n\nAttached below.";
        let waiter = {
            let (state, session) = (state.clone(), session.clone());
            let body = mail_body(message);
            tokio::spawn(async move { call(&state, &session, port, "POST", Some(&body)).await })
        };
        let pending = wait_pending(&state, session.id).await;
        // the host read the real body
        let preview = pending.preview.clone().expect("a preview");
        let shown = preview.to_string();
        for want in [
            "ana@example.com",
            "boss@example.com",
            "Quarterly numbers",
            "Attached below.",
        ] {
            assert!(shown.contains(want), "{want} in {shown}");
        }
        // the phone signs a digest of that rendering
        let planned = pending.planned_action.clone().unwrap();
        let expected = crate::preview::render(
            crate::preview::GMAIL_MESSAGE,
            mail_body(message).as_bytes(),
            None,
        )
        .unwrap();
        assert_eq!(planned["preview_sha256"], expected.sha256());
        // and none of the text is in the prompt, the planned action, or the journal
        let journal =
            serde_json::to_string(&state.store.audit.list(None, 100).await.unwrap()).unwrap();
        for text in [&pending.prompt, &planned.to_string(), &journal] {
            for secret in [
                "ana@example.com",
                "boss@example.com",
                "Quarterly numbers",
                "Attached below.",
            ] {
                assert!(!text.contains(secret), "{secret} leaked into {text}");
            }
        }
        assert_eq!(
            hits.load(std::sync::atomic::Ordering::SeqCst),
            0,
            "nothing sent before the decision"
        );
        state
            .store
            .transition_approval(pending.id, ApprovalStatus::Approved, None, None)
            .await
            .unwrap();
        waiter.await.unwrap().unwrap();
        assert_eq!(hits.load(std::sync::atomic::Ordering::SeqCst), 1);
        let decided = state.store.get_approval(pending.id).await.unwrap();
        assert!(
            decided.preview.is_none(),
            "the rendering is dropped once decided"
        );
        let on_disk =
            std::fs::read_to_string(state.config.state_dir.join("approvals.json")).unwrap();
        assert!(
            !on_disk.contains("boss@example.com"),
            "and it is not left on disk"
        );
    }

    #[tokio::test]
    async fn a_body_the_host_cannot_render_is_not_sent_and_opens_no_approval() {
        let (port, hits) = upstream_counter().await;
        let (state, session) =
            approval_gated_agent_with(port, json!({"preview": "gmail-message"})).await;
        for body in [
            mail_body("To: a@b.co\nContent-Type: text/html\n\n<b>x</b>"),
            "not json".to_string(),
            json!({"id": "draft-1"}).to_string(),
        ] {
            let err = call(&state, &session, port, "POST", Some(&body))
                .await
                .unwrap_err();
            assert_eq!(err.0, StatusCode::UNPROCESSABLE_ENTITY, "{body}");
            assert!(err.1.contains("cannot be shown for approval"), "{}", err.1);
        }
        assert!(state.store.list_approvals().await.is_empty());
        assert_eq!(hits.load(std::sync::atomic::Ordering::SeqCst), 0);
    }

    /// A fake "Google": a token endpoint over plain loopback HTTP (`access_token = "at-" + refresh token`) and an HTTPS API (a certificate from a
    /// throwaway CA, which the caller must trust) that answers with the Authorization header it was called with. Returns the API port, the token
    /// port, the CA certificate (PEM) and every Authorization the API saw.
    async fn fake_google() -> (u16, u16, String, Arc<std::sync::Mutex<Vec<String>>>) {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        let dir = tempfile::tempdir().unwrap();
        let mitm = crate::mitm::Mitm::load_or_create(dir.path()).await.unwrap();
        let acceptor = tokio_rustls::TlsAcceptor::from(mitm.server_config("127.0.0.1").unwrap());
        let seen: Arc<std::sync::Mutex<Vec<String>>> = Arc::default();
        let api = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let api_port = api.local_addr().unwrap().port();
        let sink = seen.clone();
        tokio::spawn(async move {
            let _keep = dir; // the CA files live as long as the server
            loop {
                let Ok((tcp, _)) = api.accept().await else {
                    return;
                };
                let (acceptor, sink) = (acceptor.clone(), sink.clone());
                tokio::spawn(async move {
                    let Ok(mut tls) = acceptor.accept(tcp).await else {
                        return;
                    };
                    let mut buf = Vec::new();
                    let mut chunk = [0u8; 2048];
                    while !buf.windows(4).any(|w| w == b"\r\n\r\n") {
                        match tls.read(&mut chunk).await {
                            Ok(0) | Err(_) => return,
                            Ok(n) => buf.extend_from_slice(&chunk[..n]),
                        }
                    }
                    let text = String::from_utf8_lossy(&buf).to_string();
                    let auth = text
                        .lines()
                        .find_map(|l| {
                            l.to_ascii_lowercase()
                                .starts_with("authorization:")
                                .then(|| l.split_once(':').unwrap().1.trim().to_string())
                        })
                        .unwrap_or_default();
                    sink.lock().unwrap().push(auth.clone());
                    let body = json!({"saw": auth}).to_string();
                    let reply = format!("HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{body}", body.len());
                    let _ = tls.write_all(reply.as_bytes()).await;
                    let _ = tls.shutdown().await;
                });
            }
        });
        let ca = mitm.ca_pem().to_string();
        let token = axum::Router::new().route(
            "/token",
            axum::routing::post(|body: String| async move {
                let rt = url::form_urlencoded::parse(body.as_bytes())
                    .find(|(k, _)| k == "refresh_token")
                    .map(|(_, v)| v.to_string())
                    .unwrap_or_default();
                // like Microsoft, hand back a new refresh token every time
                axum::Json(
                    json!({"access_token": format!("at-{rt}"), "expires_in": 3600, "refresh_token": format!("{rt}~")}),
                )
            }),
        );
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let token_port = listener.local_addr().unwrap().port();
        tokio::spawn(async move { axum::serve(listener, token).await.unwrap() });
        (api_port, token_port, ca, seen)
    }

    #[tokio::test]
    async fn a_per_person_credential_reaches_the_api_with_that_persons_own_token_and_only_once_connected(
    ) {
        std::env::set_var("EG_PP_ID", "cid");
        std::env::set_var("EG_PP_SECRET", "csecret");
        let (api_port, token_port, ca_pem, seen) = fake_google().await;
        let ca_file = std::env::temp_dir().join(format!("zyvor-ca-{}.pem", uuid::Uuid::new_v4()));
        std::fs::write(&ca_file, ca_pem).unwrap();
        let file = std::env::temp_dir().join(format!("zyvor-cred-{}.json", uuid::Uuid::new_v4()));
        std::fs::write(
            &file,
            json!({"gmail": {
                "host": "127.0.0.1", "header": "authorization", "kind": "oauth-refresh", "allowed_ports": [api_port], "allowed_methods": ["GET"],
                "oauth": {"token_url": format!("http://127.0.0.1:{token_port}/token"), "client_id_env": "EG_PP_ID", "client_secret_env": "EG_PP_SECRET", "connection": "google"}
            }})
            .to_string(),
        )
        .unwrap();
        let (state, base) = state_and_session_cfg(|config| {
            config.credentials_file = Some(file);
            config.extra_ca_files = vec![ca_file];
        })
        .await;
        let mut m = manifest(EgressMode::Deny, Some(30));
        m.credentials = vec!["gmail".into()];
        m.egress_allow_hosts = vec!["127.0.0.1".into()];
        m.allow_private_networks = true;
        let deployed = state
            .store
            .deploy_agent(crate::model::DeployAgentRequest {
                name: base.agent.clone(),
                bundle_base64: base64::engine::general_purpose::STANDARD.encode("export default 1"),
                manifest: m,
            })
            .await
            .unwrap();
        let session_of = |user: Option<&str>| {
            let mut s = base.clone();
            s.id = uuid::Uuid::new_v4();
            s.agent_version = deployed.version.clone();
            s.user_id = user.map(str::to_string);
            s
        };
        let (ana, ben, nobody) = (
            session_of(Some("ana")),
            session_of(Some("ben")),
            session_of(None),
        );
        for s in [&ana, &ben, &nobody] {
            state.store.save_session(s.clone()).await.unwrap();
        }
        let read = |session: SessionRecord| {
            let state = state.clone();
            async move {
                let mut headers = HeaderMap::new();
                headers.insert(
                    "x-zyvor-session-id",
                    session.id.to_string().parse().unwrap(),
                );
                headers.insert("x-zyvor-egress-capability", "cap".parse().unwrap());
                proxy_inner(
                    &state,
                    &headers,
                    EgressRequest {
                        url: format!(
                            "https://127.0.0.1:{api_port}/gmail/v1/users/me/messages?maxResults=3"
                        ),
                        method: "GET".into(),
                        headers: Default::default(),
                        body_base64: None,
                        credential: Some("gmail".into()),
                    },
                    None,
                )
                .await
            }
        };
        // before anyone connected: refused, and it says what to do
        let err = read(ana.clone()).await.unwrap_err();
        assert_eq!(err.0, StatusCode::FORBIDDEN);
        assert!(
            err.1.contains("connect your google account first"),
            "{}",
            err.1
        );
        assert!(seen.lock().unwrap().is_empty(), "nothing reached the API");
        // a session with no user cannot use a person's connection at all
        assert!(read(nobody).await.unwrap_err().1.contains("has no user"));
        // each person's request carries their own token
        state
            .store
            .connections
            .set("ana", "google", "1//ana-refresh")
            .await
            .unwrap();
        state
            .store
            .connections
            .set("ben", "google", "1//ben-refresh")
            .await
            .unwrap();
        let (a, b) = (
            read(ana.clone()).await.unwrap(),
            read(ben.clone()).await.unwrap(),
        );
        for (reply, who) in [(&a, "ana"), (&b, "ben")] {
            let body: Value = serde_json::from_slice(
                &base64::engine::general_purpose::STANDARD
                    .decode(reply["body_base64"].as_str().unwrap())
                    .unwrap(),
            )
            .unwrap();
            assert_eq!(body["saw"], format!("Bearer at-1//{who}-refresh"), "{who}");
        }
        assert_eq!(
            *seen.lock().unwrap(),
            ["Bearer at-1//ana-refresh", "Bearer at-1//ben-refresh"]
        );
        // the rotated refresh tokens were kept, each person's own
        for (who, want) in [("ana", "1//ana-refresh~"), ("ben", "1//ben-refresh~")] {
            assert_eq!(
                state
                    .store
                    .connections
                    .refresh_token(who, "google")
                    .await
                    .as_deref(),
                Some(want),
                "{who}"
            );
        }
        // ana disconnecting closes her at once; ben is unaffected
        state
            .store
            .connections
            .remove("ana", "google")
            .await
            .unwrap();
        state.credentials.forget_user_connection("ana", "google");
        assert!(read(ana)
            .await
            .unwrap_err()
            .1
            .contains("connect your google account first"));
        assert!(read(ben).await.is_ok(), "ben is still connected");
        assert!(
            state
                .store
                .connections
                .refresh_token("ana", "google")
                .await
                .is_none(),
            "a rotation never brings a disconnected person back"
        );
    }

    #[tokio::test]
    async fn send_credential_holds_the_request_until_approved_and_never_records_the_body() {
        let (port, hits) = upstream_counter().await;
        let (state, session) = approval_gated_agent(port).await;
        let waiter = {
            let (state, session) = (state.clone(), session.clone());
            tokio::spawn(async move {
                call(&state, &session, port, "POST", Some("hello secret body")).await
            })
        };
        let pending = wait_pending(&state, session.id).await;
        assert_eq!(pending.kind, ApprovalKind::Send);
        let planned = pending.planned_action.clone().unwrap();
        assert_eq!(planned["method"], "POST");
        assert_eq!(planned["credential"], "mail");
        assert_eq!(planned["body_bytes"], 17);
        let expected = hex::encode(sha2::Sha256::digest(b"hello secret body"));
        assert_eq!(planned["body_sha256"], expected);
        let stored = format!("{planned} {}", pending.prompt);
        assert!(
            !stored.contains("hello secret body"),
            "body leaked: {stored}"
        );
        assert!(!stored.contains("hunter2"), "query leaked: {stored}");
        // Nothing left the host while the decision was pending.
        assert_eq!(hits.load(std::sync::atomic::Ordering::SeqCst), 0);
        assert!(!waiter.is_finished());

        state
            .store
            .transition_approval(
                pending.id,
                ApprovalStatus::Approved,
                None,
                Some(GrantScope::Once),
            )
            .await
            .unwrap();
        let reply = waiter.await.unwrap().unwrap();
        assert_eq!(reply["status"], 200);
        assert_eq!(hits.load(std::sync::atomic::Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn approving_one_send_does_not_cover_the_next_and_denial_stops_it() {
        let (port, hits) = upstream_counter().await;
        let (state, session) = approval_gated_agent(port).await;
        for decision in [ApprovalStatus::Approved, ApprovalStatus::Denied] {
            let waiter = {
                let (state, session) = (state.clone(), session.clone());
                tokio::spawn(async move { call(&state, &session, port, "POST", Some("x")).await })
            };
            // Each request opens its own approval, even after an earlier approval.
            let pending = wait_pending(&state, session.id).await;
            state
                .store
                .transition_approval(pending.id, decision, None, Some(GrantScope::Session))
                .await
                .unwrap();
            let result = waiter.await.unwrap();
            if decision == ApprovalStatus::Denied {
                assert!(result.unwrap_err().1.contains("denied by an operator"));
            } else {
                assert!(result.is_ok());
            }
        }
        assert_eq!(hits.load(std::sync::atomic::Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn methods_not_listed_skip_the_approval() {
        let (port, hits) = upstream_counter().await;
        let (state, session) = approval_gated_agent(port).await;
        let reply = call(&state, &session, port, "GET", None).await.unwrap();
        assert_eq!(reply["status"], 200);
        assert_eq!(hits.load(std::sync::atomic::Ordering::SeqCst), 1);
        assert!(state.store.list_approvals().await.is_empty());
    }

    // ---- the agent cannot reach the approval API ----

    async fn status(
        router: axum::Router,
        method: &str,
        uri: &str,
        bearer: Option<&str>,
    ) -> StatusCode {
        use tower::ServiceExt;
        let mut request = axum::http::Request::builder().method(method).uri(uri);
        if let Some(token) = bearer {
            request = request.header("authorization", format!("Bearer {token}"));
        }
        let request = request
            .header("content-type", "application/json")
            .body(axum::body::Body::from("{\"decision\":\"approved\"}"))
            .unwrap();
        router.oneshot(request).await.unwrap().status()
    }

    #[tokio::test]
    async fn the_egress_broker_does_not_serve_operator_routes() {
        let (state, session) = state_and_session().await;
        let id = uuid::Uuid::new_v4();
        for (method, uri) in [
            ("GET", "/v1/approvals".to_string()),
            ("POST", format!("/v1/approvals/{id}")),
            ("GET", "/v1/audit".to_string()),
            ("POST", "/v1/agents".to_string()),
            ("GET", format!("/v1/sessions/{}", session.id)),
        ] {
            let code = status(broker_router(state.clone()), method, &uri, Some("cap")).await;
            assert_eq!(code, StatusCode::NOT_FOUND, "{method} {uri}");
        }
    }

    #[tokio::test]
    async fn a_session_capability_is_not_an_operator_token() {
        let (state, session) =
            state_and_session_cfg(|c| c.api_token = Some("operator".into())).await;
        let public = || crate::app::public_router(state.clone());
        let id = uuid::Uuid::new_v4();
        let uri = format!("/v1/approvals/{id}");
        // The agent's own credential is refused outright.
        for token in ["cap", &session.id.to_string()] {
            assert_eq!(
                status(public(), "GET", "/v1/approvals", Some(token)).await,
                StatusCode::UNAUTHORIZED
            );
            assert_eq!(
                status(public(), "POST", &uri, Some(token)).await,
                StatusCode::UNAUTHORIZED
            );
        }
        assert_eq!(
            status(public(), "GET", "/v1/approvals", None).await,
            StatusCode::UNAUTHORIZED
        );
        // The operator token gets through to the handler (404: no such approval).
        assert_eq!(
            status(public(), "POST", &uri, Some("operator")).await,
            StatusCode::NOT_FOUND
        );
        assert_eq!(
            status(public(), "GET", "/v1/approvals", Some("operator")).await,
            StatusCode::OK
        );
    }

    // ---- layer 7 rules, DLP, and taint ----

    /// Deploy an agent that may reach 127.0.0.1 and adjust its manifest, and
    /// point the session at that version.
    async fn deploy_local_agent(
        state: &AppState,
        session: &SessionRecord,
        tweak: impl FnOnce(&mut AgentManifest),
    ) -> SessionRecord {
        let mut m = manifest(EgressMode::Deny, Some(30));
        m.egress_allow_hosts = vec!["127.0.0.1".into()];
        m.allow_private_networks = true;
        tweak(&mut m);
        let deployed = state
            .store
            .deploy_agent(crate::model::DeployAgentRequest {
                name: session.agent.clone(),
                bundle_base64: base64::engine::general_purpose::STANDARD.encode("export default 1"),
                manifest: m,
            })
            .await
            .unwrap();
        state
            .store
            .update_session(session.id, |s| s.agent_version = deployed.version.clone())
            .await
            .unwrap()
    }

    async fn plain_call(
        state: &AppState,
        session: &SessionRecord,
        port: u16,
        method: &str,
        body: Option<&str>,
    ) -> Result<Value, (StatusCode, String)> {
        let mut headers = HeaderMap::new();
        headers.insert(
            "x-zyvor-session-id",
            session.id.to_string().parse().unwrap(),
        );
        headers.insert("x-zyvor-egress-capability", "cap".parse().unwrap());
        proxy_inner(
            state,
            &headers,
            EgressRequest {
                url: format!("http://127.0.0.1:{port}/send"),
                method: method.into(),
                headers: Default::default(),
                body_base64: body.map(|b| base64::engine::general_purpose::STANDARD.encode(b)),
                credential: None,
            },
            None,
        )
        .await
    }

    fn hits(counter: &Arc<std::sync::atomic::AtomicUsize>) -> usize {
        counter.load(std::sync::atomic::Ordering::SeqCst)
    }

    #[tokio::test]
    async fn egress_rules_refuse_before_anything_is_asked_or_sent() {
        let (port, counter) = upstream_counter().await;
        let (state, session) = state_and_session().await;
        let session = deploy_local_agent(&state, &session, |m| {
            m.egress_rules = vec![crate::model::EgressRule {
                host: "127.0.0.1".into(),
                methods: vec!["GET".into()],
                path_prefixes: vec![],
                max_body_bytes: None,
                ..Default::default()
            }];
        })
        .await;
        assert!(plain_call(&state, &session, port, "GET", None)
            .await
            .is_ok());
        let error = plain_call(&state, &session, port, "POST", Some("x"))
            .await
            .unwrap_err();
        assert_eq!(error.0, StatusCode::FORBIDDEN);
        assert!(error.1.contains("egress rules"), "{}", error.1);
        assert_eq!(hits(&counter), 1);
        assert!(state.store.list_approvals().await.is_empty());
    }

    #[tokio::test]
    async fn the_guard_refuses_a_request_before_it_is_sent_and_a_dead_guard_refuses_everything() {
        let (port, counter) = upstream_counter().await;
        // A guard that refuses any body containing "forbidden".
        let guard = axum::Router::new().route(
            "/check",
            axum::routing::post(|axum::Json(v): axum::Json<Value>| async move {
                let body = base64::engine::general_purpose::STANDARD
                    .decode(v["body_base64"].as_str().unwrap_or(""))
                    .unwrap_or_default();
                if String::from_utf8_lossy(&body).contains("forbidden") {
                    axum::Json(json!({"decision": "deny", "reason": "policy says no"}))
                } else {
                    axum::Json(json!({"decision": "allow"}))
                }
            }),
        );
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let guard_url = format!("http://{}/check", listener.local_addr().unwrap());
        tokio::spawn(async move { axum::serve(listener, guard).await.unwrap() });

        let cfg = |url: &str| {
            crate::guard::GuardConfig::new(url, None, std::time::Duration::from_secs(2)).unwrap()
        };
        let (state, session) = state_and_session_cfg(|c| c.guard = Some(cfg(&guard_url))).await;
        let session = deploy_local_agent(&state, &session, |_| {}).await;

        assert!(plain_call(&state, &session, port, "POST", Some("fine"))
            .await
            .is_ok());
        assert_eq!(hits(&counter), 1);
        let error = plain_call(&state, &session, port, "POST", Some("forbidden thing"))
            .await
            .unwrap_err();
        assert_eq!(error.0, StatusCode::FORBIDDEN);
        assert!(error.1.contains("egress guard"), "{}", error.1);
        assert!(error.1.contains("policy says no"), "{}", error.1);
        assert_eq!(hits(&counter), 1, "a refused request must never be sent");
        assert!(state.store.list_approvals().await.is_empty());

        // A guard that is down refuses everything, including what it would have allowed.
        let (state, session) =
            state_and_session_cfg(|c| c.guard = Some(cfg("http://127.0.0.1:1/check"))).await;
        let session = deploy_local_agent(&state, &session, |_| {}).await;
        let error = plain_call(&state, &session, port, "POST", Some("fine"))
            .await
            .unwrap_err();
        assert_eq!(error.0, StatusCode::FORBIDDEN);
        assert!(error.1.contains("egress guard"), "{}", error.1);
        assert_eq!(hits(&counter), 1);
    }

    #[tokio::test]
    async fn body_rules_refuse_a_disallowed_mcp_call_before_it_is_sent() {
        let (port, counter) = upstream_counter().await;
        let (state, session) = state_and_session().await;
        let session = deploy_local_agent(&state, &session, |m| {
            m.egress_rules = vec![crate::model::EgressRule {
                host: "127.0.0.1".into(),
                methods: vec!["POST".into()],
                rpc_methods: vec!["tools/call".into(), "tools/list".into()],
                mcp_tools: vec!["search".into()],
                ..Default::default()
            }];
        })
        .await;
        let allowed =
            r#"{"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"search"}}"#;
        assert!(plain_call(&state, &session, port, "POST", Some(allowed))
            .await
            .is_ok());
        assert_eq!(hits(&counter), 1);

        for refused in [
            r#"{"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"delete_all"}}"#,
            r#"{"jsonrpc":"2.0","id":3,"method":"resources/read"}"#,
            r#"[{"method":"tools/list","id":4},{"method":"tools/call","id":5,"params":{"name":"delete_all"}}]"#,
            "not json",
        ] {
            let error = plain_call(&state, &session, port, "POST", Some(refused))
                .await
                .unwrap_err();
            assert_eq!(error.0, StatusCode::FORBIDDEN, "{refused}");
            assert!(error.1.contains("request body"), "{}", error.1);
        }
        assert_eq!(
            hits(&counter),
            1,
            "a refused call must never reach the host"
        );
        assert!(state.store.list_approvals().await.is_empty());
    }

    /// A stand-in FluxVM whose guest `process` answers with whatever `answer` holds, counting calls.
    async fn fake_fluxvm(
        answer: Arc<std::sync::Mutex<Value>>,
    ) -> (String, Arc<std::sync::atomic::AtomicUsize>) {
        let calls = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let app = axum::Router::new()
            .route(
                "/v1/sandboxes/{id}/process",
                axum::routing::post(
                    |axum::extract::State((answer, calls)): axum::extract::State<(
                        Arc<std::sync::Mutex<Value>>,
                        Arc<std::sync::atomic::AtomicUsize>,
                    )>,
                     axum::Json(body): axum::Json<Value>| async move {
                        calls.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                        assert!(
                            body["command"]
                                .as_str()
                                .unwrap()
                                .starts_with(crate::binary_id::GUEST_PATH),
                            "unexpected guest command {body}"
                        );
                        axum::Json(answer.lock().unwrap().clone())
                    },
                ),
            )
            .with_state((answer, calls.clone()));
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        (url, calls)
    }

    fn guest_says(path: &str, sha: &str) -> Value {
        json!({"result": "exec", "exit_code": 0, "stdout": format!("77 {path} {sha}\n"), "stderr": ""})
    }

    async fn call_from(
        state: &AppState,
        session: &SessionRecord,
        port: u16,
        peer: Option<std::net::SocketAddr>,
    ) -> Result<Value, (StatusCode, String)> {
        let mut headers = HeaderMap::new();
        headers.insert(
            "x-zyvor-session-id",
            session.id.to_string().parse().unwrap(),
        );
        headers.insert("x-zyvor-egress-capability", "cap".parse().unwrap());
        proxy_inner(
            state,
            &headers,
            EgressRequest {
                url: format!("http://127.0.0.1:{port}/send"),
                method: "POST".into(),
                headers: Default::default(),
                body_base64: None,
                credential: None,
            },
            peer,
        )
        .await
    }

    #[tokio::test]
    async fn a_rule_with_binaries_admits_only_the_program_the_guest_names() {
        let (port, counter) = upstream_counter().await;
        let answer = Arc::new(std::sync::Mutex::new(guest_says(
            "/usr/bin/node",
            &"a".repeat(64),
        )));
        let (fluxvm_url, lookups) = fake_fluxvm(answer.clone()).await;
        let (state, session) = state_and_session_cfg(|c| c.fluxvm_url = fluxvm_url).await;
        let session = deploy_local_agent(&state, &session, |m| {
            m.egress_rules = vec![crate::model::EgressRule {
                host: "127.0.0.1".into(),
                methods: vec!["POST".into()],
                binaries: vec![crate::model::BinaryRule {
                    path: "/usr/bin/node".into(),
                    sha256: None,
                }],
                ..Default::default()
            }];
        })
        .await;
        let peer = |port: u16| Some(std::net::SocketAddr::from(([127, 0, 0, 1], port)));

        // The named program gets through, and its hash is remembered.
        assert!(call_from(&state, &session, port, peer(40001)).await.is_ok());
        assert_eq!(hits(&counter), 1);
        let pins = crate::binary_id::list_pins(&state.config.state_dir, &session.agent)
            .await
            .unwrap();
        assert_eq!(pins, vec![("/usr/bin/node".to_string(), "a".repeat(64))]);
        assert_eq!(lookups.load(std::sync::atomic::Ordering::SeqCst), 1);

        // The same connection again within the cache window does not ask the guest again.
        assert!(call_from(&state, &session, port, peer(40001)).await.is_ok());
        assert_eq!(lookups.load(std::sync::atomic::Ordering::SeqCst), 1);
        assert_eq!(hits(&counter), 2);

        // Same path, different hash: not the program first seen.
        *answer.lock().unwrap() = guest_says("/usr/bin/node", &"b".repeat(64));
        let err = call_from(&state, &session, port, peer(40002))
            .await
            .unwrap_err();
        assert_eq!(err.0, StatusCode::FORBIDDEN);
        assert!(err.1.contains("clear the pin"), "{}", err.1);

        // A program the rule does not name.
        *answer.lock().unwrap() = guest_says("/tmp/dropper", &"c".repeat(64));
        let err = call_from(&state, &session, port, peer(40003))
            .await
            .unwrap_err();
        assert_eq!(err.0, StatusCode::FORBIDDEN);
        assert!(err.1.contains("program"), "{}", err.1);

        // The guest cannot find the owner, or answers with junk.
        *answer.lock().unwrap() =
            json!({"result": "exec", "exit_code": 1, "stdout": "", "stderr": ""});
        assert!(call_from(&state, &session, port, peer(40004))
            .await
            .unwrap_err()
            .1
            .contains("could not be identified"));
        *answer.lock().unwrap() =
            json!({"result": "exec", "exit_code": 0, "stdout": "not an identity", "stderr": ""});
        assert!(call_from(&state, &session, port, peer(40005))
            .await
            .is_err());

        // No peer address (a path the runtime cannot attribute) refuses too.
        *answer.lock().unwrap() = guest_says("/usr/bin/node", &"a".repeat(64));
        assert!(call_from(&state, &session, port, None)
            .await
            .unwrap_err()
            .1
            .contains("could not be identified"));
        assert_eq!(hits(&counter), 2, "nothing refused may reach the host");

        // An operator clears the pin; the rebuilt program is trusted afresh.
        *answer.lock().unwrap() = guest_says("/usr/bin/node", &"b".repeat(64));
        assert_eq!(
            crate::binary_id::clear_pins(&state.config.state_dir, &session.agent, None)
                .await
                .unwrap(),
            1
        );
        assert!(call_from(&state, &session, port, peer(40006)).await.is_ok());
        assert_eq!(hits(&counter), 3);
        assert!(state.store.list_approvals().await.is_empty());
    }

    #[tokio::test]
    async fn a_pinned_hash_in_the_rule_needs_no_first_use_and_a_host_without_binaries_never_asks() {
        let (port, counter) = upstream_counter().await;
        let answer = Arc::new(std::sync::Mutex::new(guest_says(
            "/usr/bin/node",
            &"a".repeat(64),
        )));
        let (fluxvm_url, lookups) = fake_fluxvm(answer.clone()).await;
        let (state, session) = state_and_session_cfg(|c| c.fluxvm_url = fluxvm_url).await;
        let pinned = "a".repeat(64);
        let session = deploy_local_agent(&state, &session, |m| {
            m.egress_rules = vec![crate::model::EgressRule {
                host: "127.0.0.1".into(),
                binaries: vec![crate::model::BinaryRule {
                    path: "/usr/bin/node".into(),
                    sha256: Some(pinned.clone()),
                }],
                ..Default::default()
            }];
        })
        .await;
        let peer = |port: u16| Some(std::net::SocketAddr::from(([127, 0, 0, 1], port)));
        assert!(call_from(&state, &session, port, peer(41001)).await.is_ok());
        // The policy pinned it, so nothing is remembered on the side.
        assert!(
            crate::binary_id::list_pins(&state.config.state_dir, &session.agent)
                .await
                .unwrap()
                .is_empty()
        );
        *answer.lock().unwrap() = guest_says("/usr/bin/node", &"b".repeat(64));
        assert!(call_from(&state, &session, port, peer(41002))
            .await
            .is_err());
        assert_eq!(hits(&counter), 1);

        // With no rule that names binaries, the guest is never asked.
        let before = lookups.load(std::sync::atomic::Ordering::SeqCst);
        let session = deploy_local_agent(&state, &session, |m| {
            m.egress_rules = vec![crate::model::EgressRule {
                host: "127.0.0.1".into(),
                methods: vec!["POST".into()],
                ..Default::default()
            }];
        })
        .await;
        assert!(call_from(&state, &session, port, peer(41003)).await.is_ok());
        assert_eq!(lookups.load(std::sync::atomic::Ordering::SeqCst), before);
    }

    #[tokio::test]
    async fn a_credential_with_a_file_source_is_injected_only_after_the_checks_pass() {
        use std::os::unix::fs::PermissionsExt;
        let (api_port, _token_port, ca_pem, seen) = fake_google().await;
        let dir = std::env::temp_dir().join(format!("zyvor-src-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let ca_file = dir.join("ca.pem");
        std::fs::write(&ca_file, ca_pem).unwrap();
        let secret = dir.join("key");
        let write = |value: &str| {
            std::fs::write(&secret, value).unwrap();
            std::fs::set_permissions(&secret, std::fs::Permissions::from_mode(0o600)).unwrap();
        };
        write("file-secret-1\n");
        let creds = dir.join("creds.json");
        std::fs::write(
            &creds,
            json!({"svc": {
                "host": "127.0.0.1", "header": "authorization", "prefix": "Bearer ",
                "allowed_ports": [api_port], "allowed_methods": ["GET"],
                "source": {"kind": "file", "path": secret, "ttl_seconds": 1}
            }})
            .to_string(),
        )
        .unwrap();
        let (state, base) = state_and_session_cfg(|config| {
            config.credentials_file = Some(creds);
            config.extra_ca_files = vec![ca_file];
        })
        .await;
        let mut m = manifest(EgressMode::Deny, Some(30));
        m.credentials = vec!["svc".into()];
        m.egress_allow_hosts = vec!["127.0.0.1".into()];
        m.allow_private_networks = true;
        let deployed = state
            .store
            .deploy_agent(crate::model::DeployAgentRequest {
                name: base.agent.clone(),
                bundle_base64: base64::engine::general_purpose::STANDARD.encode("export default 1"),
                manifest: m,
            })
            .await
            .unwrap();
        let session = state
            .store
            .update_session(base.id, |s| s.agent_version = deployed.version.clone())
            .await
            .unwrap();
        let send = |method: &'static str| {
            let (state, session) = (state.clone(), session.clone());
            async move {
                let mut headers = HeaderMap::new();
                headers.insert(
                    "x-zyvor-session-id",
                    session.id.to_string().parse().unwrap(),
                );
                headers.insert("x-zyvor-egress-capability", "cap".parse().unwrap());
                proxy_inner(
                    &state,
                    &headers,
                    EgressRequest {
                        url: format!("https://127.0.0.1:{api_port}/v1/x"),
                        method: method.into(),
                        headers: Default::default(),
                        body_base64: (method == "POST")
                            .then(|| base64::engine::general_purpose::STANDARD.encode("x")),
                        credential: Some("svc".into()),
                    },
                    None,
                )
                .await
            }
        };

        // An allowed request gets the secret in the header, read from the file.
        let first = send("GET").await;
        assert!(first.is_ok(), "{first:?}");
        assert_eq!(seen.lock().unwrap().as_slice(), ["Bearer file-secret-1"]);
        assert_eq!(state.credentials.source_fetches(), 1);

        // A method the descriptor does not allow is refused, and the file is not read for it.
        let refused = send("POST").await.unwrap_err();
        assert_eq!(refused.0, StatusCode::FORBIDDEN);
        assert!(!refused.1.contains("file-secret"), "{}", refused.1);
        assert_eq!(seen.lock().unwrap().len(), 1);
        assert_eq!(state.credentials.source_fetches(), 1);

        // Rotation is seen after the cache time, not before.
        write("file-secret-2");
        assert!(send("GET").await.is_ok());
        assert_eq!(seen.lock().unwrap()[1], "Bearer file-secret-1");
        tokio::time::sleep(std::time::Duration::from_millis(1150)).await;
        assert!(send("GET").await.is_ok());
        assert_eq!(seen.lock().unwrap()[2], "Bearer file-secret-2");

        // Once the file is gone and the cache has expired the request fails closed: nothing is sent,
        // and the error does not carry the old secret.
        std::fs::remove_file(&secret).unwrap();
        tokio::time::sleep(std::time::Duration::from_millis(1150)).await;
        let gone = send("GET").await.unwrap_err();
        assert_eq!(gone.0, StatusCode::FORBIDDEN);
        assert!(!gone.1.contains("file-secret"), "{}", gone.1);
        assert_eq!(seen.lock().unwrap().len(), 3);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn dlp_holds_a_request_carrying_a_secret_and_never_stores_it() {
        let (port, counter) = upstream_counter().await;
        let (state, session) = state_and_session().await;
        let session = deploy_local_agent(&state, &session, |m| m.dlp = true).await;
        // A clean request goes straight through.
        assert!(plain_call(&state, &session, port, "POST", Some("hello"))
            .await
            .is_ok());
        assert_eq!(hits(&counter), 1);

        let secret = "AKIAIOSFODNN7EXAMPLE";
        let waiter = {
            let (state, session) = (state.clone(), session.clone());
            tokio::spawn(async move {
                plain_call(
                    &state,
                    &session,
                    port,
                    "POST",
                    Some(&format!("key={secret}")),
                )
                .await
            })
        };
        let pending = wait_pending(&state, session.id).await;
        assert_eq!(pending.kind, ApprovalKind::Send);
        let stored = format!("{:?} {}", pending.planned_action, pending.prompt);
        assert!(stored.contains("aws-access-key"), "{stored}");
        assert!(!stored.contains(secret), "secret leaked: {stored}");
        assert_eq!(hits(&counter), 1, "held request must not have been sent");
        state
            .store
            .transition_approval(pending.id, ApprovalStatus::Denied, None, None)
            .await
            .unwrap();
        assert!(waiter.await.unwrap().is_err());
        assert_eq!(hits(&counter), 1);
    }

    #[tokio::test]
    async fn reading_untrusted_content_taints_and_a_tainted_session_needs_approval_to_write() {
        let (port, counter) = upstream_counter().await;
        let (state, session) = state_and_session().await;
        let session = deploy_local_agent(&state, &session, |m| {
            m.taint = Some(crate::model::TaintPolicy {
                trusted_hosts: vec!["trusted.example".into()],
            });
        })
        .await;
        // A fresh session is clean, so its first write is not held.
        assert!(state
            .store
            .get_session(session.id)
            .await
            .unwrap()
            .tainted_by
            .is_empty());
        assert!(plain_call(&state, &session, port, "POST", Some("a"))
            .await
            .is_ok());
        // Any response from an untrusted host is content the agent has read, so
        // even that first write's reply taints the session.
        assert!(plain_call(&state, &session, port, "GET", None)
            .await
            .is_ok());
        let tainted = state.store.get_session(session.id).await.unwrap();
        assert_eq!(tainted.tainted_by, vec!["127.0.0.1".to_string()]);
        let entries = state.store.audit.list(Some(session.id), 50).await.unwrap();
        assert!(entries.iter().any(|e| e.action == "session.tainted"));
        // Reads still flow; a write is held.
        assert!(plain_call(&state, &session, port, "GET", None)
            .await
            .is_ok());
        let before = hits(&counter);
        let waiter = {
            let (state, session) = (state.clone(), session.clone());
            tokio::spawn(async move { plain_call(&state, &session, port, "POST", Some("b")).await })
        };
        let pending = wait_pending(&state, session.id).await;
        assert!(
            pending.prompt.contains("tainted by 127.0.0.1"),
            "{}",
            pending.prompt
        );
        assert_eq!(hits(&counter), before);
        state
            .store
            .transition_approval(
                pending.id,
                ApprovalStatus::Approved,
                None,
                Some(GrantScope::Once),
            )
            .await
            .unwrap();
        assert!(waiter.await.unwrap().is_ok());
        // An operator clearing the taint frees later writes.
        state.store.untaint_session(session.id).await.unwrap();
        assert!(plain_call(&state, &session, port, "POST", Some("c"))
            .await
            .is_ok());
    }

    #[tokio::test]
    async fn trusted_hosts_do_not_taint() {
        let (port, _counter) = upstream_counter().await;
        let (state, session) = state_and_session().await;
        let session = deploy_local_agent(&state, &session, |m| {
            m.taint = Some(crate::model::TaintPolicy {
                trusted_hosts: vec!["127.0.0.1".into()],
            });
        })
        .await;
        assert!(plain_call(&state, &session, port, "GET", None)
            .await
            .is_ok());
        assert!(state
            .store
            .get_session(session.id)
            .await
            .unwrap()
            .tainted_by
            .is_empty());
    }

    #[tokio::test]
    async fn a_tainted_session_cannot_be_auto_allowed_by_sentinel() {
        let mut cfg = reviewer(r#"{"verdict":"allow","reason":"looks fine"}"#).await;
        cfg.can_allow = true;
        let (state, session) = state_and_session_with(Some(cfg)).await;
        let mut m = manifest(EgressMode::Sentinel, Some(30));
        m.taint = Some(Default::default());
        // Untainted: the reviewer's allow lets it through with no human.
        authorize_unlisted_host(&state, &session, &m, &url(), "GET")
            .await
            .unwrap();
        assert!(state.store.list_approvals().await.is_empty());
        // Tainted: the same verdict now escalates to a person.
        state
            .store
            .taint_session(session.id, "evil.example")
            .await
            .unwrap();
        let waiter = {
            let (state, session, m) = (state.clone(), session.clone(), m.clone());
            tokio::spawn(async move {
                authorize_unlisted_host(&state, &session, &m, &url(), "GET").await
            })
        };
        let pending = wait_pending(&state, session.id).await;
        assert!(pending.planned_action.as_ref().unwrap()["sentinel"]
            .as_str()
            .unwrap()
            .contains("tainted"));
        state
            .store
            .transition_approval(pending.id, ApprovalStatus::Denied, None, None)
            .await
            .unwrap();
        assert!(waiter.await.unwrap().is_err());
    }

    #[tokio::test]
    async fn only_the_operator_can_untaint_a_session() {
        let (state, session) =
            state_and_session_cfg(|c| c.api_token = Some("operator".into())).await;
        state
            .store
            .taint_session(session.id, "evil.example")
            .await
            .unwrap();
        let uri = format!("/v1/sessions/{}/untaint", session.id);
        let public = || crate::app::public_router(state.clone());
        assert_eq!(
            status(public(), "POST", &uri, Some("cap")).await,
            StatusCode::UNAUTHORIZED
        );
        assert_eq!(
            status(broker_router(state.clone()), "POST", &uri, Some("cap")).await,
            StatusCode::NOT_FOUND
        );
        assert!(!state
            .store
            .get_session(session.id)
            .await
            .unwrap()
            .tainted_by
            .is_empty());
        assert_eq!(
            status(public(), "POST", &uri, Some("operator")).await,
            StatusCode::OK
        );
        assert!(state
            .store
            .get_session(session.id)
            .await
            .unwrap()
            .tainted_by
            .is_empty());
        let entries = state.store.audit.list(Some(session.id), 50).await.unwrap();
        assert!(entries.iter().any(|e| e.action == "session.untainted"));
    }

    /// Regression: deciding an approval the broker is holding used to steer the
    /// session too, which fails when the guest is unreachable and is meaningless
    /// because no agent is waiting for it.
    #[tokio::test]
    async fn deciding_a_broker_held_approval_does_not_steer_the_session() {
        let (state, session) =
            state_and_session_cfg(|c| c.api_token = Some("operator".into())).await;
        let open = |held: bool| ApprovalRecord {
            id: uuid::Uuid::new_v4(),
            session_id: session.id,
            kind: ApprovalKind::Send,
            subject: Some("mail.example".into()),
            planned_action: None,
            prompt: "send".into(),
            status: ApprovalStatus::Pending,
            comment: None,
            created_at: chrono::Utc::now(),
            decided_at: None,
            source_seq: None,
            broker_held: held,
            grant_scope: None,
            preview: None,
        };
        let held = open(true);
        state.store.save_approval(held.clone()).await.unwrap();
        let uri = format!("/v1/approvals/{}", held.id);
        let public = crate::app::public_router(state.clone());
        assert_eq!(
            status(public, "POST", &uri, Some("operator")).await,
            StatusCode::OK
        );
        assert_eq!(
            state.store.get_approval(held.id).await.unwrap().status,
            ApprovalStatus::Approved
        );
        // Control: an agent-initiated approval still steers the (unreachable) guest.
        let asked = open(false);
        state.store.save_approval(asked.clone()).await.unwrap();
        let uri = format!("/v1/approvals/{}", asked.id);
        let public = crate::app::public_router(state.clone());
        assert_ne!(
            status(public, "POST", &uri, Some("operator")).await,
            StatusCode::OK
        );
    }
}
