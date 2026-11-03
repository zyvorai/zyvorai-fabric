// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: Apache-2.0

//! OpenTelemetry export over OTLP/HTTP with JSON bodies (`/v1/logs` and `/v1/traces`).
//!
//! Off unless `OTEL_EXPORTER_OTLP_ENDPOINT` is set. Every journal row is also sent as an OTLP log
//! record, built in [`crate::audit::Audit::append`], the one place rows are written, so this export
//! cannot drift from the journal or from the OCSF export. A decided approval is also sent as a span
//! (created to decided). The records of one session share a trace id, so a collector can group them.
//!
//! What leaves the host is narrower than the journal. A record carries the action, phase, hash chain
//! position, session id and an allowlist of scalar detail fields (ids, a decision, an exit code, a
//! host, a method). It never carries the free-text `error` or `tail`, the approval prompt, a
//! preview, a request body, or the `subject` (which can be a recipient) unless
//! `ZYVOR_AGENT_OTLP_INCLUDE_SUBJECT=1`. The journal stays the source of truth.
//!
//! Delivery is best effort and never blocks the caller: records go through a bounded queue and are
//! dropped when it is full or the collector is down. Only `http/json` is spoken; gRPC and protobuf are not.

use crate::{
    audit::{AuditEntry, AuditPhase},
    model::{ApprovalRecord, ApprovalStatus},
};
use serde_json::{json, Value};
use std::{sync::OnceLock, time::Duration};
use tokio::sync::mpsc;

const QUEUE: usize = 1024;
const BATCH: usize = 100;
const FLUSH_EVERY: Duration = Duration::from_secs(2);
const SEND_TIMEOUT: Duration = Duration::from_secs(10);
const MAX_STRING: usize = 200;

/// Detail fields that may leave the host, and only as a string, number or boolean.
pub(crate) const DETAIL_ALLOWLIST: &[&str] = &[
    "approval_id",
    "changeset_id",
    "device_id",
    "decision",
    "exit_code",
    "channel",
    "push_kind",
    "format",
    "method",
    "host",
    "kind",
    "identity",
];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OtlpConfig {
    /// Base URL, without `/v1/logs`.
    pub endpoint: String,
    pub headers: Vec<(String, String)>,
    pub service_name: String,
    pub include_subject: bool,
}

impl OtlpConfig {
    /// Read the standard `OTEL_*` variables. `None` when export is off or the setting is unusable.
    pub fn from_env(get: impl Fn(&str) -> Option<String>) -> Option<Self> {
        let get = |k: &str| get(k).filter(|v| !v.is_empty());
        let endpoint = get("OTEL_EXPORTER_OTLP_ENDPOINT")?;
        let ok = url::Url::parse(&endpoint).is_ok_and(|u| {
            u.scheme() == "https"
                || (u.scheme() == "http"
                    && matches!(u.host_str(), Some("localhost" | "127.0.0.1" | "[::1]")))
        });
        if !ok {
            tracing::warn!("OTEL_EXPORTER_OTLP_ENDPOINT must be https (or http on loopback); OTLP export is off");
            return None;
        }
        if let Some(protocol) = get("OTEL_EXPORTER_OTLP_PROTOCOL") {
            if protocol != "http/json" {
                tracing::warn!(%protocol, "only OTEL_EXPORTER_OTLP_PROTOCOL=http/json is supported; OTLP export is off");
                return None;
            }
        }
        let headers = get("OTEL_EXPORTER_OTLP_HEADERS")
            .map(|raw| {
                raw.split(',')
                    .filter_map(|pair| pair.split_once('='))
                    .map(|(k, v)| (k.trim().to_string(), v.trim().to_string()))
                    .filter(|(k, _)| !k.is_empty())
                    .collect()
            })
            .unwrap_or_default();
        Some(Self {
            endpoint: endpoint.trim_end_matches('/').to_string(),
            headers,
            service_name: get("OTEL_SERVICE_NAME").unwrap_or_else(|| "zyvor-merion-runtime".into()),
            include_subject: get("ZYVOR_AGENT_OTLP_INCLUDE_SUBJECT").is_some_and(|v| v == "1"),
        })
    }
}

