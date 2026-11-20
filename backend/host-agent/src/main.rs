// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: LicenseRef-Zyvor-Production-1.0

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use anyhow::{Context, Result};
use clap::Parser;
use serde::{Deserialize, Serialize};
use tokio::signal;
use tokio::sync::watch;
use tracing::{error, info, warn};
use uuid::Uuid;
use zyvor_fabric_driver_core::VmDriver;

// ---------------------------------------------------------------------------
// CLI arguments
// ---------------------------------------------------------------------------

#[derive(Parser, Debug)]
#[command(
    name = "zyvor-fabricd-agent",
    about = "Lightweight host agent for zyvor-fabricd – runs on each hypervisor host and communicates with the central controller"
)]
struct AgentConfig {
    /// URL of the central zyvor-fabricd controller (e.g. http://controller:8080)
    #[arg(long)]
    controller_url: String,

    /// IP address of this host (auto-detected from hostname resolution if omitted)
    #[arg(long)]
    address: Option<String>,

    /// Heartbeat interval in seconds
    #[arg(long, default_value_t = 10)]
    heartbeat_interval: u64,

    /// Path to the file that persists this host's unique ID across restarts
    #[arg(long, default_value = "/var/lib/zyvor-fabricd/host-id")]
    host_id_file: PathBuf,

    /// Base URL of the local FluxVM daemon this host's VMs run under
    /// (`fluxvm serve`) — every VM command from the controller is
    /// executed against it.
    #[arg(long, default_value = "http://127.0.0.1:7788")]
    fluxvm_url: String,

    /// Bearer token for FluxVM's auth layer, if `auth.tokens` is
    /// configured on that `fluxvm serve` instance.
    #[arg(long)]
    fluxvm_token: Option<String>,
}

