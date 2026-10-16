# Action receipts and at-most-once

When an agent does something a person had to approve, Keep now writes down **what was done**, and an agent that retries can no longer do it twice.

## Receipts

Whenever the egress broker performs a request that needed an approval (a credential with `requires_approval`, a secret-shaped request, a write from a tainted session), it records a **receipt** after the upstream answered: the person, agent and session, the credential, the method, the URL **without its query string**, the body's size and SHA-256 (**never the body**), the approval that allowed it, and the status the upstream returned. Receipts are append-only in `receipts/receipts.jsonl` on the host, and each one is also journaled as `keep.action.performed` (ids and status only). The agent's reply carries `receipt_id`.

`GET /v1/receipts` lists them, newest first (`?limit=`, at most 500). A user token sees only its own; the operator sees everyone's, or one person's with `?user_id=`.
A request the upstream answered with a 5xx, and one that never reached it, leave no receipt (they may be retried); a denied or expired approval leaves none either.

## At-most-once for a retrying agent

A retry (a goal step tried again after a failure, a client timeout) would otherwise send the same email twice, each time asking the person again. If the agent sets an **`Idempotency-Key`** header on such a request:

| The request | The broker |
|---|---|
| first time this key is used | holds it for approval as usual, sends it, keeps the receipt |
| the **same** request again (same method, URL with query, body) | does **not** send it and does **not** ask again: it answers with the recorded status and a small JSON body `{"replayed": true, "receipt_id": ..., "original_status": ...}` (headers `x-keep-replayed`, `x-keep-receipt`); journaled as `keep.action.replayed` |
| the same key with a **different** request | refused, 409 |
| the same key while the first is still waiting or running | refused, 409 (never two in flight) |
| an invalid key (empty, spaces, over 128 characters) | refused, 400, before anything is asked or sent |

A key belongs to the **person and the credential**, not the session, because a retry is usually a new session; another person's identical key is independent, and so is the operator's. The key is also forwarded upstream, so an API that supports idempotency keys de-duplicates on its own side as well. The original response body is not kept, so a replay cannot return it; an agent that needs the result of the first call should keep it.

## Limits, said plainly

- This covers the **JSON egress broker** (`ctx.fetch`), not the CONNECT proxy or intercepted TLS.
- It protects only requests that needed approval; other requests are not recorded or deduplicated.
- A receipt is written after the upstream answers; if the runtime dies between the send and the write, that one action has no receipt and a retry with its key would be asked about again. (Forwarding the key upstream is the second line of defence.)
- Recent receipts (the last 10,000) are held in memory for lookups; the file keeps all of them unless the operator sets `ZYVOR_AGENT_RECEIPT_RETENTION_DAYS` (see [retention](../threads/README.md#threads-and-messages); a forgotten receipt no longer answers a repeat of its key). The chat page has a read-only **Done** tab that lists them (when, method, host and path, the answer, the agent, whether you approved it; never the body, its digest, the query or the key), checked in a real browser against a fake host and by 5 more proxy tests. Solvor and the iPhone app have no receipts view yet.
- As with the rest of the host's data, the operator can read the file.

## Tests

`receipts::tests` (5: keys and scoping, first/replay/conflict, a failed attempt does not burn a key, restart and listing, the replay body) and, against a real local upstream and the real approval flow, `egress::ask_tests` (7): a keyed request is sent once and its retry is answered from the receipt without a new approval; a different request under the key is refused and nothing is sent; unkeyed requests are each asked about and recorded; a denied request leaves no receipt and its key can be used again; a bad key is refused first; the same key while the first waits is refused; one person's key never answers for another. `tenancy_tests::receipts_are_listed_per_person_and_carry_no_body` covers the API. Mutation checks (each fails the tests above): treating a different request as a replay, dropping the person from the key, and making lookups miss so a retry would be sent again.
