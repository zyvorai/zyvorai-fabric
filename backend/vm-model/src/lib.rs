// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: LicenseRef-Zyvor-Production-1.0

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VM {
    pub name: String,
    pub state: VMState,
    pub cpus: u32,
    pub memory: u64, // in MB
    pub disk: u64,   // in GB
    pub image: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ip: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pid: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mac_address: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hostname: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tags: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub labels: Option<HashMap<String, String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub vnc_port: Option<u16>,
    pub created: DateTime<Utc>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub updated: Option<DateTime<Utc>>,
    /// Last error message from an async operation (e.g. failed start).
    /// Cleared on next successful state transition.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_error: Option<String>,
    /// Host-port -> guest-port forwards for this VM's usermode networking
    /// (e.g. exposing guest port 22 for SSH). Only takes effect on the VM's
    /// next (re)creation in FluxVM -- usermode/slirp networking has no
    /// way to add a forward to an already-running instance.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub port_forwards: Vec<PortForwardSpec>,
    /// Use bridged (tap + private network namespace + DHCP) networking
    /// instead of the default NAT/usermode networking. A bridged VM gets a
    /// real, externally-reachable IP (visible on its Network tab) instead
    /// of needing explicit `port_forwards`, at the cost of the VM needing a
    /// full recreate in FluxVM to change once set (same one-shot-at-
    /// creation-time limitation `port_forwards` has under NAT).
    #[serde(default)]
    pub network_tap: bool,
    /// Only meaningful when `network_tap` is set: configure the guest's
    /// address statically via cloud-init instead of relying on its own
    /// DHCP client (which not every image runs automatically on boot).
    #[serde(default)]
    pub network_static_ip: bool,
    /// Host NIC for a bridge-less FluxVM tap (`l2-uplink` by default).
    /// Mutually exclusive with `network_tap` and user-mode NAT. Stored on
    /// the VM and applied on first start.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub direct_uplink: Option<String>,
    /// `l2-uplink` (default) or `peer-veth`. Absent means `l2-uplink`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub direct_mode: Option<String>,
    /// Guest IPv4s for `l2-uplink` ARP steering. At most 8.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub direct_guest_ips: Vec<String>,
    /// SSH public keys to inject via cloud-init on (re)creation -- also the
    /// reason *any* cloud-init seed gets attached at all for a plain NAT
    /// VM with no other cloud-init needs: without one, cloud-init finds no
    /// datasource and never runs its own default network config, which is
    /// what actually brings the guest's DHCP client up (found live: a VM
    /// with zero cloud-init configured sat forever on
    /// systemd-networkd-wait-online, with no network -- and therefore no
    /// working port forward -- ever coming up).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub ssh_authorized_keys: Vec<String>,
    /// APT/dnf packages to install via cloud-init on (re)creation.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub cloud_init_packages: Vec<String>,
    /// Shell commands to run via cloud-init `runcmd` on first boot.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub cloud_init_runcmd: Vec<String>,
    /// Files to write into the guest before first boot via cloud-init
    /// `write_files`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub cloud_init_write_files: Vec<CloudInitFile>,
    /// FluxVM storage backend: `default` | `lvm-thin` | `nbd` | `ceph-rbd`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub storage: Option<String>,
    #[serde(default)]
    pub enable_qga: bool,
    #[serde(default)]
    pub hyperv: bool,
}

/// A file to write into the guest before first boot via cloud-init.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CloudInitFile {
    pub path: String,
    pub content: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub permissions: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum VMState {
    Running,
    Stopped,
    Paused,
    Starting,
    Stopping,
    Failed,
    Unknown,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateVMRequest {
    pub name: String,
    pub image: String,
    pub cpus: u32,
    pub memory: u64,
    #[serde(default = "default_disk_size")]
    pub disk: u64, // in GB
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hostname: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tags: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub labels: Option<HashMap<String, String>>,
    /// Optional first-class tenant. Merged into `labels["tenant"]` on create
    /// so it flows to FluxVM and `GET /api/vms?tenant=`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tenant: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub port_forwards: Vec<PortForwardSpec>,
    #[serde(default)]
    pub network_tap: bool,
    #[serde(default)]
    pub network_static_ip: bool,
    /// Host NIC for a bridge-less FluxVM tap. Mutually exclusive with
    /// `network_tap` and `port_forwards`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub direct_uplink: Option<String>,
    /// `l2-uplink` (default) or `peer-veth`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub direct_mode: Option<String>,
    /// Guest IPv4s for `l2-uplink` ARP steering. At most 8.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub direct_guest_ips: Vec<String>,
    /// FluxVM storage backend: `default` | `lvm-thin` | `nbd` | `ceph-rbd`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub storage: Option<String>,
    /// Enable QEMU Guest Agent at create (Windows Kryton).
    #[serde(default)]
    pub enable_qga: bool,
    #[serde(default)]
    pub hyperv: bool,
}

fn default_disk_size() -> u64 {
    20 // 20GB default disk size
}

impl CreateVMRequest {
    /// Validate all fields for correctness.
    /// Returns a list of validation errors, or Ok(()) if all fields are valid.
    pub fn validate(&self) -> Result<(), Vec<String>> {
        let mut errors = Vec::new();

        if self.image.is_empty() {
            errors.push("Image must not be empty".to_string());
        }

        if self.cpus < 1 || self.cpus > 256 {
            errors.push(format!("CPUs must be between 1 and 256, got {}", self.cpus));
        }

        if self.memory < 128 || self.memory > 1_048_576 {
            errors.push(format!(
                "Memory must be between 128 MB and 1048576 MB (1 TB), got {} MB",
                self.memory
            ));
        }

        if self.disk < 1 || self.disk > 65_536 {
            errors.push(format!(
                "Disk must be between 1 GB and 65536 GB (64 TB), got {} GB",
                self.disk
            ));
        }

        errors.extend(direct_uplink_errors(
            self.direct_uplink.as_deref(),
            self.direct_mode.as_deref(),
            &self.direct_guest_ips,
            self.network_tap,
            !self.port_forwards.is_empty(),
        ));

        if errors.is_empty() {
            Ok(())
        } else {
            Err(errors)
        }
    }
}

/// Console mode for the VM
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(rename_all = "kebab-case")]
pub enum ConsoleMode {
    #[default]
    Interactive,
    ReadOnly,
    Native,
    Gui,
}

impl std::fmt::Display for ConsoleMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ConsoleMode::Interactive => write!(f, "interactive"),
            ConsoleMode::ReadOnly => write!(f, "read-only"),
            ConsoleMode::Native => write!(f, "native"),
            ConsoleMode::Gui => write!(f, "gui"),
        }
    }
}

/// A bind mount from host into the VM
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct BindMount {
    /// Source path on the host
    pub source: String,
    /// Destination path in the VM (if different from source)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub destination: Option<String>,
    /// Read-only mount
    #[serde(default)]
    pub read_only: bool,
}

/// Manager scope for the VM (system or user)
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum ManagerScope {
    System,
    User,
}

/// SSH key type for VM key generation
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum SshKeyType {
    Ed25519,
    Ecdsa,
    Rsa,
}

impl std::fmt::Display for SshKeyType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SshKeyType::Ed25519 => write!(f, "ed25519"),
            SshKeyType::Ecdsa => write!(f, "ecdsa"),
            SshKeyType::Rsa => write!(f, "rsa"),
        }
    }
}

impl std::fmt::Display for ManagerScope {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ManagerScope::System => write!(f, "system"),
            ManagerScope::User => write!(f, "user"),
        }
    }
}

/// A credential to load from a file path via --load-credential
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct LoadCredential {
    pub id: String,
    /// File path to load the credential value from
    #[serde(alias = "value")]
    pub path: String,
}

/// Display configuration for VM remote access.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct DisplayConfig {
    #[serde(default = "default_display_type")]
    pub display_type: DisplayType,
    pub port: Option<u16>,
}

