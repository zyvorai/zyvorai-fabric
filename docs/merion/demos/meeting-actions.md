# Meeting actions demo

Transcript (`.txt` or `.vtt`) in → `actions.md` out: candidate action items with owners, and decisions. No browser. Expect **0 CONNECT**.

## Surfaces

| Surface | Path |
|---|---|
| Console | `/app/merion` → **Meeting actions** |
| Script | `./scripts/merion-demo.sh meeting-actions [file.vtt]` |
| Runtime API | `POST /v1/demos/meeting-actions` (multipart field `file`, optional) |
| fabricd proxy | `POST /api/demos/meeting-actions` (JWT) |
| Pack | [`examples/merion-agents/meeting-actions/`](https://github.com/zyvorai/fabric/tree/main/examples/merion-agents/meeting-actions) |

## Pass bar

- Artifact `actions.md` in the session
- Cockpit `egress_connects == 0`
- On failure (CONNECT > 0): session `agent_paused_reason: ebpf_deny` + FluxVM freeze
- Evidence class remains `software-test`

## How it works

The upload is written to a fixed path inside the cell (`/home/agent/work/input.txt`),
the guest extracts text with a fixed command, and the summary is built **on the host**
from that text. Marker phrases ("action item", "will send", "follow up", "by Friday" …). WebVTT headers and timestamps are skipped. Up to 30 actions and 15 decisions.

## Requirements

- FluxVM reachable; agent-runtime `/healthz`
- Template `node22-agent`: Nothing beyond `head` in the guest.
- Strict confinement applied (`deny_udp` + gateway-only broker/proxy ports) — see [confine.md](../confine.md)

## Honesty

The result is **extractive**: no model is called, and nothing found in the file is run,
opened or sent. Zero CONNECT is a **Keep audit** claim (plus FluxVM `drop_reasons` when
the dataplane is attached). PacketWolf is optional. Evidence class stays `software-test`;
this does not mean the operator cannot read the cell.
