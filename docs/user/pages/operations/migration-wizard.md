# Migration Wizard

## Purpose

Migration Wizard — a three-step wizard (Source → Configure → Review) for converting an existing disk image (local file or remote host) into a new Zyvor Fabric VM.

This imports/converts a **disk image into a new VM**. Live host-to-host moves of an existing Fabric VM live on [Migrations](migrations.md). Spec-only batch builders live under More → Batch Migration.

## When to use it

- To bring in a VM from elsewhere — a disk image on the local filesystem, or one reachable over SSH on a remote host
- To convert a disk image to a different format (QCOW2, RAW, or VMDK) as part of importing it
- To size the resulting VM's vCPUs and memory and decide whether it should auto-start once migration finishes
- When you already have a path like `/path/to/disk.vmdk` or `user@hostname:/path` and want a guided import
- After [Migration Readiness](../more-images-migrations-managers/migration-readiness.md) checks pass for larger fleets

## How to get there

- Route / id: `/migration-wizard`
- Nav: **Operations → Migration Wizard** (sidebar, command palette, or desktop nav)

## Operate from the console (UX)

**1. Source** — choose **Local File** (disk image path, e.g. `/path/to/disk.vmdk`) or **Remote Host** (`user@hostname:/path` over SSH).

**2. Configure** — new VM name, target format (QCOW2, RAW, or VMDK), vCPUs (1–64), memory (MB, 256 MB steps), output directory (defaults to `/var/lib/zyvor-fabricd/images`), and whether to auto-start when done.

**3. Review & Submit** — summary of choices, then **Submit Migration**. Success or failure shows inline (with hints on failure); you can't resubmit once it succeeds. Use **Back** before submit to revisit earlier steps.

After submit, watch progress on [Pipeline](../more-images-migrations-managers/pipeline.md) / [Job Monitor](../more-images-migrations-managers/job-monitor.md), then confirm the VM on [Virtual Machines](../core/vms.md). For format-only conversion without creating a VM, use [Disk Converter](../more-images-migrations-managers/disk-converter.md).

**Empty / fail:** Error banner, empty table, or failed toast — confirm you are signed in, `zyvor-fabricd` is healthy (`/readyz` on `http://127.0.0.1:<port>` or `https://<host>`), and any backend this page needs (FluxVM, storage, network) is reachable. See [Admin basics](../../admin-basics.md).

**Success:** Live data loads without error; creates/updates appear in the list or detail view and any confirmation toast clears cleanly.

## Related pages

- [Migrations](migrations.md)
- [Disk Converter](../more-images-migrations-managers/disk-converter.md)
- [Pipeline](../more-images-migrations-managers/pipeline.md)
- [Job Monitor](../more-images-migrations-managers/job-monitor.md)
- [Migration History](../more-images-migrations-managers/migration-history.md)
- [Backups](backups.md)
- [Getting Started](../../getting-started.md)
- [Page index](../../PAGE_INDEX.md)
