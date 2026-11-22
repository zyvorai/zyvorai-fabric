// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: LicenseRef-Zyvor-Production-2.0

use anyhow::Result;
use std::collections::{BTreeMap, HashMap, HashSet};

use crate::host_firewalld::{self, DiscoveredFirewalldZone};
use crate::nftables::NftManager;

/// A host security identity grouping discovered from firewalld or nftables.
#[derive(Debug, Clone)]
pub struct DiscoveredHostIdentity {
    pub key: String,
    pub labels: BTreeMap<String, String>,
    pub endpoints: Vec<String>,
    pub description: String,
}

/// Discover host identity groupings from firewalld zones and nftables address sets.
pub fn discover_host_identities() -> Result<Vec<DiscoveredHostIdentity>> {
    let mut out = Vec::new();
    let mut keys = HashSet::new();

    for zone in host_firewalld::discover_firewalld_zones().unwrap_or_default() {
        let identity = identity_from_firewalld_zone(zone);
        if keys.insert(identity.key.clone()) {
            out.push(identity);
        }
    }

    for identity in discover_nft_set_identities().unwrap_or_default() {
        if keys.insert(identity.key.clone()) {
            out.push(identity);
        }
    }

    Ok(out)
}

fn identity_from_firewalld_zone(zone: DiscoveredFirewalldZone) -> DiscoveredHostIdentity {
    let mut endpoints = zone.interfaces;
    endpoints.extend(zone.sources);
    endpoints.sort();
    endpoints.dedup();

    let mut labels = BTreeMap::new();
    labels.insert("source".to_string(), "firewalld".to_string());
    labels.insert("zone".to_string(), zone.name.clone());

    DiscoveredHostIdentity {
        key: format!("firewalld:{}", zone.name),
        labels,
        endpoints,
        description: zone.description,
    }
}

fn discover_nft_set_identities() -> Result<Vec<DiscoveredHostIdentity>> {
    let nft = NftManager::new();
    let json = nft.list_ruleset()?;
    Ok(parse_nft_set_identities(&json))
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

fn is_address_set_type(type_name: &str) -> bool {
    matches!(
        type_name,
        "ipv4_addr" | "ipv6_addr" | "inet_addr" | "ip_addr"
    )
}

fn parse_nft_set_identities(json: &serde_json::Value) -> Vec<DiscoveredHostIdentity> {
    let Some(items) = json.get("nftables").and_then(|v| v.as_array()) else {
        return Vec::new();
    };

    let mut address_sets: HashMap<(String, String), String> = HashMap::new();
    let mut elements: HashMap<(String, String), Vec<String>> = HashMap::new();

    for item in items {
        if let Some(set) = item.get("set") {
            let family = set.get("family").and_then(|f| f.as_str()).unwrap_or("ip");
            if family != "ip" && family != "ip6" && family != "inet" {
                continue;
            }
            let table = set
                .get("table")
                .and_then(|t| t.as_str())
                .unwrap_or("")
                .to_string();
            if skip_table(&table) {
                continue;
            }
            let name = set
                .get("name")
                .and_then(|n| n.as_str())
                .unwrap_or("")
                .to_string();
            if name.is_empty() || name.starts_with("identity_") {
                continue;
            }
            let type_name = set.get("type").and_then(|t| t.as_str()).unwrap_or("");
            if !is_address_set_type(type_name) {
                continue;
            }
            address_sets.insert((table.clone(), name.clone()), type_name.to_string());
            collect_set_elements(set, &mut elements, &table, &name);
        }

        if let Some(element) = item.get("element") {
            let table = element
                .get("table")
                .and_then(|t| t.as_str())
                .unwrap_or("")
                .to_string();
            if skip_table(&table) {
                continue;
            }
            let name = element
                .get("name")
                .and_then(|n| n.as_str())
                .unwrap_or("")
                .to_string();
            if name.is_empty() {
                continue;
            }
            let key = (table, name);
            let bucket = elements.entry(key).or_default();
            if let Some(elem) = element.get("elem") {
                extend_elements(bucket, elem);
            }
        }
    }

    address_sets
        .into_iter()
        .filter_map(|((table, name), type_name)| {
            let endpoints = elements
                .remove(&(table.clone(), name.clone()))
                .unwrap_or_default();
            if endpoints.is_empty() {
                return None;
            }
            let mut labels = BTreeMap::new();
            labels.insert("source".to_string(), "nftables".to_string());
            labels.insert("table".to_string(), table.clone());
            labels.insert("set".to_string(), name.clone());

            let count = endpoints.len();
            Some(DiscoveredHostIdentity {
                key: format!("nft:{table}:{name}"),
                labels,
                endpoints,
                description: format!(
                    "Host nftables address set {table}/{name} ({type_name}, {count} members)"
                ),
            })
        })
        .collect()
}

fn collect_set_elements(
    set: &serde_json::Value,
    elements: &mut HashMap<(String, String), Vec<String>>,
    table: &str,
    name: &str,
) {
    let key = (table.to_string(), name.to_string());
    let bucket = elements.entry(key).or_default();
    if let Some(elem) = set.get("elem") {
        extend_elements(bucket, elem);
    }
}

fn extend_elements(out: &mut Vec<String>, elem: &serde_json::Value) {
    match elem {
        serde_json::Value::Array(items) => {
            for item in items {
                if let Some(value) = element_value(item) {
                    out.push(value);
                }
            }
        }
        _ => {
            if let Some(value) = element_value(elem) {
                out.push(value);
            }
        }
    }
}

fn element_value(item: &serde_json::Value) -> Option<String> {
    if let Some(s) = item.as_str() {
        return Some(s.to_string());
    }
    if let Some(inner) = item.get("elem").and_then(|v| v.as_str()) {
        return Some(inner.to_string());
    }
    if let Some(prefix) = item.get("prefix") {
        let addr = prefix.get("addr").and_then(|a| a.as_str()).unwrap_or("");
        let len = prefix.get("len").and_then(|l| l.as_u64()).unwrap_or(0);
        if !addr.is_empty() {
            return Some(format!("{addr}/{len}"));
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_nft_address_set_elements() {
        let json: serde_json::Value = serde_json::json!({
            "nftables": [
                {
                    "set": {
                        "family": "ip",
                        "table": "filter",
                        "name": "trusted",
                        "type": "ipv4_addr",
                        "elem": ["10.0.0.1", "10.0.0.2"]
                    }
                }
            ]
        });
        let identities = parse_nft_set_identities(&json);
        assert_eq!(identities.len(), 1);
        assert_eq!(identities[0].endpoints.len(), 2);
        assert_eq!(
            identities[0].labels.get("set").map(String::as_str),
            Some("trusted")
        );
    }

    #[test]
    fn skips_zyvor_fabricd_sets() {
        let json: serde_json::Value = serde_json::json!({
            "nftables": [{
                "set": {
                    "family": "ip",
                    "table": "zyvor-fabricd_policy",
                    "name": "identity_256",
                    "type": "ipv4_addr",
                    "elem": ["10.0.0.5"]
                }
            }]
        });
        assert!(parse_nft_set_identities(&json).is_empty());
    }
}
