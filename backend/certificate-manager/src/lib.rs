// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: LicenseRef-Zyvor-Production-1.0

use std::collections::HashMap;
use std::sync::{Arc, RwLock};

use anyhow::{bail, Result};
use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};

pub mod crypto;

// ---------------------------------------------------------------------------
// Data models – enums
// ---------------------------------------------------------------------------

/// Cryptographic key algorithm used for certificate key pairs.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum KeyAlgorithm {
    Rsa2048,
    Rsa4096,
    EcdsaP256,
    EcdsaP384,
    Ed25519,
}

/// Intended usage of a certificate.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CertificateUsage {
    Server,
    Client,
    Ca,
    CodeSigning,
}

/// Lifecycle status of a certificate.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CertStatus {
    Active,
    Expired,
    Revoked,
    PendingRenewal,
}

/// Type of certificate authority.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CaType {
    Internal,
    ExternalAcme,
    ExternalManual,
}

/// Status of a certificate signing request.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CsrStatus {
    Pending,
    Approved,
    Rejected,
    Issued,
}

/// Status of a certificate rotation operation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RotationStatus {
    Scheduled,
    InProgress,
    Completed,
    Failed,
}

/// Trust attestation status for a host.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AttestationStatus {
    Trusted,
    Untrusted,
    Unknown,
    PendingVerification,
}

// ---------------------------------------------------------------------------
// Data models – structs
// ---------------------------------------------------------------------------

/// A TLS/X.509 certificate managed by the system.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Certificate {
    pub id: String,
    pub common_name: String,
    pub subject_alt_names: Vec<String>,
    pub issuer: String,
    pub serial_number: String,
    pub not_before: DateTime<Utc>,
    pub not_after: DateTime<Utc>,
    pub fingerprint_sha256: String,
    pub key_algorithm: KeyAlgorithm,
    pub usage: CertificateUsage,
    pub status: CertStatus,
    pub auto_renew: bool,
    pub component: String,
    pub created: DateTime<Utc>,
    pub updated: DateTime<Utc>,
    /// PEM-encoded X.509 certificate (generated via rcgen).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cert_pem: Option<String>,
    /// PEM-encoded private key (PKCS#8, generated via rcgen).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub key_pem: Option<String>,
}

/// A certificate authority (internal or external).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CertificateAuthority {
    pub id: String,
    pub name: String,
    pub ca_type: CaType,
    pub root_cert_id: Option<String>,
    pub endpoint: Option<String>,
    pub email: Option<String>,
    pub auto_approve: bool,
    pub certificates_issued: u32,
    pub created: DateTime<Utc>,
    pub updated: DateTime<Utc>,
    /// PEM-encoded CA certificate (generated via rcgen for internal CAs).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ca_cert_pem: Option<String>,
    /// PEM-encoded CA private key (PKCS#8, generated via rcgen for internal CAs).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ca_key_pem: Option<String>,
    /// SHA-256 fingerprint of the CA certificate.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fingerprint_sha256: Option<String>,
}

/// A certificate signing request submitted to a CA.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CertificateRequest {
    pub id: String,
    pub common_name: String,
    pub subject_alt_names: Vec<String>,
    pub key_algorithm: KeyAlgorithm,
    pub usage: CertificateUsage,
    pub ca_id: String,
    pub validity_days: u32,
    pub auto_renew: bool,
    pub component: String,
    pub status: CsrStatus,
    pub requested_by: String,
    pub created: DateTime<Utc>,
}

/// A scheduled or completed certificate rotation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CertificateRotation {
    pub id: String,
    pub certificate_id: String,
    pub old_cert_fingerprint: String,
    pub new_cert_fingerprint: Option<String>,
    pub status: RotationStatus,
    pub scheduled_at: DateTime<Utc>,
    pub completed_at: Option<DateTime<Utc>>,
    pub error: Option<String>,
}

/// Trust chain information for a host.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrustChain {
    pub host_id: String,
    pub hostname: String,
    pub trusted_ca_ids: Vec<String>,
    pub client_cert_id: Option<String>,
    pub verified: bool,
    pub last_verified: Option<DateTime<Utc>>,
}

/// Aggregate health dashboard for the certificate infrastructure.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CertHealthDashboard {
    #[serde(rename = "total_certificates")]
    pub total_certs: u32,
    pub active: u32,
    pub expiring_soon: u32,
    pub expired: u32,
    pub revoked: u32,
    #[serde(rename = "ca_count")]
    pub cas: u32,
    pub pending_requests: u32,
    pub recent_rotations: Vec<CertificateRotation>,
    /// Percentage of certificates that are `Active` (vs. expired/revoked).
    /// `100.0` when there are no certificates at all — an empty inventory
    /// isn't "non-compliant".
    pub overall_compliance_pct: f64,
    pub expiring_within_30_days: Vec<ExpiringCertSummary>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExpiringCertSummary {
    pub cert_id: String,
    pub subject: String,
    pub valid_to: DateTime<Utc>,
    pub days_remaining: i64,
}

/// Hardware/firmware trust attestation for a host.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrustAttestation {
    pub host_id: String,
    pub hostname: String,
    pub tpm_present: bool,
    pub tpm_version: Option<String>,
    pub secure_boot_enabled: bool,
    pub measured_boot_valid: bool,
    pub platform_identity: Option<String>,
    pub attestation_status: AttestationStatus,
    pub last_attested: Option<DateTime<Utc>>,
    pub attestation_evidence: Option<String>,
}

/// A security baseline that VMs must comply with.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VmSecurityBaseline {
    pub id: String,
    pub name: String,
    pub description: Option<String>,
    pub require_encryption: bool,
    pub require_tpm: bool,
    pub require_secure_boot: bool,
    pub require_trusted_host: bool,
    pub allowed_host_ids: Option<Vec<String>>,
    pub created: DateTime<Utc>,
    pub updated: DateTime<Utc>,
}

