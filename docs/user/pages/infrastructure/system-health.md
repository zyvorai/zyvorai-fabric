# System Health

## Purpose

System Health — a live, read-only dashboard of host resource utilization, refreshing every 2 seconds: CPU, memory, disk I/O, filesystems, network interfaces, and top processes, rolled up into a single health score.

## When to use it

- To get a fast, single-number (0–100) read on whether the host is under stress
- To find out what's bottlenecking the host — CPU, memory, disk, or network — before adding more VMs to it
- To identify the specific process consuming the most CPU or memory
- To check filesystem usage or per-disk I/O latency and queue depth on the physical host

## How to get there

- Route / id: `/app/system-health`
- Nav: **Infrastructure → System Health** (sidebar, command palette, or desktop nav)

## Operate from the console (UX)

1. Health score gauge (0–100, colored red/amber/green) with a status label and a summary panel calling out the current bottleneck, if any.
2. **CPU** panel — overall usage, core count, 1-minute and 5-minute load averages, and a per-core usage grid.
3. **Memory** panel — RAM and swap usage bars with used/total detail, plus total, available, and cached memory.
4. **Filesystems** — a usage bar per mounted filesystem (mountpoint, filesystem type, used/total), shown when data is available.
5. **Disk I/O** — per-device reads/writes completed, bytes read/written, average latency, and queue depth.
6. **Network** — per-interface RX/TX bytes and error counts, TCP connection-state counts, and host-wide RX/TX/retransmit totals.
7. **Top CPU** and **Top Memory** process tables — PID, name, CPU%, and memory (MB).

Entirely read-only monitoring — there's no create/edit/delete action here; data refreshes automatically every 2 seconds.

**Empty / fail:** Error banner, empty table, or failed toast — confirm you are signed in, `zyvor-fabricd` is healthy (`/readyz`), and any backend this page needs (FluxVM, storage, network) is reachable. See [Admin basics](../../admin-basics.md).

**Success:** Live data loads without error; creates/updates appear in the list or detail view and any confirmation toast clears cleanly.

## Related pages

- [Network](network.md)
- [Net Security](network-security.md)
- [Edge Dataplane](edge-dataplane.md)
- [Getting Started](../../getting-started.md)
- [Page index](../../PAGE_INDEX.md)
