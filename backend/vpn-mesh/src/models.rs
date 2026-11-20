// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: LicenseRef-Zyvor-Production-1.0

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use uuid::Uuid;

/// VPN network topology type.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(rename_all = "snake_case")]
pub enum VpnTopology {
    PointToPoint,
    HubSpoke,
    #[default]
    FullMesh,
}

/// Selects endpoints by matching labels (AND semantics).
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct LabelSelector {
    #[serde(default)]
    pub match_labels: HashMap<String, String>,
}

impl LabelSelector {
    pub fn matches(&self, labels: &HashMap<String, String>) -> bool {
        self.match_labels
            .iter()
            .all(|(k, v)| labels.get(k) == Some(v))
    }
}

fn default_keepalive() -> u16 {
    25
}

fn default_listen_port() -> u16 {
    51820
}

fn default_true() -> bool {
    true
}

/// A WireGuard peer configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VpnPeer {
    pub public_key: String,
    #[serde(default)]
    pub endpoint: Option<String>,
    pub allowed_ips: Vec<String>,
    #[serde(default = "default_keepalive")]
    pub persistent_keepalive: u16,
}

/// A single WireGuard tunnel definition.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VpnTunnel {
    pub id: Uuid,
    pub name: String,
    #[serde(default)]
    pub description: String,
    pub interface_name: String,
    #[serde(default = "default_listen_port")]
    pub listen_port: u16,
    pub address: String,
    pub private_key_ref: String,
    #[serde(default)]
    pub peers: Vec<VpnPeer>,
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default = "default_true")]
    pub managed: bool,
    pub created: DateTime<Utc>,
    pub updated: DateTime<Utc>,
}

/// A VPN network that auto-generates tunnels from VM selectors.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VpnNetwork {
    pub id: Uuid,
    pub name: String,
    #[serde(default)]
    pub description: String,
    pub selector: LabelSelector,
    pub subnet: String,
    #[serde(default)]
    pub topology: VpnTopology,
    #[serde(default = "default_listen_port")]
    pub listen_port: u16,
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default = "default_true")]
    pub managed: bool,
    pub created: DateTime<Utc>,
    pub updated: DateTime<Utc>,
}

/// API request to create a VPN tunnel.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateVpnTunnelRequest {
    pub name: String,
    #[serde(default)]
    pub description: String,
    pub interface_name: String,
    #[serde(default = "default_listen_port")]
    pub listen_port: u16,
    pub address: String,
    pub private_key_ref: String,
    #[serde(default)]
    pub peers: Vec<VpnPeer>,
    #[serde(default = "default_true")]
    pub enabled: bool,
}

/// API request to create a VPN network.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateVpnNetworkRequest {
    pub name: String,
    #[serde(default)]
    pub description: String,
    pub selector: LabelSelector,
    pub subnet: String,
    #[serde(default)]
    pub topology: VpnTopology,
    #[serde(default = "default_listen_port")]
    pub listen_port: u16,
    #[serde(default = "default_true")]
    pub enabled: bool,
}

/// A compiled WireGuard interface ready for enforcement.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompiledWgInterface {
    pub interface_name: String,
    pub listen_port: u16,
    pub address: String,
    pub private_key_ref: String,
    pub peers: Vec<CompiledWgPeer>,
}

/// A compiled WireGuard peer.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompiledWgPeer {
    pub public_key: String,
    pub endpoint: Option<String>,
    pub allowed_ips: Vec<String>,
    pub persistent_keepalive: u16,
}

/// Status report for a VPN tunnel.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VpnTunnelStatus {
    pub tunnel_id: Uuid,
    pub name: String,
    pub interface_name: String,
    pub peer_count: usize,
    pub enforced: bool,
}

/// Status report for a VPN network.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VpnNetworkStatus {
    pub network_id: Uuid,
    pub name: String,
    pub matching_vms: usize,
    pub generated_interfaces: usize,
    pub enforced: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_label_selector_matching() {
        let mut selector = LabelSelector::default();
        selector
            .match_labels
            .insert("role".to_string(), "vpn".to_string());

        let mut labels = HashMap::new();
        labels.insert("role".to_string(), "vpn".to_string());
        assert!(selector.matches(&labels));

        let empty = HashMap::new();
        assert!(!selector.matches(&empty));

        let empty_selector = LabelSelector::default();
        assert!(empty_selector.matches(&labels));
    }

    #[test]
    fn test_topology_default() {
        let net: VpnNetwork = serde_json::from_str(
            r#"{"id":"00000000-0000-0000-0000-000000000000","name":"test","selector":{},"subnet":"10.10.0.0/24","listen_port":51820,"enabled":true,"created":"2024-01-01T00:00:00Z","updated":"2024-01-01T00:00:00Z"}"#,
        ).unwrap();
        assert_eq!(net.topology, VpnTopology::FullMesh);
    }

    #[test]
    fn test_default_listen_port() {
        let tunnel: VpnTunnel = serde_json::from_str(
            r#"{"id":"00000000-0000-0000-0000-000000000000","name":"wg0","interface_name":"wg0","address":"10.0.0.1/24","private_key_ref":"key-ref","peers":[],"enabled":true,"created":"2024-01-01T00:00:00Z","updated":"2024-01-01T00:00:00Z"}"#,
        ).unwrap();
        assert_eq!(tunnel.listen_port, 51820);
    }

    #[test]
    fn test_default_keepalive() {
        let peer: VpnPeer =
            serde_json::from_str(r#"{"public_key":"abc123","allowed_ips":["10.0.0.0/24"]}"#)
                .unwrap();
        assert_eq!(peer.persistent_keepalive, 25);
    }

    #[test]
    fn test_tunnel_serde_roundtrip() {
        let tunnel = VpnTunnel {
            id: Uuid::new_v4(),
            name: "wg0".to_string(),
            description: "Test tunnel".to_string(),
            interface_name: "wg0".to_string(),
            listen_port: 51820,
            address: "10.0.0.1/24".to_string(),
            private_key_ref: "secret-ref".to_string(),
            peers: vec![VpnPeer {
                public_key: "peer-key".to_string(),
                endpoint: Some("1.2.3.4:51820".to_string()),
                allowed_ips: vec!["10.0.0.2/32".to_string()],
                persistent_keepalive: 25,
            }],
            enabled: true,
            managed: true,
            created: Utc::now(),
            updated: Utc::now(),
        };

        let json = serde_json::to_string(&tunnel).unwrap();
        let deserialized: VpnTunnel = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.name, "wg0");
        assert_eq!(deserialized.peers.len(), 1);
        assert_eq!(deserialized.peers[0].public_key, "peer-key");
    }

    #[test]
    fn test_vpn_peer_serde() {
        let peer = VpnPeer {
            public_key: "test-key".to_string(),
            endpoint: None,
            allowed_ips: vec!["10.0.0.0/24".to_string(), "10.0.1.0/24".to_string()],
            persistent_keepalive: 30,
        };

        let json = serde_json::to_string(&peer).unwrap();
        let deserialized: VpnPeer = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.allowed_ips.len(), 2);
        assert!(deserialized.endpoint.is_none());
        assert_eq!(deserialized.persistent_keepalive, 30);
    }
}
