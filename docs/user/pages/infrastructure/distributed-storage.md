# Distributed Storage

## Purpose

Distributed Storage — the enterprise/clustered layer above a single storage backend: replicated pools spanning multiple hosts, storage policies (tiering, replication, encryption/dedup/compression), in-flight VM disk migrations between pools, and datastore clusters with automatic space/latency-based balancing. For creating and starting a single NFS/LVM/ZFS/Ceph pool, see [Storage Pools](storage-pools.md); for browsing volumes inside pools, see [Storage](storage.md).

## When to use it

- To provision a storage pool replicated across multiple hosts with a target replication factor
- To define a storage policy (e.g. a "performance" tier with RF=3 and encryption on) that pools or workloads should conform to
- To watch a VM disk migration between pools and confirm it completed
- To group pools into a datastore cluster that rebalances automatically once a space or IO-latency threshold is crossed

## How to get there

- Route / id: `/distributed-storage`
- Nav: **Infrastructure → Distributed Storage** (sidebar, command palette, or desktop nav)

## Operate from the console (UX)

Four tabs — **Pools**, **Policies**, **Migrations**, **Datastore Clusters** — plus summary cards for total pools, aggregate capacity, policy count, and active migrations.

1. **Pools** — distributed storage pools with a status badge, host count, and replication factor, and a used/available capacity bar. **Create Pool** sets a name, type, and replication factor. Delete a pool with a confirmation dialog.
2. **Policies** — a table of storage policies showing tier, replication factor, failure tolerance, and whether encryption, deduplication, and compression are on. **Create Policy** sets name, description, replication factor, stripe width, failures-to-tolerate, and tier (performance / standard / archive). Delete with confirmation.
3. **Migrations** — a read-only table of VM storage migrations: source pool, target pool, progress %, bytes transferred, and status (pending / in progress / completed / failed).
4. **Datastore Clusters** — a table of clusters showing datastore count, whether storage DRS (SDRS) is enabled, space threshold %, total capacity, and VM count. **Create Cluster** sets a name, space threshold %, and IO-latency threshold (ms) that trigger automatic rebalancing.

**Empty / fail:** Error banner, empty table, or failed toast — confirm you are signed in, `zyvor-fabricd` is healthy (`/readyz`), and any backend this page needs (FluxVM, storage, network) is reachable. See [Admin basics](../../admin-basics.md).

**Success:** Live data loads without error; creates/updates appear in the list or detail view and any confirmation toast clears cleanly.

## Related pages

- [Network](network.md)
- [Net Security](network-security.md)
- [Edge Dataplane](edge-dataplane.md)
- [Getting Started](../../getting-started.md)
- [Page index](../../PAGE_INDEX.md)
