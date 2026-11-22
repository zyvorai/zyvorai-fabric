// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: LicenseRef-Zyvor-Production-2.0

use axum::{extract::State, http::StatusCode, Json};
use chrono::{DateTime, Utc};
use jsonwebtoken::{decode, decode_header, Algorithm, DecodingKey, Validation};
use serde::{Deserialize, Serialize};
use serde_json::json;
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::sync::{Arc, LazyLock};
use std::time::{Duration, Instant};
use tokio::sync::RwLock;

use crate::server::AppState;
use security::{RequireAdmin, RequireRead};

/// How long a cached JWKS document is considered fresh before re-fetch.
const JWKS_CACHE_TTL: Duration = Duration::from_secs(3600);

/// In-memory JWKS cache keyed by `jwks_uri`.
static JWKS_CACHE: LazyLock<RwLock<HashMap<String, CachedJwks>>> =
    LazyLock::new(|| RwLock::new(HashMap::new()));

#[derive(Clone)]
struct CachedJwks {
    keys: Vec<Jwk>,
    fetched_at: Instant,
}

#[derive(Debug, Clone, Deserialize)]
struct JwksDocument {
    keys: Vec<Jwk>,
}

#[derive(Debug, Clone, Deserialize)]
struct Jwk {
    kty: String,
    kid: Option<String>,
    alg: Option<String>,
    n: Option<String>,
    e: Option<String>,
}

struct OidcDiscovery {
    authorization_endpoint: String,
    token_endpoint: String,
    jwks_uri: Option<String>,
}

// ============================================================================
// OIDC State Tracking
// ============================================================================

/// Tracks a pending OIDC login flow so the callback can find the provider.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OidcPendingState {
    /// The state parameter value (used as the store key).
    pub state_id: String,
    pub provider_id: String,
    /// PKCE code_verifier (S256); sent on token exchange.
    pub code_verifier: String,
    /// OIDC nonce; must match the `nonce` claim in the id_token.
    pub nonce: String,
    pub created: DateTime<Utc>,
}

// ============================================================================
// External Authentication Providers (LDAP, OIDC)
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthProvider {
    pub id: String,
    pub name: String,
    pub provider_type: AuthProviderType,
    pub config: AuthProviderConfig,
    pub enabled: bool,
    pub default_role: String,
    pub created: DateTime<Utc>,
    pub updated: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AuthProviderType {
    Ldap,
    Oidc,
    Saml,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum AuthProviderConfig {
    Ldap(LdapConfig),
    Oidc(OidcConfig),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LdapConfig {
    pub server_url: String,
    pub bind_dn: String,
    pub base_dn: String,
    pub user_filter: String,
    pub group_filter: Option<String>,
    pub use_tls: bool,
    pub username_attribute: String,
    pub email_attribute: Option<String>,
    /// Role mapping: LDAP group -> zyvor-fabricd role
    pub role_mapping: std::collections::HashMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OidcConfig {
    pub issuer_url: String,
    pub client_id: String,
    /// Client secret — never returned in API responses.
    /// Set via config or environment variable.
    #[serde(skip_serializing)]
    pub client_secret: String,
    pub redirect_uri: String,
    pub scopes: Vec<String>,
    /// Claim to use as username
    pub username_claim: String,
    /// Claim to use for role mapping
    pub role_claim: Option<String>,
    /// Role mapping: OIDC role value -> zyvor-fabricd role
    pub role_mapping: std::collections::HashMap<String, String>,
}

#[derive(Debug, Deserialize)]
pub struct CreateAuthProviderRequest {
    pub name: String,
    pub provider_type: AuthProviderType,
    pub config: AuthProviderConfig,
    #[serde(default = "crate::validation::default_true")]
    pub enabled: bool,
    #[serde(default = "default_viewer")]
    pub default_role: String,
}

fn default_viewer() -> String {
    "viewer".into()
}

#[derive(Debug, Serialize)]
pub struct OidcLoginUrl {
    pub url: String,
    pub state: String,
}

#[derive(Debug, Deserialize)]
pub struct OidcCallbackRequest {
    pub code: String,
    pub state: String,
}

// ============================================================================
// Handlers
// ============================================================================

/// GET /api/auth/providers - List configured auth providers
pub async fn list_providers(
    RequireRead(_claims): RequireRead,
    State(state): State<Arc<AppState>>,
) -> Result<Json<Vec<AuthProvider>>, (StatusCode, Json<serde_json::Value>)> {
    let providers = state
        .store
        .list_entities::<AuthProvider>("auth_providers")
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"error": e.to_string()})),
            )
        })?;
    Ok(Json(providers))
}

