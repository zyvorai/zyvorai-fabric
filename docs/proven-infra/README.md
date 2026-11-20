# Proven infrastructure (Phase 4)

Closes the remaining P2 items from the v0.3 foundation epic:

| Issue | Deliverable |
|-------|-------------|
| #14 | [`../../benchmarks/`](../../benchmarks/README.md) harness + published baseline template |
| #15 | [Chaos / HA qualification](CHAOS.md) + crate tests |
| #16 | [`../../scripts/upgrade-rollback.sh`](../../scripts/upgrade-rollback.sh) N→N+1 contract |
| #17 | [Compatibility matrix](COMPATIBILITY.md) + [SLOs](SLOS.md) |

These documents record **what is measured in-tree** versus **what still requires a live FluxVM lab**. They do not reintroduce the fictional etcd-cluster / memory-live-migration claims removed in 0.2.0.
