// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: Apache-2.0

use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc, RwLock,
    },
    time::{Duration, Instant},
};
use tokio::sync::Mutex as AsyncMutex;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CredentialDescriptor {
    /// Exact host or parent suffix that may receive this credential.
    pub host: String,
    /// Header injected by the host broker, e.g. `authorization` or `x-api-key`.
    pub header: String,
    /// Host environment variable containing the secret. Its value is never persisted.
    /// For `kind=fabric`, leave empty — no external provider key is required.
    #[serde(default)]
    pub env: String,
    #[serde(default)]
    pub prefix: String,
    /// Optional HTTP method allowlist for this credential. Empty means any method.
    #[serde(default)]
    pub allowed_methods: Vec<String>,
    /// Optional URL path-prefix allowlist. Empty means any path on the bound host. An entry ending in `$` matches that exact path only
    /// (`/gmail/v1/users/me/drafts$` allows the drafts collection but not `/drafts/send`).
    #[serde(default)]
    pub path_prefixes: Vec<String>,
    /// Additional HTTPS ports that may receive this credential. Port 443 is always allowed.
    /// For `kind=fabric`, non-TLS Maglev ports (e.g. 8000) may be listed.
    #[serde(default)]
    pub allowed_ports: Vec<u16>,
    /// `provider` (default) injects a host env secret over HTTPS.
    /// `fabric` routes to a Fabric InferenceEndpoint VIP with no external API key
    /// (optional `FABRIC_AI_API_KEY` when endpoint keys are enabled).
    #[serde(default = "default_kind")]
    pub kind: String,
    /// Methods (or `*` for any) that need a human decision before this credential
    /// is used, e.g. `["POST"]` for a mail-sending or checkout credential.
    #[serde(default)]
    pub requires_approval: Vec<String>,
    /// Session `user_id` allowlist. Empty means any user (including no user_id).
    #[serde(default)]
    pub allowed_users: Vec<String>,
    /// Let the CONNECT proxy terminate TLS for this credential's host so a
    /// per-session surrogate token the agent holds can be swapped for the real
    /// secret in flight. Needs `ZYVOR_AGENT_MITM_CA_DIR`; see `mitm`.
    #[serde(default)]
    pub intercept: bool,
    /// What such an approval is called to the operator: `send` (default) or `purchase`.
    #[serde(default)]
    pub approval_kind: Option<String>,
    /// Approvals for this credential must be signed by the user's enrolled phone key when a user
    /// token decides them (see `devices.rs`). The operator token can always decide unsigned.
    #[serde(default)]
    pub require_device_signature: bool,
    /// What the person is shown when they decide a request that uses this credential: the host reads the request body and renders it
    /// (`gmail-message`, `calendar-event`; see `preview.rs`). A body it cannot render faithfully is refused before anything is sent.
    /// Only meaningful with `requires_approval`.
    #[serde(default)]
    pub preview: Option<String>,
    /// For `kind=oauth-refresh`: how the host mints short-lived access tokens from a long-lived
    /// refresh token (e.g. Google). The access token is what gets injected; the refresh token,
    /// client id and client secret stay in host env and never reach a cell.
    #[serde(default)]
    pub oauth: Option<OAuthRefresh>,
    /// Where the secret comes from instead of `env`: see [`SecretSource`]. Set at most one of `env` and
    /// `source`. Read from the operator's credentials file at startup, like everything else here; no
    /// API route writes it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<SecretSource>,
}

/// Longest a fetched secret is kept before it is fetched again.
pub const MAX_SOURCE_TTL_SECS: u64 = 3600;
/// Default cache time of a secret read from a file.
pub const DEFAULT_FILE_TTL_SECS: u64 = 60;
/// A secret file larger than this is refused.
const MAX_SECRET_FILE_BYTES: u64 = 64 * 1024;
/// Default cache time of a secret read from Vault.
pub const DEFAULT_VAULT_TTL_SECS: u64 = 300;
/// Default and longest time one Vault request may take.
pub const DEFAULT_VAULT_TIMEOUT_SECS: u64 = 10;
pub const MAX_VAULT_TIMEOUT_SECS: u64 = 30;
/// A Vault answer larger than this is refused.
const MAX_VAULT_ANSWER_BYTES: usize = 256 * 1024;

fn default_vault_mount() -> String {
    "secret".into()
}

/// How the runtime proves itself to Vault.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "method", rename_all = "kebab-case", deny_unknown_fields)]
pub enum VaultAuth {
    /// A Vault token the operator provides: read from a file (what Vault Agent's file sink writes, and
    /// re-read on every fetch so a renewed token is picked up) or from an environment variable. Set
    /// exactly one.
    Token {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        token_file: Option<PathBuf>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        token_env: Option<String>,
    },
    /// AppRole login: `POST /v1/auth/{mount}/login` with the role id and a secret id, for a token
    /// the runtime then keeps until shortly before it expires. The secret id comes from a file or a
    /// variable (set exactly one). If Vault refuses a cached token the runtime logs in once more
    /// and retries; a token that was just issued and is refused is not retried.
    #[serde(rename = "approle")]
    AppRole {
        role_id: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        secret_id_file: Option<PathBuf>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        secret_id_env: Option<String>,
        /// The auth mount. Default `approle`.
        #[serde(default = "default_approle_mount")]
        mount: String,
    },
    /// Kubernetes login: `POST /v1/auth/{mount}/login` with a role and the pod's service account
    /// JWT, read from `jwt_file` on every login so a rotated projected token is used.
    Kubernetes {
        role: String,
        /// Default `/var/run/secrets/kubernetes.io/serviceaccount/token`. The file is a projected
        /// token that Kubernetes makes world-readable, so its permissions are not checked.
        #[serde(default = "default_k8s_jwt_file")]
        jwt_file: PathBuf,
        /// The auth mount. Default `kubernetes`.
        #[serde(default = "default_k8s_mount")]
        mount: String,
    },
}

fn default_approle_mount() -> String {
    "approle".into()
}

fn default_k8s_mount() -> String {
    "kubernetes".into()
}

fn default_k8s_jwt_file() -> PathBuf {
    PathBuf::from("/var/run/secrets/kubernetes.io/serviceaccount/token")
}

/// Why reading a secret from its source failed, as a class that is safe to show an operator: it says
/// what kind of thing went wrong and never carries an answer, a path's contents or a secret.
#[derive(Debug)]
struct SourceError {
    class: &'static str,
    message: String,
}

impl std::fmt::Display for SourceError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for SourceError {}

fn fail(class: &'static str, message: String) -> anyhow::Error {
    anyhow::Error::new(SourceError { class, message })
}

/// Give an untyped error a class, keeping its message.
fn tag(class: &'static str, error: anyhow::Error) -> anyhow::Error {
    if error.downcast_ref::<SourceError>().is_some() {
        error
    } else {
        fail(class, format!("{error:#}"))
    }
}

/// The class of a failed read: `unreachable`, `refused`, `not_found`, `malformed`, `config`, `file`
/// or `error`.
fn failure_class(error: &anyhow::Error) -> &'static str {
    error
        .downcast_ref::<SourceError>()
        .map(|e| e.class)
        .unwrap_or("error")
}

/// Where a credential's secret is kept. The authorisation checks (host, method, path, port, user,
/// approval) are the same whatever the source; a source only changes where the value is read from.
// A handful of these are loaded once at startup, so the size difference between the variants costs
// nothing, and boxing the Vault fields would only make every match on them longer.
#[allow(clippy::large_enum_variant)]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "kebab-case", deny_unknown_fields)]
pub enum SecretSource {
    /// Read the secret from a file, trimmed, and cache it for `ttl_seconds`. This covers Vault Agent
    /// sinks, Kubernetes Secrets mounted as volumes, the CSI Secrets Store driver and systemd
    /// `LoadCredential=`. Rotation is replacing the file; the change is seen after the cache time.
    File {
        /// Absolute path, followed through symlinks (Kubernetes mounts secrets through them).
        path: PathBuf,
        /// Cache time in seconds, 1 to 3600. Default 60. This is also how long a revoked or rotated
        /// secret keeps working.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        ttl_seconds: Option<u64>,
        /// Accept a file readable by group or others. Off by default because a secret file should be
        /// 0400 or 0600; a Kubernetes Secret volume is 0644 unless its `defaultMode` says otherwise.
        #[serde(default, skip_serializing_if = "std::ops::Not::not")]
        allow_loose_permissions: bool,
    },
    /// Read one field of a HashiCorp Vault KV v2 secret: `GET {addr}/v1/{mount}/data/{path}` with
    /// `X-Vault-Token`, the value at `data.data.{field}`. Cached for `ttl_seconds` (default 300).
    /// `addr` must be `https` (plain `http` only to a loopback address); redirects are never followed.
    Vault {
        /// Base address, e.g. `https://vault.internal:8200`.
        addr: String,
        /// The KV v2 mount. Default `secret`.
        #[serde(default = "default_vault_mount")]
        mount: String,
        /// Secret path under the mount, e.g. `keep/github`.
        path: String,
        /// Which field of the secret is the credential.
        field: String,
        /// Vault Enterprise namespace, sent as `X-Vault-Namespace`.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        namespace: Option<String>,
        auth: VaultAuth,
        /// PEM file of an extra CA to trust for `addr` (private PKI).
        #[serde(default, skip_serializing_if = "Option::is_none")]
        ca_file: Option<PathBuf>,
        /// Cache time in seconds, 1 to 3600. Default 300. Also how long a revoked secret keeps working.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        ttl_seconds: Option<u64>,
        /// Per-request timeout in seconds, 1 to 30. Default 10.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        timeout_seconds: Option<u64>,
        /// Accept a token file readable by group or others (as `file` does for a secret file).
        #[serde(default, skip_serializing_if = "std::ops::Not::not")]
        allow_loose_permissions: bool,
    },
}

fn valid_env_name(var: &str) -> bool {
    let mut chars = var.chars();
    matches!(chars.next(), Some(c) if c.is_ascii_alphabetic() || c == '_')
        && chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
}

fn absolute_no_parent(p: &Path) -> bool {
    p.is_absolute()
        && !p
            .components()
            .any(|c| matches!(c, std::path::Component::ParentDir))
}

/// Exactly one of a file and a variable, each valid.
fn validate_file_or_env(
    name: &str,
    what: &str,
    file: &Option<PathBuf>,
    env: &Option<String>,
) -> Result<()> {
    match (file, env) {
        (Some(f), None) => {
            if !absolute_no_parent(f) {
                bail!(
                    "credential '{name}' vault {what}_file must be an absolute path without `..`"
                );
            }
        }
        (None, Some(var)) => {
            if !valid_env_name(var) {
                bail!("credential '{name}' vault {what}_env is not a valid variable name");
            }
        }
        _ => bail!(
            "credential '{name}' vault {what} needs exactly one of {what}_file and {what}_env"
        ),
    }
    Ok(())
}

fn validate_vault_auth(name: &str, auth: &VaultAuth) -> Result<()> {
    match auth {
        VaultAuth::Token {
            token_file,
            token_env,
        } => validate_file_or_env(name, "token", token_file, token_env),
        VaultAuth::AppRole {
            role_id,
            secret_id_file,
            secret_id_env,
            mount,
        } => {
            if role_id.is_empty() || role_id.len() > 128 || role_id.chars().any(char::is_control) {
                bail!("credential '{name}' vault approle role_id must be 1 to 128 characters without control characters");
            }
            if !plain_segments(mount) {
                bail!("credential '{name}' vault approle mount must be plain path segments");
            }
            validate_file_or_env(name, "secret_id", secret_id_file, secret_id_env)
        }
        VaultAuth::Kubernetes {
            role,
            jwt_file,
            mount,
        } => {
            if role.is_empty()
                || role.len() > 128
                || !role
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'-' | b'.'))
            {
                bail!("credential '{name}' vault kubernetes role must be 1 to 128 letters, digits, _ - .");
            }
            if !absolute_no_parent(jwt_file) {
                bail!("credential '{name}' vault kubernetes jwt_file must be an absolute path without `..`");
            }
            if !plain_segments(mount) {
                bail!("credential '{name}' vault kubernetes mount must be plain path segments");
            }
            Ok(())
        }
    }
}

/// A path-like value made of plain segments: letters, digits, `_`, `-`, `.`; no empty segment, no
/// `.` or `..`, no leading or trailing `/`. Nothing here can change the shape of the Vault URL.
fn plain_segments(s: &str) -> bool {
    !s.is_empty()
        && s.len() <= 256
        && s.split('/').all(|seg| {
            !seg.is_empty()
                && seg != "."
                && seg != ".."
                && seg
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'-' | b'.'))
        })
}

/// The Vault URL of a KV v2 read, with each segment pushed on separately so none can escape.
fn vault_url(addr: &str, mount: &str, path: &str) -> Result<url::Url> {
    let mut url = url::Url::parse(addr).context("the Vault address is not a URL")?;
    {
        let mut segments = url
            .path_segments_mut()
            .map_err(|_| anyhow::anyhow!("the Vault address cannot carry a path"))?;
        segments.pop_if_empty().push("v1");
        for seg in mount.split('/') {
            segments.push(seg);
        }
        segments.push("data");
        for seg in path.split('/') {
            segments.push(seg);
        }
    }
    Ok(url)
}

fn is_loopback_host(url: &url::Url) -> bool {
    match url.host() {
        Some(url::Host::Domain(d)) => d.eq_ignore_ascii_case("localhost"),
        Some(url::Host::Ipv4(ip)) => ip.is_loopback(),
        Some(url::Host::Ipv6(ip)) => ip.is_loopback(),
        None => false,
    }
}

impl SecretSource {
    pub fn kind(&self) -> &'static str {
        match self {
            Self::File { .. } => "file",
            Self::Vault { .. } => "vault",
        }
    }

    fn ttl(&self) -> Duration {
        match self {
            Self::File { ttl_seconds, .. } => {
                Duration::from_secs(ttl_seconds.unwrap_or(DEFAULT_FILE_TTL_SECS))
            }
            Self::Vault { ttl_seconds, .. } => {
                Duration::from_secs(ttl_seconds.unwrap_or(DEFAULT_VAULT_TTL_SECS))
            }
        }
    }

    fn validate(&self, name: &str) -> Result<()> {
        match self {
            Self::File {
                path, ttl_seconds, ..
            } => {
                if !path.is_absolute() {
                    bail!("credential '{name}' source path must be absolute");
                }
                if path
                    .components()
                    .any(|c| matches!(c, std::path::Component::ParentDir))
                {
                    bail!("credential '{name}' source path may not contain `..`");
                }
                if let Some(ttl) = ttl_seconds {
                    if !(1..=MAX_SOURCE_TTL_SECS).contains(ttl) {
                        bail!(
                            "credential '{name}' source ttl_seconds must be between 1 and {MAX_SOURCE_TTL_SECS}"
                        );
                    }
                }
                Ok(())
            }
            Self::Vault {
                addr,
                mount,
                path,
                field,
                namespace,
                auth,
                ca_file,
                ttl_seconds,
                timeout_seconds,
                ..
            } => {
                let url = url::Url::parse(addr)
                    .with_context(|| format!("credential '{name}' vault addr is not a URL"))?;
                if !url.username().is_empty()
                    || url.password().is_some()
                    || url.query().is_some()
                    || url.fragment().is_some()
                {
                    bail!("credential '{name}' vault addr may not carry credentials, a query or a fragment");
                }
                let loopback = is_loopback_host(&url);
                if url.scheme() != "https" && !(url.scheme() == "http" && loopback) {
                    bail!("credential '{name}' vault addr must be https (http only for loopback)");
                }
                if !plain_segments(mount) {
                    bail!("credential '{name}' vault mount must be plain path segments (letters, digits, _ - .)");
                }
                if !plain_segments(path) {
                    bail!("credential '{name}' vault path must be plain path segments (letters, digits, _ - .)");
                }
                if field.is_empty()
                    || field.len() > 128
                    || !field
                        .bytes()
                        .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'-' | b'.'))
                {
                    bail!(
                        "credential '{name}' vault field must be 1 to 128 letters, digits, _ - ."
                    );
                }
                if let Some(ns) = namespace {
                    if !plain_segments(ns) {
                        bail!("credential '{name}' vault namespace must be plain path segments");
                    }
                }
                validate_vault_auth(name, auth)?;
                if let Some(f) = ca_file {
                    if !f.is_absolute()
                        || f.components()
                            .any(|c| matches!(c, std::path::Component::ParentDir))
                    {
                        bail!("credential '{name}' vault ca_file must be an absolute path without `..`");
                    }
                }
                if let Some(ttl) = ttl_seconds {
                    if !(1..=MAX_SOURCE_TTL_SECS).contains(ttl) {
                        bail!("credential '{name}' source ttl_seconds must be between 1 and {MAX_SOURCE_TTL_SECS}");
                    }
                }
                if let Some(t) = timeout_seconds {
                    if !(1..=MAX_VAULT_TIMEOUT_SECS).contains(t) {
                        bail!("credential '{name}' vault timeout_seconds must be between 1 and {MAX_VAULT_TIMEOUT_SECS}");
                    }
                }
                Ok(())
            }
        }
    }
}

