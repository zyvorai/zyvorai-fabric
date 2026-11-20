# The VM driver: FluxVM

Zyvor Fabric's VM lifecycle runs through `driver-core::VmDriver`, implemented against [FluxVM](https://github.com/zyvorai/fluxvm) — a standalone disposable-VM control plane with no systemd dependency, spoken to over its REST API and its vsock guest agent. FluxVM itself can run guests on **QEMU/KVM, Cloud Hypervisor, Firecracker, or the in-tree FluxVM hypervisor** (`backend: "flux-vm"`, agent-sandbox track). Fabric talks to whichever backends FluxVM has configured; there is no separate Fabric-side VMM picker.

This page covers what's wired up today, what isn't yet, and how to configure it.

## Configuration

```toml
[driver]
fluxvm_url = "http://127.0.0.1:7788"   # FluxVM's REST API base URL
# fluxvm_token = "..."                  # only if FluxVM has auth.tokens configured
# microvm_metrics_url = "http://127.0.0.1:9108/metrics"
# [[driver.fluxvm_nodes]]               # extra nodes for Service Fabric + migration receivers
# name = "node-b"
# url = "http://10.0.0.2:7788"
```

See [FluxVM's own README](https://github.com/zyvorai/fluxvm#readme) for running `fluxvm serve` itself.

For Network Fabric (TC/eBPF VM-edge dataplane — groups, CNP,
health/ipcache), Fabric ships [`configs/fluxvm-dataplane.toml`](../../configs/fluxvm-dataplane.toml)
and mounts it as `/etc/fluxvm.toml` in compose/k8s. A current attach reports
**schema 11**. `mode = "ebpf"` is required for the bridge-less direct datapath,
which also needs `fluxvm_direct.bpf.o` next to `fluxvm_tc.bpf.o`. Service Fabric
is not attached to direct taps.

```toml
[sandbox.dataplane]
mode = "ebpf"
bpf_object = "/usr/lib/fluxvm/bpf/fluxvm_tc.bpf.o"
pin_root = "/sys/fs/bpf/fluxvm"
required = true   # fail-closed when a host-visible VM edge exists
```

Operator UX: [fluxvm-dataplane.md](fluxvm-dataplane.md) · tutorials
[09-edge-dataplane.md](../../tutorials/09-edge-dataplane.md).

Production alignment with FluxVM: Fabric `GET /readyz`, create `tenant` /
`labels.tenant`, `GET /api/vms?tenant=`, `fabricctl create --tenant`, and JWT
`tenant` claim enforcement (create mismatch → 403; get/mutate other tenant → 404).
When FluxVM enables `[[auth.tokens]]`, set `driver.fluxvm_token`. Optional
`network.hubble_ui_url` is an external Hubble link only — see
[hubble-ui.md](../operations/hubble-ui.md). FluxVM production bar:
[PRODUCTION.md](https://github.com/zyvorai/fluxvm/blob/main/docs/PRODUCTION.md)
and [production tutorials](https://github.com/zyvorai/fluxvm/blob/main/docs/tutorials/production/README.md).

The FluxVM image must include both BPF objects (`fluxvm_tc.bpf.o` and
`fluxvm_direct.bpf.o`); the DaemonSet/compose also mounts host `/sys/fs/bpf` and raises memlock (`SYS_RESOURCE` / `ulimit memlock=-1`).

## Wired vs missing (FluxVM 0.4.x catch-up)

| Area | Fabric status |
| --- | --- |
| VM lifecycle, agent shell/console/files, resources, freeze/thaw | Wired |
| Guest **pause/resume** (`/api/vms/{name}/pause\|resume`) | Wired → FluxVM pause/resume |
| Image catalog + warm pools | Wired |
| Network Fabric (policy/status/stats/flows/effective/groups/CNP/…; schema is the number FluxVM returns, 11 on a current attach) | Wired |
| Bridge-less direct tap (`network.direct`, `l2-uplink`) | Wired on create and NIC hotplug |
| Drop-reasons + pod-policy proxies | Wired |
| Service Fabric Maglev + remote ipcache fan-out | Wired |
| CEP endpoints + MicroVM metrics + Hubble flows proxy | Wired |
| Runtime capabilities + source-side native migration | Wired |
| Migration **receivers** + network migration state | Wired (client + prepare/activate/abort APIs) |
| Storage backends (`default`/`lvm-thin`/`nbd`/`ceph-rbd`) + jailer fields on wire | Wired through create |
| QGA (Windows Kryton) | Wired (`/api/vms/{name}/qga/*`) |
| Secure Containers placement (`ContainerGroup` + `/readyz.secure_containers`) | Wired (placement + host nested readiness UX) |
| Pod-ingress hooks on `DataplaneStatus` | Wired (`pod_ingress_required` / `pod_ingress_attached`) |
| Per-direction Pod-policy counters on REST | Wired (`DataplaneStats.pod_policy` via `ppstat`/`prhit`) |
| Agent-sandbox product UI | Wired via **agent-runtime** (`/api/agents`, `/api/sessions` + web Agents/Sessions). FLUXKVM1 snapshot engine stays FluxVM lab-only |
| `fluxvm-kube` MicroVM CRDs / `fluxvm-agent` fleet | FluxVM co-deploy only — not absorbed into Fabric charts (see boundary doc) |

## What's wired today

The `fluxvm-driver`/`fluxvm-client` crates (`backend/crates/`) implement `driver-core`'s trait family against FluxVM's REST API. Every VM this driver creates requests FluxVM's vsock guest agent (`CreateVmRequest.agent.enabled: true`) by default, so shell exec, console, and file copy below work without any extra opt-in — FluxVM bakes in the agent and its auth token at create time, transparent to the caller.

Bridged VMs are created with `NetworkSpec::Tap { netns: true }` (per-VM network namespace + dnsmasq), not a shared host-bridge tap. A direct uplink is `NetworkSpec::Tap { netns: false, direct: { mode: "l2-uplink", … } }` and is mutually exclusive with that path and with user-mode NAT.

```json
{"network":{"mode":"tap","mac":"02:00:00:00:0a:0a","direct":{"outer":"enp1s0","mode":"l2-uplink","guest_ips":["192.168.1.50"]}}}
```

On Fabric that is `direct_uplink` plus optional `direct_guest_ips` on create (`fabricctl create --direct-uplink enp1s0 --direct-guest-ip 192.168.1.50`). The spec is stored on the VM and sent on first start. A later change is `PUT /api/vms/{name}/direct-uplink` (`fabricctl direct-uplink`), which drops the FluxVM record so the next start creates the new tap. NIC hotplug of a direct uplink is `POST /v1/vms/{id}/hotplug/nic`; a bridged hotplug stays on QMP. A hotplug onto a VM that is not already bridged or NAT is stored on the VM so a restart keeps it.

| Capability | `driver-core` trait | FluxVM endpoint(s) |
| --- | --- | --- |
| Create, list, get, resolve by name | `VMDriver` | `POST`/`GET /v1/vms`, `GET /v1/vms?name=`, `GET /v1/vms?tenant=` |
| Readiness | — | Fabric `GET /readyz` proxies FluxVM `GET /readyz` + local store |
| Start, stop, pause, resume, delete | `VMDriver` | `/v1/vms/{id}/{start,stop,pause,resume}`, `DELETE /v1/vms/{id}` |
| Hotplug (CPU/memory/disk) | — | Generic — resolves `VMDriver::get_control_socket` and speaks QMP directly |
| Hotplug NIC (bridge) | — | QMP `netdev_add` + `device_add` |
| Hotplug NIC (direct uplink) | — | `POST /v1/vms/{id}/hotplug/nic` with `direct` (FluxVM opens the tap and attaches `fluxvm_direct`) |
| CPU pinning (cgroup cpuset) | `ResourceControlDriver::{set,get}_cpuset` | `POST /v1/vms/{id}/resources`, `GET /v1/vms/{id}/cpuset` |
| CPU/memory/IO/pids limits | `ResourceControlDriver` | `POST /v1/vms/{id}/resources` |
| Point-in-time usage + PSI pressure | `ResourceStatsDriver` | `GET /v1/vms/{id}/{stats,pressure}` |
| Freeze/thaw (cgroup v2 freezer) | — | `POST /v1/vms/{id}/{freeze,thaw}`, `GET .../frozen` |
| Live console log streaming | `LogDriver` | `GET /v1/vms/{id}/logs?follow=true` |
| Shell exec (no SSH needed) | `ShellDriver::shell` | `POST /v1/vms/{id}/agent` — over FluxVM's vsock guest agent |
| Interactive console (real PTY) | `ConsoleDriver::open_console` | `GET /v1/vms/{id}/console` — a WebSocket, relayed end-to-end from the browser's own console tab through to a PTY-backed shell in the guest. No live terminal resize — the PTY is sized once at open time |
| File copy to/from the guest | `ShellDriver::{copy_to,copy_from}` | `POST /v1/vms/{id}/agent/{put,get}-file` — same vsock agent, base64-in-one-request, capped at 64MiB |
| SSH info | — | Resolves the VM's MAC (pinned at create time) to an IP via zyvor-fabricd's own DHCP lease file — no vsock/FluxVM call at all. `key_path` is always `null`; key management is the operator's own responsibility (e.g. cloud-init) |
| Bind-mount replacement (virtiofs) | `VMStartOptions.bind_mounts` (create-time only) | `CreateVmRequest.shared_folders` — one `virtiofsd` per share, auto-mounted in-guest via a generated cloud-init `/etc/fstab` entry |
| Image catalog CRUD, incl. read-only flag + orphaned-download cleanup | `ImageDriver` | `/v1/images/catalog` add/remove/rename/clone/export/read-only/clean |
| Pluggable storage + QGA enable | create path | `CreateVmRequest.storage` / `qga` / jailer fields on `VmRecord` |
| Native migration receivers | — | `/v1/migration/receivers` (+ activate); Fabric `…/migration/native/prepare-receiver` |
| Network migration state | — | `/v1/vms/{id}/network/migration/{state,quiesce,export,restore,resume}` |
| QGA | — | `/v1/vms/{id}/qga/{ping,exec,firewall/*}` via Fabric `/api/vms/{name}/qga/*` |
| **Network Fabric (VM edge dataplane, schema 11 on a current attach)** | `VmDataplaneDriver` | `/v1/vms/{id}/network/{policy,status,stats,flows,effective,drop-reasons,pod-policy}` + `/v1/network/{groups,cnp,identities,observe,health,ipcache,refresh-dns}` — proxied as Fabric `/api/vms/{name}/dataplane/*` and `/api/dataplane/*`. Status shows the schema FluxVM returns; Service Fabric is not attached to direct taps |
| **Service Fabric v6 (BPF schema 4 — Maglev VIP LB)** | `service-lb` + FluxVM services API | `/v1/network/services…` — proxied as `/api/dataplane/services…` (status/health/ads/GC/flows/delta/policy); see [ebpf-service-fabric.md](../../ebpf-service-fabric.md) |

### Fabric API and CLI for the dataplane

| Fabric | FluxVM |
| --- | --- |
| `GET /api/vms/{name}/dataplane/status` | `…/network/status` |
| `GET/POST /api/vms/{name}/dataplane/policy` | `…/network/policy` |
| `GET /api/vms/{name}/dataplane/effective` | `…/network/effective` |
| `GET /api/vms/{name}/dataplane/stats` | `…/network/stats` |
| `GET /api/vms/{name}/dataplane/flows?limit=` | `…/network/flows` |
| `GET /api/vms/{name}/dataplane/drop-reasons` | `…/network/drop-reasons` |
| `GET/POST/DELETE /api/vms/{name}/dataplane/pod-policy` | `…/network/pod-policy` |
| `GET/POST/DELETE /api/dataplane/groups[/{name}]` | `/v1/network/groups…` |
| `GET/POST/DELETE /api/dataplane/cnp[/{name}]` | `/v1/network/cnp…` |
| `GET /api/dataplane/{identities,observe,health,ipcache}` | matching `/v1/network/…` |
| `POST /api/dataplane/refresh-dns` | `POST /v1/network/refresh-dns` |
| `GET/POST/DELETE /api/dataplane/services…` | `/v1/network/services…` (+ health/ads/GC) |

```bash
# HTTPS labs — URL + JWT (self-signed accepted when URL is https://)
export ZYVOR_FABRIC_URL=https://127.0.0.1:9095
export ZYVOR_FABRIC_TOKEN=…   # from POST /api/auth/login

fabricctl dataplane status <name>
fabricctl dataplane policy get <name>
fabricctl dataplane policy set <name> --file policy.json
fabricctl dataplane effective <name>
fabricctl dataplane stats <name>
fabricctl dataplane flows <name> --limit 100
fabricctl dataplane health
fabricctl dataplane service list
fabricctl dataplane service status
fabricctl dataplane service health
fabricctl dataplane group list
fabricctl dataplane cnp list
fabricctl dataplane observe
```

**Do not confuse** this with Fabric's `/api/network-policies` (label→nftables SDN on the host). The VM-detail tab is labeled **Dataplane**; cluster UI is **Edge Dataplane** (`/app/edge-dataplane`).

Operator UX detail: [fluxvm-dataplane.md](fluxvm-dataplane.md) ·
[Tutorial 09](../../tutorials/09-edge-dataplane.md).
Log streaming's one fidelity reduction: raw serial console output has no journald-equivalent per-line priority/unit metadata, so every entry is stamped uniformly rather than carrying real per-line priority. Image catalog's `pull-tar`/`import-tar`/`export-tar` are permanently unsupported, not just for now — a tar rootfs isn't a bootable disk image for a real hardware VM, so building that would mean writing a full tar-to-bootable-image converter, a different project from wiring up an existing capability.

## Remaining gaps

`fluxvm-client`'s wire types are a **hand-synced mirror** of `fluxvm-core::model` — integration is out-of-process (REST, not a Cargo dependency on FluxVM's own crates), a deliberate trade for not coupling this repo's build to FluxVM's crate versions.

Still deferred / out of scope for Fabric:

- **Agent-sandbox / FLUXKVM1 product surface in fabricd** — FluxVM's `/v1/sandboxes`, in-tree KVM memory snapshots, AutoPause, L7 egress, and `/console` ops UI stay on FluxVM / [`agent-runtime`](../../../agent-runtime/). Fabric Snapshot Manager remains QMP Disk/Full for classic VMs.
- **Native live migration GA marketing** — receivers are wired; GA still requires a green e2e KVM-host test (see [FLUXVM-FABRIC-BOUNDARY.md](../../FLUXVM-FABRIC-BOUNDARY.md)).
- **Policy Observer `:9091`** — optional Prometheus scrape (see below); not a Fabric product UI.
- **Not applicable to this driver** — FluxVM's `fluxvm-kube` Kubernetes `DisposableVm` CRD/operator, and its `fluxvm-agent` distributed fleet registry.

### Policy Observer scrape (optional)

FluxVM Secure Containers Set 15 ships a read-only Policy Observer on `:9091`. Add a Prometheus job when running Sentinel:

```yaml
- job_name: fluxvm-policy-observer
  static_configs:
    - targets: ["127.0.0.1:9091"]
```

## See also

- [FluxVM README](https://github.com/zyvorai/fluxvm#readme) — the full feature set, storage backends, agent-sandbox track, Kubernetes operator, and distributed node-agent.
- [VM edge dataplane](fluxvm-dataplane.md) — SDN vs VM-edge, lab verify steps.
- [Operations guide](../operations/README.md) — the driver in the broader operational context.
- [Ownership boundary](../../FLUXVM-FABRIC-BOUNDARY.md)
