# Zyvor Fabric — Complete page index

Marketing: `/`, `/product`, `/platform`, `/security`, `/sign-in`.

Console routes under `/app` — every primary navigable ops route.

_Generated: 2026-09-11 · 88 routes_

Regenerate: `node scripts/user-docs/generate-page-index.mjs`

## Marketing & auth

| Page | Route | Purpose |
|------|-------|---------|
| Home | `/` | Public marketing home |
| Product | `/product` | Product story |
| Platform | `/platform` | Interfaces (Web, CLI, Operator, Terraform) |
| Security | `/security` | Security story |
| Sign in | `/sign-in` | Console authentication (`/login` redirects here) |

## Core

| Page | Route | Purpose | Guide |
|------|-------|---------|-------|
| Dashboard | `/app` | Dashboard — Core surface. | [Open](pages/core/home.md) |
| Favorites | `/app/favorites` | Favorites — Core surface. | [Open](pages/core/favorites.md) |
| Virtual Machines | `/app/vms` | Virtual Machines — Core surface. | [Open](pages/core/vms.md) |
| Machines | `/app/machines` | Machines — Core surface. | [Open](pages/core/machines.md) |
| Profiles | `/app/profiles` | Profiles — Core surface. | [Open](pages/core/profiles.md) |
| Datacenters | `/app/datacenters` | Datacenters — Core surface. | [Open](pages/core/datacenters.md) |
| VM Browser | `/app/vm-browser` | VM Browser — Core surface. | [Open](pages/core/vm-browser.md) |
| Create VM | `/app/create` | Create VM — Core surface. | [Open](pages/core/create.md) |
| VM Console | `/app/vms/:name/console` | VM Console — Core surface. | — |
| Settings | `/app/settings` | Settings — Core product and console preferences. | [Open](pages/core/settings.md) |
| VM Wizard | `/app/vm-wizard` | VM Wizard — guided VM creation flow. | [Open](pages/core/vm-wizard.md) |

## Infrastructure

| Page | Route | Purpose | Guide |
|------|-------|---------|-------|
| Network | `/app/network` | Network — Infrastructure surface. | [Open](pages/infrastructure/network.md) |
| Net Security | `/app/network-security` | Net Security — Infrastructure surface. | [Open](pages/infrastructure/network-security.md) |
| Edge Dataplane | `/app/edge-dataplane` | Edge Dataplane — cluster Network Fabric / Service Fabric console. | [Open](pages/infrastructure/edge-dataplane.md) |
| Storage | `/app/storage` | Storage — Infrastructure surface. | [Open](pages/infrastructure/storage.md) |
| Storage Pools | `/app/storage-pools` | Storage Pools — Infrastructure surface. | [Open](pages/infrastructure/storage-pools.md) |
| Distributed Storage | `/app/distributed-storage` | Distributed Storage — Infrastructure surface. | [Open](pages/infrastructure/distributed-storage.md) |
| Resource Pools | `/app/resource-pools` | Resource Pools — Infrastructure surface. | [Open](pages/infrastructure/resource-pools.md) |
| System | `/app/system` | System — Infrastructure surface. | [Open](pages/infrastructure/system.md) |
| System Health | `/app/system-health` | System Health — Infrastructure surface. | [Open](pages/infrastructure/system-health.md) |
| Containers | `/app/containers` | Containers — Infrastructure surface. | [Open](pages/infrastructure/containers.md) |
| Zones | `/app/zones` | Zones — Infrastructure surface for placement domains. | [Open](pages/infrastructure/zones.md) |
| VM Pools | `/app/vm-pools` | VM Pools — warm / capacity pools for fast provisioning. | [Open](pages/infrastructure/vm-pools.md) |

## Operations

