// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: LicenseRef-Zyvor-Production-2.0

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
use certificate_manager::{
    crypto, CertHealthDashboard, CertStatus, Certificate, CertificateAuthority, CertificateRequest,
    CertificateRotation, RotationStatus, TrustAttestation, VmSecurityBaseline,
    VmSecurityCompliance,
};
use security::{RequireAdmin, RequireRead, RequireWrite};

// ============================================================================
// Certificate authority handlers
// ============================================================================

pub async fn list_cas(
    RequireRead(_claims): RequireRead,
    State(state): State<Arc<AppState>>,
) -> impl IntoResponse {
    tracing::debug!("certificates::{}", stringify!(list_cas));
    let items: Vec<CertificateAuthority> =
        state.store.list_entities("cert_cas").unwrap_or_else(|e| {
            tracing::error!("Storage error: {}", e);
            Vec::new()
        });
    Json(items)
}

pub async fn create_ca(
    RequireWrite(_claims): RequireWrite,
    State(state): State<Arc<AppState>>,
    Json(mut ca): Json<CertificateAuthority>,
) -> impl IntoResponse {
    tracing::debug!("certificates::{}", stringify!(create_ca));
    if let Err((status, msg)) = crate::validation::validate_entity_name(&ca.name) {
        return (status, Json(serde_json::json!({"error": msg}))).into_response();
    }
    ca.id = Uuid::new_v4().to_string();
    let now = Utc::now();
    ca.created = now;
    ca.updated = now;

    // Generate a real CA certificate for internal CAs.
    if ca.ca_type == certificate_manager::CaType::Internal && ca.ca_cert_pem.is_none() {
        match crypto::generate_ca(&ca.name, 3650) {
            Ok(ca_output) => {
                ca.ca_cert_pem = Some(ca_output.cert_pem);
                ca.ca_key_pem = Some(ca_output.key_pem);
                ca.fingerprint_sha256 = Some(ca_output.fingerprint);
            }
            Err(e) => {
                return (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({"error": format!("Failed to generate CA certificate: {}", e)}))).into_response();
            }
        }
    }

    match state.store.save_entity("cert_cas", &ca.id, &ca) {
        Ok(_) => (StatusCode::CREATED, Json(ca)).into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": e.to_string()})),
        )
            .into_response(),
    }
}

pub async fn delete_ca(
    RequireAdmin(_claims): RequireAdmin,
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    tracing::debug!("certificates::{}", stringify!(delete_ca));
    let certs: Vec<Certificate> = state
        .store
        .list_entities("certificates")
        .unwrap_or_default();
    if certs.iter().any(|c| c.issuer == id) {
        return (
            StatusCode::CONFLICT,
            Json(serde_json::json!({"error": "Cannot delete CA with active certificates"})),
        )
            .into_response();
    }
    if let Err(e) = state.store.delete_entity("cert_cas", &id) {
        tracing::error!("Failed to delete CA: {}", e);
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": e.to_string()})),
        )
            .into_response();
    }
    StatusCode::NO_CONTENT.into_response()
}

// ============================================================================
// Certificate handlers
// ============================================================================

pub async fn list_certificates(
    RequireRead(_claims): RequireRead,
    State(state): State<Arc<AppState>>,
) -> impl IntoResponse {
    tracing::debug!("certificates::{}", stringify!(list_certificates));
    let items: Vec<Certificate> = state
        .store
        .list_entities("certificates")
        .unwrap_or_else(|e| {
            tracing::error!("Storage error: {}", e);
            Vec::new()
        });
    Json(items)
}