/// Token-endpoint settings for `kind=oauth-refresh` (RFC 6749 section 6, refresh-token grant).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OAuthRefresh {
    /// `https` token endpoint, e.g. `https://oauth2.googleapis.com/token`. Plain `http` is
    /// accepted only for a loopback address (tests, local fixtures).
    pub token_url: String,
    /// Host env variable holding the OAuth client id.
    pub client_id_env: String,
    /// Host env variable holding the OAuth client secret (empty env value means a public client).
    pub client_secret_env: String,
    /// Host env variable holding the refresh token: one identity for the whole host. Leave empty when `connection` is set.
    #[serde(default)]
    pub refresh_token_env: String,
    /// Makes the credential **per person**: the refresh token is the one that person stored under this connection name
    /// (`PUT /v1/connections/{name}`), and the access token is minted for them on first use. A session with no user cannot use it.
    #[serde(default)]
    pub connection: Option<String>,
    /// Space-separated scopes to ask the token endpoint for (`scope` in the refresh request). Narrows the access token to what this credential
    /// needs, e.g. a read-only credential asking for `Mail.Read` only, even though the person consented to more. Microsoft accepts it (and its
    /// refresh tokens rotate, which per-person connections follow); Google's endpoint does not need it. Empty: send none.
    #[serde(default)]
    pub scope: String,
    /// Refresh this many seconds before the access token expires. Default 300.
    #[serde(default = "default_refresh_margin")]
    pub refresh_margin_secs: u64,
}

fn default_refresh_margin() -> u64 {
    300
}

/// The kind name for credentials whose secret is a refreshed OAuth access token.
pub const KIND_OAUTH_REFRESH: &str = "oauth-refresh";

fn env_secret(var: &str, required: bool) -> Result<String> {
    match std::env::var(var) {
        Ok(v) if !v.is_empty() => Ok(v),
        _ if !required => Ok(String::new()),
        _ => bail!("host environment variable {var} is not set"),
    }
}

/// An access token, how long it lives, and a **new refresh token** when the endpoint rotated it (Microsoft does, on every use).
struct Minted {
    token: String,
    lifetime: Duration,
    rotated: Option<String>,
}

/// The refresh-token grant against the token endpoint. The client id and secret come from host env; `refresh_token` is passed in. The error
/// text carries the provider's error code at most, never a secret.
async fn mint_access_token(
    oauth: &OAuthRefresh,
    refresh_token: &str,
    name: &str,
    http: &reqwest::Client,
) -> Result<Minted> {
    // Re-checked here, not only at descriptor-validate time: a descriptor built via
    // `CredentialVault::from_descriptors` never went through `validate_descriptor`, and the request below
    // carries the client secret and refresh token in its body, so a plain-http endpoint would send both
    // in cleartext.
    require_https_or_loopback(name, &oauth.token_url)?;
    // The check above is a real, tested guard (a call to a function, so a static scanner tracing
    // this variable's flow into the request below may not credit it) but is repeated here, inline
    // and immediately before the request, in the simplest form: a scanner that cannot see across
    // the function call above should still see this.
    if !oauth.token_url.starts_with("https://")
        && !oauth.token_url.starts_with("http://127.0.0.1")
        && !oauth.token_url.starts_with("http://localhost")
        && !oauth.token_url.starts_with("http://[::1]")
    {
        bail!("credential '{name}' oauth token_url must be https (http only for loopback)");
    }
    // built in its own block: the serializer is not `Sync`, so it must not be alive across the await below
    let body = {
        let mut form = url::form_urlencoded::Serializer::new(String::new());
        form.append_pair("grant_type", "refresh_token")
            .append_pair("client_id", &env_secret(&oauth.client_id_env, true)?);
        // a public client (Microsoft's desktop and mobile apps) has no secret, and sending an empty one is refused
        let secret = env_secret(&oauth.client_secret_env, false)?;
        if !secret.is_empty() {
            form.append_pair("client_secret", &secret);
        }
        form.append_pair("refresh_token", refresh_token);
        if !oauth.scope.trim().is_empty() {
            form.append_pair("scope", oauth.scope.trim());
        }
        form.finish()
    };
    let response = http
        .post(&oauth.token_url)
        .header("content-type", "application/x-www-form-urlencoded")
        .header("accept", "application/json")
        .body(body)
        .timeout(Duration::from_secs(20))
        .send()
        .await
        .with_context(|| format!("token endpoint for '{name}' is unreachable"))?;
    let status = response.status();
    let json: serde_json::Value = response.json().await.with_context(|| {
        format!("token endpoint for '{name}' returned status {status} with a non-JSON body")
    })?;
    if !status.is_success() {
        let code = json
            .get("error")
            .and_then(|v| v.as_str())
            .unwrap_or("unknown");
        bail!("token endpoint for '{name}' refused the refresh: status {status}, error {code}");
    }
    let token = json
        .get("access_token")
        .and_then(|v| v.as_str())
        .filter(|t| !t.is_empty())
        .with_context(|| format!("token endpoint for '{name}' returned no access_token"))?;
    let lifetime = Duration::from_secs(
        json.get("expires_in")
            .and_then(|v| v.as_u64())
            .unwrap_or(3600)
            .max(1),
    );
    let rotated = json
        .get("refresh_token")
        .and_then(|v| v.as_str())
        .filter(|t| !t.is_empty() && *t != refresh_token)
        .map(str::to_string);
    Ok(Minted {
        token: token.to_string(),
        lifetime,
        rotated,
    })
}

#[derive(Clone)]
struct CachedToken {
    value: String,
    valid_until: Instant,
}

// A token is a secret: never print it, even from a `{:?}` of the whole vault.
impl std::fmt::Debug for CachedToken {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CachedToken")
            .field("value", &"<redacted>")
            .field("valid_until", &self.valid_until)
            .finish()
    }
}

/// One sourced credential's state: its cached value and, for a Vault source, the HTTP client built for
/// it on first use. The lock is held across a fetch, so a burst of requests after the value expires
/// makes one read, not many.
#[derive(Default)]
struct SourceState {
    cached: Option<CachedToken>,
    client: Option<reqwest::Client>,
    /// The token a Vault login (AppRole, Kubernetes) returned, and when to stop using it.
    login: Option<(String, Instant)>,
    last_ok: Option<Instant>,
    last_failure: Option<&'static str>,
}

// Never prints the value, only whether there is one.
impl std::fmt::Debug for SourceState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SourceState")
            .field("cached", &self.cached.is_some())
            .field("client", &self.client.is_some())
            .field("login", &self.login.is_some())
            .field("last_failure", &self.last_failure)
            .finish()
    }
}

type SourceSlot = Arc<AsyncMutex<SourceState>>;

/// One sourced credential's state, as shown by `GET /v1/vault/status`.
#[derive(Debug, Clone, Serialize)]
pub struct SourceStatus {
    pub name: String,
    pub kind: &'static str,
    /// A value is cached and still within its TTL.
    pub cached: bool,
    /// A read is in flight right now.
    pub fetching: bool,
    pub last_success_seconds_ago: Option<u64>,
    /// `unreachable`, `refused`, `not_found`, `malformed`, `config`, `file` or `error`; none if the last read worked.
    pub last_failure: Option<&'static str>,
}

/// The stand-in for credential `name` that a session's agent holds. It is derived
/// from the session's capability, so it is stable for the session, different for
/// every session, and worthless anywhere but through this runtime.
pub fn surrogate(capability: &str, name: &str) -> String {
    let mac = crate::schedules::hmac_sha256(capability.as_bytes(), name.as_bytes());
    format!("zy_sur_{}", &hex::encode(mac)[..32])
}

impl CredentialDescriptor {
    /// The approval kind this request needs, if its method requires one.
    pub fn approval_kind_for(
        &self,
        method: &reqwest::Method,
    ) -> Option<crate::model::ApprovalKind> {
        let needed = self
            .requires_approval
            .iter()
            .any(|m| m == "*" || m.eq_ignore_ascii_case(method.as_str()));
        needed.then_some(match self.approval_kind.as_deref() {
            Some("purchase") => crate::model::ApprovalKind::Purchase,
            _ => crate::model::ApprovalKind::Send,
        })
    }
}

fn default_kind() -> String {
    "provider".into()
}

#[derive(Debug, Clone, Default)]
pub struct CredentialVault {
    descriptors: HashMap<String, CredentialDescriptor>,
    /// Current access tokens of `oauth-refresh` credentials. Shared by clones of the vault so
    /// the refresher task and request handlers see the same values. Never persisted or logged.
    tokens: Arc<RwLock<HashMap<String, CachedToken>>>,
    /// Access tokens minted for one person's connection, by (credential, person).
    user_tokens: Arc<RwLock<HashMap<(String, String), CachedToken>>>,
    /// A cache per credential that has a `source`, built once because descriptors never change after load.
    sourced: Arc<HashMap<String, SourceSlot>>,
    /// How many times a secret has been read from its source, for tests and diagnosis. Never a value.
    source_fetches: Arc<AtomicUsize>,
}

impl CredentialVault {
    pub async fn load(path: Option<&Path>) -> Result<Self> {
        let Some(path) = path else {
            return Ok(Self::default());
        };
        let raw = tokio::fs::read(path)
            .await
            .with_context(|| format!("reading credentials descriptor file {}", path.display()))?;
        let descriptors = serde_json::from_slice::<HashMap<String, CredentialDescriptor>>(&raw)
            .context("decoding credentials descriptor JSON")?;
        for (name, d) in &descriptors {
            validate_descriptor(name, d)?;
            startup_check_source(name, d).await?;
        }
        let vault = Self::from_descriptors(descriptors);
        vault.prefetch_vault_sources().await;
        Ok(vault)
    }

    /// Read every Vault-sourced secret once at startup, so a wrong path or a denied policy shows up at
    /// boot as a warning instead of on the first request. It never stops startup: the store may come
    /// up after the runtime, and the first real request will try again.
    async fn prefetch_vault_sources(&self) {
        let mut names: Vec<&String> = self
            .descriptors
            .iter()
            .filter(|(_, d)| matches!(d.source, Some(SecretSource::Vault { .. })))
            .map(|(n, _)| n)
            .collect();
        names.sort();
        for name in names {
            if let Some(d) = self.descriptors.get(name) {
                // The failure is already logged (class only) by `resolve_sourced`.
                let _ = self.resolve_sourced(name, d).await;
            }
        }
    }

    /// A vault of the given descriptors (used by tests and embedders).
    pub fn from_descriptors(descriptors: HashMap<String, CredentialDescriptor>) -> Self {
        let sourced = descriptors
            .iter()
            .filter(|(_, d)| d.source.is_some())
            .map(|(name, _)| (name.clone(), SourceSlot::default()))
            .collect();
        Self {
            descriptors,
            tokens: Arc::default(),
            user_tokens: Arc::default(),
            sourced: Arc::new(sourced),
            source_fetches: Arc::default(),
        }
    }

    /// Where each sourced credential stands, for `GET /v1/vault/status`. Names, kinds, timing and the
    /// class of the last failure only; never a value, a path, or an answer.
    pub fn source_status(&self) -> Vec<SourceStatus> {
        let mut out: Vec<SourceStatus> = self
            .descriptors
            .iter()
            .filter_map(|(name, d)| {
                let source = d.source.as_ref()?;
                let mut status = SourceStatus {
                    name: name.clone(),
                    kind: source.kind(),
                    cached: false,
                    fetching: false,
                    last_success_seconds_ago: None,
                    last_failure: None,
                };
                match self.sourced.get(name).map(|slot| slot.try_lock()) {
                    Some(Ok(state)) => {
                        status.cached = state
                            .cached
                            .as_ref()
                            .is_some_and(|c| c.valid_until > Instant::now());
                        status.last_success_seconds_ago =
                            state.last_ok.map(|t| t.elapsed().as_secs());
                        status.last_failure = state.last_failure;
                    }
                    _ => status.fetching = true,
                }
                Some(status)
            })
            .collect();
        out.sort_by(|a, b| a.name.cmp(&b.name));
        out
    }

    /// How many times a secret has been read from a source since startup.
    pub fn source_fetches(&self) -> usize {
        self.source_fetches.load(Ordering::SeqCst)
    }

    pub fn descriptor(&self, name: &str) -> Option<&CredentialDescriptor> {
        self.descriptors.get(name)
    }

    /// Descriptor names only (never secret values).
    pub fn names(&self) -> Vec<String> {
        let mut names: Vec<_> = self.descriptors.keys().cloned().collect();
        names.sort();
        names
    }

    pub fn is_injection_header(&self, header: &str) -> bool {
        self.descriptors
            .values()
            .any(|d| d.header.eq_ignore_ascii_case(header))
    }

