// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: Apache-2.0

use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::fs;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    pub daemon: DaemonConfig,
    pub storage: StorageConfig,
    pub network: NetworkConfig,
    #[serde(default)]
    pub controller: ControllerConfig,
    #[serde(default)]
    pub auth: AuthConfig,
    #[serde(default)]
    pub driver: DriverConfig,
    #[serde(default)]
    pub tls: TlsConfig,
    #[serde(default)]
    pub container_groups: ContainerGroupsConfig,
    #[serde(default)]
    pub agent_runtime: AgentRuntimeConfig,
}

/// Optional proxy to the sibling `zyvor-fabric-agent-runtime` service
/// (`/v1/agents`, `/v1/sessions`). When `base_url` is unset, agent APIs
/// return 503.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AgentRuntimeConfig {
    #[serde(default)]
    pub base_url: Option<String>,
    /// Bearer token for agent-runtime. Prefer env
    /// `ZYVOR_FABRICD_AGENT_RUNTIME_TOKEN` in production.
    #[serde(default)]
    pub token: Option<String>,
}

/// Gates the `ContainerGroup` workload (FluxVM Secure Containers, scheduled
/// by fabric's own placement onto a Kubernetes cluster). Default **off** —
/// Secure Containers is developer-preview/non-conformance-tested upstream
/// (see `docs/secure-containers-set3.md` in the fluxvm repo).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContainerGroupsConfig {
    #[serde(default)]
    pub enabled: bool,
    /// Kubeconfig for the target cluster. `None` uses the ambient
    /// in-cluster or default kubeconfig context. v1 supports exactly one
    /// target cluster, matching the VM path's own single-`fluxvm_url`
    /// maturity level today — multi-site resolution is a follow-up.
    #[serde(default)]
    pub kubeconfig_path: Option<String>,
    #[serde(default = "default_container_groups_namespace")]
    pub namespace: String,
    /// When true (default), a ContainerGroup with a `tenant` is placed in a
    /// dedicated namespace derived from it (`{namespace}-{tenant}`),
    /// auto-created on first use, instead of landing in the single shared
    /// `namespace` alongside every other tenant's Pods. Untenanted groups
    /// always use the shared `namespace` regardless of this setting.
    #[serde(default = "default_namespace_per_tenant")]
    pub namespace_per_tenant: bool,
}

impl Default for ContainerGroupsConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            kubeconfig_path: None,
            namespace: default_container_groups_namespace(),
            namespace_per_tenant: default_namespace_per_tenant(),
        }
    }
}

fn default_namespace_per_tenant() -> bool {
    true
}

fn default_container_groups_namespace() -> String {
    "default".to_string()
}

/// Native TLS, on by default. Deliberately not a reverse-proxy config
/// (nginx/Caddy in front) — this is the daemon's own listening socket, so
/// the exact same behavior applies whether run bare-metal, in a VM, or in
/// a Kubernetes pod (a cert-manager-issued cert just gets mounted at the
/// same `cert_path`/`key_path`, no cluster-specific ingress dependency).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TlsConfig {
    #[serde(default = "default_tls_enabled")]
    pub enabled: bool,
    /// If missing at startup and `enabled`, a self-signed cert is
    /// generated here automatically — matches the path
    /// `zyvor-fabricd-ctl tls` already writes to.
    #[serde(default = "default_tls_cert_path")]
    pub cert_path: String,
    #[serde(default = "default_tls_key_path")]
    pub key_path: String,
}

impl Default for TlsConfig {
    fn default() -> Self {
        Self {
            enabled: default_tls_enabled(),
            cert_path: default_tls_cert_path(),
            key_path: default_tls_key_path(),
        }
    }
}

fn default_tls_enabled() -> bool {
    true
}
fn default_tls_cert_path() -> String {
    "/etc/zyvor-fabricd/tls/server.crt".to_string()
}
fn default_tls_key_path() -> String {
    "/etc/zyvor-fabricd/tls/server.key".to_string()
}

