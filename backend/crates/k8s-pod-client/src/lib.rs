// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: LicenseRef-Zyvor-Production-2.0

//! Narrow client for creating/observing the Kubernetes `Pod` objects that
//! back a fabric `ContainerGroup`. Deliberately thin: this wraps
//! `kube::Api<Pod>` CRUD only, not `kube::runtime`'s controller/watch
//! machinery — that belongs to `zyvor-fabricd-operator`, which reacts to
//! CRDs already applied to some cluster. This crate is the *outbound* side:
//! fabric reaching into a (possibly customer-owned) cluster it holds a
//! kubeconfig for, to place a Pod on the host its own placement decided on.

use std::collections::BTreeMap;

use anyhow::{Context, Result};
use k8s_openapi::api::core::v1::{
    Container, EnvVar, ExecAction, HTTPGetAction, HostPathVolumeSource, LocalObjectReference,
    Namespace, Pod, PodSpec, PodStatus, Probe, ResourceRequirements, TCPSocketAction, Toleration,
    Volume, VolumeMount as K8sVolumeMount,
};
use k8s_openapi::api::networking::v1::{
    IPBlock, NetworkPolicy, NetworkPolicyEgressRule, NetworkPolicyIngressRule, NetworkPolicyPeer,
    NetworkPolicyPort, NetworkPolicySpec as K8sNetworkPolicySpec,
};
use k8s_openapi::apimachinery::pkg::api::resource::Quantity;
use k8s_openapi::apimachinery::pkg::apis::meta::v1::{LabelSelector, ObjectMeta};
use k8s_openapi::apimachinery::pkg::util::intstr::IntOrString;
use kube::api::{Api, DeleteParams, PatchParams, PostParams};
use kube::config::{KubeConfigOptions, Kubeconfig};
use kube::{Client, Config};
use serde::Serialize;

/// The `RuntimeClass` FluxVM's Secure Containers shim registers as
/// (`deploy/containerd/runtimeclass.yaml` in the fluxvm repo).
pub const SECURE_CONTAINERS_RUNTIME_CLASS: &str = "fluxvm";

/// Taint/toleration pair placed on Secure-Containers-capable nodes so
/// kube-scheduler doesn't also place ordinary workloads on capacity
/// fabric's own placement (`predictive_drs`) is bin-packing.
pub const SECURE_CONTAINERS_TAINT_KEY: &str = "fluxvm.io/secure-containers";

#[derive(Debug, Clone)]
pub struct K8sPodClientConfig {
    /// Path to a kubeconfig file for the target cluster/site. `None` uses
    /// the ambient in-cluster or default kubeconfig context.
    pub kubeconfig_path: Option<String>,
    pub namespace: String,
}

impl Default for K8sPodClientConfig {
    fn default() -> Self {
        Self {
            kubeconfig_path: None,
            namespace: "default".to_string(),
        }
    }
}

pub struct PodVolumeMount {
    pub name: String,
    pub mount_path: String,
    pub read_only: bool,
}

/// `host_path` is the v1 volume backing (matches the host-path semantics
/// `VolumeMount.host` already has for VMs). A CSI/virtiofs-backed source is
/// a natural follow-up once ContainerGroup volumes need to move with a
/// rescheduled Pod rather than stay pinned to one host's local path.
pub struct PodVolumeSource {
    pub name: String,
    pub host_path: String,
}

/// What a probe actually checks. Covers the three most common Kubernetes
/// probe mechanisms; gRPC probes are a natural follow-up if a customer
/// asks for one, deliberately left out for a first version to keep the
/// fabric-facing API surface small.
pub enum ProbeCheck {
    Http { path: String, port: u16 },
    Tcp { port: u16 },
    Exec { command: Vec<String> },
}

/// Mirrors `k8s_openapi::api::core::v1::Probe`'s tunables directly — these
/// are the same knobs `kubectl`/a Kubernetes manifest would expose, no
/// fabric-specific defaults layered on top beyond what the Kubernetes API
/// itself already defaults server-side when a field is left `None`.
pub struct ProbeSpec {
    pub check: ProbeCheck,
    pub initial_delay_secs: Option<u32>,
    pub period_secs: Option<u32>,
    pub timeout_secs: Option<u32>,
    pub success_threshold: Option<u32>,
    pub failure_threshold: Option<u32>,
}

