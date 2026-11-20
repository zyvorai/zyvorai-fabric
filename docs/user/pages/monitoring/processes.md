# Processes

## Purpose

Processes — a live process monitor for the host: every OS process with its CPU and memory usage, refreshed every 3 seconds, with a per-process detail drill-down.

This is the **host** process table (not per-guest). For guest activity use VM Console or Live Metrics / Analytics.

## When to use it

- Finding what's consuming CPU or memory on the host right now
- Spotting zombie or stopped processes, or an unexpectedly high process count
- Digging into a specific process's command line, I/O, open file descriptors, or context-switch activity
- When [System Health](../infrastructure/system-health.md) shows pressure and you need the offending PID
- Alongside [Debug Tools](debug.md) when you want a structured table instead of raw `top` text

## How to get there

- Console URL pattern: `http://127.0.0.1:<port>/…` or `https://<host>/…`
- Route / id: `/processes`
- Nav: **Monitoring → Processes** (sidebar, command palette, or desktop nav)

## Operate from the console (UX)

1. Summary cards: **Total Processes**, **Running**, **Sleeping**.
2. Filter box matches PID, process name, or state as you type.
3. Table: PID, name, CPU % (color bar — green &lt;20%, blue &lt;50%, amber &lt;80%, red 80%+), memory MB, state badge (Running / Sleeping / Disk Wait / Zombie / Stopped), thread count.
4. Click a row for **Process Detail** — command line, IO read/write bytes, open FDs, voluntary/involuntary context switches. **Close** collapses it.
5. Auto-refresh every 3 seconds; manual refresh in the header.

Typical flow: sort attention to red CPU bars → open detail → note command line → correlate with FluxVM/QEMU or container workloads on [Containers](../infrastructure/containers.md). Read-only — no kill/signal from this page.

**Empty / fail:** Error banner, empty table, or failed toast — confirm you are signed in, `zyvor-fabricd` is healthy (`/readyz` on `http://127.0.0.1:<port>` or `https://<host>`), and any backend this page needs (FluxVM, storage, network) is reachable. See [Admin basics](../../admin-basics.md).

**Success:** Live data loads without error; creates/updates appear in the list or detail view and any confirmation toast clears cleanly.

## Related pages

- [Debug Tools](debug.md)
- [System Health](../infrastructure/system-health.md)
- [Live Metrics](live-metrics.md)
- [Containers](../infrastructure/containers.md)
- [Kernel](kernel.md)
- [Getting Started](../../getting-started.md)
- [Page index](../../PAGE_INDEX.md)
