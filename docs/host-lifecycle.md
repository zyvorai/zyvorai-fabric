# Host Lifecycle: Maintenance and Evacuation

Fabric already has host inventory, a `HostStatus::Maintenance` state, and lifecycle remediation models. The missing operational layer is the part that safely moves workloads before a host is taken out of service.

This change adds a dedicated `host-lifecycle` crate that owns that orchestration logic without hard-coding it to a specific hypervisor transport.

## Goals

- Prevent new placement on a host before evacuation starts.
- Preflight every workload before touching production state.
- Merion capacity headroom on evacuation targets.
- Respect cluster boundaries by default.
- Respect workload label requirements and hard host pins.
- Select live or cold migration from an explicit policy.
- Run migrations with bounded concurrency.
- Enter maintenance only after every planned migration succeeds.
- Leave a partially evacuated host cordoned on failure so the scheduler cannot race operator recovery.
- Return the host to service explicitly after maintenance is complete.

## State model

```text
                   +-----------+
                   |  BLOCKED  |
                   +-----------+
                         |
                       cancel
                         v
                    CANCELLED

PLANNED -> RUNNING -> MAINTENANCE -> COMPLETED
              |
              +----------------------> FAILED
```

A job only reaches `MAINTENANCE` after all workload assignments have completed successfully and the backend confirms the host entered maintenance mode.

## Preflight blockers

The planner emits stable blocker codes suitable for the API, CLI and UI:

| Code | Meaning |
| --- | --- |
| `source_host_unavailable` | Source host is neither connected nor already in maintenance. |
| `workload_not_on_source` | Inventory is inconsistent with the requested source host. |
| `live_migration_required` | Policy is `live_only` but the workload cannot live-migrate. |
| `pinned_to_source` | A hard host pin prevents evacuation. |
| `pinned_target_unavailable` | A workload is pinned to a target that is unavailable/ineligible. |
| `no_eligible_target` | Cluster/status/labels leave no target host. |
| `insufficient_capacity` | Eligible hosts cannot fit the workload after reserve headroom. |

A blocked plan is still returned in full so the UI can explain exactly what the operator must fix.

## Placement behavior

The planner uses deterministic largest-first placement:

1. Filter targets by host status, scheduling gate and cluster boundary.
2. Apply CPU and memory reserve percentages.
3. Sort workloads by memory, then CPU, largest first.
4. Apply hard host pinning and required labels.
5. Select the eligible target with the most remaining memory, then CPU.
6. Deduct the workload's requested capacity before placing the next workload.

This is intentionally deterministic so repeated preflight calls against the same inventory produce the same plan.

## Policy example

```json
{
  "strategy": "prefer_live",
  "max_parallel": 2,
  "reserve_cpu_percent": 10,
  "reserve_memory_percent": 10,
  "allow_cross_cluster": false,
  "allow_pinned": false,
  "dry_run": false
}
```

### Strategies

- `live_only`: block any VM that cannot live-migrate.
- `prefer_live`: live-migrate capable VMs and cold-migrate the rest.
- `cold_only`: always use cold migration.

## Backend integration

`EvacuationExecutor` is the transport boundary:

```rust
#[async_trait]
pub trait EvacuationExecutor: Send + Sync {
    async fn cordon_host(&self, host_id: &str) -> Result<(), String>;
    async fn migrate(&self, assignment: &EvacuationAssignment) -> Result<(), String>;
    async fn enter_maintenance(&self, host_id: &str) -> Result<(), String>;
    async fn exit_maintenance(&self, host_id: &str) -> Result<(), String>;
    async fn uncordon_host(&self, host_id: &str) -> Result<(), String>;
}
```

A Fabric server adapter can map these operations to the host agent / FluxVM transport without putting transport concerns into the planner.

## Suggested REST wiring

The new crate is designed for these endpoints:

```text
POST /api/hosts/{id}/maintenance/plan
POST /api/hosts/{id}/maintenance/execute
GET  /api/hosts/{id}/maintenance/jobs
GET  /api/maintenance/jobs/{job_id}
POST /api/maintenance/jobs/{job_id}/cancel
POST /api/maintenance/jobs/{job_id}/complete
```

`plan` should accept a dry-run policy and persist the serialized `MaintenanceJob` to Fabric's `StateStore`. The server should rebuild a manager from persisted jobs on restart or introduce a `JobRepository` adapter in the next integration slice.

## Failure semantics

The source is cordoned before migration begins. If any migration fails:

- that assignment becomes `failed`;
- the overall job becomes `failed`;
- remaining buffered migrations are cancelled when the execution stream is dropped;
- the host stays cordoned intentionally;
- the error is retained on both the failed assignment and the job.

Fabric should not automatically uncordon a partially evacuated host because doing so can allow new workloads onto a host while an operator is recovering split placement.

## Tests included

The crate covers:

- deterministic capacity-aware placement;
- live-only blocking;
- pinned source protection;
- label constraints;
- reserve headroom;
- invalid policy rejection;
- successful async evacuation and return-to-service;
- migration failure behavior;
- duplicate active job prevention;
- blocked-job cancellation.

## Follow-up integration

The safest follow-up is to wire this engine into `zyvor-fabricd` and persist `MaintenanceJob` entities in `StateStore`, then expose the routes above and add `fabricctl host maintenance` commands. That wiring should use the existing host-agent/driver APIs rather than introduce a second VM control transport.