fn default_display_type() -> DisplayType {
    DisplayType::None
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum DisplayType {
    Vnc,
    Spice,
    None,
}

/// A single host-port -> guest-port forward for user-mode (SLIRP)
/// networking — the cheap way to reach one VM's SSH/service from outside
/// the host without bridged networking + nftables floating IPs. Ignored
/// by backends/options that use `network_tap` instead.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PortForwardSpec {
    pub host_port: u16,
    pub guest_port: u16,
    #[serde(default = "default_port_forward_protocol")]
    pub protocol: String,
}
fn default_port_forward_protocol() -> String {
    "tcp".to_string()
}

/// Options for starting a VM via systemd-vmspawn
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct VMStartOptions {
    // -- Manager Scope --
    /// Legacy manager scope (ignored under FluxVM)
    /// (None = auto: system when root, user otherwise)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scope: Option<ManagerScope>,

    // -- Image Options --
    /// Use directory instead of image
    #[serde(skip_serializing_if = "Option::is_none")]
    pub directory: Option<String>,

    // -- Host Configuration --
    /// Use KVM acceleration (None = auto-detect)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub kvm: Option<bool>,
    /// Enable Secure Boot firmware
    #[serde(skip_serializing_if = "Option::is_none")]
    pub secure_boot: Option<bool>,
    /// Enable VSock networking
    #[serde(skip_serializing_if = "Option::is_none")]
    pub vsock: Option<bool>,
    /// VSock CID (None = auto)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub vsock_cid: Option<u32>,
    /// Enable TPM support (None = auto-detect swtpm)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tpm: Option<bool>,
    /// TPM state directory path, "auto", or "off"
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tpm_state: Option<String>,
    /// Linux kernel image path for direct kernel boot
    #[serde(skip_serializing_if = "Option::is_none")]
    pub linux: Option<String>,
    /// Initrd paths for direct kernel boot (can be multiple, merged)
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub initrd: Vec<String>,
    /// Create a TAP device for networking
    #[serde(default)]
    pub network_tap: bool,
    /// Only meaningful when `network_tap` is set: configure the guest's
    /// address statically via cloud-init instead of DHCP.
    #[serde(default)]
    pub network_static_ip: bool,
    /// Use user mode networking
    #[serde(default)]
    pub network_user_mode: bool,
    /// Host NIC for a bridge-less FluxVM tap. Mutually exclusive with
    /// `network_tap` and user-mode NAT.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub direct_uplink: Option<String>,
    /// `l2-uplink` (default) or `peer-veth`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub direct_mode: Option<String>,
    /// Guest IPv4s for `l2-uplink` ARP steering. At most 8.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub direct_guest_ips: Vec<String>,
    /// Host-port -> guest-port forwards for user-mode networking. Ignored
    /// when `network_tap` is set.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub port_forwards: Vec<PortForwardSpec>,
    /// SSH public keys to inject via cloud-init on (re)creation. Also what
    /// gets a cloud-init seed disk attached at all for a plain NAT VM with
    /// nothing else cloud-init-worthy configured -- without one, cloud-init
    /// never finds a datasource and never runs its own default network
    /// config, which is what actually brings the guest's DHCP client up.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub ssh_authorized_keys: Vec<String>,
    /// APT/dnf packages to install via cloud-init on (re)creation.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub cloud_init_packages: Vec<String>,
    /// Shell commands to run via cloud-init `runcmd` on first boot.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub cloud_init_runcmd: Vec<String>,
    /// Files to write into the guest before first boot via cloud-init
    /// `write_files`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub cloud_init_write_files: Vec<CloudInitFile>,
    /// Firmware definition file path
    #[serde(skip_serializing_if = "Option::is_none")]
    pub firmware: Option<String>,
    /// Process discard requests from the VM (default: true)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub discard_disk: Option<bool>,
    /// Grow the image to the specified size (e.g. "50G")
    #[serde(skip_serializing_if = "Option::is_none")]
    pub grow_image: Option<String>,
    /// SMBIOS Type #11 vendor strings
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub smbios11: Vec<String>,
    /// Notify ready behavior (default: true)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub notify_ready: Option<bool>,

    // -- System Identity Options --
    /// Machine UUID
    #[serde(skip_serializing_if = "Option::is_none")]
    pub uuid: Option<String>,

    // -- Property Options --
    /// Systemd slice for the VM scope unit
    #[serde(skip_serializing_if = "Option::is_none")]
    pub slice: Option<String>,
    /// Unit properties for the scope unit
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub properties: Vec<String>,
    /// Legacy register flag (ignored; FluxVM does not use systemd-machined)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub register: Option<bool>,

    // -- User Namespacing Options --
    /// Private users mapping (e.g. "1000:65536")
    #[serde(skip_serializing_if = "Option::is_none")]
    pub private_users: Option<String>,

    // -- Mount Options --
    /// Bind mounts from host into VM
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub bind_mounts: Vec<BindMount>,
    /// Extra drives (disk images or block devices)
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub extra_drives: Vec<String>,
    /// Bind host users into the VM
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub bind_users: Vec<String>,
    /// Shell for bound users (bool or absolute path)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bind_user_shell: Option<String>,
    /// Auxiliary groups for bound users
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub bind_user_groups: Vec<String>,

    // -- Integration Options --
    /// Forward VM journal to host (file or directory path)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub forward_journal: Option<String>,
    /// Generate and pass SSH key to VM (default: true)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pass_ssh_key: Option<bool>,
    /// SSH key type to generate (default: ed25519)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ssh_key_type: Option<SshKeyType>,

    // -- Input/Output Options --
    /// Console mode
    #[serde(skip_serializing_if = "Option::is_none")]
    pub console: Option<ConsoleMode>,
    /// Terminal background color (ANSI SGR)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub background: Option<String>,
    /// Quiet mode (suppress VM status output)
    #[serde(default)]
    pub quiet: bool,

    // -- Credentials --
    /// Credentials to pass (ID -> value) via --set-credential
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub credentials: Vec<VMCredential>,
    /// Credentials to load from file via --load-credential
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub load_credentials: Vec<LoadCredential>,

    // -- Display Options --
    /// Display configuration (VNC, SPICE, or None)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub display: Option<DisplayConfig>,

    // -- Extra kernel command line arguments --
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub extra_args: Vec<String>,

    /// FluxVM pluggable storage backend: `default`, `lvm-thin`, `nbd`, `ceph-rbd`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub storage: Option<String>,
    /// Enable QEMU Guest Agent channel at create time (Windows Kryton path).
    #[serde(default)]
    pub enable_qga: bool,
    /// Expose Hyper-V enlightenments to the guest (Windows).
    #[serde(default)]
    pub hyperv: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub numa_node: Option<u8>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cpuset: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hugepages: Option<bool>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub vfio_devices: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct VMCredential {
    pub id: String,
    pub value: String,
}

// ============================================================================
// VMStartOptions validation
// ============================================================================

/// Allowlist of safe systemd unit properties for VM scope units.
/// Only resource-control and informational properties are permitted.
const SAFE_PROPERTY_PREFIXES: &[&str] = &[
    // Memory
    "MemoryMax=",
    "MemoryMin=",
    "MemoryHigh=",
    "MemoryLow=",
    "MemorySwapMax=",
    "MemoryLimit=",
    // CPU
    "CPUQuota=",
    "CPUWeight=",
    "CPUShares=",
    "AllowedCPUs=",
    // IO
    "IOWeight=",
    "IOReadBandwidthMax=",
    "IOWriteBandwidthMax=",
    "IOReadIOPSMax=",
    "IOWriteIOPSMax=",
    "IODeviceWeight=",
    // Tasks
    "TasksMax=",
    // Network
    "IPAddressAllow=",
    "IPAddressDeny=",
    // Limits
    "LimitNOFILE=",
    "LimitNPROC=",
    "LimitMEMLOCK=",
    "LimitAS=",
    "LimitCORE=",
    "LimitFSIZE=",
    // Description
    "Description=",
];

fn is_valid_uuid(s: &str) -> bool {
    let parts: Vec<&str> = s.split('-').collect();
    if parts.len() != 5 {
        return false;
    }
    let expected_lengths = [8, 4, 4, 4, 12];
    parts
        .iter()
        .zip(expected_lengths.iter())
        .all(|(part, &len)| part.len() == len && part.chars().all(|c| c.is_ascii_hexdigit()))
}

fn is_valid_credential_id(id: &str) -> bool {
    !id.is_empty()
        && !id.contains(':')
        && !id.contains('/')
        && id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_'))
}

fn is_valid_size_string(s: &str) -> bool {
    let s = s.trim();
    if s.is_empty() {
        return false;
    }
    let num_part = s.trim_end_matches(|c: char| c.is_ascii_alphabetic());
    if num_part.is_empty() {
        return false;
    }
    if !num_part.chars().all(|c| c.is_ascii_digit()) {
        return false;
    }
    let suffix = &s[num_part.len()..];
    suffix.is_empty()
        || matches!(
            suffix,
            "K" | "M" | "G" | "T" | "P" | "E" | "k" | "m" | "g" | "t" | "p" | "e"
        )
}

fn path_has_traversal(path: &str) -> bool {
    std::path::Path::new(path)
        .components()
        .any(|c| matches!(c, std::path::Component::ParentDir))
}

impl VMStartOptions {
    /// Validate all fields for security and correctness.
    /// Returns a list of validation errors, or Ok(()) if all fields are valid.
    pub fn validate(&self) -> Result<(), Vec<String>> {
        let mut errors = Vec::new();

        // Validate UUID format
        if let Some(ref uuid) = self.uuid {
            if !is_valid_uuid(uuid) {
                errors.push(format!("Invalid UUID format: '{}'", uuid));
            }
        }

        // Validate credential IDs
        for cred in &self.credentials {
            if !is_valid_credential_id(&cred.id) {
                errors.push(format!(
                    "Invalid credential ID '{}': must be alphanumeric/dot/hyphen/underscore, no colons",
                    cred.id
                ));
            }
        }
        for cred in &self.load_credentials {
            if !is_valid_credential_id(&cred.id) {
                errors.push(format!(
                    "Invalid load-credential ID '{}': must be alphanumeric/dot/hyphen/underscore, no colons",
                    cred.id
                ));
            }
        }

        // Validate credential values: max length and no control characters
        for cred in &self.credentials {
            if cred.value.len() > 65536 {
                errors.push(format!(
                    "Credential value for '{}' exceeds maximum length of 64KB",
                    cred.id
                ));
            }
            if cred.value.chars().any(|c| c.is_control() && c != '\n') {
                errors.push(format!(
                    "Credential value for '{}' contains control characters",
                    cred.id
                ));
            }
        }

        // Validate extra_args don't start with '-' and contain no control characters
        for arg in &self.extra_args {
            if arg.starts_with('-') {
                errors.push(format!(
                    "Extra argument must not start with '-' (prevents flag injection): '{}'",
                    arg
                ));
            }
            if arg.chars().any(|c| c.is_control()) {
                errors.push(format!(
                    "Extra argument must not contain control characters: '{}'",
                    arg
                ));
            }
        }

        // Validate properties against allowlist
        for prop in &self.properties {
            if !SAFE_PROPERTY_PREFIXES
                .iter()
                .any(|prefix| prop.starts_with(prefix))
            {
                errors.push(format!(
                    "Property '{}' not in allowlist. Allowed prefixes: MemoryMax, CPUQuota, IOWeight, TasksMax, etc.",
                    prop
                ));
            }
        }

        // Validate smbios11 entries don't override systemd credentials
        // Block both io.systemd.credential: and io.systemd.credential.binary:
        for s in &self.smbios11 {
            if s.starts_with("io.systemd.credential") {
                errors.push(
                    "SMBIOS11 entries must not start with 'io.systemd.credential' (use credentials field instead)".to_string()
                );
            }
            if s.chars().any(|c| c.is_control()) {
                errors.push("SMBIOS11 entries must not contain control characters".to_string());
            }
        }

        // Validate bind_users don't include root or system accounts
        const BLOCKED_USERS: &[&str] = &[
            "root",
            "daemon",
            "bin",
            "sys",
            "sync",
            "games",
            "man",
            "lp",
            "mail",
            "news",
            "uucp",
            "proxy",
            "www-data",
            "backup",
            "nobody",
            "systemd-network",
            "systemd-resolve",
            "messagebus",
            "sshd",
            "polkitd",
            "avahi",
        ];
        for user in &self.bind_users {
            if BLOCKED_USERS.contains(&user.as_str()) {
                errors.push(format!("Cannot bind system user '{}' into VM", user));
            }
            // Block numeric UIDs < 1000
            if let Ok(uid) = user.parse::<u32>() {
                if uid < 1000 {
                    errors.push(format!("Cannot bind system UID {} (< 1000) into VM", uid));
                }
            }
            // Validate username character set
            if user.is_empty()
                || !user
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'))
            {
                errors.push(format!(
                    "Invalid bind_user '{}': must be alphanumeric with hyphens/underscores/dots only",
                    user
                ));
            }
        }

        // Validate slice name format
        if let Some(ref slice) = self.slice {
            let valid = slice.ends_with(".slice")
                && !slice.is_empty()
                && slice
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'));
            if !valid {
                errors.push(format!(
                    "Invalid slice name '{}': must end with '.slice', alphanumeric/hyphens/underscores/dots only",
                    slice
                ));
            }
        }

        // Validate grow_image format
        if let Some(ref grow) = self.grow_image {
            if !is_valid_size_string(grow) {
                errors.push(format!(
                    "Invalid grow_image size '{}': use format like '50G', '100M'",
                    grow
                ));
            }
        }

        // Validate bind_user_groups names (alphanumeric, hyphens, underscores only)
        for group in &self.bind_user_groups {
            if group.is_empty()
                || !group
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_'))
            {
                errors.push(format!(
                    "Invalid bind_user_group '{}': must be alphanumeric with hyphens/underscores only",
                    group
                ));
            }
        }

        // Validate bind_user_shell: must be absolute path or boolean-like
        if let Some(ref shell) = self.bind_user_shell {
            let is_bool = matches!(shell.as_str(), "true" | "false" | "yes" | "no");
            let is_abs_path = shell.starts_with('/')
                && shell
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || matches!(c, '/' | '-' | '_' | '.'));
            if !is_bool && !is_abs_path {
                errors.push(format!(
                    "Invalid bind_user_shell '{}': must be an absolute path or boolean",
                    shell
                ));
            }
            if path_has_traversal(shell) {
                errors.push("bind_user_shell must not contain '..' path traversal".to_string());
            }
        }

        // Validate background (ANSI SGR): only digits and semicolons
        if let Some(ref bg) = self.background {
            if !bg.chars().all(|c| c.is_ascii_digit() || c == ';') {
                errors.push(format!(
                    "Invalid background '{}': must contain only digits and semicolons (ANSI SGR)",
                    bg
                ));
            }
        }

        // Validate private_users format
        if let Some(ref pu) = self.private_users {
            let valid = matches!(pu.as_str(), "yes" | "no" | "identity" | "pick")
                || pu
                    .split(':')
                    .all(|part| !part.is_empty() && part.chars().all(|c| c.is_ascii_digit()));
            if !valid {
                errors.push(format!(
                    "Invalid private_users '{}': must be yes|no|identity|pick or UID:COUNT format",
                    pu
                ));
            }
        }

        // Validate path fields don't contain '..' traversal
        let path_fields: &[(&str, &Option<String>)] = &[
            ("directory", &self.directory),
            ("linux", &self.linux),
            ("firmware", &self.firmware),
            ("forward_journal", &self.forward_journal),
        ];
        for (name, field) in path_fields {
            if let Some(ref path) = field {
                if path_has_traversal(path) {
                    errors.push(format!(
                        "Field '{}' must not contain '..' path traversal",
                        name
                    ));
                }
            }
        }

        // tpm_state can be "auto" or "off" as special values
        if let Some(ref tpm_state) = self.tpm_state {
            if tpm_state != "auto" && tpm_state != "off" && path_has_traversal(tpm_state) {
                errors.push("Field 'tpm_state' must not contain '..' path traversal".to_string());
            }
        }

        for (i, initrd) in self.initrd.iter().enumerate() {
            if path_has_traversal(initrd) {
                errors.push(format!(
                    "initrd[{}] must not contain '..' path traversal",
                    i
                ));
            }
        }

        for (i, drive) in self.extra_drives.iter().enumerate() {
            if path_has_traversal(drive) {
                errors.push(format!(
                    "extra_drives[{}] must not contain '..' path traversal",
                    i
                ));
            }
        }

        for (i, bm) in self.bind_mounts.iter().enumerate() {
            if path_has_traversal(&bm.source) {
                errors.push(format!(
                    "bind_mounts[{}].source must not contain '..' path traversal",
                    i
                ));
            }
            if let Some(ref dest) = bm.destination {
                if path_has_traversal(dest) {
                    errors.push(format!(
                        "bind_mounts[{}].destination must not contain '..' path traversal",
                        i
                    ));
                }
            }
        }

        for (i, cred) in self.load_credentials.iter().enumerate() {
            if path_has_traversal(&cred.path) {
                errors.push(format!(
                    "load_credentials[{}].path must not contain '..' path traversal",
                    i
                ));
            }
        }

        {
            let mut seen_host_ports = std::collections::HashSet::new();
            for (i, fwd) in self.port_forwards.iter().enumerate() {
                if fwd.host_port == 0 || fwd.guest_port == 0 {
                    errors.push(format!("port_forwards[{}]: ports must be non-zero", i));
                }
                if !matches!(fwd.protocol.as_str(), "tcp" | "udp") {
                    errors.push(format!(
                        "port_forwards[{}].protocol must be 'tcp' or 'udp', got '{}'",
                        i, fwd.protocol
                    ));
                }
                if !seen_host_ports.insert(fwd.host_port) {
                    errors.push(format!(
                        "port_forwards[{}]: host_port {} is forwarded more than once",
                        i, fwd.host_port
                    ));
                }
            }
        }

        errors.extend(direct_uplink_errors(
            self.direct_uplink.as_deref(),
            self.direct_mode.as_deref(),
            &self.direct_guest_ips,
            self.network_tap,
            self.network_user_mode || !self.port_forwards.is_empty(),
        ));

        if errors.is_empty() {
            Ok(())
        } else {
            Err(errors)
        }
    }
}

