# Page-by-page guides

Each guide follows: Purpose → When to use it → How to get there → What you can do → Related pages.

Every route is also listed in the [complete page index](../PAGE_INDEX.md).

## Auth

| Page | What it covers |
|------|----------------|
| [Sign in](auth/login.md) | Sign-in screen for Zyvor Fabric. Authenticates against the local admin account or a configured identity provider / PAM user on the host. |

## Core

| Page | What it covers |
|------|----------------|
| [Create VM](core/create.md) | Create VM — a three-step wizard (Basics → Resources → Review) for launching a new virtual machine, including how it's networked and whether it's reachable from outside the host. |
| [Datacenters](core/datacenters.md) | Datacenters — the physical inventory tree: datacenters, the clusters inside each one, and the hosts registered to each cluster, with live CPU/memory usage and VM counts per host. |
| [Favorites](core/favorites.md) | Favorites — a personal, starred shortlist of VMs pulled from your full VM list, so the machines you use most are one click away instead of buried in a longer list. |
| [Dashboard](core/home.md) | Dashboard — the fabric at a glance: how many VMs exist and in what state, live CPU/memory trends, and whether each backend subsystem is actually reachable. |
| [Machines (removed)](core/machines.md) |  |
| [Profiles](core/profiles.md) | Profiles (shown in the UI as Instance Types) — a library of VM sizing presets (vCPUs, memory, disk, and optionally network bandwidth) you can pick instead of hand-tuning resources every time you create a VM. |
| [Settings](core/settings.md) | Settings — Core product and console preferences: daemon identity and log level, auto-refresh, default bridge/DNS, default storage pool and disk format, snapshot retention, auth/TLS/session/audit toggles, and notification hooks (email / webhook / VM lifecycle events). |
| [VM Browser](core/vm-browser.md) | VM Browser — a lightweight, read-only grid of every VM, for quickly scanning or searching without the bulk-action tooling of the full [Virtual Machines](core/vms.md) list. |
| [VM Wizard](core/vm-wizard.md) | VM Wizard — legacy guided VM creation route. The console now uses the three-step Create VM wizard; `/app/vm-wizard` redirects there automatically. |
| [VM Console](core/vms-name-console.md) | VM Console — a real, live console into a running VM, from the browser, without SSH or any other client installed. |
| [Virtual Machines](core/vms.md) | Virtual Machines — the fleet view of every VM in the fabric, and the starting point for managing any one of them. |

## Infrastructure

