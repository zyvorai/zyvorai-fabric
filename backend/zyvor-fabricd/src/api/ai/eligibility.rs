// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: LicenseRef-Zyvor-Production-2.0
//!
//! When a replica may receive Maglev traffic.
//! Stale or failed scrapes are unhealthy — they must not look idle.

use chrono::Utc;

use super::types::{InferenceReplica, ReplicaMetrics};

const FRESH_SECS: i64 = 30;

pub fn metrics_fresh(metrics: Option<&ReplicaMetrics>) -> bool {
    let Some(m) = metrics else {
        return false;
    };
    if m.source.as_deref() == Some("scrape_failed") {
        return false;
    }
    let Some(at) = m.scraped_at else {
        return false;
    };
    Utc::now().signed_duration_since(at).num_seconds() < FRESH_SECS
}

/// Ready, not draining, and backed by a fresh successful scrape.
pub fn replica_serving(rep: &InferenceReplica) -> bool {
    rep.ready && !rep.draining && metrics_fresh(rep.metrics.as_ref())
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Duration;

    fn metrics(source: &str, age_secs: i64) -> ReplicaMetrics {
        ReplicaMetrics {
            source: Some(source.into()),
            scraped_at: Some(Utc::now() - Duration::seconds(age_secs)),
            ..Default::default()
        }
    }

    #[test]
    fn stale_or_failed_is_unhealthy() {
        assert!(!metrics_fresh(None));
        assert!(!metrics_fresh(Some(&metrics("scrape_failed", 0))));
        assert!(!metrics_fresh(Some(&metrics("vllm_prometheus", 120))));
        assert!(metrics_fresh(Some(&metrics("vllm_prometheus", 1))));
    }
}
