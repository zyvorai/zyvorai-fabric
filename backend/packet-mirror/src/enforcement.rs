// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: LicenseRef-Zyvor-Production-2.0

use anyhow::Result;
use tracing;

use crate::models::{CompiledMirrorRule, MirrorDirection, MirrorFilter};

/// Applies traffic mirroring using tc mirred actions.
///
/// tc pattern:
/// ```text
/// tc qdisc add dev <tap> ingress
/// tc filter add dev <tap> parent ffff: protocol ip u32 match u32 0 0 \
///     action mirred egress mirror dev <collector>
/// ```
pub struct MirrorEnforcer;

impl Default for MirrorEnforcer {
    fn default() -> Self {
        Self::new()
    }
}

impl MirrorEnforcer {
    pub fn new() -> Self {
        Self
    }

    /// Apply a single mirror rule.
    pub fn apply_mirror(&self, rule: &CompiledMirrorRule) -> Result<()> {
        // Remove existing qdisc first (ignore errors)
        let _ = run_tc(&["qdisc", "del", "dev", &rule.source_interface, "ingress"]);

        match rule.direction {
            MirrorDirection::Ingress | MirrorDirection::Both => {
                let qdisc_args = self.build_ingress_qdisc_args(&rule.source_interface);
                let str_args: Vec<&str> = qdisc_args.iter().map(|s| s.as_str()).collect();
                run_tc(&str_args)?;

                let mirror_args = self.build_ingress_mirror_args(
                    &rule.source_interface,
                    &rule.collector_target,
                    rule.filter.as_ref(),
                );
                let str_args: Vec<&str> = mirror_args.iter().map(|s| s.as_str()).collect();
                run_tc(&str_args)?;
            }
            MirrorDirection::Egress => {}
        }

        match rule.direction {
            MirrorDirection::Egress | MirrorDirection::Both => {
                let mirror_args = self.build_egress_mirror_args(
                    &rule.source_interface,
                    &rule.collector_target,
                    rule.filter.as_ref(),
                );
                let str_args: Vec<&str> = mirror_args.iter().map(|s| s.as_str()).collect();
                run_tc(&str_args)?;
            }
            MirrorDirection::Ingress => {}
        }

        tracing::debug!(
            "Applied mirror on {} → {} ({:?})",
            rule.source_interface,
            rule.collector_target,
            rule.direction
        );
        Ok(())
    }

    /// Sync all mirror rules.
    pub fn sync_all(&self, rules: &[CompiledMirrorRule]) -> Result<()> {
        for rule in rules {
            self.apply_mirror(rule)?;
        }
        tracing::info!("Synced {} mirror rules", rules.len());
        Ok(())
    }

    /// Remove mirroring from an interface.
    pub fn remove_mirror(&self, iface: &str) -> Result<()> {
        let _ = run_tc(&["qdisc", "del", "dev", iface, "ingress"]);
        tracing::info!("Removed mirror from {}", iface);
        Ok(())
    }

    /// Cleanup all managed mirrors.
    pub fn cleanup(&self, interfaces: &[String]) -> Result<()> {
        for iface in interfaces {
            let _ = run_tc(&["qdisc", "del", "dev", iface, "ingress"]);
        }
        tracing::info!("Cleaned up {} mirror interfaces", interfaces.len());
        Ok(())
    }

    /// Build ingress qdisc arguments.
    pub fn build_ingress_qdisc_args(&self, iface: &str) -> Vec<String> {
        vec![
            "qdisc".to_string(),
            "add".to_string(),
            "dev".to_string(),
            iface.to_string(),
            "ingress".to_string(),
        ]
    }

    /// Build ingress mirror filter arguments.
    pub fn build_ingress_mirror_args(
        &self,
        src: &str,
        dst: &str,
        filter: Option<&MirrorFilter>,
    ) -> Vec<String> {
        let mut args = vec![
            "filter".to_string(),
            "add".to_string(),
            "dev".to_string(),
            src.to_string(),
            "parent".to_string(),
            "ffff:".to_string(),
            "protocol".to_string(),
            "ip".to_string(),
            "u32".to_string(),
        ];

        if let Some(f) = filter {
            if let Some(ref dst_port) = f.dst_port {
                args.push("match".to_string());
                args.push("ip".to_string());
                args.push("dport".to_string());
                args.push(dst_port.to_string());
                args.push("0xffff".to_string());
            } else {
                args.push("match".to_string());
                args.push("u32".to_string());
                args.push("0".to_string());
                args.push("0".to_string());
            }
        } else {
            args.push("match".to_string());
            args.push("u32".to_string());
            args.push("0".to_string());
            args.push("0".to_string());
        }

        args.push("action".to_string());
        args.push("mirred".to_string());
        args.push("egress".to_string());
        args.push("mirror".to_string());
        args.push("dev".to_string());
        args.push(dst.to_string());

        args
    }

