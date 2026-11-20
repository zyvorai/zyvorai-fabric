# Agent Runtime example

`agent.ts` shows a model-provider request using `ctx.fetch(..., { credential: "anthropic" })`. The Anthropic key is injected by the host-side Fabric egress broker and never exists in the sandbox.

`client.ts` starts a durable session and consumes the resumable event stream.

Deploy:

```bash
cd sdk/agent-runtime
npm install
./src/cli.js deploy ../../examples/agent-runtime/agent.ts \
  --name research-agent \
  --template node22-agent \
  --credential anthropic \
  --allow-host api.anthropic.com
```

`fanout.ts` shows retry-safe, bounded-concurrency multi-session orchestration with one stable `request_id` per target.

## Ops/automation example

`ops-agent.ts` is a contrasting example that needs no credential grant and no `egress_allow_hosts` entry: it's a plain Node.js health check (disk/memory/load via `node:fs`/`node:os`, `uptime` via `node:child_process`) that never leaves the sandbox. Deploy it the same way, dropping the credential/allow-host flags:

```bash
./src/cli.js deploy ../../examples/agent-runtime/ops-agent.ts \
  --name ops-healthcheck \
  --template node22-agent
```

To deploy either example from the web console instead of the CLI, build a bundle first with `fabric-agent build` and upload it via **Agents → Deploy agent** — see [Tutorial 13](../../docs/tutorials/13-deploy-agent-from-console.md).

## Warm-pool example

Deploy the agent with `--warm-pool 4`, then run `warm-pool.ts` to inspect/reconcile the pool and start a TTL-bounded session while printing its `start_mode` and `startup_ms`.

## Hello Go

`hello-go-agent.ts` writes a small Go program, runs it with `go run`, and returns both the source and the program output. It needs Go on `PATH` in the guest and no credential grant. The agent-runtime session job deploys it and checks that stdout is `hello`.

```bash
./src/cli.js deploy ../../examples/agent-runtime/hello-go-agent.ts \
  --name hello-go \
  --template node22-agent \
  --runtime-port 18084
```
