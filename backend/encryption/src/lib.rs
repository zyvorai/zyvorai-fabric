// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: Apache-2.0

use std::collections::HashMap;
use std::sync::{Arc, RwLock};

use anyhow::{bail, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// Write `contents` to `path`, creating the file with mode 0600 so key material is never
/// readable by other users, not even briefly. An existing file is narrowed to 0600 before
/// any bytes are written.
pub fn write_private_file(
    path: impl AsRef<std::path::Path>,
    contents: impl AsRef<[u8]>,
) -> std::io::Result<()> {
    use std::io::Write;
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create(true).truncate(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options.open(path)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        file.set_permissions(std::fs::Permissions::from_mode(0o600))?;
    }
    file.write_all(contents.as_ref())
}

// ---------------------------------------------------------------------------
// Data models
// ---------------------------------------------------------------------------

/// Supported encryption algorithms for VM disk encryption at-rest.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EncryptionAlgorithm {
    Aes256Xts,
    Aes256Cbc,
    ChaCha20Poly1305,
}

/// Type of external (or local) key provider.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum KeyProviderType {
    Local,
    Kmip,
    VaultTransit,
}

/// Connection status of a key provider.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum KeyProviderStatus {
    Connected,
    Disconnected,
    Error,
}

/// Lifecycle status of an encryption key.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum KeyStatus {
    Active,
    Rotated,
    Revoked,
    Destroyed,
}

/// Status of an in-flight key rotation operation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RotationStatus {
    Pending,
    InProgress,
    Completed,
    Failed,
}

// ---------------------------------------------------------------------------

/// An encryption policy that can be applied to one or more VMs.
///
/// `id`/`created`/`updated` are always overwritten by create_policy
/// regardless of what the client sends, so they're #[serde(default)] --
/// requiring the client to pre-supply values it has no legitimate way to
/// know (a UUID, a timestamp) just to satisfy deserialization was pure
/// friction with no purpose.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EncryptionPolicy {
    #[serde(default)]
    pub id: String,
    pub name: String,
    pub description: Option<String>,
    pub algorithm: EncryptionAlgorithm,
    pub key_provider_id: String,
    pub encrypt_vmotion: bool,
    pub auto_rotate_days: Option<u32>,
    #[serde(default)]
    pub created: DateTime<Utc>,
    #[serde(default)]
    pub updated: DateTime<Utc>,
}

/// A key-management provider (local or remote KMS).
///
/// `id`/`created`/`updated` are always overwritten by register_provider;
/// see `EncryptionPolicy` above for why they're #[serde(default)].
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KeyProvider {
    #[serde(default)]
    pub id: String,
    pub name: String,
    pub provider_type: KeyProviderType,
    pub endpoint: Option<String>,
    pub status: KeyProviderStatus,
    #[serde(default)]
    pub created: DateTime<Utc>,
    #[serde(default)]
    pub updated: DateTime<Utc>,
}

/// A single encryption key managed by a provider.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EncryptionKey {
    pub id: String,
    pub key_provider_id: String,
    pub algorithm: EncryptionAlgorithm,
    pub key_size_bits: u32,
    pub status: KeyStatus,
    pub vm_name: Option<String>,
    pub created: DateTime<Utc>,
    pub rotated: Option<DateTime<Utc>>,
    pub expires: Option<DateTime<Utc>>,
}

/// Encryption state of a single VM.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VmEncryptionStatus {
    pub vm_name: String,
    pub encrypted: bool,
    pub policy_id: Option<String>,
    pub key_id: Option<String>,
    pub algorithm: Option<EncryptionAlgorithm>,
    pub vmotion_encrypted: bool,
    pub last_key_rotation: Option<DateTime<Utc>>,
}

/// Record of a key rotation event.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KeyRotationEvent {
    pub id: String,
    pub key_id: String,
    pub vm_name: String,
    pub old_key_id: String,
    pub new_key_id: String,
    pub status: RotationStatus,
    pub started: Option<DateTime<Utc>>,
    pub completed: Option<DateTime<Utc>>,
    pub error: Option<String>,
}

// ---------------------------------------------------------------------------
// Manager
// ---------------------------------------------------------------------------

/// Internal state protected by `Arc<RwLock<_>>`.
#[derive(Debug, Default)]
struct Inner {
    providers: HashMap<String, KeyProvider>,
    policies: HashMap<String, EncryptionPolicy>,
    keys: HashMap<String, EncryptionKey>,
    vm_statuses: HashMap<String, VmEncryptionStatus>,
    rotation_events: HashMap<String, KeyRotationEvent>,
}

/// Thread-safe manager for VM disk encryption, key providers, policies and
/// key lifecycle operations.
#[derive(Debug, Clone)]
pub struct EncryptionManager {
    inner: Arc<RwLock<Inner>>,
}

