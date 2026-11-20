# Using the Dashboard

Zyvor Fabric’s authenticated console lives under `/app` after you sign in at `/sign-in`. Public marketing stays on `/`, `/product`, `/platform`, `/security`. Use the left nav groups, top-bar quick links (Settings, API Playground), or `Ctrl/Cmd+K` command palette.

Open the UI at `https://<host>:9095` (never hardcode lab IPs).

## Browse vs act

Inventory and status views are safe to explore. Mutating actions (create, migrate, delete, remediate) follow role gates and confirmation dialogs — review impact first.

## Nav map by category

### Core

| Page | Route | When to use |
|------|-------|-------------|
| Dashboard | `/app` | Landing view: VM counts, live charts, subsystem health |
| Favorites | `/app/favorites` | Starred shortlist of VMs you touch often |
| Virtual Machines | `/app/vms` | Fleet list + per-VM detail / actions |
| Warm Pools | `/app/vm-pools` | Pre-booted paused VMs you can claim instantly |
| Profiles | `/app/profiles` | Instance-type sizing presets for create |
| Datacenters | `/app/datacenters` | Physical inventory: DCs → clusters → hosts |
| VM Browser | `/app/vm-browser` | Lightweight read-only VM grid |
| Create VM | `/app/create` | Three-step provision wizard |
| Settings | `/app/settings` | Console / product preferences (also top-bar) |

`/app/machines` is **removed** — use `/app/vms`. `/app/vm-wizard` redirects to `/app/create`.

### Infrastructure

| Page | Route | When to use |
|------|-------|-------------|
| Network | `/app/network` | Day-2 NAT/bridge mode and port forwards |
| Net Security | `/app/network-security` | Host SDN: policies, firewall, VPN, QoS, NAT… |
| Edge Dataplane | `/app/edge-dataplane` | Groups/CNP + Maglev Services (Service Fabric v6) |
| Storage | `/app/storage` | Pool capacity + volume ledger |
| Storage Pools | `/app/storage-pools` | Create/start/stop NFS/LVM/ZFS/Ceph pools |
| Distributed Storage | `/app/distributed-storage` | Multi-host replicated / policy storage |
| Resource Pools | `/app/resource-pools` | Hierarchical CPU/memory admission |
| System | `/app/system` | Host hardware topology + VM tips |
| System Health | `/app/system-health` | Live host utilization score |
| Containers | `/app/containers` | Read-only Docker/Podman workload view |

Per-VM TC/eBPF edge (Network Fabric schema v4) is on the VM detail **Dataplane** tab — not a separate top-level nav item. See [VM Dataplane](pages/infrastructure/dataplane.md).

### Operations

| Page | Route | When to use |
|------|-------|-------------|
| DRS | `/app/drs` | Placement balance / affinity across hosts |
| Fault Tolerance | `/app/fault-tolerance` | Live secondary replica for a VM |
| Replication | `/app/replication` | Cross-site replication + RPO |
| Site Recovery | `/app/site-recovery` | DR plans and failover executions |
| Migrations | `/app/migrations` | Move a VM to another host |
| Migration Wizard | `/app/migration-wizard` | Disk-image → Fabric VM conversion |
| Templates | `/app/templates` | Stamp new VMs from saved configs |
| Content Library | `/app/content-library` | Templates / ISOs / guest specs |
| Schedules | `/app/schedules` | Recurring start/stop/restart/snapshot |
| Autoscale | `/app/autoscale` | Per-VM CPU/memory grow/shrink policies |
| Availability Zones | `/app/zones` | Placement zones + spot instances |
| Snapshots | `/app/snapshots` | Point-in-time disk (or disk+memory) |
| Backups | `/app/backups` | Full/incremental backup and restore |
| Quotas | `/app/quotas` | Cap CPU/memory/disk/VM count |
| Lifecycle | `/app/lifecycle` | Patch baselines and host remediation |
| Bulk Operations | `/app/bulk-operations` | Start/stop/restart/snapshot many VMs |

### Monitoring

| Page | Route | When to use |
|------|-------|-------------|
| Logs | `/app/logs` | Searchable Fabric audit/event log |
| Analytics | `/app/analytics` | Fleet utilization trends / reports |
| Audit | `/app/audit` | Who did what (security trail) |
| Notifications | `/app/notifications` | Delivery channels, rules, history |
| Alerts | `/app/alerts` | Currently firing alerts |
| Timeline | `/app/timeline` | Merged audit + alert feed |
| Processes | `/app/processes` | Live host process table |
| Kernel | `/app/kernel` | Kernel version, modules, sysctls |
| Debug Tools | `/app/debug` | top / iostat / vmstat / netstat panels |
| Explain | `/app/explain` | Plain-language metric explanations |
| Live Metrics | `/app/live-metrics` | 1s CPU/mem/disk/net sparklines |
| Event Stream | `/app/event-stream` | Live SSE VM lifecycle events |
| Optimizer | `/app/resource-optimizer` | Right-size recommendations |
| Capacity | `/app/capacity-planning` | Usage vs capacity trends |
| Service Map | `/app/service-map` | Discovered services and deps |

### Security

| Page | Route | When to use |
|------|-------|-------------|
| Security Dashboard | `/app/security-dashboard` | Live risk / threats / failed logins |
| Encryption | `/app/encryption` | KMS providers + encryption policies |
| Certificates | `/app/certificates` | PKI, CSRs, attestation, baselines |
| Compliance | `/app/compliance` | Config compliance scorecard |
| Access Control | `/app/access-control` | Local users and roles |
| Plugins | `/app/plugins` | Enable/disable server extensions |

### Tools

| Page | Route | When to use |
|------|-------|-------------|
| Webhooks | `/app/webhooks` | Outbound event webhooks |
| Cost Estimator | `/app/cost-estimator` | What-if cloud storage cost |
| VM Compare | `/app/vm-compare` | Side-by-side VM config diff |
| VM Health Check | `/app/vm-healthcheck` | On-demand per-VM checks |
| Notification Center | `/app/notification-center` | Session-only live alert tray |
| API Playground | `/app/playground` | Ad-hoc authenticated API calls |

### More — images, migrations & managers

| Page | Route | When to use |
|------|-------|-------------|
| Readiness / History / Report | `/app/migration-*` | Preflight, history, shareable report |
| Migration Templates / Batch | `/app/migration-templates`, `/app/batch-migration` | Reusable / multi-VM migration specs |
| ISO / Disk Images | `/app/iso-images`, `/app/disk-images` | Inventory of ISOs and disks |
| Upload / Download Disk | `/app/upload-disk`, `/app/download-disk` | Move images to/from the host |
| Disk Converter / Pipeline / Jobs | `/app/disk-converter`, `/app/pipeline`, `/app/job-monitor` | Convert and watch jobs |
| Image Builder | `/app/image-builder` | mkosi-based custom images |
| Backup Scheduler | `/app/backup-scheduler` | Recurring backup jobs |
| Batch Import | `/app/batch-import` | Bulk-create VMs from YAML/JSON |
| Snapshot / Storage Mgr | `/app/snapshot-manager`, `/app/storage-manager` | Alternate snapshot/pool browsers |
| Manifest Builder | `/app/manifest-builder` | Client-side YAML scratchpad |
| Network Topology | `/app/network-topology` | VM ↔ bridge / NIC map |

Every route is also listed in the [complete page index](PAGE_INDEX.md).

## Related

- [Getting Started](getting-started.md)
- [Common workflows](workflows.md)
- [Admin basics](admin-basics.md)
- [Page-by-page guides](pages/README.md)
