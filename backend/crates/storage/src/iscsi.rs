// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: LicenseRef-Zyvor-Production-2.0

//! iSCSI storage backend using iscsiadm CLI.

use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IscsiTarget {
    pub portal: String,
    pub target_iqn: String,
    pub lun: u32,
    pub auth_username: Option<String>,
    pub device_path: Option<String>,
    pub connected: bool,
}

/// Discover iSCSI targets on a portal.
pub fn discover_targets(portal: &str) -> Result<Vec<String>> {
    let portal = input_guard::vet!(portal, anyhow!("rejected iSCSI portal"));
    let output = input_guard::argv_checked!(portal, anyhow!("rejected iSCSI portal"), |portal| {
        std::process::Command::new("iscsiadm")
            .args(["-m", "discovery", "-t", "sendtargets", "-p", portal])
            .output()
            .map_err(|e| anyhow!("Failed to run iscsiadm: {}. Is open-iscsi installed?", e))?
    });

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(anyhow!("iSCSI discovery failed: {}", stderr));
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let targets: Vec<String> = stdout
        .lines()
        .filter_map(|line| line.split_whitespace().nth(1).map(|s| s.to_string()))
        .collect();
    Ok(targets)
}

/// Login to an iSCSI target.
pub fn login_target(portal: &str, target_iqn: &str) -> Result<()> {
    let portal = input_guard::vet!(portal, anyhow!("rejected iSCSI portal"));
    let target_iqn = input_guard::vet!(target_iqn, anyhow!("rejected iSCSI target"));
    let output = input_guard::argv_checked!(portal, anyhow!("rejected iSCSI portal"), |portal| {
        input_guard::argv_checked!(target_iqn, anyhow!("rejected iSCSI target"), |target_iqn| {
            std::process::Command::new("iscsiadm")
                .args(["-m", "node", "-T", target_iqn, "-p", portal, "--login"])
                .output()
                .map_err(|e| anyhow!("Failed to run iscsiadm login: {}", e))?
        })
    });

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(anyhow!("iSCSI login failed: {}", stderr));
    }
    Ok(())
}

/// Logout from an iSCSI target.
pub fn logout_target(portal: &str, target_iqn: &str) -> Result<()> {
    let portal = input_guard::vet!(portal, anyhow!("rejected iSCSI portal"));
    let target_iqn = input_guard::vet!(target_iqn, anyhow!("rejected iSCSI target"));
    let output = input_guard::argv_checked!(portal, anyhow!("rejected iSCSI portal"), |portal| {
        input_guard::argv_checked!(target_iqn, anyhow!("rejected iSCSI target"), |target_iqn| {
            std::process::Command::new("iscsiadm")
                .args(["-m", "node", "-T", target_iqn, "-p", portal, "--logout"])
                .output()
                .map_err(|e| anyhow!("Failed to run iscsiadm logout: {}", e))?
        })
    });

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(anyhow!("iSCSI logout failed: {}", stderr));
    }
    Ok(())
}

/// List active iSCSI sessions.
pub fn list_sessions() -> Result<Vec<IscsiTarget>> {
    let output = std::process::Command::new("iscsiadm")
        .args(["-m", "session", "-P", "3"])
        .output()
        .map_err(|e| anyhow!("Failed to run iscsiadm: {}", e))?;

    let stdout = String::from_utf8_lossy(&output.stdout);
    let mut targets = Vec::new();
    let mut current_portal = String::new();
    let mut current_iqn = String::new();

    for line in stdout.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("Current Portal:") {
            current_portal = trimmed
                .split(':')
                .nth(1)
                .unwrap_or("")
                .trim()
                .split(',')
                .next()
                .unwrap_or("")
                .to_string();
        } else if trimmed.starts_with("Target:") {
            current_iqn = trimmed.split_whitespace().nth(1).unwrap_or("").to_string();
        } else if trimmed.starts_with("Attached scsi disk") {
            let device = trimmed.split_whitespace().nth(3).unwrap_or("");
            targets.push(IscsiTarget {
                portal: current_portal.clone(),
                target_iqn: current_iqn.clone(),
                lun: 0,
                auth_username: None,
                device_path: Some(format!("/dev/{}", device)),
                connected: true,
            });
        }
    }
    Ok(targets)
}
