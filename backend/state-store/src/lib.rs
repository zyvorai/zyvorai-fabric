// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: LicenseRef-Zyvor-Production-2.0

use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs::{self, File, OpenOptions};
use std::io::{Seek, SeekFrom, Write};
use std::os::unix::io::AsRawFd;
use std::path::{Path, PathBuf};
use std::sync::{Arc, RwLock};
use tracing::warn;
use vm_model::VM;

#[derive(Clone)]
pub struct StateStore {
    path: PathBuf,
    vms: Arc<RwLock<HashMap<String, VM>>>,
}

/// True when `try_create_entity` lost the race to an existing id.
pub fn is_entity_conflict(err: &anyhow::Error) -> bool {
    err.to_string().contains("entity already exists")
}

struct FlockGuard {
    fd: std::os::unix::io::RawFd,
}

impl FlockGuard {
    fn lock(file: &File) -> Result<Self> {
        let fd = file.as_raw_fd();
        let rc = unsafe { libc::flock(fd, libc::LOCK_EX) };
        if rc != 0 {
            anyhow::bail!("flock failed: {}", std::io::Error::last_os_error());
        }
        Ok(Self { fd })
    }
}

impl Drop for FlockGuard {
    fn drop(&mut self) {
        unsafe {
            libc::flock(self.fd, libc::LOCK_UN);
        }
    }
}

/// Generic entity storage helper
impl StateStore {
    /// Save any serializable entity to a subdirectory (atomic write)
    /// Validate that an entity ID does not contain path traversal characters.
    fn validate_entity_id(id: &str) -> Result<()> {
        if id.is_empty()
            || id.contains('\\')
            || id.contains('/')
            || id.contains("..")
            || id.contains('\0')
        {
            anyhow::bail!("Invalid entity ID: must not contain path separators, traversal sequences, null bytes, or be empty");
        }
        Ok(())
    }

    /// Entity directories may be nested (`notifications/channels`). Each
    /// segment is a single safe component; `..` and separators inside a
    /// segment are rejected.
    fn entity_subdir(subdir: &str) -> Result<&str> {
        if subdir.is_empty()
            || subdir.starts_with('/')
            || subdir.ends_with('/')
            || subdir.contains('\\')
            || subdir.contains('\0')
        {
            anyhow::bail!("Invalid entity directory");
        }
        let mut count = 0usize;
        for part in subdir.split('/') {
            if !input_guard::is_safe_component(part) {
                anyhow::bail!("Invalid entity directory");
            }
            count += 1;
            if count > 8 {
                anyhow::bail!("Invalid entity directory");
            }
        }
        Ok(subdir)
    }

    pub fn save_entity<T: Serialize>(&self, subdir: &str, id: &str, entity: &T) -> Result<()> {
        let subdir = Self::entity_subdir(subdir)?;
        let id = input_guard::vet_component!(id, anyhow::anyhow!("Invalid entity ID"));
        Self::validate_entity_id(id)?;
        let root = self.path.to_string_lossy();
        let file_path = format!("{root}/{subdir}/{id}.json");
        let tmp_path = format!("{file_path}.tmp");
        input_guard::fs_checked!(file_path, anyhow::anyhow!("rejected path"), |file_path| {
            input_guard::fs_checked!(tmp_path, anyhow::anyhow!("rejected path"), |tmp_path| {
                if let Some((dir, _)) = file_path.rsplit_once('/') {
                    fs::create_dir_all(dir)?;
                }
                let content = serde_json::to_string_pretty(entity)?;
                fs::write(&tmp_path, content)?;
                fs::rename(&tmp_path, &file_path)?;
                Ok(())
            })
        })
    }

