# pdf-brief pack

One-click Keep demo: PDF in → `brief.md` out. **No browser. Zero CONNECT.**

```bash
./scripts/merion-demo-pdf.sh examples/merion-agents/pdf-brief/sample.pdf
# or console: /app/merion → Brief this PDF
# or API: POST /v1/demos/pdf-brief (multipart field `pdf`)
```

Requires FluxVM + `node22-agent` with `pdftotext` (poppler). Host eBPF
(`deny_udp` + gateway-only ports) is applied by agent-runtime confinement.

Docs: [Tutorial 17](../../../docs/tutorials/17-merion-pdf-brief.md) ·
[demos/pdf-brief.md](../../../docs/merion/demos/pdf-brief.md) ·
[confine.md](../../../docs/merion/confine.md).
