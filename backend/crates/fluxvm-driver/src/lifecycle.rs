// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: LicenseRef-Zyvor-Production-1.0

use std::collections::HashMap;
use std::path::PathBuf;

use anyhow::{bail, Context, Result};
use async_trait::async_trait;
use vm_model::{VMStartOptions, VMState, VM};
use zyvor_fabric_driver_core::{MachineInfo, VMDriver};
use zyvor_fabric_fluxvm_client::{
    BackendKind, CreateVmRequest, DirectMode, DirectSpec, NetworkSpec, PortForward, QgaSpec,
    StorageBackend, VmRecord, VmStatus,
};

use crate::FluxVmDriver;

fn generate_mac_address() -> String {
    use rand::RngExt;
    let mut rng = rand::rng();
    format!(
        "52:54:00:{:02x}:{:02x}:{:02x}",
        rng.random::<u8>(),
        rng.random::<u8>(),
        rng.random::<u8>()
    )
}

#[async_trait]
impl VMDriver for FluxVmDriver {
    async fn start(&self, name: &str) -> Result<()> {
        let vm = self.resolve(name).await?;
        self.client.start_vm(vm.id).await.map(|_| ())
    }

    async fn start_with_options(&self, vm: &VM, opts: &VMStartOptions) -> Result<()> {
        // Already known to FluxVM: options were (or should have been)
        // baked in at creation time — replay the stored request rather
        // than trying to apply a second, possibly-different option set.
        if let Some(record) = self.client.find_by_name(&vm.name).await? {
            // Direct uplink, netns tap, and user NAT are fixed at FluxVM
            // create time. A later start must not replay a stale record
            // after Fabric's stored network changed (same reason port
            // forwards delete the record). MAC is ignored: it is pinned
            // fresh on each create and must not force a recreate.
            let desired = network_key(vm, opts)?;
            if network_key_of(&record.request.network) != desired {
                self.client.delete_vm(record.id).await?;
                let req = translate_start_options(vm, opts)?;
                self.client.create_vm(&req).await?;
                return Ok(());
            }
            return self.client.start_vm(record.id).await.map(|_| ());
        }
        // First launch: translate into an FluxVM CreateVmRequest.
        let req = translate_start_options(vm, opts)?;
        self.client.create_vm(&req).await.map(|_| ())
    }

    async fn start_from_snapshot(&self, name: &str, tag: &str) -> Result<()> {
        let vm = self.resolve(name).await?;
        self.client
            .start_vm_from_snapshot(vm.id, tag)
            .await
            .map(|_| ())
    }

    async fn poweroff(&self, name: &str) -> Result<()> {
        let vm = self.resolve(name).await?;
        self.client.stop_vm(vm.id).await.map(|_| ())
    }

    async fn terminate(&self, name: &str) -> Result<()> {
        // FluxVM's `stop` already does graceful-shutdown-then-SIGKILL
        // internally (see `VmManager::stop`) — there's no separate "force"
        // endpoint to escalate to, so terminate and poweroff are the same
        // call here.
        let vm = self.resolve(name).await?;
        self.client.stop_vm(vm.id).await.map(|_| ())
    }

    async fn delete(&self, name: &str) -> Result<()> {
        let vm = self.resolve(name).await?;
        self.client.delete_vm(vm.id).await
    }

    async fn reboot(&self, name: &str) -> Result<()> {
        // FluxVM has no reboot endpoint yet (would need a per-backend QMP
        // `system_reset`/`ch-remote` call — see the migration plan). Stop
        // then start is a coarser substitute: the guest sees a full
        // poweroff and cold boot rather than a soft reset, but it's a
        // functional stand-in until that lands.
        let vm = self.resolve(name).await?;
        self.client.stop_vm(vm.id).await?;
        self.client.start_vm(vm.id).await.map(|_| ())
    }

    async fn pause(&self, name: &str) -> Result<()> {
        let vm = self.resolve(name).await?;
        self.client.pause_vm(vm.id).await.map(|_| ())
    }

