// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: LicenseRef-Zyvor-Production-2.0

use anyhow::Result;
use chrono::Utc;
use std::collections::{BTreeMap, HashMap};
use std::sync::{Arc, RwLock};

use crate::models::{SecurityIdentity, IDENTITY_USER_MIN};

/// Allocates and manages security identities. Each unique set of labels gets a
/// single numeric identity ID. Thread-safe via Arc<RwLock<...>>.
#[derive(Debug, Clone)]
pub struct IdentityAllocator {
    inner: Arc<RwLock<Inner>>,
}

#[derive(Debug)]
struct Inner {
    /// Canonical label key → identity
    identities: HashMap<String, SecurityIdentity>,
    /// Reverse map: identity ID → canonical key
    id_to_key: HashMap<u32, String>,
    /// IP address → identity ID
    ip_map: HashMap<String, u32>,
    /// Next identity ID to allocate
    next_id: u32,
}

impl Default for IdentityAllocator {
    fn default() -> Self {
        Self::new()
    }
}

impl IdentityAllocator {
    pub fn new() -> Self {
        Self {
            inner: Arc::new(RwLock::new(Inner {
                identities: HashMap::new(),
                id_to_key: HashMap::new(),
                ip_map: HashMap::new(),
                next_id: IDENTITY_USER_MIN,
            })),
        }
    }

    /// Produces a deterministic canonical key from labels, e.g. "app=web,env=prod".
    /// Uses BTreeMap ordering for consistency.
    pub fn canonical_key(labels: &HashMap<String, String>) -> String {
        let sorted: BTreeMap<&String, &String> = labels.iter().collect();
        sorted
            .iter()
            .map(|(k, v)| format!("{}={}", k, v))
            .collect::<Vec<_>>()
            .join(",")
    }

    /// Allocate a new identity for the given labels, or return the existing one.
    /// The vm_name is added as an endpoint to the identity.
    pub fn allocate_or_get(&self, labels: &HashMap<String, String>, vm_name: &str) -> Result<u32> {
        let key = Self::canonical_key(labels);
        let mut inner = self
            .inner
            .write()
            .map_err(|e| anyhow::anyhow!("Lock poisoned: {}", e))?;

        if let Some(identity) = inner.identities.get_mut(&key) {
            if !identity.endpoints.contains(&vm_name.to_string()) {
                identity.endpoints.push(vm_name.to_string());
                identity.updated = Utc::now();
            }
            return Ok(identity.id);
        }

        let id = inner.next_id;
        inner.next_id += 1;

        let sorted_labels: BTreeMap<String, String> =
            labels.iter().map(|(k, v)| (k.clone(), v.clone())).collect();
        let now = Utc::now();

        let identity = SecurityIdentity {
            id,
            labels: sorted_labels,
            endpoints: vec![vm_name.to_string()],
            description: String::new(),
            managed: true,
            created: now,
            updated: now,
        };

        inner.id_to_key.insert(id, key.clone());
        inner.identities.insert(key, identity);

        Ok(id)
    }

    /// Remove an endpoint from its identity. If the identity has no more endpoints,
    /// it is garbage-collected.
    pub fn deallocate(&self, vm_name: &str, labels: &HashMap<String, String>) -> Result<()> {
        let key = Self::canonical_key(labels);
        let mut inner = self
            .inner
            .write()
            .map_err(|e| anyhow::anyhow!("Lock poisoned: {}", e))?;

        if let Some(identity) = inner.identities.get_mut(&key) {
            identity.endpoints.retain(|e| e != vm_name);
            identity.updated = Utc::now();

            if identity.endpoints.is_empty() {
                let id = identity.id;
                inner.identities.remove(&key);
                inner.id_to_key.remove(&id);
            }
        }

        Ok(())
    }

    /// Map an IP address to an identity ID.
    pub fn update_ip_mapping(&self, ip: &str, identity_id: u32) -> Result<()> {
        let mut inner = self
            .inner
            .write()
            .map_err(|e| anyhow::anyhow!("Lock poisoned: {}", e))?;
        inner.ip_map.insert(ip.to_string(), identity_id);
        Ok(())
    }

    /// Remove an IP address mapping.
    pub fn remove_ip_mapping(&self, ip: &str) -> Result<()> {
        let mut inner = self
            .inner
            .write()
            .map_err(|e| anyhow::anyhow!("Lock poisoned: {}", e))?;
        inner.ip_map.remove(ip);
        Ok(())
    }

    /// Get a security identity by its numeric ID.
    pub fn get_identity(&self, id: u32) -> Option<SecurityIdentity> {
        let inner = self.inner.read().ok()?;
        let key = inner.id_to_key.get(&id)?;
        inner.identities.get(key).cloned()
    }

    /// Get the identity ID for a set of labels, if one exists.
    pub fn get_identity_for_labels(&self, labels: &HashMap<String, String>) -> Option<u32> {
        let key = Self::canonical_key(labels);
        let inner = self.inner.read().ok()?;
        inner.identities.get(&key).map(|i| i.id)
    }

    /// Get the identity ID for an IP address.
    pub fn get_identity_for_ip(&self, ip: &str) -> Option<u32> {
        let inner = self.inner.read().ok()?;
        inner.ip_map.get(ip).copied()
    }

    /// List all current security identities.
    pub fn list_identities(&self) -> Vec<SecurityIdentity> {
        let inner = self.inner.read().unwrap_or_else(|e| e.into_inner());
        inner.identities.values().cloned().collect()
    }

