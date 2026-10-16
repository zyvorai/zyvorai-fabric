# Keep — phases complete (local)

Scoreboard (ahead-of-Muse gate, software-test only): [`AHEAD.md`](AHEAD.md) · `./scripts/merion-scoreboard.sh` · `agent-runtime/src/scoreboard.rs`.

| Phase | Status | Where |
|---|---|---|
| FluxVM Phase 6 | Merged (`security_profile` / measured) | [zyvorai/fluxvm](https://github.com/zyvorai/fluxvm) |
| **Keep 0.1 pilot** | Live gate twice (happy + deny) on FluxVM host | [`pilot-runs/`](pilot-runs/) · `./scripts/merion-pilot-gate.sh` |
| Keep 0.1 live proof | Keep mode + live e2e path + browser view + credential authority | this tree |
| Keep docs | In Fabric | `docs/merion/` |
| merionctl | Script | `scripts/merionctl` |
| Agent runtime | Policy YAML, goals/artifacts, cockpit, export-token | `agent-runtime/` |
| Packaged agents | `infra-ops`, `migration-op`, `deploy-op`, `browser-research`, `pdf-brief` + `_fabric` | [`examples/merion-agents/`](../../examples/merion-agents/) |
| Keep console view | Goal → task → evidence → approval → outcome + PDF demo home | `/app/keep` · `/app/keep/:sessionId` |
| Keep 0.2 | **Soft scaffolding complete** + brokered browser; **hardware still gated** | [`KEEP-0.2.md`](MERION-0.2.md) · [`browser/DRIVER.md`](browser/DRIVER.md) |
| Keep Browser 0.3 | Split-sight, trajectory-as-code, origin IFC, SNI-identity badge, goal tabs | [`browser/BROWSER-0.3.md`](browser/BROWSER-0.3.md) |
| Host eBPF (FluxVM only) | `deny_udp` + gateway pin; audit `egress_connects`; freeze on deny | FluxVM TC + agent-runtime confine |
| One-click use cases | Table-driven demos (7): drop a file, get an artifact; expect 0 CONNECT | [`demos/`](demos/README.md) · `examples/merion-agents/<id>/` · `merion-demo.sh` |
| Your own use cases | Declarative `pack.json` (no code) deployed from the console or `merionctl deploy`; TypeScript agent packs signed with Node and deployed in one command | [`PACKS.md`](PACKS.md) · [Tutorial 19](../tutorials/19-build-your-own-use-case.md) · [Tutorial 20: mail export digest](../tutorials/20-sort-a-mail-export.md) |
| Run history | Artifact TTL, per-use-case history, line diff between runs, `run.finished` / `run.failed` webhook, console `/app/keep/history`, `merionctl run\|list\|artifacts\|diff\|audit\|approvals` | [`merionctl/README.md`](merionctl/README.md) |
| Triggers and batch | Multi-file runs (one cell each), signed webhook and watched-folder triggers, `merionctl trigger` | [`TRIGGERS.md`](TRIGGERS.md) |
| More file types | `docx`, `xlsx`, `pptx`, `html`, `eml`/`mbox` extractors, zip fan-out, `regex_extract` / `json_path` / `table` rules | [`PACKS.md`](PACKS.md) |
| Model-assisted use cases | Opt-in host-side model step: vault-gated endpoint, first-use approval, audited calls, sanitised reply, `merionctl grants` | [`MODEL.md`](MODEL.md) |
| Scenarios and the cell template | Seven scenario packs; `node22-agent` template and `merion-bake-node22-agent.sh` | [`SCENARIOS.md`](SCENARIOS.md) |
| Many users | User tokens, per-user isolation, quotas, usage, revocation | [`TENANCY.md`](TENANCY.md) |
| Phone-signed approvals | Device enrolment, push relays, signed decisions, `keep-phone`, test vectors | [`mobile/README.md`](mobile/README.md) |
| Model choice | `model_socket` wired for agents (`ctx.model.chat()`, CLI harness), any OpenAI-compatible endpoint | [`MODELS.md`](MODELS.md) |
| Phone vendors | Blueprint, reference gateway, benchmark, partial zh-CN console | [`VENDORS.md`](VENDORS.md) |
| Install Keep | `./scripts/deploy keep user@host` (needs FluxVM on the host); `merionctl doctor` | [`PRODUCTION.md`](PRODUCTION.md) · `scripts/deploy-merion.sh` |
| Install on Kubernetes | `charts/zyvor-merion`: a `hostNetwork` DaemonSet beside FluxVM, Keep mode on by default, credential descriptors, secret volumes, a service account. Rendered and schema-checked; **never installed in a cluster** | [`PRODUCTION.md`](PRODUCTION.md#install-keep-on-kubernetes) |
| Policy changes | A risk check (409 until acknowledged) on every policy update; `merionctl policy suggest` drafts allow rules from an agent's denied requests | [`sentinel/README.md`](sentinel/README.md) |
| Egress rules by body and program | MCP, JSON-RPC and GraphQL body rules; `binaries` (which program in the cell may call, from the guest's `/proc`). The attribution script was run on real Linux, including across a `bwrap` PID namespace; **not** in a real cell | [`sentinel/README.md`](sentinel/README.md) |
| Operator egress guard | `ZYVOR_AGENT_GUARD_URL`: an HTTP service that may refuse any brokered request; fails closed | `agent-runtime/README.md` |
| Cell hardening | A seccomp syscall filter in the inner container (x86_64). The filter was run under a real `bwrap` on Linux; **not** through a real cell or with Chromium | `agent-runtime/README.md` |
| GPU cells | `gpus` in the agent manifest; FluxVM picks free VFIO-bound GPUs under a lock. Picking logic tested; **no real GPU** | `agent-runtime/README.md` · FluxVM `docs/sandbox-gpus.md` |
| Audit export | `GET /v1/export/audit?format=ocsf`: OCSF-shaped NDJSON, chain hashes carried; not run through the OCSF schema validator | [`merionctl/README.md`](merionctl/README.md) |
| Credential sources | A credential's secret from a file, or from HashiCorp Vault (token, AppRole, Kubernetes login); `GET /v1/vault/status` lists each source. Tested against a mock; **no real Vault** | [`vault/README.md`](vault/README.md#credentials-from-a-file-source) |
| CI | Keep workflow: unit tests, **demos e2e** (7 built-ins, a custom use case, signed pack deploy in Keep mode, against the real runtime and the FluxVM stand-in), stub e2e | [`.github/workflows/merion.yml`](../../.github/workflows/merion.yml) |
| Tutorial | Hands-on + pack appendix + demos + your own | [Tutorial 16](../tutorials/16-merion-workstation.md) · [Tutorial 17](../tutorials/17-merion-pdf-brief.md) · [Tutorial 18](../tutorials/18-merion-use-cases.md) · [Tutorial 19](../tutorials/19-build-your-own-use-case.md) |

## Packaged agents

| Pack | Fabric surface |
|---|---|
| infra-ops | alerts, VMs, lifecycle; restart/remediation behind ask |
| migration-op | `/api/migrations` + GuestKit inspect/rescue (no Transiva in-repo) |
| deploy-op | `/readyz` + `/health` → readiness artifact |
| browser-research | allowlisted a11y browse → research markdown |
| pdf-brief | PDF → `brief.md`; no browser; 0 CONNECT |
| contract-clauses | contract PDF → `clauses.md`; no browser; 0 CONNECT |
| security-questionnaire | questionnaire PDF → `answers.md`; no browser; 0 CONNECT |
| meeting-actions | `.txt`/`.vtt` → `actions.md`; no browser; 0 CONNECT |
| log-triage | log file → `triage.md`; no browser; 0 CONNECT |
| sbom-summary | CycloneDX/SPDX/SARIF → `summary.md`; no browser; 0 CONNECT |
| csv-clean | CSV → `clean.csv` + `report.md`; no browser; 0 CONNECT |

## How to test

```bash
cargo test --manifest-path agent-runtime/Cargo.toml --lib
cargo test --manifest-path agent-runtime/Cargo.toml goals -- --nocapture
./scripts/merion-e2e.sh
# One-click demos, custom use case and signed pack deploy (real runtime + FluxVM stand-in, no VM):
bash agent-runtime/tests/demos-ci.sh
# Lab FluxVM host (template required — soft-pass removed):
MERION_E2E_TEMPLATE=node22-agent ./scripts/merion-live-lab.sh
# Full pilot (happy + deny, archived logs):
./scripts/merion-pilot-gate.sh
./scripts/merion-pack-demo.sh infra-ops
./scripts/merion-demo-pdf.sh
```

## Honesty

Measured = `software-test`. Host can still see the VM until Keep 0.2 + hardware.
Guest vsock is healthy on QEMU `node22-agent` (musl-static guest-agent) and on
Firecracker `node22-fc` (flat ext4 rootfs — see `scripts/merion-bake-fc-rootfs.sh`).
The pilot gate prefers `node22-fc` when that template is registered.

### Not verified live (2026-09-30)

The controls listed above from "Policy changes" down were built and tested against fakes and, where noted, on real Linux outside a cell. These have
**not** been run: the policy 409 and acknowledge flow, the OCSF export and `policy suggest` against a live runtime; per-program `binaries` and the
seccomp filter in a real cell (and Chromium under the filter); a GPU cell on real hardware; the credential sources against a real Vault, AppRole or
Kubernetes login; the Helm chart in a cluster. Several need the runtime's API token; see [TODO.md](TODO.md).

## What the demos e2e proves, and what it does not

`agent-runtime/tests/demos-ci.sh` runs the real runtime binary against `tests/sandbox_stub.py`
(guest commands run on the CI machine, real `pdftotext`, no VM). It checks the seven built-in demos and
their artifacts, refusal of wrong file types, oversize uploads and unknown ids, spreadsheet-formula
neutralisation, deploying and deleting a user-defined use case, `merionctl doctor`, and, in Keep mode,
that an unsigned deploy is refused while a Node-signed one is accepted and a one-byte change to the
signed bytes is refused. It does **not** prove cell isolation or that the host eBPF pin stops a
connection: that needs a FluxVM host (`merion-live-lab.sh`). The freeze-on-connect rule is unit-tested
by counting the `egress.connect` / `ebpf.*` audit rows it depends on.
