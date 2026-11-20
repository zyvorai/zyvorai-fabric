// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: LicenseRef-Zyvor-Production-1.0

//! Request-level (layer 7) egress policy for the JSON broker: per-host rules and
//! a data-loss scan. Both run on what the agent supplied (URL, headers, body)
//! before anything leaves the host.
//!
//! The CONNECT proxy cannot apply either: a TLS tunnel exposes only `host:port`.

use crate::{
    binary_id::BinaryIdentity,
    credentials::host_matches,
    model::{BinaryRule, EgressRule},
};

/// Bodies larger than this are not parsed for `rpc_methods`, `mcp_tools` or `graphql_operations`;
/// a rule that needs to read the body refuses them instead.
pub const MAX_PARSED_BODY: usize = 1024 * 1024;

/// Check a request against the agent's rules for `host`.
///
/// A host with no rules is unrestricted (the allowlist already gates it). A host
/// with rules needs at least one rule that matches the method, the path prefix,
/// the body size and, when the rule names JSON-RPC methods, MCP tools or GraphQL
/// operations, what the body actually asks for. A body that cannot be read as the
/// expected JSON never satisfies such a rule.
///
/// A rule that lists `binaries` also needs `caller`, the program that made the request.
/// `None` means the caller could not be identified, and such a rule then refuses.
pub fn check_rules(
    rules: &[EgressRule],
    host: &str,
    method: &str,
    path: &str,
    body: &[u8],
    caller: Option<&BinaryIdentity>,
) -> Result<(), String> {
    let body_len = body.len();
    let mut any = false;
    let mut body_refused = false;
    let mut binary_refused = false;
    for rule in rules.iter().filter(|r| host_matches(&r.host, host)) {
        any = true;
        let method_ok =
            rule.methods.is_empty() || rule.methods.iter().any(|m| m.eq_ignore_ascii_case(method));
        let path_ok =
            rule.path_prefixes.is_empty() || rule.path_prefixes.iter().any(|p| path.starts_with(p));
        let size_ok = rule.max_body_bytes.is_none_or(|max| body_len as u64 <= max);
        if !(method_ok && path_ok && size_ok) {
            continue;
        }
        if !body_allowed(rule, body) {
            body_refused = true;
            continue;
        }
        if !binary_allowed(rule, caller) {
            binary_refused = true;
            continue;
        }
        return Ok(());
    }
    if !any {
        Ok(())
    } else if binary_refused && !body_refused {
        Err(format!(
            "{method} {path} is not permitted by this agent's egress rules for {host}: the program making the request is not one of the allowed programs, or could not be identified"
        ))
    } else if body_refused {
        Err(format!(
            "{method} {path} is not permitted by this agent's egress rules for {host}: the request body is not an allowed JSON-RPC, MCP or GraphQL call"
        ))
    } else {
        Err(format!(
            "{method} {path} ({body_len} body bytes) is not permitted by this agent's egress rules for {host}"
        ))
    }
}

/// Does the caller satisfy the rule's `binaries`? Always true for a rule that lists none.
fn binary_allowed(rule: &EgressRule, caller: Option<&BinaryIdentity>) -> bool {
    if rule.binaries.is_empty() {
        return true;
    }
    let Some(caller) = caller else {
        return false;
    };
    rule.binaries.iter().any(|b| {
        b.path == caller.path
            && b.sha256
                .as_deref()
                .is_none_or(|pinned| pinned.eq_ignore_ascii_case(&caller.sha256))
    })
}

/// The `binaries` entries of every rule for `host`. Non-empty means the caller must be identified.
pub fn host_binary_rules<'a>(rules: &'a [EgressRule], host: &str) -> Vec<&'a BinaryRule> {
    rules
        .iter()
        .filter(|r| host_matches(&r.host, host))
        .flat_map(|r| r.binaries.iter())
        .collect()
}

fn reads_body(rule: &EgressRule) -> bool {
    !rule.rpc_methods.is_empty()
        || !rule.mcp_tools.is_empty()
        || !rule.graphql_operations.is_empty()
}

