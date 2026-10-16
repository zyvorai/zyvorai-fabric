// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: Apache-2.0

//! A rich, read-only card an agent can show — the "outputs that live in the chat and outside it too" piece of a personal agent.
//!
//! Not an approval: a card decides nothing and grants no authority, so it never touches the "no approvals in chat" rule. It is bounded and
//! host-cleaned the way [`crate::preview`] cleans an approval's fields, and persisted twice: as a `card.rendered` session event (so it
//! plays back in the AG-UI stream as `keep.card`, [`crate::agui`]) and as an ordinary [`crate::goals::ArtifactRecord`] (so it shows up in
//! Runs/Artifacts too, the "outside the chat" half).
//!
//! **What this is not.** [`crate::preview`] renders an approval from the *actual outgoing request bytes* at the egress broker, so an agent
//! cannot lie about what it is about to send — there is an independent ground truth to check it against. A card has no such thing: it is
//! data the agent computed itself, with nothing outgoing to verify it against. The guarantee here is narrower and still real: whatever an
//! agent sends, a card can only ever become a bounded set of plain-text label/value pairs of one recognised kind — never raw HTML, a
//! script, or an unbounded blob. That is a claim about *shape*, not about *truth*.
//!
//! One kind exists so far: [`SUGGESTION_DIGEST`], a "here is what I found" summary — the first thing a proactive finder
//! ([`crate::suggestions`]) naturally has to show that is not an approval. Adding a second kind is a decision, not a default.

use crate::{
    audit::AuditPhase,
    goals::ArtifactRecord,
    model::{AgentManifest, SessionRecord},
    AppState,
};
use chrono::Utc;
use serde_json::{json, Value};
use uuid::Uuid;

pub const MAX_FIELDS: usize = 12;
pub const MAX_LABEL_CHARS: usize = 40;
pub const MAX_VALUE_CHARS: usize = 500;

/// A finder's own "here is what I found": a few items and why each is worth a look. Emitted by the same kind of agent that emits
/// `suggestion.propose` ([`crate::suggestions`]), typically alongside it, but a card decides nothing on its own.
pub const SUGGESTION_DIGEST: &str = "suggestion-digest";

fn known_kind(kind: &str) -> bool {
    kind == SUGGESTION_DIGEST
}

/// Plain text only: no control, zero-width or direction-changing characters, at most `max` characters. Same rule as
/// [`crate::preview`] and [`crate::suggestions`] use for agent-supplied or request-derived text.
fn clean(text: &str, max: usize) -> String {
    text.chars()
        .filter(|c| {
            !(c.is_control()
                || matches!(c, '\u{200B}'..='\u{200F}' | '\u{202A}'..='\u{202E}' | '\u{2060}'..='\u{2064}' | '\u{2066}'..='\u{2069}' | '\u{FEFF}'))
        })
        .take(max)
        .collect::<String>()
        .trim()
        .to_string()
}

/// A cleaned card — a known kind and 1 to [`MAX_FIELDS`] plain-text label/value pairs — or why `data` (a `card.propose` event's
/// payload) is not acceptable. Pure, so it is unit-tested without a session or a store.
pub fn render(data: &Value) -> Result<(String, Vec<(String, String)>), String> {
    let kind = data.get("kind").and_then(Value::as_str).unwrap_or_default();
    if !known_kind(kind) {
        return Err(format!("unknown card kind '{kind}'"));
    }
    let Some(list) = data.get("fields").and_then(Value::as_array) else {
        return Err("a card needs a fields list".into());
    };
    if list.is_empty() || list.len() > MAX_FIELDS {
        return Err(format!("a card has 1 to {MAX_FIELDS} fields"));
    }
    let mut fields = Vec::new();
    for (i, f) in list.iter().enumerate() {
        let label = clean(
            f.get("label").and_then(Value::as_str).unwrap_or_default(),
            MAX_LABEL_CHARS,
        );
        let value = clean(
            f.get("value").and_then(Value::as_str).unwrap_or_default(),
            MAX_VALUE_CHARS,
        );
        if label.is_empty() {
            return Err(format!("field {} has no label", i + 1));
        }
        fields.push((label, value));
    }
    Ok((kind.to_string(), fields))
}