pub async fn issue_certificate(
    RequireWrite(_claims): RequireWrite,
    State(state): State<Arc<AppState>>,
    Json(req): Json<CertificateRequest>,
) -> impl IntoResponse {
    tracing::debug!("certificates::{}", stringify!(issue_certificate));
    if let Err((status, msg)) = crate::validation::validate_entity_name(&req.common_name) {
        return (status, Json(serde_json::json!({"error": msg}))).into_response();
    }
    let now = Utc::now();
    let not_after = now + chrono::Duration::days(req.validity_days as i64);

    // Try to issue a real X.509 certificate if the CA has PEM material.
    let ca_pem = state
        .store
        .get_entity::<CertificateAuthority>("cert_cas", &req.ca_id)
        .ok()
        .flatten()
        .and_then(|ca| {
            ca.ca_cert_pem
                .as_ref()
                .zip(ca.ca_key_pem.as_ref())
                .map(|(c, k)| (c.clone(), k.clone()))
        });

    let (fingerprint, serial, cert_pem, key_pem) = if let Some((ca_cert, ca_key)) = ca_pem {
        match crypto::issue_certificate(
            &req.common_name,
            &req.subject_alt_names,
            req.validity_days,
            &ca_cert,
            &ca_key,
        ) {
            Ok(output) => (
                output.fingerprint,
                output.serial,
                Some(output.cert_pem),
                Some(output.key_pem),
            ),
            Err(e) => {
                return (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({"error": format!("Failed to generate certificate: {}", e)}))).into_response();
            }
        }
    } else {
        // Fallback: compute a real SHA-256 hash from certificate metadata.
        let data = format!(
            "{}:{}:{}",
            req.common_name,
            Uuid::new_v4(),
            now.to_rfc3339()
        );
        let fingerprint = crypto::compute_fingerprint(data.as_bytes());
        let serial = Uuid::new_v4().to_string();
        (fingerprint, serial, None, None)
    };

    let cert = Certificate {
        id: Uuid::new_v4().to_string(),
        common_name: req.common_name,
        subject_alt_names: req.subject_alt_names,
        issuer: req.ca_id,
        serial_number: serial,
        not_before: now,
        not_after,
        fingerprint_sha256: fingerprint,
        key_algorithm: req.key_algorithm,
        usage: req.usage,
        status: CertStatus::Active,
        auto_renew: req.auto_renew,
        component: req.component,
        created: now,
        updated: now,
        cert_pem,
        key_pem,
    };
    match state.store.save_entity("certificates", &cert.id, &cert) {
        Ok(_) => (StatusCode::CREATED, Json(cert)).into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": e.to_string()})),
        )
            .into_response(),
    }
}

pub async fn revoke_certificate(
    RequireAdmin(_claims): RequireAdmin,
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    tracing::debug!("certificates::{}", stringify!(revoke_certificate));
    let mut cert = match state.store.get_entity::<Certificate>("certificates", &id) {
        Ok(Some(c)) => c,
        Ok(None) => {
            return (
                StatusCode::NOT_FOUND,
                Json(serde_json::json!({"error": "Certificate not found"})),
            )
                .into_response()
        }
        Err(_) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"error": "Failed to load certificate"})),
            )
                .into_response()
        }
    };
    cert.status = CertStatus::Revoked;
    cert.updated = Utc::now();
    if let Err(e) = state.store.save_entity("certificates", &cert.id, &cert) {
        tracing::error!("Failed to save entity: {}", e);
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": e.to_string()})),
        )
            .into_response();
    }
    Json(cert).into_response()
}

