# Notifications

## Purpose

Notifications — configure how and when Zyvor Fabric alerts you: the delivery channels (email, Slack, Teams, webhook), the rules that trigger them, and a history of what was actually sent.

## When to use it

- Setting up a Slack, Teams, email, or webhook channel to receive alerts
- Checking whether a rule has fired, how often, and whether delivery succeeded or failed
- Testing a channel to confirm it's wired up correctly before relying on it
- Temporarily disabling a rule or channel without deleting its configuration

## How to get there

- Route / id: `/notifications`
- Nav: **Monitoring → Notifications** (sidebar, command palette, or desktop nav)

## Operate from the console (UX)

The page has three tabs — **Channels**, **Rules**, **History** — loaded together on open.

**Channels** (default tab)
- **Add Channel** opens a form: name, type (Webhook, Slack, Teams, Email), and type-specific fields — a URL for Webhook/Slack/Teams, or SMTP host/port + from/to address for Email.
- **Test** sends a real test notification through that channel (disabled if the channel isn't enabled).
- **Delete** removes a channel after a confirmation prompt.

**Rules**
- Each rule shows enabled/disabled state, its description, up to 3 event-type tags (with a "+N more" overflow), how many times it has triggered, and when it last fired.
- The enable/disable toggle (power icon) flips a rule on or off immediately.
- **Delete** removes a rule after a confirmation prompt.
- **Create Rule** opens a form: name, description, VM event types (created, started, stopped, snapshot taken, hotplug, error, etc.), severity levels, and which channels to notify. Every enabled rule is evaluated in real time against actual VM events — a matching rule sends through each selected channel and updates its trigger count and last-fired time here.

**History**
- A table of everything that's actually been sent: timestamp, rule name, event type (severity-colored), affected VM (if any), channel, and delivery status (`SENT`/`FAILED`).

If the channels list fails to load, an error banner with retry appears; rules and history failing to load independently just show as empty rather than blocking the page.

**Empty / fail:** Error banner, empty table, or failed toast — confirm you are signed in, `zyvor-fabricd` is healthy (`/readyz`), and any backend this page needs (FluxVM, storage, network) is reachable. See [Admin basics](../../admin-basics.md).

**Success:** Live data loads without error; creates/updates appear in the list or detail view and any confirmation toast clears cleanly.

## Related pages

- [Alerts](alerts.md)
- [Logs](logs.md)
- [Audit](audit.md)
- [Timeline](timeline.md)
- [Webhooks](../tools/webhooks.md)
- [Notification Center](../tools/notification-center.md)
- [Getting Started](../../getting-started.md)
- [Page index](../../PAGE_INDEX.md)