fn markdown(kind: &str, fields: &[(String, String)]) -> String {
    let mut body = format!("# {kind}\n\n");
    for (label, value) in fields {
        body.push_str(&format!("- **{label}**: {value}\n"));
    }
    body
}

/// `card.propose` (an agent's own event, [`crate::app`]'s guest-event dispatch): refuses unless the agent's manifest asked for it
/// (`"card": true`, the same opt-in shape as [`crate::suggestions::record_proposal`] and [`crate::memory::record_proposal`]), then
/// cleans it, saves it as an artifact, and appends a `card.rendered` event with the cleaned content and the artifact id. An
/// unacceptable one is refused with a reason, never partially shown.
pub async fn record_card(
    state: &AppState,
    session: &SessionRecord,
    manifest: &AgentManifest,
    data: &Value,
) {
    if !manifest.card {
        let _ = state
            .store
            .append_event(
                session.id,
                "card.refused",
                json!({ "reason": "the agent did not ask to show cards (manifest \"card\": true)" }),
            )
            .await;
        return;
    }
    let (kind, fields) = match render(data) {
        Ok(v) => v,
        Err(reason) => {
            let _ = state
                .store
                .append_event(session.id, "card.refused", json!({ "reason": reason }))
                .await;
            return;
        }
    };
    let artifact = ArtifactRecord {
        id: Uuid::new_v4(),
        kind: "card".into(),
        title: format!("{kind}.md"),
        body: markdown(&kind, &fields),
        content_type: Some("text/markdown".into()),
        goal_id: None,
        session_id: Some(session.id),
        agent: Some(session.agent.clone()),
        metadata: json!({ "card_kind": kind }),
        created_at: Utc::now(),
        expires_at: None,
    };
    if let Err(error) = state.store.save_artifact(artifact.clone()).await {
        let _ = state
            .store
            .append_event(
                session.id,
                "card.refused",
                json!({ "reason": format!("could not save the card: {error}") }),
            )
            .await;
        return;
    }
    let field_values: Vec<Value> = fields
        .iter()
        .map(|(label, value)| json!({ "label": label, "value": value }))
        .collect();
    let _ = state
        .store
        .append_event(
            session.id,
            "card.rendered",
            json!({ "kind": kind, "fields": field_values, "artifact_id": artifact.id }),
        )
        .await;
    let _ = state
        .store
        .audit
        .append(
            Some(session.id),
            AuditPhase::Performed,
            "merion.card.rendered",
            Some(session.agent.clone()),
            json!({ "kind": kind, "artifact_id": artifact.id }),
        )
        .await;
}

#[cfg(test)]
mod tests {
    use super::*;

    fn field(label: &str, value: &str) -> Value {
        json!({ "label": label, "value": value })
    }

