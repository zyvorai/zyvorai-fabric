// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: Apache-2.0

//! Network confinement: make the egress broker and CONNECT proxy the only way
//! out of a sandbox.
//!
//! FluxVM enforces this on the **host** (TC/eBPF on the sandbox veth) — never
//! in guest Chromium. With `deny_udp`, WebRTC/QUIC/STUN die at L4. Public DNS
//! and cloud metadata are deny-listed. Destination FQDNs from Keep policy are
//! passed as `allow_fqdns` for FluxVM to resolve into CIDR maps.

use anyhow::{bail, Result};
use serde_json::{json, Value};
use std::net::IpAddr;

/// The policy for a cell that needs **no IP networking at all**: a use-case run, which the host reaches over
/// vsock and which never calls out. Everything is denied, so nothing depends on finding a gateway inside the
/// guest (which is not there before the guest has booted, and may not be there at all).
pub fn deny_all_policy(session_id: Option<&str>, agent: Option<&str>) -> Value {
    let mut labels = vec![
        "merion.zyvor.dev/proxy=strict".to_string(),
        "merion.zyvor.dev/egress=none".to_string(),
    ];
    if let Some(sid) = session_id {
        labels.push(format!("merion.zyvor.dev/session={sid}"));
    }
    if let Some(a) = agent {
        labels.push(format!("merion.zyvor.dev/agent={a}"));
    }
    json!({
        "default_allow": false,
        "allow_cidrs": [],
        "allow_ports": [],
        "deny_udp": true,
        "allow_icmp": false,
        "deny_cidrs": [],
        "allow_fqdns": [],
        "labels": labels,
        "sample_rate": 0,
        "audit_mode": false,
    })
}

/// Build a FluxVM `VmNetworkPolicy` for a Keep / confined cell.
pub fn strict_policy(
    gateway: IpAddr,
    broker_port: u16,
    proxy_port: Option<u16>,
    allow_fqdns: &[String],
    session_id: Option<&str>,
    agent: Option<&str>,
) -> Value {
    let prefix = if gateway.is_ipv4() { 32 } else { 128 };
    let mut ports = vec![format!("tcp/{broker_port}")];
    if let Some(port) = proxy_port {
        ports.push(format!("tcp/{port}"));
    }
    let mut labels = vec![
        "merion.zyvor.dev/role=browser".to_string(),
        "merion.zyvor.dev/proxy=strict".to_string(),
    ];
    if let Some(sid) = session_id {
        labels.push(format!("merion.zyvor.dev/session={sid}"));
    }
    if let Some(a) = agent {
        labels.push(format!("merion.zyvor.dev/agent={a}"));
    }
    let mut fqdns: Vec<String> = allow_fqdns
        .iter()
        .map(|h| h.trim().trim_start_matches("*.").to_ascii_lowercase())
        .filter(|h| !h.is_empty())
        .collect();
    fqdns.sort();
    fqdns.dedup();

    json!({
        "default_allow": false,
        "allow_cidrs": [format!("{gateway}/{prefix}")],
        "allow_ports": ports,
        // Proxy-or-die: only TCP to broker/proxy. Explicit UDP kill for QUIC/WebRTC.
        "deny_udp": true,
        "allow_icmp": false,
        "deny_cidrs": [
            // Cloud metadata
            "169.254.169.254/32",
            "169.254.0.0/16",
            // Public recursive DNS / DoH edges guests love to hit
            "8.8.8.8/32",
            "8.8.4.4/32",
            "1.1.1.1/32",
            "1.0.0.1/32",
            "9.9.9.9/32",
        ],
        "allow_fqdns": fqdns,
        "labels": labels,
        "sample_rate": 0,
        "audit_mode": false,
    })
}

/// Confinement needs an IP address to pin to; a hostname would have to be
/// resolved by a guest that is not yet confined.
pub fn parse_gateway(host: &str) -> Result<IpAddr> {
    let trimmed = host.trim().trim_start_matches('[').trim_end_matches(']');
    match trimmed.parse() {
        Ok(ip) => Ok(ip),
        Err(_) => bail!(
            "confinement needs the egress gateway as an IP address, got {host:?}; \
             set ZYVOR_AGENT_EGRESS_ADVERTISE_HOST to an IP or use tap+netns"
        ),
    }
}

/// Applies a confinement step, trying again a couple of times before giving up.
///
/// Applying a policy is idempotent (it replaces the VM's policy), and on a busy FluxVM host the eBPF load
/// or map update occasionally fails once and works a moment later (seen as `bpftool prog load` and
/// `bpftool map update` errors, about 3 in 110 cell runs). A cell must never run unconfined, so after the
/// last attempt the caller still fails closed with the last error; this only removes the spurious refusals.
pub async fn with_retries<F, Fut>(
    attempts: u32,
    pause: std::time::Duration,
    mut apply: F,
) -> Result<()>
where
    F: FnMut() -> Fut,
    Fut: std::future::Future<Output = Result<()>>,
{
    let attempts = attempts.max(1);
    let mut last = None;
    for n in 1..=attempts {
        match apply().await {
            Ok(()) => return Ok(()),
            Err(e) => {
                if n < attempts {
                    tracing::warn!(attempt = n, of = attempts, error = %format!("{e:#}"), "confinement failed, retrying");
                    tokio::time::sleep(pause).await;
                }
                last = Some(e);
            }
        }
    }
    Err(last.expect("at least one attempt ran"))
}

