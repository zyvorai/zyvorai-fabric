// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: Apache-2.0

use crate::{guard::GuardConfig, notify::ApprovalWebhook, sentinel::SentinelConfig};
use anyhow::{Context, Result};
use std::{net::SocketAddr, path::PathBuf, time::Duration};

#[derive(Clone, Debug)]
pub struct Config {
    pub listen: SocketAddr,
    pub egress_listen: SocketAddr,
    /// HTTPS CONNECT proxy for browsers in the sandbox. `None` turns it off.
    pub proxy_listen: Option<SocketAddr>,
    /// Ports a CONNECT tunnel may target.
    pub proxy_connect_ports: Vec<u16>,
    /// Directory holding the interception CA (`ca.pem`, `ca.key`), created on first
    /// use. Absent turns TLS interception off. See `mitm`.
    pub mitm_ca_dir: Option<PathBuf>,
    /// Extra PEM root certificates the broker trusts for upstream TLS (private CAs).
    pub extra_ca_files: Vec<PathBuf>,
    pub state_dir: PathBuf,
    pub snapshot_dir: PathBuf,
    pub fluxvm_url: String,
    pub fluxvm_token: Option<String>,
    pub api_token: Option<String>,
    pub credentials_file: Option<PathBuf>,
    /// Operator-written fallback chains for the model step (see `model_gateway.rs`).
    pub model_chains_file: Option<PathBuf>,
    /// Operator-written prices and per-agent budgets for model calls (see `model_gateway::Prices`).
    pub model_prices_file: Option<PathBuf>,
    /// Seconds a use-case model reply may be reused for the same user, agent, endpoint and text. Off unless set.
    pub model_cache_secs: Option<u64>,
    /// JSON policy mapping an agent `skill_scope` to the skill scopes it may mount.
    pub skill_scopes_file: Option<PathBuf>,
    /// Reviewer model for `egress_mode: "sentinel"`. Absent means sentinel
    /// agents fall back to asking an operator.
    pub sentinel: Option<SentinelConfig>,
    /// Operator-run service that may refuse any brokered request. See `guard`.
    pub guard: Option<GuardConfig>,
    /// Where new approvals are pushed so a person sees them. See `notify`.
    pub approval_webhook: Option<ApprovalWebhook>,
    /// Ceilings for a manifest's `resources`. Absent means FluxVM's own limits decide.
    /// Force `confinement: strict` for every agent, whatever its manifest says.
    pub confine_all: bool,
    /// FluxVM Phase 6 security profile for sandboxes (default `measured` in Keep mode).
    pub security_profile: Option<String>,
    /// Dual-key break-glass for host recover on measured/standard only (Keep 0.2).
    /// Both must be set and presented; confidential profiles never accept recover.
    pub recover_key_a: Option<String>,
    pub recover_key_b: Option<String>,
    pub max_vcpus: Option<u8>,
    pub max_memory_mib: Option<u64>,
    pub egress_advertise_host: Option<String>,
    pub sync_interval_ms: u64,
    pub guest_start_timeout_secs: u64,
    /// Forget threads idle this many days (`ZYVOR_AGENT_THREAD_RETENTION_DAYS`); `None` keeps them until the user forgets them.
    pub thread_retention_days: Option<u64>,
    /// How often the goal worker looks at autorun goals (`ZYVOR_AGENT_GOAL_TICK_MS`, at least 200).
    pub goal_tick_ms: u64,
    /// The agent that proposes plans for goals (`ZYVOR_AGENT_PLANNER_AGENT`); a request may name another. `None`: the request must name one.
    pub planner_agent: Option<String>,
    /// The first retry delay of a failed goal step, doubled each time up to ten minutes (`ZYVOR_AGENT_GOAL_RETRY_BASE_SECS`).
    pub goal_retry_base_secs: u64,
    /// Forget action receipts this many days old (`ZYVOR_AGENT_RECEIPT_RETENTION_DAYS`); `None` keeps them. A forgotten receipt no longer
    /// answers a repeat of its idempotency key, so keep this longer than any agent retries.
    pub receipt_retention_days: Option<u64>,
    /// Forget suggestions (`ZYVOR_AGENT_SUGGESTION_RETENTION_DAYS`): undecided ones after this many days, decided ones this many days after the
    /// decision. `None` keeps them (the last 200 decided per person are kept regardless). A forgotten dismissal can be proposed again.
    pub suggestion_retention_days: Option<u64>,
    /// Drop memory proposals nobody reviewed after this many days (`ZYVOR_AGENT_MEMORY_PROPOSAL_RETENTION_DAYS`); `None` keeps them.
    pub memory_proposal_retention_days: Option<u64>,
    /// Delete the event log of sessions that ended this many days ago (`ZYVOR_AGENT_EVENT_RETENTION_DAYS`); `None` keeps it.
    pub event_retention_days: Option<u64>,
    /// 32-byte Ed25519 seed, hex (`ZYVOR_AGENT_RECORDING_KEY`). Off when unset: no session recording can be exported, because an
    /// unsigned recording would prove nothing. Held by the host only; the agent never sees it.
    pub recording_key: Option<[u8; 32]>,
    pub idle_scan_interval_ms: u64,
    pub warm_pool_reconcile_interval_ms: u64,
    pub warm_pool_max_create_per_tick: usize,
    pub warm_pool_claim_stale_secs: u64,
    pub expiry_scan_interval_ms: u64,
}

