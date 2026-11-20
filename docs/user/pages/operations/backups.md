# Backups

## Purpose

Backups & Restore — create full or incremental VM backups, track running backup/restore jobs, and restore a VM from a completed backup, either in place or as a new VM.

## When to use it

- To take a one-off backup of a VM before a risky change, or as part of a regular backup routine
- To check on a backup or restore job that's currently running
- To roll a VM back to a previous backup, or clone it into a new VM from that backup

## How to get there

- Route / id: `/app/backups`
- Nav: **Operations → Backups** (sidebar, command palette, or desktop nav)

## Operate from the console (UX)

1. Review the summary tiles: total backups, total size, a breakdown by type (full vs. incremental), and the timestamp of the newest backup.
2. Watch **Active Jobs** — running or queued backup/restore jobs with a live progress bar, refreshed automatically every 5 seconds.
3. **Create Backup** opens a dialog to pick a VM and choose **Full** or **Incremental**; the job is created compressed with a 30-day retention by default.
4. Search the backup table by VM name, type, or status, and review each backup's size, created time, expiry (or "Never"), and status (completed / in progress / failed).
5. **Restore** (enabled only once a backup is `completed`) opens a dialog where you either restore over the original VM, or check **Restore to new VM** and give it a name to clone the backup into a fresh VM instead. Restoring always brings back config and disks; VM runtime state is not restored.
6. **Delete** a backup permanently, after a confirmation dialog warning the action can't be undone.

**Empty / fail:** Error banner, empty table, or failed toast — confirm you are signed in, `zyvor-fabricd` is healthy (`/readyz`), and any backend this page needs (FluxVM, storage, network) is reachable. See [Admin basics](../../admin-basics.md).

**Success:** Live data loads without error; creates/updates appear in the list or detail view and any confirmation toast clears cleanly.

## Related pages

- [Snapshots](snapshots.md)
- [Backup Scheduler](../more-images-migrations-managers/backup-scheduler.md)
- [Replication](replication.md)
- [Site Recovery](site-recovery.md)
- [Migrations](migrations.md)
- [Migration Wizard](migration-wizard.md)
- [Schedules](schedules.md)
- [Getting Started](../../getting-started.md)
- [Page index](../../PAGE_INDEX.md)
