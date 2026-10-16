// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: Apache-2.0

//! Keep scoreboard — the claims we are allowed to make against Muse.
//!
//! A scoreboard receipt is not an attestation. It is a signed statement of
//! what this run actually measured. Hardware flags stay false until a
//! verified SNP/TDX launch sets them. Training stays off. Secrets never
//! enter the pack.

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

pub const SCOREBOARD_SCHEMA: &str = "merion.scoreboard/v1";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum EvidenceClass {
    /// Unit / stub / software measurement. Host can still see the cell.
    SoftwareTest,
    /// Real Firecracker/KVM launch, still operator-readable.
    Measured,
    /// Hardware launch verified and host-recover denied. Not shipped.
    Confidential,
}

impl EvidenceClass {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::SoftwareTest => "software-test",
            Self::Measured => "measured",
            Self::Confidential => "confidential",
        }
    }

    /// What marketing is allowed to say. Confidential is unreachable here.
    pub fn claim(self) -> &'static str {
        match self {
            Self::SoftwareTest => "software-test measurement; host can read the cell",
            Self::Measured => "measured microVM; host can still read the cell",
            Self::Confidential => "refused: confidential is gated on a verified SNP/TDX run",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ScoreboardReceipt {
    pub schema: String,
    pub policy_sha256: String,
    pub evidence_class: EvidenceClass,
    pub egress_connects: u64,
    pub session_frozen: bool,
    pub training_default: &'static str,
    pub model_sockets: Vec<String>,
    pub pack_roundtrip: bool,
    pub secrets_in_pack: bool,
    pub snp_launch_verified: bool,
    pub tdx_launch_verified: bool,
    pub operator_can_read: bool,
    pub host_recover_allowed: bool,
}

impl ScoreboardReceipt {
    /// Honest default for CI and the laptop gate. Never claims hardware.
    pub fn software_test(policy_sha256: impl Into<String>) -> Self {
        Self {
            schema: SCOREBOARD_SCHEMA.to_string(),
            policy_sha256: policy_sha256.into(),
            evidence_class: EvidenceClass::SoftwareTest,
            egress_connects: 0,
            session_frozen: false,
            training_default: "off",
            model_sockets: Vec::new(),
            pack_roundtrip: false,
            secrets_in_pack: false,
            snp_launch_verified: false,
            tdx_launch_verified: false,
            operator_can_read: true,
            host_recover_allowed: true,
        }
    }

    pub fn with_models(mut self, sockets: impl IntoIterator<Item = impl Into<String>>) -> Self {
        self.model_sockets = sockets.into_iter().map(Into::into).collect();
        self
    }

    pub fn with_pack_roundtrip(mut self, ok: bool) -> Self {
        self.pack_roundtrip = ok;
        self.secrets_in_pack = false;
        self
    }

    pub fn freeze_on_egress(mut self, connects: u64) -> Self {
        self.egress_connects = connects;
        self.session_frozen = connects > 0;
        self
    }

    /// Fail closed. A receipt that over-claims is not a receipt.
    pub fn validate(&self) -> Result<(), String> {
        if self.schema != SCOREBOARD_SCHEMA {
            return Err(format!("schema {} != {SCOREBOARD_SCHEMA}", self.schema));
        }
        if self.policy_sha256.len() != 64
            || !self.policy_sha256.chars().all(|c| c.is_ascii_hexdigit())
        {
            return Err("policy_sha256 must be 64 hex chars".into());
        }
        if self.training_default != "off" {
            return Err("training_default must be off".into());
        }
        if self.secrets_in_pack {
            return Err("secrets must not be in the pack".into());
        }
        if self.snp_launch_verified || self.tdx_launch_verified {
            return Err("hardware flags are gated; this builder cannot set them".into());
        }
        if !self.operator_can_read || !self.host_recover_allowed {
            return Err("measured/software-test still allows host read and host recover".into());
        }
        if self.evidence_class == EvidenceClass::Confidential {
            return Err(EvidenceClass::Confidential.claim().into());
        }
        if self.egress_connects > 0 && !self.session_frozen {
            return Err("egress > 0 must freeze the session".into());
        }
        if self.model_sockets.len() > 1 {
            // Any two sockets naming the same value are not a two-model proof, not only every
            // socket in the list being the same value (a `windows(2)`-adjacency check would miss
            // e.g. ["a", "b", "a"] — a non-adjacent repeat).
            let unique: std::collections::HashSet<&String> = self.model_sockets.iter().collect();
            if unique.len() != self.model_sockets.len() {
                return Err("two-model proof needs distinct sockets".into());
            }
        }
        Ok(())
    }
}

pub fn policy_sha256(yaml: &str) -> String {
    let mut h = Sha256::new();
    h.update(yaml.as_bytes());
    hex_encode(&h.finalize())
}

fn hex_encode(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        out.push(HEX[(b >> 4) as usize] as char);
        out.push(HEX[(b & 0xf) as usize] as char);
    }
    out
}

