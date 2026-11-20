# Readiness

## Purpose

Migration Readiness — pre-flight checks that verify the environment is in a good state before you start a migration, with a pass/fail summary and per-check detail.

Run this before large Migration Wizard / batch work so failures are environmental, not mid-job surprises.

## When to use it

- Before starting migrations, to verify host/environment readiness
- To read per-check pass/fail detail when the summary is not all green
- Prefer this page when the job matches the purpose above
- After fixing storage/network/FluxVM issues, to re-validate before retry

## How to get there

- Route / id: `/migration-readiness`
- Nav: **More — images, migrations & managers → Readiness** (sidebar, command palette, or desktop nav)

## Operate from the console (UX)

1. Open Readiness and run/load the pre-flight check set.
2. Read the pass/fail summary first.
3. Expand or scan per-check detail for anything failing.
4. Remediate owning systems (storage space, FluxVM health, network reachability to remote sources).
5. Re-run checks until the summary is clean, then proceed to Migration Wizard / Batch Migration.

Typical flow: Readiness → fix fails → Readiness again → Migration Wizard → Pipeline/History. Dashboard capability chips should already be Live before you trust a green readiness result.

**Empty / fail:** Error banner, empty table, or failed toast — confirm you are signed in, `zyvor-fabricd` is healthy (`/readyz` on `http://127.0.0.1:<port>` or `https://<host>`), and any backend this page needs (FluxVM, storage, network) is reachable. See [Admin basics](../../admin-basics.md).

**Success:** Live data loads without error; creates/updates appear in the list or detail view and any confirmation toast clears cleanly.

## Related pages

- [Migration Wizard](../operations/migration-wizard.md)
- [Batch Migration](batch-migration.md)
- [Dashboard](../core/home.md)
- [Migration History](migration-history.md)
- [Storage](../infrastructure/storage.md)
- [Job Monitor](job-monitor.md)
- [Pipeline](pipeline.md)
- [Getting Started](../../getting-started.md)
- [Page index](../../PAGE_INDEX.md)
