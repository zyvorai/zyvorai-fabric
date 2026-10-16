# deploy-op — Keep packaged agent

Customer deployment readiness. Probes fabricd `/readyz` and `/health`, emits a
readiness artifact mapped to [FABRIC_DOCTOR](../../../docs/FABRIC_DOCTOR.md).

**No host writes by default** — the agent proposes privileged install commands;
an operator runs them.

## Demo

```bash
./scripts/merion-pack-demo.sh deploy-op
```

## Artifacts

- Prerequisites checklist + readiness report (markdown)
