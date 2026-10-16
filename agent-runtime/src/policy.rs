// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: Apache-2.0

//! Keep Sentinel policy: export/import `keep.policy.yaml` from an agent manifest.
//!
//! **Keep mode** (`ZYVOR_AGENT_KEEP_MODE=1`): trusted signers are required at
//! startup, and every PUT policy must carry a valid Ed25519 signature
//! (`X-Keep-Policy-Signature: <hex>`). Agent deploy (`POST /v1/agents`) uses the
//! same scheme over the exact JSON body (`X-Keep-Manifest-Signature`).
//! `ZYVOR_AGENT_POLICY_REQUIRE_SIGNATURE=0`
//! is ignored in Keep mode (fail-closed).
//!
//! Without Keep mode, when `ZYVOR_AGENT_POLICY_TRUSTED_SIGNERS` is set, PUT
//! policy requires a signature unless `ZYVOR_AGENT_POLICY_REQUIRE_SIGNATURE=0`.

use crate::model::{AgentManifest, BrowserPolicy, EgressMode, EgressRule, TaintPolicy};
use anyhow::{bail, Context, Result};
use ed25519_dalek::{Signature, Signer, SigningKey, Verifier, VerifyingKey};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct KeepPolicy {
    pub version: u32,
    #[serde(default = "default_deny")]
    pub default_egress: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub allow: Vec<KeepAllow>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub deny: Vec<KeepDeny>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub taint: Option<KeepTaint>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub browser: Option<KeepBrowser>,
}

