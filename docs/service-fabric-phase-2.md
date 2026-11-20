# Service Fabric phase 2 — archive (shipped)

This page is historical. Dual-stack DSR/SNAT/north-south XDP and the items below
shipped in Service Fabric **v2**, then were carried into later schemas.

**Current:** [ebpf-service-fabric.md](ebpf-service-fabric.md) (v6) ·
[phase 4](service-fabric-phase4.md) · [phase 5](service-fabric-phase5.md) ·
[phase 6 (shipped + remaining)](service-fabric-phase6.md)

## Originally planned (now done unless noted)

1. **North-south XDP service ingress** — **done** (v2).
2. **DSR** — **done** (v2).
3. **SNAT** — **done** (v2).
4. **IPv6 services** — **done** (v2).
5. **Active health** — **done** (v3 TCP probes; Fabric owns durable intent).
6. **Identity-aware service policy** — **done** (v6).
7. **EDT bandwidth manager** — **done** (v4).
8. **Hubble-grade / FluxScope service events** — **done** (v4).
9. **Local redirect / cgroup connect** — still open ([phase 6 remaining](service-fabric-phase6.md)).
10. **L7 redirect contract** — **done** (v6; Envoy owns parsing).

Do not treat this file as an active backlog.
