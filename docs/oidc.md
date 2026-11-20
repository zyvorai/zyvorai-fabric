# OIDC / External SSO

Fabric supports OpenID Connect (OIDC) Authorization Code flow for enterprise
SSO (Keycloak, Microsoft Entra ID, Okta, and other OIDC-compliant IdPs).

API surface:

- `GET /api/auth/oidc/login/{provider_id}` — returns the IdP authorization URL
- `POST /api/auth/oidc/callback` — exchanges `code` + `state` for a Fabric JWT

Configure providers via `POST /api/auth/providers` with `provider_type: "oidc"`.

## Security posture

The login callback implements:

| Control | Behavior |
|---------|----------|
| **PKCE (S256)** | `code_verifier` stored in pending state; `code_challenge` + `code_challenge_method=S256` on the auth URL; `code_verifier` sent on token exchange |
| **State TTL** | Pending `state` entries expire after ~10 minutes (and are purged by a background cleaner) |
| **Nonce** | Generated at login, stored in pending state, sent on the auth URL, verified against the `id_token` `nonce` claim |
| **JWKS verification** | `id_token` signature verified with RSA keys from discovery `jwks_uri`; also checks `iss`, `aud` (`client_id`), `exp`, and `nonce` |
| **JWKS cache** | In-memory cache (~1h TTL) with one forced refresh on unknown `kid` (key rotation) |
| **Role mapping** | Existing `role_claim` / `role_mapping` / `default_role` (and optional SCIM overlay) unchanged |

Supported signing algorithms for `id_token`: **RS256**, **RS384**, **RS512**.

Issuer and discovered endpoint URLs are validated against SSRF allowlists (public
HTTPS destinations only).

## Provider setup notes

### Common fields

```json
{
  "name": "corp-sso",
  "provider_type": "oidc",
  "default_role": "viewer",
  "config": {
    "issuer_url": "https://idp.example.com/realms/fabric",
    "client_id": "zyvor-fabric",
    "client_secret": "<secret>",
    "redirect_uri": "https://fabric.example.com/auth/oidc/callback",
    "scopes": ["openid", "profile", "email"],
    "username_claim": "preferred_username",
    "role_claim": "roles",
    "role_mapping": {
      "fabric-admins": "admin",
      "fabric-users": "user"
    }
  }
}
```

Register the Fabric `redirect_uri` as an allowed redirect on the IdP client.
The client must allow the **authorization code** grant and PKCE.

### Keycloak

1. Create a confidential client (or public + PKCE-only if you omit the secret —
   Fabric currently always sends `client_secret` on token exchange).
2. Valid redirect URIs: your Fabric callback URL.
3. Issuer is typically `https://<host>/realms/<realm>`.
4. Map realm/client roles into a claim named to match `role_claim` (e.g. via a
   protocol mapper), or rely on `default_role` / SCIM.

### Microsoft Entra ID

1. App registration → Web redirect URI = Fabric callback.
2. Issuer: `https://login.microsoftonline.com/<tenant-id>/v2.0` (or the
   organization-specific issuer from the discovery document).
3. Create a client secret under Certificates & secrets.
4. Prefer `preferred_username` or `email` for `username_claim`. Group/role
   claims require optional claims / group overage configuration in Entra;
   Fabric maps a **string** `role_claim` value today.

### Okta

1. Create an OIDC Web application with Authorization Code + PKCE.
2. Issuer: `https://<org>.okta.com` or `https://<org>.okta.com/oauth2/<authServerId>`.
3. Assign groups and map a groups/roles claim to match `role_claim` if used.

## Related

- [SCIM provisioning](scim-identity.md) — lifecycle and group→role sync on top of
  an existing OIDC/SAML/LDAP provider
- [Security overview](security.md)