/// Result of checking a VM against a security baseline.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VmSecurityCompliance {
    pub vm_name: String,
    pub baseline_id: String,
    pub baseline_name: String,
    pub compliant: bool,
    pub violations: Vec<String>,
    pub checked_at: DateTime<Utc>,
}

// ---------------------------------------------------------------------------
// Manager – internal state
// ---------------------------------------------------------------------------

/// Internal state protected by `Arc<RwLock<_>>`.
#[derive(Debug, Default)]
struct Inner {
    cas: HashMap<String, CertificateAuthority>,
    certificates: HashMap<String, Certificate>,
    requests: HashMap<String, CertificateRequest>,
    rotations: HashMap<String, CertificateRotation>,
    trust_chains: HashMap<String, TrustChain>,
    attestations: HashMap<String, TrustAttestation>,
    baselines: HashMap<String, VmSecurityBaseline>,
}

// ---------------------------------------------------------------------------
// Manager
// ---------------------------------------------------------------------------

/// Thread-safe manager for PKI, TLS certificate lifecycle, internal CA, and
/// integration with external certificate authorities.
#[derive(Debug, Clone)]
pub struct CertificateManager {
    inner: Arc<RwLock<Inner>>,
}

impl Default for CertificateManager {
    fn default() -> Self {
        Self::new()
    }
}

impl CertificateManager {
    /// Create a new, empty certificate manager.
    pub fn new() -> Self {
        Self {
            inner: Arc::new(RwLock::new(Inner::default())),
        }
    }

    // -- Certificate Authorities --------------------------------------------

    /// Register a new certificate authority.
    ///
    /// For internal CAs (`CaType::Internal`), a real self-signed X.509 CA
    /// certificate is generated using `rcgen` and stored in the returned
    /// [`CertificateAuthority`].
    pub fn create_ca(&self, mut ca: CertificateAuthority) -> Result<CertificateAuthority> {
        let mut inner = self.inner.write().unwrap_or_else(|e| e.into_inner());
        if inner.cas.contains_key(&ca.id) {
            bail!("Certificate authority '{}' already exists", ca.id);
        }

        // Generate a real CA certificate for internal CAs.
        if ca.ca_type == CaType::Internal && ca.ca_cert_pem.is_none() {
            let ca_output = crypto::generate_ca(&ca.name, 3650)?;
            ca.ca_cert_pem = Some(ca_output.cert_pem);
            ca.ca_key_pem = Some(ca_output.key_pem);
            ca.fingerprint_sha256 = Some(ca_output.fingerprint);
        }

        tracing::info!(
            "Creating certificate authority '{}' (type: {:?})",
            ca.name,
            ca.ca_type
        );
        inner.cas.insert(ca.id.clone(), ca.clone());
        Ok(ca)
    }

    /// Look up a certificate authority by ID.
    pub fn get_ca(&self, id: &str) -> Option<CertificateAuthority> {
        let inner = self.inner.read().unwrap_or_else(|e| e.into_inner());
        inner.cas.get(id).cloned()
    }

    /// List all registered certificate authorities.
    pub fn list_cas(&self) -> Vec<CertificateAuthority> {
        let inner = self.inner.read().unwrap_or_else(|e| e.into_inner());
        inner.cas.values().cloned().collect()
    }

    /// Delete a certificate authority. Fails if certificates have been issued
    /// by it and are still active.
    pub fn delete_ca(&self, id: &str) -> Result<()> {
        let mut inner = self.inner.write().unwrap_or_else(|e| e.into_inner());
        if !inner.cas.contains_key(id) {
            bail!("Certificate authority '{}' not found", id);
        }

        let active_certs = inner
            .certificates
            .values()
            .any(|c| c.issuer == id && c.status == CertStatus::Active);
        if active_certs {
            bail!(
                "Cannot delete CA '{}': active certificates still reference it",
                id
            );
        }

        inner.cas.remove(id);
        tracing::info!("Deleted certificate authority '{}'", id);
        Ok(())
    }

    // -- Certificates -------------------------------------------------------

    /// Issue a new certificate from a certificate request.
    ///
    /// When the CA has PEM material (internal CAs), a real X.509 certificate
    /// is generated using `rcgen`.  Otherwise a SHA-256 fingerprint is still
    /// computed from the certificate metadata so the value is always a real
    /// hash rather than a random UUID.
    pub fn issue_certificate(&self, req: CertificateRequest) -> Result<Certificate> {
        let mut inner = self.inner.write().unwrap_or_else(|e| e.into_inner());

        let ca = inner
            .cas
            .get_mut(&req.ca_id)
            .ok_or_else(|| anyhow::anyhow!("CA '{}' not found", req.ca_id))?;

        let now = Utc::now();
        let not_after = now + Duration::days(req.validity_days as i64);

        // Try to issue a real X.509 certificate if CA PEM material is available.
        let (fingerprint, serial, cert_pem, key_pem) = if let (Some(ca_cert), Some(ca_key)) =
            (ca.ca_cert_pem.clone(), ca.ca_key_pem.clone())
        {
            let output = crypto::issue_certificate(
                &req.common_name,
                &req.subject_alt_names,
                req.validity_days,
                &ca_cert,
                &ca_key,
            )?;
            (
                output.fingerprint,
                output.serial,
                Some(output.cert_pem),
                Some(output.key_pem),
            )
        } else {
            // Fallback: compute a real SHA-256 hash from certificate metadata.
            let data = format!(
                "{}:{}:{}",
                req.common_name,
                uuid::Uuid::new_v4(),
                now.to_rfc3339()
            );
            let fingerprint = crypto::compute_fingerprint(data.as_bytes());
            let serial = uuid::Uuid::new_v4().to_string();
            (fingerprint, serial, None, None)
        };

        let cert = Certificate {
            id: uuid::Uuid::new_v4().to_string(),
            common_name: req.common_name.clone(),
            subject_alt_names: req.subject_alt_names.clone(),
            issuer: req.ca_id.clone(),
            serial_number: serial,
            not_before: now,
            not_after,
            fingerprint_sha256: fingerprint,
            key_algorithm: req.key_algorithm.clone(),
            usage: req.usage.clone(),
            status: CertStatus::Active,
            auto_renew: req.auto_renew,
            component: req.component.clone(),
            created: now,
            updated: now,
            cert_pem,
            key_pem,
        };

        ca.certificates_issued += 1;

        tracing::info!(
            "Issued certificate '{}' for CN='{}' via CA '{}'",
            cert.id,
            cert.common_name,
            req.ca_id
        );
        inner.certificates.insert(cert.id.clone(), cert.clone());
        Ok(cert)
    }