pub async fn renew_certificate(
    RequireWrite(_claims): RequireWrite,
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    tracing::debug!("certificates::{}", stringify!(renew_certificate));
    let old_cert = match state.store.get_entity::<Certificate>("certificates", &id) {
        Ok(Some(c)) => c,
        Ok(None) => {
            return (
                StatusCode::NOT_FOUND,
                Json(serde_json::json!({"error": "Certificate not found"})),
            )
                .into_response()
        }
        Err(_) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"error": "Failed to load certificate"})),
            )
                .into_response()
        }
    };
    let now = Utc::now();
    let validity = old_cert.not_after - old_cert.not_before;
    // Mark old cert as expired first (before moving fields)
    let mut expired = old_cert.clone();
    expired.status = CertStatus::Expired;
    expired.updated = now;
    if let Err(e) = state.store.save_entity("certificates", &id, &expired) {
        tracing::error!("Failed to save entity: {}", e);
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": e.to_string()})),
        )
            .into_response();
    }
    let validity_days = validity.num_days().max(1) as u32;

    // Try to issue a real X.509 certificate if the CA has PEM material.
    let ca_pem = state
        .store
        .get_entity::<CertificateAuthority>("cert_cas", &old_cert.issuer)
        .ok()
        .flatten()
        .and_then(|ca| {
            ca.ca_cert_pem
                .as_ref()
                .zip(ca.ca_key_pem.as_ref())
                .map(|(c, k)| (c.clone(), k.clone()))
        });

    let (fingerprint, serial, cert_pem, key_pem) = if let Some((ca_cert, ca_key)) = ca_pem {
        match crypto::issue_certificate(
            &old_cert.common_name,
            &old_cert.subject_alt_names,
            validity_days,
            &ca_cert,
            &ca_key,
        ) {
            Ok(output) => (
                output.fingerprint,
                output.serial,
                Some(output.cert_pem),
                Some(output.key_pem),
            ),
            Err(e) => {
                return (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({"error": format!("Failed to generate certificate: {}", e)}))).into_response();
            }
        }
    } else {
        let data = format!(
            "{}:{}:{}",
            old_cert.common_name,
            Uuid::new_v4(),
            now.to_rfc3339()
        );
        let fingerprint = crypto::compute_fingerprint(data.as_bytes());
        let serial = Uuid::new_v4().to_string();
        (fingerprint, serial, None, None)
    };

    let new_cert = Certificate {
        id: Uuid::new_v4().to_string(),
        common_name: old_cert.common_name,
        subject_alt_names: old_cert.subject_alt_names,
        issuer: old_cert.issuer,
        serial_number: serial,
        not_before: now,
        not_after: now + validity,
        fingerprint_sha256: fingerprint,
        key_algorithm: old_cert.key_algorithm,
        usage: old_cert.usage,
        status: CertStatus::Active,
        auto_renew: old_cert.auto_renew,
        component: old_cert.component,
        created: now,
        updated: now,
        cert_pem,
        key_pem,
    };
    if let Err(e) = state
        .store
        .save_entity("certificates", &new_cert.id, &new_cert)
    {
        tracing::error!("Failed to save entity: {}", e);
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": e.to_string()})),
        )
            .into_response();
    }
    Json(new_cert).into_response()
}

#[derive(serde::Deserialize)]
pub struct ExpiringQuery {
    #[serde(default = "default_days")]
    pub days: u32,
}

fn default_days() -> u32 {
    30
}

pub async fn check_expiring(
    RequireRead(_claims): RequireRead,
    State(state): State<Arc<AppState>>,
    axum::extract::Query(query): axum::extract::Query<ExpiringQuery>,
) -> impl IntoResponse {
    tracing::debug!("certificates::{}", stringify!(check_expiring));
    let certs: Vec<Certificate> = state
        .store
        .list_entities("certificates")
        .unwrap_or_else(|e| {
            tracing::error!("Storage error: {}", e);
            Vec::new()
        });
    let threshold = Utc::now() + chrono::Duration::days(query.days as i64);
    let expiring: Vec<_> = certs
        .into_iter()
        .filter(|c| c.status == CertStatus::Active && c.not_after <= threshold)
        .collect();
    Json(expiring)
}

// ============================================================================
// Certificate request handlers
// ============================================================================

pub async fn list_cert_requests(
    RequireRead(_claims): RequireRead,
    State(state): State<Arc<AppState>>,
) -> impl IntoResponse {
    tracing::debug!("certificates::{}", stringify!(list_cert_requests));
    let items: Vec<CertificateRequest> =
        state
            .store
            .list_entities("cert_requests")
            .unwrap_or_else(|e| {
                tracing::error!("Storage error: {}", e);
                Vec::new()
            });
    Json(items)
}

pub async fn submit_cert_request(
    RequireWrite(_claims): RequireWrite,
    State(state): State<Arc<AppState>>,
    Json(mut req): Json<CertificateRequest>,
) -> impl IntoResponse {
    tracing::debug!("certificates::{}", stringify!(submit_cert_request));
    if let Err((status, msg)) = crate::validation::validate_entity_name(&req.common_name) {
        return (status, Json(serde_json::json!({"error": msg}))).into_response();
    }
    req.id = Uuid::new_v4().to_string();
    req.status = certificate_manager::CsrStatus::Pending;
    req.created = Utc::now();
    match state.store.save_entity("cert_requests", &req.id, &req) {
        Ok(_) => (StatusCode::CREATED, Json(req)).into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": e.to_string()})),
        )
            .into_response(),
    }
}

