# infra-ops — Keep packaged agent

Fabric-facing infrastructure operator. Reads system alerts, VM inventory, and
lifecycle compliance; proposes VM restart / remediation **only after approval**.

> Contrast: [`examples/agent-runtime/ops-agent.ts`](../../agent-runtime/ops-agent.ts)
> is an **in-guest** healthcheck with no Fabric API access.

## Prerequisites

1. Agent runtime + FluxVM template `node22-agent` (or `agent-node`).
2. Host credentials file includes `fabric-api` from
   [`../_fabric/credentials.fabric-api.json`](../_fabric/credentials.fabric-api.json);
   `export FABRIC_API_TOKEN=…` (fabricd JWT).
3. fabricd reachable over **HTTPS** at the host in the credential + policy.

## Demo

```bash
# From repo root — deploy pack, create goal, sample artifact, approval hook
./scripts/merion-pack-demo.sh infra-ops

# Or manually:
cd sdk/agent-runtime && npm ci
npx fabric-agent build ../../examples/merion-agents/infra-ops/agent.ts \
  --out /tmp/infra-ops.bundle.mjs
# Fill deploy.json bundle_base64, then:
./scripts/merionctl create -f examples/merion-agents/infra-ops/deploy.json
```

Session input:

```json
{
  "fabricBase": "https://127.0.0.1:9095",
  "restartVm": "optional-vm-name"
}
```

Goal plan (created by demo script):

1. Read alerts / VMs / compliance → artifact `incident-timeline`
2. Apply fix (`requires_approval`) → blocked until `POST /v1/approvals/{id}`

## Artifacts

- Markdown incident timeline
- `proposed_fix` JSON (restart / remediation sketch)
