# History

## Purpose

Migration History — a read-only log of completed and failed migration jobs, with status, timing, and where the output landed.

No filters/search/actions — a plain historical log. Empty → "No migration history yet."

## When to use it

- To check whether a past migration succeeded or failed
- To read the error message left behind by a failed migration
- To see how long a migration took, or where its output disk was written
- Prefer this page when the job matches the purpose above
- After Pipeline clears, to confirm final status and output path

## How to get there

- Route / id: `/migration-history`
- Nav: **More — images, migrations & managers → History** (sidebar, command palette, or desktop nav)

## Operate from the console (UX)

1. History loads from the migrations API on open; header **Refresh** reloads.
2. Columns: **Name**, **VM**, **Status** (completed / failed / running, with inline error under failed), **Started**, **Duration**, **Output** path.
3. No per-row actions — use output path with Download Disk / Disk Images as needed.
4. Correlate failures with [Migration Report](migration-report.md) totals and Job Monitor logs.
5. For pre-checks before the next run, open [Migration Readiness](migration-readiness.md).

Typical flow: refresh after a batch → note failed rows + errors → fix source/path → re-run wizard → confirm completed + output path.

**Empty / fail:** Error banner, empty table, or failed toast — confirm you are signed in, `zyvor-fabricd` is healthy (`/readyz` on `http://127.0.0.1:<port>` or `https://<host>`), and any backend this page needs (FluxVM, storage, network) is reachable. See [Admin basics](../../admin-basics.md).

**Success:** Live data loads without error; creates/updates appear in the list or detail view and any confirmation toast clears cleanly.

## Related pages

- [Migration Report](migration-report.md)
- [Migration Readiness](migration-readiness.md)
- [Migration Wizard](../operations/migration-wizard.md)
- [Download Disk](download-disk.md)
- [Job Monitor](job-monitor.md)
- [Pipeline](pipeline.md)
- [Getting Started](../../getting-started.md)
- [Page index](../../PAGE_INDEX.md)
