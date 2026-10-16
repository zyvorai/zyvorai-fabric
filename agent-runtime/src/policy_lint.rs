// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: Apache-2.0

//! Risk lint for Keep policy changes.
//!
//! [`review_change`] compares the policy an agent runs now with the one a PUT
//! wants to load, and reports what the change *adds*: a wider default, new
//! hosts, private or metadata addresses, wildcards, extra methods, weaker
//! approval. It is a syntactic check on the policy, not a proof about what a
//! cell can reach, and it does not replace the signature check.
//!
//! A [`Severity::High`] finding makes `PUT …/policy` refuse the change unless
//! the caller acknowledges it (see `put_agent_policy`).

use crate::policy::MerionPolicy;
use serde::Serialize;
use std::collections::BTreeSet;
use std::net::IpAddr;

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "lowercase")]
pub enum Severity {
    Info,
    Medium,
    High,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct Finding {
    pub severity: Severity,
    /// Stable machine-readable id, e.g. `default_egress_allow`.
    pub code: &'static str,
    pub message: String,
}

impl Finding {
    fn new(severity: Severity, code: &'static str, message: impl Into<String>) -> Self {
        Self {
            severity,
            code,
            message: message.into(),
        }
    }
}

/// Hosts that expose cloud instance credentials.
const METADATA_HOSTS: &[&str] = &[
    "169.254.169.254",
    "169.254.170.2",
    "100.100.100.200",
    "fd00:ec2::254",
    "metadata.google.internal",
    "metadata",
];

const READ_METHODS: &[&str] = &["GET", "HEAD", "OPTIONS"];

fn norm_host(h: &str) -> String {
    h.trim().trim_end_matches('.').to_ascii_lowercase()
}

fn is_metadata(host: &str) -> bool {
    let bare = host.trim_start_matches('[').trim_end_matches(']');
    METADATA_HOSTS.contains(&bare)
}

fn is_internal_ip(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => {
            v4.is_private()
                || v4.is_loopback()
                || v4.is_link_local()
                || v4.is_unspecified()
                // 100.64.0.0/10 shared address space
                || (v4.octets()[0] == 100 && (v4.octets()[1] & 0xc0) == 64)
        }
        IpAddr::V6(v6) => {
            v6.is_loopback()
                || v6.is_unspecified()
                || v6.is_unique_local()
                || v6.is_unicast_link_local()
        }
    }
}

fn is_internal_name(host: &str) -> bool {
    host == "localhost"
        || host.ends_with(".localhost")
        || host.ends_with(".internal")
        || host.ends_with(".local")
        || host.ends_with(".lan")
        || !host.contains('.') && !host.contains(':')
}

fn is_wildcard(host: &str) -> bool {
    host.contains('*')
}

fn methods_of(methods: &[String]) -> BTreeSet<String> {
    methods
        .iter()
        .map(|m| m.trim().to_ascii_uppercase())
        .collect()
}

fn rank_ask(ask: Option<&str>) -> u8 {
    match ask {
        Some("always") => 3,
        Some("first") | Some("ask") => 2,
        _ => 1, // never, unset
    }
}

/// Everything about a single allowed host that is risky on its own, no matter
/// what the old policy said.
fn host_findings(host: &str, out: &mut Vec<Finding>) {
    let h = norm_host(host);
    let bare = h.trim_start_matches('[').trim_end_matches(']');
    if is_metadata(&h) {
        out.push(Finding::new(
            Severity::High,
            "metadata_host",
            format!("`{host}` is a cloud metadata address; it can hand out instance credentials"),
        ));
    } else if bare.parse::<IpAddr>().map(is_internal_ip).unwrap_or(false) {
        out.push(Finding::new(
            Severity::High,
            "private_ip_host",
            format!("`{host}` is a private, loopback or link-local address"),
        ));
    } else if is_internal_name(&h) {
        out.push(Finding::new(
            Severity::Medium,
            "internal_name_host",
            format!("`{host}` looks like an internal name, not a public host"),
        ));
    }
    if h == "*" {
        out.push(Finding::new(
            Severity::High,
            "wildcard_all_hosts",
            "`*` allows every host",
        ));
    } else if is_wildcard(&h) {
        out.push(Finding::new(
            Severity::Medium,
            "wildcard_host",
            format!("`{host}` is a wildcard; it covers hosts the operator has not listed"),
        ));
    }
}

