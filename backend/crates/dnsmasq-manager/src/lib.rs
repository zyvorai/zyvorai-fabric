// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: LicenseRef-Zyvor-Production-2.0

//! Per-bridge DHCP server, replacing systemd-networkd's built-in
//! `[DHCPServer]` `.network`-file directive + `networkctl reload`. Spawns
//! and supervises one `dnsmasq` process per bridge — the same "own the
//! child process, no systemd unit" pattern `tpm-support` already uses for
//! `swtpm`. See the systemd-removal migration plan, Phase 4.
//!
//! `dnsmasq` is started with its own DNS listener disabled (`port=0`) so
//! its behavior matches what systemd-networkd's `[DHCPServer]` actually
//! provided: DHCP leases plus a DNS-server *option* pushed to clients
//! (upstream DNS advertised via DHCP, not a resolver running on the
//! bridge) — not a full DNS proxy.

use std::net::Ipv4Addr;
use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};

/// What one bridge's DHCP server should hand out. Mirrors
/// `zyvor-fabricd::api::network_cloud::DhcpServerConfig` but lives here so
/// this crate doesn't depend back on the daemon crate.
#[derive(Debug, Clone)]
pub struct DhcpConfig {
    pub bridge: String,
    /// Bridge's own address, e.g. "10.0.0.1" — the pool and netmask are
    /// derived from this assuming a /24, matching the old
    /// `generate_dhcp_network_file`'s `Address={gw}/24` assumption.
    pub gateway: Ipv4Addr,
    pub pool_offset: u32,
    pub pool_size: u32,
    pub default_lease_time_sec: u32,
    pub dns_servers: Vec<String>,
    pub domain: Option<String>,
    /// Directory of hosts-format zone files (see `dns_policy::enforcement`)
    /// to also serve as DNS on this same instance, in addition to DHCP.
    /// `None` keeps the old DHCP-only, `port=0` behavior. dnsmasq watches
    /// the directory via inotify, so zone records created/updated after
    /// this instance starts are picked up live, no restart needed.
    /// `bind-interfaces` + `interface={bridge}` (below) scope the DNS
    /// listener to just that bridge's address, so this can never collide
    /// with the host's own resolver (e.g. systemd-resolved on loopback).
    pub zone_hosts_dir: Option<PathBuf>,
}

pub struct DnsmasqManager {
    run_dir: PathBuf,
}

impl DnsmasqManager {
    pub fn new(run_dir: impl Into<PathBuf>) -> Self {
        Self {
            run_dir: run_dir.into(),
        }
    }

    /// Start (or restart, if one is already running for this bridge) the
    /// DHCP server. `dnsmasq` daemonizes itself by default — the spawned
    /// process exits once it has forked into the background and written
    /// its pidfile, so there's nothing to hold a live handle to; `stop`
    /// finds it again via that pidfile.
    pub async fn start(&self, cfg: &DhcpConfig) -> Result<()> {
        let run_dir =
            input_guard::vet_path!(&self.run_dir, anyhow::anyhow!("invalid DHCP run directory"));
        tokio::fs::create_dir_all(&run_dir)
            .await
            .with_context(|| format!("failed to create {}", run_dir.display()))?;
        // dnsmasq requires --hostsdir's target to already exist at startup
        // (unlike the files inside it, which it's fine to watch appear
        // later) -- ensure it's there even if no zone has been created yet.
        if let Some(dir) = &cfg.zone_hosts_dir {
            tokio::fs::create_dir_all(dir)
                .await
                .with_context(|| format!("failed to create {}", dir.display()))?;
        }

        // Idempotent: tear down any prior instance for this bridge first.
        let _ = self.stop(&cfg.bridge).await;

        let conf_path = self.conf_path(&cfg.bridge)?;
        let pid_path = self.pid_path(&cfg.bridge)?;
        let lease_path = self.lease_path(&cfg.bridge)?;
        let conf = render_config(cfg, &pid_path, &lease_path)?;
        tokio::fs::write(&conf_path, conf)
            .await
            .with_context(|| format!("failed to write {}", conf_path.display()))?;

        let output = tokio::process::Command::new("dnsmasq")
            // dnsmasq drops privileges to `nobody` by default after
            // binding its sockets. zyvor-fabricd's own systemd unit sets
            // RestrictSUIDSGID=yes (it already runs as root and doesn't
            // need setuid/setgid itself), which blocks that same syscall
            // family for every child process too — dnsmasq's privilege
            // drop then fails and it exits immediately (observed exit
            // status 3/5 depending on version). Telling it to stay root
            // (zyvor-fabricd already runs as root, so this isn't a
            // privilege escalation) avoids the drop entirely.
            .arg("--user=root")
            .arg("--group=root")
            .arg(format!("--conf-file={}", conf_path.display()))
            .output()
            .await
            .context("failed to run dnsmasq")?;
        if !output.status.success() {
            bail!(
                "dnsmasq exited with {} for bridge '{}' (conf: {}): {}",
                output.status,
                cfg.bridge,
                conf_path.display(),
                String::from_utf8_lossy(&output.stderr).trim(),
            );
        }

        tracing::info!(bridge = %cfg.bridge, "Started DHCP server");
        Ok(())
    }

