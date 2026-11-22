// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: LicenseRef-Zyvor-Production-2.0

use kube::{
    api::{Api, Patch, PatchParams},
    runtime::controller::Action,
    ResourceExt,
};
use serde_json::json;
use std::sync::Arc;
use std::time::Duration;

use crate::{
    controller::Context,
    crd::{ContainerGroup, ContainerGroupStatus, VirtualMachine, VirtualMachineStatus},
    error::OperatorError,
};

fn with_auth(ctx: &Context, req: reqwest::RequestBuilder) -> reqwest::RequestBuilder {
    if let Some(ref token) = ctx.zyvor_fabricd_token {
        req.bearer_auth(token)
    } else {
        req
    }
}

pub async fn reconcile(
    vm: Arc<VirtualMachine>,
    ctx: Arc<Context>,
) -> Result<Action, OperatorError> {
    let name = vm.name_any();
    let namespace = vm.namespace().unwrap_or_default();

    tracing::info!("Reconciling VM {}/{}", namespace, name);

    let vm_api: Api<VirtualMachine> = Api::namespaced(ctx.client.clone(), &namespace);
    let http_client = &ctx.http;
    let vm_url = format!("{}/api/vms", ctx.zyvor_fabricd_url);

    let create_req = json!({
        "name": name,
        "image": vm.spec.image,
        "cpus": vm.spec.cpus,
        "memory": vm.spec.memory,
    });

    let vm_check_url = format!("{}/api/vms/{}", ctx.zyvor_fabricd_url, name);
    let exists = with_auth(&ctx, http_client.get(&vm_check_url))
        .send()
        .await?
        .status()
        .is_success();

    let mut observed_state = "unknown".to_string();
    let mut observed_ip: Option<String> = None;

    if !exists {
        let resp = with_auth(&ctx, http_client.post(&vm_url))
            .json(&create_req)
            .send()
            .await?;

        if !resp.status().is_success() {
            tracing::error!("Failed to create VM: {:?}", resp.text().await?);
            return Ok(Action::requeue(Duration::from_secs(30)));
        }

        tracing::info!("Created VM {}", name);

        if let Some(cloud_init) = &vm.spec.cloud_init {
            let cloud_init_url = format!("{}/api/vms/{}/cloud-init", ctx.zyvor_fabricd_url, name);
            let cloud_init_req = json!({
                "instance_id": name,
                "hostname": name,
                "user_data": cloud_init.user_data,
                "network_config": cloud_init.network_config,
            });

            if let Err(e) = with_auth(&ctx, http_client.post(&cloud_init_url))
                .json(&cloud_init_req)
                .send()
                .await
            {
                tracing::warn!("Failed to configure cloud-init for VM '{}': {}", name, e);
            }
        }

        let start_url = format!("{}/api/vms/{}/start", ctx.zyvor_fabricd_url, name);
        if let Err(e) = with_auth(&ctx, http_client.post(&start_url)).send().await {
            tracing::error!("Failed to start VM '{}': {}", name, e);
        }
    }

    if let Ok(resp) = with_auth(&ctx, http_client.get(&vm_check_url)).send().await {
        if resp.status().is_success() {
            if let Ok(body) = resp.json::<serde_json::Value>().await {
                if let Some(state) = body.get("state").and_then(|v| v.as_str()) {
                    observed_state = state.to_string();
                }
                observed_ip = body.get("ip").and_then(|v| v.as_str()).map(str::to_string);
            }
        }
    }

    let status = VirtualMachineStatus {
        state: observed_state,
        ip: observed_ip,
        node: Some(std::env::var("NODE_NAME").unwrap_or_else(|_| "unknown".to_string())),
    };

    let patch = json!({ "status": status });

    let ps = PatchParams::default();
    let _patched = vm_api
        .patch_status(&name, &ps, &Patch::Merge(&patch))
        .await?;

    Ok(Action::requeue(Duration::from_secs(300)))
}

