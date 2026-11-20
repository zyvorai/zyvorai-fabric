# Timeline

## Purpose

Timeline — a single reverse-chronological activity feed that merges audit-log actions and system alerts, so you can see what happened and in what order without switching between Logs and Notifications.

Client-side filter chips classify entries; the feed auto-refreshes every 10 seconds.

## When to use it

- Prefer this page when the job matches the purpose above
- Reconstructing a sequence of events — what led up to an error, in order
- Getting a quick "what's happened lately" view of the whole system
- Filtering down to just deploys, or just errors, to review one class of events
- During an incident, before diving into raw [Logs](logs.md) or [Audit](audit.md)
- After a change window, to confirm expected Actions/Deploys and no unexpected Errors

## How to get there

- Console URL pattern: `http://127.0.0.1:<port>/…` or `https://<host>/…`
- Route / id: `/timeline`
- Nav: **Monitoring → Timeline** (sidebar, command palette, or desktop nav)

## Operate from the console (UX)

1. Filter chips — **All**, **Actions**, **Alerts**, **Deploys**, **Errors** — filter the merged feed client-side.
2. Each entry is auto-classified with icon/color, description, relative timestamp, and type tag: failed/error audit → **Error**; create/deploy-style → **Deploy**; other audit → **Action**; alerts → **Alert** (or **Error** if severity is critical/error).
3. The feed auto-refreshes every 10 seconds; the header shows "Updated Xm ago," plus a manual refresh button.
4. If a background refresh fails after data has already loaded, an amber banner reports it while the last known feed stays on screen.

Typical flow: open Timeline → filter **Errors** → note timestamps → jump to Logs/Audit for the same window → clear filter to **All** for surrounding context. This page does not acknowledge or mute alerts.

**Empty / fail:** Error banner, empty table, or failed toast — confirm you are signed in, `zyvor-fabricd` is healthy (`/readyz` on `http://127.0.0.1:<port>` or `https://<host>`), and any backend this page needs (FluxVM, storage, network) is reachable. See [Admin basics](../../admin-basics.md).

**Success:** Live data loads without error; creates/updates appear in the list or detail view and any confirmation toast clears cleanly.

## Related pages

- [Logs](logs.md)
- [Audit](audit.md)
- [Alerts](alerts.md)
- [Notifications](notifications.md)
- [Event Stream](event-stream.md)
- [Getting Started](../../getting-started.md)
- [Page index](../../PAGE_INDEX.md)
