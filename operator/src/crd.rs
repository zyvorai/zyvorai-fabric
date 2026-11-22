// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: LicenseRef-Zyvor-Production-2.0

use kube::CustomResource;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(CustomResource, Debug, Clone, Deserialize, Serialize, JsonSchema)]
#[kube(
    group = "zyvor-fabricd.io",
    version = "v1alpha1",
    kind = "VirtualMachine",
    plural = "virtualmachines",
    shortname = "vm",
    status = "VirtualMachineStatus",
    namespaced
)]
#[kube(printcolumn = r#"{"name":"State", "type":"string", "jsonPath":".status.state"}"#)]
#[kube(printcolumn = r#"{"name":"Age", "type":"date", "jsonPath":".metadata.creationTimestamp"}"#)]
pub struct VirtualMachineSpec {
    /// VM image path
    pub image: String,

    /// Number of CPUs
    #[serde(default = "default_cpus")]
    pub cpus: u32,

    /// Memory in MB
    #[serde(default = "default_memory")]
    pub memory: u64,

    /// cloud-init configuration
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cloud_init: Option<CloudInitSpec>,

    /// TPM configuration
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tpm: Option<TPMSpec>,

    /// VNC configuration
    #[serde(skip_serializing_if = "Option::is_none")]
    pub vnc: Option<VNCSpec>,
}

#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
pub struct CloudInitSpec {
    pub user_data: Option<String>,
    pub network_config: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
pub struct TPMSpec {
    pub enabled: bool,
    #[serde(default = "default_tpm_version")]
    pub version: String,
}

#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
pub struct VNCSpec {
    pub enabled: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub port: Option<u16>,
}

#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
pub struct VirtualMachineStatus {
    pub state: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ip: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub node: Option<String>,
}

fn default_cpus() -> u32 {
    2
}

fn default_memory() -> u64 {
    2048
}

fn default_tpm_version() -> String {
    "2.0".to_string()
}

// ---------------------------------------------------------------------------
// AI Workloads — ModelArtifact + InferenceDeployment (preview).
// Translators to /api/ai/* — see docs/ai-workloads.md.
// ---------------------------------------------------------------------------

#[derive(CustomResource, Debug, Clone, Deserialize, Serialize, JsonSchema)]
#[kube(
    group = "zyvor-fabricd.io",
    version = "v1alpha1",
    kind = "ModelArtifact",
    plural = "modelartifacts",
    shortname = "ma",
    status = "ModelArtifactStatus",
    namespaced
)]
#[kube(printcolumn = r#"{"name":"Source", "type":"string", "jsonPath":".spec.source"}"#)]
#[kube(printcolumn = r#"{"name":"Age", "type":"date", "jsonPath":".metadata.creationTimestamp"}"#)]
pub struct ModelArtifactSpec {
    pub source: String,
    pub format: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub revision: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub checksum: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tenant: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema, Default)]
pub struct ModelArtifactStatus {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub local_path: Option<String>,
    #[serde(default)]
    pub ready: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
}

#[derive(CustomResource, Debug, Clone, Deserialize, Serialize, JsonSchema)]
#[kube(
    group = "zyvor-fabricd.io",
    version = "v1alpha1",
    kind = "InferenceDeployment",
    plural = "inferencedeployments",
    shortname = "infdep",
    status = "InferenceDeploymentStatus",
    namespaced
)]
#[kube(printcolumn = r#"{"name":"Phase", "type":"string", "jsonPath":".status.phase"}"#)]
#[kube(printcolumn = r#"{"name":"Replicas", "type":"integer", "jsonPath":".spec.replicas"}"#)]
#[kube(printcolumn = r#"{"name":"Age", "type":"date", "jsonPath":".metadata.creationTimestamp"}"#)]
pub struct InferenceDeploymentSpec {
    /// ModelArtifact name (must exist in fabricd).
    pub model: String,
    /// InferenceProfile name (must exist in fabricd).
    pub profile: String,
    #[serde(default = "default_replicas")]
    pub replicas: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tenant: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema, Default)]
