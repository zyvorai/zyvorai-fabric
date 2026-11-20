# Net Security

## Purpose

Net Security — the advanced SDN and security control plane: network policies scoped to security identities, host firewall profiles/zones/VM assignments, exposed services, QoS traffic shaping, DNS zones/policies, WireGuard VPN tunnels and networks, traffic mirroring, NAT rules/pools/gateways, and bandwidth monitoring with alerts.

For everyday per-VM networking mode and port forwards, see [Network](network.md). For **per-VM TC/eBPF edge** allowlists, groups/CNP, Mbps/PPS, and flows (FluxVM Network Fabric schema v4), see [VM Dataplane](dataplane.md) and [Edge Dataplane](edge-dataplane.md) — that plane is orthogonal and does not replace these host policies. Maglev VIP load balancing is Service Fabric v6 (BPF schema 4) on the same Edge Dataplane **Services** tab ([ebpf-service-fabric.md](../../../ebpf-service-fabric.md)).

## When to use it

- To write a network policy scoped to a security identity (a label-based group of VMs) rather than to individual IPs
- To manage host firewall zones/profiles and see which VMs are assigned to which firewall profile
- To set up a WireGuard VPN tunnel or network between sites
- To rate-limit a VM's traffic with a QoS policy, or mirror its traffic to a collector for inspection
- To bring firewalld/nftables/WireGuard configuration that already exists on the host under Zyvor's management instead of recreating it by hand

## How to get there

- Route / id: `/app/network-security`
- Nav: **Infrastructure → Net Security** (sidebar, command palette, or desktop nav)

## Operate from the console (UX)

Nine tabs, each with its own live count in the header: **Policies, Firewall, Services, QoS, DNS, VPN, Mirror, NAT, Monitor**. Data refreshes automatically every 15 seconds and on demand via **Refresh**. Read-only accounts see a read-only notice and can't create, adopt, sync, or delete anything.

1. **Policies** — network policies bound to security identities (identities are auto-created from VM labels or discovered from firewalld zones/nftables sets on the host). Create or delete a policy; adopt a discovered policy or identity into management.
2. **Firewall** — firewall profiles, zones, and per-VM assignments. Create/delete profiles and zones, remove a VM's firewall assignment, and adopt a zone or profile that already exists on the host.
3. **Services** — exposed host listeners. Create/delete a service, or adopt one already running on the host.
4. **QoS** — bandwidth-shaping policies. Create/delete/adopt.
5. **DNS** — DNS zones and DNS policies. Create/delete/adopt both zones and policies.
6. **VPN** — WireGuard tunnels and VPN networks. Create/delete tunnels and networks; adopt a tunnel already configured on the host.
7. **Mirror** — traffic mirror sessions that copy a VM's traffic to a collector address. Create/delete/adopt.
8. **NAT** — NAT rules, NAT pools, and gateway configs. Create/delete each; adopt a NAT rule found on the host.
9. **Monitor** — bandwidth monitor policies, live traffic metrics, and bandwidth alerts. Create/delete/adopt a monitor policy, and acknowledge an alert.

Every tab has a **Sync** action that rescans the host's actual configuration (firewalld, nftables, WireGuard, etc.) for items not yet under Zyvor's management — synced items show up as adoptable so you can bring them under management with one click instead of recreating them.

If the page stays empty, check service health, auth configuration, and that dependencies for this domain are installed.

## Related pages

- [VM Dataplane](dataplane.md) — FluxVM eBPF edge (orthogonal)
- [Network](network.md) — NAT / bridge / port forwards
- [Getting Started](../../getting-started.md)
- [Page index](../../PAGE_INDEX.md)
