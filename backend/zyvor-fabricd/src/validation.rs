// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: LicenseRef-Zyvor-Production-2.0

use axum::http::StatusCode;

/// Validate a VM name to prevent command injection.
/// Allows alphanumeric characters, dots, hyphens, and underscores.
/// Must start with an alphanumeric character and be 1-64 characters long.
pub fn validate_vm_name(name: &str) -> Result<(), (StatusCode, String)> {
    if name.is_empty() || name.len() > 64 {
        return Err((
            StatusCode::BAD_REQUEST,
            "VM name must be between 1 and 64 characters".to_string(),
        ));
    }

    // Safety: name is non-empty (checked above), so .next() always returns Some
    let first = match name.chars().next() {
        Some(c) => c,
        None => {
            return Err((
                StatusCode::BAD_REQUEST,
                "VM name must not be empty".to_string(),
            ));
        }
    };
    if !first.is_ascii_alphanumeric() {
        return Err((
            StatusCode::BAD_REQUEST,
            "VM name must start with an alphanumeric character".to_string(),
        ));
    }

    if !name
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '.' || c == '-' || c == '_')
    {
        return Err((
            StatusCode::BAD_REQUEST,
            "VM name may only contain alphanumeric characters, dots, hyphens, and underscores"
                .to_string(),
        ));
    }

    Ok(())
}

/// Validate an entity name (datacenter, cluster, resource pool, etc.).
/// Allows alphanumeric characters, hyphens, underscores, dots, and spaces.
/// Must start with an alphanumeric character. 1-128 characters.
pub fn validate_entity_name(name: &str) -> Result<(), (StatusCode, String)> {
    if name.is_empty() || name.len() > 128 {
        return Err((
            StatusCode::BAD_REQUEST,
            "Entity name must be between 1 and 128 characters".to_string(),
        ));
    }
    if let Some(c) = name.chars().next() {
        if !c.is_ascii_alphanumeric() {
            return Err((
                StatusCode::BAD_REQUEST,
                "Entity name must start with an alphanumeric character".to_string(),
            ));
        }
    }
    if !name
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.' | ' '))
    {
        return Err((StatusCode::BAD_REQUEST, "Entity name may only contain alphanumeric characters, hyphens, underscores, dots, and spaces".to_string()));
    }
    Ok(())
}

/// Validate a network device name (Linux IFNAMSIZ max 15 chars, no dots).
pub fn validate_device_name(name: &str) -> Result<(), (StatusCode, String)> {
    if name.is_empty() || name.len() > 15 {
        return Err((
            StatusCode::BAD_REQUEST,
            "Device name must be between 1 and 15 characters".to_string(),
        ));
    }
    if let Some(c) = name.chars().next() {
        if !c.is_ascii_alphanumeric() {
            return Err((
                StatusCode::BAD_REQUEST,
                "Device name must start with an alphanumeric character".to_string(),
            ));
        }
    }
    if !name
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_'))
    {
        return Err((
            StatusCode::BAD_REQUEST,
            "Device name may only contain alphanumeric characters, hyphens, and underscores"
                .to_string(),
        ));
    }
    Ok(())
}

// === Serde default helpers ===

/// Serde default for boolean true. Used across multiple API modules.
pub fn default_true() -> bool {
    true
}

/// Serde default for retention period (30 days).
pub fn default_retention() -> u32 {
    30
}

// === VM image path helpers ===

/// Allowed base directories for VM images.
const IMAGE_ALLOWED_PREFIXES: &[&str] = &[
    "/var/lib/machines",
    "/var/lib/zyvor-fabricd/images",
    "/var/lib/fluxvm/images",
];