/// One thing to send.
#[derive(Debug, Clone)]
pub enum Item {
    Log(Value),
    Span(Value),
}

static QUEUE_TX: OnceLock<(mpsc::Sender<Item>, bool)> = OnceLock::new();

/// Start the exporter if the environment asks for it. Call once, inside the Tokio runtime.
pub fn init(http: reqwest::Client) {
    let Some(config) = OtlpConfig::from_env(|k| std::env::var(k).ok()) else {
        return;
    };
    let (tx, rx) = mpsc::channel(QUEUE);
    if QUEUE_TX.set((tx, config.include_subject)).is_err() {
        return;
    }
    tracing::info!(endpoint = %config.endpoint, "OTLP export on");
    tokio::spawn(run(http, config, rx));
}

/// Queue a journal row. A no-op when export is off; never blocks.
pub fn emit_entry(entry: &AuditEntry) {
    if let Some((tx, include_subject)) = QUEUE_TX.get() {
        let _ = tx.try_send(Item::Log(log_record(entry, *include_subject)));
    }
}

/// Queue a decided approval as a span. A no-op when export is off; never blocks.
pub fn emit_approval(record: &ApprovalRecord) {
    if let Some((tx, _)) = QUEUE_TX.get() {
        if let Some(span) = approval_span(record) {
            let _ = tx.try_send(Item::Span(span));
        }
    }
}

async fn run(http: reqwest::Client, config: OtlpConfig, mut rx: mpsc::Receiver<Item>) {
    let mut batch = Vec::new();
    let mut tick = tokio::time::interval(FLUSH_EVERY);
    loop {
        tokio::select! {
            item = rx.recv() => match item {
                Some(item) => {
                    batch.push(item);
                    if batch.len() >= BATCH {
                        send_batch(&http, &config, std::mem::take(&mut batch)).await;
                    }
                }
                None => break,
            },
            _ = tick.tick() => {
                if !batch.is_empty() {
                    send_batch(&http, &config, std::mem::take(&mut batch)).await;
                }
            }
        }
    }
    if !batch.is_empty() {
        send_batch(&http, &config, batch).await;
    }
}

/// POST the logs and the spans of one batch. A failure is logged once and the batch dropped.
pub async fn send_batch(http: &reqwest::Client, config: &OtlpConfig, batch: Vec<Item>) {
    let (mut logs, mut spans) = (Vec::new(), Vec::new());
    for item in batch {
        match item {
            Item::Log(v) => logs.push(v),
            Item::Span(v) => spans.push(v),
        }
    }
    if !logs.is_empty() {
        post(
            http,
            config,
            "/v1/logs",
            logs_request(&config.service_name, logs),
        )
        .await;
    }
    if !spans.is_empty() {
        post(
            http,
            config,
            "/v1/traces",
            traces_request(&config.service_name, spans),
        )
        .await;
    }
}

async fn post(http: &reqwest::Client, config: &OtlpConfig, path: &str, body: Value) {
    let mut request = http
        .post(format!("{}{path}", config.endpoint))
        .timeout(SEND_TIMEOUT)
        .json(&body);
    for (k, v) in &config.headers {
        request = request.header(k, v);
    }
    match request.send().await {
        Ok(r) if r.status().is_success() => {}
        Ok(r) => tracing::warn!(status = %r.status(), path, "OTLP export refused; batch dropped"),
        Err(error) => tracing::warn!(%error, path, "OTLP export failed; batch dropped"),
    }
}

fn kv(key: &str, value: Value) -> Value {
    let any = match value {
        Value::String(s) => json!({"stringValue": s}),
        Value::Bool(b) => json!({"boolValue": b}),
        Value::Number(n) if n.is_i64() || n.is_u64() => json!({"intValue": n.to_string()}),
        Value::Number(n) => json!({"doubleValue": n}),
        other => json!({"stringValue": other.to_string()}),
    };
    json!({"key": key, "value": any})
}

