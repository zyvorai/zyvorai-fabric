// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: LicenseRef-Zyvor-Production-2.0

//! Public instance identity for the sign-in screen (no auth required).

use axum::Json;
use serde::Serialize;
use std::path::Path;

#[derive(Debug, Serialize)]
pub struct InstanceInfo {
    pub product: String,
    pub product_id: String,
    pub version: String,
    pub hostname: String,
    pub deploy_mode: String,
    pub deploy_label: String,
    pub kubernetes: bool,
    pub kubernetes_namespace: Option<String>,
    pub listen: Option<String>,
}

fn hostname() -> String {
    if let Ok(s) = std::fs::read_to_string("/etc/hostname") {
        let t = s.trim();
        if !t.is_empty() {
            return t.to_string();
        }
    }
    std::env::var("HOSTNAME")
        .ok()
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "localhost".into())
}

fn in_kubernetes() -> bool {
    Path::new("/var/run/secrets/kubernetes.io/serviceaccount").exists()
        || std::env::var_os("KUBERNETES_SERVICE_HOST").is_some()
}

fn deploy_mode() -> (String, String, bool, Option<String>) {
    if let Ok(mode) = std::env::var("ZYVOR_FABRICD_DEPLOY_MODE") {
        let mode = mode.trim().to_lowercase();
        let k8s = mode == "kubernetes" || mode == "k8s";
        let ns = std::env::var("ZYVOR_FABRICD_K8S_NAMESPACE")
            .ok()
            .or_else(|| std::env::var("POD_NAMESPACE").ok())
            .filter(|s| !s.is_empty());
        let label = match mode.as_str() {
            "kubernetes" | "k8s" => {
                if let Some(ref n) = ns {
                    format!("Kubernetes · {n}")
                } else {
                    "Kubernetes".into()
                }
            }
            "docker" | "podman" | "compose" => "Docker / Podman".into(),
            "systemd" | "bare-metal" | "host" => "Bare metal · systemd".into(),
            other => other.to_string(),
        };
        return (if k8s { "kubernetes".into() } else { mode }, label, k8s, ns);
    }

    if in_kubernetes() {
        let ns = std::env::var("ZYVOR_FABRICD_K8S_NAMESPACE")
            .ok()
            .or_else(|| std::env::var("POD_NAMESPACE").ok())
            .or_else(|| {
                std::fs::read_to_string("/var/run/secrets/kubernetes.io/serviceaccount/namespace")
                    .ok()
                    .map(|s| s.trim().to_string())
            })
            .filter(|s| !s.is_empty());
        let label = match &ns {
            Some(n) => format!("Kubernetes · {n}"),
            None => "Kubernetes".into(),
        };
        return ("kubernetes".into(), label, true, ns);
    }

    if Path::new("/.dockerenv").exists() {
        return ("docker".into(), "Docker / Podman".into(), false, None);
    }

    ("host".into(), "Bare metal · host".into(), false, None)
}

/// GET /api/instance — public product / host / deploy identity for the login UI.
pub async fn get_instance() -> Json<InstanceInfo> {
    let (deploy_mode, deploy_label, kubernetes, kubernetes_namespace) = deploy_mode();
    let listen = std::env::var("ZYVOR_FABRICD_LISTEN")
        .ok()
        .filter(|s| !s.is_empty());
    Json(InstanceInfo {
        product: "Zyvor Fabric".into(),
        product_id: "zyvor-fabric".into(),
        version: env!("CARGO_PKG_VERSION").into(),
        hostname: hostname(),
        deploy_mode,
        deploy_label,
        kubernetes,
        kubernetes_namespace,
        listen,
    })
}
