// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: LicenseRef-Zyvor-Production-1.0

use anyhow::{anyhow, Result};
use totp_rs::{Algorithm, Builder, Secret};

/// Generate a new TOTP secret for a user.
pub fn generate_secret(username: &str, issuer: &str) -> Result<(String, String)> {
    let secret = Secret::generate();
    let totp = Builder::new()
        .with_algorithm(Algorithm::SHA1)
        .with_digits(6)
        .with_skew(1)
        .with_step_duration(30)
        .with_secret(secret.as_bytes().to_vec())
        .with_issuer(Some(issuer.to_string()))
        .with_account_name(username.to_string())
        .build()
        .map_err(|e| anyhow!("totp build: {e}"))?;

    let secret_base32 = totp.secret().to_base32();
    let otpauth_url = totp
        .to_url()
        .map_err(|e| anyhow!("totp url: {e}"))?
        .to_string();

    Ok((secret_base32, otpauth_url))
}

/// Verify a TOTP code against a stored secret.
pub fn verify_code(secret_base32: &str, code: &str) -> Result<bool> {
    let secret =
        Secret::try_from_base32(secret_base32).map_err(|e| anyhow!("invalid totp secret: {e}"))?;
    let totp = Builder::new()
        .with_algorithm(Algorithm::SHA1)
        .with_digits(6)
        .with_skew(1)
        .with_step_duration(30)
        .with_secret(secret.as_bytes().to_vec())
        .with_issuer(Some("zyvor-fabricd".to_string()))
        .with_account_name("user".to_string())
        .build()
        .map_err(|e| anyhow!("totp build: {e}"))?;

    Ok(totp.check_current(code).is_some())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_generate_and_verify() {
        let (secret, url) = generate_secret("testuser", "zyvor-fabricd").unwrap();
        assert!(!secret.is_empty());
        assert!(url.contains("otpauth://"));
        let _ = verify_code(&secret, "000000").unwrap();
    }
}
