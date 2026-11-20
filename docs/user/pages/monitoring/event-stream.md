# Event Stream

## Purpose

Event Stream — a live, scrolling log of VM lifecycle events (create, start, stop, delete, and similar) pushed over an authenticated SSE connection as they happen. There's no history — you only see events that occur while the page is open.

"Waiting for events…" with a green Connected indicator is normal on a quiet fleet.

## When to use it

- To watch VM lifecycle activity happen in real time, e.g. while running a script that creates or tears down several VMs
- To confirm an action you just took (start, stop, delete) actually registered
- To catch error/warning-level events as they occur without polling a log page
- During bulk ops or migrations when you want a live side channel
- When [Logs](logs.md) is too noisy and you only care about lifecycle as it fires

## How to get there

- Route / id: `/event-stream`
- Nav: **Monitoring → Event Stream** (sidebar, command palette, or desktop nav)

## Operate from the console (UX)

1. **Connection indicator** — Connected (green) or Reconnecting… (amber, pulsing) for the SSE link.
2. **Pause / Resume** — pause freezes the view and drops incoming events (does not queue); resume appends new events from that point.
3. **Clear** — empties the on-screen list (does not affect the connection).
4. **Level filter** — All, Info, Warning, Error, or Debug (level inferred from event type, e.g. names containing fail/error → Error).
5. Each line: time, level, source VM name, message. Keeps the most recent 500 events and auto-scrolls unless paused.

Typical flow: open stream → Connected → perform create/start elsewhere → watch the matching lines → Pause to inspect → Clear when starting a new test. For historical trails use Logs / Audit / Timeline.

**Empty / fail:** Error banner, empty table, or failed toast — confirm you are signed in, `zyvor-fabricd` is healthy (`/readyz` on `http://127.0.0.1:<port>` or `https://<host>`), and any backend this page needs (FluxVM, storage, network) is reachable. See [Admin basics](../../admin-basics.md).

**Success:** Live data loads without error; creates/updates appear in the list or detail view and any confirmation toast clears cleanly.

## Related pages

- [Logs](logs.md)
- [Timeline](timeline.md)
- [Audit](audit.md)
- [Bulk Operations](../operations/bulk-operations.md)
- [Virtual Machines](../core/vms.md)
- [Notifications](notifications.md)
- [Getting Started](../../getting-started.md)
- [Page index](../../PAGE_INDEX.md)
