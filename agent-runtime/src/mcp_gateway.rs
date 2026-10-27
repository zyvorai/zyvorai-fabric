// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: Apache-2.0

//! The MCP gateway: tool servers the operator has registered, reached through the egress broker.
//! (docs/merion/design/mcp-gateway.md)
//!
//! An agent calls `POST /v1/mcp/{server}` on the broker port with a JSON-RPC body. The gateway checks the server is
//! registered, the JSON-RPC method is one it relays, a `tools/call` names a tool the operator listed, and the
//! session is under the server's rate limit; it journals the call (names only, never arguments) and then hands the
//! request to the ordinary egress broker, so the agent's host rules, body rules, DLP, taint and approvals all still
//! apply. A registry entry is necessary, not sufficient.

use crate::{
    app::{ApiError, ApiResult},
    audit::AuditPhase,
    model::EgressRequest,
    AppState,
};
use axum::{
    extract::{Path, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    Json,
};
use base64::Engine;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{
    collections::{HashMap, VecDeque},
    sync::{Arc, Mutex, OnceLock},
    time::{Duration, Instant},
};
use uuid::Uuid;

/// JSON-RPC methods the gateway relays. Everything else (sampling, resources, prompts, completion, unknown) is refused.
const RELAYED: [&str; 5] = [
    "initialize",
    "notifications/initialized",
    "ping",
    "tools/list",
    "tools/call",
];
const MAX_NAME: usize = 64;
const MAX_TOOLS: usize = 256;
const MAX_RATE: u32 = 6000;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct McpServer {
    /// Set from the path, never from the body.
    #[serde(default)]
    pub name: String,
    /// `https`, or `http` to a loopback address.
    pub url: String,
    /// Name of a vault credential the broker attaches. The agent never sees its value.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub credential: Option<String>,
    /// Tools a `tools/call` may name. Empty means no tool can be called.
    #[serde(default)]
    pub tools: Vec<String>,
    /// Calls per session per minute (any relayed method). No limit when absent.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_calls_per_minute: Option<u32>,
}

impl McpServer {
    pub fn parse(name: &str, text: &str) -> Result<Self, String> {
        if name.is_empty()
            || name.len() > MAX_NAME
            || !name
                .chars()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
        {
            return Err("name must be 1 to 64 characters of a-z, 0-9 and -".into());
        }
        let mut s: Self =
            serde_json::from_str(text).map_err(|e| format!("server is not valid: {e}"))?;
        s.name = name.to_string();
        let url = url::Url::parse(&s.url).map_err(|e| format!("url is not valid: {e}"))?;
        if !url.username().is_empty() || url.password().is_some() {
            return Err("url may not carry credentials; use `credential`".into());
        }
        let loopback = match url.host() {
            Some(url::Host::Ipv4(ip)) => ip.is_loopback(),
            Some(url::Host::Ipv6(ip)) => ip.is_loopback(),
            Some(url::Host::Domain(d)) => d.eq_ignore_ascii_case("localhost"),
            None => false,
        };
        match url.scheme() {
            "https" => {}
            "http" if loopback => {}
            _ => return Err("url must be https, or http to a loopback address".into()),
        }
        if s.tools.len() > MAX_TOOLS || s.tools.iter().any(|t| t.trim().is_empty()) {
            return Err(format!("tools: at most {MAX_TOOLS}, none empty"));
        }
        if s.max_calls_per_minute
            .is_some_and(|n| n == 0 || n > MAX_RATE)
        {
            return Err(format!("max_calls_per_minute must be 1 to {MAX_RATE}"));
        }
        Ok(s)
    }
}

/// What the gateway makes of a request body.
#[derive(Debug, PartialEq, Eq)]
pub enum Call {
    /// A relayed method; `tool` is set for `tools/call`.
    Relay {
        method: String,
        tool: Option<String>,
    },
    Refuse(String),
}

/// Pure check of a JSON-RPC body against a server's allowlist.
pub fn check(server: &McpServer, body: &Value) -> Call {
    let Some(object) = body.as_object() else {
        return Call::Refuse("batches and non-objects are not relayed".into());
    };
    if object.get("jsonrpc").and_then(Value::as_str) != Some("2.0") {
        return Call::Refuse("not a JSON-RPC 2.0 request".into());
    }
    let Some(method) = object.get("method").and_then(Value::as_str) else {
        return Call::Refuse("no method".into());
    };
    if !RELAYED.contains(&method) {
        return Call::Refuse(format!("method {method} is not relayed"));
    }
    if method != "tools/call" {
        return Call::Relay {
            method: method.into(),
            tool: None,
        };
    }
    let tool = object
        .get("params")
        .and_then(|p| p.get("name"))
        .and_then(Value::as_str);
    match tool {
        Some(t) if server.tools.iter().any(|allowed| allowed == t) => Call::Relay {
            method: method.into(),
            tool: Some(t.into()),
        },
        Some(t) => Call::Refuse(format!("tool {t} is not listed for this server")),
        None => Call::Refuse("tools/call names no tool".into()),
    }
}

type Window = VecDeque<Instant>;
fn windows() -> &'static Mutex<HashMap<(Uuid, String), Window>> {
    static W: OnceLock<Mutex<HashMap<(Uuid, String), Window>>> = OnceLock::new();
    W.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Records a call and says whether the session is still under `limit` per minute.
pub fn within_rate(session: Uuid, server: &str, limit: Option<u32>, now: Instant) -> bool {
    let Some(limit) = limit else { return true };
    let mut all = windows().lock().unwrap_or_else(|e| e.into_inner());
    // A session that is gone leaves a window behind; keep the map small.
    if all.len() > 4096 {
        all.retain(|_, w| {
            w.back()
                .is_some_and(|t| now.duration_since(*t) < Duration::from_secs(60))
        });
    }
    let w = all.entry((session, server.to_string())).or_default();
    while w
        .front()
        .is_some_and(|t| now.duration_since(*t) >= Duration::from_secs(60))
    {
        w.pop_front();
    }
    if w.len() >= limit as usize {
        return false;
    }
    w.push_back(now);
    true
}

/// `POST /v1/mcp/{server}` on the broker router.
pub async fn relay(
    State(state): State<Arc<AppState>>,
    Path(name): Path<String>,
    headers: HeaderMap,
    crate::egress::MaybePeer(peer): crate::egress::MaybePeer,
    Json(body): Json<Value>,
) -> Response {
    let deny = |status: StatusCode, message: String| {
        (status, Json(json!({ "error": message }))).into_response()
    };
    let session = match crate::egress::authenticate_session(&state, &headers).await {
        Ok(s) => s,
        Err((status, message)) => return deny(status, message),
    };
    let Some(server) = state.store.get_mcp_server(&name).await else {
        return deny(StatusCode::NOT_FOUND, "no such MCP server".into());
    };
    let journal = |phase: AuditPhase, detail: Value| {
        let state = state.clone();
        let (session, server) = (session.id, server.name.clone());
        async move {
            if let Err(error) = state
                .store
                .audit
                .append(Some(session), phase, "mcp.call", Some(server), detail)
                .await
            {
                tracing::error!(%error, "failed to write mcp audit entry");
            }
        }
    };
    let (method, tool) = match check(&server, &body) {
        Call::Relay { method, tool } => (method, tool),
        Call::Refuse(reason) => {
            journal(AuditPhase::Denied, json!({ "reason": reason })).await;
            return deny(StatusCode::FORBIDDEN, reason);
        }
    };
    if !within_rate(
        session.id,
        &server.name,
        server.max_calls_per_minute,
        Instant::now(),
    ) {
        let reason = "rate limit for this server reached; try again shortly".to_string();
        journal(
            AuditPhase::Denied,
            json!({ "reason": reason, "method": method, "tool": tool }),
        )
        .await;
        return deny(StatusCode::TOO_MANY_REQUESTS, reason);
    }
    let request = EgressRequest {
        url: server.url.clone(),
        method: "POST".into(),
        headers: [("content-type".to_string(), "application/json".to_string())].into(),
        body_base64: Some(
            base64::engine::general_purpose::STANDARD.encode(body.to_string().as_bytes()),
        ),
        credential: server.credential.clone(),
    };
    match crate::egress::proxy_inner(&state, &headers, request, peer).await {
        Ok(value) => {
            journal(
                AuditPhase::Performed,
                json!({ "method": method, "tool": tool, "status": value.get("status") }),
            )
            .await;
            (StatusCode::OK, Json(value)).into_response()
        }
        Err((status, message)) => {
            journal(
                AuditPhase::Denied,
                json!({ "method": method, "tool": tool, "reason": message }),
            )
            .await;
            deny(status, message)
        }
    }
}

// ---- operator routes (public router) ----

pub(crate) async fn list_servers(State(state): State<Arc<AppState>>) -> Json<Value> {
    Json(json!({ "servers": state.store.list_mcp_servers().await }))
}

pub(crate) async fn get_server(
    State(state): State<Arc<AppState>>,
    Path(name): Path<String>,
) -> ApiResult<Json<Value>> {
    state
        .store
        .get_mcp_server(&name)
        .await
        .map(|s| Json(json!(s)))
        .ok_or_else(|| ApiError::not_found("no such MCP server"))
}

pub(crate) async fn put_server(
    State(state): State<Arc<AppState>>,
    Path(name): Path<String>,
    body: String,
) -> ApiResult<Json<Value>> {
    let server = McpServer::parse(&name, &body).map_err(ApiError::bad_request)?;
    state
        .store
        .save_mcp_server(server.clone())
        .await
        .map_err(ApiError::internal)?;
    let _ = state
        .store
        .audit
        .append(
            None,
            AuditPhase::Performed,
            "merion.mcp_server.set",
            Some(server.name.clone()),
            json!({ "url": server.url, "tools": server.tools, "credential": server.credential,
                    "max_calls_per_minute": server.max_calls_per_minute }),
        )
        .await;
    Ok(Json(json!(server)))
}

pub(crate) async fn delete_server(
    State(state): State<Arc<AppState>>,
    Path(name): Path<String>,
) -> ApiResult<Json<Value>> {
    let removed = state
        .store
        .delete_mcp_server(&name)
        .await
        .map_err(ApiError::internal)?;
    if !removed {
        return Err(ApiError::not_found("no such MCP server"));
    }
    let _ = state
        .store
        .audit
        .append(
            None,
            AuditPhase::Performed,
            "merion.mcp_server.removed",
            Some(name),
            json!({}),
        )
        .await;
    Ok(Json(json!({ "removed": true })))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn server() -> McpServer {
        McpServer::parse(
            "files",
            r#"{"url":"https://mcp.example/rpc","tools":["read_file","list_dir"],"max_calls_per_minute":2}"#,
        )
        .unwrap()
    }

    #[test]
    fn registration_is_validated() {
        assert!(McpServer::parse("Files", r#"{"url":"https://a.example"}"#).is_err());
        assert!(McpServer::parse("a", r#"{"url":"http://a.example"}"#).is_err());
        assert!(McpServer::parse("a", r#"{"url":"http://127.0.0.1:9/x"}"#).is_ok());
        assert!(McpServer::parse("a", r#"{"url":"https://u:p@a.example"}"#).is_err());
        assert!(McpServer::parse("a", r#"{"url":"https://a.example","bogus":1}"#).is_err());
        assert!(McpServer::parse(
            "a",
            r#"{"url":"https://a.example","max_calls_per_minute":0}"#
        )
        .is_err());
        assert!(McpServer::parse("a", r#"{"url":"https://a.example","tools":[" "]}"#).is_err());
        assert_eq!(server().name, "files", "the name comes from the path");
    }

    #[test]
    fn only_listed_tools_and_relayed_methods_pass() {
        let s = server();
        let call = |v: Value| check(&s, &v);
        assert_eq!(
            call(
                json!({"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"read_file","arguments":{}}})
            ),
            Call::Relay {
                method: "tools/call".into(),
                tool: Some("read_file".into())
            }
        );
        assert!(matches!(
            call(
                json!({"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"delete_all"}})
            ),
            Call::Refuse(_)
        ));
        assert!(matches!(
            call(json!({"jsonrpc":"2.0","id":1,"method":"tools/call"})),
            Call::Refuse(_)
        ));
        assert!(matches!(
            call(json!({"jsonrpc":"2.0","id":1,"method":"sampling/createMessage"})),
            Call::Refuse(_)
        ));
        assert!(matches!(
            call(json!([{"jsonrpc":"2.0","method":"ping"}])),
            Call::Refuse(_)
        ));
        assert!(matches!(
            call(json!({"id":1,"method":"ping"})),
            Call::Refuse(_)
        ));
        assert!(matches!(
            call(json!({"jsonrpc":"2.0","id":1,"method":"tools/list"})),
            Call::Relay { tool: None, .. }
        ));
    }

    #[test]
    fn the_rate_limit_is_per_session_and_server_and_recovers() {
        let (a, b) = (Uuid::new_v4(), Uuid::new_v4());
        let t0 = Instant::now();
        assert!(within_rate(a, "s", Some(2), t0));
        assert!(within_rate(a, "s", Some(2), t0));
        assert!(!within_rate(a, "s", Some(2), t0), "third in a minute");
        assert!(
            within_rate(b, "s", Some(2), t0),
            "another session has its own window"
        );
        assert!(within_rate(a, "other", Some(2), t0), "another server too");
        assert!(
            within_rate(a, "s", Some(2), t0 + Duration::from_secs(61)),
            "window slides"
        );
        assert!(within_rate(a, "s", None, t0), "no limit set");
    }
}
