// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: LicenseRef-Zyvor-Production-1.0

//! How the gateway reaches an inference replica.
//!
//! The default is plain HTTP. HTTPS and client certificates are opt-in.

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BackendTransport {
    Http,
    Https,
    Mtls {
        cert: String,
        key: String,
        ca: String,
    },
}

fn filled(value: Option<&str>) -> Option<&str> {
    value.map(str::trim).filter(|value| !value.is_empty())
}

pub fn backend_transport(
    tls: bool,
    cert: Option<&str>,
    key: Option<&str>,
    ca: Option<&str>,
) -> Result<BackendTransport, &'static str> {
    match (tls, filled(cert), filled(key), filled(ca)) {
        (false, None, None, None) => Ok(BackendTransport::Http),
        (true, None, None, None) => Ok(BackendTransport::Https),
        (true, Some(cert), Some(key), Some(ca)) => Ok(BackendTransport::Mtls {
            cert: cert.to_string(),
            key: key.to_string(),
            ca: ca.to_string(),
        }),
        (false, Some(_), Some(_), Some(_)) => {
            Err("FLUXVM_AI_BACKEND_TLS=1 is required with backend client certificates")
        }
        _ => Err(
            "set FLUXVM_AI_BACKEND_CLIENT_CERT, FLUXVM_AI_BACKEND_CLIENT_KEY, and FLUXVM_AI_BACKEND_CA together",
        ),
    }
}

pub fn upstream_origin(transport: &BackendTransport, target: &str, path: &str) -> String {
    let scheme = match transport {
        BackendTransport::Http => "http",
        BackendTransport::Https | BackendTransport::Mtls { .. } => "https",
    };
    format!("{scheme}://{target}{path}")
}

pub fn websocket_origin(transport: &BackendTransport, target: &str, path: &str) -> String {
    let scheme = match transport {
        BackendTransport::Http => "ws",
        BackendTransport::Https | BackendTransport::Mtls { .. } => "wss",
    };
    format!("{scheme}://{target}{path}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn http_stays_the_default_and_mtls_needs_every_file() {
        assert_eq!(
            backend_transport(false, None, None, None).unwrap(),
            BackendTransport::Http
        );
        assert!(matches!(
            backend_transport(true, None, None, None).unwrap(),
            BackendTransport::Https
        ));
        let mtls = backend_transport(true, Some("c.pem"), Some("k.pem"), Some("ca.pem")).unwrap();
        assert!(matches!(mtls, BackendTransport::Mtls { .. }));
        assert!(backend_transport(true, Some("c.pem"), None, None).is_err());
        assert!(backend_transport(false, Some("c.pem"), Some("k.pem"), Some("ca.pem")).is_err());
        assert_eq!(
            upstream_origin(
                &BackendTransport::Http,
                "10.0.0.8:8000",
                "/v1/chat/completions"
            ),
            "http://10.0.0.8:8000/v1/chat/completions"
        );
        assert_eq!(
            websocket_origin(&BackendTransport::Http, "10.0.0.8:8000", "/v1/realtime"),
            "ws://10.0.0.8:8000/v1/realtime"
        );
        assert!(
            websocket_origin(&BackendTransport::Https, "10.0.0.8:8000", "/v1/realtime")
                .starts_with("wss://")
        );
        assert!(
            upstream_origin(&BackendTransport::Https, "10.0.0.8:8000", "/v1/models")
                .starts_with("https://")
        );
    }
}
