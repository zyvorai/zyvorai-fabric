# Edge dataplane tutorials (Fabric → FluxVM schema v4)

Hands-on guides for the **VM edge** plane: FluxVM Network Fabric TC/eBPF,
proxied by Zyvor Fabric. These are **not** Fabric SDN Net Security
(`/api/network-policies`).

| Tutorial | Focus | Time |
|----------|-------|------|
| [01 — Getting started](01-getting-started.md) | Enable edge, `/readyz`, health, status, schema=4 | ~15 min |
| [02 — Per-VM policy](02-per-vm-policy.md) | Allowlists, deny CIDRs, ICMP, rate limits | ~20 min |
| [03 — Security groups](03-security-groups.md) | Label identities + group CRUD | ~20 min |
| [04 — CNP documents](04-cnp.md) | Apply CNP JSON via Fabric | ~20 min |
| [05 — Effective merge](05-effective-merge.md) | Declared vs group-merged policy | ~15 min |
| [06 — Observe & identities](06-observe.md) | Snapshot + reserved/group IDs | ~15 min |
| [07 — Health, ipcache, FQDN](07-production-ops.md) | `/readyz`, tenant filter, production ops | ~15 min |
| [08 — Console UX](08-console-ux.md) | Dataplane tab + Edge Dataplane page | ~15 min |

Parent walkthrough (single long form): [Tutorial 09](../09-edge-dataplane.md).
Service Maglev VIP: [Tutorial 10](../10-service-fabric.md).

Operator reference: [fluxvm-dataplane.md](../../guides/vm-drivers/fluxvm-dataplane.md) ·
[Service Fabric v6 (BPF schema 4)](../../ebpf-service-fabric.md) ·
User: [dataplane.md](../../user/pages/infrastructure/dataplane.md) ·
[edge-dataplane.md](../../user/pages/infrastructure/edge-dataplane.md).

Kernel / BPF source of truth: [FluxVM Network Fabric](https://github.com/zyvorai/fluxvm/blob/main/docs/network-fabric.md) ·
[Service Fabric](https://github.com/zyvorai/fluxvm/blob/main/docs/service-fabric.md).

## Shared prerequisites

1. Fabric + FluxVM on a Linux/KVM host with eBPF enabled:

```toml
# /etc/fluxvm.toml (Fabric ships configs/fluxvm-dataplane.toml)
[sandbox.dataplane]
mode = "ebpf"
bpf_object = "/usr/lib/fluxvm/bpf/fluxvm_tc.bpf.o"
pin_root = "/sys/fs/bpf/fluxvm"
required = true
default_allow = false
```

2. Shell env (HTTPS lab example):

```bash
export FABRIC_HOST="https://127.0.0.1:9095"
TOKEN=$(curl -sk "$FABRIC_HOST/api/auth/login" \
  -H "Content-Type: application/json" \
  -d '{"username":"admin","password":"YOUR_PASSWORD"}' | jq -r '.token')
AUTH=(-H "Authorization: Bearer $TOKEN" -H "Content-Type: application/json")
# Optional CLI:
export ZYVOR_FABRIC_URL="$FABRIC_HOST"
export ZYVOR_FABRIC_TOKEN="$TOKEN"
```

3. A **bridged** VM (`network_tap: true`) so TC can attach on the host `vh…`
   interface. User-mode NAT / `mode=none` soft-skips attach.

## CLI cheat sheet

| Task | Command |
|------|---------|
| Health | `fabricctl dataplane health -o json` |
| Groups | `fabricctl dataplane group list\|get\|create\|delete` |
| CNP | `fabricctl dataplane cnp list\|apply\|delete` |
| Effective | `fabricctl dataplane effective NAME -o json` |
| Observe | `fabricctl dataplane observe -o json` |
| Ipcache / refresh | `fabricctl dataplane ipcache\|refresh-dns -o json` |
| Per-VM | `fabricctl dataplane status\|policy\|stats\|flows NAME` |

## Automated check

```bash
FABRIC_URL="$FABRIC_HOST" FABRIC_TOKEN="$TOKEN" \
  ./scripts/test-edge-dataplane-e2e.sh
```
