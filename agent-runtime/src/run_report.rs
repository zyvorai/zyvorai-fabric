//! A run report: what a session did, what the host checked, what it did not, and what can be undone
//! (docs/merion/design/run-report.md).
//!
//! The report is built only from records the host already holds: approvals, the journal and receipts. It
//! never reads agent output, so nothing in it is an agent's claim presented as a fact. A line is "checked"
//! only if it is one of the measured checks below; everything else is listed as not checked.

use crate::audit::{AuditEntry, AuditPhase};
use crate::model::{ApprovalKind, ApprovalRecord, ApprovalStatus, SessionRecord};
use crate::receipts::Receipt;
use chrono::Utc;
use serde_json::{json, Value};

/// The start of the comment an automatic approval carries (see `app.rs`).
pub(crate) const AUTO_APPROVED_PREFIX: &str = "auto-approved by policy";

/// Always listed: the host did not observe these, whatever happened.
const NOT_OBSERVED: [&str; 3] = [
    "Whether the content of the changed files is correct: the host does not run tests or read the files.",
    "Anything the agent said about its own work: it is not a host record and is not counted as a check.",
    "Whether an outside service acted on a request it accepted (an accepted email is not a read email).",
];

/// Everything the report is built from. Plain data, so the builder is a pure function.
pub struct Inputs<'a> {
    pub session: &'a SessionRecord,
    pub approvals: &'a [ApprovalRecord],
    /// This session's journal rows, oldest first.
    pub audit: &'a [AuditEntry],
    pub receipts: &'a [Receipt],
}

fn check(name: &str, passed: bool, source: &str, reference: &Value) -> Value {
    json!({ "check": name, "result": if passed { "passed" } else { "failed" }, "source": source, "ref": reference })
}

fn count(v: &Value) -> usize {
    v.as_array().map_or(0, Vec::len)
}

/// The journal row that settled this changeset approval, if any: `changeset.apply` or `changeset.reject`.
fn settled<'a>(audit: &'a [AuditEntry], approval: &ApprovalRecord) -> Option<&'a AuditEntry> {
    audit.iter().rev().find(|e| {
        (e.action == "changeset.apply" || e.action == "changeset.reject")
            && e.detail.get("approval_id").and_then(Value::as_str) == Some(&approval.id.to_string())
    })
}

