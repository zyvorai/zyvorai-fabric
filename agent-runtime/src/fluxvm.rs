// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: Apache-2.0

use crate::model::Resources;
use anyhow::{bail, Context, Result};
use base64::Engine;
use reqwest::{Method, Url};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use uuid::Uuid;

#[derive(Clone)]
pub struct FluxVm {
    base: Url,
    http: reqwest::Client,
    token: Option<String>,
    /// Bounds how many `POST /v1/sandboxes` are in flight at once. Guestkit ≥1.2.5
    /// serializes nbd allocate+connect with flock (fluxvm#104), so concurrent creates
    /// are safe. Default is 4; override with `ZYVOR_AGENT_SANDBOX_CREATE_CONCURRENCY`
    /// (1–64). A cell that is already running is not affected.
    create_gate: std::sync::Arc<tokio::sync::Semaphore>,
}

/// Default concurrent sandbox creations (safe after guestkit flock fix).
const DEFAULT_CREATE_CONCURRENCY: usize = 4;

/// 1 to 64; anything unset, unparsable or below 1 falls back to the default.
fn parse_create_concurrency(value: Option<&str>) -> usize {
    value
        .and_then(|v| v.trim().parse::<usize>().ok())
        .map(|n| n.clamp(1, 64))
        .unwrap_or(DEFAULT_CREATE_CONCURRENCY)
}

fn create_concurrency() -> usize {
    parse_create_concurrency(
        std::env::var("ZYVOR_AGENT_SANDBOX_CREATE_CONCURRENCY")
            .ok()
            .as_deref(),
    )
}

#[derive(Debug, Clone, Deserialize)]
pub struct SandboxRecord {
    pub id: Uuid,
    /// Set by the local simulator (`agent-runtime/sim/`, `scripts/keep-demo-local.sh`), never by FluxVM. A simulated cell runs the
    /// extractors on the operator's own machine with no VM and no network policy, so a run in it must not claim to be sealed.
    #[serde(default)]
    pub simulated: bool,
    #[serde(default)]
    pub guest_ip: Option<String>,
    #[serde(default)]
    pub status: Option<String>,
    /// Present when the create request asked for `confidential`. An older FluxVM
    /// ignores the request and omits this.
    #[serde(default)]
    pub confidential: Option<crate::model::ConfidentialStatus>,
    /// The create request FluxVM ran. Its `vfio_devices` are the GPUs it assigned.
    #[serde(default)]
    pub request: Option<SandboxRequest>,
}

/// The part of FluxVM's stored create request the runtime reads back.
#[derive(Debug, Clone, Deserialize, Default, PartialEq, Eq)]
pub struct SandboxRequest {
    #[serde(default)]
    pub vfio_devices: Vec<String>,
}

/// FluxVM `GET /v1/security/capabilities` (Phase 6 HostCapabilities subset).
#[derive(Debug, Clone, Deserialize, Default, PartialEq, Eq)]
pub struct HostSecurityCapabilities {
    #[serde(default)]
    pub snp_present: bool,
    #[serde(default)]
    pub tdx_present: bool,
    #[serde(default)]
    pub snp_launch_verified: bool,
    #[serde(default)]
    pub tdx_launch_verified: bool,
}

#[derive(Debug, Serialize)]
struct SandboxCreate<'a> {
    name: String,
    template: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    ttl_seconds: Option<u64>,
    http_proxy_port: u16,
    #[serde(skip_serializing_if = "<[SandboxVolume]>::is_empty")]
    volumes: &'a [SandboxVolume],
    #[serde(skip_serializing_if = "Option::is_none")]
    vcpus: Option<u8>,
    #[serde(skip_serializing_if = "Option::is_none")]
    memory_mib: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    confidential: Option<&'static str>,
    /// FluxVM Phase 6 profile (`measured`, …). Older FluxVM ignores unknown fields.
    #[serde(skip_serializing_if = "Option::is_none")]
    security_profile: Option<&'a str>,
    /// Free GPUs to pass through. An older FluxVM ignores it, so the caller checks the record.
    #[serde(skip_serializing_if = "Option::is_none")]
    gpus: Option<u8>,
}

/// A FluxVM sandbox volume (`POST /v1/sandboxes` `volumes`).
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct SandboxVolume {
    pub name: String,
    pub guest_path: String,
}

