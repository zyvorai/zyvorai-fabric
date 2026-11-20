# Backup Scheduler

## Purpose

Backup Scheduler — create and manage recurring, automated backup jobs that snapshot one or more VMs' disks to a directory on a schedule, with retention and format controls.

Companion to one-off [Backups](../operations/backups.md). Schedules land under a host directory (default `/var/lib/zyvor-fabricd/backups`).

## When to use it

- To set up nightly or weekly backups for a group of VMs instead of exporting disks by hand
- To back up several VMs on a shared cadence (e.g. every 6 hours) with a single schedule
- To control how much backup history is kept, and in what disk format, without babysitting the job
- Prefer this page when the job matches the purpose above
- When Operations → Backups covers ad-hoc jobs but you need recurrence + retention

## How to get there

- Route / id: `/backup-scheduler`
- Nav: **More — images, migrations & managers → Backup Scheduler** (sidebar, command palette, or desktop nav)

## Operate from the console (UX)

**Select VMs** — check VMs (state shown); **Select All** / **Deselect All**.

**Schedule Configuration**

- **Schedule Name** (e.g. `nightly-backup`)
- **Frequency** — Daily 2 AM, Weekly Sun 3 AM, Every 6h, or Custom cron (plain-English readout)
- **Output Directory** (default `/var/lib/zyvor-fabricd/backups`)
- **Retention (keep last N)** — 1–365
- **Format** — QCOW2, RAW, or VMDK
- **Enable compression** toggle

**Create Schedule** requires ≥1 VM and a name. **Existing Schedules** lists enabled/disabled, cron (raw + human), VM count, next run when available.

Typical flow: select critical VMs → Daily 2 AM + retention → Create → confirm next run → restore still via Operations → Backups when needed. Watch jobs on Job Monitor if a run fails.

**Empty / fail:** Error banner, empty table, or failed toast — confirm you are signed in, `zyvor-fabricd` is healthy (`/readyz` on `http://127.0.0.1:<port>` or `https://<host>`), and any backend this page needs (FluxVM, storage, network) is reachable. See [Admin basics](../../admin-basics.md).

**Success:** Live data loads without error; creates/updates appear in the list or detail view and any confirmation toast clears cleanly.

## Related pages

- [Backups](../operations/backups.md)
- [Snapshots](../operations/snapshots.md)
- [Snapshot Mgr](snapshot-manager.md)
- [Schedules](../operations/schedules.md)
- [Replication](../operations/replication.md)
- [Job Monitor](job-monitor.md)
- [Pipeline](pipeline.md)
- [Getting Started](../../getting-started.md)
- [Page index](../../PAGE_INDEX.md)
