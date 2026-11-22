// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: LicenseRef-Zyvor-Production-2.0

use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

use crate::models::{Backend, BackendHealth, HealthCheckType, Service};

/// Tracks and updates backend health state for all services.
pub struct HealthChecker {
    backends: Arc<RwLock<HashMap<String, Vec<Backend>>>>,
}

impl Default for HealthChecker {
    fn default() -> Self {
        Self::new()
    }
}

impl HealthChecker {
    pub fn new() -> Self {
        Self {
            backends: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Perform a TCP health check against the given address.
    pub async fn check_tcp(ip: &str, port: u16, timeout_secs: u64) -> bool {
        let addr = format!("{}:{}", ip, port);
        let timeout = std::time::Duration::from_secs(timeout_secs);

        matches!(
            tokio::time::timeout(timeout, tokio::net::TcpStream::connect(&addr)).await,
            Ok(Ok(_))
        )
    }

    /// Perform an HTTP health check via raw TCP.
    pub async fn check_http(
        ip: &str,
        port: u16,
        path: &str,
        expected_codes: &[u16],
        timeout_secs: u64,
    ) -> bool {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};

        let addr = format!("{}:{}", ip, port);
        let timeout = std::time::Duration::from_secs(timeout_secs);
        let path = if path.is_empty() { "/" } else { path };

        let result = tokio::time::timeout(timeout, async {
            let mut stream = tokio::net::TcpStream::connect(&addr).await?;
            let request = format!(
                "GET {} HTTP/1.1\r\nHost: {}\r\nConnection: close\r\n\r\n",
                path, ip
            );
            stream.write_all(request.as_bytes()).await?;

            let mut buf = vec![0u8; 1024];
            let n = stream.read(&mut buf).await?;
            let response = String::from_utf8_lossy(&buf[..n]);

            // Parse status code from "HTTP/1.x NNN ..."
            if let Some(status_line) = response.lines().next() {
                if let Some(code_str) = status_line.split_whitespace().nth(1) {
                    if let Ok(code) = code_str.parse::<u16>() {
                        let expected = if expected_codes.is_empty() {
                            vec![200]
                        } else {
                            expected_codes.to_vec()
                        };
                        return Ok::<bool, std::io::Error>(expected.contains(&code));
                    }
                }
            }
            Ok(false)
        })
        .await;

        matches!(result, Ok(Ok(true)))
    }

    /// Run health checks for all backends of a service and update their health state.
    pub async fn run_checks(&self, service: &Service) {
        let mut backends = self.backends.write().await;
        let service_backends = match backends.get_mut(&service.name) {
            Some(b) => b,
            None => return,
        };

        for backend in service_backends.iter_mut() {
            let is_healthy = match service.health_check.check_type {
                HealthCheckType::Tcp => {
                    let port = service
                        .ports
                        .first()
                        .map(|p| p.effective_target_port())
                        .unwrap_or(80);
                    Self::check_tcp(&backend.ip, port, service.health_check.timeout_secs).await
                }
                HealthCheckType::Http => {
                    let port = service
                        .ports
                        .first()
                        .map(|p| p.effective_target_port())
                        .unwrap_or(80);
                    Self::check_http(
                        &backend.ip,
                        port,
                        &service.health_check.http_path,
                        &service.health_check.expected_status_codes,
                        service.health_check.timeout_secs,
                    )
                    .await
                }
            };

            backend.last_check = Some(chrono::Utc::now());

            if is_healthy {
                backend.consecutive_successes += 1;
                backend.consecutive_failures = 0;

                if backend.consecutive_successes >= service.health_check.healthy_threshold {
                    backend.health = BackendHealth::Healthy;
                }
            } else {
                backend.consecutive_failures += 1;
                backend.consecutive_successes = 0;

                if backend.consecutive_failures >= service.health_check.unhealthy_threshold {
                    backend.health = BackendHealth::Unhealthy;
                }
            }
        }
    }

    /// Get backends that are currently healthy for a given service.
    pub async fn get_healthy_backends(&self, service_name: &str) -> Vec<Backend> {
        let backends = self.backends.read().await;
        backends
            .get(service_name)
            .map(|bs| {
                bs.iter()
                    .filter(|b| b.health == BackendHealth::Healthy)
                    .cloned()
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Get all backends for a given service.
    pub async fn get_all_backends(&self, service_name: &str) -> Vec<Backend> {
        let backends = self.backends.read().await;
        backends.get(service_name).cloned().unwrap_or_default()
    }

    /// Replace the backend list for a service.
    pub async fn update_backends(&self, service_name: &str, new_backends: Vec<Backend>) {
        let mut backends = self.backends.write().await;
        backends.insert(service_name.to_string(), new_backends);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_backend(name: &str, ip: &str) -> Backend {
        Backend {
            vm_name: name.to_string(),
            ip: ip.to_string(),
            health: BackendHealth::Unknown,
            consecutive_successes: 0,
            consecutive_failures: 0,
            last_check: None,
        }
    }

    #[tokio::test]
    async fn test_new_checker() {
        let checker = HealthChecker::new();
        let backends = checker.get_all_backends("test-svc").await;
        assert!(backends.is_empty());
    }

    #[tokio::test]
    async fn test_unknown_initial_state() {
        let checker = HealthChecker::new();
        let backend = make_backend("vm1", "10.0.0.5");
        checker.update_backends("svc1", vec![backend]).await;

        let all = checker.get_all_backends("svc1").await;
        assert_eq!(all.len(), 1);
        assert_eq!(all[0].health, BackendHealth::Unknown);
    }

    #[tokio::test]
    async fn test_get_healthy_filters() {
        let checker = HealthChecker::new();
        let healthy = Backend {
            vm_name: "vm1".to_string(),
            ip: "10.0.0.5".to_string(),
            health: BackendHealth::Healthy,
            consecutive_successes: 3,
            consecutive_failures: 0,
            last_check: None,
        };
        let unhealthy = Backend {
            vm_name: "vm2".to_string(),
            ip: "10.0.0.6".to_string(),
            health: BackendHealth::Unhealthy,
            consecutive_successes: 0,
            consecutive_failures: 3,
            last_check: None,
        };
        let unknown = make_backend("vm3", "10.0.0.7");

        checker
            .update_backends("svc1", vec![healthy, unhealthy, unknown])
            .await;

        let healthy_list = checker.get_healthy_backends("svc1").await;
        assert_eq!(healthy_list.len(), 1);
        assert_eq!(healthy_list[0].vm_name, "vm1");
    }

    #[tokio::test]
    async fn test_update_replaces() {
        let checker = HealthChecker::new();
        checker
            .update_backends("svc1", vec![make_backend("vm1", "10.0.0.5")])
            .await;
        assert_eq!(checker.get_all_backends("svc1").await.len(), 1);

        checker
            .update_backends(
                "svc1",
                vec![
                    make_backend("vm2", "10.0.0.6"),
                    make_backend("vm3", "10.0.0.7"),
                ],
            )
            .await;
        let backends = checker.get_all_backends("svc1").await;
        assert_eq!(backends.len(), 2);
        assert_eq!(backends[0].vm_name, "vm2");
    }

    #[tokio::test]
    async fn test_empty_backends() {
        let checker = HealthChecker::new();
        checker.update_backends("svc1", vec![]).await;
        let healthy = checker.get_healthy_backends("svc1").await;
        assert!(healthy.is_empty());
    }

    #[tokio::test]
    async fn test_consecutive_counts() {
        let checker = HealthChecker::new();
        let mut backend = make_backend("vm1", "10.0.0.5");
        backend.consecutive_successes = 5;
        backend.consecutive_failures = 0;
        backend.health = BackendHealth::Healthy;

        checker.update_backends("svc1", vec![backend]).await;
        let backends = checker.get_all_backends("svc1").await;
        assert_eq!(backends[0].consecutive_successes, 5);
        assert_eq!(backends[0].consecutive_failures, 0);
    }

    #[tokio::test]
    async fn test_healthy_tracking() {
        let checker = HealthChecker::new();
        let mut backend = make_backend("vm1", "10.0.0.5");
        backend.health = BackendHealth::Healthy;
        backend.consecutive_successes = 3;

        checker.update_backends("svc1", vec![backend]).await;
        let healthy = checker.get_healthy_backends("svc1").await;
        assert_eq!(healthy.len(), 1);
    }

    #[tokio::test]
    async fn test_unhealthy_tracking() {
        let checker = HealthChecker::new();
        let mut backend = make_backend("vm1", "10.0.0.5");
        backend.health = BackendHealth::Unhealthy;
        backend.consecutive_failures = 3;

        checker.update_backends("svc1", vec![backend]).await;
        let healthy = checker.get_healthy_backends("svc1").await;
        assert!(healthy.is_empty());

        let all = checker.get_all_backends("svc1").await;
        assert_eq!(all[0].health, BackendHealth::Unhealthy);
    }
}
