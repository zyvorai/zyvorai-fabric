# Storage Pools

## Purpose

Storage Pools — create, start/stop, and monitor the storage backends VM disks live on: local directories, NFS exports, LVM and LVM-thin volume groups, ZFS pools, or Ceph RBD pools. This is where a pool's lifecycle and health live; for the volumes (disks) inside those pools see [Storage](storage.md), and for multi-host replicated/policy-driven storage see [Distributed Storage](distributed-storage.md).

## When to use it

- To add a new storage backend — e.g. mount an NFS export or attach a Ceph RBD pool — for VM disks to be created on
- To start or stop a pool, or delete one that's no longer needed
- To check NFS or Ceph pool health at a glance
- To search for a pool by name, path, or state and see its live capacity/usage

## How to get there

- Route / id: `/app/storage-pools`
- Nav: **Infrastructure → Storage Pools** (sidebar, command palette, or desktop nav)

## Operate from the console (UX)

1. Review stat cards: total pools, active pools, total capacity, and total available space.
2. Search pools by name, path, or state.
3. Table of pools — name, type with backend-specific detail (NFS server:export path, LVM volume group, LVM-thin volume group/thin pool, ZFS zpool/dataset, or Ceph pool name + monitor count), path, capacity, available space, a usage bar, current state, and a health indicator for NFS/Ceph pools.
4. **Create Pool** — pick a type (Local, NFS, LVM, LVM-thin, ZFS, Ceph) and fill in its backend-specific fields (for NFS: server, export path, mount path, NFS version, mount options; for Ceph: monitor addresses, pool name, optional user/keyring; etc.), plus an auto-start-on-daemon-boot toggle.
5. Per-pool actions: **Start** an inactive pool, **Stop** an active one, **Refresh** to pull current stats, or **Delete** — disabled while the pool is active, so stop it first — with a confirmation dialog.
6. For Ceph pools, a **Manage RBD images** action opens a panel to list, create, and delete raw RBD images in that pool. This is proxied through the Atlas storage control plane (Zyvor's Ceph system of record) and requires Atlas to be configured on the backend; image creation is queued and completes asynchronously rather than immediately.

If the page stays empty, check service health, auth configuration, and that dependencies for this domain are installed. Ceph pool health/stats and RBD image management additionally require an Atlas connection — if that's not configured, those specific actions will show an unavailable error rather than the whole page failing.

**Empty / fail:** Error banner, empty table, or failed toast — confirm you are signed in, `zyvor-fabricd` is healthy (`/readyz`), and any backend this page needs (FluxVM, storage, network) is reachable. See [Admin basics](../../admin-basics.md).

**Success:** Live data loads without error; creates/updates appear in the list or detail view and any confirmation toast clears cleanly.

## Related pages

- [Network](network.md)
- [Net Security](network-security.md)
- [Edge Dataplane](edge-dataplane.md)
- [Getting Started](../../getting-started.md)
- [Page index](../../PAGE_INDEX.md)