fn bounded(text: &str) -> String {
    text.chars()
        .filter(|c| !c.is_control())
        .take(MAX_STRING)
        .collect()
}

/// 32 hex characters naming the session's trace, or `None` without a session.
fn trace_id(session: Option<uuid::Uuid>) -> Option<String> {
    session.map(|s| s.simple().to_string())
}

/// 16 hex characters from an id.
fn span_id(id: uuid::Uuid) -> String {
    id.simple().to_string()[..16].to_string()
}

fn nanos(at: chrono::DateTime<chrono::Utc>) -> String {
    at.timestamp_nanos_opt().unwrap_or(0).to_string()
}

fn phase_name(phase: AuditPhase) -> String {
    serde_json::to_value(phase)
        .ok()
        .and_then(|v| v.as_str().map(str::to_owned))
        .unwrap_or_default()
}

/// One journal row as an OTLP log record.
pub fn log_record(e: &AuditEntry, include_subject: bool) -> Value {
    let phase = phase_name(e.phase);
    let (severity_number, severity_text) = match e.phase {
        AuditPhase::Denied | AuditPhase::Failed => (13, "WARN"),
        _ => (9, "INFO"),
    };
    let mut attributes = vec![
        kv("merion.action", json!(e.action)),
        kv("merion.phase", json!(phase)),
        kv("merion.seq", json!(e.seq)),
        kv("merion.hash", json!(e.hash)),
        kv("merion.prev_hash", json!(e.prev_hash)),
    ];
    if let Some(s) = e.session_id {
        attributes.push(kv("merion.session_id", json!(s.to_string())));
    }
    if include_subject {
        if let Some(subject) = &e.subject {
            attributes.push(kv("merion.subject", json!(bounded(subject))));
        }
    }
    if let Some(detail) = e.detail.as_object() {
        for key in DETAIL_ALLOWLIST {
            match detail.get(*key) {
                Some(Value::String(s)) => {
                    attributes.push(kv(&format!("merion.detail.{key}"), json!(bounded(s))))
                }
                Some(v @ (Value::Number(_) | Value::Bool(_))) => {
                    attributes.push(kv(&format!("merion.detail.{key}"), v.clone()))
                }
                _ => {}
            }
        }
    }
    let mut record = json!({
        "timeUnixNano": nanos(e.at),
        "observedTimeUnixNano": nanos(e.at),
        "severityNumber": severity_number,
        "severityText": severity_text,
        "body": {"stringValue": format!("{} {}", e.action, phase)},
        "attributes": attributes,
    });
    if let Some(t) = trace_id(e.session_id) {
        record["traceId"] = json!(t);
    }
    record
}

/// A decided approval as a span from request to decision. `None` while it is still pending.
pub fn approval_span(r: &ApprovalRecord) -> Option<Value> {
    let decided = r.decided_at?;
    let decision = match r.status {
        ApprovalStatus::Approved => "approved",
        ApprovalStatus::Denied => "denied",
        _ => return None,
    };
    Some(json!({
        "traceId": trace_id(Some(r.session_id)),
        "spanId": span_id(r.id),
        "name": format!("merion.approval {}", r.kind.as_str()),
        "kind": 1,
        "startTimeUnixNano": nanos(r.created_at),
        "endTimeUnixNano": nanos(decided),
        "attributes": [
            kv("merion.approval_id", json!(r.id.to_string())),
            kv("merion.approval_kind", json!(r.kind.as_str())),
            kv("merion.decision", json!(decision)),
            kv("merion.session_id", json!(r.session_id.to_string())),
        ],
    }))
}

fn resource(service: &str) -> Value {
    json!({"attributes": [kv("service.name", json!(service))]})
}

fn scope() -> Value {
    json!({"name": "zyvor-agent-runtime", "version": env!("CARGO_PKG_VERSION")})
}

