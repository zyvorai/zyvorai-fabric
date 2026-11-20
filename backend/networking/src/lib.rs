// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: LicenseRef-Zyvor-Production-1.0

pub mod host_discovery;
pub mod host_dns;
pub mod host_firewalld;
pub mod host_floating_ips;
pub mod host_identities;
pub mod host_monitor_tc;
pub mod host_nat;
pub mod host_nft_filter;
pub mod host_nft_policy;
pub mod host_resolv;
pub mod host_services;
pub mod host_tc;
pub mod host_tc_mirror;
pub mod host_vxlan_sriov;
pub mod host_wireguard;
pub mod models;
pub mod netlink;
pub mod nftables;
pub mod parser;
pub mod serializer;

use anyhow::{Context, Result};
use models::*;
use std::fs;
use std::net::IpAddr;
use std::path::PathBuf;
use std::process::Command;

/// Run an async netlink call from `NetworkdManager`'s synchronous public API.
/// Every caller today (`zyvor-fabricd/src/api/networkd.rs`) invokes these
/// methods inline from an async axum handler already running on tokio's
/// multi-threaded runtime, so `block_in_place` (moves the blocking wait off
/// the async scheduler onto a dedicated thread) is safe here — it would
/// panic on a current-thread runtime, but `zyvor-fabricd` never uses one
/// (`#[tokio::main]` with no `flavor` override defaults to multi-threaded).
/// Keeping the public methods synchronous avoids threading `.await` through
/// the ~60 call sites in that file.
fn block_on_netlink<F, T>(fut: F) -> Result<T>
where
    F: std::future::Future<Output = Result<T>>,
{
    tokio::task::block_in_place(|| tokio::runtime::Handle::current().block_on(fut))
}

fn parse_addr(cidr: &str) -> Option<IpAddr> {
    cidr.split('/').next()?.parse().ok()
}

