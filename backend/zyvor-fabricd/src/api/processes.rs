// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: LicenseRef-Zyvor-Production-2.0

use axum::{extract::Query, http::StatusCode, Json};
use serde::{Deserialize, Serialize};
use std::process::Command;

use security::RequireRead;

#[derive(Debug, Deserialize)]
pub struct ListProcessesQuery {
    #[serde(default = "default_limit")]
    pub limit: u32,
}

fn default_limit() -> u32 {
    500
}

#[derive(Debug, Serialize)]
pub struct ProcessRow {
    pub pid: u32,
    pub name: String,
    pub cpu_percent: f64,
    pub memory_mb: f64,
    pub state: String,
    pub threads: u32,
}

#[derive(Debug, Deserialize)]
pub struct ProcessDetailQuery {
    pub pid: u32,
}

#[derive(Debug, Serialize)]
pub struct ProcessDetail {
    pub pid: u32,
    pub cmdline: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub io_read_bytes: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub io_write_bytes: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fds: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub voluntary_ctx_switches: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub involuntary_ctx_switches: Option<u64>,
}

fn validate_pid(pid: u32) -> Result<(), (StatusCode, String)> {
    if pid <= 1 {
        return Err((
            StatusCode::BAD_REQUEST,
            "invalid pid: must be greater than 1".into(),
        ));
    }
    if pid == std::process::id() {
        return Err((
            StatusCode::BAD_REQUEST,
            "invalid pid: cannot inspect the daemon process".into(),
        ));
    }
    if pid > 4_194_304 {
        return Err((StatusCode::BAD_REQUEST, "invalid pid: out of range".into()));
    }
    Ok(())
}

/// GET /api/system/processes - List host processes via ps.
pub async fn list_processes(
    RequireRead(_claims): RequireRead,
    Query(query): Query<ListProcessesQuery>,
) -> Result<Json<Vec<ProcessRow>>, (StatusCode, String)> {
    let limit = query.limit.clamp(1, 2000);
    let rows = tokio::task::spawn_blocking(move || list_processes_blocking(limit))
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("process listing task failed: {e}"),
            )
        })??;
    Ok(Json(rows))
}

/// GET /api/system/process?pid=N - Read /proc details for one process.
pub async fn get_process(
    RequireRead(_claims): RequireRead,
    Query(query): Query<ProcessDetailQuery>,
) -> Result<Json<ProcessDetail>, (StatusCode, String)> {
    validate_pid(query.pid)?;
    let pid = query.pid;
    let detail = tokio::task::spawn_blocking(move || read_process_detail(pid))
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("process detail task failed: {e}"),
            )
        })??;
    Ok(Json(detail))
}

fn list_processes_blocking(limit: u32) -> Result<Vec<ProcessRow>, (StatusCode, String)> {
    #[cfg(not(target_os = "linux"))]
    {
        let _ = limit;
        Ok(Vec::new())
    }
    #[cfg(target_os = "linux")]
    {
        list_processes_linux(limit)
    }
}

#[cfg(target_os = "linux")]
fn list_processes_linux(limit: u32) -> Result<Vec<ProcessRow>, (StatusCode, String)> {
    let output = Command::new("ps")
        .args(["-eo", "pid=,pcpu=,rss=,stat=,nlwp=,comm=", "--no-headers"])
        .output()
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("ps failed: {e}")))?;
    if !output.status.success() {
        return Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("ps failed: {}", String::from_utf8_lossy(&output.stderr)),
        ));
    }

    let text = String::from_utf8_lossy(&output.stdout);
    let mut rows = Vec::new();
    for line in text.lines() {
        if rows.len() >= limit as usize {
            break;
        }
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let mut parts = line.split_whitespace();
        let Some(pid_s) = parts.next() else { continue };
        let Some(pcpu_s) = parts.next() else { continue };
        let Some(rss_s) = parts.next() else { continue };
        let Some(stat) = parts.next() else { continue };
        let Some(nlwp_s) = parts.next() else { continue };
        let comm = parts.collect::<Vec<_>>().join(" ");
        let pid: u32 = pid_s.parse().unwrap_or(0);
        if pid == 0 {
            continue;
        }
        let cpu_percent: f64 = pcpu_s.parse().unwrap_or(0.0);
        let rss_kb: u64 = rss_s.parse().unwrap_or(0);
        let threads: u32 = nlwp_s.parse().unwrap_or(1);
        let state = stat
            .chars()
            .next()
            .map(|c| c.to_string())
            .unwrap_or_else(|| "?".to_string());
        let mut name = if comm.is_empty() {
            "?".to_string()
        } else {
            comm
        };
        if name.len() > 64 {
            name.truncate(61);
            name.push_str("...");
        }
        rows.push(ProcessRow {
            pid,
            name,
            cpu_percent,
            memory_mb: rss_kb as f64 / 1024.0,
            state,
            threads,
        });
    }
    Ok(rows)
}

