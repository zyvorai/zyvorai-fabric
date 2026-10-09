# keepctl (Fabric)

Binary: [`scripts/keepctl`](../../../scripts/keepctl)

```bash
export KEEP_API=http://127.0.0.1:9096
export KEEP_TOKEN=…   # ZYVOR_AGENT_API_TOKEN

keepctl create -f deploy.json
keepctl policy show my-agent
# Keep mode: signature required
keepctl policy set my-agent docs/keep/sentinel/keep.policy.yaml keep.policy.yaml.sig
keepctl pack   /tmp/keep-pack my-agent
keepctl unpack /tmp/keep-pack my-agent
keepctl export-token 'trajectory:read:7d' 3600
keepctl cockpit <session-uuid>
```

Run a use case and look back at what it produced:

```bash
keepctl list                                      # use cases this runtime can run
keepctl run csv-clean ./orders.csv                # exits 2 if the cell made an outbound connection
keepctl artifacts --use-case csv-clean --since 2026-09-01T00:00:00Z
keepctl diff <older-artifact-id> <newer-artifact-id>   # what changed between two runs
keepctl audit <session-uuid> --limit 20           # journal rows; the hash-chain check goes to stderr
keepctl approvals pending                         # pending | approved | denied, or none for all
keepctl speculate <session-id> --paths /work -- <cmd>   # run in an isolated copy of the cell; changes wait behind a `changeset` approval
keepctl decide <approval-id> approved|denied             # a person's decision; approved applies the changeset on FluxVM
```

Batches and triggers ([TRIGGERS.md](../TRIGGERS.md)):

```bash
keepctl run csv-clean jan.csv feb.csv            # one cell per file
keepctl trigger add-webhook csv-clean            # prints a secret once
keepctl trigger fire <id> <secret> ./orders.csv  # a signed call, as a webhook sender would make
keepctl trigger add-folder log-triage inbox 30   # needs ZYVOR_AGENT_WATCH_ROOT on the runtime
keepctl trigger list
```

The console shows the same three views at `/app/keep/history` (Runs, Audit, Approvals).

Send the audit journal to a SIEM. `GET /v1/export/audit?format=ocsf` returns newline-delimited events shaped like
OCSF *API Activity* (class 6003), one per journal row. It needs an export token with the `audit` scope, the same as the
JSON export:

```bash
TOK=$(keepctl export-token audit:read:1h 600 | jq -r .token)
curl -sD- -H "Authorization: Bearer $KEEP_TOKEN" -H "X-Keep-Export-Token: $TOK" \
  "$KEEP_API/v1/export/audit?format=ocsf&limit=500" -o audit.ndjson
```

Each event carries its row's `hash` (as `metadata.uid`) and `prev_hash` (under `unmapped`), so the receiver can
re-check the order. The chain verdict is in the `X-Keep-Audit-Chain` response header (`ok` or `broken`). The mapping is
shape-only and has not been run through the OCSF schema validator; `audit.jsonl` and `keepctl audit` stay the
source of truth.

Lab live gate (stub + FluxVM proof):

```bash
./scripts/keep-e2e.sh
./scripts/keep-live-lab.sh
```

Lives in **Fabric**, not FluxVM, not a third repo. FluxVM remains the VMM
(`security_profile`, Firecracker/QEMU cell). See [PRODUCTION.md](../PRODUCTION.md).
