# Sign in

## Purpose

Sign-in screen for Zyvor Fabric. Authenticates against the local admin account or a configured identity provider / PAM user on the host.

## When to use it

- Whenever you're not signed in — protected `/app` routes redirect here if the session is missing or expired
- To sign in as the local admin (`admin` + generated password)
- To sign in with your own system account when PAM/OIDC is configured
- After an expired JWT session when the console sends you back here

## How to get there

- Route: `/sign-in` (legacy `/login` redirects here)
- From marketing pages: **Sign in** in the top nav
- After signing in you land on the console at `/app`

## Operate from the console (UX)

1. Enter a **username** and click **Continue**.
2. Enter your **password** and sign in — a failed attempt shows an inline error.
3. On success you are taken to `/app` (dashboard).
4. Two common ways to sign in: **local admin** — username `admin`, password from `./zyvor-fabricd-ctl password` or `/var/lib/zyvor-fabricd/.admin_password` — or your own **system user** account when PAM/OIDC is configured.

**Empty / fail:** Inline error on bad credentials, or the page never loads — confirm `zyvor-fabricd` is reachable on `:9095` and that you have the current admin password ([Admin basics](../../admin-basics.md)).

**Success:** You land on `/app` with the console nav visible.

## Related pages

- [Dashboard](../core/home.md)
- [Access Control](../security/access-control.md)
- [Getting Started](../../getting-started.md)
- [Admin basics](../../admin-basics.md)
- [Page index](../../PAGE_INDEX.md)
