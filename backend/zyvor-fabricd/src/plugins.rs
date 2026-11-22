// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: LicenseRef-Zyvor-Production-2.0

use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;

// ============================================================================
// Plugin Trait Definitions
// ============================================================================

/// Core plugin trait that all plugins must implement
pub trait Plugin: Send + Sync {
    fn name(&self) -> &str;
    fn version(&self) -> &str;
    fn description(&self) -> &str;
    fn plugin_type(&self) -> PluginType;
    fn on_load(&self) -> Result<()> {
        Ok(())
    }
    fn on_unload(&self) -> Result<()> {
        Ok(())
    }
}

/// Storage backend plugin trait
pub trait StorageBackendPlugin: Plugin {
    fn create_pool(&self, name: &str, config: &serde_json::Value) -> Result<()>;
    fn delete_pool(&self, name: &str) -> Result<()>;
    fn get_pool_stats(&self, name: &str) -> Result<serde_json::Value>;
    fn storage_type(&self) -> &str;
}

/// VM driver plugin trait for alternative hypervisor backends
pub trait VmDriverPlugin: Plugin {
    fn start_vm(&self, name: &str, config: &serde_json::Value) -> Result<()>;
    fn stop_vm(&self, name: &str) -> Result<()>;
    fn get_vm_state(&self, name: &str) -> Result<String>;
    fn driver_type(&self) -> &str;
}

/// Scheduler plugin trait for custom placement algorithms
pub trait SchedulerPlugin: Plugin {
    fn compute_placement(
        &self,
        vm_requirements: &serde_json::Value,
        available_hosts: &[serde_json::Value],
    ) -> Result<String>; // Returns best host ID
    fn scheduler_type(&self) -> &str;
}

/// Event hook plugin trait for custom event handlers
pub trait EventHookPlugin: Plugin {
    fn on_event(&self, event_type: &str, payload: &serde_json::Value) -> Result<()>;
    fn subscribed_events(&self) -> Vec<String>;
}

// ============================================================================
// Plugin Types and Metadata
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum PluginType {
    StorageBackend,
    VmDriver,
    Scheduler,
    EventHook,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginInfo {
    pub name: String,
    pub version: String,
    pub description: String,
    pub plugin_type: PluginType,
    pub enabled: bool,
    pub loaded: bool,
}

// ============================================================================
// Plugin Registry
// ============================================================================

pub struct PluginRegistry {
    plugins: HashMap<String, Arc<dyn Plugin>>,
    storage_backends: HashMap<String, Arc<dyn StorageBackendPlugin>>,
    vm_drivers: HashMap<String, Arc<dyn VmDriverPlugin>>,
    schedulers: HashMap<String, Arc<dyn SchedulerPlugin>>,
    event_hooks: HashMap<String, Arc<dyn EventHookPlugin>>,
}

impl Default for PluginRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl PluginRegistry {
    pub fn new() -> Self {
        Self {
            plugins: HashMap::new(),
            storage_backends: HashMap::new(),
            vm_drivers: HashMap::new(),
            schedulers: HashMap::new(),
            event_hooks: HashMap::new(),
        }
    }

    /// Register a storage backend plugin
    pub fn register_storage_backend(
        &mut self,
        plugin: Arc<dyn StorageBackendPlugin>,
    ) -> Result<()> {
        let name = plugin.name().to_string();
        plugin.on_load()?;
        self.plugins.insert(name.clone(), plugin.clone());
        self.storage_backends
            .insert(plugin.storage_type().to_string(), plugin);
        tracing::info!("Registered storage backend plugin: {}", name);
        Ok(())
    }

    /// Register a VM driver plugin
    pub fn register_vm_driver(&mut self, plugin: Arc<dyn VmDriverPlugin>) -> Result<()> {
        let name = plugin.name().to_string();
        plugin.on_load()?;
        self.plugins.insert(name.clone(), plugin.clone());
        self.vm_drivers
            .insert(plugin.driver_type().to_string(), plugin);
        tracing::info!("Registered VM driver plugin: {}", name);
        Ok(())
    }

    /// Register a scheduler plugin
    pub fn register_scheduler(&mut self, plugin: Arc<dyn SchedulerPlugin>) -> Result<()> {
        let name = plugin.name().to_string();
        plugin.on_load()?;
        self.plugins.insert(name.clone(), plugin.clone());
        self.schedulers
            .insert(plugin.scheduler_type().to_string(), plugin);
        tracing::info!("Registered scheduler plugin: {}", name);
        Ok(())
    }

    /// Register an event hook plugin
    pub fn register_event_hook(&mut self, plugin: Arc<dyn EventHookPlugin>) -> Result<()> {
        let name = plugin.name().to_string();
        plugin.on_load()?;
        self.plugins.insert(name.clone(), plugin.clone());
        self.event_hooks.insert(name.clone(), plugin);
        tracing::info!("Registered event hook plugin: {}", name);
        Ok(())
    }

    /// Unregister a plugin by name
    pub fn unregister(&mut self, name: &str) -> Result<()> {
        if let Some(plugin) = self.plugins.remove(name) {
            plugin.on_unload()?;
            // Remove from type-specific maps
            self.storage_backends.retain(|_, p| p.name() != name);
            self.vm_drivers.retain(|_, p| p.name() != name);
            self.schedulers.retain(|_, p| p.name() != name);
            self.event_hooks.retain(|_, p| p.name() != name);
            tracing::info!("Unregistered plugin: {}", name);
        }
        Ok(())
    }

    /// Get a storage backend by type
    pub fn get_storage_backend(
        &self,
        storage_type: &str,
    ) -> Option<&Arc<dyn StorageBackendPlugin>> {
        self.storage_backends.get(storage_type)
    }

    /// Get a VM driver by type
    pub fn get_vm_driver(&self, driver_type: &str) -> Option<&Arc<dyn VmDriverPlugin>> {
        self.vm_drivers.get(driver_type)
    }

    /// Get a scheduler by type
    pub fn get_scheduler(&self, scheduler_type: &str) -> Option<&Arc<dyn SchedulerPlugin>> {
        self.schedulers.get(scheduler_type)
    }

    /// List all registered plugins
    pub fn list_plugins(&self) -> Vec<PluginInfo> {
        self.plugins
            .values()
            .map(|p| PluginInfo {
                name: p.name().to_string(),
                version: p.version().to_string(),
                description: p.description().to_string(),
                plugin_type: p.plugin_type(),
                enabled: true,
                loaded: true,
            })
            .collect()
    }

    /// Fire an event to all subscribed hooks
    pub fn fire_event(&self, event_type: &str, payload: &serde_json::Value) {
        for hook in self.event_hooks.values() {
            if hook
                .subscribed_events()
                .iter()
                .any(|e| e == event_type || e == "*")
            {
                if let Err(e) = hook.on_event(event_type, payload) {
                    tracing::error!(
                        "Event hook '{}' failed for event '{}': {}",
                        hook.name(),
                        event_type,
                        e
                    );
                }
            }
        }
    }
}

// ============================================================================
// Plugin API Handlers
// ============================================================================

use axum::{extract::State, Json};
use security::RequireRead;

use crate::server::AppState;

/// GET /api/plugins - List all registered plugins
pub async fn list_plugins(
    RequireRead(_claims): RequireRead,
    State(state): State<Arc<AppState>>,
) -> Json<Vec<PluginInfo>> {
    let registry = state.plugin_registry.read().await;
    Json(registry.list_plugins())
}