    /// Look up a certificate by ID.
    pub fn get_certificate(&self, id: &str) -> Option<Certificate> {
        let inner = self.inner.read().unwrap_or_else(|e| e.into_inner());
        inner.certificates.get(id).cloned()
    }

    /// List certificates, optionally filtered by component name.
    pub fn list_certificates(&self, component: Option<&str>) -> Vec<Certificate> {
        let inner = self.inner.read().unwrap_or_else(|e| e.into_inner());
        inner
            .certificates
            .values()
            .filter(|c| match component {
                Some(comp) => c.component == comp,
                None => true,
            })
            .cloned()
            .collect()
    }

    /// Revoke an active certificate.
    pub fn revoke_certificate(&self, id: &str) -> Result<()> {
        let mut inner = self.inner.write().unwrap_or_else(|e| e.into_inner());
        let cert = inner
            .certificates
            .get_mut(id)
            .ok_or_else(|| anyhow::anyhow!("Certificate '{}' not found", id))?;

        if cert.status == CertStatus::Revoked {
            bail!("Certificate '{}' is already revoked", id);
        }

        cert.status = CertStatus::Revoked;
        cert.updated = Utc::now();
        tracing::info!("Revoked certificate '{}'", id);
        Ok(())
    }

    /// Renew an existing certificate. Creates a new certificate with the same
    /// parameters and marks the old one as expired.
    ///
    /// If the issuing CA has PEM material, a new real X.509 certificate is
    /// generated.  Otherwise a SHA-256 fingerprint is computed from metadata.
    pub fn renew_certificate(&self, id: &str) -> Result<Certificate> {
        let mut inner = self.inner.write().unwrap_or_else(|e| e.into_inner());

        let old_cert = inner
            .certificates
            .get(id)
            .ok_or_else(|| anyhow::anyhow!("Certificate '{}' not found", id))?
            .clone();

        if old_cert.status == CertStatus::Revoked {
            bail!("Cannot renew revoked certificate '{}'", id);
        }

        let now = Utc::now();
        let validity = old_cert.not_after - old_cert.not_before;
        let validity_days = validity.num_days().max(1) as u32;

        // Try to issue a real X.509 certificate if CA PEM material is available.
        let ca_pem = inner.cas.get(&old_cert.issuer).and_then(|ca| {
            ca.ca_cert_pem
                .as_ref()
                .zip(ca.ca_key_pem.as_ref())
                .map(|(c, k)| (c.clone(), k.clone()))
        });

        let (fingerprint, serial, cert_pem, key_pem) = if let Some((ca_cert, ca_key)) = ca_pem {
            let output = crypto::issue_certificate(
                &old_cert.common_name,
                &old_cert.subject_alt_names,
                validity_days,
                &ca_cert,
                &ca_key,
            )?;
            (
                output.fingerprint,
                output.serial,
                Some(output.cert_pem),
                Some(output.key_pem),
            )
        } else {
            // Fallback: compute a real SHA-256 hash from certificate metadata.
            let data = format!(
                "{}:{}:{}",
                old_cert.common_name,
                uuid::Uuid::new_v4(),
                now.to_rfc3339()
            );
            let fingerprint = crypto::compute_fingerprint(data.as_bytes());
            let serial = uuid::Uuid::new_v4().to_string();
            (fingerprint, serial, None, None)
        };

        let new_cert = Certificate {
            id: uuid::Uuid::new_v4().to_string(),
            common_name: old_cert.common_name.clone(),
            subject_alt_names: old_cert.subject_alt_names.clone(),
            issuer: old_cert.issuer.clone(),
            serial_number: serial,
            not_before: now,
            not_after: now + validity,
            fingerprint_sha256: fingerprint,
            key_algorithm: old_cert.key_algorithm.clone(),
            usage: old_cert.usage.clone(),
            status: CertStatus::Active,
            auto_renew: old_cert.auto_renew,
            component: old_cert.component.clone(),
            created: now,
            updated: now,
            cert_pem,
            key_pem,
        };

        // Mark old certificate as expired.
        let old = inner.certificates.get_mut(id).unwrap();
        old.status = CertStatus::Expired;
        old.updated = now;

        // Increment CA issued count.
        if let Some(ca) = inner.cas.get_mut(&new_cert.issuer) {
            ca.certificates_issued += 1;
        }

        tracing::info!("Renewed certificate '{}' -> '{}'", id, new_cert.id);
        inner
            .certificates
            .insert(new_cert.id.clone(), new_cert.clone());
        Ok(new_cert)
    }

