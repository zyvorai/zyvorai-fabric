# Dataplane observe + control pack

Implemented via `policy_control` (Rust) / `policyControls` (TS) — not a separate
`policy_engine` module.

| Feature | Surface |
|---------|---------|
| Open / Audit / Guard / Invert / Block / Allow | Policy tab + `fabricctl dataplane policy …` |
| Explain dest:port | Policy tab + `fabricctl dataplane explain` |
| Dry-run Guard | Policy tab + `fabricctl dataplane dry-run` |
| Templates | open, guard, web, dns-only, no-world |
| Block from flow row | VM Dataplane → Flows |
| Drop reasons | POLICY_DENIED, DEFAULT_DENY, PORT_DENIED, AUDIT_WOULD_DROP |

```bash
fabricctl dataplane policy guard web-1
fabricctl dataplane explain web-1 1.1.1.1 --port 443 --proto tcp
fabricctl dataplane dry-run web-1
python3 scripts/test-policy-engine.py
```

See also: [user dataplane](user/pages/infrastructure/dataplane.md),
[fluxvm-dataplane](guides/vm-drivers/fluxvm-dataplane.md).