pub fn error_policy(
    _vm: Arc<VirtualMachine>,
    _error: &OperatorError,
    _ctx: Arc<Context>,
) -> Action {
    Action::requeue(Duration::from_secs(60))
}

/// Thin translator to fabric's `/api/container-groups/*` REST API — never
/// creates a Pod directly (that's fabric's own placement + `k8s-pod-client`
/// job), exactly like `reconcile()` above never calls FluxVM directly.
/// Unlike VM's create-once dance, `apply_container_group_spec` is
/// idempotent (server-side apply on the Pod objects it creates), so every
/// reconcile tick just re-applies the current spec.
pub async fn reconcile_container_group(
    cg: Arc<ContainerGroup>,
    ctx: Arc<Context>,
) -> Result<Action, OperatorError> {
    let name = cg.name_any();
    let namespace = cg.namespace().unwrap_or_default();

    tracing::info!("Reconciling ContainerGroup {}/{}", namespace, name);

    let cg_api: Api<ContainerGroup> = Api::namespaced(ctx.client.clone(), &namespace);
    let apply_url = format!("{}/api/container-groups/apply", ctx.zyvor_fabricd_url);

    let apply_req = json!({
        "name": name,
        "containers": cg.spec.containers,
        "replicas": cg.spec.replicas,
        "placement": cg.spec.placement,
        "restart_policy": cg.spec.restart_policy,
        "tenant": cg.spec.tenant,
        "image_pull_secrets": cg.spec.image_pull_secrets,
        "network_policy": cg.spec.network_policy,
    });

    let resp = with_auth(&ctx, ctx.http.post(&apply_url))
        .json(&apply_req)
        .send()
        .await?;

    if !resp.status().is_success() {
        tracing::error!(
            "Failed to apply ContainerGroup '{}': {:?}",
            name,
            resp.text().await
        );
        return Ok(Action::requeue(Duration::from_secs(30)));
    }

    let observed_state = "applied".to_string();
    let mut observed_host: Option<String> = None;
    if let Ok(body) = resp.json::<serde_json::Value>().await {
        observed_host = body
            .get("host_name")
            .and_then(|v| v.as_str())
            .map(str::to_string);
    }

    let status = ContainerGroupStatus {
        state: observed_state,
        host_name: observed_host,
    };

    let patch = json!({ "status": status });
    let ps = PatchParams::default();
    let _patched = cg_api
        .patch_status(&name, &ps, &Patch::Merge(&patch))
        .await?;

    Ok(Action::requeue(Duration::from_secs(300)))
}

pub fn error_policy_container_group(
    _cg: Arc<ContainerGroup>,
    _error: &OperatorError,
    _ctx: Arc<Context>,
) -> Action {
    Action::requeue(Duration::from_secs(60))
}

pub async fn reconcile_model_artifact(
    ma: Arc<crate::crd::ModelArtifact>,
    ctx: Arc<Context>,
) -> Result<Action, OperatorError> {
    let name = ma.name_any();
    let namespace = ma.namespace().unwrap_or_default();
    tracing::info!("Reconciling ModelArtifact {}/{}", namespace, name);

    let api: Api<crate::crd::ModelArtifact> = Api::namespaced(ctx.client.clone(), &namespace);
    let get_url = format!("{}/api/ai/models/{}", ctx.zyvor_fabricd_url, name);
    let exists = with_auth(&ctx, ctx.http.get(&get_url))
        .send()
        .await?
        .status()
        .is_success();

    if !exists {
        let create_url = format!("{}/api/ai/models", ctx.zyvor_fabricd_url);
        let body = json!({
            "name": name,
            "source": ma.spec.source,
            "format": ma.spec.format,
            "revision": ma.spec.revision,
            "checksum": ma.spec.checksum,
            "tenant": ma.spec.tenant,
        });
        let resp = with_auth(&ctx, ctx.http.post(&create_url))
            .json(&body)
            .send()
            .await?;
        if !resp.status().is_success() {
            tracing::error!("Failed to create ModelArtifact: {:?}", resp.text().await?);
            return Ok(Action::requeue(Duration::from_secs(30)));
        }
    }

    let mut status = crate::crd::ModelArtifactStatus::default();
    if let Ok(resp) = with_auth(&ctx, ctx.http.get(&get_url)).send().await {
        if resp.status().is_success() {
            if let Ok(body) = resp.json::<serde_json::Value>().await {
                status.local_path = body
                    .get("local_path")
                    .and_then(|v| v.as_str())
                    .map(str::to_string);
                status.ready = status.local_path.is_some();
                status.message = Some(if status.ready {
                    "materialized".into()
                } else {
                    "registered (local_path pending)".into()
                });
            }
        }
    }

    let patch = json!({ "status": status });
    let _ = api
        .patch_status(&name, &PatchParams::default(), &Patch::Merge(&patch))
        .await?;
    Ok(Action::requeue(Duration::from_secs(300)))
}

