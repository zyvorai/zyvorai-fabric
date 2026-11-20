# Service Fabric phase 3 — shipped as v3

The phase-3 candidates below landed as **Service Fabric schema v3** and remain
part of the cumulative **v6** dataplane (BPF schema 4 ABI)
([ebpf-service-fabric.md](ebpf-service-fabric.md)). This page is retained as an
archive so older links keep working.

## Shipped (P0)

1. **Forward conntrack/backend pinning** — Maglev affinity maps; TCP/UDP timeouts; GC/pressure.
2. **Backend health + graceful drain** — `ready` / `draining` / `unhealthy`, drain deadlines, TCP active health.
3. **VIP advertisement** — FluxVM atomic snapshot; Fabric `advertise` + lease-gated ECMP/BGP intent.
4. **HA service state** — `EdgeLease`, `withdraw_node()`, optional whitelisted conntrack export/import.

## Later phases

Phase-4 performance/observability (EDT, FluxScope, host-routing, FRR/BIRD/File)
shipped as **schema v4** — see [service-fabric-phase4.md](service-fabric-phase4.md).

Phase-5 HA/state-plane (streaming HA deltas, durable lease controller, incremental
reconcile) shipped as **v5** — see [service-fabric-phase5.md](service-fabric-phase5.md).

Phase-6 identity/L7 policy + HA mutation queue shipped as **v6** — see
[service-fabric-phase6.md](service-fabric-phase6.md) (remaining candidates listed there).
