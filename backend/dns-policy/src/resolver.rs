// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: LicenseRef-Zyvor-Production-1.0

use std::collections::{HashMap, HashSet};

use crate::models::*;

/// A snapshot of a VM's state for DNS resolution.
pub struct VMSnapshot {
    pub name: String,
    pub labels: HashMap<String, String>,
    pub ip: Option<String>,
}

/// Resolves DNS policies against VM state to produce DNS records.
pub struct DnsResolver;

impl Default for DnsResolver {
    fn default() -> Self {
        Self::new()
    }
}

impl DnsResolver {
    pub fn new() -> Self {
        Self
    }

    /// Resolve a single policy against VM state to produce DNS records.
    pub fn resolve_policy(
        &self,
        policy: &DnsPolicy,
        zone: &DnsZone,
        all_vms: &[VMSnapshot],
    ) -> Vec<DnsRecord> {
        if !policy.enabled {
            return vec![];
        }

        let matching_vms: Vec<&VMSnapshot> = all_vms
            .iter()
            .filter(|vm| policy.selector.matches(&vm.labels) && vm.ip.is_some())
            .collect();

        matching_vms
            .into_iter()
            .filter_map(|vm| {
                let hostname =
                    self.expand_template(&policy.record_template, &vm.name, &vm.labels, &zone.name);
                let value = match policy.record_type {
                    DnsRecordType::A => vm.ip.clone()?,
                    DnsRecordType::Cname => hostname.clone(),
                    DnsRecordType::Srv => {
                        // SRV records point to the A record hostname
                        format!("0 10 0 {}", hostname)
                    }
                };

                Some(DnsRecord {
                    name: hostname,
                    record_type: policy.record_type.clone(),
                    value,
                    ttl: 300,
                    vm_name: vm.name.clone(),
                })
            })
            .collect()
    }

    /// Resolve all policies and deduplicate records by FQDN.
    pub fn resolve_all(
        &self,
        policies: &[DnsPolicy],
        zones: &[DnsZone],
        all_vms: &[VMSnapshot],
    ) -> Vec<DnsRecord> {
        let zone_map: HashMap<uuid::Uuid, &DnsZone> = zones.iter().map(|z| (z.id, z)).collect();

        let mut seen = HashSet::new();
        let mut records = Vec::new();

        for policy in policies {
            if let Some(zone) = zone_map.get(&policy.zone_id) {
                for record in self.resolve_policy(policy, zone, all_vms) {
                    let key = format!("{}:{:?}", record.name, record.record_type);
                    if seen.insert(key) {
                        records.push(record);
                    }
                }
            }
        }

        records
    }

