// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: LicenseRef-Zyvor-Production-2.0

//! GPU group selection and MIG safety.
//!
//! A MIG profile change is refused while any replica still holds the parent
//! device. Fractional sharing without MIG is refused.

use super::types::NodeGpu;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GpuGroup {
    pub bdfs: Vec<String>,
}

pub fn gpu_schedulable(gpu: &NodeGpu, max_temp_c: u32, reject_ecc: bool) -> bool {
    if !gpu.healthy {
        return false;
    }
    if max_temp_c > 0 && gpu.temperature_c > max_temp_c {
        return false;
    }
    if reject_ecc && gpu.ecc_errors > 0 {
        return false;
    }
    true
}

fn max_gpu_temp_c() -> u32 {
    std::env::var("FLUXVM_AI_MAX_GPU_TEMP_C")
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(0)
}

fn reject_ecc() -> bool {
    std::env::var("FLUXVM_AI_REJECT_ECC").ok().as_deref() == Some("1")
}

pub fn select_group(gpus: &[NodeGpu], count: u32, mig_profile: &str) -> Result<GpuGroup, String> {
    let need = count.max(1) as usize;
    if need > 8 {
        return Err("gpu.count must be between 1 and 8".into());
    }
    let mut chosen = Vec::new();
    for gpu in gpus {
        if !gpu_schedulable(gpu, max_gpu_temp_c(), reject_ecc()) {
            continue;
        }
        if !mig_profile.is_empty() && gpu.mig_profile != mig_profile {
            continue;
        }
        if mig_profile.is_empty() && !gpu.mig_profile.is_empty() {
            continue;
        }
        chosen.push(gpu.bdf.clone());
        if chosen.len() == need {
            break;
        }
    }
    if chosen.len() != need {
        return Err(format!(
            "need {need} matching GPUs on one node, found {}",
            chosen.len()
        ));
    }
    Ok(GpuGroup { bdfs: chosen })
}

/// True when no allocation names this BDF or lists it as a parent.
pub fn mig_reconfigure_allowed(parent_bdf: &str, allocated: &[(String, String)]) -> bool {
    !allocated
        .iter()
        .any(|(_, bdf)| bdf.eq_ignore_ascii_case(parent_bdf))
}

pub fn quarantine(gpu: &mut NodeGpu) {
    gpu.healthy = false;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct JanusMigProfile {
    pub name: &'static str,
    pub memory_gib: u32,
    pub max_per_gpu: u32,
}

/// H100 profiles from the Janus / PCI MIG catalog.
const JANUS_MIG: &[JanusMigProfile] = &[
    JanusMigProfile {
        name: "1g.10gb",
        memory_gib: 10,
        max_per_gpu: 7,
    },
    JanusMigProfile {
        name: "1g",
        memory_gib: 10,
        max_per_gpu: 7,
    },
    JanusMigProfile {
        name: "2g.20gb",
        memory_gib: 20,
        max_per_gpu: 3,
    },
    JanusMigProfile {
        name: "2g",
        memory_gib: 20,
        max_per_gpu: 3,
    },
    JanusMigProfile {
        name: "3g.40gb",
        memory_gib: 40,
        max_per_gpu: 2,
    },
    JanusMigProfile {
        name: "3g",
        memory_gib: 40,
        max_per_gpu: 2,
    },
    JanusMigProfile {
        name: "7g.80gb",
        memory_gib: 80,
        max_per_gpu: 1,
    },
    JanusMigProfile {
        name: "7g",
        memory_gib: 80,
        max_per_gpu: 1,
    },
];

pub fn janus_mig_profile(name: &str) -> Option<&'static JanusMigProfile> {
    JANUS_MIG.iter().find(|profile| profile.name == name)
}

pub fn slice_fits(parent_vram_gib: u32, profile: &JanusMigProfile, existing: u32) -> bool {
    profile.memory_gib <= parent_vram_gib && existing < profile.max_per_gpu
}

pub fn slice_bdf(parent_bdf: &str, profile: &str, index: u32) -> String {
    format!("{parent_bdf}--{profile}--{index}")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn gpu(bdf: &str, healthy: bool, mig: &str) -> NodeGpu {
        NodeGpu {
            bdf: bdf.into(),
            vendor: "nvidia".into(),
            vram_gib: 40,
            model: "A100".into(),
            healthy,
            mig_profile: mig.into(),
            parent_bdf: String::new(),
            nvlink: true,
            temperature_c: 0,
            power_watts: 0,
            ecc_errors: 0,
        }
    }

    #[test]
    fn group_is_atomic_and_skips_a_quarantined_device() {
        let gpus = vec![
            gpu("0000:01:00.0", false, ""),
            gpu("0000:02:00.0", true, ""),
            gpu("0000:03:00.0", true, ""),
        ];
        let group = select_group(&gpus, 2, "").unwrap();
        assert_eq!(group.bdfs, vec!["0000:02:00.0", "0000:03:00.0"]);
        assert!(select_group(&gpus, 3, "").is_err());
    }

    #[test]
    fn mig_profile_must_match_and_a_held_parent_cannot_change() {
        let gpus = vec![gpu("0000:01:00.0", true, "1g.10gb")];
        assert!(select_group(&gpus, 1, "1g.5gb").is_err());
        assert!(select_group(&gpus, 1, "1g.10gb").is_ok());
        assert!(!mig_reconfigure_allowed(
            "0000:01:00.0",
            &[("node-a".into(), "0000:01:00.0".into())]
        ));
        assert!(mig_reconfigure_allowed("0000:01:00.0", &[]));
        assert!(janus_mig_profile("1g.10gb").is_some());
        assert!(janus_mig_profile("9g").is_none());
        assert!(slice_fits(80, janus_mig_profile("1g.10gb").unwrap(), 6));
        assert!(!slice_fits(80, janus_mig_profile("1g.10gb").unwrap(), 7));
        assert_eq!(
            slice_bdf("janus:node-0:gpu-0", "1g.10gb", 0),
            "janus:node-0:gpu-0--1g.10gb--0"
        );
    }

    #[test]
    fn hot_or_ecc_gpus_are_not_schedulable() {
        let mut hot = gpu("hot", true, "");
        hot.temperature_c = 90;
        let mut ecc = gpu("ecc", true, "");
        ecc.ecc_errors = 2;
        assert!(!gpu_schedulable(&hot, 80, false));
        assert!(gpu_schedulable(&hot, 0, false));
        assert!(!gpu_schedulable(&ecc, 0, true));
        assert!(gpu_schedulable(&ecc, 0, false));
    }
}