pub struct InferenceDeploymentStatus {
    #[serde(default)]
    pub phase: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
    #[serde(default)]
    pub ready_replicas: u32,
}

#[derive(CustomResource, Debug, Clone, Deserialize, Serialize, JsonSchema)]
#[kube(
    group = "zyvor-fabricd.io",
    version = "v1alpha1",
    kind = "InferenceProfile",
    plural = "inferenceprofiles",
    namespaced
)]
pub struct InferenceProfileSpec {
    #[serde(default = "default_vllm_runtime")]
    pub runtime: String,
    #[serde(default = "default_replicas")]
    pub gpu_count: u32,
    pub cpu: u32,
    pub memory_gib: u32,
}

#[derive(CustomResource, Debug, Clone, Deserialize, Serialize, JsonSchema)]
#[kube(
    group = "zyvor-fabricd.io",
    version = "v1alpha1",
    kind = "InferenceEndpoint",
    plural = "inferenceendpoints",
    namespaced
)]
pub struct InferenceEndpointSpec {
    pub deployment: String,
    #[serde(default = "default_openai")]
    pub protocol: String,
}

#[derive(CustomResource, Debug, Clone, Deserialize, Serialize, JsonSchema)]
#[kube(
    group = "zyvor-fabricd.io",
    version = "v1alpha1",
    kind = "InferenceRollout",
    plural = "inferencerollouts",
    namespaced
)]
pub struct InferenceRolloutSpec {
    pub deployment: String,
    #[serde(default = "default_rolling")]
    pub strategy: String,
}

#[derive(CustomResource, Debug, Clone, Deserialize, Serialize, JsonSchema)]
#[kube(
    group = "zyvor-fabricd.io",
    version = "v1alpha1",
    kind = "GpuNode",
    plural = "gpunodes",
    namespaced
)]
pub struct GpuNodeSpec {
    pub site: String,
}

#[derive(CustomResource, Debug, Clone, Deserialize, Serialize, JsonSchema)]
#[kube(
    group = "zyvor-fabricd.io",
    version = "v1alpha1",
    kind = "AiSite",
    plural = "aisites",
    namespaced
)]
pub struct AiSiteSpec {
    #[serde(default)]
    pub residency: String,
}

#[derive(CustomResource, Debug, Clone, Deserialize, Serialize, JsonSchema)]
#[kube(
    group = "zyvor-fabricd.io",
    version = "v1alpha1",
    kind = "InferenceApiKeyPolicy",
    plural = "inferenceapikeypolicies",
    namespaced
)]
pub struct InferenceApiKeyPolicySpec {
    pub endpoint: String,
    #[serde(default)]
    pub max_ttl_secs: i64,
}

#[derive(CustomResource, Debug, Clone, Deserialize, Serialize, JsonSchema)]
#[kube(
    group = "zyvor-fabricd.io",
    version = "v1alpha1",
    kind = "InferenceAutoscaler",
    plural = "inferenceautoscalers",
    namespaced
)]
pub struct InferenceAutoscalerSpec {
    pub deployment: String,
    #[serde(default)]
    pub min_replicas: u32,
    #[serde(default)]
    pub max_replicas: u32,
}

fn default_vllm_runtime() -> String {
    "vllm".into()
}

fn default_openai() -> String {
    "openai".into()
}

fn default_rolling() -> String {
    "rolling".into()
}

pub fn production_ai_kinds() -> &'static [&'static str] {
    let _ = (
        std::any::type_name::<InferenceProfile>(),
        std::any::type_name::<InferenceEndpoint>(),
        std::any::type_name::<InferenceRollout>(),
        std::any::type_name::<GpuNode>(),
        std::any::type_name::<AiSite>(),
        std::any::type_name::<InferenceApiKeyPolicy>(),
        std::any::type_name::<InferenceAutoscaler>(),
    );
    &[
        "ModelArtifact",
        "InferenceProfile",
        "InferenceDeployment",
        "InferenceEndpoint",
        "InferenceRollout",
        "GpuNode",
        "AiSite",
        "InferenceApiKeyPolicy",
        "InferenceAutoscaler",
    ]
}

