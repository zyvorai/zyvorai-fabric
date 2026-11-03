// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: Apache-2.0

//! A read-mostly view across several Fabric daemons.
//!
//! An operator registers other daemons ("peers"). `GET /federation/overview` asks each one, in
//! parallel and with a short timeout, for its hosts and VM count, using the peer's read token, and
//! returns one inventory. A peer that is down, refuses the token or answers badly shows up as such
//! in its own row and never fails the rest.
//!
//! Writes are not done with a stored credential. Putting a peer host into or out of maintenance
//! forwards the *caller's own* `Authorization` header to that peer, so the peer decides, with its
//! own RBAC, whether this person may. A peer that does not share this daemon's token trust refuses it.
//!
//! The peer's read token lives in the secrets manager, which is in memory: after a restart a peer
//! shows `token_missing` until its token is set again (`PUT /federation/peers/{id}/token`).
//! Registering a peer makes this daemon call that URL, so it is admin-only, takes `http` or `https`
//! only, refuses link-local and unspecified literal addresses, and never follows a redirect.

use axum::{
    extract::{Path, State},
    http::{header::AUTHORIZATION, HeaderMap, StatusCode},
    response::IntoResponse,
    Json,
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{
    net::IpAddr,
    sync::{Arc, OnceLock},
    time::{Duration, Instant},
};
use uuid::Uuid;

use crate::server::AppState;
use security::{RequireAdmin, RequireRead};

const COLLECTION: &str = "federation_peers";
const PEER_TIMEOUT: Duration = Duration::from_secs(5);
const MAX_PEERS: usize = 64;
const MAX_BODY: usize = 4 * 1024 * 1024;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Peer {
    pub id: String,
    pub name: String,
    /// Origin only: `scheme://host[:port]`.
    pub url: String,
    #[serde(default)]
    pub token_secret_id: Option<String>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Deserialize)]
pub struct RegisterPeerRequest {
    pub name: String,
    pub url: String,
    #[serde(default)]
    pub token: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct SetTokenRequest {
    pub token: String,
}

fn error(status: StatusCode, message: impl Into<String>) -> axum::response::Response {
    (status, Json(json!({"error": message.into()}))).into_response()
}

/// One client for all peer calls: short timeout, and a redirect is an answer, not a detour.
fn client() -> &'static reqwest::Client {
    static CLIENT: OnceLock<reqwest::Client> = OnceLock::new();
    CLIENT.get_or_init(|| {
        reqwest::Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .timeout(PEER_TIMEOUT)
            .build()
            .unwrap_or_default()
    })
}

/// Check a peer URL and reduce it to its origin.
pub fn validate_peer_url(raw: &str) -> Result<String, String> {
    let url = reqwest::Url::parse(raw.trim()).map_err(|_| "not a valid URL".to_string())?;
    if !matches!(url.scheme(), "http" | "https") {
        return Err("the URL must be http or https".into());
    }
    if !url.username().is_empty() || url.password().is_some() {
        return Err("the URL must not carry a user name or password".into());
    }
    if url.query().is_some() || url.fragment().is_some() {
        return Err("the URL must not have a query or fragment".into());
    }
    let host = url
        .host_str()
        .ok_or_else(|| "the URL has no host".to_string())?;
    let ip = host
        .trim_start_matches('[')
        .trim_end_matches(']')
        .parse::<IpAddr>()
        .ok();
    if let Some(ip) = ip {
        let link_local = match ip {
            IpAddr::V4(a) => a.is_link_local(),
            IpAddr::V6(a) => (a.segments()[0] & 0xffc0) == 0xfe80,
        };
        if link_local || ip.is_unspecified() || ip.is_multicast() {
            return Err("that address cannot be a peer".into());
        }
    }
    Ok(url.origin().ascii_serialization())
}

/// A host id that is safe to put in a path.
pub fn valid_host_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 128
        && id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.'))
        && !id.starts_with('.')
}

#[derive(Debug, Default, Deserialize)]
struct HostRow {
    #[serde(default)]
    id: String,
    #[serde(default)]
    hostname: String,
    #[serde(default)]
    status: String,
    #[serde(default)]
    cpus: u64,
    #[serde(default)]
    memory_mb: u64,
    #[serde(default)]
    vm_count: u64,
    #[serde(default)]
    cpu_usage_pct: f64,
    #[serde(default)]
    memory_usage_pct: f64,
}

fn bearer(token: Option<&str>, request: reqwest::RequestBuilder) -> reqwest::RequestBuilder {
    match token {
        Some(t) => request.bearer_auth(t),
        None => request,
    }
}

