// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: LicenseRef-Zyvor-Production-2.0

//! HTTPS CONNECT proxy for programs that cannot call the JSON egress broker,
//! chiefly a browser running inside the sandbox.
//!
//! A tunnel goes through the same gates as a brokered request: the agent's
//! allowlist, `ask`/`sentinel` review of unlisted hosts, DNS pinning and the
//! private-network gate. It is journaled as `egress.connect`. Because the
//! traffic is TLS the broker sees only `host:port`, so the parts of the broker
//! that need to read a request (credential injection, path-level review) do not
//! apply. Only `CONNECT` is served; plain `http://` proxying is refused.
//!
//! This only constrains a guest whose network path leads here. The template's
//! network must not give the guest a direct route to the internet.

use crate::{
    audit::AuditPhase,
    credentials::host_matches,
    egress::{authorize_unlisted_host, constant_time_eq, resolve_and_validate_destination},
    mitm::Mitm,
    model::{AgentRecord, SessionRecord},
    AppState,
};
use axum::http::StatusCode;
use base64::Engine;
use reqwest::Url;
use serde_json::json;
use std::{sync::Arc, time::Duration};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{TcpListener, TcpStream},
};

const MAX_HEAD_BYTES: usize = 16 * 1024;
const HEAD_TIMEOUT: Duration = Duration::from_secs(10);
const CONNECT_TIMEOUT: Duration = Duration::from_secs(15);
/// Longest a single tunnel may stay open.
const MAX_TUNNEL: Duration = Duration::from_secs(3600);

/// Why a tunnel was refused; `auth` asks the client to authenticate again.
struct Refusal {
    status: StatusCode,
    message: String,
    auth: bool,
}

impl Refusal {
    fn new(status: StatusCode, message: impl Into<String>) -> Self {
        Self {
            status,
            message: message.into(),
            auth: false,
        }
    }
    fn auth(message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::PROXY_AUTHENTICATION_REQUIRED,
            message: message.into(),
            auth: true,
        }
    }
}

impl From<(StatusCode, String)> for Refusal {
    fn from((status, message): (StatusCode, String)) -> Self {
        Self::new(status, message)
    }
}

pub async fn serve(state: Arc<AppState>, listener: TcpListener) {
    loop {
        match listener.accept().await {
            Ok((stream, _)) => {
                let state = state.clone();
                tokio::spawn(async move { handle(state, stream).await });
            }
            Err(error) => {
                tracing::warn!(%error, "proxy accept failed");
                tokio::time::sleep(Duration::from_millis(100)).await;
            }
        }
    }
}

async fn handle(state: Arc<AppState>, mut client: TcpStream) {
    let (head, leftover) = match tokio::time::timeout(HEAD_TIMEOUT, read_head(&mut client)).await {
        Ok(Ok(parts)) => parts,
        _ => return,
    };
    match open_tunnel(&state, &head).await {
        Ok(tunnel) => run_tunnel(&state, client, leftover, tunnel).await,
        Err(refusal) => {
            let mut response = format!(
                "HTTP/1.1 {} {}\r\nContent-Type: text/plain\r\nContent-Length: {}\r\nConnection: close\r\n",
                refusal.status.as_u16(),
                refusal.status.canonical_reason().unwrap_or(""),
                refusal.message.len()
            );
            if refusal.auth {
                response.push_str("Proxy-Authenticate: Basic realm=\"zyvor\"\r\n");
            }
            response.push_str("\r\n");
            response.push_str(&refusal.message);
            let _ = client.write_all(response.as_bytes()).await;
        }
    }
}

/// A blind byte-for-byte tunnel to the destination.
struct DirectTunnel {
    upstream: TcpStream,
    session_id: uuid::Uuid,
    host: String,
    port: u16,
}

/// A tunnel whose TLS the runtime terminates, so each request goes through the
/// broker. Only for hosts a credential descriptor asks to intercept.
struct Interception {
    session: SessionRecord,
    agent: AgentRecord,
    mitm: Arc<Mitm>,
    host: String,
    port: u16,
}

