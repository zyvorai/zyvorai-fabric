# Templates

## Purpose

VM Templates — reusable VM configurations (CPU/memory/disk, tags) that you save from an existing VM and use to stamp out new VMs quickly, instead of configuring resources from scratch each time.

Templates capture **resource configuration**, not a full disk clone. For disk images and ISOs, see [Content Library](content-library.md) and the images managers under More.

## When to use it

- To create a new VM with the same resource configuration as one you've already tuned
- To standardize resource sizing across a team (e.g. a "small-dev" or "large-prod" template)
- To browse the templates already saved and their specs before deciding which to use
- After you have at least one well-sized VM you trust as a pattern
- When [Profiles](../core/profiles.md) cover sizing only and you also need tags/defaults from a real VM

## How to get there

- Route / id: `/app/templates`
- Nav: **Operations → Templates** (sidebar, command palette, or desktop nav)

## Operate from the console (UX)

1. Templates aren't created on this page — click **Create from VM** to jump to [Virtual Machines](../core/vms.md), where a template is saved from an existing VM's configuration.
2. Each saved template shows as a card with name, description, CPUs, memory (MB), disk size (GB), tags, and creation date.
3. **Create VM** on a template card opens a dialog asking only for a new VM name, then deploys a new VM using that template's saved resource configuration.
4. **Delete** removes a template (confirmation dialog, cannot be undone) — this does not affect VMs already created from it.
5. If no templates exist yet, the empty state points you to the VMs page to create one.

Typical flow: tune one golden VM → save template from VMs → stamp new names from the template cards → verify on Dashboard / Virtual Machines. Pair with [Quotas](quotas.md) if teams share the host.

**Empty / fail:** Error banner, empty table, or failed toast — confirm you are signed in, `zyvor-fabricd` is healthy (`/readyz` on `http://127.0.0.1:<port>` or `https://<host>`), and any backend this page needs (FluxVM, storage, network) is reachable. See [Admin basics](../../admin-basics.md).

**Success:** Live data loads without error; creates/updates appear in the list or detail view and any confirmation toast clears cleanly.

## Related pages

- [Virtual Machines](../core/vms.md)
- [Create VM](../core/create.md)
- [Profiles](../core/profiles.md)
- [Content Library](content-library.md)
- [Quotas](quotas.md)
- [Backups](backups.md)
- [Getting Started](../../getting-started.md)
- [Page index](../../PAGE_INDEX.md)
