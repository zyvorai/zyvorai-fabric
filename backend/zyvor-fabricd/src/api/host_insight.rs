// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: LicenseRef-Zyvor-Production-2.0

use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    Json,
};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::process::Command;

use crate::api::{backups, images};
use crate::server::AppState;
use security::RequireRead;

#[derive(Debug, Serialize)]
pub struct DebugOutput {
    pub lines: Vec<String>,
}

#[derive(Debug, Deserialize)]
pub struct TimeseriesQuery {
    #[serde(default = "default_period")]
    pub period: String,
    #[serde(default = "default_metric")]
    pub metric: String,
}

fn default_metric() -> String {
    "cpu".to_string()
}

fn default_period() -> String {
    "1h".to_string()
}

fn read_file(path: &str) -> Option<String> {
    std::fs::read_to_string(path).ok()
}

fn parse_loadavg() -> (f64, f64, f64) {
    read_file("/proc/loadavg")
        .and_then(|s| {
            let parts: Vec<&str> = s.split_whitespace().collect();
            if parts.len() >= 3 {
                Some((
                    parts[0].parse().unwrap_or(0.0),
                    parts[1].parse().unwrap_or(0.0),
                    parts[2].parse().unwrap_or(0.0),
                ))
            } else {
                None
            }
        })
        .unwrap_or((0.0, 0.0, 0.0))
}

fn parse_cpu_usage() -> (f64, u32) {
    let content = match read_file("/proc/stat") {
        Some(c) => c,
        None => return (0.0, 1),
    };
    let line = match content.lines().next() {
        Some(l) => l,
        None => return (0.0, 1),
    };
    let nums: Vec<u64> = line
        .split_whitespace()
        .skip(1)
        .filter_map(|s| s.parse().ok())
        .collect();
    if nums.len() < 4 {
        return (0.0, nums.len().max(1) as u32);
    }
    let idle = nums.get(3).copied().unwrap_or(0) + nums.get(4).copied().unwrap_or(0);
    let total: u64 = nums.iter().sum();
    let usage = if total > 0 {
        100.0 * (1.0 - idle as f64 / total as f64)
    } else {
        0.0
    };
    let cores = std::thread::available_parallelism()
        .map(|n| n.get() as u32)
        .unwrap_or(1);
    (usage.clamp(0.0, 100.0), cores)
}

fn parse_meminfo() -> serde_json::Value {
    let content = match read_file("/proc/meminfo") {
        Some(c) => c,
        None => {
            return serde_json::json!({
                "usage_percent": 0,
                "used_bytes": 0,
                "total_bytes": 0,
                "available_bytes": 0,
                "cached_bytes": 0,
                "swap_percent": 0,
                "swap_used_bytes": 0,
                "swap_total_bytes": 0
            });
        }
    };
    let mut kv = std::collections::HashMap::new();
    for line in content.lines() {
        if let Some((k, v)) = line.split_once(':') {
            let val: u64 = v
                .split_whitespace()
                .next()
                .unwrap_or("0")
                .parse()
                .unwrap_or(0);
            kv.insert(k.trim().to_string(), val * 1024);
        }
    }
    let total = *kv.get("MemTotal").unwrap_or(&0);
    let available = *kv.get("MemAvailable").unwrap_or(&0);
    let cached = *kv.get("Cached").unwrap_or(&0);
    let used = total.saturating_sub(available);
    let swap_total = *kv.get("SwapTotal").unwrap_or(&0);
    let swap_free = *kv.get("SwapFree").unwrap_or(&0);
    let swap_used = swap_total.saturating_sub(swap_free);
    let usage_pct = if total > 0 {
        100.0 * used as f64 / total as f64
    } else {
        0.0
    };
    let swap_pct = if swap_total > 0 {
        100.0 * swap_used as f64 / swap_total as f64
    } else {
        0.0
    };
    serde_json::json!({
        "usage_percent": usage_pct,
        "used_bytes": used,
        "total_bytes": total,
        "available_bytes": available,
        "cached_bytes": cached,
        "swap_percent": swap_pct,
        "swap_used_bytes": swap_used,
        "swap_total_bytes": swap_total
    })
}

