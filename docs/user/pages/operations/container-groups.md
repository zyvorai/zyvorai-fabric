# Container Groups

## Purpose

Apply and manage ContainerGroup workloads on FluxVM Secure Containers (`RuntimeClass fluxvm`); placement uses hosts with `secure_containers_ready`.

Fabric places Pods via `spec.nodeName` after DRS filters to ready hosts; it does not talk to the Secure Containers shim directly (heartbeat / `/readyz.secure_containers`).

## When to use it

- Run container workloads on FluxVM Secure Containers through Fabric
- Inspect live status for a named group

## How to get there

- Route: `/app/container-groups`
- Nav: **Operations → Container Groups**

## What you can do

- List / apply / delete groups (`/api/container-groups…`)
- Expand a group for live status (`GET /api/container-groups/{name}/status`)

## Related

- Operator guide: [container-groups.md](../../../container-groups.md)
- [Datacenters](../core/datacenters.md) — host `secure_containers_ready` UX
- [Containers](../infrastructure/containers.md) — Docker/Podman host view (different)
- [Page index](../../PAGE_INDEX.md)
