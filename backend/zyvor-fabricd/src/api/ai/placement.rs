// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: LicenseRef-Zyvor-Production-2.0

//! GPU placement helpers for AI inference (preview).

use zyvor_fabric_fluxvm_client::HostGpu;

use super::types::GpuRequirements;

/// Pick free NVIDIA GPUs that meet VRAM and vendor filters.
///
/// `allocated_bdfs` are BDFs already claimed by Fabric deployments (or
/// FluxVM-running VMs). Prefers devices already bound to vfio-pci that are
/// not held by another process.
pub fn place_gpus<'a>(
    inventory: &'a [HostGpu],
    req: &GpuRequirements,
    allocated_bdfs: &std::collections::HashSet<String>,
) -> Result<Vec<&'a HostGpu>, String> {
    let vendor = req.vendor.trim().to_ascii_lowercase();
    if vendor != "nvidia" {
        return Err(format!(
            "unsupported GPU vendor '{vendor}' (MVP supports only 'nvidia')"
        ));
    }
    let need = req.count.max(1) as usize;

    let mut candidates: Vec<&HostGpu> = inventory
        .iter()
        .filter(|g| {
            g.vendor.eq_ignore_ascii_case("nvidia")
                || g.vendor_id == 0x10de
                || g.vendor.to_ascii_lowercase().contains("nvidia")
        })
        .filter(|g| !allocated_bdfs.contains(&g.bdf.to_ascii_lowercase()))
        .filter(|g| !g.group_held)
        .filter(|g| match g.vram_gib {
            Some(v) => v >= req.minimum_vram_gib,
            None => req.minimum_vram_gib == 0,
        })
        .collect();

    // Prefer already-bound vfio groups (less host disruption).
    candidates.sort_by_key(|g| (!g.group_bound_to_vfio, g.bdf.as_str()));

    if candidates.len() < need {
        return Err(format!(
            "not enough free NVIDIA GPUs: need {need}, found {}",
            candidates.len()
        ));
    }
    Ok(candidates.into_iter().take(need).collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn gpu(bdf: &str, vram: Option<u32>, bound: bool, held: bool) -> HostGpu {
        HostGpu {
            bdf: bdf.into(),
            vendor_id: 0x10de,
            device_id: 0x2684,
            vendor: "NVIDIA".into(),
            class_id: 0x0302,
            driver: if bound {
                Some("vfio-pci".into())
            } else {
                Some("nvidia".into())
            },
            iommu_group: Some(1),
            iommu_members: vec![bdf.into()],
            group_bound_to_vfio: bound,
            group_held: held,
            numa_node: Some(0),
            vram_gib: vram,
            previous_driver: None,
        }
    }

    #[test]
    fn places_free_nvidia_preferring_vfio_bound() {
        let inv = vec![
            gpu("0000:01:00.0", Some(24), false, false),
            gpu("0000:02:00.0", Some(48), true, false),
            gpu("0000:03:00.0", Some(48), true, true),
        ];
        let req = GpuRequirements {
            vendor: "nvidia".into(),
            count: 1,
            minimum_vram_gib: 40,
        };
        let allocated = std::collections::HashSet::new();
        let picked = place_gpus(&inv, &req, &allocated).unwrap();
        assert_eq!(picked.len(), 1);
        assert_eq!(picked[0].bdf, "0000:02:00.0");
    }

    #[test]
    fn skips_allocated_and_errors_when_short() {
        let inv = vec![gpu("0000:01:00.0", Some(24), true, false)];
        let req = GpuRequirements {
            vendor: "nvidia".into(),
            count: 1,
            minimum_vram_gib: 0,
        };
        let mut allocated = std::collections::HashSet::new();
        allocated.insert("0000:01:00.0".into());
        assert!(place_gpus(&inv, &req, &allocated).is_err());
    }

    #[test]
    fn rejects_non_nvidia_vendor() {
        let inv = vec![gpu("0000:01:00.0", Some(24), true, false)];
        let req = GpuRequirements {
            vendor: "amd".into(),
            count: 1,
            minimum_vram_gib: 0,
        };
        assert!(place_gpus(&inv, &req, &std::collections::HashSet::new()).is_err());
    }
}
