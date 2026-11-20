# On behalf of

**Status:** built in part, default behaviour unchanged. Tested against a local token server; no real identity provider.

## What already existed

Per-person credentials (`oauth.connection`): the host mints a short-lived access token from that person's own stored
refresh token, optionally narrowed with `oauth.scope`. The agent never sees either token.

## What this adds

1. **`identity` on every `egress.http` journal row.**
   - `autonomous`: the request named no credential.
   - `delegated`: it used a person's own connection; `acting_for` names the session's user.
   - `service`: it used a credential the operator holds for the whole host.
2. **`oauth.audience`**, sent as `resource` (RFC 8707), so a provider that honours it issues a token only that API accepts.
   A provider that ignores it issues the token it always did.

`identity` is a scalar and is on the OTLP and recording allowlists; `acting_for` is not.

## Not done

- RFC 8693 token exchange (swapping a person's token for a narrower one at another service).
- Sender-constrained tokens (DPoP or mTLS binding). Tokens stay bearer tokens, held only by the host.
- Journal rows for the CONNECT proxy path.
