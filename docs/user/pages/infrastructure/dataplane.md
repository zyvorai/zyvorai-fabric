# VM Dataplane (Network Fabric schema v4)

## Purpose

Per-VM **edge** security and telemetry powered by FluxVM Network Fabric
**schema v4** (TC/eBPF on the host-visible TAP/netns interface). Use this to:

- Allowlist destinations and L4 ports (`tcp/PORT`, `udp/PORT`, `icmp/0`)
- Deny CIDRs that override allows
- Attach **security groups** / labels and inspect **effective** merged policy
- Cap egress Mbps/PPS and sample flows
- Inspect allow/drop counters — without touching Fabric’s host SDN

This is **not** the same as [Net Security](network-security.md) network policies
(label → host nftables). For cluster-wide groups/CNP/health UI, see
[Edge Dataplane](edge-dataplane.md).

## When to use it

- Lock down what a sandbox / CI / AI agent VM can reach
- Apply a live deny or rate limit without restarting the guest
- Prove what left the box (stats + flows)
- Confirm eBPF is attached. The schema version is the number FluxVM returns (11 on a current attach), not a hard-coded 4, before relying on `required=true`
- Debug group membership via the **Effective** tab

## How to get there

- Route: `/app/vms/:name` → tab **Dataplane**
- Shortcut: VM → **Network** → **Open Dataplane**
- Dashboard: capability card **VM dataplane** (`/app`)
- Cluster console: [Edge Dataplane](edge-dataplane.md) (`/app/edge-dataplane`)

## Operate from the console (UX)

### Prerequisites

1. FluxVM with `[sandbox.dataplane] mode = "ebpf"` (see operator guide).
2. VM with **bridged / network tap** (`network_tap: true`) so a host `vh…`
   exists for TC attach.
3. Write permission to change policy (viewers can inspect).

### Status

Confirm **Attached = yes**, **Mode = ebpf**, **Schema version = 4**, policy
synced, identity, pin dir, and the **Active policy snapshot** (CIDRs, ports,
deny, groups, ICMP, Mbps/PPS). When FluxVM reports them, note
**pod_ingress_required** / **pod_ingress_attached** on status (Secure Containers
/ pod-edge hooks).

### Policy

Cilium-style **packet-flow control** (VM edge only — no Cilium-private maps):

| Control | Effect |
|---------|--------|
| **Open** | Default allow, enforcement off |
| **Audit** | Evaluate policy, do not drop (`audit_mode`) |
| **Guard** | Default-deny enforcement + flow sampling |
| **Invert** | Swap allow/deny CIDRs and flip default allow |
| **Block** / **Allow** | Add host or CIDR to deny/allow lists |

Also: **Explain** dest:port, **Dry-run Guard**, and policy **templates** —
see [dataplane-observe-pack.md](../../../dataplane-observe-pack.md).

Flows tab **Block** adds that destination to `deny_cidrs`.
Cluster-wide Hubble-lite view: [Edge Dataplane → Packet flow](edge-dataplane.md).

1. Presets (**Allow all** / **Deny all** / **Web egress**) or edit tags.
2. Ports must look like `tcp/443` or `udp/53` (UI validates).
3. Optional **Deny CIDRs**, **Groups**, **Labels**, **FQDNs**, **Entities**,
   **Allow ICMP**, **Audit mode**.
4. Optional **Max egress Mbps** / **PPS** and **Sample rate** (≥1 for flows).
5. **Save policy** — live map update (brief over-deny possible; never an
   allow-all gap).

### Effective

JSON snapshot of **declared** policy, **membership** (matched groups /
identities), and **effective** merged policy (union of CIDRs/ports; tightest
rate limits; fail-closed default).

### Stats / Flows

Allow vs drop counters; LRU flow table with identity, family, 5-tuple, verdict.
Stats may include nested **`pod_policy`** counters when FluxVM exposes them.
Operators can also poll `GET /api/vms/{name}/dataplane/drop-reasons` and
manage `…/dataplane/pod-policy` (see operator guide).

## API & CLI (quick)

| Action | Surface |
|--------|---------|
| Status / policy / stats / flows / effective / drop-reasons / pod-policy | `/api/vms/{name}/dataplane/…` |
| Groups / CNP / health / observe / ipcache / refresh-dns | `/api/dataplane/…` |
| QGA (Windows Kryton) | `/api/vms/{name}/qga/{ping,exec,firewall/…}` |
| CLI | `fabricctl dataplane policy guard\|audit\|open\|invert\|block\|allow` |

Tutorials: [Tutorial 09](../../../tutorials/09-edge-dataplane.md) ·
[edge-dataplane series](../../../tutorials/edge-dataplane/README.md).

## Related pages

- [Edge Dataplane](edge-dataplane.md) — cluster groups/CNP/health/**Packet flow** console
- [Network](network.md) — NAT / bridge / port forwards
- [Net Security](network-security.md) — host SDN (orthogonal)
- [Virtual Machines](../core/vms.md)
- Operator guide: [VM edge dataplane](../../../guides/vm-drivers/fluxvm-dataplane.md)
- [Page index](../../PAGE_INDEX.md)
