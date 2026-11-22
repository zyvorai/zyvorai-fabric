// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: LicenseRef-Zyvor-Production-2.0
//!
//! VIP allocator for AI endpoints.
//!
//! Address syntax matches Rivora's AddressPool
//! (`../rivora/internal/ipam`: CIDR, `start-end` range, or a single address).
//! `AvoidBuggyIPs` skips IPv4 addresses whose last octet is 0 or 255.
//! Prefixes larger than /16 are rejected here; Rivora keeps those sparse.
//! IPv6 pools stay on the Rivora controller.
//!
//! Allocations are persisted in `ai_vip_allocations` so a fabricd restart
//! does not hand the same address out twice. The in-process mutex is the
//! serialization point, same role as Rivora's allocator lock.

use std::collections::HashSet;
use std::net::Ipv4Addr;
use std::sync::Mutex;

use serde::{Deserialize, Serialize};

use crate::server::AppState;

use super::types::InferenceEndpoint;
use super::STORE_ENDPOINTS;

pub const STORE_VIPS: &str = "ai_vip_allocations";
/// Unpinned reservations older than this can be reclaimed. Zero means held.
pub const RESERVATION_TTL_SECS: i64 = 1800;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VipAllocation {
    pub vip: String,
    pub endpoint: String,
    pub pool: String,
    #[serde(default)]
    pub owner: String,
    #[serde(default)]
    pub created_unix: i64,
    /// `0` once the endpoint record exists. Until then, expiry reclaims a crash.
    #[serde(default)]
    pub expires_unix: i64,
}

fn ipam_lock() -> &'static Mutex<()> {
    static LOCK: Mutex<()> = Mutex::new(());
    &LOCK
}

/// Pool specs: `FLUXVM_AI_ADDRESS_POOL` (comma-separated Rivora addresses),
/// else `FLUXVM_AI_SERVICE_CIDR`, else `10.96.0.0/16`.
pub fn pool_specs() -> Vec<String> {
    if let Ok(raw) = std::env::var("FLUXVM_AI_ADDRESS_POOL") {
        let specs: Vec<String> = raw
            .split(',')
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect();
        if !specs.is_empty() {
            return specs;
        }
    }
    vec![std::env::var("FLUXVM_AI_SERVICE_CIDR").unwrap_or_else(|_| "10.96.0.0/16".into())]
}

pub fn avoid_buggy_ips() -> bool {
    !matches!(
        std::env::var("FLUXVM_AI_AVOID_BUGGY_IPS").as_deref(),
        Ok(v) if v == "0" || v.eq_ignore_ascii_case("false")
    )
}

/// First free address in `specs` that is not in `used`.
pub fn next_free(
    used: &HashSet<String>,
    specs: &[String],
    avoid_buggy: bool,
) -> Result<String, String> {
    if specs.is_empty() {
        return Err("no AI address pool configured".into());
    }
    for spec in specs {
        if let Some(ip) = first_free_spec(spec, used, avoid_buggy)? {
            return Ok(ip);
        }
    }
    Err(format!("AI address pool exhausted ({})", specs.join(",")))
}

/// Reserve a VIP for `endpoint`. Idempotent when this endpoint already holds one.
/// A lost create-new race retries the next free address.
pub fn reserve(state: &AppState, endpoint: &str) -> Result<String, String> {
    let _guard = ipam_lock().lock().unwrap_or_else(|e| e.into_inner());
    let mut blocked = HashSet::new();
    for _ in 0..64 {
        let all: Vec<VipAllocation> = state.store.list_entities(STORE_VIPS).unwrap_or_default();
        if let Some(existing) = all.iter().find(|a| a.endpoint == endpoint) {
            return Ok(existing.vip.clone());
        }
        let mut used: HashSet<String> = all.iter().map(|a| a.vip.clone()).collect();
        if let Ok(unreadable) = state.store.list_unreadable_entities(STORE_VIPS) {
            used.extend(unreadable);
        }
        used.extend(blocked.iter().cloned());
        let specs = pool_specs();
        let vip = next_free(&used, &specs, avoid_buggy_ips())?;
        match save_allocation(state, &vip, endpoint) {
            Ok(()) => return Ok(vip),
            Err(e) if e.starts_with("conflict:") => {
                if !reclaim_stale_vip(state, &vip) {
                    blocked.insert(vip);
                }
                continue;
            }
            Err(e) => return Err(e),
        }
    }
    Err("could not reserve a VIP".into())
}

/// Record an explicit VIP (endpoint.spec.vip or a VIP already on the record).
/// Refuses an address owned by a different endpoint, matching Rivora `Allocate` pinned IPs.
pub fn adopt(state: &AppState, endpoint: &str, vip: &str) -> Result<String, String> {
    let _guard = ipam_lock().lock().unwrap_or_else(|e| e.into_inner());
    let all: Vec<VipAllocation> = state.store.list_entities(STORE_VIPS).unwrap_or_default();
    if let Some(existing) = all.iter().find(|a| a.endpoint == endpoint) {
        return Ok(existing.vip.clone());
    }
    if let Some(owner) = all.iter().find(|a| a.vip == vip) {
        return Err(format!(
            "requested address {vip} is already assigned to {}",
            owner.endpoint
        ));
    }
    if !address_in_pool(vip, &pool_specs(), avoid_buggy_ips())? {
        return Err(format!(
            "requested address {vip} is not in any configured pool"
        ));
    }
    save_allocation(state, vip, endpoint)?;
    Ok(vip.to_string())
}

