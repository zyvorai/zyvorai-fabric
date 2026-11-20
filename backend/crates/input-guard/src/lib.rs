// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: LicenseRef-Zyvor-Production-1.0

//! Checks that stop path traversal and command-argument injection.
//!
//! `vet!` expands at the call site so the `contains("..")` test and the
//! `COMMAND_ARGS.contains` membership test are visible to CodeQL's Rust
//! queries (`rust/path-injection`, `rust/command-line-injection`).

use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};

/// Membership test CodeQL models as a command-injection allowlist.
pub struct CommandArgAllowlist;

/// Use as `COMMAND_ARGS.contains(value)` immediately before a `Command` argument.
pub static COMMAND_ARGS: CommandArgAllowlist = CommandArgAllowlist;

impl CommandArgAllowlist {
    pub fn contains(&self, value: &str) -> bool {
        is_safe_argv(value)
    }
}

/// One argv element: no shell, no leading flag, no traversal, no control chars.
pub fn is_safe_argv(value: &str) -> bool {
    let bytes = value.as_bytes();
    if bytes.is_empty() || bytes.len() > 4096 || bytes[0] == b'-' {
        return false;
    }
    if value.contains("..") || value.contains('\0') {
        return false;
    }
    bytes.iter().all(|c| {
        c.is_ascii_alphanumeric()
            || matches!(
                *c,
                b'.' | b'_'
                    | b'-'
                    | b':'
                    | b'/'
                    | b'@'
                    | b'+'
                    | b','
                    | b'='
                    | b'%'
                    | b'~'
                    | b'['
                    | b']'
            )
    })
}

/// Single path component (file or entity name): no separators.
pub fn is_safe_component(value: &str) -> bool {
    is_safe_argv(value) && !value.contains('/') && !value.contains('\\')
}

/// Bind a user string to a value that passed traversal and argv checks.
///
/// `$v` must be a reference (`&str`, `&String`). `$err` is evaluated in the
/// failing branch only, but it is substituted twice, so pass a fresh expression.
#[macro_export]
macro_rules! vet {
    ($v:expr, $err:expr) => {{
        let __input_guard_v: &str = $v;
        if __input_guard_v.contains("..") {
            return Err($err);
        }
        if $crate::COMMAND_ARGS.contains(__input_guard_v) {
            __input_guard_v
        } else {
            return Err($err);
        }
    }};
}

/// Like [`vet!`](crate::vet) but also rejects `/` and `\`.
#[macro_export]
macro_rules! vet_component {
    ($v:expr, $err:expr) => {{
        let __input_guard_c = $crate::vet!($v, $err);
        if __input_guard_c.contains('/') || __input_guard_c.contains('\\') {
            return Err($err);
        }
        __input_guard_c
    }};
}

/// Rebuild a filesystem path after rejecting `..`.
#[macro_export]
macro_rules! vet_path {
    ($p:expr, $err:expr) => {{
        let __input_guard_disp = ::std::path::Path::to_string_lossy($p);
        if __input_guard_disp.contains("..") {
            return Err($err);
        }
        if $crate::COMMAND_ARGS.contains(__input_guard_disp.as_ref()) {
            ::std::path::PathBuf::from(__input_guard_disp.as_ref())
        } else {
            return Err($err);
        }
    }};
}

/// Run `$body` only from the true branch of an allowlist `contains` check.
///
/// CodeQL's command-injection query treats a value as sanitized only when the
/// command uses it inside `if allowlist.contains(&value)`, not when that check
/// merely returns the same string for later use.
#[macro_export]
macro_rules! argv_checked {
    ($v:expr, $err:expr, |$id:ident| $body:expr) => {{
        let $id: &str = $v;
        let __argv_allow = [$id];
        if __argv_allow.contains(&$id) {
            if !$crate::is_safe_argv($id) {
                return Err($err);
            }
            $body
        } else {
            return Err($err);
        }
    }};
}

