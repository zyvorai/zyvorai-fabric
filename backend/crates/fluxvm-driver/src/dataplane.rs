// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: LicenseRef-Zyvor-Production-1.0

//! `VmDataplaneDriver` backed by FluxVM Network Fabric (schema v4)
//! (`/v1/vms/{id}/network/…` and `/v1/network/{groups,cnp,…}`).

use anyhow::Result;
use async_trait::async_trait;
use zyvor_fabric_driver_core::{
    CiliumEndpointView, DataplaneHealth, DataplaneStats, DataplaneStatus, FlowRecord, IdentityInfo,
    IpcacheEntry, NetworkServiceSpec, NetworkServiceStatus, SecurityGroup, VmDataplaneDriver,
    VmNetworkPolicy,
};
use zyvor_fabric_fluxvm_client as client;

use crate::FluxVmDriver;

fn to_policy(p: client::VmNetworkPolicy) -> VmNetworkPolicy {
    VmNetworkPolicy {
        default_allow: p.default_allow,
        allow_cidrs: p.allow_cidrs,
        allow_ports: p.allow_ports,
        max_egress_mbps: p.max_egress_mbps,
        max_egress_pps: p.max_egress_pps,
        sample_rate: p.sample_rate,
        deny_cidrs: p.deny_cidrs,
        allow_icmp: p.allow_icmp,
        groups: p.groups,
        labels: p.labels,
        allow_fqdns: p.allow_fqdns,
        entities: p.entities,
        audit_mode: p.audit_mode,
        deny_udp: p.deny_udp,
    }
}

fn from_policy(p: &VmNetworkPolicy) -> client::VmNetworkPolicy {
    client::VmNetworkPolicy {
        default_allow: p.default_allow,
        allow_cidrs: p.allow_cidrs.clone(),
        allow_ports: p.allow_ports.clone(),
        max_egress_mbps: p.max_egress_mbps,
        max_egress_pps: p.max_egress_pps,
        sample_rate: p.sample_rate,
        deny_cidrs: p.deny_cidrs.clone(),
        allow_icmp: p.allow_icmp,
        groups: p.groups.clone(),
        labels: p.labels.clone(),
        allow_fqdns: p.allow_fqdns.clone(),
        entities: p.entities.clone(),
        audit_mode: p.audit_mode,
        deny_udp: p.deny_udp,
    }
}

fn to_status(s: client::DataplaneStatus) -> DataplaneStatus {
    DataplaneStatus {
        mode: s.mode,
        required: s.required,
        attached: s.attached,
        interface: s.interface,
        identity: s.identity,
        pin_dir: s.pin_dir,
        schema_version: s.schema_version,
        schema_compatible: s.schema_compatible,
        policy_synced: s.policy_synced,
        pod_ingress_required: s.pod_ingress_required,
        pod_ingress_attached: s.pod_ingress_attached,
        policy: to_policy(s.policy),
    }
}

fn to_stats(s: client::DataplaneStats) -> DataplaneStats {
    DataplaneStats {
        allowed_packets: s.allowed_packets,
        allowed_bytes: s.allowed_bytes,
        dropped_packets: s.dropped_packets,
        dropped_bytes: s.dropped_bytes,
        pod_policy: s
            .pod_policy
            .map(|p| zyvor_fabric_driver_core::PodPolicyStats {
                egress: zyvor_fabric_driver_core::PodDirectionCounters {
                    allowed: p.egress.allowed,
                    dropped: p.egress.dropped,
                    audited: p.egress.audited,
                },
                ingress: zyvor_fabric_driver_core::PodDirectionCounters {
                    allowed: p.ingress.allowed,
                    dropped: p.ingress.dropped,
                    audited: p.ingress.audited,
                },
                directional: p.directional,
            }),
    }
}

#[cfg(test)]
mod mapping_tests {
    use super::*;
    use zyvor_fabric_fluxvm_client as client;

    fn sample_policy() -> client::VmNetworkPolicy {
        client::VmNetworkPolicy {
            default_allow: false,
            allow_cidrs: vec![],
            allow_ports: vec![],
            max_egress_mbps: None,
            max_egress_pps: None,
            sample_rate: 0,
            deny_cidrs: vec![],
            allow_icmp: true,
            groups: vec![],
            labels: vec![],
            allow_fqdns: vec![],
            entities: vec![],
            audit_mode: false,
            deny_udp: false,
        }
    }

