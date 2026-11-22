// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: LicenseRef-Zyvor-Production-2.0

//! Auth matrix for the combined compat router: every mutating route must
//! reject missing, unknown and expired tokens with the OpenStack-style 401
//! and keep its normal success status for a live token.

use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use http_body_util::BodyExt;
use openstack_compat::{router_with_cloud, Cloud};
use serde_json::{json, Value};
use tower::ServiceExt;

async fn send(
    app: &axum::Router,
    method: &str,
    uri: &str,
    token: Option<&str>,
    body: Option<Value>,
) -> (StatusCode, Value) {
    let mut b = Request::builder().method(method).uri(uri);
    if let Some(t) = token {
        b = b.header("x-auth-token", t);
    }
    if body.is_some() {
        b = b.header("content-type", "application/json");
    }
    let req = b
        .body(body.map_or_else(Body::empty, |v| Body::from(v.to_string())))
        .unwrap();
    let res = app.clone().oneshot(req).await.unwrap();
    let status = res.status();
    let bytes = res.into_body().collect().await.unwrap().to_bytes();
    let v = serde_json::from_slice(&bytes).unwrap_or(Value::Null);
    (status, v)
}

struct Fixture {
    app: axum::Router,
    live: String,
    expired: String,
    server_id: String,
    volume_id: String,
    network_id: String,
}

async fn fixture() -> Fixture {
    let cloud = Cloud::new("http://127.0.0.1:8080");
    let live = cloud.issue_token("admin", "admin").await.id;
    let expired = cloud
        .issue_token_with_ttl("admin", "admin", chrono::Duration::seconds(-60))
        .await
        .id;
    let server_id = cloud
        .create_server(
            "fx".into(),
            "1".into(),
            "cirros".into(),
            "proj-admin".into(),
            None,
        )
        .await
        .id;
    let volume_id = cloud
        .create_volume("fx".into(), 1, "proj-admin".into())
        .await
        .id;
    let network_id = cloud
        .create_network("fx".into(), "proj-admin".into())
        .await
        .id;
    Fixture {
        app: router_with_cloud(cloud),
        live,
        expired,
        server_id,
        volume_id,
        network_id,
    }
}

/// (method, uri, body, status expected with a live token). `{s}`, `{v}` and
/// `{n}` are replaced by the fixture server, volume and network ids.
fn mutating() -> Vec<(&'static str, String, Value, StatusCode)> {
    let a = StatusCode::ACCEPTED;
    vec![
        (
            "POST",
            "/compute/v2.1/servers".into(),
            json!({"server":{"name":"x","flavorRef":"1","imageRef":"cirros"}}),
            a,
        ),
        (
            "POST",
            "/compute/v2.1/servers/{s}/action".into(),
            json!({"os-stop": null}),
            a,
        ),
        (
            "DELETE",
            "/compute/v2.1/servers/{s}".into(),
            Value::Null,
            StatusCode::NO_CONTENT,
        ),
        (
            "POST",
            "/image/v2/images".into(),
            json!({"name":"img"}),
            StatusCode::CREATED,
        ),
        (
            "DELETE",
            "/image/v2/images/aaaaaaaa-aaaa-aaaa-aaaa-aaaaaaaaaaa1".into(),
            Value::Null,
            StatusCode::NO_CONTENT,
        ),
        (
            "POST",
            "/network/v2.0/networks".into(),
            json!({"network":{"name":"n"}}),
            StatusCode::CREATED,
        ),
        (
            "POST",
            "/network/v2.0/subnets".into(),
            json!({"subnet":{"network_id":"{n}","cidr":"10.9.0.0/24"}}),
            StatusCode::CREATED,
        ),
        (
            "POST",
            "/network/v2.0/ports".into(),
            json!({"port":{"network_id":"{n}"}}),
            StatusCode::CREATED,
        ),
        (
            "POST",
            "/volume/v3/proj-admin/volumes".into(),
            json!({"volume":{"size":1}}),
            a,
        ),
        (
            "POST",
            "/volume/v3/proj-admin/volumes/{v}/action".into(),
            json!({"os-detach": {}}),
            a,
        ),
    ]
}

fn fill(s: &str, f: &Fixture) -> String {
    s.replace("{s}", &f.server_id)
        .replace("{v}", &f.volume_id)
        .replace("{n}", &f.network_id)
}

fn body_of(v: &Value, f: &Fixture) -> Option<Value> {
    if v.is_null() {
        None
    } else {
        Some(serde_json::from_str(&fill(&v.to_string(), f)).unwrap())
    }
}

