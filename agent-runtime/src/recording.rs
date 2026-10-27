// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: Apache-2.0

//! Session recording: a signed, replayable bundle of what a session did (docs/merion/design/session-recording.md).
//!
//! The bundle is built only from host records: the session's journal rows, its event log and its approvals. Nothing in
//! it is an agent's claim. It is masked before it is signed:
//!
//! * a journal row keeps only the scalar fields on the OTLP allowlist (ids, a decision, a host, a method); the free-text
//!   `error` or `tail`, request bodies, previews and the `subject` are dropped, and the row says how many fields were;
//! * an event keeps its kind, sequence and time, plus the length and SHA-256 of its data, never the data;
//! * an approval keeps its kind, status and times, never the prompt or the planned action.
//!
//! The digest covers the masked bundle, so a signature proves the host produced exactly this view. It does not prove
//! the masked-out content, and a journal row's own `hash` is kept as a pointer into the journal: the hash was computed
//! over the unmasked row, so it cannot be recomputed from the bundle. Signing needs `ZYVOR_AGENT_RECORDING_KEY`; with
//! none set nothing is exported.

use crate::{
    audit::AuditEntry,
    model::{ApprovalRecord, SessionEvent, SessionRecord},
    otlp::DETAIL_ALLOWLIST,
};
use chrono::{DateTime, Utc};
use ed25519_dalek::{Signature, Signer, SigningKey, Verifier, VerifyingKey};
use serde_json::{json, Map, Value};
use sha2::{Digest, Sha256};

pub const FORMAT: &str = "merion-recording/1";

/// Everything the bundle is built from. Plain data, so the builder is a pure function.
pub struct Inputs<'a> {
    pub session: &'a SessionRecord,
    pub audit: &'a [AuditEntry],
    pub events: &'a [SessionEvent],
    pub approvals: &'a [ApprovalRecord],
}

/// The allowlisted scalar fields of a journal detail, and how many other fields were left out.
fn mask_detail(detail: &Value) -> (Value, usize) {
    let Some(obj) = detail.as_object() else {
        return (json!({}), 0);
    };
    let mut kept = Map::new();
    for (k, v) in obj {
        if DETAIL_ALLOWLIST.contains(&k.as_str())
            && (v.is_string() || v.is_number() || v.is_boolean())
        {
            kept.insert(k.clone(), v.clone());
        }
    }
    let dropped = obj.len() - kept.len();
    (Value::Object(kept), dropped)
}

fn rfc(t: DateTime<Utc>) -> String {
    t.to_rfc3339_opts(chrono::SecondsFormat::Millis, true)
}

/// The masked, ordered steps of one session. Same-instant steps keep journal, then event, then approval order.
pub fn build(inp: &Inputs) -> Value {
    let mut steps: Vec<(DateTime<Utc>, u8, Value)> = Vec::new();
    for e in inp.audit {
        let (detail, masked) = mask_detail(&e.detail);
        steps.push((
            e.at,
            0,
            json!({
                "source": "journal", "seq": e.seq, "at": rfc(e.at), "phase": e.phase,
                "action": e.action, "detail": detail, "masked_fields": masked,
                "has_subject": e.subject.is_some(), "prev_hash": e.prev_hash, "hash": e.hash,
            }),
        ));
    }
    for ev in inp.events {
        let raw = serde_json::to_vec(&ev.data).unwrap_or_default();
        steps.push((
            ev.timestamp,
            1,
            json!({
                "source": "event", "seq": ev.seq, "at": rfc(ev.timestamp), "kind": ev.kind,
                "data_bytes": raw.len(), "data_sha256": hex::encode(Sha256::digest(&raw)),
            }),
        ));
    }
    for a in inp.approvals {
        steps.push((
            a.created_at,
            2,
            json!({
                "source": "approval", "id": a.id, "at": rfc(a.created_at), "kind": a.kind,
                "status": a.status, "decided_at": a.decided_at.map(rfc),
            }),
        ));
    }
    steps.sort_by_key(|(t, order, _)| (*t, *order));
    let steps: Vec<Value> = steps
        .into_iter()
        .enumerate()
        .map(|(i, (_, _, mut v))| {
            v["n"] = json!(i);
            v
        })
        .collect();
    let s = inp.session;
    json!({
        "format": FORMAT,
        "session": {
            "id": s.id, "agent": s.agent, "agent_version": s.agent_version, "status": s.status,
            "created_at": rfc(s.created_at), "updated_at": rfc(s.updated_at),
        },
        "steps": steps,
        "masking": "journal detail is an allowlist of scalars; event data and approval text are not included",
    })
}

fn digest(bundle: &Value) -> [u8; 32] {
    Sha256::digest(serde_json::to_vec(bundle).unwrap_or_default()).into()
}

/// Wrap a bundle with its digest, an Ed25519 signature over the digest, and the signer's public key.
pub fn sign(bundle: Value, seed: &[u8; 32]) -> Value {
    let key = SigningKey::from_bytes(seed);
    let d = digest(&bundle);
    json!({
        "bundle": bundle,
        "digest": hex::encode(d),
        "signature": hex::encode(key.sign(&d).to_bytes()),
        "signer": hex::encode(key.verifying_key().to_bytes()),
    })
}

