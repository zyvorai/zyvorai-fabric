# Storage Mgr

## Purpose

Storage Manager — browse storage pools with their capacity and usage, and drill into a pool to see its volumes.

Read-only. Pool lifecycle lives on [Storage Pools](../infrastructure/storage-pools.md); the tracking ledger on [Storage](../infrastructure/storage.md).

## When to use it

- To check how full a storage pool is before provisioning new disks on it
- To see what volumes live in a given pool, and their format and size
- To find a volume's on-disk path
- Prefer this page when the job matches the purpose above
- When you want a quick pool→volumes browser under More

## How to get there

- Route / id: `/storage-manager`
- Nav: **More — images, migrations & managers → Storage Mgr** (sidebar, command palette, or desktop nav)

## Operate from the console (UX)

1. Pools load on open; header **Refresh** reloads.
2. Pool cards: name, type badge (dir / logical / netfs / disk / iscsi / rbd / zfs), state, usage bar (amber &gt;70%, red &gt;90%), used/capacity/available.
3. Click a pool to load its volumes.
4. Volumes table: Name, Format, Capacity, Allocation, Path.
5. No create/resize/delete here — use Storage Pools / Storage for management vs ledger.

Typical flow: scan red/amber pools → click pool → note volume paths → free space or expand via Storage Pools before new VMs. Capacity Planning gives host-level trends.

**Empty / fail:** Error banner, empty table, or failed toast — confirm you are signed in, `zyvor-fabricd` is healthy (`/readyz` on `http://127.0.0.1:<port>` or `https://<host>`), and any backend this page needs (FluxVM, storage, network) is reachable. See [Admin basics](../../admin-basics.md).

**Success:** Live data loads without error; creates/updates appear in the list or detail view and any confirmation toast clears cleanly.

## Related pages

- [Storage](../infrastructure/storage.md)
- [Storage Pools](../infrastructure/storage-pools.md)
- [Distributed Storage](../infrastructure/distributed-storage.md)
- [Disk Images](disk-images.md)
- [Capacity](../monitoring/capacity-planning.md)
- [Job Monitor](job-monitor.md)
- [Pipeline](pipeline.md)
- [Getting Started](../../getting-started.md)
- [Page index](../../PAGE_INDEX.md)
