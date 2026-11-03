// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: Apache-2.0

//! Two users and an operator against the real router: nothing may cross between users.

use crate::{
    app::public_router,
    audit::AuditPhase,
    egress::ask_tests::state_and_session_cfg,
    goals::ArtifactRecord,
    model::{ApprovalKind, ApprovalRecord, ApprovalStatus, SessionRecord},
    AppState,
};
use axum::{
    body::Body,
    http::{Request, StatusCode},
    Router,
};
use chrono::Utc;
use serde_json::{json, Value};
use std::sync::Arc;
use tower::ServiceExt;
use uuid::Uuid;

/// The operator token, built at run time (see `crate::fixture`).
fn op() -> String {
    crate::fixture::text("operator-token")
}

struct World {
    app: Router,
    state: Arc<AppState>,
    ana: Fixture,
    ben: Fixture,
    orphan_artifact: Uuid,
}

struct Fixture {
    session: Uuid,
    approval: Uuid,
    artifact: Uuid,
    token: String,
}

async fn call(
    app: &Router,
    method: &str,
    path: &str,
    token: Option<&str>,
    body: Option<Value>,
) -> (StatusCode, Value) {
    let mut b = Request::builder().method(method).uri(path);
    if let Some(t) = token {
        b = b.header("authorization", format!("Bearer {t}"));
    }
    let req = match body {
        Some(v) => b
            .header("content-type", "application/json")
            .body(Body::from(v.to_string())),
        None => b.body(Body::empty()),
    }
    .unwrap();
    let resp = app.clone().oneshot(req).await.unwrap();
    let status = resp.status();
    let bytes = axum::body::to_bytes(resp.into_body(), 1 << 20)
        .await
        .unwrap();
    (
        status,
        serde_json::from_slice(&bytes).unwrap_or(Value::Null),
    )
}

async fn fixture(state: &Arc<AppState>, base: &SessionRecord, app: &Router, user: &str) -> Fixture {
    let now = Utc::now();
    let mut s = base.clone();
    s.id = Uuid::new_v4();
    s.user_id = Some(user.into());
    state.store.save_session(s.clone()).await.unwrap();
    let approval = ApprovalRecord {
        id: Uuid::new_v4(),
        session_id: s.id,
        kind: ApprovalKind::Send,
        subject: Some(format!("{user}.example")),
        planned_action: Some(json!({"body_sha256": "abc"})),
        prompt: format!("{user} wants to send"),
        status: ApprovalStatus::Pending,
        comment: None,
        created_at: now,
        decided_at: None,
        source_seq: None,
        grant_scope: None,
        preview: None,
        broker_held: true,
    };
    state.store.save_approval(approval.clone()).await.unwrap();
    let artifact = ArtifactRecord {
        id: Uuid::new_v4(),
        kind: "report".into(),
        title: format!("{user}-report"),
        body: format!("private notes of {user}\nline two"),
        content_type: None,
        goal_id: None,
        session_id: Some(s.id),
        agent: None,
        metadata: Value::Null,
        created_at: now,
        expires_at: None,
    };
    state.store.save_artifact(artifact.clone()).await.unwrap();
    state
        .store
        .audit
        .append(
            Some(s.id),
            AuditPhase::Performed,
            "demo.done",
            Some(user.into()),
            json!({"who": user}),
        )
        .await
        .unwrap();
    let (st, v) = call(
        app,
        "POST",
        "/v1/user-tokens",
        Some(op().as_str()),
        Some(json!({"user_id": user, "ttl_seconds": 600})),
    )
    .await;
    assert_eq!(st, StatusCode::CREATED, "{v}");
    Fixture {
        session: s.id,
        approval: approval.id,
        artifact: artifact.id,
        token: v["token"].as_str().unwrap().to_string(),
    }
}

async fn world() -> World {
    let (state, base) = state_and_session_cfg(|c| c.api_token = Some(op())).await;
    let app = public_router(state.clone());
    let ana = fixture(&state, &base, &app, "ana").await;
    let ben = fixture(&state, &base, &app, "ben").await;
    // An artifact made outside any session belongs to no user.
    let orphan = ArtifactRecord {
        id: Uuid::new_v4(),
        kind: "note".into(),
        title: "operator-note".into(),
        body: "x".into(),
        content_type: None,
        goal_id: None,
        session_id: None,
        agent: None,
        metadata: Value::Null,
        created_at: Utc::now(),
        expires_at: None,
    };
    state.store.save_artifact(orphan.clone()).await.unwrap();
    World {
        app,
        state,
        ana,
        ben,
        orphan_artifact: orphan.id,
    }
}

/// A POST that says it is JSON but has no body, as a client that always sets the header sends for "no options".
async fn post_empty_json(app: &Router, path: &str, token: &str) -> (StatusCode, String) {
    let req = Request::builder()
        .method("POST")
        .uri(path)
        .header("authorization", format!("Bearer {token}"))
        .header("content-type", "application/json")
        .body(Body::empty())
        .unwrap();
    let resp = app.clone().oneshot(req).await.unwrap();
    let status = resp.status();
    let bytes = axum::body::to_bytes(resp.into_body(), 1 << 20)
        .await
        .unwrap();
    (status, String::from_utf8_lossy(&bytes).into_owned())
}