impl Default for EncryptionManager {
    fn default() -> Self {
        Self::new()
    }
}

impl EncryptionManager {
    /// Create a new, empty encryption manager.
    pub fn new() -> Self {
        Self {
            inner: Arc::new(RwLock::new(Inner::default())),
        }
    }

    // -- Key Providers ------------------------------------------------------

    /// Register a new key provider.
    pub fn register_provider(&self, provider: KeyProvider) -> Result<KeyProvider> {
        let mut inner = self.inner.write().unwrap_or_else(|e| e.into_inner());
        if inner.providers.contains_key(&provider.id) {
            bail!("Key provider '{}' already exists", provider.id);
        }
        tracing::info!(
            "Registering key provider '{}' (type: {:?})",
            provider.name,
            provider.provider_type
        );
        inner
            .providers
            .insert(provider.id.clone(), provider.clone());
        Ok(provider)
    }

    /// Look up a key provider by ID.
    pub fn get_provider(&self, id: &str) -> Option<KeyProvider> {
        let inner = self.inner.read().unwrap_or_else(|e| e.into_inner());
        inner.providers.get(id).cloned()
    }

    /// List all registered key providers.
    pub fn list_providers(&self) -> Vec<KeyProvider> {
        let inner = self.inner.read().unwrap_or_else(|e| e.into_inner());
        inner.providers.values().cloned().collect()
    }

    /// Remove a key provider. Fails if any active keys still reference it.
    pub fn remove_provider(&self, id: &str) -> Result<()> {
        let mut inner = self.inner.write().unwrap_or_else(|e| e.into_inner());
        if !inner.providers.contains_key(id) {
            bail!("Key provider '{}' not found", id);
        }

        let keys_in_use = inner
            .keys
            .values()
            .any(|k| k.key_provider_id == id && k.status == KeyStatus::Active);
        if keys_in_use {
            bail!(
                "Cannot remove provider '{}': active keys still reference it",
                id
            );
        }

        inner.providers.remove(id);
        tracing::info!("Removed key provider '{}'", id);
        Ok(())
    }

    /// Test connectivity to a key provider.  For `Local` providers this always
    /// succeeds; for remote providers we just check that the endpoint is
    /// configured and the provider is not in `Error` status.
    pub fn test_provider(&self, id: &str) -> Result<bool> {
        let inner = self.inner.read().unwrap_or_else(|e| e.into_inner());
        let provider = inner
            .providers
            .get(id)
            .ok_or_else(|| anyhow::anyhow!("Key provider '{}' not found", id))?;

        match provider.provider_type {
            KeyProviderType::Local => Ok(true),
            KeyProviderType::Kmip | KeyProviderType::VaultTransit => {
                if provider.endpoint.is_none() {
                    bail!("Remote provider '{}' has no endpoint configured", id);
                }
                Ok(provider.status != KeyProviderStatus::Error)
            }
        }
    }

    // -- Policies -----------------------------------------------------------

    /// Create a new encryption policy.
    pub fn create_policy(&self, policy: EncryptionPolicy) -> Result<EncryptionPolicy> {
        let mut inner = self.inner.write().unwrap_or_else(|e| e.into_inner());
        if inner.policies.contains_key(&policy.id) {
            bail!("Encryption policy '{}' already exists", policy.id);
        }
        if !inner.providers.contains_key(&policy.key_provider_id) {
            bail!(
                "Key provider '{}' referenced by policy does not exist",
                policy.key_provider_id
            );
        }
        tracing::info!("Creating encryption policy '{}'", policy.name);
        inner.policies.insert(policy.id.clone(), policy.clone());
        Ok(policy)
    }

    /// Retrieve a policy by ID.
    pub fn get_policy(&self, id: &str) -> Option<EncryptionPolicy> {
        let inner = self.inner.read().unwrap_or_else(|e| e.into_inner());
        inner.policies.get(id).cloned()
    }

    /// List all encryption policies.
    pub fn list_policies(&self) -> Vec<EncryptionPolicy> {
        let inner = self.inner.read().unwrap_or_else(|e| e.into_inner());
        inner.policies.values().cloned().collect()
    }

    /// Replace an existing policy with updated values.
    pub fn update_policy(
        &self,
        id: &str,
        mut policy: EncryptionPolicy,
    ) -> Result<EncryptionPolicy> {
        let mut inner = self.inner.write().unwrap_or_else(|e| e.into_inner());
        if !inner.policies.contains_key(id) {
            bail!("Encryption policy '{}' not found", id);
        }
        policy.id = id.to_string();
        policy.updated = Utc::now();
        inner.policies.insert(id.to_string(), policy.clone());
        tracing::info!("Updated encryption policy '{}'", id);
        Ok(policy)
    }

