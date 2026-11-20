# Autoscale

## Purpose

Autoscale — define per-VM policies that automatically grow or shrink a VM's vCPUs and memory within set bounds based on load, and review the history of scaling actions that were triggered.

Policies are **per VM** (one policy per VM). Bounds and cooldown prevent runaway growth; recent scale events show what actually fired.

## When to use it

- To let a VM's resources flex automatically instead of manually resizing it under load
- To cap how far a VM is allowed to scale (min/max vCPUs, min/max memory) so autoscaling can't run away
- To check what scaling actions actually fired and when, via the recent scale events log
- When a workload is bursty but still needs a hard ceiling for host capacity / quotas
- After reviewing [Optimizer](../monitoring/resource-optimizer.md) recommendations, if you want ongoing auto adjust instead of one-shot apply

## How to get there

- Route / id: `/autoscale`
- Nav: **Operations → Autoscale** (sidebar, command palette, or desktop nav)

## Operate from the console (UX)

1. Review the policy table — one row per VM: vCPU range, memory range, CPU scale-up/scale-down thresholds, and cooldown (seconds between actions).
2. **Create Policy** — pick a VM (only VMs without an existing policy), set min/max vCPUs, min/max memory (MB), CPU scale-up and scale-down thresholds (%), and cooldown seconds.
3. **Delete** a policy from its row (confirmation) to stop autoscaling that VM.
4. Check **Recent scale events** — last 20 actions (VM, action, resource, timestamp).
5. On a read-only account, create/delete controls are hidden and a read-only notice is shown.

Typical flow: set conservative max bounds → create policy → watch Recent scale events under load → tighten thresholds or cooldown if it flaps. Confirm host capacity on [Capacity](../monitoring/capacity-planning.md) and [Quotas](quotas.md).

Scale events only appear after real threshold crossings; idle VMs with a policy and no load will show an empty recent log.

**Empty / fail:** Error banner, empty table, or failed toast — confirm you are signed in, `zyvor-fabricd` is healthy (`/readyz` on `http://127.0.0.1:<port>` or `https://<host>`), and any backend this page needs (FluxVM, storage, network) is reachable. See [Admin basics](../../admin-basics.md).

**Success:** Live data loads without error; creates/updates appear in the list or detail view and any confirmation toast clears cleanly.

## Related pages

- [Quotas](quotas.md)
- [Optimizer](../monitoring/resource-optimizer.md)
- [Capacity](../monitoring/capacity-planning.md)
- [Virtual Machines](../core/vms.md)
- [Schedules](schedules.md)
- [Getting Started](../../getting-started.md)
- [Page index](../../PAGE_INDEX.md)