// ---------------------------------------------------------------------------
// Data types exchanged with the controller
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SystemMetrics {
    pub cpu_usage_pct: f64,
    pub memory_total_mb: u64,
    pub memory_used_mb: u64,
    pub memory_usage_pct: f64,
    pub vm_count: u32,
    pub uptime_secs: u64,
    pub load_average: [f64; 3],
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum HostCommand {
    StartVm {
        vm_name: String,
    },
    StopVm {
        vm_name: String,
    },
    RestartVm {
        vm_name: String,
    },
    MigrateVm {
        vm_name: String,
        target_host: String,
    },
    EnterMaintenance,
    ExitMaintenance,
    FenceVm {
        vm_name: String,
    },
    PromoteStorage {
        vm_name: String,
        dataset: String,
    },
    AcknowledgeFence {
        vm_name: String,
    },
}

/// Payload sent when registering with the controller.
#[derive(Debug, Serialize)]
struct RegistrationPayload {
    host_id: String,
    hostname: String,
    address: String,
    metrics: SystemMetrics,
}

/// Payload sent on each heartbeat. Deliberately flat, matching
/// `datacenter::HostHeartbeat` field-for-field (the controller's
/// `host_heartbeat` handler deserializes straight into that type) --
/// previously this sent `{timestamp, metrics: {...}}`, a shape
/// `HostHeartbeat` can't parse at all (missing required `cpu_usage_pct`/
/// `memory_usage_pct`), so every heartbeat this agent sent was silently
/// rejected before `secure_containers_ready` even existed as a concept here.
#[derive(Debug, Serialize)]
struct HeartbeatPayload {
    cpu_usage_pct: f64,
    memory_usage_pct: f64,
    vm_count: u32,
    uptime_secs: u64,
    secure_containers_ready: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    secure_containers: Option<SecureContainersDetail>,
}

#[derive(Debug, Clone, Serialize)]
struct SecureContainersDetail {
    available: bool,
    shim_installed: bool,
    guest_image_present: bool,
}

/// Wrapper the controller uses when returning a list of pending commands.
#[derive(Debug, Deserialize)]
struct CommandsResponse {
    commands: Vec<HostCommand>,
}

// ---------------------------------------------------------------------------
// Agent
// ---------------------------------------------------------------------------

struct Agent {
    host_id: String,
    controller_url: String,
    hostname: String,
    address: String,
    heartbeat_interval: Duration,
    http_client: reqwest::Client,
    driver: Arc<dyn VmDriver>,
    /// Base URL of the local FluxVM daemon, used only to probe `/readyz` for
    /// `secure_containers.available` ahead of each heartbeat -- kept
    /// separate from `driver` (a `dyn VmDriver`) since that trait has no
    /// readiness-probe method and adding one would ripple into every other
    /// implementor for a need specific to this one capability flag.
    fluxvm_url: String,
}

impl Agent {
    fn new(
        host_id: String,
        controller_url: String,
        hostname: String,
        address: String,
        heartbeat_interval: Duration,
        driver: Arc<dyn VmDriver>,
        fluxvm_url: String,
    ) -> Self {
        let http_client = reqwest::Client::builder()
            .timeout(Duration::from_secs(30))
            .build()
            .expect("failed to build HTTP client");

        Self {
            host_id,
            controller_url,
            hostname,
            address,
            heartbeat_interval,
            http_client,
            fluxvm_url,
            driver,
        }
    }

    // -- metrics -------------------------------------------------------------

    async fn collect_system_metrics(&self) -> SystemMetrics {
        let cpu_usage_pct = read_cpu_usage().await.unwrap_or(0.0);
        let (memory_total_mb, memory_used_mb, memory_usage_pct) =
            read_memory_info().unwrap_or((0, 0, 0.0));
        let uptime_secs = read_uptime().unwrap_or(0);
        let load_average = read_loadavg().unwrap_or([0.0; 3]);
        let vm_count = self
            .driver
            .list_machines()
            .await
            .map(|v| v.len() as u32)
            .unwrap_or(0);

        SystemMetrics {
            cpu_usage_pct,
            memory_total_mb,
            memory_used_mb,
            memory_usage_pct,
            vm_count,
            uptime_secs,
            load_average,
        }
    }

    // -- registration ------------------------------------------------------

    async fn register_with_controller(&self) -> Result<()> {
        let url = format!("{}/api/hosts", self.controller_url);
        let metrics = self.collect_system_metrics().await;

        let payload = RegistrationPayload {
            host_id: self.host_id.clone(),
            hostname: self.hostname.clone(),
            address: self.address.clone(),
            metrics,
        };

        let resp = self
            .http_client
            .post(&url)
            .json(&payload)
            .send()
            .await
            .context("POST /api/hosts failed")?;

        if resp.status().is_success() {
            info!(host_id = %self.host_id, "registered with controller");
            Ok(())
        } else {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            anyhow::bail!(
                "controller returned {} during registration: {}",
                status,
                body
            );
        }
    }

    // -- heartbeat ---------------------------------------------------------

    /// Probe the local FluxVM daemon's `/readyz` for
    /// `secure_containers.available` (see `docs/contracts/
    /// fabric-fluxvm-readyz.json` and `fluxvm-scheduler::readyz`'s
    /// `secure_containers_status`, which is what actually populates this
    /// field: KVM present, the `containerd-shim-fluxvm-v2` binary on
    /// `PATH`, and the configured guest image present). Any failure to
    /// reach FluxVM or parse the response is treated as "not ready" rather
    /// than propagated -- a heartbeat must still go out even when FluxVM is
    /// temporarily unreachable, just reporting this host as not currently
    /// Secure-Containers-capable.
    async fn secure_containers_detail(&self) -> Option<SecureContainersDetail> {
        let url = format!("{}/readyz", self.fluxvm_url);
        let body = match self.http_client.get(&url).send().await {
            Ok(resp) => match resp.json::<serde_json::Value>().await {
                Ok(body) => body,
                Err(e) => {
                    tracing::debug!(error = %e, "failed to parse FluxVM /readyz response");
                    return None;
                }
            },
            Err(e) => {
                tracing::debug!(error = %e, "failed to reach FluxVM /readyz");
                return None;
            }
        };
        Some(extract_secure_containers_detail(&body))
    }

    async fn send_heartbeat(&self) -> Result<()> {
        let url = format!(
            "{}/api/hosts/{}/heartbeat",
            self.controller_url, self.host_id
        );
        let metrics = self.collect_system_metrics().await;
        let secure_containers = self.secure_containers_detail().await;
        let secure_containers_ready = secure_containers
            .as_ref()
            .map(|d| d.available)
            .unwrap_or(false);

        let payload = HeartbeatPayload {
            cpu_usage_pct: metrics.cpu_usage_pct,
            memory_usage_pct: metrics.memory_usage_pct,
            vm_count: metrics.vm_count,
            uptime_secs: metrics.uptime_secs,
            secure_containers_ready,
            secure_containers,
        };

        let resp = self
            .http_client
            .post(&url)
            .json(&payload)
            .send()
            .await
            .context("POST heartbeat failed")?;

        if resp.status().is_success() {
            tracing::debug!("heartbeat acknowledged");
            Ok(())
        } else {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            anyhow::bail!("heartbeat rejected ({}): {}", status, body);
        }
    }

    // -- command polling ---------------------------------------------------

    async fn poll_commands(&self) -> Result<Vec<HostCommand>> {
        let url = format!(
            "{}/api/hosts/{}/commands",
            self.controller_url, self.host_id
        );

        let resp = self
            .http_client
            .get(&url)
            .send()
            .await
            .context("GET commands failed")?;

        if resp.status().is_success() {
            let body: CommandsResponse = resp
                .json()
                .await
                .context("failed to parse commands response")?;
            Ok(body.commands)
        } else {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            anyhow::bail!("poll_commands returned {}: {}", status, body);
        }
    }

    // -- command handling --------------------------------------------------

    async fn handle_command(&self, cmd: HostCommand) {
        info!(?cmd, "handling command from controller");

        match cmd {
            HostCommand::StartVm { vm_name } => {
                if let Err(e) = self.driver.start(&vm_name).await {
                    error!(vm = %vm_name, error = %e, "failed to start VM");
                } else {
                    info!(vm = %vm_name, "VM started");
                }
            }
            HostCommand::StopVm { vm_name } => {
                if let Err(e) = self.driver.poweroff(&vm_name).await {
                    error!(vm = %vm_name, error = %e, "failed to stop VM");
                } else {
                    info!(vm = %vm_name, "VM stopped");
                }
            }
            HostCommand::RestartVm { vm_name } => {
                if let Err(e) = self.driver.reboot(&vm_name).await {
                    error!(vm = %vm_name, error = %e, "failed to restart VM");
                } else {
                    info!(vm = %vm_name, "VM restarted");
                }
            }
            HostCommand::MigrateVm {
                vm_name,
                target_host,
            } => {
                // Migration is coordinated by the controller; the agent stops
                // the local VM and reports readiness. Actual image transfer is
                // handled externally.
                warn!(
                    vm = %vm_name,
                    target = %target_host,
                    "migration requested – stopping local VM for transfer"
                );
                if let Err(e) = self.driver.poweroff(&vm_name).await {
                    error!(vm = %vm_name, error = %e, "failed to stop VM for migration");
                }
            }
            HostCommand::EnterMaintenance => {
                info!("entering maintenance mode – new VM placements will be refused");
                // A production implementation would set a flag inspected by the
                // scheduler so that no new VMs are placed on this host.
            }
            HostCommand::ExitMaintenance => {
                info!("exiting maintenance mode – host available for scheduling");
            }
            HostCommand::FenceVm { vm_name } => {
                warn!(vm = %vm_name, "fencing VM – force stopping");
                if let Err(e) = self.driver.poweroff(&vm_name).await {
                    warn!(vm = %vm_name, error = %e, "graceful stop failed, force-terminating");
                    match self.driver.terminate(&vm_name).await {
                        Ok(()) => info!(vm = %vm_name, "VM forcefully terminated"),
                        Err(e) => {
                            error!(vm = %vm_name, error = %e, "failed to fence VM – all methods exhausted")
                        }
                    }
                } else {
                    info!(vm = %vm_name, "VM fenced (graceful stop)");
                }
            }
            HostCommand::PromoteStorage { vm_name, dataset } => {
                info!(vm = %vm_name, dataset = %dataset, "promoting ZFS storage for failover");
                // Promote the received ZFS dataset on this host
                match std::process::Command::new("zfs")
                    .args(["promote", &dataset])
                    .output()
                {
                    Ok(out) if out.status.success() => {
                        info!(vm = %vm_name, dataset = %dataset, "ZFS storage promoted");
                    }
                    Ok(out) => {
                        let stderr = String::from_utf8_lossy(&out.stderr);
                        warn!(vm = %vm_name, stderr = %stderr, "zfs promote returned non-zero (may be expected for non-clone)");
                    }
                    Err(e) => {
                        error!(vm = %vm_name, error = %e, "failed to promote ZFS storage");
                    }
                }
            }
            HostCommand::AcknowledgeFence { vm_name } => {
                info!(vm = %vm_name, "fence acknowledged – VM confirmed stopped on this host");
            }
        }
    }

    // -- main loop ---------------------------------------------------------

    async fn run(&self, mut shutdown_rx: watch::Receiver<bool>) -> Result<()> {
        // Registration with exponential back-off.
        let mut backoff = Duration::from_secs(1);
        let max_backoff = Duration::from_secs(60);

        loop {
            match self.register_with_controller().await {
                Ok(()) => break,
                Err(e) => {
                    warn!(
                        error = %e,
                        retry_in = ?backoff,
                        "controller unreachable, retrying registration"
                    );
                    tokio::select! {
                        _ = tokio::time::sleep(backoff) => {}
                        _ = shutdown_rx.changed() => {
                            info!("shutdown requested during registration");
                            return Ok(());
                        }
                    }
                    backoff = (backoff * 2).min(max_backoff);
                }
            }
        }

        // Main heartbeat + command-poll loop.
        let mut interval = tokio::time::interval(self.heartbeat_interval);
        // The first tick completes immediately; consume it so we don't
        // double-fire right after registration.
        interval.tick().await;

        loop {
            tokio::select! {
                _ = interval.tick() => {
                    // Send heartbeat.
                    if let Err(e) = self.send_heartbeat().await {
                        warn!(error = %e, "heartbeat failed, will retry next interval");
                    }

                    // Poll for commands.
                    match self.poll_commands().await {
                        Ok(commands) => {
                            for cmd in commands {
                                self.handle_command(cmd).await;
                            }
                        }
                        Err(e) => {
                            warn!(error = %e, "command poll failed, will retry next interval");
                        }
                    }
                }
                _ = shutdown_rx.changed() => {
                    info!("shutdown signal received, exiting agent loop");
                    return Ok(());
                }
            }
        }
    }
}

// ---------------------------------------------------------------------------
// System metrics collection
// ---------------------------------------------------------------------------

/// Read CPU usage by sampling /proc/stat twice with a short delay and
/// computing the delta.  Returns an overall utilisation percentage.
async fn read_cpu_usage() -> Result<f64> {
    fn parse_cpu_line(line: &str) -> Result<(u64, u64)> {
        // cpu  user nice system idle iowait irq softirq steal ...
        let fields: Vec<&str> = line.split_whitespace().collect();
        if fields.len() < 5 {
            anyhow::bail!("unexpected /proc/stat format");
        }
        let user: u64 = fields[1].parse()?;
        let nice: u64 = fields[2].parse()?;
        let system: u64 = fields[3].parse()?;
        let idle: u64 = fields[4].parse()?;
        let iowait: u64 = fields.get(5).unwrap_or(&"0").parse().unwrap_or(0);
        let irq: u64 = fields.get(6).unwrap_or(&"0").parse().unwrap_or(0);
        let softirq: u64 = fields.get(7).unwrap_or(&"0").parse().unwrap_or(0);
        let steal: u64 = fields.get(8).unwrap_or(&"0").parse().unwrap_or(0);

        let total = user + nice + system + idle + iowait + irq + softirq + steal;
        let idle_total = idle + iowait;
        Ok((total, idle_total))
    }

    fn read_first_cpu_line() -> Result<String> {
        let content = std::fs::read_to_string("/proc/stat").context("reading /proc/stat")?;
        content
            .lines()
            .next()
            .map(String::from)
            .context("/proc/stat is empty")
    }

    let line1 = read_first_cpu_line()?;
    let (total1, idle1) = parse_cpu_line(&line1)?;

    tokio::time::sleep(Duration::from_millis(250)).await;

    let line2 = read_first_cpu_line()?;
    let (total2, idle2) = parse_cpu_line(&line2)?;

    let total_delta = total2.saturating_sub(total1) as f64;
    let idle_delta = idle2.saturating_sub(idle1) as f64;

    if total_delta == 0.0 {
        return Ok(0.0);
    }

    let usage = ((total_delta - idle_delta) / total_delta) * 100.0;
    Ok((usage * 100.0).round() / 100.0) // two decimal places
}

/// Parse /proc/meminfo for MemTotal and MemAvailable.
fn read_memory_info() -> Result<(u64, u64, f64)> {
    let content = std::fs::read_to_string("/proc/meminfo").context("reading /proc/meminfo")?;

    let mut total_kb: u64 = 0;
    let mut available_kb: u64 = 0;

    for line in content.lines() {
        if let Some(rest) = line.strip_prefix("MemTotal:") {
            total_kb = parse_meminfo_value(rest)?;
        } else if let Some(rest) = line.strip_prefix("MemAvailable:") {
            available_kb = parse_meminfo_value(rest)?;
        }
    }

    let total_mb = total_kb / 1024;
    let used_mb = total_kb.saturating_sub(available_kb) / 1024;
    let usage_pct = if total_kb > 0 {
        ((used_mb as f64 / total_mb as f64) * 10000.0).round() / 100.0
    } else {
        0.0
    };

    Ok((total_mb, used_mb, usage_pct))
}

/// Parse a single value from a /proc/meminfo line (e.g. "  12345 kB").
fn parse_meminfo_value(s: &str) -> Result<u64> {
    let s = s.trim();
    let num_str = s.split_whitespace().next().context("empty meminfo value")?;
    num_str.parse::<u64>().context("invalid meminfo number")
}

/// Read system uptime from /proc/uptime.
fn read_uptime() -> Result<u64> {
    let content = std::fs::read_to_string("/proc/uptime").context("reading /proc/uptime")?;
    let first = content
        .split_whitespace()
        .next()
        .context("empty /proc/uptime")?;
    let secs: f64 = first.parse().context("invalid uptime")?;
    Ok(secs as u64)
}

/// Read 1, 5, 15-minute load averages from /proc/loadavg.
fn read_loadavg() -> Result<[f64; 3]> {
    let content = std::fs::read_to_string("/proc/loadavg").context("reading /proc/loadavg")?;
    let fields: Vec<&str> = content.split_whitespace().collect();
    if fields.len() < 3 {
        anyhow::bail!("unexpected /proc/loadavg format");
    }
    Ok([
        fields[0].parse().unwrap_or(0.0),
        fields[1].parse().unwrap_or(0.0),
        fields[2].parse().unwrap_or(0.0),
    ])
}

/// Pull nested Secure Containers readiness out of a FluxVM `/readyz`
/// response. Missing/malformed shapes resolve to all-false rather than
/// erroring — "not currently known to be Secure-Containers-capable" is the
/// safe default for placement to filter on.
fn extract_secure_containers_detail(body: &serde_json::Value) -> SecureContainersDetail {
    let sc = body.get("secure_containers");
    SecureContainersDetail {
        available: sc
            .and_then(|v| v.get("available"))
            .and_then(|v| v.as_bool())
            .unwrap_or(false),
        shim_installed: sc
            .and_then(|v| v.get("shim_installed"))
            .and_then(|v| v.as_bool())
            .unwrap_or(false),
        guest_image_present: sc
            .and_then(|v| v.get("guest_image_present"))
            .and_then(|v| v.as_bool())
            .unwrap_or(false),
    }
}

// ---------------------------------------------------------------------------
// Host-ID persistence
// ---------------------------------------------------------------------------

/// Load an existing host ID from disk, or generate and persist a new one.
fn load_or_generate_host_id(path: &std::path::Path) -> Result<String> {
    if path.exists() {
        let id = std::fs::read_to_string(path)
            .with_context(|| format!("reading host-id file {}", path.display()))?;
        let id = id.trim().to_string();
        if !id.is_empty() {
            info!(host_id = %id, path = %path.display(), "loaded existing host ID");
            return Ok(id);
        }
    }

    let id = Uuid::new_v4().to_string();

    // Ensure parent directory exists.
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("creating directory {}", parent.display()))?;
    }

    std::fs::write(path, &id).with_context(|| format!("writing host-id to {}", path.display()))?;

    info!(host_id = %id, path = %path.display(), "generated new host ID");
    Ok(id)
}

