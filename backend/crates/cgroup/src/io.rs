// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: LicenseRef-Zyvor-Production-2.0

use std::fmt;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::error::{CgroupError, Result};
use crate::pressure::PressureStats;
use crate::util::{read_cgroup_file, write_cgroup_file};

/// Block device identifier (major:minor).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct DeviceId {
    pub major: u32,
    pub minor: u32,
}

impl fmt::Display for DeviceId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}:{}", self.major, self.minor)
    }
}

impl DeviceId {
    /// Parse "major:minor" string.
    pub fn parse(s: &str) -> Option<Self> {
        let (major, minor) = s.split_once(':')?;
        Some(Self {
            major: major.parse().ok()?,
            minor: minor.parse().ok()?,
        })
    }
}

/// Per-device I/O limits from io.max.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IoMax {
    /// Read bytes per second limit, None = unlimited.
    pub rbps: Option<u64>,
    /// Write bytes per second limit, None = unlimited.
    pub wbps: Option<u64>,
    /// Read I/O operations per second limit, None = unlimited.
    pub riops: Option<u64>,
    /// Write I/O operations per second limit, None = unlimited.
    pub wiops: Option<u64>,
}

/// Per-device I/O statistics from io.stat.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IoStat {
    pub device: DeviceId,
    pub rbytes: u64,
    pub wbytes: u64,
    pub rios: u64,
    pub wios: u64,
    pub dbytes: u64,
    pub dios: u64,
}

/// Controller for the io cgroup v2 subsystem.
pub struct IoController {
    path: PathBuf,
}

impl IoController {
    pub fn new(path: PathBuf) -> Self {
        Self { path }
    }

    /// Read io.weight (1-10000, default 100).
    pub fn get_weight(&self) -> Result<u64> {
        let file = self.path.join("io.weight");
        let content = read_cgroup_file(&file)?;
        // io.weight may contain "default <value>" or just "<value>"
        let value_str = content.strip_prefix("default ").unwrap_or(&content);
        value_str
            .parse::<u64>()
            .map_err(|_| CgroupError::ParseError {
                path: file,
                content,
                detail: "expected u64 for io.weight".to_string(),
            })
    }

    /// Set io.weight (1-10000).
    pub fn set_weight(&self, weight: u64) -> Result<()> {
        if !(1..=10000).contains(&weight) {
            return Err(CgroupError::InvalidValue {
                field: "io.weight".to_string(),
                value: weight.to_string(),
                expected: "1-10000".to_string(),
            });
        }
        let file = self.path.join("io.weight");
        write_cgroup_file(&file, &format!("default {weight}"))
    }

    /// Read io.max for a specific device.
    pub fn get_max(&self, device: DeviceId) -> Result<Option<IoMax>> {
        let file = self.path.join("io.max");
        let content = read_cgroup_file(&file)?;
        parse_io_max_for_device(&file, &content, device)
    }

    /// Set io.max for a specific device.
    pub fn set_max(&self, device: DeviceId, limits: &IoMax) -> Result<()> {
        let file = self.path.join("io.max");
        let rbps = format_limit(limits.rbps);
        let wbps = format_limit(limits.wbps);
        let riops = format_limit(limits.riops);
        let wiops = format_limit(limits.wiops);
        let line = format!("{device} rbps={rbps} wbps={wbps} riops={riops} wiops={wiops}");
        write_cgroup_file(&file, &line)
    }

    /// Read io.stat.
    pub fn get_stat(&self) -> Result<Vec<IoStat>> {
        let file = self.path.join("io.stat");
        let content = read_cgroup_file(&file)?;
        parse_io_stat(&file, &content)
    }

    /// Read io.pressure (PSI).
    pub fn get_pressure(&self) -> Result<PressureStats> {
        crate::pressure::read_pressure(&self.path.join("io.pressure"))
    }
}

fn format_limit(limit: Option<u64>) -> String {
    match limit {
        Some(v) => v.to_string(),
        None => "max".to_string(),
    }
}

fn parse_io_max_for_device(path: &Path, content: &str, target: DeviceId) -> Result<Option<IoMax>> {
    for line in content.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let mut parts = line.split_whitespace();
        let dev_str = match parts.next() {
            Some(s) => s,
            None => continue,
        };
        let dev = DeviceId::parse(dev_str).ok_or_else(|| CgroupError::ParseError {
            path: path.to_path_buf(),
            content: line.to_string(),
            detail: format!("invalid device id: {dev_str:?}"),
        })?;
        if dev != target {
            continue;
        }

        let mut rbps = None;
        let mut wbps = None;
        let mut riops = None;
        let mut wiops = None;

        for token in parts {
            if let Some((key, value)) = token.split_once('=') {
                let parsed = parse_io_limit(value);
                match key {
                    "rbps" => rbps = parsed,
                    "wbps" => wbps = parsed,
                    "riops" => riops = parsed,
                    "wiops" => wiops = parsed,
                    _ => {}
                }
            }
        }

        return Ok(Some(IoMax {
            rbps,
            wbps,
            riops,
            wiops,
        }));
    }
    Ok(None)
}

