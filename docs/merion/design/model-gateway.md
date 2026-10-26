# Design: a model gateway for agents (routing, failover, cache, spend)

Status: **step 1 built, scoped down; the rest is still a proposal.** What exists: operator-written fallback chains for a use
case's `model` step, with failover, a circuit breaker, journal fields and `GET /v1/model-usage` (see
[MODEL.md](../MODEL.md#fallback-chains-when-the-endpoint-is-down)). Tested against stub providers (17 model-call tests, 561 in the
runtime); not run against a real provider.

**Where it ended up, and why it differs from the plan below.** I recommended extending the fabricd gateway (decision 1, option A).
I built it in the agent runtime instead, because the vault, first-use approvals and the `model.call` journal live there; fabricd's
gateway cannot read Merion's vault, and its endpoints are bound to self-hosted deployments. Consequences: the fabricd gateway is
unchanged, spend budgets use the existing per-user daily allowance rather than a new per-key budget, and there is no cache.

**Not built:** the agent `model_socket` path (the egress broker, not `model_call.rs`), the response cache, per-agent budgets, a
price table and the console page.

## The problem

An agent's brain is one `model_socket` (`base_url`, `model`, `credential`; see [MODELS.md](../MODELS.md)). One endpoint means:

- If it is down or rate-limited, the agent stops. There is no second provider to try.
- Nothing records what an agent or a person spent, or how slow the endpoint was. The `model.call` audit row has sizes and
  status, never cost or latency.
- Nothing caps spend. A looping agent can burn a key.
- Repeated identical prompts are paid for every time.

Products in this space (for example Open Computer's "Inferno Router") sell exactly this: one OpenAI-compatible endpoint with
routing, failover, caching, and per-key usage and quotas.

## What exists today (checked against the code)

- **Merion side.** `agent-runtime/src/model_call.rs` makes the host-side call for a use-case `model` step: vault check, a
  first-use approval keyed by use case, `base_url`, `model` and `credential`, a `model.call` audit row, a sanitised reply.
  The agent `model_socket` goes through the egress broker (`agent-runtime/src/egress.rs`) with the same vault rules.
- **Fabric side.** `backend/zyvor-fabricd/src/api/ai/gateway.rs` is already an OpenAI-compatible gateway
  (`/api/ai/openai/{endpoint}/...`) for **self-hosted deployments**: `fvai_` keys (`keys.rs`), request and token limits per
  global, tenant, project, model and user (`limits.rs` and the `*_rpm`/`*_tpm` helpers), a per-endpoint circuit breaker
  (`circuit.rs`, five failures open it for 30 s), a retry onto another replica before a stream starts
  (`retry_before_stream`), and queue shedding.
- **What it does not do** (from reading `gateway.rs`, `routing.rs`, `circuit.rs`; not exhaustively): route to **external
  providers**, fall back **across endpoints or models**, cache responses, or record **cost**. Its routing is replica
  weighting inside one deployment.

So the router is not a new service. It is the existing gateway plus a few missing pieces, and a Merion agent pointed at it.

## Proposal

### 1. Point Merion at the gateway (no new code in the cell)

A pack's `model_socket.base_url` can already be any OpenAI-compatible URL. The operator adds the fabricd gateway URL as a
vault credential (`host`, `path_prefixes: ["/api/ai/openai/<endpoint>/"]`, key from `env`). The agent keeps using
`ctx.model.chat()`. Merion's guarantees are unchanged: the agent never holds the key, the broker decides reachability on
every call, the first use asks a person. **Step zero is to run this and write down what works**; I have not tried it.

### 2. Fallback chains in the gateway

A new endpoint kind, `chain`: an ordered list of targets, each a local deployment or an **external upstream**
(`base_url`, `model`, vault-style key reference). Rules:

- Try the first healthy target. A target is skipped while its circuit breaker is open (reuse `circuit.rs`).
- Fail over only on connect errors, timeouts, 429 and 5xx, and **only before any response byte is sent** (the same rule
  `retry_before_stream` already enforces). A 4xx other than 429 is the caller's error and is returned as is.
- The reply says which target answered (`x-fabric-upstream`, `x-fabric-failover: 1`). Failover is never silent in the log.
- Chains are written by an operator. An agent or pack cannot create or edit one (same line as
  [agent-tools.md](agent-tools.md)).

### 3. Response cache (off by default)

- Key: SHA-256 of target, model, and the full request body after removing fields that do not change the answer
  (`stream`, `user`). Temperature must be 0 or absent; anything else is never cached.
- Scoped **per tenant and per key**, never shared, so one person's prompt cannot be answered from another's.
- Opt-in per chain with a TTL. Streaming requests are not cached in the first version.
- A cached answer is marked (`x-fabric-cache: hit`) and journaled as a hit.
- Honest limit: the cache holds prompts and replies on the host. It is subject to the same retention setting as other
  host data, and the operator can read it. It must not be enabled for a chain that carries documents the operator is not
  willing to store.

### 4. Usage and spend

- Per `fvai_` key and per Merion session (a header the broker adds, `x-merion-session`): requests, prompt and completion
  tokens (from the upstream `usage` field; absent means "unknown", not zero), latency, target, status, cache hit.
- Cost needs a price table the operator supplies (per model, per million tokens). No built-in prices: they go stale and
  would present guesses as facts.
- A **budget** per key and per Merion agent: at the cap the gateway answers 429 with a clear body. Fail closed; a budget
  that cannot be read blocks the call rather than allowing it.
- Merion's `model.call` audit row gains `target`, `tokens`, `latency_ms`, `cache`. Still never the text or the key.

### 5. Console

A read-only page under the existing AI pages: per key and per agent usage, failover events, cache hit rate.

## What I would refuse to build

- A chain, price table or budget that an agent or pack can write.
- Putting a provider key in the cell, a manifest, or a log to make failover easier.
- Silent fallback to a **different model family** on a use-case `model` step. MODEL.md says a failed step fails the run
  rather than quietly changing the result. A chain is explicit about its models, and the audit row names the one that
  answered. Whether a chain may mix models at all is decision 2 below.
- A shared cache across tenants.

## Plan

| Step | What | Test |
|---|---|---|
| 0 | Point a `model_socket` at the existing gateway; document it | live run against a local vLLM or stub |
| 1 | `chain` endpoints with external upstreams and breaker-based failover | stub upstreams: A down then B answers; 4xx not retried; no failover after first byte |
| 2 | Usage records and budgets | budget hit gives 429; unreadable store blocks |
| 3 | Cache | cross-key miss; temperature > 0 never cached; hit is marked |
| 4 | Merion audit fields and console page | `model_call` tests extended |

Each step is independent and ships alone.

## Decisions for you

1. **Where does it live?** A: extend the fabricd gateway (recommended: keys, limits and breaker already exist). B: a
   separate small service next to the agent runtime (simpler to ship alone, duplicates keys and limits).
2. **May a chain mix model families?** Recommend yes for agents (an operator opts in per chain), **no** for the use-case
   `model` step.
3. **Cache in the first release?** Recommend no: ship steps 0 to 2 first, since the cache is the part with data-retention
   risk.
4. **Prices:** operator-supplied table only, or leave cost out and show tokens only? Recommend operator-supplied.