/// A speculative run and its decision state (`/v1/sandboxes/{id}/changesets`).
/// Only the fields Keep shows or acts on are typed; `changes` and
/// `side_effects` stay raw JSON so a FluxVM that adds fields keeps working.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Changeset {
    pub id: Uuid,
    pub sandbox_id: Uuid,
    /// `pending`, `approved`, `applied`, `rejected`, `expired` or `failed`.
    pub state: String,
    #[serde(default)]
    pub expires_at: u64,
    pub command: String,
    #[serde(default)]
    pub exit_code: i32,
    #[serde(default)]
    pub stdout: String,
    #[serde(default)]
    pub stderr: String,
    #[serde(default)]
    pub paths: Vec<String>,
    #[serde(default)]
    pub changes: Value,
    #[serde(default)]
    pub side_effects: Value,
    /// Changed files whose contents could not be kept; FluxVM refuses to apply while any.
    #[serde(default)]
    pub unstaged: Vec<String>,
    #[serde(default)]
    pub error: Option<String>,
}

/// What to run speculatively. `command` is passed to the guest as given.
#[derive(Debug, Clone, Serialize)]
pub struct SpeculateRequest {
    pub command: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timeout_seconds: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub paths: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ttl_seconds: Option<u64>,
}

/// What a sandbox gets beyond its template: volumes, size, confidential launch.
#[derive(Debug, Default)]
pub struct SandboxOptions<'a> {
    pub volumes: &'a [SandboxVolume],
    pub resources: Option<Resources>,
    pub confidential: crate::model::Confidential,
    /// Phase 6 security profile name (e.g. `measured`). Evidence class stays
    /// `software-test` until Keep 0.2 + attested hardware.
    pub security_profile: Option<&'a str>,
    /// How many GPUs FluxVM should pass through. See `AgentManifest::gpus`.
    pub gpus: Option<u8>,
}

