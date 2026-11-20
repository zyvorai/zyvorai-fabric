# VM Migration

Zyvor Fabric supports two migration paths:

| Path | What it does | Status |
|------|----------------|--------|
| **Disk copy (`/api/migrations`)** | Copies disk + config over SSH with `rsync`. No shared storage; no guest memory transfer. | Production for host drain / rebalance when SSH + `rsync` are available |
| **Native FluxVM (`…/migration/native/*`)** | VMM live-migration transport + target **receivers** (shared/identical disk, memory cutover). | **Preview** until a green KVM shared-disk e2e — see [FLUXVM-FABRIC-BOUNDARY.md](FLUXVM-FABRIC-BOUNDARY.md) |

`live` and `offline` on `/api/migrations` are two modes of the **same rsync-based copy**, not QEMU live migration:

- **Offline** — stop the VM, `rsync` its data to the target, done.
- **Live** — `rsync` in the background while the VM runs, then pause briefly for a final sync and cutover. Downtime is the last sync, not the whole transfer.

For native transport (receivers, prepare/start/status/cancel), use the APIs below and `fabricctl runtime migrate …`. Do **not** market native live migration as GA without the KVM e2e gate.

---

## Start a disk-copy migration

```bash
curl -X POST http://localhost:9095/api/migrations \
  -H "Content-Type: application/json" \
  -d '{
    "vm_name": "myvm",
    "target_host": "node2",
    "migration_type": "live",
    "compress": true,
    "bandwidth_mbps": 100
  }'
```

`migration_type` is `"live"`, `"offline"`, or `"storage"`. `compress` and `bandwidth_mbps` are both
optional and map straight to `rsync -z` / `rsync --bwlimit`. `target_host` must be reachable over
SSH (key-based, no password prompt) as whatever user runs `zyvor-fabricd` -- that reachability is
checked before anything else happens.

## Track and Cancel

```bash
curl http://localhost:9095/api/migrations              # all migrations
curl http://localhost:9095/api/migrations/{id}          # one migration's status
curl -X POST http://localhost:9095/api/migrations/{id}/cancel
```

## Status Shape

```json
{
  "id": "…",
  "vm_name": "myvm",
  "target_host": "node2",
  "migration_type": "live",
  "state": "syncing",
  "progress_percent": 60,
  "bytes_transferred": 1932735283,
  "started": "2026-09-02T02:00:00Z",
  "completed": null,
  "error": null
}
```

`state` moves through `pending` -> `precheck` -> `syncing` -> `switching` -> `completed` (or
`failed` / `cancelled`).

Two related read-only endpoints: `GET /api/migrations/history` (past migrations) and
`GET /api/migrations/readiness` (checks whether `rsync` and SSH connectivity are actually available
on this host before you try).

---

## Native migration (preview)

Fabric proxies FluxVM receivers and source-side transport:

| Fabric API | Role |
|---|---|
| `POST /api/vms/{name}/migration/native/prepare-receiver` | Arm target incoming QEMU |
| `POST /api/vms/{name}/migration/native/start` | Start prepared-target transport |
| `GET /api/vms/{name}/migration/native/status` | Poll progress |
| `POST /api/vms/{name}/migration/native/cancel` | Cancel in-flight transport |
| `GET /api/vms/{name}/migration/native/network-state` | Dataplane migration phase |
| `POST /api/migration/receivers/{id}/activate` | Promote receiver after cutover |
| `DELETE /api/migration/receivers/{id}` | Abort unused receiver |

CLI: `fabricctl runtime capabilities` and `fabricctl runtime migrate …`. Full sequence and ownership: [FLUXVM-FABRIC-BOUNDARY.md](FLUXVM-FABRIC-BOUNDARY.md).

---

## Known Gap: Target-Side Start (rsync path)

Pausing the source VM for the final sync goes through the FluxVM `VmDriver` on
this host. Starting the VM on the *target* node after cutover shells
`ssh <target> fabricctl start <vm>` (machinectl was removed). Ensure `fabricctl`
is installed on the target and can reach local Fabric/FluxVM; otherwise start
the VM via `POST /api/vms/{name}/start` on the target Fabric API.

---

## Requirements (rsync path)

- Key-based SSH from the source host to the target host (no shared storage, no cluster membership).
- `rsync` installed on both ends.
- Enough free disk on the target for the VM's full disk image.

---

## Prometheus Metrics

> **Not yet implemented.** The daemon's Prometheus exporter currently only exports VM-count and
> lifecycle metrics (`zyvor_fabricd_vms_total`, `_vms_running`, `_vms_stopped`,
> `_vm_{starts,stops,creates,deletes}_total`) -- no migration-specific series exist yet. Poll
> `GET /api/migrations` for live progress in the meantime (see the JSON example above).

---

## Troubleshooting

| Problem | Solution |
|---------|----------|
| Migration fails at the pre-check step | Confirm `ssh <target_host> echo ok` works non-interactively (key-based, `BatchMode=yes`) |
| Migration fails during sync | Check `journalctl -u zyvor-fabricd`, verify `rsync` is installed on both hosts |
| Insufficient disk space | Check `df -h /var/lib/zyvor-fabricd` on the target node |
| VM won't start on the target after cutover | See [Known Gap](#known-gap-target-side-start-rsync-path) above -- this step isn't driver-generic yet |
| Native prepare/start fails | Confirm FluxVM receivers API, shared/identical disks, and preview status in the boundary doc |
