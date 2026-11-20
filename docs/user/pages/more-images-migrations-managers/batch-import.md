# Batch Import

## Purpose

Batch Import — bulk-create VMs from a YAML or JSON list, with a preview step and a per-VM status readout as each one is submitted.

Creates VMs via the VM creation API one at a time after Preview. Use [Download Template] for a three-VM sample file.

## When to use it

- To stand up several VMs at once from a single definition file instead of repeating Create VM
- To reprovision from a saved YAML/JSON inventory starting from the downloadable template
- To spot bad entries (missing name or image) before anything is created
- Prefer this page when the job matches the purpose above
- When [Batch Migration](batch-migration.md) only builds a migration spec and you need actual creates

## How to get there

- Route / id: `/batch-import`
- Nav: **More — images, migrations & managers → Batch Import** (sidebar, command palette, or desktop nav)

## Operate from the console (UX)

1. **Provide input** — drop `.yaml`/`.yml`/`.json`, browse, or paste. Each entry needs `name` and `image`; `cpus`/`memory` optional (default `2` / `2G`). **Download Template** for examples.
2. **Preview** — parses into a table; missing name/image → inline error.
3. **Review** — name, vCPUs, memory, image path, status icon (pending/submitting/submitted/failed).
4. **Submit All** — creates sequentially; live row status; failures show under the image path; summary tracks total/submitted/failed.
5. **Back to Editor** — fix and re-preview without losing place.

Typical flow: Download Template → edit names/images → Preview → Submit All → check Virtual Machines / Event Stream. Respect [Quotas](../operations/quotas.md) before large batches.

**Empty / fail:** Error banner, empty table, or failed toast — confirm you are signed in, `zyvor-fabricd` is healthy (`/readyz` on `http://127.0.0.1:<port>` or `https://<host>`), and any backend this page needs (FluxVM, storage, network) is reachable. See [Admin basics](../../admin-basics.md).

**Success:** Live data loads without error; creates/updates appear in the list or detail view and any confirmation toast clears cleanly.

## Related pages

- [Create VM](../core/create.md)
- [Manifest Builder](manifest-builder.md)
- [Batch Migration](batch-migration.md)
- [Quotas](../operations/quotas.md)
- [Virtual Machines](../core/vms.md)
- [Job Monitor](job-monitor.md)
- [Pipeline](pipeline.md)
- [Getting Started](../../getting-started.md)
- [Page index](../../PAGE_INDEX.md)
