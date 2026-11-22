// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: LicenseRef-Zyvor-Production-2.0

use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CloudInitConfig {
    pub instance_id: String,
    pub hostname: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub user_data: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub meta_data: Option<MetaData>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub network_config: Option<NetworkConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetaData {
    pub instance_id: String,
    pub hostname: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub public_keys: Option<Vec<String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetworkConfig {
    pub version: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ethernets: Option<serde_yaml::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserData {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub users: Option<Vec<User>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ssh_authorized_keys: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub packages: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub runcmd: Option<Vec<String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct User {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sudo: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub shell: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ssh_authorized_keys: Option<Vec<String>>,
}

pub struct CloudInitGenerator {
    output_dir: PathBuf,
}

impl CloudInitGenerator {
    pub fn new<P: AsRef<Path>>(output_dir: P) -> Result<Self> {
        let output_dir = output_dir.as_ref().to_path_buf();
        fs::create_dir_all(&output_dir)?;
        Ok(Self { output_dir })
    }

    pub fn generate(&self, config: &CloudInitConfig) -> Result<PathBuf> {
        let vm_dir = self.output_dir.join(&config.instance_id);
        fs::create_dir_all(&vm_dir)?;

        // Write meta-data
        if let Some(meta) = &config.meta_data {
            let meta_data = serde_yaml::to_string(meta)?;
            fs::write(vm_dir.join("meta-data"), meta_data)?;
        } else {
            // Default meta-data
            let meta = MetaData {
                instance_id: config.instance_id.clone(),
                hostname: config.hostname.clone(),
                public_keys: None,
            };
            let meta_data = serde_yaml::to_string(&meta)?;
            fs::write(vm_dir.join("meta-data"), meta_data)?;
        }

        // Write user-data
        if let Some(user_data) = &config.user_data {
            fs::write(
                vm_dir.join("user-data"),
                format!("#cloud-config\n{}", user_data),
            )?;
        }

        // Write network-config
        if let Some(network) = &config.network_config {
            let network_data = serde_yaml::to_string(network)?;
            fs::write(vm_dir.join("network-config"), network_data)?;
        }

        // Generate ISO image (NoCloud datasource)
        self.generate_iso(&vm_dir)?;

        Ok(vm_dir.join("cloud-init.iso"))
    }

    fn generate_iso(&self, vm_dir: &Path) -> Result<()> {
        use std::process::Command;

        let iso_path = vm_dir.join("cloud-init.iso");

        // Use genisoimage or mkisofs to create ISO
        let output = Command::new("genisoimage")
            .arg("-output")
            .arg(&iso_path)
            .arg("-volid")
            .arg("cidata")
            .arg("-joliet")
            .arg("-rock")
            .arg(vm_dir)
            .output();

        match output {
            Ok(out) if out.status.success() => Ok(()),
            Ok(out) => {
                let stderr = String::from_utf8_lossy(&out.stderr);
                Err(anyhow::anyhow!("Failed to generate ISO: {}", stderr))
            }
            Err(_) => {
                // Fallback to mkisofs
                let output = Command::new("mkisofs")
                    .arg("-output")
                    .arg(&iso_path)
                    .arg("-volid")
                    .arg("cidata")
                    .arg("-joliet")
                    .arg("-rock")
                    .arg(vm_dir)
                    .output()?;

                if output.status.success() {
                    Ok(())
                } else {
                    let stderr = String::from_utf8_lossy(&output.stderr);
                    Err(anyhow::anyhow!("Failed to generate ISO: {}", stderr))
                }
            }
        }
    }
}

/// `agent_url` is the guest-reachable URL for zyvor-fabricd's own
/// `/vendor/zyvor-guest-agent` route (see server.rs) -- there's no distro
/// package for GuestKit's in-guest agent (unlike qemu-guest-agent), so
/// installing it means curling the binary from somewhere. Pass `None` to
/// skip agent installation entirely rather than reference a package that
/// doesn't exist.
pub fn create_default_user_data(
    username: &str,
    ssh_key: Option<&str>,
    agent_url: Option<&str>,
) -> String {
    let user_data = UserData {
        users: Some(vec![User {
            name: username.to_string(),
            sudo: Some("ALL=(ALL) NOPASSWD:ALL".to_string()),
            shell: Some("/bin/bash".to_string()),
            ssh_authorized_keys: ssh_key.map(|key| vec![key.to_string()]),
        }]),
        ssh_authorized_keys: ssh_key.map(|key| vec![key.to_string()]),
        packages: None,
        runcmd: agent_url.map(|url| {
            vec![
                format!("curl -fsSL {url} -o /usr/local/bin/zyvor-guest-agent"),
                "chmod +x /usr/local/bin/zyvor-guest-agent".to_string(),
            ]
        }),
    };

    serde_yaml::to_string(&user_data).unwrap_or_default()
}
