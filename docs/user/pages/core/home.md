# Dashboard

## Purpose

Dashboard — the fabric at a glance: how many VMs exist and in what state, live CPU/memory trends, and whether each backend subsystem is actually reachable.

This is the post-sign-in landing page (`/app`). Treat capability chips as the first health gate before change windows.

## When to use it

- As your console landing page — it loads at `/app`
- To check overall health before digging into a specific VM or subsystem
- To confirm FluxVM / storage / network / dataplane capability chips before a change window
- Start here if you're new to the product; a first-time install lands somewhere useful, not an empty table
- To verify **VM dataplane** reports Network Fabric **schema=4** when eBPF edge is expected

## How to get there

- Route: `/app`
- Nav: **Core → Dashboard** (sidebar or command palette), or the wordmark in the top bar
- After sign-in at `/sign-in` you land here
- Open `http://127.0.0.1:<port>/app` or `https://<host>/app` depending on how you run the console

## Operate from the console (UX)

1. Check subsystem status chips (VM driver, storage, network security, **VM dataplane**, authentication, events) — each shows Live, Unreachable, or Off.
2. For **VM dataplane**, a healthy FluxVM Network Fabric edge reads like `mode=ebpf · attached · schema=4`. If schema is missing or not 4, do not rely on Guard/deny policy until [VM Dataplane](../infrastructure/dataplane.md) shows Attached + schema 4.
3. Read the stat cards: total VMs, running, stopped, and total allocated memory/vCPUs.
4. Watch live CPU and memory usage charts when VMs are running.
5. Scan the VM table, or open [Virtual Machines](vms.md) for full fleet actions.
6. **On a fresh install with no VMs yet**, use Getting Started links to create a VM, templates, API playground, or access control.
7. For Maglev Services / Service Fabric **v6** cluster console, open [Edge Dataplane](../infrastructure/edge-dataplane.md) (`/app/edge-dataplane`) — that is separate from the per-VM Network Fabric schema v4 chip.

**Empty / fail:** Error banner, empty table, or failed toast — confirm you are signed in, `zyvor-fabricd` is healthy (`/readyz` on `http://127.0.0.1:<port>` or `https://<host>`), and any backend this page needs (FluxVM, storage, network) is reachable. See [Admin basics](../../admin-basics.md).

**Success:** Live data loads without error; creates/updates appear in the list or detail view and any confirmation toast clears cleanly.

## Related pages

- [Virtual Machines](vms.md)
- [VM Dataplane](../infrastructure/dataplane.md)
- [Edge Dataplane](../infrastructure/edge-dataplane.md)
- [Create VM](create.md)
- [Favorites](favorites.md)
- [System Health](../infrastructure/system-health.md)
- [Getting Started](../../getting-started.md)
- [Page index](../../PAGE_INDEX.md)
