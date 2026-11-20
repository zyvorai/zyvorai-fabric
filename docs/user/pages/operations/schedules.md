# Schedules

## Purpose

VM Schedules — automate a recurring lifecycle action (start, stop, restart, or snapshot) for a single VM, on a one-time, daily, or weekly schedule.

## When to use it

- To automatically stop dev/test VMs overnight or on weekends to save resources
- To take a recurring snapshot of a VM on a schedule
- To run a one-off action at a specific future time without staying logged in
- To check whether a scheduled action actually ran, and whether it succeeded

## How to get there

- Route / id: `/app/schedules`
- Nav: **Operations → Schedules** (sidebar, command palette, or desktop nav)

## Operate from the console (UX)

1. **Create Schedule** — pick a VM, an action (Start VM, Stop VM, Restart VM, Create Snapshot), a schedule type, and a time (24-hour UTC):
   - **Once** — runs a single time
   - **Daily** — runs every day at the given time
   - **Weekly** — pick one or more days of the week to run on
   A checkbox enables the schedule immediately on creation.
2. Search schedules by name, VM, or action using the search box above the list.
3. Each schedule shows as a card with its enabled/disabled state, action badge, target VM, a human-readable description of its cadence (e.g. "Weekly on Mon, Wed at 09:00"), and next/last run times (relative).
4. Per-schedule actions: **Run Now** (executes immediately, confirmation required, disabled when the schedule itself is disabled), **Enable/Disable** (toggle without deleting), **Edit** (change name, action, cadence, or time), and **Delete** (confirmation, cannot be undone).
5. **History** button switches the page to an execution history table (latest 20 runs) showing schedule, VM, action, when it ran, and success/failure status with the error message if it failed.

**Empty / fail:** Error banner, empty table, or failed toast — confirm you are signed in, `zyvor-fabricd` is healthy (`/readyz`), and any backend this page needs (FluxVM, storage, network) is reachable. See [Admin basics](../../admin-basics.md).

**Success:** Live data loads without error; creates/updates appear in the list or detail view and any confirmation toast clears cleanly.

## Related pages

- [Snapshots](snapshots.md)
- [Backups](backups.md)
- [Migrations](migrations.md)
- [Getting Started](../../getting-started.md)
- [Page index](../../PAGE_INDEX.md)
