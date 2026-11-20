# Report

## Purpose

Migration Report — a shareable summary of all migration jobs (totals by status, average duration) plus the full per-migration detail table, with copy and print actions.

Use for stakeholder updates and post-mortems; History is the raw log, Report is the rollup.

## When to use it

- To summarize migration outcomes (counts by status, average duration)
- To copy or print a shareable report after a migration wave
- Prefer this page when the job matches the purpose above
- After History shows the wave is done, to package results for others

## How to get there

- Route / id: `/migration-report`
- Nav: **More — images, migrations & managers → Report** (sidebar, command palette, or desktop nav)

## Operate from the console (UX)

1. Open Report to load totals by status and average duration.
2. Review the per-migration detail table alongside the summary.
3. Use **Copy** / **Print** (or equivalent share actions on the page) for handoff.
4. Refresh after new jobs complete so totals stay current.
5. Drill into failures via [Migration History](migration-history.md) and Job Monitor logs.

Typical flow: finish wave → Report → copy/print → attach to change ticket → fix failures and re-run readiness/wizard as needed.

Operator tip: print/copy only after History shows the wave is idle so totals are not mid-flight.

**Empty / fail:** Error banner, empty table, or failed toast — confirm you are signed in, `zyvor-fabricd` is healthy (`/readyz` on `http://127.0.0.1:<port>` or `https://<host>`), and any backend this page needs (FluxVM, storage, network) is reachable. See [Admin basics](../../admin-basics.md).

**Success:** Live data loads without error; creates/updates appear in the list or detail view and any confirmation toast clears cleanly.

## Related pages

- [Migration History](migration-history.md)
- [Migration Readiness](migration-readiness.md)
- [Pipeline](pipeline.md)
- [Migration Wizard](../operations/migration-wizard.md)
- [Job Monitor](job-monitor.md)
- [Getting Started](../../getting-started.md)
- [Page index](../../PAGE_INDEX.md)
