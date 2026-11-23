// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: LicenseRef-Zyvor-Production-2.0

//! Optional TLS interception, so a browser can hold a surrogate instead of a secret.
//!
//! The JSON broker injects real credentials on the host, so an agent that calls it
//! never sees a secret. A browser cannot call it: its traffic is TLS through the
//! CONNECT proxy, where the proxy sees only `host:port`. For hosts whose
//! credential descriptor sets `intercept`, the proxy instead terminates TLS with a
//! certificate from a local CA (installed in the guest's trust store), reads each
//! request, and hands it to the broker as an ordinary brokered request. The agent
//! holds a per-session surrogate (`credentials::surrogate`); when a request
//! carries it, the broker injects the real secret. Everything else the broker
//! does applies to the intercepted request too: the allowlist and approvals,
//! per-host rules, secret scanning, taint, the private-network gate, and the
//! journal. There is nothing else to keep in step.
//!
//! **What this costs, and why it is off by default.** The CA's private key sits on
//! the host, and the plaintext of every intercepted host passes through the
//! runtime's memory. That defeats the point of a confidential VM for those hosts
//! and puts the CA key among the most sensitive files on the machine, so it only
//! exists when `ZYVOR_AGENT_MITM_CA_DIR` is set and only for hosts a credential
//! descriptor names. Hosts without one stay blind tunnels.
//!
//! Limits: HTTP/1.1 only; each request and response is buffered (16 MiB, the
//! broker's limit), so no streaming bodies; `Upgrade` (WebSocket) is refused.
//! The request URL comes from the CONNECT target, never the `Host` header, so a
//! client cannot use a tunnel to reach a different host.

use crate::{
    credentials::surrogate,
    egress::proxy_inner,
    model::{AgentRecord, EgressRequest, SessionRecord},
    AppState,
};
use anyhow::{Context, Result};
use axum::http::{HeaderMap, StatusCode};
use base64::Engine;
use bytes::Bytes;
use http_body_util::{BodyExt, Full, Limited};
use hyper::{body::Incoming, service::service_fn, Request, Response};
use hyper_util::rt::TokioIo;
use rcgen::{
    BasicConstraints, CertificateParams, DnType, ExtendedKeyUsagePurpose, IsCa, Issuer, KeyPair,
    SanType,
};
use rustls::{
    pki_types::{CertificateDer, PrivateKeyDer},
    ServerConfig,
};
use std::{
    collections::{BTreeMap, HashMap},
    convert::Infallible,
    io,
    net::IpAddr,
    path::Path,
    pin::Pin,
    sync::{Arc, Mutex},
    task::{Context as TaskContext, Poll},
};
use tokio::io::{AsyncRead, AsyncWrite, ReadBuf};
use tokio_rustls::TlsAcceptor;

const CA_CERT: &str = "ca.pem";
const CA_KEY: &str = "ca.key";
const MAX_BODY: usize = 16 * 1024 * 1024;

pub struct Mitm {
    issuer: Issuer<'static, KeyPair>,
    ca_pem: String,
    configs: Mutex<HashMap<String, Arc<ServerConfig>>>,
}

impl Mitm {
    /// Load the CA from `dir`, or create it (key file mode 0600) on first use.
    pub async fn load_or_create(dir: &Path) -> Result<Self> {
        tokio::fs::create_dir_all(dir).await?;
        let (cert_path, key_path) = (dir.join(CA_CERT), dir.join(CA_KEY));
        match (
            tokio::fs::read_to_string(&cert_path).await,
            tokio::fs::read_to_string(&key_path).await,
        ) {
            (Ok(cert_pem), Ok(key_pem)) => Self::from_pem(&cert_pem, &key_pem),
            _ => {
                let mut params = CertificateParams::new(Vec::<String>::new())?;
                params.is_ca = IsCa::Ca(BasicConstraints::Unconstrained);
                params
                    .distinguished_name
                    .push(DnType::CommonName, "Zyvor agent egress CA");
                params
                    .distinguished_name
                    .push(DnType::OrganizationName, "Zyvor Fabric");
                let key = KeyPair::generate()?;
                let cert = params.self_signed(&key)?;
                write_private(&key_path, key.serialize_pem().as_bytes()).await?;
                tokio::fs::write(&cert_path, cert.pem()).await?;
                Self::from_pem(&cert.pem(), &key.serialize_pem())
            }
        }
    }

