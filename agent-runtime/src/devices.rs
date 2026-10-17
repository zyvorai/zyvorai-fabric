// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: Apache-2.0

//! Enrolled phones, and approvals a phone signs.
//!
//! A user enrols a device by registering its **public** key (the private key stays in the phone's
//! keystore). When an approval is waiting, the phone signs an exact text payload that names the
//! approval, the decision, a digest of what is being approved and a server-made challenge. The
//! runtime rebuilds that payload and checks the signature against the enrolled key, so a decision
//! that was not made by that phone (or was altered on the way, or replayed for another approval)
//! is refused.
//!
//! What this proves, and what it does not: it proves the holder of the enrolled key made *this*
//! decision on *this* approval. It does not make the vault user-held (the secrets still come from
//! the host environment) and it says nothing about the phone being uncompromised.
//!
//! Keys: ECDSA P-256 (what Android Keystore/StrongBox hold) as an X.509 SubjectPublicKeyInfo or a SEC1
//! point, and Ed25519 (raw 32 bytes or SubjectPublicKeyInfo). Signatures are base64: DER or raw
//! `r||s` for P-256, 64 bytes for Ed25519.

use crate::{
    model::{ApprovalRecord, ApprovalStatus},
    schedules::hmac_sha256,
};
use base64::Engine;
use chrono::{DateTime, Utc};
use ed25519_dalek::Verifier as _;
use p256::pkcs8::DecodePublicKey;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};

pub const PAYLOAD_FORMAT: &str = "keep-approval-v1";
/// The same payload under its post-rename name. A signature over one text never verifies as the other,
/// so accepting both loses no domain separation. Enrolled phones sign [`PAYLOAD_FORMAT`] today.
pub const PAYLOAD_FORMAT_MERION: &str = "merion-approval-v1";
/// Payload formats the host verifies, oldest first.
pub const ACCEPTED_FORMATS: [&str; 2] = [PAYLOAD_FORMAT, PAYLOAD_FORMAT_MERION];
const DEFAULT_SIGN_TTL: i64 = 3600;
const ED25519_SPKI_PREFIX: [u8; 12] = [
    0x30, 0x2a, 0x30, 0x05, 0x06, 0x03, 0x2b, 0x65, 0x70, 0x03, 0x21, 0x00,
];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum KeyAlg {
    P256,
    Ed25519,
}

/// Where the operator's push relay should send a wake-up for this device. `kind` picks the relay
/// (`fcm`, `webhook`, or a vendor channel); `token` is that channel's address for the device.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PushTarget {
    pub kind: String,
    pub token: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeviceRecord {
    pub user_id: String,
    pub device_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    pub alg: KeyAlg,
    /// The public key as enrolled (base64).
    pub public_key: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub push: Option<PushTarget>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Deserialize)]
pub struct EnrollRequest {
    pub device_id: String,
    #[serde(default)]
    pub name: Option<String>,
    pub alg: KeyAlg,
    pub public_key: String,
    #[serde(default)]
    pub push: Option<PushTarget>,
}

pub fn valid_device_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 64
        && id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.'))
}

fn b64(s: &str) -> Result<Vec<u8>, String> {
    let s = s.trim();
    base64::engine::general_purpose::STANDARD
        .decode(s)
        .or_else(|_| base64::engine::general_purpose::URL_SAFE_NO_PAD.decode(s))
        .or_else(|_| base64::engine::general_purpose::URL_SAFE.decode(s))
        .map_err(|_| "not valid base64".to_string())
}

/// Check an enrolment request and return the record to store.
pub fn build_record(
    user_id: &str,
    req: EnrollRequest,
    now: DateTime<Utc>,
) -> Result<DeviceRecord, String> {
    if !valid_device_id(&req.device_id) {
        return Err("device_id must be 1-64 letters, digits, '-', '_' or '.'".into());
    }
    if req
        .name
        .as_deref()
        .is_some_and(|n| n.len() > 80 || n.chars().any(char::is_control))
    {
        return Err("name must be at most 80 plain characters".into());
    }
    if let Some(p) = &req.push {
        let ok = !p.kind.is_empty()
            && p.kind.len() <= 32
            && p.kind
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_'))
            && !p.token.is_empty()
            && p.token.len() <= 4096;
        if !ok {
            return Err("push needs a short kind (letters, digits, '-', '_') and a token".into());
        }
    }
    // Parse now, so a key that can never verify is refused at enrolment.
    check_public_key(req.alg, &req.public_key)?;
    Ok(DeviceRecord {
        user_id: user_id.to_string(),
        device_id: req.device_id,
        name: req.name,
        alg: req.alg,
        public_key: req.public_key.trim().to_string(),
        push: req.push,
        created_at: now,
    })
}

