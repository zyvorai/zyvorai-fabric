// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: LicenseRef-Zyvor-Production-2.0

use std::collections::HashMap;

use crate::models::*;

/// A snapshot of a VM's state for VPN compilation.
pub struct VMSnapshot {
    pub name: String,
    pub labels: HashMap<String, String>,
    pub ip: Option<String>,
}

/// Compiles VPN tunnels and networks into WireGuard interface configs.
pub struct TunnelCompiler;

impl Default for TunnelCompiler {
    fn default() -> Self {
        Self::new()
    }
}

impl TunnelCompiler {
    pub fn new() -> Self {
        Self
    }

    /// Compile a single tunnel into a WireGuard interface config.
    pub fn compile_tunnel(&self, tunnel: &VpnTunnel) -> Option<CompiledWgInterface> {
        if !tunnel.enabled {
            return None;
        }

        let peers = tunnel
            .peers
            .iter()
            .map(|p| CompiledWgPeer {
                public_key: p.public_key.clone(),
                endpoint: p.endpoint.clone(),
                allowed_ips: p.allowed_ips.clone(),
                persistent_keepalive: p.persistent_keepalive,
            })
            .collect();

        Some(CompiledWgInterface {
            interface_name: tunnel.interface_name.clone(),
            listen_port: tunnel.listen_port,
            address: tunnel.address.clone(),
            private_key_ref: tunnel.private_key_ref.clone(),
            peers,
        })
    }

    /// Compile a VPN network against VM state, generating interfaces per topology.
    pub fn compile_network(
        &self,
        network: &VpnNetwork,
        all_vms: &[VMSnapshot],
    ) -> Vec<CompiledWgInterface> {
        if !network.enabled {
            return vec![];
        }

        let matching_vms: Vec<&VMSnapshot> = all_vms
            .iter()
            .filter(|vm| network.selector.matches(&vm.labels) && vm.ip.is_some())
            .collect();

        if matching_vms.is_empty() {
            return vec![];
        }

        let subnet_base = network.subnet.split('/').next().unwrap_or("10.10.0.0");

        match network.topology {
            VpnTopology::FullMesh => self.build_full_mesh(&matching_vms, subnet_base, network),
            VpnTopology::HubSpoke => self.build_hub_spoke(&matching_vms, subnet_base, network),
            VpnTopology::PointToPoint => {
                self.build_point_to_point(&matching_vms, subnet_base, network)
            }
        }
    }

    /// Compile all tunnels and networks.
    pub fn compile_all(
        &self,
        tunnels: &[VpnTunnel],
        networks: &[VpnNetwork],
        all_vms: &[VMSnapshot],
    ) -> Vec<CompiledWgInterface> {
        let mut interfaces = Vec::new();

        for tunnel in tunnels {
            if let Some(iface) = self.compile_tunnel(tunnel) {
                interfaces.push(iface);
            }
        }

        for network in networks {
            interfaces.extend(self.compile_network(network, all_vms));
        }

        interfaces
    }

    /// Build full mesh: every VM peers with every other VM.
    fn build_full_mesh(
        &self,
        vms: &[&VMSnapshot],
        subnet_base: &str,
        network: &VpnNetwork,
    ) -> Vec<CompiledWgInterface> {
        let base_parts: Vec<&str> = subnet_base.split('.').collect();
        let base_prefix = if base_parts.len() >= 3 {
            format!("{}.{}.{}", base_parts[0], base_parts[1], base_parts[2])
        } else {
            subnet_base.to_string()
        };

        let mut interfaces = Vec::new();

        for (i, vm) in vms.iter().enumerate() {
            let addr = format!("{}.{}/24", base_prefix, i + 1);
            let iface_name = format!("wg-{}", vm.name);

            let peers: Vec<CompiledWgPeer> = vms
                .iter()
                .enumerate()
                .filter(|(j, _)| *j != i)
                .map(|(j, peer_vm)| CompiledWgPeer {
                    public_key: format!("pubkey-{}", peer_vm.name),
                    endpoint: peer_vm
                        .ip
                        .as_ref()
                        .map(|ip| format!("{}:{}", ip, network.listen_port)),
                    allowed_ips: vec![format!("{}.{}/32", base_prefix, j + 1)],
                    persistent_keepalive: 25,
                })
                .collect();

            interfaces.push(CompiledWgInterface {
                interface_name: iface_name,
                listen_port: network.listen_port,
                address: addr,
                private_key_ref: format!("privkey-{}", vm.name),
                peers,
            });
        }

        interfaces
    }

