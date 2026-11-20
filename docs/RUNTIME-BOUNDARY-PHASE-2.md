# Phase 2 roadmap — receiver + production cutover

This merge kit intentionally establishes the ownership boundary before adding more cross-repo surface.

## 1. FluxVM target receiver

**Status:** Wired in FluxVM and proxied by Fabric
(`POST /api/vms/{name}/migration/native/prepare-receiver`, activate/abort). See
[FLUXVM-FABRIC-BOUNDARY.md](FLUXVM-FABRIC-BOUNDARY.md). Native live migration
remains **preview** until production gates below pass.

Add a target-side operation that creates an **incoming** QEMU instance without first cold-booting the guest:

- `POST /v1/migration/receivers`
- validates target CPU model, machine type, memory, device topology and image identity;
- reserves a target VM UUID/workspace/cgroup/netns;
- creates or opens the target disk according to the supplied storage contract;
- launches QEMU with `-incoming defer` (or an equivalent safe receiver state);
- returns a one-time migration URI/token and receiver expiry;
- `POST /v1/migration/receivers/{id}/activate` / `DELETE ...`;
- reconciles abandoned receivers on daemon restart.

The receiver must be a runtime primitive only. It must not choose the node.

## 2. Fabric migration transaction

Build a durable Fabric transaction around the source/target runtimes:

`Plan -> Reserve -> PrepareStorage -> PrepareReceiver -> Transfer -> Cutover -> Commit -> Cleanup`

Persist every transition. Add idempotency keys, timeout policy and rollback actions. The transaction owns:

- DRS / requested target selection;
- maintenance and anti-affinity checks;
- CPU/device/network compatibility;
- migration-network selection;
- Ceph/shared-storage verification;
- target receiver lifecycle;
- source transport lifecycle;
- inventory commit and audit;
- cancellation, fencing and recovery.

## 3. Storage contract

Runtime contract v1 requires shared storage. v2 should add explicit strategies:

- `shared-rbd` — no disk copy;
- `block-precopy` — runtime-supported dirty-block transfer;
- `snapshot-seed` — Fabric prepares target disk from a portable snapshot, then memory migrates;
- `cold-copy` — offline fallback.

Do not hide storage strategy behind a generic `live=true` flag.

## 4. Production gates

Native live migration is GA only after automated real-host tests cover:

- QEMU pre-copy over two KVM hosts;
- multifd;
- post-copy cancellation/recovery;
- source/target daemon restart during preparation;
- tenant isolation;
- eBPF policy re-attachment on destination;
- Ceph RBD shared-disk migration;
- failed target / failed source / broken migration network;
- max-downtime enforcement and metrics;
- no double-running VM after commit/rollback.