fn p256_key(bytes: &[u8]) -> Result<p256::ecdsa::VerifyingKey, String> {
    p256::ecdsa::VerifyingKey::from_public_key_der(bytes)
        .or_else(|_| p256::ecdsa::VerifyingKey::from_sec1_bytes(bytes))
        .map_err(|_| {
            "not a P-256 public key (expected SubjectPublicKeyInfo or a SEC1 point)".to_string()
        })
}

fn ed25519_key(bytes: &[u8]) -> Result<ed25519_dalek::VerifyingKey, String> {
    let raw: [u8; 32] = if bytes.len() == 32 {
        bytes.try_into().map_err(|_| "bad key".to_string())?
    } else if bytes.len() == 44 && bytes[..12] == ED25519_SPKI_PREFIX {
        bytes[12..].try_into().map_err(|_| "bad key".to_string())?
    } else {
        return Err(
            "not an Ed25519 public key (expected 32 raw bytes or SubjectPublicKeyInfo)".into(),
        );
    };
    ed25519_dalek::VerifyingKey::from_bytes(&raw)
        .map_err(|_| "not a valid Ed25519 public key".to_string())
}

pub fn check_public_key(alg: KeyAlg, public_key: &str) -> Result<(), String> {
    let bytes = b64(public_key)?;
    match alg {
        KeyAlg::P256 => p256_key(&bytes).map(|_| ()),
        KeyAlg::Ed25519 => ed25519_key(&bytes).map(|_| ()),
    }
}

/// Does `signature` (base64) over `message` verify under the device's key?
pub fn verify_signature(device: &DeviceRecord, message: &[u8], signature: &str) -> bool {
    let (Ok(key), Ok(sig)) = (b64(&device.public_key), b64(signature)) else {
        return false;
    };
    match device.alg {
        KeyAlg::P256 => {
            use p256::ecdsa::signature::Verifier;
            let Ok(vk) = p256_key(&key) else { return false };
            let parsed = p256::ecdsa::Signature::from_der(&sig)
                .or_else(|_| p256::ecdsa::Signature::from_slice(&sig));
            parsed.is_ok_and(|s| vk.verify(message, &s).is_ok())
        }
        KeyAlg::Ed25519 => {
            let Ok(vk) = ed25519_key(&key) else {
                return false;
            };
            ed25519_dalek::Signature::from_slice(&sig).is_ok_and(|s| vk.verify(message, &s).is_ok())
        }
    }
}

/// Digest of what is being approved, so a signature covers the action and not just an id.
pub fn action_sha256(approval: &ApprovalRecord) -> String {
    let planned = approval.planned_action.clone().unwrap_or(Value::Null);
    hex::encode(Sha256::digest(
        serde_json::to_vec(&planned).unwrap_or_default(),
    ))
}

/// How long a phone has to sign after the approval opened.
pub fn sign_ttl_seconds() -> i64 {
    std::env::var("ZYVOR_AGENT_APPROVAL_SIGN_TTL_SECONDS")
        .ok()
        .and_then(|v| v.trim().parse::<i64>().ok())
        .unwrap_or(DEFAULT_SIGN_TTL)
        .clamp(60, 86_400)
}

pub fn expires_at(approval: &ApprovalRecord) -> i64 {
    approval.created_at.timestamp() + sign_ttl_seconds()
}

/// The server-made challenge for one approval. Derived, not stored: the same key and approval
/// always give the same value, and nobody without the key can predict it.
pub fn challenge(key: &[u8], approval: &ApprovalRecord) -> String {
    let msg = format!(
        "approval-challenge:{}:{}",
        approval.id,
        approval.created_at.timestamp()
    );
    hex::encode(&hmac_sha256(key, msg.as_bytes())[..16])
}

/// The exact text the phone signs. Human-readable on purpose: the phone can show it as is.
pub fn signing_payload(
    approval: &ApprovalRecord,
    decision: ApprovalStatus,
    challenge: &str,
) -> String {
    signing_payload_as(PAYLOAD_FORMAT, approval, decision, challenge)
}

