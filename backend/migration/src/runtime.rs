// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: LicenseRef-Zyvor-Production-2.0

//! Native migration orchestration over FluxVM's node-local runtime contract.
//! ZYVOR_RUNTIME_BOUNDARY_V1
//!
//! Fabric selects the destination, confirms storage, arms the target receiver,
//! optionally quiesces/exports VM-edge network state, starts source VMM
//! transport, then activates the receiver and restores network state.

use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use uuid::Uuid;
use zyvor_fabric_fluxvm_client::{
    BackendKind, CreateVmRequest, FluxVmClient, MigrationMode, MigrationPhase,
    MigrationReceiverInfo, MigrationReceiverRequest, MigrationStartRequest, MigrationStateStatus,
    MigrationStatus, RuntimeCapabilities, VmNetworkStateSnapshot, VmStatus,
};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NativeMigrationOptions {
    #[serde(default)]
    pub mode: MigrationMode,
    #[serde(default)]
    pub bandwidth_mbps: Option<u64>,
    #[serde(default)]
    pub max_downtime_ms: Option<u64>,
    #[serde(default)]
    pub multifd_channels: Option<u8>,
    /// Fabric must set this only after checking the VM's storage policy and
    /// confirming the destination can see the exact same disk state.
    #[serde(default)]
    pub shared_storage_confirmed: bool,
    /// When true, quiesce + export source dataplane conntrack before VMM
    /// migration and restore/resume on the target after activate.
    #[serde(default)]
    pub transfer_network_state: bool,
}

