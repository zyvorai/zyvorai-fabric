# Tutorial 09: VM Edge Dataplane (Network Fabric schema v4)

End-to-end lab for FluxVM’s TC/eBPF **VM edge** plane as exposed by Zyvor
Fabric. For bite-sized steps, use the series under
[edge-dataplane/](edge-dataplane/README.md).

> **Not Fabric SDN.** Host label→nftables policies live under
> `/api/network-policies` and **Net Security**. This tutorial only uses
> `/api/vms/{name}/dataplane/*` and `/api/dataplane/*`.

**Level:** Intermediate  
**Time:** 60 minutes  
**Prerequisites:** Tutorial 01, FluxVM with `sandbox.dataplane.mode = "ebpf"`,
bridged (`network_tap`) VM

---

## What you will learn

1. Probe readiness (`/readyz`), capabilities, and cluster health (`schema=4`)
2. Set per-VM allow/deny/ICMP/rate policy
3. Create security groups and attach by name/labels
4. Apply a CNP document and inspect effective merge
5. Use observe, identities, ipcache, and refresh-dns
6. Drive the same flows from the console and `fabricctl`

---

## Setup

```bash
export FABRIC_HOST="https://127.0.0.1:9095"   # or your lab URL
TOKEN=$(curl -sk "$FABRIC_HOST/api/auth/login" \
  -H "Content-Type: application/json" \
  -d '{"username":"admin","password":"YOUR_PASSWORD"}' | jq -r '.token')
AUTH=(-H "Authorization: Bearer $TOKEN" -H "Content-Type: application/json")
export ZYVOR_FABRIC_URL="$FABRIC_HOST" ZYVOR_FABRIC_TOKEN="$TOKEN"

curl -sk "$FABRIC_HOST/readyz" | jq '{ok, store, fluxvm_ok: .fluxvm.ok}'

VM=$(curl -sk "$FABRIC_HOST/api/vms" "${AUTH[@]}" | jq -r '
  (if type=="array" then . else (.items // .vms // []) end)
  | map(select(.state=="running" or .status=="Running"))
  | .[0].name // empty')
echo "Using VM=$VM"
```

Enable edge on FluxVM if needed — see
[`configs/fluxvm-dataplane.toml`](../../configs/fluxvm-dataplane.toml).

---

## Step 1: Health and status

```bash
curl -sk "$FABRIC_HOST/readyz" | jq '{ok, store, fluxvm_ok: .fluxvm.ok}'
curl -sk "$FABRIC_HOST/api/capabilities" "${AUTH[@]}" | jq '.vm_dataplane'
curl -sk "$FABRIC_HOST/api/dataplane/health" "${AUTH[@]}" | jq '{ok, mode, bpf_object_present, groups, policies}'
curl -sk "$FABRIC_HOST/api/vms/$VM/dataplane/status" "${AUTH[@]}" | jq '{
  attached, schema_version, schema_compatible, policy_synced, interface
}'
```

Pass bar: `/readyz` `"ok": true`, then `attached=true` and the schema version FluxVM returns (11 on a current attach). Do not require `schema_version=4`.

---

## Step 2: Per-VM policy

```bash
curl -sk -X POST "$FABRIC_HOST/api/vms/$VM/dataplane/policy" "${AUTH[@]}" -d '{
  "default_allow": false,
  "allow_cidrs": ["0.0.0.0/0"],
  "deny_cidrs": ["10.66.0.0/16"],
  "allow_ports": ["tcp/80", "tcp/443", "udp/53"],
  "allow_icmp": true,
  "groups": [],
  "labels": ["app=web", "env=lab"],
  "allow_fqdns": [],
  "entities": [],
  "audit_mode": false,
  "max_egress_mbps": 100,
  "max_egress_pps": 10000,
  "sample_rate": 1
}' | jq '{default_allow, allow_ports, deny_cidrs, labels}'
```

---

## Step 3: Security group