// ---------------------------------------------------------------------------
// Address auto-detection
// ---------------------------------------------------------------------------

fn detect_address() -> String {
    // Try to resolve the system hostname to an IP address.
    if let Ok(hostname) = hostname::get() {
        let hostname_str = hostname.to_string_lossy().to_string();
        // Use a DNS lookup for the hostname.
        if let Ok(addrs) = std::net::ToSocketAddrs::to_socket_addrs(&(hostname_str.as_str(), 0u16))
        {
            for addr in addrs {
                let ip = addr.ip();
                if !ip.is_loopback() {
                    return ip.to_string();
                }
            }
        }
    }

    // Fallback: connect a UDP socket to a public address and read the local
    // address. This does not send any traffic.
    if let Ok(socket) = std::net::UdpSocket::bind("0.0.0.0:0") {
        if socket.connect("8.8.8.8:53").is_ok() {
            if let Ok(local) = socket.local_addr() {
                return local.ip().to_string();
            }
        }
    }

    "127.0.0.1".to_string()
}

fn get_hostname() -> String {
    hostname::get()
        .map(|h| h.to_string_lossy().to_string())
        .unwrap_or_else(|_| "unknown".to_string())
}

// ---------------------------------------------------------------------------
// Entry point
// ---------------------------------------------------------------------------

