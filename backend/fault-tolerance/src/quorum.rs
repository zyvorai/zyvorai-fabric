// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: LicenseRef-Zyvor-Production-2.0

use anyhow::Result;
use std::sync::Arc;
use zyvor_fabric_driver_core::VmDriver;

/// Check if quorum is held by writing/reading a heartbeat file on shared storage.
pub fn check_quorum(quorum_path: &str, host_id: &str) -> Result<bool> {
    let heartbeat_file = format!("{}/{}.heartbeat", quorum_path, host_id);
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)?
        .as_secs();

    // Write our heartbeat
    std::fs::write(&heartbeat_file, now.to_string())?;

    // Read all heartbeats and check how many are recent (within 30s)
    let mut alive_hosts = 0;
    let mut total_hosts = 0;

    if let Ok(entries) = std::fs::read_dir(quorum_path) {
        for entry in entries.flatten() {
            if entry.path().extension().and_then(|e| e.to_str()) == Some("heartbeat") {
                total_hosts += 1;
                if let Ok(content) = std::fs::read_to_string(entry.path()) {
                    if let Ok(ts) = content.trim().parse::<u64>() {
                        if now - ts < 30 {
                            alive_hosts += 1;
                        }
                    }
                }
            }
        }
    }

    // Quorum requires majority
    let has_quorum = total_hosts == 0 || alive_hosts > total_hosts / 2;
    if !has_quorum {
        tracing::warn!("Lost quorum: {}/{} hosts alive", alive_hosts, total_hosts);
    }
    Ok(has_quorum)
}

/// Self-fence: stop all FT-protected VMs when quorum is lost, via the
/// active FluxVM `VmDriver` rather than shelling out to a CLI.
pub async fn self_fence(driver: &Arc<dyn VmDriver>) -> Result<()> {
    tracing::error!("QUORUM LOST: self-fencing - stopping all FT VMs");
    let machines = driver.list_machines().await?;
    for machine in machines {
        tracing::warn!("Self-fence: stopping VM '{}'", machine.name);
        if let Err(e) = driver.poweroff(&machine.name).await {
            tracing::warn!("Self-fence: failed to stop VM '{}': {e:#}", machine.name);
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp() -> std::path::PathBuf {
        let p = std::env::temp_dir().join(format!(
            "fabric-quorum-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&p).unwrap();
        p
    }

    #[test]
    fn single_writer_has_quorum() {
        let dir = tmp();
        assert!(check_quorum(dir.to_str().unwrap(), "host-a").unwrap());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn majority_alive_keeps_quorum() {
        let dir = tmp();
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();
        std::fs::write(dir.join("host-b.heartbeat"), now.to_string()).unwrap();
        std::fs::write(dir.join("host-c.heartbeat"), "1").unwrap(); // stale
        assert!(check_quorum(dir.to_str().unwrap(), "host-a").unwrap());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn minority_alive_loses_quorum() {
        let dir = tmp();
        std::fs::write(dir.join("host-b.heartbeat"), "1").unwrap();
        std::fs::write(dir.join("host-c.heartbeat"), "1").unwrap();
        // host-a writes a fresh heartbeat inside check_quorum → 1/3 alive
        assert!(!check_quorum(dir.to_str().unwrap(), "host-a").unwrap());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
