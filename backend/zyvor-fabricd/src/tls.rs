// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: Apache-2.0

//! Native TLS for the daemon's own listening socket — not a reverse-proxy
//! config. Works identically bare-metal, in a VM, or in a Kubernetes pod: a
//! real cert (e.g. from cert-manager) just gets mounted at the configured
//! `cert_path`/`key_path`; nothing here depends on a specific ingress
//! controller or sidecar. If no cert exists at those paths, one is
//! generated automatically on first start so HTTPS works with zero manual
//! setup.

use anyhow::{Context, Result};
use std::path::Path;

/// Best-effort discovery of this host's outbound IP, for the self-signed
/// cert's SAN list — doesn't actually send any packets (UDP `connect`
/// only resolves a route), so this works even fully offline, just less
/// usefully (falls back silently if it fails).
fn detect_local_ip() -> Option<String> {
    let socket = std::net::UdpSocket::bind("0.0.0.0:0").ok()?;
    socket.connect("8.8.8.8:80").ok()?;
    socket.local_addr().ok().map(|a| a.ip().to_string())
}

/// Generates a self-signed cert/key pair at `cert_path`/`key_path` if
/// either is missing. Leaves an existing cert/key alone — this only ever
/// fills a gap, never overwrites a real cert an operator (or
/// cert-manager) put there.
pub fn ensure_self_signed_cert(cert_path: &str, key_path: &str) -> Result<()> {
    let cert_path = Path::new(cert_path);
    let key_path = Path::new(key_path);

    if cert_path.exists() && key_path.exists() {
        return Ok(());
    }

    tracing::info!(
        cert = %cert_path.display(),
        key = %key_path.display(),
        "no TLS cert found, generating a self-signed one"
    );

    let mut sans = vec![
        "localhost".to_string(),
        "127.0.0.1".to_string(),
        "::1".to_string(),
    ];
    if let Ok(hostname) = std::process::Command::new("hostname").output() {
        if let Ok(name) = String::from_utf8(hostname.stdout) {
            let name = name.trim();
            if !name.is_empty() {
                sans.push(name.to_string());
            }
        }
    }
    if let Some(ip) = detect_local_ip() {
        sans.push(ip);
    }

    let cert_key =
        rcgen::generate_simple_self_signed(sans).context("generating self-signed certificate")?;

    if let Some(parent) = cert_path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("creating {}", parent.display()))?;
    }
    if let Some(parent) = key_path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("creating {}", parent.display()))?;
    }

    std::fs::write(cert_path, cert_key.cert.pem())
        .with_context(|| format!("writing {}", cert_path.display()))?;
    encryption::write_private_file(key_path, cert_key.signing_key.serialize_pem())
        .with_context(|| format!("writing {}", key_path.display()))?;

    // Private key: owner read/write only.
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(key_path, std::fs::Permissions::from_mode(0o600))
            .with_context(|| format!("setting permissions on {}", key_path.display()))?;
    }

    tracing::info!("self-signed TLS certificate generated");
    Ok(())
}
