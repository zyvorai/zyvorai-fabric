// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: Apache-2.0

//! Keep Browser 0.3 — measured appliance helpers.
//!
//! Split-sight pause, origin taint lattice (IFC), trajectory-as-code, and the
//! honesty / network-identity badge. The model never sees DOM, JS, or passwords.

use crate::model::AgentPausedReason;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet};
use uuid::Uuid;

/// Per-session browse IFC + trajectory (durable on [`crate::model::SessionRecord`]).
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
pub struct BrowseState {
    /// Tab id → origin set (hosts read into that tab).
    #[serde(default)]
    pub tabs: BTreeMap<String, OriginSet>,
    /// Active tab id (guest driver id).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub active_tab: Option<String>,
    /// Clipboard / paste buffer origins.
    #[serde(default)]
    pub clipboard: OriginSet,
    /// Model-context citations (artifact origins allowed into the prompt).
    #[serde(default)]
    pub model_context: OriginSet,
    /// Ordered successful browse acts for trajectory-as-code.
    #[serde(default)]
    pub steps: Vec<BrowseStep>,
    /// Bound Keep goal (goal-bound tabs).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub goal_id: Option<Uuid>,
    /// Fabric network identity label for the renderer (SNI / dataplane).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub network_identity: Option<String>,
    /// Dead-man counters (wire limits, soft).
    #[serde(default)]
    pub limits: BrowseLimits,
    /// Dual cookie jar: agent vs operator (structural split).
    #[serde(default)]
    pub cookie_jar: CookieJarKind,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum CookieJarKind {
    #[default]
    Agent,
    Operator,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
pub struct BrowseLimits {
    #[serde(default)]
    pub origins_seen: BTreeSet<String>,
    #[serde(default)]
    pub checkout_asks: u32,
    #[serde(default)]
    pub taint_events: u32,
    #[serde(default)]
    pub max_origins_per_hour: u32,
    #[serde(default)]
    pub max_checkout_asks: u32,
}

impl BrowseLimits {
    pub fn with_defaults() -> Self {
        Self {
            max_origins_per_hour: 32,
            max_checkout_asks: 8,
            ..Default::default()
        }
    }
}

/// Set of origin hosts (IFC label).
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
pub struct OriginSet {
    #[serde(default)]
    pub hosts: BTreeSet<String>,
}

impl OriginSet {
    pub fn singleton(host: impl Into<String>) -> Self {
        let mut hosts = BTreeSet::new();
        hosts.insert(normalize_host(&host.into()));
        Self { hosts }
    }

    pub fn is_empty(&self) -> bool {
        self.hosts.is_empty()
    }

    pub fn union(&self, other: &Self) -> Self {
        Self {
            hosts: self.hosts.union(&other.hosts).cloned().collect(),
        }
    }

    pub fn is_subset_of(&self, other: &Self) -> bool {
        self.hosts.is_subset(&other.hosts)
    }

    pub fn intersects(&self, other: &Self) -> bool {
        !self.hosts.is_disjoint(&other.hosts)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct BrowseStep {
    pub seq: u64,
    pub tool: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub op: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ref_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub role: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub snapshot_hash: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub screenshot_hash: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub policy_hash: Option<String>,
    pub at: String,
}

/// IFC verdict for moving data between labeled objects.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum IfcVerdict {
    Allow,
    Deny(&'static str),
    Ask(&'static str),
}

/// Paste/upload from `src` into a tab labeled `dst`.
pub fn check_cross_origin_flow(src: &OriginSet, dst: &OriginSet) -> IfcVerdict {
    if src.is_empty() || src.is_subset_of(dst) {
        return IfcVerdict::Allow;
    }
    if src.intersects(dst) {
        return IfcVerdict::Ask("cross-origin exfil (overlapping labels)");
    }
    IfcVerdict::Deny("clipboard/paste across disjoint origins requires kind: send")
}

/// Cite an artifact into model context only if its origins ⊆ goal allow set.
pub fn check_cite_to_model(artifact: &OriginSet, goal_allow: &OriginSet) -> IfcVerdict {
    if artifact.is_empty() || artifact.is_subset_of(goal_allow) {
        IfcVerdict::Allow
    } else {
        IfcVerdict::Deny("artifact origin not ⊆ goal.allow_hosts")
    }
}

pub fn normalize_host(host: &str) -> String {
    host.trim_end_matches('.').trim().to_ascii_lowercase()
}

pub fn host_from_url(url: &str) -> Option<String> {
    url::Url::parse(url)
        .ok()
        .and_then(|u| u.host_str().map(normalize_host))
}

/// Fabric network identity for the Chromium renderer (dataplane / PacketWolf).
pub fn browser_network_identity(tenant: &str, session_id: Uuid) -> String {
    format!("merion-browser/{tenant}/{session_id}")
}

/// Honesty / capability badge — one JSON object for cockpit + marketing.
pub fn honesty_badge(
    evidence_class: &str,
    operator_can_read: bool,
    host_recover: &str,
    confined: bool,
    paused: Option<&AgentPausedReason>,
    network_identity: Option<&str>,
) -> Value {
    json!({
        "evidence": evidence_class,
        "operator_can_read": operator_can_read,
        "host_recover": host_recover,
        "browser": "a11y-only",
        "proxy": if confined { "strict" } else { "open" },
        "agent_paused_reason": paused.map(|p| p.as_str()),
        "network_identity": network_identity,
        "honesty": if evidence_class == "software-test" {
            "Host can still see the guest until Keep 0.2 hardware (SNP/TDX)."
        } else {
            "Launch-verified confidential cell; CDP listing still host-visible until attested guest channel."
        },
    })
}

/// Render a replayable Playwright-core script from trajectory steps (roles/names, not CSS).
pub fn render_browse_script(session_id: Uuid, steps: &[BrowseStep]) -> String {
    let mut out = String::new();
    out.push_str("// Generated by Keep trajectory-as-code — replay without a model.\n");
    out.push_str(&format!("// session={session_id}\n"));
    out.push_str("import { chromium } from 'playwright-core';\n\n");
    out.push_str("const CDP = process.env.ZYVOR_BROWSER_CDP || 'http://127.0.0.1:9222';\n");
    out.push_str("const headed = process.env.HEADED === '1';\n\n");
    out.push_str("async function main() {\n");
    out.push_str("  const browser = await chromium.connectOverCDP(CDP).catch(async () =>\n");
    out.push_str("    chromium.launch({ headless: !headed, args: ['--no-sandbox'] }));\n");
    out.push_str("  const context = browser.contexts()[0] || await browser.newContext();\n");
    out.push_str("  const page = context.pages()[0] || await context.newPage();\n");
    for step in steps {
        out.push_str(&format!(
            "  // seq {} tool={} at {}\n",
            step.seq, step.tool, step.at
        ));
        match step.tool.as_str() {
            "open" => {
                if let Some(url) = &step.url {
                    out.push_str(&format!(
                        "  await page.goto({});\n",
                        serde_json::to_string(url).unwrap_or_else(|_| "\"\"".into())
                    ));
                }
            }
            "act" => {
                let op = step.op.as_deref().unwrap_or("click");
                let role = step.role.as_deref().unwrap_or("button");
                let name = step.name.as_deref().unwrap_or("");
                match op {
                    "click" => {
                        out.push_str(&format!(
                            "  await page.getByRole('{}', {{ name: {} }}).click();\n",
                            role,
                            serde_json::to_string(name).unwrap_or_else(|_| "\"\"".into())
                        ));
                    }
                    "fill" | "type" => {
                        out.push_str(&format!(
                            "  // sensitive fills must use host vault — skipped in replay\n  await page.getByRole('{}', {{ name: {} }}).click();\n",
                            role,
                            serde_json::to_string(name).unwrap_or_else(|_| "\"\"".into())
                        ));
                    }
                    "press" => {
                        out.push_str("  await page.keyboard.press('Enter');\n");
                    }
                    "scroll" => {
                        out.push_str("  await page.mouse.wheel(0, 600);\n");
                    }
                    _ => out.push_str(&format!("  // unknown op {op}\n")),
                }
            }
            _ => {}
        }
    }
    out.push_str("  console.log('browse replay ok');\n");
    out.push_str("}\n\nmain().catch((e) => { console.error(e); process.exit(1); });\n");
    out
}

/// Cheap overlay / clickjack heuristic: a11y name vs OCR-ish token from JPEG bytes.
/// Without a real OCR stack we use filename/hash entropy in the crop metadata.
pub fn overlay_mismatch(a11y_name: &str, pixel_label: Option<&str>) -> bool {
    let Some(label) = pixel_label.map(str::trim).filter(|s| !s.is_empty()) else {
        return false;
    };
    let a = a11y_name.trim().to_ascii_lowercase();
    let b = label.to_ascii_lowercase();
    if a.is_empty() || b.is_empty() {
        return false;
    }
    // Same string or one contains the other → ok.
    if a == b || a.contains(&b) || b.contains(&a) {
        return false;
    }
    true
}

/// Witness vote scaffold: allow unless the proposed act looks like a high-risk submit
/// without matching snapshot context (full witness model is a Fabric inference VIP).
pub fn witness_vote(proposed_op: &str, ref_name: Option<&str>, snapshot_text: &str) -> WitnessVote {
    let name = ref_name.unwrap_or("").to_ascii_lowercase();
    let snap = snapshot_text.to_ascii_lowercase();
    let risky = matches!(proposed_op, "click")
        && (name.contains("buy")
            || name.contains("purchase")
            || name.contains("pay")
            || name.contains("submit")
            || name.contains("confirm"));
    if !risky {
        return WitnessVote::Allow;
    }
    if snap.contains(&name) || name.is_empty() {
        WitnessVote::Allow
    } else {
        WitnessVote::Deny("witness: risky click label absent from last snapshot")
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WitnessVote {
    Allow,
    Deny(&'static str),
}

impl AgentPausedReason {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::VaultFill => "vault_fill",
            Self::OperatorWatch => "operator_watch",
            Self::Taint => "taint",
            Self::EbpfDeny => "ebpf_deny",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "vault_fill" => Some(Self::VaultFill),
            "operator_watch" => Some(Self::OperatorWatch),
            "taint" => Some(Self::Taint),
            "ebpf_deny" => Some(Self::EbpfDeny),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn paste_across_disjoint_origins_denied() {
        let clip = OriginSet::singleton("github.com");
        let tab = OriginSet::singleton("evil.example");
        assert!(matches!(
            check_cross_origin_flow(&clip, &tab),
            IfcVerdict::Deny(_)
        ));
    }

    #[test]
    fn same_origin_paste_allowed() {
        let clip = OriginSet::singleton("github.com");
        let tab = OriginSet::singleton("github.com");
        assert_eq!(check_cross_origin_flow(&clip, &tab), IfcVerdict::Allow);
    }

    #[test]
    fn cite_requires_goal_allow_subset() {
        let art = OriginSet::singleton("gmail.com");
        let goal = OriginSet::singleton("vcenter.local");
        assert!(matches!(
            check_cite_to_model(&art, &goal),
            IfcVerdict::Deny(_)
        ));
    }

    #[test]
    fn script_contains_goto_and_role() {
        let steps = vec![
            BrowseStep {
                seq: 1,
                tool: "open".into(),
                op: None,
                url: Some("https://example.com/".into()),
                ref_id: None,
                role: None,
                name: None,
                snapshot_hash: None,
                screenshot_hash: None,
                policy_hash: None,
                at: "t0".into(),
            },
            BrowseStep {
                seq: 2,
                tool: "act".into(),
                op: Some("click".into()),
                url: None,
                ref_id: Some("@e2".into()),
                role: Some("link".into()),
                name: Some("More information".into()),
                snapshot_hash: Some("abc".into()),
                screenshot_hash: None,
                policy_hash: None,
                at: "t1".into(),
            },
        ];
        let script = render_browse_script(Uuid::nil(), &steps);
        assert!(script.contains("page.goto"));
        assert!(script.contains("getByRole"));
        assert!(!script.contains("querySelector"));
    }

    #[test]
    fn overlay_flags_mismatched_labels() {
        assert!(overlay_mismatch("Cancel", Some("Buy now")));
        assert!(!overlay_mismatch("Submit", Some("submit order")));
    }

    #[test]
    fn witness_blocks_orphan_purchase_click() {
        assert!(matches!(
            witness_vote("click", Some("Buy now"), "heading Example Domain"),
            WitnessVote::Deny(_)
        ));
        assert_eq!(
            witness_vote("click", Some("Buy now"), "button Buy now"),
            WitnessVote::Allow
        );
    }

    #[test]
    fn honesty_badge_shape() {
        let b = honesty_badge(
            "software-test",
            true,
            "allowed",
            true,
            Some(&AgentPausedReason::VaultFill),
            Some("merion-browser/t/s"),
        );
        assert_eq!(b["browser"], "a11y-only");
        assert_eq!(b["proxy"], "strict");
        assert_eq!(b["agent_paused_reason"], "vault_fill");
    }
}