    /// Get the current IP → identity mapping.
    pub fn get_ip_map(&self) -> HashMap<String, u32> {
        let inner = self.inner.read().unwrap_or_else(|e| e.into_inner());
        inner.ip_map.clone()
    }

    /// Get all IPs mapped to a specific identity.
    pub fn get_identity_ips(&self, identity_id: u32) -> Vec<String> {
        let inner = self.inner.read().unwrap_or_else(|e| e.into_inner());
        inner
            .ip_map
            .iter()
            .filter(|(_, &id)| id == identity_id)
            .map(|(ip, _)| ip.clone())
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn labels(pairs: &[(&str, &str)]) -> HashMap<String, String> {
        pairs
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect()
    }

    #[test]
    fn test_new_allocation() {
        let alloc = IdentityAllocator::new();
        let id = alloc
            .allocate_or_get(&labels(&[("app", "web")]), "vm-1")
            .unwrap();
        assert!(id >= IDENTITY_USER_MIN);
    }

    #[test]
    fn test_same_labels_reuse() {
        let alloc = IdentityAllocator::new();
        let id1 = alloc
            .allocate_or_get(&labels(&[("app", "web")]), "vm-1")
            .unwrap();
        let id2 = alloc
            .allocate_or_get(&labels(&[("app", "web")]), "vm-2")
            .unwrap();
        assert_eq!(id1, id2);

        let identity = alloc.get_identity(id1).unwrap();
        assert_eq!(identity.endpoints.len(), 2);
    }

    #[test]
    fn test_different_labels_different_id() {
        let alloc = IdentityAllocator::new();
        let id1 = alloc
            .allocate_or_get(&labels(&[("app", "web")]), "vm-1")
            .unwrap();
        let id2 = alloc
            .allocate_or_get(&labels(&[("app", "db")]), "vm-2")
            .unwrap();
        assert_ne!(id1, id2);
    }

    #[test]
    fn test_deallocate_keeps_identity_with_other_endpoints() {
        let alloc = IdentityAllocator::new();
        let id = alloc
            .allocate_or_get(&labels(&[("app", "web")]), "vm-1")
            .unwrap();
        alloc
            .allocate_or_get(&labels(&[("app", "web")]), "vm-2")
            .unwrap();

        alloc
            .deallocate("vm-1", &labels(&[("app", "web")]))
            .unwrap();

        let identity = alloc.get_identity(id).unwrap();
        assert_eq!(identity.endpoints, vec!["vm-2".to_string()]);
    }

    #[test]
    fn test_deallocate_removes_empty_identity() {
        let alloc = IdentityAllocator::new();
        let id = alloc
            .allocate_or_get(&labels(&[("app", "web")]), "vm-1")
            .unwrap();
        alloc
            .deallocate("vm-1", &labels(&[("app", "web")]))
            .unwrap();
        assert!(alloc.get_identity(id).is_none());
    }

    #[test]
    fn test_ip_lifecycle() {
        let alloc = IdentityAllocator::new();
        let id = alloc
            .allocate_or_get(&labels(&[("app", "web")]), "vm-1")
            .unwrap();

        alloc.update_ip_mapping("10.0.0.5", id).unwrap();
        assert_eq!(alloc.get_identity_for_ip("10.0.0.5"), Some(id));

        alloc.remove_ip_mapping("10.0.0.5").unwrap();
        assert_eq!(alloc.get_identity_for_ip("10.0.0.5"), None);
    }

    #[test]
    fn test_canonical_key_ordering() {
        // Different insertion order should produce same key
        let key1 = IdentityAllocator::canonical_key(&labels(&[("b", "2"), ("a", "1")]));
        let key2 = IdentityAllocator::canonical_key(&labels(&[("a", "1"), ("b", "2")]));
        assert_eq!(key1, key2);
        assert_eq!(key1, "a=1,b=2");
    }

    #[test]
    fn test_monotonic_ids() {
        let alloc = IdentityAllocator::new();
        let id1 = alloc
            .allocate_or_get(&labels(&[("a", "1")]), "vm-1")
            .unwrap();
        let id2 = alloc
            .allocate_or_get(&labels(&[("b", "2")]), "vm-2")
            .unwrap();
        let id3 = alloc
            .allocate_or_get(&labels(&[("c", "3")]), "vm-3")
            .unwrap();
        assert!(id1 < id2);
        assert!(id2 < id3);
    }

    #[test]
    fn test_list_identities() {
        let alloc = IdentityAllocator::new();
        alloc
            .allocate_or_get(&labels(&[("app", "web")]), "vm-1")
            .unwrap();
        alloc
            .allocate_or_get(&labels(&[("app", "db")]), "vm-2")
            .unwrap();

        let list = alloc.list_identities();
        assert_eq!(list.len(), 2);
    }

    #[test]
    fn test_get_identity_ips() {
        let alloc = IdentityAllocator::new();
        let id = alloc
            .allocate_or_get(&labels(&[("app", "web")]), "vm-1")
            .unwrap();

        alloc.update_ip_mapping("10.0.0.5", id).unwrap();
        alloc.update_ip_mapping("10.0.0.6", id).unwrap();

        let ips = alloc.get_identity_ips(id);
        assert_eq!(ips.len(), 2);
        assert!(ips.contains(&"10.0.0.5".to_string()));
        assert!(ips.contains(&"10.0.0.6".to_string()));
    }
}
