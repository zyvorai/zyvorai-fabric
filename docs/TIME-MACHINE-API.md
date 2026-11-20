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

Use for periodic dumps before incidents and diffing configuration over time.

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

1. Poll `/api/config/snapshot` on an interval per cluster
2. Persist snapshots locally with cluster ID + timestamp
3. On incident, copilot correlates snapshot diffs with `/api/events` and audit logs

See [integrations/machina.md](integrations/machina.md) and [POSITIONING.md](POSITIONING.md).