pub fn error_policy_model_artifact(
    _ma: Arc<crate::crd::ModelArtifact>,
    _error: &OperatorError,
    _ctx: Arc<Context>,
) -> Action {
    Action::requeue(Duration::from_secs(60))
}

pub async fn reconcile_inference_deployment(
    dep: Arc<crate::crd::InferenceDeployment>,
    ctx: Arc<Context>,
) -> Result<Action, OperatorError> {
    let name = dep.name_any();
    let namespace = dep.namespace().unwrap_or_default();
    tracing::info!("Reconciling InferenceDeployment {}/{}", namespace, name);

    let api: Api<crate::crd::InferenceDeployment> = Api::namespaced(ctx.client.clone(), &namespace);
    let get_url = format!("{}/api/ai/deployments/{}", ctx.zyvor_fabricd_url, name);
    let get_resp = with_auth(&ctx, ctx.http.get(&get_url)).send().await?;
    let exists = get_resp.status().is_success();

    if !exists {
        let create_url = format!("{}/api/ai/deployments", ctx.zyvor_fabricd_url);
        let body = json!({
            "name": name,
            "model": dep.spec.model,
            "profile": dep.spec.profile,
            "replicas": dep.spec.replicas,
            "tenant": dep.spec.tenant,
        });
        let resp = with_auth(&ctx, ctx.http.post(&create_url))
            .json(&body)
            .send()
            .await?;
        if !resp.status().is_success() {
            tracing::error!(
                "Failed to create InferenceDeployment: {:?}",
                resp.text().await?
            );
            return Ok(Action::requeue(Duration::from_secs(30)));
        }
    } else if let Ok(body) = get_resp.json::<serde_json::Value>().await {
        let current = body.get("replicas").and_then(|v| v.as_u64()).unwrap_or(0) as u32;
        if current != dep.spec.replicas {
            let scale_url = format!(
                "{}/api/ai/deployments/{}/scale",
                ctx.zyvor_fabricd_url, name
            );
            let scale_body = json!({ "replicas": dep.spec.replicas });
            let resp = with_auth(&ctx, ctx.http.post(&scale_url))
                .json(&scale_body)
                .send()
                .await?;
            if !resp.status().is_success() {
                tracing::warn!(
                    "Failed to scale InferenceDeployment '{}': {:?}",
                    name,
                    resp.text().await
                );
            }
        }
    }

    let mut status = crate::crd::InferenceDeploymentStatus {
        phase: "Unknown".into(),
        message: None,
        ready_replicas: 0,
    };
    if let Ok(resp) = with_auth(&ctx, ctx.http.get(&get_url)).send().await {
        if resp.status().is_success() {
            if let Ok(body) = resp.json::<serde_json::Value>().await {
                if let Some(st) = body.get("status") {
                    status.phase = st
                        .get("phase")
                        .and_then(|v| v.as_str())
                        .unwrap_or("Unknown")
                        .to_string();
                    status.message = st
                        .get("message")
                        .and_then(|v| v.as_str())
                        .map(str::to_string);
                    if let Some(reps) = st.get("replicas").and_then(|v| v.as_array()) {
                        status.ready_replicas = reps
                            .iter()
                            .filter(|r| r.get("ready").and_then(|x| x.as_bool()) == Some(true))
                            .count() as u32;
                    }
                }
            }
        }
    }

    let patch = json!({ "status": status });
    let _ = api
        .patch_status(&name, &PatchParams::default(), &Patch::Merge(&patch))
        .await?;
    Ok(Action::requeue(Duration::from_secs(60)))
}