// ---------------------------------------------------------------------------
// ContainerGroup — FluxVM Secure Containers workload, sibling to
// VirtualMachine. Its `reconcile_container_group` (see reconcile.rs) is a
// thin translator to fabric's own `/api/container-groups/*` REST API,
// exactly like VirtualMachine's reconcile — it never creates Pods directly;
// placement/execution stay fabric's job.
// ---------------------------------------------------------------------------

#[derive(CustomResource, Debug, Clone, Deserialize, Serialize, JsonSchema)]
#[kube(
    group = "zyvor-fabricd.io",
    version = "v1alpha1",
    kind = "ContainerGroup",
    plural = "containergroups",
    shortname = "cg",
    status = "ContainerGroupStatus",
    namespaced
)]
#[kube(printcolumn = r#"{"name":"Host", "type":"string", "jsonPath":".status.hostName"}"#)]
#[kube(printcolumn = r#"{"name":"State", "type":"string", "jsonPath":".status.state"}"#)]
#[kube(printcolumn = r#"{"name":"Age", "type":"date", "jsonPath":".metadata.creationTimestamp"}"#)]
pub struct ContainerGroupSpec {
    pub containers: Vec<ContainerSpec>,
    #[serde(default = "default_replicas")]
    pub replicas: u32,
    #[serde(default)]
    pub placement: PlacementSpec,
    #[serde(default = "default_restart_policy")]
    pub restart_policy: String,
    /// Owning tenant, forwarded verbatim to fabric's
    /// `/api/container-groups/apply` — see `tenant_scope::apply_create_tenant`
    /// on the fabric side for enforcement against the caller's JWT.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tenant: Option<String>,
    /// Names of `kubernetes.io/dockerconfigjson` Secrets, already present in
    /// the target namespace, to pull this group's images with.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub image_pull_secrets: Vec<String>,
    /// Kubernetes `NetworkPolicy` isolating this group's Pods, forwarded
    /// verbatim like `tenant`/`image_pull_secrets`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub network_policy: Option<NetworkPolicySpec>,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema)]
pub struct NetworkPolicySpec {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ingress: Option<Vec<NetworkPolicyRuleSpec>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub egress: Option<Vec<NetworkPolicyRuleSpec>>,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema)]
pub struct NetworkPolicyRuleSpec {
    #[serde(default)]
    pub from_container_groups: Vec<String>,
    #[serde(default)]
    pub from_cidrs: Vec<String>,
    #[serde(default)]
    pub ports: Vec<u16>,
}

#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
pub struct ContainerSpec {
    pub name: String,
    /// OCI image reference.
    pub image: String,
    #[serde(default)]
    pub command: Vec<String>,
    #[serde(default)]
    pub args: Vec<String>,
    #[serde(default)]
    pub env: Vec<EnvVarSpec>,
    pub cpus: u32,
    /// Memory with unit suffix, matching fabric's `ResourceSpec.memory`:
    /// "2G", "512M", "4096" (MB).
    pub memory: String,
    #[serde(default)]
    pub volume_mounts: Vec<VolumeMountSpec>,
    #[serde(default)]
    pub liveness_probe: Option<ProbeSpec>,
    #[serde(default)]
    pub readiness_probe: Option<ProbeSpec>,
}

/// Mirrors fabric's `container_declarative::ProbeSpec` 1:1 — forwarded
/// verbatim by `reconcile_container_group`, so the field shapes must match.
#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
pub struct ProbeSpec {
    #[serde(flatten)]
    pub check: ProbeCheckSpec,
    #[serde(default)]
    pub initial_delay_secs: Option<u32>,
    #[serde(default)]
    pub period_secs: Option<u32>,
    #[serde(default)]
    pub timeout_secs: Option<u32>,
    #[serde(default)]
    pub success_threshold: Option<u32>,
    #[serde(default)]
    pub failure_threshold: Option<u32>,
}

#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ProbeCheckSpec {
    Http { path: String, port: u16 },
    Tcp { port: u16 },
    Exec { command: Vec<String> },
}

#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
pub struct EnvVarSpec {
    pub name: String,
    pub value: String,
}

