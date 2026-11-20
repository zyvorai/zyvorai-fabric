// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: LicenseRef-Zyvor-Production-1.0

use axum::{
    extract::State,
    http::{HeaderMap, StatusCode},
    response::IntoResponse,
    routing::get,
    Json, Router,
};
use serde::Deserialize;
use serde_json::{json, Value};

use crate::identity::auth_token;
use crate::store::Cloud;

pub fn router(cloud: Cloud) -> Router {
    Router::new()
        .route("/", get(versions))
        .route("/v2.0/networks", get(list_nets).post(create_net))
        .route("/v2.0/subnets", get(list_subnets).post(create_subnet))
        .route("/v2.0/ports", get(list_ports).post(create_port))
        .with_state(cloud)
}

async fn versions() -> Json<Value> {
    Json(json!({"versions": [{"id": "v2.0", "status": "CURRENT"}]}))
}

async fn list_nets(State(cloud): State<Cloud>) -> Json<Value> {
    Json(json!({"networks": cloud.list_networks().await}))
}

#[derive(Deserialize)]
struct WrapNet {
    network: NewNet,
}
#[derive(Deserialize)]
struct NewNet {
    name: String,
}

async fn create_net(
    State(cloud): State<Cloud>,
    headers: HeaderMap,
    Json(body): Json<WrapNet>,
) -> impl IntoResponse {
    let project = cloud
        .token(auth_token(&headers))
        .await
        .map(|t| t.project_id)
        .unwrap_or_else(|| "proj-admin".into());
    let net = cloud.create_network(body.network.name, project).await;
    (StatusCode::CREATED, Json(json!({"network": net})))
}

async fn list_subnets(State(cloud): State<Cloud>) -> Json<Value> {
    Json(json!({"subnets": cloud.list_subnets().await}))
}

#[derive(Deserialize)]
struct WrapSubnet {
    subnet: NewSubnet,
}
#[derive(Deserialize)]
struct NewSubnet {
    name: Option<String>,
    network_id: String,
    cidr: String,
}

async fn create_subnet(
    State(cloud): State<Cloud>,
    headers: HeaderMap,
    Json(body): Json<WrapSubnet>,
) -> impl IntoResponse {
    let project = cloud
        .token(auth_token(&headers))
        .await
        .map(|t| t.project_id)
        .unwrap_or_else(|| "proj-admin".into());
    let name = body.subnet.name.unwrap_or_else(|| "subnet".into());
    let sn = cloud
        .create_subnet(name, body.subnet.network_id, body.subnet.cidr, project)
        .await;
    (StatusCode::CREATED, Json(json!({"subnet": sn})))
}

async fn list_ports(State(cloud): State<Cloud>) -> Json<Value> {
    Json(json!({"ports": cloud.list_ports().await}))
}

#[derive(Deserialize)]
struct WrapPort {
    port: NewPort,
}
#[derive(Deserialize)]
struct NewPort {
    name: Option<String>,
    network_id: String,
}

async fn create_port(
    State(cloud): State<Cloud>,
    headers: HeaderMap,
    Json(body): Json<WrapPort>,
) -> impl IntoResponse {
    let project = cloud
        .token(auth_token(&headers))
        .await
        .map(|t| t.project_id)
        .unwrap_or_else(|| "proj-admin".into());
    let name = body.port.name.unwrap_or_else(|| "port".into());
    let port = cloud.create_port(name, body.port.network_id, project).await;
    (StatusCode::CREATED, Json(json!({"port": port})))
}
