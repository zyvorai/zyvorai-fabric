# Tutorial 14: How Users Interact with Agents

There are three doors. The web console deploys an agent and starts a
session. The agent-runtime API is where you schedule work, approve a
decision, and read a result. An IDE uses the same runtime through MCP.
This tutorial walks each door with the agents that ship in the repo.

**Level:** Beginner
**Time:** 25 minutes
**Prerequisites:** [Tutorial 13](13-deploy-agent-from-console.md) (console
deploy) and a running `zyvor-fabric-agent-runtime`. A FluxVM template is
required for a real sandbox. Without one, the same calls still work
against the session stand-in described in
[agent-runtime/README.md](../../agent-runtime/README.md#continuous-integration).

```bash
export FABRIC_HOST="https://127.0.0.1:9095"    # zyvor-fabricd, console API
export AGENT_URL="http://127.0.0.1:9096"       # zyvor-fabric-agent-runtime
export AGENT_TOKEN="the ZYVOR_AGENT_API_TOKEN value"
```

`FABRIC_HOST` is the daemon. It proxies only agent list, deploy, and
session start/status. Schedules, webhooks, loops, approvals, delegation,
and MCP are **not** on port 9095. Call those on `AGENT_URL`.

If the runtime was started with `ZYVOR_AGENT_ALLOW_NO_AUTH=1`, omit the
`Authorization` header below. Do not do that on a host other people can
reach.

---

## What You Will Learn

1. Which action belongs to the console, the runtime API, or MCP
2. Run the ops agent from the console and read `healthcheck.completed`
3. Run the hello-go agent and read the source and `hello` it returns
4. Chat with an agent from MCP
5. Admit work with cron, a signed webhook, and a bounded loop
6. Approve a harness question, and delegate from one session to another

---

## The three doors

```text
You
 ├─ Fabric console (:9095)     deploy, list, Run session, hibernate/cancel
 ├─ Agent runtime API (:9096)  schedules, webhooks, loops, approvals, delegate
 └─ MCP  POST /mcp             list agents, list executions, chat
```

| You want to… | Where |
|---|---|
| Upload a bundle and click **Run session** | Console, **Operations → Agents** |
| Watch status, hibernate, cancel, or delete | Console, **Operations → Sessions** |
| Read the program an agent wrote, or its stdout | `GET $AGENT_URL/v1/sessions/{id}/events` |
| Run an agent every night, from a webhook, or N times | `$AGENT_URL` `/v1/schedules`, `/v1/hooks/{id}`, `/v1/loops` |
| Answer `ZYVOR_APPROVAL`, or send the session to another agent | `$AGENT_URL` `/v1/approvals/{id}`, `/v1/sessions/{id}/delegate` |
| Ask an agent from Cursor or Claude Code | `POST $AGENT_URL/mcp` |

Provider keys never appear in any of these responses. The guest receives
a session capability. The host broker adds the real key only after the
request leaves the sandbox.

---

## Use cases

Pick the job. The steps below are the hands-on path for the first cases.
This list is the map: who it is for, which example, which door, and what
you read back.

**In-guest ops.** An operator wants disk, memory, and uptime, and does
not want a model key or any network egress. Deploy
[ops-agent.ts](../../examples/agent-runtime/ops-agent.ts) and click
**Run session**. The journal event `healthcheck.completed` is the
report. Step 1 walks this.

**Coding.** Someone asks for a program and wants the source back, plus
what it printed. Deploy
[hello-go-agent.ts](../../examples/agent-runtime/hello-go-agent.ts) and
start the session on the runtime API. `session.result` carries
`source` and `stdout` (`hello`). The guest image needs `go` on `PATH`.
Step 2 walks this.

**Research.** Someone asks a model a question. The key stays on the
host. Deploy [agent.ts](../../examples/agent-runtime/agent.ts) from the
console with credential `anthropic` and egress host `api.anthropic.com`.
The model's reply is in the event journal. The guest never sees the key.

**Nightly check.** Nobody clicks. `POST /v1/schedules` with a five-field
UTC cron starts the ops agent when it is due. This is the runtime API
only. It is not a console button, and it is not fabricd's VM
`/schedules` routes. Step 4 shows the call.

**External ping.** Another system POSTs a body to `/v1/hooks/{id}` with
`X-Zyvor-Signature`. A bad signature is rejected. A good one admits a
session that still shows up in the console session list. Step 4 shows
the signature.

**Try once.** `POST /v1/loops` with `max_runs: 1` admits one session and
then stops. The other bounds are `max_duration_secs`, `max_cost`, and
`max_no_progress`. A loop must set at least one. Step 4 uses
`max_runs`.

**Human gate.** A `claude`, `codex`, or `gemini` harness prints
`ZYVOR_APPROVAL <question>` and waits. The operator POSTs
`/v1/approvals/{id}` with `approved` or `denied`. The console has no
approval button. Step 5 shows the call.

**Handoff.** A session that is blocked in `ctx.nextSteer()` (see
[waiter.mjs](../../agent-runtime/tests/waiter.mjs)) delegates to the ops
agent. The child keeps its own allowlist and credentials.
`parent_session_id` on the child is the waiting session. Depth stops at
three. Step 5 shows the call.

**From the IDE.** MCP `chat_with_agent` opens a normal session, or
steers one that already exists. It does not grant credentials and does
not widen egress. `list_agents` and `list_executions` are the other two
tools. Step 3 shows the call.

**Many targets.** [fanout.ts](../../examples/agent-runtime/fanout.ts)
starts one session per target through the SDK, each with its own stable
`request_id`, and caps how many run at once. Repeating a `request_id`
returns the original session instead of a second sandbox.

---

## Step 1: Deploy from the console

Build the credential-free ops agent and open **Operations → Agents →
Deploy agent**. Use the bundle you just built. Leave credentials and
egress hosts empty. Template `node22-agent` (Tutorial 11). Name
`ops-healthcheck`.

```bash
cd sdk/agent-runtime
./src/cli.js build ../../examples/agent-runtime/ops-agent.ts \
  --out /tmp/ops-agent.bundle.mjs
```

The same deploy from a shell, talking to the runtime directly:

```bash
./src/cli.js deploy ../../examples/agent-runtime/ops-agent.ts \
  --name ops-healthcheck \
  --template node22-agent \
  --url "$AGENT_URL" \
  --token "$AGENT_TOKEN"
```

Click **Run session** on that row. The console creates a session and
opens **Sessions**. Status moves `creating` → `running` → `completed`.
Provisioning happens after the click returns, so a session that still
says `creating` is not stuck; wait for `running` or `failed`.

The console does not show the event journal. Read it from the runtime:

```bash
SESSION=the-id-from-the-sessions-page
curl -sN --max-time 10 \
  -H "Authorization: Bearer $AGENT_TOKEN" \
  "$AGENT_URL/v1/sessions/$SESSION/events"
```

A finished ops session includes an event named `healthcheck.completed`
whose `data` is the disk, memory, and uptime report.

---

## Step 2: Ask a coding agent for source

`hello-go-agent.ts` writes a Go file, runs it, and returns the source
plus what the program printed. The guest image needs `go` on `PATH`.
No credential and no egress host.

```bash
./src/cli.js deploy ../../examples/agent-runtime/hello-go-agent.ts \
  --name hello-go \
  --template node22-agent \
  --runtime-port 18084 \
  --url "$AGENT_URL" \
  --token "$AGENT_TOKEN"

SESSION=$(curl -s -X POST "$AGENT_URL/v1/sessions" \
  -H "Authorization: Bearer $AGENT_TOKEN" \
  -H "content-type: application/json" \
  -d '{"agent":"hello-go","input":{"task":"print hello"}}' | jq -r .id)

# Poll until status is completed or failed (give up after about a minute).
curl -s -H "Authorization: Bearer $AGENT_TOKEN" \
  "$AGENT_URL/v1/sessions/$SESSION" | jq '{id, status, error}'
```

When `status` is `completed`, the `session.result` event is the return
value. `source` is the Go program. `stdout` is `hello` plus a newline,
because `fmt.Println` adds one. `stderr` is empty when the compile and
the run both succeeded. `path` is the temporary file inside that
session's sandbox; it is gone after the sandbox is deleted.

```bash
curl -sN --max-time 10 \
  -H "Authorization: Bearer $AGENT_TOKEN" \
  "$AGENT_URL/v1/sessions/$SESSION/events"
```

```json
{
  "language": "go",
  "path": "/tmp/zyvor-hello-xxxx/hello.go",
  "source": "package main\n\nimport \"fmt\"\n\nfunc main() {\n\tfmt.Println(\"hello\")\n}\n",
  "stdout": "hello\n",
  "stderr": ""
}
```

Use a different `--runtime-port` for each agent that might run at the
same time. Two sessions of one agent share that port, so wait until the
first is `completed` before starting another of the same agent.

---

## Step 3: Chat from an IDE

Point the IDE's MCP config at the runtime. The request is JSON-RPC on
`POST /mcp`, with the same bearer token as the rest of the API.

```bash
curl -s -X POST "$AGENT_URL/mcp" \
  -H "Authorization: Bearer $AGENT_TOKEN" \
  -H "content-type: application/json" \
  -d '{"jsonrpc":"2.0","id":1,"method":"tools/list"}' | jq .
```

Three tools come back:

| Tool | What the user did |
|---|---|
| `list_agents` | "What agents are deployed?" |
| `list_executions` | "What sessions exist?" Pass `session_id` to read one. |
| `chat_with_agent` | "Talk to this agent." Creates a session, or steers one when `session_id` is set. |

```bash
curl -s -X POST "$AGENT_URL/mcp" \
  -H "Authorization: Bearer $AGENT_TOKEN" \
  -H "content-type: application/json" \
  -d '{"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"chat_with_agent","arguments":{"agent":"ops-healthcheck","message":"check the host"}}}'
```

The tool result is text containing JSON. The `session.id` inside it is
the same kind of session as **Run session** in the console. Chat does
not grant credentials and does not widen that agent's egress allowlist.

---

## Step 4: Let something else start the session

A person does not have to click **Run session**. Three other callers
admit a session for an agent that is already deployed. All three are on
`$AGENT_URL`.

**Cron.** Five fields, UTC. This example is 00:00 on 1 January, so it
will not fire during the tutorial. Delete it when you are done looking.

```bash
curl -s -X POST "$AGENT_URL/v1/schedules" \
  -H "Authorization: Bearer $AGENT_TOKEN" \
  -H "content-type: application/json" \
  -d '{"agent":"ops-healthcheck","cron":"0 0 1 1 *","input":{}}' | jq .
```

**Signed webhook.** The create call returns `secret` once. Later calls
do not need the API token. They need `X-Zyvor-Signature: sha256=<hex>`
over the raw body. A wrong signature is rejected.

```bash
HOOK=$(curl -s -X POST "$AGENT_URL/v1/webhooks" \
  -H "Authorization: Bearer $AGENT_TOKEN" \
  -H "content-type: application/json" \
  -d '{"agent":"ops-healthcheck","input":{}}')
HOOK_ID=$(jq -r .id <<<"$HOOK")
SECRET=$(jq -r .secret <<<"$HOOK")
BODY='{"ping":true}'
SIG=$(printf '%s' "$BODY" | openssl dgst -sha256 -hmac "$SECRET" | awk '{print $2}')

curl -s -X POST "$AGENT_URL/v1/hooks/$HOOK_ID" \
  -H "content-type: application/json" \
  -H "X-Zyvor-Signature: sha256=$SIG" \
  --data "$BODY" | jq .
```

The response is `session_id`. That session shows up in the console
session list like any other.

**Bounded loop.** A loop has to say when to stop. `max_runs: 1` admits
one session and then stops.

```bash
curl -s -X POST "$AGENT_URL/v1/loops" \
  -H "Authorization: Bearer $AGENT_TOKEN" \
  -H "content-type: application/json" \
  -d '{"agent":"ops-healthcheck","input":{},"bounds":{"max_runs":1}}' | jq .
```

---

## Step 5: Approve, or hand the work to another agent

These two are also only on `$AGENT_URL`. The console has no approval
button and no delegate button.

A `claude`, `codex`, or `gemini` agent is a markdown instruction file
plus that CLI in the FluxVM template. Deploy it with `--runtime claude`.
Keys stay on the host. When the CLI prints a line
`ZYVOR_APPROVAL ship it?`, the runtime opens an approval and the
session stays `running`.

```bash
curl -s -H "Authorization: Bearer $AGENT_TOKEN" \
  "$AGENT_URL/v1/approvals" | jq '.items[] | {id, session_id, status, prompt}'

curl -s -X POST "$AGENT_URL/v1/approvals/$APPROVAL_ID" \
  -H "Authorization: Bearer $AGENT_TOKEN" \
  -H "content-type: application/json" \
  -d '{"decision":"approved"}' | jq .
```

`decision` is `approved` or `denied`. The runtime steers that answer
back into the session.

Delegation starts a **different** agent from a session that is
`running`. The child uses its own manifest. It does not inherit the
parent's credentials or egress allowlist. `parent_session_id` on the
child is the session you delegated from. Depth stops at three.

```bash
curl -s -X POST "$AGENT_URL/v1/sessions/$PARENT/delegate" \
  -H "Authorization: Bearer $AGENT_TOKEN" \
  -H "content-type: application/json" \
  -d '{"agent":"ops-healthcheck","input":{}}' | jq '{id, agent, parent_session_id}'
```

The parent has to be waiting, not already finished. The waiter example
(`agent-runtime/tests/waiter.mjs`) blocks in `ctx.nextSteer()` for that
reason.

---

## When something fails

| What you see | What it means |
|---|---|
| Console Agents page is empty and deploy returns 503 | `[agent_runtime]` is missing in `zyvor-fabricd.toml`, or the runtime is not listening on `base_url` |
| `missing or invalid bearer token` from `$AGENT_URL` | Set `Authorization: Bearer $AGENT_TOKEN`, or the runtime was started without `ZYVOR_AGENT_API_TOKEN` |
| Session stays `creating`, then `failed` | The sandbox did not boot, or the guest never opened its port. On FluxVM, the template must be Node 20+ and `--cpus 1` |
| `hello-go` session `failed` | `go` is not on `PATH` in the guest, or another session still holds that agent's `runtime_port` |
| Webhook returns 401 | The signature was not `sha256=` plus HMAC-SHA256 of the **raw** body with the secret from create |
| Delegate returns 409 | The parent session is not `running` |
| Approval returns 409 | The session is not `running`, or that approval was already decided |

---

## What You Accomplished

- Deployed and started an agent from the console, then read its event
  journal from the runtime
- Ran the hello-go agent and got back the source and the word `hello`
- Listed MCP tools and opened a chat session
- Admitted a session from cron, from a signed webhook, and from a
  one-run loop
- Approved a harness question and delegated to an agent that keeps its
  own grants

## Next Steps

1. [agent-runtime/README.md](../../agent-runtime/README.md) — full HTTP
   API, warm pools, and the security model
2. [Tutorial 11](11-agent-runtime-quickstart.md) — build the FluxVM
   template these sessions boot
3. [Tutorial 13](13-deploy-agent-from-console.md) — the console deploy
   dialog, field by field
4. [examples/agent-runtime/README.md](../../examples/agent-runtime/README.md)
   — ops agent, hello-go, warm pool, and fan-out
