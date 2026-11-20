# 08 — Console UX

**Time:** ~15 min · **Level:** Beginner · **Prereq:** [01](01-getting-started.md)

## Dashboard

1. Open `https://HOST:9095/app` (or your `FABRIC_HOST`).
2. Sign in at `/sign-in`: username → **Continue** → password.
3. Find capability **VM dataplane** — expect **Live** with
   `mode=ebpf · attached · schema=4` when a bridged sample VM is attached.

## VM → Dataplane

1. **Virtual Machines** → open a bridged running VM → tab **Dataplane**.
2. **Status** — attached, schema 4, identity, pin dir, policy snapshot
   (including deny/groups/ICMP when set).
3. **Policy** — presets; allow/deny CIDRs; ports; groups; labels; FQDNs;
   entities; ICMP; audit; Mbps/PPS; Advanced JSON; **Save**.
4. **Effective** — declared + membership + merged JSON.
5. **Stats** / **Flows** — counters and LRU table (enable sample rate ≥ 1).

Soft banner appears when the `vm_dataplane` capability is off/unreachable.

## Infrastructure → Edge Dataplane

Route: `/app/edge-dataplane` (nav under **Infrastructure**, beside Net Security).

| Tab | Use |
|-----|-----|
| Health | Cluster readiness / notes |
| Services | Maglev VIP upsert/delete; Service Fabric v6 (BPF schema 4) badge; health reconcile; ads; conntrack GC; HA deltas; identity/L7 policy |
| Groups | Create quick group / delete |
| CNP | Paste JSON apply / delete |
| Identities | Reserved + group table |
| Observe | Full snapshot JSON |
| Ipcache | Guest IP → identity |
| (header) Refresh DNS | `POST /api/dataplane/refresh-dns` (best-effort) |

Copy on the page reminds operators this is the **VM edge** / Service Fabric plane,
not Fabric SDN Net Security. Maglev detail:
[ebpf-service-fabric.md](../../ebpf-service-fabric.md).

## Automated UX API check

```bash
FABRIC_URL="$FABRIC_HOST" FABRIC_TOKEN="$TOKEN" FABRIC_VM="$VM" \
  ./scripts/test-edge-dataplane-e2e.sh
```
