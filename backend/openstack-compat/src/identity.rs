// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: LicenseRef-Zyvor-Production-2.0

use axum::{
    extract::State,
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use serde::Deserialize;
use serde_json::{json, Value};

use crate::store::{Cloud, Token};

pub fn router(cloud: Cloud) -> Router {
    Router::new()
        .route("/", get(versions))
        .route("/v3", get(v3_root))
        .route("/v3/", get(v3_root))
        .route("/v3/auth/tokens", post(issue_token).get(validate_token))
        .route("/v3/auth/catalog", get(catalog))
        .route("/v3/projects", get(projects))
        .route("/v3/users", get(users))
        .with_state(cloud)
}

async fn versions(State(cloud): State<Cloud>) -> Json<Value> {
    Json(json!({
        "versions": {
            "values": [{
                "id": "v3.14",
                "status": "stable",
                "updated": "2026-01-01T00:00:00Z",
                "links": [{"rel": "self", "href": format!("{}/identity/v3/", cloud.public_url)}]
            }]
        }
    }))
}

async fn v3_root(State(cloud): State<Cloud>) -> Json<Value> {
    Json(json!({
        "version": {
            "id": "v3.14",
            "status": "stable",
            "media-types": [{"base": "application/json", "type": "application/vnd.openstack.identity-v3+json"}],
            "links": [{"rel": "self", "href": format!("{}/identity/v3/", cloud.public_url)}]
        }
    }))
}

#[derive(Deserialize)]
struct AuthDoc {
    auth: Auth,
}

#[derive(Deserialize)]
struct Auth {
    identity: Identity,
    scope: Option<Scope>,
}

#[derive(Deserialize)]
struct Identity {
    password: Option<Password>,
    token: Option<TokenId>,
    #[allow(dead_code)]
    methods: Option<Vec<String>>,
}

#[derive(Deserialize)]
struct Password {
    user: UserAuth,
}

#[derive(Deserialize)]
struct UserAuth {
    name: Option<String>,
    id: Option<String>,
    password: Option<String>,
}

#[derive(Deserialize)]
struct TokenId {
    id: String,
}

#[derive(Deserialize)]
struct Scope {
    project: Option<ProjectScope>,
}

#[derive(Deserialize)]
struct ProjectScope {
    name: Option<String>,
    id: Option<String>,
}

async fn issue_token(State(cloud): State<Cloud>, Json(body): Json<AuthDoc>) -> Response {
    let user = body
        .auth
        .identity
        .password
        .as_ref()
        .and_then(|p| p.user.name.clone().or(p.user.id.clone()))
        .or_else(|| body.auth.identity.token.as_ref().map(|t| t.id.clone()))
        .unwrap_or_else(|| "admin".into());
    // Dev-mode Keystone: any password is accepted so `openstack` CLI works
    // against a fresh Fabric node. Wire to enterprise-identity on merge.
    let _ = body
        .auth
        .identity
        .password
        .as_ref()
        .and_then(|p| p.user.password.clone());
    let project = body
        .auth
        .scope
        .as_ref()
        .and_then(|s| s.project.as_ref())
        .and_then(|p| p.name.clone().or(p.id.clone()))
        .unwrap_or_else(|| "admin".into());
    let token = cloud.issue_token(&user, &project).await;
    let payload = token_body(&cloud, &token);
    let mut res = Json(payload).into_response();
    res.headers_mut().insert(
        "x-subject-token",
        token
            .id
            .parse()
            .unwrap_or(axum::http::HeaderValue::from_static("invalid")),
    );
    *res.status_mut() = StatusCode::CREATED;
    res
}

async fn validate_token(State(cloud): State<Cloud>, headers: HeaderMap) -> impl IntoResponse {
    let subject = headers
        .get("x-subject-token")
        .or_else(|| headers.get("x-auth-token"))
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    match cloud.token(subject).await {
        Some(token) => Json(token_body(&cloud, &token)).into_response(),
        None => (
            StatusCode::UNAUTHORIZED,
            Json(json!({"error": {"code": 401, "title": "Unauthorized"}})),
        )
            .into_response(),
    }
}

async fn catalog(State(cloud): State<Cloud>, headers: HeaderMap) -> impl IntoResponse {
    if cloud.token(auth_token(&headers)).await.is_none() {
        return (
            StatusCode::UNAUTHORIZED,
            Json(json!({"error": {"code": 401}})),
        )
            .into_response();
    }
    Json(json!({"catalog": crate::catalog::catalog(&cloud.public_url)})).into_response()
}

async fn projects(State(_cloud): State<Cloud>) -> Json<Value> {
    Json(json!({
        "projects": [
            {"id": "proj-admin", "name": "admin", "domain_id": "default", "enabled": true},
            {"id": "proj-fabric-default", "name": "fabric-default", "domain_id": "default", "enabled": true}
        ]
    }))
}

async fn users(State(_cloud): State<Cloud>) -> Json<Value> {
    Json(json!({
        "users": [
            {"id": "user-admin", "name": "admin", "domain_id": "default", "enabled": true}
        ]
    }))
}

pub fn auth_token(headers: &HeaderMap) -> &str {
    headers
        .get("x-auth-token")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
}

fn token_body(cloud: &Cloud, token: &Token) -> Value {
    json!({
        "token": {
            "expires_at": token.expires_at,
            "issued_at": chrono::Utc::now().to_rfc3339(),
            "methods": ["password"],
            "user": {"id": token.user_id, "name": token.user_name, "domain": {"id": "default", "name": "Default"}},
            "project": {"id": token.project_id, "name": token.project_name, "domain": {"id": "default", "name": "Default"}},
            "roles": token.roles.iter().map(|r| json!({"id": r, "name": r})).collect::<Vec<_>>(),
            "catalog": crate::catalog::catalog(&cloud.public_url)
        }
    })
}
