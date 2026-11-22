// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: LicenseRef-Zyvor-Production-2.0

use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use tower::ServiceExt;

mod common;

async fn get(app: &axum::Router, uri: &str) -> (StatusCode, String) {
    let resp = app
        .clone()
        .oneshot(Request::builder().uri(uri).body(Body::empty()).unwrap())
        .await
        .unwrap();
    let status = resp.status();
    let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX)
        .await
        .unwrap();
    (status, String::from_utf8_lossy(&bytes).into_owned())
}

#[tokio::test]
async fn metrics_exports_request_series_with_bounded_labels() {
    let app = common::create_test_app().await;
    assert_eq!(get(&app, "/health").await.0, StatusCode::OK);
    // A path that matches no API route must not create a per-path series.
    get(&app, "/api/definitely-not-a-route-12345").await;

    let (status, body) = get(&app, "/metrics").await;
    assert_eq!(status, StatusCode::OK);
    assert!(body.contains(
        "zyvor_fabricd_http_requests_total{method=\"GET\",route=\"/health\",status_class=\"2xx\"}"
    ));
    assert!(body.contains("zyvor_fabricd_http_request_duration_seconds_bucket"));
    assert!(!body.contains("definitely-not-a-route"));
    // VM gauges are refreshed from the store on scrape (fresh store: zero VMs).
    assert!(body.contains("zyvor_fabricd_vms_total 0"));
    assert!(body.contains("zyvor_fabricd_vms_by_state{state=\"running\"} 0"));
    // Per-VM series are off by default.
    assert!(!body.contains("zyvor_fabricd_vm_cpu_usage_percent{"));
}
