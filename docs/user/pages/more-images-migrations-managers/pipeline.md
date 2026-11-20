# Pipeline

## Purpose

Pipeline Monitor — a live, auto-refreshing view of in-progress migration/conversion jobs, showing each job's percent complete and which of five stages it's currently in.

Stages: **inspect → prepare → convert → validate → deploy**. Read-only — no start/cancel/retry here.

## When to use it

- To watch a migration or conversion in progress and see exactly which stage it's at
- To catch a failure as it happens instead of waiting for the history page to update
- To find a job's duration or output path as soon as it's available
- Prefer this page when the job matches the purpose above
- Alongside Job Monitor when you want a stage tracker more than raw logs

## How to get there

- Route / id: `/pipeline`
- Nav: **More — images, migrations & managers → Pipeline** (sidebar, command palette, or desktop nav)

## Operate from the console (UX)

1. Loads active jobs on open; polls every 3 seconds; header **Refresh** forces reload.
2. Each card: VM name, job ID, source, status badge (pending / running / completed / failed).
3. Progress bar shows percent complete.
4. Stage tracker: completed green, current pulses blue (red if failed), remaining grey.
5. Duration and output path appear when available; failures show a red error panel.
6. Read-only — start/cancel/retry from the tool that created the job.

Typical flow: submit Migration Wizard / converter → open Pipeline → watch stages through deploy → on failure read the error panel and Job Monitor logs → check [Migration History](migration-history.md).

**Empty / fail:** Error banner, empty table, or failed toast — confirm you are signed in, `zyvor-fabricd` is healthy (`/readyz` on `http://127.0.0.1:<port>` or `https://<host>`), and any backend this page needs (FluxVM, storage, network) is reachable. See [Admin basics](../../admin-basics.md).

**Success:** Live data loads without error; creates/updates appear in the list or detail view and any confirmation toast clears cleanly.

## Related pages

- [Job Monitor](job-monitor.md)
- [Migration Wizard](../operations/migration-wizard.md)
- [Disk Converter](disk-converter.md)
- [Migration History](migration-history.md)
- [Migration Report](migration-report.md)
- [Pipeline](pipeline.md)
- [Getting Started](../../getting-started.md)
- [Page index](../../PAGE_INDEX.md)
