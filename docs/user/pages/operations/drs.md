# DRS

## Purpose

Distributed Resource Scheduler (DRS) — balances VM placement across the hosts in a cluster, surfaces migration recommendations, enforces affinity/anti-affinity rules, and can test where a new VM would land before you create it.

## When to use it

- To see how evenly CPU and memory load is spread across your cluster's hosts
- To review (and approve or reject) migrations DRS suggests to rebalance load
- To keep certain VMs together (affinity) or apart (anti-affinity) — e.g. two replicas of the same service on different hosts
- To test where a hypothetical VM (given its vCPU/memory needs) would be placed before actually creating it

## How to get there

- Route / id: `/app/drs`
- Nav: **Operations → DRS** (sidebar, command palette, or desktop nav)

## Operate from the console (UX)

**DRS configuration** (top of page) — toggle DRS **On/Off**, set the **Automation Level** (Manual, Semi-Auto, or Fully Auto), adjust the **Migration Threshold** (1–5, how aggressively it rebalances), and see the configured check interval. Changes apply immediately.

Four tabs below that:

1. **Balance** — the overall cluster balance score plus CPU/memory imbalance percentages, and a per-host breakdown with live CPU and memory usage bars (color-coded amber/red as usage climbs).
2. **Recommendations** — a table of migrations DRS suggests (VM, source host, target host, reason, priority, estimated benefit). Pending recommendations can be **approved** (checkmark) or **rejected** (X) directly from the row.
3. **Rules** — affinity/anti-affinity rules, each showing type, VM count, whether it's mandatory, and whether it's enabled. **Create Rule** sets a name, chooses **Affinity** (keep together) or **Anti-Affinity** (keep apart), lists VM IDs (comma-separated), and an optional mandatory flag. Delete a rule from its row (confirmation required).
4. **Placement** — a calculator: enter a hypothetical VM's CPU (MHz) and memory (MB), click **Test Placement**, and see the recommended host, its score and reasoning, projected CPU/memory usage after placement, and scored alternative hosts.

**Empty / fail:** Error banner, empty table, or failed toast — confirm you are signed in, `zyvor-fabricd` is healthy (`/readyz`), and any backend this page needs (FluxVM, storage, network) is reachable. See [Admin basics](../../admin-basics.md).

**Success:** Live data loads without error; creates/updates appear in the list or detail view and any confirmation toast clears cleanly.

## Related pages

- [Snapshots](snapshots.md)
- [Backups](backups.md)
- [Migrations](migrations.md)
- [Getting Started](../../getting-started.md)
- [Page index](../../PAGE_INDEX.md)
