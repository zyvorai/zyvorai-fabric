// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: LicenseRef-Zyvor-Production-1.0

use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum FirmwareError {
    #[error("OVMF firmware not found: {0}")]
    OvmfNotFound(String),

    #[error("Failed to copy OVMF vars: {0}")]
    VarsCopyFailed(String),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Secure Boot not supported")]
    SecureBootNotSupported,

    #[error("Invalid firmware configuration: {0}")]
    InvalidConfig(String),
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum Firmware {
    BIOS,
    UEFI { secure_boot: bool },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum TpmVersion {
    V1_2,
    V2_0,
}

impl TpmVersion {
    pub fn as_str(&self) -> &str {
        match self {
            TpmVersion::V1_2 => "1.2",
            TpmVersion::V2_0 => "2.0",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OvmfConfig {
    pub code_path: PathBuf,
    pub vars_path: PathBuf,
    pub secure_boot: bool,
    pub tpm_version: Option<TpmVersion>,
}

impl OvmfConfig {
    /// Create a new OVMF configuration for a VM
    pub fn new(_vm_name: &str, vm_dir: &Path, secure_boot: bool) -> Result<Self, FirmwareError> {
        // Determine OVMF firmware paths
        let (code_path, vars_template) = if secure_boot {
            Self::find_ovmf_paths_secureboot()?
        } else {
            Self::find_ovmf_paths()?
        };

        // VM-specific vars file
        let vars_path = vm_dir.join("OVMF_VARS.fd");

        // Copy template to VM-specific vars file if it doesn't exist
        if !vars_path.exists() {
            fs::copy(&vars_template, &vars_path)
                .map_err(|e| FirmwareError::VarsCopyFailed(e.to_string()))?;
        }

        Ok(Self {
            code_path,
            vars_path,
            secure_boot,
            tpm_version: None,
        })
    }

    /// Find standard OVMF firmware paths
    fn find_ovmf_paths() -> Result<(PathBuf, PathBuf), FirmwareError> {
        // Common OVMF locations in different distributions
        let possible_locations = vec![
            (
                "/usr/share/OVMF/OVMF_CODE.fd",
                "/usr/share/OVMF/OVMF_VARS.fd",
            ),
            (
                "/usr/share/edk2-ovmf/OVMF_CODE.fd",
                "/usr/share/edk2-ovmf/OVMF_VARS.fd",
            ),
            (
                "/usr/share/qemu/OVMF_CODE.fd",
                "/usr/share/qemu/OVMF_VARS.fd",
            ),
            // Fedora/RHEL
            (
                "/usr/share/edk2/ovmf/OVMF_CODE.fd",
                "/usr/share/edk2/ovmf/OVMF_VARS.fd",
            ),
            // Debian/Ubuntu
            (
                "/usr/share/OVMF/OVMF_CODE_4M.fd",
                "/usr/share/OVMF/OVMF_VARS_4M.fd",
            ),
        ];

        for (code, vars) in possible_locations {
            let code_path = PathBuf::from(code);
            let vars_path = PathBuf::from(vars);

            if code_path.exists() && vars_path.exists() {
                return Ok((code_path, vars_path));
            }
        }

        Err(FirmwareError::OvmfNotFound(
            "No OVMF firmware found in standard locations".to_string(),
        ))
    }

    /// Find Secure Boot-enabled OVMF firmware paths
    fn find_ovmf_paths_secureboot() -> Result<(PathBuf, PathBuf), FirmwareError> {
        let possible_locations = vec![
            (
                "/usr/share/OVMF/OVMF_CODE.secboot.fd",
                "/usr/share/OVMF/OVMF_VARS.secboot.fd",
            ),
            (
                "/usr/share/edk2-ovmf/OVMF_CODE.secboot.fd",
                "/usr/share/edk2-ovmf/OVMF_VARS.secboot.fd",
            ),
            (
                "/usr/share/qemu/OVMF_CODE.secboot.fd",
                "/usr/share/qemu/OVMF_VARS.secboot.fd",
            ),
            // Fedora/RHEL with Secure Boot
            (
                "/usr/share/edk2/ovmf/OVMF_CODE.secboot.fd",
                "/usr/share/edk2/ovmf/OVMF_VARS.secboot.fd",
            ),
            // Alternative naming
            (
                "/usr/share/OVMF/OVMF_CODE.sb.fd",
                "/usr/share/OVMF/OVMF_VARS.sb.fd",
            ),
        ];

        for (code, vars) in possible_locations {
            let code_path = PathBuf::from(code);
            let vars_path = PathBuf::from(vars);

            if code_path.exists() && vars_path.exists() {
                return Ok((code_path, vars_path));
            }
        }

        // Fall back to standard OVMF if Secure Boot versions not found
        Self::find_ovmf_paths()
    }

    /// Enable TPM support
    pub fn with_tpm(mut self, version: TpmVersion) -> Self {
        self.tpm_version = Some(version);
        self
    }

    /// Generate QEMU command-line arguments
    pub fn to_qemu_args(&self) -> Vec<String> {
        let mut args = Vec::new();

        // Add pflash drives for OVMF
        args.push("-drive".to_string());
        args.push(format!(
            "if=pflash,format=raw,readonly=on,file={}",
            self.code_path.display()
        ));

        args.push("-drive".to_string());
        args.push(format!(
            "if=pflash,format=raw,file={}",
            self.vars_path.display()
        ));

        // Add Secure Boot flag if needed
        if self.secure_boot {
            args.push("-global".to_string());
            args.push("ICH9-LPC.disable_s3=1".to_string());
        }

        args
    }

    /// Generate systemd-vmspawn arguments
    pub fn to_vmspawn_args(&self) -> Vec<String> {
        // systemd-vmspawn firmware options
        let mut args = vec![
            "--firmware".to_string(),
            self.code_path.display().to_string(),
            "--firmware-vars".to_string(),
            self.vars_path.display().to_string(),
        ];

        if self.secure_boot {
            args.push("--secure-boot".to_string());
        }

        // TPM support
        if let Some(ref tpm) = self.tpm_version {
            args.push("--tpm".to_string());
            args.push(tpm.as_str().to_string());
        }

        args
    }

    /// Reset NVRAM variables to template defaults
    pub fn reset_nvram(&self) -> Result<(), FirmwareError> {
        let (_code_path, vars_template) = if self.secure_boot {
            Self::find_ovmf_paths_secureboot()?
        } else {
            Self::find_ovmf_paths()?
        };

        fs::copy(&vars_template, &self.vars_path)
            .map_err(|e| FirmwareError::VarsCopyFailed(e.to_string()))?;

        Ok(())
    }

    /// Get firmware status
    pub fn get_status(&self) -> FirmwareStatus {
        FirmwareStatus {
            firmware_type: if self.secure_boot {
                "UEFI (Secure Boot)".to_string()
            } else {
                "UEFI".to_string()
            },
            code_path: self.code_path.clone(),
            vars_path: self.vars_path.clone(),
            secure_boot_enabled: self.secure_boot,
            tpm_enabled: self.tpm_version.is_some(),
            tpm_version: self.tpm_version.as_ref().map(|v| v.as_str().to_string()),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FirmwareStatus {
    pub firmware_type: String,
    pub code_path: PathBuf,
    pub vars_path: PathBuf,
    pub secure_boot_enabled: bool,
    pub tpm_enabled: bool,
    pub tpm_version: Option<String>,
}

/// Check if OVMF is available on the system
pub fn is_ovmf_available() -> bool {
    OvmfConfig::find_ovmf_paths().is_ok()
}

/// Check if Secure Boot OVMF is available
pub fn is_secureboot_available() -> bool {
    OvmfConfig::find_ovmf_paths_secureboot().is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tpm_version() {
        assert_eq!(TpmVersion::V1_2.as_str(), "1.2");
        assert_eq!(TpmVersion::V2_0.as_str(), "2.0");
    }

    #[test]
    fn test_firmware_enum() {
        let bios = Firmware::BIOS;
        let uefi = Firmware::UEFI { secure_boot: false };
        let uefi_sb = Firmware::UEFI { secure_boot: true };

        assert_eq!(bios, Firmware::BIOS);
        assert_ne!(uefi, uefi_sb);
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn test_ovmf_detection() {
        let available = is_ovmf_available();
        println!("OVMF available: {}", available);

        if available {
            let secureboot = is_secureboot_available();
            println!("Secure Boot OVMF available: {}", secureboot);
        }
    }

    #[test]
    fn test_qemu_args_generation() {
        use tempfile::tempdir;

        let temp_dir = tempdir().unwrap();
        let vm_dir = temp_dir.path();

        // Create mock firmware files
        fs::write(vm_dir.join("code.fd"), b"mock code").unwrap();
        fs::write(vm_dir.join("vars.fd"), b"mock vars").unwrap();

        let config = OvmfConfig {
            code_path: vm_dir.join("code.fd"),
            vars_path: vm_dir.join("vars.fd"),
            secure_boot: true,
            tpm_version: Some(TpmVersion::V2_0),
        };

        let args = config.to_qemu_args();
        assert!(args.contains(&"-drive".to_string()));
        assert!(args.iter().any(|a| a.contains("pflash")));
    }

    #[test]
    fn test_vmspawn_args_generation() {
        use tempfile::tempdir;

        let temp_dir = tempdir().unwrap();
        let vm_dir = temp_dir.path();

        fs::write(vm_dir.join("code.fd"), b"mock code").unwrap();
        fs::write(vm_dir.join("vars.fd"), b"mock vars").unwrap();

        let config = OvmfConfig {
            code_path: vm_dir.join("code.fd"),
            vars_path: vm_dir.join("vars.fd"),
            secure_boot: true,
            tpm_version: Some(TpmVersion::V2_0),
        };

        let args = config.to_vmspawn_args();
        assert!(args.contains(&"--firmware".to_string()));
        assert!(args.contains(&"--secure-boot".to_string()));
        assert!(args.contains(&"--tpm".to_string()));
    }
}