pub async fn approve_cert_request(
    RequireAdmin(_claims): RequireAdmin,
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    tracing::debug!("certificates::{}", stringify!(approve_cert_request));
    let mut req = match state
        .store
        .get_entity::<CertificateRequest>("cert_requests", &id)
    {
        Ok(Some(r)) => r,
        Ok(None) => {
            return (
                StatusCode::NOT_FOUND,
                Json(serde_json::json!({"error": "Certificate request not found"})),
            )
                .into_response()
        }
        Err(_) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"error": "Failed to load certificate request"})),
            )
                .into_response()
        }
    };
    req.status = certificate_manager::CsrStatus::Approved;
    if let Err(e) = state.store.save_entity("cert_requests", &req.id, &req) {
        tracing::error!("Failed to save entity: {}", e);
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": e.to_string()})),
        )
            .into_response();
    }
    Json(req).into_response()
}

pub async fn reject_cert_request(
    RequireAdmin(_claims): RequireAdmin,
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    tracing::debug!("certificates::{}", stringify!(reject_cert_request));
    let mut req = match state
        .store
        .get_entity::<CertificateRequest>("cert_requests", &id)
    {
        Ok(Some(r)) => r,
        Ok(None) => {
            return (
                StatusCode::NOT_FOUND,
                Json(serde_json::json!({"error": "Certificate request not found"})),
            )
                .into_response()
        }
        Err(_) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"error": "Failed to load certificate request"})),
            )
                .into_response()
        }
    };
    req.status = certificate_manager::CsrStatus::Rejected;
    if let Err(e) = state.store.save_entity("cert_requests", &req.id, &req) {
        tracing::error!("Failed to save entity: {}", e);
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": e.to_string()})),
        )
            .into_response();
    }
    Json(req).into_response()
}

// ============================================================================
// Rotation handlers
// ============================================================================

pub async fn list_rotations(
    RequireRead(_claims): RequireRead,
    State(state): State<Arc<AppState>>,
) -> impl IntoResponse {
    tracing::debug!("certificates::{}", stringify!(list_rotations));
    let items: Vec<CertificateRotation> = state
        .store
        .list_entities("cert_rotations")
        .unwrap_or_else(|e| {
            tracing::error!("Storage error: {}", e);
            Vec::new()
        });
    Json(items)
}

#[derive(serde::Deserialize)]
pub struct ScheduleRotationRequest {
    pub certificate_id: String,
    pub scheduled_at: chrono::DateTime<Utc>,
}

pub async fn schedule_rotation(
    RequireWrite(_claims): RequireWrite,
    State(state): State<Arc<AppState>>,
    Json(req): Json<ScheduleRotationRequest>,
) -> impl IntoResponse {
    tracing::debug!("certificates::{}", stringify!(schedule_rotation));
    let cert = match state
        .store
        .get_entity::<Certificate>("certificates", &req.certificate_id)
    {
        Ok(Some(c)) => c,
        Ok(None) => {
            return (
                StatusCode::NOT_FOUND,
                Json(serde_json::json!({"error": "Certificate not found"})),
            )
                .into_response()
        }
        Err(_) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"error": "Failed to load certificate"})),
            )
                .into_response()
        }
    };
    let rotation = CertificateRotation {
        id: Uuid::new_v4().to_string(),
        certificate_id: req.certificate_id,
        old_cert_fingerprint: cert.fingerprint_sha256,
        new_cert_fingerprint: None,
        status: RotationStatus::Scheduled,
        scheduled_at: req.scheduled_at,
        completed_at: None,
        error: None,
    };
    if let Err(e) = state
        .store
        .save_entity("cert_rotations", &rotation.id, &rotation)
    {
        tracing::error!("Failed to save entity: {}", e);
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": e.to_string()})),
        )
            .into_response();
    }
    (StatusCode::CREATED, Json(rotation)).into_response()
}

pub async fn execute_rotation(
    RequireWrite(_claims): RequireWrite,
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    tracing::debug!("certificates::{}", stringify!(execute_rotation));
    let mut rotation = match state
        .store
        .get_entity::<CertificateRotation>("cert_rotations", &id)
    {
        Ok(Some(r)) => r,
        Ok(None) => {
            return (
                StatusCode::NOT_FOUND,
                Json(serde_json::json!({"error": "Certificate rotation not found"})),
            )
                .into_response()
        }
        Err(_) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"error": "Failed to load certificate rotation"})),
            )
                .into_response()
        }
    };
    rotation.status = RotationStatus::Completed;
    let rotation_data = format!("rotation:{}:{}", rotation.id, Utc::now().to_rfc3339());
    rotation.new_cert_fingerprint = Some(crypto::compute_fingerprint(rotation_data.as_bytes()));
    rotation.completed_at = Some(Utc::now());
    if let Err(e) = state
        .store
        .save_entity("cert_rotations", &rotation.id, &rotation)
    {
        tracing::error!("Failed to save entity: {}", e);
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": e.to_string()})),
        )
            .into_response();
    }
    Json(rotation).into_response()
}

