# memory-agent

The smallest agent that uses a person's [memory](../../../docs/merion/memory/README.md): no model, no credentials, no network.

- `"memory": true` in `pack.json` is what lets the runtime hand the person's accepted entries to this agent (`ctx.memory.items`) when the person
  has turned memory on. Without it the agent gets none and its proposals are refused.
- Say `remember I prefer window seats` and it calls `ctx.memory.propose(...)`. The suggestion appears in the person's memory list (and inbox) and is
  **not used until they accept it**.
- Ask anything else and it lists what the person's memory holds, marking entries that came from unverified content.

Deploy it like [echo-agent](../echo-agent/README.md): `merionctl deploy examples/merion-agents/memory-agent`. Try it from the chat page
(`scripts/merion-chat.py --agent memory-agent`) with a user token.
