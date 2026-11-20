# Resource Pools

## Purpose

Resource Pools — hierarchical CPU/memory allocation pools (nested, with shares, reservations, and limits) that VMs draw from, plus an admission-control test to check whether a workload's requirements would fit before you commit to it.

## When to use it

- To divide a cluster's CPU/memory into weighted pools — for example giving "prod" more shares than "dev"
- To nest a child pool under a parent to further subdivide its allocation
- To check whether a pool has room for a new VM's CPU/memory requirements before creating it
- To see how much of a pool's reservation or limit is actually in use

## How to get there

- Route / id: `/resource-pools`
- Nav: **Infrastructure → Resource Pools** (sidebar, command palette, or desktop nav)

## Operate from the console (UX)

1. Review stat cards: total pools, total CPU shares, and total VMs across all pools.
2. Browse the expandable pool tree — click a pool to expand/collapse its children; each row shows VM count, CPU/memory shares, and live usage bars that turn red above 80%.
3. Expand a pool to see its CPU/memory reservation, limit, currently available capacity, and child pool count.
4. **Create Pool** — set a name, cluster ID, an optional parent pool (to nest it under another pool), and CPU/memory shares, reservations, and limits (use `-1` for unlimited).
5. **Test Admission** — on any pool, enter a required CPU (MHz) and memory (MB) and run the check; the result shows Admitted or Denied with a reason and the pool's currently available capacity.
6. Delete a pool — asks for confirmation before removing it.

**Empty / fail:** Error banner, empty table, or failed toast — confirm you are signed in, `zyvor-fabricd` is healthy (`/readyz`), and any backend this page needs (FluxVM, storage, network) is reachable. See [Admin basics](../../admin-basics.md).

**Success:** Live data loads without error; creates/updates appear in the list or detail view and any confirmation toast clears cleanly.

## Related pages

- [Network](network.md)
- [Net Security](network-security.md)
- [Edge Dataplane](edge-dataplane.md)
- [Getting Started](../../getting-started.md)
- [Page index](../../PAGE_INDEX.md)
