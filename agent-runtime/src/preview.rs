// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: LicenseRef-Zyvor-Production-2.0

//! What a person is shown before they approve a request that sends something.
//!
//! An approval that says only "POST to gmail.googleapis.com, 812 bytes, sha256 ..." asks the person to sign something they cannot read. A
//! credential descriptor may name a `preview` kind; the host then reads the **actual request body** and renders the parts that matter (who
//! gets the mail, the subject and text; when the event is, who is invited). The agent supplies none of it, so it cannot describe one thing
//! and send another.
//!
//! * **Fail closed.** If the body cannot be rendered faithfully (multipart or HTML mail, an unknown encoding, JSON that is not what the API
//!   takes), the request is refused before anything is sent. There is no "approve without seeing it" fallback.
//! * **Bounded and plain.** Every value is length-limited and stripped of control and direction-changing characters, so a subject cannot
//!   be made to display differently from what it is.
//! * **Kept off the permanent record.** The rendering lives on the approval only while it is pending, and never goes into the audit
//!   journal, an operator webhook or a push relay (those carry only the generic prompt). The journal gets `preview_sha256`, and that hash is
//!   part of what the phone signs.

use anyhow::{bail, Context, Result};
use base64::Engine;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};

pub const GMAIL_MESSAGE: &str = "gmail-message";
pub const CALENDAR_EVENT: &str = "calendar-event";
/// Microsoft Graph: a message to send or to save as a draft, and an event to create.
pub const GRAPH_MESSAGE: &str = "graph-message";
pub const GRAPH_EVENT: &str = "graph-event";

const MAX_VALUE_CHARS: usize = 1000;
const MAX_TEXT_CHARS: usize = 1500;
const MAX_MAIL_BYTES: usize = 256 * 1024;
const MAX_EVENT_BYTES: usize = 64 * 1024;

/// Whether `kind` is a preview this host can render.
pub fn is_known(kind: &str) -> bool {
    matches!(
        kind,
        GMAIL_MESSAGE | CALENDAR_EVENT | GRAPH_MESSAGE | GRAPH_EVENT
    )
}

/// A rendered request: a kind and labelled, plain-text fields in a fixed order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Preview {
    pub kind: &'static str,
    pub fields: Vec<(String, String)>,
}

impl Preview {
    pub fn to_value(&self) -> Value {
        json!({
            "kind": self.kind,
            "fields": self.fields.iter().map(|(l, v)| json!({"label": l, "value": v})).collect::<Vec<_>>(),
        })
    }

    /// Digest of the rendering. It goes into the planned action, so the device signature covers what was shown.
    pub fn sha256(&self) -> String {
        hex::encode(Sha256::digest(self.to_value().to_string().as_bytes()))
    }

    fn push(&mut self, label: &str, value: impl AsRef<str>) {
        self.fields
            .push((label.to_string(), clean(value.as_ref(), MAX_VALUE_CHARS)));
    }
}

/// Renders the request body of `kind`. `query` is the URL's query string, which can change what a request does (a calendar
/// event is emailed to its guests only when `sendUpdates` says so).
pub fn render(kind: &str, body: &[u8], query: Option<&str>) -> Result<Preview> {
    match kind {
        GMAIL_MESSAGE => gmail_message(body),
        CALENDAR_EVENT => calendar_event(body, query),
        GRAPH_MESSAGE => graph_message(body),
        GRAPH_EVENT => graph_event(body),
        other => bail!("unknown preview kind '{other}'"),
    }
}

/// Keeps only what is safe to show: no control characters (newlines and tabs stay), no zero-width or direction-changing
/// characters, and at most `max` characters.
fn clean(text: &str, max: usize) -> String {
    let mut out = String::new();
    let mut count = 0usize;
    let mut cut = false;
    for c in text.chars() {
        let bad = (c.is_control() && c != '\n' && c != '\t')
            || matches!(c, '\u{200B}'..='\u{200F}' | '\u{202A}'..='\u{202E}' | '\u{2060}'..='\u{2064}' | '\u{2066}'..='\u{2069}' | '\u{FEFF}' | '\u{00AD}');
        if bad {
            continue;
        }
        if count == max {
            cut = true;
            break;
        }
        out.push(c);
        count += 1;
    }
    if cut {
        out.push('…');
    }
    out
}

fn json_body(body: &[u8], limit: usize) -> Result<Value> {
    if body.len() > limit {
        bail!(
            "the request is too large to review ({} bytes; the limit is {limit})",
            body.len()
        );
    }
    serde_json::from_slice(body).context("the request body is not JSON")
}