    /// Return all certificates expiring within the given number of days.
    pub fn check_expiring_certificates(&self, days: u32) -> Vec<Certificate> {
        let inner = self.inner.read().unwrap_or_else(|e| e.into_inner());
        let threshold = Utc::now() + Duration::days(days as i64);
        inner
            .certificates
            .values()
            .filter(|c| c.status == CertStatus::Active && c.not_after <= threshold)
            .cloned()
            .collect()
    }

    // -- Certificate Requests -----------------------------------------------

    /// Submit a new certificate signing request.
    pub fn submit_request(&self, mut req: CertificateRequest) -> Result<CertificateRequest> {
        let mut inner = self.inner.write().unwrap_or_else(|e| e.into_inner());

        if !inner.cas.contains_key(&req.ca_id) {
            bail!("CA '{}' not found", req.ca_id);
        }

        req.status = CsrStatus::Pending;
        req.created = Utc::now();
        if req.id.is_empty() {
            req.id = uuid::Uuid::new_v4().to_string();
        }

        tracing::info!(
            "Submitted certificate request '{}' for CN='{}'",
            req.id,
            req.common_name
        );
        inner.requests.insert(req.id.clone(), req.clone());
        Ok(req)
    }

    /// Approve a pending certificate request and issue the certificate.
    pub fn approve_request(&self, id: &str) -> Result<Certificate> {
        let req = {
            let mut inner = self.inner.write().unwrap_or_else(|e| e.into_inner());
            let req = inner
                .requests
                .get_mut(id)
                .ok_or_else(|| anyhow::anyhow!("Certificate request '{}' not found", id))?;

            if req.status != CsrStatus::Pending {
                bail!(
                    "Cannot approve request '{}': status is {:?}",
                    id,
                    req.status
                );
            }

            req.status = CsrStatus::Approved;
            req.clone()
        };

        // Issue the certificate from the approved request.
        let cert = self.issue_certificate(req.clone())?;

        // Mark the request as issued.
        let mut inner = self.inner.write().unwrap_or_else(|e| e.into_inner());
        if let Some(r) = inner.requests.get_mut(id) {
            r.status = CsrStatus::Issued;
        }

        tracing::info!(
            "Approved request '{}' and issued certificate '{}'",
            id,
            cert.id
        );
        Ok(cert)
    }

    /// Reject a pending certificate request.
    pub fn reject_request(&self, id: &str) -> Result<()> {
        let mut inner = self.inner.write().unwrap_or_else(|e| e.into_inner());
        let req = inner
            .requests
            .get_mut(id)
            .ok_or_else(|| anyhow::anyhow!("Certificate request '{}' not found", id))?;

        if req.status != CsrStatus::Pending {
            bail!("Cannot reject request '{}': status is {:?}", id, req.status);
        }

        req.status = CsrStatus::Rejected;
        tracing::info!("Rejected certificate request '{}'", id);
        Ok(())
    }

    /// List certificate requests, optionally filtered by status.
    pub fn list_requests(&self, status: Option<CsrStatus>) -> Vec<CertificateRequest> {
        let inner = self.inner.read().unwrap_or_else(|e| e.into_inner());
        inner
            .requests
            .values()
            .filter(|r| match &status {
                Some(s) => r.status == *s,
                None => true,
            })
            .cloned()
            .collect()
    }

    // -- Rotation -----------------------------------------------------------

    /// Schedule a certificate rotation for a future time.
    pub fn schedule_rotation(
        &self,
        cert_id: &str,
        scheduled_at: DateTime<Utc>,
    ) -> Result<CertificateRotation> {
        let mut inner = self.inner.write().unwrap_or_else(|e| e.into_inner());

        let cert = inner
            .certificates
            .get(cert_id)
            .ok_or_else(|| anyhow::anyhow!("Certificate '{}' not found", cert_id))?;

        let rotation = CertificateRotation {
            id: uuid::Uuid::new_v4().to_string(),
            certificate_id: cert_id.to_string(),
            old_cert_fingerprint: cert.fingerprint_sha256.clone(),
            new_cert_fingerprint: None,
            status: RotationStatus::Scheduled,
            scheduled_at,
            completed_at: None,
            error: None,
        };

        tracing::info!(
            "Scheduled rotation '{}' for certificate '{}'",
            rotation.id,
            cert_id
        );
        inner
            .rotations
            .insert(rotation.id.clone(), rotation.clone());
        Ok(rotation)
    }

    /// Execute a previously scheduled rotation: renew the certificate and
    /// record the result.
    pub fn execute_rotation(&self, rotation_id: &str) -> Result<Certificate> {
        let cert_id = {
            let mut inner = self.inner.write().unwrap_or_else(|e| e.into_inner());
            let rotation = inner
                .rotations
                .get_mut(rotation_id)
                .ok_or_else(|| anyhow::anyhow!("Rotation '{}' not found", rotation_id))?;

            if rotation.status != RotationStatus::Scheduled {
                bail!(
                    "Rotation '{}' is not in scheduled state (status: {:?})",
                    rotation_id,
                    rotation.status
                );
            }

            rotation.status = RotationStatus::InProgress;
            rotation.certificate_id.clone()
        };

        // Renew the certificate.
        let new_cert = match self.renew_certificate(&cert_id) {
            Ok(cert) => cert,
            Err(e) => {
                let mut inner = self.inner.write().unwrap_or_else(|e| e.into_inner());
                if let Some(rot) = inner.rotations.get_mut(rotation_id) {
                    rot.status = RotationStatus::Failed;
                    rot.error = Some(e.to_string());
                    rot.completed_at = Some(Utc::now());
                }
                return Err(e);
            }
        };

        // Mark rotation as completed.
        let mut inner = self.inner.write().unwrap_or_else(|e| e.into_inner());
        if let Some(rot) = inner.rotations.get_mut(rotation_id) {
            rot.status = RotationStatus::Completed;
            rot.new_cert_fingerprint = Some(new_cert.fingerprint_sha256.clone());
            rot.completed_at = Some(Utc::now());
        }

        tracing::info!(
            "Completed rotation '{}': new certificate '{}'",
            rotation_id,
            new_cert.id
        );
        Ok(new_cert)
    }

