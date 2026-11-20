# Access Control

## Purpose

Access Control — manage the user accounts that can sign in to Zyvor Fabric: create accounts, assign a role (admin, operator, or viewer), and enable/disable or delete them.

Roles: **Admin**, **Operator**, **Viewer**. Disable locks a user out without deleting history.

## When to use it

- To create a login for a new team member and decide up front what they're allowed to do
- To temporarily lock a user out without deleting their account
- To see at a glance who has admin access, or when someone last logged in
- To remove an account that's no longer needed
- Prefer this page when the job matches the purpose above
- After failed-login spikes on Security Dashboard, disable the targeted account here

## How to get there

- Route / id: `/app/access-control`
- Nav: **Security → Access Control** (sidebar, command palette, or desktop nav)

## Operate from the console (UX)

1. Review tiles — Total Users, Admins, Operators, Viewers.
2. **Add User** — username (3–32 chars: letters, numbers, hyphens, underscores), password (8+), role via **Admin** / **Operator** / **Viewer**. Client-side validation before submit.
3. User table: avatar/username, role badge, **Active/Disabled** toggle, created date, last login (or "Never").
4. Flip **Active/Disabled** to lock or restore without deleting.
5. Trash icon deletes after confirmation (`Delete user "…"?`).

Typical flow: Add User as Viewer first → raise to Operator when needed → keep Admin count small → Disable instead of Delete when investigating. Confirm actions in [Audit](../monitoring/audit.md).

**Empty / fail:** Error banner, empty table, or failed toast — confirm you are signed in, `zyvor-fabricd` is healthy (`/readyz` on `http://127.0.0.1:<port>` or `https://<host>`), and any backend this page needs (FluxVM, storage, network) is reachable. See [Admin basics](../../admin-basics.md).

**Success:** Live data loads without error; creates/updates appear in the list or detail view and any confirmation toast clears cleanly.

## Related pages

- [Security Dashboard](security-dashboard.md)
- [Audit](../monitoring/audit.md)
- [Compliance](compliance.md)
- [Certificates](certificates.md)
- [Sign in](../auth/login.md)
- [Getting Started](../../getting-started.md)
- [Page index](../../PAGE_INDEX.md)