/// Find the disk image path for a VM by checking common locations.
/// Returns the path if found, or None if not.
/// The returned path is validated to be under allowed directories.
///
/// NOTE: this only guesses by naming convention (does it exist at
/// "<dir>/<vm name>.<ext>"?) -- it never consults the VM's actual stored
/// disk path, so it misses any VM created from an image not named after
/// the VM itself (e.g. multiple VMs cloned from the same base image).
/// Every real call site (snapshots, backups, checkpoints, cloning,
/// forking, resize, storage migration) has been switched to
/// `state.driver.get_disk_path(name)` instead, which resolves the VM's
/// actual live disk through FluxVM. What's left here is only the
/// fallback path for a target name FluxVM doesn't know about yet
/// (backup restore to a new VM, hibernate-resume default) -- a genuine
/// "no live disk to query, so guess the default location" case, not a
/// bug.
pub fn find_vm_image(name: &str) -> Option<String> {
    let candidates = [
        format!("/var/lib/machines/{}.qcow2", name),
        format!("/var/lib/machines/{}.raw", name),
        format!("/var/lib/machines/{}/{}.qcow2", name, name),
        format!("/var/lib/zyvor-fabricd/images/{}.qcow2", name),
        format!("/var/lib/zyvor-fabricd/images/{}.raw", name),
        format!("/var/lib/fluxvm/images/{}.qcow2", name),
        format!("/var/lib/fluxvm/images/{}.raw", name),
    ];

    for path in &candidates {
        let p = std::path::Path::new(path);
        if p.exists() {
            // Canonicalize to resolve symlinks and verify the real path
            // is under an allowed prefix
            let resolved = match std::fs::canonicalize(p) {
                Ok(r) => r,
                Err(_) => continue,
            };
            let resolved_str = resolved.to_string_lossy();
            if IMAGE_ALLOWED_PREFIXES
                .iter()
                .any(|prefix| resolved_str.starts_with(prefix))
            {
                return Some(resolved_str.to_string());
            }
        }
    }

    None
}

/// Find the disk image path for a VM, returning a default path if not found.
pub fn find_vm_image_or_default(name: &str) -> String {
    find_vm_image(name).unwrap_or_else(|| format!("/var/lib/zyvor-fabricd/images/{}.qcow2", name))
}

// === Input validation helpers ===

/// Validate that a string is a valid hostname or IP address.
/// Rejects shell metacharacters and other injection vectors.
/// Validate a PCI Bus:Device.Function address (e.g. "0000:01:00.0") before
/// it's used to build a sysfs path. Without this, a value like "/etc/passwd"
/// would make `Path::new("/sys/bus/pci/devices").join(addr)` discard the
/// base entirely (Rust's `Path::join` replaces on an absolute joinee) --
/// harmless in `attach_pci` today only because the vfio-pci driver check
/// after it rejects anything that isn't already passthrough-bound, but that
/// shouldn't be the only thing standing in the way.
pub fn validate_pci_address(addr: &str) -> Result<(), (StatusCode, String)> {
    let parts: Vec<&str> = addr.split(':').collect();
    let valid = match parts.as_slice() {
        [domain, bus, rest] if domain.len() == 4 => match rest.split_once('.') {
            Some((dev, func)) => {
                dev.len() == 2
                    && func.len() == 1
                    && [*domain, *bus, dev, func]
                        .iter()
                        .all(|s| s.chars().all(|c| c.is_ascii_hexdigit()))
            }
            None => false,
        },
        _ => false,
    };
    if !valid {
        return Err((
            StatusCode::BAD_REQUEST,
            format!(
                "'{}' is not a valid PCI address (expected dddd:bb:dd.f, e.g. 0000:01:00.0)",
                addr
            ),
        ));
    }
    Ok(())
}

pub fn validate_hostname(host: &str) -> Result<(), String> {
    if host.is_empty() || host.len() > 253 {
        return Err("Hostname must be between 1 and 253 characters".to_string());
    }

    let valid = host
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '.' || c == '-' || c == ':' || c == '_');

    if !valid {
        return Err(format!(
            "Hostname '{}' contains invalid characters. Only alphanumeric, dots, hyphens, underscores, and colons are allowed.",
            host
        ));
    }

    if host.starts_with('-') {
        return Err("Hostname must not start with a hyphen".to_string());
    }

    Ok(())
}

/// Validate that a string is a valid IPv4 or IPv6 address.
pub fn validate_ip_address(addr: &str) -> Result<(), String> {
    addr.parse::<std::net::IpAddr>()
        .map(|_| ())
        .map_err(|_| format!("Invalid IP address: '{}'", addr))
}