    async fn resume(&self, name: &str) -> Result<()> {
        let vm = self.resolve(name).await?;
        self.client.resume_vm(vm.id).await.map(|_| ())
    }

    async fn get_state(&self, name: &str) -> Result<VMState> {
        match self.client.find_by_name(name).await? {
            Some(vm) => Ok(map_status(vm.status)),
            None => Ok(VMState::Stopped),
        }
    }

    async fn list_machines(&self) -> Result<Vec<MachineInfo>> {
        Ok(self
            .client
            .list_vms()
            .await?
            .into_iter()
            .map(to_machine_info)
            .collect())
    }

    async fn get_properties(&self, name: &str) -> Result<HashMap<String, String>> {
        let vm = self.resolve(name).await?;
        Ok(properties_of(&vm))
    }

    async fn get_leader_pid(&self, name: &str) -> Result<u32> {
        let vm = self.resolve(name).await?;
        vm.pid
            .with_context(|| format!("VM '{name}' has no leader pid (not running)"))
    }

    async fn enable(&self, _name: &str) -> Result<()> {
        // "Enable at boot" doesn't belong in FluxVM (a disposable-VM
        // engine, not a service manager) — per the migration plan this
        // becomes an `autostart` flag in zyvor-fabricd's own StateStore plus a
        // startup reconciliation pass, not yet wired up. No-op for now
        // rather than erroring, since nothing in zyvor-fabricd calls this yet.
        Ok(())
    }

    async fn disable(&self, _name: &str) -> Result<()> {
        Ok(())
    }

    async fn get_control_socket(&self, name: &str) -> Result<Option<std::path::PathBuf>> {
        Ok(self.resolve(name).await?.control_socket)
    }

    async fn get_mac_address(&self, name: &str) -> Result<Option<String>> {
        Ok(match self.resolve(name).await?.request.network {
            NetworkSpec::Tap { mac, .. } => mac,
            _ => None,
        })
    }

    async fn get_vnc_socket(&self, name: &str) -> Result<Option<std::path::PathBuf>> {
        Ok(Some(self.resolve(name).await?.workspace.join("vnc.sock")))
    }

    async fn get_disk_path(&self, name: &str) -> Result<std::path::PathBuf> {
        Ok(self.resolve(name).await?.disk)
    }

    async fn get_cgroup_path(&self, name: &str) -> Result<Option<std::path::PathBuf>> {
        Ok(self.resolve(name).await?.cgroup_path)
    }
}

pub(crate) fn map_status(status: VmStatus) -> VMState {
    match status {
        VmStatus::Creating => VMState::Starting,
        VmStatus::Running => VMState::Running,
        VmStatus::Paused => VMState::Paused,
        VmStatus::Stopped => VMState::Stopped,
        VmStatus::Failed => VMState::Failed,
        VmStatus::Receiving => VMState::Starting,
    }
}

fn to_machine_info(vm: VmRecord) -> MachineInfo {
    MachineInfo {
        name: vm.name,
        class: "vm".to_string(),
        service: "fluxvm".to_string(),
        state: map_status(vm.status),
        leader_pid: vm.pid,
    }
}

fn properties_of(vm: &VmRecord) -> HashMap<String, String> {
    let mut props = HashMap::new();
    props.insert("Name".to_string(), vm.name.clone());
    props.insert("Class".to_string(), "vm".to_string());
    props.insert("Service".to_string(), "fluxvm".to_string());
    props.insert(
        "State".to_string(),
        format!("{:?}", vm.status).to_lowercase(),
    );
    props.insert(
        "Leader".to_string(),
        vm.pid.map_or_else(String::new, |p| p.to_string()),
    );
    props.insert("Backend".to_string(), format!("{:?}", vm.backend));
    props.insert("Id".to_string(), vm.id.to_string());
    props.insert("Disk".to_string(), vm.disk.display().to_string());
    if let Some(tap) = &vm.tap_name {
        props.insert("TapName".to_string(), tap.clone());
    }
    // Only ever set for NetworkSpec::Tap { netns: true, .. } VMs (FluxVM
    // resolves this fresh from the per-namespace DHCP lease file on every
    // read) -- the frontend's Network tab already reads properties
    // .IPAddress with no changes needed on that end.
    if let Some(ip) = &vm.guest_ip {
        props.insert("IPAddress".to_string(), ip.clone());
    }
    props
}

