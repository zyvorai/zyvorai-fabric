# Wave 8 research: hypervisor control planes

Goal: find operator-facing capabilities in mature VM management platforms that
Fabric lacks and that fit Fabric's data model (a daemon with a VM store, a
datacenter/cluster/host registry, DRS affinity rules, snapshot policies,
overcommit policy). Searches were run in November 2026. Several vendor pages
could not be opened directly; claims below are limited to what search results
stated, and gaps are marked.

## What the platforms do

| Platform | Capability | Source |
|---|---|---|
| Proxmox VE | HA groups are deprecated in favour of HA rules: node-affinity rules (strict or non-strict) and resource-affinity rules. Maintenance mode only moves resources if enough nodes remain once all rules are applied; a strict rule can keep VMs on a node in maintenance. Rule checker rejects rules it cannot prove resolvable. | https://pve.proxmox.com/pve-docs/ha-manager.1.html , https://forum.proxmox.com/threads/clarification-related-to-ha-maintenance-mode-affinity-rules.182890/ |
| Proxmox Backup Server | Verify jobs (re-verify at least monthly, skip already verified snapshots), email notification of job results. No built-in "time since last good backup" check was found. | https://pbs.proxmox.com/docs-2/maintenance.html |
| VMware DRS | Migration threshold from conservative to aggressive; at the most conservative level DRS only fixes rule violations or evacuates a host entering maintenance. "Must" rules plus host groups can leave nowhere to evacuate so maintenance mode stalls; guidance is to use "should" or disable the rule. | https://docs.vmware.com/en/VMware-vSphere/8.0/vsphere-resource-management/GUID-188B0D09-F697-4905-83B2-95E075F80FAC.html , https://knowledge.broadcom.com/external/article/398210/drs-should-run-on-hosts-in-group-affinit.html |
| OpenStack Nova | Evacuate, live and cold migrate go through the scheduler; forced moves are discouraged and the `force` option was removed in microversion 2.68, so server group policy applies on moves. Anti-affinity was historically only honoured at boot, and concurrent evacuations could land two anti-affine VMs on one host. Host aggregates group hosts; the aggregate "evacuate-host" action is deprecated since 2.43 in favour of disabling the service. | https://docs.openstack.org/nova/train/user/aggregates , https://bugs.launchpad.net/bugs/1735407 , https://blueprints.launchpad.net/nova/+spec/anti-affinity-on-migration , https://bugs.launchpad.net/bugs/1890065 |
| oVirt / RHV | Cluster CPU type is the oldest CPU family of its hosts; hosts older than that cannot join. Scheduler filters include a CPU-level filter and VM affinity groups (positive or negative, hard or soft). | https://www.ovirt.org/develop/release-management/features/sla/scheduler-policies , https://ovirt.org/develop/release-management/features/sla/vm-affinity.html , https://docs.redhat.com/en/documentation/red_hat_virtualization/4.4/html/administration_guide/chap-clusters |
| libvirt / QEMU | Incremental backup via persistent dirty bitmaps (qcow2 only) and checkpoints; `virsh hypervisor-cpu-compare` and `hypervisor-cpu-baseline` compute whether a CPU definition runs on a host and a common baseline for a migration pool; `host-model` expresses the host CPU as model plus features. | https://libvirt.org/kbase/internals/incremental-backup.html , https://ubuntu.com/server/docs/explanation/virtualisation/live-migration/ , https://www.ibm.com/docs/en/linuxonibm/ldva/ldva_r_virsh_hypervisor_cpu_compare.html |
| KubeVirt / OpenShift Virtualization / Harvester | Per-VM eviction strategy (`LiveMigrate`, `LiveMigrateIfPossible`, `None`); a LiveMigrate VM that cannot move blocks a node drain. Harvester maintenance mode batch-migrates live-migratable VMs and leaves non-migratable ones to the operator (from a third-party blog; official Harvester docs were not reachable). | https://docs.redhat.com/en/documentation/openshift_container_platform/4.21/html/virtualization/nodes , https://oneuptime.com/blog/post/2026-03-20-harvester-migrate-vms-between-nodes/view |

## What Fabric had and lacked

Had: per-VM start preflight, declarative plans with a plan hash, capacity
what-if on the local host, DRS affinity rules and host maintenance flag,
autostart reconcile, snapshot policies with retention, audit trail.

Lacked:
1. Any way to ask "what happens if I put this host in maintenance". Entering
   maintenance only flips a status flag; nothing tells the operator which VMs
   are stuck, which rules block the move, or in what order to act. All of
   Proxmox, DRS, KubeVirt and Harvester treat evacuation as rule-aware.
2. A fleet view of backup posture. Snapshot policies exist, but nothing
   compares the age of the newest snapshot with the policy, flags broken
   parent chains, or forecasts snapshot space.
3. CPU model compatibility for moves (oVirt, libvirt).
4. Placement scoring with anti-affinity in the what-if planner.

## Ranked ideas

| Rank | Idea | Rationale | Verdict |
|---|---|---|---|
| 1 | Host drain planner (dry run, ordered, rule-aware, plan hash) | Highest operator value; the documented failure mode on VMware, Proxmox and KubeVirt is a drain that silently stalls on a rule or non-migratable VM, so surfacing blockers up front is the feature. Needs only data Fabric has (hosts, VMs, DRS rules, overcommit). | Built |
| 2 | Snapshot health scoreboard (RPO vs policy, broken chains, space forecast) | Cheap, read-only, uses existing policies and snapshot records; PBS shows the gap (no last-good-backup check). | Built |
| 3 | Anti-affinity in the what-if planner | The drain planner already implements the placement rules (required and preferred, VM and host rules); reusing them in `fit_vms` would change the shared Rust/TypeScript fixture contract and the what-if is single-host today. | Deferred |
| 4 | CPU model compatibility check | Fabric's VM and host records do not store a CPU model or feature set, and the daemon does not query hosts for one. The check would be guesswork until the host agent reports CPU features. | Rejected for now |
| 5 | Dirty-bitmap incremental backup | Needs QMP bitmap management and a backup target in the FluxVM driver; far larger than a wave. | Rejected |

## Honest limits of the built features

* Fabric does not record which host a VM runs on. The drain planner reads the
  label `fabric.zyvor.dev/host` (host id or hostname); unlabelled VMs are not
  planned and the plan says how many were skipped.
* The planner is advisory: there is no apply step, and the plan hash lets a
  client detect that the plan changed between two reads.
* Capacity uses assigned VM sizes of running VMs, not live utilisation.
* Snapshot space forecast is snapshot usage only; Fabric has no free-disk
  figure to compare against.
