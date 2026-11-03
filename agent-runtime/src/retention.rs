// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: Apache-2.0

//! Retention: an hourly sweep that forgets what the operator has said not to keep. Both settings are off by default, so nothing
//! disappears unless it was asked for:
//!
//! * `ZYVOR_AGENT_THREAD_RETENTION_DAYS`: conversation threads (and their messages) idle for that many days are forgotten. A thread whose
//!   session is still running is never forgotten.
//! * `ZYVOR_AGENT_EVENT_RETENTION_DAYS`: the event log of a session that ended that many days ago is deleted. The session record, its
//!   artifacts and approvals stay.
//!
//! * `ZYVOR_AGENT_RECEIPT_RETENTION_DAYS`: action receipts older than that are forgotten. A forgotten receipt no longer answers a repeat of
//!   its idempotency key, so keep this longer than any agent retries.
//! * `ZYVOR_AGENT_MEMORY_PROPOSAL_RETENTION_DAYS`: memory proposals nobody accepted or rejected for that long are dropped.
//! * `ZYVOR_AGENT_SUGGESTION_RETENTION_DAYS`: suggestions nobody decided for that long, and decided ones that long after the decision. A
//!   forgotten dismissal can be proposed again.
//! * Always, with no setting: memory entries whose **expiry** the user set have passed are deleted from disk (until now they were only
//!   hidden). Accepted entries without an expiry are never removed by the sweep.
//!
//! Each sweep that removes something is journaled with counts only (`merion.retention.sweep`), never with names or text.

use crate::{audit::AuditPhase, AppState};
use chrono::{DateTime, Duration as Days, Utc};
use serde_json::json;
use std::{sync::Arc, time::Duration};

/// How often the sweep runs.
const SWEEP_EVERY: Duration = Duration::from_secs(3600);

/// What one sweep removed.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Swept {
    pub threads: usize,
    pub messages: u64,
    pub event_logs: usize,
    pub receipts: usize,
    pub memory_expired: usize,
    pub memory_proposals: usize,
    pub suggestions_undecided: usize,
    pub suggestions_decided: usize,
}

/// One sweep as of `now`, using the configured periods. Public so a test (or an operator tool) can run it directly.
pub async fn sweep(state: &AppState, now: DateTime<Utc>) -> anyhow::Result<Swept> {
    let mut swept = Swept::default();
    if let Some(days) = state.config.thread_retention_days {
        let cutoff = now - Days::days(days as i64);
        // decide which threads are protected before the store deletes anything: those whose session has not ended
        let mut protected = std::collections::HashSet::new();
        for t in state.store.threads.list(None).await {
            if let Some(sid) = t.session_id {
                if state
                    .store
                    .get_session(sid)
                    .await
                    .is_some_and(|s| !s.status.is_terminal())
                {
                    protected.insert(t.id);
                }
            }
        }
        for t in state
            .store
            .threads
            .purge_idle(cutoff, |t| protected.contains(&t.id))
            .await?
        {
            swept.threads += 1;
            swept.messages += t.message_count;
        }
    }
    if let Some(days) = state.config.event_retention_days {
        swept.event_logs = state
            .store
            .purge_ended_session_events(now - Days::days(days as i64))
            .await?;
    }
    if let Some(days) = state.config.receipt_retention_days {
        swept.receipts = state
            .store
            .receipts
            .purge_before(now - Days::days(days as i64))
            .await?;
    }
    let (expired, stale) = state
        .store
        .memory
        .purge(
            now,
            state
                .config
                .memory_proposal_retention_days
                .map(|d| now - Days::days(d as i64)),
        )
        .await?;
    swept.memory_expired = expired;
    swept.memory_proposals = stale;
    // Runs even with no operator default: a person may have set their own retention (`PUT /v1/suggestions/settings`).
    let (undecided, decided) = state
        .store
        .suggestions
        .purge_before(now, state.config.suggestion_retention_days)
        .await?;
    swept.suggestions_undecided = undecided;
    swept.suggestions_decided = decided;
    if swept != Swept::default() {
        let _ = state
            .store
            .audit
            .append(
                None,
                AuditPhase::Performed,
                "merion.retention.sweep",
                None,
                json!({ "threads": swept.threads, "messages": swept.messages, "event_logs": swept.event_logs, "receipts": swept.receipts, "memory_expired": swept.memory_expired, "memory_proposals": swept.memory_proposals, "suggestions_undecided": swept.suggestions_undecided, "suggestions_decided": swept.suggestions_decided }),
            )
            .await;
    }
    Ok(swept)
}

