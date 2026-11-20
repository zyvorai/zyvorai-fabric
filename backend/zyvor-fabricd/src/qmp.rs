// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: LicenseRef-Zyvor-Production-1.0

use anyhow::{Context, Result};
use serde_json::Value;
use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::UnixStream;
use std::time::Duration;

/// Commands allowed through the QMP interface.
const ALLOWED_QMP_COMMANDS: &[&str] = &[
    "query-status",
    "query-block",
    "query-blockstats",
    "query-cpus-fast",
    "query-hotpluggable-cpus",
    "query-memory-size-summary",
    "query-vnc",
    "query-spice",
    // Read-only PCI topology inspection -- used to check which hotplug
    // root ports already have a child device before picking one for
    // device_add (see hotplug.rs::occupied_hotplug_buses).
    "query-pci",
    "system_powerdown",
    "system_reset",
    "stop",
    "cont",
    "balloon",
    "block_resize",
    "blockdev-snapshot-sync",
    // Internal qcow2 snapshot (disk only, no vmstate) — used for
    // snapshot_type=Disk on a running VM. Distinct from snapshot-save,
    // which dumps memory and can take minutes under disk contention.
    "blockdev-snapshot-internal-sync",
    "blockdev-snapshot-delete-internal-sync",
    "blockdev-add",
    "blockdev-del",
    "device_add",
    "device_del",
    "netdev_add",
    "netdev_del",
    "object-add",
    "object-del",
    "chardev-add",
    "chardev-remove",
    "savevm",
    "loadvm",
    "delvm",
    "cpu-add",
    "drive-backup",
    // Modern (QEMU 6.0+) job-based internal-snapshot API. `savevm` above
    // is the old HMP command name and isn't a real top-level QMP command
    // on current QEMU -- found live: "The command savevm has not been
    // found" -- `human-monitor-command` would work but is deliberately
    // never allow-listed (see test_disallowed_command_rejected below):
    // it's a raw HMP passthrough, a much bigger attack surface than any
    // single whitelisted command. snapshot-save is the supported
    // replacement for what savevm was meant to do here.
    "snapshot-save",
    "query-jobs",
    "job-dismiss",
    // Immediately terminates this one VM's QEMU process -- narrowly
    // scoped (unlike human-monitor-command's arbitrary-command
    // passthrough) and no more privileged than system_powerdown/
    // system_reset above. Needed by hibernate_vm: once a snapshot-save
    // has captured memory+disk state, the guest must not keep running
    // and mutating disk state past that point, so a graceful ACPI
    // powerdown (which the guest could ignore or delay) is the wrong
    // tool -- found live: without this, hibernate's own "quit" call
    // always failed (blocked by this same allowlist), leaving the VM
    // marked Stopped in the store while its QEMU process kept running.
    "quit",
];

/// Check whether a QMP command is in the allowed list.
fn is_command_allowed(command: &str) -> bool {
    ALLOWED_QMP_COMMANDS.contains(&command)
}

/// Minimal QMP (QEMU Machine Protocol) client for communicating with a
/// QEMU monitor socket, at a path resolved by `state.driver.get_control_socket()`
/// (backend-specific: the systemd-vmspawn convention for `MachinectlDriver`,
/// `VmRecord.control_socket` for `FluxVmDriver`).
pub struct QmpClient {
    socket_path: String,
}

impl QmpClient {
    /// Create a QMP client for an already-resolved control socket path.
    pub fn for_socket(socket_path: impl Into<String>) -> Self {
        Self {
            socket_path: socket_path.into(),
        }
    }

    /// Check if the QMP socket exists and is accessible
    pub fn is_available(&self) -> bool {
        std::path::Path::new(&self.socket_path).exists()
    }

    /// Execute a single QMP command over its own connection.
    ///
    /// Fine for one-shot commands, but do NOT use this for a sequence where
    /// a later command references an object/device an earlier one just
    /// created (`object-add` then `device_add` with `memdev`/`drive`
    /// pointing at it, `blockdev-add` then `device_add`, `netdev_add` then
    /// `device_add`, etc.) -- found live: memory hotplug's `object-add`
    /// would return success, but the very next `device_add` referencing
    /// that same backend id failed with "Device '<id>' not found", even
    /// though replaying the exact same two commands over one held-open
    /// connection worked every time. Reconnecting and renegotiating
    /// `qmp_capabilities` between the two calls is enough for QEMU to not
    /// yet consider the object visible to the new monitor connection. Use
    /// [`QmpClient::open_session`] for any multi-command sequence instead.
    pub fn execute(&self, command: &str, args: Value) -> Result<Value> {
        self.open_session()?.execute(command, args)
    }

