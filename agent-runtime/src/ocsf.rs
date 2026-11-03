// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: Apache-2.0

//! Map journal rows to OCSF-shaped events, so the audit log can go to a SIEM.
//!
//! Each row becomes an *API Activity* event (`class_uid` 6003, category 6). The
//! mapping is shape-only: it fills the fields a SIEM keys on and keeps everything
//! else under `unmapped`. It has not been checked against the OCSF schema validator.
//! The hash chain in `audit.jsonl` stays the source of truth; each event carries
//! its row's `hash` and `prev_hash` so a receiver can re-verify order.

use crate::audit::{AuditEntry, AuditPhase};
use chrono::SecondsFormat;
use serde_json::{json, Value};

const OCSF_VERSION: &str = "1.3.0";
const CLASS_UID: u32 = 6003; // API Activity
const CATEGORY_UID: u32 = 6; // Application Activity

/// OCSF `activity_id`: 1 Create, 2 Read, 3 Update, 4 Delete, 99 Other.
/// Journal rows describe a decision or an outcome, not a CRUD verb, so they are all Other.
const ACTIVITY_OTHER: u32 = 99;

fn status_id(phase: AuditPhase) -> u32 {
    match phase {
        AuditPhase::Approved | AuditPhase::Performed => 1, // Success
        AuditPhase::Denied | AuditPhase::Failed => 2,      // Failure
        AuditPhase::Planned => 0,                          // Unknown: intent, not an outcome
    }
}

fn severity_id(phase: AuditPhase) -> u32 {
    match phase {
        AuditPhase::Denied | AuditPhase::Failed => 3, // Medium
        _ => 1,                                       // Informational
    }
}

/// One journal row as an OCSF-shaped event.
pub fn to_ocsf(e: &AuditEntry) -> Value {
    let phase = serde_json::to_value(e.phase)
        .ok()
        .and_then(|v| v.as_str().map(str::to_owned))
        .unwrap_or_default();
    json!({
        "class_uid": CLASS_UID,
        "category_uid": CATEGORY_UID,
        "activity_id": ACTIVITY_OTHER,
        "type_uid": CLASS_UID * 100 + ACTIVITY_OTHER,
        "time": e.at.timestamp_millis(),
        "severity_id": severity_id(e.phase),
        "status_id": status_id(e.phase),
        "message": format!("{} {}", e.action, phase),
        "metadata": {
            "version": OCSF_VERSION,
            "uid": e.hash,
            "product": {"name": "Zyvor Keep", "vendor_name": "Zyvor AI Labs"},
        },
        "api": {"operation": e.action},
        "unmapped": {
            "seq": e.seq,
            "phase": phase,
            "session_id": e.session_id,
            "subject": e.subject,
            "prev_hash": e.prev_hash,
            "at": e.at.to_rfc3339_opts(SecondsFormat::Nanos, true),
            "detail": e.detail,
        },
    })
}

