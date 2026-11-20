# Datacenters

## Purpose

Datacenters — the physical inventory tree: datacenters, the clusters inside each one, and the hosts registered to each cluster, with live CPU/memory usage and VM counts per host.

## When to use it

- To see how your infrastructure is organized — datacenter → cluster → host — before deciding where new capacity should go
- To register a new host into a cluster, or stand up a new datacenter or cluster
- To check a host's CPU/memory load, VM count, or connection status
- To put a host into maintenance mode before doing physical work on it

## How to get there

- Route / id: `/app/datacenters`
- Nav: **Core → Datacenters** (sidebar, command palette, or desktop nav)

## Operate from the console (UX)

1. Read the top-of-page summary cards: total datacenters, clusters, hosts, and VMs across the whole fabric.
2. Click a datacenter to expand it — you'll see a rollup (cluster count, host count, VM count, and, once summary data loads, total CPUs and memory) plus its list of clusters.
3. **Create a datacenter** with the button in the header — a modal asks for a name and optional description.
4. Inside an expanded datacenter, click **+ Cluster** to create a cluster there (name + description), or the trash icon to delete the datacenter — both are confirmed first.
5. Expand a cluster to see its HA and DRS status and a table of its hosts: hostname, address, CPUs, memory, live CPU/memory usage bars, VM count, and status (Connected, Disconnected, Maintenance).
6. Click **+ Host** on a cluster to register a host (hostname, IP address, CPUs, memory in MB), or use the trash icon to remove one — removal is confirmed first.
7. Toggle a host in or out of maintenance mode with the wrench icon next to it.
8. Host rows also surface **Secure Containers readiness** when heartbeats /
   FluxVM `/readyz.secure_containers` report it (`secure_containers_ready`).
   Fabric uses that flag to place [Container Groups](../operations/container-groups.md)
   — it does not talk to the Secure Containers shim directly.

**Empty / fail:** Error banner, empty table, or failed toast — confirm you are signed in, `zyvor-fabricd` is healthy (`/readyz`), and any backend this page needs (FluxVM, storage, network) is reachable. See [Admin basics](../../admin-basics.md).

**Success:** Live data loads without error; creates/updates appear in the list or detail view and any confirmation toast clears cleanly.

## Related pages

- [Dashboard](home.md)
- [Virtual Machines](vms.md)
- [Create VM](create.md)
- [Container Groups](../operations/container-groups.md)
- [Getting Started](../../getting-started.md)
- [Page index](../../PAGE_INDEX.md)
