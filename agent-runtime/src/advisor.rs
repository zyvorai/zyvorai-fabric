// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: Apache-2.0

//! Policy advisor: turn an agent's denied egress into draft allow rules.
//!
//! A deny-mode agent that asks for a host outside its allowlist leaves a `Denied` row in the
//! journal. [`suggest`] groups those rows by host and proposes the narrowest allow entry that
//! would have let the calls through, with `ask: always`. Every proposal is linted with
//! [`crate::policy_lint`] first, so a suggestion that would widen access dangerously says so.
//!
//! Nothing here changes policy. The agent chooses the hosts it asks for, so the output is a
//! draft for a person to read, sign and load with `PUT …/policy`, which lints it again.

use crate::audit::{AuditEntry, AuditPhase};
use crate::policy::{KeepAllow, KeepPolicy};
use crate::policy_lint::{self, Finding};
use chrono::{DateTime, Utc};
use serde::Serialize;
use std::collections::{BTreeMap, BTreeSet, HashSet};
use uuid::Uuid;

/// At most this many suggestions are returned, most-denied first.
pub const MAX_SUGGESTIONS: usize = 50;
const MAX_PATH_CHARS: usize = 200;

#[derive(Debug, Clone, Serialize)]
pub struct Suggestion {
    pub host: String,
    pub methods: Vec<String>,
    pub denials: u64,
    pub sessions: usize,
    pub last_seen: DateTime<Utc>,
    /// A path from the most recent denial, for context only. It is agent-supplied text.
    pub example_path: String,
    /// The entry to add under `allow:`.
    pub allow: KeepAllow,
    /// What adding it would report, per [`policy_lint::review_change`].
    pub findings: Vec<Finding>,
    /// True when a High finding means `PUT …/policy` would demand an acknowledgement.
    pub needs_ack: bool,
}

struct Acc {
    methods: BTreeSet<String>,
    denials: u64,
    sessions: HashSet<Uuid>,
    last_seen: DateTime<Utc>,
    example_path: String,
}

/// A host the agent asked for is text the agent wrote. Keep only plain host or IP forms, so a
/// suggestion cannot smuggle YAML, control characters or terminal escapes to the reviewer.
fn plausible_host(h: &str) -> bool {
    !h.is_empty()
        && h.len() <= 253
        && h.chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_' | ':' | '[' | ']'))
}

fn plausible_method(m: &str) -> bool {
    !m.is_empty() && m.len() <= 10 && m.chars().all(|c| c.is_ascii_alphabetic())
}

fn clean_path(p: &str) -> String {
    p.chars()
        .filter(|c| c.is_ascii_graphic())
        .take(MAX_PATH_CHARS)
        .collect()
}

fn norm(h: &str) -> String {
    h.trim_end_matches('.').to_ascii_lowercase()
}

/// Draft allow rules from the denied egress of `sessions` (one agent's sessions), skipping
/// hosts `current` already allows.
pub fn suggest(
    entries: &[AuditEntry],
    sessions: &HashSet<Uuid>,
    current: &KeepPolicy,
) -> Vec<Suggestion> {
    let allowed: HashSet<String> = current.allow.iter().map(|a| norm(&a.host)).collect();
    let mut by_host: BTreeMap<String, Acc> = BTreeMap::new();

    for e in entries {
        let Some(sid) = e.session_id.filter(|id| sessions.contains(id)) else {
            continue;
        };
        if e.phase != AuditPhase::Denied || e.action != "egress.http" {
            continue;
        }
        let reason = e
            .detail
            .get("reason")
            .and_then(|r| r.as_str())
            .unwrap_or("");
        if !reason.contains(crate::egress::NOT_ALLOWLISTED) {
            continue; // an operator refusal, a reviewer's verdict, or an error: not a missing rule
        }
        let Some(host) = e.subject.as_deref().map(norm).filter(|h| plausible_host(h)) else {
            continue;
        };
        if allowed.contains(&host) {
            continue;
        }
        let acc = by_host.entry(host).or_insert_with(|| Acc {
            methods: BTreeSet::new(),
            denials: 0,
            sessions: HashSet::new(),
            last_seen: e.at,
            example_path: String::new(),
        });
        acc.denials += 1;
        acc.sessions.insert(sid);
        if let Some(m) = e
            .detail
            .get("method")
            .and_then(|m| m.as_str())
            .map(str::to_ascii_uppercase)
            .filter(|m| plausible_method(m))
        {
            acc.methods.insert(m);
        }
        if e.at >= acc.last_seen {
            acc.last_seen = e.at;
            acc.example_path =
                clean_path(e.detail.get("path").and_then(|p| p.as_str()).unwrap_or(""));
        }
    }

    let mut out: Vec<Suggestion> = by_host
        .into_iter()
        .map(|(host, acc)| {
            let allow = KeepAllow {
                host: host.clone(),
                methods: acc.methods.iter().cloned().collect(),
                ask: Some("always".into()),
                ..Default::default()
            };
            let mut proposed = current.clone();
            proposed.allow.push(allow.clone());
            let findings = policy_lint::review_change(Some(current), &proposed);
            Suggestion {
                needs_ack: policy_lint::needs_ack(&findings),
                host,
                methods: allow.methods.clone(),
                denials: acc.denials,
                sessions: acc.sessions.len(),
                last_seen: acc.last_seen,
                example_path: acc.example_path,
                allow,
                findings,
            }
        })
        .collect();
    out.sort_by(|a, b| b.denials.cmp(&a.denials).then_with(|| a.host.cmp(&b.host)));
    out.truncate(MAX_SUGGESTIONS);
    out
}