    /// Build hub-spoke: first VM is hub, rest are spokes peering only with hub.
    fn build_hub_spoke(
        &self,
        vms: &[&VMSnapshot],
        subnet_base: &str,
        network: &VpnNetwork,
    ) -> Vec<CompiledWgInterface> {
        if vms.is_empty() {
            return vec![];
        }

        let base_parts: Vec<&str> = subnet_base.split('.').collect();
        let base_prefix = if base_parts.len() >= 3 {
            format!("{}.{}.{}", base_parts[0], base_parts[1], base_parts[2])
        } else {
            subnet_base.to_string()
        };

        let mut interfaces = Vec::new();
        let hub = vms[0];

        // Hub interface: peers with all spokes
        let hub_peers: Vec<CompiledWgPeer> = vms
            .iter()
            .enumerate()
            .skip(1)
            .map(|(j, spoke)| CompiledWgPeer {
                public_key: format!("pubkey-{}", spoke.name),
                endpoint: spoke
                    .ip
                    .as_ref()
                    .map(|ip| format!("{}:{}", ip, network.listen_port)),
                allowed_ips: vec![format!("{}.{}/32", base_prefix, j + 1)],
                persistent_keepalive: 25,
            })
            .collect();

        interfaces.push(CompiledWgInterface {
            interface_name: format!("wg-{}", hub.name),
            listen_port: network.listen_port,
            address: format!("{}.1/24", base_prefix),
            private_key_ref: format!("privkey-{}", hub.name),
            peers: hub_peers,
        });

        // Spoke interfaces: only peer with hub
        for (i, spoke) in vms.iter().enumerate().skip(1) {
            interfaces.push(CompiledWgInterface {
                interface_name: format!("wg-{}", spoke.name),
                listen_port: network.listen_port,
                address: format!("{}.{}/24", base_prefix, i + 1),
                private_key_ref: format!("privkey-{}", spoke.name),
                peers: vec![CompiledWgPeer {
                    public_key: format!("pubkey-{}", hub.name),
                    endpoint: hub
                        .ip
                        .as_ref()
                        .map(|ip| format!("{}:{}", ip, network.listen_port)),
                    allowed_ips: vec![format!("{}.0/24", base_prefix)],
                    persistent_keepalive: 25,
                }],
            });
        }

        interfaces
    }