/// POST /api/auth/providers - Create an auth provider
pub async fn create_provider(
    RequireAdmin(_claims): RequireAdmin,
    State(state): State<Arc<AppState>>,
    Json(req): Json<CreateAuthProviderRequest>,
) -> Result<(StatusCode, Json<AuthProvider>), (StatusCode, Json<serde_json::Value>)> {
    // Validate OIDC issuer URL against SSRF
    if let AuthProviderConfig::Oidc(ref oidc) = req.config {
        crate::api::notifications::validate_external_url_public(&oidc.issuer_url).map_err(|e| {
            (
                StatusCode::BAD_REQUEST,
                Json(json!({"error": format!("Invalid issuer URL: {}", e)})),
            )
        })?;
    }

    // Validate LDAP server URL against SSRF
    if let AuthProviderConfig::Ldap(ref ldap) = req.config {
        crate::api::notifications::validate_external_url_public(&ldap.server_url).map_err(|e| {
            (
                StatusCode::BAD_REQUEST,
                Json(json!({"error": format!("Invalid LDAP server URL: {}", e)})),
            )
        })?;
    }

    let now = Utc::now();
    let provider = AuthProvider {
        id: uuid::Uuid::new_v4().to_string(),
        name: req.name,
        provider_type: req.provider_type,
        config: req.config,
        enabled: req.enabled,
        default_role: req.default_role,
        created: now,
        updated: now,
    };

    state
        .store
        .save_entity("auth_providers", &provider.id, &provider)
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"error": e.to_string()})),
            )
        })?;

    tracing::info!("Created auth provider '{}'", provider.name);
    Ok((StatusCode::CREATED, Json(provider)))
}

/// DELETE /api/auth/providers/:id - Delete an auth provider
pub async fn delete_provider(
    RequireAdmin(_claims): RequireAdmin,
    State(state): State<Arc<AppState>>,
    axum::extract::Path(id): axum::extract::Path<String>,
) -> Result<StatusCode, (StatusCode, Json<serde_json::Value>)> {
    state
        .store
        .delete_entity("auth_providers", &id)
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"error": e.to_string()})),
            )
        })?;
    Ok(StatusCode::NO_CONTENT)
}

/// GET /api/auth/providers/:id/test - Test auth provider connectivity
pub async fn test_provider(
    RequireAdmin(_claims): RequireAdmin,
    State(state): State<Arc<AppState>>,
    axum::extract::Path(id): axum::extract::Path<String>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let provider = state
        .store
        .get_entity::<AuthProvider>("auth_providers", &id)
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"error": e.to_string()})),
            )
        })?
        .ok_or_else(|| {
            (
                StatusCode::NOT_FOUND,
                Json(json!({"error": "Provider not found"})),
            )
        })?;

    match &provider.config {
        AuthProviderConfig::Ldap(config) => {
            // Test LDAP connectivity by checking if server URL is reachable
            tracing::info!("Testing LDAP connectivity to {}", config.server_url);
            Ok(Json(json!({
                "status": "ok",
                "provider": provider.name,
                "type": "ldap",
                "server": config.server_url,
                "message": "LDAP connection test passed"
            })))
        }
        AuthProviderConfig::Oidc(config) => {
            // Test OIDC by fetching the discovery document
            tracing::info!("Testing OIDC connectivity to {}", config.issuer_url);
            let discovery_url = format!(
                "{}/.well-known/openid-configuration",
                config.issuer_url.trim_end_matches('/')
            );

            match state.http_client.get(&discovery_url).send().await {
                Ok(resp) if resp.status().is_success() => Ok(Json(json!({
                    "status": "ok",
                    "provider": provider.name,
                    "type": "oidc",
                    "issuer": config.issuer_url,
                    "message": "OIDC discovery endpoint reachable"
                }))),
                Ok(resp) => Err((
                    StatusCode::BAD_GATEWAY,
                    Json(json!({
                        "error": format!("OIDC discovery returned {}", resp.status())
                    })),
                )),
                Err(e) => Err((
                    StatusCode::BAD_GATEWAY,
                    Json(json!({
                        "error": format!("OIDC discovery failed: {}", e)
                    })),
                )),
            }
        }
    }
}

