# 04 — CNP documents

**Time:** ~20 min · **Level:** Intermediate · **Prereq:** [03](03-security-groups.md)

CNP-shaped JSON (`apiVersion: cilium.io/v2`, `kind: CiliumNetworkPolicy`) is
**compiled onto FluxVM security groups**. Fabric stores and applies the document
via `/api/dataplane/cnp` — this does **not** install foreign CNI CRDs or write
foreign private maps.

Supported subset (same as FluxVM): `endpointSelector.matchLabels`,
egress/egressDeny/ingress/ingressDeny, `toCIDR` / `toCIDRSet`, `toEntities`,
`toFQDNs`, `toPorts`, `fromCIDR`, `fromEntities`, `enableDefaultDeny`,
`auditMode`, `description`.

## 1. Apply

```bash
curl -sk -X POST "$FABRIC_HOST/api/dataplane/cnp" "${AUTH[@]}" -d '{
  "apiVersion": "cilium.io/v2",
  "kind": "CiliumNetworkPolicy",
  "metadata": { "name": "web-egress" },
  "spec": {
    "endpointSelector": { "matchLabels": { "app": "web" } },
    "egress": [{
      "toCIDR": ["10.0.0.0/8"],
      "toPorts": [{ "ports": [{ "port": "443", "protocol": "TCP" }] }]
    }]
  }
}' | jq .
```

Response is the compiled **security group** (name / identity / policy).

## 2. List / get / delete

```bash
curl -sk "$FABRIC_HOST/api/dataplane/cnp" "${AUTH[@]}" | jq '.items[].metadata.name'
curl -sk "$FABRIC_HOST/api/dataplane/cnp/web-egress" "${AUTH[@]}" | jq .
curl -sk -X DELETE "$FABRIC_HOST/api/dataplane/cnp/web-egress" "${AUTH[@]}" | jq .
```

## 3. CLI

```bash
cat >/tmp/web-egress.cnp.json <<'EOF'
{
  "apiVersion": "cilium.io/v2",
  "kind": "CiliumNetworkPolicy",
  "metadata": { "name": "web-egress" },
  "spec": {
    "endpointSelector": { "matchLabels": { "app": "web" } },
    "egress": [{
      "toCIDR": ["10.0.0.0/8"],
      "toPorts": [{ "ports": [{ "port": "443", "protocol": "TCP" }] }]
    }]
  }
}
EOF
fabricctl dataplane cnp apply --file /tmp/web-egress.cnp.json -o json
fabricctl dataplane cnp list -o json
```

## Next

[05 — Effective merge](05-effective-merge.md)
