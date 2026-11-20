# Network

## Purpose

Network — day-to-day VM networking: which mode a VM uses, its port forwards, and its assigned address. For the advanced SDN stack (policies, firewalls, VPN mesh, QoS, mirroring), see [Net Security](network-security.md). For per-VM TC/eBPF edge allowlists, rate limits, and flows, see [VM Dataplane](dataplane.md).

## When to use it

- To see how a VM is networked and what's reachable from outside the host
- To add or remove a port forward without recreating the VM
- To check a VM's assigned IP address
- To jump into **Dataplane** for eBPF edge policy on a bridged VM

## How to get there

- Route / id: `/app/network`
- Nav: **Infrastructure → Network** (sidebar, command palette, or desktop nav)
- Per-VM networking is also managed from that VM's own **Network** tab on its detail page (includes **Open Dataplane**)

## Operate from the console (UX)

1. Review VMs by networking mode:
   - **NAT** — private, outbound-only by default; individual ports can be exposed with a host→guest port forward (set at creation in the [Create VM](../core/create.md) wizard, or added later from a VM's Network tab).
   - **Bridged** — the VM has its own address on the local network, either DHCP-assigned or set statically via cloud-init at boot. Bridged VMs are what attach FluxVM Network Fabric eBPF.
2. Add or remove a port forward for a running or stopped VM — if the VM is running, it restarts automatically to apply the change.
3. Confirm a forwarded port is actually reachable from another machine on your network, not just from the host itself.
4. Check a bridged VM's assigned IP address once it's leased (or immediately, if it was assigned statically).
5. Open **Dataplane** from the Network teaser when you need edge allowlists / Mbps caps / flows.

If the page stays empty, check service health, auth configuration, and that dependencies for this domain are installed.

## Related pages

- [VM Dataplane](dataplane.md)
- [Net Security](network-security.md)
- [Create VM](../core/create.md)
- [Virtual Machines](../core/vms.md)
- [Getting Started](../../getting-started.md)
- [Page index](../../PAGE_INDEX.md)