fn parse_io_limit(s: &str) -> Option<u64> {
    if s == "max" {
        None
    } else {
        s.parse().ok()
    }
}

fn parse_io_stat(path: &Path, content: &str) -> Result<Vec<IoStat>> {
    let mut stats = Vec::new();
    for line in content.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let mut parts = line.split_whitespace();
        let dev_str = match parts.next() {
            Some(s) => s,
            None => continue,
        };
        let device = DeviceId::parse(dev_str).ok_or_else(|| CgroupError::ParseError {
            path: path.to_path_buf(),
            content: line.to_string(),
            detail: format!("invalid device id: {dev_str:?}"),
        })?;

        let mut rbytes = 0;
        let mut wbytes = 0;
        let mut rios = 0;
        let mut wios = 0;
        let mut dbytes = 0;
        let mut dios = 0;

        for token in parts {
            if let Some((key, value)) = token.split_once('=') {
                let v = value.parse::<u64>().unwrap_or(0);
                match key {
                    "rbytes" => rbytes = v,
                    "wbytes" => wbytes = v,
                    "rios" => rios = v,
                    "wios" => wios = v,
                    "dbytes" => dbytes = v,
                    "dios" => dios = v,
                    _ => {}
                }
            }
        }

        stats.push(IoStat {
            device,
            rbytes,
            wbytes,
            rios,
            wios,
            dbytes,
            dios,
        });
    }
    Ok(stats)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn test_device_id_parse() {
        let dev = DeviceId::parse("8:0").unwrap();
        assert_eq!(dev.major, 8);
        assert_eq!(dev.minor, 0);
        assert_eq!(dev.to_string(), "8:0");
    }

    #[test]
    fn test_device_id_parse_invalid() {
        assert!(DeviceId::parse("invalid").is_none());
        assert!(DeviceId::parse("a:b").is_none());
        assert!(DeviceId::parse("").is_none());
    }

    #[test]
    fn test_parse_io_stat() {
        let p = PathBuf::from("/test/io.stat");
        let content = "8:0 rbytes=1024 wbytes=2048 rios=10 wios=20 dbytes=0 dios=0\n\
                        259:0 rbytes=4096 wbytes=8192 rios=5 wios=15 dbytes=512 dios=1\n";
        let stats = parse_io_stat(&p, content).unwrap();
        assert_eq!(stats.len(), 2);

        assert_eq!(stats[0].device, DeviceId { major: 8, minor: 0 });
        assert_eq!(stats[0].rbytes, 1024);
        assert_eq!(stats[0].wbytes, 2048);
        assert_eq!(stats[0].rios, 10);
        assert_eq!(stats[0].wios, 20);

        assert_eq!(
            stats[1].device,
            DeviceId {
                major: 259,
                minor: 0
            }
        );
        assert_eq!(stats[1].rbytes, 4096);
        assert_eq!(stats[1].dbytes, 512);
        assert_eq!(stats[1].dios, 1);
    }

    #[test]
    fn test_parse_io_max() {
        let p = PathBuf::from("/test/io.max");
        let content = "8:0 rbps=1048576 wbps=max riops=100 wiops=max\n";
        let dev = DeviceId { major: 8, minor: 0 };
        let max = parse_io_max_for_device(&p, content, dev).unwrap().unwrap();
        assert_eq!(max.rbps, Some(1048576));
        assert!(max.wbps.is_none());
        assert_eq!(max.riops, Some(100));
        assert!(max.wiops.is_none());
    }

    #[test]
    fn test_parse_io_max_device_not_found() {
        let p = PathBuf::from("/test/io.max");
        let content = "8:0 rbps=max wbps=max riops=max wiops=max\n";
        let dev = DeviceId {
            major: 259,
            minor: 0,
        };
        let result = parse_io_max_for_device(&p, content, dev).unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn test_parse_io_stat_empty() {
        let p = PathBuf::from("/test/io.stat");
        let stats = parse_io_stat(&p, "").unwrap();
        assert!(stats.is_empty());
    }
}