/// [`signing_payload`] under a named format (one of [`ACCEPTED_FORMATS`]).
pub fn signing_payload_as(
    format: &str,
    approval: &ApprovalRecord,
    decision: ApprovalStatus,
    challenge: &str,
) -> String {
    let decision = match decision {
        ApprovalStatus::Approved => "approved",
        ApprovalStatus::Denied => "denied",
        _ => "other",
    };
    format!(
        "{format}\napproval: {}\ndecision: {decision}\nkind: {}\nsubject: {}\naction-sha256: {}\nchallenge: {challenge}\nexpires: {}\n",
        approval.id,
        approval.kind.as_str(),
        approval.subject.as_deref().unwrap_or(""),
        action_sha256(approval),
        expires_at(approval),
    )
}

/// What a phone needs, next to an approval, to sign it.
pub fn signing_info(key: &[u8], approval: &ApprovalRecord) -> Value {
    json!({
        // `format` stays the legacy name: it is what a client built before the rename signs and may check.
        // `formats` lists what the host verifies; a client that knows `merion-approval-v1` may sign that.
        "format": PAYLOAD_FORMAT,
        "formats": ACCEPTED_FORMATS,
        "challenge": challenge(key, approval),
        "expires_at": expires_at(approval),
        "action_sha256": action_sha256(approval),
        "algorithms": ["p256", "ed25519"],
    })
}

/// Why a signed decision was refused.
#[derive(Debug, PartialEq, Eq)]
pub enum SignError {
    Expired,
    BadSignature,
}

/// Verify a phone's signature over the decision it claims to have made.
pub fn verify_decision(
    key: &[u8],
    device: &DeviceRecord,
    approval: &ApprovalRecord,
    decision: ApprovalStatus,
    signature: &str,
    now: DateTime<Utc>,
) -> Result<(), SignError> {
    verify_decision_format(key, device, approval, decision, signature, now).map(|_| ())
}

/// [`verify_decision`], also returning which payload format the signature was over.
pub fn verify_decision_format(
    key: &[u8],
    device: &DeviceRecord,
    approval: &ApprovalRecord,
    decision: ApprovalStatus,
    signature: &str,
    now: DateTime<Utc>,
) -> Result<&'static str, SignError> {
    if now.timestamp() >= expires_at(approval) {
        return Err(SignError::Expired);
    }
    let challenge = challenge(key, approval);
    for format in ACCEPTED_FORMATS {
        let payload = signing_payload_as(format, approval, decision, &challenge);
        if verify_signature(device, payload.as_bytes(), signature) {
            return Ok(format);
        }
    }
    Err(SignError::BadSignature)
}

/// Should a decision made with a user token have to be signed by an enrolled phone?
pub(crate) fn signature_required(state: &crate::AppState, approval: &ApprovalRecord) -> bool {
    if std::env::var("ZYVOR_AGENT_REQUIRE_DEVICE_SIGNATURE").is_ok_and(|v| v == "1") {
        return true;
    }
    approval
        .planned_action
        .as_ref()
        .and_then(|p| p.get("credential"))
        .and_then(Value::as_str)
        .and_then(|name| state.credentials.descriptor(name))
        .is_some_and(|d| d.require_device_signature)
}

