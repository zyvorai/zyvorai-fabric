// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: LicenseRef-Zyvor-Production-2.0

use axum::{http::StatusCode, response::IntoResponse};
use lazy_static::lazy_static;
use prometheus::{
    register_gauge_vec, register_histogram_vec, register_int_counter, register_int_counter_vec,
    register_int_gauge, register_int_gauge_vec, Encoder, GaugeVec, HistogramVec, IntCounter,
    IntCounterVec, IntGauge, IntGaugeVec, TextEncoder,
};
use vm_model::{VMState, VM};

/// Every VM state, in the order series are emitted. Each is always exported
/// (zero when no VM is in that state) so dashboards never see gaps.
pub const ALL_VM_STATES: [(VMState, &str); 7] = [
    (VMState::Running, "running"),
    (VMState::Stopped, "stopped"),
    (VMState::Paused, "paused"),
    (VMState::Starting, "starting"),
    (VMState::Stopping, "stopping"),
    (VMState::Failed, "failed"),
    (VMState::Unknown, "unknown"),
];

/// Default cap on the number of VMs that get per-VM CPU/memory series.
pub const DEFAULT_PER_VM_MAX: usize = 50;

/// Latency histogram buckets (seconds) for API requests.
const HTTP_BUCKETS: [f64; 11] = [
    0.005, 0.01, 0.025, 0.05, 0.1, 0.25, 0.5, 1.0, 2.5, 10.0, 60.0,
];

lazy_static! {
    pub static ref VMS_TOTAL: IntGauge =
        register_int_gauge!("zyvor_fabricd_vms_total", "Total number of VMs").unwrap();
    pub static ref VMS_RUNNING: IntGauge =
        register_int_gauge!("zyvor_fabricd_vms_running", "Number of running VMs").unwrap();
    pub static ref VMS_STOPPED: IntGauge =
        register_int_gauge!("zyvor_fabricd_vms_stopped", "Number of stopped VMs").unwrap();
    pub static ref VM_START_COUNT: IntCounter =
        register_int_counter!("zyvor_fabricd_vm_starts_total", "Total VM starts").unwrap();
    pub static ref VM_STOP_COUNT: IntCounter =
        register_int_counter!("zyvor_fabricd_vm_stops_total", "Total VM stops").unwrap();
    pub static ref VM_CREATE_COUNT: IntCounter =
        register_int_counter!("zyvor_fabricd_vm_creates_total", "Total VM creates").unwrap();
    pub static ref VM_DELETE_COUNT: IntCounter =
        register_int_counter!("zyvor_fabricd_vm_deletes_total", "Total VM deletes").unwrap();
    /// VM count per lifecycle state (`state` label has a fixed 7-value domain).
    pub static ref VMS_BY_STATE: IntGaugeVec = register_int_gauge_vec!(
        "zyvor_fabricd_vms_by_state",
        "Number of VMs in each lifecycle state",
        &["state"]
    )
    .unwrap();
    /// API requests by method, matched route template and status class.
    pub static ref HTTP_REQUESTS: IntCounterVec = register_int_counter_vec!(
        "zyvor_fabricd_http_requests_total",
        "API requests by method, route template and status class",
        &["method", "route", "status_class"]
    )
    .unwrap();
    /// API request latency by method and matched route template.
    pub static ref HTTP_DURATION: HistogramVec = register_histogram_vec!(
        "zyvor_fabricd_http_request_duration_seconds",
        "API request latency in seconds by method and route template",
        &["method", "route"],
        HTTP_BUCKETS.to_vec()
    )
    .unwrap();
    /// Opt-in per-VM CPU utilisation (percent), capped VM count.
    pub static ref VM_CPU_PERCENT: GaugeVec = register_gauge_vec!(
        "zyvor_fabricd_vm_cpu_usage_percent",
        "Per-VM CPU usage percent (only when per-VM metrics are enabled)",
        &["vm"]
    )
    .unwrap();
    /// Opt-in per-VM memory utilisation (percent of allocated), capped VM count.
    pub static ref VM_MEMORY_PERCENT: GaugeVec = register_gauge_vec!(
        "zyvor_fabricd_vm_memory_usage_percent",
        "Per-VM memory usage percent of allocation (only when per-VM metrics are enabled)",
        &["vm"]
    )
    .unwrap();
}

