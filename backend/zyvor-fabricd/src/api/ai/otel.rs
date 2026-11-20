// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: LicenseRef-Zyvor-Production-1.0

//! Optional OTLP/HTTP traces for the inference gateway.
//!
//! Unset `FLUXVM_AI_OTEL_ENDPOINT` exports nothing. The prompt and the API
//! key are not attributes. A failed export does not fail the request.

use serde_json::{json, Value};

pub fn span_body(endpoint: &str, status: u16, tokens: u64) -> Value {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or(0);
    json!({
        "resourceSpans": [{
            "scopeSpans": [{
                "scope": { "name": "zyvor-fabricd-ai" },
                "spans": [{
                    "traceId": hex_id(16),
                    "spanId": hex_id(8),
                    "name": "inference",
                    "kind": 2,
                    "startTimeUnixNano": now.to_string(),
                    "endTimeUnixNano": now.to_string(),
                    "attributes": [
                        { "key": "endpoint", "value": { "stringValue": endpoint } },
                        { "key": "http.status_code", "value": { "intValue": status } },
                        { "key": "tokens", "value": { "intValue": tokens } }
                    ]
                }]
            }]
        }]
    })
}

fn hex_id(bytes: usize) -> String {
    let mut raw = vec![0u8; bytes];
    if let Ok(mut file) = std::fs::File::open("/dev/urandom") {
        use std::io::Read;
        let _ = file.read_exact(&mut raw);
    }
    raw.iter().map(|byte| format!("{byte:02x}")).collect()
}

pub fn emit_gateway(endpoint: &str, status: u16, tokens: u64) {
    let Ok(collector) = std::env::var("FLUXVM_AI_OTEL_ENDPOINT") else {
        return;
    };
    let collector = collector.trim().trim_end_matches('/').to_string();
    if collector.is_empty() {
        return;
    }
    let body = span_body(endpoint, status, tokens);
    tokio::spawn(async move {
        let url = format!("{collector}/v1/traces");
        let result = reqwest::Client::new()
            .post(url)
            .header("content-type", "application/json")
            .json(&body)
            .send()
            .await;
        if let Err(err) = result {
            tracing::warn!("OTLP export failed: {err}");
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn span_names_the_endpoint_and_not_a_prompt() {
        let body = span_body("qwen", 200, 12);
        let text = body.to_string();
        assert!(text.contains("qwen"));
        assert!(text.contains("\"tokens\""));
        assert!(!text.contains("prompt"));
        assert!(!text.contains("api_key"));
    }
}
