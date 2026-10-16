# sbom-summary pack

CycloneDX, SPDX or SARIF JSON in → `summary.md` out: component or package counts, licenses, items without a license, and severity counts. **No browser. Zero CONNECT.**

```bash
./scripts/merion-demo.sh sbom-summary examples/merion-agents/sbom-summary/sample.json
# or console: /app/keep → SBOM summary
# or API: POST /v1/demos/sbom-summary (multipart field `file`)
```

Third-party SBOMs and scanner output are untrusted input. They are parsed on the host as data only.

Requires FluxVM + `node22-agent`: Nothing beyond `head` in the guest. Host eBPF (`deny_udp` + gateway-only
ports) is applied by agent-runtime confinement.

Docs: [demos/sbom-summary.md](../../../docs/merion/demos/sbom-summary.md) ·
[confine.md](../../../docs/merion/confine.md).
