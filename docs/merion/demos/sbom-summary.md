# SBOM summary demo

CycloneDX, SPDX or SARIF JSON in → `summary.md` out: component or package counts, licenses, items without a license, and severity counts. No browser. Expect **0 CONNECT**.

## Surfaces

| Surface | Path |
|---|---|
| Console | `/app/keep` → **SBOM summary** |
| Script | `./scripts/merion-demo.sh sbom-summary [file.json]` |
| Runtime API | `POST /v1/demos/sbom-summary` (multipart field `file`, optional) |
| fabricd proxy | `POST /api/demos/sbom-summary` (JWT) |
| Pack | [`examples/merion-agents/sbom-summary/`](https://github.com/zyvorai/fabric/tree/main/examples/merion-agents/sbom-summary) |

## Pass bar

- Artifact `summary.md` in the session
- Cockpit `egress_connects == 0`
- On failure (CONNECT > 0): session `agent_paused_reason: ebpf_deny` + FluxVM freeze
- Evidence class remains `software-test`

## How it works

The upload is written to a fixed path inside the cell (`/home/agent/work/input.json`),
the guest extracts text with a fixed command, and the summary is built **on the host**
from that text. Format detected from `bomFormat`, `spdxVersion` or `runs`. Files over 300 KB are refused.

## Requirements

- FluxVM reachable; agent-runtime `/healthz`
- Template `node22-agent`: Nothing beyond `head` in the guest.
- Strict confinement applied (`deny_udp` + gateway-only broker/proxy ports) — see [confine.md](../confine.md)

## Honesty

The result is **extractive**: no model is called, and nothing found in the file is run,
opened or sent. Zero CONNECT is a **Keep audit** claim (plus FluxVM `drop_reasons` when
the dataplane is attached). PacketWolf is optional. Evidence class stays `software-test`;
this does not mean the operator cannot read the cell.
