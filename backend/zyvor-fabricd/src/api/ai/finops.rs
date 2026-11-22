// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: LicenseRef-Zyvor-Production-2.0

//! Chargeback counters. These are arithmetic on already-recorded usage, not a
//! billing system.

pub fn gpu_seconds(replicas: u32, seconds: u64) -> u64 {
    u64::from(replicas).saturating_mul(seconds)
}

pub fn over_budget(spent: u64, budget: u64) -> bool {
    budget > 0 && spent > budget
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct UsageReport {
    pub gpu_seconds: u64,
    pub gpu_memory_gib_hours: u64,
    pub prompt_tokens: u64,
    pub completion_tokens: u64,
    pub storage_bytes: u64,
    pub transfer_bytes: u64,
    pub egress_bytes: u64,
    pub reserved_gpus: u32,
    pub utilized_gpus: u32,
    pub alert: bool,
}

pub struct UsageInput {
    pub reserved_gpus: u32,
    pub utilized_gpus: u32,
    pub seconds: u64,
    pub memory_gib: u32,
    pub prompt_tokens: u64,
    pub completion_tokens: u64,
    pub storage_bytes: u64,
    pub transfer_bytes: u64,
    pub egress_bytes: u64,
}

pub fn summarize(input: &UsageInput) -> UsageReport {
    UsageReport {
        gpu_seconds: gpu_seconds(input.utilized_gpus, input.seconds),
        gpu_memory_gib_hours: u64::from(input.utilized_gpus)
            .saturating_mul(u64::from(input.memory_gib))
            .saturating_mul(input.seconds)
            / 3600,
        prompt_tokens: input.prompt_tokens,
        completion_tokens: input.completion_tokens,
        storage_bytes: input.storage_bytes,
        transfer_bytes: input.transfer_bytes,
        egress_bytes: input.egress_bytes,
        reserved_gpus: input.reserved_gpus,
        utilized_gpus: input.utilized_gpus,
        alert: false,
    }
}

pub async fn report(
    security::RequireRead(_claims): security::RequireRead,
    axum::extract::State(state): axum::extract::State<std::sync::Arc<crate::server::AppState>>,
) -> Result<axum::Json<UsageReport>, (axum::http::StatusCode, axum::Json<serde_json::Value>)> {
    let deployments: Vec<super::types::InferenceDeployment> = state
        .store
        .list_entities(super::STORE_DEPLOYMENTS)
        .unwrap_or_default();
    let mut reserved = 0u32;
    let mut utilized = 0u32;
    for dep in &deployments {
        reserved =
            reserved.saturating_add(dep.replicas.saturating_mul(dep.gpus_per_replica.max(1)));
        let ready = dep
            .status
            .replicas
            .iter()
            .filter(|rep| rep.ready && !rep.draining)
            .count() as u32;
        utilized = utilized.saturating_add(ready.saturating_mul(dep.gpus_per_replica.max(1)));
    }
    let keys: Vec<super::types::InferenceApiKey> = state
        .store
        .list_entities(super::keys::STORE_API_KEYS)
        .unwrap_or_default();
    let tokens = keys.iter().map(|key| key.tokens_used_window).sum();
    let mut report = summarize(&UsageInput {
        reserved_gpus: reserved,
        utilized_gpus: utilized,
        seconds: 3600,
        memory_gib: 0,
        prompt_tokens: tokens,
        completion_tokens: 0,
        storage_bytes: 0,
        transfer_bytes: 0,
        egress_bytes: 0,
    });
    let spent = report
        .prompt_tokens
        .saturating_add(report.completion_tokens);
    report.alert = over_budget(spent, token_budget());
    Ok(axum::Json(report))
}

fn token_budget() -> u64 {
    std::env::var("FLUXVM_AI_TOKEN_BUDGET")
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(0)
}

pub fn token_charge(
    prompt_tokens: u64,
    completion_tokens: u64,
    prompt_rate: u64,
    completion_rate: u64,
) -> u64 {
    prompt_tokens
        .saturating_mul(prompt_rate)
        .saturating_add(completion_tokens.saturating_mul(completion_rate))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chargeback_multiplies_usage() {
        assert_eq!(gpu_seconds(2, 30), 60);
        assert_eq!(token_charge(10, 4, 2, 3), 32);
        let report = summarize(&UsageInput {
            reserved_gpus: 4,
            utilized_gpus: 2,
            seconds: 3600,
            memory_gib: 40,
            prompt_tokens: 10,
            completion_tokens: 4,
            storage_bytes: 100,
            transfer_bytes: 50,
            egress_bytes: 7,
        });
        assert_eq!(report.gpu_seconds, 7200);
        assert_eq!(report.gpu_memory_gib_hours, 80);
        assert_eq!(report.reserved_gpus, 4);
        assert!(!report.alert);
        assert!(over_budget(11, 10));
        assert!(!over_budget(10, 0));
    }
}