    /// Insert an entity only when its file does not exist.
    ///
    /// `create_new` is the cross-process lease: two fabricd processes cannot
    /// both win the same id. Returns an error containing `entity already exists`
    /// on conflict (`is_entity_conflict`).
    pub fn try_create_entity<T: Serialize>(
        &self,
        subdir: &str,
        id: &str,
        entity: &T,
    ) -> Result<()> {
        let subdir = Self::entity_subdir(subdir)?;
        let id = input_guard::vet_component!(id, anyhow::anyhow!("Invalid entity ID"));
        Self::validate_entity_id(id)?;
        let root = self.path.to_string_lossy();
        let file_path = format!("{root}/{subdir}/{id}.json");
        input_guard::fs_checked!(file_path, anyhow::anyhow!("rejected path"), |file_path| {
            if let Some((dir, _)) = file_path.rsplit_once('/') {
                fs::create_dir_all(dir)?;
            }
            let content = serde_json::to_string_pretty(entity)?;
            match OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&file_path)
            {
                Ok(mut file) => {
                    file.write_all(content.as_bytes())?;
                    file.sync_all()?;
                    if let Some((dir, _)) = file_path.rsplit_once('/') {
                        fsync_dir(dir)?;
                    }
                    Ok(())
                }
                Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
                    anyhow::bail!("entity already exists")
                }
                Err(e) => Err(e.into()),
            }
        })
    }

    /// Delete one entity file when it is missing, empty, or not JSON.
    ///
    /// `try_create_entity` creates the file before the write finishes. A crash
    /// in that window leaves a name that `list_entities` skips and that a
    /// later create can never take.
    pub fn reclaim_unreadable_entity(&self, subdir: &str, id: &str) -> Result<bool> {
        let subdir = Self::entity_subdir(subdir)?;
        let id = input_guard::vet_component!(id, anyhow::anyhow!("Invalid entity ID"));
        Self::validate_entity_id(id)?;
        let root = self.path.to_string_lossy();
        let file_path = format!("{root}/{subdir}/{id}.json");
        input_guard::fs_checked!(file_path, anyhow::anyhow!("rejected path"), |file_path| {
            let content = match fs::read_to_string(&file_path) {
                Ok(content) => content,
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(false),
                Err(e) => return Err(e.into()),
            };
            if !content.trim().is_empty()
                && serde_json::from_str::<serde_json::Value>(&content).is_ok()
            {
                return Ok(false);
            }
            fs::remove_file(&file_path)?;
            if let Some((dir, _)) = file_path.rsplit_once('/') {
                fsync_dir(dir)?;
            }
            Ok(true)
        })
    }

    /// Delete every empty or non-JSON entity file in `subdir`.
    pub fn reclaim_unreadable_entities(&self, subdir: &str) -> Result<Vec<String>> {
        let subdir = Self::entity_subdir(subdir)?;
        let root = self.path.to_string_lossy();
        let dir = format!("{root}/{subdir}");
        input_guard::fs_checked!(dir, anyhow::anyhow!("rejected path"), |dir| {
            if !Path::new(&dir).exists() {
                return Ok(Vec::new());
            }
            let mut removed = Vec::new();
            for entry in fs::read_dir(&dir)? {
                let entry = entry?;
                let path = entry.path();
                if path.extension().and_then(|s| s.to_str()) != Some("json") {
                    continue;
                }
                let content = fs::read_to_string(&path).unwrap_or_default();
                if !content.trim().is_empty()
                    && serde_json::from_str::<serde_json::Value>(&content).is_ok()
                {
                    continue;
                }
                let id = path
                    .file_stem()
                    .and_then(|s| s.to_str())
                    .unwrap_or_default()
                    .to_string();
                fs::remove_file(&path)?;
                removed.push(id);
            }
            if !removed.is_empty() {
                fsync_dir(&dir)?;
            }
            Ok(removed)
        })
    }

    /// Ids whose files are empty or not JSON. Does not delete them.
    pub fn list_unreadable_entities(&self, subdir: &str) -> Result<Vec<String>> {
        let subdir = Self::entity_subdir(subdir)?;
        let root = self.path.to_string_lossy();
        let dir = format!("{root}/{subdir}");
        input_guard::fs_checked!(dir, anyhow::anyhow!("rejected path"), |dir| {
            if !Path::new(&dir).exists() {
                return Ok(Vec::new());
            }
            let mut ids = Vec::new();
            for entry in fs::read_dir(&dir)? {
                let entry = entry?;
                let path = entry.path();
                if path.extension().and_then(|s| s.to_str()) != Some("json") {
                    continue;
                }
                let content = fs::read_to_string(&path).unwrap_or_default();
                if !content.trim().is_empty()
                    && serde_json::from_str::<serde_json::Value>(&content).is_ok()
                {
                    continue;
                }
                let id = path
                    .file_stem()
                    .and_then(|s| s.to_str())
                    .unwrap_or_default()
                    .to_string();
                if !id.is_empty() {
                    ids.push(id);
                }
            }
            Ok(ids)
        })
    }

    /// Exclusive same-host lease. The flock is the lock; the file records the
    /// owner, a fencing token, and an expiry. A dead holder drops the flock.
    /// Two nodes that do not share this filesystem need a transactional store.
    pub fn try_acquire_lease(
        &self,
        subdir: &str,
        id: &str,
        owner: &str,
        ttl_secs: i64,
        resource: &str,
    ) -> Result<FileLease> {
        let subdir = Self::entity_subdir(subdir)?;
        let id = input_guard::vet_component!(id, anyhow::anyhow!("Invalid entity ID"));
        Self::validate_entity_id(id)?;
        let root = self.path.to_string_lossy();
        let file_path = format!("{root}/{subdir}/{id}.json");
        input_guard::fs_checked!(file_path, anyhow::anyhow!("rejected path"), |file_path| {
            if let Some((dir, _)) = file_path.rsplit_once('/') {
                fs::create_dir_all(dir)?;
            }
            let mut file = OpenOptions::new()
                .read(true)
                .write(true)
                .create(true)
                .truncate(false)
                .open(&file_path)?;
            let rc = unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) };
            if rc != 0 {
                let err = std::io::Error::last_os_error();
                if err.kind() == std::io::ErrorKind::WouldBlock {
                    anyhow::bail!("lease held");
                }
                anyhow::bail!("flock failed: {err}");
            }
            let content = fs::read_to_string(&file_path).unwrap_or_default();
            let previous: u64 = serde_json::from_str::<LeaseBody>(&content)
                .map(|b| b.fencing_token)
                .unwrap_or(0);
            let now = unix_now();
            let body = LeaseBody {
                resource: resource.to_string(),
                owner: owner.to_string(),
                fencing_token: previous.saturating_add(1),
                acquired_unix: now,
                expires_unix: now.saturating_add(ttl_secs.max(1)),
            };
            let encoded = serde_json::to_string_pretty(&body)?;
            file.seek(SeekFrom::Start(0))?;
            file.set_len(0)?;
            file.write_all(encoded.as_bytes())?;
            file.sync_all()?;
            if let Some((dir, _)) = file_path.rsplit_once('/') {
                fsync_dir(dir)?;
            }
            Ok(FileLease {
                file,
                resource: body.resource,
                owner: body.owner,
                fencing_token: body.fencing_token,
                acquired_unix: body.acquired_unix,
                expires_unix: body.expires_unix,
            })
        })
    }

    /// Re-read, update, and rewrite one entity while holding an exclusive
    /// flock on its file. `save_entity` renames over the inode, so quota
    /// updates must write this same file or two processes can both increment.
    pub fn update_entity_exclusive<T, E, F>(&self, subdir: &str, id: &str, update: F) -> Result<T>
    where
        T: Serialize + for<'de> Deserialize<'de>,
        F: FnOnce(T) -> std::result::Result<T, E>,
        E: std::fmt::Display,
    {
        let subdir = Self::entity_subdir(subdir)?;
        let id = input_guard::vet_component!(id, anyhow::anyhow!("Invalid entity ID"));
        Self::validate_entity_id(id)?;
        let root = self.path.to_string_lossy();
        let file_path = format!("{root}/{subdir}/{id}.json");
        input_guard::fs_checked!(file_path, anyhow::anyhow!("rejected path"), |file_path| {
            let mut file = OpenOptions::new().read(true).write(true).open(&file_path)?;
            let _guard = FlockGuard::lock(&file)?;
            let content = fs::read_to_string(&file_path)?;
            let current: T = serde_json::from_str(&content)?;
            let next = update(current).map_err(|e| anyhow::anyhow!("{e}"))?;
            let encoded = serde_json::to_string_pretty(&next)?;
            file.seek(SeekFrom::Start(0))?;
            file.set_len(0)?;
            file.write_all(encoded.as_bytes())?;
            file.sync_all()?;
            Ok(next)
        })
    }

    /// Load a specific entity by ID
    pub fn get_entity<T: for<'de> Deserialize<'de>>(
        &self,
        subdir: &str,
        id: &str,
    ) -> Result<Option<T>> {
        let subdir = Self::entity_subdir(subdir)?;
        let id = input_guard::vet_component!(id, anyhow::anyhow!("Invalid entity ID"));
        Self::validate_entity_id(id)?;
        let root = self.path.to_string_lossy();
        let file_path = format!("{root}/{subdir}/{id}.json");
        input_guard::fs_checked!(file_path, anyhow::anyhow!("rejected path"), |file_path| {
            match fs::read_to_string(&file_path) {
                Ok(content) => {
                    let entity = serde_json::from_str(&content)?;
                    Ok(Some(entity))
                }
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
                Err(e) => Err(e.into()),
            }
        })
    }

    /// List all entities in a subdirectory
    pub fn list_entities<T: for<'de> Deserialize<'de>>(&self, subdir: &str) -> Result<Vec<T>> {
        let subdir = Self::entity_subdir(subdir)?;
        let root = self.path.to_string_lossy();
        let dir = format!("{root}/{subdir}");
        input_guard::fs_checked!(dir, anyhow::anyhow!("rejected path"), |dir| {
            if !Path::new(&dir).exists() {
                return Ok(Vec::new());
            }

            let mut entities = Vec::new();

            for entry in fs::read_dir(&dir)? {
                let entry = entry?;
                let path = entry.path();
                if path.extension().and_then(|s| s.to_str()) == Some("json") {
                    match fs::read_to_string(&path) {
                        Ok(content) => match serde_json::from_str::<T>(&content) {
                            Ok(entity) => entities.push(entity),
                            Err(e) => {
                                warn!(
                                    "Failed to deserialize entity from {}: {}",
                                    path.display(),
                                    e
                                );
                            }
                        },
                        Err(e) => {
                            warn!("Failed to read entity file {}: {}", path.display(), e);
                        }
                    }
                }
            }

            Ok(entities)
        })
    }

    /// List entities with a filter predicate and limit, avoiding loading all into memory.
    pub fn list_entities_filtered<T, F>(
        &self,
        subdir: &str,
        predicate: F,
        limit: usize,
    ) -> Result<Vec<T>>
    where
        T: for<'de> Deserialize<'de>,
        F: Fn(&T) -> bool,
    {
        let subdir = Self::entity_subdir(subdir)?;
        let root = self.path.to_string_lossy();
        let dir = format!("{root}/{subdir}");
        input_guard::fs_checked!(dir, anyhow::anyhow!("rejected path"), |dir| {
            if !Path::new(&dir).exists() {
                return Ok(Vec::new());
            }

            let mut entities = Vec::new();
            for entry in fs::read_dir(&dir)? {
                if entities.len() >= limit {
                    break;
                }
                let entry = entry?;
                let path = entry.path();
                if path.extension().and_then(|s| s.to_str()) == Some("json") {
                    match fs::read_to_string(&path) {
                        Ok(content) => match serde_json::from_str::<T>(&content) {
                            Ok(entity) => {
                                if predicate(&entity) {
                                    entities.push(entity);
                                }
                            }
                            Err(e) => {
                                warn!(
                                    "Failed to deserialize entity from {}: {}",
                                    path.display(),
                                    e
                                );
                            }
                        },
                        Err(e) => {
                            warn!("Failed to read entity file {}: {}", path.display(), e);
                        }
                    }
                }
            }

            Ok(entities)
        })
    }

    /// Delete an entity by ID
    pub fn delete_entity(&self, subdir: &str, id: &str) -> Result<()> {
        let subdir = Self::entity_subdir(subdir)?;
        let id = input_guard::vet_component!(id, anyhow::anyhow!("Invalid entity ID"));
        Self::validate_entity_id(id)?;
        let root = self.path.to_string_lossy();
        let file_path = format!("{root}/{subdir}/{id}.json");
        input_guard::fs_checked!(file_path, anyhow::anyhow!("rejected path"), |file_path| {
            match fs::remove_file(&file_path) {
                Ok(()) => {
                    if let Some((dir, _)) = file_path.rsplit_once('/') {
                        fsync_dir(dir)?;
                    }
                    Ok(())
                }
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
                Err(e) => Err(e.into()),
            }
        })
    }
}

