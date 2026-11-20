# 01 — Getting started (edge dataplane)

**Time:** ~15 min · **Level:** Beginner

Confirm Fabric can reach FluxVM Network Fabric **schema v4**, inspect health,
and read status on a bridged VM.

## 0. Platform readiness

```bash
curl -sk "$FABRIC_HOST/readyz" | jq '{ok, store, fluxvm_ok: .fluxvm.ok}'
```

Expect `"ok": true`. This is unauthenticated and checks the Fabric store plus
FluxVM `GET /readyz`.

## 1. Capability card

```bash
curl -sk "$FABRIC_HOST/api/capabilities" "${AUTH[@]}" | jq '.vm_dataplane'
```

Expect something like:

```json
{
  "phase": "live",
  "detail": "mode=ebpf · attached · schema=4"
}
```

| Phase | Meaning |
|-------|---------|
| `live` | API reachable; detail shows mode / attach / schema |
| `off` | `sandbox.dataplane.mode=legacy` |
| `unreachable` | FluxVM driver or sample VM probe failed |

## 2. Cluster health

```bash
curl -sk "$FABRIC_HOST/api/dataplane/health" "${AUTH[@]}" | jq .
# or: fabricctl dataplane health -o json
```

Important fields:

| Field | Expect |
|-------|--------|
| `ok` | `true` when notes empty |
| `mode` | `ebpf` (or `cilium` coexistence) |
| `bpf_object_present` | `true` |
| `bpffs_present` | `true` |
| `groups` / `policies` / `ipcache_entries` | counts |

## 3. Pick a bridged VM

```bash
VM=$(curl -sk "$FABRIC_HOST/api/vms" "${AUTH[@]}" | jq -r '
  (if type=="array" then . else (.items // .vms // []) end)
  | map(select(.state=="running" or .status=="Running"))
  | .[0].name // empty')
echo "VM=$VM"
```

Create one with tap/netns if needed (name/image vary by lab):

```bash
# Example — adjust image path for your host
curl -sk -X POST "$FABRIC_HOST/api/vms" "${AUTH[@]}" -d '{
  "name": "edge-lab-01",
  "cpus": 1,
  "memory_mb": 512,
  "network_tap": true
}' | jq .
curl -sk -X POST "$FABRIC_HOST/api/vms/edge-lab-01/start" "${AUTH[@]}" | jq .
VM=edge-lab-01
```

## 4. Per-VM status

```bash
curl -sk "$FABRIC_HOST/api/vms/$VM/dataplane/status" "${AUTH[@]}" | jq '{
  mode, attached, schema_version, schema_compatible,
  policy_synced, interface, identity, pin_dir
}'
```

Success criteria after a clean attach:

- `attached: true`
- `schema_version: 4`
- `schema_compatible: true`
- `interface` like `vh…`
- `policy_synced: true`

If `attached: false` but `pin_dir` is set after a FluxVM upgrade, stale TC
filters may remain — restart the VM or clear `tc filter` / pins under
`/sys/fs/bpf/fluxvm/vms/…` (see operator guide troubleshooting).

## 5. Stats and flows smoke

```bash
curl -sk "$FABRIC_HOST/api/vms/$VM/dataplane/stats" "${AUTH[@]}" | jq .
curl -sk "$FABRIC_HOST/api/vms/$VM/dataplane/flows?limit=10" "${AUTH[@]}" | jq .
```

## Next

[02 — Per-VM policy](02-per-vm-policy.md)
