// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: LicenseRef-Zyvor-Production-1.0

//! Phase 1 harness test (see the systemd-removal plan): boots a real
//! `fluxvm serve` process from the sibling FluxVM checkout and exercises
//! the wire contract `zyvor-fabric-fluxvm-client` depends on.
//!
//! This intentionally does NOT create a real VM (that needs a QEMU/Cloud
//! Hypervisor/Firecracker binary and a base disk image — heavier
//! integration coverage that belongs to the Phase 3/5 lifecycle-cutover
//! tests). It only proves: the `fluxvm` binary starts, serves `/healthz`,
//! and returns wire-compatible JSON for the read-only endpoints this client
//! parses.
//!
//! Skips (does not fail) if the `fluxvm` binary can't be found, so this
//! passes on machines/CI that haven't built the sibling repo yet. Point it
//! at a specific binary with `FLUXVM_BIN=/path/to/fluxvm`.

use std::{
    path::PathBuf,
    process::{Child, Command, Stdio},
    time::Duration,
};

use zyvor_fabric_fluxvm_client::FluxVmClient;

struct FluxVmServe {
    child: Child,
    _workdir: PathBuf,
}

impl Drop for FluxVmServe {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

fn find_fluxvm_binary() -> Option<PathBuf> {
    if let Ok(path) = std::env::var("FLUXVM_BIN") {
        let path = PathBuf::from(path);
        return path.exists().then_some(path);
    }
    // Sibling checkout: .../zyvor-fabric/backend/crates/fluxvm-client -> .../tt/FluxVM
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let candidate = manifest_dir
        .join("../../../../FluxVM/target/debug/fluxvm")
        .canonicalize()
        .ok()?;
    candidate.exists().then_some(candidate)
}

async fn spawn_fluxvm(bin: &PathBuf, port: u16) -> anyhow::Result<FluxVmServe> {
    let workdir = std::env::temp_dir().join(format!("fluxvm-harness-{}", std::process::id()));
    std::fs::create_dir_all(&workdir)?;
    let state_dir = workdir.join("state");
    let run_dir = workdir.join("run");
    let config_path = workdir.join("config.toml");
    std::fs::write(
        &config_path,
        format!(
            "listen = \"127.0.0.1:{port}\"\nstate_dir = \"{}\"\nrun_dir = \"{}\"\nreaper_interval_secs = 3600\n",
            state_dir.display(),
            run_dir.display(),
        ),
    )?;

    let child = Command::new(bin)
        .arg("--config")
        .arg(&config_path)
        .arg("serve")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()?;

    let client = FluxVmClient::new(format!("http://127.0.0.1:{port}"))?;
    let deadline = std::time::Instant::now() + Duration::from_secs(10);
    while std::time::Instant::now() < deadline {
        if client.healthy().await {
            return Ok(FluxVmServe {
                child,
                _workdir: workdir,
            });
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    anyhow::bail!("fluxvm serve did not become healthy within 10s")
}

#[tokio::test]
async fn boots_and_serves_healthz() {
    let Some(bin) = find_fluxvm_binary() else {
        eprintln!(
            "SKIP: no fluxvm binary found (build ../FluxVM with `cargo build` or set FLUXVM_BIN)"
        );
        return;
    };

    let serve = spawn_fluxvm(&bin, 17788)
        .await
        .expect("fluxvm serve failed to start");
    let client = FluxVmClient::new("http://127.0.0.1:17788").unwrap();

    assert!(client.healthy().await, "healthz should report healthy");

    let vms = client
        .list_vms()
        .await
        .expect("list_vms should succeed against a fresh instance");
    assert!(vms.is_empty(), "a fresh fluxvm instance should have no VMs");

    assert!(
        client
            .find_by_name("does-not-exist")
            .await
            .unwrap()
            .is_none(),
        "find_by_name should return None, not error, for an unknown name"
    );

    let missing = client.get_vm(uuid::Uuid::new_v4()).await;
    assert!(
        missing.is_err(),
        "getting a nonexistent VM id should error, not panic or hang"
    );

    drop(serve);
}
