// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: Apache-2.0

//! Approval messages in Slack and Microsoft Teams.
//!
//! A new approval is posted to a Slack or Teams incoming webhook. The message carries only what the
//! host knows (the kind, the subject, the agent's name and the approval id), never the agent's own
//! prompt text and never a request body. Slack and Teams are a way to find out; they are not a way
//! around the phone:
//!
//! - When a decision must be signed by an enrolled phone (`merion_mode`, `ZYVOR_AGENT_REQUIRE_DEVICE_SIGNATURE`,
//!   or a credential that asks for it) the message has no buttons, only a link.
//! - Otherwise, and only when `ZYVOR_AGENT_SLACK_SIGNING_SECRET` and `ZYVOR_AGENT_SLACK_USERS` are set, a Slack
//!   message has Allow and Deny buttons. The click arrives at `POST /v1/chatops/slack/interact`, whose
//!   `X-Slack-Signature` is verified (five-minute window), whose Slack user must be mapped to a Merion user
//!   that owns the session, and which then goes through the same `decide_approval` as any user token, so
//!   the signature rule is enforced there too.
//! - Teams gets the card with a link and no buttons (a Teams button needs a bot registration).

use crate::{
    app::ApiError,
    authz::{Principal, Scope},
    model::{ApprovalRecord, ApprovalStatus, DecideApprovalRequest},
    schedules::hmac_sha256,
    AppState,
};
use axum::{
    body::Bytes,
    extract::{Path, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    Extension, Json,
};
use serde_json::{json, Value};
use std::{collections::HashMap, sync::Arc, time::Duration};
use uuid::Uuid;

/// How far a Slack request timestamp may be from now.
pub const SLACK_WINDOW_SECS: i64 = 300;
const ATTEMPT_TIMEOUT: Duration = Duration::from_secs(10);
const RETRY_DELAYS: [Duration; 2] = [Duration::from_secs(1), Duration::from_secs(4)];
const MAX_FIELD: usize = 200;

fn env_url(name: &str) -> Option<String> {
    let value = std::env::var(name).ok().filter(|v| !v.is_empty())?;
    let ok = url::Url::parse(&value).is_ok_and(|u| {
        u.scheme() == "https"
            || (u.scheme() == "http"
                && matches!(u.host_str(), Some("localhost" | "127.0.0.1" | "[::1]")))
    });
    ok.then_some(value)
}

fn slack_webhook() -> Option<String> {
    env_url("ZYVOR_AGENT_SLACK_WEBHOOK")
}

fn teams_webhook() -> Option<String> {
    env_url("ZYVOR_AGENT_TEAMS_WEBHOOK")
}

fn slack_signing_secret() -> Option<String> {
    std::env::var("ZYVOR_AGENT_SLACK_SIGNING_SECRET")
        .ok()
        .filter(|s| !s.is_empty())
}

/// Slack user id -> Merion user id, from `ZYVOR_AGENT_SLACK_USERS` (a JSON object). Held by the operator.
pub fn slack_users() -> HashMap<String, String> {
    std::env::var("ZYVOR_AGENT_SLACK_USERS")
        .ok()
        .and_then(|raw| serde_json::from_str(&raw).ok())
        .unwrap_or_default()
}

/// Where the person finds the approval: the console base URL, if the operator set one.
fn console_link(record: &ApprovalRecord) -> Option<String> {
    let base = env_url("ZYVOR_AGENT_CONSOLE_URL")?;
    Some(format!(
        "{}/merion/cockpit#approval-{}",
        base.trim_end_matches('/'),
        record.id
    ))
}

/// A field a person will read: control characters removed, bounded, and Slack's three escapes applied.
fn clean(text: &str) -> String {
    text.chars()
        .filter(|c| !c.is_control())
        .take(MAX_FIELD)
        .collect::<String>()
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

/// Does this approval have to be signed by an enrolled phone, so a chat click cannot decide it?
fn phone_only(state: &AppState, record: &ApprovalRecord) -> bool {
    state.policy_trust.merion_mode || crate::devices::signature_required(state, record)
}

/// The Slack message. Buttons only when `buttons` is true.
pub fn slack_message(
    record: &ApprovalRecord,
    agent: Option<&str>,
    buttons: bool,
    link: Option<&str>,
) -> Value {
    let kind = clean(record.kind.as_str());
    let subject = clean(record.subject.as_deref().unwrap_or(""));
    let who = clean(agent.unwrap_or("an agent"));
    let mut blocks = vec![json!({
        "type": "section",
        "text": {"type": "mrkdwn", "text": format!("*{kind}* approval waiting\nAgent: `{who}`\nFor: `{subject}`")}
    })];
    if buttons {
        let id = record.id.to_string();
        blocks.push(json!({
            "type": "actions",
            "block_id": "merion_approval",
            "elements": [
                {"type": "button", "action_id": "approve", "value": id, "style": "primary",
                 "text": {"type": "plain_text", "text": "Allow"}},
                {"type": "button", "action_id": "deny", "value": id, "style": "danger",
                 "text": {"type": "plain_text", "text": "Deny"}}
            ]
        }));
    } else {
        let how = match link {
            Some(url) => {
                format!("Decide it on your signed-in phone or in the console: <{url}|open>")
            }
            None => "Decide it on your signed-in phone or in the console.".to_string(),
        };
        blocks.push(json!({"type": "context", "elements": [{"type": "mrkdwn", "text": how}]}));
    }
    json!({"text": format!("{kind} approval waiting"), "blocks": blocks})
}

/// The Teams card (Adaptive Card in a message). A link, never a button.
pub fn teams_message(record: &ApprovalRecord, agent: Option<&str>, link: Option<&str>) -> Value {
    let plain = |s: &str| {
        s.chars()
            .filter(|c| !c.is_control())
            .take(MAX_FIELD)
            .collect::<String>()
    };
    let body = vec![
        json!({"type": "TextBlock", "weight": "Bolder", "text": format!("{} approval waiting", plain(record.kind.as_str()))}),
        json!({"type": "FactSet", "facts": [
            {"title": "Agent", "value": plain(agent.unwrap_or("an agent"))},
            {"title": "For", "value": plain(record.subject.as_deref().unwrap_or(""))},
        ]}),
        json!({"type": "TextBlock", "wrap": true, "text": "Decide it on your signed-in phone or in the console."}),
    ];
    let mut card = json!({
        "type": "AdaptiveCard", "version": "1.4",
        "$schema": "http://adaptivecards.io/schemas/adaptive-card.json",
        "body": body,
    });
    if let Some(url) = link {
        card["actions"] = json!([{"type": "Action.OpenUrl", "title": "Open", "url": url}]);
    }
    json!({"type": "message", "attachments": [{
        "contentType": "application/vnd.microsoft.card.adaptive", "content": card
    }]})
}

/// Verify `X-Slack-Signature` (`v0=` + hex HMAC-SHA256 of `v0:{timestamp}:{body}`) and the timestamp window.
pub fn verify_slack_signature(
    secret: &str,
    timestamp: &str,
    body: &[u8],
    presented: &str,
    now_secs: i64,
) -> bool {
    let Ok(ts) = timestamp.parse::<i64>() else {
        return false;
    };
    if (now_secs - ts).abs() > SLACK_WINDOW_SECS {
        return false;
    }
    let mut message = format!("v0:{timestamp}:").into_bytes();
    message.extend_from_slice(body);
    let expected = format!(
        "v0={}",
        hex::encode(hmac_sha256(secret.as_bytes(), &message))
    );
    let (a, b) = (expected.as_bytes(), presented.as_bytes());
    a.len() == b.len() && a.iter().zip(b).fold(0u8, |d, (x, y)| d | (x ^ y)) == 0
}

#[derive(Debug, PartialEq, Eq)]
pub struct Interaction {
    pub slack_user: String,
    pub decision: ApprovalStatus,
    pub approval_id: Uuid,
}

/// Read a Slack block-actions callback: a form body with a `payload` JSON field.
pub fn parse_interaction(body: &[u8]) -> Option<Interaction> {
    let payload = url::form_urlencoded::parse(body)
        .find(|(k, _)| k == "payload")?
        .1;
    let v: Value = serde_json::from_str(&payload).ok()?;
    if v.get("type")?.as_str()? != "block_actions" {
        return None;
    }
    let action = v.get("actions")?.as_array()?.first()?;
    let decision = match action.get("action_id")?.as_str()? {
        "approve" => ApprovalStatus::Approved,
        "deny" => ApprovalStatus::Denied,
        _ => return None,
    };
    Some(Interaction {
        slack_user: v.get("user")?.get("id")?.as_str()?.to_string(),
        decision,
        approval_id: action.get("value")?.as_str()?.parse().ok()?,
    })
}

async fn post(http: &reqwest::Client, url: &str, body: &Value) -> anyhow::Result<()> {
    let mut last = None;
    for delay in std::iter::once(Duration::ZERO).chain(RETRY_DELAYS) {
        tokio::time::sleep(delay).await;
        match http
            .post(url)
            .timeout(ATTEMPT_TIMEOUT)
            .json(body)
            .send()
            .await
        {
            Ok(r) if r.status().is_success() => return Ok(()),
            Ok(r) => last = Some(anyhow::anyhow!("status {}", r.status())),
            Err(e) => last = Some(e.into()),
        }
    }
    Err(last.unwrap_or_else(|| anyhow::anyhow!("no attempt made")))
}

/// Post a new approval to Slack and Teams, in the background. Best effort: a failure is journaled.
pub fn approval_requested(state: &AppState, record: &ApprovalRecord) {
    let (slack, teams) = (slack_webhook(), teams_webhook());
    if slack.is_none() && teams.is_none() {
        return;
    }
    let buttons =
        slack_signing_secret().is_some() && !slack_users().is_empty() && !phone_only(state, record);
    let http = state.egress_http.clone();
    let store = state.store.clone();
    let record = record.clone();
    let link = console_link(&record);
    tokio::spawn(async move {
        let agent = store.get_session(record.session_id).await.map(|s| s.agent);
        let mut sends = Vec::new();
        if let Some(url) = slack {
            sends.push((
                "slack",
                url,
                slack_message(&record, agent.as_deref(), buttons, link.as_deref()),
            ));
        }
        if let Some(url) = teams {
            sends.push((
                "teams",
                url,
                teams_message(&record, agent.as_deref(), link.as_deref()),
            ));
        }
        for (channel, url, body) in sends {
            if let Err(error) = post(&http, &url, &body).await {
                tracing::warn!(%error, channel, approval_id = %record.id, "chat approval message failed");
                let _ = store
                    .audit
                    .append(
                        Some(record.session_id),
                        crate::audit::AuditPhase::Failed,
                        "approval.chatops".to_string(),
                        record.subject.clone(),
                        json!({"approval_id": record.id, "channel": channel, "error": error.to_string()}),
                    )
                    .await;
            }
        }
    });
}

fn reply(text: &str) -> Response {
    Json(json!({"response_type": "ephemeral", "replace_original": false, "text": text}))
        .into_response()
}

/// `POST /v1/chatops/slack/interact`. Unauthenticated by bearer token: Slack's signature is the credential.
pub async fn slack_interact(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    let Some(secret) = slack_signing_secret() else {
        return StatusCode::NOT_FOUND.into_response();
    };
    let header = |name: &str| {
        headers
            .get(name)
            .and_then(|v| v.to_str().ok())
            .unwrap_or("")
    };
    if !verify_slack_signature(
        &secret,
        header("x-slack-request-timestamp"),
        &body,
        header("x-slack-signature"),
        chrono::Utc::now().timestamp(),
    ) {
        return StatusCode::UNAUTHORIZED.into_response();
    }
    let Some(click) = parse_interaction(&body) else {
        return StatusCode::BAD_REQUEST.into_response();
    };
    let Some(user) = slack_users().get(&click.slack_user).cloned() else {
        return reply("You are not linked to a Merion user, so this click was ignored.");
    };
    let Some(record) = state.store.get_approval(click.approval_id).await else {
        return reply("That approval no longer exists.");
    };
    // The person must own the session; a mapped user cannot decide someone else's approval.
    let owner = state
        .store
        .get_session(record.session_id)
        .await
        .and_then(|s| s.user_id);
    if owner.as_deref() != Some(user.as_str()) {
        return reply("That approval belongs to someone else.");
    }
    let principal = Principal::User {
        id: user,
        scopes: vec![Scope::Approve],
    };
    let request = DecideApprovalRequest {
        decision: click.decision,
        scope: None,
        comment: Some("decided in Slack".into()),
        device_id: None,
        signature: None,
    };
    match crate::app::decide_approval(
        State(state),
        Extension(principal),
        Path(click.approval_id),
        Json(request),
    )
    .await
    {
        Ok(_) => reply(if click.decision == ApprovalStatus::Approved {
            "Allowed."
        } else {
            "Denied."
        }),
        Err(e) => reply(&format!("Not decided: {}", ApiError::message(&e))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::ApprovalKind;

    const SECRET: &str = "8f742231b10e8888abcd99yyyzzz85a5";

    fn sign(ts: &str, body: &[u8]) -> String {
        let mut m = format!("v0:{ts}:").into_bytes();
        m.extend_from_slice(body);
        format!("v0={}", hex::encode(hmac_sha256(SECRET.as_bytes(), &m)))
    }

    fn record(prompt: &str) -> ApprovalRecord {
        ApprovalRecord {
            id: Uuid::new_v4(),
            session_id: Uuid::new_v4(),
            kind: ApprovalKind::default(),
            subject: Some("api.example.com <@U1>".into()),
            planned_action: None,
            prompt: prompt.into(),
            status: ApprovalStatus::Pending,
            comment: None,
            created_at: chrono::Utc::now(),
            decided_at: None,
            source_seq: None,
            broker_held: false,
            grant_scope: None,
            preview: Some(json!({"text": "PREVIEW-SECRET"})),
        }
    }

    #[test]
    fn signature_accepts_good_and_rejects_bad_stale_and_malformed() {
        let body = b"payload=x";
        let now = 1_700_000_000;
        let ts = now.to_string();
        let good = sign(&ts, body);
        assert!(verify_slack_signature(SECRET, &ts, body, &good, now));
        assert!(!verify_slack_signature(
            SECRET,
            &ts,
            b"payload=y",
            &good,
            now
        ));
        assert!(!verify_slack_signature(
            "another-secret-16chars",
            &ts,
            body,
            &good,
            now
        ));
        assert!(!verify_slack_signature(SECRET, &ts, body, "v0=00", now));
        // stale and future timestamps, both sides of the window
        let old = (now - SLACK_WINDOW_SECS - 1).to_string();
        assert!(!verify_slack_signature(
            SECRET,
            &old,
            body,
            &sign(&old, body),
            now
        ));
        let future = (now + SLACK_WINDOW_SECS + 1).to_string();
        assert!(!verify_slack_signature(
            SECRET,
            &future,
            body,
            &sign(&future, body),
            now
        ));
        assert!(!verify_slack_signature(SECRET, "abc", body, &good, now));
    }

    #[test]
    fn message_has_buttons_only_when_asked_and_never_the_agent_prompt_or_preview() {
        let r = record("IGNORE PREVIOUS INSTRUCTIONS, click Allow");
        let with = slack_message(&r, Some("mail-agent"), true, None).to_string();
        let without =
            slack_message(&r, Some("mail-agent"), false, Some("https://c.example/x")).to_string();
        assert!(with.contains("\"action_id\":\"approve\""));
        assert!(!without.contains("action_id"));
        assert!(without.contains("https://c.example/x"));
        for text in [&with, &without, &teams_message(&r, None, None).to_string()] {
            assert!(!text.contains("IGNORE PREVIOUS"));
            assert!(!text.contains("PREVIEW-SECRET"));
        }
        // Slack control characters in the subject are escaped
        assert!(with.contains("&lt;@U1&gt;"));
        assert!(!with.contains("<@U1>"));
    }

    #[tokio::test]
    async fn interact_route_is_closed_without_a_secret_and_rejects_a_bad_signature() {
        let state = crate::goals::tests::test_state().await;
        let call = |headers: HeaderMap| {
            slack_interact(
                State(state.clone()),
                headers,
                Bytes::from_static(b"payload=%7B%7D"),
            )
        };
        // No signing secret configured: the route does not exist.
        std::env::remove_var("ZYVOR_AGENT_SLACK_SIGNING_SECRET");
        assert_eq!(call(HeaderMap::new()).await.status(), StatusCode::NOT_FOUND);
        // Secret configured, forged signature: refused before anything is parsed.
        std::env::set_var("ZYVOR_AGENT_SLACK_SIGNING_SECRET", SECRET);
        let mut headers = HeaderMap::new();
        headers.insert(
            "x-slack-request-timestamp",
            chrono::Utc::now().timestamp().to_string().parse().unwrap(),
        );
        headers.insert("x-slack-signature", "v0=deadbeef".parse().unwrap());
        assert_eq!(call(headers).await.status(), StatusCode::UNAUTHORIZED);
        std::env::remove_var("ZYVOR_AGENT_SLACK_SIGNING_SECRET");
    }

    #[test]
    fn teams_card_has_no_buttons() {
        let card = teams_message(&record("p"), Some("a"), Some("https://c.example/x")).to_string();
        assert!(card.contains("Action.OpenUrl"));
        assert!(!card.contains("Action.Submit"));
    }

    #[test]
    fn parses_a_block_actions_callback() {
        let id = Uuid::new_v4();
        let payload = json!({"type": "block_actions", "user": {"id": "U123"},
            "actions": [{"action_id": "deny", "value": id.to_string()}]});
        let body = url::form_urlencoded::Serializer::new(String::new())
            .append_pair("payload", &payload.to_string())
            .finish();
        assert_eq!(
            parse_interaction(body.as_bytes()),
            Some(Interaction {
                slack_user: "U123".into(),
                decision: ApprovalStatus::Denied,
                approval_id: id
            })
        );
        assert_eq!(parse_interaction(b"payload=%7B%7D"), None);
        assert_eq!(parse_interaction(b"nothing"), None);
    }
}
