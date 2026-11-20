// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: LicenseRef-Zyvor-Production-1.0

mod daemon;

use anyhow::Result;
use daemon::Daemon;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

#[tokio::main]
async fn main() -> Result<()> {
    // rustls 0.23 requires an explicit process-level CryptoProvider when more
    // than one backend (aws-lc-rs via jsonwebtoken, ring via axum-server TLS)
    // is linked. Install aws-lc-rs before any TLS acceptor is constructed.
    rustls::crypto::aws_lc_rs::default_provider()
        .install_default()
        .expect("failed to install rustls aws-lc-rs CryptoProvider");

    // Initialize tracing with ZYVOR_FABRICD_LOG_LEVEL or RUST_LOG support
    // Priority: ZYVOR_FABRICD_LOG_LEVEL > RUST_LOG > default (info)
    //
    // The directive used to be scoped to just `zyvor_fabricd=` (+
    // `tower_http=`) -- since the workspace's dozens of other crates
    // (networking, dns-policy, vm-firewall, packet-mirror, ...) all live
    // under their own crate names, not `zyvor_fabricd::`, EVERY
    // tracing::info!/warn!/error! call in any of them was silently
    // dropped in production. Found live: a `tracing::info!` added to
    // networking::lib.rs to debug a bridge STP issue never appeared in
    // `journalctl -u zyvor-fabricd` at all, at any level. Default to
    // `{level}` with no target prefix (applies workspace-wide, matching
    // every first-party crate without having to enumerate them), and
    // dial down the known-noisiest transport/runtime dependencies
    // explicitly rather than trying to allowlist every first-party crate
    // by name.
    let noisy_deps =
        "h2=warn,hyper=warn,hyper_util=warn,tokio_util=warn,rustls=warn,want=warn,mio=warn";
    let env_filter = if let Ok(level) = std::env::var("ZYVOR_FABRICD_LOG_LEVEL") {
        tracing_subscriber::EnvFilter::new(format!("{level},{noisy_deps}"))
    } else {
        tracing_subscriber::EnvFilter::try_from_default_env()
            .unwrap_or_else(|_| format!("info,{noisy_deps}").into())
    };

    tracing_subscriber::registry()
        .with(env_filter)
        .with(tracing_subscriber::fmt::layer())
        .init();

    tracing::info!("Starting zyvor-fabricd");
    tracing::info!("Press Ctrl+C to quit");
    tracing::debug!("Debug logging enabled");

    let daemon = Daemon::new()?;
    daemon.start().await?;

    Ok(())
}
