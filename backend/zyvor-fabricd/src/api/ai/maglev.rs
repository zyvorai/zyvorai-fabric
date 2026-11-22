// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: LicenseRef-Zyvor-Production-2.0

//! Maglev ServiceSpec builder for AI inference endpoints (preview).

use std::net::IpAddr;

use zyvor_fabric_fluxvm_client::{
    NetworkServiceAlgorithm, NetworkServiceBackend, NetworkServiceExposure, NetworkServiceMode,
    NetworkServiceProtocol, NetworkServiceSpec,
};

use super::eligibility::replica_serving;
use super::types::InferenceReplica;

/// Split a replica address into an IP Maglev can load-balance and a TCP port.
///
/// FluxVM rejects `host:port` in `backends[].address`. Janus replicas store
/// `127.0.0.1:30818`; real VMs usually store a bare guest IP and use the
/// endpoint port. Non-IP hosts are omitted so the gateway can still proxy
/// them without a Maglev upsert.
pub fn backend_hostport(raw: &str, default_port: u16) -> Option<(String, u16)> {
    let raw = raw.trim();
    if raw.is_empty() {
        return None;
    }
    if let Ok(ip) = raw.parse::<IpAddr>() {
        return Some((ip.to_string(), default_port));
    }
    let (host, port_text) = raw.rsplit_once(':')?;
    if host.parse::<IpAddr>().is_err() {
        return None;
    }
    let port: u16 = port_text.parse().ok()?;
    if port == 0 {
        return None;
    }
    Some((host.to_string(), port))
}

/// Build an equal-weight Maglev service from ready replicas.
pub fn build_maglev_service_spec(
    name: &str,
    vip: &str,
    port: u16,
    replicas: &[InferenceReplica],
    max_egress_mbps: Option<u32>,
) -> NetworkServiceSpec {
    build_maglev_service_spec_weighted(name, vip, port, replicas, max_egress_mbps)
}

/// Build a Maglev service using each replica's `maglev_weight` (default 1).
pub fn build_maglev_service_spec_weighted(
    name: &str,
    vip: &str,
    port: u16,
    replicas: &[InferenceReplica],
    max_egress_mbps: Option<u32>,
) -> NetworkServiceSpec {
    let backends: Vec<NetworkServiceBackend> = replicas
        .iter()
        .filter(|r| replica_serving(r))
        .filter_map(|r| {
            let raw = r.address.as_ref()?;
            let (address, backend_port) = backend_hostport(raw, port)?;
            let weight = r.maglev_weight.unwrap_or(1).clamp(1, 32);
            Some(NetworkServiceBackend {
                address,
                port: backend_port,
                weight,
                enabled: true,
                state: Default::default(),
                drain_until_unix_ms: None,
            })
        })
        .collect();
    NetworkServiceSpec {
        name: name.to_string(),
        vip: vip.to_string(),
        port,
        protocol: NetworkServiceProtocol::Tcp,
        algorithm: NetworkServiceAlgorithm::Maglev,
        mode: NetworkServiceMode::Nat,
        exposure: NetworkServiceExposure::EastWest,
        backends,
        maglev_table_size: None,
        snat_address: None,
        health_check: None,
        advertise: false,
        max_egress_mbps,
        flow_sample_rate: 0,
        host_routing: false,
    }
}

