// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: LicenseRef-Zyvor-Production-1.0

//! `ResourceStatsDriver`/`ResourceControlDriver`/`LogDriver` mapped onto
//! FluxVM's cgroup-delegation and log-streaming extensions (systemd-removal
//! migration plan, Phase 5): `GET/POST /v1/vms/{id}/resources|freeze|thaw|
//! frozen|stats|pressure|logs`.

use anyhow::Result;
use async_trait::async_trait;
use futures::StreamExt;
use vm_model::{PressureRecord, VMMetrics, VMPressure};
use zyvor_fabric_driver_core::{
    LogDriver, LogEntry, LogStream, ResourceControlDriver, ResourceStatsDriver,
};

use crate::FluxVmDriver;

fn convert_pressure(
    p: Option<zyvor_fabric_fluxvm_client::PressureRecord>,
) -> Option<PressureRecord> {
    p.map(|p| PressureRecord {
        avg10: p.avg10,
        avg60: p.avg60,
        avg300: p.avg300,
        total: p.total,
    })
}

#[async_trait]
impl ResourceStatsDriver for FluxVmDriver {
    async fn get_metrics(&self, name: &str) -> Result<VMMetrics> {
        let record = self.resolve(name).await?;
        let m = self.client.stats(record.id).await?;
        Ok(VMMetrics {
            cpu_usage: m.cpu_usage_percent,
            memory_usage: m.memory_usage_bytes,
            // FluxVM's cgroup stats have no disk-size concept (that's a
            // filesystem-level property, not a cgroup `io` controller one);
            // `disk_usage` here means "cgroup-attributed I/O bytes", so sum
            // read+write rather than leave it zeroed.
            disk_usage: m.disk_read_bytes + m.disk_write_bytes,
            // Network accounting isn't cgroup-scoped (no `net_cls`/`net_prio`
            // delegation in the v2 hierarchy) — FluxVM doesn't expose it,
            // matching machinectl-driver's own stats gap for the same reason.
            network_rx: 0,
            network_tx: 0,
        })
    }

    async fn get_pressure(&self, name: &str) -> Result<VMPressure> {
        let record = self.resolve(name).await?;
        let p = self.client.pressure(record.id).await?;
        Ok(VMPressure {
            cpu_some: convert_pressure(p.cpu_some),
            memory_some: convert_pressure(p.memory_some),
            memory_full: convert_pressure(p.memory_full),
            io_some: convert_pressure(p.io_some),
            io_full: convert_pressure(p.io_full),
        })
    }
}

#[async_trait]
impl ResourceControlDriver for FluxVmDriver {
    async fn set_cpu_quota(&self, name: &str, percent: u32) -> Result<()> {
        let record = self.resolve(name).await?;
        let patch = zyvor_fabric_fluxvm_client::ResourcePatch {
            cpu_quota_percent: Some(percent),
            ..Default::default()
        };
        self.client.set_resources(record.id, &patch).await
    }

    async fn set_memory_max(&self, name: &str, bytes: u64) -> Result<()> {
        let record = self.resolve(name).await?;
        let patch = zyvor_fabric_fluxvm_client::ResourcePatch {
            memory_max_bytes: Some(bytes),
            ..Default::default()
        };
        self.client.set_resources(record.id, &patch).await
    }

    async fn set_io_weight(&self, name: &str, weight: u32) -> Result<()> {
        let record = self.resolve(name).await?;
        let patch = zyvor_fabric_fluxvm_client::ResourcePatch {
            io_weight: Some(weight),
            ..Default::default()
        };
        self.client.set_resources(record.id, &patch).await
    }

    async fn freeze(&self, name: &str) -> Result<()> {
        let record = self.resolve(name).await?;
        self.client.freeze(record.id).await
    }

    async fn thaw(&self, name: &str) -> Result<()> {
        let record = self.resolve(name).await?;
        self.client.thaw(record.id).await
    }

    async fn is_frozen(&self, name: &str) -> Result<bool> {
        let record = self.resolve(name).await?;
        self.client.is_frozen(record.id).await
    }

    async fn set_pids_max(&self, name: &str, max: u64) -> Result<()> {
        let record = self.resolve(name).await?;
        let patch = zyvor_fabric_fluxvm_client::ResourcePatch {
            pids_max: Some(max),
            ..Default::default()
        };
        self.client.set_resources(record.id, &patch).await
    }

    async fn set_cpuset(&self, name: &str, cpus: &[u32]) -> Result<()> {
        let record = self.resolve(name).await?;
        let patch = zyvor_fabric_fluxvm_client::ResourcePatch {
            cpuset_cpus: Some(cpus.to_vec()),
            ..Default::default()
        };
        self.client.set_resources(record.id, &patch).await
    }

    async fn get_cpuset(&self, name: &str) -> Result<Vec<u32>> {
        let record = self.resolve(name).await?;
        self.client.get_cpuset(record.id).await
    }
}

#[async_trait]
impl LogDriver for FluxVmDriver {
    async fn stream_logs(&self, name: &str, lines: u32) -> Result<LogStream> {
        let record = self.resolve(name).await?;
        let unit = name.to_string();
        let lines_stream = self.client.stream_logs(record.id, lines, true).await?;

        // Raw serial console output has no journald-equivalent per-line
        // priority/unit metadata (see `fluxvm_api::vm_logs`), so every
        // entry is stamped with the same "info" priority and the VM's own
        // name as the unit — an accepted fidelity reduction versus
        // `MachinectlDriver`'s `journalctl --output=json` mapping.
        let entries = lines_stream.filter_map(move |line| {
            let unit = unit.clone();
            async move {
                match line {
                    Ok(message) => Some(LogEntry {
                        timestamp: chrono::Utc::now(),
                        message,
                        priority: 6,
                        unit,
                    }),
                    Err(e) => {
                        tracing::warn!("FluxVM log stream for '{unit}' ended with error: {e:#}");
                        None
                    }
                }
            }
        });

        Ok(Box::pin(entries))
    }
}
