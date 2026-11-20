# Webhooks

## Purpose

Webhook Configuration — manage outbound webhooks that notify an external endpoint (generic HTTP, Slack, or Discord) when specific VM and backup events occur.

Created webhooks are **enabled by default**. Use **Test** before relying on them in production. Distinct from [Notifications](../monitoring/notifications.md) channel/rules UI — this page is the Tools webhook registry for VM/backup events.

## When to use it

- To wire VM lifecycle events (started, stopped, created, deleted) or backup results (completed, failed) into Slack, Discord, or your own HTTP endpoint
- To verify a webhook endpoint is reachable and correctly configured before relying on it
- To audit which webhooks exist, what events they listen for, and whether they're enabled
- Prefer this page when the job matches the purpose above
- When an external automation needs a simple HTTP callback instead of polling Fabric APIs

## How to get there

- Route / id: `/webhooks`
- Nav: **Tools → Webhooks** (sidebar, command palette, or desktop nav)

## Operate from the console (UX)

1. **Add Webhook** — destination URL, delivery type (generic / Slack / Discord), multi-select events (`vm.started`, `vm.stopped`, `vm.created`, `vm.deleted`, `backup.completed`, `backup.failed`). URL + ≥1 event required.
2. Save posts `POST /api/webhooks` (enabled by default) and refreshes the list.
3. Each row: URL (copy button), type badge, event tags, enabled/disabled indicator.
4. **Test** — `POST /api/webhooks/test`; reports delivered/failed inline.
5. Trash → confirm → `DELETE /api/webhooks/{id}`.
6. Header refresh reloads the list.

Typical flow: Add with a test URL on `https://<host>/…` or your chat webhook → Test → subscribe only the events you need → remove unused hooks. For richer channel/rules/history, also configure Monitoring → Notifications.

**Empty / fail:** Error banner, empty table, or failed toast — confirm you are signed in, `zyvor-fabricd` is healthy (`/readyz` on `http://127.0.0.1:<port>` or `https://<host>`), and any backend this page needs (FluxVM, storage, network) is reachable. See [Admin basics](../../admin-basics.md).

**Success:** Live data loads without error; creates/updates appear in the list or detail view and any confirmation toast clears cleanly.

## Related pages

- [Notifications](../monitoring/notifications.md)
- [Notification Center](notification-center.md)
- [Backups](../operations/backups.md)
- [Event Stream](../monitoring/event-stream.md)
- [API Playground](playground.md)
- [VM Health Check](vm-healthcheck.md)
- [Getting Started](../../getting-started.md)
- [Page index](../../PAGE_INDEX.md)