    /// Build egress mirror filter arguments.
    pub fn build_egress_mirror_args(
        &self,
        src: &str,
        dst: &str,
        filter: Option<&MirrorFilter>,
    ) -> Vec<String> {
        let mut args = vec![
            "filter".to_string(),
            "add".to_string(),
            "dev".to_string(),
            src.to_string(),
            "parent".to_string(),
            "1:".to_string(),
            "protocol".to_string(),
            "ip".to_string(),
            "u32".to_string(),
        ];

        if let Some(f) = filter {
            if let Some(ref dst_port) = f.dst_port {
                args.push("match".to_string());
                args.push("ip".to_string());
                args.push("dport".to_string());
                args.push(dst_port.to_string());
                args.push("0xffff".to_string());
            } else {
                args.push("match".to_string());
                args.push("u32".to_string());
                args.push("0".to_string());
                args.push("0".to_string());
            }
        } else {
            args.push("match".to_string());
            args.push("u32".to_string());
            args.push("0".to_string());
            args.push("0".to_string());
        }

        args.push("action".to_string());
        args.push("mirred".to_string());
        args.push("egress".to_string());
        args.push("mirror".to_string());
        args.push("dev".to_string());
        args.push(dst.to_string());

        args
    }
}

/// Execute a tc command.
fn run_tc(args: &[&str]) -> Result<()> {
    let output = std::process::Command::new("tc").args(args).output();

    match output {
        Ok(out) if out.status.success() => Ok(()),
        Ok(out) => {
            let stderr = String::from_utf8_lossy(&out.stderr);
            tracing::warn!("tc command failed: {:?} — {}", args, stderr);
            Err(anyhow::anyhow!("tc failed: {}", stderr))
        }
        Err(e) => {
            tracing::warn!("Failed to execute tc: {}", e);
            Err(anyhow::anyhow!("Failed to execute tc: {}", e))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_enforcer() -> MirrorEnforcer {
        MirrorEnforcer::new()
    }

    #[test]
    fn test_ingress_qdisc_args() {
        let enforcer = make_enforcer();
        let args = enforcer.build_ingress_qdisc_args("tap-web-1");
        assert_eq!(args, vec!["qdisc", "add", "dev", "tap-web-1", "ingress"]);
    }

    #[test]
    fn test_ingress_mirror_args() {
        let enforcer = make_enforcer();
        let args = enforcer.build_ingress_mirror_args("tap-web-1", "mon0", None);
        assert!(args.contains(&"filter".to_string()));
        assert!(args.contains(&"tap-web-1".to_string()));
        assert!(args.contains(&"ffff:".to_string()));
        assert!(args.contains(&"mirred".to_string()));
        assert!(args.contains(&"mirror".to_string()));
        assert!(args.contains(&"mon0".to_string()));
    }

    #[test]
    fn test_egress_mirror_args() {
        let enforcer = make_enforcer();
        let args = enforcer.build_egress_mirror_args("tap-web-1", "mon0", None);
        assert!(args.contains(&"filter".to_string()));
        assert!(args.contains(&"tap-web-1".to_string()));
        assert!(args.contains(&"1:".to_string()));
        assert!(args.contains(&"mirred".to_string()));
        assert!(args.contains(&"mirror".to_string()));
        assert!(args.contains(&"mon0".to_string()));
    }

    #[test]
    fn test_mirror_with_filter() {
        let enforcer = make_enforcer();
        let filter = MirrorFilter {
            protocol: Some("tcp".to_string()),
            src_cidr: None,
            dst_cidr: None,
            dst_port: Some(80),
        };

        let args = enforcer.build_ingress_mirror_args("tap-web-1", "mon0", Some(&filter));
        assert!(args.contains(&"dport".to_string()));
        assert!(args.contains(&"80".to_string()));
        assert!(args.contains(&"0xffff".to_string()));
    }

    #[test]
    fn test_cleanup_safe() {
        let enforcer = make_enforcer();
        let result = enforcer.cleanup(&[]);
        assert!(result.is_ok());
    }

    #[test]
    fn test_remote_target() {
        let enforcer = make_enforcer();
        let args = enforcer.build_ingress_mirror_args("tap-web-1", "gre0", None);
        assert!(args.contains(&"gre0".to_string()));
    }
}
