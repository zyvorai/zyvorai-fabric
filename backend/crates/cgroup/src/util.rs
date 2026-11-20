// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: LicenseRef-Zyvor-Production-1.0

use std::path::Path;

use crate::error::{CgroupError, Result};

/// Read a cgroup file, returning its trimmed contents.
pub fn read_cgroup_file(path: &Path) -> Result<String> {
    let content = std::fs::read_to_string(path).map_err(|e| CgroupError::ReadFailed {
        path: path.to_path_buf(),
        source: e,
    })?;
    Ok(content.trim().to_string())
}

/// Write a value to a cgroup file.
pub fn write_cgroup_file(path: &Path, value: &str) -> Result<()> {
    tracing::debug!("writing {:?} to {}", value, path.display());
    std::fs::write(path, value).map_err(|e| CgroupError::WriteFailed {
        path: path.to_path_buf(),
        source: e,
    })
}

/// Parse a string as u64, treating "max" as u64::MAX.
pub fn parse_u64_or_max(path: &Path, content: &str) -> Result<u64> {
    let trimmed = content.trim();
    if trimmed == "max" {
        return Ok(u64::MAX);
    }
    trimmed.parse::<u64>().map_err(|_| CgroupError::ParseError {
        path: path.to_path_buf(),
        content: trimmed.to_string(),
        detail: "expected u64 or \"max\"".to_string(),
    })
}

/// Read a cgroup file and parse as u64, treating "max" as u64::MAX.
pub fn read_u64_or_max(path: &Path) -> Result<u64> {
    let content = read_cgroup_file(path)?;
    parse_u64_or_max(path, &content)
}

/// Read a cgroup file and parse as u64.
pub fn read_u64(path: &Path) -> Result<u64> {
    let content = read_cgroup_file(path)?;
    content.parse::<u64>().map_err(|_| CgroupError::ParseError {
        path: path.to_path_buf(),
        content,
        detail: "expected u64".to_string(),
    })
}

/// Parse flat keyed format: "key value\n" lines into (key, value) pairs.
pub fn parse_flat_keyed(path: &Path, content: &str) -> Result<Vec<(String, u64)>> {
    let mut entries = Vec::new();
    for line in content.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let mut parts = line.splitn(2, ' ');
        let key = parts.next().unwrap_or("");
        let value_str = parts.next().unwrap_or("").trim();
        let value = value_str
            .parse::<u64>()
            .map_err(|_| CgroupError::ParseError {
                path: path.to_path_buf(),
                content: line.to_string(),
                detail: format!("expected u64 value for key {key:?}, got {value_str:?}"),
            })?;
        entries.push((key.to_string(), value));
    }
    Ok(entries)
}

/// Read a flat-keyed cgroup file.
pub fn read_flat_keyed(path: &Path) -> Result<Vec<(String, u64)>> {
    let content = read_cgroup_file(path)?;
    parse_flat_keyed(path, &content)
}

/// Look up a key in flat-keyed entries, returning 0 if not found.
pub fn lookup_key(entries: &[(String, u64)], key: &str) -> u64 {
    entries
        .iter()
        .find(|(k, _)| k == key)
        .map(|(_, v)| *v)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn test_parse_u64_or_max() {
        let p = PathBuf::from("/test");
        assert_eq!(parse_u64_or_max(&p, "max").unwrap(), u64::MAX);
        assert_eq!(parse_u64_or_max(&p, "12345").unwrap(), 12345);
        assert_eq!(parse_u64_or_max(&p, "  max  ").unwrap(), u64::MAX);
        assert_eq!(parse_u64_or_max(&p, "  0  ").unwrap(), 0);
        assert!(parse_u64_or_max(&p, "abc").is_err());
    }

    #[test]
    fn test_parse_flat_keyed() {
        let p = PathBuf::from("/test");
        let content = "usage_usec 123456\nuser_usec 100000\nsystem_usec 23456\n";
        let entries = parse_flat_keyed(&p, content).unwrap();
        assert_eq!(entries.len(), 3);
        assert_eq!(entries[0], ("usage_usec".to_string(), 123456));
        assert_eq!(entries[1], ("user_usec".to_string(), 100000));
        assert_eq!(entries[2], ("system_usec".to_string(), 23456));
    }

    #[test]
    fn test_parse_flat_keyed_empty() {
        let p = PathBuf::from("/test");
        let entries = parse_flat_keyed(&p, "").unwrap();
        assert!(entries.is_empty());
    }

    #[test]
    fn test_parse_flat_keyed_invalid() {
        let p = PathBuf::from("/test");
        assert!(parse_flat_keyed(&p, "key notanumber").is_err());
    }

    #[test]
    fn test_lookup_key() {
        let entries = vec![
            ("anon".to_string(), 100),
            ("file".to_string(), 200),
            ("kernel".to_string(), 300),
        ];
        assert_eq!(lookup_key(&entries, "anon"), 100);
        assert_eq!(lookup_key(&entries, "file"), 200);
        assert_eq!(lookup_key(&entries, "missing"), 0);
    }
}
