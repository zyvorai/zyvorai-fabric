// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: LicenseRef-Zyvor-Production-1.0

use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use cloud_init::{CloudInitConfig, CloudInitGenerator};
use serde_json::json;
use std::sync::Arc;

use crate::server::AppState;

pub async fn configure_cloud_init(
    State(state): State<Arc<AppState>>,
    Path(vm_name): Path<String>,
    Json(config): Json<CloudInitConfig>,
) -> impl IntoResponse {
    let generator = match CloudInitGenerator::new("/var/lib/zyvor-fabricd/cloud-init") {
        Ok(gen) => gen,
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": e.to_string() })),
            )
                .into_response()
        }
    };

    match generator.generate(&config) {
        Ok(iso_path) => (
            StatusCode::OK,
            Json(json!({
                "status": "created",
                "iso_path": iso_path.to_string_lossy()
            })),
        )
            .into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": e.to_string() })),
        )
            .into_response(),
    }
}
