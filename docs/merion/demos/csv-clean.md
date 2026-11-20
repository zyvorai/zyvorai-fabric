# CSV cleanup demo

CSV in → `clean.csv` and `report.md` out: blank rows and exact duplicates removed, cells trimmed, and any cell starting with `=`, `+`, `-` or `@` neutralised so a spreadsheet cannot run it as a formula. No browser. Expect **0 CONNECT**.

## Surfaces

| Surface | Path |
|---|---|
| Console | `/app/merion` → **CSV cleanup** |
| Script | `./scripts/merion-demo.sh csv-clean [file.csv]` |
| Runtime API | `POST /v1/demos/csv-clean` (multipart field `file`, optional) |
| fabricd proxy | `POST /api/demos/csv-clean` (JWT) |
| Pack | [`examples/merion-agents/csv-clean/`](https://github.com/zyvorai/fabric/tree/main/examples/merion-agents/csv-clean) |

## Pass bar

- Artifact `clean.csv` and `report.md` in the session
- Cockpit `egress_connects == 0`
- On failure (CONNECT > 0): session `agent_paused_reason: ebpf_deny` + FluxVM freeze
- Evidence class remains `software-test`

## How it works

The upload is written to a fixed path inside the cell (`/home/agent/work/input.csv`),
the guest extracts text with a fixed command, and the summary is built **on the host**
from that text. RFC 4180 parsing (quotes, embedded newlines). Signed numbers such as `-3.5` are not treated as formulas. Files over 300 KB are refused.

## Requirements

- FluxVM reachable; agent-runtime `/healthz`
- Template `node22-agent`: Nothing beyond `head` in the guest.
- Strict confinement applied (`deny_udp` + gateway-only broker/proxy ports) — see [confine.md](../confine.md)

## Honesty

The result is **extractive**: no model is called, and nothing found in the file is run,
opened or sent. Zero CONNECT is a **Merion audit** claim (plus FluxVM `drop_reasons` when
the dataplane is attached). PacketWolf is optional. Evidence class stays `software-test`;
this does not mean the operator cannot read the cell.