/// Does the body satisfy the rule's JSON-RPC, MCP and GraphQL constraints? Always true for a
/// rule that names none. Fail closed: an unreadable body, or one that is too large, is refused.
fn body_allowed(rule: &EgressRule, body: &[u8]) -> bool {
    if !reads_body(rule) {
        return true;
    }
    if body.len() > MAX_PARSED_BODY {
        return false;
    }
    let Ok(doc) = serde_json::from_slice::<serde_json::Value>(body) else {
        return false;
    };
    let messages: Vec<&serde_json::Value> = match &doc {
        serde_json::Value::Array(items) if !items.is_empty() => items.iter().collect(),
        serde_json::Value::Array(_) => return false,
        other => vec![other],
    };
    if (!rule.rpc_methods.is_empty() || !rule.mcp_tools.is_empty())
        && !messages.iter().all(|m| rpc_message_allowed(rule, m))
    {
        return false;
    }
    if !rule.graphql_operations.is_empty() {
        return messages.iter().all(|m| graphql_allowed(rule, m));
    }
    true
}

/// One JSON-RPC 2.0 message. A response to a server-initiated request (an `id` and a `result` or
/// `error`, no `method`) carries no call, so it passes; MCP clients send these.
fn rpc_message_allowed(rule: &EgressRule, msg: &serde_json::Value) -> bool {
    let Some(obj) = msg.as_object() else {
        return false;
    };
    let Some(method) = obj.get("method").and_then(|m| m.as_str()) else {
        return obj.contains_key("id") && (obj.contains_key("result") || obj.contains_key("error"));
    };
    if !rule.rpc_methods.is_empty() && !rule.rpc_methods.iter().any(|m| m == method) {
        return false;
    }
    if !rule.mcp_tools.is_empty() && method == "tools/call" {
        let tool = obj
            .get("params")
            .and_then(|p| p.get("name"))
            .and_then(|n| n.as_str());
        return tool.is_some_and(|t| rule.mcp_tools.iter().any(|allowed| allowed == t));
    }
    true
}

/// A GraphQL-over-HTTP body: `{"query": "..."}`. Every operation in the document must be an
/// allowed type, whichever one `operationName` would pick.
fn graphql_allowed(rule: &EgressRule, msg: &serde_json::Value) -> bool {
    let Some(query) = msg.get("query").and_then(|q| q.as_str()) else {
        return false;
    };
    let Some(ops) = graphql_operation_types(query) else {
        return false;
    };
    !ops.is_empty()
        && ops
            .iter()
            .all(|op| rule.graphql_operations.iter().any(|allowed| allowed == op))
}

/// The operation type of each top-level operation in a GraphQL document, or `None` if the
/// document is not one this scanner can read with confidence (unbalanced braces or an unclosed
/// string). Fragments are skipped; a bare `{ ... }` is a query.
pub fn graphql_operation_types(doc: &str) -> Option<Vec<&'static str>> {
    let b = doc.as_bytes();
    let (mut i, mut depth, mut ops) = (0usize, 0i32, Vec::new());
    let mut expect_definition = true;
    while i < b.len() {
        match b[i] {
            b'#' => {
                while i < b.len() && b[i] != b'\n' && b[i] != b'\r' {
                    i += 1;
                }
            }
            b'"' => {
                if b[i..].starts_with(b"\"\"\"") {
                    i += 3;
                    loop {
                        if i >= b.len() {
                            return None;
                        }
                        if b[i..].starts_with(b"\\\"\"\"") {
                            i += 4;
                        } else if b[i..].starts_with(b"\"\"\"") {
                            i += 3;
                            break;
                        } else {
                            i += 1;
                        }
                    }
                } else {
                    i += 1;
                    loop {
                        match b.get(i) {
                            None | Some(b'\n') => return None,
                            Some(b'\\') => i += 2,
                            Some(b'"') => {
                                i += 1;
                                break;
                            }
                            Some(_) => i += 1,
                        }
                    }
                }
            }
            b'{' => {
                if depth == 0 && expect_definition {
                    ops.push("query"); // shorthand query
                }
                depth += 1;
                i += 1;
            }
            b'}' => {
                depth -= 1;
                if depth < 0 {
                    return None;
                }
                if depth == 0 {
                    expect_definition = true;
                }
                i += 1;
            }
            c if depth == 0 && (c.is_ascii_alphabetic() || c == b'_') => {
                let start = i;
                while i < b.len() && (b[i].is_ascii_alphanumeric() || b[i] == b'_') {
                    i += 1;
                }
                if expect_definition {
                    match &doc[start..i] {
                        "query" => ops.push("query"),
                        "mutation" => ops.push("mutation"),
                        "subscription" => ops.push("subscription"),
                        "fragment" => {}
                        _ => return None, // not a definition keyword: do not guess
                    }
                    expect_definition = false;
                }
            }
            _ => i += 1,
        }
    }
    (depth == 0).then_some(ops)
}

