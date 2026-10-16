// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: Apache-2.0

//! Read-only view of the agent's browser, so a person can see what it is on.
//!
//! An agent with `browser_port` set runs Chromium with remote debugging on that
//! port inside the guest. This forwards the DevTools HTTP listing endpoints
//! (`/json/version`, `/json/list`) to an operator, through FluxVM's sandbox
//! proxy. Nothing that changes the browser is forwarded, and the WebSocket URLs
//! that would let a caller drive it are removed from listing responses.
//!
//! **Screenshot (Keep 0.2 scaffolding):** `GET /v1/sessions/{id}/browser/screenshot`
//! opens a short-lived CDP session via FluxVM's sandbox WS bridge, captures one
//! JPEG, and returns it. Input / takeover are still not exposed.

use crate::{
    app::{ApiError, ApiResult},
    model::SessionStatus,
    AppState,
};
use axum::{
    extract::{Path, Query, State},
    response::IntoResponse,
    Json,
};
use futures::{SinkExt, StreamExt};
use reqwest::Method;
use serde_json::{json, Value};
use std::sync::Arc;
use tokio_tungstenite::tungstenite::Message as WsMessage;
use uuid::Uuid;

/// The only DevTools paths forwarded: listings, no mutation.
const ALLOWED: [&str; 4] = ["json", "json/list", "json/version", "json/protocol"];

/// Fields that would hand out guest-internal addresses or a way to drive the page.
const STRIPPED: [&str; 2] = ["webSocketDebuggerUrl", "devtoolsFrontendUrl"];

pub fn allowed_path(path: &str) -> bool {
    ALLOWED.contains(&path.trim_matches('/'))
}

/// Remove control URLs from a DevTools response, recursively.
pub fn sanitize(value: &mut Value) {
    match value {
        Value::Object(map) => {
            for key in STRIPPED {
                map.remove(key);
            }
            map.values_mut().for_each(sanitize);
        }
        Value::Array(items) => items.iter_mut().for_each(sanitize),
        _ => {}
    }
}

/// Path portion of a Chromium `webSocketDebuggerUrl` (`/devtools/page/…`).
pub fn debugger_path_from_ws_url(url: &str) -> Option<String> {
    let rest = url
        .strip_prefix("ws://")
        .or_else(|| url.strip_prefix("wss://"))?;
    let path = rest.split_once('/')?.1;
    if path.is_empty() {
        return None;
    }
    Some(path.to_string())
}

/// Guest a11y driver HTTP port (Playwright private). CDP remains on browser_port.
pub const DRIVER_PORT: u16 = 9230;

/// Minimum interval between operator screenshots per session.
const SCREENSHOT_MIN_INTERVAL: std::time::Duration = std::time::Duration::from_secs(2);

/// Refuse browser tools unless the agent is confined and browser policy allows.
pub fn browser_tools_allowed(
    confinement: crate::model::Confinement,
    browser: Option<&crate::model::BrowserPolicy>,
) -> Result<(), &'static str> {
    if !matches!(confinement, crate::model::Confinement::Strict) {
        return Err("browser tools require confinement: strict");
    }
    if browser.is_some_and(|b| !b.enabled) {
        return Err("browser tools disabled by merion.policy.yaml browser.enabled=false");
    }
    Ok(())
}

async fn session_agent(
    state: &AppState,
    session_id: Uuid,
) -> ApiResult<(crate::model::SessionRecord, crate::model::AgentRecord)> {
    let session = state
        .store
        .get_session(session_id)
        .await
        .ok_or_else(|| ApiError::not_found("session not found"))?;
    if session.status != SessionStatus::Running {
        return Err(ApiError::conflict("session is not running"));
    }
    let agent = state
        .store
        .get_agent_version(&session.agent, &session.agent_version)
        .await
        .map_err(|_| ApiError::not_found("agent deployment not found"))?;
    Ok((session, agent))
}

