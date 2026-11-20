# Service Fabric Phase 5 — delivered tranche

This cumulative kit delivers the state-plane/HA tranche on top of v4 while deliberately retaining the v4 TC/XDP ABI.

## Delivered

1. **Bounded HA delta journal in FluxVM**
   - monotonic sequence numbers;
   - source acknowledgement watermark;
   - bounded replay retention;
   - gap detection and explicit `reset_required`;
   - full-snapshot barrier for history gaps;
   - per-service standby replay cursor;
   - whitelisted conntrack/NAT maps only.

2. **Streaming/delta replication orchestration in Fabric**
   - durable per source/target cursor store;
   - per-target catch-up loops;
   - full-snapshot fallback on a journal gap;
   - source journal acknowledgement only to the **minimum** replicated target cursor;
   - replay-safe idempotent target import.

3. **Durable edge lease controller**
   - atomic JSON state store;
   - persistent per-service fencing epoch high-water marks (no ABA reuse);
   - healthy lease expiry extension without epoch churn;
   - expired-lease compaction while retaining epoch history;
   - withdraw-before-release for unhealthy edges;
   - deterministic replacement selection;
   - replacement service is staged with advertisement disabled;
   - conntrack state is seeded before advertisement;
   - local FluxVM advertisement readiness is verified before the replacement counts as active.

4. **Incremental BPF service-intent reconciliation**
   - desired map image is calculated in userspace;
   - unchanged map entries are left untouched;
   - only changed/new entries are updated;
   - stale service-intent entries are deleted;
   - forward conntrack, reverse NAT and backend telemetry remain lifecycle state and are not cleared;
   - the existing update guard remains fail-closed if reconciliation fails.

5. **Cumulative v4 integration fixes**
   - ServiceFabricConfig carries EDT and OTLP settings required by v4;
   - `fluxvm-network` enables reqwest's JSON feature for OTLP export;
   - FluxVM REST exposes FluxScope flow/OTLP endpoints and v5 delta endpoints;
   - Fabric driver-core/fabricd carry v4 service performance fields through to `service-lb`.

## ABI decision

`SERVICE_SCHEMA_VERSION` remains **4** because v5 does not change TC/XDP map layouts. The v5 changes are state/control-plane semantics around the existing v4 BPF ABI. This avoids an unnecessary BPF reload and verifier migration merely to add HA journal/lease behavior.
