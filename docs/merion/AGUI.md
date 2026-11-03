---
sidebar_position: 10
---

# AG-UI: drive a Merion agent from a chat client

`POST /v1/agui` lets a chat client that speaks the [AG-UI protocol](https://github.com/ag-ui-protocol/ag-ui) talk to an agent that runs in a sealed cell. The client posts a
`RunAgentInput` and reads a stream of server-sent events. Merion maps the run onto a session; it adds no new authority. Sessions are started and steered through the same code as
`POST /v1/sessions` and `POST /v1/sessions/{id}/steer`, so scopes, quotas, tenancy and the agent's own policy apply unchanged, and a user token needs the `run` scope.

```bash
curl -N -X POST "$MERION_API/v1/agui" -H "Authorization: Bearer $MERION_TOKEN" -H 'content-type: application/json' -d '{
  "threadId": "t-1", "runId": "r-1",
  "messages": [{"id": "m1", "role": "user", "content": "What is the total due?"}],
  "forwardedProps": {"agent": "model-agent"}
}'
```

`forwardedProps.agent` names a deployed Merion agent. In a CopilotKit or other AG-UI client, point the HTTP agent at this URL and put the agent name in `forwardedProps`.

## A small web chat: `scripts/merion-chat.py`

No chat framework needed to try it. `merion-chat.py` serves one static page and forwards exactly one route to `/v1/agui`:

```bash
MERION_API=http://127.0.0.1:9096 MERION_TOKEN=... ./scripts/merion-chat.py --agent echo-agent      # then open http://127.0.0.1:8787
```

Your token stays in that process; the browser never sees it. The agent is fixed by `--agent` (the page cannot choose another), it listens on `127.0.0.1` only and refuses a
foreign `Host` or `Origin`. Besides the chat run it forwards three thread routes (`GET /threads`, `GET /threads/<id>/messages`, `DELETE /threads/<id>`), and a thread is reachable only if the Merion host itself lists it for this agent (and, with the operator token, for `--user`, default `operator`); nothing else on the host is reachable.
The page is laid out like a messaging app: a chat list (avatar, title, message count, time, a two-click Forget), a header with the agent and its live status (working, waiting for your approval), grouped bubbles with times and ticks, a typing indicator and a glowing composer while the agent works, day separators, dark and light, and on a phone width the list slides over the chat. It reopens the conversation you had after a reload. It renders text with `textContent` only, streams the reply as it arrives, shows an approval request as a notice, and cannot approve it.
The left panel has three tabs. **Chats** is the list above. **Goals** lists the person's goals for this agent with a progress bar and their steps, adds a goal from a title and one step per line (with a "run the steps automatically" switch, so the [goal worker](goals/README.md) runs them), pauses or resumes one, and cancels one (two clicks). **Memory** shows the switch (off by default), adds a note, lists what the agent remembers with a two-click Forget, and shows what the agent *suggested* (marked "suggested by your agent", and "came from unverified content" when it was) with Accept and Reject. The proxy forwards only these routes for them (`/goals`, `/memory`): each reads or changes only what the Merion host itself lists for this person (and, for goals, this agent), lets only named fields through, needs a same-origin request for every write, and learns who the token is from `GET /v1/whoami` (with the operator token it uses `--user`, default `operator`).
It is the lightest client, not a product: one agent, no search, and no attachments. With `scripts/merion-demo-local.sh` (a simulator, not sealed) the example agent is already deployed.

## What happens

- **A thread is a conversation kept on the host** ([threads](threads/README.md)). The first run on a `threadId` creates the thread and starts the agent's session with
  `{"message": <latest user text>, "threadId": ..., "state": ..., "history": [...]}` as its input (`history` is empty for a new thread). Both sides' messages are stored. Later runs on the same thread
  **steer** the session while it runs. When the thread's session has ended, the next run starts a **new session under the same thread**, and `history` carries the most recent earlier messages
  (up to 20, within 16 KiB) so the agent can continue; an agent that ignores `history` simply starts fresh. A thread belongs to one agent (another agent on the same `threadId` is a 409) and to
  one caller: two users who pick the same `threadId` never share a thread or a session. Sending the same `runId` again (a client retry) returns the session it already started and stores nothing twice.
- **Events.** The agent's session events become AG-UI events:

| Merion session event | AG-UI events |
|---|---|
| (start) | `RUN_STARTED` |
| `session.log`, stdout | `TEXT_MESSAGE_START` (once), `TEXT_MESSAGE_CONTENT` per line |
| `session.log`, stderr | `CUSTOM` `keep.log` (not assistant text) |
| `session.waiting`, `session.running` | `CUSTOM` `keep.waiting`, `keep.running` |
| an event the agent emitted with `ctx.emit` | `CUSTOM` `keep.event` with `{kind, data}` |
| `card.propose` from an agent whose manifest set `"card": true` ([cards](card/README.md)) | `CUSTOM` `keep.card` with `{kind, fields, artifact_id}` — a host-cleaned, read-only display, saved as an artifact too; never an approval, decides nothing |
| `approval.requested` | `TEXT_MESSAGE_END` if open, then `CUSTOM` `keep.approval_requested` with the prompt |
| an approval the host holds for the agent (a request that needs a person, e.g. sending mail), while it waits | `CUSTOM` `keep.approval_requested` with `{approval_id, kind, prompt, preview}` (`preview` is the host's rendering of what would be sent, when the credential has one; see [connectors](connectors/README.md)), then `CUSTOM` `keep.approval_decided` with `{approval_id, decision}` (`approved`, `denied` or `expired`) |
| `session.result` | a text message if the result is a string, then `RUN_FINISHED` with `result` |
| `session.failed`, `cancelled`, `expired`, `deleted` | `RUN_ERROR` with a `code` |
| (start, when the thread already has messages) | `MESSAGES_SNAPSHOT` right after `RUN_STARTED`: the stored conversation, including the message just sent |

**Event names.** `CUSTOM` events are named `keep.<x>` by default, so clients built before the rename keep working. A client that sends
`forwardedProps.eventNames: "merion"` gets `merion.<x>` instead (`merion.log`, `merion.approval_requested`, ...). Nothing else about the
stream changes. Handle both prefixes if you can't control which runtime version you talk to. Audit rows written from this version on use
`merion.*` action names; earlier rows keep `keep.*`, so a query over a long window should match both.

Assistant message ids are `<runId>-a<n>` (one per message, so every message of a thread has its own id in the stream); the stored copies in `GET /v1/threads/{id}/messages` are `msg-<seq>`, and the snapshot uses those.

## What it cannot do, on purpose

**A chat client cannot approve or deny anything.** An approval the agent asks for is shown as a `CUSTOM` event so the user knows to look, and it is decided on the user's own device through
`/v1/approvals` (in Solvor: with Touch ID). Nothing sent to this endpoint can decide one.

Not implemented: tool-call events, `STATE_SNAPSHOT` / `STATE_DELTA`, and token-level streaming of a model's reply (text arrives per stdout line or as the final result). A
run reconnects by posting again (the stored conversation is sent as a snapshot); there is no separate resume call, and a run that was cut off is not replayed from where it stopped.

## Prerequisite: agent sessions need IP networking

`/v1/agui` drives an **agent session**, not a one-click use case. Use-case cells talk to the host over vsock and need no IP networking; an agent session reaches the runtime's egress broker over the network, so the host must
be set up for it (a FluxVM tap+netns network with `dnsmasq` installed on the host for the guest DHCP, and a cell image whose NIC is brought up by `systemd-networkd`, as the `node22-agent` template does since it configures DHCP, or `ZYVOR_AGENT_EGRESS_ADVERTISE_HOST` set to an address the guest can reach). On a host that is only set up for use cases the run ends with a `RUN_ERROR` carrying the
runtime's message (`sandbox did not report a default gateway; use tap+netns or set ZYVOR_AGENT_EGRESS_ADVERTISE_HOST`). Try it first with [`echo-agent`](../../examples/merion-agents/echo-agent/README.md), which needs no model or credentials.

## Verified, and what is not

- **Threads over AG-UI** (`demos-ci.sh`, real runtime and stub cell): a second message after the first session ended continues the same thread with a `MESSAGES_SNAPSHOT` of the stored messages (the stream validates against `@ag-ui/core`), `/v1/threads` shows the conversation, and a retried run adds nothing twice. Unit tests cover the message ids, the history budget and the request id.
- The events are validated against the official `@ag-ui/core` 1.0.0 schemas (`EventSchemas`) with ordering checks (RUN_STARTED first, balanced text messages, one terminal event last) in
  `agent-runtime/tests/agui-conformance.mjs`, run by `demos-ci.sh` against a real runtime and a stub cell with a real agent (`model-agent`). Unit tests cover the event mapping, thread isolation and the
  route's authorisation (a user token may POST `/v1/agui`, and nothing else on that path).
- **The chat page and its proxy:** 28 unit tests against a fake host (`agent-runtime/tests/merion-chat-test.py`: token added server-side, agent fixed, events streamed as they arrive, foreign host and origin refused, size and JSON limits, errors passed through, and the thread routes: only this agent's continuable threads listed and only those reachable, nothing deletable or readable unless the host lists it, a host that cannot list is a 502),
  a `demos-ci.sh` check against a real runtime and a stub cell, and the page driven in a real browser (typed text is shown literally, the agent's progress note and reply appear). The Goals and Memory tabs were driven in a real browser too (below). The redesigned page with threads was driven in a real browser (desktop and a 430 px phone width) against a real local runtime with the simulator: a message and its reply appear with times, HTML is shown literally, the chat list shows the conversation, a second message after the first session ended continues the same thread, a reload reopens it with all its messages, and the back button opens the list on a phone. The typing indicator and the glow were checked by forcing the page's working state (the echo agent answers too fast to catch), so they are verified as rendering only. Forgetting a chat and the approval notice were not clicked through in the browser (the proxy's forget route is unit tested).
- **On the real lab host (FluxVM, Merion mode):** a signed `echo-agent` was deployed and a run posted to `/v1/agui`. The stream from a real cold-started cell (about 20 s) was `RUN_STARTED`, two `CUSTOM` events, `TEXT_MESSAGE_START/CONTENT/END` ("You said: ...") and `RUN_FINISHED`, and it validated against the `@ag-ui/core` 1.0.0 schemas with the same ordering checks (7 events valid). Reaching that took three fixes: the cell image now brings its NIC up with DHCP, the host needs `dnsmasq`, and the worker must accept requests without a `Host` header (the FluxVM build on that host forwards none, and Node answers 400). This was one run by hand; it is not part of `demos-ci.sh`, which uses a stub cell.
- **Not tested:** an actual CopilotKit or other AG-UI client. Model-backed agents on a real cell (the `echo-agent` needs no model).