fn gmail_message(body: &[u8]) -> Result<Preview> {
    let v = json_body(body, MAX_MAIL_BYTES * 2)?;
    let nested = v.get("message").and_then(|m| m.get("raw"));
    let top = v.get("raw");
    let raw = match (nested, top) {
        (Some(Value::String(s)), None) | (None, Some(Value::String(s))) => s,
        (Some(_), Some(_)) => bail!("the request has two raw messages"),
        _ => bail!("the request has no raw message to review"),
    };
    let bytes = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(raw.trim_end_matches('='))
        .context("the raw message is not base64url")?;
    if bytes.len() > MAX_MAIL_BYTES {
        bail!("the message is too large to review ({} bytes)", bytes.len());
    }
    let text = String::from_utf8(bytes).context("the message is not UTF-8 text")?;
    let (head, message) = split_headers(&text);
    let headers = parse_headers(head);

    let mut p = Preview {
        kind: GMAIL_MESSAGE,
        fields: vec![],
    };
    let mut content_type = None;
    let mut encoding = None;
    let mut other: Vec<String> = vec![];
    let mut seen_recipient = false;
    for (name, value) in &headers {
        let lower = name.to_ascii_lowercase();
        match lower.as_str() {
            "to" | "cc" | "bcc" | "reply-to" | "from" | "subject" => {
                if matches!(lower.as_str(), "to" | "cc" | "bcc") {
                    seen_recipient = true;
                }
                let label = match lower.as_str() {
                    "to" => "To",
                    "cc" => "Cc",
                    "bcc" => "Bcc (hidden from the others)",
                    "reply-to" => "Replies go to",
                    "from" => "From",
                    _ => "Subject",
                };
                p.push(label, decode_words(value));
            }
            "content-type" => content_type = Some(value.clone()),
            "content-transfer-encoding" => encoding = Some(value.clone()),
            "date" | "message-id" | "mime-version" | "in-reply-to" | "references" => {}
            _ => other.push(name.clone()),
        }
    }
    if !seen_recipient {
        p.fields.insert(0, ("To".into(), "(no recipient)".into()));
    }
    if let Some(ct) = content_type {
        let ct = ct.to_ascii_lowercase();
        let mut parts = ct.split(';').map(str::trim);
        if parts.next() != Some("text/plain") {
            bail!("only plain-text mail can be reviewed here (multipart, HTML and attachments are not supported)");
        }
        for param in parts {
            if let Some(cs) = param.strip_prefix("charset=") {
                if !matches!(cs.trim_matches('"'), "utf-8" | "us-ascii") {
                    bail!("only UTF-8 or ASCII plain-text mail can be reviewed here");
                }
            }
        }
    }
    if let Some(enc) = encoding {
        if !matches!(enc.trim().to_ascii_lowercase().as_str(), "7bit" | "8bit") {
            bail!("only 7bit or 8bit plain-text mail can be reviewed here");
        }
    }
    if !other.is_empty() {
        p.push("Other headers", other.join(", "));
    }
    let total = message.chars().count();
    let shown = clean(message.trim(), MAX_TEXT_CHARS);
    p.fields.push(("Message".into(), shown));
    if total > MAX_TEXT_CHARS {
        p.push(
            "Length",
            format!("{total} characters; only the first {MAX_TEXT_CHARS} are shown"),
        );
    }
    Ok(p)
}

/// The header block and the body, split at the first blank line.
fn split_headers(text: &str) -> (&str, &str) {
    let crlf = text.find("\r\n\r\n").map(|i| (i, 4));
    let lf = text.find("\n\n").map(|i| (i, 2));
    match (crlf, lf) {
        (Some(a), Some(b)) => {
            let (i, n) = if a.0 <= b.0 { a } else { b };
            (&text[..i], &text[i + n..])
        }
        (Some((i, n)), None) | (None, Some((i, n))) => (&text[..i], &text[i + n..]),
        (None, None) => (text, ""),
    }
}

/// Header lines with continuation lines folded in, in order, repeats kept.
fn parse_headers(head: &str) -> Vec<(String, String)> {
    let mut out: Vec<(String, String)> = vec![];
    for line in head.lines() {
        if line.starts_with([' ', '\t']) {
            if let Some(last) = out.last_mut() {
                last.1.push(' ');
                last.1.push_str(line.trim());
            }
        } else if let Some((name, value)) = line.split_once(':') {
            out.push((name.trim().to_string(), value.trim().to_string()));
        }
    }
    out
}