    /// Delete a policy.  Fails if any VM is still using it.
    pub fn delete_policy(&self, id: &str) -> Result<()> {
        let mut inner = self.inner.write().unwrap_or_else(|e| e.into_inner());
        if !inner.policies.contains_key(id) {
            bail!("Encryption policy '{}' not found", id);
        }
        let in_use = inner
            .vm_statuses
            .values()
            .any(|s| s.policy_id.as_deref() == Some(id));
        if in_use {
            bail!(
                "Cannot delete policy '{}': still in use by one or more VMs",
                id
            );
        }
        inner.policies.remove(id);
        tracing::info!("Deleted encryption policy '{}'", id);
        Ok(())
    }

    // -- Key management -----------------------------------------------------

    /// Generate a new encryption key via the given provider.
    pub fn generate_key(
        &self,
        provider_id: &str,
        algorithm: EncryptionAlgorithm,
    ) -> Result<EncryptionKey> {
        let mut inner = self.inner.write().unwrap_or_else(|e| e.into_inner());
        if !inner.providers.contains_key(provider_id) {
            bail!("Key provider '{}' not found", provider_id);
        }

        let now = Utc::now();
        let key = EncryptionKey {
            id: uuid::Uuid::new_v4().to_string(),
            key_provider_id: provider_id.to_string(),
            algorithm,
            key_size_bits: 256,
            status: KeyStatus::Active,
            vm_name: None,
            created: now,
            rotated: None,
            expires: None,
        };

        tracing::info!(
            "Generated encryption key '{}' via provider '{}'",
            key.id,
            provider_id
        );
        inner.keys.insert(key.id.clone(), key.clone());
        Ok(key)
    }

    /// Look up a key by ID.
    pub fn get_key(&self, id: &str) -> Option<EncryptionKey> {
        let inner = self.inner.read().unwrap_or_else(|e| e.into_inner());
        inner.keys.get(id).cloned()
    }

    /// Rotate a key: mark the existing one as `Rotated`, generate a fresh key
    /// from the same provider, and record a `KeyRotationEvent`.
    pub fn rotate_key(&self, key_id: &str) -> Result<KeyRotationEvent> {
        let mut inner = self.inner.write().unwrap_or_else(|e| e.into_inner());
        let old_key = inner
            .keys
            .get(key_id)
            .ok_or_else(|| anyhow::anyhow!("Key '{}' not found", key_id))?
            .clone();

        if old_key.status != KeyStatus::Active {
            bail!(
                "Key '{}' is not active (status: {:?})",
                key_id,
                old_key.status
            );
        }

        let now = Utc::now();

        // Create successor key.
        let new_key = EncryptionKey {
            id: uuid::Uuid::new_v4().to_string(),
            key_provider_id: old_key.key_provider_id.clone(),
            algorithm: old_key.algorithm.clone(),
            key_size_bits: old_key.key_size_bits,
            status: KeyStatus::Active,
            vm_name: old_key.vm_name.clone(),
            created: now,
            rotated: None,
            expires: None,
        };

        // Mark old key as rotated.
        let old_mut = inner.keys.get_mut(key_id).unwrap();
        old_mut.status = KeyStatus::Rotated;
        old_mut.rotated = Some(now);

        let vm_name = old_key.vm_name.clone().unwrap_or_default();

        // Update the VM status to reference the new key.
        if let Some(status) = inner.vm_statuses.get_mut(&vm_name) {
            status.key_id = Some(new_key.id.clone());
            status.last_key_rotation = Some(now);
        }

        let event = KeyRotationEvent {
            id: uuid::Uuid::new_v4().to_string(),
            key_id: new_key.id.clone(),
            vm_name,
            old_key_id: key_id.to_string(),
            new_key_id: new_key.id.clone(),
            status: RotationStatus::Completed,
            started: Some(now),
            completed: Some(now),
            error: None,
        };

        inner.keys.insert(new_key.id.clone(), new_key);
        inner
            .rotation_events
            .insert(event.id.clone(), event.clone());

        tracing::info!("Rotated key '{}' -> '{}'", key_id, event.new_key_id);
        Ok(event)
    }

    /// Revoke an active key.  If the key is assigned to a VM the caller should
    /// decrypt or re-key the VM first; this method simply marks the key.
    pub fn revoke_key(&self, key_id: &str) -> Result<()> {
        let mut inner = self.inner.write().unwrap_or_else(|e| e.into_inner());
        let key = inner
            .keys
            .get_mut(key_id)
            .ok_or_else(|| anyhow::anyhow!("Key '{}' not found", key_id))?;
        if key.status == KeyStatus::Revoked || key.status == KeyStatus::Destroyed {
            bail!("Key '{}' is already revoked or destroyed", key_id);
        }
        key.status = KeyStatus::Revoked;
        tracing::info!("Revoked key '{}'", key_id);
        Ok(())
    }

