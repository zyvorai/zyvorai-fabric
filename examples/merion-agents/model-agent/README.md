# model-agent

The smallest agent that uses a **model socket**: the manifest names an OpenAI-compatible endpoint, and the agent
calls `ctx.model.chat()`. Point the socket at Qwen, DeepSeek, GLM, a local vLLM or your own service by editing
`model_socket` in `pack.json` (see [MODELS.md](../../../docs/merion/MODELS.md)).

```bash
./scripts/merionctl deploy examples/merion-agents/model-agent
curl -X POST $KEEP_API/v1/sessions -H "Authorization: Bearer $KEEP_TOKEN" -H 'content-type: application/json' \
  -d '{"agent":"model-agent","input":{"question":"What is 2+2?"}}'
```

- The agent never holds the key. The vault credential `llm` (see [MODEL.md](../../../docs/merion/MODEL.md#configure-the-endpoint))
  is added on the host by the egress broker, and only for the host, method, path and port its descriptor allows.
- `model_socket.credential` must also be listed in `credentials`, or deploy is refused.
- CLI agents (`runtime: "codex"` and other OpenAI-compatible CLIs) are pointed at the same endpoint through the
  harness shim, so a declared socket replaces the built-in provider.
- This is an **agent** (a brain that can call tools and take steps). For a one-shot summary of a document,
  use a use-case pack with a `model` step instead ([MODEL.md](../../../docs/merion/MODEL.md)).