enum Tunnel {
    Direct(DirectTunnel),
    Intercept(Box<Interception>),
}

async fn open_tunnel(state: &Arc<AppState>, head: &str) -> Result<Tunnel, Refusal> {
    let mut lines = head.split("\r\n");
    let request_line = lines.next().unwrap_or_default();
    let mut parts = request_line.split_whitespace();
    let (method, target) = (parts.next().unwrap_or(""), parts.next().unwrap_or(""));
    if !method.eq_ignore_ascii_case("CONNECT") {
        return Err(Refusal::new(
            StatusCode::METHOD_NOT_ALLOWED,
            "only CONNECT (HTTPS) is supported by the egress proxy",
        ));
    }

    let credentials = lines
        .filter_map(|line| line.split_once(':'))
        .find(|(name, _)| name.trim().eq_ignore_ascii_case("proxy-authorization"))
        .map(|(_, value)| value.trim().to_string())
        .ok_or_else(|| Refusal::auth("missing Proxy-Authorization"))?;
    let (session_id, capability) = parse_basic(&credentials)
        .ok_or_else(|| Refusal::auth("Proxy-Authorization must be Basic session:capability"))?;
    let session = state
        .store
        .get_session(session_id)
        .await
        .filter(|s| constant_time_eq(s.capability_token.as_bytes(), capability.as_bytes()))
        .ok_or_else(|| Refusal::auth("invalid session or capability"))?;
    if session.status.is_terminal() {
        return Err(Refusal::new(
            StatusCode::FORBIDDEN,
            "session is no longer active",
        ));
    }
    let agent = state
        .store
        .get_agent_version(&session.agent, &session.agent_version)
        .await
        .map_err(|_| {
            Refusal::new(
                StatusCode::FORBIDDEN,
                "pinned agent deployment no longer exists",
            )
        })?;

    let (host, port) = parse_authority(target)
        .ok_or_else(|| Refusal::new(StatusCode::BAD_REQUEST, "invalid CONNECT target"))?;
    let refuse = |status: StatusCode, message: String| {
        refuse(state, session.id, &host, port, status, message)
    };

    if !state.config.proxy_connect_ports.contains(&port) {
        return refuse(
            StatusCode::FORBIDDEN,
            format!("CONNECT to port {port} is not permitted"),
        )
        .await;
    }
    // A host a credential wants intercepted is not tunnelled at all: the broker
    // judges every request inside it (allowlist, approvals, rules, DLP, taint), so
    // none of the checks below apply to the tunnel itself.
    if let Some(mitm) = &state.mitm {
        if !state
            .credentials
            .intercepted_for(&agent.manifest.credentials, &host)
            .is_empty()
        {
            record(
                state,
                session.id,
                AuditPhase::Approved,
                &host,
                json!({"port": port, "intercepted": true}),
            )
            .await;
            return Ok(Tunnel::Intercept(Box::new(Interception {
                session: session.clone(),
                agent: agent.clone(),
                mitm: mitm.clone(),
                host: host.clone(),
                port,
            })));
        }
    }
    // Per-host rules constrain methods and paths, which a TLS tunnel hides.
    if crate::l7::host_has_rules(&agent.manifest.egress_rules, &host) {
        return refuse(
            StatusCode::FORBIDDEN,
            format!("{host} has egress rules that cannot be enforced through a CONNECT tunnel"),
        )
        .await;
    }
    if !agent
        .manifest
        .egress_allow_hosts
        .iter()
        .any(|h| host_matches(h, &host))
    {
        let url = Url::parse(&format!("https://{}/", url_host(&host)))
            .map_err(|_| Refusal::new(StatusCode::BAD_REQUEST, "invalid CONNECT target"))?;
        if let Err((status, message)) =
            authorize_unlisted_host(state, &session, &agent.manifest, &url, "CONNECT").await
        {
            return refuse(status, message).await;
        }
    }
    let pinned =
        match resolve_and_validate_destination(&host, port, agent.manifest.allow_private_networks)
            .await
        {
            Ok(addr) => addr,
            Err((status, message)) => return refuse(status, message).await,
        };
    let upstream = match tokio::time::timeout(CONNECT_TIMEOUT, TcpStream::connect(pinned)).await {
        Ok(Ok(stream)) => stream,
        Ok(Err(error)) => {
            return refuse(StatusCode::BAD_GATEWAY, format!("connect failed: {error}")).await
        }
        Err(_) => return refuse(StatusCode::GATEWAY_TIMEOUT, "connect timed out".into()).await,
    };
    record(
        state,
        session.id,
        AuditPhase::Approved,
        &host,
        json!({"port": port}),
    )
    .await;
    // The browser is about to read from this host.
    crate::egress::taint_on_read(state, &session, &agent.manifest, &host).await;
    Ok(Tunnel::Direct(DirectTunnel {
        upstream,
        session_id: session.id,
        host,
        port,
    }))
}