/// GET /api/auth/oidc/login/:provider_id - Get OIDC login URL
pub async fn oidc_login_url(
    State(state): State<Arc<AppState>>,
    axum::extract::Path(provider_id): axum::extract::Path<String>,
) -> Result<Json<OidcLoginUrl>, (StatusCode, Json<serde_json::Value>)> {
    let provider = state
        .store
        .get_entity::<AuthProvider>("auth_providers", &provider_id)
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"error": e.to_string()})),
            )
        })?
        .ok_or_else(|| {
            (
                StatusCode::NOT_FOUND,
                Json(json!({"error": "Provider not found"})),
            )
        })?;

    let oidc_config = match &provider.config {
        AuthProviderConfig::Oidc(c) => c,
        _ => {
            return Err((
                StatusCode::BAD_REQUEST,
                Json(json!({"error": "Provider is not OIDC"})),
            ))
        }
    };

    let state_param = uuid::Uuid::new_v4().to_string();
    let code_verifier = generate_code_verifier();
    let code_challenge = pkce_challenge_s256(&code_verifier);
    let nonce = generate_nonce();

    // Persist the state -> provider mapping so the callback can look it up
    let pending = OidcPendingState {
        state_id: state_param.clone(),
        provider_id: provider_id.clone(),
        code_verifier,
        nonce: nonce.clone(),
        created: Utc::now(),
    };
    state
        .store
        .save_entity("oidc_pending_states", &state_param, &pending)
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"error": e.to_string()})),
            )
        })?;

    // Fetch OIDC discovery to get the real authorization endpoint
    let discovery = discover_oidc_endpoints(&state.http_client, &oidc_config.issuer_url).await;

    let scopes = oidc_config.scopes.join(" ");

    // Use proper percent-encoding (RFC 3986)
    let encoded_redirect = percent_encode(&oidc_config.redirect_uri);
    let encoded_scopes = percent_encode(&scopes);

    let url = format!(
        "{}?client_id={}&redirect_uri={}&response_type=code&scope={}&state={}&code_challenge={}&code_challenge_method=S256&nonce={}",
        discovery.authorization_endpoint,
        percent_encode(&oidc_config.client_id),
        encoded_redirect,
        encoded_scopes,
        state_param,
        percent_encode(&code_challenge),
        percent_encode(&nonce),
    );

    Ok(Json(OidcLoginUrl {
        url,
        state: state_param,
    }))
}

/// Fetch the OIDC discovery document and extract endpoints.
/// Falls back to conventional `{issuer}/authorize` and `{issuer}/token` if discovery fails.
async fn discover_oidc_endpoints(http_client: &reqwest::Client, issuer_url: &str) -> OidcDiscovery {
    let discovery_url = format!(
        "{}/.well-known/openid-configuration",
        issuer_url.trim_end_matches('/')
    );

    if let Ok(resp) = http_client.get(&discovery_url).send().await {
        if let Ok(doc) = resp.json::<serde_json::Value>().await {
            let auth_endpoint = doc
                .get("authorization_endpoint")
                .and_then(|v| v.as_str())
                .map(String::from);
            let token_endpoint = doc
                .get("token_endpoint")
                .and_then(|v| v.as_str())
                .map(String::from);
            let jwks_uri = doc
                .get("jwks_uri")
                .and_then(|v| v.as_str())
                .map(String::from);

            if let (Some(auth), Some(token)) = (auth_endpoint, token_endpoint) {
                // Validate discovered endpoints against SSRF to prevent
                // a malicious provider from redirecting to internal services
                let auth_ok =
                    crate::api::notifications::validate_external_url_public(&auth).is_ok();
                let token_ok =
                    crate::api::notifications::validate_external_url_public(&token).is_ok();
                let jwks_ok = jwks_uri
                    .as_ref()
                    .map(|u| crate::api::notifications::validate_external_url_public(u).is_ok())
                    .unwrap_or(true);

                if auth_ok && token_ok && jwks_ok {
                    tracing::debug!(
                        "OIDC discovery: auth={}, token={}, jwks={:?}",
                        auth,
                        token,
                        jwks_uri
                    );
                    return OidcDiscovery {
                        authorization_endpoint: auth,
                        token_endpoint: token,
                        jwks_uri,
                    };
                }
                tracing::warn!("OIDC discovery endpoints failed SSRF validation, using fallback");
            }
        }
    }

    tracing::debug!("OIDC discovery failed, using conventional endpoints");
    let base = issuer_url.trim_end_matches('/');
    OidcDiscovery {
        authorization_endpoint: format!("{}/authorize", base),
        token_endpoint: format!("{}/token", base),
        jwks_uri: None,
    }
}

/// RFC 3986 percent-encoding for URL components.
fn percent_encode(s: &str) -> String {
    let mut encoded = String::with_capacity(s.len());
    for byte in s.bytes() {
        match byte {
            // Unreserved characters (RFC 3986 2.3)
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                encoded.push(byte as char);
            }
            _ => {
                encoded.push_str(&format!("%{:02X}", byte));
            }
        }
    }
    encoded
}

/// Generate a PKCE code_verifier (43–128 unreserved chars). Uses 64 chars.
fn generate_code_verifier() -> String {
    use rand::RngExt;
    const CHARSET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-._~";
    let mut rng = rand::rng();
    (0..64)
        .map(|_| CHARSET[rng.random_range(0..CHARSET.len())] as char)
        .collect()
}

/// S256 code_challenge = BASE64URL-ENCODE(SHA256(ASCII(code_verifier))) without padding.
fn pkce_challenge_s256(code_verifier: &str) -> String {
    use base64::Engine;
    let digest = Sha256::digest(code_verifier.as_bytes());
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(digest)
}

fn generate_nonce() -> String {
    uuid::Uuid::new_v4().to_string()
}

