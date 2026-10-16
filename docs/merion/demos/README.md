# Keep one-click demos

Drop an untrusted file into a sealed cell, get an artifact back. Every demo runs with
**no browser** and expects **0 CONNECT**. If the audit journal ever shows an outbound
connection, the run fails closed: the session is frozen with `agent_paused_reason: ebpf_deny`
and the API returns `409`.

| Demo | Input | Artifact | Page |
|---|---|---|---|
| `pdf-brief` | PDF | `brief.md` | [pdf-brief.md](pdf-brief.md) |
| `contract-clauses` | contract PDF | `clauses.md` | [contract-clauses.md](contract-clauses.md) |
| `security-questionnaire` | questionnaire PDF | `answers.md` | [security-questionnaire.md](security-questionnaire.md) |
| `meeting-actions` | `.txt` / `.vtt` transcript | `actions.md` | [meeting-actions.md](meeting-actions.md) |
| `log-triage` | `.log` / `.txt` | `triage.md` | [log-triage.md](log-triage.md) |
| `sbom-summary` | CycloneDX / SPDX / SARIF JSON | `summary.md` | [sbom-summary.md](sbom-summary.md) |
| `csv-clean` | `.csv` | `clean.csv` + `report.md` | [csv-clean.md](csv-clean.md) |

## One API for all of them

```bash
./scripts/merion-demo.sh list                 # what this runtime offers
./scripts/merion-demo.sh csv-clean            # built-in sample
./scripts/merion-demo.sh log-triage app.log   # your own file
```

| Surface | Path |
|---|---|
| Console | `/app/merion` (pick a use case) |
| Runtime API | `GET /v1/demos`, `POST /v1/demos/{id}` (multipart field `file`) |
| fabricd proxy | `GET /api/demos`, `POST /api/demos/{id}` (JWT) |
| Registry | `agent-runtime/src/demos.rs` (`DEMOS`) |
| Builders | `agent-runtime/src/demo_builders.rs` |

## Testing

```bash
bash agent-runtime/tests/demos-ci.sh   # real runtime + FluxVM stand-in; needs node 20, python3, curl (pdftotext for the PDF demos)
```

See [STATUS.md](../STATUS.md#what-the-demos-e2e-proves-and-what-it-does-not) for what this does and does not prove.

## What "extractive" means

The guest runs one fixed command to pull text out of the upload (`pdftotext`, or `head`
for text formats). The summary is then built **on the host** with plain string handling.
No model is called, nothing found in the file is executed, opened or sent, and every
untrusted line is defanged before it lands in markdown. A model-written summary would
need a model socket, which these demos deliberately do not use.

## Adding a use case

Most use cases need **no Rust**. Describe one as a `pack.json` (an extractor plus a few summary
rules), or fill in the form at `/app/merion` → **Deploy your own use case**, then:

```bash
./scripts/merionctl deploy ./my-usecase --test
```

See [Tutorial 19](../../tutorials/19-build-your-own-use-case.md) and the field reference in
[PACKS.md](../PACKS.md). A use case that needs code is a TypeScript `agent` pack that runs inside
the cell.

To add a **built-in** (shipped with the runtime) instead: add a builder to `demo_builders.rs` with
a unit test, a `DemoSpec` to `DEMOS` in `demos.rs`, then `examples/merion-agents/<id>/` (with
`"kind": "builtin"`) and `docs/merion/demos/<id>.md`. The console picker, the fabricd proxy and
`merion-demo.sh` pick it up without further changes.

Honesty: evidence class stays `software-test`; zero CONNECT is a Keep audit claim, not
"the operator cannot read the cell".
