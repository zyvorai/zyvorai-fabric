# Security

Zyvor Fabric provides authentication, authorization, TLS, audit logging, and API keys for securing access to the VM management API.

For enterprise identity provider (Entra ID / Okta) lifecycle provisioning and
group-to-role sync via SCIM 2.0, see [scim-identity.md](scim-identity.md).

For the experimental OpenStack Keystone-compat surface (`/identity`, …), see
[openstack-compat.md](openstack-compat.md) and
[Tutorial 08](tutorials/08-openstack-clients.md). That path is separate from Fabric
JWT auth and from SCIM; treat it as a dialect adapter until it is wired to
`enterprise-identity`.

---

## Authentication

### Configuration

Authentication is configured in `/etc/zyvor-fabricd/zyvor-fabricd.toml`:

```toml
[auth]
enabled = true                          # Enable/disable authentication
# jwt_secret = "..."                    # Optional: auto-generated if omitted
# db_path = "/var/lib/zyvor-fabricd/auth.db" # SQLite user database
# token_expiration_hours = 24           # JWT token lifetime
# default_admin_password = "..."        # Optional: auto-generated if omitted
```

### First Startup

On first startup with authentication enabled, Zyvor Fabric:

1. **Creates an `admin` user** with a randomly generated password
2. **Writes the password** to `/var/lib/zyvor-fabricd/.admin_password` (mode `0600`, root-only readable)
3. **Generates a JWT signing secret** and persists it to `/var/lib/zyvor-fabricd/.jwt_secret` (mode `0600`)

To retrieve the admin password:

```bash
sudo cat /var/lib/zyvor-fabricd/.admin_password
```

To set a custom admin password before first startup:

```bash
# Option 1: Environment variable
export ZYVOR_FABRICD_ADMIN_PASSWORD="your-strong-password"
sudo systemctl start zyvor-fabricd

# Option 2: Config file
# Add to /etc/zyvor-fabricd/zyvor-fabricd.toml:
# [auth]
# default_admin_password = "your-strong-password"
```

To provide your own JWT secret:

```bash
export ZYVOR_FABRICD_JWT_SECRET="your-64-char-secret-here"
```

### JWT Tokens

All API endpoints (except `/api/auth/login`, `/health`, and `/readyz`) require authentication via JWT.

Optional **`tenant` claim** (from the user DB `users.tenant` column): when present on the
JWT, Fabric mirrors FluxVM token-tenant rules —

- create inherits the claim (body mismatch → **403**)
- list is force-scoped (`?tenant=` mismatch → **403**)
- get/mutate of another tenant’s VM → **404**

Platform admins without a tenant claim see the full fleet. Set a user’s tenant via the
auth DB (`UPDATE users SET tenant = 'acme' WHERE username = '…'`) then re-login.

**Login:**

```bash
# Read the generated password
PASSWORD=$(sudo cat /var/lib/zyvor-fabricd/.admin_password)

# Login
curl -X POST http://localhost:9095/api/auth/login \
  -H "Content-Type: application/json" \
  -d "{\"username\": \"admin\", \"password\": \"$PASSWORD\"}"
```

```json
{
  "token": "eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9...",
  "user_id": "admin",
  "role": "admin",
  "tenant": "acme"
}
```

**Use the token:**

```bash
curl -H "Authorization: Bearer <token>" \
  http://localhost:9095/api/vms
```

### OIDC SSO

Enterprise SSO uses the OIDC Authorization Code flow with PKCE (S256), state
TTL, nonce binding, and JWKS-based `id_token` verification (`iss` / `aud` /
`exp` / `nonce`). Setup notes for Keycloak, Entra ID, and Okta:
[oidc.md](oidc.md).

### API Keys

For service-to-service and CI/CD authentication:

```bash
# Generate an API key (admin only)
curl -X POST http://localhost:9095/api/auth/api-keys \
  -H "Authorization: Bearer <admin-token>" \
  -H "Content-Type: application/json" \
  -d '{"name": "ci-system", "role": "user"}'
```

```json
{
  "api_key": "zf_xxxxxxxxxxxxx",
  "name": "ci-system",
  "role": "user"
}
```

```bash
# Use the API key
curl -H "X-API-Key: zf_xxxxxxxxxxxxx" \
  http://localhost:9095/api/vms
```

---

## Authorization (RBAC)

Three built-in roles with progressively restricted permissions:

| Action | Admin | User | Viewer |
|--------|:-----:|:----:|:------:|
| List/view VMs | Yes | Yes | Yes |
| Create VMs | Yes | Yes | -- |
| Start/stop VMs | Yes | Yes | -- |
| Delete VMs | Yes | -- | -- |
| Manage users | Yes | -- | -- |
| View audit logs | Yes | Yes | Yes |
| Manage API keys | Yes | -- | -- |

### User Management