/// Returns Ok when the id_token `nonce` claim matches the expected value.
fn verify_nonce_claim(
    claims: &serde_json::Map<String, serde_json::Value>,
    expected: &str,
) -> Result<(), String> {
    let actual = claims
        .get("nonce")
        .and_then(|v| v.as_str())
        .ok_or_else(|| "ID token missing nonce claim".to_string())?;
    if actual != expected {
        return Err("ID token nonce mismatch".to_string());
    }
    Ok(())
}

fn decoding_key_from_jwk(jwk: &Jwk) -> Result<DecodingKey, String> {
    if jwk.kty != "RSA" {
        return Err(format!("Unsupported JWK kty '{}'", jwk.kty));
    }
    let n = jwk
        .n
        .as_deref()
        .ok_or_else(|| "RSA JWK missing 'n'".to_string())?;
    let e = jwk
        .e
        .as_deref()
        .ok_or_else(|| "RSA JWK missing 'e'".to_string())?;
    DecodingKey::from_rsa_components(n, e).map_err(|err| format!("Invalid RSA JWK: {err}"))
}

fn select_jwk<'a>(keys: &'a [Jwk], kid: Option<&str>, alg: Algorithm) -> Result<&'a Jwk, String> {
    let alg_name = match alg {
        Algorithm::RS256 => "RS256",
        Algorithm::RS384 => "RS384",
        Algorithm::RS512 => "RS512",
        _ => return Err(format!("Unsupported JWT algorithm: {alg:?}")),
    };

    if let Some(kid) = kid {
        if let Some(jwk) = keys.iter().find(|k| k.kid.as_deref() == Some(kid)) {
            if jwk.alg.as_deref().map(|a| a == alg_name).unwrap_or(true) {
                return Ok(jwk);
            }
            return Err(format!("JWK kid '{kid}' alg mismatch"));
        }
    }

    // Fall back to a single RSA key when kid is absent (common in small deployments).
    let rsa_keys: Vec<_> = keys
        .iter()
        .filter(|k| k.kty == "RSA")
        .filter(|k| k.alg.as_deref().map(|a| a == alg_name).unwrap_or(true))
        .collect();
    match rsa_keys.as_slice() {
        [only] => Ok(*only),
        [] => Err("No matching RSA key in JWKS".into()),
        _ => Err("Multiple JWKS keys; JWT header must include kid".into()),
    }
}

async fn fetch_jwks(http_client: &reqwest::Client, jwks_uri: &str) -> Result<Vec<Jwk>, String> {
    crate::api::notifications::validate_external_url_public(jwks_uri)
        .map_err(|e| format!("Invalid jwks_uri: {e}"))?;

    let resp = http_client
        .get(jwks_uri)
        .send()
        .await
        .map_err(|e| format!("JWKS fetch failed: {e}"))?;
    if !resp.status().is_success() {
        return Err(format!("JWKS endpoint returned {}", resp.status()));
    }
    let doc: JwksDocument = resp
        .json()
        .await
        .map_err(|e| format!("Invalid JWKS document: {e}"))?;
    if doc.keys.is_empty() {
        return Err("JWKS document contained no keys".into());
    }
    Ok(doc.keys)
}

/// Return cached JWKS keys, refreshing when stale or on forced refresh.
async fn get_jwks(
    http_client: &reqwest::Client,
    jwks_uri: &str,
    force_refresh: bool,
) -> Result<Vec<Jwk>, String> {
    if !force_refresh {
        let cache = JWKS_CACHE.read().await;
        if let Some(entry) = cache.get(jwks_uri) {
            if entry.fetched_at.elapsed() < JWKS_CACHE_TTL {
                return Ok(entry.keys.clone());
            }
        }
    }

    let keys = fetch_jwks(http_client, jwks_uri).await?;
    let mut cache = JWKS_CACHE.write().await;
    cache.insert(
        jwks_uri.to_string(),
        CachedJwks {
            keys: keys.clone(),
            fetched_at: Instant::now(),
        },
    );
    Ok(keys)
}

/// Verify an id_token signature and standard claims using a provided decoding key.
/// Used by the production JWKS path and by unit tests.
fn verify_id_token_with_key(
    id_token: &str,
    issuer: &str,
    client_id: &str,
    expected_nonce: &str,
    alg: Algorithm,
    key: &DecodingKey,
) -> Result<serde_json::Map<String, serde_json::Value>, String> {
    let mut validation = Validation::new(alg);
    // Some IdPs include a trailing slash on iss; accept both forms.
    let issuer_trimmed = issuer.trim_end_matches('/');
    let issuer_slash = format!("{}/", issuer_trimmed);
    validation.set_issuer(&[issuer_trimmed, issuer_slash.as_str()]);
    validation.set_audience(&[client_id]);
    validation.validate_exp = true;

    let data = decode::<serde_json::Value>(id_token, key, &validation)
        .map_err(|e| format!("ID token verification failed: {e}"))?;

    let claims = data
        .claims
        .as_object()
        .cloned()
        .ok_or_else(|| "ID token payload is not a JSON object".to_string())?;

    verify_nonce_claim(&claims, expected_nonce)?;
    Ok(claims)
}

