// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: LicenseRef-Zyvor-Production-2.0

//! Thin wrapper around the shared [`api-error`] crate for Axum handlers.

use axum::{http::StatusCode, Json};

pub use api_error::api_error_json;

/// Map HTTP status to a stable `error_code` when none is provided explicitly.
pub fn error_code_for_status(status: StatusCode) -> &'static str {
    match status {
        StatusCode::NOT_FOUND => "not_found",
        StatusCode::BAD_REQUEST | StatusCode::CONFLICT => "invalid_request",
        StatusCode::FORBIDDEN => "forbidden",
        StatusCode::UNAUTHORIZED => "unauthorized",
        StatusCode::TOO_MANY_REQUESTS => "rate_limited",
        StatusCode::INTERNAL_SERVER_ERROR => "internal_error",
        _ => "operation_failed",
    }
}

/// Standard JSON error response with stable `error_code`.
pub fn json_error(
    status: StatusCode,
    msg: impl Into<String>,
) -> (StatusCode, Json<serde_json::Value>) {
    let code = error_code_for_status(status);
    (status, Json(api_error_json(code, msg.into())))
}

/// JSON error with an explicit stable `error_code`.
pub fn json_error_code(
    status: StatusCode,
    code: &str,
    msg: impl Into<String>,
) -> (StatusCode, Json<serde_json::Value>) {
    (status, Json(api_error_json(code, msg.into())))
}

/// Build error JSON with extra fields (e.g. `requires_2fa`).
pub fn json_error_extras(
    status: StatusCode,
    code: &str,
    msg: impl Into<String>,
    extras: serde_json::Value,
) -> (StatusCode, Json<serde_json::Value>) {
    let mut body = api_error_json(code, msg.into());
    if let Some(obj) = body.as_object_mut() {
        if let serde_json::Value::Object(extra) = extras {
            for (k, v) in extra {
                obj.insert(k, v);
            }
        }
    }
    (status, Json(body))
}
