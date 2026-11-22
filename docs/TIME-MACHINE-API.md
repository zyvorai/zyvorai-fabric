# Infrastructure Time Machine — Fabric API foundation

Machina's Time Machine consumes these **Zyvor Fabric** (`zyvor-fabricd`) APIs. Fabric remains the source of truth; Machina records and correlates exports.

## Config snapshot

`GET /api/config/snapshot` (auth: read)

Returns a versioned JSON bundle:

- `vms` — VM inventory from state store
- `network_policies` — declared policies
- `storage_pools` — pool list from storage manager
- `recent_events_count` — events currently retained on disk
- `exported_at` — RFC3339 timestamp

Use for periodic dumps before incidents. Fabric itself can now store and diff snapshots, see below.

## Snapshot history and diff

Fabric can keep a short history of config snapshots and diff any two of them (or one against the live configuration). This only stores a record; it never changes VMs, networks or storage.

| Method | Path | Auth | Description |
|--------|------|------|-------------|
| POST | `/api/config/snapshots` | write | Store the current snapshot. Returns `201` with metadata. Audit-logged as `config.snapshot.create`. |
| GET | `/api/config/snapshots` | read | Metadata for stored snapshots, newest first: `{ "snapshots": [...], "max": 50 }` |
| GET | `/api/config/diff?from=<id\|current>&to=<id\|current>` | read | Structural diff. `current` means the live configuration. |

Details:

- Snapshots are stored in the state store (`config_snapshots/`) with an id like `snap-20260101T120000123-ab12cd34`, `created_at`, and `created_by`. At most **50** are kept; storing another drops the oldest.
- List entries are metadata only: `id`, `created_at`, `created_by` and the number of `vms`, `network_policies`, `storage_pools` in it.
- Diff errors: `400` if `from` or `to` is missing or an invalid id, `404` if a snapshot id does not exist.
- The diff compares `vms` (keyed by VM `name`, falling back to `id`), `network_policies` (keyed by `id`, falling back to `name`) and `storage_pools` (keyed by `name`, falling back to `id`). `exported_at`, `version` and `recent_events_count` are not compared.
- Response: `{ "from", "to", "change_count", "changes": [...] }`. Each change has `section`, `key`, `kind` (`added`, `removed` or `changed`) and, for `changed`, a dotted `field` path with `old` and `new` values. For `added` / `removed`, `new` / `old` holds the whole entity. Nested objects are compared field by field; arrays are compared as a whole.
- Changes are sorted by section (vms, network_policies, storage_pools), then key, then field, so the same two snapshots always give the same output.
- Redaction: any key containing `password`, `secret`, `token`, `key` or `credential` (case-insensitive) is shown as `***REDACTED***` in diff output. A change to a secret is still reported, as redacted to redacted. Stored snapshots keep the raw values (as `GET /api/config/snapshot` returns them), so treat the state store as sensitive.
- The web UI exposes this as the "Snapshots & diff" tab of the Timeline page.

## Event retention

| Method | Path | Description |
|--------|------|-------------|
| GET | `/api/events/retention` | Current `max_events` (default 1000, clamp 100–100000) |
| PUT | `/api/events/retention` | Update retention (auth: write) |

Events are pruned in the background when new events are recorded. SSE stream (`/api/events/stream`) is unaffected.

## Related endpoints

- `GET /api/events` — recent lifecycle events (newest first)
- `GET /api/audit/logs` — operator audit trail
- `GET /api/network/topology` — topology for RCA correlation

## Machina integration (planned)

1. Poll `/api/config/snapshot` on an interval per cluster (Fabric's own history is the 50 most recent snapshots; there is no scheduled capture yet)
2. Persist snapshots locally with cluster ID + timestamp
3. On incident, copilot correlates snapshot diffs (`/api/config/diff`) with `/api/events` and audit logs

See [integrations/machina.md](integrations/machina.md) and [POSITIONING.md](POSITIONING.md).