/// Configures the `driver-core::VmDriver` (`Arc<dyn VmDriver>`) that
/// `AppState.driver` is built from — always FluxVM. The systemd-machined/
/// D-Bus backend this replaced (`machinectl-driver`/`machined-dbus`) is
/// gone as of the systemd-removal migration's final phase; there is no
/// backend selector to configure anymore.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DriverConfig {
    /// FluxVM's REST API base URL, e.g. `http://127.0.0.1:7788`.
    #[serde(default = "default_fluxvm_url")]
    pub fluxvm_url: String,
    /// Bearer token for FluxVM's auth layer, if it has `auth.tokens`
    /// configured. Leave unset against a deployment with auth disabled.
    #[serde(default)]
    pub fluxvm_token: Option<String>,
    /// Extra FluxVM nodes for Maglev service fan-out via `service-lb`.
    /// When empty, Fabric applies services only to `fluxvm_url`.
    #[serde(default)]
    pub fluxvm_nodes: Vec<FluxVmNodeConfig>,
    /// Prometheus text scrape URL for FluxVM MicroVM histograms
    /// (`MICROVM_METRICS_ADDR`, default `http://127.0.0.1:9108/metrics`).
    /// Set to `off` to disable the Fabric proxy.
    #[serde(default = "default_microvm_metrics_url")]
    pub microvm_metrics_url: Option<String>,
}

/// Optional additional FluxVM node for Service Fabric fan-out.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FluxVmNodeConfig {
    pub name: String,
    pub url: String,
    #[serde(default)]
    pub token: Option<String>,
}

impl Default for DriverConfig {
    fn default() -> Self {
        Self {
            fluxvm_url: default_fluxvm_url(),
            fluxvm_token: None,
            fluxvm_nodes: Vec::new(),
            microvm_metrics_url: default_microvm_metrics_url(),
        }
    }
}

fn default_fluxvm_url() -> String {
    "http://127.0.0.1:7788".to_string()
}

