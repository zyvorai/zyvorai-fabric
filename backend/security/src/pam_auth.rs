// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: LicenseRef-Zyvor-Production-2.0

use anyhow::{Context, Result};

/// Authenticate a user against PAM.
///
/// Uses the "zyvor-fabricd" PAM service if available, falling back to "login".
pub fn authenticate(username: &str, password: &str) -> Result<()> {
    let service = if std::path::Path::new("/etc/pam.d/zyvor-fabricd").exists() {
        "zyvor-fabricd"
    } else {
        "login"
    };

    let mut client = pam::Client::with_password(service).context("Failed to create PAM client")?;

    client
        .conversation_mut()
        .set_credentials(username, password);

    client
        .authenticate()
        .map_err(|e| anyhow::anyhow!("PAM authentication failed: {:?}", e))?;

    Ok(())
}
