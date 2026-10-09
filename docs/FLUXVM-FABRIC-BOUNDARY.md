# FluxVM ↔ Zyvor Fabric ownership contract

<!-- ZYVOR_RUNTIME_BOUNDARY_V1 -->

**GuestKit prepares the disk. FluxVM runs one machine. Fabric runs the cloud.**

## Ownership

| Capability | FluxVM | Fabric |
|---|---|---|
| QEMU / Cloud Hypervisor / Firecracker / in-tree KVM | owns | consumes |
| VMM process lifecycle and device state | owns | orchestrates |
| TAP/netns and VM-edge TC/eBPF | owns | policy + UX |
| Memory/device snapshots | owns mechanism | catalog/retention/DR |
| Live-migration VMM transport | owns mechanism | destination, reservation, audit, rollback |
| VFIO/SR-IOV/mdev attach | owns mechanism | inventory + scheduling |
| Node image cache | owns | populates |
| Global content library | no | owns |
| Host placement / DRS / HA / fencing / evacuation | no | owns |
| Multi-site replication / site recovery | no | owns |
| Tenant/project/RBAC/quotas/billing | minimal node auth only | owns |
| Kubernetes `MicroVM` ephemeral runtime | standalone FluxVM surface | no |
| Kubernetes private-cloud `VirtualMachine` | no | Fabric operator |
| Secure Containers shim + guest-agent + microVM lifecycle | owns mechanism | no |
| `ContainerGroup` placement / scheduling / lifecycle API | no | owns |
| Container Pod execution (CRI, `RuntimeClass fluxvm`, kubelet) | no | consumes only |

### `ContainerGroup` (FluxVM Secure Containers)

Fabric's `ContainerGroup` workload (`POST /api/container-groups/apply`) is a
Kubernetes-native sibling to `VirtualMachine`, but with a different execution
path: Fabric decides **placement** (`predictive_drs::DrsManager`, the same
engine scoring VM placement, filtered to hosts with `secure_containers_ready`)
and pins the host via a Pod's `spec.nodeName`, but Fabric never talks to
FluxVM's Secure Containers shim or guest-agent directly — only to the target
cluster's Kubernetes API (create/delete the Pod) and to FluxVM's `/readyz`
for capability discovery. The shim (`containerd-shim-fluxvm-v2`), guest-agent,
and the microVM it creates per Pod remain entirely FluxVM's mechanism,
unchanged from the standalone `RuntimeClass fluxvm` path FluxVM already
documents (`docs/secure-containers.md`, `docs/secure-containers-set3.md` in
the fluxvm repo) — Fabric adds a placement/scheduling layer in front of it,
it does not replace or re-implement it. For the fabric-side operator
walkthrough (node setup → host registration → `ContainerGroup` apply →
verification), see [container-groups.md](container-groups.md).

## Migration contract v1

Fabric discovers FluxVM with `GET /v1/runtime/capabilities` and drives the
source runtime through typed endpoints. The legacy Fabric migration path that
used iterative `rsync` and finally `machinectl start` is fail-closed for
`live=true`; it must not be used with the FluxVM-only architecture.

The `migration::RuntimeMigrationManager` prepared-target API:

1. select and reserve the target node;
2. validate CPU/device/network compatibility;
3. confirm shared storage or prepare identical target storage;
4. create/arm the target FluxVM incoming receiver (`POST /v1/migration/receivers`);
5. select a protected migration network/URI (`tcp:<host>:<port>` from the receiver);
6. optionally quiesce/export VM-edge network state;
7. call source-side migration and monitor status;
8. activate the receiver, restore/resume network state, update Fabric inventory;
9. roll back/fence on failure according to HA policy.

Step 4 is implemented in FluxVM and proxied by Fabric
(`POST /api/vms/{name}/migration/native/prepare-receiver`,
`POST /api/migration/receivers/{id}/activate`,
`DELETE /api/migration/receivers/{id}`). Native live migration remains
**preview** until an end-to-end KVM-host test passes; do not market it as GA
without that gate.

Fabric exposes the control-plane contract:

| Fabric API | Role |
|---|---|
| `GET /api/runtime/capabilities` | Proxy FluxVM runtime contract |
| `POST /api/vms/{name}/migration/native/prepare-receiver` | Arm target incoming QEMU |
| `POST /api/vms/{name}/migration/native/start` | Start prepared-target transport |
| `GET /api/vms/{name}/migration/native/status` | Poll progress |
| `POST /api/vms/{name}/migration/native/cancel` | Cancel in-flight transport |
| `GET /api/vms/{name}/migration/native/network-state` | Dataplane migration phase |
| `POST /api/migration/receivers/{id}/activate` | Promote receiver after cutover |
| `DELETE /api/migration/receivers/{id}` | Abort unused receiver |

CLI: `fabricctl runtime capabilities` and `fabricctl runtime migrate …`
(`prepare-receiver`, `activate-receiver`, `abort-receiver`, `start`, `status`, `cancel`).