pub fn release(state: &AppState, endpoint: &str) {
    let _guard = ipam_lock().lock().unwrap_or_else(|e| e.into_inner());
    let all: Vec<VipAllocation> = state.store.list_entities(STORE_VIPS).unwrap_or_default();
    for rec in all.into_iter().filter(|a| a.endpoint == endpoint) {
        let _ = state.store.delete_entity(STORE_VIPS, &rec.vip);
    }
}

/// Mark an allocation permanent once the endpoint record is stored.
pub fn pin(state: &AppState, endpoint: &str) {
    let _guard = ipam_lock().lock().unwrap_or_else(|e| e.into_inner());
    let all: Vec<VipAllocation> = state.store.list_entities(STORE_VIPS).unwrap_or_default();
    for mut rec in all.into_iter().filter(|a| a.endpoint == endpoint) {
        rec.expires_unix = 0;
        let _ = state.store.save_entity(STORE_VIPS, &rec.vip, &rec);
    }
}

/// A reservation that nobody holds and whose lease has expired can be taken.
pub fn reservation_reclaimable(expires_unix: i64, held: bool, now: i64) -> bool {
    !held && expires_unix > 0 && now >= expires_unix
}

fn save_allocation(state: &AppState, vip: &str, endpoint: &str) -> Result<(), String> {
    let now = chrono::Utc::now().timestamp();
    let rec = VipAllocation {
        vip: vip.to_string(),
        endpoint: endpoint.to_string(),
        pool: "default".into(),
        owner: format!("fabricd-{}", std::process::id()),
        created_unix: now,
        expires_unix: now.saturating_add(RESERVATION_TTL_SECS),
    };
    match state.store.try_create_entity(STORE_VIPS, vip, &rec) {
        Ok(()) => Ok(()),
        Err(e) if state_store::is_entity_conflict(&e) => {
            Err(format!("conflict: address {vip} is already reserved"))
        }
        Err(e) => Err(e.to_string()),
    }
}

fn reclaim_stale_vip(state: &AppState, vip: &str) -> bool {
    let Ok(Some(rec)) = state.store.get_entity::<VipAllocation>(STORE_VIPS, vip) else {
        return false;
    };
    let held = state
        .store
        .get_entity::<InferenceEndpoint>(STORE_ENDPOINTS, &rec.endpoint)
        .ok()
        .flatten()
        .is_some();
    if reservation_reclaimable(rec.expires_unix, held, chrono::Utc::now().timestamp()) {
        return state.store.delete_entity(STORE_VIPS, vip).is_ok();
    }
    false
}

fn address_in_pool(vip: &str, specs: &[String], avoid_buggy: bool) -> Result<bool, String> {
    let want: Ipv4Addr = vip
        .parse()
        .map_err(|_| format!("requested address {vip} is not IPv4"))?;
    if avoid_buggy && is_buggy_ipv4(want) {
        return Ok(false);
    }
    for spec in specs {
        if spec_contains(spec, want)? {
            return Ok(true);
        }
    }
    Ok(false)
}

fn first_free_spec(
    spec: &str,
    used: &HashSet<String>,
    avoid_buggy: bool,
) -> Result<Option<String>, String> {
    if spec.contains('/') {
        return first_free_cidr(spec, used, avoid_buggy);
    }
    if spec.contains('-') {
        for addr in expand_range(spec)? {
            if avoid_buggy && is_buggy_ipv4(addr) {
                continue;
            }
            let s = addr.to_string();
            if !used.contains(&s) {
                return Ok(Some(s));
            }
        }
        return Ok(None);
    }
    let addr: Ipv4Addr = spec
        .parse()
        .map_err(|_| format!("address '{spec}' is not an IPv4 address, CIDR, or range"))?;
    if avoid_buggy && is_buggy_ipv4(addr) {
        return Ok(None);
    }
    let s = addr.to_string();
    if used.contains(&s) {
        Ok(None)
    } else {
        Ok(Some(s))
    }
}

fn first_free_cidr(
    spec: &str,
    used: &HashSet<String>,
    avoid_buggy: bool,
) -> Result<Option<String>, String> {
    let (network, prefix) = parse_cidr(spec)?;
    let host_bits = 32 - prefix;
    if host_bits > 16 {
        return Err(format!(
            "address pool '{spec}' is larger than /16; Rivora allocates those sparsely, this allocator does not"
        ));
    }
    let size = 1u64 << host_bits;
    let base = u32::from(network);
    // /30 and wider have a network and broadcast address. /31 and /32 do not.
    let skip_ends = prefix <= 30;
    for i in 0..size {
        if skip_ends && (i == 0 || i + 1 == size) {
            continue;
        }
        let addr = Ipv4Addr::from(base.wrapping_add(i as u32));
        if avoid_buggy && is_buggy_ipv4(addr) {
            continue;
        }
        let s = addr.to_string();
        if !used.contains(&s) {
            return Ok(Some(s));
        }
    }
    Ok(None)
}