#[tokio::test]
async fn mutating_routes_reject_missing_bad_and_expired_tokens() {
    for (method, uri, body, _) in mutating() {
        for (label, tok) in [
            ("none", None),
            ("bad", Some("tok-nope")),
            ("expired", Some("EXP")),
        ] {
            let f = fixture().await;
            let tok = match tok {
                Some("EXP") => Some(f.expired.as_str()),
                other => other,
            };
            let (st, v) = send(&f.app, method, &fill(&uri, &f), tok, body_of(&body, &f)).await;
            assert_eq!(
                st,
                StatusCode::UNAUTHORIZED,
                "{method} {uri} with {label} token"
            );
            assert_eq!(v, json!({"error": {"code": 401}}), "{method} {uri} {label}");
        }
        // 401 comes before body parsing, so a malformed body is still 401.
        let f = fixture().await;
        let req = Request::builder()
            .method(method)
            .uri(fill(&uri, &f))
            .header("content-type", "application/json")
            .body(Body::from("not json"))
            .unwrap();
        let res = f.app.clone().oneshot(req).await.unwrap();
        assert_eq!(
            res.status(),
            StatusCode::UNAUTHORIZED,
            "{method} {uri} bad body"
        );
    }
}

#[tokio::test]
async fn mutating_routes_accept_a_live_token() {
    for (method, uri, body, want) in mutating() {
        let f = fixture().await;
        let (st, _) = send(
            &f.app,
            method,
            &fill(&uri, &f),
            Some(&f.live),
            body_of(&body, &f),
        )
        .await;
        assert_eq!(st, want, "{method} {uri} with live token");
    }
}

#[tokio::test]
async fn token_issuance_and_version_discovery_stay_open() {
    let f = fixture().await;
    let auth = json!({"auth":{"identity":{"methods":["password"],"password":{"user":{"name":"u","password":"anything"}}}}});
    let (st, _) = send(&f.app, "POST", "/identity/v3/auth/tokens", None, Some(auth)).await;
    assert_eq!(st, StatusCode::CREATED);
    for uri in [
        "/",
        "/identity",
        "/identity/v3",
        "/compute",
        "/compute/v2.1/",
        "/image",
        "/network",
        "/volume",
    ] {
        let (st, _) = send(&f.app, "GET", uri, None, None).await;
        assert_eq!(st, StatusCode::OK, "GET {uri}");
    }
}

#[tokio::test]
async fn read_routes_keep_current_auth_behaviour() {
    let f = fixture().await;
    // Open without a token (unchanged).
    for uri in [
        "/identity/v3/projects",
        "/identity/v3/users",
        "/compute/v2.1/flavors",
        "/compute/v2.1/flavors/detail",
        "/compute/v2.1/flavors/1",
        "/compute/v2.1/servers",
        "/compute/v2.1/servers/detail",
        "/image/v2/images",
        "/image/v2/images/aaaaaaaa-aaaa-aaaa-aaaa-aaaaaaaaaaa1",
        "/network/v2.0/networks",
        "/network/v2.0/subnets",
        "/network/v2.0/ports",
        "/volume/v3/proj-admin/volumes",
        "/volume/v3/proj-admin/volumes/detail",
    ] {
        let (st, _) = send(&f.app, "GET", uri, None, None).await;
        assert_eq!(st, StatusCode::OK, "GET {uri} without token");
    }
    let (st, _) = send(
        &f.app,
        "GET",
        &fill("/compute/v2.1/servers/{s}", &f),
        None,
        None,
    )
    .await;
    assert_eq!(st, StatusCode::OK);
    let (st, _) = send(
        &f.app,
        "GET",
        &fill("/volume/v3/proj-admin/volumes/{v}", &f),
        None,
        None,
    )
    .await;
    assert_eq!(st, StatusCode::OK);

    // Token-gated reads: catalog and token validation.
    let (st, v) = send(&f.app, "GET", "/identity/v3/auth/catalog", None, None).await;
    assert_eq!(
        (st, v),
        (StatusCode::UNAUTHORIZED, json!({"error": {"code": 401}}))
    );
    let (st, _) = send(
        &f.app,
        "GET",
        "/identity/v3/auth/catalog",
        Some("tok-nope"),
        None,
    )
    .await;
    assert_eq!(st, StatusCode::UNAUTHORIZED);
    let (st, _) = send(
        &f.app,
        "GET",
        "/identity/v3/auth/catalog",
        Some(&f.expired),
        None,
    )
    .await;
    assert_eq!(st, StatusCode::UNAUTHORIZED);
    let (st, _) = send(
        &f.app,
        "GET",
        "/identity/v3/auth/catalog",
        Some(&f.live),
        None,
    )
    .await;
    assert_eq!(st, StatusCode::OK);

    // GET /v3/auth/tokens validates X-Subject-Token (or X-Auth-Token).
    for (tok, want) in [
        ("", StatusCode::UNAUTHORIZED),
        ("tok-nope", StatusCode::UNAUTHORIZED),
        (f.expired.as_str(), StatusCode::UNAUTHORIZED),
        (f.live.as_str(), StatusCode::OK),
    ] {
        let req = Request::builder()
            .uri("/identity/v3/auth/tokens")
            .header("x-subject-token", tok)
            .body(Body::empty())
            .unwrap();
        let res = f.app.clone().oneshot(req).await.unwrap();
        assert_eq!(res.status(), want, "validate {tok:?}");
    }
}
