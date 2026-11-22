// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: LicenseRef-Zyvor-Production-2.0

//! Single-use prewarmed FluxVM sandboxes for low-latency agent starts.
//!
//! A warm sandbox contains only the generic Fabric worker. It is paused before
//! entering the pool, is claimed exactly once, and is never returned after user
//! code has executed. That preserves per-session filesystem/process isolation.

use crate::{
    app::{HARNESS, WORKER},
    model::{
        AgentRecord, AgentRuntimeKind, WarmPoolReconcileResult, WarmPoolView, WarmSandboxRecord,
        WarmSandboxState,
    },
    AppState,
};
use anyhow::{Context, Result};
use chrono::Utc;
use sha2::{Digest, Sha256};
use std::{sync::Arc, time::Duration};
use uuid::Uuid;

pub fn entrypoint_bytes(runtime: AgentRuntimeKind) -> &'static [u8] {
    if runtime.is_harness() {
        HARNESS
    } else {
        WORKER
    }
}

pub fn worker_digest_sha256(runtime: AgentRuntimeKind) -> String {
    let mut hasher = Sha256::new();
    hasher.update(runtime.as_str().as_bytes());
    hasher.update([0]);
    hasher.update(entrypoint_bytes(runtime));
    hex::encode(hasher.finalize())
}

pub async fn pool_view(state: &AppState, agent_name: &str) -> Result<WarmPoolView> {
    let agent = state
        .store
        .get_agent(agent_name)
        .await
        .with_context(|| format!("agent '{agent_name}' not found"))?;
    let sandboxes = state
        .store
        .list_warm_sandboxes_for(&agent.name, &agent.version)
        .await;
    let ready = sandboxes
        .iter()
        .filter(|record| record.state == WarmSandboxState::Ready)
        .count();
    let reconciling = sandboxes
        .iter()
        .filter(|record| record.state == WarmSandboxState::Reconciling)
        .count();
    let claiming = sandboxes
        .iter()
        .filter(|record| record.state == WarmSandboxState::Claiming)
        .count();
    Ok(WarmPoolView {
        agent: agent.name,
        agent_version: agent.version,
        desired: agent.manifest.warm_pool_size,
        ready,
        reconciling,
        claiming,
        sandboxes,
    })
}

pub async fn reconcile_agent(
    state: &AppState,
    agent_name: &str,
) -> Result<WarmPoolReconcileResult> {
    let _guard = state.warm_pool_reconcile_lock.lock().await;
    let agent = state
        .store
        .get_agent(agent_name)
        .await
        .with_context(|| format!("agent '{agent_name}' not found"))?;
    reconcile_agent_locked(state, &agent).await
}

pub async fn reconcile_all(state: &AppState) -> Result<WarmPoolReconcileResult> {
    let _guard = state.warm_pool_reconcile_lock.lock().await;
    let agents = state.store.list_agents().await;
    let mut total = WarmPoolReconcileResult::default();

    // First remove old deployment pools and resolve durable in-flight claims.
    let records = state.store.list_warm_sandboxes().await;
    for record in records {
        if record.state == WarmSandboxState::Claiming {
            if let Some(session_id) = record.claimed_by {
                if state
                    .store
                    .get_session(session_id)
                    .await
                    .is_some_and(|session| session.sandbox_id == record.sandbox_id)
                {
                    // The session record is the durable owner now. A crash may
                    // have happened before the pool record was removed.
                    state.store.forget_warm_sandbox(record.sandbox_id).await?;
                    total.removed += 1;
                    continue;
                }
            }
            let age = Utc::now()
                .signed_duration_since(record.updated_at)
                .num_seconds()
                .max(0) as u64;
            if age < state.config.warm_pool_claim_stale_secs {
                continue;
            }
            discard_claimed(state, record.sandbox_id).await?;
            total.removed += 1;
            continue;
        }

        let current = agents.iter().find(|agent| agent.name == record.agent);
        let keep = current.is_some_and(|agent| {
            agent.version == record.agent_version
                && agent.manifest.warm_pool_size > 0
                && record.runtime_port == agent.manifest.runtime_port
                && record.worker_digest_sha256 == worker_digest_sha256(agent.manifest.runtime)
        });
        if !keep {
            let Some(reserved) = state.store.begin_warm_reconcile(record.sandbox_id).await? else {
                // A concurrent session owns this sandbox now.
                continue;
            };
            delete_reserved(state, &reserved).await?;
            total.removed += 1;
        }
    }

    for agent in &agents {
        let result = reconcile_agent_locked(state, agent).await?;
        total.created += result.created;
        total.removed += result.removed;
        total.repaired += result.repaired;
        total.ready += result.ready;
    }
    Ok(total)
}

