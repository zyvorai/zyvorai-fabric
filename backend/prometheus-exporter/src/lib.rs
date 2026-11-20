// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: LicenseRef-Zyvor-Production-1.0

use axum::{http::StatusCode, response::IntoResponse};
use lazy_static::lazy_static;
use prometheus::{
    register_int_counter, register_int_gauge, Encoder, IntCounter, IntGauge, TextEncoder,
};

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
