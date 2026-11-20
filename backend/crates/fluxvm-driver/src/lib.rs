// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: LicenseRef-Zyvor-Production-1.0

//! `driver-core` implementation backed by [FluxVM](https://github.com/zyvorai/fluxvm)
//! — the only `VmDriver` implementation left as of the systemd-removal
//! migration's final phase; the systemd-machined/systemd-vmspawn backend
//! this replaced (`machinectl-driver`/`machined-dbus`) is gone. `VMDriver`'s
//! create/stop/pause/resume/list/state/properties/leader-pid map directly
//! onto FluxVM's REST API. `ResourceControlDriver`/`ResourceStatsDriver`
//! are backed by FluxVM's cgroup-delegation extension (see
//! `resource_control.rs`). `LogDriver` streams real captured console output
//! over FluxVM's `GET /v1/vms/{id}/logs` (see `resource_control.rs`'s
//! `stream_logs` impl); the one fidelity reduction versus the old
//! journald-backed driver is that raw serial console output has no
//! per-line priority/unit metadata, so every entry is stamped uniformly.
//! `ConsoleDriver` (see `console.rs`) gives an interactive shell over
//! FluxVM's console WebSocket, itself backed by the vsock guest agent's
//! `OpenShell` op.
//!
//! **Wire sync:** `fluxvm-client` mirrors FluxVM 0.4.x models (storage backends,
//! jailer/vsock/QGA fields, migration receivers, network-migration state,
//! pod-policy, drop-reasons). Bridged VMs use `NetworkSpec::Tap { netns:
//! true }`, and Network Fabric schema v4 (`VmDataplaneDriver`) proxies FluxVM's
//! `/v1/vms/{id}/network/*` and `/v1/network/{groups,cnp,…}` when FluxVM runs with
//! `[sandbox.dataplane] mode = "ebpf"`. Also orthogonal to this driver:
//! FluxVM's `fluxvm-kube` Kubernetes CRD/operator and `fluxvm-agent`
//! distributed fleet registry.

mod console;
mod dataplane;
mod images;
mod lifecycle;
mod pools;
mod resource_control;
mod shell;

use anyhow::{Context, Result};
use zyvor_fabric_fluxvm_client::FluxVmClient;

pub use zyvor_fabric_driver_core::{
    CapabilityProvider, ConsoleDriver, DataplaneStats, DataplaneStatus, FlowRecord, ImageDriver,
    ImageInfo, LogDriver, LogEntry, MachineInfo, PoolDriver, PoolInfo, ResourceControlDriver,
    ResourceStatsDriver, ShellDriver, ShellOutput, VMDriver, VmDataplaneDriver, VmDriver,
    VmNetworkPolicy,
};

/// Driver backed by one `fluxvm serve` instance's REST API.
#[derive(Clone)]
pub struct FluxVmDriver {
    client: FluxVmClient,
}

impl FluxVmDriver {
    /// `base_url` is FluxVM's listen address, e.g. `http://127.0.0.1:7788`.
    pub fn new(base_url: impl AsRef<str>) -> Result<Self> {
        Ok(Self {
            client: FluxVmClient::new(base_url)?,
        })
    }

    pub fn with_token(mut self, token: impl Into<String>) -> Self {
        self.client = self.client.with_token(token);
        self
    }

    /// Resolve a `driver-core` name to FluxVM's `Uuid`, since `VmRecord`
    /// is keyed by id while `VMDriver` is keyed by name (systemd-machined's
    /// model). Fails loudly rather than silently no-op'ing on an unknown
    /// name, matching machinectl-driver's behavior for the same case.
    async fn resolve(&self, name: &str) -> Result<zyvor_fabric_fluxvm_client::VmRecord> {
        self.client
            .find_by_name(name)
            .await?
            .with_context(|| format!("no VM named '{name}' known to FluxVM"))
    }
}

impl CapabilityProvider for FluxVmDriver {
    fn backend_name(&self) -> &'static str {
        "fluxvm"
    }

    fn has_resource_control(&self) -> bool {
        // Backed by FluxVM's cgroup-delegation extension — see `resource_control.rs`.
        true
    }

    fn has_vm_dataplane(&self) -> bool {
        // Proxies FluxVM Network Fabric schema v4; effective when FluxVM runs with
        // `[sandbox.dataplane] mode = "ebpf"` (or `cilium`).
        true
    }
}
