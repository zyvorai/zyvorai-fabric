// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: LicenseRef-Zyvor-Production-2.0

use axum::{
    extract::{
        ws::{Message, WebSocket, WebSocketUpgrade},
        Path, Query, State,
    },
    http::StatusCode,
    response::IntoResponse,
};
use futures::{sink::SinkExt, stream::StreamExt};
use serde::Deserialize;
use std::sync::Arc;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::UnixStream;
use zyvor_fabric_driver_core::VmDriver;

#[derive(Debug, Deserialize)]
pub struct VncQuery {
    pub token: Option<String>,
}

/// VNC WebSocket proxy handler.
/// Bridges WebSocket (browser) <-> VNC server (FluxVM's per-VM UNIX
/// socket at `<workspace>/vnc.sock`, resolved via `VmDriver::get_vnc_socket`
/// rather than a guessed TCP port — see driver-core's doc comment).
pub async fn vnc_handler<S>(
    ws: WebSocketUpgrade,
    Path(vm_name): Path<String>,
    Query(query): Query<VncQuery>,
    State(state): State<Arc<S>>,
) -> Result<impl IntoResponse, StatusCode>
where
    S: VncProxyState + Send + Sync + 'static,
{
    let jwt_config = state.jwt_config().ok_or_else(|| {
        tracing::warn!("VNC connection rejected: authentication not configured");
        StatusCode::UNAUTHORIZED
    })?;

    let token = query.token.as_deref().ok_or_else(|| {
        tracing::warn!(
            "VNC connection rejected: no auth token for VM '{}'",
            vm_name
        );
        StatusCode::UNAUTHORIZED
    })?;

    let claims = jwt_config.validate_token(token).map_err(|e| {
        tracing::warn!("VNC auth failed for VM '{}': {}", vm_name, e);
        StatusCode::UNAUTHORIZED
    })?;

    if !claims.role.can_write() {
        tracing::warn!(
            "VNC connection rejected: user '{}' has insufficient permissions",
            claims.sub
        );
        return Err(StatusCode::FORBIDDEN);
    }

    let socket_path = state
        .driver()
        .get_vnc_socket(&vm_name)
        .await
        .map_err(|e| {
            tracing::warn!("Failed to resolve VNC socket for VM '{}': {:#}", vm_name, e);
            StatusCode::BAD_GATEWAY
        })?
        .ok_or(StatusCode::NOT_FOUND)?;

    Ok(ws.on_upgrade(move |socket| handle_vnc(socket, vm_name, socket_path)))
}

/// Minimal state accessor `vnc_handler` needs from `zyvor-fabricd::AppState`,
/// kept as a trait so this crate doesn't depend on `zyvor-fabricd` itself.
pub trait VncProxyState {
    fn driver(&self) -> Arc<dyn VmDriver>;
    fn jwt_config(&self) -> Option<Arc<security::JwtConfig>>;
}

async fn handle_vnc(socket: WebSocket, vm_name: String, socket_path: std::path::PathBuf) {
    tracing::info!("VNC WebSocket connection for VM: {}", vm_name);

    let unix_stream = match UnixStream::connect(&socket_path).await {
        Ok(stream) => stream,
        Err(e) => {
            tracing::error!(
                "Failed to connect to VNC socket {} for VM '{}': {}",
                socket_path.display(),
                vm_name,
                e
            );
            return;
        }
    };

    let (mut ws_sender, mut ws_receiver) = socket.split();
    let (mut vnc_read, mut vnc_write) = unix_stream.into_split();

    // VNC -> WebSocket
    let vm_name_clone = vm_name.clone();
    let vnc_to_ws = tokio::spawn(async move {
        let mut buf = vec![0u8; 8192];
        loop {
            match vnc_read.read(&mut buf).await {
                Ok(0) => break,
                Ok(n) => {
                    if ws_sender
                        .send(Message::Binary(buf[..n].to_vec().into()))
                        .await
                        .is_err()
                    {
                        break;
                    }
                }
                Err(e) => {
                    tracing::error!("VNC read error: {}", e);
                    break;
                }
            }
        }
        tracing::info!("VNC to WS task ended for VM: {}", vm_name_clone);
    });

    // WebSocket -> VNC
    let vm_name_clone = vm_name.clone();
    let ws_to_vnc = tokio::spawn(async move {
        while let Some(msg) = ws_receiver.next().await {
            match msg {
                Ok(Message::Binary(data)) => {
                    if vnc_write.write_all(&data).await.is_err() {
                        break;
                    }
                }
                Ok(Message::Close(_)) => break,
                Err(e) => {
                    tracing::error!("WebSocket error: {}", e);
                    break;
                }
                _ => {}
            }
        }
        tracing::info!("WS to VNC task ended for VM: {}", vm_name_clone);
    });

    let _ = tokio::join!(vnc_to_ws, ws_to_vnc);

    tracing::info!("VNC connection closed for VM: {}", vm_name);
}
