# Profiles

## Purpose

Profiles (shown in the UI as **Instance Types**) — a library of VM sizing presets (vCPUs, memory, disk, and optionally network bandwidth) you can pick instead of hand-tuning resources every time you create a VM.

Built-in profiles ship with the product; custom profiles you create are editable/deletable. Built-ins cannot be removed.

## When to use it

- To review available instance-type presets and their specs, grouped by category (general, compute, memory, storage, GPU)
- To create a reusable custom profile for a sizing you use often
- To remove a custom profile you no longer need — built-in profiles can't be deleted
- Before opening [Create VM](create.md), so you know which preset matches the workload
- To standardize team sizing language ("small-dev", "mem-heavy") without retyping numbers

## How to get there

- Route / id: `/app/profiles`
- Nav: **Core → Profiles** (sidebar, command palette, or desktop nav)

## Operate from the console (UX)

1. Browse profile **cards** — each shows category badge, description, CPUs, memory, disk, and network bandwidth (if set). Built-in cards are labeled **Built-in** and have no delete control.
2. Filter mentally by category (general / compute / memory / storage / GPU) when choosing a preset for a new VM.
3. **Create Profile** — name, category, CPU count, memory (MB), and disk (GB). Submit adds the card to the grid immediately.
4. Delete a **custom** profile with the trash icon on its card. Built-ins stay; the UI simply omits delete for them.
5. After creating a custom profile, use it from the create/wizard flows that offer instance-type selection — this page manages the library, it does not launch VMs by itself.

Typical flow: decide category → create or pick a profile → create the VM from Create/Wizard using that instance type → if quotas apply, confirm the preset still fits under [Quotas](../operations/quotas.md).

**Empty / fail:** Error banner, empty table, or failed toast — confirm you are signed in, `zyvor-fabricd` is healthy (`/readyz` on `http://127.0.0.1:<port>` or `https://<host>`), and any backend this page needs (FluxVM, storage, network) is reachable. See [Admin basics](../../admin-basics.md).

**Success:** Live data loads without error; creates/updates appear in the list or detail view and any confirmation toast clears cleanly.

## Related pages

- [Dashboard](home.md)
- [Virtual Machines](vms.md)
- [Create VM](create.md)
- [VM Wizard](vm-wizard.md)
- [Templates](../operations/templates.md)
- [Quotas](../operations/quotas.md)
- [Getting Started](../../getting-started.md)
- [Page index](../../PAGE_INDEX.md)
