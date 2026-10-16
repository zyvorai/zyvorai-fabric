---
sidebar_position: 6
---

# Triggers and batch

A use case normally runs when you drop a file on the console. Two things start one without
you, and one lets you run many files at once. All of them go through the same path as an
upload, so the extension and size checks, the strict network policy and the
freeze-on-connect rule apply unchanged. Each file gets its own sealed cell.

## Batch

Send several `file` fields in one `POST /v1/demos/{id}`, or pass several files to
`merionctl run`. At most 20 files and 64 MiB in total.

```bash
merionctl run csv-clean jan.csv feb.csv mar.csv
```

The answer is one report with a `batch_id`, a count, and a row per file. If every file worked
the status is 201. If some failed it is **207** and the good files still ran (a wrong
file type is a 400 row, not a failed batch). If any file's cell makes an outbound
connection, that session is frozen (409) and the rest of the batch is skipped. Through
`fabricd` the whole request is capped at 32 MiB.

## Webhook trigger

```bash
merionctl trigger add-webhook csv-clean      # prints the secret once
merionctl trigger fire <trigger-id> <secret> ./orders.csv
```

`POST /v1/triggers/{id}/hook` takes the file as the request body and its name in
`x-zyvor-filename`. The call is signed: `x-zyvor-signature: sha256=<HMAC-SHA256 of the body>`,
keyed with the secret. The signature is the credential, so this route is outside the
bearer token. It runs synchronously and answers with the run's result. A bad signature is 401.
The secret is never listed again.

## Folder trigger

Set `ZYVOR_AGENT_WATCH_ROOT` on the runtime to a directory you control. Folder triggers are off
without it.

```bash
merionctl trigger add-folder log-triage inbox 30   # scan <root>/inbox every 30 s
```

- The folder is a single plain name under the root. Anything that resolves outside it is refused.
- Symlinks, dot files, other extensions, empty files and files over the use case's size cap are
  ignored (an oversize file is recorded as the trigger's last error).
- A file is picked up once it has been quiet for 2 seconds, and runs once. Handled files are
  remembered by name, size and modification time, so a changed file runs again.
- At most 5 files run per scan. The interval is 5 to 86400 seconds; the API also accepts a
  `cron` expression instead.
- Nothing is moved or deleted in your folder.

## Notifications

Triggered runs send `run.finished` / `run.failed` like any run, if the approval webhook is set.

`merionctl trigger list` shows each trigger's run count and last error. The console has the same list
under **Keep history → Triggers**.
