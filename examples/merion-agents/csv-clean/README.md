# csv-clean pack

CSV in → `clean.csv` and `report.md` out: blank rows and exact duplicates removed, cells trimmed, and any cell starting with `=`, `+`, `-` or `@` neutralised so a spreadsheet cannot run it as a formula. **No browser. Zero CONNECT.**

```bash
./scripts/merion-demo.sh csv-clean examples/merion-agents/csv-clean/sample.csv
# or console: /app/keep → CSV cleanup
# or API: POST /v1/demos/csv-clean (multipart field `file`)
```

CSV files from outside can carry spreadsheet formula payloads. The cell cleans them before they reach a spreadsheet.

Requires FluxVM + `node22-agent`: Nothing beyond `head` in the guest. Host eBPF (`deny_udp` + gateway-only
ports) is applied by agent-runtime confinement.

Docs: [demos/csv-clean.md](../../../docs/merion/demos/csv-clean.md) ·
[confine.md](../../../docs/merion/confine.md).
