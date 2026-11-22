// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: LicenseRef-Zyvor-Production-2.0

//! Egress guard: an operator-run HTTP service that may refuse a brokered request.
//!
//! When `ZYVOR_AGENT_GUARD_URL` is set, every request through the JSON broker is described to the
//! guard after the agent's own rules have passed and before an operator is asked or a credential
//! is added, so the guard never sees a secret the runtime injects. The guard answers `allow` or
//! `deny`. It can only narrow what policy allows, never widen it, and it fails closed: a timeout,
//! an error status, an unreadable answer, or anything but an explicit `allow` refuses the request.
//!
//! What it is sent: the agent, session, method, host, path (no query string or credentials), the
//! names of the headers the agent set (not their values), the DLP shapes found, and the body up
//! to [`MAX_BODY_SENT`] bytes (larger bodies are described by size and SHA-256 only).
//!
//! The CONNECT proxy sees only `host:port`, so it cannot be described to a guard; use
//! `egress_rules` on a host to keep it off the proxy.

use anyhow::{bail, Context, Result};
use base64::Engine;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::time::Duration;

/// Bodies up to this size are sent to the guard in full.
pub const MAX_BODY_SENT: usize = 256 * 1024;
const MAX_ANSWER: usize = 64 * 1024;
const MAX_REASON_CHARS: usize = 200;

#[derive(Clone, Debug)]
pub struct GuardConfig {
    pub url: reqwest::Url,
    /// Sent as `Authorization: Bearer`. Only allowed over https or to a loopback host.
    pub token: Option<String>,
    pub timeout: Duration,
}

impl GuardConfig {
    pub fn new(url: &str, token: Option<String>, timeout: Duration) -> Result<Self> {
        let url = reqwest::Url::parse(url).context("ZYVOR_AGENT_GUARD_URL is not a URL")?;
        if !matches!(url.scheme(), "http" | "https") {
            bail!("ZYVOR_AGENT_GUARD_URL must be http or https");
        }
        let loopback = match url.host() {
            Some(url::Host::Domain(d)) => d.eq_ignore_ascii_case("localhost"),
            Some(url::Host::Ipv4(ip)) => ip.is_loopback(),
            Some(url::Host::Ipv6(ip)) => ip.is_loopback(),
            None => false,
        };
        if token.is_some() && url.scheme() != "https" && !loopback {
            bail!("ZYVOR_AGENT_GUARD_TOKEN would travel in clear text: use an https ZYVOR_AGENT_GUARD_URL or a loopback host");
        }
        Ok(Self {
            url,
            token,
            timeout,
        })
    }
}

/// What the guard is told about one request.
pub struct GuardRequest<'a> {
    pub session_id: uuid::Uuid,
    pub agent: &'a str,
    pub method: &'a str,
    /// Scheme, host and path only.
    pub url: &'a str,
    pub host: &'a str,
    pub path: &'a str,
    pub header_names: Vec<String>,
    pub dlp_hits: &'a [&'static str],
    pub body: &'a [u8],
}

#[derive(Debug, PartialEq, Eq)]
pub enum Decision {
    Allow,
    Deny(String),
}

fn describe(req: &GuardRequest<'_>) -> Value {
    let mut v = json!({
        "version": 1,
        "session_id": req.session_id,
        "agent": req.agent,
        "method": req.method,
        "url": req.url,
        "host": req.host,
        "path": req.path,
        "header_names": req.header_names,
        "dlp": req.dlp_hits,
        "body_bytes": req.body.len(),
        "body_sha256": hex::encode(Sha256::digest(req.body)),
    });
    if req.body.len() <= MAX_BODY_SENT {
        v["body_base64"] = json!(base64::engine::general_purpose::STANDARD.encode(req.body));
    } else {
        v["body_omitted"] = json!(true);
    }
    v
}

fn clean_reason(raw: &str) -> String {
    let s: String = raw
        .chars()
        .filter(|c| !c.is_control())
        .take(MAX_REASON_CHARS)
        .collect();
    if s.trim().is_empty() {
        "no reason given".into()
    } else {
        s
    }
}