/// Shared checks for a bridge-less uplink. Empty when `uplink` is unset and
/// no stray mode or guest IPs were sent.
pub fn direct_uplink_errors(
    uplink: Option<&str>,
    mode: Option<&str>,
    guest_ips: &[String],
    network_tap: bool,
    user_nat: bool,
) -> Vec<String> {
    let mut errors = Vec::new();
    let outer = uplink.map(str::trim).filter(|s| !s.is_empty());
    if outer.is_none() {
        if mode.map(|m| !m.trim().is_empty()).unwrap_or(false) || !guest_ips.is_empty() {
            errors.push("direct_mode and direct_guest_ips require direct_uplink".to_string());
        }
        return errors;
    }
    let outer = outer.unwrap();
    if network_tap {
        errors.push("direct_uplink is mutually exclusive with network_tap".to_string());
    }
    if user_nat {
        errors.push(
            "direct_uplink is mutually exclusive with user-mode NAT (port forwards)".to_string(),
        );
    }
    if !(1..=15).contains(&outer.len()) || outer.contains('/') || outer.contains(' ') {
        errors.push(format!(
            "direct_uplink '{outer}' must be an interface name (1-15 characters, no slash or space)"
        ));
    }
    let mode = mode
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .unwrap_or("l2-uplink");
    if mode != "l2-uplink" && mode != "peer-veth" {
        errors.push(format!(
            "direct_mode must be 'l2-uplink' or 'peer-veth', got '{mode}'"
        ));
    }
    if mode != "l2-uplink" && !guest_ips.is_empty() {
        errors.push("direct_guest_ips are only valid for direct_mode l2-uplink".to_string());
    }
    if guest_ips.len() > 8 {
        errors.push(format!(
            "direct_guest_ips accepts at most 8 addresses, got {}",
            guest_ips.len()
        ));
    }
    for ip in guest_ips {
        if ip.parse::<std::net::Ipv4Addr>().is_err() {
            errors.push(format!(
                "direct_guest_ips entry '{ip}' is not an IPv4 address"
            ));
        }
    }
    errors
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VMMetrics {
    pub cpu_usage: f64,
    pub memory_usage: u64,
    pub disk_usage: u64,
    pub network_rx: u64,
    pub network_tx: u64,
}

/// PSI pressure record for a single stall category (some or full).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PressureRecord {
    pub avg10: f64,
    pub avg60: f64,
    pub avg300: f64,
    pub total: u64,
}