    /// Stop the DHCP server for `bridge`, if one is running.
    pub async fn stop(&self, bridge: &str) -> Result<()> {
        let pid_path = self.pid_path(bridge)?;
        if let Ok(pid_str) = tokio::fs::read_to_string(&pid_path).await {
            if let Ok(pid) = pid_str.trim().parse::<u32>() {
                let _ = tokio::process::Command::new("kill")
                    .arg(pid.to_string())
                    .status()
                    .await;
            }
            let _ = tokio::fs::remove_file(&pid_path).await;
        }
        let _ = tokio::fs::remove_file(self.conf_path(bridge)?).await;
        let _ = tokio::fs::remove_file(self.lease_path(bridge)?).await;
        tracing::info!(bridge = %bridge, "Stopped DHCP server");
        Ok(())
    }

    fn conf_path(&self, bridge: &str) -> Result<PathBuf> {
        let bridge = input_guard::vet_component!(bridge, anyhow::anyhow!("invalid bridge name"));
        Ok(self.runtime_dir()?.join(format!("{bridge}.conf")))
    }

    fn pid_path(&self, bridge: &str) -> Result<PathBuf> {
        let bridge = input_guard::vet_component!(bridge, anyhow::anyhow!("invalid bridge name"));
        Ok(self.runtime_dir()?.join(format!("{bridge}.pid")))
    }

    fn lease_path(&self, bridge: &str) -> Result<PathBuf> {
        let bridge = input_guard::vet_component!(bridge, anyhow::anyhow!("invalid bridge name"));
        Ok(self.runtime_dir()?.join(format!("{bridge}.leases")))
    }

    fn runtime_dir(&self) -> Result<PathBuf> {
        Ok(input_guard::vet_path!(
            &self.run_dir,
            anyhow::anyhow!("invalid DHCP run directory")
        ))
    }

    /// Look up the current DHCP-leased IP for a MAC address, across every
    /// bridge this manager has a lease file for — the caller doesn't
    /// necessarily know which bridge a given VM's tap landed on. Each
    /// dnsmasq lease-file line is `<expiry-epoch> <mac> <ip> <hostname>
    /// <client-id>`; returns the first match, case-insensitively (dnsmasq
    /// writes MACs lowercase, but a caller-supplied MAC might not be).
    pub async fn lookup_lease_by_mac(&self, mac: &str) -> Result<Option<String>> {
        let mac = mac.to_ascii_lowercase();
        let mut entries = match tokio::fs::read_dir(&self.run_dir).await {
            Ok(entries) => entries,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(e) => return Err(e).with_context(|| format!("reading {}", self.run_dir.display())),
        };
        while let Some(entry) = entries.next_entry().await? {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) != Some("leases") {
                continue;
            }
            let Ok(content) = tokio::fs::read_to_string(&path).await else {
                continue;
            };
            for line in content.lines() {
                let mut fields = line.split_whitespace();
                let _expiry = fields.next();
                let Some(line_mac) = fields.next() else {
                    continue;
                };
                let Some(ip) = fields.next() else { continue };
                if line_mac.eq_ignore_ascii_case(&mac) {
                    return Ok(Some(ip.to_string()));
                }
            }
        }
        Ok(None)
    }
}

