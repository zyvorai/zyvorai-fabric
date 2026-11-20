# Lifecycle

## Purpose

Lifecycle Manager — define patch/upgrade baselines, scan hosts for compliance against them, remediate non-compliant hosts, and track rolling updates across a host fleet.

This is **host** patch/upgrade lifecycle, not VM create/start/stop. VM lifecycle actions stay on Virtual Machines, Schedules, and Bulk Operations.

## When to use it

- To define a baseline (patch, upgrade, or extension) with a severity level, and see which hosts already meet it
- To scan hosts for compliance against a baseline and find out which are missing patches
- To watch a remediation task apply patches to a host, or a rolling update roll out across a fleet host by host
- Before a maintenance window, to know which hosts fail Critical/Important baselines
- After remediation, to confirm compliance scan results and rolling-update progress

## How to get there

- Route / id: `/app/lifecycle`
- Nav: **Operations → Lifecycle** (sidebar, command palette, or desktop nav)

## Operate from the console (UX)

Summary tiles: total baselines, non-compliant hosts, active remediation tasks, and running rolling updates. Four tabs:

1. **Baselines** — type, severity, release date, host count, compliant count, compliance bar. **Create Baseline** sets name, optional description, type (Patch/Upgrade/Extension), severity (Critical/Important/Moderate/Low). Play icon **runs a compliance scan**; trash **deletes** (confirmation).
2. **Compliance Scans** — per host: baseline, status (compliant / non-compliant / incompatible / etc.), missing patch count, last scanned time.
3. **Remediation** — tasks per host: status, progress bar, patches applied vs total, error message if any.
4. **Rolling Updates** — cards with status, hosts completed vs total, parallelism, current host, progress bar, timestamps, failed-host count.

Typical flow: create Critical patch baseline → run scan → review Compliance Scans → follow Remediation / Rolling Updates until non-compliant count drops. Pair with [Compliance](../security/compliance.md) for config scorecards vs patch baselines.

**Empty / fail:** Error banner, empty table, or failed toast — confirm you are signed in, `zyvor-fabricd` is healthy (`/readyz` on `http://127.0.0.1:<port>` or `https://<host>`), and any backend this page needs (FluxVM, storage, network) is reachable. See [Admin basics](../../admin-basics.md).

**Success:** Live data loads without error; creates/updates appear in the list or detail view and any confirmation toast clears cleanly.

## Related pages

- [Compliance](../security/compliance.md)
- [Content Library](content-library.md)
- [System Health](../infrastructure/system-health.md)
- [System](../infrastructure/system.md)
- [Schedules](schedules.md)
- [Getting Started](../../getting-started.md)
- [Page index](../../PAGE_INDEX.md)
