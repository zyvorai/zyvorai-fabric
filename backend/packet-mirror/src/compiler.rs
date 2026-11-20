// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: LicenseRef-Zyvor-Production-1.0

use std::collections::HashMap;

use crate::models::*;

/// A snapshot of a VM's state for mirror compilation.
pub struct VMSnapshot {
    pub name: String,
    pub labels: HashMap<String, String>,
    pub tap_interface: Option<String>,
}

/// Resolves mirror sessions to per-VM mirror rules.
pub struct MirrorCompiler;

impl Default for MirrorCompiler {
    fn default() -> Self {
        Self::new()
    }
}

impl MirrorCompiler {
    pub fn new() -> Self {
        Self
    }

    /// Compile a single mirror session against VM state.
    pub fn compile_session(
        &self,
        session: &MirrorSession,
        all_vms: &[VMSnapshot],
    ) -> Vec<CompiledMirrorRule> {
        if !session.enabled {
            return vec![];
        }

        all_vms
            .iter()
            .filter(|vm| session.selector.matches(&vm.labels) && vm.tap_interface.is_some())
            .map(|vm| CompiledMirrorRule {
                source_interface: vm.tap_interface.clone().unwrap(),
                vm_name: vm.name.clone(),
                collector_target: session.collector_target.clone(),
                direction: session.direction.clone(),
                filter: session.filter.clone(),
                session_name: session.name.clone(),
            })
            .collect()
    }

    /// Compile all mirror sessions.
    pub fn compile_all(
        &self,
        sessions: &[MirrorSession],
        all_vms: &[VMSnapshot],
    ) -> Vec<CompiledMirrorRule> {
        let mut rules = Vec::new();
        for session in sessions {
            rules.extend(self.compile_session(session, all_vms));
        }
        rules
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use uuid::Uuid;

    fn make_vm(name: &str, labels: &[(&str, &str)], tap: Option<&str>) -> VMSnapshot {
        VMSnapshot {
            name: name.to_string(),
            labels: labels
                .iter()
                .map(|(k, v)| (k.to_string(), v.to_string()))
                .collect(),
            tap_interface: tap.map(|s| s.to_string()),
        }
    }

    fn make_session(
        name: &str,
        selector: &[(&str, &str)],
        target: &str,
        direction: MirrorDirection,
        filter: Option<MirrorFilter>,
        enabled: bool,
    ) -> MirrorSession {
        MirrorSession {
            id: Uuid::new_v4(),
            name: name.to_string(),
            description: String::new(),
            selector: LabelSelector {
                match_labels: selector
                    .iter()
                    .map(|(k, v)| (k.to_string(), v.to_string()))
                    .collect(),
            },
            collector_type: CollectorType::Interface,
            collector_target: target.to_string(),
            direction,
            filter,
            enabled,
            managed: true,
            created: Utc::now(),
            updated: Utc::now(),
        }
    }

    #[test]
    fn test_basic_compilation() {
        let compiler = MirrorCompiler::new();
        let vms = vec![make_vm("web-1", &[("env", "debug")], Some("tap-web-1"))];
        let session = make_session(
            "debug",
            &[("env", "debug")],
            "mon0",
            MirrorDirection::Both,
            None,
            true,
        );

        let rules = compiler.compile_session(&session, &vms);
        assert_eq!(rules.len(), 1);
        assert_eq!(rules[0].source_interface, "tap-web-1");
        assert_eq!(rules[0].collector_target, "mon0");
        assert_eq!(rules[0].vm_name, "web-1");
    }

    #[test]
    fn test_no_matches() {
        let compiler = MirrorCompiler::new();
        let vms = vec![make_vm("web-1", &[("env", "prod")], Some("tap-web-1"))];
        let session = make_session(
            "debug",
            &[("env", "debug")],
            "mon0",
            MirrorDirection::Both,
            None,
            true,
        );

        let rules = compiler.compile_session(&session, &vms);
        assert!(rules.is_empty());
    }

    #[test]
    fn test_multiple_vms() {
        let compiler = MirrorCompiler::new();
        let vms = vec![
            make_vm("web-1", &[("env", "debug")], Some("tap-web-1")),
            make_vm("web-2", &[("env", "debug")], Some("tap-web-2")),
            make_vm("db-1", &[("env", "prod")], Some("tap-db-1")),
        ];
        let session = make_session(
            "debug",
            &[("env", "debug")],
            "mon0",
            MirrorDirection::Ingress,
            None,
            true,
        );

        let rules = compiler.compile_session(&session, &vms);
        assert_eq!(rules.len(), 2);
    }

    #[test]
    fn test_disabled_session() {
        let compiler = MirrorCompiler::new();
        let vms = vec![make_vm("web-1", &[("env", "debug")], Some("tap-web-1"))];
        let session = make_session(
            "debug",
            &[("env", "debug")],
            "mon0",
            MirrorDirection::Both,
            None,
            false,
        );

        let rules = compiler.compile_session(&session, &vms);
        assert!(rules.is_empty());
    }

    #[test]
    fn test_filter_propagation() {
        let compiler = MirrorCompiler::new();
        let vms = vec![make_vm("web-1", &[("env", "debug")], Some("tap-web-1"))];
        let filter = MirrorFilter {
            protocol: Some("tcp".to_string()),
            src_cidr: None,
            dst_cidr: None,
            dst_port: Some(80),
        };
        let session = make_session(
            "debug",
            &[("env", "debug")],
            "mon0",
            MirrorDirection::Ingress,
            Some(filter),
            true,
        );

        let rules = compiler.compile_session(&session, &vms);
        assert_eq!(rules.len(), 1);
        assert!(rules[0].filter.is_some());
        let f = rules[0].filter.as_ref().unwrap();
        assert_eq!(f.protocol, Some("tcp".to_string()));
        assert_eq!(f.dst_port, Some(80));
    }

    #[test]
    fn test_direction_propagation() {
        let compiler = MirrorCompiler::new();
        let vms = vec![make_vm("web-1", &[("env", "debug")], Some("tap-web-1"))];
        let session = make_session(
            "egress-only",
            &[("env", "debug")],
            "mon0",
            MirrorDirection::Egress,
            None,
            true,
        );

        let rules = compiler.compile_session(&session, &vms);
        assert_eq!(rules.len(), 1);
        assert_eq!(rules[0].direction, MirrorDirection::Egress);
    }

    #[test]
    fn test_compile_all() {
        let compiler = MirrorCompiler::new();
        let vms = vec![
            make_vm("web-1", &[("env", "debug")], Some("tap-web-1")),
            make_vm("db-1", &[("role", "db")], Some("tap-db-1")),
        ];
        let sessions = vec![
            make_session(
                "debug",
                &[("env", "debug")],
                "mon0",
                MirrorDirection::Both,
                None,
                true,
            ),
            make_session(
                "db-mirror",
                &[("role", "db")],
                "mon1",
                MirrorDirection::Ingress,
                None,
                true,
            ),
        ];

        let rules = compiler.compile_all(&sessions, &vms);
        assert_eq!(rules.len(), 2);
    }
}
