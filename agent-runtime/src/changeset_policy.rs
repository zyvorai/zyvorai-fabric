// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: Apache-2.0

//! Would a pre-authorised policy have approved this changeset? (docs/merion/design/auto-approve-changesets.md)
//!
//! This is stage one: a pure evaluator plus a dry-run audit entry. Nothing here approves anything. The
//! policy matches on what FluxVM measured, never on the command text, and every doubt means "a person decides".

use serde::{Deserialize, Serialize};
use serde_json::Value;

const KINDS: [&str; 3] = ["added", "modified", "deleted"];

/// Never auto-approvable, whatever a policy says. A policy can add to this list, not remove from it.
const DENY_FLOOR: [&str; 6] = ["*/.ssh/*", "*.pem", "*.key", "*/.env*", "*/.aws/*", "*/.gnupg/*"];

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct ChangesetPolicy {
    pub id: String,
    #[serde(default)]
    pub version: u32,
    /// Every changed file must be under one of these guest directories.
    pub paths: Vec<String>,
    /// Allowed change kinds; only `added` when absent.
    #[serde(default = "default_kinds")]
    pub kinds: Vec<String>,
    #[serde(default = "default_max_files")]
    pub max_files: usize,
    /// Total size of added and modified files. When set, a file with no reported size disqualifies.
    #[serde(default)]
    pub max_bytes: Option<u64>,
    #[serde(default)]
    pub deny_globs: Vec<String>,
    #[serde(default)]
    pub require_exit_code: i32,
}

/// A policy an operator set for one user's agent (stage two). Stored in `changeset_policies.json`;
/// `policy.version` is assigned by the store (previous + 1), never taken from the request.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoredPolicy {
    pub user_id: String,
    pub agent: String,
    pub policy: ChangesetPolicy,
    pub updated_at: chrono::DateTime<chrono::Utc>,
}

/// At most this many auto-approvals per session per hour; past it a person decides.
pub const MAX_AUTO_PER_HOUR: usize = 30;

fn default_kinds() -> Vec<String> {
    vec!["added".to_string()]
}
fn default_max_files() -> usize {
    20
}

/// What the evaluator needs from a changeset and its session.
pub struct Subject<'a> {
    pub exit_code: i32,
    pub changes: &'a Value,
    pub side_effects: &'a Value,
    pub staged: &'a Value,
    pub unstaged: &'a [String],
    pub tainted: bool,
    /// Set by the caller only when the cell's network is confined to Merion's own egress broker and the agent
    /// is set to deny egress: the destinations FluxVM may list for that cell (`<gateway>/32 tcp/<port>`).
    /// With `None`, any egress that is not `blocked` disqualifies.
    pub broker_destinations: Option<&'a [String]>,
}

impl ChangesetPolicy {
    pub fn parse(text: &str) -> Result<Self, String> {
        let p: Self = serde_json::from_str(text).map_err(|e| format!("policy is not valid: {e}"))?;
        if p.id.trim().is_empty() {
            return Err("policy needs an id".into());
        }
        if p.paths.is_empty() || p.paths.iter().any(|d| !d.starts_with('/') || d.contains("..")) {
            return Err("paths must be absolute guest directories without `..`".into());
        }
        if p.kinds.is_empty() || p.kinds.iter().any(|k| !KINDS.contains(&k.as_str())) {
            return Err("kinds may only list added, modified, deleted".into());
        }
        Ok(p)
    }
}

