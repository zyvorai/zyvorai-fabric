# log-triage pack

Log file in → `triage.md` out: lines by level, the most repeated errors (numbers and timestamps collapsed), first and last timestamp, and the busiest error minutes. **No browser. Zero CONNECT.**

```bash
./scripts/merion-demo.sh log-triage examples/merion-agents/log-triage/sample.txt
# or console: /app/merion → Log triage
# or API: POST /v1/demos/log-triage (multipart field `file`)
```

Logs from other systems carry attacker-controlled strings. They are counted and quoted, never interpreted.

Requires FluxVM + `node22-agent`: Nothing beyond `head` in the guest. Host eBPF (`deny_udp` + gateway-only
ports) is applied by agent-runtime confinement.

Docs: [demos/log-triage.md](../../../docs/merion/demos/log-triage.md) ·
[confine.md](../../../docs/merion/confine.md).
