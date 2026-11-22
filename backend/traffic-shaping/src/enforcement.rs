// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: LicenseRef-Zyvor-Production-2.0

use anyhow::Result;
use tracing;

use crate::models::CompiledQoSRule;

/// Applies QoS rules using Linux tc (traffic control).
///
/// tc structure:
/// ```text
/// tc qdisc add dev br0 root handle 1: htb default 999
/// tc class add dev br0 parent 1: classid 1:1 htb rate 10gbit
/// tc class add dev br0 parent 1:1 classid 1:256 htb rate 100mbit ceil 500mbit burst 15k prio 1
/// tc filter add dev br0 parent 1: protocol ip u32 match ip src 10.0.0.5/32 flowid 1:256
/// ```
pub struct QoSEnforcer;

impl Default for QoSEnforcer {
    fn default() -> Self {
        Self::new()
    }
}

impl QoSEnforcer {
    pub fn new() -> Self {
        Self
    }

    /// Ensure the root HTB qdisc exists on the given interface.
    pub fn ensure_root_qdisc(&self, interface: &str) -> Result<()> {
        // Delete existing qdisc (ignore errors if none)
        let _ = run_tc(&["qdisc", "del", "dev", interface, "root"]);

        // Create root HTB qdisc
        run_tc(&[
            "qdisc", "add", "dev", interface, "root", "handle", "1:", "htb", "default", "999",
        ])?;

        // Create root class with maximum bandwidth
        run_tc(&[
            "class", "add", "dev", interface, "parent", "1:", "classid", "1:1", "htb", "rate",
            "10gbit",
        ])?;

        // Create default class for unclassified traffic
        run_tc(&[
            "class", "add", "dev", interface, "parent", "1:1", "classid", "1:999", "htb", "rate",
            "1gbit", "ceil", "10gbit",
        ])?;

        tracing::debug!("Root HTB qdisc ensured on {}", interface);
        Ok(())
    }

    /// Sync traffic classes from compiled rules.
    pub fn sync_classes(&self, rules: &[CompiledQoSRule]) -> Result<()> {
        for rule in rules {
            let args = self.build_class_args(rule);
            let str_args: Vec<&str> = args.iter().map(|s| s.as_str()).collect();
            run_tc(&str_args)?;
        }

        tracing::debug!("Synced {} traffic classes", rules.len());
        Ok(())
    }

    /// Sync filters to direct traffic to the correct classes.
    pub fn sync_filters(&self, rules: &[CompiledQoSRule]) -> Result<()> {
        for rule in rules {
            for ip in &rule.vm_ips {
                let args = self.build_filter_args(rule, ip);
                let str_args: Vec<&str> = args.iter().map(|s| s.as_str()).collect();
                run_tc(&str_args)?;
            }
        }

        tracing::debug!("Synced traffic filters");
        Ok(())
    }

    /// Remove the root qdisc from an interface, removing all classes and filters.
    pub fn cleanup(&self, interface: &str) -> Result<()> {
        let _ = run_tc(&["qdisc", "del", "dev", interface, "root"]);
        tracing::info!("Cleaned up tc on {}", interface);
        Ok(())
    }

    /// Full sync: root qdisc → classes → filters.
    pub fn sync_all(&self, rules: &[CompiledQoSRule]) -> Result<()> {
        // Group rules by interface
        let mut by_interface: std::collections::HashMap<String, Vec<&CompiledQoSRule>> =
            std::collections::HashMap::new();
        for rule in rules {
            by_interface
                .entry(rule.interface.clone())
                .or_default()
                .push(rule);
        }

        for (interface, interface_rules) in &by_interface {
            self.ensure_root_qdisc(interface)?;

            for rule in interface_rules {
                let class_args = self.build_class_args(rule);
                let str_args: Vec<&str> = class_args.iter().map(|s| s.as_str()).collect();
                run_tc(&str_args)?;

                for ip in &rule.vm_ips {
                    let filter_args = self.build_filter_args(rule, ip);
                    let str_args: Vec<&str> = filter_args.iter().map(|s| s.as_str()).collect();
                    run_tc(&str_args)?;
                }
            }
        }

        tracing::info!("Synced {} QoS rules", rules.len());
        Ok(())
    }

    /// Build tc class arguments for a compiled rule.
    pub fn build_class_args(&self, rule: &CompiledQoSRule) -> Vec<String> {
        let mut args = vec![
            "class".to_string(),
            "add".to_string(),
            "dev".to_string(),
            rule.interface.clone(),
            "parent".to_string(),
            "1:1".to_string(),
            "classid".to_string(),
            format!("1:{}", rule.class_id),
            "htb".to_string(),
            "rate".to_string(),
            rule.rate.clone(),
            "ceil".to_string(),
            rule.ceil.clone(),
        ];

        if let Some(ref burst) = rule.burst {
            args.push("burst".to_string());
            args.push(burst.clone());
        }

        args.push("prio".to_string());
        args.push(rule.priority.to_string());

        args
    }

