# Job Monitor

## Purpose

Job Monitor — a live view of background jobs (disk conversions, migrations, and other pipeline work), with per-job progress, pipeline stage, and streaming logs.

Use this as the operator console for long-running image/migration work started elsewhere.

## When to use it

- To watch conversion/migration jobs with progress, stage, and logs
- To diagnose a failed job from its streaming log without SSHing in
- Prefer this page when the job matches the purpose above
- After starting Disk Converter, Migration Wizard, Image Builder, or related pipelines

## How to get there

- Route / id: `/job-monitor`
- Nav: **More — images, migrations & managers → Job Monitor** (sidebar, command palette, or desktop nav)

## Operate from the console (UX)

1. Open Job Monitor to load active/recent background jobs.
2. Select a job to inspect percent complete, pipeline stage, and status.
3. Follow streaming logs for the selected job while it runs.
4. Refresh if the list looks stale after submitting work from another page.
5. On failure, capture the log excerpt, then retry from the originating tool (converter/wizard/builder).

Typical flow: start a conversion/migration → open Job Monitor → watch stage + logs → on success confirm [Disk Images](disk-images.md) / [Migration History](migration-history.md). [Pipeline](pipeline.md) is a stage-centric companion view.

Operator tip: keep this tab open during long conversions; stage stalls often show up in the streaming log first.

**Empty / fail:** Error banner, empty table, or failed toast — confirm you are signed in, `zyvor-fabricd` is healthy (`/readyz` on `http://127.0.0.1:<port>` or `https://<host>`), and any backend this page needs (FluxVM, storage, network) is reachable. See [Admin basics](../../admin-basics.md).

**Success:** Live data loads without error; creates/updates appear in the list or detail view and any confirmation toast clears cleanly.

## Related pages

- [Pipeline](pipeline.md)
- [Disk Converter](disk-converter.md)
- [Migration Wizard](../operations/migration-wizard.md)
- [Migration History](migration-history.md)
- [Image Builder](image-builder.md)
- [Job Monitor](job-monitor.md)
- [Getting Started](../../getting-started.md)
- [Page index](../../PAGE_INDEX.md)
