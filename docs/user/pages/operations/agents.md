# Agents

## Purpose

Deploy and list agents via Fabric’s agent-runtime proxy (`/api/agents`); 503 unless `[agent_runtime]` is configured. An agent is either a JavaScript worker or a `claude`, `codex`, or `gemini` harness. Start sessions from a row.

## When to use it

- See which agents are deployed on this fabric
- Start a new session against an agent (opens Sessions)

## How to get there

- Route: `/app/agents`
- Nav: **Operations → Agents**

## What you can do

1. Refresh the agent list (`GET /api/agents` → agent-runtime `/v1/agents`).
2. **Run session** on a row → `POST /api/sessions` then navigate to
   `/app/sessions/{id}`.

## Related

- [Sessions](sessions.md)
- Operator walkthrough: [How users interact](../../../tutorials/14-interacting-with-agents.md)
- Daemon docs: [agent-runtime/README.md](../../../../agent-runtime/README.md)
- Config: `[agent_runtime] base_url` in `configs/zyvor-fabricd.toml`
- [Page index](../../PAGE_INDEX.md)