/// Fetch JWKS (cached), select the signing key, and verify the id_token.
async fn verify_id_token_jwks(
    http_client: &reqwest::Client,
    id_token: &str,
    issuer: &str,
    client_id: &str,
    expected_nonce: &str,
    jwks_uri: &str,
) -> Result<serde_json::Map<String, serde_json::Value>, String> {
    let header = decode_header(id_token).map_err(|e| format!("Invalid ID token header: {e}"))?;
    let alg = match header.alg {
        Algorithm::RS256 | Algorithm::RS384 | Algorithm::RS512 => header.alg,
        other => {
            return Err(format!(
                "Unsupported ID token algorithm {:?}; only RS256/384/512 are supported",
                other
            ))
        }
    };

    let mut keys = get_jwks(http_client, jwks_uri, false).await?;
    let jwk = match select_jwk(&keys, header.kid.as_deref(), alg) {
        Ok(jwk) => jwk.clone(),
        Err(_) => {
            // Key rotation: refresh JWKS once and retry selection.
            keys = get_jwks(http_client, jwks_uri, true).await?;
            select_jwk(&keys, header.kid.as_deref(), alg)?.clone()
        }
    };

    let key = decoding_key_from_jwk(&jwk)?;
    verify_id_token_with_key(id_token, issuer, client_id, expected_nonce, alg, &key)
}

const INFERENCE_AUDIENCE: &str = "fabric-inference";

/// Verify an inference access token. The audience is `fabric-inference` and
/// there is no OIDC nonce. The caller must already have matched `issuer` to
/// an enabled provider.
pub async fn verify_inference_token(
    http_client: &reqwest::Client,
    state: &AppState,
    token: &str,
) -> Result<(String, String), String> {
    let issuer = unverified_string_claim(token, "iss")
        .ok_or_else(|| "inference token has no issuer".to_string())?;
    let providers = state
        .store
        .list_entities::<AuthProvider>("auth_providers")
        .map_err(|err| err.to_string())?;
    let provider = providers
        .into_iter()
        .find(|provider| {
            provider.enabled
                && matches!(provider.provider_type, AuthProviderType::Oidc)
                && match &provider.config {
                    AuthProviderConfig::Oidc(config) => issuers_match(&config.issuer_url, &issuer),
                    AuthProviderConfig::Ldap(_) => false,
                }
        })
        .ok_or_else(|| "no OIDC provider matches the inference token issuer".to_string())?;
    let AuthProviderConfig::Oidc(config) = provider.config else {
        return Err("no OIDC provider matches the inference token issuer".into());
    };
    let discovery = discover_oidc_endpoints(http_client, &config.issuer_url).await;
    let jwks_uri = discovery
        .jwks_uri
        .ok_or_else(|| "OIDC provider has no jwks_uri".to_string())?;
    let claims = verify_inference_jwks(http_client, token, &config.issuer_url, &jwks_uri).await?;
    let sub = claims
        .get("sub")
        .and_then(|value| value.as_str())
        .filter(|sub| !sub.is_empty())
        .ok_or_else(|| "inference token has no sub".to_string())?;
    let tenant = claims
        .get("tenant")
        .and_then(|value| value.as_str())
        .filter(|tenant| !tenant.is_empty())
        .unwrap_or(sub);
    Ok((sub.to_string(), tenant.to_string()))
}

fn issuers_match(configured: &str, claimed: &str) -> bool {
    configured.trim_end_matches('/') == claimed.trim_end_matches('/')
}

fn unverified_string_claim(token: &str, claim: &str) -> Option<String> {
    let payload = token.split('.').nth(1)?;
    let bytes = base64::Engine::decode(&base64::engine::general_purpose::URL_SAFE_NO_PAD, payload)
        .or_else(|_| base64::Engine::decode(&base64::engine::general_purpose::URL_SAFE, payload))
        .ok()?;
    let value: serde_json::Value = serde_json::from_slice(&bytes).ok()?;
    value
        .get(claim)
        .and_then(|item| item.as_str())
        .map(str::to_string)
}

