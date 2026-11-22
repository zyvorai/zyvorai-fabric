// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: LicenseRef-Zyvor-Production-2.0

use crate::models::{ParsedConfigFile, ParsedSection};
use anyhow::{Context, Result};
use std::fs;
use std::path::Path;

/// Parse a systemd-networkd INI-style config file into sections
pub fn parse_config(content: &str) -> Vec<ParsedSection> {
    let mut sections = Vec::new();
    let mut current: Option<ParsedSection> = None;

    for line in content.lines() {
        let line = line.trim();

        // Skip empty lines and comments
        if line.is_empty() || line.starts_with('#') || line.starts_with(';') {
            continue;
        }

        // Section header
        if line.starts_with('[') && line.ends_with(']') {
            if let Some(section) = current.take() {
                sections.push(section);
            }
            current = Some(ParsedSection {
                name: line[1..line.len() - 1].to_string(),
                entries: Vec::new(),
            });
            continue;
        }

        // Key=Value entry
        if let Some(ref mut section) = current {
            if let Some(eq_pos) = line.find('=') {
                let key = line[..eq_pos].trim().to_string();
                let value = line[eq_pos + 1..].trim().to_string();
                section.entries.push((key, value));
            }
        }
    }

    if let Some(section) = current {
        sections.push(section);
    }

    sections
}

/// Determine the file type from filename extension
fn file_type_from_name(filename: &str) -> &str {
    if filename.ends_with(".netdev") {
        "netdev"
    } else if filename.ends_with(".network") {
        "network"
    } else if filename.ends_with(".link") {
        "link"
    } else {
        "unknown"
    }
}

/// Scan a directory for systemd-networkd config files (.netdev, .network, .link)
/// and parse each one
pub fn scan_networkd_dir(dir: &Path) -> Result<Vec<ParsedConfigFile>> {
    let dir_s = dir.to_string_lossy().into_owned();
    if dir_s.contains("..") {
        anyhow::bail!("rejected networkd directory");
    }
    if !input_guard::COMMAND_ARGS.contains(&dir_s) {
        anyhow::bail!("rejected networkd directory");
    }
    let dir = Path::new(&dir_s);
    let mut results = Vec::new();

    if !dir.exists() {
        return Ok(results);
    }

    let mut entries: Vec<_> = fs::read_dir(dir)
        .with_context(|| format!("Failed to read directory {}", dir.display()))?
        .filter_map(|e| e.ok())
        .collect();

    entries.sort_by_key(|e| e.file_name());

    for entry in entries {
        let filename = entry.file_name().to_string_lossy().to_string();

        if !filename.ends_with(".netdev")
            && !filename.ends_with(".network")
            && !filename.ends_with(".link")
        {
            continue;
        }

        let content = fs::read_to_string(entry.path())
            .with_context(|| format!("Failed to read {}", entry.path().display()))?;

        let sections = parse_config(&content);

        results.push(ParsedConfigFile {
            filename,
            file_type: file_type_from_name(&entry.file_name().to_string_lossy()).to_string(),
            sections,
        });
    }

    Ok(results)
}

/// Get a value from parsed sections by section name and key
pub fn get_value(sections: &[ParsedSection], section_name: &str, key: &str) -> Option<String> {
    for section in sections {
        if section.name == section_name {
            for (k, v) in &section.entries {
                if k == key {
                    return Some(v.clone());
                }
            }
        }
    }
    None
}