pub struct PodContainerSpec {
    pub name: String,
    pub image: String,
    pub command: Vec<String>,
    pub args: Vec<String>,
    pub env: Vec<(String, String)>,
    /// Millicores. Used as both request and limit (Guaranteed QoS) — the
    /// simplest correct choice for a first version; burstable requests can
    /// follow once ContainerGroup needs it.
    pub cpu_millis: u32,
    pub memory_mb: u64,
    pub volume_mounts: Vec<PodVolumeMount>,
    pub liveness_probe: Option<ProbeSpec>,
    pub readiness_probe: Option<ProbeSpec>,
}

pub struct PodRequest {
    pub name: String,
    /// The Kubernetes node to pin this Pod to — the host fabric's own
    /// `predictive_drs`-based placement already chose. Set as
    /// `spec.nodeName` directly, bypassing kube-scheduler for this Pod.
    pub node_name: String,
    pub labels: BTreeMap<String, String>,
    pub containers: Vec<PodContainerSpec>,
    pub volumes: Vec<PodVolumeSource>,
    /// "Always" | "Never" | "OnFailure".
    pub restart_policy: String,
    /// Names of `kubernetes.io/dockerconfigjson` Secrets (already present in
    /// the target namespace — this client doesn't create them) to pull the
    /// group's images with. Pod-level in the Kubernetes API, so it applies
    /// to every container in the Pod, not per-container.
    pub image_pull_secrets: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct PodStatusView {
    pub phase: Option<String>,
    pub pod_ip: Option<String>,
    pub node_name: Option<String>,
    pub uid: Option<String>,
    pub resource_version: Option<String>,
}

/// One `from`/`to` peer plus the ports it's allowed on, for one direction of
/// a `NetworkPolicy` rule. Peers are OR'd together (any one matching is
/// enough); an empty `peer_labels`/`cidr` set with a non-empty `ports` list
/// means "any source/destination, but only on these ports" (an absent
/// Kubernetes `from`/`to` field, not an empty-array "match nothing").
pub struct NetworkPolicyRule {
    /// Each entry becomes its own `podSelector` peer (OR'd with the others
    /// and with `cidr`) — e.g. `{"fabric.zyvor.dev/container-group": "web"}`
    /// to allow from another ContainerGroup's Pods in the same namespace.
    pub peer_label_selectors: Vec<BTreeMap<String, String>>,
    pub cidrs: Vec<String>,
    /// Empty means "all ports" for whatever peers this rule matches.
    pub ports: Vec<u16>,
}

/// A Kubernetes `NetworkPolicy`. `ingress`/`egress` being `None` leaves that
/// direction untouched by this policy (unrestricted, unless some *other*
/// NetworkPolicy in the namespace isolates it); `Some(vec![])` isolates
/// that direction with zero allowed rules (deny-all); `Some(rules)` allows
/// only what the rules describe.
pub struct NetworkPolicyRequest {
    pub name: String,
    /// Pods this policy applies to — a ContainerGroup's own
    /// `fabric.zyvor.dev/container-group` label in practice.
    pub pod_selector_labels: BTreeMap<String, String>,
    pub ingress: Option<Vec<NetworkPolicyRule>>,
    pub egress: Option<Vec<NetworkPolicyRule>>,
}

pub struct K8sPodClient {
    client: Client,
    /// Namespace used when a caller doesn't ask for a specific one — the
    /// pre-multi-tenancy default, still what an untenanted ContainerGroup
    /// lands in.
    default_namespace: String,
}

impl K8sPodClient {
    pub async fn connect(config: &K8sPodClientConfig) -> Result<Self> {
        let client = match &config.kubeconfig_path {
            Some(path) => {
                let kubeconfig = Kubeconfig::read_from(path)
                    .with_context(|| format!("reading kubeconfig at {path}"))?;
                let kube_config =
                    Config::from_custom_kubeconfig(kubeconfig, &KubeConfigOptions::default())
                        .await
                        .context("building kube client config from kubeconfig")?;
                Client::try_from(kube_config).context("constructing Kubernetes client")?
            }
            None => Client::try_default()
                .await
                .context("constructing Kubernetes client from ambient context")?,
        };
        Ok(Self {
            client,
            default_namespace: config.namespace.clone(),
        })
    }

