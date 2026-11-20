# Settings

## Purpose

Settings — Core product and console preferences: daemon identity and log level, auto-refresh, default bridge/DNS, default storage pool and disk format, snapshot retention, auth/TLS/session/audit toggles, and notification hooks (email / webhook / VM lifecycle events).

## When to use it

- To change how often the console auto-refreshes live data
- To set the default bridge, DNS servers, or IPv6 preference for new networking
- To pick the default storage pool / disk format / compression and snapshot retention days
- To toggle auth, TLS expectation, session timeout, or audit logging flags exposed in the UI
- To wire a webhook URL and choose which VM events (start / stop / error) should notify

## How to get there

- Route / id: `/app/settings`
- Nav: top-bar **Settings** icon, or command palette → “Settings”
- Legacy `/settings` redirects here

## Operate from the console (UX)

1. Wait for settings to load from `/api/settings`. A load banner appears if the call fails — use **Retry**.
2. **General** — edit daemon name, log level (`debug` / `info` / `warn` / `error`), enable auto-refresh, and set the refresh interval in seconds.
3. **Network** — default bridge name, comma-separated DNS servers, Enable IPv6 checkbox.
4. **Storage** — default pool (dropdown populated from storage pools when available), default format (e.g. qcow2), compression toggle, snapshot retention days.
5. **Security** — enable auth, enable TLS, session timeout seconds, audit logging.
6. **Notifications** — email notifications toggle, webhook URL, and notify-on-start / stop / error checkboxes.
7. **Save Changes** persists via PUT. **Reset** restores the in-form defaults (save again to persist).
8. **Empty / fail:** Error banner on load, or toast on save failure — confirm you are signed in as admin and `zyvor-fabricd` is healthy (`/readyz`).
9. **Success:** “Settings saved successfully” toast; subsequent page loads show the values you set.

## Related pages

- [Dashboard](home.md)
- [Storage Pools](../infrastructure/storage-pools.md)
- [Notifications](../monitoring/notifications.md)
- [Webhooks](../tools/webhooks.md)
- [Getting Started](../../getting-started.md)
- [Page index](../../PAGE_INDEX.md)
