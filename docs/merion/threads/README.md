# Conversation threads

A thread is a person's conversation with one agent. It outlives any single session: the chat client keeps one thread while
the agent's sessions start and end. Threads live on the **host**, under `threads/<id>/` in the runtime's state directory
(`thread.json` plus an append-only `messages.jsonl`), never inside a cell.

**Status: the store, the HTTP API and the AG-UI endpoint are built and tested.** `POST /v1/agui` keeps every conversation as a thread,
continues it after the agent's session ended, and sends the stored messages to a reconnecting client ([AGUI.md](../AGUI.md)). The chat page
(`scripts/merion-chat.py`) lists, reopens and forgets them and reopens your last conversation after a reload.

| Route | Who | What |
|---|---|---|
| `GET /v1/threads` | user (own) or operator (all) | Newest first. |
| `POST /v1/threads` | user or operator | `{"agent", "title"?, "client_thread_id"?}`. A user token is always its own owner; the operator names `user_id`. The same `client_thread_id` returns the same thread for the same user. |
| `GET /v1/threads/{id}` | owner or operator | The thread. |
| `GET /v1/threads/{id}/messages?after=<seq>&limit=<n>` | owner or operator | Messages with `seq > after`, oldest first, at most 500. |
| `DELETE /v1/threads/{id}` | owner or operator | Forgets the thread and every message on disk. |

**Isolation.** Another user's thread looks like one that does not exist (404). The journal records that a thread was deleted
(thread id, user, message count), never what was said. Tests: `threads::tests` and `tenancy_tests::threads_are_private_to_their_owner`.

**Limits.** 500 threads per user, 32 KiB per message (refused, not cut), 120-character titles, 500 messages per page.

**Retention.** Off by default: nothing is removed unless the operator sets a period, and threads are otherwise kept until the user forgets them. Five settings, each a whole number of days (1 to 3650; unset or 0 keeps everything):

| Variable | What it removes |
|---|---|
| `ZYVOR_AGENT_THREAD_RETENTION_DAYS` | Threads (and their messages) untouched for that long. A thread whose session is still running is never removed. |
| `ZYVOR_AGENT_EVENT_RETENTION_DAYS` | The event log (`events.jsonl`) of sessions that ended that long ago. The session record, artifacts and approvals stay; reading the events of such a session returns none. |
| `ZYVOR_AGENT_RECEIPT_RETENTION_DAYS` | [Action receipts](../receipts/README.md) made that long ago. A forgotten receipt no longer answers a repeat of its idempotency key (that key can be used again), so keep this longer than any agent retries. |
| `ZYVOR_AGENT_SUGGESTION_RETENTION_DAYS` | [Suggestions](../suggestions/README.md) nobody decided for that long, and decided ones that long after the decision. A forgotten dismissal can be proposed again. |
| `ZYVOR_AGENT_MEMORY_PROPOSAL_RETENTION_DAYS` | [Memory](../memory/README.md) proposals an agent made that nobody accepted or rejected for that long. Accepted entries are never removed by this. |

Separately, and with no setting: memory entries whose **expiry** the person set have passed are now deleted from disk (they were only hidden before). An hourly sweep applies all of this (the first one a minute after start). A sweep that removes something is journaled as `keep.retention.sweep` with counts only, never names or text. Tests: `retention::tests`, including that a running session's thread survives, that nothing is removed when no period is set, that a forgotten receipt's key is free again while a recent one still answers, that expired memory (and a stale proposal only when a period is set) is deleted from the file while accepted entries stay, and that the journal holds counts only.
This is deletion from the host's files, not secure erasure: copies in backups, snapshots or the disk's free space are not touched.

**Not built yet.** Search and attachments in the chat page. Like the vault, the operator of the host can read this data; it is not
encrypted to the user.