/// Decodes RFC 2047 `=?utf-8?B?...?=` and `=?utf-8?Q?...?=` words. Anything else is left as written.
fn decode_words(value: &str) -> String {
    let mut out = String::new();
    let mut rest = value;
    let mut after_word = false;
    while let Some(start) = rest.find("=?") {
        // Whitespace between two encoded words is not part of the text (RFC 2047 section 6.2).
        if !(after_word && rest[..start].trim().is_empty()) {
            out.push_str(&rest[..start]);
        }
        let after = &rest[start + 2..];
        let parsed = (|| {
            let mut it = after.splitn(3, '?');
            let charset = it.next()?;
            let enc = it.next()?;
            let tail = it.next()?;
            let end = tail.find("?=")?;
            if !matches!(charset.to_ascii_lowercase().as_str(), "utf-8" | "us-ascii") {
                return None;
            }
            let data = &tail[..end];
            let bytes = match enc {
                "B" | "b" => base64::engine::general_purpose::STANDARD
                    .decode(data)
                    .ok()?,
                "Q" | "q" => {
                    let raw = data.as_bytes();
                    let mut b = vec![];
                    let mut i = 0;
                    while i < raw.len() {
                        match raw[i] {
                            b'_' => b.push(b' '),
                            b'=' if i + 2 < raw.len() => {
                                let hex = std::str::from_utf8(&raw[i + 1..i + 3]).ok()?;
                                b.push(u8::from_str_radix(hex, 16).ok()?);
                                i += 2;
                            }
                            b'=' => return None,
                            c => b.push(c),
                        }
                        i += 1;
                    }
                    b
                }
                _ => return None,
            };
            let text = String::from_utf8(bytes).ok()?;
            Some((
                text,
                start + 2 + charset.len() + 1 + enc.len() + 1 + end + 2,
            ))
        })();
        match parsed {
            Some((text, consumed)) => {
                out.push_str(&text);
                rest = &rest[consumed..];
                after_word = true;
            }
            None => {
                out.push_str("=?");
                rest = &rest[start + 2..];
                after_word = false;
            }
        }
    }
    out.push_str(rest);
    out
}

fn calendar_event(body: &[u8], query: Option<&str>) -> Result<Preview> {
    let v = json_body(body, MAX_EVENT_BYTES)?;
    let Some(event) = v.as_object() else {
        bail!("the request is not a calendar event");
    };
    let text = |key: &str| -> Result<Option<String>> {
        match event.get(key) {
            None | Some(Value::Null) => Ok(None),
            Some(Value::String(s)) => Ok(Some(s.clone())),
            Some(_) => bail!("the event's '{key}' is not text"),
        }
    };
    let when = |key: &str| -> Result<String> {
        let Some(Value::Object(o)) = event.get(key) else {
            bail!("the event has no '{key}' time");
        };
        let at = match (o.get("dateTime"), o.get("date")) {
            (Some(Value::String(s)), None) => s.clone(),
            (None, Some(Value::String(s))) => format!("{s} (all day)"),
            _ => bail!("the event's '{key}' time is neither a dateTime nor a date"),
        };
        Ok(match o.get("timeZone") {
            Some(Value::String(z)) => format!("{at} {z}"),
            _ => at,
        })
    };

    let mut p = Preview {
        kind: CALENDAR_EVENT,
        fields: vec![],
    };
    p.push(
        "Title",
        text("summary")?.unwrap_or_else(|| "(no title)".into()),
    );
    p.push("Starts", when("start")?);
    p.push("Ends", when("end")?);
    if let Some(loc) = text("location")? {
        p.push("Where", loc);
    }
    match event.get("attendees") {
        None | Some(Value::Null) => {}
        Some(Value::Array(list)) => {
            let mut emails = vec![];
            for a in list {
                match a.get("email") {
                    Some(Value::String(e)) => emails.push(e.clone()),
                    _ => bail!("a guest has no email address"),
                }
            }
            if !emails.is_empty() {
                p.push("Guests", emails.join(", "));
            }
        }
        Some(_) => bail!("the event's 'attendees' is not a list"),
    }
    let notify = query_value(query, "sendUpdates");
    let legacy = query_value(query, "sendNotifications");
    let guests = event
        .get("attendees")
        .and_then(Value::as_array)
        .is_some_and(|a| !a.is_empty());
    if guests {
        p.push(
            "Guests are emailed",
            match (notify.as_deref(), legacy.as_deref()) {
                (Some("all"), _) => "yes, all of them",
                (Some("externalOnly"), _) => "yes, guests outside your organization",
                (Some("none"), _) => "no",
                (None, Some("true")) => "yes, all of them",
                _ => "no",
            },
        );
    }
    if let Some(rule) = event.get("recurrence") {
        p.push("Repeats", rule.to_string());
    }
    if let Some(d) = text("description")? {
        p.push("Notes", clean(&d, 600));
    }
    let extra: Vec<&str> = event
        .keys()
        .map(String::as_str)
        .filter(|k| {
            !matches!(
                *k,
                "summary"
                    | "start"
                    | "end"
                    | "location"
                    | "attendees"
                    | "recurrence"
                    | "description"
            )
        })
        .collect();
    if !extra.is_empty() {
        p.push("Also sets", extra.join(", "));
    }
    Ok(p)
}

