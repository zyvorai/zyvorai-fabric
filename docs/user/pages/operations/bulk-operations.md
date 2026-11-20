# Bulk Operations

## Purpose

Bulk Operations — select any number of VMs and start, stop, restart, or snapshot them together, with a per-VM progress log for the batch.

Actions run **sequentially** across the selection. There is **no confirmation dialog** — the action fires as soon as you click it. Prefer this over clicking one VM at a time on the fleet list.

## When to use it

- To restart or stop a group of VMs at once (e.g. before a maintenance window) instead of one at a time
- To take a quick snapshot of several VMs together before a risky change
- To see which VMs in a batch action succeeded and which failed, with the specific error for each
- When [Virtual Machines](../core/vms.md) selection is awkward for a large filtered set
- For a maintenance window: filter → select → Stop or Snapshot → Clear results when done

## How to get there

- Route / id: `/app/bulk-operations`
- Nav: **Operations → Bulk Operations** (sidebar, command palette, or desktop nav)

## Operate from the console (UX)

1. Filter the VM list by name, then click rows to select (or **Select All** / **Deselect All** for the current filter).
2. Once at least one VM is selected, the action bar shows **Start**, **Stop**, **Restart**, and **Snapshot**. Snapshot creates `bulk-snap-<timestamp>` on each selected VM.
3. Each action runs sequentially; **Batch Progress** shows pending → running → done/error per VM, plus the error message when one fails while others succeed.
4. **Clear** deselects everything; **Clear results** dismisses the progress log after a run.
5. The table also shows each VM's state, CPU count, and memory while you select.

Operator tip: double-check the filter before **Select All** — there is no confirm step. For disk + memory snapshots of one VM with more control, use [Snapshots](snapshots.md) or [Snapshot Mgr](../more-images-migrations-managers/snapshot-manager.md).

Snapshot in bulk is disk-oriented via the bulk timestamp name; for Full (disk+memory) snapshots, use the per-VM Snapshots UI instead.

**Empty / fail:** Error banner, empty table, or failed toast — confirm you are signed in, `zyvor-fabricd` is healthy (`/readyz` on `http://127.0.0.1:<port>` or `https://<host>`), and any backend this page needs (FluxVM, storage, network) is reachable. See [Admin basics](../../admin-basics.md).

**Success:** Live data loads without error; creates/updates appear in the list or detail view and any confirmation toast clears cleanly.

## Related pages

- [Virtual Machines](../core/vms.md)
- [Snapshots](snapshots.md)
- [Schedules](schedules.md)
- [Backups](backups.md)
- [Migrations](migrations.md)
- [Getting Started](../../getting-started.md)
- [Page index](../../PAGE_INDEX.md)