fn default_microvm_metrics_url() -> Option<String> {
    Some("http://127.0.0.1:9108/metrics".into())
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DaemonConfig {
    pub listen: String,
    #[serde(default = "default_cors_origins")]
    pub cors_origins: Vec<String>,
    /// External base URL advertised to clients (OpenStack catalog, etc.).
    /// When unset, derived from `listen` + TLS (`http(s)://host:port`).
    /// Override with `ZYVOR_FABRICD_PUBLIC_URL`.
    #[serde(default)]
    pub public_url: Option<String>,
}

fn default_cors_origins() -> Vec<String> {
    vec!["http://127.0.0.1:9095".to_string()]
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StorageConfig {
    pub path: String,
    #[serde(default = "default_image_path")]
    pub image_path: String,
    /// Base URL of the Atlas storage control plane's API (e.g.
    /// "http://localhost:5110/api/atlas/v1"), used to proxy Ceph RBD image
    /// operations for `Ceph`-typed storage pools. zyvor-fabricd has no
    /// direct Ceph/RBD driver of its own -- Atlas is the system of record
    /// for distributed storage. Unset means RBD image endpoints are
    /// unavailable (503) rather than silently no-op'ing.
    #[serde(default)]
    pub atlas_base_url: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetworkConfig {
    pub bridge: String,
    #[serde(default = "default_networkd_config_dir")]
    pub networkd_config_dir: String,
    #[serde(default = "default_networkd_file_prefix")]
    pub networkd_file_prefix: String,
    /// Optional URL for an external Cilium Hubble UI. Fabric does not embed
    /// Hubble; when set, the console surfaces a link (coexistence only).
    #[serde(default)]
    pub hubble_ui_url: Option<String>,
}

fn default_networkd_config_dir() -> String {
    "/etc/systemd/network".to_string()
}

fn default_networkd_file_prefix() -> String {
    "50-zyvor-fabricd-".to_string()
}

fn default_image_path() -> String {
    "/var/lib/zyvor-fabricd/images".to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum ControllerMode {
    #[default]
    Standalone,
    Controller,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ControllerConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub mode: ControllerMode,
    #[serde(default)]
    pub cluster_name: Option<String>,
    #[serde(default)]
    pub datacenter_name: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthConfig {
    #[serde(default = "default_auth_enabled")]
    pub enabled: bool,
    #[serde(default = "default_jwt_secret", skip_serializing)]
    pub jwt_secret: String,
    #[serde(default = "default_auth_db_path")]
    pub db_path: String,
    #[serde(default = "default_admin_password", skip_serializing)]
    pub default_admin_password: String,
    #[serde(default = "default_token_expiration_hours")]
    pub token_expiration_hours: i64,
}

fn default_auth_enabled() -> bool {
    true
}

fn default_jwt_secret() -> String {
    match std::env::var("ZYVOR_FABRICD_JWT_SECRET") {
        Ok(secret) => secret,
        Err(_) => {
            // Generate a random secret and persist it so tokens survive restarts
            let secret = generate_random_secret();
            let secret_path = "/var/lib/zyvor-fabricd/.jwt_secret";
            // Try to load a previously persisted secret first
            if let Ok(persisted) = std::fs::read_to_string(secret_path) {
                let trimmed = persisted.trim().to_string();
                if !trimmed.is_empty() {
                    return trimmed;
                }
            }
            // Persist the newly generated secret
            if let Some(parent) = std::path::Path::new(secret_path).parent() {
                if let Err(e) = std::fs::create_dir_all(parent) {
                    tracing::warn!("Failed to create directory for JWT secret: {}", e);
                }
            }
            if let Err(e) = encryption::write_private_file(secret_path, &secret) {
                tracing::warn!(
                    "Could not persist JWT secret to {}: {}. \
                     Tokens will be invalidated on restart. \
                     Set ZYVOR_FABRICD_JWT_SECRET or fix file permissions.",
                    secret_path,
                    e
                );
            } else {
                // Set restrictive permissions on the secret file
                #[cfg(unix)]
                {
                    use std::os::unix::fs::PermissionsExt;
                    if let Err(e) = std::fs::set_permissions(
                        secret_path,
                        std::fs::Permissions::from_mode(0o600),
                    ) {
                        tracing::warn!("Failed to set permissions on JWT secret file: {}", e);
                    }
                }
            }
            secret
        }
    }
}

fn generate_random_secret() -> String {
    use rand::RngExt;
    let mut rng = rand::rng();
    (0..64)
        .map(|_| rng.sample(rand::distr::Alphanumeric) as char)
        .collect()
}

fn default_auth_db_path() -> String {
    "/var/lib/zyvor-fabricd/auth.db".to_string()
}

fn default_admin_password() -> String {
    // Use env var or generate a secure random password (never default to "admin")
    std::env::var("ZYVOR_FABRICD_ADMIN_PASSWORD").unwrap_or_else(|_| {
        // Check if a password was already persisted from a previous run
        let pw_path = "/var/lib/zyvor-fabricd/.admin_password";
        if let Ok(existing) = std::fs::read_to_string(pw_path) {
            let trimmed = existing.trim().to_string();
            if !trimmed.is_empty() {
                return trimmed;
            }
        }

        let password = generate_random_secret();
        tracing::warn!(
            "================================================================"
        );
        tracing::warn!(
            "No admin password configured. Generated random password."
        );
        tracing::warn!(
            "Set ZYVOR_FABRICD_ADMIN_PASSWORD or auth.default_admin_password in config."
        );
        tracing::warn!(
            "Generated admin password has been written to /var/lib/zyvor-fabricd/.admin_password"
        );
        // Persist to a file instead of logging the password
        let pw_path = "/var/lib/zyvor-fabricd/.admin_password";
        if let Some(parent) = std::path::Path::new(pw_path).parent() {
            if let Err(e) = std::fs::create_dir_all(parent) {
                tracing::warn!("Failed to create directory for admin password: {}", e);
            }
        }
        if let Err(e) = encryption::write_private_file(pw_path, &password) {
            tracing::error!("Failed to write admin password file: {}", e);
        } else {
            // Set restrictive permissions on the password file
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                if let Err(e) = std::fs::set_permissions(pw_path, std::fs::Permissions::from_mode(0o600)) {
                    tracing::error!("SECURITY: Failed to set permissions on admin password file: {}. File may be world-readable!", e);
                    // Remove the file if we can't secure it
                    let _ = std::fs::remove_file(pw_path);
                }
            }
        }
        tracing::warn!(
            "================================================================"
        );
        password
    })
}

fn default_token_expiration_hours() -> i64 {
    24
}

impl Default for AuthConfig {
    fn default() -> Self {
        Self {
            enabled: default_auth_enabled(),
            jwt_secret: default_jwt_secret(),
            db_path: default_auth_db_path(),
            default_admin_password: default_admin_password(),
            token_expiration_hours: default_token_expiration_hours(),
        }
    }
}

impl Config {
    pub fn load() -> Result<Self> {
        let mut config = Self::load_from_file_or_default()?;
        if let Ok(listen) = std::env::var("ZYVOR_FABRICD_LISTEN") {
            tracing::info!(
                "Overriding daemon.listen from ZYVOR_FABRICD_LISTEN: {}",
                listen
            );
            config.daemon.listen = listen;
        }
        if let Ok(public_url) = std::env::var("ZYVOR_FABRICD_PUBLIC_URL") {
            let public_url = public_url.trim().trim_end_matches('/').to_string();
            if !public_url.is_empty() {
                tracing::info!(
                    "Overriding daemon.public_url from ZYVOR_FABRICD_PUBLIC_URL: {}",
                    public_url
                );
                config.daemon.public_url = Some(public_url);
            }
        }
        Ok(config)
    }

    /// Base URL clients should use to reach this daemon (no trailing slash).
    ///
    /// Order: `daemon.public_url` / `ZYVOR_FABRICD_PUBLIC_URL` → derive from
    /// `daemon.listen` and TLS. Wildcard binds (`0.0.0.0`, `::`) become
    /// `127.0.0.1` for local catalog URLs — set `public_url` for remote hosts.
    pub fn public_base_url(&self) -> String {
        if let Some(ref url) = self.daemon.public_url {
            return url.trim().trim_end_matches('/').to_string();
        }
        let scheme = if self.tls.enabled { "https" } else { "http" };
        let (host, port) = match self.daemon.listen.rsplit_once(':') {
            Some((host, port)) => {
                let host = match host.trim() {
                    "0.0.0.0" | "*" | "::" | "[::]" => "127.0.0.1",
                    h => h.trim_start_matches('[').trim_end_matches(']'),
                };
                (host.to_string(), port.to_string())
            }
            None => (self.daemon.listen.clone(), "9095".to_string()),
        };
        format!("{scheme}://{host}:{port}")
    }

    fn load_from_file_or_default() -> Result<Self> {
        if let Ok(path) = std::env::var("ZYVOR_FABRICD_CONFIG") {
            if let Ok(config) = Self::from_file(&path) {
                return Ok(config);
            }
        }

        let paths = vec![
            "/etc/zyvor-fabricd/zyvor-fabricd.toml",
            "configs/zyvor-fabricd.toml",
            "zyvor-fabricd.toml",
        ];

        for path in paths {
            if let Ok(config) = Self::from_file(path) {
                return Ok(config);
            }
        }

        tracing::warn!("No config file found, using defaults");

        // Default config
        Ok(Config {
            daemon: DaemonConfig {
                listen: "127.0.0.1:9095".to_string(),
                cors_origins: default_cors_origins(),
                public_url: None,
            },
            storage: StorageConfig {
                path: "/var/lib/zyvor-fabricd".to_string(),
                image_path: "/var/lib/zyvor-fabricd/images".to_string(),
                atlas_base_url: None,
            },
            network: NetworkConfig {
                bridge: "br0".to_string(),
                networkd_config_dir: default_networkd_config_dir(),
                networkd_file_prefix: default_networkd_file_prefix(),
                hubble_ui_url: None,
            },
            controller: ControllerConfig::default(),
            auth: AuthConfig::default(),
            driver: DriverConfig::default(),
            tls: TlsConfig::default(),
            container_groups: ContainerGroupsConfig::default(),
            agent_runtime: AgentRuntimeConfig::default(),
        })
    }

    fn from_file(path: &str) -> Result<Self> {
        let content = fs::read_to_string(path)?;
        let config: Config = toml::from_str(&content)?;
        tracing::info!("Loaded config from {}", path);
        for origin in &config.daemon.cors_origins {
            if origin.parse::<axum::http::HeaderValue>().is_err() {
                tracing::warn!(
                    "Invalid CORS origin '{}' in config — will be ignored",
                    origin
                );
            }
        }
        Ok(config)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn base_config(listen: &str, tls: bool, public_url: Option<&str>) -> Config {
        Config {
            daemon: DaemonConfig {
                listen: listen.into(),
                cors_origins: default_cors_origins(),
                public_url: public_url.map(str::to_string),
            },
            storage: StorageConfig {
                path: "/tmp".into(),
                image_path: "/tmp".into(),
                atlas_base_url: None,
            },
            network: NetworkConfig {
                bridge: "br0".into(),
                networkd_config_dir: default_networkd_config_dir(),
                networkd_file_prefix: default_networkd_file_prefix(),
                hubble_ui_url: None,
            },
            controller: ControllerConfig::default(),
            auth: AuthConfig::default(),
            driver: DriverConfig::default(),
            tls: TlsConfig {
                enabled: tls,
                ..TlsConfig::default()
            },
            container_groups: ContainerGroupsConfig::default(),
            agent_runtime: AgentRuntimeConfig::default(),
        }
    }

    #[test]
    fn public_base_url_derives_from_listen_and_tls() {
        let cfg = base_config("127.0.0.1:9095", true, None);
        assert_eq!(cfg.public_base_url(), "https://127.0.0.1:9095");

        let cfg = base_config("0.0.0.0:9095", false, None);
        assert_eq!(cfg.public_base_url(), "http://127.0.0.1:9095");
    }

    #[test]
    fn public_base_url_honors_explicit_override() {
        let cfg = base_config(
            "0.0.0.0:9095",
            true,
            Some("https://fabric.example.com:9443/"),
        );
        assert_eq!(cfg.public_base_url(), "https://fabric.example.com:9443");
    }
}