    /// Open one QMP connection (greeting + capabilities negotiated once)
    /// for a caller to run a whole related command sequence over -- see
    /// the warning on [`QmpClient::execute`] for why this matters for
    /// anything beyond a single command.
    pub fn open_session(&self) -> Result<QmpSession> {
        let mut stream = UnixStream::connect(&self.socket_path)
            .with_context(|| format!("Failed to connect to QMP socket: {}", self.socket_path))?;

        // 300s, not 10s: found live via strace on a real snapshot-save call --
        // qmp_capabilities and query-block both come back in under a
        // millisecond, but snapshot-save's own command-level {"return":{}}
        // doesn't arrive until *after* its vmstate dump finishes (it fires a
        // QMP "STOP" event pausing the VM, then the monitor's reply to the
        // command itself -- not just job completion, reported separately via
        // query-jobs -- waits on that dump). This is genuinely variable, not
        // a fixed cost: repeated live tests against the same idle VM under
        // real disk/host contention saw it take anywhere from under a
        // second to well past 60s with no other change to the VM or
        // request -- 60s still wasn't consistently enough. A slow disk
        // under load is exactly the situation this shouldn't give up on;
        // a stuck connection taking 30x longer to be detected as dead is a
        // fine trade against killing a snapshot that's still genuinely in
        // progress.
        stream.set_read_timeout(Some(Duration::from_secs(300)))?;
        stream.set_write_timeout(Some(Duration::from_secs(5)))?;

        let mut reader = BufReader::new(stream.try_clone()?);

        // Read the QMP greeting
        let mut greeting = String::new();
        reader.read_line(&mut greeting)?;

        // Send qmp_capabilities to negotiate
        let caps = serde_json::json!({"execute": "qmp_capabilities"});
        writeln!(stream, "{}", caps)?;
        stream.flush()?;

        // Read capabilities response
        let mut caps_response = String::new();
        reader.read_line(&mut caps_response)?;

        Ok(QmpSession { stream, reader })
    }
}

/// A single QMP connection, already past the greeting/`qmp_capabilities`
/// handshake, that can run more than one command in sequence -- see
/// [`QmpClient::execute`]'s doc comment for why this exists.
pub struct QmpSession {
    stream: UnixStream,
    reader: BufReader<UnixStream>,
}

impl QmpSession {
    pub fn execute(&mut self, command: &str, args: Value) -> Result<Value> {
        if !is_command_allowed(command) {
            return Err(anyhow::anyhow!(
                "QMP command '{}' is not in the allowed list",
                command
            ));
        }

        let cmd = if args.is_null() {
            serde_json::json!({"execute": command})
        } else {
            serde_json::json!({"execute": command, "arguments": args})
        };

        writeln!(self.stream, "{}", cmd)?;
        self.stream.flush()?;

        // Events can be interleaved with command responses on the same
        // connection -- skip any line that isn't the reply to this command
        // (no "return"/"error" key) rather than treating it as the answer.
        loop {
            let mut response_str = String::new();
            self.reader.read_line(&mut response_str)?;

            let response: Value = serde_json::from_str(&response_str)
                .with_context(|| format!("Failed to parse QMP response: {}", response_str))?;

            if let Some(error) = response.get("error") {
                return Err(anyhow::anyhow!(
                    "QMP error: {}",
                    error
                        .get("desc")
                        .and_then(|d| d.as_str())
                        .unwrap_or("unknown")
                ));
            }

            if let Some(ret) = response.get("return") {
                return Ok(ret.clone());
            }
            // else: an async event line (e.g. ACPI_DEVICE_OST) -- keep reading.
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_disallowed_command_rejected() {
        assert!(!is_command_allowed("human-monitor-command"));
        assert!(!is_command_allowed(""));
        assert!(!is_command_allowed("rm -rf /"));
    }

    #[test]
    fn test_allowed_commands_pass() {
        for cmd in ALLOWED_QMP_COMMANDS {
            assert!(
                is_command_allowed(cmd),
                "Command '{}' should be allowed",
                cmd
            );
        }
        // Verify specific commands critical for hotplug
        assert!(is_command_allowed("blockdev-add"));
        assert!(is_command_allowed("blockdev-del"));
        assert!(is_command_allowed("device_add"));
        assert!(is_command_allowed("query-hotpluggable-cpus"));
    }

    #[test]
    fn test_is_available_nonexistent() {
        let client = QmpClient::for_socket("/run/systemd/vmspawn/nonexistent-vm-12345/qemu.sock");
        assert!(!client.is_available());
    }
}