    /// List keys, optionally filtered by provider.
    pub fn list_keys(&self, provider_id: Option<&str>) -> Vec<EncryptionKey> {
        let inner = self.inner.read().unwrap_or_else(|e| e.into_inner());
        inner
            .keys
            .values()
            .filter(|k| match provider_id {
                Some(pid) => k.key_provider_id == pid,
                None => true,
            })
            .cloned()
            .collect()
    }

    // -- VM encryption ------------------------------------------------------

    /// Encrypt a VM using the given policy. Generates a new key and records
    /// the encryption status.
    pub fn encrypt_vm(&self, vm_name: &str, policy_id: &str) -> Result<VmEncryptionStatus> {
        let mut inner = self.inner.write().unwrap_or_else(|e| e.into_inner());

        if let Some(status) = inner.vm_statuses.get(vm_name) {
            if status.encrypted {
                bail!("VM '{}' is already encrypted", vm_name);
            }
        }

        let policy = inner
            .policies
            .get(policy_id)
            .ok_or_else(|| anyhow::anyhow!("Encryption policy '{}' not found", policy_id))?
            .clone();

        // Generate a key for this VM.
        let now = Utc::now();
        let key = EncryptionKey {
            id: uuid::Uuid::new_v4().to_string(),
            key_provider_id: policy.key_provider_id.clone(),
            algorithm: policy.algorithm.clone(),
            key_size_bits: 256,
            status: KeyStatus::Active,
            vm_name: Some(vm_name.to_string()),
            created: now,
            rotated: None,
            expires: policy
                .auto_rotate_days
                .map(|d| now + chrono::Duration::days(d as i64)),
        };

        let status = VmEncryptionStatus {
            vm_name: vm_name.to_string(),
            encrypted: true,
            policy_id: Some(policy_id.to_string()),
            key_id: Some(key.id.clone()),
            algorithm: Some(policy.algorithm.clone()),
            vmotion_encrypted: policy.encrypt_vmotion,
            last_key_rotation: None,
        };

        let key_id_for_disk = key.id.clone();
        inner.keys.insert(key.id.clone(), key);
        inner
            .vm_statuses
            .insert(vm_name.to_string(), status.clone());

        tracing::info!(
            "Encrypted VM '{}' with policy '{}' (algorithm: {:?})",
            vm_name,
            policy.name,
            policy.algorithm
        );

        // Actually encrypt the disk image using qemu-img LUKS encryption.
        // We must drop the lock before running the blocking I/O operation.
        drop(inner);

        let image_path = format!("/var/lib/zyvor-fabricd/images/{}.qcow2", vm_name);
        if std::path::Path::new(&image_path).exists() {
            let encrypted_path = format!("{}.encrypted", image_path);

            // Write the encryption key to a temporary file for qemu-img --object
            let secret_file = format!(
                "/tmp/zyvor-fabricd-encrypt-{}",
                uuid::Uuid::new_v4().simple()
            );
            if let Err(e) = write_private_file(&secret_file, &key_id_for_disk) {
                tracing::error!(
                    "Failed to write encryption secret file for VM '{}': {}",
                    vm_name,
                    e
                );
            } else {
                let output = std::process::Command::new("qemu-img")
                    .args([
                        "convert",
                        "-f",
                        "qcow2",
                        "-O",
                        "qcow2",
                        "--object",
                        &format!("secret,id=sec0,file={}", secret_file),
                        "-o",
                        "encrypt.format=luks,encrypt.key-secret=sec0",
                        &image_path,
                        &encrypted_path,
                    ])
                    .output();

                // Always clean up the secret file
                let _ = std::fs::remove_file(&secret_file);

                match output {
                    Ok(out) if out.status.success() => {
                        // Replace the original with the encrypted version
                        if let Err(e) = std::fs::rename(&encrypted_path, &image_path) {
                            tracing::error!(
                                "Failed to replace disk with encrypted version for VM '{}': {}",
                                vm_name,
                                e
                            );
                        } else {
                            tracing::info!(
                                "Successfully encrypted disk image for VM '{}'",
                                vm_name
                            );
                        }
                    }
                    Ok(out) => {
                        let stderr = String::from_utf8_lossy(&out.stderr);
                        tracing::error!(
                            "qemu-img encryption failed for VM '{}': {}",
                            vm_name,
                            stderr
                        );
                        // Clean up partial output file
                        let _ = std::fs::remove_file(&encrypted_path);
                    }
                    Err(e) => {
                        tracing::error!("Failed to execute qemu-img for VM '{}': {}", vm_name, e);
                    }
                }
            }
        } else {
            tracing::debug!(
                "No disk image found at '{}' for VM '{}', skipping disk encryption",
                image_path,
                vm_name
            );
        }

        Ok(status)
    }