/// PSI pressure metrics for a VM's cgroup.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VMPressure {
    /// CPU pressure (some only).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cpu_some: Option<PressureRecord>,
    /// Memory pressure (some).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub memory_some: Option<PressureRecord>,
    /// Memory pressure (full).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub memory_full: Option<PressureRecord>,
    /// I/O pressure (some).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub io_some: Option<PressureRecord>,
    /// I/O pressure (full).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub io_full: Option<PressureRecord>,
}

impl VM {
    pub fn new(name: String, image: String, cpus: u32, memory: u64) -> Self {
        Self::with_disk(name, image, cpus, memory, 20)
    }

    pub fn with_disk(name: String, image: String, cpus: u32, memory: u64, disk: u64) -> Self {
        Self {
            name,
            state: VMState::Stopped,
            cpus,
            memory,
            disk,
            image,
            ip: None,
            pid: None,
            mac_address: None,
            hostname: None,
            tags: None,
            labels: None,
            vnc_port: None,
            created: Utc::now(),
            updated: None,
            last_error: None,
            port_forwards: Vec::new(),
            network_tap: false,
            network_static_ip: false,
            direct_uplink: None,
            direct_mode: None,
            direct_guest_ips: Vec::new(),
            ssh_authorized_keys: Vec::new(),
            cloud_init_packages: Vec::new(),
            cloud_init_runcmd: Vec::new(),
            cloud_init_write_files: Vec::new(),
            storage: None,
            enable_qga: false,
            hyperv: false,
        }
    }