fn ids(v: &Value, key: &str) -> Vec<String> {
    v["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|i| i[key].as_str().unwrap_or_default().to_string())
        .collect()
}

#[tokio::test]
async fn a_user_sees_only_their_own_sessions_approvals_artifacts_and_audit() {
    let w = world().await;
    let t = Some(w.ana.token.as_str());

    let (_, v) = call(&w.app, "GET", "/v1/sessions", t, None).await;
    assert_eq!(ids(&v, "id"), vec![w.ana.session.to_string()]);
    // Asking for someone else's list by parameter changes nothing.
    let (_, v) = call(&w.app, "GET", "/v1/sessions?user_id=ben", t, None).await;
    assert_eq!(ids(&v, "id"), vec![w.ana.session.to_string()]);

    let (_, v) = call(&w.app, "GET", "/v1/approvals", t, None).await;
    assert_eq!(ids(&v, "id"), vec![w.ana.approval.to_string()]);

    let (_, v) = call(&w.app, "GET", "/v1/artifacts", t, None).await;
    assert_eq!(
        ids(&v, "id"),
        vec![w.ana.artifact.to_string()],
        "no other user's and no session-less artifact"
    );

    let (_, v) = call(&w.app, "GET", "/v1/audit", t, None).await;
    let rows = v["items"].as_array().unwrap();
    assert!(
        !rows.is_empty()
            && rows
                .iter()
                .all(|r| r["session_id"] == w.ana.session.to_string())
    );
    assert!(
        !v.to_string().contains("\"ben\""),
        "another user's rows leaked: {v}"
    );
    assert!(
        v["chain"].get("entries").is_none(),
        "the journal size is not shown to users"
    );
    assert_eq!(v["chain"]["chain_ok"], true);
}

#[tokio::test]
async fn a_user_cannot_read_or_touch_anything_of_another_user() {
    let w = world().await;
    let t = Some(w.ana.token.as_str());
    for path in [
        format!("/v1/sessions/{}", w.ben.session),
        format!("/v1/sessions/{}/cockpit", w.ben.session),
        format!("/v1/sessions/{}/events", w.ben.session),
        format!("/v1/sessions/{}/browser/view", w.ben.session),
        format!("/v1/artifacts/{}", w.ben.artifact),
        format!("/v1/artifacts/{}", w.orphan_artifact),
        format!("/v1/artifacts/{}/diff/{}", w.ana.artifact, w.ben.artifact),
        format!("/v1/artifacts/{}/diff/{}", w.ben.artifact, w.ana.artifact),
    ] {
        let (st, _) = call(&w.app, "GET", &path, t, None).await;
        assert_eq!(st, StatusCode::NOT_FOUND, "GET {path}");
    }
    for path in [
        format!("/v1/sessions/{}/cancel", w.ben.session),
        format!("/v1/sessions/{}/steer", w.ben.session),
    ] {
        let (st, _) = call(&w.app, "POST", &path, t, Some(json!({}))).await;
        assert_eq!(st, StatusCode::NOT_FOUND, "POST {path}");
    }
    let (st, _) = call(
        &w.app,
        "DELETE",
        &format!("/v1/sessions/{}", w.ben.session),
        t,
        None,
    )
    .await;
    assert_eq!(st, StatusCode::NOT_FOUND);
    // Deciding another user's approval is refused and the approval is untouched.
    let path = format!("/v1/approvals/{}", w.ben.approval);
    let (st, _) = call(
        &w.app,
        "POST",
        &path,
        t,
        Some(json!({"decision": "approved"})),
    )
    .await;
    assert_eq!(st, StatusCode::NOT_FOUND);
    assert_eq!(
        w.state
            .store
            .get_approval(w.ben.approval)
            .await
            .unwrap()
            .status,
        ApprovalStatus::Pending
    );
    // Their own works.
    let (st, _) = call(
        &w.app,
        "POST",
        &format!("/v1/approvals/{}", w.ana.approval),
        t,
        Some(json!({"decision": "denied"})),
    )
    .await;
    assert_eq!(st, StatusCode::OK);
    assert_eq!(
        w.state
            .store
            .get_approval(w.ana.approval)
            .await
            .unwrap()
            .status,
        ApprovalStatus::Denied
    );
}

#[tokio::test]
async fn operator_routes_are_closed_to_user_tokens() {
    let w = world().await;
    let t = Some(w.ana.token.as_str());
    for (m, p) in [
        ("GET", "/v1/keep/status"),
        ("GET", "/v1/merion/status"),
        ("GET", "/v1/agents"),
        ("POST", "/v1/agents"),
        ("POST", "/v1/user-tokens"),
        ("POST", "/v1/users/ben/revoke-tokens"),
        ("GET", "/v1/triggers"),
        ("POST", "/v1/triggers"),
        ("GET", "/v1/model-grants"),
        ("GET", "/v1/export/audit"),
        ("POST", "/v1/export-tokens"),
        ("GET", "/v1/vault/status"),
        (
            "POST",
            "/v1/goals/00000000-0000-4000-8000-000000000000/advance",
        ),
        ("POST", "/v1/artifacts"),
        ("POST", "/v1/approvals"),
        ("POST", "/v1/demos"),
        ("DELETE", "/v1/demos/csv-clean"),
        ("GET", "/v1/workstations/a/b"),
        ("POST", "/mcp"),
    ] {
        let (st, _) = call(&w.app, m, p, t, Some(json!({}))).await;
        assert_eq!(st, StatusCode::FORBIDDEN, "{m} {p}");
    }
}

#[tokio::test]
async fn tokens_are_header_only_scoped_and_revocable() {
    let w = world().await;
    // Never in a query string.
    let (st, _) = call(
        &w.app,
        "GET",
        &format!("/v1/sessions?token={}", w.ana.token),
        None,
        None,
    )
    .await;
    assert_eq!(st, StatusCode::UNAUTHORIZED);
    // No token, a garbage token, and a token from another shard's key.
    for bad in [None, Some("kut1.garbage.00"), Some("nope")] {
        let (st, _) = call(&w.app, "GET", "/v1/sessions", bad, None).await;
        assert_eq!(st, StatusCode::UNAUTHORIZED, "{bad:?}");
    }
    // A read-only token cannot decide an approval or start a run.
    let (_, v) = call(
        &w.app,
        "POST",
        "/v1/user-tokens",
        Some(op().as_str()),
        Some(json!({"user_id": "ana", "scopes": ["read"]})),
    )
    .await;
    let ro = v["token"].as_str().unwrap().to_string();
    let (st, v) = call(
        &w.app,
        "POST",
        &format!("/v1/approvals/{}", w.ana.approval),
        Some(&ro),
        Some(json!({"decision": "approved"})),
    )
    .await;
    assert_eq!(st, StatusCode::FORBIDDEN, "{v}");
    let (st, _) = call(
        &w.app,
        "POST",
        "/v1/sessions",
        Some(&ro),
        Some(json!({"agent": "x"})),
    )
    .await;
    assert_eq!(st, StatusCode::FORBIDDEN);
    let (st, _) = call(&w.app, "GET", "/v1/sessions", Some(&ro), None).await;
    assert_eq!(st, StatusCode::OK);
    // A user token cannot start a session for someone else.
    let (st, _) = call(
        &w.app,
        "POST",
        "/v1/sessions",
        Some(&w.ana.token),
        Some(json!({"agent": "x", "user_id": "ben"})),
    )
    .await;
    assert_eq!(st, StatusCode::FORBIDDEN);
    // Unknown scope and bad user are refused at mint time.
    let (st, _) = call(
        &w.app,
        "POST",
        "/v1/user-tokens",
        Some(op().as_str()),
        Some(json!({"user_id": "ana", "scopes": ["root"]})),
    )
    .await;
    assert_eq!(st, StatusCode::BAD_REQUEST);
    let (st, _) = call(
        &w.app,
        "POST",
        "/v1/user-tokens",
        Some(op().as_str()),
        Some(json!({"user_id": "Ana Silva"})),
    )
    .await;
    assert_eq!(st, StatusCode::BAD_REQUEST);

    // Revoking cuts off ana, not ben.
    let (st, _) = call(
        &w.app,
        "POST",
        "/v1/users/ana/revoke-tokens",
        Some(op().as_str()),
        None,
    )
    .await;
    assert_eq!(st, StatusCode::OK);
    let (st, _) = call(&w.app, "GET", "/v1/sessions", Some(&w.ana.token), None).await;
    assert_eq!(
        st,
        StatusCode::UNAUTHORIZED,
        "the old token must stop working"
    );
    let (st, _) = call(&w.app, "GET", "/v1/sessions", Some(&w.ben.token), None).await;
    assert_eq!(st, StatusCode::OK);
    tokio::time::sleep(std::time::Duration::from_millis(2100)).await;
    let (_, v) = call(
        &w.app,
        "POST",
        "/v1/user-tokens",
        Some(op().as_str()),
        Some(json!({"user_id": "ana"})),
    )
    .await;
    let fresh = v["token"].as_str().unwrap().to_string();
    let (st, _) = call(&w.app, "GET", "/v1/sessions", Some(&fresh), None).await;
    assert_eq!(
        st,
        StatusCode::OK,
        "a token minted after the revocation works"
    );
}

#[tokio::test]
async fn the_operator_still_sees_everything() {
    let w = world().await;
    let token = op();
    let t = Some(token.as_str());
    let (_, v) = call(&w.app, "GET", "/v1/approvals", t, None).await;
    assert_eq!(ids(&v, "id").len(), 2);
    let (_, v) = call(&w.app, "GET", "/v1/artifacts", t, None).await;
    assert_eq!(ids(&v, "id").len(), 3);
    let (_, v) = call(
        &w.app,
        "GET",
        &format!("/v1/artifacts/{}", w.orphan_artifact),
        t,
        None,
    )
    .await;
    assert_eq!(v["title"], "operator-note");
    let (_, v) = call(&w.app, "GET", "/v1/audit", t, None).await;
    assert!(v["chain"]["entries"].as_u64().unwrap() >= 2);
}

#[tokio::test]
async fn usage_and_inbox_are_per_user() {
    let w = world().await;
    let t = Some(w.ana.token.as_str());
    let (st, v) = call(&w.app, "GET", "/v1/usage?user_id=ben", t, None).await;
    assert_eq!(st, StatusCode::OK);
    assert_eq!(
        v["usage"]["user_id"], "ana",
        "a user always reads their own usage"
    );
    assert_eq!(v["usage"]["runs"], 1);
    assert_eq!(v["usage"]["artifacts"], 1);
    assert!(v["usage"]["artifact_bytes"].as_u64().unwrap() > 0);

    let (_, v) = call(&w.app, "GET", "/v1/inbox", t, None).await;
    assert_eq!(v["user_id"], "ana");
    assert_eq!(v["pending_approvals"].as_array().unwrap().len(), 1);
    assert_eq!(v["pending_approvals"][0]["id"], w.ana.approval.to_string());
    assert_eq!(v["recent_runs"].as_array().unwrap().len(), 1);
    assert_eq!(v["recent_runs"][0]["artifacts"][0]["title"], "ana-report");
    assert!(!v.to_string().contains("ben"));

    // The operator names the user; without one it is a 400.
    let (st, _) = call(&w.app, "GET", "/v1/usage", Some(op().as_str()), None).await;
    assert_eq!(st, StatusCode::BAD_REQUEST);
    let (_, v) = call(
        &w.app,
        "GET",
        "/v1/usage?user_id=ben",
        Some(op().as_str()),
        None,
    )
    .await;
    assert_eq!(v["usage"]["user_id"], "ben");
}

#[tokio::test]
async fn quotas_return_429_when_the_operator_sets_them() {
    let w = world().await;
    // ana already has 1 session and 1 artifact. Limits are read from the environment, so this
    // test uses `check_run_quota` directly with explicit limits (no process-wide env changes).
    let limits = crate::usage::Limits {
        max_runs_per_day: Some(1),
        ..Default::default()
    };
    let err = crate::usage::check_run_quota(&w.state, "ana", limits)
        .await
        .unwrap_err();
    assert_eq!(err.status(), StatusCode::TOO_MANY_REQUESTS);
    assert!(err.message().contains("1 of 1 runs"), "{}", err.message());
    let roomy = crate::usage::Limits {
        max_runs_per_day: Some(5),
        max_artifacts: Some(5),
        ..Default::default()
    };
    assert!(crate::usage::check_run_quota(&w.state, "ana", roomy)
        .await
        .is_ok());
    let art = crate::usage::Limits {
        max_artifacts: Some(1),
        ..Default::default()
    };
    assert!(crate::usage::check_run_quota(&w.state, "ben", art)
        .await
        .is_err());
    assert!(crate::usage::check_run_quota(&w.state, "cy-no-data", art)
        .await
        .is_ok());
    let none = crate::usage::Limits::default();
    assert!(crate::usage::check_run_quota(&w.state, "ana", none)
        .await
        .is_ok());
    let model = crate::usage::Limits {
        max_model_calls_per_day: Some(1),
        ..Default::default()
    };
    assert!(
        crate::usage::check_model_quota(&w.state, "ana", model)
            .await
            .is_ok(),
        "no model calls yet"
    );
}

// ---- phone-signed decisions -------------------------------------------------------------------

use base64::Engine as _;
use p256::ecdsa::signature::Signer as _;
use p256::pkcs8::EncodePublicKey as _;

fn os_rng() -> impl ed25519_dalek::ed25519::signature::rand_core::CryptoRngCore {
    ed25519_dalek::ed25519::signature::rand_core::OsRng
}

fn b64(bytes: &[u8]) -> String {
    base64::engine::general_purpose::STANDARD.encode(bytes)
}

/// An enrolled phone: the private key stays here, the public key goes to the runtime.
struct Phone {
    id: &'static str,
    sk: p256::ecdsa::SigningKey,
}

impl Phone {
    fn new(id: &'static str) -> Self {
        Self {
            id,
            sk: p256::ecdsa::SigningKey::random(&mut os_rng()),
        }
    }

    fn enrol_body(&self) -> Value {
        let spki = self.sk.verifying_key().to_public_key_der().unwrap();
        json!({"device_id": self.id, "alg": "p256", "public_key": b64(spki.as_bytes()),
               "push": {"kind": "fcm", "token": "device-push-token"}})
    }

    /// Sign the payload the server tells the phone to sign.
    fn sign(&self, approval: &ApprovalRecord, decision: ApprovalStatus) -> String {
        let key = crate::authz::signing_key(Some(op().as_str()), None).unwrap();
        let payload = crate::devices::signing_payload(
            approval,
            decision,
            &crate::devices::challenge(&key, approval),
        );
        let sig: p256::ecdsa::Signature = self.sk.sign(payload.as_bytes());
        b64(sig.to_der().as_bytes())
    }
}

async fn decide(
    w: &World,
    who: &str,
    id: Uuid,
    decision: &str,
    phone: Option<(&str, String)>,
) -> (StatusCode, Value) {
    let mut body = json!({ "decision": decision });
    if let Some((device, sig)) = phone {
        body["device_id"] = json!(device);
        body["signature"] = json!(sig);
    }
    call(
        &w.app,
        "POST",
        &format!("/v1/approvals/{id}"),
        Some(who),
        Some(body),
    )
    .await
}

#[tokio::test]
async fn a_phone_signed_decision_is_accepted_and_recorded() {
    let w = world().await;
    let phone = Phone::new("ana-phone");
    let (st, v) = call(
        &w.app,
        "POST",
        "/v1/users/ana/devices",
        Some(op().as_str()),
        Some(phone.enrol_body()),
    )
    .await;
    assert_eq!(st, StatusCode::CREATED, "{v}");
    let a = w.state.store.get_approval(w.ana.approval).await.unwrap();
    let sig = phone.sign(&a, ApprovalStatus::Approved);
    let (st, v) = decide(&w, &w.ana.token, a.id, "approved", Some((phone.id, sig))).await;
    assert_eq!(st, StatusCode::OK, "{v}");
    assert_eq!(
        w.state.store.get_approval(a.id).await.unwrap().status,
        ApprovalStatus::Approved
    );
    let rows = w
        .state
        .store
        .audit
        .list(Some(a.session_id), 100)
        .await
        .unwrap();
    assert!(rows.iter().any(|r| r.action == "approval.device_signature"
        && r.phase == AuditPhase::Performed
        && r.detail["device_id"] == "ana-phone"));
    assert!(
        rows.iter()
            .any(|r| r.action == "approval.send" && r.detail["device_id"] == "ana-phone"),
        "the decision row names the device"
    );
}

#[tokio::test]
async fn a_forged_flipped_or_borrowed_signature_is_refused_and_changes_nothing() {
    let w = world().await;
    let phone = Phone::new("ana-phone");
    call(
        &w.app,
        "POST",
        "/v1/users/ana/devices",
        Some(op().as_str()),
        Some(phone.enrol_body()),
    )
    .await;
    // Ben has a phone too.
    let bens = Phone::new("ben-phone");
    call(
        &w.app,
        "POST",
        "/v1/users/ben/devices",
        Some(op().as_str()),
        Some(bens.enrol_body()),
    )
    .await;
    let a = w.state.store.get_approval(w.ana.approval).await.unwrap();
    let intruder = Phone::new("ana-phone");

    for (why, device, sig, decision) in [
        (
            "another key under ana's device id",
            "ana-phone",
            intruder.sign(&a, ApprovalStatus::Approved),
            "approved",
        ),
        (
            "signed approved, sent denied",
            "ana-phone",
            phone.sign(&a, ApprovalStatus::Approved),
            "denied",
        ),
        (
            "ben's phone on ana's approval",
            "ben-phone",
            bens.sign(&a, ApprovalStatus::Approved),
            "approved",
        ),
        (
            "a device nobody enrolled",
            "ghost",
            phone.sign(&a, ApprovalStatus::Approved),
            "approved",
        ),
        ("garbage", "ana-phone", "AAAA".to_string(), "approved"),
    ] {
        let (st, v) = decide(&w, &w.ana.token, a.id, decision, Some((device, sig))).await;
        assert_eq!(st, StatusCode::FORBIDDEN, "{why}: {v}");
        assert_eq!(
            w.state.store.get_approval(a.id).await.unwrap().status,
            ApprovalStatus::Pending,
            "{why}"
        );
    }
    // A signature without a device, or a device without a signature, is a plain 400.
    let (st, _) = call(
        &w.app,
        "POST",
        &format!("/v1/approvals/{}", a.id),
        Some(&w.ana.token),
        Some(json!({"decision": "approved", "device_id": "ana-phone"})),
    )
    .await;
    assert_eq!(st, StatusCode::BAD_REQUEST);
    let rows = w
        .state
        .store
        .audit
        .list(Some(a.session_id), 100)
        .await
        .unwrap();
    assert!(
        rows.iter()
            .filter(|r| r.action == "approval.device_signature" && r.phase == AuditPhase::Failed)
            .count()
            >= 4
    );
}

#[tokio::test]
async fn a_signature_after_the_window_is_refused() {
    let w = world().await;
    let phone = Phone::new("ana-phone");
    call(
        &w.app,
        "POST",
        "/v1/users/ana/devices",
        Some(op().as_str()),
        Some(phone.enrol_body()),
    )
    .await;
    let mut a = w.state.store.get_approval(w.ana.approval).await.unwrap();
    a.created_at = Utc::now() - chrono::Duration::seconds(crate::devices::sign_ttl_seconds() + 60);
    w.state.store.save_approval(a.clone()).await.unwrap();
    let (st, v) = decide(
        &w,
        &w.ana.token,
        a.id,
        "approved",
        Some((phone.id, phone.sign(&a, ApprovalStatus::Approved))),
    )
    .await;
    assert_eq!(st, StatusCode::FORBIDDEN);
    assert!(v["error"].as_str().unwrap().contains("window"), "{v}");
}

#[tokio::test]
async fn users_cannot_enrol_devices_but_can_list_their_own() {
    let w = world().await;
    let phone = Phone::new("evil");
    // A stolen user token must not be able to add its own key.
    for path in ["/v1/users/ana/devices", "/v1/users/ben/devices"] {
        let (st, _) = call(
            &w.app,
            "POST",
            path,
            Some(&w.ana.token),
            Some(phone.enrol_body()),
        )
        .await;
        assert_eq!(st, StatusCode::FORBIDDEN, "{path}");
    }
    let (st, _) = call(
        &w.app,
        "DELETE",
        "/v1/users/ana/devices/x",
        Some(&w.ana.token),
        None,
    )
    .await;
    assert_eq!(st, StatusCode::FORBIDDEN);
    call(
        &w.app,
        "POST",
        "/v1/users/ana/devices",
        Some(op().as_str()),
        Some(Phone::new("ana-phone").enrol_body()),
    )
    .await;
    call(
        &w.app,
        "POST",
        "/v1/users/ben/devices",
        Some(op().as_str()),
        Some(Phone::new("ben-phone").enrol_body()),
    )
    .await;
    let (st, v) = call(
        &w.app,
        "GET",
        "/v1/users/ana/devices",
        Some(&w.ana.token),
        None,
    )
    .await;
    assert_eq!(st, StatusCode::OK);
    assert_eq!(ids(&v, "device_id"), vec!["ana-phone"]);
    let (st, _) = call(
        &w.app,
        "GET",
        "/v1/users/ben/devices",
        Some(&w.ana.token),
        None,
    )
    .await;
    assert_eq!(
        st,
        StatusCode::NOT_FOUND,
        "another user's device list is not visible"
    );
    // The operator manages them, and a removed phone stops signing.
    let (st, _) = call(
        &w.app,
        "DELETE",
        "/v1/users/ana/devices/ana-phone",
        Some(op().as_str()),
        None,
    )
    .await;
    assert_eq!(st, StatusCode::NO_CONTENT);
    let (_, v) = call(
        &w.app,
        "GET",
        "/v1/users/ana/devices",
        Some(op().as_str()),
        None,
    )
    .await;
    assert!(v["items"].as_array().unwrap().is_empty());
    // Bad keys are refused at enrolment.
    let (st, _) = call(
        &w.app,
        "POST",
        "/v1/users/ana/devices",
        Some(op().as_str()),
        Some(json!({"device_id": "d", "alg": "p256", "public_key": "AAAA"})),
    )
    .await;
    assert_eq!(st, StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn the_inbox_carries_what_a_phone_needs_to_sign() {
    let w = world().await;
    let (_, v) = call(&w.app, "GET", "/v1/inbox", Some(&w.ana.token), None).await;
    let sign = &v["pending_approvals"][0]["sign"];
    assert_eq!(sign["format"], "merion-approval-v1");
    let a = w.state.store.get_approval(w.ana.approval).await.unwrap();
    let key = crate::authz::signing_key(Some(op().as_str()), None).unwrap();
    assert_eq!(sign["challenge"], crate::devices::challenge(&key, &a));
    assert_eq!(sign["action_sha256"], crate::devices::action_sha256(&a));
    assert_eq!(sign["expires_at"], crate::devices::expires_at(&a));
}

#[tokio::test]
async fn a_credential_can_require_the_phone_while_the_operator_can_still_decide() {
    let file = std::env::temp_dir().join(format!("zyvor-creds-{}.json", Uuid::new_v4()));
    std::fs::write(
        &file,
        json!({"mail": {"host": "mail.example", "header": "authorization", "env": "UNUSED_KEY",
                        "require_device_signature": true}})
        .to_string(),
    )
    .unwrap();
    let (state, base) = state_and_session_cfg(|c| {
        c.api_token = Some(op());
        c.credentials_file = Some(file);
    })
    .await;
    let app = public_router(state.clone());
    let ana = fixture(&state, &base, &app, "ana").await;
    let phone = Phone::new("ana-phone");
    call(
        &app,
        "POST",
        "/v1/users/ana/devices",
        Some(op().as_str()),
        Some(phone.enrol_body()),
    )
    .await;
    // Make ana's approval one that uses the credential.
    let mut a = state.store.get_approval(ana.approval).await.unwrap();
    a.planned_action = Some(json!({"credential": "mail", "method": "POST"}));
    state.store.save_approval(a.clone()).await.unwrap();
    let w = World {
        app: app.clone(),
        state: state.clone(),
        ana,
        ben: fixture(&state, &base, &app, "ben").await,
        orphan_artifact: Uuid::new_v4(),
    };

    let (st, v) = decide(&w, &w.ana.token, a.id, "approved", None).await;
    assert_eq!(st, StatusCode::FORBIDDEN, "{v}");
    assert!(
        v["error"].as_str().unwrap().contains("must be signed"),
        "{v}"
    );
    assert_eq!(
        state.store.get_approval(a.id).await.unwrap().status,
        ApprovalStatus::Pending
    );
    let (st, v) = decide(
        &w,
        &w.ana.token,
        a.id,
        "approved",
        Some((phone.id, phone.sign(&a, ApprovalStatus::Approved))),
    )
    .await;
    assert_eq!(st, StatusCode::OK, "{v}");

    // The operator (the gateway acting for the user, or an admin) may decide unsigned.
    let b = state.store.get_approval(w.ben.approval).await.unwrap();
    let mut b2 = b.clone();
    b2.planned_action = Some(json!({"credential": "mail"}));
    state.store.save_approval(b2).await.unwrap();
    let (st, _) = decide(&w, &op(), b.id, "denied", None).await;
    assert_eq!(st, StatusCode::OK);
}

#[tokio::test]
async fn threads_are_private_to_their_owner() {
    let w = world().await;
    let agent = w
        .state
        .store
        .get_session(w.ana.session)
        .await
        .unwrap()
        .agent;
    w.state
        .store
        .deploy_agent(crate::model::DeployAgentRequest {
            name: agent.clone(),
            bundle_base64: base64::Engine::encode(
                &base64::engine::general_purpose::STANDARD,
                "export default 1",
            ),
            manifest: crate::egress::ask_tests::manifest(crate::model::EgressMode::Deny, Some(30)),
        })
        .await
        .unwrap();
    let (ana, ben) = (Some(w.ana.token.as_str()), Some(w.ben.token.as_str()));
    let opt = op();
    let operator = Some(opt.as_str());

    let (st, thread) = call(
        &w.app,
        "POST",
        "/v1/threads",
        ana,
        Some(json!({"agent": agent, "title": "Trip to Goa", "client_thread_id": "chat-1"})),
    )
    .await;
    assert_eq!(st, StatusCode::CREATED, "{thread}");
    assert_eq!(thread["user_id"], "ana");
    let tid: Uuid = thread["id"].as_str().unwrap().parse().unwrap();
    w.state
        .store
        .threads
        .append(
            tid,
            crate::threads::Role::User,
            "book a table for two",
            None,
            None,
        )
        .await
        .unwrap();

    // the owner lists, reads and pages their own thread
    let (_, v) = call(&w.app, "GET", "/v1/threads", ana, None).await;
    assert_eq!(ids(&v, "id"), [tid.to_string()]);
    let (st, v) = call(
        &w.app,
        "GET",
        &format!("/v1/threads/{tid}/messages?after=0"),
        ana,
        None,
    )
    .await;
    assert_eq!(st, StatusCode::OK);
    assert_eq!(v["items"][0]["text"], "book a table for two");
    let (_, v) = call(
        &w.app,
        "GET",
        &format!("/v1/threads/{tid}/messages?after=1"),
        ana,
        None,
    )
    .await;
    assert!(v["items"].as_array().unwrap().is_empty());

    // another user sees nothing, and every probe looks like a thread that does not exist
    let (_, v) = call(&w.app, "GET", "/v1/threads", ben, None).await;
    assert!(ids(&v, "id").is_empty());
    for (method, path) in [
        ("GET", format!("/v1/threads/{tid}")),
        ("GET", format!("/v1/threads/{tid}/messages")),
        ("DELETE", format!("/v1/threads/{tid}")),
    ] {
        let (st, _) = call(&w.app, method, &path, ben, None).await;
        assert_eq!(st, StatusCode::NOT_FOUND, "{method} {path}");
    }
    assert!(
        w.state.store.threads.get(tid).await.is_some(),
        "another user's delete changed nothing"
    );

    // a user token may not create a thread for someone else; without the token nothing works
    let (st, _) = call(
        &w.app,
        "POST",
        "/v1/threads",
        ben,
        Some(json!({"agent": agent, "user_id": "ana"})),
    )
    .await;
    assert_eq!(st, StatusCode::BAD_REQUEST);
    let (st, _) = call(&w.app, "GET", "/v1/threads", None, None).await;
    assert_eq!(st, StatusCode::UNAUTHORIZED);

    // the operator sees every user's threads, must name the owner when creating one, and can read a thread
    let (st, _) = call(
        &w.app,
        "POST",
        "/v1/threads",
        operator,
        Some(json!({"agent": agent})),
    )
    .await;
    assert_eq!(st, StatusCode::BAD_REQUEST);
    let (st, _) = call(
        &w.app,
        "POST",
        "/v1/threads",
        operator,
        Some(json!({"agent": agent, "user_id": "ben"})),
    )
    .await;
    assert_eq!(st, StatusCode::CREATED);
    let (_, v) = call(&w.app, "GET", "/v1/threads", operator, None).await;
    assert_eq!(ids(&v, "user_id").len(), 2);
    let (st, _) = call(&w.app, "GET", &format!("/v1/threads/{tid}"), operator, None).await;
    assert_eq!(st, StatusCode::OK);

    // the list can be narrowed by agent, and by user for the operator; a user token cannot widen or redirect it
    let (_, v) = call(
        &w.app,
        "GET",
        &format!("/v1/threads?agent={agent}"),
        operator,
        None,
    )
    .await;
    assert_eq!(ids(&v, "user_id").len(), 2);
    let (_, v) = call(
        &w.app,
        "GET",
        "/v1/threads?agent=someone-else",
        operator,
        None,
    )
    .await;
    assert!(ids(&v, "id").is_empty());
    let (_, v) = call(&w.app, "GET", "/v1/threads?user_id=ben", operator, None).await;
    assert_eq!(ids(&v, "user_id"), ["ben"]);
    let (_, v) = call(&w.app, "GET", "/v1/threads?user_id=ben", ana, None).await;
    assert_eq!(
        ids(&v, "user_id"),
        ["ana"],
        "a user token ignores user_id and stays on its own threads"
    );

    // an unknown agent is refused
    let (st, _) = call(
        &w.app,
        "POST",
        "/v1/threads",
        ana,
        Some(json!({"agent": "no-such-agent"})),
    )
    .await;
    assert_eq!(st, StatusCode::NOT_FOUND);

    // the owner forgets the thread: gone, messages gone, and the journal records that it happened (not what was said)
    let (st, _) = call(&w.app, "DELETE", &format!("/v1/threads/{tid}"), ana, None).await;
    assert_eq!(st, StatusCode::NO_CONTENT);
    let (st, _) = call(
        &w.app,
        "GET",
        &format!("/v1/threads/{tid}/messages"),
        ana,
        None,
    )
    .await;
    assert_eq!(st, StatusCode::NOT_FOUND);
    let (_, audit) = call(&w.app, "GET", "/v1/audit", operator, None).await;
    let text = audit.to_string();
    assert!(
        text.contains("merion.thread.delete"),
        "the deletion is not journaled"
    );
    assert!(
        !text.contains("book a table"),
        "message text must never reach the journal"
    );
}

#[tokio::test]
async fn memory_is_private_opt_in_and_decided_by_the_user() {
    let w = world().await;
    let (ana, ben) = (Some(w.ana.token.as_str()), Some(w.ben.token.as_str()));
    let opt = op();
    let operator = Some(opt.as_str());

    // off by default: nothing can be added
    let (_, v) = call(&w.app, "GET", "/v1/memory", ana, None).await;
    assert_eq!(v["enabled"], false);
    let (st, _) = call(
        &w.app,
        "POST",
        "/v1/memory",
        ana,
        Some(json!({"text": "likes tea"})),
    )
    .await;
    assert_eq!(st, StatusCode::BAD_REQUEST, "memory is off");
    let (st, v) = call(
        &w.app,
        "PUT",
        "/v1/memory/settings",
        ana,
        Some(json!({"enabled": true})),
    )
    .await;
    assert_eq!((st, v["enabled"].clone()), (StatusCode::OK, json!(true)));
    let (st, item) = call(
        &w.app,
        "POST",
        "/v1/memory",
        ana,
        Some(json!({"text": "vegetarian", "kind": "fact", "pinned": true})),
    )
    .await;
    assert_eq!(st, StatusCode::CREATED, "{item}");
    assert_eq!(
        (item["origin"].as_str(), item["status"].as_str()),
        (Some("user"), Some("active"))
    );
    let mid = item["id"].as_str().unwrap().to_string();

    // a credential is refused and the answer names the shape, not the text
    let (st, v) = call(
        &w.app,
        "POST",
        "/v1/memory",
        ana,
        Some(json!({"text": "token ghp_abcdefghijklmnopqrstuvwxyz0123456789"})),
    )
    .await;
    assert_eq!(st, StatusCode::BAD_REQUEST);
    assert!(
        v.to_string().contains("github-token") && !v.to_string().contains("ghp_abc"),
        "{v}"
    );

    // another user has their own, empty and off; they cannot see, edit, delete, accept or reject ana's
    let (_, v) = call(&w.app, "GET", "/v1/memory", ben, None).await;
    assert_eq!(
        (v["enabled"].clone(), v["items"].as_array().unwrap().len()),
        (json!(false), 0)
    );
    let (_, v) = call(&w.app, "GET", "/v1/memory?user_id=ana", ben, None).await;
    assert!(
        v["items"].as_array().unwrap().is_empty(),
        "a user token ignores user_id"
    );
    for (method, path) in [
        ("PATCH", format!("/v1/memory/{mid}")),
        ("DELETE", format!("/v1/memory/{mid}")),
        ("POST", format!("/v1/memory/{mid}/accept")),
        ("POST", format!("/v1/memory/{mid}/reject")),
    ] {
        let (st, _) = call(
            &w.app,
            method,
            &path,
            ben,
            Some(json!({"text": "hijacked"})),
        )
        .await;
        assert_eq!(st, StatusCode::NOT_FOUND, "{method} {path}");
    }
    assert_eq!(
        w.state.store.memory.get("ana").await.items[0].text,
        "vegetarian",
        "nothing of ana's changed"
    );

    // an agent's proposal waits: not an item, not context, until the user accepts it
    let p = w
        .state
        .store
        .memory
        .propose(
            "ana",
            "prefers window seats",
            "preference",
            crate::memory::Source::default(),
            true,
        )
        .await
        .unwrap();
    let (_, v) = call(&w.app, "GET", "/v1/memory", ana, None).await;
    assert_eq!(v["proposals"][0]["id"], p.id.to_string());
    assert_eq!(v["proposals"][0]["tainted"], true);
    // the user's inbox lists it for review; another user's inbox does not
    let (_, inbox) = call(&w.app, "GET", "/v1/inbox", ana, None).await;
    assert_eq!(inbox["memory_proposals"][0]["id"], p.id.to_string());
    assert_eq!(inbox["memory_proposals"][0]["tainted"], true);
    let (_, inbox) = call(&w.app, "GET", "/v1/inbox", ben, None).await;
    assert!(inbox["memory_proposals"].as_array().unwrap().is_empty());
    assert_eq!(v["items"].as_array().unwrap().len(), 1);
    let (st, _) = call(
        &w.app,
        "POST",
        &format!("/v1/memory/{}/accept", p.id),
        ana,
        None,
    )
    .await;
    assert_eq!(st, StatusCode::OK);
    let (_, v) = call(&w.app, "GET", "/v1/memory", ana, None).await;
    assert_eq!(
        (
            v["items"].as_array().unwrap().len(),
            v["proposals"].as_array().unwrap().len()
        ),
        (2, 0)
    );

    // edit, then the operator: must name the user, and every access is journaled without the text
    let (st, v) = call(
        &w.app,
        "PATCH",
        &format!("/v1/memory/{mid}"),
        ana,
        Some(json!({"text": "vegan", "pinned": false})),
    )
    .await;
    assert_eq!((st, v["text"].as_str()), (StatusCode::OK, Some("vegan")));
    let (st, _) = call(&w.app, "GET", "/v1/memory", operator, None).await;
    assert_eq!(
        st,
        StatusCode::BAD_REQUEST,
        "the operator must name whose memory"
    );
    let (st, v) = call(&w.app, "GET", "/v1/memory?user_id=ana", operator, None).await;
    assert_eq!(st, StatusCode::OK);
    assert_eq!(v["items"].as_array().unwrap().len(), 2);
    let (_, audit) = call(&w.app, "GET", "/v1/audit", operator, None).await;
    let text = audit.to_string();
    assert!(
        text.contains("merion.memory.operator_access"),
        "the operator's read is journaled"
    );
    assert!(
        !text.contains("vegan") && !text.contains("window seats"),
        "memory text never reaches the journal"
    );

    // forgetting everything keeps the on/off choice; without a token nothing works
    let (st, v) = call(&w.app, "DELETE", "/v1/memory", ana, None).await;
    assert_eq!((st, v["removed"].clone()), (StatusCode::OK, json!(2)));
    let (_, v) = call(&w.app, "GET", "/v1/memory", ana, None).await;
    assert!(v["items"].as_array().unwrap().is_empty() && v["enabled"] == true);
    let (st, _) = call(&w.app, "GET", "/v1/memory", None, None).await;
    assert_eq!(st, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn goals_are_private_bounded_and_only_cancelled_or_paused_by_their_owner() {
    let w = world().await;
    let (ana, ben) = (Some(w.ana.token.as_str()), Some(w.ben.token.as_str()));
    let opt = op();
    let operator = Some(opt.as_str());
    let agent = w
        .state
        .store
        .get_session(w.ana.session)
        .await
        .unwrap()
        .agent;
    w.state
        .store
        .deploy_agent(crate::model::DeployAgentRequest {
            name: agent.clone(),
            bundle_base64: base64::Engine::encode(
                &base64::engine::general_purpose::STANDARD,
                "export default 1",
            ),
            manifest: crate::egress::ask_tests::manifest(crate::model::EgressMode::Deny, Some(30)),
        })
        .await
        .unwrap();
    let goal = |autorun: bool| json!({"title": "Trip", "agent": agent, "autorun": autorun, "plan": [{"title": "a"}, {"title": "b", "requires_approval": true}]});

    // a user creates a goal for themselves; the owner is forced, whatever the body says
    let (st, g) = call(&w.app, "POST", "/v1/goals", ana, Some(goal(false))).await;
    assert_eq!(st, StatusCode::CREATED, "{g}");
    assert_eq!(g["user_id"], "ana");
    let gid = g["id"].as_str().unwrap().to_string();
    let (st, _) = call(
        &w.app,
        "POST",
        "/v1/goals",
        ana,
        Some(json!({"title": "x", "agent": agent, "user_id": "ben"})),
    )
    .await;
    assert_eq!(st, StatusCode::BAD_REQUEST, "not for someone else");
    let (st, _) = call(
        &w.app,
        "POST",
        "/v1/goals",
        ana,
        Some(json!({"title": "x", "agent": agent, "session_id": w.ben.session})),
    )
    .await;
    assert_eq!(
        st,
        StatusCode::FORBIDDEN,
        "binding a session is the operator's"
    );
    let (st, _) = call(
        &w.app,
        "POST",
        "/v1/goals",
        ana,
        Some(json!({"title": "x", "agent": agent, "allow_hosts": ["a.example"]})),
    )
    .await;
    assert_eq!(st, StatusCode::FORBIDDEN);
    let (st, _) = call(
        &w.app,
        "POST",
        "/v1/goals",
        ana,
        Some(json!({"title": "x", "agent": "no-such-agent"})),
    )
    .await;
    assert_eq!(st, StatusCode::NOT_FOUND);
    let many: Vec<Value> = (0..crate::goals::USER_MAX_STEPS + 1)
        .map(|i| json!({"title": format!("s{i}")}))
        .collect();
    let (st, _) = call(
        &w.app,
        "POST",
        "/v1/goals",
        ana,
        Some(json!({"title": "x", "agent": agent, "plan": many})),
    )
    .await;
    assert_eq!(st, StatusCode::BAD_REQUEST, "too many steps");

    // list and read: own only; another user's goal is a 404 for every probe
    let (_, v) = call(&w.app, "GET", "/v1/goals", ana, None).await;
    assert_eq!(ids(&v, "id"), std::slice::from_ref(&gid));
    let (_, v) = call(&w.app, "GET", "/v1/goals", ben, None).await;
    assert!(ids(&v, "id").is_empty());
    for (method, body) in [
        ("GET", None),
        ("PATCH", Some(json!({"status": "cancelled"}))),
    ] {
        let (st, _) = call(&w.app, method, &format!("/v1/goals/{gid}"), ben, body).await;
        assert_eq!(st, StatusCode::NOT_FOUND, "{method}");
    }
    assert_ne!(
        w.state
            .store
            .get_goal(gid.parse().unwrap())
            .await
            .unwrap()
            .status,
        crate::goals::GoalStatus::Cancelled,
        "ben changed nothing"
    );
    // the operator's routes stay the operator's
    for path in [
        format!("/v1/goals/{gid}/advance"),
        format!("/v1/goals/{gid}/browse"),
    ] {
        let (st, _) = call(&w.app, "POST", &path, ana, Some(json!({}))).await;
        assert_eq!(st, StatusCode::FORBIDDEN, "{path}");
    }
    let (st, _) = call(
        &w.app,
        "PATCH",
        &format!("/v1/goals/{gid}"),
        ana,
        Some(json!({"plan": []})),
    )
    .await;
    assert_eq!(st, StatusCode::FORBIDDEN, "a user cannot rewrite the plan");
    let (st, _) = call(
        &w.app,
        "PATCH",
        &format!("/v1/goals/{gid}"),
        ana,
        Some(json!({"status": "done"})),
    )
    .await;
    assert_eq!(st, StatusCode::FORBIDDEN, "a user cannot mark a goal done");
    let (st, _) = call(
        &w.app,
        "PATCH",
        &format!("/v1/goals/{gid}"),
        ana,
        Some(json!({"session_id": w.ben.session})),
    )
    .await;
    assert_eq!(st, StatusCode::FORBIDDEN);

    // automatic goals are capped per user
    let mut auto = Vec::new();
    for _ in 0..crate::goals::USER_MAX_ACTIVE_AUTORUN {
        let (st, g) = call(&w.app, "POST", "/v1/goals", ana, Some(goal(true))).await;
        assert_eq!(st, StatusCode::CREATED, "{g}");
        auto.push(g["id"].as_str().unwrap().to_string());
    }
    let (st, _) = call(&w.app, "POST", "/v1/goals", ana, Some(goal(true))).await;
    assert_eq!(
        st,
        StatusCode::TOO_MANY_REQUESTS,
        "the sixth automatic goal"
    );
    let (st, _) = call(
        &w.app,
        "PATCH",
        &format!("/v1/goals/{gid}"),
        ana,
        Some(json!({"autorun": true})),
    )
    .await;
    assert_eq!(
        st,
        StatusCode::TOO_MANY_REQUESTS,
        "switching one on counts too"
    );
    // ben's count is his own
    let (st, _) = call(&w.app, "POST", "/v1/goals", ben, Some(goal(true))).await;
    assert_eq!(st, StatusCode::CREATED);
    // pausing frees a slot; cancelling ends the goal and it cannot be switched on again
    let (st, v) = call(
        &w.app,
        "PATCH",
        &format!("/v1/goals/{}", auto[0]),
        ana,
        Some(json!({"autorun": false})),
    )
    .await;
    assert_eq!((st, v["autorun"].clone()), (StatusCode::OK, json!(false)));
    let (st, _) = call(
        &w.app,
        "PATCH",
        &format!("/v1/goals/{gid}"),
        ana,
        Some(json!({"autorun": true})),
    )
    .await;
    assert_eq!(st, StatusCode::OK);
    let (st, v) = call(
        &w.app,
        "PATCH",
        &format!("/v1/goals/{}", auto[1]),
        ana,
        Some(json!({"status": "cancelled"})),
    )
    .await;
    assert_eq!(
        (st, v["status"].clone()),
        (StatusCode::OK, json!("cancelled"))
    );
    let (st, _) = call(
        &w.app,
        "PATCH",
        &format!("/v1/goals/{}", auto[1]),
        ana,
        Some(json!({"autorun": true})),
    )
    .await;
    assert_eq!(
        st,
        StatusCode::CONFLICT,
        "a cancelled goal is not run again"
    );

    // the operator sees every goal and may still do everything
    let (_, v) = call(&w.app, "GET", "/v1/goals", operator, None).await;
    assert!(
        ids(&v, "user_id").iter().any(|u| u == "ben")
            && ids(&v, "user_id").iter().any(|u| u == "ana")
    );
    let (st, _) = call(
        &w.app,
        "PATCH",
        &format!("/v1/goals/{gid}"),
        operator,
        Some(json!({"allow_hosts": ["x.example"]})),
    )
    .await;
    assert_eq!(st, StatusCode::OK);
    // the operator can narrow the list to one user; a user token cannot widen it
    let (_, v) = call(&w.app, "GET", "/v1/goals?user_id=ben", operator, None).await;
    assert_eq!(ids(&v, "user_id"), ["ben"]);
    let (_, v) = call(&w.app, "GET", "/v1/goals?user_id=ben", ana, None).await;
    assert!(
        ids(&v, "user_id").iter().all(|u| u == "ana"),
        "a user token ignores user_id"
    );
    // who am I
    let (_, v) = call(&w.app, "GET", "/v1/whoami", ana, None).await;
    assert_eq!(
        (v["role"].clone(), v["user_id"].clone()),
        (json!("user"), json!("ana"))
    );
    let (_, v) = call(&w.app, "GET", "/v1/whoami", operator, None).await;
    assert_eq!(v["role"], "operator");
    let (st, _) = call(&w.app, "GET", "/v1/goals", None, None).await;
    assert_eq!(st, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn receipts_are_listed_per_person_and_carry_no_body() {
    let w = world().await;
    let (ana, ben) = (Some(w.ana.token.as_str()), Some(w.ben.token.as_str()));
    let opt = op();
    let operator = Some(opt.as_str());
    let make = |user: &str, status: u16| crate::receipts::Receipt {
        id: Uuid::new_v4(),
        at: Utc::now(),
        user_id: Some(user.into()),
        session_id: Uuid::new_v4(),
        agent: "mail".into(),
        credential: Some("mail".into()),
        method: "POST".into(),
        url: "https://mail.example/send".into(),
        body_bytes: 12,
        body_sha256: "digest".into(),
        approval_id: Some(Uuid::new_v4()),
        idempotency_key: Some(format!("{user}-key")),
        status,
        fingerprint: "fp".into(),
    };
    w.state
        .store
        .receipts
        .record(make("ana", 200))
        .await
        .unwrap();
    w.state
        .store
        .receipts
        .record(make("ben", 201))
        .await
        .unwrap();
    w.state
        .store
        .receipts
        .record(make("ana", 202))
        .await
        .unwrap();

    let (st, v) = call(&w.app, "GET", "/v1/receipts", ana, None).await;
    assert_eq!(st, StatusCode::OK);
    assert_eq!(
        v["items"]
            .as_array()
            .unwrap()
            .iter()
            .map(|r| r["status"].as_u64().unwrap())
            .collect::<Vec<_>>(),
        [202, 200],
        "hers, newest first"
    );
    assert!(!v.to_string().contains("ben-key"), "nothing of ben's");
    let (_, v) = call(&w.app, "GET", "/v1/receipts?user_id=ana", ben, None).await;
    assert_eq!(ids(&v, "user_id"), ["ben"], "a user token ignores user_id");
    assert!(
        !v.to_string().contains("body\":"),
        "a receipt has a digest and a size, never the body"
    );
    let (_, v) = call(&w.app, "GET", "/v1/receipts", operator, None).await;
    assert_eq!(v["items"].as_array().unwrap().len(), 3);
    let (_, v) = call(
        &w.app,
        "GET",
        "/v1/receipts?user_id=ben&limit=1",
        operator,
        None,
    )
    .await;
    assert_eq!(ids(&v, "user_id"), ["ben"]);
    for method in ["POST", "DELETE", "PUT"] {
        let (st, _) = call(&w.app, method, "/v1/receipts", ana, Some(json!({}))).await;
        assert_eq!(st, StatusCode::FORBIDDEN, "{method}");
    }
    let (st, _) = call(&w.app, "GET", "/v1/receipts", None, None).await;
    assert_eq!(st, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn connections_are_private_write_only_and_offered_only_by_the_host() {
    // a token endpoint on loopback, so the test can mint a real access token and see disconnecting drop it
    let token_app = Router::new().route(
        "/token",
        axum::routing::post(|| async {
            axum::Json(json!({"access_token": "at-live", "expires_in": 3600}))
        }),
    );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let token_url = format!("http://{}/token", listener.local_addr().unwrap());
    tokio::spawn(async move { axum::serve(listener, token_app).await.unwrap() });
    std::env::set_var("TEN_ID", "cid");
    std::env::set_var("TEN_SECRET", "csecret");
    let file = std::env::temp_dir().join(format!("zyvor-conn-{}.json", Uuid::new_v4()));
    std::fs::write(
        &file,
        json!({"gmail-read": {
            "host": "gmail.googleapis.com", "header": "authorization", "kind": "oauth-refresh", "allowed_methods": ["GET"],
            "oauth": {"token_url": token_url, "client_id_env": "TEN_ID", "client_secret_env": "TEN_SECRET", "connection": "google"}
        }})
        .to_string(),
    )
    .unwrap();
    let (state, base) = state_and_session_cfg(|c| {
        c.api_token = Some(op());
        c.credentials_file = Some(file);
    })
    .await;
    let app = public_router(state.clone());
    let mint = |user: &str| {
        let (app, user) = (app.clone(), user.to_string());
        async move {
            let (_, v) = call(
                &app,
                "POST",
                "/v1/user-tokens",
                Some(op().as_str()),
                Some(json!({"user_id": user, "ttl_seconds": 600})),
            )
            .await;
            v["token"].as_str().unwrap().to_string()
        }
    };
    let _ = base;
    let (ana_t, ben_t) = (mint("ana").await, mint("ben").await);
    let (ana, ben) = (Some(ana_t.as_str()), Some(ben_t.as_str()));
    let opt = op();
    let operator = Some(opt.as_str());
    let secret = "1//ana-very-secret-refresh-token";

    // the host offers "google"; nobody is connected
    let (st, v) = call(&app, "GET", "/v1/connections", ana, None).await;
    assert_eq!(st, StatusCode::OK);
    assert_eq!(
        (
            v["items"][0]["name"].clone(),
            v["items"][0]["connected"].clone()
        ),
        (json!("google"), json!(false))
    );
    // only names the host's credentials ask for; and the token must look like one
    let (st, _) = call(
        &app,
        "PUT",
        "/v1/connections/dropbox",
        ana,
        Some(json!({"refresh_token": secret})),
    )
    .await;
    assert_eq!(
        st,
        StatusCode::NOT_FOUND,
        "the host offers no such connection"
    );
    for bad in ["", "short", "has spaces in it here"] {
        let (st, _) = call(
            &app,
            "PUT",
            "/v1/connections/google",
            ana,
            Some(json!({"refresh_token": bad})),
        )
        .await;
        assert_eq!(st, StatusCode::BAD_REQUEST, "{bad:?}");
    }
    // connect: the answer never contains the token, and neither does anything read afterwards
    let (st, v) = call(
        &app,
        "PUT",
        "/v1/connections/google",
        ana,
        Some(json!({"refresh_token": secret})),
    )
    .await;
    assert_eq!((st, v["connected"].clone()), (StatusCode::OK, json!(true)));
    let mut everything = v.to_string();
    let (_, v) = call(&app, "GET", "/v1/connections", ana, None).await;
    assert_eq!(v["items"][0]["connected"], true);
    assert!(v["items"][0]["connected_at"].is_string());
    everything += &v.to_string();
    let (_, audit) = call(&app, "GET", "/v1/audit", operator, None).await;
    everything += &audit.to_string();
    assert!(
        everything.contains("merion.connection.connected"),
        "the connection is journaled"
    );
    assert!(
        !everything.contains(secret) && !everything.contains("very-secret"),
        "the refresh token is write-only"
    );

    // the person's access token is minted from it, and replacing the connection drops the old one
    let http = reqwest::Client::new();
    let ana_token = state.store.connections.refresh_token("ana", "google").await;
    state
        .credentials
        .ensure_user_token("gmail-read", "ana", ana_token.as_deref(), &http)
        .await
        .unwrap();
    assert!(state
        .credentials
        .resolve_for("gmail-read", Some("ana"))
        .is_ok());
    let (st, _) = call(
        &app,
        "PUT",
        "/v1/connections/google",
        ana,
        Some(json!({"refresh_token": "1//ana-a-replacement-token"})),
    )
    .await;
    assert_eq!(st, StatusCode::OK);
    assert!(
        state
            .credentials
            .resolve_for("gmail-read", Some("ana"))
            .is_err(),
        "a new connection drops the access token minted from the old one"
    );
    let ana_token = state.store.connections.refresh_token("ana", "google").await;
    state
        .credentials
        .ensure_user_token("gmail-read", "ana", ana_token.as_deref(), &http)
        .await
        .unwrap();

    // another person is unaffected and cannot disconnect ana's
    let (_, v) = call(&app, "GET", "/v1/connections", ben, None).await;
    assert_eq!(v["items"][0]["connected"], false);
    let (st, _) = call(&app, "DELETE", "/v1/connections/google", ben, None).await;
    assert_eq!(st, StatusCode::NOT_FOUND, "ben has nothing to disconnect");
    assert_eq!(
        state
            .store
            .connections
            .refresh_token("ana", "google")
            .await
            .as_deref(),
        Some("1//ana-a-replacement-token"),
        "ana's is untouched"
    );
    let (_, v) = call(&app, "GET", "/v1/connections?user_id=ana", ben, None).await;
    assert_eq!(
        v["items"][0]["connected"], false,
        "a user token ignores user_id"
    );

    // the operator must name the person, and each access is journaled without the token
    let (st, _) = call(&app, "GET", "/v1/connections", operator, None).await;
    assert_eq!(st, StatusCode::BAD_REQUEST);
    let (st, v) = call(&app, "GET", "/v1/connections?user_id=ana", operator, None).await;
    assert_eq!(
        (st, v["items"][0]["connected"].clone()),
        (StatusCode::OK, json!(true))
    );
    let (_, audit) = call(&app, "GET", "/v1/audit", operator, None).await;
    assert!(
        audit
            .to_string()
            .contains("merion.connection.operator_access")
            && !audit.to_string().contains(secret)
    );

    // disconnect: gone at once; the routes need a token
    let (st, _) = call(&app, "DELETE", "/v1/connections/google", ana, None).await;
    assert_eq!(st, StatusCode::NO_CONTENT);
    assert!(state
        .store
        .connections
        .refresh_token("ana", "google")
        .await
        .is_none());
    assert!(
        state
            .credentials
            .resolve_for("gmail-read", Some("ana"))
            .is_err(),
        "disconnecting drops the cached access token at once"
    );
    let (st, _) = call(&app, "DELETE", "/v1/connections/google", ana, None).await;
    assert_eq!(st, StatusCode::NOT_FOUND, "already disconnected");
    let (st, _) = call(&app, "GET", "/v1/connections", None, None).await;
    assert_eq!(st, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn a_plan_is_proposed_by_the_planner_and_only_the_owner_can_accept_or_reject_it() {
    let w = world().await;
    let (ana, ben) = (Some(w.ana.token.as_str()), Some(w.ben.token.as_str()));
    let agent = w
        .state
        .store
        .get_session(w.ana.session)
        .await
        .unwrap()
        .agent;
    w.state
        .store
        .deploy_agent(crate::model::DeployAgentRequest {
            name: agent.clone(),
            bundle_base64: base64::Engine::encode(
                &base64::engine::general_purpose::STANDARD,
                "export default 1",
            ),
            manifest: crate::egress::ask_tests::manifest(crate::model::EgressMode::Deny, Some(30)),
        })
        .await
        .unwrap();
    let (st, g) = call(
        &w.app,
        "POST",
        "/v1/goals",
        ana,
        Some(json!({"title": "Trip to Lisbon", "agent": agent})),
    )
    .await;
    assert_eq!(st, StatusCode::CREATED, "{g}");
    let gid = g["id"].as_str().unwrap().to_string();
    let uid: Uuid = gid.parse().unwrap();
    let plan_path = format!("/v1/goals/{gid}/plan");

    let (st, msg) = post_empty_json(&w.app, &plan_path, &w.ana.token).await;
    assert!(
        st == StatusCode::BAD_REQUEST && msg.contains("no planner agent"),
        "an empty JSON body is read as no options (then: no planner named): {st} {msg}"
    );
    // asking for a plan: refused without a planner, or with one that is not deployed; someone else's goal is a 404
    let (st, v) = call(&w.app, "POST", &plan_path, ana, None).await;
    assert_eq!(st, StatusCode::BAD_REQUEST, "{v}");
    let (st, _) = call(
        &w.app,
        "POST",
        &plan_path,
        ana,
        Some(json!({"planner": "no-such-planner"})),
    )
    .await;
    assert_eq!(st, StatusCode::NOT_FOUND);
    let (st, _) = call(
        &w.app,
        "POST",
        &plan_path,
        ben,
        Some(json!({"planner": agent})),
    )
    .await;
    assert_eq!(st, StatusCode::NOT_FOUND, "another user's goal");
    let (st, _) = call(&w.app, "POST", &plan_path, None, None).await;
    assert_eq!(st, StatusCode::UNAUTHORIZED);

    // a session that was not started to plan this goal cannot propose anything
    let ana_session = w.state.store.get_session(w.ana.session).await.unwrap();
    let data = json!({"steps": [{"title": "Find flights", "input": {"message": "flights to Lisbon"}}, {"title": "Book one", "requires_approval": true, "approval_reason": "it costs money"}]});
    crate::goal_plan::record_proposal(&w.state, &ana_session, &data).await;
    let events = w.state.store.events_after(ana_session.id, 0).await.unwrap();
    assert!(
        events.iter().any(|e| e.kind == "goal.plan_refused"),
        "no planning session, no proposal"
    );
    let (_, v) = call(&w.app, "GET", &format!("/v1/goals/{gid}"), ana, None).await;
    assert!(v.get("proposed_plan").is_none());

    // the planning session started for the goal proposes once; a bad plan is refused and stored as nothing
    let mut goal = w.state.store.get_goal(uid).await.unwrap();
    goal.planning_session_id = Some(ana_session.id);
    w.state.store.save_goal(goal).await.unwrap();
    crate::goal_plan::record_proposal(&w.state, &ana_session, &json!({"steps": []})).await;
    let (_, v) = call(&w.app, "GET", &format!("/v1/goals/{gid}"), ana, None).await;
    assert!(
        v.get("proposed_plan").is_none(),
        "an empty plan is not a proposal"
    );
    crate::goal_plan::record_proposal(&w.state, &ana_session, &data).await;
    crate::goal_plan::record_proposal(
        &w.state,
        &ana_session,
        &json!({"steps": [{"title": "second try"}]}),
    )
    .await;
    let (_, v) = call(&w.app, "GET", &format!("/v1/goals/{gid}"), ana, None).await;
    let titles: Vec<&str> = v["proposed_plan"]["steps"]
        .as_array()
        .unwrap()
        .iter()
        .map(|s| s["title"].as_str().unwrap())
        .collect();
    assert_eq!(
        titles,
        ["Find flights", "Book one"],
        "one proposal per planning session"
    );
    assert_eq!(
        v["plan"].as_array().unwrap().len(),
        0,
        "nothing is in the plan until it is accepted"
    );

    // another user sees and decides nothing
    for path in [
        format!("/v1/goals/{gid}/plan/accept"),
        format!("/v1/goals/{gid}/plan/reject"),
    ] {
        let (st, _) = call(&w.app, "POST", &path, ben, None).await;
        assert_eq!(st, StatusCode::NOT_FOUND, "{path}");
    }
    let (_, v) = call(&w.app, "GET", "/v1/goals", ben, None).await;
    assert!(v["items"].as_array().unwrap().is_empty());

    // the journal knows a plan was proposed and how long, not what it said
    let journal =
        serde_json::to_string(&w.state.store.audit.list(None, 200).await.unwrap()).unwrap();
    assert!(
        journal.contains("merion.goal.plan_proposed")
            && !journal.contains("Find flights")
            && !journal.contains("flights to Lisbon"),
        "{journal}"
    );

    // a planner that had read untrusted content needs an explicit confirmation
    w.state
        .store
        .update_session(ana_session.id, |s| {
            s.tainted_by = vec!["evil.example".into()]
        })
        .await
        .unwrap();
    let mut goal = w.state.store.get_goal(uid).await.unwrap();
    goal.proposed_plan.as_mut().unwrap().tainted = true;
    w.state.store.save_goal(goal).await.unwrap();
    let (st, v) = call(
        &w.app,
        "POST",
        &format!("/v1/goals/{gid}/plan/accept"),
        ana,
        None,
    )
    .await;
    assert_eq!(st, StatusCode::CONFLICT, "{v}");
    assert!(v["error"].as_str().unwrap().contains("confirm_tainted"));
    // rejecting throws it away; a new proposal can be accepted, with the goal's own opt-in to run it
    let (st, _) = call(
        &w.app,
        "POST",
        &format!("/v1/goals/{gid}/plan/reject"),
        ana,
        None,
    )
    .await;
    assert_eq!(st, StatusCode::OK);
    let (st, _) = call(
        &w.app,
        "POST",
        &format!("/v1/goals/{gid}/plan/accept"),
        ana,
        None,
    )
    .await;
    assert_eq!(st, StatusCode::CONFLICT, "nothing left to accept");
    let mut goal = w.state.store.get_goal(uid).await.unwrap();
    goal.planning_session_id = Some(ana_session.id);
    w.state.store.save_goal(goal).await.unwrap();
    crate::goal_plan::record_proposal(&w.state, &ana_session, &data).await;
    let (st, v) = call(
        &w.app,
        "POST",
        &format!("/v1/goals/{gid}/plan/accept"),
        ana,
        Some(json!({"confirm_tainted": true, "autorun": true})),
    )
    .await;
    assert_eq!(st, StatusCode::OK, "{v}");
    assert_eq!(v["plan"].as_array().unwrap().len(), 2);
    assert_eq!(v["plan"][0]["id"], "s1");
    assert_eq!(
        v["plan"][0]["input"],
        json!({"message": "flights to Lisbon"})
    );
    assert_eq!(v["plan"][1]["requires_approval"], true);
    assert_eq!(
        v["plan"][1]["approval_reason"], "it costs money",
        "carried from the proposal to the accepted plan"
    );
    assert_eq!(
        v["plan"][0]["approval_reason"],
        Value::Null,
        "a step with no reason has none"
    );
    assert_eq!(v["autorun"], true);
    assert!(v.get("proposed_plan").is_none());
    // a goal with a plan cannot be planned again
    let (st, _) = call(
        &w.app,
        "POST",
        &plan_path,
        ana,
        Some(json!({"planner": agent})),
    )
    .await;
    assert_eq!(st, StatusCode::CONFLICT);
    // a late proposal does not replace the accepted plan
    let mut goal = w.state.store.get_goal(uid).await.unwrap();
    goal.planning_session_id = Some(ana_session.id);
    w.state.store.save_goal(goal).await.unwrap();
    crate::goal_plan::record_proposal(
        &w.state,
        &ana_session,
        &json!({"steps": [{"title": "late"}]}),
    )
    .await;
    let (_, v) = call(&w.app, "GET", &format!("/v1/goals/{gid}"), ana, None).await;
    assert!(
        v.get("proposed_plan").is_none() && v["plan"].as_array().unwrap().len() == 2,
        "{v}"
    );
}

#[tokio::test]
async fn suggestions_are_opt_in_private_bounded_and_only_the_owner_turns_them_into_goals() {
    let w = world().await;
    let (ana, ben) = (Some(w.ana.token.as_str()), Some(w.ben.token.as_str()));
    let agent = w
        .state
        .store
        .get_session(w.ana.session)
        .await
        .unwrap()
        .agent;
    let mut manifest = crate::egress::ask_tests::manifest(crate::model::EgressMode::Deny, Some(30));
    manifest.suggestions = true;
    w.state
        .store
        .deploy_agent(crate::model::DeployAgentRequest {
            name: agent.clone(),
            bundle_base64: base64::Engine::encode(
                &base64::engine::general_purpose::STANDARD,
                "export default 1",
            ),
            manifest: manifest.clone(),
        })
        .await
        .unwrap();
    let session = w.state.store.get_session(w.ana.session).await.unwrap();
    let refused_events = |sid: Uuid| {
        let state = w.state.clone();
        async move {
            state
                .store
                .events_after(sid, 0)
                .await
                .unwrap()
                .into_iter()
                .filter(|e| e.kind == "suggestion.refused")
                .count()
        }
    };
    let propose = |title: &str, s: SessionRecord, m: crate::model::AgentManifest| {
        let state = w.state.clone();
        let data = json!({"title": title, "reason": "you have a gap on Friday"});
        async move { crate::suggestions::record_proposal(&state, &s, &m, &data).await }
    };

    // off by default: a proposal is refused and nothing is kept
    let (_, v) = call(&w.app, "GET", "/v1/suggestions", ana, None).await;
    assert_eq!(
        (v["enabled"].clone(), v["pending"].as_array().unwrap().len()),
        (json!(false), 0)
    );
    propose("Book the dentist", session.clone(), manifest.clone()).await;
    assert_eq!(refused_events(session.id).await, 1);
    let (st, _) = call(
        &w.app,
        "POST",
        &format!("/v1/suggestions/{}/accept", Uuid::new_v4()),
        ana,
        None,
    )
    .await;
    assert_eq!(st, StatusCode::NOT_FOUND);

    // turned on, a proposal waits; a repeat, an agent that did not ask, a session with no user and a secret are refused
    let (st, v) = call(
        &w.app,
        "PUT",
        "/v1/suggestions/settings",
        ana,
        Some(json!({"enabled": true})),
    )
    .await;
    assert_eq!(
        (st, v["enabled"].clone(), v["retention_days"].clone()),
        (StatusCode::OK, json!(true), Value::Null),
        "no retention_days sent, none stored"
    );
    let (st, v) = call(
        &w.app,
        "PUT",
        "/v1/suggestions/settings",
        ana,
        Some(json!({"enabled": true, "retention_days": 14})),
    )
    .await;
    assert_eq!(
        (st, v["retention_days"].clone()),
        (StatusCode::OK, json!(14))
    );
    for bad in [json!(0), json!(3651)] {
        let (st, _) = call(
            &w.app,
            "PUT",
            "/v1/suggestions/settings",
            ana,
            Some(json!({"enabled": true, "retention_days": bad})),
        )
        .await;
        assert_eq!(st, StatusCode::BAD_REQUEST, "retention_days {bad}");
    }
    // another user's settings are untouched by ana's
    let (st, v) = call(
        &w.app,
        "PUT",
        "/v1/suggestions/settings",
        ben,
        Some(json!({"enabled": true})),
    )
    .await;
    assert_eq!(
        (st, v["retention_days"].clone()),
        (StatusCode::OK, Value::Null)
    );
    propose("Book the dentist", session.clone(), manifest.clone()).await;
    propose("  book  THE dentist ", session.clone(), manifest.clone()).await;
    let mut quiet = manifest.clone();
    quiet.suggestions = false;
    propose("Not allowed", session.clone(), quiet).await;
    let mut nobody = session.clone();
    nobody.user_id = None;
    propose("No user", nobody.clone(), manifest.clone()).await;
    propose(
        "Use token ghp_abcdefghijklmnopqrstuvwxyz0123456789",
        session.clone(),
        manifest.clone(),
    )
    .await;
    assert_eq!(
        refused_events(session.id).await,
        5,
        "the first (off), the repeat, the quiet agent, and the secret"
    );
    assert_eq!(
        refused_events(nobody.id).await,
        5,
        "same session id, counted together"
    );
    let (_, v) = call(&w.app, "GET", "/v1/suggestions", ana, None).await;
    let pending = v["pending"].as_array().unwrap();
    assert_eq!(pending.len(), 1);
    assert_eq!(pending[0]["agent"], agent.as_str());
    let sid = pending[0]["id"].as_str().unwrap().to_string();

    // another person sees none and can decide none; the inbox shows hers
    let (_, v) = call(&w.app, "GET", "/v1/suggestions", ben, None).await;
    assert!(v["pending"].as_array().unwrap().is_empty());
    for path in [
        format!("/v1/suggestions/{sid}/accept"),
        format!("/v1/suggestions/{sid}/dismiss"),
    ] {
        let (st, _) = call(&w.app, "POST", &path, ben, None).await;
        assert_eq!(st, StatusCode::NOT_FOUND, "{path}");
    }
    let (_, v) = call(&w.app, "GET", "/v1/inbox", ana, None).await;
    assert_eq!(v["suggestions"][0]["title"], "Book the dentist");
    let (_, v) = call(&w.app, "GET", "/v1/inbox", ben, None).await;
    assert!(v["suggestions"].as_array().unwrap().is_empty());
    let journal =
        serde_json::to_string(&w.state.store.audit.list(None, 200).await.unwrap()).unwrap();
    assert!(
        journal.contains("merion.suggestion.proposed")
            && !journal.contains("dentist")
            && !journal.contains("gap on Friday"),
        "{journal}"
    );

    // accepting makes an ordinary goal for her: no plan, nothing running
    let (st, msg) = post_empty_json(
        &w.app,
        &format!("/v1/suggestions/{}/accept", Uuid::new_v4()),
        &w.ana.token,
    )
    .await;
    assert_eq!(
        st,
        StatusCode::NOT_FOUND,
        "an empty JSON body is read as no options, not a 400: {msg}"
    );
    let (st, v) = call(
        &w.app,
        "POST",
        &format!("/v1/suggestions/{sid}/accept"),
        ana,
        None,
    )
    .await;
    assert_eq!(st, StatusCode::OK, "{v}");
    assert_eq!(v["goal"]["user_id"], "ana");
    assert_eq!(
        (v["goal"]["title"].clone(), v["goal"]["autorun"].clone()),
        (json!("Book the dentist"), json!(false))
    );
    assert!(v["goal"]["plan"].as_array().unwrap().is_empty());
    assert_eq!(v["suggestion"]["status"], "accepted");
    let (_, g) = call(&w.app, "GET", "/v1/goals", ben, None).await;
    assert!(g["items"].as_array().unwrap().is_empty());
    let (st, _) = call(
        &w.app,
        "POST",
        &format!("/v1/suggestions/{sid}/accept"),
        ana,
        None,
    )
    .await;
    assert_eq!(st, StatusCode::NOT_FOUND, "already decided");

    // a session that had read untrusted content needs an explicit confirmation; dismissing suppresses a repeat
    let mut tainted = session.clone();
    tainted.tainted_by = vec!["evil.example".into()];
    propose("Wire the money", tainted.clone(), manifest.clone()).await;
    let (_, v) = call(&w.app, "GET", "/v1/suggestions", ana, None).await;
    let tid = v["pending"][0]["id"].as_str().unwrap().to_string();
    assert_eq!(v["pending"][0]["tainted"], true);
    let (st, v) = call(
        &w.app,
        "POST",
        &format!("/v1/suggestions/{tid}/accept"),
        ana,
        None,
    )
    .await;
    assert_eq!(st, StatusCode::CONFLICT, "{v}");
    let (st, v) = call(
        &w.app,
        "POST",
        &format!("/v1/suggestions/{tid}/dismiss"),
        ana,
        None,
    )
    .await;
    assert_eq!(
        (st, v["status"].clone()),
        (StatusCode::OK, json!("dismissed"))
    );
    let before = refused_events(session.id).await;
    propose("Wire the money", session.clone(), manifest.clone()).await;
    assert_eq!(
        refused_events(session.id).await,
        before + 1,
        "a dismissed suggestion is not made again"
    );

    // a person's list is bounded
    for i in 0..crate::suggestions::MAX_PENDING + 3 {
        propose(
            &format!("Idea number {i}"),
            session.clone(),
            manifest.clone(),
        )
        .await;
    }
    let (_, v) = call(&w.app, "GET", "/v1/suggestions", ana, None).await;
    assert_eq!(
        v["pending"].as_array().unwrap().len(),
        crate::suggestions::MAX_PENDING
    );

    // the operator must name whose, and it is journaled; anonymous is refused
    let opt = op();
    let (st, _) = call(&w.app, "GET", "/v1/suggestions", Some(opt.as_str()), None).await;
    assert_eq!(st, StatusCode::BAD_REQUEST);
    let (st, v) = call(
        &w.app,
        "GET",
        "/v1/suggestions?user_id=ana",
        Some(opt.as_str()),
        None,
    )
    .await;
    assert_eq!(
        (st, v["pending"].as_array().unwrap().len()),
        (StatusCode::OK, crate::suggestions::MAX_PENDING)
    );
    let journal =
        serde_json::to_string(&w.state.store.audit.list(None, 400).await.unwrap()).unwrap();
    assert!(journal.contains("merion.suggestion.operator_access"));
    let (st, _) = call(&w.app, "GET", "/v1/suggestions", None, None).await;
    assert_eq!(st, StatusCode::UNAUTHORIZED);
}
