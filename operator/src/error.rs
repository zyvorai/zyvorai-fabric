// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: LicenseRef-Zyvor-Production-1.0

use thiserror::Error;

#[derive(Debug, Error)]
pub enum OperatorError {
    #[error("kubernetes API: {0}")]
    Kube(#[from] kube::Error),
    #[error("fabric API: {0}")]
    Http(#[from] reqwest::Error),
    #[error("{0}")]
    Other(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn other_variant_displays_its_message_verbatim() {
        let err = OperatorError::Other("VM spec missing an image".to_string());
        assert_eq!(err.to_string(), "VM spec missing an image");
    }

    #[test]
    fn kube_error_is_prefixed_and_wrapped_via_from() {
        let kube_err = kube::Error::Api(Box::new(kube::core::Status {
            status: Some(kube::core::response::StatusSummary::Failure),
            message: "virtualmachines.zyvor-fabricd.io \"web-01\" not found".to_string(),
            reason: "NotFound".to_string(),
            code: 404,
            ..Default::default()
        }));
        let err: OperatorError = kube_err.into();
        assert!(err.to_string().starts_with("kubernetes API: "));
        assert!(matches!(err, OperatorError::Kube(_)));
    }

    #[test]
    fn http_error_from_reqwest_is_prefixed() {
        // reqwest::Client::get with an invalid URL fails at request-build time
        // with a real reqwest::Error, no network access needed.
        let build_err = reqwest::Client::new().get("not a url").build().unwrap_err();
        let err: OperatorError = build_err.into();
        assert!(err.to_string().starts_with("fabric API: "));
        assert!(matches!(err, OperatorError::Http(_)));
    }
}
