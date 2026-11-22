// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: LicenseRef-Zyvor-Production-2.0

//! Prometheus request-metrics middleware.
//!
//! Counts API requests by method / matched route template / status class and
//! records latency in a histogram. Label cardinality is bounded: the route
//! label is always the axum `MatchedPath` template (e.g. `/api/vms/{name}`),
//! never the raw URI, so IDs and names never become label values. Requests
//! that match no route (static files, 404s, scanners) share one label,
//! [`UNMATCHED_ROUTE`]. Purely observational: the response is returned
//! untouched.

use axum::{
    extract::{MatchedPath, Request},
    middleware::Next,
    response::Response,
};
use std::time::Instant;

/// Single route label used for every request that matched no route template.
pub const UNMATCHED_ROUTE: &str = "unmatched";

/// Axum `from_fn` middleware; apply with
/// `.layer(axum::middleware::from_fn(metrics_layer::track_metrics))`.
pub async fn track_metrics(req: Request, next: Next) -> Response {
    let start = Instant::now();
    let method = req.method().as_str().to_owned();
    let route = req
        .extensions()
        .get::<MatchedPath>()
        .map(|p| p.as_str().to_owned())
        .unwrap_or_else(|| UNMATCHED_ROUTE.to_owned());

    let response = next.run(req).await;

    prometheus_exporter::observe_http_request(
        &method,
        &route,
        response.status().as_u16(),
        start.elapsed().as_secs_f64(),
    );
    response
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{body::Body, http::StatusCode, routing::get, Router};
    use tower::ServiceExt;

    fn app(tag: &str) -> Router {
        Router::new()
            .route(&format!("/{tag}/vms/{{name}}"), get(|| async { "ok" }))
            .route(
                &format!("/{tag}/boom"),
                get(|| async { StatusCode::INTERNAL_SERVER_ERROR }),
            )
            .layer(axum::middleware::from_fn(track_metrics))
    }

    async fn hit(app: &Router, uri: &str) -> StatusCode {
        app.clone()
            .oneshot(Request::builder().uri(uri).body(Body::empty()).unwrap())
            .await
            .unwrap()
            .status()
    }

    fn count(route: &str, class: &str) -> u64 {
        prometheus_exporter::HTTP_REQUESTS
            .with_label_values(&["GET", route, class])
            .get()
    }

    #[tokio::test]
    async fn uses_route_template_not_raw_path() {
        let app = app("t1");
        let before = count("/t1/vms/{name}", "2xx");
        assert_eq!(hit(&app, "/t1/vms/alpha").await, StatusCode::OK);
        assert_eq!(hit(&app, "/t1/vms/beta").await, StatusCode::OK);
        assert_eq!(count("/t1/vms/{name}", "2xx"), before + 2);

        // No series carries a raw VM name.
        let out = prometheus_exporter::render_text();
        for line in out
            .lines()
            .filter(|l| l.starts_with("zyvor_fabricd_http_requests_total"))
        {
            assert!(
                !line.contains("alpha") && !line.contains("beta"),
                "raw path leaked into label: {line}"
            );
        }
    }

    #[tokio::test]
    async fn unknown_routes_share_one_label_and_status_class_is_recorded() {
        let app = app("t2");
        // Unmatched counter is shared by design, so only assert it grows by >= 2.
        let before = count(UNMATCHED_ROUTE, "4xx");
        assert_eq!(hit(&app, "/nope/1").await, StatusCode::NOT_FOUND);
        assert_eq!(hit(&app, "/nope/2/3").await, StatusCode::NOT_FOUND);
        assert!(count(UNMATCHED_ROUTE, "4xx") >= before + 2);

        let before5 = count("/t2/boom", "5xx");
        assert_eq!(
            hit(&app, "/t2/boom").await,
            StatusCode::INTERNAL_SERVER_ERROR
        );
        assert_eq!(count("/t2/boom", "5xx"), before5 + 1);
    }

    #[tokio::test]
    async fn latency_histogram_is_observed() {
        let app = app("t3");
        let h = prometheus_exporter::HTTP_DURATION.with_label_values(&["GET", "/t3/vms/{name}"]);
        let before = h.get_sample_count();
        hit(&app, "/t3/vms/x").await;
        assert_eq!(h.get_sample_count(), before + 1);
    }
}