fn parse_filesystems() -> Vec<serde_json::Value> {
    let output = std::process::Command::new("df")
        .args(["-B1", "-T"])
        .output()
        .ok();
    let stdout = match output {
        Some(o) if o.status.success() => String::from_utf8_lossy(&o.stdout).to_string(),
        _ => return Vec::new(),
    };
    let mut rows = Vec::new();
    for line in stdout.lines().skip(1) {
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() < 7 {
            continue;
        }
        let total: u64 = parts[2].parse().unwrap_or(0);
        let used: u64 = parts[3].parse().unwrap_or(0);
        let pct = if total > 0 {
            100.0 * used as f64 / total as f64
        } else {
            0.0
        };
        rows.push(serde_json::json!({
            "mountpoint": parts[6],
            "fs_type": parts[1],
            "total_bytes": total,
            "used_bytes": used,
            "usage_percent": pct
        }));
    }
    rows
}

fn parse_net_dev() -> serde_json::Value {
    let content = match read_file("/proc/net/dev") {
        Some(c) => c,
        None => {
            return serde_json::json!({
                "interfaces": [],
                "total_rx_bytes": 0,
                "total_tx_bytes": 0
            });
        }
    };
    let mut interfaces = Vec::new();
    let mut total_rx = 0u64;
    let mut total_tx = 0u64;
    for line in content.lines().skip(2) {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let (name, rest) = match line.split_once(':') {
            Some(p) => p,
            None => continue,
        };
        let name = name.trim();
        if name == "lo" {
            continue;
        }
        let nums: Vec<u64> = rest
            .split_whitespace()
            .filter_map(|s| s.parse().ok())
            .collect();
        if nums.len() < 16 {
            continue;
        }
        let rx = nums[0];
        let tx = nums[8];
        total_rx += rx;
        total_tx += tx;
        interfaces.push(serde_json::json!({
            "name": name,
            "rx_bytes": rx,
            "tx_bytes": tx,
            "rx_errors": nums[2],
            "tx_errors": nums[10]
        }));
    }
    serde_json::json!({
        "interfaces": interfaces,
        "total_rx_bytes": total_rx,
        "total_tx_bytes": total_tx,
        "tcp_states": {},
        "retransmits": 0
    })
}

async fn run_command_lines(program: &str, args: &[&str], max_lines: usize) -> Vec<String> {
    let output = Command::new(program).args(args).output().await.ok();
    match output {
        Some(o) if o.status.success() || !o.stdout.is_empty() => String::from_utf8_lossy(&o.stdout)
            .lines()
            .take(max_lines)
            .map(|s| s.to_string())
            .collect(),
        _ => vec![format!("{} unavailable on this host", program)],
    }
}

/// GET /api/system/info
pub async fn get_system_info(RequireRead(_claims): RequireRead) -> Json<serde_json::Value> {
    let (cpu_pct, cores) = parse_cpu_usage();
    let (load1, load5, load15) = parse_loadavg();
    let memory = parse_meminfo();
    let mem_pct = memory
        .get("usage_percent")
        .and_then(|v| v.as_f64())
        .unwrap_or(0.0);

    let mut score = 100i32;
    if cpu_pct > 90.0 {
        score -= 30;
    } else if cpu_pct > 75.0 {
        score -= 15;
    }
    if mem_pct > 90.0 {
        score -= 30;
    } else if mem_pct > 80.0 {
        score -= 10;
    }
    let status = if score >= 80 {
        "healthy"
    } else if score >= 50 {
        "degraded"
    } else {
        "critical"
    };

    let processes = crate::api::processes::list_processes_sync(10);
    let top_cpu: Vec<_> = processes
        .iter()
        .take(5)
        .map(|p| {
            serde_json::json!({
                "pid": p.pid,
                "name": p.name,
                "cpu_percent": p.cpu_percent,
                "memory_mb": p.memory_mb
            })
        })
        .collect();

    Json(serde_json::json!({
        "health": {
            "score": score,
            "status": status,
            "summary": format!("CPU {:.1}%, memory {:.1}%", cpu_pct, mem_pct)
        },
        "cpu": {
            "usage_percent": cpu_pct,
            "core_count": cores,
            "load_avg_1": load1,
            "load_avg_5": load5,
            "load_avg_15": load15,
            "cores": []
        },
        "memory": memory,
        "disks": [],
        "filesystems": parse_filesystems(),
        "network": parse_net_dev(),
        "processes": {
            "total": processes.len(),
            "top_cpu": top_cpu,
            "top_memory": top_cpu
        }
    }))
}

