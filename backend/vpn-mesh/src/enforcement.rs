// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: LicenseRef-Zyvor-Production-2.0

use std::path::Path;

use anyhow::{Context, Result};
use tracing;

use crate::models::{CompiledWgInterface, CompiledWgPeer};

/// Manages WireGuard interfaces directly via netlink (device create/
/// address/up/down) and the `wg` CLI (key/peer/allowed-ips config — those
/// live in WireGuard's own generic-netlink family, a different subsystem
/// from the route-netlink calls `networking::netlink` otherwise makes, so
/// `wg set` is the pragmatic tool for that part, same as `host_wireguard.rs`
/// already uses for discovery). Replaces the old systemd-networkd
/// .netdev/.network-file + `networkctl reload` write path — see the
/// systemd-removal migration plan, Phase 4.
pub struct WireguardEnforcer;

/// Run an async netlink call from this crate's synchronous public API — see
/// `networking::NetworkdManager`'s identical helper for why `block_in_place`
/// is safe here (every caller already runs on tokio's multi-threaded
/// runtime via an axum handler).
fn block_on_netlink<F, T>(fut: F) -> Result<T>
where
    F: std::future::Future<Output = Result<T>>,
{
    tokio::task::block_in_place(|| tokio::runtime::Handle::current().block_on(fut))
}

impl WireguardEnforcer {
    pub fn new() -> Self {
        Self
    }

    /// Kept for API/test compatibility with the old file-based enforcer —
    /// there's no config directory to point at anymore, netlink calls are
    /// device-name-addressed.
    pub fn with_config_dir(_config_dir: &Path) -> Self {
        Self
    }

    /// Full sync: create/update every interface in `interfaces`, tear down
    /// Fabric-managed interfaces that are no longer active.
    ///
    /// `managed_names` is the set of interface names Fabric owns (enabled +
    /// disabled tunnels / network-compiled ifaces). Only those are eligible
    /// for stale removal — unmanaged host WireGuard (e.g. Cilium `cilium_wg0`)
    /// is left alone. When `managed_names` is empty, falls back to treating
    /// `interfaces` as the managed set (live unit-test compatibility).
    pub fn sync_all(
        &self,
        interfaces: &[CompiledWgInterface],
        managed_names: &[String],
    ) -> Result<()> {
        let active_names: Vec<&str> = interfaces
            .iter()
            .map(|i| i.interface_name.as_str())
            .collect();
        let managed: Vec<&str> = if managed_names.is_empty() {
            active_names.clone()
        } else {
            managed_names.iter().map(String::as_str).collect()
        };
        self.remove_stale(&active_names, &managed)?;

        for iface in interfaces {
            self.apply_interface(iface)?;
        }

        tracing::info!(
            "Synced {} WireGuard interfaces via netlink",
            interfaces.len()
        );
        Ok(())
    }

    /// Create (if missing) and configure one WireGuard interface: device,
    /// listen-port/private-key/peers via `wg set`, address, then bring up.
    fn apply_interface(&self, iface: &CompiledWgInterface) -> Result<()> {
        let name = &iface.interface_name;

        let exists = block_on_netlink(async {
            let handle = networking::netlink::connect().await?;
            Ok::<bool, anyhow::Error>(
                networking::netlink::link_index_by_name(&handle, name)
                    .await
                    .is_ok(),
            )
        })?;
        if !exists {
            block_on_netlink(networking::netlink::create_wireguard_device(name))
                .with_context(|| format!("failed to create WireGuard device '{name}'"))?;
        }

        self.wg_set(iface)?;

        // Idempotent: re-sync after the first apply. Kernel/rtnetlink phrasing
        // varies ("File exists", "Address already assigned", EEXIST / os error 17).
        if let Err(e) = block_on_netlink(networking::netlink::set_addr(name, &iface.address)) {
            if !is_addr_already_present(&e) {
                return Err(e).with_context(|| format!("failed to set address on '{name}'"));
            }
        }

        block_on_netlink(networking::netlink::set_link_up(name))
            .with_context(|| format!("failed to bring up '{name}'"))?;

        Ok(())
    }

    /// Push listen-port/private-key/peers onto `iface` via `wg set`. The
    /// private key is written to a 0600 temp file for `wg set ... private-key
    /// <path>` — `wg` intentionally refuses to take key material as a plain
    /// CLI argument (it would be visible in the process list).
    fn wg_set(&self, iface: &CompiledWgInterface) -> Result<()> {
        let key_file = tempfile::NamedTempFile::new()
            .context("failed to create temp file for WireGuard private key")?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(key_file.path(), std::fs::Permissions::from_mode(0o600))
                .context("failed to restrict private key temp file permissions")?;
        }
        std::fs::write(key_file.path(), &iface.private_key_ref)
            .context("failed to write WireGuard private key to temp file")?;

