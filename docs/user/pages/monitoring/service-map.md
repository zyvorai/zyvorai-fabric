# Service Map

## Purpose

Service Map — shows the services discovered across your VMs, which ones depend on each other (protocol and port), and each service's current health.

Use it to understand blast radius before restarting a VM or changing a service. Refreshes every 15 seconds.

## When to use it

- Understanding what a VM's service depends on, or what depends on it, before restarting or changing it
- Spotting degraded or down services at a glance
- Tracing a specific inbound or outbound connection between two services
- During incident triage when you know a port/protocol but not the owning service
- After deploy, to confirm new services appear and dependencies look sane

## How to get there

- Console URL pattern: `http://127.0.0.1:<port>/…` or `https://<host>/…`
- Route / id: `/service-map`
- Nav: **Monitoring → Service Map** (sidebar, command palette, or desktop nav)

## Operate from the console (UX)

1. Summary cards show **Services**, **Healthy**, **Degraded**, and **Down** counts.
2. Service cards show name, health dot (green/amber/red/gray), type badge (web, database, cache, queue, api, proxy, …), host VM, port, and inbound/outbound link counts.
3. Click a service card to filter **Dependencies** to that service's links (both directions); unrelated cards dim. **Show all** clears the selection.
4. The Dependencies panel lists each link as `From → To` with protocol and port.
5. Auto-refresh every 15 seconds; manual refresh is in the page header.

Typical flow: find Down/Degraded cards → select one → read Dependencies → open the host VM from Virtual Machines if you need console or restart. This page is read-only discovery — it does not change networking.

**Empty / fail:** Error banner, empty table, or failed toast — confirm you are signed in, `zyvor-fabricd` is healthy (`/readyz` on `http://127.0.0.1:<port>` or `https://<host>`), and any backend this page needs (FluxVM, storage, network) is reachable. See [Admin basics](../../admin-basics.md).

**Success:** Live data loads without error; creates/updates appear in the list or detail view and any confirmation toast clears cleanly.

## Related pages

- [Live Metrics](live-metrics.md)
- [Alerts](alerts.md)
- [Network Topology](../more-images-migrations-managers/network-topology.md)
- [Virtual Machines](../core/vms.md)
- [Event Stream](event-stream.md)
- [Getting Started](../../getting-started.md)
- [Page index](../../PAGE_INDEX.md)