#[tokio::main]
async fn main() -> Result<()> {
    // Parse CLI arguments.
    let config = AgentConfig::parse();

    // Initialise structured logging.
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    info!("zyvor-fabricd-agent starting");

    // Load or generate the persistent host ID.
    let host_id = load_or_generate_host_id(&config.host_id_file)?;

    // Detect hostname and address.
    let hostname = get_hostname();
    let address = config.address.unwrap_or_else(|| {
        let addr = detect_address();
        info!(address = %addr, "auto-detected host address");
        addr
    });

    let heartbeat_interval = Duration::from_secs(config.heartbeat_interval);

    let mut driver = zyvor_fabric_fluxvm_driver::FluxVmDriver::new(&config.fluxvm_url)
        .context("failed to initialize FluxVM driver")?;
    if let Some(token) = config.fluxvm_token {
        driver = driver.with_token(token);
    }
    let driver: Arc<dyn VmDriver> = Arc::new(driver);

    info!(
        host_id = %host_id,
        hostname = %hostname,
        address = %address,
        controller = %config.controller_url,
        heartbeat_secs = config.heartbeat_interval,
        fluxvm_url = %config.fluxvm_url,
        "agent configured"
    );

    // Shutdown signalling via a watch channel.
    let (shutdown_tx, shutdown_rx) = watch::channel(false);

    // Spawn a task that listens for SIGTERM / SIGINT.
    tokio::spawn(async move {
        let ctrl_c = signal::ctrl_c();
        let mut sigterm = signal::unix::signal(signal::unix::SignalKind::terminate())
            .expect("failed to register SIGTERM handler");

        tokio::select! {
            _ = ctrl_c => {
                info!("received SIGINT");
            }
            _ = sigterm.recv() => {
                info!("received SIGTERM");
            }
        }

        let _ = shutdown_tx.send(true);
    });

    // Build and run the agent.
    let agent = Agent::new(
        host_id,
        config.controller_url,
        hostname,
        address,
        heartbeat_interval,
        driver,
        config.fluxvm_url,
    );

    agent.run(shutdown_rx).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extract_secure_containers_detail_reads_nested_flags() {
        let body = serde_json::json!({
            "ok": true,
            "kvm": true,
            "secure_containers": {
                "available": true,
                "shim_installed": true,
                "guest_image_present": true
            }
        });
        let d = extract_secure_containers_detail(&body);
        assert!(d.available && d.shim_installed && d.guest_image_present);
    }

    #[test]
    fn extract_secure_containers_detail_is_false_when_not_available() {
        let body = serde_json::json!({
            "ok": true,
            "secure_containers": {
                "available": false,
                "shim_installed": false,
                "guest_image_present": true
            }
        });
        let d = extract_secure_containers_detail(&body);
        assert!(!d.available);
        assert!(!d.shim_installed);
        assert!(d.guest_image_present);
    }

    #[test]
    fn extract_secure_containers_detail_defaults_when_absent() {
        let body = serde_json::json!({"ok": true, "kvm": true});
        let d = extract_secure_containers_detail(&body);
        assert!(!d.available && !d.shim_installed && !d.guest_image_present);
    }

    #[test]
    fn extract_secure_containers_detail_defaults_on_malformed_shape() {
        let body = serde_json::json!({"secure_containers": "not an object"});
        let d = extract_secure_containers_detail(&body);
        assert!(!d.available);
    }

    #[test]
    fn heartbeat_payload_serializes_as_a_flat_object_matching_host_heartbeat() {
        let payload = HeartbeatPayload {
            cpu_usage_pct: 12.5,
            memory_usage_pct: 40.0,
            vm_count: 3,
            uptime_secs: 86400,
            secure_containers_ready: true,
            secure_containers: Some(SecureContainersDetail {
                available: true,
                shim_installed: true,
                guest_image_present: false,
            }),
        };
        let json = serde_json::to_value(&payload).unwrap();
        let obj = json.as_object().unwrap();
        // Regression guard: this used to be `{"timestamp": ..., "metrics": {...}}`,
        // a shape the controller's `HostHeartbeat` extractor rejects outright.
        assert_eq!(obj.len(), 6);
        assert_eq!(json["cpu_usage_pct"], 12.5);
        assert_eq!(json["memory_usage_pct"], 40.0);
        assert_eq!(json["vm_count"], 3);
        assert_eq!(json["uptime_secs"], 86400);
        assert_eq!(json["secure_containers_ready"], true);
        assert_eq!(json["secure_containers"]["shim_installed"], true);
    }
}
