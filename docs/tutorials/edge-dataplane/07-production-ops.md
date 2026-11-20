# 07 — Health, ipcache, and FQDN refresh

**Time:** ~15 min · **Level:** Intermediate · **Prereq:** [01](01-getting-started.md)

Production-oriented cluster endpoints (also in FluxVM
[production-dataplane.md](https://github.com/zyvorai/fluxvm/blob/main/docs/production-dataplane.md)
and whole-stack [PRODUCTION.md](https://github.com/zyvorai/fluxvm/blob/main/docs/PRODUCTION.md)).

## Platform readiness

```bash
curl -sk "$FABRIC_HOST/readyz" | jq .
# Liveness only: curl -sk "$FABRIC_HOST/health"
```

`ok: false` / HTTP 503 means the Fabric store or FluxVM `/readyz` failed (missing
state dir, required dataplane not healthy, etc.).

## Dataplane health

```bash
curl -sk "$FABRIC_HOST/api/dataplane/health" "${AUTH[@]}" | jq .
```

`ok: false` with notes usually means missing BPF object, missing `/sys/fs/bpf`,
or `mode=cilium` without `cilium.sock`.

## Ipcache

Guest IP → FluxVM identity map (control-plane JSON under FluxVM state_dir):

```bash
curl -sk "$FABRIC_HOST/api/dataplane/ipcache" "${AUTH[@]}" | jq '.items'
# fabricctl dataplane ipcache -o json
```

## FQDN refresh

If policy / CNP used `allow_fqdns` / `toFQDNs`, re-resolve into CIDRs:

```bash
curl -sk -X POST "$FABRIC_HOST/api/dataplane/refresh-dns" "${AUTH[@]}" | jq .
# fabricctl dataplane refresh-dns -o json
```

Expect `{"refreshed": N}`. If refresh fails with “Filter already exists” after a
schema upgrade, clean stale TC filters and restart the VM (see
[01](01-getting-started.md)).

## Tenant filter (optional)

VMs created with `"tenant": "acme"` (or `labels.tenant`) are filterable:

```bash
curl -sk "$FABRIC_HOST/api/vms?tenant=acme" "${AUTH[@]}" | jq '.items[].name'
```

## Example: FQDN on VM policy

```bash
curl -sk -X POST "$FABRIC_HOST/api/vms/$VM/dataplane/policy" "${AUTH[@]}" -d '{
  "default_allow": false,
  "allow_cidrs": [],
  "deny_cidrs": [],
  "allow_ports": ["tcp/443"],
  "allow_icmp": false,
  "groups": [],
  "labels": [],
  "allow_fqdns": ["example.com"],
  "entities": ["world"],
  "audit_mode": false,
  "max_egress_mbps": null,
  "max_egress_pps": null,
  "sample_rate": 1
}' | jq '{allow_fqdns, entities, allow_cidrs}'

curl -sk -X POST "$FABRIC_HOST/api/dataplane/refresh-dns" "${AUTH[@]}" | jq .
curl -sk "$FABRIC_HOST/api/vms/$VM/dataplane/policy" "${AUTH[@]}" | jq '{allow_fqdns, allow_cidrs}'
```

## Next

[08 — Console UX](08-console-ux.md)
