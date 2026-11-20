// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: LicenseRef-Zyvor-Production-1.0

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComplianceProfile {
    pub id: String,
    pub name: String,
    pub description: String,
    pub rules: Vec<ComplianceRule>,
    pub created: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComplianceRule {
    pub id: String,
    pub name: String,
    pub category: RuleCategory,
    pub severity: RuleSeverity,
    pub check_type: CheckType,
    pub expected_value: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum RuleCategory {
    Security,
    Network,
    Storage,
    Compute,
    Configuration,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum RuleSeverity {
    Critical,
    High,
    Medium,
    Low,
    Info,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CheckType {
    DiskEncrypted,
    TpmEnabled,
    SecureBootEnabled,
    FirewallAssigned,
    MinCpus,
    MinMemoryMb,
    NetworkPolicyAssigned,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComplianceScanResult {
    pub id: String,
    pub profile_id: String,
    pub vm_name: String,
    pub scan_time: DateTime<Utc>,
    pub overall_status: ScanStatus,
    pub checks: Vec<CheckResult>,
    pub score: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum ScanStatus {
    Compliant,
    NonCompliant,
    Error,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CheckResult {
    pub rule_id: String,
    pub rule_name: String,
    pub passed: bool,
    pub actual_value: Option<String>,
    pub message: String,
}

/// Scan a VM against a compliance profile.
pub fn scan_vm(vm: &serde_json::Value, profile: &ComplianceProfile) -> ComplianceScanResult {
    let vm_name = vm
        .get("name")
        .and_then(|v| v.as_str())
        .unwrap_or("unknown")
        .to_string();
    let mut checks = Vec::new();
    let mut passed_count = 0;

    for rule in &profile.rules {
        let (passed, actual, message) = evaluate_rule(vm, rule);
        if passed {
            passed_count += 1;
        }
        checks.push(CheckResult {
            rule_id: rule.id.clone(),
            rule_name: rule.name.clone(),
            passed,
            actual_value: actual,
            message,
        });
    }

    let total = checks.len() as f64;
    let score = if total > 0.0 {
        (passed_count as f64 / total) * 100.0
    } else {
        100.0
    };

    ComplianceScanResult {
        id: uuid::Uuid::new_v4().to_string(),
        profile_id: profile.id.clone(),
        vm_name,
        scan_time: Utc::now(),
        overall_status: if score >= 100.0 {
            ScanStatus::Compliant
        } else {
            ScanStatus::NonCompliant
        },
        checks,
        score,
    }
}

fn evaluate_rule(vm: &serde_json::Value, rule: &ComplianceRule) -> (bool, Option<String>, String) {
    match rule.check_type {
        CheckType::DiskEncrypted => {
            let encrypted = vm
                .get("encrypted")
                .and_then(|v| v.as_bool())
                .unwrap_or(false);
            (
                encrypted,
                Some(encrypted.to_string()),
                if encrypted {
                    "Disk is encrypted".into()
                } else {
                    "Disk is NOT encrypted".into()
                },
            )
        }
        CheckType::TpmEnabled => {
            let tpm = vm.get("tpm").and_then(|v| v.as_bool()).unwrap_or(false);
            (
                tpm,
                Some(tpm.to_string()),
                if tpm {
                    "TPM is enabled".into()
                } else {
                    "TPM is NOT enabled".into()
                },
            )
        }
        CheckType::SecureBootEnabled => {
            let sb = vm
                .get("secure_boot")
                .and_then(|v| v.as_bool())
                .unwrap_or(false);
            (
                sb,
                Some(sb.to_string()),
                if sb {
                    "Secure boot enabled".into()
                } else {
                    "Secure boot NOT enabled".into()
                },
            )
        }
        CheckType::MinCpus => {
            let cpus = vm.get("cpus").and_then(|v| v.as_u64()).unwrap_or(0);
            let min = rule
                .expected_value
                .as_deref()
                .and_then(|v| v.parse::<u64>().ok())
                .unwrap_or(1);
            (
                cpus >= min,
                Some(cpus.to_string()),
                format!("CPUs: {} (min: {})", cpus, min),
            )
        }
        CheckType::MinMemoryMb => {
            let mem = vm.get("memory").and_then(|v| v.as_u64()).unwrap_or(0);
            let min = rule
                .expected_value
                .as_deref()
                .and_then(|v| v.parse::<u64>().ok())
                .unwrap_or(256);
            (
                mem >= min,
                Some(mem.to_string()),
                format!("Memory: {}MB (min: {}MB)", mem, min),
            )
        }
        CheckType::FirewallAssigned => {
            let has_fw = vm
                .get("firewall_profile")
                .and_then(|v| v.as_str())
                .is_some();
            (
                has_fw,
                Some(has_fw.to_string()),
                if has_fw {
                    "Firewall profile assigned".into()
                } else {
                    "No firewall profile".into()
                },
            )
        }
        CheckType::NetworkPolicyAssigned => {
            let has_np = vm.get("network_policy").and_then(|v| v.as_str()).is_some();
            (
                has_np,
                Some(has_np.to_string()),
                if has_np {
                    "Network policy assigned".into()
                } else {
                    "No network policy".into()
                },
            )
        }
    }
}

/// Generate a default CIS-like security baseline profile.
pub fn default_security_profile() -> ComplianceProfile {
    ComplianceProfile {
        id: "cis-baseline-v1".to_string(),
        name: "CIS Security Baseline v1".to_string(),
        description: "Basic security compliance checks for VM workloads".to_string(),
        rules: vec![
            ComplianceRule {
                id: "CIS-001".into(),
                name: "Disk Encryption".into(),
                category: RuleCategory::Security,
                severity: RuleSeverity::Critical,
                check_type: CheckType::DiskEncrypted,
                expected_value: None,
            },
            ComplianceRule {
                id: "CIS-002".into(),
                name: "TPM 2.0".into(),
                category: RuleCategory::Security,
                severity: RuleSeverity::High,
                check_type: CheckType::TpmEnabled,
                expected_value: None,
            },
            ComplianceRule {
                id: "CIS-003".into(),
                name: "Secure Boot".into(),
                category: RuleCategory::Security,
                severity: RuleSeverity::High,
                check_type: CheckType::SecureBootEnabled,
                expected_value: None,
            },
            ComplianceRule {
                id: "CIS-004".into(),
                name: "Firewall Profile".into(),
                category: RuleCategory::Network,
                severity: RuleSeverity::Medium,
                check_type: CheckType::FirewallAssigned,
                expected_value: None,
            },
            ComplianceRule {
                id: "CIS-005".into(),
                name: "Network Policy".into(),
                category: RuleCategory::Network,
                severity: RuleSeverity::Medium,
                check_type: CheckType::NetworkPolicyAssigned,
                expected_value: None,
            },
            ComplianceRule {
                id: "CIS-006".into(),
                name: "Minimum CPUs".into(),
                category: RuleCategory::Compute,
                severity: RuleSeverity::Low,
                check_type: CheckType::MinCpus,
                expected_value: Some("1".into()),
            },
            ComplianceRule {
                id: "CIS-007".into(),
                name: "Minimum Memory".into(),
                category: RuleCategory::Compute,
                severity: RuleSeverity::Low,
                check_type: CheckType::MinMemoryMb,
                expected_value: Some("256".into()),
            },
        ],
        created: Utc::now(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_profile() {
        let profile = default_security_profile();
        assert_eq!(profile.id, "cis-baseline-v1");
        assert_eq!(profile.rules.len(), 7);
    }

    #[test]
    fn test_scan_compliant_vm() {
        let vm = serde_json::json!({
            "name": "test-vm",
            "encrypted": true,
            "tpm": true,
            "secure_boot": true,
            "firewall_profile": "default",
            "network_policy": "allow-all",
            "cpus": 4,
            "memory": 2048
        });

        let profile = default_security_profile();
        let result = scan_vm(&vm, &profile);

        assert_eq!(result.vm_name, "test-vm");
        assert!(matches!(result.overall_status, ScanStatus::Compliant));
        assert_eq!(result.score, 100.0);
    }

    #[test]
    fn test_scan_non_compliant_vm() {
        let vm = serde_json::json!({
            "name": "insecure-vm",
            "cpus": 1,
            "memory": 512
        });

        let profile = default_security_profile();
        let result = scan_vm(&vm, &profile);

        assert_eq!(result.vm_name, "insecure-vm");
        assert!(matches!(result.overall_status, ScanStatus::NonCompliant));
        assert!(result.score < 100.0);
    }
}
