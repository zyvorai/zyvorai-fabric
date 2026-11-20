# VM Wizard

## Purpose

VM Wizard — legacy guided VM creation route. The console now uses the three-step **Create VM** wizard; `/app/vm-wizard` redirects there automatically.

## When to use it

- You followed an old bookmark or doc that pointed at `/vm-wizard` or `/app/vm-wizard`
- You want the current Basics → Resources → Review create flow (use Create VM)
- Prefer Create VM from Core nav for all new provisioning

## How to get there

- Route / id: `/app/vm-wizard` → **redirects to** `/app/create`
- Legacy `/vm-wizard` also redirects into the `/app` create flow
- Nav: use **Core → Create VM** (`/app/create`) — there is no separate Wizard nav item

## Operate from the console (UX)

1. Opening `/app/vm-wizard` immediately navigates to `/app/create` — you should not stay on a distinct Wizard page.
2. Complete the Create VM wizard: **Basics** (name + image) → **Resources** (vCPUs, memory, disk, NAT/bridged networking) → **Review** → create.
3. After create, continue on `/app/vms/:name` for console, network, dataplane, and snapshots.
4. **Empty / fail:** If create fails, check FluxVM on the Dashboard capability chips and image availability (see [Admin basics](../../admin-basics.md)).
5. **Success:** Redirect lands on Create VM; after submit, the new VM appears under Virtual Machines.

## Related pages

- [Create VM](create.md) — the live wizard
- [Virtual Machines](vms.md)
- [Profiles](profiles.md)
- [Getting Started](../../getting-started.md)
- [Page index](../../PAGE_INDEX.md)
