// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: LicenseRef-Zyvor-Production-2.0

use anyhow::Result;
use futures::StreamExt;
use kube::{
    api::Api,
    runtime::{watcher::Config, Controller},
    Client,
};
use std::sync::Arc;

use crate::{
    crd::{
        AiSite, ContainerGroup, GpuNode, InferenceApiKeyPolicy, InferenceAutoscaler,
        InferenceDeployment, InferenceEndpoint, InferenceProfile, InferenceRollout, ModelArtifact,
        VirtualMachine,
    },
    reconcile,
};

pub struct Context {
    pub client: Client,
    pub http: reqwest::Client,
    pub zyvor_fabricd_url: String,
    pub zyvor_fabricd_token: Option<String>,
}

pub async fn run(client: Client) -> Result<()> {
    let vms = Api::<VirtualMachine>::all(client.clone());
    let container_groups = Api::<ContainerGroup>::all(client.clone());

    let zyvor_fabricd_url = std::env::var("ZYVOR_FABRICD_URL")
        .unwrap_or_else(|_| "http://zyvor-fabricd:9095".to_string());
    let zyvor_fabricd_token = std::env::var("ZYVOR_FABRICD_TOKEN")
        .ok()
        .filter(|s| !s.is_empty());

    let context = Arc::new(Context {
        client: client.clone(),
        http: reqwest::Client::new(),
        zyvor_fabricd_url,
        zyvor_fabricd_token,
    });

    let vm_controller = Controller::new(vms, Config::default())
        .run(
            reconcile::reconcile,
            reconcile::error_policy,
            context.clone(),
        )
        .for_each(|res| async move {
            match res {
                Ok(o) => tracing::info!("Reconciled VM: {:?}", o),
                Err(e) => tracing::error!("VM reconcile error: {:?}", e),
            }
        });

    let container_group_controller = Controller::new(container_groups, Config::default())
        .run(
            reconcile::reconcile_container_group,
            reconcile::error_policy_container_group,
            context.clone(),
        )
        .for_each(|res| async move {
            match res {
                Ok(o) => tracing::info!("Reconciled ContainerGroup: {:?}", o),
                Err(e) => tracing::error!("ContainerGroup reconcile error: {:?}", e),
            }
        });

    tokio::join!(
        vm_controller,
        container_group_controller,
        watch_model_artifacts(client.clone(), context.clone()),
        watch_inference_deployments(client.clone(), context.clone()),
        watch_inference_profiles(client.clone(), context.clone()),
        watch_inference_endpoints(client.clone(), context.clone()),
        watch_inference_rollouts(client.clone(), context.clone()),
        watch_gpu_nodes(client.clone(), context.clone()),
        watch_ai_sites(client.clone(), context.clone()),
        watch_key_policies(client.clone(), context.clone()),
        watch_autoscalers(client.clone(), context.clone()),
    );

    Ok(())
}

async fn crd_installed(client: &Client, plural: &str) -> bool {
    let path = format!(
        "/apis/apiextensions.k8s.io/v1/customresourcedefinitions/{plural}.zyvor-fabricd.io"
    );
    let request = match http::Request::builder()
        .method("GET")
        .uri(&path)
        .body(Vec::new())
    {
        Ok(request) => request,
        Err(err) => {
            tracing::warn!("skipping {plural}: {err}");
            return false;
        }
    };
    match client.request::<serde_json::Value>(request).await {
        Ok(_) => true,
        Err(err) => {
            tracing::warn!(
                "AI CRD {plural}.zyvor-fabricd.io is not installed; skipping its controller: {err}"
            );
            false
        }
    }
}

async fn watch_model_artifacts(client: Client, context: Arc<Context>) {
    if !crd_installed(&client, "modelartifacts").await {
        return;
    }
    let api = Api::<ModelArtifact>::all(client);
    Controller::new(api, Config::default())
        .run(
            reconcile::reconcile_model_artifact,
            reconcile::error_policy_model_artifact,
            context,
        )
        .for_each(|res| async move {
            match res {
                Ok(o) => tracing::info!("Reconciled ModelArtifact: {:?}", o),
                Err(e) => tracing::error!("ModelArtifact reconcile error: {:?}", e),
            }
        })
        .await;
}

async fn watch_inference_deployments(client: Client, context: Arc<Context>) {
    if !crd_installed(&client, "inferencedeployments").await {
        return;
    }
    let api = Api::<InferenceDeployment>::all(client);
    Controller::new(api, Config::default())
        .run(
            reconcile::reconcile_inference_deployment,
            reconcile::error_policy_inference_deployment,
            context,
        )
        .for_each(|res| async move {
            match res {
                Ok(o) => tracing::info!("Reconciled InferenceDeployment: {:?}", o),
                Err(e) => tracing::error!("InferenceDeployment reconcile error: {:?}", e),
            }
        })
        .await;
}