/// `{"emailAddress": {"address": ..., "name": ...}}` entries as `address (name)`, the address first because the name is free text.
fn graph_recipients(v: &Value, what: &str) -> Result<Vec<String>> {
    let Some(list) = v.as_array() else {
        bail!("'{what}' is not a list");
    };
    let mut out = Vec::new();
    for r in list {
        let Some(addr) = r.pointer("/emailAddress/address").and_then(Value::as_str) else {
            bail!("a recipient in '{what}' has no email address");
        };
        out.push(
            match r.pointer("/emailAddress/name").and_then(Value::as_str) {
                Some(n) if !n.trim().is_empty() && n != addr => format!("{addr} ({n})"),
                _ => addr.to_string(),
            },
        );
    }
    Ok(out)
}

/// A Graph `body` object: plain text only. HTML cannot be shown faithfully, so it is refused.
fn graph_text_body(v: &Value, what: &str) -> Result<String> {
    match v {
        Value::Null => Ok(String::new()),
        Value::Object(o) => {
            let kind = o
                .get("contentType")
                .and_then(Value::as_str)
                .unwrap_or("text");
            if !kind.eq_ignore_ascii_case("text") {
                bail!("only plain-text {what} can be reviewed here (not {kind})");
            }
            Ok(o.get("content")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string())
        }
        _ => bail!("the {what} body is not an object"),
    }
}

fn graph_message(body: &[u8]) -> Result<Preview> {
    let v = json_body(body, MAX_MAIL_BYTES)?;
    // sendMail wraps the message ({"message": {...}, "saveToSentItems": ...}); creating a draft posts the message itself
    let (m, wrapped) = match v.get("message") {
        Some(inner) => (inner, true),
        None => (&v, false),
    };
    let Some(msg) = m.as_object() else {
        bail!("the request has no message to review");
    };
    if msg.is_empty() {
        bail!("the request has no message to review");
    }
    if wrapped
        && v.as_object()
            .is_some_and(|o| o.keys().any(|k| k != "message" && k != "saveToSentItems"))
    {
        bail!("the request has fields next to the message that cannot be reviewed");
    }
    if msg.contains_key("attachments") {
        bail!("a message with attachments cannot be reviewed here");
    }
    let mut p = Preview {
        kind: GRAPH_MESSAGE,
        fields: vec![],
    };
    let mut any_recipient = false;
    for (key, label) in [
        ("toRecipients", "To"),
        ("ccRecipients", "Cc"),
        ("bccRecipients", "Bcc (hidden from the others)"),
        ("replyTo", "Replies go to"),
    ] {
        if let Some(list) = msg.get(key) {
            let r = graph_recipients(list, key)?;
            any_recipient |= !r.is_empty() && key != "replyTo";
            if !r.is_empty() {
                p.push(label, r.join(", "));
            }
        }
    }
    if !any_recipient {
        p.fields.insert(0, ("To".into(), "(no recipient)".into()));
    }
    for key in ["from", "sender"] {
        if let Some(f) = msg.get(key) {
            p.push(
                if key == "from" { "From" } else { "Sender" },
                graph_recipients(&Value::Array(vec![f.clone()]), key)?.join(", "),
            );
        }
    }
    p.push(
        "Subject",
        msg.get("subject")
            .and_then(Value::as_str)
            .unwrap_or_default(),
    );
    let text = graph_text_body(msg.get("body").unwrap_or(&Value::Null), "mail")?;
    let total = text.chars().count();
    p.fields
        .push(("Message".into(), clean(text.trim(), MAX_TEXT_CHARS)));
    if total > MAX_TEXT_CHARS {
        p.push(
            "Length",
            format!("{total} characters; only the first {MAX_TEXT_CHARS} are shown"),
        );
    }
    let other: Vec<&str> = msg
        .keys()
        .map(String::as_str)
        .filter(|k| {
            !matches!(
                *k,
                "subject"
                    | "body"
                    | "toRecipients"
                    | "ccRecipients"
                    | "bccRecipients"
                    | "replyTo"
                    | "from"
                    | "sender"
                    | "importance"
            )
        })
        .collect();
    if !other.is_empty() {
        p.push("Also sets", other.join(", "));
    }
    if let Some(i) = msg.get("importance").and_then(Value::as_str) {
        p.push("Importance", i);
    }
    Ok(p)
}

