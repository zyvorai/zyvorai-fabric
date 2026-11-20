# VM Health Check

## Purpose

VM Health Check — runs a set of health verification checks against a single VM, on demand, and reports pass/warning/fail per check plus an overall status.

On-demand via `GET /api/vms/{name}/healthcheck`. A failed **request** (banner + retry) is different from a check that returns fail/warning.

## When to use it

- Before or after a change, to confirm a VM is still in good health
- To triage a VM you suspect is unhealthy, with a per-check breakdown instead of just up/down
- Before promoting a VM to production or handing it off
- Prefer this page when the job matches the purpose above
- After start/migrate/restore, before pointing traffic at the guest

## How to get there

- Route / id: `/vm-healthcheck`
- Nav: **Tools → VM Health Check** (sidebar, command palette, or desktop nav)

## Operate from the console (UX)

1. Select a VM from the dropdown (live VM list).
2. Click **Run Health Check**.
3. Overall banner: **Healthy** or **Issues Found**, with "X of Y checks passed".
4. Each check: status icon (pass / warning / fail), message, optional detail line.
5. Request failure → error banner with retry (not the same as a failing check).
6. Header refresh reloads the VM list.

Typical flow: pick VM → Run → if Issues Found, open detail/console and fix → Run again until Healthy. Pair with [VM Compare](vm-compare.md) when the symptom is config drift rather than runtime health.

**Empty / fail:** Error banner, empty table, or failed toast — confirm you are signed in, `zyvor-fabricd` is healthy (`/readyz` on `http://127.0.0.1:<port>` or `https://<host>`), and any backend this page needs (FluxVM, storage, network) is reachable. See [Admin basics](../../admin-basics.md).

**Success:** Live data loads without error; creates/updates appear in the list or detail view and any confirmation toast clears cleanly.

## Related pages

- [VM Compare](vm-compare.md)
- [Virtual Machines](../core/vms.md)
- [VM Console](../core/vms-name-console.md)
- [Live Metrics](../monitoring/live-metrics.md)
- [Alerts](../monitoring/alerts.md)
- [API Playground](playground.md)
- [Getting Started](../../getting-started.md)
- [Page index](../../PAGE_INDEX.md)
