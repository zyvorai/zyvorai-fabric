# Network Topology

## Purpose

Network Topology — a live map of which VMs are attached to which virtual networks or host bridges, alongside the host's own bridges and physical NICs, auto-refreshing every 15 seconds.

Read-only. Change attachments via Create VM or the VM's Network tab. For day-2 networking modes/port-forwards see [Network](../infrastructure/network.md); for Network Fabric schema v4 edge see [VM Dataplane](../infrastructure/dataplane.md); Service Fabric v6 lives on [Edge Dataplane](../infrastructure/edge-dataplane.md).

## When to use it

- To see which VMs share a network or bridge, or find VMs that aren't attached
- To check a host bridge's or physical NIC's operational state and addresses
- To look up a VM's interface type, MAC, and network without opening each VM
- Prefer this page when the job matches the purpose above
- During network incidents before changing Net Security policies

## How to get there

- Route / id: `/network-topology`
- Nav: **More — images, migrations & managers → Network Topology** (sidebar, command palette, or desktop nav)

## Operate from the console (UX)

1. Loads VM interfaces, virtual networks, host bridges, and links on open; refreshes every 15s; header **Refresh** forces reload.
2. Summary counts: virtual networks, host bridges, VMs.
3. **Host interfaces** — bridge/NIC cards with operational state and addresses.
4. **VM → network connections** — table of VM name/state, network/bridge, interface type, MAC.
5. **VM attachments** — per network/bridge cards listing attached VMs; unattached VMs grouped under "Unattached VMs."

Typical flow: find Unattached VMs → open VM Network tab to attach → Refresh topology → confirm. Do not confuse this map with eBPF dataplane policy (schema v4) or Maglev Services (Service Fabric v6).

**Empty / fail:** Error banner, empty table, or failed toast — confirm you are signed in, `zyvor-fabricd` is healthy (`/readyz` on `http://127.0.0.1:<port>` or `https://<host>`), and any backend this page needs (FluxVM, storage, network) is reachable. See [Admin basics](../../admin-basics.md).

**Success:** Live data loads without error; creates/updates appear in the list or detail view and any confirmation toast clears cleanly.

## Related pages

- [Network](../infrastructure/network.md)
- [Net Security](../infrastructure/network-security.md)
- [VM Dataplane](../infrastructure/dataplane.md)
- [Edge Dataplane](../infrastructure/edge-dataplane.md)
- [Service Map](../monitoring/service-map.md)
- [Virtual Machines](../core/vms.md)
- [Getting Started](../../getting-started.md)
- [Page index](../../PAGE_INDEX.md)
