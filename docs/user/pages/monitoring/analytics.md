# Analytics

## Purpose

Performance Analytics — fleet-wide resource utilization, trends over time, and per-VM performance insights, with exportable reports.

Time range drives the chart and reloads page data. Read-only — act on VMs from their detail page.

## When to use it

- To check overall CPU/memory/disk/network utilization across the fleet
- To find which VMs are consuming the most CPU, memory, or network right now
- To review flagged performance issues (e.g. a VM running hot) with a recommendation
- To pull a PDF or CSV performance report for a stakeholder or incident writeup
- For longer windows than Live Metrics' 60-second sparklines (up to 30 days)

## How to get there

- Route / id: `/app/analytics`
- Nav: **Monitoring → Analytics** (sidebar, command palette, or desktop nav)

## Operate from the console (UX)

1. **Time range** — Last Hour, 6 Hours, 24 Hours, 7 Days, or 30 Days; reloads chart and data.
2. **Export** — PDF or CSV performance report for the selected range.
3. **Resource Utilization Overview** — CPU, Memory, Disk, Network tiles with % and color bars (green &lt;50%, blue 50–75%, amber 75–90%, red 90%+).
4. **Performance Insights** — up to 5 issues (critical/warning/info) with VM, resource, value, recommendation.
5. **Top VMs by Resource** — top 5 by CPU, Memory, and Network.
6. **System Performance Over Time** — area chart of total CPU% and memory% plus averages and VM counts for the window.

Typical flow: pick 24 Hours or 7 Days → scan Insights and Top VMs → Export if needed → right-size via [Optimizer](resource-optimizer.md) or open the hot VM. Use [Explain](explain.md) for a prose take on one metric.

**Empty / fail:** Error banner, empty table, or failed toast — confirm you are signed in, `zyvor-fabricd` is healthy (`/readyz` on `http://127.0.0.1:<port>` or `https://<host>`), and any backend this page needs (FluxVM, storage, network) is reachable. See [Admin basics](../../admin-basics.md).

**Success:** Live data loads without error; creates/updates appear in the list or detail view and any confirmation toast clears cleanly.

## Related pages

- [Live Metrics](live-metrics.md)
- [Explain](explain.md)
- [Optimizer](resource-optimizer.md)
- [Capacity](capacity-planning.md)
- [Alerts](alerts.md)
- [Virtual Machines](../core/vms.md)
- [Getting Started](../../getting-started.md)
- [Page index](../../PAGE_INDEX.md)