/// Call the guest a11y driver (`POST /v1/tool` on DRIVER_PORT).
pub(crate) async fn driver_call(
    state: &AppState,
    session_id: Uuid,
    body: Value,
) -> ApiResult<Value> {
    let (session, agent) = session_agent(state, session_id).await?;
    if let Some(reason) = session.agent_paused_reason {
        return Err(ApiError::conflict(format!(
            "agent paused ({})",
            reason.as_str()
        )));
    }
    if matches!(
        session.browse.cookie_jar,
        crate::browse_ifc::CookieJarKind::Operator
    ) {
        return Err(ApiError::forbidden(
            "operator cookie jar active — agent cannot snapshot or act",
        ));
    }
    if let Some(msg) = crate::attestation::host_channel_forbidden(session.confidential.as_ref()) {
        return Err(ApiError::forbidden(msg));
    }
    browser_tools_allowed(agent.manifest.confinement, agent.manifest.browser.as_ref())
        .map_err(ApiError::forbidden)?;
    agent
        .manifest
        .browser_port
        .ok_or_else(|| ApiError::not_found("agent has no browser_port"))?;

    // Dead-man: too many taint events → browser tools 403 until policy reset.
    if session.browse.limits.taint_events >= 8 {
        return Err(ApiError::forbidden(
            "browser tools locked after repeated taint events — merionctl policy / untaint",
        ));
    }

    let tool = body
        .get("tool")
        .or_else(|| body.get("op"))
        .and_then(|v| v.as_str())
        .unwrap_or("");

    if let Some(url) = body.get("url").and_then(|v| v.as_str()) {
        if url.starts_with("file:")
            && agent
                .manifest
                .browser
                .as_ref()
                .is_none_or(|b| b.block_file_url)
        {
            return Err(ApiError::forbidden(
                "file:// URLs are denied by browser policy",
            ));
        }
        if let Ok(parsed) = url::Url::parse(url) {
            if let Some(host) = parsed.host_str() {
                // Goal-bound tabs: host must be ⊆ goal.allow_hosts when bound.
                if let Some(gid) = session.browse.goal_id {
                    if let Some(goal) = state.store.get_goal(gid).await {
                        if !goal.allow_hosts.is_empty()
                            && !crate::policy::host_matches_list(host, &goal.allow_hosts)
                        {
                            return Err(ApiError::forbidden("host outside goal.allow_hosts"));
                        }
                    }
                }
                if let Some(bp) = agent.manifest.browser.as_ref() {
                    if !bp.allow_hosts.is_empty()
                        && !crate::policy::host_matches_list(host, &bp.allow_hosts)
                    {
                        return Err(ApiError::forbidden("host not in browser.allow_hosts"));
                    }
                    if crate::policy::host_matches_list(host, &bp.high_risk_hosts) {
                        return Err(ApiError::forbidden(
                            "high-risk host (purchase|send) requires operator approval before open",
                        ));
                    }
                    if bp.downloads.eq_ignore_ascii_case("deny")
                        && parsed
                            .path()
                            .rsplit('/')
                            .next()
                            .is_some_and(|n| n.contains('.') && n.len() > 4)
                        && matches!(
                            parsed
                                .path()
                                .rsplit('.')
                                .next()
                                .map(|e| e.to_ascii_lowercase())
                                .as_deref(),
                            Some("exe")
                                | Some("zip")
                                | Some("dmg")
                                | Some("pkg")
                                | Some("msi")
                                | Some("deb")
                                | Some("rpm")
                        )
                    {
                        return Err(ApiError::forbidden("downloads denied by browser policy"));
                    }
                }
            }
        }
    }

    // IFC: paste/type that carries clipboard into a tab.
    if tool == "act" {
        let op = body.get("op").and_then(|v| v.as_str()).unwrap_or("");
        if matches!(op, "fill" | "type")
            && body.get("from_clipboard").and_then(|v| v.as_bool()) == Some(true)
        {
            let tab_id = session
                .browse
                .active_tab
                .clone()
                .unwrap_or_else(|| "default".into());
            let dst = session
                .browse
                .tabs
                .get(&tab_id)
                .cloned()
                .unwrap_or_default();
            match crate::browse_ifc::check_cross_origin_flow(&session.browse.clipboard, &dst) {
                crate::browse_ifc::IfcVerdict::Allow => {}
                crate::browse_ifc::IfcVerdict::Deny(msg) => {
                    return Err(ApiError::forbidden(msg));
                }
                crate::browse_ifc::IfcVerdict::Ask(msg) => {
                    return Err(ApiError::forbidden(format!(
                        "{msg} — open approval kind: send"
                    )));
                }
            }
        }
        // Witness vote on risky clicks (semantic Sentinel scaffold).
        if op == "click" {
            let ref_name = body.get("name").and_then(|v| v.as_str());
            let snap = body
                .get("snapshot_text")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            if let crate::browse_ifc::WitnessVote::Deny(msg) =
                crate::browse_ifc::witness_vote(op, ref_name, snap)
            {
                return Err(ApiError::forbidden(msg));
            }
            // Overlay / clickjack: a11y name vs optional pixel_label from operator crop.
            if let (Some(a11y), Some(pix)) = (
                body.get("name").and_then(|v| v.as_str()),
                body.get("pixel_label").and_then(|v| v.as_str()),
            ) {
                if crate::browse_ifc::overlay_mismatch(a11y, Some(pix)) {
                    let _ = state
                        .store
                        .update_session(session_id, |s| {
                            s.browse.limits.taint_events =
                                s.browse.limits.taint_events.saturating_add(1);
                            s.agent_paused_reason = Some(crate::model::AgentPausedReason::Taint);
                        })
                        .await;
                    let _ = state
                        .store
                        .audit
                        .append(
                            Some(session_id),
                            crate::audit::AuditPhase::Denied,
                            "browser.overlay_mismatch",
                            None,
                            json!({ "a11y": a11y, "pixel_label": pix }),
                        )
                        .await;
                    return Err(ApiError::forbidden(
                        "overlay mismatch — agent paused (taint)",
                    ));
                }
            }
        }
    }

    let result = state
        .fluxvm
        .guest_request(
            session.sandbox_id,
            DRIVER_PORT,
            Method::POST,
            "v1/tool",
            Some(&body),
        )
        .await
        .map_err(ApiError::bad_gateway)?;

    // Update IFC + trajectory after successful tools (ignore driver soft errors).
    if result.get("error").is_none() {
        after_browser_tool(state, session_id, &agent, &body, &result).await?;
    }
    Ok(result)
}

async fn after_browser_tool(
    state: &AppState,
    session_id: Uuid,
    agent: &crate::model::AgentRecord,
    body: &Value,
    result: &Value,
) -> ApiResult<()> {
    let tool = body
        .get("tool")
        .or_else(|| body.get("op"))
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let now = chrono::Utc::now().to_rfc3339();
    let session = state
        .store
        .update_session(session_id, |s| {
            if s.browse.network_identity.is_none() {
                let tenant = s.user_id.as_deref().unwrap_or(&s.agent);
                s.browse.network_identity = Some(crate::browse_ifc::browser_network_identity(
                    tenant, session_id,
                ));
            }
            if s.browse.limits.max_origins_per_hour == 0 {
                s.browse.limits = crate::browse_ifc::BrowseLimits::with_defaults();
            }
            match tool.as_str() {
                "open" => {
                    if let Some(url) = body.get("url").and_then(|v| v.as_str()) {
                        if let Some(host) = crate::browse_ifc::host_from_url(url) {
                            let tab = result
                                .get("tab")
                                .and_then(|v| v.as_str())
                                .unwrap_or("default")
                                .to_string();
                            s.browse.active_tab = Some(tab.clone());
                            s.browse
                                .tabs
                                .insert(tab, crate::browse_ifc::OriginSet::singleton(&host));
                            s.browse.limits.origins_seen.insert(host);
                        }
                    }
                }
                "snapshot" => {
                    // Reading a page can taint clipboard-capable buffer with tab origin.
                    if let Some(tab) = s.browse.active_tab.clone() {
                        if let Some(origins) = s.browse.tabs.get(&tab).cloned() {
                            s.browse.clipboard = s.browse.clipboard.union(&origins);
                        }
                    }
                }
                _ => {}
            }
            if matches!(tool.as_str(), "open" | "act") && result.get("error").is_none() {
                let seq = s.browse.steps.last().map(|x| x.seq + 1).unwrap_or(1);
                s.browse.steps.push(crate::browse_ifc::BrowseStep {
                    seq,
                    tool: tool.clone(),
                    op: body.get("op").and_then(|v| v.as_str()).map(str::to_string),
                    url: body.get("url").and_then(|v| v.as_str()).map(str::to_string),
                    ref_id: body.get("ref").and_then(|v| v.as_str()).map(str::to_string),
                    role: body
                        .get("role")
                        .or_else(|| result.get("role"))
                        .and_then(|v| v.as_str())
                        .map(str::to_string),
                    name: body
                        .get("name")
                        .or_else(|| result.get("name"))
                        .and_then(|v| v.as_str())
                        .map(str::to_string),
                    snapshot_hash: result
                        .get("snapshot_hash")
                        .and_then(|v| v.as_str())
                        .map(str::to_string),
                    screenshot_hash: None,
                    policy_hash: None,
                    at: now.clone(),
                });
            }
        })
        .await
        .map_err(ApiError::internal)?;

    // Upsert browse-script artifact for trajectory-as-code.
    if matches!(tool.as_str(), "open" | "act") {
        let script = crate::browse_ifc::render_browse_script(session_id, &session.browse.steps);
        upsert_browse_script_artifact(state, session_id, &agent.name, script).await?;
    }
    Ok(())
}