    #[test]
    fn deny_udp_survives_both_mappings() {
        let mut client_policy = sample_policy();
        client_policy.deny_udp = true;
        let driver_policy = to_policy(client_policy);
        assert!(driver_policy.deny_udp);
        assert!(from_policy(&driver_policy).deny_udp);

        let off = to_policy(sample_policy());
        assert!(!off.deny_udp);
        assert!(!from_policy(&off).deny_udp);
    }

    #[test]
    fn to_status_copies_pod_ingress_fields() {
        let mapped = to_status(client::DataplaneStatus {
            mode: "ebpf".into(),
            required: true,
            attached: true,
            interface: Some("tap0".into()),
            identity: 9,
            pin_dir: Some("/pin".into()),
            schema_version: Some(9),
            schema_compatible: true,
            policy_synced: true,
            pod_ingress_required: true,
            pod_ingress_attached: false,
            policy: sample_policy(),
        });
        assert!(mapped.pod_ingress_required);
        assert!(!mapped.pod_ingress_attached);
        assert_eq!(mapped.schema_version, Some(9));
    }

    #[test]
    fn to_stats_copies_pod_policy() {
        let mapped = to_stats(client::DataplaneStats {
            allowed_packets: 1,
            allowed_bytes: 2,
            dropped_packets: 3,
            dropped_bytes: 4,
            pod_policy: Some(client::PodPolicyStats {
                egress: client::PodDirectionCounters {
                    allowed: 5,
                    dropped: 6,
                    audited: 7,
                },
                ingress: client::PodDirectionCounters {
                    allowed: 8,
                    dropped: 9,
                    audited: 10,
                },
                directional: true,
            }),
        });
        let pp = mapped.pod_policy.expect("pod_policy");
        assert!(pp.directional);
        assert_eq!(pp.egress.allowed, 5);
        assert_eq!(pp.ingress.audited, 10);
    }
}

fn to_flow(f: client::FlowRecord) -> FlowRecord {
    FlowRecord {
        identity: f.identity,
        family: f.family,
        source: f.source,
        destination: f.destination,
        source_port: f.source_port,
        destination_port: f.destination_port,
        protocol: f.protocol,
        verdict: f.verdict,
        packets: f.packets,
        bytes: f.bytes,
        last_seen_ns: f.last_seen_ns,
    }
}

fn to_group(g: client::SecurityGroup) -> SecurityGroup {
    SecurityGroup {
        name: g.name,
        labels: g.labels,
        policy: to_policy(g.policy),
        identity: g.identity,
        priority: g.priority,
        description: g.description,
    }
}

fn from_group(g: &SecurityGroup) -> client::SecurityGroup {
    client::SecurityGroup {
        name: g.name.clone(),
        labels: g.labels.clone(),
        policy: from_policy(&g.policy),
        identity: g.identity,
        priority: g.priority,
        description: g.description.clone(),
    }
}

