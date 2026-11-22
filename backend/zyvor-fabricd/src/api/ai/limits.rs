// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: LicenseRef-Zyvor-Production-2.0

//! Store-backed request and token windows.
//!
//! The decision itself holds no process-local counter. Callers persist the
//! returned record with an exclusive file lock.

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
pub struct RateCounter {
    pub id: String,
    #[serde(default)]
    pub window_started_unix: i64,
    #[serde(default)]
    pub requests: u64,
    #[serde(default)]
    pub tokens: u64,
    /// Unix time when the daily token bucket started. `0` means unused.
    #[serde(default)]
    pub day_started_unix: i64,
    #[serde(default)]
    pub day_tokens: u64,
    /// In-flight calls. The minute window does not clear this.
    #[serde(default)]
    pub inflight: u32,
}

pub fn admit(
    mut counter: RateCounter,
    now_unix: i64,
    requested_tokens: u64,
    requests_per_minute: u64,
    tokens_per_minute: u64,
    streams: u32,
) -> Result<RateCounter, String> {
    if (requests_per_minute > 0 || tokens_per_minute > 0)
        && now_unix.saturating_sub(counter.window_started_unix) >= 60
    {
        counter.window_started_unix = now_unix;
        counter.requests = 0;
        counter.tokens = 0;
    }
    if requests_per_minute > 0 && counter.requests.saturating_add(1) > requests_per_minute {
        return Err(format!(
            "rate limit exceeded for {} ({} requests/minute)",
            counter.id, requests_per_minute
        ));
    }
    let tokens = requested_tokens.max(1);
    if tokens_per_minute > 0 && counter.tokens.saturating_add(tokens) > tokens_per_minute {
        return Err(format!(
            "token limit exceeded for {} ({} tokens/minute)",
            counter.id, tokens_per_minute
        ));
    }
    if streams > 0 && counter.inflight.saturating_add(1) > streams {
        return Err(format!(
            "stream limit exceeded for {} ({streams} in flight)",
            counter.id
        ));
    }
    if requests_per_minute > 0 || tokens_per_minute > 0 {
        counter.requests = counter.requests.saturating_add(1);
        counter.tokens = counter.tokens.saturating_add(tokens);
    }
    if streams > 0 {
        counter.inflight = counter.inflight.saturating_add(1);
    }
    Ok(counter)
}

pub fn release_inflight(mut counter: RateCounter) -> RateCounter {
    counter.inflight = counter.inflight.saturating_sub(1);
    counter
}

const DAY_SECS: i64 = 86_400;

/// A separate 24-hour token bucket. A zero allowance does not change the counter.
pub fn admit_day(
    mut counter: RateCounter,
    now_unix: i64,
    requested_tokens: u64,
    daily_tokens: u64,
) -> Result<RateCounter, String> {
    if daily_tokens == 0 {
        return Ok(counter);
    }
    if counter.day_started_unix == 0
        || now_unix.saturating_sub(counter.day_started_unix) >= DAY_SECS
    {
        counter.day_started_unix = now_unix;
        counter.day_tokens = 0;
    }
    let tokens = requested_tokens.max(1);
    if counter.day_tokens.saturating_add(tokens) > daily_tokens {
        return Err(format!(
            "daily token allowance exceeded for {} ({daily_tokens} tokens)",
            counter.id
        ));
    }
    counter.day_tokens = counter.day_tokens.saturating_add(tokens);
    Ok(counter)
}

pub fn counter_id(scope: &str, name: &str) -> String {
    let name = name.replace('/', "_");
    format!("{scope}:{name}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn window_resets_and_then_refuses_the_next_request() {
        let counter = RateCounter {
            id: "endpoint:qwen".into(),
            window_started_unix: 0,
            requests: 2,
            tokens: 0,
            day_started_unix: 0,
            day_tokens: 0,
            inflight: 0,
        };
        let admitted = admit(counter, 61, 1, 2, 0, 0).unwrap();
        assert_eq!(admitted.requests, 1);
        let again = admit(admitted.clone(), 61, 1, 2, 0, 0).unwrap();
        assert!(admit(again, 61, 1, 2, 0, 0).is_err());
    }

    #[test]
    fn token_window_counts_the_request() {
        let counter = RateCounter {
            id: "model:qwen".into(),
            window_started_unix: 10,
            requests: 0,
            tokens: 8,
            day_started_unix: 0,
            day_tokens: 0,
            inflight: 0,
        };
        assert!(admit(counter, 10, 4, 0, 10, 0).is_err());
    }

    #[test]
    fn one_call_counts_once_against_requests_and_tokens() {
        let counter = RateCounter {
            id: "model:qwen".into(),
            window_started_unix: 10,
            requests: 0,
            tokens: 0,
            day_started_unix: 0,
            day_tokens: 0,
            inflight: 0,
        };
        let next = admit(counter, 10, 4, 2, 6, 0).unwrap();
        assert_eq!(next.requests, 1);
        assert_eq!(next.tokens, 4);
        assert!(admit(next, 10, 4, 2, 6, 0).is_err());
    }

    #[test]
    fn daily_bucket_resets_after_a_day_and_refuses_the_overflow() {
        let counter = RateCounter {
            id: "global:all".into(),
            window_started_unix: 0,
            requests: 0,
            tokens: 0,
            day_started_unix: 1_000,
            day_tokens: 9,
            inflight: 0,
        };
        assert!(admit_day(counter.clone(), 1_000, 2, 10).is_err());
        let next = admit_day(counter, 1_000 + 86_400, 2, 10).unwrap();
        assert_eq!(next.day_tokens, 2);
        assert_eq!(next.requests, 0);
    }

    #[test]
    fn stream_cap_survives_the_minute_window_and_releases() {
        let counter = RateCounter {
            id: "global:fabric".into(),
            window_started_unix: 10,
            requests: 4,
            tokens: 9,
            day_started_unix: 0,
            day_tokens: 0,
            inflight: 1,
        };
        assert!(admit(counter.clone(), 80, 1, 0, 0, 1).is_err());
        let released = release_inflight(counter);
        assert_eq!(released.inflight, 0);
        assert_eq!(released.requests, 4);
        let next = admit(released, 80, 1, 0, 0, 1).unwrap();
        assert_eq!(next.inflight, 1);
        assert_eq!(next.requests, 4);
    }
}