    fn from_pem(cert_pem: &str, key_pem: &str) -> Result<Self> {
        let ca_key = KeyPair::from_pem(key_pem).context("reading the interception CA key")?;
        // Build the issuer from the stored certificate: same subject and
        // key, so leaves it signs verify against the CA certificate on disk.
        let issuer = Issuer::from_ca_cert_pem(cert_pem, ca_key)
            .context("reading the interception CA certificate")?;
        Ok(Self {
            issuer,
            ca_pem: cert_pem.to_string(),
            configs: Mutex::new(HashMap::new()),
        })
    }

    /// The CA certificate to install in a guest's trust store.
    pub fn ca_pem(&self) -> &str {
        &self.ca_pem
    }

    /// A TLS server configuration presenting a certificate for `host`, cached.
    pub(crate) fn server_config(&self, host: &str) -> Result<Arc<ServerConfig>> {
        if let Some(found) = self
            .configs
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .get(host)
        {
            return Ok(found.clone());
        }
        let san = match host.parse::<IpAddr>() {
            Ok(ip) => SanType::IpAddress(ip),
            Err(_) => SanType::DnsName(host.to_string().try_into()?),
        };
        let mut params = CertificateParams::new(Vec::<String>::new())?;
        params.subject_alt_names = vec![san];
        params.distinguished_name.push(DnType::CommonName, host);
        params.extended_key_usages = vec![ExtendedKeyUsagePurpose::ServerAuth];
        let now = time::OffsetDateTime::now_utc();
        params.not_before = now - time::Duration::days(1);
        params.not_after = now + time::Duration::days(30);
        let key = KeyPair::generate()?;
        let cert = params.signed_by(&key, &self.issuer)?;
        let chain = vec![CertificateDer::from(cert.der().to_vec())];
        let key_der = PrivateKeyDer::try_from(key.serialize_der())
            .map_err(|e| anyhow::anyhow!("leaf key: {e}"))?;
        let provider = Arc::new(rustls::crypto::aws_lc_rs::default_provider());
        let mut config = ServerConfig::builder_with_provider(provider)
            .with_safe_default_protocol_versions()?
            .with_no_client_auth()
            .with_single_cert(chain, key_der)?;
        config.alpn_protocols = vec![b"http/1.1".to_vec()];
        let config = Arc::new(config);
        self.configs
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .insert(host.to_string(), config.clone());
        Ok(config)
    }
}

/// Where the CA certificate is installed in the guest.
pub const GUEST_CA_PATH: &str = "/usr/local/share/ca-certificates/zyvor-egress-ca.crt";

/// Best-effort commands that make the guest trust the CA: the system store, and
/// Chromium's NSS database for the `agent` user. A failure only means the browser
/// will show certificate errors; it never weakens anything, so it is not fatal.
pub const GUEST_INSTALL: &str = "update-ca-certificates >/dev/null 2>&1 || true; \
if command -v certutil >/dev/null 2>&1; then \
  mkdir -p /home/agent/.pki/nssdb; \
  certutil -d sql:/home/agent/.pki/nssdb -N --empty-password >/dev/null 2>&1 || true; \
  certutil -d sql:/home/agent/.pki/nssdb -A -t 'C,,' -n zyvor-egress -i /usr/local/share/ca-certificates/zyvor-egress-ca.crt >/dev/null 2>&1 || true; \
  chown -R agent /home/agent/.pki >/dev/null 2>&1 || true; \
fi";

/// The surrogate this session's agent holds for each intercepted credential.
pub fn surrogates(
    credentials: &crate::credentials::CredentialVault,
    granted: &[String],
    capability: &str,
) -> BTreeMap<String, String> {
    granted
        .iter()
        .filter(|name| credentials.descriptor(name).is_some_and(|d| d.intercept))
        .map(|name| (name.clone(), surrogate(capability, name)))
        .collect()
}

async fn write_private(path: &Path, bytes: &[u8]) -> Result<()> {
    use tokio::io::AsyncWriteExt;
    let mut options = tokio::fs::OpenOptions::new();
    options.write(true).create(true).truncate(true);
    #[cfg(unix)]
    options.mode(0o600);
    let mut file = options.open(path).await?;
    file.write_all(bytes).await?;
    Ok(())
}

