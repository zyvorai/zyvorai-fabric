# Enterprise Identity: SCIM 2.0 Provisioning

Fabric already supports LDAP/OIDC/SAML external authentication with claim-based
role mapping. This feature adds lifecycle provisioning on top of that stack so
an identity provider (Microsoft Entra ID, Okta, etc.) can create, update,
disable and delete Fabric users and groups directly, and have that state
enforced at login.

## Included

- `backend/enterprise-identity/` — SCIM models, provisioning-token helpers,
  patch/filter engine, and role resolution. No HTTP or persistence code; the
  API server persists these types through Fabric's existing `StateStore`.
- `backend/zyvor-fabricd/src/api/scim.rs` — admin profile/token APIs plus the
  SCIM `User`/`Group` endpoints and the `provisioning_decision` hook used by
  OIDC login.

## Behavior

1. An admin links a provisioning profile to an existing Fabric auth provider.
2. The admin maps enterprise groups to `admin`, `user`, or `viewer`.
3. Fabric mints a dedicated, one-time-visible SCIM bearer token.
4. Entra ID / Okta provisions Users and Groups through `/scim/v2`.
5. Fabric recalculates each user's effective role from group membership.
6. Deactivation or deletion blocks the linked external user at their next
   login — SCIM is authoritative once a profile is linked and enabled.
7. Auth providers without a linked provisioning profile keep their existing
   claim-based login behavior unchanged.

## Endpoints

Admin routes (behind Fabric's normal JWT + `RequireAdmin`):

```text
GET/POST   /api/v1/identity/scim/profiles
PUT/DELETE /api/v1/identity/scim/profiles/{id}
GET/POST   /api/v1/identity/scim/tokens
DELETE     /api/v1/identity/scim/tokens/{id}
```

SCIM data-plane routes (behind the profile-scoped provisioning bearer token,
not a Fabric JWT):

```text
/scim/v2/ServiceProviderConfig
/scim/v2/ResourceTypes
/scim/v2/Schemas
/scim/v2/Users[/{id}]
/scim/v2/Groups[/{id}]
```

## Configuring a provider

```bash
curl -X POST https://fabric.example.com/api/v1/identity/scim/profiles \
  -H "Authorization: Bearer $FABRIC_ADMIN_JWT" \
  -H "Content-Type: application/json" \
  -d '{
    "name": "Microsoft Entra ID",
    "authProviderId": "<existing-oidc-provider-id>",
    "enabled": true,
    "requireProvisionedUser": true,
    "defaultRole": "viewer",
    "groupRoleMapping": {
      "Fabric Admins": "admin",
      "Fabric Operators": "user",
      "Fabric Viewers": "viewer"
    }
  }'
```

```bash
curl -X POST https://fabric.example.com/api/v1/identity/scim/tokens \
  -H "Authorization: Bearer $FABRIC_ADMIN_JWT" \
  -H "Content-Type: application/json" \
  -d '{"profileId": "<profile-id>", "name": "entra-production"}'
```

The plaintext token is returned **once**. Fabric persists only a SHA-256 hash
of it. Configure the IdP's tenant URL as `https://fabric.example.com/scim/v2`
and its secret token as the returned `fscim_...` value.

## Security properties

- Dedicated SCIM bearer tokens are separate from human/API session JWTs.
- Provisioning tokens are never persisted in plaintext; verification hashes
  the presented token and compares in constant time.
- Tokens are scoped to exactly one provisioning profile and can be revoked
  independently.
- Cross-profile resource IDs return `404`, preventing tenant/profile
  enumeration.
- Deleted users are tombstoned and disabled rather than hard-erased,
  retaining identity history in `StateStore`.
- Group role resolution is server-controlled: the IdP cannot grant itself an
  Admin role unless a Fabric administrator mapped that group name to Admin. A
  profile's `defaultRole` is a floor — group mappings can only raise a user's
  rank above it, never below it.
- `requireProvisionedUser=true` turns provisioning into an explicit allow-list
  for the linked auth provider; an unprovisioned user is denied login rather
  than falling back to the provider's default role.
- The OIDC ID token this feature reads is JWKS-verified by Fabric (signature,
  `iss`, `aud`, `exp`, `nonce`) after the authorization-code + PKCE token
  exchange, so the `username` claim used for provisioning lookups is not
  attacker-controlled. See [oidc.md](oidc.md).

## Follow-up hardening

OIDC Authorization Code flow now includes PKCE (S256), nonce binding, and
JWKS-based `id_token` verification. Remaining external-auth gap: the LDAP
connectivity stub still needs a real TLS bind/search — that is unrelated to
SCIM provisioning. See [oidc.md](oidc.md).