        let mut args = vec![
            "set".to_string(),
            iface.interface_name.clone(),
            "listen-port".to_string(),
            iface.listen_port.to_string(),
            "private-key".to_string(),
            key_file.path().display().to_string(),
        ];
        for peer in &iface.peers {
            args.extend(peer_args(peer));
        }

        let output = std::process::Command::new("wg")
            .args(&args)
            .output()
            .context("failed to execute `wg set`")?;
        if !output.status.success() {
            anyhow::bail!(
                "wg set {} failed: {}",
                iface.interface_name,
                String::from_utf8_lossy(&output.stderr)
            );
        }
        Ok(())
    }

    /// Delete a managed WireGuard interface.
    pub fn remove_interface(&self, name: &str) -> Result<()> {
        block_on_netlink(networking::netlink::delete_link(name))
            .with_context(|| format!("failed to delete WireGuard device '{name}'"))?;
        tracing::info!("Removed WireGuard interface {}", name);
        Ok(())
    }

    /// Tear down every interface in `managed`.
    pub fn cleanup(&self, managed: &[String]) -> Result<()> {
        for name in managed {
            let _ = self.remove_interface(name);
        }
        tracing::info!("Cleaned up {} WireGuard interfaces", managed.len());
        Ok(())
    }

    /// No-op: netlink/`wg set` calls above already apply immediately.
    /// Kept only so nothing calling `.reload()` needs updating.
    pub fn reload(&self) -> Result<()> {
        Ok(())
    }

    fn remove_stale(&self, active_names: &[&str], managed_names: &[&str]) -> Result<()> {
        let on_host = block_on_netlink(async {
            let ifaces = networking::netlink::list_interfaces().await?;
            Ok::<Vec<String>, anyhow::Error>(
                ifaces
                    .into_iter()
                    .filter(|i| i.kind.as_deref() == Some("wireguard"))
                    .map(|i| i.name)
                    .collect(),
            )
        })?;
        for name in on_host {
            if !managed_names.contains(&name.as_str()) {
                // Unmanaged host overlay (Cilium, operator-owned wg*) — keep.
                continue;
            }
            if !active_names.contains(&name.as_str()) {
                if let Err(e) = self.remove_interface(&name) {
                    tracing::warn!("Failed to remove stale WireGuard interface {}: {}", name, e);
                }
            }
        }
        Ok(())
    }
}

/// True when adding an address failed because it is already present.
fn is_addr_already_present(err: &anyhow::Error) -> bool {
    for cause in err.chain() {
        let msg = cause.to_string().to_ascii_lowercase();
        if msg.contains("file exists")
            || msg.contains("address already assigned")
            || msg.contains("already assigned")
            || msg.contains("already exists")
            || msg.contains("eexist")
            || msg.contains("os error 17")
        {
            return true;
        }
    }
    false
}

impl Default for WireguardEnforcer {
    fn default() -> Self {
        Self::new()
    }
}