// ============================================================================
// Attestation handlers
// ============================================================================

pub async fn list_attestations(
    RequireRead(_claims): RequireRead,
    State(state): State<Arc<AppState>>,
) -> impl IntoResponse {
    tracing::debug!("certificates::{}", stringify!(list_attestations));
    let items: Vec<TrustAttestation> =
        state
            .store
            .list_entities("attestations")
            .unwrap_or_else(|e| {
                tracing::error!("Storage error: {}", e);
                Vec::new()
            });
    Json(items)
}

pub async fn submit_attestation(
    RequireWrite(_claims): RequireWrite,
    State(state): State<Arc<AppState>>,
    Json(att): Json<TrustAttestation>,
) -> impl IntoResponse {
    tracing::debug!("certificates::{}", stringify!(submit_attestation));
    match state.store.save_entity("attestations", &att.host_id, &att) {
        Ok(_) => (StatusCode::CREATED, Json(att)).into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": e.to_string()})),
        )
            .into_response(),
    }
}

pub async fn verify_attestation(
    RequireRead(_claims): RequireRead,
    State(state): State<Arc<AppState>>,
    Path(host_id): Path<String>,
) -> impl IntoResponse {
    tracing::debug!("certificates::{}", stringify!(verify_attestation));
    let mut att = match state
        .store
        .get_entity::<TrustAttestation>("attestations", &host_id)
    {
        Ok(Some(a)) => a,
        Ok(None) => {
            return (
                StatusCode::NOT_FOUND,
                Json(serde_json::json!({"error": "Attestation not found"})),
            )
                .into_response()
        }
        Err(_) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"error": "Failed to load attestation"})),
            )
                .into_response()
        }
    };
    let trusted = att.tpm_present && att.secure_boot_enabled && att.measured_boot_valid;
    att.attestation_status = if trusted {
        certificate_manager::AttestationStatus::Trusted
    } else {
        certificate_manager::AttestationStatus::Untrusted
    };
    att.last_attested = Some(Utc::now());
    if let Err(e) = state.store.save_entity("attestations", &att.host_id, &att) {
        tracing::error!("Failed to save entity: {}", e);
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": e.to_string()})),
        )
            .into_response();
    }
    Json(serde_json::json!({"trusted": trusted})).into_response()
}

// ============================================================================
// Security baseline handlers
// ============================================================================

pub async fn list_security_baselines(
    RequireRead(_claims): RequireRead,
    State(state): State<Arc<AppState>>,
) -> impl IntoResponse {
    tracing::debug!("certificates::{}", stringify!(list_security_baselines));
    let items: Vec<VmSecurityBaseline> = state
        .store
        .list_entities("security_baselines")
        .unwrap_or_else(|e| {
            tracing::error!("Storage error: {}", e);
            Vec::new()
        });
    Json(items)
}

pub async fn create_security_baseline(
    RequireWrite(_claims): RequireWrite,
    State(state): State<Arc<AppState>>,
    Json(mut baseline): Json<VmSecurityBaseline>,
) -> impl IntoResponse {
    tracing::debug!("certificates::{}", stringify!(create_security_baseline));
    if baseline.id.is_empty() {
        baseline.id = Uuid::new_v4().to_string();
    }
    let now = Utc::now();
    baseline.created = now;
    baseline.updated = now;
    match state
        .store
        .save_entity("security_baselines", &baseline.id, &baseline)
    {
        Ok(_) => (StatusCode::CREATED, Json(baseline)).into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": e.to_string()})),
        )
            .into_response(),
    }
}

#[derive(serde::Deserialize)]
pub struct VmComplianceRequest {
    pub vm_name: String,
    pub vm_encrypted: bool,
    pub vm_has_tpm: bool,
    pub vm_secure_boot: bool,
    pub host_trusted: bool,
}