fn fsync_dir(dir: &str) -> Result<()> {
    let file = File::open(dir)?;
    file.sync_all()?;
    Ok(())
}

fn unix_now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

#[derive(Debug, Serialize, Deserialize)]
struct LeaseBody {
    resource: String,
    owner: String,
    fencing_token: u64,
    acquired_unix: i64,
    expires_unix: i64,
}

/// Held exclusive lease. Dropping it unlocks the file.
#[derive(Debug)]
pub struct FileLease {
    file: File,
    pub resource: String,
    pub owner: String,
    pub fencing_token: u64,
    pub acquired_unix: i64,
    pub expires_unix: i64,
}

impl Drop for FileLease {
    fn drop(&mut self) {
        unsafe {
            libc::flock(self.file.as_raw_fd(), libc::LOCK_UN);
        }
    }
}

impl StateStore {
    pub fn new<P: AsRef<Path>>(path: P) -> Result<Self> {
        let path = path.as_ref().to_path_buf();
        fs::create_dir_all(&path)?;

        let mut vms = HashMap::new();

        // Load existing VMs from disk
        if let Ok(entries) = fs::read_dir(&path) {
            for entry in entries.flatten() {
                if let Ok(content) = fs::read_to_string(entry.path()) {
                    if let Ok(vm) = serde_json::from_str::<VM>(&content) {
                        vms.insert(vm.name.clone(), vm);
                    }
                }
            }
        }

        Ok(Self {
            path,
            vms: Arc::new(RwLock::new(vms)),
        })
    }

