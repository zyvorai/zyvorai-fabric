---
sidebar_position: 9
---

# Recipes: calling Merion from the tools you already use

Merion already has the ways in: the HTTP API with user tokens, signed webhook and folder triggers, and the `POST /mcp`
endpoint ([TENANCY.md](TENANCY.md), [TRIGGERS.md](TRIGGERS.md), the [runtime README](https://github.com/zyvorai/fabric/blob/main/agent-runtime/README.md)). There is
no Siri, Mac or Windows connector, and none is needed: a tool that can send an HTTP request with a file can run a use case.
These recipes show how, using the same two calls every time.

**Verified** below means the call was made against a real Merion host. **Unverified** means the client side (Siri, Shortcuts,
launchd, Task Scheduler) was not run here; the steps follow those tools' documented behaviour, so try them on a test file first.

## The two calls

```bash
# 1. run a use case on a file (returns JSON with the artifact id and egress_connects)
curl -s -X POST -H "Authorization: Bearer $MERION_TOKEN" -F file=@report.pdf "$MERION_API/v1/demos/pdf-brief"

# 2. read the summary
curl -s -H "Authorization: Bearer $MERION_TOKEN" "$MERION_API/v1/artifacts/<artifacts[0].id>"   # the "body" field is Markdown
```

*Verified* (operator token; a user token reaching only its own runs is verified in the live tenancy script). Use a **scoped user
token**, minted per person by your gateway or operator (`POST /v1/user-tokens`, scopes `run` and `read`), not the operator token.
Use any pack name in place of `pdf-brief`: the [scenario packs](SCENARIOS.md) and your own.

## Watch a page for changes (verified: the script; unverified: cron and launchd)

`scripts/merion-watch.sh` notices when a public page changes. **The script fetches the page on your machine's network** (http or https only, 2 MiB and 30 s at most, no
login, no JavaScript); the sealed cell never touches the network. The saved page is read by a use case (by default `status-page-watch`), and a change is the runtime's
own diff between this summary and the last one, so you see the lines that changed, once.

```bash
export MERION_API=http://127.0.0.1:9096 MERION_TOKEN=...      # a user token
./scripts/merion-watch.sh https://status.example.com                          # first run: BASELINE; later: UNCHANGED, or CHANGED with the lines (exit 3)
./scripts/merion-watch.sh https://shop.example.com/item --match "in stock"    # alert ONLY when "in stock" newly appears in the summary
./scripts/merion-watch.sh https://status.example.com --notify                 # also a desktop notification (osascript / notify-send)
```

Exit codes: `0` unchanged (or the first run), `3` changed (or matched), `2` the fetch or run failed, `4` failed `--max-failures` times in a row (default 3). Run it every few
minutes from cron (`*/10 * * * * MERION_API=... MERION_TOKEN=... /path/to/merion-watch.sh URL --notify`) or a launchd `StartInterval` job; a non-zero exit is your alert. The last
result per URL is kept in `~/.local/state/merion-watch/`. Use a use case that reads what you care about (your own pack works if it accepts `.html`); a page is judged only as
well as the pack's keywords match its words. Do not put a password or token in the URL: the script refuses `user:pass@`.

Verified by `agent-runtime/tests/demos-ci.sh` against a local page and a real runtime: baseline, unchanged, a change reported once, `--match` alerting only on new text,
repeated failures escalating to exit 4, and bad URLs refused. Not verified: cron and launchd themselves, and desktop notifications.

## Siri, through Apple Shortcuts (unverified)

Siri runs a Shortcut by its name, and a Shortcut can send the two calls above. In the Shortcuts app:

1. New Shortcut, named for what you will say, for example **Summarise with Merion**. Set it to accept **Files** from the Share Sheet.
2. Add **Get Contents of URL**: URL `https://YOUR-GATEWAY/v1/demos/pdf-brief`, Method **POST**, Headers `Authorization: Bearer <your user token>`,
   Request Body **Form**, a **File** field named `file` set to *Shortcut Input*.
3. Add **Get Dictionary Value** for `artifacts`, take its first item, get `id`.
4. Add **Get Contents of URL** again: `https://YOUR-GATEWAY/v1/artifacts/<that id>`, header as above, then **Get Dictionary Value** `body`.
5. Add **Show Result** (or **Speak Text**).

Then "Hey Siri, Summarise with Merion" runs it, and it works from the Share Sheet on a file. Siri itself only starts the Shortcut; it
does not read the file. Keep the token in the Shortcut's own storage and do not share the Shortcut.

## macOS: a folder that runs a use case (unverified)

Save as `~/bin/keep-drop.sh`, `chmod +x` it. The token comes from the Keychain, not from the script.

```bash
#!/bin/bash
# usage: keep-drop.sh <use-case> <file>
TOKEN=$(security find-generic-password -s keep-token -w)
RESP=$(curl -s -X POST -H "Authorization: Bearer $TOKEN" -F file=@"$2" "$MERION_API/v1/demos/$1")
ID=$(python3 -c 'import json,sys; print(json.loads(sys.argv[1])["artifacts"][0]["id"])' "$RESP")
curl -s -H "Authorization: Bearer $TOKEN" "$MERION_API/v1/artifacts/$ID" > "${2%.*}.keep.json"
```

Store the token once with `security add-generic-password -s keep-token -a "$USER" -w`. To run it whenever a file lands in a folder,
use an Automator **Folder Action**, or a launchd job with a `WatchPaths` entry pointing at the folder that runs the script. Or skip the
script and use a **folder trigger** on the Merion side ([TRIGGERS.md](TRIGGERS.md)) if the folder is on the Merion host.

## Windows: PowerShell and Task Scheduler (unverified)

With PowerShell 7 (`-Form` needs it), install the `Microsoft.PowerShell.SecretManagement` module with a vault, store the token (`Set-Secret -Name MerionToken`), then:

```powershell
$token = Get-Secret -Name MerionToken -AsPlainText
$r = Invoke-RestMethod -Method Post -Uri "$env:MERION_API/v1/demos/pdf-brief" `
      -Headers @{ Authorization = "Bearer $token" } -Form @{ file = Get-Item .\report.pdf }
(Invoke-RestMethod -Uri "$env:MERION_API/v1/artifacts/$($r.artifacts[0].id)" -Headers @{ Authorization = "Bearer $token" }).body
```

Run it on a schedule with **Task Scheduler** (an action that starts `pwsh -File keep-drop.ps1`). Windows PowerShell 5.1 has no `-Form`; use
`curl.exe -F file=@report.pdf` (shipped with Windows 10 and later) with the same header.

## Webhooks, and other agents

- **A system that can POST:** a signed **webhook trigger** runs a use case when it fires (`merionctl trigger add-webhook <pack>`, then
  `merionctl trigger fire`). *Verified* in the live scenarios script.
- **Another agent (Claude, Codex, a script):** `POST /mcp` speaks MCP over JSON-RPC (`tools/list`, `tools/call`) on the same bearer
  token, and the plain HTTP calls above work from any agent that can run `curl`. No change is needed on the Merion side.

## A pack that watches for you, on its own schedule

The two-call flow above is for a `kind: usecase` pack you run on demand. A `kind: agent` pack with `"suggestions": true` (for example
`examples/merion-agents/calendar-suggestions` or `examples/merion-agents/price-watch`) instead runs on a cron you set with
`POST /v1/schedules` and proposes a suggestion when it finds something, rather than returning an artifact from a single call. And
whatever an agent shows you as a card (`docs/merion/card/README.md`) — a small read-only digest, never an approval — is retrievable the
same way as any other artifact, `GET /v1/artifacts/{id}`.

## What these recipes do not do

They send a file you choose and bring back a summary. They do not give Siri, Shortcuts or a Windows task any access to your files
beyond the one you pass, and they do not send mail, post to a ledger or change anything on your machine. Keep the token out of
scripts and chat, prefer a short-lived scoped user token, and remember the evidence class is `software-test`: whoever operates the
Merion host could read a cell's memory ([VENDORS.md](VENDORS.md)).
