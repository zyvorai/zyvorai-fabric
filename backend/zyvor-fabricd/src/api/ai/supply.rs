// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: LicenseRef-Zyvor-Production-2.0

//! Content-addressed model supply chain helpers.
//!
//! An optimization never replaces the source digest. It names a derived
//! artifact whose digest is the hash of the source digest and the kind.

use sha2::{Digest, Sha256};
use subtle::ConstantTimeEq;

pub fn derived_digest(source_digest: &str, kind: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(source_digest.as_bytes());
    hasher.update(b"\n");
    hasher.update(kind.as_bytes());
    hex(&hasher.finalize())
}

pub fn optimization_kind(kind: &str) -> Result<&'static str, String> {
    match kind.trim().to_ascii_lowercase().as_str() {
        "awq" => Ok("awq"),
        "gptq" => Ok("gptq"),
        "fp16" => Ok("fp16"),
        "bf16" => Ok("bf16"),
        "gguf" => Ok("gguf"),
        "tensorrt" => Ok("tensorrt"),
        other => Err(format!("unsupported optimization '{other}'")),
    }
}

pub fn file_count_allowed(count: usize, limit: usize) -> Result<(), String> {
    if count > limit {
        return Err(format!("model has {count} files; limit is {limit}"));
    }
    Ok(())
}

pub fn max_files() -> usize {
    std::env::var("FLUXVM_AI_MODEL_MAX_FILES")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(10_000)
}

/// HMAC-SHA256 of the digest, hex-encoded, compared in constant time.
pub fn signature_matches(digest: &str, signature_hex: &str, key: &str) -> bool {
    let expect = hmac_sha256_hex(key.as_bytes(), digest.as_bytes());
    bool::from(expect.as_bytes().ct_eq(signature_hex.trim().as_bytes()))
}

pub fn require_signature(digest: Option<&str>, signature: &str) -> Result<(), String> {
    let digest = digest.ok_or_else(|| "model digest is missing".to_string())?;
    let key = std::env::var("FLUXVM_AI_MODEL_SIGNING_KEY").map_err(|_| {
        "model signature is set but FLUXVM_AI_MODEL_SIGNING_KEY is unset".to_string()
    })?;
    if key.is_empty() {
        return Err("FLUXVM_AI_MODEL_SIGNING_KEY is empty".into());
    }
    if signature_matches(digest, signature, &key) {
        Ok(())
    } else {
        Err("model signature does not match the digest".into())
    }
}

fn hmac_sha256_hex(key: &[u8], msg: &[u8]) -> String {
    const BLOCK: usize = 64;
    let mut key_block = [0u8; BLOCK];
    if key.len() > BLOCK {
        let digested = Sha256::digest(key);
        key_block[..32].copy_from_slice(&digested);
    } else {
        key_block[..key.len()].copy_from_slice(key);
    }
    let mut ipad = [0x36u8; BLOCK];
    let mut opad = [0x5cu8; BLOCK];
    for i in 0..BLOCK {
        ipad[i] ^= key_block[i];
        opad[i] ^= key_block[i];
    }
    let mut inner = Sha256::new();
    inner.update(ipad);
    inner.update(msg);
    let mut outer = Sha256::new();
    outer.update(opad);
    outer.update(inner.finalize());
    hex(&outer.finalize())
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn derived_digest_is_stable_and_distinct_from_the_source() {
        let source = "abc123";
        let derived = derived_digest(source, "awq");
        assert_eq!(derived, derived_digest(source, "awq"));
        assert_ne!(derived, source);
        assert_ne!(derived, derived_digest(source, "gguf"));
    }

    #[test]
    fn file_count_and_kind_gates() {
        assert!(file_count_allowed(10, 10).is_ok());
        assert!(file_count_allowed(11, 10).is_err());
        assert_eq!(optimization_kind("AWQ").unwrap(), "awq");
        assert!(optimization_kind("pickle").is_err());
    }

    #[test]
    fn signature_rejects_a_different_digest() {
        assert!(signature_matches(
            "digest",
            &hmac_sha256_hex(b"key", b"digest"),
            "key"
        ));
        assert!(!signature_matches(
            "other",
            &hmac_sha256_hex(b"key", b"digest"),
            "key"
        ));
    }
}