    pub fn save_vm(&self, vm: &VM) -> Result<()> {
        let name = input_guard::vet_component!(&vm.name, anyhow::anyhow!("Invalid VM name"));
        Self::validate_entity_id(name)?;
        let content = serde_json::to_string_pretty(vm)?;
        let root = self.path.to_string_lossy();
        let vm_file = format!("{root}/{name}.json");
        let tmp_file = format!("{vm_file}.tmp");
        input_guard::fs_checked!(vm_file, anyhow::anyhow!("rejected path"), |vm_file| {
            input_guard::fs_checked!(tmp_file, anyhow::anyhow!("rejected path"), |tmp_file| {
                fs::write(&tmp_file, &content)?;
                fs::rename(&tmp_file, &vm_file)?;
                Ok::<(), anyhow::Error>(())
            })
        })?;

        // Only update in-memory state after file write succeeds
        let mut vms = self
            .vms
            .write()
            .map_err(|e| anyhow::anyhow!("Lock poisoned: {}", e))?;
        vms.insert(vm.name.clone(), vm.clone());

        Ok(())
    }

    pub fn get_vm(&self, name: &str) -> Result<Option<VM>> {
        let vms = self
            .vms
            .read()
            .map_err(|e| anyhow::anyhow!("Lock poisoned: {}", e))?;
        Ok(vms.get(name).cloned())
    }