| Page | What it covers |
|------|----------------|
| [Containers](infrastructure/containers.md) | Containers — a read-only Docker/Podman host view (not Container Groups / Secure Containers). Shows per-container state, image, CPU/memory, and network I/O. Monitoring-only (auto-refresh ~3s); no start/stop/delete. |
| [VM Dataplane (Network Fabric schema v4)](infrastructure/dataplane.md) | Per-VM edge security and telemetry powered by FluxVM Network Fabric |
| [Distributed Storage](infrastructure/distributed-storage.md) | Distributed Storage — the enterprise/clustered layer above a single storage backend: replicated pools spanning multiple hosts, storage policies (tiering, replication, encryption/dedup/compression), in-flight VM disk migrations between pools, and datastore clusters with automatic space/latency-based balancing. For creating and starting a single NFS/LVM/ZFS/Ceph pool, see [Storage Pools](infrastructure/storage-pools.md); for browsing volumes inside pools, see [Storage](infrastructure/storage.md). |
| [Edge Dataplane (cluster)](infrastructure/edge-dataplane.md) | Cluster-wide console for FluxVM Network Fabric schema v4 (security groups, |
| [Net Security](infrastructure/network-security.md) | Net Security — the advanced SDN and security control plane: network policies scoped to security identities, host firewall profiles/zones/VM assignments, exposed services, QoS traffic shaping, DNS zones/policies, WireGuard VPN tunnels and networks, traffic mirroring, NAT rules/pools/gateways, and bandwidth monitoring with alerts. |
| [Network](infrastructure/network.md) | Network — day-to-day VM networking: which mode a VM uses, its port forwards, and its assigned address. For the advanced SDN stack (policies, firewalls, VPN mesh, QoS, mirroring), see [Net Security](infrastructure/network-security.md). For per-VM TC/eBPF edge allowlists, rate limits, and flows, see [VM Dataplane](infrastructure/dataplane.md). |
| [Resource Pools](infrastructure/resource-pools.md) | Resource Pools — hierarchical CPU/memory allocation pools (nested, with shares, reservations, and limits) that VMs draw from, plus an admission-control test to check whether a workload's requirements would fit before you commit to it. |
| [Storage Pools](infrastructure/storage-pools.md) | Storage Pools — create, start/stop, and monitor the storage backends VM disks live on: local directories, NFS exports, LVM and LVM-thin volume groups, ZFS pools, or Ceph RBD pools. This is where a pool's lifecycle and health live; for the volumes (disks) inside those pools see [Storage](infrastructure/storage.md), and for multi-host replicated/policy-driven storage see [Distributed Storage](infrastructure/distributed-storage.md). |
| [Storage](infrastructure/storage.md) | Storage — a consolidated view of every storage pool's capacity alongside a manual volume tracking ledger. To create or manage a pool itself, use [Storage Pools](infrastructure/storage-pools.md); for replicated/policy-driven storage across hosts, see [Distributed Storage](infrastructure/distributed-storage.md). |
| [System Health](infrastructure/system-health.md) | System Health — a live, read-only dashboard of host resource utilization, refreshing every 2 seconds: CPU, memory, disk I/O, filesystems, network interfaces, and top processes, rolled up into a single health score. |
| [System](infrastructure/system.md) | System — the physical host's hardware topology (CPU sockets/cores/threads, NUMA nodes, hugepages) plus topology-aware optimization recommendations for individual VMs. This is distinct from [System Health](infrastructure/system-health.md), which tracks live utilization rather than hardware layout. |
| [Warm Pools (VM Pools)](infrastructure/vm-pools.md) | Warm Pools — keep N VMs pre-booted and paused from a chosen disk image so you can claim one instantly instead of cold-creating. Each pool shows ready members vs size, vCPUs/memory, and the image path. |
| [Availability Zones](infrastructure/zones.md) | Zones — availability / placement domains for the fabric: named zones (region, status, host count) plus spot instances (max price, priority, eviction policy) that can be requested, evicted, or deleted against those zones. |

## Monitoring

| Page | What it covers |
|------|----------------|
| [Alerts](monitoring/alerts.md) | Alerts — a live view of currently firing system alerts and the notification rules that generate them, polling for updates automatically. |
| [Analytics](monitoring/analytics.md) | Performance Analytics — fleet-wide resource utilization, trends over time, and per-VM performance insights, with exportable reports. |
| [Audit](monitoring/audit.md) | Audit Logs — the security and compliance trail of who did what: every tracked action, which user performed it, on which resource, whether it succeeded, and from what IP address. |
| [Capacity](monitoring/capacity-planning.md) | Capacity Planning — resource usage against total capacity per resource (memory, CPU, storage), with week-over-week trend, so you can see what's running out and when. |
| [Debug Tools](monitoring/debug.md) | Debug Tools — raw, terminal-style output from four classic Linux diagnostic commands (top, iostat, vmstat, netstat) against the host, rendered as monospace panels. This is the closest the dashboard gets to SSHing in and running commands yourself. |
| [Event Stream](monitoring/event-stream.md) | Event Stream — a live, scrolling log of VM lifecycle events (create, start, stop, delete, and similar) pushed over an authenticated SSE connection as they happen. There's no history — you only see events that occur while the page is open. |
| [Explain](monitoring/explain.md) | Explain — plain-language, AI-generated explanations for a chosen system metric: its current value and trend, an assessment of its status, what's contributing to it, and what to do about it. This is the interpretive layer on top of the raw numbers you'd see in Analytics or Debug Tools. |
| [Kernel](monitoring/kernel.md) | Kernel — a snapshot of the host's kernel configuration: version, hostname, architecture, boot command line, loaded kernel modules, and sysctl parameters. This is static configuration, not live activity — for that, see Debug Tools or Event Stream. |
| [Live Metrics](monitoring/live-metrics.md) | Live Metrics — a real-time view of host performance: CPU, memory, disk I/O, and network throughput, each as a rolling sparkline that updates once per second. |
| [Logs](monitoring/logs.md) | Logs — a searchable, filterable console view of Zyvor Fabric's audit log: every recorded action and event, level-coded and continuously refreshed. |
| [Notifications](monitoring/notifications.md) | Notifications — configure how and when Zyvor Fabric alerts you: the delivery channels (email, Slack, Teams, webhook), the rules that trigger them, and a history of what was actually sent. |
| [Processes](monitoring/processes.md) | Processes — a live process monitor for the host: every OS process with its CPU and memory usage, refreshed every 3 seconds, with a per-process detail drill-down. |
| [Optimizer](monitoring/resource-optimizer.md) | Optimizer — a right-sizing advisor that analyzes each VM's actual resource usage and recommends CPU/memory/disk adjustments, with a one-click apply per VM. |
| [Service Map](monitoring/service-map.md) | Service Map — shows the services discovered across your VMs, which ones depend on each other (protocol and port), and each service's current health. |
| [Timeline](monitoring/timeline.md) | Timeline — a single reverse-chronological activity feed that merges audit-log actions and system alerts, so you can see what happened and in what order without switching between Logs and Notifications. |

## More Images Migrations Managers

| Page | What it covers |
|------|----------------|
| [Backup Scheduler](more-images-migrations-managers/backup-scheduler.md) | Backup Scheduler — create and manage recurring, automated backup jobs that snapshot one or more VMs' disks to a directory on a schedule, with retention and format controls. |
| [Batch Import](more-images-migrations-managers/batch-import.md) | Batch Import — bulk-create VMs from a YAML or JSON list, with a preview step and a per-VM status readout as each one is submitted. |
| [Batch Migration](more-images-migrations-managers/batch-migration.md) | Batch Migration Builder — a form-based editor for assembling a multi-VM migration job spec (source disk, target format, sizing) and exporting it as JSON. It builds the spec only; it does not run the migration itself. |
| [Disk Converter](more-images-migrations-managers/disk-converter.md) | Disk Format Converter — convert a single disk image between QCOW2, VMDK, VHD, VHDX, and RAW, tracking the conversion job's progress to completion. |
| [Disk Images](more-images-migrations-managers/disk-images.md) | Disk Images — a read-only inventory of the VM disk images present on the host: name, format, size, and path for each one. |
| [Download Disk](more-images-migrations-managers/download-disk.md) | Download Disk — browse the disk images available on the Fabric host and download any of them straight to your machine. |
| [Image Builder](more-images-migrations-managers/image-builder.md) | Image Builder — build custom VM disk images from scratch using [mkosi](https://github.com/systemd/mkosi), by picking a Linux distribution and a package list, and track builds from queued through to a finished image. |
| [ISO Images](more-images-migrations-managers/iso-images.md) | ISO Images — a read-only inventory of installer and driver ISO files sitting in the host's configured images directory, showing which VMs currently have each one attached. |
| [Job Monitor](more-images-migrations-managers/job-monitor.md) | Job Monitor — a live view of background jobs (disk conversions, migrations, and other pipeline work), with per-job progress, pipeline stage, and streaming logs. |
| [Manifest Builder](more-images-migrations-managers/manifest-builder.md) | Manifest Builder — a client-side form for assembling a VM configuration manifest and exporting it as YAML, with a live preview as you type. It doesn't create a VM or call the API; it's a scratchpad for drafting config to copy elsewhere. |
| [History](more-images-migrations-managers/migration-history.md) | Migration History — a read-only log of completed and failed migration jobs, with status, timing, and where the output landed. |
| [Readiness](more-images-migrations-managers/migration-readiness.md) | Migration Readiness — pre-flight checks that verify the environment is in a good state before you start a migration, with a pass/fail summary and per-check detail. |
| [Report](more-images-migrations-managers/migration-report.md) | Migration Report — a shareable summary of all migration jobs (totals by status, average duration) plus the full per-migration detail table, with copy and print actions. |
| [Migration Templates](more-images-migrations-managers/migration-templates.md) | Migration Templates — reusable migration configuration presets (disk format, vCPUs, memory, network, compression) that you can copy as JSON instead of re-entering the same settings for every migration. |
| [Network Topology](more-images-migrations-managers/network-topology.md) | Network Topology — a live map of which VMs are attached to which virtual networks or host bridges, alongside the host's own bridges and physical NICs, auto-refreshing every 15 seconds. |
| [Pipeline](more-images-migrations-managers/pipeline.md) | Pipeline Monitor — a live, auto-refreshing view of in-progress migration/conversion jobs, showing each job's percent complete and which of five stages it's currently in. |
| [Snapshot Mgr](more-images-migrations-managers/snapshot-manager.md) | Snapshot Manager — create, revert to, and delete disk-state snapshots for a selected VM. |
| [Storage Mgr](more-images-migrations-managers/storage-manager.md) | Storage Manager — browse storage pools with their capacity and usage, and drill into a pool to see its volumes. |
| [Upload Disk](more-images-migrations-managers/upload-disk.md) | Upload Disk Image — drag-and-drop (or browse) upload of a VM disk image file to the server, with live progress and an in-session upload history. |

## Operations

| Page | What it covers |
|------|----------------|
| [Agents](operations/agents.md) | Deploy and list TypeScript agents via Fabric’s agent-runtime proxy (`/api/agents`); 503 unless `[agent_runtime]` is configured. Start sessions from a row. |
| [Autoscale](operations/autoscale.md) | Autoscale — define per-VM policies that automatically grow or shrink a VM's vCPUs and memory within set bounds based on load, and review the history of scaling actions that were triggered. |
| [Backups](operations/backups.md) | Backups & Restore — create full or incremental VM backups, track running backup/restore jobs, and restore a VM from a completed backup, either in place or as a new VM. |
| [Bulk Operations](operations/bulk-operations.md) | Bulk Operations — select any number of VMs and start, stop, restart, or snapshot them together, with a per-VM progress log for the batch. |
| [Container Groups](operations/container-groups.md) | Apply and manage ContainerGroup workloads on FluxVM Secure Containers (`RuntimeClass fluxvm`); placement uses hosts with `secure_containers_ready`. |
| [Content Library](operations/content-library.md) | Content Library — a catalog of reusable provisioning building blocks: libraries of templates/ISOs/OVFs/scripts, guest customization specs (per-OS hostname/domain/DNS settings), and host compliance profiles. |
| [DRS](operations/drs.md) | Distributed Resource Scheduler (DRS) — balances VM placement across the hosts in a cluster, surfaces migration recommendations, enforces affinity/anti-affinity rules, and can test where a new VM would land before you create it. |
| [Fault Tolerance](operations/fault-tolerance.md) | Fault Tolerance (FT) — protect individual VMs with a live secondary replica on another host, so a host failure fails the VM over instead of taking it down, and monitor replication health. |
| [Lifecycle](operations/lifecycle.md) | Lifecycle Manager — define patch/upgrade baselines, scan hosts for compliance against them, remediate non-compliant hosts, and track rolling updates across a host fleet. |
| [Migration Wizard](operations/migration-wizard.md) | Migration Wizard — a three-step wizard (Source → Configure → Review) for converting an existing disk image (local file or remote host) into a new Zyvor Fabric VM. |
| [Migrations](operations/migrations.md) | VM Migrations — disk-copy move between hosts (`/api/migrations`: offline / live rsync / storage) with live progress. Native FluxVM transport is a separate preview API. |
| [Quotas](operations/quotas.md) | Resource Quotas — cap CPU, memory, disk, and VM-count usage, applied either globally or to VMs matching specific tags, so a team or workload can't consume unlimited host resources. |
| [Replication](operations/replication.md) | Replication — register remote replication sites, configure per-VM replication to them with a target RPO (recovery point objective), and monitor sync health and RPO compliance across your fleet. |
| [Schedules](operations/schedules.md) | VM Schedules — automate a recurring lifecycle action (start, stop, restart, or snapshot) for a single VM, on a one-time, daily, or weekly schedule. |
| [Sessions](operations/sessions.md) | Inspect and control agent-runtime sessions (`/api/sessions`); hibernate, resume, cancel, or delete. 503 unless `[agent_runtime]` is configured. |
| [Site Recovery](operations/site-recovery.md) | Site Recovery — define disaster recovery plans that group VMs by source and target site, then execute those plans as a test failover, a planned migration, or a full disaster recovery, and track how each execution unfolds. |
| [Snapshots](operations/snapshots.md) | VM Snapshots — create point-in-time snapshots of a specific VM's disk (or disk + memory), and revert or delete them. Unlike most Operations pages, this one is scoped to one VM at a time, entered by name. |
| [Templates](operations/templates.md) | VM Templates — reusable VM configurations (CPU/memory/disk, tags) that you save from an existing VM and use to stamp out new VMs quickly, instead of configuring resources from scratch each time. |

## Security

| Page | What it covers |
|------|----------------|
| [Access Control](security/access-control.md) | Access Control — manage the user accounts that can sign in to Zyvor Fabric: create accounts, assign a role (admin, operator, or viewer), and enable/disable or delete them. |
| [Certificates](security/certificates.md) | Certificates & Security — a PKI console for Zyvor Fabric: certificate authorities, issued certificates, the CSR approval queue, host TPM/boot attestation, and VM security baselines, rolled up into one health dashboard. |
| [Compliance](security/compliance.md) | Compliance Dashboard — a security and configuration compliance scorecard: an overall score, pass/warning/fail counts by category, and remediation guidance for anything that isn't passing. |
| [Encryption](security/encryption.md) | Encryption — manage VM disk and vMotion encryption: register key management providers (KMIP, HashiCorp Vault Transit, or local software keys), define reusable encryption policies, and see which VMs are encrypted under which policy. |
| [Plugins](security/plugins.md) | Plugin Manager — enable, disable, and review the server extensions installed on Zyvor Fabric (storage, network, security, monitoring, and backup plugin types). |
| [Security Dashboard](security/security-dashboard.md) | Security — a real-time security posture and threat-monitoring view: an overall risk score, active security alerts, recent failed login attempts, and listening network ports on the host. Data auto-refreshes every 5 seconds. |

## Tools

| Page | What it covers |
|------|----------------|
| [Cost Estimator](tools/cost-estimator.md) | Storage Cost Estimator — a what-if calculator that projects cloud storage cost (AWS S3, Azure Blob, GCS) for your VM fleet and compares it against an on-premises baseline. It's a planning tool: figures come from the inputs you set, not from your actual billing or usage. |
| [Notification Center](tools/notification-center.md) | Notification Center — a live, session-only tray of VM events and system alerts, polled from the server every 10 seconds. It's separate from Monitoring → Notifications, which manages persistent delivery channels, rules, and history; this page just surfaces what's firing right now and forgets everything when you reload. |
| [API Playground](tools/playground.md) | API Playground — send authenticated, ad-hoc HTTP requests to the Zyvor Fabric API from the browser: pick a preset endpoint or type any path, choose method, optional JSON body, inspect status/headers/body, and keep a short in-session history. |
| [VM Compare](tools/vm-compare.md) | VM Comparison — a side-by-side diff of two VMs' configurations, run on demand against the live VM list. |
| [VM Health Check](tools/vm-healthcheck.md) | VM Health Check — runs a set of health verification checks against a single VM, on demand, and reports pass/warning/fail per check plus an overall status. |
| [Webhooks](tools/webhooks.md) | Webhook Configuration — manage outbound webhooks that notify an external endpoint (generic HTTP, Slack, or Discord) when specific VM and backup events occur. |

---

89 guides. Regenerate: `node scripts/user-docs/generate-guide-index.mjs`.
