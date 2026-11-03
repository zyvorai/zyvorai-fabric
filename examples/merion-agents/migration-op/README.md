# migration-op — Merion packaged agent

Migration operator against **Fabric** [`/api/migrations`](../../../docs/migration.md)
and GuestKit `inspect` / `rescue` (`/api/vms/{name}/inspect`, `/rescue`).

**Out of scope in this repo:** Transiva / hypersdk inventory. Wire those
out-of-band; this pack only talks to fabricd.

## Demo

```bash
./scripts/merion-pack-demo.sh migration-op
```

Session input examples:

```json
{
  "fabricBase": "https://127.0.0.1:9095",
  "inspectVm": "stopped-guest",
  "createMigration": { "vm_name": "web-1", "target_host": "node-b" }
}
```

## Artifacts

- Wave plan + preflight report + cutover checklist (markdown)
- Mutating create/cancel/rescue always hit `fabric-api` approval + Merion ask
