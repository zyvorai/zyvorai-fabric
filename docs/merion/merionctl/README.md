# merionctl (Fabric)

Binary: [`scripts/merionctl`](../../../scripts/merionctl)

```bash
export MERION_API=http://127.0.0.1:9096
export MERION_TOKEN=…   # ZYVOR_AGENT_API_TOKEN

merionctl create -f deploy.json
merionctl policy show my-agent
# Merion mode: signature required
merionctl policy set my-agent docs/merion/sentinel/merion.policy.yaml merion.policy.yaml.sig
merionctl pack   /tmp/keep-pack my-agent
merionctl unpack /tmp/keep-pack my-agent
merionctl export-token 'trajectory:read:7d' 3600
merionctl cockpit <session-uuid>
```

Run a use case and look back at what it produced:

```bash
merionctl list                                      # use cases this runtime can run
merionctl run csv-clean ./orders.csv                # exits 2 if the cell made an outbound connection
merionctl artifacts --use-case csv-clean --since 2026-09-01T00:00:00Z
merionctl diff <older-artifact-id> <newer-artifact-id>   # what changed between two runs
merionctl audit <session-uuid> --limit 20           # journal rows; the hash-chain check goes to stderr
merionctl approvals pending                         # pending | approved | denied, or none for all
merionctl speculate <session-id> --paths /work -- <cmd>   # run in an isolated copy of the cell; changes wait behind a `changeset` approval
merionctl decide <approval-id> approved|denied             # a person's decision; approved applies the changeset on FluxVM
```

Batches and triggers ([TRIGGERS.md](../TRIGGERS.md)):

```bash
merionctl run csv-clean jan.csv feb.csv            # one cell per file
merionctl trigger add-webhook csv-clean            # prints a secret once
merionctl trigger fire <id> <secret> ./orders.csv  # a signed call, as a webhook sender would make
merionctl trigger add-folder log-triage inbox 30   # needs ZYVOR_AGENT_WATCH_ROOT on the runtime
merionctl trigger list
```

The console shows the same three views at `/app/merion/history` (Runs, Audit, Approvals).

Send the audit journal to a SIEM. `GET /v1/export/audit?format=ocsf` returns newline-delimited events shaped like
OCSF *API Activity* (class 6003), one per journal row. It needs an export token with the `audit` scope, the same as the
JSON export:

```bash
TOK=$(merionctl export-token audit:read:1h 600 | jq -r .token)
curl -sD- -H "Authorization: Bearer $MERION_TOKEN" -H "X-Merion-Export-Token: $TOK" \
  "$MERION_API/v1/export/audit?format=ocsf&limit=500" -o audit.ndjson
```

Each event carries its row's `hash` (as `metadata.uid`) and `prev_hash` (under `unmapped`), so the receiver can
re-check the order. The chain verdict is in the `X-Merion-Audit-Chain` response header (`ok` or `broken`). The mapping is
shape-only and has not been run through the OCSF schema validator; `audit.jsonl` and `merionctl audit` stay the
source of truth.

Lab live gate (stub + FluxVM proof):

```bash
./scripts/merion-e2e.sh
./scripts/merion-live-lab.sh
```

Lives in **Fabric**, not FluxVM, not a third repo. FluxVM remains the VMM
(`security_profile`, Firecracker/QEMU cell). See [PRODUCTION.md](../PRODUCTION.md).
