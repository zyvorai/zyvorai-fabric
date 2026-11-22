// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: LicenseRef-Zyvor-Production-2.0

//! Per-endpoint circuit breaker. Five consecutive failures open it for 30s.

pub const FAILURE_THRESHOLD: u32 = 5;
pub const OPEN_SECS: i64 = 30;

#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
pub struct Breaker {
    #[serde(default)]
    pub failures: u32,
    #[serde(default)]
    pub open_until_unix: i64,
}

pub fn is_open(breaker: &Breaker, now_unix: i64) -> bool {
    breaker.open_until_unix > now_unix
}

pub fn observe(breaker: Breaker, failed: bool, now_unix: i64) -> Breaker {
    if is_open(&breaker, now_unix) {
        return breaker;
    }
    if !failed {
        return Breaker::default();
    }
    let failures = breaker.failures.saturating_add(1);
    if failures >= FAILURE_THRESHOLD {
        Breaker {
            failures,
            open_until_unix: now_unix.saturating_add(OPEN_SECS),
        }
    } else {
        Breaker {
            failures,
            open_until_unix: 0,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fifth_failure_opens_and_a_success_after_the_window_closes() {
        let mut breaker = Breaker::default();
        for _ in 0..4 {
            breaker = observe(breaker, true, 100);
            assert!(!is_open(&breaker, 100));
        }
        breaker = observe(breaker, true, 100);
        assert!(is_open(&breaker, 100));
        assert!(is_open(&breaker, 129));
        assert!(!is_open(&breaker, 130));
        breaker = observe(breaker, false, 130);
        assert_eq!(breaker.failures, 0);
    }
}
