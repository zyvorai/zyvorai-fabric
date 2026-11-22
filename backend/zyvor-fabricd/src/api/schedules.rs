// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: LicenseRef-Zyvor-Production-2.0

use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use chrono::{DateTime, Datelike, Duration, Timelike, Utc};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use uuid::Uuid;

use crate::server::AppState;
use security::{RequireAdmin, RequireRead, RequireWrite};

// ============================================================================
// Data Structures
// ============================================================================

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum VMAction {
    Start,
    Stop,
    Restart,
    Snapshot,
}

/// Run a `Start`/`Stop`/`Restart` schedule action through the active VM
/// driver (FluxVM) instead of
/// shelling directly to a hypervisor CLI. `Snapshot` isn't a
/// driver action — callers handle it separately (qemu-img against the
/// VM's disk file).
pub async fn run_vm_action(
    driver: &Arc<dyn zyvor_fabric_driver_core::VmDriver>,
    action: &VMAction,
    vm_name: &str,
) -> anyhow::Result<()> {
    match action {
        VMAction::Start => driver.start(vm_name).await,
        VMAction::Stop => driver.poweroff(vm_name).await,
        VMAction::Restart => {
            driver.poweroff(vm_name).await?;
            // Brief pause to allow the VM to fully stop before restarting,
            // matching the prior stop-then-start behavior (not an in-guest
            // reboot request).
            tokio::time::sleep(std::time::Duration::from_secs(2)).await;
            driver.start(vm_name).await
        }
        VMAction::Snapshot => anyhow::bail!("Snapshot is not a driver action"),
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ScheduleType {
    Once,
    Daily,
    Weekly,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Schedule {
    pub id: String,
    pub name: String,
    pub vm_name: String,
    pub action: VMAction,
    pub schedule_type: ScheduleType,
    pub time: String,                  // HH:MM format
    pub days_of_week: Option<Vec<u8>>, // 0-6, Sunday = 0
    pub enabled: bool,
    pub created: DateTime<Utc>,
    pub last_run: Option<DateTime<Utc>>,
    pub next_run: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateScheduleRequest {
    pub name: String,
    pub vm_name: String,
    pub action: VMAction,
    pub schedule_type: ScheduleType,
    pub time: String,
    pub days_of_week: Option<Vec<u8>>,
    #[serde(default = "crate::validation::default_true")]
    pub enabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateScheduleRequest {
    pub name: Option<String>,
    pub vm_name: Option<String>,
    pub action: Option<VMAction>,
    pub schedule_type: Option<ScheduleType>,
    pub time: Option<String>,
    pub days_of_week: Option<Vec<u8>>,
    pub enabled: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ExecutionStatus {
    Success,
    Failed,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScheduleHistory {
    pub schedule_id: String,
    pub schedule_name: String,
    pub vm_name: String,
    pub action: String,
    pub executed_at: DateTime<Utc>,
    pub status: ExecutionStatus,
    pub error: Option<String>,
}

// ============================================================================
// Validation Functions
// ============================================================================

fn validate_schedule(req: &CreateScheduleRequest) -> Result<(), String> {
    // Validate time format (HH:MM)
    let parts: Vec<&str> = req.time.split(':').collect();
    if parts.len() != 2 {
        return Err("Time must be in HH:MM format".to_string());
    }

    let hour: Result<u32, _> = parts[0].parse();
    let minute: Result<u32, _> = parts[1].parse();

    match (hour, minute) {
        (Ok(h), Ok(m)) => {
            if h >= 24 {
                return Err("Hour must be between 0 and 23".to_string());
            }
            if m >= 60 {
                return Err("Minute must be between 0 and 59".to_string());
            }
        }
        _ => {
            return Err("Invalid time format".to_string());
        }
    }

    // Validate days_of_week for weekly schedules
    if matches!(req.schedule_type, ScheduleType::Weekly) {
        if let Some(days) = &req.days_of_week {
            if days.is_empty() {
                return Err("Weekly schedules must specify at least one day".to_string());
            }
            for day in days {
                if *day > 6 {
                    return Err(
                        "Day of week must be between 0 (Sunday) and 6 (Saturday)".to_string()
                    );
                }
            }
        } else {
            return Err("Weekly schedules require days_of_week".to_string());
        }
    }

    // Validate VM name is not empty
    if req.vm_name.trim().is_empty() {
        return Err("VM name cannot be empty".to_string());
    }

    // Validate schedule name is not empty
    if req.name.trim().is_empty() {
        return Err("Schedule name cannot be empty".to_string());
    }

    Ok(())
}

// ============================================================================
// Helper Functions
// ============================================================================

/// Public wrapper for background scheduler use
pub fn calculate_next_run_pub(
    schedule_type: &ScheduleType,
    time: &str,
    days_of_week: &Option<Vec<u8>>,
) -> Option<DateTime<Utc>> {
    calculate_next_run(schedule_type, time, days_of_week)
}

fn calculate_next_run(
    schedule_type: &ScheduleType,
    time: &str,
    days_of_week: &Option<Vec<u8>>,
) -> Option<DateTime<Utc>> {
    let now = Utc::now();

    // Parse time (HH:MM)
    let parts: Vec<&str> = time.split(':').collect();
    if parts.len() != 2 {
        return None;
    }

    let hour: u32 = parts[0].parse().ok()?;
    let minute: u32 = parts[1].parse().ok()?;

    match schedule_type {
        ScheduleType::Once => {
            // Schedule for today at the specified time if not passed, otherwise tomorrow
            let mut next = now
                .with_hour(hour)?
                .with_minute(minute)?
                .with_second(0)?
                .with_nanosecond(0)?;

            if next <= now {
                next += Duration::days(1);
            }

            Some(next)
        }
        ScheduleType::Daily => {
            // Schedule for today at the specified time if not passed, otherwise tomorrow
            let mut next = now
                .with_hour(hour)?
                .with_minute(minute)?
                .with_second(0)?
                .with_nanosecond(0)?;

            if next <= now {
                next += Duration::days(1);
            }

            Some(next)
        }
        ScheduleType::Weekly => {
            // Find the next matching day of week
            let days = days_of_week.as_ref()?;
            if days.is_empty() {
                return None;
            }

            // Find next matching day
            for offset in 0..7 {
                let check_date = now + Duration::days(offset);
                let check_weekday = check_date.weekday().num_days_from_sunday() as u8;

                if days.contains(&check_weekday) {
                    let target_time = check_date
                        .with_hour(hour)?
                        .with_minute(minute)?
                        .with_second(0)?
                        .with_nanosecond(0)?;

                    if target_time > now {
                        return Some(target_time);
                    }
                }
            }

            None
        }
    }
}

// ============================================================================
// Schedule Handlers
// ============================================================================

pub async fn list_schedules(
    RequireRead(_claims): RequireRead,
    State(state): State<Arc<AppState>>,
) -> Result<Json<Vec<Schedule>>, (StatusCode, Json<serde_json::Value>)> {
    tracing::debug!("schedules::{}", stringify!(list_schedules));
    let schedules = state
        .store
        .list_entities::<Schedule>("schedules")
        .map_err(|e| {
            tracing::error!("Failed to load schedules: {}", e);
            crate::api_error::json_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                "Failed to load schedules",
            )
        })?;

    Ok(Json(schedules))
}

pub async fn get_schedule(
    RequireRead(_claims): RequireRead,
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<Schedule>, (StatusCode, Json<serde_json::Value>)> {
    tracing::debug!("schedules::{}", stringify!(get_schedule));
    // Load from state store
    let schedule = state
        .store
        .get_entity::<Schedule>("schedules", &id)
        .map_err(|_| {
            crate::api_error::json_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                "Failed to load schedule",
            )
        })?
        .ok_or_else(|| crate::api_error::json_error(StatusCode::NOT_FOUND, "Schedule not found"))?;

    Ok(Json(schedule))
}

pub async fn create_schedule(
    RequireWrite(_claims): RequireWrite,
    State(state): State<Arc<AppState>>,
    Json(req): Json<CreateScheduleRequest>,
) -> Result<(StatusCode, Json<Schedule>), (StatusCode, Json<serde_json::Value>)> {
    tracing::debug!("schedules::{}", stringify!(create_schedule));
    // Validate schedule
    if let Err(err) = validate_schedule(&req) {
        tracing::warn!("Invalid schedule: {}", err);
        return Err(crate::api_error::json_error(StatusCode::BAD_REQUEST, err));
    }

    let next_run = if req.enabled {
        calculate_next_run(&req.schedule_type, &req.time, &req.days_of_week)
    } else {
        None
    };

    let schedule = Schedule {
        id: Uuid::new_v4().to_string(),
        name: req.name,
        vm_name: req.vm_name,
        action: req.action,
        schedule_type: req.schedule_type,
        time: req.time,
        days_of_week: req.days_of_week,
        enabled: req.enabled,
        created: Utc::now(),
        last_run: None,
        next_run,
    };

    // Save to state store
    if let Err(e) = state
        .store
        .save_entity("schedules", &schedule.id, &schedule)
    {
        tracing::error!("Failed to save schedule: {}", e);
        return Err(crate::api_error::json_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "Failed to save schedule",
        ));
    }

    Ok((StatusCode::CREATED, Json(schedule)))
}

pub async fn update_schedule(
    RequireWrite(_claims): RequireWrite,
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    Json(req): Json<UpdateScheduleRequest>,
) -> Result<Json<Schedule>, (StatusCode, Json<serde_json::Value>)> {
    tracing::debug!("schedules::{}", stringify!(update_schedule));
    // Load existing schedule from state store
    let mut schedule = state
        .store
        .get_entity::<Schedule>("schedules", &id)
        .map_err(|_| {
            crate::api_error::json_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                "Failed to load schedule",
            )
        })?
        .ok_or_else(|| crate::api_error::json_error(StatusCode::NOT_FOUND, "Schedule not found"))?;

    let mut recalculate_next_run = false;

    // Update fields if provided
    if let Some(name) = req.name {
        if name.trim().is_empty() {
            return Err(crate::api_error::json_error(
                StatusCode::BAD_REQUEST,
                "Schedule name cannot be empty",
            ));
        }
        schedule.name = name;
    }
    if let Some(vm_name) = req.vm_name {
        if vm_name.trim().is_empty() {
            return Err(crate::api_error::json_error(
                StatusCode::BAD_REQUEST,
                "VM name cannot be empty",
            ));
        }
        schedule.vm_name = vm_name;
    }
    if let Some(action) = req.action {
        schedule.action = action;
    }
    if let Some(schedule_type) = req.schedule_type {
        schedule.schedule_type = schedule_type;
        recalculate_next_run = true;
    }
    if let Some(time) = req.time {
        // Validate time format
        let parts: Vec<&str> = time.split(':').collect();
        if parts.len() != 2 {
            return Err(crate::api_error::json_error(
                StatusCode::BAD_REQUEST,
                "Time must be in HH:MM format",
            ));
        }
        schedule.time = time;
        recalculate_next_run = true;
    }
    if let Some(days_of_week) = req.days_of_week {
        schedule.days_of_week = Some(days_of_week);
        recalculate_next_run = true;
    }
    if let Some(enabled) = req.enabled {
        schedule.enabled = enabled;
        recalculate_next_run = true;
    }

    // Recalculate next_run if time/schedule_type changed
    if recalculate_next_run {
        schedule.next_run = if schedule.enabled {
            calculate_next_run(
                &schedule.schedule_type,
                &schedule.time,
                &schedule.days_of_week,
            )
        } else {
            None
        };
    }

    // Save to state store
    if let Err(e) = state
        .store
        .save_entity("schedules", &schedule.id, &schedule)
    {
        tracing::error!("Failed to update schedule: {}", e);
        return Err(crate::api_error::json_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "Failed to update schedule",
        ));
    }

    Ok(Json(schedule))
}

pub async fn delete_schedule(
    RequireAdmin(_claims): RequireAdmin,
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<StatusCode, (StatusCode, Json<serde_json::Value>)> {
    tracing::debug!("schedules::{}", stringify!(delete_schedule));
    // Remove from state store
    if let Err(e) = state.store.delete_entity("schedules", &id) {
        tracing::error!("Failed to delete schedule: {}", e);
        return Err(crate::api_error::json_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "Failed to delete schedule",
        ));
    }

    Ok(StatusCode::NO_CONTENT)
}

pub async fn enable_schedule(
    RequireWrite(_claims): RequireWrite,
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<StatusCode, (StatusCode, Json<serde_json::Value>)> {
    tracing::debug!("schedules::{}", stringify!(enable_schedule));
    // Load schedule from state store
    let mut schedule = state
        .store
        .get_entity::<Schedule>("schedules", &id)
        .map_err(|_| {
            crate::api_error::json_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                "Failed to load schedule",
            )
        })?
        .ok_or_else(|| crate::api_error::json_error(StatusCode::NOT_FOUND, "Schedule not found"))?;

    // Set enabled = true
    schedule.enabled = true;

    // Calculate next_run
    schedule.next_run = calculate_next_run(
        &schedule.schedule_type,
        &schedule.time,
        &schedule.days_of_week,
    );

    // Save to state store
    if let Err(e) = state
        .store
        .save_entity("schedules", &schedule.id, &schedule)
    {
        tracing::error!("Failed to enable schedule: {}", e);
        return Err(crate::api_error::json_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "Failed to enable schedule",
        ));
    }

    Ok(StatusCode::OK)
}

pub async fn disable_schedule(
    RequireWrite(_claims): RequireWrite,
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<StatusCode, (StatusCode, Json<serde_json::Value>)> {
    tracing::debug!("schedules::{}", stringify!(disable_schedule));
    // Load schedule from state store
    let mut schedule = state
        .store
        .get_entity::<Schedule>("schedules", &id)
        .map_err(|_| {
            crate::api_error::json_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                "Failed to load schedule",
            )
        })?
        .ok_or_else(|| crate::api_error::json_error(StatusCode::NOT_FOUND, "Schedule not found"))?;

    // Set enabled = false
    schedule.enabled = false;

    // Clear next_run
    schedule.next_run = None;

    // Save to state store
    if let Err(e) = state
        .store
        .save_entity("schedules", &schedule.id, &schedule)
    {
        tracing::error!("Failed to disable schedule: {}", e);
        return Err(crate::api_error::json_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "Failed to disable schedule",
        ));
    }

    Ok(StatusCode::OK)
}

pub async fn run_schedule_now(
    RequireWrite(_claims): RequireWrite,
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<StatusCode, (StatusCode, Json<serde_json::Value>)> {
    tracing::debug!("schedules::{}", stringify!(run_schedule_now));
    // Load schedule from state store
    let mut schedule = state
        .store
        .get_entity::<Schedule>("schedules", &id)
        .map_err(|_| {
            crate::api_error::json_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                "Failed to load schedule",
            )
        })?
        .ok_or_else(|| crate::api_error::json_error(StatusCode::NOT_FOUND, "Schedule not found"))?;

    // Execute the scheduled action immediately (call VM API)
    tracing::info!(
        "Executing schedule {} immediately: {:?} on VM {}",
        schedule.name,
        schedule.action,
        schedule.vm_name
    );

    let vm_name_clone = schedule.vm_name.clone();
    let result = if schedule.action == VMAction::Snapshot {
        // Resolved before spawn_blocking: get_disk_path is async (goes
        // through the driver, not just a filesystem guess), but the
        // qemu-img call itself is fine to run as a blocking subprocess.
        let image_path = state.driver.get_disk_path(&vm_name_clone).await;
        tokio::task::spawn_blocking(move || {
            let snap_name = format!("scheduled-{}", chrono::Utc::now().format("%Y%m%d-%H%M%S"));
            match image_path {
                Ok(path) => {
                    let path = path.display().to_string();
                    let output = std::process::Command::new("qemu-img")
                        .args(["snapshot", "-c", &snap_name, &path])
                        .output();
                    match output {
                        Ok(o) if o.status.success() => Ok(()),
                        Ok(o) => Err(anyhow::anyhow!(
                            "qemu-img snapshot failed: {}",
                            String::from_utf8_lossy(&o.stderr)
                        )),
                        Err(e) => Err(anyhow::anyhow!("Failed to run qemu-img: {}", e)),
                    }
                }
                Err(e) => Err(anyhow::anyhow!(
                    "No disk image found for VM '{}': {}",
                    vm_name_clone,
                    e
                )),
            }
        })
        .await
        .unwrap_or_else(|e| Err(anyhow::anyhow!("Task panicked: {}", e)))
    } else {
        run_vm_action(&state.driver, &schedule.action, &vm_name_clone).await
    };

    // Check if execution was successful
    let (success, error) = match result {
        Ok(_) => (true, None),
        Err(e) => (false, Some(e.to_string())),
    };

    if !success {
        tracing::error!("Failed to execute schedule {}: {:?}", schedule.name, error);
    }

    let executed_at = Utc::now();

    // Update last_run
    schedule.last_run = Some(executed_at);

    // Save to state store
    if let Err(e) = state
        .store
        .save_entity("schedules", &schedule.id, &schedule)
    {
        tracing::error!("Failed to update schedule last_run: {}", e);
        return Err(crate::api_error::json_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "Failed to update schedule",
        ));
    }

    // Add entry to history
    let action_str = match schedule.action {
        VMAction::Start => "start",
        VMAction::Stop => "stop",
        VMAction::Restart => "restart",
        VMAction::Snapshot => "snapshot",
    };

    let history_entry = ScheduleHistory {
        schedule_id: schedule.id.clone(),
        schedule_name: schedule.name.clone(),
        vm_name: schedule.vm_name.clone(),
        action: action_str.to_string(),
        executed_at,
        status: if success {
            ExecutionStatus::Success
        } else {
            ExecutionStatus::Failed
        },
        error: error.clone(),
    };

    let history_id = Uuid::new_v4().to_string();
    if let Err(e) = state
        .store
        .save_entity("schedule_history", &history_id, &history_entry)
    {
        tracing::error!("Failed to save schedule history: {}", e);
        // Don't fail the request if history fails
    }

    Ok(StatusCode::OK)
}