    /// Build point-to-point: first two VMs only.
    fn build_point_to_point(
        &self,
        vms: &[&VMSnapshot],
        subnet_base: &str,
        network: &VpnNetwork,
    ) -> Vec<CompiledWgInterface> {
        if vms.len() < 2 {
            return vec![];
        }

        let base_parts: Vec<&str> = subnet_base.split('.').collect();
        let base_prefix = if base_parts.len() >= 3 {
            format!("{}.{}.{}", base_parts[0], base_parts[1], base_parts[2])
        } else {
            subnet_base.to_string()
        };

        let vm_a = vms[0];
        let vm_b = vms[1];

        vec![
            CompiledWgInterface {
                interface_name: format!("wg-{}", vm_a.name),
                listen_port: network.listen_port,
                address: format!("{}.1/24", base_prefix),
                private_key_ref: format!("privkey-{}", vm_a.name),
                peers: vec![CompiledWgPeer {
                    public_key: format!("pubkey-{}", vm_b.name),
                    endpoint: vm_b
                        .ip
                        .as_ref()
                        .map(|ip| format!("{}:{}", ip, network.listen_port)),
                    allowed_ips: vec![format!("{}.2/32", base_prefix)],
                    persistent_keepalive: 25,
                }],
            },
            CompiledWgInterface {
                interface_name: format!("wg-{}", vm_b.name),
                listen_port: network.listen_port,
                address: format!("{}.2/24", base_prefix),
                private_key_ref: format!("privkey-{}", vm_b.name),
                peers: vec![CompiledWgPeer {
                    public_key: format!("pubkey-{}", vm_a.name),
                    endpoint: vm_a
                        .ip
                        .as_ref()
                        .map(|ip| format!("{}:{}", ip, network.listen_port)),
                    allowed_ips: vec![format!("{}.1/32", base_prefix)],
                    persistent_keepalive: 25,
                }],
            },
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use uuid::Uuid;

    fn make_vm(name: &str, labels: &[(&str, &str)], ip: Option<&str>) -> VMSnapshot {
        VMSnapshot {
            name: name.to_string(),
            labels: labels
                .iter()
                .map(|(k, v)| (k.to_string(), v.to_string()))
                .collect(),
            ip: ip.map(|s| s.to_string()),
        }
    }

    fn make_tunnel(name: &str, enabled: bool) -> VpnTunnel {
        VpnTunnel {
            id: Uuid::new_v4(),
            name: name.to_string(),
            description: String::new(),
            interface_name: format!("wg-{}", name),
            listen_port: 51820,
            address: "10.0.0.1/24".to_string(),
            private_key_ref: "key-ref".to_string(),
            peers: vec![VpnPeer {
                public_key: "peer-pubkey".to_string(),
                endpoint: Some("1.2.3.4:51820".to_string()),
                allowed_ips: vec!["10.0.0.2/32".to_string()],
                persistent_keepalive: 25,
            }],
            enabled,
            managed: true,
            created: Utc::now(),
            updated: Utc::now(),
        }
    }

    fn make_network(name: &str, selector: &[(&str, &str)], topology: VpnTopology) -> VpnNetwork {
        VpnNetwork {
            id: Uuid::new_v4(),
            name: name.to_string(),
            description: String::new(),
            selector: LabelSelector {
                match_labels: selector
                    .iter()
                    .map(|(k, v)| (k.to_string(), v.to_string()))
                    .collect(),
            },
            subnet: "10.10.0.0/24".to_string(),
            topology,
            listen_port: 51820,
            enabled: true,
            managed: true,
            created: Utc::now(),
            updated: Utc::now(),
        }
    }

    #[test]
    fn test_compile_single_tunnel() {
        let compiler = TunnelCompiler::new();
        let tunnel = make_tunnel("wg0", true);

        let result = compiler.compile_tunnel(&tunnel);
        assert!(result.is_some());
        let iface = result.unwrap();
        assert_eq!(iface.interface_name, "wg-wg0");
        assert_eq!(iface.listen_port, 51820);
        assert_eq!(iface.peers.len(), 1);
        assert_eq!(iface.peers[0].public_key, "peer-pubkey");
    }

    #[test]
    fn test_disabled_tunnel() {
        let compiler = TunnelCompiler::new();
        let tunnel = make_tunnel("wg0", false);

        let result = compiler.compile_tunnel(&tunnel);
        assert!(result.is_none());
    }

    #[test]
    fn test_full_mesh() {
        let compiler = TunnelCompiler::new();
        let vms = vec![
            make_vm("vm-a", &[("role", "vpn")], Some("192.168.1.10")),
            make_vm("vm-b", &[("role", "vpn")], Some("192.168.1.11")),
            make_vm("vm-c", &[("role", "vpn")], Some("192.168.1.12")),
        ];
        let network = make_network("mesh", &[("role", "vpn")], VpnTopology::FullMesh);

        let interfaces = compiler.compile_network(&network, &vms);
        assert_eq!(interfaces.len(), 3);
        // Each VM should peer with 2 others
        assert_eq!(interfaces[0].peers.len(), 2);
        assert_eq!(interfaces[1].peers.len(), 2);
        assert_eq!(interfaces[2].peers.len(), 2);
    }

    #[test]
    fn test_hub_spoke() {
        let compiler = TunnelCompiler::new();
        let vms = vec![
            make_vm("hub", &[("role", "vpn")], Some("192.168.1.10")),
            make_vm("spoke-1", &[("role", "vpn")], Some("192.168.1.11")),
            make_vm("spoke-2", &[("role", "vpn")], Some("192.168.1.12")),
        ];
        let network = make_network("hs", &[("role", "vpn")], VpnTopology::HubSpoke);

        let interfaces = compiler.compile_network(&network, &vms);
        assert_eq!(interfaces.len(), 3);
        // Hub peers with all spokes
        assert_eq!(interfaces[0].peers.len(), 2);
        // Spokes only peer with hub
        assert_eq!(interfaces[1].peers.len(), 1);
        assert_eq!(interfaces[2].peers.len(), 1);
    }

    #[test]
    fn test_no_matching_vms() {
        let compiler = TunnelCompiler::new();
        let vms = vec![make_vm("web-1", &[("role", "web")], Some("10.0.0.5"))];
        let network = make_network("mesh", &[("role", "vpn")], VpnTopology::FullMesh);

        let interfaces = compiler.compile_network(&network, &vms);
        assert!(interfaces.is_empty());
    }

    #[test]
    fn test_compile_all() {
        let compiler = TunnelCompiler::new();
        let tunnels = vec![make_tunnel("wg0", true)];
        let vms = vec![
            make_vm("vm-a", &[("role", "vpn")], Some("10.0.0.1")),
            make_vm("vm-b", &[("role", "vpn")], Some("10.0.0.2")),
        ];
        let networks = vec![make_network(
            "mesh",
            &[("role", "vpn")],
            VpnTopology::FullMesh,
        )];

        let result = compiler.compile_all(&tunnels, &networks, &vms);
        // 1 tunnel + 2 full mesh interfaces
        assert_eq!(result.len(), 3);
    }

    #[test]
    fn test_point_to_point() {
        let compiler = TunnelCompiler::new();
        let vms = vec![
            make_vm("vm-a", &[("role", "vpn")], Some("192.168.1.10")),
            make_vm("vm-b", &[("role", "vpn")], Some("192.168.1.11")),
            make_vm("vm-c", &[("role", "vpn")], Some("192.168.1.12")),
        ];
        let network = make_network("p2p", &[("role", "vpn")], VpnTopology::PointToPoint);

        let interfaces = compiler.compile_network(&network, &vms);
        // Only first two VMs
        assert_eq!(interfaces.len(), 2);
        assert_eq!(interfaces[0].peers.len(), 1);
        assert_eq!(interfaces[1].peers.len(), 1);
    }

    #[test]
    fn test_empty_peers() {
        let compiler = TunnelCompiler::new();
        let mut tunnel = make_tunnel("wg0", true);
        tunnel.peers = vec![];

        let result = compiler.compile_tunnel(&tunnel);
        assert!(result.is_some());
        assert!(result.unwrap().peers.is_empty());
    }
}