/// True when `host` has rules, which a TLS tunnel could not enforce.
pub fn host_has_rules(rules: &[EgressRule], host: &str) -> bool {
    rules.iter().any(|r| host_matches(&r.host, host))
}

/// Names of the secret shapes found in `text`. Only names are returned, never
/// the matched text, so the result is safe to store and show to an operator.
pub fn scan(text: &str) -> Vec<&'static str> {
    let mut found = Vec::new();
    let mut add = |name: &'static str, hit: bool| {
        if hit && !found.contains(&name) {
            found.push(name);
        }
    };
    add(
        "private-key",
        text.contains("-----BEGIN") && text.contains("PRIVATE KEY-----"),
    );
    add("aws-access-key", token_after(text, "AKIA", 16, upper_alnum));
    for prefix in ["ghp_", "gho_", "ghu_", "ghs_", "ghr_"] {
        add("github-token", token_after(text, prefix, 36, alnum));
    }
    add(
        "github-token",
        token_after(text, "github_pat_", 22, alnum_underscore),
    );
    add("slack-token", slack_token(text));
    add("api-key", token_after(text, "sk-", 32, alnum_dash));
    add("jwt", has_jwt(text));
    found
}

fn upper_alnum(c: char) -> bool {
    c.is_ascii_uppercase() || c.is_ascii_digit()
}
fn alnum(c: char) -> bool {
    c.is_ascii_alphanumeric()
}
fn alnum_underscore(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '_'
}
fn alnum_dash(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '-' || c == '_'
}

/// `prefix` followed by at least `min` characters accepted by `class`.
fn token_after(text: &str, prefix: &str, min: usize, class: fn(char) -> bool) -> bool {
    text.match_indices(prefix).any(|(at, _)| {
        text[at + prefix.len()..]
            .chars()
            .take_while(|c| class(*c))
            .count()
            >= min
    })
}

fn slack_token(text: &str) -> bool {
    ["xoxb-", "xoxp-", "xoxa-", "xoxs-"]
        .iter()
        .any(|p| token_after(text, p, 20, alnum_dash))
}

