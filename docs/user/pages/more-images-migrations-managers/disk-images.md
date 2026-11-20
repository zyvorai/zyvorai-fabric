# Disk Images

## Purpose

Disk Images — a read-only inventory of the VM disk images present on the host: name, format, size, and path for each one.

Selection only feeds the Selected counter — there is no bulk action on this page.

## When to use it

- To inventory what disk files exist before create/import/convert
- To search by name, format, or path and see total size across images
- Prefer this page when the job matches the purpose above
- When deciding whether to convert (VMDK→QCOW2) or upload a missing image

## How to get there

- Route / id: `/disk-images`
- Nav: **More — images, migrations & managers → Disk Images** (sidebar, command palette, or desktop nav)

## Operate from the console (UX)

1. Review summary tiles: **Total Images**, **Total Size**, distinct **Formats**, and **Selected** count.
2. **Search** by name, format, or path.
3. Click a row or checkbox to select/deselect — local only; no bulk action attached.
4. Each row: name, color-coded format badge (qcow2, vmdk, vhd/vhdx, raw, img), size, full path.
5. **Refresh** reloads from the host.

Typical flow: search for a format → note path → open Disk Converter or Migration Wizard with that path → refresh after jobs complete. ISOs live on [ISO Images](iso-images.md).

Operator tip: Selected count is informational only — convert or download from their dedicated pages.

**Empty / fail:** Error banner, empty table, or failed toast — confirm you are signed in, `zyvor-fabricd` is healthy (`/readyz` on `http://127.0.0.1:<port>` or `https://<host>`), and any backend this page needs (FluxVM, storage, network) is reachable. See [Admin basics](../../admin-basics.md).

**Success:** Live data loads without error; creates/updates appear in the list or detail view and any confirmation toast clears cleanly.

## Related pages

- [ISO Images](iso-images.md)
- [Upload Disk](upload-disk.md)
- [Disk Converter](disk-converter.md)
- [Storage Mgr](storage-manager.md)
- [Job Monitor](job-monitor.md)
- [Pipeline](pipeline.md)
- [Getting Started](../../getting-started.md)
- [Page index](../../PAGE_INDEX.md)
