# Reference vendor gateway

A small service that shows how a phone vendor puts its **accounts, regions and push** in front of many Keep
shards. It is a **reference**: read it, copy what you need, replace the adapters with your own. It has no
dependencies (Node 20+), and it is not production-hardened (see the end).

```
phone ── vendor login (JWT) ──> gateway ──> the user's shard (Keep host)
   ▲                              │  mints a short-lived, scoped user token per user
   └── push ◄── /relay/push ◄─────┘  (a shard's signed approval message → your push provider)
```

The gateway is thin **on purpose**. Isolation is the shard's job: a user token can reach only that user's sessions,
approvals, artifacts and audit rows ([TENANCY.md](../../docs/merion/TENANCY.md)), so a bug in the gateway cannot
widen what a user can reach beyond what the shard allows that token. Keep the operator token here and nowhere else.

## What it does

| Job | How |
|---|---|
| **Identity** | Verifies a pinned OIDC issuer, audience and JWKS (RS256/ES256), or a local HS256 reference token. Needs `sub`; optional `region`, `acr` |
| **User ids** | Maps an account id to the runtime's `[a-z0-9._-]{1,32}`: kept if it fits, otherwise a stable hash. Two accounts never share one |
| **Placement** | On first sight, puts the user on **one shard in their region** (rendezvous hashing) and remembers it in `stateFile`. A user never moves: Keep does not migrate cells between hosts |
| **Tokens** | Mints a user token per (shard, user) with the shard's operator token, caches it for 15 minutes, re-mints once on a 401 |
| **Proxy** | `/api/<x>` → shard `/v1/<x>` for `sessions`, `approvals`, `artifacts`, `audit`, `demos`, `usage`, `inbox` only. Uploads and event streams pass through |
| **Devices** | `POST /api/devices` enrols a phone's public key for the caller, **only with a strong login** (`acr: "strong"`). Enrolling uses the operator token, so a stolen user token can never add a key |
| **Push relay** | `POST /relay/push` takes the shard's signed message, checks the HMAC, and hands it to the adapter for the device's `push.kind` |
| **Limits** | A per-user rate limit (429), and a 503 when a region has no shard |
| **Admin** | `GET /admin/shards` (never shows tokens), `GET /admin/usage?user_id=` (rolls up the shard's `/v1/usage`), behind `x-admin-key` |

## Run it

```json
{
  "oidc": {
    "issuer": "https://login.example/realms/zyvor",
    "audience": "keep-phone",
    "jwksUrl": "https://login.example/realms/zyvor/protocol/openid-connect/certs"
  },
  "relaySecret": "…", "adminKey": "…",
  "defaultRegion": "eu", "port": 8443, "stateFile": "/var/lib/gateway/users.json",
  "ratePerMinute": 120,
  "pushWebhookAllowedHosts": ["push.vendor.example"],
  "shards": [
    { "id": "eu-1", "region": "eu", "url": "http://10.0.1.5:9096", "token": "<that shard's operator token>" },
    { "id": "eu-2", "region": "eu", "url": "http://10.0.1.6:9096", "token": "…" },
    { "id": "sg-1", "region": "sg", "url": "http://10.1.1.5:9096", "token": "…" }
  ]
}
```

```bash
node src/main.js gateway.json
npm test          # local HTTP fixtures; no external network
```

Configure exactly one of `oidc` or `jwtSecret`. Use an access token issued for
the gateway's dedicated audience, with `iss`, `aud`, `sub` and `exp` claims. A
multi-audience token also needs `azp` equal to the configured audience. OIDC
mode accepts RS256 and ES256 only. It loads public keys from the configured
`jwksUrl`, caches them for five minutes, and refreshes when an unknown key id
arrives (unknown-key refreshes are limited to one every five seconds). Token
headers cannot redirect key retrieval; JWKS redirects are refused.
The JWKS URL must use HTTPS. For local tests only, set `allowLoopbackHttp: true`
under `oidc` and use a loopback HTTP URL. Never use that setting with a remote
HTTP identity provider. Rotate keys with a new `kid` and keep old keys published
until their issued tokens expire. If the provider is unreachable after the cache
expires, login fails closed.

The existing `jwtSecret` HS256 mode remains for the local reference setup. It
does not pin an issuer or audience, so use `oidc` for deployments connected to
an identity provider. The gateway still trusts the token's `acr: "strong"` for
phone enrolment: configure and verify that claim's meaning in your provider.

On each shard set `ZYVOR_AGENT_PUSH_RELAYS='{"webhook":"https://gateway.example/relay/push"}'` and
`ZYVOR_AGENT_PUSH_RELAY_SECRET` to the gateway's `relaySecret`, and give every shard the same
`ZYVOR_AGENT_USER_TOKEN_SECRET` if any token should work across shards.

## Push adapters

`webhook` POSTs to the device's enrolled URL only when its HTTPS hostname exactly matches
`pushWebhookAllowedHosts`. Set this to the hostnames of **your own push delivery service**; with an empty or
missing list, webhook delivery is disabled. Every DNS answer must be a public address, the chosen address is
pinned to the TLS request, certificate verification uses the original hostname, and redirects are refused. The
adapter limits notifications to 64 KiB and waits at most five seconds. `log` writes the notification to stdout.
`fcm`, `mipush`, `hms`, `oppo` and `vivo` are **placeholders that throw**: each needs the vendor's own push
credentials and SDK, which do not belong in this repository. Implement `send(device, message)` for the ones you
use. A 5xx from the relay makes the shard retry twice and then journal the failure.

## Not production-hardened

- One process, state in a JSON file. Run several behind a load balancer only after moving `stateFile` and the rate
  limiter to a shared store.
- No TLS termination, no request logging, no abuse controls beyond the per-user rate limit.
- OIDC verification is pinned to one issuer/audience; account lifecycle, MFA
  policy, key provisioning and user deprovisioning remain the vendor's job.
- Placement is sticky and never rebalances. Adding a shard only takes **new** users.
- Uploads are buffered in memory (up to 70 MiB), fine for documents, not for large media.
- Oversized requests return 413. The gateway refuses shard redirects so scoped user and operator tokens cannot
  be forwarded to a redirect target; configure each shard with its final URL.
