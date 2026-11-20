# Site Recovery

## Purpose

Site Recovery — define disaster recovery plans that group VMs by source and target site, then execute those plans as a test failover, a planned migration, or a full disaster recovery, and track how each execution unfolds.

## When to use it

- To set up which VMs fail over together, and to which target site, ahead of an actual incident
- To run a non-destructive test failover to validate a plan works before you need it for real
- To execute an actual disaster recovery when a site goes down
- To check fleet-wide DR posture — how many VMs are protected, average RTO/RPO, and which plans have been tested

## How to get there

- Route / id: `/app/site-recovery`
- Nav: **Operations → Site Recovery** (sidebar, command palette, or desktop nav)

## Operate from the console (UX)

1. **DR Dashboard tab** — summary tiles for protected vs. unprotected VMs, average RTO, average RPO, and plans tested vs. total plans; plus a per-site status list and the 5 most recent executions with their status.
2. **Recovery Plans tab** — table of plans (name/description, VM group count and total VMs, status, last tested date, last executed date). **Create Plan** defines a plan name, description, source site ID, target site ID, a VM group name, and a comma-separated list of VM IDs in that group.
3. Each plan can be **executed** — opens a modal with three execution types, each with different risk:
   - **Test Failover** — non-destructive, runs in an isolated network
   - **Planned Migration** — graceful, zero data loss
   - **Disaster Recovery** — emergency failover; some data loss possible; confirmation dialog warns this fails over all VMs to the target site
   Plans can also be **deleted** (confirmation dialog).
4. **Execution History tab** — each past/in-progress execution shows plan name, execution type, status, VMs recovered vs. total, actual RTO once known, a live progress bar with the current step, a chip per step showing its individual status, and start/completion timestamps.

**Empty / fail:** Error banner, empty table, or failed toast — confirm you are signed in, `zyvor-fabricd` is healthy (`/readyz`), and any backend this page needs (FluxVM, storage, network) is reachable. See [Admin basics](../../admin-basics.md).

**Success:** Live data loads without error; creates/updates appear in the list or detail view and any confirmation toast clears cleanly.

## Related pages

- [Snapshots](snapshots.md)
- [Backups](backups.md)
- [Migrations](migrations.md)
- [Getting Started](../../getting-started.md)
- [Page index](../../PAGE_INDEX.md)