pub fn error_policy_inference_deployment(
    _dep: Arc<crate::crd::InferenceDeployment>,
    _error: &OperatorError,
    _ctx: Arc<Context>,
) -> Action {
    Action::requeue(Duration::from_secs(60))
}

async fn ensure_json(
    ctx: &Context,
    get_url: &str,
    write_url: &str,
    method: &str,
    body: serde_json::Value,
) -> Result<(), OperatorError> {
    let exists = with_auth(ctx, ctx.http.get(get_url))
        .send()
        .await?
        .status()
        .is_success();
    if exists {
        return Ok(());
    }
    let request = match method {
        "PUT" => with_auth(ctx, ctx.http.put(write_url)),
        _ => with_auth(ctx, ctx.http.post(write_url)),
    };
    let response = request.json(&body).send().await?;
    if !response.status().is_success() {
        tracing::error!(
            "fabricd {} {} returned {}",
            method,
            write_url,
            response.status()
        );
    }
    Ok(())
}

macro_rules! ai_error_policy {
    ($fn_name:ident, $ty:ty) => {
        pub fn $fn_name(_object: Arc<$ty>, _error: &OperatorError, _ctx: Arc<Context>) -> Action {
            Action::requeue(Duration::from_secs(60))
        }
    };
}

pub async fn reconcile_inference_profile(
    object: Arc<crate::crd::InferenceProfile>,
    ctx: Arc<Context>,
) -> Result<Action, OperatorError> {
    let name = object.name_any();
    ensure_json(
        &ctx,
        &format!("{}/api/ai/profiles/{name}", ctx.zyvor_fabricd_url),
        &format!("{}/api/ai/profiles", ctx.zyvor_fabricd_url),
        "POST",
        json!({
            "name": name,
            "runtime": object.spec.runtime,
            "gpu": { "vendor": "nvidia", "count": object.spec.gpu_count },
            "cpu": object.spec.cpu,
            "memory_gib": object.spec.memory_gib
        }),
    )
    .await?;
    Ok(Action::requeue(Duration::from_secs(60)))
}
ai_error_policy!(error_policy_inference_profile, crate::crd::InferenceProfile);

pub async fn reconcile_inference_endpoint(
    object: Arc<crate::crd::InferenceEndpoint>,
    ctx: Arc<Context>,
) -> Result<Action, OperatorError> {
    let name = object.name_any();
    ensure_json(
        &ctx,
        &format!("{}/api/ai/endpoints/{name}", ctx.zyvor_fabricd_url),
        &format!("{}/api/ai/endpoints", ctx.zyvor_fabricd_url),
        "POST",
        json!({
            "name": name,
            "deployment": object.spec.deployment,
            "protocol": object.spec.protocol,
            "port": 8000
        }),
    )
    .await?;
    Ok(Action::requeue(Duration::from_secs(60)))
}
ai_error_policy!(
    error_policy_inference_endpoint,
    crate::crd::InferenceEndpoint
);