async fn reconcile_agent_locked(
    state: &AppState,
    agent: &AgentRecord,
) -> Result<WarmPoolReconcileResult> {
    let mut result = WarmPoolReconcileResult::default();
    let worker_digest = worker_digest_sha256(agent.manifest.runtime);

    // Validate every claimable/reconciling record against FluxVM. Reserving
    // the record in `Reconciling` makes the health action mutually exclusive
    // with a session claim, even when the FluxVM request takes time.
    let records = state
        .store
        .list_warm_sandboxes_for(&agent.name, &agent.version)
        .await;
    for candidate in records {
        if candidate.state == WarmSandboxState::Claiming {
            continue;
        }
        let Some(record) = state
            .store
            .begin_warm_reconcile(candidate.sandbox_id)
            .await?
        else {
            continue;
        };
        if record.worker_digest_sha256 != worker_digest
            || record.runtime_port != agent.manifest.runtime_port
        {
            delete_reserved(state, &record).await?;
            result.removed += 1;
            continue;
        }
        let sandbox = match state.fluxvm.get_sandbox(record.sandbox_id).await {
            Ok(sandbox) => sandbox,
            Err(error) if is_not_found(&error) => {
                state.store.forget_warm_sandbox(record.sandbox_id).await?;
                result.removed += 1;
                continue;
            }
            Err(error) => {
                // Preserve tracking and keep it unclaimable until FluxVM is
                // reachable again; never turn an unknown VM into an orphan.
                return Err(error).context("checking warm sandbox health");
            }
        };
        match sandbox
            .status
            .as_deref()
            .unwrap_or("unknown")
            .to_ascii_lowercase()
            .as_str()
        {
            "paused" => {
                state.store.finish_warm_reconcile(record.sandbox_id).await?;
            }
            "running" => match state.fluxvm.pause(record.sandbox_id).await {
                Ok(()) => {
                    state.store.finish_warm_reconcile(record.sandbox_id).await?;
                    result.repaired += 1;
                }
                Err(_) => {
                    delete_reserved(state, &record).await?;
                    result.removed += 1;
                }
            },
            _ => {
                delete_reserved(state, &record).await?;
                result.removed += 1;
            }
        }
    }

    let ready = state
        .store
        .list_warm_sandboxes_for(&agent.name, &agent.version)
        .await
        .into_iter()
        .filter(|record| record.state == WarmSandboxState::Ready)
        .count();
    if ready < agent.manifest.warm_pool_size {
        let missing = agent.manifest.warm_pool_size - ready;
        let budget = state.config.warm_pool_max_create_per_tick.max(1);
        for _ in 0..missing.min(budget) {
            create_warm_sandbox(state, agent, &worker_digest).await?;
            result.created += 1;
        }
    } else if ready > agent.manifest.warm_pool_size {
        let excess = ready - agent.manifest.warm_pool_size;
        let records = state
            .store
            .list_warm_sandboxes_for(&agent.name, &agent.version)
            .await;
        for record in records
            .into_iter()
            .filter(|record| record.state == WarmSandboxState::Ready)
            .rev()
            .take(excess)
        {
            let Some(reserved) = state.store.begin_warm_reconcile(record.sandbox_id).await? else {
                continue;
            };
            delete_reserved(state, &reserved).await?;
            result.removed += 1;
        }
    }

    result.ready = state
        .store
        .list_warm_sandboxes_for(&agent.name, &agent.version)
        .await
        .into_iter()
        .filter(|record| record.state == WarmSandboxState::Ready)
        .count();
    Ok(result)
}

fn is_not_found(error: &anyhow::Error) -> bool {
    let text = error.to_string().to_ascii_lowercase();
    text.contains("404") || text.contains("not found")
}

pub async fn discard_claimed(state: &AppState, sandbox_id: Uuid) -> Result<()> {
    match state.fluxvm.delete(sandbox_id).await {
        Ok(()) => {}
        Err(error) if is_not_found(&error) => {}
        Err(error) => {
            // Keep the durable `Claiming` record so reconciliation can retry.
            return Err(error).context("deleting claimed warm sandbox");
        }
    }
    state.store.forget_warm_sandbox(sandbox_id).await?;
    Ok(())
}

