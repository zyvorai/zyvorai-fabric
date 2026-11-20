# Virtual Machines

## Purpose

Virtual Machines — the fleet view of every VM in the fabric, and the starting point for managing any one of them.

## When to use it

- To see everything running (and everything stopped) at a glance
- To find a specific VM quickly once you have more than a handful
- To act on several VMs at once (start, stop, delete, archive)
- Start from the product home / dashboard if you are unsure where to begin

## How to get there

- Route / id: `/app/vms`
- Nav: **Core → Virtual Machines** (sidebar, command palette, or desktop nav)
- A single VM's detail page is `/app/vms/:name`, reached by clicking its name here

## Operate from the console (UX)

**On the list**

1. Switch between grid and table view with the toggle in the top right.
2. Search by name, image, or tag — the search box also shows "N of M VMs" so you always know if you're looking at a filtered subset.
3. Filter by tag using the tag chips; combine with search.
4. Select one or more VMs with the checkboxes, then use the bulk actions bar to start, stop, delete, or archive them together.
5. Copy a VM's name straight from the list — hover the name and click the copy icon that appears next to it.
6. Click a VM's name to open its detail page.

**On a VM's detail page** (`/app/vms/:name`)

1. Start, stop, pause, resume, restart, or clone the VM from the header actions.
2. Open a real console — either the browser terminal (xterm.js) or a graphical VNC session — from the Console tab.
3. Manage disks, network (including port forwards), **Dataplane** (FluxVM eBPF edge policy / stats / flows — see [VM Dataplane](../infrastructure/dataplane.md)), snapshots, hotplug devices, and cloud-init from their respective tabs.
4. From the **Network** tab, use **Open Dataplane** as a shortcut into the edge policy editor.
5. **Deleting a VM is undoable for a few seconds.** Confirming delete doesn't remove the VM immediately — a bar appears at the bottom of the screen with an **Undo** button and a countdown; the VM is only actually deleted once that countdown finishes unclicked. If you change your mind, click Undo before it runs out.

If the page stays empty, check service health, auth configuration, and that dependencies for this domain are installed.

## Related pages

- [Create VM](create.md)
- [VM Console](vms-name-console.md)
- [VM Dataplane](../infrastructure/dataplane.md)
- [Getting Started](../../getting-started.md)
- [Page index](../../PAGE_INDEX.md)