async fn record(
    state: &AppState,
    session_id: uuid::Uuid,
    phase: AuditPhase,
    host: &str,
    detail: serde_json::Value,
) {
    if let Err(error) = state
        .store
        .audit
        .append(
            Some(session_id),
            phase,
            "egress.connect",
            Some(host.to_string()),
            detail,
        )
        .await
    {
        tracing::error!(%error, "failed to write audit entry");
    }
}

/// Journal a refused tunnel and return it as the error.
async fn refuse<T>(
    state: &AppState,
    session_id: uuid::Uuid,
    host: &str,
    port: u16,
    status: StatusCode,
    message: String,
) -> Result<T, Refusal> {
    record(
        state,
        session_id,
        AuditPhase::Denied,
        host,
        json!({"port": port, "reason": message}),
    )
    .await;
    Err(Refusal::new(status, message))
}

async fn run_tunnel(
    state: &Arc<AppState>,
    mut client: TcpStream,
    leftover: Vec<u8>,
    tunnel: Tunnel,
) {
    let DirectTunnel {
        mut upstream,
        session_id,
        host,
        port,
    } = match tunnel {
        Tunnel::Direct(direct) => direct,
        Tunnel::Intercept(interception) => {
            let Interception {
                session,
                agent,
                mitm,
                host,
                port,
            } = *interception;
            if client
                .write_all(b"HTTP/1.1 200 Connection Established\r\n\r\n")
                .await
                .is_err()
            {
                return;
            }
            let target = crate::mitm::Target {
                session,
                agent,
                host,
                port,
            };
            crate::mitm::intercept(state.clone(), mitm, target, client, leftover).await;
            return;
        }
    };
    if client
        .write_all(b"HTTP/1.1 200 Connection Established\r\n\r\n")
        .await
        .is_err()
    {
        return;
    }
    if !leftover.is_empty() && upstream.write_all(&leftover).await.is_err() {
        return;
    }
    let copied = tokio::time::timeout(
        MAX_TUNNEL,
        tokio::io::copy_bidirectional(&mut client, &mut upstream),
    )
    .await;
    let (phase, detail) = match copied {
        Ok(Ok((up, down))) => (
            AuditPhase::Performed,
            json!({"port": port, "bytes_up": up, "bytes_down": down}),
        ),
        Ok(Err(error)) => (
            AuditPhase::Failed,
            json!({"port": port, "error": error.to_string()}),
        ),
        Err(_) => (
            AuditPhase::Failed,
            json!({"port": port, "error": "tunnel exceeded its maximum lifetime"}),
        ),
    };
    if let Err(error) = state
        .store
        .audit
        .append(
            Some(session_id),
            phase,
            "egress.connect",
            Some(host),
            detail,
        )
        .await
    {
        tracing::error!(%error, "failed to write audit entry");
    }
}