pub async fn reconcile_inference_rollout(
    object: Arc<crate::crd::InferenceRollout>,
    ctx: Arc<Context>,
) -> Result<Action, OperatorError> {
    let deployment = &object.spec.deployment;
    let get_url = format!("{}/api/ai/deployments/{deployment}", ctx.zyvor_fabricd_url);
    let response = with_auth(&ctx, ctx.http.get(&get_url)).send().await?;
    if !response.status().is_success() {
        return Ok(Action::requeue(Duration::from_secs(60)));
    }
    let body: serde_json::Value = response.json().await.unwrap_or(serde_json::json!({}));
    if body.get("rollout").is_some_and(|value| !value.is_null()) {
        return Ok(Action::requeue(Duration::from_secs(60)));
    }
    let post_url = format!(
        "{}/api/ai/deployments/{deployment}/rollouts",
        ctx.zyvor_fabricd_url
    );
    let created = with_auth(&ctx, ctx.http.post(&post_url))
        .json(&json!({ "strategy": object.spec.strategy }))
        .send()
        .await?;
    if !created.status().is_success() {
        tracing::error!("rollout for {deployment} returned {}", created.status());
    }
    Ok(Action::requeue(Duration::from_secs(60)))
}
ai_error_policy!(error_policy_inference_rollout, crate::crd::InferenceRollout);

pub async fn reconcile_gpu_node(
    object: Arc<crate::crd::GpuNode>,
    ctx: Arc<Context>,
) -> Result<Action, OperatorError> {
    let name = object.name_any();
    ensure_json(
        &ctx,
        &format!("{}/api/ai/nodes/{name}", ctx.zyvor_fabricd_url),
        &format!("{}/api/ai/nodes", ctx.zyvor_fabricd_url),
        "POST",
        json!({ "id": name, "site": object.spec.site }),
    )
    .await?;
    Ok(Action::requeue(Duration::from_secs(60)))
}
ai_error_policy!(error_policy_gpu_node, crate::crd::GpuNode);

pub async fn reconcile_ai_site(
    object: Arc<crate::crd::AiSite>,
    ctx: Arc<Context>,
) -> Result<Action, OperatorError> {
    let name = object.name_any();
    ensure_json(
        &ctx,
        &format!("{}/api/ai/sites/{name}", ctx.zyvor_fabricd_url),
        &format!("{}/api/ai/sites", ctx.zyvor_fabricd_url),
        "POST",
        json!({ "id": name, "residency": object.spec.residency }),
    )
    .await?;
    Ok(Action::requeue(Duration::from_secs(60)))
}
ai_error_policy!(error_policy_ai_site, crate::crd::AiSite);

pub async fn reconcile_key_policy(
    object: Arc<crate::crd::InferenceApiKeyPolicy>,
    ctx: Arc<Context>,
) -> Result<Action, OperatorError> {
    let endpoint = &object.spec.endpoint;
    ensure_json(
        &ctx,
        &format!("{}/api/ai/key-policies/{endpoint}", ctx.zyvor_fabricd_url),
        &format!("{}/api/ai/key-policies/{endpoint}", ctx.zyvor_fabricd_url),
        "PUT",
        json!({
            "endpoint": endpoint,
            "max_ttl_secs": object.spec.max_ttl_secs
        }),
    )
    .await?;
    Ok(Action::requeue(Duration::from_secs(60)))
}
ai_error_policy!(error_policy_key_policy, crate::crd::InferenceApiKeyPolicy);

pub async fn reconcile_autoscaler(
    object: Arc<crate::crd::InferenceAutoscaler>,
    ctx: Arc<Context>,
) -> Result<Action, OperatorError> {
    let deployment = &object.spec.deployment;
    let url = format!(
        "{}/api/ai/deployments/{deployment}/autoscaling",
        ctx.zyvor_fabricd_url
    );
    let response = with_auth(&ctx, ctx.http.put(&url))
        .json(&json!({
            "autoscaling": {
                "enabled": true,
                "min_replicas": object.spec.min_replicas,
                "max_replicas": object.spec.max_replicas
            }
        }))
        .send()
        .await?;
    if !response.status().is_success() {
        tracing::error!(
            "autoscaling update for {deployment} returned {}",
            response.status()
        );
    }
    Ok(Action::requeue(Duration::from_secs(60)))
}
ai_error_policy!(error_policy_autoscaler, crate::crd::InferenceAutoscaler);

