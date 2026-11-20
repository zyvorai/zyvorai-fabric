# Alerts

## Purpose

Alerts — a live view of currently firing system alerts and the notification rules that generate them, polling for updates automatically.

Read-only here: you cannot acknowledge, mute, or edit rules on this page. Configure delivery on [Notifications](notifications.md).

## When to use it

- Prefer this page when the job matches the purpose above
- To see at a glance whether anything is critical or warning-level right now
- To check what an active alert actually means before deciding whether to act
- To review which alert rules are configured and enabled, and at what threshold they fire
- As a first stop during on-call before opening Timeline/Logs
- To confirm the fleet is clear ("No active alerts") after remediation

## How to get there

- Route / id: `/app/alerts`
- Nav: **Monitoring → Alerts** (sidebar, command palette, or desktop nav)

## Operate from the console (UX)

1. **Active Alerts summary** — total active, critical count, warning count.
2. **Active Alerts list** — cards with severity badge, timestamp, title, message, triggering value when available; left border red/amber/blue by severity. Empty → "No active alerts".
3. **Alert Rules table** — when rules exist: name, condition, threshold, severity, enabled Yes/No (read-only).
4. Auto-refresh every 5 seconds; failed background refresh keeps last data with an amber note. Header refresh forces reload.

Typical flow: read critical cards → follow message to the subsystem (VM, storage, host) → remediate → wait for poll to clear. Wire Slack/email via Notifications if you need push delivery of the same signals.

**Empty / fail:** Error banner, empty table, or failed toast — confirm you are signed in, `zyvor-fabricd` is healthy (`/readyz` on `http://127.0.0.1:<port>` or `https://<host>`), and any backend this page needs (FluxVM, storage, network) is reachable. See [Admin basics](../../admin-basics.md).

**Success:** Live data loads without error; creates/updates appear in the list or detail view and any confirmation toast clears cleanly.

## Related pages

- [Notifications](notifications.md)
- [Timeline](timeline.md)
- [Logs](logs.md)
- [Audit](audit.md)
- [Security Dashboard](../security/security-dashboard.md)
- [Live Metrics](live-metrics.md)
- [Getting Started](../../getting-started.md)
- [Page index](../../PAGE_INDEX.md)