| Page | Route | Purpose | Guide |
|------|-------|---------|-------|
| DRS | `/app/drs` | DRS — Operations surface. | [Open](pages/operations/drs.md) |
| Fault Tolerance | `/app/fault-tolerance` | Fault Tolerance — Operations surface. | [Open](pages/operations/fault-tolerance.md) |
| Replication | `/app/replication` | Replication — Operations surface. | [Open](pages/operations/replication.md) |
| Site Recovery | `/app/site-recovery` | Site Recovery — Operations surface. | [Open](pages/operations/site-recovery.md) |
| Migrations | `/app/migrations` | Migrations — Operations surface. | [Open](pages/operations/migrations.md) |
| Migration Wizard | `/app/migration-wizard` | Migration Wizard — Operations surface. | [Open](pages/operations/migration-wizard.md) |
| Templates | `/app/templates` | Templates — Operations surface. | [Open](pages/operations/templates.md) |
| Content Library | `/app/content-library` | Content Library — Operations surface. | [Open](pages/operations/content-library.md) |
| Schedules | `/app/schedules` | Schedules — Operations surface. | [Open](pages/operations/schedules.md) |
| Autoscale | `/app/autoscale` | Autoscale — Operations surface. | [Open](pages/operations/autoscale.md) |
| Snapshots | `/app/snapshots` | Snapshots — Operations surface. | [Open](pages/operations/snapshots.md) |
| Backups | `/app/backups` | Backups — Operations surface. | [Open](pages/operations/backups.md) |
| Container Groups | `/app/container-groups` | Container Groups — Secure Containers workloads via Fabric. | [Open](pages/operations/container-groups.md) |
| Agents | `/app/agents` | Agents — agent-runtime proxy list / deploy. | [Open](pages/operations/agents.md) |
| Sessions | `/app/sessions` | Sessions — agent-runtime session control. | [Open](pages/operations/sessions.md) |
| Quotas | `/app/quotas` | Quotas — Operations surface. | [Open](pages/operations/quotas.md) |
| Lifecycle | `/app/lifecycle` | Lifecycle — Operations surface. | [Open](pages/operations/lifecycle.md) |
| Bulk Operations | `/app/bulk-operations` | Bulk Operations — Operations surface. | [Open](pages/operations/bulk-operations.md) |

## Monitoring

| Page | Route | Purpose | Guide |
|------|-------|---------|-------|
| Logs | `/app/logs` | Logs — Monitoring surface. | [Open](pages/monitoring/logs.md) |
| Analytics | `/app/analytics` | Analytics — Monitoring surface. | [Open](pages/monitoring/analytics.md) |
| Audit | `/app/audit` | Audit — Monitoring surface. | [Open](pages/monitoring/audit.md) |
| Notifications | `/app/notifications` | Notifications — Monitoring surface. | [Open](pages/monitoring/notifications.md) |
| Alerts | `/app/alerts` | Alerts — Monitoring surface. | [Open](pages/monitoring/alerts.md) |
| Timeline | `/app/timeline` | Timeline — Monitoring surface. | [Open](pages/monitoring/timeline.md) |
| Processes | `/app/processes` | Processes — Monitoring surface. | [Open](pages/monitoring/processes.md) |
| Kernel | `/app/kernel` | Kernel — Monitoring surface. | [Open](pages/monitoring/kernel.md) |
| Debug Tools | `/app/debug` | Debug Tools — Monitoring surface. | [Open](pages/monitoring/debug.md) |
| Explain | `/app/explain` | Explain — Monitoring surface. | [Open](pages/monitoring/explain.md) |
| Live Metrics | `/app/live-metrics` | Live Metrics — Monitoring surface. | [Open](pages/monitoring/live-metrics.md) |
| Event Stream | `/app/event-stream` | Event Stream — Monitoring surface. | [Open](pages/monitoring/event-stream.md) |
| Optimizer | `/app/resource-optimizer` | Optimizer — Monitoring surface. | [Open](pages/monitoring/resource-optimizer.md) |
| Capacity | `/app/capacity-planning` | Capacity — Monitoring surface. | [Open](pages/monitoring/capacity-planning.md) |
| Service Map | `/app/service-map` | Service Map — Monitoring surface. | [Open](pages/monitoring/service-map.md) |

## Security

| Page | Route | Purpose | Guide |
|------|-------|---------|-------|
| Security Dashboard | `/app/security-dashboard` | Security Dashboard — Security surface. | [Open](pages/security/security-dashboard.md) |
| Encryption | `/app/encryption` | Encryption — Security surface. | [Open](pages/security/encryption.md) |
| Certificates | `/app/certificates` | Certificates — Security surface. | [Open](pages/security/certificates.md) |
| Compliance | `/app/compliance` | Compliance — Security surface. | [Open](pages/security/compliance.md) |
| Access Control | `/app/access-control` | Access Control — Security surface. | [Open](pages/security/access-control.md) |
| Plugins | `/app/plugins` | Plugins — Security surface. | [Open](pages/security/plugins.md) |

## Tools