async fn get_json(
    url: String,
    token: Option<&str>,
) -> Result<Value, (&'static str, Option<String>)> {
    let response = bearer(token, client().get(url)).send().await.map_err(|e| {
        let why = if e.is_timeout() {
            "timed out"
        } else {
            "connection failed"
        };
        ("unreachable", Some(why.to_string()))
    })?;
    let status = response.status();
    if status == StatusCode::UNAUTHORIZED || status == StatusCode::FORBIDDEN {
        return Err(("unauthorized", Some(format!("peer answered {status}"))));
    }
    if !status.is_success() {
        return Err(("error", Some(format!("peer answered {status}"))));
    }
    let bytes = response
        .bytes()
        .await
        .map_err(|_| ("error", Some("could not read the answer".to_string())))?;
    if bytes.len() > MAX_BODY {
        return Err(("error", Some("the answer was too large".to_string())));
    }
    serde_json::from_slice(&bytes)
        .map_err(|_| ("error", Some("the answer was not JSON".to_string())))
}

/// One peer's row in the overview. Never fails: trouble becomes the row's `status`.
pub async fn peer_view(peer: &Peer, token: Option<String>) -> Value {
    let started = Instant::now();
    let base = json!({"id": peer.id, "name": peer.name, "url": peer.url});
    let row = |status: &str, message: Option<String>, extra: Value| {
        let mut v = base.clone();
        v["status"] = json!(status);
        v["latency_ms"] = json!(started.elapsed().as_millis() as u64);
        if let Some(m) = message {
            v["message"] = json!(m);
        }
        if let (Some(o), Some(e)) = (v.as_object_mut(), extra.as_object()) {
            o.extend(e.clone());
        }
        v
    };
    if peer.token_secret_id.is_some() && token.is_none() {
        return row(
            "token_missing",
            Some("set this peer's token again (it is kept in memory)".into()),
            json!({}),
        );
    }
    let hosts = match get_json(format!("{}/api/v1/hosts", peer.url), token.as_deref()).await {
        Ok(v) => v,
        Err((status, message)) => return row(status, message, json!({})),
    };
    let Ok(rows) = serde_json::from_value::<Vec<HostRow>>(hosts) else {
        return row(
            "error",
            Some("the host list had an unexpected shape".into()),
            json!({}),
        );
    };
    // The VM total is a courtesy: if it fails the hosts still show.
    let vm_total = get_json(format!("{}/api/v1/vms?limit=1", peer.url), token.as_deref())
        .await
        .ok()
        .and_then(|v| v.get("total").and_then(Value::as_u64));
    let maintenance = rows.iter().filter(|h| h.status == "maintenance").count();
    let hosts: Vec<Value> = rows
        .iter()
        .map(|h| {
            json!({
                "id": h.id, "hostname": h.hostname, "status": h.status, "cpus": h.cpus,
                "memory_mb": h.memory_mb, "vm_count": h.vm_count,
                "cpu_usage_pct": h.cpu_usage_pct, "memory_usage_pct": h.memory_usage_pct,
            })
        })
        .collect();
    row(
        "ok",
        None,
        json!({
            "vm_total": vm_total,
            "totals": {
                "hosts": rows.len(), "hosts_in_maintenance": maintenance,
                "cpus": rows.iter().map(|h| h.cpus).sum::<u64>(),
                "memory_mb": rows.iter().map(|h| h.memory_mb).sum::<u64>(),
            },
            "hosts": hosts,
        }),
    )
}

/// Add up the rows `peer_view` made.
pub fn overview_totals(peers: &[Value]) -> Value {
    let ok = |p: &&Value| p["status"] == "ok";
    let sum = |key: &str| -> u64 {
        peers
            .iter()
            .filter(ok)
            .filter_map(|p| p["totals"][key].as_u64())
            .sum()
    };
    json!({
        "peers": peers.len(),
        "reachable": peers.iter().filter(ok).count(),
        "hosts": sum("hosts"),
        "hosts_in_maintenance": sum("hosts_in_maintenance"),
        "vms": peers.iter().filter(ok).filter_map(|p| p["vm_total"].as_u64()).sum::<u64>(),
    })
}

/// Forward a maintenance change to a peer, as the caller. Returns the peer's HTTP status.
pub async fn forward_maintenance(
    base: &str,
    host_id: &str,
    enter: bool,
    caller_authorization: &str,
) -> Result<u16, String> {
    let action = if enter { "enter" } else { "exit" };
    let response = client()
        .post(format!(
            "{base}/api/v1/hosts/{host_id}/maintenance/{action}"
        ))
        .header(AUTHORIZATION, caller_authorization)
        .send()
        .await
        .map_err(|e| {
            if e.is_timeout() {
                "the peer timed out".to_string()
            } else {
                "could not reach the peer".to_string()
            }
        })?;
    Ok(response.status().as_u16())
}