impl Config {
    pub fn from_env() -> Result<Self> {
        let api_token = env_opt("ZYVOR_AGENT_API_TOKEN");
        validate_auth(
            api_token.as_deref(),
            env_opt("ZYVOR_AGENT_ALLOW_NO_AUTH").as_deref(),
        )?;
        Ok(Self {
            listen: env_parse("ZYVOR_AGENT_LISTEN", "127.0.0.1:9096")?,
            egress_listen: env_parse("ZYVOR_AGENT_EGRESS_LISTEN", "0.0.0.0:18082")?,
            proxy_listen: proxy_listen_from_env()?,
            mitm_ca_dir: env_opt("ZYVOR_AGENT_MITM_CA_DIR").map(PathBuf::from),
            extra_ca_files: env_opt("ZYVOR_AGENT_EXTRA_CA_FILE")
                .map(|v| v.split(',').map(|p| PathBuf::from(p.trim())).collect())
                .unwrap_or_default(),
            proxy_connect_ports: env_or("ZYVOR_AGENT_PROXY_CONNECT_PORTS", "443")
                .split(',')
                .map(|p| {
                    p.trim()
                        .parse()
                        .context("invalid ZYVOR_AGENT_PROXY_CONNECT_PORTS")
                })
                .collect::<Result<_>>()?,
            state_dir: PathBuf::from(env_or(
                "ZYVOR_AGENT_STATE_DIR",
                "/var/lib/zyvor-fabric-agent",
            )),
            snapshot_dir: PathBuf::from(env_or(
                "ZYVOR_AGENT_SNAPSHOT_DIR",
                "/var/lib/fluxvm/agent-snapshots",
            )),
            fluxvm_url: env_or("ZYVOR_AGENT_FLUXVM_URL", "http://127.0.0.1:7788"),
            fluxvm_token: env_opt("ZYVOR_AGENT_FLUXVM_TOKEN"),
            api_token,
            credentials_file: env_opt("ZYVOR_AGENT_CREDENTIALS_FILE").map(PathBuf::from),
            model_chains_file: env_opt("ZYVOR_AGENT_MODEL_CHAINS_FILE").map(PathBuf::from),
            model_prices_file: env_opt("ZYVOR_AGENT_MODEL_PRICES_FILE").map(PathBuf::from),
            model_cache_secs: env_opt("ZYVOR_AGENT_MODEL_CACHE_SECS")
                .and_then(|v| v.trim().parse::<u64>().ok())
                .filter(|n| *n > 0)
                .map(|n| n.min(86_400)),
            skill_scopes_file: env_opt("ZYVOR_AGENT_SKILL_SCOPES_FILE").map(PathBuf::from),
            sentinel: sentinel_from_env()?,
            guard: guard_from_env()?,
            approval_webhook: approval_webhook_from_env()?,
            confine_all: env_opt("ZYVOR_AGENT_CONFINE").is_some_and(|v| v == "1"),
            security_profile: {
                let keep = env_compat("ZYVOR_AGENT_MERION_MODE", "ZYVOR_AGENT_KEEP_MODE")
                    .as_deref()
                    == Some("1");
                match env_opt("ZYVOR_AGENT_SECURITY_PROFILE") {
                    Some(s) if !s.is_empty() => Some(s),
                    _ if keep => Some("measured".into()),
                    _ => None,
                }
            },
            recover_key_a: env_opt("ZYVOR_AGENT_RECOVER_KEY_A").filter(|s| !s.is_empty()),
            recover_key_b: env_opt("ZYVOR_AGENT_RECOVER_KEY_B").filter(|s| !s.is_empty()),
            max_vcpus: env_opt("ZYVOR_AGENT_MAX_VCPUS")
                .map(|v| v.parse().context("invalid ZYVOR_AGENT_MAX_VCPUS"))
                .transpose()?,
            max_memory_mib: env_opt("ZYVOR_AGENT_MAX_MEMORY_MIB")
                .map(|v| v.parse().context("invalid ZYVOR_AGENT_MAX_MEMORY_MIB"))
                .transpose()?,
            egress_advertise_host: env_opt("ZYVOR_AGENT_EGRESS_ADVERTISE_HOST"),
            sync_interval_ms: env_parse("ZYVOR_AGENT_SYNC_INTERVAL_MS", "300")?,
            guest_start_timeout_secs: env_parse("ZYVOR_AGENT_GUEST_START_TIMEOUT_SECS", "30")?,
            thread_retention_days: env_days("ZYVOR_AGENT_THREAD_RETENTION_DAYS")?,
            goal_tick_ms: env_parse("ZYVOR_AGENT_GOAL_TICK_MS", "5000")?,
            goal_retry_base_secs: env_parse("ZYVOR_AGENT_GOAL_RETRY_BASE_SECS", "15")?,
            planner_agent: env_opt("ZYVOR_AGENT_PLANNER_AGENT"),
            event_retention_days: env_days("ZYVOR_AGENT_EVENT_RETENTION_DAYS")?,
            recording_key: recording_key_from_env()?,
            receipt_retention_days: env_days("ZYVOR_AGENT_RECEIPT_RETENTION_DAYS")?,
            memory_proposal_retention_days: env_days("ZYVOR_AGENT_MEMORY_PROPOSAL_RETENTION_DAYS")?,
            suggestion_retention_days: env_days("ZYVOR_AGENT_SUGGESTION_RETENTION_DAYS")?,
            idle_scan_interval_ms: env_parse("ZYVOR_AGENT_IDLE_SCAN_INTERVAL_MS", "1000")?,
            warm_pool_reconcile_interval_ms: env_parse(
                "ZYVOR_AGENT_WARM_POOL_RECONCILE_INTERVAL_MS",
                "2000",
            )?,
            warm_pool_max_create_per_tick: env_parse(
                "ZYVOR_AGENT_WARM_POOL_MAX_CREATE_PER_TICK",
                "2",
            )?,
            warm_pool_claim_stale_secs: env_parse("ZYVOR_AGENT_WARM_POOL_CLAIM_STALE_SECS", "300")?,
            expiry_scan_interval_ms: env_parse("ZYVOR_AGENT_EXPIRY_SCAN_INTERVAL_MS", "1000")?,
        })
    }
}

