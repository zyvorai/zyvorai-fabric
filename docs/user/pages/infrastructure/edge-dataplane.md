# Edge Dataplane (cluster)

## Purpose

Cluster-wide console for FluxVM Network Fabric **schema v4** (security groups,
CNP, identities, observe, Hubble-style packet flow, health, ipcache, FQDN refresh)
and **Service Fabric v6 (BPF schema 4)** Maglev VIP services (affinity, drain/health,
advertisements, conntrack GC, HA delta replication, EDT pacing, FluxScope flows, host-routing).

This is the **VM edge / service** control plane proxied by Fabric
(`/api/dataplane/*`). It does **not** replace [Net Security](network-security.md)
(host SDN / `/api/network-policies`).

Per-VM Status / Policy / Effective / Stats / Flows stay on the VM detail
[Dataplane](dataplane.md) tab. Service Fabric detail:
[ebpf-service-fabric.md](../../../ebpf-service-fabric.md).

## When to use it

- Create or delete shared security groups used by many VMs
- Apply or remove CNP-shaped documents (compiled onto groups)
- Upsert Maglev VIP services (east-west / north-south, NAT/DSR, drain/health,
  optional `max_egress_mbps` / `flow_sample_rate` / `host_routing`)
- Check dataplane health after upgrades (`ok`, BPF object, bpffs)
- Inspect Service Fabric host schema, backend health, VIP advertisements, flows
- Inspect reserved + group identities and observe endpoints
- Re-resolve FQDN allowlists after DNS changes

## How to get there

- Route: `/app/edge-dataplane`
- Nav: **Infrastructure → Edge Dataplane** (beside Net Security)
- Command palette: search “Edge Dataplane”

Sign in at `/sign-in` with the two-step flow: enter username → **Continue** → password.

## Operate from the console (UX)

| Tab | Actions |
|-----|---------|
| **Health** | Mode, BPF/bpffs presence, group/CNP/ipcache counts, notes |
| **Services** | Maglev upsert/delete; schema badge; health reconcile; ads JSON; conntrack GC; HA deltas |
| **Groups** | Quick create (name + label) or delete rows |
| **CNP** | Paste JSON → Apply; delete listed documents |
| **Identities** | Reserved entities + group identities |
| **Observe** | Read-only JSON snapshot |
| **Packet flow** | Hubble-lite hops with Colorful / Normal theme |
| **Ipcache** | Guest IP → identity table |
| Header **Refresh DNS** | `POST /api/dataplane/refresh-dns` (admin); best-effort — succeeds even when FluxVM has no FQDN entries to refresh |

Write actions require admin/write role. Soft banner appears when the
`vm_dataplane` capability is off or unreachable.

## Related pages

- [VM Dataplane](dataplane.md) — per-VM tab
- [Service Fabric v6 (BPF schema 4)](../../../ebpf-service-fabric.md) — ownership, API, leases, HA deltas, identity/L7 policy
- [Net Security](network-security.md) — host SDN (orthogonal)
- Tutorials: [edge-dataplane/](../../../tutorials/edge-dataplane/README.md)
- Operator: [fluxvm-dataplane.md](../../../guides/vm-drivers/fluxvm-dataplane.md)
- [Page index](../../PAGE_INDEX.md)
