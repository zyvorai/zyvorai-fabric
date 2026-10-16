---
sidebar_position: 10
---

# Choosing the model

Keep does not care which model answers. Two places take one, and they work differently.

| | Use-case `model` step | Agent `model_socket` |
|---|---|---|
| What it is | One call that adds a generated summary to a document use case | The brain of an agent: `ctx.model.chat()` in the cell, or an OpenAI-compatible CLI |
| Who calls | The **host**, after the cell finished | The **agent in the cell**, through the egress broker |
| The cell's network | None (0 CONNECT) | Only what the broker allows for that endpoint |
| Approval | First use per use case, endpoint and model | The egress policy of the agent (`egress_mode`, `ask`, credentials with `requires_approval`) |
| Docs | [MODEL.md](MODEL.md) | this page |

Both use an **OpenAI-compatible** `chat/completions` endpoint and a **vault credential**: the API key is read from the
host environment and added by the host, so neither a pack nor an agent ever holds it.

## Agent model socket

```json
"manifest": {
  "credentials": ["llm"],
  "model_socket": { "base_url": "https://api.example.com/v1", "model": "small-1", "credential": "llm" }
}
```

- `base_url` keeps its own path (for example `/v1`), like an OpenAI SDK's `base_url`. Keep appends `/chat/completions`.
- `credential` must also be in `credentials`. Leave it out for a local server that needs no key.
- Deploy checks the URL (http or https, no key or query in it) and the credential. Whether the endpoint may actually
  be reached is decided by the egress broker on **every call**, from the vault descriptor (host, method, path, port)
  and the agent's egress policy.

In the agent (Node runtime):

```ts
const reply = await ctx.model.chat([{ role: "user", content: "…" }], { maxTokens: 300 });
reply.text;   // the answer
ctx.model.configured;   // false if the manifest declares no socket (chat then throws)
```

For CLI agents (`runtime: "codex"` and other OpenAI-compatible CLIs) the harness points `OPENAI_BASE_URL` at a
loopback shim that forwards to the socket through the broker with its credential.

## Vault credentials for common providers

Add a descriptor per provider to the credentials file (`ZYVOR_AGENT_CREDENTIALS_FILE`) and put the key in the named
environment variable on the runtime host. **Verify each base URL against the provider's current documentation before
you rely on it**: providers change endpoints, and these are examples, not endorsements.

```json
{
  "dashscope": { "host": "dashscope.aliyuncs.com", "header": "authorization", "prefix": "Bearer ", "env": "DASHSCOPE_API_KEY",
                 "allowed_methods": ["POST"], "path_prefixes": ["/compatible-mode/v1/"] },
  "deepseek":  { "host": "api.deepseek.com",        "header": "authorization", "prefix": "Bearer ", "env": "DEEPSEEK_API_KEY",
                 "allowed_methods": ["POST"] },
  "zhipu":     { "host": "open.bigmodel.cn",        "header": "authorization", "prefix": "Bearer ", "env": "ZHIPU_API_KEY",
                 "allowed_methods": ["POST"], "path_prefixes": ["/api/paas/v4/"] }
}
```

| Provider (OpenAI-compatible mode) | `base_url` to check |
|---|---|
| Alibaba Cloud Model Studio (Qwen) | `https://dashscope.aliyuncs.com/compatible-mode/v1` |
| DeepSeek | `https://api.deepseek.com/v1` |
| Zhipu (GLM) | `https://open.bigmodel.cn/api/paas/v4` |
| A local server (vLLM, llama.cpp, Ollama) | `http://127.0.0.1:8000/v1`, with `"allowed_ports": [8000]` on the credential |
| A vendor's own model service | whatever base URL it exposes, on a host you list in the descriptor |

A vendor running its own model in the same data center lists that host in the descriptor and keeps the traffic inside
its network; nothing in Keep sends a prompt anywhere the vault does not allow.

## Honest limits

- Only **OpenAI-compatible chat completions** are supported. A provider with its own protocol needs a small gateway in
  front of it, or an agent that speaks it through `ctx.fetch`.
- The Claude, Codex and Gemini CLIs are baked into templates you build; Keep does not ship them.
- Tool use, streaming and vision are not wrapped by `ctx.model.chat()`; call `ctx.fetch` for those.
- Prompts and replies pass through whatever endpoint you allow. Keep does not vouch for a provider.
