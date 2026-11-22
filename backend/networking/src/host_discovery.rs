// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: LicenseRef-Zyvor-Production-2.0

use anyhow::{Context, Result};
use std::collections::HashMap;
use std::path::Path;
use std::process::Command;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HostDeviceType {
    Bridge,
    Bond,
    Vlan,
    Macvtap,
    Tap,
    Veth,
    Physical,
}

#[derive(Debug, Clone)]
pub struct HostNetDevice {
    pub name: String,
    pub operstate: String,
    pub device_type: HostDeviceType,
    pub vlan_id: Option<u16>,
    pub parent: Option<String>,
    pub addresses: Vec<String>,
    pub mac: Option<String>,
}

/// Discover live network interfaces from the host (no networkctl required).
pub fn discover_interfaces() -> Result<Vec<HostNetDevice>> {
    let link_output = Command::new("ip")
        .args(["-j", "link", "show"])
        .output()
        .context("ip -j link show failed")?;
    if !link_output.status.success() {
        anyhow::bail!(
            "ip -j link show failed: {}",
            String::from_utf8_lossy(&link_output.stderr)
        );
    }

    let links: Vec<serde_json::Value> =
        serde_json::from_slice(&link_output.stdout).context("parse ip link json")?;

    let addr_output = Command::new("ip")
        .args(["-j", "addr", "show"])
        .output()
        .ok();
    let addr_map = addr_output
        .filter(|o| o.status.success())
        .and_then(|o| serde_json::from_slice::<Vec<serde_json::Value>>(&o.stdout).ok())
        .map(|entries| {
            let mut m = HashMap::new();
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
            m
        })
        .unwrap_or_default();

    let mut devices = Vec::new();
    for entry in links {
        let name = match entry.get("ifname").and_then(|v| v.as_str()) {
            Some(n) => n.to_string(),
            None => continue,
        };
        if name == "lo" {
            continue;
        }

        let operstate = entry
            .get("operstate")
            .and_then(|v| v.as_str())
            .unwrap_or("unknown")
            .to_lowercase();
        let mac = entry
            .get("address")
            .and_then(|v| v.as_str())
            .map(str::to_string);
        let addresses = addr_map.get(&name).cloned().unwrap_or_default();

        let sysfs = format!("/sys/class/net/{}", name);
        let device_type = classify_interface(
            &name,
            &sysfs,
            entry.get("link_type").and_then(|v| v.as_str()),
        );
        let (vlan_id, parent) = parse_vlan_info(&name, &sysfs, device_type);

        devices.push(HostNetDevice {
            name,
            operstate,
            device_type,
            vlan_id,
            parent,
            addresses,
            mac,
        });
    }

    devices.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(devices)
}

fn classify_interface(name: &str, sysfs: &str, link_type: Option<&str>) -> HostDeviceType {
    if Path::new(&format!("{}/bridge", sysfs)).exists() {
        return HostDeviceType::Bridge;
    }
    if Path::new(&format!("{}/bonding", sysfs)).exists() {
        return HostDeviceType::Bond;
    }
    if Path::new(&format!("{}/vlan", sysfs)).exists() {
        return HostDeviceType::Vlan;
    }
    if Path::new(&format!("{}/tun_flags", sysfs)).exists() {
        return HostDeviceType::Tap;
    }
    if is_macvtap(sysfs) {
        return HostDeviceType::Macvtap;
    }
    if name.starts_with("lxc") || name.starts_with("tap") || name.starts_with("veth") {
        return HostDeviceType::Veth;
    }
    if link_type == Some("veth") {
        return HostDeviceType::Veth;
    }
    if name.contains('@') {
        return HostDeviceType::Vlan;
    }
    if name.contains('.') && !name.starts_with("cilium") {
        // eth0.100 style VLAN
        let parts: Vec<&str> = name.split('.').collect();
        if parts.len() == 2 && parts[1].chars().all(|c| c.is_ascii_digit()) {
            return HostDeviceType::Vlan;
        }
    }
    HostDeviceType::Physical
}

fn is_macvtap(sysfs: &str) -> bool {
    let uevent = format!("{}/uevent", sysfs);
    std::fs::read_to_string(&uevent)
        .map(|s| s.contains("DEVTYPE=macvtap") || s.contains("MACVTAP"))
        .unwrap_or(false)
}

fn parse_vlan_info(name: &str, sysfs: &str, kind: HostDeviceType) -> (Option<u16>, Option<String>) {
    if kind != HostDeviceType::Vlan {
        return (None, None);
    }
    if let Ok(id) = std::fs::read_to_string(format!("{}/vlan/vlanid", sysfs)) {
        if let Ok(vid) = id.trim().parse::<u16>() {
            let parent = std::fs::read_to_string(format!("{}/vlan/vlan_parent", sysfs))
                .ok()
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty());
            return (Some(vid), parent);
        }
    }
    if let Some((base, vid)) = name.split_once('.') {
        if let Ok(id) = vid.parse::<u16>() {
            return (Some(id), Some(base.to_string()));
        }
    }
    if let Some((vlan, parent)) = name.split_once('@') {
        if let Some(id_str) = vlan.strip_prefix("vlan") {
            if let Ok(id) = id_str.parse::<u16>() {
                return (Some(id), Some(parent.to_string()));
            }
        }
    }
    (None, None)
}

/// List `.link`, `.netdev`, `.network` files in a config directory.
pub fn list_systemd_network_files(config_dir: &str) -> Vec<String> {
    let mut files = Vec::new();
    if config_dir.contains("..") {
        return files;
    }
    if !input_guard::COMMAND_ARGS.contains(config_dir) {
        return files;
    }
    let Ok(entries) = std::fs::read_dir(config_dir) else {
        return files;
    };
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().to_string();
        if name.ends_with(".link") || name.ends_with(".netdev") || name.ends_with(".network") {
            files.push(name);
        }
    }
    files.sort();
    files
}