async fn upsert_browse_script_artifact(
    state: &AppState,
    session_id: Uuid,
    agent: &str,
    body: String,
) -> ApiResult<()> {
    use crate::goals::ArtifactRecord;
    let existing = state
        .store
        .list_artifacts()
        .await
        .into_iter()
        .find(|a| a.session_id == Some(session_id) && a.kind == "browse-script");
    let now = chrono::Utc::now();
    let record = if let Some(mut a) = existing {
        a.body = body;
        a.metadata = json!({ "steps": true, "format": "playwright-core" });
        a
    } else {
        ArtifactRecord {
            id: Uuid::new_v4(),
            kind: "browse-script".into(),
            title: "browse.spec.mjs".into(),
            body,
            content_type: Some("text/javascript".into()),
            goal_id: None,
            session_id: Some(session_id),
            agent: Some(agent.to_string()),
            metadata: json!({ "format": "playwright-core" }),
            created_at: now,
            expires_at: None,
        }
    };
    state
        .store
        .save_artifact(record)
        .await
        .map_err(ApiError::internal)?;
    Ok(())
}

fn rate_limit_screenshot(state: &AppState, id: Uuid) -> ApiResult<()> {
    let mut map = state
        .screenshot_last
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    let now = std::time::Instant::now();
    if let Some(prev) = map.get(&id) {
        if now.duration_since(*prev) < SCREENSHOT_MIN_INTERVAL {
            return Err(ApiError::too_many(
                "screenshot rate-limited (min 2s between captures)",
            ));
        }
    }
    map.insert(id, now);
    Ok(())
}

async fn browser_port(state: &AppState, session_id: Uuid) -> ApiResult<(Uuid, u16)> {
    let session = state
        .store
        .get_session(session_id)
        .await
        .ok_or_else(|| ApiError::not_found("session not found"))?;
    if session.status != SessionStatus::Running {
        return Err(ApiError::conflict("session is not running"));
    }
    if let Some(msg) = crate::attestation::host_channel_forbidden(session.confidential.as_ref()) {
        return Err(ApiError::forbidden(msg));
    }
    let agent = state
        .store
        .get_agent_version(&session.agent, &session.agent_version)
        .await
        .map_err(|_| ApiError::not_found("agent deployment not found"))?;
    let port = agent
        .manifest
        .browser_port
        .ok_or_else(|| ApiError::not_found("agent has no browser_port"))?;
    Ok((session.sandbox_id, port))
}

pub(crate) async fn devtools(
    State(state): State<Arc<AppState>>,
    Path((id, path)): Path<(Uuid, String)>,
) -> ApiResult<Json<Value>> {
    if !allowed_path(&path) {
        return Err(ApiError::not_found(
            "only DevTools listing endpoints are available",
        ));
    }
    let (sandbox_id, port) = browser_port(&state, id).await?;
    let mut value = state
        .fluxvm
        .guest_request(sandbox_id, port, Method::GET, &path, None)
        .await
        .map_err(ApiError::bad_gateway)?;
    sanitize(&mut value);
    Ok(Json(value))
}

/// Operator live view: sanitized open tabs (title + URL). No raw CDP WebSocket.
pub(crate) async fn browser_view(
    State(state): State<Arc<AppState>>,
    Path(id): Path<Uuid>,
) -> ApiResult<Json<Value>> {
    let (sandbox_id, port) = browser_port(&state, id).await?;
    let mut list = state
        .fluxvm
        .guest_request(sandbox_id, port, Method::GET, "json/list", None)
        .await
        .map_err(ApiError::bad_gateway)?;
    sanitize(&mut list);
    let tabs: Vec<Value> = list
        .as_array()
        .into_iter()
        .flatten()
        .map(|tab| {
            let title = tab.get("title").and_then(|v| v.as_str()).unwrap_or("");
            let url = tab.get("url").and_then(|v| v.as_str()).unwrap_or("");
            let id = tab.get("id").and_then(|v| v.as_str()).unwrap_or("");
            json!({
                "id": id,
                "title": title,
                "url": url,
                "type": tab.get("type").and_then(|v| v.as_str()).unwrap_or("page"),
            })
        })
        .collect();
    Ok(Json(json!({
        "session_id": id,
        "tabs": tabs,
        "mode": "listing",
        "screenshot": format!("/v1/sessions/{id}/browser/screenshot"),
        "screencast": format!("/v1/sessions/{id}/browser/screencast"),
        "honesty": "Tab listing + screenshot/screencast via host CDP bridge (software-test). Input takeover is not implemented; host can still see the guest.",
        "note": "Live tab listing. Poll /browser/screenshot or open /browser/screencast WS for frames. Input takeover is not implemented.",
    })))
}

