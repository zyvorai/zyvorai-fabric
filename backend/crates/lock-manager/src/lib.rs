// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: LicenseRef-Zyvor-Production-2.0

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use uuid::Uuid;

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VmLock {
    pub vm_name: String,
    pub holder_host_id: String,
    pub lease_id: String,
    pub acquired_at: DateTime<Utc>,
    pub last_renewed: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
    pub fence_token: u64,
    pub status: LockStatus,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LockStatus {
    Active,
    Expired,
    Fencing,
    Released,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FenceAction {
    pub id: String,
    pub vm_name: String,
    pub target_host_id: String,
    pub fence_type: FenceType,
    pub status: FenceStatus,
    pub initiated_at: DateTime<Utc>,
    pub completed_at: Option<DateTime<Utc>>,
    pub error: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FenceType {
    StopVm,
    TerminateVm,
    PowerOffHost,
    NetworkIsolate,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FenceStatus {
    Pending,
    InProgress,
    Completed,
    Failed,
    Skipped,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LockEvent {
    pub id: String,
    pub vm_name: String,
    pub event_type: LockEventType,
    pub host_id: String,
    pub fence_token: u64,
    pub details: Option<String>,
    pub timestamp: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LockEventType {
    Acquired,
    Renewed,
    Expired,
    Released,
    FenceInitiated,
    FenceCompleted,
    FenceFailed,
    Stolen,
}

// ---------------------------------------------------------------------------
// Errors
// ---------------------------------------------------------------------------

#[derive(Debug, thiserror::Error)]
pub enum LockError {
    #[error("VM '{0}' is already locked by host '{1}'")]
    AlreadyLocked(String, String),

    #[error("VM '{0}' is not locked")]
    NotLocked(String),

    #[error("VM '{0}' is locked by a different host (expected '{1}')")]
    WrongHolder(String, String),

    #[error("Stale fence token for VM '{0}': provided {1}, current {2}")]
    StaleFenceToken(String, u64, u64),

    #[error("Lease expired for VM '{0}'")]
    LeaseExpired(String),

    #[error("Fencing required before lock can be stolen for VM '{0}'")]
    FencingRequired(String),

    #[error("Internal lock error: {0}")]
    Internal(String),
}

/// Helper to convert a poisoned lock error into a LockError.
fn lock_err<T>(e: std::sync::PoisonError<T>) -> LockError {
    LockError::Internal(format!("Lock poisoned: {}", e))
}

// ---------------------------------------------------------------------------
// Config
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LockConfig {
    pub lease_duration_secs: u64,
    pub grace_period_secs: u64,
    pub fence_timeout_secs: u64,
    pub default_fence_type: FenceType,
}

impl Default for LockConfig {
    fn default() -> Self {
        Self {
            lease_duration_secs: 60,
            grace_period_secs: 30,
            fence_timeout_secs: 120,
            default_fence_type: FenceType::StopVm,
        }
    }
}

// ---------------------------------------------------------------------------
// LockManager
// ---------------------------------------------------------------------------

#[derive(Clone)]
pub struct LockManager {
    locks: Arc<RwLock<HashMap<String, VmLock>>>,
    fence_actions: Arc<RwLock<HashMap<String, FenceAction>>>,
    events: Arc<RwLock<Vec<LockEvent>>>,
    fence_token_counter: Arc<RwLock<u64>>,
    config: LockConfig,
}

impl LockManager {
    pub fn new(config: LockConfig) -> Self {
        Self {
            locks: Arc::new(RwLock::new(HashMap::new())),
            fence_actions: Arc::new(RwLock::new(HashMap::new())),
            events: Arc::new(RwLock::new(Vec::new())),
            fence_token_counter: Arc::new(RwLock::new(0)),
            config,
        }
    }

    fn next_fence_token(&self) -> Result<u64, LockError> {
        let mut counter = self.fence_token_counter.write().map_err(lock_err)?;
        *counter += 1;
        Ok(*counter)
    }

    fn record_event(&self, event: LockEvent) {
        if let Ok(mut events) = self.events.write() {
            events.push(event);
        }
    }

    pub fn acquire_lock(&self, vm_name: &str, host_id: &str) -> Result<VmLock, LockError> {
        let mut locks = self.locks.write().map_err(lock_err)?;

        if let Some(existing) = locks.get(vm_name) {
            if existing.status == LockStatus::Active {
                return Err(LockError::AlreadyLocked(
                    vm_name.to_string(),
                    existing.holder_host_id.clone(),
                ));
            }
        }

        let now = Utc::now();
        let fence_token = self.next_fence_token()?;
        let lease_id = Uuid::new_v4().to_string();
        let expires_at = now + chrono::Duration::seconds(self.config.lease_duration_secs as i64);

        let lock = VmLock {
            vm_name: vm_name.to_string(),
            holder_host_id: host_id.to_string(),
            lease_id: lease_id.clone(),
            acquired_at: now,
            last_renewed: now,
            expires_at,
            fence_token,
            status: LockStatus::Active,
        };

        locks.insert(vm_name.to_string(), lock.clone());

        tracing::info!(
            vm = vm_name,
            host = host_id,
            fence_token = fence_token,
            "VM lock acquired"
        );

        self.record_event(LockEvent {
            id: Uuid::new_v4().to_string(),
            vm_name: vm_name.to_string(),
            event_type: LockEventType::Acquired,
            host_id: host_id.to_string(),
            fence_token,
            details: Some(format!("lease_id={lease_id}")),
            timestamp: now,
        });

        Ok(lock)
    }

    pub fn renew_lock(
        &self,
        vm_name: &str,
        host_id: &str,
        lease_id: &str,
    ) -> Result<VmLock, LockError> {
        let mut locks = self.locks.write().map_err(lock_err)?;

        let lock = locks
            .get_mut(vm_name)
            .ok_or_else(|| LockError::NotLocked(vm_name.to_string()))?;

        if lock.holder_host_id != host_id {
            return Err(LockError::WrongHolder(
                vm_name.to_string(),
                host_id.to_string(),
            ));
        }

        if lock.lease_id != lease_id {
            return Err(LockError::WrongHolder(
                vm_name.to_string(),
                host_id.to_string(),
            ));
        }

        if lock.status != LockStatus::Active {
            return Err(LockError::LeaseExpired(vm_name.to_string()));
        }

        let now = Utc::now();
        lock.last_renewed = now;
        lock.expires_at = now + chrono::Duration::seconds(self.config.lease_duration_secs as i64);

        let result = lock.clone();

        self.record_event(LockEvent {
            id: Uuid::new_v4().to_string(),
            vm_name: vm_name.to_string(),
            event_type: LockEventType::Renewed,
            host_id: host_id.to_string(),
            fence_token: result.fence_token,
            details: None,
            timestamp: now,
        });

        Ok(result)
    }

    pub fn release_lock(&self, vm_name: &str, host_id: &str) -> Result<(), LockError> {
        let mut locks = self.locks.write().map_err(lock_err)?;

        let lock = locks
            .get_mut(vm_name)
            .ok_or_else(|| LockError::NotLocked(vm_name.to_string()))?;

        if lock.holder_host_id != host_id {
            return Err(LockError::WrongHolder(
                vm_name.to_string(),
                host_id.to_string(),
            ));
        }

        let fence_token = lock.fence_token;
        lock.status = LockStatus::Released;

        tracing::info!(vm = vm_name, host = host_id, "VM lock released");

        self.record_event(LockEvent {
            id: Uuid::new_v4().to_string(),
            vm_name: vm_name.to_string(),
            event_type: LockEventType::Released,
            host_id: host_id.to_string(),
            fence_token,
            details: None,
            timestamp: Utc::now(),
        });

        Ok(())
    }

    pub fn check_expired_locks(&self) -> Vec<VmLock> {
        let now = Utc::now();
        let grace = chrono::Duration::seconds(self.config.grace_period_secs as i64);
        let mut expired = Vec::new();

        let locks = match self.locks.read() {
            Ok(l) => l,
            Err(e) => {
                tracing::error!("Failed to read locks: {}", e);
                return expired;
            }
        };
        for lock in locks.values() {
            if lock.status == LockStatus::Active && now > lock.expires_at + grace {
                expired.push(lock.clone());
            }
        }

        expired
    }

    pub fn initiate_fence(
        &self,
        vm_name: &str,
        fence_type: FenceType,
    ) -> Result<FenceAction, LockError> {
        // Lock ordering: always acquire `locks` first, then `fence_actions`
        let mut locks = self.locks.write().map_err(lock_err)?;

        let lock = locks
            .get_mut(vm_name)
            .ok_or_else(|| LockError::NotLocked(vm_name.to_string()))?;

        lock.status = LockStatus::Fencing;
        let target_host_id = lock.holder_host_id.clone();
        let fence_token = lock.fence_token;

        let now = Utc::now();
        let action = FenceAction {
            id: Uuid::new_v4().to_string(),
            vm_name: vm_name.to_string(),
            target_host_id: target_host_id.clone(),
            fence_type,
            status: FenceStatus::Pending,
            initiated_at: now,
            completed_at: None,
            error: None,
        };

        let mut fences = self.fence_actions.write().map_err(lock_err)?;
        fences.insert(vm_name.to_string(), action.clone());

        tracing::info!(
            vm = vm_name,
            target = %target_host_id,
            "Fence initiated"
        );

        self.record_event(LockEvent {
            id: Uuid::new_v4().to_string(),
            vm_name: vm_name.to_string(),
            event_type: LockEventType::FenceInitiated,
            host_id: target_host_id,
            fence_token,
            details: None,
            timestamp: now,
        });

        Ok(action)
    }

    pub fn complete_fence(&self, vm_name: &str, fence_action_id: &str) -> Result<(), LockError> {
        // Lock ordering: always acquire `locks` first, then `fence_actions`
        let mut locks = self.locks.write().map_err(lock_err)?;
        let mut fences = self.fence_actions.write().map_err(lock_err)?;

        let action = fences
            .get_mut(vm_name)
            .ok_or_else(|| LockError::NotLocked(vm_name.to_string()))?;

        if action.id != fence_action_id {
            return Err(LockError::NotLocked(vm_name.to_string()));
        }

        action.status = FenceStatus::Completed;
        action.completed_at = Some(Utc::now());

        // Mark lock as expired
        if let Some(lock) = locks.get_mut(vm_name) {
            let fence_token = lock.fence_token;
            lock.status = LockStatus::Expired;

            self.record_event(LockEvent {
                id: Uuid::new_v4().to_string(),
                vm_name: vm_name.to_string(),
                event_type: LockEventType::FenceCompleted,
                host_id: lock.holder_host_id.clone(),
                fence_token,
                details: None,
                timestamp: Utc::now(),
            });
        }

        tracing::info!(vm = vm_name, "Fence completed");
        Ok(())
    }

    pub fn fail_fence(
        &self,
        vm_name: &str,
        fence_action_id: &str,
        error: String,
    ) -> Result<(), LockError> {
        // Lock ordering: always acquire `locks` first (read), then `fence_actions` (write)
        let locks = self.locks.read().map_err(lock_err)?;
        let mut fences = self.fence_actions.write().map_err(lock_err)?;

        let action = fences
            .get_mut(vm_name)
            .ok_or_else(|| LockError::NotLocked(vm_name.to_string()))?;

        if action.id != fence_action_id {
            return Err(LockError::NotLocked(vm_name.to_string()));
        }

        action.status = FenceStatus::Failed;
        action.completed_at = Some(Utc::now());
        action.error = Some(error.clone());

        if let Some(lock) = locks.get(vm_name) {
            self.record_event(LockEvent {
                id: Uuid::new_v4().to_string(),
                vm_name: vm_name.to_string(),
                event_type: LockEventType::FenceFailed,
                host_id: lock.holder_host_id.clone(),
                fence_token: lock.fence_token,
                details: Some(error),
                timestamp: Utc::now(),
            });
        }

        tracing::warn!(vm = vm_name, "Fence failed");
        Ok(())
    }

    pub fn steal_lock(&self, vm_name: &str, new_host_id: &str) -> Result<VmLock, LockError> {
        // Lock ordering: always acquire `locks` first, then `fence_actions`
        let mut locks = self.locks.write().map_err(lock_err)?;

        // Pre-condition: current lock must be Expired or fencing must be Completed
        if let Some(existing) = locks.get(vm_name) {
            match existing.status {
                LockStatus::Expired | LockStatus::Released => {
                    // OK to steal
                }
                LockStatus::Fencing => {
                    // Check if fence action is completed
                    let fences = self.fence_actions.read().map_err(lock_err)?;
                    if let Some(action) = fences.get(vm_name) {
                        if action.status != FenceStatus::Completed {
                            return Err(LockError::FencingRequired(vm_name.to_string()));
                        }
                    } else {
                        return Err(LockError::FencingRequired(vm_name.to_string()));
                    }
                }
                LockStatus::Active => {
                    return Err(LockError::FencingRequired(vm_name.to_string()));
                }
            }
        }

        let now = Utc::now();
        let fence_token = self.next_fence_token()?;
        let lease_id = Uuid::new_v4().to_string();
        let expires_at = now + chrono::Duration::seconds(self.config.lease_duration_secs as i64);

        let lock = VmLock {
            vm_name: vm_name.to_string(),
            holder_host_id: new_host_id.to_string(),
            lease_id,
            acquired_at: now,
            last_renewed: now,
            expires_at,
            fence_token,
            status: LockStatus::Active,
        };

        locks.insert(vm_name.to_string(), lock.clone());

        tracing::info!(
            vm = vm_name,
            new_host = new_host_id,
            fence_token = fence_token,
            "VM lock stolen"
        );

        self.record_event(LockEvent {
            id: Uuid::new_v4().to_string(),
            vm_name: vm_name.to_string(),
            event_type: LockEventType::Stolen,
            host_id: new_host_id.to_string(),
            fence_token,
            details: None,
            timestamp: now,
        });

        Ok(lock)
    }

    pub fn renew_all_locks_for_host(&self, host_id: &str) -> u32 {
        let mut locks = match self.locks.write() {
            Ok(l) => l,
            Err(e) => {
                tracing::error!("Failed to acquire lock for bulk renewal: {}", e);
                return 0;
            }
        };
        let now = Utc::now();
        let new_expiry = now + chrono::Duration::seconds(self.config.lease_duration_secs as i64);
        let mut count = 0;

        for lock in locks.values_mut() {
            if lock.holder_host_id == host_id && lock.status == LockStatus::Active {
                lock.last_renewed = now;
                lock.expires_at = new_expiry;
                count += 1;
            }
        }

        if count > 0 {
            tracing::debug!(host = host_id, count = count, "Bulk lock renewal");
        }

        count
    }

    pub fn get_lock(&self, vm_name: &str) -> Option<VmLock> {
        let locks = self.locks.read().ok()?;
        locks.get(vm_name).cloned()
    }

    pub fn list_locks(&self) -> Vec<VmLock> {
        let locks = match self.locks.read() {
            Ok(l) => l,
            Err(e) => {
                tracing::error!("Failed to read locks: {}", e);
                return Vec::new();
            }
        };
        locks.values().cloned().collect()
    }

    pub fn get_events(&self, vm_name: &str) -> Vec<LockEvent> {
        let events = match self.events.read() {
            Ok(e) => e,
            Err(e) => {
                tracing::error!("Failed to read events: {}", e);
                return Vec::new();
            }
        };
        events
            .iter()
            .filter(|e| e.vm_name == vm_name)
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

    fn test_config() -> LockConfig {
        LockConfig {
            lease_duration_secs: 10,
            grace_period_secs: 5,
            fence_timeout_secs: 30,
            default_fence_type: FenceType::StopVm,
        }
    }

    fn mgr() -> LockManager {
        LockManager::new(test_config())
    }

    // -- acquire / release --------------------------------------------------

    #[test]
    fn test_acquire_and_release() {
        let lm = mgr();

        let lock = lm.acquire_lock("vm-1", "host-a").unwrap();
        assert_eq!(lock.vm_name, "vm-1");
        assert_eq!(lock.holder_host_id, "host-a");
        assert_eq!(lock.status, LockStatus::Active);
        assert!(lock.fence_token > 0);

        lm.release_lock("vm-1", "host-a").unwrap();

        let lock = lm.get_lock("vm-1").unwrap();
        assert_eq!(lock.status, LockStatus::Released);
    }

    #[test]
    fn test_double_acquire_rejected() {
        let lm = mgr();

        lm.acquire_lock("vm-1", "host-a").unwrap();
        let result = lm.acquire_lock("vm-1", "host-b");

        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), LockError::AlreadyLocked(..)));
    }

    #[test]
    fn test_release_wrong_holder() {
        let lm = mgr();

        lm.acquire_lock("vm-1", "host-a").unwrap();
        let result = lm.release_lock("vm-1", "host-b");

        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), LockError::WrongHolder(..)));
    }

    // -- renew --------------------------------------------------------------

    #[test]
    fn test_renew_lock() {
        let lm = mgr();

        let lock = lm.acquire_lock("vm-1", "host-a").unwrap();
        let original_expires = lock.expires_at;

        let renewed = lm.renew_lock("vm-1", "host-a", &lock.lease_id).unwrap();
        assert!(renewed.expires_at >= original_expires);
        assert_eq!(renewed.status, LockStatus::Active);
    }

    #[test]
    fn test_renew_wrong_holder() {
        let lm = mgr();

        let lock = lm.acquire_lock("vm-1", "host-a").unwrap();
        let result = lm.renew_lock("vm-1", "host-b", &lock.lease_id);

        assert!(result.is_err());
    }

    // -- expiry detection ---------------------------------------------------

    #[test]
    fn test_expiry_detection() {
        let config = LockConfig {
            lease_duration_secs: 0,
            grace_period_secs: 0,
            ..test_config()
        };
        let lm = LockManager::new(config);

        lm.acquire_lock("vm-1", "host-a").unwrap();

        // With zero lease + zero grace, lock should be immediately expired
        let expired = lm.check_expired_locks();
        assert_eq!(expired.len(), 1);
        assert_eq!(expired[0].vm_name, "vm-1");
    }

    #[test]
    fn test_active_lock_not_expired() {
        let lm = mgr();

        lm.acquire_lock("vm-1", "host-a").unwrap();

        let expired = lm.check_expired_locks();
        assert!(expired.is_empty());
    }

    // -- fence lifecycle ----------------------------------------------------

    #[test]
    fn test_fence_lifecycle() {
        let config = LockConfig {
            lease_duration_secs: 0,
            grace_period_secs: 0,
            ..test_config()
        };
        let lm = LockManager::new(config);

        lm.acquire_lock("vm-1", "host-a").unwrap();

        // Initiate fence
        let action = lm.initiate_fence("vm-1", FenceType::StopVm).unwrap();
        assert_eq!(action.status, FenceStatus::Pending);
        assert_eq!(action.target_host_id, "host-a");

        let lock = lm.get_lock("vm-1").unwrap();
        assert_eq!(lock.status, LockStatus::Fencing);

        // Complete fence
        lm.complete_fence("vm-1", &action.id).unwrap();

        let lock = lm.get_lock("vm-1").unwrap();
        assert_eq!(lock.status, LockStatus::Expired);
    }

    #[test]
    fn test_fence_failure() {
        let lm = mgr();

        lm.acquire_lock("vm-1", "host-a").unwrap();

        let action = lm.initiate_fence("vm-1", FenceType::TerminateVm).unwrap();
        lm.fail_fence("vm-1", &action.id, "Connection refused".to_string())
            .unwrap();

        // Lock should still be in Fencing status after failure
        let lock = lm.get_lock("vm-1").unwrap();
        assert_eq!(lock.status, LockStatus::Fencing);
    }

    // -- fence token monotonicity -------------------------------------------

    #[test]
    fn test_fence_token_monotonic() {
        let lm = mgr();

        let lock1 = lm.acquire_lock("vm-1", "host-a").unwrap();
        let lock2 = lm.acquire_lock("vm-2", "host-b").unwrap();

        assert!(lock2.fence_token > lock1.fence_token);

        // Release and re-acquire — new token should be higher
        lm.release_lock("vm-1", "host-a").unwrap();
        let lock3 = lm.acquire_lock("vm-1", "host-c").unwrap();
        assert!(lock3.fence_token > lock2.fence_token);
    }

    // -- steal_lock ---------------------------------------------------------

    #[test]
    fn test_steal_lock_after_expiry() {
        let config = LockConfig {
            lease_duration_secs: 0,
            grace_period_secs: 0,
            ..test_config()
        };
        let lm = LockManager::new(config);

        lm.acquire_lock("vm-1", "host-a").unwrap();

        // Initiate and complete fence to transition to Expired
        let action = lm.initiate_fence("vm-1", FenceType::StopVm).unwrap();
        lm.complete_fence("vm-1", &action.id).unwrap();

        // Now steal should succeed
        let stolen = lm.steal_lock("vm-1", "host-b").unwrap();
        assert_eq!(stolen.holder_host_id, "host-b");
        assert_eq!(stolen.status, LockStatus::Active);
    }

    #[test]
    fn test_steal_lock_active_rejected() {
        let lm = mgr();

        lm.acquire_lock("vm-1", "host-a").unwrap();

        let result = lm.steal_lock("vm-1", "host-b");
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            LockError::FencingRequired(..)
        ));
    }

    #[test]
    fn test_steal_lock_fence_token_increases() {
        let config = LockConfig {
            lease_duration_secs: 0,
            grace_period_secs: 0,
            ..test_config()
        };
        let lm = LockManager::new(config);

        let original = lm.acquire_lock("vm-1", "host-a").unwrap();

        let action = lm.initiate_fence("vm-1", FenceType::StopVm).unwrap();
        lm.complete_fence("vm-1", &action.id).unwrap();

        let stolen = lm.steal_lock("vm-1", "host-b").unwrap();
        assert!(stolen.fence_token > original.fence_token);
    }

    // -- bulk renewal -------------------------------------------------------

    #[test]
    fn test_renew_all_locks_for_host() {
        let lm = mgr();

        lm.acquire_lock("vm-1", "host-a").unwrap();
        lm.acquire_lock("vm-2", "host-a").unwrap();
        lm.acquire_lock("vm-3", "host-b").unwrap();

        let count = lm.renew_all_locks_for_host("host-a");
        assert_eq!(count, 2);
    }

    // -- events -------------------------------------------------------------

    #[test]
    fn test_event_recording() {
        let lm = mgr();

        lm.acquire_lock("vm-1", "host-a").unwrap();
        lm.release_lock("vm-1", "host-a").unwrap();

        let events = lm.get_events("vm-1");
        assert!(events.len() >= 2);
        assert_eq!(events[0].event_type, LockEventType::Acquired);
        assert_eq!(events[1].event_type, LockEventType::Released);
    }

    // -- list / get ---------------------------------------------------------

    #[test]
    fn test_list_locks() {
        let lm = mgr();

        assert!(lm.list_locks().is_empty());

        lm.acquire_lock("vm-1", "host-a").unwrap();
        lm.acquire_lock("vm-2", "host-b").unwrap();

        assert_eq!(lm.list_locks().len(), 2);
    }

    #[test]
    fn test_get_lock_not_found() {
        let lm = mgr();
        assert!(lm.get_lock("nonexistent").is_none());
    }

    // -- serde roundtrip ----------------------------------------------------

    #[test]
    fn test_serde_roundtrip() {
        let lm = mgr();
        let lock = lm.acquire_lock("vm-1", "host-a").unwrap();

        let json = serde_json::to_string(&lock).unwrap();
        let deserialized: VmLock = serde_json::from_str(&json).unwrap();

        assert_eq!(deserialized.vm_name, lock.vm_name);
        assert_eq!(deserialized.status, LockStatus::Active);
        assert!(json.contains("\"active\""));
    }
}
