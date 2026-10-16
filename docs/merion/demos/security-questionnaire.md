# Security questionnaire demo

Vendor questionnaire PDF in → `answers.md` out: every question paired with the text that follows it, and a count of unanswered ones. No browser. Expect **0 CONNECT**.

## Surfaces

| Surface | Path |
|---|---|
| Console | `/app/keep` → **Security questionnaire** |
| Script | `./scripts/merion-demo.sh security-questionnaire [file.pdf]` |
| Runtime API | `POST /v1/demos/security-questionnaire` (multipart field `file`, optional) |
| fabricd proxy | `POST /api/demos/security-questionnaire` (JWT) |
| Pack | [`examples/merion-agents/security-questionnaire/`](https://github.com/zyvorai/fabric/tree/main/examples/merion-agents/security-questionnaire) |

## Pass bar

- Artifact `answers.md` in the session
- Cockpit `egress_connects == 0`
- On failure (CONNECT > 0): session `agent_paused_reason: ebpf_deny` + FluxVM freeze
- Evidence class remains `software-test`

## How it works

The upload is written to a fixed path inside the cell (`/home/agent/work/input.pdf`),
the guest extracts text with a fixed command, and the summary is built **on the host**
from that text. Question-shaped lines (a trailing `?`, or "Do you", "Describe", "Please" …) paired with the lines up to the next question. Up to 40 questions.

## Requirements

- FluxVM reachable; agent-runtime `/healthz`
- Template `node22-agent`: `pdftotext` (poppler) in `node22-agent`.
- Strict confinement applied (`deny_udp` + gateway-only broker/proxy ports) — see [confine.md](../confine.md)

## Honesty

The result is **extractive**: no model is called, and nothing found in the file is run,
opened or sent. Zero CONNECT is a **Keep audit** claim (plus FluxVM `drop_reasons` when
the dataplane is attached). PacketWolf is optional. Evidence class stays `software-test`;
this does not mean the operator cannot read the cell.
