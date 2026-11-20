# Service level objectives

Issue #17. Numbers marked **Target** are design goals. Numbers marked **Measured** come from `benchmarks/` on the hardware named in the baseline file. FluxVM-dependent figures must be labeled as such.

## Control plane (zyvor-fabricd)

| SLO | Target | Window | Measured |
|-----|--------|--------|----------|
| `GET /health` availability | 99.9% | 30 d | not yet from production telemetry |
| `GET /readyz` success when store + FluxVM up | 99.9% | 30 d | CI smoke only |
| API p99 (auth'd JSON, in-process / local) | ≤ 50 ms | 5 min bench | see `benchmarks/baselines/` |
| Concurrent inventory list (`GET /api/vms`) p99 | ≤ 200 ms at 500 VMs | bench | harness `inventory` mode |

## VM lifecycle (FluxVM-dependent — label every published number)

| SLO | Target | Notes |
|-----|--------|-------|
| Cold start (pre-pulled qcow2, 2 vCPU / 2 GiB) | p50 ≤ 8 s, p99 ≤ 20 s | FluxVM + image cache |
| Stop → stopped | p99 ≤ 10 s | ACPI then force |
| Snapshot disk-only (qcow2 internal) | p99 ≤ 30 s | not full memory dump |
| Backup create 10 GiB disk, local, gzip | p99 ≤ 180 s | `backup` crate path |

## Failover / maintenance

| Objective | Target | Evidence |
|-----------|--------|----------|
| Host evacuate plan (no I/O) | deterministic, largest-first | `host-lifecycle` unit tests |
| Mid-migration failure | job `failed`, source **stays cordoned** | `migration_failure_marks_job_failed_and_leaves_host_cordoned` |
| Quorum loss (file heartbeat majority) | `check_quorum` → false at ≤ majority | `fault-tolerance::quorum` tests |
| RTO after node death with shared storage + FT enabled | **unmeasured** — do not quote | needs chaos lab (#15 live track) |
| RPO for incremental backup | last successful backup timestamp | metadata in `backup` crate |

## Error budget policy

- Publish a baseline JSON before claiming a number in README or sales decks.
- FluxVM-dependent rows must include `fluxvm_version` and `labeled_fluxvm_dependent: true`.
- If a live bench cannot run (no daemon), the harness writes `status: offline` and must not be copied into FEATURES.md as a measurement.
