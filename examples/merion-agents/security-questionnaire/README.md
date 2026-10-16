# security-questionnaire pack

Vendor questionnaire PDF in → `answers.md` out: every question paired with the text that follows it, and a count of unanswered ones. **No browser. Zero CONNECT.**

```bash
./scripts/merion-demo.sh security-questionnaire examples/merion-agents/security-questionnaire/sample.pdf
# or console: /app/keep → Security questionnaire
# or API: POST /v1/demos/security-questionnaire (multipart field `file`)
```

Questionnaires arrive from outside your organisation. The cell reads them without any way to phone home.

Requires FluxVM + `node22-agent`: `pdftotext` (poppler) in `node22-agent`. Host eBPF (`deny_udp` + gateway-only
ports) is applied by agent-runtime confinement.

Docs: [demos/security-questionnaire.md](../../../docs/merion/demos/security-questionnaire.md) ·
[confine.md](../../../docs/merion/confine.md).