fn peer_args(peer: &CompiledWgPeer) -> Vec<String> {
    let mut args = vec!["peer".to_string(), peer.public_key.clone()];
    if let Some(endpoint) = &peer.endpoint {
        args.push("endpoint".to_string());
        args.push(endpoint.clone());
    }
    if !peer.allowed_ips.is_empty() {
        args.push("allowed-ips".to_string());
        args.push(peer.allowed_ips.join(","));
    }
    if peer.persistent_keepalive > 0 {
        args.push("persistent-keepalive".to_string());
        args.push(peer.persistent_keepalive.to_string());
    }
    args
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_interface(name: &str, peers: Vec<CompiledWgPeer>) -> CompiledWgInterface {
        CompiledWgInterface {
            interface_name: name.to_string(),
            listen_port: 51820,
            address: "10.0.0.1/24".to_string(),
            private_key_ref: "aWFtYXByaXZhdGVrZXk=".to_string(),
            peers,
        }
    }

    fn make_peer(
        key: &str,
        endpoint: Option<&str>,
        allowed_ips: &[&str],
        keepalive: u16,
    ) -> CompiledWgPeer {
        CompiledWgPeer {
            public_key: key.to_string(),
            endpoint: endpoint.map(|s| s.to_string()),
            allowed_ips: allowed_ips.iter().map(|s| s.to_string()).collect(),
            persistent_keepalive: keepalive,
        }
    }

    #[test]
    fn test_peer_args_basic() {
        let peer = make_peer("pubkey-1", Some("1.2.3.4:51820"), &["10.0.0.2/32"], 25);
        let args = peer_args(&peer);
        assert_eq!(
            args,
            vec![
                "peer",
                "pubkey-1",
                "endpoint",
                "1.2.3.4:51820",
                "allowed-ips",
                "10.0.0.2/32",
                "persistent-keepalive",
                "25"
            ]
        );
    }

    #[test]
    fn test_peer_args_no_endpoint_no_keepalive() {
        let peer = make_peer("pubkey-1", None, &["10.0.0.0/24"], 0);
        let args = peer_args(&peer);
        assert_eq!(args, vec!["peer", "pubkey-1", "allowed-ips", "10.0.0.0/24"]);
    }

    #[test]
    fn test_peer_args_multiple_allowed_ips_joined() {
        let peer = make_peer("pubkey-1", None, &["10.0.0.0/24", "10.0.1.0/24"], 0);
        let args = peer_args(&peer);
        assert_eq!(
            args,
            vec!["peer", "pubkey-1", "allowed-ips", "10.0.0.0/24,10.0.1.0/24"]
        );
    }

    #[test]
    fn test_cleanup_empty() {
        let enforcer = WireguardEnforcer::new();
        let result = enforcer.cleanup(&[]);
        assert!(result.is_ok());
    }

    #[test]
    fn test_reload_is_noop() {
        let enforcer = WireguardEnforcer::new();
        assert!(enforcer.reload().is_ok());
    }

    /// End-to-end against the real kernel + `wg` binary: create a WireGuard
    /// interface with one peer, verify via `wg show`, tear it down. Requires
    /// root/CAP_NET_ADMIN and the `wireguard-tools` package, so it's
    /// `#[ignore]`d by default — run explicitly with
    /// `sudo -E cargo test -p vpn-mesh -- --ignored`.
    #[test]
    #[ignore = "needs root/CAP_NET_ADMIN, the wg binary, and a real kernel"]
    fn test_sync_all_live() {
        let rt = tokio::runtime::Runtime::new().unwrap();
        let _guard = rt.enter();

        // `wg` validates key length/encoding, so the shared fixtures'
        // placeholder key (fine for the non-live tests above, which never
        // call `wg`) won't pass here — generate real ones.
        let real_private_key = String::from_utf8(
            std::process::Command::new("wg")
                .arg("genkey")
                .output()
                .unwrap()
                .stdout,
        )
        .unwrap()
        .trim()
        .to_string();
        let real_peer_pubkey = {
            let genkey = std::process::Command::new("wg")
                .arg("genkey")
                .output()
                .unwrap();
            let mut pubkey_cmd = std::process::Command::new("wg")
                .arg("pubkey")
                .stdin(std::process::Stdio::piped())
                .stdout(std::process::Stdio::piped())
                .spawn()
                .unwrap();
            use std::io::Write;
            pubkey_cmd
                .stdin
                .take()
                .unwrap()
                .write_all(&genkey.stdout)
                .unwrap();
            let out = pubkey_cmd.wait_with_output().unwrap();
            String::from_utf8(out.stdout).unwrap().trim().to_string()
        };

        let enforcer = WireguardEnforcer::new();
        let peer = make_peer(&real_peer_pubkey, None, &["10.250.252.2/32"], 25);
        let mut iface = make_interface("zftwg0", vec![peer]);
        iface.private_key_ref = real_private_key;

        enforcer
            .sync_all(&[iface.clone()], &[iface.interface_name.clone()])
            .unwrap();

        let output = std::process::Command::new("wg")
            .args(["show", "zftwg0"])
            .output()
            .unwrap();
        let shown = String::from_utf8_lossy(&output.stdout);
        assert!(
            shown.contains("listening port: 51820"),
            "unexpected `wg show` output: {shown}"
        );
        assert!(shown.contains(&real_peer_pubkey));

        // Second sync must be idempotent (address already assigned).
        enforcer
            .sync_all(&[iface.clone()], &[iface.interface_name.clone()])
            .unwrap();

        assert!(is_addr_already_present(&anyhow::anyhow!(
            "failed to add address 10.250.252.1/24 to 'zftwg0': Address already assigned."
        )));

        enforcer.remove_interface("zftwg0").unwrap();
        let seen = rt.block_on(networking::netlink::list_interfaces()).unwrap();
        assert!(!seen.iter().any(|i| i.name == "zftwg0"));
    }

    #[test]
    fn test_is_addr_already_present_variants() {
        assert!(is_addr_already_present(&anyhow::anyhow!("File exists")));
        assert!(is_addr_already_present(&anyhow::anyhow!(
            "ipv4: Address already assigned."
        )));
        assert!(is_addr_already_present(&anyhow::anyhow!("EEXIST")));
        assert!(is_addr_already_present(&anyhow::anyhow!("os error 17")));
        // Outer context must not hide nested netlink EEXIST (anyhow::to_string
        // is only the top layer).
        let nested = anyhow::anyhow!("Received a netlink error message File exists (os error 17)")
            .context("failed to add address 10.0.0.1/24 to 'zftwg0'");
        assert!(is_addr_already_present(&nested));
        assert!(!is_addr_already_present(&anyhow::anyhow!(
            "permission denied"
        )));
    }
}
