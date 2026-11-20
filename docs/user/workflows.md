# Common workflows

Short operator paths for the jobs you run most often. Open the console at `https://<host>:9095/app` after signing in (see [Getting Started](getting-started.md)). Never publish lab IPs — use `<host>` or `127.0.0.1`.

## Workflow index

| Workflow | Route | Guide |
|----------|-------|-------|
| Create a VM | `/app/create` | [Create VM](pages/core/create.md) |
| Manage the fleet | `/app/vms` | [Virtual Machines](pages/core/vms.md) |
| Per-VM Network Fabric (schema v4) | `/app/vms/:name` → **Dataplane** | [VM Dataplane](pages/infrastructure/dataplane.md) |
| Edge Maglev / Service Fabric v6 | `/app/edge-dataplane` → **Services** | [Edge Dataplane](pages/infrastructure/edge-dataplane.md) |
| Container Groups (Secure Containers) | `/app/container-groups` | [Container Groups](pages/operations/container-groups.md) |
| Agents / Sessions | `/app/agents`, `/app/sessions` | [Agents](pages/operations/agents.md) · [Sessions](pages/operations/sessions.md) |
| Snapshots | `/app/snapshots` or VM → **Snapshots** | [Snapshots](pages/operations/snapshots.md) |
| Backups & restore | `/app/backups` | [Backups](pages/operations/backups.md) |

## Create a VM

1. Open **Core → Create VM** (`/app/create`).
2. **Basics** — name + catalog image (or host path).
3. **Resources** — vCPUs, memory, disk. Networking:
   - **NAT** (default) — private outbound; add **Expose ports** / **Expose SSH (22)** for inbound from clients that can reach the host.
   - **Bridged** — own LAN address (DHCP or static via cloud-init). Use bridged when you will attach VM Dataplane eBPF.
4. **Review** → create → land on `/app/vms/:name`.
5. **Empty / fail:** Image missing or FluxVM unreachable — check Dashboard capability chips and [Admin basics](admin-basics.md).
6. **Success:** VM appears on `/app/vms` with the expected state badge.

## Attach VM Dataplane (Network Fabric schema v4)

1. Create or pick a **bridged** VM with a host TAP (`network_tap`).
2. Open `/app/vms/:name` → **Dataplane** (or Network → **Open Dataplane**).
3. On **Status**, confirm **Attached = yes**, **Mode = ebpf**, **Schema version = 4**.
4. On **Policy**, pick a preset or edit allow/deny CIDRs and `tcp/PORT` / `udp/PORT` ports → **Save policy**.
5. Use **Effective**, **Stats**, and **Flows** to verify merges and allow/drop counters.
6. **Empty / fail:** Schema missing or not attached — enable FluxVM `[sandbox.dataplane] mode = "ebpf"` and confirm the TAP exists (operator guide linked from the dataplane page).
7. **Success:** Status shows schema 4 attached; counters move when traffic matches policy.

## Create an Edge Maglev service (Service Fabric v6)

1. Open **Infrastructure → Edge Dataplane** (`/app/edge-dataplane`).
2. Open the **Services** tab (Service Fabric **v6** / BPF schema **4**).
3. Upsert a Maglev VIP service (backends, affinity, optional drain/health, pacing / host-routing fields the form exposes).
4. Check **Health** for BPF/bpffs presence; use ads / conntrack / HA controls as needed.
5. **Empty / fail:** Health not ok — FluxVM service dataplane not enabled or bpffs missing; see [Edge Dataplane](pages/infrastructure/edge-dataplane.md) and [ebpf-service-fabric.md](../ebpf-service-fabric.md).
6. **Success:** Service row appears; schema badge shows Service Fabric v6; backend health reconciles.

## Take and revert a snapshot

1. Prefer **Virtual Machines →** open the VM → **Snapshots**, or **Operations → Snapshots** (`/app/snapshots`) and enter the VM name.
2. **Create** — name, optional description, type **Disk** (default, fast) or **Full** (disk + memory, slower).
3. To roll back: **stop the VM**, then **Revert** on the snapshot row (confirmation required).
4. **Empty / fail:** Create **409** right after start — QMP not ready yet; wait and retry. Revert while running is rejected.
5. **Success:** Snapshot listed with type and timestamp; after revert, VM boots from that disk state.

Full notes: [Snapshots](pages/operations/snapshots.md).

## Backup and restore

1. Open **Operations → Backups** (`/app/backups`).
2. **Create Backup** — pick VM, **Full** or **Incremental** (compressed, 30-day retention by default).
3. Watch **Active Jobs** until status is `completed`.
4. **Restore** — in place on the original VM, or **Restore to new VM** with a new name (config + disks; not live runtime state).
5. **Empty / fail:** Job stuck/failed — check Active Jobs error text, storage pool space, and FluxVM health.
6. **Success:** Backup row shows `completed` with size/expiry; restore produces a runnable VM on `/app/vms`.

Guide: [Backups](pages/operations/backups.md). Recurring jobs: [Backup Scheduler](pages/more-images-migrations-managers/backup-scheduler.md).

## Related

- [Getting Started](getting-started.md)
- [Using the Dashboard](using-the-dashboard.md)
- [Admin basics](admin-basics.md)
- [Page-by-page guides](pages/README.md)
- [Complete page index](PAGE_INDEX.md)
