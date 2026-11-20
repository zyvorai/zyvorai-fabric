# Getting Started with Zyvor Fabric

Your first hour on a fresh Fabric install: open the UI, sign in, read the dashboard, create a VM, and open its console.

## What you need

- A host running `zyvor-fabricd` (see [Admin basics](admin-basics.md) for ports, auth, deploy, TLS, and the FluxVM dependency)
- Browser access to `https://<host>:9095` (or `http://127.0.0.1:9095` on the host itself)
- Admin credentials — retrieve with `./zyvor-fabricd-ctl password` or `sudo cat /var/lib/zyvor-fabricd/.admin_password`

Never hardcode lab IPs in docs or scripts; use `<host>` or `127.0.0.1`.

## 1. Open the product

Browse to `https://<host>:9095/` (or `http://127.0.0.1:9095/` locally).

You land on the public marketing home. Product pages: `/product`, `/platform`, `/security`. The authenticated console lives under `/app/*`.

## 2. Sign in

1. Open `/sign-in` (or click **Sign in** in the top nav). Legacy `/login` redirects here.
2. Enter username `admin` → **Continue** → paste the generated admin password.
3. On success you land on the console dashboard at `/app`.

Failed sign-in shows an inline error — confirm `zyvor-fabricd` is up (`./zyvor-fabricd-ctl status`) and that you have the current password.

Details: [Sign in](pages/auth/login.md).

## 3. Orient on the dashboard

At `/app`:

1. Check subsystem status chips (VM driver / FluxVM, storage, network security, **VM dataplane**, auth, events) — each should read Live when healthy. **VM dataplane** reports Network Fabric `schema=4` when eBPF is attached.
2. Read the VM count cards (total / running / stopped) and live CPU/memory charts.
3. On a fresh install with no VMs, use the Getting Started links to create a VM or open the API playground.

Full nav map: [Using the Dashboard](using-the-dashboard.md). Page guide: [Dashboard](pages/core/home.md).

## 4. Create your first VM

1. Go to **Core → Create VM** (`/app/create`), or use the dashboard shortcut.
2. **Basics** — name the VM and pick a catalog image (or a host disk path).
3. **Resources** — set vCPUs, memory, disk. Under Advanced Options choose **NAT** (default; use **Expose ports** / **Expose SSH (22)** if you need inbound) or **Bridged** (own address on the LAN — required later for VM Dataplane eBPF).
4. **Review** → create. You are taken to the new VM’s detail page (`/app/vms/:name`).

Guide: [Create VM](pages/core/create.md).

## 5. Open the console

1. From the VM detail page, open the **Console** tab (or `/app/vms/:name/console`).
2. Use **Terminal** for an interactive serial/PTY shell, or **VNC** for a graphical framebuffer.
3. Confirm the VM state badge is **running**. If the console stays black, wait a few seconds after start for QEMU/FluxVM to come up, then refresh.

Guides: [VM Console](pages/core/vms-name-console.md) · [Virtual Machines](pages/core/vms.md).

## Next workflows

| Job | Start here |
|-----|------------|
| Fleet list / bulk start-stop | [Virtual Machines](pages/core/vms.md) (`/app/vms`) |
| Per-VM eBPF edge policy | [VM Dataplane](pages/infrastructure/dataplane.md) |
| Maglev VIP / Service Fabric v6 | [Edge Dataplane](pages/infrastructure/edge-dataplane.md) |
| Snapshots / backups | [Snapshots](pages/operations/snapshots.md) · [Backups](pages/operations/backups.md) |
| Common paths end-to-end | [Common workflows](workflows.md) |

## Related

- [Admin basics](admin-basics.md)
- [Using the Dashboard](using-the-dashboard.md)
- [Common workflows](workflows.md)
- [Page-by-page guides](pages/README.md)
- [Complete page index](PAGE_INDEX.md)
- Operator web UI notes: [Web UI guide](../getting-started/04-Web-UI.md)