    /// Names, among `granted`, of credentials whose host is `host` and that ask to
    /// be intercepted.
    pub fn intercepted_for<'a>(&self, granted: &'a [String], host: &str) -> Vec<&'a str> {
        granted
            .iter()
            .filter(|name| {
                self.descriptor(name)
                    .is_some_and(|d| d.intercept && host_matches(&d.host, host))
            })
            .map(String::as_str)
            .collect()
    }

    pub fn resolve(&self, name: &str) -> Result<(&CredentialDescriptor, String)> {
        self.resolve_for(name, None)
    }

    /// Like [`resolve`](Self::resolve), for a request made on behalf of `user` (needed for a per-person credential).
    pub fn resolve_for(
        &self,
        name: &str,
        user: Option<&str>,
    ) -> Result<(&CredentialDescriptor, String)> {
        let descriptor = self.descriptor(name).with_context(|| {
            format!("credential '{name}' is not configured on this Fabric host")
        })?;
        if descriptor.source.is_some() {
            bail!("credential '{name}' has a source and can only be resolved asynchronously");
        }
        if descriptor.kind.eq_ignore_ascii_case("fabric") {
            // Optional endpoint API key; empty means no Authorization header.
            let value = std::env::var("FABRIC_AI_API_KEY").unwrap_or_default();
            if value.is_empty() {
                return Ok((descriptor, String::new()));
            }
            return Ok((descriptor, format!("{}{}", descriptor.prefix, value)));
        }
        if descriptor.kind.eq_ignore_ascii_case(KIND_OAUTH_REFRESH) {
            let per_person = descriptor
                .oauth
                .as_ref()
                .is_some_and(|o| o.connection.is_some());
            let candidate = if per_person {
                let user = user.with_context(|| {
                    format!("credential '{name}' belongs to a person's own connection and needs a session with a user")
                })?;
                self.user_tokens
                    .read()
                    .ok()
                    .and_then(|t| t.get(&(name.to_string(), user.to_string())).cloned())
            } else {
                self.tokens.read().ok().and_then(|t| t.get(name).cloned())
            };
            let cached = candidate.filter(|t| t.valid_until > Instant::now()).with_context(|| {
                format!("credential '{name}' has no valid access token yet (the last refresh failed, is pending, or the person has not connected)")
            })?;
            let prefix = if descriptor.prefix.is_empty() {
                "Bearer "
            } else {
                descriptor.prefix.as_str()
            };
            return Ok((descriptor, format!("{prefix}{}", cached.value)));
        }
        let value = std::env::var(&descriptor.env)
            .with_context(|| format!("host environment variable {} is not set", descriptor.env))?;
        if value.is_empty() {
            bail!("host environment variable {} is empty", descriptor.env);
        }
        Ok((descriptor, format!("{}{}", descriptor.prefix, value)))
    }

    /// Mint an access token for the `oauth-refresh` credential `name` now and cache it. Returns
    /// how long the new token is valid. Nothing secret is put in the error text.
    pub async fn refresh_now(&self, name: &str, http: &reqwest::Client) -> Result<Duration> {
        let descriptor = self
            .descriptor(name)
            .with_context(|| format!("credential '{name}' is not configured"))?;
        let oauth = descriptor
            .oauth
            .as_ref()
            .filter(|o| {
                descriptor.kind.eq_ignore_ascii_case(KIND_OAUTH_REFRESH) && o.connection.is_none()
            })
            .with_context(|| {
                format!("credential '{name}' is not a host-wide oauth-refresh credential")
            })?;
        let refresh_token = env_secret(&oauth.refresh_token_env, true)?;
        let Minted {
            token,
            lifetime,
            rotated,
        } = mint_access_token(oauth, &refresh_token, name, http).await?;
        if rotated.is_some() {
            // A host-wide refresh token lives in the host's environment and cannot be replaced from here: the old one keeps working until its
            // own expiry, so say so instead of silently drifting.
            tracing::warn!(credential = %name, "the token endpoint rotated the refresh token, but a host-wide one cannot be updated; use a per-person connection for providers that rotate");
        }
        self.tokens
            .write()
            .map_err(|_| anyhow::anyhow!("token cache lock poisoned"))?
            .insert(
                name.to_string(),
                CachedToken {
                    value: token,
                    valid_until: Instant::now() + lifetime,
                },
            );
        Ok(lifetime)
    }

    /// Whether `name` is a per-person credential (its refresh token is the person's own connection).
    pub fn is_per_person(&self, name: &str) -> bool {
        self.descriptors
            .get(name)
            .and_then(|d| d.oauth.as_ref())
            .is_some_and(|o| o.connection.is_some())
    }

    /// The connection a per-person credential uses (`google`), if it is one.
    pub fn connection_of(&self, name: &str) -> Option<&str> {
        self.descriptors
            .get(name)?
            .oauth
            .as_ref()?
            .connection
            .as_deref()
    }

    /// Every connection name some credential asks a person to provide, sorted.
    pub fn connection_names(&self) -> Vec<String> {
        let mut names: Vec<String> = self
            .descriptors
            .values()
            .filter_map(|d| d.oauth.as_ref()?.connection.clone())
            .collect();
        names.sort();
        names.dedup();
        names
    }

    /// Makes sure `user` has a valid access token for the per-person credential `name`, minting one from `refresh_token` (the person's own,
    /// from their connection) when there is none or it is about to expire. A missing connection or a refusal is an error that names no secret.
    /// Returns the **new refresh token** when the endpoint rotated it, for the caller to store (`ConnectionStore::rotate`).
    pub async fn ensure_user_token(
        &self,
        name: &str,
        user: &str,
        refresh_token: Option<&str>,
        http: &reqwest::Client,
    ) -> Result<Option<String>> {
        let descriptor = self
            .descriptor(name)
            .with_context(|| format!("credential '{name}' is not configured"))?;
        let oauth = descriptor
            .oauth
            .as_ref()
            .filter(|o| o.connection.is_some())
            .with_context(|| format!("credential '{name}' is not a per-person credential"))?;
        let key = (name.to_string(), user.to_string());
        let margin = Duration::from_secs(oauth.refresh_margin_secs.min(600));
        let fresh = self
            .user_tokens
            .read()
            .map_err(|_| anyhow::anyhow!("token cache lock poisoned"))?
            .get(&key)
            .is_some_and(|t| t.valid_until > Instant::now() + margin);
        if fresh {
            return Ok(None);
        }
        let refresh_token = refresh_token.with_context(|| {
            format!(
                "connect your {} account first (credential '{name}' uses your own connection)",
                oauth.connection.as_deref().unwrap_or("account")
            )
        })?;
        let Minted {
            token,
            lifetime,
            rotated,
        } = mint_access_token(oauth, refresh_token, name, http).await?;
        self.user_tokens
            .write()
            .map_err(|_| anyhow::anyhow!("token cache lock poisoned"))?
            .insert(
                key,
                CachedToken {
                    value: token,
                    valid_until: Instant::now() + lifetime,
                },
            );
        Ok(rotated)
    }

    /// Drops the cached access tokens of `user` for every credential that uses `connection` (after they disconnect or replace it).
    pub fn forget_user_connection(&self, user: &str, connection: &str) {
        let names: Vec<&String> = self
            .descriptors
            .iter()
            .filter(|(_, d)| {
                d.oauth.as_ref().and_then(|o| o.connection.as_deref()) == Some(connection)
            })
            .map(|(n, _)| n)
            .collect();
        if let Ok(mut cache) = self.user_tokens.write() {
            cache.retain(|(n, u), _| !(u == user && names.contains(&n)));
        }
    }

    /// Get a first token for every `oauth-refresh` credential (a failure is logged, not fatal:
    /// the credential then fails closed until a later attempt works) and keep refreshing each one
    /// in the background, `refresh_margin_secs` before it expires, with backoff after a failure.
    pub async fn start_oauth_refresh(&self, http: reqwest::Client) {
        let names: Vec<String> = self
            .descriptors
            .iter()
            .filter(|(_, d)| {
                d.kind.eq_ignore_ascii_case(KIND_OAUTH_REFRESH)
                    && d.oauth.as_ref().is_some_and(|o| o.connection.is_none())
            })
            .map(|(n, _)| n.clone())
            .collect();
        for name in names {
            let margin = self
                .descriptors
                .get(&name)
                .and_then(|d| d.oauth.as_ref())
                .map_or(300, |o| o.refresh_margin_secs);
            let first =
                tokio::time::timeout(Duration::from_secs(25), self.refresh_now(&name, &http))
                    .await
                    .unwrap_or_else(|_| Err(anyhow::anyhow!("timed out")));
            match &first {
                Ok(life) => {
                    tracing::info!(credential = %name, expires_in = life.as_secs(), "oauth access token obtained")
                }
                Err(error) => {
                    tracing::warn!(credential = %name, %error, "oauth token refresh failed; the credential fails closed until it works")
                }
            }
            let vault = self.clone();
            let http = http.clone();
            tokio::spawn(async move {
                let mut backoff = Duration::from_secs(15);
                let mut wait = match first {
                    Ok(life) => life
                        .saturating_sub(Duration::from_secs(margin))
                        .max(Duration::from_secs(30)),
                    Err(_) => backoff,
                };
                loop {
                    tokio::time::sleep(wait).await;
                    match vault.refresh_now(&name, &http).await {
                        Ok(life) => {
                            backoff = Duration::from_secs(15);
                            wait = life
                                .saturating_sub(Duration::from_secs(margin))
                                .max(Duration::from_secs(30));
                        }
                        Err(error) => {
                            tracing::warn!(credential = %name, %error, "oauth token refresh failed; retrying");
                            wait = backoff;
                            backoff = (backoff * 2).min(Duration::from_secs(300));
                        }
                    }
                }
            });
        }
    }

    /// Credential authority (Keep 0.1): resolve a secret only when the request
    /// matches the descriptor allowlist for host / method / path / port / user.
    /// Secrets still come from host env — user-held unwrap is Keep 0.2.
    pub async fn authorize_resolve(
        &self,
        name: &str,
        ctx: &ResolveContext<'_>,
    ) -> Result<(&CredentialDescriptor, String)> {
        let descriptor = self.descriptor(name).with_context(|| {
            format!("credential '{name}' is not configured on this Fabric host")
        })?;
        if !host_matches(&descriptor.host, ctx.host) {
            bail!("credential '{name}' cannot be used for host {}", ctx.host);
        }
        if !credential_allows_request(descriptor, ctx.method, ctx.path, ctx.port) {
            bail!(
                "credential '{name}' policy denies {} {} on port {}",
                ctx.method.as_str(),
                ctx.path,
                ctx.port
            );
        }
        if !descriptor.allowed_users.is_empty() {
            let Some(uid) = ctx.user_id.filter(|u| !u.is_empty()) else {
                bail!("credential '{name}' requires a session user_id");
            };
            if !descriptor
                .allowed_users
                .iter()
                .any(|u| u.eq_ignore_ascii_case(uid))
            {
                bail!("credential '{name}' is not allowed for user '{uid}'");
            }
        }
        // Every check above ran before any secret was read, whatever the source.
        if descriptor.source.is_some() {
            let value = self.resolve_sourced(name, descriptor).await?;
            return Ok((descriptor, format!("{}{}", descriptor.prefix, value)));
        }
        self.resolve_for(name, ctx.user_id)
    }

    /// The secret of a credential with a `source`, from its cache or freshly read. A read that fails
    /// clears the cache and fails the request: a value is never used past its own cache time.
    async fn resolve_sourced(
        &self,
        name: &str,
        descriptor: &CredentialDescriptor,
    ) -> Result<String> {
        let source = descriptor
            .source
            .as_ref()
            .with_context(|| format!("credential '{name}' has no source"))?;
        let slot = self
            .sourced
            .get(name)
            .with_context(|| format!("credential '{name}' has no source cache"))?;
        let mut state = slot.lock().await;
        if let Some(c) = state
            .cached
            .as_ref()
            .filter(|c| c.valid_until > Instant::now())
        {
            return Ok(c.value.clone());
        }
        self.source_fetches.fetch_add(1, Ordering::SeqCst);
        let fetched = match source {
            SecretSource::File {
                path,
                allow_loose_permissions,
                ..
            } => read_secret_file(name, path, *allow_loose_permissions)
                .await
                .map_err(|e| tag("file", e)),
            SecretSource::Vault { .. } => fetch_vault(name, source, &mut state).await,
        };
        let value = match fetched {
            Ok(value) => value,
            Err(error) => {
                state.cached = None;
                state.last_failure = Some(failure_class(&error));
                tracing::warn!(credential = %name, source = source.kind(), class = failure_class(&error), "could not read the credential's secret");
                return Err(error);
            }
        };
        // The value must be usable as the tail of a header, and must not smuggle in a second header.
        if reqwest::header::HeaderValue::from_str(&format!("{}{}", descriptor.prefix, value))
            .is_err()
        {
            state.cached = None;
            state.last_failure = Some("malformed");
            return Err(fail(
                "malformed",
                format!(
                    "credential '{name}': the secret from its source is not a valid header value"
                ),
            ));
        }
        state.last_ok = Some(Instant::now());
        state.last_failure = None;
        state.cached = Some(CachedToken {
            value: value.clone(),
            valid_until: Instant::now() + source.ttl(),
        });
        Ok(value)
    }
}

/// Read a secret file: a regular file (symlinks are followed, as Kubernetes needs), not readable by
/// group or others unless allowed, at most 64 KiB, UTF-8, trimmed, non-empty and free of control
/// characters. Errors name the credential and the file, never its contents.
async fn read_secret_file(name: &str, path: &Path, allow_loose: bool) -> Result<String> {
    use tokio::io::AsyncReadExt;
    let file = tokio::fs::File::open(path).await.with_context(|| {
        format!(
            "credential '{name}': the secret file {} cannot be opened",
            path.display()
        )
    })?;
    // Check the file we opened, not the path, so it cannot be swapped between check and read.
    let meta = file.metadata().await.with_context(|| {
        format!(
            "credential '{name}': the secret file {} cannot be inspected",
            path.display()
        )
    })?;
    if !meta.is_file() {
        bail!(
            "credential '{name}': {} is not a regular file",
            path.display()
        );
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = meta.permissions().mode() & 0o777;
        if !allow_loose && mode & 0o077 != 0 {
            bail!(
                "credential '{name}': the secret file {} is readable by group or others (mode {mode:04o}); make it 0400 or 0600, or set allow_loose_permissions",
                path.display()
            );
        }
    }
    #[cfg(not(unix))]
    let _ = allow_loose;
    let mut bytes = Vec::new();
    file.take(MAX_SECRET_FILE_BYTES + 1)
        .read_to_end(&mut bytes)
        .await
        .with_context(|| {
            format!(
                "credential '{name}': the secret file {} cannot be read",
                path.display()
            )
        })?;
    if bytes.len() as u64 > MAX_SECRET_FILE_BYTES {
        bail!(
            "credential '{name}': the secret file {} is larger than {MAX_SECRET_FILE_BYTES} bytes",
            path.display()
        );
    }
    let text = String::from_utf8(bytes).map_err(|_| {
        anyhow::anyhow!(
            "credential '{name}': the secret file {} is not valid UTF-8",
            path.display()
        )
    })?;
    let value = text.trim();
    if value.is_empty() {
        bail!(
            "credential '{name}': the secret file {} is empty",
            path.display()
        );
    }
    if value.chars().any(char::is_control) {
        bail!("credential '{name}': the secret file {} contains a control character (a newline inside the value?)", path.display());
    }
    Ok(value.to_string())
}

/// The HTTP client for one Vault source: no redirects (a redirect could carry the token elsewhere),
/// a timeout, and an extra CA when the operator names one.
async fn vault_client(name: &str, source: &SecretSource) -> Result<reqwest::Client> {
    let SecretSource::Vault {
        ca_file,
        timeout_seconds,
        ..
    } = source
    else {
        bail!("credential '{name}' is not a Vault source");
    };
    let timeout = Duration::from_secs(timeout_seconds.unwrap_or(DEFAULT_VAULT_TIMEOUT_SECS));
    let mut builder = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .timeout(timeout)
        .connect_timeout(timeout.min(Duration::from_secs(5)));
    if let Some(path) = ca_file {
        let pem = tokio::fs::read(path).await.with_context(|| {
            format!(
                "credential '{name}': the Vault CA file {} cannot be read",
                path.display()
            )
        })?;
        // `from_pem` accepts garbage and only fails later as an opaque handshake error, so check first.
        if !pem
            .windows(27)
            .any(|w| w == b"-----BEGIN CERTIFICATE-----".as_slice())
        {
            bail!(
                "credential '{name}': the Vault CA file {} is not a PEM certificate",
                path.display()
            );
        }
        let cert = reqwest::Certificate::from_pem(&pem).with_context(|| {
            format!(
                "credential '{name}': the Vault CA file {} is not a PEM certificate",
                path.display()
            )
        })?;
        builder = builder.add_root_certificate(cert);
    }
    builder
        .build()
        .with_context(|| format!("credential '{name}': the Vault client could not be built"))
}

/// A secret the operator configured as a file or a variable: read afresh every time, so a renewed
/// token or a rotated secret id is used.
async fn read_configured_secret(
    name: &str,
    what: &str,
    file: &Option<PathBuf>,
    env: &Option<String>,
    allow_loose: bool,
) -> Result<String> {
    if let Some(path) = file {
        return read_secret_file(name, path, allow_loose)
            .await
            .map_err(|e| tag("config", e));
    }
    let var = env.as_deref().ok_or_else(|| {
        fail(
            "config",
            format!("credential '{name}': no Vault {what} is configured"),
        )
    })?;
    let value = std::env::var(var).map_err(|_| {
        fail(
            "config",
            format!("credential '{name}': the Vault {what} variable {var} is not set"),
        )
    })?;
    let value = value.trim();
    if value.is_empty() || value.chars().any(char::is_control) {
        return Err(fail(
            "config",
            format!("credential '{name}': the Vault {what} variable {var} is empty or contains a control character"),
        ));
    }
    Ok(value.to_string())
}

/// What a failed request to Vault was, without the details of the transport.
fn send_error(name: &str, e: &reqwest::Error) -> anyhow::Error {
    let why = if e.is_timeout() {
        "timed out"
    } else if e.is_connect() {
        "connection failed"
    } else {
        "the request failed"
    };
    fail(
        "unreachable",
        format!("credential '{name}': the secret store could not be reached ({why})"),
    )
}

/// The status of a Vault answer as an error class, or `Ok` for a 200.
fn check_status(name: &str, what: &str, status: reqwest::StatusCode) -> Result<()> {
    match status.as_u16() {
        200 => Ok(()),
        400 | 401 | 403 => Err(fail(
            "refused",
            format!("credential '{name}': the secret store refused the {what} ({})", status.as_u16()),
        )),
        404 => Err(fail(
            "not_found",
            format!("credential '{name}': the {what} target or its mount was not found (404)"),
        )),
        300..=399 => Err(fail(
            "unreachable",
            format!("credential '{name}': the secret store answered a redirect ({status}), which is not followed"),
        )),
        _ => Err(fail(
            "unreachable",
            format!("credential '{name}': the secret store answered {status}"),
        )),
    }
}

/// The body of an answer, refused past 256 KiB.
async fn read_capped(name: &str, mut response: reqwest::Response) -> Result<Vec<u8>> {
    let mut body = Vec::new();
    while let Some(chunk) = response.chunk().await.map_err(|_| {
        fail(
            "unreachable",
            format!("credential '{name}': the secret store's answer could not be read"),
        )
    })? {
        body.extend_from_slice(&chunk);
        if body.len() > MAX_VAULT_ANSWER_BYTES {
            return Err(fail(
                "malformed",
                format!("credential '{name}': the secret store's answer is larger than {MAX_VAULT_ANSWER_BYTES} bytes"),
            ));
        }
    }
    Ok(body)
}

/// `{addr}/v1/auth/{mount}/login`, built from segments like the read URL.
fn vault_login_url(addr: &str, mount: &str) -> Result<url::Url> {
    let mut url = url::Url::parse(addr).context("the Vault address is not a URL")?;
    {
        let mut segments = url
            .path_segments_mut()
            .map_err(|_| anyhow::anyhow!("the Vault address cannot carry a path"))?;
        segments.pop_if_empty().push("v1").push("auth");
        for seg in mount.split('/') {
            segments.push(seg);
        }
        segments.push("login");
    }
    Ok(url)
}

/// Log in with AppRole or Kubernetes and return the token and how long to keep using it. The secret
/// id and the JWT go only in the request body; a refusal says so without echoing the answer.
async fn vault_login(
    name: &str,
    source: &SecretSource,
    client: &reqwest::Client,
) -> Result<(String, Duration)> {
    let SecretSource::Vault {
        addr,
        namespace,
        auth,
        allow_loose_permissions,
        ..
    } = source
    else {
        bail!("credential '{name}' is not a Vault source");
    };
    let (mount, body) = match auth {
        VaultAuth::AppRole {
            role_id,
            secret_id_file,
            secret_id_env,
            mount,
        } => {
            let secret_id = read_configured_secret(
                name,
                "secret_id",
                secret_id_file,
                secret_id_env,
                *allow_loose_permissions,
            )
            .await?;
            (
                mount,
                serde_json::json!({"role_id": role_id, "secret_id": secret_id}),
            )
        }
        VaultAuth::Kubernetes {
            role,
            jwt_file,
            mount,
        } => {
            // A projected service account token is world-readable by design, so it is read as is.
            let jwt = read_secret_file(name, jwt_file, true)
                .await
                .map_err(|e| tag("config", e))?;
            (mount, serde_json::json!({"role": role, "jwt": jwt}))
        }
        VaultAuth::Token { .. } => bail!("credential '{name}' uses a token and does not log in"),
    };
    let mut request = client
        .post(vault_login_url(addr, mount)?)
        .header(reqwest::header::ACCEPT, "application/json")
        .json(&body);
    if let Some(ns) = namespace {
        request = request.header("X-Vault-Namespace", ns.as_str());
    }
    let response = request.send().await.map_err(|e| send_error(name, &e))?;
    check_status(name, "login", response.status())?;
    let bytes = read_capped(name, response).await?;
    let doc: serde_json::Value = serde_json::from_slice(&bytes).map_err(|_| {
        fail(
            "malformed",
            format!("credential '{name}': the secret store's login answer is not JSON"),
        )
    })?;
    let token = doc
        .pointer("/auth/client_token")
        .and_then(|t| t.as_str())
        .map(str::trim)
        .filter(|t| !t.is_empty() && !t.chars().any(char::is_control))
        .ok_or_else(|| {
            fail(
                "malformed",
                format!("credential '{name}': the login answer has no usable client_token"),
            )
        })?
        .to_string();
    let lease = doc
        .pointer("/auth/lease_duration")
        .and_then(|l| l.as_u64())
        .unwrap_or(0);
    // Stop using it a little early. A token that never expires is refreshed on the cache schedule.
    let keep = if lease == 0 {
        DEFAULT_VAULT_TTL_SECS
    } else {
        lease.saturating_sub(30).max(1)
    }
    .min(MAX_SOURCE_TTL_SECS);
    Ok((token, Duration::from_secs(keep)))
}

