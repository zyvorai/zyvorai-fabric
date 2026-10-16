# Log triage demo

Log file in → `triage.md` out: lines by level, the most repeated errors (numbers and timestamps collapsed), first and last timestamp, and the busiest error minutes. No browser. Expect **0 CONNECT**.

## Surfaces

| Surface | Path |
|---|---|
| Console | `/app/keep` → **Log triage** |
| Script | `./scripts/merion-demo.sh log-triage [file.log]` |
| Runtime API | `POST /v1/demos/log-triage` (multipart field `file`, optional) |
| fabricd proxy | `POST /api/demos/log-triage` (JWT) |
| Pack | [`examples/merion-agents/log-triage/`](https://github.com/zyvorai/fabric/tree/main/examples/merion-agents/log-triage) |

## Pass bar

- Artifact `triage.md` in the session
- Cockpit `egress_connects == 0`
- On failure (CONNECT > 0): session `agent_paused_reason: ebpf_deny` + FluxVM freeze
- Evidence class remains `software-test`

## How it works

The upload is written to a fixed path inside the cell (`/home/agent/work/input.log`),
the guest extracts text with a fixed command, and the summary is built **on the host**
from that text. Level keywords (FATAL/CRITICAL, ERROR, WARN, INFO, DEBUG) and ISO timestamps (`YYYY-MM-DDTHH:MM:SS`).

## Requirements

- FluxVM reachable; agent-runtime `/healthz`
- Template `node22-agent`: Nothing beyond `head` in the guest.
- Strict confinement applied (`deny_udp` + gateway-only broker/proxy ports) — see [confine.md](../confine.md)

## Honesty

The result is **extractive**: no model is called, and nothing found in the file is run,
opened or sent. Zero CONNECT is a **Keep audit** claim (plus FluxVM `drop_reasons` when
the dataplane is attached). PacketWolf is optional. Evidence class stays `software-test`;
this does not mean the operator cannot read the cell.