/// `current` plus every suggestion that needs no acknowledgement. Suggestions with a High
/// finding are left out; a person adds those by hand if they mean them.
pub fn candidate_policy(current: &KeepPolicy, suggestions: &[Suggestion]) -> KeepPolicy {
    let mut p = current.clone();
    p.allow.extend(
        suggestions
            .iter()
            .filter(|s| !s.needs_ack)
            .map(|s| s.allow.clone()),
    );
    p
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;
    use serde_json::json;

    const CUR: &str = "version: 1\ndefault_egress: deny\nallow:\n  - { host: api.github.com, methods: [GET], ask: first }\n";

    fn cur() -> KeepPolicy {
        KeepPolicy::from_yaml(CUR).unwrap()
    }

    fn denial(sid: Uuid, host: &str, method: &str, secs: u32) -> AuditEntry {
        AuditEntry {
            seq: secs as u64,
            at: Utc.with_ymd_and_hms(2026, 9, 30, 12, 0, secs).unwrap(),
            session_id: Some(sid),
            phase: AuditPhase::Denied,
            action: "egress.http".into(),
            subject: Some(host.into()),
            detail: json!({
                "reason": format!("host {host} {}", crate::egress::NOT_ALLOWLISTED),
                "method": method,
                "path": "/v1/things",
            }),
            prev_hash: String::new(),
            hash: String::new(),
        }
    }

    fn mine(id: Uuid) -> HashSet<Uuid> {
        HashSet::from([id])
    }

    #[test]
    fn groups_denials_by_host_and_collects_methods() {
        let s = Uuid::new_v4();
        let rows = [
            denial(s, "api.example.com", "get", 1),
            denial(s, "API.example.com.", "POST", 2),
            denial(s, "api.example.com", "GET", 3),
            denial(s, "other.example.com", "GET", 4),
        ];
        let out = suggest(&rows, &mine(s), &cur());
        assert_eq!(out.len(), 2);
        assert_eq!(out[0].host, "api.example.com");
        assert_eq!(out[0].denials, 3);
        assert_eq!(out[0].methods, vec!["GET", "POST"]);
        assert_eq!(out[0].allow.ask.as_deref(), Some("always"));
        assert_eq!(out[1].host, "other.example.com");
    }

    #[test]
    fn ignores_other_agents_hosts_already_allowed_and_non_allowlist_denials() {
        let s = Uuid::new_v4();
        let other = Uuid::new_v4();
        let mut operator_no = denial(s, "shop.example.com", "POST", 1);
        operator_no.detail["reason"] =
            json!("egress to shop.example.com was denied by an operator");
        let mut not_egress = denial(s, "x.example.com", "GET", 2);
        not_egress.action = "merion.policy.set".into();
        let mut performed = denial(s, "y.example.com", "GET", 3);
        performed.phase = AuditPhase::Performed;
        let rows = [
            denial(other, "theirs.example.com", "GET", 1),
            denial(s, "api.github.com", "GET", 2),
            operator_no,
            not_egress,
            performed,
        ];
        assert!(suggest(&rows, &mine(s), &cur()).is_empty());
    }

    #[test]
    fn drops_hosts_that_are_not_plain_hosts() {
        let s = Uuid::new_v4();
        let rows = [
            denial(s, "evil.com\n  - { host: \"*\" }", "GET", 1),
            denial(s, "a b.com", "GET", 2),
            denial(s, "", "GET", 3),
        ];
        assert!(suggest(&rows, &mine(s), &cur()).is_empty());
    }

    #[test]
    fn bad_methods_are_dropped_and_path_is_cleaned() {
        let s = Uuid::new_v4();
        let mut e = denial(s, "api.example.com", "GET;rm", 1);
        e.detail["path"] = json!("/a\u{1b}[31mb\n c");
        let out = suggest(&[e], &mine(s), &cur());
        assert!(out[0].methods.is_empty());
        assert_eq!(out[0].example_path, "/a[31mbc");
    }

    #[test]
    fn risky_hosts_are_flagged_and_left_out_of_the_candidate() {
        let s = Uuid::new_v4();
        let rows = [
            denial(s, "169.254.169.254", "GET", 1),
            denial(s, "api.example.com", "GET", 2),
        ];
        let out = suggest(&rows, &mine(s), &cur());
        let meta = out.iter().find(|x| x.host == "169.254.169.254").unwrap();
        assert!(meta.needs_ack);
        assert!(meta.findings.iter().any(|f| f.code == "metadata_host"));
        let ok = out.iter().find(|x| x.host == "api.example.com").unwrap();
        assert!(!ok.needs_ack);

        let cand = candidate_policy(&cur(), &out);
        let hosts: Vec<&str> = cand.allow.iter().map(|a| a.host.as_str()).collect();
        assert_eq!(hosts, vec!["api.github.com", "api.example.com"]);
    }

    #[test]
    fn candidate_yaml_round_trips_and_passes_the_lint() {
        let s = Uuid::new_v4();
        let out = suggest(&[denial(s, "api.example.com", "GET", 1)], &mine(s), &cur());
        let yaml = candidate_policy(&cur(), &out).to_yaml().unwrap();
        let back = KeepPolicy::from_yaml(&yaml).unwrap();
        assert!(!policy_lint::needs_ack(&policy_lint::review_change(
            Some(&cur()),
            &back
        )));
    }

    #[test]
    fn output_is_capped() {
        let s = Uuid::new_v4();
        let rows: Vec<AuditEntry> = (0..MAX_SUGGESTIONS + 10)
            .map(|i| denial(s, &format!("h{i}.example.com"), "GET", 1))
            .collect();
        assert_eq!(suggest(&rows, &mine(s), &cur()).len(), MAX_SUGGESTIONS);
    }

    #[tokio::test]
    async fn suggestions_route_is_operator_only_and_returns_a_draft() {
        use axum::{body::Body, http::Request};
        use tower::ServiceExt;

        let token = crate::fixture::text("operator-token");
        let (state, base) =
            crate::egress::ask_tests::state_and_session_cfg(|c| c.api_token = Some(token.clone()))
                .await;
        let app = crate::app::public_router(state.clone());

        let deploy = json!({
            "name": "advisor-desk",
            "bundle_base64": "ZXhwb3J0IGRlZmF1bHQgKCkgPT4ge30=",
            "manifest": {"template": "agent-node", "egress_mode": "deny",
                         "egress_allow_hosts": ["api.github.com"]}
        });
        let call = |method: &'static str, uri: &'static str, tok: String, body: Option<String>| {
            let app = app.clone();
            async move {
                let b = Request::builder()
                    .method(method)
                    .uri(uri)
                    .header("authorization", format!("Bearer {tok}"));
                let req = match body {
                    Some(v) => b
                        .header("content-type", "application/json")
                        .body(Body::from(v)),
                    None => b.body(Body::empty()),
                }
                .unwrap();
                let resp = app.oneshot(req).await.unwrap();
                let st = resp.status();
                let bytes = axum::body::to_bytes(resp.into_body(), 1 << 20)
                    .await
                    .unwrap();
                (
                    st,
                    serde_json::from_slice::<serde_json::Value>(&bytes).unwrap_or_default(),
                )
            }
        };
        let (st, _) = call(
            "POST",
            "/v1/agents",
            token.clone(),
            Some(deploy.to_string()),
        )
        .await;
        assert_eq!(st, 201);

        // A session of that agent, and one denied and one refused-by-operator call in its journal.
        let mut session = base.clone();
        session.id = Uuid::new_v4();
        session.agent = "advisor-desk".into();
        state.store.save_session(session.clone()).await.unwrap();
        for (host, reason) in [
            (
                "api.example.com",
                format!("host api.example.com {}", crate::egress::NOT_ALLOWLISTED),
            ),
            (
                "shop.example.com",
                "egress to shop.example.com was denied by an operator".into(),
            ),
        ] {
            state
                .store
                .audit
                .append(
                    Some(session.id),
                    AuditPhase::Denied,
                    "egress.http",
                    Some(host.into()),
                    json!({"reason": reason, "method": "POST", "path": "/v1/x"}),
                )
                .await
                .unwrap();
        }

        let (st, body) = call(
            "GET",
            "/v1/agents/advisor-desk/policy-suggestions",
            token.clone(),
            None,
        )
        .await;
        assert_eq!(st, 200, "{body}");
        let hosts: Vec<&str> = body["suggestions"]
            .as_array()
            .unwrap()
            .iter()
            .map(|s| s["host"].as_str().unwrap())
            .collect();
        assert_eq!(hosts, vec!["api.example.com"], "{body}");
        assert_eq!(body["suggestions"][0]["methods"], json!(["POST"]));
        let yaml = body["candidate_yaml"].as_str().unwrap();
        assert!(
            yaml.contains("api.example.com") && yaml.contains("api.github.com"),
            "{yaml}"
        );

        let (st, _) = call(
            "GET",
            "/v1/agents/nobody/policy-suggestions",
            token.clone(),
            None,
        )
        .await;
        assert_eq!(st, 404);

        // A user token cannot read it.
        let (st, v) = call(
            "POST",
            "/v1/user-tokens",
            token.clone(),
            Some(json!({"user_id": "ana", "ttl_seconds": 600}).to_string()),
        )
        .await;
        assert_eq!(st, 201, "{v}");
        let user_tok = v["token"].as_str().unwrap().to_string();
        let (st, _) = call(
            "GET",
            "/v1/agents/advisor-desk/policy-suggestions",
            user_tok,
            None,
        )
        .await;
        assert_eq!(st, 403);
    }
}