fn graph_event(body: &[u8]) -> Result<Preview> {
    let v = json_body(body, MAX_EVENT_BYTES)?;
    let Some(event) = v.as_object() else {
        bail!("the request is not a calendar event");
    };
    let when = |key: &str| -> Result<String> {
        let Some(o) = event.get(key).and_then(Value::as_object) else {
            bail!("the event has no '{key}' time");
        };
        let Some(at) = o.get("dateTime").and_then(Value::as_str) else {
            bail!("the event's '{key}' time has no dateTime");
        };
        Ok(match o.get("timeZone").and_then(Value::as_str) {
            Some(z) => format!("{at} {z}"),
            None => at.to_string(),
        })
    };
    let mut p = Preview {
        kind: GRAPH_EVENT,
        fields: vec![],
    };
    p.push(
        "Title",
        event
            .get("subject")
            .and_then(Value::as_str)
            .filter(|s| !s.trim().is_empty())
            .unwrap_or("(no title)"),
    );
    p.push("Starts", when("start")?);
    p.push("Ends", when("end")?);
    if event.get("isAllDay").and_then(Value::as_bool) == Some(true) {
        p.push("All day", "yes");
    }
    if let Some(loc) = event.get("location") {
        let name = loc
            .get("displayName")
            .and_then(Value::as_str)
            .ok_or_else(|| anyhow::anyhow!("the event's location has no displayName"))?;
        p.push("Where", name);
    }
    let mut guests = 0usize;
    if let Some(list) = event.get("attendees") {
        let r = graph_recipients(list, "attendees")?;
        guests = r.len();
        if guests > 0 {
            p.push("Guests", r.join(", "));
        }
    }
    if guests > 0 {
        // Graph emails the invitation to every attendee when the event is created; there is no switch for it.
        p.push(
            "Guests are emailed",
            "yes, all of them (Microsoft sends the invitation when the event is created)",
        );
    }
    if event.get("isOnlineMeeting").and_then(Value::as_bool) == Some(true) {
        p.push("Online meeting", "yes, a meeting link is added");
    }
    if let Some(rule) = event.get("recurrence").filter(|r| !r.is_null()) {
        p.push("Repeats", rule.to_string());
    }
    if let Some(b) = event.get("body") {
        p.push("Notes", clean(&graph_text_body(b, "event notes")?, 600));
    }
    let extra: Vec<&str> = event
        .keys()
        .map(String::as_str)
        .filter(|k| {
            !matches!(
                *k,
                "subject"
                    | "start"
                    | "end"
                    | "isAllDay"
                    | "location"
                    | "attendees"
                    | "isOnlineMeeting"
                    | "recurrence"
                    | "body"
            )
        })
        .collect();
    if !extra.is_empty() {
        p.push("Also sets", extra.join(", "));
    }
    Ok(p)
}