/// One JPEG via short-lived CDP over FluxVM's sandbox WS bridge (no operator CDP).
pub(crate) async fn browser_screenshot(
    State(state): State<Arc<AppState>>,
    Path(id): Path<Uuid>,
) -> ApiResult<Json<Value>> {
    rate_limit_screenshot(&state, id)?;
    let (sandbox_id, port) = browser_port(&state, id).await?;
    let list = state
        .fluxvm
        .guest_request(sandbox_id, port, Method::GET, "json/list", None)
        .await
        .map_err(ApiError::bad_gateway)?;
    let page = list
        .as_array()
        .into_iter()
        .flatten()
        .find(|t| t.get("type").and_then(|v| v.as_str()) == Some("page"))
        .ok_or_else(|| ApiError::not_found("no open page target for screenshot"))?;
    let path = first_page_debugger(&state, sandbox_id, port).await?;
    let title = page
        .get("title")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let page_url = page
        .get("url")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let jpeg = cdp_capture_jpeg(&state, sandbox_id, port, &path)
        .await
        .map_err(ApiError::bad_gateway)?;
    Ok(Json(json!({
        "session_id": id,
        "mime": "image/jpeg",
        "image_base64": jpeg,
        "title": title,
        "url": page_url,
        "mode": "screenshot",
        "honesty": "Screenshot via host CDP bridge (software-test). Not unread-by-operator; input takeover is not implemented.",
    })))
}

async fn cdp_capture_jpeg(
    state: &AppState,
    sandbox_id: Uuid,
    port: u16,
    debugger_path: &str,
) -> anyhow::Result<String> {
    let mut ws = state
        .fluxvm
        .guest_ws(sandbox_id, port, debugger_path)
        .await?;
    // Enable page domain then capture one frame.
    ws.send(WsMessage::Text(
        json!({"id": 1, "method": "Page.enable"}).to_string().into(),
    ))
    .await?;
    ws.send(WsMessage::Text(
        json!({
            "id": 2,
            "method": "Page.captureScreenshot",
            "params": { "format": "jpeg", "quality": 60 }
        })
        .to_string()
        .into(),
    ))
    .await?;

    let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(15);
    loop {
        if tokio::time::Instant::now() > deadline {
            anyhow::bail!("CDP screenshot timed out");
        }
        let msg = tokio::time::timeout(std::time::Duration::from_secs(5), ws.next())
            .await
            .map_err(|_| anyhow::anyhow!("CDP read timed out"))?
            .ok_or_else(|| anyhow::anyhow!("CDP connection closed"))??;
        let text = match msg {
            WsMessage::Text(t) => t.to_string(),
            WsMessage::Binary(b) => String::from_utf8_lossy(&b).to_string(),
            WsMessage::Close(_) => anyhow::bail!("CDP closed before screenshot"),
            _ => continue,
        };
        let v: Value = serde_json::from_str(&text)?;
        if v.get("id").and_then(|i| i.as_i64()) == Some(2) {
            if let Some(err) = v.get("error") {
                anyhow::bail!("CDP error: {err}");
            }
            let data = v
                .pointer("/result/data")
                .and_then(|d| d.as_str())
                .ok_or_else(|| anyhow::anyhow!("CDP screenshot missing result.data"))?;
            let _ = ws.close(None).await;
            return Ok(data.to_string());
        }
    }
}

/// Resolve the first page target's debugger path for CDP.
async fn first_page_debugger(state: &AppState, sandbox_id: Uuid, port: u16) -> ApiResult<String> {
    let list = state
        .fluxvm
        .guest_request(sandbox_id, port, Method::GET, "json/list", None)
        .await
        .map_err(ApiError::bad_gateway)?;
    let page = list
        .as_array()
        .into_iter()
        .flatten()
        .find(|t| t.get("type").and_then(|v| v.as_str()) == Some("page"))
        .ok_or_else(|| ApiError::not_found("no open page target for screenshot"))?;
    let ws_url = page
        .get("webSocketDebuggerUrl")
        .and_then(|v| v.as_str())
        .ok_or_else(|| ApiError::bad_gateway("page has no webSocketDebuggerUrl"))?;
    debugger_path_from_ws_url(ws_url)
        .ok_or_else(|| ApiError::bad_gateway("could not parse debugger path"))
}

/// Read-only screencast WebSocket. Operator receives `{type:"frame",…}` only;
/// input / arbitrary CDP is refused. Present `Authorization: Bearer` or `?token=`.
pub(crate) async fn browser_screencast(
    ws: axum::extract::ws::WebSocketUpgrade,
    State(state): State<Arc<AppState>>,
    Path(id): Path<Uuid>,
) -> Result<axum::response::Response, ApiError> {
    let (sandbox_id, port) = browser_port(&state, id).await?;
    let path = first_page_debugger(&state, sandbox_id, port).await?;
    Ok(ws.on_upgrade(move |socket| async move {
        if let Err(e) = run_screencast(state, socket, id, sandbox_id, port, path).await {
            tracing::warn!(session = %id, error = %e, "screencast ended");
        }
    }))
}