/// The public Agent Runtime API can deploy arbitrary agent bundles and use
/// any configured credential, so it refuses to start without a bearer token
/// unless the operator explicitly opts out (e.g. because something else in
/// front of this process, or the network it's bound to, already restricts
/// access).
fn validate_auth(api_token: Option<&str>, allow_no_auth: Option<&str>) -> Result<()> {
    if api_token.is_none() && allow_no_auth.is_none() {
        anyhow::bail!(
            "ZYVOR_AGENT_API_TOKEN is not set. Set it, or set \
             ZYVOR_AGENT_ALLOW_NO_AUTH=1 to explicitly opt into running without one."
        );
    }
    Ok(())
}

/// `ZYVOR_AGENT_PROXY_LISTEN`: an address, or `off` to disable the proxy.
fn proxy_listen_from_env() -> Result<Option<SocketAddr>> {
    let value = env_or("ZYVOR_AGENT_PROXY_LISTEN", "0.0.0.0:18083");
    if value.eq_ignore_ascii_case("off") {
        return Ok(None);
    }
    value
        .parse()
        .map(Some)
        .context("invalid ZYVOR_AGENT_PROXY_LISTEN")
}

fn approval_webhook_from_env() -> Result<Option<ApprovalWebhook>> {
    let Some(url) = env_opt("ZYVOR_AGENT_APPROVAL_WEBHOOK") else {
        return Ok(None);
    };
    let secret = env_opt("ZYVOR_AGENT_APPROVAL_WEBHOOK_SECRET").context(
        "ZYVOR_AGENT_APPROVAL_WEBHOOK_SECRET is required when ZYVOR_AGENT_APPROVAL_WEBHOOK is set",
    )?;
    Ok(Some(ApprovalWebhook { url, secret }))
}

