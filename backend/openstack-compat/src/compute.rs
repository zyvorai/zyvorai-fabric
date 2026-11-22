// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: LicenseRef-Zyvor-Production-2.0

use axum::{
    extract::{Path, State},
    http::{HeaderMap, StatusCode},
    response::IntoResponse,
    routing::{get, post},
    Json, Router,
};
use serde::Deserialize;
use serde_json::{json, Value};

use crate::identity::auth_token;
use crate::store::Cloud;

pub fn router(cloud: Cloud) -> Router {
    Router::new()
        .route("/", get(version))
        .route("/v2.1", get(version))
        .route("/v2.1/", get(version))
        .route("/v2.1/flavors", get(flavors))
        .route("/v2.1/flavors/detail", get(flavors_detail))
        .route("/v2.1/flavors/{id}", get(flavor_show))
        .route("/v2.1/servers", get(list_servers).post(create_server))
        .route("/v2.1/servers/detail", get(list_servers_detail))
        .route("/v2.1/servers/{id}", get(get_server).delete(delete_server))
        .route("/v2.1/servers/{id}/action", post(server_action))
        .with_state(cloud)
}

async fn version(State(cloud): State<Cloud>) -> Json<Value> {
    Json(json!({
        "version": {
            "id": "v2.1",
            "status": "CURRENT",
            "min_version": "2.1",
            "version": "2.96",
            "links": [{"rel": "self", "href": format!("{}/compute/v2.1/", cloud.public_url)}]
        }
    }))
}

fn flavor_json(f: &crate::store::Flavor, detail: bool) -> Value {
    let mut v = json!({"id": f.id, "name": f.name, "links": []});
    if detail {
        v["vcpus"] = json!(f.vcpus);
        v["ram"] = json!(f.ram);
        v["disk"] = json!(f.disk);
        v["os-flavor-access:is_public"] = json!(true);
    }
    v
}

async fn flavors(State(cloud): State<Cloud>) -> Json<Value> {
    let list: Vec<Value> = cloud
        .flavors()
        .await
        .iter()
        .map(|f| flavor_json(f, false))
        .collect();
    Json(json!({"flavors": list}))
}

async fn flavors_detail(State(cloud): State<Cloud>) -> Json<Value> {
    let list: Vec<Value> = cloud
        .flavors()
        .await
        .iter()
        .map(|f| flavor_json(f, true))
        .collect();
    Json(json!({"flavors": list}))
}

async fn flavor_show(State(cloud): State<Cloud>, Path(id): Path<String>) -> impl IntoResponse {
    match cloud.flavor(&id).await {
        Some(f) => Json(json!({"flavor": flavor_json(&f, true)})).into_response(),
        None => StatusCode::NOT_FOUND.into_response(),
    }
}

fn server_json(s: &crate::store::Server, detail: bool) -> Value {
    let mut v = json!({
        "id": s.id,
        "name": s.name,
        "status": s.status,
        "links": []
    });
    if detail {
        v["flavor"] = json!({"id": s.flavor_id});
        v["image"] = json!({"id": s.image_id});
        v["addresses"] = json!(s.addresses);
        v["tenant_id"] = json!(s.project_id);
        v["user_id"] = json!("user-admin");
        v["created"] = json!(s.created);
        v["updated"] = json!(s.updated);
        v["OS-EXT-STS:vm_state"] = json!(if s.status == "ACTIVE" {
            "active"
        } else {
            "stopped"
        });
        v["OS-EXT-STS:power_state"] = json!(if s.status == "ACTIVE" { 1 } else { 4 });
        v["metadata"] = json!({"fabric": "true"});
    }
    v
}

async fn list_servers(State(cloud): State<Cloud>) -> Json<Value> {
    let list: Vec<Value> = cloud
        .list_servers()
        .await
        .iter()
        .map(|s| server_json(s, false))
        .collect();
    Json(json!({"servers": list}))
}

async fn list_servers_detail(State(cloud): State<Cloud>) -> Json<Value> {
    let list: Vec<Value> = cloud
        .list_servers()
        .await
        .iter()
        .map(|s| server_json(s, true))
        .collect();
    Json(json!({"servers": list}))
}

#[derive(Deserialize)]
struct CreateServer {
    server: CreateServerBody,
}

#[derive(Deserialize)]
#[allow(non_snake_case)]
struct CreateServerBody {
    name: String,
    flavorRef: Option<String>,
    imageRef: Option<String>,
    networks: Option<Vec<NetRef>>,
}

#[derive(Deserialize)]
struct NetRef {
    uuid: Option<String>,
}

async fn create_server(
    State(cloud): State<Cloud>,
    headers: HeaderMap,
    Json(body): Json<CreateServer>,
) -> impl IntoResponse {
    let flavor = body.server.flavorRef.unwrap_or_else(|| "1".into());
    if cloud.flavor(&flavor).await.is_none() {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({"badRequest": {"code": 400, "message": "flavor not found"}})),
        )
            .into_response();
    }
    let image = body.server.imageRef.unwrap_or_else(|| "cirros".into());
    if cloud.get_image(&image).await.is_none() {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({"badRequest": {"code": 400, "message": "image not found"}})),
        )
            .into_response();
    }
    let net = body
        .server
        .networks
        .and_then(|n| n.into_iter().next().and_then(|x| x.uuid));
    let project = cloud
        .token(auth_token(&headers))
        .await
        .map(|t| t.project_id)
        .unwrap_or_else(|| "proj-admin".into());
    let server = cloud
        .create_server(body.server.name, flavor, image, project, net)
        .await;
    (
        StatusCode::ACCEPTED,
        Json(json!({"server": server_json(&server, true)})),
    )
        .into_response()
}

async fn get_server(State(cloud): State<Cloud>, Path(id): Path<String>) -> impl IntoResponse {
    match cloud.get_server(&id).await {
        Some(s) => Json(json!({"server": server_json(&s, true)})).into_response(),
        None => (
            StatusCode::NOT_FOUND,
            Json(json!({"itemNotFound": {"code": 404, "message": "server not found"}})),
        )
            .into_response(),
    }
}

async fn delete_server(State(cloud): State<Cloud>, Path(id): Path<String>) -> impl IntoResponse {
    if cloud.delete_server(&id).await {
        StatusCode::NO_CONTENT.into_response()
    } else {
        StatusCode::NOT_FOUND.into_response()
    }
}

async fn server_action(
    State(cloud): State<Cloud>,
    Path(id): Path<String>,
    Json(body): Json<Value>,
) -> impl IntoResponse {
    let status = if body.get("os-start").is_some() {
        "ACTIVE"
    } else if body.get("os-stop").is_some() {
        "SHUTOFF"
    } else if body.get("reboot").is_some() {
        "ACTIVE"
    } else if body.get("pause").is_some() {
        "PAUSED"
    } else if body.get("unpause").is_some() {
        "ACTIVE"
    } else {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({"badRequest": {"message": "unsupported action"}})),
        )
            .into_response();
    };
    match cloud.set_server_status(&id, status).await {
        Some(_) => StatusCode::ACCEPTED.into_response(),
        None => StatusCode::NOT_FOUND.into_response(),
    }
}
