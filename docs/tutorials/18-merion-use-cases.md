# Tutorial 18: Keep use cases — drop a file, get an artifact

Tutorial 17 briefs one PDF. This one runs the whole family of one-click demos the same
way: contract clauses, a security questionnaire, meeting actions, log triage, an SBOM
summary and a CSV cleanup. Each takes an untrusted file, works inside a sealed cell with
no browser, and must finish with **0 CONNECT**.

**Level:** Beginner  
**Time:** 20 minutes  
**Repos:** Fabric agent-runtime, FluxVM with template `node22-agent` (+ `pdftotext` for the
PDF demos), `curl`, optional Fabric console JWT.

> **Honesty.** The summaries are **extractive**: no model is called and nothing found in
> the file is run, opened or sent. Evidence class stays `software-test`. Zero CONNECT means
> the guest never brokered egress, not "the operator cannot read the VM".

Related: [Tutorial 17](17-merion-pdf-brief.md) · [demos index](../merion/demos/README.md) ·
[host confinement](../merion/confine.md)

---

## What you will learn

1. List the demos a runtime offers and run any of them from the script or `/app/keep`.
2. Read each artifact and see what was, and was not, extracted.
3. Confirm the fail-closed rule (`egress_connects > 0` freezes the session, `409`).

---

## Step 0: Preflight

```bash
cd fabric
export KEEP_API="${KEEP_API:-http://127.0.0.1:9096}"
export KEEP_TOKEN="${KEEP_TOKEN:-}"   # if runtime auth is on

./scripts/merion-demo.sh list
```

You should see seven demos. If the list is empty or the call fails, the runtime is older
than this tutorial; run Tutorial 17 first.

---

## Step 1: Run each demo on its sample

Every demo ships a small sample under `examples/merion-agents/<id>/`:

```bash
./scripts/merion-demo.sh contract-clauses
./scripts/merion-demo.sh security-questionnaire
./scripts/merion-demo.sh meeting-actions
./scripts/merion-demo.sh log-triage
./scripts/merion-demo.sh sbom-summary
./scripts/merion-demo.sh csv-clean
```

Each run prints the JSON result and ends with `OK — <artifacts> ready, 0 CONNECT`. The
script exits `2` if `egress_connects` is not `0`.

Console path: sign in → **Keep** → `/app/keep`, pick a use case, choose a file (or leave
it empty for the sample), and click **Run**.

---

## Step 2: Read the artifacts

Open the session cockpit (`/app/keep/<session>`) and read each artifact:

| Demo | What to look for |
|---|---|
| `contract-clauses` | A section per topic found, and a **Not found** list |
| `security-questionnaire` | A table of questions with the answer as written; `(no answer found)` where the next line is another question |
| `meeting-actions` | Owners from `Name:` prefixes; WebVTT timestamps are gone |
| `log-triage` | Level counts, errors grouped after numbers collapse (`timeout after #s`), the busiest minutes |
| `sbom-summary` | Component and license counts; severity table for CycloneDX vulnerabilities or SARIF levels |
| `csv-clean` | `clean.csv` with formula-like cells prefixed by `'`, and a `report.md` listing them |

---

## Step 3: Try a hostile file

The point of the cell is what happens when the input misbehaves. Try:

```bash
printf 'name,note\nAnn,=HYPERLINK("http://example.com","click")\n' > /tmp/evil.csv
./scripts/merion-demo.sh csv-clean /tmp/evil.csv
```

The formula cell comes back as `'=HYPERLINK(...)` in `clean.csv` and is listed in
`report.md`. It never ran, and the cockpit still shows **0 CONNECT**.

Wrong file type is refused before any cell is created:

```bash
./scripts/merion-demo.sh csv-clean README.md   # 400: the csv-clean demo accepts .csv files
```

---

## Step 4: Fail closed

If the audit journal shows any `egress.connect` or `ebpf.*` event during a run, the runtime
freezes the sandbox, sets `agent_paused_reason: ebpf_deny` and returns `409`. There is no
browser fallback. See [confine.md](../merion/confine.md).

---

## Where next

- Add your own use case: [demos/README.md](../merion/demos/README.md#adding-a-use-case).
- Use cases that need the network or a browser are packs with an allowlist instead:
  [Tutorial 16](16-merion-workstation.md).