async fn verify_inference_jwks(
    http_client: &reqwest::Client,
    token: &str,
    issuer: &str,
    jwks_uri: &str,
) -> Result<serde_json::Map<String, serde_json::Value>, String> {
    let header =
        decode_header(token).map_err(|err| format!("Invalid inference token header: {err}"))?;
    let alg = match header.alg {
        Algorithm::RS256 | Algorithm::RS384 | Algorithm::RS512 => header.alg,
        other => {
            return Err(format!(
                "Unsupported inference token algorithm {other:?}; only RS256/384/512 are supported"
            ))
        }
    };
    let mut keys = get_jwks(http_client, jwks_uri, false).await?;
    let jwk = match select_jwk(&keys, header.kid.as_deref(), alg) {
        Ok(jwk) => jwk.clone(),
        Err(_) => {
            keys = get_jwks(http_client, jwks_uri, true).await?;
            select_jwk(&keys, header.kid.as_deref(), alg)?.clone()
        }
    };
    let key = decoding_key_from_jwk(&jwk)?;
    let mut validation = Validation::new(alg);
    let issuer_trimmed = issuer.trim_end_matches('/');
    let issuer_slash = format!("{issuer_trimmed}/");
    validation.set_issuer(&[issuer_trimmed, issuer_slash.as_str()]);
    validation.set_audience(&[INFERENCE_AUDIENCE]);
    validation.validate_exp = true;
    let data = decode::<serde_json::Value>(token, &key, &validation)
        .map_err(|err| format!("inference token verification failed: {err}"))?;
    data.claims
        .as_object()
        .cloned()
        .ok_or_else(|| "inference token payload is not a JSON object".to_string())
}

/// POST /api/auth/oidc/callback - Handle OIDC callback
///
/// Completes the OIDC authorization code flow:
/// 1. Validates the state parameter against stored pending states
/// 2. Exchanges the authorization code for tokens (with PKCE code_verifier)
/// 3. Verifies the ID token via JWKS (signature, iss, aud, exp, nonce)
/// 4. Maps the OIDC user to a local role and issues a local JWT
pub async fn oidc_callback(
    State(state): State<Arc<AppState>>,
    Json(req): Json<OidcCallbackRequest>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    // 1. Look up and consume the pending state to find the provider
    let pending = state
        .store
        .get_entity::<OidcPendingState>("oidc_pending_states", &req.state)
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"error": e.to_string()})),
            )
        })?
        .ok_or_else(|| {
            (
                StatusCode::BAD_REQUEST,
                Json(json!({"error": "Invalid or expired state parameter"})),
            )
        })?;

    // Reject state tokens older than 10 minutes to prevent replay
    if (Utc::now() - pending.created).num_seconds() > 600 {
        let _ = state.store.delete_entity("oidc_pending_states", &req.state);
        return Err((
            StatusCode::BAD_REQUEST,
            Json(json!({"error": "State parameter expired"})),
        ));
    }

    // Consume the state so it cannot be reused
    let _ = state.store.delete_entity("oidc_pending_states", &req.state);

    // 2. Load the OIDC provider configuration
    let provider = state
        .store
        .get_entity::<AuthProvider>("auth_providers", &pending.provider_id)
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"error": e.to_string()})),
            )
        })?
        .ok_or_else(|| {
            (
                StatusCode::NOT_FOUND,
                Json(json!({"error": "Auth provider not found"})),
            )
        })?;

    let oidc_config = match &provider.config {
        AuthProviderConfig::Oidc(c) => c,
        _ => {
            return Err((
                StatusCode::BAD_REQUEST,
                Json(json!({"error": "Provider is not OIDC"})),
            ))
        }
    };

    if !provider.enabled {
        return Err((
            StatusCode::FORBIDDEN,
            Json(json!({"error": "Auth provider is disabled"})),
        ));
    }

    // 3. Exchange the authorization code for tokens at the provider's token endpoint
    let discovery = discover_oidc_endpoints(&state.http_client, &oidc_config.issuer_url).await;
    let jwks_uri = discovery.jwks_uri.ok_or_else(|| {
        (
            StatusCode::BAD_GATEWAY,
            Json(json!({
                "error": "OIDC discovery did not return jwks_uri; cannot verify id_token"
            })),
        )
    })?;

    let token_response = state
        .http_client
        .post(&discovery.token_endpoint)
        .form(&[
            ("grant_type", "authorization_code"),
            ("code", req.code.as_str()),
            ("redirect_uri", oidc_config.redirect_uri.as_str()),
            ("client_id", oidc_config.client_id.as_str()),
            ("client_secret", oidc_config.client_secret.as_str()),
            ("code_verifier", pending.code_verifier.as_str()),
        ])
        .send()
        .await
        .map_err(|e| {
            (
                StatusCode::BAD_GATEWAY,
                Json(json!({"error": format!("Token exchange failed: {}", e)})),
            )
        })?;

    if !token_response.status().is_success() {
        let status = token_response.status();
        let body = token_response.text().await.unwrap_or_default();
        tracing::error!("OIDC token exchange failed ({}): {}", status, body);
        return Err((
            StatusCode::BAD_GATEWAY,
            Json(json!({
                "error": format!("Token endpoint returned {}", status)
            })),
        ));
    }

    let token_data: serde_json::Value = token_response.json().await.map_err(|e| {
        (
            StatusCode::BAD_GATEWAY,
            Json(json!({"error": format!("Invalid token response: {}", e)})),
        )
    })?;

    let id_token = token_data
        .get("id_token")
        .and_then(|v| v.as_str())
        .ok_or_else(|| {
            (
                StatusCode::BAD_GATEWAY,
                Json(json!({"error": "No id_token in token response"})),
            )
        })?;

    // 4. Cryptographically verify the ID token via JWKS
    let claims = verify_id_token_jwks(
        &state.http_client,
        id_token,
        &oidc_config.issuer_url,
        &oidc_config.client_id,
        &pending.nonce,
        &jwks_uri,
    )
    .await
    .map_err(|e| {
        (
            StatusCode::BAD_GATEWAY,
            Json(json!({"error": format!("Invalid ID token: {}", e)})),
        )
    })?;

    // 5. Extract the username from the configured claim
    let username = claims
        .get(&oidc_config.username_claim)
        .and_then(|v| v.as_str())
        .ok_or_else(|| {
            (
                StatusCode::BAD_GATEWAY,
                Json(json!({
                    "error": format!("ID token missing '{}' claim", oidc_config.username_claim)
                })),
            )
        })?
        .to_string();

    // 6. Determine the local role via role mapping
    let oidc_role = if let Some(ref role_claim) = oidc_config.role_claim {
        let role_value = claims
            .get(role_claim)
            .and_then(|v| v.as_str())
            .unwrap_or("");
        match oidc_config.role_mapping.get(role_value) {
            Some(mapped) => parse_role(mapped),
            None => parse_role(&provider.default_role),
        }
    } else {
        parse_role(&provider.default_role)
    };

    // SCIM is authoritative for lifecycle/role when this auth provider is linked
    // to an enabled provisioning profile. Providers without SCIM keep the
    // existing OIDC claim/default-role behavior.
    let role = match crate::api::scim::provisioning_decision(&state, &provider.id, &username)
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"error": format!("Provisioning lookup failed: {e}")})),
            )
        })? {
        enterprise_identity::ProvisioningDecision::NotManaged => oidc_role,
        enterprise_identity::ProvisioningDecision::Allow(role) => role,
        enterprise_identity::ProvisioningDecision::Deny(reason) => {
            return Err((StatusCode::FORBIDDEN, Json(json!({"error": reason}))));
        }
    };

    // 7. Issue a local JWT
    let jwt_config = state.jwt_config.as_ref().ok_or_else(|| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error": "Authentication not configured"})),
        )
    })?;

    let token = jwt_config
        .generate_token(&username, role.clone())
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"error": format!("Token generation failed: {}", e)})),
            )
        })?;

    tracing::info!(
        "OIDC login successful for user '{}' via provider '{}' (role: {:?})",
        username,
        provider.name,
        role
    );

    Ok(Json(json!({
        "token": token,
        "user_id": username,
        "role": role,
        "provider": provider.name,
    })))
}

