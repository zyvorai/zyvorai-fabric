// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: LicenseRef-Zyvor-Production-1.0

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::error::{CgroupError, Result};
use crate::pressure::PressureStats;
use crate::util::{lookup_key, read_cgroup_file, read_flat_keyed, write_cgroup_file};

/// CPU bandwidth limit from cpu.max.
///
/// Format: "$QUOTA $PERIOD" where QUOTA can be "max" (unlimited).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CpuMax {
    /// Quota in microseconds, or None for unlimited ("max").
    pub quota_usec: Option<u64>,
    /// Period in microseconds (default 100000 = 100ms).
    pub period_usec: u64,
}

impl CpuMax {
    /// Create an unlimited CPU max (no throttling).
    pub fn unlimited() -> Self {
        Self {
            quota_usec: None,
            period_usec: 100_000,
        }
    }

    /// Create a CPU max from a percentage (e.g., 50 = 50% of one CPU).
    pub fn from_percent(percent: u64) -> Self {
        Self {
            quota_usec: Some(percent * 1_000),
            period_usec: 100_000,
        }
    }
}

/// CPU usage statistics from cpu.stat.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CpuStat {
    pub usage_usec: u64,
    pub user_usec: u64,
    pub system_usec: u64,
    pub nr_periods: u64,
    pub nr_throttled: u64,
    pub throttled_usec: u64,
}

/// Controller for the cpu cgroup v2 subsystem.
pub struct CpuController {
    path: PathBuf,
}

impl CpuController {
    pub fn new(path: PathBuf) -> Self {
        Self { path }
    }

    /// Read cpu.max and return the current bandwidth limit.
    pub fn get_max(&self) -> Result<CpuMax> {
        let file = self.path.join("cpu.max");
        let content = read_cgroup_file(&file)?;
        parse_cpu_max(&file, &content)
    }

    /// Set cpu.max bandwidth limit.
    pub fn set_max(&self, max: &CpuMax) -> Result<()> {
        let file = self.path.join("cpu.max");
        let quota = match max.quota_usec {
            Some(q) => q.to_string(),
            None => "max".to_string(),
        };
        write_cgroup_file(&file, &format!("{quota} {}", max.period_usec))
    }

    /// Read the cpu.weight value (1-10000, default 100).
    pub fn get_weight(&self) -> Result<u64> {
        let file = self.path.join("cpu.weight");
        let content = read_cgroup_file(&file)?;
        content.parse::<u64>().map_err(|_| CgroupError::ParseError {
            path: file,
            content,
            detail: "expected u64".to_string(),
        })
    }

    /// Set cpu.weight (1-10000).
    pub fn set_weight(&self, weight: u64) -> Result<()> {
        if !(1..=10000).contains(&weight) {
            return Err(CgroupError::InvalidValue {
                field: "cpu.weight".to_string(),
                value: weight.to_string(),
                expected: "1-10000".to_string(),
            });
        }
        let file = self.path.join("cpu.weight");
        write_cgroup_file(&file, &weight.to_string())
    }

    /// Read cpu.stat.
    pub fn get_stat(&self) -> Result<CpuStat> {
        let file = self.path.join("cpu.stat");
        let entries = read_flat_keyed(&file)?;
        Ok(CpuStat {
            usage_usec: lookup_key(&entries, "usage_usec"),
            user_usec: lookup_key(&entries, "user_usec"),
            system_usec: lookup_key(&entries, "system_usec"),
            nr_periods: lookup_key(&entries, "nr_periods"),
            nr_throttled: lookup_key(&entries, "nr_throttled"),
            throttled_usec: lookup_key(&entries, "throttled_usec"),
        })
    }

    /// Read cpu.pressure (PSI).
    pub fn get_pressure(&self) -> Result<PressureStats> {
        crate::pressure::read_pressure(&self.path.join("cpu.pressure"))
    }
}

fn parse_cpu_max(path: &Path, content: &str) -> Result<CpuMax> {
    let mut parts = content.split_whitespace();
    let quota_str = parts.next().ok_or_else(|| CgroupError::ParseError {
        path: path.to_path_buf(),
        content: content.to_string(),
        detail: "empty cpu.max".to_string(),
    })?;
    let period_str = parts.next().unwrap_or("100000");

    let quota_usec = if quota_str == "max" {
        None
    } else {
        Some(
            quota_str
                .parse::<u64>()
                .map_err(|_| CgroupError::ParseError {
                    path: path.to_path_buf(),
                    content: content.to_string(),
                    detail: format!("invalid quota: {quota_str:?}"),
                })?,
        )
    };

    let period_usec = period_str
        .parse::<u64>()
        .map_err(|_| CgroupError::ParseError {
            path: path.to_path_buf(),
            content: content.to_string(),
            detail: format!("invalid period: {period_str:?}"),
        })?;

    Ok(CpuMax {
        quota_usec,
        period_usec,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn test_parse_cpu_max_unlimited() {
        let p = PathBuf::from("/test/cpu.max");
        let max = parse_cpu_max(&p, "max 100000").unwrap();
        assert!(max.quota_usec.is_none());
        assert_eq!(max.period_usec, 100_000);
    }

    #[test]
    fn test_parse_cpu_max_limited() {
        let p = PathBuf::from("/test/cpu.max");
        let max = parse_cpu_max(&p, "50000 100000").unwrap();
        assert_eq!(max.quota_usec, Some(50000));
        assert_eq!(max.period_usec, 100_000);
    }

    #[test]
    fn test_parse_cpu_max_no_period() {
        let p = PathBuf::from("/test/cpu.max");
        let max = parse_cpu_max(&p, "max").unwrap();
        assert!(max.quota_usec.is_none());
        assert_eq!(max.period_usec, 100_000);
    }

    #[test]
    fn test_cpu_max_unlimited() {
        let max = CpuMax::unlimited();
        assert!(max.quota_usec.is_none());
        assert_eq!(max.period_usec, 100_000);
    }

    #[test]
    fn test_cpu_max_from_percent() {
        let max = CpuMax::from_percent(50);
        assert_eq!(max.quota_usec, Some(50_000));
        assert_eq!(max.period_usec, 100_000);
    }
}
