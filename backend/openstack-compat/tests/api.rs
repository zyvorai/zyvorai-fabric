// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: LicenseRef-Zyvor-Production-2.0

use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use http_body_util::BodyExt;
use openstack_compat::router;
use serde_json::{json, Value};
use tower::ServiceExt;

async fn call(
    app: axum::Router,
    method: &str,
    uri: &str,
    token: Option<&str>,
    body: Option<Value>,
) -> (StatusCode, Value) {
    let mut builder = Request::builder().method(method).uri(uri);
    if let Some(t) = token {
        builder = builder.header("x-auth-token", t);
    }
    if body.is_some() {
        builder = builder.header("content-type", "application/json");
    }
    let req = builder
        .body(
            body.map(|v| Body::from(v.to_string()))
                .unwrap_or_else(Body::empty),
        )
        .unwrap();
    let res = app.oneshot(req).await.unwrap();
    let status = res.status();
    let bytes = res.into_body().collect().await.unwrap().to_bytes();
    let json = if bytes.is_empty() {
        Value::Null
    } else {
        serde_json::from_slice(&bytes)
            .unwrap_or(Value::String(String::from_utf8_lossy(&bytes).into()))
    };
    (status, json)
}

#[tokio::test]
async fn keystone_issues_token_and_catalog() {
    let app = router("http://127.0.0.1:8080");
    let req = Request::builder()
        .method("POST")
        .uri("/identity/v3/auth/tokens")
        .header("content-type", "application/json")
        .body(Body::from(
            json!({"auth":{"identity":{"methods":["password"],"password":{"user":{"name":"admin","password":"secret"}}},"scope":{"project":{"name":"admin"}}}}).to_string(),
        ))
        .unwrap();
    let res = app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::CREATED);
    let token = res
        .headers()
        .get("x-subject-token")
        .unwrap()
        .to_str()
        .unwrap()
        .to_string();
    let body = res.into_body().collect().await.unwrap().to_bytes();
    let v: Value = serde_json::from_slice(&body).unwrap();
    assert!(v["token"]["catalog"].as_array().unwrap().len() >= 5);

    let (st, cat) = call(app, "GET", "/identity/v3/auth/catalog", Some(&token), None).await;
    assert_eq!(st, StatusCode::OK);
    assert!(cat["catalog"]
        .as_array()
        .unwrap()
        .iter()
        .any(|s| s["type"] == "compute"));
}

async fn login(app: &axum::Router) -> String {
    let req = Request::builder()
        .method("POST")
        .uri("/identity/v3/auth/tokens")
        .header("content-type", "application/json")
        .body(Body::from(
            json!({"auth":{"identity":{"methods":["password"],"password":{"user":{"name":"admin","password":"x"}}},"scope":{"project":{"name":"admin"}}}}).to_string(),
        ))
        .unwrap();
    let res = app.clone().oneshot(req).await.unwrap();
    res.headers()
        .get("x-subject-token")
        .unwrap()
        .to_str()
        .unwrap()
        .to_string()
}

#[tokio::test]
async fn nova_server_lifecycle_like_openstack() {
    let app = router("http://127.0.0.1:8080");
    let tok = login(&app).await;
    let (st, flavors) = call(
        app.clone(),
        "GET",
        "/compute/v2.1/flavors/detail",
        None,
        None,
    )
    .await;
    assert_eq!(st, StatusCode::OK);
    assert!(flavors["flavors"]
        .as_array()
        .unwrap()
        .iter()
        .any(|f| f["name"] == "m1.tiny"));

    let (st, created) = call(
        app.clone(),
        "POST",
        "/compute/v2.1/servers",
        Some(&tok),
        Some(json!({"server":{"name":"web-1","flavorRef":"1","imageRef":"cirros"}})),
    )
    .await;
    assert_eq!(st, StatusCode::ACCEPTED);
    let id = created["server"]["id"].as_str().unwrap().to_string();
    assert_eq!(created["server"]["status"], "ACTIVE");

    let (st, got) = call(
        app.clone(),
        "GET",
        &format!("/compute/v2.1/servers/{id}"),
        None,
        None,
    )
    .await;
    assert_eq!(st, StatusCode::OK);
    assert_eq!(got["server"]["name"], "web-1");

    let (st, _) = call(
        app.clone(),
        "POST",
        &format!("/compute/v2.1/servers/{id}/action"),
        Some(&tok),
        Some(json!({"os-stop": null})),
    )
    .await;
    assert_eq!(st, StatusCode::ACCEPTED);
    let (_, got) = call(
        app.clone(),
        "GET",
        &format!("/compute/v2.1/servers/{id}"),
        None,
        None,
    )
    .await;
    assert_eq!(got["server"]["status"], "SHUTOFF");

    let (st, _) = call(
        app.clone(),
        "DELETE",
        &format!("/compute/v2.1/servers/{id}"),
        Some(&tok),
        None,
    )
    .await;
    assert_eq!(st, StatusCode::NO_CONTENT);
}

#[tokio::test]
async fn glance_neutron_cinder_roundtrip() {
    let app = router("http://127.0.0.1:8080");
    let tok = login(&app).await;
    let (st, imgs) = call(app.clone(), "GET", "/image/v2/images", None, None).await;
    assert_eq!(st, StatusCode::OK);
    assert!(imgs["images"]
        .as_array()
        .unwrap()
        .iter()
        .any(|i| i["name"] == "cirros"));

    let (st, net) = call(
        app.clone(),
        "POST",
        "/network/v2.0/networks",
        Some(&tok),
        Some(json!({"network":{"name":"lab"}})),
    )
    .await;
    assert_eq!(st, StatusCode::CREATED);
    let net_id = net["network"]["id"].as_str().unwrap();

    let (st, _) = call(
        app.clone(),
        "POST",
        "/network/v2.0/subnets",
        Some(&tok),
        Some(json!({"subnet":{"name":"lab-sub","network_id": net_id, "cidr":"192.168.10.0/24"}})),
    )
    .await;
    assert_eq!(st, StatusCode::CREATED);

    let (st, vol) = call(
        app.clone(),
        "POST",
        "/volume/v3/proj-admin/volumes",
        Some(&tok),
        Some(json!({"volume":{"name":"data","size":10}})),
    )
    .await;
    assert_eq!(st, StatusCode::ACCEPTED);
    let vid = vol["volume"]["id"].as_str().unwrap();

    let (st, created) = call(
        app.clone(),
        "POST",
        "/compute/v2.1/servers",
        Some(&tok),
        Some(json!({"server":{"name":"db","flavorRef":"2","imageRef":"cirros"}})),
    )
    .await;
    let sid = created["server"]["id"].as_str().unwrap();
    let (st2, _) = call(
        app,
        "POST",
        &format!("/volume/v3/proj-admin/volumes/{vid}/action"),
        Some(&tok),
        Some(json!({"os-attach":{"instance_uuid": sid}})),
    )
    .await;
    assert_eq!(st, StatusCode::ACCEPTED);
    assert_eq!(st2, StatusCode::ACCEPTED);
}