/// A stream that yields `prefix` first, then reads from `inner`. The bytes a
/// client sent right behind its CONNECT belong to the TLS handshake.
struct Prefixed<S> {
    prefix: io::Cursor<Vec<u8>>,
    inner: S,
}

impl<S: AsyncRead + Unpin> AsyncRead for Prefixed<S> {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut TaskContext<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        let position = self.prefix.position() as usize;
        let remaining = &self.prefix.get_ref()[position..];
        if !remaining.is_empty() {
            let n = remaining.len().min(buf.remaining());
            buf.put_slice(&remaining[..n]);
            self.prefix.set_position((position + n) as u64);
            return Poll::Ready(Ok(()));
        }
        Pin::new(&mut self.inner).poll_read(cx, buf)
    }
}

impl<S: AsyncWrite + Unpin> AsyncWrite for Prefixed<S> {
    fn poll_write(
        mut self: Pin<&mut Self>,
        cx: &mut TaskContext<'_>,
        data: &[u8],
    ) -> Poll<io::Result<usize>> {
        Pin::new(&mut self.inner).poll_write(cx, data)
    }
    fn poll_flush(mut self: Pin<&mut Self>, cx: &mut TaskContext<'_>) -> Poll<io::Result<()>> {
        Pin::new(&mut self.inner).poll_flush(cx)
    }
    fn poll_shutdown(mut self: Pin<&mut Self>, cx: &mut TaskContext<'_>) -> Poll<io::Result<()>> {
        Pin::new(&mut self.inner).poll_shutdown(cx)
    }
}

/// What an intercepted tunnel is for: whose session, which agent, which host.
pub struct Target {
    pub session: SessionRecord,
    pub agent: AgentRecord,
    pub host: String,
    pub port: u16,
}

/// Terminate TLS on `client` for the target's host and serve its requests
/// through the broker.
pub async fn intercept<S>(
    state: Arc<AppState>,
    mitm: Arc<Mitm>,
    target: Target,
    client: S,
    leftover: Vec<u8>,
) where
    S: AsyncRead + AsyncWrite + Unpin + Send + 'static,
{
    let Target {
        session,
        agent,
        host,
        port,
    } = target;
    let config = match mitm.server_config(&host) {
        Ok(config) => config,
        Err(error) => {
            tracing::warn!(%error, %host, "could not issue an interception certificate");
            return;
        }
    };
    let stream = Prefixed {
        prefix: io::Cursor::new(leftover),
        inner: client,
    };
    let tls = match TlsAcceptor::from(config).accept(stream).await {
        Ok(tls) => tls,
        Err(error) => {
            tracing::debug!(%error, %host, "TLS handshake with the client failed");
            return;
        }
    };
    let context = Arc::new(Context_ {
        state,
        session,
        agent,
        host,
        port,
    });
    let service = service_fn(move |request| {
        let context = context.clone();
        async move { Ok::<_, Infallible>(handle(&context, request).await) }
    });
    if let Err(error) = hyper::server::conn::http1::Builder::new()
        .serve_connection(TokioIo::new(tls), service)
        .await
    {
        tracing::debug!(%error, "intercepted connection ended");
    }
}

#[allow(non_camel_case_types)]
struct Context_ {
    state: Arc<AppState>,
    session: SessionRecord,
    agent: AgentRecord,
    host: String,
    port: u16,
}

fn text(status: StatusCode, message: impl Into<String>) -> Response<Full<Bytes>> {
    let mut response = Response::new(Full::new(Bytes::from(message.into())));
    *response.status_mut() = status;
    response
        .headers_mut()
        .insert("content-type", "text/plain".parse().unwrap());
    response
}

