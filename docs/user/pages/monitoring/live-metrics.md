# Live Metrics

## Purpose

Live Metrics — a real-time view of host performance: CPU, memory, disk I/O, and network throughput, each as a rolling sparkline that updates once per second.

Network RX/TX are **client-derived rates** from counter deltas, not raw totals. Pause freezes a spike for inspection.

## When to use it

- Watching load in real time while you run a benchmark or try to reproduce a slow VM
- Confirming the host is actually under CPU, memory, disk, or network pressure right now
- Pausing the feed to freeze a spike so you can read the exact numbers or take a screenshot
- During a change window to watch impact as VMs start or migrate
- When [Analytics](analytics.md) trends look fine but you suspect a short spike

## How to get there

- Route / id: `/app/live-metrics`
- Nav: **Monitoring → Live Metrics** (sidebar, command palette, or desktop nav)

## Operate from the console (UX)

- Six live cards — CPU Usage, Memory Usage, Disk Read, Disk Write, Network RX, Network TX — current value plus a 60-second sparkline, polled every second.
- Network RX/TX are computed rates (bytes/sec) from successive cumulative counter samples.
- **Pause / Resume** freezes or resumes the 1-second loop.
- Status dot: **Streaming** (green), **Paused** (amber), or **Error** (red).
- Background refresh failure after load → amber banner, last known values kept.
- First-load failure → error banner with retry hints instead of cards.

Typical flow: start Streaming → reproduce the issue → Pause on the spike → note which of the six cards peaked → follow up in Processes / Debug Tools / Explain.

**Empty / fail:** Error banner, empty table, or failed toast — confirm you are signed in, `zyvor-fabricd` is healthy (`/readyz` on `http://127.0.0.1:<port>` or `https://<host>`), and any backend this page needs (FluxVM, storage, network) is reachable. See [Admin basics](../../admin-basics.md).

**Success:** Live data loads without error; creates/updates appear in the list or detail view and any confirmation toast clears cleanly.

## Related pages

- [Analytics](analytics.md)
- [Processes](processes.md)
- [Debug Tools](debug.md)
- [Explain](explain.md)
- [System Health](../infrastructure/system-health.md)
- [Capacity](capacity-planning.md)
- [Getting Started](../../getting-started.md)
- [Page index](../../PAGE_INDEX.md)