/// Read up to the blank line that ends the request head. Anything read past it
/// belongs to the tunnel and is returned as the second value.
async fn read_head(stream: &mut TcpStream) -> std::io::Result<(String, Vec<u8>)> {
    let mut buffer = Vec::with_capacity(1024);
    let mut chunk = [0u8; 1024];
    loop {
        if let Some(end) = buffer.windows(4).position(|w| w == b"\r\n\r\n") {
            let leftover = buffer.split_off(end + 4);
            buffer.truncate(end);
            return Ok((String::from_utf8_lossy(&buffer).into_owned(), leftover));
        }
        if buffer.len() > MAX_HEAD_BYTES {
            return Err(std::io::ErrorKind::InvalidData.into());
        }
        let n = stream.read(&mut chunk).await?;
        if n == 0 {
            return Err(std::io::ErrorKind::UnexpectedEof.into());
        }
        buffer.extend_from_slice(&chunk[..n]);
    }
}

fn parse_basic(value: &str) -> Option<(uuid::Uuid, String)> {
    let (scheme, encoded) = value.split_once(' ')?;
    if !scheme.eq_ignore_ascii_case("basic") {
        return None;
    }
    let decoded = base64::engine::general_purpose::STANDARD
        .decode(encoded.trim())
        .ok()?;
    let decoded = String::from_utf8(decoded).ok()?;
    let (session, capability) = decoded.split_once(':')?;
    Some((uuid::Uuid::parse_str(session).ok()?, capability.to_string()))
}

/// `host:port` or `[v6]:port` to a lowercase host (without brackets) and port.
fn parse_authority(target: &str) -> Option<(String, u16)> {
    let (host, port) = target.rsplit_once(':')?;
    let host = host
        .strip_prefix('[')
        .and_then(|h| h.strip_suffix(']'))
        .unwrap_or(host);
    let valid = !host.is_empty()
        && host
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | ':' | '_'));
    let port: u16 = port.parse().ok().filter(|p| *p != 0)?;
    valid.then(|| (host.to_ascii_lowercase(), port))
}

