// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: LicenseRef-Zyvor-Production-1.0

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
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::time::{timeout, Duration};

static ACTIVE_WS_CONNECTIONS: AtomicUsize = AtomicUsize::new(0);
const MAX_WS_CONNECTIONS: usize = 50;

use crate::server::AppState;
use crate::validation::validate_vm_name;

/// Maximum WebSocket message size (64KB)
const MAX_MESSAGE_SIZE: usize = 64 * 1024;

/// Idle timeout for WebSocket connections (5 minutes)
const IDLE_TIMEOUT: Duration = Duration::from_secs(5 * 60);

#[derive(Debug, Deserialize)]
pub struct ConsoleQuery {
    pub token: Option<String>,
    #[serde(default = "default_console_cols")]
    pub cols: u16,
    #[serde(default = "default_console_rows")]
    pub rows: u16,
}
fn default_console_cols() -> u16 {
    80
}
fn default_console_rows() -> u16 {
    24
}

pub async fn console_handler(
    ws: WebSocketUpgrade,
    Path(vm_name): Path<String>,
    Query(query): Query<ConsoleQuery>,
    State(state): State<Arc<AppState>>,
) -> Result<impl IntoResponse, StatusCode> {
    // Check concurrent WebSocket connection limit
    let current = ACTIVE_WS_CONNECTIONS.load(Ordering::Relaxed);
    if current >= MAX_WS_CONNECTIONS {
        tracing::warn!(
            "WebSocket connection limit reached ({}/{})",
            current,
            MAX_WS_CONNECTIONS
        );
        return Err(StatusCode::SERVICE_UNAVAILABLE);
    }

    validate_vm_name(&vm_name).map_err(|_| StatusCode::BAD_REQUEST)?;

    // Validate authentication - reject if auth is not configured
    let jwt_config = match state.jwt_config.as_ref() {
        Some(c) => c,
        None => {
            tracing::warn!("WebSocket console rejected: authentication not configured");
            return Err(StatusCode::UNAUTHORIZED);
        }
    };

    let token = query.token.as_deref().ok_or_else(|| {
        tracing::warn!(
            "WebSocket console connection rejected: no auth token for VM '{}'",
            vm_name
        );
        StatusCode::UNAUTHORIZED
    })?;

    let _claims = jwt_config.validate_token(token).map_err(|e| {
        tracing::warn!("WebSocket auth failed for VM '{}': {}", vm_name, e);
        StatusCode::UNAUTHORIZED
    })?;

    // Require at least write permission for console access
    if !_claims.role.can_write() {
        tracing::warn!(
            "WebSocket console rejected: user '{}' has insufficient permissions",
            _claims.sub
        );
        return Err(StatusCode::FORBIDDEN);
    }

    // Opening the console before upgrading (the old approach) would let a
    // failure come back as a normal HTTP status -- except the browser's
    // `WebSocket` object never exposes the failed handshake's status or
    // body, so that "diagnostic" was invisible to the UI regardless of
    // what the server sent. Upgrading unconditionally and reporting a
    // failure as a text frame instead means the browser's `onmessage`
    // actually sees it before `onclose` fires -- same pattern already used
    // for the idle-timeout notice below.
    Ok(ws
        .max_message_size(MAX_MESSAGE_SIZE)
        .on_upgrade(move |socket| async move {
            match state
                .driver
                .open_console(&vm_name, query.cols, query.rows)
                .await
            {
                Ok(console) => handle_console(socket, vm_name, console).await,
                Err(e) => {
                    tracing::warn!("Failed to open console for VM '{}': {:#}", vm_name, e);
                    send_console_open_error(socket, &vm_name, &e).await;
                }
            }
        }))
}

