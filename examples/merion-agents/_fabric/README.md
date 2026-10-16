# Fabric connector for Keep packaged agents

Shared helpers live here. Each pack (`infra-ops`, `migration-op`, `deploy-op`)
imports `client.ts` and grants the `fabric-api` credential.

## Credential recipe

Merge [`credentials.fabric-api.json`](./credentials.fabric-api.json) into the
host `ZYVOR_AGENT_CREDENTIALS_FILE`. Set `FABRIC_API_TOKEN` to a fabricd JWT
(or admin token). Change `host` / `allowed_ports` to match fabricd.

- Path allowlist: `/api/…`, `/readyz`, `/health`
- Mutating methods (`POST`/`PUT`/`DELETE`) set `requires_approval` so the
  broker opens a send approval before injecting the bearer.

Credential injection requires **HTTPS** to fabricd.

## Policy recipe

[`keep.policy.fabric.yaml`](./merion.policy.fabric.yaml) — allow GET without ask,
mutate always ask. Sign in Keep mode before `PUT /v1/agents/{name}/policy`.

## Client

```ts
import { FabricClient, fabricBaseFromInput } from "../_fabric/client.js";

const fabric = new FabricClient({
  baseUrl: fabricBaseFromInput(ctx.input as Record<string, unknown>),
  fetch: ctx.fetch.bind(ctx),
});
const alerts = await fabric.get("/api/system/alerts");
```
