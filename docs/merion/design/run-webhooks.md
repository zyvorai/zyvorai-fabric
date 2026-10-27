# Run webhooks

**Status:** built, default off. Tested against a local receiver.

`ZYVOR_AGENT_RUN_WEBHOOK` (https, or http to loopback) with `ZYVOR_AGENT_RUN_WEBHOOK_SECRET`. One `run.finished` event
is POSTed when a session first moves into a final status (completed, failed, cancelled, expired). Headers match the
approval webhook: `x-zyvor-event: run.finished` and `x-zyvor-signature: sha256=<hex HMAC-SHA256 of the body>`.

Body: `session_id`, `agent`, `agent_version`, `status`, `started_at`, `finished_at`. No error text, input, output or user:
a receiver that needs more asks the operator API. The hook sits where the store changes a session's status, so every path
that ends a run sends it, and a session already final does not send a second one.

## Not done

- Other events (run started, goal and step lifecycle, approvals decided). The approval request webhook already exists.
- A durable queue: an event waiting during a restart is lost, and a final failure is logged, not journaled.
- Per-event secrets or several receivers.