/// Translate a `vm-model` `VM`/`VMStartOptions` pair — systemd-vmspawn's
/// launch-option shape — into an FluxVM `CreateVmRequest`. Errors loudly
/// on any option FluxVM has no equivalent for yet (per the
/// systemd-removal migration plan's FluxVM gap list) rather than
/// silently dropping it, since a dropped option is a correctness bug a
/// caller has no way to notice.
fn translate_start_options(vm: &VM, opts: &VMStartOptions) -> Result<CreateVmRequest> {
    if opts.directory.is_some() {
        bail!(
            "the fluxvm backend does not support directory-based boot (VMStartOptions.directory)"
        );
    }
    if opts.tpm == Some(true) {
        bail!("the fluxvm backend does not yet support TPM (VMStartOptions.tpm)");
    }
    if opts.secure_boot == Some(true) {
        bail!("the fluxvm backend does not yet support secure boot (VMStartOptions.secure_boot)");
    }
    if opts.vsock == Some(true) {
        bail!(
            "the fluxvm backend does not support raw vsock passthrough (VMStartOptions.vsock) \
             — use CreateVmRequest.agent for the in-guest vsock agent instead"
        );
    }
    if !opts.extra_drives.is_empty() {
        bail!("the fluxvm backend does not support extra drives (VMStartOptions.extra_drives)");
    }
    if !opts.bind_users.is_empty() {
        bail!("the fluxvm backend does not support bind users (VMStartOptions.bind_users)");
    }
    if !opts.credentials.is_empty() || !opts.load_credentials.is_empty() {
        bail!(
            "the fluxvm backend does not support systemd credentials \
             (VMStartOptions.credentials/load_credentials) — use cloud_init instead"
        );
    }
    if !opts.smbios11.is_empty() {
        bail!("the fluxvm backend does not support SMBIOS injection (VMStartOptions.smbios11)");
    }

    let vcpus: u8 = vm.cpus.try_into().map_err(|_| {
        anyhow::anyhow!("vcpu count {} exceeds the fluxvm backend's limit", vm.cpus)
    })?;

    let network = if let Some(direct) = direct_tap(vm, opts)? {
        direct
    } else if opts.network_tap {
        // Always pin an explicit MAC rather than letting QEMU auto-assign
        // one: FluxVM never persists an auto-generated MAC anywhere on
        // VmRecord, so a later ssh_info lookup (get_mac_address, below)
        // would have nothing to resolve to a DHCP-leased IP.
        let mac = vm.mac_address.clone().unwrap_or_else(generate_mac_address);
        // netns: true rather than a shared host bridge -- gives the VM its
        // own network namespace with a per-namespace dnsmasq DHCP server
        // (FluxVM's fluxvm_network::netns), so it gets a real,
        // externally-reachable IP with zero host bridge configuration
        // needed on this end. `bridge` is ignored by FluxVM when netns
        // is set.
        NetworkSpec::Tap {
            tap_name: None,
            bridge: None,
            mac: Some(mac),
            netns: true,
            direct: None,
        }
    } else {
        NetworkSpec::User {
            forwards: opts
                .port_forwards
                .iter()
                .map(|f| PortForward {
                    host_port: f.host_port,
                    guest_port: f.guest_port,
                    protocol: f.protocol.clone(),
                })
                .collect(),
        }
    };

    Ok(CreateVmRequest {
        name: vm.name.clone(),
        backend: BackendKind::Qemu,
        image: PathBuf::from(&vm.image),
        vcpus,
        memory_mib: vm.memory,
        max_vcpus: None,
        max_memory_mib: None,
        disk_size_gib: if vm.disk > 0 { Some(vm.disk) } else { None },
        kernel: opts.linux.clone().map(PathBuf::from),
        initrd: opts.initrd.first().cloned().map(PathBuf::from),
        firmware: opts.firmware.clone().map(PathBuf::from),
        kernel_args: if opts.extra_args.is_empty() {
            None
        } else {
            Some(opts.extra_args.join(" "))
        },
        loadvm_tag: None,
        network,
        // Always attach a cloud-init seed, even an empty one, not just when
        // there's an ssh key/hostname to inject -- without ANY NoCloud
        // datasource present, cloud-init never finds one to read and never
        // runs its own default network config, which is what actually
        // brings the guest's DHCP client up. Found live: a VM started with
        // no cloud-init configured at all sat forever on
        // systemd-networkd-wait-online with no working network -- true even
        // for a stock Ubuntu cloud image, not something specific to a
        // hand-built test image. `static_network` stays gated on netns tap
        // networking -- it needs a reserved address to inject, which only
        // that mode has (see FluxVM's fluxvm_network::netns::NetnsHandle).
        cloud_init: Some(zyvor_fabric_fluxvm_client::CloudInitSpec {
            hostname: vm.hostname.clone(),
            ssh_authorized_keys: opts.ssh_authorized_keys.clone(),
            static_network: opts.network_tap && opts.network_static_ip,
            packages: opts.cloud_init_packages.clone(),
            runcmd: opts.cloud_init_runcmd.clone(),
            write_files: opts
                .cloud_init_write_files
                .iter()
                .map(|f| zyvor_fabric_fluxvm_client::CloudInitFile {
                    path: f.path.clone(),
                    content: f.content.clone(),
                    permissions: f.permissions.clone(),
                })
                .collect(),
            ..Default::default()
        }),
        ttl_seconds: None,
        extra_args: vec![],
        // Enabled by default: needed for ShellDriver::shell, ConsoleDriver,
        // and file copy to work on any VM without a separate opt-in. Was
        // gated off pending a real, live-verified fix for an intermittent
        // guest-agent vsock listener bug — see FluxVM's README
        // ("Interactive console" section) and docs/guides/vm-drivers/fluxvm.md.
        agent: Some(zyvor_fabric_fluxvm_client::AgentSpec {
            enabled: true,
            ..Default::default()
        }),
        qga: if opts.enable_qga {
            Some(QgaSpec { enabled: true })
        } else {
            None
        },
        hyperv: opts.hyperv,
        storage: parse_storage_backend(opts.storage.as_deref()),
        shared_folders: opts
            .bind_mounts
            .iter()
            .map(|bm| zyvor_fabric_fluxvm_client::SharedFolder {
                host_path: PathBuf::from(&bm.source),
                guest_path: bm.destination.clone().unwrap_or_else(|| bm.source.clone()),
                read_only: bm.read_only,
            })
            .collect(),
        numa_node: opts.numa_node,
        cpuset: opts.cpuset.clone(),
        hugepages: opts.hugepages,
        vfio_devices: opts.vfio_devices.clone(),
        pod_uid: None,
        migration_incoming: false,
        // Prefer explicit label `tenant=…` so Fabric project/billing labels
        // flow into FluxVM's first-class tenant filter (`GET /v1/vms?tenant=`).
        tenant: vm
            .labels
            .as_ref()
            .and_then(|l| l.get("tenant").cloned())
            .or_else(|| {
                vm.tags.as_ref().and_then(|tags| {
                    tags.iter()
                        .find_map(|t| t.strip_prefix("tenant:").map(|s| s.to_string()))
                })
            }),
    })
}