/// Aggregate VM counts computed from a VM list.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VmCounts {
    pub total: i64,
    pub running: i64,
    pub stopped: i64,
    /// Count per state label, in `ALL_VM_STATES` order (zeros included).
    pub by_state: Vec<(&'static str, i64)>,
}

/// Pure: count VMs overall and per state.
pub fn count_vms(vms: &[VM]) -> VmCounts {
    let by_state: Vec<(&'static str, i64)> = ALL_VM_STATES
        .iter()
        .map(|(st, label)| {
            let n = vms
                .iter()
                .filter(|v| std::mem::discriminant(&v.state) == std::mem::discriminant(st))
                .count() as i64;
            (*label, n)
        })
        .collect();
    let get = |l: &str| {
        by_state
            .iter()
            .find(|(k, _)| *k == l)
            .map_or(0, |(_, n)| *n)
    };
    VmCounts {
        total: vms.len() as i64,
        running: get("running"),
        stopped: get("stopped"),
        by_state,
    }
}

/// Update the VM gauges (total/running/stopped and per-state) from a VM list.
/// Read-only with respect to VMs; only touches Prometheus gauges.
pub fn update_vm_gauges(vms: &[VM]) {
    let c = count_vms(vms);
    update_vm_counts(c.total, c.running, c.stopped);
    for (label, n) in &c.by_state {
        VMS_BY_STATE.with_label_values(&[label]).set(*n);
    }
}

/// Select which VMs get per-VM series: sorted by name, capped at `max`.
pub fn select_per_vm_names(mut names: Vec<String>, max: usize) -> Vec<String> {
    names.sort();
    names.dedup();
    names.truncate(max);
    names
}

/// Replace the per-VM CPU/memory series with `samples` (name, cpu%, mem%).
/// At most `max` VMs (alphabetically first) are labelled; the rest are dropped,
/// as are series of VMs no longer present. Returns the number labelled.
pub fn set_per_vm_usage(samples: &[(String, f64, f64)], max: usize) -> usize {
    VM_CPU_PERCENT.reset();
    VM_MEMORY_PERCENT.reset();
    let keep = select_per_vm_names(samples.iter().map(|s| s.0.clone()).collect(), max);
    for (name, cpu, mem) in samples.iter().filter(|s| keep.contains(&s.0)) {
        VM_CPU_PERCENT.with_label_values(&[name]).set(*cpu);
        VM_MEMORY_PERCENT.with_label_values(&[name]).set(*mem);
    }
    keep.len()
}

/// Clear per-VM series (used when the flag is off).
pub fn clear_per_vm_usage() {
    VM_CPU_PERCENT.reset();
    VM_MEMORY_PERCENT.reset();
}

/// Bounded method label: standard HTTP methods only, everything else `OTHER`.
pub fn method_label(m: &str) -> &'static str {
    match m {
        "GET" => "GET",
        "POST" => "POST",
        "PUT" => "PUT",
        "PATCH" => "PATCH",
        "DELETE" => "DELETE",
        "HEAD" => "HEAD",
        "OPTIONS" => "OPTIONS",
        _ => "OTHER",
    }
}

/// Status class label (`2xx`, `4xx`, ...); anything outside 100..=599 is `other`.
pub fn status_class(code: u16) -> &'static str {
    match code {
        100..=199 => "1xx",
        200..=299 => "2xx",
        300..=399 => "3xx",
        400..=499 => "4xx",
        500..=599 => "5xx",
        _ => "other",
    }
}

/// Record one API request. `route` must be a route template (or the single
/// `unmatched` label), never a raw path.
pub fn observe_http_request(method: &str, route: &str, status: u16, seconds: f64) {
    let m = method_label(method);
    HTTP_REQUESTS
        .with_label_values(&[m, route, status_class(status)])
        .inc();
    HTTP_DURATION
        .with_label_values(&[m, route])
        .observe(seconds);
}

/// Render the default registry in Prometheus text exposition format.
pub fn render_text() -> String {
    let mut buf = Vec::new();
    match TextEncoder::new().encode(&prometheus::gather(), &mut buf) {
        Ok(()) => String::from_utf8_lossy(&buf).into_owned(),
        Err(_) => String::new(),
    }
}

