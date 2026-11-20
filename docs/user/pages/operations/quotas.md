# Quotas

## Purpose

Resource Quotas — cap CPU, memory, disk, and VM-count usage, applied either globally or to VMs matching specific tags, so a team or workload can't consume unlimited host resources.

Exceeded quotas block **new** matching VMs until usage drops. Enable/disable lets you park a quota without deleting it.

## When to use it

- To limit how many CPUs, how much memory/disk, or how many VMs a team can create
- To scope a limit to a subset of VMs by tag, rather than the whole host
- To check whether a quota is currently exceeded and which resource pushed it over
- To temporarily disable a quota without deleting its configuration
- Before handing a shared host to multiple teams — tag VMs and attach per-team quotas

## How to get there

- Route / id: `/app/quotas`
- Nav: **Operations → Quotas** (sidebar, command palette, or desktop nav)

## Operate from the console (UX)

1. **Create Quota** — name; max CPUs, memory (MB), disk (GB), max VMs; optional tags to scope (empty tags = global). Checkbox to enable immediately.
2. Each quota card shows **Enabled/Disabled** and **Exceeded** badges, plus four usage bars (CPUs, memory, disk, VMs) with used/limit and color (green &lt;75%, yellow 75–89%, red 90%+).
3. If exceeded, the card lists which resources are over limit and notes that new matching VMs cannot be created until usage drops.
4. Per-quota: **Enable/Disable**, **Edit** (same form; warns if a new limit would go below current usage), **Delete** (confirmation, cannot be undone).
5. Empty state offers a shortcut to create the first quota.

Typical flow: agree tag scheme → create tagged quotas with headroom → watch bars after create storms → disable temporarily for emergency capacity, then re-enable. Cross-check [Capacity](../monitoring/capacity-planning.md) for host-level ceilings.

**Empty / fail:** Error banner, empty table, or failed toast — confirm you are signed in, `zyvor-fabricd` is healthy (`/readyz` on `http://127.0.0.1:<port>` or `https://<host>`), and any backend this page needs (FluxVM, storage, network) is reachable. See [Admin basics](../../admin-basics.md).

**Success:** Live data loads without error; creates/updates appear in the list or detail view and any confirmation toast clears cleanly.

## Related pages

- [Profiles](../core/profiles.md)
- [Templates](templates.md)
- [Autoscale](autoscale.md)
- [Capacity](../monitoring/capacity-planning.md)
- [Virtual Machines](../core/vms.md)
- [Access Control](../security/access-control.md)
- [Getting Started](../../getting-started.md)
- [Page index](../../PAGE_INDEX.md)
