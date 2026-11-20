// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: LicenseRef-Zyvor-Production-1.0

use anyhow::{anyhow, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use uuid::Uuid;

// ---------------------------------------------------------------------------
// Enums
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LibraryType {
    Local,
    Subscribed,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ItemType {
    Template,
    Iso,
    Ovf,
    Ova,
    VmImage,
    Other,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SubscriptionStatus {
    Active,
    Paused,
    Error,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OsType {
    Linux,
    Windows,
}

// ---------------------------------------------------------------------------
// Data Models
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Library {
    pub id: String,
    pub name: String,
    pub description: Option<String>,
    pub library_type: LibraryType,
    pub storage_path: String,
    pub publish_url: Option<String>,
    pub subscription_url: Option<String>,
    pub auto_sync: bool,
    pub sync_interval_hours: u32,
    pub last_sync: Option<DateTime<Utc>>,
    pub item_count: u32,
    pub total_size_bytes: u64,
    pub created: DateTime<Utc>,
    pub updated: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LibraryItem {
    pub id: String,
    pub library_id: String,
    pub name: String,
    pub description: Option<String>,
    pub item_type: ItemType,
    pub version: u32,
    pub versions: Vec<ItemVersion>,
    pub size_bytes: u64,
    pub file_path: String,
    pub checksum: Option<String>,
    pub properties: HashMap<String, String>,
    pub created: DateTime<Utc>,
    pub updated: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ItemVersion {
    pub version: u32,
    pub size_bytes: u64,
    pub file_path: String,
    pub checksum: Option<String>,
    pub created: DateTime<Utc>,
    pub changelog: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Subscription {
    pub id: String,
    pub subscriber_library_id: String,
    pub publisher_library_id: String,
    pub publisher_url: String,
    pub auto_sync: bool,
    pub status: SubscriptionStatus,
    pub last_sync: Option<DateTime<Utc>>,
    pub sync_errors: Vec<String>,
    pub created: DateTime<Utc>,
    pub updated: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OvfPackage {
    pub name: String,
    pub description: Option<String>,
    pub disks: Vec<OvfDisk>,
    pub networks: Vec<OvfNetwork>,
    pub properties: Vec<OvfProperty>,
    pub hardware_requirements: OvfHardware,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OvfDisk {
    pub id: String,
    pub file_ref: String,
    pub capacity_gb: u64,
    pub format: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OvfNetwork {
    pub name: String,
    pub description: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OvfProperty {
    pub key: String,
    pub value_type: String,
    pub default_value: Option<String>,
    pub description: Option<String>,
    pub user_configurable: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OvfHardware {
    pub cpus: u32,
    pub memory_mb: u64,
    pub disk_count: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GuestCustomizationSpec {
    pub id: String,
    pub name: String,
    pub description: Option<String>,
    pub os_type: OsType,
    pub hostname: Option<String>,
    pub domain: Option<String>,
    pub dns_servers: Vec<String>,
    pub network_configs: Vec<GuestNetworkConfig>,
    pub ssh_keys: Vec<String>,
    pub timezone: Option<String>,
    pub run_once_commands: Vec<String>,
    pub created: DateTime<Utc>,
    pub updated: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GuestNetworkConfig {
    pub interface: String,
    pub dhcp: bool,
    pub ip_address: Option<String>,
    pub netmask: Option<String>,
    pub gateway: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HostProfile {
    pub id: String,
    pub name: String,
    pub description: Option<String>,
    pub source_host_id: String,
    pub network_config: serde_json::Value,
    pub storage_config: serde_json::Value,
    pub security_config: serde_json::Value,
    pub kernel_params: HashMap<String, String>,
    pub created: DateTime<Utc>,
    pub updated: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComplianceResult {
    pub host_id: String,
    pub profile_id: String,
    pub compliant: bool,
    pub deviations: Vec<ProfileDeviation>,
    pub checked_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProfileDeviation {
    pub category: String,
    pub setting: String,
    pub expected: String,
    pub actual: String,
}

// ---------------------------------------------------------------------------
// Request types
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateLibraryRequest {
    pub name: String,
    pub description: Option<String>,
    pub library_type: LibraryType,
    pub storage_path: String,
    pub publish_url: Option<String>,
    pub subscription_url: Option<String>,
    pub auto_sync: bool,
    pub sync_interval_hours: Option<u32>,
}

// ---------------------------------------------------------------------------
// ContentLibraryManager
// ---------------------------------------------------------------------------

#[derive(Clone)]
pub struct ContentLibraryManager {
    libraries: Arc<RwLock<HashMap<String, Library>>>,
    items: Arc<RwLock<HashMap<String, LibraryItem>>>,
    subscriptions: Arc<RwLock<HashMap<String, Subscription>>>,
    customization_specs: Arc<RwLock<HashMap<String, GuestCustomizationSpec>>>,
    host_profiles: Arc<RwLock<HashMap<String, HostProfile>>>,
}

impl ContentLibraryManager {
    pub fn new() -> Self {
        Self {
            libraries: Arc::new(RwLock::new(HashMap::new())),
            items: Arc::new(RwLock::new(HashMap::new())),
            subscriptions: Arc::new(RwLock::new(HashMap::new())),
            customization_specs: Arc::new(RwLock::new(HashMap::new())),
            host_profiles: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    // -----------------------------------------------------------------------
    // Libraries
    // -----------------------------------------------------------------------

    pub fn create_library(&self, req: CreateLibraryRequest) -> Result<Library> {
        let now = Utc::now();
        let library = Library {
            id: Uuid::new_v4().to_string(),
            name: req.name,
            description: req.description,
            library_type: req.library_type,
            storage_path: req.storage_path,
            publish_url: req.publish_url,
            subscription_url: req.subscription_url,
            auto_sync: req.auto_sync,
            sync_interval_hours: req.sync_interval_hours.unwrap_or(24),
            last_sync: None,
            item_count: 0,
            total_size_bytes: 0,
            created: now,
            updated: now,
        };

        let mut libs = self
            .libraries
            .write()
            .map_err(|e| anyhow!("Lock poisoned: {}", e))?;
        libs.insert(library.id.clone(), library.clone());
        tracing::info!(library_id = %library.id, name = %library.name, "Created library");
        Ok(library)
    }

    pub fn get_library(&self, id: &str) -> Option<Library> {
        let libs = self.libraries.read().ok()?;
        libs.get(id).cloned()
    }

    pub fn list_libraries(&self) -> Vec<Library> {
        let libs = self.libraries.read().unwrap_or_else(|e| e.into_inner());
        libs.values().cloned().collect()
    }

    pub fn delete_library(&self, id: &str) -> Result<()> {
        let mut libs = self
            .libraries
            .write()
            .map_err(|e| anyhow!("Lock poisoned: {}", e))?;

        if libs.remove(id).is_none() {
            return Err(anyhow!("Library '{}' not found", id));
        }

        // Remove all items belonging to this library
        let mut items = self
            .items
            .write()
            .map_err(|e| anyhow!("Lock poisoned: {}", e))?;
        items.retain(|_, item| item.library_id != id);

        tracing::info!(library_id = %id, "Deleted library");
        Ok(())
    }

    pub fn sync_library(&self, id: &str) -> Result<()> {
        let mut libs = self
            .libraries
            .write()
            .map_err(|e| anyhow!("Lock poisoned: {}", e))?;

        let library = libs
            .get_mut(id)
            .ok_or_else(|| anyhow!("Library '{}' not found", id))?;

        if library.library_type != LibraryType::Subscribed {
            return Err(anyhow!(
                "Library '{}' is not a subscribed library, cannot sync",
                id
            ));
        }

        // In a real implementation this would fetch content from the publisher URL.
        // Here we just update the last_sync timestamp.
        library.last_sync = Some(Utc::now());
        library.updated = Utc::now();
        tracing::info!(library_id = %id, "Synced subscribed library");
        Ok(())
    }

    // -----------------------------------------------------------------------
    // Items
    // -----------------------------------------------------------------------

    pub fn add_item(&self, library_id: &str, mut item: LibraryItem) -> Result<LibraryItem> {
        // Verify library exists
        {
            let libs = self
                .libraries
                .read()
                .map_err(|e| anyhow!("Lock poisoned: {}", e))?;
            if !libs.contains_key(library_id) {
                return Err(anyhow!("Library '{}' not found", library_id));
            }
        }

        item.id = Uuid::new_v4().to_string();
        item.library_id = library_id.to_string();
        let now = Utc::now();
        item.created = now;
        item.updated = now;

        if item.version == 0 {
            item.version = 1;
        }

        // Create initial version entry if versions list is empty
        if item.versions.is_empty() {
            item.versions.push(ItemVersion {
                version: item.version,
                size_bytes: item.size_bytes,
                file_path: item.file_path.clone(),
                checksum: item.checksum.clone(),
                created: now,
                changelog: Some("Initial version".to_string()),
            });
        }

        let mut items = self
            .items
            .write()
            .map_err(|e| anyhow!("Lock poisoned: {}", e))?;
        items.insert(item.id.clone(), item.clone());

        // Update library counters
        let mut libs = self
            .libraries
            .write()
            .map_err(|e| anyhow!("Lock poisoned: {}", e))?;
        if let Some(lib) = libs.get_mut(library_id) {
            lib.item_count += 1;
            lib.total_size_bytes += item.size_bytes;
            lib.updated = Utc::now();
        }

        tracing::info!(item_id = %item.id, name = %item.name, "Added item to library");
        Ok(item)
    }

    pub fn get_item(&self, id: &str) -> Option<LibraryItem> {
        let items = self.items.read().ok()?;
        items.get(id).cloned()
    }

    pub fn list_items(&self, library_id: &str) -> Vec<LibraryItem> {
        let items = self.items.read().unwrap_or_else(|e| e.into_inner());
        items
            .values()
            .filter(|item| item.library_id == library_id)
            .cloned()
            .collect()
    }

    pub fn update_item(
        &self,
        id: &str,
        name: Option<String>,
        description: Option<String>,
    ) -> Result<LibraryItem> {
        let mut items = self
            .items
            .write()
            .map_err(|e| anyhow!("Lock poisoned: {}", e))?;

        let item = items
            .get_mut(id)
            .ok_or_else(|| anyhow!("Item '{}' not found", id))?;

        if let Some(n) = name {
            item.name = n;
        }
        if let Some(d) = description {
            item.description = Some(d);
        }
        item.updated = Utc::now();

        tracing::info!(item_id = %id, "Updated item");
        Ok(item.clone())
    }

    pub fn delete_item(&self, id: &str) -> Result<()> {
        let mut items = self
            .items
            .write()
            .map_err(|e| anyhow!("Lock poisoned: {}", e))?;

        let item = items
            .remove(id)
            .ok_or_else(|| anyhow!("Item '{}' not found", id))?;

        // Update library counters
        let mut libs = self
            .libraries
            .write()
            .map_err(|e| anyhow!("Lock poisoned: {}", e))?;
        if let Some(lib) = libs.get_mut(&item.library_id) {
            lib.item_count = lib.item_count.saturating_sub(1);
            lib.total_size_bytes = lib.total_size_bytes.saturating_sub(item.size_bytes);
            lib.updated = Utc::now();
        }

        tracing::info!(item_id = %id, "Deleted item");
        Ok(())
    }

    pub fn add_item_version(&self, item_id: &str, version: ItemVersion) -> Result<LibraryItem> {
        let mut items = self
            .items
            .write()
            .map_err(|e| anyhow!("Lock poisoned: {}", e))?;

        let item = items
            .get_mut(item_id)
            .ok_or_else(|| anyhow!("Item '{}' not found", item_id))?;

        // Auto-increment the item version number
        item.version += 1;

        let mut new_version = version;
        new_version.version = item.version;

        // Update item fields to reflect latest version
        item.size_bytes = new_version.size_bytes;
        item.file_path = new_version.file_path.clone();
        item.checksum = new_version.checksum.clone();
        item.updated = Utc::now();
        item.versions.push(new_version);

        tracing::info!(
            item_id = %item_id,
            version = item.version,
            "Added new version to item"
        );
        Ok(item.clone())
    }

    pub fn search_items(&self, query: &str) -> Vec<LibraryItem> {
        let items = self.items.read().unwrap_or_else(|e| e.into_inner());
        let query_lower = query.to_lowercase();
        items
            .values()
            .filter(|item| item.name.to_lowercase().contains(&query_lower))
            .cloned()
            .collect()
    }

    // -----------------------------------------------------------------------
    // Subscriptions
    // -----------------------------------------------------------------------

    pub fn create_subscription(&self, mut sub: Subscription) -> Result<Subscription> {
        sub.id = Uuid::new_v4().to_string();
        let now = Utc::now();
        sub.created = now;
        sub.updated = now;

        let mut subs = self
            .subscriptions
            .write()
            .map_err(|e| anyhow!("Lock poisoned: {}", e))?;
        subs.insert(sub.id.clone(), sub.clone());

        tracing::info!(
            subscription_id = %sub.id,
            subscriber = %sub.subscriber_library_id,
            publisher = %sub.publisher_library_id,
            "Created subscription"
        );
        Ok(sub)
    }

    pub fn list_subscriptions(&self, library_id: Option<&str>) -> Vec<Subscription> {
        let subs = self.subscriptions.read().unwrap_or_else(|e| e.into_inner());
        match library_id {
            Some(lid) => subs
                .values()
                .filter(|s| s.subscriber_library_id == lid || s.publisher_library_id == lid)
                .cloned()
                .collect(),
            None => subs.values().cloned().collect(),
        }
    }

    pub fn delete_subscription(&self, id: &str) -> Result<()> {
        let mut subs = self
            .subscriptions
            .write()
            .map_err(|e| anyhow!("Lock poisoned: {}", e))?;
        if subs.remove(id).is_none() {
            return Err(anyhow!("Subscription '{}' not found", id));
        }
        tracing::info!(subscription_id = %id, "Deleted subscription");
        Ok(())
    }

    pub fn pause_subscription(&self, id: &str) -> Result<()> {
        let mut subs = self
            .subscriptions
            .write()
            .map_err(|e| anyhow!("Lock poisoned: {}", e))?;
        let sub = subs
            .get_mut(id)
            .ok_or_else(|| anyhow!("Subscription '{}' not found", id))?;
        sub.status = SubscriptionStatus::Paused;
        sub.updated = Utc::now();
        tracing::info!(subscription_id = %id, "Paused subscription");
        Ok(())
    }

    pub fn resume_subscription(&self, id: &str) -> Result<()> {
        let mut subs = self
            .subscriptions
            .write()
            .map_err(|e| anyhow!("Lock poisoned: {}", e))?;
        let sub = subs
            .get_mut(id)
            .ok_or_else(|| anyhow!("Subscription '{}' not found", id))?;
        sub.status = SubscriptionStatus::Active;
        sub.updated = Utc::now();
        tracing::info!(subscription_id = %id, "Resumed subscription");
        Ok(())
    }

    // -----------------------------------------------------------------------
    // OVF
    // -----------------------------------------------------------------------

    pub fn parse_ovf_metadata(
        &self,
        name: &str,
        properties: HashMap<String, String>,
    ) -> OvfPackage {
        let cpus = properties
            .get("cpus")
            .and_then(|v| v.parse::<u32>().ok())
            .unwrap_or(1);
        let memory_mb = properties
            .get("memory_mb")
            .and_then(|v| v.parse::<u64>().ok())
            .unwrap_or(1024);
        let disk_capacity = properties
            .get("disk_capacity_gb")
            .and_then(|v| v.parse::<u64>().ok())
            .unwrap_or(20);
        let disk_format = properties
            .get("disk_format")
            .cloned()
            .unwrap_or_else(|| "qcow2".to_string());
        let network_name = properties
            .get("network")
            .cloned()
            .unwrap_or_else(|| "default".to_string());

        let mut ovf_properties = Vec::new();
        for (k, v) in &properties {
            if ![
                "cpus",
                "memory_mb",
                "disk_capacity_gb",
                "disk_format",
                "network",
            ]
            .contains(&k.as_str())
            {
                ovf_properties.push(OvfProperty {
                    key: k.clone(),
                    value_type: "string".to_string(),
                    default_value: Some(v.clone()),
                    description: None,
                    user_configurable: true,
                });
            }
        }

        OvfPackage {
            name: name.to_string(),
            description: properties.get("description").cloned(),
            disks: vec![OvfDisk {
                id: "disk-0".to_string(),
                file_ref: format!("{}-disk0.{}", name, disk_format),
                capacity_gb: disk_capacity,
                format: disk_format,
            }],
            networks: vec![OvfNetwork {
                name: network_name,
                description: Some("Primary network".to_string()),
            }],
            properties: ovf_properties,
            hardware_requirements: OvfHardware {
                cpus,
                memory_mb,
                disk_count: 1,
            },
        }
    }

    pub fn import_ovf(&self, library_id: &str, ovf: OvfPackage) -> Result<LibraryItem> {
        let total_size: u64 = ovf
            .disks
            .iter()
            .map(|d| d.capacity_gb * 1024 * 1024 * 1024)
            .sum();

        let mut properties = HashMap::new();
        properties.insert(
            "cpus".to_string(),
            ovf.hardware_requirements.cpus.to_string(),
        );
        properties.insert(
            "memory_mb".to_string(),
            ovf.hardware_requirements.memory_mb.to_string(),
        );
        properties.insert(
            "disk_count".to_string(),
            ovf.hardware_requirements.disk_count.to_string(),
        );
        if let Some(desc) = &ovf.description {
            properties.insert("description".to_string(), desc.clone());
        }
        for prop in &ovf.properties {
            if let Some(val) = &prop.default_value {
                properties.insert(prop.key.clone(), val.clone());
            }
        }

        let item = LibraryItem {
            id: String::new(),
            library_id: library_id.to_string(),
            name: ovf.name.clone(),
            description: ovf.description.clone(),
            item_type: ItemType::Ovf,
            version: 1,
            versions: Vec::new(),
            size_bytes: total_size,
            file_path: format!("/content/{}/{}.ovf", library_id, ovf.name),
            checksum: None,
            properties,
            created: Utc::now(),
            updated: Utc::now(),
        };

        self.add_item(library_id, item)
    }

    /// Download an image from a URL into a library
    pub async fn download_image(
        &self,
        library_id: &str,
        url: &str,
        name: &str,
        item_type: ItemType,
    ) -> Result<LibraryItem> {
        // Get library storage path
        let storage_path = {
            let libs = self
                .libraries
                .read()
                .map_err(|e| anyhow!("Lock poisoned: {}", e))?;
            let lib = libs
                .get(library_id)
                .ok_or_else(|| anyhow!("Library '{}' not found", library_id))?;
            lib.storage_path.clone()
        };

        // Ensure storage directory exists
        std::fs::create_dir_all(&storage_path)?;

        // Download the file
        let url = input_guard::sanitize_outbound_url(url);
        if url.is_empty() {
            anyhow::bail!("refusing to download from an unsafe URL");
        }
        let client = reqwest::Client::builder()
            .redirect(reqwest::redirect::Policy::custom(|attempt| {
                if input_guard::is_safe_outbound_url(attempt.url().as_str()) {
                    attempt.follow()
                } else {
                    attempt.stop()
                }
            }))
            .build()
            .map_err(|e| anyhow!("HTTP client setup failed: {e}"))?;
        let response = client
            .get(url)
            .send()
            .await
            .map_err(|e| anyhow!("Download failed: {}", e))?;

        if !response.status().is_success() {
            return Err(anyhow!(
                "Download failed with status: {}",
                response.status()
            ));
        }

        let content_length = response.content_length().unwrap_or(0);

        // Validate name to prevent path traversal
        if name.contains('/') || name.contains('\\') || name.contains("..") {
            return Err(anyhow!(
                "Item name must not contain path separators or '..'"
            ));
        }

        // Determine file extension from URL
        let extension = url
            .rsplit('/')
            .next()
            .and_then(|f| f.rsplit('.').next())
            .unwrap_or("img");
        // Validate extension too
        if extension.contains('/') || extension.contains('\\') || extension.contains("..") {
            return Err(anyhow!("Invalid file extension"));
        }
        let filename = format!("{}.{}", name, extension);
        let dest_path = format!("{}/{}", storage_path, filename);

        // Write to file
        let bytes = response
            .bytes()
            .await
            .map_err(|e| anyhow!("Failed to read response body: {}", e))?;
        std::fs::write(&dest_path, &bytes)?;

        let actual_size = bytes.len() as u64;

        tracing::info!(
            library_id = %library_id, name = %name, url = %url,
            size = actual_size, "Downloaded image"
        );

        // Create library item
        let item = LibraryItem {
            id: String::new(),
            library_id: library_id.to_string(),
            name: name.to_string(),
            description: Some(format!("Downloaded from {}", url)),
            item_type,
            version: 1,
            versions: Vec::new(),
            size_bytes: if content_length > 0 {
                content_length
            } else {
                actual_size
            },
            file_path: dest_path,
            checksum: None,
            properties: HashMap::new(),
            created: Utc::now(),
            updated: Utc::now(),
        };

        self.add_item(library_id, item)
    }

    /// Import an image from a local file path into a library
    pub fn import_from_path(
        &self,
        library_id: &str,
        source_path: &str,
        name: &str,
    ) -> Result<LibraryItem> {
        let storage_path = {
            let libs = self
                .libraries
                .read()
                .map_err(|e| anyhow!("Lock poisoned: {}", e))?;
            let lib = libs
                .get(library_id)
                .ok_or_else(|| anyhow!("Library '{}' not found", library_id))?;
            lib.storage_path.clone()
        };

        std::fs::create_dir_all(&storage_path)?;

        let source = std::path::Path::new(source_path);
        if !source.exists() {
            return Err(anyhow!("Source path '{}' does not exist", source_path));
        }

        // Validate name to prevent path traversal
        if name.contains('/') || name.contains('\\') || name.contains("..") {
            return Err(anyhow!(
                "Item name must not contain path separators or '..'"
            ));
        }

        let extension = source.extension().and_then(|e| e.to_str()).unwrap_or("img");
        let filename = format!("{}.{}", name, extension);
        let dest_path = format!("{}/{}", storage_path, filename);

        std::fs::copy(source_path, &dest_path)?;

        let metadata = std::fs::metadata(&dest_path)?;

        tracing::info!(
            library_id = %library_id, name = %name, source = %source_path,
            "Imported image from path"
        );

        let item = LibraryItem {
            id: String::new(),
            library_id: library_id.to_string(),
            name: name.to_string(),
            description: Some(format!("Imported from {}", source_path)),
            item_type: ItemType::VmImage,
            version: 1,
            versions: Vec::new(),
            size_bytes: metadata.len(),
            file_path: dest_path,
            checksum: None,
            properties: HashMap::new(),
            created: Utc::now(),
            updated: Utc::now(),
        };

        self.add_item(library_id, item)
    }

    pub fn export_vm_as_ovf(&self, vm_name: &str) -> Result<OvfPackage> {
        // In a real implementation this would inspect a running/stopped VM and
        // produce an OVF descriptor.  Here we return a template package.
        let ovf = OvfPackage {
            name: vm_name.to_string(),
            description: Some(format!("Exported from VM '{}'", vm_name)),
            disks: vec![OvfDisk {
                id: "disk-0".to_string(),
                file_ref: format!("{}-disk0.qcow2", vm_name),
                capacity_gb: 20,
                format: "qcow2".to_string(),
            }],
            networks: vec![OvfNetwork {
                name: "default".to_string(),
                description: Some("Primary network".to_string()),
            }],
            properties: vec![],
            hardware_requirements: OvfHardware {
                cpus: 2,
                memory_mb: 2048,
                disk_count: 1,
            },
        };
        tracing::info!(vm_name = %vm_name, "Exported VM as OVF");
        Ok(ovf)
    }

    // -----------------------------------------------------------------------
    // Guest Customization
    // -----------------------------------------------------------------------

    pub fn create_customization_spec(
        &self,
        mut spec: GuestCustomizationSpec,
    ) -> Result<GuestCustomizationSpec> {
        spec.id = Uuid::new_v4().to_string();
        let now = Utc::now();
        spec.created = now;
        spec.updated = now;

        let mut specs = self
            .customization_specs
            .write()
            .map_err(|e| anyhow!("Lock poisoned: {}", e))?;
        specs.insert(spec.id.clone(), spec.clone());

        tracing::info!(spec_id = %spec.id, name = %spec.name, "Created customization spec");
        Ok(spec)
    }

    pub fn get_customization_spec(&self, id: &str) -> Option<GuestCustomizationSpec> {
        let specs = self.customization_specs.read().ok()?;
        specs.get(id).cloned()
    }

    pub fn list_customization_specs(&self) -> Vec<GuestCustomizationSpec> {
        let specs = self
            .customization_specs
            .read()
            .unwrap_or_else(|e| e.into_inner());
        specs.values().cloned().collect()
    }

    pub fn delete_customization_spec(&self, id: &str) -> Result<()> {
        let mut specs = self
            .customization_specs
            .write()
            .map_err(|e| anyhow!("Lock poisoned: {}", e))?;
        if specs.remove(id).is_none() {
            return Err(anyhow!("Customization spec '{}' not found", id));
        }
        tracing::info!(spec_id = %id, "Deleted customization spec");
        Ok(())
    }

    // -----------------------------------------------------------------------
    // Host Profiles
    // -----------------------------------------------------------------------

    pub fn create_host_profile(&self, mut profile: HostProfile) -> Result<HostProfile> {
        profile.id = Uuid::new_v4().to_string();
        let now = Utc::now();
        profile.created = now;
        profile.updated = now;

        let mut profiles = self
            .host_profiles
            .write()
            .map_err(|e| anyhow!("Lock poisoned: {}", e))?;
        profiles.insert(profile.id.clone(), profile.clone());

        tracing::info!(profile_id = %profile.id, name = %profile.name, "Created host profile");
        Ok(profile)
    }

    pub fn get_host_profile(&self, id: &str) -> Option<HostProfile> {
        let profiles = self.host_profiles.read().ok()?;
        profiles.get(id).cloned()
    }

    pub fn list_host_profiles(&self) -> Vec<HostProfile> {
        let profiles = self.host_profiles.read().unwrap_or_else(|e| e.into_inner());
        profiles.values().cloned().collect()
    }

    pub fn delete_host_profile(&self, id: &str) -> Result<()> {
        let mut profiles = self
            .host_profiles
            .write()
            .map_err(|e| anyhow!("Lock poisoned: {}", e))?;
        if profiles.remove(id).is_none() {
            return Err(anyhow!("Host profile '{}' not found", id));
        }
        tracing::info!(profile_id = %id, "Deleted host profile");
        Ok(())
    }

    pub fn check_host_compliance(
        &self,
        host_id: &str,
        profile_id: &str,
        current_config: &serde_json::Value,
    ) -> ComplianceResult {
        let profiles = self.host_profiles.read().unwrap_or_else(|e| e.into_inner());

        let mut deviations = Vec::new();

        if let Some(profile) = profiles.get(profile_id) {
            // Compare network_config
            if let (Some(expected), Some(actual)) = (
                profile.network_config.as_object(),
                current_config
                    .get("network_config")
                    .and_then(|v| v.as_object()),
            ) {
                for (key, exp_val) in expected {
                    match actual.get(key) {
                        Some(act_val) if act_val != exp_val => {
                            deviations.push(ProfileDeviation {
                                category: "network".to_string(),
                                setting: key.clone(),
                                expected: exp_val.to_string(),
                                actual: act_val.to_string(),
                            });
                        }
                        None => {
                            deviations.push(ProfileDeviation {
                                category: "network".to_string(),
                                setting: key.clone(),
                                expected: exp_val.to_string(),
                                actual: "<missing>".to_string(),
                            });
                        }
                        _ => {}
                    }
                }
            }

            // Compare storage_config
            if let (Some(expected), Some(actual)) = (
                profile.storage_config.as_object(),
                current_config
                    .get("storage_config")
                    .and_then(|v| v.as_object()),
            ) {
                for (key, exp_val) in expected {
                    match actual.get(key) {
                        Some(act_val) if act_val != exp_val => {
                            deviations.push(ProfileDeviation {
                                category: "storage".to_string(),
                                setting: key.clone(),
                                expected: exp_val.to_string(),
                                actual: act_val.to_string(),
                            });
                        }
                        None => {
                            deviations.push(ProfileDeviation {
                                category: "storage".to_string(),
                                setting: key.clone(),
                                expected: exp_val.to_string(),
                                actual: "<missing>".to_string(),
                            });
                        }
                        _ => {}
                    }
                }
            }

            // Compare security_config
            if let (Some(expected), Some(actual)) = (
                profile.security_config.as_object(),
                current_config
                    .get("security_config")
                    .and_then(|v| v.as_object()),
            ) {
                for (key, exp_val) in expected {
                    match actual.get(key) {
                        Some(act_val) if act_val != exp_val => {
                            deviations.push(ProfileDeviation {
                                category: "security".to_string(),
                                setting: key.clone(),
                                expected: exp_val.to_string(),
                                actual: act_val.to_string(),
                            });
                        }
                        None => {
                            deviations.push(ProfileDeviation {
                                category: "security".to_string(),
                                setting: key.clone(),
                                expected: exp_val.to_string(),
                                actual: "<missing>".to_string(),
                            });
                        }
                        _ => {}
                    }
                }
            }
        }

        ComplianceResult {
            host_id: host_id.to_string(),
            profile_id: profile_id.to_string(),
            compliant: deviations.is_empty(),
            deviations,
            checked_at: Utc::now(),
        }
    }
}

impl Default for ContentLibraryManager {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn make_manager() -> ContentLibraryManager {
        ContentLibraryManager::new()
    }

    fn create_local_library(mgr: &ContentLibraryManager) -> Library {
        mgr.create_library(CreateLibraryRequest {
            name: "test-library".to_string(),
            description: Some("A test library".to_string()),
            library_type: LibraryType::Local,
            storage_path: "/var/lib/content".to_string(),
            publish_url: None,
            subscription_url: None,
            auto_sync: false,
            sync_interval_hours: None,
        })
        .unwrap()
    }

    fn create_subscribed_library(mgr: &ContentLibraryManager) -> Library {
        mgr.create_library(CreateLibraryRequest {
            name: "subscribed-library".to_string(),
            description: Some("A subscribed library".to_string()),
            library_type: LibraryType::Subscribed,
            storage_path: "/var/lib/content-sub".to_string(),
            publish_url: None,
            subscription_url: Some("https://publisher.example.com/library".to_string()),
            auto_sync: true,
            sync_interval_hours: Some(12),
        })
        .unwrap()
    }

    fn make_item(library_id: &str) -> LibraryItem {
        LibraryItem {
            id: String::new(),
            library_id: library_id.to_string(),
            name: "ubuntu-22.04".to_string(),
            description: Some("Ubuntu 22.04 LTS template".to_string()),
            item_type: ItemType::Template,
            version: 0,
            versions: Vec::new(),
            size_bytes: 2_000_000_000,
            file_path: "/images/ubuntu-22.04.qcow2".to_string(),
            checksum: Some("abc123sha256".to_string()),
            properties: HashMap::new(),
            created: Utc::now(),
            updated: Utc::now(),
        }
    }

    // -----------------------------------------------------------------------
    // Library CRUD
    // -----------------------------------------------------------------------

    #[test]
    fn test_create_and_get_library() {
        let mgr = make_manager();
        let lib = create_local_library(&mgr);

        assert_eq!(lib.name, "test-library");
        assert_eq!(lib.library_type, LibraryType::Local);
        assert_eq!(lib.sync_interval_hours, 24);
        assert_eq!(lib.item_count, 0);

        let fetched = mgr.get_library(&lib.id).unwrap();
        assert_eq!(fetched.id, lib.id);
        assert_eq!(fetched.name, "test-library");
    }

    #[test]
    fn test_list_and_delete_library() {
        let mgr = make_manager();
        let lib1 = create_local_library(&mgr);
        let _lib2 = create_subscribed_library(&mgr);

        assert_eq!(mgr.list_libraries().len(), 2);

        mgr.delete_library(&lib1.id).unwrap();
        assert_eq!(mgr.list_libraries().len(), 1);
        assert!(mgr.get_library(&lib1.id).is_none());
    }

    #[test]
    fn test_delete_nonexistent_library() {
        let mgr = make_manager();
        let result = mgr.delete_library("does-not-exist");
        assert!(result.is_err());
    }

    #[test]
    fn test_sync_subscribed_library() {
        let mgr = make_manager();
        let lib = create_subscribed_library(&mgr);

        mgr.sync_library(&lib.id).unwrap();

        let updated = mgr.get_library(&lib.id).unwrap();
        assert!(updated.last_sync.is_some());
    }

    #[test]
    fn test_sync_local_library_fails() {
        let mgr = make_manager();
        let lib = create_local_library(&mgr);

        let result = mgr.sync_library(&lib.id);
        assert!(result.is_err());
    }

    // -----------------------------------------------------------------------
    // Item versioning
    // -----------------------------------------------------------------------

    #[test]
    fn test_add_item_and_versioning() {
        let mgr = make_manager();
        let lib = create_local_library(&mgr);
        let item = make_item(&lib.id);

        let added = mgr.add_item(&lib.id, item).unwrap();
        assert_eq!(added.version, 1);
        assert_eq!(added.versions.len(), 1);
        assert_eq!(added.versions[0].version, 1);

        // Add a new version
        let new_version = ItemVersion {
            version: 0, // will be auto-set
            size_bytes: 2_500_000_000,
            file_path: "/images/ubuntu-22.04-v2.qcow2".to_string(),
            checksum: Some("def456sha256".to_string()),
            created: Utc::now(),
            changelog: Some("Updated kernel to 5.15.100".to_string()),
        };

        let versioned = mgr.add_item_version(&added.id, new_version).unwrap();
        assert_eq!(versioned.version, 2);
        assert_eq!(versioned.versions.len(), 2);
        assert_eq!(versioned.size_bytes, 2_500_000_000);
        assert_eq!(versioned.file_path, "/images/ubuntu-22.04-v2.qcow2");
    }

    #[test]
    fn test_update_and_delete_item() {
        let mgr = make_manager();
        let lib = create_local_library(&mgr);
        let item = make_item(&lib.id);
        let added = mgr.add_item(&lib.id, item).unwrap();

        let updated = mgr
            .update_item(
                &added.id,
                Some("ubuntu-22.04-updated".to_string()),
                Some("Updated description".to_string()),
            )
            .unwrap();
        assert_eq!(updated.name, "ubuntu-22.04-updated");
        assert_eq!(updated.description, Some("Updated description".to_string()));

        // Library counter should reflect the item
        let lib_state = mgr.get_library(&lib.id).unwrap();
        assert_eq!(lib_state.item_count, 1);

        mgr.delete_item(&added.id).unwrap();

        assert!(mgr.get_item(&added.id).is_none());
        let lib_state = mgr.get_library(&lib.id).unwrap();
        assert_eq!(lib_state.item_count, 0);
    }

    // -----------------------------------------------------------------------
    // Item search
    // -----------------------------------------------------------------------

    #[test]
    fn test_search_items() {
        let mgr = make_manager();
        let lib = create_local_library(&mgr);

        let mut item1 = make_item(&lib.id);
        item1.name = "ubuntu-22.04-server".to_string();
        mgr.add_item(&lib.id, item1).unwrap();

        let mut item2 = make_item(&lib.id);
        item2.name = "fedora-39-workstation".to_string();
        mgr.add_item(&lib.id, item2).unwrap();

        let mut item3 = make_item(&lib.id);
        item3.name = "ubuntu-24.04-desktop".to_string();
        mgr.add_item(&lib.id, item3).unwrap();

        let results = mgr.search_items("ubuntu");
        assert_eq!(results.len(), 2);

        let results = mgr.search_items("fedora");
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].name, "fedora-39-workstation");

        let results = mgr.search_items("UBUNTU");
        assert_eq!(results.len(), 2); // case-insensitive
    }

    // -----------------------------------------------------------------------
    // Subscription management
    // -----------------------------------------------------------------------

    #[test]
    fn test_subscription_lifecycle() {
        let mgr = make_manager();
        let pub_lib = create_local_library(&mgr);
        let sub_lib = create_subscribed_library(&mgr);

        let sub = Subscription {
            id: String::new(),
            subscriber_library_id: sub_lib.id.clone(),
            publisher_library_id: pub_lib.id.clone(),
            publisher_url: "https://publisher.example.com/library".to_string(),
            auto_sync: true,
            status: SubscriptionStatus::Active,
            last_sync: None,
            sync_errors: Vec::new(),
            created: Utc::now(),
            updated: Utc::now(),
        };

        let created = mgr.create_subscription(sub).unwrap();
        assert_eq!(created.status, SubscriptionStatus::Active);
        assert!(!created.id.is_empty());

        // Pause
        mgr.pause_subscription(&created.id).unwrap();
        let all = mgr.list_subscriptions(None);
        assert_eq!(all.len(), 1);
        assert_eq!(all[0].status, SubscriptionStatus::Paused);

        // Resume
        mgr.resume_subscription(&created.id).unwrap();
        let filtered = mgr.list_subscriptions(Some(&sub_lib.id));
        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered[0].status, SubscriptionStatus::Active);

        // Delete
        mgr.delete_subscription(&created.id).unwrap();
        assert_eq!(mgr.list_subscriptions(None).len(), 0);
    }

    // -----------------------------------------------------------------------
    // OVF parsing
    // -----------------------------------------------------------------------

    #[test]
    fn test_parse_ovf_metadata() {
        let mgr = make_manager();

        let mut props = HashMap::new();
        props.insert("cpus".to_string(), "4".to_string());
        props.insert("memory_mb".to_string(), "8192".to_string());
        props.insert("disk_capacity_gb".to_string(), "100".to_string());
        props.insert("disk_format".to_string(), "vmdk".to_string());
        props.insert("network".to_string(), "production-lan".to_string());
        props.insert("app_version".to_string(), "3.2.1".to_string());

        let ovf = mgr.parse_ovf_metadata("my-appliance", props);

        assert_eq!(ovf.name, "my-appliance");
        assert_eq!(ovf.hardware_requirements.cpus, 4);
        assert_eq!(ovf.hardware_requirements.memory_mb, 8192);
        assert_eq!(ovf.disks.len(), 1);
        assert_eq!(ovf.disks[0].capacity_gb, 100);
        assert_eq!(ovf.disks[0].format, "vmdk");
        assert_eq!(ovf.networks[0].name, "production-lan");
        // Extra properties should be captured
        assert!(ovf.properties.iter().any(|p| p.key == "app_version"));
    }

    #[test]
    fn test_import_and_export_ovf() {
        let mgr = make_manager();
        let lib = create_local_library(&mgr);

        let mut props = HashMap::new();
        props.insert("cpus".to_string(), "2".to_string());
        props.insert("memory_mb".to_string(), "4096".to_string());
        let ovf = mgr.parse_ovf_metadata("web-server", props);

        let item = mgr.import_ovf(&lib.id, ovf).unwrap();
        assert_eq!(item.name, "web-server");
        assert_eq!(item.item_type, ItemType::Ovf);
        assert_eq!(item.properties.get("cpus").unwrap(), "2");

        let exported = mgr.export_vm_as_ovf("test-vm").unwrap();
        assert_eq!(exported.name, "test-vm");
        assert_eq!(exported.hardware_requirements.cpus, 2);
    }

    // -----------------------------------------------------------------------
    // Guest Customization
    // -----------------------------------------------------------------------

    #[test]
    fn test_customization_spec_management() {
        let mgr = make_manager();

        let spec = GuestCustomizationSpec {
            id: String::new(),
            name: "linux-web-server".to_string(),
            description: Some("Standard web server config".to_string()),
            os_type: OsType::Linux,
            hostname: Some("web01".to_string()),
            domain: Some("example.com".to_string()),
            dns_servers: vec!["8.8.8.8".to_string(), "8.8.4.4".to_string()],
            network_configs: vec![GuestNetworkConfig {
                interface: "eth0".to_string(),
                dhcp: false,
                ip_address: Some("10.0.1.100".to_string()),
                netmask: Some("255.255.255.0".to_string()),
                gateway: Some("10.0.1.1".to_string()),
            }],
            ssh_keys: vec!["ssh-ed25519 AAAA... user@host".to_string()],
            timezone: Some("UTC".to_string()),
            run_once_commands: vec![
                "apt-get update".to_string(),
                "apt-get install -y nginx".to_string(),
            ],
            created: Utc::now(),
            updated: Utc::now(),
        };

        let created = mgr.create_customization_spec(spec).unwrap();
        assert!(!created.id.is_empty());
        assert_eq!(created.name, "linux-web-server");
        assert_eq!(created.os_type, OsType::Linux);

        let fetched = mgr.get_customization_spec(&created.id).unwrap();
        assert_eq!(fetched.hostname, Some("web01".to_string()));
        assert_eq!(fetched.dns_servers.len(), 2);
        assert_eq!(
            fetched.network_configs[0].ip_address,
            Some("10.0.1.100".to_string())
        );

        let all = mgr.list_customization_specs();
        assert_eq!(all.len(), 1);

        mgr.delete_customization_spec(&created.id).unwrap();
        assert!(mgr.get_customization_spec(&created.id).is_none());
        assert_eq!(mgr.list_customization_specs().len(), 0);
    }

    // -----------------------------------------------------------------------
    // Host Profile compliance
    // -----------------------------------------------------------------------

    #[test]
    fn test_host_profile_compliance_check() {
        let mgr = make_manager();

        let profile = HostProfile {
            id: String::new(),
            name: "production-baseline".to_string(),
            description: Some("Production host baseline".to_string()),
            source_host_id: "host-001".to_string(),
            network_config: serde_json::json!({
                "mtu": 9000,
                "bonding_mode": "802.3ad"
            }),
            storage_config: serde_json::json!({
                "scheduler": "deadline",
                "read_ahead_kb": 256
            }),
            security_config: serde_json::json!({
                "selinux": "enforcing",
                "firewall": "enabled"
            }),
            kernel_params: HashMap::from([("vm.swappiness".to_string(), "10".to_string())]),
            created: Utc::now(),
            updated: Utc::now(),
        };

        let created = mgr.create_host_profile(profile).unwrap();
        assert!(!created.id.is_empty());

        // Compliant host
        let compliant_config = serde_json::json!({
            "network_config": {
                "mtu": 9000,
                "bonding_mode": "802.3ad"
            },
            "storage_config": {
                "scheduler": "deadline",
                "read_ahead_kb": 256
            },
            "security_config": {
                "selinux": "enforcing",
                "firewall": "enabled"
            }
        });

        let result = mgr.check_host_compliance("host-002", &created.id, &compliant_config);
        assert!(result.compliant);
        assert!(result.deviations.is_empty());
        assert_eq!(result.host_id, "host-002");

        // Non-compliant host
        let deviant_config = serde_json::json!({
            "network_config": {
                "mtu": 1500,
                "bonding_mode": "802.3ad"
            },
            "storage_config": {
                "scheduler": "cfq",
                "read_ahead_kb": 256
            },
            "security_config": {
                "selinux": "permissive",
                "firewall": "enabled"
            }
        });

        let result = mgr.check_host_compliance("host-003", &created.id, &deviant_config);
        assert!(!result.compliant);
        assert_eq!(result.deviations.len(), 3);

        // Verify specific deviations
        let mtu_dev = result
            .deviations
            .iter()
            .find(|d| d.setting == "mtu")
            .unwrap();
        assert_eq!(mtu_dev.category, "network");
        assert_eq!(mtu_dev.expected, "9000");
        assert_eq!(mtu_dev.actual, "1500");

        let sched_dev = result
            .deviations
            .iter()
            .find(|d| d.setting == "scheduler")
            .unwrap();
        assert_eq!(sched_dev.category, "storage");

        let selinux_dev = result
            .deviations
            .iter()
            .find(|d| d.setting == "selinux")
            .unwrap();
        assert_eq!(selinux_dev.category, "security");

        // Profile CRUD
        let all = mgr.list_host_profiles();
        assert_eq!(all.len(), 1);

        let fetched = mgr.get_host_profile(&created.id).unwrap();
        assert_eq!(fetched.name, "production-baseline");

        mgr.delete_host_profile(&created.id).unwrap();
        assert!(mgr.get_host_profile(&created.id).is_none());
    }
}
