# Snapshot Mgr

## Purpose

Snapshot Manager — create, revert to, and delete disk-state snapshots for a selected VM.

Sibling to Operations → [Snapshots](../operations/snapshots.md). Prefer Disk Only for routine checkpoints; Full (disk+memory) can take minutes under load.

## When to use it

- Before a risky change, to capture disk state for rollback
- To revert a VM to an earlier snapshot after something went wrong
- To clean up old snapshots or check creation time and parent
- Prefer this page when the job matches the purpose above
- When you want a dropdown-scoped manager under More instead of `/app/snapshots`

## How to get there

- Route / id: `/snapshot-manager`
- Nav: **More — images, migrations & managers → Snapshot Mgr** (sidebar, command palette, or desktop nav)

## Operate from the console (UX)

1. **Select VM** — dropdown loads snapshots; auto-refresh every 15s while selected, or **Refresh**.
2. **Create Snapshot** — name, **Disk Only** (default) or **Full (disk + memory)**; UI shows progress and retries if the monitor is still starting.
3. Table: Name, Created, State, Parent. **Revert** (confirmation; VM must be stopped for revert) and **Delete** (confirmation).
4. Empty states: no VM selected → "Select a VM"; none → "No snapshots."
5. Success banner confirms create.

Typical flow: select VM → Disk Only snapshot → make change → revert if needed (stop VM first) → delete old snaps. For fleet-wide timestamped snaps, [Bulk Operations](../operations/bulk-operations.md) also offers Snapshot.

**Empty / fail:** Error banner, empty table, or failed toast — confirm you are signed in, `zyvor-fabricd` is healthy (`/readyz` on `http://127.0.0.1:<port>` or `https://<host>`), and any backend this page needs (FluxVM, storage, network) is reachable. See [Admin basics](../../admin-basics.md).

**Success:** Live data loads without error; creates/updates appear in the list or detail view and any confirmation toast clears cleanly.

## Related pages

- [Snapshots](../operations/snapshots.md)
- [Backups](../operations/backups.md)
- [Backup Scheduler](backup-scheduler.md)
- [Bulk Operations](../operations/bulk-operations.md)
- [Virtual Machines](../core/vms.md)
- [Job Monitor](job-monitor.md)
- [Pipeline](pipeline.md)
- [Getting Started](../../getting-started.md)
- [Page index](../../PAGE_INDEX.md)
