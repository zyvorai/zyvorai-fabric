# Migration Templates

## Purpose

Migration Templates — reusable migration configuration presets (disk format, vCPUs, memory, network, compression) that you can copy as JSON instead of re-entering the same settings for every migration.

Presets are copied as JSON for reuse; they do not by themselves execute a migration.

## When to use it

- To standardize migration settings across many similar VMs
- To copy a preset as JSON into Batch Migration or an external runner
- Prefer this page when the job matches the purpose above
- When teams keep repeating the same format/CPU/memory/compression choices

## How to get there

- Route / id: `/migration-templates`
- Nav: **More — images, migrations & managers → Migration Templates** (sidebar, command palette, or desktop nav)

## Operate from the console (UX)

1. Browse existing migration presets (format, vCPUs, memory, network, compression).
2. Create or edit a preset when your standard settings change.
3. Copy the preset JSON for use in Batch Migration or other tools.
4. Apply the values in [Migration Wizard](../operations/migration-wizard.md) / [Batch Migration](batch-migration.md) as appropriate.
5. Refresh the list after teammates add shared presets.

Typical flow: define a QCOW2 + sizing preset → copy JSON → paste into batch work → run readiness + wizard. For VM resource templates (not migration), see Operations → [Templates](../operations/templates.md).

Operator tip: treat copied JSON as a starting point — still run Readiness before a large wave.

**Empty / fail:** Error banner, empty table, or failed toast — confirm you are signed in, `zyvor-fabricd` is healthy (`/readyz` on `http://127.0.0.1:<port>` or `https://<host>`), and any backend this page needs (FluxVM, storage, network) is reachable. See [Admin basics](../../admin-basics.md).

**Success:** Live data loads without error; creates/updates appear in the list or detail view and any confirmation toast clears cleanly.

## Related pages

- [Batch Migration](batch-migration.md)
- [Migration Wizard](../operations/migration-wizard.md)
- [Templates](../operations/templates.md)
- [Migration Readiness](migration-readiness.md)
- [Job Monitor](job-monitor.md)
- [Pipeline](pipeline.md)
- [Getting Started](../../getting-started.md)
- [Page index](../../PAGE_INDEX.md)
