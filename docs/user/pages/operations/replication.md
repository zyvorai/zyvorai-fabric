# Replication

## Purpose

Replication — register remote replication sites, configure per-VM replication to them with a target RPO (recovery point objective), and monitor sync health and RPO compliance across your fleet.

Use this for ongoing sync to a secondary (or bidirectional) site. For one-shot move/failover plans, see [Migrations](migrations.md) and [Site Recovery](site-recovery.md).

## When to use it

- Prefer this page when the job matches the purpose above
- To register a secondary (or bidirectional) site that VMs can replicate to
- To start replicating a VM to another site with a target RPO in minutes
- To pause or resume replication for a VM without tearing down the configuration
- To check overall replication health, or find which VMs are currently violating their RPO target
- Before a DR drill, to confirm sites are healthy and RPO violations are empty

## How to get there

- Route / id: `/app/replication`
- Nav: **Operations → Replication** (sidebar, command palette, or desktop nav)

## Operate from the console (UX)

1. **Dashboard tab** — summary tiles for active replications, RPO violation count, average RPO, and paused/error counts, plus a per-site health list (healthy/degraded/unhealthy) with each site's replication count.
2. **Sites tab** — table of registered sites (name, type, endpoint, status, replication count, last sync). **Add Site** registers name, endpoint URL, and type (Primary, Recovery/secondary, or Bidirectional). Remove a site with confirmation.
3. **Replications tab** — per-VM configs: target RPO, status, live sync progress, last/next sync. **Configure Replication** sets VM ID, source site, target site, and RPO in minutes. **Pause** / **Resume** without tearing down config.
4. **RPO Violations tab** — replications missing target RPO (target vs current, compliance, bandwidth, sync/failure counts). Shows "All replications are compliant" when clear.

Typical flow: register Recovery site → configure replication for critical VMs with an RPO → watch Dashboard health → investigate Violations before relying on Site Recovery.

Endpoints should be reachable from this Fabric host (use `<host>` URLs you control — never paste lab-only addresses into docs).

**Empty / fail:** Error banner, empty table, or failed toast — confirm you are signed in, `zyvor-fabricd` is healthy (`/readyz` on `http://127.0.0.1:<port>` or `https://<host>`), and any backend this page needs (FluxVM, storage, network) is reachable. See [Admin basics](../../admin-basics.md).

**Success:** Live data loads without error; creates/updates appear in the list or detail view and any confirmation toast clears cleanly.

## Related pages

- [Site Recovery](site-recovery.md)
- [Migrations](migrations.md)
- [Fault Tolerance](fault-tolerance.md)
- [Backups](backups.md)
- [Snapshots](snapshots.md)
- [Getting Started](../../getting-started.md)
- [Page index](../../PAGE_INDEX.md)