pub fn build(i: &Inputs<'_>) -> Value {
    let mut did: Vec<Value> = Vec::new();
    let mut checked: Vec<Value> = Vec::new();
    let mut rollback: Vec<Value> = Vec::new();
    let mut remaining: Vec<Value> = Vec::new();

    for a in i.approvals.iter().filter(|a| a.kind == ApprovalKind::Changeset) {
        let plan = a.planned_action.clone().unwrap_or(Value::Null);
        let cs_id = plan.get("changeset_id").cloned().unwrap_or(Value::Null);
        let command = plan.get("command").and_then(Value::as_str).unwrap_or("");
        let changes = plan.get("changes").cloned().unwrap_or(Value::Null);
        let n_files = ["added", "modified", "deleted"]
            .iter()
            .map(|k| count(&changes[*k]))
            .sum::<usize>();
        let by_policy = a.comment.as_deref().is_some_and(|c| c.starts_with(AUTO_APPROVED_PREFIX));
        let row = settled(i.audit, a);
        let reference = json!({ "approval_id": a.id, "changeset_id": cs_id });
        let at = a.decided_at.unwrap_or(a.created_at);
        match a.status {
            ApprovalStatus::Pending => {
                remaining.push(json!({
                    "what": format!("{n_files} file change(s) from `{command}` are waiting for a decision"),
                    "ref": reference,
                }));
            }
            ApprovalStatus::Approved => {
                let applied = row.is_some_and(|e| e.action == "changeset.apply" && e.phase == AuditPhase::Performed);
                let failed = row.is_some_and(|e| e.action == "changeset.apply" && e.phase == AuditPhase::Failed);
                let by = if by_policy { "auto-approved by a policy" } else { "approved by a person" };
                if applied {
                    did.push(json!({
                        "at": at, "type": "changeset_applied", "source": "approvals+journal",
                        "summary": format!("Applied {n_files} file change(s) from `{command}` ({by})"),
                        "ref": reference,
                    }));
                    let exit = plan.get("exit_code").and_then(Value::as_i64);
                    checked.push(check("the command exited 0", exit == Some(0), "changeset exit_code", &reference));
                    let kept = plan.get("unstaged").map_or(0, count) == 0;
                    checked.push(check("every changed file was kept", kept, "changeset unstaged", &reference));
                    let same = row
                        .and_then(|e| e.detail.get("changeset_id"))
                        .is_some_and(|id| *id == cs_id);
                    checked.push(check("the applied changeset is the one approved", same, "journal changeset.apply", &reference));
                    rollback.push(json!({
                        "item": format!("{n_files} file change(s) from `{command}`"),
                        "state": "applied_no_automatic_undo",
                        "note": "Merion has no restore for applied file changes. Review the files, or restore from a snapshot you took.",
                        "ref": reference,
                    }));
                } else if failed {
                    let err = row.and_then(|e| e.detail.get("error")).and_then(Value::as_str).unwrap_or("unknown error");
                    did.push(json!({
                        "at": at, "type": "changeset_apply_failed", "source": "journal",
                        "summary": format!("Approved `{command}` but FluxVM could not apply it: {err}"),
                        "ref": reference,
                    }));
                    rollback.push(json!({
                        "item": format!("changes from `{command}`"), "state": "unknown",
                        "note": "The apply failed part-way or not at all; inspect the files before relying on them.",
                        "ref": reference,
                    }));
                } else {
                    remaining.push(json!({
                        "what": format!("`{command}` was approved but no apply was recorded"),
                        "ref": reference,
                    }));
                    rollback.push(json!({
                        "item": format!("changes from `{command}`"), "state": "unknown",
                        "note": "No apply was recorded, so the host cannot say whether the files changed.",
                        "ref": reference,
                    }));
                }
            }
            ApprovalStatus::Denied | ApprovalStatus::Expired => {
                let verb = if a.status == ApprovalStatus::Denied { "rejected" } else { "expired undecided" };
                let rejected = row.is_some_and(|e| e.action == "changeset.reject" && e.phase == AuditPhase::Performed);
                let unconfirmed = row.is_some_and(|e| e.phase == AuditPhase::Failed);
                did.push(json!({
                    "at": at, "type": "changeset_not_applied", "source": "approvals",
                    "summary": format!("{n_files} file change(s) from `{command}` {verb}; not applied"),
                    "ref": reference,
                }));
                if unconfirmed {
                    rollback.push(json!({
                        "item": format!("changes from `{command}`"), "state": "unknown",
                        "note": "The rejection could not be confirmed with FluxVM.", "ref": reference,
                    }));
                } else {
                    rollback.push(json!({
                        "item": format!("changes from `{command}`"), "state": "nothing_to_undo",
                        "note": if rejected { "Rejected and confirmed: nothing was applied." } else { "Never approved: nothing was applied." },
                        "ref": reference,
                    }));
                }
            }
        }
    }

    for r in i.receipts {
        let reference = json!({ "receipt_id": r.id, "approval_id": r.approval_id });
        did.push(json!({
            "at": r.at, "type": "action_performed", "source": "receipts",
            "summary": format!("{} {} answered {}", r.method, r.url, r.status),
            "ref": reference,
        }));
        checked.push(check(
            &format!("{} {} was accepted by the service", r.method, r.url),
            (200..300).contains(&r.status),
            "receipt status",
            &reference,
        ));
        rollback.push(json!({
            "item": format!("{} {}", r.method, r.url),
            "state": "cannot_be_undone",
            "note": "This request left the cell. Merion cannot take it back.",
            "ref": reference,
        }));
    }

    let calls: Vec<&AuditEntry> = i.audit.iter().filter(|e| e.action == "model.call" && e.phase != AuditPhase::Planned).collect();
    if !calls.is_empty() {
        let failed = calls.iter().filter(|e| e.phase == AuditPhase::Failed).count();
        did.push(json!({
            "at": calls[0].at, "type": "model_calls", "source": "journal",
            "summary": format!("{} model call(s), {failed} failed (no text is recorded)", calls.len()),
            "ref": Value::Null,
        }));
    }

    for a in i.approvals.iter().filter(|a| a.kind != ApprovalKind::Changeset && a.status == ApprovalStatus::Pending) {
        remaining.push(json!({ "what": format!("waiting for a decision: {}", a.prompt), "ref": { "approval_id": a.id } }));
    }
    if i.session.status == crate::model::SessionStatus::Running {
        remaining.push(json!({ "what": "the session is still running; this report is a snapshot", "ref": Value::Null }));
    }
    if !i.session.tainted_by.is_empty() {
        remaining.push(json!({
            "what": format!("the session read untrusted content ({}), so its actions needed a person", i.session.tainted_by.join(", ")),
            "ref": Value::Null,
        }));
    }
    did.sort_by(|a, b| a["at"].as_str().cmp(&b["at"].as_str()));

    json!({
        "session_id": i.session.id,
        "agent": i.session.agent,
        "status": i.session.status,
        "generated_at": Utc::now(),
        "did": did,
        "checked": checked,
        "not_checked": NOT_OBSERVED,
        "remaining": remaining,
        "rollback": rollback,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::audit::AuditEntry;
    use uuid::Uuid;

    fn session() -> SessionRecord {
        let now = Utc::now();
        SessionRecord {
            id: Uuid::new_v4(),
            agent: "a".into(),
            agent_version: "v".into(),
            sandbox_id: Uuid::new_v4(),
            status: crate::model::SessionStatus::Running,
            input: json!({}),
            created_at: now,
            updated_at: now,
            last_event_seq: 0,
            guest_event_cursor: 0,
            request_id: None,
            start_policy: crate::model::SessionStartPolicy::PreferWarm,
            start_mode: crate::model::SessionStartMode::Cold,
            startup_ms: None,
            expires_at: None,
            sandbox_released: false,
            capability_token: "cap".into(),
            error: None,
            parent_session_id: None,
            user_id: None,
            tainted_by: vec![],
            confidential: None,
            agent_paused_reason: None,
            browse: Default::default(),
        }
    }

    fn approval(sid: Uuid, status: ApprovalStatus, exit: i64, unstaged: Value, comment: Option<&str>) -> ApprovalRecord {
        ApprovalRecord {
            id: Uuid::new_v4(),
            session_id: sid,
            kind: ApprovalKind::Changeset,
            subject: None,
            planned_action: Some(json!({
                "changeset_id": Uuid::new_v4(), "command": "make", "exit_code": exit,
                "changes": { "added": ["/work/a"], "modified": [], "deleted": [] }, "unstaged": unstaged,
            })),
            prompt: "Apply?".into(),
            status,
            comment: comment.map(str::to_string),
            created_at: Utc::now(),
            decided_at: Some(Utc::now()),
            source_seq: None,
            grant_scope: None,
            preview: None,
            broker_held: false,
        }
    }

    fn row(a: &ApprovalRecord, action: &str, phase: AuditPhase, extra: Value) -> AuditEntry {
        let mut detail = json!({ "approval_id": a.id, "changeset_id": a.planned_action.as_ref().unwrap()["changeset_id"] });
        if let (Some(d), Some(e)) = (detail.as_object_mut(), extra.as_object()) {
            d.extend(e.clone());
        }
        AuditEntry {
            seq: 1, at: Utc::now(), session_id: Some(a.session_id), phase, action: action.into(),
            subject: None, detail, prev_hash: String::new(), hash: String::new(),
        }
    }

    fn report(s: &SessionRecord, approvals: &[ApprovalRecord], audit: &[AuditEntry], receipts: &[Receipt]) -> Value {
        build(&Inputs { session: s, approvals, audit, receipts })
    }

    fn by_result<'a>(r: &'a Value, name: &str) -> Option<&'a str> {
        r["checked"].as_array()?.iter().find(|c| c["check"] == name)?["result"].as_str()
    }

    #[test]
    fn an_applied_change_is_checked_only_on_measured_facts_and_has_no_automatic_undo() {
        let s = session();
        let a = approval(s.id, ApprovalStatus::Approved, 0, json!([]), None);
        let r = report(&s, &[a.clone()], &[row(&a, "changeset.apply", AuditPhase::Performed, json!({}))], &[]);
        assert_eq!(by_result(&r, "the command exited 0"), Some("passed"));
        assert_eq!(by_result(&r, "every changed file was kept"), Some("passed"));
        assert_eq!(by_result(&r, "the applied changeset is the one approved"), Some("passed"));
        assert_eq!(r["rollback"][0]["state"], "applied_no_automatic_undo");
        assert!(r["did"][0]["summary"].as_str().unwrap().contains("approved by a person"));
        assert_eq!(r["not_checked"].as_array().unwrap().len(), NOT_OBSERVED.len());
    }

    #[test]
    fn a_failing_exit_code_or_an_unkept_file_shows_as_failed_not_omitted() {
        let s = session();
        let a = approval(s.id, ApprovalStatus::Approved, 2, json!(["/work/big"]), None);
        let r = report(&s, &[a.clone()], &[row(&a, "changeset.apply", AuditPhase::Performed, json!({}))], &[]);
        assert_eq!(by_result(&r, "the command exited 0"), Some("failed"));
        assert_eq!(by_result(&r, "every changed file was kept"), Some("failed"));
    }

    #[test]
    fn an_applied_row_for_a_different_changeset_does_not_count_as_the_one_approved() {
        let s = session();
        let a = approval(s.id, ApprovalStatus::Approved, 0, json!([]), None);
        let other = row(&a, "changeset.apply", AuditPhase::Performed, json!({ "changeset_id": Uuid::new_v4() }));
        let r = report(&s, &[a], &[other], &[]);
        assert_eq!(by_result(&r, "the applied changeset is the one approved"), Some("failed"));
    }

    #[test]
    fn a_policy_approval_is_labelled_as_one() {
        let s = session();
        let a = approval(s.id, ApprovalStatus::Approved, 0, json!([]), Some("auto-approved by policy docs v3"));
        let r = report(&s, std::slice::from_ref(&a), &[row(&a, "changeset.apply", AuditPhase::Performed, json!({}))], &[]);
        assert!(r["did"][0]["summary"].as_str().unwrap().contains("auto-approved by a policy"));
    }

    #[test]
    fn a_rejected_change_has_nothing_to_undo_unless_the_rejection_failed() {
        let s = session();
        let a = approval(s.id, ApprovalStatus::Denied, 0, json!([]), None);
        let ok = report(&s, &[a.clone()], &[row(&a, "changeset.reject", AuditPhase::Performed, json!({}))], &[]);
        assert_eq!(ok["rollback"][0]["state"], "nothing_to_undo");
        assert!(ok["checked"].as_array().unwrap().is_empty());
        let bad = report(&s, &[a.clone()], &[row(&a, "changeset.reject", AuditPhase::Failed, json!({ "error": "x" }))], &[]);
        assert_eq!(bad["rollback"][0]["state"], "unknown");
    }

    #[test]
    fn approved_without_a_recorded_apply_is_unknown_and_remaining() {
        let s = session();
        let a = approval(s.id, ApprovalStatus::Approved, 0, json!([]), None);
        let r = report(&s, &[a], &[], &[]);
        assert_eq!(r["rollback"][0]["state"], "unknown");
        assert!(r["remaining"][0]["what"].as_str().unwrap().contains("no apply was recorded"));
        assert!(r["checked"].as_array().unwrap().is_empty());
    }

    #[test]
    fn a_pending_change_is_remaining_and_not_in_did() {
        let s = session();
        let a = approval(s.id, ApprovalStatus::Pending, 0, json!([]), None);
        let r = report(&s, &[a], &[], &[]);
        assert!(r["did"].as_array().unwrap().is_empty());
        assert!(r["remaining"][0]["what"].as_str().unwrap().contains("waiting for a decision"));
    }

    #[test]
    fn an_outbound_action_is_checked_by_status_and_can_never_be_undone() {
        let s = session();
        let receipt = Receipt {
            id: Uuid::new_v4(), at: Utc::now(), user_id: None, session_id: s.id, agent: "mail".into(),
            credential: None, method: "POST".into(), url: "https://mail.example/send".into(), body_bytes: 3,
            body_sha256: "ab".into(), approval_id: None, idempotency_key: None, status: 502, fingerprint: String::new(),
        };
        let r = report(&s, &[], &[], &[receipt]);
        assert_eq!(by_result(&r, "POST https://mail.example/send was accepted by the service"), Some("failed"));
        assert_eq!(r["rollback"][0]["state"], "cannot_be_undone");
    }

    #[test]
    fn an_empty_run_reports_nothing_done_and_still_says_what_was_not_observed() {
        let mut s = session();
        s.status = crate::model::SessionStatus::Completed;
        let r = report(&s, &[], &[], &[]);
        assert!(r["did"].as_array().unwrap().is_empty());
        assert!(r["remaining"].as_array().unwrap().is_empty());
        assert!(!r["not_checked"].as_array().unwrap().is_empty());
    }

    #[test]
    fn the_report_carries_no_file_contents_or_bodies() {
        let s = session();
        let a = approval(s.id, ApprovalStatus::Approved, 0, json!([]), None);
        let r = report(&s, &[a.clone()], &[row(&a, "changeset.apply", AuditPhase::Performed, json!({}))], &[]).to_string();
        for forbidden in ["stdout", "stderr", "body_sha256", "\"staged\""] {
            assert!(!r.contains(forbidden), "{forbidden} leaked into the report");
        }
    }
}
