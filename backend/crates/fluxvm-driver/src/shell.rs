// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: Apache-2.0

//! `ShellDriver` backed by FluxVM's vsock guest-agent `Exec` op
//! (`POST /v1/vms/{id}/agent`) — requires the VM to have been created with
//! `CreateVmRequest.agent.enabled`; FluxVM itself errors clearly
//! (`"guest agent is not enabled for this VM"`) rather than hanging when
//! it wasn't.

use anyhow::{bail, Context, Result};
use async_trait::async_trait;
use base64::{engine::general_purpose::STANDARD as B64, Engine};
use zyvor_fabric_driver_core::{ShellDriver, ShellOutput};
use zyvor_fabric_fluxvm_client::AgentResponse;

use crate::FluxVmDriver;

/// Matches `fluxvm_guest_protocol::MAX_FILE_TRANSFER_BYTES` — checked
/// host-side too so a too-large file fails fast instead of base64-encoding
/// the whole thing into memory first.
const MAX_FILE_TRANSFER_BYTES: u64 = 64 * 1024 * 1024;

#[async_trait]
impl ShellDriver for FluxVmDriver {
    async fn shell(
        &self,
        name: &str,
        command: &str,
        timeout_seconds: Option<u64>,
    ) -> Result<ShellOutput> {
        let vm = self.resolve(name).await?;
        match self
            .client
            .agent_exec(vm.id, command, timeout_seconds)
            .await?
        {
            AgentResponse::Exec {
                exit_code,
                stdout,
                stderr,
                ..
            } => Ok(ShellOutput {
                stdout,
                stderr,
                exit_code,
            }),
            AgentResponse::Error { message } => bail!("guest agent error: {message}"),
            other => bail!("unexpected guest agent response to Exec: {other:?}"),
        }
    }

    async fn copy_to(
        &self,
        name: &str,
        host_path: &str,
        machine_path: &str,
        mode: Option<u32>,
    ) -> Result<()> {
        let vm = self.resolve(name).await?;
        let metadata = tokio::fs::metadata(host_path)
            .await
            .with_context(|| format!("reading {host_path}"))?;
        if metadata.len() > MAX_FILE_TRANSFER_BYTES {
            bail!("{host_path} is {} bytes, exceeds the {MAX_FILE_TRANSFER_BYTES}-byte transfer limit", metadata.len());
        }
        let mode = mode.or_else(|| {
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                Some(metadata.permissions().mode() & 0o777)
            }
            #[cfg(not(unix))]
            {
                None
            }
        });
        let bytes = tokio::fs::read(host_path)
            .await
            .with_context(|| format!("reading {host_path}"))?;
        match self
            .client
            .agent_put_file(vm.id, machine_path, &B64.encode(&bytes), mode)
            .await?
        {
            AgentResponse::FileWritten => Ok(()),
            AgentResponse::Error { message } => bail!("guest agent error: {message}"),
            other => bail!("unexpected guest agent response to PutFile: {other:?}"),
        }
    }

    async fn copy_from(&self, name: &str, machine_path: &str, host_path: &str) -> Result<()> {
        let vm = self.resolve(name).await?;
        match self.client.agent_get_file(vm.id, machine_path).await? {
            AgentResponse::FileContent {
                content_base64,
                mode,
            } => {
                let bytes = B64
                    .decode(&content_base64)
                    .context("guest agent returned invalid base64")?;
                if let Some(parent) = std::path::Path::new(host_path).parent() {
                    tokio::fs::create_dir_all(parent)
                        .await
                        .with_context(|| format!("creating {}", parent.display()))?;
                }
                tokio::fs::write(host_path, &bytes)
                    .await
                    .with_context(|| format!("writing {host_path}"))?;
                #[cfg(unix)]
                {
                    use std::os::unix::fs::PermissionsExt;
                    tokio::fs::set_permissions(host_path, std::fs::Permissions::from_mode(mode))
                        .await
                        .with_context(|| format!("setting permissions on {host_path}"))?;
                }
                Ok(())
            }
            AgentResponse::Error { message } => bail!("guest agent error: {message}"),
            other => bail!("unexpected guest agent response to GetFile: {other:?}"),
        }
    }
}
