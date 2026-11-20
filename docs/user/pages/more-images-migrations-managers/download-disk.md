# Download Disk

## Purpose

Download Disk — browse the disk images available on the Fabric host and download any of them straight to your machine.

Read inventory + download. Uploading is the inverse flow on [Upload Disk](upload-disk.md).

## When to use it

- To pull a host-side disk image onto your workstation for offline inspection or archive
- To retrieve a converted or migrated output path after a job finishes
- Prefer this page when the job matches the purpose above
- After [Disk Converter](disk-converter.md) or [Pipeline](pipeline.md) completes, to fetch the output file

## How to get there

- Route / id: `/download-disk`
- Nav: **More — images, migrations & managers → Download Disk** (sidebar, command palette, or desktop nav)

## Operate from the console (UX)

1. Open the page and wait for the host disk-image list to load (or use header **Refresh**).
2. Browse or search the listed images — name, format, size, and path.
3. Choose an image and start the download to your browser's download location.
4. Confirm the file size matches expectations before transferring elsewhere.
5. For inventory without download, [Disk Images](disk-images.md) is a read-only table of the same class of artifacts.

Typical flow: finish convert/migrate → open Download Disk → fetch the output path → optionally delete local copies when done. Paths are host-local under directories such as `/var/lib/…` — not remote lab URLs.

Operator tip: large downloads can take time; keep the tab open until the browser finishes.

**Empty / fail:** Error banner, empty table, or failed toast — confirm you are signed in, `zyvor-fabricd` is healthy (`/readyz` on `http://127.0.0.1:<port>` or `https://<host>`), and any backend this page needs (FluxVM, storage, network) is reachable. See [Admin basics](../../admin-basics.md).

**Success:** Live data loads without error; creates/updates appear in the list or detail view and any confirmation toast clears cleanly.

## Related pages

- [Upload Disk](upload-disk.md)
- [Disk Images](disk-images.md)
- [Disk Converter](disk-converter.md)
- [Migration History](migration-history.md)
- [Job Monitor](job-monitor.md)
- [Pipeline](pipeline.md)
- [Getting Started](../../getting-started.md)
- [Page index](../../PAGE_INDEX.md)