/// Validate that a host path is within allowed directories.
/// Prevents arbitrary file access by restricting to safe prefixes.
/// Canonicalizes the path to resolve symlinks and prevent traversal.
pub fn validate_host_path(path: &str) -> Result<(), (StatusCode, String)> {
    let raw = std::path::Path::new(path);

    // Reject .. components before canonicalization
    for component in raw.components() {
        if let std::path::Component::ParentDir = component {
            return Err((
                StatusCode::BAD_REQUEST,
                "Path must not contain '..' components".to_string(),
            ));
        }
    }

    let allowed_prefixes = [
        "/var/lib/machines",
        "/var/lib/zyvor-fabricd",
        "/var/lib/fluxvm",
    ];

    // Try to canonicalize to resolve symlinks. If the file doesn't exist yet,
    // canonicalize the parent directory to prevent symlink-based traversal.
    let resolved = match std::fs::canonicalize(raw) {
        Ok(r) => r,
        Err(_) => {
            // File doesn't exist yet -- canonicalize parent directory
            if let Some(parent) = raw.parent() {
                let resolved_parent = std::fs::canonicalize(parent).map_err(|_| {
                    (
                        StatusCode::BAD_REQUEST,
                        "Parent directory does not exist".to_string(),
                    )
                })?;
                resolved_parent.join(raw.file_name().unwrap_or_default())
            } else {
                raw.to_path_buf()
            }
        }
    };
    let resolved_str = resolved.to_string_lossy();

    if !allowed_prefixes
        .iter()
        .any(|prefix| resolved_str.starts_with(prefix))
    {
        return Err((
            StatusCode::FORBIDDEN,
            format!(
                "Host path must be under one of: {}",
                allowed_prefixes.join(", ")
            ),
        ));
    }

    Ok(())
}

/// Validate a path inside a machine/container.
/// Must be absolute and must not contain path traversal sequences.
pub fn validate_machine_path(path: &str) -> Result<(), (StatusCode, String)> {
    if path.is_empty() {
        return Err((
            StatusCode::BAD_REQUEST,
            "Machine path must not be empty".to_string(),
        ));
    }
    if !path.starts_with('/') {
        return Err((
            StatusCode::BAD_REQUEST,
            "Machine path must be absolute".to_string(),
        ));
    }
    for component in std::path::Path::new(path).components() {
        if let std::path::Component::ParentDir = component {
            return Err((
                StatusCode::BAD_REQUEST,
                "Machine path must not contain '..' components".to_string(),
            ));
        }
    }
    if path.contains('\0') {
        return Err((
            StatusCode::BAD_REQUEST,
            "Machine path must not contain null bytes".to_string(),
        ));
    }
    Ok(())
}

/// Escape a string for safe CSV output. Prevents CSV injection.
pub fn escape_csv_field(field: &str) -> String {
    // A leading tab or carriage return is also a spreadsheet formula vector.
    let is_formula = field.starts_with(['=', '+', '-', '@', '\t', '\r']);
    let needs_quoting = is_formula || field.contains([',', '\n', '\r', '"']);

    let mut escaped = if is_formula {
        format!("'{}", field)
    } else {
        field.to_string()
    };

    if needs_quoting {
        escaped = format!("\"{}\"", escaped.replace('"', "\"\""));
    }

    escaped
}

/// Validate a snapshot or checkpoint name.
/// Allows alphanumeric characters, hyphens, underscores, and dots.
/// Must not start with a hyphen (prevents argument injection).
pub fn validate_snapshot_name(name: &str) -> Result<(), (StatusCode, String)> {
    if name.is_empty() || name.len() > 64 {
        return Err((
            StatusCode::BAD_REQUEST,
            "Snapshot name must be between 1 and 64 characters".to_string(),
        ));
    }
    if name.starts_with('-') {
        return Err((
            StatusCode::BAD_REQUEST,
            "Snapshot name must not start with a hyphen".to_string(),
        ));
    }
    if !name
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'))
    {
        return Err((
            StatusCode::BAD_REQUEST,
            "Snapshot name may only contain alphanumeric characters, hyphens, underscores, and dots".to_string(),
        ));
    }
    Ok(())
}

/// Validate a CIDR notation string (e.g., "10.0.0.0/8", "192.168.1.0/24").
pub fn validate_cidr(cidr: &str) -> Result<(), String> {
    let parts: Vec<&str> = cidr.split('/').collect();
    if parts.len() != 2 {
        return Err("CIDR must be in format IP/prefix".to_string());
    }
    parts[0]
        .parse::<std::net::IpAddr>()
        .map_err(|_| format!("Invalid IP address in CIDR: '{}'", parts[0]))?;
    let prefix: u8 = parts[1]
        .parse()
        .map_err(|_| format!("Invalid prefix length: '{}'", parts[1]))?;
    let is_v4 = parts[0].parse::<std::net::Ipv4Addr>().is_ok();
    if is_v4 && prefix > 32 {
        return Err("IPv4 prefix length must be <= 32".to_string());
    }
    if !is_v4 && prefix > 128 {
        return Err("IPv6 prefix length must be <= 128".to_string());
    }
    Ok(())
}