// ============================================================================
// History Handlers
// ============================================================================

pub async fn get_schedule_history(
    RequireRead(_claims): RequireRead,
    State(state): State<Arc<AppState>>,
    Path(schedule_id): Path<String>,
) -> Result<Json<Vec<ScheduleHistory>>, (StatusCode, Json<serde_json::Value>)> {
    tracing::debug!("schedules::{}", stringify!(get_schedule_history));
    // Load history for specific schedule from state store
    let all_history = state
        .store
        .list_entities::<ScheduleHistory>("schedule_history")
        .unwrap_or_default();

    // Filter by schedule_id and sort by execution time (most recent first)
    let mut history: Vec<ScheduleHistory> = all_history
        .into_iter()
        .filter(|h| h.schedule_id == schedule_id)
        .collect();

    history.sort_by_key(|a| std::cmp::Reverse(a.executed_at));

    // Limit to last 100 entries
    history.truncate(100);

    Ok(Json(history))
}

pub async fn get_all_schedule_history(
    RequireRead(_claims): RequireRead,
    State(state): State<Arc<AppState>>,
) -> Result<Json<Vec<ScheduleHistory>>, (StatusCode, Json<serde_json::Value>)> {
    tracing::debug!("schedules::{}", stringify!(get_all_schedule_history));
    // Load all history from state store
    let mut history = state
        .store
        .list_entities::<ScheduleHistory>("schedule_history")
        .unwrap_or_default();

    // Sort by execution time (most recent first)
    history.sort_by_key(|a| std::cmp::Reverse(a.executed_at));

    // Limit to last 100 entries
    history.truncate(100);

    Ok(Json(history))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_req(
        name: &str,
        vm: &str,
        action: VMAction,
        stype: ScheduleType,
        time: &str,
        days: Option<Vec<u8>>,
    ) -> CreateScheduleRequest {
        CreateScheduleRequest {
            name: name.to_string(),
            vm_name: vm.to_string(),
            action,
            schedule_type: stype,
            time: time.to_string(),
            days_of_week: days,
            enabled: true,
        }
    }

    #[test]
    fn test_validate_valid_daily() {
        let req = make_req(
            "backup",
            "web-01",
            VMAction::Stop,
            ScheduleType::Daily,
            "23:30",
            None,
        );
        assert!(validate_schedule(&req).is_ok());
    }

    #[test]
    fn test_validate_invalid_time() {
        let req = make_req(
            "x",
            "vm",
            VMAction::Start,
            ScheduleType::Daily,
            "25:00",
            None,
        );
        assert!(validate_schedule(&req).is_err());
        let req2 = make_req("x", "vm", VMAction::Start, ScheduleType::Daily, "abc", None);
        assert!(validate_schedule(&req2).is_err());
    }

    #[test]
    fn test_validate_weekly_no_days() {
        let req = make_req(
            "x",
            "vm",
            VMAction::Start,
            ScheduleType::Weekly,
            "10:00",
            None,
        );
        assert!(validate_schedule(&req).is_err());
    }

    #[test]
    fn test_validate_weekly_invalid_day() {
        let req = make_req(
            "x",
            "vm",
            VMAction::Start,
            ScheduleType::Weekly,
            "10:00",
            Some(vec![7]),
        );
        assert!(validate_schedule(&req).is_err());
    }

    #[test]
    fn test_validate_empty_vm_name() {
        let req = make_req(
            "sched",
            " ",
            VMAction::Stop,
            ScheduleType::Daily,
            "10:00",
            None,
        );
        assert!(validate_schedule(&req).is_err());
    }

    #[test]
    fn test_next_run_daily() {
        let next = calculate_next_run(&ScheduleType::Daily, "12:00", &None);
        assert!(next.is_some());
        let dt = next.unwrap();
        assert!(dt > chrono::Utc::now());
        assert_eq!(dt.minute(), 0);
    }

    #[test]
    fn test_next_run_weekly_empty_days() {
        let next = calculate_next_run(&ScheduleType::Weekly, "12:00", &Some(vec![]));
        assert!(next.is_none());
    }
}