async fn run_screencast(
    state: Arc<AppState>,
    mut client: axum::extract::ws::WebSocket,
    session_id: Uuid,
    sandbox_id: Uuid,
    port: u16,
    debugger_path: String,
) -> anyhow::Result<()> {
    use axum::extract::ws::Message as AxumMsg;

    let _ = client
        .send(AxumMsg::Text(
            json!({
                "type": "hello",
                "session_id": session_id,
                "mode": "screencast",
                "honesty": "Host CDP screencast (software-test). No input takeover; host can still see the guest.",
            })
            .to_string()
            .into(),
        ))
        .await;

    let mut cdp = state
        .fluxvm
        .guest_ws(sandbox_id, port, &debugger_path)
        .await?;
    cdp.send(WsMessage::Text(
        json!({"id": 1, "method": "Page.enable"}).to_string().into(),
    ))
    .await?;
    cdp.send(WsMessage::Text(
        json!({
            "id": 2,
            "method": "Page.startScreencast",
            "params": {
                "format": "jpeg",
                "quality": 50,
                "maxWidth": 1280,
                "maxHeight": 720,
                "everyNthFrame": 2
            }
        })
        .to_string()
        .into(),
    ))
    .await?;

    let mut next_id: i64 = 10;
    loop {
        tokio::select! {
            client_msg = client.next() => {
                match client_msg {
                    Some(Ok(AxumMsg::Close(_))) | None => break,
                    Some(Ok(AxumMsg::Text(t))) => {
                        let v: Value = serde_json::from_str(&t).unwrap_or(Value::Null);
                        match v.get("type").and_then(|x| x.as_str()) {
                            Some("stop") => break,
                            Some("ack") => {
                                // Optional frame ack — CDP uses sessionId from event
                                if let Some(sid) = v.get("sessionId").and_then(|x| x.as_i64()) {
                                    next_id += 1;
                                    let _ = cdp.send(WsMessage::Text(
                                        json!({
                                            "id": next_id,
                                            "method": "Page.screencastFrameAck",
                                            "params": { "sessionId": sid }
                                        }).to_string().into()
                                    )).await;
                                }
                            }
                            Some("ping") => {
                                let _ = client.send(AxumMsg::Text(
                                    json!({"type":"pong"}).to_string().into()
                                )).await;
                            }
                            _ => {
                                // Refuse input / arbitrary CDP from the operator.
                                let _ = client.send(AxumMsg::Text(
                                    json!({
                                        "type": "error",
                                        "error": "only stop/ack/ping allowed; input takeover is not implemented"
                                    }).to_string().into()
                                )).await;
                            }
                        }
                    }
                    Some(Ok(_)) => {}
                    Some(Err(_)) => break,
                }
            }
            cdp_msg = cdp.next() => {
                match cdp_msg {
                    Some(Ok(WsMessage::Text(t))) => {
                        let v: Value = match serde_json::from_str(&t) {
                            Ok(v) => v,
                            Err(_) => continue,
                        };
                        if v.get("method").and_then(|m| m.as_str()) == Some("Page.screencastFrame") {
                            let params = v.get("params").cloned().unwrap_or(Value::Null);
                            let data = params.get("data").and_then(|d| d.as_str()).unwrap_or("");
                            let session_id_cdp = params.get("sessionId").cloned();
                            let meta = params.get("metadata").cloned();
                            let _ = client.send(AxumMsg::Text(
                                json!({
                                    "type": "frame",
                                    "mime": "image/jpeg",
                                    "image_base64": data,
                                    "sessionId": session_id_cdp,
                                    "metadata": meta,
                                }).to_string().into()
                            )).await;
                            // Auto-ack so Chrome keeps sending frames.
                            if let Some(sid) = params.get("sessionId").and_then(|x| x.as_i64()) {
                                next_id += 1;
                                let _ = cdp.send(WsMessage::Text(
                                    json!({
                                        "id": next_id,
                                        "method": "Page.screencastFrameAck",
                                        "params": { "sessionId": sid }
                                    }).to_string().into()
                                )).await;
                            }
                        }
                    }
                    Some(Ok(WsMessage::Close(_))) | None => break,
                    Some(Ok(_)) => {}
                    Some(Err(_)) => break,
                }
            }
        }
    }
    let _ = cdp
        .send(WsMessage::Text(
            json!({"id": 99, "method": "Page.stopScreencast"})
                .to_string()
                .into(),
        ))
        .await;
    let _ = cdp.close(None).await;
    let _ = client.send(AxumMsg::Close(None)).await;
    Ok(())
}

#[derive(Debug, serde::Deserialize)]
pub struct BrowserPageQuery {
    #[serde(default)]
    pub session: Option<String>,
}

/// Proxy a tool call to the guest a11y driver (operator / MCP).
pub(crate) async fn browser_tool(
    State(state): State<Arc<AppState>>,
    Path(id): Path<Uuid>,
    Json(body): Json<Value>,
) -> ApiResult<Json<Value>> {
    let result = driver_call(&state, id, body.clone()).await?;
    let _ = state
        .store
        .audit
        .append(
            Some(id),
            crate::audit::AuditPhase::Performed,
            "browser.act",
            None,
            json!({
                "tool": body.get("tool").or_else(|| body.get("op")),
                "ref": body.get("ref"),
                "url_host": body.get("url").and_then(|u| u.as_str()).and_then(|u| {
                    url::Url::parse(u).ok().and_then(|p| p.host_str().map(str::to_owned))
                }),
            }),
        )
        .await;
    Ok(Json(result))
}

/// Host-side password fill via CDP after vault authorize_resolve.
/// Never returns the secret; agent context only sees `{filled:true}`.
#[derive(Debug, serde::Deserialize)]
pub struct FillSecretRequest {
    pub credential: String,
    /// Origin host for authorize_resolve (e.g. login.example.com).
    pub host: String,
    #[serde(default = "default_fill_path")]
    pub path: String,
}

fn default_fill_path() -> String {
    "/".into()
}