/// GET /api/system/metrics
pub async fn get_system_metrics(RequireRead(_claims): RequireRead) -> Json<serde_json::Value> {
    let (cpu_pct, _) = parse_cpu_usage();
    let memory = parse_meminfo();
    let mem_pct = memory
        .get("usage_percent")
        .and_then(|v| v.as_f64())
        .unwrap_or(0.0);
    let net = parse_net_dev();
    Json(serde_json::json!({
        "cpu_percent": cpu_pct,
        "memory_percent": mem_pct,
        "disk_read_bytes": 0,
        "disk_write_bytes": 0,
        "net_rx_bytes": net.get("total_rx_bytes").cloned().unwrap_or(serde_json::json!(0)),
        "net_tx_bytes": net.get("total_tx_bytes").cloned().unwrap_or(serde_json::json!(0))
    }))
}

/// GET /api/system/kernel
pub async fn get_kernel_info(RequireRead(_claims): RequireRead) -> Json<serde_json::Value> {
    let version = read_file("/proc/version").unwrap_or_default();
    let hostname = read_file("/proc/sys/kernel/hostname").unwrap_or_default();
    let uname = run_command_lines("uname", &["-a"], 1).await;
    let modules = run_command_lines("lsmod", &[], 50).await;
    let module_objs: Vec<serde_json::Value> = modules
        .iter()
        .skip(1)
        .filter_map(|line| {
            let name = line.split_whitespace().next()?;
            Some(serde_json::json!({ "name": name }))
        })
        .collect();
    Json(serde_json::json!({
        "version": version.trim(),
        "kernel_version": version.trim(),
        "hostname": hostname.trim(),
        "architecture": std::env::consts::ARCH,
        "cmdline": read_file("/proc/cmdline").unwrap_or_default().trim(),
        "uname": uname.first().cloned().unwrap_or_default(),
        "modules": module_objs
    }))
}

/// GET /api/system/containers
pub async fn get_containers(
    RequireRead(_claims): RequireRead,
    State(state): State<Arc<AppState>>,
) -> Json<serde_json::Value> {
    let machines = state.driver.list_machines().await.unwrap_or_default();
    let mut containers = Vec::with_capacity(machines.len());
    let mut total_cpu = 0.0;
    let mut total_memory = 0u64;
    for m in &machines {
        // Metrics collection is best-effort per machine (e.g. a machine
        // that just stopped mid-scan) -- fall back to zeroed stats rather
        // than dropping the whole row, matching list_machines' own
        // unwrap_or_default() above.
        let metrics = state
            .driver
            .get_metrics(&m.name)
            .await
            .unwrap_or(vm_model::VMMetrics {
                cpu_usage: 0.0,
                memory_usage: 0,
                disk_usage: 0,
                network_rx: 0,
                network_tx: 0,
            });
        let memory_limit = state
            .store
            .get_vm(&m.name)
            .ok()
            .flatten()
            .map(|vm| vm.memory * 1024 * 1024);
        total_cpu += metrics.cpu_usage;
        total_memory += metrics.memory_usage;
        containers.push(serde_json::json!({
            "id": m.name,
            "name": m.name,
            "state": format!("{:?}", m.state).to_lowercase(),
            "image": "",
            "cpu_percent": metrics.cpu_usage,
            "memory_bytes": metrics.memory_usage,
            "memory_limit": memory_limit,
            "net_rx": metrics.network_rx,
            "net_tx": metrics.network_tx,
        }));
    }
    let running = machines
        .iter()
        .filter(|m| matches!(m.state, vm_model::VMState::Running))
        .count();
    Json(serde_json::json!({
        "summary": {
            "total": containers.len(),
            "running": running,
            "total_cpu": total_cpu,
            "total_memory": total_memory
        },
        "containers": containers
    }))
}