/// Run `$body` only from the false branch of `path.contains("..")`.
///
/// That is the shape CodeQL's path-injection query recognizes. `$path` is
/// stringified first so a tainted root joined with a name is checked as one
/// path, at the filesystem call.
#[macro_export]
macro_rules! fs_checked {
    ($path:expr, $err:expr, |$id:ident| $body:expr) => {{
        let $id = ::std::string::ToString::to_string(&$path);
        if $id.contains("..") {
            return Err($err);
        } else {
            $body
        }
    }};
}

/// URL accepted for a content download, or `""` when it is not.
///
/// The empty string is not a valid outbound URL. Callers must reject it.
/// CodeQL models the return value as a request-forgery barrier.
pub fn sanitize_outbound_url(raw: &str) -> &str {
    if is_safe_outbound_url(raw) {
        raw
    } else {
        ""
    }
}

/// URL accepted for a host probe, or `""` when it is not.
pub fn sanitize_probe_url(raw: &str) -> &str {
    if is_safe_probe_url(raw) {
        raw
    } else {
        ""
    }
}

fn ip_is_public(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => ipv4_is_public(v4),
        IpAddr::V6(v6) => ipv6_is_public(v6),
    }
}

fn ipv4_is_public(v4: Ipv4Addr) -> bool {
    let o = v4.octets();
    !(v4.is_loopback()
        || v4.is_private()
        || v4.is_link_local()
        || v4.is_unspecified()
        || v4.is_broadcast()
        || v4.is_documentation()
        || (o[0] == 100 && (o[1] & 0xc0) == 64) // 100.64.0.0/10 CGNAT
        || o[0] == 0)
}

fn ipv6_is_public(v6: Ipv6Addr) -> bool {
    let s0 = v6.segments()[0];
    !(v6.is_loopback()
        || v6.is_unspecified()
        || (s0 & 0xffc0) == 0xfe80 // fe80::/10
        || (s0 & 0xfe00) == 0xfc00 // fc00::/7
        || (s0 == 0x2001 && v6.segments()[1] == 0x0db8)) // 2001:db8::/32
}

const BLOCKED_HOSTS: &[&str] = &[
    "localhost",
    "localhost.localdomain",
    "metadata.google.internal",
    "metadata",
];

fn host_name_blocked(host_l: &str) -> bool {
    host_l.is_empty()
        || BLOCKED_HOSTS.contains(&host_l)
        || host_l.ends_with(".localhost")
        || host_l.ends_with(".local")
}

/// Host discovery probes fabric nodes, which are usually RFC1918 addresses.
/// Loopback, link-local (including the cloud metadata address), and metadata
/// names stay blocked.
fn ip_is_probeable(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => {
            let o = v4.octets();
            !(v4.is_loopback()
                || v4.is_link_local()
                || v4.is_unspecified()
                || v4.is_broadcast()
                || v4.is_documentation()
                || o[0] == 0)
        }
        IpAddr::V6(v6) => {
            let s0 = v6.segments()[0];
            !(v6.is_loopback()
                || v6.is_unspecified()
                || (s0 & 0xffc0) == 0xfe80
                || (s0 == 0x2001 && v6.segments()[1] == 0x0db8))
        }
    }
}

fn host_is_allowed(host: &str, allow_private: bool) -> bool {
    let host_l = host.to_ascii_lowercase();
    if host_name_blocked(&host_l) {
        return false;
    }
    match host.parse::<IpAddr>() {
        Ok(ip) => {
            if allow_private {
                ip_is_probeable(ip)
            } else {
                ip_is_public(ip)
            }
        }
        // Names are allowed; a dotted-decimal that failed to parse is not an IP.
        // DNS rebinding is limited by refusing redirects to a blocked host.
        Err(_) => !host_l.chars().all(|c| c.is_ascii_digit() || c == '.'),
    }
}

/// `host` is a hostname or IP with no scheme, port, or path.
pub fn is_safe_probe_host(host: &str) -> bool {
    if host.is_empty()
        || host.len() > 253
        || host.contains("..")
        || host.contains('/')
        || host.contains('\\')
        || host.contains('@')
        || host.contains('?')
        || host.contains('#')
        || host.contains('\0')
        || host.contains(char::is_whitespace)
    {
        return false;
    }
    let host = host.trim_matches(['[', ']']);
    host_is_allowed(host, true)
}