/// Reports a console-open failure as a readable message inside the
/// terminal itself, then closes -- see `console_handler`'s doc comment for
/// why this replaces a pre-upgrade HTTP error.
async fn send_console_open_error(mut socket: WebSocket, vm_name: &str, err: &anyhow::Error) {
    let hint = if err.to_string().contains("Connection reset by peer")
        || err.to_string().contains("connect(vsock")
    {
        "\r\n[The in-guest agent isn't responding. This usually means the VM's image doesn't \
         have fluxvm-guest-agent installed/enabled, or the guest hasn't finished booting yet.]\r\n"
    } else {
        ""
    };
    let msg = format!("\r\n[Console unavailable for '{vm_name}': {err:#}]\r\n{hint}");
    let _ = socket.send(Message::Text(msg.into())).await;
    let _ = socket.close().await;
}

async fn handle_console(
    socket: WebSocket,
    vm_name: String,
    console: zyvor_fabric_driver_core::ConsoleSession,
) {
    ACTIVE_WS_CONNECTIONS.fetch_add(1, Ordering::Relaxed);
    tracing::info!(
        "WebSocket console connection established for VM: {} (active: {})",
        vm_name,
        ACTIVE_WS_CONNECTIONS.load(Ordering::Relaxed),
    );

    let (mut console_rx, mut console_tx) = tokio::io::split(console);
    let (mut ws_sender, mut ws_receiver) = socket.split();

    // Read from the console and send to WebSocket
    let vm_name_clone = vm_name.clone();
    let console_to_ws = tokio::spawn(async move {
        let mut buf = [0u8; 4096];
        loop {
            match timeout(IDLE_TIMEOUT, console_rx.read(&mut buf)).await {
                Ok(Ok(0)) => break,
                Ok(Ok(n)) => {
                    if ws_sender
                        .send(Message::Binary(buf[..n].to_vec().into()))
                        .await
                        .is_err()
                    {
                        break;
                    }
                }
                Ok(Err(e)) => {
                    tracing::error!("Error reading from console: {}", e);
                    break;
                }
                Err(_) => {
                    tracing::info!("Console idle timeout reached for VM: {}", vm_name_clone);
                    let _ = ws_sender
                        .send(Message::Text(
                            "\r\n[Session timed out due to inactivity]\r\n".into(),
                        ))
                        .await;
                    break;
                }
            }
        }
        tracing::info!("Console-to-WebSocket task ended for VM: {}", vm_name_clone);
    });

    // Read from WebSocket and write to the console
    let vm_name_clone = vm_name.clone();
    let ws_to_console = tokio::spawn(async move {
        loop {
            match timeout(IDLE_TIMEOUT, ws_receiver.next()).await {
                Ok(Some(msg)) => match msg {
                    Ok(Message::Text(text)) => {
                        // `write_all` alone doesn't reach the guest:
                        // `ConsoleWs::poll_write` only queues the frame via
                        // the underlying WS sink's `start_send` — nothing
                        // actually puts it on the wire until `flush`. Found
                        // live: without this, `write_all` reports success
                        // (the queue accepted it) but the guest never sees
                        // a byte, silently and with no error anywhere.
                        if console_tx.write_all(text.as_bytes()).await.is_err()
                            || console_tx.flush().await.is_err()
                        {
                            break;
                        }
                    }
                    Ok(Message::Binary(data)) => {
                        if console_tx.write_all(&data).await.is_err()
                            || console_tx.flush().await.is_err()
                        {
                            break;
                        }
                    }
                    Ok(Message::Close(_)) => break,
                    Err(e) => {
                        tracing::error!("WebSocket error: {}", e);
                        break;
                    }
                    _ => {}
                },
                Ok(None) => break,
                Err(_) => {
                    tracing::info!("Console input idle timeout for VM: {}", vm_name_clone);
                    break;
                }
            }
        }
        tracing::info!("WebSocket-to-console task ended for VM: {}", vm_name_clone);
    });

    // Wait for either direction to end — the other side would otherwise
    // block on a read nothing more is coming from.
    tokio::select! {
        _ = console_to_ws => {}
        _ = ws_to_console => {}
    }

    ACTIVE_WS_CONNECTIONS.fetch_sub(1, Ordering::Relaxed);
    tracing::info!(
        "WebSocket console closed for VM: {} (active: {})",
        vm_name,
        ACTIVE_WS_CONNECTIONS.load(Ordering::Relaxed),
    );
}
