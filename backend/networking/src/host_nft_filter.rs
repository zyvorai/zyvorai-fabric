// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: LicenseRef-Zyvor-Production-2.0

use anyhow::Result;

use crate::nftables::NftManager;

/// A filter chain discovered from host nftables (read-only profile).
#[derive(Debug, Clone)]
pub struct DiscoveredNftFilterChain {
    pub key: String,
    pub name: String,
    pub description: String,
    pub table: String,
    pub chain: String,
    pub rule_count: usize,
    pub default_action: String,
}

/// Discover filter chains from the host nftables ruleset.
pub fn discover_host_nft_filter_chains() -> Result<Vec<DiscoveredNftFilterChain>> {
    let nft = NftManager::new();
    let json = nft.list_ruleset()?;
    Ok(parse_filter_chains(&json))
}

fn parse_filter_chains(json: &serde_json::Value) -> Vec<DiscoveredNftFilterChain> {
    let Some(items) = json.get("nftables").and_then(|v| v.as_array()) else {
        return Vec::new();
    };

    let mut filter_chains: std::collections::HashMap<(String, String), (usize, String)> =
        std::collections::HashMap::new();

    for item in items {
        let Some(chain) = item.get("chain") else {
            continue;
        };
        let family = chain.get("family").and_then(|f| f.as_str()).unwrap_or("ip");
        if family != "ip" && family != "ip6" {
            continue;
        }
        let table = chain
            .get("table")
            .and_then(|t| t.as_str())
            .unwrap_or("")
            .to_string();
        if table == "zyvor-fabricd" || table == "zyvor-fabricd6" {
            continue;
        }
        let hook = chain.get("hook").and_then(|h| h.as_str()).unwrap_or("");
        if !matches!(hook, "input" | "output" | "forward") {
            continue;
        }
        let name = chain
            .get("name")
            .and_then(|n| n.as_str())
            .unwrap_or("")
            .to_string();
        let policy = chain
            .get("policy")
            .and_then(|p| p.as_str())
            .unwrap_or("drop")
            .to_string();
        filter_chains.entry((table, name)).or_insert((0, policy));
    }

    for item in items {
        let Some(rule) = item.get("rule") else {
            continue;
        };
        let table = rule
            .get("table")
            .and_then(|t| t.as_str())
            .unwrap_or("")
            .to_string();
        if table == "zyvor-fabricd" || table == "zyvor-fabricd6" {
            continue;
        }
        let chain = rule
            .get("chain")
            .and_then(|c| c.as_str())
            .unwrap_or("")
            .to_string();
        if let Some(entry) = filter_chains.get_mut(&(table, chain)) {
            entry.0 += 1;
        }
    }

    filter_chains
        .into_iter()
        .filter(|(_, (count, _))| *count > 0)
        .map(|((table, chain), (rule_count, default_action))| {
            let key = format!("{table}:{chain}");
            DiscoveredNftFilterChain {
                name: format!("host-nft-{table}-{chain}"),
                description: format!(
                    "Host nftables filter chain {table}/{chain} ({rule_count} rules, policy {default_action})"
                ),
                table: table.clone(),
                chain: chain.clone(),
                rule_count,
                default_action,
                key,
            }
        })
        .collect()
}
