// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: LicenseRef-Zyvor-Production-1.0

mod cli;
mod help;
mod packetflow;
mod style;

use anyhow::Result;
use clap::FromArgMatches;
use cli::Cli;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize tracing with ZYVOR_FABRICD_LOG_LEVEL or RUST_LOG support
    let env_filter = if let Ok(level) = std::env::var("ZYVOR_FABRICD_LOG_LEVEL") {
        tracing_subscriber::EnvFilter::new(format!("fabricctl={level}"))
    } else {
        tracing_subscriber::EnvFilter::try_from_default_env()
            .unwrap_or_else(|_| "fabricctl=warn".into())
    };

    tracing_subscriber::registry()
        .with(env_filter)
        .with(tracing_subscriber::fmt::layer().with_target(false))
        .init();

    tracing::debug!("fabricctl starting");

    let mut cmd = Cli::command_with_grouped_help();
    let matches = cmd.get_matches_mut();
    let cli = Cli::from_arg_matches(&matches)?;
    cli.run().await
}
