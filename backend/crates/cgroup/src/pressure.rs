// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: LicenseRef-Zyvor-Production-1.0

use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::error::{CgroupError, Result};
use crate::util::read_cgroup_file;

/// A single PSI pressure record (some or full).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PressureRecord {
    pub avg10: f64,
    pub avg60: f64,
    pub avg300: f64,
    pub total: u64,
}

/// PSI pressure stats for a resource.
///
/// CPU only has `some`; memory and IO have both `some` and `full`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PressureStats {
    pub some: PressureRecord,
    pub full: Option<PressureRecord>,
}

/// Parse a PSI pressure line like:
/// `some avg10=0.00 avg60=0.00 avg300=0.00 total=0`
fn parse_pressure_line<'a>(path: &Path, line: &'a str) -> Result<(&'a str, PressureRecord)> {
    let mut parts = line.split_whitespace();
    let kind = parts.next().ok_or_else(|| CgroupError::ParseError {
        path: path.to_path_buf(),
        content: line.to_string(),
        detail: "empty pressure line".to_string(),
    })?;

    let mut avg10 = 0.0;
    let mut avg60 = 0.0;
    let mut avg300 = 0.0;
    let mut total = 0u64;

    for token in parts {
        let (key, value) = token
            .split_once('=')
            .ok_or_else(|| CgroupError::ParseError {
                path: path.to_path_buf(),
                content: line.to_string(),
                detail: format!("invalid key=value token: {token:?}"),
            })?;
        match key {
            "avg10" => {
                avg10 = value.parse().map_err(|_| CgroupError::ParseError {
                    path: path.to_path_buf(),
                    content: line.to_string(),
                    detail: format!("invalid avg10: {value:?}"),
                })?;
            }
            "avg60" => {
                avg60 = value.parse().map_err(|_| CgroupError::ParseError {
                    path: path.to_path_buf(),
                    content: line.to_string(),
                    detail: format!("invalid avg60: {value:?}"),
                })?;
            }
            "avg300" => {
                avg300 = value.parse().map_err(|_| CgroupError::ParseError {
                    path: path.to_path_buf(),
                    content: line.to_string(),
                    detail: format!("invalid avg300: {value:?}"),
                })?;
            }
            "total" => {
                total = value.parse().map_err(|_| CgroupError::ParseError {
                    path: path.to_path_buf(),
                    content: line.to_string(),
                    detail: format!("invalid total: {value:?}"),
                })?;
            }
            _ => {}
        }
    }

    Ok((
        kind,
        PressureRecord {
            avg10,
            avg60,
            avg300,
            total,
        },
    ))
}

/// Parse full PSI pressure content from a pressure file.
pub fn parse_pressure(path: &Path, content: &str) -> Result<PressureStats> {
    let mut some = None;
    let mut full = None;

    for line in content.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let (kind, record) = parse_pressure_line(path, line)?;
        match kind {
            "some" => some = Some(record),
            "full" => full = Some(record),
            _ => {}
        }
    }

    let some = some.ok_or_else(|| CgroupError::ParseError {
        path: path.to_path_buf(),
        content: content.to_string(),
        detail: "missing 'some' pressure line".to_string(),
    })?;

    Ok(PressureStats { some, full })
}

/// Read and parse a PSI pressure file.
pub fn read_pressure(path: &Path) -> Result<PressureStats> {
    let content = read_cgroup_file(path)?;
    parse_pressure(path, &content)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn test_parse_cpu_pressure() {
        let p = PathBuf::from("/test/cpu.pressure");
        let content = "some avg10=1.50 avg60=2.30 avg300=0.80 total=123456\n";
        let stats = parse_pressure(&p, content).unwrap();
        assert!((stats.some.avg10 - 1.50).abs() < f64::EPSILON);
        assert!((stats.some.avg60 - 2.30).abs() < f64::EPSILON);
        assert!((stats.some.avg300 - 0.80).abs() < f64::EPSILON);
        assert_eq!(stats.some.total, 123456);
        assert!(stats.full.is_none());
    }

    #[test]
    fn test_parse_memory_pressure() {
        let p = PathBuf::from("/test/memory.pressure");
        let content = "\
some avg10=0.00 avg60=0.00 avg300=0.00 total=0
full avg10=0.00 avg60=0.00 avg300=0.00 total=0
";
        let stats = parse_pressure(&p, content).unwrap();
        assert!((stats.some.avg10 - 0.0).abs() < f64::EPSILON);
        let full = stats.full.unwrap();
        assert!((full.avg10 - 0.0).abs() < f64::EPSILON);
        assert_eq!(full.total, 0);
    }

    #[test]
    fn test_parse_pressure_missing_some() {
        let p = PathBuf::from("/test/pressure");
        let content = "full avg10=0.00 avg60=0.00 avg300=0.00 total=0\n";
        assert!(parse_pressure(&p, content).is_err());
    }

    #[test]
    #[allow(clippy::approx_constant)]
    fn test_parse_pressure_with_values() {
        let p = PathBuf::from("/test/io.pressure");
        let content = "\
some avg10=10.25 avg60=5.50 avg300=1.75 total=999999
full avg10=3.14 avg60=2.71 avg300=1.41 total=500000
";
        let stats = parse_pressure(&p, content).unwrap();
        assert!((stats.some.avg10 - 10.25).abs() < f64::EPSILON);
        assert!((stats.some.avg60 - 5.50).abs() < f64::EPSILON);
        assert_eq!(stats.some.total, 999999);
        let full = stats.full.unwrap();
        assert!((full.avg10 - 3.14).abs() < f64::EPSILON);
        assert_eq!(full.total, 500000);
    }
}
