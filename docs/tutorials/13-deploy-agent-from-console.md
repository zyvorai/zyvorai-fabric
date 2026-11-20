# Tutorial 13: Deploy an Agent from the Web Console

Deploy a Fabric agent entirely through the web console — no `curl`, no
`agent-runtime` API token in your shell — using a bundle you build locally
with the `fabric-agent` CLI's `build` subcommand.

**Level:** Beginner
**Time:** 20 minutes
**Prerequisites:** Completed [Tutorial 11](11-agent-runtime-quickstart.md)
Steps 1-2 (a FluxVM Node.js template already built and registered, e.g.
`node22-agent`), Node.js 20+ locally (for the SDK/CLI), a running
`zyvor-fabricd` with `[agent_runtime]` configured and pointed at a live
`zyvor-fabric-agent-runtime` instance, and console access as a user with
admin rights (`POST /api/agents` requires `RequireAdmin`).

---

## What You Will Learn

1. Build a deployable agent bundle locally with `fabric-agent build`, with
   no `agent-runtime` service running
2. Deploy it through the console's **Agents → Deploy agent** dialog instead
   of the CLI's `deploy` subcommand
3. Run a session from the console and watch it on the Sessions page
4. The difference between an agent that needs a credential grant (calls an
   external API) and one that doesn't (stays entirely in-guest)

---

## Step 1: Build a bundle locally

You don't need `agent-runtime` running for this step — `fabric-agent build`
only runs esbuild and writes a file:

```bash
cd sdk/agent-runtime
npm install --no-audit --no-fund

./src/cli.js build ../../examples/agent-runtime/agent.ts \
  --out /tmp/research-agent.bundle.mjs

./src/cli.js build ../../examples/agent-runtime/ops-agent.ts \
  --out /tmp/ops-agent.bundle.mjs
```

Each command prints the output path and bundle size, e.g.
`Built /tmp/research-agent.bundle.mjs (4213 bytes)`. Both bundles are
self-contained ESM — the `@zyvor/fabric-agent` SDK helpers are inlined, same
as `fabric-agent deploy` produces.

---

## Step 2: Open the console

Sign in to the Fabric console and go to **Agents** in the left nav. If no
agents are deployed yet, the empty state has a **Deploy agent** button;
otherwise it's in the page header. Click it to open the Deploy agent dialog.

---

## Step 3: Fill in the form

Every field maps directly to a `fabric-agent deploy` flag:

| Console field | CLI flag | Notes |
|---|---|---|
| Name | `--name` | e.g. `research-agent` |
| Template | `--template` | e.g. `node22-agent` from Tutorial 11 — a FluxVM sandbox template, not one of the VM golden-image templates on the Templates page |
| Bundle file | *(built via `fabric-agent build`)* | upload the `.mjs` from Step 1 |
| Credentials | `--credential` (repeatable) | `anthropic` for the research agent; leave empty for the ops agent |
| Egress allow hosts | `--allow-host` (repeatable) | `api.anthropic.com` for the research agent; leave empty for the ops agent |
| Allow private networks | `--allow-private-network` | leave off unless you know you need it |
| Runtime port | `--runtime-port` | default `8080` |
| TTL (seconds) | `--ttl` | optional |
| Max concurrent sessions | `--max-concurrency` | optional |
| Idle hibernate (seconds) | `--idle-hibernate` | optional |
| Warm pool size | `--warm-pool` | `0`-`64` |

For `research-agent`, set **Credentials** to `anthropic` and **Egress allow
hosts** to `api.anthropic.com` — the same host-side credential descriptor
from [Tutorial 11 Step 3](11-agent-runtime-quickstart.md#step-3-deploy-an-agent)
must already be configured on the `agent-runtime` process (via
`ZYVOR_AGENT_CREDENTIALS_FILE` and `ANTHROPIC_API_KEY`), since the console
only grants the agent access to a credential — it doesn't create one.

For `ops-agent`, leave Credentials and Egress allow hosts empty entirely:
the health check never leaves the sandbox.

> **Bundle size.** Keep the uploaded file well under ~1.5MB. The whole
> request body — including the base64-encoded bundle, which is ~33% larger
> than the raw file — is capped at 2MB
> (`DefaultBodyLimit` in `backend/zyvor-fabricd/src/server.rs`). Both example
> bundles are only a few KB.

Click **Deploy**. On success you'll see a toast and the new agent in the
list with its version and content digest.

---

## Step 4: Run a session and watch it

From the Agents list, click **Run session** next to the agent you just
deployed. This creates a session and takes you to its detail page under
**Sessions**, where you can watch live events stream in and steer, cancel,
hibernate, or resume the session — all documented behavior already covered
by the Sessions page itself.

For `research-agent`, the session result is the raw Anthropic API response.
For `ops-agent`, it's the structured disk/memory/uptime report emitted by
`ctx.emit("healthcheck.completed", ...)`.

---

## Troubleshooting

| Symptom | Fix |
|---------|-----|
| "Select a bundle file to upload" on submit | You closed/cleared the file picker — click **Choose bundle file…** again |
| Deploy fails with an HTTP 413 or a body-too-large style error | Your bundle (base64-encoded) exceeds the 2MB request cap — rebuild without extra bundled dependencies, or raise `DefaultBodyLimit` server-side if you control the deployment |
| Deploy fails with a validation error mentioning `template` | The FluxVM template name doesn't exist yet — complete Tutorial 11 Steps 1-2 first |
| "Failed to deploy agent" toast with a 503-style message | `agent_runtime.base_url` isn't configured in `zyvor-fabricd.toml`, or the `agent-runtime` process isn't running |
| Session never leaves `creating` | Same FluxVM sandbox-boot limitation documented in [Tutorial 11](11-agent-runtime-quickstart.md) — not specific to console-based deploys |

---

## What You Accomplished

- Built two agent bundles locally with `fabric-agent build`, without running
  `agent-runtime`
- Deployed both through the console's Deploy agent dialog, mapping every
  manifest field to its CLI equivalent
- Ran sessions for a credentialed, network-calling agent and a fully
  in-guest, credential-free agent, and watched both on the Sessions page

## Next Steps

1. [Tutorial 14](14-interacting-with-agents.md) — console, API, and MCP: read a result, schedule a run, approve, and delegate
2. Reference: [agent-runtime/README.md](../../agent-runtime/README.md) — full HTTP API, warm pools, hibernation, security model
3. Fan-out and idempotency: [Tutorial 11](11-agent-runtime-quickstart.md) and `examples/agent-runtime/fanout.ts`
4. Both example agents: [examples/agent-runtime/README.md](../../examples/agent-runtime/README.md)