/// `Ok(files)` when the policy would approve; otherwise the first reason a person has to decide.
pub fn eligible(policy: &ChangesetPolicy, s: &Subject) -> Result<usize, String> {
    if s.tainted {
        return Err("session is tainted".into());
    }
    if !s.unstaged.is_empty() {
        return Err("changes that could not be staged".into());
    }
    if s.exit_code != policy.require_exit_code {
        return Err(format!("exit code {} is not {}", s.exit_code, policy.require_exit_code));
    }
    let se = s.side_effects.as_object().ok_or("side effects not reported")?;
    // Egress must be reported as blocked (or absent); anything else sent something out.
    let non_replayable = se.get("non_replayable").and_then(Value::as_array).ok_or("non-replayable side effects")?;
    match se.get("egress").and_then(Value::as_str) {
        Some("blocked") | None => {
            if !non_replayable.is_empty() {
                return Err("non-replayable side effects".into());
            }
        }
        // The cell can only reach Merion's own broker, which the agent's `deny` mode refuses: FluxVM lists the
        // broker as its one allowed destination and says it could not replay traffic there. Any other
        // destination, or any other non-replayable effect, is still a person's decision.
        Some("allow_listed") if s.broker_destinations.is_some() => {
            let broker = s.broker_destinations.unwrap_or_default();
            let listed = se.get("destinations").and_then(Value::as_array).ok_or("destinations not reported")?;
            let mut expected = Vec::new();
            for d in listed {
                let d = d.as_str().ok_or("a destination is not text")?;
                if !broker.iter().any(|b| b == d) {
                    return Err(format!("egress is allowed to {d}, which is not Merion's broker"));
                }
                expected.push(format!("possible traffic to allow-listed destination {d}: recorded, never replayed"));
            }
            if listed.is_empty() {
                return Err("egress is allow-listed with no destination".into());
            }
            for n in non_replayable {
                let n = n.as_str().ok_or("a side effect is not text")?;
                if !expected.iter().any(|e| e == n) {
                    return Err("non-replayable side effects".into());
                }
            }
        }
        Some(other) => return Err(format!("egress is {other}")),
    }
    let changes = s.changes.as_object().ok_or("changes not reported")?;
    if let Some(unknown) = changes.keys().find(|k| !KINDS.contains(&k.as_str()) && *k != "unchanged") {
        return Err(format!("unknown change kind {unknown}"));
    }
    let (mut files, mut bytes) = (0usize, 0u64);
    for kind in KINDS {
        let list = changes.get(kind).and_then(Value::as_array).ok_or_else(|| format!("{kind} not reported"))?;
        if list.is_empty() {
            continue;
        }
        if !policy.kinds.iter().any(|k| k == kind) {
            return Err(format!("{kind} is not allowed"));
        }
        for item in list {
            let path = item.as_str().ok_or("a changed path is not text")?;
            if path.contains("..") || !under(path, &policy.paths) {
                return Err(format!("{path} is outside the allowed paths"));
            }
            if DENY_FLOOR.iter().copied().chain(policy.deny_globs.iter().map(String::as_str)).any(|g| glob(g, path))
                || path.rsplit('/').next().is_some_and(|b| b.starts_with('.'))
            {
                return Err(format!("{path} is on the never-auto-approve list"));
            }
            if kind != "deleted" {
                if let Some(max) = policy.max_bytes {
                    let size = s.staged.get(path).and_then(|f| f.get("size")).and_then(Value::as_u64);
                    bytes += size.ok_or_else(|| format!("no size reported for {path}"))?;
                    if bytes > max {
                        return Err(format!("{bytes} bytes is over {max}"));
                    }
                }
            }
            files += 1;
        }
    }
    if files == 0 {
        return Err("no changes".into());
    }
    if files > policy.max_files {
        return Err(format!("{files} files is over {}", policy.max_files));
    }
    Ok(files)
}

fn under(path: &str, dirs: &[String]) -> bool {
    dirs.iter().any(|d| {
        let d = d.trim_end_matches('/');
        path.strip_prefix(d).is_some_and(|rest| rest.starts_with('/'))
    })
}