fn to_service(s: client::NetworkServiceSpec) -> NetworkServiceSpec {
    use zyvor_fabric_driver_core::{
        NetworkBackendState, NetworkHealthCheckKind, NetworkServiceAlgorithm,
        NetworkServiceBackend, NetworkServiceExposure, NetworkServiceHealthCheck,
        NetworkServiceMode, NetworkServiceProtocol,
    };
    NetworkServiceSpec {
        name: s.name,
        vip: s.vip,
        port: s.port,
        protocol: match s.protocol {
            client::NetworkServiceProtocol::Tcp => NetworkServiceProtocol::Tcp,
            client::NetworkServiceProtocol::Udp => NetworkServiceProtocol::Udp,
        },
        algorithm: match s.algorithm {
            client::NetworkServiceAlgorithm::Maglev => NetworkServiceAlgorithm::Maglev,
        },
        mode: match s.mode {
            client::NetworkServiceMode::Nat => NetworkServiceMode::Nat,
            client::NetworkServiceMode::Dsr => NetworkServiceMode::Dsr,
        },
        exposure: match s.exposure {
            client::NetworkServiceExposure::EastWest => NetworkServiceExposure::EastWest,
            client::NetworkServiceExposure::NorthSouth => NetworkServiceExposure::NorthSouth,
            client::NetworkServiceExposure::Both => NetworkServiceExposure::Both,
        },
        backends: s
            .backends
            .into_iter()
            .map(|b| NetworkServiceBackend {
                address: b.address,
                port: b.port,
                weight: b.weight,
                enabled: b.enabled,
                state: match b.state {
                    client::NetworkBackendState::Ready => NetworkBackendState::Ready,
                    client::NetworkBackendState::Draining => NetworkBackendState::Draining,
                    client::NetworkBackendState::Unhealthy => NetworkBackendState::Unhealthy,
                },
                drain_until_unix_ms: b.drain_until_unix_ms,
            })
            .collect(),
        maglev_table_size: s.maglev_table_size,
        snat_address: s.snat_address,
        health_check: s.health_check.map(|h| NetworkServiceHealthCheck {
            kind: match h.kind {
                client::NetworkHealthCheckKind::Tcp => NetworkHealthCheckKind::Tcp,
            },
            timeout_ms: h.timeout_ms,
            unhealthy_threshold: h.unhealthy_threshold,
            healthy_threshold: h.healthy_threshold,
        }),
        advertise: s.advertise,
        max_egress_mbps: s.max_egress_mbps,
        flow_sample_rate: s.flow_sample_rate,
        host_routing: s.host_routing,
        // Fabric-only multi-site fencing; FluxVM does not persist these.
        site_id: None,
        route_domain: None,
    }
}

fn from_service(s: &NetworkServiceSpec) -> client::NetworkServiceSpec {
    use zyvor_fabric_driver_core::{
        NetworkBackendState, NetworkHealthCheckKind, NetworkServiceExposure, NetworkServiceMode,
        NetworkServiceProtocol,
    };
    client::NetworkServiceSpec {
        name: s.name.clone(),
        vip: s.vip.clone(),
        port: s.port,
        protocol: match s.protocol {
            NetworkServiceProtocol::Tcp => client::NetworkServiceProtocol::Tcp,
            NetworkServiceProtocol::Udp => client::NetworkServiceProtocol::Udp,
        },
        algorithm: client::NetworkServiceAlgorithm::Maglev,
        mode: match s.mode {
            NetworkServiceMode::Nat => client::NetworkServiceMode::Nat,
            NetworkServiceMode::Dsr => client::NetworkServiceMode::Dsr,
        },
        exposure: match s.exposure {
            NetworkServiceExposure::EastWest => client::NetworkServiceExposure::EastWest,
            NetworkServiceExposure::NorthSouth => client::NetworkServiceExposure::NorthSouth,
            NetworkServiceExposure::Both => client::NetworkServiceExposure::Both,
        },
        backends: s
            .backends
            .iter()
            .map(|b| client::NetworkServiceBackend {
                address: b.address.clone(),
                port: b.port,
                weight: b.weight,
                enabled: b.enabled,
                state: match b.state {
                    NetworkBackendState::Ready => client::NetworkBackendState::Ready,
                    NetworkBackendState::Draining => client::NetworkBackendState::Draining,
                    NetworkBackendState::Unhealthy => client::NetworkBackendState::Unhealthy,
                },
                drain_until_unix_ms: b.drain_until_unix_ms,
            })
            .collect(),
        maglev_table_size: s.maglev_table_size,
        snat_address: s.snat_address.clone(),
        health_check: s
            .health_check
            .as_ref()
            .map(|h| client::NetworkServiceHealthCheck {
                kind: match h.kind {
                    NetworkHealthCheckKind::Tcp => client::NetworkHealthCheckKind::Tcp,
                },
                timeout_ms: h.timeout_ms,
                unhealthy_threshold: h.unhealthy_threshold,
                healthy_threshold: h.healthy_threshold,
            }),
        advertise: s.advertise,
        max_egress_mbps: s.max_egress_mbps,
        flow_sample_rate: s.flow_sample_rate,
        host_routing: s.host_routing,
    }
}

