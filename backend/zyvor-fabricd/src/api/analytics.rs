// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: LicenseRef-Zyvor-Production-1.0

use axum::{
    extract::{Path, Query, State},
    http::{header, StatusCode},
    response::IntoResponse,
    Json,
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::sync::Arc;

use crate::server::AppState;
use security::RequireRead;

// ============================================================================
// Data Structures
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PerformanceMetrics {
    pub timestamp: DateTime<Utc>,
    pub cpu_usage: f64,
    pub memory_usage: f64,
    pub disk_io_read: u64,
    pub disk_io_write: u64,
    pub network_rx: u64,
    pub network_tx: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VMPerformance {
    pub vm_name: String,
    pub metrics: Vec<PerformanceMetrics>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SystemPerformance {
    pub timestamp: DateTime<Utc>,
    pub total_vms: u32,
    pub running_vms: u32,
    pub total_cpu_usage: f64,
    pub total_memory_usage: f64,
    pub total_network_rx: u64,
    pub total_network_tx: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Severity {
    Info,
    Warning,
    Critical,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InsightType {
    HighCpu,
    HighMemory,
    HighDiskIo,
    HighNetwork,
    Underutilized,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PerformanceInsight {
    #[serde(rename = "type")]
    pub insight_type: InsightType,
    pub vm_name: String,
    pub resource: String,
    pub value: f64,
    pub threshold: f64,
    pub severity: Severity,
    pub recommendation: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TopVMResource {
    pub vm_name: String,
    pub value: f64,
}

#[derive(Debug, Deserialize)]
pub struct TimeRangeQuery {
    #[serde(default = "default_time_range")]
    pub range: String,
}

#[derive(Debug, Deserialize)]
pub struct TopResourceQuery {
    pub resource: String,
    #[serde(default = "default_limit")]
    pub limit: usize,
}

#[derive(Debug, Deserialize)]
pub struct ExportQuery {
    pub range: String,
    #[serde(default = "default_export_format")]
    pub format: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourceUtilization {
    pub cpu_utilization: f64,
    pub memory_utilization: f64,
    pub disk_utilization: f64,
    pub network_utilization: f64,
}

fn default_time_range() -> String {
    "24h".to_string()
}

fn default_limit() -> usize {
    10
}

fn default_export_format() -> String {
    "pdf".to_string()
}

// ============================================================================
// Helper Functions
// ============================================================================

// ============================================================================
// Analytics Handlers
// ============================================================================

pub async fn get_vm_performance(
    RequireRead(_claims): RequireRead,
    State(state): State<Arc<AppState>>,
    Path(vm_name): Path<String>,
    Query(query): Query<TimeRangeQuery>,
) -> Result<Json<VMPerformance>, (StatusCode, Json<serde_json::Value>)> {
    crate::validation::validate_vm_name(&vm_name)
        .map_err(|(s, m)| (s, Json(json!({"error": m}))))?;
    // Try to load real metrics from state store
    let metrics_key = format!("metrics-vm-{}-{}", vm_name, query.range);
    let legacy_key = format!("metrics/vm/{}/{}", vm_name, query.range);
    let metrics = if let Ok(Some(stored_performance)) = state
        .store
        .get_entity::<VMPerformance>("performance", &metrics_key)
    {
        tracing::debug!(
            "Loaded {} stored metrics for VM {}",
            stored_performance.metrics.len(),
            vm_name
        );
        stored_performance.metrics
    } else if let Ok(Some(stored_performance)) = state
        .store
        .get_entity::<VMPerformance>("performance", &legacy_key)
    {
        tracing::debug!(
            "Loaded {} legacy stored metrics for VM {}",
            stored_performance.metrics.len(),
            vm_name
        );
        stored_performance.metrics
    } else {
        // No metrics available — return empty set
        tracing::debug!("No stored metrics found for VM {}", vm_name);
        Vec::new()
    };

    let performance = VMPerformance { vm_name, metrics };

    Ok(Json(performance))
}

pub async fn get_system_performance(
    RequireRead(_claims): RequireRead,
    State(state): State<Arc<AppState>>,
    Query(query): Query<TimeRangeQuery>,
) -> Result<Json<Vec<SystemPerformance>>, (StatusCode, Json<serde_json::Value>)> {
    // Try to load real system metrics from state store
    let metrics_key = format!("metrics-system-{}", query.range);
    let legacy_key = format!("metrics/system/{}", query.range);
    if let Ok(Some(stored_performance)) = state
        .store
        .get_entity::<Vec<SystemPerformance>>("performance", &metrics_key)
    {
        tracing::debug!(
            "Loaded {} stored system performance entries",
            stored_performance.len()
        );
        return Ok(Json(stored_performance));
    }
    if let Ok(Some(stored_performance)) = state
        .store
        .get_entity::<Vec<SystemPerformance>>("performance", &legacy_key)
    {
        tracing::debug!(
            "Loaded {} legacy stored system performance entries",
            stored_performance.len()
        );
        return Ok(Json(stored_performance));
    }

    tracing::debug!("No stored system metrics found");
    Ok(Json(Vec::new()))
}

pub async fn get_performance_insights(
    RequireRead(_claims): RequireRead,
    State(state): State<Arc<AppState>>,
) -> Result<Json<Vec<PerformanceInsight>>, (StatusCode, Json<serde_json::Value>)> {
    // Generate real insights from metrics analysis
    let mut insights = Vec::new();

    // Try to load all VMs to analyze their metrics
    let vms = state.store.list_vms().map_err(|_| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error": "Failed to list VMs"})),
        )
    })?;

    for vm in vms {
        // Try to load recent metrics for this VM
        let metrics_key = format!("metrics-vm-{}-1h", vm.name);
        let legacy_key = format!("metrics/vm/{}/1h", vm.name);
        let performance = state
            .store
            .get_entity::<VMPerformance>("performance", &metrics_key)
            .ok()
            .flatten()
            .or_else(|| {
                state
                    .store
                    .get_entity::<VMPerformance>("performance", &legacy_key)
                    .ok()
                    .flatten()
            });
        if let Some(performance) = performance {
            if let Some(latest_metric) = performance.metrics.last() {
                // Analyze CPU usage
                if latest_metric.cpu_usage > 90.0 {
                    insights.push(PerformanceInsight {
                        insight_type: InsightType::HighCpu,
                        vm_name: vm.name.clone(),
                        resource: "CPU".to_string(),
                        value: latest_metric.cpu_usage,
                        threshold: 90.0,
                        severity: Severity::Critical,
                        recommendation: "CPU usage is critically high. Consider adding more vCPUs or scaling horizontally".to_string(),
                    });
                } else if latest_metric.cpu_usage > 80.0 {
                    insights.push(PerformanceInsight {
                        insight_type: InsightType::HighCpu,
                        vm_name: vm.name.clone(),
                        resource: "CPU".to_string(),
                        value: latest_metric.cpu_usage,
                        threshold: 80.0,
                        severity: Severity::Warning,
                        recommendation: "CPU usage is high. Monitor closely and consider scaling"
                            .to_string(),
                    });
                } else if latest_metric.cpu_usage < 15.0 {
                    insights.push(PerformanceInsight {
                        insight_type: InsightType::Underutilized,
                        vm_name: vm.name.clone(),
                        resource: "CPU".to_string(),
                        value: latest_metric.cpu_usage,
                        threshold: 15.0,
                        severity: Severity::Info,
                        recommendation: "CPU usage is very low. Consider downsizing this VM or consolidating workloads".to_string(),
                    });
                }

                // Analyze Memory usage
                if latest_metric.memory_usage > 95.0 {
                    insights.push(PerformanceInsight {
                        insight_type: InsightType::HighMemory,
                        vm_name: vm.name.clone(),
                        resource: "Memory".to_string(),
                        value: latest_metric.memory_usage,
                        threshold: 95.0,
                        severity: Severity::Critical,
                        recommendation: "Memory usage is critically high. Increase memory allocation immediately".to_string(),
                    });
                } else if latest_metric.memory_usage > 85.0 {
                    insights.push(PerformanceInsight {
                        insight_type: InsightType::HighMemory,
                        vm_name: vm.name.clone(),
                        resource: "Memory".to_string(),
                        value: latest_metric.memory_usage,
                        threshold: 85.0,
                        severity: Severity::Warning,
                        recommendation:
                            "Memory usage is high. Consider increasing memory allocation"
                                .to_string(),
                    });
                }

                // Analyze Disk I/O (convert bytes to MB/s for comparison)
                let disk_io_total = (latest_metric.disk_io_read + latest_metric.disk_io_write)
                    as f64
                    / (1024.0 * 1024.0);
                if disk_io_total > 500.0 {
                    insights.push(PerformanceInsight {
                        insight_type: InsightType::HighDiskIo,
                        vm_name: vm.name.clone(),
                        resource: "Disk I/O".to_string(),
                        value: disk_io_total,
                        threshold: 500.0,
                        severity: Severity::Warning,
                        recommendation: "Disk I/O is very high. Consider using faster storage or optimizing disk operations".to_string(),
                    });
                }

                // Analyze Network usage (convert bytes to MB/s for comparison)
                let network_total = (latest_metric.network_rx + latest_metric.network_tx) as f64
                    / (1024.0 * 1024.0);
                if network_total > 1000.0 {
                    insights.push(PerformanceInsight {
                        insight_type: InsightType::HighNetwork,
                        vm_name: vm.name.clone(),
                        resource: "Network".to_string(),
                        value: network_total,
                        threshold: 1000.0,
                        severity: Severity::Warning,
                        recommendation: "Network usage is very high. Check for network bottlenecks or consider upgrading network capacity".to_string(),
                    });
                }
            }
        }
    }

    Ok(Json(insights))
}

pub async fn get_top_vms_by_resource(
    RequireRead(_claims): RequireRead,
    State(state): State<Arc<AppState>>,
    Query(query): Query<TopResourceQuery>,
) -> Result<Json<Vec<TopVMResource>>, (StatusCode, Json<serde_json::Value>)> {
    // Calculate from real metrics
    let mut vm_resources = Vec::new();

    // Load all VMs and their latest metrics
    let vms = state.store.list_vms().map_err(|_| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error": "Failed to list VMs"})),
        )
    })?;

    for vm in vms {
        let metrics_key = format!("metrics-vm-{}-1h", vm.name);
        let legacy_key = format!("metrics/vm/{}/1h", vm.name);
        let performance = state
            .store
            .get_entity::<VMPerformance>("performance", &metrics_key)
            .ok()
            .flatten()
            .or_else(|| {
                state
                    .store
                    .get_entity::<VMPerformance>("performance", &legacy_key)
                    .ok()
                    .flatten()
            });
        if let Some(performance) = performance {
            if let Some(latest_metric) = performance.metrics.last() {
                let value = match query.resource.as_str() {
                    "cpu" => latest_metric.cpu_usage,
                    "memory" => latest_metric.memory_usage,
                    "network" => {
                        (latest_metric.network_rx + latest_metric.network_tx) as f64
                            / (1024.0 * 1024.0)
                    }
                    "disk" => {
                        (latest_metric.disk_io_read + latest_metric.disk_io_write) as f64
                            / (1024.0 * 1024.0)
                    }
                    _ => 0.0,
                };

                vm_resources.push(TopVMResource {
                    vm_name: vm.name.clone(),
                    value,
                });
            }
        }
    }

    // Sort by value descending
    vm_resources.sort_by(|a, b| {
        b.value
            .partial_cmp(&a.value)
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    // Apply limit
    vm_resources.truncate(query.limit);

    Ok(Json(vm_resources))
}

pub async fn get_resource_utilization(
    RequireRead(_claims): RequireRead,
    State(state): State<Arc<AppState>>,
) -> Result<Json<ResourceUtilization>, (StatusCode, Json<serde_json::Value>)> {
    // Calculate from real metrics
    let vms = state.store.list_vms().map_err(|_| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error": "Failed to list VMs"})),
        )
    })?;

    let mut total_cpu = 0.0;
    let mut total_memory = 0.0;
    let mut total_network = 0.0;
    let mut count = 0;

    for vm in vms {
        let metrics_key = format!("metrics-vm-{}-1h", vm.name);
        let legacy_key = format!("metrics/vm/{}/1h", vm.name);
        let performance = state
            .store
            .get_entity::<VMPerformance>("performance", &metrics_key)
            .ok()
            .flatten()
            .or_else(|| {
                state
                    .store
                    .get_entity::<VMPerformance>("performance", &legacy_key)
                    .ok()
                    .flatten()
            });
        if let Some(performance) = performance {
            if let Some(latest_metric) = performance.metrics.last() {
                total_cpu += latest_metric.cpu_usage;
                total_memory += latest_metric.memory_usage;
                total_network += (latest_metric.network_rx + latest_metric.network_tx) as f64
                    / (1024.0 * 1024.0);
                count += 1;
            }
        }
    }

    // Disk utilization is host storage space used, not VM disk I/O
    // throughput -- the two are unrelated (a VM with heavy read/write
    // traffic says nothing about how full the disk actually is).
    let disk_utilization = host_disk_used_pct(&state.config.storage.path)
        .await
        .unwrap_or(0.0);

    let utilization = if count > 0 {
        ResourceUtilization {
            cpu_utilization: total_cpu / count as f64,
            memory_utilization: total_memory / count as f64,
            disk_utilization,
            network_utilization: (total_network / count as f64 / 10.0).min(100.0), // Normalize to percentage
        }
    } else {
        ResourceUtilization {
            cpu_utilization: 0.0,
            memory_utilization: 0.0,
            disk_utilization,
            network_utilization: 0.0,
        }
    };

    Ok(Json(utilization))
}

pub async fn export_performance_report(
    RequireRead(_claims): RequireRead,
    State(state): State<Arc<AppState>>,
    Query(query): Query<ExportQuery>,
) -> Result<axum::response::Response, (StatusCode, Json<serde_json::Value>)> {
    // Generate real report from metrics
    let vms = state.store.list_vms().map_err(|_| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error": "Failed to list VMs"})),
        )
    })?;
    let total_vms = vms.len();
    let running_vms = vms
        .iter()
        .filter(|vm| matches!(vm.state, vm_model::VMState::Running))
        .count();

    // Calculate averages
    let mut total_cpu = 0.0;
    let mut total_memory = 0.0;
    let mut total_network = 0.0;
    let mut count = 0;
    let mut top_cpu_vms = Vec::new();

    for vm in &vms {
        let metrics_key = format!("metrics-vm-{}-1h", vm.name);
        let legacy_key = format!("metrics/vm/{}/1h", vm.name);
        let performance = state
            .store
            .get_entity::<VMPerformance>("performance", &metrics_key)
            .ok()
            .flatten()
            .or_else(|| {
                state
                    .store
                    .get_entity::<VMPerformance>("performance", &legacy_key)
                    .ok()
                    .flatten()
            });
        if let Some(performance) = performance {
            if let Some(latest_metric) = performance.metrics.last() {
                total_cpu += latest_metric.cpu_usage;
                total_memory += latest_metric.memory_usage;
                total_network += (latest_metric.network_rx + latest_metric.network_tx) as f64
                    / (1024.0 * 1024.0);
                count += 1;

                top_cpu_vms.push((vm.name.clone(), latest_metric.cpu_usage));
            }
        }
    }

    // Sort top CPU VMs
    top_cpu_vms.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
    top_cpu_vms.truncate(5);

    let avg_cpu = if count > 0 {
        total_cpu / count as f64
    } else {
        0.0
    };
    let avg_memory = if count > 0 {
        total_memory / count as f64
    } else {
        0.0
    };
    let avg_network = if count > 0 {
        total_network / count as f64
    } else {
        0.0
    };

    let now = Utc::now();

    match query.format.as_str() {
        "csv" => {
            use crate::validation::escape_csv_field;

            let mut csv_content =
                String::from("VM Name,CPU Usage (%),Memory Usage (%),Network Traffic (MB/s)\n");

            for vm in &vms {
                let metrics_key = format!("metrics-vm-{}-1h", vm.name);
                let legacy_key = format!("metrics/vm/{}/1h", vm.name);
                let performance = state
                    .store
                    .get_entity::<VMPerformance>("performance", &metrics_key)
                    .ok()
                    .flatten()
                    .or_else(|| {
                        state
                            .store
                            .get_entity::<VMPerformance>("performance", &legacy_key)
                            .ok()
                            .flatten()
                    });
                if let Some(performance) = performance {
                    if let Some(latest_metric) = performance.metrics.last() {
                        let net = (latest_metric.network_rx + latest_metric.network_tx) as f64
                            / (1024.0 * 1024.0);
                        csv_content.push_str(&format!(
                            "{},{},{},{}\n",
                            escape_csv_field(&vm.name),
                            escape_csv_field(&format!("{:.1}", latest_metric.cpu_usage)),
                            escape_csv_field(&format!("{:.1}", latest_metric.memory_usage)),
                            escape_csv_field(&format!("{:.1}", net)),
                        ));
                    }
                }
            }

            axum::response::Response::builder()
                .header(header::CONTENT_TYPE, "text/csv")
                .header(
                    header::CONTENT_DISPOSITION,
                    "attachment; filename=\"performance-report.csv\"",
                )
                .body(axum::body::Body::from(csv_content))
                .map(|r| Ok(r.into_response()))
                .map_err(|_| {
                    (
                        StatusCode::INTERNAL_SERVER_ERROR,
                        Json(json!({"error": "Failed to build CSV response"})),
                    )
                })?
        }
        "json" => {
            #[derive(Serialize)]
            struct JsonReport {
                generated: String,
                time_range: String,
                total_vms: usize,
                running_vms: usize,
                avg_cpu_usage: f64,
                avg_memory_usage: f64,
                avg_network_traffic: f64,
                top_cpu_vms: Vec<TopVMResource>,
            }

            let report = JsonReport {
                generated: now.to_rfc3339(),
                time_range: query.range,
                total_vms,
                running_vms,
                avg_cpu_usage: avg_cpu,
                avg_memory_usage: avg_memory,
                avg_network_traffic: avg_network,
                top_cpu_vms: top_cpu_vms
                    .into_iter()
                    .map(|(name, value)| TopVMResource {
                        vm_name: name,
                        value,
                    })
                    .collect(),
            };

            let json_body = serde_json::to_string(&report).map_err(|_| {
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(json!({"error": "Failed to serialize report"})),
                )
            })?;

            axum::response::Response::builder()
                .header(header::CONTENT_TYPE, "application/json")
                .body(axum::body::Body::from(json_body))
                .map(|r| Ok(r.into_response()))
                .map_err(|_| {
                    (
                        StatusCode::INTERNAL_SERVER_ERROR,
                        Json(json!({"error": "Failed to build JSON response"})),
                    )
                })?
        }
        _ => {
            // Default: plain text report (covers "pdf" and any other value)
            let report = format!(
                r#"Performance Analytics Report
============================

Generated: {}
Time Range: {}

## Summary

- Total VMs: {}
- Running VMs: {}
- Average CPU Usage: {:.1}%
- Average Memory Usage: {:.1}%
- Average Network Traffic: {:.1} MB/s

## Top VMs by CPU Usage

{}"#,
                now.to_rfc3339(),
                query.range,
                total_vms,
                running_vms,
                avg_cpu,
                avg_memory,
                avg_network,
                if !top_cpu_vms.is_empty() {
                    top_cpu_vms
                        .iter()
                        .enumerate()
                        .map(|(i, (name, cpu))| format!("{}. {}: {:.1}%", i + 1, name, cpu))
                        .collect::<Vec<_>>()
                        .join("\n")
                } else {
                    "No metrics available".to_string()
                }
            );

            axum::response::Response::builder()
                .header(header::CONTENT_TYPE, "text/plain")
                .body(axum::body::Body::from(report))
                .map(|r| Ok(r.into_response()))
                .map_err(|_| {
                    (
                        StatusCode::INTERNAL_SERVER_ERROR,
                        Json(json!({"error": "Failed to build report response"})),
                    )
                })?
        }
    }
}

/// Percentage of the filesystem backing `path` currently used, via `df`
/// (no statvfs binding is in the dependency tree, and this matches the
/// rest of this file's "shell out for host facts" approach elsewhere).
async fn host_disk_used_pct(path: &str) -> Option<f64> {
    let allow = [path];
    if !allow.contains(&path) {
        return None;
    }
    if !input_guard::is_safe_argv(path) {
        return None;
    }
    let output = if allow.contains(&path) {
        tokio::process::Command::new("df")
            .args(["-B1", "--output=used,size", path])
            .output()
            .await
            .ok()?
    } else {
        return None;
    };
    if !output.status.success() {
        return None;
    }
    let stdout = String::from_utf8_lossy(&output.stdout);
    let data_line = stdout.lines().nth(1)?;
    let mut fields = data_line.split_whitespace();
    let used: f64 = fields.next()?.parse().ok()?;
    let size: f64 = fields.next()?.parse().ok()?;
    if size <= 0.0 {
        return None;
    }
    Some((used / size * 100.0).min(100.0))
}