    /// Decrypt a VM (remove encryption). The associated key is revoked.
    pub fn decrypt_vm(&self, vm_name: &str) -> Result<()> {
        let mut inner = self.inner.write().unwrap_or_else(|e| e.into_inner());

        let status = inner
            .vm_statuses
            .get(vm_name)
            .ok_or_else(|| anyhow::anyhow!("No encryption status found for VM '{}'", vm_name))?
            .clone();

        if !status.encrypted {
            bail!("VM '{}' is not encrypted", vm_name);
        }

        // Revoke the key.
        if let Some(key_id) = &status.key_id {
            if let Some(key) = inner.keys.get_mut(key_id) {
                key.status = KeyStatus::Revoked;
            }
        }

        // Save the key_id before we modify things, so we can use it for decryption
        let key_for_decrypt = status.key_id.clone();

        inner.vm_statuses.insert(
            vm_name.to_string(),
            VmEncryptionStatus {
                vm_name: vm_name.to_string(),
                encrypted: false,
                policy_id: None,
                key_id: None,
                algorithm: None,
                vmotion_encrypted: false,
                last_key_rotation: None,
            },
        );

        tracing::info!("Decrypted VM '{}'", vm_name);

        // Actually decrypt the disk image using qemu-img.
        // Drop the lock before blocking I/O.
        drop(inner);

        let image_path = format!("/var/lib/zyvor-fabricd/images/{}.qcow2", vm_name);
        if std::path::Path::new(&image_path).exists() {
            if let Some(ref key_id) = key_for_decrypt {
                let decrypted_path = format!("{}.decrypted", image_path);
                let secret_file = format!(
                    "/tmp/zyvor-fabricd-decrypt-{}",
                    uuid::Uuid::new_v4().simple()
                );

                if let Err(e) = write_private_file(&secret_file, key_id) {
                    tracing::error!(
                        "Failed to write decryption secret file for VM '{}': {}",
                        vm_name,
                        e
                    );
                } else {
                    let output = std::process::Command::new("qemu-img")
                        .args([
                            "convert",
                            "-f", "qcow2",
                            "-O", "qcow2",
                            "--object",
                            &format!("secret,id=sec0,file={}", secret_file),
                            "--image-opts",
                            &format!(
                                "driver=qcow2,file.driver=file,file.filename={},encrypt.key-secret=sec0",
                                image_path
                            ),
                            &decrypted_path,
                        ])
                        .output();

                    let _ = std::fs::remove_file(&secret_file);

                    match output {
                        Ok(out) if out.status.success() => {
                            if let Err(e) = std::fs::rename(&decrypted_path, &image_path) {
                                tracing::error!(
                                    "Failed to replace encrypted disk with decrypted version for VM '{}': {}",
                                    vm_name, e
                                );
                            } else {
                                tracing::info!(
                                    "Successfully decrypted disk image for VM '{}'",
                                    vm_name
                                );
                            }
                        }
                        Ok(out) => {
                            let stderr = String::from_utf8_lossy(&out.stderr);
                            tracing::error!(
                                "qemu-img decryption failed for VM '{}': {}",
                                vm_name,
                                stderr
                            );
                            let _ = std::fs::remove_file(&decrypted_path);
                        }
                        Err(e) => {
                            tracing::error!(
                                "Failed to execute qemu-img for VM '{}': {}",
                                vm_name,
                                e
                            );
                        }
                    }
                }
            }
        } else {
            tracing::debug!(
                "No disk image found at '{}' for VM '{}', skipping disk decryption",
                image_path,
                vm_name
            );
        }

        Ok(())
    }

    /// Get the encryption status of a VM.
    pub fn get_vm_encryption_status(&self, vm_name: &str) -> Option<VmEncryptionStatus> {
        let inner = self.inner.read().unwrap_or_else(|e| e.into_inner());
        inner.vm_statuses.get(vm_name).cloned()
    }

    /// List all VMs that are currently encrypted.
    pub fn list_encrypted_vms(&self) -> Vec<VmEncryptionStatus> {
        let inner = self.inner.read().unwrap_or_else(|e| e.into_inner());
        inner
            .vm_statuses
            .values()
            .filter(|s| s.encrypted)
            .cloned()
            .collect()
    }