/// Get all values for a key from a section (for repeated keys like Address, DNS)
pub fn get_values(sections: &[ParsedSection], section_name: &str, key: &str) -> Vec<String> {
    let mut values = Vec::new();
    for section in sections {
        if section.name == section_name {
            for (k, v) in &section.entries {
                if k == key {
                    values.push(v.clone());
                }
            }
        }
    }
    values
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_bridge_netdev() {
        let content = r#"
[NetDev]
Name=br0
Kind=bridge
MTUBytes=1500

[Bridge]
STP=yes
ForwardDelaySec=15
"#;
        let sections = parse_config(content);
        assert_eq!(sections.len(), 2);
        assert_eq!(sections[0].name, "NetDev");
        assert_eq!(sections[0].entries.len(), 3);
        assert_eq!(sections[1].name, "Bridge");
        assert_eq!(get_value(&sections, "NetDev", "Name"), Some("br0".into()));
        assert_eq!(
            get_value(&sections, "NetDev", "Kind"),
            Some("bridge".into())
        );
        assert_eq!(get_value(&sections, "Bridge", "STP"), Some("yes".into()));
    }

    #[test]
    fn test_parse_network_with_routes() {
        let content = r#"
[Match]
Name=eth0

[Network]
Address=10.0.0.1/24
Address=10.0.0.2/24
Gateway=10.0.0.254
DNS=8.8.8.8
DNS=1.1.1.1
DHCP=no

[Route]
Destination=172.16.0.0/12
Gateway=10.0.0.1
Metric=100
"#;
        let sections = parse_config(content);
        assert_eq!(sections.len(), 3);
        let addrs = get_values(&sections, "Network", "Address");
        assert_eq!(addrs, vec!["10.0.0.1/24", "10.0.0.2/24"]);
        let dns = get_values(&sections, "Network", "DNS");
        assert_eq!(dns, vec!["8.8.8.8", "1.1.1.1"]);
        assert_eq!(
            get_value(&sections, "Route", "Destination"),
            Some("172.16.0.0/12".into())
        );
    }

    #[test]
    fn test_parse_link_file() {
        let content = r#"
[Match]
MACAddress=00:11:22:33:44:55

[Link]
Name=lan0
MTUBytes=9000
WakeOnLan=magic
"#;
        let sections = parse_config(content);
        assert_eq!(sections.len(), 2);
        assert_eq!(
            get_value(&sections, "Match", "MACAddress"),
            Some("00:11:22:33:44:55".into())
        );
        assert_eq!(get_value(&sections, "Link", "Name"), Some("lan0".into()));
        assert_eq!(
            get_value(&sections, "Link", "WakeOnLan"),
            Some("magic".into())
        );
    }

    #[test]
    fn test_parse_comments_and_empty_lines() {
        let content = r#"
# This is a comment
; Another comment

[NetDev]
Name=test
Kind=tap

# More comments
"#;
        let sections = parse_config(content);
        assert_eq!(sections.len(), 1);
        assert_eq!(sections[0].entries.len(), 2);
    }

    #[test]
    fn test_parse_bond_netdev() {
        let content = r#"
[NetDev]
Name=bond0
Kind=bond

[Bond]
Mode=802.3ad
MIIMonitorSec=100
LACPTransmitRate=fast
TransmitHashPolicy=layer3+4
"#;
        let sections = parse_config(content);
        assert_eq!(sections.len(), 2);
        assert_eq!(get_value(&sections, "Bond", "Mode"), Some("802.3ad".into()));
        assert_eq!(
            get_value(&sections, "Bond", "LACPTransmitRate"),
            Some("fast".into())
        );
    }

    #[test]
    fn test_scan_directory() {
        let dir = tempfile::tempdir().unwrap();

        fs::write(
            dir.path().join("10-br0.netdev"),
            "[NetDev]\nName=br0\nKind=bridge\n",
        )
        .unwrap();
        fs::write(
            dir.path().join("10-br0.network"),
            "[Match]\nName=br0\n\n[Network]\nDHCP=yes\n",
        )
        .unwrap();
        fs::write(dir.path().join("README.txt"), "not a config").unwrap();

        let results = scan_networkd_dir(dir.path()).unwrap();
        assert_eq!(results.len(), 2);
        assert_eq!(results[0].file_type, "netdev");
        assert_eq!(results[1].file_type, "network");
    }

    #[test]
    fn test_scan_nonexistent_dir() {
        let results = scan_networkd_dir(Path::new("/nonexistent/path/12345")).unwrap();
        assert!(results.is_empty());
    }

    #[test]
    fn test_get_value_missing() {
        let sections = parse_config("[NetDev]\nName=test\n");
        assert_eq!(get_value(&sections, "NetDev", "Kind"), None);
        assert_eq!(get_value(&sections, "Missing", "Name"), None);
    }
}
