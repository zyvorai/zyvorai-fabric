# Debug Tools

## Purpose

Debug Tools — raw, terminal-style output from four classic Linux diagnostic commands (top, iostat, vmstat, netstat) against the host, rendered as monospace panels. This is the closest the dashboard gets to SSHing in and running commands yourself.

Panels do **not** auto-load; click Refresh (per panel or Refresh All). Optional auto-refresh every 3 seconds.

## When to use it

- Prefer this page when the job matches the purpose above
- To check live process/CPU activity, disk I/O, virtual memory stats, or network connections on the host without opening a shell
- To troubleshoot a performance problem in real time, side by side across all four views
- When Kernel or Analytics show something is wrong and you need the raw command output to confirm it
- When [Processes](processes.md) is not enough and you want classic `top`/`vmstat` text
- To capture a pasteable snippet for an incident channel (copy from the monospace panels)

## How to get there

- Route / id: `/debug`
- Nav: **Monitoring → Debug Tools** (sidebar, command palette, or desktop nav)

## Operate from the console (UX)

1. **Four independent panels** — Top, IOStat, VMStat, NetStat — each backed by its own endpoint, monospace scrollable output.
2. Each panel starts with "Click Refresh to load data" until you refresh that panel or use **Refresh All**.
3. **Auto-refresh** — header switch re-pulls all four every 3 seconds; turn off when done to stop polling.
4. A failed panel shows its own error without blocking the others.

Typical flow: Refresh All → enable Auto-refresh while reproducing → disable Auto-refresh → copy the hot panel output. Pair with Live Metrics for sparklines and Processes for PID drill-down. Read-only — no process control from here.

**Empty / fail:** Error banner, empty table, or failed toast — confirm you are signed in, `zyvor-fabricd` is healthy (`/readyz` on `http://127.0.0.1:<port>` or `https://<host>`), and any backend this page needs (FluxVM, storage, network) is reachable. See [Admin basics](../../admin-basics.md).

**Success:** Live data loads without error; creates/updates appear in the list or detail view and any confirmation toast clears cleanly.

## Related pages

- [Processes](processes.md)
- [Live Metrics](live-metrics.md)
- [Kernel](kernel.md)
- [Explain](explain.md)
- [System Health](../infrastructure/system-health.md)
- [Analytics](analytics.md)
- [Getting Started](../../getting-started.md)
- [Page index](../../PAGE_INDEX.md)
