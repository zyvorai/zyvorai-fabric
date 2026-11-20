// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: LicenseRef-Zyvor-Production-1.0

//! Minimal ClusterMesh-like remote identity directory.
//!
//! Fabric owns publish/pull of peer identities keyed by
//! `(route_domain, identity_id)`. Reconcile fans CIDRs into FluxVM remote
//! ipcache on target nodes so service policy compile can resolve cross-site
//! `allow_identities` / `deny_identities`. Remote backend mesh lives in
//! [`crate::remote_backend`].

use crate::{domain_key, EdgeLease, NodeTarget};
use anyhow::{bail, Context, Result};
use async_trait::async_trait;
use reqwest::StatusCode;
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, HashSet},
    fs,
    path::{Path, PathBuf},
};

const MAX_CIDRS: usize = 1024;
const MAX_LABELS: usize = 64;

/// Published remote (peer-site) identity for a route domain.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RemoteIdentity {
    pub identity_id: u32,
    #[serde(default = "default_site")]
    pub site_id: String,
    #[serde(default = "default_route_domain")]
    pub route_domain: String,
    #[serde(default)]
    pub cidrs: Vec<String>,
    #[serde(default)]
    pub labels: BTreeMap<String, String>,
    /// Filled by Fabric on upsert when omitted / zero.
    #[serde(default)]
    pub updated_unix_ms: u64,
}

fn default_site() -> String {
    "default".into()
}
fn default_route_domain() -> String {
    "default".into()
}

