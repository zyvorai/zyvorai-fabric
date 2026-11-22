// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: LicenseRef-Zyvor-Production-2.0

use std::path::{Path, PathBuf};

use crate::error::{CgroupError, Result};
use crate::util::{read_cgroup_file, write_cgroup_file};

/// Controller for the cpuset cgroup v2 subsystem.
pub struct CpusetController {
    path: PathBuf,
}

impl CpusetController {
    pub fn new(path: PathBuf) -> Self {
        Self { path }
    }

    /// Read cpuset.cpus.
    pub fn get_cpus(&self) -> Result<Vec<u32>> {
        let file = self.path.join("cpuset.cpus");
        let content = read_cgroup_file(&file)?;
        parse_set(&file, &content)
    }

    /// Set cpuset.cpus (e.g., vec![0,1,2,3] writes "0-3").
    pub fn set_cpus(&self, cpus: &[u32]) -> Result<()> {
        let file = self.path.join("cpuset.cpus");
        write_cgroup_file(&file, &format_set(cpus))
    }

    /// Read cpuset.cpus.effective.
    pub fn get_cpus_effective(&self) -> Result<Vec<u32>> {
        let file = self.path.join("cpuset.cpus.effective");
        let content = read_cgroup_file(&file)?;
        parse_set(&file, &content)
    }

    /// Read cpuset.mems.
    pub fn get_mems(&self) -> Result<Vec<u32>> {
        let file = self.path.join("cpuset.mems");
        let content = read_cgroup_file(&file)?;
        parse_set(&file, &content)
    }

    /// Set cpuset.mems (e.g., vec![0,1] writes "0-1").
    pub fn set_mems(&self, mems: &[u32]) -> Result<()> {
        let file = self.path.join("cpuset.mems");
        write_cgroup_file(&file, &format_set(mems))
    }

    /// Read cpuset.mems.effective.
    pub fn get_mems_effective(&self) -> Result<Vec<u32>> {
        let file = self.path.join("cpuset.mems.effective");
        let content = read_cgroup_file(&file)?;
        parse_set(&file, &content)
    }
}

/// Parse a CPU/memory set string like "0-3,8,10-12" into a sorted Vec of IDs.
pub fn parse_set(path: &Path, content: &str) -> Result<Vec<u32>> {
    let trimmed = content.trim();
    if trimmed.is_empty() {
        return Ok(Vec::new());
    }

    let mut result = Vec::new();
    for part in trimmed.split(',') {
        let part = part.trim();
        if part.is_empty() {
            continue;
        }
        if let Some((start_str, end_str)) = part.split_once('-') {
            let start: u32 = start_str
                .trim()
                .parse()
                .map_err(|_| CgroupError::ParseError {
                    path: path.to_path_buf(),
                    content: content.to_string(),
                    detail: format!("invalid range start: {start_str:?}"),
                })?;
            let end: u32 = end_str
                .trim()
                .parse()
                .map_err(|_| CgroupError::ParseError {
                    path: path.to_path_buf(),
                    content: content.to_string(),
                    detail: format!("invalid range end: {end_str:?}"),
                })?;
            for id in start..=end {
                result.push(id);
            }
        } else {
            let id: u32 = part.parse().map_err(|_| CgroupError::ParseError {
                path: path.to_path_buf(),
                content: content.to_string(),
                detail: format!("invalid cpu/mem id: {part:?}"),
            })?;
            result.push(id);
        }
    }

    result.sort_unstable();
    result.dedup();
    Ok(result)
}

/// Format a sorted list of IDs into a compact set string like "0-3,8,10-12".
pub fn format_set(ids: &[u32]) -> String {
    if ids.is_empty() {
        return String::new();
    }

    let mut sorted: Vec<u32> = ids.to_vec();
    sorted.sort_unstable();
    sorted.dedup();

    let mut ranges: Vec<String> = Vec::new();
    let mut start = sorted[0];
    let mut end = sorted[0];

    for &id in &sorted[1..] {
        if id == end + 1 {
            end = id;
        } else {
            ranges.push(format_range(start, end));
            start = id;
            end = id;
        }
    }
    ranges.push(format_range(start, end));

    ranges.join(",")
}

fn format_range(start: u32, end: u32) -> String {
    if start == end {
        start.to_string()
    } else {
        format!("{start}-{end}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn test_parse_set_range() {
        let p = PathBuf::from("/test");
        assert_eq!(parse_set(&p, "0-3").unwrap(), vec![0, 1, 2, 3]);
    }

    #[test]
    fn test_parse_set_mixed() {
        let p = PathBuf::from("/test");
        assert_eq!(
            parse_set(&p, "0-3,8,10-12").unwrap(),
            vec![0, 1, 2, 3, 8, 10, 11, 12]
        );
    }

    #[test]
    fn test_parse_set_single() {
        let p = PathBuf::from("/test");
        assert_eq!(parse_set(&p, "5").unwrap(), vec![5]);
    }

    #[test]
    fn test_parse_set_empty() {
        let p = PathBuf::from("/test");
        assert_eq!(parse_set(&p, "").unwrap(), Vec::<u32>::new());
    }

    #[test]
    fn test_parse_set_whitespace() {
        let p = PathBuf::from("/test");
        assert_eq!(parse_set(&p, "  0-3  ").unwrap(), vec![0, 1, 2, 3]);
    }

    #[test]
    fn test_format_set_range() {
        assert_eq!(format_set(&[0, 1, 2, 3]), "0-3");
    }

    #[test]
    fn test_format_set_mixed() {
        assert_eq!(format_set(&[0, 1, 2, 3, 8, 10, 11, 12]), "0-3,8,10-12");
    }

    #[test]
    fn test_format_set_single() {
        assert_eq!(format_set(&[5]), "5");
    }

    #[test]
    fn test_format_set_empty() {
        assert_eq!(format_set(&[]), "");
    }

    #[test]
    fn test_format_set_unsorted() {
        assert_eq!(format_set(&[3, 1, 2, 0]), "0-3");
    }

    #[test]
    fn test_format_set_gaps() {
        assert_eq!(format_set(&[0, 2, 4, 6]), "0,2,4,6");
    }

    #[test]
    fn test_roundtrip() {
        let p = PathBuf::from("/test");
        let original = "0-3,8,10-12";
        let parsed = parse_set(&p, original).unwrap();
        let formatted = format_set(&parsed);
        assert_eq!(formatted, original);
    }
}
