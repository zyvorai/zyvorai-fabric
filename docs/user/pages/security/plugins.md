# Plugins

## Purpose

Plugin Manager — enable, disable, and review the server extensions installed on Zyvor Fabric (storage, network, security, monitoring, and backup plugin types).

Enable/disable only — there is no install or configure-plugin flow on this page.

## When to use it

- Prefer this page when the job matches the purpose above
- To see which plugins are installed and whether each is running, stopped, or erroring
- To turn a plugin on or off without restarting the whole service
- To check a plugin's version and author before relying on it
- Prefer this page when the job matches the purpose above
- After an upgrade, to confirm expected plugins are Running and Errors is zero

## How to get there

- Route / id: `/plugins`
- Nav: **Security → Plugins** (sidebar, command palette, or desktop nav)

## Operate from the console (UX)

1. Review the four stat tiles: Total Plugins, Running, Errors, and Types (distinct categories).
2. Browse plugin cards — name, version, type badge (storage/network/security/monitoring/backup), status (running/stopped/error), description, author when provided.
3. Click **Enable**/**Disable** on a card; the button spins while the request is in flight and the card status updates after.

Typical flow: scan Errors tile → open the erroring card → Disable if it is blocking, or fix backend deps then Enable again. Confirm Dashboard capability chips still look healthy after toggles.

Operator tip: treat Errors > 0 as blocking for change windows until the plugin is fixed or intentionally Disabled.

**Empty / fail:** Error banner, empty table, or failed toast — confirm you are signed in, `zyvor-fabricd` is healthy (`/readyz` on `http://127.0.0.1:<port>` or `https://<host>`), and any backend this page needs (FluxVM, storage, network) is reachable. See [Admin basics](../../admin-basics.md).

**Success:** Live data loads without error; creates/updates appear in the list or detail view and any confirmation toast clears cleanly.

## Related pages

- [Security Dashboard](security-dashboard.md)
- [Access Control](access-control.md)
- [Certificates](certificates.md)
- [Encryption](encryption.md)
- [Backups](../operations/backups.md)
- [Dashboard](../core/home.md)
- [Getting Started](../../getting-started.md)
- [Page index](../../PAGE_INDEX.md)
