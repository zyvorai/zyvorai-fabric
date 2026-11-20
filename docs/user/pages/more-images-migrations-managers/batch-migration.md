# Batch Migration

## Purpose

Batch Migration Builder — a form-based editor for assembling a multi-VM migration job spec (source disk, target format, sizing) and exporting it as JSON. It builds the spec only; it does **not** run the migration itself.

Export with **Copy** or **Download** (`batch-migration.json`) for tools/pipelines elsewhere.

## When to use it

- To describe several VMs' migrations (source path, target format, vCPUs, memory) before handing off
- To generate a JSON migration manifest to copy, save, or version-control
- To sketch VMDK→QCOW2 conversions without a text editor
- Prefer this page when the job matches the purpose above
- When you want a spec first; run live imports later via Migration Wizard / pipelines

## How to get there

- Route / id: `/batch-migration`
- Nav: **More — images, migrations & managers → Batch Migration** (sidebar, command palette, or desktop nav)

## Operate from the console (UX)

1. **Add VM** — adds a numbered, expanded entry card.
2. Fill **VM Name**, **Source Path** (e.g. `/path/to/disk.vmdk`), **Target Format** (QCOW2/RAW/VMDK), **vCPUs**, **Memory (MB)**. Collapse headers to scan name/source.
3. Trash icon removes an entry.
4. **JSON Preview** builds `migrations: [...]` with `vm_name`, `source_path`, `target_format`, `cpus`, `memory_mb` once ≥1 entry exists.
5. **Copy** (clipboard confirmation) or **Download** as `batch-migration.json`.

Typical flow: Add entries → preview JSON → Download → feed your runner → watch [Pipeline](pipeline.md) / [Migration History](migration-history.md). For a single guided import that actually runs, use [Migration Wizard](../operations/migration-wizard.md).

**Empty / fail:** Error banner, empty table, or failed toast — confirm you are signed in, `zyvor-fabricd` is healthy (`/readyz` on `http://127.0.0.1:<port>` or `https://<host>`), and any backend this page needs (FluxVM, storage, network) is reachable. See [Admin basics](../../admin-basics.md).

**Success:** Live data loads without error; creates/updates appear in the list or detail view and any confirmation toast clears cleanly.

## Related pages

- [Migration Wizard](../operations/migration-wizard.md)
- [Migration Templates](migration-templates.md)
- [Migration Readiness](migration-readiness.md)
- [Migration History](migration-history.md)
- [Job Monitor](job-monitor.md)
- [Pipeline](pipeline.md)
- [Getting Started](../../getting-started.md)
- [Page index](../../PAGE_INDEX.md)