/// Ask the guard. Never errors: every failure is a [`Decision::Deny`].
pub async fn check(
    http: &reqwest::Client,
    config: &GuardConfig,
    request: &GuardRequest<'_>,
) -> Decision {
    match ask(http, config, request).await {
        Ok(decision) => decision,
        Err(error) => {
            tracing::warn!(%error, "egress guard unavailable; refusing the request");
            Decision::Deny("the egress guard could not be reached or gave no valid answer".into())
        }
    }
}

async fn ask(
    http: &reqwest::Client,
    config: &GuardConfig,
    request: &GuardRequest<'_>,
) -> Result<Decision> {
    let mut call = http
        .post(config.url.clone())
        .timeout(config.timeout)
        .json(&describe(request));
    if let Some(token) = &config.token {
        call = call.bearer_auth(token);
    }
    let mut response = call.send().await.context("guard request failed")?;
    if response.status() != reqwest::StatusCode::OK {
        bail!("guard answered {}", response.status());
    }
    let mut answer = Vec::new();
    while let Some(chunk) = response
        .chunk()
        .await
        .context("reading the guard's answer")?
    {
        answer.extend_from_slice(&chunk);
        if answer.len() > MAX_ANSWER {
            bail!("the guard's answer is larger than {MAX_ANSWER} bytes");
        }
    }
    let value: Value = serde_json::from_slice(&answer).context("the guard's answer is not JSON")?;
    match value.get("decision").and_then(Value::as_str) {
        Some("allow") => Ok(Decision::Allow),
        Some("deny") => Ok(Decision::Deny(clean_reason(
            value.get("reason").and_then(Value::as_str).unwrap_or(""),
        ))),
        other => bail!("the guard's decision is {other:?}, not allow or deny"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{extract::State, http::HeaderMap, routing::post, Router};
    use std::sync::{Arc, Mutex};

    type Seen = Arc<Mutex<Vec<(Option<String>, Value)>>>;

    /// A guard that answers with `status` and `body`, recording what it was sent.
    async fn guard_server(status: u16, body: String) -> (String, Seen) {
        let seen: Seen = Arc::default();
        let app = Router::new()
            .route(
                "/check",
                post(
                    |State((seen, status, body)): State<(Seen, u16, String)>,
                     headers: HeaderMap,
                     bytes: axum::body::Bytes| async move {
                        let auth = headers
                            .get("authorization")
                            .and_then(|v| v.to_str().ok())
                            .map(str::to_string);
                        seen.lock()
                            .unwrap()
                            .push((auth, serde_json::from_slice(&bytes).unwrap_or(Value::Null)));
                        (axum::http::StatusCode::from_u16(status).unwrap(), body)
                    },
                ),
            )
            .with_state((seen.clone(), status, body));
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        (format!("http://{addr}/check"), seen)
    }

    fn client() -> reqwest::Client {
        reqwest::Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .unwrap()
    }

    fn cfg(url: &str) -> GuardConfig {
        GuardConfig::new(url, None, Duration::from_secs(2)).unwrap()
    }

    fn req<'a>(body: &'a [u8]) -> GuardRequest<'a> {
        GuardRequest {
            session_id: uuid::Uuid::nil(),
            agent: "desk",
            method: "POST",
            url: "https://api.example.com/v1/x",
            host: "api.example.com",
            path: "/v1/x",
            header_names: vec!["content-type".into()],
            dlp_hits: &["aws-access-key"],
            body,
        }
    }

    #[tokio::test]
    async fn allow_and_deny_are_read_and_the_request_is_described() {
        let (url, seen) = guard_server(200, r#"{"decision":"allow"}"#.into()).await;
        assert_eq!(
            check(&client(), &cfg(&url), &req(b"hello")).await,
            Decision::Allow
        );
        let (auth, sent) = seen.lock().unwrap()[0].clone();
        assert!(auth.is_none());
        assert_eq!(sent["version"], 1);
        assert_eq!(sent["agent"], "desk");
        assert_eq!(sent["method"], "POST");
        assert_eq!(sent["host"], "api.example.com");
        assert_eq!(sent["path"], "/v1/x");
        assert_eq!(sent["header_names"], json!(["content-type"]));
        assert_eq!(sent["dlp"], json!(["aws-access-key"]));
        assert_eq!(sent["body_bytes"], 5);
        assert_eq!(
            base64::engine::general_purpose::STANDARD
                .decode(sent["body_base64"].as_str().unwrap())
                .unwrap(),
            b"hello"
        );
        assert_eq!(sent["body_sha256"], hex::encode(Sha256::digest(b"hello")));

        let (url, _) = guard_server(
            200,
            "{\"decision\":\"deny\",\"reason\":\"no\\nexfil\\u001b[31m\"}".into(),
        )
        .await;
        assert_eq!(
            check(&client(), &cfg(&url), &req(b"")).await,
            Decision::Deny("noexfil[31m".into())
        );
    }

    #[tokio::test]
    async fn everything_but_an_explicit_allow_is_a_refusal() {
        for (status, body) in [
            (200, r#"{"decision":"maybe"}"#),
            (200, r#"{"decision":"ALLOW"}"#),
            (200, r#"{"allow":true}"#),
            (200, "not json"),
            (200, ""),
            (500, r#"{"decision":"allow"}"#),
            (204, ""),
            (302, r#"{"decision":"allow"}"#),
        ] {
            let (url, _) = guard_server(status, body.into()).await;
            assert!(
                matches!(
                    check(&client(), &cfg(&url), &req(b"x")).await,
                    Decision::Deny(_)
                ),
                "{status} {body:?} must refuse"
            );
        }
    }

    #[tokio::test]
    async fn an_unreachable_slow_or_oversized_guard_refuses() {
        assert!(matches!(
            check(&client(), &cfg("http://127.0.0.1:1/check"), &req(b"")).await,
            Decision::Deny(_)
        ));
        let huge = format!(
            r#"{{"decision":"allow","pad":"{}"}}"#,
            "a".repeat(MAX_ANSWER + 10)
        );
        let (url, _) = guard_server(200, huge).await;
        assert!(matches!(
            check(&client(), &cfg(&url), &req(b"")).await,
            Decision::Deny(_)
        ));

        // A listener that accepts and never answers.
        let stall = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = stall.local_addr().unwrap();
        tokio::spawn(async move {
            let mut held = Vec::new();
            loop {
                if let Ok((s, _)) = stall.accept().await {
                    held.push(s);
                }
            }
        });
        let slow = GuardConfig::new(
            &format!("http://{addr}/check"),
            None,
            Duration::from_millis(100),
        )
        .unwrap();
        assert!(matches!(
            check(&client(), &slow, &req(b"")).await,
            Decision::Deny(_)
        ));
    }

    #[tokio::test]
    async fn a_large_body_is_described_not_sent_and_the_token_is_a_bearer() {
        let (url, seen) = guard_server(200, r#"{"decision":"allow"}"#.into()).await;
        let config = GuardConfig::new(&url, Some("tok".into()), Duration::from_secs(2)).unwrap();
        let body = vec![b'x'; MAX_BODY_SENT + 1];
        assert_eq!(
            check(&client(), &config, &req(&body)).await,
            Decision::Allow
        );
        let (auth, sent) = seen.lock().unwrap()[0].clone();
        assert_eq!(auth.as_deref(), Some("Bearer tok"));
        assert_eq!(sent["body_omitted"], true);
        assert!(sent.get("body_base64").is_none());
        assert_eq!(sent["body_bytes"], MAX_BODY_SENT + 1);
    }

    #[test]
    fn config_rejects_a_bad_url_and_a_token_in_clear_text() {
        let t = Duration::from_secs(1);
        assert!(GuardConfig::new("not a url", None, t).is_err());
        assert!(GuardConfig::new("ftp://guard.example/x", None, t).is_err());
        assert!(GuardConfig::new("http://guard.example/x", Some("t".into()), t).is_err());
        assert!(GuardConfig::new("http://guard.example/x", None, t).is_ok());
        assert!(GuardConfig::new("https://guard.example/x", Some("t".into()), t).is_ok());
        assert!(GuardConfig::new("http://127.0.0.1:9/x", Some("t".into()), t).is_ok());
        assert!(GuardConfig::new("http://localhost:9/x", Some("t".into()), t).is_ok());
        assert!(GuardConfig::new("http://[::1]:9/x", Some("t".into()), t).is_ok());
    }
}