/// Three dot-separated base64url segments, the first two starting `eyJ` (JSON).
fn has_jwt(text: &str) -> bool {
    text.match_indices("eyJ").any(|(at, _)| {
        let candidate: String = text[at..]
            .chars()
            .take_while(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'))
            .collect();
        let parts: Vec<&str> = candidate.split('.').collect();
        parts.len() >= 3
            && parts[1].starts_with("eyJ")
            && parts.iter().take(3).all(|p| p.len() >= 8)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rule(host: &str, methods: &[&str], prefixes: &[&str], max: Option<u64>) -> EgressRule {
        EgressRule {
            host: host.into(),
            methods: methods.iter().map(|s| s.to_string()).collect(),
            path_prefixes: prefixes.iter().map(|s| s.to_string()).collect(),
            max_body_bytes: max,
            ..Default::default()
        }
    }

    fn n(len: usize) -> Vec<u8> {
        vec![b'x'; len]
    }

    /// Most tests are about the request, not the caller.
    fn check_rules(
        rules: &[EgressRule],
        host: &str,
        method: &str,
        path: &str,
        body: &[u8],
    ) -> Result<(), String> {
        super::check_rules(rules, host, method, path, body, None)
    }

    #[test]
    fn hosts_without_rules_are_unrestricted() {
        let rules = [rule("api.example.com", &["GET"], &[], None)];
        assert!(check_rules(&rules, "other.example", "POST", "/x", &n(1_000_000)).is_ok());
        assert!(check_rules(&[], "api.example.com", "DELETE", "/x", &[]).is_ok());
    }

    #[test]
    fn a_host_with_rules_needs_one_matching_rule() {
        let rules = [
            rule("api.example.com", &["GET"], &[], None),
            rule("api.example.com", &["POST"], &["/v1/messages"], Some(1024)),
        ];
        assert!(check_rules(&rules, "api.example.com", "get", "/anything", &[]).is_ok());
        assert!(check_rules(
            &rules,
            "api.example.com",
            "POST",
            "/v1/messages/1",
            &n(1024)
        )
        .is_ok());
        for (method, path, len) in [
            ("POST", "/v1/messages", 1025),
            ("POST", "/v1/other", 10),
            ("DELETE", "/v1/messages", 0),
        ] {
            let error = check_rules(&rules, "api.example.com", method, path, &n(len)).unwrap_err();
            assert!(error.contains("egress rules"), "{error}");
        }
    }

    #[test]
    fn rules_match_subdomains_like_the_allowlist_does() {
        let rules = [rule("example.com", &["GET"], &[], None)];
        assert!(host_has_rules(&rules, "api.example.com"));
        assert!(check_rules(&rules, "api.example.com", "POST", "/", &[]).is_err());
        assert!(!host_has_rules(&rules, "evilexample.com"));
    }

    #[test]
    fn scan_finds_common_secret_shapes_by_name_only() {
        let aws = "key=AKIAIOSFODNN7EXAMPLE";
        assert_eq!(scan(aws), vec!["aws-access-key"]);
        assert_eq!(
            scan("-----BEGIN RSA PRIVATE KEY-----\nMIIE"),
            vec!["private-key"]
        );
        assert_eq!(
            scan(&format!("token ghp_{}", "a1B2".repeat(9))),
            vec!["github-token"]
        );
        assert_eq!(scan(&format!("sk-{}", "x".repeat(40))), vec!["api-key"]);
        let jwt = "eyJhbGciOiJIUzI1NiJ9.eyJzdWIiOiIxMjM0NTY3ODkwIn0.dBjftJeZ4CVPmB92K27uhbUJU1p1r";
        assert_eq!(scan(jwt), vec!["jwt"]);
        assert_eq!(
            scan(&format!("xoxb-{}", "1".repeat(24))),
            vec!["slack-token"]
        );
        // Nothing echoes the secret itself.
        for name in scan(aws) {
            assert!(!aws.contains(name));
        }
    }

    #[test]
    fn scan_ignores_ordinary_text_and_short_lookalikes() {
        for text in [
            "hello world",
            "AKIA short",
            "ghp_tooshort",
            "sk-abc",
            "eyJ only one segment",
            "the price is $5 - sk",
        ] {
            assert!(scan(text).is_empty(), "{text}");
        }
    }

    fn body_rule(rpc: &[&str], tools: &[&str], gql: &[&str]) -> EgressRule {
        EgressRule {
            host: "mcp.example.com".into(),
            methods: vec!["POST".into()],
            rpc_methods: rpc.iter().map(|s| s.to_string()).collect(),
            mcp_tools: tools.iter().map(|s| s.to_string()).collect(),
            graphql_operations: gql.iter().map(|s| s.to_string()).collect(),
            ..Default::default()
        }
    }

    fn call(rules: &[EgressRule], body: &str) -> Result<(), String> {
        check_rules(rules, "mcp.example.com", "POST", "/rpc", body.as_bytes())
    }

    #[test]
    fn rpc_methods_limit_what_the_body_can_call() {
        let rules = [body_rule(&["tools/list", "tools/call"], &[], &[])];
        assert!(call(&rules, r#"{"jsonrpc":"2.0","id":1,"method":"tools/list"}"#).is_ok());
        assert!(call(
            &rules,
            r#"{"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"x"}}"#
        )
        .is_ok());
        let err = call(
            &rules,
            r#"{"jsonrpc":"2.0","id":1,"method":"resources/read"}"#,
        )
        .unwrap_err();
        assert!(err.contains("request body"), "{err}");
        // Method names are case-sensitive in JSON-RPC.
        assert!(call(&rules, r#"{"jsonrpc":"2.0","id":1,"method":"Tools/List"}"#).is_err());
    }

    #[test]
    fn a_batch_needs_every_call_allowed() {
        let rules = [body_rule(&["tools/list"], &[], &[])];
        assert!(call(
            &rules,
            r#"[{"method":"tools/list","id":1},{"method":"tools/list","id":2}]"#
        )
        .is_ok());
        assert!(call(
            &rules,
            r#"[{"method":"tools/list","id":1},{"method":"tools/call","id":2}]"#
        )
        .is_err());
        assert!(call(&rules, "[]").is_err());
    }

    #[test]
    fn mcp_tools_limit_tools_call_by_name() {
        let rules = [body_rule(&["tools/call", "tools/list"], &["search"], &[])];
        assert!(call(
            &rules,
            r#"{"method":"tools/call","id":1,"params":{"name":"search","arguments":{}}}"#
        )
        .is_ok());
        assert!(call(
            &rules,
            r#"{"method":"tools/call","id":1,"params":{"name":"delete_all"}}"#
        )
        .is_err());
        // A tools/call with no readable tool name is refused, not waved through.
        assert!(call(&rules, r#"{"method":"tools/call","id":1,"params":{}}"#).is_err());
        assert!(call(
            &rules,
            r#"{"method":"tools/call","id":1,"params":{"name":7}}"#
        )
        .is_err());
        // Other methods are not tool calls.
        assert!(call(&rules, r#"{"method":"tools/list","id":1}"#).is_ok());
    }

    #[test]
    fn responses_to_server_requests_pass_but_junk_does_not() {
        let rules = [body_rule(&["tools/list"], &[], &[])];
        assert!(call(&rules, r#"{"jsonrpc":"2.0","id":9,"result":{"roots":[]}}"#).is_ok());
        assert!(call(
            &rules,
            r#"{"jsonrpc":"2.0","id":9,"error":{"code":-1,"message":"no"}}"#
        )
        .is_ok());
        assert!(call(&rules, r#"{"jsonrpc":"2.0"}"#).is_err());
        assert!(call(&rules, r#"{"method":7}"#).is_err());
        assert!(call(&rules, "not json").is_err());
        assert!(call(&rules, "").is_err());
        assert!(call(&rules, "42").is_err());
    }

    #[test]
    fn oversized_bodies_are_refused_by_body_rules_but_not_by_plain_ones() {
        let big = format!(
            r#"{{"method":"tools/list","pad":"{}"}}"#,
            "a".repeat(MAX_PARSED_BODY)
        );
        assert!(call(&[body_rule(&["tools/list"], &[], &[])], &big).is_err());
        assert!(call(&[rule("mcp.example.com", &["POST"], &[], None)], &big).is_ok());
    }

    #[test]
    fn graphql_operation_types_gate_the_document() {
        let rules = [body_rule(&[], &[], &["query"])];
        let q = |s: &str| serde_json::json!({ "query": s }).to_string();
        assert!(call(&rules, &q("{ viewer { login } }")).is_ok());
        assert!(call(&rules, &q("query Me { viewer { login } }")).is_ok());
        assert!(call(&rules, &q("mutation { deleteRepo(id: 1) { ok } }")).is_err());
        assert!(call(&rules, &q("subscription { tick }")).is_err());
        // Every operation in the document counts, whichever operationName selects.
        assert!(call(&rules, &q("query A { a } mutation B { b }")).is_err());
        assert!(call(&rules, &q("query A { a } fragment F on T { x }")).is_ok());
        // A batch needs every entry allowed.
        let batch = format!("[{},{}]", q("{ a }"), q("mutation { b }"));
        assert!(call(&rules, &batch).is_err());
        // No query, or a GET-style body-less call, cannot satisfy the rule.
        assert!(call(&rules, r#"{"operationName":"X"}"#).is_err());
        assert!(call(&rules, "").is_err());
        // Allowing mutations too is a choice the rule can make.
        let both = [body_rule(&[], &[], &["query", "mutation"])];
        assert!(call(&both, &q("mutation { b }")).is_ok());
    }

    #[test]
    fn graphql_scanner_is_not_fooled_by_comments_strings_or_bad_input() {
        let ops = |s: &str| graphql_operation_types(s);
        assert_eq!(ops("# mutation { x }\n{ a }"), Some(vec!["query"]));
        assert_eq!(ops(r#"{ a(s: "mutation { x }") }"#), Some(vec!["query"]));
        assert_eq!(ops(r#"{ a(s: "say \"hi\" {") }"#), Some(vec!["query"]));
        assert_eq!(ops("{ a(s: \"\"\"block { \"\"\" ) }"), Some(vec!["query"]));
        assert_eq!(
            ops("query { a } mutation { b }"),
            Some(vec!["query", "mutation"])
        );
        assert_eq!(ops("fragment F on T { x }"), Some(vec![]));
        // Not confident: refuse rather than guess.
        assert_eq!(ops("{ a "), None);
        assert_eq!(ops("a }"), None);
        assert_eq!(ops(r#"{ a(s: "unclosed) }"#), None);
        assert_eq!(ops("extend schema { query: Q }"), None);
        assert_eq!(ops("nonsense { a }"), None);
    }

    #[test]
    fn any_matching_rule_may_allow_the_call() {
        let rules = [
            body_rule(&["tools/list"], &[], &[]),
            body_rule(&["tools/call"], &["search"], &[]),
        ];
        assert!(call(
            &rules,
            r#"{"method":"tools/call","id":1,"params":{"name":"search"}}"#
        )
        .is_ok());
        assert!(call(&rules, r#"{"method":"tools/list","id":1}"#).is_ok());
        assert!(call(
            &rules,
            r#"{"method":"tools/call","id":1,"params":{"name":"other"}}"#
        )
        .is_err());
    }

    fn who(path: &str, sha: &str) -> BinaryIdentity {
        BinaryIdentity {
            pid: 1,
            path: path.into(),
            sha256: sha.into(),
        }
    }

    fn bin_rule(methods: &[&str], binaries: &[(&str, Option<&str>)]) -> EgressRule {
        EgressRule {
            host: "api.example.com".into(),
            methods: methods.iter().map(|s| s.to_string()).collect(),
            binaries: binaries
                .iter()
                .map(|(p, h)| BinaryRule {
                    path: (*p).into(),
                    sha256: h.map(str::to_string),
                })
                .collect(),
            ..Default::default()
        }
    }

    fn as_caller(
        rules: &[EgressRule],
        method: &str,
        caller: Option<&BinaryIdentity>,
    ) -> Result<(), String> {
        super::check_rules(rules, "api.example.com", method, "/x", b"", caller)
    }

    #[test]
    fn a_rule_with_binaries_needs_the_right_program() {
        let rules = [bin_rule(
            &["GET"],
            &[("/usr/bin/curl", None), ("/usr/bin/node", None)],
        )];
        assert!(as_caller(&rules, "GET", Some(&who("/usr/bin/curl", &"a".repeat(64)))).is_ok());
        assert!(as_caller(&rules, "GET", Some(&who("/usr/bin/node", &"a".repeat(64)))).is_ok());
        let err =
            as_caller(&rules, "GET", Some(&who("/tmp/dropper", &"a".repeat(64)))).unwrap_err();
        assert!(err.contains("program"), "{err}");
        // A path that only looks similar is a different program.
        assert!(as_caller(&rules, "GET", Some(&who("/usr/bin/curl2", &"a".repeat(64)))).is_err());
        assert!(as_caller(&rules, "GET", Some(&who("/usr/bin/", &"a".repeat(64)))).is_err());
    }

    #[test]
    fn an_unidentified_caller_never_satisfies_a_binaries_rule() {
        let rules = [bin_rule(&["GET"], &[("/usr/bin/curl", None)])];
        assert!(as_caller(&rules, "GET", None)
            .unwrap_err()
            .contains("could not be identified"));
        // A rule with no binaries does not need a caller at all.
        let open = [bin_rule(&["GET"], &[])];
        assert!(as_caller(&open, "GET", None).is_ok());
    }

    #[test]
    fn a_pinned_hash_must_match_and_an_unpinned_one_is_left_to_first_use() {
        let good = "a".repeat(64);
        let rules = [bin_rule(&["GET"], &[("/usr/bin/curl", Some(&good))])];
        assert!(as_caller(&rules, "GET", Some(&who("/usr/bin/curl", &good))).is_ok());
        assert!(as_caller(&rules, "GET", Some(&who("/usr/bin/curl", &"b".repeat(64)))).is_err());
        let unpinned = [bin_rule(&["GET"], &[("/usr/bin/curl", None)])];
        assert!(as_caller(
            &unpinned,
            "GET",
            Some(&who("/usr/bin/curl", &"b".repeat(64)))
        )
        .is_ok());
    }

    #[test]
    fn a_looser_rule_does_not_let_another_program_through_a_stricter_one() {
        // GET is for curl only; POST is for anyone. A GET from node must not ride the POST rule.
        let rules = [
            bin_rule(&["GET"], &[("/usr/bin/curl", None)]),
            bin_rule(&["POST"], &[]),
        ];
        let node = who("/usr/bin/node", &"a".repeat(64));
        assert!(as_caller(&rules, "GET", Some(&node)).is_err());
        assert!(as_caller(&rules, "POST", Some(&node)).is_ok());
        assert!(as_caller(&rules, "GET", Some(&who("/usr/bin/curl", &"a".repeat(64)))).is_ok());
    }

    #[test]
    fn host_binary_rules_collects_across_rules_and_subdomains() {
        let rules = [
            bin_rule(&["GET"], &[("/usr/bin/curl", None)]),
            bin_rule(&["POST"], &[("/usr/bin/node", None)]),
            EgressRule {
                host: "other.example".into(),
                binaries: vec![BinaryRule {
                    path: "/x".into(),
                    sha256: None,
                }],
                ..Default::default()
            },
        ];
        assert_eq!(host_binary_rules(&rules, "api.example.com").len(), 2);
        assert!(host_binary_rules(&rules, "unrelated.test").is_empty());
    }
}
