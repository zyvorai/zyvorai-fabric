# Authentication API Reference

Detailed specification of the Zyvor Fabric authentication system, covering the login flow, JWT token structure, token lifecycle, role-based access control, and PAM integration.

## Table of Contents

- [Login Flow](#login-flow)
- [JWT Token Structure](#jwt-token-structure)
- [Token Lifecycle](#token-lifecycle)
- [Token Revocation](#token-revocation)
- [Role-Based Access Control](#role-based-access-control)
- [PAM Integration](#pam-integration)
- [Rate Limiting](#rate-limiting)
- [Two-Factor Authentication (TOTP)](#two-factor-authentication-totp)

---

## Login Flow

Authentication is performed by sending system credentials to the login endpoint. The backend authenticates against PAM (Pluggable Authentication Modules) and issues a signed JWT token.

```
Client                          Zyvor Fabric                         PAM
  |                                |                              |
  |  POST /api/auth/login          |                              |
  |  {"username","password"}       |                              |
  |------------------------------->|                              |
  |                                |  authenticate(user, pass)    |
  |                                |----------------------------->|
  |                                |                              |
  |                                |  result: success/failure     |
  |                                |<-----------------------------|
  |                                |                              |
  |                                |  lookup groups (id -Gn user) |
  |                                |  determine role              |
  |                                |  generate JWT                |
  |                                |                              |
  |  200 {token, user_id, role}    |                              |
  |<-------------------------------|                              |
```

**Request:**

```http
POST /api/auth/login
Content-Type: application/json

{
  "username": "admin",
  "password": "secret"
}
```

**Response (200 OK):**

```json
{
  "token": "eyJhbGciOiJIUzI1NiJ9.eyJzdWIiOiJhZG1pbiIsInJvbGUiOiJhZG1pbiIsImV4cCI6MTcxMzAwMDAwMCwianRpIjoiNTUwZTg0MDAtZTI5Yi00MWQ0LWE3MTYtNDQ2NjU1NDQwMDAwIn0.signature",
  "user_id": "admin",
  "role": "admin",
  "username": "admin"
}
```

The login endpoint does **not** require an existing JWT token. It is a public
API endpoint along with `/health` (liveness) and `/readyz` (readiness: Fabric
store + FluxVM `/readyz`).

---

## JWT Token Structure

Tokens are signed with HMAC-SHA256 (`HS256`) using a server-configured secret.

### Header

```json
{
  "alg": "HS256",
  "typ": "JWT"
}
```

### Claims (Payload)

| Claim | Type | Description |
|-------|------|-------------|
| `sub` | string | Subject -- the username (user ID) |
| `role` | string | User role: `admin`, `user`, or `viewer` |
| `exp` | integer | Expiration time (Unix timestamp) |
| `jti` | string | JWT ID -- unique identifier (UUIDv4) for revocation tracking |
| `tenant` | string (optional) | When set, scopes VM create/list/get/mutate to that tenant |

**Example decoded payload:**

```json
{
  "sub": "admin",
  "role": "admin",
  "tenant": "acme",
  "exp": 1713000000,
  "jti": "550e8400-e29b-41d4-a716-446655440000"
}
```

When `tenant` is set, create inherits it (body mismatch → 403), list is force-scoped,
and get/mutate of another tenant’s VM returns 404. Admins with no tenant claim see all VMs.
Assign via `UPDATE users SET tenant = 'acme' WHERE username = '…'` then re-login.

### Using the Token

Include the token in the `Authorization` header of every API request:

```http
GET /api/vms
Authorization: Bearer eyJhbGciOiJIUzI1NiJ9...
```

---

## Token Lifecycle

| Event | Behavior |
|-------|----------|
| **Issued** | On successful `POST /api/auth/login` |
| **Expiration** | Default: 24 hours after issuance (configurable via `expiration_hours`) |
| **Validation** | Every request: signature check, expiration check, revocation check |
| **Minimum TTL** | 1 hour (if configured below 1, the server enforces a 1-hour minimum with a warning) |
| **Revoked** | Token's `jti` is added to the in-memory revocation set |

### Handling Expired Tokens

When a token expires, the API returns:

```http
HTTP/1.1 401 Unauthorized
Content-Type: application/json

{
  "error": "Authentication required"
}
```

The client should re-authenticate by calling `POST /api/auth/login` to obtain a new token.

---

## Token Revocation

Tokens can be revoked before expiration using the `jti` (JWT ID) claim. Revoked tokens are rejected during validation even if they have not expired.

Revocation is tracked in an in-memory set within the `JwtConfig` instance. This means:

- Revocations are immediate and in-process.
- Revocations are lost on service restart. After a restart, expired tokens are naturally rejected; unexpired tokens remain valid until their `exp` time passes.
- For forced invalidation of all tokens, rotate the JWT secret and restart the service.

---

## Role-Based Access Control

### Roles

| Role | Description | Assigned When |
|------|-------------|---------------|
| **Admin** | Full access to all operations | User is `root`, or is a member of `wheel`, `sudo`, or `adm` groups |
| **User** | Can create and manage VMs, take backups, manage networking | All other authenticated system users |
| **Viewer** | Read-only access to all resources | Manually assigned (not auto-assigned by PAM login) |

### Permission Matrix

API endpoints enforce minimum permission levels using Axum extractors:

| Extractor | Minimum Role | Operations |
|-----------|-------------|------------|
| `RequireRead` | Viewer | List, get, view metrics, browse events |
| `RequireWrite` | User | Create, start, stop, restart, pause, resume, clone, backup, configure |
| `RequireAdmin` | Admin | Delete VMs, delete snapshots/backups, manage storage pools, manage notification channels, shell access, file transfer, terminate machines |

### Permission Hierarchy

```
Admin  >  User  >  Viewer
  |         |        |
  |         |        +-- can_read()   = true
  |         +----------- can_write()  = true, can_read() = true
  +---------------------- can_manage() = true, can_write() = true, can_read() = true
```

Every higher role inherits all permissions of lower roles.

### Error Sanitization

Non-admin users receive sanitized error messages that do not expose internal file paths or system details. Admin users see full error details for debugging.

---

## PAM Integration

Zyvor Fabric authenticates against the system's PAM stack. This means:

- **User accounts are system accounts.** There is no separate user database for Zyvor Fabric. Users log in with their Linux credentials.
- **Password policies are inherited** from PAM modules (pam_pwquality, pam_faillock, etc.).
- **Account lockouts, password aging, and two-factor authentication** are supported if configured at the PAM level.
- **Group membership determines the role.** After successful authentication, Zyvor Fabric checks the user's Unix groups:
  - Members of `wheel`, `sudo`, or `adm` receive the `admin` role.
  - All others receive the `user` role.

### PAM Service Configuration

The PAM authentication call uses the service name configured in the zyvor-fabricd binary (typically `zyvor-fabricd` or `login`). Ensure the PAM service file exists at `/etc/pam.d/zyvor-fabricd` or that the fallback service (`/etc/pam.d/other`) is permissive enough for your use case.

### Security Notes

- PAM authentication is performed on a **blocking thread pool** (`tokio::task::spawn_blocking`) to avoid blocking the async runtime.
- Failed login attempts record the **username** for rate limiting but do not log the password.
- Successful login clears the per-user rate limit counter.

---

## Rate Limiting

Two independent rate limiters protect the login endpoint:

### Per-User Rate Limit

| Parameter | Value |
|-----------|-------|
| Window | 5 minutes (sliding) |
| Max failed attempts | 5 per username |
| Response when limited | `429 Too Many Requests` |

### Global Rate Limit

| Parameter | Value |
|-----------|-------|
| Window | 5 minutes (sliding) |
| Max failed attempts | 50 across all usernames |
| Response when limited | `429 Too Many Requests` |

### Behavior

- Only **failed** login attempts count toward the limit. Successful logins do not increment the counter.
- A **successful login** clears the per-user counter for that username.
- The global limiter prevents distributed brute-force attacks that target many usernames.
- Rate limiter state is stored in memory and is cleared on service restart.
- When the in-memory rate limit map exceeds 1,000 entries, stale entries are automatically evicted.

### Rate Limit Response

```http
HTTP/1.1 429 Too Many Requests
Content-Type: application/json

{
  "error": "Too many login attempts, try again later"
}
```

---

## Two-Factor Authentication (TOTP)

Zyvor Fabric supports optional TOTP-based two-factor authentication. When 2FA is enabled for a user, the login flow requires an additional `totp_code` field.

### 2FA Setup Flow

Setting up 2FA is a two-step process:

1. **Generate secret** -- Call `POST /api/auth/2fa/setup` to generate a TOTP secret and provisioning URI.
2. **Verify and enable** -- Scan the QR code or enter the secret into an authenticator app, then call `POST /api/auth/2fa/verify` with a valid TOTP code to confirm the setup.

```
User                            Zyvor Fabric
  |                                |
  |  POST /api/auth/2fa/setup      |
  |------------------------------->|
  |                                |  generate TOTP secret
  |  200 {secret, provisioning_uri}|
  |<-------------------------------|
  |                                |
  |  (configure authenticator app) |
  |                                |
  |  POST /api/auth/2fa/verify     |
  |  {"code": "123456"}            |
  |------------------------------->|
  |                                |  validate code against secret
  |  200 {status, recovery_codes}  |
  |<-------------------------------|
```

**Step 1: Generate the TOTP secret**

```bash
curl -s -X POST http://localhost:3000/api/auth/2fa/setup \
  -H "Authorization: Bearer $TOKEN" | jq
```

**Response:**

```json
{
  "secret": "JBSWY3DPEHPK3PXP",
  "provisioning_uri": "otpauth://totp/zyvor-fabricd:admin?secret=JBSWY3DPEHPK3PXP&issuer=zyvor-fabricd",
  "qr_code": "data:image/png;base64,..."
}
```

Use the `provisioning_uri` or `qr_code` to configure your authenticator app (Google Authenticator, Authy, FreeOTP, etc.).

**Step 2: Verify and enable**

```bash
curl -s -X POST http://localhost:3000/api/auth/2fa/verify \
  -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d '{"code": "123456"}' | jq
```

**Response:**

```json
{
  "status": "2fa_enabled",
  "recovery_codes": ["a1b2c3d4", "e5f6g7h8", "i9j0k1l2"]
}
```

Store the recovery codes in a safe location. They can be used as a fallback if you lose access to your authenticator app.

### Login with TOTP Code

Once 2FA is enabled, the login request must include a `totp_code` field:

```http
POST /api/auth/login
Content-Type: application/json

{
  "username": "admin",
  "password": "secret",
  "totp_code": "123456"
}
```

**curl example:**

```bash
curl -s -X POST http://localhost:3000/api/auth/login \
  -H "Content-Type: application/json" \
  -d '{
    "username": "admin",
    "password": "secret",
    "totp_code": "123456"
  }' | jq
```

If a user has 2FA enabled and the `totp_code` field is missing or contains an invalid code, the login request returns `401 Unauthorized`.

### Disabling 2FA

To disable 2FA, call `POST /api/auth/2fa/disable` with a valid TOTP code to confirm the action:

```bash
curl -s -X POST http://localhost:3000/api/auth/2fa/disable \
  -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d '{"code": "654321"}' | jq
```

**Response:**

```json
{
  "status": "2fa_disabled"
}
```

After disabling 2FA, the `totp_code` field is no longer required in login requests.