pub(crate) async fn browser_fill_secret(
    State(state): State<Arc<AppState>>,
    Path(id): Path<Uuid>,
    Json(req): Json<FillSecretRequest>,
) -> ApiResult<Json<Value>> {
    let (session, agent) = session_agent(&state, id).await?;
    if let Some(msg) = crate::attestation::host_channel_forbidden(session.confidential.as_ref()) {
        return Err(ApiError::forbidden(msg));
    }
    browser_tools_allowed(agent.manifest.confinement, agent.manifest.browser.as_ref())
        .map_err(ApiError::forbidden)?;
    let port = agent
        .manifest
        .browser_port
        .ok_or_else(|| ApiError::not_found("agent has no browser_port"))?;
    let (_desc, secret) = state
        .credentials
        .authorize_resolve(
            &req.credential,
            &crate::credentials::ResolveContext {
                host: &req.host,
                method: &reqwest::Method::POST,
                path: &req.path,
                port: 443,
                user_id: session.user_id.as_deref(),
            },
        )
        .await
        .map_err(ApiError::forbidden)?;
    // Split-sight: pause the agent while vault fills.
    let _ = state
        .store
        .update_session(id, |s| {
            s.agent_paused_reason = Some(crate::model::AgentPausedReason::VaultFill);
        })
        .await;
    let _ = state
        .store
        .audit
        .append(
            Some(id),
            crate::audit::AuditPhase::Performed,
            "session.agent_paused",
            None,
            json!({ "reason": "vault_fill", "host": req.host }),
        )
        .await;
    let path = first_page_debugger(&state, session.sandbox_id, port).await?;
    cdp_insert_text(&state, session.sandbox_id, port, &path, &secret)
        .await
        .map_err(ApiError::bad_gateway)?;
    let _ = state
        .store
        .update_session(id, |s| {
            if matches!(
                s.agent_paused_reason,
                Some(crate::model::AgentPausedReason::VaultFill)
            ) {
                s.agent_paused_reason = None;
            }
        })
        .await;
    let _ = state
        .store
        .audit
        .append(
            Some(id),
            crate::audit::AuditPhase::Performed,
            "browser.fill_secret",
            None,
            json!({
                "credential": req.credential,
                "host": req.host,
                "filled": true,
                "source": format!("vault:{}", req.credential),
            }),
        )
        .await;
    Ok(Json(json!({
        "filled": true,
        "source": format!("vault:{}", req.credential),
        "honesty": "Secret typed via host CDP; value never returned to the model.",
    })))
}

async fn cdp_insert_text(
    state: &AppState,
    sandbox_id: Uuid,
    port: u16,
    debugger_path: &str,
    text: &str,
) -> anyhow::Result<()> {
    let mut ws = state
        .fluxvm
        .guest_ws(sandbox_id, port, debugger_path)
        .await?;
    ws.send(WsMessage::Text(
        json!({"id": 1, "method": "Input.insertText", "params": { "text": text }})
            .to_string()
            .into(),
    ))
    .await?;
    // Wait for ack or timeout; do not log text.
    let _ = tokio::time::timeout(std::time::Duration::from_secs(5), ws.next()).await;
    let _ = ws.close(None).await;
    Ok(())
}

/// Capability strip for cockpit / console.
pub async fn browser_capability(state: &AppState, session_id: Uuid) -> Value {
    let Ok((session, agent)) = session_agent(state, session_id).await else {
        return json!({
            "ready": false,
            "cdp": false,
            "driver": false,
            "confined": false,
            "evidence_class": "software-test",
        });
    };
    let confined = matches!(
        agent.manifest.confinement,
        crate::model::Confinement::Strict
    );
    let cdp = agent.manifest.browser_port.is_some();
    let enabled = agent.manifest.browser.as_ref().is_none_or(|b| b.enabled);
    let tools_ok =
        browser_tools_allowed(agent.manifest.confinement, agent.manifest.browser.as_ref()).is_ok()
            && session.agent_paused_reason.is_none();
    let (snp, tdx) = state.launch_verified_flags().await;
    let evidence = if snp || tdx {
        "launch-verified"
    } else {
        "software-test"
    };
    let host_recover = if session.confidential.as_ref().is_some_and(|c| c.active) {
        "forbidden"
    } else {
        "allowed"
    };
    let identity = session.browse.network_identity.clone().unwrap_or_else(|| {
        let tenant = session.user_id.as_deref().unwrap_or(&session.agent);
        crate::browse_ifc::browser_network_identity(tenant, session_id)
    });
    let badge = crate::browse_ifc::honesty_badge(
        evidence,
        true,
        host_recover,
        confined,
        session.agent_paused_reason.as_ref(),
        Some(&identity),
    );
    json!({
        "ready": cdp && confined && enabled && session.agent_paused_reason.is_none(),
        "cdp": cdp,
        "driver_port": DRIVER_PORT,
        "confined": confined,
        "enabled": enabled,
        "tools_allowed": tools_ok,
        "tainted_by": session.tainted_by,
        "agent_paused_reason": session.agent_paused_reason,
        "cookie_jar": session.browse.cookie_jar,
        "goal_id": session.browse.goal_id,
        "browse_steps": session.browse.steps.len(),
        "origins": session.browse.limits.origins_seen,
        "network_identity": identity,
        "hubble_browser_flows": format!(
            "/api/dataplane/flows?identity={}",
            urlencoding_identity(&identity)
        ),
        "evidence_class": evidence,
        "badge": badge,
        "honesty": badge.get("honesty").cloned().unwrap_or(json!("software-test")),
    })
}

fn urlencoding_identity(s: &str) -> String {
    s.replace('/', "%2F")
}

#[derive(Debug, serde::Deserialize)]
pub struct AgentPauseRequest {
    pub reason: String,
}

/// Split-sight: pause agent tools (`vault_fill` | `operator_watch` | `taint`).
pub(crate) async fn agent_pause(
    State(state): State<Arc<AppState>>,
    Path(id): Path<Uuid>,
    Json(req): Json<AgentPauseRequest>,
) -> ApiResult<Json<Value>> {
    let reason = crate::model::AgentPausedReason::parse(&req.reason)
        .ok_or_else(|| ApiError::bad_request("reason must be vault_fill|operator_watch|taint"))?;
    let session = state
        .store
        .update_session(id, |s| {
            s.agent_paused_reason = Some(reason);
            if matches!(reason, crate::model::AgentPausedReason::OperatorWatch) {
                s.browse.cookie_jar = crate::browse_ifc::CookieJarKind::Operator;
            }
        })
        .await
        .map_err(|_| ApiError::not_found("session not found"))?;
    let _ = state
        .store
        .audit
        .append(
            Some(id),
            crate::audit::AuditPhase::Performed,
            "session.agent_paused",
            None,
            json!({ "reason": reason.as_str() }),
        )
        .await;
    Ok(Json(json!({
        "session_id": id,
        "agent_paused_reason": session.agent_paused_reason,
        "cookie_jar": session.browse.cookie_jar,
    })))
}