    /// List all rotation records.
    pub fn list_rotations(&self) -> Vec<CertificateRotation> {
        let inner = self.inner.read().unwrap_or_else(|e| e.into_inner());
        inner.rotations.values().cloned().collect()
    }

    // -- Trust Chains -------------------------------------------------------

    /// Register or update a host trust chain.
    pub fn register_host_trust(&self, trust: TrustChain) -> Result<()> {
        let inner_read = self.inner.read().unwrap_or_else(|e| e.into_inner());
        for ca_id in &trust.trusted_ca_ids {
            if !inner_read.cas.contains_key(ca_id) {
                drop(inner_read);
                bail!("CA '{}' referenced in trust chain not found", ca_id);
            }
        }
        drop(inner_read);

        let mut inner = self.inner.write().unwrap_or_else(|e| e.into_inner());
        tracing::info!(
            "Registered trust chain for host '{}' ({})",
            trust.host_id,
            trust.hostname
        );
        inner.trust_chains.insert(trust.host_id.clone(), trust);
        Ok(())
    }

    /// Verify a host's trust chain. Checks that all referenced CAs exist and
    /// the client certificate (if any) is active.
    pub fn verify_host_trust(&self, host_id: &str) -> Result<bool> {
        let mut inner = self.inner.write().unwrap_or_else(|e| e.into_inner());

        let trust = inner
            .trust_chains
            .get(host_id)
            .ok_or_else(|| anyhow::anyhow!("No trust chain for host '{}'", host_id))?
            .clone();

        // Check all CAs still exist.
        for ca_id in &trust.trusted_ca_ids {
            if !inner.cas.contains_key(ca_id) {
                let t = inner.trust_chains.get_mut(host_id).unwrap();
                t.verified = false;
                t.last_verified = Some(Utc::now());
                return Ok(false);
            }
        }

        // If a client cert is specified, verify it is active.
        if let Some(ref cert_id) = trust.client_cert_id {
            match inner.certificates.get(cert_id) {
                Some(cert) if cert.status == CertStatus::Active => {}
                _ => {
                    let t = inner.trust_chains.get_mut(host_id).unwrap();
                    t.verified = false;
                    t.last_verified = Some(Utc::now());
                    return Ok(false);
                }
            }
        }

        let t = inner.trust_chains.get_mut(host_id).unwrap();
        t.verified = true;
        t.last_verified = Some(Utc::now());
        tracing::info!("Host '{}' trust chain verified", host_id);
        Ok(true)
    }

    /// List all registered trust chains.
    pub fn list_trust_chains(&self) -> Vec<TrustChain> {
        let inner = self.inner.read().unwrap_or_else(|e| e.into_inner());
        inner.trust_chains.values().cloned().collect()
    }

    // -- Attestation --------------------------------------------------------

    /// Submit a trust attestation for a host.
    pub fn submit_attestation(&self, attestation: TrustAttestation) -> Result<TrustAttestation> {
        let mut inner = self.inner.write().unwrap_or_else(|e| e.into_inner());
        tracing::info!(
            "Submitted attestation for host '{}' ({})",
            attestation.host_id,
            attestation.hostname
        );
        inner
            .attestations
            .insert(attestation.host_id.clone(), attestation.clone());
        Ok(attestation)
    }

    /// Look up a host attestation by host ID.
    pub fn get_attestation(&self, host_id: &str) -> Option<TrustAttestation> {
        let inner = self.inner.read().unwrap_or_else(|e| e.into_inner());
        inner.attestations.get(host_id).cloned()
    }

    /// List all attestations.
    pub fn list_attestations(&self) -> Vec<TrustAttestation> {
        let inner = self.inner.read().unwrap_or_else(|e| e.into_inner());
        inner.attestations.values().cloned().collect()
    }

    /// Verify a host attestation. The host is considered trusted when TPM is
    /// present, secure boot is enabled, and measured boot is valid.
    pub fn verify_attestation(&self, host_id: &str) -> Result<bool> {
        let mut inner = self.inner.write().unwrap_or_else(|e| e.into_inner());
        let att = inner
            .attestations
            .get_mut(host_id)
            .ok_or_else(|| anyhow::anyhow!("No attestation for host '{}'", host_id))?;

        let trusted = att.tpm_present && att.secure_boot_enabled && att.measured_boot_valid;

        att.attestation_status = if trusted {
            AttestationStatus::Trusted
        } else {
            AttestationStatus::Untrusted
        };
        att.last_attested = Some(Utc::now());

        tracing::info!("Attestation for host '{}': trusted={}", host_id, trusted);
        Ok(trusted)
    }

    // -- VM Security Baselines ----------------------------------------------

    /// Create a new VM security baseline.
    pub fn create_security_baseline(
        &self,
        baseline: VmSecurityBaseline,
    ) -> Result<VmSecurityBaseline> {
        let mut inner = self.inner.write().unwrap_or_else(|e| e.into_inner());
        if inner.baselines.contains_key(&baseline.id) {
            bail!("Security baseline '{}' already exists", baseline.id);
        }
        tracing::info!("Created security baseline '{}'", baseline.name);
        inner
            .baselines
            .insert(baseline.id.clone(), baseline.clone());
        Ok(baseline)
    }

    /// Look up a security baseline by ID.
    pub fn get_security_baseline(&self, id: &str) -> Option<VmSecurityBaseline> {
        let inner = self.inner.read().unwrap_or_else(|e| e.into_inner());
        inner.baselines.get(id).cloned()
    }

