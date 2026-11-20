# FluxVM Network Fabric (VM edge dataplane)

Fabric exposes FluxVM **Network Fabric** — a TC/eBPF classifier on each VM’s
host-visible edge — as first-class **API · Web · CLI**. The status payload
shows the schema number FluxVM returns (**11** on a current attach). Do not
require `schema_version == 4`. This is **not** Fabric’s host SDN
(`/api/network-policies` → nftables). Both planes can run together.
Service Fabric is not attached to bridge-less direct taps.

| Layer | Owns | Surface |
| --- | --- | --- |
| **Fabric SDN** | Host isolation (label → nftables) | `/api/network-policies` · **Net Security → Policies** |
| **VM edge (Network Fabric)** | Per-VM L3/L4 allowlists, Mbps/PPS, stats, LRU flows on TAP/netns. Current attach schema is 11 | `/api/vms/{name}/dataplane/*` · VM → **Dataplane** · `fabricctl dataplane` |

Kernel program and safety properties live in FluxVM:
[Network Fabric architecture](https://github.com/zyvorai/fluxvm#network-fabric-architecture-how-it-works) ·
[docs/network-fabric.md](https://github.com/zyvorai/fluxvm/blob/main/docs/network-fabric.md).

Fabric-side diagrams (control plane, packet path, modes):
[Network Fabric architecture](../network-fabric-architecture.md).

---

## What operators get

| Capability | Detail |
| --- | --- |
| Live policy | In-place BPF map rewrite (deny-all window only — never allow-all). Lab ~100–120 ms p50 via Fabric → FluxVM |
| Dual-stack L3+L4 | `allow_cidrs` + `tcp/PORT` / `udp/PORT` (both dimensions must match when both set) |
| Egress caps | `max_egress_mbps` / `max_egress_pps` on the same classifier |
| Telemetry | Allow/drop counters + LRU flows with stable **identity** |
| Bootstrap | ARP/DHCP/NDP/DHCPv6 always allowed |
| Platform readiness | Fabric `GET /readyz` (store + FluxVM `/readyz`); unauthenticated |
| Tenant | Create with `tenant` / `labels.tenant` → FluxVM; `GET /api/vms?tenant=` |
| Attach backends | QEMU / Cloud Hypervisor / Firecracker — scheduler attaches on **all** backends when an iface exists |
| Soft skip | `mode=ebpf` + `network.mode=none` / user NAT (no host-visible iface) → soft-skip even when `required=true` |
| GA fail-closed | `required=true` + TAP/netns edge present but attach fails → create/start errors |

---

## Enable on the FluxVM side

Ship [`configs/fluxvm-dataplane.toml`](../../configs/fluxvm-dataplane.toml)
(compose/k8s already mount it as `/etc/fluxvm.toml`):

```toml
[sandbox.dataplane]
mode = "ebpf"                                          # legacy | ebpf | cilium
bpf_object = "/usr/lib/fluxvm/bpf/fluxvm_tc.bpf.o"    # direct taps also need fluxvm_direct.bpf.o beside this
pin_root = "/sys/fs/bpf/fluxvm"
required = true                                        # GA: fail-closed when a VM edge exists
```

Requirements:

1. BPF objects present in the FluxVM image (`/usr/lib/fluxvm/bpf/fluxvm_tc.bpf.o` and, for a direct uplink, `fluxvm_direct.bpf.o` in the same directory).
2. Host `/sys/fs/bpf` mounted into the FluxVM process (compose/k8s/systemd).
3. Raised memlock (`LimitMEMLOCK=infinity` / `ulimit -l unlimited` / `SYS_RESOURCE`).
4. Bridged Fabric VMs use `network_tap: true` → FluxVM `NetworkSpec::Tap { netns: true }` so the classifier attaches on the **host** veth (`vh…`). A bridge-less uplink is `direct_uplink` (FluxVM `direct.mode = "l2-uplink"`, `netns: false`). Service Fabric is not applied to that tap.

GA default in `configs/fluxvm-dataplane.toml` is already `required = true`.
Confirm the schema number FluxVM returns (11 on a current attach) and `attached=true` on a bridged VM after deploy. Do not require `schema_version == 4`.

---

## REST surface (Fabric ↔ FluxVM)

| Fabric | FluxVM | Role |
| --- | --- | --- |
| `GET /api/vms/{name}/dataplane/status` | `GET /v1/vms/{id}/network/status` | mode, attached, schema, identity, iface, policy snapshot; `pod_ingress_required` / `pod_ingress_attached` when FluxVM reports them |
| `GET /api/vms/{name}/dataplane/policy` | `GET /v1/vms/{id}/network/policy` | Durable policy |
| `POST /api/vms/{name}/dataplane/policy` | `POST /v1/vms/{id}/network/policy` | Replace durable policy + live maps |
| `POST /api/vms/{name}/dataplane/policy/control` | read-modify-write FluxVM policy | Guard / Audit / Open / Invert / Block / Allow |
| `GET /api/vms/{name}/dataplane/explain?dest=&port=&proto=` | — | Explain dest:port verdict under current policy |
| `GET /api/vms/{name}/dataplane/dry-run?limit=` | — | Live flows that would drop under Guard |
| `GET /api/dataplane/templates` | — | Policy templates (open, guard, web, dns-only, no-world) |
| `GET /api/vms/{name}/dataplane/effective` | `GET /v1/vms/{id}/network/effective` | Declared + group-merged policy |
| `GET /api/vms/{name}/dataplane/stats` | `GET /v1/vms/{id}/network/stats` | allow/drop packets + bytes; optional `pod_policy` counters |
| `GET /api/vms/{name}/dataplane/flows?limit=` | `GET /v1/vms/{id}/network/flows` | LRU flows (`family` 4/6, identity, verdict) |
| `GET /api/vms/{name}/dataplane/drop-reasons` | `GET /v1/vms/{id}/network/drop-reasons` | Aggregated drop-reason histogram |
| `GET/POST/DELETE /api/vms/{name}/dataplane/pod-policy` | `…/network/pod-policy` | Per-VM pod-ingress policy |
| `GET/POST/DELETE /api/dataplane/groups[/{name}]` | `/v1/network/groups…` | Security-group CRUD |
| `GET/POST/DELETE /api/dataplane/cnp[/{name}]` | `/v1/network/cnp…` | CNP apply/list/delete |
| `GET /api/dataplane/identities` | `GET /v1/network/identities` | Reserved + group identities |
| `GET /api/dataplane/endpoints` | `GET /v1/network/endpoints` | CEP-*shaped* views (`identity_source`) |
| `GET /api/dataplane/microvm-metrics` | MicroVM `:9108/metrics` | Schedule→Running histograms (proxy) |
| `GET /api/dataplane/observe` | `GET /v1/network/observe` | Snapshot identities/groups/CNPs/VMs |
| `GET /api/dataplane/hubble/flows` | `GET /v1/network/hubble/flows` | Hubble-lite packet flows + hops |
| `GET /api/dataplane/health` | `GET /v1/network/health` | Dataplane health |
| `GET /api/dataplane/ipcache` | `GET /v1/network/ipcache` | Guest IP → identity |
| `POST /api/dataplane/refresh-dns` | `POST /v1/network/refresh-dns` | Re-resolve FQDN allowlists |

### Service Fabric v6 (BPF schema 4 — Maglev VIP LB)

Orthogonal to per-VM policy. Fabric fans out through `service-lb`; FluxVM owns
programs/maps (program generation 6). Full contract:
[ebpf-service-fabric.md](../../ebpf-service-fabric.md).

**Maglev + VM edge:** after VIP DNAT, FluxVM service TC returns `TC_ACT_OK` and stops
the clsact chain — Maglev-forwarded flows do not need backend ports in VM
`allow_ports`.

**North-south / HA:** configure FluxVM `[sandbox.dataplane.service]
north_south_interfaces` on edge nodes. Services need `exposure` north-south or both;
north-south NAT requires `snat_address`. Fabric proxies HA delta and advertisement
endpoints below; FluxVM must have north-south TC pinned for them to return data.

| Fabric | FluxVM | Role |
| --- | --- | --- |
| `GET/POST /api/dataplane/services` | `/v1/network/services` | List / upsert Maglev service |
| `GET/DELETE /api/dataplane/services/{name}` | `/v1/network/services/{name}` | Get / delete |
| `GET /api/dataplane/services/status` | `/v1/network/services/status` | Host schema / interfaces / XDP |
| `GET /api/dataplane/services/stats` | `/v1/network/services/stats` | Counters |
| `GET /api/dataplane/services/health` | `/v1/network/services/health` | Backend health |
| `POST …/health/reconcile` | `POST …/health/reconcile` | TCP probes |
| `POST …/conntrack/gc` | `POST …/conntrack/gc` | Affinity / NAT GC |
| `GET …/advertisements` | `GET …/advertisements` | VIP advertise snapshot |
| `GET …/flows` | `GET …/flows` | FluxScope service flows |
| `POST …/telemetry/export` | `POST …/telemetry/export` | OTLP/HTTP JSON export |
| `GET …/{name}/conntrack/delta` | `GET …/{name}/conntrack/delta` | HA delta export |
| `POST …/{name}/conntrack/delta/import` | `POST …/{name}/conntrack/delta/import` | Apply HA delta batch |
| `POST …/{name}/conntrack/delta/ack` | `POST …/{name}/conntrack/delta/ack` | Advance source watermark |
| `GET/POST …/policies` | `/v1/network/services/policies` | Identity + L7 policy |
| `GET/DELETE …/{name}/policy` | `/v1/network/services/{name}/policy` | Get / delete policy |
| `GET …/{name}/l7/envoy` | `/v1/network/services/{name}/l7/envoy` | Envoy redirect contract |

CLI: `fabricctl dataplane service …` (incl. `flows` / `export-telemetry` / `delta`). Console: **Edge Dataplane → Services**.

Observe pack (explain, dry-run Guard, templates): [dataplane-observe-pack.md](../../dataplane-observe-pack.md).

Capability probe (dashboard health card):

```http
GET /api/capabilities → vm_dataplane: { phase, detail }
```

When a running sample VM exists with eBPF attached, detail looks like
`mode=ebpf · attached · schema=4`. With `mode=legacy`, phase is `off`.

### Policy JSON shape

```json
{
  "default_allow": false,
  "allow_cidrs": ["0.0.0.0/0", "::/0"],
  "allow_ports": ["tcp/80", "tcp/443", "udp/53"],
  "deny_cidrs": ["10.66.0.0/16"],
  "allow_icmp": true,
  "groups": ["web"],
  "labels": ["app=web"],
  "allow_fqdns": [],
  "entities": [],
  "audit_mode": false,
  "deny_udp": false,
  "max_egress_mbps": 100,
  "max_egress_pps": 10000,
  "sample_rate": 1
}
```

Ports **must** be `tcp/PORT` or `udp/PORT` (optionally `tcp/8000-8999`). Bare
`443` is rejected by the UI and ignored/mis-parsed by the dataplane.
ICMP may use `icmp/0` / `icmp6/0`. Console: **Edge Dataplane** (`/app/edge-dataplane`)
for cluster groups/CNP/health; VM → **Dataplane** for per-VM policy + **Effective**.

---

## Web console UX

### Dashboard

**VM dataplane** capability card — Live / Off / Unreachable with FluxVM mode /
attach / schema detail (from `GET /api/capabilities`).

### VM detail → Dataplane tab

Also reachable from the Network tab teaser (**Open Dataplane**).

| Tab | Contents |
| --- | --- |
| **Status** | mode, attached, schema version/compat, policy synced, required, interface, identity, pin dir, **active policy snapshot** |
| **Policy** | Presets, Guard/Audit/Open/Invert/Block/Allow controls, allow/deny CIDRs, ports, groups/labels, FQDNs, entities, ICMP, audit, Mbps/PPS, Advanced JSON |
| **Effective** | Declared + group-merged policy JSON |
| **Stats** | Allowed/dropped packets + bytes, drop rate, Refresh counters |
| **Flows** | LRU table with **Identity**, family, 5-tuple, proto, verdict, packets, bytes, last seen; Block-from-flow; limit + auto-refresh |

Also: console **Edge Dataplane** (`/app/edge-dataplane`) — Health · Groups · CNP · Identities · Observe · **Packet flow** · Ipcache.

Soft banner when `vm_dataplane` capability is off/unreachable (`SubsystemBanner`).

### Lab UX checklist (verified)

On a bridged running VM with `mode=ebpf`:

1. Sign in → Dashboard shows **VM dataplane · Live · mode=ebpf · attached · schema=4**.
2. Open VM → **Dataplane → Status** — attached yes, schema 4, policy snapshot populated.
3. **Policy** — add `tcp/22`, deny CIDR, labels, **Save policy** → `POST …/dataplane/policy` returns 200.
4. **Effective** — membership shows matched groups after attaching a group.
5. Open **Infrastructure → Edge Dataplane** — Health ok; create a group; apply a CNP; Observe lists endpoints.
6. **Stats** — counters move after guest traffic (or host-side generators).
7. **Flows** — rows with matching identity (`sample_rate ≥ 1`).

Hands-on: [Tutorial 09](../../tutorials/09-edge-dataplane.md) ·
[edge-dataplane series](../../tutorials/edge-dataplane/README.md).

---

## CLI (`fabricctl`)

`fabricctl list` decodes the paginated `GET /api/vms` envelope (`{items, total, …}`),
not a bare JSON array. Root help is Cilium-style grouped with emoji markers
(`fabricctl --help`); `--color auto|always|never` colorizes tables/status
(honors `NO_COLOR`).

```bash
# HTTPS labs (self-signed cert accepted when URL is https://)
export ZYVOR_FABRIC_URL=https://127.0.0.1:9095
export ZYVOR_FABRIC_TOKEN="$(curl -sk -X POST "$ZYVOR_FABRIC_URL/api/auth/login" \
  -H 'Content-Type: application/json' \
  -d '{"username":"admin","password":"YOUR_PASSWORD"}' \
  | python3 -c 'import sys,json;print(json.load(sys.stdin)["token"])')"

fabricctl status                # Fabric API + dataplane checklist
fabricctl config                # effective --server / token / color
fabricctl list -o json          # items[] from paginated /api/vms
fabricctl dataplane status <name> -o json
fabricctl dataplane policy get <name> -o json
fabricctl dataplane policy set <name> --file /tmp/dp-policy.json
fabricctl dataplane effective <name> -o json
fabricctl dataplane stats <name> -o json
fabricctl dataplane flows <name> --limit 20 -o json
fabricctl dataplane health -o json
fabricctl dataplane group list -o json
fabricctl dataplane cnp list -o json
fabricctl dataplane observe -o json
fabricctl dataplane refresh-dns -o json
fabricctl dataplane hubble --style color   # default: color on TTY, plain when piped
fabricctl completion zsh > ~/.zfunc/_fabricctl
```

Aliases: `FABRIC_URL`, `FABRIC_TOKEN` (same as `ZYVOR_FABRIC_*`). Default URL remains `http://localhost:9095`
for local Docker eval. Overrides: `--server`, `--token`.

---

## Create a bridged VM that attaches

`POST /api/vms` provisions a disk image under Fabric storage (copy/reflink from
`image`, or `qemu-img create`) so clone and start have a backing file.

```bash
# Fabric API — network_tap enables Tap+netns
curl -sk -X POST "$ZYVOR_FABRIC_URL/api/vms" \
  -H "Authorization: Bearer $ZYVOR_FABRIC_TOKEN" \
  -H 'Content-Type: application/json' \
  -d '{
    "name": "lab-dp",
    "cpus": 1,
    "memory": 1024,
    "disk": 8,
    "image": "/var/lib/fluxvm/images/noble-server-cloudimg-amd64.img",
    "network_tap": true
  }'

curl -sk -X POST "$ZYVOR_FABRIC_URL/api/vms/lab-dp/start" \
  -H "Authorization: Bearer $ZYVOR_FABRIC_TOKEN"

# Wait until state=running, then:
fabricctl dataplane status lab-dp -o json
# expect: mode=ebpf, attached=true, schema_version=11
```

A bridge-less uplink (mutually exclusive with `network_tap` and user-mode NAT).
Service Fabric is not attached to this tap:

```bash
curl -sk -X POST "$ZYVOR_FABRIC_URL/api/vms" \
  -H "Authorization: Bearer $ZYVOR_FABRIC_TOKEN" \
  -H 'Content-Type: application/json' \
  -d '{
    "name": "lab-direct",
    "cpus": 1,
    "memory": 1024,
    "disk": 8,
    "image": "/var/lib/fluxvm/images/noble-server-cloudimg-amd64.img",
    "direct_uplink": "enp1s0",
    "direct_guest_ips": ["192.168.1.50"]
  }'
```

FluxVM sees that as:

```json
{"network":{"mode":"tap","direct":{"outer":"enp1s0","mode":"l2-uplink","guest_ips":["192.168.1.50"]}}}
```

User-mode NAT / `network.mode=none` VMs do **not** attach eBPF (no host edge
iface). That is expected.

---

## Troubleshooting

| Symptom | Check |
| --- | --- |
| `attached=false`, mode=ebpf | Bridged/`network_tap` or direct uplink? Both BPF objects (`fluxvm_tc.bpf.o` and `fluxvm_direct.bpf.o`)? memlock? `/sys/fs/bpf` writable? |
| Direct tap has no Service Fabric | Expected. FluxVM does not attach Service Fabric to bridge-less direct taps |
| `mode=legacy` | `[sandbox.dataplane] mode` in `/etc/fluxvm.toml` |
| Policy POST 4xx on ports | Use `tcp/443`, not `443` |
| `fabricctl` 401 | Set `ZYVOR_FABRIC_TOKEN` from `/api/auth/login` |
| `fabricctl` TLS errors | Use `https://` URL (client accepts self-signed) |
| Metrics all zero on stopped VM | Expected when VM is in Fabric store but not registered in FluxVM (`200`, not `404`) |
| Auth file ≠ auth.db after deploy | `FORCE_ADMIN_RESET=1 FABRIC_LAB_DEFAULTS=1 ./scripts/deploy remote …` |
| Dashboard card stuck “Checking…” | First `/api/capabilities` before login is 401; refresh after sign-in |
| Conflating SDN vs edge | Net Security policies ≠ Dataplane tab |

```bash
# FluxVM direct
curl -s http://127.0.0.1:7788/v1/vms | jq .
# Host pins
sudo ls /sys/fs/bpf/fluxvm/vms/
sudo cat /run/fluxvm/ebpf/vms/*/iface /run/fluxvm/ebpf/vms/*/schema_version
```

---

## Related docs

- [FluxVM driver](fluxvm.md) — full driver surface
- [Service Fabric v6 (BPF schema 4)](../../ebpf-service-fabric.md) — Maglev VIP LB / leases / HA / policy / health / EDT / flows
- [Networking](../../networking.md) — Fabric SDN + bridges + this plane
- [Web UI](../../web-ui.md) — console surfaces
- [User: VM Dataplane](../../user/pages/infrastructure/dataplane.md)
- [User: Edge Dataplane](../../user/pages/infrastructure/edge-dataplane.md)
- FluxVM [network-fabric.md](https://github.com/zyvorai/fluxvm/blob/main/docs/network-fabric.md) · [service-fabric.md](https://github.com/zyvorai/fluxvm/blob/main/docs/service-fabric.md) · [ebpf-cilium.md](https://github.com/zyvorai/fluxvm/blob/main/docs/ebpf-cilium.md)