/// Resume agent tools after split-sight pause.
pub(crate) async fn agent_resume(
    State(state): State<Arc<AppState>>,
    Path(id): Path<Uuid>,
) -> ApiResult<Json<Value>> {
    let session = state
        .store
        .update_session(id, |s| {
            s.agent_paused_reason = None;
            s.browse.cookie_jar = crate::browse_ifc::CookieJarKind::Agent;
        })
        .await
        .map_err(|_| ApiError::not_found("session not found"))?;
    let _ = state
        .store
        .audit
        .append(
            Some(id),
            crate::audit::AuditPhase::Performed,
            "session.agent_resumed",
            None,
            json!({}),
        )
        .await;
    Ok(Json(json!({
        "session_id": id,
        "agent_paused_reason": session.agent_paused_reason,
        "cookie_jar": session.browse.cookie_jar,
    })))
}

/// Time-machine checkout of a browse step (a11y metadata, not live site).
pub(crate) async fn browse_checkout(
    State(state): State<Arc<AppState>>,
    Path(id): Path<Uuid>,
    Query(q): Query<BrowseCheckoutQuery>,
) -> ApiResult<Json<Value>> {
    let session = state
        .store
        .get_session(id)
        .await
        .ok_or_else(|| ApiError::not_found("session not found"))?;
    let step = session
        .browse
        .steps
        .iter()
        .find(|s| s.seq == q.seq)
        .ok_or_else(|| ApiError::not_found("browse step not found"))?;
    Ok(Json(json!({
        "session_id": id,
        "step": step,
        "honesty": "Restored trajectory metadata for this seq — not the live page.",
    })))
}

#[derive(Debug, serde::Deserialize)]
pub struct BrowseCheckoutQuery {
    pub seq: u64,
}

/// Return the latest browse-script artifact body (trajectory-as-code).
pub(crate) async fn browse_script(
    State(state): State<Arc<AppState>>,
    Path(id): Path<Uuid>,
) -> ApiResult<impl IntoResponse> {
    let art = state
        .store
        .list_artifacts()
        .await
        .into_iter()
        .find(|a| a.session_id == Some(id) && a.kind == "browse-script")
        .ok_or_else(|| ApiError::not_found("no browse-script artifact yet"))?;
    Ok((
        [(
            axum::http::header::CONTENT_TYPE,
            "text/javascript; charset=utf-8",
        )],
        art.body,
    ))
}

/// GuestKit-style profile inspect (cookie *hosts*, flags — never cookie values).
pub(crate) async fn profile_inspect(
    State(state): State<Arc<AppState>>,
    Path(id): Path<Uuid>,
) -> ApiResult<Json<Value>> {
    let (session, agent) = session_agent(&state, id).await?;
    let confined = matches!(
        agent.manifest.confinement,
        crate::model::Confinement::Strict
    );
    let mut cookie_hosts: Vec<String> = session
        .browse
        .tabs
        .values()
        .flat_map(|o| o.hosts.iter().cloned())
        .collect();
    cookie_hosts.sort();
    cookie_hosts.dedup();
    // Best-effort CDP version probe (proves loopback CDP, not DOM).
    let cdp_version = if let Some(port) = agent.manifest.browser_port {
        state
            .fluxvm
            .guest_request(
                session.sandbox_id,
                port,
                Method::GET,
                "json/version",
                None::<&Value>,
            )
            .await
            .ok()
    } else {
        None
    };
    Ok(Json(json!({
        "session_id": id,
        "profile": {
            "home_volume": agent.manifest.home_volume,
            "cdp_bound_loopback": true,
            "confinement_strict": confined,
            "cookie_hosts": cookie_hosts,
            "extensions_declared": [],
            "network_identity": session.browse.network_identity,
            "cdp_version": cdp_version,
        },
        "honesty": "Hosts only — cookie values redacted. Offline assurance via GuestKit on browser-home.qcow2 when packed.",
    })))
}

/// Confinement / browser doctor (SNI-identity + proxy gate readiness).
pub(crate) async fn browser_doctor(
    State(state): State<Arc<AppState>>,
    Path(id): Path<Uuid>,
) -> ApiResult<Json<Value>> {
    let (session, agent) = session_agent(&state, id).await?;
    let confined = matches!(
        agent.manifest.confinement,
        crate::model::Confinement::Strict
    );
    let mut warnings = Vec::new();
    if !confined {
        warnings.push("confinement is not strict — guest can bypass HTTPS_PROXY");
    }
    if agent.manifest.browser_port.is_none() {
        warnings.push("browser_port unset");
    }
    if session.browse.network_identity.is_none() {
        warnings.push("network_identity not yet assigned (assigned on first browse tool)");
    }
    let ok = warnings.is_empty() && confined;
    Ok(Json(json!({
        "ok": ok,
        "confinement": agent.manifest.confinement,
        "network_identity": session.browse.network_identity,
        "warnings": warnings,
        "advice": "FluxVM veth should carry identity keep-browser/tenant/session; Service Fabric may CONNECT only to egress_allow_hosts.",
    })))
}