fn url_host(host: &str) -> String {
    if host.contains(':') {
        format!("[{host}]")
    } else {
        host.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        egress::ask_tests::{manifest, state_and_session_cfg},
        model::EgressMode,
    };
    use tokio::net::TcpListener;

    fn basic(session: uuid::Uuid, capability: &str) -> String {
        let raw = format!("{session}:{capability}");
        format!(
            "Basic {}",
            base64::engine::general_purpose::STANDARD.encode(raw)
        )
    }

    /// Run the proxy on a local port and return that port.
    async fn start(state: Arc<AppState>) -> u16 {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        tokio::spawn(serve(state, listener));
        port
    }

    /// A TCP server that echoes what it receives, prefixed with `echo:`.
    async fn echo() -> u16 {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        tokio::spawn(async move {
            loop {
                let (mut stream, _) = listener.accept().await.unwrap();
                tokio::spawn(async move {
                    let mut buf = [0u8; 64];
                    let n = stream.read(&mut buf).await.unwrap_or(0);
                    let _ = stream.write_all(b"echo:").await;
                    let _ = stream.write_all(&buf[..n]).await;
                });
            }
        });
        port
    }

    /// Send one request head through the proxy and read the status line back.
    async fn status_of(proxy: u16, head: String) -> String {
        let mut stream = TcpStream::connect(("127.0.0.1", proxy)).await.unwrap();
        stream.write_all(head.as_bytes()).await.unwrap();
        let mut buf = vec![0u8; 512];
        let n = stream.read(&mut buf).await.unwrap();
        String::from_utf8_lossy(&buf[..n])
            .lines()
            .next()
            .unwrap_or("")
            .to_string()
    }

    async fn setup(
        allow: &[&str],
        private: bool,
        mode: EgressMode,
        target_port: u16,
    ) -> (Arc<AppState>, crate::model::SessionRecord, u16) {
        setup_with(allow, private, mode, target_port, |_| {}).await
    }

    async fn setup_with(
        allow: &[&str],
        private: bool,
        mode: EgressMode,
        target_port: u16,
        tweak: impl FnOnce(&mut crate::model::AgentManifest),
    ) -> (Arc<AppState>, crate::model::SessionRecord, u16) {
        let (state, session) =
            state_and_session_cfg(|config| config.proxy_connect_ports = vec![target_port]).await;
        let mut agent_manifest = manifest(mode, Some(5));
        agent_manifest.egress_allow_hosts = allow.iter().map(|h| h.to_string()).collect();
        agent_manifest.allow_private_networks = private;
        tweak(&mut agent_manifest);
        let deployed = state
            .store
            .deploy_agent(crate::model::DeployAgentRequest {
                name: session.agent.clone(),
                bundle_base64: base64::engine::general_purpose::STANDARD.encode("export default 1"),
                manifest: agent_manifest,
            })
            .await
            .unwrap();
        let session = state
            .store
            .update_session(session.id, |s| s.agent_version = deployed.version.clone())
            .await
            .unwrap();
        let port = start(state.clone()).await;
        (state, session, port)
    }

    #[tokio::test]
    async fn tunnels_to_an_allowlisted_host_and_journals_it() {
        let target = echo().await;
        let (state, session, proxy) = setup(&["127.0.0.1"], true, EgressMode::Deny, target).await;
        let mut stream = TcpStream::connect(("127.0.0.1", proxy)).await.unwrap();
        let head = format!(
            "CONNECT 127.0.0.1:{target} HTTP/1.1\r\nHost: x\r\nProxy-Authorization: {}\r\n\r\nhello",
            basic(session.id, "cap")
        );
        stream.write_all(head.as_bytes()).await.unwrap();
        let mut reply = Vec::new();
        let mut buf = [0u8; 256];
        while !String::from_utf8_lossy(&reply).contains("echo:hello") {
            let n = stream.read(&mut buf).await.unwrap();
            assert!(n > 0, "closed early: {}", String::from_utf8_lossy(&reply));
            reply.extend_from_slice(&buf[..n]);
        }
        assert!(String::from_utf8_lossy(&reply).starts_with("HTTP/1.1 200"));
        drop(stream);
        for _ in 0..100 {
            let entries = state.store.audit.list(Some(session.id), 50).await.unwrap();
            if entries
                .iter()
                .any(|e| e.action == "egress.connect" && e.phase == AuditPhase::Performed)
            {
                return;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
        panic!("tunnel was not journaled");
    }

    #[tokio::test]
    async fn refuses_without_or_with_wrong_credentials() {
        let target = echo().await;
        let (_state, session, proxy) = setup(&["127.0.0.1"], true, EgressMode::Deny, target).await;
        let base = format!("CONNECT 127.0.0.1:{target} HTTP/1.1\r\n");
        let none = status_of(proxy, format!("{base}\r\n")).await;
        assert!(none.contains("407"), "{none}");
        let wrong = status_of(
            proxy,
            format!(
                "{base}Proxy-Authorization: {}\r\n\r\n",
                basic(session.id, "nope")
            ),
        )
        .await;
        assert!(wrong.contains("407"), "{wrong}");
    }

    #[tokio::test]
    async fn only_connect_is_served() {
        let target = echo().await;
        let (_state, session, proxy) = setup(&["127.0.0.1"], true, EgressMode::Deny, target).await;
        let status = status_of(
            proxy,
            format!(
                "GET http://127.0.0.1:{target}/ HTTP/1.1\r\nProxy-Authorization: {}\r\n\r\n",
                basic(session.id, "cap")
            ),
        )
        .await;
        assert!(status.contains("405"), "{status}");
    }

    #[tokio::test]
    async fn unlisted_hosts_and_ports_are_refused() {
        let target = echo().await;
        let (state, session, proxy) = setup(&["example.com"], true, EgressMode::Deny, target).await;
        let auth = basic(session.id, "cap");
        let unlisted = status_of(
            proxy,
            format!("CONNECT 127.0.0.1:{target} HTTP/1.1\r\nProxy-Authorization: {auth}\r\n\r\n"),
        )
        .await;
        assert!(unlisted.contains("403"), "{unlisted}");
        let bad_port = status_of(
            proxy,
            format!("CONNECT example.com:25 HTTP/1.1\r\nProxy-Authorization: {auth}\r\n\r\n"),
        )
        .await;
        assert!(bad_port.contains("403"), "{bad_port}");
        let port80 = status_of(
            proxy,
            format!("CONNECT example.com:80 HTTP/1.1\r\nProxy-Authorization: {auth}\r\n\r\n"),
        )
        .await;
        assert!(port80.contains("403"), "{port80}");
        let port8080 = status_of(
            proxy,
            format!("CONNECT example.com:8080 HTTP/1.1\r\nProxy-Authorization: {auth}\r\n\r\n"),
        )
        .await;
        assert!(port8080.contains("403"), "{port8080}");
        let entries = state.store.audit.list(Some(session.id), 50).await.unwrap();
        assert!(
            entries
                .iter()
                .filter(|e| e.action == "egress.connect" && e.phase == AuditPhase::Denied)
                .count()
                >= 4
        );
    }

    #[tokio::test]
    async fn private_destinations_stay_blocked_even_when_allowlisted() {
        let target = echo().await;
        let (_state, session, proxy) = setup(&["127.0.0.1"], false, EgressMode::Deny, target).await;
        let status = status_of(
            proxy,
            format!(
                "CONNECT 127.0.0.1:{target} HTTP/1.1\r\nProxy-Authorization: {}\r\n\r\n",
                basic(session.id, "cap")
            ),
        )
        .await;
        assert!(status.contains("403"), "{status}");
    }

    #[test]
    fn parses_authorities() {
        assert_eq!(
            parse_authority("Example.com:443"),
            Some(("example.com".into(), 443))
        );
        assert_eq!(parse_authority("[::1]:443"), Some(("::1".into(), 443)));
        for bad in ["example.com", "example.com:0", "a b:443", ":443", "x/y:443"] {
            assert_eq!(parse_authority(bad), None, "{bad}");
        }
    }

    #[tokio::test]
    async fn hosts_with_egress_rules_cannot_be_tunnelled() {
        let target = echo().await;
        let (_state, session, proxy) =
            setup_with(&["127.0.0.1"], true, EgressMode::Deny, target, |m| {
                m.egress_rules = vec![crate::model::EgressRule {
                    host: "127.0.0.1".into(),
                    methods: vec!["GET".into()],
                    path_prefixes: vec![],
                    max_body_bytes: None,
                    ..Default::default()
                }];
            })
            .await;
        let status = status_of(
            proxy,
            format!(
                "CONNECT 127.0.0.1:{target} HTTP/1.1\r\nProxy-Authorization: {}\r\n\r\n",
                basic(session.id, "cap")
            ),
        )
        .await;
        assert!(status.contains("403"), "{status}");
    }

    #[tokio::test]
    async fn a_tunnel_to_an_untrusted_host_taints_the_session() {
        let target = echo().await;
        let (state, session, proxy) =
            setup_with(&["127.0.0.1"], true, EgressMode::Deny, target, |m| {
                m.taint = Some(Default::default());
            })
            .await;
        let status = status_of(
            proxy,
            format!(
                "CONNECT 127.0.0.1:{target} HTTP/1.1\r\nProxy-Authorization: {}\r\n\r\n",
                basic(session.id, "cap")
            ),
        )
        .await;
        assert!(status.contains("200"), "{status}");
        let tainted = state
            .store
            .get_session(session.id)
            .await
            .unwrap()
            .tainted_by;
        assert_eq!(tainted, vec!["127.0.0.1".to_string()]);
    }
}