#[cfg(test)]
mod tests {
    use super::*;
    use bytes::Bytes;
    use http::{Request, Response};
    use http_body_util::{BodyExt, Full};
    use kube::Client;
    use serde_json::Value;
    use std::convert::Infallible;
    use std::sync::{Arc as StdArc, Mutex};
    use tower::service_fn;
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    fn test_vm(name: &str) -> VirtualMachine {
        let spec = crate::crd::VirtualMachineSpec {
            image: "ubuntu-24.04.qcow2".to_string(),
            cpus: 2,
            memory: 2048,
            cloud_init: None,
            tpm: None,
            vnc: None,
        };
        let mut vm = VirtualMachine::new(name, spec);
        vm.metadata.namespace = Some("default".to_string());
        vm
    }

    /// A kube `Client` backed by an in-memory tower service instead of a
    /// real API server -- `reconcile()`'s only Kubernetes call is the final
    /// `patch_status`, so this just needs to answer that one PATCH with a
    /// body that deserializes back into a `VirtualMachine`, and record how
    /// many times (and with what body) it was called.
    fn mock_kube_client(vm_name: &str) -> (Client, StdArc<Mutex<Vec<Value>>>) {
        let calls = StdArc::new(Mutex::new(Vec::new()));
        let calls_for_service = calls.clone();
        let name = vm_name.to_string();
        let service = service_fn(move |req: Request<kube::client::Body>| {
            let calls = calls_for_service.clone();
            let vm = test_vm(&name);
            async move {
                let body_bytes = req
                    .into_body()
                    .collect()
                    .await
                    .map(|c| c.to_bytes())
                    .unwrap_or_default();
                if !body_bytes.is_empty() {
                    if let Ok(v) = serde_json::from_slice::<Value>(&body_bytes) {
                        calls.lock().unwrap().push(v);
                    }
                }
                let body = serde_json::to_vec(&vm).unwrap();
                Ok::<_, Infallible>(
                    Response::builder()
                        .status(200)
                        .header("content-type", "application/json")
                        .body(Full::new(Bytes::from(body)))
                        .unwrap(),
                )
            }
        });
        (Client::new(service, "default"), calls)
    }

    fn test_context(client: Client, zyvor_fabricd_url: String) -> StdArc<Context> {
        StdArc::new(Context {
            client,
            http: reqwest::Client::new(),
            zyvor_fabricd_url,
            zyvor_fabricd_token: None,
        })
    }

    #[tokio::test]
    async fn error_policy_always_requeues_after_60_seconds() {
        let vm = StdArc::new(test_vm("web-01"));
        let err = OperatorError::Other("boom".to_string());
        let (client, _) = mock_kube_client("web-01");
        let ctx = test_context(client, "http://127.0.0.1".to_string());
        let action = error_policy(vm, &err, ctx);
        assert_eq!(action, Action::requeue(Duration::from_secs(60)));
    }

    #[tokio::test]
    async fn with_auth_adds_bearer_header_only_when_a_token_is_configured() {
        let http = reqwest::Client::new();

        let ctx_no_token = Context {
            client: mock_kube_client("x").0,
            http: http.clone(),
            zyvor_fabricd_url: "http://127.0.0.1".to_string(),
            zyvor_fabricd_token: None,
        };
        let req = with_auth(&ctx_no_token, http.get("https://example.invalid/api/vms"))
            .build()
            .unwrap();
        assert!(req.headers().get("authorization").is_none());

        let ctx_with_token = Context {
            client: mock_kube_client("x").0,
            http: http.clone(),
            zyvor_fabricd_url: "http://127.0.0.1".to_string(),
            zyvor_fabricd_token: Some("s3cr3t".to_string()),
        };
        let req = with_auth(&ctx_with_token, http.get("https://example.invalid/api/vms"))
            .build()
            .unwrap();
        assert_eq!(req.headers().get("authorization").unwrap(), "Bearer s3cr3t");
    }