/// Newline-delimited OCSF events, one per row, in the order given.
pub fn to_ndjson(entries: &[AuditEntry]) -> String {
    let mut out = String::new();
    for e in entries {
        out.push_str(&to_ocsf(e).to_string());
        out.push('\n');
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{TimeZone, Utc};
    use uuid::Uuid;

    fn entry(phase: AuditPhase, action: &str) -> AuditEntry {
        AuditEntry {
            seq: 7,
            at: Utc.with_ymd_and_hms(2026, 9, 30, 12, 0, 0).unwrap(),
            session_id: Some(Uuid::nil()),
            phase,
            action: action.into(),
            subject: Some("api.github.com".into()),
            detail: json!({"k": "v"}),
            prev_hash: "aa".into(),
            hash: "bb".into(),
        }
    }

    #[test]
    fn maps_the_fields_a_siem_keys_on() {
        let v = to_ocsf(&entry(AuditPhase::Performed, "egress.connect"));
        assert_eq!(v["class_uid"], 6003);
        assert_eq!(v["category_uid"], 6);
        assert_eq!(v["type_uid"], 600399);
        assert_eq!(v["time"], 1_790_769_600_000_i64);
        assert_eq!(v["status_id"], 1);
        assert_eq!(v["severity_id"], 1);
        assert_eq!(v["api"]["operation"], "egress.connect");
        assert_eq!(v["metadata"]["uid"], "bb");
        assert_eq!(v["metadata"]["version"], "1.3.0");
    }

    #[test]
    fn keeps_the_chain_and_the_rest_under_unmapped() {
        let v = to_ocsf(&entry(AuditPhase::Performed, "x"));
        assert_eq!(v["unmapped"]["prev_hash"], "aa");
        assert_eq!(v["unmapped"]["seq"], 7);
        assert_eq!(v["unmapped"]["subject"], "api.github.com");
        assert_eq!(v["unmapped"]["detail"]["k"], "v");
        assert_eq!(v["unmapped"]["phase"], "performed");
    }

    #[test]
    fn denials_and_failures_are_failures_at_medium() {
        for p in [AuditPhase::Denied, AuditPhase::Failed] {
            let v = to_ocsf(&entry(p, "x"));
            assert_eq!(v["status_id"], 2);
            assert_eq!(v["severity_id"], 3);
        }
    }

    #[test]
    fn a_plan_is_not_an_outcome() {
        assert_eq!(to_ocsf(&entry(AuditPhase::Planned, "x"))["status_id"], 0);
    }

    #[test]
    fn ndjson_is_one_line_per_row() {
        let rows = [
            entry(AuditPhase::Planned, "a"),
            entry(AuditPhase::Denied, "b"),
        ];
        let out = to_ndjson(&rows);
        assert_eq!(out.lines().count(), 2);
        assert!(out.ends_with('\n'));
        for line in out.lines() {
            serde_json::from_str::<Value>(line).unwrap();
        }
    }

    #[tokio::test]
    async fn export_audit_serves_ocsf_ndjson_behind_the_export_token() {
        use axum::{body::Body, http::Request};
        use tower::ServiceExt;

        let token = crate::fixture::text("operator-token");
        let (state, _) =
            crate::egress::ask_tests::state_and_session_cfg(|c| c.api_token = Some(token.clone()))
                .await;
        state
            .store
            .audit
            .append(
                None,
                AuditPhase::Denied,
                "egress.connect",
                Some("evil.example".into()),
                json!({}),
            )
            .await
            .unwrap();
        let (export, _) = state
            .export_tokens
            .mint("audit:read:1h", 600)
            .await
            .unwrap();
        let app = crate::app::public_router(state);

        let get = |uri: &'static str, with_export: bool| {
            let mut b = Request::builder()
                .uri(uri)
                .header("authorization", format!("Bearer {token}"));
            if with_export {
                b = b.header("x-merion-export-token", export.clone());
            }
            let app = app.clone();
            let req = b.body(Body::empty()).unwrap();
            async move {
                let resp = app.oneshot(req).await.unwrap();
                let status = resp.status();
                let headers = resp.headers().clone();
                let bytes = axum::body::to_bytes(resp.into_body(), 1 << 20)
                    .await
                    .unwrap();
                (status, headers, String::from_utf8_lossy(&bytes).to_string())
            }
        };

        let (st, _, _) = get("/v1/export/audit?format=ocsf", false).await;
        assert_eq!(st, 403);

        let (st, headers, body) = get("/v1/export/audit?format=ocsf", true).await;
        assert_eq!(st, 200, "{body}");
        assert!(headers["content-type"]
            .to_str()
            .unwrap()
            .starts_with("application/x-ndjson"));
        assert_eq!(headers["x-merion-audit-chain"], "ok");
        let first: Value = serde_json::from_str(body.lines().next().unwrap()).unwrap();
        assert_eq!(first["class_uid"], 6003);
        assert!(body
            .lines()
            .any(|l| l.contains("\"egress.connect\"") && l.contains("\"status_id\":2")));

        let (st, _, body) = get("/v1/export/audit?format=json", true).await;
        assert_eq!(st, 200);
        assert!(body.contains("\"export\":true"), "{body}");

        let (st, _, _) = get("/v1/export/audit?format=xml", true).await;
        assert_eq!(st, 400);
    }
}