/// Check the phone signature on a decision, if there is one or if one is required.
/// Returns the device id that signed, or `None` for an unsigned decision that is allowed.
pub(crate) async fn check_decision(
    state: &crate::AppState,
    principal: &crate::authz::Principal,
    approval: &ApprovalRecord,
    user_id: Option<&str>,
    req: &crate::model::DecideApprovalRequest,
) -> Result<Option<String>, crate::app::ApiError> {
    use crate::{app::ApiError, audit::AuditPhase};
    let (device_id, signature) = match (&req.device_id, &req.signature) {
        (None, None) => {
            // Only a user token has to be signed; the operator can always decide.
            if principal.user().is_some() && signature_required(state, approval) {
                return Err(ApiError::forbidden(
                    "this approval must be signed by an enrolled device (device_id and signature)",
                ));
            }
            return Ok(None);
        }
        (Some(d), Some(s)) => (d, s),
        _ => return Err(ApiError::bad_request("device_id and signature go together")),
    };
    let audit = |phase: AuditPhase, detail: Value| async move {
        let _ = state
            .store
            .audit
            .append(
                Some(approval.session_id),
                phase,
                "approval.device_signature",
                approval.subject.clone(),
                detail,
            )
            .await;
    };
    let Some(user) = user_id else {
        return Err(ApiError::forbidden(
            "this approval's session has no user, so it has no enrolled devices",
        ));
    };
    let Some(key) = crate::authz::signing_key_for(state) else {
        return Err(ApiError::bad_request(format!(
            "device signatures need an operator token or {}",
            crate::authz::SECRET_ENV
        )));
    };
    let Some(device) = state.store.get_device(user, device_id).await else {
        audit(
            AuditPhase::Failed,
            json!({"approval_id": approval.id, "device_id": device_id, "error": "unknown device"}),
        )
        .await;
        return Err(ApiError::forbidden("unknown device for this user"));
    };
    match verify_decision_format(&key, &device, approval, req.decision, signature, Utc::now()) {
        Ok(format) => {
            audit(AuditPhase::Performed, json!({"approval_id": approval.id, "device_id": device_id, "decision": req.decision, "format": format})).await;
            Ok(Some(device_id.clone()))
        }
        Err(e) => {
            let why = match e {
                SignError::Expired => "the signing window has passed",
                SignError::BadSignature => "the signature does not match the enrolled device",
            };
            audit(
                AuditPhase::Failed,
                json!({"approval_id": approval.id, "device_id": device_id, "error": why}),
            )
            .await;
            Err(ApiError::forbidden(why))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::ApprovalKind;
    use chrono::Duration;
    use ed25519_dalek::Signer as _;
    use p256::pkcs8::EncodePublicKey;
    use uuid::Uuid;

    fn approval() -> ApprovalRecord {
        ApprovalRecord {
            id: Uuid::new_v4(),
            session_id: Uuid::new_v4(),
            kind: ApprovalKind::Send,
            subject: Some("mail.example".into()),
            planned_action: Some(json!({"method": "POST", "body_sha256": "abc"})),
            prompt: "send it".into(),
            status: ApprovalStatus::Pending,
            comment: None,
            created_at: Utc::now(),
            decided_at: None,
            source_seq: None,
            grant_scope: None,
            preview: None,
            broker_held: true,
        }
    }

    fn p256_device() -> (DeviceRecord, p256::ecdsa::SigningKey) {
        let sk = p256::ecdsa::SigningKey::random(&mut rand_core_os());
        let spki = sk.verifying_key().to_public_key_der().unwrap();
        let rec = build_record(
            "ana",
            EnrollRequest {
                device_id: "phone-1".into(),
                name: None,
                alg: KeyAlg::P256,
                public_key: base64::engine::general_purpose::STANDARD.encode(spki.as_bytes()),
                push: None,
            },
            Utc::now(),
        )
        .unwrap();
        (rec, sk)
    }

    fn ed_device() -> (DeviceRecord, ed25519_dalek::SigningKey) {
        let sk = ed25519_dalek::SigningKey::generate(&mut rand_core_os());
        let rec = build_record(
            "ana",
            EnrollRequest {
                device_id: "phone-2".into(),
                name: Some("Ana's phone".into()),
                alg: KeyAlg::Ed25519,
                public_key: base64::engine::general_purpose::STANDARD
                    .encode(sk.verifying_key().as_bytes()),
                push: Some(PushTarget {
                    kind: "fcm".into(),
                    token: "tok".into(),
                }),
            },
            Utc::now(),
        )
        .unwrap();
        (rec, sk)
    }

    fn rand_core_os() -> impl ed25519_dalek::ed25519::signature::rand_core::CryptoRngCore {
        ed25519_dalek::ed25519::signature::rand_core::OsRng
    }

    fn server_key() -> Vec<u8> {
        crate::fixture::bytes(b"server-key-for-tests")
    }

    fn sign_p256(
        sk: &p256::ecdsa::SigningKey,
        a: &ApprovalRecord,
        d: ApprovalStatus,
        der: bool,
    ) -> String {
        let payload = signing_payload(a, d, &challenge(&server_key(), a));
        let sig: p256::ecdsa::Signature = sk.sign(payload.as_bytes());
        let bytes = if der {
            sig.to_der().as_bytes().to_vec()
        } else {
            sig.to_bytes().to_vec()
        };
        base64::engine::general_purpose::STANDARD.encode(bytes)
    }

    #[test]
    fn a_p256_signature_verifies_in_der_and_raw_form() {
        let (dev, sk) = p256_device();
        let a = approval();
        for der in [true, false] {
            let sig = sign_p256(&sk, &a, ApprovalStatus::Approved, der);
            assert_eq!(
                verify_decision(
                    &server_key(),
                    &dev,
                    &a,
                    ApprovalStatus::Approved,
                    &sig,
                    Utc::now()
                ),
                Ok(()),
                "der={der}"
            );
        }
    }

    #[test]
    fn an_ed25519_signature_verifies() {
        let (dev, sk) = ed_device();
        let a = approval();
        let payload = signing_payload(&a, ApprovalStatus::Denied, &challenge(&server_key(), &a));
        let sig = base64::engine::general_purpose::STANDARD
            .encode(sk.sign(payload.as_bytes()).to_bytes());
        assert_eq!(
            verify_decision(
                &server_key(),
                &dev,
                &a,
                ApprovalStatus::Denied,
                &sig,
                Utc::now()
            ),
            Ok(())
        );
    }

    #[test]
    fn a_decision_cannot_be_flipped_moved_or_forged() {
        let (dev, sk) = p256_device();
        let a = approval();
        let approved = sign_p256(&sk, &a, ApprovalStatus::Approved, true);
        // Signed "approved", submitted as "denied".
        assert_eq!(
            verify_decision(
                &server_key(),
                &dev,
                &a,
                ApprovalStatus::Denied,
                &approved,
                Utc::now()
            ),
            Err(SignError::BadSignature)
        );
        // Replayed onto a different approval.
        let other = approval();
        assert_eq!(
            verify_decision(
                &server_key(),
                &dev,
                &other,
                ApprovalStatus::Approved,
                &approved,
                Utc::now()
            ),
            Err(SignError::BadSignature)
        );
        // A different action under the same id (planned action changed after signing).
        let mut changed = a.clone();
        changed.planned_action = Some(json!({"method": "POST", "body_sha256": "different"}));
        assert_eq!(
            verify_decision(
                &server_key(),
                &dev,
                &changed,
                ApprovalStatus::Approved,
                &approved,
                Utc::now()
            ),
            Err(SignError::BadSignature)
        );
        // Another server's key gives another challenge.
        assert_eq!(
            verify_decision(
                &crate::fixture::bytes(b"other-server-key"),
                &dev,
                &a,
                ApprovalStatus::Approved,
                &approved,
                Utc::now()
            ),
            Err(SignError::BadSignature)
        );
        // Someone else's key, garbage, and an empty signature.
        let (_, intruder) = p256_device();
        let forged = sign_p256(&intruder, &a, ApprovalStatus::Approved, true);
        for bad in [forged.as_str(), "AAAA", "", "not base64!!"] {
            assert_eq!(
                verify_decision(
                    &server_key(),
                    &dev,
                    &a,
                    ApprovalStatus::Approved,
                    bad,
                    Utc::now()
                ),
                Err(SignError::BadSignature),
                "{bad:?}"
            );
        }
    }

    #[test]
    fn a_signature_expires() {
        let (dev, sk) = p256_device();
        let a = approval();
        let sig = sign_p256(&sk, &a, ApprovalStatus::Approved, true);
        let late = Utc::now() + Duration::seconds(sign_ttl_seconds() + 5);
        assert_eq!(
            verify_decision(
                &server_key(),
                &dev,
                &a,
                ApprovalStatus::Approved,
                &sig,
                late
            ),
            Err(SignError::Expired)
        );
    }

    #[test]
    fn enrolment_refuses_bad_keys_ids_and_push_targets() {
        let ok = || EnrollRequest {
            device_id: "d1".into(),
            name: None,
            alg: KeyAlg::Ed25519,
            public_key: base64::engine::general_purpose::STANDARD.encode([7u8; 32]),
            push: None,
        };
        // 32 arbitrary bytes are not always a valid Ed25519 point, so use a real key.
        let (dev, _) = ed_device();
        let good = EnrollRequest {
            public_key: dev.public_key.clone(),
            ..ok()
        };
        assert!(build_record("ana", good, Utc::now()).is_ok());
        for bad in [
            EnrollRequest {
                device_id: "".into(),
                ..ok()
            },
            EnrollRequest {
                device_id: "a b".into(),
                ..ok()
            },
            EnrollRequest {
                public_key: "!!!".into(),
                ..ok()
            },
            EnrollRequest {
                public_key: base64::engine::general_purpose::STANDARD.encode([1u8; 5]),
                ..ok()
            },
            EnrollRequest {
                alg: KeyAlg::P256,
                ..ok()
            },
            EnrollRequest {
                push: Some(PushTarget {
                    kind: "".into(),
                    token: "t".into(),
                }),
                ..ok()
            },
            EnrollRequest {
                push: Some(PushTarget {
                    kind: "fcm".into(),
                    token: "".into(),
                }),
                ..ok()
            },
        ] {
            assert!(build_record("ana", bad, Utc::now()).is_err());
        }
    }

    #[test]
    fn the_payload_is_readable_and_names_everything_signed() {
        let a = approval();
        let p = signing_payload(&a, ApprovalStatus::Approved, "c0ffee");
        assert!(p.starts_with("keep-approval-v1\n"));
        let wants = [
            format!("approval: {}", a.id),
            "decision: approved".to_string(),
            "kind: send".to_string(),
            "subject: mail.example".to_string(),
            "challenge: c0ffee".to_string(),
        ];
        for want in &wants {
            assert!(p.contains(want.as_str()), "{want}\n{p}");
        }
        let info = signing_info(&server_key(), &a);
        assert_eq!(info["challenge"].as_str().unwrap().len(), 32);
        assert_eq!(info["action_sha256"], action_sha256(&a));
    }

    // ---- cross-language test vectors ---------------------------------------------------------
    //
    // `docs/merion/mobile/test-vectors.json` pins the exact payload and signatures, so a phone client
    // in any language can prove it agrees with this verifier. The signatures are deterministic
    // (Ed25519, and P-256 with RFC 6979), so the file is stable. Regenerate it after a deliberate
    // change with `MERION_WRITE_VECTORS=1 cargo test --lib test_vectors`.

    fn vector_approval() -> ApprovalRecord {
        let mut a = approval();
        a.id = Uuid::parse_str("11111111-1111-4111-8111-111111111111").unwrap();
        a.created_at = DateTime::from_timestamp(1_790_000_000, 0).unwrap();
        a.subject = Some("mail.example".into());
        a.planned_action = Some(json!({"method": "POST", "body_sha256": "abc"}));
        a
    }

    fn build_vectors() -> Value {
        let a = vector_approval();
        let key =
            crate::authz::signing_key(Some(&crate::fixture::text("vector-operator-token")), None)
                .unwrap();
        let ch = challenge(&key, &a);
        let p256_sk = p256::ecdsa::SigningKey::from_slice(&[0x42u8; 32]).unwrap();
        let ed_sk = ed25519_dalek::SigningKey::from_bytes(&[0x07u8; 32]);
        let std = base64::engine::general_purpose::STANDARD;
        let mut cases = Vec::new();
        let mut cases_merion = Vec::new();
        // The same cases under `merion-approval-v1`, in their own array so a client that checks
        // every entry of `cases` against its own payload builder is unaffected.
        for decision in [ApprovalStatus::Approved, ApprovalStatus::Denied] {
            let payload = signing_payload_as(PAYLOAD_FORMAT_MERION, &a, decision, &ch);
            cases_merion.push(json!({
                "alg": "ed25519",
                "decision": if decision == ApprovalStatus::Approved { "approved" } else { "denied" },
                "public_key": std.encode(ed_sk.verifying_key().as_bytes()),
                "payload": payload,
                "signature": std.encode(ed_sk.sign(payload.as_bytes()).to_bytes()),
            }));
        }
        for decision in [ApprovalStatus::Approved, ApprovalStatus::Denied] {
            let payload = signing_payload(&a, decision, &ch);
            let der: p256::ecdsa::Signature = p256_sk.sign(payload.as_bytes());
            cases.push(json!({
                "alg": "p256",
                "decision": if decision == ApprovalStatus::Approved { "approved" } else { "denied" },
                "public_key": std.encode(p256_sk.verifying_key().to_public_key_der().unwrap().as_bytes()),
                "payload": payload,
                "signature": std.encode(der.to_der().as_bytes()),
                "signature_raw": std.encode(der.to_bytes()),
            }));
            cases.push(json!({
                "alg": "ed25519",
                "decision": if decision == ApprovalStatus::Approved { "approved" } else { "denied" },
                "public_key": std.encode(ed_sk.verifying_key().as_bytes()),
                "payload": payload,
                "signature": std.encode(ed_sk.sign(payload.as_bytes()).to_bytes()),
            }));
        }
        json!({
            "format": PAYLOAD_FORMAT,
            "note": "Generated by the runtime's test suite. The payload is the exact text to sign; the challenge comes from the server.",
            "operator_token": "vector-operator-token",
            "approval": {
                "id": a.id,
                "kind": a.kind.as_str(),
                "subject": a.subject,
                "created_at_unix": a.created_at.timestamp(),
                "planned_action": a.planned_action,
            },
            "sign": signing_info(&key, &a),
            "cases": cases,
            "cases_merion": cases_merion,
        })
    }

    #[test]
    fn test_vectors_are_current_and_verify() {
        let path = concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../docs/merion/mobile/test-vectors.json"
        );
        let want = build_vectors();
        let write_vectors = std::env::var("MERION_WRITE_VECTORS").is_ok_and(|v| v == "1")
            || std::env::var("KEEP_WRITE_VECTORS").is_ok_and(|v| v == "1");
        if write_vectors {
            std::fs::create_dir_all(std::path::Path::new(path).parent().unwrap()).unwrap();
            std::fs::write(path, serde_json::to_string_pretty(&want).unwrap() + "\n").unwrap();
        }
        let on_disk: Value = serde_json::from_str(&std::fs::read_to_string(path).expect(
            "docs/merion/mobile/test-vectors.json is missing: MERION_WRITE_VECTORS=1 cargo test --lib test_vectors",
        ))
        .unwrap();
        assert_eq!(
            on_disk, want,
            "the committed vectors are stale: regenerate with MERION_WRITE_VECTORS=1"
        );
        // Every committed signature verifies under the server's own code.
        for c in want["cases"].as_array().unwrap() {
            let alg = if c["alg"] == "p256" {
                KeyAlg::P256
            } else {
                KeyAlg::Ed25519
            };
            let dev = DeviceRecord {
                user_id: "ana".into(),
                device_id: "vec".into(),
                name: None,
                alg,
                public_key: c["public_key"].as_str().unwrap().into(),
                push: None,
                created_at: Utc::now(),
            };
            let payload = c["payload"].as_str().unwrap().as_bytes();
            assert!(
                verify_signature(&dev, payload, c["signature"].as_str().unwrap()),
                "{c}"
            );
            if let Some(raw) = c.get("signature_raw") {
                assert!(
                    verify_signature(&dev, payload, raw.as_str().unwrap()),
                    "raw {c}"
                );
            }
            // The other decision's signature must not verify this payload.
            let flipped = payload.to_vec();
            let text = String::from_utf8(flipped)
                .unwrap()
                .replace("decision: approved", "decision: denied");
            if text.as_bytes() != payload {
                assert!(!verify_signature(
                    &dev,
                    text.as_bytes(),
                    c["signature"].as_str().unwrap()
                ));
            }
        }
    }

    #[test]
    fn a_signature_over_either_format_verifies_and_the_formats_do_not_mix() {
        let key = b"k".to_vec();
        let a = approval();
        let (dev, sk) = ed_device();
        let ch = challenge(&key, &a);
        let sign = |format: &str, decision| {
            let payload = signing_payload_as(format, &a, decision, &ch);
            base64::engine::general_purpose::STANDARD.encode(sk.sign(payload.as_bytes()).to_bytes())
        };
        let now = Utc::now();
        for f in ACCEPTED_FORMATS {
            let sig = sign(f, ApprovalStatus::Approved);
            assert_eq!(
                verify_decision_format(&key, &dev, &a, ApprovalStatus::Approved, &sig, now),
                Ok(f)
            );
            // The same signature never verifies for a different decision.
            assert_eq!(
                verify_decision_format(&key, &dev, &a, ApprovalStatus::Denied, &sig, now),
                Err(SignError::BadSignature)
            );
        }
        // A format the host does not list is refused.
        let other = sign("other-approval-v1", ApprovalStatus::Approved);
        assert_eq!(
            verify_decision(&key, &dev, &a, ApprovalStatus::Approved, &other, now),
            Err(SignError::BadSignature)
        );
        // The announced info keeps the legacy name and lists both.
        let info = signing_info(&key, &a);
        assert_eq!(info["format"], "keep-approval-v1");
        assert_eq!(
            info["formats"],
            json!(["keep-approval-v1", "merion-approval-v1"])
        );
    }
}
