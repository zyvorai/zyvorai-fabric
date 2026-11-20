// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: LicenseRef-Zyvor-Production-1.0

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UsageRecord {
    pub id: String,
    pub tenant_id: String,
    pub vm_name: String,
    pub period_start: DateTime<Utc>,
    pub period_end: DateTime<Utc>,
    pub cpu_hours: f64,
    pub memory_gb_hours: f64,
    pub storage_gb_hours: f64,
    pub network_bytes: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PricingRule {
    pub id: String,
    pub name: String,
    pub cpu_per_hour: f64,
    pub memory_gb_per_hour: f64,
    pub storage_gb_per_hour: f64,
    pub network_per_gb: f64,
    pub currency: String,
}

impl Default for PricingRule {
    fn default() -> Self {
        Self {
            id: "default".into(),
            name: "Default Pricing".into(),
            cpu_per_hour: 0.05,
            memory_gb_per_hour: 0.01,
            storage_gb_per_hour: 0.001,
            network_per_gb: 0.01,
            currency: "USD".into(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Invoice {
    pub id: String,
    pub tenant_id: String,
    pub period_start: DateTime<Utc>,
    pub period_end: DateTime<Utc>,
    pub line_items: Vec<InvoiceLineItem>,
    pub total: f64,
    pub currency: String,
    pub generated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InvoiceLineItem {
    pub description: String,
    pub quantity: f64,
    pub unit_price: f64,
    pub amount: f64,
}

/// Calculate cost from usage and pricing.
pub fn calculate_cost(usage: &UsageRecord, pricing: &PricingRule) -> Invoice {
    let cpu_cost = usage.cpu_hours * pricing.cpu_per_hour;
    let mem_cost = usage.memory_gb_hours * pricing.memory_gb_per_hour;
    let storage_cost = usage.storage_gb_hours * pricing.storage_gb_per_hour;
    let network_cost = (usage.network_bytes as f64 / 1_073_741_824.0) * pricing.network_per_gb;

    let items = vec![
        InvoiceLineItem {
            description: "CPU Hours".into(),
            quantity: usage.cpu_hours,
            unit_price: pricing.cpu_per_hour,
            amount: cpu_cost,
        },
        InvoiceLineItem {
            description: "Memory (GB-Hours)".into(),
            quantity: usage.memory_gb_hours,
            unit_price: pricing.memory_gb_per_hour,
            amount: mem_cost,
        },
        InvoiceLineItem {
            description: "Storage (GB-Hours)".into(),
            quantity: usage.storage_gb_hours,
            unit_price: pricing.storage_gb_per_hour,
            amount: storage_cost,
        },
        InvoiceLineItem {
            description: "Network Transfer (GB)".into(),
            quantity: usage.network_bytes as f64 / 1_073_741_824.0,
            unit_price: pricing.network_per_gb,
            amount: network_cost,
        },
    ];

    let total = cpu_cost + mem_cost + storage_cost + network_cost;

    Invoice {
        id: uuid::Uuid::new_v4().to_string(),
        tenant_id: usage.tenant_id.clone(),
        period_start: usage.period_start,
        period_end: usage.period_end,
        line_items: items,
        total,
        currency: pricing.currency.clone(),
        generated_at: Utc::now(),
    }
}

/// Collect usage from running VMs for a given period.
pub fn collect_vm_usage(
    vm_name: &str,
    tenant_id: &str,
    cpus: u32,
    memory_mb: u64,
    disk_gb: u64,
    hours: f64,
) -> UsageRecord {
    UsageRecord {
        id: uuid::Uuid::new_v4().to_string(),
        tenant_id: tenant_id.to_string(),
        vm_name: vm_name.to_string(),
        period_start: Utc::now() - chrono::Duration::hours(hours as i64),
        period_end: Utc::now(),
        cpu_hours: cpus as f64 * hours,
        memory_gb_hours: (memory_mb as f64 / 1024.0) * hours,
        storage_gb_hours: disk_gb as f64 * hours,
        network_bytes: 0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_calculate_cost() {
        let usage = UsageRecord {
            id: "test".into(),
            tenant_id: "tenant-1".into(),
            vm_name: "vm-1".into(),
            period_start: Utc::now(),
            period_end: Utc::now(),
            cpu_hours: 100.0,
            memory_gb_hours: 200.0,
            storage_gb_hours: 500.0,
            network_bytes: 10_737_418_240, // 10 GB
        };
        let pricing = PricingRule::default();
        let invoice = calculate_cost(&usage, &pricing);

        assert!(invoice.total > 0.0);
        assert_eq!(invoice.line_items.len(), 4);
        assert_eq!(invoice.currency, "USD");
    }

    #[test]
    fn test_calculate_cost_values() {
        let usage = UsageRecord {
            id: "test".into(),
            tenant_id: "tenant-1".into(),
            vm_name: "vm-1".into(),
            period_start: Utc::now(),
            period_end: Utc::now(),
            cpu_hours: 100.0,
            memory_gb_hours: 200.0,
            storage_gb_hours: 500.0,
            network_bytes: 10_737_418_240, // 10 GB
        };
        let pricing = PricingRule::default();
        let invoice = calculate_cost(&usage, &pricing);

        // CPU: 100 * 0.05 = 5.0
        assert!((invoice.line_items[0].amount - 5.0).abs() < 0.001);
        // Memory: 200 * 0.01 = 2.0
        assert!((invoice.line_items[1].amount - 2.0).abs() < 0.001);
        // Storage: 500 * 0.001 = 0.5
        assert!((invoice.line_items[2].amount - 0.5).abs() < 0.001);
        // Network: 10 * 0.01 = 0.1
        assert!((invoice.line_items[3].amount - 0.1).abs() < 0.001);
        // Total: 7.6
        assert!((invoice.total - 7.6).abs() < 0.001);
    }

    #[test]
    fn test_collect_vm_usage() {
        let usage = collect_vm_usage("test-vm", "tenant-1", 4, 8192, 100, 24.0);
        assert_eq!(usage.vm_name, "test-vm");
        assert_eq!(usage.tenant_id, "tenant-1");
        assert!((usage.cpu_hours - 96.0).abs() < 0.001); // 4 CPUs * 24 hours
        assert!((usage.memory_gb_hours - 192.0).abs() < 0.001); // 8 GB * 24 hours
        assert!((usage.storage_gb_hours - 2400.0).abs() < 0.001); // 100 GB * 24 hours
    }

    #[test]
    fn test_default_pricing() {
        let pricing = PricingRule::default();
        assert_eq!(pricing.id, "default");
        assert_eq!(pricing.currency, "USD");
        assert!(pricing.cpu_per_hour > 0.0);
    }
}
