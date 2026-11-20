// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: LicenseRef-Zyvor-Production-1.0

//! Keep attestation receipt — honest until FluxVM flips SNP/TDX verified flags.
//!
//! Cockpit and the console Keep view surface this object so a `software-test`
//! session is never labeled unread-by-operator. Keep 0.2 hardware runs flip
//! [`AttestationReceipt::snp_launch_verified`] /
//! [`AttestationReceipt::tdx_launch_verified`] only after a real launch.

use crate::model::ConfidentialStatus;
use serde::Serialize;

/// Structured receipt returned on `GET /v1/sessions/{id}/cockpit`.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct AttestationReceipt {
    /// FluxVM / Keep security profile requested (`measured`, `confidential-snp`, …).
    pub security_profile: Option<String>,
    /// Soft pin until FluxVM returns a launch measurement (agent version today).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub image_hash: Option<String>,
    /// Evidence class. Never `sev-snp` / `tdx` unless the matching verified flag is set.
    pub evidence_class: String,
    /// Always false until FluxVM reports a verified SNP launch.
    pub snp_launch_verified: bool,
    /// Always false until FluxVM reports a verified TDX launch.
    pub tdx_launch_verified: bool,
    /// False for confidential profiles / active confidential guests — no host recover.
    pub host_recover_allowed: bool,
    /// True whenever evidence is `software-test` (host can still see guest memory).
    pub operator_can_read: bool,
    pub honesty: String,
}

/// Build a receipt from config + session confidential status.
///
/// `snp_launch_verified` / `tdx_launch_verified` stay false until a hardware
/// integration wires FluxVM flags through; callers must not invent them.
pub fn build_receipt(
    security_profile: Option<&str>,
    image_hash: Option<String>,
    confidential: Option<&ConfidentialStatus>,
    snp_launch_verified: bool,
    tdx_launch_verified: bool,
) -> AttestationReceipt {
    let evidence_class = evidence_class(security_profile, snp_launch_verified, tdx_launch_verified);
    let host_recover_allowed = host_recover_allowed(security_profile, confidential);
    let operator_can_read = evidence_class == "software-test";
    let honesty = if operator_can_read {
        "Evidence class software-test: the host can still see this VM. Keep 0.2 + attested hardware is required before claiming otherwise.".into()
    } else {
        format!(
            "Evidence class {evidence_class}: launch verified on this host. Host recover is {}.",
            if host_recover_allowed {
                "allowed only via dual-key break-glass on measured/standard"
            } else {
                "forbidden (confidential)"
            }
        )
    };
    AttestationReceipt {
        security_profile: security_profile.map(str::to_owned),
        image_hash,
        evidence_class: evidence_class.into(),
        snp_launch_verified,
        tdx_launch_verified,
        host_recover_allowed,
        operator_can_read,
        honesty,
    }
}

pub fn evidence_class(profile: Option<&str>, snp: bool, tdx: bool) -> &'static str {
    match profile {
        Some("confidential-snp") if snp => "sev-snp",
        Some("confidential-tdx") if tdx => "tdx",
        _ => "software-test",
    }
}

pub fn host_recover_allowed(
    profile: Option<&str>,
    confidential: Option<&ConfidentialStatus>,
) -> bool {
    if confidential.is_some_and(|c| c.active) {
        return false;
    }
    !matches!(profile, Some(p) if p.starts_with("confidential"))
}

/// Refuse host-channel guest-agent calls when confidential launch is active.
pub fn host_channel_forbidden(confidential: Option<&ConfidentialStatus>) -> Option<&'static str> {
    if confidential.is_some_and(|c| c.active) {
        Some("host guest-agent channel disabled for confidential sessions (Keep 0.2)")
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn measured_stays_software_test_without_verified_flags() {
        let r = build_receipt(Some("measured"), Some("agent@1".into()), None, false, false);
        assert_eq!(r.evidence_class, "software-test");
        assert!(r.operator_can_read);
        assert!(r.host_recover_allowed);
        assert!(!r.snp_launch_verified);
    }

    #[test]
    fn confidential_profile_without_verified_stays_software_test() {
        let r = build_receipt(Some("confidential-snp"), None, None, false, false);
        assert_eq!(r.evidence_class, "software-test");
        assert!(r.operator_can_read);
        assert!(!r.host_recover_allowed);
    }

    #[test]
    fn snp_verified_flips_evidence_class() {
        let r = build_receipt(Some("confidential-snp"), None, None, true, false);
        assert_eq!(r.evidence_class, "sev-snp");
        assert!(!r.operator_can_read);
        assert!(!r.host_recover_allowed);
    }

    #[test]
    fn active_confidential_forbids_host_channel_and_recover() {
        let st = ConfidentialStatus {
            active: true,
            tech: Some("sev-snp".into()),
            reason: "ok".into(),
        };
        assert!(!host_recover_allowed(Some("measured"), Some(&st)));
        assert!(host_channel_forbidden(Some(&st)).is_some());
    }
}