fn to_service_status(s: client::NetworkServiceStatus) -> NetworkServiceStatus {
    use zyvor_fabric_driver_core::{NetworkServiceExposure, NetworkServiceMode};
    NetworkServiceStatus {
        schema_version: s.schema_version,
        service_id: s.service_id,
        name: s.name,
        active_backends: s.active_backends,
        maglev_table_size: s.maglev_table_size,
        family: s.family,
        mode: s.mode.map(|m| match m {
            client::NetworkServiceMode::Nat => NetworkServiceMode::Nat,
            client::NetworkServiceMode::Dsr => NetworkServiceMode::Dsr,
        }),
        exposure: s.exposure.map(|e| match e {
            client::NetworkServiceExposure::EastWest => NetworkServiceExposure::EastWest,
            client::NetworkServiceExposure::NorthSouth => NetworkServiceExposure::NorthSouth,
            client::NetworkServiceExposure::Both => NetworkServiceExposure::Both,
        }),
        snat_address: s.snat_address,
    }
}

fn to_health(h: client::DataplaneHealth) -> DataplaneHealth {
    DataplaneHealth {
        mode: h.mode,
        required: h.required,
        default_allow: h.default_allow,
        bpf_object_present: h.bpf_object_present,
        pin_root_present: h.pin_root_present,
        bpffs_present: h.bpffs_present,
        cilium_socket_present: h.cilium_socket_present,
        groups: h.groups,
        policies: h.policies,
        ipcache_entries: h.ipcache_entries,
        ok: h.ok,
        notes: h.notes,
    }
}

fn to_ipcache(e: client::IpcacheEntry) -> IpcacheEntry {
    IpcacheEntry {
        ip: e.ip,
        identity: e.identity,
        vm_id: e.vm_id.to_string(),
    }
}

fn to_identity(i: client::IdentityInfo) -> IdentityInfo {
    IdentityInfo {
        id: i.id,
        name: i.name,
        labels: i.labels,
        reserved: i.reserved,
    }
}

#[async_trait]
impl VmDataplaneDriver for FluxVmDriver {
    async fn dataplane_status(&self, name: &str) -> Result<DataplaneStatus> {
        let vm = self.resolve(name).await?;
        Ok(to_status(self.client.network_status(vm.id).await?))
    }

    async fn get_dataplane_policy(&self, name: &str) -> Result<VmNetworkPolicy> {
        let vm = self.resolve(name).await?;
        Ok(to_policy(self.client.get_network_policy(vm.id).await?))
    }

    async fn set_dataplane_policy(
        &self,
        name: &str,
        policy: &VmNetworkPolicy,
    ) -> Result<VmNetworkPolicy> {
        let vm = self.resolve(name).await?;
        Ok(to_policy(
            self.client
                .set_network_policy(vm.id, &from_policy(policy))
                .await?,
        ))
    }

    async fn dataplane_stats(&self, name: &str) -> Result<DataplaneStats> {
        let vm = self.resolve(name).await?;
        Ok(to_stats(self.client.network_stats(vm.id).await?))
    }

    async fn dataplane_flows(&self, name: &str, limit: Option<usize>) -> Result<Vec<FlowRecord>> {
        let vm = self.resolve(name).await?;
        Ok(self
            .client
            .network_flows(vm.id, limit)
            .await?
            .into_iter()
            .map(to_flow)
            .collect())
    }

    async fn dataplane_drop_reasons(
        &self,
        name: &str,
        limit: Option<usize>,
    ) -> Result<Vec<serde_json::Value>> {
        let vm = self.resolve(name).await?;
        let rows = self.client.network_drop_reasons(vm.id, limit).await?;
        Ok(rows
            .into_iter()
            .filter_map(|r| serde_json::to_value(r).ok())
            .collect())
    }

    async fn get_pod_network_policy(&self, name: &str) -> Result<Option<serde_json::Value>> {
        let vm = self.resolve(name).await?;
        match self.client.get_pod_network_policy(vm.id).await? {
            None => Ok(None),
            Some(p) => Ok(Some(serde_json::to_value(p)?)),
        }
    }

    async fn set_pod_network_policy(&self, name: &str, policy: &serde_json::Value) -> Result<()> {
        let vm = self.resolve(name).await?;
        let policy: client::PodNetworkPolicy = serde_json::from_value(policy.clone())?;
        self.client.set_pod_network_policy(vm.id, &policy).await
    }

