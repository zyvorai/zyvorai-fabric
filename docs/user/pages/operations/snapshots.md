# Snapshots

## Purpose

VM Snapshots — create point-in-time snapshots of a specific VM's disk (or disk + memory), and revert or delete them. Unlike most Operations pages, this one is scoped to one VM at a time, entered by name.

You can also manage snapshots from the **VM detail → Snapshots** tab (`/app/vms/:name`), which is the preferred path when you are already looking at a VM.

## When to use it

- To capture a VM's disk state before a risky change, then roll back if needed
- To browse the snapshot history for a specific VM
- To revert a VM to an earlier snapshot
- To clean up old snapshots you no longer need

## Snapshot types

| Type | What it captures | Running VM | Typical duration |
|------|------------------|------------|------------------|
| **Disk** (default) | qcow2 internal disk snapshot only | Live via QMP `blockdev-snapshot-internal-sync` | Usually under a second once the monitor is up |
| **Full** | Disk + guest memory (vmstate) | Live via QMP `snapshot-save` | Can take minutes under host disk load; allow up to ~5 minutes |

Stopped VMs use `qemu-img snapshot` for both types (no live monitor required).

**Tips**

- Prefer **Disk** for routine checkpoints. Use **Full** only when you need to restore running process state.
- If create returns **409** (“could not reach the VM monitor yet”), wait a few seconds after start/restart and retry — the QMP socket is not ready until QEMU has fully come up. The console retries automatically a few times.
- Revert still requires the VM to be **stopped**.

## Troubleshooting

| Symptom | Likely cause | What to do |
|---------|--------------|------------|
| Snapshot create **409** | QMP monitor not ready yet after start/restart | Wait a few seconds and retry (UI auto-retries). Confirm FluxVM shows the VM `running` and `qmp.sock` exists under `/var/lib/fluxvm/instances/`. |
| **Full** snapshot hangs / times out | Memory dump under host disk load | Prefer **Disk**; allow up to ~5 minutes for Full; reduce host load (other large guests). |
| Auto-healer restarting right after start | Guest/QEMU flapping during boot | Fabric skips healing for 90s after start; if it continues, check FluxVM console logs and the guest image. |
| Start stuck / Failed with FluxVM timeout | Disk clone exceeded the old 30s client timeout | Fixed at 180s — redeploy fabricd if the host is still on an older build. |

## How to get there

- Route / id: `/app/snapshots`
- Nav: **Operations → Snapshots** (sidebar, command palette, or desktop nav)
- Or: **Virtual Machines →** open a VM → **Snapshots** tab

## Operate from the console (UX)

### Global Snapshots page (`/app/snapshots`)

1. Type a VM name into the **VM Name** field and click **Load Snapshots** (or wait — snapshots load automatically once a name is entered) to see that VM's snapshot list. Nothing loads until a VM name is provided.
2. **Create** — opens a dialog for a snapshot name, an optional description, and a type: **Disk Only** (default) or **Full (disk + memory — slower)**.
3. The snapshot table lists each snapshot's name, type, description, and relative creation time.
4. Per-snapshot actions: **Revert to snapshot** (confirmation dialog warns the VM must be stopped first) and **Delete snapshot** (confirmation dialog).
5. If the VM has no snapshots yet, the table shows "No snapshots found for this VM."

### VM detail Snapshots tab

1. Open `/app/vms/:name` → **Snapshots**.
2. **Create Snapshot** — name, optional description, type (**Disk** by default).
3. **Create** runs against the live API; success refreshes the list. Delete and revert work the same as on the global page.

If the page stays empty, check service health, auth configuration, and that dependencies for this domain are installed.

## Related pages

- [Getting Started](../../getting-started.md)
- [Snapshots & backups tutorial](../../../tutorials/03-snapshots-backups.md)
- [Page index](../../PAGE_INDEX.md)