pub async fn metrics_handler() -> impl IntoResponse {
    let encoder = TextEncoder::new();
    let metric_families = prometheus::gather();
    let mut buffer = Vec::new();

    match encoder.encode(&metric_families, &mut buffer) {
        Ok(_) => (StatusCode::OK, buffer),
        Err(_) => (StatusCode::INTERNAL_SERVER_ERROR, Vec::new()),
    }
}

pub fn record_vm_start() {
    VM_START_COUNT.inc();
    VMS_RUNNING.inc();
    VMS_STOPPED.dec();
}

pub fn record_vm_stop() {
    VM_STOP_COUNT.inc();
    VMS_RUNNING.dec();
    VMS_STOPPED.inc();
}

pub fn record_vm_create() {
    VM_CREATE_COUNT.inc();
    VMS_TOTAL.inc();
    VMS_STOPPED.inc();
}

pub fn record_vm_delete() {
    VM_DELETE_COUNT.inc();
    VMS_TOTAL.dec();
}

pub fn update_vm_counts(total: i64, running: i64, stopped: i64) {
    VMS_TOTAL.set(total);
    VMS_RUNNING.set(running);
    VMS_STOPPED.set(stopped);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn vm(name: &str, state: VMState) -> VM {
        serde_json::from_value(serde_json::json!({
            "name": name, "state": state, "cpus": 1, "memory": 512, "disk": 1,
            "image": "i", "created": "2026-01-01T00:00:00Z"
        }))
        .unwrap()
    }

    fn fixture() -> Vec<VM> {
        vec![
            vm("a", VMState::Running),
            vm("b", VMState::Running),
            vm("c", VMState::Stopped),
            vm("d", VMState::Failed),
            vm("e", VMState::Paused),
        ]
    }

    #[test]
    fn counts_total_running_stopped_and_states() {
        let c = count_vms(&fixture());
        assert_eq!((c.total, c.running, c.stopped), (5, 2, 1));
        assert_eq!(c.by_state.len(), 7);
        let get = |l: &str| c.by_state.iter().find(|(k, _)| *k == l).unwrap().1;
        assert_eq!(get("failed"), 1);
        assert_eq!(get("paused"), 1);
        assert_eq!(get("starting"), 0);
        assert_eq!(c.by_state.iter().map(|(_, n)| n).sum::<i64>(), c.total);
    }

    #[test]
    fn counts_empty() {
        let c = count_vms(&[]);
        assert_eq!((c.total, c.running, c.stopped), (0, 0, 0));
    }

    #[test]
    fn per_vm_selection_is_sorted_and_capped() {
        let n = vec!["z".into(), "b".into(), "a".into(), "b".into()];
        assert_eq!(select_per_vm_names(n, 2), vec!["a", "b"]);
    }

    #[test]
    fn label_helpers_are_bounded() {
        assert_eq!(method_label("BREW"), "OTHER");
        assert_eq!(method_label("GET"), "GET");
        assert_eq!(status_class(204), "2xx");
        assert_eq!(status_class(503), "5xx");
        assert_eq!(status_class(0), "other");
    }

    // Single test touching the shared global registry for the VM/per-VM series.
    #[test]
    fn gauges_render_in_exposition_output() {
        update_vm_gauges(&fixture());
        let samples: Vec<(String, f64, f64)> = ["x", "y", "w"]
            .iter()
            .map(|n| (n.to_string(), 10.0, 20.0))
            .collect();
        assert_eq!(set_per_vm_usage(&samples, 2), 2);
        let mut buf = Vec::new();
        TextEncoder::new()
            .encode(&prometheus::gather(), &mut buf)
            .unwrap();
        let out = String::from_utf8(buf).unwrap();
        assert!(out.contains("zyvor_fabricd_vms_total 5"));
        assert!(out.contains("zyvor_fabricd_vms_running 2"));
        assert!(out.contains("zyvor_fabricd_vms_by_state{state=\"failed\"} 1"));
        assert!(out.contains("zyvor_fabricd_vm_cpu_usage_percent{vm=\"w\"} 10"));
        assert!(out.contains("zyvor_fabricd_vm_cpu_usage_percent{vm=\"x\"} 10"));
        assert!(!out.contains("vm=\"y\""));
        clear_per_vm_usage();
    }
}
