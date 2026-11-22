// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: LicenseRef-Zyvor-Production-2.0

pub mod db;
pub mod pam_auth;
pub mod totp;
pub mod trial;

use anyhow::Result;
use axum::{
    extract::{Request, State},
    http::{header, StatusCode},
    middleware::Next,
    response::Response,
};
use jsonwebtoken::{decode, encode, DecodingKey, EncodingKey, Header, Validation};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Claims {
    pub sub: String, // subject (user id)
    pub role: Role,
    pub exp: usize, // expiration time
    #[serde(default)]
    pub jti: String, // JWT ID for revocation
    /// When set, scopes VM create/list/get/mutate to this tenant (mirrors FluxVM token tenant).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tenant: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    Admin,
    User,
    Viewer,
}

impl Role {
    pub fn can_write(&self) -> bool {
        matches!(self, Role::Admin | Role::User)
    }

    pub fn can_read(&self) -> bool {
        true // All roles can read
    }

    pub fn can_manage(&self) -> bool {
        matches!(self, Role::Admin)
    }
}

pub struct JwtConfig {
    pub secret: String,
    pub expiration_hours: i64,
    /// Maps JTI -> expiration timestamp so expired entries can be evicted.
    revoked_tokens: std::sync::Mutex<std::collections::HashMap<String, usize>>,
}

impl JwtConfig {
    pub fn new(secret: String) -> Self {
        Self {
            secret,
            expiration_hours: 24,
            revoked_tokens: std::sync::Mutex::new(std::collections::HashMap::new()),
        }
    }

    pub fn generate_token(&self, user_id: &str, role: Role) -> Result<String> {
        self.generate_token_with_tenant(user_id, role, None)
    }

    pub fn generate_token_with_tenant(
        &self,
        user_id: &str,
        role: Role,
        tenant: Option<String>,
    ) -> Result<String> {
        let hours = if self.expiration_hours < 1 {
            tracing::warn!(
                "Token expiration_hours ({}) is too low, using minimum of 1 hour",
                self.expiration_hours
            );
            1
        } else {
            self.expiration_hours
        };
        let expiration = chrono::Utc::now()
            .checked_add_signed(chrono::Duration::hours(hours))
            .ok_or_else(|| anyhow::anyhow!("Token expiration time overflow"))?
            .timestamp() as usize;

        let claims = Claims {
            sub: user_id.to_string(),
            role,
            exp: expiration,
            jti: uuid::Uuid::new_v4().to_string(),
            tenant: tenant.filter(|t| !t.is_empty()),
        };

        let token = encode(
            &Header::default(),
            &claims,
            &EncodingKey::from_secret(self.secret.as_bytes()),
        )?;

        Ok(token)
    }

    pub fn validate_token(&self, token: &str) -> Result<Claims> {
        let token_data = decode::<Claims>(
            token,
            &DecodingKey::from_secret(self.secret.as_bytes()),
            &Validation::default(),
        )?;

        let claims = token_data.claims;
        let revoked = self.revoked_tokens.lock().unwrap_or_else(|e| {
            tracing::warn!("Revoked tokens mutex was poisoned, recovering");
            e.into_inner()
        });
        if !claims.jti.is_empty() && revoked.contains_key(&claims.jti) {
            return Err(anyhow::anyhow!("Token has been revoked"));
        }
        drop(revoked);
        Ok(claims)
    }

    pub fn revoke_token(&self, jti: &str) {
        self.revoke_token_with_exp(jti, 0);
    }

    /// Revoke a token, recording its expiration time for later cleanup.
    pub fn revoke_token_with_exp(&self, jti: &str, exp: usize) {
        if let Ok(mut revoked) = self.revoked_tokens.lock() {
            revoked.insert(jti.to_string(), exp);
            // Evict expired entries when the set grows large
            if revoked.len() > 100_000 {
                let now = chrono::Utc::now().timestamp() as usize;
                revoked.retain(|_, token_exp| *token_exp == 0 || *token_exp > now);
            }
        }
    }
}

pub async fn auth_middleware(
    State(jwt_config): State<Arc<JwtConfig>>,
    mut req: Request,
    next: Next,
) -> Result<Response, StatusCode> {
    let auth_header = req
        .headers()
        .get(header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok());

    let token = match auth_header {
        Some(header) => header
            .strip_prefix("Bearer ")
            .ok_or(StatusCode::UNAUTHORIZED)?,
        _ => return Err(StatusCode::UNAUTHORIZED),
    };

    let claims = jwt_config
        .validate_token(token)
        .map_err(|_| StatusCode::UNAUTHORIZED)?;

    // Add claims to request extensions
    req.extensions_mut().insert(claims);

    Ok(next.run(req).await)
}

