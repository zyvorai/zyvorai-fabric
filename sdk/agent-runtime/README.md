# @zyvor/fabric-agent

Model-independent TypeScript/JavaScript SDK for the Zyvor Fabric Agent Runtime.

```ts
import { defineAgent } from "@zyvor/fabric-agent";

export default defineAgent(async (ctx) => {
  const r = await ctx.fetch("https://api.openai.com/v1/responses", {
    method: "POST",
    credential: "openai",
    headers: { "content-type": "application/json" },
    body: JSON.stringify({ model: "gpt-5", input: ctx.input.prompt })
  });
  return r.json();
});
```

See `../../agent-runtime/README.md` for runtime deployment, credentials, session APIs, hibernation and security details.

## Keep approvals and evidence

The SDK exposes Keep's existing operator APIs. Supply a runtime API token; a user token is
restricted to its own sessions and needs the `approve` scope to decide an approval.
An agent's sandbox token cannot call these operator routes.

```ts
import { Fabric } from "@zyvor/fabric-agent";

const keep = new Fabric({ baseUrl: "https://keep.example", token: process.env.MERION_API_TOKEN });
const session = await keep.agent("research").run({ question: "Investigate this issue" },
  { user_id: "alice", request_id: "ticket-1042" });

const pending = (await keep.approvals.list()).filter(
  (a) => a.session_id === session.id && a.status === "pending",
);
// Show prompt, subject and planned_action to an authorized human before deciding.
// Keep itself never approves an action on the agent's behalf.
if (pending.length) await keep.approvals.decide(pending[0].id, "denied", { comment: "Review first" });

const cockpit = await keep.evidence.cockpit(session.id);
const audit = await keep.evidence.audit({ sessionId: session.id, limit: 100 });
const receipts = await keep.evidence.receipts({ userId: "alice" });
const metering = await keep.usage({ userId: "alice" });
console.log(cockpit, audit.chain.chain_ok, receipts.length, metering.usage.runs);
```

`audit()` reads a bounded recent view, including the chain verification result.
For a full audit export, an operator must separately mint an export token scoped to
`audit`; call `keep.evidence.exportAudit({ exportToken, sessionId })`. The SDK sends
that capability in `X-Merion-Export-Token`, never in a URL. Receipts record brokered
approved actions, not every agent action; see `agent-runtime/src/receipts.rs`.

An operator can create a restricted client token and later revoke its user's
previous tokens:

```ts
const { token } = await keep.identity.mintUserToken("alice", {
  scopes: ["read", "run"], ttlSeconds: 3600,
});
// Deliver token to Alice through your application's secret channel; never log it.
await keep.identity.revokeUserTokens("alice");
```

`identity.whoami()` reports whether the current token belongs to an operator or
a user. Approval decisions by user tokens need `approve` in their scopes, and
the runtime may additionally require a signed phone decision. The SDK does not
bypass that check; the runtime's 403 response is returned as an error.

## Session evidence bundles

`keep-evidence` collects the existing cockpit, a session-scoped audit export and
approved-action receipts into **one private JSON file**. An operator token and a
separately minted audit export token are required:

```bash
export MERION_API_TOKEN='operator-token' MERION_EXPORT_TOKEN='audit-scoped-export-token'
keep-evidence collect --url https://keep.example \
  --session d564e4fe-3658-4b13-9336-45cbbe1a89d1 --out ./incident-1042.json
keep-evidence verify ./incident-1042.json
```

The output is created with mode `0600` and an existing path is never replaced.
Keep the file in an approved private location: cockpit and audit rows can contain
sensitive metadata. The CLI never puts either token in the URL or the bundle.
`verify` requires no network access or token. It checks the local SHA-256 checksum,
format and session references. Someone who changes the file can recompute that
checksum, so it **does not establish authenticity**. The bundle records the
runtime's global audit-chain verdict but cannot independently verify the global
chain from a session-scoped slice. It also is not hardware attestation.

Exports are bounded to the newest 5,000 audit rows for a session and the newest
500 receipts across the runtime. The bundle flags saturated windows and always
labels receipts as potentially incomplete (retention and purges also apply).
Capture runs over several HTTP calls rather than one atomic snapshot; its
`captured_from` and `captured_until` fields show that interval. For long-lived
retention, schedule periodic exports before the windows fill.

The Node API is available from `@zyvor/fabric-agent/evidence`:

```ts
import { collectEvidence, verifyEvidence } from "@zyvor/fabric-agent/evidence";
const bundle = await collectEvidence(keep, { sessionId, exportToken });
const result = verifyEvidence(bundle); // local checksum and shape only
```

## Idempotent fan-out

```ts
const sessions = await fabric.sessions.createMany([
  { agent: "research", input: { target: "a" }, request_id: "scan:a" },
  { agent: "research", input: { target: "b" }, request_id: "scan:b" }
], { concurrency: 2 });
```

`request_id` prevents duplicate sandboxes when a caller retries the same agent start. `createMany` bounds client-side launch concurrency and preserves input/result ordering.

## Warm pools

Deploy with `fabric-agent deploy agent.ts --name research --template node22-agent --warm-pool 4`, then inspect or force reconciliation through the SDK:

```ts
const pool = await fabric.agents.warmPool("research");
console.log(pool.ready, pool.desired);
await fabric.agents.reconcileWarmPool("research");

const session = await fabric.agent("research").run(
  { prompt: "hello" },
  { start_policy: "prefer-warm" },
);
console.log(session.start_mode, session.startup_ms, session.sandbox_released);
```

Standby VMs are single-use. Fabric never returns a sandbox to the pool after user agent code has executed in it. Use `start_policy: "require-warm"` when a caller prefers backpressure over a cold start, or `"cold-only"` to bypass the pool.

## Packs: one-command deploy

`fabric-agent pack` deploys a directory with a `pack.json` (see [docs/merion/PACKS.md](../../docs/merion/PACKS.md)):

```bash
export MERION_POLICY_SEED=$(openssl rand -hex 32)       # your signing key (Keep mode)
fabric-agent pack keys                                # public key to register on the runtime
fabric-agent pack deploy ./my-pack --run              # build, sign, deploy, apply policy, start a session
fabric-agent pack deploy ./my-usecase --test          # declarative use case: save, run on its sample, need 0 CONNECT
fabric-agent pack bundle ./my-pack                    # <name>.merionpack.json for the console's Deploy a pack
```

Signing uses Node's built-in Ed25519, byte-compatible with `keep_sign_policy`, so no Rust toolchain
is needed. `--dry-run` prints the plan without contacting the runtime.
