# Audit export, integrity and retention

Audit events are stored one per file in the daemon's state store. This page covers getting them out, proving they were not altered, and pruning old ones.

## Export

`GET /api/audit/export?from&to&actor&action&format=ndjson|csv`

| Parameter | Meaning |
|-----------|---------|
| `from` | Earliest time, inclusive. RFC 3339 or `YYYY-MM-DD`. |
| `to` | Latest time, exclusive. A bare `YYYY-MM-DD` covers that whole day. |
| `actor` | Only events by this user. Exact match, ignoring case. |
| `action` | Only this action. Exact match, ignoring case. |
| `format` | `ndjson` (default) or `csv`. |

Events are streamed oldest first. NDJSON is one full event per line, including the chain fields, so an export can be re-checked offline. CSV has a header row; any cell starting with `=`, `+`, `-`, `@`, a tab or a carriage return is prefixed with `'` so a spreadsheet does not run it as a formula, and cells with commas, quotes or line breaks are quoted.

An export that matches more than `[audit] max_export_events` (default 100000) events is refused with `413`; narrow the time range. Every export is recorded as an `AUDIT_EXPORT` event (the export does not contain its own event).

Who may call it: `admin` and the read-only `viewer` role (the auditor). The write-capable `user` role gets `403`. A token with a `tenant` claim only sees events tagged with its tenant or performed by itself; events written by the daemon carry no tenant tag, so they are visible to non-tenant tokens only.

```
fabricctl audit export --from 2026-03-01 --to 2026-04-01 --actor alice --format csv --file march.csv
```

## Tamper evidence (hash chain)

Off by default. Turn it on with:

```toml
[audit]
chain = true
```

Each new event then carries `seq` (1, 2, 3, ...), `prev_hash` (the previous event's `hash`; 64 zeros for the first) and `hash` (SHA-256 over the event's own fields including `seq` and `prev_hash`). The newest `seq` and `hash` are also kept in a small head record. Once a chain has started it continues even if `chain` is later set to `false`, so there is no unchained hole in the middle. Events written before the chain started stay unchained and are reported as such, not as a failure.

`GET /api/audit/verify` (admin or read-only auditor, not tenant-scoped tokens) walks the chain and returns `status` (`ok`, `broken`, or `not_chained`), counts, the chain `head` and retention `anchor`, and `first_break` with a `reason`, the `seq` and event `id`:

| `reason` | What happened |
|----------|---------------|
| `hash_mismatch` | An event was edited after it was written. |
| `prev_hash_mismatch` | Events were reordered or replaced. |
| `sequence_gap` | One or more events were removed. |
| `duplicate_seq` | Two events share a sequence number. |
| `truncated` | Newer events were removed (the head is ahead of the newest event). |
| `head_missing` / `head_mismatch` | The head record was removed or does not match. |
| `anchor_mismatch` | An event does not match the retention anchor. |
| `unchained_inside_chain` | An event without a hash is dated inside the chain (its chain fields were stripped, or it was inserted). |
| `unreadable_record` | A stored event file cannot be read. |

```
fabricctl audit verify        # prints the first broken link and exits 2 if the chain is broken
```

Limits: the hashes are unkeyed. This detects edits, deletions and reordering by anyone who cannot also rewrite the whole store; someone who can rewrite every event and the head can rebuild a consistent chain. Copy exports off the host regularly (and keep the head `hash` somewhere else) to cover that case. Events dated inside the chain but written without a hash are judged by timestamp, so a large clock step can produce a false `unchained_inside_chain`.

## Retention

`POST /api/audit/retention?dry_run=true|false&max_age_days=N&max_events=N&force=true` (admin, not tenant-scoped)

Limits default to the config; query values override them:

```toml
[audit]
retention_max_age_days = 365
retention_max_events = 1000000
max_export_events = 100000
```

- Nothing prunes on its own. Run it from cron: `fabricctl audit retention --max-age-days 365`.
- `dry_run` defaults to `true` over HTTP. A dry run reports what would go (counts, date range, new anchor) and changes nothing.
- A real run first writes an `AUDIT_RETENTION` event (counts, range, limits, who ran it). If that write fails nothing is deleted.
- It prunes a prefix of the oldest events: older than the age limit, or beyond the count limit, whichever removes more. A future-dated event counts as age 0. It never prunes the newest event. Limits of 0 are rejected, and at least one limit is required.
- Chained events go oldest first; the last pruned one is saved as the anchor, so `verify` still checks the remaining chain and still catches later removals.
- A real run is refused with `409` (and the verify report) when the chain does not verify, so retention cannot be used to erase evidence of tampering. `force=true` overrides that, and is recorded in the event.

```
fabricctl audit retention --dry-run --max-events 50000
fabricctl audit retention --max-events 50000
```

## Older events

Events written by older versions with the `user_id` / `resource` shape are now read and listed with the normal fields (`user`, `resource_name`); before, they were silently skipped by `GET /api/audit/logs`.
