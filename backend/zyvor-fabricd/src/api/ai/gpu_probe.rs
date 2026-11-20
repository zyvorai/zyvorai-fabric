// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: LicenseRef-Zyvor-Production-1.0

//! Live GPU readings from `nvidia-smi` when that binary is on `PATH`.
//!
//! A missing binary or a failed command does not invent a temperature, a
//! power draw, or an ECC count. Janus ids are not queried. This does not
//! change a MIG profile.

use super::types::NodeGpu;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GpuSample {
    pub bdf: String,
    pub temperature_c: u32,
    pub power_watts: u32,
    pub ecc_errors: u64,
}

pub fn query_nvidia_smi() -> Option<String> {
    if !cfg!(target_os = "linux") {
        return None;
    }
    let output = std::process::Command::new("nvidia-smi")
        .args([
            "--query-gpu=pci.bus_id,temperature.gpu,power.draw,ecc.errors.uncorrected.volatile",
            "--format=csv,noheader,nounits",
        ])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    Some(String::from_utf8_lossy(&output.stdout).into_owned())
}

pub fn parse_nvidia_smi(text: &str) -> Vec<GpuSample> {
    text.lines()
        .filter_map(|line| {
            let mut fields = line.split(',').map(|field| field.trim());
            let bdf = fields.next().filter(|value| !value.is_empty())?;
            let temperature_c = number(fields.next().unwrap_or("")) as u32;
            let power_watts = number(fields.next().unwrap_or("")) as u32;
            let ecc_errors = number(fields.next().unwrap_or(""));
            Some(GpuSample {
                bdf: bdf.to_string(),
                temperature_c,
                power_watts,
                ecc_errors,
            })
        })
        .collect()
}

fn number(value: &str) -> u64 {
    let value = value.split('.').next().unwrap_or("").trim();
    value.parse().unwrap_or(0)
}

fn bdf_tail(bdf: &str) -> String {
    let mut parts = bdf.rsplitn(3, ':');
    let function = parts.next().unwrap_or("");
    let device = parts.next().unwrap_or("");
    format!("{device}:{function}").to_ascii_lowercase()
}

pub fn apply_samples(gpus: &mut [NodeGpu], samples: &[GpuSample]) {
    for gpu in gpus {
        if gpu.bdf.starts_with("janus:") {
            continue;
        }
        let Some(sample) = samples
            .iter()
            .find(|sample| bdf_tail(&sample.bdf) == bdf_tail(&gpu.bdf))
        else {
            continue;
        };
        gpu.temperature_c = sample.temperature_c;
        gpu.power_watts = sample.power_watts;
        gpu.ecc_errors = sample.ecc_errors;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn gpu(bdf: &str) -> NodeGpu {
        NodeGpu {
            bdf: bdf.into(),
            vendor: "nvidia".into(),
            vram_gib: 80,
            model: "H100".into(),
            healthy: true,
            mig_profile: String::new(),
            parent_bdf: String::new(),
            nvlink: true,
            temperature_c: 0,
            power_watts: 0,
            ecc_errors: 0,
        }
    }

    #[test]
    fn csv_fills_a_matching_device_and_skips_janus() {
        let samples = parse_nvidia_smi(
            "00000000:01:00.0, 61, 250.40, 2\n00000000:02:00.0, [N/A], [N/A], [N/A]\n",
        );
        let mut gpus = vec![gpu("0000:01:00.0"), gpu("janus:node-0:gpu-0")];
        apply_samples(&mut gpus, &samples);
        assert_eq!(gpus[0].temperature_c, 61);
        assert_eq!(gpus[0].power_watts, 250);
        assert_eq!(gpus[0].ecc_errors, 2);
        assert_eq!(gpus[1].temperature_c, 0);
        assert_eq!(gpus[1].ecc_errors, 0);
    }
}