```bash
# Create a new user (admin only)
curl -X POST http://localhost:9095/api/auth/users \
  -H "Authorization: Bearer <admin-token>" \
  -H "Content-Type: application/json" \
  -d '{"username": "operator", "password": "strong-password", "role": "user"}'

# List users
curl -H "Authorization: Bearer <admin-token>" \
  http://localhost:9095/api/auth/users

# Delete a user
curl -X DELETE http://localhost:9095/api/auth/users/<user-id> \
  -H "Authorization: Bearer <admin-token>"
```

---

## Credential Files

Zyvor Fabric stores sensitive credentials in `/var/lib/zyvor-fabricd/` with restricted permissions:

| File | Purpose | Permissions |
|------|---------|-------------|
| `.admin_password` | Auto-generated admin password (first startup only) | `0600` (root) |
| `.jwt_secret` | JWT signing secret (persisted across restarts) | `0600` (root) |
| `auth.db` | SQLite user database with bcrypt password hashes | `0644` |

---

## TLS/HTTPS

### Enable TLS

```toml
# /etc/zyvor-fabricd/zyvor-fabricd.toml
[daemon]
listen = "0.0.0.0:8443"
tls_cert = "/etc/zyvor-fabricd/cert.pem"
tls_key = "/etc/zyvor-fabricd/key.pem"
```

### Generate a Self-Signed Certificate

```bash
openssl req -x509 -newkey rsa:4096 \
  -keyout /etc/zyvor-fabricd/key.pem \
  -out /etc/zyvor-fabricd/cert.pem \
  -days 365 -nodes \
  -subj "/CN=Zyvor Fabric"
```

For production, use certificates from a trusted CA or an ACME provider (Let's Encrypt).

---

## Audit Logging

All API actions are logged with user, action, resource, timestamp, and result:

```
AUDIT: admin CREATE vm/test-vm SUCCESS at 2026-02-18T12:00:00Z
AUDIT: user1 START vm/prod-vm SUCCESS at 2026-02-18T12:01:00Z
AUDIT: viewer DELETE vm/test DENIED at 2026-02-18T12:02:00Z
```

**View audit logs:**

```bash
# Via journalctl
sudo journalctl -u Zyvor Fabric | grep AUDIT

# Via API (with filtering)
curl -H "Authorization: Bearer <token>" \
  http://localhost:9095/api/audit/logs
```

Audit logs can be exported as JSON or CSV for compliance and analysis.

---

## Rate Limiting

Protect against abuse with configurable rate limits:

```toml
# /etc/zyvor-fabricd/zyvor-fabricd.toml
[security]
rate_limit_per_minute = 60
max_concurrent_requests = 100
```

---

## Network Security

### Restrict API Access with Firewall Rules

```bash
# Allow only from management network
sudo iptables -A INPUT -p tcp --dport 8080 -s 192.168.1.0/24 -j ACCEPT
sudo iptables -A INPUT -p tcp --dport 8080 -j DROP
```

### Access via SSH Tunnel

```bash
# On your workstation
ssh -L 8080:localhost:9095 user@Zyvor Fabric-server

# Then access via localhost
curl http://localhost:9095/api/vms
```

---

## Environment Variables

| Variable | Purpose |
|----------|---------|
| `ZYVOR_FABRICD_JWT_SECRET` | Override the JWT signing secret (takes priority over auto-generated) |
| `ZYVOR_FABRICD_ADMIN_PASSWORD` | Set the initial admin password (used only on first startup) |
| `ZYVOR_FABRICD_CONFIG` | Override the config file path |
| `ZYVOR_FABRICD_LISTEN` | Override `daemon.listen` (`host:port`) |
| `ZYVOR_FABRICD_PUBLIC_URL` | External base URL for OpenStack catalog / clients |

---

## Best Practices

1. **Always enable TLS in production** -- never expose the API over plain HTTP on untrusted networks
2. **Set `ZYVOR_FABRICD_ADMIN_PASSWORD`** -- use a strong password via environment variable before first startup, then remove it from the environment
3. **Rotate JWT secrets** -- update `ZYVOR_FABRICD_JWT_SECRET` and restart; existing tokens will be invalidated
4. **Use strong passwords** -- enforce a minimum of 12 characters
5. **Scope API keys** -- grant minimum required role for each key
6. **Monitor audit logs** -- set up alerts for failed authentication and denied actions
7. **Enable rate limiting** -- prevent brute-force and denial-of-service attacks
8. **Use a firewall** -- restrict API access to management networks
9. **Merion Zyvor Fabric updated** -- apply security patches promptly
10. **Delete `.admin_password`** after reading it -- avoid leaving credentials on disk

---

## Vulnerability Reporting

Report security vulnerabilities to: **security@Zyvor Fabric.io**

Do not disclose publicly until a patch is available.