    pub fn list_vms(&self) -> Result<Vec<VM>> {
        let vms = self
            .vms
            .read()
            .map_err(|e| anyhow::anyhow!("Lock poisoned: {}", e))?;
        Ok(vms.values().cloned().collect())
    }

    /// List VMs with pagination. Returns (items, total_count).
    pub fn list_vms_paginated(&self, offset: usize, limit: usize) -> Result<(Vec<VM>, usize)> {
        let vms = self
            .vms
            .read()
            .map_err(|e| anyhow::anyhow!("Lock poisoned: {}", e))?;
        let total = vms.len();
        let mut sorted: Vec<&VM> = vms.values().collect();
        sorted.sort_by(|a, b| a.name.cmp(&b.name));
        let items: Vec<VM> = sorted
            .into_iter()
            .skip(offset)
            .take(limit)
            .cloned()
            .collect();
        Ok((items, total))
    }

    /// Count VMs without cloning.
    pub fn count_vms(&self) -> Result<usize> {
        let vms = self
            .vms
            .read()
            .map_err(|e| anyhow::anyhow!("Lock poisoned: {}", e))?;
        Ok(vms.len())
    }

    pub fn delete_vm(&self, name: &str) -> Result<()> {
        let name = input_guard::vet_component!(name, anyhow::anyhow!("Invalid VM name"));
        Self::validate_entity_id(name)?;
        let root = self.path.to_string_lossy();
        let vm_file = format!("{root}/{name}.json");
        input_guard::fs_checked!(vm_file, anyhow::anyhow!("rejected path"), |vm_file| {
            match fs::remove_file(&vm_file) {
                Ok(()) => Ok::<(), anyhow::Error>(()),
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
                Err(e) => Err(e.into()),
            }
        })?;

        // Only update in-memory state after file deletion succeeds
        let mut vms = self
            .vms
            .write()
            .map_err(|e| anyhow::anyhow!("Lock poisoned: {}", e))?;
        vms.remove(name);

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn test_store() -> (StateStore, TempDir) {
        let dir = TempDir::new().unwrap();
        let store = StateStore::new(dir.path()).unwrap();
        (store, dir)
    }

    #[test]
    fn test_save_and_load_vm() {
        let (store, _dir) = test_store();
        let vm = VM::new("test-vm".to_string(), "ubuntu.img".to_string(), 2, 1024);

        store.save_vm(&vm).unwrap();
        let loaded = store.get_vm("test-vm").unwrap();
        assert!(loaded.is_some());
        let loaded = loaded.unwrap();
        assert_eq!(loaded.name, "test-vm");
        assert_eq!(loaded.cpus, 2);
        assert_eq!(loaded.memory, 1024);
    }

    #[test]
    fn test_list_vms() {
        let (store, _dir) = test_store();
        store
            .save_vm(&VM::new("vm1".to_string(), "img".to_string(), 1, 512))
            .unwrap();
        store
            .save_vm(&VM::new("vm2".to_string(), "img".to_string(), 2, 1024))
            .unwrap();

        let vms = store.list_vms().unwrap();
        assert_eq!(vms.len(), 2);
    }

    #[test]
    fn test_delete_vm() {
        let (store, _dir) = test_store();
        store
            .save_vm(&VM::new("to-delete".to_string(), "img".to_string(), 1, 512))
            .unwrap();
        assert!(store.get_vm("to-delete").unwrap().is_some());

        store.delete_vm("to-delete").unwrap();
        assert!(store.get_vm("to-delete").unwrap().is_none());
    }

    #[test]
    fn test_get_nonexistent_vm() {
        let (store, _dir) = test_store();
        assert!(store.get_vm("nonexistent").unwrap().is_none());
    }

    #[test]
    fn test_atomic_write_creates_file() {
        let (store, dir) = test_store();
        let vm = VM::new("atomic-test".to_string(), "img".to_string(), 1, 512);
        store.save_vm(&vm).unwrap();

        let file = dir.path().join("atomic-test.json");
        assert!(file.exists());
        // Ensure no .tmp file remains
        let tmp = dir.path().join("atomic-test.json.tmp");
        assert!(!tmp.exists());
    }

    #[test]
    fn test_save_and_load_entity() {
        let (store, _dir) = test_store();

        #[derive(Debug, serde::Serialize, serde::Deserialize, PartialEq)]
        struct TestEntity {
            id: String,
            value: i32,
        }

        let entity = TestEntity {
            id: "test-1".to_string(),
            value: 42,
        };

        store
            .save_entity("test_entities", "test-1", &entity)
            .unwrap();
        let loaded: Option<TestEntity> = store.get_entity("test_entities", "test-1").unwrap();
        assert!(loaded.is_some());
        assert_eq!(loaded.unwrap().value, 42);
    }

    #[test]
    fn test_list_entities() {
        let (store, _dir) = test_store();

        #[derive(Debug, serde::Serialize, serde::Deserialize)]
        struct Item {
            name: String,
        }

        store
            .save_entity(
                "items",
                "a",
                &Item {
                    name: "alpha".to_string(),
                },
            )
            .unwrap();
        store
            .save_entity(
                "items",
                "b",
                &Item {
                    name: "beta".to_string(),
                },
            )
            .unwrap();

        let items: Vec<Item> = store.list_entities("items").unwrap();
        assert_eq!(items.len(), 2);
    }

    #[test]
    fn nested_entity_directory_round_trips() {
        let (store, _dir) = test_store();
        store
            .save_entity(
                "notifications/channels",
                "mail",
                &serde_json::json!({"id": "mail"}),
            )
            .unwrap();
        let items: Vec<serde_json::Value> = store.list_entities("notifications/channels").unwrap();
        assert_eq!(items.len(), 1);
        assert!(store
            .save_entity("notifications/../channels", "x", &serde_json::json!({}))
            .is_err());
    }

    #[test]
    fn test_delete_entity() {
        let (store, _dir) = test_store();

        #[derive(Debug, serde::Serialize, serde::Deserialize)]
        struct Item {
            name: String,
        }

        store
            .save_entity(
                "items",
                "x",
                &Item {
                    name: "x".to_string(),
                },
            )
            .unwrap();
        store.delete_entity("items", "x").unwrap();

        let loaded: Option<Item> = store.get_entity("items", "x").unwrap();
        assert!(loaded.is_none());
    }

    #[test]
    fn test_corrupted_json_skipped() {
        let (store, dir) = test_store();

        // Write a valid entity
        store
            .save_entity("test", "good", &serde_json::json!({"id": "good"}))
            .unwrap();

        // Write a corrupted file directly
        let bad_path = dir.path().join("test").join("bad.json");
        fs::write(&bad_path, "not valid json {{{").unwrap();

        // list_entities should skip the bad file
        let items: Vec<serde_json::Value> = store.list_entities("test").unwrap();
        assert_eq!(items.len(), 1);
    }

    #[test]
    fn test_concurrent_access() {
        let (store, _dir) = test_store();
        let store = Arc::new(store);

        let mut handles = vec![];
        for i in 0..10 {
            let store = store.clone();
            let handle = std::thread::spawn(move || {
                let vm = VM::new(format!("vm-{}", i), "img".to_string(), 1, 512);
                store.save_vm(&vm).unwrap();
            });
            handles.push(handle);
        }

        for handle in handles {
            handle.join().unwrap();
        }

        let vms = store.list_vms().unwrap();
        assert_eq!(vms.len(), 10);
    }

    #[test]
    fn test_persistence_across_instances() {
        let dir = TempDir::new().unwrap();

        // First instance writes
        {
            let store = StateStore::new(dir.path()).unwrap();
            store
                .save_vm(&VM::new(
                    "persistent".to_string(),
                    "img".to_string(),
                    4,
                    2048,
                ))
                .unwrap();
        }

        // Second instance reads
        {
            let store = StateStore::new(dir.path()).unwrap();
            let vm = store.get_vm("persistent").unwrap();
            assert!(vm.is_some());
            assert_eq!(vm.unwrap().cpus, 4);
        }
    }

    #[test]
    fn try_create_rejects_a_second_writer() {
        let (store, _dir) = test_store();
        store
            .try_create_entity("leases", "bdf-1", &serde_json::json!({"owner": "a"}))
            .unwrap();
        let err = store
            .try_create_entity("leases", "bdf-1", &serde_json::json!({"owner": "b"}))
            .unwrap_err();
        assert!(crate::is_entity_conflict(&err));
    }

    #[test]
    fn exclusive_update_increments_once_per_caller() {
        let (store, _dir) = test_store();
        store.try_create_entity("counters", "quota", &0u64).unwrap();
        let store = Arc::new(store);
        let mut handles = vec![];
        for _ in 0..8 {
            let store = store.clone();
            handles.push(std::thread::spawn(move || {
                store
                    .update_entity_exclusive("counters", "quota", |n: u64| {
                        Ok::<_, String>(n.saturating_add(1))
                    })
                    .unwrap();
            }));
        }
        for handle in handles {
            handle.join().unwrap();
        }
        let n: u64 = store.get_entity("counters", "quota").unwrap().unwrap();
        assert_eq!(n, 8);
    }

    #[test]
    fn unreadable_reservation_is_reclaimed() {
        let (store, dir) = test_store();
        let path = dir.path().join("ai_gpu_reservations");
        fs::create_dir_all(&path).unwrap();
        fs::write(path.join("0000-01.json"), "").unwrap();
        assert!(store
            .reclaim_unreadable_entity("ai_gpu_reservations", "0000-01")
            .unwrap());
        store
            .try_create_entity(
                "ai_gpu_reservations",
                "0000-01",
                &serde_json::json!({"bdf": "0000:01:00.0"}),
            )
            .unwrap();
        fs::write(path.join("partial.json"), "{").unwrap();
        let removed = store
            .reclaim_unreadable_entities("ai_gpu_reservations")
            .unwrap();
        assert_eq!(removed, vec!["partial".to_string()]);
        let kept: Option<serde_json::Value> =
            store.get_entity("ai_gpu_reservations", "0000-01").unwrap();
        assert!(kept.is_some());
    }

    #[test]
    fn lease_excludes_a_second_acquire_and_bumps_fencing_token() {
        let (store, _dir) = test_store();
        let first = store
            .try_acquire_lease(
                "ai_deployment_leases",
                "qwen",
                "owner-a",
                120,
                "deployment/qwen",
            )
            .unwrap();
        assert_eq!(first.fencing_token, 1);
        assert_eq!(first.resource, "deployment/qwen");
        let err = store
            .try_acquire_lease(
                "ai_deployment_leases",
                "qwen",
                "owner-b",
                120,
                "deployment/qwen",
            )
            .unwrap_err();
        assert!(err.to_string().contains("lease held"));
        drop(first);
        let second = store
            .try_acquire_lease(
                "ai_deployment_leases",
                "qwen",
                "owner-b",
                120,
                "deployment/qwen",
            )
            .unwrap();
        assert_eq!(second.fencing_token, 2);
        assert_eq!(second.owner, "owner-b");
    }
}