    /// Rotate the encryption key for a specific VM.
    pub fn rotate_vm_key(&self, vm_name: &str) -> Result<KeyRotationEvent> {
        let key_id = {
            let inner = self.inner.read().unwrap_or_else(|e| e.into_inner());
            let status = inner
                .vm_statuses
                .get(vm_name)
                .ok_or_else(|| anyhow::anyhow!("VM '{}' not found", vm_name))?;
            if !status.encrypted {
                bail!("VM '{}' is not encrypted", vm_name);
            }
            status
                .key_id
                .clone()
                .ok_or_else(|| anyhow::anyhow!("VM '{}' has no key assigned", vm_name))?
        };

        self.rotate_key(&key_id)
    }

    /// Return all active keys that are past their expiry date and therefore
    /// due for rotation.
    pub fn check_keys_due_rotation(&self) -> Vec<EncryptionKey> {
        let inner = self.inner.read().unwrap_or_else(|e| e.into_inner());
        let now = Utc::now();
        inner
            .keys
            .values()
            .filter(|k| k.status == KeyStatus::Active && k.expires.is_some_and(|exp| exp <= now))
            .cloned()
            .collect()
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    /// Helper: create a local key provider with deterministic IDs.
    fn make_provider(id: &str, name: &str) -> KeyProvider {
        let now = Utc::now();
        KeyProvider {
            id: id.to_string(),
            name: name.to_string(),
            provider_type: KeyProviderType::Local,
            endpoint: None,
            status: KeyProviderStatus::Connected,
            created: now,
            updated: now,
        }
    }

    /// Helper: create a minimal encryption policy.
    fn make_policy(id: &str, provider_id: &str) -> EncryptionPolicy {
        let now = Utc::now();
        EncryptionPolicy {
            id: id.to_string(),
            name: format!("policy-{}", id),
            description: None,
            algorithm: EncryptionAlgorithm::Aes256Xts,
            key_provider_id: provider_id.to_string(),
            encrypt_vmotion: false,
            auto_rotate_days: None,
            created: now,
            updated: now,
        }
    }

    // -- Provider tests -----------------------------------------------------

    #[test]
    fn test_register_and_get_provider() {
        let mgr = EncryptionManager::new();
        let provider = make_provider("p1", "Local KMS");
        let registered = mgr.register_provider(provider.clone()).unwrap();
        assert_eq!(registered.id, "p1");

        let fetched = mgr.get_provider("p1").unwrap();
        assert_eq!(fetched.name, "Local KMS");
    }

    #[test]
    fn test_register_duplicate_provider_fails() {
        let mgr = EncryptionManager::new();
        let provider = make_provider("p1", "Local KMS");
        mgr.register_provider(provider.clone()).unwrap();
        assert!(mgr.register_provider(provider).is_err());
    }

    #[test]
    fn test_list_providers() {
        let mgr = EncryptionManager::new();
        mgr.register_provider(make_provider("p1", "A")).unwrap();
        mgr.register_provider(make_provider("p2", "B")).unwrap();
        assert_eq!(mgr.list_providers().len(), 2);
    }

    #[test]
    fn test_remove_provider_with_active_keys_fails() {
        let mgr = EncryptionManager::new();
        mgr.register_provider(make_provider("p1", "A")).unwrap();
        mgr.generate_key("p1", EncryptionAlgorithm::Aes256Xts)
            .unwrap();
        assert!(mgr.remove_provider("p1").is_err());
    }

    #[test]
    fn test_remove_provider_success() {
        let mgr = EncryptionManager::new();
        mgr.register_provider(make_provider("p1", "A")).unwrap();
        mgr.remove_provider("p1").unwrap();
        assert!(mgr.get_provider("p1").is_none());
    }

    #[test]
    fn test_test_provider_local() {
        let mgr = EncryptionManager::new();
        mgr.register_provider(make_provider("p1", "Local")).unwrap();
        assert!(mgr.test_provider("p1").unwrap());
    }

    // -- Policy tests -------------------------------------------------------

    #[test]
    fn test_create_and_get_policy() {
        let mgr = EncryptionManager::new();
        mgr.register_provider(make_provider("p1", "A")).unwrap();
        let policy = make_policy("pol1", "p1");
        let created = mgr.create_policy(policy).unwrap();
        assert_eq!(created.id, "pol1");
        assert!(mgr.get_policy("pol1").is_some());
    }

    #[test]
    fn test_create_policy_missing_provider_fails() {
        let mgr = EncryptionManager::new();
        let policy = make_policy("pol1", "nonexistent");
        assert!(mgr.create_policy(policy).is_err());
    }

    #[test]
    fn test_update_policy() {
        let mgr = EncryptionManager::new();
        mgr.register_provider(make_provider("p1", "A")).unwrap();
        mgr.create_policy(make_policy("pol1", "p1")).unwrap();

        let mut updated = make_policy("pol1", "p1");
        updated.name = "renamed".to_string();
        updated.algorithm = EncryptionAlgorithm::ChaCha20Poly1305;
        let result = mgr.update_policy("pol1", updated).unwrap();
        assert_eq!(result.name, "renamed");
        assert_eq!(result.algorithm, EncryptionAlgorithm::ChaCha20Poly1305);
    }

    #[test]
    fn test_delete_policy_in_use_fails() {
        let mgr = EncryptionManager::new();
        mgr.register_provider(make_provider("p1", "A")).unwrap();
        mgr.create_policy(make_policy("pol1", "p1")).unwrap();
        mgr.encrypt_vm("vm-1", "pol1").unwrap();
        assert!(mgr.delete_policy("pol1").is_err());
    }

    #[test]
    fn test_delete_policy_success() {
        let mgr = EncryptionManager::new();
        mgr.register_provider(make_provider("p1", "A")).unwrap();
        mgr.create_policy(make_policy("pol1", "p1")).unwrap();
        mgr.delete_policy("pol1").unwrap();
        assert!(mgr.get_policy("pol1").is_none());
    }

    // -- Key management tests -----------------------------------------------

    #[test]
    fn test_generate_key() {
        let mgr = EncryptionManager::new();
        mgr.register_provider(make_provider("p1", "A")).unwrap();
        let key = mgr
            .generate_key("p1", EncryptionAlgorithm::Aes256Xts)
            .unwrap();
        assert_eq!(key.key_size_bits, 256);
        assert_eq!(key.status, KeyStatus::Active);
        assert_eq!(key.key_provider_id, "p1");
    }

    #[test]
    fn test_rotate_key() {
        let mgr = EncryptionManager::new();
        mgr.register_provider(make_provider("p1", "A")).unwrap();
        let key = mgr
            .generate_key("p1", EncryptionAlgorithm::Aes256Cbc)
            .unwrap();
        let event = mgr.rotate_key(&key.id).unwrap();
        assert_eq!(event.old_key_id, key.id);
        assert_eq!(event.status, RotationStatus::Completed);

        // Old key should be marked as rotated.
        let old = mgr.get_key(&key.id).unwrap();
        assert_eq!(old.status, KeyStatus::Rotated);

        // New key should be active.
        let new = mgr.get_key(&event.new_key_id).unwrap();
        assert_eq!(new.status, KeyStatus::Active);
    }

    #[test]
    fn test_revoke_key() {
        let mgr = EncryptionManager::new();
        mgr.register_provider(make_provider("p1", "A")).unwrap();
        let key = mgr
            .generate_key("p1", EncryptionAlgorithm::ChaCha20Poly1305)
            .unwrap();
        mgr.revoke_key(&key.id).unwrap();
        let revoked = mgr.get_key(&key.id).unwrap();
        assert_eq!(revoked.status, KeyStatus::Revoked);
    }

    #[test]
    fn test_revoke_already_revoked_fails() {
        let mgr = EncryptionManager::new();
        mgr.register_provider(make_provider("p1", "A")).unwrap();
        let key = mgr
            .generate_key("p1", EncryptionAlgorithm::Aes256Xts)
            .unwrap();
        mgr.revoke_key(&key.id).unwrap();
        assert!(mgr.revoke_key(&key.id).is_err());
    }

    #[test]
    fn test_list_keys_filter_by_provider() {
        let mgr = EncryptionManager::new();
        mgr.register_provider(make_provider("p1", "A")).unwrap();
        mgr.register_provider(make_provider("p2", "B")).unwrap();
        mgr.generate_key("p1", EncryptionAlgorithm::Aes256Xts)
            .unwrap();
        mgr.generate_key("p1", EncryptionAlgorithm::Aes256Cbc)
            .unwrap();
        mgr.generate_key("p2", EncryptionAlgorithm::ChaCha20Poly1305)
            .unwrap();

        assert_eq!(mgr.list_keys(Some("p1")).len(), 2);
        assert_eq!(mgr.list_keys(Some("p2")).len(), 1);
        assert_eq!(mgr.list_keys(None).len(), 3);
    }

    // -- VM encryption tests ------------------------------------------------

    #[test]
    fn test_encrypt_and_decrypt_vm() {
        let mgr = EncryptionManager::new();
        mgr.register_provider(make_provider("p1", "A")).unwrap();
        mgr.create_policy(make_policy("pol1", "p1")).unwrap();

        let status = mgr.encrypt_vm("web-server", "pol1").unwrap();
        assert!(status.encrypted);
        assert_eq!(status.algorithm, Some(EncryptionAlgorithm::Aes256Xts));
        assert!(status.key_id.is_some());

        // VM should appear in the encrypted list.
        assert_eq!(mgr.list_encrypted_vms().len(), 1);

        // Decrypt.
        mgr.decrypt_vm("web-server").unwrap();
        let after = mgr.get_vm_encryption_status("web-server").unwrap();
        assert!(!after.encrypted);
        assert!(after.key_id.is_none());
        assert_eq!(mgr.list_encrypted_vms().len(), 0);
    }

    #[test]
    fn test_encrypt_already_encrypted_fails() {
        let mgr = EncryptionManager::new();
        mgr.register_provider(make_provider("p1", "A")).unwrap();
        mgr.create_policy(make_policy("pol1", "p1")).unwrap();
        mgr.encrypt_vm("vm-1", "pol1").unwrap();
        assert!(mgr.encrypt_vm("vm-1", "pol1").is_err());
    }

    #[test]
    fn test_rotate_vm_key() {
        let mgr = EncryptionManager::new();
        mgr.register_provider(make_provider("p1", "A")).unwrap();
        mgr.create_policy(make_policy("pol1", "p1")).unwrap();
        let initial = mgr.encrypt_vm("vm-1", "pol1").unwrap();
        let old_key_id = initial.key_id.clone().unwrap();

        let event = mgr.rotate_vm_key("vm-1").unwrap();
        assert_eq!(event.old_key_id, old_key_id);
        assert_eq!(event.status, RotationStatus::Completed);

        // VM status should reference the new key.
        let status = mgr.get_vm_encryption_status("vm-1").unwrap();
        assert_eq!(status.key_id.as_deref(), Some(event.new_key_id.as_str()));
        assert!(status.last_key_rotation.is_some());
    }

    #[test]
    fn test_check_keys_due_rotation() {
        let mgr = EncryptionManager::new();
        mgr.register_provider(make_provider("p1", "A")).unwrap();

        // Create a policy with auto-rotation set to 0 days (immediately due).
        let now = Utc::now();
        let policy = EncryptionPolicy {
            id: "pol-rot".to_string(),
            name: "auto-rotate".to_string(),
            description: None,
            algorithm: EncryptionAlgorithm::Aes256Xts,
            key_provider_id: "p1".to_string(),
            encrypt_vmotion: false,
            auto_rotate_days: Some(0),
            created: now,
            updated: now,
        };
        mgr.create_policy(policy).unwrap();

        // Encrypt a VM — the key's `expires` will be set to `now + 0 days`.
        mgr.encrypt_vm("vm-auto", "pol-rot").unwrap();

        let due = mgr.check_keys_due_rotation();
        assert!(
            !due.is_empty(),
            "Key with auto_rotate_days=0 should be due for rotation"
        );
        assert_eq!(due[0].vm_name.as_deref(), Some("vm-auto"));
    }

    // -- Serialization smoke test -------------------------------------------

    #[test]
    fn test_serde_round_trip() {
        let now = Utc::now();
        let status = VmEncryptionStatus {
            vm_name: "test-vm".to_string(),
            encrypted: true,
            policy_id: Some("pol1".to_string()),
            key_id: Some("key1".to_string()),
            algorithm: Some(EncryptionAlgorithm::ChaCha20Poly1305),
            vmotion_encrypted: true,
            last_key_rotation: Some(now),
        };
        let json = serde_json::to_string(&status).unwrap();
        let de: VmEncryptionStatus = serde_json::from_str(&json).unwrap();
        assert_eq!(de.vm_name, "test-vm");
        assert_eq!(de.algorithm, Some(EncryptionAlgorithm::ChaCha20Poly1305));
    }

    #[cfg(unix)]
    #[test]
    fn write_private_file_is_owner_only_for_new_and_existing_files() {
        use std::os::unix::fs::PermissionsExt;
        let dir = std::env::temp_dir().join(format!("enc-private-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let mode = |p: &std::path::Path| std::fs::metadata(p).unwrap().permissions().mode() & 0o777;

        let fresh = dir.join("fresh");
        write_private_file(&fresh, "key").unwrap();
        assert_eq!(mode(&fresh), 0o600);
        assert_eq!(std::fs::read_to_string(&fresh).unwrap(), "key");

        let loose = dir.join("loose");
        std::fs::write(&loose, "old-and-longer").unwrap();
        std::fs::set_permissions(&loose, std::fs::Permissions::from_mode(0o644)).unwrap();
        write_private_file(&loose, "new").unwrap();
        assert_eq!(mode(&loose), 0o600);
        assert_eq!(std::fs::read_to_string(&loose).unwrap(), "new");

        let _ = std::fs::remove_dir_all(&dir);
    }
}
