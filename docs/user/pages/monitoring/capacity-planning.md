# Capacity

## Purpose

Capacity Planning — resource usage against total capacity per resource (memory, CPU, storage), with week-over-week trend, so you can see what's running out and when.

Read-only reporting. A warning banner appears when any tracked resource is above 75% usage.

## When to use it

- Prefer this page when the job matches the purpose above
- To check whether the host is approaching a resource ceiling before it becomes an incident
- To see which resources are growing week over week and by how much
- To get a rough sense of how many active VMs are contributing to current usage
- Before approving large template/profile creates or raising [Quotas](../operations/quotas.md)
- In weekly capacity reviews alongside [Analytics](analytics.md) trends

## How to get there

- Route / id: `/capacity-planning`
- Nav: **Monitoring → Capacity** (sidebar, command palette, or desktop nav)

## Operate from the console (UX)

1. **Capacity warning banner** — appears when any tracked resource is above 75%, naming which ones.
2. **Summary tiles** — Active VMs, Resources Tracked, resources Over 75% Usage, Growing Resources.
3. **Per-resource cards** — used/total (GB→TB past 1024 GB), usage %, color progress bar, weekly trend (%/week with arrow), remaining capacity, projected-full date when supplied.
4. Auto-refresh every 30 seconds; header refresh forces immediate reload.

Typical flow: open Capacity → note Over 75% / Growing → open the hot resource card → plan reclaim (Optimizer, stop idle VMs) or expand storage via [Storage Pools](../infrastructure/storage-pools.md). No configure actions on this page.

**Empty / fail:** Error banner, empty table, or failed toast — confirm you are signed in, `zyvor-fabricd` is healthy (`/readyz` on `http://127.0.0.1:<port>` or `https://<host>`), and any backend this page needs (FluxVM, storage, network) is reachable. See [Admin basics](../../admin-basics.md).

**Success:** Live data loads without error; creates/updates appear in the list or detail view and any confirmation toast clears cleanly.

## Related pages

- [Analytics](analytics.md)
- [Optimizer](resource-optimizer.md)
- [Quotas](../operations/quotas.md)
- [Storage](../infrastructure/storage.md)
- [Storage Pools](../infrastructure/storage-pools.md)
- [Live Metrics](live-metrics.md)
- [Getting Started](../../getting-started.md)
- [Page index](../../PAGE_INDEX.md)
