// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: Apache-2.0

//! Live checks against a real `fluxctl serve`. Ignored by default.
//!
//! ```text
//! FLUXVM_LIVE_URL=http://127.0.0.1:7799 \
//! FLUXVM_LIVE_IMAGE=/var/lib/fluxvm/images/linux-agent.raw \
//! FLUXVM_LIVE_KERNEL=/var/lib/fluxvm/kernels/vmlinux \
//!   cargo test -p zyvor-fabric-fluxvm-client --test live_fluxvm -- --ignored --nocapture
//! ```
//!
//! Use a throwaway daemon (own `state_dir`, network mode `none`); the test
//! creates and deletes its own VMs and does not touch existing ones.

use serde_json::json;
use uuid::Uuid;
use zyvor_fabric_fluxvm_client::{CreateVmRequest, FluxVmClient, ForkVmRequest, ReadyOptions};

fn live() -> Option<(FluxVmClient, CreateVmRequest)> {
    let url = std::env::var("FLUXVM_LIVE_URL").ok()?;
    let image = std::env::var("FLUXVM_LIVE_IMAGE")
        .unwrap_or_else(|_| "/var/lib/fluxvm/images/linux-agent.raw".into());
    let kernel = std::env::var("FLUXVM_LIVE_KERNEL")
        .unwrap_or_else(|_| "/var/lib/fluxvm/kernels/vmlinux".into());
    let req = serde_json::from_value(json!({
        "name": format!("live-{}", &Uuid::new_v4().to_string()[..8]),
        "backend": "flux-vm",
        "image": image,
        "kernel": kernel,
        "vcpus": 1,
        "memory_mib": 512,
        "ttl_seconds": 1800,
        "network": { "mode": "none" },
        "agent": { "enabled": true }
    }))
    .expect("CreateVmRequest");
    Some((FluxVmClient::new(url).expect("client"), req))
}

#[tokio::test]
#[ignore = "needs FLUXVM_LIVE_URL"]
async fn idempotent_create_and_delete_replay_on_a_real_daemon() {
    let Some((client, req)) = live() else { return };
    let key = format!("live-create-{}", Uuid::new_v4());

    let first = client
        .create_vm_idempotent(&req, &key)
        .await
        .expect("create");
    let again = client
        .create_vm_idempotent(&req, &key)
        .await
        .expect("replayed create");
    assert_eq!(first.id, again.id, "a replay must return the same VM");
    let named: Vec<_> = client
        .list_vms()
        .await
        .unwrap()
        .into_iter()
        .filter(|v| v.name == req.name)
        .collect();
    assert_eq!(named.len(), 1, "a replay must not create a second VM");

    let del = format!("live-delete-{}", Uuid::new_v4());
    client
        .delete_vm_idempotent(first.id, &del)
        .await
        .expect("delete");
    client
        .delete_vm_idempotent(first.id, &del)
        .await
        .expect("replayed delete");
}

#[tokio::test]
#[ignore = "needs FLUXVM_LIVE_URL"]
async fn fork_snapshot_restore_round_trip_on_a_real_daemon() {
    let Some((client, req)) = live() else { return };
    let parent = client
        .create_vm_ready(
            &req,
            &ReadyOptions {
                ready_exec: true,
                ..Default::default()
            },
        )
        .await
        .expect("create parent");
    println!("parent first_command_ms={:?}", parent.first_command_ms);
    assert!(parent.first_command_ms.is_some() || parent.first_command_error.is_some());
    let id = parent.vm.id;

    let outcome = async {
        client.snapshot_vm(id, "live-tag", None).await?;
        let snaps = client.list_vm_snapshots(id).await?;
        assert!(snaps.iter().any(|s| s.tag == "live-tag"), "{snaps:?}");

        let forked = client
            .fork_vm(
                id,
                &ForkVmRequest {
                    count: 2,
                    name_prefix: Some("live-kid".into()),
                },
                &ReadyOptions {
                    ready_exec: true,
                    idempotency_key: Some(&format!("live-fork-{}", Uuid::new_v4())),
                },
            )
            .await?;
        println!(
            "fork: {} children, elapsed_ms={}, first_command_ms={:?}",
            forked.items.len(),
            forked.elapsed_ms,
            forked.first_command_ms
        );
        assert_eq!(forked.items.len(), 2);
        for kid in &forked.items {
            client.delete_vm(kid.id).await?;
        }

        client.restore_vm(id, "live-tag").await?;
        client.delete_vm_snapshot(id, "live-tag").await?;
        let left = client.list_vm_snapshots(id).await?;
        assert!(!left.iter().any(|s| s.tag == "live-tag"));
        anyhow::Ok(())
    }
    .await;

    // Always remove the parent, then report what happened.
    let _ = client.delete_vm(id).await;
    outcome.expect("fork/snapshot/restore round trip");
}
