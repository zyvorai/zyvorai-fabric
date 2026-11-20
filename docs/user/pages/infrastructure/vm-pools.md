# Warm Pools (VM Pools)

## Purpose

Warm Pools — keep N VMs **pre-booted and paused** from a chosen disk image so you can **claim** one instantly instead of cold-creating. Each pool shows ready members vs size, vCPUs/memory, and the image path.

In the UI and Core nav this is **Warm Pools** (`/app/vm-pools`). Older docs may say “VM Pools.”

## When to use it

- To cut provision latency for bursty or CI-style workloads
- To keep a standing set of identical paused members ready to claim
- To claim a ready member into a named running VM and jump straight to its detail page
- To tear down a pool (and its members) when you no longer need the capacity

## How to get there

- Route / id: `/app/vm-pools`
- Nav: **Core → Warm Pools** (sidebar, command palette, or desktop nav)

## Operate from the console (UX)

1. On first visit with no pools, the empty state explains the feature and offers **Create Pool**.
2. **Create Pool** — pool name, disk image (from catalog), size (member count), vCPUs, memory → submit. A success toast notes that members are booting; the ready count climbs as they pause.
3. Each pool card shows name, image path, **Ready to claim** `ready / size` with a progress bar, and vCPU/memory.
4. **Claim Instantly** — enabled only when `ready_members > 0`. Enter the VM name to assign; on success you navigate to `/app/vms/:name`.
5. **Delete** (trash) — confirmation warns that the pool and its pre-booted members will be removed; member teardown continues in the background after the pool disappears from the list.
6. **Empty / fail:** Load error banner with retry; create/claim/delete toasts on failure; Claim disabled when no ready members (tooltip: “No ready members right now”).
7. **Success:** Pool card appears with rising ready count; claim opens the new VM detail page with a success toast.

## Related pages

- [Create VM](../core/create.md)
- [Virtual Machines](../core/vms.md)
- [Profiles](../core/profiles.md)
- [Resource Pools](resource-pools.md)
- [Getting Started](../../getting-started.md)
- [Page index](../../PAGE_INDEX.md)
