# API versioning

Zyvor Fabric (`zyvor-fabricd`) mounts the same REST router at two URL prefixes:

| Prefix | Status |
|--------|--------|
| `/api/*` | **Canonical** — use in SDK, Terraform, OpenAPI, and new integrations |
| `/api/v1/*` | **Alias** — backward compatible; identical handlers |

Example:

```bash
curl -H "Authorization: Bearer $TOKEN" http://localhost:9095/api/vms
curl -H "Authorization: Bearer $TOKEN" http://localhost:9095/api/v1/vms
```

Both return the same JSON payload.

## Client guidance

| Client | Path |
|--------|------|
| `zyvor-fabric-sdk` | `/api/*` |
| Terraform provider `zyvor-fabricd` | `/api/*` |
| Web UI | `/api/*` |
| OpenAPI (`backend/api-docs/openapi.yaml`) | `/api/*` |

## CI parity check

When a daemon is running with auth enabled:

```bash
FABRIC_USER=admin FABRIC_PASS=... ./scripts/test-api-prefix-parity.sh http://127.0.0.1:9095
```

This is also run from `scripts/ci-api-audit.sh` after the UX audit smoke test.