/// Pack body is policy + manifest + notes. Vault material is rejected by name.
pub fn pack_allows(files: &[(String, String)]) -> Result<(), String> {
    const BANNED: &[&str] = &[
        "vault",
        "secret",
        "credential",
        "token",
        "private_key",
        "unwrap",
    ];
    for (name, body) in files {
        let lower = name.to_ascii_lowercase();
        if BANNED.iter().any(|b| lower.contains(b)) {
            return Err(format!("refusing to pack {name}"));
        }
        if body.contains("BEGIN PRIVATE KEY") || body.contains("ZYVOR_AGENT_API_TOKEN=") {
            return Err(format!("refusing secret-looking body in {name}"));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    const POLICY: &str = "\
schema: merion.policy/v1
training: off
egress:
  default: deny
  allow: []
approvals:
  buy: out-of-band
  send: out-of-band
  delete: out-of-band
model_socket: socket://lab/a
";

    #[test]
    fn software_test_receipt_is_honest() {
        let sha = policy_sha256(POLICY);
        let r = ScoreboardReceipt::software_test(sha)
            .with_models(["socket://lab/a", "socket://lab/b"])
            .with_pack_roundtrip(true)
            .freeze_on_egress(0);
        r.validate().unwrap();
        assert_eq!(r.evidence_class.as_str(), "software-test");
        assert!(r.operator_can_read);
        assert!(!r.snp_launch_verified);
        assert_eq!(r.model_sockets.len(), 2);
    }

    #[test]
    fn egress_slip_freezes() {
        let r = ScoreboardReceipt::software_test(policy_sha256(POLICY)).freeze_on_egress(1);
        r.validate().unwrap();
        assert!(r.session_frozen);
    }

    #[test]
    fn cannot_claim_confidential() {
        let mut r = ScoreboardReceipt::software_test(policy_sha256(POLICY));
        r.evidence_class = EvidenceClass::Confidential;
        r.operator_can_read = false;
        r.host_recover_allowed = false;
        assert!(r.validate().is_err());
    }

    #[test]
    fn pack_rejects_vault() {
        let ok = vec![
            ("merion.policy.yaml".into(), POLICY.into()),
            ("agent.json".into(), "{\"name\":\"pdf-brief\"}".into()),
            ("MIGRATION.md".into(), "notes only".into()),
        ];
        pack_allows(&ok).unwrap();
        let bad = vec![("vault.json".into(), "{}".into())];
        assert!(pack_allows(&bad).is_err());
    }

    #[test]
    fn two_identical_sockets_are_refused() {
        let r = ScoreboardReceipt::software_test(policy_sha256(POLICY))
            .with_models(["socket://lab/a", "socket://lab/a"]);
        assert!(
            r.validate().is_err(),
            "identical sockets are not a two-model proof"
        );
    }

    /// A non-adjacent repeat (a, b, a) is not "every socket the same" — an adjacency-based check
    /// (comparing each socket only to its neighbor) would miss this. Any repeat anywhere in the
    /// list must be refused, since two of the three claimed sockets are not actually distinct.
    #[test]
    fn a_non_adjacent_repeat_is_still_refused() {
        let r = ScoreboardReceipt::software_test(policy_sha256(POLICY)).with_models([
            "socket://lab/a",
            "socket://lab/b",
            "socket://lab/a",
        ]);
        assert!(
            r.validate().is_err(),
            "a repeated socket anywhere in the list is not a distinct-sockets proof"
        );
    }
}
