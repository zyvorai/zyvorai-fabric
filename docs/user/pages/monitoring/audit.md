# Audit

## Purpose

Audit Logs — the security and compliance trail of who did what: every tracked action, which user performed it, on which resource, whether it succeeded, and from what IP address.

Prefer Audit when you need **actor + IP + success/fail**. Prefer [Logs](logs.md) for streaming console-style messages; [Timeline](timeline.md) for a merged narrative.

## When to use it

- To investigate who created, deleted, started, or stopped a specific VM (or other resource) and when
- To check recent failures for signs of misconfiguration or unauthorized attempts
- To pull an export of the audit trail for a compliance review or incident report
- After access-control changes, to verify which admin performed them
- When correlating failed logins from [Security Dashboard](../security/security-dashboard.md) with resource actions

## How to get there

- Route / id: `/app/audit`
- Nav: **Monitoring → Audit** (sidebar, command palette, or desktop nav)

## Operate from the console (UX)

1. **Stats row** — total logs, success rate, recent failures, top 3 most common actions.
2. **Search** — matches action, user, resource name, and resource type (Enter or live filter).
3. **Filters** — Status (success/failed) and Resource Type (VM, network, storage, template, quota, schedule); **Clear Filters** resets filters and search.
4. **Export** — downloads the filtered set as JSON or CSV.
5. **Logs table** — relative timestamp, user, action (color-coded), resource type/name, success/failed badge (error inline on fail), source IP. Footer shows filtered vs total counts.

Typical flow: filter Failed → search VM name → Export CSV/JSON for the ticket → open Access Control if a user must be disabled. Read-only history.

**Empty / fail:** Error banner, empty table, or failed toast — confirm you are signed in, `zyvor-fabricd` is healthy (`/readyz` on `http://127.0.0.1:<port>` or `https://<host>`), and any backend this page needs (FluxVM, storage, network) is reachable. See [Admin basics](../../admin-basics.md).

**Success:** Live data loads without error; creates/updates appear in the list or detail view and any confirmation toast clears cleanly.

## Related pages

- [Logs](logs.md)
- [Timeline](timeline.md)
- [Access Control](../security/access-control.md)
- [Security Dashboard](../security/security-dashboard.md)
- [Compliance](../security/compliance.md)
- [Notifications](notifications.md)
- [Getting Started](../../getting-started.md)
- [Page index](../../PAGE_INDEX.md)
