# VM Compare

## Purpose

VM Comparison — a side-by-side diff of two VMs' configurations, run on demand against the live VM list.

Compares live config fields via `GET /api/vms/compare?source=…&target=…`. It does not mutate either VM.

## When to use it

- To see what differs between two VMs before assuming they're equivalent (useful when troubleshooting "why does this one behave differently")
- To confirm a newly cloned or templated VM actually matches its source
- To audit configuration drift between two VMs that are supposed to be identical
- Prefer this page when the job matches the purpose above
- After [Templates](../operations/templates.md) create, to verify the stamp matches the golden VM

## How to get there

- Route / id: `/vm-compare`
- Nav: **Tools → VM Compare** (sidebar, command palette, or desktop nav)

## Operate from the console (UX)

1. Pick **Source VM** and **Target VM** from dropdowns populated from the VM list (target excludes the chosen source).
2. Click **Compare** (disabled until both are selected).
3. Results table: one row per field — Source value, Target value, Match (Yes green / No amber).
4. Header refresh re-fetches the VM list if it is stale.
5. Re-run Compare after you change either VM's config to confirm drift is gone.

Typical flow: source = golden / template parent → target = new VM → note No rows → fix on Virtual Machines → Compare again. For health rather than config, use [VM Health Check](vm-healthcheck.md).

**Empty / fail:** Error banner, empty table, or failed toast — confirm you are signed in, `zyvor-fabricd` is healthy (`/readyz` on `http://127.0.0.1:<port>` or `https://<host>`), and any backend this page needs (FluxVM, storage, network) is reachable. See [Admin basics](../../admin-basics.md).

**Success:** Live data loads without error; creates/updates appear in the list or detail view and any confirmation toast clears cleanly.

## Related pages

- [VM Health Check](vm-healthcheck.md)
- [Virtual Machines](../core/vms.md)
- [Templates](../operations/templates.md)
- [Profiles](../core/profiles.md)
- [API Playground](playground.md)
- [Webhooks](webhooks.md)
- [Getting Started](../../getting-started.md)
- [Page index](../../PAGE_INDEX.md)