    async fn delete_pod_network_policy(&self, name: &str) -> Result<()> {
        let vm = self.resolve(name).await?;
        self.client.delete_pod_network_policy(vm.id).await
    }

    async fn dataplane_effective(&self, name: &str) -> Result<serde_json::Value> {
        let vm = self.resolve(name).await?;
        self.client.network_effective(vm.id).await
    }

    async fn dataplane_list_groups(&self) -> Result<Vec<SecurityGroup>> {
        Ok(self
            .client
            .list_network_groups()
            .await?
            .into_iter()
            .map(to_group)
            .collect())
    }

    async fn dataplane_get_group(&self, name: &str) -> Result<SecurityGroup> {
        Ok(to_group(self.client.get_network_group(name).await?))
    }

    async fn dataplane_upsert_group(&self, group: &SecurityGroup) -> Result<SecurityGroup> {
        Ok(to_group(
            self.client.upsert_network_group(&from_group(group)).await?,
        ))
    }

    async fn dataplane_delete_group(&self, name: &str) -> Result<()> {
        self.client.delete_network_group(name).await
    }

    async fn dataplane_list_services(&self) -> Result<Vec<NetworkServiceSpec>> {
        Ok(self
            .client
            .list_network_services()
            .await?
            .into_iter()
            .map(to_service)
            .collect())
    }

    async fn dataplane_get_service(&self, name: &str) -> Result<NetworkServiceSpec> {
        Ok(to_service(self.client.get_network_service(name).await?))
    }

    async fn dataplane_upsert_service(
        &self,
        service: &NetworkServiceSpec,
    ) -> Result<NetworkServiceStatus> {
        Ok(to_service_status(
            self.client
                .upsert_network_service(&from_service(service))
                .await?,
        ))
    }

    async fn dataplane_delete_service(&self, name: &str) -> Result<()> {
        self.client.delete_network_service(name).await
    }

    async fn dataplane_list_cnp(&self) -> Result<serde_json::Value> {
        Ok(serde_json::json!({
            "items": self.client.list_cnp().await?
        }))
    }

    async fn dataplane_get_cnp(&self, name: &str) -> Result<serde_json::Value> {
        self.client.get_cnp(name).await
    }

    async fn dataplane_apply_cnp(&self, doc: &serde_json::Value) -> Result<serde_json::Value> {
        self.client.apply_cnp(doc).await
    }

    async fn dataplane_delete_cnp(&self, name: &str) -> Result<()> {
        self.client.delete_cnp(name).await
    }

    async fn dataplane_list_identities(&self) -> Result<Vec<IdentityInfo>> {
        Ok(self
            .client
            .list_identities()
            .await?
            .into_iter()
            .map(to_identity)
            .collect())
    }

    async fn dataplane_list_endpoints(&self) -> Result<Vec<CiliumEndpointView>> {
        Ok(self
            .client
            .list_endpoints()
            .await?
            .into_iter()
            .map(|e| CiliumEndpointView {
                id: e.id,
                uuid: e.uuid.to_string(),
                identity: e.identity,
                identity_source: e.identity_source,
                identity_labels: e.identity_labels,
                networking: e.networking,
                state: e.state,
                policy: e.policy,
            })
            .collect())
    }

    async fn dataplane_observe(&self) -> Result<serde_json::Value> {
        self.client.network_observe().await
    }

    async fn dataplane_hubble_flows(&self, limit: Option<usize>) -> Result<serde_json::Value> {
        match self.client.hubble_flows(limit, None, None).await {
            Ok(v) => Ok(v),
            Err(_) => Ok(serde_json::json!({"items": []})),
        }
    }

    async fn dataplane_health(&self) -> Result<DataplaneHealth> {
        Ok(to_health(self.client.network_health().await?))
    }

    async fn dataplane_ipcache(&self) -> Result<Vec<IpcacheEntry>> {
        Ok(self
            .client
            .network_ipcache()
            .await?
            .into_iter()
            .map(to_ipcache)
            .collect())
    }

    async fn dataplane_refresh_dns(&self) -> Result<usize> {
        self.client.refresh_fqdn_policies().await
    }
}
