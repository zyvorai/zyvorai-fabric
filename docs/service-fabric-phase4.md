# Service Fabric Phase 4 — implemented scope

Phase 4 turns the v3 HA-capable dataplane into a more production-oriented service edge without moving distributed ownership into FluxVM.

Implemented:

1. **EDT service pacing** — per-service byte rate, BPF departure clock, `skb->tstamp`, explicit fq ownership.
2. **FluxScope service flows** — shared TC/XDP flow map, backend/service identity, rich verdict/reason telemetry, OTLP/HTTP JSON export.
3. **Optional BPF host routing** — NAT translation followed by FIB redirect when compatible, with non-destructive host-stack fallback.
4. **Expanded service metrics** — EDT, fast-route, fallback and telemetry counters on top of v3 connection/health counters.
5. **Production routing adapters in Fabric** — FRR, BIRD and atomic file intent while preserving Fabric ownership of ASN/peer/session policy.
6. **Full Fabric v4 contract** — dual stack, exposure, backend lifecycle, SNAT, health, advertise, pacing, sampling and host-routing settings through driver-core/fabricd/service-lb.

Not included in Phase 4: cgroup/socket-level LB, Envoy L7 policy, automatic durable lease controller, streamed conntrack replication and direct AF_XDP/DPDK paths. Those remain Phase 5 candidates.
