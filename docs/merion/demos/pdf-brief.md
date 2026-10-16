# PDF brief demo

One-click Merion demo: **PDF in → `brief.md` out.** No browser. Expect **0 CONNECT**.

## Surfaces

| Surface | Path |
|---|---|
| Console | `/app/merion` → **Brief this PDF** |
| Script | `./scripts/merion-demo.sh pdf-brief [file.pdf]` (`merion-demo-pdf.sh` still works) |
| Runtime API | `POST /v1/demos/pdf-brief` (multipart field `file`, or `pdf`; optional) |
| fabricd proxy | `POST /api/demos/pdf-brief` (JWT) |
| Pack | [`examples/merion-agents/pdf-brief/`](../../../examples/merion-agents/pdf-brief/) |

## Pass bar

- Artifact `brief.md` in the session  
- Cockpit `egress_connects == 0`  
- On failure (CONNECT > 0): session `agent_paused_reason: ebpf_deny` + FluxVM freeze  
- Evidence class remains `software-test`

## Requirements

- FluxVM reachable; agent-runtime `/healthz`  
- Template `node22-agent` with `pdftotext` (poppler)  
- Strict confinement applied (`deny_udp` + gateway-only broker/proxy ports) — see [confine.md](../confine.md)

## Honesty

PacketWolf is optional. Zero CONNECT is a **Merion audit** claim (plus FluxVM
`drop_reasons` when the dataplane is attached). Do not claim wire product
surfaces you did not attach.

Hands-on: [Tutorial 17](../../tutorials/17-merion-pdf-brief.md). More use cases of the same
shape: [demos/README.md](README.md).
