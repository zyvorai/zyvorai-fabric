// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: Apache-2.0

//! Signed webhooks for run events (docs/merion/design/run-webhooks.md).
//!
//! Off unless `ZYVOR_AGENT_RUN_WEBHOOK` (an `https` URL, or `http` for loopback) and `ZYVOR_AGENT_RUN_WEBHOOK_SECRET`
//! are both set. When a session first reaches a final status (completed, failed, cancelled, expired), one `run.finished`
//! event is POSTed, signed the same way as the approval webhook: `x-zyvor-signature: sha256=<hex HMAC-SHA256>` and
//! `x-zyvor-event: run.finished`.
//!
//! The body names the run and how it ended, never why: no free-text error, input, output or user. A receiver that wants
//! more asks the operator API. Delivery is best effort, retried twice, and never blocks the runtime; a delivery that
//! finally fails is logged. A restart can lose an event still in the queue.

use crate::{
    model::SessionRecord,
    notify::{deliver_event, ApprovalWebhook},
};
use serde_json::json;
use std::{sync::OnceLock, time::Duration};
use tokio::sync::mpsc;

const QUEUE: usize = 256;
const RETRY_DELAYS: [Duration; 2] = [Duration::from_secs(1), Duration::from_secs(4)];

static QUEUE_TX: OnceLock<mpsc::Sender<Vec<u8>>> = OnceLock::new();

/// The webhook from the environment. `None` when off or unusable (no secret, or an unsafe URL).
pub fn from_env(get: impl Fn(&str) -> Option<String>) -> Option<ApprovalWebhook> {
    let url = get("ZYVOR_AGENT_RUN_WEBHOOK").filter(|s| !s.is_empty())?;
    let secret = get("ZYVOR_AGENT_RUN_WEBHOOK_SECRET").filter(|s| !s.is_empty())?;
    let parsed = url::Url::parse(&url).ok()?;
    let loopback = matches!(parsed.host_str(), Some("localhost" | "127.0.0.1" | "[::1]"));
    (parsed.scheme() == "https" || (parsed.scheme() == "http" && loopback))
        .then_some(ApprovalWebhook { url, secret })
}

/// Start the sender if the environment asks for it. Call once, inside the Tokio runtime.
pub fn init(http: reqwest::Client) {
    let Some(webhook) = from_env(|k| std::env::var(k).ok()) else {
        return;
    };
    let (tx, mut rx) = mpsc::channel::<Vec<u8>>(QUEUE);
    if QUEUE_TX.set(tx).is_err() {
        return;
    }
    tracing::info!("run webhook on");
    tokio::spawn(async move {
        while let Some(body) = rx.recv().await {
            if let Err(error) =
                deliver_event(&http, &webhook, "run.finished", &body, &RETRY_DELAYS).await
            {
                tracing::warn!(%error, "run webhook delivery failed");
            }
        }
    });
}

/// What the receiver gets for a finished run.
pub fn payload(s: &SessionRecord) -> Vec<u8> {
    serde_json::to_vec(&json!({
        "event": "run.finished",
        "session_id": s.id,
        "agent": s.agent,
        "agent_version": s.agent_version,
        "status": s.status,
        "started_at": s.created_at,
        "finished_at": s.updated_at,
    }))
    .unwrap_or_default()
}

/// Queue the event for a session that has just reached a final status. A no-op when the webhook is off; never blocks.
pub fn finished(s: &SessionRecord) {
    if let Some(tx) = QUEUE_TX.get() {
        let _ = tx.try_send(payload(s));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn env(pairs: &[(&str, &str)]) -> impl Fn(&str) -> Option<String> {
        let m: HashMap<String, String> = pairs
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect();
        move |k| m.get(k).cloned()
    }

    #[test]
    fn it_needs_a_secret_and_a_safe_url() {
        let s = ("ZYVOR_AGENT_RUN_WEBHOOK_SECRET", "k");
        assert!(from_env(env(&[
            ("ZYVOR_AGENT_RUN_WEBHOOK", "https://hooks.example.com/x"),
            s
        ]))
        .is_some());
        assert!(from_env(env(&[
            ("ZYVOR_AGENT_RUN_WEBHOOK", "http://127.0.0.1:9/x"),
            s
        ]))
        .is_some());
        assert!(from_env(env(&[
            ("ZYVOR_AGENT_RUN_WEBHOOK", "http://hooks.example.com/x"),
            s
        ]))
        .is_none());
        assert!(from_env(env(&[(
            "ZYVOR_AGENT_RUN_WEBHOOK",
            "https://hooks.example.com/x"
        )]))
        .is_none());
        assert!(from_env(env(&[])).is_none());
    }

    #[tokio::test]
    async fn a_finished_run_is_posted_once_signed_and_without_free_text() {
        use crate::model::SessionStatus;
        let seen: std::sync::Arc<std::sync::Mutex<Vec<(String, String, Vec<u8>)>>> =
            Default::default();
        let sink = seen.clone();
        let app = axum::Router::new().route(
            "/hook",
            axum::routing::post(move |h: axum::http::HeaderMap, body: axum::body::Bytes| {
                let sink = sink.clone();
                async move {
                    let g = |n: &str| {
                        h.get(n)
                            .and_then(|v| v.to_str().ok())
                            .unwrap_or("")
                            .to_string()
                    };
                    sink.lock().unwrap().push((
                        g("x-zyvor-event"),
                        g("x-zyvor-signature"),
                        body.to_vec(),
                    ));
                }
            }),
        );
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        std::env::set_var(
            "ZYVOR_AGENT_RUN_WEBHOOK",
            format!("http://127.0.0.1:{port}/hook"),
        );
        std::env::set_var("ZYVOR_AGENT_RUN_WEBHOOK_SECRET", "whsec");
        init(reqwest::Client::new());

        let state = crate::goals::tests::test_state().await;
        let mut record: SessionRecord = serde_json::from_value(json!({
            "id": uuid::Uuid::new_v4(), "agent": "a", "agent_version": "1", "sandbox_id": uuid::Uuid::nil(),
            "status": "running", "input": {"secret": "do-not-send"}, "created_at": chrono::Utc::now(),
            "updated_at": chrono::Utc::now(), "last_event_seq": 0, "guest_event_cursor": 0,
            "capability_token": "t", "user_id": "ana",
        }))
        .unwrap();
        record.error = Some("boom: token=abc".into());
        state.store.save_session(record.clone()).await.unwrap();
        state
            .store
            .update_session(record.id, |s| s.status = SessionStatus::Completed)
            .await
            .unwrap();
        // already final: a second update is not a second event
        state
            .store
            .update_session(record.id, |s| s.status = SessionStatus::Failed)
            .await
            .unwrap();
        tokio::time::sleep(Duration::from_millis(500)).await;

        let mine: Vec<_> = seen
            .lock()
            .unwrap()
            .iter()
            .filter(|(_, _, b)| String::from_utf8_lossy(b).contains(&record.id.to_string()))
            .cloned()
            .collect();
        assert_eq!(mine.len(), 1, "exactly one event for the run");
        let (event, signature, body) = &mine[0];
        assert_eq!(event, "run.finished");
        let want = hex::encode(crate::schedules::hmac_sha256(b"whsec", body));
        assert_eq!(signature, &format!("sha256={want}"));
        let text = String::from_utf8_lossy(body);
        for leaked in ["do-not-send", "token=abc", "ana"] {
            assert!(!text.contains(leaked), "{leaked} leaked");
        }
        assert!(text.contains("\"status\":\"completed\""), "{text}");
    }
}