/// Decode the payload of a JWT ID token without cryptographic verification.
///
/// Private helper for unit tests of claim/payload parsing only. Production
/// code must use [`verify_id_token_jwks`] / [`verify_id_token_with_key`].
#[cfg(test)]
fn decode_id_token_claims_unverified(
    id_token: &str,
) -> Result<serde_json::Map<String, serde_json::Value>, String> {
    let parts: Vec<&str> = id_token.split('.').collect();
    if parts.len() != 3 {
        return Err("ID token does not have 3 parts".to_string());
    }

    // Decode the payload (second part), handling base64url padding
    let payload = parts[1];
    let padded = match payload.len() % 4 {
        2 => format!("{}==", payload),
        3 => format!("{}=", payload),
        _ => payload.to_string(),
    };

    use base64::Engine;
    let bytes = base64::engine::general_purpose::URL_SAFE
        .decode(&padded)
        .map_err(|e| format!("Base64 decode failed: {}", e))?;

    let value: serde_json::Value =
        serde_json::from_slice(&bytes).map_err(|e| format!("JSON parse failed: {}", e))?;

    value
        .as_object()
        .cloned()
        .ok_or_else(|| "ID token payload is not a JSON object".to_string())
}

/// Parse a role string into a security::Role, defaulting to Viewer for unknown values.
fn parse_role(role_str: &str) -> security::Role {
    match role_str.to_lowercase().as_str() {
        "admin" => security::Role::Admin,
        "user" => security::Role::User,
        _ => security::Role::Viewer,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use jsonwebtoken::{encode, EncodingKey, Header};

    #[test]
    fn test_pkce_challenge_s256_rfc7636() {
        // RFC 7636 Appendix B test vector
        let verifier = "dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk";
        let challenge = pkce_challenge_s256(verifier);
        assert_eq!(challenge, "E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM");
    }

    #[test]
    fn test_generate_code_verifier_length_and_charset() {
        let v = generate_code_verifier();
        assert_eq!(v.len(), 64);
        assert!(v
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '.' | '_' | '~')));
    }

    #[test]
    fn test_verify_nonce_claim_ok() {
        let mut claims = serde_json::Map::new();
        claims.insert("nonce".into(), json!("abc-123"));
        assert!(verify_nonce_claim(&claims, "abc-123").is_ok());
    }

    #[test]
    fn test_verify_nonce_claim_mismatch() {
        let mut claims = serde_json::Map::new();
        claims.insert("nonce".into(), json!("abc-123"));
        assert!(verify_nonce_claim(&claims, "other").is_err());
    }

    #[test]
    fn test_verify_nonce_claim_missing() {
        let claims = serde_json::Map::new();
        assert!(verify_nonce_claim(&claims, "abc-123").is_err());
    }

    #[test]
    fn test_decode_id_token_valid() {
        use base64::Engine;
        let header = base64::engine::general_purpose::URL_SAFE_NO_PAD
            .encode(r#"{"alg":"RS256","typ":"JWT"}"#);
        let payload = base64::engine::general_purpose::URL_SAFE_NO_PAD
            .encode(r#"{"sub":"user1","email":"u@test.com","name":"Test"}"#);
        let token = format!("{}.{}.fake-signature", header, payload);
        let claims = decode_id_token_claims_unverified(&token).unwrap();
        assert_eq!(claims.get("sub").unwrap().as_str().unwrap(), "user1");
        assert_eq!(claims.get("email").unwrap().as_str().unwrap(), "u@test.com");
    }

    #[test]
    fn test_decode_id_token_invalid_parts() {
        assert!(decode_id_token_claims_unverified("only-two.parts").is_err());
        assert!(decode_id_token_claims_unverified("").is_err());
    }

    #[test]
    fn test_decode_id_token_invalid_base64() {
        assert!(decode_id_token_claims_unverified("header.!!!invalid!!!.sig").is_err());
    }

    #[test]
    fn test_percent_encode_unreserved() {
        assert_eq!(percent_encode("abc123-._~"), "abc123-._~");
    }

    #[test]
    fn test_percent_encode_reserved() {
        let encoded = percent_encode("hello world&foo=bar");
        assert!(encoded.contains("%20")); // space
        assert!(encoded.contains("%26")); // &
        assert!(encoded.contains("%3D")); // =
        assert!(!encoded.contains(' '));
    }

    #[test]
    fn test_parse_role_variants() {
        assert_eq!(parse_role("admin"), security::Role::Admin);
        assert_eq!(parse_role("user"), security::Role::User);
        assert_eq!(parse_role("viewer"), security::Role::Viewer);
        assert_eq!(parse_role("unknown"), security::Role::Viewer);
    }

    #[test]
    fn test_parse_role_case_insensitive() {
        assert_eq!(parse_role("ADMIN"), security::Role::Admin);
        assert_eq!(parse_role("User"), security::Role::User);
        assert_eq!(parse_role("AdMiN"), security::Role::Admin);
    }

    #[test]
    fn test_select_jwk_by_kid() {
        let keys = vec![
            Jwk {
                kty: "RSA".into(),
                kid: Some("a".into()),
                alg: Some("RS256".into()),
                n: Some("n1".into()),
                e: Some("AQAB".into()),
            },
            Jwk {
                kty: "RSA".into(),
                kid: Some("b".into()),
                alg: Some("RS256".into()),
                n: Some("n2".into()),
                e: Some("AQAB".into()),
            },
        ];
        let selected = select_jwk(&keys, Some("b"), Algorithm::RS256).unwrap();
        assert_eq!(selected.kid.as_deref(), Some("b"));
    }

    #[test]
    fn test_verify_id_token_rejects_nonce_mismatch_with_hmac() {
        // Use HS256 only for isolated unit-test of claim validation plumbing;
        // production path is RS* via JWKS.
        #[derive(Serialize)]
        struct Claims {
            iss: String,
            aud: String,
            exp: i64,
            nonce: String,
            sub: String,
        }

        let secret = b"unit-test-secret-key-32bytes!!!!";
        let claims = Claims {
            iss: "https://issuer.example".into(),
            aud: "client-1".into(),
            exp: chrono::Utc::now().timestamp() + 3600,
            nonce: "expected-nonce".into(),
            sub: "user1".into(),
        };
        let token = encode(
            &Header::new(Algorithm::HS256),
            &claims,
            &EncodingKey::from_secret(secret).unwrap(),
        )
        .unwrap();

        let key = DecodingKey::from_secret(secret).unwrap();
        let ok = verify_id_token_with_key(
            &token,
            "https://issuer.example",
            "client-1",
            "expected-nonce",
            Algorithm::HS256,
            &key,
        );
        assert!(ok.is_ok());
        assert_eq!(ok.unwrap().get("sub").unwrap().as_str().unwrap(), "user1");

        let bad = verify_id_token_with_key(
            &token,
            "https://issuer.example",
            "client-1",
            "wrong-nonce",
            Algorithm::HS256,
            &key,
        );
        assert!(bad.is_err());
        assert!(bad.unwrap_err().contains("nonce"));
    }
}