/// `*` matches any run of characters, `/` included.
fn glob(pattern: &str, text: &str) -> bool {
    let parts: Vec<&str> = pattern.split('*').collect();
    let (first, last) = (parts[0], parts[parts.len() - 1]);
    if parts.len() == 1 {
        return pattern == text;
    }
    if !text.starts_with(first) || !text.ends_with(last) || text.len() < first.len() + last.len() {
        return false;
    }
    let mut rest = &text[first.len()..text.len() - last.len()];
    for mid in &parts[1..parts.len() - 1] {
        match rest.find(mid) {
            Some(i) => rest = &rest[i + mid.len()..],
            None => return false,
        }
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn policy() -> ChangesetPolicy {
        ChangesetPolicy::parse(r#"{"id":"notes","paths":["/work"],"max_bytes":100}"#).unwrap()
    }
    fn ok_changes() -> Value {
        json!({"added":["/work/a.txt"],"modified":[],"deleted":[],"unchanged":3})
    }
    fn se() -> Value {
        json!({"egress":"blocked","non_replayable":[],"replayable":[]})
    }
    fn staged() -> Value {
        json!({"/work/a.txt":{"size":6}})
    }
    fn check(p: &ChangesetPolicy, changes: &Value, side: &Value, staged: &Value, exit: i32, unstaged: &[String], tainted: bool) -> Result<usize, String> {
        eligible(p, &Subject { exit_code: exit, changes, side_effects: side, staged, unstaged, tainted, broker_destinations: None })
    }
    fn check_broker(side: &Value, broker: &[String]) -> Result<usize, String> {
        eligible(
            &policy(),
            &Subject { exit_code: 0, changes: &ok_changes(), side_effects: side, staged: &staged(), unstaged: &[], tainted: false, broker_destinations: Some(broker) },
        )
    }

    #[test]
    fn a_small_added_file_under_an_allowed_path_is_eligible() {
        assert_eq!(check(&policy(), &ok_changes(), &se(), &staged(), 0, &[], false), Ok(1));
    }

    const BROKER: &str = "10.0.0.1/32 tcp/9097";
    fn listed(dests: &[&str], notes: &[&str]) -> Value {
        json!({"egress":"allow_listed","destinations":dests,"non_replayable":notes,"replayable":[]})
    }
    fn note(d: &str) -> String {
        format!("possible traffic to allow-listed destination {d}: recorded, never replayed")
    }

    #[test]
    fn a_cell_confined_to_the_broker_is_eligible_only_when_the_caller_vouches_for_it() {
        let side = listed(&[BROKER], &[&note(BROKER)]);
        assert_eq!(check_broker(&side, &[BROKER.to_string()]), Ok(1));
        // Without the caller's say-so, allow_listed is a person's decision as before.
        assert!(check(&policy(), &ok_changes(), &side, &staged(), 0, &[], false).unwrap_err().contains("egress is allow_listed"));
    }

    #[test]
    fn any_other_destination_or_effect_still_needs_a_person() {
        let broker = [BROKER.to_string()];
        let other = "10.9.9.9/32 tcp/443";
        let e = check_broker(&listed(&[other], &[&note(other)]), &broker).unwrap_err();
        assert!(e.contains("not Merion's broker"), "{e}");
        assert!(check_broker(&listed(&[BROKER, other], &[]), &broker).is_err());
        assert!(check_broker(&listed(&[], &[]), &broker).unwrap_err().contains("no destination"));
        assert!(check_broker(&listed(&[BROKER], &[&note(BROKER), "sent mail"]), &broker).unwrap_err().contains("non-replayable"));
        assert!(check_broker(&json!({"egress":"unrestricted","non_replayable":[]}), &broker).unwrap_err().contains("unrestricted"));
        assert!(check_broker(&json!({"egress":"allow_listed","non_replayable":[]}), &broker).is_err());
    }

    #[test]
    fn each_disqualifier_leaves_it_to_a_person() {
        let p = policy();
        let c = ok_changes();
        let bad = |r: Result<usize, String>, needle: &str| {
            let e = r.unwrap_err();
            assert!(e.contains(needle), "{e} should mention {needle}");
        };
        bad(check(&p, &c, &se(), &staged(), 0, &[], true), "tainted");
        bad(check(&p, &c, &se(), &staged(), 0, &["/work/x".into()], false), "staged");
        bad(check(&p, &c, &se(), &staged(), 1, &[], false), "exit code");
        bad(check(&p, &c, &json!({"egress":"allowed","non_replayable":[]}), &staged(), 0, &[], false), "egress");
        bad(check(&p, &c, &json!({"egress":"blocked","non_replayable":["sent mail"]}), &staged(), 0, &[], false), "non-replayable");
        bad(check(&p, &c, &json!({"egress":"blocked"}), &staged(), 0, &[], false), "non-replayable");
        bad(check(&p, &json!({"added":[],"modified":["/work/a.txt"],"deleted":[]}), &se(), &staged(), 0, &[], false), "modified is not allowed");
        bad(check(&p, &json!({"added":[],"modified":[],"deleted":["/work/a.txt"]}), &se(), &staged(), 0, &[], false), "deleted is not allowed");
        bad(check(&p, &json!({"added":["/etc/passwd"],"modified":[],"deleted":[]}), &se(), &staged(), 0, &[], false), "outside");
        bad(check(&p, &json!({"added":["/work/../etc/x"],"modified":[],"deleted":[]}), &se(), &staged(), 0, &[], false), "outside");
        bad(check(&p, &json!({"added":["/workshop/a"],"modified":[],"deleted":[]}), &se(), &staged(), 0, &[], false), "outside");
        bad(check(&p, &c, &se(), &json!({"/work/a.txt":{"size":101}}), 0, &[], false), "over");
        bad(check(&p, &c, &se(), &json!({}), 0, &[], false), "no size");
        bad(check(&p, &json!({"added":[],"modified":[],"deleted":[]}), &se(), &staged(), 0, &[], false), "no changes");
    }

    #[test]
    fn the_deny_floor_cannot_be_removed_and_dotfiles_never_pass() {
        let p = policy();
        for path in ["/work/.ssh/id_rsa", "/work/server.pem", "/work/.env", "/work/deploy.key", "/work/.hidden"] {
            let c = json!({"added":[path],"modified":[],"deleted":[]});
            let e = check(&p, &c, &se(), &json!({path:{"size":1}}), 0, &[], false).unwrap_err();
            assert!(e.contains("never-auto-approve"), "{path}: {e}");
        }
        let extra = ChangesetPolicy::parse(r#"{"id":"n","paths":["/work"],"deny_globs":["*.secret"]}"#).unwrap();
        let c = json!({"added":["/work/a.secret"],"modified":[],"deleted":[]});
        assert!(check(&extra, &c, &se(), &json!({}), 0, &[], false).is_err());
    }

    #[test]
    fn missing_or_odd_shapes_fail_closed() {
        let p = policy();
        assert!(check(&p, &Value::Null, &se(), &staged(), 0, &[], false).is_err());
        assert!(check(&p, &ok_changes(), &Value::Null, &staged(), 0, &[], false).is_err());
        assert!(check(&p, &json!({"added":["/work/a"],"modified":[],"deleted":[],"renamed":["x"]}), &se(), &staged(), 0, &[], false).is_err());
        assert!(check(&p, &json!({"added":["/work/a"]}), &se(), &staged(), 0, &[], false).is_err());
        assert!(check(&p, &json!({"added":[7],"modified":[],"deleted":[]}), &se(), &staged(), 0, &[], false).is_err());
    }

    #[test]
    fn too_many_files_is_refused() {
        let p = ChangesetPolicy::parse(r#"{"id":"n","paths":["/work"],"max_files":1}"#).unwrap();
        let c = json!({"added":["/work/a","/work/b"],"modified":[],"deleted":[]});
        assert!(check(&p, &c, &se(), &json!({}), 0, &[], false).unwrap_err().contains("over 1"));
    }

    #[test]
    fn bad_policies_are_rejected() {
        for text in [
            "{}",
            r#"{"id":"","paths":["/work"]}"#,
            r#"{"id":"a","paths":[]}"#,
            r#"{"id":"a","paths":["work"]}"#,
            r#"{"id":"a","paths":["/work/.."]}"#,
            r#"{"id":"a","paths":["/work"],"kinds":["renamed"]}"#,
            r#"{"id":"a","paths":["/work"],"surprise":1}"#,
        ] {
            assert!(ChangesetPolicy::parse(text).is_err(), "{text}");
        }
    }
}
