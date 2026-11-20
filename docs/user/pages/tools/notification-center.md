# Notification Center

## Purpose

Notification Center — a live, session-only tray of VM events and system alerts, polled from the server every 10 seconds. It's separate from **Monitoring → Notifications**, which manages persistent delivery channels, rules, and history; this page just surfaces what's firing right now and forgets everything when you reload.

## When to use it

- To catch alerts and warnings as they happen without watching the Monitoring dashboard
- To scan a short-lived list of recent events during troubleshooting
- Not for setting up where alerts get delivered (email/Slack/webhook) — use [Notifications](../monitoring/notifications.md) for that

## How to get there

- Route / id: `/notification-center`
- Nav: **Tools → Notification Center** (sidebar, command palette, or desktop nav)

## Operate from the console (UX)

1. The page polls `GET /api/system/alerts` every 10 seconds; each newly seen alert is added to the feed, tagged **Alert** or **System Warning** based on its severity. It also subscribes to the live `/api/events/stream` feed for **VM Started** / **VM Stopped** events, so all four categories populate from real activity.
2. Category chips at the top show a live count per type.
3. An unread count appears when there are unread notifications; clicking a notification marks it read (dims it).
4. Click the **×** on a notification to dismiss it individually, or **Clear All** to empty the whole feed.
5. If polling fails, an amber banner reports the error; polling keeps retrying every 10 seconds in the background.

5. **Empty / fail:** Check health, auth, and domain dependencies.
6. **Success:** Live data loads; mutations complete without error toasts.

## Related pages

- [API Playground](playground.md)
- [Webhooks](webhooks.md)
- [VM Compare](vm-compare.md)
- [Getting Started](../../getting-started.md)
- [Page index](../../PAGE_INDEX.md)
