// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: LicenseRef-Zyvor-Production-1.0

//! `ConsoleDriver` backed by FluxVM's `GET /v1/vms/{id}/console`
//! WebSocket (`FluxVmClient::open_console`), itself backed by the vsock
//! guest agent's `OpenShell` op — see that op's doc comment for why there's
//! no live terminal resize.

use anyhow::Result;
use async_trait::async_trait;
use zyvor_fabric_driver_core::{ConsoleDriver, ConsoleSession};

use crate::FluxVmDriver;

#[async_trait]
impl ConsoleDriver for FluxVmDriver {
    async fn open_console(&self, name: &str, cols: u16, rows: u16) -> Result<ConsoleSession> {
        let vm = self.resolve(name).await?;
        let ws = self.client.open_console(vm.id, cols, rows).await?;
        Ok(Box::pin(ws))
    }
}