fn spec_contains(spec: &str, want: Ipv4Addr) -> Result<bool, String> {
    if spec.contains('/') {
        let (network, prefix) = parse_cidr(spec)?;
        let host_bits = 32 - prefix;
        if host_bits > 16 {
            return Err(format!("address pool '{spec}' is larger than /16"));
        }
        let mask = if prefix == 0 {
            0
        } else {
            u32::MAX << (32 - prefix)
        };
        if u32::from(want) & mask != u32::from(network) {
            return Ok(false);
        }
        return Ok(!is_network_or_broadcast(want, network, prefix));
    }
    if spec.contains('-') {
        return Ok(expand_range(spec)?.into_iter().any(|a| a == want));
    }
    let addr: Ipv4Addr = spec
        .parse()
        .map_err(|_| format!("address '{spec}' is not an IPv4 address, CIDR, or range"))?;
    Ok(addr == want)
}

fn parse_cidr(spec: &str) -> Result<(Ipv4Addr, u32), String> {
    let (addr, pref) = spec
        .split_once('/')
        .ok_or_else(|| format!("invalid CIDR '{spec}'"))?;
    let ip: Ipv4Addr = addr.parse().map_err(|_| {
        format!("address '{spec}' is not an IPv4 CIDR (IPv6 pools stay on the Rivora controller)")
    })?;
    let prefix: u32 = pref
        .parse()
        .map_err(|_| format!("invalid CIDR prefix '{pref}'"))?;
    if prefix > 32 {
        return Err(format!("invalid CIDR prefix '{pref}'"));
    }
    let mask = if prefix == 0 {
        0
    } else {
        u32::MAX << (32 - prefix)
    };
    let network = Ipv4Addr::from(u32::from(ip) & mask);
    Ok((network, prefix))
}

fn expand_range(spec: &str) -> Result<Vec<Ipv4Addr>, String> {
    let (start_s, end_s) = spec
        .split_once('-')
        .ok_or_else(|| format!("invalid range '{spec}'"))?;
    let start: Ipv4Addr = start_s
        .trim()
        .parse()
        .map_err(|_| format!("invalid range start in '{spec}'"))?;
    let end: Ipv4Addr = end_s
        .trim()
        .parse()
        .map_err(|_| format!("invalid range end in '{spec}'"))?;
    if u32::from(start) > u32::from(end) {
        return Err(format!("range start > end in '{spec}'"));
    }
    let mut out = Vec::new();
    let mut cur = u32::from(start);
    let last = u32::from(end);
    loop {
        out.push(Ipv4Addr::from(cur));
        if out.len() > (1 << 16) {
            return Err(format!("range '{spec}' is too large"));
        }
        if cur == last {
            break;
        }
        cur = cur.wrapping_add(1);
    }
    Ok(out)
}

fn is_buggy_ipv4(addr: Ipv4Addr) -> bool {
    let o = addr.octets();
    o[3] == 0 || o[3] == 255
}

fn is_network_or_broadcast(addr: Ipv4Addr, network: Ipv4Addr, prefix: u32) -> bool {
    if prefix > 30 {
        return false;
    }
    let host_bits = 32 - prefix;
    let size = 1u32 << host_bits;
    let base = u32::from(network);
    let n = u32::from(addr);
    n == base || n == base.wrapping_add(size.wrapping_sub(1))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn expired_unheld_reservation_can_be_reclaimed() {
        assert!(!reservation_reclaimable(0, false, 100));
        assert!(!reservation_reclaimable(200, true, 300));
        assert!(!reservation_reclaimable(200, false, 100));
        assert!(reservation_reclaimable(200, false, 200));
    }

    #[test]
    fn does_not_reuse_and_release_frees() {
        let specs = vec!["10.96.0.10-10.96.0.11".to_string()];
        let mut used = HashSet::new();
        let a = next_free(&used, &specs, true).unwrap();
        used.insert(a.clone());
        let b = next_free(&used, &specs, true).unwrap();
        assert_ne!(a, b);
        used.insert(b);
        assert!(next_free(&used, &specs, true).is_err());
        used.remove(&a);
        assert_eq!(next_free(&used, &specs, true).unwrap(), a);
    }

    #[test]
    fn skips_buggy_ipv4_like_rivora() {
        let specs = vec!["10.96.0.0/30".to_string()];
        let used = HashSet::new();
        assert_eq!(next_free(&used, &specs, true).unwrap(), "10.96.0.1");
    }

    #[test]
    fn cidr_skips_network_and_broadcast() {
        let specs = vec!["10.96.0.0/30".to_string()];
        let mut used = HashSet::new();
        let a = next_free(&used, &specs, false).unwrap();
        assert_eq!(a, "10.96.0.1");
        used.insert(a);
        let b = next_free(&used, &specs, false).unwrap();
        assert_eq!(b, "10.96.0.2");
        used.insert(b);
        assert!(next_free(&used, &specs, false).is_err());
    }
}