pub async fn retention_loop(state: Arc<AppState>) {
    tracing::info!(
        thread_days = ?state.config.thread_retention_days,
        event_days = ?state.config.event_retention_days,
        receipt_days = ?state.config.receipt_retention_days,
        memory_proposal_days = ?state.config.memory_proposal_retention_days,
        suggestion_days = ?state.config.suggestion_retention_days,
        "retention sweep enabled"
    );
    tokio::time::sleep(Duration::from_secs(60)).await;
    loop {
        match sweep(&state, Utc::now()).await {
            Ok(s) if s != Swept::default() => tracing::info!(?s, "retention sweep removed data"),
            Ok(_) => {}
            Err(error) => tracing::warn!(%error, "retention sweep failed"),
        }
        tokio::time::sleep(SWEEP_EVERY).await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        config::parse_days, egress::ask_tests::state_and_session_cfg, model::SessionStatus,
        threads::Role,
    };

    #[test]
    fn retention_days_are_off_unless_set_and_bounded() {
        assert_eq!(parse_days(None).unwrap(), None);
        assert_eq!(parse_days(Some("0")).unwrap(), None, "0 means keep");
        assert_eq!(parse_days(Some(" 30 ")).unwrap(), Some(30));
        assert_eq!(parse_days(Some("3650")).unwrap(), Some(3650));
        for bad in ["3651", "-1", "x", "1.5", ""] {
            assert!(parse_days(Some(bad)).is_err(), "{bad}");
        }
    }

    #[tokio::test]
    async fn threads_idle_past_the_period_are_forgotten_but_not_while_their_session_runs() {
        let (state, base) = state_and_session_cfg(|c| c.thread_retention_days = Some(30)).await;
        let threads = &state.store.threads;
        let old = threads
            .get_or_create("ana", "chat", "old", Some("c-old"))
            .await
            .unwrap();
        threads
            .append(old.id, Role::User, "old talk", None, None)
            .await
            .unwrap();
        let running = threads
            .get_or_create("ana", "chat", "running", Some("c-run"))
            .await
            .unwrap();
        threads
            .append(running.id, Role::User, "still going", None, None)
            .await
            .unwrap();
        let mut live = base.clone();
        live.id = uuid::Uuid::new_v4();
        live.status = SessionStatus::Running;
        state.store.save_session(live.clone()).await.unwrap();
        threads
            .set_session(running.id, Some(live.id))
            .await
            .unwrap();

        // now: nothing is 30 days idle yet
        assert_eq!(sweep(&state, Utc::now()).await.unwrap(), Swept::default());
        assert_eq!(threads.list(None).await.len(), 2);

        // 31 days later both are idle, but the one with a running session stays
        let later = Utc::now() + Days::days(31);
        let swept = sweep(&state, later).await.unwrap();
        assert_eq!(
            swept,
            Swept {
                threads: 1,
                messages: 1,
                ..Default::default()
            }
        );
        assert!(threads.get(old.id).await.is_none());
        assert!(threads.get(running.id).await.is_some());

        // once that session ended, the thread goes on the next sweep
        state
            .store
            .update_session(live.id, |s| s.status = SessionStatus::Completed)
            .await
            .unwrap();
        assert_eq!(sweep(&state, later).await.unwrap().threads, 1);
        assert!(threads.list(None).await.is_empty());
        assert_eq!(
            sweep(&state, later).await.unwrap(),
            Swept::default(),
            "repeating removes nothing more"
        );
    }

    #[tokio::test]
    async fn the_journal_gets_counts_and_never_text() {
        let (state, _) = state_and_session_cfg(|c| c.thread_retention_days = Some(1)).await;
        let t = state
            .store
            .threads
            .get_or_create("ana", "chat", "secret title", None)
            .await
            .unwrap();
        state
            .store
            .threads
            .append(t.id, Role::User, "very private words", None, None)
            .await
            .unwrap();
        sweep(&state, Utc::now() + Days::days(2)).await.unwrap();
        let entries = state.store.audit.list(None, 100).await.unwrap();
        let text = serde_json::to_string(&entries).unwrap();
        assert!(
            text.contains("merion.retention.sweep"),
            "the sweep is journaled: {text}"
        );
        assert!(
            !text.contains("very private words") && !text.contains("secret title"),
            "no text in the journal"
        );
    }

    #[tokio::test]
    async fn event_logs_of_ended_sessions_go_after_the_period_and_running_ones_stay() {
        let (state, base) = state_and_session_cfg(|c| c.event_retention_days = Some(7)).await;
        let mut ended = base.clone();
        ended.id = uuid::Uuid::new_v4();
        ended.status = SessionStatus::Running;
        state.store.save_session(ended.clone()).await.unwrap();
        state
            .store
            .append_event(ended.id, "session.log", json!({"line": "x"}))
            .await
            .unwrap();
        state
            .store
            .update_session(ended.id, |s| s.status = SessionStatus::Completed)
            .await
            .unwrap();
        let mut running = base.clone();
        running.id = uuid::Uuid::new_v4();
        running.status = SessionStatus::Running;
        state.store.save_session(running.clone()).await.unwrap();
        state
            .store
            .append_event(running.id, "session.log", json!({"line": "y"}))
            .await
            .unwrap();

        assert_eq!(
            sweep(&state, Utc::now()).await.unwrap(),
            Swept::default(),
            "not old enough"
        );
        let later = Utc::now() + Days::days(8);
        assert_eq!(sweep(&state, later).await.unwrap().event_logs, 1);
        assert!(
            state
                .store
                .events_after(ended.id, 0)
                .await
                .unwrap()
                .is_empty(),
            "the ended session's events are gone"
        );
        assert!(
            state.store.get_session(ended.id).await.is_some(),
            "its record stays"
        );
        assert_eq!(
            state.store.events_after(running.id, 0).await.unwrap().len(),
            1,
            "a running session keeps its events"
        );
        assert_eq!(
            sweep(&state, later).await.unwrap(),
            Swept::default(),
            "safe to repeat"
        );
    }

    #[tokio::test]
    async fn with_no_period_set_nothing_is_ever_removed() {
        let (state, _) = state_and_session_cfg(|_| {}).await;
        let t = state
            .store
            .threads
            .get_or_create("ana", "chat", "t", None)
            .await
            .unwrap();
        state
            .store
            .threads
            .append(t.id, Role::User, "keep me", None, None)
            .await
            .unwrap();
        assert_eq!(
            sweep(&state, Utc::now() + Days::days(5000)).await.unwrap(),
            Swept::default()
        );
        assert!(state.store.threads.get(t.id).await.is_some());
    }

    fn receipt(user: &str, key: Option<&str>, at: DateTime<Utc>) -> crate::receipts::Receipt {
        crate::receipts::Receipt {
            id: uuid::Uuid::new_v4(),
            at,
            user_id: Some(user.into()),
            session_id: uuid::Uuid::new_v4(),
            agent: "mail".into(),
            credential: Some("mail".into()),
            method: "POST".into(),
            url: "https://mail.example/send".into(),
            body_bytes: 3,
            body_sha256: "digest".into(),
            approval_id: None,
            idempotency_key: key.map(str::to_string),
            status: 200,
            fingerprint: "fp".into(),
        }
    }

    #[tokio::test]
    async fn old_receipts_are_forgotten_and_their_keys_can_be_used_again_but_recent_ones_stay() {
        let (state, _) = state_and_session_cfg(|c| c.receipt_retention_days = Some(30)).await;
        let r = &state.store.receipts;
        let now = Utc::now();
        r.record(receipt("ana", Some("old-key"), now - Days::days(40)))
            .await
            .unwrap();
        r.record(receipt("ana", Some("new-key"), now - Days::days(5)))
            .await
            .unwrap();
        r.record(receipt("ben", None, now - Days::days(50)))
            .await
            .unwrap();
        let scope = |k: &str| crate::receipts::scope_key(Some("ana"), Some("mail"), k);
        assert!(matches!(
            r.begin(&scope("old-key"), "fp").await,
            crate::receipts::Begin::Replay(_)
        ));

        let swept = sweep(&state, now).await.unwrap();
        assert_eq!(swept.receipts, 2);
        assert_eq!(r.list(None, 10).await.len(), 1);
        assert!(
            matches!(
                r.begin(&scope("new-key"), "fp").await,
                crate::receipts::Begin::Replay(_)
            ),
            "a recent key still answers a repeat"
        );
        assert!(
            matches!(
                r.begin(&scope("old-key"), "fp").await,
                crate::receipts::Begin::Proceed(_)
            ),
            "a forgotten key is free again"
        );
        // it is gone from the file too: a restart does not bring it back
        let reopened = crate::receipts::ReceiptStore::open(state.config.state_dir.join("receipts"))
            .await
            .unwrap();
        assert_eq!(reopened.list(None, 10).await.len(), 1);
        assert_eq!(
            sweep(&state, now).await.unwrap().receipts,
            0,
            "repeating removes nothing more"
        );
    }

    #[tokio::test]
    async fn receipts_are_kept_when_no_period_is_set() {
        let (state, _) = state_and_session_cfg(|_| {}).await;
        state
            .store
            .receipts
            .record(receipt("ana", Some("k"), Utc::now() - Days::days(3000)))
            .await
            .unwrap();
        assert_eq!(sweep(&state, Utc::now()).await.unwrap().receipts, 0);
        assert_eq!(state.store.receipts.list(None, 10).await.len(), 1);
    }

    #[tokio::test]
    async fn memory_that_expired_is_deleted_and_stale_proposals_only_when_asked_but_accepted_entries_stay(
    ) {
        let (state, _) = state_and_session_cfg(|_| {}).await;
        let m = &state.store.memory;
        m.set_enabled("ana", true).await.unwrap();
        m.set_enabled("ben", true).await.unwrap();
        let src = || crate::memory::Source::default();
        let kept = m
            .add(
                "ana",
                "prefers window seats",
                "preference",
                false,
                None,
                src(),
            )
            .await
            .unwrap();
        m.add("ana", "trip on the 5th", "note", false, Some(1), src())
            .await
            .unwrap();
        m.propose("ana", "likes aisle seats", "preference", src(), false)
            .await
            .unwrap();
        m.add("ben", "ben's note", "note", false, Some(1), src())
            .await
            .unwrap();

        // no proposal period set: only the expired entries go, on both users
        let later = Utc::now() + Days::days(400);
        let swept = sweep(&state, later).await.unwrap();
        assert_eq!((swept.memory_expired, swept.memory_proposals), (2, 0));
        let ana = m.get("ana").await;
        assert_eq!(
            ana.items.len(),
            2,
            "the accepted entry and the proposal remain"
        );
        assert!(ana.items.iter().any(|i| i.id == kept.id));
        assert!(m.get("ben").await.items.is_empty());
        // the file agrees, so a restart does not bring the expired entry back
        let reopened = crate::memory::MemoryStore::open(state.config.state_dir.join("memory"))
            .await
            .unwrap();
        assert_eq!(reopened.get("ana").await.items.len(), 2);
    }

    #[tokio::test]
    async fn unreviewed_proposals_are_dropped_after_the_period_and_never_accepted_entries() {
        let (state, _) =
            state_and_session_cfg(|c| c.memory_proposal_retention_days = Some(30)).await;
        let m = &state.store.memory;
        m.set_enabled("ana", true).await.unwrap();
        let src = || crate::memory::Source::default();
        let kept = m
            .add("ana", "accepted long ago", "note", false, None, src())
            .await
            .unwrap();
        m.propose("ana", "nobody looked at this", "note", src(), false)
            .await
            .unwrap();
        assert_eq!(
            sweep(&state, Utc::now() + Days::days(10)).await.unwrap(),
            Swept::default(),
            "too new"
        );
        let swept = sweep(&state, Utc::now() + Days::days(31)).await.unwrap();
        assert_eq!((swept.memory_expired, swept.memory_proposals), (0, 1));
        let left = m.get("ana").await.items;
        assert_eq!(left.len(), 1);
        assert_eq!(left[0].id, kept.id);
        // the journal has counts, not text
        let journal =
            serde_json::to_string(&state.store.audit.list(None, 50).await.unwrap()).unwrap();
        assert!(
            journal.contains("memory_proposals")
                && !journal.contains("nobody looked")
                && !journal.contains("accepted long ago"),
            "{journal}"
        );
    }

    #[tokio::test]
    async fn old_suggestions_are_forgotten_undecided_ones_by_age_decided_ones_by_the_decision_and_a_dismissal_can_be_made_again(
    ) {
        let (state, _) = state_and_session_cfg(|c| c.suggestion_retention_days = Some(30)).await;
        let s = &state.store.suggestions;
        s.set_enabled("ana", true).await.unwrap();
        let sid = uuid::Uuid::new_v4();
        let make = |title: &str| {
            let title = title.to_string();
            async move {
                s.propose("ana", &title, "why", "echo", "echo", sid, false)
                    .await
                    .unwrap()
            }
        };
        let waiting = make("Nobody looked at this").await;
        let accepted = make("Book the dentist").await;
        let dismissed = make("Wire the money").await;
        s.decide("ana", accepted.id, Some(uuid::Uuid::new_v4()))
            .await
            .unwrap();
        s.decide("ana", dismissed.id, None).await.unwrap();

        assert_eq!(
            sweep(&state, Utc::now() + Days::days(10)).await.unwrap(),
            Swept::default(),
            "too new"
        );
        let swept = sweep(&state, Utc::now() + Days::days(31)).await.unwrap();
        assert_eq!(
            (swept.suggestions_undecided, swept.suggestions_decided),
            (1, 2)
        );
        assert!(s.get("ana").await.items.is_empty());
        assert!(!waiting.title.is_empty());
        // the file agrees, so a restart does not bring them back
        let reopened =
            crate::suggestions::SuggestionStore::open(state.config.state_dir.join("suggestions"))
                .await
                .unwrap();
        assert!(reopened.get("ana").await.items.is_empty());
        assert!(
            reopened.get("ana").await.enabled,
            "the person's switch is not a suggestion and stays"
        );
        // with the memory of the dismissal gone, the same suggestion may be made again
        assert!(s
            .propose("ana", "Wire the money", "why", "echo", "echo", sid, false)
            .await
            .is_ok());
        // the journal has counts, not text
        let journal =
            serde_json::to_string(&state.store.audit.list(None, 50).await.unwrap()).unwrap();
        assert!(
            journal.contains("suggestions_decided")
                && !journal.contains("dentist")
                && !journal.contains("Wire the money"),
            "{journal}"
        );
    }

    #[tokio::test]
    async fn suggestions_are_kept_when_no_period_is_set() {
        let (state, _) = state_and_session_cfg(|_| {}).await;
        let s = &state.store.suggestions;
        s.set_enabled("ana", true).await.unwrap();
        s.propose(
            "ana",
            "Old idea",
            "",
            "echo",
            "echo",
            uuid::Uuid::new_v4(),
            false,
        )
        .await
        .unwrap();
        assert_eq!(
            sweep(&state, Utc::now() + Days::days(4000)).await.unwrap(),
            Swept::default()
        );
        assert_eq!(s.get("ana").await.items.len(), 1);
    }

    #[tokio::test]
    async fn a_persons_own_retention_overrides_the_operators_default_and_works_with_no_default_at_all(
    ) {
        let (state, _) = state_and_session_cfg(|c| c.suggestion_retention_days = Some(30)).await;
        let s = &state.store.suggestions;
        s.set_enabled("ana", true).await.unwrap();
        s.set_enabled("ben", true).await.unwrap();
        s.set_retention_days("ana", Some(5)).await.unwrap();
        for user in ["ana", "ben"] {
            s.propose(
                user,
                "Old idea",
                "",
                "echo",
                "echo",
                uuid::Uuid::new_v4(),
                false,
            )
            .await
            .unwrap();
        }
        // 10 days on: ana asked for 5, so hers is gone; ben still has the operator's 30
        let swept = sweep(&state, Utc::now() + Days::days(10)).await.unwrap();
        assert_eq!(swept.suggestions_undecided, 1);
        assert!(s.get("ana").await.items.is_empty());
        assert_eq!(s.get("ben").await.items.len(), 1);

        // with no operator default at all, ana's own 5 days still applies to a fresh one; a person with no setting keeps everything
        let (state2, _) = state_and_session_cfg(|_| {}).await;
        let s2 = &state2.store.suggestions;
        s2.set_enabled("ana", true).await.unwrap();
        s2.set_enabled("ben", true).await.unwrap();
        s2.set_retention_days("ana", Some(5)).await.unwrap();
        for user in ["ana", "ben"] {
            s2.propose(
                user,
                "Old idea",
                "",
                "echo",
                "echo",
                uuid::Uuid::new_v4(),
                false,
            )
            .await
            .unwrap();
        }
        let swept = sweep(&state2, Utc::now() + Days::days(10)).await.unwrap();
        assert_eq!(
            swept.suggestions_undecided, 1,
            "ana's own 5 days still applies with no operator default; ben's has neither, so it stays"
        );
        assert!(s2.get("ana").await.items.is_empty());
        assert_eq!(s2.get("ben").await.items.len(), 1);
    }

    #[tokio::test]
    async fn a_decided_suggestion_is_judged_by_when_it_was_decided_not_when_it_was_made() {
        let (state, _) = state_and_session_cfg(|_| {}).await;
        let s = &state.store.suggestions;
        s.set_enabled("ana", true).await.unwrap();
        let made = s
            .propose(
                "ana",
                "Made long ago, decided just now",
                "",
                "echo",
                "echo",
                uuid::Uuid::new_v4(),
                false,
            )
            .await
            .unwrap();
        tokio::time::sleep(std::time::Duration::from_millis(30)).await;
        let cutoff = Utc::now();
        tokio::time::sleep(std::time::Duration::from_millis(30)).await;
        s.decide("ana", made.id, None).await.unwrap();
        assert_eq!(
            s.purge_before(cutoff, Some(0)).await.unwrap(),
            (0, 0),
            "decided after the cutoff, so it stays whatever its age"
        );
        assert_eq!(s.get("ana").await.items.len(), 1);
        assert_eq!(
            s.purge_before(Utc::now() + Days::days(1), Some(0))
                .await
                .unwrap(),
            (0, 1)
        );
    }
}
