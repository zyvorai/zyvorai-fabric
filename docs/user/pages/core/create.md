# Create VM

## Purpose

Create VM — a three-step wizard (Basics → Resources → Review) for launching a new virtual machine, including how it's networked and whether it's reachable from outside the host.

## When to use it

- To provision a new VM from a catalog image or a custom disk path
- To decide up front how the VM will be reachable — private-and-forwarded, or a real address on your network
- Start from the product home / dashboard if you are unsure where to begin

## How to get there

- Route / id: `/app/create`
- Nav: **Core → Create VM** (sidebar, command palette, or desktop nav)

## Operate from the console (UX)

**1. Basics** — name the VM and pick a disk image, either from the built-in catalog or a path on the host.

**2. Resources** — set vCPUs, memory, and root disk size. Under **Advanced Options** you choose networking:

- **NAT (default)** — the VM gets a private, outbound-only address, same as most VMs today. To make a service on the VM reachable from your laptop or the wider network, use **Expose ports**: add a host port → guest port mapping (there's a one-click **Expose SSH (22)** preset for the common case of just needing to SSH in). Forwarded ports are reachable from any client that can reach the host — not just from the host itself.
- **Bridged** — the VM gets its own real address on the local network via a dedicated network namespace, instead of sharing the host's NAT. Two addressing options:
  - **DHCP** (default under Bridged) — the VM's guest agent leases an address automatically.
  - **Assign the IP statically via cloud-init** — check this box to bake a fixed address into the VM at boot instead of waiting on DHCP, useful when the guest image doesn't run a DHCP client or you need a predictable address before first boot.

**3. Review** — confirms name, image, resources, and the networking mode you chose, then creates the VM.

**After creation** — port forwards aren't locked in at creation time: open the VM's **Network** tab from its detail page to add or remove forwards later (the VM restarts automatically if it's running when you do).

If the page stays empty, check service health, auth configuration, and that dependencies for this domain are installed.

## Related pages

- [Virtual Machines](vms.md)
- [Network](../infrastructure/network.md)
- [Getting Started](../../getting-started.md)
- [Page index](../../PAGE_INDEX.md)