    /// Expand a record template against VM name and labels.
    ///
    /// Supported placeholders:
    /// - `{name}` → VM name
    /// - `{label:KEY}` → value of the label with the given key
    ///
    /// If a label is not found, the placeholder is replaced with "unknown".
    /// The zone name is automatically appended if not present in the template.
    pub fn expand_template(
        &self,
        template: &str,
        vm_name: &str,
        labels: &HashMap<String, String>,
        zone_name: &str,
    ) -> String {
        let mut result = template.to_string();

        // Replace {name}
        result = result.replace("{name}", vm_name);

        // Replace {label:KEY} patterns
        while let Some(start) = result.find("{label:") {
            if let Some(end) = result[start..].find('}') {
                let key = &result[start + 7..start + end];
                let value = labels
                    .get(key)
                    .cloned()
                    .unwrap_or_else(|| "unknown".to_string());
                result = format!(
                    "{}{}{}",
                    &result[..start],
                    value,
                    &result[start + end + 1..]
                );
            } else {
                break;
            }
        }

        // Append zone name if not already present
        if !result.ends_with(zone_name) {
            if !result.ends_with('.') {
                result.push('.');
            }
            result.push_str(zone_name);
        }

        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use uuid::Uuid;

    fn make_vm(name: &str, labels: &[(&str, &str)], ip: Option<&str>) -> VMSnapshot {
        VMSnapshot {
            name: name.to_string(),
            labels: labels
                .iter()
                .map(|(k, v)| (k.to_string(), v.to_string()))
                .collect(),
            ip: ip.map(|s| s.to_string()),
        }
    }

    fn make_zone(name: &str) -> DnsZone {
        DnsZone {
            id: Uuid::new_v4(),
            name: name.to_string(),
            description: String::new(),
            managed: true,
            created: Utc::now(),
            updated: Utc::now(),
        }
    }

    fn make_policy(
        name: &str,
        zone_id: Uuid,
        selector: &[(&str, &str)],
        template: &str,
        record_type: DnsRecordType,
    ) -> DnsPolicy {
        DnsPolicy {
            id: Uuid::new_v4(),
            name: name.to_string(),
            description: String::new(),
            zone_id,
            selector: LabelSelector {
                match_labels: selector
                    .iter()
                    .map(|(k, v)| (k.to_string(), v.to_string()))
                    .collect(),
            },
            record_template: template.to_string(),
            record_type,
            enabled: true,
            managed: true,
            created: Utc::now(),
            updated: Utc::now(),
        }
    }

    #[test]
    fn test_basic_resolution() {
        let resolver = DnsResolver::new();
        let zone = make_zone("zyvor-fabricd.local");
        let vms = vec![make_vm("web-1", &[("app", "web")], Some("10.0.0.5"))];
        let policy = make_policy(
            "web-dns",
            zone.id,
            &[("app", "web")],
            "{name}.{label:app}",
            DnsRecordType::A,
        );

        let records = resolver.resolve_policy(&policy, &zone, &vms);
        assert_eq!(records.len(), 1);
        assert_eq!(records[0].name, "web-1.web.zyvor-fabricd.local");
        assert_eq!(records[0].value, "10.0.0.5");
        assert_eq!(records[0].vm_name, "web-1");
    }

    #[test]
    fn test_template_expansion() {
        let resolver = DnsResolver::new();
        let mut labels = HashMap::new();
        labels.insert("app".to_string(), "web".to_string());
        labels.insert("env".to_string(), "prod".to_string());

        let result =
            resolver.expand_template("{name}.{label:app}.{label:env}", "vm-1", &labels, "local");
        assert_eq!(result, "vm-1.web.prod.local");
    }

    #[test]
    fn test_no_matches() {
        let resolver = DnsResolver::new();
        let zone = make_zone("zyvor-fabricd.local");
        let vms = vec![make_vm("db-1", &[("app", "db")], Some("10.0.0.20"))];
        let policy = make_policy(
            "web-dns",
            zone.id,
            &[("app", "web")],
            "{name}",
            DnsRecordType::A,
        );

        let records = resolver.resolve_policy(&policy, &zone, &vms);
        assert!(records.is_empty());
    }

    #[test]
    fn test_srv_records() {
        let resolver = DnsResolver::new();
        let zone = make_zone("zyvor-fabricd.local");
        let vms = vec![make_vm("web-1", &[("app", "web")], Some("10.0.0.5"))];
        let policy = make_policy(
            "web-srv",
            zone.id,
            &[("app", "web")],
            "_http._tcp.{label:app}",
            DnsRecordType::Srv,
        );

        let records = resolver.resolve_policy(&policy, &zone, &vms);
        assert_eq!(records.len(), 1);
        assert_eq!(records[0].record_type, DnsRecordType::Srv);
        assert!(records[0].value.starts_with("0 10 0 "));
    }

    #[test]
    fn test_cname() {
        let resolver = DnsResolver::new();
        let zone = make_zone("zyvor-fabricd.local");
        let vms = vec![make_vm("web-1", &[("app", "web")], Some("10.0.0.5"))];
        let policy = make_policy(
            "web-cname",
            zone.id,
            &[("app", "web")],
            "{name}.{label:app}",
            DnsRecordType::Cname,
        );

        let records = resolver.resolve_policy(&policy, &zone, &vms);
        assert_eq!(records.len(), 1);
        assert_eq!(records[0].record_type, DnsRecordType::Cname);
        assert_eq!(records[0].name, records[0].value);
    }

    #[test]
    fn test_dedup() {
        let resolver = DnsResolver::new();
        let zone = make_zone("zyvor-fabricd.local");
        let vms = vec![make_vm("web-1", &[("app", "web")], Some("10.0.0.5"))];

        let policy1 = make_policy(
            "dns-1",
            zone.id,
            &[("app", "web")],
            "{name}.{label:app}",
            DnsRecordType::A,
        );
        let policy2 = make_policy(
            "dns-2",
            zone.id,
            &[("app", "web")],
            "{name}.{label:app}",
            DnsRecordType::A,
        );

        let records = resolver.resolve_all(&[policy1, policy2], &[zone], &vms);
        // Same FQDN + type = deduplicated
        assert_eq!(records.len(), 1);
    }

    #[test]
    fn test_multiple_policies_same_zone() {
        let resolver = DnsResolver::new();
        let zone = make_zone("zyvor-fabricd.local");
        let vms = vec![
            make_vm("web-1", &[("app", "web")], Some("10.0.0.5")),
            make_vm("api-1", &[("app", "api")], Some("10.0.0.10")),
        ];

        let policy1 = make_policy(
            "web-dns",
            zone.id,
            &[("app", "web")],
            "{name}.{label:app}",
            DnsRecordType::A,
        );
        let policy2 = make_policy(
            "api-dns",
            zone.id,
            &[("app", "api")],
            "{name}.{label:app}",
            DnsRecordType::A,
        );

        let records = resolver.resolve_all(&[policy1, policy2], &[zone], &vms);
        assert_eq!(records.len(), 2);
    }

    #[test]
    fn test_disabled_policy() {
        let resolver = DnsResolver::new();
        let zone = make_zone("zyvor-fabricd.local");
        let vms = vec![make_vm("web-1", &[("app", "web")], Some("10.0.0.5"))];
        let mut policy = make_policy(
            "web-dns",
            zone.id,
            &[("app", "web")],
            "{name}",
            DnsRecordType::A,
        );
        policy.enabled = false;

        let records = resolver.resolve_policy(&policy, &zone, &vms);
        assert!(records.is_empty());
    }
}
