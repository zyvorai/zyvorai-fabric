# Credential sources (Vault, files)

> **Status.** All three steps are built: the `file` source, the `vault` source with token, AppRole and Kubernetes auth, the status output and the
> chart values. See [vault/README.md](../vault/README.md#credentials-from-a-file-source). They follow the recommendations below. **Nothing has been
> run against a real Vault**; the behaviour is tested against a mock.

## Why

Every credential secret lives in the runtime's process environment. A descriptor names a variable (`"env": "STRIPE_KEY"`) and the runtime reads it
with `std::env::var` on each use. That is simple, and it has four costs:

- **Custody.** The secret sits in a unit file, an env file or a Kubernetes manifest, so it is copied to wherever the runtime is configured, and it
  is readable in `/proc/<pid>/environ` by anyone who can read the process.
- **Rotation.** Changing a key means editing the env and restarting the runtime, which ends running sessions.
- **No central audit or revocation.** A company that keeps its secrets in Vault, or in Kubernetes Secrets mounted by a CSI driver, cannot see or
  cut off Merion's use of them from the place that owns them.
- **The Helm chart** (`charts/zyvor-merion`) has no good answer: secrets go through `runtime.extraEnv`, which puts them in the pod spec.

This note is about **where the secret comes from**. It does not change who may use it: host, method, path, port, user and approval checks stay
exactly as they are.

## What exists today

Checked against the code (`agent-runtime/src/credentials.rs` unless stated):

- A `CredentialDescriptor` has `env` (the variable name) and `kind`: `provider` (default, injects the env secret), `fabric` (optional
  `FABRIC_AI_API_KEY`) or `oauth-refresh` (client id, client secret and refresh token, each from an env variable, and a token minted and cached in
  memory by a background task).
- Descriptors come from the file named by `ZYVOR_AGENT_CREDENTIALS_FILE`, read **once at startup** (`CredentialVault::load`). No API route writes a
  descriptor. That is worth keeping: a network caller can never change where a secret is read from.
- `authorize_resolve` checks the descriptor's host, method, path prefixes, ports and allowed users **first**, and only then calls `resolve_for`,
  which reads the secret. So a request that fails policy never touches a secret.
- `resolve_for` is **synchronous** and does no I/O beyond `std::env::var`. It has three callers: `egress.rs` (`proxy_inner`, async),
  `browser.rs` (`browser_fill_secret`, async) and `model_call.rs` (`authorize`, a plain `fn`).
- The unwrap ceremony (`ZYVOR_AGENT_VAULT_UNWRAP_REQUIRED=1`) gates injection behind an operator unlock. It sits in front of resolution
  (`egress.rs`, `model_call.rs`) and is independent of where the value comes from.
- `docs/merion/vault/README.md` is honest about the limit, and this note keeps that honesty: the secret is in host process memory, so **the host
  operator can read it**. A credential source changes where the secret is kept and who can rotate or revoke it, not whether the operator can read it
  while Merion is using it.
- `oauth-refresh` already shows the pattern for a secret that expires: a `tokens` map behind `Arc<RwLock<..>>`, a `valid_until`, and a failed
  refresh **fails closed**.
- There is no `vault` binary and no Docker on the machines I test on, so a real Vault cannot be run here.

## Principles

- **Same checks, different source.** Authorisation stays in front; a secret is fetched only after a request passes it.
- **Fail closed.** If a secret cannot be fetched, the request is refused. A cached value may be used until its own TTL runs out and not a moment
  longer; there is no "serve stale during an outage".
- **Never in a log, an error or an audit row.** Errors name the credential and the class of failure ("the secret store refused the request"),
  never a response body, which can echo paths or policy.
- **Operator-only configuration.** A `source` is read from the credentials file at startup like `env` is. Nothing an agent or a user token can call
  may create or change one.
- **`env` keeps working, unchanged.** A descriptor with `env` and no `source` behaves exactly as it does now.

## Proposal

A descriptor may carry a `source` instead of `env`:

```json
{
  "stripe": {
    "host": "api.stripe.com", "header": "authorization", "prefix": "Bearer ",
    "allowed_methods": ["POST"], "requires_approval": ["POST"],
    "source": { "kind": "file", "path": "/run/secrets/stripe-key" }
  },
  "github": {
    "host": "api.github.com", "header": "authorization", "prefix": "Bearer ",
    "source": {
      "kind": "vault", "addr": "https://vault.internal:8200", "mount": "secret",
      "path": "keep/github", "field": "token", "ttl_seconds": 300,
      "auth": { "method": "kubernetes", "role": "keep" }
    }
  }
}
```

**Two source kinds, in this order:**

1. **`file`.** Read the secret from a file, trimmed, cached for `ttl_seconds` (default 60). This one small source covers Vault Agent (which renders
   secrets to files), Kubernetes Secrets mounted as volumes, the CSI Secrets Store driver, systemd `LoadCredential=`, and `sops`/`age` tooling.
   It needs no client, no network and no new dependency, and rotation works by replacing the file.
2. **`vault`.** HashiCorp Vault KV v2 over its HTTP API: `GET /v1/{mount}/data/{path}` with `X-Vault-Token`, the value at `data.data.{field}`.
   Authentication by `token` (from a file or variable, for Vault Agent), `approle` (`role_id` and `secret_id`) or `kubernetes` (the pod's service
   account JWT and a role). A 403 triggers one re-login and one retry, no more.

**Resolution changes shape.** A new async `CredentialVault::resolve_secret` is used for sourced credentials; env, fabric and oauth credentials keep
the synchronous path. `authorize_resolve` becomes async (its three callers are already async or one small step from it: `model_call::authorize`
becomes `async`). The cache is a `RwLock<HashMap<name, {value, valid_until}>>` like the OAuth token map, with **one fetch in flight per
credential**, so a burst of requests after expiry makes one call to the store, not many.

**Guards on `vault`:** `addr` must be `https` (plain `http` only to loopback, the rule the OAuth token endpoint already follows); redirects are
never followed; the response is size-capped and the request has a timeout; `mount`, `path` and `field` are restricted to a plain character set with
no `..`; an optional CA file for private PKI. On `file`: the path must be absolute, no `..`, must be a regular file, and a file readable by group or
others is refused at startup with a clear message (it is a secret).

**Visibility, without values.** `GET /v1/vault/status` (and `merionctl vault-status`) gains, per credential, the source kind, the last successful
fetch time, and the last failure class. It never returns a value, and neither do the journal or the cockpit.

## What I would refuse to build

- **Any API that writes a source**, or lets a session, agent or user token choose a path. That would let a caller point a credential at a secret it
  should not reach.
- **Templated paths from request data** (`keep/users/{user_id}/token`) in this step. It is useful for per-person credentials, but it puts
  untrusted input into a path on the secret store and needs its own design.
- **Serving a stale secret while the store is down.** A revoked secret must stop working when its TTL ends.
- **Reading the store from inside the cell**, or handing a Vault token to a cell. The host fetches; the cell never sees a token or a path.
- **Fetching every secret at startup and holding it forever.** The point is rotation and revocation.
- **A claim that this protects a secret from the host operator.** It does not, and the docs will keep saying so.
- **A bundled Vault client SDK.** The KV v2 read and the three login calls are a handful of HTTP requests; a dependency is not worth it.

## How I would verify it

- Unit tests with a **mock Vault** (an HTTP server in the test): KV v2 read and field extraction, each login method, a 403 causing exactly one
  re-login and retry, TTL expiry, one fetch per burst, a refusal on a 5xx, a timeout, an oversized body, a redirect, a malformed response, and
  `http` to a non-loopback host. Tests for `file`: trimming, rotation after the TTL, a missing file, a group-readable file refused, `..` refused.
- A test through the real broker path: a sourced secret is injected on an allowed request, a request that fails the descriptor's checks **never
  causes a fetch** (the mock records zero calls), and no secret value appears in the journal, an error or the cockpit.
- The existing credential tests and `keep-e2e` run unchanged, which is the test that `env` still works.
- **What I cannot verify here:** a real Vault. The mock proves the requests and the failure handling, not that a particular Vault version or auth
  backend accepts them. Someone with a Vault should run the documented dev-server steps once before this is trusted; I would write them down.

## Size

Three PRs, each reviewable on its own:

1. `source` in the descriptor, the async resolution path, the cache, and the `file` source, with docs. About 500 lines plus tests. The `authorize`
   change in `model_call.rs` is the only edit outside `credentials.rs` and its callers.
2. The `vault` source with token auth.
3. AppRole and Kubernetes auth, the status output, and chart values (mount the service account or a Secret volume; no secrets in `extraEnv`).

`oauth-refresh` credentials (client secret and refresh token) are not in these three; see decision 3.

## Decisions for you

1. **Order.** Recommendation: **`file` first, then `vault`**, because `file` alone covers Kubernetes Secrets, the CSI driver and Vault Agent, and is
   the smaller step to review. The alternative is `vault` first if a native client is what you actually need.
2. **Vault auth methods.** Recommendation: token (for Vault Agent), AppRole, and Kubernetes, in that order. Tell me if you only use one, and I will
   build only that.
3. **OAuth credentials.** Should the client secret and refresh token of `oauth-refresh` credentials also be readable from a source? Recommendation:
   **not in the first three PRs**. Per-person refresh tokens are stored by the runtime itself, so the shared client secret is the only piece that
   would move, and it can wait.
4. **TTL defaults.** Recommendation: 60 s for `file`, 300 s for `vault`, both capped at one hour, and no stale serving. A shorter TTL revokes faster
   and calls the store more.
5. **Startup checks.** Should a sourced credential be fetched once at startup so a wrong path or a denied policy fails at boot, not on the first
   request? Recommendation: **yes, log a warning but do not refuse to start**, since the store may come up after Merion.
6. **Namespaces and other stores.** Do you need Vault Enterprise namespaces (`X-Vault-Namespace`), or AWS or GCP secret managers directly?
   Recommendation: a `namespace` field is cheap and I would include it; cloud managers are better reached through Vault or a mounted file than
   through their SDKs.
