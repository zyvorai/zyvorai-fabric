# Tutorial 10: Service Fabric Maglev VIP (v6)

Create a Maglev VIP through Fabric’s Edge Dataplane APIs, backed by FluxVM
Service Fabric **v6** (BPF schema **4** / program generation **6**).

**Level:** Intermediate  
**Time:** 35 minutes  
**Prerequisites:** Tutorial 01, Tutorial 09 (edge dataplane), FluxVM with
`[sandbox.dataplane] mode = "ebpf"` and service maps configured.

> Orthogonal to per-VM Network Fabric policy. This tutorial uses
> `/api/dataplane/services…` only.

---

## What you will learn

1. Check Service Fabric host status via Fabric
2. Upsert a Maglev service (NAT / backends / health)
3. Reconcile health, inspect ads and flows
4. Apply a v6 identity policy and Envoy contract lookup
5. Clean up

---

## Setup

```bash
export FABRIC_HOST="https://127.0.0.1:9095"
TOKEN=$(curl -sk "$FABRIC_HOST/api/auth/login" \
  -H "Content-Type: application/json" \
  -d '{"username":"admin","password":"YOUR_PASSWORD"}' | jq -r '.token')
AUTH=(-H "Authorization: Bearer $TOKEN" -H "Content-Type: application/json")
export ZYVOR_FABRIC_URL="$FABRIC_HOST" ZYVOR_FABRIC_TOKEN="$TOKEN"

curl -sk "$FABRIC_HOST/readyz" | jq '{ok, fluxvm_ok: .fluxvm.ok}'
```

Never hardcode lab IPs in docs or scripts you commit — use `$FABRIC_HOST`.

---

## Step 1: Host status

```bash
curl -sk "$FABRIC_HOST/api/dataplane/services/status" "${AUTH[@]}" | jq '{
  schema_version, program_generation, interfaces
}'
```

Pass bar: schema **4**. Generation **6** on a v6 FluxVM build.

---

## Step 2: Upsert Maglev service

```bash
curl -sk -X POST "$FABRIC_HOST/api/dataplane/services" "${AUTH[@]}" \
  --data @docs/examples/service-fabric-v3/ha-draining-service.json | jq .

fabricctl dataplane service list
fabricctl dataplane service status
```

Backend fields use `"address"` (not `"ip"`). States: `ready` / `draining` /
`unhealthy`.

---

## Step 3: Health, ads, flows

```bash
curl -sk -X POST "$FABRIC_HOST/api/dataplane/services/health/reconcile" "${AUTH[@]}" | jq .
curl -sk "$FABRIC_HOST/api/dataplane/services/health" "${AUTH[@]}" | jq .
curl -sk "$FABRIC_HOST/api/dataplane/services/advertisements" "${AUTH[@]}" | jq .
curl -sk "$FABRIC_HOST/api/dataplane/services/flows?limit=20" "${AUTH[@]}" | jq .
```

North-south advertisements require FluxVM `north_south_interfaces` and matching
service `exposure`. NAT north-south needs `snat_address`.

---

## Step 4: Identity / L7 policy (v6)

```bash
curl -sk -X POST "$FABRIC_HOST/api/dataplane/services/policies" "${AUTH[@]}" -d '{
  "service": "payments",
  "enabled": true,
  "default_action": "deny",
  "allow_identities": [1001],
  "deny_identities": [],
  "audit_only": false,
  "l7": null
}' | jq .

curl -sk "$FABRIC_HOST/api/dataplane/services/policies" "${AUTH[@]}" | jq .
curl -sk "$FABRIC_HOST/api/dataplane/services/payments/policy" "${AUTH[@]}" | jq .
curl -sk "$FABRIC_HOST/api/dataplane/services/payments/l7/envoy" "${AUTH[@]}" | jq .
```

Policy JSON uses `service` / `default_action` / `allow_identities` (not
`name` / `default`).

---

## Step 5: Console

1. **Infrastructure → Edge Dataplane** (`/app/edge-dataplane`)
2. **Services** tab — upsert/list, health reconcile, ads, GC, flows
3. Confirm Service Fabric **v6** / schema **4** badge when shown

---

## Cleanup

```bash
curl -sk -X DELETE "$FABRIC_HOST/api/dataplane/services/payments/policy" "${AUTH[@]}"
curl -sk -X DELETE "$FABRIC_HOST/api/dataplane/services/payments" "${AUTH[@]}"
# or: fabricctl dataplane service delete payments
```

---

## Next steps

- Contract: [ebpf-service-fabric.md](../ebpf-service-fabric.md)
- FluxVM tutorial: [service-fabric](https://github.com/zyvorai/fluxvm/blob/main/docs/tutorials/service-fabric/README.md)
- [Tutorial 09](09-edge-dataplane.md) — per-VM Network Fabric (orthogonal)
- [edge-dataplane series](edge-dataplane/README.md)