fn sentinel_from_env() -> Result<Option<SentinelConfig>> {
    let Some(url) = env_opt("ZYVOR_AGENT_SENTINEL_URL") else {
        return Ok(None);
    };
    let model = env_opt("ZYVOR_AGENT_SENTINEL_MODEL")
        .context("ZYVOR_AGENT_SENTINEL_MODEL is required when ZYVOR_AGENT_SENTINEL_URL is set")?;
    Ok(Some(SentinelConfig {
        url,
        model,
        api_key: env_opt("ZYVOR_AGENT_SENTINEL_API_KEY"),
        timeout: Duration::from_secs(env_parse("ZYVOR_AGENT_SENTINEL_TIMEOUT_SECS", "15")?),
        can_allow: env_opt("ZYVOR_AGENT_SENTINEL_CAN_ALLOW").is_some_and(|v| v == "1"),
    }))
}

fn guard_from_env() -> Result<Option<GuardConfig>> {
    let Some(url) = env_opt("ZYVOR_AGENT_GUARD_URL") else {
        return Ok(None);
    };
    let secs: u64 = env_parse("ZYVOR_AGENT_GUARD_TIMEOUT_SECS", "3")?;
    if !(1..=60).contains(&secs) {
        anyhow::bail!("ZYVOR_AGENT_GUARD_TIMEOUT_SECS must be between 1 and 60");
    }
    Ok(Some(GuardConfig::new(
        &url,
        env_opt("ZYVOR_AGENT_GUARD_TOKEN"),
        Duration::from_secs(secs),
    )?))
}

fn env_or(name: &str, default: &str) -> String {
    std::env::var(name).unwrap_or_else(|_| default.to_string())
}

fn env_opt(name: &str) -> Option<String> {
    std::env::var(name).ok().filter(|v| !v.trim().is_empty())
}

/// A setting that was renamed: `new` wins, `old` is still read so existing env files and
/// units keep working. Using only the old name logs one warning per process start.
pub fn env_compat(new: &str, old: &str) -> Option<String> {
    if let Some(v) = env_opt(new) {
        return Some(v);
    }
    let v = env_opt(old)?;
    tracing::warn!("{old} is deprecated, set {new} instead");
    Some(v)
}

/// A retention period in days: unset or 0 means "keep", otherwise 1 to 3650.
fn env_days(name: &str) -> Result<Option<u64>> {
    parse_days(env_opt(name).as_deref()).with_context(|| format!("invalid {name}"))
}

pub(crate) fn parse_days(value: Option<&str>) -> Result<Option<u64>> {
    let Some(v) = value else { return Ok(None) };
    let days: u64 = v.trim().parse().context("not a whole number of days")?;
    match days {
        0 => Ok(None),
        1..=3650 => Ok(Some(days)),
        _ => anyhow::bail!("at most 3650 days"),
    }
}

fn env_parse<T>(name: &str, default: &str) -> Result<T>
where
    T: std::str::FromStr,
    T::Err: std::error::Error + Send + Sync + 'static,
{
    env_or(name, default)
        .parse::<T>()
        .with_context(|| format!("invalid {name}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_renamed_setting_prefers_the_new_name_and_falls_back_to_the_old() {
        // Names unique to this test: the process environment is shared between tests.
        let (new, old) = ("ZYVOR_TEST_COMPAT_NEW_A", "ZYVOR_TEST_COMPAT_OLD_A");
        assert_eq!(env_compat(new, old), None);
        std::env::set_var(old, "legacy");
        assert_eq!(env_compat(new, old).as_deref(), Some("legacy"));
        std::env::set_var(new, "current");
        assert_eq!(env_compat(new, old).as_deref(), Some("current"));
        std::env::set_var(new, "  ");
        assert_eq!(env_compat(new, old).as_deref(), Some("legacy"));
        std::env::remove_var(new);
        std::env::remove_var(old);
    }

    #[test]
    fn refuses_to_start_with_neither_token_nor_opt_out() {
        assert!(validate_auth(None, None).is_err());
    }

    #[test]
    fn accepts_an_explicit_api_token() {
        assert!(validate_auth(Some("secret"), None).is_ok());
    }

    #[test]
    fn accepts_an_explicit_no_auth_opt_out() {
        assert!(validate_auth(None, Some("1")).is_ok());
    }
}

/// `ZYVOR_AGENT_RECORDING_KEY`: 64 hex characters. A malformed value is an error, not a silent "off".
fn recording_key_from_env() -> Result<Option<[u8; 32]>> {
    let Some(v) = env_opt("ZYVOR_AGENT_RECORDING_KEY").filter(|s| !s.is_empty()) else {
        return Ok(None);
    };
    let bytes = hex::decode(v.trim()).context("ZYVOR_AGENT_RECORDING_KEY must be hex")?;
    let seed: [u8; 32] = bytes.try_into().map_err(|_| {
        anyhow::anyhow!("ZYVOR_AGENT_RECORDING_KEY must be 32 bytes (64 hex characters)")
    })?;
    Ok(Some(seed))
}