    fn pods_in(&self, namespace: Option<&str>) -> Api<Pod> {
        Api::namespaced(
            self.client.clone(),
            namespace.unwrap_or(&self.default_namespace),
        )
    }

    /// Create the namespace if it doesn't already exist — real per-tenant
    /// isolation means each tenant's namespace must actually exist before
    /// their first `ContainerGroup` apply, not just be a config value fabric
    /// assumes a cluster admin pre-created. A no-op (not an error) if it's
    /// already there, so callers can call this unconditionally before every
    /// apply.
    pub async fn ensure_namespace(&self, namespace: &str) -> Result<()> {
        let namespaces: Api<Namespace> = Api::all(self.client.clone());
        if namespaces.get_opt(namespace).await?.is_some() {
            return Ok(());
        }
        let ns = Namespace {
            metadata: ObjectMeta {
                name: Some(namespace.to_string()),
                ..Default::default()
            },
            ..Default::default()
        };
        match namespaces.create(&PostParams::default(), &ns).await {
            Ok(_) => Ok(()),
            // Lost a create race with a concurrent apply for the same
            // tenant -- the namespace existing is all that matters here.
            Err(kube::Error::Api(e)) if e.code == 409 => Ok(()),
            Err(e) => Err(e).with_context(|| format!("creating namespace '{namespace}'")),
        }
    }

    /// Create (or replace, via server-side apply) the Pod backing a
    /// ContainerGroup. Idempotent under the same field manager, so re-`apply`
    /// of an unchanged spec is a no-op on the API server. `namespace: None`
    /// uses the client's configured default (an untenanted group).
    pub async fn create_pod(&self, req: &PodRequest, namespace: Option<&str>) -> Result<Pod> {
        let pod = build_pod(req);
        let pp = PatchParams::apply("zyvor-fabricd").force();
        self.pods_in(namespace)
            .patch(&req.name, &pp, &kube::api::Patch::Apply(&pod))
            .await
            .with_context(|| format!("applying Pod '{}'", req.name))
    }

    pub async fn delete_pod(&self, name: &str, namespace: Option<&str>) -> Result<()> {
        match self
            .pods_in(namespace)
            .delete(name, &DeleteParams::default())
            .await
        {
            Ok(_) => Ok(()),
            Err(kube::Error::Api(e)) if e.code == 404 => Ok(()),
            Err(e) => Err(e).with_context(|| format!("deleting Pod '{name}'")),
        }
    }

    pub async fn get_pod_status(
        &self,
        name: &str,
        namespace: Option<&str>,
    ) -> Result<Option<PodStatusView>> {
        match self.pods_in(namespace).get_opt(name).await {
            Ok(Some(pod)) => Ok(Some(to_status_view(&pod))),
            Ok(None) => Ok(None),
            Err(e) => Err(e).with_context(|| format!("getting Pod '{name}'")),
        }
    }

    fn network_policies_in(&self, namespace: Option<&str>) -> Api<NetworkPolicy> {
        Api::namespaced(
            self.client.clone(),
            namespace.unwrap_or(&self.default_namespace),
        )
    }

    /// Create (or replace, via server-side apply) the `NetworkPolicy`
    /// isolating a ContainerGroup. Idempotent the same way `create_pod` is.
    pub async fn apply_network_policy(
        &self,
        req: &NetworkPolicyRequest,
        namespace: Option<&str>,
    ) -> Result<()> {
        let policy = build_network_policy(req);
        let pp = PatchParams::apply("zyvor-fabricd").force();
        self.network_policies_in(namespace)
            .patch(&req.name, &pp, &kube::api::Patch::Apply(&policy))
            .await
            .with_context(|| format!("applying NetworkPolicy '{}'", req.name))?;
        Ok(())
    }