fn query_value(query: Option<&str>, key: &str) -> Option<String> {
    url::form_urlencoded::parse(query?.as_bytes())
        .find(|(k, _)| k == key)
        .map(|(_, v)| v.into_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn raw(message: &str) -> Vec<u8> {
        let enc = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(message);
        json!({"message": {"raw": enc}}).to_string().into_bytes()
    }

    fn field<'a>(p: &'a Preview, label: &str) -> Vec<&'a str> {
        p.fields
            .iter()
            .filter(|(l, _)| l == label)
            .map(|(_, v)| v.as_str())
            .collect()
    }

    #[test]
    fn a_plain_message_shows_who_gets_it_the_subject_and_the_text() {
        let p = render(
            GMAIL_MESSAGE,
            &raw("To: ana@example.com\r\nCc: ben@example.com\r\nSubject: Lunch?\r\nContent-Type: text/plain; charset=utf-8\r\n\r\nAre you free at noon?"),
            None,
        )
        .unwrap();
        assert_eq!(field(&p, "To"), ["ana@example.com"]);
        assert_eq!(field(&p, "Cc"), ["ben@example.com"]);
        assert_eq!(field(&p, "Subject"), ["Lunch?"]);
        assert_eq!(field(&p, "Message"), ["Are you free at noon?"]);
    }

    #[test]
    fn the_top_level_raw_of_a_send_is_read_too() {
        let enc = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode("To: a@b.co\n\nhi");
        let body = json!({"raw": enc}).to_string();
        let p = render(GMAIL_MESSAGE, body.as_bytes(), None).unwrap();
        assert_eq!(field(&p, "To"), ["a@b.co"]);
    }

    #[test]
    fn hidden_recipients_replies_and_unusual_headers_are_shown_not_skipped() {
        let p = render(
            GMAIL_MESSAGE,
            &raw("To: a@b.co\nBcc: spy@evil.test\nReply-To: other@evil.test\nX-Track: 1\nSubject: s\n\nbody"),
            None,
        )
        .unwrap();
        assert_eq!(field(&p, "Bcc (hidden from the others)"), ["spy@evil.test"]);
        assert_eq!(field(&p, "Replies go to"), ["other@evil.test"]);
        assert_eq!(field(&p, "Other headers"), ["X-Track"]);
    }

    #[test]
    fn a_repeated_recipient_header_is_shown_every_time_and_folded_lines_are_joined() {
        let p = render(
            GMAIL_MESSAGE,
            &raw("To: a@b.co\nTo: sneaky@evil.test\nSubject: a very\n long subject\n\nx"),
            None,
        )
        .unwrap();
        assert_eq!(field(&p, "To"), ["a@b.co", "sneaky@evil.test"]);
        assert_eq!(field(&p, "Subject"), ["a very long subject"]);
    }

    #[test]
    fn encoded_words_are_decoded_so_the_person_reads_what_the_recipient_reads() {
        let subject = base64::engine::general_purpose::STANDARD.encode("Café ☕");
        let p = render(
            GMAIL_MESSAGE,
            &raw(&format!(
                "To: a@b.co\nSubject: =?UTF-8?B?{subject}?= today\n\nx"
            )),
            None,
        )
        .unwrap();
        assert_eq!(field(&p, "Subject"), ["Café ☕ today"]);
        let words = format!(
            "=?UTF-8?B?{}?= =?UTF-8?B?{}?=",
            base64::engine::general_purpose::STANDARD.encode("Über"),
            base64::engine::general_purpose::STANDARD.encode("gabe")
        );
        let p = render(
            GMAIL_MESSAGE,
            &raw(&format!("To: a@b.co\nSubject: {words}\n\nx")),
            None,
        )
        .unwrap();
        assert_eq!(
            field(&p, "Subject"),
            ["Übergabe"],
            "the space between two encoded words is not text"
        );
        let p = render(
            GMAIL_MESSAGE,
            &raw("To: a@b.co\nSubject: =?utf-8?Q?a_b=C3=A9?=\n\nx"),
            None,
        )
        .unwrap();
        assert_eq!(field(&p, "Subject"), ["a bé"]);
        let p = render(
            GMAIL_MESSAGE,
            &raw("To: a@b.co\nSubject: =?utf-8?Q?trunc=C3?=\n\nx"),
            None,
        )
        .unwrap();
        assert!(
            field(&p, "Subject")[0].contains("=?"),
            "a broken word is shown as written"
        );
    }

    #[test]
    fn what_cannot_be_shown_faithfully_is_refused() {
        for (why, msg) in [
            ("html", "To: a@b.co\nContent-Type: text/html\n\n<b>x</b>"),
            (
                "multipart",
                "To: a@b.co\nContent-Type: multipart/mixed; boundary=z\n\n--z\n",
            ),
            (
                "other charset",
                "To: a@b.co\nContent-Type: text/plain; charset=iso-8859-1\n\nx",
            ),
            (
                "base64 body",
                "To: a@b.co\nContent-Transfer-Encoding: base64\n\neA==",
            ),
            (
                "quoted-printable",
                "To: a@b.co\nContent-Transfer-Encoding: quoted-printable\n\nx=3D",
            ),
        ] {
            assert!(render(GMAIL_MESSAGE, &raw(msg), None).is_err(), "{why}");
        }
        assert!(render(GMAIL_MESSAGE, b"not json", None).is_err());
        assert!(
            render(GMAIL_MESSAGE, br#"{"id":"draft-1"}"#, None).is_err(),
            "a draft send has no message to show"
        );
        let big = raw(&format!("To: a@b.co\n\n{}", "x".repeat(MAX_MAIL_BYTES)));
        assert!(render(GMAIL_MESSAGE, &big, None).is_err());
        assert!(render("no-such-kind", b"{}", None).is_err());
    }

    #[test]
    fn hidden_and_direction_changing_characters_cannot_disguise_text() {
        let p = render(
            GMAIL_MESSAGE,
            &raw("To: a@b.co\nSubject: pay \u{202E}fdp.exe\u{200B}\n\nhel\u{0007}lo\u{FEFF}"),
            None,
        )
        .unwrap();
        assert_eq!(field(&p, "Subject"), ["pay fdp.exe"]);
        assert_eq!(field(&p, "Message"), ["hello"]);
    }

    #[test]
    fn a_long_message_is_cut_and_says_so() {
        let p = render(
            GMAIL_MESSAGE,
            &raw(&format!("To: a@b.co\n\n{}", "y".repeat(3000))),
            None,
        )
        .unwrap();
        let shown = field(&p, "Message")[0];
        assert_eq!(shown.chars().count(), MAX_TEXT_CHARS + 1);
        assert!(shown.ends_with('…'));
        assert!(field(&p, "Length")[0].starts_with("3000 characters"));
    }

    #[test]
    fn a_message_with_no_recipient_says_so() {
        let p = render(GMAIL_MESSAGE, &raw("Subject: draft\n\nnotes to self"), None).unwrap();
        assert_eq!(field(&p, "To"), ["(no recipient)"]);
    }

    fn event(v: Value) -> Vec<u8> {
        v.to_string().into_bytes()
    }

    #[test]
    fn an_event_shows_when_where_and_who_is_invited_and_whether_they_are_emailed() {
        let body = event(json!({
            "summary": "Dinner", "location": "Home",
            "start": {"dateTime": "2026-10-01T19:00:00", "timeZone": "Europe/Berlin"},
            "end": {"dateTime": "2026-10-01T21:00:00", "timeZone": "Europe/Berlin"},
            "attendees": [{"email": "ana@example.com"}, {"email": "ben@example.com"}],
            "description": "Bring wine"
        }));
        let p = render(CALENDAR_EVENT, &body, Some("sendUpdates=all")).unwrap();
        assert_eq!(field(&p, "Title"), ["Dinner"]);
        assert_eq!(field(&p, "Starts"), ["2026-10-01T19:00:00 Europe/Berlin"]);
        assert_eq!(field(&p, "Guests"), ["ana@example.com, ben@example.com"]);
        assert_eq!(field(&p, "Guests are emailed"), ["yes, all of them"]);
        assert_eq!(field(&p, "Notes"), ["Bring wine"]);
        let quiet = render(CALENDAR_EVENT, &body, None).unwrap();
        assert_eq!(field(&quiet, "Guests are emailed"), ["no"]);
    }

    #[test]
    fn an_event_shows_repeats_all_day_and_fields_it_does_not_render() {
        let body = event(json!({
            "start": {"date": "2026-10-01"}, "end": {"date": "2026-10-02"},
            "recurrence": ["RRULE:FREQ=WEEKLY"], "conferenceData": {"createRequest": {}}
        }));
        let p = render(CALENDAR_EVENT, &body, None).unwrap();
        assert_eq!(field(&p, "Title"), ["(no title)"]);
        assert_eq!(field(&p, "Starts"), ["2026-10-01 (all day)"]);
        assert!(field(&p, "Repeats")[0].contains("WEEKLY"));
        assert_eq!(field(&p, "Also sets"), ["conferenceData"]);
    }

    #[test]
    fn a_malformed_event_is_refused() {
        for body in [
            json!({"summary": "x"}),
            json!({"summary": 5, "start": {"date": "d"}, "end": {"date": "d"}}),
            json!({"start": {"date": "d"}, "end": {"date": "d"}, "attendees": "a@b.co"}),
            json!({"start": {"date": "d"}, "end": {"date": "d"}, "attendees": [{"name": "x"}]}),
            json!([1, 2]),
        ] {
            assert!(
                render(CALENDAR_EVENT, &event(body.clone()), None).is_err(),
                "{body}"
            );
        }
    }

    #[test]
    fn the_digest_follows_the_rendering() {
        let a = render(GMAIL_MESSAGE, &raw("To: a@b.co\n\nhi"), None).unwrap();
        let same = render(GMAIL_MESSAGE, &raw("To: a@b.co\n\nhi"), None).unwrap();
        let other = render(GMAIL_MESSAGE, &raw("To: c@d.co\n\nhi"), None).unwrap();
        assert_eq!(a.sha256(), same.sha256());
        assert_ne!(a.sha256(), other.sha256());
        assert_eq!(a.sha256().len(), 64);
    }

    fn graph(v: Value) -> Vec<u8> {
        v.to_string().into_bytes()
    }

    #[test]
    fn a_graph_message_shows_recipients_first_by_address_and_the_text() {
        let send = graph(json!({"message": {
            "subject": "Numbers",
            "body": {"contentType": "Text", "content": "Attached below."},
            "toRecipients": [{"emailAddress": {"address": "ana@example.com", "name": "boss@corp.example"}}],
            "ccRecipients": [{"emailAddress": {"address": "ben@example.com"}}],
            "bccRecipients": [{"emailAddress": {"address": "spy@evil.test"}}]
        }, "saveToSentItems": true}));
        let p = render(GRAPH_MESSAGE, &send, None).unwrap();
        assert_eq!(
            field(&p, "To"),
            ["ana@example.com (boss@corp.example)"],
            "the address leads, the free-text name cannot pose as it"
        );
        assert_eq!(field(&p, "Cc"), ["ben@example.com"]);
        assert_eq!(field(&p, "Bcc (hidden from the others)"), ["spy@evil.test"]);
        assert_eq!(field(&p, "Subject"), ["Numbers"]);
        assert_eq!(field(&p, "Message"), ["Attached below."]);
        // a draft posts the message itself
        let draft = graph(
            json!({"subject": "Hi", "body": {"contentType": "text", "content": "x"}, "toRecipients": [{"emailAddress": {"address": "a@b.co"}}]}),
        );
        assert_eq!(
            field(&render(GRAPH_MESSAGE, &draft, None).unwrap(), "To"),
            ["a@b.co"]
        );
        let none = graph(json!({"subject": "notes"}));
        assert_eq!(
            field(&render(GRAPH_MESSAGE, &none, None).unwrap(), "To"),
            ["(no recipient)"]
        );
    }

    #[test]
    fn what_a_graph_message_hides_or_cannot_be_shown_is_refused_or_listed() {
        let ok = |m: Value| render(GRAPH_MESSAGE, &graph(m), None);
        let base = || json!({"subject": "s", "body": {"contentType": "Text", "content": "b"}, "toRecipients": [{"emailAddress": {"address": "a@b.co"}}]});
        assert!(
            ok(json!({"subject": "s", "body": {"contentType": "HTML", "content": "<b>x</b>"}}))
                .is_err(),
            "html"
        );
        let mut with_attachment = base();
        with_attachment["attachments"] = json!([{"name": "x.exe"}]);
        assert!(ok(with_attachment).is_err(), "attachments");
        assert!(
            ok(json!({"message": base(), "extra": 1})).is_err(),
            "fields beside the message"
        );
        assert!(
            ok(json!({})).is_err() && ok(json!({"message": {}})).is_err(),
            "nothing to show"
        );
        assert!(
            ok(json!({"toRecipients": [{"emailAddress": {}}]})).is_err(),
            "a recipient with no address"
        );
        assert!(ok(json!({"toRecipients": "a@b.co"})).is_err());
        let mut odd = base();
        odd["internetMessageHeaders"] = json!([{"name": "x", "value": "y"}]);
        assert_eq!(
            field(&ok(odd).unwrap(), "Also sets"),
            ["internetMessageHeaders"],
            "what it cannot show it names"
        );
        assert!(render(GRAPH_MESSAGE, b"nope", None).is_err());
    }

    #[test]
    fn a_graph_event_says_guests_are_emailed_because_microsoft_always_does() {
        let body = graph(json!({
            "subject": "Dinner", "location": {"displayName": "Home"}, "isOnlineMeeting": true,
            "start": {"dateTime": "2026-10-01T19:00:00", "timeZone": "Europe/Berlin"},
            "end": {"dateTime": "2026-10-01T21:00:00", "timeZone": "Europe/Berlin"},
            "attendees": [{"emailAddress": {"address": "ana@example.com"}, "type": "required"}],
            "body": {"contentType": "Text", "content": "Bring wine"}
        }));
        let p = render(GRAPH_EVENT, &body, None).unwrap();
        assert_eq!(field(&p, "Title"), ["Dinner"]);
        assert_eq!(field(&p, "Starts"), ["2026-10-01T19:00:00 Europe/Berlin"]);
        assert_eq!(field(&p, "Where"), ["Home"]);
        assert_eq!(field(&p, "Guests"), ["ana@example.com"]);
        assert!(field(&p, "Guests are emailed")[0].starts_with("yes"));
        assert!(field(&p, "Online meeting")[0].starts_with("yes"));
        assert_eq!(field(&p, "Notes"), ["Bring wine"]);
        let solo = render(GRAPH_EVENT, &graph(json!({"start": {"dateTime": "d"}, "end": {"dateTime": "d"}, "reminderMinutesBeforeStart": 5})), None).unwrap();
        assert!(
            field(&solo, "Guests are emailed").is_empty(),
            "no guests, no email"
        );
        assert_eq!(field(&solo, "Title"), ["(no title)"]);
        assert_eq!(field(&solo, "Also sets"), ["reminderMinutesBeforeStart"]);
        for bad in [
            json!({"subject": "x"}),
            json!({"start": {"dateTime": "d"}, "end": {"dateTime": "d"}, "body": {"contentType": "html", "content": "<b>"}}),
            json!({"start": {"dateTime": "d"}, "end": {"dateTime": "d"}, "attendees": [{"type": "required"}]}),
            json!({"start": {"dateTime": "d"}, "end": {"dateTime": "d"}, "location": {"address": {}}}),
            json!([1]),
        ] {
            assert!(
                render(GRAPH_EVENT, &graph(bad.clone()), None).is_err(),
                "{bad}"
            );
        }
    }
}