/// Review a policy change. `old` is `None` for a first policy, in which case
/// every allowed host counts as new.
pub fn review_change(old: Option<&MerionPolicy>, new: &MerionPolicy) -> Vec<Finding> {
    let mut out = Vec::new();

    if new.default_egress == "allow" && old.is_none_or(|o| o.default_egress != "allow") {
        out.push(Finding::new(
            Severity::High,
            "default_egress_allow",
            "default_egress changes to `allow`: every host not listed under deny is reachable",
        ));
    }

    let old_allow = |host: &str| {
        old.and_then(|o| {
            o.allow
                .iter()
                .find(|a| norm_host(&a.host) == norm_host(host))
        })
    };

    for a in &new.allow {
        let prev = old_allow(&a.host);
        let cur_methods = methods_of(&a.methods);

        if prev.is_none() {
            out.push(Finding::new(
                Severity::Info,
                "new_host",
                format!("new allowed host `{}`", a.host),
            ));
            // Only new hosts get the per-host checks, so an unchanged risky
            // entry does not block every later edit.
            host_findings(&a.host, &mut out);
            if cur_methods.is_empty() {
                out.push(Finding::new(
                    Severity::Medium,
                    "new_host_any_method",
                    format!("`{}` lists no methods, so every method is allowed", a.host),
                ));
            }
        } else if let Some(p) = prev {
            let old_methods = methods_of(&p.methods);
            let widened = if old_methods.is_empty() {
                false // already any-method
            } else if cur_methods.is_empty() {
                true
            } else {
                !cur_methods.is_subset(&old_methods)
            };
            if widened {
                let added: Vec<String> = if cur_methods.is_empty() {
                    vec!["(all methods)".into()]
                } else {
                    cur_methods.difference(&old_methods).cloned().collect()
                };
                let writes = cur_methods.is_empty()
                    || added.iter().any(|m| !READ_METHODS.contains(&m.as_str()));
                out.push(Finding::new(
                    if writes {
                        Severity::High
                    } else {
                        Severity::Medium
                    },
                    "methods_widened",
                    format!("`{}` gains methods: {}", a.host, added.join(", ")),
                ));
            }
            for (what, was, now) in [
                ("JSON-RPC methods", &p.rpc_methods, &a.rpc_methods),
                ("MCP tools", &p.mcp_tools, &a.mcp_tools),
                (
                    "GraphQL operations",
                    &p.graphql_operations,
                    &a.graphql_operations,
                ),
            ] {
                // An empty list means unrestricted, so dropping the list widens access.
                let widened =
                    !was.is_empty() && (now.is_empty() || !now.iter().all(|n| was.contains(n)));
                if widened {
                    out.push(Finding::new(
                        Severity::High,
                        "body_rules_widened",
                        format!("`{}` is no longer limited to the same {what}", a.host),
                    ));
                }
            }
            // A program is covered if the old entry named the same path with no hash pinned, or
            // with the same hash. Dropping the list, or adding a program, widens who may call.
            let covered = |b: &crate::model::BinaryRule| {
                p.binaries
                    .iter()
                    .any(|o| o.path == b.path && (o.sha256.is_none() || o.sha256 == b.sha256))
            };
            if !p.binaries.is_empty() && (a.binaries.is_empty() || !a.binaries.iter().all(covered))
            {
                out.push(Finding::new(
                    Severity::High,
                    "binaries_widened",
                    format!(
                        "`{}` may now be reached by programs it was not open to before",
                        a.host
                    ),
                ));
            }
            if rank_ask(a.ask.as_deref()) < rank_ask(p.ask.as_deref()) {
                out.push(Finding::new(
                    Severity::High,
                    "approval_weakened",
                    format!(
                        "`{}` approval goes from `{}` to `{}`",
                        a.host,
                        p.ask.as_deref().unwrap_or("unset"),
                        a.ask.as_deref().unwrap_or("unset")
                    ),
                ));
            }
        }

        if prev.is_none() && rank_ask(a.ask.as_deref()) == 1 {
            let writes = cur_methods.is_empty()
                || cur_methods
                    .iter()
                    .any(|m| !READ_METHODS.contains(&m.as_str()));
            if writes {
                out.push(Finding::new(
                    Severity::Medium,
                    "new_write_host_no_approval",
                    format!("`{}` can be written to with no approval step", a.host),
                ));
            }
        }
    }

    // Deny entries that disappear re-open hosts.
    if let Some(o) = old {
        let kept: BTreeSet<String> = new.deny.iter().map(|d| norm_host(&d.host)).collect();
        for d in &o.deny {
            if !kept.contains(&norm_host(&d.host)) {
                out.push(Finding::new(
                    Severity::Medium,
                    "deny_removed",
                    format!("deny entry `{}` is removed", d.host),
                ));
            }
        }
        let had_taint = o.taint.as_ref().and_then(|t| t.on_untrusted_page.as_ref());
        let has_taint = new
            .taint
            .as_ref()
            .and_then(|t| t.on_untrusted_page.as_ref());
        if had_taint.is_some() && has_taint.is_none() {
            out.push(Finding::new(
                Severity::High,
                "taint_guard_removed",
                "`taint.on_untrusted_page` is removed: egress is no longer blocked after an untrusted page",
            ));
        }
    }

    out.sort_by_key(|f| std::cmp::Reverse(f.severity));
    out
}

