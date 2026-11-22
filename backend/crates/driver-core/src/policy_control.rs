// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: LicenseRef-Zyvor-Production-2.0

//! Cilium-style per-VM packet-flow controls mapped onto FluxVM
//! `VmNetworkPolicy` (schema v4). Fabric does not write Cilium-private maps.
//!
//! | UX action | Policy effect |
//! |-----------|----------------|
//! | **Open**  | `default_allow=true`, `audit_mode=false` |
//! | **Audit** | evaluate policy, do not drop (`audit_mode=true`) |
//! | **Guard** | enforce default-deny (`default_allow=false`, `audit_mode=false`) |
//! | **Block** | add destination CIDR to `deny_cidrs` |
//! | **Allow** | add destination CIDR to `allow_cidrs` |
//! | **Invert**| swap allow/deny CIDR lists and flip `default_allow` |
//!
//! Also: explain, dry-run Guard, templates, drop-reason catalog,
//! flow filters, Guard timers, management lockout, policy fingerprint.

use crate::{FlowRecord, VmNetworkPolicy};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum EnforcementMode {
    Open,
    Audit,
    Guard,
}

impl EnforcementMode {
    pub fn from_policy(p: &VmNetworkPolicy) -> Self {
        if p.audit_mode {
            Self::Audit
        } else if p.default_allow {
            Self::Open
        } else {
            Self::Guard
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ControlAction {
    Open,
    Audit,
    Guard,
    Invert,
    Block,
    Allow,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ControlRequest {
    pub action: ControlAction,
    #[serde(default)]
    pub cidr: Option<String>,
    #[serde(default)]
    pub port: Option<String>,
    #[serde(default)]
    pub entity: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum DropReason {
    None,
    PolicyDenied,
    DefaultDeny,
    PortDenied,
    RateLimit,
    AuditWouldDrop,
    ManagementLockout,
    Unknown,
}

impl DropReason {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::None => "NONE",
            Self::PolicyDenied => "POLICY_DENIED",
            Self::DefaultDeny => "DEFAULT_DENY",
            Self::PortDenied => "PORT_DENIED",
            Self::RateLimit => "RATE_LIMIT",
            Self::AuditWouldDrop => "AUDIT_WOULD_DROP",
            Self::ManagementLockout => "MANAGEMENT_LOCKOUT",
            Self::Unknown => "UNKNOWN",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ExplainResult {
    pub verdict: String,
    pub reason: DropReason,
    pub would_drop: bool,
    pub matched_deny: Option<String>,
    pub matched_allow: Option<String>,
    pub matched_port: Option<String>,
    pub mode: EnforcementMode,
    pub summary: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DryRunHit {
    pub source: String,
    pub destination: String,
    pub protocol: String,
    pub destination_port: u16,
    pub current_verdict: String,
    pub dry_verdict: String,
    pub reason: DropReason,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DryRunReport {
    pub mode: EnforcementMode,
    pub would_drop: usize,
    pub examined: usize,
    pub hits: Vec<DryRunHit>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct GuardTimer {
    pub vm: String,
    pub revert: String,
    pub expires_unix: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct FlowFilter {
    #[serde(default)]
    pub verdict: Option<String>,
    #[serde(default)]
    pub protocol: Option<String>,
    #[serde(default)]
    pub dest_contains: Option<String>,
}

fn push_unique(list: &mut Vec<String>, value: String) {
    if !value.is_empty() && !list.iter().any(|x| x == &value) {
        list.push(value);
    }
}

pub fn host_cidr(addr: &str) -> String {
    let raw = addr.trim();
    let host = raw.split('/').next().unwrap_or(raw).trim();
    if host.contains(':') && !raw.contains('/') {
        format!("{host}/128")
    } else if host.contains('.') && !raw.contains('/') {
        format!("{host}/32")
    } else {
        raw.to_string()
    }
}

fn parse_ipv4(s: &str) -> Option<u32> {
    let p: Vec<&str> = s.split('.').collect();
    if p.len() != 4 {
        return None;
    }
    let mut out = 0u32;
    for x in p {
        let o: u32 = x.parse().ok()?;
        if o > 255 {
            return None;
        }
        out = (out << 8) | o;
    }
    Some(out)
}

pub fn cidr_contains(cidr: &str, ip: &str) -> bool {
    let ip = ip.split('/').next().unwrap_or(ip);
    if cidr.contains(':') || ip.contains(':') {
        let (net, plen) = match cidr.split_once('/') {
            Some((n, l)) => (n, l.parse::<u32>().unwrap_or(128)),
            None => (cidr, 128u32),
        };
        if plen >= 128 {
            return net == ip;
        }
        return cidr == ip || net == ip;
    }
    let (net, plen) = match cidr.split_once('/') {
        Some((n, l)) => (n, l.parse::<u32>().unwrap_or(32).min(32)),
        None => (cidr, 32u32),
    };
    let Some(n) = parse_ipv4(net) else {
        return cidr == ip;
    };
    let Some(a) = parse_ipv4(ip) else {
        return false;
    };
    if plen == 0 {
        return true;
    }
    let mask = if plen == 32 {
        u32::MAX
    } else {
        !((1u32 << (32 - plen)) - 1)
    };
    (n & mask) == (a & mask)
}

pub fn proto_name(p: u8) -> &'static str {
    match p {
        1 => "icmp",
        6 => "tcp",
        17 => "udp",
        58 => "icmpv6",
        _ => "any",
    }
}

/// Block of Fabric API / default GW should warn (not applied here — caller decides).
pub fn is_management_cidr(cidr: &str) -> bool {
    let h = cidr.split('/').next().unwrap_or(cidr);
    matches!(h, "127.0.0.1" | "::1" | "0.0.0.0") || h.starts_with("169.254.")
}

pub fn policy_fingerprint(p: &VmNetworkPolicy) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut s = std::collections::hash_map::DefaultHasher::new();
    p.default_allow.hash(&mut s);
    p.audit_mode.hash(&mut s);
    p.allow_cidrs.hash(&mut s);
    p.deny_cidrs.hash(&mut s);
    p.allow_ports.hash(&mut s);
    p.sample_rate.hash(&mut s);
    p.deny_udp.hash(&mut s);
    s.finish()
}

pub fn apply_mode(mut p: VmNetworkPolicy, mode: EnforcementMode) -> VmNetworkPolicy {
    match mode {
        EnforcementMode::Open => {
            p.default_allow = true;
            p.audit_mode = false;
        }
        EnforcementMode::Audit => {
            p.audit_mode = true;
            if p.sample_rate == 0 {
                p.sample_rate = 1;
            }
        }
        EnforcementMode::Guard => {
            p.default_allow = false;
            p.audit_mode = false;
            if p.sample_rate == 0 {
                p.sample_rate = 1;
            }
        }
    }
    p
}

pub fn invert_policy(mut p: VmNetworkPolicy) -> VmNetworkPolicy {
    std::mem::swap(&mut p.allow_cidrs, &mut p.deny_cidrs);
    p.default_allow = !p.default_allow;
    p
}

pub fn block_cidr(mut p: VmNetworkPolicy, cidr: &str) -> VmNetworkPolicy {
    let cidr = host_cidr(cidr);
    p.deny_cidrs.retain(|c| c != &cidr);
    push_unique(&mut p.deny_cidrs, cidr);
    p
}

pub fn allow_cidr(mut p: VmNetworkPolicy, cidr: &str) -> VmNetworkPolicy {
    let cidr = host_cidr(cidr);
    p.deny_cidrs.retain(|c| c != &cidr);
    push_unique(&mut p.allow_cidrs, cidr);
    p
}

pub fn allow_port(mut p: VmNetworkPolicy, port: &str) -> VmNetworkPolicy {
    push_unique(&mut p.allow_ports, port.trim().to_ascii_lowercase());
    p
}

pub fn allow_entity(mut p: VmNetworkPolicy, entity: &str) -> VmNetworkPolicy {
    push_unique(&mut p.entities, entity.trim().to_string());
    p
}

pub fn apply_control(
    policy: VmNetworkPolicy,
    req: &ControlRequest,
) -> Result<VmNetworkPolicy, String> {
    match req.action {
        ControlAction::Open => Ok(apply_mode(policy, EnforcementMode::Open)),
        ControlAction::Audit => Ok(apply_mode(policy, EnforcementMode::Audit)),
        ControlAction::Guard => Ok(apply_mode(policy, EnforcementMode::Guard)),
        ControlAction::Invert => Ok(invert_policy(policy)),
        ControlAction::Block => {
            let cidr = req
                .cidr
                .as_deref()
                .ok_or_else(|| "block requires cidr".to_string())?;
            let mut p = block_cidr(policy, cidr);
            if let Some(port) = req.port.as_deref() {
                p = allow_port(p, port);
            }
            Ok(p)
        }
        ControlAction::Allow => {
            let mut p = policy;
            if let Some(cidr) = req.cidr.as_deref() {
                p = allow_cidr(p, cidr);
            }
            if let Some(port) = req.port.as_deref() {
                p = allow_port(p, port);
            }
            if let Some(entity) = req.entity.as_deref() {
                p = allow_entity(p, entity);
            }
            if req.cidr.is_none() && req.port.is_none() && req.entity.is_none() {
                return Err("allow requires cidr, port, or entity".into());
            }
            Ok(p)
        }
    }
}

fn port_token(proto: &str, dport: u16) -> String {
    format!("{}/{}", proto.to_ascii_lowercase(), dport)
}

/// L3/L4 explain against declared VM-edge policy (not group-merged).
pub fn explain(
    policy: &VmNetworkPolicy,
    dest_ip: &str,
    dest_port: u16,
    proto: &str,
) -> ExplainResult {
    let mode = EnforcementMode::from_policy(policy);
    let proto = proto.to_ascii_lowercase();
    let token = port_token(&proto, dest_port);

    if is_management_cidr(dest_ip) && policy.deny_cidrs.iter().any(|c| cidr_contains(c, dest_ip)) {
        return ExplainResult {
            verdict: "DROPPED".into(),
            reason: DropReason::ManagementLockout,
            would_drop: true,
            matched_deny: Some(dest_ip.into()),
            matched_allow: None,
            matched_port: None,
            mode,
            summary: format!("DROPPED MANAGEMENT_LOCKOUT dest={dest_ip}"),
        };
    }

    let matched_deny = policy
        .deny_cidrs
        .iter()
        .find(|c| cidr_contains(c, dest_ip))
        .cloned();
    let matched_allow = policy
        .allow_cidrs
        .iter()
        .find(|c| cidr_contains(c, dest_ip))
        .cloned();
    let port_ok = policy.allow_ports.is_empty()
        || proto == "icmp"
        || proto == "any"
        || policy
            .allow_ports
            .iter()
            .any(|p| p.eq_ignore_ascii_case(&token));

    let (verdict, reason, would_drop) = if matched_deny.is_some() {
        ("DROPPED", DropReason::PolicyDenied, true)
    } else if !policy.allow_cidrs.is_empty() && matched_allow.is_none() && !policy.default_allow {
        ("DROPPED", DropReason::DefaultDeny, true)
    } else if !port_ok && !policy.default_allow {
        ("DROPPED", DropReason::PortDenied, true)
    } else if !policy.default_allow
        && policy.allow_cidrs.is_empty()
        && policy.allow_ports.is_empty()
    {
        ("DROPPED", DropReason::DefaultDeny, true)
    } else {
        ("FORWARDED", DropReason::None, false)
    };

    let (verdict, reason) = if policy.audit_mode && would_drop {
        ("AUDIT", DropReason::AuditWouldDrop)
    } else {
        (verdict, reason)
    };

    ExplainResult {
        verdict: verdict.into(),
        reason,
        would_drop,
        matched_deny,
        matched_allow,
        matched_port: if port_ok && !policy.allow_ports.is_empty() {
            Some(token)
        } else {
            None
        },
        mode,
        summary: format!(
            "{verdict} {reason} dest={dest_ip}:{dest_port}/{proto} mode={mode:?}",
            reason = reason.as_str()
        ),
    }
}

pub fn dry_run_guard(policy: &VmNetworkPolicy, flows: &[FlowRecord]) -> DryRunReport {
    let mut guarded = policy.clone();
    guarded.default_allow = false;
    guarded.audit_mode = false;
    let mut hits = Vec::new();
    for f in flows {
        let proto = proto_name(f.protocol);
        let r = explain(&guarded, &f.destination, f.destination_port, proto);
        if r.would_drop {
            hits.push(DryRunHit {
                source: format!("{}:{}", f.source, f.source_port),
                destination: format!("{}:{}", f.destination, f.destination_port),
                protocol: proto.into(),
                destination_port: f.destination_port,
                current_verdict: f.verdict.clone(),
                dry_verdict: r.verdict,
                reason: r.reason,
            });
        }
    }
    DryRunReport {
        mode: EnforcementMode::Guard,
        would_drop: hits.len(),
        examined: flows.len(),
        hits,
    }
}

pub fn filter_flows<'a>(flows: &'a [FlowRecord], f: &FlowFilter) -> Vec<&'a FlowRecord> {
    flows
        .iter()
        .filter(|rec| {
            if let Some(v) = &f.verdict {
                if v != "all" && !rec.verdict.eq_ignore_ascii_case(v) {
                    return false;
                }
            }
            if let Some(p) = &f.protocol {
                if p != "all" && proto_name(rec.protocol) != p.as_str() {
                    return false;
                }
            }
            if let Some(d) = &f.dest_contains {
                if !rec.destination.contains(d.as_str()) {
                    return false;
                }
            }
            true
        })
        .collect()
}

pub fn timer_expired(t: &GuardTimer, now: u64) -> bool {
    now >= t.expires_unix
}

pub fn templates() -> Vec<VmNetworkPolicy> {
    vec![
        template_by_id("open").unwrap(),
        template_by_id("guard").unwrap(),
    ]
}

pub fn template_by_id(id: &str) -> Option<VmNetworkPolicy> {
    Some(match id {
        "open" => VmNetworkPolicy {
            default_allow: true,
            sample_rate: 1,
            ..VmNetworkPolicy::default()
        },
        "guard" | "deny" => VmNetworkPolicy {
            default_allow: false,
            sample_rate: 1,
            ..VmNetworkPolicy::default()
        },
        "web" => VmNetworkPolicy {
            default_allow: false,
            allow_cidrs: vec!["0.0.0.0/0".into(), "::/0".into()],
            allow_ports: vec!["tcp/80".into(), "tcp/443".into(), "udp/53".into()],
            max_egress_mbps: Some(100),
            max_egress_pps: Some(10_000),
            sample_rate: 1,
            ..VmNetworkPolicy::default()
        },
        "dns-only" => VmNetworkPolicy {
            default_allow: false,
            allow_cidrs: vec!["0.0.0.0/0".into()],
            allow_ports: vec!["udp/53".into(), "tcp/53".into()],
            sample_rate: 1,
            ..VmNetworkPolicy::default()
        },
        "no-world" => VmNetworkPolicy {
            default_allow: false,
            deny_cidrs: vec!["0.0.0.0/0".into()],
            entities: vec!["host".into()],
            sample_rate: 1,
            ..VmNetworkPolicy::default()
        },
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn base() -> VmNetworkPolicy {
        VmNetworkPolicy {
            default_allow: true,
            allow_cidrs: vec!["10.0.0.0/8".into()],
            deny_cidrs: vec!["1.1.1.1/32".into()],
            ..VmNetworkPolicy::default()
        }
    }

    fn pol() -> VmNetworkPolicy {
        VmNetworkPolicy {
            default_allow: false,
            allow_cidrs: vec!["10.0.0.0/8".into()],
            deny_cidrs: vec!["10.66.0.0/16".into()],
            allow_ports: vec!["tcp/443".into()],
            ..VmNetworkPolicy::default()
        }
    }

    #[test]
    fn guard_is_default_deny_enforced() {
        let p = apply_mode(base(), EnforcementMode::Guard);
        assert!(!p.default_allow);
        assert!(!p.audit_mode);
        assert!(p.sample_rate >= 1);
        assert_eq!(EnforcementMode::from_policy(&p), EnforcementMode::Guard);
    }

    #[test]
    fn audit_keeps_lists_and_sets_flag() {
        let p = apply_mode(base(), EnforcementMode::Audit);
        assert!(p.audit_mode);
        assert_eq!(p.allow_cidrs, vec!["10.0.0.0/8"]);
    }

    #[test]
    fn invert_swaps_cidrs_and_default() {
        let p = invert_policy(base());
        assert!(!p.default_allow);
        assert_eq!(p.allow_cidrs, vec!["1.1.1.1/32"]);
        assert_eq!(p.deny_cidrs, vec!["10.0.0.0/8"]);
    }

    #[test]
    fn block_promotes_host_to_slash32() {
        let p = block_cidr(base(), "8.8.8.8");
        assert!(p.deny_cidrs.contains(&"8.8.8.8/32".into()));
    }

    #[test]
    fn apply_control_block_requires_cidr() {
        let err = apply_control(
            base(),
            &ControlRequest {
                action: ControlAction::Block,
                cidr: None,
                port: None,
                entity: None,
            },
        )
        .unwrap_err();
        assert!(err.contains("cidr"));
    }

    #[test]
    fn cidr_v4_prefix() {
        assert!(cidr_contains("10.0.0.0/8", "10.1.2.3"));
        assert!(!cidr_contains("10.0.0.0/8", "11.0.0.1"));
        assert!(cidr_contains("8.8.8.8/32", "8.8.8.8"));
        assert_eq!(host_cidr("8.8.8.8"), "8.8.8.8/32");
    }

    #[test]
    fn explain_deny_beats_allow() {
        let r = explain(&pol(), "10.66.1.1", 443, "tcp");
        assert!(r.would_drop);
        assert_eq!(r.reason, DropReason::PolicyDenied);
    }

    #[test]
    fn explain_port_denied() {
        let r = explain(&pol(), "10.1.1.1", 22, "tcp");
        assert_eq!(r.reason, DropReason::PortDenied);
    }

    #[test]
    fn explain_forward_https() {
        let r = explain(&pol(), "10.1.1.1", 443, "tcp");
        assert_eq!(r.verdict, "FORWARDED");
        assert!(!r.would_drop);
    }

    #[test]
    fn audit_would_drop() {
        let mut p = pol();
        p.audit_mode = true;
        let r = explain(&p, "1.1.1.1", 443, "tcp");
        assert_eq!(r.verdict, "AUDIT");
        assert_eq!(r.reason, DropReason::AuditWouldDrop);
        assert!(r.would_drop);
    }

    #[test]
    fn dry_run_lists_world() {
        let flows = vec![FlowRecord {
            identity: 1,
            family: 4,
            source: "10.0.0.2".into(),
            destination: "1.1.1.1".into(),
            source_port: 1,
            destination_port: 443,
            protocol: 6,
            verdict: "FORWARDED".into(),
            packets: 1,
            bytes: 1,
            last_seen_ns: 0,
        }];
        let open = VmNetworkPolicy {
            default_allow: true,
            ..VmNetworkPolicy::default()
        };
        let report = dry_run_guard(&open, &flows);
        assert_eq!(report.would_drop, 1);
        assert_eq!(report.hits[0].reason, DropReason::DefaultDeny);
    }

    #[test]
    fn invert_and_guard() {
        let p = apply_mode(pol(), EnforcementMode::Guard);
        assert!(!p.default_allow);
        let inv = invert_policy(pol());
        assert!(inv.default_allow);
        assert_eq!(inv.allow_cidrs, vec!["10.66.0.0/16"]);
    }

    #[test]
    fn management_lockout() {
        let p = VmNetworkPolicy {
            default_allow: true,
            deny_cidrs: vec!["127.0.0.1/32".into()],
            ..VmNetworkPolicy::default()
        };
        let r = explain(&p, "127.0.0.1", 443, "tcp");
        assert_eq!(r.reason, DropReason::ManagementLockout);
        assert!(r.would_drop);
        assert_eq!(DropReason::ManagementLockout.as_str(), "MANAGEMENT_LOCKOUT");
        assert!(is_management_cidr("169.254.1.1"));
        assert!(!is_management_cidr("10.0.0.1"));
    }

    #[test]
    fn timer_and_fingerprint() {
        let t = GuardTimer {
            vm: "web".into(),
            revert: "open".into(),
            expires_unix: 10,
        };
        assert!(timer_expired(&t, 11));
        assert!(!timer_expired(&t, 9));
        assert_ne!(policy_fingerprint(&pol()), 0);
    }

    #[test]
    fn fingerprint_changes_when_deny_udp_flips() {
        let off = pol();
        let mut on = off.clone();
        on.deny_udp = true;
        assert_ne!(policy_fingerprint(&off), policy_fingerprint(&on));
    }

    #[test]
    fn filter_flows_by_verdict() {
        let flows = vec![
            FlowRecord {
                identity: 1,
                family: 4,
                source: "10.0.0.2".into(),
                destination: "1.1.1.1".into(),
                source_port: 1,
                destination_port: 443,
                protocol: 6,
                verdict: "DROPPED".into(),
                packets: 1,
                bytes: 1,
                last_seen_ns: 0,
            },
            FlowRecord {
                identity: 1,
                family: 4,
                source: "10.0.0.2".into(),
                destination: "8.8.8.8".into(),
                source_port: 1,
                destination_port: 53,
                protocol: 17,
                verdict: "FORWARDED".into(),
                packets: 1,
                bytes: 1,
                last_seen_ns: 0,
            },
        ];
        let filtered = filter_flows(
            &flows,
            &FlowFilter {
                verdict: Some("DROPPED".into()),
                protocol: None,
                dest_contains: None,
            },
        );
        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered[0].destination, "1.1.1.1");
    }
}