async fn watch_inference_profiles(client: Client, context: Arc<Context>) {
    if !crd_installed(&client, "inferenceprofiles").await {
        return;
    }
    let api = Api::<InferenceProfile>::all(client);
    Controller::new(api, Config::default())
        .run(
            reconcile::reconcile_inference_profile,
            reconcile::error_policy_inference_profile,
            context,
        )
        .for_each(|res| async move {
            match res {
                Ok(o) => tracing::info!("Reconciled InferenceProfile: {:?}", o),
                Err(e) => tracing::error!("InferenceProfile reconcile error: {:?}", e),
            }
        })
        .await;
}

async fn watch_inference_endpoints(client: Client, context: Arc<Context>) {
    if !crd_installed(&client, "inferenceendpoints").await {
        return;
    }
    let api = Api::<InferenceEndpoint>::all(client);
    Controller::new(api, Config::default())
        .run(
            reconcile::reconcile_inference_endpoint,
            reconcile::error_policy_inference_endpoint,
            context,
        )
        .for_each(|res| async move {
            match res {
                Ok(o) => tracing::info!("Reconciled InferenceEndpoint: {:?}", o),
                Err(e) => tracing::error!("InferenceEndpoint reconcile error: {:?}", e),
            }
        })
        .await;
}

async fn watch_inference_rollouts(client: Client, context: Arc<Context>) {
    if !crd_installed(&client, "inferencerollouts").await {
        return;
    }
    let api = Api::<InferenceRollout>::all(client);
    Controller::new(api, Config::default())
        .run(
            reconcile::reconcile_inference_rollout,
            reconcile::error_policy_inference_rollout,
            context,
        )
        .for_each(|res| async move {
            match res {
                Ok(o) => tracing::info!("Reconciled InferenceRollout: {:?}", o),
                Err(e) => tracing::error!("InferenceRollout reconcile error: {:?}", e),
            }
        })
        .await;
}

async fn watch_gpu_nodes(client: Client, context: Arc<Context>) {
    if !crd_installed(&client, "gpunodes").await {
        return;
    }
    let api = Api::<GpuNode>::all(client);
    Controller::new(api, Config::default())
        .run(
            reconcile::reconcile_gpu_node,
            reconcile::error_policy_gpu_node,
            context,
        )
        .for_each(|res| async move {
            match res {
                Ok(o) => tracing::info!("Reconciled GpuNode: {:?}", o),
                Err(e) => tracing::error!("GpuNode reconcile error: {:?}", e),
            }
        })
        .await;
}

async fn watch_ai_sites(client: Client, context: Arc<Context>) {
    if !crd_installed(&client, "aisites").await {
        return;
    }
    let api = Api::<AiSite>::all(client);
    Controller::new(api, Config::default())
        .run(
            reconcile::reconcile_ai_site,
            reconcile::error_policy_ai_site,
            context,
        )
        .for_each(|res| async move {
            match res {
                Ok(o) => tracing::info!("Reconciled AiSite: {:?}", o),
                Err(e) => tracing::error!("AiSite reconcile error: {:?}", e),
            }
        })
        .await;
}

async fn watch_key_policies(client: Client, context: Arc<Context>) {
    if !crd_installed(&client, "inferenceapikeypolicies").await {
        return;
    }
    let api = Api::<InferenceApiKeyPolicy>::all(client);
    Controller::new(api, Config::default())
        .run(
            reconcile::reconcile_key_policy,
            reconcile::error_policy_key_policy,
            context,
        )
        .for_each(|res| async move {
            match res {
                Ok(o) => tracing::info!("Reconciled InferenceApiKeyPolicy: {:?}", o),
                Err(e) => tracing::error!("InferenceApiKeyPolicy reconcile error: {:?}", e),
            }
        })
        .await;
}

async fn watch_autoscalers(client: Client, context: Arc<Context>) {
    if !crd_installed(&client, "inferenceautoscalers").await {
        return;
    }
    let api = Api::<InferenceAutoscaler>::all(client);
    Controller::new(api, Config::default())
        .run(
            reconcile::reconcile_autoscaler,
            reconcile::error_policy_autoscaler,
            context,
        )
        .for_each(|res| async move {
            match res {
                Ok(o) => tracing::info!("Reconciled InferenceAutoscaler: {:?}", o),
                Err(e) => tracing::error!("InferenceAutoscaler reconcile error: {:?}", e),
            }
        })
        .await;
}