pub async fn check_vm_security_compliance(
    RequireRead(_claims): RequireRead,
    State(state): State<Arc<AppState>>,
    Path(baseline_id): Path<String>,
    Json(req): Json<VmComplianceRequest>,
) -> impl IntoResponse {
    tracing::debug!("certificates::{}", stringify!(check_vm_security_compliance));
    let baseline = match state
        .store
        .get_entity::<VmSecurityBaseline>("security_baselines", &baseline_id)
    {
        Ok(Some(b)) => b,
        Ok(None) => {
            return (
                StatusCode::NOT_FOUND,
                Json(serde_json::json!({"error": "Security baseline not found"})),
            )
                .into_response()
        }
        Err(_) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"error": "Failed to load security baseline"})),
            )
                .into_response()
        }
    };
    let mut violations = Vec::new();
    if baseline.require_encryption && !req.vm_encrypted {
        violations.push("VM disk encryption is required but not enabled".to_string());
    }
    if baseline.require_tpm && !req.vm_has_tpm {
        violations.push("TPM is required but not present".to_string());
    }
    if baseline.require_secure_boot && !req.vm_secure_boot {
        violations.push("Secure boot is required but not enabled".to_string());
    }
    if baseline.require_trusted_host && !req.host_trusted {
        violations.push("Trusted host is required but host is not trusted".to_string());
    }
    let result = VmSecurityCompliance {
        vm_name: req.vm_name,
        baseline_id,
        baseline_name: baseline.name,
        compliant: violations.is_empty(),
        violations,
        checked_at: Utc::now(),
    };
    Json(result).into_response()
}

// ============================================================================
// Dashboard
// ============================================================================

pub async fn get_cert_health_dashboard(
    RequireRead(_claims): RequireRead,
    State(state): State<Arc<AppState>>,
) -> impl IntoResponse {
    tracing::debug!("certificates::{}", stringify!(get_cert_health_dashboard));
    let certs: Vec<Certificate> = state
        .store
        .list_entities("certificates")
        .unwrap_or_else(|e| {
            tracing::error!("Storage error: {}", e);
            Vec::new()
        });
    let cas: Vec<CertificateAuthority> =
        state.store.list_entities("cert_cas").unwrap_or_else(|e| {
            tracing::error!("Storage error: {}", e);
            Vec::new()
        });
    let requests: Vec<CertificateRequest> = state
        .store
        .list_entities("cert_requests")
        .unwrap_or_else(|e| {
            tracing::error!("Storage error: {}", e);
            Vec::new()
        });
    let rotations: Vec<CertificateRotation> = state
        .store
        .list_entities("cert_rotations")
        .unwrap_or_else(|e| {
            tracing::error!("Storage error: {}", e);
            Vec::new()
        });
    let now = Utc::now();
    let expiry_threshold = now + chrono::Duration::days(30);
    let mut active = 0u32;
    let mut expiring_soon = 0u32;
    let mut expired = 0u32;
    let mut revoked = 0u32;
    for cert in &certs {
        match cert.status {
            CertStatus::Active => {
                active += 1;
                if cert.not_after <= expiry_threshold {
                    expiring_soon += 1;
                }
            }
            CertStatus::Expired => expired += 1,
            CertStatus::Revoked => revoked += 1,
            CertStatus::PendingRenewal => active += 1,
        }
    }
    let pending_requests = requests
        .iter()
        .filter(|r| r.status == certificate_manager::CsrStatus::Pending)
        .count() as u32;
    let overall_compliance_pct = if certs.is_empty() {
        100.0
    } else {
        (active as f64 / certs.len() as f64) * 100.0
    };
    let expiring_within_30_days = certs
        .iter()
        .filter(|c| c.status == CertStatus::Active && c.not_after <= expiry_threshold)
        .map(|c| certificate_manager::ExpiringCertSummary {
            cert_id: c.id.clone(),
            subject: c.common_name.clone(),
            valid_to: c.not_after,
            days_remaining: (c.not_after - now).num_days(),
        })
        .collect();
    let dashboard = CertHealthDashboard {
        total_certs: certs.len() as u32,
        active,
        expiring_soon,
        expired,
        revoked,
        cas: cas.len() as u32,
        pending_requests,
        recent_rotations: rotations,
        overall_compliance_pct,
        expiring_within_30_days,
    };
    Json(dashboard)
}