/// Bridge-less tap when a direct uplink is set on the start options, or
/// (if those are empty) on the stored VM. `None` keeps the netns or NAT path.
fn direct_tap(vm: &VM, opts: &VMStartOptions) -> Result<Option<NetworkSpec>> {
    let from_opts = opts
        .direct_uplink
        .as_ref()
        .map(|s| !s.trim().is_empty())
        .unwrap_or(false);
    let (uplink, mode, guest_ips) = if from_opts {
        (
            opts.direct_uplink.clone(),
            opts.direct_mode.clone(),
            opts.direct_guest_ips.clone(),
        )
    } else {
        (
            vm.direct_uplink.clone(),
            vm.direct_mode.clone(),
            vm.direct_guest_ips.clone(),
        )
    };
    let Some(outer) = uplink
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
    else {
        return Ok(None);
    };
    let errors = vm_model::direct_uplink_errors(
        Some(&outer),
        mode.as_deref(),
        &guest_ips,
        opts.network_tap,
        opts.network_user_mode || !opts.port_forwards.is_empty(),
    );
    if !errors.is_empty() {
        bail!("{}", errors.join("; "));
    }
    let parsed = match mode.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
        None | Some("l2-uplink") => DirectMode::L2Uplink,
        Some("peer-veth") => DirectMode::PeerVeth,
        Some(other) => bail!("direct_mode must be 'l2-uplink' or 'peer-veth', got '{other}'"),
    };
    let mac = vm.mac_address.clone().unwrap_or_else(generate_mac_address);
    Ok(Some(NetworkSpec::Tap {
        tap_name: None,
        bridge: None,
        mac: Some(mac),
        netns: false,
        direct: Some(DirectSpec {
            outer,
            netns_path: None,
            mode: parsed,
            guest_ips,
        }),
    }))
}

