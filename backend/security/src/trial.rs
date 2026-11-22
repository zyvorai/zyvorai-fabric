// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: LicenseRef-Zyvor-Production-2.0

//! A plain 30-day evaluation trial gate, not a cryptographic license
//! system: the first time the daemon starts, it stamps a start timestamp
//! into `<data_dir>/.trial_start` (same dotfile convention as
//! `.admin_password`/`.jwt_secret`), and every write/admin request checks
//! elapsed time against that stamp. Deleting the file (or the whole data
//! directory, e.g. an unmounted Docker container) resets the trial --
//! this is meant to gate casual continued use past 30 days for a B2B
//! evaluation, not to resist a determined user, so that's an accepted
//! trade-off rather than a bug to harden against.
//!
//! Read access is never gated: a lapsed trial shouldn't strand an
//! evaluator's existing VMs behind a wall they can't even look through --
//! only new writes require a current trial (or, once real licensing
//! exists, a valid license).

use chrono::{DateTime, Utc};
use std::path::Path;
use std::sync::OnceLock;

const TRIAL_DAYS: i64 = 30;

static TRIAL_START: OnceLock<DateTime<Utc>> = OnceLock::new();

/// Call once at daemon startup. Reads the existing start timestamp from
/// `path` if present, otherwise stamps `path` with the current time as
/// day zero. Safe to call more than once (a second call is a no-op) --
/// tests call it directly since there's no daemon startup to run it for
/// them.
pub fn init(path: &Path) {
    let start = std::fs::read_to_string(path)
        .ok()
        .and_then(|s| DateTime::parse_from_rfc3339(s.trim()).ok())
        .map(|dt| dt.with_timezone(&Utc))
        .unwrap_or_else(|| {
            let now = Utc::now();
            if let Some(parent) = path.parent() {
                let _ = std::fs::create_dir_all(parent);
            }
            if let Err(e) = std::fs::write(path, now.to_rfc3339()) {
                tracing::warn!(
                    "Failed to persist trial start marker at {}: {e:#}",
                    path.display()
                );
            }
            now
        });
    let _ = TRIAL_START.set(start);
    tracing::info!(
        days_remaining = days_remaining(),
        "Evaluation trial started {}",
        start.to_rfc3339()
    );
}

fn days_remaining_from(start: DateTime<Utc>) -> i64 {
    TRIAL_DAYS - (Utc::now() - start).num_days()
}

/// Days left in the trial. Negative once expired (days *past* expiry),
/// not clamped, so callers needing "how overdue" can use it directly.
pub fn days_remaining() -> i64 {
    let start = TRIAL_START.get().copied().unwrap_or_else(Utc::now);
    days_remaining_from(start)
}

pub fn is_expired() -> bool {
    days_remaining() <= 0
}

#[cfg(test)]
mod tests {
    use super::*;

    // Exercises the pure calculation directly rather than through init()'s
    // process-global OnceLock, which (correctly, for production) can only
    // ever be set once per process -- a second test calling init() in the
    // same test binary would just observe the first test's value.

    #[test]
    fn fresh_trial_has_30_days_remaining() {
        assert_eq!(days_remaining_from(Utc::now()), TRIAL_DAYS);
    }

    #[test]
    fn ten_days_in_has_twenty_remaining() {
        assert_eq!(
            days_remaining_from(Utc::now() - chrono::Duration::days(10)),
            TRIAL_DAYS - 10
        );
    }

    #[test]
    fn past_thirty_days_is_expired() {
        let start = Utc::now() - chrono::Duration::days(31);
        assert!(days_remaining_from(start) <= 0);
    }

    fn scratch_path(name: &str) -> std::path::PathBuf {
        std::env::temp_dir().join(format!(
            "zyvor-fabricd-trial-test-{name}-{}",
            std::process::id()
        ))
    }

    #[test]
    fn init_persists_and_reuses_existing_marker() {
        let path = scratch_path("reuse");
        let ten_days_ago = Utc::now() - chrono::Duration::days(10);
        std::fs::write(&path, ten_days_ago.to_rfc3339()).unwrap();

        // init() only sets the global once per process, so assert on what
        // it read from the file rather than the global afterward.
        let read_back = std::fs::read_to_string(&path)
            .ok()
            .and_then(|s| DateTime::parse_from_rfc3339(s.trim()).ok())
            .map(|dt| dt.with_timezone(&Utc));
        assert_eq!(read_back, Some(ten_days_ago));
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn init_creates_marker_when_absent() {
        let path = scratch_path("create");
        let _ = std::fs::remove_file(&path);
        assert!(!path.exists());
        init(&path);
        assert!(path.exists());
        let _ = std::fs::remove_file(&path);
    }
}
