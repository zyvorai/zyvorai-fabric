// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: Apache-2.0

//! An AG-UI endpoint over Keep sessions: `POST /v1/agui`.
//!
//! Chat frameworks that speak the AG-UI protocol post a `RunAgentInput` and read a stream of events. This maps one run onto a Keep session
//! and its events, so such a client can talk to an agent that runs in a sealed cell. It adds no new authority:
//!
//! * a session is started (or, for a thread whose session is still running, steered) through the same functions as `POST /v1/sessions` and
//!   `POST /v1/sessions/{id}/steer`, so scopes, quotas, tenancy and the agent's own policy all apply unchanged;
//! * the conversation is kept in a [thread](crate::threads): both sides' messages are stored on the host, a thread whose session
//!   has ended simply starts a new session (given the earlier messages as `history`), and a client that reconnects is sent
//!   the stored conversation as a `MESSAGES_SNAPSHOT`;
//! * a user token needs the `run` scope, like starting a session;
//! * an approval the agent asks for appears as a `CUSTOM` event carrying the prompt. **There is no way to approve or deny from this
//!   endpoint**: an approval is decided on the user's device, through `/v1/approvals`, and nothing a chat client sends can do it.
//! * so is one the host is holding for the agent (a request that needs a person, such as sending mail): `keep.approval_requested` when it
//!   opens, with the host's `preview` of what would be sent, and `keep.approval_decided` when it is decided or expires. Without this the
//!   chat would show only a spinner while the agent waits for a phone. The stream belongs to the session's owner, and the preview is not stored.

use crate::{
    app::{self, ApiError, ApiResult},
    authz::Principal,
    model::{CreateSessionRequest, SessionEvent, SessionStatus, SteerRequest},
    threads::{MessageRecord, Role, MAX_PAGE},
    AppState,
};
use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::sse::{Event, KeepAlive, Sse},
    Extension, Json,
};
use serde::Deserialize;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{collections::HashSet, convert::Infallible, sync::Arc, time::Duration};

/// Approvals the host is holding for the run's session, announced to the chat as they open and as they close.
#[derive(Default)]
struct ApprovalWatch {
    open: HashSet<uuid::Uuid>,
}

impl ApprovalWatch {
    /// The events for what changed since the last call, given every approval of the session.
    fn changes(&mut self, approvals: &[crate::model::ApprovalRecord]) -> Vec<Value> {
        use crate::model::ApprovalStatus;
        let mut out = Vec::new();
        for a in approvals.iter().filter(|a| a.broker_held) {
            if a.status == ApprovalStatus::Pending {
                if self.open.insert(a.id) {
                    let mut value = json!({
                        "approval_id": a.id,
                        "kind": a.kind.as_str(),
                        "prompt": a.prompt,
                        "decide": "on your device: this endpoint cannot approve or deny",
                    });
                    if let Some(preview) = &a.preview {
                        value["preview"] = preview.clone();
                    }
                    out.push(json!({"type": "CUSTOM", "name": "keep.approval_requested", "value": value}));
                }
            } else if self.open.remove(&a.id) {
                out.push(json!({"type": "CUSTOM", "name": "keep.approval_decided",
                    "value": {"approval_id": a.id, "decision": a.status}}));
            }
        }
        out
    }
}

/// The longest user message accepted from a chat client.
const MAX_MESSAGE_BYTES: usize = 16 * 1024;

/// The AG-UI `RunAgentInput` fields Keep uses; the rest (`tools`, `context`, ...) are accepted and ignored.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RunAgentInput {
    pub thread_id: String,
    pub run_id: String,
    #[serde(default)]
    pub messages: Vec<InMessage>,
    #[serde(default)]
    pub state: Value,
    /// `forwardedProps.agent` names the deployed Keep agent to talk to.
    #[serde(default)]
    pub forwarded_props: Value,
}

#[derive(Debug, Deserialize)]
pub struct InMessage {
    #[serde(default)]
    pub role: String,
    #[serde(default)]
    pub content: Value,
}

