# Vault / authd

## Keep 0.1 — credential authority (host-side)

- Agent never sees raw passwords in tool results: the egress broker injects
  secrets after allowlist checks.
- `CredentialVault::authorize_resolve` only returns a secret when
  `(credential_id, host, method, path, port, user_id)` matches the descriptor
  (`host`, `allowed_methods`, `path_prefixes`, `allowed_ports`, `allowed_users`).
- Surrogate tokens (`zy_sur_…`) can stand in for intercepted TLS hosts; the real
  secret is swapped only at approved egress (see `mitm`).
- **Honesty:** secret material still lives in the **host process environment** /
  descriptor file. The operator of the host can read it. This is **not** a claim
  that the agent cell is unread by the operator.

### Credentials from a file (`source`)

A descriptor can name a file instead of an environment variable, so the secret does not have to live in a unit file, an env file or a pod
spec, and can be rotated without restarting the runtime:

```json
{ "stripe": { "host": "api.stripe.com", "header": "authorization", "prefix": "Bearer ",
              "source": { "kind": "file", "path": "/run/secrets/stripe-key", "ttl_seconds": 60 } } }
```

This covers Vault Agent (which renders secrets to files), Kubernetes Secrets mounted as volumes, the CSI Secrets Store driver and systemd
`LoadCredential=`. Set **either** `env` **or** `source`, not both; `source` is not for `fabric` or `oauth-refresh` credentials.

- **Same checks.** Host, method, path, port, user and approval are checked first and unchanged. A request that fails them **never causes the
  file to be read**.
- **Cached, then re-read.** The value is cached for `ttl_seconds` (default 60, at most 3600) with one read in flight however many requests
  arrive. The TTL is also how long a rotated or revoked secret keeps working.
- **Fails closed.** If the file cannot be read after the cache expires, the request is refused; a cached value is never used past its TTL.
  Errors name the credential and the file, never its contents.
- **The file must be private.** A file readable by group or others (anything but 0400 or 0600) stops startup, unless the source sets
  `allow_loose_permissions: true`; a Kubernetes Secret volume is 0644 unless its `defaultMode` says otherwise. A file that does not exist yet only
  warns at startup, since whatever renders it may start later.
- **What is read.** A regular file (symlinks are followed, as Kubernetes needs), at most 64 KiB, UTF-8, trimmed. An empty file, or a value with
  a control character such as a newline inside it, is refused so it cannot add a second header.
- **The limit.** The secret is still in the runtime's memory while it is used, so the host operator can read it. A source changes where it is
  kept and who can rotate it, not that.

### Credentials from HashiCorp Vault (`source.kind: "vault"`)

The runtime can read one field of a **KV v2** secret directly:

```json
{ "github": { "host": "api.github.com", "header": "authorization", "prefix": "Bearer ",
              "source": { "kind": "vault", "addr": "https://vault.internal:8200", "mount": "secret",
                          "path": "keep/github", "field": "token", "ttl_seconds": 300,
                          "auth": { "method": "token", "token_file": "/vault/secrets/token" } } } }
```

It sends `GET {addr}/v1/{mount}/data/{path}` with `X-Vault-Token` (and `X-Vault-Namespace` if `namespace` is set) and uses the text at
`data.data.{field}`. Everything under "Credentials from a file" applies here too: the authorisation checks run first and a refused request never
reaches Vault, the value is cached and read once per burst, and a failure fails closed with no stale value.

- **Auth: a token you provide.** `token_file` (what a Vault Agent file sink writes; it is **re-read on every fetch**, so a renewed token is picked
  up, and it must be private like any secret file) or `token_env`. Set exactly one.
- **Auth: AppRole.** `{"method": "approle", "role_id": "…", "secret_id_file": "/run/…"}` (or `secret_id_env`; `mount` defaults to `approle`). The
  runtime logs in with `POST /v1/auth/{mount}/login`, keeps the returned token until shortly before its lease ends (30 s early, at most an hour),
  and reads with it. The secret id goes only in the login request body.
- **Auth: Kubernetes.** `{"method": "kubernetes", "role": "keep"}` (`mount` defaults to `kubernetes`, `jwt_file` to
  `/var/run/secrets/kubernetes.io/serviceaccount/token`). The pod's service account token is sent to Vault, which checks it against its role. It is
  read at each login, so a rotated projected token is used, and its file permissions are not checked because Kubernetes makes it world-readable.
