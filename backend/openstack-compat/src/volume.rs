// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: LicenseRef-Zyvor-Production-2.0

use axum::{
    extract::{Path, State},
    http::{HeaderMap, StatusCode},
    response::IntoResponse,
    routing::{get, post, MethodRouter},
    Json, Router,
};
use serde::Deserialize;
use serde_json::{json, Value};

use crate::identity::auth_token;
use crate::store::Cloud;

pub fn router(cloud: Cloud) -> Router {
    Router::new()
        .route("/", get(versions))
        .route(
            "/v3/{project_id}/volumes",
            get(list).merge(guarded(&cloud, post(create))),
        )
        .route("/v3/{project_id}/volumes/detail", get(list))
        .route("/v3/{project_id}/volumes/{id}", get(get_one))
        .route(
            "/v3/{project_id}/volumes/{id}/action",
            guarded(&cloud, post(action)),
        )
        .with_state(cloud)
}

/// Wrap a mutating method handler so it requires a live token.
fn guarded(cloud: &Cloud, m: MethodRouter<Cloud>) -> MethodRouter<Cloud> {
    m.route_layer(axum::middleware::from_fn_with_state(
        cloud.clone(),
        crate::identity::require_token,
    ))
}

async fn versions() -> Json<Value> {
    Json(json!({"versions": [{"id": "v3.0", "status": "CURRENT"}]}))
}

fn vol_json(v: &crate::store::Volume) -> Value {
    json!({
        "id": v.id,
        "name": v.name,
        "size": v.size,
        "status": v.status,
        "bootable": if v.bootable { "true" } else { "false" },
        "attachments": v.attachments,
        "os-vol-tenant-attr:tenant_id": v.project_id
    })
}

async fn list(State(cloud): State<Cloud>) -> Json<Value> {
    let volumes: Vec<Value> = cloud.list_volumes().await.iter().map(vol_json).collect();
    Json(json!({"volumes": volumes}))
}

#[derive(Deserialize)]
struct WrapVol {
    volume: NewVol,
}
#[derive(Deserialize)]
struct NewVol {
    name: Option<String>,
    size: u32,
}

async fn create(
    State(cloud): State<Cloud>,
    headers: HeaderMap,
    Path(_project): Path<String>,
    Json(body): Json<WrapVol>,
) -> impl IntoResponse {
    let project = cloud
        .token(auth_token(&headers))
        .await
        .map(|t| t.project_id)
        .unwrap_or_else(|| "proj-admin".into());
    let name = body.volume.name.unwrap_or_else(|| "volume".into());
    let vol = cloud.create_volume(name, body.volume.size, project).await;
    (
        StatusCode::ACCEPTED,
        Json(json!({"volume": vol_json(&vol)})),
    )
}

async fn get_one(
    State(cloud): State<Cloud>,
    Path((_project, id)): Path<(String, String)>,
) -> impl IntoResponse {
    match cloud.get_volume(&id).await {
        Some(v) => Json(json!({"volume": vol_json(&v)})).into_response(),
        None => StatusCode::NOT_FOUND.into_response(),
    }
}

async fn action(
    State(cloud): State<Cloud>,
    Path((_project, id)): Path<(String, String)>,
    Json(body): Json<Value>,
) -> impl IntoResponse {
    if let Some(att) = body.get("os-attach") {
        let server = att
            .get("instance_uuid")
            .or_else(|| att.get("server_id"))
            .and_then(|v| v.as_str())
            .unwrap_or("");
        return match cloud.attach_volume(&id, server).await {
            Some(_) => StatusCode::ACCEPTED.into_response(),
            None => StatusCode::NOT_FOUND.into_response(),
        };
    }
    if body.get("os-detach").is_some() {
        return match cloud.detach_volume(&id).await {
            Some(_) => StatusCode::ACCEPTED.into_response(),
            None => StatusCode::NOT_FOUND.into_response(),
        };
    }
    (
        StatusCode::BAD_REQUEST,
        Json(json!({"badRequest": {"message": "unsupported volume action"}})),
    )
        .into_response()
}
