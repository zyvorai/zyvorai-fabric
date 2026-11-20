# API Playground

## Purpose

API Playground — send authenticated, ad-hoc HTTP requests to the Zyvor Fabric API from the browser: pick a preset endpoint or type any path, choose method, optional JSON body, inspect status/headers/body, and keep a short in-session history.

## When to use it

- To probe `/health`, `/api/vms`, storage, templates, backups, audit, and other presets without leaving the console
- To try a POST/PUT body against an endpoint before scripting it
- To copy a formatted JSON response for tickets or docs
- To confirm auth is working (requests use your current session)

## How to get there

- Route / id: `/app/playground`
- Nav: top-bar **API Playground** icon, **Tools** (when listed), or command palette
- Legacy `/playground` redirects here

## Operate from the console (UX)

1. Open the page — left column lists **preset endpoints** (Health, List VMs, CPU/NUMA topology, network links, storage pools, templates, backups, audit, certificates, quotas, schedules, webhooks, optimizations).
2. Click a preset to fill method + path, or type your own path (e.g. `/api/vms/myvm`) and pick **GET** / **POST** / **PUT** / **DELETE**.
3. For POST/PUT, paste a JSON body in the request editor when needed.
4. **Send** — response panel shows status, duration, headers, and pretty-printed JSON when possible. Use **Copy** on the body.
5. **History** (session-only, last ~20) lets you re-select a prior call; clear with the trash control.
6. **Empty / fail:** Network/auth errors show in the error banner with daemon hints; HTTP 4xx/5xx still populate the response panel so you can read the API error body.
7. **Success:** Green/2xx status chip, formatted body, and a new history row.

For contracts and examples prefer operator docs under `docs/` (e.g. [API reference](../../../guides/cli/api-reference.md)) — the playground does not invent endpoints.

## Related pages

- [Dashboard](../core/home.md)
- [Virtual Machines](../core/vms.md)
- [Audit](../monitoring/audit.md)
- [Getting Started](../../getting-started.md)
- [Page index](../../PAGE_INDEX.md)
