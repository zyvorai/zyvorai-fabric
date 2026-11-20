# Chaos / HA qualification

Issue #15. Two tracks:

1. **In-tree (CI)** — no etcd, no SSH, no live QEMU. Deterministic fail-safe tests.
2. **Live lab** — `scripts/chaos-qualify.sh` against a running fabricd + FluxVM.

## In-tree scenarios

| Scenario | Expected fail-safe | Test |
|----------|-------------------|------|
| Interrupted / corrupt backup archive | restore returns error; target tree not treated as success | `backup::interrupted_archive_does_not_restore` |
| Backup round-trip | config + disk bytes restored | `backup::create_list_restore_delete_roundtrip` |
| Quorum: 1 of 3 heartbeats fresh | `check_quorum` is false | `fault-tolerance::quorum::minority_alive_loses_quorum` |
| Quorum: 2 of 3 fresh | true | `majority_alive_keeps_quorum` |
| Stale HA heartbeat (>30s) | node unhealthy | `ha::stale_heartbeat_marks_node_unhealthy` |
| Evacuation migration fails mid-job | job Failed, host left cordoned, no uncordon | `host-lifecycle::migration_failure_marks_job_failed_and_leaves_host_cordoned` |
| Invalid VM name on migrate cancel | error, no pkill | `migration` name validation (live script) |

## Live lab scenarios (`scripts/chaos-qualify.sh`)

These are **skipped** unless `FABRIC_URL` is reachable.

| Scenario | Inject | Expected |
|----------|--------|----------|
| fabricd kill mid-start | `kill -9` during `POST /api/vms/:name/start` | `/readyz` recovers after restart; VM not left "Starting" forever (driver owns state) |
| FluxVM down | stop FluxVM process | `/readyz` `fluxvm.ok=false`; create/start fail closed |
| Disk full | `backup` dest on tiny tmpfs | create_backup errors; no truncated-success metadata |
| Interrupted snapshot | SIGSTOP fabricd during snapshot | 409/5xx; disk-only snapshot does not advertise Completed |

Record outcomes under `docs/proven-infra/runs/`. Do not mark FEATURES.md "HA clustering" as proven from track 1 alone.