    pub fn from_request(req: &CreateVMRequest) -> Self {
        let mut labels = req.labels.clone();
        if let Some(ref tenant) = req.tenant {
            let map = labels.get_or_insert_with(HashMap::new);
            map.entry("tenant".to_string())
                .or_insert_with(|| tenant.clone());
        }
        Self {
            name: req.name.clone(),
            state: VMState::Stopped,
            cpus: req.cpus,
            memory: req.memory,
            disk: req.disk,
            image: req.image.clone(),
            ip: None,
            pid: None,
            mac_address: None,
            hostname: req.hostname.clone(),
            tags: req.tags.clone(),
            labels,
            vnc_port: None,
            created: Utc::now(),
            updated: None,
            last_error: None,
            port_forwards: req.port_forwards.clone(),
            network_tap: req.network_tap,
            network_static_ip: req.network_static_ip,
            direct_uplink: req.direct_uplink.clone(),
            direct_mode: req.direct_mode.clone(),
            direct_guest_ips: req.direct_guest_ips.clone(),
            ssh_authorized_keys: Vec::new(),
            cloud_init_packages: Vec::new(),
            cloud_init_runcmd: Vec::new(),
            cloud_init_write_files: Vec::new(),
            storage: req.storage.clone(),
            enable_qga: req.enable_qga,
            hyperv: req.hyperv,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_vm_new() {
        let vm = VM::new("test".to_string(), "img.qcow2".to_string(), 4, 2048);
        assert_eq!(vm.name, "test");
        assert_eq!(vm.cpus, 4);
        assert_eq!(vm.memory, 2048);
        assert_eq!(vm.disk, 20); // default
        assert_eq!(vm.state, VMState::Stopped);
        assert!(vm.ip.is_none());
    }

    #[test]
    fn test_vm_with_disk() {
        let vm = VM::with_disk("db".to_string(), "img.qcow2".to_string(), 8, 4096, 100);
        assert_eq!(vm.disk, 100);
    }

    #[test]
    fn test_vm_from_request() {
        let mut labels = HashMap::new();
        labels.insert("app".to_string(), "web".to_string());
        labels.insert("env".to_string(), "prod".to_string());
        let req = CreateVMRequest {
            name: "web-01".to_string(),
            image: "ubuntu.img".to_string(),
            cpus: 2,
            memory: 1024,
            disk: 50,
            hostname: Some("web-server".to_string()),
            tags: Some(vec!["production".to_string()]),
            labels: Some(labels.clone()),
            tenant: None,
            port_forwards: Vec::new(),
            network_tap: false,
            network_static_ip: false,
            direct_uplink: None,
            direct_mode: None,
            direct_guest_ips: Vec::new(),
            storage: None,
            enable_qga: false,
            hyperv: false,
        };
        let vm = VM::from_request(&req);
        assert_eq!(vm.name, "web-01");
        assert_eq!(vm.hostname, Some("web-server".to_string()));
        assert_eq!(vm.tags, Some(vec!["production".to_string()]));
        assert_eq!(vm.labels, Some(labels));
    }

    #[test]
    fn test_serialization_roundtrip() {
        let vm = VM::new("roundtrip".to_string(), "test.img".to_string(), 2, 1024);
        let json = serde_json::to_string(&vm).unwrap();
        let deserialized: VM = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.name, "roundtrip");
        assert_eq!(deserialized.cpus, 2);
        assert_eq!(deserialized.memory, 1024);
    }

    #[test]
    fn test_vmstate_serialization() {
        let json = serde_json::to_string(&VMState::Running).unwrap();
        assert_eq!(json, "\"running\"");

        let state: VMState = serde_json::from_str("\"stopped\"").unwrap();
        assert_eq!(state, VMState::Stopped);
    }

    #[test]
    fn test_labels_backward_compat() {
        // Deserialize JSON without labels field — should default to None
        let json = r#"{
            "name": "old-vm",
            "state": "stopped",
            "cpus": 2,
            "memory": 1024,
            "disk": 20,
            "image": "img.qcow2",
            "created": "2025-01-01T00:00:00Z"
        }"#;
        let vm: VM = serde_json::from_str(json).unwrap();
        assert!(vm.labels.is_none());
    }

