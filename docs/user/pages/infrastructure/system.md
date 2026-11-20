# System

## Purpose

System — the physical host's hardware topology (CPU sockets/cores/threads, NUMA nodes, hugepages) plus topology-aware optimization recommendations for individual VMs. This is distinct from [System Health](system-health.md), which tracks live utilization rather than hardware layout.

Four tabs: CPU Topology, NUMA Topology, Memory & Hugepages, Optimization (badged with pending recommendation count).

## When to use it

- To see the host's CPU socket/core/thread layout and NUMA node boundaries before pinning a VM's vCPUs
- To check how many hugepages (2MB/1GB) are allocated and free, or allocate more for a memory-intensive VM
- To review and apply topology-aware optimization recommendations (e.g. NUMA/CPU pinning) for a specific running VM
- Prefer this page when the job matches the purpose above
- Before placing latency-sensitive guests, to understand NUMA distances

## How to get there

- Route / id: `/app/system`
- Nav: **Infrastructure → System** (sidebar, command palette, or desktop nav)

## Operate from the console (UX)

Stat cards: total CPUs (sockets × cores), NUMA node count, total/available memory, 2MB hugepage total/free.

1. **CPU Topology** — CPUs by socket, online/offline; hover for core, thread, NUMA node.
2. **NUMA Topology** — per-node CPUs, memory total/free + bar, hugepage counts, inter-node distance matrix.
3. **Memory & Hugepages** — memory breakdown (total, available, buffers, cached) plus 2MB/1GB hugepage stats. **Allocate Hugepages** picks page size and count and applies immediately.
4. **Optimization** — per-VM recommendations (resource, current → recommended, reason, impact). **Apply** runs the change against that VM.

Typical flow: learn topology → allocate hugepages if needed → open Optimization → Apply carefully on non-prod first → confirm on VM detail / Live Metrics. For live utilization (not layout), use System Health.

**Empty / fail:** Error banner, empty table, or failed toast — confirm you are signed in, `zyvor-fabricd` is healthy (`/readyz` on `http://127.0.0.1:<port>` or `https://<host>`), and any backend this page needs (FluxVM, storage, network) is reachable. See [Admin basics](../../admin-basics.md).

**Success:** Live data loads without error; creates/updates appear in the list or detail view and any confirmation toast clears cleanly.

## Related pages

- [System Health](system-health.md)
- [Resource Pools](resource-pools.md)
- [Optimizer](../monitoring/resource-optimizer.md)
- [Live Metrics](../monitoring/live-metrics.md)
- [Kernel](../monitoring/kernel.md)
- [Virtual Machines](../core/vms.md)
- [Getting Started](../../getting-started.md)
- [Page index](../../PAGE_INDEX.md)