/// The text of the latest user message: a string, or the text parts of a content array.
pub fn last_user_text(messages: &[InMessage]) -> Option<String> {
    let m = messages.iter().rev().find(|m| m.role == "user")?;
    let text = match &m.content {
        Value::String(s) => s.clone(),
        Value::Array(parts) => parts
            .iter()
            .filter(|p| p.get("type").and_then(Value::as_str) == Some("text"))
            .filter_map(|p| p.get("text").and_then(Value::as_str))
            .collect::<Vec<_>>()
            .join("\n"),
        _ => return None,
    };
    let text = text.trim().to_string();
    (!text.is_empty()).then_some(text)
}

/// The request id of the session a run starts: it depends on the thread's own id (which belongs to one user) and the run id, so a
/// client that retries a run gets the same session back and two users who pick the same run id never share one.
pub fn run_request_id(thread: uuid::Uuid, run_id: &str) -> String {
    let mut h = Sha256::new();
    h.update(run_id.as_bytes());
    let hex: String = h.finalize().iter().map(|b| format!("{b:02x}")).collect();
    format!("thr:{thread}:{}", &hex[..16])
}

/// How much of the earlier conversation a new session is given as `history`: the most recent messages, within a size budget.
const HISTORY_MESSAGES: usize = 20;
const HISTORY_BYTES: usize = 16 * 1024;

fn history_for_input(prior: &[MessageRecord]) -> Vec<Value> {
    let mut used = 0;
    let mut out: Vec<Value> = Vec::new();
    for m in prior.iter().rev().take(HISTORY_MESSAGES) {
        used += m.text.len();
        if used > HISTORY_BYTES {
            break;
        }
        out.push(json!({"role": m.role, "text": m.text}));
    }
    out.reverse();
    out
}

/// The same shape as a pack name: `^[a-z0-9][a-z0-9-]{0,39}$`.
pub(crate) fn valid_agent_name(name: &str) -> bool {
    let b = name.as_bytes();
    !b.is_empty()
        && b.len() <= 40
        && b[0] != b'-'
        && b.iter()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || *c == b'-')
}

/// What the stream is doing between events: whether an assistant text message is open, and the text of the ones already closed.
#[derive(Debug, Default)]
pub struct Mapper {
    open: bool,
    /// The message ids are `<run id>-a<n>`, so every assistant message of a thread has its own.
    n: usize,
    id: String,
    buf: String,
    closed: Vec<String>,
}

impl Mapper {
    fn close(&mut self, out: &mut Vec<Value>) {
        if self.open {
            out.push(json!({"type": "TEXT_MESSAGE_END", "messageId": self.id}));
            self.open = false;
            self.closed.push(std::mem::take(&mut self.buf));
        }
    }

    fn text(&mut self, out: &mut Vec<Value>, run_id: &str, delta: &str) {
        if !self.open {
            self.n += 1;
            self.id = format!("{run_id}-a{}", self.n);
            out.push(
                json!({"type": "TEXT_MESSAGE_START", "messageId": self.id, "role": "assistant"}),
            );
            self.open = true;
        }
        self.buf.push_str(delta);
        out.push(json!({"type": "TEXT_MESSAGE_CONTENT", "messageId": self.id, "delta": delta}));
    }

    /// The text of the assistant messages that have ended since the last call, to be stored in the thread.
    pub fn take_closed(&mut self) -> Vec<String> {
        std::mem::take(&mut self.closed)
    }

