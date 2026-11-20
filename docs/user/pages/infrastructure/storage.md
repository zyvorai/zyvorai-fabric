# Storage

## Purpose

Storage — a consolidated view of every storage pool's capacity alongside a manual volume tracking ledger. To create or manage a pool itself, use [Storage Pools](storage-pools.md); for replicated/policy-driven storage across hosts, see [Distributed Storage](distributed-storage.md).

**Important:** the Volumes table is a **manual tracking ledger**, not live disk provisioning. Resize/Attach/Detach/Delete here update the record only — they do not change real disks or VM configs.

## When to use it

- To see total capacity, used space, and pool count across the whole host in one place
- To keep a manual record of volumes you've provisioned elsewhere (pool, size, intended VM)
- Prefer this page when the job matches the purpose above
- Start from the [Dashboard](../core/home.md) if you are unsure where to begin
- Before creating large VMs, to see which pools are near full

## How to get there

- Route / id: `/app/storage`
- Nav: **Infrastructure → Storage** (sidebar, command palette, or desktop nav)

## Operate from the console (UX)

1. Review stat cards: total capacity, used space, volume count, and pool count.
2. Storage Pools panel — name, path, type badge, state, used/total bar, **Add Volume Record**.
3. Volumes table — name, pool, size, attached VM (or "Not attached"), plus **Resize**, **Attach**/**Detach**, **Delete** (confirmation).
4. Use records as notes for volumes you provision via Storage Pools (including Ceph RBD image management) or host tools.
5. Pool create/start/stop remains on [Storage Pools](storage-pools.md); multi-host policy on [Distributed Storage](distributed-storage.md).

Typical flow: check pool fullness → add volume records for inventory → manage real pools on Storage Pools → use [Storage Mgr](../more-images-migrations-managers/storage-manager.md) for a read-only pool/volume browser if preferred.

Operator tip: if a pool bar is red, free space or expand on Storage Pools before creating large disks.

**Empty / fail:** Error banner, empty table, or failed toast — confirm you are signed in, `zyvor-fabricd` is healthy (`/readyz` on `http://127.0.0.1:<port>` or `https://<host>`), and any backend this page needs (FluxVM, storage, network) is reachable. See [Admin basics](../../admin-basics.md).

**Success:** Live data loads without error; creates/updates appear in the list or detail view and any confirmation toast clears cleanly.

## Related pages

- [Storage Pools](storage-pools.md)
- [Distributed Storage](distributed-storage.md)
- [Storage Mgr](../more-images-migrations-managers/storage-manager.md)
- [Capacity](../monitoring/capacity-planning.md)
- [Disk Images](../more-images-migrations-managers/disk-images.md)
- [Getting Started](../../getting-started.md)
- [Page index](../../PAGE_INDEX.md)