pub fn logs_request(service: &str, records: Vec<Value>) -> Value {
    json!({"resourceLogs": [{
        "resource": resource(service),
        "scopeLogs": [{"scope": scope(), "logRecords": records}],
    }]})
}

pub fn traces_request(service: &str, spans: Vec<Value>) -> Value {
    json!({"resourceSpans": [{
        "resource": resource(service),
        "scopeSpans": [{"scope": scope(), "spans": spans}],
    }]})
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::ApprovalKind;
    use axum::{extract::State, routing::post as route, Json, Router};
    use chrono::{TimeZone, Utc};
    use std::sync::{Arc, Mutex};
    use uuid::Uuid;

    fn entry(detail: Value) -> AuditEntry {
        AuditEntry {
            seq: 7,
            at: Utc.with_ymd_and_hms(2026, 10, 27, 12, 0, 0).unwrap(),
            session_id: Some(Uuid::from_u128(0xabc)),
            phase: AuditPhase::Denied,
            action: "approval.egress".into(),
            subject: Some("person@example.com".into()),
            detail,
            prev_hash: "p".into(),
            hash: "h".into(),
        }
    }

    fn env(pairs: &'static [(&'static str, &'static str)]) -> impl Fn(&str) -> Option<String> {
        move |k| {
            pairs
                .iter()
                .find(|(n, _)| *n == k)
                .map(|(_, v)| v.to_string())
        }
    }

    #[test]
    fn config_is_off_without_an_endpoint_and_rejects_unusable_settings() {
        assert_eq!(OtlpConfig::from_env(env(&[])), None);
        assert_eq!(
            OtlpConfig::from_env(env(&[(
                "OTEL_EXPORTER_OTLP_ENDPOINT",
                "http://collector.example:4318"
            )])),
            None
        );
        assert_eq!(
            OtlpConfig::from_env(env(&[
                ("OTEL_EXPORTER_OTLP_ENDPOINT", "http://127.0.0.1:4318"),
                ("OTEL_EXPORTER_OTLP_PROTOCOL", "grpc"),
            ])),
            None
        );
    }

    #[test]
    fn config_reads_the_standard_variables() {
        let c = OtlpConfig::from_env(env(&[
            ("OTEL_EXPORTER_OTLP_ENDPOINT", "https://otel.example:4318/"),
            (
                "OTEL_EXPORTER_OTLP_HEADERS",
                "authorization=Bearer abc, x-team=ops,junk",
            ),
            ("OTEL_SERVICE_NAME", "merion-lab"),
        ]))
        .unwrap();
        assert_eq!(c.endpoint, "https://otel.example:4318");
        assert_eq!(c.service_name, "merion-lab");
        assert_eq!(
            c.headers,
            vec![
                ("authorization".into(), "Bearer abc".into()),
                ("x-team".into(), "ops".into())
            ]
        );
        assert!(!c.include_subject);
    }

    #[test]
    fn a_log_record_leaves_out_everything_not_on_the_allowlist() {
        let e = entry(json!({
            "approval_id": "a1", "exit_code": 2, "host": "api.github.com",
            "error": "SECRET-TOKEN-in-error", "tail": "SECRET-TOKEN-in-output",
            "prompt": "SECRET-PROMPT", "nested": {"host": "SECRET-NESTED"},
        }));
        let text = log_record(&e, false).to_string();
        assert!(!text.contains("SECRET"), "{text}");
        assert!(!text.contains("person@example.com"));
        assert!(text.contains("merion.detail.approval_id"));
        assert!(text.contains("api.github.com"));
        assert!(text.contains("\"intValue\":\"2\""));
        // the subject is opt-in
        assert!(log_record(&e, true)
            .to_string()
            .contains("person@example.com"));
    }

    #[test]
    fn a_log_record_has_otlp_shape_and_a_per_session_trace_id() {
        let r = log_record(&entry(json!({})), false);
        assert_eq!(r["severityText"], "WARN");
        assert_eq!(r["traceId"], Uuid::from_u128(0xabc).simple().to_string());
        assert_eq!(r["timeUnixNano"], "1793102400000000000");
        let no_session = AuditEntry {
            session_id: None,
            ..entry(json!({}))
        };
        assert!(log_record(&no_session, false).get("traceId").is_none());
    }

    #[test]
    fn a_span_exists_only_for_a_decided_approval_and_carries_no_prompt() {
        let created = Utc.with_ymd_and_hms(2026, 10, 27, 12, 0, 0).unwrap();
        let mut r = ApprovalRecord {
            id: Uuid::new_v4(),
            session_id: Uuid::new_v4(),
            kind: ApprovalKind::default(),
            subject: Some("person@example.com".into()),
            planned_action: None,
            prompt: "SECRET-PROMPT".into(),
            status: ApprovalStatus::Pending,
            comment: Some("SECRET-COMMENT".into()),
            created_at: created,
            decided_at: None,
            source_seq: None,
            broker_held: false,
            grant_scope: None,
            preview: Some(json!("SECRET-PREVIEW")),
        };
        assert!(approval_span(&r).is_none());
        r.status = ApprovalStatus::Denied;
        r.decided_at = Some(created + chrono::Duration::seconds(5));
        let span = approval_span(&r).unwrap();
        let text = span.to_string();
        assert!(!text.contains("SECRET") && !text.contains("person@example.com"));
        assert_eq!(span["endTimeUnixNano"].as_str().unwrap().len(), 19);
        assert_eq!(span["spanId"].as_str().unwrap().len(), 16);
        assert_eq!(span["traceId"].as_str().unwrap().len(), 32);
        assert!(text.contains("\"stringValue\":\"denied\""));
    }

    #[tokio::test]
    async fn a_batch_is_posted_to_logs_and_traces_with_the_configured_headers() {
        type Seen = Arc<Mutex<Vec<(String, Value)>>>;
        let seen: Seen = Default::default();
        async fn logs(State(s): State<Seen>, Json(b): Json<Value>) {
            s.lock().unwrap().push(("logs".into(), b));
        }
        async fn traces(State(s): State<Seen>, Json(b): Json<Value>) {
            s.lock().unwrap().push(("traces".into(), b));
        }
        let app = Router::new()
            .route("/v1/logs", route(logs))
            .route("/v1/traces", route(traces))
            .with_state(seen.clone());
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });

        let config = OtlpConfig {
            endpoint: format!("http://127.0.0.1:{port}"),
            headers: vec![],
            service_name: "svc".into(),
            include_subject: false,
        };
        let batch = vec![
            Item::Log(log_record(&entry(json!({"approval_id": "a1"})), false)),
            Item::Span(json!({"name": "s"})),
        ];
        send_batch(&reqwest::Client::new(), &config, batch).await;
        let seen = seen.lock().unwrap();
        assert_eq!(seen.len(), 2);
        let (_, logs) = seen.iter().find(|(p, _)| p == "logs").unwrap();
        assert_eq!(
            logs["resourceLogs"][0]["resource"]["attributes"][0]["value"]["stringValue"],
            "svc"
        );
        assert_eq!(
            logs["resourceLogs"][0]["scopeLogs"][0]["logRecords"]
                .as_array()
                .unwrap()
                .len(),
            1
        );
        assert!(seen.iter().any(|(p, b)| p == "traces"
            && b["resourceSpans"][0]["scopeSpans"][0]["spans"][0]["name"] == "s"));
    }

    #[tokio::test]
    async fn a_dead_collector_does_not_panic_or_hang_the_caller() {
        let config = OtlpConfig {
            endpoint: "http://127.0.0.1:1".into(),
            headers: vec![],
            service_name: "svc".into(),
            include_subject: false,
        };
        send_batch(
            &reqwest::Client::new(),
            &config,
            vec![Item::Span(json!({}))],
        )
        .await;
    }
}
