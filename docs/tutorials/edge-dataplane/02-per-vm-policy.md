# 02 — Per-VM policy

**Time:** ~20 min · **Level:** Intermediate · **Prereq:** [01](01-getting-started.md)

Replace durable edge policy on one VM. Ports must be `tcp/PORT` or `udp/PORT`
(optional range `tcp/8000-8999`). Schema v4 also accepts deny CIDRs, ICMP,
groups, labels, FQDNs, entities, and audit mode.

## 1. Read current policy

```bash
curl -sk "$FABRIC_HOST/api/vms/$VM/dataplane/policy" "${AUTH[@]}" | jq .
```

## 2. Web egress + deny list

```bash
curl -sk -X POST "$FABRIC_HOST/api/vms/$VM/dataplane/policy" "${AUTH[@]}" -d '{
  "default_allow": false,
  "allow_cidrs": ["0.0.0.0/0", "::/0"],
  "deny_cidrs": ["10.66.0.0/16"],
  "allow_ports": ["tcp/80", "tcp/443", "udp/53", "icmp/0"],
  "allow_icmp": true,
  "groups": [],
  "labels": ["app=web", "env=lab"],
  "allow_fqdns": [],
  "entities": [],
  "audit_mode": false,
  "max_egress_mbps": 100,
  "max_egress_pps": 10000,
  "sample_rate": 1
}' | jq .
```

Notes:

- Non-empty allow CIDRs **or** ports become an explicit allowlist (unmatched denied).
- `deny_cidrs` drops even if an allow CIDR would match.
- `allow_icmp: true` lets ICMP/ICMPv6 through L4 enforcement; `icmp/0` is also valid in `allow_ports`.
- `sample_rate: 1` maximizes flow sampling (0 disables).

## 3. Confirm sync

```bash
curl -sk "$FABRIC_HOST/api/vms/$VM/dataplane/status" "${AUTH[@]}" \
  | jq '{attached, schema_version, policy_synced, policy}'
```

## 4. CLI equivalent

```bash
cat >/tmp/dp-policy.json <<'EOF'
{
  "default_allow": false,
  "allow_cidrs": ["10.0.0.0/8"],
  "deny_cidrs": [],
  "allow_ports": ["tcp/443", "udp/53"],
  "allow_icmp": true,
  "groups": [],
  "labels": ["app=web"],
  "allow_fqdns": [],
  "entities": [],
  "audit_mode": false,
  "max_egress_mbps": 50,
  "max_egress_pps": null,
  "sample_rate": 1
}
EOF
fabricctl dataplane policy set "$VM" --file /tmp/dp-policy.json -o json
```

## Next

[03 — Security groups](03-security-groups.md)