/// GET /api/system/debug/:tool
pub async fn get_debug_output(
    RequireRead(_claims): RequireRead,
    Path(tool): Path<String>,
) -> Result<Json<DebugOutput>, (StatusCode, String)> {
    let lines = match tool.as_str() {
        "top" => run_command_lines("ps", &["aux", "--sort=-%cpu"], 30).await,
        "iostat" => run_command_lines("iostat", &[], 40).await,
        "vmstat" => run_command_lines("vmstat", &[], 20).await,
        "netstat" => run_command_lines("ss", &["-tunap"], 50).await,
        other => {
            return Err((
                StatusCode::BAD_REQUEST,
                format!("unknown debug tool: {}", other),
            ))
        }
    };
    Ok(Json(DebugOutput { lines }))
}

/// GET /api/system/security
pub async fn get_security_summary(
    RequireRead(_claims): RequireRead,
    State(state): State<Arc<AppState>>,
) -> Json<serde_json::Value> {
    let cert_count = state
        .store
        .list_entities::<serde_json::Value>("certificates")
        .unwrap_or_default()
        .len();
    let compliance_results: Vec<compliance::ComplianceScanResult> = state
        .store
        .list_entities("compliance_results")
        .unwrap_or_default();
    let non_compliant = compliance_results
        .iter()
        .filter(|r| !matches!(r.overall_status, compliance::ScanStatus::Compliant))
        .count();
    let ports = run_command_lines("ss", &["-tln"], 30).await;
    let listening: Vec<serde_json::Value> = ports
        .iter()
        .skip(1)
        .filter_map(|line| {
            let parts: Vec<&str> = line.split_whitespace().collect();
            let addr = parts.get(3)?;
            let port = addr.rsplit(':').next()?;
            Some(serde_json::json!({
                "port": port.parse::<u32>().unwrap_or(0),
                "protocol": "tcp"
            }))
        })
        .collect();
    let failed_logins: Vec<serde_json::Value> = run_command_lines(
        "journalctl",
        &[
            "-u",
            "ssh",
            "-u",
            "sshd",
            "--no-pager",
            "-n",
            "50",
            "-o",
            "short-iso",
        ],
        50,
    )
    .await
    .iter()
    .filter(|l| l.contains("Failed") || l.contains("Invalid user"))
    .take(10)
    .map(|l| serde_json::json!({ "message": l.trim() }))
    .collect();
    let exposed_ports = listening.len();
    let mut risk_score = 20i32;
    risk_score += (exposed_ports as i32).min(30);
    risk_score += (non_compliant as i32) * 10;
    risk_score += failed_logins.len() as i32 * 3;
    if cert_count == 0 {
        risk_score += 10;
    }
    risk_score = risk_score.min(100);
    let alerts: Vec<serde_json::Value> = state
        .net_monitor
        .evaluator
        .get_active_alerts()
        .await
        .into_iter()
        .map(|a| {
            serde_json::json!({
                "severity": format!("{:?}", a.severity).to_lowercase(),
                "message": format!(
                    "{} bandwidth on {} ({:.0} bps, threshold {})",
                    format!("{:?}", a.direction).to_lowercase(),
                    a.vm_name,
                    a.actual_bps,
                    a.threshold_bps
                ),
                "source": a.policy_name
            })
        })
        .collect();
    Json(serde_json::json!({
        "risk_score": risk_score,
        "alerts": alerts,
        "failed_logins": failed_logins,
        "listening_ports": listening,
        "certificates_tracked": cert_count,
        "compliance_scans": compliance_results.len()
    }))
}