| Page | Route | Purpose | Guide |
|------|-------|---------|-------|
| Webhooks | `/app/webhooks` | Webhooks — Tools surface. | [Open](pages/tools/webhooks.md) |
| Cost Estimator | `/app/cost-estimator` | Cost Estimator — Tools surface. | [Open](pages/tools/cost-estimator.md) |
| VM Compare | `/app/vm-compare` | VM Compare — Tools surface. | [Open](pages/tools/vm-compare.md) |
| VM Health Check | `/app/vm-healthcheck` | VM Health Check — Tools surface. | [Open](pages/tools/vm-healthcheck.md) |
| Notification Center | `/app/notification-center` | Notification Center — Tools surface. | [Open](pages/tools/notification-center.md) |
| API Playground | `/app/playground` | API Playground — try Fabric APIs interactively. | [Open](pages/tools/playground.md) |

## More — images, migrations & managers

| Page | Route | Purpose | Guide |
|------|-------|---------|-------|
| Readiness | `/app/migration-readiness` | Readiness — More — images, migrations & managers surface. | [Open](pages/more-images-migrations-managers/migration-readiness.md) |
| History | `/app/migration-history` | History — More — images, migrations & managers surface. | [Open](pages/more-images-migrations-managers/migration-history.md) |
| Report | `/app/migration-report` | Report — More — images, migrations & managers surface. | [Open](pages/more-images-migrations-managers/migration-report.md) |
| Migration Templates | `/app/migration-templates` | Migration Templates — More — images, migrations & managers surface. | [Open](pages/more-images-migrations-managers/migration-templates.md) |
| Batch Migration | `/app/batch-migration` | Batch Migration — More — images, migrations & managers surface. | [Open](pages/more-images-migrations-managers/batch-migration.md) |
| ISO Images | `/app/iso-images` | ISO Images — More — images, migrations & managers surface. | [Open](pages/more-images-migrations-managers/iso-images.md) |
| Upload Disk | `/app/upload-disk` | Upload Disk — More — images, migrations & managers surface. | [Open](pages/more-images-migrations-managers/upload-disk.md) |
| Download Disk | `/app/download-disk` | Download Disk — More — images, migrations & managers surface. | [Open](pages/more-images-migrations-managers/download-disk.md) |
| Image Builder | `/app/image-builder` | Image Builder — More — images, migrations & managers surface. | [Open](pages/more-images-migrations-managers/image-builder.md) |
| Pipeline | `/app/pipeline` | Pipeline — More — images, migrations & managers surface. | [Open](pages/more-images-migrations-managers/pipeline.md) |
| Disk Images | `/app/disk-images` | Disk Images — More — images, migrations & managers surface. | [Open](pages/more-images-migrations-managers/disk-images.md) |
| Disk Converter | `/app/disk-converter` | Disk Converter — More — images, migrations & managers surface. | [Open](pages/more-images-migrations-managers/disk-converter.md) |
| Backup Scheduler | `/app/backup-scheduler` | Backup Scheduler — More — images, migrations & managers surface. | [Open](pages/more-images-migrations-managers/backup-scheduler.md) |
| Batch Import | `/app/batch-import` | Batch Import — More — images, migrations & managers surface. | [Open](pages/more-images-migrations-managers/batch-import.md) |
| Snapshot Mgr | `/app/snapshot-manager` | Snapshot Mgr — More — images, migrations & managers surface. | [Open](pages/more-images-migrations-managers/snapshot-manager.md) |
| Storage Mgr | `/app/storage-manager` | Storage Mgr — More — images, migrations & managers surface. | [Open](pages/more-images-migrations-managers/storage-manager.md) |
| Manifest Builder | `/app/manifest-builder` | Manifest Builder — More — images, migrations & managers surface. | [Open](pages/more-images-migrations-managers/manifest-builder.md) |
| Job Monitor | `/app/job-monitor` | Job Monitor — More — images, migrations & managers surface. | [Open](pages/more-images-migrations-managers/job-monitor.md) |
| Network Topology | `/app/network-topology` | Network Topology — More — images, migrations & managers surface. | [Open](pages/more-images-migrations-managers/network-topology.md) |

## Auth

| Page | Route | Purpose | Guide |
|------|-------|---------|-------|
| Login | `/app/sign-in` | Login — Auth surface. | [Open](pages/auth/login.md) |

## Related

- [User docs home](README.md)
- [Page-by-page guides](pages/README.md)
