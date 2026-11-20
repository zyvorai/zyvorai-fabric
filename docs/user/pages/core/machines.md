# Machines (removed)

The `/app/machines` page and `/api/machines` surface have been **removed**.

Use **Virtual Machines** (`/app/vms`) and the FluxVM-backed VM APIs instead
(`GET/POST /api/vms`, lifecycle under `/api/vms/{name}/…`).

Historically this page exposed systemd-machined / machinectl operations; that
backend is gone from Fabric.