    #[test]
    fn test_labels_roundtrip() {
        let mut vm = VM::new("labeled".to_string(), "img.qcow2".to_string(), 2, 1024);
        let mut labels = HashMap::new();
        labels.insert("app".to_string(), "web".to_string());
        labels.insert("env".to_string(), "prod".to_string());
        vm.labels = Some(labels.clone());

        let json = serde_json::to_string(&vm).unwrap();
        assert!(json.contains("\"labels\""));
        let deserialized: VM = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.labels, Some(labels));
    }

    #[test]
    fn test_optional_fields_omitted() {
        let vm = VM::new("minimal".to_string(), "img".to_string(), 1, 512);
        let json = serde_json::to_string(&vm).unwrap();
        assert!(!json.contains("\"ip\""));
        assert!(!json.contains("\"pid\""));
        assert!(!json.contains("\"tags\""));
    }

    #[test]
    fn test_default_disk_size() {
        let req: CreateVMRequest = serde_json::from_str(
            r#"{
            "name": "test",
            "image": "img",
            "cpus": 1,
            "memory": 512
        }"#,
        )
        .unwrap();
        assert_eq!(req.disk, 20); // default
    }

    // ========================================================================
    // VMStartOptions tests
    // ========================================================================

    #[test]
    fn test_start_options_default() {
        let opts = VMStartOptions::default();
        assert!(opts.scope.is_none());
        assert!(opts.directory.is_none());
        assert!(opts.kvm.is_none());
        assert!(opts.secure_boot.is_none());
        assert!(opts.vsock.is_none());
        assert!(opts.vsock_cid.is_none());
        assert!(opts.tpm.is_none());
        assert!(opts.tpm_state.is_none());
        assert!(opts.linux.is_none());
        assert!(opts.initrd.is_empty());
        assert!(!opts.network_tap);
        assert!(!opts.network_user_mode);
        assert!(opts.firmware.is_none());
        assert!(opts.discard_disk.is_none());
        assert!(opts.grow_image.is_none());
        assert!(opts.smbios11.is_empty());
        assert!(opts.notify_ready.is_none());
        assert!(opts.uuid.is_none());
        assert!(opts.slice.is_none());
        assert!(opts.properties.is_empty());
        assert!(opts.register.is_none());
        assert!(opts.private_users.is_none());
        assert!(opts.bind_mounts.is_empty());
        assert!(opts.extra_drives.is_empty());
        assert!(opts.bind_users.is_empty());
        assert!(opts.bind_user_shell.is_none());
        assert!(opts.bind_user_groups.is_empty());
        assert!(opts.forward_journal.is_none());
        assert!(opts.pass_ssh_key.is_none());
        assert!(opts.ssh_key_type.is_none());
        assert!(opts.console.is_none());
        assert!(opts.background.is_none());
        assert!(!opts.quiet);
        assert!(opts.credentials.is_empty());
        assert!(opts.load_credentials.is_empty());
        assert!(opts.extra_args.is_empty());
    }

    #[test]
    fn test_start_options_roundtrip() {
        let opts = VMStartOptions {
            scope: Some(ManagerScope::User),
            directory: Some("/my/dir".into()),
            kvm: Some(true),
            secure_boot: Some(false),
            vsock: Some(true),
            vsock_cid: Some(42),
            tpm: Some(true),
            tpm_state: Some("auto".into()),
            linux: Some("/boot/vmlinuz".into()),
            initrd: vec!["/boot/initrd.img".into()],
            network_tap: true,
            network_static_ip: false,
            network_user_mode: false,
            port_forwards: vec![PortForwardSpec {
                host_port: 2222,
                guest_port: 22,
                protocol: "tcp".into(),
            }],
            ssh_authorized_keys: vec!["ssh-ed25519 AAAA test".into()],
            cloud_init_packages: vec!["curl".into()],
            cloud_init_runcmd: vec!["echo hi".into()],
            cloud_init_write_files: vec![CloudInitFile {
                path: "/etc/motd".into(),
                content: "hi".into(),
                permissions: Some("0644".into()),
            }],
            firmware: Some("/usr/share/ovmf/OVMF.fd".into()),
            discard_disk: Some(true),
            grow_image: Some("50G".into()),
            smbios11: vec!["io.systemd.foo=bar".into()],
            notify_ready: Some(true),
            uuid: Some("550e8400-e29b-41d4-a716-446655440000".into()),
            slice: Some("vm.slice".into()),
            properties: vec!["MemoryMax=4G".into()],
            register: Some(true),
            private_users: Some("1000:65536".into()),
            bind_mounts: vec![BindMount {
                source: "/host/data".into(),
                destination: Some("/vm/data".into()),
                read_only: true,
            }],
            extra_drives: vec!["/extra/disk.raw".into()],
            bind_users: vec!["testuser".into()],
            bind_user_shell: Some("/bin/bash".into()),
            bind_user_groups: vec!["wheel".into()],
            forward_journal: Some("/var/log/vm.journal".into()),
            pass_ssh_key: Some(true),
            ssh_key_type: Some(SshKeyType::Ed25519),
            console: Some(ConsoleMode::Interactive),
            background: Some("44".into()),
            quiet: true,
            credentials: vec![VMCredential {
                id: "passwd.hashed-password.root".into(),
                value: "hunter2".into(),
            }],
            load_credentials: vec![LoadCredential {
                id: "ssh.authorized_keys.root".into(),
                path: "/root/.ssh/authorized_keys".into(),
            }],
            display: None,
            extra_args: vec!["enforcing=0".into()],
            storage: Some("default".into()),
            enable_qga: true,
            hyperv: false,
            numa_node: Some(0),
            cpuset: Some("0-1".into()),
            hugepages: Some(false),
            vfio_devices: vec![],
            direct_uplink: None,
            direct_mode: None,
            direct_guest_ips: vec![],
        };

        let json = serde_json::to_string(&opts).unwrap();
        let deserialized: VMStartOptions = serde_json::from_str(&json).unwrap();

        assert_eq!(deserialized.scope, opts.scope);
        assert_eq!(deserialized.directory, opts.directory);
        assert_eq!(deserialized.kvm, opts.kvm);
        assert_eq!(deserialized.secure_boot, opts.secure_boot);
        assert_eq!(deserialized.vsock, opts.vsock);
        assert_eq!(deserialized.vsock_cid, opts.vsock_cid);
        assert_eq!(deserialized.tpm, opts.tpm);
        assert_eq!(deserialized.tpm_state, opts.tpm_state);
        assert_eq!(deserialized.linux, opts.linux);
        assert_eq!(deserialized.initrd, opts.initrd);
        assert_eq!(deserialized.network_tap, opts.network_tap);
        assert_eq!(deserialized.network_user_mode, opts.network_user_mode);
        assert_eq!(deserialized.firmware, opts.firmware);
        assert_eq!(deserialized.discard_disk, opts.discard_disk);
        assert_eq!(deserialized.grow_image, opts.grow_image);
        assert_eq!(deserialized.smbios11, opts.smbios11);
        assert_eq!(deserialized.notify_ready, opts.notify_ready);
        assert_eq!(deserialized.uuid, opts.uuid);
        assert_eq!(deserialized.slice, opts.slice);
        assert_eq!(deserialized.properties, opts.properties);
        assert_eq!(deserialized.register, opts.register);
        assert_eq!(deserialized.private_users, opts.private_users);
        assert_eq!(deserialized.extra_drives, opts.extra_drives);
        assert_eq!(deserialized.bind_users, opts.bind_users);
        assert_eq!(deserialized.bind_user_shell, opts.bind_user_shell);
        assert_eq!(deserialized.bind_user_groups, opts.bind_user_groups);
        assert_eq!(deserialized.forward_journal, opts.forward_journal);
        assert_eq!(deserialized.pass_ssh_key, opts.pass_ssh_key);
        assert_eq!(deserialized.ssh_key_type, opts.ssh_key_type);
        assert_eq!(deserialized.console, opts.console);
        assert_eq!(deserialized.background, opts.background);
        assert_eq!(deserialized.quiet, opts.quiet);
        assert_eq!(deserialized.extra_args, opts.extra_args);
        // BindMount fields
        assert_eq!(deserialized.bind_mounts.len(), 1);
        assert_eq!(deserialized.bind_mounts[0].source, "/host/data");
        assert_eq!(
            deserialized.bind_mounts[0].destination,
            Some("/vm/data".into())
        );
        assert!(deserialized.bind_mounts[0].read_only);
        // Credentials
        assert_eq!(deserialized.credentials.len(), 1);
        assert_eq!(
            deserialized.credentials[0].id,
            "passwd.hashed-password.root"
        );
        assert_eq!(deserialized.load_credentials.len(), 1);
        assert_eq!(
            deserialized.load_credentials[0].id,
            "ssh.authorized_keys.root"
        );
    }

    #[test]
    fn test_start_options_backward_compat() {
        // Deserializing an empty JSON object should produce valid defaults
        let opts: VMStartOptions = serde_json::from_str("{}").unwrap();
        assert!(opts.kvm.is_none());
        assert!(!opts.network_tap);
        assert!(!opts.quiet);
        assert!(opts.credentials.is_empty());
        assert!(opts.bind_mounts.is_empty());
        assert!(opts.console.is_none());
    }

    #[test]
    fn test_start_options_old_json_compat() {
        // Simulate JSON from a client that only knows the old fields
        let json = r#"{
            "kvm": true,
            "secure_boot": false,
            "vsock": true,
            "vsock_cid": 99,
            "directory": "/my/rootfs",
            "credentials": [{"id": "foo", "value": "bar"}]
        }"#;
        let opts: VMStartOptions = serde_json::from_str(json).unwrap();
        assert_eq!(opts.kvm, Some(true));
        assert_eq!(opts.secure_boot, Some(false));
        assert_eq!(opts.vsock, Some(true));
        assert_eq!(opts.vsock_cid, Some(99));
        assert_eq!(opts.directory, Some("/my/rootfs".into()));
        assert_eq!(opts.credentials.len(), 1);
        // New fields default gracefully
        assert!(opts.tpm.is_none());
        assert!(opts.linux.is_none());
        assert!(opts.initrd.is_empty());
        assert!(!opts.network_tap);
        assert!(opts.console.is_none());
    }

    #[test]
    fn test_start_options_optional_fields_omitted() {
        let opts = VMStartOptions::default();
        let json = serde_json::to_string(&opts).unwrap();
        // All Option::None and empty Vec fields should be omitted
        assert!(!json.contains("\"kvm\""));
        assert!(!json.contains("\"vsock\""));
        assert!(!json.contains("\"tpm\""));
        assert!(!json.contains("\"linux\""));
        assert!(!json.contains("\"initrd\""));
        assert!(!json.contains("\"uuid\""));
        assert!(!json.contains("\"slice\""));
        assert!(!json.contains("\"firmware\""));
        assert!(!json.contains("\"console\""));
        assert!(!json.contains("\"background\""));
        assert!(!json.contains("\"credentials\""));
        assert!(!json.contains("\"load_credentials\""));
        assert!(!json.contains("\"bind_mounts\""));
        assert!(!json.contains("\"extra_drives\""));
        assert!(!json.contains("\"bind_users\""));
        assert!(!json.contains("\"smbios11\""));
        assert!(!json.contains("\"properties\""));
        assert!(!json.contains("\"extra_args\""));
    }

    // ========================================================================
    // ConsoleMode tests
    // ========================================================================

    #[test]
    fn test_console_mode_serialization() {
        assert_eq!(
            serde_json::to_string(&ConsoleMode::Interactive).unwrap(),
            "\"interactive\""
        );
        assert_eq!(
            serde_json::to_string(&ConsoleMode::ReadOnly).unwrap(),
            "\"read-only\""
        );
        assert_eq!(
            serde_json::to_string(&ConsoleMode::Native).unwrap(),
            "\"native\""
        );
        assert_eq!(serde_json::to_string(&ConsoleMode::Gui).unwrap(), "\"gui\"");
    }

    #[test]
    fn test_console_mode_deserialization() {
        assert_eq!(
            serde_json::from_str::<ConsoleMode>("\"interactive\"").unwrap(),
            ConsoleMode::Interactive
        );
        assert_eq!(
            serde_json::from_str::<ConsoleMode>("\"read-only\"").unwrap(),
            ConsoleMode::ReadOnly
        );
        assert_eq!(
            serde_json::from_str::<ConsoleMode>("\"native\"").unwrap(),
            ConsoleMode::Native
        );
        assert_eq!(
            serde_json::from_str::<ConsoleMode>("\"gui\"").unwrap(),
            ConsoleMode::Gui
        );
    }

    // ========================================================================
    // BindMount tests
    // ========================================================================

    #[test]
    fn test_bind_mount_with_destination() {
        let bm = BindMount {
            source: "/host/path".into(),
            destination: Some("/vm/path".into()),
            read_only: false,
        };
        let json = serde_json::to_string(&bm).unwrap();
        assert!(json.contains("\"source\":\"/host/path\""));
        assert!(json.contains("\"destination\":\"/vm/path\""));
        let deserialized: BindMount = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.source, "/host/path");
        assert_eq!(deserialized.destination, Some("/vm/path".into()));
        assert!(!deserialized.read_only);
    }

    #[test]
    fn test_bind_mount_same_path() {
        let bm = BindMount {
            source: "/shared".into(),
            destination: None,
            read_only: true,
        };
        let json = serde_json::to_string(&bm).unwrap();
        // destination should be omitted when None
        assert!(!json.contains("\"destination\""));
        assert!(json.contains("\"read_only\":true"));
    }

    // ========================================================================
    // ManagerScope tests
    // ========================================================================

    #[test]
    fn test_manager_scope_serialization() {
        assert_eq!(
            serde_json::to_string(&ManagerScope::System).unwrap(),
            "\"system\""
        );
        assert_eq!(
            serde_json::to_string(&ManagerScope::User).unwrap(),
            "\"user\""
        );
    }

    #[test]
    fn test_manager_scope_deserialization() {
        assert_eq!(
            serde_json::from_str::<ManagerScope>("\"system\"").unwrap(),
            ManagerScope::System
        );
        assert_eq!(
            serde_json::from_str::<ManagerScope>("\"user\"").unwrap(),
            ManagerScope::User
        );
    }

    #[test]
    fn test_start_options_with_scope() {
        let json = r#"{"scope": "user"}"#;
        let opts: VMStartOptions = serde_json::from_str(json).unwrap();
        assert_eq!(opts.scope, Some(ManagerScope::User));

        let json = r#"{"scope": "system"}"#;
        let opts: VMStartOptions = serde_json::from_str(json).unwrap();
        assert_eq!(opts.scope, Some(ManagerScope::System));

        // Omitted scope defaults to None
        let json = r#"{}"#;
        let opts: VMStartOptions = serde_json::from_str(json).unwrap();
        assert!(opts.scope.is_none());
    }

    #[test]
    fn test_scope_omitted_in_default_serialization() {
        let opts = VMStartOptions::default();
        let json = serde_json::to_string(&opts).unwrap();
        assert!(!json.contains("\"scope\""));
    }

    // ========================================================================
    // VMCredential tests
    // ========================================================================

    #[test]
    fn test_credential_roundtrip() {
        let cred = VMCredential {
            id: "passwd.hashed-password.root".into(),
            value: "$y$j9T$salt$hash".into(),
        };
        let json = serde_json::to_string(&cred).unwrap();
        let deserialized: VMCredential = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized, cred);
    }

    #[test]
    fn test_load_credential_roundtrip() {
        let cred = LoadCredential {
            id: "ssh.authorized_keys.root".into(),
            path: "/root/.ssh/authorized_keys".into(),
        };
        let json = serde_json::to_string(&cred).unwrap();
        let deserialized: LoadCredential = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized, cred);
    }

    #[test]
    fn test_load_credential_backward_compat() {
        // Old JSON used "value" instead of "path"
        let json = r#"{"id": "test", "value": "/some/path"}"#;
        let cred: LoadCredential = serde_json::from_str(json).unwrap();
        assert_eq!(cred.id, "test");
        assert_eq!(cred.path, "/some/path");
    }

    #[test]
    fn test_ssh_key_type_serialization() {
        assert_eq!(
            serde_json::to_string(&SshKeyType::Ed25519).unwrap(),
            "\"ed25519\""
        );
        assert_eq!(
            serde_json::to_string(&SshKeyType::Ecdsa).unwrap(),
            "\"ecdsa\""
        );
        assert_eq!(serde_json::to_string(&SshKeyType::Rsa).unwrap(), "\"rsa\"");
    }

    #[test]
    fn test_ssh_key_type_deserialization() {
        assert_eq!(
            serde_json::from_str::<SshKeyType>("\"ed25519\"").unwrap(),
            SshKeyType::Ed25519
        );
        assert_eq!(
            serde_json::from_str::<SshKeyType>("\"ecdsa\"").unwrap(),
            SshKeyType::Ecdsa
        );
        assert_eq!(
            serde_json::from_str::<SshKeyType>("\"rsa\"").unwrap(),
            SshKeyType::Rsa
        );
    }

    #[test]
    fn test_console_mode_default() {
        assert_eq!(ConsoleMode::default(), ConsoleMode::Interactive);
    }

    // ========================================================================
    // Validation tests
    // ========================================================================

    #[test]
    fn test_validate_default_options() {
        let opts = VMStartOptions::default();
        assert!(opts.validate().is_ok());
    }

    #[test]
    fn test_validate_uuid_format() {
        let mut opts = VMStartOptions {
            uuid: Some("550e8400-e29b-41d4-a716-446655440000".into()),
            ..Default::default()
        };
        assert!(opts.validate().is_ok());

        opts.uuid = Some("not-a-uuid".into());
        assert!(opts.validate().is_err());
    }

    #[test]
    fn test_validate_credential_id_no_colon() {
        let mut opts = VMStartOptions {
            credentials: vec![VMCredential {
                id: "valid.credential-id".into(),
                value: "test".into(),
            }],
            ..Default::default()
        };
        assert!(opts.validate().is_ok());

        opts.credentials = vec![VMCredential {
            id: "invalid:id".into(),
            value: "test".into(),
        }];
        assert!(opts.validate().is_err());
    }

    #[test]
    fn test_validate_extra_args_no_flags() {
        let mut opts = VMStartOptions {
            extra_args: vec!["enforcing=0".into()],
            ..Default::default()
        };
        assert!(opts.validate().is_ok());

        opts.extra_args = vec!["--image=/etc/shadow".into()];
        assert!(opts.validate().is_err());
    }

    #[test]
    fn test_validate_properties_allowlist() {
        let mut opts = VMStartOptions {
            properties: vec!["MemoryMax=4G".into(), "CPUQuota=200%".into()],
            ..Default::default()
        };
        assert!(opts.validate().is_ok());

        opts.properties = vec!["ExecStartPost=/bin/bash".into()];
        assert!(opts.validate().is_err());
    }

    #[test]
    fn test_validate_smbios11_no_credential_override() {
        let mut opts = VMStartOptions {
            smbios11: vec!["custom.string=hello".into()],
            ..Default::default()
        };
        assert!(opts.validate().is_ok());

        opts.smbios11 = vec!["io.systemd.credential:secret=pwned".into()];
        assert!(opts.validate().is_err());

        // Also block io.systemd.credential.binary: prefix
        opts.smbios11 = vec!["io.systemd.credential.binary:secret=base64data".into()];
        assert!(opts.validate().is_err());
    }

    #[test]
    fn test_validate_bind_users_no_root() {
        let mut opts = VMStartOptions {
            bind_users: vec!["testuser".into()],
            ..Default::default()
        };
        assert!(opts.validate().is_ok());

        opts.bind_users = vec!["root".into()];
        assert!(opts.validate().is_err());
    }

    #[test]
    fn test_validate_slice_format() {
        let mut opts = VMStartOptions {
            slice: Some("vm.slice".into()),
            ..Default::default()
        };
        assert!(opts.validate().is_ok());

        opts.slice = Some("bad slice name".into());
        assert!(opts.validate().is_err());

        opts.slice = Some("no-suffix".into());
        assert!(opts.validate().is_err());
    }

    #[test]
    fn test_validate_grow_image_format() {
        let mut opts = VMStartOptions {
            grow_image: Some("50G".into()),
            ..Default::default()
        };
        assert!(opts.validate().is_ok());

        opts.grow_image = Some("".into());
        assert!(opts.validate().is_err());

        opts.grow_image = Some("not-a-size".into());
        assert!(opts.validate().is_err());
    }

    #[test]
    fn test_validate_path_traversal() {
        let mut opts = VMStartOptions {
            directory: Some("/safe/path".into()),
            ..Default::default()
        };
        assert!(opts.validate().is_ok());

        opts.directory = Some("/unsafe/../etc/shadow".into());
        assert!(opts.validate().is_err());
    }

    #[test]
    fn test_validate_bind_mount_traversal() {
        let mut opts = VMStartOptions {
            bind_mounts: vec![BindMount {
                source: "/safe/path".into(),
                destination: Some("/vm/path".into()),
                read_only: false,
            }],
            ..Default::default()
        };
        assert!(opts.validate().is_ok());

        opts.bind_mounts = vec![BindMount {
            source: "/unsafe/../../etc".into(),
            destination: None,
            read_only: false,
        }];
        assert!(opts.validate().is_err());
    }

    #[test]
    fn test_validate_load_credential_path_traversal() {
        let mut opts = VMStartOptions {
            load_credentials: vec![LoadCredential {
                id: "test".into(),
                path: "/safe/path".into(),
            }],
            ..Default::default()
        };
        assert!(opts.validate().is_ok());

        opts.load_credentials = vec![LoadCredential {
            id: "test".into(),
            path: "/unsafe/../etc/shadow".into(),
        }];
        assert!(opts.validate().is_err());
    }

    #[test]
    fn test_validate_bind_user_shell() {
        let mut opts = VMStartOptions {
            bind_user_shell: Some("/bin/bash".into()),
            ..Default::default()
        };
        assert!(opts.validate().is_ok());

        opts.bind_user_shell = Some("true".into());
        assert!(opts.validate().is_ok());

        opts.bind_user_shell = Some("../../bin/sh".into());
        assert!(opts.validate().is_err());

        opts.bind_user_shell = Some("not-a-path".into());
        assert!(opts.validate().is_err());
    }

    #[test]
    fn test_validate_background_ansi_sgr() {
        let mut opts = VMStartOptions {
            background: Some("44".into()),
            ..Default::default()
        };
        assert!(opts.validate().is_ok());

        opts.background = Some("38;5;200".into());
        assert!(opts.validate().is_ok());

        opts.background = Some("evil\x1b[0m".into());
        assert!(opts.validate().is_err());
    }

    #[test]
    fn test_validate_private_users_format() {
        let mut opts = VMStartOptions {
            private_users: Some("1000:65536".into()),
            ..Default::default()
        };
        assert!(opts.validate().is_ok());

        opts.private_users = Some("yes".into());
        assert!(opts.validate().is_ok());

        opts.private_users = Some("pick".into());
        assert!(opts.validate().is_ok());

        opts.private_users = Some("garbage input".into());
        assert!(opts.validate().is_err());
    }

    #[test]
    fn test_validate_bind_users_system_accounts() {
        let mut opts = VMStartOptions {
            bind_users: vec!["daemon".into()],
            ..Default::default()
        };
        assert!(opts.validate().is_err());

        opts.bind_users = vec!["0".into()];
        assert!(opts.validate().is_err());

        opts.bind_users = vec!["999".into()];
        assert!(opts.validate().is_err());

        opts.bind_users = vec!["1000".into()];
        assert!(opts.validate().is_ok());
    }

    #[test]
    fn test_validate_bind_user_groups() {
        let mut opts = VMStartOptions {
            bind_user_groups: vec!["wheel".into()],
            ..Default::default()
        };
        assert!(opts.validate().is_ok());

        opts.bind_user_groups = vec!["".into()];
        assert!(opts.validate().is_err());

        opts.bind_user_groups = vec!["bad group".into()];
        assert!(opts.validate().is_err());
    }

    #[test]
    fn test_validate_credential_value_length() {
        let opts = VMStartOptions {
            credentials: vec![VMCredential {
                id: "test".into(),
                value: "a".repeat(65537),
            }],
            ..Default::default()
        };
        assert!(opts.validate().is_err());
    }

    #[test]
    fn test_validate_size_string_suffix() {
        // Valid suffixes
        assert!(is_valid_size_string("50G"));
        assert!(is_valid_size_string("100M"));
        assert!(is_valid_size_string("1T"));
        assert!(is_valid_size_string("500"));

        // Invalid suffixes
        assert!(!is_valid_size_string("50XYZ"));
        assert!(!is_valid_size_string("50GB"));
        assert!(!is_valid_size_string(""));
    }

    #[test]
    fn test_validate_properties_no_dangerous() {
        // DeviceAllow and Delegate should now be rejected
        let mut opts = VMStartOptions {
            properties: vec!["DeviceAllow=block-* rwm".into()],
            ..Default::default()
        };
        assert!(opts.validate().is_err());

        opts.properties = vec!["Delegate=yes".into()];
        assert!(opts.validate().is_err());
    }

    #[test]
    fn test_validate_extra_args_control_chars() {
        let mut opts = VMStartOptions {
            extra_args: vec!["enforcing=0".into()],
            ..Default::default()
        };
        assert!(opts.validate().is_ok());

        opts.extra_args = vec!["bad\x00value".into()];
        assert!(opts.validate().is_err());

        opts.extra_args = vec!["bad\nvalue".into()];
        assert!(opts.validate().is_err());
    }

    #[test]
    fn test_validate_load_credential_invalid_id() {
        let mut opts = VMStartOptions {
            load_credentials: vec![LoadCredential {
                id: "valid.id".into(),
                path: "/some/path".into(),
            }],
            ..Default::default()
        };
        assert!(opts.validate().is_ok());

        opts.load_credentials = vec![LoadCredential {
            id: "invalid:id".into(),
            path: "/some/path".into(),
        }];
        assert!(opts.validate().is_err());

        opts.load_credentials = vec![LoadCredential {
            id: "invalid/id".into(),
            path: "/some/path".into(),
        }];
        assert!(opts.validate().is_err());
    }

    #[test]
    fn test_validate_smbios11_control_chars() {
        let mut opts = VMStartOptions {
            smbios11: vec!["valid.string=hello".into()],
            ..Default::default()
        };
        assert!(opts.validate().is_ok());

        opts.smbios11 = vec!["bad\x00string".into()];
        assert!(opts.validate().is_err());
    }

    #[test]
    fn test_validate_bind_users_charset() {
        let mut opts = VMStartOptions {
            bind_users: vec!["valid-user".into()],
            ..Default::default()
        };
        assert!(opts.validate().is_ok());

        opts.bind_users = vec!["bad user".into()];
        assert!(opts.validate().is_err());

        opts.bind_users = vec!["bad;user".into()];
        assert!(opts.validate().is_err());

        opts.bind_users = vec!["".into()];
        assert!(opts.validate().is_err());
    }

    #[test]
    fn test_display_impls() {
        assert_eq!(ConsoleMode::Interactive.to_string(), "interactive");
        assert_eq!(ConsoleMode::ReadOnly.to_string(), "read-only");
        assert_eq!(ConsoleMode::Native.to_string(), "native");
        assert_eq!(ConsoleMode::Gui.to_string(), "gui");
        assert_eq!(SshKeyType::Ed25519.to_string(), "ed25519");
        assert_eq!(SshKeyType::Ecdsa.to_string(), "ecdsa");
        assert_eq!(SshKeyType::Rsa.to_string(), "rsa");
        assert_eq!(ManagerScope::System.to_string(), "system");
        assert_eq!(ManagerScope::User.to_string(), "user");
    }
}