async fn delete_reserved(state: &AppState, record: &WarmSandboxRecord) -> Result<()> {
    match state.fluxvm.delete(record.sandbox_id).await {
        Ok(()) => {}
        Err(error) if is_not_found(&error) => {}
        Err(error) => {
            // Leave the durable record in `Reconciling`, which is deliberately
            // unclaimable. A later reconciliation can retry without orphaning
            // a VM whose deletion outcome is unknown.
            return Err(error).context("deleting warm sandbox");
        }
    }
    state.store.forget_warm_sandbox(record.sandbox_id).await?;
    Ok(())
}

async fn create_warm_sandbox(
    state: &AppState,
    agent: &AgentRecord,
    worker_digest: &str,
) -> Result<WarmSandboxRecord> {
    let nonce = Uuid::new_v4().simple().to_string();
    let version = agent.version.chars().take(6).collect::<String>();
    let name = format!("agent-warm-{version}-{}", &nonce[..8]);
    let sandbox = state
        .fluxvm
        .create_sandbox(
            name,
            &agent.manifest.template,
            None,
            agent.manifest.runtime_port,
            // Deploy validation forbids warm pools together with a home volume
            // or a confidential launch.
            &crate::fluxvm::SandboxOptions {
                resources: agent.manifest.resources,
                ..Default::default()
            },
        )
        .await?;

    // Track the VM before doing multi-step preparation. If this process dies
    // after creation, the durable Reconciling record prevents the VM from
    // becoming claimable until the next health reconciliation completes.
    let now = Utc::now();
    let record = WarmSandboxRecord {
        sandbox_id: sandbox.id,
        agent: agent.name.clone(),
        agent_version: agent.version.clone(),
        runtime_port: agent.manifest.runtime_port,
        worker_digest_sha256: worker_digest.to_string(),
        state: WarmSandboxState::Reconciling,
        claimed_by: None,
        created_at: now,
        updated_at: now,
    };
    if let Err(error) = state.store.save_warm_sandbox(record.clone()).await {
        let cleanup = state.fluxvm.delete(sandbox.id).await;
        if let Err(cleanup_error) = cleanup {
            tracing::error!(
                sandbox = %sandbox.id,
                error = %cleanup_error,
                "warm sandbox record persistence and cleanup both failed"
            );
        }
        return Err(error).context("persisting new warm sandbox record");
    }

    let prepare = async {
        state
            .fluxvm
            .process(sandbox.id, "mkdir -p /opt/zyvor/agent", Some(10))
            .await?;
        state
            .fluxvm
            .fs_write(
                sandbox.id,
                "/opt/zyvor/worker.mjs",
                entrypoint_bytes(agent.manifest.runtime),
                0o755,
            )
            .await?;
        state.fluxvm.pause(sandbox.id).await?;
        Ok::<(), anyhow::Error>(())
    }
    .await;
    if let Err(error) = prepare {
        if let Err(cleanup_error) = delete_reserved(state, &record).await {
            tracing::warn!(
                sandbox = %sandbox.id,
                error = %cleanup_error,
                "failed to clean up partially prepared warm sandbox"
            );
        }
        return Err(error).context("preparing warm sandbox");
    }

    state.store.finish_warm_reconcile(sandbox.id).await?;
    tracing::info!(
        agent = %agent.name,
        version = %agent.version,
        sandbox = %sandbox.id,
        "prewarmed agent sandbox"
    );
    let mut ready = record;
    ready.state = WarmSandboxState::Ready;
    ready.updated_at = Utc::now();
    Ok(ready)
}

pub async fn warm_pool_loop(state: Arc<AppState>) {
    let interval = Duration::from_millis(state.config.warm_pool_reconcile_interval_ms.max(250));
    loop {
        match reconcile_all(&state).await {
            Ok(result) if result.created + result.removed + result.repaired > 0 => {
                tracing::info!(
                    created = result.created,
                    removed = result.removed,
                    repaired = result.repaired,
                    ready = result.ready,
                    "agent warm-pool reconciliation complete"
                );
            }
            Ok(_) => {}
            Err(error) => tracing::warn!(%error, "agent warm-pool reconciliation failed"),
        }
        tokio::time::sleep(interval).await;
    }
}
