// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: LicenseRef-Zyvor-Production-2.0

use futures_util::StreamExt;
use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use tauri::{AppHandle, Emitter, State};
use zyvor_fabric_sdk::{Client, ClientConfig};

#[derive(Serialize)]
struct VmSummary {
    name: String,
    state: String,
    cpus: u32,
    memory: u64,
}

#[derive(Deserialize)]
struct CopilotRequest {
    endpoint: String,
    token: Option<String>,
    question: String,
}

struct EventStreamState {
    cancel: Arc<AtomicBool>,
}

pub struct FabricRuntime {
    stream: Mutex<Option<EventStreamState>>,
}

impl FabricRuntime {
    fn new() -> Self {
        Self {
            stream: Mutex::new(None),
        }
    }
}

fn client(endpoint: &str, token: Option<String>) -> Result<Client, String> {
    Client::new(ClientConfig {
        endpoint: endpoint.to_string(),
        token,
    })
    .map_err(|e| e.to_string())
}

#[tauri::command]
async fn fabric_health(endpoint: String, token: Option<String>) -> Result<String, String> {
    let client = client(&endpoint, token)?;
    client.health().await.map_err(|e| e.to_string())
}

#[tauri::command]
async fn fabric_list_vms(endpoint: String, token: Option<String>) -> Result<Vec<VmSummary>, String> {
    let client = client(&endpoint, token)?;
    let vms = client.list_vms().await.map_err(|e| e.to_string())?;
    Ok(vms
        .into_iter()
        .map(|vm| VmSummary {
            name: vm.name,
            state: format!("{:?}", vm.state),
            cpus: vm.cpus,
            memory: vm.memory,
        })
        .collect())
}

#[tauri::command]
async fn fabric_start_events(
    app: AppHandle,
    runtime: State<'_, FabricRuntime>,
    endpoint: String,
    token: Option<String>,
) -> Result<(), String> {
    {
        let mut guard = runtime.stream.lock().map_err(|e| e.to_string())?;
        if let Some(handle) = guard.take() {
            handle.cancel.store(true, Ordering::Relaxed);
        }
    }
    let client = client(&endpoint, token)?;
    let mut stream = client.stream_events().await.map_err(|e| e.to_string())?;
    let cancel = Arc::new(AtomicBool::new(false));
    let worker_cancel = cancel.clone();
    *runtime.stream.lock().map_err(|e| e.to_string())? = Some(EventStreamState {
        cancel: cancel.clone(),
    });
    tokio::spawn(async move {
        loop {
            if worker_cancel.load(Ordering::Relaxed) {
                break;
            }
            match stream.next().await {
                Some(Ok(ev)) => {
                    let _ = app.emit("fabric-event", &ev);
                }
                Some(Err(e)) => {
                    let _ = app.emit("fabric-stream-error", e.to_string());
                    break;
                }
                None => break,
            }
        }
        let _ = app.emit("fabric-stream-stopped", ());
    });
    Ok(())
}

#[tauri::command]
fn fabric_stop_events(runtime: State<'_, FabricRuntime>) -> Result<(), String> {
    let mut guard = runtime.stream.lock().map_err(|e| e.to_string())?;
    if let Some(handle) = guard.take() {
        handle.cancel.store(true, Ordering::Relaxed);
    }
    Ok(())
}

fn vm_name_in_question(question: &str, names: &[String]) -> Option<String> {
    let q = question.to_lowercase();
    names
        .iter()
        .filter(|name| q.contains(&name.to_lowercase()))
        .max_by_key(|name| name.len())
        .cloned()
}