/// Same as `auth_middleware`, but also accepts the token as a `?token=`
/// query parameter when there's no `Authorization` header. Browsers'
/// native `WebSocket` constructor can't set arbitrary headers on the
/// upgrade request, so this is the only way a bearer-token-authenticated
/// browser client can authenticate a WebSocket handshake at all — use this
/// (never `auth_middleware`) on WebSocket routes specifically, not on
/// regular HTTP API routes, to keep tokens-in-URLs scoped to the one case
/// that needs it.
pub async fn ws_auth_middleware(
    State(jwt_config): State<Arc<JwtConfig>>,
    mut req: Request,
    next: Next,
) -> Result<Response, StatusCode> {
    let header_token = req
        .headers()
        .get(header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.strip_prefix("Bearer "))
        .map(str::to_string);

    let query_token = header_token
        .is_none()
        .then(|| {
            req.uri().query().and_then(|query| {
                query.split('&').find_map(|pair| {
                    let (key, value) = pair.split_once('=')?;
                    (key == "token").then(|| value.to_string())
                })
            })
        })
        .flatten();

    let token = header_token
        .or(query_token)
        .ok_or(StatusCode::UNAUTHORIZED)?;

    let claims = jwt_config
        .validate_token(&token)
        .map_err(|_| StatusCode::UNAUTHORIZED)?;

    req.extensions_mut().insert(claims);

    Ok(next.run(req).await)
}

// ============================================================================
// Role-based authorization extractors
// ============================================================================

/// When auth middleware is not active (auth disabled), extractors use this
/// default Claims. Defaults to Viewer role so that unauthenticated requests
/// only get read access, not full admin privileges. Endpoints that require
/// write or admin access will reject these claims via their role checks.
fn unauthenticated_claims() -> Claims {
    Claims {
        sub: "anonymous".to_string(),
        role: Role::Viewer,
        exp: usize::MAX,
        jti: String::new(),
        tenant: None,
    }
}

/// Extractor that requires the caller to have at least read permission (any role).
/// When auth is disabled (no claims in extensions), allows the request through.
pub struct RequireRead(pub Claims);

impl<S> axum::extract::FromRequestParts<S> for RequireRead
where
    S: Send + Sync,
{
    type Rejection = StatusCode;

    async fn from_request_parts(
        parts: &mut axum::http::request::Parts,
        _state: &S,
    ) -> Result<Self, Self::Rejection> {
        let claims = parts
            .extensions
            .get::<Claims>()
            .cloned()
            .unwrap_or_else(unauthenticated_claims);
        Ok(RequireRead(claims))
    }
}

/// Extractor that requires the caller to have write permission (Admin or User).
/// Rejects Viewer role with 403 Forbidden. Also rejects with 402 Payment
/// Required once the evaluation trial has lapsed -- reads stay open (see
/// `trial` module's doc comment) so a lapsed trial doesn't strand an
/// evaluator behind a wall they can't even look through; only new writes
/// require a current trial (or, once real licensing exists, a license).
/// When auth is disabled (no claims in extensions), defaults to Viewer role
/// which will be rejected by the role check below.
pub struct RequireWrite(pub Claims);

impl<S> axum::extract::FromRequestParts<S> for RequireWrite
where
    S: Send + Sync,
{
    type Rejection = StatusCode;

    async fn from_request_parts(
        parts: &mut axum::http::request::Parts,
        _state: &S,
    ) -> Result<Self, Self::Rejection> {
        let claims = parts
            .extensions
            .get::<Claims>()
            .cloned()
            .unwrap_or_else(unauthenticated_claims);
        if !claims.role.can_write() {
            tracing::warn!(
                "Authorization denied: user '{}' with role {:?} attempted a write operation",
                claims.sub,
                claims.role
            );
            return Err(StatusCode::FORBIDDEN);
        }
        if trial::is_expired() {
            tracing::warn!("Write operation rejected: evaluation trial has expired");
            return Err(StatusCode::PAYMENT_REQUIRED);
        }
        Ok(RequireWrite(claims))
    }
}

/// Extractor that requires the caller to have admin permission (Admin only).
/// Rejects User and Viewer roles with 403 Forbidden. Also rejects with 402
/// Payment Required once the evaluation trial has lapsed -- see
/// `RequireWrite`'s matching doc comment.
/// When auth is disabled (no claims in extensions), defaults to Viewer role
/// which will be rejected by the role check below.
pub struct RequireAdmin(pub Claims);

