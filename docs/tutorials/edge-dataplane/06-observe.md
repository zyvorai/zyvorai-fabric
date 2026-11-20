# 06 — Observe & identities

**Time:** ~15 min · **Level:** Intermediate · **Prereq:** [01](01-getting-started.md)

## Identities

```bash
curl -sk "$FABRIC_HOST/api/dataplane/identities" "${AUTH[@]}" | jq '.items[:8]'
# fabricctl dataplane identities -o json
```

Includes reserved entities (`host=1`, `world=2`, …) and security-group identities
(`0x10000+`). Per-VM identities remain in `1..=0xffff`.

## Observe snapshot

```bash
curl -sk "$FABRIC_HOST/api/dataplane/observe" "${AUTH[@]}" | jq '{
  identity_count: (.identities|length),
  groups: [.groups[].name],
  policies: [.policies[].metadata.name // .policies[].name],
  endpoints: [.endpoints[]? | {name, labels, groups, identity}]
}'
# fabricctl dataplane observe -o json
```

Use this after applying groups/CNP to confirm endpoints picked up labels.

## Next

[07 — Health, ipcache, FQDN](07-production-ops.md)
