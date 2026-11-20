// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: LicenseRef-Zyvor-Production-1.0

use anyhow::Result;
use state_store::StateStore;
use zyvor_fabricd::{config::Config, server::Server};

pub struct Daemon {
    config: Config,
    state: StateStore,
}

impl Daemon {
    pub fn new() -> Result<Self> {
        let config = Config::load()?;
        ensure_runtime_dirs(&config);
        security::trial::init(&std::path::Path::new(&config.storage.path).join(".trial_start"));
        let state = StateStore::new(&config.storage.path)?;

        Ok(Self { config, state })
    }

    pub async fn start(self) -> Result<()> {
        tracing::info!(
            "zyvor-fabricd daemon starting on {}",
            self.config.daemon.listen
        );

        let server = Server::new(self.state, self.config).await?;
        server.run().await
    }
}

/// Create the directories zyvor-fabricd needs at startup, in place of what
/// systemd-tmpfiles previously provisioned before every boot (systemd-removal
/// migration plan, Phase 6). `/var/lib/zyvor-fabricd` itself is created
/// separately by `StateStore::new`; the rest — including `/run/...`, which
/// is tmpfs and would otherwise vanish on every reboot with no init system
/// to recreate it — are handled here. Best-effort: a permission failure
/// (e.g. running as non-root against paths another install step already
/// provisioned) logs a warning rather than aborting startup.
fn ensure_runtime_dirs(config: &Config) {
    let dirs = [
        config.storage.image_path.as_str(),
        "/var/lib/zyvor-fabricd/state",
        "/run/zyvor-fabricd",
        "/var/log/zyvor-fabricd",
    ];
    for dir in dirs {
        if let Err(e) = std::fs::create_dir_all(dir) {
            tracing::warn!("Failed to create runtime directory '{dir}': {e}");
        }
    }
}
