# 03 — Security groups

**Time:** ~20 min · **Level:** Intermediate · **Prereq:** [02](02-per-vm-policy.md)

Security groups are named label sets + shared L3/L4 policy. Identities are
stable hashes in the `0x10000+` range. Membership is explicit (`groups`) and/or
label match (`labels` on the VM policy).

## 1. Create a group

```bash
curl -sk -X POST "$FABRIC_HOST/api/dataplane/groups" "${AUTH[@]}" -d '{
  "name": "web",
  "labels": ["app=web", "env=lab"],
  "priority": 10,
  "description": "HTTPS egress for web tier",
  "identity": 0,
  "policy": {
    "default_allow": false,
    "allow_cidrs": ["10.0.0.0/8", "172.16.0.0/12"],
    "deny_cidrs": ["10.66.0.0/16"],
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
}' | jq '{name, identity, labels, priority}'
```

## 2. List / get

```bash
curl -sk "$FABRIC_HOST/api/dataplane/groups" "${AUTH[@]}" | jq '.items[] | {name, identity, labels}'
curl -sk "$FABRIC_HOST/api/dataplane/groups/web" "${AUTH[@]}" | jq .
# fabricctl dataplane group list -o json
```

## 3. Attach the VM via labels (or group name)

```bash
curl -sk -X POST "$FABRIC_HOST/api/vms/$VM/dataplane/policy" "${AUTH[@]}" -d '{
  "default_allow": false,
  "allow_cidrs": [],
  "deny_cidrs": [],
  "allow_ports": [],
  "allow_icmp": false,
  "groups": ["web"],
  "labels": ["app=web", "env=lab"],
  "allow_fqdns": [],
  "entities": [],
  "audit_mode": false,
  "max_egress_mbps": null,
  "max_egress_pps": null,
  "sample_rate": 1
}' | jq '{groups, labels}'
```

Cap: **8 groups per VM**.

## 4. CLI

```bash
cat >/tmp/group-web.json <<'EOF'
{
  "name": "web",
  "labels": ["app=web"],
  "priority": 10,
  "description": "web",
  "identity": 0,
  "policy": {
    "default_allow": false,
    "allow_cidrs": ["10.0.0.0/8"],
    "deny_cidrs": [],
    "allow_ports": ["tcp/443"],
    "allow_icmp": true,
    "groups": [],
    "labels": [],
    "allow_fqdns": [],
    "entities": [],
    "audit_mode": false,
    "max_egress_mbps": 100,
    "max_egress_pps": null,
    "sample_rate": 0
  }
}
EOF
fabricctl dataplane group create --file /tmp/group-web.json -o json
```

## Next

[04 — CNP documents](04-cnp.md) · [05 — Effective merge](05-effective-merge.md)