    /// List all security baselines.
    pub fn list_security_baselines(&self) -> Vec<VmSecurityBaseline> {
        let inner = self.inner.read().unwrap_or_else(|e| e.into_inner());
        inner.baselines.values().cloned().collect()
    }

    /// Delete a security baseline.
    pub fn delete_security_baseline(&self, id: &str) -> Result<()> {
        let mut inner = self.inner.write().unwrap_or_else(|e| e.into_inner());
        if !inner.baselines.contains_key(id) {
            bail!("Security baseline '{}' not found", id);
        }
        inner.baselines.remove(id);
        tracing::info!("Deleted security baseline '{}'", id);
        Ok(())
    }

    /// Check a VM against a security baseline and return compliance results.
    pub fn check_vm_compliance(
        &self,
        vm_name: &str,
        baseline_id: &str,
        vm_encrypted: bool,
        vm_has_tpm: bool,
        vm_secure_boot: bool,
        host_trusted: bool,
    ) -> VmSecurityCompliance {
        let inner = self.inner.read().unwrap_or_else(|e| e.into_inner());

        let baseline = match inner.baselines.get(baseline_id) {
            Some(b) => b.clone(),
            None => {
                return VmSecurityCompliance {
                    vm_name: vm_name.to_string(),
                    baseline_id: baseline_id.to_string(),
                    baseline_name: "unknown".to_string(),
                    compliant: false,
                    violations: vec![format!("Baseline '{}' not found", baseline_id)],
                    checked_at: Utc::now(),
                };
            }
        };

        let mut violations = Vec::new();

        if baseline.require_encryption && !vm_encrypted {
            violations.push("VM disk encryption is required but not enabled".to_string());
        }
        if baseline.require_tpm && !vm_has_tpm {
            violations.push("TPM is required but not present".to_string());
        }
        if baseline.require_secure_boot && !vm_secure_boot {
            violations.push("Secure boot is required but not enabled".to_string());
        }
        if baseline.require_trusted_host && !host_trusted {
            violations.push("Trusted host is required but host is not trusted".to_string());
        }

        VmSecurityCompliance {
            vm_name: vm_name.to_string(),
            baseline_id: baseline_id.to_string(),
            baseline_name: baseline.name.clone(),
            compliant: violations.is_empty(),
            violations,
            checked_at: Utc::now(),
        }
    }

    // -- Dashboard ----------------------------------------------------------

