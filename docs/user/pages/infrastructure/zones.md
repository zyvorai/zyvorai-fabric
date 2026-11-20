# Availability Zones

## Purpose

Zones — availability / placement domains for the fabric: named zones (region, status, host count) plus **spot instances** (max price, priority, eviction policy) that can be requested, evicted, or deleted against those zones.

In the UI this page is titled **Availability Zones**.

## When to use it

- To define placement domains (e.g. `us-east-1a`) before you reason about spot capacity
- To see which zones are `available`, `degraded`, or `unavailable` and how many hosts each has
- To request a spot instance for a VM with a max price/hour and eviction policy
- To evict a running spot instance (applies its eviction policy immediately) or delete the spot record

## How to get there

- Route / id: `/app/zones`
- Nav: **Operations → Availability Zones** (sidebar, command palette, or desktop nav)
- Legacy `/zones` redirects under `/app` when configured

## Operate from the console (UX)

Summary tiles show zone count, running spot instances, and evicted count. Two tabs: **Zones** and **Spot Instances**. Use the header refresh to reload.

### Zones tab

1. **Create Zone** — name (required), optional description and region → **Create**.
2. The table lists Name, Region, Status badge, Hosts count.
3. Delete a zone with the trash icon (confirmation required).
4. Empty state: “No availability zones.”

### Spot Instances tab

1. **Request Spot Instance** — VM name, max price/hour, priority (`low` / `regular`), zone, eviction policy (`stop` / `delete` / `deallocate`).
2. Table columns: VM, Max Price/hr, Priority, Status (`running` / `evicted` / `terminated`), Eviction Policy.
3. On a **running** spot: **Evict** applies the eviction policy to the VM immediately (confirmation).
4. Trash deletes the spot **record** (confirmation), separate from evict.
5. Empty state: “No spot instances.”

6. **Empty / fail:** Page load banner “Could not load availability zones” — check auth and API; create/evict toasts on failure.
7. **Success:** New zone or spot row appears; evict moves status to `evicted` and shows a success toast.

## Related pages

- [Datacenters](../core/datacenters.md)
- [DRS](../operations/drs.md)
- [Resource Pools](resource-pools.md)
- [Virtual Machines](../core/vms.md)
- [Getting Started](../../getting-started.md)
- [Page index](../../PAGE_INDEX.md)