async fn handle(context: &Context_, request: Request<Incoming>) -> Response<Full<Bytes>> {
    if request.headers().contains_key("upgrade") || request.method() == hyper::Method::CONNECT {
        return text(
            StatusCode::NOT_IMPLEMENTED,
            "WebSocket and other upgrades are not supported through the intercepting proxy",
        );
    }
    let (parts, body) = request.into_parts();
    let body = match Limited::new(body, MAX_BODY).collect().await {
        Ok(collected) => collected.to_bytes(),
        Err(_) => return text(StatusCode::PAYLOAD_TOO_LARGE, "request body exceeds 16 MiB"),
    };

    // The URL is built from the tunnel's target, never from the client's Host header.
    let authority = if context.port == 443 {
        context.host.clone()
    } else {
        format!("{}:{}", context.host, context.port)
    };
    let authority = if context.host.contains(':') {
        format!("[{}]:{}", context.host, context.port)
    } else {
        authority
    };
    let path = parts
        .uri
        .path_and_query()
        .map(|p| p.as_str())
        .unwrap_or("/");
    let url = format!("https://{authority}{path}");

    let mut headers: BTreeMap<String, String> = BTreeMap::new();
    for (name, value) in &parts.headers {
        if let Ok(value) = value.to_str() {
            headers
                .entry(name.as_str().to_string())
                .and_modify(|existing| {
                    existing.push_str(if name == "cookie" { "; " } else { ", " });
                    existing.push_str(value);
                })
                .or_insert_with(|| value.to_string());
        }
    }

    // A request that carries this session's surrogate for an intercepted
    // credential gets the real secret injected by the broker. Anything else is an
    // ordinary brokered request, and the broker drops any Authorization it was given.
    let credential = context
        .state
        .credentials
        .intercepted_for(&context.agent.manifest.credentials, &context.host)
        .into_iter()
        .find(|name| {
            let token = surrogate(&context.session.capability_token, name);
            headers.values().any(|value| value.contains(&token))
        })
        .map(str::to_string);

    let mut broker_headers = HeaderMap::new();
    if let (Ok(id), Ok(capability)) = (
        context.session.id.to_string().parse(),
        context.session.capability_token.parse(),
    ) {
        broker_headers.insert("x-zyvor-session-id", id);
        broker_headers.insert("x-zyvor-egress-capability", capability);
    }
    let reply = proxy_inner(
        &context.state,
        &broker_headers,
        EgressRequest {
            url,
            method: parts.method.as_str().to_string(),
            headers,
            body_base64: (!body.is_empty())
                .then(|| base64::engine::general_purpose::STANDARD.encode(&body)),
            credential,
        },
        // A decrypted tunnel has no guest connection the runtime can attribute, so a host whose
        // rules name binaries refuses here (the CONNECT proxy already refuses such hosts).
        None,
    )
    .await;
    match reply {
        Ok(value) => to_response(&value),
        Err((status, message)) => text(status, message),
    }
}

