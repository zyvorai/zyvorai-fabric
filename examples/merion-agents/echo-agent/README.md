# echo-agent

An agent with no model, no credentials and no network that repeats what it is told. It exists to try the [AG-UI endpoint](../../../docs/merion/AGUI.md) end to end.

```bash
export KEEP_POLICY_SEED=$(cat ~/.config/zyvor/keep-signer.seed)     # agents are signed in Keep mode
./scripts/merionctl deploy examples/merion-agents/echo-agent
curl -N -X POST "$KEEP_API/v1/agui" -H "Authorization: Bearer $KEEP_TOKEN" -H 'content-type: application/json' -d '{
  "threadId": "t-1", "runId": "r-1",
  "messages": [{"id": "m1", "role": "user", "content": "hello"}],
  "forwardedProps": {"agent": "echo-agent"}
}'
```

You get `RUN_STARTED`, a `CUSTOM` event for what the agent emitted, an assistant text message ("You said: hello") and `RUN_FINISHED`. The agent runs in a sealed cell like any other.
