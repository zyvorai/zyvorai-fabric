---
name: keep-cli
description: Drive a Keep runtime with merionctl. Use when asked to run a Keep use case, deploy a pack, list runs or artifacts, read the audit journal, or check pending approvals.
---

# merionctl

`merionctl` is `scripts/merionctl`. Reference: `docs/merion/merionctl/README.md`.

## Setup

```bash
export MERION_API=http://127.0.0.1:9096
export MERION_TOKEN=...   # the runtime's ZYVOR_AGENT_API_TOKEN, never print it
```

## Common tasks

| Task | Command |
|---|---|
| What can this runtime run? | `merionctl list` |
| Run a use case on a file | `merionctl run csv-clean ./orders.csv` (exit code 2 means the cell made an outbound connection) |
| Many files | `merionctl run csv-clean jan.csv feb.csv` (one cell per file) |
| Past outputs | `merionctl artifacts --use-case csv-clean --since 2026-09-01T00:00:00Z` |
| What changed between runs | `merionctl diff <older-id> <newer-id>` |
| Journal for a session | `merionctl audit <session-uuid> --limit 20` (the hash-chain check prints to stderr) |
| Pending approvals | `merionctl approvals pending` |
| Deploy a pack | `merionctl deploy <dir>`; add `--test` to run a `builtin` pack |
| Triggers | `merionctl trigger add-webhook|add-folder|list|fire` (see `docs/merion/TRIGGERS.md`) |

## Rules

- Read `docs/merion/PACKS.md` before writing a `pack.json`. The `extract` field is a fixed list, never a command.
- Do not print, log or commit `MERION_TOKEN`.
- Policy changes go through the `keep-policy` skill, not ad-hoc PUTs.
