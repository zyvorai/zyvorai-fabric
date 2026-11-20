// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: LicenseRef-Zyvor-Production-1.0

use anyhow::Result;

use crate::nftables::NftManager;

/// A forward filter chain discovered from host nftables (read-only policy).
#[derive(Debug, Clone)]
pub struct DiscoveredNftPolicyChain {
    pub key: String,
    pub name: String,
    pub description: String,
    pub table: String,
    pub chain: String,
    pub rule_count: usize,
}

fn skip_table(table: &str) -> bool {
    matches!(
        table,
        "zyvor-fabricd"
            | "zyvor-fabricd6"
            | "zyvor-fabricd_policy"
            | "zyvor-fabricd_nat"
            | "zyvor-fabricd_dnat"
    )
}

/// Discover forward filter chains from the host nftables ruleset.
pub fn discover_host_nft_policy_chains() -> Result<Vec<DiscoveredNftPolicyChain>> {
    let nft = NftManager::new();
    let json = nft.list_ruleset()?;
    Ok(parse_forward_chains(&json))
}

fn parse_forward_chains(json: &serde_json::Value) -> Vec<DiscoveredNftPolicyChain> {
    let Some(items) = json.get("nftables").and_then(|v| v.as_array()) else {
        return Vec::new();
    };

    let mut chains: std::collections::HashMap<(String, String), usize> =
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
        if skip_table(&table) {
            continue;
        }
        let hook = chain.get("hook").and_then(|h| h.as_str()).unwrap_or("");
        if hook != "forward" {
            continue;
        }
        let name = chain
            .get("name")
            .and_then(|n| n.as_str())
            .unwrap_or("")
            .to_string();
        chains.entry((table, name)).or_insert(0);
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
        if skip_table(&table) {
            continue;
        }
        let chain = rule
            .get("chain")
            .and_then(|c| c.as_str())
            .unwrap_or("")
            .to_string();
        if let Some(count) = chains.get_mut(&(table.clone(), chain.clone())) {
            *count += 1;
        }
    }

    chains
        .into_iter()
        .filter(|(_, count)| *count > 0)
        .map(|((table, chain), rule_count)| {
            let key = format!("{table}:{chain}");
            DiscoveredNftPolicyChain {
                name: format!("host-policy-{table}-{chain}"),
                description: format!(
                    "Host nftables forward chain {table}/{chain} ({rule_count} rules)"
                ),
                table: table.clone(),
                chain: chain.clone(),
                rule_count,
                key,
            }
        })
        .collect()
}
