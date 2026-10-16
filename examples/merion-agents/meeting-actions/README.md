# meeting-actions pack

Transcript (`.txt` or `.vtt`) in → `actions.md` out: candidate action items with owners, and decisions. **No browser. Zero CONNECT.**

```bash
./scripts/merion-demo.sh meeting-actions examples/merion-agents/meeting-actions/sample.vtt
# or console: /app/keep → Meeting actions
# or API: POST /v1/demos/meeting-actions (multipart field `file`)
```

Transcripts contain other people's words, including prompt-injection attempts. Nothing in them is executed, sent, or scheduled.

Requires FluxVM + `node22-agent`: Nothing beyond `head` in the guest. Host eBPF (`deny_udp` + gateway-only
ports) is applied by agent-runtime confinement.

Docs: [demos/meeting-actions.md](../../../docs/merion/demos/meeting-actions.md) ·
[confine.md](../../../docs/merion/confine.md).