    #[test]
    fn a_card_is_a_known_kind_and_bounded_plain_text_fields() {
        let ok = render(&json!({
            "kind": SUGGESTION_DIGEST,
            "fields": [field("  Headphones \u{202e}", "dropped to 140"), field("Kettle", "at 45")],
        }))
        .unwrap();
        assert_eq!(ok.0, SUGGESTION_DIGEST);
        assert_eq!(
            ok.1,
            vec![
                ("Headphones".to_string(), "dropped to 140".to_string()),
                ("Kettle".to_string(), "at 45".to_string()),
            ]
        );
        for (why, bad) in [
            (
                "unknown kind",
                json!({"kind": "anything-i-like", "fields": [field("a", "b")]}),
            ),
            ("no kind at all", json!({"fields": [field("a", "b")]})),
            ("no fields list", json!({"kind": SUGGESTION_DIGEST})),
            (
                "empty fields",
                json!({"kind": SUGGESTION_DIGEST, "fields": []}),
            ),
            (
                "too many fields",
                json!({"kind": SUGGESTION_DIGEST, "fields": (0..=MAX_FIELDS).map(|i| field(&format!("f{i}"), "x")).collect::<Vec<_>>()}),
            ),
            (
                "a field with no label",
                json!({"kind": SUGGESTION_DIGEST, "fields": [field("", "x")]}),
            ),
        ] {
            assert!(render(&bad).is_err(), "{why}");
        }
        let long = render(&json!({"kind": SUGGESTION_DIGEST, "fields": [field(&"x".repeat(200), &"y".repeat(2000))]})).unwrap();
        assert_eq!(long.1[0].0.chars().count(), MAX_LABEL_CHARS);
        assert_eq!(long.1[0].1.chars().count(), MAX_VALUE_CHARS);
    }

    #[tokio::test]
    async fn a_good_card_becomes_an_event_and_an_artifact_a_bad_one_becomes_neither() {
        let (state, session) = crate::egress::ask_tests::state_and_session_cfg(|_| {}).await;
        let mut manifest =
            crate::egress::ask_tests::manifest(crate::model::EgressMode::Deny, Some(30));
        manifest.card = true;

        record_card(
            &state,
            &session,
            &manifest,
            &json!({"kind": SUGGESTION_DIGEST, "fields": [field("Headphones", "dropped to 140")]}),
        )
        .await;
        let events = state.store.events_after(session.id, 0).await.unwrap();
        let rendered = events
            .iter()
            .find(|e| e.kind == "card.rendered")
            .expect("a card.rendered event");
        assert_eq!(rendered.data["kind"], SUGGESTION_DIGEST);
        assert_eq!(
            rendered.data["fields"],
            json!([{"label": "Headphones", "value": "dropped to 140"}])
        );
        let artifact_id: Uuid =
            serde_json::from_value(rendered.data["artifact_id"].clone()).unwrap();
        let artifact = state.store.get_artifact(artifact_id).await.unwrap();
        assert_eq!(artifact.kind, "card");
        assert_eq!(artifact.session_id, Some(session.id));
        assert!(artifact.body.contains("Headphones") && artifact.body.contains("dropped to 140"));

        record_card(
            &state,
            &session,
            &manifest,
            &json!({"kind": "not-a-real-kind", "fields": [field("a", "b")]}),
        )
        .await;
        let events = state.store.events_after(session.id, 0).await.unwrap();
        assert!(
            events.iter().any(|e| e.kind == "card.refused"),
            "the bad one is refused, not silently shown"
        );
        assert_eq!(
            events.iter().filter(|e| e.kind == "card.rendered").count(),
            1,
            "and does not become a second artifact"
        );
    }

    #[tokio::test]
    async fn a_manifest_that_did_not_ask_for_cards_gets_none() {
        let (state, session) = crate::egress::ask_tests::state_and_session_cfg(|_| {}).await;
        let quiet = crate::egress::ask_tests::manifest(crate::model::EgressMode::Deny, Some(30));
        assert!(!quiet.card);
        record_card(
            &state,
            &session,
            &quiet,
            &json!({"kind": SUGGESTION_DIGEST, "fields": [field("Headphones", "dropped to 140")]}),
        )
        .await;
        let events = state.store.events_after(session.id, 0).await.unwrap();
        assert!(!events.iter().any(|e| e.kind == "card.rendered"));
        let refused = events.iter().find(|e| e.kind == "card.refused").unwrap();
        assert!(refused.data["reason"]
            .as_str()
            .unwrap()
            .contains("\"card\": true"));
        assert_eq!(
            state.store.list_artifacts().await.len(),
            0,
            "nothing was saved"
        );
    }
}