/// The token to read with, and whether it was just issued by a login. A `token` auth is read from its
/// file or variable each time; a login auth is cached until shortly before it expires.
async fn vault_token_for(
    name: &str,
    source: &SecretSource,
    client: &reqwest::Client,
    state: &mut SourceState,
) -> Result<(String, bool)> {
    let SecretSource::Vault {
        auth,
        allow_loose_permissions,
        ..
    } = source
    else {
        bail!("credential '{name}' is not a Vault source");
    };
    if let VaultAuth::Token {
        token_file,
        token_env,
    } = auth
    {
        let token = read_configured_secret(
            name,
            "token",
            token_file,
            token_env,
            *allow_loose_permissions,
        )
        .await?;
        return Ok((token, false));
    }
    if let Some((token, until)) = &state.login {
        if *until > Instant::now() {
            return Ok((token.clone(), false));
        }
    }
    let (token, keep) = vault_login(name, source, client).await?;
    state.login = Some((token.clone(), Instant::now() + keep));
    Ok((token, true))
}

/// Read a Vault secret: get a token, read, and if a token that was not just issued is refused, get a
/// fresh one (a new login, or the token file read again) and try once more. A token issued a moment
/// ago that is refused means the policy says no, so it is not retried; nothing here loops.
async fn fetch_vault(name: &str, source: &SecretSource, state: &mut SourceState) -> Result<String> {
    if state.client.is_none() {
        let client = vault_client(name, source)
            .await
            .map_err(|e| tag("config", e))?;
        state.client = Some(client);
    }
    let client = state.client.clone().ok_or_else(|| {
        fail(
            "config",
            format!("credential '{name}': no client for the secret store"),
        )
    })?;
    let (token, fresh) = vault_token_for(name, source, &client, state).await?;
    match read_vault_secret(name, source, &client, &token).await {
        Err(e) if failure_class(&e) == "refused" && !fresh => {
            state.login = None;
            let (again, _) = vault_token_for(name, source, &client, state).await?;
            if again == token {
                return Err(e);
            }
            read_vault_secret(name, source, &client, &again).await
        }
        other => other,
    }
}

/// Read one field of a KV v2 secret with `token`. Errors say what class of thing went wrong
/// (unreachable, refused, not found, malformed) and never include the answer, which can echo paths
/// or policy.
async fn read_vault_secret(
    name: &str,
    source: &SecretSource,
    client: &reqwest::Client,
    token: &str,
) -> Result<String> {
    let SecretSource::Vault {
        addr,
        mount,
        path,
        field,
        namespace,
        ..
    } = source
    else {
        bail!("credential '{name}' is not a Vault source");
    };
    let mut token = reqwest::header::HeaderValue::from_str(token).map_err(|_| {
        fail(
            "config",
            format!("credential '{name}': the Vault token is not a valid header value"),
        )
    })?;
    token.set_sensitive(true);
    let mut request = client
        .get(vault_url(addr, mount, path)?)
        .header("X-Vault-Token", token)
        .header(reqwest::header::ACCEPT, "application/json");
    if let Some(ns) = namespace {
        request = request.header("X-Vault-Namespace", ns.as_str());
    }
    let response = request.send().await.map_err(|e| send_error(name, &e))?;
    check_status(name, "token", response.status())?;
    let body = read_capped(name, response).await?;
    let doc: serde_json::Value = serde_json::from_slice(&body).map_err(|_| {
        fail(
            "malformed",
            format!("credential '{name}': the secret store's answer is not JSON"),
        )
    })?;
    let value = doc
        .pointer("/data/data")
        .and_then(|d| d.get(field.as_str()))
        .and_then(|v| v.as_str())
        .ok_or_else(|| {
            fail(
                "malformed",
                format!("credential '{name}': the secret has no text field '{field}' (is it a KV v2 secret?)"),
            )
        })?
        .trim();
    if value.is_empty() {
        return Err(fail(
            "malformed",
            format!("credential '{name}': the field '{field}' of the secret is empty"),
        ));
    }
    if value.chars().any(char::is_control) {
        return Err(fail(
            "malformed",
            format!("credential '{name}': the field '{field}' of the secret contains a control character"),
        ));
    }
    Ok(value.to_string())
}

/// Startup check of a file source. A file that is readable by others is a configuration error and
/// stops startup. A file that is not there yet only warns, because whatever renders it (Vault
/// Agent, a CSI driver) may start after this does.
async fn startup_check_source(name: &str, d: &CredentialDescriptor) -> Result<()> {
    let Some(SecretSource::File {
        path,
        allow_loose_permissions,
        ..
    }) = &d.source
    else {
        return Ok(());
    };
    match tokio::fs::metadata(path).await {
        Ok(_) => {
            // Reuses the read path's checks and discards the value.
            read_secret_file(name, path, *allow_loose_permissions)
                .await
                .map(|_| ())
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            tracing::warn!(credential = %name, path = %path.display(), "the credential's secret file does not exist yet");
            Ok(())
        }
        Err(e) => Err(e).with_context(|| {
            format!(
                "credential '{name}': the secret file {} cannot be inspected",
                path.display()
            )
        }),
    }
}

/// Request context for [`CredentialVault::authorize_resolve`].
#[derive(Debug, Clone, Copy)]
pub struct ResolveContext<'a> {
    pub host: &'a str,
    pub method: &'a reqwest::Method,
    pub path: &'a str,
    pub port: u16,
    pub user_id: Option<&'a str>,
}

/// Refuses a token endpoint that would send an OAuth client secret and refresh token in cleartext:
/// `https` always allowed, plain `http` only to loopback (local testing). Checked at descriptor-validate
/// time (`validate_descriptor`, for the normal `CredentialVault::load` path) *and* again right before the
/// actual request (`mint_access_token`), because `CredentialVault::from_descriptors` is a public
/// constructor for tests and embedders that does not itself call `validate_descriptor`.
fn require_https_or_loopback(name: &str, token_url: &str) -> Result<url::Url> {
    let url = url::Url::parse(token_url)
        .with_context(|| format!("credential '{name}' has an invalid oauth token_url"))?;
    let loopback = matches!(url.host_str(), Some("127.0.0.1" | "localhost" | "[::1]"));
    if url.scheme() != "https" && !(url.scheme() == "http" && loopback) {
        bail!("credential '{name}' oauth token_url must be https (http only for loopback)");
    }
    Ok(url)
}

fn validate_descriptor(name: &str, d: &CredentialDescriptor) -> Result<()> {
    if name.is_empty() || d.host.trim().is_empty() || d.header.trim().is_empty() {
        bail!("credential descriptors require non-empty name, host and header");
    }
    if let Some(source) = &d.source {
        if !d.env.trim().is_empty() {
            bail!("credential '{name}' sets both env and source; choose one");
        }
        if d.kind.eq_ignore_ascii_case("fabric") || d.kind.eq_ignore_ascii_case(KIND_OAUTH_REFRESH)
        {
            bail!(
                "credential '{name}' has kind {} and cannot use a source",
                d.kind
            );
        }
        source.validate(name)?;
    } else if d.kind.eq_ignore_ascii_case(KIND_OAUTH_REFRESH) {
        let Some(o) = &d.oauth else {
            bail!("credential '{name}' has kind oauth-refresh and needs an oauth block");
        };
        require_https_or_loopback(name, &o.token_url)?;
        if o.client_id_env.trim().is_empty() {
            bail!("credential '{name}' oauth needs client_id_env");
        }
        match (&o.connection, o.refresh_token_env.trim().is_empty()) {
            (None, true) => bail!("credential '{name}' oauth needs refresh_token_env, or a connection for a per-person credential"),
            (Some(_), false) => bail!("credential '{name}' oauth sets both refresh_token_env and connection; choose one"),
            (Some(c), true) => {
                if c.is_empty() || c.len() > 32 || !c.bytes().all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-') {
                    bail!("credential '{name}' oauth connection must be 1 to 32 characters of a-z, 0-9 and '-'");
                }
                if d.intercept {
                    bail!("credential '{name}' is per person and cannot be intercepted");
                }
            }
            (None, false) => {}
        }
    } else if !d.kind.eq_ignore_ascii_case("fabric") && d.env.trim().is_empty() {
        bail!("credential '{name}' requires env unless kind is fabric or oauth-refresh");
    }
    if d.header.eq_ignore_ascii_case("host") || d.header.eq_ignore_ascii_case("content-length") {
        bail!("credential '{name}' may not inject the {} header", d.header);
    }
    reqwest::header::HeaderName::from_bytes(d.header.as_bytes())
        .with_context(|| format!("credential '{name}' has an invalid HTTP header name"))?;
    for method in &d.allowed_methods {
        reqwest::Method::from_bytes(method.as_bytes()).with_context(|| {
            format!("credential '{name}' has invalid allowed method '{method}'")
        })?;
    }
    if let Some(kind) = &d.preview {
        if !crate::preview::is_known(kind) {
            bail!("credential '{name}' has an unknown preview '{kind}'");
        }
        if d.requires_approval.is_empty() {
            bail!("credential '{name}' sets a preview but never asks for approval");
        }
    }
    for method in &d.requires_approval {
        if method != "*" {
            reqwest::Method::from_bytes(method.as_bytes()).with_context(|| {
                format!("credential '{name}' has invalid requires_approval method '{method}'")
            })?;
        }
    }
    if let Some(kind) = d.approval_kind.as_deref() {
        if !matches!(kind, "send" | "purchase") {
            bail!("credential '{name}' approval_kind must be 'send' or 'purchase'");
        }
    }
    if d.path_prefixes.iter().any(|p| !p.starts_with('/')) {
        bail!("credential '{name}' path_prefixes must start with '/'");
    }
    if d.allowed_ports.contains(&0) {
        bail!("credential '{name}' allowed_ports may not contain 0");
    }
    Ok(())
}

pub fn credential_allows_request(
    descriptor: &CredentialDescriptor,
    method: &reqwest::Method,
    path: &str,
    port: u16,
) -> bool {
    let method_ok = descriptor.allowed_methods.is_empty()
        || descriptor
            .allowed_methods
            .iter()
            .any(|m| m.eq_ignore_ascii_case(method.as_str()));
    let path_ok = descriptor.path_prefixes.is_empty()
        || descriptor
            .path_prefixes
            .iter()
            .any(|prefix| match prefix.strip_suffix('$') {
                Some(exact) => path == exact,
                None => path.starts_with(prefix),
            });
    let port_ok = port == 443
        || port == 80
        || descriptor.allowed_ports.contains(&port)
        || (descriptor.kind.eq_ignore_ascii_case("fabric")
            && descriptor.allowed_ports.is_empty()
            && (port == 8000 || port == 8080));
    method_ok && path_ok && port_ok
}