## Service Fabric fan-out (v6)

`POST/DELETE /api/dataplane/services` go through `service-lb` with Service Fabric
**v6** (BPF schema **4** ABI / FluxVM program generation **6**): conntrack affinity,
ready/draining/unhealthy backends, optional TCP health checks, VIP `advertise`
intent, Fabric `EdgeLease` / durable lease controller, sequence/ack HA delta
replication (full-snapshot fallback on gaps), `max_egress_mbps`, `flow_sample_rate`,
`host_routing`, plus **identity/L7 service policy** transactions. FRR/BIRD/File
adapters own BGP sessions; FluxVM only publishes local ads.

Additional proxies: `/api/dataplane/services/{status,stats,health,advertisements,flows}`,
`POST …/health/reconcile`, `POST …/conntrack/gc`, `POST …/telemetry/export`,
`GET/POST …/conntrack/delta` (+ `/import`, `/ack`),
`GET/POST …/policies`, `GET/DELETE …/{name}/policy`, `GET …/{name}/l7/envoy`.

FluxVM must expose north-south service TC (`north_south_interfaces`) before HA deltas
or VIP advertisements are meaningful; north-south NAT services need `snat_address`.
Maglev DNAT on the VM edge returns `TC_ACT_OK` so per-VM sandbox policy does not
require backend ports in `allow_ports` for VIP-forwarded flows.

Operator docs: [ebpf-service-fabric.md](ebpf-service-fabric.md) ·
FluxVM [service-fabric.md](https://github.com/zyvorai/fluxvm/blob/main/docs/service-fabric.md) ·
Examples: [examples/service-fabric-v3/](examples/service-fabric-v3/).

## FluxVM routes Fabric consumes beyond migration

Fabric talks to FluxVM over REST only. These routes are wired in `fluxvm-client` and Keep's
`FluxVm` client. "Live" means exercised against a real FluxVM on KVM; everything else is
tested against in-process fakes.

| FluxVM route | Used by | Status |
| --- | --- | --- |
| `Idempotency-Key` on `POST /v1/vms`, `DELETE /v1/vms/{id}`, snapshot, fork | `fluxvm-client` | Live |
| `POST /v1/vms/{id}/fork`, snapshots list/restore/delete, `?ready=exec` | `fluxvm-client` | Live (needs the `flux-vm` backend on the KVM engine) |
| `POST /v1/sandboxes/{id}/speculate`, `changesets/*` approve, reject, apply | Keep `POST /v1/sessions/{id}/speculate`; a human approval applies it | Client shape live; Keep handler fake-tested |
| `POST /v1/sandboxes/{id}/grants` | Keep `/v1/sessions/{id}/grants`, only for credentials with no Keep-side limits | Fake-tested |
| `policy` / `enforcement` on guest exec | `agent_exec_confined`, `FluxVm::process_confined` (fails closed) | Fake-tested |
| `GET\|POST /v1/vms/{id}/balloon`, `GET /v1/vms/{id}/memory` | `fabricctl memory\|balloon`, fabricd `/vms/{name}/memory\|balloon` | **Beta**: FluxVM itself lists balloon as not live-verified |
| `POST /v1/vms/{id}/cdroms/{name}/eject` | `FluxVmClient::eject_cdrom` | Fake-tested |
| `backend: "vz"` | `BackendKind::Vz` (VMs on a macOS FluxVM host) | Decodes and encodes; not run on a Mac |

`POST /v1/sandboxes` is not covered by FluxVM's idempotency, so Keep does not send a key there.

## Standalone FluxVM fleet mode

`fluxvm-agent` stays useful as a lightweight standalone multi-host option. It
is not the home for Fabric-class etcd HA, DRS, datacenters, fencing, site
recovery, tenant placement or enterprise content-library semantics.

## Optional FluxVM co-deploy (MicroVM / DisposableVm CRDs)

Fabric's k8s-native path is its own operator CRDs (`VirtualMachine`,
`ContainerGroup`) → Fabric REST → FluxVM `serve :7788`. FluxVM also ships
standalone CRDs (`DisposableVm`, `MicroVM`) and `fluxvm-agent :7799` under
`deploy/k8s/` in the FluxVM repo.

**Do not copy those CRDs into the Fabric Helm chart** — that would create dual
control planes. After installing the Fabric chart (which may already ship a
FluxVM DaemonSet for `:7788`), operators who also want kubectl-driven
ephemeral MicroVMs can optionally:

```bash
kubectl apply -f https://raw.githubusercontent.com/zyvorai/fluxvm/main/deploy/k8s/crd.yaml
# plus microvm CRDs / node-agent manifests as documented in FluxVM
```

Fabric multi-host placement continues to use `[[driver.fluxvm_nodes]]` + DRS,
not `fluxvm-agent` central.