#[tauri::command]
async fn fabric_copilot(req: CopilotRequest) -> Result<String, String> {
    let client = client(&req.endpoint, req.token)?;
    let question = req.question.trim();
    if question.is_empty() {
        return Ok("Ask about VMs, events, or cluster health.".into());
    }
    let q = question.to_lowercase();
    let health = client.health().await.unwrap_or_else(|_| "unknown".into());
    let vms = client.list_vms().await.map_err(|e| e.to_string())?;
    let events = client.list_events().await.unwrap_or_default();

    if q.contains("health") || q.contains("status") {
        return Ok(format!(
            "Cluster health is `{health}`. {count} VM(s) registered.",
            health = health,
            count = vms.len()
        ));
    }

    if q.contains("unhealthy") || q.contains("failed") || q.contains("error") {
        let stopped: Vec<_> = vms
            .iter()
            .filter(|v| !format!("{:?}", v.state).to_lowercase().contains("running"))
            .map(|v| v.name.as_str())
            .collect();
        let recent_errors: Vec<_> = events
            .iter()
            .filter(|e| e.event_type.to_lowercase().contains("error"))
            .take(5)
            .map(|e| format!("{} ({})", e.vm_name, e.event_type))
            .collect();
        if stopped.is_empty() && recent_errors.is_empty() {
            return Ok("No unhealthy VMs or recent error events detected.".into());
        }
        return Ok(format!(
            "Potential issues:\n- Non-running VMs: {}\n- Recent error events: {}",
            if stopped.is_empty() {
                "none".into()
            } else {
                stopped.join(", ")
            },
            if recent_errors.is_empty() {
                "none".into()
            } else {
                recent_errors.join("; ")
            }
        ));
    }

    if q.contains("slow") || q.contains("cpu") || q.contains("performance") || q.contains("memory") {
        let names: Vec<String> = vms.iter().map(|v| v.name.clone()).collect();
        let target = vm_name_in_question(question, &names).or_else(|| {
            vms.iter()
                .find(|v| format!("{:?}", v.state).to_lowercase().contains("running"))
                .map(|v| v.name.clone())
        });
        if let Some(name) = target {
            match client.vm_metrics(&name).await {
                Ok(metrics) => {
                    return Ok(format!(
                        "Metrics for `{name}`:\n{}",
                        serde_json::to_string_pretty(&metrics.raw).unwrap_or_else(|_| "{}".into())
                    ));
                }
                Err(e) => {
                    return Ok(format!(
                        "Could not load metrics for `{name}`: {e}. \
                         Ensure the VM is running and metrics are enabled on the Fabric host."
                    ));
                }
            }
        }
        return Ok(
            "Name a VM in your question (e.g. \"why is web-01 slow?\") or ensure at least one VM is running. \
             You can also run `machina-fabric vms metrics <name>`."
                .into(),
        );
    }

    if q.contains("event") || q.contains("what changed") || q.contains("outage") {
        let lines: Vec<String> = events
            .iter()
            .take(8)
            .map(|e| {
                format!(
                    "- {} {} {} {}",
                    e.timestamp,
                    e.event_type,
                    e.vm_name,
                    e.detail.as_deref().unwrap_or("")
                )
            })
            .collect();
        if lines.is_empty() {
            return Ok("No recent events in the event log.".into());
        }
        return Ok(format!("Recent infrastructure events:\n{}", lines.join("\n")));
    }

    if q.contains("list") || q.contains("how many") || q.contains("vm") {
        if vms.is_empty() {
            return Ok("No VMs found on this cluster.".into());
        }
        let lines: Vec<String> = vms
            .iter()
            .map(|v| format!("- {} ({:?}, {} CPU, {} MB)", v.name, v.state, v.cpus, v.memory))
            .collect();
        return Ok(format!("{count} VM(s):\n{body}", count = vms.len(), body = lines.join("\n")));
    }

    Ok(format!(
        "Zyvor Fabric copilot (v0.1)\n- Health: {health}\n- VMs: {vm_count}\n- Recent events: {event_count}\n\n\
         Try: \"list VMs\", \"any unhealthy VMs?\", \"what changed recently?\"",
        health = health,
        vm_count = vms.len(),
        event_count = events.len()
    ))
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .manage(FabricRuntime::new())
        .invoke_handler(tauri::generate_handler![
            fabric_health,
            fabric_list_vms,
            fabric_start_events,
            fabric_stop_events,
            fabric_copilot,
        ])
        .run(tauri::generate_context!())
        .expect("error while running Machina desktop");
}
