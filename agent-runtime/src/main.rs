// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: Apache-2.0

use anyhow::Result;
use tower_http::trace::TraceLayer;
use zyvor_fabric_agent_runtime::{app, config::Config, egress, pool, AppState};

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    let config = Config::from_env()?;
    let public_addr = config.listen;
    let egress_addr = config.egress_listen;
    let state = AppState::from_config(config).await?;
    zyvor_fabric_agent_runtime::otlp::init(state.egress_http.clone());

    let public = app::public_router(state.clone()).layer(TraceLayer::new_for_http());
    let broker = egress::broker_router(state.clone()).layer(TraceLayer::new_for_http());

    tokio::spawn(app::sync_loop(state.clone()));
    tokio::spawn(app::auto_hibernate_loop(state.clone()));
    tokio::spawn(app::expiry_loop(state.clone()));
    tokio::spawn(app::terminal_cleanup_loop(state.clone()));
    tokio::spawn(zyvor_fabric_agent_runtime::schedules::schedule_loop(
        state.clone(),
    ));
    tokio::spawn(zyvor_fabric_agent_runtime::triggers::trigger_loop(
        state.clone(),
    ));
    tokio::spawn(zyvor_fabric_agent_runtime::workstations::workstation_loop(
        state.clone(),
    ));
    tokio::spawn(pool::warm_pool_loop(state.clone()));
    tokio::spawn(zyvor_fabric_agent_runtime::goal_worker::goal_loop(
        state.clone(),
    ));
    tokio::spawn(zyvor_fabric_agent_runtime::retention::retention_loop(
        state.clone(),
    ));

    let public_listener = tokio::net::TcpListener::bind(public_addr).await?;
    let egress_listener = tokio::net::TcpListener::bind(egress_addr).await?;
    if let Some(proxy_addr) = state.config.proxy_listen {
        let proxy_listener = tokio::net::TcpListener::bind(proxy_addr).await?;
        tracing::info!(%proxy_addr, "Fabric Agent Runtime CONNECT proxy listening");
        tokio::spawn(zyvor_fabric_agent_runtime::proxy::serve(
            state.clone(),
            proxy_listener,
        ));
    }
    tracing::info!(%public_addr, "Fabric Agent Runtime API listening");
    tracing::info!(%egress_addr, "Fabric Agent Runtime egress broker listening");

    tokio::try_join!(
        axum::serve(public_listener, public),
        axum::serve(
            egress_listener,
            broker.into_make_service_with_connect_info::<std::net::SocketAddr>(),
        ),
    )?;
    Ok(())
}
