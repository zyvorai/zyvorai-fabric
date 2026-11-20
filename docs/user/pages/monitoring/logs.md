# Logs

## Purpose

Logs — a searchable, filterable console view of Zyvor Fabric's audit log: every recorded action and event, level-coded and continuously refreshed.

Polls every 5 seconds. **Clear** only clears the local view — underlying history returns on the next poll. Use **Export** for a filtered `.txt` slice.

## When to use it

- Investigating what changed and when — VM creates/deletes, config changes, and other recorded actions
- Filtering down to just `ERROR`/`WARN` entries to find what went wrong
- Searching by keyword or source, or exporting a filtered slice, before opening a support ticket
- During live debugging with Auto-scroll on
- When [Timeline](timeline.md) showed an Error and you need the full message text

## How to get there

- Route / id: `/app/logs`
- Nav: **Monitoring → Logs** (sidebar, command palette, or desktop nav)

## Operate from the console (UX)

1. **Filter box** — matches message and source as you type.
2. **Level dropdown** — `ALL`, `INFO`, `WARN`, `ERROR`, or `DEBUG`.
3. **Auto-scroll** — pins to newest entries as the feed polls every 5 seconds.
4. **Refresh** — manual reload in addition to the automatic poll.
5. **Export** — downloads currently filtered entries as `.txt` (timestamp, level, source, message).
6. **Clear** (trash) — clears the local view only; entries reappear on next refresh/poll.
7. Entries are color-coded by level and show timestamp, level, source, and message.

Typical flow: set Level to ERROR → search by VM name → Export the slice → open [Audit](audit.md) if you need user/IP/resource columns for compliance.

**Empty / fail:** Error banner, empty table, or failed toast — confirm you are signed in, `zyvor-fabricd` is healthy (`/readyz` on `http://127.0.0.1:<port>` or `https://<host>`), and any backend this page needs (FluxVM, storage, network) is reachable. See [Admin basics](../../admin-basics.md).

**Success:** Live data loads without error; creates/updates appear in the list or detail view and any confirmation toast clears cleanly.

## Related pages

- [Audit](audit.md)
- [Timeline](timeline.md)
- [Alerts](alerts.md)
- [Notifications](notifications.md)
- [Event Stream](event-stream.md)
- [Getting Started](../../getting-started.md)
- [Page index](../../PAGE_INDEX.md)
