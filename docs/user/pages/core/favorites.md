# Favorites

## Purpose

Favorites — a personal, starred shortlist of VMs pulled from your full VM list, so the machines you use most are one click away instead of buried in a longer list.

Favorites are **browser-local** (local storage). They do not sync across browsers, devices, or user accounts on the host.

## When to use it

- To jump straight to the handful of VMs you check on or console into regularly
- To pin or unpin VMs as your day-to-day working set changes
- To search across every VM (not just the pinned ones) from a single search box
- When the full [Virtual Machines](vms.md) list is long and you only need a few daily targets
- After create/start, pin the new VM so it stays visible without hunting by name

## How to get there

- Route / id: `/app/favorites`
- Nav: **Core → Favorites** (sidebar, command palette, or desktop nav)

## Operate from the console (UX)

1. Open Favorites and wait for the VM list to load from the server (same fleet as `/app/vms`).
2. Click the **star** next to any VM to pin or unpin it. Pinned VMs move into **Pinned VMs**; everything else stays under **All VMs**.
3. Use the **search** box to filter both sections by name or state.
4. Each row shows vCPU count, memory, and a state badge. Click the **name** for the VM detail page, or **Console** for the console tab.
5. **Refresh** reloads the underlying VM list from the server (stars themselves stay in local storage).
6. Clearing site data / using another browser resets the shortlist — re-star the VMs you need.

Typical flow: pin the VMs you will touch in a change window → use search to confirm state → open Console or detail from the pinned section → unpin when the work is done so the shortlist stays small.

**Empty / fail:** Error banner, empty table, or failed toast — confirm you are signed in, `zyvor-fabricd` is healthy (`/readyz` on `http://127.0.0.1:<port>` or `https://<host>`), and any backend this page needs (FluxVM, storage, network) is reachable. See [Admin basics](../../admin-basics.md).

**Success:** Live data loads without error; creates/updates appear in the list or detail view and any confirmation toast clears cleanly.

## Related pages

- [Dashboard](home.md)
- [Virtual Machines](vms.md)
- [VM Browser](vm-browser.md)
- [VM Console](vms-name-console.md)
- [Create VM](create.md)
- [Getting Started](../../getting-started.md)
- [Page index](../../PAGE_INDEX.md)