    /// The AG-UI events for one Keep session event, and whether the run is over.
    pub fn map(&mut self, ev: &SessionEvent, thread_id: &str, run_id: &str) -> (Vec<Value>, bool) {
        let mut out = Vec::new();
        let d = &ev.data;
        match ev.kind.as_str() {
            "session.log" => {
                let line = d.get("line").and_then(Value::as_str).unwrap_or_default();
                if d.get("stream").and_then(Value::as_str) == Some("stderr") {
                    out.push(json!({"type": "CUSTOM", "name": "keep.log", "value": {"stream": "stderr", "line": line}}));
                } else {
                    self.text(&mut out, run_id, &format!("{line}\n"));
                }
            }
            "approval.requested" => {
                self.close(&mut out);
                let prompt = d.get("prompt").cloned().unwrap_or(Value::Null);
                out.push(json!({"type": "CUSTOM", "name": "keep.approval_requested",
                    "value": {"prompt": prompt, "decide": "on your device: this endpoint cannot approve or deny"}}));
            }
            "session.waiting" | "session.running" => {
                out.push(json!({"type": "CUSTOM", "name": format!("keep.{}", &ev.kind["session.".len()..]), "value": d}));
            }
            // A card the host already cleaned and saved as an artifact (crate::card). Distinct from the generic `keep.event`
            // fallthrough below: this one carries only host-checked kind/fields, never whatever shape the agent actually sent.
            "card.rendered" => {
                out.push(json!({"type": "CUSTOM", "name": "keep.card",
                    "value": {"kind": d.get("kind"), "fields": d.get("fields"), "artifact_id": d.get("artifact_id")}}));
            }
            "session.result" => {
                self.close(&mut out);
                if let Some(text) = d.as_str() {
                    self.text(&mut out, run_id, text);
                    self.close(&mut out);
                }
                out.push(json!({"type": "RUN_FINISHED", "threadId": thread_id, "runId": run_id, "result": d}));
                return (out, true);
            }
            "session.failed" | "session.cancelled" | "session.expired" | "session.deleted" => {
                self.close(&mut out);
                let code = &ev.kind["session.".len()..];
                let message = d
                    .get("error")
                    .and_then(Value::as_str)
                    .map(str::to_string)
                    .unwrap_or_else(|| format!("the session ended: {code}"));
                out.push(json!({"type": "RUN_ERROR", "message": message, "code": code}));
                return (out, true);
            }
            // events the agent itself emitted (`ctx.emit`), for a UI that wants progress; the runtime's own lifecycle events are not passed on
            k if !k.starts_with("session.")
                && !k.starts_with("runtime.")
                && !k.starts_with("approval.") =>
            {
                out.push(json!({"type": "CUSTOM", "name": "keep.event", "value": {"kind": k, "data": d}}));
            }
            _ => {}
        }
        (out, false)
    }
}

fn sse(v: &Value) -> Event {
    Event::default().data(v.to_string())
}

/// Whether the client asked for `merion.*` custom event names (`forwardedProps.eventNames: "merion"`).
/// The default stays `keep.*` so clients that were built against the old names keep working.
fn wants_merion_names(forwarded_props: &Value) -> bool {
    forwarded_props.get("eventNames").and_then(Value::as_str) == Some("merion")
}

/// `sse`, with a `CUSTOM` event's `keep.` name prefix rewritten to `merion.` when the client opted in.
fn sse_as(v: &Value, merion: bool) -> Event {
    if merion && v.get("type").and_then(Value::as_str) == Some("CUSTOM") {
        if let Some(rest) = v.get("name").and_then(Value::as_str).and_then(|n| n.strip_prefix("keep.")) {
            let mut v = v.clone();
            v["name"] = Value::String(format!("merion.{rest}"));
            return sse(&v);
        }
    }
    sse(v)
}