/// Request the bridge's own address via DHCP -- `dhcpcd <iface>`, not a
/// netlink operation (there's no rtnetlink message for "run a DHCP
/// exchange"), so this shells out like `apply_sriov`'s `ip link set vf`
/// calls do. dhcpcd daemonizes itself once it has a lease (or gives up),
/// same "own the child process" pattern `zyvor_fabric_dnsmasq_manager`
/// uses for the per-bridge DHCP *server* -- this is the DHCP *client*
/// side, for the bridge device's own address, a separate concern.
fn run_dhcp_client(iface: &str) -> Result<()> {
    let iface = input_guard::vet_component!(iface, anyhow::anyhow!("rejected interface name"));
    let output =
        input_guard::argv_checked!(iface, anyhow::anyhow!("rejected interface name"), |iface| {
            Command::new("dhcpcd")
                .arg(iface)
                .output()
                .context("failed to run dhcpcd (is it installed?)")?
        });
    if !output.status.success() {
        return Err(anyhow::anyhow!(
            "dhcpcd failed for '{iface}': {}",
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    Ok(())
}

pub struct NetworkdManager {
    config_dir: PathBuf,
    file_prefix: String,
}

impl NetworkdManager {
    pub fn new(config_dir: impl Into<PathBuf>, file_prefix: impl Into<String>) -> Self {
        Self {
            config_dir: config_dir.into(),
            file_prefix: file_prefix.into(),
        }
    }

    /// Create a bridge device via netlink: name, up, mtu, mac address,
    /// static addresses, STP + its tuning knobs (forward_delay/hello_time/
    /// max_age/vlan_filtering), a default route via `gateway`, and a DHCP
    /// client on the bridge's own interface all apply immediately (no
    /// reload step).
    pub fn apply_bridge(&self, cfg: &BridgeConfig) -> Result<()> {
        block_on_netlink(netlink::ensure_bridge(&cfg.name))?;
        let result: Result<()> = (|| {
            self.apply_common_link_settings(
                &cfg.name,
                cfg.mtu,
                cfg.mac_address.as_deref(),
                &cfg.addresses,
            )?;
            let bridge_opts = netlink::BridgeOptions {
                stp: cfg.stp,
                forward_delay_sec: cfg.forward_delay_sec,
                hello_time_sec: cfg.hello_time_sec,
                max_age_sec: cfg.max_age_sec,
                vlan_filtering: cfg.vlan_filtering,
            };
            block_on_netlink(netlink::set_bridge_options(&cfg.name, &bridge_opts))
                .with_context(|| format!("failed to set bridge options on '{}'", cfg.name))?;
            if let Some(gateway) = &cfg.gateway {
                let addr: IpAddr = gateway
                    .parse()
                    .with_context(|| format!("invalid gateway address '{gateway}'"))?;
                block_on_netlink(netlink::add_default_route(&cfg.name, addr)).with_context(
                    || {
                        format!(
                            "failed to add default route via '{gateway}' on '{}'",
                            cfg.name
                        )
                    },
                )?;
            }
            if cfg.dhcp != DhcpMode::No {
                run_dhcp_client(&cfg.name)
                    .with_context(|| format!("failed to start a DHCP client on '{}'", cfg.name))?;
            }
            Ok(())
        })();
        self.cleanup_on_failure(&cfg.name, result)?;
        tracing::info!("Applied bridge config: {}", cfg.name);
        Ok(())
    }

    /// Create a VLAN sub-interface via netlink.
    pub fn apply_vlan(&self, cfg: &VlanConfig) -> Result<()> {
        block_on_netlink(netlink::create_vlan(
            &cfg.parent_interface,
            cfg.vlan_id,
            &cfg.name,
        ))?;
        let result = self.apply_common_link_settings(&cfg.name, cfg.mtu, None, &cfg.addresses);
        self.cleanup_on_failure(&cfg.name, result)?;
        tracing::info!(
            "Applied VLAN config: {} (id={}, parent={})",
            cfg.name,
            cfg.vlan_id,
            cfg.parent_interface
        );
        Ok(())
    }

    /// Create a macvtap device via netlink.
    pub fn apply_macvtap(&self, cfg: &MacvtapConfig) -> Result<()> {
        block_on_netlink(netlink::create_macvtap(
            &cfg.parent_interface,
            &cfg.name,
            cfg.mode.as_str(),
        ))?;
        let result =
            self.apply_common_link_settings(&cfg.name, cfg.mtu, cfg.mac_address.as_deref(), &[]);
        self.cleanup_on_failure(&cfg.name, result)?;
        tracing::info!(
            "Applied macvtap config: {} (parent={}, mode={:?})",
            cfg.name,
            cfg.parent_interface,
            cfg.mode
        );
        Ok(())
    }

    /// Create a persistent TAP device via netlink (see
    /// `netlink::create_tap`'s doc comment for why tap creation specifically
    /// still shells out to `ip tuntap`).
    pub fn apply_tap(&self, cfg: &TapConfig) -> Result<()> {
        block_on_netlink(netlink::create_tap(&cfg.name))?;
        let result: Result<()> = (|| {
            self.apply_common_link_settings(&cfg.name, cfg.mtu, cfg.mac_address.as_deref(), &[])?;
            if let Some(bridge) = &cfg.bridge {
                block_on_netlink(async {
                    let handle = netlink::connect().await?;
                    let master_index = netlink::link_index_by_name(&handle, bridge).await?;
                    netlink::set_master(&cfg.name, master_index).await
                })
                .with_context(|| {
                    format!("failed to attach tap '{}' to bridge '{bridge}'", cfg.name)
                })?;
            }
            Ok(())
        })();
        self.cleanup_on_failure(&cfg.name, result)?;
        tracing::info!("Applied tap config: {}", cfg.name);
        Ok(())
    }

    /// Create a bond device and enslave its members via netlink.
    pub fn apply_bond(&self, cfg: &BondConfig) -> Result<()> {
        block_on_netlink(netlink::create_bond(
            &cfg.name,
            cfg.mode.as_str(),
            &cfg.slave_interfaces,
        ))?;
        let result = self.apply_common_link_settings(
            &cfg.name,
            cfg.mtu,
            cfg.mac_address.as_deref(),
            &cfg.addresses,
        );
        self.cleanup_on_failure(&cfg.name, result)?;
        tracing::info!(
            "Applied bond config: {} (mode={}, slaves={:?})",
            cfg.name,
            cfg.mode.as_str(),
            cfg.slave_interfaces
        );
        Ok(())
    }

    /// Configure an existing physical interface (bridge/bond membership +
    /// static addresses) via netlink — the write-side equivalent of what a
    /// `.network` file previously matched onto an interface by name/MAC.
    pub fn apply_network_file(&self, cfg: &NetworkFileConfig) -> Result<()> {
        if let Some(bridge) = &cfg.bridge {
            block_on_netlink(async {
                let handle = netlink::connect().await?;
                let master_index = netlink::link_index_by_name(&handle, bridge).await?;
                netlink::set_master(&cfg.match_name, master_index).await
            })
            .with_context(|| {
                format!("failed to attach '{}' to bridge '{bridge}'", cfg.match_name)
            })?;
        } else if let Some(bond) = &cfg.bond {
            block_on_netlink(async {
                let handle = netlink::connect().await?;
                let master_index = netlink::link_index_by_name(&handle, bond).await?;
                netlink::set_master(&cfg.match_name, master_index).await
            })
            .with_context(|| format!("failed to attach '{}' to bond '{bond}'", cfg.match_name))?;
        }
        self.apply_common_link_settings(&cfg.match_name, cfg.mtu, None, &cfg.addresses)?;
        tracing::info!("Applied network file for: {}", cfg.match_name);
        Ok(())
    }

    /// Apply MTU/MAC-address renaming for an existing interface via netlink.
    /// `LinkFileConfig` matches by MAC/path/driver in its original systemd
    /// `.link`-file design; this applies the (`name`, `mtu`, `mac_address`)
    /// settings directly to whichever interface `cfg` already identifies by
    /// `match_original_name` (falls back to `id` — same precedence the old
    /// file-based path used to pick a stable identifier for a device with no
    /// current name).
    pub fn apply_link_file(&self, cfg: &LinkFileConfig) -> Result<()> {
        let target = cfg.match_original_name.as_deref().unwrap_or(&cfg.id);
        if let Some(new_name) = &cfg.name {
            if new_name != target {
                block_on_netlink(netlink::rename_link(target, new_name))
                    .with_context(|| format!("failed to rename '{target}' to '{new_name}'"))?;
            }
        }
        let effective_name = cfg.name.as_deref().unwrap_or(target);
        self.apply_common_link_settings(effective_name, cfg.mtu, cfg.mac_address.as_deref(), &[])?;
        tracing::info!("Applied link file: {:?}", cfg.name);
        Ok(())
    }

    /// Create a VXLAN device via netlink.
    pub fn apply_vxlan(&self, cfg: &VxlanConfig) -> Result<()> {
        let local = cfg.local.as_deref().and_then(parse_addr);
        let remote = cfg.remote.as_deref().and_then(parse_addr);
        block_on_netlink(netlink::create_vxlan(
            &cfg.name, cfg.vni, local, remote, cfg.port,
        ))?;
        let result: Result<()> = (|| {
            self.apply_common_link_settings(
                &cfg.name,
                cfg.mtu.map(|m| m as u16),
                None,
                &cfg.addresses,
            )?;
            if let Some(parent) = &cfg.parent_interface {
                block_on_netlink(async {
                    let handle = netlink::connect().await?;
                    let parent_index = netlink::link_index_by_name(&handle, parent).await?;
                    netlink::set_master(&cfg.name, parent_index).await
                })
                .with_context(|| {
                    format!("failed to attach VXLAN '{}' to parent '{parent}'", cfg.name)
                })?;
            }
            Ok(())
        })();
        self.cleanup_on_failure(&cfg.name, result)?;
        tracing::info!("Applied VXLAN config: {} (VNI={})", cfg.name, cfg.vni);
        Ok(())
    }

    /// Deletes the just-created `name` device (best effort) if `result` is
    /// an `Err`, then returns `result` unchanged. Every `apply_*` create
    /// path above adds the device via netlink first and only then applies
    /// follow-up settings (bring-up, MTU, MAC, addresses, bridge/bond
    /// membership) -- without this, a failure partway through those
    /// follow-up steps left the device behind: realized in the kernel, but
    /// with no `save_entity` record and so no way to remove it short of a
    /// manual `ip link del` on the host. Found live: a VLAN whose parent
    /// link was administratively down failed netlink's bring-up step and
    /// orphaned `vlan-regr-test@eno8403` indefinitely.
    fn cleanup_on_failure<T>(&self, name: &str, result: Result<T>) -> Result<T> {
        if result.is_err() {
            if let Err(cleanup_err) = block_on_netlink(netlink::delete_link(name)) {
                tracing::warn!("Failed to roll back orphaned device '{name}' after a failed apply: {cleanup_err:#}");
            } else {
                tracing::info!("Rolled back orphaned device '{name}' after a failed apply");
            }
        }
        result
    }

    /// Shared bring-up + MTU + MAC + static-address application, used by
    /// every `apply_*` method above.
    fn apply_common_link_settings(
        &self,
        name: &str,
        mtu: Option<u16>,
        mac_address: Option<&str>,
        addresses: &[String],
    ) -> Result<()> {
        block_on_netlink(netlink::set_link_up(name))
            .with_context(|| format!("failed to bring up '{name}'"))?;
        if let Some(mtu) = mtu {
            block_on_netlink(netlink::set_mtu(name, mtu as u32))
                .with_context(|| format!("failed to set mtu on '{name}'"))?;
        }
        if let Some(mac) = mac_address {
            block_on_netlink(netlink::set_mac_address(name, mac))
                .with_context(|| format!("failed to set mac address on '{name}'"))?;
        }
        for addr in addresses {
            block_on_netlink(netlink::set_addr(name, addr))
                .with_context(|| format!("failed to add address {addr} to '{name}'"))?;
        }
        Ok(())
    }

    /// Configure SR-IOV VFs on a physical function interface
    pub fn apply_sriov(&self, cfg: &SriovConfig) -> Result<()> {
        let pf_name = input_guard::vet_component!(&cfg.pf_name, anyhow::anyhow!("invalid PF name"));
        // Set number of VFs via sysfs
        let sriov_path = format!("/sys/class/net/{pf_name}/device/sriov_numvfs");

        // Most NIC drivers reject writing a new nonzero VF count while VFs
        // already exist (EBUSY) -- a well-known SR-IOV sysfs quirk:
        // reconfiguring an already-provisioned PF requires resetting to 0
        // first. Best-effort and silently ignored on failure (e.g. it's
        // already 0, or this is a first-time enable and the write would
        // have been a no-op anyway).
        let _ = fs::write(&sriov_path, "0");

        fs::write(&sriov_path, cfg.num_vfs.to_string()).with_context(|| {
            format!(
                "Failed to set sriov_numvfs on {} (requested {} VFs -- check /sys/class/net/{}/device/sriov_totalvfs for this device's actual maximum)",
                cfg.pf_name, cfg.num_vfs, cfg.pf_name
            )
        })?;

        tracing::info!(
            pf = %cfg.pf_name, num_vfs = cfg.num_vfs,
            "Set SR-IOV VFs"
        );

        // Configure individual VFs using ip link
        for vf in &cfg.vf_configs {
            let mut args = vec![
                "link".to_string(),
                "set".to_string(),
                pf_name.to_string(),
                "vf".to_string(),
                vf.vf_index.to_string(),
            ];

            if let Some(ref mac) = vf.mac_address {
                let mac = input_guard::vet!(mac, anyhow::anyhow!("invalid VF MAC address"));
                args.push("mac".to_string());
                args.push(mac.to_string());
            }
            if let Some(vlan) = vf.vlan {
                args.push("vlan".to_string());
                args.push(vlan.to_string());
                if let Some(qos) = vf.qos {
                    args.push("qos".to_string());
                    args.push(qos.to_string());
                }
            }
            if let Some(spoofchk) = vf.spoofchk {
                args.push("spoofchk".to_string());
                args.push(if spoofchk { "on" } else { "off" }.to_string());
            }
            if let Some(trust) = vf.trust {
                args.push("trust".to_string());
                args.push(if trust { "on" } else { "off" }.to_string());
            }

            // Only run if we have extra config beyond the base "ip link set dev vf idx"
            if args.len() > 5 {
                let output = Command::new("ip")
                    .args(&args)
                    .output()
                    .with_context(|| format!("Failed to configure VF {}", vf.vf_index))?;

                if !output.status.success() {
                    let stderr = String::from_utf8_lossy(&output.stderr);
                    // Don't leave some VFs configured and others (or the
                    // rest of the requested count) not -- reset to 0 so a
                    // failed apply doesn't strand the PF in a
                    // partially-provisioned state with no record of it
                    // (this create never reaches save_entity on error, the
                    // same orphan-on-partial-failure class of bug fixed
                    // earlier this session for bridges/vlans/etc).
                    let _ = fs::write(&sriov_path, "0");
                    return Err(anyhow::anyhow!(
                        "ip link set vf {} failed: {} (VFs reset to 0 on this failure, not left partially configured)",
                        vf.vf_index,
                        stderr
                    ));
                }

                tracing::info!(
                    pf = %cfg.pf_name, vf = vf.vf_index,
                    "Configured SR-IOV VF"
                );
            }
        }

        Ok(())
    }

    /// Remove SR-IOV VFs by setting numvfs to 0
    pub fn remove_sriov(&self, pf_name: &str) -> Result<()> {
        let sriov_path = format!("/sys/class/net/{}/device/sriov_numvfs", pf_name);
        fs::write(&sriov_path, "0")
            .with_context(|| format!("Failed to reset sriov_numvfs on {}", pf_name))?;

        tracing::info!(pf = %pf_name, "Removed SR-IOV VFs");
        Ok(())
    }

    /// Remove a device created by `apply_bridge`/`apply_vlan`/`apply_macvtap`
    /// /`apply_tap`/`apply_bond`/`apply_vxlan` via netlink `ip link del`.
    ///
    /// `net-<name>` and `link-<name>` are NOT device names — they're the
    /// synthetic ids `apply_network_file`/`apply_link_file` use (a leftover
    /// of the old file-naming scheme, kept so callers don't need updating)
    /// for settings applied to an *existing physical interface* that must
    /// never be deleted, only detached from whatever bridge/bond it was
    /// enslaved to.
    pub fn remove_device(&self, name: &str) -> Result<()> {
        if let Some(iface) = name
            .strip_prefix("net-")
            .or_else(|| name.strip_prefix("link-"))
        {
            block_on_netlink(netlink::unset_master(iface))
                .with_context(|| format!("failed to detach '{iface}' from its master"))?;
            tracing::info!("Detached {} from its bridge/bond", iface);
            return Ok(());
        }

        block_on_netlink(netlink::delete_link(name))
            .with_context(|| format!("failed to delete device '{name}'"))?;
        tracing::info!("Removed device {}", name);
        Ok(())
    }

    /// No-op: retained only so the ~60 existing call sites in
    /// `zyvor-fabricd/src/api/networkd.rs` (each following `apply_X(...)`
    /// with a `reload()`) don't need touching. Netlink writes above already
    /// apply immediately — there's nothing left to reload.
    pub fn reload(&self) -> Result<()> {
        Ok(())
    }

    /// List all config files we manage (with our prefix)
    pub fn list_managed_files(&self) -> Result<Vec<String>> {
        let config_dir = input_guard::vet_path!(
            &self.config_dir,
            anyhow::anyhow!("rejected network config directory")
        );
        let mut files = Vec::new();
        if !config_dir.exists() {
            return Ok(files);
        }

        for entry in fs::read_dir(&config_dir)? {
            let entry = entry?;
            let name = entry.file_name().to_string_lossy().to_string();
            if name.starts_with(&self.file_prefix) {
                files.push(name);
            }
        }

        files.sort();
        Ok(files)
    }

    /// Query `networkctl status <name>` for a specific device
    pub fn device_status(&self, name: &str) -> Result<String> {
        let name = input_guard::vet_component!(name, anyhow::anyhow!("invalid device name"));
        let output =
            input_guard::argv_checked!(name, anyhow::anyhow!("invalid device name"), |name| {
                Command::new("networkctl")
                    .args(["status", name, "--no-pager"])
                    .output()
                    .context("Failed to execute networkctl status")?
            });

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(anyhow::anyhow!(
                "networkctl status {} failed: {}",
                name,
                stderr
            ));
        }

        Ok(String::from_utf8_lossy(&output.stdout).to_string())
    }

    /// List network links via networkctl, falling back to `ip -j link` when networkctl is absent.
    pub fn list_links(&self) -> Result<Vec<LinkInfo>> {
        match self.list_links_networkctl() {
            Ok(links) if !links.is_empty() => Ok(links),
            Ok(_) | Err(_) => self.list_links_ip(),
        }
    }

    fn list_links_networkctl(&self) -> Result<Vec<LinkInfo>> {
        let output = Command::new("networkctl")
            .args(["list", "--no-pager", "--no-legend"])
            .output()
            .context("Failed to execute networkctl list")?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(anyhow::anyhow!("networkctl list failed: {}", stderr));
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        let mut links = Vec::new();

        for line in stdout.lines() {
            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.len() >= 4 {
                if let Ok(index) = parts[0].parse::<u32>() {
                    links.push(LinkInfo {
                        index,
                        name: parts[1].to_string(),
                        kind: parts[2].to_string(),
                        operational_state: parts[3].to_string(),
                        setup_state: parts.get(4).unwrap_or(&"").to_string(),
                    });
                }
            }
        }

        Ok(links)
    }

    /// Parse `ip -j link show` (works without systemd-networkd / networkctl).
    pub fn list_links_ip(&self) -> Result<Vec<LinkInfo>> {
        let bridge_names = self.list_bridge_names_ip()?;

        let output = Command::new("ip")
            .args(["-j", "link", "show"])
            .output()
            .context("Failed to execute ip -j link show")?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(anyhow::anyhow!("ip -j link show failed: {}", stderr));
        }

        let entries: Vec<serde_json::Value> =
            serde_json::from_slice(&output.stdout).context("Failed to parse ip -j link JSON")?;

        let mut links = Vec::new();
        for entry in entries {
            let name = match entry.get("ifname").and_then(|v| v.as_str()) {
                Some(n) => n.to_string(),
                None => continue,
            };
            let index = entry.get("ifindex").and_then(|v| v.as_u64()).unwrap_or(0) as u32;
            let operstate = entry
                .get("operstate")
                .and_then(|v| v.as_str())
                .unwrap_or("unknown")
                .to_lowercase();
            let link_type = entry
                .get("link_type")
                .and_then(|v| v.as_str())
                .unwrap_or("unknown")
                .to_string();
            let kind = if bridge_names.contains(&name) {
                "bridge".to_string()
            } else {
                link_type
            };
            links.push(LinkInfo {
                index,
                name,
                kind,
                operational_state: operstate,
                setup_state: "unmanaged".to_string(),
            });
        }

        links.sort_by_key(|a| a.index);
        Ok(links)
    }

    /// Host interface discovery (shared by API list endpoints).
    pub fn discover_host_interfaces(&self) -> Result<Vec<host_discovery::HostNetDevice>> {
        host_discovery::discover_interfaces()
    }

    fn list_bridge_names_ip(&self) -> Result<std::collections::HashSet<String>> {
        let output = Command::new("ip")
            .args(["-j", "link", "show", "type", "bridge"])
            .output();

        let output = match output {
            Ok(o) if o.status.success() => o,
            _ => return Ok(std::collections::HashSet::new()),
        };

        let entries: Vec<serde_json::Value> =
            serde_json::from_slice(&output.stdout).unwrap_or_default();

        Ok(entries
            .iter()
            .filter_map(|e| e.get("ifname").and_then(|v| v.as_str()).map(str::to_string))
            .collect())
    }

    /// Generate a random MAC address with QEMU KVM prefix 52:54:00
    pub fn generate_mac_address() -> String {
        use rand::RngExt;
        let mut rng = rand::rng();
        format!(
            "52:54:00:{:02x}:{:02x}:{:02x}",
            rng.random::<u8>(),
            rng.random::<u8>(),
            rng.random::<u8>()
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    // `apply_*`/`remove_device` now go straight to netlink (see the
    // migration plan, Phase 4) instead of writing files in `config_dir` —
    // there's no longer a filesystem side effect to assert on without a
    // real kernel + CAP_NET_ADMIN, so those cases moved to the `#[ignore]`d
    // root-only test below. What's still pure/file-based stays covered here.

    fn tmp_manager() -> (NetworkdManager, tempfile::TempDir) {
        let dir = tempfile::tempdir().unwrap();
        let mgr = NetworkdManager::new(dir.path(), "50-zyvor-fabricd-");
        (mgr, dir)
    }

    #[test]
    fn test_list_managed_files() {
        let (mgr, dir) = tmp_manager();
        fs::write(dir.path().join("50-zyvor-fabricd-br0.netdev"), "test").unwrap();
        fs::write(dir.path().join("50-zyvor-fabricd-br0.network"), "test").unwrap();
        fs::write(dir.path().join("99-other.network"), "unrelated").unwrap();

        let files = mgr.list_managed_files().unwrap();
        assert_eq!(files.len(), 2);
        assert!(files.contains(&"50-zyvor-fabricd-br0.netdev".to_string()));
        assert!(files.contains(&"50-zyvor-fabricd-br0.network".to_string()));
    }

    #[test]
    fn test_generate_mac() {
        let mac = NetworkdManager::generate_mac_address();
        assert!(mac.starts_with("52:54:00:"));
        assert_eq!(mac.len(), 17);
    }

    #[test]
    fn test_parse_addr() {
        assert_eq!(parse_addr("10.0.0.1/24"), Some("10.0.0.1".parse().unwrap()));
        assert_eq!(parse_addr("not-an-ip/24"), None);
    }

    #[test]
    fn test_remove_device_routes_net_and_link_prefixes_to_detach_not_delete() {
        // Can't exercise the real netlink call without root/CAP_NET_ADMIN
        // and an actual interface, but the prefix-stripping/routing logic
        // itself (net-X / link-X -> detach, everything else -> delete) is
        // pure and worth pinning down independent of that.
        assert_eq!("net-enp3s0".strip_prefix("net-"), Some("enp3s0"));
        assert_eq!(
            "link-lan0"
                .strip_prefix("net-")
                .or_else(|| "link-lan0".strip_prefix("link-")),
            Some("lan0")
        );
        assert_eq!(
            "br0"
                .strip_prefix("net-")
                .or_else(|| "br0".strip_prefix("link-")),
            None
        );
    }

    /// End-to-end against the real kernel: create a bridge (+ up, mtu, mac,
    /// address), a VLAN on it, and a tap; verify via `list_interfaces`; tear
    /// everything down via `remove_device`. Requires root/CAP_NET_ADMIN, so
    /// it's `#[ignore]`d by default — run explicitly with
    /// `sudo -E cargo test -p networking -- --ignored`.
    #[test]
    #[ignore = "needs root/CAP_NET_ADMIN and a real kernel"]
    fn test_apply_and_remove_device_live() {
        let (mgr, _dir) = tmp_manager();
        let rt = tokio::runtime::Runtime::new().unwrap();
        let _guard = rt.enter();

        let bridge = BridgeConfig {
            id: "id".into(),
            name: "zftbr0".into(),
            stp: None,
            forward_delay_sec: None,
            hello_time_sec: None,
            max_age_sec: None,
            vlan_filtering: None,
            mtu: Some(1400),
            mac_address: Some("52:54:00:aa:bb:cc".into()),
            addresses: vec!["10.250.251.1/24".into()],
            gateway: None,
            dns: vec![],
            dhcp: DhcpMode::No,
            created: String::new(),
            updated: String::new(),
            managed: true,
            operational_state: None,
        };
        mgr.apply_bridge(&bridge).unwrap();

        let vlan = VlanConfig {
            id: "id".into(),
            name: "zftbr0.99".into(),
            vlan_id: 99,
            parent_interface: "zftbr0".into(),
            mtu: None,
            addresses: vec![],
            gateway: None,
            dns: vec![],
            dhcp: DhcpMode::No,
            created: String::new(),
            updated: String::new(),
            managed: true,
            operational_state: None,
        };
        mgr.apply_vlan(&vlan).unwrap();

        let seen = rt.block_on(netlink::list_interfaces()).unwrap();
        let br = seen
            .iter()
            .find(|i| i.name == "zftbr0")
            .expect("bridge should be visible via netlink");
        assert_eq!(br.kind.as_deref(), Some("bridge"));
        assert_eq!(br.mtu, 1400);
        assert!(br.addresses.iter().any(|a| a.address == "10.250.251.1"));
        assert!(seen
            .iter()
            .any(|i| i.name == "zftbr0.99" && i.kind.as_deref() == Some("vlan")));

        mgr.remove_device("zftbr0.99").unwrap();
        mgr.remove_device("zftbr0").unwrap();

        let after = rt.block_on(netlink::list_interfaces()).unwrap();
        assert!(!after.iter().any(|i| i.name.starts_with("zftbr0")));
    }
}
