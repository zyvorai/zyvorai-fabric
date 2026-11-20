# Containers

## Purpose

Containers — a read-only Docker/Podman host view (not Container Groups / Secure Containers). Shows per-container state, image, CPU/memory, and network I/O. Monitoring-only (auto-refresh ~3s); no start/stop/delete.

## When to use it

- To check whether container workloads on this host are running, restarting, or exited
- To spot a container consuming excessive CPU or memory before it starves VMs sharing the same host
- To confirm a container's network throughput (RX/TX)
- Prefer this page when the job matches the purpose above
- When [Processes](../monitoring/processes.md) shows container runtimes and you want a container-centric view

## How to get there

- Route / id: `/app/containers`
- Nav: **Infrastructure → Containers** (sidebar, command palette, or desktop nav)

## Operate from the console (UX)

1. Summary cards: total containers, running count, total CPU%, total memory across detected containers.
2. Container grid — name/ID, state badge (running / exited / paused / restarting), image, live CPU/memory bars, RX/TX when available.
3. Auto-refresh every 3 seconds; header refresh forces an immediate reload.
4. Correlate hot containers with host pressure on [Live Metrics](../monitoring/live-metrics.md) / [System Health](system-health.md).

Typical flow: scan for restarting/exited → note image and resource bars → remediate with your container runtime outside Fabric → confirm the card returns to running. VM workloads stay on [Virtual Machines](../core/vms.md).

Operator tip: high container CPU/memory can starve VMs on the same host — correlate with Live Metrics.

**Empty / fail:** Error banner, empty table, or failed toast — confirm you are signed in, `zyvor-fabricd` is healthy (`/readyz` on `http://127.0.0.1:<port>` or `https://<host>`), and any backend this page needs (FluxVM, storage, network) is reachable. See [Admin basics](../../admin-basics.md).

**Success:** Live data loads without error; creates/updates appear in the list or detail view and any confirmation toast clears cleanly.

## Related pages

- [Container Groups](../operations/container-groups.md) — Secure Containers workloads
- [Virtual Machines](../core/vms.md)
- [Processes](../monitoring/processes.md)
- [Live Metrics](../monitoring/live-metrics.md)
- [System Health](system-health.md)
- [System](system.md)
- [Getting Started](../../getting-started.md)
- [Page index](../../PAGE_INDEX.md)
