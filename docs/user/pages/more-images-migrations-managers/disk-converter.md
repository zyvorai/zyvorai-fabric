# Disk Converter

## Purpose

Disk Format Converter — convert a single disk image between QCOW2, VMDK, VHD, VHDX, and RAW, tracking the conversion job's progress to completion.

Converts one disk; does not create a VM. For import+VM create, use [Migration Wizard](../operations/migration-wizard.md).

## When to use it

- To convert an imported disk (e.g. a VMDK) into QCOW2 before using it with a VM
- To produce a RAW or VHD/VHDX copy for a tool or target platform that needs a specific format
- To watch a long-running conversion through to completion without leaving the page
- Prefer this page when the job matches the purpose above
- After [Upload Disk](upload-disk.md) when the uploaded format is wrong for FluxVM

## How to get there

- Route / id: `/disk-converter`
- Nav: **More — images, migrations & managers → Disk Converter** (sidebar, command palette, or desktop nav)

## Operate from the console (UX)

1. **Pick a source disk** — dropdown of known images or type a path (e.g. `/path/to/disk.vmdk`). Use **Load available disk images** if the list is empty.
2. **Choose the target format** — QCOW2, VMDK, VHD, VHDX, or RAW.
3. **Output path** — auto-derived from source + format; edit if needed.
4. **Convert** — submits and polls every 2 seconds; progress through pending → running → completed/failed.
5. Failure shows backend error inline; success shows output path. **Reset** clears form/job state.

Typical flow: load images → convert to QCOW2 → watch Job Monitor/Pipeline → download or attach the output. Batch JSON specs without running live on [Batch Migration](batch-migration.md).

**Empty / fail:** Error banner, empty table, or failed toast — confirm you are signed in, `zyvor-fabricd` is healthy (`/readyz` on `http://127.0.0.1:<port>` or `https://<host>`), and any backend this page needs (FluxVM, storage, network) is reachable. See [Admin basics](../../admin-basics.md).

**Success:** Live data loads without error; creates/updates appear in the list or detail view and any confirmation toast clears cleanly.

## Related pages

- [Disk Images](disk-images.md)
- [Upload Disk](upload-disk.md)
- [Migration Wizard](../operations/migration-wizard.md)
- [Batch Migration](batch-migration.md)
- [Job Monitor](job-monitor.md)
- [Pipeline](pipeline.md)
- [Getting Started](../../getting-started.md)
- [Page index](../../PAGE_INDEX.md)