/// Mark a backend draining by address (scale-down / delete path).
pub fn drain_backend(spec: &mut NetworkServiceSpec, address: &str) {
    let now_ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0);
    let target = backend_hostport(address, 0)
        .map(|(ip, _)| ip)
        .unwrap_or_else(|| address.to_string());
    for b in &mut spec.backends {
        if b.address == target || b.address == address {
            b.enabled = false;
            b.state = zyvor_fabric_fluxvm_client::NetworkBackendState::Draining;
            b.drain_until_unix_ms = Some(now_ms.saturating_add(30_000));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::ai::types::{InferenceReplica, ReplicaMetrics};
    use chrono::{Duration, Utc};

    fn fresh() -> Option<ReplicaMetrics> {
        Some(ReplicaMetrics {
            source: Some("vllm_prometheus".into()),
            scraped_at: Some(Utc::now()),
            ..Default::default()
        })
    }

    fn replica(
        name: &str,
        ready: bool,
        address: Option<&str>,
        draining: bool,
        weight: Option<u16>,
        metrics: Option<ReplicaMetrics>,
    ) -> InferenceReplica {
        InferenceReplica {
            replica_id: name.into(),
            ordinal: 0,
            vm_name: name.into(),
            bdf: "0000:01:00.0".into(),
            ready,
            address: address.map(str::to_string),
            metrics,
            maglev_weight: weight,
            site: None,
            cost_tier: None,
            draining,
            unhealthy_streak: 0,
            deployment: String::new(),
            revision: 0,
            model_digest: String::new(),
            profile_digest: String::new(),
            host: String::new(),
            generation: 0,
            lifecycle: String::new(),
            health: String::new(),
            created_at: None,
        }
    }

    #[test]
    fn equal_weights_only_ready_with_address() {
        let replicas = vec![
            replica("a", true, Some("10.0.0.1"), false, None, fresh()),
            replica("b", false, Some("10.0.0.2"), false, None, fresh()),
            replica("c", true, None, false, None, fresh()),
        ];
        let spec = build_maglev_service_spec("ai-llm", "10.96.0.50", 8000, &replicas, Some(1000));
        assert_eq!(spec.backends.len(), 1);
        assert_eq!(spec.backends[0].weight, 1);
        assert_eq!(spec.backends[0].address, "10.0.0.1");
        assert_eq!(spec.max_egress_mbps, Some(1000));
        assert!(matches!(spec.algorithm, NetworkServiceAlgorithm::Maglev));
    }

    #[test]
    fn weighted_uses_replica_maglev_weight() {
        let replicas = vec![replica(
            "a",
            true,
            Some("10.0.0.1"),
            false,
            Some(8),
            fresh(),
        )];
        let spec =
            build_maglev_service_spec_weighted("ai-llm", "10.96.0.50", 8000, &replicas, None);
        assert_eq!(spec.backends[0].weight, 8);
    }

    #[test]
    fn draining_backend_stays_out_of_the_spec() {
        let replicas = vec![replica("a", true, Some("10.0.0.1"), true, Some(1), fresh())];
        let spec = build_maglev_service_spec("ai-llm", "10.96.0.50", 8000, &replicas, None);
        assert!(spec.backends.is_empty());
    }

    #[test]
    fn stale_metrics_are_omitted() {
        let mut stale = fresh().unwrap();
        stale.scraped_at = Some(Utc::now() - Duration::seconds(120));
        let replicas = vec![replica(
            "a",
            true,
            Some("10.0.0.1"),
            false,
            Some(8),
            Some(stale),
        )];
        let spec = build_maglev_service_spec("ai-llm", "10.96.0.50", 8000, &replicas, None);
        assert!(spec.backends.is_empty());
    }

    #[test]
    fn drain_marks_backend() {
        let mut spec = build_maglev_service_spec(
            "ai-llm",
            "10.96.0.50",
            8000,
            &[replica("a", true, Some("10.0.0.1"), false, None, fresh())],
            None,
        );
        drain_backend(&mut spec, "10.0.0.1");
        assert!(!spec.backends[0].enabled);
        assert!(matches!(
            spec.backends[0].state,
            zyvor_fabric_fluxvm_client::NetworkBackendState::Draining
        ));
    }

    #[test]
    fn janus_hostport_becomes_ip_and_port() {
        let replicas = vec![replica(
            "j",
            true,
            Some("127.0.0.1:30818"),
            false,
            Some(1),
            fresh(),
        )];
        let spec = build_maglev_service_spec("janus-ep", "10.96.0.50", 8000, &replicas, None);
        assert_eq!(spec.backends.len(), 1);
        assert_eq!(spec.backends[0].address, "127.0.0.1");
        assert_eq!(spec.backends[0].port, 30818);
    }

    #[test]
    fn hostname_upstream_is_omitted_from_maglev() {
        let replicas = vec![replica(
            "j",
            true,
            Some("janus.lab:30818"),
            false,
            Some(1),
            fresh(),
        )];
        let spec = build_maglev_service_spec("janus-ep", "10.96.0.50", 8000, &replicas, None);
        assert!(spec.backends.is_empty());
    }
}