fn default_deny() -> String {
    "deny".into()
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct KeepAllow {
    pub host: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub methods: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub action: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ask: Option<String>,
    /// JSON-RPC methods the body may call (see [`crate::model::EgressRule::rpc_methods`]).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub rpc_methods: Vec<String>,
    /// MCP tool names a `tools/call` may use.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub mcp_tools: Vec<String>,
    /// GraphQL operation types the body may run.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub graphql_operations: Vec<String>,
    /// Programs in the cell that may reach this host (see [`crate::binary_id`]).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub binaries: Vec<crate::model::BinaryRule>,
}

impl KeepAllow {
    /// True when the entry restricts what the request body may ask for.
    pub fn reads_body(&self) -> bool {
        !self.rpc_methods.is_empty()
            || !self.mcp_tools.is_empty()
            || !self.graphql_operations.is_empty()
    }

    /// True when the entry restricts something beyond the host itself, so it needs an egress rule.
    pub fn restricts_request(&self) -> bool {
        !self.methods.is_empty() || self.reads_body() || !self.binaries.is_empty()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct KeepDeny {
    pub host: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct KeepTaint {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub trusted_hosts: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub on_untrusted_page: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct KeepBrowser {
    #[serde(default = "keep_browser_enabled_default")]
    pub enabled: bool,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub allow_hosts: Vec<String>,
    #[serde(default = "keep_browser_true")]
    pub block_file_url: bool,
    #[serde(default = "keep_browser_max_tabs")]
    pub max_tabs: u32,
    #[serde(default = "keep_browser_true")]
    pub snapshot_only: bool,
    #[serde(default = "keep_browser_downloads")]
    pub downloads: String,
}

fn keep_browser_enabled_default() -> bool {
    true
}
fn keep_browser_true() -> bool {
    true
}
fn keep_browser_max_tabs() -> u32 {
    8
}
fn keep_browser_downloads() -> String {
    "deny".into()
}

impl KeepBrowser {
    pub fn to_manifest_policy(&self) -> BrowserPolicy {
        BrowserPolicy {
            enabled: self.enabled,
            allow_hosts: self.allow_hosts.clone(),
            high_risk_hosts: vec![],
            block_file_url: self.block_file_url,
            max_tabs: self.max_tabs,
            snapshot_only: self.snapshot_only,
            downloads: self.downloads.clone(),
        }
    }

    pub fn from_manifest_policy(p: &BrowserPolicy) -> Self {
        Self {
            enabled: p.enabled,
            allow_hosts: p.allow_hosts.clone(),
            block_file_url: p.block_file_url,
            max_tabs: p.max_tabs,
            snapshot_only: p.snapshot_only,
            downloads: p.downloads.clone(),
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct PolicyTrust {
    /// Hex-encoded 32-byte Ed25519 public keys.
    pub trusted_signers: Vec<[u8; 32]>,
    /// When true, unsigned policy is refused (requires non-empty signers).
    pub require_signature: bool,
    /// Keep mode: fail-closed; signers mandatory; REQUIRE_SIGNATURE=0 ignored.
    pub keep_mode: bool,
}

impl PolicyTrust {
    pub fn from_env() -> Result<Self> {
        let keep_mode = std::env::var("ZYVOR_AGENT_KEEP_MODE").ok().as_deref() == Some("1");
        let raw = std::env::var("ZYVOR_AGENT_POLICY_TRUSTED_SIGNERS").unwrap_or_default();
        let mut trusted_signers = Vec::new();
        for part in raw.split(',').map(str::trim).filter(|s| !s.is_empty()) {
            let bytes = hex::decode(part).context("invalid POLICY_TRUSTED_SIGNERS hex")?;
            let arr: [u8; 32] = bytes
                .try_into()
                .map_err(|_| anyhow::anyhow!("POLICY_TRUSTED_SIGNERS entry must be 32 bytes"))?;
            trusted_signers.push(arr);
        }
        if keep_mode && trusted_signers.is_empty() {
            bail!(
                "ZYVOR_AGENT_KEEP_MODE=1 requires ZYVOR_AGENT_POLICY_TRUSTED_SIGNERS \
                 (comma-separated hex Ed25519 public keys)"
            );
        }
        let require_signature = if keep_mode {
            true
        } else {
            match std::env::var("ZYVOR_AGENT_POLICY_REQUIRE_SIGNATURE")
                .ok()
                .as_deref()
            {
                Some("1") => true,
                Some("0") => false,
                _ => !trusted_signers.is_empty(),
            }
        };
        let trust = Self {
            trusted_signers,
            require_signature,
            keep_mode,
        };
        trust.validate_configuration()?;
        Ok(trust)
    }

    fn validate_configuration(&self) -> Result<()> {
        if self.keep_mode && (self.trusted_signers.is_empty() || !self.require_signature) {
            bail!("Keep mode requires trusted policy signers and signature enforcement");
        }
        Ok(())
    }

    /// Sign the exact JSON request body on direct deployments. The policy YAML
    /// covers only a subset of manifest fields, so signing its projection would
    /// leave security-sensitive fields writable without authorization.
    pub fn verify_deployment(&self, body: &[u8], signature_hex: Option<&str>) -> Result<()> {
        if !self.keep_mode {
            return Ok(());
        }
        self.verify_yaml(body, signature_hex)
            .context("Keep mode requires X-Keep-Manifest-Signature over the exact deployment JSON")
    }

    pub fn verify_yaml(&self, yaml: &[u8], signature_hex: Option<&str>) -> Result<()> {
        if self.keep_mode || self.require_signature {
            if self.trusted_signers.is_empty() {
                bail!("policy signature required but no trusted signers configured");
            }
            let Some(sig_hex) = signature_hex.filter(|s| !s.trim().is_empty()) else {
                bail!("policy signature required (X-Keep-Policy-Signature)");
            };
            return self.verify_signature(yaml, sig_hex);
        }
        if self.trusted_signers.is_empty() {
            return Ok(());
        }
        let Some(sig_hex) = signature_hex.filter(|s| !s.trim().is_empty()) else {
            return Ok(());
        };
        self.verify_signature(yaml, sig_hex)
    }

    fn verify_signature(&self, yaml: &[u8], sig_hex: &str) -> Result<()> {
        let sig_bytes = hex::decode(sig_hex.trim()).context("invalid policy signature hex")?;
        let signature = Signature::from_slice(&sig_bytes).context("malformed Ed25519 signature")?;
        for pk in &self.trusted_signers {
            let key = VerifyingKey::from_bytes(pk).context("invalid trusted signer public key")?;
            if key.verify(yaml, &signature).is_ok() {
                return Ok(());
            }
        }
        bail!("policy signature did not match any trusted signer");
    }
}

/// Sign policy YAML with a 32-byte seed (tests / `merionctl policy sign`).
pub fn sign_policy_yaml(yaml: &[u8], seed32: &[u8; 32]) -> String {
    let key = SigningKey::from_bytes(seed32);
    hex::encode(key.sign(yaml).to_bytes())
}

impl KeepPolicy {
    pub fn from_manifest(m: &AgentManifest) -> Self {
        let ask = match m.egress_mode {
            EgressMode::Ask | EgressMode::Sentinel => Some("always".into()),
            EgressMode::Deny => None,
        };
        let mut allow: Vec<KeepAllow> = m
            .egress_allow_hosts
            .iter()
            .map(|host| KeepAllow {
                host: host.clone(),
                ask: ask.clone(),
                ..Default::default()
            })
            .collect();
        for rule in &m.egress_rules {
            if let Some(existing) = allow.iter_mut().find(|a| a.host == rule.host) {
                existing.methods = rule.methods.clone();
                existing.rpc_methods = rule.rpc_methods.clone();
                existing.mcp_tools = rule.mcp_tools.clone();
                existing.graphql_operations = rule.graphql_operations.clone();
                existing.binaries = rule.binaries.clone();
            } else {
                allow.push(KeepAllow {
                    host: rule.host.clone(),
                    methods: rule.methods.clone(),
                    ask: ask.clone(),
                    rpc_methods: rule.rpc_methods.clone(),
                    mcp_tools: rule.mcp_tools.clone(),
                    graphql_operations: rule.graphql_operations.clone(),
                    binaries: rule.binaries.clone(),
                    ..Default::default()
                });
            }
        }
        let taint = m.taint.as_ref().map(|t| KeepTaint {
            trusted_hosts: t.trusted_hosts.clone(),
            on_untrusted_page: Some("block_egress_until_ask".into()),
        });
        let browser = m.browser.as_ref().map(KeepBrowser::from_manifest_policy);
        Self {
            version: 1,
            default_egress: "deny".into(),
            allow,
            deny: vec![],
            taint,
            browser,
        }
    }

    /// Canonical YAML for signing (stable key order via serde_yaml on sorted maps
    /// is not guaranteed for structs; we sign the exact bytes the API receives).
    pub fn to_yaml(&self) -> Result<String> {
        Ok(serde_yaml::to_string(self)?)
    }

    pub fn from_yaml(raw: &str) -> Result<Self> {
        let policy: Self = serde_yaml::from_str(raw)?;
        if policy.version != 1 {
            bail!("unsupported keep.policy.yaml version {}", policy.version);
        }
        if policy.default_egress != "deny" && policy.default_egress != "allow" {
            bail!("default_egress must be deny or allow");
        }
        Ok(policy)
    }

    /// Refuse policy fields the runtime does not enforce, so a signed intent is never silently dropped.
    ///
    /// `apply_to_manifest` has no place for `default_egress: allow` or for `deny[]`; egress is
    /// default-deny and only `allow[]` hosts are reachable. A `deny` host that no `allow` entry
    /// covers is already refused, so only an overlap needs rejecting.
    pub fn enforceable(&self) -> Result<()> {
        if self.default_egress != "deny" {
            bail!(
                "default_egress: {} is not enforced; egress is always default-deny",
                self.default_egress
            );
        }
        let bare = |h: &str| h.trim_start_matches("*.").to_ascii_lowercase();
        for d in &self.deny {
            let denied = bare(&d.host);
            let overlap = self.allow.iter().find(|a| {
                let allowed = bare(&a.host);
                host_matches_list(&allowed, std::slice::from_ref(&denied))
                    || host_matches_list(&denied, std::slice::from_ref(&allowed))
            });
            if let Some(a) = overlap {
                bail!(
                    "deny host {} overlaps allow host {}; deny is not enforced over allow, remove one",
                    d.host,
                    a.host
                );
            }
        }
        Ok(())
    }

    pub fn apply_to_manifest(&self, m: &mut AgentManifest) {
        m.egress_allow_hosts = self.allow.iter().map(|a| a.host.clone()).collect();
        m.egress_rules = self
            .allow
            .iter()
            .filter(|a| a.restricts_request())
            .map(|a| EgressRule {
                host: a.host.clone(),
                methods: a.methods.clone(),
                path_prefixes: vec![],
                max_body_bytes: None,
                rpc_methods: a.rpc_methods.clone(),
                mcp_tools: a.mcp_tools.clone(),
                graphql_operations: a.graphql_operations.clone(),
                binaries: a.binaries.clone(),
            })
            .collect();
        if self.allow.iter().any(|a| {
            matches!(
                a.ask.as_deref(),
                Some("always") | Some("first") | Some("ask")
            )
        }) {
            m.egress_mode = EgressMode::Ask;
        }
        if let Some(t) = &self.taint {
            m.taint = Some(TaintPolicy {
                trusted_hosts: t.trusted_hosts.clone(),
            });
        }
        if let Some(b) = &self.browser {
            let mut bp = b.to_manifest_policy();
            // purchase|send action keys on allow entries → high-risk browser open.
            for a in &self.allow {
                if matches!(a.action.as_deref(), Some("purchase") | Some("send"))
                    && !bp.high_risk_hosts.iter().any(|h| h == &a.host)
                {
                    bp.high_risk_hosts.push(a.host.clone());
                }
            }
            m.browser = Some(bp);
        }
    }
}

/// Host match for browser allow / high-risk lists (exact or parent suffix).
pub fn host_matches_list(host: &str, list: &[String]) -> bool {
    let host = host.trim_end_matches('.').to_ascii_lowercase();
    list.iter().any(|pat| {
        let p = pat.trim_end_matches('.').to_ascii_lowercase();
        host == p || host.ends_with(&format!(".{p}"))
    })
}

/// Pack metadata written by `merionctl pack` (no raw secrets).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KeepPackManifest {
    pub version: u32,
    pub agent: String,
    pub agent_version: String,
    pub packed_at: String,
    #[serde(default)]
    pub model_socket: Option<crate::model::ModelSocket>,
    #[serde(default)]
    pub cell_backend: Option<crate::model::CellBackend>,
    #[serde(default)]
    pub credential_names: Vec<String>,
    #[serde(default)]
    pub fluxvm_notes: BTreeMap<String, String>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::EgressMode;

    fn bare_manifest() -> AgentManifest {
        serde_json::from_value(serde_json::json!({
            "template": "agent-node",
            "egress_allow_hosts": ["api.github.com"],
            "egress_mode": "ask",
            "taint": { "trusted_hosts": ["api.github.com"] }
        }))
        .unwrap()
    }

    #[test]
    fn round_trip_yaml() {
        let m = bare_manifest();
        let yaml = KeepPolicy::from_manifest(&m).to_yaml().unwrap();
        assert!(yaml.contains("api.github.com"));
        let back = KeepPolicy::from_yaml(&yaml).unwrap();
        let mut m2 = bare_manifest();
        m2.egress_allow_hosts.clear();
        m2.egress_mode = EgressMode::Deny;
        m2.taint = None;
        back.apply_to_manifest(&mut m2);
        assert_eq!(m2.egress_allow_hosts, vec!["api.github.com".to_string()]);
        assert_eq!(m2.egress_mode, EgressMode::Ask);
    }

    #[test]
    fn browser_policy_round_trip() {
        let yaml = r#"
version: 1
default_egress: deny
allow:
  - host: example.com
browser:
  enabled: true
  allow_hosts: [example.com, github.com]
  block_file_url: true
  max_tabs: 4
  snapshot_only: true
  downloads: deny
"#;
        let p = KeepPolicy::from_yaml(yaml).unwrap();
        let b = p.browser.as_ref().unwrap();
        assert!(b.enabled);
        assert_eq!(b.max_tabs, 4);
        assert_eq!(b.allow_hosts, vec!["example.com", "github.com"]);
        let mut m = bare_manifest();
        p.apply_to_manifest(&mut m);
        assert_eq!(m.browser.as_ref().unwrap().max_tabs, 4);
        assert!(m.browser.as_ref().unwrap().block_file_url);
    }

    #[test]
    fn enforceable_rejects_unenforced_fields() {
        let ok = |y: &str| KeepPolicy::from_yaml(y).unwrap().enforceable();
        assert!(
            ok("version: 1\ndefault_egress: deny\nallow:\n  - { host: api.github.com }\n").is_ok()
        );
        assert!(ok("version: 1\ndefault_egress: allow\n").is_err());
        // deny with no overlapping allow is already refused by default-deny
        assert!(ok(
            "version: 1\nallow:\n  - { host: api.github.com }\ndeny:\n  - { host: evil.com }\n"
        )
        .is_ok());
        // exact and parent/child overlaps, either direction
        assert!(ok(
            "version: 1\nallow:\n  - { host: github.com }\ndeny:\n  - { host: github.com }\n"
        )
        .is_err());
        assert!(ok(
            "version: 1\nallow:\n  - { host: github.com }\ndeny:\n  - { host: gist.github.com }\n"
        )
        .is_err());
        assert!(ok(
            "version: 1\nallow:\n  - { host: gist.github.com }\ndeny:\n  - { host: github.com }\n"
        )
        .is_err());
        assert!(ok("version: 1\nallow:\n  - { host: \"*.github.com\" }\ndeny:\n  - { host: gist.github.com }\n").is_err());
        // a similar suffix that is not a subdomain does not overlap
        assert!(ok(
            "version: 1\nallow:\n  - { host: github.com }\ndeny:\n  - { host: notgithub.com }\n"
        )
        .is_ok());
    }

    #[test]
    fn purchase_send_actions_become_high_risk_hosts() {
        let yaml = r#"
version: 1
default_egress: deny
allow:
  - host: pay.example.com
    action: purchase
    ask: always
  - host: mail.example.com
    action: send
  - host: example.com
browser:
  enabled: true
  allow_hosts: [example.com, pay.example.com]
"#;
        let p = KeepPolicy::from_yaml(yaml).unwrap();
        let mut m = bare_manifest();
        p.apply_to_manifest(&mut m);
        let bp = m.browser.as_ref().unwrap();
        assert!(bp.high_risk_hosts.contains(&"pay.example.com".into()));
        assert!(bp.high_risk_hosts.contains(&"mail.example.com".into()));
        assert!(!bp.high_risk_hosts.contains(&"example.com".into()));
    }

    #[test]
    fn host_matches_list_suffix() {
        assert!(host_matches_list("api.github.com", &["github.com".into()]));
        assert!(!host_matches_list(
            "github.com.evil",
            &["github.com".into()]
        ));
    }

    #[test]
    fn signature_round_trip() {
        let seed = [7u8; 32];
        let yaml = b"version: 1\ndefault_egress: deny\n";
        let sig = sign_policy_yaml(yaml, &seed);
        let pk = SigningKey::from_bytes(&seed).verifying_key().to_bytes();
        let trust = PolicyTrust {
            trusted_signers: vec![pk],
            require_signature: true,
            keep_mode: false,
        };
        trust.verify_yaml(yaml, Some(&sig)).unwrap();
        assert!(trust.verify_yaml(yaml, Some("00")).is_err());
        assert!(trust.verify_yaml(yaml, None).is_err());
    }

    /// Ed25519 is deterministic, so a signature made by the Node SDK
    /// (`fabric-agent pack`, sdk/agent-runtime/src/sign.js) must equal the one
    /// `keep_sign_policy` makes for the same seed and bytes. This fixture was
    /// produced by the Node signer; if it ever drifts, Node-signed deploys would
    /// be rejected in Keep mode.
    #[test]
    fn node_sdk_signature_matches_the_rust_signer() {
        let seed = [7u8; 32];
        let yaml = b"version: 1\ndefault_egress: deny\n";
        assert_eq!(
            sign_policy_yaml(yaml, &seed),
            "5665d748703e5d6db9c7a2b7f9bf1623c33dc4d77767527ea4a4b530a01e36429a30f5a0b6998e3ca286cab2f1103f2f4a5540add46c04fa3d82686748492508"
        );
        let pk = SigningKey::from_bytes(&seed).verifying_key().to_bytes();
        assert_eq!(
            hex::encode(pk),
            "ea4a6c63e29c520abef5507b132ec5f9954776aebebe7b92421eea691446d22c"
        );
    }

    #[test]
    fn keep_mode_refuses_unsigned_even_if_require_flag_would_be_off() {
        let seed = [9u8; 32];
        let yaml = b"version: 1\ndefault_egress: deny\n";
        let sig = sign_policy_yaml(yaml, &seed);
        let pk = SigningKey::from_bytes(&seed).verifying_key().to_bytes();
        let trust = PolicyTrust {
            trusted_signers: vec![pk],
            require_signature: false, // would allow unsigned without keep_mode
            keep_mode: true,
        };
        assert!(trust.verify_yaml(yaml, None).is_err());
        trust.verify_yaml(yaml, Some(&sig)).unwrap();
    }

    #[test]
    fn keep_mode_refuses_when_no_signers() {
        let trust = PolicyTrust {
            trusted_signers: vec![],
            require_signature: true,
            keep_mode: true,
        };
        assert!(trust
            .verify_yaml(b"version: 1\n", Some("abcd"))
            .unwrap_err()
            .to_string()
            .contains("no trusted signers"));
    }

    #[test]
    fn without_keep_mode_empty_signers_accept_unsigned() {
        let trust = PolicyTrust {
            trusted_signers: vec![],
            require_signature: false,
            keep_mode: false,
        };
        trust.verify_yaml(b"version: 1\n", None).unwrap();
    }

    #[test]
    fn keep_deployment_signature_covers_full_request() {
        let seed = [9u8; 32];
        let trust = PolicyTrust {
            trusted_signers: vec![SigningKey::from_bytes(&seed).verifying_key().to_bytes()],
            require_signature: true,
            keep_mode: true,
        };
        let body = br#"{"name":"operator","manifest":{"allow_private_networks":false}}"#;
        let signature = sign_policy_yaml(body, &seed);
        trust.verify_deployment(body, Some(&signature)).unwrap();
        assert!(trust.verify_deployment(body, None).is_err());
        let changed = br#"{"name":"operator","manifest":{"allow_private_networks":true}}"#;
        assert!(trust.verify_deployment(changed, Some(&signature)).is_err());
    }

    #[test]
    fn keep_mode_refuses_missing_signer_or_disabled_enforcement() {
        let mut trust = PolicyTrust {
            keep_mode: true,
            ..Default::default()
        };
        assert!(trust.validate_configuration().is_err());
        trust.trusted_signers.push([1u8; 32]);
        assert!(trust.validate_configuration().is_err());
        trust.require_signature = true;
        trust.validate_configuration().unwrap();
    }

    #[test]
    fn body_rules_survive_the_policy_round_trip() {
        let yaml = "version: 1\nallow:\n  - { host: mcp.example.com, methods: [POST], rpc_methods: [tools/call, tools/list], mcp_tools: [search], ask: first }\n  - { host: graph.example.com, graphql_operations: [query] }\n";
        let policy = KeepPolicy::from_yaml(yaml).unwrap();
        let mut m = bare_manifest();
        policy.apply_to_manifest(&mut m);
        let mcp = m
            .egress_rules
            .iter()
            .find(|r| r.host == "mcp.example.com")
            .unwrap();
        assert_eq!(mcp.rpc_methods, vec!["tools/call", "tools/list"]);
        assert_eq!(mcp.mcp_tools, vec!["search"]);
        // A host with only a body rule (no methods) still gets a rule, so it is enforced.
        let graph = m
            .egress_rules
            .iter()
            .find(|r| r.host == "graph.example.com")
            .unwrap();
        assert_eq!(graph.graphql_operations, vec!["query"]);
        assert!(m.validate_egress_policy().is_ok());
        // And back out again unchanged.
        let back = KeepPolicy::from_manifest(&m);
        let allow = back
            .allow
            .iter()
            .find(|a| a.host == "mcp.example.com")
            .unwrap();
        assert_eq!(allow.mcp_tools, vec!["search"]);
        assert_eq!(allow.rpc_methods, vec!["tools/call", "tools/list"]);
    }

    #[test]
    fn invalid_body_rules_are_refused_at_deploy() {
        let mut m = bare_manifest();
        m.egress_rules = vec![EgressRule {
            host: "x.example.com".into(),
            graphql_operations: vec!["mutate".into()],
            ..Default::default()
        }];
        assert!(m
            .validate_egress_policy()
            .unwrap_err()
            .contains("graphql_operations"));
        m.egress_rules[0].graphql_operations.clear();
        m.egress_rules[0].mcp_tools = vec![String::new()];
        assert!(m.validate_egress_policy().is_err());
    }
}
