// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: Apache-2.0

use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use chrono::Utc;
use std::sync::Arc;
use uuid::Uuid;

use crate::server::AppState;
use encryption::{EncryptionPolicy, KeyProvider, VmEncryptionStatus};
use security::{RequireAdmin, RequireRead, RequireWrite};

// ============================================================================
// Key provider handlers
// ============================================================================

pub async fn list_providers(
    RequireRead(_claims): RequireRead,
    State(state): State<Arc<AppState>>,
) -> impl IntoResponse {
    tracing::debug!("vm_encryption::{}", stringify!(list_providers));
    let items: Vec<KeyProvider> = state
        .store
        .list_entities("key_providers")
        .unwrap_or_else(|e| {
            tracing::error!("Storage error: {}", e);
            Vec::new()
        });
    Json(items)
}

pub async fn register_provider(
    RequireWrite(_claims): RequireWrite,
    State(state): State<Arc<AppState>>,
    Json(mut provider): Json<KeyProvider>,
) -> impl IntoResponse {
    tracing::debug!("vm_encryption::{}", stringify!(register_provider));
    provider.id = Uuid::new_v4().to_string();
    let now = Utc::now();
    provider.created = now;
    provider.updated = now;
    match state
        .store
        .save_entity("key_providers", &provider.id, &provider)
    {
        Ok(_) => (StatusCode::CREATED, Json(provider)).into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": e.to_string()})),
        )
            .into_response(),
    }
}

pub async fn remove_provider(
    RequireAdmin(_claims): RequireAdmin,
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    tracing::debug!("vm_encryption::{}", stringify!(remove_provider));
    if let Err(e) = state.store.delete_entity("key_providers", &id) {
        tracing::error!("Failed to delete provider: {}", e);
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": e.to_string()})),
        )
            .into_response();
    }
    (
        StatusCode::NO_CONTENT,
        Json(serde_json::json!({"status": "deleted"})),
    )
        .into_response()
}

pub async fn test_provider(
    RequireWrite(_claims): RequireWrite,
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    tracing::debug!("vm_encryption::{}", stringify!(test_provider));
    match state.store.get_entity::<KeyProvider>("key_providers", &id) {
        Ok(Some(p)) => {
            let ok = match p.provider_type {
                encryption::KeyProviderType::Local => true,
                _ => p.endpoint.is_some() && p.status != encryption::KeyProviderStatus::Error,
            };
            Json(serde_json::json!({"connected": ok})).into_response()
        }
        Ok(None) => (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({"error": "Key provider not found"})),
        )
            .into_response(),
        Err(_) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": "Internal server error"})),
        )
            .into_response(),
    }
}

// ============================================================================
// Encryption policy handlers
// ============================================================================

pub async fn list_policies(
    RequireRead(_claims): RequireRead,
    State(state): State<Arc<AppState>>,
) -> impl IntoResponse {
    tracing::debug!("vm_encryption::{}", stringify!(list_policies));
    let items: Vec<EncryptionPolicy> = state
        .store
        .list_entities("encryption_policies")
        .unwrap_or_else(|e| {
            tracing::error!("Storage error: {}", e);
            Vec::new()
        });
    Json(items)
}

pub async fn create_policy(
    RequireWrite(_claims): RequireWrite,
    State(state): State<Arc<AppState>>,
    Json(mut policy): Json<EncryptionPolicy>,
) -> impl IntoResponse {
    tracing::debug!("vm_encryption::{}", stringify!(create_policy));
    if policy.id.is_empty() {
        policy.id = Uuid::new_v4().to_string();
    }
    let now = Utc::now();
    policy.created = now;
    policy.updated = now;
    match state
        .store
        .save_entity("encryption_policies", &policy.id, &policy)
    {
        Ok(_) => (StatusCode::CREATED, Json(policy)).into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": e.to_string()})),
        )
            .into_response(),
    }
}

pub async fn get_policy(
    RequireRead(_claims): RequireRead,
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    tracing::debug!("vm_encryption::{}", stringify!(get_policy));
    match state
        .store
        .get_entity::<EncryptionPolicy>("encryption_policies", &id)
    {
        Ok(Some(p)) => Json(p).into_response(),
        Ok(None) => (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({"error": "Encryption policy not found"})),
        )
            .into_response(),
        Err(_) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": "Internal server error"})),
        )
            .into_response(),
    }
}

pub async fn update_policy(
    RequireWrite(_claims): RequireWrite,
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    Json(mut policy): Json<EncryptionPolicy>,
) -> impl IntoResponse {
    tracing::debug!("vm_encryption::{}", stringify!(update_policy));
    if state
        .store
        .get_entity::<EncryptionPolicy>("encryption_policies", &id)
        .ok()
        .flatten()
        .is_none()
    {
        return (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({"error": "Not found"})),
        )
            .into_response();
    }
    policy.id = id.clone();
    policy.updated = Utc::now();
    if let Err(e) = state.store.save_entity("encryption_policies", &id, &policy) {
        tracing::error!("Failed to save: {}", e);
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": e.to_string()})),
        )
            .into_response();
    }
    Json(policy).into_response()
}