    /// Build tc filter arguments to match a VM IP to a class.
    pub fn build_filter_args(&self, rule: &CompiledQoSRule, ip: &str) -> Vec<String> {
        vec![
            "filter".to_string(),
            "add".to_string(),
            "dev".to_string(),
            rule.interface.clone(),
            "parent".to_string(),
            "1:".to_string(),
            "protocol".to_string(),
            "ip".to_string(),
            "u32".to_string(),
            "match".to_string(),
            "ip".to_string(),
            "src".to_string(),
            format!("{}/32", ip),
            "flowid".to_string(),
            format!("1:{}", rule.class_id),
        ]
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

    fn make_enforcer() -> QoSEnforcer {
        QoSEnforcer::new()
    }

    fn make_rule(
        interface: &str,
        class_id: u16,
        rate: &str,
        ceil: &str,
        burst: Option<&str>,
        priority: u8,
        ips: &[&str],
    ) -> CompiledQoSRule {
        CompiledQoSRule {
            interface: interface.to_string(),
            class_id,
            rate: rate.to_string(),
            ceil: ceil.to_string(),
            burst: burst.map(|s| s.to_string()),
            priority,
            vm_ips: ips.iter().map(|s| s.to_string()).collect(),
        }
    }

    #[test]
    fn test_build_class_args_with_burst() {
        let enforcer = make_enforcer();
        let rule = make_rule(
            "br0",
            256,
            "100mbit",
            "500mbit",
            Some("15k"),
            1,
            &["10.0.0.5"],
        );

        let args = enforcer.build_class_args(&rule);
        assert!(args.contains(&"class".to_string()));
        assert!(args.contains(&"dev".to_string()));
        assert!(args.contains(&"br0".to_string()));
        assert!(args.contains(&"1:256".to_string()));
        assert!(args.contains(&"rate".to_string()));
        assert!(args.contains(&"100mbit".to_string()));
        assert!(args.contains(&"ceil".to_string()));
        assert!(args.contains(&"500mbit".to_string()));
        assert!(args.contains(&"burst".to_string()));
        assert!(args.contains(&"15k".to_string()));
        assert!(args.contains(&"prio".to_string()));
        assert!(args.contains(&"1".to_string()));
    }

    #[test]
    fn test_build_class_args_without_burst() {
        let enforcer = make_enforcer();
        let rule = make_rule("br0", 256, "50mbit", "200mbit", None, 4, &["10.0.0.5"]);

        let args = enforcer.build_class_args(&rule);
        assert!(!args.contains(&"burst".to_string()));
        assert!(args.contains(&"50mbit".to_string()));
        assert!(args.contains(&"200mbit".to_string()));
    }

    #[test]
    fn test_build_filter_args() {
        let enforcer = make_enforcer();
        let rule = make_rule("br0", 256, "100mbit", "500mbit", None, 1, &["10.0.0.5"]);

        let args = enforcer.build_filter_args(&rule, "10.0.0.5");
        assert!(args.contains(&"filter".to_string()));
        assert!(args.contains(&"br0".to_string()));
        assert!(args.contains(&"u32".to_string()));
        assert!(args.contains(&"10.0.0.5/32".to_string()));
        assert!(args.contains(&"flowid".to_string()));
        assert!(args.contains(&"1:256".to_string()));
    }

    #[test]
    fn test_default_class() {
        let enforcer = make_enforcer();
        let rule = make_rule("br0", 999, "1gbit", "10gbit", None, 7, &[]);

        let args = enforcer.build_class_args(&rule);
        assert!(args.contains(&"1:999".to_string()));
    }

    #[test]
    fn test_multiple_ips() {
        let enforcer = make_enforcer();
        let rule = make_rule(
            "br0",
            256,
            "100mbit",
            "500mbit",
            None,
            1,
            &["10.0.0.5", "10.0.0.6"],
        );

        let args1 = enforcer.build_filter_args(&rule, "10.0.0.5");
        let args2 = enforcer.build_filter_args(&rule, "10.0.0.6");

        assert!(args1.contains(&"10.0.0.5/32".to_string()));
        assert!(args2.contains(&"10.0.0.6/32".to_string()));
        // Both should use the same class ID
        assert!(args1.contains(&"1:256".to_string()));
        assert!(args2.contains(&"1:256".to_string()));
    }
}
