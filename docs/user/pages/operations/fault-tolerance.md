# Fault Tolerance

## Purpose

Fault Tolerance (FT) — protect individual VMs with a live secondary replica on another host, so a host failure fails the VM over instead of taking it down, and monitor replication health.

## When to use it

- To keep a critical VM running through a host failure by giving it a hot secondary on another host
- To check whether a VM is actually FT-compatible before enabling it, and fix any blocking issues first
- To test a failover safely, or trigger a real one when you need to move a VM off its current host
- To watch replication health — log bandwidth, checkpoint latency, secondary host load, uptime — for protected VMs

## How to get there

- Route / id: `/app/fault-tolerance`
- Nav: **Operations → Fault Tolerance** (sidebar, command palette, or desktop nav)

## Operate from the console (UX)

Summary tiles show protected VM count, how many are running, how many need attention, and total failover events. **Enable FT** opens a form to name the VM, set its primary host ID, and optionally a secondary host ID (leave blank to auto-select). Four tabs:

1. **Protected VMs** — table of FT-enabled VMs with their secondary host, status, and bandwidth limit. Per row: **Test** (a non-disruptive failover test), the play icon to **trigger a real failover** (confirmation required — moves the VM to its secondary host), and the trash icon to **disable FT** (confirmation required).
2. **Compatibility Check** — enter a VM ID and check whether it's FT-compatible; results show a pass/fail badge, any blocking or warning issues (each with a suggested fix), and recommended secondary hosts with a compatibility score.
3. **Events** — a timeline of failover events (test and real), each showing status, failover type, source → target host, downtime in ms, timestamp, and the error message if it failed.
4. **Metrics** — per protected VM: log bandwidth usage, checkpoint latency, secondary host's CPU/memory usage, failover count, and uptime percentage.

**Empty / fail:** Error banner, empty table, or failed toast — confirm you are signed in, `zyvor-fabricd` is healthy (`/readyz`), and any backend this page needs (FluxVM, storage, network) is reachable. See [Admin basics](../../admin-basics.md).

**Success:** Live data loads without error; creates/updates appear in the list or detail view and any confirmation toast clears cleanly.

## Related pages

- [Snapshots](snapshots.md)
- [Backups](backups.md)
- [Migrations](migrations.md)
- [Getting Started](../../getting-started.md)
- [Page index](../../PAGE_INDEX.md)
