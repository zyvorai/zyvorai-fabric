// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: LicenseRef-Zyvor-Production-1.0

mod controller;
mod crd;
mod error;
mod reconcile;

use anyhow::Result;
use kube::Client;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "zyvor_fabricd_operator=info".into()),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();

    tracing::info!(
        "Starting zyvor-fabricd Kubernetes operator; AI kinds: {}",
        crate::crd::production_ai_kinds().join(", ")
    );

    // kube and reqwest pull in rustls with both the `ring` and `aws-lc-rs`
    // crypto-provider backends available transitively, so rustls can't pick
    // one on its own and panics on the first TLS handshake unless a process
    // default is installed explicitly.
    rustls::crypto::ring::default_provider()
        .install_default()
        .expect("no other rustls CryptoProvider installed yet");

    let client = Client::try_default().await?;

    controller::run(client).await?;

    Ok(())
}