- **When Vault refuses a token.** If a token that was **cached** is refused (revoked, expired early), the runtime gets a new one (a new login, or the
  token file read again) and tries once more. A token that was **just issued** and is refused means the policy says no, so it is not retried, and
  nothing loops.
- **Transport.** `addr` must be `https` (plain `http` only to a loopback address). Redirects are never followed. Each request has a timeout
  (`timeout_seconds`, default 10, at most 30) and the answer is capped at 256 KiB. For a private CA, `ca_file` names a PEM certificate to trust.
- **Cache.** `ttl_seconds` defaults to **300** (at most 3600). It is also how long a revoked secret keeps working.
- **What it will not build a URL from.** `mount`, `path` and `namespace` are plain segments (letters, digits, `_ - .`), with no `..`, no empty
  segment and no leading slash, and each is pushed onto the URL as a separate segment, so nothing in the configuration can change the shape of the
  request.
- **At startup** every Vault-sourced secret is read once. A failure only **warns**, since Vault may come up after the runtime; the first request
  tries again and fails closed if it still cannot.
- **Errors** say which class of thing went wrong (could not be reached, refused the token, not found, not a KV v2 secret) and never include the
  answer, which can echo paths or policy.
- **Not yet verified against a real Vault.** The behaviour is tested against a mock; a real server, a real Vault Agent and a Vault Enterprise
  namespace have not been tried.

#### Seeing where a source stands

`GET /v1/vault/status` (operator token only; `merionctl vault-status`) lists each credential that has a `source`: its name, its kind (`file` or
`vault`), whether a value is cached, how many seconds ago the last read worked, whether a read is in flight, and the **class** of the last failure:
`unreachable`, `refused`, `not_found`, `malformed`, `config` or `file`. It never shows a value, a path or an answer. Failures also log a warning
with the same class. `config` usually means a token, secret id or CA file that is missing or unreadable; `refused` means Vault answered 400, 401 or
403.

#### On Kubernetes

The `zyvor-keep` chart (`charts/zyvor-keep`) takes `credentials.descriptors` (rendered to a ConfigMap and mounted read-only, and never holding a
secret), `runtime.extraVolumes` and `runtime.extraVolumeMounts` (to mount the Secret, CSI or Vault Agent files that `file` sources read), and
`serviceAccount.name` (the account whose token Vault's Kubernetes login sends). A changed descriptor restarts the runtime, which reads them once at
startup.

The design is in [design/credential-sources.md](../design/credential-sources.md).

### Refreshing OAuth credentials

A descriptor with `kind: "oauth-refresh"` keeps its client id, client secret and refresh token in host env and injects a short-lived access token that the runtime refreshes in the background; a failed refresh fails closed. See [connectors](../connectors/README.md) (Google Gmail and Calendar).

### Optional software unwrap ceremony

When `ZYVOR_AGENT_VAULT_UNWRAP_REQUIRED=1`:

```bash
merionctl unwrap-token vault          # → token (still host-env secrets)
merionctl unwrap "$TOKEN"             # unlocks inject for ~1h
merionctl vault-status                # names + honesty; never secret values
```

`POST /v1/vault/unwrap-tokens`, `POST /v1/vault/unwrap`, `GET /v1/vault/status`.
Cockpit includes `vault.{secret_backend,unwrap_required,unlocked,honesty}`.
Set `ZYVOR_AGENT_VAULT_USER_HELD=1` to label `secret_backend: user-held-pending`
(still host-env material).

## Keep 0.2 scaffolding (software-test)

```bash
merionctl user-held-challenge         # → {id, nonce} for phone/YubiKey ceremony design
merionctl user-held-complete ID NONCE # → 403 until snp/tdx_launch_verified
```

`POST /v1/vault/user-held/challenge` and `…/complete`. Complete reads FluxVM
`GET /v1/security/capabilities` and is **fail-closed** while verified flags are
false. When a hardware run flips them, complete grants a vault lease via the
key-broker **stub** (wrapped LUKS disk key still not implemented). Vault opens
for real only after the user’s phone or YubiKey unwraps a key onto a
measured/attested guest. No “operator may open for support” on confidential.
Connector scopes like OAuth for CLI tools: `github:read:one-repo`, not a raw PAT.
Payment rails stay pluggable (Stripe Link / virtual card / paste OTP).
