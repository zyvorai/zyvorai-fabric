# Image Builder

## Purpose

Image Builder — build custom VM disk images from scratch using [mkosi](https://github.com/systemd/mkosi), by picking a Linux distribution and a package list, and track builds from queued through to a finished image.

Builds run as jobs; watch status through to a finished disk image usable for create/import.

## When to use it

- To build a custom guest image with a chosen distro and package set
- To track build progress from queued to finished without shelling into mkosi manually
- Prefer this page when the job matches the purpose above
- When stock cloud images are not enough and you need a repeatable package baseline

## How to get there

- Route / id: `/image-builder`
- Nav: **More — images, migrations & managers → Image Builder** (sidebar, command palette, or desktop nav)

## Operate from the console (UX)

1. Choose a Linux distribution and the package list for the image.
2. Submit the build and note the job enters a queued/running state.
3. Track progress until the build completes or fails; capture the output image location on success.
4. Use header refresh / job views if the status looks stale.
5. After success, confirm the artifact on [Disk Images](disk-images.md) and create a VM from it.

Typical flow: pick distro + packages → build → watch [Job Monitor](job-monitor.md) → register/use the image path. For one-off format conversion of an existing disk, use Disk Converter instead.

Operator tip: long builds belong in Job Monitor; do not assume a silent page means success.

**Empty / fail:** Error banner, empty table, or failed toast — confirm you are signed in, `zyvor-fabricd` is healthy (`/readyz` on `http://127.0.0.1:<port>` or `https://<host>`), and any backend this page needs (FluxVM, storage, network) is reachable. See [Admin basics](../../admin-basics.md).

**Success:** Live data loads without error; creates/updates appear in the list or detail view and any confirmation toast clears cleanly.

## Related pages

- [Disk Images](disk-images.md)
- [Create VM](../core/create.md)
- [Content Library](../operations/content-library.md)
- [Upload Disk](upload-disk.md)
- [Job Monitor](job-monitor.md)
- [Pipeline](pipeline.md)
- [Getting Started](../../getting-started.md)
- [Page index](../../PAGE_INDEX.md)