impl Default for NativeMigrationOptions {
    fn default() -> Self {
        Self {
            mode: MigrationMode::PreCopy,
            bandwidth_mbps: None,
            max_downtime_ms: Some(300),
            multifd_channels: Some(4),
            shared_storage_confirmed: false,
            transfer_network_state: false,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PrepareReceiverOptions {
    pub disk_path: PathBuf,
    #[serde(default)]
    pub receiver_ttl_seconds: Option<u64>,
    /// Optional override; when absent, copies the source VM's create request
    /// with `migration_incoming` left for the server to force.
    #[serde(default)]
    pub spec: Option<CreateVmRequest>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PreparedTarget {
    pub receiver: MigrationReceiverInfo,
    /// `tcp:<listen_host>:<port>` suitable for source `start_migration`.
    pub target_uri: String,
}

#[derive(Clone)]
pub struct RuntimeMigrationManager {
    source: FluxVmClient,
    target: Option<FluxVmClient>,
}

impl RuntimeMigrationManager {
    pub fn new(source_fluxvm_url: impl AsRef<str>, token: Option<&str>) -> Result<Self> {
        Ok(Self {
            source: client_with_token(source_fluxvm_url, token)?,
            target: None,
        })
    }

    /// Dual-node prepared-target orchestration (source + target FluxVM URLs).
    pub fn with_target(
        source_fluxvm_url: impl AsRef<str>,
        target_fluxvm_url: impl AsRef<str>,
        token: Option<&str>,
    ) -> Result<Self> {
        Ok(Self {
            source: client_with_token(source_fluxvm_url, token)?,
            target: Some(client_with_token(target_fluxvm_url, token)?),
        })
    }

    fn target_client(&self) -> Result<&FluxVmClient> {
        self.target
            .as_ref()
            .context("target FluxVM URL is required for receiver operations")
    }

    pub async fn capabilities(&self) -> Result<RuntimeCapabilities> {
        self.source.runtime_capabilities().await
    }

    /// Arm an incoming QEMU receiver on the target FluxVM node.
    pub async fn prepare_receiver(
        &self,
        source_vm_name: &str,
        listen_host: &str,
        options: &PrepareReceiverOptions,
    ) -> Result<PreparedTarget> {
        let target = self.target_client()?;
        let source_vm = self
            .source
            .find_by_name(source_vm_name)
            .await?
            .with_context(|| format!("FluxVM source has no VM named '{source_vm_name}'"))?;

        let mut spec = options
            .spec
            .clone()
            .unwrap_or_else(|| source_vm.request.clone());
        spec.migration_incoming = false; // server forces this
        if spec.name.is_empty() {
            spec.name = source_vm.name.clone();
        }

        let receiver = target
            .create_migration_receiver(&MigrationReceiverRequest {
                spec,
                disk_path: options.disk_path.clone(),
                receiver_ttl_seconds: options.receiver_ttl_seconds,
            })
            .await?;

        let host = listen_host.trim();
        if host.is_empty() {
            bail!("listen_host is required to build tcp:<host>:<port> target_uri");
        }
        let target_uri = format!("tcp:{host}:{}", receiver.port);
        Ok(PreparedTarget {
            receiver,
            target_uri,
        })
    }

    pub async fn get_receiver(&self, id: Uuid) -> Result<MigrationReceiverInfo> {
        self.target_client()?.get_migration_receiver(id).await
    }

    pub async fn abort_receiver(&self, id: Uuid) -> Result<()> {
        self.target_client()?.abort_migration_receiver(id).await
    }

    pub async fn activate_receiver(
        &self,
        id: Uuid,
    ) -> Result<zyvor_fabric_fluxvm_client::VmRecord> {
        self.target_client()?.activate_migration_receiver(id).await
    }

    pub async fn network_migration_state(&self, vm_name: &str) -> Result<MigrationStateStatus> {
        let vm = self
            .source
            .find_by_name(vm_name)
            .await?
            .with_context(|| format!("FluxVM source has no VM named '{vm_name}'"))?;
        self.source.network_migration_state(vm.id).await
    }

    pub async fn network_migration_quiesce(&self, vm_name: &str) -> Result<MigrationStateStatus> {
        let vm = self
            .source
            .find_by_name(vm_name)
            .await?
            .with_context(|| format!("FluxVM source has no VM named '{vm_name}'"))?;
        self.source.network_migration_quiesce(vm.id).await
    }

    pub async fn network_migration_export(&self, vm_name: &str) -> Result<VmNetworkStateSnapshot> {
        let vm = self
            .source
            .find_by_name(vm_name)
            .await?
            .with_context(|| format!("FluxVM source has no VM named '{vm_name}'"))?;
        self.source.network_migration_export(vm.id).await
    }

    pub async fn network_migration_restore(
        &self,
        receiver_id: Uuid,
        snapshot: &VmNetworkStateSnapshot,
    ) -> Result<MigrationStateStatus> {
        self.target_client()?
            .network_migration_restore(receiver_id, snapshot)
            .await
    }

    pub async fn network_migration_resume(
        &self,
        receiver_id: Uuid,
    ) -> Result<MigrationStateStatus> {
        self.target_client()?
            .network_migration_resume(receiver_id)
            .await
    }

    /// Full prepared-target flow when a target client is configured:
    /// optional network quiesce/export → source VMM migrate → activate →
    /// optional network restore/resume.
    pub async fn migrate_prepared_target(
        &self,
        vm_name: &str,
        listen_host: &str,
        prepare: &PrepareReceiverOptions,
        options: &NativeMigrationOptions,
    ) -> Result<(PreparedTarget, MigrationStatus)> {
        let prepared = self.prepare_receiver(vm_name, listen_host, prepare).await?;

        let network_snapshot = if options.transfer_network_state {
            self.network_migration_quiesce(vm_name).await?;
            Some(self.network_migration_export(vm_name).await?)
        } else {
            None
        };

        let status = self
            .start_prepared_target(vm_name, &prepared.target_uri, options)
            .await
            .inspect_err(|_| {
                // Best-effort abort of the unused receiver on failure to start.
                let rid = prepared.receiver.id;
                let target = self.target.clone();
                tokio::spawn(async move {
                    if let Some(t) = target {
                        let _ = t.abort_migration_receiver(rid).await;
                    }
                });
            })?;

        // Poll until completed/failed before activate — callers may also poll
        // separately; here we do a single status read after start returns.
        if status.phase == MigrationPhase::Completed {
            self.activate_receiver(prepared.receiver.id).await?;
            if let Some(snap) = network_snapshot.as_ref() {
                self.network_migration_restore(prepared.receiver.id, snap)
                    .await?;
                self.network_migration_resume(prepared.receiver.id).await?;
            }
        }

        Ok((prepared, status))
    }

    /// Start source-side VMM transport to an already-prepared target receiver.
    ///
    /// `target_uri` is normally `tcp:<migration-address>:<port>`. This method
    /// refuses to proceed when FluxVM says shared storage is required and the
    /// Fabric caller has not explicitly confirmed it.
    pub async fn start_prepared_target(
        &self,
        vm_name: &str,
        target_uri: &str,
        options: &NativeMigrationOptions,
    ) -> Result<MigrationStatus> {
        let caps = self.capabilities().await?;
        if caps.api_version != "runtime.fluxvm.zyvor.io/v1" {
            bail!("unsupported FluxVM runtime contract {}", caps.api_version);
        }

        let vm = self
            .source
            .find_by_name(vm_name)
            .await?
            .with_context(|| format!("FluxVM source has no VM named '{vm_name}'"))?;
        if vm.backend != BackendKind::Qemu {
            bail!(
                "native migration contract v1 supports qemu only (VM backend={:?})",
                vm.backend
            );
        }
        if vm.status != VmStatus::Running {
            bail!("VM '{vm_name}' must be running before live migration");
        }

        let capability = caps
            .migration
            .iter()
            .find(|c| c.backend == vm.backend && c.live)
            .context("source FluxVM does not advertise live migration for this backend")?;
        if capability.requires_shared_storage && !options.shared_storage_confirmed {
            bail!(
                "source runtime requires shared storage for migration contract v1; Fabric must confirm storage reachability before cutover"
            );
        }
        let transport = target_uri.split(':').next().unwrap_or_default();
        if !capability.transports.iter().any(|t| t == transport) {
            bail!(
                "target migration transport '{transport}' not advertised by source FluxVM ({:?})",
                capability.transports
            );
        }
        if options.mode == MigrationMode::PostCopy && !capability.post_copy {
            bail!("source FluxVM does not advertise post-copy migration");
        }
        if options.multifd_channels.unwrap_or(1) > 1 && !capability.multifd {
            bail!("source FluxVM does not advertise multifd migration");
        }

        self.source
            .start_migration(
                vm.id,
                &MigrationStartRequest {
                    destination: target_uri.to_string(),
                    mode: options.mode,
                    bandwidth_mbps: options.bandwidth_mbps,
                    max_downtime_ms: options.max_downtime_ms,
                    multifd_channels: options.multifd_channels,
                },
            )
            .await
    }

    pub async fn status(&self, vm_name: &str) -> Result<MigrationStatus> {
        let vm = self
            .source
            .find_by_name(vm_name)
            .await?
            .with_context(|| format!("FluxVM source has no VM named '{vm_name}'"))?;
        self.source.migration_status(vm.id).await
    }

    pub async fn cancel(&self, vm_name: &str) -> Result<MigrationStatus> {
        let vm = self
            .source
            .find_by_name(vm_name)
            .await?
            .with_context(|| format!("FluxVM source has no VM named '{vm_name}'"))?;
        self.source.cancel_migration(vm.id).await
    }
}

fn client_with_token(url: impl AsRef<str>, token: Option<&str>) -> Result<FluxVmClient> {
    let client = FluxVmClient::new(url)?;
    Ok(match token {
        Some(token) if !token.is_empty() => client.with_token(token.to_owned()),
        _ => client,
    })
}

/// Deterministic UI/API progress estimate from QEMU's RAM counters.
pub fn progress_percent(status: &MigrationStatus) -> u8 {
    match status.phase {
        MigrationPhase::Completed => return 100,
        MigrationPhase::Failed | MigrationPhase::Cancelled => return 0,
        _ => {}
    }
    let transferred = status.ram_transferred.unwrap_or(0);
    let total = status
        .ram_total
        .or_else(|| status.ram_remaining.map(|r| transferred.saturating_add(r)))
        .unwrap_or(0);
    if total == 0 {
        return match status.phase {
            MigrationPhase::Setup => 1,
            MigrationPhase::Active | MigrationPhase::PostcopyActive => 50,
            _ => 0,
        };
    }
    ((transferred.saturating_mul(100) / total).min(99)) as u8
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{Duration, Utc};
    use serde_json::json;
    use wiremock::matchers::{method, path, path_regex};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    fn status(phase: MigrationPhase, transferred: u64, remaining: u64) -> MigrationStatus {
        MigrationStatus {
            phase,
            status: format!("{phase:?}"),
            ram_transferred: Some(transferred),
            ram_remaining: Some(remaining),
            ram_total: None,
            total_time_ms: None,
            downtime_ms: None,
            error: None,
        }
    }

    #[test]
    fn progress_uses_ram_counters_but_reserves_100_for_completed() {
        assert_eq!(
            progress_percent(&status(MigrationPhase::Active, 75, 25)),
            75
        );
        assert_eq!(
            progress_percent(&status(MigrationPhase::Active, 100, 0)),
            99
        );
        assert_eq!(
            progress_percent(&status(MigrationPhase::Completed, 100, 0)),
            100
        );
    }

    #[test]
    fn default_requires_explicit_shared_storage_confirmation() {
        assert!(!NativeMigrationOptions::default().shared_storage_confirmed);
    }

    fn sample_vm(name: &str) -> serde_json::Value {
        json!({
            "id": "aaaaaaaa-aaaa-aaaa-aaaa-aaaaaaaaaaaa",
            "name": name,
            "backend": "qemu",
            "status": "running",
            "pid": null,
            "created_at": "2026-01-01T00:00:00Z",
            "expires_at": null,
            "workspace": "/tmp",
            "disk": "/tmp/disk.qcow2",
            "seed_disk": null,
            "tap_name": "tap0",
            "control_socket": null,
            "log_path": "/tmp/log",
            "error": null,
            "request": {
                "name": name,
                "backend": "qemu",
                "image": "/tmp/base.qcow2",
                "vcpus": 1,
                "memory_mib": 512
            },
            "virtiofsd_pids": [],
            "dhcp_leasefile": null
        })
    }

    #[tokio::test]
    async fn prepare_receiver_builds_tcp_target_uri() {
        let source = MockServer::start().await;
        let target = MockServer::start().await;
        let recv_id = Uuid::parse_str("00000000-0000-0000-0000-000000000001").unwrap();
        let expires = Utc::now() + Duration::seconds(120);

        Mock::given(method("GET"))
            .and(path("/v1/vms"))
            .respond_with(
                ResponseTemplate::new(200).set_body_json(json!({"items": [sample_vm("src-vm")]})),
            )
            .mount(&source)
            .await;

        Mock::given(method("POST"))
            .and(path("/v1/migration/receivers"))
            .respond_with(ResponseTemplate::new(201).set_body_json(json!({
                "id": recv_id,
                "status": "receiving",
                "port": 5555,
                "expires_at": expires
            })))
            .mount(&target)
            .await;

        let mgr = RuntimeMigrationManager::with_target(source.uri(), target.uri(), None).unwrap();
        let prepared = mgr
            .prepare_receiver(
                "src-vm",
                "10.0.0.9",
                &PrepareReceiverOptions {
                    disk_path: "/data/shared.qcow2".into(),
                    receiver_ttl_seconds: Some(60),
                    spec: None,
                },
            )
            .await
            .unwrap();
        assert_eq!(prepared.target_uri, "tcp:10.0.0.9:5555");
        assert_eq!(prepared.receiver.port, 5555);
    }

    #[tokio::test]
    async fn activate_and_abort_hit_target_receiver_routes() {
        let source = MockServer::start().await;
        let target = MockServer::start().await;
        let recv_id = Uuid::parse_str("00000000-0000-0000-0000-000000000001").unwrap();

        Mock::given(method("POST"))
            .and(path(format!("/v1/migration/receivers/{recv_id}/activate")))
            .respond_with(ResponseTemplate::new(200).set_body_json(sample_vm("recv")))
            .mount(&target)
            .await;

        Mock::given(method("DELETE"))
            .and(path(format!("/v1/migration/receivers/{recv_id}")))
            .respond_with(ResponseTemplate::new(204))
            .mount(&target)
            .await;

        // unused source matcher so with_target constructs cleanly
        Mock::given(method("GET"))
            .and(path_regex(r"/.*"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({})))
            .mount(&source)
            .await;

        let mgr = RuntimeMigrationManager::with_target(source.uri(), target.uri(), None).unwrap();
        let _ = mgr.activate_receiver(recv_id).await.unwrap();
        mgr.abort_receiver(recv_id).await.unwrap();
    }
}
