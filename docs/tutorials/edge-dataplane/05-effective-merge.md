# 05 — Effective policy merge

**Time:** ~15 min · **Level:** Intermediate · **Prereq:** [03](03-security-groups.md)

`GET …/dataplane/effective` returns declared policy, membership, merged
effective policy, and group identities.

Merge rules (FluxVM):

- Allow/deny CIDRs and ports are **unioned**
- Tightest Mbps/PPS wins
- Any fail-closed member forces `default_allow=false`

## 1. Fetch effective

```bash
curl -sk "$FABRIC_HOST/api/vms/$VM/dataplane/effective" "${AUTH[@]}" | jq '{
  vm_identity,
  group_identities,
  membership: .membership,
  declared_groups: .declared.groups,
  declared_labels: .declared.labels,
  effective_cidrs: .effective.allow_cidrs,
  effective_ports: .effective.allow_ports,
  effective_deny: .effective.deny_cidrs,
  effective_default_allow: .effective.default_allow,
  effective_mbps: .effective.max_egress_mbps
}'
# fabricctl dataplane effective "$VM" -o json
```

## 2. Multi-group experiment

Create a second group with a tighter rate limit, attach both names on the VM
policy `groups` array, then re-fetch effective — `max_egress_mbps` should be
the minimum of the members.

## Console

VM → **Dataplane → Effective** shows the same JSON snapshot.

## Next

[06 — Observe & identities](06-observe.md)