#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
pub struct VolumeMountSpec {
    /// Host path on the node the container's Pod is scheduled to (v1 is
    /// hostPath-backed only — see fabric's `k8s-pod-client`).
    pub host: String,
    pub guest: String,
    #[serde(default)]
    pub readonly: bool,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema)]
pub struct PlacementSpec {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub node_hint: Option<String>,
    #[serde(default)]
    pub auto_reschedule: bool,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema)]
pub struct ContainerGroupStatus {
    #[serde(default)]
    pub state: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub host_name: Option<String>,
}

fn default_replicas() -> u32 {
    1
}

fn default_restart_policy() -> String {
    "Always".to_string()
}

#[cfg(test)]
mod container_group_tests {
    use super::*;

    #[test]
    fn spec_deserializes_with_defaults_when_replicas_and_placement_are_omitted() {
        let spec: ContainerGroupSpec = serde_json::from_str(
            r#"{"containers": [{"name": "app", "image": "nginx:latest", "cpus": 1, "memory": "512M"}]}"#,
        )
        .unwrap();
        assert_eq!(spec.replicas, 1);
        assert_eq!(spec.restart_policy, "Always");
        assert!(!spec.placement.auto_reschedule);
        assert!(spec.placement.node_hint.is_none());
        assert_eq!(spec.containers[0].image, "nginx:latest");
        assert!(spec.tenant.is_none());
    }

    #[test]
    fn spec_serializes_tenant_when_set_and_omits_it_when_absent() {
        let mut spec: ContainerGroupSpec = serde_json::from_str(
            r#"{"containers": [{"name": "app", "image": "nginx:latest", "cpus": 1, "memory": "512M"}]}"#,
        )
        .unwrap();
        let json = serde_json::to_value(&spec).unwrap();
        assert!(!json.as_object().unwrap().contains_key("tenant"));

        spec.tenant = Some("acme".to_string());
        let json = serde_json::to_value(&spec).unwrap();
        assert_eq!(json["tenant"], "acme");
    }

    #[test]
    fn spec_image_pull_secrets_defaults_to_empty_and_is_omitted_from_json() {
        let mut spec: ContainerGroupSpec = serde_json::from_str(
            r#"{"containers": [{"name": "app", "image": "nginx:latest", "cpus": 1, "memory": "512M"}]}"#,
        )
        .unwrap();
        assert!(spec.image_pull_secrets.is_empty());
        let json = serde_json::to_value(&spec).unwrap();
        assert!(!json.as_object().unwrap().contains_key("image_pull_secrets"));

        spec.image_pull_secrets = vec!["registry-creds".to_string()];
        let json = serde_json::to_value(&spec).unwrap();
        assert_eq!(json["image_pull_secrets"][0], "registry-creds");
    }