```bash
curl -sk -X POST "$FABRIC_HOST/api/dataplane/groups" "${AUTH[@]}" -d '{
  "name": "tutorial-web",
  "labels": ["app=web", "env=lab"],
  "priority": 10,
  "description": "tutorial 09",
  "identity": 0,
  "policy": {
    "default_allow": false,
    "allow_cidrs": ["10.0.0.0/8"],
    "deny_cidrs": [],
    "allow_ports": ["tcp/443", "udp/53"],
    "allow_icmp": true,
    "groups": [],
    "labels": [],
    "allow_fqdns": [],
    "entities": [],
    "audit_mode": false,
    "max_egress_mbps": 250,
    "max_egress_pps": null,
    "sample_rate": 0
  }
}' | jq '{name, identity}'

curl -sk -X POST "$FABRIC_HOST/api/vms/$VM/dataplane/policy" "${AUTH[@]}" -d '{
  "default_allow": false,
  "allow_cidrs": [],
  "deny_cidrs": [],
  "allow_ports": [],
  "allow_icmp": false,
  "groups": ["tutorial-web"],
  "labels": ["app=web", "env=lab"],
  "allow_fqdns": [],
  "entities": [],
  "audit_mode": false,
  "max_egress_mbps": null,
  "max_egress_pps": null,
  "sample_rate": 1
}' | jq '{groups, labels}'
```

---

## Step 4: Effective merge

```bash
curl -sk "$FABRIC_HOST/api/vms/$VM/dataplane/effective" "${AUTH[@]}" | jq '{
  group_identities,
  membership: .membership.vm_groups,
  effective: {
    allow_cidrs: .effective.allow_cidrs,
    allow_ports: .effective.allow_ports,
    max_egress_mbps: .effective.max_egress_mbps,
    default_allow: .effective.default_allow
  }
}'
```

---

## Step 5: CNP apply

```bash
curl -sk -X POST "$FABRIC_HOST/api/dataplane/cnp" "${AUTH[@]}" -d '{
  "apiVersion": "cilium.io/v2",
  "kind": "CiliumNetworkPolicy",
  "metadata": { "name": "tutorial-web-egress" },
  "spec": {
    "endpointSelector": { "matchLabels": { "app": "web" } },
    "egress": [{
      "toCIDR": ["172.16.0.0/12"],
      "toPorts": [{ "ports": [{ "port": "443", "protocol": "TCP" }] }]
    }]
  }
}' | jq '{name, identity}'

curl -sk "$FABRIC_HOST/api/dataplane/cnp" "${AUTH[@]}" | jq '.items[].metadata.name'
```

---

## Step 6: Observe, identities, ipcache, refresh-dns

```bash
curl -sk "$FABRIC_HOST/api/dataplane/identities" "${AUTH[@]}" | jq '.items|length'
curl -sk "$FABRIC_HOST/api/dataplane/observe" "${AUTH[@]}" | jq 'keys'
curl -sk "$FABRIC_HOST/api/dataplane/ipcache" "${AUTH[@]}" | jq '.items|length'
curl -sk -X POST "$FABRIC_HOST/api/dataplane/refresh-dns" "${AUTH[@]}" | jq .
```

---

## Step 7: Stats / flows

```bash
curl -sk "$FABRIC_HOST/api/vms/$VM/dataplane/stats" "${AUTH[@]}" | jq .
curl -sk "$FABRIC_HOST/api/vms/$VM/dataplane/flows?limit=20" "${AUTH[@]}" | jq '.items[:5]'
```

---

## Step 8: Console and CLI

1. Dashboard → **VM dataplane · Live · schema=4**
2. VM → **Dataplane** (Status / Policy / Effective / Stats / Flows)
3. **Infrastructure → Edge Dataplane** (`/app/edge-dataplane`)

```bash
fabricctl dataplane health -o json
fabricctl dataplane group list -o json
fabricctl dataplane effective "$VM" -o json
fabricctl dataplane observe -o json
```

---

## Cleanup

```bash
curl -sk -X DELETE "$FABRIC_HOST/api/dataplane/cnp/tutorial-web-egress" "${AUTH[@]}"
curl -sk -X DELETE "$FABRIC_HOST/api/dataplane/groups/tutorial-web" "${AUTH[@]}"
```

---

## Automated verification

```bash
FABRIC_URL="$FABRIC_HOST" FABRIC_TOKEN="$TOKEN" FABRIC_VM="$VM" \
  ./scripts/test-edge-dataplane-e2e.sh
```

---

## Next steps

- Deep dive series: [edge-dataplane/](edge-dataplane/README.md)
- Operator guide: [fluxvm-dataplane.md](../guides/vm-drivers/fluxvm-dataplane.md)
- [Tutorial 02](02-networking.md) — bridges / Fabric SDN (orthogonal)
- [Tutorial 06](06-security-hardening.md) — host firewalls / RBAC