fn directory_key(route_domain: &str, identity_id: u32) -> String {
    format!("{route_domain}|{identity_id}")
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct RemoteIdentityState {
    pub generation: u64,
    #[serde(default)]
    pub identities: BTreeMap<String, RemoteIdentity>,
}

/// Durable Fabric-owned remote identity catalog.
#[derive(Debug, Clone)]
pub struct RemoteIdentityDirectory {
    path: PathBuf,
}

impl RemoteIdentityDirectory {
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self { path: path.into() }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn load(&self) -> Result<RemoteIdentityState> {
        if !self.path.exists() {
            return Ok(RemoteIdentityState::default());
        }
        let raw = fs::read(&self.path)
            .with_context(|| format!("reading remote identity store {}", self.path.display()))?;
        serde_json::from_slice(&raw)
            .with_context(|| format!("parsing remote identity store {}", self.path.display()))
    }

    pub fn save(&self, state: &RemoteIdentityState) -> Result<()> {
        crate::atomic_write(&self.path, &serde_json::to_vec_pretty(state)?)
    }

    pub fn upsert(&self, mut identity: RemoteIdentity) -> Result<RemoteIdentity> {
        validate_remote_identity(&identity)?;
        if identity.site_id.is_empty() {
            identity.site_id = default_site();
        }
        if identity.route_domain.is_empty() {
            identity.route_domain = default_route_domain();
        }
        let key = directory_key(&identity.route_domain, identity.identity_id);
        let mut state = self.load()?;
        state.identities.insert(key, identity.clone());
        state.generation = state.generation.saturating_add(1);
        self.save(&state)?;
        Ok(identity)
    }

    pub fn get(&self, route_domain: &str, identity_id: u32) -> Result<Option<RemoteIdentity>> {
        let rd = if route_domain.is_empty() {
            default_route_domain()
        } else {
            route_domain.to_string()
        };
        Ok(self
            .load()?
            .identities
            .get(&directory_key(&rd, identity_id))
            .cloned())
    }

    pub fn list(
        &self,
        site_id: Option<&str>,
        route_domain: Option<&str>,
    ) -> Result<Vec<RemoteIdentity>> {
        let state = self.load()?;
        let mut out: Vec<_> = state
            .identities
            .into_values()
            .filter(|id| {
                if let Some(site) = site_id {
                    if id.site_id != site {
                        return false;
                    }
                }
                if let Some(rd) = route_domain {
                    let want = if rd.is_empty() {
                        default_route_domain()
                    } else {
                        rd.to_string()
                    };
                    if id.route_domain != want {
                        return false;
                    }
                }
                true
            })
            .collect();
        out.sort_by(|a, b| (&a.route_domain, a.identity_id).cmp(&(&b.route_domain, b.identity_id)));
        Ok(out)
    }

    /// Delete by `(route_domain, identity_id)`. Returns the removed entry if any.
    pub fn delete(&self, route_domain: &str, identity_id: u32) -> Result<Option<RemoteIdentity>> {
        let rd = if route_domain.is_empty() {
            default_route_domain()
        } else {
            route_domain.to_string()
        };
        let key = directory_key(&rd, identity_id);
        let mut state = self.load()?;
        let removed = state.identities.remove(&key);
        if removed.is_some() {
            state.generation = state.generation.saturating_add(1);
            self.save(&state)?;
        }
        Ok(removed)
    }

    /// Same-domain directory entries for the given identity IDs.
    pub fn lookup_same_domain(
        &self,
        route_domain: Option<&str>,
        identity_ids: &[u32],
    ) -> Result<Vec<RemoteIdentity>> {
        let want_rd = route_domain.unwrap_or("default");
        let want: HashSet<u32> = identity_ids.iter().copied().collect();
        Ok(self
            .list(None, Some(want_rd))?
            .into_iter()
            .filter(|id| want.contains(&id.identity_id))
            .collect())
    }
}

pub fn validate_remote_identity(id: &RemoteIdentity) -> Result<()> {
    if id.identity_id == 0 {
        bail!("identity_id 0 is reserved/unresolved");
    }
    if id.cidrs.len() > MAX_CIDRS {
        bail!("remote identity cidrs bounded to {MAX_CIDRS}");
    }
    if id.labels.len() > MAX_LABELS {
        bail!("remote identity labels bounded to {MAX_LABELS}");
    }
    if id.cidrs.is_empty() {
        bail!("remote identity requires at least one cidr");
    }
    let mut seen = HashSet::new();
    for c in &id.cidrs {
        if c.is_empty() || c.len() > 128 {
            bail!("invalid remote identity cidr length");
        }
        if !seen.insert(c.as_str()) {
            bail!("duplicate remote identity cidr");
        }
        let host = c.split('/').next().unwrap_or(c);
        if host.parse::<std::net::IpAddr>().is_err() {
            bail!("invalid remote identity cidr host {c:?}");
        }
    }
    Ok(())
}

/// Merge local node identities with same-domain remote directory entries.
///
/// Identities present in `local_identity_ids` or in the remote directory for
/// `route_domain` are resolved. Cross-domain remote rows are ignored. Returns
/// unresolved IDs (caller should fail-closed).
pub fn resolve_policy_identities(
    allow_identities: &[u32],
    deny_identities: &[u32],
    route_domain: Option<&str>,
    directory: &RemoteIdentityDirectory,
    local_identity_ids: &HashSet<u32>,
) -> Result<ResolveReport> {
    let needed: Vec<u32> = allow_identities
        .iter()
        .chain(deny_identities.iter())
        .copied()
        .collect::<HashSet<_>>()
        .into_iter()
        .collect();
    let remote = directory.lookup_same_domain(route_domain, &needed)?;
    let remote_ids: HashSet<u32> = remote.iter().map(|r| r.identity_id).collect();
    let mut resolved_remote = Vec::new();
    let mut unresolved = Vec::new();
    for id in &needed {
        if remote_ids.contains(id) {
            resolved_remote.push(*id);
        } else if local_identity_ids.contains(id) {
            // local only
        } else {
            unresolved.push(*id);
        }
    }
    resolved_remote.sort_unstable();
    unresolved.sort_unstable();
    Ok(ResolveReport {
        route_domain: route_domain.unwrap_or("default").to_string(),
        resolved_remote,
        unresolved,
        remote_entries: remote,
    })
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ResolveReport {
    pub route_domain: String,
    pub resolved_remote: Vec<u32>,
    pub unresolved: Vec<u32>,
    pub remote_entries: Vec<RemoteIdentity>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RemoteReconcileReport {
    pub route_domain: String,
    pub identities: Vec<u32>,
    pub applied_nodes: Vec<String>,
    pub deleted: bool,
}

#[async_trait]
pub trait RemoteIpcacheClient: Send + Sync {
    async fn upsert_remote(&self, node: &NodeTarget, identity: u32, cidrs: &[String])
        -> Result<()>;
    async fn delete_remote(&self, node: &NodeTarget, identity: u32) -> Result<()>;
    /// Refresh FluxVM sid maps after ipcache mutation (best-effort optional).
    async fn reconcile_policies(&self, node: &NodeTarget) -> Result<()> {
        let _ = node;
        Ok(())
    }
}

pub struct FluxVmRemoteIpcacheHttpClient {
    http: reqwest::Client,
}

impl FluxVmRemoteIpcacheHttpClient {
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

#[async_trait]
impl RemoteIpcacheClient for FluxVmRemoteIpcacheHttpClient {
    async fn upsert_remote(
        &self,
        node: &NodeTarget,
        identity: u32,
        cidrs: &[String],
    ) -> Result<()> {
        let body = serde_json::json!({ "identity": identity, "cidrs": cidrs });
        let resp = self
            .auth(
                node,
                self.http
                    .post(Self::url(node, "/v1/network/ipcache/remote")?),
            )
            .json(&body)
            .send()
            .await?;
        Self::expect(resp, "upsert remote ipcache").await?;
        Ok(())
    }

    async fn delete_remote(&self, node: &NodeTarget, identity: u32) -> Result<()> {
        let resp = self
            .auth(
                node,
                self.http.delete(Self::url(
                    node,
                    &format!("/v1/network/ipcache/remote/{identity}"),
                )?),
            )
            .send()
            .await?;
        if resp.status() == StatusCode::NOT_FOUND {
            return Ok(());
        }
        Self::expect(resp, "delete remote ipcache").await?;
        Ok(())
    }

    async fn reconcile_policies(&self, node: &NodeTarget) -> Result<()> {
        let resp = self
            .auth(
                node,
                self.http
                    .post(Self::url(node, "/v1/network/services/policies/reconcile")?),
            )
            .send()
            .await?;
        Self::expect(resp, "reconcile service policies").await?;
        Ok(())
    }
}

pub struct RemoteIdentityOrchestrator<C> {
    directory: RemoteIdentityDirectory,
    client: C,
}

impl<C: RemoteIpcacheClient> RemoteIdentityOrchestrator<C> {
    pub fn new(directory: RemoteIdentityDirectory, client: C) -> Self {
        Self { directory, client }
    }

    pub fn directory(&self) -> &RemoteIdentityDirectory {
        &self.directory
    }

    /// Fan directory entries for `route_domain` onto nodes (optionally lease-scoped).
    pub async fn reconcile(
        &self,
        route_domain: Option<&str>,
        site_id: Option<&str>,
        nodes: &[NodeTarget],
        leases: &[EdgeLease],
        service_for_leases: Option<&str>,
        now_unix_ms: u64,
    ) -> Result<RemoteReconcileReport> {
        if nodes.is_empty() {
            bail!("remote identity reconcile requires at least one FluxVM node");
        }
        let rd = route_domain.unwrap_or("default");
        let entries = self.directory.list(site_id, Some(rd))?;
        let targets = reconcile_targets(
            nodes,
            site_id,
            Some(rd),
            leases,
            service_for_leases,
            now_unix_ms,
        );
        if targets.is_empty() {
            bail!("remote identity reconcile has no nodes in the owning site/route-domain");
        }
        let mut applied = Vec::new();
        for node in &targets {
            for entry in &entries {
                self.client
                    .upsert_remote(node, entry.identity_id, &entry.cidrs)
                    .await
                    .with_context(|| {
                        format!(
                            "upsert remote identity {} on node {}",
                            entry.identity_id, node.name
                        )
                    })?;
            }
            let _ = self.client.reconcile_policies(node).await;
            applied.push((*node).clone());
        }
        Ok(RemoteReconcileReport {
            route_domain: rd.to_string(),
            identities: entries.iter().map(|e| e.identity_id).collect(),
            applied_nodes: applied.into_iter().map(|n| n.name).collect(),
            deleted: false,
        })
    }

    /// Delete from directory and strip remote ipcache rows on fan-out nodes.
    #[allow(clippy::too_many_arguments)]
    pub async fn delete_and_unfan(
        &self,
        route_domain: &str,
        identity_id: u32,
        nodes: &[NodeTarget],
        site_id: Option<&str>,
        leases: &[EdgeLease],
        service_for_leases: Option<&str>,
        now_unix_ms: u64,
    ) -> Result<RemoteReconcileReport> {
        let removed = self.directory.delete(route_domain, identity_id)?;
        if removed.is_none() {
            bail!("remote identity {identity_id} not found in route_domain {route_domain}");
        }
        let rd = if route_domain.is_empty() {
            "default"
        } else {
            route_domain
        };
        let targets = reconcile_targets(
            nodes,
            site_id,
            Some(rd),
            leases,
            service_for_leases,
            now_unix_ms,
        );
        let mut applied = Vec::new();
        for node in &targets {
            self.client.delete_remote(node, identity_id).await?;
            let _ = self.client.reconcile_policies(node).await;
            applied.push(node.name.clone());
        }
        Ok(RemoteReconcileReport {
            route_domain: rd.to_string(),
            identities: vec![identity_id],
            applied_nodes: applied,
            deleted: true,
        })
    }

    /// Upsert one identity then fan it to domain nodes.
    pub async fn upsert_and_reconcile(
        &self,
        identity: RemoteIdentity,
        nodes: &[NodeTarget],
        leases: &[EdgeLease],
        service_for_leases: Option<&str>,
        now_unix_ms: u64,
    ) -> Result<(RemoteIdentity, RemoteReconcileReport)> {
        let stored = self.directory.upsert(identity)?;
        let report = self
            .reconcile(
                Some(&stored.route_domain),
                Some(&stored.site_id),
                nodes,
                leases,
                service_for_leases,
                now_unix_ms,
            )
            .await?;
        Ok((stored, report))
    }
}

fn reconcile_targets<'a>(
    nodes: &'a [NodeTarget],
    site_id: Option<&str>,
    route_domain: Option<&str>,
    leases: &[EdgeLease],
    service_for_leases: Option<&str>,
    now_unix_ms: u64,
) -> Vec<&'a NodeTarget> {
    if site_id.is_none() && route_domain.is_none() {
        return nodes.iter().collect();
    }
    // Without service leases, fall back to all nodes when fencing fields are set
    // but no service context is provided (directory-wide reconcile).
    let Some(service) = service_for_leases else {
        return nodes.iter().collect();
    };
    let want = domain_key(site_id, route_domain);
    let leased: HashSet<&str> = leases
        .iter()
        .filter(|lease| lease.active(service, now_unix_ms))
        .filter(|lease| domain_key(lease.site_id.as_deref(), lease.route_domain.as_deref()) == want)
        .map(|lease| lease.node.as_str())
        .collect();
    if leased.is_empty() {
        // No matching leases → still fan to all nodes for directory sync so
        // publish works before service leases exist.
        return nodes.iter().collect();
    }
    nodes
        .iter()
        .filter(|node| leased.contains(node.name.as_str()))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Mutex};
    use tempfile::tempdir;

    #[derive(Clone, Default)]
    struct FakeIpcache {
        /// node -> identity -> cidrs
        state: Arc<Mutex<BTreeMap<String, BTreeMap<u32, Vec<String>>>>>,
    }

    #[async_trait]
    impl RemoteIpcacheClient for FakeIpcache {
        async fn upsert_remote(
            &self,
            node: &NodeTarget,
            identity: u32,
            cidrs: &[String],
        ) -> Result<()> {
            self.state
                .lock()
                .unwrap()
                .entry(node.name.clone())
                .or_default()
                .insert(identity, cidrs.to_vec());
            Ok(())
        }
        async fn delete_remote(&self, node: &NodeTarget, identity: u32) -> Result<()> {
            if let Some(m) = self.state.lock().unwrap().get_mut(&node.name) {
                m.remove(&identity);
            }
            Ok(())
        }
    }

    fn nodes() -> Vec<NodeTarget> {
        vec![
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
        ]
    }

    #[test]
    fn same_domain_remote_identity_resolves() {
        let dir = tempdir().unwrap();
        let store = RemoteIdentityDirectory::new(dir.path().join("remote.json"));
        store
            .upsert(RemoteIdentity {
                identity_id: 42,
                site_id: "site-a".into(),
                route_domain: "rd-1".into(),
                cidrs: vec!["10.10.0.5/32".into()],
                labels: BTreeMap::new(),
                updated_unix_ms: 1,
            })
            .unwrap();
        let report =
            resolve_policy_identities(&[42], &[], Some("rd-1"), &store, &HashSet::new()).unwrap();
        assert_eq!(report.resolved_remote, vec![42]);
        assert!(report.unresolved.is_empty());
    }

    #[test]
    fn cross_domain_remote_identity_ignored() {
        let dir = tempdir().unwrap();
        let store = RemoteIdentityDirectory::new(dir.path().join("remote.json"));
        store
            .upsert(RemoteIdentity {
                identity_id: 42,
                site_id: "site-b".into(),
                route_domain: "rd-other".into(),
                cidrs: vec!["10.10.0.5/32".into()],
                labels: BTreeMap::new(),
                updated_unix_ms: 1,
            })
            .unwrap();
        let report =
            resolve_policy_identities(&[42], &[], Some("rd-1"), &store, &HashSet::new()).unwrap();
        assert!(report.resolved_remote.is_empty());
        assert_eq!(report.unresolved, vec![42]);
    }

    #[tokio::test]
    async fn delete_removes_fan_out() {
        let dir = tempdir().unwrap();
        let store = RemoteIdentityDirectory::new(dir.path().join("remote.json"));
        let fake = FakeIpcache::default();
        let orch = RemoteIdentityOrchestrator::new(store, fake.clone());
        orch.directory
            .upsert(RemoteIdentity {
                identity_id: 7,
                site_id: "site-a".into(),
                route_domain: "rd-1".into(),
                cidrs: vec!["10.0.0.7/32".into()],
                labels: BTreeMap::new(),
                updated_unix_ms: 1,
            })
            .unwrap();
        orch.reconcile(Some("rd-1"), None, &nodes(), &[], None, 0)
            .await
            .unwrap();
        assert!(fake
            .state
            .lock()
            .unwrap()
            .get("a")
            .unwrap()
            .contains_key(&7));

        orch.delete_and_unfan("rd-1", 7, &nodes(), None, &[], None, 0)
            .await
            .unwrap();
        assert!(!fake
            .state
            .lock()
            .unwrap()
            .get("a")
            .unwrap()
            .contains_key(&7));
        assert!(orch.directory.get("rd-1", 7).unwrap().is_none());
    }

    #[tokio::test]
    async fn reconcile_fans_same_domain_only() {
        let dir = tempdir().unwrap();
        let store = RemoteIdentityDirectory::new(dir.path().join("remote.json"));
        let fake = FakeIpcache::default();
        let orch = RemoteIdentityOrchestrator::new(store, fake.clone());
        orch.directory
            .upsert(RemoteIdentity {
                identity_id: 1,
                site_id: "site-a".into(),
                route_domain: "rd-1".into(),
                cidrs: vec!["10.0.0.1/32".into()],
                labels: BTreeMap::new(),
                updated_unix_ms: 1,
            })
            .unwrap();
        orch.directory
            .upsert(RemoteIdentity {
                identity_id: 2,
                site_id: "site-b".into(),
                route_domain: "rd-2".into(),
                cidrs: vec!["10.0.0.2/32".into()],
                labels: BTreeMap::new(),
                updated_unix_ms: 1,
            })
            .unwrap();
        let report = orch
            .reconcile(Some("rd-1"), None, &nodes(), &[], None, 0)
            .await
            .unwrap();
        assert_eq!(report.identities, vec![1]);
        let a = fake.state.lock().unwrap().get("a").cloned().unwrap();
        assert!(a.contains_key(&1));
        assert!(!a.contains_key(&2));
    }
}