#[cfg(test)]
mod tests {
    #[test]
    fn a_use_case_cell_is_denied_everything_and_needs_no_gateway() {
        let p = deny_all_policy(Some("sess-1"), Some("csv-clean"));
        assert_eq!(p["default_allow"], false);
        assert_eq!(p["allow_cidrs"], json!([]));
        assert_eq!(p["allow_ports"], json!([]));
        assert_eq!(p["allow_fqdns"], json!([]));
        assert_eq!(p["deny_udp"], true);
        assert_eq!(p["allow_icmp"], false);
        let labels: Vec<String> = serde_json::from_value(p["labels"].clone()).unwrap();
        assert!(labels.contains(&"merion.zyvor.dev/egress=none".to_string()));
        assert!(labels.contains(&"merion.zyvor.dev/session=sess-1".to_string()));
        // Nothing is allowed, unlike the broker-only policy an agent session gets.
        let strict = strict_policy("10.0.0.1".parse().unwrap(), 9097, None, &[], None, None);
        assert_ne!(p["allow_cidrs"], strict["allow_cidrs"]);
    }

    use super::*;
    use std::sync::atomic::{AtomicU32, Ordering};
    use std::time::Duration;

    #[tokio::test]
    async fn confinement_is_retried_and_succeeds_when_a_later_attempt_works() {
        let calls = AtomicU32::new(0);
        let r = with_retries(3, Duration::from_millis(1), || async {
            if calls.fetch_add(1, Ordering::SeqCst) < 2 {
                bail!("bpftool prog load failed")
            }
            Ok(())
        })
        .await;
        assert!(r.is_ok());
        assert_eq!(calls.load(Ordering::SeqCst), 3);
    }

    #[tokio::test]
    async fn confinement_still_fails_closed_after_the_last_attempt() {
        let calls = AtomicU32::new(0);
        let r = with_retries(3, Duration::from_millis(1), || async {
            calls.fetch_add(1, Ordering::SeqCst);
            bail!("bpftool map update failed")
        })
        .await;
        let e = r.unwrap_err();
        assert!(format!("{e:#}").contains("map update"), "{e:#}");
        assert_eq!(calls.load(Ordering::SeqCst), 3);
    }

    #[tokio::test]
    async fn a_step_that_works_first_time_is_not_repeated() {
        let calls = AtomicU32::new(0);
        with_retries(3, Duration::from_millis(1), || async {
            calls.fetch_add(1, Ordering::SeqCst);
            Ok(())
        })
        .await
        .unwrap();
        assert_eq!(calls.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn allows_only_the_gateway_on_the_broker_and_proxy_ports() {
        let policy = strict_policy(
            "10.0.2.1".parse().unwrap(),
            18082,
            Some(18083),
            &["example.com".into()],
            Some("sess"),
            Some("pdf-brief"),
        );
        assert_eq!(policy["default_allow"], false);
        assert_eq!(policy["deny_udp"], true);
        assert_eq!(policy["allow_cidrs"], json!(["10.0.2.1/32"]));
        assert_eq!(policy["allow_ports"], json!(["tcp/18082", "tcp/18083"]));
        assert!(policy["deny_cidrs"]
            .as_array()
            .unwrap()
            .iter()
            .any(|c| c == "169.254.169.254/32"));
        assert_eq!(policy["allow_fqdns"], json!(["example.com"]));
        assert!(policy["labels"]
            .as_array()
            .unwrap()
            .iter()
            .any(|l| l.as_str() == Some("merion.zyvor.dev/role=browser")));
    }

    #[test]
    fn proxy_port_is_left_out_when_the_proxy_is_off() {
        let policy = strict_policy("10.0.2.1".parse().unwrap(), 18082, None, &[], None, None);
        assert_eq!(policy["allow_ports"], json!(["tcp/18082"]));
    }

    #[test]
    fn ipv6_gateways_use_a_host_prefix() {
        let policy = strict_policy("fd00::1".parse().unwrap(), 18082, None, &[], None, None);
        assert_eq!(policy["allow_cidrs"], json!(["fd00::1/128"]));
    }

    #[test]
    fn gateway_must_be_an_ip() {
        assert_eq!(
            parse_gateway("10.0.2.1").unwrap(),
            "10.0.2.1".parse::<IpAddr>().unwrap()
        );
        assert!(parse_gateway("[fd00::1]").is_ok());
        assert!(parse_gateway("broker.internal")
            .unwrap_err()
            .to_string()
            .contains("IP address"));
    }
}