impl<S> axum::extract::FromRequestParts<S> for RequireAdmin
where
    S: Send + Sync,
{
    type Rejection = StatusCode;

    async fn from_request_parts(
        parts: &mut axum::http::request::Parts,
        _state: &S,
    ) -> Result<Self, Self::Rejection> {
        let claims = parts
            .extensions
            .get::<Claims>()
            .cloned()
            .unwrap_or_else(unauthenticated_claims);
        if !claims.role.can_manage() {
            tracing::warn!(
                "Authorization denied: user '{}' with role {:?} attempted an admin operation",
                claims.sub,
                claims.role
            );
            return Err(StatusCode::FORBIDDEN);
        }
        if trial::is_expired() {
            tracing::warn!("Admin operation rejected: evaluation trial has expired");
            return Err(StatusCode::PAYMENT_REQUIRED);
        }
        Ok(RequireAdmin(claims))
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct User {
    pub id: String,
    pub username: String,
    pub password_hash: String,
    pub role: Role,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tenant: Option<String>,
}

impl User {
    pub fn new(id: String, username: String, password: &str, role: Role) -> Result<Self> {
        let password_hash = bcrypt::hash(password, bcrypt::DEFAULT_COST)?;
        Ok(Self {
            id,
            username,
            password_hash,
            role,
            tenant: None,
        })
    }

    pub fn verify_password(&self, password: &str) -> Result<bool> {
        Ok(bcrypt::verify(password, &self.password_hash)?)
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct LoginRequest {
    pub username: String,
    pub password: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct LoginResponse {
    pub token: String,
    pub user_id: String,
    pub role: Role,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tenant: Option<String>,
}

// Audit logging
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditLog {
    pub id: String,
    pub timestamp: chrono::DateTime<chrono::Utc>,
    pub user_id: String,
    pub action: String,
    pub resource: String,
    pub status: String,
}

impl AuditLog {
    pub fn new(user_id: String, action: String, resource: String, status: String) -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            timestamp: chrono::Utc::now(),
            user_id,
            action,
            resource,
            status,
        }
    }

    pub fn log(&self) -> Result<()> {
        tracing::info!(
            "AUDIT: {} {} {} {} at {}",
            self.user_id,
            self.action,
            self.resource,
            self.status,
            self.timestamp
        );
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_jwt() -> JwtConfig {
        JwtConfig::new("test-secret-key-for-unit-tests".to_string())
    }

    #[test]
    fn test_generate_and_validate_token() {
        let jwt = test_jwt();
        let token = jwt.generate_token("alice", Role::Admin).unwrap();
        let claims = jwt.validate_token(&token).unwrap();
        assert_eq!(claims.sub, "alice");
        assert_eq!(claims.role, Role::Admin);
        assert!(!claims.jti.is_empty());
    }

    #[test]
    fn test_validate_token_wrong_secret() {
        let jwt_a = JwtConfig::new("secret-A".to_string());
        let jwt_b = JwtConfig::new("secret-B".to_string());
        let token = jwt_a.generate_token("alice", Role::User).unwrap();
        assert!(jwt_b.validate_token(&token).is_err());
    }

    #[test]
    fn test_revoke_token_rejects_validation() {
        let jwt = test_jwt();
        let token = jwt.generate_token("alice", Role::User).unwrap();
        let claims = jwt.validate_token(&token).unwrap();
        jwt.revoke_token(&claims.jti);
        let result = jwt.validate_token(&token);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("revoked"));
    }

    #[test]
    fn test_revoke_token_with_exp() {
        let jwt = test_jwt();
        let token = jwt.generate_token("bob", Role::Viewer).unwrap();
        let claims = jwt.validate_token(&token).unwrap();
        jwt.revoke_token_with_exp(&claims.jti, claims.exp);
        assert!(jwt.validate_token(&token).is_err());
    }

    #[test]
    fn test_generate_token_all_roles() {
        let jwt = test_jwt();
        for role in &[Role::Admin, Role::User, Role::Viewer] {
            let token = jwt.generate_token("user", role.clone()).unwrap();
            let claims = jwt.validate_token(&token).unwrap();
            assert_eq!(&claims.role, role);
        }
    }

    #[test]
    fn test_role_permissions() {
        assert!(Role::Admin.can_read());
        assert!(Role::Admin.can_write());
        assert!(Role::Admin.can_manage());

        assert!(Role::User.can_read());
        assert!(Role::User.can_write());
        assert!(!Role::User.can_manage());

        assert!(Role::Viewer.can_read());
        assert!(!Role::Viewer.can_write());
        assert!(!Role::Viewer.can_manage());
    }

    #[test]
    fn test_validate_expired_token() {
        let jwt = test_jwt();
        // Manually construct an expired token
        let claims = Claims {
            sub: "expired-user".to_string(),
            role: Role::User,
            exp: 1, // Unix timestamp 1 = 1970-01-01, long expired
            jti: "test-jti".to_string(),
            tenant: None,
        };
        let token = jsonwebtoken::encode(
            &jsonwebtoken::Header::default(),
            &claims,
            &jsonwebtoken::EncodingKey::from_secret(jwt.secret.as_bytes()),
        )
        .unwrap();
        assert!(jwt.validate_token(&token).is_err());
    }

    #[test]
    fn test_revoked_tokens_eviction() {
        let jwt = test_jwt();
        let past_exp = 1usize; // long expired
                               // Insert enough entries to trigger eviction
        for i in 0..100_001 {
            jwt.revoke_token_with_exp(&format!("jti-{}", i), past_exp);
        }
        // Expired entries should have been evicted
        let revoked = jwt.revoked_tokens.lock().unwrap();
        // After eviction of expired entries, the map should be much smaller
        // (only entries with exp=0 or exp>now survive — our entries are all past_exp=1 which is expired)
        assert!(
            revoked.len() < 100_001,
            "Eviction should have removed expired entries"
        );
    }
}