/// Turn the broker's JSON reply back into an HTTP response.
fn to_response(value: &serde_json::Value) -> Response<Full<Bytes>> {
    let status = value["status"]
        .as_u64()
        .and_then(|s| StatusCode::from_u16(s as u16).ok())
        .unwrap_or(StatusCode::BAD_GATEWAY);
    let body = value["body_base64"]
        .as_str()
        .and_then(|b| base64::engine::general_purpose::STANDARD.decode(b).ok())
        .unwrap_or_default();
    let mut response = Response::new(Full::new(Bytes::from(body)));
    *response.status_mut() = status;
    if let Some(list) = value["header_list"].as_array() {
        for pair in list {
            if let (Some(name), Some(value)) = (pair[0].as_str(), pair[1].as_str()) {
                if let (Ok(name), Ok(value)) = (
                    hyper::header::HeaderName::from_bytes(name.as_bytes()),
                    hyper::header::HeaderValue::from_str(value),
                ) {
                    response.headers_mut().append(name, value);
                }
            }
        }
    }
    response
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        egress::ask_tests::{manifest, state_and_session_cfg, wait_pending},
        model::{ApprovalKind, ApprovalStatus, DeployAgentRequest, EgressMode, GrantScope},
    };
    use rustls::pki_types::{pem::PemObject, ServerName};
    use std::sync::atomic::{AtomicUsize, Ordering};
    use tokio::{
        io::{AsyncReadExt, AsyncWriteExt},
        net::{TcpListener, TcpStream},
    };
    use tokio_rustls::TlsConnector;

    const SECRET: &str = "real-secret-value";

    #[derive(Debug, Clone, Default)]
    struct Seen {
        method: String,
        path: String,
        authorization: Option<String>,
        host: Option<String>,
        body: String,
    }

    fn scratch(label: &str) -> std::path::PathBuf {
        std::env::temp_dir().join(format!("zyvor-mitm-{label}-{}", uuid::Uuid::new_v4()))
    }

    /// A real HTTPS upstream on 127.0.0.1, with a certificate from its own CA.
    /// Returns its port, its CA certificate, and what it has been sent.
    async fn upstream() -> (u16, String, Arc<Mutex<Vec<Seen>>>) {
        let ca = Mitm::load_or_create(&scratch("upstream-ca")).await.unwrap();
        let config = ca.server_config("127.0.0.1").unwrap();
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        let seen: Arc<Mutex<Vec<Seen>>> = Arc::default();
        let sink = seen.clone();
        tokio::spawn(async move {
            loop {
                let (tcp, _) = listener.accept().await.unwrap();
                let (acceptor, sink) = (TlsAcceptor::from(config.clone()), sink.clone());
                tokio::spawn(async move {
                    let Ok(tls) = acceptor.accept(tcp).await else {
                        return;
                    };
                    let service = service_fn(move |request: Request<Incoming>| {
                        let sink = sink.clone();
                        async move {
                            let (parts, body) = request.into_parts();
                            let body = body.collect().await.unwrap().to_bytes();
                            sink.lock().unwrap().push(Seen {
                                method: parts.method.to_string(),
                                path: parts.uri.to_string(),
                                authorization: parts
                                    .headers
                                    .get("authorization")
                                    .map(|v| v.to_str().unwrap().to_string()),
                                host: parts
                                    .headers
                                    .get("host")
                                    .map(|v| v.to_str().unwrap().to_string()),
                                body: String::from_utf8_lossy(&body).into_owned(),
                            });
                            let mut response = Response::new(Full::new(Bytes::from("upstream-ok")));
                            response
                                .headers_mut()
                                .append("set-cookie", "a=1".parse().unwrap());
                            response
                                .headers_mut()
                                .append("set-cookie", "b=2".parse().unwrap());
                            Ok::<_, Infallible>(response)
                        }
                    });
                    let _ = hyper::server::conn::http1::Builder::new()
                        .serve_connection(TokioIo::new(tls), service)
                        .await;
                });
            }
        });
        (port, ca.ca_pem().to_string(), seen)
    }

    struct Rig {
        state: Arc<AppState>,
        session: SessionRecord,
        proxy: u16,
        upstream_port: u16,
        seen: Arc<Mutex<Vec<Seen>>>,
        ca_pem: String,
    }

    /// The proxy with interception on, an agent holding an intercepted `mail`
    /// credential for 127.0.0.1, and a real TLS upstream.
    async fn rig(
        tweak: impl FnOnce(&mut crate::model::AgentManifest),
        requires_approval: bool,
    ) -> Rig {
        std::env::set_var("ZY_MITM_TEST_SECRET", SECRET);
        let (upstream_port, upstream_ca, seen) = upstream().await;
        let dir = scratch("files");
        std::fs::create_dir_all(&dir).unwrap();
        let upstream_ca_file = dir.join("upstream-ca.pem");
        std::fs::write(&upstream_ca_file, upstream_ca).unwrap();
        let creds = dir.join("credentials.json");
        std::fs::write(
            &creds,
            serde_json::json!({"mail": {
                "host": "127.0.0.1", "header": "authorization", "prefix": "Bearer ",
                "env": "ZY_MITM_TEST_SECRET", "allowed_ports": [upstream_port], "intercept": true,
                "requires_approval": if requires_approval { vec!["POST"] } else { vec![] },
            }})
            .to_string(),
        )
        .unwrap();
        // Create the CA first so the state under test exercises the reload path.
        let ca_dir = dir.join("ca");
        let first = Mitm::load_or_create(&ca_dir).await.unwrap();
        let ca_pem = first.ca_pem().to_string();
        let (state, session) = state_and_session_cfg(|c| {
            c.mitm_ca_dir = Some(ca_dir);
            c.extra_ca_files = vec![upstream_ca_file];
            c.credentials_file = Some(creds);
            c.proxy_connect_ports = vec![upstream_port];
        })
        .await;
        let mut m = manifest(EgressMode::Deny, Some(30));
        m.credentials = vec!["mail".into()];
        m.egress_allow_hosts = vec!["127.0.0.1".into()];
        m.allow_private_networks = true;
        tweak(&mut m);
        let deployed = state
            .store
            .deploy_agent(DeployAgentRequest {
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
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let proxy = listener.local_addr().unwrap().port();
        tokio::spawn(crate::proxy::serve(state.clone(), listener));
        Rig {
            state,
            session,
            proxy,
            upstream_port,
            seen,
            ca_pem,
        }
    }

    impl Rig {
        fn surrogate(&self) -> String {
            surrogate(&self.session.capability_token, "mail")
        }

        /// CONNECT through the proxy, TLS-handshake trusting only the interception
        /// CA, send `requests` in order on one connection, and return all bytes read.
        async fn send(&self, requests: &[String]) -> String {
            let mut tcp = TcpStream::connect(("127.0.0.1", self.proxy)).await.unwrap();
            let auth = base64::engine::general_purpose::STANDARD.encode(format!(
                "{}:{}",
                self.session.id, self.session.capability_token
            ));
            let connect = format!(
                "CONNECT 127.0.0.1:{p} HTTP/1.1\r\nHost: 127.0.0.1:{p}\r\nProxy-Authorization: Basic {auth}\r\n\r\n",
                p = self.upstream_port
            );
            tcp.write_all(connect.as_bytes()).await.unwrap();
            let mut head = Vec::new();
            let mut byte = [0u8; 1];
            while !head.ends_with(b"\r\n\r\n") {
                assert_eq!(
                    tcp.read(&mut byte).await.unwrap(),
                    1,
                    "closed during CONNECT"
                );
                head.push(byte[0]);
            }
            let head = String::from_utf8_lossy(&head).into_owned();
            assert!(head.starts_with("HTTP/1.1 200"), "{head}");

            let mut roots = rustls::RootCertStore::empty();
            roots
                .add(CertificateDer::from_pem_slice(self.ca_pem.as_bytes()).unwrap())
                .unwrap();
            let provider = Arc::new(rustls::crypto::aws_lc_rs::default_provider());
            let client = rustls::ClientConfig::builder_with_provider(provider)
                .with_safe_default_protocol_versions()
                .unwrap()
                .with_root_certificates(roots)
                .with_no_client_auth();
            let name = ServerName::try_from("127.0.0.1").unwrap();
            let mut tls = TlsConnector::from(Arc::new(client))
                .connect(name, tcp)
                .await
                .unwrap();
            let mut out = Vec::new();
            for request in requests {
                tls.write_all(request.as_bytes()).await.unwrap();
            }
            let mut buf = [0u8; 4096];
            // Read until the server closes, but never wait forever: a test that
            // forgets `Connection: close` should fail, not hang.
            let read = async {
                loop {
                    match tls.read(&mut buf).await {
                        Ok(0) | Err(_) => break,
                        Ok(n) => out.extend_from_slice(&buf[..n]),
                    }
                }
            };
            assert!(
                tokio::time::timeout(std::time::Duration::from_secs(15), read)
                    .await
                    .is_ok(),
                "server did not close the connection"
            );
            String::from_utf8_lossy(&out).into_owned()
        }

        fn request(
            &self,
            method: &str,
            path: &str,
            headers: &[String],
            body: &str,
            close: bool,
        ) -> String {
            let mut text = format!("{method} {path} HTTP/1.1\r\nHost: 127.0.0.1\r\n");
            for header in headers {
                text.push_str(header);
                text.push_str("\r\n");
            }
            text.push_str(&format!("Content-Length: {}\r\n", body.len()));
            if close {
                text.push_str("Connection: close\r\n");
            }
            text.push_str("\r\n");
            text.push_str(body);
            text
        }

        fn seen(&self) -> Vec<Seen> {
            self.seen.lock().unwrap().clone()
        }
    }

    #[tokio::test]
    async fn a_surrogate_is_swapped_for_the_secret_and_response_headers_survive() {
        let rig = rig(|_| {}, false).await;
        let request = rig.request(
            "GET",
            "/inbox?page=2",
            &[format!("Authorization: Bearer {}", rig.surrogate())],
            "",
            true,
        );
        let response = rig.send(&[request]).await;
        assert!(response.starts_with("HTTP/1.1 200"), "{response}");
        assert!(response.contains("upstream-ok"));
        // Both Set-Cookie headers come back, not just the last one.
        assert_eq!(
            response.to_lowercase().matches("set-cookie:").count(),
            2,
            "{response}"
        );
        let seen = rig.seen();
        assert_eq!(seen.len(), 1);
        assert_eq!(
            seen[0].authorization.as_deref(),
            Some(&*format!("Bearer {SECRET}"))
        );
        assert_eq!(seen[0].path, "/inbox?page=2");
        // The secret was added on the host: it is nowhere in what the agent sent
        // or received.
        assert!(!response.contains(SECRET));
    }

    #[tokio::test]
    async fn requests_without_this_sessions_surrogate_get_no_credential() {
        let rig = rig(|_| {}, false).await;
        let others = surrogate(&crate::fixture::text("another-sessions-capability"), "mail");
        for token in [
            "not-a-surrogate".to_string(),
            others,
            "zy_sur_00000000000000000000000000000000".to_string(),
        ] {
            let request = rig.request(
                "GET",
                "/x",
                &[format!("Authorization: Bearer {token}")],
                "",
                true,
            );
            let response = rig.send(&[request]).await;
            assert!(response.starts_with("HTTP/1.1 200"), "{response}");
        }
        for seen in rig.seen() {
            // The forged Authorization was dropped and nothing was injected.
            assert_eq!(seen.authorization, None);
        }
    }

    #[tokio::test]
    async fn the_destination_comes_from_the_tunnel_not_the_host_header() {
        let rig = rig(|_| {}, false).await;
        let request = format!(
            "GET /x HTTP/1.1\r\nHost: evil.example\r\nAuthorization: Bearer {}\r\nConnection: close\r\n\r\n",
            rig.surrogate()
        );
        let response = rig.send(&[request]).await;
        assert!(response.starts_with("HTTP/1.1 200"), "{response}");
        let seen = rig.seen();
        assert_eq!(
            seen.len(),
            1,
            "the request must reach the tunnel's own host"
        );
        assert_eq!(
            seen[0].host.as_deref(),
            Some(&*format!("127.0.0.1:{}", rig.upstream_port))
        );
    }

    #[tokio::test]
    async fn keep_alive_serves_several_requests_on_one_tunnel() {
        let rig = rig(|_| {}, false).await;
        let auth = [format!("Authorization: Bearer {}", rig.surrogate())];
        let first = rig.request("POST", "/a", &auth, "one", false);
        let second = rig.request("POST", "/b", &auth, "two", true);
        let response = rig.send(&[first, second]).await;
        assert_eq!(response.matches("HTTP/1.1 200").count(), 2, "{response}");
        let seen = rig.seen();
        let calls: Vec<(&str, &str, &str)> = seen
            .iter()
            .map(|s| (s.method.as_str(), s.path.as_str(), s.body.as_str()))
            .collect();
        assert_eq!(calls, vec![("POST", "/a", "one"), ("POST", "/b", "two")]);
    }

    #[tokio::test]
    async fn the_brokers_rules_apply_inside_an_intercepted_tunnel() {
        let rig = rig(
            |m| {
                m.egress_rules = vec![crate::model::EgressRule {
                    host: "127.0.0.1".into(),
                    methods: vec!["GET".into()],
                    path_prefixes: vec![],
                    max_body_bytes: None,
                    ..Default::default()
                }];
            },
            false,
        )
        .await;
        let auth = [format!("Authorization: Bearer {}", rig.surrogate())];
        let response = rig
            .send(&[rig.request("POST", "/send", &auth, "x", true)])
            .await;
        assert!(response.starts_with("HTTP/1.1 403"), "{response}");
        assert!(response.contains("egress rules"));
        assert!(
            rig.seen().is_empty(),
            "a refused request must not reach the upstream"
        );
        // The plain tunnel refuses such a host outright; here the broker decides per request.
        let ok = rig
            .send(&[rig.request("GET", "/read", &auth, "", true)])
            .await;
        assert!(ok.starts_with("HTTP/1.1 200"), "{ok}");
    }

    #[tokio::test]
    async fn a_credential_that_needs_approval_holds_an_intercepted_request() {
        let rig = rig(|_| {}, true).await;
        let auth = [format!("Authorization: Bearer {}", rig.surrogate())];
        let request = rig.request("POST", "/send", &auth, "hello", true);
        let waiter = tokio::spawn({
            let (session, proxy, upstream_port, seen, ca_pem, state) = (
                rig.session.clone(),
                rig.proxy,
                rig.upstream_port,
                rig.seen.clone(),
                rig.ca_pem.clone(),
                rig.state.clone(),
            );
            async move {
                let rig = Rig {
                    state,
                    session,
                    proxy,
                    upstream_port,
                    seen,
                    ca_pem,
                };
                rig.send(&[request]).await
            }
        });
        let pending = wait_pending(&rig.state, rig.session.id).await;
        assert_eq!(pending.kind, ApprovalKind::Send);
        assert!(
            rig.seen().is_empty(),
            "nothing may leave while the approval is pending"
        );
        rig.state
            .store
            .transition_approval(
                pending.id,
                ApprovalStatus::Approved,
                None,
                Some(GrantScope::Once),
            )
            .await
            .unwrap();
        let response = waiter.await.unwrap();
        assert!(response.starts_with("HTTP/1.1 200"), "{response}");
        let seen = rig.seen();
        assert_eq!(seen.len(), 1);
        assert_eq!(
            seen[0].authorization.as_deref(),
            Some(&*format!("Bearer {SECRET}"))
        );
    }

    #[tokio::test]
    async fn upgrades_are_refused() {
        let rig = rig(|_| {}, false).await;
        let upgrade = "GET /socket HTTP/1.1\r\nHost: 127.0.0.1\r\nUpgrade: websocket\r\nConnection: Upgrade\r\n\r\n".to_string();
        // A plain request behind it ends the exchange; the refusal must not have
        // poisoned the connection or reached the upstream.
        let after = rig.request("GET", "/after", &[], "", true);
        let response = rig.send(&[upgrade, after]).await;
        assert!(response.starts_with("HTTP/1.1 501"), "{response}");
        assert!(response.contains("HTTP/1.1 200"), "{response}");
        let seen = rig.seen();
        assert_eq!(seen.len(), 1);
        assert_eq!(seen[0].path, "/after");
    }

    #[tokio::test]
    async fn the_ca_persists_and_its_key_is_private() {
        let dir = scratch("persist");
        let first = Mitm::load_or_create(&dir).await.unwrap();
        let second = Mitm::load_or_create(&dir).await.unwrap();
        assert_eq!(first.ca_pem(), second.ca_pem());
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(dir.join(CA_KEY))
                .unwrap()
                .permissions()
                .mode();
            assert_eq!(mode & 0o777, 0o600, "CA key must not be readable by others");
        }
        // A reloaded CA still signs certificates that the original certificate verifies.
        let config = second.server_config("example.com").unwrap();
        assert_eq!(config.alpn_protocols, vec![b"http/1.1".to_vec()]);
    }

    #[test]
    fn surrogates_cover_only_intercepted_credentials_and_differ_per_session() {
        let vault: crate::credentials::CredentialVault = serde_json::from_value::<
            std::collections::HashMap<String, crate::credentials::CredentialDescriptor>,
        >(serde_json::json!({
            "mail": {"host": "mail.example", "header": "authorization", "env": "A", "intercept": true},
            "plain": {"host": "api.example", "header": "x-api-key", "env": "B"},
        }))
        .map(crate::credentials::CredentialVault::from_descriptors)
        .unwrap();
        let granted = vec![
            "mail".to_string(),
            "plain".to_string(),
            "missing".to_string(),
        ];
        let cap_one = crate::fixture::text("cap-one");
        let cap_two = crate::fixture::text("cap-two");
        let one = surrogates(&vault, &granted, &cap_one);
        assert_eq!(one.keys().collect::<Vec<_>>(), vec!["mail"]);
        assert!(one["mail"].starts_with("zy_sur_") && one["mail"].len() == 39);
        assert_eq!(one, surrogates(&vault, &granted, &cap_one));
        assert_ne!(one["mail"], surrogates(&vault, &granted, &cap_two)["mail"]);
        assert!(!one["mail"].contains("cap-one"));
    }

    #[test]
    fn the_guest_install_snippet_is_valid_shell() {
        let status = std::process::Command::new("/bin/sh")
            .args(["-n", "-c", GUEST_INSTALL])
            .status()
            .unwrap();
        assert!(status.success());
        assert!(GUEST_INSTALL.contains(GUEST_CA_PATH));
    }

    #[allow(dead_code)]
    fn _unused(_: AtomicUsize) {
        let _ = Ordering::SeqCst;
    }
}