    #[tokio::test]
    async fn reconcile_creates_configures_and_starts_a_vm_that_does_not_exist_yet() {
        let server = MockServer::start().await;
        let name = "web-01";

        // First existence check: 404, VM isn't there yet.
        Mock::given(method("GET"))
            .and(path(format!("/api/vms/{name}")))
            .respond_with(ResponseTemplate::new(404))
            .up_to_n_times(1)
            .expect(1)
            .mount(&server)
            .await;

        Mock::given(method("POST"))
            .and(path("/api/vms"))
            .respond_with(ResponseTemplate::new(201))
            .expect(1)
            .mount(&server)
            .await;

        Mock::given(method("POST"))
            .and(path(format!("/api/vms/{name}/start")))
            .respond_with(ResponseTemplate::new(200))
            .expect(1)
            .mount(&server)
            .await;

        // Second existence check, after creation: now it's running with an IP.
        Mock::given(method("GET"))
            .and(path(format!("/api/vms/{name}")))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "name": name,
                "state": "running",
                "ip": "10.0.0.5",
            })))
            .mount(&server)
            .await;

        let (client, calls) = mock_kube_client(name);
        let ctx = test_context(client, server.uri());
        let vm = StdArc::new(test_vm(name));

        let action = reconcile(vm, ctx).await.expect("reconcile should succeed");
        assert_eq!(action, Action::requeue(Duration::from_secs(300)));

        let recorded = calls.lock().unwrap();
        assert_eq!(recorded.len(), 1, "expected exactly one status patch");
        let status = &recorded[0]["status"];
        assert_eq!(status["state"], "running");
        assert_eq!(status["ip"], "10.0.0.5");
    }

    #[tokio::test]
    async fn reconcile_skips_create_and_start_for_a_vm_that_already_exists() {
        let server = MockServer::start().await;
        let name = "web-02";

        // Existence check succeeds immediately -- already there.
        Mock::given(method("GET"))
            .and(path(format!("/api/vms/{name}")))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "name": name,
                "state": "paused",
                "ip": "10.0.0.9",
            })))
            .mount(&server)
            .await;

        // If reconcile() ever POSTs to create or start an already-existing
        // VM, wiremock has no matching mock for it and the request 404s --
        // which would surface as a reconcile Ok(requeue-on-failure) or Err,
        // either way not the 300s success path asserted below.
        let (client, calls) = mock_kube_client(name);
        let ctx = test_context(client, server.uri());
        let vm = StdArc::new(test_vm(name));

        let action = reconcile(vm, ctx).await.expect("reconcile should succeed");
        assert_eq!(action, Action::requeue(Duration::from_secs(300)));

        let recorded = calls.lock().unwrap();
        assert_eq!(recorded.len(), 1);
        assert_eq!(recorded[0]["status"]["state"], "paused");
        assert_eq!(recorded[0]["status"]["ip"], "10.0.0.9");
    }

    #[tokio::test]
    async fn reconcile_sends_the_configured_bearer_token_to_the_fabric_api() {
        let server = MockServer::start().await;
        let name = "web-03";

        Mock::given(method("GET"))
            .and(path(format!("/api/vms/{name}")))
            .and(wiremock::matchers::header(
                "authorization",
                "Bearer topsecret",
            ))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "name": name,
                "state": "running",
                "ip": "10.0.0.1",
            })))
            .mount(&server)
            .await;

        let (client, _calls) = mock_kube_client(name);
        let ctx = StdArc::new(Context {
            client,
            http: reqwest::Client::new(),
            zyvor_fabricd_url: server.uri(),
            zyvor_fabricd_token: Some("topsecret".to_string()),
        });
        let vm = StdArc::new(test_vm(name));

        // If the bearer token were missing, the GET wouldn't match the mock
        // above (no unauthenticated fallback mocked) and wiremock would
        // 404 both checks -- reconcile() would then try to POST /api/vms,
        // which also has no mock, and end up returning the create-failure
        // requeue(30s) path instead of the 300s success path.
        let action = reconcile(vm, ctx).await.expect("reconcile should succeed");
        assert_eq!(action, Action::requeue(Duration::from_secs(300)));
    }
}
