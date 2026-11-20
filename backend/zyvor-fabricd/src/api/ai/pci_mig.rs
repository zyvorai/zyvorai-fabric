// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: LicenseRef-Zyvor-Production-1.0

//! PCI MIG create/destroy via `nvidia-smi` when enabled.
//!
//! Janus BDFs stay record-only. A real PCI BDF requires `FLUXVM_AI_PCI_MIG=1`.
//! `FLUXVM_AI_PCI_MIG_RECORD_ONLY=1` skips the driver and only writes inventory
//! (lab / CI without an NVIDIA GPU).

use super::gpu_orch::JanusMigProfile;

/// NVIDIA MIG GPU instance profile ids for H100 80GB (compute instance follows).
fn profile_gi_id(profile: &str) -> Option<&'static str> {
    match profile {
        "1g.10gb" | "1g" => Some("19"),
        "2g.20gb" | "2g" => Some("14"),
        "3g.40gb" | "3g" => Some("9"),
        "7g.80gb" | "7g" => Some("0"),
        _ => None,
    }
}

pub fn pci_mig_enabled() -> bool {
    matches!(
        std::env::var("FLUXVM_AI_PCI_MIG").as_deref(),
        Ok("1") | Ok("true") | Ok("TRUE") | Ok("yes")
    )
}

pub fn record_only() -> bool {
    matches!(
        std::env::var("FLUXVM_AI_PCI_MIG_RECORD_ONLY").as_deref(),
        Ok("1") | Ok("true") | Ok("TRUE") | Ok("yes")
    )
}

pub fn is_pci_bdf(bdf: &str) -> bool {
    if bdf.starts_with("janus:") || bdf.contains("--") {
        return false;
    }
    // 0000:01:00.0 or 01:00.0
    let parts: Vec<&str> = bdf.split(':').collect();
    if parts.len() < 2 {
        return false;
    }
    let last = parts.last().copied().unwrap_or("");
    last.contains('.') && last.split('.').all(|p| !p.is_empty())
}

pub fn create_argv(parent_bdf: &str, profile: &JanusMigProfile) -> Result<Vec<String>, String> {
    let gi = profile_gi_id(profile.name)
        .ok_or_else(|| format!("no nvidia-smi GI id for profile '{}'", profile.name))?;
    Ok(vec![
        "mig".into(),
        "-i".into(),
        parent_bdf.to_string(),
        "-cgi".into(),
        gi.to_string(),
        "-C".into(),
    ])
}

pub fn destroy_argv(slice_bdf: &str) -> Vec<String> {
    // Slice inventory id is `{parent}--{profile}--{index}`; driver destroy uses parent.
    let parent = slice_bdf.split("--").next().unwrap_or(slice_bdf);
    vec![
        "mig".into(),
        "-i".into(),
        parent.to_string(),
        "-dci".into(),
        "-dgi".into(),
    ]
}

pub fn run_nvidia_smi(argv: &[String]) -> Result<(), String> {
    if record_only() {
        return Ok(());
    }
    if !cfg!(target_os = "linux") {
        return Err(
            "PCI MIG requires Linux nvidia-smi (or FLUXVM_AI_PCI_MIG_RECORD_ONLY=1)".into(),
        );
    }
    let output = std::process::Command::new("nvidia-smi")
        .args(argv)
        .output()
        .map_err(|err| format!("nvidia-smi: {err}"))?;
    if output.status.success() {
        return Ok(());
    }
    let stderr = String::from_utf8_lossy(&output.stderr);
    let stdout = String::from_utf8_lossy(&output.stdout);
    Err(
        format!("nvidia-smi mig failed: {} {}", stderr.trim(), stdout.trim())
            .trim()
            .to_string(),
    )
}

pub fn apply_create(parent_bdf: &str, profile: &JanusMigProfile) -> Result<(), String> {
    let argv = create_argv(parent_bdf, profile)?;
    run_nvidia_smi(&argv)
}

pub fn apply_destroy(slice_bdf: &str) -> Result<(), String> {
    let argv = destroy_argv(slice_bdf);
    run_nvidia_smi(&argv)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::ai::gpu_orch::janus_mig_profile;

    #[test]
    fn pci_bdf_shape_and_create_argv() {
        assert!(is_pci_bdf("0000:01:00.0"));
        assert!(is_pci_bdf("01:00.0"));
        assert!(!is_pci_bdf("janus:node-0:gpu-0"));
        assert!(!is_pci_bdf("0000:01:00.0--1g.10gb--0"));
        let profile = janus_mig_profile("1g.10gb").unwrap();
        let argv = create_argv("0000:01:00.0", profile).unwrap();
        assert_eq!(argv, vec!["mig", "-i", "0000:01:00.0", "-cgi", "19", "-C"]);
        assert_eq!(
            destroy_argv("0000:01:00.0--1g.10gb--0"),
            vec!["mig", "-i", "0000:01:00.0", "-dci", "-dgi"]
        );
    }
}