pub async fn delete_policy(
    RequireAdmin(_claims): RequireAdmin,
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    tracing::debug!("vm_encryption::{}", stringify!(delete_policy));
    if let Err(e) = state.store.delete_entity("encryption_policies", &id) {
        tracing::error!("Failed to delete encryption policy: {}", e);
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": e.to_string()})),
        )
            .into_response();
    }
    (
        StatusCode::NO_CONTENT,
        Json(serde_json::json!({"status": "deleted"})),
    )
        .into_response()
}

// ============================================================================
// VM encryption handlers
// ============================================================================

#[derive(serde::Deserialize)]
pub struct EncryptVmRequest {
    pub vm_name: String,
    pub policy_id: String,
}

pub async fn encrypt_vm(
    RequireWrite(_claims): RequireWrite,
    State(state): State<Arc<AppState>>,
    Json(req): Json<EncryptVmRequest>,
) -> impl IntoResponse {
    tracing::debug!("vm_encryption::{}", stringify!(encrypt_vm));
    if let Err((s, m)) = crate::validation::validate_vm_name(&req.vm_name) {
        return (s, Json(serde_json::json!({"error": m}))).into_response();
    }
    let policy = match state
        .store
        .get_entity::<EncryptionPolicy>("encryption_policies", &req.policy_id)
    {
        Ok(Some(p)) => p,
        Ok(None) => {
            return (
                StatusCode::NOT_FOUND,
                Json(serde_json::json!({"error": "Policy not found"})),
            )
                .into_response()
        }
        Err(_) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"error": "Internal server error"})),
            )
                .into_response()
        }
    };
    let key_id = Uuid::new_v4().to_string();

    // Resolve the VM's actual, live disk (not a naming-convention guess --
    // almost every real VM is created from a shared base image not named
    // after itself, so the old guess silently never matched anything) and
    // run the qemu-img LUKS conversion BEFORE persisting or reporting
    // encrypted:true. The old code saved encrypted:true unconditionally and
    // then ran the actual encryption in a fire-and-forget spawn_blocking it
    // never awaited, so the API/dashboard reported success regardless of
    // whether encryption even started.
    let vm_name = req.vm_name.clone();
    let key_for_encrypt = key_id.clone();
    let disk_path = state.driver.get_disk_path(&vm_name).await;
    // FluxVM only learns about a VM on its first start (see start_vm's
    // lazy-create fallback in routes.rs) -- a VM made through the Create VM
    // wizard and never started has no FluxVM-side record yet, so disk
    // resolution fails here. Surface that as a clear precondition instead of
    // leaking "known to FluxVM" (an internal driver/dependency name the
    // user has no reason to recognize) straight into the error toast.
    if let Err(e) = &disk_path {
        if e.to_string().contains("known to FluxVM") {
            return (
                StatusCode::CONFLICT,
                Json(serde_json::json!({
                    "error": format!("'{}' must be started at least once before its disk can be encrypted.", vm_name)
                })),
            )
                .into_response();
        }
    }
    let encrypt_result = tokio::task::spawn_blocking(move || -> Result<(), String> {
        let image_path = disk_path
            .map_err(|e| format!("No disk image found for VM '{}': {}", vm_name, e))?
            .display()
            .to_string();
        let encrypted_path = format!("{}.encrypted", image_path);
        let secret_file = format!("/tmp/zyvor-fabricd-encrypt-{}", Uuid::new_v4().simple());
        encryption::write_private_file(&secret_file, &key_for_encrypt)
            .map_err(|e| format!("Failed to write secret file: {}", e))?;
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
        let _ = std::fs::remove_file(&secret_file);
        match output {
            Ok(out) if out.status.success() => {
                std::fs::rename(&encrypted_path, &image_path)
                    .map_err(|e| format!("Failed to replace with encrypted disk: {}", e))?;
                tracing::info!("Encrypted disk image for VM '{}'", vm_name);
                Ok(())
            }
            Ok(out) => {
                let stderr = String::from_utf8_lossy(&out.stderr).to_string();
                let _ = std::fs::remove_file(&encrypted_path);
                Err(format!("qemu-img encryption failed: {}", stderr))
            }
            Err(e) => Err(format!("Failed to run qemu-img: {}", e)),
        }
    })
    .await;

    let encrypted = match &encrypt_result {
        Ok(Ok(())) => true,
        Ok(Err(e)) => {
            tracing::error!("Disk encryption failed for VM '{}': {}", req.vm_name, e);
            false
        }
        Err(e) => {
            tracing::error!("Encryption task failed for VM '{}': {}", req.vm_name, e);
            false
        }
    };

    let status = VmEncryptionStatus {
        vm_name: req.vm_name.clone(),
        encrypted,
        policy_id: Some(req.policy_id),
        key_id: Some(key_id),
        algorithm: Some(policy.algorithm),
        vmotion_encrypted: policy.encrypt_vmotion,
        last_key_rotation: None,
    };
    if let Err(e) = state
        .store
        .save_entity("vm_encryption", &req.vm_name, &status)
    {
        tracing::error!("Failed to save encryption status: {}", e);
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": e.to_string()})),
        )
            .into_response();
    }

    if !encrypted {
        let msg = match &encrypt_result {
            Ok(Err(e)) => e.clone(),
            Err(e) => e.to_string(),
            Ok(Ok(())) => unreachable!(),
        };
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": format!("Encryption failed: {}", msg)})),
        )
            .into_response();
    }

    (StatusCode::OK, Json(status)).into_response()
}

