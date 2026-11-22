// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: LicenseRef-Zyvor-Production-2.0

//! Distributed Service Fabric v6+ identity/L7 policy orchestration.
//! Fabric owns fan-out and rollback; FluxVM owns the local compiler/BPF maps.
//! Optional site_id / route_domain scoping fences multi-site policy apply.
//! Remote identity directory merge lets cross-site allow_identities resolve
//! via ClusterMesh-like CIDR fan-out into node ipcache (fail-closed if still
//! unresolved after local+remote merge).

use crate::{
    domain_key,
    remote_identity::{resolve_policy_identities, RemoteIdentityDirectory, RemoteIpcacheClient},
    EdgeLease, NodeTarget, ServiceSpec,
};
use anyhow::{bail, Context, Result};
use reqwest::StatusCode;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

const MAX_IDENTITIES: usize = 2048;
const MAX_L7_ITEMS: usize = 256;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "lowercase")]
pub enum PolicyDefaultAction {
    #[default]
    Allow,
    Deny,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum L7Protocol {
    Http,
    Grpc,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum L7Mode {
    Observe,
    Enforce,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct L7Policy {
    pub protocol: L7Protocol,
    pub mode: L7Mode,
    #[serde(default)]
    pub proxy_ifindex: u32,
    #[serde(default)]
    pub bypass_mark: u32,
    #[serde(default)]
    pub authorities: Vec<String>,
    #[serde(default)]
    pub path_prefixes: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ServicePolicySpec {
    pub service: String,
    #[serde(default = "default_enabled")]
    pub enabled: bool,
    #[serde(default)]
    pub default_action: PolicyDefaultAction,
    #[serde(default)]
    pub allow_identities: Vec<u32>,
    #[serde(default)]
    pub deny_identities: Vec<u32>,
    #[serde(default)]
    pub audit_only: bool,
    #[serde(default)]
    pub l7: Option<L7Policy>,
}
fn default_enabled() -> bool {
    true
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ServicePolicyStatus {
    pub schema_version: u32,
    pub program_generation: u32,
    pub service: String,
    pub service_id: u32,
    pub enabled: bool,
    pub compiled_ipv4: usize,
    pub compiled_ipv6: usize,
    pub unresolved_identities: Vec<u32>,
    pub l7_ready: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct EnvoyRedirectContract {
    pub schema_version: u32,
    pub service: String,
    pub enabled: bool,
    pub protocol: Option<L7Protocol>,
    pub mode: Option<L7Mode>,
    pub proxy_ifindex: u32,
    pub bypass_mark: u32,
    pub preserve_original_destination: bool,
    pub authorities: Vec<String>,
    pub path_prefixes: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PolicyApplyReport {
    pub service: String,
    pub applied_nodes: Vec<String>,
    pub rolled_back_nodes: Vec<String>,
}

pub fn validate_policy(spec: &ServicePolicySpec) -> Result<()> {
    if spec.service.is_empty() || spec.service.len() > 63 {
        bail!("policy service name must contain 1..=63 characters");
    }
    if spec.allow_identities.len() > MAX_IDENTITIES || spec.deny_identities.len() > MAX_IDENTITIES {
        bail!("identity lists are bounded to {MAX_IDENTITIES} entries each");
    }
    let allow: HashSet<u32> = spec.allow_identities.iter().copied().collect();
    let deny: HashSet<u32> = spec.deny_identities.iter().copied().collect();
    if allow.len() != spec.allow_identities.len() {
        bail!("duplicate allow identity");
    }
    if deny.len() != spec.deny_identities.len() {
        bail!("duplicate deny identity");
    }
    if allow.iter().any(|id| deny.contains(id)) {
        bail!("identity cannot be both allowed and denied");
    }
    if allow.contains(&0) || deny.contains(&0) {
        bail!("identity 0 is reserved/unresolved");
    }
    if let Some(l7) = &spec.l7 {
        if l7.authorities.len() > MAX_L7_ITEMS || l7.path_prefixes.len() > MAX_L7_ITEMS {
            bail!("too many L7 match entries");
        }
        for a in &l7.authorities {
            if a.is_empty() || a.len() > 253 {
                bail!("invalid authority length");
            }
        }
        for p in &l7.path_prefixes {
            if p.is_empty() || p.len() > 1024 || !p.starts_with('/') {
                bail!("invalid path prefix");
            }
        }
        if matches!(l7.mode, L7Mode::Enforce) && (l7.proxy_ifindex == 0 || l7.bypass_mark == 0) {
            bail!("L7 enforce requires proxy_ifindex and non-zero bypass_mark");
        }
    }
    Ok(())
}

#[async_trait::async_trait]
pub trait PolicyNodeClient: Send + Sync {
    async fn get_policy(
        &self,
        node: &NodeTarget,
        service: &str,
    ) -> Result<Option<ServicePolicySpec>>;
    async fn upsert_policy(
        &self,
        node: &NodeTarget,
        spec: &ServicePolicySpec,
    ) -> Result<ServicePolicyStatus>;
    async fn delete_policy(&self, node: &NodeTarget, service: &str) -> Result<()>;
    async fn envoy_contract(
        &self,
        node: &NodeTarget,
        service: &str,
    ) -> Result<EnvoyRedirectContract>;
}

pub struct FluxVmPolicyHttpClient {
    http: reqwest::Client,
}
impl FluxVmPolicyHttpClient {
    pub fn new() -> Result<Self> {
        Ok(Self {
            http: reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(30))
                .build()?,
        })
    }
    fn url(node: &NodeTarget, path: &str) -> Result<reqwest::Url> {
        let base = node.base_url.trim_end_matches('/');
        reqwest::Url::parse(&format!("{base}{path}")).context("invalid FluxVM node URL")
    }
    fn auth(&self, node: &NodeTarget, req: reqwest::RequestBuilder) -> reqwest::RequestBuilder {
        match &node.token {
            Some(t) => req.bearer_auth(t),
            None => req,
        }
    }
    async fn expect(resp: reqwest::Response, what: &str) -> Result<reqwest::Response> {
        if resp.status().is_success() {
            return Ok(resp);
        }
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        bail!("{what}: FluxVM HTTP {status}: {body}")
    }
}

#[async_trait::async_trait]
impl PolicyNodeClient for FluxVmPolicyHttpClient {
    async fn get_policy(
        &self,
        node: &NodeTarget,
        service: &str,
    ) -> Result<Option<ServicePolicySpec>> {
        let resp = self
            .auth(
                node,
                self.http.get(Self::url(
                    node,
                    &format!("/v1/network/services/{service}/policy"),
                )?),
            )
            .send()
            .await?;
        if resp.status() == StatusCode::NOT_FOUND {
            return Ok(None);
        }
        Ok(Some(
            Self::expect(resp, "get service policy")
                .await?
                .json()
                .await?,
        ))
    }
    async fn upsert_policy(
        &self,
        node: &NodeTarget,
        spec: &ServicePolicySpec,
    ) -> Result<ServicePolicyStatus> {
        validate_policy(spec)?;
        let resp = self
            .auth(
                node,
                self.http
                    .post(Self::url(node, "/v1/network/services/policies")?),
            )
            .json(spec)
            .send()
            .await?;
        Ok(Self::expect(resp, "upsert service policy")
            .await?
            .json()
            .await?)
    }
    async fn delete_policy(&self, node: &NodeTarget, service: &str) -> Result<()> {
        let resp = self
            .auth(
                node,
                self.http.delete(Self::url(
                    node,
                    &format!("/v1/network/services/{service}/policy"),
                )?),
            )
            .send()
            .await?;
        if resp.status() == StatusCode::NOT_FOUND {
            return Ok(());
        }
        Self::expect(resp, "delete service policy").await?;
        Ok(())
    }
    async fn envoy_contract(
        &self,
        node: &NodeTarget,
        service: &str,
    ) -> Result<EnvoyRedirectContract> {
        let resp = self
            .auth(
                node,
                self.http.get(Self::url(
                    node,
                    &format!("/v1/network/services/{service}/l7/envoy"),
                )?),
            )
            .send()
            .await?;
        Ok(Self::expect(resp, "get Envoy redirect contract")
            .await?
            .json()
            .await?)
    }
}

pub struct PolicyOrchestrator<C> {
    client: C,
}
impl<C: PolicyNodeClient> PolicyOrchestrator<C> {
    pub fn new(client: C) -> Self {
        Self { client }
    }

    pub async fn apply(
        &self,
        spec: &ServicePolicySpec,
        nodes: &[NodeTarget],
    ) -> Result<PolicyApplyReport> {
        // Back-compat: no site fencing → fan out to every node.
        self.apply_with_site(spec, nodes, None, None, &[], 0).await
    }

    /// Apply identity/L7 policy with optional multi-site fencing.
    ///
    /// When `site_id` (or `route_domain`) is set, only nodes that hold an
    /// active owning-domain [`EdgeLease`] receive the policy. Unset site
    /// fields keep the legacy all-nodes fan-out.
    ///
    /// Unresolved remote identities: FluxVM compiles allow/deny lists against
    /// the node-local ipcache. Identities that cannot be resolved on a node
    /// remain fail-closed there (`unresolved_identities` on
    /// [`ServicePolicyStatus`]). When a [`RemoteIdentityDirectory`] is supplied
    /// via [`Self::apply_with_remote_directory`], Fabric merges same-domain
    /// remote CIDRs into target node ipcache first; cross-domain directory
    /// entries are ignored. Full mesh datapath remains out of scope.
    pub async fn apply_with_site(
        &self,
        spec: &ServicePolicySpec,
        nodes: &[NodeTarget],
        site_id: Option<&str>,
        route_domain: Option<&str>,
        leases: &[EdgeLease],
        now_unix_ms: u64,
    ) -> Result<PolicyApplyReport> {
        validate_policy(spec)?;
        if nodes.is_empty() {
            bail!("policy apply requires at least one FluxVM node");
        }
        let targets = policy_fanout_nodes(
            &spec.service,
            nodes,
            site_id,
            route_domain,
            leases,
            now_unix_ms,
        );
        if targets.is_empty() {
            bail!("policy apply has no nodes in the owning site/route-domain");
        }
        let mut before = Vec::with_capacity(targets.len());
        for node in &targets {
            before.push((
                (*node).clone(),
                self.client.get_policy(node, &spec.service).await?,
            ));
        }
        let mut applied = Vec::new();
        for node in &targets {
            // See apply_with_site docs: unresolved identities stay fail-closed
            // on the FluxVM node; status.unresolved_identities is informational.
            if let Err(e) = self.client.upsert_policy(node, spec).await {
                let rolled = self.rollback(&spec.service, &applied, &before).await;
                bail!(
                    "policy '{}' failed on node '{}': {e:#}; rolled back {:?}",
                    spec.service,
                    node.name,
                    rolled
                );
            }
            applied.push((*node).clone());
        }
        Ok(PolicyApplyReport {
            service: spec.service.clone(),
            applied_nodes: applied.into_iter().map(|n| n.name).collect(),
            rolled_back_nodes: Vec::new(),
        })
    }

    /// Apply policy after merging same-domain remote identities into node ipcache.
    ///
    /// Resolution: `local_identity_ids ∪ same-domain remote directory`.
    /// When `require_resolved` is true, Fabric fails closed if any allow/deny
    /// identity is still unresolved after that merge. When false, Fabric still
    /// fans matching remote CIDRs and leaves residual gaps to FluxVM's
    /// node-local fail-closed compile (`unresolved_identities`).
    #[allow(clippy::too_many_arguments)]
    pub async fn apply_with_remote_directory<R: RemoteIpcacheClient>(
        &self,
        spec: &ServicePolicySpec,
        nodes: &[NodeTarget],
        site_id: Option<&str>,
        route_domain: Option<&str>,
        leases: &[EdgeLease],
        now_unix_ms: u64,
        directory: &RemoteIdentityDirectory,
        remote_client: &R,
        local_identity_ids: &HashSet<u32>,
        require_resolved: bool,
    ) -> Result<PolicyApplyReport> {
        validate_policy(spec)?;
        let report = resolve_policy_identities(
            &spec.allow_identities,
            &spec.deny_identities,
            route_domain,
            directory,
            local_identity_ids,
        )?;
        if require_resolved && !report.unresolved.is_empty() {
            bail!(
                "policy '{}' has unresolved identities after local+remote merge: {:?}",
                spec.service,
                report.unresolved
            );
        }
        let targets = policy_fanout_nodes(
            &spec.service,
            nodes,
            site_id,
            route_domain,
            leases,
            now_unix_ms,
        );
        if targets.is_empty() {
            bail!("policy apply has no nodes in the owning site/route-domain");
        }
        for node in &targets {
            for entry in &report.remote_entries {
                remote_client
                    .upsert_remote(node, entry.identity_id, &entry.cidrs)
                    .await
                    .with_context(|| {
                        format!(
                            "fan remote identity {} onto node {}",
                            entry.identity_id, node.name
                        )
                    })?;
            }
            let _ = remote_client.reconcile_policies(node).await;
        }
        self.apply_with_site(spec, nodes, site_id, route_domain, leases, now_unix_ms)
            .await
    }

    /// Convenience: scope policy fan-out from a [`ServiceSpec`]'s site fields.
    pub async fn apply_for_service(
        &self,
        policy: &ServicePolicySpec,
        service: &ServiceSpec,
        nodes: &[NodeTarget],
        leases: &[EdgeLease],
        now_unix_ms: u64,
    ) -> Result<PolicyApplyReport> {
        self.apply_with_site(
            policy,
            nodes,
            service.site_id.as_deref(),
            service.route_domain.as_deref(),
            leases,
            now_unix_ms,
        )
        .await
    }

    pub async fn delete(&self, service: &str, nodes: &[NodeTarget]) -> Result<PolicyApplyReport> {
        if service.is_empty() {
            bail!("policy service must not be empty");
        }
        if nodes.is_empty() {
            bail!("policy delete requires at least one FluxVM node");
        }
        let mut before = Vec::with_capacity(nodes.len());
        for node in nodes {
            before.push((node.clone(), self.client.get_policy(node, service).await?));
        }
        let mut applied = Vec::new();
        for node in nodes {
            if let Err(e) = self.client.delete_policy(node, service).await {
                let rolled = self.rollback(service, &applied, &before).await;
                bail!(
                    "policy delete '{service}' failed on node '{}': {e:#}; rolled back {:?}",
                    node.name,
                    rolled
                );
            }
            applied.push(node.clone());
        }
        Ok(PolicyApplyReport {
            service: service.to_string(),
            applied_nodes: applied.into_iter().map(|n| n.name).collect(),
            rolled_back_nodes: Vec::new(),
        })
    }

    async fn rollback(
        &self,
        service: &str,
        applied: &[NodeTarget],
        before: &[(NodeTarget, Option<ServicePolicySpec>)],
    ) -> Vec<String> {
        let mut rolled = Vec::new();
        for node in applied.iter().rev() {
            let prior = before
                .iter()
                .find(|(n, _)| n.name == node.name)
                .and_then(|(_, p)| p.clone());
            let result = match prior {
                Some(spec) => self.client.upsert_policy(node, &spec).await.map(|_| ()),
                None => self.client.delete_policy(node, service).await,
            };
            if result.is_ok() {
                rolled.push(node.name.clone());
            }
        }
        rolled
    }
}

/// Select nodes for identity policy fan-out.
///
/// Missing `site_id` and `route_domain` → every node (single-site back-compat).
/// When either is set, only nodes with an active lease in that owning domain
/// for `service` are targeted.
fn policy_fanout_nodes<'a>(
    service: &str,
    nodes: &'a [NodeTarget],
    site_id: Option<&str>,
    route_domain: Option<&str>,
    leases: &[EdgeLease],
    now_unix_ms: u64,
) -> Vec<&'a NodeTarget> {
    if site_id.is_none() && route_domain.is_none() {
        return nodes.iter().collect();
    }
    let want = domain_key(site_id, route_domain);
    let leased: HashSet<&str> = leases
        .iter()
        .filter(|lease| lease.active(service, now_unix_ms))
        .filter(|lease| domain_key(lease.site_id.as_deref(), lease.route_domain.as_deref()) == want)
        .map(|lease| lease.node.as_str())
        .collect();
    nodes
        .iter()
        .filter(|node| leased.contains(node.name.as_str()))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        collections::HashMap,
        sync::{Arc, Mutex},
    };

    #[derive(Clone, Default)]
    struct Fake {
        state: Arc<Mutex<HashMap<String, ServicePolicySpec>>>,
        fail_node: Arc<Mutex<Option<String>>>,
    }
    #[async_trait::async_trait]
    impl PolicyNodeClient for Fake {
        async fn get_policy(
            &self,
            node: &NodeTarget,
            service: &str,
        ) -> Result<Option<ServicePolicySpec>> {
            Ok(self
                .state
                .lock()
                .unwrap()
                .get(&format!("{}|{service}", node.name))
                .cloned())
        }
        async fn upsert_policy(
            &self,
            node: &NodeTarget,
            spec: &ServicePolicySpec,
        ) -> Result<ServicePolicyStatus> {
            if self.fail_node.lock().unwrap().as_deref() == Some(&node.name) {
                bail!("injected");
            }
            self.state
                .lock()
                .unwrap()
                .insert(format!("{}|{}", node.name, spec.service), spec.clone());
            Ok(ServicePolicyStatus {
                schema_version: 1,
                program_generation: 6,
                service: spec.service.clone(),
                service_id: 1,
                enabled: true,
                compiled_ipv4: 0,
                compiled_ipv6: 0,
                unresolved_identities: vec![],
                l7_ready: true,
            })
        }
        async fn delete_policy(&self, node: &NodeTarget, service: &str) -> Result<()> {
            self.state
                .lock()
                .unwrap()
                .remove(&format!("{}|{service}", node.name));
            Ok(())
        }
        async fn envoy_contract(
            &self,
            _: &NodeTarget,
            service: &str,
        ) -> Result<EnvoyRedirectContract> {
            Ok(EnvoyRedirectContract {
                schema_version: 1,
                service: service.into(),
                enabled: false,
                protocol: None,
                mode: None,
                proxy_ifindex: 0,
                bypass_mark: 0,
                preserve_original_destination: true,
                authorities: vec![],
                path_prefixes: vec![],
            })
        }
    }

    #[tokio::test]
    async fn rollback_after_partial_failure() {
        let fake = Fake::default();
        *fake.fail_node.lock().unwrap() = Some("b".into());
        let orch = PolicyOrchestrator::new(fake.clone());
        let nodes = vec![
            NodeTarget {
                name: "a".into(),
                base_url: "http://a".into(),
                token: None,
            },
            NodeTarget {
                name: "b".into(),
                base_url: "http://b".into(),
                token: None,
            },
        ];
        let spec = ServicePolicySpec {
            service: "web".into(),
            enabled: true,
            default_action: PolicyDefaultAction::Deny,
            allow_identities: vec![1],
            deny_identities: vec![],
            audit_only: false,
            l7: None,
        };
        assert!(orch.apply(&spec, &nodes).await.is_err());
        assert!(fake.state.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn site_scoped_policy_fans_out_only_matching_leases() {
        let fake = Fake::default();
        let orch = PolicyOrchestrator::new(fake.clone());
        let nodes = vec![
            NodeTarget {
                name: "a".into(),
                base_url: "http://a".into(),
                token: None,
            },
            NodeTarget {
                name: "b".into(),
                base_url: "http://b".into(),
                token: None,
            },
        ];
        let policy = ServicePolicySpec {
            service: "web".into(),
            enabled: true,
            default_action: PolicyDefaultAction::Deny,
            allow_identities: vec![1],
            deny_identities: vec![],
            audit_only: false,
            l7: None,
        };
        let leases = vec![
            EdgeLease {
                service: "web".into(),
                node: "a".into(),
                epoch: 1,
                expires_unix_ms: 200,
                site_id: Some("site-a".into()),
                route_domain: Some("rd-1".into()),
            },
            EdgeLease {
                service: "web".into(),
                node: "b".into(),
                epoch: 1,
                expires_unix_ms: 200,
                site_id: Some("site-b".into()),
                route_domain: Some("rd-2".into()),
            },
        ];
        let report = orch
            .apply_with_site(&policy, &nodes, Some("site-a"), Some("rd-1"), &leases, 100)
            .await
            .unwrap();
        assert_eq!(report.applied_nodes, vec!["a"]);
        let state = fake.state.lock().unwrap();
        assert!(state.contains_key("a|web"));
        assert!(!state.contains_key("b|web"));
    }

    #[tokio::test]
    async fn policy_apply_merges_same_domain_remote_and_fail_closed() {
        use crate::remote_identity::{
            RemoteIdentity, RemoteIdentityDirectory, RemoteIpcacheClient,
        };
        use async_trait::async_trait;
        use std::collections::BTreeMap;
        use tempfile::tempdir;

        #[derive(Clone, Default)]
        struct FakeRemote {
            cidrs: Arc<Mutex<BTreeMap<String, Vec<String>>>>,
        }
        #[async_trait]
        impl RemoteIpcacheClient for FakeRemote {
            async fn upsert_remote(
                &self,
                node: &NodeTarget,
                identity: u32,
                cidrs: &[String],
            ) -> Result<()> {
                self.cidrs
                    .lock()
                    .unwrap()
                    .insert(format!("{}|{identity}", node.name), cidrs.to_vec());
                Ok(())
            }
            async fn delete_remote(&self, _: &NodeTarget, _: u32) -> Result<()> {
                Ok(())
            }
        }

        let dir = tempdir().unwrap();
        let store = RemoteIdentityDirectory::new(dir.path().join("r.json"));
        store
            .upsert(RemoteIdentity {
                identity_id: 99,
                site_id: "site-a".into(),
                route_domain: "rd-1".into(),
                cidrs: vec!["10.9.9.9/32".into()],
                labels: BTreeMap::new(),
                updated_unix_ms: 1,
            })
            .unwrap();

        let fake = Fake::default();
        let remote = FakeRemote::default();
        let orch = PolicyOrchestrator::new(fake.clone());
        let nodes = vec![NodeTarget {
            name: "a".into(),
            base_url: "http://a".into(),
            token: None,
        }];
        let policy = ServicePolicySpec {
            service: "web".into(),
            enabled: true,
            default_action: PolicyDefaultAction::Deny,
            allow_identities: vec![99],
            deny_identities: vec![],
            audit_only: false,
            l7: None,
        };
        orch.apply_with_remote_directory(
            &policy,
            &nodes,
            None,
            Some("rd-1"),
            &[EdgeLease {
                service: "web".into(),
                node: "a".into(),
                epoch: 1,
                expires_unix_ms: 999,
                site_id: None,
                route_domain: Some("rd-1".into()),
            }],
            100,
            &store,
            &remote,
            &HashSet::new(),
            true,
        )
        .await
        .unwrap();
        assert_eq!(
            remote.cidrs.lock().unwrap().get("a|99").cloned().unwrap(),
            vec!["10.9.9.9/32".to_string()]
        );

        // Cross-domain-only: identity 99 is not in rd-2 → fail-closed when required.
        let policy2 = ServicePolicySpec {
            allow_identities: vec![99],
            ..policy.clone()
        };
        let err = orch
            .apply_with_remote_directory(
                &policy2,
                &nodes,
                None,
                Some("rd-2"),
                &[EdgeLease {
                    service: "web".into(),
                    node: "a".into(),
                    epoch: 1,
                    expires_unix_ms: 999,
                    site_id: None,
                    route_domain: Some("rd-2".into()),
                }],
                100,
                &store,
                &remote,
                &HashSet::new(),
                true,
            )
            .await
            .unwrap_err();
        assert!(err.to_string().contains("unresolved"));
    }
}