/// Synchronous process list for host insight aggregation.
pub fn list_processes_sync(limit: u32) -> Vec<ProcessRow> {
    list_processes_blocking(limit).unwrap_or_default()
}

fn read_process_detail(pid: u32) -> Result<ProcessDetail, (StatusCode, String)> {
    #[cfg(not(target_os = "linux"))]
    {
        let _ = pid;
        Err((
            StatusCode::NOT_IMPLEMENTED,
            "process details are only available on Linux".into(),
        ))
    }
    #[cfg(target_os = "linux")]
    {
        read_process_detail_linux(pid)
    }
}

#[cfg(target_os = "linux")]
fn read_process_detail_linux(pid: u32) -> Result<ProcessDetail, (StatusCode, String)> {
    let pid_s = pid.to_string();
    if pid_s.contains("..") {
        return Err((StatusCode::BAD_REQUEST, "invalid pid".into()));
    }
    let proc_dir = format!("/proc/{pid_s}");
    if !std::path::Path::new(&proc_dir).exists() {
        return Err((StatusCode::NOT_FOUND, format!("process {pid} not found")));
    }

    let cmdline = read_proc_cmdline(pid);
    let (io_read_bytes, io_write_bytes) = read_proc_io(pid);
    let fds = count_proc_fds(pid);
    let (voluntary_ctx_switches, involuntary_ctx_switches) = read_proc_status(pid);

    Ok(ProcessDetail {
        pid,
        cmdline,
        io_read_bytes,
        io_write_bytes,
        fds,
        voluntary_ctx_switches,
        involuntary_ctx_switches,
    })
}

#[cfg(target_os = "linux")]
fn read_proc_cmdline(pid: u32) -> String {
    let pid_s = pid.to_string();
    if pid_s.contains("..") {
        return String::new();
    }
    let path = format!("/proc/{pid_s}/cmdline");
    std::fs::read(&path)
        .map(|b| {
            let s = String::from_utf8_lossy(&b)
                .replace('\0', " ")
                .trim()
                .to_string();
            if s.len() > 280 {
                format!("{}...", &s[..277])
            } else {
                s
            }
        })
        .unwrap_or_default()
}

#[cfg(target_os = "linux")]
fn read_proc_io(pid: u32) -> (Option<u64>, Option<u64>) {
    let pid_s = pid.to_string();
    if pid_s.contains("..") {
        return (None, None);
    }
    let path = format!("/proc/{pid_s}/io");
    let Ok(content) = std::fs::read_to_string(&path) else {
        return (None, None);
    };
    let mut read_bytes = None;
    let mut write_bytes = None;
    for line in content.lines() {
        if let Some(v) = line.strip_prefix("read_bytes:") {
            read_bytes = v.trim().parse().ok();
        } else if let Some(v) = line.strip_prefix("write_bytes:") {
            write_bytes = v.trim().parse().ok();
        }
    }
    (read_bytes, write_bytes)
}

#[cfg(target_os = "linux")]
fn count_proc_fds(pid: u32) -> Option<u32> {
    let pid_s = pid.to_string();
    if pid_s.contains("..") {
        return None;
    }
    let path = format!("/proc/{pid_s}/fd");
    std::fs::read_dir(&path)
        .ok()
        .map(|entries| entries.filter_map(Result::ok).count() as u32)
}

#[cfg(target_os = "linux")]
fn read_proc_status(pid: u32) -> (Option<u64>, Option<u64>) {
    let pid_s = pid.to_string();
    if pid_s.contains("..") {
        return (None, None);
    }
    let path = format!("/proc/{pid_s}/status");
    let Ok(content) = std::fs::read_to_string(&path) else {
        return (None, None);
    };
    let mut voluntary = None;
    let mut involuntary = None;
    for line in content.lines() {
        if let Some(v) = line.strip_prefix("voluntary_ctxt_switches:") {
            voluntary = v.trim().parse().ok();
        } else if let Some(v) = line.strip_prefix("nonvoluntary_ctxt_switches:") {
            involuntary = v.trim().parse().ok();
        }
    }
    (voluntary, involuntary)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validate_pid_rejects_invalid() {
        assert!(validate_pid(0).is_err());
        assert!(validate_pid(1).is_err());
        assert!(validate_pid(std::process::id()).is_err());
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn list_processes_returns_rows() {
        let rows = list_processes_linux(5).unwrap();
        assert!(!rows.is_empty());
        assert!(rows[0].pid > 0);
        assert!(!rows[0].name.is_empty());
    }
}