fn peer_info(state: &AppState, peer: &Peer) -> Value {
    let has_token = peer
        .token_secret_id
        .as_deref()
        .is_some_and(|id| state.secrets_manager.get_secret(id).is_some());
    json!({
        "id": peer.id, "name": peer.name, "url": peer.url,
        "token_set": peer.token_secret_id.is_some(), "token_available": has_token,
        "created_at": peer.created_at,
    })
}

fn peer_token(state: &AppState, peer: &Peer) -> Option<String> {
    peer.token_secret_id
        .as_deref()
        .and_then(|id| state.secrets_manager.get_secret(id))
        .map(|s| s.value)
}

fn load_peers(state: &AppState) -> Vec<Peer> {
    let mut peers: Vec<Peer> = state.store.list_entities(COLLECTION).unwrap_or_default();
    peers.sort_by(|a, b| a.name.cmp(&b.name).then(a.id.cmp(&b.id)));
    peers
}

pub async fn list_peers(
    RequireRead(_claims): RequireRead,
    State(state): State<Arc<AppState>>,
) -> impl IntoResponse {
    let peers: Vec<Value> = load_peers(&state)
        .iter()
        .map(|p| peer_info(&state, p))
        .collect();
    Json(peers)
}

pub async fn register_peer(
    RequireAdmin(claims): RequireAdmin,
    State(state): State<Arc<AppState>>,
    Json(req): Json<RegisterPeerRequest>,
) -> impl IntoResponse {
    if let Err((status, msg)) = crate::validation::validate_entity_name(&req.name) {
        return error(status, msg);
    }
    let url = match validate_peer_url(&req.url) {
        Ok(u) => u,
        Err(msg) => return error(StatusCode::BAD_REQUEST, msg),
    };
    let existing = load_peers(&state);
    if existing.len() >= MAX_PEERS {
        return error(StatusCode::CONFLICT, format!("at most {MAX_PEERS} peers"));
    }
    if existing.iter().any(|p| p.url == url) {
        return error(StatusCode::CONFLICT, "that peer is already registered");
    }
    let id = Uuid::new_v4().to_string();
    let token_secret_id = match req.token.as_deref().filter(|t| !t.is_empty()) {
        Some(token) => {
            match state
                .secrets_manager
                .create_secret(&format!("federation-peer-{id}"), token, None)
            {
                Ok(s) => Some(s.id),
                Err(e) => return error(StatusCode::BAD_REQUEST, e.to_string()),
            }
        }
        None => None,
    };
    let peer = Peer {
        id: id.clone(),
        name: req.name,
        url,
        token_secret_id,
        created_at: Utc::now(),
    };
    if let Err(e) = state.store.save_entity(COLLECTION, &id, &peer) {
        return error(StatusCode::INTERNAL_SERVER_ERROR, e.to_string());
    }
    tracing::info!(user = %claims.sub, peer = %peer.id, url = %peer.url, "federation peer registered");
    (StatusCode::CREATED, Json(peer_info(&state, &peer))).into_response()
}

pub async fn set_peer_token(
    RequireAdmin(claims): RequireAdmin,
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    Json(req): Json<SetTokenRequest>,
) -> impl IntoResponse {
    let Ok(Some(mut peer)) = state.store.get_entity::<Peer>(COLLECTION, &id) else {
        return error(StatusCode::NOT_FOUND, "peer not found");
    };
    let secret = match state.secrets_manager.create_secret(
        &format!("federation-peer-{id}"),
        &req.token,
        None,
    ) {
        Ok(s) => s,
        Err(e) => return error(StatusCode::BAD_REQUEST, e.to_string()),
    };
    if let Some(old) = peer.token_secret_id.replace(secret.id) {
        state.secrets_manager.delete_secret(&old);
    }
    if let Err(e) = state.store.save_entity(COLLECTION, &id, &peer) {
        return error(StatusCode::INTERNAL_SERVER_ERROR, e.to_string());
    }
    tracing::info!(user = %claims.sub, peer = %id, "federation peer token set");
    Json(peer_info(&state, &peer)).into_response()
}

