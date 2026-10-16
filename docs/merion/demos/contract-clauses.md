# Contract clauses demo

Contract PDF in → `clauses.md` out: the term, renewal, termination, payment, liability, confidentiality, governing-law and data-protection lines, plus the topics it could not find. No browser. Expect **0 CONNECT**.

## Surfaces

| Surface | Path |
|---|---|
| Console | `/app/merion` → **Contract clauses** |
| Script | `./scripts/merion-demo.sh contract-clauses [file.pdf]` |
| Runtime API | `POST /v1/demos/contract-clauses` (multipart field `file`, optional) |
| fabricd proxy | `POST /api/demos/contract-clauses` (JWT) |
| Pack | [`examples/merion-agents/contract-clauses/`](https://github.com/zyvorai/fabric/tree/main/examples/merion-agents/contract-clauses) |

## Pass bar

- Artifact `clauses.md` in the session
- Cockpit `egress_connects == 0`
- On failure (CONNECT > 0): session `agent_paused_reason: ebpf_deny` + FluxVM freeze
- Evidence class remains `software-test`

## How it works

The upload is written to a fixed path inside the cell (`/home/agent/work/input.pdf`),
the guest extracts text with a fixed command, and the summary is built **on the host**
from that text. Keyword match per topic (up to three lines each). It is a reading aid, not legal advice.

## Requirements

- FluxVM reachable; agent-runtime `/healthz`
- Template `node22-agent`: `pdftotext` (poppler) in `node22-agent`.
- Strict confinement applied (`deny_udp` + gateway-only broker/proxy ports) — see [confine.md](../confine.md)

## Honesty

The result is **extractive**: no model is called, and nothing found in the file is run,
opened or sent. Zero CONNECT is a **Keep audit** claim (plus FluxVM `drop_reasons` when
the dataplane is attached). PacketWolf is optional. Evidence class stays `software-test`;
this does not mean the operator cannot read the cell.
