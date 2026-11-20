# Optimizer

## Purpose

Optimizer — a right-sizing advisor that analyzes each VM's actual resource usage and recommends CPU/memory/disk adjustments, with a one-click apply per VM.

Recommendations are based on observed usage. **Auto-Optimize** applies that VM's recommendations via the optimize endpoint; skipped changes are reported in the result.

## When to use it

- Checking whether VMs are over- or under-provisioned before a capacity or cost review
- Applying a recommended resource change without manually editing a VM's spec
- Triaging which VMs have high-impact recommendations first
- After a quiet period of metrics, so recommendations reflect steady state not a spike
- When [Autoscale](../operations/autoscale.md) is too aggressive and you want a one-shot right-size instead

## How to get there

- Route / id: `/resource-optimizer`
- Nav: **Monitoring → Optimizer** (sidebar, command palette, or desktop nav)

## Operate from the console (UX)

1. Summary cards: **VMs Analyzed**, total **Recommendations**, **High Impact**, **Medium Impact**.
2. If nothing needs changing, the page shows "All VMs are optimally configured."
3. Otherwise each VM card lists recommendations: resource, current → recommended, reason, impact badge (High / Medium / Low).
4. **Auto-Optimize** per VM applies all of that VM's recommendations — button spins, then becomes disabled **Applied** and reports applied vs skipped counts.
5. Manual refresh is in the page header.

Typical flow: sort attention to High Impact → review reason → Auto-Optimize one VM → confirm on Virtual Machines / Live Metrics. Respect [Quotas](../operations/quotas.md) before applying large increases.

**Empty / fail:** Error banner, empty table, or failed toast — confirm you are signed in, `zyvor-fabricd` is healthy (`/readyz` on `http://127.0.0.1:<port>` or `https://<host>`), and any backend this page needs (FluxVM, storage, network) is reachable. See [Admin basics](../../admin-basics.md).

**Success:** Live data loads without error; creates/updates appear in the list or detail view and any confirmation toast clears cleanly.

## Related pages

- [Analytics](analytics.md)
- [Capacity](capacity-planning.md)
- [Live Metrics](live-metrics.md)
- [Autoscale](../operations/autoscale.md)
- [Quotas](../operations/quotas.md)
- [Explain](explain.md)
- [Getting Started](../../getting-started.md)
- [Page index](../../PAGE_INDEX.md)