#[derive(serde::Deserialize)]
pub struct DecryptVmRequest {
    pub vm_name: String,
}

pub async fn decrypt_vm(
    RequireWrite(_claims): RequireWrite,
    State(state): State<Arc<AppState>>,
    Json(req): Json<DecryptVmRequest>,
) -> impl IntoResponse {
    tracing::debug!("vm_encryption::{}", stringify!(decrypt_vm));
    if let Err((s, m)) = crate::validation::validate_vm_name(&req.vm_name) {
        return (s, Json(serde_json::json!({"error": m}))).into_response();
    }
    let status = VmEncryptionStatus {
        vm_name: req.vm_name.clone(),
        encrypted: false,
        policy_id: None,
        key_id: None,
        algorithm: None,
        vmotion_encrypted: false,
        last_key_rotation: None,
    };
    if let Err(e) = state
        .store
        .save_entity("vm_encryption", &req.vm_name, &status)
    {
        tracing::error!("Failed to save: {}", e);
    }
    (
        StatusCode::OK,
        Json(serde_json::json!({"status": "VM decrypted"})),
    )
        .into_response()
}

pub async fn get_vm_encryption_status(
    RequireRead(_claims): RequireRead,
    State(state): State<Arc<AppState>>,
    Path(vm_name): Path<String>,
) -> impl IntoResponse {
    tracing::debug!("vm_encryption::{}", stringify!(get_vm_encryption_status));
    if let Err((s, m)) = crate::validation::validate_vm_name(&vm_name) {
        return (s, Json(serde_json::json!({"error": m}))).into_response();
    }
    match state
        .store
        .get_entity::<VmEncryptionStatus>("vm_encryption", &vm_name)
    {
        Ok(Some(s)) => Json(s).into_response(),
        Ok(None) => (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({"error": "VM encryption status not found"})),
        )
            .into_response(),
        Err(_) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": "Internal server error"})),
        )
            .into_response(),
    }
}

pub async fn list_encrypted_vms(
    RequireRead(_claims): RequireRead,
    State(state): State<Arc<AppState>>,
) -> impl IntoResponse {
    tracing::debug!("vm_encryption::{}", stringify!(list_encrypted_vms));
    let items: Vec<VmEncryptionStatus> =
        state
            .store
            .list_entities("vm_encryption")
            .unwrap_or_else(|e| {
                tracing::error!("Storage error: {}", e);
                Vec::new()
            });
    let encrypted: Vec<_> = items.into_iter().filter(|s| s.encrypted).collect();
    Json(encrypted)
}

pub async fn rotate_vm_key(
    RequireWrite(_claims): RequireWrite,
    State(state): State<Arc<AppState>>,
    Path(vm_name): Path<String>,
) -> impl IntoResponse {
    tracing::debug!("vm_encryption::{}", stringify!(rotate_vm_key));
    if let Err((s, m)) = crate::validation::validate_vm_name(&vm_name) {
        return (s, Json(serde_json::json!({"error": m}))).into_response();
    }
    let mut status = match state
        .store
        .get_entity::<VmEncryptionStatus>("vm_encryption", &vm_name)
    {
        Ok(Some(s)) => s,
        Ok(None) => {
            return (
                StatusCode::NOT_FOUND,
                Json(serde_json::json!({"error": "VM encryption status not found"})),
            )
                .into_response()
        }
        Err(_) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"error": "Internal server error"})),
            )
                .into_response()
        }
    };
    if !status.encrypted {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"error": "VM is not encrypted"})),
        )
            .into_response();
    }
    status.key_id = Some(Uuid::new_v4().to_string());
    status.last_key_rotation = Some(Utc::now());
    if let Err(e) = state.store.save_entity("vm_encryption", &vm_name, &status) {
        tracing::error!("Failed to save: {}", e);
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": e.to_string()})),
        )
            .into_response();
    }
    Json(status).into_response()
}
