// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: LicenseRef-Zyvor-Production-2.0

use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::IntoResponse,
    routing::get,
    Json, Router,
};
use serde::Deserialize;
use serde_json::{json, Value};

use crate::store::Cloud;

pub fn router(cloud: Cloud) -> Router {
    Router::new()
        .route("/", get(versions))
        .route("/v2/images", get(list).post(create))
        .route("/v2/images/{id}", get(get_one).delete(delete))
        .with_state(cloud)
}

async fn versions() -> Json<Value> {
    Json(json!({"versions": [{"id": "v2.9", "status": "CURRENT"}]}))
}

fn image_json(i: &crate::store::Image) -> Value {
    json!({
        "id": i.id,
        "name": i.name,
        "status": i.status,
        "visibility": i.visibility,
        "disk_format": i.disk_format,
        "container_format": i.container_format,
        "size": i.size,
        "schema": "/schemas/image"
    })
}

async fn list(State(cloud): State<Cloud>) -> Json<Value> {
    let images: Vec<Value> = cloud.list_images().await.iter().map(image_json).collect();
    Json(json!({"images": images, "first": "/v2/images", "schema": "/schemas/images"}))
}

#[derive(Deserialize)]
struct CreateImage {
    name: String,
    #[serde(default = "qcow")]
    disk_format: String,
}

fn qcow() -> String {
    "qcow2".into()
}

async fn create(State(cloud): State<Cloud>, Json(body): Json<CreateImage>) -> impl IntoResponse {
    let img = cloud.create_image(body.name, body.disk_format).await;
    (StatusCode::CREATED, Json(image_json(&img)))
}

async fn get_one(State(cloud): State<Cloud>, Path(id): Path<String>) -> impl IntoResponse {
    match cloud.get_image(&id).await {
        Some(i) => Json(image_json(&i)).into_response(),
        None => StatusCode::NOT_FOUND.into_response(),
    }
}

async fn delete(State(cloud): State<Cloud>, Path(id): Path<String>) -> impl IntoResponse {
    if cloud.delete_image(&id).await {
        StatusCode::NO_CONTENT.into_response()
    } else {
        StatusCode::NOT_FOUND.into_response()
    }
}