pub fn host_matches(pattern: &str, host: &str) -> bool {
    let pattern = pattern
        .trim()
        .trim_start_matches('.')
        .trim_end_matches('.')
        .to_ascii_lowercase();
    let host = host.trim().trim_end_matches('.').to_ascii_lowercase();
    host == pattern || host.ends_with(&format!(".{pattern}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_path_ending_in_a_dollar_sign_matches_that_exact_path_only() {
        let d: CredentialDescriptor = serde_json::from_value(serde_json::json!({
            "host": "h", "header": "authorization", "env": "E",
            "path_prefixes": ["/gmail/v1/users/me/drafts$", "/calendar/"]
        }))
        .unwrap();
        let ok = |p: &str| credential_allows_request(&d, &reqwest::Method::POST, p, 443);
        assert!(ok("/gmail/v1/users/me/drafts"));
        assert!(
            !ok("/gmail/v1/users/me/drafts/send"),
            "a draft is not sent by this credential"
        );
        assert!(!ok("/gmail/v1/users/me/drafts/"));
        assert!(!ok("/gmail/v1/users/me/messages/send"));
        assert!(
            ok("/calendar/v3/anything"),
            "a plain prefix still matches by prefix"
        );
    }

    #[test]
    fn a_preview_must_be_a_known_kind_and_belong_to_a_credential_that_asks_for_approval() {
        let make = |preview: &str, approval: bool| {
            let mut d = serde_json::json!({"host": "h", "header": "authorization", "env": "E", "preview": preview});
            if approval {
                d["requires_approval"] = serde_json::json!(["POST"]);
            }
            serde_json::from_value::<CredentialDescriptor>(d).unwrap()
        };
        assert!(validate_descriptor("m", &make("gmail-message", true)).is_ok());
        assert!(validate_descriptor("m", &make("calendar-event", true)).is_ok());
        assert!(validate_descriptor("m", &make("nonsense", true)).is_err());
        assert!(validate_descriptor("m", &make("gmail-message", false)).is_err());
    }

    #[test]
    fn host_suffix_is_boundary_safe() {
        assert!(host_matches("openai.com", "api.openai.com"));
        assert!(host_matches("api.openai.com", "api.openai.com"));
        assert!(!host_matches("openai.com", "evilopenai.com"));
    }

    #[test]
    fn credential_policy_checks_method_path_and_port() {
        let d = CredentialDescriptor {
            host: "api.example.com".into(),
            header: "authorization".into(),
            env: "EXAMPLE_KEY".into(),
            prefix: "Bearer ".into(),
            allowed_methods: vec!["POST".into()],
            path_prefixes: vec!["/v1/".into()],
            allowed_ports: vec![8443],
            kind: "provider".into(),
            requires_approval: vec![],
            allowed_users: vec![],
            approval_kind: None,
            intercept: false,
            require_device_signature: false,
            preview: None,
            oauth: None,
            source: None,
        };
        assert!(credential_allows_request(
            &d,
            &reqwest::Method::POST,
            "/v1/run",
            443
        ));
        assert!(credential_allows_request(
            &d,
            &reqwest::Method::POST,
            "/v1/run",
            8443
        ));
        assert!(!credential_allows_request(
            &d,
            &reqwest::Method::GET,
            "/v1/run",
            443
        ));
        assert!(!credential_allows_request(
            &d,
            &reqwest::Method::POST,
            "/admin",
            443
        ));
        assert!(!credential_allows_request(
            &d,
            &reqwest::Method::POST,
            "/v1/run",
            9443
        ));
    }

    fn gated(methods: &[&str], kind: Option<&str>) -> CredentialDescriptor {
        serde_json::from_value(serde_json::json!({
            "host": "mail.example", "header": "authorization", "env": "K",
            "requires_approval": methods, "approval_kind": kind,
        }))
        .unwrap()
    }

    #[test]
    fn approval_kind_follows_the_method_list() {
        use crate::model::ApprovalKind;
        let d = gated(&["POST", "delete"], None);
        assert_eq!(
            d.approval_kind_for(&reqwest::Method::POST),
            Some(ApprovalKind::Send)
        );
        assert_eq!(
            d.approval_kind_for(&reqwest::Method::DELETE),
            Some(ApprovalKind::Send)
        );
        assert_eq!(d.approval_kind_for(&reqwest::Method::GET), None);
        let any = gated(&["*"], Some("purchase"));
        assert_eq!(
            any.approval_kind_for(&reqwest::Method::GET),
            Some(ApprovalKind::Purchase)
        );
        assert_eq!(
            gated(&[], Some("purchase")).approval_kind_for(&reqwest::Method::POST),
            None
        );
    }

    #[test]
    fn descriptor_validation_rejects_bad_approval_settings() {
        assert!(validate_descriptor("c", &gated(&["POST", "*"], Some("send"))).is_ok());
        assert!(validate_descriptor("c", &gated(&["PO ST"], None)).is_err());
        assert!(validate_descriptor("c", &gated(&["POST"], Some("wire"))).is_err());
    }

    #[tokio::test]
    async fn authorize_resolve_checks_user_allowlist() {
        std::env::set_var("AUTH_TEST_KEY", "secret-value");
        let mut map = HashMap::new();
        map.insert(
            "mail".into(),
            serde_json::from_value::<CredentialDescriptor>(serde_json::json!({
                "host": "mail.example",
                "header": "authorization",
                "env": "AUTH_TEST_KEY",
                "allowed_users": ["alice"],
            }))
            .unwrap(),
        );
        let vault = CredentialVault::from_descriptors(map);
        let method = reqwest::Method::GET;
        let denied = vault
            .authorize_resolve(
                "mail",
                &ResolveContext {
                    host: "mail.example",
                    method: &method,
                    path: "/",
                    port: 443,
                    user_id: Some("bob"),
                },
            )
            .await;
        assert!(denied.is_err());
        let ok = vault
            .authorize_resolve(
                "mail",
                &ResolveContext {
                    host: "mail.example",
                    method: &method,
                    path: "/",
                    port: 443,
                    user_id: Some("alice"),
                },
            )
            .await
            .unwrap();
        assert!(ok.1.contains("secret-value"));
        std::env::remove_var("AUTH_TEST_KEY");
    }

    /// A token endpoint on loopback that records each form body and answers with `reply`.
    async fn fake_token_endpoint(
        status: u16,
        reply: serde_json::Value,
    ) -> (String, Arc<std::sync::Mutex<Vec<String>>>) {
        use axum::{extract::State, http::StatusCode, routing::post, Router};
        type Seen = Arc<std::sync::Mutex<Vec<String>>>;
        let seen: Seen = Arc::default();
        let app = Router::new()
            .route(
                "/token",
                post(
                    |State((seen, status, reply)): State<(Seen, u16, serde_json::Value)>,
                     body: String| async move {
                        seen.lock().unwrap().push(body);
                        (StatusCode::from_u16(status).unwrap(), axum::Json(reply))
                    },
                ),
            )
            .with_state((seen.clone(), status, reply));
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}/token", listener.local_addr().unwrap());
        tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        (url, seen)
    }

    fn oauth_vault(token_url: &str, tag: &str) -> CredentialVault {
        std::env::set_var(format!("OA_{tag}_ID"), "client-id-1");
        std::env::set_var(format!("OA_{tag}_SECRET"), "client-secret-1");
        std::env::set_var(format!("OA_{tag}_REFRESH"), "refresh-token-1");
        let d: CredentialDescriptor = serde_json::from_value(serde_json::json!({
            "host": "gmail.googleapis.com", "header": "authorization", "kind": "oauth-refresh",
            "allowed_methods": ["GET"],
            "oauth": {
                "token_url": token_url,
                "client_id_env": format!("OA_{tag}_ID"),
                "client_secret_env": format!("OA_{tag}_SECRET"),
                "refresh_token_env": format!("OA_{tag}_REFRESH"),
            }
        }))
        .unwrap();
        validate_descriptor("google", &d).unwrap();
        CredentialVault::from_descriptors(HashMap::from([("google".to_string(), d)]))
    }

    /// `from_descriptors` is a public constructor for tests and embedders that skips `validate_descriptor`
    /// (unlike `CredentialVault::load`), so the https-or-loopback rule has to hold again right where the
    /// client secret and refresh token actually go on the wire, not only at descriptor-validate time.
    #[tokio::test]
    async fn a_descriptor_that_skipped_validation_still_cannot_refresh_over_plain_http() {
        std::env::set_var("OA_SKIP_ID", "client-id-1");
        std::env::set_var("OA_SKIP_SECRET", "client-secret-1");
        std::env::set_var("OA_SKIP_REFRESH", "refresh-token-1");
        let d: CredentialDescriptor = serde_json::from_value(serde_json::json!({
            "host": "gmail.googleapis.com", "header": "authorization", "kind": "oauth-refresh",
            "allowed_methods": ["GET"],
            "oauth": {
                "token_url": "http://oauth2.example/token", // plain http, not loopback: never validated
                "client_id_env": "OA_SKIP_ID",
                "client_secret_env": "OA_SKIP_SECRET",
                "refresh_token_env": "OA_SKIP_REFRESH",
            }
        }))
        .unwrap();
        // Deliberately not calling validate_descriptor, unlike every other test in this module.
        let vault = CredentialVault::from_descriptors(HashMap::from([("google".to_string(), d)]));
        let error = vault
            .refresh_now("google", &reqwest::Client::new())
            .await
            .unwrap_err();
        assert!(
            error.to_string().contains("https"),
            "expected an https refusal, got: {error}"
        );
    }

    #[tokio::test]
    async fn oauth_refresh_mints_caches_and_injects_a_bearer_token() {
        let (url, seen) = fake_token_endpoint(
            200,
            serde_json::json!({"access_token": "ya29.fresh", "expires_in": 3599, "token_type": "Bearer"}),
        )
        .await;
        let vault = oauth_vault(&url, "OK");
        // Fails closed before the first refresh.
        assert!(vault.resolve("google").is_err());
        let life = vault
            .refresh_now("google", &reqwest::Client::new())
            .await
            .unwrap();
        assert_eq!(life.as_secs(), 3599);
        let (_, value) = vault.resolve("google").unwrap();
        assert_eq!(value, "Bearer ya29.fresh");
        let body = seen.lock().unwrap()[0].clone();
        assert!(body.contains("grant_type=refresh_token"));
        assert!(body.contains("client_id=client-id-1"));
        assert!(body.contains("client_secret=client-secret-1"));
        assert!(body.contains("refresh_token=refresh-token-1"));
        // The policy checks still apply through authorize_resolve.
        let get = reqwest::Method::GET;
        let post = reqwest::Method::POST;
        let ctx = |m| ResolveContext {
            host: "gmail.googleapis.com",
            method: m,
            path: "/gmail/v1/users/me/messages",
            port: 443,
            user_id: None,
        };
        assert!(vault.authorize_resolve("google", &ctx(&get)).await.is_ok());
        assert!(vault
            .authorize_resolve("google", &ctx(&post))
            .await
            .is_err());
    }

    #[tokio::test]
    async fn oauth_refresh_failure_keeps_the_credential_closed_and_leaks_nothing() {
        let (url, _) = fake_token_endpoint(
            400,
            serde_json::json!({"error": "invalid_grant", "error_description": "Token has been revoked. refresh-token-1"}),
        )
        .await;
        let vault = oauth_vault(&url, "BAD");
        let error = vault
            .refresh_now("google", &reqwest::Client::new())
            .await
            .unwrap_err();
        let text = format!("{error:#}");
        assert!(text.contains("invalid_grant"), "{text}");
        assert!(
            !text.contains("refresh-token-1") && !text.contains("client-secret-1"),
            "{text}"
        );
        assert!(vault.resolve("google").is_err());
    }

    #[tokio::test]
    async fn oauth_token_stops_being_used_after_it_expires() {
        let (url, _) = fake_token_endpoint(
            200,
            serde_json::json!({"access_token": "short", "expires_in": 1}),
        )
        .await;
        let vault = oauth_vault(&url, "EXP");
        vault
            .refresh_now("google", &reqwest::Client::new())
            .await
            .unwrap();
        assert!(vault.resolve("google").is_ok());
        tokio::time::sleep(Duration::from_millis(1150)).await;
        assert!(vault.resolve("google").is_err());
    }

    #[tokio::test]
    async fn background_refresher_gets_the_first_token_at_startup() {
        let (url, _) = fake_token_endpoint(
            200,
            serde_json::json!({"access_token": "boot", "expires_in": 3600}),
        )
        .await;
        let vault = oauth_vault(&url, "BOOT");
        vault.start_oauth_refresh(reqwest::Client::new()).await;
        assert_eq!(vault.resolve("google").unwrap().1, "Bearer boot");
    }

    #[test]
    fn oauth_descriptor_validation() {
        let make =
            |v: serde_json::Value| serde_json::from_value::<CredentialDescriptor>(v).unwrap();
        let base = |oauth: serde_json::Value| {
            make(serde_json::json!({
                "host": "gmail.googleapis.com", "header": "authorization", "kind": "oauth-refresh", "oauth": oauth,
            }))
        };
        let ok = serde_json::json!({"token_url": "https://oauth2.googleapis.com/token", "client_id_env": "A", "client_secret_env": "B", "refresh_token_env": "C"});
        assert!(validate_descriptor("g", &base(ok.clone())).is_ok());
        let mut plain_http = ok.clone();
        plain_http["token_url"] = "http://oauth2.example/token".into();
        assert!(validate_descriptor("g", &base(plain_http)).is_err());
        let mut loopback = ok.clone();
        loopback["token_url"] = "http://127.0.0.1:9/token".into();
        assert!(validate_descriptor("g", &base(loopback)).is_ok());
        let mut no_refresh = ok.clone();
        no_refresh["refresh_token_env"] = "".into();
        assert!(validate_descriptor("g", &base(no_refresh)).is_err());
        let missing = make(
            serde_json::json!({"host": "h", "header": "authorization", "kind": "oauth-refresh"}),
        );
        assert!(validate_descriptor("g", &missing).is_err());
    }

    /// A token endpoint that answers `access_token = "at-" + the refresh token it was given`, and records the refresh tokens seen.
    async fn per_person_token_endpoint() -> (String, Arc<std::sync::Mutex<Vec<String>>>) {
        use axum::{extract::State, routing::post, Router};
        let seen: Arc<std::sync::Mutex<Vec<String>>> = Arc::default();
        let app = Router::new()
            .route(
                "/token",
                post(|State(seen): State<Arc<std::sync::Mutex<Vec<String>>>>, body: String| async move {
                    let rt = url::form_urlencoded::parse(body.as_bytes()).find(|(k, _)| k == "refresh_token").map(|(_, v)| v.to_string()).unwrap_or_default();
                    seen.lock().unwrap().push(rt.clone());
                    axum::Json(serde_json::json!({"access_token": format!("at-{rt}"), "expires_in": 3600}))
                }),
            )
            .with_state(seen.clone());
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}/token", listener.local_addr().unwrap());
        tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        (url, seen)
    }

    fn person_vault(token_url: &str) -> CredentialVault {
        std::env::set_var("PP_CLIENT_ID", "cid");
        std::env::set_var("PP_CLIENT_SECRET", "csecret");
        let d: CredentialDescriptor = serde_json::from_value(serde_json::json!({
            "host": "gmail.googleapis.com", "header": "authorization", "kind": "oauth-refresh", "allowed_methods": ["GET"],
            "oauth": {"token_url": token_url, "client_id_env": "PP_CLIENT_ID", "client_secret_env": "PP_CLIENT_SECRET", "connection": "google"}
        }))
        .unwrap();
        validate_descriptor("gmail", &d).unwrap();
        CredentialVault::from_descriptors(HashMap::from([("gmail".to_string(), d)]))
    }

    #[tokio::test]
    async fn a_per_person_credential_uses_each_persons_own_refresh_token_and_caches_it() {
        let (url, seen) = per_person_token_endpoint().await;
        let vault = person_vault(&url);
        let http = reqwest::Client::new();
        assert!(vault.is_per_person("gmail") && vault.connection_of("gmail") == Some("google"));
        assert_eq!(vault.connection_names(), ["google"]);
        // nothing works without a user, or before the person connected
        assert!(
            vault.resolve_for("gmail", None).is_err(),
            "a session with no user"
        );
        assert!(vault.resolve("gmail").is_err(), "and the host-wide path");
        let err = vault
            .ensure_user_token("gmail", "ana", None, &http)
            .await
            .unwrap_err()
            .to_string();
        assert!(err.contains("connect your google account first"), "{err}");
        assert!(
            seen.lock().unwrap().is_empty(),
            "nothing was sent to the token endpoint"
        );
        // two people, two tokens, each minted from their own refresh token
        vault
            .ensure_user_token("gmail", "ana", Some("1//ana-refresh"), &http)
            .await
            .unwrap();
        vault
            .ensure_user_token("gmail", "ben", Some("1//ben-refresh"), &http)
            .await
            .unwrap();
        assert_eq!(
            vault.resolve_for("gmail", Some("ana")).unwrap().1,
            "Bearer at-1//ana-refresh"
        );
        assert_eq!(
            vault.resolve_for("gmail", Some("ben")).unwrap().1,
            "Bearer at-1//ben-refresh"
        );
        assert!(
            vault.resolve_for("gmail", Some("cam")).is_err(),
            "a person who has not connected gets nothing"
        );
        // a second use does not call the endpoint again
        vault
            .ensure_user_token("gmail", "ana", Some("1//ana-refresh"), &http)
            .await
            .unwrap();
        assert_eq!(*seen.lock().unwrap(), ["1//ana-refresh", "1//ben-refresh"]);
        // the policy still applies through authorize_resolve
        let get = reqwest::Method::GET;
        let post = reqwest::Method::POST;
        let ctx = |m, u| ResolveContext {
            host: "gmail.googleapis.com",
            method: m,
            path: "/gmail/v1/users/me/messages",
            port: 443,
            user_id: u,
        };
        assert!(vault
            .authorize_resolve("gmail", &ctx(&get, Some("ana")))
            .await
            .is_ok());
        assert!(vault
            .authorize_resolve("gmail", &ctx(&post, Some("ana")))
            .await
            .is_err());
        assert!(vault
            .authorize_resolve("gmail", &ctx(&get, None))
            .await
            .is_err());
    }

    #[tokio::test]
    async fn disconnecting_or_replacing_a_connection_drops_only_that_persons_tokens() {
        let (url, seen) = per_person_token_endpoint().await;
        let vault = person_vault(&url);
        let http = reqwest::Client::new();
        vault
            .ensure_user_token("gmail", "ana", Some("1//ana-refresh"), &http)
            .await
            .unwrap();
        vault
            .ensure_user_token("gmail", "ben", Some("1//ben-refresh"), &http)
            .await
            .unwrap();
        vault.forget_user_connection("ana", "google");
        assert!(
            vault.resolve_for("gmail", Some("ana")).is_err(),
            "ana's token is gone at once"
        );
        assert!(
            vault.resolve_for("gmail", Some("ben")).is_ok(),
            "ben's is not"
        );
        vault.forget_user_connection("ben", "another-connection");
        assert!(
            vault.resolve_for("gmail", Some("ben")).is_ok(),
            "a different connection name leaves it alone"
        );
        // a new refresh token mints a new access token
        vault
            .ensure_user_token("gmail", "ana", Some("1//ana-new"), &http)
            .await
            .unwrap();
        assert_eq!(
            vault.resolve_for("gmail", Some("ana")).unwrap().1,
            "Bearer at-1//ana-new"
        );
        assert_eq!(seen.lock().unwrap().len(), 3);
    }

    #[tokio::test]
    async fn a_refused_refresh_names_the_error_code_and_no_secret() {
        use axum::{routing::post, Router};
        let app = Router::new().route("/token", post(|| async { (axum::http::StatusCode::BAD_REQUEST, axum::Json(serde_json::json!({"error": "invalid_grant", "error_description": "Token has been revoked 1//ana-refresh"}))) }));
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}/token", listener.local_addr().unwrap());
        tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        let vault = person_vault(&url);
        let err = format!(
            "{:#}",
            vault
                .ensure_user_token(
                    "gmail",
                    "ana",
                    Some("1//ana-refresh"),
                    &reqwest::Client::new()
                )
                .await
                .unwrap_err()
        );
        assert!(
            err.contains("invalid_grant")
                && !err.contains("1//ana-refresh")
                && !err.contains("csecret"),
            "{err}"
        );
        assert!(
            vault.resolve_for("gmail", Some("ana")).is_err(),
            "and it stays closed"
        );
    }

    #[test]
    fn the_documented_google_examples_are_valid_descriptors() {
        let dir =
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../docs/merion/connectors");
        for (file, per_person) in [
            ("google.credentials.json", false),
            ("google.per-person.credentials.json", true),
        ] {
            let text = std::fs::read_to_string(dir.join(file)).unwrap();
            let map: HashMap<String, CredentialDescriptor> = serde_json::from_str(&text).unwrap();
            assert_eq!(map.len(), 5, "{file}");
            for (name, d) in &map {
                validate_descriptor(name, d).unwrap_or_else(|e| panic!("{file}: {e}"));
            }
            assert_eq!(
                CredentialVault::from_descriptors(map).is_per_person("gmail-read"),
                per_person,
                "{file}"
            );
        }
    }

    #[test]
    fn the_documented_price_watch_example_is_a_valid_plain_provider_descriptor() {
        let dir =
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../docs/merion/connectors");
        let text = std::fs::read_to_string(dir.join("price-watch.credentials.json")).unwrap();
        let map: HashMap<String, CredentialDescriptor> = serde_json::from_str(&text).unwrap();
        assert_eq!(map.len(), 1);
        let d = &map["price-watch-read"];
        validate_descriptor("price-watch-read", d).unwrap();
        assert_eq!(
            d.kind,
            default_kind(),
            "the plain, default kind: no OAuth, no Rust code needed to add it"
        );
        assert!(d.oauth.is_none());
        assert_eq!(d.allowed_methods, vec!["GET".to_string()]);
    }

    #[test]
    fn the_documented_microsoft_examples_are_valid_narrow_per_person_descriptors() {
        let dir =
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../docs/merion/connectors");
        let text =
            std::fs::read_to_string(dir.join("microsoft.per-person.credentials.json")).unwrap();
        let map: HashMap<String, CredentialDescriptor> = serde_json::from_str(&text).unwrap();
        assert_eq!(map.len(), 5);
        for (name, d) in &map {
            validate_descriptor(name, d).unwrap();
            let o = d.oauth.as_ref().unwrap();
            assert_eq!(
                o.connection.as_deref(),
                Some("microsoft"),
                "{name}: rotating refresh tokens need a per-person connection"
            );
            assert!(o.client_secret_env.is_empty(), "{name}: a public client");
            let scopes: Vec<&str> = o.scope.split_whitespace().collect();
            assert_eq!(
                scopes.len(),
                2,
                "{name}: offline_access and exactly one Graph permission: {scopes:?}"
            );
            assert_eq!(d.host, "graph.microsoft.com");
            if d.allowed_methods.iter().any(|m| m == "POST") {
                assert!(
                    d.requires_approval.iter().any(|m| m == "POST")
                        && d.require_device_signature
                        && d.preview.is_some(),
                    "{name}: a write needs a signed decision on a preview"
                );
                assert!(
                    d.path_prefixes.iter().all(|p| p.ends_with('$')),
                    "{name}: writes are exact paths"
                );
            }
        }
        let allows = |name: &str, method: reqwest::Method, path: &str| {
            credential_allows_request(&map[name], &method, path, 443)
        };
        use reqwest::Method as M;
        assert!(allows("outlook-read", M::GET, "/v1.0/me/messages/AAMk123"));
        assert!(!allows("outlook-read", M::POST, "/v1.0/me/sendMail"));
        assert!(allows("outlook-draft", M::POST, "/v1.0/me/messages"));
        assert!(
            !allows("outlook-draft", M::POST, "/v1.0/me/messages/AAMk123/send"),
            "a draft credential cannot send a draft"
        );
        assert!(!allows("outlook-draft", M::POST, "/v1.0/me/sendMail"));
        assert!(allows("outlook-send", M::POST, "/v1.0/me/sendMail"));
        assert!(!allows("outlook-send", M::POST, "/v1.0/me/messages"));
        assert!(allows("outlook-calendar-write", M::POST, "/v1.0/me/events"));
        assert!(!allows(
            "outlook-calendar-write",
            M::POST,
            "/v1.0/me/events/abc/accept"
        ));
        assert!(!allows(
            "outlook-calendar-write",
            M::DELETE,
            "/v1.0/me/events"
        ));
    }

    #[test]
    fn a_descriptor_uses_either_a_host_refresh_token_or_a_persons_connection() {
        let make = |oauth: serde_json::Value, extra: serde_json::Value| {
            let mut d = serde_json::json!({"host": "h", "header": "authorization", "kind": "oauth-refresh", "oauth": oauth});
            for (k, v) in extra.as_object().cloned().unwrap_or_default() {
                d[k] = v;
            }
            serde_json::from_value::<CredentialDescriptor>(d).unwrap()
        };
        let base = |rt: &str, conn: Option<&str>| {
            let mut o = serde_json::json!({"token_url": "https://oauth2.googleapis.com/token", "client_id_env": "A", "client_secret_env": "B", "refresh_token_env": rt});
            if let Some(c) = conn {
                o["connection"] = c.into();
            }
            o
        };
        assert!(
            validate_descriptor("g", &make(base("C", None), serde_json::json!({}))).is_ok(),
            "host-wide"
        );
        assert!(
            validate_descriptor("g", &make(base("", Some("google")), serde_json::json!({})))
                .is_ok(),
            "per person"
        );
        assert!(
            validate_descriptor("g", &make(base("", None), serde_json::json!({}))).is_err(),
            "neither"
        );
        assert!(
            validate_descriptor("g", &make(base("C", Some("google")), serde_json::json!({})))
                .is_err(),
            "both"
        );
        for bad in ["", "Google", "with space", &"x".repeat(33)] {
            assert!(
                validate_descriptor("g", &make(base("", Some(bad)), serde_json::json!({})))
                    .is_err(),
                "{bad:?}"
            );
        }
        assert!(
            validate_descriptor(
                "g",
                &make(
                    base("", Some("google")),
                    serde_json::json!({"intercept": true})
                )
            )
            .is_err(),
            "a per-person credential is not intercepted"
        );
    }

    #[tokio::test]
    async fn the_host_wide_refresher_leaves_per_person_credentials_alone() {
        let (url, seen) = per_person_token_endpoint().await;
        let vault = person_vault(&url);
        vault.start_oauth_refresh(reqwest::Client::new()).await;
        assert!(
            seen.lock().unwrap().is_empty(),
            "no host refresh token exists for a per-person credential"
        );
    }

    #[tokio::test]
    async fn a_scope_narrows_the_token_and_a_public_client_sends_no_secret() {
        let (url, seen) = fake_token_endpoint(
            200,
            serde_json::json!({"access_token": "at", "expires_in": 3600}),
        )
        .await;
        std::env::set_var("SC_ID", "app-id");
        std::env::remove_var("SC_NO_SECRET");
        let make = |scope: &str| {
            let d: CredentialDescriptor = serde_json::from_value(serde_json::json!({
                "host": "graph.microsoft.com", "header": "authorization", "kind": "oauth-refresh", "allowed_methods": ["GET"],
                "oauth": {"token_url": url, "client_id_env": "SC_ID", "client_secret_env": "SC_NO_SECRET", "connection": "microsoft", "scope": scope}
            }))
            .unwrap();
            validate_descriptor("outlook", &d).unwrap();
            CredentialVault::from_descriptors(HashMap::from([("outlook".to_string(), d)]))
        };
        let http = reqwest::Client::new();
        make("https://graph.microsoft.com/Mail.Read offline_access")
            .ensure_user_token("outlook", "ana", Some("rt-1"), &http)
            .await
            .unwrap();
        make("")
            .ensure_user_token("outlook", "ana", Some("rt-1"), &http)
            .await
            .unwrap();
        let bodies = seen.lock().unwrap().clone();
        assert!(
            bodies[0]
                .contains("scope=https%3A%2F%2Fgraph.microsoft.com%2FMail.Read+offline_access"),
            "{}",
            bodies[0]
        );
        assert!(
            !bodies[1].contains("scope="),
            "no scope when none is set: {}",
            bodies[1]
        );
        for b in &bodies {
            assert!(
                b.contains("client_id=app-id") && !b.contains("client_secret"),
                "a public client has no secret to send: {b}"
            );
        }
    }

    #[tokio::test]
    async fn a_rotated_refresh_token_is_handed_back_only_when_it_changed() {
        let (url, _) = fake_token_endpoint(
            200,
            serde_json::json!({"access_token": "at", "expires_in": 3600, "refresh_token": "rt-2"}),
        )
        .await;
        std::env::set_var("RT_ID", "app-id");
        let d: CredentialDescriptor = serde_json::from_value(serde_json::json!({
            "host": "graph.microsoft.com", "header": "authorization", "kind": "oauth-refresh", "allowed_methods": ["GET"],
            "oauth": {"token_url": url, "client_id_env": "RT_ID", "client_secret_env": "", "connection": "microsoft"}
        }))
        .unwrap();
        let vault = CredentialVault::from_descriptors(HashMap::from([("outlook".to_string(), d)]));
        let http = reqwest::Client::new();
        assert_eq!(
            vault
                .ensure_user_token("outlook", "ana", Some("rt-1"), &http)
                .await
                .unwrap()
                .as_deref(),
            Some("rt-2"),
            "the endpoint rotated it"
        );
        assert_eq!(
            vault
                .ensure_user_token("outlook", "ana", Some("rt-1"), &http)
                .await
                .unwrap(),
            None,
            "a cached token asks nothing"
        );
        assert_eq!(
            vault
                .ensure_user_token("outlook", "ben", Some("rt-2"), &http)
                .await
                .unwrap(),
            None,
            "the same refresh token echoed back is not a change"
        );
    }

    // ---- sources -------------------------------------------------------------------------

    fn scratch_dir() -> PathBuf {
        let d = std::env::temp_dir().join(format!("zyvor-src-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    fn write_secret(dir: &Path, name: &str, content: &str, mode: u32) -> PathBuf {
        use std::os::unix::fs::PermissionsExt;
        let path = dir.join(name);
        std::fs::write(&path, content).unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(mode)).unwrap();
        path
    }

    fn file_vault(path: &Path, extra: serde_json::Value) -> CredentialVault {
        let mut d = serde_json::json!({
            "host": "api.example.com", "header": "authorization", "prefix": "Bearer ",
            "source": {"kind": "file", "path": path},
        });
        for (k, v) in extra.as_object().cloned().unwrap_or_default() {
            d["source"][k] = v;
        }
        let mut map = HashMap::new();
        map.insert("svc".to_string(), serde_json::from_value(d).unwrap());
        CredentialVault::from_descriptors(map)
    }

    fn ctx_for<'a>(method: &'a reqwest::Method) -> ResolveContext<'a> {
        ResolveContext {
            host: "api.example.com",
            method,
            path: "/v1/x",
            port: 443,
            user_id: None,
        }
    }

    async fn resolve(v: &CredentialVault) -> Result<String> {
        let get = reqwest::Method::GET;
        v.authorize_resolve("svc", &ctx_for(&get))
            .await
            .map(|(_, s)| s)
    }

    #[tokio::test]
    async fn a_file_source_is_read_trimmed_and_prefixed() {
        let dir = scratch_dir();
        let path = write_secret(&dir, "k", "  s3cr3t-value \n", 0o600);
        let v = file_vault(&path, serde_json::json!({}));
        assert_eq!(resolve(&v).await.unwrap(), "Bearer s3cr3t-value");
        assert_eq!(v.source_fetches(), 1);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn the_value_is_cached_until_its_ttl_and_then_read_again() {
        let dir = scratch_dir();
        let path = write_secret(&dir, "k", "one", 0o600);
        let v = file_vault(&path, serde_json::json!({"ttl_seconds": 1}));
        assert_eq!(resolve(&v).await.unwrap(), "Bearer one");
        write_secret(&dir, "k", "two", 0o600);
        assert_eq!(resolve(&v).await.unwrap(), "Bearer one", "still cached");
        assert_eq!(v.source_fetches(), 1);
        tokio::time::sleep(Duration::from_millis(1150)).await;
        assert_eq!(
            resolve(&v).await.unwrap(),
            "Bearer two",
            "rotated after the ttl"
        );
        assert_eq!(v.source_fetches(), 2);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn a_burst_after_expiry_reads_the_file_once() {
        let dir = scratch_dir();
        let path = write_secret(&dir, "k", "burst", 0o600);
        let v = Arc::new(file_vault(&path, serde_json::json!({})));
        let mut tasks = Vec::new();
        for _ in 0..32 {
            let v = v.clone();
            tasks.push(tokio::spawn(async move { resolve(&v).await.unwrap() }));
        }
        for t in tasks {
            assert_eq!(t.await.unwrap(), "Bearer burst");
        }
        assert_eq!(v.source_fetches(), 1);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn a_failed_read_fails_closed_and_never_serves_the_old_value() {
        let dir = scratch_dir();
        let path = write_secret(&dir, "k", "old-secret", 0o600);
        let v = file_vault(&path, serde_json::json!({"ttl_seconds": 1}));
        assert_eq!(resolve(&v).await.unwrap(), "Bearer old-secret");
        std::fs::remove_file(&path).unwrap();
        // Inside the ttl the cached value is still good; that is the revocation latency.
        assert!(resolve(&v).await.is_ok());
        tokio::time::sleep(Duration::from_millis(1150)).await;
        let err = resolve(&v).await.unwrap_err().to_string();
        assert!(err.contains("cannot be opened"), "{err}");
        assert!(
            !err.contains("old-secret"),
            "the error leaked the secret: {err}"
        );
        // A failed read clears the cache; when the file returns, the new value is read.
        write_secret(&dir, "k", "new-secret", 0o600);
        assert_eq!(resolve(&v).await.unwrap(), "Bearer new-secret");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn a_secret_file_must_be_private_unless_the_operator_says_otherwise() {
        let dir = scratch_dir();
        let loose = write_secret(&dir, "loose", "s", 0o644);
        let err = resolve(&file_vault(&loose, serde_json::json!({})))
            .await
            .unwrap_err()
            .to_string();
        assert!(err.contains("readable by group or others"), "{err}");
        assert!(err.contains("0644"), "{err}");
        assert!(!err.contains("Bearer s"), "{err}");
        let ok = file_vault(&loose, serde_json::json!({"allow_loose_permissions": true}));
        assert_eq!(resolve(&ok).await.unwrap(), "Bearer s");
        let group = write_secret(&dir, "group", "s", 0o640);
        assert!(resolve(&file_vault(&group, serde_json::json!({})))
            .await
            .is_err());
        let owner_only = write_secret(&dir, "ro", "s", 0o400);
        assert!(resolve(&file_vault(&owner_only, serde_json::json!({})))
            .await
            .is_ok());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn symlinks_are_followed_but_only_to_a_regular_file() {
        let dir = scratch_dir();
        let real = write_secret(&dir, "real", "through-a-link", 0o600);
        // Kubernetes mounts a secret file as a symlink into a timestamped directory.
        let link = dir.join("link");
        std::os::unix::fs::symlink(&real, &link).unwrap();
        assert_eq!(
            resolve(&file_vault(&link, serde_json::json!({})))
                .await
                .unwrap(),
            "Bearer through-a-link"
        );
        let err = resolve(&file_vault(&dir, serde_json::json!({})))
            .await
            .unwrap_err()
            .to_string();
        assert!(
            err.contains("not a regular file") || err.contains("cannot be"),
            "{err}"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn an_empty_oversized_binary_or_multiline_secret_is_refused_without_echoing_it() {
        let dir = scratch_dir();
        for (name, content, want) in [
            ("empty", "  \n".to_string(), "is empty"),
            ("big", "x".repeat(70_000), "larger than"),
            (
                "multi",
                "line-one\nline-two".to_string(),
                "control character",
            ),
            ("nul", "abc\u{0}def".to_string(), "control character"),
        ] {
            let path = write_secret(&dir, name, &content, 0o600);
            let err = resolve(&file_vault(&path, serde_json::json!({})))
                .await
                .unwrap_err()
                .to_string();
            assert!(err.contains(want), "{name}: {err}");
            assert!(
                !err.contains("line-one") && !err.contains("abc"),
                "{name} leaked: {err}"
            );
        }
        let bin = dir.join("bin");
        std::fs::write(&bin, [0xff, 0xfe, 0xfd]).unwrap();
        std::fs::set_permissions(&bin, std::os::unix::fs::PermissionsExt::from_mode(0o600))
            .unwrap();
        let err = resolve(&file_vault(&bin, serde_json::json!({})))
            .await
            .unwrap_err()
            .to_string();
        assert!(err.contains("UTF-8"), "{err}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn a_request_that_fails_the_descriptors_checks_never_reads_the_file() {
        let dir = scratch_dir();
        let path = write_secret(&dir, "k", "guarded", 0o600);
        let v = file_vault(&path, serde_json::json!({}));
        let mut map = HashMap::new();
        let mut d = v.descriptor("svc").unwrap().clone();
        d.allowed_methods = vec!["GET".into()];
        d.allowed_users = vec!["alice".into()];
        map.insert("svc".to_string(), d);
        let v = CredentialVault::from_descriptors(map);
        let get = reqwest::Method::GET;
        let post = reqwest::Method::POST;
        let deny = |m: &reqwest::Method, host: &'static str, user: Option<&'static str>| {
            let v = v.clone();
            let m = m.clone();
            async move {
                v.authorize_resolve(
                    "svc",
                    &ResolveContext {
                        host,
                        method: &m,
                        path: "/v1/x",
                        port: 443,
                        user_id: user,
                    },
                )
                .await
                .is_err()
            }
        };
        assert!(deny(&post, "api.example.com", Some("alice")).await); // method
        assert!(deny(&get, "evil.example", Some("alice")).await); // host
        assert!(deny(&get, "api.example.com", Some("bob")).await); // user
        assert!(deny(&get, "api.example.com", None).await); // no user
        assert_eq!(
            v.source_fetches(),
            0,
            "a refused request must not touch the secret"
        );
        let ok = v
            .authorize_resolve(
                "svc",
                &ResolveContext {
                    host: "api.example.com",
                    method: &get,
                    path: "/v1/x",
                    port: 443,
                    user_id: Some("alice"),
                },
            )
            .await
            .unwrap();
        assert_eq!(ok.1, "Bearer guarded");
        assert_eq!(v.source_fetches(), 1);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_sync_path_refuses_a_sourced_credential_and_the_debug_output_hides_secrets() {
        let dir = scratch_dir();
        let path = write_secret(&dir, "k", "never-printed", 0o600);
        let v = file_vault(&path, serde_json::json!({}));
        assert!(v
            .resolve("svc")
            .unwrap_err()
            .to_string()
            .contains("asynchronously"));
        let token = CachedToken {
            value: "tok-secret".into(),
            valid_until: Instant::now(),
        };
        let printed = format!("{token:?}");
        assert!(
            printed.contains("<redacted>") && !printed.contains("tok-secret"),
            "{printed}"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn descriptor_validation_covers_sources() {
        let dir = scratch_dir();
        let path = write_secret(&dir, "k", "v", 0o600);
        let file = dir.join("creds.json");
        let load = |d: serde_json::Value| {
            std::fs::write(&file, serde_json::json!({"svc": d}).to_string()).unwrap();
            let file = file.clone();
            async move { CredentialVault::load(Some(&file)).await }
        };
        let base = |source: serde_json::Value| serde_json::json!({"host": "h.example", "header": "authorization", "source": source});
        let src = serde_json::json!({"kind": "file", "path": path});
        assert!(load(base(src.clone())).await.is_ok());
        // both env and source
        let mut both = base(src.clone());
        both["env"] = "X".into();
        assert!(load(both)
            .await
            .unwrap_err()
            .to_string()
            .contains("both env and source"));
        // not for fabric or oauth
        let mut fabric = base(src.clone());
        fabric["kind"] = "fabric".into();
        assert!(load(fabric)
            .await
            .unwrap_err()
            .to_string()
            .contains("cannot use a source"));
        // path and ttl rules, unknown kind and unknown field
        for bad in [
            serde_json::json!({"kind": "file", "path": "relative/secret"}),
            serde_json::json!({"kind": "file", "path": "/etc/../etc/secret"}),
            serde_json::json!({"kind": "file", "path": path, "ttl_seconds": 0}),
            serde_json::json!({"kind": "file", "path": path, "ttl_seconds": 3601}),
            serde_json::json!({"kind": "tape", "path": path}),
            serde_json::json!({"kind": "file", "path": path, "extra": 1}),
        ] {
            assert!(
                load(base(bad.clone())).await.is_err(),
                "{bad} should be refused"
            );
        }
        // A loose file stops startup; a missing one only warns.
        let loose = write_secret(&dir, "loose", "v", 0o644);
        let err = load(base(serde_json::json!({"kind": "file", "path": loose})))
            .await
            .err()
            .unwrap()
            .to_string();
        assert!(err.contains("readable by group or others"), "{err}");
        assert!(load(base(
            serde_json::json!({"kind": "file", "path": dir.join("not-yet")})
        ))
        .await
        .is_ok());
        // A descriptor with env and no source still loads, as before.
        assert!(load(
            serde_json::json!({"host": "h.example", "header": "authorization", "env": "X"})
        )
        .await
        .is_ok());
        let _ = std::fs::remove_dir_all(&dir);
    }

    // ---- vault source --------------------------------------------------------------------

    #[derive(Clone)]
    struct Reply {
        status: u16,
        body: String,
        delay_ms: u64,
        location: Option<String>,
    }

    /// path, X-Vault-Token, X-Vault-Namespace of each request the mock saw.
    type Hits = Arc<std::sync::Mutex<Vec<(String, Option<String>, Option<String>)>>>;

    fn kv(field: &str, value: &str) -> String {
        serde_json::json!({"data": {"data": {field: value}, "metadata": {"version": 1}}})
            .to_string()
    }

    fn ok(body: String) -> Reply {
        Reply {
            status: 200,
            body,
            delay_ms: 0,
            location: None,
        }
    }

    async fn mock_vault(reply: Reply) -> (String, Hits, Arc<std::sync::Mutex<Reply>>) {
        use axum::{
            body::Body,
            extract::State,
            http::{HeaderMap, Uri},
            response::Response,
            Router,
        };
        let hits: Hits = Arc::default();
        let reply = Arc::new(std::sync::Mutex::new(reply));
        async fn handle(
            State((hits, reply)): State<(Hits, Arc<std::sync::Mutex<Reply>>)>,
            uri: Uri,
            headers: HeaderMap,
        ) -> Response {
            let h = |n: &str| {
                headers
                    .get(n)
                    .and_then(|v| v.to_str().ok())
                    .map(str::to_string)
            };
            hits.lock().unwrap().push((
                uri.path().to_string(),
                h("x-vault-token"),
                h("x-vault-namespace"),
            ));
            let r = reply.lock().unwrap().clone();
            if r.delay_ms > 0 {
                tokio::time::sleep(Duration::from_millis(r.delay_ms)).await;
            }
            let mut b = Response::builder()
                .status(r.status)
                .header("content-type", "application/json");
            if let Some(l) = r.location {
                b = b.header("location", l);
            }
            b.body(Body::from(r.body)).unwrap()
        }
        let app = Router::new()
            .fallback(handle)
            .with_state((hits.clone(), reply.clone()));
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = format!("http://{}", listener.local_addr().unwrap());
        tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        (addr, hits, reply)
    }

    const VAULT_TOKEN: &str = "hvs.mock-token-value";
    const VAULT_SECRET: &str = "ghp_the-real-secret";

    fn vault_source(addr: &str, token_file: &Path, extra: serde_json::Value) -> serde_json::Value {
        let mut source = serde_json::json!({
            "kind": "vault", "addr": addr, "path": "keep/github", "field": "token",
            "auth": {"method": "token", "token_file": token_file},
        });
        for (k, v) in extra.as_object().cloned().unwrap_or_default() {
            source[k] = v;
        }
        source
    }

    fn vault_vault(source: serde_json::Value) -> CredentialVault {
        let d = serde_json::json!({
            "host": "api.example.com", "header": "authorization", "prefix": "Bearer ",
            "source": source,
        });
        let mut map = HashMap::new();
        map.insert("svc".to_string(), serde_json::from_value(d).unwrap());
        CredentialVault::from_descriptors(map)
    }

    /// A vault reading `keep/github` field `token` from `addr`, with a private token file.
    fn vault_at(addr: &str, dir: &Path, extra: serde_json::Value) -> CredentialVault {
        let token = write_secret(dir, "vault-token", VAULT_TOKEN, 0o600);
        vault_vault(vault_source(addr, &token, extra))
    }

    #[tokio::test]
    async fn a_vault_field_is_read_with_the_token_and_namespace_from_the_right_url() {
        let dir = scratch_dir();
        let (addr, hits, _) = mock_vault(ok(kv("token", VAULT_SECRET))).await;
        let v = vault_at(&addr, &dir, serde_json::json!({"namespace": "team-a/keep"}));
        assert_eq!(resolve(&v).await.unwrap(), format!("Bearer {VAULT_SECRET}"));
        let seen = hits.lock().unwrap().clone();
        assert_eq!(
            seen,
            vec![(
                "/v1/secret/data/keep/github".to_string(),
                Some(VAULT_TOKEN.to_string()),
                Some("team-a/keep".to_string())
            )]
        );
        // A custom mount is a path segment, and the address may carry a prefix.
        let (addr, hits, _) = mock_vault(ok(kv("token", VAULT_SECRET))).await;
        let v = vault_at(
            &format!("{addr}/vault/"),
            &dir,
            serde_json::json!({"mount": "kv/v2"}),
        );
        assert!(resolve(&v).await.is_ok());
        assert_eq!(
            hits.lock().unwrap()[0].0,
            "/vault/v1/kv/v2/data/keep/github"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_url_is_built_from_segments_so_nothing_can_change_its_shape() {
        assert_eq!(
            vault_url("https://v.example:8200", "secret", "a/b")
                .unwrap()
                .as_str(),
            "https://v.example:8200/v1/secret/data/a/b"
        );
        assert_eq!(
            vault_url("https://v.example/x/", "kv", "p")
                .unwrap()
                .as_str(),
            "https://v.example/x/v1/kv/data/p"
        );
    }

    #[tokio::test]
    async fn a_renewed_token_file_is_used_on_the_next_fetch_and_a_token_variable_works() {
        let dir = scratch_dir();
        let (addr, hits, _) = mock_vault(ok(kv("token", VAULT_SECRET))).await;
        let v = vault_at(&addr, &dir, serde_json::json!({"ttl_seconds": 1}));
        assert!(resolve(&v).await.is_ok());
        write_secret(&dir, "vault-token", "hvs.renewed", 0o600);
        tokio::time::sleep(Duration::from_millis(1150)).await;
        assert!(resolve(&v).await.is_ok());
        let tokens: Vec<_> = hits
            .lock()
            .unwrap()
            .iter()
            .map(|h| h.1.clone().unwrap())
            .collect();
        assert_eq!(tokens, [VAULT_TOKEN, "hvs.renewed"]);

        let var = format!("ZYVOR_TEST_VT_{}", uuid::Uuid::new_v4().simple());
        std::env::set_var(&var, "hvs.from-env");
        let source = serde_json::json!({
            "kind": "vault", "addr": addr, "path": "keep/github", "field": "token",
            "auth": {"method": "token", "token_env": var},
        });
        assert!(resolve(&vault_vault(source.clone())).await.is_ok());
        assert_eq!(
            hits.lock().unwrap().last().unwrap().1.as_deref(),
            Some("hvs.from-env")
        );
        std::env::remove_var(&var);
        let err = resolve(&vault_vault(source)).await.unwrap_err().to_string();
        assert!(err.contains(&var) && err.contains("not set"), "{err}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn every_kind_of_vault_failure_fails_closed_and_leaks_neither_the_token_nor_the_secret() {
        let dir = scratch_dir();
        let big = serde_json::json!({"data": {"data": {"token": "x".repeat(300_000)}}}).to_string();
        let cases: Vec<(&str, Reply, &str)> = vec![
            (
                "denied",
                Reply {
                    status: 403,
                    body: format!("permission denied {VAULT_TOKEN} {VAULT_SECRET}"),
                    delay_ms: 0,
                    location: None,
                },
                "refused the token",
            ),
            (
                "missing",
                Reply {
                    status: 404,
                    body: "{}".into(),
                    delay_ms: 0,
                    location: None,
                },
                "not found",
            ),
            (
                "server error",
                Reply {
                    status: 500,
                    body: format!("boom {VAULT_SECRET}"),
                    delay_ms: 0,
                    location: None,
                },
                "answered 500",
            ),
            (
                "redirect",
                Reply {
                    status: 302,
                    body: String::new(),
                    delay_ms: 0,
                    location: Some("/elsewhere".into()),
                },
                "redirect",
            ),
            (
                "not json",
                ok(format!("<html>{VAULT_SECRET}</html>")),
                "not JSON",
            ),
            (
                "kv v1 shape",
                ok(serde_json::json!({"data": {"token": VAULT_SECRET}}).to_string()),
                "no text field",
            ),
            (
                "wrong field",
                ok(kv("other", VAULT_SECRET)),
                "no text field",
            ),
            (
                "not a string",
                ok(serde_json::json!({"data": {"data": {"token": 42}}}).to_string()),
                "no text field",
            ),
            ("empty", ok(kv("token", "  ")), "is empty"),
            ("newline", ok(kv("token", "a\nb")), "control character"),
            ("oversized", ok(big), "larger than"),
        ];
        for (label, reply, want) in cases {
            let (addr, hits, _) = mock_vault(reply).await;
            let v = vault_at(&addr, &dir, serde_json::json!({}));
            let err = resolve(&v).await.unwrap_err().to_string();
            assert!(err.contains(want), "{label}: {err}");
            assert!(
                !err.contains(VAULT_TOKEN) && !err.contains(VAULT_SECRET),
                "{label} leaked: {err}"
            );
            // A redirect is not followed: exactly one request reached the mock.
            assert_eq!(hits.lock().unwrap().len(), 1, "{label}");
        }
        // Unreachable, and slow.
        let v = vault_at("http://127.0.0.1:1", &dir, serde_json::json!({}));
        let err = resolve(&v).await.unwrap_err().to_string();
        assert!(err.contains("could not be reached"), "{err}");
        let (addr, _, _) = mock_vault(Reply {
            delay_ms: 3000,
            ..ok(kv("token", VAULT_SECRET))
        })
        .await;
        let v = vault_at(&addr, &dir, serde_json::json!({"timeout_seconds": 1}));
        let started = Instant::now();
        let err = resolve(&v).await.unwrap_err().to_string();
        assert!(err.contains("timed out"), "{err}");
        assert!(
            started.elapsed() < Duration::from_millis(2500),
            "{:?}",
            started.elapsed()
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn vault_values_are_cached_read_once_per_burst_and_never_served_after_a_failure() {
        let dir = scratch_dir();
        let (addr, hits, reply) = mock_vault(ok(kv("token", VAULT_SECRET))).await;
        let v = Arc::new(vault_at(&addr, &dir, serde_json::json!({"ttl_seconds": 1})));
        let mut tasks = Vec::new();
        for _ in 0..20 {
            let v = v.clone();
            tasks.push(tokio::spawn(async move { resolve(&v).await.unwrap() }));
        }
        for t in tasks {
            assert_eq!(t.await.unwrap(), format!("Bearer {VAULT_SECRET}"));
        }
        assert_eq!(
            hits.lock().unwrap().len(),
            1,
            "one read for the whole burst"
        );
        // Within the ttl a revoked secret still works: that is the revocation latency.
        *reply.lock().unwrap() = Reply {
            status: 403,
            body: "{}".into(),
            delay_ms: 0,
            location: None,
        };
        assert!(resolve(&v).await.is_ok());
        assert_eq!(hits.lock().unwrap().len(), 1);
        // After it, the failure refuses the request, and the old value is gone.
        tokio::time::sleep(Duration::from_millis(1150)).await;
        assert!(resolve(&v).await.is_err());
        assert_eq!(hits.lock().unwrap().len(), 2);
        *reply.lock().unwrap() = ok(kv("token", "rotated-secret"));
        assert_eq!(resolve(&v).await.unwrap(), "Bearer rotated-secret");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn a_request_that_fails_the_checks_never_reaches_vault_and_debug_hides_secrets() {
        let dir = scratch_dir();
        let (addr, hits, _) = mock_vault(ok(kv("token", VAULT_SECRET))).await;
        let v = vault_at(&addr, &dir, serde_json::json!({}));
        let post = reqwest::Method::POST;
        let mut map = HashMap::new();
        let mut d = v.descriptor("svc").unwrap().clone();
        d.allowed_methods = vec!["GET".into()];
        map.insert("svc".to_string(), d);
        let v = CredentialVault::from_descriptors(map);
        assert!(v.authorize_resolve("svc", &ctx_for(&post)).await.is_err());
        let wrong_host = ResolveContext {
            host: "evil.example",
            ..ctx_for(&reqwest::Method::GET)
        };
        assert!(v.authorize_resolve("svc", &wrong_host).await.is_err());
        assert!(
            hits.lock().unwrap().is_empty(),
            "a refused request must not reach Vault"
        );
        assert!(resolve(&v).await.is_ok());
        let shown = format!("{v:?}");
        assert!(
            !shown.contains(VAULT_SECRET) && !shown.contains(VAULT_TOKEN),
            "{shown}"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn a_private_ca_is_trusted_only_when_named() {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        let dir = scratch_dir();
        let ca_dir = tempfile::tempdir().unwrap();
        let mitm = crate::mitm::Mitm::load_or_create(ca_dir.path())
            .await
            .unwrap();
        let acceptor = tokio_rustls::TlsAcceptor::from(mitm.server_config("127.0.0.1").unwrap());
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = format!("https://{}", listener.local_addr().unwrap());
        tokio::spawn(async move {
            let _keep = ca_dir;
            loop {
                let Ok((tcp, _)) = listener.accept().await else {
                    return;
                };
                let acceptor = acceptor.clone();
                tokio::spawn(async move {
                    let Ok(mut tls) = acceptor.accept(tcp).await else {
                        return;
                    };
                    let mut buf = [0u8; 4096];
                    let _ = tls.read(&mut buf).await;
                    let body = kv("token", VAULT_SECRET);
                    let reply = format!("HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{body}", body.len());
                    let _ = tls.write_all(reply.as_bytes()).await;
                    let _ = tls.shutdown().await;
                });
            }
        });
        let untrusted = vault_at(&addr, &dir, serde_json::json!({}));
        let err = resolve(&untrusted).await.unwrap_err().to_string();
        assert!(err.contains("could not be reached"), "{err}");
        let ca = write_secret(&dir, "ca.pem", mitm.ca_pem(), 0o644);
        let trusted = vault_at(&addr, &dir, serde_json::json!({"ca_file": ca}));
        assert_eq!(
            resolve(&trusted).await.unwrap(),
            format!("Bearer {VAULT_SECRET}")
        );
        // A CA file that is not a certificate is a clear setup error.
        let junk = write_secret(&dir, "junk.pem", "not a certificate", 0o644);
        let err = resolve(&vault_at(&addr, &dir, serde_json::json!({"ca_file": junk})))
            .await
            .unwrap_err()
            .to_string();
        assert!(err.contains("not a PEM certificate"), "{err}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn loading_reads_vault_once_at_startup_and_a_dead_vault_never_stops_startup() {
        let dir = scratch_dir();
        let (addr, hits, _) = mock_vault(ok(kv("token", VAULT_SECRET))).await;
        let token = write_secret(&dir, "vault-token", VAULT_TOKEN, 0o600);
        let file = dir.join("creds.json");
        let write = |addr: &str| {
            std::fs::write(
                &file,
                serde_json::json!({"svc": {
                    "host": "api.example.com", "header": "authorization",
                    "source": vault_source(addr, &token, serde_json::json!({})),
                }})
                .to_string(),
            )
            .unwrap();
        };
        write(&addr);
        let v = CredentialVault::load(Some(&file)).await.unwrap();
        assert_eq!(hits.lock().unwrap().len(), 1, "prefetched at startup");
        // The first request is served from that read.
        assert!(resolve(&v).await.is_ok());
        assert_eq!(hits.lock().unwrap().len(), 1);
        // A Vault that is down at startup only warns; the request then fails closed.
        write("http://127.0.0.1:1");
        let v = CredentialVault::load(Some(&file))
            .await
            .expect("startup must not fail");
        assert!(resolve(&v).await.is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn vault_source_validation() {
        let dir = scratch_dir();
        let token = write_secret(&dir, "t", "hvs.x", 0o600);
        let file = dir.join("creds.json");
        let load = |source: serde_json::Value| {
            std::fs::write(
                &file,
                serde_json::json!({"svc": {"host": "h.example", "header": "authorization", "source": source}}).to_string(),
            )
            .unwrap();
            let file = file.clone();
            async move {
                // Validation only: a dead address is fine because loading never fails on a fetch.
                CredentialVault::load(Some(&file)).await
            }
        };
        let good =
            |extra: serde_json::Value| vault_source("https://vault.example:8200", &token, extra);
        assert!(load(good(serde_json::json!({}))).await.is_ok());
        assert!(load(vault_source(
            "http://127.0.0.1:8200",
            &token,
            serde_json::json!({})
        ))
        .await
        .is_ok());
        assert!(load(vault_source(
            "http://localhost:8200",
            &token,
            serde_json::json!({})
        ))
        .await
        .is_ok());
        for (label, source) in [
            (
                "http to a remote host",
                vault_source("http://vault.example:8200", &token, serde_json::json!({})),
            ),
            (
                "ftp",
                vault_source("ftp://vault.example", &token, serde_json::json!({})),
            ),
            (
                "not a url",
                vault_source("vault.example", &token, serde_json::json!({})),
            ),
            (
                "userinfo",
                vault_source("https://u:p@vault.example", &token, serde_json::json!({})),
            ),
            (
                "query",
                vault_source("https://vault.example/?x=1", &token, serde_json::json!({})),
            ),
            (
                "dot dot path",
                good(serde_json::json!({"path": "keep/../root"})),
            ),
            (
                "leading slash",
                good(serde_json::json!({"path": "/keep/x"})),
            ),
            (
                "empty segment",
                good(serde_json::json!({"path": "keep//x"})),
            ),
            ("percent", good(serde_json::json!({"path": "keep/%2e%2e"}))),
            ("space", good(serde_json::json!({"path": "keep/a b"}))),
            (
                "question mark",
                good(serde_json::json!({"path": "keep/x?y"})),
            ),
            ("bad mount", good(serde_json::json!({"mount": "../sys"}))),
            ("empty field", good(serde_json::json!({"field": ""}))),
            ("bad field", good(serde_json::json!({"field": "a/b"}))),
            (
                "bad namespace",
                good(serde_json::json!({"namespace": "a/../b"})),
            ),
            ("ttl zero", good(serde_json::json!({"ttl_seconds": 0}))),
            (
                "ttl too long",
                good(serde_json::json!({"ttl_seconds": 3601})),
            ),
            (
                "timeout zero",
                good(serde_json::json!({"timeout_seconds": 0})),
            ),
            (
                "timeout too long",
                good(serde_json::json!({"timeout_seconds": 31})),
            ),
            (
                "relative ca",
                good(serde_json::json!({"ca_file": "ca.pem"})),
            ),
            ("unknown field", good(serde_json::json!({"extra": 1}))),
            (
                "unknown auth",
                good(serde_json::json!({"auth": {"method": "ldap"}})),
            ),
            (
                "both tokens",
                good(
                    serde_json::json!({"auth": {"method": "token", "token_file": token, "token_env": "X"}}),
                ),
            ),
            (
                "no token",
                good(serde_json::json!({"auth": {"method": "token"}})),
            ),
            (
                "relative token file",
                good(serde_json::json!({"auth": {"method": "token", "token_file": "t"}})),
            ),
            (
                "bad token env",
                good(serde_json::json!({"auth": {"method": "token", "token_env": "1 BAD"}})),
            ),
        ] {
            assert!(
                load(source.clone()).await.is_err(),
                "{label} should be refused: {source}"
            );
        }
        // A loose token file stops startup, like a loose secret file.
        let loose = write_secret(&dir, "loose", "hvs.y", 0o644);
        let err = load(vault_source(
            "https://vault.example",
            &loose,
            serde_json::json!({}),
        ))
        .await;
        assert!(
            err.is_ok(),
            "the token file is only checked when it is read, and a failed read only warns"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    // ---- vault login (AppRole, Kubernetes) ------------------------------------------------

    #[derive(Default)]
    struct LoginMock {
        /// (path, JSON body) of each login request.
        logins: std::sync::Mutex<Vec<(String, serde_json::Value)>>,
        /// The X-Vault-Token of each read.
        reads: std::sync::Mutex<Vec<Option<String>>>,
        /// Tokens a read accepts; any other gets 403.
        valid: std::sync::Mutex<std::collections::HashSet<String>>,
        next_token: std::sync::Mutex<String>,
        lease: std::sync::Mutex<u64>,
        login_status: std::sync::Mutex<u16>,
        login_body: std::sync::Mutex<Option<String>>,
    }

    async fn login_mock() -> (String, Arc<LoginMock>) {
        use axum::{
            body::Body,
            extract::State,
            http::{HeaderMap, Method, Uri},
            response::Response,
            Router,
        };
        let mock = Arc::new(LoginMock::default());
        *mock.next_token.lock().unwrap() = "hvs.login-1".into();
        *mock.lease.lock().unwrap() = 3600;
        *mock.login_status.lock().unwrap() = 200;
        mock.valid.lock().unwrap().insert("hvs.login-1".into());
        async fn handle(
            State(m): State<Arc<LoginMock>>,
            method: Method,
            uri: Uri,
            headers: HeaderMap,
            body: axum::body::Bytes,
        ) -> Response {
            if method == Method::POST {
                m.logins.lock().unwrap().push((
                    uri.path().to_string(),
                    serde_json::from_slice(&body).unwrap_or(serde_json::Value::Null),
                ));
                let status = *m.login_status.lock().unwrap();
                let text = match m.login_body.lock().unwrap().clone() {
                    Some(b) => b,
                    None if status == 200 => serde_json::json!({"auth": {
                        "client_token": m.next_token.lock().unwrap().clone(),
                        "lease_duration": *m.lease.lock().unwrap(),
                    }})
                    .to_string(),
                    // A real Vault can echo what it was sent; the runtime must not repeat it.
                    None => format!("denied: {}", String::from_utf8_lossy(&body)),
                };
                return Response::builder()
                    .status(status)
                    .body(Body::from(text))
                    .unwrap();
            }
            let token = headers
                .get("x-vault-token")
                .and_then(|v| v.to_str().ok())
                .map(str::to_string);
            m.reads.lock().unwrap().push(token.clone());
            let allowed = token.is_some_and(|t| m.valid.lock().unwrap().contains(&t));
            Response::builder()
                .status(if allowed { 200 } else { 403 })
                .body(Body::from(if allowed {
                    kv("token", VAULT_SECRET)
                } else {
                    "permission denied".into()
                }))
                .unwrap()
        }
        let app = Router::new().fallback(handle).with_state(mock.clone());
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = format!("http://{}", listener.local_addr().unwrap());
        tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        (addr, mock)
    }

    const SECRET_ID: &str = "the-approle-secret-id";

    fn approle_vault(addr: &str, dir: &Path, extra: serde_json::Value) -> CredentialVault {
        let sid = write_secret(dir, "secret-id", SECRET_ID, 0o600);
        let mut source = serde_json::json!({
            "kind": "vault", "addr": addr, "path": "keep/github", "field": "token",
            "auth": {"method": "approle", "role_id": "role-1", "secret_id_file": sid},
        });
        for (k, v) in extra.as_object().cloned().unwrap_or_default() {
            source[k] = v;
        }
        vault_vault(source)
    }

    #[tokio::test]
    async fn approle_logs_in_once_uses_the_token_for_each_read_and_logs_in_again_when_it_expires() {
        let dir = scratch_dir();
        let (addr, mock) = login_mock().await;
        *mock.lease.lock().unwrap() = 3600;
        let v = approle_vault(&addr, &dir, serde_json::json!({"ttl_seconds": 1}));
        assert_eq!(resolve(&v).await.unwrap(), format!("Bearer {VAULT_SECRET}"));
        {
            let logins = mock.logins.lock().unwrap();
            assert_eq!(logins.len(), 1);
            assert_eq!(logins[0].0, "/v1/auth/approle/login");
            assert_eq!(
                logins[0].1,
                serde_json::json!({"role_id": "role-1", "secret_id": SECRET_ID})
            );
        }
        // The KV cache expires, the login token is still good: read again without logging in.
        tokio::time::sleep(Duration::from_millis(1150)).await;
        assert!(resolve(&v).await.is_ok());
        assert_eq!(mock.logins.lock().unwrap().len(), 1, "the token was reused");
        assert_eq!(mock.reads.lock().unwrap().len(), 2);

        // A token with a short lease is dropped a little early and replaced by a new login.
        let (addr, mock) = login_mock().await;
        *mock.lease.lock().unwrap() = 1;
        let v = approle_vault(&addr, &dir, serde_json::json!({"ttl_seconds": 1}));
        assert!(resolve(&v).await.is_ok());
        tokio::time::sleep(Duration::from_millis(1150)).await;
        assert!(resolve(&v).await.is_ok());
        assert_eq!(
            mock.logins.lock().unwrap().len(),
            2,
            "an expired token means a new login"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn a_revoked_login_token_is_replaced_once_and_a_freshly_issued_one_is_not_retried() {
        let dir = scratch_dir();
        let (addr, mock) = login_mock().await;
        let v = approle_vault(&addr, &dir, serde_json::json!({"ttl_seconds": 1}));
        assert!(resolve(&v).await.is_ok()); // logs in, gets hvs.login-1
                                            // Vault revokes that token; the next login issues a good one.
        mock.valid.lock().unwrap().clear();
        mock.valid.lock().unwrap().insert("hvs.login-2".into());
        *mock.next_token.lock().unwrap() = "hvs.login-2".into();
        tokio::time::sleep(Duration::from_millis(1150)).await;
        assert!(resolve(&v).await.is_ok());
        assert_eq!(mock.logins.lock().unwrap().len(), 2, "one re-login");
        assert_eq!(
            mock.reads.lock().unwrap().clone(),
            [
                Some("hvs.login-1".to_string()),
                Some("hvs.login-1".to_string()),
                Some("hvs.login-2".to_string())
            ]
        );

        // A token that was just issued and is still refused means the policy says no: no more logins.
        let (addr, mock) = login_mock().await;
        mock.valid.lock().unwrap().clear(); // nothing is ever accepted
        let v = approle_vault(&addr, &dir, serde_json::json!({}));
        let err = resolve(&v).await.unwrap_err();
        assert_eq!(failure_class(&err), "refused");
        assert_eq!(mock.logins.lock().unwrap().len(), 1, "no login loop");
        assert_eq!(mock.reads.lock().unwrap().len(), 1);
        // And again a cached-then-refused token gets exactly one more login, then stops.
        let (addr, mock) = login_mock().await;
        let v = approle_vault(&addr, &dir, serde_json::json!({"ttl_seconds": 1}));
        assert!(resolve(&v).await.is_ok());
        mock.valid.lock().unwrap().clear();
        tokio::time::sleep(Duration::from_millis(1150)).await;
        assert!(resolve(&v).await.is_err());
        assert_eq!(
            mock.logins.lock().unwrap().len(),
            2,
            "one re-login, then it stops"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn a_refused_or_malformed_login_fails_closed_without_echoing_the_secret_id() {
        let dir = scratch_dir();
        for (status, body, class) in [
            (400u16, None, "refused"),
            (403, None, "refused"),
            (404, None, "not_found"),
            (500, None, "unreachable"),
            (200, Some(r#"{"auth":{}}"#.to_string()), "malformed"),
            (200, Some("not json".to_string()), "malformed"),
            (
                200,
                Some(r#"{"auth":{"client_token":"a\nb"}}"#.to_string()),
                "malformed",
            ),
        ] {
            let (addr, mock) = login_mock().await;
            *mock.login_status.lock().unwrap() = status;
            *mock.login_body.lock().unwrap() = body;
            let v = approle_vault(&addr, &dir, serde_json::json!({}));
            let err = resolve(&v).await.unwrap_err();
            assert_eq!(failure_class(&err), class, "{status}: {err}");
            let text = err.to_string();
            assert!(
                !text.contains(SECRET_ID) && !text.contains("role-1"),
                "{status} leaked: {text}"
            );
            assert!(
                mock.reads.lock().unwrap().is_empty(),
                "no read without a token"
            );
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn kubernetes_login_sends_the_role_and_the_current_jwt_from_the_mount() {
        let dir = scratch_dir();
        let (addr, mock) = login_mock().await;
        // A projected service account token is world-readable, and that must not matter.
        let jwt = write_secret(&dir, "sa-token", "eyJhbGciOi.the-jwt.sig", 0o644);
        let v = vault_vault(serde_json::json!({
            "kind": "vault", "addr": addr, "path": "keep/github", "field": "token", "ttl_seconds": 1,
            "auth": {"method": "kubernetes", "role": "keep", "jwt_file": jwt, "mount": "k8s/prod"},
        }));
        assert!(resolve(&v).await.is_ok());
        let logins = mock.logins.lock().unwrap().clone();
        assert_eq!(logins[0].0, "/v1/auth/k8s/prod/login");
        assert_eq!(
            logins[0].1,
            serde_json::json!({"role": "keep", "jwt": "eyJhbGciOi.the-jwt.sig"})
        );
        // The default jwt_file and mount are the standard ones.
        let default: VaultAuth =
            serde_json::from_value(serde_json::json!({"method": "kubernetes", "role": "keep"}))
                .unwrap();
        assert_eq!(
            default,
            VaultAuth::Kubernetes {
                role: "keep".into(),
                jwt_file: PathBuf::from("/var/run/secrets/kubernetes.io/serviceaccount/token"),
                mount: "kubernetes".into(),
            }
        );
        // A missing JWT file is a setup error that names the file, not the token.
        let v = vault_vault(serde_json::json!({
            "kind": "vault", "addr": addr, "path": "keep/github", "field": "token",
            "auth": {"method": "kubernetes", "role": "keep", "jwt_file": dir.join("absent")},
        }));
        let err = resolve(&v).await.unwrap_err();
        assert_eq!(failure_class(&err), "config");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn the_status_shows_kind_timing_and_failure_class_and_never_a_value() {
        let dir = scratch_dir();
        let (addr, mock) = login_mock().await;
        let v = approle_vault(&addr, &dir, serde_json::json!({"ttl_seconds": 1}));
        let before = v.source_status();
        assert_eq!(before.len(), 1);
        assert_eq!((before[0].name.as_str(), before[0].kind), ("svc", "vault"));
        assert!(
            !before[0].cached
                && before[0].last_success_seconds_ago.is_none()
                && before[0].last_failure.is_none()
        );
        assert!(resolve(&v).await.is_ok());
        let ok = v.source_status();
        assert!(ok[0].cached);
        assert_eq!(ok[0].last_success_seconds_ago, Some(0));
        assert_eq!(ok[0].last_failure, None);
        // A revoked secret store answer shows as a failure class after the cache expires.
        mock.valid.lock().unwrap().clear();
        *mock.login_status.lock().unwrap() = 403;
        tokio::time::sleep(Duration::from_millis(1150)).await;
        assert!(resolve(&v).await.is_err());
        let failed = v.source_status();
        assert!(!failed[0].cached);
        assert_eq!(failed[0].last_failure, Some("refused"));
        let shown = serde_json::to_string(&failed).unwrap();
        for secret in [
            VAULT_SECRET,
            SECRET_ID,
            "hvs.login-1",
            "role-1",
            "secret-id",
        ] {
            assert!(
                !shown.contains(secret),
                "the status leaked {secret}: {shown}"
            );
        }
        // A file source reports too, and a credential without a source does not appear.
        let f = write_secret(&dir, "k", "v", 0o600);
        let fv = file_vault(&f, serde_json::json!({}));
        assert!(resolve(&fv).await.is_ok());
        assert_eq!(fv.source_status()[0].kind, "file");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn login_auth_validation() {
        let ok = |auth: serde_json::Value| {
            let source = serde_json::json!({
                "kind": "vault", "addr": "https://vault.example", "path": "a/b", "field": "f", "auth": auth,
            });
            let d = serde_json::json!({"host": "h.example", "header": "authorization", "source": source});
            serde_json::from_value::<CredentialDescriptor>(d)
                .map_err(|e| e.to_string())
                .and_then(|d| validate_descriptor("svc", &d).map_err(|e| e.to_string()))
        };
        assert!(ok(
            serde_json::json!({"method": "approle", "role_id": "r", "secret_id_env": "SID"})
        )
        .is_ok());
        assert!(ok(serde_json::json!({"method": "approle", "role_id": "r", "secret_id_file": "/run/sid", "mount": "corp/approle"})).is_ok());
        assert!(ok(serde_json::json!({"method": "kubernetes", "role": "keep"})).is_ok());
        assert!(ok(serde_json::json!({"method": "kubernetes", "role": "keep", "jwt_file": "/x/jwt", "mount": "k8s"})).is_ok());
        for (label, bad) in [
            (
                "no role_id",
                serde_json::json!({"method": "approle", "secret_id_env": "SID"}),
            ),
            (
                "empty role_id",
                serde_json::json!({"method": "approle", "role_id": "", "secret_id_env": "SID"}),
            ),
            (
                "no secret id",
                serde_json::json!({"method": "approle", "role_id": "r"}),
            ),
            (
                "both secret ids",
                serde_json::json!({"method": "approle", "role_id": "r", "secret_id_env": "SID", "secret_id_file": "/x"}),
            ),
            (
                "relative secret id file",
                serde_json::json!({"method": "approle", "role_id": "r", "secret_id_file": "sid"}),
            ),
            (
                "bad env name",
                serde_json::json!({"method": "approle", "role_id": "r", "secret_id_env": "1 X"}),
            ),
            (
                "bad approle mount",
                serde_json::json!({"method": "approle", "role_id": "r", "secret_id_env": "S", "mount": "../x"}),
            ),
            ("no k8s role", serde_json::json!({"method": "kubernetes"})),
            (
                "bad k8s role",
                serde_json::json!({"method": "kubernetes", "role": "a b"}),
            ),
            (
                "relative jwt",
                serde_json::json!({"method": "kubernetes", "role": "keep", "jwt_file": "jwt"}),
            ),
            (
                "jwt with ..",
                serde_json::json!({"method": "kubernetes", "role": "keep", "jwt_file": "/a/../b"}),
            ),
            (
                "bad k8s mount",
                serde_json::json!({"method": "kubernetes", "role": "keep", "mount": "a b"}),
            ),
            (
                "unknown field",
                serde_json::json!({"method": "kubernetes", "role": "keep", "extra": 1}),
            ),
            ("unknown method", serde_json::json!({"method": "ldap"})),
        ] {
            assert!(ok(bad.clone()).is_err(), "{label} should be refused: {bad}");
        }
    }

    #[tokio::test]
    async fn the_vault_status_route_lists_sources_for_the_operator_only() {
        use axum::{body::Body, http::Request};
        use tower::ServiceExt;
        let dir = scratch_dir();
        let secret = write_secret(&dir, "key", "route-secret-value", 0o600);
        let creds = dir.join("creds.json");
        std::fs::write(
            &creds,
            serde_json::json!({
                "svc": {"host": "h.example", "header": "authorization",
                        "source": {"kind": "file", "path": secret}},
                "plain": {"host": "p.example", "header": "authorization", "env": "SOME_ENV"},
            })
            .to_string(),
        )
        .unwrap();
        let token = crate::fixture::text("operator-token");
        let (state, _) = crate::egress::ask_tests::state_and_session_cfg(|c| {
            c.api_token = Some(token.clone());
            c.credentials_file = Some(creds.clone());
        })
        .await;
        let app = crate::app::public_router(state.clone());
        let call = |tok: String| {
            let app = app.clone();
            async move {
                let req = Request::builder()
                    .uri("/v1/vault/status")
                    .header("authorization", format!("Bearer {tok}"))
                    .body(Body::empty())
                    .unwrap();
                let resp = app.oneshot(req).await.unwrap();
                let st = resp.status();
                let bytes = axum::body::to_bytes(resp.into_body(), 1 << 20)
                    .await
                    .unwrap();
                (st, String::from_utf8_lossy(&bytes).to_string())
            }
        };
        let (st, body) = call(token.clone()).await;
        assert_eq!(st, 200, "{body}");
        let v: serde_json::Value = serde_json::from_str(&body).unwrap();
        let sources = v["sources"].as_array().unwrap();
        assert_eq!(
            sources.len(),
            1,
            "only credentials with a source are listed"
        );
        assert_eq!(sources[0]["name"], "svc");
        assert_eq!(sources[0]["kind"], "file");
        assert!(!body.contains("route-secret-value"), "leaked: {body}");
        assert!(v["sources_honesty"]
            .as_str()
            .unwrap()
            .contains("host operator can read it"));
        assert!(
            !body.contains(dir.to_string_lossy().as_ref()),
            "the status must not show paths: {body}"
        );

        // A user token cannot read it.
        let (st, mint) = {
            let req = Request::builder()
                .method("POST")
                .uri("/v1/user-tokens")
                .header("authorization", format!("Bearer {token}"))
                .header("content-type", "application/json")
                .body(Body::from(
                    serde_json::json!({"user_id": "ana", "ttl_seconds": 600}).to_string(),
                ))
                .unwrap();
            let resp = app.clone().oneshot(req).await.unwrap();
            let st = resp.status();
            let bytes = axum::body::to_bytes(resp.into_body(), 1 << 20)
                .await
                .unwrap();
            (
                st,
                serde_json::from_slice::<serde_json::Value>(&bytes).unwrap(),
            )
        };
        assert_eq!(st, 201, "{mint}");
        let (st, _) = call(mint["token"].as_str().unwrap().to_string()).await;
        assert_eq!(st, 403);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
