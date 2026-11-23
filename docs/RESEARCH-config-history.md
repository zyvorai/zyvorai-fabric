# Research: VM configuration history and drift

Question: how do mature platforms answer "who changed this VM, when, and what is
different from when it worked"? Searches were run in November 2026. Vendor pages
were mostly not opened directly; claims are limited to what search results
stated, and anything not confirmed from a primary source is marked [U].

| Platform | What it offers | Gap for Fabric's question |
|---|---|---|
| AWS Config | Keeps a per-resource history of configuration items and a timeline; pairs with CloudTrail for "who" (API activity). Detection latency is described inconsistently by secondary sources (minutes vs hours) [U]. | Timeline plus actor is the model to copy. Fabric has the actor already: its own audit log. |
| Terraform / CloudFormation drift | Compare a declared template with the live resource and list differing properties. | Needs a declared desired state. Fabric VMs are often created imperatively, so the baseline is "a previous recorded revision", not a template. |
| Proxmox VE | Pending changes: edits that cannot be hot-applied show as pending until reboot; `qm pending` lists current vs pending values. No stored history of past configs found. | Shows "not yet applied", not "what changed last week". |
| oVirt | A snapshot stores the VM configuration; comparing it with the live VM is a manual diff of two documents [U]. | Snapshots are heavy; a config-only timeline is cheaper. |
| vSphere | Host Profiles and Configuration Profiles detect drift of host settings from a desired state, on a schedule (8 or 24 hours, sources disagree) [U]. VM-level history is the vCenter task/event log. | Host-level and desired-state based. |
| libvirt | Domain XML has no history; `virsh dumpxml` plus an external diff is the usual practice [U]. | Nothing built in. |

## What Fabric does with this

- Record a redacted revision per change (timeline, as AWS Config), chained to
  the audit event that carries the actor (as AWS Config pairs with CloudTrail).
- Diff any two revisions at field level with plain-language explanations and
  risk flags. No desired state is required, so it also works for VMs created by
  hand.
- "Last known good" instead of "desired state": a revision becomes the
  comparison baseline once the VM has run for a while after it, or when an
  operator says so.

## Rejected or deferred

- Automatic revert: the request was read-only; a wrong revert on a running VM is
  worse than the original problem. The API prints nothing destructive.
- Guest or application health as the good signal: Fabric has no reliable
  in-guest health signal for every VM. The automatic mark uses lifecycle events
  (started, no stop/pause/error) and says so; operators can mark explicitly.
- Storing full configuration text: cloud-init commands, kernel arguments and
  keys can contain secrets, and there is no safe way to tell them apart from
  harmless text, so they are kept only as salted hash markers.