/// `POST /v1/agui`
pub(crate) async fn agui_run(
    State(state): State<Arc<AppState>>,
    Extension(principal): Extension<Principal>,
    Json(input): Json<RunAgentInput>,
) -> ApiResult<Sse<impl futures_core::Stream<Item = Result<Event, Infallible>>>> {
    let agent = input
        .forwarded_props
        .get("agent")
        .and_then(Value::as_str)
        .filter(|a| valid_agent_name(a))
        .ok_or_else(|| {
            ApiError::bad_request("forwardedProps.agent must name a deployed Keep agent (lowercase letters, digits, '-')")
        })?
        .to_string();
    if input.thread_id.is_empty()
        || input.thread_id.len() > 200
        || input.run_id.is_empty()
        || input.run_id.len() > 200
    {
        return Err(ApiError::bad_request(
            "threadId and runId are required and at most 200 characters",
        ));
    }
    let text = last_user_text(&input.messages)
        .ok_or_else(|| ApiError::bad_request("the run needs a user message with text"))?;
    if text.len() > MAX_MESSAGE_BYTES {
        return Err(ApiError::bad_request(format!(
            "the message is over {MAX_MESSAGE_BYTES} bytes"
        )));
    }
    // The conversation is a thread owned by the caller (the operator uses the name "operator"). The client's thread id finds it
    // again; a thread belongs to one agent.
    let owner = principal.user().unwrap_or("operator").to_string();
    let title: String = text.chars().take(60).collect();
    let thread = state
        .store
        .threads
        .get_or_create(&owner, &agent, &title, Some(&input.thread_id))
        .await
        .map_err(ApiError::bad_request)?;
    if thread.agent != agent {
        return Err(ApiError::conflict(format!(
            "this thread belongs to agent '{}'",
            thread.agent
        )));
    }
    let prior = state
        .store
        .threads
        .messages(thread.id, 0, MAX_PAGE)
        .await
        .map_err(ApiError::internal)?;

    // The thread's session is steered while it runs. When there is none, or it has ended, a new session starts under the same thread
    // and is given the earlier messages. A retried run finds the session it already started (same request id) and adds nothing twice.
    let running = match thread.session_id {
        Some(sid) => state
            .store
            .get_session(sid)
            .await
            .filter(|s| !s.status.is_terminal()),
        None => None,
    };
    let request_id = run_request_id(thread.id, &input.run_id);
    let (session_id, mut cursor, is_new_message) = match running {
        Some(existing) => {
            let id = existing.id;
            // the reply of a steer is not needed; an error (the session is not running) ends this run with a clear message
            let _ = app::steer_session(
                State(state.clone()),
                Path(id),
                Json(SteerRequest {
                    message: Value::String(text.clone()),
                }),
            )
            .await?;
            (id, existing.last_event_seq, true)
        }
        None => {
            let req = CreateSessionRequest {
                agent: agent.clone(),
                input: json!({
                    "message": text,
                    "threadId": input.thread_id,
                    "state": input.state,
                    "history": history_for_input(&prior),
                }),
                ttl_seconds: None,
                request_id: Some(request_id),
                start_policy: Default::default(),
                parent_session_id: None,
                user_id: None,
            };
            let (status, Json(view)) =
                app::create_session_route(State(state.clone()), Extension(principal), Json(req))
                    .await?;
            if status != StatusCode::CREATED && status != StatusCode::OK {
                return Err(ApiError::internal(format!(
                    "unexpected status {status} creating the session"
                )));
            }
            // 200 is the same run sent again: its message is already in the thread
            (view.id, 0, status == StatusCode::CREATED)
        }
    };
    let threads = &state.store.threads;
    if is_new_message {
        threads
            .append(thread.id, Role::User, &text, Some(session_id), None)
            .await
            .map_err(ApiError::internal)?;
    }
    threads
        .set_session(thread.id, Some(session_id))
        .await
        .map_err(ApiError::internal)?;
    // what a reconnecting client needs to show the whole conversation
    let snapshot: Vec<Value> = if prior.is_empty() {
        Vec::new()
    } else {
        threads
            .messages(thread.id, 0, MAX_PAGE)
            .await
            .map_err(ApiError::internal)?
            .iter()
            .map(|m| json!({"id": m.id, "role": m.role, "content": m.text}))
            .collect()
    };

    let merion = wants_merion_names(&input.forwarded_props);
    let (client_thread, run_id, keep_thread) = (input.thread_id, input.run_id, thread.id);
    let stream = async_stream::stream! {
        yield Ok(sse_as(&json!({"type": "RUN_STARTED", "threadId": client_thread, "runId": run_id}), merion));
        if !snapshot.is_empty() {
            yield Ok(sse_as(&json!({"type": "MESSAGES_SNAPSHOT", "messages": snapshot}), merion));
        }
        let mut mapper = Mapper::default();
        let mut watch = ApprovalWatch::default();
        loop {
            for v in watch.changes(&state.store.approvals_of_session(session_id).await) { yield Ok(sse_as(&v, merion)); }
            match state.store.events_after(session_id, cursor).await {
                Ok(events) => {
                    for ev in events {
                        cursor = ev.seq;
                        let (out, done) = mapper.map(&ev, &client_thread, &run_id);
                        for v in out { yield Ok(sse_as(&v, merion)); }
                        store_closed(&state, keep_thread, session_id, cursor, &mut mapper).await;
                        if done { return; }
                    }
                }
                Err(e) => {
                    yield Ok(sse_as(&json!({"type": "RUN_ERROR", "message": e.to_string(), "code": "events"}), merion));
                    return;
                }
            }
            let terminal = state.store.get_session(session_id).await.map(|s| s.status.is_terminal() || matches!(s.status, SessionStatus::Hibernated)).unwrap_or(true);
            if terminal {
                // drain what arrived between the last read and the status change, then stop
                if let Ok(events) = state.store.events_after(session_id, cursor).await {
                    for ev in events {
                        cursor = ev.seq;
                        let (out, _) = mapper.map(&ev, &client_thread, &run_id);
                        for v in out { yield Ok(sse_as(&v, merion)); }
                        store_closed(&state, keep_thread, session_id, cursor, &mut mapper).await;
                    }
                }
                yield Ok(sse_as(&json!({"type": "RUN_ERROR", "message": "the session ended without a result", "code": "ended"}), merion));
                return;
            }
            tokio::time::sleep(Duration::from_millis(250)).await;
        }
    };
    Ok(Sse::new(stream).keep_alive(KeepAlive::default()))
}

