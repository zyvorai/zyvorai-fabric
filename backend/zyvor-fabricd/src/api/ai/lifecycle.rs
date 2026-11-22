// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: LicenseRef-Zyvor-Production-2.0

//! Schema migration and chaos qualification for the inference control plane.
//!
//! Same-host file leases resume a rollout after fabricd restarts. They are
//! not a three-node quorum. Leader loss stays degraded until this process
//! has committed a three-voter Raft membership.

pub const SCHEMA_VERSION: u32 = 2;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChaosEvent {
    FabricdCrash,
    LeaderLoss,
    StateStoreUnavailable,
    GpuNodeLoss,
    SitePartition,
    MaglevFailure,
    ModelCacheCorruption,
    SlowModelDownload,
    ExpiredCertificate,
    RolloutInterrupted,
    GatewayOverload,
    ClientDisconnect,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Recovery {
    /// Durable records are enough to continue.
    Resumes,
    /// The last healthy revision or local snapshot stays, and new placement waits.
    Degraded,
    /// The request fails closed instead of guessing.
    FailClosed,
}

pub struct Objectives {
    pub rto_secs: u64,
    pub rpo_secs: u64,
}

pub fn objectives() -> Objectives {
    Objectives {
        rto_secs: 120,
        rpo_secs: 0,
    }
}

pub fn migrate(from: u32) -> Result<u32, String> {
    match from {
        1 | 2 => Ok(SCHEMA_VERSION),
        other => Err(format!("unsupported inference schema {other}")),
    }
}

/// `consensus` is true only when a three-voter membership has committed.
pub fn recovery(event: ChaosEvent, consensus: bool) -> Recovery {
    match event {
        ChaosEvent::FabricdCrash
        | ChaosEvent::RolloutInterrupted
        | ChaosEvent::SlowModelDownload
        | ChaosEvent::GpuNodeLoss
        | ChaosEvent::ClientDisconnect => Recovery::Resumes,
        ChaosEvent::GatewayOverload | ChaosEvent::ExpiredCertificate => Recovery::FailClosed,
        ChaosEvent::ModelCacheCorruption => Recovery::FailClosed,
        ChaosEvent::MaglevFailure => Recovery::Degraded,
        ChaosEvent::LeaderLoss | ChaosEvent::StateStoreUnavailable | ChaosEvent::SitePartition => {
            if consensus {
                Recovery::Resumes
            } else {
                Recovery::Degraded
            }
        }
    }
}

pub fn consensus_configured() -> bool {
    std::env::var("FLUXVM_AI_CONSENSUS")
        .map(|v| v == "external")
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn schema_one_migrates_and_unknown_versions_fail() {
        assert_eq!(migrate(1).unwrap(), 2);
        assert!(migrate(9).is_err());
    }

    #[test]
    fn crash_resumes_and_leader_loss_stays_degraded_without_quorum() {
        assert_eq!(recovery(ChaosEvent::FabricdCrash, false), Recovery::Resumes);
        assert_eq!(recovery(ChaosEvent::LeaderLoss, false), Recovery::Degraded);
        assert_eq!(recovery(ChaosEvent::LeaderLoss, true), Recovery::Resumes);
        assert_eq!(
            recovery(ChaosEvent::ModelCacheCorruption, false),
            Recovery::FailClosed
        );
        assert_eq!(objectives().rpo_secs, 0);
        assert!(!consensus_configured());
    }
}
