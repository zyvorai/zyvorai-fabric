// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: LicenseRef-Zyvor-Production-1.0

use anyhow::{Context, Result};
use std::fs;
use std::path::Path;
use std::process::Command;

/// Host VXLAN interface discovered via `ip -d -j link`.
#[derive(Debug, Clone)]
pub struct DiscoveredVxlan {
    pub name: String,
    pub vni: u32,
    pub remote: Option<String>,
    pub local: Option<String>,
    pub port: Option<u16>,
    pub parent_interface: Option<String>,
    pub addresses: Vec<String>,
    pub operstate: String,
}

/// Host SR-IOV PF discovered via sysfs.
#[derive(Debug, Clone)]
pub struct DiscoveredSriovPf {
    pub pf_name: String,
    pub num_vfs: u32,
    pub vf_names: Vec<String>,
}

pub fn discover_host_vxlans() -> Result<Vec<DiscoveredVxlan>> {
    let output = Command::new("ip")
        .args(["-d", "-j", "link", "show"])
        .output()
        .context("ip -d -j link show failed")?;
    if !output.status.success() {
        return Ok(Vec::new());
    }

    let links: Vec<serde_json::Value> = serde_json::from_slice(&output.stdout).unwrap_or_default();

    let addr_map = load_addr_map()?;
    let mut out = Vec::new();

    for entry in links {
        let name = match entry.get("ifname").and_then(|v| v.as_str()) {
            Some(n) => n.to_string(),
            None => continue,
        };
        let linkinfo = entry.get("linkinfo");
        let info_kind = linkinfo
            .and_then(|l| l.get("info_kind"))
            .and_then(|k| k.as_str());
        if info_kind != Some("vxlan") {
            continue;
        }
        let info = linkinfo.and_then(|l| l.get("info_data"));
        let vni = info
            .and_then(|d| d.get("id"))
            .and_then(|v| v.as_u64())
            .unwrap_or(0) as u32;
        let remote = info
            .and_then(|d| d.get("group").or_else(|| d.get("remote")))
            .and_then(|v| v.as_str())
            .map(str::to_string);
        let local = info
            .and_then(|d| d.get("local"))
            .and_then(|v| v.as_str())
            .map(str::to_string);
        let port = info
            .and_then(|d| d.get("dstport").or_else(|| d.get("port")))
            .and_then(|v| v.as_u64())
            .map(|p| p as u16);
        let parent = entry
            .get("master")
            .and_then(|m| m.as_str())
            .map(str::to_string);
        let operstate = entry
            .get("operstate")
            .and_then(|v| v.as_str())
            .unwrap_or("unknown")
            .to_lowercase();

        let addresses = addr_map.get(&name).cloned().unwrap_or_default();
        out.push(DiscoveredVxlan {
            name: name.clone(),
            vni,
            remote,
            local,
            port,
            parent_interface: parent,
            addresses,
            operstate,
        });
    }

    Ok(out)
}

pub fn discover_host_sriov() -> Result<Vec<DiscoveredSriovPf>> {
    let mut pfs = Vec::new();
    let net = Path::new("/sys/class/net");
    let Ok(entries) = fs::read_dir(net) else {
        return Ok(pfs);
    };

    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().to_string();
        let sriov_path = net.join(&name).join("device/sriov_numvfs");
        let Ok(content) = fs::read_to_string(&sriov_path) else {
            continue;
        };
        let num_vfs: u32 = content.trim().parse().unwrap_or(0);
        if num_vfs == 0 {
            continue;
        }
        let vf_names = list_vf_interfaces(&name);
        pfs.push(DiscoveredSriovPf {
            pf_name: name,
            num_vfs,
            vf_names,
        });
    }

    Ok(pfs)
}

fn list_vf_interfaces(pf: &str) -> Vec<String> {
    let mut vfs = Vec::new();
    let Ok(entries) = fs::read_dir("/sys/class/net") else {
        return vfs;
    };
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().to_string();
        let physfn = format!("/sys/class/net/{}/device/physfn_net/dev", name);
        if let Ok(target) = fs::read_link(&physfn) {
            if target.to_string_lossy().contains(pf) {
                vfs.push(name);
            }
        }
    }
    vfs.sort();
    vfs
}

fn load_addr_map() -> Result<std::collections::HashMap<String, Vec<String>>> {
    let output = Command::new("ip")
        .args(["-j", "addr", "show"])
        .output()
        .context("ip -j addr show")?;
    if !output.status.success() {
        return Ok(std::collections::HashMap::new());
    }
    let entries: Vec<serde_json::Value> = serde_json::from_slice(&output.stdout)?;
    let mut m = std::collections::HashMap::new();
    for e in entries {
        if let Some(name) = e.get("ifname").and_then(|v| v.as_str()) {
            let addrs: Vec<String> = e
                .get("addr_info")
                .and_then(|v| v.as_array())
                .map(|arr| {
                    arr.iter()
                        .filter_map(|a| {
                            let local = a.get("local")?.as_str()?;
                            let prefix = a.get("prefixlen")?.as_u64()?;
                            Some(format!("{}/{}", local, prefix))
                        })
                        .collect()
                })
                .unwrap_or_default();
            m.insert(name.to_string(), addrs);
        }
    }
    Ok(m)
}
