// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: LicenseRef-Zyvor-Production-1.0

use anyhow::Result;
use vm_model::{CreateVMRequest, VM};

impl super::Client {
    /// List all VMs.
    pub async fn list_vms(&self) -> Result<Vec<VM>> {
        let resp: serde_json::Value = self.get("/api/vms").await?;
        let vms: Vec<VM> = serde_json::from_value(
            resp.get("items")
                .cloned()
                .unwrap_or(serde_json::Value::Array(vec![])),
        )?;
        Ok(vms)
    }

    /// Get a VM by name.
    pub async fn get_vm(&self, name: &str) -> Result<VM> {
        self.get(&format!("/api/vms/{}", name)).await
    }

    /// Create a new VM.
    pub async fn create_vm(&self, req: &CreateVMRequest) -> Result<VM> {
        self.post("/api/vms", req).await
    }

    /// Start a VM.
    pub async fn start_vm(&self, name: &str) -> Result<serde_json::Value> {
        self.post(&format!("/api/vms/{}/start", name), &serde_json::json!({}))
            .await
    }

    /// Stop a VM.
    pub async fn stop_vm(&self, name: &str) -> Result<serde_json::Value> {
        self.post(&format!("/api/vms/{}/stop", name), &serde_json::json!({}))
            .await
    }

    /// Delete a VM.
    pub async fn delete_vm(&self, name: &str) -> Result<()> {
        self.delete(&format!("/api/vms/{}", name)).await
    }

    /// Clone a VM.
    pub async fn clone_vm(&self, name: &str, target: &str, linked: bool) -> Result<VM> {
        self.post(
            &format!("/api/vms/{}/clone", name),
            &serde_json::json!({
                "target_name": target,
                "linked_clone": linked,
            }),
        )
        .await
    }
}