/// Stores the assistant messages the mapper has finished as messages of the thread. A failure to store is logged, not sent to the
/// client: the run itself is fine.
async fn store_closed(
    state: &AppState,
    thread: uuid::Uuid,
    session: uuid::Uuid,
    event_seq: u64,
    mapper: &mut Mapper,
) {
    for mut text in mapper.take_closed() {
        if text.len() > crate::threads::MAX_MESSAGE_BYTES {
            let mut cut = crate::threads::MAX_MESSAGE_BYTES - 16;
            while !text.is_char_boundary(cut) {
                cut -= 1;
            }
            text.truncate(cut);
            text.push_str("\n[truncated]");
        }
        if let Err(error) = state
            .store
            .threads
            .append(
                thread,
                Role::Assistant,
                &text,
                Some(session),
                Some(event_seq),
            )
            .await
        {
            tracing::warn!(%error, %thread, "could not store an assistant message");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use uuid::Uuid;

    fn ev(kind: &str, data: Value) -> SessionEvent {
        SessionEvent {
            session_id: Uuid::nil(),
            seq: 1,
            kind: kind.into(),
            data,
            timestamp: Utc::now(),
        }
    }
    fn msg(role: &str, content: Value) -> InMessage {
        InMessage {
            role: role.into(),
            content,
        }
    }

    #[test]
    fn the_last_user_message_is_the_input() {
        let ms = vec![
            msg("user", json!("first")),
            msg("assistant", json!("hi")),
            msg("user", json!("  second  ")),
        ];
        assert_eq!(last_user_text(&ms).as_deref(), Some("second"));
        let parts = vec![msg(
            "user",
            json!([{"type": "text", "text": "a"}, {"type": "image", "url": "x"}, {"type": "text", "text": "b"}]),
        )];
        assert_eq!(last_user_text(&parts).as_deref(), Some("a\nb"));
        assert_eq!(last_user_text(&[msg("assistant", json!("x"))]), None);
        assert_eq!(last_user_text(&[msg("user", json!("   "))]), None);
        assert_eq!(last_user_text(&[msg("user", json!({"not": "text"}))]), None);
    }

    #[test]
    fn a_run_request_id_is_stable_per_thread_and_run_and_valid_for_the_runtime() {
        let (t1, t2) = (uuid::Uuid::new_v4(), uuid::Uuid::new_v4());
        assert_eq!(
            run_request_id(t1, "r1"),
            run_request_id(t1, "r1"),
            "a retry finds its session"
        );
        assert_ne!(run_request_id(t1, "r1"), run_request_id(t1, "r2"));
        assert_ne!(
            run_request_id(t1, "r1"),
            run_request_id(t2, "r1"),
            "another thread (another user) never shares it"
        );
        let id = run_request_id(t1, &"x".repeat(200));
        assert!(
            id.len() <= 128
                && id
                    .bytes()
                    .all(|c| c.is_ascii_alphanumeric() || c == b'-' || c == b':')
        );
    }

    #[test]
    fn history_is_the_recent_messages_within_a_budget() {
        let mk = |seq: u64, role: Role, text: &str| MessageRecord {
            id: format!("msg-{seq}"),
            thread_id: uuid::Uuid::nil(),
            seq,
            role,
            text: text.into(),
            created_at: Utc::now(),
            session_id: None,
            event_seq: None,
        };
        let few = vec![mk(1, Role::User, "hi"), mk(2, Role::Assistant, "hello")];
        let h = history_for_input(&few);
        assert_eq!(h[0]["role"], "user");
        assert_eq!(h[1]["text"], "hello");
        let many: Vec<_> = (1..=50)
            .map(|i| mk(i, Role::User, &format!("m{i}")))
            .collect();
        let h = history_for_input(&many);
        assert_eq!(h.len(), HISTORY_MESSAGES);
        assert_eq!(
            h.last().unwrap()["text"],
            "m50",
            "the newest are kept, oldest first"
        );
        let big = vec![
            mk(1, Role::User, &"z".repeat(HISTORY_BYTES + 1)),
            mk(2, Role::User, "small"),
        ];
        let h = history_for_input(&big);
        assert_eq!(h.len(), 1, "an over-budget message is dropped, not cut");
    }

    #[test]
    fn each_assistant_message_has_its_own_id_and_its_text_is_kept() {
        let mut m = Mapper::default();
        let (a, _) = m.map(
            &ev("session.log", json!({"stream": "stdout", "line": "one"})),
            "t",
            "run7",
        );
        assert_eq!(a[0]["messageId"], "run7-a1");
        m.map(
            &ev("approval.requested", json!({"prompt": "ok?"})),
            "t",
            "run7",
        );
        let (b, _) = m.map(
            &ev("session.log", json!({"stream": "stdout", "line": "two"})),
            "t",
            "run7",
        );
        assert_eq!(
            b[0]["messageId"], "run7-a2",
            "a message after a pause is a new message"
        );
        m.map(&ev("session.result", json!({"ok": true})), "t", "run7");
        assert_eq!(m.take_closed(), ["one\n", "two\n"]);
        assert!(m.take_closed().is_empty(), "each is handed over once");
    }

    #[test]
    fn agent_names_are_the_pack_name_shape() {
        for ok in ["csv-clean", "a", "model-agent2", "0day"] {
            assert!(valid_agent_name(ok), "{ok}");
        }
        for bad in ["", "Upper", "has space", "../x", "-lead", &"a".repeat(41)] {
            assert!(!valid_agent_name(bad), "{bad}");
        }
    }

    #[test]
    fn stdout_becomes_one_assistant_message_and_the_result_finishes_the_run() {
        let mut m = Mapper::default();
        let (a, done) = m.map(
            &ev("session.log", json!({"stream": "stdout", "line": "hello"})),
            "t",
            "r",
        );
        assert!(!done);
        assert_eq!(a[0]["type"], "TEXT_MESSAGE_START");
        assert_eq!(a[0]["role"], "assistant");
        assert_eq!(a[1]["type"], "TEXT_MESSAGE_CONTENT");
        assert_eq!(a[1]["delta"], "hello\n");
        // a second line continues the same message
        let (b, _) = m.map(
            &ev("session.log", json!({"stream": "stdout", "line": "world"})),
            "t",
            "r",
        );
        assert_eq!(b.len(), 1);
        assert_eq!(b[0]["messageId"], a[0]["messageId"]);
        let (c, done) = m.map(&ev("session.result", json!({"ok": true})), "t", "r");
        assert!(done);
        assert_eq!(c[0]["type"], "TEXT_MESSAGE_END");
        assert_eq!(c[1]["type"], "RUN_FINISHED");
        assert_eq!(c[1]["threadId"], "t");
        assert_eq!(c[1]["runId"], "r");
        assert_eq!(c[1]["result"]["ok"], true);
    }

    #[test]
    fn stderr_is_not_assistant_text_and_a_string_result_is_a_message() {
        let mut m = Mapper::default();
        let (a, _) = m.map(
            &ev("session.log", json!({"stream": "stderr", "line": "warn"})),
            "t",
            "r",
        );
        assert_eq!(a.len(), 1);
        assert_eq!(a[0]["type"], "CUSTOM");
        assert_eq!(a[0]["name"], "keep.log");
        let (b, done) = m.map(&ev("session.result", json!("the answer")), "t", "r");
        assert!(done);
        let kinds: Vec<&str> = b.iter().map(|v| v["type"].as_str().unwrap()).collect();
        assert_eq!(
            kinds,
            [
                "TEXT_MESSAGE_START",
                "TEXT_MESSAGE_CONTENT",
                "TEXT_MESSAGE_END",
                "RUN_FINISHED"
            ]
        );
        assert_eq!(b[1]["delta"], "the answer");
    }

    #[test]
    fn an_approval_request_is_shown_but_can_only_be_decided_elsewhere() {
        let mut m = Mapper::default();
        m.map(
            &ev("session.log", json!({"stream": "stdout", "line": "x"})),
            "t",
            "r",
        );
        let (a, done) = m.map(
            &ev("approval.requested", json!({"prompt": "send the email?"})),
            "t",
            "r",
        );
        assert!(!done);
        assert_eq!(
            a[0]["type"], "TEXT_MESSAGE_END",
            "an open message is closed first"
        );
        assert_eq!(a[1]["type"], "CUSTOM");
        assert_eq!(a[1]["name"], "keep.approval_requested");
        assert_eq!(a[1]["value"]["prompt"], "send the email?");
        assert!(a[1]["value"]["decide"]
            .as_str()
            .unwrap()
            .contains("cannot approve or deny"));
    }

    #[test]
    fn failures_and_cancellations_end_the_run_with_an_error() {
        for (kind, code) in [
            ("session.failed", "failed"),
            ("session.cancelled", "cancelled"),
            ("session.expired", "expired"),
        ] {
            let mut m = Mapper::default();
            let (a, done) = m.map(&ev(kind, json!({"error": "boom"})), "t", "r");
            assert!(done, "{kind}");
            assert_eq!(a.last().unwrap()["type"], "RUN_ERROR");
            assert_eq!(a.last().unwrap()["code"], code);
        }
        let mut m = Mapper::default();
        let (a, _) = m.map(&ev("session.cancelled", Value::Null), "t", "r");
        assert_eq!(a[0]["message"], "the session ended: cancelled");
    }

    #[test]
    fn lifecycle_noise_is_dropped_and_waiting_is_a_custom_event() {
        let mut m = Mapper::default();
        for quiet in [
            "session.created",
            "session.started",
            "runtime.ready",
            "session.checkpoint",
            "session.steer",
        ] {
            assert!(
                m.map(&ev(quiet, Value::Null), "t", "r").0.is_empty(),
                "{quiet}"
            );
        }
        // an event the agent emitted with ctx.emit is passed on as a CUSTOM event
        let (e, done) = m.map(&ev("echo.received", json!({"chars": 3})), "t", "r");
        assert!(!done);
        assert_eq!(e[0]["type"], "CUSTOM");
        assert_eq!(e[0]["name"], "keep.event");
        assert_eq!(e[0]["value"]["kind"], "echo.received");
        let (a, done) = m.map(&ev("session.waiting", json!({"timeout_ms": 5})), "t", "r");
        assert!(!done);
        assert_eq!(a[0]["name"], "keep.waiting");
    }

    #[test]
    fn a_rendered_card_is_its_own_custom_event_not_the_generic_one() {
        let mut m = Mapper::default();
        let (e, done) = m.map(
            &ev(
                "card.rendered",
                json!({"kind": "suggestion-digest", "fields": [{"label": "Headphones", "value": "dropped to 140"}], "artifact_id": "a1"}),
            ),
            "t",
            "r",
        );
        assert!(!done);
        assert_eq!(e[0]["type"], "CUSTOM");
        assert_eq!(e[0]["name"], "keep.card");
        assert_eq!(e[0]["value"]["kind"], "suggestion-digest");
        assert_eq!(
            e[0]["value"]["fields"],
            json!([{"label": "Headphones", "value": "dropped to 140"}])
        );
        assert_eq!(e[0]["value"]["artifact_id"], "a1");
        // a refused one (the manifest did not ask, or it failed validation) is not a card event: nothing to show
        let (e, _) = m.map(&ev("card.refused", json!({"reason": "x"})), "t", "r");
        assert_ne!(e[0]["name"], "keep.card");
    }

    #[test]
    fn an_approval_the_host_holds_is_announced_once_with_its_preview_and_again_when_decided() {
        use crate::model::{ApprovalKind, ApprovalRecord, ApprovalStatus};
        let mk = |broker_held: bool| ApprovalRecord {
            id: uuid::Uuid::new_v4(),
            session_id: uuid::Uuid::new_v4(),
            kind: ApprovalKind::Send,
            subject: Some("gmail.googleapis.com".into()),
            planned_action: Some(json!({"body_sha256": "abc"})),
            prompt: "Agent wants to POST".into(),
            status: ApprovalStatus::Pending,
            comment: None,
            created_at: chrono::Utc::now(),
            decided_at: None,
            source_seq: None,
            broker_held,
            grant_scope: None,
            preview: Some(
                json!({"kind": "gmail-message", "fields": [{"label": "To", "value": "ana@example.com"}]}),
            ),
        };
        let mut watch = ApprovalWatch::default();
        let mut held = mk(true);
        let agent_asked = mk(false);
        let first = watch.changes(&[held.clone(), agent_asked.clone()]);
        assert_eq!(
            first.len(),
            1,
            "only the host-held one; the agent's own request comes from its events"
        );
        assert_eq!(first[0]["name"], "keep.approval_requested");
        assert_eq!(
            first[0]["value"]["preview"]["fields"][0]["value"],
            "ana@example.com"
        );
        assert!(
            first[0]["value"].get("planned_action").is_none(),
            "no request details beyond the preview"
        );
        assert!(watch.changes(&[held.clone()]).is_empty(), "announced once");
        held.status = ApprovalStatus::Denied;
        held.preview = None;
        let closed = watch.changes(&[held.clone()]);
        assert_eq!(closed.len(), 1);
        assert_eq!(closed[0]["name"], "keep.approval_decided");
        assert_eq!(closed[0]["value"]["decision"], "denied");
        assert!(watch.changes(&[held]).is_empty(), "and closed once");
        let mut fresh = ApprovalWatch::default();
        let mut never_seen = mk(true);
        never_seen.status = ApprovalStatus::Approved;
        assert!(
            fresh.changes(&[never_seen]).is_empty(),
            "one decided before the chat looked says nothing"
        );
    }

    #[test]
    fn custom_event_names_follow_the_clients_opt_in() {
        assert!(!wants_merion_names(&json!({"agent": "a"})));
        assert!(!wants_merion_names(&json!({"eventNames": "keep"})));
        assert!(wants_merion_names(&json!({"eventNames": "merion"})));
        let data = |e: Event| format!("{e:?}");
        let ev = json!({"type": "CUSTOM", "name": "keep.approval_requested", "value": {}});
        assert!(data(sse_as(&ev, true)).contains("merion.approval_requested"));
        assert!(data(sse_as(&ev, false)).contains("keep.approval_requested"));
        // Only CUSTOM events are renamed; other event types pass through untouched.
        let text = json!({"type": "TEXT_MESSAGE_CONTENT", "name": "keep.x"});
        assert!(data(sse_as(&text, true)).contains("keep.x"));
    }
}