/// True when a change should be refused unless the caller acknowledges it.
pub fn needs_ack(findings: &[Finding]) -> bool {
    findings.iter().any(|f| f.severity == Severity::High)
}

/// One-line summary of the High findings, for an error message.
pub fn summarize_high(findings: &[Finding]) -> String {
    findings
        .iter()
        .filter(|f| f.severity == Severity::High)
        .map(|f| format!("{}: {}", f.code, f.message))
        .collect::<Vec<_>>()
        .join("; ")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pol(yaml: &str) -> MerionPolicy {
        MerionPolicy::from_yaml(yaml).unwrap()
    }

    fn codes(f: &[Finding]) -> Vec<&'static str> {
        f.iter().map(|x| x.code).collect()
    }

    const BASE: &str = "version: 1\ndefault_egress: deny\nallow:\n  - { host: api.github.com, methods: [GET], ask: first }\n";

    #[test]
    fn identical_policy_has_no_findings() {
        let p = pol(BASE);
        assert!(review_change(Some(&p), &p).is_empty());
    }

    #[test]
    fn default_allow_is_high() {
        let new = pol("version: 1\ndefault_egress: allow\n");
        let f = review_change(Some(&pol(BASE)), &new);
        assert!(codes(&f).contains(&"default_egress_allow"));
        assert!(needs_ack(&f));
    }

    #[test]
    fn metadata_and_private_hosts_are_high() {
        let new = pol(
            "version: 1\nallow:\n  - { host: 169.254.169.254, methods: [GET], ask: always }\n  - { host: 10.0.0.5, methods: [GET], ask: always }\n  - { host: \"fd00::1\", methods: [GET], ask: always }\n",
        );
        let f = review_change(None, &new);
        assert!(codes(&f).contains(&"metadata_host"));
        assert_eq!(
            codes(&f)
                .iter()
                .filter(|c| **c == "private_ip_host")
                .count(),
            2
        );
    }

    #[test]
    fn internal_name_is_medium_not_blocking() {
        let new =
            pol("version: 1\nallow:\n  - { host: db.internal, methods: [GET], ask: always }\n");
        let f = review_change(None, &new);
        assert!(codes(&f).contains(&"internal_name_host"));
        assert!(!needs_ack(&f));
    }

    #[test]
    fn wildcards() {
        let new = pol("version: 1\nallow:\n  - { host: \"*\", methods: [GET], ask: always }\n  - { host: \"*.example.com\", methods: [GET], ask: always }\n");
        let f = review_change(None, &new);
        assert!(codes(&f).contains(&"wildcard_all_hosts"));
        assert!(codes(&f).contains(&"wildcard_host"));
    }

    #[test]
    fn new_public_host_is_info_only() {
        let new = pol(&format!(
            "{BASE}  - {{ host: api.example.com, methods: [GET], ask: first }}\n"
        ));
        let f = review_change(Some(&pol(BASE)), &new);
        assert_eq!(codes(&f), vec!["new_host"]);
        assert!(!needs_ack(&f));
    }

    #[test]
    fn write_method_added_is_high_read_method_is_medium() {
        let post = pol(
            "version: 1\nallow:\n  - { host: api.github.com, methods: [GET, POST], ask: first }\n",
        );
        let f = review_change(Some(&pol(BASE)), &post);
        assert_eq!(f[0].code, "methods_widened");
        assert_eq!(f[0].severity, Severity::High);

        let head = pol(
            "version: 1\nallow:\n  - { host: api.github.com, methods: [GET, HEAD], ask: first }\n",
        );
        let f = review_change(Some(&pol(BASE)), &head);
        assert_eq!(f[0].severity, Severity::Medium);
    }

    #[test]
    fn dropping_methods_list_means_all_methods() {
        let new = pol("version: 1\nallow:\n  - { host: api.github.com, ask: first }\n");
        let f = review_change(Some(&pol(BASE)), &new);
        assert!(needs_ack(&f));
    }

    #[test]
    fn approval_weakened() {
        let new =
            pol("version: 1\nallow:\n  - { host: api.github.com, methods: [GET], ask: never }\n");
        let f = review_change(Some(&pol(BASE)), &new);
        assert!(codes(&f).contains(&"approval_weakened"));
    }

    #[test]
    fn approval_strengthened_is_quiet() {
        let new =
            pol("version: 1\nallow:\n  - { host: api.github.com, methods: [GET], ask: always }\n");
        assert!(review_change(Some(&pol(BASE)), &new).is_empty());
    }

    #[test]
    fn removed_deny_and_taint_guard() {
        let old = pol("version: 1\ndeny:\n  - { host: \"*.onion\" }\ntaint:\n  on_untrusted_page: block_egress_until_ask\n");
        let new = pol("version: 1\n");
        let f = review_change(Some(&old), &new);
        assert!(codes(&f).contains(&"deny_removed"));
        assert!(codes(&f).contains(&"taint_guard_removed"));
        assert!(needs_ack(&f));
    }

    #[test]
    fn first_policy_counts_every_host_as_new() {
        let f = review_change(None, &pol(BASE));
        assert!(codes(&f).contains(&"new_host"));
    }

    #[test]
    fn high_findings_sort_first_and_summarize() {
        let new = pol("version: 1\ndefault_egress: allow\nallow:\n  - { host: api.example.com, methods: [GET], ask: first }\n");
        let f = review_change(Some(&pol(BASE)), &new);
        assert_eq!(f[0].severity, Severity::High);
        assert!(summarize_high(&f).contains("default_egress_allow"));
    }

    #[tokio::test]
    async fn put_policy_refuses_a_wider_change_until_acknowledged() {
        use axum::{body::Body, http::Request};
        use tower::ServiceExt;

        let token = crate::fixture::text("operator-token");
        let (state, _) =
            crate::egress::ask_tests::state_and_session_cfg(|c| c.api_token = Some(token.clone()))
                .await;
        let app = crate::app::public_router(state);

        let deploy = serde_json::json!({
            "name": "lint-desk",
            "bundle_base64": "ZXhwb3J0IGRlZmF1bHQgKCkgPT4ge30=",
            "manifest": {
                "template": "agent-node",
                "egress_mode": "ask",
                "egress_allow_hosts": ["api.github.com"]
            }
        });
        let req = Request::builder()
            .method("POST")
            .uri("/v1/agents")
            .header("authorization", format!("Bearer {token}"))
            .header("content-type", "application/json")
            .body(Body::from(deploy.to_string()))
            .unwrap();
        let resp = app.clone().oneshot(req).await.unwrap();
        assert_eq!(resp.status(), 201);

        let put = |yaml: &'static str, ack: bool| {
            let mut b = Request::builder()
                .method("PUT")
                .uri("/v1/agents/lint-desk/policy")
                .header("authorization", format!("Bearer {token}"))
                .header("content-type", "application/x-yaml");
            if ack {
                b = b.header("x-merion-policy-ack-risk", "1");
            }
            let app = app.clone();
            let req = b.body(Body::from(yaml)).unwrap();
            async move {
                let resp = app.oneshot(req).await.unwrap();
                let status = resp.status();
                let bytes = axum::body::to_bytes(resp.into_body(), 1 << 20)
                    .await
                    .unwrap();
                (status, String::from_utf8_lossy(&bytes).to_string())
            }
        };

        // A narrower policy goes through without an acknowledgement.
        let (st, body) = put(
            "version: 1\nallow:\n  - { host: api.github.com, methods: [GET], ask: always }\n",
            false,
        )
        .await;
        assert_eq!(st, 200, "{body}");

        // Flipping the default to allow is not enforced, so it is refused outright, ack or not.
        let unenforced = "version: 1\ndefault_egress: allow\n";
        let (st, body) = put(unenforced, true).await;
        assert_eq!(st, 400, "{body}");
        assert!(body.contains("not enforced"), "{body}");

        // Adding a write method widens access: refused until acknowledged, and says why.
        let wide =
            "version: 1\nallow:\n  - { host: api.github.com, methods: [GET, POST], ask: always }\n";
        let (st, body) = put(wide, false).await;
        assert_eq!(st, 409, "{body}");
        assert!(body.contains("methods_widened"), "{body}");
        assert!(body.contains("X-Merion-Policy-Ack-Risk"), "{body}");

        // The same change with the acknowledgement is applied and reports its risks.
        let (st, body) = put(wide, true).await;
        assert_eq!(st, 200, "{body}");
        assert!(body.contains("methods_widened"), "{body}");
    }

    #[test]
    fn dropping_or_widening_a_body_restriction_is_high() {
        let old = pol("version: 1\nallow:\n  - { host: mcp.example.com, methods: [POST], mcp_tools: [search], rpc_methods: [tools/call], ask: first }\n");
        let dropped =
            pol("version: 1\nallow:\n  - { host: mcp.example.com, methods: [POST], ask: first }\n");
        let f = review_change(Some(&old), &dropped);
        assert_eq!(
            f.iter().filter(|x| x.code == "body_rules_widened").count(),
            2
        );
        assert!(needs_ack(&f));

        let wider = pol("version: 1\nallow:\n  - { host: mcp.example.com, methods: [POST], mcp_tools: [search, delete], rpc_methods: [tools/call], ask: first }\n");
        assert_eq!(
            codes(&review_change(Some(&old), &wider)),
            vec!["body_rules_widened"]
        );

        let narrower = pol("version: 1\nallow:\n  - { host: mcp.example.com, methods: [POST], mcp_tools: [search], rpc_methods: [tools/call], ask: first }\n");
        assert!(review_change(Some(&old), &narrower).is_empty());
        // Adding a restriction where there was none is not a widening.
        let base = pol(BASE);
        let restricted = pol("version: 1\nallow:\n  - { host: api.github.com, methods: [GET], graphql_operations: [query], ask: first }\n");
        assert!(review_change(Some(&base), &restricted).is_empty());
    }

    #[test]
    fn widening_who_may_call_is_high_and_narrowing_is_not() {
        let h = "a".repeat(64);
        let base = |b: &str| {
            pol(&format!("version: 1\nallow:\n  - {{ host: api.example.com, methods: [GET], ask: first, binaries: [{b}] }}\n"))
        };
        let old = base(
            "{ path: /usr/bin/curl, sha256: HASH }"
                .replace("HASH", &h)
                .as_str(),
        );
        // Dropped entirely: anyone may call.
        let dropped =
            pol("version: 1\nallow:\n  - { host: api.example.com, methods: [GET], ask: first }\n");
        assert!(codes(&review_change(Some(&old), &dropped)).contains(&"binaries_widened"));
        // A second program.
        let more = base(&format!(
            "{{ path: /usr/bin/curl, sha256: {h} }}, {{ path: /usr/bin/node }}"
        ));
        assert!(needs_ack(&review_change(Some(&old), &more)));
        // The pin is loosened to trust-on-first-use.
        let unpinned = base("{ path: /usr/bin/curl }");
        assert!(codes(&review_change(Some(&old), &unpinned)).contains(&"binaries_widened"));
        // A different pinned hash is a replacement, not the same program.
        let swapped = base(&format!(
            "{{ path: /usr/bin/curl, sha256: {} }}",
            "b".repeat(64)
        ));
        assert!(codes(&review_change(Some(&old), &swapped)).contains(&"binaries_widened"));
        // Same list: quiet. Pinning a hash where there was none: narrower, quiet.
        assert!(review_change(Some(&old), &old).is_empty());
        let was_open = base("{ path: /usr/bin/curl }");
        let pinned = base(&format!("{{ path: /usr/bin/curl, sha256: {h} }}"));
        assert!(review_change(Some(&was_open), &pinned).is_empty());
        // Adding a restriction where there was none is not a widening.
        assert!(review_change(Some(&dropped), &old).is_empty());
    }
}