/// Validate a log prefix for nftables rules.
/// Allows alphanumeric characters, hyphens, underscores, colons, dots, and spaces.
pub fn validate_log_prefix(prefix: &str) -> Result<(), String> {
    if prefix.is_empty() || prefix.len() > 64 {
        return Err("Log prefix must be between 1 and 64 characters".to_string());
    }
    if !prefix
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | ':' | '.' | ' '))
    {
        return Err("Log prefix may only contain alphanumeric characters, hyphens, underscores, colons, dots, and spaces".to_string());
    }
    Ok(())
}

/// Allowed disk image formats for qemu-img operations.
pub const ALLOWED_IMAGE_FORMATS: &[&str] = &["qcow2", "raw", "vmdk", "vdi", "vhd", "vhdx", "qed"];

/// Validate a disk image format against the allowlist.
pub fn validate_image_format(format: &str) -> Result<(), (StatusCode, serde_json::Value)> {
    if !ALLOWED_IMAGE_FORMATS.contains(&format) {
        return Err((
            StatusCode::BAD_REQUEST,
            serde_json::json!({
                "error": std::format!("Invalid image format '{}'. Allowed: {}", format, ALLOWED_IMAGE_FORMATS.join(", "))
            }),
        ));
    }
    Ok(())
}

/// Sanitize an error message by redacting filesystem paths.
/// Replaces absolute paths like /var/lib/zyvor-fabricd/images/foo.qcow2 with <path>.
/// Use for error messages returned to non-admin users.
pub fn sanitize_error(msg: &str) -> String {
    // Match absolute paths: / followed by path characters with at least one more /
    let mut result = msg.to_string();
    let mut i = 0;
    while i < result.len() {
        if result.as_bytes()[i] == b'/' {
            // Check if this looks like a path (has at least one more segment)
            let start = i;
            let mut j = i + 1;
            let bytes = result.as_bytes();
            while j < bytes.len()
                && (bytes[j].is_ascii_alphanumeric()
                    || matches!(bytes[j], b'/' | b'.' | b'-' | b'_'))
            {
                j += 1;
            }
            // Only redact if it looks like a real path (has at least one /)
            if j - start > 2 && result[start..j].contains('/') && result[start + 1..j].contains('/')
            {
                result.replace_range(start..j, "<path>");
                i = start + 6; // length of "<path>"
                continue;
            }
        }
        i += 1;
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_valid_vm_names() {
        assert!(validate_vm_name("web-server-01").is_ok());
        assert!(validate_vm_name("db.prod.01").is_ok());
        assert!(validate_vm_name("test_vm").is_ok());
        assert!(validate_vm_name("a").is_ok());
        assert!(validate_vm_name("A123").is_ok());
    }

    #[test]
    fn test_invalid_vm_names() {
        assert!(validate_vm_name("").is_err());
        assert!(validate_vm_name("-starts-with-dash").is_err());
        assert!(validate_vm_name(".starts-with-dot").is_err());
        assert!(validate_vm_name("_starts-with-underscore").is_err());
        assert!(validate_vm_name("has spaces").is_err());
        assert!(validate_vm_name("test;echo pwned").is_err());
        assert!(validate_vm_name("$(whoami)").is_err());
        assert!(validate_vm_name("vm`id`").is_err());
        assert!(validate_vm_name("vm|cat /etc/passwd").is_err());
        assert!(validate_vm_name(&"a".repeat(65)).is_err());
    }

    #[test]
    fn test_default_true() {
        assert!(default_true());
    }

    #[test]
    fn test_validate_hostname() {
        assert!(validate_hostname("example.com").is_ok());
        assert!(validate_hostname("192.168.1.1").is_ok());
        assert!(validate_hostname("::1").is_ok());
        assert!(validate_hostname("-bad").is_err());
        assert!(validate_hostname("").is_err());
        assert!(validate_hostname("bad;host").is_err());
    }

    #[test]
    fn test_validate_ip_address() {
        assert!(validate_ip_address("192.168.1.1").is_ok());
        assert!(validate_ip_address("::1").is_ok());
        assert!(validate_ip_address("not-an-ip").is_err());
    }

    #[test]
    fn test_escape_csv_field() {
        assert_eq!(escape_csv_field("hello"), "hello");
        assert_eq!(escape_csv_field("hello,world"), "\"hello,world\"");
        assert_eq!(escape_csv_field("=SUM(A1)"), "\"'=SUM(A1)\"");
        assert_eq!(escape_csv_field("\t=1"), "\"'\t=1\"");
        assert_eq!(escape_csv_field("a\rb"), "\"a\rb\"");
    }
}