/// Identity of the create-time network, ignoring MAC and tap name.
#[derive(Debug, Clone, PartialEq, Eq)]
enum NetworkKey {
    User,
    Netns,
    Direct {
        outer: String,
        mode: String,
        guest_ips: Vec<String>,
    },
}

fn network_key_of(spec: &NetworkSpec) -> NetworkKey {
    match spec {
        NetworkSpec::Tap {
            direct: Some(direct),
            ..
        } => {
            let mut guest_ips = direct.guest_ips.clone();
            guest_ips.sort();
            let mode = match direct.mode {
                DirectMode::L2Uplink => "l2-uplink",
                DirectMode::PeerVeth => "peer-veth",
            };
            NetworkKey::Direct {
                outer: direct.outer.clone(),
                mode: mode.to_string(),
                guest_ips,
            }
        }
        NetworkSpec::Tap { netns: true, .. } => NetworkKey::Netns,
        _ => NetworkKey::User,
    }
}

fn network_key(vm: &VM, opts: &VMStartOptions) -> Result<NetworkKey> {
    Ok(match direct_tap(vm, opts)? {
        Some(spec) => network_key_of(&spec),
        None if opts.network_tap => NetworkKey::Netns,
        None => NetworkKey::User,
    })
}

fn parse_storage_backend(raw: Option<&str>) -> StorageBackend {
    match raw.map(|s| s.trim().to_ascii_lowercase()).as_deref() {
        None | Some("") | Some("default") => StorageBackend::Default,
        Some("lvm-thin") | Some("lvm_thin") | Some("lvm") => StorageBackend::LvmThin,
        Some("nbd") => StorageBackend::Nbd,
        Some("ceph-rbd") | Some("ceph_rbd") | Some("ceph") | Some("rbd") => StorageBackend::CephRbd,
        Some(_) => StorageBackend::Default,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use vm_model::BindMount;

    #[test]
    fn bind_mounts_translate_to_shared_folders() {
        let vm = VM::new(
            "fixture".to_string(),
            "/tmp/base.qcow2".to_string(),
            2,
            2048,
        );
        let opts = VMStartOptions {
            bind_mounts: vec![
                BindMount {
                    source: "/srv/data".to_string(),
                    destination: Some("/mnt/data".to_string()),
                    read_only: true,
                },
                BindMount {
                    source: "/srv/scratch".to_string(),
                    destination: None,
                    read_only: false,
                },
            ],
            ..Default::default()
        };
        let req = translate_start_options(&vm, &opts).unwrap();
        assert_eq!(req.shared_folders.len(), 2);
        assert_eq!(req.shared_folders[0].host_path, PathBuf::from("/srv/data"));
        assert_eq!(req.shared_folders[0].guest_path, "/mnt/data");
        assert!(req.shared_folders[0].read_only);
        // No destination -> mounted at the same path inside the guest.
        assert_eq!(req.shared_folders[1].guest_path, "/srv/scratch");
        assert!(!req.shared_folders[1].read_only);
    }

    #[test]
    fn no_bind_mounts_means_no_shared_folders() {
        let vm = VM::new(
            "fixture".to_string(),
            "/tmp/base.qcow2".to_string(),
            2,
            2048,
        );
        let req = translate_start_options(&vm, &VMStartOptions::default()).unwrap();
        assert!(req.shared_folders.is_empty());
    }

    #[test]
    fn direct_uplink_emits_l2_uplink() {
        let mut vm = VM::new(
            "fixture".to_string(),
            "/tmp/base.qcow2".to_string(),
            2,
            2048,
        );
        vm.direct_uplink = Some("enp1s0".to_string());
        vm.direct_guest_ips = vec!["192.168.1.50".to_string()];
        let opts = VMStartOptions {
            direct_uplink: Some("enp1s0".to_string()),
            direct_guest_ips: vec!["192.168.1.50".to_string()],
            ..Default::default()
        };
        let req = translate_start_options(&vm, &opts).unwrap();
        match req.network {
            NetworkSpec::Tap {
                netns,
                bridge,
                direct,
                ..
            } => {
                assert!(!netns);
                assert!(bridge.is_none());
                let spec = direct.expect("direct");
                assert_eq!(spec.outer, "enp1s0");
                assert_eq!(spec.mode, DirectMode::L2Uplink);
                assert_eq!(spec.guest_ips, vec!["192.168.1.50".to_string()]);
            }
            other => panic!("expected tap, got {other:?}"),
        }
    }

    #[test]
    fn direct_uplink_rejects_network_tap() {
        let vm = VM::new(
            "fixture".to_string(),
            "/tmp/base.qcow2".to_string(),
            2,
            2048,
        );
        let opts = VMStartOptions {
            direct_uplink: Some("enp1s0".to_string()),
            network_tap: true,
            ..Default::default()
        };
        let err = translate_start_options(&vm, &opts).unwrap_err();
        assert!(
            err.to_string().contains("network_tap"),
            "unexpected error: {err}"
        );
    }

    #[test]
    fn network_key_ignores_mac_and_sees_an_uplink_change() {
        let vm = VM::new(
            "fixture".to_string(),
            "/tmp/base.qcow2".to_string(),
            2,
            2048,
        );
        let direct = VMStartOptions {
            direct_uplink: Some("enp1s0".to_string()),
            direct_guest_ips: vec!["192.168.1.50".to_string()],
            ..Default::default()
        };
        let again = VMStartOptions {
            direct_uplink: Some("enp1s0".to_string()),
            direct_guest_ips: vec!["192.168.1.50".to_string()],
            ..Default::default()
        };
        let nat = VMStartOptions::default();
        let key = network_key(&vm, &direct).unwrap();
        assert_eq!(key, network_key(&vm, &again).unwrap());
        assert_ne!(key, network_key(&vm, &nat).unwrap());

        let stored = translate_start_options(&vm, &direct).unwrap();
        let replay = translate_start_options(&vm, &again).unwrap();
        assert_eq!(
            network_key_of(&stored.network),
            network_key_of(&replay.network)
        );
    }
}