    /// Build an aggregate health dashboard for the certificate infrastructure.
    pub fn get_health_dashboard(&self) -> CertHealthDashboard {
        let inner = self.inner.read().unwrap_or_else(|e| e.into_inner());
        let now = Utc::now();
        let expiry_threshold = now + Duration::days(30);

        let mut active = 0u32;
        let mut expiring_soon = 0u32;
        let mut expired = 0u32;
        let mut revoked = 0u32;

        for cert in inner.certificates.values() {
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

        let pending_requests = inner
            .requests
            .values()
            .filter(|r| r.status == CsrStatus::Pending)
            .count() as u32;

        let recent_rotations: Vec<CertificateRotation> =
            inner.rotations.values().cloned().collect();

        let overall_compliance_pct = if inner.certificates.is_empty() {
            100.0
        } else {
            (active as f64 / inner.certificates.len() as f64) * 100.0
        };
        let expiring_within_30_days = inner
            .certificates
            .values()
            .filter(|c| c.status == CertStatus::Active && c.not_after <= expiry_threshold)
            .map(|c| ExpiringCertSummary {
                cert_id: c.id.clone(),
                subject: c.common_name.clone(),
                valid_to: c.not_after,
                days_remaining: (c.not_after - now).num_days(),
            })
            .collect();

        CertHealthDashboard {
            total_certs: inner.certificates.len() as u32,
            active,
            expiring_soon,
            expired,
            revoked,
            cas: inner.cas.len() as u32,
            pending_requests,
            recent_rotations,
            overall_compliance_pct,
            expiring_within_30_days,
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    /// Helper: create an internal CA with deterministic IDs.
    fn make_ca(id: &str, name: &str) -> CertificateAuthority {
        let now = Utc::now();
        CertificateAuthority {
            id: id.to_string(),
            name: name.to_string(),
            ca_type: CaType::Internal,
            root_cert_id: None,
            endpoint: None,
            email: None,
            auto_approve: false,
            certificates_issued: 0,
            created: now,
            updated: now,
            ca_cert_pem: None,
            ca_key_pem: None,
            fingerprint_sha256: None,
        }
    }

    /// Helper: create a minimal certificate request.
    fn make_request(cn: &str, ca_id: &str, component: &str) -> CertificateRequest {
        CertificateRequest {
            id: String::new(),
            common_name: cn.to_string(),
            subject_alt_names: vec![cn.to_string()],
            key_algorithm: KeyAlgorithm::EcdsaP256,
            usage: CertificateUsage::Server,
            ca_id: ca_id.to_string(),
            validity_days: 365,
            auto_renew: false,
            component: component.to_string(),
            status: CsrStatus::Pending,
            requested_by: "admin".to_string(),
            created: Utc::now(),
        }
    }

    /// Helper: create a security baseline.
    fn make_baseline(id: &str, name: &str) -> VmSecurityBaseline {
        let now = Utc::now();
        VmSecurityBaseline {
            id: id.to_string(),
            name: name.to_string(),
            description: None,
            require_encryption: true,
            require_tpm: true,
            require_secure_boot: true,
            require_trusted_host: true,
            allowed_host_ids: None,
            created: now,
            updated: now,
        }
    }

    /// Helper: create a trust attestation.
    fn make_attestation(
        host_id: &str,
        hostname: &str,
        tpm: bool,
        secure_boot: bool,
        measured_boot: bool,
    ) -> TrustAttestation {
        TrustAttestation {
            host_id: host_id.to_string(),
            hostname: hostname.to_string(),
            tpm_present: tpm,
            tpm_version: if tpm { Some("2.0".to_string()) } else { None },
            secure_boot_enabled: secure_boot,
            measured_boot_valid: measured_boot,
            platform_identity: None,
            attestation_status: AttestationStatus::PendingVerification,
            last_attested: None,
            attestation_evidence: None,
        }
    }

    // -- CA tests -----------------------------------------------------------

    #[test]
    fn test_create_and_get_ca() {
        let mgr = CertificateManager::new();
        let ca = make_ca("ca-1", "Internal Root CA");
        let created = mgr.create_ca(ca).unwrap();
        assert_eq!(created.id, "ca-1");

        let fetched = mgr.get_ca("ca-1").unwrap();
        assert_eq!(fetched.name, "Internal Root CA");
        assert_eq!(fetched.ca_type, CaType::Internal);
    }

    #[test]
    fn test_create_duplicate_ca_fails() {
        let mgr = CertificateManager::new();
        mgr.create_ca(make_ca("ca-1", "First")).unwrap();
        assert!(mgr.create_ca(make_ca("ca-1", "Second")).is_err());
    }

    // -- Certificate issuance tests -----------------------------------------

    #[test]
    fn test_issue_certificate() {
        let mgr = CertificateManager::new();
        mgr.create_ca(make_ca("ca-1", "Root CA")).unwrap();

        let req = make_request("zyvor-fabricd.local", "ca-1", "zyvor-fabricd");
        let cert = mgr.issue_certificate(req).unwrap();

        assert_eq!(cert.common_name, "zyvor-fabricd.local");
        assert_eq!(cert.status, CertStatus::Active);
        assert_eq!(cert.component, "zyvor-fabricd");
        assert!(cert.fingerprint_sha256.starts_with("sha256:"));

        // CA issued count should be incremented.
        let ca = mgr.get_ca("ca-1").unwrap();
        assert_eq!(ca.certificates_issued, 1);
    }

    // -- Expiration check tests ---------------------------------------------

    #[test]
    fn test_check_expiring_certificates() {
        let mgr = CertificateManager::new();
        mgr.create_ca(make_ca("ca-1", "Root CA")).unwrap();

        // Issue a certificate with 10 day validity (expires within 30 days).
        let mut req = make_request("short-lived.local", "ca-1", "host-agent");
        req.validity_days = 10;
        let cert = mgr.issue_certificate(req).unwrap();

        // Issue another certificate with 365 day validity.
        let long_req = make_request("long-lived.local", "ca-1", "controller");
        mgr.issue_certificate(long_req).unwrap();

        let expiring = mgr.check_expiring_certificates(30);
        assert_eq!(expiring.len(), 1);
        assert_eq!(expiring[0].id, cert.id);
    }

    // -- Rotation scheduling tests ------------------------------------------

    #[test]
    fn test_schedule_and_execute_rotation() {
        let mgr = CertificateManager::new();
        mgr.create_ca(make_ca("ca-1", "Root CA")).unwrap();

        let req = make_request("rotate-me.local", "ca-1", "zyvor-fabricd");
        let cert = mgr.issue_certificate(req).unwrap();

        let rotation = mgr.schedule_rotation(&cert.id, Utc::now()).unwrap();
        assert_eq!(rotation.status, RotationStatus::Scheduled);
        assert_eq!(rotation.old_cert_fingerprint, cert.fingerprint_sha256);

        let new_cert = mgr.execute_rotation(&rotation.id).unwrap();
        assert_eq!(new_cert.status, CertStatus::Active);
        assert_ne!(new_cert.id, cert.id);

        // Old cert should be expired.
        let old = mgr.get_certificate(&cert.id).unwrap();
        assert_eq!(old.status, CertStatus::Expired);

        // Rotation should be completed with new fingerprint.
        let rotations = mgr.list_rotations();
        assert_eq!(rotations.len(), 1);
        assert_eq!(rotations[0].status, RotationStatus::Completed);
        assert!(rotations[0].new_cert_fingerprint.is_some());
    }

    // -- Request approval/rejection tests -----------------------------------

    #[test]
    fn test_submit_and_approve_request() {
        let mgr = CertificateManager::new();
        mgr.create_ca(make_ca("ca-1", "Root CA")).unwrap();

        let req = make_request("approved.local", "ca-1", "host-agent");
        let submitted = mgr.submit_request(req).unwrap();
        assert_eq!(submitted.status, CsrStatus::Pending);

        let cert = mgr.approve_request(&submitted.id).unwrap();
        assert_eq!(cert.common_name, "approved.local");
        assert_eq!(cert.status, CertStatus::Active);

        // Request should be marked as issued.
        let requests = mgr.list_requests(Some(CsrStatus::Issued));
        assert_eq!(requests.len(), 1);
    }

    #[test]
    fn test_submit_and_reject_request() {
        let mgr = CertificateManager::new();
        mgr.create_ca(make_ca("ca-1", "Root CA")).unwrap();

        let req = make_request("rejected.local", "ca-1", "controller");
        let submitted = mgr.submit_request(req).unwrap();

        mgr.reject_request(&submitted.id).unwrap();

        let requests = mgr.list_requests(Some(CsrStatus::Rejected));
        assert_eq!(requests.len(), 1);
        assert_eq!(requests[0].common_name, "rejected.local");
    }

    // -- Host trust verification tests --------------------------------------

    #[test]
    fn test_host_trust_verification() {
        let mgr = CertificateManager::new();
        mgr.create_ca(make_ca("ca-1", "Root CA")).unwrap();

        // Issue a client certificate.
        let mut req = make_request("host-1.local", "ca-1", "host-agent");
        req.usage = CertificateUsage::Client;
        let client_cert = mgr.issue_certificate(req).unwrap();

        let trust = TrustChain {
            host_id: "host-1".to_string(),
            hostname: "host-1.local".to_string(),
            trusted_ca_ids: vec!["ca-1".to_string()],
            client_cert_id: Some(client_cert.id.clone()),
            verified: false,
            last_verified: None,
        };
        mgr.register_host_trust(trust).unwrap();

        let verified = mgr.verify_host_trust("host-1").unwrap();
        assert!(verified);

        // After verification, the trust chain should be marked as verified.
        let chains = mgr.list_trust_chains();
        assert_eq!(chains.len(), 1);
        assert!(chains[0].verified);
        assert!(chains[0].last_verified.is_some());
    }

    // -- Attestation tests --------------------------------------------------

    #[test]
    fn test_submit_and_verify_attestation_trusted() {
        let mgr = CertificateManager::new();

        let att = make_attestation("host-1", "host-1.local", true, true, true);
        mgr.submit_attestation(att).unwrap();

        let trusted = mgr.verify_attestation("host-1").unwrap();
        assert!(trusted);

        let fetched = mgr.get_attestation("host-1").unwrap();
        assert_eq!(fetched.attestation_status, AttestationStatus::Trusted);
    }

    #[test]
    fn test_verify_attestation_untrusted() {
        let mgr = CertificateManager::new();

        // No TPM present.
        let att = make_attestation("host-2", "host-2.local", false, true, true);
        mgr.submit_attestation(att).unwrap();

        let trusted = mgr.verify_attestation("host-2").unwrap();
        assert!(!trusted);

        let fetched = mgr.get_attestation("host-2").unwrap();
        assert_eq!(fetched.attestation_status, AttestationStatus::Untrusted);
    }

    // -- VM Security compliance tests ---------------------------------------

    #[test]
    fn test_vm_compliance_pass() {
        let mgr = CertificateManager::new();
        mgr.create_security_baseline(make_baseline("bl-1", "Strict"))
            .unwrap();

        let result = mgr.check_vm_compliance(
            "vm-secure",
            "bl-1",
            true, // encrypted
            true, // tpm
            true, // secure boot
            true, // trusted host
        );
        assert!(result.compliant);
        assert!(result.violations.is_empty());
        assert_eq!(result.baseline_name, "Strict");
    }

    #[test]
    fn test_vm_compliance_fail() {
        let mgr = CertificateManager::new();
        mgr.create_security_baseline(make_baseline("bl-1", "Strict"))
            .unwrap();

        let result = mgr.check_vm_compliance(
            "vm-insecure",
            "bl-1",
            false, // not encrypted
            false, // no tpm
            true,  // secure boot ok
            false, // host not trusted
        );
        assert!(!result.compliant);
        assert_eq!(result.violations.len(), 3);
        assert!(result.violations.iter().any(|v| v.contains("encryption")));
        assert!(result.violations.iter().any(|v| v.contains("TPM")));
        assert!(result.violations.iter().any(|v| v.contains("host")));
    }

    // -- Dashboard test -----------------------------------------------------

    #[test]
    fn test_health_dashboard() {
        let mgr = CertificateManager::new();
        mgr.create_ca(make_ca("ca-1", "Root CA")).unwrap();
        mgr.create_ca(make_ca("ca-2", "Intermediate CA")).unwrap();

        // Issue certificates.
        let req1 = make_request("active.local", "ca-1", "zyvor-fabricd");
        mgr.issue_certificate(req1).unwrap();

        let mut req2 = make_request("expiring.local", "ca-1", "host-agent");
        req2.validity_days = 5; // expiring within 30 days
        let cert2 = mgr.issue_certificate(req2).unwrap();

        // Revoke one certificate.
        mgr.revoke_certificate(&cert2.id).unwrap();

        // Submit a pending request.
        let req3 = make_request("pending.local", "ca-1", "controller");
        mgr.submit_request(req3).unwrap();

        let dashboard = mgr.get_health_dashboard();
        assert_eq!(dashboard.total_certs, 2);
        assert_eq!(dashboard.cas, 2);
        assert_eq!(dashboard.revoked, 1);
        assert_eq!(dashboard.pending_requests, 1);
        // One active cert (active.local), which is not expiring soon (365 days).
        assert_eq!(dashboard.active, 1);
    }

    // -- Serde round-trip test ----------------------------------------------

    #[test]
    fn test_serde_round_trip() {
        let now = Utc::now();
        let cert = Certificate {
            id: "cert-1".to_string(),
            common_name: "test.local".to_string(),
            subject_alt_names: vec!["test.local".to_string(), "*.test.local".to_string()],
            issuer: "ca-1".to_string(),
            serial_number: "serial-123".to_string(),
            not_before: now,
            not_after: now + Duration::days(365),
            fingerprint_sha256: "sha256:abc123".to_string(),
            key_algorithm: KeyAlgorithm::Ed25519,
            usage: CertificateUsage::Server,
            status: CertStatus::Active,
            auto_renew: true,
            component: "zyvor-fabricd".to_string(),
            created: now,
            updated: now,
            cert_pem: None,
            key_pem: None,
        };
        let json = serde_json::to_string(&cert).unwrap();
        let de: Certificate = serde_json::from_str(&json).unwrap();
        assert_eq!(de.common_name, "test.local");
        assert_eq!(de.key_algorithm, KeyAlgorithm::Ed25519);
        assert_eq!(de.subject_alt_names.len(), 2);
        // Verify that None PEM fields are omitted from JSON.
        assert!(!json.contains("cert_pem"));
        assert!(!json.contains("key_pem"));
    }
}
