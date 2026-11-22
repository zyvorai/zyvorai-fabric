// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: LicenseRef-Zyvor-Production-2.0

//! Real X.509 certificate generation using the `rcgen` crate.

use anyhow::{Context, Result};
use rcgen::string::Ia5String;
use rcgen::{
    BasicConstraints, CertificateParams, DistinguishedName, DnType, IsCa, Issuer, KeyPair,
    KeyUsagePurpose, SanType,
};
use sha2::{Digest, Sha256};

pub struct CaOutput {
    pub cert_pem: String,
    pub key_pem: String,
    pub fingerprint: String,
}

pub struct CertOutput {
    pub cert_pem: String,
    pub key_pem: String,
    pub fingerprint: String,
    pub serial: String,
}

pub fn compute_fingerprint(der_bytes: &[u8]) -> String {
    let hash = Sha256::digest(der_bytes);
    format!("sha256:{}", hex::encode(hash))
}

pub fn generate_ca(common_name: &str, validity_days: u32) -> Result<CaOutput> {
    let mut params = CertificateParams::default();

    let mut dn = DistinguishedName::new();
    dn.push(DnType::CommonName, common_name);
    params.distinguished_name = dn;

    params.is_ca = IsCa::Ca(BasicConstraints::Unconstrained);
    params.key_usages = vec![KeyUsagePurpose::KeyCertSign, KeyUsagePurpose::CrlSign];

    let now = time::OffsetDateTime::now_utc();
    params.not_before = now;
    params.not_after = now
        .checked_add(time::Duration::days(validity_days as i64))
        .context("validity period overflow")?;

    let key_pair = KeyPair::generate().context("failed to generate CA key pair")?;
    let cert = params
        .self_signed(&key_pair)
        .context("failed to self-sign CA certificate")?;

    Ok(CaOutput {
        fingerprint: compute_fingerprint(cert.der().as_ref()),
        cert_pem: cert.pem(),
        key_pem: key_pair.serialize_pem(),
    })
}

pub fn issue_certificate(
    common_name: &str,
    san_names: &[String],
    validity_days: u32,
    ca_cert_pem: &str,
    ca_key_pem: &str,
) -> Result<CertOutput> {
    let mut params = CertificateParams::default();

    let mut dn = DistinguishedName::new();
    dn.push(DnType::CommonName, common_name);
    params.distinguished_name = dn;

    for name in san_names {
        let ia5 =
            Ia5String::try_from(name.as_str()).context(format!("invalid SAN name: {}", name))?;
        params.subject_alt_names.push(SanType::DnsName(ia5));
    }

    let now = time::OffsetDateTime::now_utc();
    params.not_before = now;
    params.not_after = now
        .checked_add(time::Duration::days(validity_days as i64))
        .context("validity period overflow")?;

    let ca_key_pair =
        KeyPair::from_pem(ca_key_pem).context("failed to parse CA private key PEM")?;
    let issuer = Issuer::from_ca_cert_pem(ca_cert_pem, ca_key_pair)
        .context("failed to parse CA certificate PEM")?;

    let key_pair = KeyPair::generate().context("failed to generate certificate key pair")?;
    let cert = params
        .signed_by(&key_pair, &issuer)
        .context("failed to sign certificate with CA")?;

    Ok(CertOutput {
        fingerprint: compute_fingerprint(cert.der().as_ref()),
        cert_pem: cert.pem(),
        key_pem: key_pair.serialize_pem(),
        serial: uuid::Uuid::new_v4().to_string(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_generate_ca() {
        let ca = generate_ca("Test Root CA", 365).unwrap();
        assert!(ca.cert_pem.contains("BEGIN CERTIFICATE"));
        assert!(ca.key_pem.contains("BEGIN PRIVATE KEY"));
        assert!(ca.fingerprint.starts_with("sha256:"));
        assert_eq!(ca.fingerprint.len(), 71);
    }

    #[test]
    fn test_issue_certificate() {
        let ca = generate_ca("Test CA", 365).unwrap();
        let cert = issue_certificate(
            "test.local",
            &["test.local".to_string(), "*.test.local".to_string()],
            90,
            &ca.cert_pem,
            &ca.key_pem,
        )
        .unwrap();

        assert!(cert.cert_pem.contains("BEGIN CERTIFICATE"));
        assert!(cert.key_pem.contains("BEGIN PRIVATE KEY"));
        assert!(cert.fingerprint.starts_with("sha256:"));
        assert_eq!(cert.fingerprint.len(), 71);
        assert!(!cert.serial.is_empty());
    }

    #[test]
    fn test_fingerprint_deterministic() {
        let data = b"some certificate bytes";
        assert_eq!(compute_fingerprint(data), compute_fingerprint(data));
    }

    #[test]
    fn test_different_certs_different_fingerprints() {
        let ca = generate_ca("Test CA", 365).unwrap();
        let cert1 = issue_certificate(
            "a.local",
            &["a.local".to_string()],
            90,
            &ca.cert_pem,
            &ca.key_pem,
        )
        .unwrap();
        let cert2 = issue_certificate(
            "b.local",
            &["b.local".to_string()],
            90,
            &ca.cert_pem,
            &ca.key_pem,
        )
        .unwrap();
        assert_ne!(cert1.fingerprint, cert2.fingerprint);
    }
}