fn render_config(cfg: &DhcpConfig, pid_path: &Path, lease_path: &Path) -> Result<String> {
    if cfg.pool_offset == 0 {
        bail!("pool_offset must be >= 1 (0 would overlap the gateway's own address)");
    }
    if cfg.pool_size == 0 {
        bail!("pool_size must be >= 1");
    }
    let end = cfg.pool_offset.checked_add(cfg.pool_size - 1).filter(|&e| e <= 254).with_context(|| {
        format!("pool_offset ({}) + pool_size ({}) exceeds the /24's usable range (max host octet 254)", cfg.pool_offset, cfg.pool_size)
    })?;

    let octets = cfg.gateway.octets();
    let base = format!("{}.{}.{}", octets[0], octets[1], octets[2]);
    let start_ip = format!("{base}.{}", cfg.pool_offset);
    let end_ip = format!("{base}.{end}");

    let mut out = String::new();
    out.push_str(&format!("interface={}\n", cfg.bridge));
    out.push_str("bind-interfaces\n");
    out.push_str("except-interface=lo\n");
    match &cfg.zone_hosts_dir {
        // Zone records are served locally; anything else this dnsmasq
        // doesn't have a hosts-file answer for forwards upstream via the
        // host's own /etc/resolv.conf (dnsmasq's default when `no-resolv`
        // isn't set), so VMs on this bridge get one DNS server for both.
        Some(dir) => out.push_str(&format!("hostsdir={}\n", dir.display())),
        // No DNS service on the bridge — only DHCP, matching what
        // systemd-networkd's [DHCPServer] directive originally provided.
        None => out.push_str("port=0\n"),
    }
    out.push_str(&format!(
        "dhcp-range={start_ip},{end_ip},255.255.255.0,{}\n",
        cfg.default_lease_time_sec
    ));
    out.push_str(&format!("dhcp-option=option:router,{}\n", cfg.gateway));
    if !cfg.dns_servers.is_empty() {
        out.push_str(&format!(
            "dhcp-option=option:dns-server,{}\n",
            cfg.dns_servers.join(",")
        ));
    }
    if let Some(domain) = &cfg.domain {
        out.push_str(&format!("domain={domain}\n"));
        out.push_str(&format!("dhcp-option=option:domain-name,{domain}\n"));
    }
    out.push_str(&format!("pid-file={}\n", pid_path.display()));
    // dnsmasq's default lease file (/var/lib/misc/dnsmasq.leases) sits
    // outside every path zyvor-fabricd's systemd unit lists in
    // ReadWritePaths= under ProtectSystem=strict, so it must be pointed at
    // our own writable run directory instead.
    out.push_str(&format!("dhcp-leasefile={}\n", lease_path.display()));

    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cfg() -> DhcpConfig {
        DhcpConfig {
            bridge: "br0".into(),
            gateway: "10.0.0.1".parse().unwrap(),
            pool_offset: 100,
            pool_size: 50,
            default_lease_time_sec: 3600,
            dns_servers: vec!["8.8.8.8".into(), "1.1.1.1".into()],
            domain: Some("vms.local".into()),
            zone_hosts_dir: None,
        }
    }

    #[test]
    fn test_render_config_pool_range() {
        let out = render_config(
            &cfg(),
            Path::new("/run/x/br0.pid"),
            Path::new("/run/x/br0.leases"),
        )
        .unwrap();
        assert!(out.contains("dhcp-range=10.0.0.100,10.0.0.149,255.255.255.0,3600"));
        assert!(out.contains("interface=br0"));
        assert!(out.contains("port=0"));
        assert!(out.contains("dhcp-option=option:router,10.0.0.1"));
        assert!(out.contains("dhcp-option=option:dns-server,8.8.8.8,1.1.1.1"));
        assert!(out.contains("domain=vms.local"));
        assert!(out.contains("pid-file=/run/x/br0.pid"));
        assert!(out.contains("dhcp-leasefile=/run/x/br0.leases"));
    }

    #[test]
    fn test_render_config_no_dns_no_domain() {
        let mut c = cfg();
        c.dns_servers.clear();
        c.domain = None;
        let out = render_config(
            &c,
            Path::new("/run/x/br0.pid"),
            Path::new("/run/x/br0.leases"),
        )
        .unwrap();
        assert!(!out.contains("dns-server"));
        assert!(!out.contains("domain="));
    }

    #[test]
    fn test_render_config_with_zone_hosts_dir_serves_dns_instead_of_port_zero() {
        let mut c = cfg();
        c.zone_hosts_dir = Some(PathBuf::from("/etc/zyvor-fabricd/dns"));
        let out = render_config(
            &c,
            Path::new("/run/x/br0.pid"),
            Path::new("/run/x/br0.leases"),
        )
        .unwrap();
        assert!(out.contains("hostsdir=/etc/zyvor-fabricd/dns"));
        assert!(!out.contains("port=0"));
    }

    #[test]
    fn test_render_config_rejects_zero_offset() {
        let mut c = cfg();
        c.pool_offset = 0;
        assert!(render_config(
            &c,
            Path::new("/run/x/br0.pid"),
            Path::new("/run/x/br0.leases")
        )
        .is_err());
    }

    #[test]
    fn test_render_config_rejects_pool_overflowing_subnet() {
        let mut c = cfg();
        c.pool_offset = 200;
        c.pool_size = 100; // 200 + 99 = 299 > 254
        assert!(render_config(
            &c,
            Path::new("/run/x/br0.pid"),
            Path::new("/run/x/br0.leases")
        )
        .is_err());
    }

    #[tokio::test]
    async fn lookup_lease_by_mac_finds_across_bridges_case_insensitively() {
        let dir = tempfile::tempdir().unwrap();
        tokio::fs::write(
            dir.path().join("br0.leases"),
            "1234567890 aa:bb:cc:dd:ee:ff 10.0.0.42 my-vm 01:aa:bb:cc:dd:ee:ff\n",
        )
        .await
        .unwrap();
        tokio::fs::write(dir.path().join("br1.leases"), "")
            .await
            .unwrap();

        let mgr = DnsmasqManager::new(dir.path());
        assert_eq!(
            mgr.lookup_lease_by_mac("AA:BB:CC:DD:EE:FF").await.unwrap(),
            Some("10.0.0.42".to_string())
        );
        assert_eq!(
            mgr.lookup_lease_by_mac("00:00:00:00:00:00").await.unwrap(),
            None
        );
    }

    #[tokio::test]
    async fn lookup_lease_by_mac_missing_run_dir_returns_none() {
        let dir = tempfile::tempdir().unwrap();
        let mgr = DnsmasqManager::new(dir.path().join("does-not-exist"));
        assert_eq!(
            mgr.lookup_lease_by_mac("aa:bb:cc:dd:ee:ff").await.unwrap(),
            None
        );
    }
}