/// GET /api/system/alerts
///
/// Merges two independent alert sources into the one shape the "Alerts"
/// page renders: live bandwidth alerts from `net_monitor` (already
/// continuously evaluated by the background task below), and cpu/memory/disk
/// threshold breaches evaluated here, on read, against the rules from
/// `/system/alerts/rules`. Before this, the rules endpoint only ever
/// returned static, never-evaluated defaults -- a host pinned at 100% CPU
/// for a week would never produce a "High CPU usage" alert, because nothing
/// ever checked the rule against a real metric.
pub async fn get_system_alerts(
    RequireRead(_claims): RequireRead,
    State(state): State<Arc<AppState>>,
) -> Json<serde_json::Value> {
    let bandwidth_alerts: Vec<serde_json::Value> = state
        .net_monitor
        .evaluator
        .get_active_alerts()
        .await
        .into_iter()
        .map(|a| {
            serde_json::json!({
                "id": a.id,
                "severity": a.severity,
                "title": format!("{:?} bandwidth on {}", a.direction, a.vm_name),
                "message": format!(
                    "{:?} bandwidth on {} is {:.0} bps (threshold {} bps)",
                    a.direction, a.vm_name, a.actual_bps, a.threshold_bps
                ),
                "value": a.actual_bps,
                "timestamp": a.triggered_at,
            })
        })
        .collect();
    let rule_alerts = evaluate_alert_rules(&load_alert_rules(&state));
    let alerts: Vec<serde_json::Value> = bandwidth_alerts.into_iter().chain(rule_alerts).collect();
    Json(serde_json::json!({ "alerts": alerts }))
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct AlertRule {
    id: String,
    name: String,
    metric: String,
    condition: String,
    threshold: f64,
    severity: String,
    enabled: bool,
}

fn default_alert_rules() -> Vec<AlertRule> {
    vec![
        AlertRule {
            id: "cpu-high".into(),
            name: "High CPU usage".into(),
            metric: "cpu".into(),
            condition: "gt".into(),
            threshold: 85.0,
            severity: "warning".into(),
            enabled: true,
        },
        AlertRule {
            id: "mem-high".into(),
            name: "High memory usage".into(),
            metric: "memory".into(),
            condition: "gt".into(),
            threshold: 90.0,
            severity: "critical".into(),
            enabled: true,
        },
        AlertRule {
            id: "disk-high".into(),
            name: "Disk nearly full".into(),
            metric: "disk".into(),
            condition: "gt".into(),
            threshold: 90.0,
            severity: "critical".into(),
            enabled: true,
        },
    ]
}

/// Loads persisted alert rules, seeding the defaults on first read. Shared by
/// `get_alert_rules` (which just displays them) and `get_system_alerts`
/// (which actually evaluates them).
fn load_alert_rules(state: &AppState) -> Vec<AlertRule> {
    let mut rules: Vec<AlertRule> = state.store.list_entities("alert_rules").unwrap_or_default();
    if rules.is_empty() {
        rules = default_alert_rules();
        for rule in &rules {
            let _ = state.store.save_entity("alert_rules", &rule.id, rule);
        }
    }
    rules
}

/// Reads the live value for a rule's `metric` and compares it against
/// `threshold` per `condition`. Unknown metrics/conditions never match,
/// rather than falling back to some default comparison that could fire
/// spuriously.
fn rule_metric_value(metric: &str) -> Option<f64> {
    match metric {
        "cpu" => Some(parse_cpu_usage().0),
        "memory" => parse_meminfo()
            .get("usage_percent")
            .and_then(|v| v.as_f64()),
        "disk" => parse_filesystems()
            .into_iter()
            .find(|fs| fs.get("mountpoint").and_then(|m| m.as_str()) == Some("/"))
            .and_then(|fs| fs.get("usage_percent").and_then(|v| v.as_f64())),
        _ => None,
    }
}

fn condition_breached(condition: &str, value: f64, threshold: f64) -> bool {
    match condition {
        "gt" => value > threshold,
        "gte" => value >= threshold,
        "lt" => value < threshold,
        "lte" => value <= threshold,
        _ => false,
    }
}

fn evaluate_alert_rules(rules: &[AlertRule]) -> Vec<serde_json::Value> {
    let now = Utc::now();
    rules
        .iter()
        .filter(|rule| rule.enabled)
        .filter_map(|rule| {
            let value = rule_metric_value(&rule.metric)?;
            if !condition_breached(&rule.condition, value, rule.threshold) {
                return None;
            }
            Some(serde_json::json!({
                "id": rule.id,
                "severity": rule.severity,
                "title": rule.name,
                "message": format!(
                    "{} is {:.1} ({} {})",
                    rule.name, value, rule.condition, rule.threshold
                ),
                "value": value,
                "timestamp": now,
            }))
        })
        .collect()
}

/// GET /api/system/alerts/rules
pub async fn get_alert_rules(
    RequireRead(_claims): RequireRead,
    State(state): State<Arc<AppState>>,
) -> Json<serde_json::Value> {
    Json(serde_json::json!({ "rules": load_alert_rules(&state) }))
}

/// GET /api/system/explain/:metric
pub async fn explain_metric(
    RequireRead(_claims): RequireRead,
    Path(metric): Path<String>,
) -> Json<serde_json::Value> {
    let (value, unit, status) = match metric.as_str() {
        "cpu" => {
            let (pct, _) = parse_cpu_usage();
            (pct, "%", if pct > 85.0 { "warning" } else { "ok" })
        }
        "memory" => {
            let mem = parse_meminfo();
            let pct = mem
                .get("usage_percent")
                .and_then(|v| v.as_f64())
                .unwrap_or(0.0);
            (pct, "%", if pct > 85.0 { "warning" } else { "ok" })
        }
        "disk" => {
            let fs = parse_filesystems();
            let max = fs
                .iter()
                .filter_map(|f| f.get("usage_percent").and_then(|v| v.as_f64()))
                .fold(0.0f64, f64::max);
            (max, "%", if max > 90.0 { "critical" } else { "ok" })
        }
        "network" => {
            let net = parse_net_dev();
            let rx = net
                .get("total_rx_bytes")
                .and_then(|v| v.as_u64())
                .unwrap_or(0) as f64;
            (rx / 1_048_576.0, "MB total rx", "ok")
        }
        _ => (0.0, "", "unknown"),
    };
    Json(serde_json::json!({
        "current_value": value,
        "unit": unit,
        "trend": "stable",
        "status": status,
        "summary": format!("Current {} reading from host", metric),
        "factors": [],
        "recommendations": []
    }))
}

/// GET /api/system/timeseries
pub async fn get_timeseries(
    RequireRead(_claims): RequireRead,
    State(state): State<Arc<AppState>>,
    Query(query): Query<TimeseriesQuery>,
) -> Json<serde_json::Value> {
    let key = format!("metrics-system-{}", query.period);
    let legacy = format!("metrics/system/{}", query.period);
    let entries: Vec<serde_json::Value> = state
        .store
        .get_entity("performance", &key)
        .ok()
        .flatten()
        .or_else(|| {
            state
                .store
                .get_entity("performance", &legacy)
                .ok()
                .flatten()
        })
        .unwrap_or_default();

    let field = match query.metric.as_str() {
        "cpu" => "total_cpu_usage",
        "memory" => "total_memory_usage",
        "network" => "total_network_rx",
        _ => "total_cpu_usage",
    };

    let mut points: Vec<serde_json::Value> = entries
        .iter()
        .filter_map(|e| {
            Some(serde_json::json!({
                "value": e.get(field)?.as_f64()?,
                "timestamp": e.get("timestamp")?.as_str().unwrap_or("")
            }))
        })
        .collect();
    if points.is_empty() {
        let (value, _) = match query.metric.as_str() {
            "cpu" => {
                let (pct, _) = parse_cpu_usage();
                (pct, "%")
            }
            "memory" => {
                let mem = parse_meminfo();
                (
                    mem.get("usage_percent")
                        .and_then(|v| v.as_f64())
                        .unwrap_or(0.0),
                    "%",
                )
            }
            "network" => {
                let net = parse_net_dev();
                (
                    net.get("total_rx_bytes")
                        .and_then(|v| v.as_u64())
                        .unwrap_or(0) as f64,
                    "bytes",
                )
            }
            _ => {
                let (pct, _) = parse_cpu_usage();
                (pct, "%")
            }
        };
        let now = Utc::now().to_rfc3339();
        points.push(serde_json::json!({ "value": value, "timestamp": now }));
    }
    Json(serde_json::json!({ "points": points }))
}

/// GET /api/system/compliance
pub async fn get_system_compliance(
    RequireRead(_claims): RequireRead,
    State(state): State<Arc<AppState>>,
) -> Json<serde_json::Value> {
    let results: Vec<compliance::ComplianceScanResult> = state
        .store
        .list_entities("compliance_results")
        .unwrap_or_default();

    let mut passed = 0u32;
    let warnings = 0u32;
    let mut failed = 0u32;
    let mut checks = Vec::new();
    for r in &results {
        for c in &r.checks {
            if c.passed {
                passed += 1;
            } else {
                failed += 1;
            }
            checks.push(serde_json::json!({
                "id": c.rule_id,
                "category": "compliance",
                "name": c.rule_name,
                "status": if c.passed { "pass" } else { "fail" },
                "description": c.message,
                "remediation": null
            }));
        }
    }
    let total = passed + warnings + failed;
    let score = if total > 0 {
        (100.0 * passed as f64 / total as f64) as i32
    } else {
        100
    };
    Json(serde_json::json!({
        "score": score,
        "total": total,
        "passed": passed,
        "warnings": warnings,
        "failed": failed,
        "categories": [],
        "last_scan": results.last().map(|r| r.scan_time.to_rfc3339()),
        "checks": checks
    }))
}

/// POST /api/system/compliance/scan
pub async fn trigger_compliance_scan(
    RequireRead(_claims): RequireRead,
    State(state): State<Arc<AppState>>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    let vms = state.store.list_vms().map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Failed to list VMs: {}", e),
        )
    })?;
    if vms.is_empty() {
        return Err((StatusCode::BAD_REQUEST, "No VMs available to scan".into()));
    }
    let profile = compliance::default_security_profile();
    let mut scanned = Vec::new();
    for vm in &vms {
        let vm_json =
            serde_json::to_value(vm).unwrap_or_else(|_| serde_json::json!({ "name": vm.name }));
        let result = compliance::scan_vm(&vm_json, &profile);
        let _ = state
            .store
            .save_entity("compliance_results", &result.id, &result);
        scanned.push(vm.name.clone());
    }
    Ok(Json(serde_json::json!({
        "message": format!("Scanned {} VM(s)", scanned.len()),
        "vms": scanned,
        "profile_id": profile.id
    })))
}

