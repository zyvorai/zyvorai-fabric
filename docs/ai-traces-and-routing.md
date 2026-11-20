# AI traces, decision router and A/B experiments (Beta)

Three additions to [AI Workloads](ai-workloads.md) that close the loop from
serving to improvement: **record** what the gateway serves, **route** each
request to the right model, and **measure** which arm is better.

Everything here sits behind the OpenAI-compatible gateway. Clients keep one
base URL and one API key.

## 1. Trace capture

Capture is opt-in per endpoint.

```bash
fabricctl ai trace enable my-endpoint --token-ids --auto-score finish_stop
```

| Option | Meaning |
|---|---|
| `--sample-rate` | Fraction of requests recorded (default 1.0) |
| `--redact` | Keep only the request SHA-256, sizes and token counts. No prompt or completion text |
| `--token-ids` | Add `return_token_ids`, `logprobs` (and `stream_options.include_usage` for streams) to the upstream request when the client did not set them. vLLM returns the exact token IDs and logprobs the engine used, so training or evaluation never has to re-tokenize text |
| `--auto-score` | `non_empty`, `finish_stop`, `json_valid` or `contains:<text>`. Failed requests score 0 |

Each recorded response carries `x-fabric-trace-id`. A trace holds the request,
completion text, token usage, latency, status, finish reason, and (when the
backend returns them) prompt/completion token IDs and per-token logprobs.
Streaming responses are reassembled from the SSE chunks.

Limits and behavior:

* Bodies larger than `max_body_bytes` (default 256 KiB) are not stored. A
  response larger than four times that keeps status and latency only.
* The newest `max_traces` (default 10 000) are kept. Pruning runs every 100 writes.
* Capture never fails a request; a failed write is logged.
* WebSocket proxying and the `FLUXVM_AI_DRY_RUN` canned responses are not traced.
* Traces can contain user data. Use `--redact`, and restrict `/api/ai/traces`
  (read) and `DELETE /api/ai/traces` (write) with RBAC.

### Score, list, export

```bash
fabricctl ai trace list --endpoint my-endpoint --limit 20
fabricctl ai trace score <trace-id> 1.0 --label good
fabricctl ai trace export --min-score 0.8 --format sft > sft.jsonl   # chat fine-tuning rows
fabricctl ai trace export --format raw > traces.jsonl               # full traces with token IDs
```

## 2. Decision router

A router is attached to a front-door endpoint (the one API keys are scoped
to). Candidates are other `InferenceEndpoint`s. Apply it from JSON:

```json
{
  "endpoint": "assistant",
  "strategy": "cost",
  "min_quality": 0.5,
  "hard_quality": 0.8,
  "hard_prompt_tokens": 2000,
  "candidates": [
    {"name": "large", "endpoint": "qwen-72b", "quality": 0.9, "cost_per_mtok_in": 1.2, "cost_per_mtok_out": 3.6},
    {"name": "small", "endpoint": "qwen-8b",  "quality": 0.6, "cost_per_mtok_in": 0.1, "cost_per_mtok_out": 0.3}
  ]
}
```

```bash
fabricctl ai router apply router.json
fabricctl ai router decide assistant request.json      # dry run, shows the arm and why
```

| Strategy | Picks |
|---|---|
| `rules` | First matching rule (prompt size, tools, keywords in the last user message, model, header), else `default` |
| `cost` | Cheapest candidate with `quality >= min_quality` that fits `max_context`. Requests with tools or at least `hard_prompt_tokens` use `hard_quality`. If nothing clears the bar, the best-quality arm |
| `split` | Weighted A/B split, sticky per `sticky_header` (default `x-user-id`); random when the header is absent |

Prompt size is estimated as characters / 4. If the chosen arm has no ready
backend the next arm (by quality) is used; if none is ready the request gets
503. Responses carry `x-fabric-variant`. `allow_override` lets callers force an
arm with the `x-fabric-variant` request header. Keep it off for experiments.

API keys scoped to a `model` are checked against the front-door endpoint's
deployment, not the arm that serves the request.

## 3. A/B experiments

Put two arms on a `split` router with trace capture and an `--auto-score` (or
feedback) on the front-door endpoint, then:

```bash
fabricctl ai router report assistant --min-samples 30
fabricctl ai router promote assistant small     # 100% of traffic to 'small'
```

The report gives, per arm: requests, errors, mean score, p50/p95 latency,
tokens and estimated cost (from the candidate prices). Each non-control arm is
compared with the control (`control`, else the first candidate) using a Welch
z-test (normal approximation) at 5%. It is only run when both arms have at
least `min_samples` scored traces. The recommendation is `promote`, `keep` or
`keep collecting`; promoting is always an explicit call.

The test assumes independent samples and roughly continuous scores. For 0/1
scores it is the usual two-proportion approximation. Do not stop the moment a
p-value dips below 0.05; fix the sample size first.

## REST

```text
GET/POST   /api/ai/trace-policies            DELETE /api/ai/trace-policies/{endpoint}
GET        /api/ai/traces                    (endpoint, router, variant, model, since, min_score, max_score, scored, limit)
GET        /api/ai/traces/export             (same filters, format=raw|sft)
DELETE     /api/ai/traces                    (needs endpoint, router or since)
GET        /api/ai/traces/{id}
POST       /api/ai/traces/{id}/feedback      {"score": 0..1, "labels": []}
GET/POST   /api/ai/routers                   GET/DELETE /api/ai/routers/{endpoint}
POST       /api/ai/routers/{endpoint}/decide     body = a chat request; no side effects
GET        /api/ai/routers/{endpoint}/report     (since, min_samples)
POST       /api/ai/routers/{endpoint}/promote    {"variant": "…"}
```

Reads need the read role; writes, feedback, purge and promote need write.