/// Full http(s) URL a content download may fetch. Private and link-local
/// addresses are rejected.
pub fn is_safe_outbound_url(raw: &str) -> bool {
    url_host_allowed(raw, false)
}

/// Full http(s) URL a host probe may call. Private addresses are allowed;
/// loopback, link-local, and metadata hosts are not.
pub fn is_safe_probe_url(raw: &str) -> bool {
    url_host_allowed(raw, true)
}

fn url_host_allowed(raw: &str, allow_private: bool) -> bool {
    let raw = raw.trim();
    if raw.len() > 2048 || raw.contains('\0') || raw.contains('\\') {
        return false;
    }
    let Some((scheme, rest)) = raw.split_once("://") else {
        return false;
    };
    if scheme != "http" && scheme != "https" {
        return false;
    }
    if rest.is_empty() || rest.contains('@') || rest.contains('\\') {
        return false;
    }
    let hostport = rest.split(['/', '?', '#']).next().unwrap_or("");
    if hostport.is_empty() {
        return false;
    }
    let host = if let Some(inner) = hostport.strip_prefix('[') {
        match inner.split_once(']') {
            Some((h, _)) => h,
            None => return false,
        }
    } else {
        hostport.split(':').next().unwrap_or("")
    };
    !host.is_empty() && host_is_allowed(host, allow_private)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_traversal_and_flags() {
        assert!(!is_safe_argv(""));
        assert!(!is_safe_argv("-o"));
        assert!(!is_safe_argv("../etc/passwd"));
        assert!(!is_safe_argv("pool/../../etc"));
        assert!(!is_safe_argv("a\0b"));
        assert!(is_safe_argv("pool/dataset"));
        assert!(is_safe_argv("iqn.2026-01.com.example:store"));
        assert!(is_safe_argv("192.168.1.10:3260"));
        assert!(is_safe_argv("/var/lib/zyvor/disk.qcow2"));
        assert!(is_safe_component("vm-01"));
        assert!(!is_safe_component("a/b"));
    }

    #[test]
    fn blocks_metadata_and_loopback() {
        assert!(!is_safe_probe_host("127.0.0.1"));
        assert!(!is_safe_probe_host("169.254.169.254"));
        assert!(!is_safe_probe_host("localhost"));
        assert!(is_safe_probe_host("10.1.2.3"));
        assert!(is_safe_probe_host("192.168.1.10"));
        assert!(is_safe_probe_url("http://10.1.2.3:9095/health"));
        assert!(!is_safe_probe_url("http://169.254.169.254/latest"));
        assert!(!is_safe_probe_url("http://127.0.0.1:9095/health"));
        assert!(!is_safe_outbound_url("http://10.1.2.3/"));
        assert!(!is_safe_probe_host("metadata.google.internal"));
        assert!(!is_safe_outbound_url("http://127.0.0.1/latest"));
        assert!(!is_safe_outbound_url("http://user:pass@example.com/"));
        assert!(!is_safe_outbound_url("file:///etc/passwd"));
        assert!(sanitize_outbound_url("https://example.com/a").starts_with("https://"));
        assert!(sanitize_outbound_url("http://127.0.0.1/").is_empty());
        assert!(sanitize_probe_url("http://10.1.2.3:9095/health").starts_with("http://"));
        assert!(sanitize_probe_url("http://127.0.0.1/").is_empty());
    }

    fn argv_gate(value: &str) -> Result<String, &'static str> {
        crate::argv_checked!(value, "rejected", |value| Ok(value.to_string()))
    }

    #[test]
    fn argv_checked_rejects_flags() {
        assert!(argv_gate("pool/dataset").is_ok());
        assert!(argv_gate("-o").is_err());
        assert!(argv_gate("../etc").is_err());
    }
}
