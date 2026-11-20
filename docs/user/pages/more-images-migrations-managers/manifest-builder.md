# Manifest Builder

## Purpose

Manifest Builder — a client-side form for assembling a VM configuration manifest and exporting it as YAML, with a live preview as you type. It doesn't create a VM or call the API; it's a scratchpad for drafting config to copy elsewhere.

No server mutation. Pair with [Batch Import](batch-import.md) or API Playground when you are ready to apply.

## When to use it

- To draft a VM YAML manifest with live preview before pasting into automation
- To explore fields without risking a create call
- Prefer this page when the job matches the purpose above
- When teaching teammates the shape of a VM config without using production APIs

## How to get there

- Route / id: `/manifest-builder`
- Nav: **More — images, migrations & managers → Manifest Builder** (sidebar, command palette, or desktop nav)

## Operate from the console (UX)

1. Fill the form fields for the VM configuration you want to express.
2. Watch the **live YAML preview** update as you type.
3. Copy or export the YAML when it looks right.
4. Apply elsewhere (Batch Import, API, git) — this page never submits creates.
5. Start over by clearing fields if the draft goes wrong.

Typical flow: draft → copy YAML → paste into Batch Import or an external pipeline → verify on Virtual Machines. For sizing presets already in the product, also see [Profiles](../core/profiles.md) / [Templates](../operations/templates.md).

**Empty / fail:** Error banner, empty table, or failed toast — confirm you are signed in, `zyvor-fabricd` is healthy (`/readyz` on `http://127.0.0.1:<port>` or `https://<host>`), and any backend this page needs (FluxVM, storage, network) is reachable. See [Admin basics](../../admin-basics.md).

**Success:** Live data loads without error; creates/updates appear in the list or detail view and any confirmation toast clears cleanly.

## Related pages

- [Batch Import](batch-import.md)
- [Create VM](../core/create.md)
- [API Playground](../tools/playground.md)
- [Templates](../operations/templates.md)
- [Profiles](../core/profiles.md)
- [Job Monitor](job-monitor.md)
- [Pipeline](pipeline.md)
- [Getting Started](../../getting-started.md)
- [Page index](../../PAGE_INDEX.md)