/// GET /api/jobs
pub async fn list_jobs(
    RequireRead(_claims): RequireRead,
    State(state): State<Arc<AppState>>,
) -> Json<serde_json::Value> {
    let mut jobs = Vec::new();
    if let Ok(backup_jobs) = state
        .store
        .list_entities::<backups::BackupJob>("backup_jobs")
    {
        for job in backup_jobs {
            jobs.push(serde_json::json!({
                "id": job.id,
                "name": job.id,
                "vm_name": job.vm_name,
                "status": format!("{:?}", job.status).to_lowercase(),
                "progress": job.progress,
                "phase": format!("{:?}", job.operation).to_lowercase(),
                "error": job.error,
                "created_at": job.started_at,
                "started_at": job.started_at,
                "completed_at": job.completed_at
            }));
        }
    }
    Json(serde_json::json!({ "jobs": jobs }))
}

/// GET /api/jobs/:id/logs
pub async fn get_job_logs(
    RequireRead(_claims): RequireRead,
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Json<serde_json::Value> {
    if let Ok(Some(job)) = state
        .store
        .get_entity::<backups::BackupJob>("backup_jobs", &id)
    {
        let lines = job.error.map(|e| vec![e]).unwrap_or_default();
        return Json(serde_json::json!({ "lines": lines }));
    }
    Json(serde_json::json!({ "lines": ["Job not found"] }))
}

/// GET /api/pipeline/jobs -- historically only ever reported mkosi image
/// builds (now removed in favor of golden images / cloud-image downloads,
/// which have their own job endpoints). No other pipeline-style job type
/// has been wired in here since, so this stays a stable empty list rather
/// than a broken/removed endpoint.
pub async fn list_pipeline_jobs(
    RequireRead(_claims): RequireRead,
    State(_state): State<Arc<AppState>>,
) -> Json<serde_json::Value> {
    Json(serde_json::json!({ "jobs": Vec::<serde_json::Value>::new() }))
}

/// GET /api/isos — legacy path wrapping /api/images/iso
pub async fn list_isos_legacy(claims: RequireRead) -> Json<serde_json::Value> {
    let axum::Json(isos) = images::list_iso_images(claims).await;
    Json(serde_json::json!({
        "isos": isos,
        "vms_with_isos": []
    }))
}

#[cfg(test)]
mod alert_rule_tests {
    use super::*;

    #[test]
    fn condition_breached_matches_each_comparison() {
        assert!(condition_breached("gt", 95.0, 90.0));
        assert!(!condition_breached("gt", 90.0, 90.0));
        assert!(condition_breached("gte", 90.0, 90.0));
        assert!(!condition_breached("gte", 89.9, 90.0));
        assert!(condition_breached("lt", 5.0, 10.0));
        assert!(!condition_breached("lt", 10.0, 10.0));
        assert!(condition_breached("lte", 10.0, 10.0));
        assert!(!condition_breached("lte", 10.1, 10.0));
    }

    #[test]
    fn condition_breached_rejects_unknown_conditions_rather_than_defaulting() {
        assert!(!condition_breached("between", 50.0, 10.0));
        assert!(!condition_breached("", 999.0, 0.0));
    }

    #[test]
    fn evaluate_alert_rules_skips_disabled_rules_even_if_breached() {
        let rules = vec![AlertRule {
            id: "cpu-high".into(),
            name: "High CPU usage".into(),
            metric: "cpu".into(),
            condition: "gte".into(),
            threshold: 0.0, // guaranteed to breach if it were evaluated
            severity: "warning".into(),
            enabled: false,
        }];
        assert!(evaluate_alert_rules(&rules).is_empty());
    }

    #[test]
    fn evaluate_alert_rules_skips_unknown_metrics_rather_than_alerting_on_none() {
        let rules = vec![AlertRule {
            id: "made-up".into(),
            name: "Not a real metric".into(),
            metric: "made-up-metric".into(),
            condition: "gt".into(),
            threshold: -1.0,
            severity: "critical".into(),
            enabled: true,
        }];
        assert!(evaluate_alert_rules(&rules).is_empty());
    }

    #[test]
    fn evaluate_alert_rules_fires_on_the_real_cpu_metric_with_a_trivially_low_threshold() {
        let rules = vec![AlertRule {
            id: "cpu-high".into(),
            name: "High CPU usage".into(),
            metric: "cpu".into(),
            condition: "gte".into(),
            threshold: 0.0,
            severity: "warning".into(),
            enabled: true,
        }];
        let alerts = evaluate_alert_rules(&rules);
        assert_eq!(alerts.len(), 1);
        assert_eq!(alerts[0]["id"], "cpu-high");
        assert_eq!(alerts[0]["severity"], "warning");
    }
}
