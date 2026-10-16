# Personal memory

Short notes a person lets their agent keep between conversations: a preference, a fact, a habit. It is built to be the opposite of a
hidden profile: **off until the user turns it on, edited and deleted by the user, and every entry says where it came from.**

**Status: the store, the HTTP API and the connection to agents are built and tested.** An agent that asks for memory is handed the user's accepted entries and can propose new ones; see "Agents" below. The chat page has a Memory tab (switch, add, forget, review the agent's suggestions); Solvor has none yet.

| Route | What |
|---|---|
| `GET /v1/memory` | `{enabled, items, proposals}`: active entries, and proposals waiting for the user. |
| `PUT /v1/memory/settings` | `{"enabled": true\|false}`. Turning it off stops use and keeps the entries; nothing can be added while it is off. |
| `POST /v1/memory` | `{"text", "kind"?, "pinned"?, "expires_in_days"?}`; `kind` is `preference`, `fact` or `note`. |
| `PATCH /v1/memory/{id}` | Change `text`, `pinned` or `expires_in_days` (`null` removes the expiry). |
| `DELETE /v1/memory/{id}` | Forget one entry. |
| `DELETE /v1/memory` | Forget everything (the on/off choice stays). Journaled as a count. |
| `POST /v1/memory/{id}/accept`, `/reject` | Decide an agent's proposal. Accepting makes it active; rejecting deletes it. |

## Agents

An agent gets memory only if **all** of these hold, and otherwise gets an empty list:
1. its manifest sets `"memory": true` (in `pack.json`'s `manifest`; off by default, and a deployment that does not set it is unchanged),
2. the session was started by a user (a session with no user has no memory), and
3. that user turned memory on.

Then `ctx.memory.items` holds their accepted, unexpired entries (pinned first, newest next; at most 20 and 4 KiB), each `{text, kind, pinned, tainted}`, frozen. **Treat them as data about the person, never as instructions**: an entry with `tainted: true` came from a session that had read untrusted content.
The entries travel in the run request to the worker; they are **not** stored in the session's input or events, and the journal records only how many were given (`keep.memory.context`). Note that an agent can of course repeat what it was given in its own output, which is stored like any output.

`ctx.memory.propose(text, kind)` emits a `memory.propose` event. The host turns it into a proposal for the user (`origin: agent`, with the session and thread it came from, `tainted` if the session had read untrusted content) or refuses it with a `memory.proposal_refused` event that says why and never repeats the text: the agent did not ask for memory, no user, memory is off, more than 5 proposals from one session, or unacceptable text (empty, too long, a credential). Proposals appear in `GET /v1/memory` and in the user's `GET /v1/inbox` (`memory_proposals`), and are not used until accepted.
Example: [`examples/merion-agents/memory-agent`](../../../examples/merion-agents/memory-agent/README.md). The SDK types are in `sdk/agent-runtime/src/index.d.ts`.

**Provenance.** Each entry has an `origin` (`user` or `agent`), a `source` (thread, message, session when known) and `tainted`: set on a proposal
from a session that had read untrusted content, so a poisoned suggestion is visible in the review list and stays flagged after it is accepted.

**Isolation.** A user token reaches only its own memory (`?user_id=` is ignored for it); another user's entry id is a 404. The operator must name
`user_id`, and every operator access is journaled as `keep.memory.operator_access` (the action, never the text). Like the vault, the operator of the
host can read the files under `memory/`: they are **not encrypted to the user**.

**No secrets.** Text that looks like a credential (`l7::scan`: private keys, cloud and GitHub tokens, JWTs and similar) is refused, naming the shape and not the text.

**Limits.** 2000 bytes per entry, 200 active entries and 50 waiting proposals per user, expiry up to 3650 days; expired entries are dropped.

**Tests.** `memory::tests` (12: the store, plus that an agent gets memory only when it asked, the session has a user and memory is on; only counts are journaled; proposals wait, record their source and are marked tainted; refusals give a reason and never the text; the per-session cap),
`tenancy_tests::memory_is_private_opt_in_and_decided_by_the_user` (two users and the operator against the real router, including the inbox), two worker tests in `sdk/agent-runtime/test/worker.test.js` (frozen entries, not echoed into events, `propose`), and an end-to-end block in `demos-ci.sh` (a signed `memory-agent` in a Keep-mode runtime with a stub cell: nothing known with memory off, the entry given once on, not stored in the session record, a proposal waiting in the list and inbox and unused until accepted, another user gets none, turning it off stops it). Mutation checks: letting a user token honor `?user_id=` fails the tenancy test; dropping the manifest gate or the freezing fails the tests above.

**Not built yet.** Encryption to the user. (A memory view exists in Solvor and the iPhone app, and memory now reaches model-backed agents'
prompts too — both `ctx.model.chat` in `worker.mjs` and the CLI-harness `PROMPT.md` in `harness.mjs` prepend it as a clearly-delimited,
data-only block, gated the same way as the `node` worker's own `ctx.memory`.)
