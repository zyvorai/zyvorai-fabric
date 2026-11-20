# Upload Disk

## Purpose

Upload Disk Image — drag-and-drop (or browse) upload of a VM disk image file to the server, with live progress and an in-session upload history.

Accepted extensions: `.qcow2`, `.vmdk`, `.vhd`, `.vhdx`, `.raw`, `.img`, `.ova`. Upload history is **session-only** (clears on reload).

## When to use it

- To get a disk image onto the host so it can be used to create a VM
- To upload a disk exported or converted elsewhere for import
- To check what you've uploaded recently in this session
- Prefer this page when the job matches the purpose above
- Before [Migration Wizard](../operations/migration-wizard.md) or Create VM when the image is still on your laptop

## How to get there

- Route / id: `/upload-disk`
- Nav: **More — images, migrations & managers → Upload Disk** (sidebar, command palette, or desktop nav)

## Operate from the console (UX)

1. Drop zone accepts drag-and-drop or click-to-browse. Other extensions are rejected with an inline error.
2. Set **Destination Directory** (defaults to `/var/lib/libvirt/images`) and click **Upload**.
3. Live progress — percent, bytes transferred/total, speed — plus **Cancel**.
4. Success banner shows saved path, format, size; file appears in **Upload History** (name, format, size, time) for this session only.
5. Failure (format, network, cancel) shows an error banner.

Typical flow: upload → note path → convert with [Disk Converter](disk-converter.md) if needed → create/import VM. To pull images off the host, use [Download Disk](download-disk.md).

**Empty / fail:** Error banner, empty table, or failed toast — confirm you are signed in, `zyvor-fabricd` is healthy (`/readyz` on `http://127.0.0.1:<port>` or `https://<host>`), and any backend this page needs (FluxVM, storage, network) is reachable. See [Admin basics](../../admin-basics.md).

**Success:** Live data loads without error; creates/updates appear in the list or detail view and any confirmation toast clears cleanly.

## Related pages

- [Download Disk](download-disk.md)
- [Disk Images](disk-images.md)
- [Disk Converter](disk-converter.md)
- [Migration Wizard](../operations/migration-wizard.md)
- [Job Monitor](job-monitor.md)
- [Pipeline](pipeline.md)
- [Getting Started](../../getting-started.md)
- [Page index](../../PAGE_INDEX.md)
