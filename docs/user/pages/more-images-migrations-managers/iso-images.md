# ISO Images

## Purpose

ISO Images — a read-only inventory of installer and driver ISO files sitting in the host's configured images directory, showing which VMs currently have each one attached.

Read-only. Attachment changes happen from VM create/detail flows, not here.

## When to use it

- To see which installer/driver ISOs are available on the host
- To find which VMs currently have a given ISO attached
- Prefer this page when the job matches the purpose above
- Before a guest install, to confirm the ISO path exists

## How to get there

- Route / id: `/iso-images`
- Nav: **More — images, migrations & managers → ISO Images** (sidebar, command palette, or desktop nav)

## Operate from the console (UX)

1. Open the page and load the ISO inventory from the configured images directory.
2. Review each ISO's identity and which VMs currently have it attached.
3. Use search/filter controls if present to narrow by name.
4. **Refresh** after copying a new ISO onto the host outside the UI.
5. Proceed to [Create VM](../core/create.md) / VM detail to attach media — this page does not attach or detach.

Typical flow: confirm ISO present → create/boot VM with that media → return here to verify attachment listing. Disk files (not ISOs) are on [Disk Images](disk-images.md).

Operator tip: after copying an ISO onto the host, Refresh before expecting it in Create VM media pickers.

**Empty / fail:** Error banner, empty table, or failed toast — confirm you are signed in, `zyvor-fabricd` is healthy (`/readyz` on `http://127.0.0.1:<port>` or `https://<host>`), and any backend this page needs (FluxVM, storage, network) is reachable. See [Admin basics](../../admin-basics.md).

**Success:** Live data loads without error; creates/updates appear in the list or detail view and any confirmation toast clears cleanly.

## Related pages

- [Disk Images](disk-images.md)
- [Content Library](../operations/content-library.md)
- [Create VM](../core/create.md)
- [Upload Disk](upload-disk.md)
- [Job Monitor](job-monitor.md)
- [Pipeline](pipeline.md)
- [Getting Started](../../getting-started.md)
- [Page index](../../PAGE_INDEX.md)