/// Check a signed recording; returns the signer's public key (hex). The caller decides whether to trust that signer.
pub fn verify(signed: &Value) -> Result<String, &'static str> {
    let bundle = signed.get("bundle").ok_or("no bundle")?;
    if bundle.get("format").and_then(Value::as_str) != Some(FORMAT) {
        return Err("unknown format");
    }
    let field = |name: &str| {
        signed
            .get(name)
            .and_then(Value::as_str)
            .ok_or("missing field")
    };
    let d = digest(bundle);
    if field("digest")? != hex::encode(d) {
        return Err("the bundle does not match its digest");
    }
    let signer = field("signer")?;
    let pk: [u8; 32] = hex::decode(signer)
        .ok()
        .and_then(|b| b.try_into().ok())
        .ok_or("malformed signer")?;
    let sig = hex::decode(field("signature")?)
        .ok()
        .and_then(|b| Signature::from_slice(&b).ok())
        .ok_or("malformed signature")?;
    VerifyingKey::from_bytes(&pk)
        .map_err(|_| "malformed signer")?
        .verify(&d, &sig)
        .map_err(|_| "the signature does not match")?;
    Ok(signer.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::audit::AuditPhase;
    use crate::model::{ApprovalKind, ApprovalStatus, SessionStatus};
    use uuid::Uuid;

    fn session() -> SessionRecord {
        serde_json::from_value(json!({
            "id": Uuid::nil(), "agent": "a", "agent_version": "1", "sandbox_id": Uuid::nil(),
            "status": SessionStatus::Running, "input": {}, "created_at": Utc::now(),
            "updated_at": Utc::now(), "last_event_seq": 0, "guest_event_cursor": 0, "capability_token": "t",
        }))
        .expect("session fixture")
    }

    fn row(seq: u64, detail: Value) -> AuditEntry {
        AuditEntry {
            seq,
            at: Utc::now(),
            session_id: None,
            phase: AuditPhase::Performed,
            action: "egress.request".into(),
            subject: Some("bob@example.com".into()),
            detail,
            prev_hash: "p".into(),
            hash: "h".into(),
        }
    }

    fn sample() -> Value {
        let s = session();
        let audit = [row(
            1,
            json!({"host": "api.example.com", "error": "token=abc", "body": {"x": 1}}),
        )];
        let events = [SessionEvent {
            session_id: s.id,
            seq: 1,
            kind: "stdout".into(),
            data: json!({"text": "secret=hunter2"}),
            timestamp: Utc::now(),
        }];
        let approvals = [ApprovalRecord {
            prompt: "send to bob@example.com".into(),
            ..serde_json::from_value(json!({
                "id": Uuid::nil(), "session_id": Uuid::nil(), "prompt": "x", "status": ApprovalStatus::Approved,
                "created_at": Utc::now(), "kind": ApprovalKind::default(),
            }))
            .expect("approval fixture")
        }];
        build(&Inputs {
            session: &s,
            audit: &audit,
            events: &events,
            approvals: &approvals,
        })
    }

    #[test]
    fn masking_drops_free_text_bodies_subjects_and_event_data() {
        let text = serde_json::to_string(&sample()).unwrap();
        for leaked in ["token=abc", "hunter2", "bob@example.com", "send to"] {
            assert!(!text.contains(leaked), "{leaked} leaked into the recording");
        }
        assert!(
            text.contains("api.example.com"),
            "the allowlisted host stays"
        );
        assert!(text.contains("\"masked_fields\":2"));
        assert!(text.contains("data_sha256"));
    }

    #[test]
    fn a_signed_recording_verifies_and_any_change_breaks_it() {
        let seed = [7u8; 32];
        let signed = sign(sample(), &seed);
        let signer = verify(&signed).expect("valid");
        assert_eq!(
            signer,
            hex::encode(SigningKey::from_bytes(&seed).verifying_key().to_bytes())
        );

        let mut edited = signed.clone();
        edited["bundle"]["steps"][0]["action"] = json!("egress.nothing");
        assert!(verify(&edited).is_err(), "an edited step must fail");

        let mut swapped = signed.clone();
        swapped["signer"] = json!(hex::encode(
            SigningKey::from_bytes(&[8u8; 32])
                .verifying_key()
                .to_bytes()
        ));
        assert!(verify(&swapped).is_err(), "a different signer must fail");

        let mut resigned = sign(sample(), &[9u8; 32]);
        resigned["signature"] = signed["signature"].clone();
        assert!(
            verify(&resigned).is_err(),
            "a signature from another key must fail"
        );
    }

    #[test]
    fn steps_are_numbered_in_time_order() {
        let b = sample();
        let steps = b["steps"].as_array().unwrap();
        assert_eq!(steps.len(), 3);
        for (i, s) in steps.iter().enumerate() {
            assert_eq!(s["n"], json!(i));
        }
    }
}