    #[test]
    fn container_spec_probes_default_to_none_and_deserialize_from_tagged_json() {
        let spec: ContainerGroupSpec = serde_json::from_str(
            r#"{"containers": [{"name": "app", "image": "nginx:latest", "cpus": 1, "memory": "512M"}]}"#,
        )
        .unwrap();
        assert!(spec.containers[0].liveness_probe.is_none());
        assert!(spec.containers[0].readiness_probe.is_none());

        let spec: ContainerGroupSpec = serde_json::from_str(
            r#"{"containers": [{"name": "app", "image": "nginx:latest", "cpus": 1, "memory": "512M",
                "readiness_probe": {"type": "http", "path": "/healthz", "port": 8080}}]}"#,
        )
        .unwrap();
        assert!(matches!(
            spec.containers[0].readiness_probe.as_ref().unwrap().check,
            ProbeCheckSpec::Http { ref path, port } if path == "/healthz" && port == 8080
        ));
    }

    #[test]
    fn network_policy_defaults_to_none_and_deserializes_from_json() {
        let spec: ContainerGroupSpec = serde_json::from_str(
            r#"{"containers": [{"name": "app", "image": "nginx:latest", "cpus": 1, "memory": "512M"}]}"#,
        )
        .unwrap();
        assert!(spec.network_policy.is_none());

        let spec: ContainerGroupSpec = serde_json::from_str(
            r#"{"containers": [{"name": "app", "image": "nginx:latest", "cpus": 1, "memory": "512M"}],
                "network_policy": {"ingress": [{"from_container_groups": ["frontend"], "ports": [8080]}]}}"#,
        )
        .unwrap();
        let ingress = spec.network_policy.unwrap().ingress.unwrap();
        assert_eq!(ingress[0].from_container_groups, vec!["frontend".to_string()]);
        assert_eq!(ingress[0].ports, vec![8080]);
    }

    #[test]
    fn status_omits_absent_host_name_from_json_rather_than_emitting_null() {
        let status = ContainerGroupStatus {
            state: "applied".to_string(),
            host_name: None,
        };
        let json = serde_json::to_value(&status).unwrap();
        assert_eq!(json["state"], "applied");
        assert!(!json.as_object().unwrap().contains_key("host_name"));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spec_deserializes_with_defaults_when_cpus_and_memory_are_omitted() {
        let spec: VirtualMachineSpec =
            serde_json::from_str(r#"{"image": "ubuntu-24.04.qcow2"}"#).unwrap();
        assert_eq!(spec.image, "ubuntu-24.04.qcow2");
        assert_eq!(spec.cpus, 2);
        assert_eq!(spec.memory, 2048);
        assert!(spec.cloud_init.is_none());
        assert!(spec.tpm.is_none());
        assert!(spec.vnc.is_none());
    }

    #[test]
    fn spec_deserializes_explicit_cpus_and_memory_without_falling_back_to_defaults() {
        let spec: VirtualMachineSpec =
            serde_json::from_str(r#"{"image": "x.qcow2", "cpus": 8, "memory": 16384}"#).unwrap();
        assert_eq!(spec.cpus, 8);
        assert_eq!(spec.memory, 16384);
    }

    #[test]
    fn tpm_spec_defaults_version_to_2_0_when_omitted() {
        let tpm: TPMSpec = serde_json::from_str(r#"{"enabled": true}"#).unwrap();
        assert_eq!(tpm.version, "2.0");
    }

    #[test]
    fn tpm_spec_keeps_an_explicit_version() {
        let tpm: TPMSpec = serde_json::from_str(r#"{"enabled": true, "version": "1.2"}"#).unwrap();
        assert_eq!(tpm.version, "1.2");
    }

    #[test]
    fn full_spec_roundtrips_through_json() {
        let spec = VirtualMachineSpec {
            image: "web.qcow2".to_string(),
            cpus: 4,
            memory: 8192,
            cloud_init: Some(CloudInitSpec {
                user_data: Some("#cloud-config\n".to_string()),
                network_config: None,
            }),
            tpm: Some(TPMSpec {
                enabled: true,
                version: "2.0".to_string(),
            }),
            vnc: Some(VNCSpec {
                enabled: true,
                port: Some(5901),
            }),
        };
        let json = serde_json::to_string(&spec).unwrap();
        let back: VirtualMachineSpec = serde_json::from_str(&json).unwrap();
        assert_eq!(back.image, spec.image);
        assert_eq!(back.cpus, spec.cpus);
        assert_eq!(back.memory, spec.memory);
        assert_eq!(
            back.cloud_init.unwrap().user_data,
            spec.cloud_init.unwrap().user_data
        );
        assert_eq!(back.tpm.unwrap().version, "2.0");
        assert_eq!(back.vnc.unwrap().port, Some(5901));
    }

    #[test]
    fn status_omits_absent_ip_and_node_from_json_rather_than_emitting_null() {
        let status = VirtualMachineStatus {
            state: "Running".to_string(),
            ip: None,
            node: None,
        };
        let json = serde_json::to_value(&status).unwrap();
        assert_eq!(json["state"], "Running");
        assert!(!json.as_object().unwrap().contains_key("ip"));
        assert!(!json.as_object().unwrap().contains_key("node"));
    }

    #[test]
    fn vnc_spec_omits_absent_port_from_json() {
        let vnc = VNCSpec {
            enabled: true,
            port: None,
        };
        let json = serde_json::to_value(&vnc).unwrap();
        assert!(!json.as_object().unwrap().contains_key("port"));
    }
}
