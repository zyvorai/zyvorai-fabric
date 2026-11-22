// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: LicenseRef-Zyvor-Production-2.0

use anyhow::{anyhow, Result};
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::process::Command;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GPUDevice {
    pub pci_address: String,
    pub vendor: String,
    pub device_name: String,
    pub driver: String,
    pub is_available: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GPUPassthroughConfig {
    pub pci_address: String,
    pub multifunction: bool,
    pub romfile: Option<String>,
}

pub struct GPUManager;

impl GPUManager {
    /// Detect available GPUs
    pub fn detect_gpus() -> Result<Vec<GPUDevice>> {
        let output = Command::new("lspci")
            .args(["-nn", "-D"])
            .output()?;

        if !output.status.success() {
            return Err(anyhow!("Failed to run lspci"));
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        let mut gpus = Vec::new();

        // Regex to match PCI addresses and VGA/3D controllers
        let re = Regex::new(r"([0-9a-f:.]+)\s+(VGA|3D)\s+.*:\s+(.+?)\s+\[([0-9a-f]+:[0-9a-f]+)\]")?;

        for line in stdout.lines() {
            if let Some(caps) = re.captures(line) {
                let pci_address = caps.get(1).unwrap().as_str().to_string();
                let device_name = caps.get(3).unwrap().as_str().to_string();
                let vendor_device = caps.get(4).unwrap().as_str();

                let driver = Self::get_driver(&pci_address)?;
                let is_available = driver == "vfio-pci" || driver.is_empty();

                gpus.push(GPUDevice {
                    pci_address: pci_address.clone(),
                    vendor: vendor_device.to_string(),
                    device_name,
                    driver,
                    is_available,
                });
            }
        }

        Ok(gpus)
    }

    /// Get current driver for PCI device
    fn get_driver(pci_address: &str) -> Result<String> {
        let driver_path = format!("/sys/bus/pci/devices/{}/driver", pci_address);

        match std::fs::read_link(&driver_path) {
            Ok(path) => {
                let driver = path
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or("")
                    .to_string();
                Ok(driver)
            }
            Err(_) => Ok(String::new()),
        }
    }

    /// Bind GPU to VFIO driver for passthrough
    pub fn bind_to_vfio(pci_address: &str) -> Result<()> {
        tracing::info!("Binding {} to vfio-pci driver", pci_address);

        // Unbind from current driver
        if let Ok(current_driver) = Self::get_driver(pci_address) {
            if !current_driver.is_empty() && current_driver != "vfio-pci" {
                let unbind_path = format!("/sys/bus/pci/drivers/{}/unbind", current_driver);
                std::fs::write(unbind_path, pci_address)?;
            }
        }

        // Get vendor and device IDs
        let vendor_id = std::fs::read_to_string(
            format!("/sys/bus/pci/devices/{}/vendor", pci_address)
        )?;
        let device_id = std::fs::read_to_string(
            format!("/sys/bus/pci/devices/{}/device", pci_address)
        )?;

        // Bind to vfio-pci
        let new_id = format!("{} {}",
            vendor_id.trim().trim_start_matches("0x"),
            device_id.trim().trim_start_matches("0x")
        );

        std::fs::write("/sys/bus/pci/drivers/vfio-pci/new_id", new_id)?;

        tracing::info!("Successfully bound {} to vfio-pci", pci_address);

        Ok(())
    }

    /// Unbind GPU from VFIO driver
    pub fn unbind_from_vfio(pci_address: &str) -> Result<()> {
        tracing::info!("Unbinding {} from vfio-pci driver", pci_address);

        let unbind_path = "/sys/bus/pci/drivers/vfio-pci/unbind";
        std::fs::write(unbind_path, pci_address)?;

        tracing::info!("Successfully unbound {} from vfio-pci", pci_address);

        Ok(())
    }

    /// Check if IOMMU is enabled
    pub fn check_iommu() -> Result<bool> {
        let cmdline = std::fs::read_to_string("/proc/cmdline")?;

        Ok(cmdline.contains("intel_iommu=on") || cmdline.contains("amd_iommu=on"))
    }

    /// Get IOMMU groups
    pub fn get_iommu_groups() -> Result<Vec<(String, Vec<String>)>> {
        let mut groups = Vec::new();

        let iommu_path = "/sys/kernel/iommu_groups";
        if !std::path::Path::new(iommu_path).exists() {
            return Ok(groups);
        }

        for entry in std::fs::read_dir(iommu_path)? {
            let entry = entry?;
            let group_id = entry.file_name().to_string_lossy().to_string();

            let devices_path = entry.path().join("devices");
            let mut devices = Vec::new();

            if let Ok(device_entries) = std::fs::read_dir(devices_path) {
                for device in device_entries {
                    if let Ok(device) = device {
                        devices.push(device.file_name().to_string_lossy().to_string());
                    }
                }
            }

            groups.push((group_id, devices));
        }

        Ok(groups)
    }

    /// Generate QEMU arguments for GPU passthrough
    pub fn generate_qemu_args(config: &GPUPassthroughConfig) -> Vec<String> {
        let mut args = vec![
            "-device".to_string(),
            format!(
                "vfio-pci,host={},{}",
                config.pci_address,
                if config.multifunction {
                    "multifunction=on"
                } else {
                    "multifunction=off"
                }
            ),
        ];

        if let Some(romfile) = &config.romfile {
            args.push("-option-rom".to_string());
            args.push(romfile.clone());
        }

        // Enable GPU acceleration features
        args.extend_from_slice(&[
            "-vga".to_string(),
            "none".to_string(),
            "-nographic".to_string(),
        ]);

        args
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_check_iommu() {
        // This test requires actual hardware with IOMMU
        let _ = GPUManager::check_iommu();
    }
}
