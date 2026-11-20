# VM Browser

## Purpose

VM Browser — a lightweight, read-only grid of every VM, for quickly scanning or searching without the bulk-action tooling of the full [Virtual Machines](vms.md) list.

Use this when you need to **find** a VM; use Virtual Machines when you need to **act** on one or many (start/stop, tags, bulk tools).

## When to use it

- To browse or search VMs by name, state, or image in a compact card layout
- To jump straight to a single VM's detail page without selection, tag filters, or bulk actions
- When you only need a visual inventory (image, vCPUs, memory, IP) and not lifecycle controls
- Prefer this page when the job matches the purpose above
- Start from the Dashboard (`/app`) if you are unsure where to begin

## How to get there

- Route / id: `/app/vm-browser`
- Nav: **Core → VM Browser** (sidebar, command palette, or desktop nav)

## Operate from the console (UX)

1. Open the page and note the header counts: total VMs and how many are running.
2. **Search** by name, state, or image — the card grid filters as you type.
3. Each card shows name, state badge, image, vCPU count, memory, and IP (if assigned).
4. Click a card to open that VM's detail page (`/app/vms/:name`).
5. **Refresh** reloads the list from the server.
6. There are no start/stop/snapshot actions here by design — switch to [Virtual Machines](vms.md) or [Bulk Operations](../operations/bulk-operations.md) for those.

Typical flow: search by image or state → open the card → continue on detail (console, network, dataplane). For starring a daily set, use [Favorites](favorites.md) instead of re-searching here.

**Empty / fail:** Error banner, empty table, or failed toast — confirm you are signed in, `zyvor-fabricd` is healthy (`/readyz` on `http://127.0.0.1:<port>` or `https://<host>`), and any backend this page needs (FluxVM, storage, network) is reachable. See [Admin basics](../../admin-basics.md).

**Success:** Live data loads without error; creates/updates appear in the list or detail view and any confirmation toast clears cleanly.

## Related pages

- [Dashboard](home.md)
- [Virtual Machines](vms.md)
- [Favorites](favorites.md)
- [Create VM](create.md)
- [VM Console](vms-name-console.md)
- [Getting Started](../../getting-started.md)
- [Page index](../../PAGE_INDEX.md)