pub async fn delete_peer(
    RequireAdmin(claims): RequireAdmin,
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    let Ok(Some(peer)) = state.store.get_entity::<Peer>(COLLECTION, &id) else {
        return error(StatusCode::NOT_FOUND, "peer not found");
    };
    if let Some(secret) = &peer.token_secret_id {
        state.secrets_manager.delete_secret(secret);
    }
    match state.store.delete_entity(COLLECTION, &id) {
        Ok(()) => {
            tracing::info!(user = %claims.sub, peer = %id, "federation peer removed");
            StatusCode::NO_CONTENT.into_response()
        }
        Err(e) => error(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()),
    }
}

pub async fn overview(
    RequireRead(_claims): RequireRead,
    State(state): State<Arc<AppState>>,
) -> impl IntoResponse {
    let peers = load_peers(&state);
    let views =
        futures::future::join_all(peers.iter().map(|p| peer_view(p, peer_token(&state, p)))).await;
    Json(json!({
        "generated_at": Utc::now(),
        "totals": overview_totals(&views),
        "peers": views,
    }))
}

/// `POST /federation/peers/{id}/hosts/{host_id}/maintenance/{action}` where action is `enter` or `exit`.
pub async fn peer_host_maintenance(
    RequireAdmin(claims): RequireAdmin,
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path((id, host_id, action)): Path<(String, String, String)>,
) -> impl IntoResponse {
    let enter = match action.as_str() {
        "enter" => true,
        "exit" => false,
        _ => return error(StatusCode::BAD_REQUEST, "the action must be enter or exit"),
    };
    if !valid_host_id(&host_id) {
        return error(StatusCode::BAD_REQUEST, "invalid host id");
    }
    let Ok(Some(peer)) = state.store.get_entity::<Peer>(COLLECTION, &id) else {
        return error(StatusCode::NOT_FOUND, "peer not found");
    };
    let Some(authorization) = headers.get(AUTHORIZATION).and_then(|v| v.to_str().ok()) else {
        return error(
            StatusCode::BAD_REQUEST,
            "this needs your own bearer token, which is forwarded to the peer; cookie sessions cannot be forwarded",
        );
    };
    match forward_maintenance(&peer.url, &host_id, enter, authorization).await {
        Ok(status) => {
            tracing::info!(user = %claims.sub, peer = %peer.id, host = %host_id, %action, peer_status = status, "federation maintenance forwarded");
            let code = if (200..300).contains(&status) {
                StatusCode::OK
            } else {
                StatusCode::BAD_GATEWAY
            };
            (code, Json(json!({"peer_status": status, "peer": peer.name, "host": host_id, "action": action}))).into_response()
        }
        Err(msg) => error(StatusCode::BAD_GATEWAY, msg),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{routing::get, routing::post, Router};
    use std::sync::Mutex;

    fn peer(url: &str, with_token: bool) -> Peer {
        Peer {
            id: "p1".into(),
            name: "lab".into(),
            url: url.into(),
            token_secret_id: with_token.then(|| "s1".into()),
            created_at: Utc::now(),
        }
    }

    async fn serve(app: Router) -> String {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        format!("http://{addr}")
    }

    fn hosts_json() -> Value {
        json!([
            {"id": "h1", "hostname": "a", "status": "connected", "cpus": 8, "memory_mb": 16384, "vm_count": 3,
             "cpu_usage_pct": 10.0, "memory_usage_pct": 20.0, "extra": "ignored"},
            {"id": "h2", "hostname": "b", "status": "maintenance", "cpus": 4, "memory_mb": 8192, "vm_count": 0,
             "cpu_usage_pct": 0.0, "memory_usage_pct": 1.0}
        ])
    }

    #[test]
    fn peer_urls_are_reduced_to_an_origin_and_unsafe_ones_refused() {
        assert_eq!(
            validate_peer_url("https://fabric.example:9095/ignored/path").unwrap(),
            "https://fabric.example:9095"
        );
        assert_eq!(
            validate_peer_url("http://10.0.0.5:9095").unwrap(),
            "http://10.0.0.5:9095"
        );
        for bad in [
            "ftp://x.example",
            "https://user:pw@x.example",
            "https://x.example/?a=1",
            "https://x.example/#f",
            "http://169.254.169.254/latest",
            "http://[fe80::1]/",
            "http://0.0.0.0/",
            "not a url",
        ] {
            assert!(validate_peer_url(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn host_ids_cannot_escape_the_path() {
        assert!(valid_host_id("host-1.lab_a"));
        for bad in [
            "",
            "..",
            ".hidden",
            "a/b",
            "a?b",
            "a b",
            "a%2f",
            &"x".repeat(129),
        ] {
            assert!(!valid_host_id(bad), "{bad}");
        }
    }

    #[tokio::test]
    async fn a_peer_row_lists_hosts_and_totals_and_sends_the_token() {
        let seen: Arc<Mutex<Vec<String>>> = Default::default();
        let s1 = seen.clone();
        let s2 = seen.clone();
        let app = Router::new()
            .route(
                "/api/v1/hosts",
                get(move |h: HeaderMap| async move {
                    s1.lock().unwrap().push(
                        h.get(AUTHORIZATION)
                            .map(|v| v.to_str().unwrap().to_string())
                            .unwrap_or_default(),
                    );
                    Json(hosts_json())
                }),
            )
            .route(
                "/api/v1/vms",
                get(move |h: HeaderMap| async move {
                    s2.lock().unwrap().push(
                        h.get(AUTHORIZATION)
                            .map(|v| v.to_str().unwrap().to_string())
                            .unwrap_or_default(),
                    );
                    Json(json!({"items": [], "total": 7}))
                }),
            );
        let url = serve(app).await;
        let view = peer_view(&peer(&url, true), Some("read-token".into())).await;
        assert_eq!(view["status"], "ok");
        assert_eq!(view["vm_total"], 7);
        assert_eq!(view["totals"]["hosts"], 2);
        assert_eq!(view["totals"]["hosts_in_maintenance"], 1);
        assert_eq!(view["totals"]["cpus"], 12);
        assert_eq!(view["hosts"][1]["status"], "maintenance");
        assert!(seen
            .lock()
            .unwrap()
            .iter()
            .all(|a| a == "Bearer read-token"));
        // the token itself never appears in the row
        assert!(!view.to_string().contains("read-token"));
        let totals = overview_totals(&[view]);
        assert_eq!(
            (totals["hosts"].as_u64(), totals["vms"].as_u64()),
            (Some(2), Some(7))
        );
    }

    #[tokio::test]
    async fn a_bad_token_a_dead_peer_a_redirect_and_a_lost_token_each_show_in_their_own_row() {
        let app = Router::new()
            .route("/api/v1/hosts", get(|| async { StatusCode::UNAUTHORIZED }))
            .route(
                "/redirecting/api/v1/hosts",
                get(|| async { (StatusCode::FOUND, [("location", "http://169.254.169.254/")]) }),
            );
        let url = serve(app).await;
        assert_eq!(
            peer_view(&peer(&url, true), Some("x".into())).await["status"],
            "unauthorized"
        );
        // a redirect is reported, never followed
        let redirected = peer_view(&peer(&format!("{url}/redirecting"), false), None).await;
        assert_eq!(redirected["status"], "error");
        // nothing listens on port 1
        let dead = peer_view(&peer("http://127.0.0.1:1", false), None).await;
        assert_eq!(dead["status"], "unreachable");
        // a token was set before a restart and the secret is gone
        assert_eq!(
            peer_view(&peer(&url, true), None).await["status"],
            "token_missing"
        );
        // one bad row does not hide a good one in the totals
        let totals = overview_totals(&[
            dead,
            json!({"status": "ok", "vm_total": 2, "totals": {"hosts": 1}}),
        ]);
        assert_eq!(
            (
                totals["peers"].as_u64(),
                totals["reachable"].as_u64(),
                totals["vms"].as_u64()
            ),
            (Some(2), Some(1), Some(2))
        );
    }

    #[tokio::test]
    async fn maintenance_is_forwarded_with_the_callers_own_header() {
        let seen: Arc<Mutex<Vec<(String, String)>>> = Default::default();
        let s = seen.clone();
        let app = Router::new().route(
            "/api/v1/hosts/{id}/maintenance/{action}",
            post(
                move |Path((id, action)): Path<(String, String)>, h: HeaderMap| async move {
                    s.lock().unwrap().push((
                        format!("{id}/{action}"),
                        h.get(AUTHORIZATION).unwrap().to_str().unwrap().to_string(),
                    ));
                    if id == "denied" {
                        StatusCode::FORBIDDEN
                    } else {
                        StatusCode::OK
                    }
                },
            ),
        );
        let url = serve(app).await;
        assert_eq!(
            forward_maintenance(&url, "h1", true, "Bearer caller").await,
            Ok(200)
        );
        assert_eq!(
            forward_maintenance(&url, "denied", false, "Bearer viewer").await,
            Ok(403)
        );
        assert_eq!(
            *seen.lock().unwrap(),
            vec![
                ("h1/enter".to_string(), "Bearer caller".to_string()),
                ("denied/exit".to_string(), "Bearer viewer".to_string())
            ]
        );
        assert!(
            forward_maintenance("http://127.0.0.1:1", "h1", true, "Bearer x")
                .await
                .is_err()
        );
    }
}