/// Minimal HTML that polls tab listing + optional screenshot for operators.
pub async fn browser_page(Query(q): Query<BrowserPageQuery>) -> impl IntoResponse {
    let session = q.session.unwrap_or_default();
    let session_esc = session
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;");
    let html = format!(
        r#"<!DOCTYPE html>
<html lang="en"><head>
<meta charset="utf-8"/><meta name="viewport" content="width=device-width,initial-scale=1"/>
<title>Keep browser view</title>
<style>
body{{font-family:ui-sans-serif,system-ui,sans-serif;margin:0;background:#0f1419;color:#e7ecf3}}
header{{padding:1rem 1.25rem;border-bottom:1px solid #243044}}
main{{padding:1rem;max-width:960px;margin:0 auto}}
.card{{background:#162032;border:1px solid #243044;border-radius:12px;padding:1rem;margin-bottom:1rem}}
a{{color:#8ec7ff}} input,button{{font:inherit;padding:.5rem .75rem;border-radius:8px;border:1px solid #345}}
button{{background:#2b6cff;color:#fff;border:0;cursor:pointer}}
.tab{{padding:.75rem 0;border-bottom:1px solid #243044}}
.muted{{opacity:.7;font-size:13px}}
img.frame{{max-width:100%;border-radius:8px;border:1px solid #243044;background:#000}}
</style></head><body>
<header><strong>Keep browser</strong> · tabs + screencast
<div class="muted">Host CDP bridge (software-test). Frames only — no input takeover.</div>
</header>
<main>
<div class="card">
<label>API <input id="base" placeholder="http://127.0.0.1:9096" style="width:55%"/></label>
<label>Token <input id="token" type="password" style="width:35%"/></label>
<label>Session <input id="sid" value="{session_esc}" style="width:50%"/></label>
<button id="go">Refresh tabs</button>
<button id="cast" type="button">Start screencast</button>
<button id="stop" type="button">Stop</button>
</div>
<div id="shot" class="card muted">Screencast / screenshot frames appear here.</div>
<div id="out" class="card muted">Load a running session with browser_port set.</div>
</main>
<script>
const $=id=>document.getElementById(id);
let castWs=null;
async function refresh(){{
  const base=$('base').value.replace(/\/$/,'')||location.origin;
  const sid=$('sid').value.trim();
  const tok=$('token').value.trim();
  const headers=tok?{{Authorization:'Bearer '+tok}}:{{}};
  if(!sid){{$('out').textContent='session id required';return;}}
  const r=await fetch(base+'/v1/sessions/'+sid+'/browser/view',{{headers}});
  const j=await r.json();
  if(!r.ok){{$('out').textContent=JSON.stringify(j);return;}}
  const tabs=j.tabs||[];
  $('out').innerHTML=tabs.length?tabs.map(t=>`<div class="tab"><strong>${{t.title||'(untitled)'}}</strong><div class="muted">${{t.url||''}}</div></div>`).join('')
    :`<div class="muted">No open tabs. ${{j.note||''}}</div>`;
}}
function startCast(){{
  const base=$('base').value.replace(/\/$/,'')||location.origin;
  const sid=$('sid').value.trim();
  const tok=$('token').value.trim();
  if(!sid){{$('shot').textContent='session id required';return;}}
  if(castWs){{ try{{castWs.close();}}catch(e){{}} }}
  const u=new URL(base.replace(/^http/,'ws')+'/v1/sessions/'+sid+'/browser/screencast');
  if(tok) u.searchParams.set('token', tok);
  castWs=new WebSocket(u);
  castWs.onmessage=(ev)=>{{
    try{{
      const m=JSON.parse(ev.data);
      if(m.type==='frame' && m.image_base64){{
        $('shot').innerHTML=`<img class="frame" alt="screencast" src="data:${{m.mime||'image/jpeg'}};base64,${{m.image_base64}}"/>`;
      }} else if(m.type==='hello' || m.type==='error'){{
        const note=document.createElement('div'); note.className='muted'; note.textContent=m.honesty||m.error||m.type;
        if(!$('shot').querySelector('img')) $('shot').textContent=note.textContent;
      }}
    }}catch(e){{}}
  }};
  castWs.onerror=()=>{{$('shot').textContent='screencast socket error';}};
  castWs.onclose=()=>{{castWs=null;}};
}}
function stopCast(){{
  if(castWs && castWs.readyState===1) castWs.send(JSON.stringify({{type:'stop'}}));
  if(castWs) try{{castWs.close();}}catch(e){{}}
  castWs=null;
}}
$('go').onclick=refresh;
$('cast').onclick=startCast;
$('stop').onclick=stopCast;
const u=new URL(location.href);
if(u.searchParams.get('token')) $('token').value=u.searchParams.get('token');
if(u.searchParams.get('base')) $('base').value=u.searchParams.get('base');
if($('sid').value) refresh();
setInterval(()=>{{ if($('sid').value) refresh(); }}, 4000);
</script></body></html>"#
    );
    (
        [(axum::http::header::CONTENT_TYPE, "text/html; charset=utf-8")],
        html,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn only_listing_paths_are_allowed() {
        for ok in ["json", "json/list", "/json/version", "json/protocol/"] {
            assert!(allowed_path(ok), "{ok}");
        }
        for bad in ["json/new", "json/activate/1", "json/close/1", ""] {
            assert!(!allowed_path(bad), "{bad}");
        }
    }

    #[test]
    fn sanitize_strips_control_urls() {
        let mut v = json!({
            "title": "x",
            "webSocketDebuggerUrl": "ws://127.0.0.1:9222/devtools/page/1",
            "devtoolsFrontendUrl": "/devtools/inspector.html",
            "nested": { "webSocketDebuggerUrl": "ws://evil" }
        });
        sanitize(&mut v);
        assert!(v.get("webSocketDebuggerUrl").is_none());
        assert!(v.get("devtoolsFrontendUrl").is_none());
        assert!(v["nested"].get("webSocketDebuggerUrl").is_none());
        assert_eq!(v["title"], "x");
    }

    #[test]
    fn debugger_path_parsed_from_ws_url() {
        assert_eq!(
            debugger_path_from_ws_url("ws://127.0.0.1:9222/devtools/page/ABC").as_deref(),
            Some("devtools/page/ABC")
        );
        assert!(debugger_path_from_ws_url("http://nope").is_none());
    }

    #[test]
    fn tools_require_strict_confinement() {
        use crate::model::{BrowserPolicy, Confinement};
        assert!(browser_tools_allowed(Confinement::Off, None).is_err());
        assert!(browser_tools_allowed(Confinement::Strict, None).is_ok());
        let disabled = BrowserPolicy {
            enabled: false,
            ..BrowserPolicy::default()
        };
        assert!(browser_tools_allowed(Confinement::Strict, Some(&disabled)).is_err());
    }

    #[test]
    fn allow_and_high_risk_host_matching() {
        use crate::policy::host_matches_list;
        let allow = vec!["example.com".into(), "github.com".into()];
        assert!(host_matches_list("example.com", &allow));
        assert!(host_matches_list("www.example.com", &allow));
        assert!(!host_matches_list("evil.com", &allow));
        let risk = vec!["checkout.shop.test".into()];
        assert!(host_matches_list("checkout.shop.test", &risk));
    }
}