    pub async fn delete_network_policy(&self, name: &str, namespace: Option<&str>) -> Result<()> {
        match self
            .network_policies_in(namespace)
            .delete(name, &DeleteParams::default())
            .await
        {
            Ok(_) => Ok(()),
            Err(kube::Error::Api(e)) if e.code == 404 => Ok(()),
            Err(e) => Err(e).with_context(|| format!("deleting NetworkPolicy '{name}'")),
        }
    }
}

fn build_network_policy_peers(rule: &NetworkPolicyRule) -> Option<Vec<NetworkPolicyPeer>> {
    let mut peers = Vec::new();
    for labels in &rule.peer_label_selectors {
        peers.push(NetworkPolicyPeer {
            pod_selector: Some(LabelSelector {
                match_labels: Some(labels.clone()),
                match_expressions: None,
            }),
            ip_block: None,
            namespace_selector: None,
        });
    }
    for cidr in &rule.cidrs {
        peers.push(NetworkPolicyPeer {
            ip_block: Some(IPBlock {
                cidr: cidr.clone(),
                except: None,
            }),
            pod_selector: None,
            namespace_selector: None,
        });
    }
    if peers.is_empty() {
        None
    } else {
        Some(peers)
    }
}

fn build_network_policy_ports(rule: &NetworkPolicyRule) -> Option<Vec<NetworkPolicyPort>> {
    if rule.ports.is_empty() {
        return None;
    }
    Some(
        rule.ports
            .iter()
            .map(|p| NetworkPolicyPort {
                port: Some(IntOrString::Int(*p as i32)),
                protocol: None,
                end_port: None,
            })
            .collect(),
    )
}

fn build_network_policy(req: &NetworkPolicyRequest) -> NetworkPolicy {
    let ingress = req.ingress.as_ref().map(|rules| {
        rules
            .iter()
            .map(|r| NetworkPolicyIngressRule {
                from: build_network_policy_peers(r),
                ports: build_network_policy_ports(r),
            })
            .collect()
    });
    let egress = req.egress.as_ref().map(|rules| {
        rules
            .iter()
            .map(|r| NetworkPolicyEgressRule {
                to: build_network_policy_peers(r),
                ports: build_network_policy_ports(r),
            })
            .collect()
    });

    let mut policy_types = Vec::new();
    if req.ingress.is_some() {
        policy_types.push("Ingress".to_string());
    }
    if req.egress.is_some() {
        policy_types.push("Egress".to_string());
    }

    NetworkPolicy {
        metadata: ObjectMeta {
            name: Some(req.name.clone()),
            ..Default::default()
        },
        spec: Some(K8sNetworkPolicySpec {
            pod_selector: Some(LabelSelector {
                match_labels: Some(req.pod_selector_labels.clone()),
                match_expressions: None,
            }),
            ingress,
            egress,
            policy_types: if policy_types.is_empty() {
                None
            } else {
                Some(policy_types)
            },
        }),
    }
}

fn to_status_view(pod: &Pod) -> PodStatusView {
    let status: Option<&PodStatus> = pod.status.as_ref();
    PodStatusView {
        phase: status.and_then(|s| s.phase.clone()),
        pod_ip: status.and_then(|s| s.pod_ip.clone()),
        node_name: pod.spec.as_ref().and_then(|s| s.node_name.clone()),
        uid: pod.metadata.uid.clone(),
        resource_version: pod.metadata.resource_version.clone(),
    }
}

fn build_probe(spec: &ProbeSpec) -> Probe {
    let (exec, http_get, tcp_socket) = match &spec.check {
        ProbeCheck::Exec { command } => (
            Some(ExecAction {
                command: Some(command.clone()),
            }),
            None,
            None,
        ),
        ProbeCheck::Http { path, port } => (
            None,
            Some(HTTPGetAction {
                path: Some(path.clone()),
                port: IntOrString::Int(*port as i32),
                ..Default::default()
            }),
            None,
        ),
        ProbeCheck::Tcp { port } => (
            None,
            None,
            Some(TCPSocketAction {
                port: IntOrString::Int(*port as i32),
                ..Default::default()
            }),
        ),
    };
    Probe {
        exec,
        http_get,
        tcp_socket,
        initial_delay_seconds: spec.initial_delay_secs.map(|v| v as i32),
        period_seconds: spec.period_secs.map(|v| v as i32),
        timeout_seconds: spec.timeout_secs.map(|v| v as i32),
        success_threshold: spec.success_threshold.map(|v| v as i32),
        failure_threshold: spec.failure_threshold.map(|v| v as i32),
        ..Default::default()
    }
}

fn build_pod(req: &PodRequest) -> Pod {
    let containers = req
        .containers
        .iter()
        .map(|c| {
            let quantity = |v: String| Quantity(v);
            let mut requests = BTreeMap::new();
            requests.insert("cpu".to_string(), quantity(format!("{}m", c.cpu_millis)));
            requests.insert("memory".to_string(), quantity(format!("{}Mi", c.memory_mb)));

            Container {
                name: c.name.clone(),
                image: Some(c.image.clone()),
                command: if c.command.is_empty() {
                    None
                } else {
                    Some(c.command.clone())
                },
                args: if c.args.is_empty() {
                    None
                } else {
                    Some(c.args.clone())
                },
                env: if c.env.is_empty() {
                    None
                } else {
                    Some(
                        c.env
                            .iter()
                            .map(|(k, v)| EnvVar {
                                name: k.clone(),
                                value: Some(v.clone()),
                                ..Default::default()
                            })
                            .collect(),
                    )
                },
                resources: Some(ResourceRequirements {
                    requests: Some(requests.clone()),
                    limits: Some(requests),
                    ..Default::default()
                }),
                volume_mounts: if c.volume_mounts.is_empty() {
                    None
                } else {
                    Some(
                        c.volume_mounts
                            .iter()
                            .map(|m| K8sVolumeMount {
                                name: m.name.clone(),
                                mount_path: m.mount_path.clone(),
                                read_only: Some(m.read_only),
                                ..Default::default()
                            })
                            .collect(),
                    )
                },
                liveness_probe: c.liveness_probe.as_ref().map(build_probe),
                readiness_probe: c.readiness_probe.as_ref().map(build_probe),
                ..Default::default()
            }
        })
        .collect();

    let volumes = if req.volumes.is_empty() {
        None
    } else {
        Some(
            req.volumes
                .iter()
                .map(|v| Volume {
                    name: v.name.clone(),
                    host_path: Some(HostPathVolumeSource {
                        path: v.host_path.clone(),
                        type_: None,
                    }),
                    ..Default::default()
                })
                .collect(),
        )
    };

    let image_pull_secrets = if req.image_pull_secrets.is_empty() {
        None
    } else {
        Some(
            req.image_pull_secrets
                .iter()
                .map(|name| LocalObjectReference { name: name.clone() })
                .collect(),
        )
    };

    Pod {
        metadata: ObjectMeta {
            name: Some(req.name.clone()),
            labels: Some(req.labels.clone()),
            ..Default::default()
        },
        spec: Some(PodSpec {
            containers,
            volumes,
            node_name: Some(req.node_name.clone()),
            runtime_class_name: Some(SECURE_CONTAINERS_RUNTIME_CLASS.to_string()),
            restart_policy: Some(req.restart_policy.clone()),
            image_pull_secrets,
            tolerations: Some(vec![Toleration {
                key: Some(SECURE_CONTAINERS_TAINT_KEY.to_string()),
                operator: Some("Equal".to_string()),
                value: Some("true".to_string()),
                effect: Some("NoSchedule".to_string()),
                ..Default::default()
            }]),
            ..Default::default()
        }),
        status: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_request() -> PodRequest {
        let mut labels = BTreeMap::new();
        labels.insert(
            "fabric.zyvor.dev/container-group".to_string(),
            "web".to_string(),
        );
        PodRequest {
            name: "web-0".to_string(),
            node_name: "node-a".to_string(),
            labels,
            containers: vec![PodContainerSpec {
                name: "app".to_string(),
                image: "nginx:latest".to_string(),
                command: vec![],
                args: vec![],
                env: vec![("FOO".to_string(), "bar".to_string())],
                cpu_millis: 500,
                memory_mb: 256,
                volume_mounts: vec![PodVolumeMount {
                    name: "app-vol-0".to_string(),
                    mount_path: "/data".to_string(),
                    read_only: true,
                }],
                liveness_probe: None,
                readiness_probe: None,
            }],
            volumes: vec![PodVolumeSource {
                name: "app-vol-0".to_string(),
                host_path: "/srv/web".to_string(),
            }],
            restart_policy: "Always".to_string(),
            image_pull_secrets: vec![],
        }
    }

    #[test]
    fn build_pod_pins_node_name_and_runtime_class() {
        let pod = build_pod(&sample_request());
        let spec = pod.spec.expect("pod spec");
        assert_eq!(spec.node_name.as_deref(), Some("node-a"));
        assert_eq!(
            spec.runtime_class_name.as_deref(),
            Some(SECURE_CONTAINERS_RUNTIME_CLASS)
        );
        assert_eq!(spec.restart_policy.as_deref(), Some("Always"));
    }

    #[test]
    fn build_pod_carries_the_secure_containers_toleration() {
        let pod = build_pod(&sample_request());
        let spec = pod.spec.expect("pod spec");
        let tolerations = spec.tolerations.expect("tolerations");
        assert_eq!(tolerations.len(), 1);
        assert_eq!(
            tolerations[0].key.as_deref(),
            Some(SECURE_CONTAINERS_TAINT_KEY)
        );
        assert_eq!(tolerations[0].effect.as_deref(), Some("NoSchedule"));
    }

    #[test]
    fn build_pod_sets_matching_container_resources_and_volume_mount() {
        let pod = build_pod(&sample_request());
        let spec = pod.spec.expect("pod spec");
        assert_eq!(spec.containers.len(), 1);
        let container = &spec.containers[0];
        assert_eq!(container.image.as_deref(), Some("nginx:latest"));

        let resources = container.resources.as_ref().expect("resources");
        let requests = resources.requests.as_ref().expect("requests");
        assert_eq!(requests.get("cpu").unwrap().0, "500m");
        assert_eq!(requests.get("memory").unwrap().0, "256Mi");
        // Guaranteed QoS: limits mirror requests exactly.
        assert_eq!(resources.limits, resources.requests);

        let mounts = container.volume_mounts.as_ref().expect("volume mounts");
        assert_eq!(mounts[0].mount_path, "/data");
        assert_eq!(mounts[0].read_only, Some(true));

        let volumes = spec.volumes.expect("volumes");
        assert_eq!(volumes[0].host_path.as_ref().unwrap().path, "/srv/web");
    }

    #[test]
    fn build_pod_omits_volumes_when_none_are_requested() {
        let mut req = sample_request();
        req.containers[0].volume_mounts.clear();
        req.volumes.clear();
        let pod = build_pod(&req);
        let spec = pod.spec.expect("pod spec");
        assert!(spec.volumes.is_none());
        assert!(spec.containers[0].volume_mounts.is_none());
    }

    #[test]
    fn build_pod_omits_image_pull_secrets_when_none_are_requested() {
        let pod = build_pod(&sample_request());
        let spec = pod.spec.expect("pod spec");
        assert!(spec.image_pull_secrets.is_none());
    }

    #[test]
    fn build_pod_sets_image_pull_secrets_at_the_pod_level_when_requested() {
        let mut req = sample_request();
        req.image_pull_secrets = vec!["registry-creds".to_string(), "another-secret".to_string()];
        let pod = build_pod(&req);
        let spec = pod.spec.expect("pod spec");
        let secrets = spec.image_pull_secrets.expect("image pull secrets");
        let names: Vec<&str> = secrets.iter().map(|s| s.name.as_str()).collect();
        assert_eq!(names, vec!["registry-creds", "another-secret"]);
    }

    #[test]
    fn build_pod_omits_probes_when_none_are_requested() {
        let pod = build_pod(&sample_request());
        let spec = pod.spec.expect("pod spec");
        assert!(spec.containers[0].liveness_probe.is_none());
        assert!(spec.containers[0].readiness_probe.is_none());
    }

    #[test]
    fn build_pod_sets_an_http_readiness_probe() {
        let mut req = sample_request();
        req.containers[0].readiness_probe = Some(ProbeSpec {
            check: ProbeCheck::Http {
                path: "/healthz".to_string(),
                port: 8080,
            },
            initial_delay_secs: Some(5),
            period_secs: Some(10),
            timeout_secs: None,
            success_threshold: None,
            failure_threshold: Some(3),
        });
        let pod = build_pod(&req);
        let spec = pod.spec.expect("pod spec");
        let probe = spec.containers[0]
            .readiness_probe
            .as_ref()
            .expect("readiness probe");
        let http_get = probe.http_get.as_ref().expect("httpGet action");
        assert_eq!(http_get.path.as_deref(), Some("/healthz"));
        assert_eq!(http_get.port, IntOrString::Int(8080));
        assert_eq!(probe.initial_delay_seconds, Some(5));
        assert_eq!(probe.period_seconds, Some(10));
        assert_eq!(probe.failure_threshold, Some(3));
        assert!(probe.exec.is_none());
        assert!(probe.tcp_socket.is_none());
    }

    #[test]
    fn build_pod_sets_a_tcp_liveness_probe() {
        let mut req = sample_request();
        req.containers[0].liveness_probe = Some(ProbeSpec {
            check: ProbeCheck::Tcp { port: 5432 },
            initial_delay_secs: None,
            period_secs: None,
            timeout_secs: None,
            success_threshold: None,
            failure_threshold: None,
        });
        let pod = build_pod(&req);
        let spec = pod.spec.expect("pod spec");
        let probe = spec.containers[0]
            .liveness_probe
            .as_ref()
            .expect("liveness probe");
        let tcp = probe.tcp_socket.as_ref().expect("tcpSocket action");
        assert_eq!(tcp.port, IntOrString::Int(5432));
    }

    #[test]
    fn build_pod_sets_an_exec_probe() {
        let mut req = sample_request();
        req.containers[0].liveness_probe = Some(ProbeSpec {
            check: ProbeCheck::Exec {
                command: vec!["cat".to_string(), "/tmp/healthy".to_string()],
            },
            initial_delay_secs: None,
            period_secs: None,
            timeout_secs: None,
            success_threshold: None,
            failure_threshold: None,
        });
        let pod = build_pod(&req);
        let spec = pod.spec.expect("pod spec");
        let probe = spec.containers[0]
            .liveness_probe
            .as_ref()
            .expect("liveness probe");
        let exec = probe.exec.as_ref().expect("exec action");
        assert_eq!(
            exec.command.as_deref(),
            Some(["cat".to_string(), "/tmp/healthy".to_string()].as_slice())
        );
    }

    fn sample_network_policy_request() -> NetworkPolicyRequest {
        let mut selector = BTreeMap::new();
        selector.insert(
            "fabric.zyvor.dev/container-group".to_string(),
            "web".to_string(),
        );
        NetworkPolicyRequest {
            name: "web".to_string(),
            pod_selector_labels: selector,
            ingress: None,
            egress: None,
        }
    }

    #[test]
    fn build_network_policy_leaves_a_direction_untouched_when_absent() {
        let req = sample_network_policy_request();
        let policy = build_network_policy(&req);
        let spec = policy.spec.expect("policy spec");
        assert!(spec.policy_types.is_none());
        assert!(spec.ingress.is_none());
        assert!(spec.egress.is_none());
    }

    #[test]
    fn build_network_policy_deny_all_ingress_is_isolated_with_no_rules() {
        let mut req = sample_network_policy_request();
        req.ingress = Some(vec![]);
        let policy = build_network_policy(&req);
        let spec = policy.spec.expect("policy spec");
        assert_eq!(
            spec.policy_types.as_deref(),
            Some(["Ingress".to_string()].as_slice())
        );
        assert_eq!(spec.ingress, Some(vec![]));
    }

    #[test]
    fn build_network_policy_allows_from_another_container_group_and_a_cidr() {
        let mut req = sample_network_policy_request();
        let mut peer_labels = BTreeMap::new();
        peer_labels.insert(
            "fabric.zyvor.dev/container-group".to_string(),
            "frontend".to_string(),
        );
        req.ingress = Some(vec![NetworkPolicyRule {
            peer_label_selectors: vec![peer_labels],
            cidrs: vec!["10.0.0.0/8".to_string()],
            ports: vec![8080],
        }]);
        let policy = build_network_policy(&req);
        let spec = policy.spec.expect("policy spec");
        let ingress = spec.ingress.expect("ingress rules");
        assert_eq!(ingress.len(), 1);
        let from = ingress[0].from.as_ref().expect("from peers");
        assert_eq!(from.len(), 2);
        assert!(from[0].pod_selector.is_some());
        assert!(from[1].ip_block.is_some());
        let ports = ingress[0].ports.as_ref().expect("ports");
        assert_eq!(ports[0].port, Some(IntOrString::Int(8080)));
    }

    #[test]
    fn build_network_policy_port_only_rule_has_no_peer_restriction() {
        let mut req = sample_network_policy_request();
        req.egress = Some(vec![NetworkPolicyRule {
            peer_label_selectors: vec![],
            cidrs: vec![],
            ports: vec![53],
        }]);
        let policy = build_network_policy(&req);
        let spec = policy.spec.expect("policy spec");
        let egress = spec.egress.expect("egress rules");
        assert!(egress[0].to.is_none());
        assert!(egress[0].ports.is_some());
    }
}