impl FluxVm {
    pub fn new(base: &str, token: Option<String>) -> Result<Self> {
        let base = Url::parse(base).with_context(|| format!("invalid FluxVM URL: {base}"))?;
        let http = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(180))
            .redirect(reqwest::redirect::Policy::none())
            .build()?;
        Ok(Self {
            base,
            http,
            token,
            create_gate: std::sync::Arc::new(tokio::sync::Semaphore::new(create_concurrency())),
        })
    }

    fn url(&self, path: &str) -> Result<Url> {
        self.base
            .join(path)
            .with_context(|| format!("joining FluxVM URL with {path}"))
    }

    fn auth(&self, b: reqwest::RequestBuilder) -> reqwest::RequestBuilder {
        match &self.token {
            Some(token) => b.bearer_auth(token),
            None => b,
        }
    }

    async fn parse<T: serde::de::DeserializeOwned>(
        &self,
        response: reqwest::Response,
    ) -> Result<T> {
        let status = response.status();
        let bytes = response.bytes().await?;
        if !status.is_success() {
            let detail = String::from_utf8_lossy(&bytes);
            bail!("FluxVM returned {status}: {detail}");
        }
        serde_json::from_slice(&bytes).with_context(|| {
            format!(
                "decoding FluxVM response: {}",
                String::from_utf8_lossy(&bytes)
            )
        })
    }

    pub async fn get_sandbox(&self, id: Uuid) -> Result<SandboxRecord> {
        let response = self
            .auth(self.http.get(self.url(&format!("/v1/vms/{id}"))?))
            .send()
            .await?;
        self.parse(response).await
    }

    /// Host SNP/TDX capability + launch-verified flags (fail-closed when unreachable).
    pub async fn security_capabilities(&self) -> Result<HostSecurityCapabilities> {
        let response = self
            .auth(self.http.get(self.url("/v1/security/capabilities")?))
            .send()
            .await?;
        self.parse(response).await
    }

    pub async fn create_sandbox(
        &self,
        name: String,
        template: &str,
        ttl_seconds: Option<u64>,
        runtime_port: u16,
        options: &SandboxOptions<'_>,
    ) -> Result<SandboxRecord> {
        // Held until FluxVM answers, so provisioning never overlaps (see `create_gate`).
        let _permit = self
            .create_gate
            .acquire()
            .await
            .context("the sandbox create gate was closed")?;
        let response = self
            .auth(self.http.post(self.url("/v1/sandboxes")?))
            .json(&SandboxCreate {
                name,
                template,
                ttl_seconds,
                http_proxy_port: runtime_port,
                volumes: options.volumes,
                vcpus: options.resources.map(|r| r.vcpus),
                memory_mib: options.resources.map(|r| r.memory_mib),
                confidential: (!options.confidential.is_off())
                    .then_some(options.confidential.as_str()),
                security_profile: options.security_profile,
                gpus: options.gpus,
            })
            .send()
            .await?;
        self.parse(response).await
    }

    pub async fn fs_write(&self, id: Uuid, path: &str, bytes: &[u8], mode: u32) -> Result<()> {
        let response = self
            .auth(
                self.http
                    .post(self.url(&format!("/v1/sandboxes/{id}/fs/write"))?),
            )
            .json(&json!({
                "path": path,
                "content_base64": base64::engine::general_purpose::STANDARD.encode(bytes),
                "mode": mode,
            }))
            .send()
            .await?;
        let _: Value = self.parse(response).await?;
        Ok(())
    }

    /// Host-channel write refused when the session's confidential launch is active.
    pub async fn fs_write_for_session(
        &self,
        confidential: Option<&crate::model::ConfidentialStatus>,
        id: Uuid,
        path: &str,
        bytes: &[u8],
        mode: u32,
    ) -> Result<()> {
        if let Some(msg) = crate::attestation::host_channel_forbidden(confidential) {
            anyhow::bail!("{msg}");
        }
        self.fs_write(id, path, bytes, mode).await
    }

    pub async fn process(
        &self,
        id: Uuid,
        command: &str,
        timeout_seconds: Option<u64>,
    ) -> Result<Value> {
        let response = self
            .auth(
                self.http
                    .post(self.url(&format!("/v1/sandboxes/{id}/process"))?),
            )
            .json(&json!({"command": command, "timeout_seconds": timeout_seconds}))
            .send()
            .await?;
        self.parse(response).await
    }

    /// Give the sandbox's egress proxy a secret to attach as `Authorization` on requests to
    /// `hosts` (`POST /v1/sandboxes/{id}/grants`). The guest never sees `value`. FluxVM's
    /// response is secret-free; it is returned as-is.
    pub async fn add_grant(
        &self,
        id: Uuid,
        secret_ref: &str,
        value: &str,
        hosts: &[String],
        ttl_seconds: Option<i64>,
    ) -> Result<Value> {
        let response = self
            .auth(
                self.http
                    .post(self.url(&format!("/v1/sandboxes/{id}/grants"))?),
            )
            .json(&json!({
                "secret_ref": secret_ref,
                "value": value,
                "hosts": hosts,
                "ttl_seconds": ttl_seconds,
            }))
            .send()
            .await?;
        self.parse(response).await
    }

    pub async fn list_grants(&self, id: Uuid) -> Result<Value> {
        let response = self
            .auth(
                self.http
                    .get(self.url(&format!("/v1/sandboxes/{id}/grants"))?),
            )
            .send()
            .await?;
        self.parse(response).await
    }

    /// Revoke one grant, or every grant of the sandbox when `grant_id` is `None`.
    pub async fn revoke_grants(&self, id: Uuid, grant_id: Option<&str>) -> Result<Value> {
        if let Some(g) = grant_id {
            if g.is_empty()
                || !g
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
            {
                bail!("grant id must be alphanumeric, '-' or '_'");
            }
        }
        let path = match grant_id {
            Some(g) => format!("/v1/sandboxes/{id}/grants/{g}"),
            None => format!("/v1/sandboxes/{id}/grants"),
        };
        let response = self.auth(self.http.delete(self.url(&path)?)).send().await?;
        self.parse(response).await
    }

    /// Run `command` confined by a guest exec `policy` (Landlock + seccomp, the
    /// `fluxvm-procbox` shape). The result is returned only if the guest enforced
    /// everything the policy asked for: a non-empty `enforcement.not_enforced`, or an
    /// older FluxVM that reports no `enforcement` at all, is an error, because a caller
    /// who asked for confinement must never believe it got it when it did not.
    /// Set `"best_effort": true` in the policy to accept partial enforcement; the
    /// returned `enforcement` then says exactly what was missing.
    pub async fn process_confined(
        &self,
        id: Uuid,
        command: &str,
        timeout_seconds: Option<u64>,
        policy: &Value,
    ) -> Result<Value> {
        let response = self
            .auth(
                self.http
                    .post(self.url(&format!("/v1/sandboxes/{id}/process"))?),
            )
            .json(&json!({
                "command": command,
                "timeout_seconds": timeout_seconds,
                "policy": policy,
            }))
            .send()
            .await?;
        let out: Value = self.parse(response).await?;
        let best_effort = policy
            .get("best_effort")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        let enforcement = out.get("enforcement").filter(|e| !e.is_null());
        let Some(enforcement) = enforcement else {
            bail!(
                "FluxVM did not report `enforcement` for a confined exec; it predates guest exec policy, so the command was not confined"
            );
        };
        let missing: Vec<&str> = enforcement
            .get("not_enforced")
            .and_then(Value::as_array)
            .map(|a| a.iter().filter_map(Value::as_str).collect())
            .unwrap_or_default();
        if !missing.is_empty() && !best_effort {
            bail!(
                "the guest did not enforce the requested exec policy: {}",
                missing.join(", ")
            );
        }
        Ok(out)
    }

    /// Host-channel exec refused when the session's confidential launch is active.
    pub async fn process_for_session(
        &self,
        confidential: Option<&crate::model::ConfidentialStatus>,
        id: Uuid,
        command: &str,
        timeout_seconds: Option<u64>,
    ) -> Result<Value> {
        if let Some(msg) = crate::attestation::host_channel_forbidden(confidential) {
            anyhow::bail!("{msg}");
        }
        self.process(id, command, timeout_seconds).await
    }

    /// Light vsock health-check (no exec). Prefer this over `process` while waiting
    /// for a cold guest boot — exec can hang longer than ping on some images.
    pub async fn agent_ping(&self, id: Uuid) -> Result<()> {
        let response = self
            .auth(
                self.http
                    .post(self.url(&format!("/v1/vms/{id}/agent/ping"))?),
            )
            .json(&json!({}))
            .send()
            .await?;
        let _: Value = self.parse(response).await?;
        Ok(())
    }

    pub async fn guest_request(
        &self,
        id: Uuid,
        port: u16,
        method: Method,
        path: &str,
        body: Option<&Value>,
    ) -> Result<Value> {
        let path = path.trim_start_matches('/');
        let url = self.url(&format!("/v1/sandboxes/{id}/http/{port}/{path}"))?;
        let mut req = self.auth(self.http.request(method, url));
        if let Some(body) = body {
            req = req.json(body);
        }
        let response = req.send().await?;
        self.parse(response).await
    }

    /// Open a FluxVM-bridged WebSocket to a guest TCP WebSocket path (CDP, etc.).
    pub async fn guest_ws(
        &self,
        id: Uuid,
        port: u16,
        path: &str,
    ) -> Result<
        tokio_tungstenite::WebSocketStream<
            tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>,
        >,
    > {
        use tokio_tungstenite::{
            connect_async,
            tungstenite::{client::IntoClientRequest, http::header::AUTHORIZATION},
        };

        let path = path.trim_start_matches('/');
        let mut ws_base = self.base.clone();
        let scheme = if self.base.scheme() == "https" {
            "wss"
        } else {
            "ws"
        };
        ws_base
            .set_scheme(scheme)
            .map_err(|()| anyhow::anyhow!("setting ws scheme"))?;
        let url = ws_base
            .join(&format!("/v1/sandboxes/{id}/ws/{port}/{path}"))
            .with_context(|| format!("joining FluxVM WS URL with {path}"))?;
        let mut request = url
            .as_str()
            .into_client_request()
            .context("building FluxVM WS request")?;
        if let Some(token) = &self.token {
            request.headers_mut().insert(
                AUTHORIZATION,
                format!("Bearer {token}")
                    .parse()
                    .context("FluxVM bearer header")?,
            );
        }
        let (stream, _) = connect_async(request)
            .await
            .context("connecting FluxVM sandbox WS bridge")?;
        Ok(stream)
    }

    pub async fn pause(&self, id: Uuid) -> Result<()> {
        let response = self
            .auth(self.http.post(self.url(&format!("/v1/vms/{id}/pause"))?))
            .send()
            .await?;
        let _: Value = self.parse(response).await?;
        Ok(())
    }

    pub async fn resume(&self, id: Uuid) -> Result<()> {
        let response = self
            .auth(self.http.post(self.url(&format!("/v1/vms/{id}/resume"))?))
            .send()
            .await?;
        let _: Value = self.parse(response).await?;
        Ok(())
    }

    pub async fn snapshot(&self, id: Uuid, path: &str) -> Result<()> {
        let response = self
            .auth(
                self.http
                    .post(self.url(&format!("/v1/sandboxes/{id}/snapshot"))?),
            )
            .json(&json!({"path": path}))
            .send()
            .await?;
        let _: Value = self.parse(response).await?;
        Ok(())
    }

    /// Run `req.command` in an isolated copy of the sandbox and return the
    /// pending changeset. Nothing reaches the real sandbox until
    /// [`Self::apply_changeset`].
    pub async fn speculate(&self, id: Uuid, req: &SpeculateRequest) -> Result<Changeset> {
        let response = self
            .auth(
                self.http
                    .post(self.url(&format!("/v1/sandboxes/{id}/speculate"))?),
            )
            .json(req)
            .send()
            .await?;
        self.parse(response).await
    }

    pub async fn get_changeset(&self, id: Uuid, changeset: Uuid) -> Result<Changeset> {
        let response = self
            .auth(
                self.http
                    .get(self.url(&format!("/v1/sandboxes/{id}/changesets/{changeset}"))?),
            )
            .send()
            .await?;
        self.parse(response).await
    }

    /// `verb` is `approve`, `reject` or `apply`. FluxVM answers 409 for an
    /// invalid transition, an expired changeset or a base that moved on.
    async fn changeset_verb(&self, id: Uuid, changeset: Uuid, verb: &str) -> Result<Changeset> {
        let response = self
            .auth(
                self.http
                    .post(self.url(&format!("/v1/sandboxes/{id}/changesets/{changeset}/{verb}"))?),
            )
            .send()
            .await?;
        self.parse(response).await
    }

    pub async fn approve_changeset(&self, id: Uuid, changeset: Uuid) -> Result<Changeset> {
        self.changeset_verb(id, changeset, "approve").await
    }

    pub async fn reject_changeset(&self, id: Uuid, changeset: Uuid) -> Result<Changeset> {
        self.changeset_verb(id, changeset, "reject").await
    }

    pub async fn apply_changeset(&self, id: Uuid, changeset: Uuid) -> Result<Changeset> {
        self.changeset_verb(id, changeset, "apply").await
    }

    pub async fn delete(&self, id: Uuid) -> Result<()> {
        let response = self
            .auth(self.http.delete(self.url(&format!("/v1/vms/{id}"))?))
            .send()
            .await?;
        let status = response.status();
        if status.is_success() || status == reqwest::StatusCode::NOT_FOUND {
            return Ok(());
        }
        bail!("FluxVM delete failed: {status}")
    }

    /// Replace the sandbox's L4 network policy (`POST /v1/vms/{id}/network/policy`).
    pub async fn set_network_policy(&self, id: Uuid, policy: &Value) -> Result<()> {
        let response = self
            .auth(
                self.http
                    .post(self.url(&format!("/v1/vms/{id}/network/policy"))?),
            )
            .json(policy)
            .send()
            .await?;
        let _: Value = self.parse(response).await?;
        Ok(())
    }

    /// Host cgroup freeze (FluxVM) — used after eBPF deny trips.
    pub async fn freeze(&self, id: Uuid) -> Result<()> {
        let response = self
            .auth(self.http.post(self.url(&format!("/v1/vms/{id}/freeze"))?))
            .send()
            .await?;
        let status = response.status();
        if status.is_success() {
            return Ok(());
        }
        let body = response.text().await.unwrap_or_default();
        bail!("FluxVM freeze failed: {status} — {body}")
    }

    pub async fn thaw(&self, id: Uuid) -> Result<()> {
        let response = self
            .auth(self.http.post(self.url(&format!("/v1/vms/{id}/thaw"))?))
            .send()
            .await?;
        let status = response.status();
        if status.is_success() {
            return Ok(());
        }
        let body = response.text().await.unwrap_or_default();
        bail!("FluxVM thaw failed: {status} — {body}")
    }

    /// FluxVM drop-reason histogram for the sandbox VM.
    pub async fn drop_reasons(&self, id: Uuid, limit: Option<usize>) -> Result<Value> {
        let mut url = self.url(&format!("/v1/vms/{id}/network/drop-reasons"))?;
        if let Some(n) = limit {
            url.query_pairs_mut().append_pair("limit", &n.to_string());
        }
        let response = self.auth(self.http.get(url)).send().await?;
        self.parse(response).await
    }

    pub async fn default_gateway(&self, id: Uuid) -> Result<String> {
        let value = self
            .process(
                id,
                "ip route show default | awk '{print $3; exit}'",
                Some(5),
            )
            .await?;
        let stdout = value
            .get("stdout")
            .and_then(Value::as_str)
            .or_else(|| {
                value
                    .get("data")
                    .and_then(|v| v.get("stdout"))
                    .and_then(Value::as_str)
            })
            .unwrap_or_default()
            .trim()
            .to_string();
        if stdout.is_empty() {
            bail!("sandbox did not report a default gateway; use tap+netns or set ZYVOR_AGENT_EGRESS_ADVERTISE_HOST")
        }
        Ok(stdout)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn serve_process(
        reply: Value,
    ) -> (FluxVm, std::sync::Arc<std::sync::Mutex<Option<Value>>>) {
        let seen = std::sync::Arc::new(std::sync::Mutex::new(None));
        let sink = seen.clone();
        let app = axum::Router::new().route(
            "/v1/sandboxes/{id}/process",
            axum::routing::post(move |axum::Json(body): axum::Json<Value>| {
                let sink = sink.clone();
                let reply = reply.clone();
                async move {
                    *sink.lock().unwrap() = Some(body);
                    axum::Json(reply)
                }
            }),
        );
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        (FluxVm::new(&format!("http://{addr}"), None).unwrap(), seen)
    }

    #[tokio::test]
    async fn process_confined_sends_the_policy_and_accepts_full_enforcement() {
        let (client, seen) = serve_process(json!({
            "exit_code": 0, "stdout": "ok", "stderr": "",
            "enforcement": {"filesystem": true, "seccomp": true, "not_enforced": []}
        }))
        .await;
        let policy = json!({"read": ["/usr"], "write": ["/work"]});
        let out = client
            .process_confined(Uuid::new_v4(), "id", Some(5), &policy)
            .await
            .unwrap();
        assert_eq!(out["stdout"], "ok");
        assert_eq!(seen.lock().unwrap().clone().unwrap()["policy"], policy);
    }

    #[tokio::test]
    async fn process_confined_refuses_partial_or_unreported_enforcement() {
        let partial = json!({
            "exit_code": 0, "stdout": "", "stderr": "",
            "enforcement": {"not_enforced": ["tcp_connect", "max_memory"]}
        });
        let (client, _) = serve_process(partial.clone()).await;
        let err = client
            .process_confined(Uuid::new_v4(), "id", None, &json!({}))
            .await
            .unwrap_err()
            .to_string();
        assert!(err.contains("tcp_connect, max_memory"), "{err}");

        // best_effort opts in to partial enforcement and still returns what was missing.
        let (client, _) = serve_process(partial).await;
        let out = client
            .process_confined(Uuid::new_v4(), "id", None, &json!({"best_effort": true}))
            .await
            .unwrap();
        assert_eq!(out["enforcement"]["not_enforced"][0], "tcp_connect");

        // An old FluxVM that ignores `policy` runs the command unconfined: never accept that.
        let (client, _) = serve_process(json!({"exit_code": 0, "stdout": "", "stderr": ""})).await;
        let err = client
            .process_confined(Uuid::new_v4(), "id", None, &json!({}))
            .await
            .unwrap_err()
            .to_string();
        assert!(err.contains("predates guest exec policy"), "{err}");
    }

    #[tokio::test]
    async fn add_grant_sends_the_value_once_and_revoke_validates_the_id() {
        let seen = std::sync::Arc::new(std::sync::Mutex::new(None));
        let sink = seen.clone();
        let app = axum::Router::new().route(
            "/v1/sandboxes/{id}/grants",
            axum::routing::post(move |axum::Json(body): axum::Json<Value>| {
                let sink = sink.clone();
                async move {
                    *sink.lock().unwrap() = Some(body);
                    (
                        axum::http::StatusCode::CREATED,
                        axum::Json(json!({"id": "g1", "secret_ref": "k", "hosts": ["h"]})),
                    )
                }
            }),
        );
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        let client = FluxVm::new(&format!("http://{addr}"), None).unwrap();

        let info = client
            .add_grant(
                Uuid::new_v4(),
                "k",
                "Bearer s3cret",
                &["h".into()],
                Some(60),
            )
            .await
            .unwrap();
        assert!(!info.to_string().contains("s3cret"));
        let body = seen.lock().unwrap().clone().unwrap();
        assert_eq!(body["value"], "Bearer s3cret");
        assert_eq!(body["hosts"], json!(["h"]));
        assert_eq!(body["ttl_seconds"], 60);

        let err = client
            .revoke_grants(Uuid::new_v4(), Some("../x"))
            .await
            .unwrap_err()
            .to_string();
        assert!(err.contains("grant id"), "{err}");
    }

    #[tokio::test]
    async fn set_network_policy_posts_the_policy_to_the_vm() {
        let seen = std::sync::Arc::new(std::sync::Mutex::new(None));
        let sink = seen.clone();
        let app = axum::Router::new().route(
            "/v1/vms/{id}/network/policy",
            axum::routing::post(
                move |axum::extract::Path(id): axum::extract::Path<String>,
                      axum::Json(body): axum::Json<Value>| {
                    let sink = sink.clone();
                    async move {
                        *sink.lock().unwrap() = Some((id, body.clone()));
                        axum::Json(body)
                    }
                },
            ),
        );
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        let client = FluxVm::new(&format!("http://{addr}"), None).unwrap();
        let id = Uuid::new_v4();
        let policy = crate::confine::strict_policy(
            "10.0.2.1".parse().unwrap(),
            18082,
            None,
            &[],
            None,
            None,
        );
        client.set_network_policy(id, &policy).await.unwrap();
        let (seen_id, seen_body) = seen.lock().unwrap().clone().unwrap();
        assert_eq!(seen_id, id.to_string());
        assert_eq!(seen_body, policy);
    }

    #[tokio::test]
    async fn set_network_policy_surfaces_a_fluxvm_error() {
        let app = axum::Router::new().route(
            "/v1/vms/{id}/network/policy",
            axum::routing::post(|| async {
                (
                    axum::http::StatusCode::BAD_REQUEST,
                    "invalid eBPF allow CIDR",
                )
            }),
        );
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        let client = FluxVm::new(&format!("http://{addr}"), None).unwrap();
        let error = client
            .set_network_policy(Uuid::new_v4(), &json!({}))
            .await
            .unwrap_err()
            .to_string();
        assert!(
            error.contains("400") && error.contains("invalid eBPF allow CIDR"),
            "{error}"
        );
    }

    /// The client never has more creates in flight than the gate allows (default `DEFAULT_CREATE_CONCURRENCY`),
    /// and does run them in parallel up to that limit.
    #[tokio::test]
    async fn sandbox_creates_are_capped_by_the_gate() {
        use std::sync::atomic::{AtomicUsize, Ordering};
        let in_flight = std::sync::Arc::new(AtomicUsize::new(0));
        let peak = std::sync::Arc::new(AtomicUsize::new(0));
        let (a, b) = (in_flight.clone(), peak.clone());
        let app = axum::Router::new().route(
            "/v1/sandboxes",
            axum::routing::post(move || {
                let (in_flight, peak) = (a.clone(), b.clone());
                async move {
                    let now = in_flight.fetch_add(1, Ordering::SeqCst) + 1;
                    peak.fetch_max(now, Ordering::SeqCst);
                    tokio::time::sleep(std::time::Duration::from_millis(60)).await;
                    in_flight.fetch_sub(1, Ordering::SeqCst);
                    axum::Json(json!({"id": Uuid::new_v4()}))
                }
            }),
        );
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        let client = FluxVm::new(&format!("http://{addr}"), None).unwrap();
        let mut jobs = Vec::new();
        for i in 0..6 {
            let c = client.clone();
            jobs.push(tokio::spawn(async move {
                c.create_sandbox(
                    format!("t-{i}"),
                    "node22-agent",
                    None,
                    18082,
                    &SandboxOptions::default(),
                )
                .await
            }));
        }
        for j in jobs {
            j.await.unwrap().unwrap();
        }
        let peak = peak.load(Ordering::SeqCst);
        assert!(
            peak <= DEFAULT_CREATE_CONCURRENCY,
            "more creates in flight than the gate allows: {peak}"
        );
        assert!(
            peak > 1,
            "the gate serialised creates although it allows {DEFAULT_CREATE_CONCURRENCY}"
        );
    }

    #[test]
    fn sandbox_create_concurrency_parsing_is_bounded() {
        assert_eq!(parse_create_concurrency(None), 4);
        assert_eq!(parse_create_concurrency(Some("4")), 4);
        assert_eq!(parse_create_concurrency(Some(" 2 ")), 2);
        assert_eq!(parse_create_concurrency(Some("0")), 1);
        assert_eq!(parse_create_concurrency(Some("999")), 64);
        assert_eq!(parse_create_concurrency(Some("not a number")), 4);
    }

    #[test]
    fn sandbox_create_omits_volumes_unless_present() {
        let none = serde_json::to_value(SandboxCreate {
            name: "n".into(),
            template: "t",
            ttl_seconds: None,
            http_proxy_port: 8080,
            volumes: &[],
            vcpus: None,
            memory_mib: None,
            confidential: None,
            security_profile: None,
            gpus: None,
        })
        .unwrap();
        assert!(none.get("volumes").is_none());
        assert!(none.get("gpus").is_none());
        assert!(none.get("vcpus").is_none() && none.get("memory_mib").is_none());
        assert!(none.get("confidential").is_none());

        let volumes = [SandboxVolume {
            name: "home".into(),
            guest_path: "/home/agent".into(),
        }];
        let some = serde_json::to_value(SandboxCreate {
            name: "n".into(),
            template: "t",
            ttl_seconds: None,
            http_proxy_port: 8080,
            volumes: &volumes,
            vcpus: Some(2),
            memory_mib: Some(7900),
            confidential: Some("auto"),
            security_profile: Some("measured"),
            gpus: Some(2),
        })
        .unwrap();
        assert_eq!(some["gpus"], 2);
        assert_eq!(some["volumes"][0]["name"], "home");
        assert_eq!(some["volumes"][0]["guest_path"], "/home/agent");
        assert_eq!(
            (some["vcpus"].as_u64(), some["memory_mib"].as_u64()),
            (Some(2), Some(7900))
        );
        assert_eq!(some["security_profile"], "measured");
    }

    #[test]
    fn a_sandbox_record_reads_back_the_gpus_fluxvm_assigned() {
        let rec: SandboxRecord = serde_json::from_value(json!({
            "id": Uuid::nil(),
            "request": {"name": "n", "vfio_devices": ["0000:41:00.0", "0000:81:00.0"]}
        }))
        .unwrap();
        assert_eq!(
            rec.request.unwrap().vfio_devices,
            ["0000:41:00.0", "0000:81:00.0"]
        );
        // A record with no request, or none of the field, reads as no devices, not an error.
        let bare: SandboxRecord = serde_json::from_value(json!({"id": Uuid::nil()})).unwrap();
        assert!(bare.request.is_none());
        let empty: SandboxRecord =
            serde_json::from_value(json!({"id": Uuid::nil(), "request": {}})).unwrap();
        assert!(empty.request.unwrap().vfio_devices.is_empty());
    }

    #[test]
    fn security_capabilities_parse_launch_flags() {
        let caps: HostSecurityCapabilities = serde_json::from_value(json!({
            "qemu": true,
            "secure_boot_ready": false,
            "swtpm": false,
            "signed_catalog_configured": false,
            "snp_present": true,
            "tdx_present": false,
            "snp_launch_verified": true,
            "tdx_launch_verified": false
        }))
        .unwrap();
        assert!(caps.snp_present);
        assert!(caps.snp_launch_verified);
        assert!(!caps.tdx_launch_verified);
        let empty: HostSecurityCapabilities = serde_json::from_value(json!({})).unwrap();
        assert!(!empty.snp_launch_verified && !empty.tdx_launch_verified);
    }

    /// A changeset exactly as a real FluxVM (current HEAD, in-tree KVM, run on
    /// the lab host) returned it. Guards `Changeset` against drift.
    #[test]
    fn decodes_a_real_fluxvm_changeset() {
        let cs: Changeset =
            serde_json::from_str(include_str!("../tests/fixtures/fluxvm-changeset.json")).unwrap();
        assert_eq!(cs.state, "pending");
        assert_eq!(cs.exit_code, 0);
        assert_eq!(cs.paths, ["/tmp"]);
        assert_eq!(cs.changes["added"][0], "/tmp/specdir/a.txt");
        assert!(cs.unstaged.is_empty());
        assert_eq!(cs.error, None);
    }
}
