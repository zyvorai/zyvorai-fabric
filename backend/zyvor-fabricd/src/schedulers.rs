// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: LicenseRef-Zyvor-Production-1.0

//! In-process replacement for the retired zyvor-fabricd-backup.timer/
//! zyvor-fabricd-cleanup.timer systemd units (systemd-removal migration
//! plan, Phase 6): a tokio task that wakes at the same wall-clock times
//! those timers used — daily at 02:00 for backup, Sunday at 03:00 for
//! cleanup — and shells out to the same scripts their `.service` units
//! ran ($libexecdir/zyvor-fabricd/{backup-vms,cleanup-store}), so the
//! daemon needs neither a systemd timer nor a cron dependency to keep
//! doing this on a schedule.

use std::path::PathBuf;
use std::sync::Arc;

use chrono::{Datelike, Duration as ChronoDuration, NaiveTime, Utc, Weekday};

use crate::server::AppState;

fn libexec_dir() -> PathBuf {
    std::env::var("ZYVOR_FABRICD_LIBEXEC_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("/usr/libexec/zyvor-fabricd"))
}

/// Time until the next occurrence of `time` today-or-tomorrow — matches
/// systemd's `OnCalendar=*-*-* HH:MM:00`.
fn duration_until_daily(time: NaiveTime) -> std::time::Duration {
    let now = Utc::now();
    let mut next = now.date_naive().and_time(time).and_utc();
    if next <= now {
        next += ChronoDuration::days(1);
    }
    (next - now)
        .to_std()
        .unwrap_or(std::time::Duration::from_secs(60))
}

/// Time until the next occurrence of `weekday` at `time` — matches
/// systemd's `OnCalendar=<Weekday> *-*-* HH:MM:00`.
fn duration_until_weekly(weekday: Weekday, time: NaiveTime) -> std::time::Duration {
    let now = Utc::now();
    let mut next = now.date_naive().and_time(time).and_utc();
    let days_ahead = (7 + weekday.num_days_from_monday() as i64
        - next.weekday().num_days_from_monday() as i64)
        % 7;
    next += ChronoDuration::days(days_ahead);
    if next <= now {
        next += ChronoDuration::days(7);
    }
    (next - now)
        .to_std()
        .unwrap_or(std::time::Duration::from_secs(60))
}

/// Run one of the libexec scheduler scripts, pointed at this daemon's own
/// listen address. Missing script = quietly skip (not every install ships
/// them — e.g. a plain `cargo run` dev checkout), not an error.
async fn run_script(name: &str, script: &str, api_url: &str) {
    let path = libexec_dir().join(script);
    if !path.exists() {
        tracing::debug!(
            "{name} scheduler: {} not installed, skipping this run",
            path.display()
        );
        return;
    }

    tracing::info!("{name} scheduler: running {}", path.display());
    let output = tokio::process::Command::new(&path)
        .env("ZYVOR_FABRICD_URL", api_url)
        .output()
        .await;

    match output {
        Ok(out) if out.status.success() => {
            tracing::info!("{name} scheduler: completed successfully");
        }
        Ok(out) => {
            tracing::error!(
                "{name} scheduler: {} exited with {}: {}",
                script,
                out.status,
                String::from_utf8_lossy(&out.stderr).trim()
            );
        }
        Err(e) => {
            tracing::error!("{name} scheduler: failed to run {}: {e}", path.display());
        }
    }
}

fn local_api_url(state: &AppState) -> String {
    let port = state
        .config
        .daemon
        .listen
        .rsplit(':')
        .next()
        .unwrap_or("9095");
    format!("http://127.0.0.1:{port}")
}

/// Replaces `zyvor-fabricd-backup.timer` (`OnCalendar=*-*-* 02:00:00`).
pub async fn run_backup_scheduler(state: Arc<AppState>) {
    let api_url = local_api_url(&state);
    loop {
        let wait = duration_until_daily(NaiveTime::from_hms_opt(2, 0, 0).unwrap());
        tokio::time::sleep(wait).await;
        run_script("backup", "backup-vms", &api_url).await;
    }
}

/// Replaces `zyvor-fabricd-cleanup.timer` (`OnCalendar=Sun *-*-* 03:00:00`).
pub async fn run_cleanup_scheduler(state: Arc<AppState>) {
    let api_url = local_api_url(&state);
    loop {
        let wait = duration_until_weekly(Weekday::Sun, NaiveTime::from_hms_opt(3, 0, 0).unwrap());
        tokio::time::sleep(wait).await;
        run_script("cleanup", "cleanup-store", &api_url).await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn daily_wait_is_never_more_than_24h() {
        for h in 0..24 {
            let wait = duration_until_daily(NaiveTime::from_hms_opt(h, 0, 0).unwrap());
            assert!(wait <= std::time::Duration::from_secs(24 * 3600));
        }
    }

    #[test]
    fn weekly_wait_is_never_more_than_7_days() {
        for wd in [
            Weekday::Mon,
            Weekday::Tue,
            Weekday::Wed,
            Weekday::Thu,
            Weekday::Fri,
            Weekday::Sat,
            Weekday::Sun,
        ] {
            let wait = duration_until_weekly(wd, NaiveTime::from_hms_opt(3, 0, 0).unwrap());
            assert!(wait <= std::time::Duration::from_secs(7 * 24 * 3600));
        }
    }
}
