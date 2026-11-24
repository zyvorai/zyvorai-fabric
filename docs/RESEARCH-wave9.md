# Wave 9 research: day-two hygiene

Goal: find housekeeping capabilities in VM platforms that Fabric lacks and that
fit its data model (VM store, snapshot records, per-VM entities, audit trail).
Searches were run in November 2026; claims are limited to what the pages and
search results stated.

## What the platforms do

| Platform | Capability | Source |
|---|---|---|
| VMware vSphere | Snapshot best practice: a maximum of 32 snapshots per chain is supported, 2 to 3 is recommended, and no single snapshot should be kept longer than 72 hours because it keeps growing. Snapshots are not backups. | https://knowledge.broadcom.com/external/article/318825/best-practices-for-using-vmware-snapshots.html |
| VMware vSphere | "Virtual machine Consolidation Needed" alarm and stale or hidden delta files left after failed snapshot deletes. | https://kb.vmware.com/s/article/2114949 |
| VMware Aria Operations | Reclamation: powered-off VMs are reported after a configurable number of days, orphaned VMDKs after a minimum number of unattached days; the list can be exported and the delete stays with the operator. | https://docs.vmware.com/en/vRealize-Operations/8.6/com.vmware.vcom.user.doc/GUID-7DBBF4C4-D4A0-4212-80DE-31E8EA1BB4F0.html , https://knowledge.broadcom.com/external/article/383876 |
| vCenter, Proxmox VE | Per-VM annotations: a vCenter custom attribute such as VM owner can be sorted and filtered; Proxmox keeps a free-text Notes field (API `description`) next to tags, but notes are not filterable, so owners end up in tags. | https://docs.vmware.com/en/VMware-vSphere/8.0/vsphere-vcenter-esxi-management/GUID-73606C4C-763C-4E27-A1DA-032E4C46219D.html , https://forum.proxmox.com/threads/custom-attributs-for-vms.168996/ |
| Proxmox VE | Backups can be marked protected (excluded from pruning) and carry notes generated from a template. Fabric already has the equivalent of protection (the `keep` snapshot tag). | https://lore.proxmox.com/all/20211216121233.162288-3-f.ebner@proxmox.com/T |
| Veeam management packs | Right-sizing reports compare history with configured CPU and memory. Needs utilisation history. | https://helpcenter.veeam.com/docs/mp/vmware_guide/vms_rightsizing_oversized.html |

## What Fabric had and lacked

Had: snapshot health (RPO), snapshot policies with `keep`, tags and labels,
config history, audit trail.

Lacked: a way to see snapshots that are simply old or chained too deep (health
only checks recency), a structured owner and lease per VM, and a list of what
could be cleaned up.

## Ranked ideas

| Rank | Idea | Verdict |
|---|---|---|
| 1 | Snapshot hygiene report with the vSphere thresholds | Built |
| 2 | VM owner, contact, notes and lease expiry with a fleet report | Built |
| 3 | Reclaim report (idle VMs, expired leases, stale snapshots) | Built |
| 4 | Orphaned disk and volume detection | Deferred: volumes are stored as size strings per pool and the daemon does not enumerate disk files on the host, so a report would guess. |
| 5 | Right-sizing from utilisation history | Rejected for now: the daemon keeps no per-VM utilisation history to judge "oversized" from. |
| 6 | Automatic stop or delete when a lease expires | Rejected: destructive and surprising; reports only, the operator decides. |

## Honest limits

* Fabric does not record when a VM was stopped. Idle time is measured from the
  VM record's last update (lifecycle changes and edits), so editing a stopped
  VM restarts the clock.
* Reclaimable disk is the provisioned size, an upper bound on thin storage.
* Reclaimable snapshot bytes are an upper bound; a merge frees only what
  neighbouring snapshots do not share.
