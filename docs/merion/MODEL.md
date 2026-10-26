---
sidebar_position: 7
---

# Model-assisted use cases

Every built-in use case is extractive: no model is called, which is why the cockpit can honestly
show `0` outbound connections. A use case can opt in to one **model step** that adds a short
generated summary. That step sends data off the machine, so it is built to be visible, gated, and
recorded.

```json
{
  "id": "invoice-brief",
  "title": "Invoice brief",
  "accepts": ["pdf"],
  "extract": "pdftotext",
  "summary": [{ "kind": "stats" }],
  "model": {
    "credential": "llm",
    "base_url": "https://api.example.com/v1",
    "model": "small-1",
    "instruction": "List the invoices and the total due."
  }
}
```

## What happens

1. The cell extracts the text, as always, and finishes. **The cell has no network path for the model
   step**, so its connection count stays `0` and the freeze-on-connect rule is unchanged.
2. The **host** sends the extracted text (cut to `max_input_chars`, default 24 000) to
   `<base_url>/chat/completions` (OpenAI-compatible), with your `instruction` as the system message
   and a fixed preamble telling the model the document is untrusted data.
3. The reply is added to the artifact under **Model summary**, marked as not extractive.

## What controls it

| Control | How |
|---|---|
| **The vault decides reachability.** | `credential` must exist in the operator's credentials file, and its host, method (`POST`), path prefix and port limits must allow the call. A pack can name an endpoint; only the operator can make it reachable. A refusal is a 403 before any approval. |
| **First use needs a person.** | The first time a use case uses an endpoint, the run waits for an out-of-band approval (webhook or phone, or the run's cockpit in the console; **History → Approvals** lists it with a link), never the chat. It says the host, the model and how many bytes leave. Approve once and that combination is remembered. |
| **A change is a new question.** | The approval is keyed by use case, `base_url`, `model` and `credential`. Change any of them and it asks again. Rewording the `instruction` does not, since it does not change where the text goes. |
| **You can take it back.** | `merionctl grants list` and `merionctl grants revoke <key>` (`GET/DELETE /v1/model-grants`). |
| **Every call is audited.** | A `model.call` row per call: host, model, credential name, request size and SHA-256, reply size, status. Never the text, never the key. |
| **No key in a pack.** | A pack has nowhere to put one (unknown fields are refused). The key comes from the host environment through the vault and is added to the request on the host. |
| **No redirects.** | The credential is never sent to a second host. |
| **Only TLS.** | `base_url` must be `https`, or `http` for a loopback address (a local model). No credentials, query or fragment in the URL. |
| **The reply is untrusted.** | Control characters and markup are removed and length is capped (16 000 characters, 600 per line) before it reaches an artifact. |

If anything fails (vault refusal, denied or expired approval, endpoint error), the **run fails**;
Merion does not quietly fall back to an extractive-only result.

## Configure the endpoint

On the runtime, add the credential (`ZYVOR_AGENT_CREDENTIALS_FILE`) and set the key in the host
environment:

```json
{
  "llm": {
    "host": "api.example.com",
    "header": "authorization",
    "prefix": "Bearer ",
    "env": "EXAMPLE_API_KEY",
    "allowed_methods": ["POST"],
    "path_prefixes": ["/v1/chat/completions"]
  }
}
```

For a local model on `http://127.0.0.1:8080/v1`, set `"host": "127.0.0.1"` and
`"allowed_ports": [8080]`.

## What this does not claim

- The extracted text **does** leave the machine, for the host you approved. The cockpit and the run
  response say so: `0 CONNECT from the cell · extracted text sent to <host>`.
- The secret lives in the host environment, so the host operator can read it (see the honest limit in
  [the overview](README.md)). The vault protects it from the agent and the pack, not from the operator.
- A model can be steered by text in the document. It has no tools here: its reply becomes text in an
  artifact and nothing else, and that text is sanitised. Treat the summary as a draft.
- The endpoint sees the text. Merion does not vouch for the provider; pick one you trust with that file.

## Fallback chains (when the endpoint is down)

A pack may add `"fallback_chain": "main"` to its `model` block. The name refers to a chain **the operator wrote** in a file
(`ZYVOR_AGENT_MODEL_CHAINS_FILE`); a pack can name a chain but cannot define, edit or see one.

```json
{ "chains": { "main": { "targets": [
  { "credential": "backup-llm", "base_url": "https://api.other.example/v1", "model": "small-2" }
] } } }
```

- Up to 4 fallbacks, tried in order, **only** when the previous endpoint could not be reached, answered 429, or answered 5xx.
  A 4xx (a bad request) is not retried elsewhere.
- Each fallback is checked like the first: the vault must allow its host, method, path and port, and its **first use asks a
  person** (a different endpoint is a different question). A fallback that is refused or denied is skipped; the run then fails
  with the last error rather than doing anything quietly.
- After 3 failures in a row an endpoint is skipped for 30 seconds, so a dead provider does not slow every run.
- A chain that is named but missing, unreadable or invalid **fails the call** before anything is sent.
- The pack's instruction, caps and framing apply to every endpoint. The journal records each attempt (`model.call` with
  `target_index`, `latency_ms`, and `prompt_tokens`/`completion_tokens` when the provider reports them) and a `model.failover`
  row naming the endpoint that answered. Never the text or the key.
- `GET /v1/model-usage` (operator only) sums the journal per endpoint: calls, failures, tokens, average latency, and how many
  calls reported no tokens (`tokens_unreported`). It has no cost figure; Merion does not guess prices.

### The agent's `model_socket`

A manifest's `model_socket` may also carry `"fallback_chain": "main"`, naming the same operator-written chain. The egress broker
then retries the agent's `POST <base_url>/chat/completions` against the chain when the socket's endpoint cannot be reached or
answers 429 or 5xx, changing only the URL, the credential and the JSON `model`. Each attempt takes the full broker path, so the
cell's egress policy and the vault still decide; a chain endpoint they refuse is skipped, and the chain's credential needs no
grant in the manifest (the operator's file is the grant, for that one request). The breaker is shared with the use-case path.
Journal: a `model.failover` row with `"path": "model_socket"`, and an `egress.http` row for each failed attempt. Counted in
`GET /v1/model-usage` (see below). Tested against stub endpoints only.

### Prices and budgets

`ZYVOR_AGENT_MODEL_PRICES_FILE` is an operator-written JSON file. Merion ships no prices.

```json
{ "currency": "USD",
  "prices": { "small-2": { "input_per_mtok": 0.5, "output_per_mtok": 1.5 } },
  "default_budget": { "max_calls_per_day": 500 },
  "budgets": { "mail-agent": { "max_calls_per_day": 100, "max_cost_per_day": 2.0 } } }
```

- `GET /v1/model-usage` then adds `cost` and `calls_unpriced` per endpoint. Cost covers only calls that reported tokens on a model
  with a price; the rest are counted as unpriced, never as free.
- A **budget** is a rolling 24 hours per agent, over both the use-case `model` step and the agent's `model_socket`. Over it, the
  call is refused with 429 before anything is sent. Only calls that succeeded count. `max_cost_per_day` counts only priced calls
  with reported tokens. A prices file that cannot be read refuses the call rather than lifting the limit.
- The `model_socket` path now writes the same `model.call` rows as the use-case step (host, model, tokens, latency, status).

Not covered yet: a response cache. The per-user daily call allowance
(`ZYVOR_AGENT_USER_MAX_MODEL_CALLS_PER_DAY`) still applies; per-agent budgets do not exist.

